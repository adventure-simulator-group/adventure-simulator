//! Propagated-pose observation and support/contact truth reconciliation.

use super::*;

const MAX_RETAINED_PLANT_REACH_CORRECTION: f32 = 0.015;

pub(in crate::animation::procedural) fn retain_monotonic_contact_sequence(
    current: u64,
    observed: u64,
) -> u64 {
    // The replicated gait planner can return to its zero-valued idle plan
    // while authored locomotion still owns the crossfade. That is an owner
    // transition, not a landing, so retain the presentation sequence until
    // stationary raised footwork takes over. Genuine cadence increments (and
    // the u64 wrap) remain forward by exactly one.
    if observed.wrapping_sub(current) <= 1 {
        observed
    } else {
        current
    }
}

pub(in crate::animation::procedural) fn release_raised_state_for_authored_locomotion(
    memory: &mut LegIkMemory,
) {
    memory.quickstep_handoff = QuickstepContactHandoff::None;
    // The pose-buffer transition captures the visible raised pelvis offset.
    // Keeping the same correction dormant here would apply it a second time
    // when stationary procedural footwork takes ownership again.
    memory.raised_pelvis_shift = 0.0;
    // Authored FK/contact weights own this interval. Do not expose the prior
    // procedural plant as a current terrain-contact claim while retaining the
    // propagated endpoints needed for the stop handoff.
    memory.left_support_weight = None;
    memory.right_support_weight = None;
}

