//! Procedural lower-body coordination.
//!
//! This facade owns Bevy query routing, presentation-tick sequencing, owner
//! handoffs, and the single ordered terrain solve. Retained component state,
//! raised footwork, stop settlement, terrain-contact policy, orientation, and
//! propagated-pose observation live in responsibility-named implementation
//! fragments below. Hand constraints, authored locomotion conformity, and the
//! analytic two-bone solver remain ordinary cohesive child modules.

use super::*;

mod hands;
mod locomotion;
mod observation;
mod orientation;
mod raised_footwork;
mod settle;
mod solver;
mod state;
mod terrain_contacts;
#[cfg(test)]
mod tests;

pub(in crate::animation) use hands::apply_arm_and_weapon_constraints;
#[cfg(test)]
pub(super) use hands::secondary_grip_world;
pub(crate) use hands::{HandIkTarget, HandSide, HeldWeaponConstraint, HumanoidIkTargets};
pub(super) use locomotion::owns as authored_locomotion_owns;
pub(in crate::animation) use observation::refresh_raised_support_after_propagation;
use observation::*;
#[cfg(test)]
pub(super) use observation::{
    raised_footwork_posture_is_valid, retained_plant_requires_release, terrain_ik_posture_is_valid,
    terrain_leg_has_support,
};
pub(super) use orientation::constrain_rendered_leg_pole;
pub(in crate::animation) use orientation::enforce_anatomical_knee_yaw;
use orientation::*;
#[cfg(test)]
pub(super) use orientation::{
    anatomical_knee_yaw_posture_is_valid, authored_knee_pole_world, slope_aligned_world_rotation,
};
pub(crate) use raised_footwork::RaisedFootworkState;
use raised_footwork::*;
#[cfg(test)]
pub(super) use raised_footwork::{constrain_foot_to_track, terrain_conformed_guard_target};
use settle::*;
#[cfg(test)]
pub(super) use settle::{
    balance_recovery_direction, plan_settle_landing, projected_capture_point, settle_swing_side,
    sole_is_at_contact,
};
pub(super) use solver::*;
use state::*;
pub(crate) use state::{ArmIkState, LegIkDiagnostics, LegIkState};
pub(crate) use terrain_contacts::locomotion_support_weights;
#[cfg(test)]
pub(super) use terrain_contacts::settle_swing_target;
pub(super) use terrain_contacts::smoothstep;
use terrain_contacts::*;

fn ik_tuning() -> InverseKinematicsConfig {
    runtime_animation_config().inverse_kinematics
}

pub(super) fn minimum_inter_foot_separation_metres() -> f32 {
    ik_tuning().minimum_inter_foot_separation_metres
}

pub(super) fn foot_track_inner_metres() -> f32 {
    minimum_inter_foot_separation_metres() * 0.5
}

pub(super) fn maximum_pelvis_correction_step_metres() -> f32 {
    ik_tuning().maximum_pelvis_correction_step_metres
}

pub(crate) fn measured_ankle_sole_offset_metres() -> f32 {
    ik_tuning().measured_ankle_sole_offset_metres
}

pub(crate) fn sole_contact_tolerance_metres() -> f32 {
    ik_tuning().sole_contact_tolerance_metres
}

