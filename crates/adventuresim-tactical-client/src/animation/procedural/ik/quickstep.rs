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
    left_solve_world: Option<Vec3>,
    right_solve_world: Option<Vec3>,
    left_target_velocity: Vec3,
    right_target_velocity: Vec3,
    left_target_acceleration: Vec3,
    right_target_acceleration: Vec3,
    left_desired_target: Option<Vec3>,
    right_desired_target: Option<Vec3>,
    left_ideal_velocity: Vec3,
    right_ideal_velocity: Vec3,
    left_ideal_acceleration: Vec3,
    right_ideal_acceleration: Vec3,
    left_ideal_history_valid: bool,
    right_ideal_history_valid: bool,
    left_knee_bend_world: Option<Vec3>,
    right_knee_bend_world: Option<Vec3>,
    left_end_direction: Option<Vec3>,
    right_end_direction: Option<Vec3>,
    evaluation_tick: Option<u64>,
}

fn seed_quickstep_from_presentation(
    action_start_tick: Option<u64>,
    presentation: Option<LegIkMemory>,
) -> QuickstepIkState {
    QuickstepIkState {
        action_start_tick,
        left_takeoff_world: presentation.and_then(|memory| memory.left_foot_world_target),
        right_takeoff_world: presentation.and_then(|memory| memory.right_foot_world_target),
        left_takeoff_rotation_world: presentation
            .and_then(|memory| memory.left_last_rendered_foot_rotation_world),
        right_takeoff_rotation_world: presentation
            .and_then(|memory| memory.right_last_rendered_foot_rotation_world),
        left_solve_world: presentation.and_then(|memory| memory.left_foot_world_target),
        right_solve_world: presentation.and_then(|memory| memory.right_foot_world_target),
        left_target_velocity: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.velocity),
        right_target_velocity: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.velocity),
        left_target_acceleration: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.acceleration),
        right_target_acceleration: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.acceleration),
        left_desired_target: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map(|follower| follower.previous_ideal),
        right_desired_target: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map(|follower| follower.previous_ideal),
        left_ideal_velocity: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_velocity),
        right_ideal_velocity: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_velocity),
        left_ideal_acceleration: presentation
            .and_then(|memory| memory.left_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_acceleration),
        right_ideal_acceleration: presentation
            .and_then(|memory| memory.right_foot_follower)
            .map_or(Vec3::ZERO, |follower| follower.previous_ideal_acceleration),
        left_ideal_history_valid: presentation
            .is_some_and(|memory| memory.left_foot_follower.is_some()),
        right_ideal_history_valid: presentation
            .is_some_and(|memory| memory.right_foot_follower.is_some()),
        left_knee_bend_world: presentation.and_then(|memory| {
            memory
                .left_terrain_pole_world
                .or(memory.left_anatomical_pole_world)
        }),
        right_knee_bend_world: presentation.and_then(|memory| {
            memory
                .right_terrain_pole_world
                .or(memory.right_anatomical_pole_world)
        }),
        left_end_direction: presentation.and_then(|memory| memory.left_terrain_end_direction),
        right_end_direction: presentation.and_then(|memory| memory.right_terrain_end_direction),
        ..default()
    }
}

