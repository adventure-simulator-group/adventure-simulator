use super::*;

/// John has three unsupported fixed samples in the calibrated quickstep. The
/// blend reaches guard on the last of them, before the landing sample.
const QUICKSTEP_GUARD_FK_BLEND_TICKS: u64 = 3;
// The John calibration releases support at semantic contact. Preserve
// airborne FK ownership through the remainder of the action even when
// replication skips the controller's unsupported samples.
const QUICKSTEP_TAKEOFF_PHASE: f32 = 0.50;

fn quickstep_phase_is_airborne(phase: f32) -> bool {
    phase >= QUICKSTEP_TAKEOFF_PHASE
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub(in crate::animation) struct QuickstepIkState {
    action_start_tick: Option<u64>,
    airborne_start_tick: Option<u64>,
    feet: Option<QuickstepFeetState>,
    takeoff_pose: Option<QuickstepLocalPose>,
    guard_pose: Option<QuickstepLocalPose>,
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
}

#[derive(Debug, Clone, Copy)]
struct QuickstepLocalPose {
    left: QuickstepLegPose,
    right: QuickstepLegPose,
}

#[derive(Debug, Clone, Copy)]
struct QuickstepLegPose {
    thigh: Transform,
    shin: Transform,
    foot: Transform,
}

/// Keeps both ankles planted during the grounded load. The instant support is
/// lost, IK releases and all six leg joints FK-blend from the last planted pose
/// to the authored guard pose. Airborne feet therefore have no world targets.
pub(in crate::animation) fn apply(
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut states: Query<&mut QuickstepIkState>,
    mut diagnostics: Query<&mut LegIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
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
        let quickstep_phase = skeleton.action_phase();
        let predicted_airborne =
            skeleton.is_quickstep() && quickstep_phase_is_airborne(quickstep_phase);
        let presentation_airborne =
            skeleton.is_quickstep() && (!skeleton.is_grounded() || predicted_airborne);
        let landing_from_quickstep = state.airborne_start_tick.is_some()
            && (!skeleton.is_quickstep() || (skeleton.is_grounded() && !predicted_airborne));
        if !skeleton.is_quickstep() && !landing_from_quickstep {
            // Remember the actual post-footwork guard chain before the action
            // begins. Capturing it after the dodge route is already active can
            // preserve that route's displaced lower body and then force a
            // large correction on the first landing frame.
            let guard_pose = (skeleton.is_grounded()
                && skeleton.posture() == Posture::Upright
                && skeleton.weapon_guard() == WeaponGuardState::Raised)
                .then(|| capture_local_pose(rig, &transforms.p1()))
                .flatten()
                .or(state.guard_pose);
            store_state(
                owner,
                QuickstepIkState {
                    guard_pose,
                    ..default()
                },
                &mut states,
                &mut commands,
            );
            continue;
        }
        let action_start_tick = skeleton.action_start_tick();
        if skeleton.is_quickstep() && state.action_start_tick != action_start_tick {
            state = QuickstepIkState {
                action_start_tick,
                guard_pose: state.guard_pose,
                ..default()
            };
        }

        if landing_from_quickstep {
            // Impact ends quickstep ownership. Do not reach back toward the
            // original takeoff plants; the authored guard legs are already in
            // place and ordinary footwork takes over on the next evaluation.
            state.feet = None;
            state.takeoff_pose = None;
            if let Some(guard) = state.guard_pose {
                apply_fk_pose_blend(rig, guard, guard, 1.0, &mut transforms.p1());
            }
            state.airborne_start_tick = None;
            let mut memory = diagnostics
                .get(owner)
                .map(|current| current.0)
                .unwrap_or_default();
            clear_quickstep_targets(&mut memory, 1.0);
            if let Ok(mut raised) = raised_states.get_mut(owner) {
                *raised = capture_world_feet(rig, &transforms.p0()).map_or_else(
                    RaisedFootworkState::default,
                    |(left, right)| RaisedFootworkState {
                        step: GuardStepState::Stationary {
                            left,
                            right,
                            // The leading foot receives the landing load; free
                            // the trailing foot first so it cannot be dragged
                            // beyond leg reach while the root decelerates.
                            next: opposite_guard_foot(select_initial_guard_swing(
                                left,
                                right,
                                skeleton.world_velocity,
                                skeleton.lead_foot,
                            )),
                        },
                        step_sequence: 0,
                        evaluation_tick: None,
                        left_support_weight: 1.0,
                        right_support_weight: 1.0,
                        left_solve_target: Some(left),
                        right_solve_target: Some(right),
                        left_knee_bend_world: None,
                        right_knee_bend_world: None,
                        left_end_direction: None,
                        right_end_direction: None,
                    },
                );
            }
            store_diagnostics(owner, memory, &mut diagnostics, &mut commands);
        } else if !presentation_airborne {
            state.airborne_start_tick = None;
            // Capture the unmodified stance before planted IK begins. This is
            // the fixed FK destination; the action route itself is not a
            // reliable source of lower-body guard transforms later in flight.
            if state.guard_pose.is_none() {
                state.guard_pose = capture_local_pose(rig, &transforms.p1());
            }
            if state.feet.is_none() {
                let capture = |role: BoneRole, helper: &TransformHelper| {
                    rig.get(&role)
                        .and_then(|foot| helper.compute_global_transform(*foot).ok())
                        .filter(|global| {
                            global.translation().is_finite() && global.rotation().is_finite()
                        })
                        .map(|global| QuickstepFootState {
                            takeoff_world: global.translation(),
                            takeoff_rotation_world: global.rotation(),
                        })
                };
                let left = capture(BoneRole::FootLeft, &transforms.p0());
                let right = capture(BoneRole::FootRight, &transforms.p0());
                if let (Some(left), Some(right)) = (left, right) {
                    state.feet = Some(QuickstepFeetState { left, right });
                }
            }
            let Some(rig_rotation) = rig
                .rig_scene()
                .and_then(|entity| transforms.p0().compute_global_transform(entity).ok())
                .map(|global| global.rotation())
            else {
                continue;
            };
            let (targets, knee_bends) =
                apply_planted_ik(rig, state.feet, rig_rotation, &parents, &mut transforms);
            state.takeoff_pose = capture_local_pose(rig, &transforms.p1());
            let mut memory = diagnostics
                .get(owner)
                .map(|current| current.0)
                .unwrap_or_default();
            memory.left_authored_world_target = targets[0];
            memory.right_authored_world_target = targets[1];
            memory.left_foot_world_target = targets[0];
            memory.right_foot_world_target = targets[1];
            memory.left_foot_target = targets[0];
            memory.right_foot_target = targets[1];
            memory.left_support_weight = Some(1.0);
            memory.right_support_weight = Some(1.0);
            memory.quickstep_handoff = QuickstepContactHandoff::None;
            if let Some(bend) = knee_bends[0] {
                memory.left_leg = Some(pole_to_owner(rig_rotation, bend));
            }
            if let Some(bend) = knee_bends[1] {
                memory.right_leg = Some(pole_to_owner(rig_rotation, bend));
            }
            store_diagnostics(owner, memory, &mut diagnostics, &mut commands);
        } else {
            let start = *state
                .airborne_start_tick
                .get_or_insert(skeleton.locomotion_sample_tick);
            // +1 starts recovery on the first observed unsupported frame.
            let elapsed = skeleton
                .locomotion_sample_tick
                .wrapping_sub(start)
                .saturating_add(1);
            if let (Some(takeoff), Some(guard)) = (state.takeoff_pose, state.guard_pose) {
                apply_fk_pose_blend(
                    rig,
                    takeoff,
                    guard,
                    quickstep_guard_fk_progress(elapsed),
                    &mut transforms.p1(),
                );
            }
            // Landing footwork must start from authored guard, not a stale plant.
            if let Ok(mut raised) = raised_states.get_mut(owner) {
                *raised = RaisedFootworkState::default();
            }
            let mut memory = diagnostics
                .get(owner)
                .map(|current| current.0)
                .unwrap_or_default();
            clear_quickstep_targets(&mut memory, 0.0);
            store_diagnostics(owner, memory, &mut diagnostics, &mut commands);
        }
        store_state(owner, state, &mut states, &mut commands);
    }
}