/// Places the planted foot on the terrain with an analytic two-bone solve,
/// then lowers the hips by the bounded residual. Existing weapon/hand
/// constraints run at the same final-pose seam.
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each independently borrowed terrain IK resource and query as a system parameter"
)]
pub(in crate::animation) fn apply_terrain_leg_ik(
    enabled: Res<super::super::TerrainIkEnabled>,
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut ik_states: Query<&mut LegIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let _spike = crate::animation::diagnostics::SpikeGuard::new("apply_terrain_leg_ik");
    let terrain = terrain.single().ok();
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        if locomotion::owns(skeleton) {
            // The authored pose remains untouched, but allocate observational
            // memory so the post-propagation pass can retain its final feet
            // for a continuous transfer to stationary procedural footwork.
            if let Ok(mut state) = ik_states.get_mut(owner) {
                // Authored locomotion has already accepted the post-dodge
                // lower body. A still-pending landing handoff must not survive
                // that owner and reappear later when the character stops.
                release_raised_state_for_authored_locomotion(&mut state.0);
            } else {
                commands
                    .entity(owner)
                    .insert(LegIkState(LegIkMemory::default()));
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                let step_sequence = retain_monotonic_contact_sequence(
                    state.step_sequence,
                    skeleton.contact_sequence,
                );
                *state = RaisedFootworkState {
                    step_sequence,
                    ..default()
                };
            } else {
                commands.entity(owner).insert(RaisedFootworkState {
                    step_sequence: skeleton.contact_sequence,
                    ..default()
                });
            }
            continue;
        }
        if !terrain_ik_posture_is_valid(skeleton) {
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = LegIkMemory::default();
            } else {
                // Quicksteps bypass every leg modifier, yet need a component
                // on which the later propagated-pose observer can record the
                // authored landing endpoints.
                commands
                    .entity(owner)
                    .insert(LegIkState(LegIkMemory::default()));
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                let step_sequence = retain_monotonic_contact_sequence(
                    state.step_sequence,
                    skeleton.contact_sequence,
                );
                *state = RaisedFootworkState {
                    step_sequence,
                    ..default()
                };
            }
            continue;
        }
        let raised_guard_follower = raised_footwork_posture_is_valid(skeleton)
            && skeleton.weapon_guard() == WeaponGuardState::Raised
            && matches!(
                skeleton.action_kind(),
                SkeletonAction::None | SkeletonAction::Attack
            );
        let raised_footwork_was_active = raised_states
            .get(owner)
            .is_ok_and(|state| state.step.initialized());
        let raised_footwork_handoff = !raised_guard_follower && raised_footwork_was_active;
        let (mut left_weight, mut right_weight) = locomotion_support_weights(skeleton);
        // Preserve the profile-owned cadence before settle, ownership, and
        // exhausted-lobe state can suppress effective support. Run touchdown
        // descent is a phase fact and must never be derived from the mutable
        // solver/reporting weights below.
        let raw_run_support =
            (locomotion_profile(skeleton).gait == LocomotionGait::Run).then(|| {
                gait_support_weights(
                    locomotion_profile(skeleton),
                    skeleton.gait_phase.rem_euclid(1.0),
                )
            });
        let mut legs = [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                left_weight,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                right_weight,
                false,
            ),
        ];
        let (mut memory, memory_was_missing) = match ik_states.get_mut(owner) {
            Ok(state) => (state.0, false),
            Err(_) => (
                // Startup is not a toggle transition: establish the configured
                // mode immediately so the first supported frame can plant.
                LegIkMemory {
                    terrain_blend: if enabled.0 { 1.0 } else { 0.0 },
                    ..default()
                },
                true,
            ),
        };
        let (state_delta_seconds, evaluation_advances) = match clock.fixed_tick {
            Some((tick, _)) if memory.evaluation_tick == Some(tick) => (0.0, false),
            Some((tick, delta_seconds)) => {
                memory.evaluation_tick = Some(tick);
                (delta_seconds, true)
            }
            None => {
                let delta_seconds = time.delta_secs();
                (delta_seconds, delta_seconds > 0.0)
            }
        };
        if repeated_fixed_tick_skips_ik(clock.fixed_tick.is_some(), evaluation_advances) {
            // Multi-view tools restore the first complete local pose at the
            // end of the procedural chain. Re-running IK from memory already
            // advanced by that first view can re-enter a decaying support
            // branch and commit a next-cycle plant into the same logical tick.
            // Skip both evaluation and state mutation for repeated fixed ticks.
            continue;
        }
        if state_delta_seconds > 0.0 {
            let desired = if terrain_ik_is_required(
                enabled.0,
                memory.settle.is_some(),
                raised_footwork_handoff,
            ) {
                1.0
            } else {
                0.0
            };
            memory.terrain_blend += (desired - memory.terrain_blend).clamp(
                -ik_tuning().terrain_blend_speed_per_second * state_delta_seconds,
                ik_tuning().terrain_blend_speed_per_second * state_delta_seconds,
            );
        }
        let terrain_blend = memory.terrain_blend.clamp(0.0, 1.0);
        // Plant and pelvis reach belong to the server-owned authored-body
        // frame. Terrain knee poles retain their world bend plane separately
        // so a sharp owner turn cannot corkscrew a planted knee.
        let (rig_origin, rig_rotation) = rig
            .rig_scene()
            .and_then(|entity| transforms.p0().compute_global_transform(entity).ok())
            .map(|global| (global.translation(), global.rotation()))
            .unwrap_or((Vec3::ZERO, Quat::IDENTITY));
        if let Some((left, right)) = memory.quickstep_handoff.targets() {
            memory.left_foot_world_target = Some(rig_origin + rig_rotation * left);
            memory.right_foot_world_target = Some(rig_origin + rig_rotation * right);
        }
        if state_delta_seconds > 0.0 {
            let previous_rig_origin = memory.rig_origin;
            let owner_discontinuous = previous_rig_origin.is_some_and(|previous| {
                previous.distance(rig_origin)
                    > ik_tuning().maximum_owner_translation_per_tick_metres
            }) || memory.rig_rotation.is_some_and(|previous| {
                previous.angle_between(rig_rotation).to_degrees()
                    > ik_tuning().maximum_owner_rotation_per_tick_degrees
            });
            memory.measured_owner_planar_speed = update_measured_owner_planar_speed(
                memory.measured_owner_planar_speed,
                previous_rig_origin,
                rig_origin,
                state_delta_seconds,
                evaluation_advances,
                owner_discontinuous,
            );
            if owner_discontinuous {
                memory.left_foot_plant = None;
                memory.right_foot_plant = None;
                memory.left_foot_plant_acquired = false;
                memory.right_foot_plant_acquired = false;
                memory.left_foot_target = None;
                memory.right_foot_target = None;
                memory.left_foot_world_target = None;
                memory.right_foot_world_target = None;
                memory.left_last_rendered_world = None;
                memory.right_last_rendered_world = None;
                memory.left_last_rendered_toe_world = None;
                memory.right_last_rendered_toe_world = None;
                memory.left_last_rendered_owner = None;
                memory.right_last_rendered_owner = None;
                memory.left_last_rendered_foot_rotation_world = None;
                memory.right_last_rendered_foot_rotation_world = None;
                memory.left_authored_world_target = None;
                memory.right_authored_world_target = None;
                clear_all_planned_contact_metadata(&mut memory);
                memory.left_support_weight = None;
                memory.right_support_weight = None;
                memory.left_transition_support_weight = None;
                memory.right_transition_support_weight = None;
                memory.left_support_exhausted_until_flight = false;
                memory.right_support_exhausted_until_flight = false;
                memory.left_terrain_pole_world = None;
                memory.right_terrain_pole_world = None;
                memory.left_terrain_end_direction = None;
                memory.right_terrain_end_direction = None;
                memory.left_foot_orientation_world = None;
                memory.right_foot_orientation_world = None;
                memory.left_contact_orientation_blend_active = false;
                memory.right_contact_orientation_blend_active = false;
                clear_slope_rotation_cache(&mut memory);
                memory.left_release_active = false;
                memory.right_release_active = false;
                memory.left_release_target = None;
                memory.right_release_target = None;
                memory.pelvis_shift = 0.0;
                memory.measured_owner_planar_speed = 0.0;
                reset_terminal_settle_reach(&mut memory);
                memory.recent_movement_velocity = Vec3::ZERO;
                memory.settle = None;
            }
            memory.rig_origin = Some(rig_origin);
            memory.rig_rotation = Some(rig_rotation);
        }
        if raised_footwork_handoff {
            // A mode that owns the full lower body supersedes guard footwork.
            // Preserve both last visible targets as the beginning of a bounded
            // balance capture instead of reacquiring authored gait feet.
            if let Ok(mut raised) = raised_states.get_mut(owner) {
                preserve_raised_handoff_targets(&mut memory, *raised, rig_origin, rig_rotation);
                *raised = RaisedFootworkState::default();
            }
        }
        let ordinary_lowered = skeleton.weapon_guard() == WeaponGuardState::Lowered
            && skeleton.action_kind() == SkeletonAction::None;
        let planar_velocity = skeleton.world_velocity.with_y(0.0);
        if ordinary_lowered && skeleton.animation_speed() > 0.05 && memory.settle.is_none() {
            // Retain the strongest recent velocity through the presentation
            // deceleration. Sampling only the final sub-threshold frame makes
            // the capture point collapse behind the body's visible momentum.
            if planar_velocity.length_squared()
                >= memory.recent_movement_velocity.length_squared() * 0.25
            {
                memory.recent_movement_velocity = planar_velocity;
            }
        } else if (ordinary_lowered || raised_footwork_handoff)
            && skeleton.animation_speed() <= 0.05
            && memory.settle.is_none()
        {
            let projected_com = projected_body_center(rig, &transforms.p0()).unwrap_or(rig_origin);
            let has_recent_velocity = memory.recent_movement_velocity.length_squared() > 0.0025;
            let stance_known =
                memory.left_foot_world_target.is_some() && memory.right_foot_world_target.is_some();
            let stance_safe = stance_known
                && terrain.is_some_and(|terrain| {
                    settle_stance_is_safe(
                        projected_com,
                        memory.left_foot_world_target,
                        memory.right_foot_world_target,
                        terrain,
                    )
                });
            let should_begin_settle =
                raised_footwork_handoff || has_recent_velocity || (stance_known && !stance_safe);
            if should_begin_settle {
                let rendered_left = rig.get(&BoneRole::FootLeft).and_then(|&foot| {
                    transforms
                        .p0()
                        .compute_global_transform(foot)
                        .ok()
                        .map(|global| global.translation())
                });
                let rendered_right = rig.get(&BoneRole::FootRight).and_then(|&foot| {
                    transforms
                        .p0()
                        .compute_global_transform(foot)
                        .ok()
                        .map(|global| global.translation())
                });
                let visible_left =
                    settle_visible_foot(memory.left_last_rendered_world, rendered_left);
                let visible_right =
                    settle_visible_foot(memory.right_last_rendered_world, rendered_right);
                let direction = if has_recent_velocity {
                    memory.recent_movement_velocity.normalize_or_zero()
                } else {
                    balance_recovery_direction(
                        projected_com,
                        memory.left_foot_world_target,
                        memory.right_foot_world_target,
                        rig_rotation * Vec3::NEG_Z,
                    )
                };
                let capture_point = if has_recent_velocity {
                    projected_capture_point(
                        projected_com,
                        memory.recent_movement_velocity.clamp_length_max(
                            ik_tuning().maximum_settle_capture_speed_metres_per_second,
                        ),
                        ik_tuning().assumed_center_of_mass_height_metres,
                    )
                } else {
                    projected_com
                };
                let support_left = choose_settle_support(
                    memory.left_support_weight,
                    memory.right_support_weight,
                    visible_left,
                    visible_right,
                    projected_com,
                    direction,
                );
                // A visible ankle is not necessarily a planted ankle: a stop
                // can begin during Run flight. Preserve truthful propagated
                // contact ownership separately from the visible world target
                // so an airborne selected support keeps its toe/sole floor
                // until it actually acquires terrain.
                let selected_support_was_acquired = if support_left {
                    memory.left_foot_plant_acquired
                        && memory
                            .left_support_weight
                            .is_some_and(terrain_leg_has_support)
                } else {
                    memory.right_foot_plant_acquired
                        && memory
                            .right_support_weight
                            .is_some_and(terrain_leg_has_support)
                };
                let swing_start = if support_left {
                    visible_right
                } else {
                    visible_left
                }
                .unwrap_or(rig_origin);
                let side = settle_swing_side(
                    rig_origin,
                    rig_rotation,
                    swing_start,
                    if support_left { 1.0 } else { -1.0 },
                );
                let landing_target =
                    plan_settle_landing(rig_origin, rig_rotation, capture_point, direction, side);
                // World-target memory may be ahead of a reach-constrained
                // rendered foot. Seed both settle chains from the visible pose
                // so neither the chosen support nor swing can teleport to that
                // invisible goal on the next zero-speed sample.
                seed_settle_from_rendered_feet(
                    &mut memory,
                    visible_left,
                    visible_right,
                    rig_origin,
                    rig_rotation,
                );
                // The selected support is already visibly on screen. Retain
                // that exact world footprint while the opposite foot captures
                // balance; reacquiring it from restored FK or an old run plan
                // can move it several decimetres on the second stop sample.
                retain_settle_support(
                    &mut memory,
                    support_left,
                    visible_left,
                    visible_right,
                    selected_support_was_acquired,
                );
                memory.settle = Some(LocomotionSettleState {
                    support_left,
                    swing_start,
                    capture_point,
                    landing_target,
                    progress: 0.0,
                    elapsed_seconds: 0.0,
                    raised_handoff: raised_footwork_handoff,
                });
            }
        }
        let settle_cancelled_for_restart =
            ordinary_lowered && skeleton.animation_speed() > 0.05 && memory.settle.is_some();
        if settle_cancelled_for_restart {
            // A restart invalidates the balance-capture trajectory immediately.
            // Keeping a cancelled settle alive until both release targets
            // converged could starve ordinary gait acquisition indefinitely:
            // the authored swing kept moving, so neither release ever became
            // idle. Retain the already bounded visible targets, but return
            // ownership to ordinary phase/contact planning on this tick.
            cancel_settle_for_restart(&mut memory, planar_velocity);
        }
        let mut settle_ready_for_contact = false;
        if let Some(mut settle) = memory.settle {
            if state_delta_seconds > 0.0 {
                settle = advance_settle_state(settle, state_delta_seconds);
            }
            settle_ready_for_contact = settle.progress >= 1.0;
            if settle.support_left {
                left_weight = 1.0;
                right_weight = 0.0;
            } else {
                left_weight = 0.0;
                right_weight = 1.0;
            }
            legs[0].3 = left_weight;
            legs[1].3 = right_weight;
            memory.settle = Some(settle);
        }
        // A compact combat stance needs real leg reach reserve. Without this
        // shared root drop the authored near-straight legs exhaust their reach
        // after only a few centimetres of lateral root travel, forcing the
        // support foot to slide. This moves the body, never either contact.
        if state_delta_seconds > 0.0 {
            let desired = if raised_guard_follower {
                -ik_tuning().guard_reach_pelvis_drop_metres
            } else {
                0.0
            };
            memory.raised_pelvis_shift =
                advance_pelvis_shift(memory.raised_pelvis_shift, desired, state_delta_seconds);
        }
        if raised_guard_follower {
            prepare_slope_rotation_cache(&mut memory, SlopeAlignmentMode::Raised);
            if memory.raised_pelvis_shift < -0.001
                && let Some(&root) = rig.get(&BoneRole::Root)
            {
                let local_delta = parents
                    .get(root)
                    .ok()
                    .and_then(|parent| {
                        transforms
                            .p0()
                            .compute_global_transform(parent.parent())
                            .ok()
                    })
                    .map(|parent| {
                        parent
                            .affine()
                            .inverse()
                            .transform_vector3(Vec3::Y * memory.raised_pelvis_shift)
                    })
                    .unwrap_or(Vec3::Y * memory.raised_pelvis_shift);
                if local_delta.is_finite()
                    && let Ok(mut transform) = transforms.p1().get_mut(root)
                {
                    transform.translation += local_delta;
                }
            }
            // Retained plants may request the minimum pelvis correction needed
            // for reach, but the authored guard stance owns the resting height.
            let left = (
                rig.get(&BoneRole::ThighLeft),
                rig.get(&BoneRole::ShinLeft),
                rig.get(&BoneRole::FootLeft),
            );
            let right = (
                rig.get(&BoneRole::ThighRight),
                rig.get(&BoneRole::ShinRight),
                rig.get(&BoneRole::FootRight),
            );
            let (Some(&left_upper), Some(&left_lower), Some(&left_foot)) = left else {
                continue;
            };
            let (Some(&right_upper), Some(&right_lower), Some(&right_foot)) = right else {
                continue;
            };
            let Some((left_upper_snapshot, left_lower_snapshot, left_foot_snapshot)) =
                snapshot_chain(
                    left_upper,
                    left_lower,
                    left_foot,
                    &parents,
                    &transforms.p0(),
                )
            else {
                continue;
            };
            let Some((right_upper_snapshot, right_lower_snapshot, right_foot_snapshot)) =
                snapshot_chain(
                    right_upper,
                    right_lower,
                    right_foot,
                    &parents,
                    &transforms.p0(),
                )
            else {
                continue;
            };
            let left_upper_length = left_upper_snapshot
                .global
                .translation()
                .distance(left_lower_snapshot.global.translation());
            let left_lower_length = left_lower_snapshot
                .global
                .translation()
                .distance(left_foot_snapshot.global.translation());
            let right_upper_length = right_upper_snapshot
                .global
                .translation()
                .distance(right_lower_snapshot.global.translation());
            let right_lower_length = right_lower_snapshot
                .global
                .translation()
                .distance(right_foot_snapshot.global.translation());
            let mut footwork = raised_states
                .get_mut(owner)
                .map(|state| *state)
                .unwrap_or_default();
            let previous_swing = footwork.step.swing_foot();
            let left_authored = left_foot_snapshot.global.translation();
            let right_authored = right_foot_snapshot.global.translation();
            let live_speed = skeleton.world_velocity.with_y(0.0).length();
            let handoff_targets = memory.quickstep_handoff.targets();
            let visible_left = if let Some((landing_local, _)) = handoff_targets {
                let authored_local = rig_rotation.inverse() * (left_authored - rig_origin);
                let landing_local =
                    landing_local + (authored_local - landing_local).clamp_length_max(0.015);
                rig_origin + rig_rotation * landing_local
            } else {
                memory.left_foot_world_target.unwrap_or(left_authored)
            };
            let visible_right = if let Some((_, landing_local)) = handoff_targets {
                let authored_local = rig_rotation.inverse() * (right_authored - rig_origin);
                let landing_local =
                    landing_local + (authored_local - landing_local).clamp_length_max(0.015);
                rig_origin + rig_rotation * landing_local
            } else {
                memory.right_foot_world_target.unwrap_or(right_authored)
            };
            if memory.quickstep_handoff.is_pending() {
                memory.quickstep_handoff.update_targets(
                    rig_rotation.inverse() * (visible_left - rig_origin),
                    rig_rotation.inverse() * (visible_right - rig_origin),
                );
                memory.left_foot_world_target = Some(visible_left);
                memory.right_foot_world_target = Some(visible_right);
                memory.left_foot_plant = Some(visible_left);
                memory.right_foot_plant = Some(visible_right);
                memory.left_foot_plant_acquired = true;
                memory.right_foot_plant_acquired = true;
            }
            let quickstep_handoff_active = memory.quickstep_handoff.is_pending();
            if memory.quickstep_handoff.is_held() && live_speed > 0.05 {
                memory.quickstep_handoff = QuickstepContactHandoff::None;
            }
            if quickstep_handoff_active {
                // Preserve the airborne pose through impact while the legs
                // converge to their authored guard length. Once converged, the
                // ordinary follower is free to step for any residual velocity.
                footwork.step = GuardStepState::Stationary {
                    left: visible_left,
                    right: visible_right,
                    next: opposite_guard_foot(skeleton.contact_foot),
                };
            }

            if !quickstep_handoff_active {
                let contact_world = |contact: Vec2, authored: Vec3| {
                    let authored_local = rig_rotation.inverse() * (authored - rig_origin);
                    let target = rig_origin
                        + rig_rotation * Vec3::new(contact.x, authored_local.y, contact.y);
                    if enabled.0 {
                        terrain.map_or(target, |terrain| {
                            terrain_conformed_guard_target(target, terrain.height_at(target.xz()))
                        })
                    } else {
                        target
                    }
                };
                let requested = match skeleton.raised_footwork() {
                    GuardFootworkPlan::Uninitialized => GuardStepState::Stationary {
                        left: visible_left,
                        right: visible_right,
                        next: opposite_guard_foot(skeleton.contact_foot),
                    },
                    GuardFootworkPlan::Planted {
                        contacts,
                        next_swing,
                    } => {
                        let authored_contact = |authored: Vec3| {
                            let local = rig_rotation.inverse() * (authored - rig_origin);
                            Vec2::new(local.x, local.z)
                        };
                        let (left_contact, right_contact) = if live_speed <= 0.05 {
                            // Once translation has stopped, the prepared combat
                            // stance is the canonical footprint. Replicated tap
                            // contacts describe the last moving sample and can
                            // otherwise leave both feet collapsed on one side.
                            (
                                authored_contact(left_authored),
                                authored_contact(right_authored),
                            )
                        } else {
                            (contacts.left(), contacts.right())
                        };
                        GuardStepState::Stationary {
                            left: contact_world(left_contact, left_authored),
                            right: contact_world(right_contact, right_authored),
                            next: next_swing,
                        }
                    }
                    GuardFootworkPlan::Stepping(step) => {
                        let contacts = step.contacts();
                        let progress = match step.swing_foot() {
                            LeadFoot::Left => (skeleton.gait_phase - 0.5).rem_euclid(1.0) * 2.0,
                            LeadFoot::Right => skeleton.gait_phase * 2.0,
                        }
                        .clamp(0.0, 1.0);
                        match step.swing_foot() {
                            LeadFoot::Left => GuardStepState::LeftSwing {
                                right_support: contact_world(contacts.right(), right_authored),
                                left: GuardSwing {
                                    start: contact_world(step.swing_start(), left_authored),
                                    end: contact_world(step.landing(), left_authored),
                                    progress,
                                },
                            },
                            LeadFoot::Right => GuardStepState::RightSwing {
                                left_support: contact_world(contacts.left(), left_authored),
                                right: GuardSwing {
                                    start: contact_world(step.swing_start(), right_authored),
                                    end: contact_world(step.landing(), right_authored),
                                    progress,
                                },
                            },
                        }
                    }
                };
                if live_speed <= 0.05
                    && !footwork.step.initialized()
                    && let (Some(left), Some(right)) = (
                        memory.left_last_rendered_world,
                        memory.right_last_rendered_world,
                    )
                {
                    let fallback = opposite_guard_foot(skeleton.contact_foot);
                    let next = match requested {
                        GuardStepState::Stationary {
                            left: desired_left,
                            right: desired_right,
                            ..
                        } => safer_guard_reacquire_foot(
                            left,
                            right,
                            desired_left,
                            desired_right,
                            fallback,
                        ),
                        _ => fallback,
                    };
                    // Reacquire from the final authored locomotion pose.
                    // The stationary stepper can then move one foot at a
                    // time toward the guard contacts without a one-frame
                    // ownership snap or contact-identity reset.
                    footwork.step = GuardStepState::Stationary { left, right, next };
                }
                footwork.step = if live_speed <= 0.05
                    && matches!(requested, GuardStepState::Stationary { .. })
                {
                    advance_stationary_turn_step(footwork.step, requested, state_delta_seconds)
                } else {
                    requested
                };
            }
            let next_swing = footwork.step.swing_foot();
            if previous_swing.is_some() && previous_swing != next_swing {
                // Match the authoritative contact-sequence convention: the
                // sequence advances when the airborne foot lands (or a
                // skipped presentation sample lands and launches the other
                // foot), not when toe-off begins.
                footwork.step_sequence = footwork.step_sequence.wrapping_add(1);
            }

            let mut left_target;
            let mut right_target;
            match footwork.step {
                GuardStepState::Uninitialized => unreachable!("guard state was initialized above"),
                GuardStepState::Stationary { left, right, .. } => {
                    left_target = left;
                    right_target = right;
                }
                GuardStepState::LeftSwing {
                    right_support,
                    left,
                } => {
                    left_target = guard_swing_target(left);
                    right_target = right_support;
                }
                GuardStepState::RightSwing {
                    left_support,
                    right,
                } => {
                    left_target = left_support;
                    right_target = guard_swing_target(right);
                }
            }

            // Contact safety outranks the swing interpolation profile. If the
            // body catches the visual swing, advance that foot immediately;
            // its authoritative landing already reserves room ahead.
            let hip_center = (left_upper_snapshot.global.translation()
                + right_upper_snapshot.global.translation())
                * 0.5;
            if let Some(step) = skeleton.raised_footwork().step() {
                let local_direction = step.direction();
                let planned_direction = (rig_rotation
                    * Vec3::new(local_direction.x, 0.0, local_direction.y))
                .with_y(0.0)
                .normalize_or_zero();
                let physical_direction = skeleton.world_velocity.with_y(0.0).normalize_or_zero();
                let direction = if physical_direction == Vec3::ZERO {
                    planned_direction
                } else {
                    physical_direction
                };
                if direction != Vec3::ZERO {
                    let support = match step.swing_foot() {
                        LeadFoot::Left => right_target,
                        LeadFoot::Right => left_target,
                    };
                    let swing = match step.swing_foot() {
                        LeadFoot::Left => &mut left_target,
                        LeadFoot::Right => &mut right_target,
                    };
                    let shortfall = hip_center.dot(direction)
                        - support.dot(direction).max(swing.dot(direction));
                    if shortfall > 0.0 {
                        *swing += direction * shortfall;
                    }
                }
            }

            let Some(validated) = validate_guard_frame_targets(
                GuardTargetRequest {
                    left: left_target,
                    right: right_target,
                },
                [
                    GuardLegGeometry {
                        hip: left_upper_snapshot.global.translation(),
                        maximum_reach: left_upper_length + left_lower_length - 0.0001,
                    },
                    GuardLegGeometry {
                        hip: right_upper_snapshot.global.translation(),
                        maximum_reach: right_upper_length + right_lower_length - 0.0001,
                    },
                ],
                footwork.step.swing_foot(),
            ) else {
                // This can only be reached for malformed/non-finite rig data.
                // Preserve the contact state: resetting it here would turn a
                // bad sample into a permanent sliding-foot failure.
                if let Ok(mut state) = raised_states.get_mut(owner) {
                    *state = footwork;
                } else {
                    commands.entity(owner).insert(footwork);
                }
                if let Ok(mut state) = ik_states.get_mut(owner) {
                    state.0 = memory;
                } else {
                    commands.entity(owner).insert(LegIkState(memory));
                }
                continue;
            };
            left_target = validated.left();
            right_target = validated.right();
            let _adjusted_for_reach = validated.adjusted_for_reach();
            let (left_nominal_support, right_nominal_support) = match footwork.step {
                GuardStepState::LeftSwing { .. } => (false, true),
                GuardStepState::RightSwing { .. } => (true, false),
                GuardStepState::Uninitialized | GuardStepState::Stationary { .. } => (true, true),
            };
            let quickstep_handoff_converged =
                memory
                    .quickstep_handoff
                    .targets()
                    .is_some_and(|(left, right)| {
                        let authored_left = rig_rotation.inverse() * (left_authored - rig_origin);
                        let authored_right = rig_rotation.inverse() * (right_authored - rig_origin);
                        left.distance(authored_left) <= 0.001
                            && right.distance(authored_right) <= 0.001
                    });
            if memory.quickstep_handoff.is_pending() && quickstep_handoff_converged {
                memory.quickstep_handoff.hold();
            }

            let mut airborne_orientation_owned = [true; 2];
            for (leg_index, (upper, lower, foot, target, left, support)) in [
                (
                    left_upper,
                    left_lower,
                    left_foot,
                    left_target,
                    true,
                    left_nominal_support,
                ),
                (
                    right_upper,
                    right_lower,
                    right_foot,
                    right_target,
                    false,
                    right_nominal_support,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
                    snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
                else {
                    continue;
                };
                let upper_length = upper_snapshot
                    .global
                    .translation()
                    .distance(lower_snapshot.global.translation());
                let lower_length = lower_snapshot
                    .global
                    .translation()
                    .distance(foot_snapshot.global.translation());
                let side = anatomical_side(
                    rig_rotation,
                    rig_origin,
                    upper_snapshot.global.translation(),
                    left,
                );
                let quickstep_bend = if left {
                    memory.left_leg
                } else {
                    memory.right_leg
                }
                .map(|bend| pole_to_world(rig_rotation, bend));
                let remembered = if quickstep_handoff_active {
                    quickstep_bend
                } else if left {
                    footwork.left_knee_bend_world.or(quickstep_bend)
                } else {
                    footwork.right_knee_bend_world.or(quickstep_bend)
                };
                let previous_end_direction = if left {
                    footwork.left_end_direction
                } else {
                    footwork.right_end_direction
                };
                let canonical_pole = canonical_knee_pole(side);
                let canonical_world = pole_to_world(rig_rotation, canonical_pole);
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
                    canonical_world,
                    foot_facing,
                )
                .unwrap_or(canonical_world);
                let pole = constrain_rendered_leg_pole(
                    rig,
                    left,
                    upper_snapshot.global.translation(),
                    foot_snapshot.global.translation(),
                    target,
                    pole,
                    &parents,
                    &transforms.p0(),
                );
                if let Some(solution) = solve_two_bone_with_reach(
                    TwoBoneChain::new(
                        upper_snapshot.global.translation(),
                        lower_snapshot.global.translation(),
                        foot_snapshot.global.translation(),
                        upper_length,
                        lower_length,
                        pole,
                    ),
                    target,
                    if support {
                        upper_length + lower_length - 0.0001
                    } else {
                        maximum_reach(upper_length, lower_length)
                    },
                ) {
                    apply_two_bone_solution(
                        upper,
                        lower,
                        foot,
                        solution,
                        &parents,
                        &mut transforms,
                    );
                    let bend = (solution.knee - upper_snapshot.global.translation())
                        .reject_from_normalized(solution.end_direction);
                    if state_delta_seconds > 0.0
                        && let Some(valid) = bend.try_normalize()
                    {
                        if left {
                            memory.left_leg = Some(pole_to_owner(rig_rotation, valid));
                            footwork.left_knee_bend_world = Some(valid);
                            footwork.left_end_direction = Some(solution.end_direction);
                        } else {
                            memory.right_leg = Some(pole_to_owner(rig_rotation, valid));
                            footwork.right_knee_bend_world = Some(valid);
                            footwork.right_end_direction = Some(solution.end_direction);
                        }
                    }
                }
                let rendered_ankle = snapshot(foot, &parents, &transforms.p0())
                    .map(|rendered| rendered.global.translation());
                let reported_support = if enabled.0 {
                    rendered_ankle.is_some_and(|ankle| {
                        terrain
                            .and_then(|terrain| terrain.height_at(ankle.xz()))
                            .is_some_and(|height| {
                                raised_support_is_actual(support, ankle.y, height)
                            })
                    })
                } else {
                    // Without terrain conformity the raised-footwork solver
                    // owns a flat semantic plant, not a sampled world-surface
                    // contact. Preserve that ownership for cadence telemetry;
                    // terrain-enabled poses still require rendered sole contact.
                    support
                };
                airborne_orientation_owned[leg_index] = !reported_support;
                if enabled.0
                    && reported_support
                    && let Some(terrain) = terrain
                    && let Some(normal) = terrain.normal_at(target.xz())
                    && let Some(sole_axis) = rig.sole_axis(left)
                {
                    let cached_chain = if left {
                        memory.left_rotation_chain
                    } else {
                        memory.right_rotation_chain
                    };
                    if evaluation_advances || cached_chain.is_none() {
                        align_foot_to_slope(foot, sole_axis, normal, &parents, &mut transforms);
                    }
                }
                if left {
                    if evaluation_advances {
                        memory.left_contact_orientation_blend_active =
                            update_contact_orientation_blend(
                                memory.left_contact_orientation_blend_active,
                                memory.left_support_weight,
                                reported_support as u8 as f32,
                            );
                    }
                    let visible_target = rendered_ankle.unwrap_or(target);
                    footwork.left_solve_target = Some(visible_target);
                    memory.left_foot_world_target = Some(visible_target);
                    memory.left_support_weight = Some(reported_support as u8 as f32);
                    footwork.left_support_weight = reported_support as u8 as f32;
                } else {
                    if evaluation_advances {
                        memory.right_contact_orientation_blend_active =
                            update_contact_orientation_blend(
                                memory.right_contact_orientation_blend_active,
                                memory.right_support_weight,
                                reported_support as u8 as f32,
                            );
                    }
                    let visible_target = rendered_ankle.unwrap_or(target);
                    footwork.right_solve_target = Some(visible_target);
                    memory.right_foot_world_target = Some(visible_target);
                    memory.right_support_weight = Some(reported_support as u8 as f32);
                    footwork.right_support_weight = reported_support as u8 as f32;
                }
            }
            let raised_orientation_handoff = [
                memory.left_foot_orientation_world.is_none()
                    && memory.left_last_rendered_foot_rotation_world.is_some(),
                memory.right_foot_orientation_world.is_none()
                    && memory.right_last_rendered_foot_rotation_world.is_some(),
            ];
            finalize_leg_rotation_chains(
                rig,
                skeleton,
                rig_rotation,
                &mut memory,
                evaluation_advances,
                state_delta_seconds,
                airborne_orientation_owned,
                raised_orientation_handoff,
                &parents,
                &mut transforms,
            );
            // Classify support and retain handoff targets only after the final
            // cached-chain/orientation seam. This is the same local-transform
            // state that transform propagation exposes to viewer telemetry.
            for (foot, left, nominal_support) in [
                (left_foot, true, left_nominal_support),
                (right_foot, false, right_nominal_support),
            ] {
                let Some(rendered) = snapshot(foot, &parents, &transforms.p0()) else {
                    continue;
                };
                let ankle = rendered.global.translation();
                let reported_support = if enabled.0 {
                    terrain
                        .and_then(|terrain| terrain.height_at(ankle.xz()))
                        .is_some_and(|height| {
                            raised_support_is_actual(nominal_support, ankle.y, height)
                        })
                } else {
                    nominal_support
                };
                if left {
                    footwork.left_solve_target = Some(ankle);
                    footwork.left_support_weight = reported_support as u8 as f32;
                    memory.left_foot_world_target = Some(ankle);
                    memory.left_support_weight = Some(reported_support as u8 as f32);
                } else {
                    footwork.right_solve_target = Some(ankle);
                    footwork.right_support_weight = reported_support as u8 as f32;
                    memory.right_foot_world_target = Some(ankle);
                    memory.right_support_weight = Some(reported_support as u8 as f32);
                }
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = footwork;
            } else {
                commands.entity(owner).insert(footwork);
            }
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = memory;
            } else {
                commands.entity(owner).insert(LegIkState(memory));
            }
            continue;
        }

        if !enabled.0
            && terrain_blend <= 0.001
            && memory.settle.is_none()
            && !memory.left_release_active
            && !memory.right_release_active
        {
            // Once the bounded release finishes, clear leg targets so a later
            // re-enable cannot resurrect stale plants. Arm pole continuity is
            // unrelated.
            memory.left_foot_plant = None;
            memory.right_foot_plant = None;
            memory.left_foot_plant_acquired = false;
            memory.right_foot_plant_acquired = false;
            memory.left_foot_target = None;
            memory.right_foot_target = None;
            memory.left_foot_world_target = None;
            memory.right_foot_world_target = None;
            memory.left_last_rendered_world = None;
            memory.right_last_rendered_world = None;
            memory.left_last_rendered_toe_world = None;
            memory.right_last_rendered_toe_world = None;
            memory.left_last_rendered_owner = None;
            memory.right_last_rendered_owner = None;
            memory.left_last_rendered_foot_rotation_world = None;
            memory.right_last_rendered_foot_rotation_world = None;
            memory.left_authored_world_target = None;
            memory.right_authored_world_target = None;
            memory.left_planned_contact_start = None;
            memory.right_planned_contact_start = None;
            memory.left_planned_contact_phase_start = None;
            memory.right_planned_contact_phase_start = None;
            memory.left_support_weight = None;
            memory.right_support_weight = None;
            memory.left_transition_support_weight = None;
            memory.right_transition_support_weight = None;
            memory.left_support_exhausted_until_flight = false;
            memory.right_support_exhausted_until_flight = false;
            memory.left_terrain_pole_world = None;
            memory.right_terrain_pole_world = None;
            memory.left_terrain_end_direction = None;
            memory.right_terrain_end_direction = None;
            memory.left_foot_orientation_world = None;
            memory.right_foot_orientation_world = None;
            memory.left_contact_orientation_blend_active = false;
            memory.right_contact_orientation_blend_active = false;
            clear_slope_rotation_cache(&mut memory);
            memory.left_release_active = false;
            memory.right_release_active = false;
            memory.left_release_target = None;
            memory.right_release_target = None;
            memory.pelvis_shift = 0.0;
            memory.measured_owner_planar_speed = 0.0;
            reset_terminal_settle_reach(&mut memory);
            memory.recent_movement_velocity = Vec3::ZERO;
            memory.settle = None;
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = memory;
            } else {
                commands.entity(owner).insert(LegIkState(memory));
            }
            continue;
        }
        let Some(terrain) = terrain else {
            continue;
        };
        prepare_slope_rotation_cache(&mut memory, SlopeAlignmentMode::Ordinary);
        let mut desired_hip_shift = 0.0_f32;
        let mut settle_contact_reached = false;
        for (upper_role, lower_role, foot_role, weight, left) in legs {
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
            let position = foot_snapshot.global.translation();
            if let Some(height) = terrain.height_at(position.xz()) {
                let desired_ankle = height + measured_ankle_sole_offset_metres();
                desired_hip_shift = desired_hip_shift
                    .min(((desired_ankle - position.y) * weight).clamp(-0.18, 0.0));
            }
            let plant = if left {
                memory.left_foot_plant
            } else {
                memory.right_foot_plant
            };
            let planned = if left {
                memory.left_planned_contact
            } else {
                memory.right_planned_contact
            };
            let acquired = if left {
                memory.left_foot_plant_acquired
            } else {
                memory.right_foot_plant_acquired
            };
            let planned_phase_start = if left {
                memory.left_planned_contact_phase_start
            } else {
                memory.right_planned_contact_phase_start
            };
            let run = locomotion_profile(skeleton).gait == LocomotionGait::Run;
            let planned_weight = if run && !acquired && (plant.is_some() || planned.is_some()) {
                let phase_to_contact = phase_to_next_contact(skeleton.gait_phase, left);
                run_contact_approach_progress(
                    phase_to_contact,
                    planned_phase_start.unwrap_or(ik_tuning().run_contact_approach_phase),
                    locomotion_profile(skeleton).support_phase_radius
                        + ik_tuning().run_contact_chain_settle_phase,
                )
            } else {
                0.0
            };
            let Some(horizontal_target) = plant.or(planned) else {
                continue;
            };
            let reach_weight = if settle_is_terminal(&memory) && plant.is_some() {
                // A completed stop owns both final contacts. Idle has no raw
                // gait support weight, but the shared rig root must continue
                // descending until both analytic chains can actually reach
                // those contacts; otherwise one leg can remain frozen above
                // the ground forever with settle progress pinned at one.
                1.0
            } else if plant.is_some() {
                weight.max(planned_weight)
            } else {
                planned_weight
            };
            // A remembered plant is world-space. Do not reproject it through
            // the rotating/moving anatomical corridor every frame: that made
            // a visibly planted foot skate during turns. New contacts are
            // constrained when acquired, and reach limiting below remains the
            // only reason an established plant may yield.
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            let target_y = height + measured_ankle_sole_offset_metres();
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot
                .global
                .translation()
                .distance(foot_snapshot.global.translation());
            let reach = terrain_maximum_reach(upper_length, lower_length);
            let reach_shift = required_hip_shift_for_reach(
                upper_snapshot.global.translation(),
                horizontal_target.with_y(target_y),
                reach,
            );
            desired_hip_shift =
                desired_hip_shift.min((reach_shift * reach_weight).clamp(-0.25, 0.0));
        }
        desired_hip_shift *= terrain_blend;
        if locomotion_profile(skeleton).gait == LocomotionGait::Run {
            // Anticipate only the reach needed by the frozen run contact. The
            // bounded contact-phase drop reinforces the two existing minima;
            // it cannot add the earlier free-running terrain wave or move the
            // authoritative controller.
            desired_hip_shift = desired_hip_shift.clamp(
                -ik_tuning().run_maximum_planned_reach_pelvis_drop_metres,
                0.0,
            );
        }
        let terminal_root_correction =
            settle_is_terminal(&memory) && memory.terminal_contacts_prepared;
        if terminal_root_correction {
            // The idle clip is sparse and can leave the procedural rig-root
            // translation in place. Capture one local baseline and apply the
            // terminal reach correction absolutely from it; repeatedly adding
            // the retained pelvis scalar reaches a false halfway equilibrium.
            let target_shift = *memory
                .terminal_reach_target_shift
                .get_or_insert(desired_hip_shift.clamp(-0.25, 0.0));
            if state_delta_seconds > 0.0 {
                memory.terminal_reach_shift = advance_pelvis_shift(
                    memory.terminal_reach_shift,
                    target_shift,
                    state_delta_seconds,
                );
            }
        } else {
            // Couple both legs through one bounded, continuous pelvis
            // correction during ordinary locomotion.
            if memory_was_missing {
                memory.pelvis_shift = desired_hip_shift;
            } else if state_delta_seconds > 0.0 {
                memory.pelvis_shift = if locomotion_profile(skeleton).gait == LocomotionGait::Run {
                    advance_scalar_at_speed(
                        memory.pelvis_shift,
                        desired_hip_shift,
                        state_delta_seconds,
                        ik_tuning().run_pelvis_correction_speed_metres_per_second,
                    )
                } else {
                    advance_pelvis_shift(
                        memory.pelvis_shift,
                        desired_hip_shift,
                        state_delta_seconds,
                    )
                };
            }
        }
        let hip_shift = if terminal_root_correction {
            memory.terminal_reach_shift
        } else {
            memory.pelvis_shift
        };
        if hip_shift < -0.001 {
            // MHR's anatomical `root` owns the spine and both thigh chains.
            // Correct its `body_world` parent so every cached knee pole and
            // local chain sees one coherent transform. Translating the pelvis
            // and two thigh locals independently inverted the knee hemisphere.
            if let Some(&bone) = rig.get(&BoneRole::Root) {
                let local_delta = parents
                    .get(bone)
                    .ok()
                    .and_then(|parent| {
                        transforms
                            .p0()
                            .compute_global_transform(parent.parent())
                            .ok()
                    })
                    .map(|parent| {
                        parent
                            .affine()
                            .inverse()
                            .transform_vector3(Vec3::Y * hip_shift)
                    })
                    .unwrap_or(Vec3::Y * hip_shift);
                if local_delta.is_finite()
                    && let Ok(mut transform) = transforms.p1().get_mut(bone)
                {
                    if terminal_root_correction {
                        let base = *memory
                            .terminal_root_base_translation
                            .get_or_insert(transform.translation);
                        transform.translation = base + local_delta;
                    } else {
                        transform.translation += local_delta;
                    }
                }
            }
        }
        let mut airborne_orientation_owned = [false; 2];
        let mut airborne_just_released = [false; 2];
        for (leg_index, (upper_role, lower_role, foot_role, weight, left)) in
            legs.into_iter().enumerate()
        {
            let mut weight = weight;
            let settle_support_owned = memory
                .settle
                .is_some_and(|settle| settle.support_left == left);
            if settle_support_owned {
                // The chosen settle support owns a retained footprint even if
                // stop began in flight. Keep that logical solve path stable
                // while its follower approaches contact; allowing the raw
                // gait lobe to drop to zero routes it through ordinary swing,
                // discards the toe floor, and prevents acquisition.
                weight = 1.0;
            }
            let terminal_contact_owned =
                settle_is_terminal(&memory) && memory.terminal_contacts_prepared;
            let raw_nominal_weight = raw_run_support
                .map(|(left_raw, right_raw)| if left { left_raw } else { right_raw })
                .unwrap_or(weight);
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
            let foot_position = foot_snapshot.global.translation();
            let toe_position = rig
                .get(if left {
                    &BoneRole::ToeLeft
                } else {
                    &BoneRole::ToeRight
                })
                .and_then(|toe| snapshot(*toe, &parents, &transforms.p0()))
                .map(|snapshot| snapshot.global.translation());
            let rendered_ankle_and_toe = if left {
                memory
                    .left_last_rendered_world
                    .zip(memory.left_last_rendered_toe_world)
            } else {
                memory
                    .right_last_rendered_world
                    .zip(memory.right_last_rendered_toe_world)
            }
            .or_else(|| toe_position.map(|toe| (foot_position, toe)));
            if left {
                memory.left_authored_world_target = Some(foot_position);
            } else {
                memory.right_authored_world_target = Some(foot_position);
            }
            let mut plant = if left {
                memory.left_foot_plant
            } else {
                memory.right_foot_plant
            };
            let settle_support_plant = settle_support_owned.then_some(plant).flatten();
            let side = anatomical_side(
                rig_rotation,
                rig_origin,
                upper_snapshot.global.translation(),
                left,
            );
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot.global.translation().distance(foot_position);
            if terminal_contact_owned {
                // Terminal dual-contact ownership is a dedicated state. It
                // must bypass every ordinary phase, discontinuity, reach-
                // release, track-constrain, and replan mutation below: those
                // transitions can rewrite a frozen idle contact one sample
                // before completion and snap the rendered chain.
                let (logical_weight, terminal_plant) =
                    terminal_contact_solve_ownership(true, weight, plant);
                let Some(frozen_plant) = terminal_plant else {
                    continue;
                };
                let Some(height) = terrain.height_at(frozen_plant.xz()) else {
                    continue;
                };
                let target = frozen_plant.with_y(height + measured_ankle_sole_offset_metres());
                let canonical_world = pole_to_world(rig_rotation, canonical_knee_pole(side));
                let (remembered_pole, previous_end_direction) = if left {
                    (
                        memory.left_terrain_pole_world,
                        memory.left_terrain_end_direction,
                    )
                } else {
                    (
                        memory.right_terrain_pole_world,
                        memory.right_terrain_end_direction,
                    )
                };
                let next_end_direction =
                    (target - upper_snapshot.global.translation()).normalize_or_zero();
                let pole = transported_terrain_pole(
                    remembered_pole,
                    previous_end_direction,
                    next_end_direction,
                    canonical_world,
                )
                .unwrap_or(canonical_world);
                let pole = constrain_rendered_leg_pole(
                    rig,
                    left,
                    upper_snapshot.global.translation(),
                    foot_position,
                    target,
                    pole,
                    &parents,
                    &transforms.p0(),
                );
                if let Some(solution) = solve_two_bone_with_reach(
                    TwoBoneChain::new(
                        upper_snapshot.global.translation(),
                        lower_snapshot.global.translation(),
                        foot_position,
                        upper_length,
                        lower_length,
                        pole,
                    ),
                    target,
                    terrain_maximum_reach(upper_length, lower_length),
                ) {
                    apply_two_bone_solution(
                        upper,
                        lower,
                        foot,
                        solution,
                        &parents,
                        &mut transforms,
                    );
                    if state_delta_seconds > 0.0 {
                        let bend = (solution.knee - upper_snapshot.global.translation())
                            .reject_from_normalized(solution.end_direction)
                            .try_normalize();
                        if left {
                            if let Some(bend) = bend {
                                memory.left_terrain_pole_world = Some(bend);
                            }
                            memory.left_terrain_end_direction = Some(solution.end_direction);
                        } else {
                            if let Some(bend) = bend {
                                memory.right_terrain_pole_world = Some(bend);
                            }
                            memory.right_terrain_end_direction = Some(solution.end_direction);
                        }
                    }
                }
                if let Some(normal) = terrain.normal_at(target.xz())
                    && let Some(sole_axis) = rig.sole_axis(left)
                {
                    align_foot_to_slope(foot, sole_axis, normal, &parents, &mut transforms);
                }
                let owner_target = rig_rotation.inverse() * (target - rig_origin);
                if left {
                    memory.left_foot_plant = Some(frozen_plant);
                    memory.left_foot_plant_acquired = false;
                    memory.left_foot_target = Some(owner_target);
                    memory.left_foot_world_target = Some(target);
                    memory.left_support_weight = Some(0.0);
                    memory.left_transition_support_weight = Some(logical_weight);
                    memory.left_release_active = false;
                    memory.left_release_target = None;
                } else {
                    memory.right_foot_plant = Some(frozen_plant);
                    memory.right_foot_plant_acquired = false;
                    memory.right_foot_target = Some(owner_target);
                    memory.right_foot_world_target = Some(target);
                    memory.right_support_weight = Some(0.0);
                    memory.right_transition_support_weight = Some(logical_weight);
                    memory.right_release_active = false;
                    memory.right_release_target = None;
                }
                airborne_orientation_owned[leg_index] = true;
                continue;
            }
            let plant_acquired = if left {
                memory.left_foot_plant_acquired
            } else {
                memory.right_foot_plant_acquired
            };
            let exhausted = if left {
                memory.left_support_exhausted_until_flight
            } else {
                memory.right_support_exhausted_until_flight
            };
            let retained_plan = if left {
                memory.left_planned_contact
            } else {
                memory.right_planned_contact
            };
            // A run can begin inside a raw support lobe whose foot was never
            // rendered at contact (notably the hard-start fixture). Without a
            // preceding swing plan there is no truthful footprint to acquire;
            // jumping to a freshly predicted plant moves the whole chain. Skip
            // the remainder of that lobe and begin normally after true flight.
            // True raw flight clears the preceding toe-off latch before any
            // effective-support suppression. Plan state must not be able to
            // keep a latch alive across a complete same-foot cycle.
            let exhausted = exhausted_latch_after_raw_cadence(exhausted, raw_nominal_weight);
            let exhausted = exhausted
                || unplanned_run_support_requires_flight(
                    locomotion_profile(skeleton).gait,
                    skeleton.animation_speed(),
                    weight,
                    plant_acquired,
                    retained_plan,
                );
            let (mut next_exhausted, mut effective_weight) =
                support_after_exhausted_lobe(exhausted, weight);
            if run_plan_is_on_rising_support(
                locomotion_profile(skeleton).gait,
                skeleton.gait_phase,
                left,
                locomotion_profile(skeleton).support_phase_radius,
                raw_nominal_weight,
                retained_plan,
                plant_acquired,
            ) {
                next_exhausted = false;
                effective_weight = raw_nominal_weight;
            }
            if left {
                memory.left_support_exhausted_until_flight = next_exhausted;
            } else {
                memory.right_support_exhausted_until_flight = next_exhausted;
            }
            let release_now = run_is_at_support_exit(
                skeleton.gait_phase,
                left,
                locomotion_profile(skeleton).support_phase_radius,
            );
            let (toe_off_started, toe_off_weight) = run_toe_off_support_weight(
                locomotion_profile(skeleton).gait,
                run_retained_support_through_lobe_edge(
                    locomotion_profile(skeleton).gait,
                    effective_weight,
                    plant_acquired && plant.is_some(),
                    release_now,
                ),
                plant_acquired && plant.is_some(),
                release_now,
            );
            weight = toe_off_weight;
            // Commit the new transition after the prior latch's flight-clear
            // result. Otherwise an abrupt 1 -> 0 profile clears a latch in the
            // same evaluation that created it, allowing next-tick reentry.
            if toe_off_started {
                if left {
                    memory.left_support_exhausted_until_flight = true;
                } else {
                    memory.right_support_exhausted_until_flight = true;
                }
            }
            if exhausted && next_exhausted {
                plant = None;
            }
            if plant_acquired
                && let Some(retained_plant) = plant
                && let Some(height) = terrain.height_at(retained_plant.xz())
            {
                let retained_target = Vec3::new(
                    retained_plant.x,
                    height + measured_ankle_sole_offset_metres(),
                    retained_plant.z,
                );
                let reachable_target = constrain_target_to_reach(
                    retained_target,
                    upper_snapshot.global.translation(),
                    terrain_maximum_reach(upper_length, lower_length),
                );
                if retained_plant_requires_release(retained_target, reachable_target) {
                    // A support footprint is either stationary or released.
                    // Do not preserve nominal support by skating the plant as
                    // the hip outruns its reachable region.
                    weight = 0.0;
                    plant = None;
                    if left {
                        memory.left_support_exhausted_until_flight = true;
                    } else {
                        memory.right_support_exhausted_until_flight = true;
                    }
                }
            }
            if settle_support_owned {
                // Run cadence/exhaustion is evaluated above for ordinary
                // locomotion and may suppress or clear an unacquired plant.
                // Stop capture has its own completion-driven ownership: put
                // the selected footprint back and keep solving it until the
                // propagated contact becomes truthful.
                weight = 1.0;
                plant = settle_support_plant;
                if left {
                    memory.left_support_exhausted_until_flight = false;
                } else {
                    memory.right_support_exhausted_until_flight = false;
                }
            }
            let opposite_acquired = if left {
                memory.right_foot_plant_acquired
                    && memory.right_foot_plant.is_some()
                    && memory
                        .right_support_weight
                        .is_some_and(terrain_leg_has_support)
            } else {
                memory.left_foot_plant_acquired
                    && memory.left_foot_plant.is_some()
                    && memory
                        .left_support_weight
                        .is_some_and(terrain_leg_has_support)
            };
            weight = coordinated_support_weight(
                locomotion_profile(skeleton).gait,
                weight,
                plant_acquired && plant.is_some(),
                opposite_acquired,
            );
            if ordinary_plant_requires_clear(weight, plant_acquired, plant, foot_position) {
                plant = None;
            }
            if !terrain_leg_has_support(weight) {
                airborne_orientation_owned[leg_index] = true;
                // An airborne foot is never retained at its old plant. During
                // ordinary locomotion it follows authored FK immediately;
                // during a stop it follows an explicit clearance arc toward
                // the balance-restoring contact.
                let settle_swing = memory.settle.filter(|settle| settle.support_left != left);
                if settle_swing.is_none() {
                    let gait = locomotion_profile(skeleton).gait;
                    let run_airborne_budget = uses_run_airborne_motion_budget(
                        gait,
                        planar_velocity
                            .length()
                            .max(memory.measured_owner_planar_speed),
                    );
                    let airborne_budget_gait = if run_airborne_budget {
                        LocomotionGait::Run
                    } else {
                        gait
                    };
                    let phase_to_contact = phase_to_next_contact(skeleton.gait_phase, left);
                    let mut retained_contact = if left {
                        memory.left_planned_contact
                    } else {
                        memory.right_planned_contact
                    };
                    let mut retained_start = if left {
                        memory.left_planned_contact_start
                    } else {
                        memory.right_planned_contact_start
                    };
                    let mut retained_phase_start = if left {
                        memory.left_planned_contact_phase_start
                    } else {
                        memory.right_planned_contact_phase_start
                    };
                    let previous_transition_support = if left {
                        memory.left_transition_support_weight
                    } else {
                        memory.right_transition_support_weight
                    };
                    let failed_acquisition_lobe_exited = acquisition_lobe_exited_without_contact(
                        retained_contact,
                        plant_acquired,
                        previous_transition_support,
                        weight,
                    );
                    if failed_acquisition_lobe_exited {
                        clear_planned_contact_metadata(
                            &mut retained_contact,
                            &mut retained_start,
                            &mut retained_phase_start,
                        );
                        if left {
                            clear_planned_contact_metadata(
                                &mut memory.left_planned_contact,
                                &mut memory.left_planned_contact_start,
                                &mut memory.left_planned_contact_phase_start,
                            );
                        } else {
                            clear_planned_contact_metadata(
                                &mut memory.right_planned_contact,
                                &mut memory.right_planned_contact_start,
                                &mut memory.right_planned_contact_phase_start,
                            );
                        }
                    }
                    let propagated_visible_target = if left {
                        memory
                            .left_last_rendered_world
                            .or(memory.left_foot_world_target)
                    } else {
                        memory
                            .right_last_rendered_world
                            .or(memory.right_foot_world_target)
                    };
                    let (was_releasing, previous_owner_target) = if left {
                        (
                            memory.left_release_active,
                            run_previous_owner_target(
                                airborne_budget_gait,
                                memory.left_last_rendered_owner,
                                memory.left_foot_target,
                            ),
                        )
                    } else {
                        (
                            memory.right_release_active,
                            run_previous_owner_target(
                                airborne_budget_gait,
                                memory.right_last_rendered_owner,
                                memory.right_foot_target,
                            ),
                        )
                    };
                    let prior_visible_target = run_plan_visible_start(
                        airborne_budget_gait,
                        retained_contact.is_none(),
                        was_releasing,
                        previous_owner_target,
                        rig_origin,
                        rig_rotation,
                        propagated_visible_target,
                    );
                    let approach_window =
                        if locomotion_profile(skeleton).gait == LocomotionGait::Run {
                            ik_tuning().run_contact_approach_phase
                        } else {
                            0.12
                        };
                    let support_lobe_exhausted = if left {
                        memory.left_support_exhausted_until_flight
                    } else {
                        memory.right_support_exhausted_until_flight
                    };
                    let planned_contact = run_planned_contact_allowed(
                        support_lobe_exhausted,
                        phase_to_contact,
                        approach_window,
                    )
                    .then(|| {
                        retained_contact.unwrap_or_else(|| {
                            let candidate = ordinary_contact_target(
                                rig_origin,
                                rig_rotation,
                                projected_body_center(rig, &transforms.p0()).unwrap_or(rig_origin),
                                planar_velocity,
                                skeleton.animation_speed(),
                                phase_to_contact,
                                side,
                            );
                            if locomotion_profile(skeleton).gait == LocomotionGait::Run {
                                reachable_run_contact_target(
                                    candidate,
                                    upper_snapshot.global.translation(),
                                    planar_velocity,
                                    skeleton.animation_speed(),
                                    phase_to_contact,
                                    locomotion_profile(skeleton).support_phase_radius
                                        + ik_tuning().run_contact_chain_settle_phase,
                                    terrain_maximum_reach(upper_length, lower_length),
                                    |xz| terrain.height_at(xz),
                                )
                            } else {
                                candidate
                            }
                        })
                    })
                    .filter(|_| ordinary_lowered);
                    let planned_start = planned_contact.map(|_| {
                        planned_contact_start(retained_start, prior_visible_target, foot_position)
                    });
                    let planned_contact = planned_contact.map(|contact| {
                        if locomotion_profile(skeleton).gait == LocomotionGait::Run
                            && late_run_plan_requires_bound(retained_contact, phase_to_contact)
                        {
                            bound_late_run_contact(
                                planned_start.unwrap_or(foot_position),
                                contact,
                                skeleton.animation_speed(),
                                phase_to_contact,
                                locomotion_profile(skeleton).support_phase_radius
                                    + ik_tuning().run_contact_chain_settle_phase,
                            )
                        } else {
                            contact
                        }
                    });
                    let planned_phase_start =
                        planned_contact.map(|_| retained_phase_start.unwrap_or(phase_to_contact));
                    if left {
                        memory.left_planned_contact = planned_contact;
                        memory.left_planned_contact_start = planned_start;
                        memory.left_planned_contact_phase_start = planned_phase_start;
                    } else {
                        memory.right_planned_contact = planned_contact;
                        memory.right_planned_contact_start = planned_start;
                        memory.right_planned_contact_phase_start = planned_phase_start;
                    }
                    let planned_progress =
                        if locomotion_profile(skeleton).gait == LocomotionGait::Run {
                            run_contact_approach_progress(
                                phase_to_contact,
                                planned_phase_start.unwrap_or(approach_window),
                                locomotion_profile(skeleton).support_phase_radius
                                    + ik_tuning().run_contact_chain_settle_phase,
                            )
                        } else {
                            smoothstep(approach_window, 0.0, phase_to_contact)
                        };
                    let mut desired_target =
                        planned_contact.map_or(foot_position, |mut contact| {
                            if let Some(height) = terrain.height_at(contact.xz()) {
                                contact.y = height + measured_ankle_sole_offset_metres();
                            }
                            planned_start
                                .unwrap_or(foot_position)
                                .lerp(contact, planned_progress)
                        });
                    if locomotion_profile(skeleton).gait == LocomotionGait::Run
                        && let Some(height) = terrain.height_at(desired_target.xz())
                    {
                        let clearance = run_swing_clearance(
                            phase_to_contact,
                            planned_contact.map(|_| planned_progress),
                        );
                        desired_target.y = desired_target
                            .y
                            .max(height + measured_ankle_sole_offset_metres() + clearance);
                    }
                    let desired_owner_target =
                        rig_rotation.inverse() * (desired_target - rig_origin);
                    let (
                        previous_owner_target,
                        previous_world_target,
                        previous_support,
                        was_releasing,
                        previous_goal,
                    ) = if left {
                        (
                            run_previous_owner_target(
                                airborne_budget_gait,
                                memory.left_last_rendered_owner,
                                memory.left_foot_target,
                            ),
                            memory.left_foot_world_target,
                            memory.left_transition_support_weight,
                            memory.left_release_active,
                            memory.left_release_target,
                        )
                    } else {
                        (
                            run_previous_owner_target(
                                airborne_budget_gait,
                                memory.right_last_rendered_owner,
                                memory.right_foot_target,
                            ),
                            memory.right_foot_world_target,
                            memory.right_transition_support_weight,
                            memory.right_release_active,
                            memory.right_release_target,
                        )
                    };
                    // Support loss releases in owner space at a bounded speed.
                    // This remains a purely airborne solve: there is no plant,
                    // terrain projection, or clearance floor. Once converged,
                    // authored FK owns the swing again until final acquisition.
                    let just_released = previous_support.is_some_and(terrain_leg_has_support);
                    let run_release_edge = run_release_edge(just_released, toe_off_started);
                    airborne_just_released[leg_index] = run_release_edge;
                    let (mut owner_target, next_release_goal) = if run_release_edge {
                        // The authored FK foot may be nearly a metre from the
                        // world plant on the first release sample. Begin from
                        // the preceding visible target and defer movement to
                        // the bounded release follower instead of rebuilding
                        // the whole chain against that authored endpoint.
                        let owner_target = release_start_owner_target(
                            airborne_budget_gait,
                            previous_owner_target,
                            previous_world_target,
                            rig_origin,
                            rig_rotation,
                            desired_owner_target,
                        );
                        (owner_target, Some(desired_owner_target))
                    } else if planned_contact.is_some() {
                        // A predicted touchdown is stationary in world space.
                        // Run's frozen-start Hermite trajectory already owns
                        // continuity and reaches support entry exactly. Feeding
                        // it through the old retained-target follower starts a
                        // new swing from the previous cycle's stale endpoint.
                        let world_target =
                            if locomotion_profile(skeleton).gait == LocomotionGait::Run {
                                desired_target
                            } else {
                                advance_foot_target_at_speed(
                                    previous_world_target,
                                    desired_target,
                                    state_delta_seconds,
                                    ik_tuning().airborne_release_step_metres
                                        * ik_tuning().continuity_sample_hz,
                                )
                            };
                        let owner_target = rig_rotation.inverse() * (world_target - rig_origin);
                        let next = (world_target.distance_squared(desired_target) > 0.000001)
                            .then_some(desired_owner_target);
                        (owner_target, next)
                    } else {
                        let needs_release = was_releasing
                            || previous_support.is_some_and(|support| support > 0.5)
                            || previous_owner_target.is_some_and(|previous| {
                                previous.distance(desired_owner_target)
                                    > (ik_tuning().airborne_release_step_metres
                                        * ik_tuning().continuity_sample_hz)
                                        * state_delta_seconds.max(0.0)
                                        + 0.001
                            });
                        let release_goal = if was_releasing {
                            previous_goal.unwrap_or(desired_owner_target)
                        } else {
                            desired_owner_target
                        };
                        let owner_target = if needs_release {
                            advance_foot_target_at_speed(
                                previous_owner_target,
                                release_goal,
                                state_delta_seconds,
                                ik_tuning().airborne_release_step_metres
                                    * ik_tuning().continuity_sample_hz,
                            )
                        } else {
                            desired_owner_target
                        };
                        let reached_goal = owner_target.distance_squared(release_goal) <= 0.000001;
                        let next = if reached_goal
                            && owner_target.distance_squared(desired_owner_target) > 0.000001
                        {
                            Some(desired_owner_target)
                        } else if reached_goal {
                            None
                        } else {
                            Some(release_goal)
                        };
                        (owner_target, next)
                    };
                    let mut target = rig_origin + rig_rotation * owner_target;
                    if run_airborne_budget && run_release_edge {
                        // On flat ground the owner-transported point is already
                        // feasible and remains unchanged. Uphill, transporting
                        // the full root delta can raise terrain plus the 5 cm
                        // flight floor beyond the 9 cm 3D budget. Aim back
                        // toward the prior visible world plant so the joint
                        // terrain/budget projection below can retain only the
                        // feasible fraction of root transport.
                        target = previous_world_target.unwrap_or(target);
                    }
                    if run_airborne_budget && let Some(height) = terrain.height_at(target.xz()) {
                        let contact_reachable = run_contact_within_follower_step(
                            previous_owner_target,
                            target,
                            rig_origin,
                            rig_rotation,
                            state_delta_seconds,
                        );
                        let support_eligible_for_descent = run_support_eligible_for_descent(
                            airborne_budget_gait,
                            skeleton.gait_phase,
                            left,
                            locomotion_profile(skeleton).support_phase_radius,
                            raw_nominal_weight,
                            contact_reachable
                                && run_contact_within_leg_reach(
                                    target,
                                    upper_snapshot.global.translation(),
                                    terrain_maximum_reach(upper_length, lower_length),
                                ),
                        );
                        let clearance = run_airborne_clearance_for_sample(
                            run_release_edge,
                            phase_to_contact,
                            planned_contact.map(|_| planned_progress),
                            support_eligible_for_descent,
                        );
                        target.y = run_clearance_target_height(
                            target.y,
                            height + measured_ankle_sole_offset_metres() + clearance,
                            support_eligible_for_descent,
                        );
                        owner_target = rig_rotation.inverse() * (target - rig_origin);
                    }
                    if run_airborne_budget {
                        // Limit the complete owner-local 3D swing after terrain
                        // height and clearance are applied. World plants never
                        // enter this airborne branch and remain exact.
                        let contact_reachable = run_contact_within_follower_step(
                            previous_owner_target,
                            target,
                            rig_origin,
                            rig_rotation,
                            state_delta_seconds,
                        );
                        let support_eligible_for_descent = run_support_eligible_for_descent(
                            airborne_budget_gait,
                            skeleton.gait_phase,
                            left,
                            locomotion_profile(skeleton).support_phase_radius,
                            raw_nominal_weight,
                            contact_reachable
                                && run_contact_within_leg_reach(
                                    target,
                                    upper_snapshot.global.translation(),
                                    terrain_maximum_reach(upper_length, lower_length),
                                ),
                        );
                        let clearance = run_airborne_clearance_for_sample(
                            run_release_edge,
                            phase_to_contact,
                            planned_contact.map(|_| planned_progress),
                            support_eligible_for_descent,
                        );
                        target = advance_run_airborne_world_target(
                            previous_owner_target,
                            target,
                            rig_origin,
                            rig_rotation,
                            state_delta_seconds,
                            run_airborne_owner_target_speed_for_sample(
                                run_release_edge,
                                settle_cancelled_for_restart,
                            ),
                            |xz| {
                                terrain.height_at(xz).map(|height| {
                                    height + measured_ankle_sole_offset_metres() + clearance
                                })
                            },
                        );
                        owner_target = rig_rotation.inverse() * (target - rig_origin);
                        if toe_off_started
                            && retained_contact.is_none()
                            && planned_contact.is_some()
                        {
                            // Toe-off and next-plan creation can occur in the
                            // same evaluation. Freeze the terrain-feasible
                            // release result as the new swing start so the next
                            // tick cannot reconstruct the plan from the
                            // pre-projection world ankle and repeat the seam.
                            if left {
                                memory.left_planned_contact_start = Some(target);
                                memory.left_planned_contact_phase_start = Some(phase_to_contact);
                            } else {
                                memory.right_planned_contact_start = Some(target);
                                memory.right_planned_contact_phase_start = Some(phase_to_contact);
                            }
                        }
                    }
                    let release_active = next_release_goal.is_some()
                        || owner_target.distance_squared(desired_owner_target) > 0.000001
                        || unplanned_terrain_solve_requires_release(
                            planned_contact,
                            target,
                            foot_position,
                        );
                    let canonical_world = pole_to_world(rig_rotation, canonical_knee_pole(side));
                    let (remembered_pole, previous_end_direction) = if left {
                        (
                            memory.left_terrain_pole_world,
                            memory.left_terrain_end_direction,
                        )
                    } else {
                        (
                            memory.right_terrain_pole_world,
                            memory.right_terrain_end_direction,
                        )
                    };
                    let next_end_direction =
                        (target - upper_snapshot.global.translation()).normalize_or_zero();
                    let pole = transported_terrain_pole(
                        remembered_pole,
                        previous_end_direction,
                        next_end_direction,
                        canonical_world,
                    )
                    .unwrap_or(canonical_world);
                    let pole = constrain_rendered_leg_pole(
                        rig,
                        left,
                        upper_snapshot.global.translation(),
                        foot_position,
                        target,
                        pole,
                        &parents,
                        &transforms.p0(),
                    );
                    let mut resolved_end = None;
                    if let Some(solution) = solve_two_bone_with_reach(
                        TwoBoneChain::new(
                            upper_snapshot.global.translation(),
                            lower_snapshot.global.translation(),
                            foot_position,
                            upper_length,
                            lower_length,
                            pole,
                        ),
                        target,
                        maximum_reach(upper_length, lower_length),
                    ) {
                        resolved_end = Some(solution.end);
                        apply_two_bone_solution(
                            upper,
                            lower,
                            foot,
                            solution,
                            &parents,
                            &mut transforms,
                        );
                        if state_delta_seconds > 0.0 {
                            let bend = (solution.knee - upper_snapshot.global.translation())
                                .reject_from_normalized(solution.end_direction)
                                .try_normalize();
                            if left {
                                if let Some(bend) = bend {
                                    memory.left_terrain_pole_world = Some(bend);
                                }
                                memory.left_terrain_end_direction = Some(solution.end_direction);
                            } else {
                                if let Some(bend) = bend {
                                    memory.right_terrain_pole_world = Some(bend);
                                }
                                memory.right_terrain_end_direction = Some(solution.end_direction);
                            }
                        }
                    }
                    if locomotion_profile(skeleton).gait == LocomotionGait::Run
                        && let Some(normal) = terrain.normal_at(target.xz())
                        && let Some(sole_axis) = rig.sole_axis(left)
                    {
                        // Run swing orientation is terrain-aware before the
                        // nominal support edge. Arriving tangent prevents the
                        // toe joint from sweeping through the ground while the
                        // nine-degree contact blend catches up.
                        align_foot_to_slope(foot, sole_axis, normal, &parents, &mut transforms);
                    }
                    if left {
                        memory.left_foot_plant = None;
                        memory.left_foot_plant_acquired = false;
                        memory.left_foot_target = Some(owner_target);
                        memory.left_foot_world_target = Some(target);
                        memory.left_support_weight = Some(0.0);
                        memory.left_transition_support_weight = Some(0.0);
                        memory.left_release_active = release_active;
                        memory.left_release_target = next_release_goal;
                    } else {
                        memory.right_foot_plant = None;
                        memory.right_foot_plant_acquired = false;
                        memory.right_foot_target = Some(owner_target);
                        memory.right_foot_world_target = Some(target);
                        memory.right_support_weight = Some(0.0);
                        memory.right_transition_support_weight = Some(0.0);
                        memory.right_release_active = release_active;
                        memory.right_release_target = next_release_goal;
                    }
                    if let Some(resolved_end) = resolved_end {
                        // A high-speed unplanned release can request a
                        // terrain waypoint beyond the current analytic reach.
                        // Continue the next sample from the ankle the player
                        // actually sees instead of repeatedly owning the
                        // rejected request. Planned swings keep their frozen
                        // endpoint metadata unchanged.
                        commit_resolved_unplanned_airborne_release(
                            &mut memory,
                            left,
                            run_airborne_budget,
                            planned_contact,
                            release_active,
                            resolved_end,
                            rig_origin,
                            rig_rotation,
                        );
                    }
                    continue;
                }
                let settle = settle_swing.expect("settle swing was checked above");
                let mut desired_target =
                    settle_swing_target(settle.swing_start, settle.landing_target, settle.progress);
                if let Some(height) = terrain.height_at(desired_target.xz()) {
                    let minimum_ankle_y = height
                        + measured_ankle_sole_offset_metres()
                        + (ik_tuning().swing_sole_clearance_metres * (1.0 - settle.progress))
                            .max(ik_tuning().terrain_transition_flight_toe_clearance_metres);
                    desired_target.y = desired_target
                        .y
                        .max(foot_position.y.lerp(minimum_ankle_y, terrain_blend));
                    if let Some((rendered_ankle, rendered_toe)) = rendered_ankle_and_toe
                        && let Some(toe_safe_ankle_y) = toe_aware_minimum_ankle_y(
                            rendered_ankle,
                            rendered_toe,
                            desired_target.xz(),
                            transition_toe_clearance_with_rotation_margin(
                                rendered_ankle,
                                rendered_toe,
                                state_delta_seconds,
                            ),
                            |xz| terrain.height_at(xz),
                        )
                    {
                        desired_target.y = desired_target.y.max(toe_safe_ankle_y);
                    }
                }
                let desired_owner_target = rig_rotation.inverse() * (desired_target - rig_origin);
                let previous_owner_target = if left {
                    memory.left_foot_target
                } else {
                    memory.right_foot_target
                };
                // Resolve the follower and toe/sole floor together. Applying
                // clearance only to the distant settle goal leaves the
                // rate-limited intermediate waypoint below terrain for several
                // frames; projecting Y after the cap can instead exceed the
                // continuity budget. This joint search finds the closest
                // terrain-valid waypoint inside the same owner-local sphere.
                let target = advance_run_airborne_world_target(
                    previous_owner_target,
                    desired_target,
                    rig_origin,
                    rig_rotation,
                    state_delta_seconds,
                    settle_target_speed(settle),
                    |xz| {
                        let sole_minimum = terrain.height_at(xz).map(|height| {
                            height
                                + measured_ankle_sole_offset_metres()
                                + ik_tuning().terrain_transition_flight_toe_clearance_metres
                        });
                        let toe_minimum =
                            rendered_ankle_and_toe.and_then(|(rendered_ankle, rendered_toe)| {
                                toe_aware_minimum_ankle_y(
                                    rendered_ankle,
                                    rendered_toe,
                                    xz,
                                    transition_toe_clearance_with_rotation_margin(
                                        rendered_ankle,
                                        rendered_toe,
                                        state_delta_seconds,
                                    ),
                                    |sample| terrain.height_at(sample),
                                )
                            });
                        sole_minimum.into_iter().chain(toe_minimum).reduce(f32::max)
                    },
                );
                let owner_target = rig_rotation.inverse() * (target - rig_origin);
                let release_active = owner_target.distance_squared(desired_owner_target) > 0.000001;
                let canonical_pole = canonical_knee_pole(side);
                let canonical_world = pole_to_world(rig_rotation, canonical_pole);
                let (remembered_pole, previous_end_direction) = if left {
                    (
                        memory.left_terrain_pole_world,
                        memory.left_terrain_end_direction,
                    )
                } else {
                    (
                        memory.right_terrain_pole_world,
                        memory.right_terrain_end_direction,
                    )
                };
                let next_end_direction =
                    (target - upper_snapshot.global.translation()).normalize_or_zero();
                let remembered = transported_terrain_pole(
                    remembered_pole,
                    previous_end_direction,
                    next_end_direction,
                    canonical_world,
                );
                let pole = remembered.unwrap_or(canonical_world);
                let pole = constrain_rendered_leg_pole(
                    rig,
                    left,
                    upper_snapshot.global.translation(),
                    foot_position,
                    target,
                    pole,
                    &parents,
                    &transforms.p0(),
                );
                if let Some(solution) = solve_two_bone_with_reach(
                    TwoBoneChain::new(
                        upper_snapshot.global.translation(),
                        lower_snapshot.global.translation(),
                        foot_position,
                        upper_length,
                        lower_length,
                        pole,
                    ),
                    target,
                    maximum_reach(upper_length, lower_length),
                ) {
                    settle_contact_reached = settle.progress >= 1.0
                        && solution.end.xz().distance(settle.landing_target.xz()) <= 0.02
                        && terrain
                            .height_at(solution.end.xz())
                            .is_some_and(|height| sole_is_at_contact(solution.end.y, height));
                    apply_two_bone_solution(
                        upper,
                        lower,
                        foot,
                        solution,
                        &parents,
                        &mut transforms,
                    );
                    if state_delta_seconds > 0.0 {
                        let bend = (solution.knee - upper_snapshot.global.translation())
                            .reject_from_normalized(solution.end_direction)
                            .try_normalize();
                        if left {
                            if let Some(bend) = bend {
                                memory.left_terrain_pole_world = Some(bend);
                            }
                            memory.left_terrain_end_direction = Some(solution.end_direction);
                        } else {
                            if let Some(bend) = bend {
                                memory.right_terrain_pole_world = Some(bend);
                            }
                            memory.right_terrain_end_direction = Some(solution.end_direction);
                        }
                    }
                }
                if let Some(normal) = terrain.normal_at(target.xz())
                    && let Some(sole_axis) = rig.sole_axis(left)
                {
                    // A settling foot approaches its contact tangent throughout
                    // the capture arc. Deferring alignment until terminal idle
                    // can drive the rear toe through rising terrain even while
                    // the ankle remains clear. The final rotation pass retains
                    // the existing bounded world-angle step.
                    align_foot_to_slope(foot, sole_axis, normal, &parents, &mut transforms);
                }
                if left {
                    memory.left_foot_plant = None;
                    memory.left_foot_plant_acquired = false;
                    memory.left_foot_target = Some(owner_target);
                    memory.left_foot_world_target = Some(target);
                    memory.left_support_weight = Some(0.0);
                    memory.left_transition_support_weight = Some(0.0);
                    memory.left_release_active = release_active;
                    memory.left_release_target = release_active.then_some(desired_owner_target);
                } else {
                    memory.right_foot_plant = None;
                    memory.right_foot_plant_acquired = false;
                    memory.right_foot_target = Some(owner_target);
                    memory.right_foot_world_target = Some(target);
                    memory.right_support_weight = Some(0.0);
                    memory.right_transition_support_weight = Some(0.0);
                    memory.right_release_active = release_active;
                    memory.right_release_target = release_active.then_some(desired_owner_target);
                }
                continue;
            }
            // Do not memorize a footprint while the swing foot is merely
            // approaching the ground. Capturing that stale position early
            // makes the pelvis outrun it, forcing the reach limiter to drag a
            // fully weighted foot and drive the knee toward extension.
            let retained_planned_contact = if left {
                memory.left_planned_contact
            } else {
                memory.right_planned_contact
            };
            // The first nominal-support sample is only an acquisition request.
            // Keep the frozen plan until the propagated sole actually reaches
            // it; clearing here made the next sample rebuild from authored FK
            // just as the ankle entered the final centimetre of contact.
            if acquired_plan_can_clear(plant_acquired) {
                if left {
                    clear_planned_contact_metadata(
                        &mut memory.left_planned_contact,
                        &mut memory.left_planned_contact_start,
                        &mut memory.left_planned_contact_phase_start,
                    );
                } else {
                    clear_planned_contact_metadata(
                        &mut memory.right_planned_contact,
                        &mut memory.right_planned_contact_start,
                        &mut memory.right_planned_contact_phase_start,
                    );
                }
            }
            let ordinary_planned_contact = (ordinary_lowered
                && skeleton.animation_speed() > 0.05
                && planar_velocity.length_squared() > 0.0025)
                .then(|| {
                    retained_planned_contact.unwrap_or_else(|| {
                        let projected_com =
                            projected_body_center(rig, &transforms.p0()).unwrap_or(rig_origin);
                        ordinary_contact_target(
                            rig_origin,
                            rig_rotation,
                            projected_com,
                            planar_velocity,
                            skeleton.animation_speed(),
                            phase_to_next_contact(skeleton.gait_phase, left),
                            side,
                        )
                    })
                });
            if plant.is_none()
                && let Some(planned_contact) = ordinary_planned_contact
            {
                // Freeze the next contact as soon as the contact ramp begins.
                // Recomputing it from the advancing COM every tick would make
                // a nominally supported foot chase the body instead of land.
                plant = Some(
                    if locomotion_profile(skeleton).gait == LocomotionGait::Run {
                        // Run planning already froze a stance-reachable world
                        // footprint. Reapplying the body's small local foot-track
                        // corridor here would replace it with a point under the
                        // advancing hip and recreate the support-entry snap.
                        planned_contact
                    } else {
                        constrain_foot_to_track(planned_contact, rig_origin, rig_rotation, side)
                    },
                );
            } else if weight >= 0.95 && plant.is_none() && !raised_guard_follower {
                let visible_contact = ordinary_planned_contact.unwrap_or_else(|| {
                    if left {
                        memory.left_foot_world_target
                    } else {
                        memory.right_foot_world_target
                    }
                    .unwrap_or(foot_position)
                });
                plant = Some(constrain_foot_to_track(
                    visible_contact,
                    rig_origin,
                    rig_rotation,
                    side,
                ));
            }
            let mut horizontal_target = plant.unwrap_or_else(|| {
                ordinary_planned_contact.unwrap_or_else(|| {
                    constrain_foot_to_track(foot_position, rig_origin, rig_rotation, side)
                })
            });
            let plant_local = rig_rotation.inverse() * (horizontal_target - rig_origin);
            if plant_local.x * side < foot_track_inner_metres() {
                // A retained world plant can rotate through the body's center
                // during an exact reversal. Move only the offending lateral
                // component back to its anatomical corridor; target velocity
                // limiting below keeps that correction continuous.
                horizontal_target =
                    constrain_foot_to_track(horizontal_target, rig_origin, rig_rotation, side);
                plant = plant.map(|_| horizontal_target);
            }
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            let sole_offset = measured_ankle_sole_offset_metres();
            let mut planted_target = Vec3::new(
                horizontal_target.x,
                height + sole_offset,
                horizontal_target.z,
            );
            // A planned terrain contact can be vertically unreachable early
            // in its acquisition. Solve toward the nearest reachable point
            // without overwriting the frozen plan; once the body arrives, the
            // same plan becomes the stationary stance plant.
            planted_target = acquisition_planted_target(
                planted_target,
                upper_snapshot.global.translation(),
                terrain_maximum_reach(upper_length, lower_length),
                locomotion_profile(skeleton).gait,
                plant_acquired,
            );
            horizontal_target.x = planted_target.x;
            horizontal_target.z = planted_target.z;
            // Reach limiting may have moved the target into another triangle.
            // Resample that actual point instead of retaining a height from the
            // old XZ and a normal from the new one.
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            planted_target.y = height + sole_offset;
            if let Some((rendered_ankle, rendered_toe)) = rendered_ankle_and_toe
                && let Some(toe_safe_ankle_y) = toe_aware_minimum_ankle_y(
                    rendered_ankle,
                    rendered_toe,
                    planted_target.xz(),
                    ik_tuning().terrain_contact_toe_clearance_metres,
                    |xz| terrain.height_at(xz),
                )
            {
                planted_target.y = planted_target.y.max(toe_safe_ankle_y);
            }
            if left {
                memory.left_foot_plant = plant;
            } else {
                memory.right_foot_plant = plant;
            }
            // Acquisition advances in world space. Rate-limiting the equivalent
            // owner-space point made a stationary plant move backward by the
            // controller's 8.6 cm/tick run displacement before applying the
            // 5.3 cm target cap, so the ankle could never catch its contact.
            // Once contact is reported, solve directly to the frozen world
            // plant; a stance foot is stationary or released, never skated.
            let solve_weight = smoothstep(0.05, 0.9, weight) * terrain_blend;
            let release_target_speed = memory.settle.map(settle_target_speed).unwrap_or(
                ik_tuning().airborne_release_step_metres * ik_tuning().continuity_sample_hz,
            );
            let support_run_airborne_budget = uses_run_airborne_motion_budget(
                locomotion_profile(skeleton).gait,
                planar_velocity
                    .length()
                    .max(memory.measured_owner_planar_speed),
            );
            let support_budget_gait = if support_run_airborne_budget {
                LocomotionGait::Run
            } else {
                locomotion_profile(skeleton).gait
            };
            let (
                previous_owner_target,
                previous_world_target,
                previous_support,
                previous_reported_support,
                was_releasing,
            ) = if left {
                (
                    run_previous_owner_target(
                        support_budget_gait,
                        memory.left_last_rendered_owner,
                        memory.left_foot_target,
                    ),
                    memory.left_foot_world_target,
                    memory.left_transition_support_weight,
                    memory.left_support_weight,
                    memory.left_release_active,
                )
            } else {
                (
                    run_previous_owner_target(
                        support_budget_gait,
                        memory.right_last_rendered_owner,
                        memory.right_foot_target,
                    ),
                    memory.right_foot_world_target,
                    memory.right_transition_support_weight,
                    memory.right_support_weight,
                    memory.right_release_active,
                )
            };
            let mut target = if plant_acquired {
                planted_target
            } else if locomotion_profile(skeleton).gait == LocomotionGait::Run {
                // Entering nominal support does not bypass the airborne
                // follower. The frozen plant becomes direct only after the
                // propagated sole has truthfully acquired it.
                let fixed_contact_reachable = run_contact_within_follower_motion_step(
                    previous_owner_target,
                    planted_target,
                    rig_origin,
                    rig_rotation,
                    state_delta_seconds,
                );
                // Match the exact reach enforced by the upright analytic
                // solve below. Planning's looser 12-degree terrain reach left
                // the captured phase-.867 target about 1 cm beyond Run's
                // 20-degree solve reach, so the isolated retarget passed while
                // the rendered sole still stopped above contact.
                let final_solve_reach = maximum_reach(upper_length, lower_length);
                let fixed_contact_within_leg_reach = planted_target
                    .distance(upper_snapshot.global.translation())
                    <= final_solve_reach + 0.001;
                let rising_support = run_support_eligible_for_descent(
                    locomotion_profile(skeleton).gait,
                    skeleton.gait_phase,
                    left,
                    locomotion_profile(skeleton).support_phase_radius,
                    raw_nominal_weight,
                    true,
                );
                if rising_support
                    && (!fixed_contact_reachable || !fixed_contact_within_leg_reach)
                    && let Some(transported_contact) = retarget_unacquired_run_contact_for_descent(
                        previous_owner_target,
                        planted_target,
                        rig_origin,
                        rig_rotation,
                        side,
                        upper_snapshot.global.translation(),
                        final_solve_reach,
                        state_delta_seconds,
                        |xz| terrain.height_at(xz),
                    )
                {
                    // The final pre-contact footprint follows the current
                    // owner displacement once, then becomes the new frozen
                    // world plant atomically. After truthful acquisition the
                    // ordinary direct-plant path keeps this point stationary.
                    planted_target = transported_contact;
                    plant = Some(transported_contact);
                    if left {
                        memory.left_foot_plant = plant;
                        memory.left_planned_contact = Some(transported_contact);
                    } else {
                        memory.right_foot_plant = plant;
                        memory.right_planned_contact = Some(transported_contact);
                    }
                }
                let contact_reachable = run_contact_within_follower_step(
                    previous_owner_target,
                    planted_target,
                    rig_origin,
                    rig_rotation,
                    state_delta_seconds,
                );
                let acquisition_clearance = if contact_reachable {
                    0.0
                } else {
                    ik_tuning().run_swing_minimum_sole_clearance_metres
                };

                advance_run_airborne_world_target(
                    previous_owner_target,
                    planted_target,
                    rig_origin,
                    rig_rotation,
                    state_delta_seconds,
                    ik_tuning().run_airborne_owner_step_metres * ik_tuning().continuity_sample_hz,
                    |xz| {
                        terrain.height_at(xz).map(|height| {
                            height + measured_ankle_sole_offset_metres() + acquisition_clearance
                        })
                    },
                )
            } else if previous_support.is_some_and(terrain_leg_has_support) {
                planted_target
            } else {
                advance_foot_target_at_speed(
                    previous_world_target,
                    planted_target,
                    state_delta_seconds,
                    release_target_speed,
                )
            };
            let unplanned_support_release_owned = unplanned_support_release_is_owned(
                was_releasing,
                previous_support,
                previous_reported_support,
                retained_planned_contact,
                target,
                planted_target,
                foot_position,
            );
            let bounded_unplanned_support_release = support_run_airborne_budget
                && retained_planned_contact.is_none()
                && !plant_acquired
                && unplanned_support_release_owned;
            target = bound_unacquired_run_support_release_target(
                bounded_unplanned_support_release,
                false,
                false,
                true,
                previous_owner_target,
                target,
                rig_origin,
                rig_rotation,
                state_delta_seconds,
                |xz| {
                    terrain
                        .height_at(xz)
                        .map(|height| height + measured_ankle_sole_offset_metres())
                },
            );
            if memory.settle.is_some() && !plant_acquired {
                // The selected settle support can begin airborne. Until its
                // rendered sole truthfully acquires contact it needs the same
                // toe/sole flight floor as the opposite capture foot. Once
                // the toe-aware contact itself fits in this tick's follower
                // and analytic-reach budgets, land atomically; permanently
                // reapplying the flight floor would make truthful acquisition
                // impossible and leave settle stuck at progress 1 forever.
                let contact_candidate = advance_run_airborne_world_target(
                    previous_owner_target,
                    planted_target,
                    rig_origin,
                    rig_rotation,
                    state_delta_seconds,
                    release_target_speed,
                    |xz| {
                        terrain
                            .height_at(xz)
                            .map(|height| height + measured_ankle_sole_offset_metres())
                    },
                );
                let contact_reachable = contact_candidate.distance_squared(planted_target)
                    <= 0.000001
                    && planted_target.distance(upper_snapshot.global.translation())
                        <= terrain_maximum_reach(upper_length, lower_length) + 0.001;
                target = if contact_reachable {
                    planted_target
                } else {
                    advance_run_airborne_world_target(
                        previous_owner_target,
                        planted_target,
                        rig_origin,
                        rig_rotation,
                        state_delta_seconds,
                        release_target_speed,
                        |xz| {
                            let sole_minimum = terrain.height_at(xz).map(|height| {
                                height
                                    + measured_ankle_sole_offset_metres()
                                    + ik_tuning().terrain_transition_flight_toe_clearance_metres
                            });
                            let toe_minimum = rendered_ankle_and_toe.and_then(
                                |(rendered_ankle, rendered_toe)| {
                                    toe_aware_minimum_ankle_y(
                                        rendered_ankle,
                                        rendered_toe,
                                        xz,
                                        transition_toe_clearance_with_rotation_margin(
                                            rendered_ankle,
                                            rendered_toe,
                                            state_delta_seconds,
                                        ),
                                        |sample| terrain.height_at(sample),
                                    )
                                },
                            );
                            sole_minimum.into_iter().chain(toe_minimum).reduce(f32::max)
                        },
                    )
                };
            }
            let release_active = target.distance_squared(planted_target) > 0.000001
                || (!plant_acquired
                    && unplanned_terrain_solve_requires_release(
                        retained_planned_contact,
                        target,
                        foot_position,
                    ));
            let owner_target = rig_rotation.inverse() * (target - rig_origin);
            let desired_owner_target = rig_rotation.inverse() * (planted_target - rig_origin);
            let release_target = support_release_diagnostic_goal(
                release_active,
                bounded_unplanned_support_release,
                owner_target,
                desired_owner_target,
            );
            if left {
                memory.left_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.left_support_weight.is_none() {
                    memory.left_support_weight = Some(weight);
                    memory.left_transition_support_weight = Some(weight);
                }
                memory.left_release_active = release_active;
                memory.left_release_target = release_target;
            } else {
                memory.right_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.right_support_weight.is_none() {
                    memory.right_support_weight = Some(weight);
                    memory.right_transition_support_weight = Some(weight);
                }
                memory.right_release_active = release_active;
                memory.right_release_target = release_target;
            }
            if left {
                memory.left_foot_world_target = Some(target);
            } else {
                memory.right_foot_world_target = Some(target);
            }
            let canonical_pole = canonical_knee_pole(side);
            let canonical_world = pole_to_world(rig_rotation, canonical_pole);
            let (remembered_pole, previous_end_direction) = if left {
                (
                    memory.left_terrain_pole_world,
                    memory.left_terrain_end_direction,
                )
            } else {
                (
                    memory.right_terrain_pole_world,
                    memory.right_terrain_end_direction,
                )
            };
            let next_end_direction =
                (target - upper_snapshot.global.translation()).normalize_or_zero();
            let remembered = transported_terrain_pole(
                remembered_pole,
                previous_end_direction,
                next_end_direction,
                canonical_world,
            );
            let pole = remembered
                .or_else(|| {
                    authored_knee_pole_world(
                        upper_snapshot.global.translation(),
                        lower_snapshot.global.translation(),
                        target,
                        canonical_world,
                    )
                })
                .unwrap_or(canonical_world);
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                upper_snapshot.global.translation(),
                foot_position,
                target,
                pole,
                &parents,
                &transforms.p0(),
            );
            let solve_reach = if skeleton.animation_speed() <= 0.05 {
                terrain_maximum_reach(upper_length, lower_length)
            } else {
                maximum_reach(upper_length, lower_length)
            };
            // The transported pole already provides temporal continuity.
            // Authored-bend preservation happens inside the generic solver
            // after pole selection and can rotate the resulting knee outside
            // the anatomical foot-facing cone, so leg IK must use the final
            // constrained pole without another authored blend.
            let solution = solve_two_bone_with_reach(
                TwoBoneChain::new(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_position,
                    upper_length,
                    lower_length,
                    pole,
                ),
                target,
                solve_reach,
            );
            let mut reported_support_weight = 0.0;
            if let Some(solution) = solution {
                let sole_at_contact = terrain.height_at(solution.end.xz()).is_some_and(|height| {
                    sole_is_at_contact(solution.end.y, height)
                        && plant.is_some_and(|plant| solution.end.xz().distance(plant.xz()) <= 0.02)
                });
                if sole_at_contact {
                    reported_support_weight = weight;
                    if left {
                        memory.left_foot_plant_acquired = true;
                        memory.left_planned_contact_phase_start = None;
                    } else {
                        memory.right_foot_plant_acquired = true;
                        memory.right_planned_contact_phase_start = None;
                    }
                }
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
                // The terrain-feasible waypoint can still lie beyond the
                // current analytic chain. Persist the end the player can
                // actually see and reach, not the rejected pre-solve
                // request, so the next sample continues from the visible
                // ankle and diagnostics report truthful release ownership.
                commit_resolved_unacquired_support_release(
                    &mut memory,
                    left,
                    bounded_unplanned_support_release,
                    solution.end,
                    rig_origin,
                    rig_rotation,
                );
                let bend = (solution.knee - upper_snapshot.global.translation())
                    .reject_from_normalized(solution.end_direction);
                if state_delta_seconds > 0.0
                    && let Some(valid) = bend.try_normalize()
                {
                    if left {
                        memory.left_terrain_pole_world = Some(valid);
                    } else {
                        memory.right_terrain_pole_world = Some(valid);
                    }
                }
                if state_delta_seconds > 0.0 {
                    if left {
                        memory.left_terrain_end_direction = Some(solution.end_direction);
                    } else {
                        memory.right_terrain_end_direction = Some(solution.end_direction);
                    }
                }
            }
            if left {
                memory.left_support_weight = Some(reported_support_weight);
            } else {
                memory.right_support_weight = Some(reported_support_weight);
            }
            // The final rendered-contact result owns orientation authority.
            // Planned acquisition is still airborne until its sole actually
            // reaches the intended contact, so bound that transition after
            // every solve/alignment path rather than inferring it from gait.
            airborne_orientation_owned[leg_index] =
                !terrain_leg_has_support(reported_support_weight);
            if evaluation_advances {
                if left {
                    memory.left_contact_orientation_blend_active = update_contact_orientation_blend(
                        memory.left_contact_orientation_blend_active,
                        previous_support,
                        reported_support_weight,
                    );
                } else {
                    memory.right_contact_orientation_blend_active =
                        update_contact_orientation_blend(
                            memory.right_contact_orientation_blend_active,
                            previous_support,
                            reported_support_weight,
                        );
                }
            }
            if solve_weight > 0.001
                && let Some(normal) = terrain.normal_at(horizontal_target.xz())
                && let Some(sole_axis) = rig.sole_axis(left)
            {
                let cached_chain = if left {
                    memory.left_rotation_chain
                } else {
                    memory.right_rotation_chain
                };
                if evaluation_advances || cached_chain.is_none() {
                    align_foot_to_slope(foot, sole_axis, normal, &parents, &mut transforms);
                }
            }
        }
        finalize_leg_rotation_chains(
            rig,
            skeleton,
            rig_rotation,
            &mut memory,
            evaluation_advances,
            state_delta_seconds,
            airborne_orientation_owned,
            airborne_just_released,
            &parents,
            &mut transforms,
        );
        let safe_settle_fallback = memory.settle.is_some_and(|settle| {
            settle.elapsed_seconds >= 0.75
                && settle_stance_is_safe(
                    projected_body_center(rig, &transforms.p0()).unwrap_or(rig_origin),
                    memory.left_foot_world_target,
                    memory.right_foot_world_target,
                    terrain,
                )
        });
        let settle_requests_completion = (settle_ready_for_contact && settle_contact_reached)
            || safe_settle_fallback
            || settle_is_terminal(&memory);
        if settle_requests_completion {
            // Completion leaves two exact terrain plants for idle. A progress
            // 1 settle with no active followers is terminal even if its last
            // analytic solve stopped above contact; otherwise it can freeze a
            // mid-stride pose forever with neither foot reporting support.
            prepare_terminal_settle_contacts(&mut memory, rig_origin, rig_rotation, |xz| {
                terrain.height_at(xz)
            });
            if terminal_settle_contacts_are_rendered(&memory, |xz| terrain.height_at(xz)) {
                finish_settle_for_idle(&mut memory);
            }
        }
        if let Ok(mut state) = ik_states.get_mut(owner) {
            state.0 = memory;
        } else {
            commands.entity(owner).insert(LegIkState(memory));
        }
    }
}
