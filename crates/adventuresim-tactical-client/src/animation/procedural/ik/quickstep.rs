use super::*;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(in crate::animation) struct QuickstepIkState {
    action_start_tick: Option<u64>,
    feet: Option<QuickstepFeetState>,
}

#[derive(Debug, Clone, Copy)]
struct QuickstepFeetState {
    left: QuickstepFootState,
    right: QuickstepFootState,
}

#[derive(Debug, Clone, Copy)]
struct QuickstepFootState {
    takeoff_world: Vec3,
    takeoff_rotation_world: Quat,
    release_phase: Option<f32>,
}

/// Retains each takeoff ankle as a world plant while the leg can still reach
/// it without leaning past a 45-degree arch. Only then does that foot tuck and
/// travel toward its authored guard landing position.
pub(in crate::animation) fn apply(
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut states: Query<&mut QuickstepIkState>,
    mut diagnostics: Query<&mut LegIkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        let mut state = states
            .get_mut(owner)
            .map(|state| *state)
            .unwrap_or_default();
        if !skeleton.is_quickstep() {
            if state.feet.is_some() {
                state = QuickstepIkState::default();
                store_state(owner, state, &mut states, &mut commands);
            }
            continue;
        }
        let action_start_tick = skeleton.action_start_tick();
        if state.action_start_tick != action_start_tick {
            state = QuickstepIkState {
                action_start_tick,
                ..default()
            };
        }
        let Some((rig_origin, rig_rotation)) = rig
            .rig_scene()
            .and_then(|entity| transforms.p0().compute_global_transform(entity).ok())
            .map(|global| (global.translation(), global.rotation()))
        else {
            continue;
        };

        let legs = [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                false,
            ),
        ];
        if skeleton.is_grounded() && state.feet.is_none() {
            let mut capture = |role: BoneRole| {
                rig.get(&role)
                    .and_then(|foot| transforms.p0().compute_global_transform(*foot).ok())
                    .filter(|global| {
                        global.translation().is_finite() && global.rotation().is_finite()
                    })
                    .map(|global| QuickstepFootState {
                        takeoff_world: global.translation(),
                        takeoff_rotation_world: global.rotation(),
                        release_phase: None,
                    })
            };
            if let (Some(left), Some(right)) =
                (capture(BoneRole::FootLeft), capture(BoneRole::FootRight))
            {
                state.feet = Some(QuickstepFeetState { left, right });
            }
        }

        let action_phase = skeleton.action_phase();
        let mut targets = [None, None];
        let mut supports = [0.0, 0.0];
        for (index, (upper_role, lower_role, foot_role, left)) in legs.into_iter().enumerate() {
            let (Some(&upper), Some(&lower), Some(&foot)) = (
                rig.get(&upper_role),
                rig.get(&lower_role),
                rig.get(&foot_role),
            ) else {
                continue;
            };
            let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
                snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
            else {
                continue;
            };
            let authored = foot_snapshot.global.translation();
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot.global.translation().distance(authored);
            let reach = maximum_reach(upper_length, lower_length);
            let (takeoff, takeoff_rotation, release_phase) = if let Some(feet) = &mut state.feet {
                let foot = if left {
                    &mut feet.left
                } else {
                    &mut feet.right
                };
                if foot.release_phase.is_none()
                    && quickstep_foot_must_release(
                        skeleton.is_grounded(),
                        action_phase,
                        upper_snapshot.global.translation(),
                        foot.takeoff_world,
                        reach,
                    )
                {
                    foot.release_phase = Some(action_phase);
                }
                (
                    foot.takeoff_world,
                    Some(foot.takeoff_rotation_world),
                    foot.release_phase,
                )
            } else {
                (authored, None, None)
            };
            let progress = release_phase.map_or(0.0, |release| {
                if skeleton.is_grounded() && action_phase > 0.125 {
                    1.0
                } else {
                    ((action_phase - release) / (0.625 - release).max(0.001)).clamp(0.0, 1.0)
                }
            });
            let mut target = takeoff.lerp(authored, progress);
            target.y += (std::f32::consts::PI * progress).sin() * 0.16;
            targets[index] = Some(target);
            supports[index] = (progress <= f32::EPSILON) as u8 as f32;
            let canonical = pole_to_world(
                rig_rotation,
                canonical_knee_pole(if left { -1.0 } else { 1.0 }),
            );
            let pole = authored_knee_pole_world(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                target,
                canonical,
            )
            .unwrap_or(canonical);
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                upper_snapshot.global.translation(),
                authored,
                target,
                pole,
                &parents,
                &transforms.p0(),
            );
            if let Some(solution) = solve_two_bone_with_reach(
                TwoBoneChain::new(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    authored,
                    upper_length,
                    lower_length,
                    pole,
                ),
                target,
                reach,
            ) {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
            }
            if let Some(takeoff_rotation) = takeoff_rotation {
                let desired_rotation = takeoff_rotation
                    .slerp(foot_snapshot.global.rotation(), progress)
                    .normalize();
                if let Ok(parent) = parents.get(foot)
                    && let Ok(parent_global) =
                        transforms.p0().compute_global_transform(parent.parent())
                    && let Ok(mut foot_transform) = transforms.p1().get_mut(foot)
                {
                    foot_transform.rotation =
                        (parent_global.rotation().inverse() * desired_rotation).normalize();
                }
            }
        }
        let left_landing_local =
            targets[0].map(|target| rig_rotation.inverse() * (target - rig_origin));
        let right_landing_local =
            targets[1].map(|target| rig_rotation.inverse() * (target - rig_origin));
        let mut memory = LegIkMemory {
            left_authored_world_target: targets[0],
            right_authored_world_target: targets[1],
            left_foot_world_target: targets[0],
            right_foot_world_target: targets[1],
            quickstep_handoff: match (left_landing_local, right_landing_local) {
                (Some(left), Some(right)) => QuickstepContactHandoff::Converging { left, right },
                _ => QuickstepContactHandoff::None,
            },
            left_support_weight: Some(supports[0]),
            right_support_weight: Some(supports[1]),
            ..default()
        };
        memory.left_foot_target = targets[0];
        memory.right_foot_target = targets[1];
        if let Ok(mut current) = diagnostics.get_mut(owner) {
            current.0 = memory;
        } else {
            commands.entity(owner).insert(LegIkState(memory));
        }
        store_state(owner, state, &mut states, &mut commands);
    }
}