/// Refresh contact diagnostics from propagated globals. The IK pass runs
/// before transform propagation, while viewer/gameplay consumers observe the
/// propagated hierarchy; twist bones and acquisition blending can make those
/// positions differ materially from the analytic endpoint.
pub(in crate::animation) fn refresh_raised_support_after_propagation(
    terrain: Query<&SceneTerrain>,
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    globals: Query<&GlobalTransform>,
    mut ik_states: Query<&mut LegIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
) {
    let Some(terrain) = terrain.single().ok() else {
        return;
    };
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        let Ok(mut state) = ik_states.get_mut(owner) else {
            continue;
        };
        // Snapshot propagated endpoints before any diagnostic filtering. These
        // are deliberately independent of analytic solve targets: a target can
        // be unreachable, while this position is the pose actually rendered.
        state.0.left_last_rendered_world = rig
            .get(&BoneRole::FootLeft)
            .and_then(|foot| globals.get(*foot).ok())
            .map(GlobalTransform::translation)
            .filter(|ankle| ankle.is_finite());
        state.0.right_last_rendered_world = rig
            .get(&BoneRole::FootRight)
            .and_then(|foot| globals.get(*foot).ok())
            .map(GlobalTransform::translation)
            .filter(|ankle| ankle.is_finite());
        state.0.left_last_rendered_toe_world = rig
            .get(&BoneRole::ToeLeft)
            .and_then(|toe| globals.get(*toe).ok())
            .map(GlobalTransform::translation)
            .filter(|toe| toe.is_finite());
        state.0.right_last_rendered_toe_world = rig
            .get(&BoneRole::ToeRight)
            .and_then(|toe| globals.get(*toe).ok())
            .map(GlobalTransform::translation)
            .filter(|toe| toe.is_finite());
        state.0.left_last_rendered_foot_rotation_world = rig
            .get(&BoneRole::FootLeft)
            .and_then(|foot| globals.get(*foot).ok())
            .map(GlobalTransform::rotation)
            .filter(|rotation| rotation.is_finite());
        state.0.right_last_rendered_foot_rotation_world = rig
            .get(&BoneRole::FootRight)
            .and_then(|foot| globals.get(*foot).ok())
            .map(GlobalTransform::rotation)
            .filter(|rotation| rotation.is_finite());
        // IK deliberately does not modify the authored quickstep poses. It
        // still observes their propagated endpoints so the first post-action
        // solve begins from the last pose the player actually saw.
        let propagated_owner_frame = rig
            .rig_scene()
            .and_then(|rig_scene| globals.get(rig_scene).ok())
            .map(|global| (global.translation(), global.rotation()));
        let left_rendered_world = state.0.left_last_rendered_world;
        let right_rendered_world = state.0.right_last_rendered_world;
        if skeleton.is_quickstep()
            && let (Some((origin, rotation)), Some(left), Some(right)) = (
                propagated_owner_frame,
                left_rendered_world,
                right_rendered_world,
            )
        {
            seed_quickstep_contact_handoff(&mut state.0, origin, rotation, left, right);
        }
        let owner_frame =
            propagated_owner_frame.or_else(|| state.0.rig_origin.zip(state.0.rig_rotation));
        state.0.left_last_rendered_owner = owner_frame.and_then(|(origin, rotation)| {
            left_rendered_world
                .map(|world| rotation.inverse() * (world - origin))
                .filter(|owner| owner.is_finite())
        });
        state.0.right_last_rendered_owner = owner_frame.and_then(|(origin, rotation)| {
            right_rendered_world
                .map(|world| rotation.inverse() * (world - origin))
                .filter(|owner| owner.is_finite())
        });
        if let Some((origin, rotation)) = propagated_owner_frame {
            // Keep discontinuity detection in the same owner frame as these
            // freshly observed endpoints. Otherwise a long authored segment
            // compares its terminal root against the stale quickstep origin
            // and discards the continuity snapshot during handoff.
            state.0.rig_origin = Some(origin);
            state.0.rig_rotation = Some(rotation);
        }
        // Authored locomotion owns the leg pose, but its final propagated feet
        // are still the continuity boundary for the stationary follower that
        // may take ownership on the next tick.
        if locomotion::owns(skeleton) {
            continue;
        }
        if locomotion_profile(skeleton).gait == LocomotionGait::Run
            && skeleton.is_grounded()
            && skeleton.action_kind() == SkeletonAction::None
            && skeleton.weapon_guard() == WeaponGuardState::Lowered
            && skeleton.animation_speed() > 0.05
        {
            let (left_nominal, right_nominal) = locomotion_support_weights(skeleton);
            for (role, left, nominal, logical, target) in [
                (
                    BoneRole::FootLeft,
                    true,
                    left_nominal,
                    state.0.left_transition_support_weight,
                    state.0.left_foot_plant,
                ),
                (
                    BoneRole::FootRight,
                    false,
                    right_nominal,
                    state.0.right_transition_support_weight,
                    state.0.right_foot_plant,
                ),
            ] {
                let ankle = rig
                    .get(&role)
                    .and_then(|foot| globals.get(*foot).ok())
                    .map(GlobalTransform::translation)
                    .filter(|ankle| ankle.is_finite());
                let terrain_height = ankle.and_then(|ankle| terrain.height_at(ankle.xz()));
                let target_distance = ankle
                    .zip(target)
                    .map(|(ankle, target)| ankle.xz().distance(target.xz()));
                let actual = target_distance.is_some_and(|distance| distance <= 0.02)
                    && ankle
                        .zip(terrain_height)
                        .is_some_and(|(ankle, height)| sole_is_at_contact(ankle.y, height));
                let reported = if actual {
                    logical.unwrap_or(nominal).max(nominal)
                } else {
                    0.0
                };
                if left {
                    state.0.left_support_weight = Some(reported);
                    state.0.left_foot_plant_acquired |= actual;
                } else {
                    state.0.right_support_weight = Some(reported);
                    state.0.right_foot_plant_acquired |= actual;
                }
            }
        }
        let Ok(mut raised) = raised_states.get_mut(owner) else {
            continue;
        };
        if !raised.step.initialized() {
            continue;
        }
        let swing_foot = raised.step.swing_foot();
        for (role, left, nominal_support) in [
            (BoneRole::FootLeft, true, swing_foot != Some(LeadFoot::Left)),
            (
                BoneRole::FootRight,
                false,
                swing_foot != Some(LeadFoot::Right),
            ),
        ] {
            let Some(&foot) = rig.get(&role) else {
                continue;
            };
            let Ok(global) = globals.get(foot) else {
                continue;
            };
            let ankle = global.translation();
            let support = terrain
                .height_at(ankle.xz())
                .is_some_and(|height| raised_support_is_actual(nominal_support, ankle.y, height));
            if left {
                raised.left_solve_target = Some(ankle);
                state.0.left_foot_world_target = Some(ankle);
                state.0.left_support_weight = Some(support as u8 as f32);
            } else {
                raised.right_solve_target = Some(ankle);
                state.0.right_foot_world_target = Some(ankle);
                state.0.right_support_weight = Some(support as u8 as f32);
            }
        }
    }
}
pub(in crate::animation::procedural) fn raised_footwork_posture_is_valid(
    skeleton: &SkeletonState,
) -> bool {
    skeleton.is_grounded() && skeleton.posture() == Posture::Upright
}