/// Retains each takeoff ankle as a world plant while the leg can still reach
/// it without leaning past a 45-degree arch. Only then does that foot tuck and
/// travel toward its authored guard landing position.
pub(in crate::animation) fn apply(
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
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
        let previous_presentation = diagnostics.get_mut(owner).ok().map(|state| state.0);
        if skeleton.action_kind() != SkeletonAction::Dodge {
            if state.left_takeoff_world.is_some() || state.right_takeoff_world.is_some() {
                state = QuickstepIkState::default();
                store_state(owner, state, &mut states, &mut commands);
            }
            continue;
        }
        let action_start_tick = skeleton.action_view().map(|action| match action {
            ActionView::Dodge { timeline, .. }
            | ActionView::Attack { timeline, .. }
            | ActionView::Block { timeline, .. } => timeline.start_tick,
        });
        if state.action_start_tick != action_start_tick {
            state = seed_quickstep_from_presentation(action_start_tick, previous_presentation);
        }
        let (delta_seconds, evaluation_advances) = match clock.fixed_step() {
            Some((tick, _)) if state.evaluation_tick == Some(tick) => (0.0, false),
            Some((tick, delta_seconds)) => {
                state.evaluation_tick = Some(tick);
                (delta_seconds, true)
            }
            None => {
                let delta_seconds = time.delta_secs().max(0.0);
                (delta_seconds, delta_seconds > 0.0)
            }
        };
        if repeated_fixed_tick_skips_ik(clock.fixed_step().is_some(), evaluation_advances) {
            continue;
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
        let mut rendered_targets = [None, None];
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
            let mut release_phase = if left {
                state.left_release_phase
            } else {
                state.right_release_phase
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
                release_phase = Some(action_phase);
            }
            let linear_progress = release_phase.map_or(0.0, |release| {
                if skeleton.is_grounded() && action_phase > 0.125 {
                    1.0
                } else {
                    ((action_phase - release) / (0.625 - release).max(0.001)).clamp(0.0, 1.0)
                }
            });
            let progress = quintic_progress(linear_progress);
            let mut target = takeoff.lerp(authored, progress);
            target.y += (std::f32::consts::PI * progress).sin() * 0.16;
            let (previous, velocity, acceleration, previous_desired) = if left {
                (
                    state.left_solve_world,
                    state.left_target_velocity,
                    state.left_target_acceleration,
                    state.left_desired_target,
                )
            } else {
                (
                    state.right_solve_world,
                    state.right_target_velocity,
                    state.right_target_acceleration,
                    state.right_desired_target,
                )
            };
            let desired_target = target;
            let (previous_ideal_velocity, previous_ideal_acceleration) = if left {
                (state.left_ideal_velocity, state.left_ideal_acceleration)
            } else {
                (state.right_ideal_velocity, state.right_ideal_acceleration)
            };
            let ideal_history_valid = if left {
                state.left_ideal_history_valid
            } else {
                state.right_ideal_history_valid
            };
            let reach_envelope = FootReachEnvelope::new(
                upper_snapshot.global.translation(),
                upper_snapshot.global.translation() + skeleton.world_velocity * delta_seconds,
                (upper_length * upper_length
                    + lower_length * lower_length
                    + 2.0 * upper_length * lower_length * 30.0_f32.to_radians().cos())
                .sqrt(),
                reach,
            );
            let tracked = advance_guard_foot_target_with_reach(
                previous,
                velocity,
                acceleration,
                previous_desired,
                previous_ideal_velocity,
                previous_ideal_acceleration,
                ideal_history_valid,
                target,
                delta_seconds,
                evaluation_advances,
                reach_envelope,
            );
            target = tracked.position;
            if left {
                state.left_solve_world = Some(target);
                state.left_target_velocity = tracked.velocity;
                state.left_target_acceleration = tracked.acceleration;
                state.left_desired_target = Some(desired_target);
                state.left_ideal_velocity = tracked.ideal_velocity;
                state.left_ideal_acceleration = tracked.ideal_acceleration;
                state.left_ideal_history_valid = tracked.ideal_history_valid;
            } else {
                state.right_solve_world = Some(target);
                state.right_target_velocity = tracked.velocity;
                state.right_target_acceleration = tracked.acceleration;
                state.right_desired_target = Some(desired_target);
                state.right_ideal_velocity = tracked.ideal_velocity;
                state.right_ideal_acceleration = tracked.ideal_acceleration;
                state.right_ideal_history_valid = tracked.ideal_history_valid;
            }
            if let Some((reason, _)) = tracked.replan {
                release_phase = Some(action_phase);
                if reason == FootFollowReason::ReachHardLimit {
                    if left {
                        state.left_release_phase = release_phase;
                    } else {
                        state.right_release_phase = release_phase;
                    }
                    // Keep presenting the follower's last-safe target on the
                    // release sample. Dropping to authored FK here would add
                    // a second, discontinuous owner handoff precisely when
                    // the reach guard is trying to retire IK safely.
                    targets[index] = Some(target);
                    supports[index] = 0.0;
                    continue;
                }
            }
            if left {
                state.left_release_phase = release_phase;
            } else {
                state.right_release_phase = release_phase;
            }
            targets[index] = Some(target);
            supports[index] = (progress <= f32::EPSILON) as u8 as f32;
            let canonical = pole_to_world(
                rig_rotation,
                canonical_knee_pole(if left { -1.0 } else { 1.0 }),
            );
            let remembered = if left {
                state.left_knee_bend_world
            } else {
                state.right_knee_bend_world
            };
            let previous_end_direction = if left {
                state.left_end_direction
            } else {
                state.right_end_direction
            };
            let foot_facing = rendered_foot_facing(
                rig,
                left,
                foot_snapshot.global.translation(),
                &parents,
                &transforms.p0(),
            );
            let pole = stabilized_knee_pole(
                remembered,
                previous_end_direction,
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                target,
                canonical,
                foot_facing,
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
            let resolved_end = if let Some(solution) = solve_two_bone_with_reach(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                authored,
                target,
                upper_length,
                lower_length,
                pole,
                reach,
            ) {
                let resolved_end = solution.end;
                let bend = (solution.knee - upper_snapshot.global.translation())
                    .reject_from_normalized(solution.end_direction)
                    .try_normalize();
                if left {
                    if bend.is_some() {
                        state.left_knee_bend_world = bend;
                    }
                    state.left_end_direction = Some(solution.end_direction);
                } else {
                    if bend.is_some() {
                        state.right_knee_bend_world = bend;
                    }
                    state.right_end_direction = Some(solution.end_direction);
                }
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
                Some(resolved_end)
            } else {
                None
            };
            rendered_targets[index] = Some(quickstep_handoff_target(target, resolved_end));
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
            left_foot_world_target: rendered_targets[0],
            right_foot_world_target: rendered_targets[1],
            quickstep_handoff_pending: true,
            quickstep_left_landing_local: rendered_targets[0]
                .map(|target| rig_rotation.inverse() * (target - rig_origin)),
            quickstep_right_landing_local: rendered_targets[1]
                .map(|target| rig_rotation.inverse() * (target - rig_origin)),
            left_support_weight: Some(supports[0]),
            right_support_weight: Some(supports[1]),
            left_foot_follower: state
                .left_ideal_history_valid
                .then(|| {
                    FootFollowerState::from_presented_pose(
                        rendered_targets[0]?,
                        state.left_target_velocity,
                        state.left_target_acceleration,
                        state.left_desired_target?,
                        state.left_ideal_velocity,
                        state.left_ideal_acceleration,
                    )
                })
                .flatten(),
            right_foot_follower: state
                .right_ideal_history_valid
                .then(|| {
                    FootFollowerState::from_presented_pose(
                        rendered_targets[1]?,
                        state.right_target_velocity,
                        state.right_target_acceleration,
                        state.right_desired_target?,
                        state.right_ideal_velocity,
                        state.right_ideal_acceleration,
                    )
                })
                .flatten(),
            ..default()
        };
        memory.left_foot_target = rendered_targets[0];
        memory.right_foot_target = rendered_targets[1];
        if let Ok(mut current) = diagnostics.get_mut(owner) {
            current.0 = memory;
        } else {
            commands.entity(owner).insert(LegIkState(memory));
        }
        store_state(owner, state, &mut states, &mut commands);
    }
}