fn capture_world_feet(rig: &HumanoidRig, helper: &TransformHelper) -> Option<(Vec3, Vec3)> {
    let left = helper
        .compute_global_transform(*rig.get(&BoneRole::FootLeft)?)
        .ok()?
        .translation();
    let right = helper
        .compute_global_transform(*rig.get(&BoneRole::FootRight)?)
        .ok()?
        .translation();
    Some((left, right))
}

fn clear_quickstep_targets(memory: &mut LegIkMemory, support_weight: f32) {
    memory.left_authored_world_target = None;
    memory.right_authored_world_target = None;
    memory.left_foot_world_target = None;
    memory.right_foot_world_target = None;
    memory.left_foot_target = None;
    memory.right_foot_target = None;
    memory.left_support_weight = Some(support_weight);
    memory.right_support_weight = Some(support_weight);
    memory.quickstep_handoff = QuickstepContactHandoff::None;
}

fn apply_planted_ik(
    rig: &HumanoidRig,
    feet: Option<QuickstepFeetState>,
    rig_rotation: Quat,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) -> ([Option<Vec3>; 2], [Option<Vec3>; 2]) {
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
    let mut targets = [None, None];
    let mut knee_bends = [None, None];
    for (index, (upper_role, lower_role, foot_role, left)) in legs.into_iter().enumerate() {
        let (Some(&upper), Some(&lower), Some(&foot)) = (
            rig.get(&upper_role),
            rig.get(&lower_role),
            rig.get(&foot_role),
        ) else {
            continue;
        };
        let Some(foot_state) = feet.map(|feet| if left { feet.left } else { feet.right }) else {
            continue;
        };
        let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
            snapshot_chain(upper, lower, foot, parents, &transforms.p0())
        else {
            continue;
        };
        let target = foot_state.takeoff_world;
        targets[index] = Some(target);
        let authored = foot_snapshot.global.translation();
        let upper_length = upper_snapshot
            .global
            .translation()
            .distance(lower_snapshot.global.translation());
        let lower_length = lower_snapshot.global.translation().distance(authored);
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
            parents,
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
            maximum_reach(upper_length, lower_length),
        ) {
            knee_bends[index] = (solution.knee - upper_snapshot.global.translation())
                .reject_from_normalized(solution.end_direction)
                .try_normalize();
            apply_two_bone_solution(upper, lower, foot, solution, parents, transforms);
        }
        if let Ok(parent) = parents.get(foot)
            && let Ok(parent_global) = transforms.p0().compute_global_transform(parent.parent())
            && let Ok(mut local) = transforms.p1().get_mut(foot)
        {
            local.rotation = (parent_global.rotation().inverse()
                * foot_state.takeoff_rotation_world)
                .normalize();
        }
    }
    (targets, knee_bends)
}