pub(in crate::animation::procedural) fn terrain_ik_posture_is_valid(
    skeleton: &SkeletonState,
) -> bool {
    skeleton.is_grounded()
        && !skeleton.is_posture_transitioning()
        && skeleton.posture() == Posture::Upright
        && matches!(
            skeleton.action_kind(),
            SkeletonAction::None | SkeletonAction::Attack
        )
}

pub(in crate::animation::procedural) fn terrain_leg_has_support(weight: f32) -> bool {
    weight > 0.05
}

pub(in crate::animation::procedural) fn update_contact_orientation_blend(
    active: bool,
    previous_support: Option<f32>,
    reported_support: f32,
) -> bool {
    let supported = terrain_leg_has_support(reported_support);
    supported && (active || !previous_support.is_some_and(terrain_leg_has_support))
}

pub(in crate::animation::procedural) fn retained_plant_requires_release(
    retained: Vec3,
    reachable: Vec3,
) -> bool {
    retained.xz().distance(reachable.xz()) > MAX_RETAINED_PLANT_REACH_CORRECTION
}

pub(in crate::animation::procedural) fn ordinary_plant_requires_clear(
    support_weight: f32,
    acquired: bool,
    plant: Option<Vec3>,
    authored_foot: Vec3,
) -> bool {
    support_weight <= 0.05
        || (!acquired
            && plant.is_some_and(|position| !plant_is_continuous(position, authored_foot)))
}

pub(in crate::animation::procedural) fn coordinated_support_weight(
    gait: LocomotionGait,
    nominal_support: f32,
    acquired_plant: bool,
    opposite_acquired: bool,
) -> f32 {
    if gait != LocomotionGait::Run && acquired_plant && !opposite_acquired {
        // Phase requests the next step; actual replacement contact completes
        // the handoff. Until then the only acquired world plant remains the
        // support owner, even beyond its nominal lobe.
        1.0
    } else {
        nominal_support
    }
}

pub(in crate::animation::procedural) fn run_toe_off_support_weight(
    gait: LocomotionGait,
    nominal_support: f32,
    acquired_plant: bool,
    at_support_exit: bool,
) -> (bool, f32) {
    if gait == LocomotionGait::Run && acquired_plant && at_support_exit {
        (true, 0.0)
    } else {
        (false, nominal_support)
    }
}

pub(in crate::animation::procedural) fn run_retained_support_through_lobe_edge(
    gait: LocomotionGait,
    nominal_support: f32,
    acquired_plant: bool,
    at_support_exit: bool,
) -> f32 {
    if gait == LocomotionGait::Run && acquired_plant && !at_support_exit {
        // Once a footprint has been truthfully acquired, it remains an exact
        // rendered contact throughout the held stance. Raw gait confidence is
        // an authored blend curve, not evidence that contact was lost; letting
        // it dip below the acquisition threshold emitted duplicate same-foot
        // touchdown events before the explicit signed-phase toe-off.
        1.0
    } else {
        nominal_support
    }
}

pub(in crate::animation::procedural) fn run_release_edge(
    previous_support_released: bool,
    toe_off_started: bool,
) -> bool {
    previous_support_released || toe_off_started
}

pub(in crate::animation::procedural) fn unplanned_terrain_solve_requires_release(
    planned_contact: Option<Vec3>,
    solved_target: Vec3,
    authored_target: Vec3,
) -> bool {
    planned_contact.is_none() && solved_target.distance(authored_target) > 0.03
}

pub(in crate::animation::procedural) fn unplanned_support_release_is_owned(
    was_releasing: bool,
    previous_transition_support: Option<f32>,
    previous_reported_support: Option<f32>,
    planned_contact: Option<Vec3>,
    solved_target: Vec3,
    planted_target: Vec3,
    authored_target: Vec3,
) -> bool {
    was_releasing
        || previous_transition_support.is_some_and(terrain_leg_has_support)
        || previous_reported_support.is_some_and(terrain_leg_has_support)
        || solved_target.distance_squared(planted_target) > 0.000001
        || unplanned_terrain_solve_requires_release(planned_contact, solved_target, authored_target)
}