fn quickstep_handoff_target(requested: Vec3, resolved_end: Option<Vec3>) -> Vec3 {
    resolved_end
        .filter(|end| end.is_finite())
        .unwrap_or(requested)
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
    fn landing_handoff_uses_the_reach_clamped_rendered_endpoint() {
        let requested = Vec3::new(-1.8, 0.1, 0.0);
        let rendered = Vec3::new(-0.9, 0.2, 0.0);

        assert_eq!(
            quickstep_handoff_target(requested, Some(rendered)),
            rendered
        );
        assert_eq!(quickstep_handoff_target(requested, None), requested);
        assert_eq!(
            quickstep_handoff_target(requested, Some(Vec3::splat(f32::NAN))),
            requested
        );
    }

    #[test]
    fn quickstep_onset_seeds_from_the_previous_rendered_foot_pose() {
        let left = Vec3::new(-0.2, 0.1, 0.0);
        let right = Vec3::new(0.2, 0.1, 0.0);
        let left_rotation = Quat::from_rotation_y(0.2);
        let memory = LegIkMemory {
            left_foot_world_target: Some(left),
            right_foot_world_target: Some(right),
            left_last_rendered_foot_rotation_world: Some(left_rotation),
            ..default()
        };

        let state = seed_quickstep_from_presentation(Some(17), Some(memory));
        assert_eq!(state.left_takeoff_world, Some(left));
        assert_eq!(state.right_takeoff_world, Some(right));
        assert_eq!(state.left_solve_world, Some(left));
        assert_eq!(state.right_solve_world, Some(right));
        assert_eq!(state.left_takeoff_rotation_world, Some(left_rotation));
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