fn quickstep_guard_fk_progress(elapsed_ticks: u64) -> f32 {
    // With only three unsupported samples, easing would concentrate almost
    // half the required leg travel into the middle sample. Linear FK gives
    // each airborne frame an equal share and reaches guard before impact.
    (elapsed_ticks as f32 / QUICKSTEP_GUARD_FK_BLEND_TICKS as f32).clamp(0.0, 1.0)
}

fn blend_transform(from: Transform, to: Transform, progress: f32) -> Transform {
    Transform {
        translation: from.translation.lerp(to.translation, progress),
        rotation: from.rotation.slerp(to.rotation, progress).normalize(),
        scale: from.scale.lerp(to.scale, progress),
    }
}

fn capture_local_pose(
    rig: &HumanoidRig,
    transforms: &Query<&mut Transform>,
) -> Option<QuickstepLocalPose> {
    Some(QuickstepLocalPose {
        left: capture_local_leg(rig, true, transforms)?,
        right: capture_local_leg(rig, false, transforms)?,
    })
}

fn capture_local_leg(
    rig: &HumanoidRig,
    left: bool,
    transforms: &Query<&mut Transform>,
) -> Option<QuickstepLegPose> {
    let roles = if left {
        (BoneRole::ThighLeft, BoneRole::ShinLeft, BoneRole::FootLeft)
    } else {
        (
            BoneRole::ThighRight,
            BoneRole::ShinRight,
            BoneRole::FootRight,
        )
    };
    Some(QuickstepLegPose {
        thigh: *transforms.get(*rig.get(&roles.0)?).ok()?,
        shin: *transforms.get(*rig.get(&roles.1)?).ok()?,
        foot: *transforms.get(*rig.get(&roles.2)?).ok()?,
    })
}

fn apply_fk_pose_blend(
    rig: &HumanoidRig,
    takeoff: QuickstepLocalPose,
    guard: QuickstepLocalPose,
    progress: f32,
    transforms: &mut Query<&mut Transform>,
) {
    apply_fk_leg_blend(rig, true, takeoff.left, guard.left, progress, transforms);
    apply_fk_leg_blend(rig, false, takeoff.right, guard.right, progress, transforms);
}

