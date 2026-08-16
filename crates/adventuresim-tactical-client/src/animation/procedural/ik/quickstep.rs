use super::*;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(in crate::animation) struct QuickstepIkState {
    action_start_tick: Option<u64>,
    left_takeoff_world: Option<Vec3>,
    right_takeoff_world: Option<Vec3>,
    left_takeoff_rotation_world: Option<Quat>,
    right_takeoff_rotation_world: Option<Quat>,
    left_release_phase: Option<f32>,
    right_release_phase: Option<f32>,
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
        if skeleton.action_kind() != SkeletonAction::Dodge {
            if state.left_takeoff_world.is_some() || state.right_takeoff_world.is_some() {
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
        if skeleton.is_grounded()
            && (state.left_takeoff_world.is_none() || state.right_takeoff_world.is_none())
        {
            for (_, _, foot_role, left) in legs {
                let takeoff = rig
                    .get(&foot_role)
                    .and_then(|foot| transforms.p0().compute_global_transform(*foot).ok())
                    .filter(|global| {
                        global.translation().is_finite() && global.rotation().is_finite()
                    });
                if left {
                    state.left_takeoff_world = takeoff.map(|global| global.translation());
                    state.left_takeoff_rotation_world = takeoff.map(|global| global.rotation());
                } else {
                    state.right_takeoff_world = takeoff.map(|global| global.translation());
                    state.right_takeoff_rotation_world = takeoff.map(|global| global.rotation());
                }
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
            let takeoff = if left {
                state.left_takeoff_world
            } else {
                state.right_takeoff_world
            }
            .unwrap_or_else(|| foot_snapshot.global.translation());
            let authored = foot_snapshot.global.translation();
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot.global.translation().distance(authored);
            let reach = maximum_reach(upper_length, lower_length);
            let release_phase = if left {
                &mut state.left_release_phase
            } else {
                &mut state.right_release_phase
            };
            if release_phase.is_none()
                && quickstep_foot_must_release(
                    skeleton.is_grounded(),
                    action_phase,
                    upper_snapshot.global.translation(),
                    takeoff,
                    reach,
                )
            {
                *release_phase = Some(action_phase);
            }
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
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                authored,
                target,
                upper_length,
                lower_length,
                pole,
                reach,
            ) {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
            }
            let takeoff_rotation = if left {
                state.left_takeoff_rotation_world
            } else {
                state.right_takeoff_rotation_world
            };
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
        let mut memory = LegIkMemory {
            left_authored_world_target: targets[0],
            right_authored_world_target: targets[1],
            left_foot_world_target: targets[0],
            right_foot_world_target: targets[1],
            quickstep_handoff_pending: true,
            quickstep_left_landing_local: targets[0]
                .map(|target| rig_rotation.inverse() * (target - rig_origin)),
            quickstep_right_landing_local: targets[1]
                .map(|target| rig_rotation.inverse() * (target - rig_origin)),
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
            left_takeoff_world: Some(Vec3::X),
            right_takeoff_world: Some(Vec3::NEG_X),
            left_release_phase: Some(0.4),
            right_release_phase: Some(0.5),
            ..default()
        };
        let next_action_start_tick = Some(40);

        if state.action_start_tick != next_action_start_tick {
            state = QuickstepIkState {
                action_start_tick: next_action_start_tick,
                ..default()
            };
        }

        assert_eq!(state.action_start_tick, next_action_start_tick);
        assert!(state.left_takeoff_world.is_none());
        assert!(state.right_takeoff_world.is_none());
        assert!(state.left_release_phase.is_none());
        assert!(state.right_release_phase.is_none());
    }
}