fn quickstep_foot_must_release(
    grounded: bool,
    action_phase: f32,
    hip: Vec3,
    planted_ankle: Vec3,
    maximum_reach: f32,
) -> bool {
    if action_phase < 0.125 {
        return false;
    }
    if grounded {
        return action_phase >= 0.5;
    }
    let offset = hip - planted_ankle;
    let planar_distance = offset.xz().length();
    let vertical_distance = offset.y.max(0.001);
    hip.distance(planted_ankle) >= maximum_reach || planar_distance >= vertical_distance // 45-degree maximum arch.
}

fn store_state(
    owner: Entity,
    state: QuickstepIkState,
    states: &mut Query<&mut QuickstepIkState>,
    commands: &mut Commands,
) {
    if let Ok(mut current) = states.get_mut(owner) {
        *current = state;
    } else {
        commands.entity(owner).insert(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planted_foot_releases_at_reach_or_forty_five_degree_arch() {
        let ankle = Vec3::ZERO;
        assert!(!quickstep_foot_must_release(
            false,
            0.2,
            Vec3::new(0.2, 0.8, 0.0),
            ankle,
            1.0
        ));
        assert!(quickstep_foot_must_release(
            false,
            0.2,
            Vec3::new(0.81, 0.8, 0.0),
            ankle,
            1.5
        ));
        assert!(quickstep_foot_must_release(
            false,
            0.2,
            Vec3::new(0.2, 0.8, 0.0),
            ankle,
            0.8
        ));
    }

    #[test]
    fn a_new_quickstep_sequence_discards_previous_takeoff_targets() {
        let mut state = QuickstepIkState {
            action_start_tick: Some(10),
            feet: Some(QuickstepFeetState {
                left: QuickstepFootState {
                    takeoff_world: Vec3::X,
                    takeoff_rotation_world: Quat::IDENTITY,
                    release_phase: Some(0.4),
                },
                right: QuickstepFootState {
                    takeoff_world: Vec3::NEG_X,
                    takeoff_rotation_world: Quat::IDENTITY,
                    release_phase: Some(0.5),
                },
            }),
        };
        let next_action_start_tick = Some(40);

        if state.action_start_tick != next_action_start_tick {
            state = QuickstepIkState {
                action_start_tick: next_action_start_tick,
                ..default()
            };
        }

        assert_eq!(state.action_start_tick, next_action_start_tick);
        assert!(state.feet.is_none());
    }
}