fn apply_fk_leg_blend(
    rig: &HumanoidRig,
    left: bool,
    takeoff: QuickstepLegPose,
    guard: QuickstepLegPose,
    progress: f32,
    transforms: &mut Query<&mut Transform>,
) {
    let roles = if left {
        (BoneRole::ThighLeft, BoneRole::ShinLeft, BoneRole::FootLeft)
    } else {
        (
            BoneRole::ThighRight,
            BoneRole::ShinRight,
            BoneRole::FootRight,
        )
    };
    for (role, from, to) in [
        (roles.0, takeoff.thigh, guard.thigh),
        (roles.1, takeoff.shin, guard.shin),
        (roles.2, takeoff.foot, guard.foot),
    ] {
        let Some(&entity) = rig.get(&role) else {
            continue;
        };
        let Ok(mut current) = transforms.get_mut(entity) else {
            continue;
        };
        *current = blend_transform(from, to, progress);
    }
}

fn store_diagnostics(
    owner: Entity,
    memory: LegIkMemory,
    diagnostics: &mut Query<&mut LegIkState>,
    commands: &mut Commands,
) {
    if let Ok(mut current) = diagnostics.get_mut(owner) {
        current.0 = memory;
    } else {
        commands.entity(owner).insert(LegIkState(memory));
    }
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
    fn guard_fk_recovery_begins_immediately_and_finishes_early() {
        assert!(quickstep_guard_fk_progress(1) > 0.0);
        assert!(quickstep_guard_fk_progress(1) < 1.0);
        assert_eq!(
            quickstep_guard_fk_progress(QUICKSTEP_GUARD_FK_BLEND_TICKS),
            1.0
        );
        assert_eq!(quickstep_guard_fk_progress(100), 1.0);
    }

    #[test]
    fn replicated_grounded_samples_cannot_hide_the_fk_airborne_window() {
        assert!(!quickstep_phase_is_airborne(
            QUICKSTEP_TAKEOFF_PHASE - 0.001
        ));
        assert!(quickstep_phase_is_airborne(QUICKSTEP_TAKEOFF_PHASE));
        assert!(quickstep_phase_is_airborne(0.999));
        assert!(quickstep_phase_is_airborne(1.0));
    }

    #[test]
    fn fk_blend_arrives_exactly_at_authored_guard_transform() {
        let takeoff = Transform::from_translation(Vec3::new(1.0, -0.3, 0.5))
            .with_rotation(Quat::from_rotation_x(0.7));
        let guard = Transform::from_translation(Vec3::new(-0.2, 0.1, -0.4))
            .with_rotation(Quat::from_rotation_z(-0.4));
        let first = blend_transform(takeoff, guard, quickstep_guard_fk_progress(1));
        assert!(!first.translation.abs_diff_eq(takeoff.translation, 0.0001));
        let landed = blend_transform(
            takeoff,
            guard,
            quickstep_guard_fk_progress(QUICKSTEP_GUARD_FK_BLEND_TICKS),
        );
        assert!(landed.translation.abs_diff_eq(guard.translation, 0.0001));
        assert!(landed.rotation.abs_diff_eq(guard.rotation, 0.0001));
        assert!((landed.rotation.length() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn a_new_quickstep_discards_previous_takeoff_pose() {
        let leg = QuickstepLegPose {
            thigh: Transform::IDENTITY,
            shin: Transform::IDENTITY,
            foot: Transform::IDENTITY,
        };
        let mut state = QuickstepIkState {
            action_start_tick: Some(10),
            airborne_start_tick: Some(20),
            feet: Some(QuickstepFeetState {
                left: QuickstepFootState {
                    takeoff_world: Vec3::X,
                    takeoff_rotation_world: Quat::IDENTITY,
                },
                right: QuickstepFootState {
                    takeoff_world: Vec3::X,
                    takeoff_rotation_world: Quat::IDENTITY,
                },
            }),
            takeoff_pose: Some(QuickstepLocalPose {
                left: leg,
                right: leg,
            }),
            guard_pose: Some(QuickstepLocalPose {
                left: leg,
                right: leg,
            }),
        };
        let next = Some(40);
        if state.action_start_tick != next {
            state = QuickstepIkState {
                action_start_tick: next,
                ..default()
            };
        }
        assert_eq!(state.action_start_tick, next);
        assert!(state.feet.is_none());
        assert!(state.takeoff_pose.is_none());
        assert!(state.guard_pose.is_none());
        assert!(state.airborne_start_tick.is_none());
    }
}
