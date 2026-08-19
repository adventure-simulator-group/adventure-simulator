use super::*;
use crate::player::LocalBodyFacing;
use std::num::NonZeroU32;

mod body_response;
mod hands;
mod locomotion;
mod quickstep;
mod solver;

pub(in crate::animation) use body_response::apply_locomotion_body_response;
#[cfg(test)]
pub(super) use body_response::body_response_target;
pub(super) use body_response::presentation_tick_delta;
pub(in crate::animation) use hands::apply_arm_and_weapon_constraints;
#[cfg(test)]
pub(super) use hands::secondary_grip_world;
pub(in crate::animation) use locomotion::apply as apply_ordinary_locomotion_ik;
pub(in crate::animation) use quickstep::apply as apply_quickstep_ik;
pub(super) use solver::*;

const MIN_ANATOMICAL_POLE_SINE: f32 = 0.02;
const ANATOMICAL_POLE_FULL_SINE: f32 = 0.05;
const ANATOMICAL_AUTHORITY_RATE: f32 = 12.0;
const ANATOMICAL_POLE_MAXIMUM_ACCELERATION: f32 = 12.0;
const ANATOMICAL_POLE_MAXIMUM_JERK: f32 = 192.0;
const ANATOMICAL_POLE_TRACKING_FREQUENCY: f32 = 6.0;
const KNEE_FACING_HORIZONTAL_AUTHORITY_START: f32 = 0.04;
const KNEE_FACING_HORIZONTAL_AUTHORITY_FULL: f32 = 0.12;
const GUARD_PELVIS_MAXIMUM_ANGULAR_ACCELERATION: f32 = 12.0;
const GUARD_PELVIS_MAXIMUM_ANGULAR_JERK: f32 = 192.0;
const GUARD_PELVIS_MAXIMUM_SCALE_ACCELERATION: f32 = 4.0;
const GUARD_PELVIS_MAXIMUM_SCALE_JERK: f32 = 64.0;

fn anatomical_pole_is_well_conditioned(projected_bend: Vec3, upper_length: f32) -> bool {
    projected_bend.is_finite()
        && upper_length.is_finite()
        && upper_length > 0.0
        && projected_bend.length() > upper_length * MIN_ANATOMICAL_POLE_SINE
}

fn interpolate_anatomical_pole_signed(from: Vec3, to: Vec3, leg_axis: Vec3, weight: f32) -> Vec3 {
    let Some(from) = from.reject_from_normalized(leg_axis).try_normalize() else {
        return to;
    };
    let Some(to) = to.reject_from_normalized(leg_axis).try_normalize() else {
        return from;
    };
    let signed_angle = leg_axis.dot(from.cross(to)).atan2(from.dot(to));
    (Quat::from_axis_angle(leg_axis, signed_angle * weight.clamp(0.0, 1.0)) * from)
        .normalize_or_zero()
}

fn track_anatomical_pole_signed(
    from: Vec3,
    to: Vec3,
    leg_axis: Vec3,
    angular_velocity: f32,
    angular_acceleration: f32,
    delta_seconds: f32,
) -> (Vec3, f32, f32) {
    let Some(from) = from.reject_from_normalized(leg_axis).try_normalize() else {
        return (to, 0.0, 0.0);
    };
    let Some(to) = to.reject_from_normalized(leg_axis).try_normalize() else {
        return (from, angular_velocity, angular_acceleration);
    };
    let dt = delta_seconds.max(f32::EPSILON);
    let cross = leg_axis.dot(from.cross(to));
    let dot = from.dot(to);
    let error = if cross.abs() <= 1.0e-6 && dot < 0.0 && angular_velocity.abs() > 1.0e-6 {
        angular_velocity.signum() * std::f32::consts::PI
    } else {
        cross.atan2(dot)
    };
    let stopping_speed = (2.0 * ANATOMICAL_POLE_MAXIMUM_ACCELERATION * error.abs()).sqrt();
    let desired_velocity =
        error.signum() * (ANATOMICAL_POLE_TRACKING_FREQUENCY * error.abs()).min(stopping_speed);
    let desired_acceleration = ((desired_velocity - angular_velocity) / dt).clamp(
        -ANATOMICAL_POLE_MAXIMUM_ACCELERATION,
        ANATOMICAL_POLE_MAXIMUM_ACCELERATION,
    );
    let jerk = ((desired_acceleration - angular_acceleration) / dt)
        .clamp(-ANATOMICAL_POLE_MAXIMUM_JERK, ANATOMICAL_POLE_MAXIMUM_JERK);
    let next_acceleration = (angular_acceleration + jerk * dt).clamp(
        -ANATOMICAL_POLE_MAXIMUM_ACCELERATION,
        ANATOMICAL_POLE_MAXIMUM_ACCELERATION,
    );
    let next_velocity = angular_velocity + next_acceleration * dt;
    let step = next_velocity * dt;
    (
        (Quat::from_axis_angle(leg_axis, step) * from).normalize_or_zero(),
        next_velocity,
        next_acceleration,
    )
}

/// Final lower-body invariant pass. Pose owners and terrain alignment may
/// choose different targets and foot rotations, but no later presentation
/// stage may leave a rendered knee outside the foot-facing anatomical cone.
pub(in crate::animation) fn enforce_anatomical_knee_yaw(
    clock: Res<ProceduralAnimationClock>,
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut states: Query<&mut LegIkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        // Downed and posture-transition poses deliberately use knee bend
        // planes that need not follow a standing foot-facing cone. Preserve
        // their authored thigh/shin relationship exactly; terrain IK already
        // rejects these postures independently.
        if !anatomical_knee_yaw_posture_is_valid(skeleton) {
            if let Ok(mut state) = states.get_mut(owner) {
                state.0.left_anatomical_pole_angular_velocity = 0.0;
                state.0.right_anatomical_pole_angular_velocity = 0.0;
                state.0.left_anatomical_pole_angular_acceleration = 0.0;
                state.0.right_anatomical_pole_angular_acceleration = 0.0;
            }
            continue;
        }
        let (semantic_tick, semantic_delta_seconds) = clock.semantic_step();
        if states
            .get_mut(owner)
            .is_ok_and(|state| state.0.knee_yaw_evaluation_tick == Some(semantic_tick))
        {
            continue;
        }
        let mut final_offsets = [0.0; 2];
        let mut final_poles = [None, None];
        let mut final_pole_motion = [(0.0, 0.0); 2];
        for (leg_index, (upper_role, lower_role, foot_role, left)) in [
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
        ]
        .into_iter()
        .enumerate()
        {
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
            let hip = upper_snapshot.global.translation();
            let knee = lower_snapshot.global.translation();
            let target = foot_snapshot.global.translation();
            let upper_length = hip.distance(knee);
            let lower_length = knee.distance(target);
            let Some(leg_direction) = (target - hip).try_normalize() else {
                continue;
            };
            let projected_bend = (knee - hip).reject_from_normalized(leg_direction);
            let well_conditioned =
                anatomical_pole_is_well_conditioned(projected_bend, upper_length);
            let current_bend = projected_bend.try_normalize();
            let delta_seconds = semantic_delta_seconds;
            let (
                posture_handoff_weight,
                retained_pole,
                authority,
                angular_velocity,
                angular_acceleration,
            ) = states
                .get_mut(owner)
                .ok()
                .map(|mut state| {
                    let retained = if left {
                        state.0.left_anatomical_pole_world
                    } else {
                        state.0.right_anatomical_pole_world
                    };
                    let mut retained_authority = if left {
                        state.0.left_anatomical_authority
                    } else {
                        state.0.right_anatomical_authority
                    };
                    let bend_ratio = projected_bend.length() / upper_length.max(0.0001);
                    let desired_authority = smoothstep(
                        MIN_ANATOMICAL_POLE_SINE,
                        ANATOMICAL_POLE_FULL_SINE,
                        bend_ratio,
                    );
                    if retained.is_none() && well_conditioned {
                        retained_authority = desired_authority;
                    }
                    let maximum_step = ANATOMICAL_AUTHORITY_RATE * delta_seconds.max(0.0);
                    let authority = retained_authority
                        + (desired_authority - retained_authority)
                            .clamp(-maximum_step, maximum_step);
                    if left {
                        state.0.left_anatomical_authority = authority;
                    } else {
                        state.0.right_anatomical_authority = authority;
                    }
                    let angular_velocity = if left {
                        state.0.left_anatomical_pole_angular_velocity
                    } else {
                        state.0.right_anatomical_pole_angular_velocity
                    };
                    let angular_acceleration = if left {
                        state.0.left_anatomical_pole_angular_acceleration
                    } else {
                        state.0.right_anatomical_pole_angular_acceleration
                    };
                    (
                        state.0.posture_handoff_weight,
                        retained,
                        authority,
                        angular_velocity,
                        angular_acceleration,
                    )
                })
                .unwrap_or((None, None, 0.0, 0.0, 0.0));
            if authority <= f32::EPSILON {
                continue;
            }
            // Near extension, retain the last nonsingular bend plane while
            // authority decays. Never let a tiny projected bend choose a new
            // pole hemisphere on the reacquisition sample.
            let Some(current_bend) = well_conditioned
                .then_some(current_bend)
                .flatten()
                .or(retained_pole)
            else {
                continue;
            };
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                hip,
                target,
                target,
                current_bend,
                &parents,
                &transforms.p0(),
            );
            // Retained world poles belong to the preceding moving leg axis
            // and foot-facing cone. Reproject before temporal tracking so a
            // lagging tracker can never escape the current hard cone.
            let retained_pole = retained_pole.map(|retained| {
                constrain_rendered_leg_pole(
                    rig,
                    left,
                    hip,
                    target,
                    target,
                    retained,
                    &parents,
                    &transforms.p0(),
                )
            });
            let pole = posture_handoff_weight
                .map(|weight| {
                    interpolate_anatomical_pole_signed(
                        retained_pole.unwrap_or(current_bend),
                        pole,
                        leg_direction,
                        weight,
                    )
                })
                .unwrap_or(pole);
            let (pole, pole_velocity, pole_acceleration) =
                retained_pole.map_or((pole, 0.0, 0.0), |retained| {
                    track_anatomical_pole_signed(
                        retained,
                        pole,
                        leg_direction,
                        angular_velocity,
                        angular_acceleration,
                        delta_seconds,
                    )
                });
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                hip,
                target,
                target,
                pole,
                &parents,
                &transforms.p0(),
            );
            final_pole_motion[leg_index] = (pole_velocity, pole_acceleration);
            if let Some(solution) = solve_two_bone_with_reach(
                hip,
                knee,
                target,
                target,
                upper_length,
                lower_length,
                pole,
                upper_length + lower_length,
            ) {
                apply_two_bone_solution_weighted(
                    upper,
                    lower,
                    foot,
                    solution,
                    posture_handoff_weight.unwrap_or(1.0) * authority,
                    &parents,
                    &mut transforms,
                );
                if let Some((final_upper, final_lower, final_foot)) =
                    snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
                    && let Some(end_direction) = (final_foot.global.translation()
                        - final_upper.global.translation())
                    .try_normalize()
                    && let Some(bend_world) = (final_lower.global.translation()
                        - final_upper.global.translation())
                    .reject_from_normalized(end_direction)
                    .try_normalize()
                    && let Some(bend) = bend_world.xz().try_normalize()
                    && let Some(facing) = rendered_foot_facing(
                        rig,
                        left,
                        final_foot.global.translation(),
                        &parents,
                        &transforms.p0(),
                    )
                    .and_then(|facing| facing.xz().try_normalize())
                {
                    final_offsets[leg_index] = bend.angle_to(facing).abs().to_degrees();
                    final_poles[leg_index] = Some(bend_world);
                }
            }
        }
        let final_rotation_chains = {
            let query = transforms.p1();
            [
                local_leg_rotation_chain(rig, true, &query),
                local_leg_rotation_chain(rig, false, &query),
            ]
        };
        if let Ok(mut state) = states.get_mut(owner) {
            state.0.left_knee_foot_yaw_offset_degrees = final_offsets[0];
            state.0.right_knee_foot_yaw_offset_degrees = final_offsets[1];
            state.0.knee_yaw_evaluation_tick = Some(semantic_tick);
            if final_poles[0].is_some() {
                state.0.left_anatomical_pole_world = final_poles[0];
                state.0.left_anatomical_pole_angular_velocity = final_pole_motion[0].0;
                state.0.left_anatomical_pole_angular_acceleration = final_pole_motion[0].1;
            }
            if final_poles[1].is_some() {
                state.0.right_anatomical_pole_world = final_poles[1];
                state.0.right_anatomical_pole_angular_velocity = final_pole_motion[1].0;
                state.0.right_anatomical_pole_angular_acceleration = final_pole_motion[1].1;
            }
            // Anatomical enforcement is downstream of every leg owner. Keep
            // its actual final local chain as the fixed-tick presentation,
            // rather than re-publishing the pre-enforcement terrain chain on
            // a repeated view.
            if final_rotation_chains[0].is_some() {
                state.0.left_rotation_chain = final_rotation_chains[0];
            }
            if final_rotation_chains[1].is_some() {
                state.0.right_rotation_chain = final_rotation_chains[1];
            }
        }
    }
}

fn local_leg_rotation_chain(
    rig: &HumanoidRig,
    left: bool,
    transforms: &Query<&mut Transform>,
) -> Option<LegRotationChain> {
    let (upper_role, lower_role, foot_role) = if left {
        (BoneRole::ThighLeft, BoneRole::ShinLeft, BoneRole::FootLeft)
    } else {
        (
            BoneRole::ThighRight,
            BoneRole::ShinRight,
            BoneRole::FootRight,
        )
    };
    let (upper, lower, foot) = (
        *rig.get(&upper_role)?,
        *rig.get(&lower_role)?,
        *rig.get(&foot_role)?,
    );
    Some(LegRotationChain {
        upper: transforms.get(upper).ok()?.rotation,
        lower: transforms.get(lower).ok()?.rotation,
        foot: transforms.get(foot).ok()?.rotation,
    })
}

pub(super) fn anatomical_knee_yaw_posture_is_valid(skeleton: &SkeletonState) -> bool {
    !skeleton.body().is_downed() && !skeleton.is_posture_transitioning()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LocomotionSettleState {
    support_left: bool,
    swing_start: Vec3,
    capture_point: Vec3,
    landing_target: Vec3,
    progress: f32,
    elapsed_seconds: f32,
    raised_handoff: bool,
    /// The controller chosen at the ownership edge remains the sole settle
    /// owner even after the measured planar speed falls to zero.
    stateful_follower: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlopeAlignmentMode {
    Raised,
    Ordinary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LegRotationChain {
    upper: Quat,
    lower: Quat,
    foot: Quat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct LegIkMemory {
    left_leg: Option<Vec3>,
    right_leg: Option<Vec3>,
    left_terrain_pole_world: Option<Vec3>,
    right_terrain_pole_world: Option<Vec3>,
    left_terrain_end_direction: Option<Vec3>,
    right_terrain_end_direction: Option<Vec3>,
    left_knee_foot_yaw_offset_degrees: f32,
    right_knee_foot_yaw_offset_degrees: f32,
    knee_yaw_evaluation_tick: Option<u64>,
    raised_refresh_evaluation_tick: Option<u64>,
    posture_handoff_weight: Option<f32>,
    left_anatomical_pole_world: Option<Vec3>,
    right_anatomical_pole_world: Option<Vec3>,
    left_anatomical_authority: f32,
    right_anatomical_authority: f32,
    left_anatomical_pole_angular_velocity: f32,
    right_anatomical_pole_angular_velocity: f32,
    left_anatomical_pole_angular_acceleration: f32,
    right_anatomical_pole_angular_acceleration: f32,
    left_rotation_chain: Option<LegRotationChain>,
    right_rotation_chain: Option<LegRotationChain>,
    left_foot_orientation_world: Option<Quat>,
    right_foot_orientation_world: Option<Quat>,
    left_contact_orientation_blend_active: bool,
    right_contact_orientation_blend_active: bool,
    slope_alignment_mode: Option<SlopeAlignmentMode>,
    left_foot_plant: Option<Vec3>,
    right_foot_plant: Option<Vec3>,
    left_foot_plant_acquired: bool,
    right_foot_plant_acquired: bool,
    left_foot_target: Option<Vec3>,
    right_foot_target: Option<Vec3>,
    left_foot_world_target: Option<Vec3>,
    right_foot_world_target: Option<Vec3>,
    left_foot_follower: Option<FootFollowerState>,
    right_foot_follower: Option<FootFollowerState>,
    left_direct_c2_active: bool,
    right_direct_c2_active: bool,
    left_contact_endpoint: Option<Vec3>,
    right_contact_endpoint: Option<Vec3>,
    left_contact_progress: Option<f32>,
    right_contact_progress: Option<f32>,
    left_contact_tick: Option<u64>,
    right_contact_tick: Option<u64>,
    left_contact_initial_lag: Option<f32>,
    right_contact_initial_lag: Option<f32>,
    left_contact_event: Option<ContactMotionEvent>,
    right_contact_event: Option<ContactMotionEvent>,
    left_contact_event_tick: Option<u64>,
    right_contact_event_tick: Option<u64>,
    left_solve_hip: Option<Vec3>,
    right_solve_hip: Option<Vec3>,
    left_solve_upper_length: Option<f32>,
    right_solve_upper_length: Option<f32>,
    left_solve_lower_length: Option<f32>,
    right_solve_lower_length: Option<f32>,
    left_commanded_pole: Option<Vec3>,
    right_commanded_pole: Option<Vec3>,
    left_motion_owner_epoch: u64,
    right_motion_owner_epoch: u64,
    // The quickstep solver writes its final visible landing stance here. The
    // ordinary raised-guard follower consumes it on the first post-action
    // frame instead of reacquiring the authored feet from scratch.
    quickstep_handoff_pending: bool,
    quickstep_guard_stance_held: bool,
    quickstep_left_landing_local: Option<Vec3>,
    quickstep_right_landing_local: Option<Vec3>,
    // The last propagated ankle positions are the last pose the player
    // actually saw. At the start of a stop, FK has already restored the new
    // idle sample before IK runs, so sampling globals in the IK pass would
    // mistake that authored pose for the preceding rendered run pose.
    left_last_rendered_world: Option<Vec3>,
    right_last_rendered_world: Option<Vec3>,
    left_last_rendered_toe_world: Option<Vec3>,
    right_last_rendered_toe_world: Option<Vec3>,
    left_last_rendered_owner: Option<Vec3>,
    right_last_rendered_owner: Option<Vec3>,
    left_last_rendered_foot_rotation_world: Option<Quat>,
    right_last_rendered_foot_rotation_world: Option<Quat>,
    left_authored_world_target: Option<Vec3>,
    right_authored_world_target: Option<Vec3>,
    left_planned_contact: Option<Vec3>,
    right_planned_contact: Option<Vec3>,
    left_planned_contact_start: Option<Vec3>,
    right_planned_contact_start: Option<Vec3>,
    left_planned_contact_phase_start: Option<f32>,
    right_planned_contact_phase_start: Option<f32>,
    left_support_weight: Option<f32>,
    right_support_weight: Option<f32>,
    // Solver ownership is separate from truthful post-propagation contact
    // diagnostics. A rendered miss may report zero without erasing the fact
    // that the next solve must release from the preceding planted chain.
    left_transition_support_weight: Option<f32>,
    right_transition_support_weight: Option<f32>,
    // A guard input can select its authored transition clip one frame before
    // the replicated Raised stance transfers IK ownership. Preserve the last
    // truthful ordinary dual-contact pose through that owner gap; the clip's
    // floating FK ankles are not a new contact state.
    last_dual_support_left_owner: Option<Vec3>,
    last_dual_support_right_owner: Option<Vec3>,
    last_dual_support_pelvis: PelvisFollowerState,
    last_dual_support_valid: bool,
    left_support_exhausted_until_flight: bool,
    right_support_exhausted_until_flight: bool,
    left_release_active: bool,
    right_release_active: bool,
    left_release_target: Option<Vec3>,
    right_release_target: Option<Vec3>,
    pelvis_shift: f32,
    pelvis_shift_velocity: f32,
    pelvis_shift_acceleration: f32,
    // Terminal stop correction is an absolute offset from the local rig-root
    // pose captured when dual-contact convergence begins. Sparse idle clips
    // do not necessarily rewrite that root every tick, so adding the retained
    // ordinary pelvis scalar repeatedly stalls or double-applies correction.
    terminal_contacts_prepared: bool,
    terminal_root_base_translation: Option<Vec3>,
    terminal_reach_shift: f32,
    terminal_reach_target_shift: Option<f32>,
    raised_pelvis_shift: f32,
    raised_pelvis_shift_velocity: f32,
    raised_pelvis_shift_acceleration: f32,
    raised_pelvis_follower_valid: bool,
    raised_pelvis_recovery: Option<PelvisRecoverySegment>,
    terrain_blend: f32,
    rig_origin: Option<Vec3>,
    rig_rotation: Option<Quat>,
    measured_owner_planar_speed: f32,
    evaluation_tick: Option<u64>,
    recent_movement_velocity: Vec3,
    settle: Option<LocomotionSettleState>,
}

fn raised_pelvis_follower_seed(memory: LegIkMemory) -> PelvisFollowerState {
    if memory.raised_pelvis_follower_valid {
        PelvisFollowerState {
            position: memory.raised_pelvis_shift,
            velocity: memory.raised_pelvis_shift_velocity,
            acceleration: memory.raised_pelvis_shift_acceleration,
        }
    } else {
        PelvisFollowerState {
            position: memory.pelvis_shift,
            velocity: memory.pelvis_shift_velocity,
            acceleration: memory.pelvis_shift_acceleration,
        }
    }
}

fn retain_last_dual_support_handoff(
    memory: &mut LegIkMemory,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    if !memory.left_foot_plant_acquired
        || !memory.right_foot_plant_acquired
        || !terrain_leg_has_support(memory.left_support_weight.unwrap_or(0.0))
        || !terrain_leg_has_support(memory.right_support_weight.unwrap_or(0.0))
    {
        return;
    }
    let Some(left) = memory.left_foot_plant.or(memory.left_foot_world_target) else {
        return;
    };
    let Some(right) = memory.right_foot_plant.or(memory.right_foot_world_target) else {
        return;
    };
    memory.last_dual_support_left_owner = Some(rig_rotation.inverse() * (left - rig_origin));
    memory.last_dual_support_right_owner = Some(rig_rotation.inverse() * (right - rig_origin));
    memory.last_dual_support_pelvis = PelvisFollowerState {
        position: memory.pelvis_shift,
        velocity: memory.pelvis_shift_velocity,
        acceleration: memory.pelvis_shift_acceleration,
    };
    memory.last_dual_support_valid = true;
}

fn restore_stationary_guard_handoff(
    memory: &mut LegIkMemory,
    rig_origin: Vec3,
    rig_rotation: Quat,
) -> bool {
    if !memory.last_dual_support_valid {
        return false;
    }
    let (Some(left_local), Some(right_local)) = (
        memory.last_dual_support_left_owner,
        memory.last_dual_support_right_owner,
    ) else {
        return false;
    };
    let left = rig_origin + rig_rotation * left_local;
    let right = rig_origin + rig_rotation * right_local;
    memory.left_foot_plant = Some(left);
    memory.right_foot_plant = Some(right);
    memory.left_foot_world_target = Some(left);
    memory.right_foot_world_target = Some(right);
    memory.left_foot_target = Some(left_local);
    memory.right_foot_target = Some(right_local);
    memory.left_foot_plant_acquired = true;
    memory.right_foot_plant_acquired = true;
    memory.left_support_weight = Some(1.0);
    memory.right_support_weight = Some(1.0);
    memory.left_transition_support_weight = Some(1.0);
    memory.right_transition_support_weight = Some(1.0);
    memory.raised_pelvis_shift = memory.last_dual_support_pelvis.position;
    memory.raised_pelvis_shift_velocity = memory.last_dual_support_pelvis.velocity;
    memory.raised_pelvis_shift_acceleration = memory.last_dual_support_pelvis.acceleration;
    memory.raised_pelvis_follower_valid = true;
    memory.raised_pelvis_recovery = None;
    true
}

fn clear_last_dual_support_handoff(memory: &mut LegIkMemory) {
    memory.last_dual_support_left_owner = None;
    memory.last_dual_support_right_owner = None;
    memory.last_dual_support_pelvis = PelvisFollowerState::default();
    memory.last_dual_support_valid = false;
}

fn cancel_rejected_stationary_pivot(footwork: &mut RaisedFootworkState, presented: Vec3) -> bool {
    let pivot_left = footwork.pivot_left;
    footwork.swing_replan_segment = None;
    footwork.swing_emergency_brake = None;
    footwork.swing_release_owner_active = false;
    footwork.awaiting_step_sequence = false;
    footwork.pivot_active = false;
    footwork.pivot_progress = 0.0;
    if pivot_left {
        footwork.left_plant = presented;
        footwork.left_desired_target = Some(presented);
    } else {
        footwork.right_plant = presented;
        footwork.right_desired_target = Some(presented);
    }
    pivot_left
}

fn clear_ownerless_guard_wait(footwork: &mut RaisedFootworkState) -> bool {
    let recovery_owner_active = footwork.swing_replan_segment.is_some()
        || footwork.swing_emergency_brake.is_some()
        || footwork.swing_release_owner_active
        || support_owner_blocks_cadence(footwork.left_support_release_owner)
        || support_owner_blocks_cadence(footwork.right_support_release_owner)
        || footwork.pending_cadence_edge.is_some();
    if footwork.awaiting_step_sequence && !recovery_owner_active {
        footwork.awaiting_step_sequence = false;
        return true;
    }
    false
}

fn guard_emergency_brake_has_settled(
    brake: EmergencyFootBrake,
    body_relative_recovery_settled: bool,
    velocity: Vec3,
    acceleration: Vec3,
) -> bool {
    if brake.owner_local_ideal.is_some() {
        body_relative_recovery_settled
    } else {
        emergency_brake_is_settled(velocity, acceleration)
    }
}

fn guard_emergency_settlement_awaits_cadence(
    cadence_can_advance: bool,
    body_relative_recovery_settled: bool,
) -> bool {
    cadence_can_advance && !body_relative_recovery_settled
}

fn defer_guard_cadence_edge_for_active_pelvis(
    footwork: &mut RaisedFootworkState,
    pending_swing_left: bool,
    current_sequence: u32,
) -> bool {
    let should_defer = footwork.pelvis_acquisition.is_some_and(|acquisition| {
        acquisition.target.is_some()
            && acquisition.progress > 0.0
            && acquisition.progress < 1.0
            && acquisition.start_sequence != current_sequence
    });
    if !should_defer {
        return false;
    }
    footwork.pending_cadence_edge = Some((pending_swing_left, current_sequence));
    if let Some(acquisition) = footwork.pelvis_acquisition.as_mut() {
        acquisition.start_sequence = current_sequence;
        acquisition.trajectory_signature.sequence = current_sequence;
    }
    true
}

fn defer_guard_cadence_edge_for_contact_recovery(
    footwork: &mut RaisedFootworkState,
    pending_swing_left: bool,
    pending_sequence: u32,
) {
    // Sequence and swing side are one semantic identity. Advancing only the
    // sequence while the previous leg still owns recovery creates a hybrid
    // state which can never be repaired by the normal sequence-delta path.
    // Keep the complete old identity until its typed owner settles, then
    // consume the complete pending identity from the visible feet.
    footwork.pending_cadence_edge = Some((pending_swing_left, pending_sequence));
    footwork.awaiting_step_sequence = true;
}

fn retain_or_defer_guard_cadence_identity(
    footwork: &mut RaisedFootworkState,
    current_swing_left: bool,
    current_sequence: u32,
    awaiting_if_current: bool,
) {
    if footwork.step_sequence != current_sequence || footwork.swing_left != current_swing_left {
        defer_guard_cadence_edge_for_contact_recovery(
            footwork,
            current_swing_left,
            current_sequence,
        );
    } else {
        footwork.awaiting_step_sequence = awaiting_if_current;
    }
}

fn guard_pending_cadence_edge_can_be_consumed(footwork: &RaisedFootworkState) -> bool {
    let completed_contact = footwork
        .swing_replan_segment
        .is_some_and(|segment| segment.end.is_contact() && segment.timing.is_complete());
    let contact_in_flight = footwork
        .swing_replan_segment
        .is_some_and(|segment| segment.end.is_contact() && !segment.timing.is_complete());
    let pelvis_in_flight = footwork
        .pelvis_acquisition
        .is_some_and(|acquisition| acquisition.target.is_some() && acquisition.progress < 1.0);
    let recovery_in_flight = footwork.swing_emergency_brake.is_some()
        || footwork.swing_release_owner_active
        || support_owner_blocks_cadence(footwork.left_support_release_owner)
        || support_owner_blocks_cadence(footwork.right_support_release_owner);
    completed_contact || (!contact_in_flight && !pelvis_in_flight && !recovery_in_flight)
}

fn raised_pelvis_local_scalar_shift(
    rig: &HumanoidRig,
    memory: LegIkMemory,
    parents: &Query<&ChildOf>,
    transforms: &TransformHelper,
) -> Option<Vec3> {
    rig.get(&BoneRole::Pelvis)
        .and_then(|pelvis| parents.get(*pelvis).ok())
        .and_then(|parent| transforms.compute_global_transform(parent.parent()).ok())
        .map(|parent_global| {
            parent_global
                .affine()
                .inverse()
                .transform_vector3(Vec3::Y * memory.raised_pelvis_shift)
        })
}

fn blend_guard_pelvis_transform(start: Transform, target: Transform, progress: f32) -> Transform {
    let weight = quintic_progress(progress);
    Transform {
        translation: start.translation.lerp(target.translation, weight),
        rotation: target.rotation,
        scale: target.scale,
    }
}

fn guard_pelvis_transform_sample(acquisition: GuardPelvisAcquisition) -> Transform {
    let target = acquisition.target.unwrap_or(acquisition.start);
    if acquisition.progress <= 0.0 {
        return acquisition.start;
    }
    if acquisition.progress >= 1.0 {
        return target;
    }
    let weight = quintic_progress(acquisition.progress);
    let translation = guard_pelvis_translation_sample(acquisition).position;
    Transform {
        translation,
        rotation: acquisition.start.rotation.slerp(target.rotation, weight),
        scale: acquisition.start.scale.lerp(target.scale, weight),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct GuardPelvisTrajectorySignature {
    world_velocity: Vec3,
    world_acceleration: Vec3,
    command_velocity: Vec3,
    source_tick: u64,
    presentation_tick: u64,
    sequence: u32,
    body_rotation: Quat,
    body_target_rotation: Quat,
}

fn guard_controller_trajectory_signature(
    skeleton: &SkeletonState,
    body_rotation: Quat,
    body_target_rotation: Quat,
    presentation_tick: u64,
) -> GuardPelvisTrajectorySignature {
    let response = tactical_movement_acceleration_hz_for_guard(WeaponGuardState::Raised);
    let command_velocity = (skeleton.world_velocity + skeleton.world_acceleration / response)
        .clamp_length_max(TACTICAL_GUARD_SPEED_METRES_PER_SECOND);
    GuardPelvisTrajectorySignature {
        world_velocity: skeleton.world_velocity,
        world_acceleration: skeleton.world_acceleration,
        command_velocity,
        source_tick: skeleton.locomotion_sample_tick,
        presentation_tick,
        sequence: skeleton.raised_locomotion().step_sequence(),
        body_rotation,
        body_target_rotation,
    }
}

fn guard_controller_command_changed(
    from: GuardPelvisTrajectorySignature,
    to: GuardPelvisTrajectorySignature,
    _delta_seconds: f32,
) -> bool {
    // Sparse authoritative samples legitimately replace the observed velocity
    // and acceleration while the semantic movement command remains unchanged.
    // Treating that presentation convergence as a new command aborts a freshly
    // admitted contact on its first live packet. The persisted proof still
    // validates its expected hip against the live hip, and every published
    // foot sample must independently remain inside the live warning/hard reach.
    // Presented acceleration is sparse between authoritative snapshots, so
    // reconstructing the command magnitude as `v + a / response` produces a
    // different speed on nearly every presentation tick even while the player
    // holds one direction. Only a material command-direction change is a new
    // semantic path. Magnitude/prediction error is owned by the live hip and
    // reach checks before every published foot sample.
    let from_direction = from.command_velocity.xz().normalize_or_zero();
    let to_direction = to.command_velocity.xz().normalize_or_zero();
    let command_direction_changed = match (from_direction, to_direction) {
        (Vec2::ZERO, Vec2::ZERO) => false,
        (Vec2::ZERO, _) | (_, Vec2::ZERO) => true,
        (from, to) => from.dot(to) < 0.995,
    };
    // Aim/body-facing targets are continuously recomputed presentation input,
    // not a discrete foot-owner command. Invalidating an admitted step for
    // every tiny facing correction can abort it before its first visible
    // sample and leave the swing foot at its old world-space plant. The proof
    // predicts facing for admission, while the mandatory live-hip reach check
    // below every direct sample remains the authority if the actual turn
    // materially departs from that prediction.
    command_direction_changed || from.sequence != to.sequence
}

fn guard_predicted_body_rotation(signature: GuardPelvisTrajectorySignature, seconds: f32) -> Quat {
    let current_yaw = (signature.body_rotation * Vec3::Z)
        .xz()
        .try_normalize()
        .map_or(0.0, |forward| forward.x.atan2(forward.y));
    let target_yaw = (signature.body_target_rotation * Vec3::Z)
        .xz()
        .try_normalize()
        .map_or(current_yaw, |forward| forward.x.atan2(forward.y));
    let mut delta = (target_yaw - current_yaw + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    if (delta + std::f32::consts::PI).abs() <= 1.0e-5 {
        delta = std::f32::consts::PI;
    }
    Quat::from_rotation_y(
        current_yaw
            + delta.clamp(
                -BODY_TURN_SPEED_RADIANS * seconds.max(0.0),
                BODY_TURN_SPEED_RADIANS * seconds.max(0.0),
            ),
    )
}

fn guard_pelvis_translation_sample(acquisition: GuardPelvisAcquisition) -> GuardSwingSample {
    let end = acquisition
        .target
        .map_or(acquisition.start.translation, |target| target.translation);
    guard_boundary_quintic_sample(
        acquisition.start.translation,
        acquisition.start_velocity,
        acquisition.start_acceleration,
        end,
        acquisition.progress,
        acquisition.duration_seconds,
        0.0,
    )
}

fn guard_pelvis_replan_duration(start: Vec3, velocity: Vec3, acceleration: Vec3, end: Vec3) -> f32 {
    for ticks in 1..=1024 {
        let duration = ticks as f32 / CONTINUITY_SAMPLE_HZ;
        if quintic_vector_dynamics_are_bounded(
            start,
            velocity,
            acceleration,
            end,
            duration,
            PELVIS_FOLLOWER_MAXIMUM_ACCELERATION,
            PELVIS_FOLLOWER_MAXIMUM_JERK,
        ) {
            return duration;
        }
    }
    1024.0 / CONTINUITY_SAMPLE_HZ
}

fn guard_pelvis_full_transform_duration(
    start: Transform,
    velocity: Vec3,
    acceleration: Vec3,
    end: Transform,
) -> f32 {
    let translation =
        guard_pelvis_replan_duration(start.translation, velocity, acceleration, end.translation);
    let angle = start.rotation.angle_between(end.rotation);
    let angular = (angle * 5.773_503 / GUARD_PELVIS_MAXIMUM_ANGULAR_ACCELERATION)
        .sqrt()
        .max((angle * 60.0 / GUARD_PELVIS_MAXIMUM_ANGULAR_JERK).cbrt());
    let scale_distance = start.scale.distance(end.scale);
    let scale = (scale_distance * 5.773_503 / GUARD_PELVIS_MAXIMUM_SCALE_ACCELERATION)
        .sqrt()
        .max((scale_distance * 60.0 / GUARD_PELVIS_MAXIMUM_SCALE_JERK).cbrt());
    let required = translation.max(angular).max(scale);
    (required * CONTINUITY_SAMPLE_HZ).ceil().max(1.0) / CONTINUITY_SAMPLE_HZ
}

fn condition_guard_pelvis_acquisition(
    footwork: &mut RaisedFootworkState,
    authored: Transform,
    local_scalar_shift: Option<Vec3>,
    moving: bool,
    sequence: u32,
    trajectory_signature: GuardPelvisTrajectorySignature,
    advances: bool,
    delta_seconds: f32,
) -> Transform {
    if moving && !footwork.was_moving && footwork.pelvis_acquisition.is_none() {
        let mut start = footwork.visible_pelvis_local_transform.unwrap_or(authored);
        if let Some(local_scalar_shift) = local_scalar_shift {
            start.translation -= local_scalar_shift;
        }
        footwork.pelvis_acquisition = Some(GuardPelvisAcquisition {
            start,
            target: None,
            start_sequence: sequence,
            progress: 0.0,
            duration_seconds: GUARD_PELVIS_ACQUISITION_SECONDS,
            advance_authorized: false,
            start_velocity: Vec3::ZERO,
            start_acceleration: Vec3::ZERO,
            trajectory_signature,
        });
    }
    let Some(mut acquisition) = footwork.pelvis_acquisition else {
        return authored;
    };
    let trajectory_changed = guard_controller_command_changed(
        acquisition.trajectory_signature,
        trajectory_signature,
        delta_seconds,
    );
    if acquisition.target.is_some() && trajectory_changed {
        // Controller motion does not change the admitted local translation
        // polynomial. Re-prove the remaining analytic path against the new
        // exact root/facing response without replacing its p/v/a boundary.
        // This avoids restarting the pelvis trajectory on every sparse source
        // correction or phase-varying authored sample.
        acquisition.advance_authorized = false;
    }
    acquisition.trajectory_signature = trajectory_signature;
    if acquisition.target.is_none() && (!moving || sequence != acquisition.start_sequence) {
        acquisition.target = Some(authored);
        acquisition.start_sequence = sequence;
        acquisition.progress = 0.0;
        acquisition.duration_seconds = guard_pelvis_full_transform_duration(
            acquisition.start,
            acquisition.start_velocity,
            acquisition.start_acceleration,
            authored,
        );
        acquisition.trajectory_signature = trajectory_signature;
    }
    if acquisition.target.is_some()
        && acquisition.progress <= 0.0
        && sequence != acquisition.start_sequence
    {
        // Sparse replicated cadence can arrive just after the discrete edge.
        // A not-yet-started owner has an exact rest boundary, so atomically
        // adopt the new authoritative epoch and current authored endpoint.
        // Active owners never take this path and retain their analytic p/v/a.
        acquisition.target = Some(authored);
        acquisition.start_sequence = sequence;
        acquisition.duration_seconds = guard_pelvis_full_transform_duration(
            acquisition.start,
            acquisition.start_velocity,
            acquisition.start_acceleration,
            authored,
        );
        acquisition.trajectory_signature = trajectory_signature;
    }
    let presented = if acquisition.target.is_some() {
        if advances && acquisition.advance_authorized {
            acquisition.progress = (acquisition.progress
                + delta_seconds.max(0.0) / acquisition.duration_seconds.max(f32::EPSILON))
            .min(1.0);
        }
        guard_pelvis_transform_sample(acquisition)
    } else {
        Transform {
            translation: acquisition.start.translation,
            rotation: acquisition.start.rotation,
            scale: acquisition.start.scale,
        }
    };
    if let Some(target) = acquisition.target.filter(|_| acquisition.progress >= 1.0) {
        if target.translation == authored.translation
            && target.rotation == authored.rotation
            && target.scale == authored.scale
        {
            footwork.pelvis_acquisition = None;
        } else {
            // Authored locomotion is phase-varying. Only retire on exact
            // endpoint equality; otherwise begin another rest-to-rest C2
            // segment from the actually presented terminal transform. This
            // preserves p/v/a across the retarget instead of snapping from a
            // stale snapshot to the live clip on the next frame.
            footwork.pelvis_acquisition = Some(GuardPelvisAcquisition {
                start: target,
                target: Some(authored),
                start_sequence: sequence,
                progress: 0.0,
                duration_seconds: guard_pelvis_full_transform_duration(
                    target,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    authored,
                ),
                advance_authorized: false,
                start_velocity: Vec3::ZERO,
                start_acceleration: Vec3::ZERO,
                trajectory_signature,
            });
        }
    } else {
        footwork.pelvis_acquisition = Some(acquisition);
    }
    presented
}

fn authorize_guard_pelvis_acquisition(footwork: &mut RaisedFootworkState, reach_admitted: bool) {
    let Some(acquisition) = footwork.pelvis_acquisition.as_mut() else {
        return;
    };
    acquisition.advance_authorized = reach_admitted && acquisition.target.is_some();
}

fn advance_guard_pelvis_scalar_semantic_ticks(
    mut current: PelvisFollowerState,
    recovery: &mut Option<PelvisRecoverySegment>,
    desired: f32,
    semantic_delta_seconds: f32,
) -> PelvisFollowerState {
    if semantic_delta_seconds <= 0.0 {
        return current;
    }
    let ticks = (semantic_delta_seconds * CONTINUITY_SAMPLE_HZ)
        .round()
        .max(1.0) as u32;
    for _ in 0..ticks {
        current = advance_pelvis_follower_with_recovery(
            current,
            recovery,
            desired,
            1.0 / CONTINUITY_SAMPLE_HZ,
        );
    }
    current
}

fn guard_pelvis_segment_fits_remaining_cadence_ticks(
    acquisition: GuardPelvisAcquisition,
    cadence_seconds_remaining: f32,
) -> bool {
    // The current semantic evaluation is an inclusive presentation sample.
    // A cadence edge observed with a small phase overshoot still owns the
    // ceil-counted sample that lands on that authoritative edge.
    let available_ticks = (cadence_seconds_remaining.max(0.0) * CONTINUITY_SAMPLE_HZ
        - f32::EPSILON * 32.0)
        .ceil()
        .max(0.0) as u32;
    let required_ticks = (acquisition.duration_seconds
        * (1.0 - acquisition.progress).clamp(0.0, 1.0)
        * CONTINUITY_SAMPLE_HZ
        - f32::EPSILON * 32.0)
        .ceil()
        .max(0.0) as u32;
    required_ticks <= available_ticks
}

fn reset_terrain_ik_preserving_anatomical_evaluation(memory: &mut LegIkMemory) {
    let evaluation_tick = memory.evaluation_tick;
    let knee_yaw_evaluation_tick = memory.knee_yaw_evaluation_tick;
    let left_knee_foot_yaw_offset_degrees = memory.left_knee_foot_yaw_offset_degrees;
    let right_knee_foot_yaw_offset_degrees = memory.right_knee_foot_yaw_offset_degrees;
    let left_anatomical_pole_world = memory.left_anatomical_pole_world;
    let right_anatomical_pole_world = memory.right_anatomical_pole_world;
    let left_anatomical_pole_angular_velocity = memory.left_anatomical_pole_angular_velocity;
    let right_anatomical_pole_angular_velocity = memory.right_anatomical_pole_angular_velocity;
    let left_anatomical_pole_angular_acceleration =
        memory.left_anatomical_pole_angular_acceleration;
    let right_anatomical_pole_angular_acceleration =
        memory.right_anatomical_pole_angular_acceleration;
    *memory = LegIkMemory {
        evaluation_tick,
        knee_yaw_evaluation_tick,
        left_knee_foot_yaw_offset_degrees,
        right_knee_foot_yaw_offset_degrees,
        left_anatomical_pole_world,
        right_anatomical_pole_world,
        left_anatomical_pole_angular_velocity,
        right_anatomical_pole_angular_velocity,
        left_anatomical_pole_angular_acceleration,
        right_anatomical_pole_angular_acceleration,
        ..default()
    };
}

#[cfg(test)]
fn discard_quickstep_contact_handoff(memory: &mut LegIkMemory) {
    memory.quickstep_handoff_pending = false;
    memory.quickstep_guard_stance_held = false;
    memory.quickstep_left_landing_local = None;
    memory.quickstep_right_landing_local = None;
    memory.left_foot_plant = None;
    memory.right_foot_plant = None;
    memory.left_foot_plant_acquired = false;
    memory.right_foot_plant_acquired = false;
    memory.left_foot_target = None;
    memory.right_foot_target = None;
    memory.left_foot_world_target = None;
    memory.right_foot_world_target = None;
    memory.left_authored_world_target = None;
    memory.right_authored_world_target = None;
    memory.left_support_weight = None;
    memory.right_support_weight = None;
}

#[derive(Debug, Clone, Copy, Default)]
struct ArmIkMemory {
    left_arm: Option<Vec3>,
    right_arm: Option<Vec3>,
}

/// One 64 Hz semantic presentation clock shared by gameplay and deterministic
/// tools. Render frequency may evaluate a semantic tick zero or several times;
/// procedural owners advance only when this clock advances.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct ProceduralAnimationClock {
    fixed_tick: Option<(u64, f32)>,
    gameplay_tick: u64,
    gameplay_accumulator: f32,
    gameplay_step: (u64, f32),
}

impl Default for ProceduralAnimationClock {
    fn default() -> Self {
        Self {
            fixed_tick: None,
            gameplay_tick: 0,
            gameplay_accumulator: 0.0,
            gameplay_step: (0, 0.0),
        }
    }
}

impl ProceduralAnimationClock {
    pub(crate) fn diagnostic_snapshot(&self) -> serde_json::Value {
        let (semantic_tick, semantic_delta_seconds) = self.semantic_step();
        serde_json::json!({
            "semantic_tick": semantic_tick,
            "semantic_delta_seconds": semantic_delta_seconds,
            "semantic_advance_ticks": (semantic_delta_seconds * CONTINUITY_SAMPLE_HZ).round() as u64,
            "fixed_tick": self.fixed_tick.map(|step| step.0),
            "gameplay_tick": self.gameplay_tick,
            "gameplay_accumulator_seconds": self.gameplay_accumulator,
        })
    }

    #[allow(dead_code)] // Used by the standalone animation viewer and unit fixtures.
    pub(crate) fn set_fixed_tick(&mut self, tick: u64, delta_seconds: f32) {
        self.fixed_tick = Some((tick, delta_seconds.max(0.0)));
    }

    pub(crate) fn fixed_step(&self) -> Option<(u64, f32)> {
        self.fixed_tick
    }

    pub(crate) fn semantic_step(&self) -> (u64, f32) {
        self.fixed_tick.unwrap_or(self.gameplay_step)
    }

    pub(crate) fn advance_gameplay(&mut self, delta_seconds: f32) {
        if self.fixed_tick.is_some() {
            return;
        }
        self.gameplay_accumulator += delta_seconds.max(0.0).min(0.25);
        let ticks = (self.gameplay_accumulator * CONTINUITY_SAMPLE_HZ + 0.000_01).floor() as u64;
        if ticks == 0 {
            self.gameplay_step = (self.gameplay_tick, 0.0);
            return;
        }
        self.gameplay_accumulator -= ticks as f32 / CONTINUITY_SAMPLE_HZ;
        self.gameplay_tick = self.gameplay_tick.wrapping_add(ticks);
        self.gameplay_step = (self.gameplay_tick, ticks as f32 / CONTINUITY_SAMPLE_HZ);
    }
}

pub(crate) fn advance_procedural_animation_clock(
    time: Res<Time>,
    mut clock: ResMut<ProceduralAnimationClock>,
) {
    clock.advance_gameplay(time.delta_secs());
}

fn repeated_fixed_tick_skips_ik(fixed_tick: bool, evaluation_advances: bool) -> bool {
    fixed_tick && !evaluation_advances
}

pub(super) const MIN_INTER_FOOT_SEPARATION: f32 = 0.16;
// Cascadeur's final ankle bones sit about 15 mm inside analytic targets after
// the complete hierarchy solve. Keep a measured planning allowance so the
// rendered bones, not merely abstract targets, retain the 0.16 m contract.
pub(super) const GUARD_TARGET_INTER_FOOT_SEPARATION: f32 = MIN_INTER_FOOT_SEPARATION + 0.04;
pub(super) const FOOT_TRACK_INNER: f32 = MIN_INTER_FOOT_SEPARATION * 0.5;
pub(super) const FOOT_TRACK_OUTER: f32 = 0.55;
const MAX_PLANT_DISCONTINUITY: f32 = 2.0;
const MAX_OWNER_TRANSLATION_PER_TICK: f32 = 0.5;
// A player can legitimately snap-turn by 90 degrees in one input sample. Only
// discard retained plants for rotations that are unmistakably teleport-like.
const MAX_OWNER_ROTATION_PER_TICK_DEGREES: f32 = 120.0;
// A two-bone knee can travel slightly more than twice as far as its ankle
// target near extension. Derive the release cap from that conservative bound
// and retain two percent of numerical margin below the viewer's 0.10 m
// contract at 64 Hz.
const MAX_KNEE_TARGET_AMPLIFICATION: f32 = 2.05;
const MAX_KNEE_STEP_METRES: f32 = 0.10;
const CONTINUITY_SAMPLE_HZ: f32 = 64.0;
const RUN_AIRBORNE_OWNER_TARGET_SPEED: f32 = 0.0875 * CONTINUITY_SAMPLE_HZ;
const RUN_FIRST_RELEASE_OWNER_TARGET_SPEED: f32 = 0.094 * CONTINUITY_SAMPLE_HZ;
const AIRBORNE_RELEASE_TARGET_SPEED: f32 =
    MAX_KNEE_STEP_METRES * CONTINUITY_SAMPLE_HZ / MAX_KNEE_TARGET_AMPLIFICATION * 0.98;
// Returning the raised pelvis consumes about 2 cm of the knee's 10 cm frame
// budget. Reserve that motion only for the raised-to-settle handoff; ordinary
// swing and settle targets retain the faster general cap.
const RAISED_SETTLE_PELVIS_KNEE_BUDGET_METRES: f32 = 0.02;
const RAISED_SETTLE_TARGET_SPEED: f32 =
    (MAX_KNEE_STEP_METRES - RAISED_SETTLE_PELVIS_KNEE_BUDGET_METRES) * CONTINUITY_SAMPLE_HZ
        / MAX_KNEE_TARGET_AMPLIFICATION
        * 0.98;
// Normal raised-guard cadence peaks at 11.93 centimetres of world-space foot
// travel per 64 Hz sample at the controller's two metre/second speed (8.83 cm
// relative to the moving root). This ceiling therefore leaves ordinary steps
// deadline-driven while bounding unusually long post-attack recovery steps
// instead of teleporting them.
const GUARD_PIVOT_TRIGGER_METRES: f32 = 0.04;
// A stationary stance correction is already a complete rest-to-rest motion
// owner. Give its largest normal corridor correction enough time to remain
// below the same 24/384 ankle-space limits as moving guard contacts, then
// present its analytic samples directly instead of filtering it a second time.
const GUARD_PIVOT_STEP_SECONDS: f32 = 0.40;
const GUARD_PELVIS_ACQUISITION_SECONDS: f32 = 0.32;
// Guard, attack, block, and quickstep endpoints drive a nearly extended
// two-bone chain, so the ankle-space follower's general 72/1152 budget can be
// amplified into an implausible knee step. Admit their fixed C2 segments with
// a lower route-specific budget; if a contact deadline cannot accommodate it,
// retain a duration-extended release owner instead. Run remains on the
// error-responsive stateful follower.
// Swing trajectories have to overtake a moving hip within one half-step. The
// safety contact below is typically a short morphology-scale catch-up step;
// 24/384 could not move it before hard reach even though the general recovery
// follower already lawfully supports 72/1152. Keep a lower guard-specific
// budget, but leave enough authority for a grounded catch-up contact.
// A biped has only one remaining support while the other foot swings. Guard
// contact motion therefore needs the same emergency-safe envelope as the
// general foot follower: a deliberately lower route budget can make a lawful
// contact mathematically slower than the planted leg's remaining reach. The
// analytic C2 owner still proves these bounds for every admitted curve.
const GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION: f32 = 72.0;
// A guard foot advances roughly two body steps between alternate contacts.
// At walk speed that is about 0.85 m in 18 semantic ticks on the production
// rabbit rig. The general follower's 1,152 m/s^3 recovery limit cannot make
// that ordinary cadence deadline and used to extend the contact into the next
// half-step, leaving the old support behind the moving body. Direct C2 gait
// owners instead share the controller command's jerk envelope: the complete
// polynomial is still proved before publication and acceleration remains
// capped at 72 m/s^2.
const GUARD_ACTION_SEGMENT_MAXIMUM_JERK: f32 = 3072.0;
const GUARD_FORCED_RELEASE_SECONDS: f32 = 0.18;
const GUARD_PIVOT_LIFT_METRES: f32 = 0.08;
const KNEE_POLE_MAX_FOOT_FACING_OFFSET_RADIANS: f32 = std::f32::consts::FRAC_PI_8;
// A 576 degree/second cap is nine degrees at the 64 Hz presentation
// cadence, retaining numeric margin below the ten-degree review gate. Contact
// and swing orientation share this boundary so terrain
// alignment can never introduce the old one-frame ankle snap.
const AIRBORNE_FOOT_ROTATION_SPEED_DEGREES: f32 = 576.0;
const FIRST_RUN_RELEASE_FOOT_ROTATION_SPEED_DEGREES: f32 = 0.0;
const MAX_RETAINED_PLANT_REACH_CORRECTION: f32 = 0.015;
// Preserve a little margin below the viewer's 2 cm pelvis-step contract.
const PELVIS_CORRECTION_SPEED: f32 = 1.2;
const RUN_PELVIS_CORRECTION_SPEED: f32 = 0.4;
pub(super) const MAX_PELVIS_CORRECTION_STEP: f32 = 0.05;
const TERRAIN_IK_BLEND_SPEED: f32 = 4.0;
const MIN_KNEE_FLEXION: f32 = 20.0_f32.to_radians();
const MIN_TERRAIN_KNEE_FLEXION: f32 = 12.0_f32.to_radians();
// Keep the normal knee reserve while a landing visibly carries weight, then
// release it before the pelvis reaches the authored height. The released
// reach remains capped at the authored leg extension, preventing a final
// recovery-frame foot lift or snap without introducing a straight-leg target.
const LANDING_KNEE_RESERVE_RELEASE_COMPRESSION: f32 = 0.012;
const LANDING_KNEE_RESERVE_FULL_COMPRESSION: f32 = 0.04;
/// Measured vertical distance from the Cascadeur ankle bone to its sole.
pub(crate) const MEASURED_ANKLE_SOLE_OFFSET_METRES: f32 = 0.085;
/// Maximum rendered ankle-to-terrain residual that still represents sole
/// contact after the complete analytic and scene-hierarchy solve.
pub(crate) const SOLE_CONTACT_TOLERANCE_METRES: f32 = 0.01;
const RAISED_SUPPORT_RETENTION_HYSTERESIS_METRES: f32 = 0.006;
const SWING_SOLE_CLEARANCE_METRES: f32 = 0.02;
const RUN_SWING_SOLE_CLEARANCE_METRES: f32 = 0.08;
const TERRAIN_TRANSITION_FLIGHT_TOE_CLEARANCE_METRES: f32 = 0.011;
const TERRAIN_CONTACT_TOE_CLEARANCE_METRES: f32 = -0.009;
const RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES: f32 = 0.051;
const RUN_CONTACT_APPROACH_PHASE: f32 = 0.95;
const RUN_CONTACT_CHAIN_SETTLE_PHASE: f32 = 0.18;
const RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP: f32 = 0.25;
const LATE_RUN_CONTACT_PLAN_PHASE: f32 = 0.5;
// A late-created plan must not compress a full stride into the few samples
// left before support entry. Keep target motion relative to the advancing
// body below the measured knee-singularity budget; ordinary full-swing plans
// retain their desired footprint because their available budget is larger.
const MAX_RUN_SWING_ROOT_RELATIVE_STEP_METRES: f32 = 0.068;
const SETTLE_STEP_SECONDS: f32 = 0.28;
const SETTLE_STEP_CLEARANCE_METRES: f32 = 0.10;
const SETTLE_CAPTURE_POINT_MARGIN_METRES: f32 = 0.12;
const ASSUMED_COM_HEIGHT_METRES: f32 = 1.0;
const MAX_SETTLE_CAPTURE_SPEED: f32 = 1.1;
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LegIkState(LegIkMemory);

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub(crate) struct LegIkDiagnostics {
    pub pelvis_shift: f32,
    pub pelvis_shift_velocity: f32,
    pub pelvis_shift_acceleration: f32,
    pub raised_pelvis_shift: f32,
    pub raised_pelvis_shift_velocity: f32,
    pub raised_pelvis_shift_acceleration: f32,
    pub raised_pelvis_follower_valid: bool,
    pub raised_pelvis_recovery_active: bool,
    pub raised_pelvis_recovery_progress: Option<f32>,
    pub left_authored_target: Option<Vec3>,
    pub right_authored_target: Option<Vec3>,
    pub left_planned_contact: Option<Vec3>,
    pub right_planned_contact: Option<Vec3>,
    pub settle_capture_point: Option<Vec3>,
    pub left_solve_target: Option<Vec3>,
    pub right_solve_target: Option<Vec3>,
    pub left_support_weight: f32,
    pub right_support_weight: f32,
    pub left_release_active: bool,
    pub right_release_active: bool,
    pub left_release_target: Option<Vec3>,
    pub right_release_target: Option<Vec3>,
    pub settle_progress: Option<f32>,
    pub left_knee_foot_yaw_offset_degrees: f32,
    pub right_knee_foot_yaw_offset_degrees: f32,
    pub left_presented_motion: Option<FootMotionDiagnostic>,
    pub right_presented_motion: Option<FootMotionDiagnostic>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FootMotionOwnerKind {
    #[default]
    None,
    StatefulFollower,
    GuardCadence,
    AdmittedC2,
    EmergencyRecovery,
    GroundSafetySlide,
    TerminalHold,
    ReleaseHandoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContactMotionEvent {
    Promised,
    AbortedLiveReach { aborted_owner_epoch: u64 },
    Completed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub(crate) struct FootMotionDiagnostic {
    pub owner: FootMotionOwnerKind,
    pub presented: Vec3,
    pub velocity: Vec3,
    pub acceleration: Vec3,
    pub commanded: Option<Vec3>,
    pub pole: Option<Vec3>,
    pub solve_hip: Option<Vec3>,
    pub upper_length: Option<f32>,
    pub lower_length: Option<f32>,
    pub commanded_pole: Option<Vec3>,
    pub pole_tracking_active: bool,
    pub pole_angular_velocity: f32,
    pub pole_angular_acceleration: f32,
    pub maximum_acceleration: f32,
    pub maximum_jerk: f32,
    pub maximum_pole_angular_acceleration: f32,
    pub maximum_pole_angular_jerk: f32,
    pub contact_endpoint: Option<Vec3>,
    pub contact_progress: Option<f32>,
    pub contact_tick: Option<u64>,
    pub permitted_contact_lag: Option<f32>,
    pub owner_epoch: u64,
    pub contact_event: Option<ContactMotionEvent>,
}

impl LegIkState {
    pub(crate) fn diagnostics(&self) -> LegIkDiagnostics {
        let settle = self.0.settle;

        LegIkDiagnostics {
            pelvis_shift: self.0.pelvis_shift,
            pelvis_shift_velocity: self.0.pelvis_shift_velocity,
            pelvis_shift_acceleration: self.0.pelvis_shift_acceleration,
            raised_pelvis_shift: self.0.raised_pelvis_shift,
            raised_pelvis_shift_velocity: self.0.raised_pelvis_shift_velocity,
            raised_pelvis_shift_acceleration: self.0.raised_pelvis_shift_acceleration,
            raised_pelvis_follower_valid: self.0.raised_pelvis_follower_valid,
            raised_pelvis_recovery_active: self.0.raised_pelvis_recovery.is_some(),
            raised_pelvis_recovery_progress: self
                .0
                .raised_pelvis_recovery
                .map(PelvisRecoverySegment::progress),
            left_authored_target: self.0.left_authored_world_target,
            right_authored_target: self.0.right_authored_world_target,
            left_planned_contact: settle
                .filter(|state| !state.support_left)
                .map(|state| state.landing_target)
                .or(self.0.left_planned_contact),
            right_planned_contact: settle
                .filter(|state| state.support_left)
                .map(|state| state.landing_target)
                .or(self.0.right_planned_contact),
            settle_capture_point: settle.map(|state| state.capture_point),
            left_solve_target: self.0.left_foot_world_target,
            right_solve_target: self.0.right_foot_world_target,
            left_support_weight: self.0.left_support_weight.unwrap_or(0.0),
            right_support_weight: self.0.right_support_weight.unwrap_or(0.0),
            left_release_active: self.0.left_release_active,
            right_release_active: self.0.right_release_active,
            left_release_target: self
                .0
                .left_release_target
                .and_then(|target| Some(self.0.rig_origin? + self.0.rig_rotation? * target)),
            right_release_target: self
                .0
                .right_release_target
                .and_then(|target| Some(self.0.rig_origin? + self.0.rig_rotation? * target)),
            settle_progress: settle.map(|state| state.progress),
            left_knee_foot_yaw_offset_degrees: self.0.left_knee_foot_yaw_offset_degrees,
            right_knee_foot_yaw_offset_degrees: self.0.right_knee_foot_yaw_offset_degrees,
            left_presented_motion: self.0.left_foot_follower.map(|state| FootMotionDiagnostic {
                owner: if self.0.left_direct_c2_active {
                    FootMotionOwnerKind::AdmittedC2
                } else {
                    FootMotionOwnerKind::StatefulFollower
                },
                presented: state.position,
                velocity: state.velocity,
                acceleration: state.acceleration,
                commanded: self.0.left_foot_world_target,
                pole: self.0.left_anatomical_pole_world,
                solve_hip: self.0.left_solve_hip,
                upper_length: self.0.left_solve_upper_length,
                lower_length: self.0.left_solve_lower_length,
                commanded_pole: self.0.left_commanded_pole,
                pole_tracking_active: self.0.left_anatomical_pole_world.is_some()
                    && self.0.left_commanded_pole.is_some(),
                pole_angular_velocity: self.0.left_anatomical_pole_angular_velocity,
                pole_angular_acceleration: self.0.left_anatomical_pole_angular_acceleration,
                maximum_acceleration: if self.0.left_direct_c2_active {
                    GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION
                } else {
                    FOOT_FOLLOWER_MAXIMUM_ACCELERATION
                },
                maximum_jerk: if self.0.left_direct_c2_active {
                    GUARD_ACTION_SEGMENT_MAXIMUM_JERK
                } else {
                    FOOT_FOLLOWER_MAXIMUM_JERK
                },
                maximum_pole_angular_acceleration: ANATOMICAL_POLE_MAXIMUM_ACCELERATION,
                maximum_pole_angular_jerk: ANATOMICAL_POLE_MAXIMUM_JERK,
                contact_endpoint: self.0.left_contact_endpoint,
                contact_progress: self.0.left_contact_progress,
                contact_tick: self.0.left_contact_tick,
                permitted_contact_lag: self
                    .0
                    .left_contact_endpoint
                    .zip(self.0.left_contact_progress)
                    .map(|(endpoint, progress)| {
                        self.0
                            .left_contact_initial_lag
                            .unwrap_or_else(|| state.previous_ideal.distance(endpoint))
                            * (1.0 - quintic_progress(progress))
                    }),
                owner_epoch: self.0.left_motion_owner_epoch,
                contact_event: self.0.left_contact_event,
            }),
            right_presented_motion: self
                .0
                .right_foot_follower
                .map(|state| FootMotionDiagnostic {
                    owner: if self.0.right_direct_c2_active {
                        FootMotionOwnerKind::AdmittedC2
                    } else {
                        FootMotionOwnerKind::StatefulFollower
                    },
                    presented: state.position,
                    velocity: state.velocity,
                    acceleration: state.acceleration,
                    commanded: self.0.right_foot_world_target,
                    pole: self.0.right_anatomical_pole_world,
                    solve_hip: self.0.right_solve_hip,
                    upper_length: self.0.right_solve_upper_length,
                    lower_length: self.0.right_solve_lower_length,
                    commanded_pole: self.0.right_commanded_pole,
                    pole_tracking_active: self.0.right_anatomical_pole_world.is_some()
                        && self.0.right_commanded_pole.is_some(),
                    pole_angular_velocity: self.0.right_anatomical_pole_angular_velocity,
                    pole_angular_acceleration: self.0.right_anatomical_pole_angular_acceleration,
                    maximum_acceleration: if self.0.right_direct_c2_active {
                        GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION
                    } else {
                        FOOT_FOLLOWER_MAXIMUM_ACCELERATION
                    },
                    maximum_jerk: if self.0.right_direct_c2_active {
                        GUARD_ACTION_SEGMENT_MAXIMUM_JERK
                    } else {
                        FOOT_FOLLOWER_MAXIMUM_JERK
                    },
                    maximum_pole_angular_acceleration: ANATOMICAL_POLE_MAXIMUM_ACCELERATION,
                    maximum_pole_angular_jerk: ANATOMICAL_POLE_MAXIMUM_JERK,
                    contact_endpoint: self.0.right_contact_endpoint,
                    contact_progress: self.0.right_contact_progress,
                    contact_tick: self.0.right_contact_tick,
                    permitted_contact_lag: self
                        .0
                        .right_contact_endpoint
                        .zip(self.0.right_contact_progress)
                        .map(|(endpoint, progress)| {
                            self.0
                                .right_contact_initial_lag
                                .unwrap_or_else(|| state.previous_ideal.distance(endpoint))
                                * (1.0 - quintic_progress(progress))
                        }),
                    owner_epoch: self.0.right_motion_owner_epoch,
                    contact_event: self.0.right_contact_event,
                }),
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct ArmIkState(ArmIkMemory);

/// Client-only world-space plants for combat-stance locomotion. The replicated
/// skeleton chooses cadence and direction; exact feet remain presentation
/// state so they never become tactical authority.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RaisedFootworkState {
    pub(crate) initialized: bool,
    was_moving: bool,
    awaiting_step_sequence: bool,
    half_step: u8,
    lead: LeadFoot,
    swing_left: bool,
    step_origin: Vec3,
    step_rotation: Quat,
    swing_stance_local: Vec3,
    swing_start: Vec3,
    swing_end: Vec3,
    swing_replan_segment: Option<PlannedGuardFootSegment>,
    swing_release_owner_active: bool,
    swing_emergency_brake: Option<EmergencyFootBrake>,
    pending_cadence_edge: Option<(bool, u32)>,
    left_plant: Vec3,
    right_plant: Vec3,
    evaluation_tick: Option<u64>,
    step_sequence: u32,
    pub(crate) left_support_weight: f32,
    pub(crate) right_support_weight: f32,
    pub(crate) left_solve_target: Option<Vec3>,
    pub(crate) right_solve_target: Option<Vec3>,
    left_command_target: Option<Vec3>,
    right_command_target: Option<Vec3>,
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
    left_support_release_owner: Option<SupportReleaseOwner>,
    right_support_release_owner: Option<SupportReleaseOwner>,
    pub(super) release_handoff_active: bool,
    release_handoff_progress: f32,
    release_left_start: Vec3,
    release_right_start: Vec3,
    visible_pelvis_owner_local: Option<Vec3>,
    visible_pelvis_local_transform: Option<Transform>,
    pelvis_acquisition: Option<GuardPelvisAcquisition>,
    release_pelvis_offset_owner: Vec3,
    pivot_active: bool,
    pivot_left: bool,
    pivot_progress: f32,
    pivot_origin: Vec3,
    pivot_start: Vec3,
    pivot_end: Vec3,
    left_knee_bend_world: Option<Vec3>,
    right_knee_bend_world: Option<Vec3>,
    left_end_direction: Option<Vec3>,
    right_end_direction: Option<Vec3>,
    left_hip_world: Option<Vec3>,
    right_hip_world: Option<Vec3>,
    left_hip_velocity: Vec3,
    right_hip_velocity: Vec3,
    left_solve_hip: Option<Vec3>,
    right_solve_hip: Option<Vec3>,
    left_solve_upper_length: Option<f32>,
    right_solve_upper_length: Option<f32>,
    left_solve_lower_length: Option<f32>,
    right_solve_lower_length: Option<f32>,
    left_commanded_pole: Option<Vec3>,
    right_commanded_pole: Option<Vec3>,
    left_contact_abort_event: Option<u64>,
    right_contact_abort_event: Option<u64>,
    left_motion_owner_epoch: u64,
    right_motion_owner_epoch: u64,
    left_motion_owner_kind: FootMotionOwnerKind,
    right_motion_owner_kind: FootMotionOwnerKind,
    raised_motion_owned_this_tick: bool,
    contact_gait: Option<ContactDrivenGuardGait>,
}

#[derive(Debug, Clone, Copy)]
struct ContactDrivenGuardGait {
    left_plant: Vec3,
    right_plant: Vec3,
    swing_active: bool,
    swing_left: bool,
    swing_start: Vec3,
    swing_target: Vec3,
    progress: f32,
    last_tick: u64,
}

#[derive(Debug, Clone, Copy)]
struct GuardPelvisAcquisition {
    start: Transform,
    start_velocity: Vec3,
    start_acceleration: Vec3,
    target: Option<Transform>,
    start_sequence: u32,
    progress: f32,
    duration_seconds: f32,
    advance_authorized: bool,
    trajectory_signature: GuardPelvisTrajectorySignature,
}

#[derive(Debug, Clone, Copy)]
struct GuardHipPathProof {
    start_tick: u64,
    contact_tick: u64,
    sequence: u32,
    swing_left: bool,
    accepts_preemptive_cadence_confirmation: bool,
    trajectory_signature: GuardPelvisTrajectorySignature,
    rig_origin: Vec3,
    pelvis_parent_affine: Option<Affine3A>,
    hip_in_pelvis: Option<Vec3>,
    fixed_hip: Vec3,
    pelvis_acquisition: Option<GuardPelvisAcquisition>,
    scalar_start: PelvisFollowerState,
    scalar_recovery: Option<PelvisRecoverySegment>,
    scalar_desired: f32,
    warning_reach: f32,
    hard_reach: f32,
}

impl GuardHipPathProof {
    fn sample(self, semantic_tick: u64) -> Option<Vec3> {
        if semantic_tick < self.start_tick || semantic_tick > self.contact_tick {
            return None;
        }
        let elapsed_ticks = semantic_tick.saturating_sub(self.start_tick) as u32;
        let seconds = elapsed_ticks as f32 / CONTINUITY_SAMPLE_HZ;
        let unturned_hip =
            if let (Some(mut acquisition), Some(parent_affine), Some(hip_in_pelvis)) = (
                self.pelvis_acquisition,
                self.pelvis_parent_affine,
                self.hip_in_pelvis,
            ) {
                acquisition.progress = (acquisition.progress
                    + seconds / acquisition.duration_seconds.max(f32::EPSILON))
                .min(1.0);
                (parent_affine * guard_pelvis_transform_sample(acquisition).compute_affine())
                    .transform_point3(hip_in_pelvis)
            } else {
                self.fixed_hip
            };
        let signature = self.trajectory_signature;
        let response = tactical_movement_acceleration_hz_for_guard(WeaponGuardState::Raised);
        let response_weight = 1.0 - (-response * seconds).exp();
        let controller_delta = signature.command_velocity * seconds
            + (signature.world_velocity - signature.command_velocity)
                * (response_weight / response);
        let predicted_body_rotation = guard_predicted_body_rotation(signature, seconds);
        let body_delta = predicted_body_rotation * signature.body_rotation.inverse();
        let mut scalar = self.scalar_start;
        let mut recovery = self.scalar_recovery;
        for _ in 0..elapsed_ticks {
            scalar = advance_guard_pelvis_scalar_semantic_ticks(
                scalar,
                &mut recovery,
                self.scalar_desired,
                1.0 / CONTINUITY_SAMPLE_HZ,
            );
        }
        Some(
            self.rig_origin
                + body_delta * (unturned_hip - self.rig_origin)
                + controller_delta
                + Vec3::Y * scalar.position,
        )
    }

    fn permits(self, point: Vec3, semantic_tick: u64, hard: bool) -> bool {
        self.sample(semantic_tick).is_some_and(|hip| {
            point.distance(hip)
                <= if hard {
                    self.hard_reach
                } else {
                    self.warning_reach
                } + 0.0001
        })
    }
}

fn guard_exact_proof_matches_live(
    proof: GuardHipPathProof,
    current_signature: Option<GuardPelvisTrajectorySignature>,
    current_sequence: u32,
    current_swing_left: bool,
    semantic_tick: u64,
    live_hip: Option<Vec3>,
    support_owner_valid: bool,
) -> bool {
    let elapsed = semantic_tick.saturating_sub(proof.start_tick) as f32 / CONTINUITY_SAMPLE_HZ;
    let proof_tick = semantic_tick.min(proof.contact_tick);
    let current_signature = current_signature.map(|mut current| {
        // Grounded feet advance from physical contact. Replicated cadence is a
        // phase/synchronization hint and may lead or trail the presented
        // landing by a sparse packet. It must not invalidate an otherwise
        // unchanged controller/body trajectory.
        current.sequence = proof.trajectory_signature.sequence;
        current
    });
    support_owner_valid
        && proof.sequence == current_sequence
        && proof.swing_left == current_swing_left
        && current_signature.is_some_and(|current| {
            !guard_controller_command_changed(proof.trajectory_signature, current, elapsed)
        })
        && proof.sample(proof_tick).is_some()
        && live_hip.is_some_and(Vec3::is_finite)
}

fn exact_guard_sample_is_live_reachable(
    proof: GuardHipPathProof,
    sample: Vec3,
    semantic_tick: u64,
    live_hip: Option<Vec3>,
    hard: bool,
) -> bool {
    let reach = if hard {
        proof.hard_reach
    } else {
        proof.warning_reach
    };
    proof.permits(sample, semantic_tick.min(proof.contact_tick), hard)
        && live_hip.is_some_and(|hip| sample.distance(hip) <= reach + 0.0001)
}

fn contact_driven_guard_owner_is_live(
    proof: GuardHipPathProof,
    current_sequence: u32,
    current_swing_left: bool,
    live_hip: Option<Vec3>,
    support_owner_valid: bool,
) -> bool {
    // The predicted trajectory proves admission. Once a physical step owns
    // the foot, harmless sparse-presentation differences must not destroy it;
    // the actual hip and warning envelope gate every sample below. Only the
    // local contact identity and its planted support remain owner invariants.
    support_owner_valid
        && proof.sequence == current_sequence
        && proof.swing_left == current_swing_left
        && live_hip.is_some_and(Vec3::is_finite)
}

fn contact_driven_guard_sample_is_live_reachable(
    proof: GuardHipPathProof,
    sample: Vec3,
    live_hip: Option<Vec3>,
    recovery_to_contact: bool,
) -> bool {
    live_hip.is_some_and(|hip| {
        sample.distance(hip)
            <= if recovery_to_contact {
                proof.hard_reach
            } else {
                proof.warning_reach
            } + 0.0001
    })
}

fn normalize_deferred_guard_signature(
    proof: GuardHipPathProof,
    pending_cadence_edge: Option<(bool, u32)>,
    mut signature: GuardPelvisTrajectorySignature,
) -> GuardPelvisTrajectorySignature {
    // An intentionally extended contact retains the old foot epoch while the
    // authoritative next cadence identity waits in `pending_cadence_edge`.
    // Normalize only that exact deferred edge for immutable trajectory
    // comparison; all other early/late sequence changes remain visible.
    let expected_next_sequence = signature.sequence == proof.sequence.wrapping_add(1);
    let expected_pending_side =
        pending_cadence_edge.is_some_and(|(pending_left, pending_sequence)| {
            pending_sequence == signature.sequence
                && (pending_left != proof.swing_left
                    || (proof.accepts_preemptive_cadence_confirmation
                        && pending_left == proof.swing_left))
        });
    if expected_next_sequence && expected_pending_side {
        signature.sequence = proof.sequence;
    }
    signature
}

fn replan_invalid_exact_guard_segment(
    previous: PlannedGuardFootSegment,
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    semantic_tick: u64,
    fresh_proof: Option<GuardHipPathProof>,
) -> GuardFootEndpointPlan {
    let GuardSegmentReachProof::Exact(previous_proof) = previous.reach else {
        return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake {
            presented,
        });
    };
    let Some(remaining_ticks) = previous_proof
        .contact_tick
        .checked_sub(semantic_tick)
        .and_then(|ticks| u32::try_from(ticks).ok())
        .and_then(SegmentTickSpan::new)
    else {
        return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake {
            presented,
        });
    };
    let Some(mut proof) = fresh_proof else {
        return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake {
            presented,
        });
    };
    proof.start_tick = semantic_tick;
    proof.contact_tick = previous_proof.contact_tick;

    if previous.end.is_contact() {
        let endpoint = previous.end.position();
        plan_guard_c2_contact_segment(
            presented,
            presented_velocity,
            presented_acceleration,
            remaining_ticks,
            Some(proof),
            |_| Some(endpoint),
        )
    } else {
        plan_exact_guard_release(
            presented,
            presented_velocity,
            presented_acceleration,
            remaining_ticks,
            proof,
        )
    }
}

#[cfg(test)]
fn stationary_exact_guard_proof(
    hip: Vec3,
    warning_reach: f32,
    hard_reach: f32,
    ticks: u32,
) -> GuardHipPathProof {
    GuardHipPathProof {
        start_tick: 0,
        contact_tick: u64::from(ticks),
        sequence: 1,
        swing_left: true,
        accepts_preemptive_cadence_confirmation: false,
        trajectory_signature: GuardPelvisTrajectorySignature {
            body_rotation: Quat::IDENTITY,
            body_target_rotation: Quat::IDENTITY,
            ..default()
        },
        rig_origin: Vec3::ZERO,
        pelvis_parent_affine: None,
        hip_in_pelvis: None,
        fixed_hip: hip,
        pelvis_acquisition: None,
        scalar_start: PelvisFollowerState::default(),
        scalar_recovery: None,
        scalar_desired: 0.0,
        warning_reach,
        hard_reach,
    }
}

#[derive(Debug, Clone, Copy)]
enum GuardSegmentReachProof {
    Exact(GuardHipPathProof),
    Retained(PredictedHipTrajectory),
}

#[derive(Debug, Clone, Copy)]
struct PlannedGuardFootSegment {
    motion: C2FootSegment,
    reach: GuardSegmentReachProof,
    recovery_to_contact: bool,
}

impl PlannedGuardFootSegment {
    fn with_owner_epoch(mut self, owner_epoch: u64) -> Self {
        self.motion = self.motion.with_owner_epoch(owner_epoch);
        if let GuardSegmentReachProof::Exact(mut proof) = self.reach {
            let ticks = u64::from(self.motion.timing.total_ticks.get());
            proof.start_tick = owner_epoch;
            proof.contact_tick = owner_epoch.saturating_add(ticks);
            self.reach = GuardSegmentReachProof::Exact(proof);
        }
        self
    }
}

impl std::ops::Deref for PlannedGuardFootSegment {
    type Target = C2FootSegment;

    fn deref(&self) -> &Self::Target {
        &self.motion
    }
}

impl std::ops::DerefMut for PlannedGuardFootSegment {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.motion
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct C2FootSegment {
    start: Vec3,
    start_velocity: Vec3,
    start_acceleration: Vec3,
    end: FootSegmentEndpoint,
    timing: SegmentTickSpan,
    owner_epoch: u64,
}

impl C2FootSegment {
    fn with_owner_epoch(mut self, owner_epoch: u64) -> Self {
        self.owner_epoch = owner_epoch;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SegmentTickSpan {
    elapsed_ticks: u32,
    total_ticks: NonZeroU32,
}

impl SegmentTickSpan {
    fn new(total_ticks: u32) -> Option<Self> {
        NonZeroU32::new(total_ticks).map(|total_ticks| Self {
            elapsed_ticks: 0,
            total_ticks,
        })
    }

    fn progress(self) -> f32 {
        self.elapsed_ticks.min(self.total_ticks.get()) as f32 / self.total_ticks.get() as f32
    }

    fn duration_seconds(self) -> f32 {
        self.total_ticks.get() as f32 / CONTINUITY_SAMPLE_HZ
    }

    fn is_complete(self) -> bool {
        self.elapsed_ticks >= self.total_ticks.get()
    }

    fn advance(&mut self) {
        self.elapsed_ticks = self
            .elapsed_ticks
            .saturating_add(1)
            .min(self.total_ticks.get());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FootSegmentEndpoint {
    Contact(FeasibleFootEndpoint),
    Release(FeasibleReleaseEndpoint),
}

impl FootSegmentEndpoint {
    const fn position(self) -> Vec3 {
        match self {
            Self::Contact(endpoint) => endpoint.position(),
            Self::Release(endpoint) => endpoint.position(),
        }
    }

    const fn is_contact(self) -> bool {
        matches!(self, Self::Contact(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FootEndpointPlan {
    Segment(C2FootSegment),
    MustReleaseOrReplan(FootReleasePlan),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FootReleasePlan {
    Segment(C2FootSegment),
    /// No complete reach-feasible path exists under the retained hip tube.
    /// Hold the semantic ideal at the presented point while the stateful
    /// follower brakes its retained velocity and acceleration within bounds.
    EmergencyBrake {
        presented: Vec3,
    },
}

#[derive(Debug, Clone, Copy)]
enum GuardFootEndpointPlan {
    Segment(PlannedGuardFootSegment),
    MustReleaseOrReplan(GuardFootReleasePlan),
}

#[derive(Debug, Clone, Copy)]
enum GuardFootReleasePlan {
    Segment(PlannedGuardFootSegment),
    EmergencyBrake { presented: Vec3 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EmergencyFootBrake {
    stationary_ideal: Vec3,
    /// A non-contact recovery acquired while the body is moving follows a
    /// reachable owner-local stance instead of preserving an obsolete world
    /// plant. Stationary pivots and support releases leave this unset.
    owner_local_ideal: Option<Vec3>,
}

impl EmergencyFootBrake {
    fn target(self, rig_origin: Vec3, rig_rotation: Quat) -> Vec3 {
        self.owner_local_ideal
            .map_or(self.stationary_ideal, |local| {
                rig_origin + rig_rotation * local
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SupportReleaseOwner {
    Segment(C2FootSegment),
    EmergencyBrake(EmergencyFootBrake),
    TerminalHold {
        endpoint: Vec3,
    },
    /// Fail-soft ownership for the last grounded support. The target follows a
    /// morphology-derived body-local workspace coordinate but is always
    /// conformed back to terrain, so an invalidated plan may skate briefly but
    /// can never lift both feet or leave a world-space leg behind the body.
    GroundSafetySlide {
        owner_local: Vec3,
    },
}

fn support_release_owner_name(owner: Option<SupportReleaseOwner>) -> Option<&'static str> {
    owner.map(|owner| match owner {
        SupportReleaseOwner::Segment(_) => "segment",
        SupportReleaseOwner::EmergencyBrake(_) => "emergency_brake",
        SupportReleaseOwner::TerminalHold { .. } => "terminal_hold",
        SupportReleaseOwner::GroundSafetySlide { .. } => "ground_safety_slide",
    })
}

fn support_owner_preserves_contact(owner: Option<SupportReleaseOwner>) -> bool {
    owner.is_none_or(|owner| matches!(owner, SupportReleaseOwner::GroundSafetySlide { .. }))
}

fn stationary_guard_pivot_side(
    left_error: f32,
    right_error: f32,
    left_has_contact: bool,
    right_has_contact: bool,
    left_separation: f32,
    right_separation: f32,
) -> bool {
    if !left_has_contact && right_has_contact {
        true
    } else if !right_has_contact && left_has_contact {
        false
    } else if left_error <= GUARD_PIVOT_TRIGGER_METRES {
        false
    } else if right_error <= GUARD_PIVOT_TRIGGER_METRES {
        true
    } else {
        left_separation >= right_separation
    }
}

fn support_owner_blocks_cadence(owner: Option<SupportReleaseOwner>) -> bool {
    owner.is_some_and(|owner| !matches!(owner, SupportReleaseOwner::GroundSafetySlide { .. }))
}

fn ground_safety_slide_target(
    owner_local: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    terrain: Option<&SceneTerrain>,
) -> Vec3 {
    let mut target = rig_origin + rig_rotation * owner_local;
    if let Some(height) = terrain.and_then(|terrain| terrain.height_at(target.xz())) {
        target.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    }
    target
}

fn ground_safety_slide_endpoint(
    presented: Vec3,
    hip: Vec3,
    warning_reach: f32,
    terrain: Option<&SceneTerrain>,
) -> Vec3 {
    let Some(terrain) = terrain else {
        return constrain_target_to_reach(presented, hip, warning_reach);
    };
    let terrain_point = |xz: Vec2| {
        terrain
            .height_at(xz)
            .map(|height| Vec3::new(xz.x, height + MEASURED_ANKLE_SOLE_OFFSET_METRES, xz.y))
    };
    let Some(center) = terrain_point(hip.xz()) else {
        return constrain_target_to_reach(presented, hip, warning_reach);
    };
    let Some(desired) = terrain_point(presented.xz()) else {
        return center;
    };
    if desired.distance(hip) <= warning_reach {
        return desired;
    }
    if center.distance(hip) > warning_reach {
        return center;
    }
    let mut low = 0.0;
    let mut high = 1.0;
    let mut endpoint = center;
    for _ in 0..16 {
        let progress = (low + high) * 0.5;
        let Some(candidate) = terrain_point(hip.xz().lerp(presented.xz(), progress)) else {
            high = progress;
            continue;
        };
        if candidate.distance(hip) <= warning_reach {
            low = progress;
            endpoint = candidate;
        } else {
            high = progress;
        }
    }
    endpoint
}

fn stationary_guard_comfort_endpoint(
    authored: Vec3,
    reach: Option<FootReachEnvelope>,
    terrain: Option<&SceneTerrain>,
) -> Vec3 {
    let Some(reach) = reach else {
        return authored;
    };
    // Preserve several semantic samples of flexion reserve at rest. This is
    // proportional to the actual two-bone chain rather than a character-scale
    // constant, so different limb lengths inherit the same usable workspace.
    ground_safety_slide_endpoint(
        authored,
        reach.current_root(),
        reach.warning_reach() * 0.94,
        terrain,
    )
}

#[allow(clippy::too_many_arguments)]
fn install_grounded_guard_fail_safe(
    footwork: &mut RaisedFootworkState,
    semantic_tick: u64,
    rig_origin: Vec3,
    rig_rotation: Quat,
    left_presented: Vec3,
    right_presented: Vec3,
    left_proof: Option<GuardHipPathProof>,
    right_proof: Option<GuardHipPathProof>,
    terrain: Option<&SceneTerrain>,
) {
    let slide = |presented: Vec3, proof: Option<GuardHipPathProof>| {
        let endpoint = proof
            .and_then(|proof| {
                proof.sample(semantic_tick).map(|hip| {
                    ground_safety_slide_endpoint(presented, hip, proof.warning_reach, terrain)
                })
            })
            .unwrap_or_else(|| {
                let mut endpoint = presented;
                if let Some(height) = terrain.and_then(|terrain| terrain.height_at(endpoint.xz())) {
                    endpoint.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
                }
                endpoint
            });
        SupportReleaseOwner::GroundSafetySlide {
            owner_local: rig_rotation.inverse() * (endpoint - rig_origin),
        }
    };

    footwork.swing_replan_segment = None;
    footwork.swing_emergency_brake = None;
    footwork.swing_release_owner_active = false;
    footwork.awaiting_step_sequence = false;
    footwork.pending_cadence_edge = None;
    footwork.left_support_release_owner = Some(slide(left_presented, left_proof));
    footwork.right_support_release_owner = Some(slide(right_presented, right_proof));
    footwork.left_support_weight = 1.0;
    footwork.right_support_weight = 1.0;
    footwork.left_motion_owner_epoch = semantic_tick;
    footwork.right_motion_owner_epoch = semantic_tick;
}

fn install_guard_swing_fallback(
    footwork: &mut RaisedFootworkState,
    release: GuardFootReleasePlan,
    semantic_tick: u64,
    rig_origin: Vec3,
    rig_rotation: Quat,
    left_authored: Vec3,
    right_authored: Vec3,
) {
    // A failed swing plan must not replace both planted contacts with a
    // second, implicit gait. Keep the opposite foot planted and give only the
    // selected swing foot a bounded owner. This is the same planted/swing
    // lifecycle used by ordinary cadence: one foot moves, one foot supports.
    if footwork.swing_left {
        footwork.left_support_release_owner = None;
        footwork.left_support_weight = 0.0;
        footwork.right_support_weight = 1.0;
        footwork.left_motion_owner_epoch = semantic_tick;
    } else {
        footwork.right_support_release_owner = None;
        footwork.right_support_weight = 0.0;
        footwork.left_support_weight = 1.0;
        footwork.right_motion_owner_epoch = semantic_tick;
    }
    footwork.pending_cadence_edge = None;
    footwork.swing_release_owner_active = true;
    footwork.awaiting_step_sequence = true;
    match release {
        GuardFootReleasePlan::Segment(segment) => {
            let segment = segment.with_owner_epoch(semantic_tick);
            footwork.swing_replan_segment = Some(segment);
            footwork.swing_emergency_brake = None;
            if footwork.swing_left {
                footwork.left_desired_target = Some(segment.start);
                footwork.left_ideal_velocity = segment.start_velocity;
                footwork.left_ideal_acceleration = segment.start_acceleration;
                footwork.left_ideal_history_valid = true;
            } else {
                footwork.right_desired_target = Some(segment.start);
                footwork.right_ideal_velocity = segment.start_velocity;
                footwork.right_ideal_acceleration = segment.start_acceleration;
                footwork.right_ideal_history_valid = true;
            }
        }
        GuardFootReleasePlan::EmergencyBrake { presented } => {
            footwork.swing_replan_segment = None;
            let authored = if footwork.swing_left {
                left_authored
            } else {
                right_authored
            };
            footwork.swing_emergency_brake = Some(EmergencyFootBrake {
                stationary_ideal: presented,
                owner_local_ideal: Some(rig_rotation.inverse() * (authored - rig_origin)),
            });
        }
    }
}

fn reacquire_grounded_guard_support(
    footwork: &mut RaisedFootworkState,
    semantic_swing_left: bool,
    visible_left: Vec3,
    visible_right: Vec3,
    terrain: Option<&SceneTerrain>,
) -> bool {
    if terrain_leg_has_support(footwork.left_support_weight)
        || terrain_leg_has_support(footwork.right_support_weight)
        || footwork.left_support_release_owner.is_some()
        || footwork.right_support_release_owner.is_some()
    {
        return false;
    }
    let support_left = !semantic_swing_left;
    let visible = if support_left {
        visible_left
    } else {
        visible_right
    };
    let Some(height) = terrain.and_then(|terrain| terrain.height_at(visible.xz())) else {
        return false;
    };
    let contact = terrain_conformed_guard_target(visible, Some(height));
    if support_left {
        footwork.left_plant = contact;
        footwork.left_support_weight = 1.0;
        footwork.left_desired_target = Some(contact);
    } else {
        footwork.right_plant = contact;
        footwork.right_support_weight = 1.0;
        footwork.right_desired_target = Some(contact);
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GuardSwingSample {
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ContactErrorEnvelope {
    initial_lag: f32,
    duration_seconds: f32,
}

impl ContactErrorEnvelope {
    fn permitted_lag(self, progress: f32) -> f32 {
        self.initial_lag * (1.0 - quintic_progress(progress.clamp(0.0, 1.0)))
    }
}

impl RaisedFootworkState {
    pub(crate) fn diagnostic_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "initialized": self.initialized,
            "was_moving": self.was_moving,
            "awaiting_step_sequence": self.awaiting_step_sequence,
            "half_step": self.half_step,
            "swing_left": self.swing_left,
            "step_sequence": self.step_sequence,
            "evaluation_tick": self.evaluation_tick,
            "pending_cadence_edge": self.pending_cadence_edge,
            "pivot_active": self.pivot_active,
            "release_handoff_active": self.release_handoff_active,
            "release_handoff_progress": self.release_handoff_progress,
            "visible_pelvis_owner_local": self.visible_pelvis_owner_local,
            "visible_pelvis_local_transform": self.visible_pelvis_local_transform.map(|transform| serde_json::json!({
                "translation": transform.translation,
                "rotation_xyzw": transform.rotation.to_array(),
                "scale": transform.scale,
            })),
            "release_pelvis_offset_owner": self.release_pelvis_offset_owner,
            "swing_segment_active": self.swing_replan_segment.is_some(),
            "swing_segment_progress": self.swing_replan_segment.map(|segment| segment.timing.progress()),
            "swing_release_owner_active": self.swing_release_owner_active,
            "swing_emergency_brake_active": self.swing_emergency_brake.is_some(),
            "left_support_release_owner": support_release_owner_name(self.left_support_release_owner),
            "right_support_release_owner": support_release_owner_name(self.right_support_release_owner),
            "left_plant": self.left_plant,
            "right_plant": self.right_plant,
            "left_command_target": self.left_command_target,
            "right_command_target": self.right_command_target,
            "left_solve_target": self.left_solve_target,
            "right_solve_target": self.right_solve_target,
            "left_target_velocity": self.left_target_velocity,
            "right_target_velocity": self.right_target_velocity,
            "left_target_acceleration": self.left_target_acceleration,
            "right_target_acceleration": self.right_target_acceleration,
            "left_support_weight": self.left_support_weight,
            "right_support_weight": self.right_support_weight,
            "left_owner_kind": self.left_motion_owner_kind,
            "right_owner_kind": self.right_motion_owner_kind,
            "left_owner_epoch": self.left_motion_owner_epoch,
            "right_owner_epoch": self.right_motion_owner_epoch,
            "raised_motion_owned_this_tick": self.raised_motion_owned_this_tick,
        })
    }

    pub(crate) fn diagnostic_is_motion_owner(&self, semantic_tick: u64) -> bool {
        self.initialized
            && self.evaluation_tick == Some(semantic_tick)
            && self.raised_motion_owned_this_tick
    }

    #[cfg(test)]
    pub(crate) fn force_stale_stationary_owner_for_test(&mut self, stale_left: bool) {
        self.pivot_active = false;
        self.pivot_left = stale_left;
        self.swing_left = stale_left;
        self.was_moving = false;
    }

    #[cfg(test)]
    pub(crate) fn swing_left_for_test(&self) -> bool {
        self.swing_left
    }

    #[cfg(test)]
    pub(crate) fn step_sequence_for_test(&self) -> u32 {
        self.step_sequence
    }

    #[cfg(test)]
    pub(crate) fn owner_summary_for_test(&self) -> (bool, bool, bool, bool, bool) {
        (
            self.swing_replan_segment.is_some(),
            self.swing_emergency_brake.is_some(),
            self.swing_release_owner_active,
            self.left_support_release_owner.is_some(),
            self.right_support_release_owner.is_some(),
        )
    }

    pub(crate) fn foot_motion_diagnostic(&self, left: bool) -> Option<FootMotionDiagnostic> {
        if !self.initialized {
            return None;
        }
        let support_owner = if left {
            self.left_support_release_owner
        } else {
            self.right_support_release_owner
        };
        let is_swing = self.swing_left == left;
        let owner = match support_owner {
            Some(SupportReleaseOwner::Segment(_)) => FootMotionOwnerKind::AdmittedC2,
            Some(SupportReleaseOwner::EmergencyBrake(_)) => FootMotionOwnerKind::EmergencyRecovery,
            Some(SupportReleaseOwner::GroundSafetySlide { .. }) => {
                FootMotionOwnerKind::GroundSafetySlide
            }
            Some(SupportReleaseOwner::TerminalHold { .. }) => FootMotionOwnerKind::TerminalHold,
            None if is_swing && self.swing_replan_segment.is_some() => {
                FootMotionOwnerKind::AdmittedC2
            }
            None if is_swing && self.swing_emergency_brake.is_some() => {
                FootMotionOwnerKind::EmergencyRecovery
            }
            None if self.release_handoff_active => FootMotionOwnerKind::ReleaseHandoff,
            None => FootMotionOwnerKind::GuardCadence,
        };
        let (presented, velocity, acceleration, commanded, pole) = if left {
            (
                self.left_solve_target?,
                self.left_target_velocity,
                self.left_target_acceleration,
                self.left_command_target,
                self.left_knee_bend_world,
            )
        } else {
            (
                self.right_solve_target?,
                self.right_target_velocity,
                self.right_target_acceleration,
                self.right_command_target,
                self.right_knee_bend_world,
            )
        };
        let (maximum_acceleration, maximum_jerk) = if owner == FootMotionOwnerKind::AdmittedC2 {
            (
                GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
                GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
            )
        } else {
            (
                FOOT_FOLLOWER_MAXIMUM_ACCELERATION,
                FOOT_FOLLOWER_MAXIMUM_JERK,
            )
        };
        let segment = match support_owner {
            Some(SupportReleaseOwner::Segment(segment)) => Some(segment),
            _ if is_swing => self.swing_replan_segment.map(|planned| planned.motion),
            _ => None,
        };
        let envelope = segment.and_then(contact_error_envelope);
        Some(FootMotionDiagnostic {
            owner,
            presented,
            velocity,
            acceleration,
            commanded,
            pole,
            solve_hip: if left {
                self.left_solve_hip
            } else {
                self.right_solve_hip
            },
            upper_length: if left {
                self.left_solve_upper_length
            } else {
                self.right_solve_upper_length
            },
            lower_length: if left {
                self.left_solve_lower_length
            } else {
                self.right_solve_lower_length
            },
            commanded_pole: if left {
                self.left_commanded_pole
            } else {
                self.right_commanded_pole
            },
            pole_tracking_active: false,
            pole_angular_velocity: 0.0,
            pole_angular_acceleration: 0.0,
            maximum_acceleration,
            maximum_jerk,
            maximum_pole_angular_acceleration: ANATOMICAL_POLE_MAXIMUM_ACCELERATION,
            maximum_pole_angular_jerk: ANATOMICAL_POLE_MAXIMUM_JERK,
            contact_endpoint: envelope.map(|_| segment.unwrap().end.position()),
            contact_progress: envelope.map(|_| segment.unwrap().timing.progress()),
            contact_tick: envelope.map(|_| {
                segment
                    .unwrap()
                    .owner_epoch
                    .saturating_add(segment.unwrap().timing.total_ticks.get() as u64)
            }),
            permitted_contact_lag: envelope
                .map(|envelope| envelope.permitted_lag(segment.unwrap().timing.progress())),
            owner_epoch: segment
                .map(|segment| segment.owner_epoch)
                .unwrap_or(if left {
                    self.left_motion_owner_epoch
                } else {
                    self.right_motion_owner_epoch
                }),
            contact_event: if let Some(aborted_owner_epoch) = if left {
                self.left_contact_abort_event
            } else {
                self.right_contact_abort_event
            } {
                Some(ContactMotionEvent::AbortedLiveReach {
                    aborted_owner_epoch,
                })
            } else {
                segment.and_then(|segment| {
                    segment.end.is_contact().then_some(
                        if segment.timing.is_complete() && self.awaiting_step_sequence {
                            ContactMotionEvent::Completed
                        } else {
                            ContactMotionEvent::Promised
                        },
                    )
                })
            },
        })
    }
}

impl Default for RaisedFootworkState {
    fn default() -> Self {
        Self {
            initialized: false,
            was_moving: false,
            awaiting_step_sequence: false,
            half_step: 0,
            lead: LeadFoot::Left,
            swing_left: false,
            step_origin: Vec3::ZERO,
            step_rotation: Quat::IDENTITY,
            swing_stance_local: Vec3::ZERO,
            swing_start: Vec3::ZERO,
            swing_end: Vec3::ZERO,
            swing_replan_segment: None,
            swing_release_owner_active: false,
            swing_emergency_brake: None,
            pending_cadence_edge: None,
            left_plant: Vec3::ZERO,
            right_plant: Vec3::ZERO,
            evaluation_tick: None,
            step_sequence: 0,
            left_support_weight: 0.0,
            right_support_weight: 0.0,
            left_solve_target: None,
            right_solve_target: None,
            left_command_target: None,
            right_command_target: None,
            left_target_velocity: Vec3::ZERO,
            right_target_velocity: Vec3::ZERO,
            left_target_acceleration: Vec3::ZERO,
            right_target_acceleration: Vec3::ZERO,
            left_desired_target: None,
            right_desired_target: None,
            left_ideal_velocity: Vec3::ZERO,
            right_ideal_velocity: Vec3::ZERO,
            left_ideal_acceleration: Vec3::ZERO,
            right_ideal_acceleration: Vec3::ZERO,
            left_ideal_history_valid: false,
            right_ideal_history_valid: false,
            left_support_release_owner: None,
            right_support_release_owner: None,
            release_handoff_active: false,
            release_handoff_progress: 0.0,
            release_left_start: Vec3::ZERO,
            release_right_start: Vec3::ZERO,
            visible_pelvis_owner_local: None,
            visible_pelvis_local_transform: None,
            pelvis_acquisition: None,
            release_pelvis_offset_owner: Vec3::ZERO,
            pivot_active: false,
            pivot_left: false,
            pivot_progress: 0.0,
            pivot_origin: Vec3::ZERO,
            pivot_start: Vec3::ZERO,
            pivot_end: Vec3::ZERO,
            left_knee_bend_world: None,
            right_knee_bend_world: None,
            left_end_direction: None,
            right_end_direction: None,
            left_hip_world: None,
            right_hip_world: None,
            left_hip_velocity: Vec3::ZERO,
            right_hip_velocity: Vec3::ZERO,
            left_solve_hip: None,
            right_solve_hip: None,
            left_solve_upper_length: None,
            right_solve_upper_length: None,
            left_solve_lower_length: None,
            right_solve_lower_length: None,
            left_commanded_pole: None,
            right_commanded_pole: None,
            left_contact_abort_event: None,
            right_contact_abort_event: None,
            left_motion_owner_epoch: 0,
            right_motion_owner_epoch: 0,
            left_motion_owner_kind: FootMotionOwnerKind::None,
            right_motion_owner_kind: FootMotionOwnerKind::None,
            raised_motion_owned_this_tick: false,
            contact_gait: None,
        }
    }
}

fn reseed_raised_from_quickstep_handoff(
    footwork: &mut RaisedFootworkState,
    memory: &LegIkMemory,
    visible_left: Vec3,
    visible_right: Vec3,
) {
    footwork.left_plant = visible_left;
    footwork.right_plant = visible_right;
    footwork.left_solve_target = Some(visible_left);
    footwork.right_solve_target = Some(visible_right);
    let mut seed = |left: bool, follower: Option<FootFollowerState>| {
        let (velocity, acceleration, desired, ideal_velocity, ideal_acceleration, valid) = follower
            .map(|follower| {
                (
                    follower.velocity,
                    follower.acceleration,
                    Some(follower.previous_ideal),
                    follower.previous_ideal_velocity,
                    follower.previous_ideal_acceleration,
                    true,
                )
            })
            .unwrap_or((Vec3::ZERO, Vec3::ZERO, None, Vec3::ZERO, Vec3::ZERO, false));
        if left {
            footwork.left_target_velocity = velocity;
            footwork.left_target_acceleration = acceleration;
            footwork.left_desired_target = desired;
            footwork.left_ideal_velocity = ideal_velocity;
            footwork.left_ideal_acceleration = ideal_acceleration;
            footwork.left_ideal_history_valid = valid;
        } else {
            footwork.right_target_velocity = velocity;
            footwork.right_target_acceleration = acceleration;
            footwork.right_desired_target = desired;
            footwork.right_ideal_velocity = ideal_velocity;
            footwork.right_ideal_acceleration = ideal_acceleration;
            footwork.right_ideal_history_valid = valid;
        }
    };
    seed(true, memory.left_foot_follower);
    seed(false, memory.right_foot_follower);
    footwork.left_knee_bend_world = memory.left_terrain_pole_world;
    footwork.right_knee_bend_world = memory.right_terrain_pole_world;
    footwork.left_end_direction = memory.left_terrain_end_direction;
    footwork.right_end_direction = memory.right_terrain_end_direction;
}

fn reseed_guard_cadence_ideal_history(
    footwork: &mut RaisedFootworkState,
    visible_left: Vec3,
    visible_right: Vec3,
) {
    // A replicated cadence edge latches a new C2 swing path. Preserve the
    // rendered follower derivatives, but seed the semantic path itself from
    // the visible endpoints with zero endpoint derivatives. Finite-
    // differencing across two different cadence paths creates the old
    // hold/resume staircase and repeated discontinuity outcomes.
    footwork.left_solve_target = Some(visible_left);
    footwork.right_solve_target = Some(visible_right);
    footwork.left_desired_target = Some(visible_left);
    footwork.right_desired_target = Some(visible_right);
    footwork.left_ideal_velocity = Vec3::ZERO;
    footwork.right_ideal_velocity = Vec3::ZERO;
    footwork.left_ideal_acceleration = Vec3::ZERO;
    footwork.right_ideal_acceleration = Vec3::ZERO;
    footwork.left_ideal_history_valid = true;
    footwork.right_ideal_history_valid = true;
}

fn raised_release_owns_ik(skeleton: &SkeletonState, state: Option<&RaisedFootworkState>) -> bool {
    if !state.is_some_and(|state| state.release_handoff_active)
        || skeleton.action_kind() != SkeletonAction::None
        || skeleton.body().is_downed()
    {
        return false;
    }
    match skeleton
        .posture_transition()
        .map(|transition| transition.kind())
    {
        None => matches!(skeleton.posture(), Posture::Upright | Posture::Crouched),
        // The ordinary handoff was seeded by the final upright release
        // sample. Once the authored down transition begins, ordinary owns its
        // quintic full-pose release and legacy raised IK must yield.
        Some(PostureTransitionKind::UprightToProne) => false,
        Some(
            PostureTransitionKind::ProneToUpright
            | PostureTransitionKind::ProneToSupine { .. }
            | PostureTransitionKind::SupineToProne { .. }
            | PostureTransitionKind::SupineToUpright
            | PostureTransitionKind::DiveToDowned { .. },
        ) => false,
    }
}

fn raised_release_uses_transition_authored_target(skeleton: &SkeletonState) -> bool {
    skeleton
        .posture_transition()
        .is_some_and(|transition| transition.kind() == PostureTransitionKind::UprightToProne)
}

fn preserve_raised_handoff_targets(
    memory: &mut LegIkMemory,
    raised: RaisedFootworkState,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    let left = raised.left_solve_target.unwrap_or(raised.left_plant);
    let right = raised.right_solve_target.unwrap_or(raised.right_plant);
    let left_support = memory.left_support_weight.unwrap_or(0.0);
    let right_support = memory.right_support_weight.unwrap_or(0.0);
    memory.left_foot_world_target = Some(left);
    memory.right_foot_world_target = Some(right);
    memory.left_foot_target = Some(rig_rotation.inverse() * (left - rig_origin));
    memory.right_foot_target = Some(rig_rotation.inverse() * (right - rig_origin));
    memory.left_foot_plant = Some(left);
    memory.right_foot_plant = Some(right);
    memory.left_foot_plant_acquired = left_support > 0.05;
    memory.right_foot_plant_acquired = right_support > 0.05;
    memory.left_transition_support_weight = Some(left_support);
    memory.right_transition_support_weight = Some(right_support);
    memory.left_release_active = true;
    memory.right_release_active = true;
    memory.left_release_target = None;
    memory.right_release_target = None;
}

fn raised_release_handoff_is_complete(footwork: RaisedFootworkState) -> bool {
    footwork.release_handoff_active && footwork.release_handoff_progress >= 1.0
}

fn guard_release_pelvis_offset(previous_visible: Option<Vec3>, current_authored: Vec3) -> Vec3 {
    previous_visible
        .map(|previous| previous - current_authored)
        .filter(|offset| offset.is_finite())
        .unwrap_or(Vec3::ZERO)
}

fn retained_guard_release_pelvis_offset(offset: Vec3, progress: f32) -> Vec3 {
    offset * (1.0 - quintic_progress(progress))
}

fn raised_refresh_advances(previous_tick: Option<u64>, tick: u64) -> bool {
    previous_tick != Some(tick)
}

fn invalid_terrain_posture_has_downstream_leg_owner(
    action: SkeletonAction,
    posture_transitioning: bool,
) -> bool {
    action == SkeletonAction::Dodge || posture_transitioning
}

fn terrain_ik_is_required(enabled: bool, settle_active: bool, raised_handoff: bool) -> bool {
    enabled || settle_active || raised_handoff
}

fn advance_settle_state(
    mut settle: LocomotionSettleState,
    delta_seconds: f32,
) -> LocomotionSettleState {
    let delta_seconds = delta_seconds.max(0.0);
    settle.elapsed_seconds += delta_seconds;
    settle.progress = (settle.progress + delta_seconds / SETTLE_STEP_SECONDS).min(1.0);
    settle
}

fn settle_target_speed(settle: LocomotionSettleState) -> f32 {
    if settle.raised_handoff {
        RAISED_SETTLE_TARGET_SPEED
    } else {
        AIRBORNE_RELEASE_TARGET_SPEED
    }
}

fn cancel_settle_for_restart(memory: &mut LegIkMemory, planar_velocity: Vec3) {
    // A stop can be cancelled while its selected support is still airborne.
    // In that case the retained terrain plant is only a future landing goal,
    // not support ownership. Resume gait from the last propagated ankle the
    // player actually saw; otherwise the first restart sample can snap from
    // the clearance follower directly to the stale settle contact.
    for left in [true, false] {
        let acquired = if left {
            memory.left_foot_plant_acquired
        } else {
            memory.right_foot_plant_acquired
        };
        if acquired {
            continue;
        }
        let (rendered_world, rendered_owner) = if left {
            (
                memory.left_last_rendered_world,
                memory.left_last_rendered_owner,
            )
        } else {
            (
                memory.right_last_rendered_world,
                memory.right_last_rendered_owner,
            )
        };
        if left {
            memory.left_foot_plant = None;
            memory.left_foot_world_target = rendered_world.or(memory.left_foot_world_target);
            memory.left_foot_target = rendered_owner.or(memory.left_foot_target);
            memory.left_support_weight = Some(0.0);
            memory.left_transition_support_weight = Some(0.0);
            memory.left_release_active = true;
            memory.left_release_target = None;
        } else {
            memory.right_foot_plant = None;
            memory.right_foot_world_target = rendered_world.or(memory.right_foot_world_target);
            memory.right_foot_target = rendered_owner.or(memory.right_foot_target);
            memory.right_support_weight = Some(0.0);
            memory.right_transition_support_weight = Some(0.0);
            memory.right_release_active = true;
            memory.right_release_target = None;
        }
    }
    memory.settle = None;
    reset_terminal_settle_reach(memory);
    memory.recent_movement_velocity = planar_velocity.with_y(0.0);
}

fn reset_terminal_settle_reach(memory: &mut LegIkMemory) {
    memory.terminal_contacts_prepared = false;
    memory.terminal_root_base_translation = None;
    memory.terminal_reach_shift = 0.0;
    memory.terminal_reach_target_shift = None;
}

fn finish_settle_for_idle(memory: &mut LegIkMemory) {
    let terminal_reach_shift = memory.terminal_reach_shift;
    memory.settle = None;
    reset_terminal_settle_reach(memory);
    // The next sparse authored-idle evaluation restores the uncorrected rig
    // root. Transfer the converged terminal reach offset to ordinary retained
    // pelvis ownership so both frozen contacts remain reachable instead of
    // dropping one support and starting another settle loop.
    memory.pelvis_shift = terminal_reach_shift;
    memory.recent_movement_velocity = Vec3::ZERO;
    // Promote both final solve targets to a stable idle stance. Clearing them
    // here made the next authored-idle evaluation pull a wide settled step
    // half a metre under the body in one frame. Movement restart still releases
    // these plants through the ordinary bounded gait handoff.
    memory.left_foot_plant = memory.left_foot_world_target;
    memory.right_foot_plant = memory.right_foot_world_target;
    memory.left_foot_plant_acquired = memory.left_foot_plant.is_some();
    memory.right_foot_plant_acquired = memory.right_foot_plant.is_some();
    memory.left_planned_contact = None;
    memory.right_planned_contact = None;
    memory.left_planned_contact_start = None;
    memory.right_planned_contact_start = None;
    memory.left_planned_contact_phase_start = None;
    memory.right_planned_contact_phase_start = None;
    memory.left_support_weight = Some(1.0);
    memory.right_support_weight = Some(1.0);
    memory.left_transition_support_weight = Some(1.0);
    memory.right_transition_support_weight = Some(1.0);
    memory.left_support_exhausted_until_flight = false;
    memory.right_support_exhausted_until_flight = false;
    memory.left_release_active = false;
    memory.right_release_active = false;
    memory.left_release_target = None;
    memory.right_release_target = None;
}

fn settle_is_terminal(memory: &LegIkMemory) -> bool {
    memory.settle.is_some_and(|settle| settle.progress >= 1.0)
        && !memory.left_release_active
        && !memory.right_release_active
}

fn prepare_terminal_settle_contacts(
    memory: &mut LegIkMemory,
    rig_origin: Vec3,
    rig_rotation: Quat,
    terrain_height_at: impl Fn(Vec2) -> Option<f32>,
) -> bool {
    if !memory.terminal_contacts_prepared {
        // Terminal solve changes from ordinary pelvis ownership to an
        // absolute rig-root correction. Seed it from the correction already
        // visible on the final settle sample; starting from zero restores the
        // authored root for one frame and lifts the whole hierarchy by the
        // accumulated settle drop.
        memory.terminal_reach_shift = memory.pelvis_shift;
        memory.terminal_reach_target_shift = None;
    }
    let left_seed = if memory.terminal_contacts_prepared {
        memory.left_foot_world_target
    } else {
        memory
            .left_last_rendered_world
            .filter(|target| target.is_finite())
            .or(memory.left_foot_world_target)
    };
    let Some(mut left) = left_seed else {
        return false;
    };
    let right_seed = if memory.terminal_contacts_prepared {
        memory.right_foot_world_target
    } else {
        memory
            .right_last_rendered_world
            .filter(|target| target.is_finite())
            .or(memory.right_foot_world_target)
    };
    let Some(mut right) = right_seed else {
        return false;
    };
    let (Some(left_height), Some(right_height)) =
        (terrain_height_at(left.xz()), terrain_height_at(right.xz()))
    else {
        return false;
    };
    left.y = left_height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    right.y = right_height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    memory.left_foot_world_target = Some(left);
    memory.right_foot_world_target = Some(right);
    memory.left_foot_target = Some(rig_rotation.inverse() * (left - rig_origin));
    memory.right_foot_target = Some(rig_rotation.inverse() * (right - rig_origin));
    memory.left_foot_plant = Some(left);
    memory.right_foot_plant = Some(right);
    memory.left_foot_plant_acquired = false;
    memory.right_foot_plant_acquired = false;
    if let Some(settle) = memory.settle.as_mut() {
        settle.landing_target = if settle.support_left { right } else { left };
    }
    memory.terminal_contacts_prepared = true;
    true
}

fn terminal_settle_contacts_are_rendered(
    memory: &LegIkMemory,
    terrain_height_at: impl Fn(Vec2) -> Option<f32>,
) -> bool {
    [
        (
            memory.left_last_rendered_world,
            memory.left_foot_world_target,
            memory.left_last_rendered_toe_world,
        ),
        (
            memory.right_last_rendered_world,
            memory.right_foot_world_target,
            memory.right_last_rendered_toe_world,
        ),
    ]
    .into_iter()
    .all(|(rendered, target, toe)| {
        rendered
            .zip(target)
            .zip(toe)
            .is_some_and(|((rendered, target), toe)| {
                rendered.xz().distance(target.xz()) <= 0.01
                    && terrain_height_at(rendered.xz()).is_some_and(|height| {
                        (rendered.y - height - MEASURED_ANKLE_SOLE_OFFSET_METRES).abs() <= 0.01
                    })
                    && terrain_height_at(toe.xz()).is_some_and(|height| {
                        let clearance = toe.y - height;
                        (-0.01..=0.10).contains(&clearance)
                    })
            })
    })
}

fn required_hip_shift_for_reach(upper: Vec3, target: Vec3, reach: f32) -> f32 {
    let horizontal_distance = (target - upper).xz().length();
    let maximum_vertical = (reach * reach - horizontal_distance * horizontal_distance)
        .max(0.0)
        .sqrt();
    target.y + maximum_vertical - upper.y
}

fn guard_warning_reach(upper_length: f32, lower_length: f32) -> f32 {
    (upper_length * upper_length
        + lower_length * lower_length
        + 2.0 * upper_length * lower_length * 30.0_f32.to_radians().cos())
    .sqrt()
}

fn terminal_contact_solve_ownership(
    terminal_prepared: bool,
    nominal_weight: f32,
    retained_plant: Option<Vec3>,
) -> (f32, Option<Vec3>) {
    if terminal_prepared && retained_plant.is_some() {
        (1.0, retained_plant)
    } else {
        (nominal_weight, retained_plant)
    }
}

fn seed_settle_from_rendered_feet(
    memory: &mut LegIkMemory,
    left: Option<Vec3>,
    right: Option<Vec3>,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    reset_terminal_settle_reach(memory);
    let left = left.filter(|target| target.is_finite());
    let right = right.filter(|target| target.is_finite());
    memory.left_foot_follower = left.and_then(|position| {
        FootFollowerState::from_presented_pose(
            position,
            Vec3::ZERO,
            Vec3::ZERO,
            position,
            Vec3::ZERO,
            Vec3::ZERO,
        )
    });
    memory.right_foot_follower = right.and_then(|position| {
        FootFollowerState::from_presented_pose(
            position,
            Vec3::ZERO,
            Vec3::ZERO,
            position,
            Vec3::ZERO,
            Vec3::ZERO,
        )
    });
    if let Some(left) = left {
        memory.left_foot_world_target = Some(left);
        memory.left_foot_target = Some(rig_rotation.inverse() * (left - rig_origin));
        memory.left_foot_plant = None;
        memory.left_foot_plant_acquired = false;
    }
    if let Some(right) = right {
        memory.right_foot_world_target = Some(right);
        memory.right_foot_target = Some(rig_rotation.inverse() * (right - rig_origin));
        memory.right_foot_plant = None;
        memory.right_foot_plant_acquired = false;
    }
    // Stop capture owns both legs. A gait plan retained from the preceding run
    // is not a valid landing goal for either the stationary support or the
    // balance-recovery swing.
    memory.left_planned_contact = None;
    memory.right_planned_contact = None;
    memory.left_planned_contact_start = None;
    memory.right_planned_contact_start = None;
    memory.left_planned_contact_phase_start = None;
    memory.right_planned_contact_phase_start = None;
}

fn settle_visible_foot(
    last_rendered_world: Option<Vec3>,
    current_authored_world: Option<Vec3>,
) -> Option<Vec3> {
    last_rendered_world
        .filter(|target| target.is_finite())
        .or_else(|| current_authored_world.filter(|target| target.is_finite()))
}

fn retain_settle_support(
    memory: &mut LegIkMemory,
    support_left: bool,
    left: Option<Vec3>,
    right: Option<Vec3>,
    acquired: bool,
) {
    if support_left {
        memory.left_foot_plant = left;
        memory.left_foot_plant_acquired = acquired && left.is_some();
        memory.left_transition_support_weight = Some(1.0);
    } else {
        memory.right_foot_plant = right;
        memory.right_foot_plant_acquired = acquired && right.is_some();
        memory.right_transition_support_weight = Some(1.0);
    }
}

/// Client-only world-space target for a hand. It is presentation data and is
/// deliberately absent from replicated `SkeletonState`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HandIkTarget {
    pub translation: Vec3,
    pub rotation: Option<Quat>,
    pub weight: f32,
}

/// Optional client-only direct hand targets.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct HumanoidIkTargets {
    pub left: Option<HandIkTarget>,
    pub right: Option<HandIkTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Public input for optional held-item constraints.
pub(crate) enum HandSide {
    Left,
    Right,
}

/// Constrains a client-side held item to an authored weapon socket. The
/// optional point is in weapon-local space and becomes an off-hand IK target.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct HeldWeaponConstraint {
    pub owner: Entity,
    pub primary_hand: HandSide,
    pub secondary_grip_local: Option<Vec3>,
}

/// Places the planted foot on the terrain with an analytic two-bone solve,
/// then lowers the hips by the bounded residual. Existing weapon/hand
/// constraints run at the same final-pose seam.
pub(in crate::animation) fn apply_terrain_leg_ik(
    enabled: Res<super::super::TerrainIkEnabled>,
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<&PresentedSkeleton>,
    body_facings: Query<&LocalBodyFacing>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut ik_states: Query<&mut LegIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
    mut ordinary_states: Query<&mut locomotion::OrdinaryLocomotionIkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let terrain = terrain.single().ok();
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        let semantic_tick = clock.semantic_step().0;
        if ik_states
            .get(owner)
            .is_ok_and(|state| leg_ik_was_evaluated_at(state.0, semantic_tick))
        {
            // Authored hierarchy input is restored for every capture view.
            // Re-publish the exact first-view terrain/raised result without
            // advancing cadence, followers, or diagnostics.
            if let Ok(memory) = ik_states.get(owner) {
                let pelvis_transform = raised_states
                    .get(owner)
                    .ok()
                    .and_then(|state| state.visible_pelvis_local_transform);
                locomotion::apply_retained_raised_lower_body(
                    rig,
                    memory.0,
                    pelvis_transform,
                    &mut transforms.p1(),
                );
            }
            continue;
        }
        if skeleton.body().is_downed()
            && let (Ok(state), Ok(memory)) = (raised_states.get(owner), ik_states.get(owner))
            && state.initialized
        {
            let pelvis_transform = state.visible_pelvis_local_transform;
            let local_scalar_shift =
                raised_pelvis_local_scalar_shift(rig, memory.0, &parents, &transforms.p0());
            locomotion::apply_retained_raised_lower_body(
                rig,
                memory.0,
                pelvis_transform,
                &mut transforms.p1(),
            );
            let mut ordinary = ordinary_states
                .get_mut(owner)
                .map(|state| *state)
                .unwrap_or_default();
            locomotion::publish_raised_release_handoff(
                &mut ordinary,
                memory.0,
                pelvis_transform,
                local_scalar_shift,
            );
            if let Ok(mut state) = ordinary_states.get_mut(owner) {
                *state = ordinary;
            } else {
                commands.entity(owner).insert(ordinary);
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = RaisedFootworkState::default();
            }
        }
        let raised_footwork_was_active = raised_states
            .get(owner)
            .is_ok_and(|state| state.initialized);
        let raised_release_active = raised_release_owns_ik(skeleton, raised_states.get(owner).ok());
        let raised_ground_contact_valid =
            raised_guard_ground_contact_is_valid(skeleton, raised_footwork_was_active);
        if let Ok(mut state) = raised_states.get_mut(owner) {
            state.raised_motion_owned_this_tick = false;
        }
        if locomotion::owns(skeleton) && !raised_footwork_was_active {
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = RaisedFootworkState::default();
            }
            continue;
        }
        if !terrain_ik_posture_is_valid(skeleton)
            && !raised_release_active
            && !raised_ground_contact_valid
        {
            // Quickstep owns the downstream leg solve and shared diagnostics.
            // Yield without clearing its first-view result before repeated
            // presentation views reach quickstep's own fixed-tick guard.
            if invalid_terrain_posture_has_downstream_leg_owner(
                skeleton.action_kind(),
                skeleton.is_posture_transitioning(),
            ) {
                continue;
            }
            if let (Ok(state), Ok(memory)) = (raised_states.get(owner), ik_states.get(owner))
                && state.initialized
            {
                let pelvis_transform = state.visible_pelvis_local_transform;
                let local_scalar_shift =
                    raised_pelvis_local_scalar_shift(rig, memory.0, &parents, &transforms.p0());
                locomotion::apply_retained_raised_lower_body(
                    rig,
                    memory.0,
                    pelvis_transform,
                    &mut transforms.p1(),
                );
                let mut ordinary = ordinary_states
                    .get_mut(owner)
                    .map(|state| *state)
                    .unwrap_or_default();
                locomotion::publish_raised_release_handoff(
                    &mut ordinary,
                    memory.0,
                    pelvis_transform,
                    local_scalar_shift,
                );
                if let Ok(mut state) = ordinary_states.get_mut(owner) {
                    *state = ordinary;
                } else {
                    commands.entity(owner).insert(ordinary);
                }
            }
            if let Ok(mut state) = ik_states.get_mut(owner) {
                reset_terrain_ik_preserving_anatomical_evaluation(&mut state.0);
                if let Some((tick, _)) = clock.fixed_step() {
                    state.0.evaluation_tick = Some(tick);
                }
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = RaisedFootworkState::default();
            }
            continue;
        }
        let raised_guard_follower = raised_ground_contact_valid
            && skeleton.weapon_guard() == WeaponGuardState::Raised
            && !skeleton.guarded_sprint_locomotion()
            && matches!(
                skeleton.action_kind(),
                SkeletonAction::None | SkeletonAction::Attack | SkeletonAction::Block
            );
        let raised_footwork_handoff =
            !raised_guard_follower && raised_footwork_was_active && !raised_release_active;
        let raised_solver_follower =
            raised_guard_follower || raised_footwork_handoff || raised_release_active;
        if let Ok(mut state) = raised_states.get_mut(owner) {
            state.raised_motion_owned_this_tick = raised_solver_follower;
        }
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
        // A grounded raised follower consumes a pending quickstep handoff via
        // its retained landing-local targets below. Clearing it here used to
        // drop every solve target on the first post-dodge frame.
        let (semantic_tick, semantic_delta_seconds) = clock.semantic_step();
        let evaluation_advances = memory.evaluation_tick != Some(semantic_tick);
        let state_delta_seconds = if evaluation_advances {
            memory.evaluation_tick = Some(semantic_tick);
            semantic_delta_seconds
        } else {
            0.0
        };
        if repeated_fixed_tick_skips_ik(true, evaluation_advances) {
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
                raised_footwork_handoff || raised_release_active,
            ) {
                1.0
            } else {
                0.0
            };
            memory.terrain_blend += (desired - memory.terrain_blend).clamp(
                -TERRAIN_IK_BLEND_SPEED * state_delta_seconds,
                TERRAIN_IK_BLEND_SPEED * state_delta_seconds,
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
        // Capture truthful ordinary dual support before this evaluation can
        // enter the authored guard-clip owner gap and report floating FK feet.
        if !raised_solver_follower {
            retain_last_dual_support_handoff(&mut memory, rig_origin, rig_rotation);
        }
        let first_stationary_raised_acquisition = raised_solver_follower
            && !skeleton.raised_locomotion().is_moving()
            && !memory.quickstep_handoff_pending
            && raised_states
                .get(owner)
                .map(|state| !state.initialized)
                .unwrap_or(true);
        if first_stationary_raised_acquisition {
            restore_stationary_guard_handoff(&mut memory, rig_origin, rig_rotation);
        }
        if memory.quickstep_handoff_pending || memory.quickstep_guard_stance_held {
            memory.left_foot_world_target = memory
                .quickstep_left_landing_local
                .map(|local| rig_origin + rig_rotation * local)
                .or(memory.left_foot_world_target);
            memory.right_foot_world_target = memory
                .quickstep_right_landing_local
                .map(|local| rig_origin + rig_rotation * local)
                .or(memory.right_foot_world_target);
        }
        if state_delta_seconds > 0.0 {
            let previous_rig_origin = memory.rig_origin;
            let owner_discontinuous = previous_rig_origin.is_some_and(|previous| {
                previous.distance(rig_origin) > MAX_OWNER_TRANSLATION_PER_TICK
            }) || memory.rig_rotation.is_some_and(|previous| {
                previous.angle_between(rig_rotation).to_degrees()
                    > MAX_OWNER_ROTATION_PER_TICK_DEGREES
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
                clear_last_dual_support_handoff(&mut memory);
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
            // The authoritative raised cadence can finish a latched half-step
            // after movement velocity reaches zero. Preserve both last visible
            // targets as the beginning of a bounded balance capture instead of
            // reacquiring authored gait feet at the half-step seam.
            let current_pelvis_owner_local = rig
                .get(&BoneRole::Pelvis)
                .and_then(|pelvis| transforms.p0().compute_global_transform(*pelvis).ok())
                .map(|pelvis| rig_rotation.inverse() * (pelvis.translation() - rig_origin));
            if let Ok(mut raised) = raised_states.get_mut(owner) {
                preserve_raised_handoff_targets(&mut memory, *raised, rig_origin, rig_rotation);
                raised.release_handoff_active = true;
                raised.release_handoff_progress = 0.0;
                raised.release_left_start = raised.left_solve_target.unwrap_or(raised.left_plant);
                raised.release_right_start =
                    raised.right_solve_target.unwrap_or(raised.right_plant);
                let scalar_owner = rig_rotation.inverse() * (Vec3::Y * memory.raised_pelvis_shift);
                let decomposed_visible = raised
                    .visible_pelvis_owner_local
                    .map(|visible| visible - scalar_owner);
                raised.release_pelvis_offset_owner = current_pelvis_owner_local
                    .map_or(Vec3::ZERO, |current| {
                        guard_release_pelvis_offset(decomposed_visible, current)
                    });
                // Release takes the exact visible residual above as its sole
                // pelvis owner. Leaving acquisition alive would apply that
                // residual a second time later in this system.
                raised.pelvis_acquisition = None;
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
                        memory
                            .recent_movement_velocity
                            .clamp_length_max(MAX_SETTLE_CAPTURE_SPEED),
                        ASSUMED_COM_HEIGHT_METRES,
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
                    stateful_follower: uses_run_airborne_motion_budget(
                        locomotion_profile(skeleton).gait,
                        planar_velocity
                            .length()
                            .max(memory.measured_owner_planar_speed),
                    ),
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
        let mut pelvis_acquisition_plan = None;
        let mut pelvis_acquisition_context = None;
        if raised_solver_follower && let Some(&pelvis) = rig.get(&BoneRole::Pelvis) {
            let local_scalar_shift =
                raised_pelvis_local_scalar_shift(rig, memory, &parents, &transforms.p0());
            let authored = transforms.p1().get(pelvis).ok().copied();
            if let (Some(authored), Ok(mut footwork)) = (authored, raised_states.get_mut(owner)) {
                let (body_rotation, body_target_rotation) = body_facings
                    .get(owner)
                    .map(|facing| (facing.rotation, facing.target_rotation))
                    .unwrap_or((rig_rotation, rig_rotation));
                let trajectory_signature = guard_controller_trajectory_signature(
                    skeleton,
                    body_rotation,
                    body_target_rotation,
                    semantic_tick,
                );
                let current_sequence = skeleton.raised_locomotion().step_sequence();
                if defer_guard_cadence_edge_for_active_pelvis(
                    &mut footwork,
                    skeleton.raised_locomotion().swing_foot() == Some(LeadFoot::Left),
                    current_sequence,
                ) {
                    // The pelvis owner was admitted against the currently
                    // visible support identity. A sparse authoritative cadence
                    // edge may arrive before that full-transform path is
                    // terminal. Retain and re-prove the same support epoch;
                    // defer the new swing identity instead of first flipping
                    // support and then releasing the newly selected leg.
                }
                let presented = condition_guard_pelvis_acquisition(
                    &mut footwork,
                    authored,
                    local_scalar_shift,
                    skeleton.raised_locomotion().is_moving(),
                    current_sequence,
                    trajectory_signature,
                    false,
                    state_delta_seconds,
                );
                pelvis_acquisition_context =
                    Some((pelvis, authored, local_scalar_shift, trajectory_signature));
                if let Some(acquisition) = footwork
                    .pelvis_acquisition
                    .filter(|acquisition| acquisition.target.is_some())
                    && let Ok(parent) = parents.get(pelvis)
                    && let Ok(parent_global) =
                        transforms.p0().compute_global_transform(parent.parent())
                {
                    pelvis_acquisition_plan = Some((
                        acquisition,
                        parent_global.affine() * presented.compute_affine(),
                        parent_global.affine(),
                    ));
                }
                if let Ok(mut pelvis) = transforms.p1().get_mut(pelvis) {
                    *pelvis = presented;
                }
            }
        }
        let mut desired_raised_pelvis_shift: f32 = 0.0;
        let mut pelvis_acquisition_reach_samples = Vec::with_capacity(128);
        let mut left_guard_hip_proof = None;
        let mut right_guard_hip_proof = None;
        let guard_contact_prediction_ticks = guard_cadence_contact_tick_span(
            skeleton.gait_phase,
            skeleton.raised_locomotion().speed(),
        )
        .map(|timing| timing.total_ticks.get())
        .unwrap_or(0);
        let guard_trajectory_signature = pelvis_acquisition_context.map(|context| context.3);
        if raised_solver_follower {
            // Guard already has useful authored knee flexion. Lower the pelvis
            // only when a retained world-space ankle target would otherwise be
            // outside the analytic chain's reach; a fixed stance drop created
            // a second, delayed crouch after quickstep contact.
            let raised_contact_ownership = raised_states.get(owner).ok().map(|footwork| {
                let moving = skeleton.raised_locomotion().is_moving();
                let body_relative_gait = moving
                    && !memory.quickstep_handoff_pending
                    && !memory.quickstep_guard_stance_held
                    && !footwork.release_handoff_active;
                // During ordinary gait the local Planted/Swinging state is
                // the ownership authority. Prove the pelvis against both
                // terrain-projected contact paths so the next foot is feasible
                // before it becomes planted; support reporting still exposes
                // only the actual planted side. Terrain contact is a sensor
                // and must not delay this preparation until after hard reach.
                let left = if body_relative_gait {
                    true
                } else {
                    raised_leg_contributes_pelvis_reach(footwork, moving, true)
                        || raised_leg_is_stationary_contact_candidate(footwork, moving, true)
                };
                let right = if body_relative_gait {
                    true
                } else {
                    raised_leg_contributes_pelvis_reach(footwork, moving, false)
                        || raised_leg_is_stationary_contact_candidate(footwork, moving, false)
                };
                let contact_target = |target: Option<Vec3>| {
                    target.map(|target| {
                        if !moving || body_relative_gait {
                            terrain_conformed_guard_target(
                                target,
                                terrain.and_then(|terrain| terrain.height_at(target.xz())),
                            )
                        } else {
                            target
                        }
                    })
                };
                (
                    (left, contact_target(footwork.left_solve_target)),
                    (right, contact_target(footwork.right_solve_target)),
                )
            });
            for (left, upper_role, lower_role, foot_role, target, contact_owned) in [
                (
                    true,
                    BoneRole::ThighLeft,
                    BoneRole::ShinLeft,
                    BoneRole::FootLeft,
                    raised_contact_ownership.and_then(|ownership| ownership.0.1),
                    raised_contact_ownership.is_some_and(|ownership| ownership.0.0),
                ),
                (
                    false,
                    BoneRole::ThighRight,
                    BoneRole::ShinRight,
                    BoneRole::FootRight,
                    raised_contact_ownership.and_then(|ownership| ownership.1.1),
                    raised_contact_ownership.is_some_and(|ownership| ownership.1.0),
                ),
            ] {
                let (Some(&upper), Some(&lower), Some(&foot), Some(target)) = (
                    rig.get(&upper_role),
                    rig.get(&lower_role),
                    rig.get(&foot_role),
                    target,
                ) else {
                    continue;
                };
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
                let warning_reach = guard_warning_reach(upper_length, lower_length);
                // Start the bounded pelvis response before a planted chain
                // reaches the solver warning boundary. This reserve scales
                // with the actual limb rather than character world scale or a
                // route-specific speed, and only contact-owned legs request it.
                let support_response_reach =
                    (warning_reach - (upper_length + lower_length) * 0.04).max(0.0);
                let hard_reach = (upper_length * upper_length
                    + lower_length * lower_length
                    + 2.0 * upper_length * lower_length * MIN_KNEE_FLEXION.cos())
                .sqrt();
                let proof_slot = if left {
                    &mut left_guard_hip_proof
                } else {
                    &mut right_guard_hip_proof
                };
                if let Some((acquisition, presented_pelvis, parent_affine)) =
                    pelvis_acquisition_plan
                {
                    let hip_in_pelvis = presented_pelvis
                        .inverse()
                        .transform_point3(upper_snapshot.global.translation());
                    *proof_slot = guard_trajectory_signature.map(|signature| GuardHipPathProof {
                        start_tick: semantic_tick,
                        contact_tick: semantic_tick
                            .saturating_add(u64::from(guard_contact_prediction_ticks)),
                        sequence: skeleton.raised_locomotion().step_sequence(),
                        swing_left: left,
                        accepts_preemptive_cadence_confirmation: false,
                        trajectory_signature: signature,
                        rig_origin,
                        pelvis_parent_affine: Some(parent_affine),
                        hip_in_pelvis: Some(hip_in_pelvis),
                        fixed_hip: upper_snapshot.global.translation(),
                        pelvis_acquisition: Some(acquisition),
                        scalar_start: raised_pelvis_follower_seed(memory),
                        scalar_recovery: memory.raised_pelvis_recovery,
                        scalar_desired: 0.0,
                        warning_reach,
                        hard_reach,
                    });
                    let remaining_ticks = guard_contact_prediction_ticks;
                    // Admit every semantic presentation sample in this
                    // immutable rest-to-rest segment. The center follows the
                    // replicated controller p/v/a; the radius permits any
                    // future acceleration inside the tactical controller's
                    // absolute acceleration contract. Cadence admission below
                    // keeps this exact support owner valid through terminal.
                    for tick in 0..=remaining_ticks {
                        let seconds = tick as f32 / CONTINUITY_SAMPLE_HZ;
                        let progress = (acquisition.progress
                            + seconds / acquisition.duration_seconds.max(f32::EPSILON))
                        .min(1.0);
                        let mut sampled_acquisition = acquisition;
                        sampled_acquisition.progress = progress;
                        let sampled_pelvis = guard_pelvis_transform_sample(sampled_acquisition);
                        let response =
                            tactical_movement_acceleration_hz_for_guard(WeaponGuardState::Raised);
                        let signature = acquisition.trajectory_signature;
                        let response_weight = 1.0 - (-response * seconds).exp();
                        let controller_delta = signature.command_velocity * seconds
                            + (signature.world_velocity - signature.command_velocity)
                                * (response_weight / response);
                        // Replay the exact speed-capped controller response
                        // and the exact bounded body-facing turn used by the
                        // local player. Command changes invalidate this proof
                        // before the next semantic sample; no arbitrary turn
                        // radius is subtracted from the anatomical reserve.
                        let unturned_hip = (parent_affine * sampled_pelvis.compute_affine())
                            .transform_point3(hip_in_pelvis);
                        let predicted_body_rotation =
                            guard_predicted_body_rotation(signature, seconds);
                        let body_delta =
                            predicted_body_rotation * signature.body_rotation.inverse();
                        let sampled_hip = rig_origin
                            + body_delta * (unturned_hip - rig_origin)
                            + controller_delta;
                        let admitted_warning = support_response_reach;
                        if contact_owned {
                            desired_raised_pelvis_shift = desired_raised_pelvis_shift.min(
                                required_hip_shift_for_reach(sampled_hip, target, admitted_warning)
                                    .clamp(-0.25, 0.0),
                            );
                        }
                        pelvis_acquisition_reach_samples.push((
                            left,
                            contact_owned,
                            tick,
                            sampled_hip,
                            target,
                            admitted_warning,
                        ));
                    }
                } else {
                    let hip = upper_snapshot.global.translation();
                    if let Some(signature) = guard_trajectory_signature {
                        *proof_slot = Some(GuardHipPathProof {
                            start_tick: semantic_tick,
                            contact_tick: semantic_tick
                                .saturating_add(u64::from(guard_contact_prediction_ticks)),
                            sequence: skeleton.raised_locomotion().step_sequence(),
                            swing_left: left,
                            accepts_preemptive_cadence_confirmation: false,
                            trajectory_signature: signature,
                            rig_origin,
                            pelvis_parent_affine: None,
                            hip_in_pelvis: None,
                            fixed_hip: hip,
                            pelvis_acquisition: None,
                            scalar_start: raised_pelvis_follower_seed(memory),
                            scalar_recovery: memory.raised_pelvis_recovery,
                            scalar_desired: 0.0,
                            warning_reach,
                            hard_reach,
                        });
                        for tick in 0..=guard_contact_prediction_ticks {
                            let seconds = tick as f32 / CONTINUITY_SAMPLE_HZ;
                            let response = tactical_movement_acceleration_hz_for_guard(
                                WeaponGuardState::Raised,
                            );
                            let response_weight = 1.0 - (-response * seconds).exp();
                            let controller_delta = signature.command_velocity * seconds
                                + (signature.world_velocity - signature.command_velocity)
                                    * (response_weight / response);
                            let predicted_body_rotation =
                                guard_predicted_body_rotation(signature, seconds);
                            let body_delta =
                                predicted_body_rotation * signature.body_rotation.inverse();
                            let sampled_hip =
                                rig_origin + body_delta * (hip - rig_origin) + controller_delta;
                            if contact_owned {
                                desired_raised_pelvis_shift = desired_raised_pelvis_shift.min(
                                    required_hip_shift_for_reach(
                                        sampled_hip,
                                        target,
                                        support_response_reach,
                                    )
                                    .clamp(-0.25, 0.0),
                                );
                            }
                            pelvis_acquisition_reach_samples.push((
                                left,
                                contact_owned,
                                tick,
                                sampled_hip,
                                target,
                                support_response_reach,
                            ));
                        }
                    }
                }
            }
        }
        let cadence_seconds_remaining = guard_cadence_contact_tick_span(
            skeleton.gait_phase,
            skeleton.raised_locomotion().speed(),
        )
        .map_or(0.0, SegmentTickSpan::duration_seconds);
        let acquisition_within_cadence =
            pelvis_acquisition_plan.is_none_or(|(acquisition, _, _)| {
                let target = acquisition.target.unwrap_or(acquisition.start);
                let positive_finite_scale = acquisition.start.scale.is_finite()
                    && target.scale.is_finite()
                    && acquisition.start.scale.min_element() > 0.0
                    && target.scale.min_element() > 0.0;
                acquisition.advance_authorized
                    || (positive_finite_scale
                        && guard_pelvis_segment_fits_remaining_cadence_ticks(
                            acquisition,
                            cadence_seconds_remaining,
                        ))
            });
        let pelvis_acquisition_reach_admitted = acquisition_within_cadence
            && pelvis_acquisition_reach_samples
                .iter()
                .filter(|(_, contact_owned, _, _, _, _)| *contact_owned)
                .all(|(_, _, _, _, _, warning)| warning.is_finite() && *warning > 0.0);
        // Prove the combined owner, not an unrealizable target state: the
        // scalar correction and full-pelvis translation advance together on
        // each semantic tick. This lets a moving acquisition begin while the
        // bounded scalar follower is still establishing reserve without
        // freezing a nonzero translation derivative.
        let current_pelvis_follower = raised_pelvis_follower_seed(memory);
        let maximum_reach_tick = pelvis_acquisition_reach_samples
            .iter()
            .map(|(_, _, tick, _, _, _)| *tick)
            .max()
            .unwrap_or(0);
        let mut predicted_recovery = memory.raised_pelvis_recovery;
        let mut predicted_scalar = current_pelvis_follower;
        let mut predicted_scalar_by_tick = Vec::with_capacity(maximum_reach_tick as usize + 1);
        predicted_scalar_by_tick.push(predicted_scalar.position);
        for _ in 0..maximum_reach_tick {
            predicted_scalar = advance_guard_pelvis_scalar_semantic_ticks(
                predicted_scalar,
                &mut predicted_recovery,
                desired_raised_pelvis_shift,
                1.0 / CONTINUITY_SAMPLE_HZ,
            );
            predicted_scalar_by_tick.push(predicted_scalar.position);
        }
        if let Some(proof) = left_guard_hip_proof.as_mut() {
            proof.scalar_desired = desired_raised_pelvis_shift;
        }
        if let Some(proof) = right_guard_hip_proof.as_mut() {
            proof.scalar_desired = desired_raised_pelvis_shift;
        }
        let mut pelvis_acquisition_reach_admitted = pelvis_acquisition_reach_admitted
            && pelvis_acquisition_plan.is_none_or(|(acquisition, _, _)| {
                acquisition.start_sequence == skeleton.raised_locomotion().step_sequence()
            })
            && pelvis_acquisition_reach_samples
                .iter()
                .filter(|(_, contact_owned, _, _, _, _)| *contact_owned)
                .all(|(_, _, tick, hip, target, warning)| {
                    let shift = predicted_scalar_by_tick
                        .get(*tick as usize)
                        .copied()
                        .unwrap_or(predicted_scalar.position);
                    (*hip + Vec3::Y * shift).distance(*target) <= *warning + 0.0001
                });
        if !pelvis_acquisition_reach_admitted
            && let Ok(mut footwork) = raised_states.get_mut(owner)
            && footwork.pelvis_acquisition.is_some_and(|acquisition| {
                acquisition.target.is_some()
                    && acquisition.progress > 0.0
                    && acquisition.progress < 1.0
            })
        {
            // An unexpected controller/support departure can invalidate a
            // previously admitted moving segment after it has nonzero p/v/a.
            // Never hold that derivative at a fixed progress, but never lift
            // the last grounded support either. Move that support into the
            // nearest terrain/reach intersection and carry it body-relative
            // until cadence establishes the next real contact. A short ground
            // skate is the deliberate fail-soft result; dual airborne feet and
            // a world-space leg left behind the body are forbidden states.
            for left in [true, false] {
                if !raised_leg_contributes_pelvis_reach(
                    &footwork,
                    skeleton.raised_locomotion().is_moving(),
                    left,
                ) {
                    continue;
                }
                let presented = if left {
                    footwork.left_solve_target
                } else {
                    footwork.right_solve_target
                }
                .or(if left {
                    memory.left_foot_world_target
                } else {
                    memory.right_foot_world_target
                })
                .unwrap_or(rig_origin);
                let proof = if left {
                    left_guard_hip_proof
                } else {
                    right_guard_hip_proof
                };
                let endpoint = proof
                    .and_then(|proof| {
                        proof.sample(semantic_tick).map(|hip| {
                            ground_safety_slide_endpoint(
                                presented,
                                hip,
                                proof.warning_reach,
                                terrain,
                            )
                        })
                    })
                    .unwrap_or(presented);
                let owner = SupportReleaseOwner::GroundSafetySlide {
                    owner_local: rig_rotation.inverse() * (endpoint - rig_origin),
                };
                if left {
                    footwork.left_support_release_owner = Some(owner);
                    footwork.left_support_weight = 1.0;
                } else {
                    footwork.right_support_release_owner = Some(owner);
                    footwork.right_support_weight = 1.0;
                }
            }
            pelvis_acquisition_reach_admitted = true;
        }
        if state_delta_seconds > 0.0 {
            let followed = advance_guard_pelvis_scalar_semantic_ticks(
                current_pelvis_follower,
                &mut memory.raised_pelvis_recovery,
                desired_raised_pelvis_shift,
                state_delta_seconds,
            );
            memory.raised_pelvis_shift = followed.position;
            memory.raised_pelvis_shift_velocity = followed.velocity;
            memory.raised_pelvis_shift_acceleration = followed.acceleration;
            memory.raised_pelvis_follower_valid = true;
            if let Ok(mut footwork) = raised_states.get_mut(owner) {
                authorize_guard_pelvis_acquisition(
                    &mut footwork,
                    pelvis_acquisition_reach_admitted,
                );
            }
        }
        if let Some((pelvis, authored, local_scalar_shift, trajectory_signature)) =
            pelvis_acquisition_context
            && let Ok(mut footwork) = raised_states.get_mut(owner)
        {
            let presented = condition_guard_pelvis_acquisition(
                &mut footwork,
                authored,
                local_scalar_shift,
                skeleton.raised_locomotion().is_moving(),
                skeleton.raised_locomotion().step_sequence(),
                trajectory_signature,
                evaluation_advances,
                state_delta_seconds,
            );
            if let Ok(mut pelvis) = transforms.p1().get_mut(pelvis) {
                *pelvis = presented;
            }
        }
        let raised_pelvis_shift = memory.raised_pelvis_shift;
        let release_pelvis_delta = raised_states
            .get(owner)
            .ok()
            .filter(|state| state.release_handoff_active)
            .map(|state| {
                rig_rotation
                    * retained_guard_release_pelvis_offset(
                        state.release_pelvis_offset_owner,
                        state.release_handoff_progress,
                    )
            })
            .unwrap_or(Vec3::ZERO);
        let pelvis_world_delta = Vec3::Y * raised_pelvis_shift + release_pelvis_delta;
        if pelvis_world_delta.length_squared() > 0.000001
            && let Some(&pelvis) = rig.get(&BoneRole::Pelvis)
        {
            let local_delta = parents
                .get(pelvis)
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
                        .transform_vector3(pelvis_world_delta)
                })
                .unwrap_or(pelvis_world_delta);
            if let Ok(mut transform) = transforms.p1().get_mut(pelvis) {
                transform.translation += local_delta;
            }
        }
        if raised_solver_follower {
            prepare_slope_rotation_cache(&mut memory, SlopeAlignmentMode::Raised);
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
            let Some((_, _, left_foot_snapshot)) = snapshot_chain(
                left_upper,
                left_lower,
                left_foot,
                &parents,
                &transforms.p0(),
            ) else {
                continue;
            };
            let Some((_, _, right_foot_snapshot)) = snapshot_chain(
                right_upper,
                right_lower,
                right_foot,
                &parents,
                &transforms.p0(),
            ) else {
                continue;
            };
            let mut footwork = raised_states
                .get_mut(owner)
                .map(|state| *state)
                .unwrap_or_default();
            let contact_driven_guard = skeleton.raised_locomotion().is_moving()
                && !memory.quickstep_handoff_pending
                && !memory.quickstep_guard_stance_held
                && !footwork.release_handoff_active;
            let previous_left_support = terrain_leg_has_support(footwork.left_support_weight);
            let previous_right_support = terrain_leg_has_support(footwork.right_support_weight);
            let tick = semantic_tick;
            let advances = footwork.evaluation_tick != Some(tick);
            footwork.evaluation_tick = Some(tick);
            if advances {
                footwork.left_contact_abort_event = None;
                footwork.right_contact_abort_event = None;
            }
            let phase = skeleton.gait_phase.rem_euclid(1.0);
            let half_step = (phase >= 0.5) as u8;
            let swing_left = skeleton.raised_locomotion().swing_foot() == Some(LeadFoot::Left);
            // Pelvis lowering must not lower the semantic movement plane.
            // Recover the pre-drop authored ankle positions for persistent
            // flat plants; the analytic solve bends the lowered legs to them.
            let left_authored =
                left_foot_snapshot.global.translation() + Vec3::Y * -raised_pelvis_shift;
            let right_authored =
                right_foot_snapshot.global.translation() + Vec3::Y * -raised_pelvis_shift;
            let live_speed = skeleton.world_velocity.with_y(0.0).length();
            let visible_left = if memory.quickstep_handoff_pending {
                let landing_local = memory
                    .quickstep_left_landing_local
                    .unwrap_or_else(|| rig_rotation.inverse() * (left_authored - rig_origin));
                memory.quickstep_left_landing_local = Some(landing_local);
                rig_origin + rig_rotation * landing_local
            } else {
                memory.left_foot_world_target.unwrap_or(left_authored)
            };
            let visible_right = if memory.quickstep_handoff_pending {
                let landing_local = memory
                    .quickstep_right_landing_local
                    .unwrap_or_else(|| rig_rotation.inverse() * (right_authored - rig_origin));
                memory.quickstep_right_landing_local = Some(landing_local);
                rig_origin + rig_rotation * landing_local
            } else {
                memory.right_foot_world_target.unwrap_or(right_authored)
            };
            if memory.quickstep_handoff_pending {
                memory.left_foot_world_target = Some(visible_left);
                memory.right_foot_world_target = Some(visible_right);
                memory.left_foot_plant = Some(visible_left);
                memory.right_foot_plant = Some(visible_right);
                memory.left_foot_plant_acquired = true;
                memory.right_foot_plant_acquired = true;
            }
            if advances
                && !contact_driven_guard
                && guard_pending_cadence_edge_can_be_consumed(&footwork)
                && let Some((pending_swing_left, pending_sequence)) = footwork.pending_cadence_edge
            {
                consume_pending_guard_cadence_edge(
                    &mut footwork,
                    pending_swing_left,
                    pending_sequence,
                    visible_left,
                    visible_right,
                    half_step,
                    rig_origin,
                    rig_rotation,
                    left_authored,
                    right_authored,
                );
            }
            let discontinuous =
                footwork.initialized && rig_origin.distance_squared(footwork.step_origin) > 4.0;
            let mut sequence_delta = guard_step_sequence_delta(
                footwork.step_sequence,
                skeleton.raised_locomotion().step_sequence(),
            );
            if advances
                && !contact_driven_guard
                && confirm_preemptive_guard_cadence_edge(
                    &mut footwork,
                    sequence_delta,
                    swing_left,
                    skeleton.raised_locomotion().step_sequence(),
                )
            {
                sequence_delta = 0;
            }
            // A terrain-proven contact is the physical gait edge. Do not hold
            // the opposite planted foot motionless until a slightly later
            // replicated cadence edge: by then a short-legged or fast-moving
            // character may already have exhausted its reach reserve. Begin
            // the opposite swing from the exact visible plants immediately;
            // the authoritative N -> N+1 edge is still retained below as the
            // identity confirmation for the already-running contact plan.
            if advances
                && !contact_driven_guard
                && begin_next_guard_swing_after_contact(
                    &mut footwork,
                    skeleton.raised_locomotion().is_moving(),
                    sequence_delta,
                    visible_left,
                    visible_right,
                    half_step,
                    rig_origin,
                    rig_rotation,
                    left_authored,
                    right_authored,
                )
            {
                reseed_guard_cadence_ideal_history(&mut footwork, visible_left, visible_right);
            }
            // A dynamics-proven contact may intentionally outlive the authored
            // cadence estimate. Keep its old swing identity and defer the new
            // complete identity until the endpoint lands; aborting here turns
            // a lawful slow step into an airborne release.
            if advances
                && !contact_driven_guard
                && sequence_delta == 1
                && footwork.swing_replan_segment.is_some_and(|segment| {
                    segment.end.is_contact()
                        && !segment.timing.is_complete()
                        && segment
                            .owner_epoch
                            .saturating_add(segment.timing.total_ticks.get() as u64)
                            != semantic_tick
                })
            {
                defer_guard_cadence_edge_for_contact_recovery(
                    &mut footwork,
                    swing_left,
                    skeleton.raised_locomotion().step_sequence(),
                );
            }
            // Presentation may predict a cadence edge between sparse
            // authoritative packets. A recovery owner from the preceding
            // half-step must not keep the old swing identity alive until an
            // unrelated later packet: retire that recovery at the semantic
            // edge and rebase the new cadence from the exact visible feet.
            let recovery_blocks_semantic_edge = footwork.swing_emergency_brake.is_some()
                || support_owner_blocks_cadence(footwork.left_support_release_owner)
                || support_owner_blocks_cadence(footwork.right_support_release_owner);
            let on_time_contact_must_complete = footwork
                .swing_replan_segment
                .is_some_and(|segment| segment.end.is_contact())
                || footwork.pelvis_acquisition.is_some_and(|acquisition| {
                    acquisition.target.is_some() && acquisition.progress < 1.0
                });
            if advances
                && !contact_driven_guard
                && guard_recovery_may_rebase_on_semantic_edge(
                    sequence_delta,
                    recovery_blocks_semantic_edge,
                    on_time_contact_must_complete,
                )
            {
                footwork.left_support_release_owner = None;
                footwork.right_support_release_owner = None;
                adopt_guard_movement_identity(
                    &mut footwork,
                    swing_left,
                    skeleton.raised_locomotion().step_sequence(),
                    visible_left,
                    visible_right,
                );
                footwork.half_step = half_step;
                footwork.step_origin = rig_origin;
                footwork.step_rotation = rig_rotation;
                footwork.swing_stance_local = rig_rotation.inverse()
                    * ((if swing_left {
                        left_authored
                    } else {
                        right_authored
                    }) - rig_origin);
                reseed_guard_cadence_ideal_history(&mut footwork, visible_left, visible_right);
            }
            let skipped_handoff =
                !contact_driven_guard && footwork.initialized && sequence_delta > 1;
            let lead_only_handoff = retain_guard_tracker_on_reinitialization(
                footwork.initialized,
                footwork.lead != skeleton.lead_foot(),
                discontinuous,
                skipped_handoff,
            );
            let retained_tracker = lead_only_handoff.then_some((
                footwork.left_target_velocity,
                footwork.right_target_velocity,
                footwork.left_target_acceleration,
                footwork.right_target_acceleration,
                footwork.left_desired_target,
                footwork.right_desired_target,
                footwork.left_ideal_velocity,
                footwork.right_ideal_velocity,
                footwork.left_ideal_acceleration,
                footwork.right_ideal_acceleration,
            ));
            if !footwork.release_handoff_active
                && !footwork.swing_release_owner_active
                && footwork.swing_replan_segment.is_none()
                && footwork.pelvis_acquisition.is_none()
                && (!footwork.initialized
                    || footwork.lead != skeleton.lead_foot()
                    || discontinuous
                    || skipped_handoff)
            {
                footwork = RaisedFootworkState {
                    initialized: true,
                    was_moving: skeleton.raised_locomotion().is_moving(),
                    awaiting_step_sequence: false,
                    half_step,
                    lead: skeleton.lead_foot(),
                    swing_left,
                    step_origin: rig_origin,
                    step_rotation: rig_rotation,
                    swing_stance_local: rig_rotation.inverse()
                        * ((if swing_left {
                            left_authored
                        } else {
                            right_authored
                        }) - rig_origin),
                    swing_start: if swing_left {
                        visible_left
                    } else {
                        visible_right
                    },
                    swing_end: if swing_left {
                        left_authored
                    } else {
                        right_authored
                    },
                    swing_replan_segment: None,
                    swing_release_owner_active: false,
                    swing_emergency_brake: None,
                    pending_cadence_edge: None,
                    left_plant: visible_left,
                    right_plant: visible_right,
                    evaluation_tick: Some(tick),
                    step_sequence: skeleton.raised_locomotion().step_sequence(),
                    left_support_weight: memory.left_support_weight.unwrap_or(0.0),
                    right_support_weight: memory.right_support_weight.unwrap_or(0.0),
                    left_solve_target: Some(visible_left),
                    right_solve_target: Some(visible_right),
                    left_command_target: Some(visible_left),
                    right_command_target: Some(visible_right),
                    left_target_velocity: retained_tracker.map_or(Vec3::ZERO, |state| state.0),
                    right_target_velocity: retained_tracker.map_or(Vec3::ZERO, |state| state.1),
                    left_target_acceleration: retained_tracker.map_or(Vec3::ZERO, |state| state.2),
                    right_target_acceleration: retained_tracker.map_or(Vec3::ZERO, |state| state.3),
                    left_desired_target: retained_tracker.and_then(|state| state.4),
                    right_desired_target: retained_tracker.and_then(|state| state.5),
                    left_ideal_velocity: retained_tracker.map_or(Vec3::ZERO, |state| state.6),
                    right_ideal_velocity: retained_tracker.map_or(Vec3::ZERO, |state| state.7),
                    left_ideal_acceleration: retained_tracker.map_or(Vec3::ZERO, |state| state.8),
                    right_ideal_acceleration: retained_tracker.map_or(Vec3::ZERO, |state| state.9),
                    left_ideal_history_valid: retained_tracker.is_some(),
                    right_ideal_history_valid: retained_tracker.is_some(),
                    left_support_release_owner: None,
                    right_support_release_owner: None,
                    release_handoff_active: false,
                    release_handoff_progress: 0.0,
                    release_left_start: Vec3::ZERO,
                    release_right_start: Vec3::ZERO,
                    visible_pelvis_owner_local: None,
                    visible_pelvis_local_transform: None,
                    pelvis_acquisition: None,
                    release_pelvis_offset_owner: Vec3::ZERO,
                    pivot_active: false,
                    pivot_left: false,
                    pivot_progress: 0.0,
                    pivot_origin: Vec3::ZERO,
                    pivot_start: Vec3::ZERO,
                    pivot_end: Vec3::ZERO,
                    left_knee_bend_world: None,
                    right_knee_bend_world: None,
                    left_end_direction: None,
                    right_end_direction: None,
                    left_hip_world: None,
                    right_hip_world: None,
                    left_hip_velocity: Vec3::ZERO,
                    right_hip_velocity: Vec3::ZERO,
                    left_solve_hip: None,
                    right_solve_hip: None,
                    left_solve_upper_length: None,
                    right_solve_upper_length: None,
                    left_solve_lower_length: None,
                    right_solve_lower_length: None,
                    left_commanded_pole: None,
                    right_commanded_pole: None,
                    left_contact_abort_event: None,
                    right_contact_abort_event: None,
                    left_motion_owner_epoch: semantic_tick,
                    right_motion_owner_epoch: semantic_tick,
                    left_motion_owner_kind: FootMotionOwnerKind::None,
                    right_motion_owner_kind: FootMotionOwnerKind::None,
                    raised_motion_owned_this_tick: raised_solver_follower,
                    contact_gait: None,
                };
            } else if !contact_driven_guard
                && guard_cadence_may_turnover(
                    advances,
                    sequence_delta,
                    footwork.swing_release_owner_active
                        || support_owner_blocks_cadence(footwork.left_support_release_owner)
                        || support_owner_blocks_cadence(footwork.right_support_release_owner),
                    footwork.swing_replan_segment.is_some_and(|segment| {
                        !(footwork.awaiting_step_sequence
                            && segment.timing.is_complete()
                            && segment.end.is_contact())
                    }),
                )
            {
                if footwork.swing_left {
                    footwork.left_plant = footwork.left_solve_target.unwrap_or(footwork.swing_end);
                } else {
                    footwork.right_plant =
                        footwork.right_solve_target.unwrap_or(footwork.swing_end);
                }
                footwork.half_step = half_step;
                footwork.awaiting_step_sequence = false;
                footwork.left_support_release_owner = None;
                footwork.right_support_release_owner = None;
                footwork.swing_replan_segment = None;
                footwork.step_sequence = skeleton.raised_locomotion().step_sequence();
                footwork.swing_left = swing_left;
                footwork.left_motion_owner_epoch = semantic_tick;
                footwork.right_motion_owner_epoch = semantic_tick;
                footwork.step_origin = rig_origin;
                footwork.step_rotation = rig_rotation;
                footwork.swing_stance_local = rig_rotation.inverse()
                    * ((if swing_left {
                        left_authored
                    } else {
                        right_authored
                    }) - rig_origin);
                footwork.swing_start = if swing_left {
                    footwork.left_plant
                } else {
                    footwork.right_plant
                };
                reseed_guard_cadence_ideal_history(&mut footwork, visible_left, visible_right);
            }
            let local_direction = skeleton
                .raised_locomotion()
                .local_direction()
                .normalize_or_zero();
            // Semantic controller axes are opposite the authored rig's X/Z
            // axes. The owner carries the single 180-degree body conversion.
            let rig_local_direction = -local_direction;
            let latched_speed = skeleton.raised_locomotion().speed();
            let left_planning_reach = foot_follow_reach_envelope(
                left_upper,
                left_lower,
                left_foot,
                skeleton.world_velocity * state_delta_seconds,
                &parents,
                &transforms.p0(),
            );
            let right_planning_reach = foot_follow_reach_envelope(
                right_upper,
                right_lower,
                right_foot,
                skeleton.world_velocity * state_delta_seconds,
                &parents,
                &transforms.p0(),
            );
            let quickstep_handoff_active = memory.quickstep_handoff_pending;
            if memory.quickstep_guard_stance_held && live_speed > 0.05 {
                memory.quickstep_guard_stance_held = false;
                memory.quickstep_left_landing_local = None;
                memory.quickstep_right_landing_local = None;
            }
            let live_step_scale = (live_speed / latched_speed.max(0.01)).clamp(0.0, 1.0);
            let step_length = guard_step_length(latched_speed) * live_step_scale;
            let quickstep_stance_held = memory.quickstep_guard_stance_held;
            let moving_cadence = skeleton.raised_locomotion().is_moving();
            let cadence_can_advance =
                moving_cadence && !quickstep_handoff_active && !quickstep_stance_held;
            let stationary_guard = guard_stationary_owns_pose(
                !moving_cadence || quickstep_handoff_active || quickstep_stance_held,
                footwork.swing_replan_segment.is_some()
                    || footwork.swing_emergency_brake.is_some()
                    || footwork.swing_release_owner_active,
            );
            if advances
                && !cadence_can_advance
                && footwork.swing_replan_segment.is_none()
                && footwork.swing_emergency_brake.is_none()
                && !footwork.swing_release_owner_active
                && footwork.left_support_release_owner.is_none()
                && footwork.right_support_release_owner.is_none()
            {
                // A stopped character has no future cadence edge that can
                // consume a deferred moving identity. Retaining it suppresses
                // stationary stance recovery and poisons the next movement
                // onset with an already-extended plant.
                footwork.pending_cadence_edge = None;
                footwork.awaiting_step_sequence = false;
            }
            if quickstep_handoff_active {
                // Residual velocity is the quickstep's landing brake, not a new
                // guard step. Carry the already completed guard stance with the
                // body until braking ends, then return it to stationary guard.
                reseed_raised_from_quickstep_handoff(
                    &mut footwork,
                    &memory,
                    visible_left,
                    visible_right,
                );
                footwork.was_moving = false;
                footwork.pivot_active = false;
                footwork.pivot_progress = 0.0;
            }
            if !stationary_guard && !footwork.was_moving {
                // A stationary pivot is presentation-only. Movement cancels
                // it immediately and replans the replicated swing from the
                // exact visible p/v/a below. Serializing the unfinished pivot
                // before locomotion let the root consume most of the planted
                // support's reach while no gait step was allowed to begin.
                let acquisition_swing_left = swing_left;
                adopt_guard_movement_identity(
                    &mut footwork,
                    acquisition_swing_left,
                    skeleton.raised_locomotion().step_sequence(),
                    visible_left,
                    visible_right,
                );
                let cadence_transfer_is_admitted = if footwork.swing_left {
                    guard_motion_can_transfer_to_cadence(
                        footwork.left_solve_target.unwrap_or(visible_left),
                        footwork.left_target_velocity,
                        footwork.left_target_acceleration,
                        left_planning_reach,
                        state_delta_seconds,
                        ReachMotionSample::Current,
                    )
                } else {
                    guard_motion_can_transfer_to_cadence(
                        footwork.right_solve_target.unwrap_or(visible_right),
                        footwork.right_target_velocity,
                        footwork.right_target_acceleration,
                        right_planning_reach,
                        state_delta_seconds,
                        ReachMotionSample::Current,
                    )
                };
                // There is no authoritative contact promise on the zero-
                // sequence movement-acquisition edge. Rebase the cadence from
                // the actually visible feet and let the typed contact/release
                // planner below choose a reachable body-relative path. The
                // former fallback installed a fixed world-space emergency
                // target and waited for an unrelated later sequence edge,
                // leaving the selected foot trailing behind the moving body.
                // Begin a new cadence from the feet that were actually
                // rendered during idle. A stationary pivot or initial guard
                // acquisition may have moved them away from the older cadence
                // seed even when no pivot remains active.
                footwork.step_origin = rig_origin;
                footwork.step_rotation = rig_rotation;
                footwork.swing_stance_local = rig_rotation.inverse()
                    * ((if footwork.swing_left {
                        visible_left
                    } else {
                        visible_right
                    }) - rig_origin);
                if cadence_transfer_is_admitted {
                    reseed_guard_cadence_ideal_history(&mut footwork, visible_left, visible_right);
                } else {
                    // Preserve the carried rendered derivatives. Only the
                    // semantic ideal changes at this ownership boundary; the
                    // stateful planner must brake/release them continuously.
                    if footwork.swing_left {
                        footwork.left_desired_target = Some(visible_left);
                        footwork.left_ideal_history_valid = false;
                    } else {
                        footwork.right_desired_target = Some(visible_right);
                        footwork.right_ideal_history_valid = false;
                    }
                }
                footwork.pivot_active = false;
                footwork.pivot_progress = 0.0;
            }
            if !stationary_guard {
                // A short controller-airborne sample can yield raised IK for
                // one frame and clear both reported supports. On grounded
                // reacquisition, establish the semantic support foot on the
                // terrain before proving a swing. Otherwise every exact proof
                // rejects for lack of support and the gait can wait forever.
                let semantic_swing_left = footwork.swing_left;
                reacquire_grounded_guard_support(
                    &mut footwork,
                    semantic_swing_left,
                    visible_left,
                    visible_right,
                    terrain,
                );
            }
            let planning_origin = if live_step_scale <= 0.05 {
                rig_origin
            } else {
                footwork.step_origin
            };
            let opposite_plant = if footwork.swing_left {
                footwork.right_plant
            } else {
                footwork.left_plant
            };
            footwork.swing_end = plan_guard_step_endpoint(
                planning_origin,
                footwork.step_rotation,
                footwork.swing_stance_local,
                rig_local_direction,
                step_length,
                footwork.swing_left,
                opposite_plant,
            );
            // An attack recovery may finish in the middle of an authoritative
            // raised-locomotion half-step. Replaying that already-consumed
            // phase from newly recovered plants would move a foot a large
            // distance on the handoff frame. Hold both plants until the next
            // replicated step sequence starts, then follow it normally.
            let step_progress = if footwork.awaiting_step_sequence {
                0.0
            } else {
                (phase * 2.0).fract()
            };
            let horizontal_progress = quintic_progress(step_progress);
            let mut swing_target = footwork
                .swing_start
                .lerp(footwork.swing_end, horizontal_progress);
            let mut left_target = footwork.left_plant;
            let mut right_target = footwork.right_plant;
            let mut left_explicit_ideal_motion = None;
            let mut right_explicit_ideal_motion = None;
            let mut left_direct_c2_sample = false;
            let mut right_direct_c2_sample = false;
            let support_target = if footwork.swing_left {
                right_target
            } else {
                left_target
            };
            swing_target = constrain_guard_swing_to_live_corridor(
                swing_target,
                support_target,
                rig_origin,
                rig_rotation,
                footwork.swing_stance_local.x.signum(),
            );
            let mut terrain_swing_end = constrain_guard_swing_to_live_corridor(
                footwork.swing_end,
                support_target,
                rig_origin,
                rig_rotation,
                footwork.swing_stance_local.x.signum(),
            );
            if enabled.0
                && let Some(terrain) = terrain
            {
                left_target = terrain_conformed_guard_target(
                    left_target,
                    terrain.height_at(left_target.xz()),
                );
                right_target = terrain_conformed_guard_target(
                    right_target,
                    terrain.height_at(right_target.xz()),
                );
                terrain_swing_end = terrain_conformed_guard_target(
                    terrain_swing_end,
                    terrain.height_at(terrain_swing_end.xz()),
                );
                swing_target.y = footwork
                    .swing_start
                    .y
                    .lerp(terrain_swing_end.y, horizontal_progress);
            }
            swing_target.y += c2_swing_arch(step_progress) * 0.10;
            if footwork.swing_left {
                left_target = swing_target;
            } else {
                right_target = swing_target;
            }
            let left_hip_current = transforms
                .p0()
                .compute_global_transform(left_upper)
                .ok()
                .map(|global| global.translation());
            let right_hip_current = transforms
                .p0()
                .compute_global_transform(right_upper)
                .ok()
                .map(|global| global.translation());
            if !contact_driven_guard {
                footwork.contact_gait = None;
            }
            if contact_driven_guard {
                // Moving raised locomotion has exactly one lower-body owner:
                // a body-relative procedural gait. Replicated cadence chooses
                // phase and side, while terrain-conformed targets provide the
                // physical stance. Persistent world contacts, release waits,
                // and planner epochs must not compete with this generator.
                footwork.swing_replan_segment = None;
                footwork.swing_emergency_brake = None;
                footwork.swing_release_owner_active = false;
                footwork.left_support_release_owner = None;
                footwork.right_support_release_owner = None;
                footwork.pending_cadence_edge = None;
                footwork.awaiting_step_sequence = false;
                let gait_origin = left_hip_current
                    .zip(right_hip_current)
                    .map(|(left, right)| {
                        let mut center = (left + right) * 0.5;
                        center.y = rig_origin.y;
                        center
                    })
                    .unwrap_or(rig_origin);
                let gait_lateral = left_hip_current
                    .zip(right_hip_current)
                    .map(|(left, right)| (right - left).with_y(0.0).normalize_or_zero())
                    .filter(|axis| axis.length_squared() > 0.5)
                    .unwrap_or(rig_rotation * Vec3::X);
                let morphology_scale = left_planning_reach
                    .zip(right_planning_reach)
                    .map(|(left, right)| {
                        ((left.hard_reach() + right.hard_reach()) * 0.5 / 0.938_72).clamp(0.25, 4.0)
                    })
                    .unwrap_or(1.0);
                let step_rate = overgrowth_guard_step_rate(live_speed, morphology_scale);
                let desired = overgrowth_guard_foot_targets(
                    gait_origin,
                    gait_lateral,
                    footwork.swing_stance_local,
                    skeleton.world_velocity,
                    step_rate,
                    enabled.0,
                    terrain,
                );
                let mut gait = footwork.contact_gait.unwrap_or(ContactDrivenGuardGait {
                    left_plant: visible_left,
                    right_plant: visible_right,
                    swing_active: false,
                    swing_left,
                    swing_start: if swing_left {
                        visible_left
                    } else {
                        visible_right
                    },
                    swing_target: if swing_left { desired.0 } else { desired.1 },
                    progress: 0.0,
                    last_tick: semantic_tick,
                });
                let elapsed_ticks = semantic_tick.saturating_sub(gait.last_tick);
                gait.last_tick = semantic_tick;
                let stance_error = overgrowth_guard_stance_error(morphology_scale);
                if !gait.swing_active {
                    let left_error = gait.left_plant.distance(desired.0);
                    let right_error = gait.right_plant.distance(desired.1);
                    if left_error.max(right_error) > stance_error {
                        gait.swing_left = if (left_error - right_error).abs() <= 0.001 {
                            swing_left
                        } else {
                            left_error > right_error
                        };
                        gait.swing_start = if gait.swing_left {
                            gait.left_plant
                        } else {
                            gait.right_plant
                        };
                        gait.swing_target = if gait.swing_left {
                            desired.0
                        } else {
                            desired.1
                        };
                        gait.progress = 0.0;
                        gait.swing_active = true;
                    }
                }
                if gait.swing_active {
                    gait.swing_target = if gait.swing_left {
                        desired.0
                    } else {
                        desired.1
                    };
                    if advances {
                        gait.progress = (gait.progress
                            + elapsed_ticks as f32 * step_rate / CONTINUITY_SAMPLE_HZ)
                            .min(1.0);
                    }
                    let blend = quintic_progress(gait.progress);
                    let mut swing_target = gait.swing_start.lerp(gait.swing_target, blend);
                    swing_target.y += c2_swing_arch(gait.progress) * GUARD_PIVOT_LIFT_METRES;
                    if gait.swing_left {
                        left_target = swing_target;
                        right_target = gait.right_plant;
                    } else {
                        left_target = gait.left_plant;
                        right_target = swing_target;
                    }
                    if advances && gait.progress >= 1.0 {
                        if gait.swing_left {
                            gait.left_plant = gait.swing_target;
                            left_target = gait.left_plant;
                        } else {
                            gait.right_plant = gait.swing_target;
                            right_target = gait.right_plant;
                        }
                        gait.swing_active = false;
                        gait.progress = 0.0;
                        footwork.step_sequence = footwork.step_sequence.wrapping_add(1);
                    }
                } else {
                    left_target = gait.left_plant;
                    right_target = gait.right_plant;
                }
                let direct_motion =
                    |previous: Option<Vec3>, previous_velocity: Vec3, target: Vec3| {
                        if !advances || elapsed_ticks == 0 {
                            (previous_velocity, Vec3::ZERO)
                        } else {
                            let dt = elapsed_ticks as f32 / CONTINUITY_SAMPLE_HZ;
                            let velocity =
                                previous.map_or(Vec3::ZERO, |previous| (target - previous) / dt);
                            (velocity, (velocity - previous_velocity) / dt)
                        }
                    };
                let left_motion = direct_motion(
                    footwork.left_solve_target,
                    footwork.left_target_velocity,
                    left_target,
                );
                let right_motion = direct_motion(
                    footwork.right_solve_target,
                    footwork.right_target_velocity,
                    right_target,
                );
                left_explicit_ideal_motion = Some(left_motion);
                right_explicit_ideal_motion = Some(right_motion);
                left_direct_c2_sample = true;
                right_direct_c2_sample = true;
                footwork.contact_gait = Some(gait);
                footwork.swing_left = gait.swing_left;
                if gait.swing_left
                    && let (Some(hip), Some(reach)) = (left_hip_current, left_planning_reach)
                {
                    left_target = constrain_guard_gait_target_to_reach(
                        left_target,
                        hip,
                        reach.warning_reach() * 0.97,
                    );
                }
                if !gait.swing_left
                    && let (Some(hip), Some(reach)) = (right_hip_current, right_planning_reach)
                {
                    right_target = constrain_guard_gait_target_to_reach(
                        right_target,
                        hip,
                        reach.warning_reach() * 0.97,
                    );
                }
                footwork.left_plant = gait.left_plant;
                footwork.right_plant = gait.right_plant;
                footwork.left_support_weight = if gait.swing_active && gait.swing_left {
                    0.0
                } else {
                    1.0
                };
                footwork.right_support_weight = if gait.swing_active && !gait.swing_left {
                    0.0
                } else {
                    1.0
                };
            }
            let left_hip_trajectory = left_planning_reach.and_then(|reach| {
                PredictedHipTrajectory::from_retained_motion(
                    reach,
                    footwork.left_hip_world,
                    footwork.left_hip_velocity,
                    state_delta_seconds,
                    MAX_PELVIS_CORRECTION_STEP,
                    PELVIS_CORRECTION_SPEED,
                )
            });
            let right_hip_trajectory = right_planning_reach.and_then(|reach| {
                PredictedHipTrajectory::from_retained_motion(
                    reach,
                    footwork.right_hip_world,
                    footwork.right_hip_velocity,
                    state_delta_seconds,
                    MAX_PELVIS_CORRECTION_STEP,
                    PELVIS_CORRECTION_SPEED,
                )
            });
            let locally_preemptive_swing = footwork.step_sequence
                == skeleton.raised_locomotion().step_sequence()
                && footwork.swing_left != swing_left;
            let cadence_contact_timing = if locally_preemptive_swing {
                guard_following_cadence_contact_tick_span(phase, latched_speed)
            } else {
                guard_cadence_contact_tick_span(phase, latched_speed)
            };
            let swing_guard_hip_proof = (if footwork.swing_left {
                left_guard_hip_proof
            } else {
                right_guard_hip_proof
            })
            .map(|mut proof| {
                if contact_driven_guard {
                    proof.sequence = footwork.step_sequence;
                    proof.trajectory_signature.sequence = footwork.step_sequence;
                }
                proof.accepts_preemptive_cadence_confirmation = locally_preemptive_swing;
                proof
            });
            let contact_driven_tick_budget = (if footwork.swing_left {
                right_guard_hip_proof
            } else {
                left_guard_hip_proof
            })
            .map(|proof| contact_driven_guard_support_tick_budget(proof, support_target))
            .unwrap_or(1);
            if advances
                && !stationary_guard
                && !footwork.awaiting_step_sequence
                && footwork.swing_replan_segment.is_none()
                && !contact_driven_guard
            {
                let (presented, velocity, acceleration) = if footwork.swing_left {
                    (
                        footwork.left_solve_target.unwrap_or(visible_left),
                        footwork.left_target_velocity,
                        footwork.left_target_acceleration,
                    )
                } else {
                    (
                        footwork.right_solve_target.unwrap_or(visible_right),
                        footwork.right_target_velocity,
                        footwork.right_target_acceleration,
                    )
                };
                let mut plan = if contact_driven_guard {
                    plan_contact_driven_guard_segment(
                        presented,
                        velocity,
                        acceleration,
                        terrain_swing_end,
                        contact_driven_tick_budget,
                        swing_guard_hip_proof,
                        terrain,
                        |progress| {
                            guard_contact_candidate_at_progress(
                                presented,
                                terrain_swing_end,
                                progress,
                                support_target,
                                rig_origin,
                                rig_rotation,
                                footwork.swing_stance_local.x.signum(),
                                enabled.0,
                                terrain,
                            )
                        },
                    )
                } else {
                    cadence_contact_timing.map_or_else(
                        || {
                            GuardFootEndpointPlan::MustReleaseOrReplan(
                                GuardFootReleasePlan::EmergencyBrake { presented },
                            )
                        },
                        |timing| {
                            plan_guard_c2_contact_segment(
                                presented,
                                velocity,
                                acceleration,
                                timing,
                                swing_guard_hip_proof,
                                |progress| {
                                    guard_contact_candidate_at_progress(
                                        presented,
                                        terrain_swing_end,
                                        progress,
                                        support_target,
                                        rig_origin,
                                        rig_rotation,
                                        footwork.swing_stance_local.x.signum(),
                                        enabled.0,
                                        terrain,
                                    )
                                },
                            )
                        },
                    )
                };
                if !matches!(plan, GuardFootEndpointPlan::Segment(_))
                    && !contact_driven_guard
                    && let Some(timing) = cadence_contact_timing
                    && let Some(recovery) = plan_guard_recovery_contact_segment(
                        presented,
                        velocity,
                        acceleration,
                        timing,
                        swing_guard_hip_proof,
                        terrain,
                    )
                {
                    plan = GuardFootEndpointPlan::Segment(recovery);
                }
                match plan {
                    GuardFootEndpointPlan::Segment(segment) => {
                        // Even when a swing must outlive the nominal cadence,
                        // the opposite foot remains the single planted support.
                        // A second body-relative slide is an alternate gait,
                        // not a contact-preserving fallback.
                        footwork.swing_replan_segment =
                            Some(segment.with_owner_epoch(semantic_tick));
                        footwork.swing_emergency_brake = None;
                        if contact_driven_guard {
                            // Support changes atomically with installation of
                            // the concrete swing owner. Before this point both
                            // landed feet remain truthful pelvis supports.
                            footwork.left_support_weight =
                                if footwork.swing_left { 0.0 } else { 1.0 };
                            footwork.right_support_weight =
                                if footwork.swing_left { 1.0 } else { 0.0 };
                        }
                        if footwork.swing_left {
                            footwork.left_desired_target = Some(segment.start);
                            footwork.left_ideal_velocity = segment.start_velocity;
                            footwork.left_ideal_acceleration = segment.start_acceleration;
                            footwork.left_ideal_history_valid = true;
                        } else {
                            footwork.right_desired_target = Some(segment.start);
                            footwork.right_ideal_velocity = segment.start_velocity;
                            footwork.right_ideal_acceleration = segment.start_acceleration;
                            footwork.right_ideal_history_valid = true;
                        }
                    }
                    GuardFootEndpointPlan::MustReleaseOrReplan(release) => {
                        if contact_driven_guard
                            && matches!(release, GuardFootReleasePlan::EmergencyBrake { .. })
                        {
                            // No bounded spatial recovery exists yet. Keep the
                            // planted support and retry without inventing a
                            // cadence wait or a zero-motion gait owner.
                            footwork.swing_replan_segment = None;
                            footwork.swing_emergency_brake = None;
                            footwork.swing_release_owner_active = false;
                            footwork.awaiting_step_sequence = false;
                        } else {
                            install_guard_swing_fallback(
                                &mut footwork,
                                release,
                                semantic_tick,
                                rig_origin,
                                rig_rotation,
                                left_authored,
                                right_authored,
                            );
                            if contact_driven_guard {
                                // This is an inward workspace recovery, not a
                                // promise to synchronize with replicated gait.
                                footwork.awaiting_step_sequence = false;
                            }
                        }
                    }
                }
            }
            if advances && state_delta_seconds > f32::EPSILON {
                if let Some(current) = left_hip_current {
                    footwork.left_hip_velocity = footwork
                        .left_hip_world
                        .map(|previous| (current - previous) / state_delta_seconds)
                        .unwrap_or(skeleton.world_velocity);
                    footwork.left_hip_world = Some(current);
                }
                if let Some(current) = right_hip_current {
                    footwork.right_hip_velocity = footwork
                        .right_hip_world
                        .map(|previous| (current - previous) / state_delta_seconds)
                        .unwrap_or(skeleton.world_velocity);
                    footwork.right_hip_world = Some(current);
                }
            }
            if let Some(mut segment) = footwork.swing_replan_segment {
                // The cadence swing owner supersedes a stale release owner on
                // that same leg. Leaving both installed lets the support path
                // overwrite the direct analytic sample later in this system.
                if footwork.swing_left {
                    footwork.left_support_release_owner = None;
                } else {
                    footwork.right_support_release_owner = None;
                }
                let support_owner_valid = if footwork.swing_left {
                    terrain_leg_has_support(footwork.right_support_weight)
                        && support_owner_preserves_contact(footwork.right_support_release_owner)
                } else {
                    terrain_leg_has_support(footwork.left_support_weight)
                        && support_owner_preserves_contact(footwork.left_support_release_owner)
                };
                let contact_proof_invalid = advances
                    && match segment.reach {
                        GuardSegmentReachProof::Exact(proof) => {
                            let live_hip = if proof.swing_left {
                                left_hip_current
                            } else {
                                right_hip_current
                            };
                            if contact_driven_guard && segment.end.is_contact() {
                                !contact_driven_guard_owner_is_live(
                                    proof,
                                    footwork.step_sequence,
                                    footwork.swing_left,
                                    live_hip,
                                    support_owner_valid,
                                )
                            } else {
                                let live_signature = guard_trajectory_signature.map(|signature| {
                                    normalize_deferred_guard_signature(
                                        proof,
                                        footwork.pending_cadence_edge,
                                        signature,
                                    )
                                });
                                !guard_exact_proof_matches_live(
                                    proof,
                                    live_signature,
                                    footwork.step_sequence,
                                    footwork.swing_left,
                                    semantic_tick,
                                    live_hip,
                                    support_owner_valid,
                                )
                            }
                        }
                        GuardSegmentReachProof::Retained(_) => false,
                    };
                if !contact_proof_invalid {
                    advance_c2_segment_tick(&mut segment.motion, advances, semantic_tick);
                }
                let sample = guard_swing_replan_sample(segment.motion);
                let live_reach = if footwork.swing_left {
                    left_planning_reach
                } else {
                    right_planning_reach
                };
                let proof_next_reachable = match segment.reach {
                    GuardSegmentReachProof::Exact(proof) => {
                        let live_hip = if proof.swing_left {
                            left_hip_current
                        } else {
                            right_hip_current
                        };
                        if contact_driven_guard && segment.end.is_contact() {
                            contact_driven_guard_sample_is_live_reachable(
                                proof,
                                sample.position,
                                live_hip,
                                segment.recovery_to_contact,
                            )
                        } else {
                            exact_guard_sample_is_live_reachable(
                                proof,
                                sample.position,
                                semantic_tick,
                                live_hip,
                                !segment.end.is_contact(),
                            )
                        }
                    }
                    GuardSegmentReachProof::Retained(_) => direct_c2_sample_is_live_reachable(
                        segment.motion,
                        sample,
                        live_reach,
                        state_delta_seconds,
                    ),
                };
                if advances && (contact_proof_invalid || !proof_next_reachable) {
                    let aborted_contact = segment.end.is_contact();
                    let (presented, presented_velocity, presented_acceleration, fresh_proof) =
                        if footwork.swing_left {
                            (
                                footwork.left_solve_target.unwrap_or(visible_left),
                                footwork.left_target_velocity,
                                footwork.left_target_acceleration,
                                left_guard_hip_proof,
                            )
                        } else {
                            (
                                footwork.right_solve_target.unwrap_or(visible_right),
                                footwork.right_target_velocity,
                                footwork.right_target_acceleration,
                                right_guard_hip_proof,
                            )
                        };
                    let replacement = if contact_driven_guard && aborted_contact {
                        plan_contact_driven_guard_segment(
                            presented,
                            presented_velocity,
                            presented_acceleration,
                            segment.end.position(),
                            contact_driven_tick_budget,
                            fresh_proof
                                .map(|mut proof| {
                                    proof.sequence = footwork.step_sequence;
                                    proof.trajectory_signature.sequence = footwork.step_sequence;
                                    proof
                                })
                                .filter(|_| support_owner_valid),
                            terrain,
                            |progress| {
                                guard_contact_candidate_at_progress(
                                    presented,
                                    segment.end.position(),
                                    progress,
                                    support_target,
                                    rig_origin,
                                    rig_rotation,
                                    footwork.swing_stance_local.x.signum(),
                                    enabled.0,
                                    terrain,
                                )
                            },
                        )
                    } else if matches!(segment.reach, GuardSegmentReachProof::Exact(_)) {
                        if aborted_contact && !skeleton.raised_locomotion().is_moving() {
                            plan_guard_contact_recovery_without_cadence_deadline(
                                presented,
                                presented_velocity,
                                presented_acceleration,
                                segment.end.position(),
                                fresh_proof.filter(|_| support_owner_valid),
                                terrain,
                            )
                            .map_or_else(
                                || {
                                    replan_invalid_exact_guard_segment(
                                        segment,
                                        presented,
                                        presented_velocity,
                                        presented_acceleration,
                                        semantic_tick,
                                        fresh_proof.filter(|_| support_owner_valid),
                                    )
                                },
                                GuardFootEndpointPlan::Segment,
                            )
                        } else {
                            replan_invalid_exact_guard_segment(
                                segment,
                                presented,
                                presented_velocity,
                                presented_acceleration,
                                semantic_tick,
                                fresh_proof.filter(|_| support_owner_valid),
                            )
                        }
                    } else {
                        GuardFootEndpointPlan::MustReleaseOrReplan(
                            GuardFootReleasePlan::EmergencyBrake { presented },
                        )
                    };
                    footwork.swing_replan_segment = None;
                    if footwork.swing_left {
                        footwork.left_contact_abort_event =
                            aborted_contact.then_some(segment.owner_epoch);
                        footwork.left_motion_owner_epoch = semantic_tick;
                        left_target = presented;
                        left_explicit_ideal_motion =
                            Some((presented_velocity, presented_acceleration));
                        left_direct_c2_sample = true;
                    } else {
                        footwork.right_contact_abort_event =
                            aborted_contact.then_some(segment.owner_epoch);
                        footwork.right_motion_owner_epoch = semantic_tick;
                        right_target = presented;
                        right_explicit_ideal_motion =
                            Some((presented_velocity, presented_acceleration));
                        right_direct_c2_sample = true;
                    }
                    match replacement {
                        GuardFootEndpointPlan::Segment(replanned) => {
                            let replanned = replanned.with_owner_epoch(semantic_tick);
                            footwork.swing_replan_segment = Some(replanned);
                            footwork.swing_emergency_brake = None;
                            footwork.swing_release_owner_active = false;
                            footwork.awaiting_step_sequence = false;
                        }
                        GuardFootEndpointPlan::MustReleaseOrReplan(
                            GuardFootReleasePlan::Segment(release),
                        ) => {
                            if contact_driven_guard {
                                footwork.swing_replan_segment = None;
                                footwork.swing_emergency_brake = None;
                                footwork.swing_release_owner_active = false;
                                footwork.awaiting_step_sequence = false;
                            } else {
                                footwork.swing_replan_segment =
                                    Some(release.with_owner_epoch(semantic_tick));
                                footwork.swing_emergency_brake = None;
                                footwork.swing_release_owner_active = true;
                                footwork.awaiting_step_sequence = true;
                            }
                        }
                        GuardFootEndpointPlan::MustReleaseOrReplan(
                            GuardFootReleasePlan::EmergencyBrake { .. },
                        ) => {
                            if contact_driven_guard {
                                footwork.swing_emergency_brake = None;
                                footwork.swing_release_owner_active = false;
                                footwork.awaiting_step_sequence = false;
                            } else {
                                footwork.swing_emergency_brake = Some(EmergencyFootBrake {
                                    stationary_ideal: presented,
                                    owner_local_ideal: Some(
                                        rig_rotation.inverse()
                                            * ((if footwork.swing_left {
                                                left_authored
                                            } else {
                                                right_authored
                                            }) - rig_origin),
                                    ),
                                });
                                footwork.swing_release_owner_active = true;
                                footwork.awaiting_step_sequence = true;
                            }
                            if footwork.swing_left {
                                left_direct_c2_sample = false;
                            } else {
                                right_direct_c2_sample = false;
                            }
                        }
                    }
                } else {
                    let replanned = sample.position;
                    if footwork.swing_left {
                        left_target = replanned;
                        left_explicit_ideal_motion = Some((sample.velocity, sample.acceleration));
                        left_direct_c2_sample = true;
                    } else {
                        right_target = replanned;
                        right_explicit_ideal_motion = Some((sample.velocity, sample.acceleration));
                        right_direct_c2_sample = true;
                    }
                    footwork.swing_replan_segment = Some(segment);
                    let reached_terminal_tick = segment
                        .owner_epoch
                        .saturating_add(segment.timing.total_ticks.get() as u64)
                        == semantic_tick;
                    let authoritative_edge_reached_held_contact = segment.end.is_contact()
                        && segment.timing.is_complete()
                        && sequence_delta == 1;
                    if segment.timing.is_complete()
                        && (reached_terminal_tick || authoritative_edge_reached_held_contact)
                    {
                        if contact_driven_guard {
                            complete_contact_driven_guard_segment(&mut footwork, segment);
                        } else {
                            let support_recovery_coexists =
                                footwork.left_support_release_owner.is_some()
                                    || footwork.right_support_release_owner.is_some();
                            complete_guard_segment_semantics(
                                &mut footwork,
                                segment,
                                swing_left,
                                skeleton.raised_locomotion().step_sequence(),
                                cadence_can_advance,
                            );
                            if support_recovery_coexists {
                                footwork.pending_cadence_edge = Some((
                                    swing_left,
                                    skeleton.raised_locomotion().step_sequence(),
                                ));
                            }
                        }
                    }
                }
            }

            if stationary_guard {
                // Stationary plant/pivot ownership replaces any in-progress
                // swing segment below. Its target must not inherit motion
                // derivatives from the superseded generator.
                left_explicit_ideal_motion = None;
                right_explicit_ideal_motion = None;
                left_direct_c2_sample = false;
                right_direct_c2_sample = false;
                if footwork.was_moving {
                    // A replicated stop can occur mid-swing. Adopt the last
                    // rendered targets before stationary pivot ownership
                    // begins instead of restoring older plant coordinates.
                    footwork.left_plant = visible_left;
                    footwork.right_plant = visible_right;
                    footwork.pivot_active = false;
                    footwork.pivot_progress = 0.0;
                }
                // Rotation has no controller velocity and therefore cannot
                // advance the replicated guard cadence. Keep the stance
                // plausible in presentation by correcting one world plant at
                // a time once the rotated authored stance is far enough away.
                // The endpoint is latched so continued camera motion cannot
                // make the foot chase a target that never lands.
                if guard_pelvis_blocks_stationary_pivot(footwork.pelvis_acquisition) {
                    footwork.pivot_active = false;
                    footwork.pivot_progress = 0.0;
                } else if advances && !quickstep_handoff_active && !quickstep_stance_held {
                    if footwork.pivot_active {
                        footwork.pivot_progress = (footwork.pivot_progress
                            + state_delta_seconds.max(0.0) / GUARD_PIVOT_STEP_SECONDS)
                            .min(1.0);
                    }
                    if !footwork.pivot_active {
                        let left_stationary_target = stationary_guard_comfort_endpoint(
                            left_authored,
                            left_planning_reach,
                            terrain,
                        );
                        let right_stationary_target = stationary_guard_comfort_endpoint(
                            right_authored,
                            right_planning_reach,
                            terrain,
                        );
                        let left_error = left_stationary_target.distance(footwork.left_plant);
                        let right_error = right_stationary_target.distance(footwork.right_plant);
                        let left_has_contact = terrain
                            .and_then(|terrain| terrain.height_at(footwork.left_plant.xz()))
                            .is_some_and(|height| {
                                sole_is_at_contact(footwork.left_plant.y, height)
                            });
                        let right_has_contact = terrain
                            .and_then(|terrain| terrain.height_at(footwork.right_plant.xz()))
                            .is_some_and(|height| {
                                sole_is_at_contact(footwork.right_plant.y, height)
                            });
                        if !left_has_contact
                            || !right_has_contact
                            || left_error.max(right_error) > GUARD_PIVOT_TRIGGER_METRES
                        {
                            footwork.pivot_active = true;
                            let left_separation =
                                (left_authored - footwork.right_plant).xz().length();
                            let right_separation =
                                (right_authored - footwork.left_plant).xz().length();
                            footwork.pivot_left = stationary_guard_pivot_side(
                                left_error,
                                right_error,
                                left_has_contact,
                                right_has_contact,
                                left_separation,
                                right_separation,
                            );
                            footwork.pivot_progress = 0.0;
                            footwork.pivot_origin = rig_origin;
                            footwork.pivot_start = if footwork.pivot_left {
                                footwork.left_plant
                            } else {
                                footwork.right_plant
                            };
                            let authored_end = if footwork.pivot_left {
                                left_stationary_target
                            } else {
                                right_stationary_target
                            };
                            let authored_local =
                                rig_rotation.inverse() * (authored_end - rig_origin);
                            let side = if authored_local.x.abs() > 0.001 {
                                authored_local.x.signum()
                            } else if footwork.pivot_left {
                                -1.0
                            } else {
                                1.0
                            };
                            let constrained_end = constrain_guard_swing_to_live_corridor(
                                authored_end,
                                if footwork.pivot_left {
                                    footwork.right_plant
                                } else {
                                    footwork.left_plant
                                },
                                rig_origin,
                                rig_rotation,
                                side,
                            );
                            // Corridor projection can change X/Z after the
                            // authored pose supplied its airborne Y. Query the
                            // final projected point and latch a true sole
                            // contact. Without terrain evidence, retain the
                            // current plant instead of inventing a contact.
                            let terrain_height = enabled
                                .0
                                .then(|| {
                                    terrain
                                        .and_then(|terrain| terrain.height_at(constrained_end.xz()))
                                })
                                .flatten();
                            if let Some(endpoint) =
                                stationary_guard_pivot_endpoint(constrained_end, terrain_height)
                            {
                                footwork.pivot_end = endpoint;
                            } else {
                                // No terrain evidence means there is no
                                // admissible planted endpoint. Cancel this
                                // presentation pivot instead of retaining an
                                // owner that suppresses one nominal support.
                                footwork.pivot_end = footwork.pivot_start;
                                footwork.pivot_active = false;
                            }
                        }
                    }
                }
                left_target = footwork.left_plant;
                right_target = footwork.right_plant;
                if footwork.pivot_active {
                    let progress = footwork.pivot_progress;
                    let pivot_start_has_contact = terrain
                        .and_then(|terrain| terrain.height_at(footwork.pivot_start.xz()))
                        .is_some_and(|height| sole_is_at_contact(footwork.pivot_start.y, height));
                    let pivot_sample = guard_boundary_quintic_sample(
                        footwork.pivot_start,
                        Vec3::ZERO,
                        Vec3::ZERO,
                        footwork.pivot_end,
                        progress,
                        GUARD_PIVOT_STEP_SECONDS,
                        if pivot_start_has_contact {
                            GUARD_PIVOT_LIFT_METRES
                        } else {
                            0.0
                        },
                    );
                    let pivot_motion_is_admitted = !advances
                        || guard_motion_can_transfer_to_cadence(
                            pivot_sample.position,
                            pivot_sample.velocity,
                            pivot_sample.acceleration,
                            if footwork.pivot_left {
                                left_planning_reach
                            } else {
                                right_planning_reach
                            },
                            state_delta_seconds,
                            ReachMotionSample::Next,
                        );
                    if pivot_motion_is_admitted {
                        if footwork.pivot_left {
                            left_target = pivot_sample.position;
                            left_explicit_ideal_motion =
                                Some((pivot_sample.velocity, pivot_sample.acceleration));
                            left_direct_c2_sample = true;
                        } else {
                            right_target = pivot_sample.position;
                            right_explicit_ideal_motion =
                                Some((pivot_sample.velocity, pivot_sample.acceleration));
                            right_direct_c2_sample = true;
                        }
                        let pivot_presented = if footwork.pivot_left {
                            visible_left
                        } else {
                            visible_right
                        };
                        let pivot_terrain_height =
                            terrain.and_then(|terrain| terrain.height_at(pivot_presented.xz()));
                        if advances
                            && footwork.pivot_progress >= 1.0
                            && stationary_guard_pivot_has_landed(
                                pivot_presented,
                                footwork.pivot_end,
                                pivot_terrain_height,
                            )
                        {
                            if footwork.pivot_left {
                                footwork.left_plant = footwork.pivot_end;
                            } else {
                                footwork.right_plant = footwork.pivot_end;
                            }
                            footwork.pivot_active = false;
                        }
                    } else {
                        let presented = if footwork.pivot_left {
                            footwork.left_solve_target.unwrap_or(visible_left)
                        } else {
                            footwork.right_solve_target.unwrap_or(visible_right)
                        };
                        // A stationary presentation pivot has no replicated
                        // cadence edge to await. If its preview cannot remain
                        // reachable, cancel the pivot and retain the truthful
                        // planted stance. Installing an already-settled brake
                        // here used to clear the owner in the same tick while
                        // leaving `awaiting_step_sequence` latched forever.
                        if cancel_rejected_stationary_pivot(&mut footwork, presented) {
                            left_target = presented;
                        } else {
                            right_target = presented;
                        }
                    }
                }
            } else if footwork.pivot_active {
                // Movement supersedes a presentation-only pivot. Preserve the
                // last visible target as the new plant before normal cadence
                // resumes instead of snapping back to the pivot origin.
                if footwork.pivot_left {
                    footwork.left_plant =
                        footwork.left_solve_target.unwrap_or(footwork.pivot_start);
                } else {
                    footwork.right_plant =
                        footwork.right_solve_target.unwrap_or(footwork.pivot_start);
                }
                footwork.pivot_active = false;
            }
            if advances
                && footwork.swing_replan_segment.is_none()
                && footwork.swing_emergency_brake.is_some()
            {
                let (presented, velocity, acceleration, body_relative_target, reach) =
                    if footwork.swing_left {
                        (
                            footwork.left_solve_target.unwrap_or(visible_left),
                            footwork.left_target_velocity,
                            footwork.left_target_acceleration,
                            left_authored,
                            left_planning_reach,
                        )
                    } else {
                        (
                            footwork.right_solve_target.unwrap_or(visible_right),
                            footwork.right_target_velocity,
                            footwork.right_target_acceleration,
                            right_authored,
                            right_planning_reach,
                        )
                    };
                let deadline_seconds = (1.0 - step_progress.clamp(0.0, 1.0)) * 0.32;
                if non_support_guard_target_requires_release(
                    presented,
                    velocity,
                    acceleration,
                    body_relative_target,
                    reach,
                    deadline_seconds,
                    state_delta_seconds,
                ) && let Some(timing) = cadence_contact_timing
                    && let GuardFootEndpointPlan::MustReleaseOrReplan(
                        GuardFootReleasePlan::Segment(segment),
                    ) = plan_exact_guard_release(
                        presented,
                        velocity,
                        acceleration,
                        timing,
                        if footwork.swing_left {
                            left_guard_hip_proof
                        } else {
                            right_guard_hip_proof
                        }
                        .expect("an exact guard release requires the matching hip proof"),
                    )
                {
                    footwork.swing_replan_segment = Some(segment.with_owner_epoch(semantic_tick));
                    footwork.swing_emergency_brake = None;
                    footwork.swing_release_owner_active = true;
                    footwork.awaiting_step_sequence = true;
                    if footwork.swing_left {
                        footwork.left_desired_target = Some(segment.start);
                        footwork.left_ideal_velocity = segment.start_velocity;
                        footwork.left_ideal_acceleration = segment.start_acceleration;
                        footwork.left_ideal_history_valid = true;
                    } else {
                        footwork.right_desired_target = Some(segment.start);
                        footwork.right_ideal_velocity = segment.start_velocity;
                        footwork.right_ideal_acceleration = segment.start_acceleration;
                        footwork.right_ideal_history_valid = true;
                    }
                } else if non_support_guard_target_requires_release(
                    presented,
                    velocity,
                    acceleration,
                    body_relative_target,
                    reach,
                    deadline_seconds,
                    state_delta_seconds,
                ) {
                    // The current cadence deadline cannot admit a bounded
                    // release. Retain the already-persistent emergency owner
                    // and explicitly defer contact rather than extending a
                    // promised contact or snapping to it late.
                    footwork.swing_release_owner_active = true;
                    footwork.awaiting_step_sequence = true;
                }
            }
            if let Some(brake) = footwork.swing_emergency_brake {
                let recovery_target = brake.target(rig_origin, rig_rotation);
                if footwork.swing_left {
                    left_target = recovery_target;
                    left_explicit_ideal_motion = brake
                        .owner_local_ideal
                        .is_none()
                        .then_some((Vec3::ZERO, Vec3::ZERO));
                } else {
                    right_target = recovery_target;
                    right_explicit_ideal_motion = brake
                        .owner_local_ideal
                        .is_none()
                        .then_some((Vec3::ZERO, Vec3::ZERO));
                }
            }
            // A support foot that enters the warning reach must immediately
            // leave its world plant. The ordinary authored guard foot is a
            // body-relative recovery goal; the stateful follower below makes
            // the release bounded while cadence waits to acquire a new plant.
            if footwork.left_support_release_owner.is_some() {
                match footwork.left_support_release_owner {
                    Some(SupportReleaseOwner::Segment(mut segment)) => {
                        advance_c2_segment_tick(&mut segment, advances, semantic_tick);
                        let sample = guard_swing_replan_sample(segment);
                        if advances
                            && !direct_c2_sample_is_live_reachable(
                                segment,
                                sample,
                                left_planning_reach,
                                state_delta_seconds,
                            )
                        {
                            let presented = footwork.left_solve_target.unwrap_or(visible_left);
                            left_target = presented;
                            left_explicit_ideal_motion = Some((Vec3::ZERO, Vec3::ZERO));
                            supersede_support_segment_with_emergency(
                                &mut footwork,
                                true,
                                presented,
                                Some(rig_rotation.inverse() * (left_authored - rig_origin)),
                            );
                        } else {
                            left_target = sample.position;
                            left_explicit_ideal_motion =
                                Some((sample.velocity, sample.acceleration));
                            left_direct_c2_sample = true;
                            footwork.left_support_release_owner = if segment.timing.is_complete() {
                                Some(SupportReleaseOwner::TerminalHold {
                                    endpoint: segment.end.position(),
                                })
                            } else {
                                Some(SupportReleaseOwner::Segment(segment))
                            };
                        }
                    }
                    Some(SupportReleaseOwner::EmergencyBrake(brake)) => {
                        left_target = brake.target(rig_origin, rig_rotation);
                        left_explicit_ideal_motion = brake
                            .owner_local_ideal
                            .is_none()
                            .then_some((Vec3::ZERO, Vec3::ZERO));
                    }
                    Some(SupportReleaseOwner::TerminalHold { endpoint }) => {
                        left_target = endpoint;
                        left_explicit_ideal_motion = Some((Vec3::ZERO, Vec3::ZERO));
                    }
                    Some(SupportReleaseOwner::GroundSafetySlide { owner_local }) => {
                        let desired = ground_safety_slide_target(
                            owner_local,
                            rig_origin,
                            rig_rotation,
                            terrain,
                        );
                        let endpoint = left_planning_reach.map_or(desired, |reach| {
                            ground_safety_slide_endpoint(
                                desired,
                                reach.current_root(),
                                reach.warning_reach() * 0.97,
                                terrain,
                            )
                        });
                        left_target = endpoint;
                        left_explicit_ideal_motion = None;
                    }
                    None => {}
                }
            }
            if footwork.right_support_release_owner.is_some() {
                match footwork.right_support_release_owner {
                    Some(SupportReleaseOwner::Segment(mut segment)) => {
                        advance_c2_segment_tick(&mut segment, advances, semantic_tick);
                        let sample = guard_swing_replan_sample(segment);
                        if advances
                            && !direct_c2_sample_is_live_reachable(
                                segment,
                                sample,
                                right_planning_reach,
                                state_delta_seconds,
                            )
                        {
                            let presented = footwork.right_solve_target.unwrap_or(visible_right);
                            right_target = presented;
                            right_explicit_ideal_motion = Some((Vec3::ZERO, Vec3::ZERO));
                            supersede_support_segment_with_emergency(
                                &mut footwork,
                                false,
                                presented,
                                Some(rig_rotation.inverse() * (right_authored - rig_origin)),
                            );
                        } else {
                            right_target = sample.position;
                            right_explicit_ideal_motion =
                                Some((sample.velocity, sample.acceleration));
                            right_direct_c2_sample = true;
                            footwork.right_support_release_owner = if segment.timing.is_complete() {
                                Some(SupportReleaseOwner::TerminalHold {
                                    endpoint: segment.end.position(),
                                })
                            } else {
                                Some(SupportReleaseOwner::Segment(segment))
                            };
                        }
                    }
                    Some(SupportReleaseOwner::EmergencyBrake(brake)) => {
                        right_target = brake.target(rig_origin, rig_rotation);
                        right_explicit_ideal_motion = brake
                            .owner_local_ideal
                            .is_none()
                            .then_some((Vec3::ZERO, Vec3::ZERO));
                    }
                    Some(SupportReleaseOwner::TerminalHold { endpoint }) => {
                        right_target = endpoint;
                        right_explicit_ideal_motion = Some((Vec3::ZERO, Vec3::ZERO));
                    }
                    Some(SupportReleaseOwner::GroundSafetySlide { owner_local }) => {
                        let desired = ground_safety_slide_target(
                            owner_local,
                            rig_origin,
                            rig_rotation,
                            terrain,
                        );
                        let endpoint = right_planning_reach.map_or(desired, |reach| {
                            ground_safety_slide_endpoint(
                                desired,
                                reach.current_root(),
                                reach.warning_reach() * 0.97,
                                terrain,
                            )
                        });
                        right_target = endpoint;
                        right_explicit_ideal_motion = None;
                    }
                    None => {}
                }
            }
            if footwork.release_handoff_active {
                if advances {
                    footwork.release_handoff_progress =
                        (footwork.release_handoff_progress + state_delta_seconds * 4.0).min(1.0);
                }
                let blend = quintic_progress(footwork.release_handoff_progress);
                left_target = footwork.release_left_start.lerp(left_authored, blend);
                right_target = footwork.release_right_start.lerp(right_authored, blend);
                // The authored endpoints may themselves move with the release
                // clip, so the segment's analytic derivatives no longer
                // describe these targets. Let the wrapper resample this owner.
                left_explicit_ideal_motion = None;
                right_explicit_ideal_motion = None;
                left_direct_c2_sample = false;
                right_direct_c2_sample = false;
            }
            if footwork.release_handoff_active
                && raised_release_uses_transition_authored_target(skeleton)
            {
                // The posture animation is already a bounded, authored motion.
                // Following its moving feet with the ordinary guard tracker
                // leaves a residual target behind when the body commits to
                // prone. Present the release blend directly so ownership can
                // converge and retire before that discrete body-state seam.
                if advances && state_delta_seconds > f32::EPSILON {
                    let previous_left_velocity = footwork.left_target_velocity;
                    let previous_right_velocity = footwork.right_target_velocity;
                    let left_velocity = footwork.left_solve_target.map_or(Vec3::ZERO, |previous| {
                        (left_target - previous) / state_delta_seconds
                    });
                    let right_velocity =
                        footwork.right_solve_target.map_or(Vec3::ZERO, |previous| {
                            (right_target - previous) / state_delta_seconds
                        });
                    footwork.left_target_velocity = left_velocity;
                    footwork.right_target_velocity = right_velocity;
                    footwork.left_target_acceleration =
                        (left_velocity - previous_left_velocity) / state_delta_seconds;
                    footwork.right_target_acceleration =
                        (right_velocity - previous_right_velocity) / state_delta_seconds;
                }
                if advances {
                    footwork.left_desired_target = Some(left_target);
                    footwork.right_desired_target = Some(right_target);
                }
            } else {
                let left_ideal = left_target;
                let left_reach = foot_follow_reach_envelope(
                    left_upper,
                    left_lower,
                    left_foot,
                    skeleton.world_velocity * state_delta_seconds,
                    &parents,
                    &transforms.p0(),
                );
                let left_track = if left_direct_c2_sample {
                    let (velocity, acceleration) = left_explicit_ideal_motion
                        .expect("a direct C2 sample carries analytic derivatives");
                    direct_c2_guard_target_sample(left_ideal, velocity, acceleration)
                } else {
                    advance_guard_foot_target_sample_with_reach(
                        footwork.left_solve_target,
                        footwork.left_target_velocity,
                        footwork.left_target_acceleration,
                        footwork.left_desired_target,
                        footwork.left_ideal_velocity,
                        footwork.left_ideal_acceleration,
                        footwork.left_ideal_history_valid,
                        left_ideal,
                        left_explicit_ideal_motion,
                        state_delta_seconds,
                        advances,
                        left_reach,
                    )
                };
                left_target = left_track.position;
                footwork.left_target_velocity = left_track.velocity;
                footwork.left_target_acceleration = left_track.acceleration;
                footwork.left_ideal_velocity = left_track.ideal_velocity;
                footwork.left_ideal_acceleration = left_track.ideal_acceleration;
                footwork.left_ideal_history_valid = left_track.ideal_history_valid;
                memory.left_foot_follower = left_track
                    .ideal_history_valid
                    .then(|| {
                        FootFollowerState::from_presented_pose(
                            left_target,
                            left_track.velocity,
                            left_track.acceleration,
                            left_ideal,
                            left_track.ideal_velocity,
                            left_track.ideal_acceleration,
                        )
                    })
                    .flatten();
                if advances {
                    footwork.left_desired_target = Some(left_ideal);
                    if memory.quickstep_handoff_pending {
                        memory.quickstep_left_landing_local =
                            Some(rig_rotation.inverse() * (left_target - rig_origin));
                    }
                }
                if !contact_driven_guard
                    && let Some((reason, _suggested_waypoint)) = left_track.replan
                {
                    if footwork.swing_left {
                        if footwork.swing_replan_segment.is_none()
                            || reason == FootFollowReason::ReachHardLimit
                        {
                            if reason == FootFollowReason::ReachHardLimit {
                                footwork.swing_replan_segment = None;
                            }
                            let plan = cadence_contact_timing.map_or_else(
                                || {
                                    GuardFootEndpointPlan::MustReleaseOrReplan(
                                        GuardFootReleasePlan::EmergencyBrake {
                                            presented: left_target,
                                        },
                                    )
                                },
                                |timing| {
                                    plan_guard_c2_contact_segment(
                                        left_target,
                                        left_track.velocity,
                                        left_track.acceleration,
                                        timing,
                                        left_guard_hip_proof,
                                        |progress| {
                                            guard_contact_candidate_at_progress(
                                                left_target,
                                                terrain_swing_end,
                                                progress,
                                                support_target,
                                                rig_origin,
                                                rig_rotation,
                                                footwork.swing_stance_local.x.signum(),
                                                enabled.0,
                                                terrain,
                                            )
                                        },
                                    )
                                },
                            );
                            match plan {
                                GuardFootEndpointPlan::Segment(segment) => {
                                    footwork.swing_replan_segment =
                                        Some(segment.with_owner_epoch(semantic_tick));
                                    footwork.swing_emergency_brake = None;
                                    footwork.left_desired_target = Some(segment.start);
                                    footwork.left_ideal_velocity = segment.start_velocity;
                                    footwork.left_ideal_acceleration = segment.start_acceleration;
                                    footwork.left_ideal_history_valid = true;
                                }
                                GuardFootEndpointPlan::MustReleaseOrReplan(release_plan) => {
                                    let release_segment = match release_plan {
                                        GuardFootReleasePlan::Segment(segment) => {
                                            Some(segment.with_owner_epoch(semantic_tick))
                                        }
                                        GuardFootReleasePlan::EmergencyBrake { .. } => None,
                                    };
                                    footwork.swing_replan_segment = release_segment;
                                    footwork.swing_emergency_brake = match release_plan {
                                        GuardFootReleasePlan::Segment(_) => None,
                                        GuardFootReleasePlan::EmergencyBrake { presented } => {
                                            Some(EmergencyFootBrake {
                                                stationary_ideal: presented,
                                                owner_local_ideal: None,
                                            })
                                        }
                                    };
                                    footwork.swing_release_owner_active = true;
                                    footwork.awaiting_step_sequence = true;
                                    if let Some(segment) = release_segment {
                                        footwork.left_desired_target = Some(segment.start);
                                        footwork.left_ideal_velocity = segment.start_velocity;
                                        footwork.left_ideal_acceleration =
                                            segment.start_acceleration;
                                        footwork.left_ideal_history_valid = true;
                                    } else {
                                        let GuardFootReleasePlan::EmergencyBrake { presented } =
                                            release_plan
                                        else {
                                            unreachable!();
                                        };
                                        footwork.swing_start = presented;
                                        footwork.left_desired_target = Some(presented);
                                        footwork.left_ideal_velocity = Vec3::ZERO;
                                        footwork.left_ideal_acceleration = Vec3::ZERO;
                                        footwork.left_ideal_history_valid = true;
                                    }
                                }
                            }
                        }
                    } else if matches!(
                        reason,
                        FootFollowReason::ReachWarning | FootFollowReason::ReachHardLimit
                    ) {
                        // The left foot is support. Do not skate its latched
                        // plant to satisfy reach; retire support and wait for
                        // the next authoritative cadence sample to acquire a
                        // new plant.
                        if footwork.left_support_release_owner.is_none() {
                            install_guard_support_release(
                                &mut footwork,
                                true,
                                left_target,
                                left_track.velocity,
                                left_track.acceleration,
                                left_hip_trajectory,
                            );
                        } else if reason == FootFollowReason::ReachHardLimit {
                            supersede_support_segment_with_emergency(
                                &mut footwork,
                                true,
                                left_target,
                                Some(rig_rotation.inverse() * (left_authored - rig_origin)),
                            );
                        }
                        footwork.awaiting_step_sequence = true;
                    }
                }
                let right_ideal = right_target;
                let right_reach = foot_follow_reach_envelope(
                    right_upper,
                    right_lower,
                    right_foot,
                    skeleton.world_velocity * state_delta_seconds,
                    &parents,
                    &transforms.p0(),
                );
                let right_track = if right_direct_c2_sample {
                    let (velocity, acceleration) = right_explicit_ideal_motion
                        .expect("a direct C2 sample carries analytic derivatives");
                    direct_c2_guard_target_sample(right_ideal, velocity, acceleration)
                } else {
                    advance_guard_foot_target_sample_with_reach(
                        footwork.right_solve_target,
                        footwork.right_target_velocity,
                        footwork.right_target_acceleration,
                        footwork.right_desired_target,
                        footwork.right_ideal_velocity,
                        footwork.right_ideal_acceleration,
                        footwork.right_ideal_history_valid,
                        right_ideal,
                        right_explicit_ideal_motion,
                        state_delta_seconds,
                        advances,
                        right_reach,
                    )
                };
                right_target = right_track.position;
                footwork.right_target_velocity = right_track.velocity;
                footwork.right_target_acceleration = right_track.acceleration;
                footwork.right_ideal_velocity = right_track.ideal_velocity;
                footwork.right_ideal_acceleration = right_track.ideal_acceleration;
                footwork.right_ideal_history_valid = right_track.ideal_history_valid;
                memory.right_foot_follower = right_track
                    .ideal_history_valid
                    .then(|| {
                        FootFollowerState::from_presented_pose(
                            right_target,
                            right_track.velocity,
                            right_track.acceleration,
                            right_ideal,
                            right_track.ideal_velocity,
                            right_track.ideal_acceleration,
                        )
                    })
                    .flatten();
                if advances {
                    footwork.right_desired_target = Some(right_ideal);
                    if memory.quickstep_handoff_pending {
                        memory.quickstep_right_landing_local =
                            Some(rig_rotation.inverse() * (right_target - rig_origin));
                    }
                }
                if !contact_driven_guard
                    && let Some((reason, _suggested_waypoint)) = right_track.replan
                {
                    if !footwork.swing_left {
                        if footwork.swing_replan_segment.is_none()
                            || reason == FootFollowReason::ReachHardLimit
                        {
                            if reason == FootFollowReason::ReachHardLimit {
                                footwork.swing_replan_segment = None;
                            }
                            let plan = cadence_contact_timing.map_or_else(
                                || {
                                    GuardFootEndpointPlan::MustReleaseOrReplan(
                                        GuardFootReleasePlan::EmergencyBrake {
                                            presented: right_target,
                                        },
                                    )
                                },
                                |timing| {
                                    plan_guard_c2_contact_segment(
                                        right_target,
                                        right_track.velocity,
                                        right_track.acceleration,
                                        timing,
                                        right_guard_hip_proof,
                                        |progress| {
                                            guard_contact_candidate_at_progress(
                                                right_target,
                                                terrain_swing_end,
                                                progress,
                                                support_target,
                                                rig_origin,
                                                rig_rotation,
                                                footwork.swing_stance_local.x.signum(),
                                                enabled.0,
                                                terrain,
                                            )
                                        },
                                    )
                                },
                            );
                            match plan {
                                GuardFootEndpointPlan::Segment(segment) => {
                                    footwork.swing_replan_segment =
                                        Some(segment.with_owner_epoch(semantic_tick));
                                    footwork.swing_emergency_brake = None;
                                    footwork.right_desired_target = Some(segment.start);
                                    footwork.right_ideal_velocity = segment.start_velocity;
                                    footwork.right_ideal_acceleration = segment.start_acceleration;
                                    footwork.right_ideal_history_valid = true;
                                }
                                GuardFootEndpointPlan::MustReleaseOrReplan(release_plan) => {
                                    let release_segment = match release_plan {
                                        GuardFootReleasePlan::Segment(segment) => {
                                            Some(segment.with_owner_epoch(semantic_tick))
                                        }
                                        GuardFootReleasePlan::EmergencyBrake { .. } => None,
                                    };
                                    footwork.swing_replan_segment = release_segment;
                                    footwork.swing_emergency_brake = match release_plan {
                                        GuardFootReleasePlan::Segment(_) => None,
                                        GuardFootReleasePlan::EmergencyBrake { presented } => {
                                            Some(EmergencyFootBrake {
                                                stationary_ideal: presented,
                                                owner_local_ideal: None,
                                            })
                                        }
                                    };
                                    footwork.swing_release_owner_active = true;
                                    footwork.awaiting_step_sequence = true;
                                    if let Some(segment) = release_segment {
                                        footwork.right_desired_target = Some(segment.start);
                                        footwork.right_ideal_velocity = segment.start_velocity;
                                        footwork.right_ideal_acceleration =
                                            segment.start_acceleration;
                                        footwork.right_ideal_history_valid = true;
                                    } else {
                                        let GuardFootReleasePlan::EmergencyBrake { presented } =
                                            release_plan
                                        else {
                                            unreachable!();
                                        };
                                        footwork.swing_start = presented;
                                        footwork.right_desired_target = Some(presented);
                                        footwork.right_ideal_velocity = Vec3::ZERO;
                                        footwork.right_ideal_acceleration = Vec3::ZERO;
                                        footwork.right_ideal_history_valid = true;
                                    }
                                }
                            }
                        }
                    } else if matches!(
                        reason,
                        FootFollowReason::ReachWarning | FootFollowReason::ReachHardLimit
                    ) {
                        // Symmetric support-foot release: keep the plant
                        // semantic fixed and let cadence reacquire it.
                        if footwork.right_support_release_owner.is_none() {
                            install_guard_support_release(
                                &mut footwork,
                                false,
                                right_target,
                                right_track.velocity,
                                right_track.acceleration,
                                right_hip_trajectory,
                            );
                        } else if reason == FootFollowReason::ReachHardLimit {
                            supersede_support_segment_with_emergency(
                                &mut footwork,
                                false,
                                right_target,
                                Some(rig_rotation.inverse() * (right_authored - rig_origin)),
                            );
                        }
                        footwork.awaiting_step_sequence = true;
                    }
                }
            }
            if advances {
                let left_motion = (
                    footwork.left_target_velocity,
                    footwork.left_target_acceleration,
                );
                let right_motion = (
                    footwork.right_target_velocity,
                    footwork.right_target_acceleration,
                );
                complete_support_release_if_settled(
                    &mut footwork,
                    true,
                    left_target,
                    left_motion.0,
                    left_motion.1,
                    swing_left,
                    skeleton.raised_locomotion().step_sequence(),
                    cadence_can_advance,
                );
                complete_support_release_if_settled(
                    &mut footwork,
                    false,
                    right_target,
                    right_motion.0,
                    right_motion.1,
                    swing_left,
                    skeleton.raised_locomotion().step_sequence(),
                    cadence_can_advance,
                );
            }
            if advances && footwork.swing_emergency_brake.is_some() {
                let brake = footwork
                    .swing_emergency_brake
                    .expect("the emergency settlement predicate requires an owner");
                let (target, velocity, acceleration) = if footwork.swing_left {
                    (
                        left_target,
                        footwork.left_target_velocity,
                        footwork.left_target_acceleration,
                    )
                } else {
                    (
                        right_target,
                        footwork.right_target_velocity,
                        footwork.right_target_acceleration,
                    )
                };
                let body_relative_recovery_settled = brake.owner_local_ideal.is_some()
                    && (if footwork.swing_left {
                        visible_left
                    } else {
                        visible_right
                    })
                    .distance(brake.target(rig_origin, rig_rotation))
                        <= 0.025
                    && (velocity - skeleton.world_velocity).length() <= 0.05;
                if guard_emergency_brake_has_settled(
                    brake,
                    body_relative_recovery_settled,
                    velocity,
                    acceleration,
                ) {
                    footwork.swing_emergency_brake = None;
                    footwork.swing_release_owner_active = false;
                    footwork.swing_start = target;
                    // A moving owner-local recovery is not a planted contact
                    // and therefore does not wait for a future cadence edge.
                    // Re-enter planning immediately from its visible p/v/a.
                    retain_or_defer_guard_cadence_identity(
                        &mut footwork,
                        swing_left,
                        skeleton.raised_locomotion().step_sequence(),
                        guard_emergency_settlement_awaits_cadence(
                            cadence_can_advance,
                            body_relative_recovery_settled,
                        ),
                    );
                    if footwork.swing_left {
                        footwork.left_plant = target;
                        footwork.left_solve_target = Some(target);
                        footwork.left_desired_target = Some(target);
                        if !body_relative_recovery_settled {
                            footwork.left_target_velocity = Vec3::ZERO;
                            footwork.left_target_acceleration = Vec3::ZERO;
                            footwork.left_ideal_velocity = Vec3::ZERO;
                            footwork.left_ideal_acceleration = Vec3::ZERO;
                        }
                        footwork.left_ideal_history_valid = true;
                    } else {
                        footwork.right_plant = target;
                        footwork.right_solve_target = Some(target);
                        footwork.right_desired_target = Some(target);
                        if !body_relative_recovery_settled {
                            footwork.right_target_velocity = Vec3::ZERO;
                            footwork.right_target_acceleration = Vec3::ZERO;
                            footwork.right_ideal_velocity = Vec3::ZERO;
                            footwork.right_ideal_acceleration = Vec3::ZERO;
                        }
                        footwork.right_ideal_history_valid = true;
                    }
                }
            }
            // Awaiting is itself not a motion owner. Every path that publishes
            // it must retain a typed segment/brake/release owner (or pending
            // cadence edge) until consumption. This catches same-tick
            // emergency settlement during both stationary and initial moving
            // cadence, where a bare wait would otherwise freeze both targets.
            clear_ownerless_guard_wait(&mut footwork);
            let release_handoff_complete = raised_release_handoff_is_complete(footwork);
            footwork.was_moving = !stationary_guard;
            let quickstep_handoff_converged =
                memory.quickstep_left_landing_local.is_some_and(|local| {
                    let authored_local = rig_rotation.inverse() * (left_authored - rig_origin);
                    local.distance(authored_local) <= 0.001
                }) && memory.quickstep_right_landing_local.is_some_and(|local| {
                    let authored_local = rig_rotation.inverse() * (right_authored - rig_origin);
                    local.distance(authored_local) <= 0.001
                });
            if memory.quickstep_handoff_pending && live_speed <= 0.05 && quickstep_handoff_converged
            {
                memory.quickstep_handoff_pending = false;
                memory.quickstep_guard_stance_held = true;
            }

            let mut airborne_orientation_owned = [true; 2];
            footwork.left_command_target = Some(left_target);
            footwork.right_command_target = Some(right_target);
            for (leg_index, (upper, lower, foot, mut target, left, support)) in [
                (
                    left_upper,
                    left_lower,
                    left_foot,
                    left_target,
                    true,
                    !footwork.release_handoff_active
                        && if stationary_guard {
                            !footwork.pivot_active || !footwork.pivot_left
                        } else {
                            !footwork.swing_left
                                && support_owner_preserves_contact(
                                    footwork.left_support_release_owner,
                                )
                        },
                ),
                (
                    right_upper,
                    right_lower,
                    right_foot,
                    right_target,
                    false,
                    !footwork.release_handoff_active
                        && if stationary_guard {
                            !footwork.pivot_active || footwork.pivot_left
                        } else {
                            footwork.swing_left
                                && support_owner_preserves_contact(
                                    footwork.right_support_release_owner,
                                )
                        },
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
                if raised_solver_follower {
                    let solve_hip = upper_snapshot.global.translation();
                    let warning_reach = (upper_length * upper_length
                        + lower_length * lower_length
                        + 2.0 * upper_length * lower_length * 30.0_f32.to_radians().cos())
                    .sqrt()
                        * 0.97;
                    let offset = target - solve_hip;
                    let distance = offset.length();
                    if distance > warning_reach {
                        if contact_driven_guard {
                            // A cadence target is a terrain contact, not an
                            // arbitrary point inside the leg sphere. Preserve
                            // its ground height and shorten only its planar
                            // displacement. Radially projecting the complete
                            // vector here lifted otherwise valid contacts into
                            // the air, after the gait planner had already
                            // conformed them to terrain.
                            target = constrain_guard_gait_target_to_reach(
                                target,
                                solve_hip,
                                warning_reach,
                            );
                        } else {
                            let radial = offset / distance;
                            target = solve_hip + radial * warning_reach;
                        }
                        let radial = (target - solve_hip).normalize_or_zero();
                        let (velocity, acceleration) = if left {
                            (
                                &mut footwork.left_target_velocity,
                                &mut footwork.left_target_acceleration,
                            )
                        } else {
                            (
                                &mut footwork.right_target_velocity,
                                &mut footwork.right_target_acceleration,
                            )
                        };
                        *velocity -= radial * velocity.dot(radial).max(0.0);
                        *acceleration -= radial * acceleration.dot(radial).max(0.0);
                    }
                    if left {
                        footwork.left_command_target = Some(target);
                    } else {
                        footwork.right_command_target = Some(target);
                    }
                }
                let side = anatomical_side(
                    rig_rotation,
                    rig_origin,
                    upper_snapshot.global.translation(),
                    left,
                );
                let remembered = if left {
                    footwork.left_knee_bend_world
                } else {
                    footwork.right_knee_bend_world
                }
                .or_else(|| {
                    if left {
                        memory.left_leg
                    } else {
                        memory.right_leg
                    }
                    .map(|bend| pole_to_world(rig_rotation, bend))
                });
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
                if left {
                    footwork.left_solve_hip = Some(upper_snapshot.global.translation());
                    footwork.left_solve_upper_length = Some(upper_length);
                    footwork.left_solve_lower_length = Some(lower_length);
                    footwork.left_commanded_pole = Some(pole);
                } else {
                    footwork.right_solve_hip = Some(upper_snapshot.global.translation());
                    footwork.right_solve_upper_length = Some(upper_length);
                    footwork.right_solve_lower_length = Some(lower_length);
                    footwork.right_commanded_pole = Some(pole);
                }
                let reach_limit = maximum_reach(upper_length, lower_length);
                if let Some(solution) = solve_two_bone_with_reach(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_snapshot.global.translation(),
                    target,
                    upper_length,
                    lower_length,
                    pole,
                    reach_limit,
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
                let toe_role = if left {
                    BoneRole::ToeLeft
                } else {
                    BoneRole::ToeRight
                };
                let rendered_toe = rig
                    .get(&toe_role)
                    .and_then(|toe| snapshot(*toe, &parents, &transforms.p0()))
                    .map(|rendered| rendered.global.translation());
                let reported_support = if enabled.0 {
                    rendered_ankle.is_some_and(|ankle| {
                        terrain
                            .and_then(|terrain| terrain.height_at(ankle.xz()))
                            .is_some_and(|height| {
                                raised_support_has_toe_clearance(
                                    raised_support_is_actual(
                                        support,
                                        if left {
                                            previous_left_support
                                        } else {
                                            previous_right_support
                                        },
                                        ankle.y,
                                        height,
                                    ),
                                    rendered_toe.map(|toe| toe.y),
                                    rendered_toe.and_then(|toe| {
                                        terrain.and_then(|terrain| terrain.height_at(toe.xz()))
                                    }),
                                )
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
            finalize_leg_rotation_chains(
                rig,
                skeleton,
                rig_rotation,
                &mut memory,
                evaluation_advances,
                state_delta_seconds,
                airborne_orientation_owned,
                [false; 2],
                &parents,
                &mut transforms,
            );
            // Classify support and retain handoff targets only after the final
            // cached-chain/orientation seam. This is the same local-transform
            // state that transform propagation exposes to viewer telemetry.
            for (foot, left, nominal_support) in [
                (
                    left_foot,
                    true,
                    !footwork.release_handoff_active
                        && if stationary_guard {
                            !footwork.pivot_active || !footwork.pivot_left
                        } else {
                            !footwork.swing_left
                                && support_owner_preserves_contact(
                                    footwork.left_support_release_owner,
                                )
                        },
                ),
                (
                    right_foot,
                    false,
                    !footwork.release_handoff_active
                        && if stationary_guard {
                            !footwork.pivot_active || footwork.pivot_left
                        } else {
                            footwork.swing_left
                                && support_owner_preserves_contact(
                                    footwork.right_support_release_owner,
                                )
                        },
                ),
            ] {
                let Some(rendered) = snapshot(foot, &parents, &transforms.p0()) else {
                    continue;
                };
                let ankle = rendered.global.translation();
                let toe_role = if left {
                    BoneRole::ToeLeft
                } else {
                    BoneRole::ToeRight
                };
                let rendered_toe = rig
                    .get(&toe_role)
                    .and_then(|toe| snapshot(*toe, &parents, &transforms.p0()))
                    .map(|rendered| rendered.global.translation());
                let reported_support = if enabled.0 {
                    terrain
                        .and_then(|terrain| terrain.height_at(ankle.xz()))
                        .is_some_and(|height| {
                            raised_support_has_toe_clearance(
                                raised_support_is_actual(
                                    nominal_support,
                                    if left {
                                        previous_left_support
                                    } else {
                                        previous_right_support
                                    },
                                    ankle.y,
                                    height,
                                ),
                                rendered_toe.map(|toe| toe.y),
                                rendered_toe.and_then(|toe| {
                                    terrain.and_then(|terrain| terrain.height_at(toe.xz()))
                                }),
                            )
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
            let visible_pelvis_local_transform = {
                let query = transforms.p1();
                rig.get(&BoneRole::Pelvis)
                    .and_then(|pelvis| query.get(*pelvis).ok())
                    .map(|pelvis| *pelvis)
            };
            if raised_release_active {
                let local_scalar_shift =
                    raised_pelvis_local_scalar_shift(rig, memory, &parents, &transforms.p0());
                let mut ordinary = ordinary_states
                    .get_mut(owner)
                    .map(|state| *state)
                    .unwrap_or_default();
                locomotion::publish_raised_release_handoff(
                    &mut ordinary,
                    memory,
                    visible_pelvis_local_transform,
                    local_scalar_shift,
                );
                if let Ok(mut state) = ordinary_states.get_mut(owner) {
                    *state = ordinary;
                } else {
                    commands.entity(owner).insert(ordinary);
                }
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                footwork.visible_pelvis_owner_local = rig
                    .get(&BoneRole::Pelvis)
                    .and_then(|pelvis| transforms.p0().compute_global_transform(*pelvis).ok())
                    .map(|pelvis| rig_rotation.inverse() * (pelvis.translation() - rig_origin));
                footwork.visible_pelvis_local_transform = visible_pelvis_local_transform;
                if release_handoff_complete {
                    footwork.release_handoff_active = false;
                    footwork.initialized = false;
                }
                if evaluation_advances {
                    if let Some(owner_kind) = footwork
                        .foot_motion_diagnostic(true)
                        .map(|motion| motion.owner)
                        && owner_kind != footwork.left_motion_owner_kind
                    {
                        footwork.left_motion_owner_kind = owner_kind;
                        footwork.left_motion_owner_epoch = semantic_tick;
                    }
                    if let Some(owner_kind) = footwork
                        .foot_motion_diagnostic(false)
                        .map(|motion| motion.owner)
                        && owner_kind != footwork.right_motion_owner_kind
                    {
                        footwork.right_motion_owner_kind = owner_kind;
                        footwork.right_motion_owner_epoch = semantic_tick;
                    }
                }
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
            clear_last_dual_support_handoff(&mut memory);
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
                let desired_ankle = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
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
                    planned_phase_start.unwrap_or(RUN_CONTACT_APPROACH_PHASE),
                    locomotion_profile(skeleton).support_phase_radius
                        + RUN_CONTACT_CHAIN_SETTLE_PHASE,
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
            let target_y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
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
            desired_hip_shift =
                desired_hip_shift.clamp(-RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP, 0.0);
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
                        RUN_PELVIS_CORRECTION_SPEED,
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
            // The thighs are siblings of the visual pelvis under the rig root.
            // Correct that shared owner so every cached knee pole and local
            // chain sees one coherent parent transform. Translating the three
            // sibling locals independently inverted the knee hemisphere.
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
                let target = frozen_plant.with_y(height + MEASURED_ANKLE_SOLE_OFFSET_METRES);
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
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_position,
                    target,
                    upper_length,
                    lower_length,
                    pole,
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
                    height + MEASURED_ANKLE_SOLE_OFFSET_METRES,
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
                            RUN_CONTACT_APPROACH_PHASE
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
                                        + RUN_CONTACT_CHAIN_SETTLE_PHASE,
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
                                    + RUN_CONTACT_CHAIN_SETTLE_PHASE,
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
                                    + RUN_CONTACT_CHAIN_SETTLE_PHASE,
                            )
                        } else {
                            smoothstep(approach_window, 0.0, phase_to_contact)
                        };
                    let mut desired_target =
                        planned_contact.map_or(foot_position, |mut contact| {
                            if let Some(height) = terrain.height_at(contact.xz()) {
                                contact.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
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
                            .max(height + MEASURED_ANKLE_SOLE_OFFSET_METRES + clearance);
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
                    let (mut owner_target, next_release_goal) = if run_airborne_budget {
                        // The typed follower owns the complete run release;
                        // do not feed its result through the legacy release
                        // speed state machine or retain that controller's goal.
                        (desired_owner_target, None)
                    } else if run_release_edge {
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
                                    AIRBORNE_RELEASE_TARGET_SPEED,
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
                                    > AIRBORNE_RELEASE_TARGET_SPEED * state_delta_seconds.max(0.0)
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
                                AIRBORNE_RELEASE_TARGET_SPEED,
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
                    let mut typed_replan = None;
                    if run_airborne_budget {
                        target.x = desired_target.x;
                        target.z = desired_target.z;
                        target.y = target.y.max(desired_target.y);
                        let contact_height = terrain
                            .height_at(target.xz())
                            .map(|height| height + MEASURED_ANKLE_SOLE_OFFSET_METRES)
                            .unwrap_or(target.y);
                        let contact_target = Vec3::new(target.x, contact_height, target.z);
                        let contact_reachable = run_contact_follower_plan(
                            &memory,
                            left,
                            previous_world_target.unwrap_or(foot_position),
                            contact_target,
                            upper_snapshot.global.translation(),
                            upper_snapshot.global.translation()
                                + skeleton.world_velocity * state_delta_seconds,
                            upper_length,
                            lower_length,
                            phase_to_contact / skeleton.animation_speed().max(0.1),
                            state_delta_seconds,
                        )
                        .feasible;
                        let support_eligible_for_descent = run_support_eligible_for_descent(
                            airborne_budget_gait,
                            skeleton.gait_phase,
                            left,
                            locomotion_profile(skeleton).support_phase_radius,
                            raw_nominal_weight,
                            contact_reachable
                                && run_contact_within_leg_reach(
                                    contact_target,
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
                            contact_height + clearance,
                            support_eligible_for_descent,
                        );
                        let ideal_target = target;
                        let ideal = WorldFootTargetSample::new(
                            ideal_target,
                            skeleton.world_velocity,
                            Vec3::ZERO,
                        )
                        .map(IdealFootTarget::WorldSwing)
                        .expect("finite run swing target");
                        let warning_reach = (upper_length * upper_length
                            + lower_length * lower_length
                            + 2.0 * upper_length * lower_length * 30.0_f32.to_radians().cos())
                        .sqrt();
                        let reach = FootReachEnvelope::new(
                            upper_snapshot.global.translation(),
                            upper_snapshot.global.translation()
                                + skeleton.world_velocity * state_delta_seconds,
                            warning_reach,
                            maximum_reach(upper_length, lower_length),
                        );
                        let deadline = planned_contact.map(|_| {
                            (phase_to_contact / skeleton.animation_speed().max(0.1))
                                .max(state_delta_seconds)
                        });
                        let (tracked_target, reason) = advance_runtime_foot_target(
                            &mut memory,
                            left,
                            previous_world_target.unwrap_or(foot_position),
                            ideal,
                            skeleton.world_velocity,
                            reach,
                            deadline,
                            state_delta_seconds,
                            evaluation_advances,
                        );
                        typed_replan = reason;
                        target = tracked_target;
                        if reason.is_some() {
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
                    let release_active = typed_replan.is_some()
                        || next_release_goal.is_some()
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
                        upper_snapshot.global.translation(),
                        lower_snapshot.global.translation(),
                        foot_position,
                        target,
                        upper_length,
                        lower_length,
                        pole,
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
                        + MEASURED_ANKLE_SOLE_OFFSET_METRES
                        + (SWING_SOLE_CLEARANCE_METRES * (1.0 - settle.progress))
                            .max(TERRAIN_TRANSITION_FLIGHT_TOE_CLEARANCE_METRES);
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
                let target = if settle.stateful_follower {
                    let ideal = WorldFootTargetSample::new(
                        desired_target,
                        skeleton.world_velocity,
                        Vec3::ZERO,
                    )
                    .map(IdealFootTarget::WorldSwing)
                    .expect("finite settle target");
                    let warning_reach = (upper_length * upper_length
                        + lower_length * lower_length
                        + 2.0 * upper_length * lower_length * 30.0_f32.to_radians().cos())
                    .sqrt();
                    let reach = FootReachEnvelope::new(
                        upper_snapshot.global.translation(),
                        upper_snapshot.global.translation()
                            + skeleton.world_velocity * state_delta_seconds,
                        warning_reach,
                        maximum_reach(upper_length, lower_length),
                    );
                    let presented_target = if left {
                        memory.left_foot_world_target
                    } else {
                        memory.right_foot_world_target
                    }
                    .unwrap_or(foot_position);
                    advance_runtime_foot_target(
                        &mut memory,
                        left,
                        presented_target,
                        ideal,
                        skeleton.world_velocity,
                        reach,
                        Some(((1.0 - settle.progress) * 0.25).max(state_delta_seconds)),
                        state_delta_seconds,
                        evaluation_advances,
                    )
                    .0
                } else {
                    advance_run_airborne_world_target(
                        previous_owner_target,
                        desired_target,
                        rig_origin,
                        rig_rotation,
                        state_delta_seconds,
                        settle_target_speed(settle),
                        |xz| {
                            let sole_minimum = terrain.height_at(xz).map(|height| {
                                height
                                    + MEASURED_ANKLE_SOLE_OFFSET_METRES
                                    + TERRAIN_TRANSITION_FLIGHT_TOE_CLEARANCE_METRES
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
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_position,
                    target,
                    upper_length,
                    lower_length,
                    pole,
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
            if plant_local.x * side < FOOT_TRACK_INNER {
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
            let sole_offset = MEASURED_ANKLE_SOLE_OFFSET_METRES;
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
                    TERRAIN_CONTACT_TOE_CLEARANCE_METRES,
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
            let release_target_speed = memory
                .settle
                .map(settle_target_speed)
                .unwrap_or(AIRBORNE_RELEASE_TARGET_SPEED);
            let support_run_airborne_budget = uses_stateful_support_follower(
                memory.settle,
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
            let mut run_acquisition_ideal = None;
            let mut target = if plant_acquired {
                planted_target
            } else if support_run_airborne_budget {
                // Entering nominal support does not bypass the airborne
                // follower. The frozen plant becomes direct only after the
                // propagated sole has truthfully acquired it.
                let fixed_contact_plan = run_contact_follower_plan(
                    &memory,
                    left,
                    previous_world_target.unwrap_or(foot_position),
                    planted_target,
                    upper_snapshot.global.translation(),
                    upper_snapshot.global.translation()
                        + skeleton.world_velocity * state_delta_seconds,
                    upper_length,
                    lower_length,
                    phase_to_next_contact(skeleton.gait_phase, left)
                        / skeleton.animation_speed().max(0.1),
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
                    && (!fixed_contact_plan.feasible || !fixed_contact_within_leg_reach)
                {
                    let mut transported_contact = fixed_contact_plan
                        .suggested_target
                        .unwrap_or(planted_target);
                    for _ in 0..2 {
                        transported_contact = upper_snapshot.global.translation()
                            + (transported_contact - upper_snapshot.global.translation())
                                .clamp_length_max(final_solve_reach);
                        transported_contact = constrain_foot_to_track(
                            transported_contact,
                            rig_origin,
                            rig_rotation,
                            side,
                        );
                        if let Some(height) = terrain.height_at(transported_contact.xz()) {
                            transported_contact.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
                        }
                    }
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
                let contact_reachable = run_contact_follower_plan(
                    &memory,
                    left,
                    previous_world_target.unwrap_or(foot_position),
                    planted_target,
                    upper_snapshot.global.translation(),
                    upper_snapshot.global.translation()
                        + skeleton.world_velocity * state_delta_seconds,
                    upper_length,
                    lower_length,
                    phase_to_next_contact(skeleton.gait_phase, left)
                        / skeleton.animation_speed().max(0.1),
                    state_delta_seconds,
                )
                .feasible;
                let acquisition_clearance = if contact_reachable {
                    0.0
                } else {
                    RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
                };
                let ideal = planted_target + Vec3::Y * acquisition_clearance;
                run_acquisition_ideal = Some((ideal, contact_reachable));
                ideal
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
            if memory.settle.is_some() && !plant_acquired && !support_run_airborne_budget {
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
                            .map(|height| height + MEASURED_ANKLE_SOLE_OFFSET_METRES)
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
                                    + MEASURED_ANKLE_SOLE_OFFSET_METRES
                                    + TERRAIN_TRANSITION_FLIGHT_TOE_CLEARANCE_METRES
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
            if support_run_airborne_budget {
                if plant_acquired {
                    let planted = FootFollowerState::from_presented_pose(
                        target,
                        Vec3::ZERO,
                        Vec3::ZERO,
                        planted_target,
                        Vec3::ZERO,
                        Vec3::ZERO,
                    );
                    if left {
                        memory.left_foot_follower = planted;
                    } else {
                        memory.right_foot_follower = planted;
                    }
                } else if let Some((ideal_position, contact_feasible)) = run_acquisition_ideal {
                    let ideal = if contact_feasible {
                        IdealFootTarget::world_plant(ideal_position)
                    } else {
                        WorldFootTargetSample::new(ideal_position, Vec3::ZERO, Vec3::ZERO)
                            .map(IdealFootTarget::WorldSwing)
                    };
                    let Some(ideal) = ideal else {
                        continue;
                    };
                    let warning_reach = (upper_length * upper_length
                        + lower_length * lower_length
                        + 2.0 * upper_length * lower_length * 30.0_f32.to_radians().cos())
                    .sqrt();
                    let reach = FootReachEnvelope::new(
                        upper_snapshot.global.translation(),
                        upper_snapshot.global.translation()
                            + skeleton.world_velocity * state_delta_seconds,
                        warning_reach,
                        maximum_reach(upper_length, lower_length),
                    );
                    let deadline = Some(
                        (phase_to_next_contact(skeleton.gait_phase, left)
                            / skeleton.animation_speed().max(0.1))
                        .max(state_delta_seconds),
                    );
                    let (tracked, reason) = advance_runtime_foot_target(
                        &mut memory,
                        left,
                        previous_world_target.unwrap_or(foot_position),
                        ideal,
                        Vec3::ZERO,
                        reach,
                        deadline,
                        state_delta_seconds,
                        evaluation_advances,
                    );
                    target = tracked;
                    if reason.is_some() {
                        plant = None;
                        if left {
                            memory.left_foot_plant = None;
                            memory.left_foot_plant_acquired = false;
                        } else {
                            memory.right_foot_plant = None;
                            memory.right_foot_plant_acquired = false;
                        }
                    }
                }
            } else {
                if left {
                    memory.left_foot_follower = None;
                } else {
                    memory.right_foot_follower = None;
                }
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
            let solve_reach =
                if skeleton.posture() == Posture::Crouched || skeleton.animation_speed() <= 0.05 {
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
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                foot_position,
                target,
                upper_length,
                lower_length,
                pole,
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

fn leg_ik_was_evaluated_at(memory: LegIkMemory, tick: u64) -> bool {
    memory.evaluation_tick == Some(tick) || memory.knee_yaw_evaluation_tick == Some(tick)
}

fn raised_leg_contributes_pelvis_reach(
    footwork: &RaisedFootworkState,
    moving: bool,
    left: bool,
) -> bool {
    let (support_weight, support_release) = if left {
        (
            footwork.left_support_weight,
            footwork.left_support_release_owner,
        )
    } else {
        (
            footwork.right_support_weight,
            footwork.right_support_release_owner,
        )
    };
    footwork.initialized
        && !footwork.release_handoff_active
        && support_owner_preserves_contact(support_release)
        && support_weight >= 0.5
        && if moving {
            footwork.swing_left != left
        } else {
            !footwork.pivot_active || footwork.pivot_left != left
        }
}

fn raised_leg_is_stationary_contact_candidate(
    footwork: &RaisedFootworkState,
    moving: bool,
    left: bool,
) -> bool {
    let support_release = if left {
        footwork.left_support_release_owner
    } else {
        footwork.right_support_release_owner
    };
    footwork.initialized
        && !moving
        && !footwork.release_handoff_active
        && support_owner_preserves_contact(support_release)
        && (!footwork.pivot_active || footwork.pivot_left == left)
}

/// Refresh contact diagnostics from propagated globals. The IK pass runs
/// before transform propagation, while viewer/gameplay consumers observe the
/// propagated hierarchy; twist bones and acquisition blending can make those
/// positions differ materially from the analytic endpoint.
pub(in crate::animation) fn refresh_raised_support_after_propagation(
    clock: Res<ProceduralAnimationClock>,
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
        if locomotion::owns(skeleton) {
            continue;
        }
        let Ok(mut state) = ik_states.get_mut(owner) else {
            continue;
        };
        let tick = clock.semantic_step().0;
        if !raised_refresh_advances(state.0.raised_refresh_evaluation_tick, tick) {
            continue;
        }
        state.0.raised_refresh_evaluation_tick = Some(tick);
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
        let owner_frame = state.0.rig_origin.zip(state.0.rig_rotation);
        let left_rendered_world = state.0.left_last_rendered_world;
        let right_rendered_world = state.0.right_last_rendered_world;
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
        if !raised.initialized || !raised_refresh_owns_support(skeleton, &raised) {
            continue;
        }
        let stationary_guard = !skeleton.raised_locomotion().is_moving();
        for (role, toe_role, left, nominal_support) in [
            (
                BoneRole::FootLeft,
                BoneRole::ToeLeft,
                true,
                !raised.swing_left,
            ),
            (
                BoneRole::FootRight,
                BoneRole::ToeRight,
                false,
                raised.swing_left,
            ),
        ] {
            let nominal_support = if stationary_guard {
                !raised.pivot_active || raised.pivot_left != left
            } else {
                nominal_support
            };
            let Some(&foot) = rig.get(&role) else {
                continue;
            };
            let Ok(global) = globals.get(foot) else {
                continue;
            };
            let ankle = global.translation();
            let rendered_toe = rig
                .get(&toe_role)
                .and_then(|toe| globals.get(*toe).ok())
                .map(GlobalTransform::translation);
            let support = terrain.height_at(ankle.xz()).is_some_and(|height| {
                raised_support_has_toe_clearance(
                    raised_support_is_actual(
                        nominal_support,
                        if left {
                            terrain_leg_has_support(raised.left_support_weight)
                        } else {
                            terrain_leg_has_support(raised.right_support_weight)
                        },
                        ankle.y,
                        height,
                    ),
                    rendered_toe.map(|toe| toe.y),
                    rendered_toe.and_then(|toe| terrain.height_at(toe.xz())),
                )
            });
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

fn raised_refresh_owns_support(skeleton: &SkeletonState, raised: &RaisedFootworkState) -> bool {
    raised_release_owns_ik(skeleton, Some(raised))
        || (raised_guard_ground_contact_is_valid(skeleton, raised.initialized)
            && skeleton.weapon_guard() == WeaponGuardState::Raised
            && matches!(
                skeleton.action_kind(),
                SkeletonAction::None | SkeletonAction::Attack | SkeletonAction::Block
            ))
}

pub(super) fn raised_footwork_posture_is_valid(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded() && skeleton.posture() == Posture::Upright
}

fn raised_guard_ground_contact_is_valid(
    skeleton: &SkeletonState,
    raised_footwork_was_active: bool,
) -> bool {
    raised_footwork_posture_is_valid(skeleton)
        || (raised_footwork_was_active
            && skeleton.posture() == Posture::Upright
            && !skeleton.is_posture_transitioning()
            && skeleton.world_velocity.y.abs() <= 0.5
            && matches!(
                skeleton.action_kind(),
                SkeletonAction::None | SkeletonAction::Attack | SkeletonAction::Block
            ))
}

pub(super) fn terrain_ik_posture_is_valid(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded()
        && !skeleton.is_posture_transitioning()
        && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
        && matches!(
            skeleton.action_kind(),
            SkeletonAction::None | SkeletonAction::Attack | SkeletonAction::Block
        )
}

pub(super) fn terrain_leg_has_support(weight: f32) -> bool {
    weight > 0.05
}

fn update_contact_orientation_blend(
    active: bool,
    previous_support: Option<f32>,
    reported_support: f32,
) -> bool {
    let supported = terrain_leg_has_support(reported_support);
    supported && (active || !previous_support.is_some_and(terrain_leg_has_support))
}

pub(super) fn retained_plant_requires_release(retained: Vec3, reachable: Vec3) -> bool {
    retained.xz().distance(reachable.xz()) > MAX_RETAINED_PLANT_REACH_CORRECTION
}

fn ordinary_plant_requires_clear(
    support_weight: f32,
    acquired: bool,
    plant: Option<Vec3>,
    authored_foot: Vec3,
) -> bool {
    support_weight <= 0.05
        || (!acquired
            && plant.is_some_and(|position| !plant_is_continuous(position, authored_foot)))
}

fn coordinated_support_weight(
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

fn run_toe_off_support_weight(
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

fn run_retained_support_through_lobe_edge(
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

fn run_release_edge(previous_support_released: bool, toe_off_started: bool) -> bool {
    previous_support_released || toe_off_started
}

fn unplanned_terrain_solve_requires_release(
    planned_contact: Option<Vec3>,
    solved_target: Vec3,
    authored_target: Vec3,
) -> bool {
    planned_contact.is_none() && solved_target.distance(authored_target) > 0.03
}

fn unplanned_support_release_is_owned(
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

fn run_airborne_owner_target_speed(just_released: bool) -> f32 {
    if just_released {
        // The first uphill flight sample must satisfy the semantic 5 cm sole
        // floor and the visible 9.5 cm foot bound simultaneously. A 9 cm
        // search sphere can contain no terrain-valid point, causing the
        // fallback to exceed both its own budget and the rendered gate. Use
        // the remaining sub-gate margin only for this release projection.
        RUN_FIRST_RELEASE_OWNER_TARGET_SPEED
    } else {
        RUN_AIRBORNE_OWNER_TARGET_SPEED
    }
}

fn run_airborne_owner_target_speed_for_sample(
    just_released: bool,
    settle_cancelled_for_restart: bool,
) -> f32 {
    if settle_cancelled_for_restart {
        // A cancelled settle already owns a bounded visible ankle and knee
        // chain. Return that chain to ordinary locomotion at the settle release
        // budget for the first restart sample; Run's wider swing budget can
        // amplify an otherwise valid ankle step past the knee continuity gate
        // near extension.
        AIRBORNE_RELEASE_TARGET_SPEED
    } else {
        run_airborne_owner_target_speed(just_released)
    }
}

fn uses_run_airborne_motion_budget(gait: LocomotionGait, planar_speed: f32) -> bool {
    gait == LocomotionGait::Run
        || planar_speed
            >= (WALK_LOCOMOTION_PROFILE.reference_speed + RUN_LOCOMOTION_PROFILE.reference_speed)
                * 0.5
}

fn uses_stateful_support_follower(
    settle: Option<LocomotionSettleState>,
    gait: LocomotionGait,
    planar_speed: f32,
) -> bool {
    settle.map_or_else(
        || uses_run_airborne_motion_budget(gait, planar_speed),
        |settle| settle.stateful_follower,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn bound_unacquired_run_support_release_target(
    run_budget: bool,
    has_plan: bool,
    acquired: bool,
    release_owned: bool,
    previous_owner_target: Option<Vec3>,
    desired_world_target: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    delta_seconds: f32,
    minimum_world_y: impl Fn(Vec2) -> Option<f32>,
) -> Vec3 {
    if run_budget && !has_plan && !acquired && release_owned {
        advance_run_airborne_world_target(
            previous_owner_target,
            desired_world_target,
            rig_origin,
            rig_rotation,
            delta_seconds,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            minimum_world_y,
        )
    } else {
        desired_world_target
    }
}

fn support_release_diagnostic_goal(
    release_active: bool,
    bounded_unplanned_release: bool,
    bounded_owner_target: Vec3,
    desired_owner_target: Vec3,
) -> Option<Vec3> {
    release_active.then_some(if bounded_unplanned_release {
        // The terrain-feasible bounded waypoint is the target this release
        // actually owns for the current sample. Reporting the unreachable
        // final contact as a frozen goal makes a necessary uphill projection
        // appear to move away from its owner even though rendered continuity
        // is valid and the waypoint advances monotonically.
        bounded_owner_target
    } else {
        desired_owner_target
    })
}

fn resolved_unacquired_support_release_ownership(
    bounded_unplanned_release: bool,
    resolved_end: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
) -> Option<(Vec3, Vec3)> {
    bounded_unplanned_release.then(|| {
        let resolved_owner = rig_rotation.inverse() * (resolved_end - rig_origin);
        (resolved_end, resolved_owner)
    })
}

fn airborne_unplanned_release_uses_resolved_end(
    run_airborne_budget: bool,
    planned_contact: Option<Vec3>,
    release_active: bool,
) -> bool {
    run_airborne_budget && planned_contact.is_none() && release_active
}

#[allow(clippy::too_many_arguments)]
fn commit_resolved_unplanned_airborne_release(
    memory: &mut LegIkMemory,
    left: bool,
    run_airborne_budget: bool,
    planned_contact: Option<Vec3>,
    release_active: bool,
    resolved_end: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    commit_resolved_unacquired_support_release(
        memory,
        left,
        airborne_unplanned_release_uses_resolved_end(
            run_airborne_budget,
            planned_contact,
            release_active,
        ),
        resolved_end,
        rig_origin,
        rig_rotation,
    );
}

fn commit_resolved_unacquired_support_release(
    memory: &mut LegIkMemory,
    left: bool,
    bounded_unplanned_release: bool,
    resolved_end: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    let Some((resolved_world, resolved_owner)) = resolved_unacquired_support_release_ownership(
        bounded_unplanned_release,
        resolved_end,
        rig_origin,
        rig_rotation,
    ) else {
        return;
    };
    if left {
        memory.left_foot_world_target = Some(resolved_world);
        memory.left_foot_target = Some(resolved_owner);
        memory.left_release_target = Some(resolved_owner);
    } else {
        memory.right_foot_world_target = Some(resolved_world);
        memory.right_foot_target = Some(resolved_owner);
        memory.right_release_target = Some(resolved_owner);
    }
}

fn update_measured_owner_planar_speed(
    retained_speed: f32,
    previous_origin: Option<Vec3>,
    current_origin: Vec3,
    delta_seconds: f32,
    evaluation_advances: bool,
    owner_discontinuous: bool,
) -> f32 {
    if !evaluation_advances {
        retained_speed
    } else if owner_discontinuous || delta_seconds <= 0.0 {
        0.0
    } else {
        previous_origin
            .map(|previous| current_origin.xz().distance(previous.xz()) / delta_seconds)
            .filter(|speed| speed.is_finite())
            .unwrap_or(0.0)
    }
}

fn run_is_at_support_exit(phase: f32, left: bool, support_radius: f32) -> bool {
    let contact_phase = if left { 0.0 } else { 0.5 };
    let post_contact = (phase - contact_phase).rem_euclid(1.0);
    // Release on the first sampled phase beyond the nominal lobe, not on its
    // decaying shoulder. The half-cycle bound distinguishes this foot's
    // post-contact side from its next rising shoulder after wrap.
    post_contact >= support_radius && post_contact < 0.5
}

fn exhausted_latch_after_raw_cadence(exhausted: bool, raw_nominal_support: f32) -> bool {
    // Exhaustion suppresses only the remainder of the current support lobe.
    // Consult the unsuppressed gait cadence here: reported/effective support
    // may be zero precisely because this latch is active and therefore cannot
    // prove that the foot has crossed true flight into its next cycle.
    exhausted && terrain_leg_has_support(raw_nominal_support)
}

fn run_plan_is_on_rising_support(
    gait: LocomotionGait,
    phase: f32,
    left: bool,
    support_radius: f32,
    raw_nominal_support: f32,
    planned_contact: Option<Vec3>,
    acquired: bool,
) -> bool {
    gait == LocomotionGait::Run
        && planned_contact.is_some()
        && !acquired
        && terrain_leg_has_support(raw_nominal_support)
        // A rising shoulder approaches this foot's contact center. The
        // post-contact shoulder has almost a complete cycle remaining and
        // must not reopen a just-exhausted lobe.
        && phase_to_next_contact(phase, left) <= support_radius + 0.001
}

fn acquired_plan_can_clear(acquired: bool) -> bool {
    acquired
}

fn clear_planned_contact_metadata(
    contact: &mut Option<Vec3>,
    start: &mut Option<Vec3>,
    phase_start: &mut Option<f32>,
) {
    *contact = None;
    *start = None;
    *phase_start = None;
}

fn clear_all_planned_contact_metadata(memory: &mut LegIkMemory) {
    clear_planned_contact_metadata(
        &mut memory.left_planned_contact,
        &mut memory.left_planned_contact_start,
        &mut memory.left_planned_contact_phase_start,
    );
    clear_planned_contact_metadata(
        &mut memory.right_planned_contact,
        &mut memory.right_planned_contact_start,
        &mut memory.right_planned_contact_phase_start,
    );
}

fn acquisition_lobe_exited_without_contact(
    planned_contact: Option<Vec3>,
    acquired: bool,
    previous_support: Option<f32>,
    current_support: f32,
) -> bool {
    planned_contact.is_some()
        && !acquired
        && previous_support.is_some_and(terrain_leg_has_support)
        && !terrain_leg_has_support(current_support)
}

fn support_after_exhausted_lobe(exhausted: bool, nominal_weight: f32) -> (bool, f32) {
    if !exhausted {
        (false, nominal_weight)
    } else if terrain_leg_has_support(nominal_weight) {
        (true, 0.0)
    } else {
        (false, nominal_weight)
    }
}

fn run_planned_contact_allowed(
    support_lobe_exhausted: bool,
    phase_to_contact: f32,
    approach_window: f32,
) -> bool {
    !support_lobe_exhausted && phase_to_contact <= approach_window
}

pub(super) fn authored_knee_pole_world(
    hip: Vec3,
    authored_knee: Vec3,
    target: Vec3,
    canonical: Vec3,
) -> Option<Vec3> {
    let target_direction = (target - hip).try_normalize()?;
    let bend = (authored_knee - hip).reject_from_normalized(target_direction);
    bend.try_normalize()
        .filter(|pole| pole.dot(canonical) > 0.2)
}

fn retained_terrain_pole(remembered: Vec3, canonical: Vec3) -> Option<Vec3> {
    let remembered = remembered.try_normalize()?;
    // The old 0.2 cutoff discarded a still-valid shallow bend during the
    // support-confidence ramp and rebuilt the knee from authored FK one tick
    // later. Owner/mode discontinuities explicitly clear this cache, so any
    // finite pole in the anatomical hemisphere remains authoritative here.
    (remembered.dot(canonical) > 0.0).then_some(remembered)
}

fn transported_terrain_pole(
    remembered: Option<Vec3>,
    previous_end_direction: Option<Vec3>,
    next_end_direction: Vec3,
    canonical: Vec3,
) -> Option<Vec3> {
    let remembered = remembered?.try_normalize()?;
    let Some(previous) = previous_end_direction else {
        return retained_terrain_pole(remembered, canonical);
    };
    let previous = previous.try_normalize()?;
    let next = next_end_direction.try_normalize()?;
    (Quat::from_rotation_arc(previous, next) * remembered).try_normalize()
}

fn guard_pivot_target(start: Vec3, end: Vec3, origin: Vec3, support: Vec3, progress: f32) -> Vec3 {
    let progress = progress.clamp(0.0, 1.0);
    let start_offset = (start - origin).xz();
    let end_offset = (end - origin).xz();
    let Some(start_direction) = start_offset.try_normalize() else {
        return start.lerp(end, progress);
    };
    let Some(end_direction) = end_offset.try_normalize() else {
        return start.lerp(end, progress);
    };
    let start_angle = start_direction.y.atan2(start_direction.x);
    let end_angle = end_direction.y.atan2(end_direction.x);
    let angle_delta = (end_angle - start_angle + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let angle = start_angle + angle_delta * progress;
    let radius = start_offset.length().lerp(end_offset.length(), progress);
    let mut planar = Vec2::new(angle.cos(), angle.sin()) * radius;
    let support_planar = (support - origin).xz();
    let separation = planar - support_planar;
    if separation.length() < GUARD_TARGET_INTER_FOOT_SEPARATION {
        let away = separation
            .try_normalize()
            .unwrap_or_else(|| planar.normalize_or_zero());
        planar = support_planar + away * GUARD_TARGET_INTER_FOOT_SEPARATION;
    }
    Vec3::new(
        origin.x + planar.x,
        start.y.lerp(end.y, progress) + c2_swing_arch(progress) * GUARD_PIVOT_LIFT_METRES,
        origin.z + planar.y,
    )
}

fn stationary_guard_pivot_has_landed(
    presented: Vec3,
    endpoint: Vec3,
    terrain_height: Option<f32>,
) -> bool {
    presented.xz().distance(endpoint.xz()) <= SOLE_CONTACT_TOLERANCE_METRES
        && terrain_height.is_some_and(|height| sole_is_at_contact(presented.y, height))
}

fn stationary_guard_pivot_endpoint(
    constrained_endpoint: Vec3,
    terrain_height: Option<f32>,
) -> Option<Vec3> {
    terrain_height.map(|height| terrain_conformed_guard_target(constrained_endpoint, Some(height)))
}

fn c2_swing_arch(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    64.0 * progress.powi(3) * (1.0 - progress).powi(3)
}

fn guard_swing_replan_sample(segment: C2FootSegment) -> GuardSwingSample {
    guard_swing_replan_sample_at_progress(segment, segment.timing.progress())
}

fn guard_swing_replan_sample_at_progress(
    segment: C2FootSegment,
    progress: f32,
) -> GuardSwingSample {
    guard_boundary_quintic_sample(
        segment.start,
        segment.start_velocity,
        segment.start_acceleration,
        segment.end.position(),
        progress,
        segment.timing.duration_seconds(),
        0.0,
    )
}

fn guard_boundary_quintic_sample(
    start: Vec3,
    start_velocity: Vec3,
    start_acceleration: Vec3,
    end: Vec3,
    progress: f32,
    duration_seconds: f32,
    arch_height: f32,
) -> GuardSwingSample {
    let progress = progress.clamp(0.0, 1.0);
    let duration = duration_seconds.max(f32::EPSILON);
    let velocity_term = start_velocity * duration;
    let acceleration_term = start_acceleration * duration.powi(2);
    let residual = end - start - velocity_term - acceleration_term * 0.5;
    let final_velocity_residual = -velocity_term - acceleration_term;
    let final_acceleration_residual = -acceleration_term;
    let c3 = residual * 10.0 - final_velocity_residual * 4.0 + final_acceleration_residual * 0.5;
    let c4 = residual * -15.0 + final_velocity_residual * 7.0 - final_acceleration_residual;
    let c5 = residual * 6.0 - final_velocity_residual * 3.0 + final_acceleration_residual * 0.5;
    let position = start
        + velocity_term * progress
        + acceleration_term * (0.5 * progress.powi(2))
        + c3 * progress.powi(3)
        + c4 * progress.powi(4)
        + c5 * progress.powi(5);
    let velocity = (velocity_term
        + acceleration_term * progress
        + c3 * (3.0 * progress.powi(2))
        + c4 * (4.0 * progress.powi(3))
        + c5 * (5.0 * progress.powi(4)))
        / duration;
    let acceleration = (acceleration_term
        + c3 * (6.0 * progress)
        + c4 * (12.0 * progress.powi(2))
        + c5 * (20.0 * progress.powi(3)))
        / duration.powi(2);
    let arch = guard_quintic_sample(Vec3::ZERO, Vec3::ZERO, progress, duration, arch_height);
    GuardSwingSample {
        position: position + arch.position,
        velocity: velocity + arch.velocity,
        acceleration: acceleration + arch.acceleration,
    }
}

/// Uses the convex-hull property of the Bernstein form to bound the complete
/// acceleration cubic and jerk quadratic, including both endpoint peaks.
/// This is conservative and does not depend on a temporal sampling rate.
fn c2_segment_dynamics_are_bounded(
    segment: C2FootSegment,
    maximum_acceleration: f32,
    maximum_jerk: f32,
) -> bool {
    let duration = segment.timing.duration_seconds();
    quintic_vector_dynamics_are_bounded(
        segment.start,
        segment.start_velocity,
        segment.start_acceleration,
        segment.end.position(),
        duration,
        maximum_acceleration,
        maximum_jerk,
    )
}

fn quintic_vector_dynamics_are_bounded(
    start: Vec3,
    start_velocity: Vec3,
    start_acceleration: Vec3,
    end: Vec3,
    duration: f32,
    maximum_acceleration: f32,
    maximum_jerk: f32,
) -> bool {
    if !duration.is_finite() || duration <= f32::EPSILON {
        return false;
    }
    let velocity_term = start_velocity * duration;
    let acceleration_term = start_acceleration * duration.powi(2);
    let residual = end - start - velocity_term - acceleration_term * 0.5;
    let final_velocity_residual = -velocity_term - acceleration_term;
    let final_acceleration_residual = -acceleration_term;
    let c3 = residual * 10.0 - final_velocity_residual * 4.0 + final_acceleration_residual * 0.5;
    let c4 = residual * -15.0 + final_velocity_residual * 7.0 - final_acceleration_residual;
    let c5 = residual * 6.0 - final_velocity_residual * 3.0 + final_acceleration_residual * 0.5;

    let inverse_duration_squared = duration.recip().powi(2);
    let acceleration_power = [
        acceleration_term * inverse_duration_squared,
        c3 * (6.0 * inverse_duration_squared),
        c4 * (12.0 * inverse_duration_squared),
        c5 * (20.0 * inverse_duration_squared),
    ];
    let acceleration_bernstein = [
        acceleration_power[0],
        acceleration_power[0] + acceleration_power[1] / 3.0,
        acceleration_power[0] + acceleration_power[1] * (2.0 / 3.0) + acceleration_power[2] / 3.0,
        acceleration_power.iter().copied().sum(),
    ];
    let inverse_duration = duration.recip();
    let jerk_power = [
        acceleration_power[1] * inverse_duration,
        acceleration_power[2] * (2.0 * inverse_duration),
        acceleration_power[3] * (3.0 * inverse_duration),
    ];
    let jerk_bernstein = [
        jerk_power[0],
        jerk_power[0] + jerk_power[1] * 0.5,
        jerk_power.iter().copied().sum(),
    ];
    cubic_bernstein_within_bound(acceleration_bernstein, maximum_acceleration + 0.001, 8)
        && quadratic_bernstein_within_bound(jerk_bernstein, maximum_jerk + 0.001, 8)
}

fn c2_segment_position_control(segment: C2FootSegment) -> [Vec3; 6] {
    let duration = segment.timing.duration_seconds();
    let end = segment.end.position();
    [
        segment.start,
        segment.start + segment.start_velocity * (duration / 5.0),
        segment.start
            + segment.start_velocity * (duration * 2.0 / 5.0)
            + segment.start_acceleration * (duration.powi(2) / 20.0),
        end,
        end,
        end,
    ]
}

fn contact_error_envelope(segment: C2FootSegment) -> Option<ContactErrorEnvelope> {
    segment.end.is_contact().then(|| ContactErrorEnvelope {
        initial_lag: segment.start.distance(segment.end.position()),
        duration_seconds: segment.timing.duration_seconds(),
    })
}

fn contact_error_envelope_is_proven(segment: C2FootSegment) -> bool {
    let Some(envelope) = contact_error_envelope(segment) else {
        return true;
    };
    let endpoint = segment.end.position();
    let controls = c2_segment_position_control(segment).map(|point| point - endpoint);
    let scalar_controls = [
        envelope.initial_lag,
        envelope.initial_lag,
        envelope.initial_lag,
        0.0,
        0.0,
        0.0,
    ];
    controls
        .into_iter()
        .zip(scalar_controls)
        .all(|(relative, permitted)| {
            relative.is_finite()
                && relative.length()
                    <= permitted + f32::EPSILON * 256.0 * envelope.initial_lag.abs().max(1.0)
        })
}

fn contact_tick_span(seconds_until_authoritative_contact: f32) -> Option<SegmentTickSpan> {
    if !seconds_until_authoritative_contact.is_finite()
        || seconds_until_authoritative_contact <= 0.0
    {
        return None;
    }
    SegmentTickSpan::new((seconds_until_authoritative_contact * CONTINUITY_SAMPLE_HZ).ceil() as u32)
}

fn quantized_release_duration(seconds: f32) -> f32 {
    ((seconds.max(1.0 / CONTINUITY_SAMPLE_HZ) * CONTINUITY_SAMPLE_HZ).ceil() / CONTINUITY_SAMPLE_HZ)
        .max(1.0 / CONTINUITY_SAMPLE_HZ)
}

fn release_tick_span(seconds: f32) -> Option<SegmentTickSpan> {
    SegmentTickSpan::new(
        (quantized_release_duration(seconds) * CONTINUITY_SAMPLE_HZ).round() as u32,
    )
}

fn advance_c2_segment_tick(segment: &mut C2FootSegment, advances: bool, current_tick: u64) {
    if !advances || current_tick <= segment.owner_epoch {
        return;
    }
    segment.timing.elapsed_ticks = current_tick
        .saturating_sub(segment.owner_epoch)
        .min(u64::from(segment.timing.total_ticks.get())) as u32;
}

fn direct_c2_sample_is_live_reachable(
    segment: C2FootSegment,
    sample: GuardSwingSample,
    reach: Option<FootReachEnvelope>,
    delta_seconds: f32,
) -> bool {
    let Some(reach) = reach else {
        return false;
    };
    if segment.end.is_contact() {
        reach.contains_warning_at(sample.position, delta_seconds, delta_seconds)
    } else {
        sample.position.distance(reach.next_root()) <= reach.hard_reach() + 0.0001
    }
}

fn quadratic_bernstein_within_bound(control: [Vec3; 3], limit: f32, depth: u8) -> bool {
    if control
        .iter()
        .all(|value| value.is_finite() && value.length() <= limit)
    {
        return true;
    }
    if depth == 0 || control.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let a = control[0].lerp(control[1], 0.5);
    let b = control[1].lerp(control[2], 0.5);
    let midpoint = a.lerp(b, 0.5);
    quadratic_bernstein_within_bound([control[0], a, midpoint], limit, depth - 1)
        && quadratic_bernstein_within_bound([midpoint, b, control[2]], limit, depth - 1)
}

fn cubic_bernstein_within_bound(control: [Vec3; 4], limit: f32, depth: u8) -> bool {
    if control
        .iter()
        .all(|value| value.is_finite() && value.length() <= limit)
    {
        return true;
    }
    if depth == 0 || control.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let a = control[0].lerp(control[1], 0.5);
    let b = control[1].lerp(control[2], 0.5);
    let c = control[2].lerp(control[3], 0.5);
    let d = a.lerp(b, 0.5);
    let e = b.lerp(c, 0.5);
    let midpoint = d.lerp(e, 0.5);
    cubic_bernstein_within_bound([control[0], a, d, midpoint], limit, depth - 1)
        && cubic_bernstein_within_bound([midpoint, e, c, control[3]], limit, depth - 1)
}

fn guard_quintic_sample(
    start: Vec3,
    end: Vec3,
    progress: f32,
    duration_seconds: f32,
    arch_height: f32,
) -> GuardSwingSample {
    let progress = progress.clamp(0.0, 1.0);
    let duration = duration_seconds.max(f32::EPSILON);
    let one_minus = 1.0 - progress;
    let blend = quintic_progress(progress);
    let blend_velocity = 30.0 * progress.powi(2) * one_minus.powi(2) / duration;
    let blend_acceleration =
        60.0 * progress * one_minus * (1.0 - 2.0 * progress) / duration.powi(2);
    let arch = c2_swing_arch(progress);
    let arch_velocity = 64.0
        * (3.0 * progress.powi(2) - 12.0 * progress.powi(3) + 15.0 * progress.powi(4)
            - 6.0 * progress.powi(5))
        / duration;
    let arch_acceleration = 64.0
        * (6.0 * progress - 36.0 * progress.powi(2) + 60.0 * progress.powi(3)
            - 30.0 * progress.powi(4))
        / duration.powi(2);
    let displacement = end - start;
    GuardSwingSample {
        position: start.lerp(end, blend) + Vec3::Y * arch * arch_height,
        velocity: displacement * blend_velocity + Vec3::Y * arch_velocity * arch_height,
        acceleration: displacement * blend_acceleration + Vec3::Y * arch_acceleration * arch_height,
    }
}

fn guard_swing_replan_segment(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    requested_endpoint: Vec3,
    cadence_progress: f32,
    trajectory: Option<PredictedHipTrajectory>,
) -> FootEndpointPlan {
    let cadence_seconds = (1.0 - cadence_progress.clamp(0.0, 1.0)) * 0.32;
    plan_c2_foot_segment(
        presented,
        presented_velocity,
        presented_acceleration,
        requested_endpoint,
        cadence_seconds,
        trajectory,
    )
}

fn guard_cadence_contact_tick_span(phase: f32, speed: f32) -> Option<SegmentTickSpan> {
    if !phase.is_finite() || !speed.is_finite() || speed <= 0.01 {
        return None;
    }
    let half_step_progress = (phase.rem_euclid(1.0) * 2.0).fract();
    // The replicated raised intent reports observed speed, which can still be
    // near zero on the first acceleration sample. Scheduling contact from that
    // instantaneous speed promises a late endpoint that the accelerating
    // authoritative cadence reaches first. Plan against the earliest lawful
    // guard edge instead. A completed contact remains a typed terminal owner
    // until a slower authoritative edge arrives.
    let deadline_speed = speed.max(TACTICAL_GUARD_SPEED_METRES_PER_SECOND);
    let half_step_seconds = guard_step_length(deadline_speed) / deadline_speed;
    guard_contact_tick_span((1.0 - half_step_progress) * half_step_seconds)
}

fn guard_following_cadence_contact_tick_span(phase: f32, speed: f32) -> Option<SegmentTickSpan> {
    if !phase.is_finite() || !speed.is_finite() || speed <= 0.01 {
        return None;
    }
    let half_step_progress = (phase.rem_euclid(1.0) * 2.0).fract();
    let deadline_speed = speed.max(TACTICAL_GUARD_SPEED_METRES_PER_SECOND);
    let half_step_seconds = guard_step_length(deadline_speed) / deadline_speed;
    // The contact-driven gait has already advanced the local swing identity,
    // while replicated cadence still describes the just-landed foot. Its
    // deadline is therefore the *following* edge: the remainder of this
    // half-step plus one complete half-step.
    guard_contact_tick_span((2.0 - half_step_progress) * half_step_seconds)
}

fn guard_contact_tick_span(seconds_until_authoritative_contact: f32) -> Option<SegmentTickSpan> {
    if !seconds_until_authoritative_contact.is_finite()
        || seconds_until_authoritative_contact <= 0.0
    {
        return None;
    }
    // Authoritative raised cadence is observed through sparse presentation
    // snapshots. A ceil-rounded local promise can therefore land one semantic
    // tick after the already-crossed server edge. Contact is a hard deadline,
    // so conservatively use the last complete tick before the predicted
    // crossing. Release owners continue to quantize upward.
    SegmentTickSpan::new(
        ((seconds_until_authoritative_contact * CONTINUITY_SAMPLE_HZ).floor() as u32).max(1),
    )
}

fn contact_driven_guard_support_tick_budget(
    mut support_proof: GuardHipPathProof,
    support_target: Vec3,
) -> u32 {
    const MAXIMUM_CONTACT_TICKS: u32 = 64;
    support_proof.contact_tick = support_proof
        .start_tick
        .saturating_add(u64::from(MAXIMUM_CONTACT_TICKS));
    (1..=MAXIMUM_CONTACT_TICKS)
        .take_while(|tick| {
            support_proof.permits(
                support_target,
                support_proof.start_tick.saturating_add(u64::from(*tick)),
                false,
            )
        })
        .last()
        .unwrap_or(1)
}

#[allow(clippy::too_many_arguments)]
fn plan_contact_driven_guard_segment(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    requested_endpoint: Vec3,
    maximum_ticks: u32,
    proof: Option<GuardHipPathProof>,
    terrain: Option<&SceneTerrain>,
    mut candidate_at_progress: impl FnMut(f32) -> Option<Vec3>,
) -> GuardFootEndpointPlan {
    let Some(proof) = proof else {
        return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake {
            presented,
        });
    };
    let mut best = None;
    let mut best_distance = 0.0;
    for ticks in 1..=maximum_ticks.clamp(1, 64) {
        let timing = SegmentTickSpan::new(ticks).expect("guard contact ticks are nonzero");
        // Physical contact owns this gait. The proof supplied by the raised
        // pose pass is initially sized to the replicated cadence remainder,
        // which can be only a few ticks after a sparse phase update. Rebuild
        // its terminal tick for each candidate duration so a lawful contact
        // is not rejected merely because it outlives that animation hint.
        let mut contact_proof = proof;
        contact_proof.contact_tick = contact_proof.start_tick.saturating_add(u64::from(ticks));
        let planned = plan_guard_c2_contact_segment(
            presented,
            presented_velocity,
            presented_acceleration,
            timing,
            Some(contact_proof),
            &mut candidate_at_progress,
        );
        let segment = match planned {
            GuardFootEndpointPlan::Segment(segment)
                if guard_contact_has_terminal_support_reserve(contact_proof, segment) =>
            {
                Some(segment)
            }
            _ => plan_guard_recovery_contact_segment(
                presented,
                presented_velocity,
                presented_acceleration,
                timing,
                Some(contact_proof),
                terrain,
            ),
        };
        let Some(segment) = segment else { continue };
        let distance = segment.end.position().distance(presented);
        if distance > best_distance {
            best_distance = distance;
            best = Some(segment);
        }
        if segment.end.position().distance(requested_endpoint) <= 0.01 {
            break;
        }
    }
    if let Some(segment) = best {
        return GuardFootEndpointPlan::Segment(segment);
    }
    // A former support can be just outside warning reach on the first tick
    // after the opposite foot lands. That state is still inside the analytic
    // hard workspace, but a Contact proof cannot legally start there. Retain
    // a typed C2 owner that moves inward under the hard envelope; its terminal
    // sample is not promoted as contact and the next tick replans terrain.
    for ticks in 1..=maximum_ticks.clamp(1, 64) {
        let timing = SegmentTickSpan::new(ticks).expect("guard recovery ticks are nonzero");
        let mut release_proof = proof;
        release_proof.contact_tick = release_proof.start_tick.saturating_add(u64::from(ticks));
        if let GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::Segment(segment)) =
            plan_exact_guard_release(
                presented,
                presented_velocity,
                presented_acceleration,
                timing,
                release_proof,
            )
        {
            return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::Segment(
                segment,
            ));
        }
    }
    GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake { presented })
}

fn guard_contact_has_terminal_support_reserve(
    proof: GuardHipPathProof,
    segment: PlannedGuardFootSegment,
) -> bool {
    proof.sample(proof.contact_tick).is_some_and(|hip| {
        segment.end.position().distance(hip) <= proof.warning_reach * 0.97 + 0.0001
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_guard_c2_contact_segment(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    timing: SegmentTickSpan,
    proof: Option<GuardHipPathProof>,
    mut candidate_at_progress: impl FnMut(f32) -> Option<Vec3>,
) -> GuardFootEndpointPlan {
    let Some(mut proof) = proof else {
        return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake {
            presented,
        });
    };
    proof.contact_tick = proof
        .start_tick
        .saturating_add(u64::from(timing.total_ticks.get()));
    let exact_hip_path = (proof.start_tick..=proof.contact_tick)
        .filter_map(|tick| {
            proof
                .sample(tick)
                .map(|hip| (hip, proof.warning_reach, proof.hard_reach))
        })
        .collect::<Vec<_>>();
    if exact_hip_path.len() != timing.total_ticks.get() as usize + 1 {
        return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake {
            presented,
        });
    }
    let contact_seconds = timing.duration_seconds();
    let try_candidate = |requested_endpoint: Vec3| {
        let endpoint = exact_hip_path.last().copied()?;
        if requested_endpoint.distance(endpoint.0) > endpoint.1 + 0.0001 {
            return None;
        }
        let contact = FeasibleFootEndpoint::from_proven_guard_contact(requested_endpoint);
        let segment = C2FootSegment {
            start: presented,
            start_velocity: presented_velocity,
            start_acceleration: presented_acceleration,
            end: FootSegmentEndpoint::Contact(contact),
            timing,
            owner_epoch: 0,
        };
        (minimum_c2_segment_duration(
            presented,
            presented_velocity,
            presented_acceleration,
            requested_endpoint,
        ) <= contact_seconds
            && c2_segment_dynamics_are_bounded(
                segment,
                GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
                GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
            )
            && contact_error_envelope_is_proven(segment)
            && guard_exact_hip_path_contains_segment(&exact_hip_path, segment))
        .then_some(segment)
    };

    // The authored guard stride may be farther than a rest-to-rest ankle can
    // travel under the route's 24/384 dynamics contract before the replicated
    // cadence edge. Select the greatest semantic progress that remains a
    // terrain/corridor contact and passes the same whole-path proofs. A
    // zero-motion hold is deliberately not a successful cadence contact.
    let mut best_progress = 0.0_f32;
    let mut best = None;
    // Reach against a moving hip and terrain-conformed height need not be
    // monotone in semantic progress. Scan from the requested endpoint toward
    // the presented pose, then refine only the highest feasible bracket.
    const COARSE_STEPS: u32 = 64;
    for step in (1..=COARSE_STEPS).rev() {
        let progress = step as f32 / COARSE_STEPS as f32;
        let candidate = candidate_at_progress(progress);
        let segment = candidate.and_then(try_candidate);
        if let Some(segment) =
            segment.filter(|segment| segment.end.position().distance_squared(presented) > 0.000_001)
        {
            best_progress = progress;
            best = Some(segment);
            break;
        }
    }
    if best.is_some() && best_progress < 1.0 {
        let mut low = best_progress;
        let mut high = (best_progress + 1.0 / COARSE_STEPS as f32).min(1.0);
        for _ in 0..12 {
            let progress = (low + high) * 0.5;
            if let Some(segment) = candidate_at_progress(progress).and_then(try_candidate) {
                low = progress;
                best = Some(segment);
            } else {
                high = progress;
            }
        }
    }
    if let Some(segment) = best {
        return GuardFootEndpointPlan::Segment(PlannedGuardFootSegment {
            motion: segment,
            reach: GuardSegmentReachProof::Exact(proof),
            recovery_to_contact: false,
        });
    }

    plan_exact_guard_release(
        presented,
        presented_velocity,
        presented_acceleration,
        timing,
        proof,
    )
}

fn plan_guard_recovery_contact_segment(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    timing: SegmentTickSpan,
    proof: Option<GuardHipPathProof>,
    terrain: Option<&SceneTerrain>,
) -> Option<PlannedGuardFootSegment> {
    let proof = proof?;
    let contact_tick = proof
        .start_tick
        .saturating_add(u64::from(timing.total_ticks.get()));
    let final_hip = proof.sample(contact_tick)?;
    // This is the morphology-independent safety target: the closest point on
    // the ground toward the predicted hip that remains inside the warning
    // workspace. It is still a real terrain contact with the same C2,
    // dynamics, and whole-path proof as an ideal cadence endpoint.
    let endpoint =
        ground_safety_slide_endpoint(presented, final_hip, proof.warning_reach * 0.97, terrain);
    match plan_guard_c2_contact_segment(
        presented,
        presented_velocity,
        presented_acceleration,
        timing,
        Some(proof),
        |_| Some(endpoint),
    ) {
        GuardFootEndpointPlan::Segment(mut segment) => {
            segment.recovery_to_contact = proof
                .sample(proof.start_tick)
                .is_some_and(|hip| presented.distance(hip) > proof.warning_reach + 0.0001);
            Some(segment)
        }
        GuardFootEndpointPlan::MustReleaseOrReplan(_) => None,
    }
}

fn plan_guard_contact_recovery_without_cadence_deadline(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    requested_endpoint: Vec3,
    proof: Option<GuardHipPathProof>,
    terrain: Option<&SceneTerrain>,
) -> Option<PlannedGuardFootSegment> {
    let proof = proof?;
    // A contact recovery has priority over cadence timing. Select the shortest
    // dynamics-proven stop-to-ground duration instead of converting the foot
    // to an airborne Release owner merely because an old movement deadline
    // has expired. Callers defer the cadence identity until this owner lands.
    for ticks in 1..=64 {
        let timing = SegmentTickSpan::new(ticks).expect("stationary contact ticks are nonzero");
        if let GuardFootEndpointPlan::Segment(segment) = plan_guard_c2_contact_segment(
            presented,
            presented_velocity,
            presented_acceleration,
            timing,
            Some(proof),
            |_| Some(requested_endpoint),
        ) && segment.end.is_contact()
        {
            return Some(segment);
        }
        if let Some(segment) = plan_guard_recovery_contact_segment(
            presented,
            presented_velocity,
            presented_acceleration,
            timing,
            Some(proof),
            terrain,
        ) && segment.end.is_contact()
        {
            return Some(segment);
        }
    }
    None
}

fn plan_exact_guard_release(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    timing: SegmentTickSpan,
    proof: GuardHipPathProof,
) -> GuardFootEndpointPlan {
    let Some(final_hip) = proof.sample(proof.contact_tick) else {
        return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake {
            presented,
        });
    };
    let radial = (presented - final_hip)
        .try_normalize()
        .unwrap_or(Vec3::NEG_Y);
    let recovery = final_hip + radial * (proof.hard_reach * 0.92);
    for step in (1..=64).rev() {
        let endpoint = presented.lerp(recovery, step as f32 / 64.0);
        let motion = C2FootSegment {
            start: presented,
            start_velocity: presented_velocity,
            start_acceleration: presented_acceleration,
            end: FootSegmentEndpoint::Release(FeasibleReleaseEndpoint::from_proven_guard_release(
                endpoint,
            )),
            timing,
            owner_epoch: 0,
        };
        if c2_segment_dynamics_are_bounded(
            motion,
            GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
            GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
        ) && guard_exact_hip_path_contains_segment_with_limit(proof, motion, true)
        {
            return GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::Segment(
                PlannedGuardFootSegment {
                    motion,
                    reach: GuardSegmentReachProof::Exact(proof),
                    recovery_to_contact: false,
                },
            ));
        }
    }
    GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::EmergencyBrake { presented })
}

fn guard_exact_hip_path_contains_segment_with_limit(
    proof: GuardHipPathProof,
    segment: C2FootSegment,
    hard: bool,
) -> bool {
    let total_ticks = segment.timing.total_ticks.get();
    (0..=total_ticks).all(|tick| {
        let sample =
            guard_swing_replan_sample_at_progress(segment, tick as f32 / total_ticks as f32);
        proof.permits(
            sample.position,
            proof.start_tick.saturating_add(u64::from(tick)),
            hard,
        )
    })
}

fn guard_exact_hip_path_contains_segment(
    exact_hip_path: &[(Vec3, f32, f32)],
    segment: C2FootSegment,
) -> bool {
    let total_ticks = segment.timing.total_ticks.get();
    exact_hip_path.len() == total_ticks as usize + 1
        && exact_hip_path
            .iter()
            .enumerate()
            .all(|(tick, (hip, warning, hard))| {
                let sample = guard_swing_replan_sample_at_progress(
                    segment,
                    tick as f32 / total_ticks as f32,
                );
                // A foot inherited from the previous plant can begin just
                // outside the conservative warning reserve. That is a valid
                // swing state as long as it never crosses hard anatomical
                // reach and arrives back inside warning reach at contact.
                let limit = if tick == total_ticks as usize {
                    *warning
                } else {
                    *hard
                };
                hip.is_finite()
                    && warning.is_finite()
                    && hard.is_finite()
                    && sample.position.distance(*hip) <= limit + 0.0001
            })
}

fn try_plan_c2_contact_segment(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    requested_endpoint: Vec3,
    timing: SegmentTickSpan,
    trajectory: PredictedHipTrajectory,
) -> Option<C2FootSegment> {
    let contact_seconds = timing.duration_seconds();
    let contact = FeasibleFootEndpoint::for_predicted_terrain_contact(
        requested_endpoint,
        trajectory,
        contact_seconds,
    )?;
    let required_seconds = minimum_c2_segment_duration(
        presented,
        presented_velocity,
        presented_acceleration,
        requested_endpoint,
    );
    if required_seconds > contact_seconds {
        return None;
    }
    let segment = C2FootSegment {
        start: presented,
        start_velocity: presented_velocity,
        start_acceleration: presented_acceleration,
        end: FootSegmentEndpoint::Contact(contact),
        timing,
        owner_epoch: 0,
    };
    (c2_segment_dynamics_are_bounded(
        segment,
        GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
        GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
    ) && contact_error_envelope_is_proven(segment)
        && trajectory.contains_quintic_path(
            c2_segment_position_control(segment),
            contact_seconds,
            false,
        ))
    .then_some(segment)
}

#[allow(clippy::too_many_arguments)]
fn plan_c2_foot_segment(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    requested_endpoint: Vec3,
    contact_seconds: f32,
    trajectory: Option<PredictedHipTrajectory>,
) -> FootEndpointPlan {
    let Some(trajectory) = trajectory else {
        return release_plan_or_hold(None, presented);
    };
    let Some(timing) = contact_tick_span(contact_seconds) else {
        return release_plan_or_hold(
            plan_c2_release_segment(
                presented,
                presented_velocity,
                presented_acceleration,
                GUARD_FORCED_RELEASE_SECONDS,
                trajectory,
            ),
            presented,
        );
    };
    let contact_seconds = timing.duration_seconds();
    let required_seconds = minimum_c2_segment_duration(
        presented,
        presented_velocity,
        presented_acceleration,
        requested_endpoint,
    );
    if required_seconds > contact_seconds {
        return release_plan_or_hold(
            plan_c2_release_segment(
                presented,
                presented_velocity,
                presented_acceleration,
                contact_seconds.max(required_seconds),
                trajectory,
            ),
            presented,
        );
    }
    let Some(segment) = try_plan_c2_contact_segment(
        presented,
        presented_velocity,
        presented_acceleration,
        requested_endpoint,
        timing,
        trajectory,
    ) else {
        return release_plan_or_hold(
            plan_c2_release_segment(
                presented,
                presented_velocity,
                presented_acceleration,
                contact_seconds,
                trajectory,
            ),
            presented,
        );
    };
    FootEndpointPlan::Segment(segment)
}

fn release_plan_or_hold(
    release_segment: Option<C2FootSegment>,
    presented: Vec3,
) -> FootEndpointPlan {
    FootEndpointPlan::MustReleaseOrReplan(match release_segment {
        Some(segment) => FootReleasePlan::Segment(segment),
        None => FootReleasePlan::EmergencyBrake { presented },
    })
}

fn minimum_c2_segment_duration(
    start: Vec3,
    start_velocity: Vec3,
    start_acceleration: Vec3,
    end: Vec3,
) -> f32 {
    let distance = start.distance(end);
    let acceleration_seconds = (5.8 * distance / GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION)
        .sqrt()
        .max(start_velocity.length() / GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION);
    let jerk_seconds = (60.0 * distance / GUARD_ACTION_SEGMENT_MAXIMUM_JERK)
        .cbrt()
        .max(start_acceleration.length() / GUARD_ACTION_SEGMENT_MAXIMUM_JERK);
    acceleration_seconds.max(jerk_seconds)
}

fn plan_c2_release_segment(
    presented: Vec3,
    presented_velocity: Vec3,
    presented_acceleration: Vec3,
    minimum_seconds: f32,
    trajectory: PredictedHipTrajectory,
) -> Option<C2FootSegment> {
    let mut duration_seconds =
        quantized_release_duration(minimum_seconds.max(GUARD_FORCED_RELEASE_SECONDS));
    for _ in 0..8 {
        let endpoint = trajectory
            .recovery_target_at(presented, duration_seconds)
            .and_then(|target| {
                FeasibleReleaseEndpoint::for_predicted_release(target, trajectory, duration_seconds)
            })?;
        let required_seconds = minimum_c2_segment_duration(
            presented,
            presented_velocity,
            presented_acceleration,
            endpoint.position(),
        );
        if required_seconds > duration_seconds {
            duration_seconds = quantized_release_duration(required_seconds);
            continue;
        }
        let segment = C2FootSegment {
            start: presented,
            start_velocity: presented_velocity,
            start_acceleration: presented_acceleration,
            end: FootSegmentEndpoint::Release(endpoint),
            timing: release_tick_span(duration_seconds)?,
            owner_epoch: 0,
        };
        if c2_segment_dynamics_are_bounded(
            segment,
            GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
            GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
        ) && trajectory.contains_quintic_path(
            c2_segment_position_control(segment),
            duration_seconds,
            true,
        ) {
            return Some(segment);
        }
        duration_seconds = quantized_release_duration(duration_seconds * 1.25);
    }
    None
}

#[derive(Debug, Clone, Copy)]
struct GuardTargetTrack {
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    ideal_velocity: Vec3,
    ideal_acceleration: Vec3,
    ideal_history_valid: bool,
    replan: Option<(FootFollowReason, Vec3)>,
}

#[allow(clippy::too_many_arguments)]
fn advance_runtime_foot_target(
    memory: &mut LegIkMemory,
    left: bool,
    presented_position: Vec3,
    ideal: IdealFootTarget,
    initial_ideal_velocity: Vec3,
    reach: Option<FootReachEnvelope>,
    contact_deadline_seconds: Option<f32>,
    delta_seconds: f32,
    advances: bool,
) -> (Vec3, Option<(FootFollowReason, Vec3)>) {
    let retained = if left {
        memory.left_foot_follower
    } else {
        memory.right_foot_follower
    }
    // The caller's presented position is the ownership boundary. A follower
    // retained by the preceding semantic owner must not replace a different
    // ankle that was actually rendered on the handoff sample.
    .filter(|state| state.position.distance_squared(presented_position) <= 0.000001);
    if !advances || delta_seconds <= f32::EPSILON {
        return (
            retained.map_or(presented_position, |state| state.position),
            None,
        );
    }
    let requested_sample = ideal.world();
    let current = retained.or_else(|| {
        FootFollowerState::from_presented_pose(
            presented_position,
            Vec3::ZERO,
            Vec3::ZERO,
            requested_sample.position() - initial_ideal_velocity * delta_seconds,
            initial_ideal_velocity,
            Vec3::ZERO,
        )
    });
    let Some(current) = current else {
        return (
            presented_position,
            Some((FootFollowReason::InvalidInput, presented_position)),
        );
    };
    let ideal = match ideal {
        IdealFootTarget::WorldPlant { .. } => ideal,
        IdealFootTarget::WorldSwing(_) => {
            let velocity = retained
                .map(|state| (requested_sample.position() - state.previous_ideal) / delta_seconds)
                .unwrap_or(initial_ideal_velocity);
            let acceleration = retained
                .map(|state| (velocity - state.previous_ideal_velocity) / delta_seconds)
                .unwrap_or(requested_sample.acceleration());
            let Some(sample) =
                WorldFootTargetSample::new(requested_sample.position(), velocity, acceleration)
            else {
                return (
                    presented_position,
                    Some((FootFollowReason::InvalidInput, presented_position)),
                );
            };
            IdealFootTarget::WorldSwing(sample)
        }
    };
    let outcome = advance_foot_follower(
        current,
        ideal,
        FootFollowerLimits::animation(reach, contact_deadline_seconds),
        delta_seconds,
    );
    let (presented, reason) = match outcome {
        FootFollowOutcome::Tracking(state) => (state, None),
        FootFollowOutcome::NeedsReleaseOrReplan {
            presented_state,
            reason,
            suggested_semantic_target,
        } => (presented_state, Some((reason, suggested_semantic_target))),
        FootFollowOutcome::Invalid(reason) => (current, Some((reason, current.position))),
    };
    let reset_history = matches!(
        reason,
        Some((
            FootFollowReason::DiscontinuousTarget | FootFollowReason::InvalidInput,
            _
        ))
    );
    if left {
        memory.left_foot_follower = (!reset_history).then_some(presented);
    } else {
        memory.right_foot_follower = (!reset_history).then_some(presented);
    }
    (presented.position, reason)
}

#[allow(clippy::too_many_arguments)]
fn run_contact_follower_plan(
    memory: &LegIkMemory,
    left: bool,
    presented_position: Vec3,
    contact_target: Vec3,
    hip: Vec3,
    next_hip: Vec3,
    upper_length: f32,
    lower_length: f32,
    deadline_seconds: f32,
    delta_seconds: f32,
) -> RunContactFollowerPlan {
    let mut preview = *memory;
    let warning_reach = (upper_length * upper_length
        + lower_length * lower_length
        + 2.0 * upper_length * lower_length * 30.0_f32.to_radians().cos())
    .sqrt();
    let Some(reach) = FootReachEnvelope::new(
        hip,
        next_hip,
        warning_reach,
        maximum_reach(upper_length, lower_length),
    ) else {
        return RunContactFollowerPlan::invalid();
    };
    let Some(ideal) = IdealFootTarget::world_plant(contact_target) else {
        return RunContactFollowerPlan::invalid();
    };
    let (_, reason) = advance_runtime_foot_target(
        &mut preview,
        left,
        presented_position,
        ideal,
        Vec3::ZERO,
        Some(reach),
        Some(deadline_seconds.max(delta_seconds)),
        delta_seconds,
        true,
    );
    RunContactFollowerPlan {
        feasible: reason.is_none(),
        suggested_target: reason.map(|(_, suggested)| suggested),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RunContactFollowerPlan {
    feasible: bool,
    suggested_target: Option<Vec3>,
}

impl RunContactFollowerPlan {
    const fn invalid() -> Self {
        Self {
            feasible: false,
            suggested_target: None,
        }
    }
}

fn advance_guard_foot_target(
    previous: Option<Vec3>,
    velocity: Vec3,
    acceleration: Vec3,
    previous_desired: Option<Vec3>,
    previous_ideal_velocity: Vec3,
    previous_ideal_acceleration: Vec3,
    ideal_history_valid: bool,
    desired: Vec3,
    delta_seconds: f32,
    advances: bool,
) -> GuardTargetTrack {
    advance_guard_foot_target_with_reach(
        previous,
        velocity,
        acceleration,
        previous_desired,
        previous_ideal_velocity,
        previous_ideal_acceleration,
        ideal_history_valid,
        desired,
        delta_seconds,
        advances,
        None,
    )
}

fn advance_guard_foot_target_with_reach(
    previous: Option<Vec3>,
    velocity: Vec3,
    acceleration: Vec3,
    previous_desired: Option<Vec3>,
    previous_ideal_velocity: Vec3,
    previous_ideal_acceleration: Vec3,
    ideal_history_valid: bool,
    desired: Vec3,
    delta_seconds: f32,
    advances: bool,
    reach: Option<FootReachEnvelope>,
) -> GuardTargetTrack {
    advance_guard_foot_target_sample_with_reach(
        previous,
        velocity,
        acceleration,
        previous_desired,
        previous_ideal_velocity,
        previous_ideal_acceleration,
        ideal_history_valid,
        desired,
        None,
        delta_seconds,
        advances,
        reach,
    )
}

fn direct_c2_guard_target_sample(
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
) -> GuardTargetTrack {
    GuardTargetTrack {
        position,
        velocity,
        acceleration,
        ideal_velocity: velocity,
        ideal_acceleration: acceleration,
        ideal_history_valid: true,
        replan: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn advance_guard_foot_target_sample_with_reach(
    previous: Option<Vec3>,
    velocity: Vec3,
    acceleration: Vec3,
    previous_desired: Option<Vec3>,
    previous_ideal_velocity: Vec3,
    previous_ideal_acceleration: Vec3,
    ideal_history_valid: bool,
    desired: Vec3,
    explicit_ideal_motion: Option<(Vec3, Vec3)>,
    delta_seconds: f32,
    advances: bool,
    reach: Option<FootReachEnvelope>,
) -> GuardTargetTrack {
    let Some(position) = previous.filter(|position| position.is_finite()) else {
        return GuardTargetTrack {
            position: desired,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            ideal_velocity: Vec3::ZERO,
            ideal_acceleration: Vec3::ZERO,
            ideal_history_valid: false,
            replan: None,
        };
    };
    if !advances || delta_seconds <= f32::EPSILON || !desired.is_finite() {
        return GuardTargetTrack {
            position,
            velocity,
            acceleration,
            ideal_velocity: previous_ideal_velocity,
            ideal_acceleration: previous_ideal_acceleration,
            ideal_history_valid,
            replan: None,
        };
    }
    let dt = delta_seconds;
    let (ideal_velocity, ideal_acceleration) = explicit_ideal_motion.unwrap_or_else(|| {
        let ideal_velocity = previous_desired
            .map(|previous| (desired - previous) / dt)
            .filter(|velocity| velocity.is_finite())
            .unwrap_or_default();
        let ideal_acceleration = (ideal_velocity - previous_ideal_velocity) / dt;
        let ideal_acceleration = ideal_acceleration
            .is_finite()
            .then_some(ideal_acceleration)
            .unwrap_or_default();
        (ideal_velocity, ideal_acceleration)
    });
    if !ideal_history_valid {
        return GuardTargetTrack {
            position,
            velocity,
            acceleration,
            ideal_velocity,
            ideal_acceleration,
            ideal_history_valid: true,
            replan: None,
        };
    }
    let Some(current) = FootFollowerState::from_presented_pose(
        position,
        velocity,
        acceleration,
        previous_desired.unwrap_or(position),
        previous_ideal_velocity,
        previous_ideal_acceleration,
    ) else {
        return GuardTargetTrack {
            position,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            ideal_velocity,
            ideal_acceleration,
            ideal_history_valid: false,
            replan: Some((FootFollowReason::DiscontinuousTarget, position)),
        };
    };
    let Some(sample) = WorldFootTargetSample::new(desired, ideal_velocity, ideal_acceleration)
    else {
        return GuardTargetTrack {
            position,
            velocity,
            acceleration,
            ideal_velocity,
            ideal_acceleration,
            ideal_history_valid: false,
            replan: Some((FootFollowReason::DiscontinuousTarget, position)),
        };
    };
    let outcome = advance_foot_follower(
        current,
        IdealFootTarget::WorldSwing(sample),
        FootFollowerLimits::animation(reach, None),
        dt,
    );
    let (state, replan) = match outcome {
        FootFollowOutcome::Tracking(state) => (state, None),
        FootFollowOutcome::NeedsReleaseOrReplan {
            presented_state,
            reason,
            suggested_semantic_target,
        } => (presented_state, Some((reason, suggested_semantic_target))),
        FootFollowOutcome::Invalid(reason) => (current, Some((reason, current.position))),
    };
    let reset_ideal_history = replan.is_some_and(|(reason, _)| {
        matches!(
            reason,
            FootFollowReason::DiscontinuousTarget | FootFollowReason::InvalidInput
        )
    });
    // A semantic replan may deliberately retain the last safe position for
    // this sample. Retaining its pre-replan velocity/acceleration alongside a
    // frozen position is not a valid physical state: the next accepted sample
    // integrates that hidden motion as a one-tick burst.
    let frozen_for_replan =
        replan.is_some() && state.position.distance_squared(current.position) <= f32::EPSILON;
    GuardTargetTrack {
        position: state.position,
        velocity: if frozen_for_replan {
            Vec3::ZERO
        } else {
            state.velocity
        },
        acceleration: if frozen_for_replan {
            Vec3::ZERO
        } else {
            state.acceleration
        },
        ideal_velocity: if reset_ideal_history {
            Vec3::ZERO
        } else {
            ideal_velocity
        },
        ideal_acceleration: if reset_ideal_history {
            Vec3::ZERO
        } else {
            ideal_acceleration
        },
        ideal_history_valid: !reset_ideal_history,
        replan,
    }
}

fn foot_follow_reach_envelope(
    upper: Entity,
    lower: Entity,
    foot: Entity,
    predicted_root_delta: Vec3,
    parents: &Query<&ChildOf>,
    helper: &TransformHelper,
) -> Option<FootReachEnvelope> {
    let (upper, lower, foot) = snapshot_chain(upper, lower, foot, parents, helper)?;
    let root = upper.global.translation();
    let upper_length = root.distance(lower.global.translation());
    let lower_length = lower
        .global
        .translation()
        .distance(foot.global.translation());
    let reach_at_flexion = |angle: f32| {
        (upper_length * upper_length
            + lower_length * lower_length
            + 2.0 * upper_length * lower_length * angle.cos())
        .sqrt()
    };
    FootReachEnvelope::new(
        root,
        root + predicted_root_delta,
        reach_at_flexion(30.0_f32.to_radians()),
        reach_at_flexion(MIN_KNEE_FLEXION),
    )
}

fn retain_guard_tracker_on_reinitialization(
    initialized: bool,
    lead_changed: bool,
    discontinuous: bool,
    skipped_handoff: bool,
) -> bool {
    initialized && lead_changed && !discontinuous && !skipped_handoff
}

fn quintic_progress(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * progress * (progress * (progress * 6.0 - 15.0) + 10.0)
}

/// Keeps a leg's authored bend plane attached to the hip-to-foot direction.
///
/// Overgrowth's leg solve rotates the animated knee, ankle, and foot together
/// when the IK target moves, which transports the authored knee plane instead
/// of selecting a fresh world-space pole every frame. Our analytic solver does
/// the equivalent explicitly: parallel-transport the last rendered bend, fall
/// back to the current authored bend, and reject either if it crosses the
/// anatomical hemisphere. The canonical pole is only the final singularity
/// fallback.
fn stabilized_knee_pole(
    remembered_bend: Option<Vec3>,
    previous_end_direction: Option<Vec3>,
    hip: Vec3,
    authored_knee: Vec3,
    target: Vec3,
    canonical_world: Vec3,
    foot_facing: Option<Vec3>,
) -> Option<Vec3> {
    let next_end_direction = (target - hip).try_normalize()?;
    let canonical_bend = canonical_world
        .reject_from_normalized(next_end_direction)
        .try_normalize()
        .or_else(|| canonical_world.try_normalize())?;
    let in_anatomical_hemisphere = |bend: Vec3| {
        let bend = bend
            .reject_from_normalized(next_end_direction)
            .try_normalize()?;
        let alignment = bend.dot(canonical_bend);
        if alignment >= 0.05 {
            Some(bend)
        } else {
            // Correct continuously at the boundary instead of discarding the
            // remembered pole and selecting an unrelated fallback next tick.
            (bend + canonical_bend * (0.05 - alignment)).try_normalize()
        }
    };

    let transported = remembered_bend
        .and_then(|bend| {
            let bend = bend.try_normalize()?;
            previous_end_direction.map_or(Some(bend), |previous| {
                let previous = previous.try_normalize()?;
                (Quat::from_rotation_arc(previous, next_end_direction) * bend).try_normalize()
            })
        })
        .and_then(in_anatomical_hemisphere);
    let authored = (authored_knee - hip)
        .reject_from_normalized(next_end_direction)
        .try_normalize()
        .and_then(in_anatomical_hemisphere);

    let selected = transported.or(authored).unwrap_or(canonical_bend);
    foot_facing
        .and_then(|facing| {
            constrain_knee_pole_to_foot_facing(
                selected,
                next_end_direction,
                facing,
                KNEE_POLE_MAX_FOOT_FACING_OFFSET_RADIANS,
            )
        })
        .or(Some(selected))
}

fn constrain_knee_pole_to_foot_facing(
    pole: Vec3,
    leg_direction: Vec3,
    foot_facing: Vec3,
    maximum_offset_radians: f32,
) -> Option<Vec3> {
    let leg_direction = leg_direction.try_normalize()?;
    let transported_pole = pole.reject_from_normalized(leg_direction).try_normalize()?;
    let facing_yaw = foot_facing.xz().try_normalize()?;
    let pole_yaw = pole.xz().try_normalize().unwrap_or(facing_yaw);
    let signed_offset = facing_yaw
        .perp_dot(pole_yaw)
        .atan2(facing_yaw.dot(pole_yaw));
    let clamped_offset = smooth_cone_limit(signed_offset, maximum_offset_radians);
    let (sin, cos) = clamped_offset.sin_cos();
    let clamped_yaw = Vec2::new(
        facing_yaw.x * cos - facing_yaw.y * sin,
        facing_yaw.x * sin + facing_yaw.y * cos,
    );

    // Preserve the clamped ground-plane yaw exactly, then choose the vertical
    // component that makes the pole perpendicular to the hip-to-foot axis.
    // Clamping only after projecting into that plane does not bound yaw for a
    // diagonal leg and was the source of visibly sideways knees.
    let cone_authority = quintic_progress(
        (leg_direction.y.abs() - KNEE_FACING_HORIZONTAL_AUTHORITY_START)
            / (KNEE_FACING_HORIZONTAL_AUTHORITY_FULL - KNEE_FACING_HORIZONTAL_AUTHORITY_START),
    );
    if cone_authority <= f32::EPSILON {
        return Some(transported_pole);
    }
    let vertical = -clamped_yaw.dot(leg_direction.xz()) / leg_direction.y;
    let constrained = Vec3::new(clamped_yaw.x, vertical, clamped_yaw.y).try_normalize()?;
    Some(interpolate_anatomical_pole_signed(
        transported_pole,
        constrained,
        leg_direction,
        cone_authority,
    ))
}

fn smooth_cone_limit(value: f32, maximum: f32) -> f32 {
    if maximum <= f32::EPSILON {
        0.0
    } else {
        const IDENTITY_FRACTION: f32 = 0.8;
        let magnitude = value.abs();
        let identity_limit = maximum * IDENTITY_FRACTION;
        if magnitude <= identity_limit {
            value
        } else {
            let shoulder = maximum - identity_limit;
            value.signum()
                * (identity_limit + shoulder * ((magnitude - identity_limit) / shoulder).tanh())
        }
    }
}

/// Applies the anatomical knee-yaw invariant at the final leg-solve boundary.
///
/// Individual pose owners may transport, preserve, or reconstruct their pole
/// differently, but every valid humanoid leg has the same hard constraint:
/// its effective pole stays within the foot-facing cone. Keeping this wrapper
/// beside the raw constraint prevents ordinary terrain and landing paths from
/// bypassing the combat-specific stabilizer.
pub(super) fn constrain_rendered_leg_pole(
    rig: &HumanoidRig,
    left: bool,
    hip: Vec3,
    foot_position: Vec3,
    target: Vec3,
    pole: Vec3,
    parents: &Query<&ChildOf>,
    transforms: &TransformHelper,
) -> Vec3 {
    rendered_foot_facing(rig, left, foot_position, parents, transforms)
        .and_then(|facing| {
            constrain_knee_pole_to_foot_facing(
                pole,
                target - hip,
                facing,
                KNEE_POLE_MAX_FOOT_FACING_OFFSET_RADIANS,
            )
        })
        // Sparse/non-humanoid rigs may not expose a toe direction. Preserve
        // their previous graceful fallback; canonical humanoids always take
        // the constrained branch.
        .unwrap_or(pole)
}

fn rendered_foot_facing(
    rig: &HumanoidRig,
    left: bool,
    foot_position: Vec3,
    parents: &Query<&ChildOf>,
    transforms: &TransformHelper,
) -> Option<Vec3> {
    let foot = *rig.get(if left {
        &BoneRole::FootLeft
    } else {
        &BoneRole::FootRight
    })?;
    let toe = *rig.get(if left {
        &BoneRole::ToeLeft
    } else {
        &BoneRole::ToeRight
    })?;
    let foot_rotation = snapshot(foot, parents, transforms)?.global.rotation();
    let toe_position = snapshot(toe, parents, transforms)?.global.translation();
    let toe_direction = (toe_position - foot_position).try_normalize()?;

    // Toe-to-ankle projected directly onto the ground reverses yaw when a
    // running or slope-aligned foot pitches through vertical. Recover yaw
    // from the pitch-stable lateral axis instead: forward cross sole-up gives
    // anatomical right, and world-up cross right gives horizontal forward.
    // This preserves the direction the foot is facing even at heel/toe roll.
    if let Some(sole_up) = rig
        .sole_axis(left)
        .map(|axis| foot_rotation * axis)
        .and_then(Vec3::try_normalize)
        && let Some(lateral) = toe_direction.cross(sole_up).try_normalize()
        && let Some(facing) = Vec3::Y.cross(lateral).xz().try_normalize()
    {
        return Some(Vec3::new(facing.x, 0.0, facing.y));
    }

    toe_direction
        .xz()
        .try_normalize()
        .map(|facing| Vec3::new(facing.x, 0.0, facing.y))
}

fn projected_body_center(rig: &HumanoidRig, transforms: &TransformHelper) -> Option<Vec3> {
    let mut weighted = Vec3::ZERO;
    let mut total = 0.0;
    for (role, weight) in [
        (BoneRole::Pelvis, 0.45),
        (BoneRole::Chest, 0.35),
        (BoneRole::Head, 0.20),
    ] {
        let Some(&bone) = rig.get(&role) else {
            continue;
        };
        let Ok(global) = transforms.compute_global_transform(bone) else {
            continue;
        };
        weighted += global.translation() * weight;
        total += weight;
    }
    (total > 0.0).then_some(weighted / total)
}

fn settle_stance_is_safe(
    projected_com: Vec3,
    left_foot: Option<Vec3>,
    right_foot: Option<Vec3>,
    terrain: &SceneTerrain,
) -> bool {
    let (Some(left), Some(right)) = (left_foot, right_foot) else {
        return false;
    };
    let at_contact = |foot: Vec3| {
        terrain
            .height_at(foot.xz())
            .is_some_and(|height| sole_is_at_contact(foot.y, height))
    };
    if !at_contact(left) || !at_contact(right) {
        return false;
    }
    let segment = right.xz() - left.xz();
    let progress = if segment.length_squared() > 0.000001 {
        ((projected_com.xz() - left.xz()).dot(segment) / segment.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    projected_com.xz().distance(left.xz() + segment * progress) <= 0.18
}

pub(super) fn sole_is_at_contact(ankle_y: f32, terrain_height: f32) -> bool {
    (ankle_y - terrain_height - MEASURED_ANKLE_SOLE_OFFSET_METRES).abs()
        <= SOLE_CONTACT_TOLERANCE_METRES
}

fn raised_support_is_actual(
    nominal_support: bool,
    previously_supported: bool,
    ankle_y: f32,
    terrain_height: f32,
) -> bool {
    let tolerance = SOLE_CONTACT_TOLERANCE_METRES
        + if previously_supported {
            RAISED_SUPPORT_RETENTION_HYSTERESIS_METRES
        } else {
            0.0
        };
    nominal_support
        && (ankle_y - terrain_height - MEASURED_ANKLE_SOLE_OFFSET_METRES).abs() <= tolerance
}

fn raised_support_has_toe_clearance(
    ankle_support: bool,
    toe_y: Option<f32>,
    toe_terrain_height: Option<f32>,
) -> bool {
    ankle_support
        && toe_y
            .zip(toe_terrain_height)
            .is_none_or(|(toe, height)| toe - height >= -SOLE_CONTACT_TOLERANCE_METRES)
}

pub(super) fn balance_recovery_direction(
    projected_com: Vec3,
    left_foot: Option<Vec3>,
    right_foot: Option<Vec3>,
    body_forward: Vec3,
) -> Vec3 {
    let unsupported_offset = match (left_foot, right_foot) {
        (Some(left), Some(right)) => {
            let segment = right.xz() - left.xz();
            let closest = if segment.length_squared() > 0.000001 {
                let progress = ((projected_com.xz() - left.xz()).dot(segment)
                    / segment.length_squared())
                .clamp(0.0, 1.0);
                left.xz() + segment * progress
            } else {
                (left.xz() + right.xz()) * 0.5
            };
            projected_com.xz() - closest
        }
        (Some(foot), None) | (None, Some(foot)) => projected_com.xz() - foot.xz(),
        (None, None) => Vec2::ZERO,
    };
    Vec3::new(unsupported_offset.x, 0.0, unsupported_offset.y)
        .try_normalize()
        .unwrap_or_else(|| body_forward.with_y(0.0).normalize_or_zero())
}

pub(super) fn projected_capture_point(com: Vec3, velocity: Vec3, com_height: f32) -> Vec3 {
    let omega = (9.81 / com_height.max(0.25)).sqrt();
    com + velocity.with_y(0.0) / omega
}

fn choose_settle_support(
    left_weight: Option<f32>,
    right_weight: Option<f32>,
    left_foot: Option<Vec3>,
    right_foot: Option<Vec3>,
    projected_com: Vec3,
    direction: Vec3,
) -> bool {
    let left_weight = left_weight.unwrap_or(0.0);
    let right_weight = right_weight.unwrap_or(0.0);
    if (left_weight - right_weight).abs() > 0.05 {
        return left_weight > right_weight;
    }
    match (left_foot, right_foot) {
        (Some(left), Some(right)) => {
            // In flight, retain the foot behind the moving body so the other
            // foot can capture ahead of its projected center.
            (left - projected_com).dot(direction) <= (right - projected_com).dot(direction)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => true,
    }
}

pub(super) fn plan_settle_landing(
    rig_origin: Vec3,
    rig_rotation: Quat,
    capture_point: Vec3,
    direction: Vec3,
    side: f32,
) -> Vec3 {
    let direction = direction
        .with_y(0.0)
        .try_normalize()
        .unwrap_or(rig_rotation * Vec3::NEG_Z);
    let lateral = rig_rotation * Vec3::X * (FOOT_TRACK_INNER + 0.04) * side.signum();
    let mut target = capture_point.with_y(rig_origin.y + MEASURED_ANKLE_SOLE_OFFSET_METRES)
        + direction * SETTLE_CAPTURE_POINT_MARGIN_METRES
        + lateral;
    // The capture-point requirement is stronger than the anatomical track
    // correction, so restore its forward margin if the corridor clamp erodes
    // it during diagonal movement.
    target = constrain_foot_to_track(target, rig_origin, rig_rotation, side);
    let shortfall = SETTLE_CAPTURE_POINT_MARGIN_METRES - (target - capture_point).dot(direction);
    if shortfall > 0.0 {
        target += direction * shortfall;
    }
    target
}

pub(super) fn settle_swing_side(
    rig_origin: Vec3,
    rig_rotation: Quat,
    swing_start: Vec3,
    semantic_fallback: f32,
) -> f32 {
    let authored_side = (rig_rotation.inverse() * (swing_start - rig_origin)).x;
    if authored_side.abs() > 0.001 {
        authored_side.signum()
    } else {
        semantic_fallback.signum()
    }
}

fn phase_to_next_contact(phase: f32, left: bool) -> f32 {
    let contact_phase = if left { 0.0 } else { 0.5 };
    (contact_phase - phase).rem_euclid(1.0)
}

fn run_contact_approach_progress(
    phase_to_contact: f32,
    approach_window: f32,
    contact_ready_phase: f32,
) -> f32 {
    let linear = ((approach_window - phase_to_contact)
        / (approach_window - contact_ready_phase).max(f32::EPSILON))
    .clamp(0.0, 1.0);
    // Constant horizontal velocity avoids both the old smoothstep mid-swing
    // spike and a one-sample ease/catch-up seam. Clearance remains a separate
    // sine arc, so the endpoint arrives early enough for bounded acquisition.
    linear
}

fn planned_contact_start(
    retained_start: Option<Vec3>,
    prior_visible_target: Option<Vec3>,
    authored_foot: Vec3,
) -> Vec3 {
    retained_start
        .or(prior_visible_target)
        .unwrap_or(authored_foot)
}

fn run_previous_owner_target(
    gait: LocomotionGait,
    last_rendered_owner: Option<Vec3>,
    analytic_owner_target: Option<Vec3>,
) -> Option<Vec3> {
    if gait == LocomotionGait::Run {
        // The analytic target can be centimetres ahead of a reach-constrained
        // rendered ankle. Continuity and final acquisition must advance from
        // the pose the player saw, not from that invisible goal.
        last_rendered_owner.or(analytic_owner_target)
    } else {
        analytic_owner_target
    }
}

fn run_plan_visible_start(
    gait: LocomotionGait,
    starts_new_plan: bool,
    was_releasing: bool,
    previous_owner_target: Option<Vec3>,
    rig_origin: Vec3,
    rig_rotation: Quat,
    propagated_visible_target: Option<Vec3>,
) -> Option<Vec3> {
    if gait == LocomotionGait::Run && starts_new_plan && was_releasing {
        // Hermite progress is zero on the creation sample. Reusing the prior
        // world ankle therefore holds it still while the controller advances
        // 8.6 cm, forcing the nearly extended knee to rearrange in one frame.
        // Preserve its owner-local position across this release-to-plan seam;
        // the new endpoint remains frozen in world space after the seed.
        previous_owner_target
            .map(|owner| rig_origin + rig_rotation * owner)
            .or(propagated_visible_target)
    } else {
        propagated_visible_target
    }
}

fn release_start_owner_target(
    gait: LocomotionGait,
    previous_owner_target: Option<Vec3>,
    previous_world_target: Option<Vec3>,
    rig_origin: Vec3,
    rig_rotation: Quat,
    fallback: Vec3,
) -> Vec3 {
    if gait == LocomotionGait::Run {
        // The first aerial sample follows controller travel in owner space and
        // adds only the clearance floor. Holding the old world plant for this
        // sample adds the full root displacement to the visible foot step.
        previous_owner_target
            .or_else(|| {
                previous_world_target.map(|world| rig_rotation.inverse() * (world - rig_origin))
            })
            .unwrap_or(fallback)
    } else {
        previous_world_target
            .map(|world| rig_rotation.inverse() * (world - rig_origin))
            .or(previous_owner_target)
            .unwrap_or(fallback)
    }
}

fn bound_late_run_contact(
    visible_start: Vec3,
    desired_contact: Vec3,
    speed: f32,
    phase_to_contact: f32,
    contact_ready_phase: f32,
) -> Vec3 {
    let remaining_phase = (phase_to_contact - contact_ready_phase).max(0.0);
    let root_travel = remaining_phase * ordinary_step_distance(speed) * 2.0;
    let remaining_seconds = if speed > 0.05 {
        root_travel / speed
    } else {
        0.0
    };
    let relative_travel =
        MAX_RUN_SWING_ROOT_RELATIVE_STEP_METRES * CONTINUITY_SAMPLE_HZ * remaining_seconds;
    let maximum_horizontal_travel = root_travel + relative_travel;
    let horizontal = desired_contact.xz() - visible_start.xz();
    let bounded = visible_start.xz() + horizontal.clamp_length_max(maximum_horizontal_travel);
    Vec3::new(bounded.x, desired_contact.y, bounded.y)
}

fn late_run_plan_requires_bound(retained_contact: Option<Vec3>, phase_to_contact: f32) -> bool {
    retained_contact.is_none() && phase_to_contact < LATE_RUN_CONTACT_PLAN_PHASE
}

fn unplanned_run_support_requires_flight(
    gait: LocomotionGait,
    speed: f32,
    nominal_support: f32,
    acquired: bool,
    planned_contact: Option<Vec3>,
) -> bool {
    gait == LocomotionGait::Run
        && speed > 0.05
        && terrain_leg_has_support(nominal_support)
        && !acquired
        && planned_contact.is_none()
}

fn run_swing_clearance(phase_to_contact: f32, planned_progress: Option<f32>) -> f32 {
    if let Some(progress) = planned_progress {
        let progress = progress.clamp(0.0, 1.0);
        RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES * (1.0 - progress)
            + (std::f32::consts::PI * progress).sin() * RUN_SWING_SOLE_CLEARANCE_METRES
    } else {
        let progress = (1.0 - phase_to_contact).clamp(0.0, 1.0);
        RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
            + (std::f32::consts::PI * progress).sin() * RUN_SWING_SOLE_CLEARANCE_METRES
    }
}

fn run_airborne_clearance(
    phase_to_contact: f32,
    planned_progress: Option<f32>,
    support_eligible_for_descent: bool,
) -> f32 {
    let clearance = run_swing_clearance(phase_to_contact, planned_progress);
    if support_eligible_for_descent {
        clearance
    } else {
        clearance.max(RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES)
    }
}

fn run_airborne_clearance_for_sample(
    just_released: bool,
    phase_to_contact: f32,
    planned_progress: Option<f32>,
    support_eligible_for_descent: bool,
) -> f32 {
    if just_released {
        // Toe-off spends its first sample establishing the semantic flight
        // floor. Adding the phase swing arc here requested ~9.6 cm of vertical
        // clearance on the captured uphill edge, leaving no terrain-valid
        // point inside the visible foot budget. Later samples build the arc.
        RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
    } else {
        run_airborne_clearance(
            phase_to_contact,
            planned_progress,
            support_eligible_for_descent,
        )
    }
}

fn run_clearance_target_height(
    current_target_y: f32,
    required_height: f32,
    support_eligible_for_descent: bool,
) -> f32 {
    if support_eligible_for_descent {
        // The target may already be resting on the semantic 5 cm flight
        // floor. Once contact becomes eligible, that old raised target is not
        // a lower bound: explicitly request the contact-height descent and let
        // the owner-local follower bound the resulting step.
        required_height
    } else {
        current_target_y.max(required_height)
    }
}

fn run_support_eligible_for_descent(
    gait: LocomotionGait,
    phase: f32,
    left: bool,
    support_radius: f32,
    raw_nominal_support: f32,
    contact_reachable: bool,
) -> bool {
    gait == LocomotionGait::Run
        && contact_reachable
        && terrain_leg_has_support(raw_nominal_support)
        // Only the rising shoulder approaches this foot's contact center.
        // The symmetric raw weight on the post-contact shoulder belongs to
        // stance/toe-off and must not pull an unacquired next-cycle plan down.
        && phase_to_next_contact(phase, left) <= support_radius + 0.001
}

#[cfg(test)]
fn run_contact_within_follower_step(
    previous_owner: Option<Vec3>,
    desired_world: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    delta_seconds: f32,
) -> bool {
    let Some(previous_owner) = previous_owner else {
        return true;
    };
    let desired_owner = rig_rotation.inverse() * (desired_world - rig_origin);
    previous_owner.distance(desired_owner)
        <= RUN_AIRBORNE_OWNER_TARGET_SPEED * delta_seconds.max(0.0) + SOLE_CONTACT_TOLERANCE_METRES
}

fn run_contact_within_leg_reach(target: Vec3, upper_root: Vec3, maximum_reach: f32) -> bool {
    target.distance(upper_root) <= maximum_reach + 0.001
}

#[cfg(test)]
fn run_contact_within_follower_motion_step(
    previous_owner: Option<Vec3>,
    desired_world: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    delta_seconds: f32,
) -> bool {
    let Some(previous_owner) = previous_owner else {
        return true;
    };
    let desired_owner = rig_rotation.inverse() * (desired_world - rig_origin);
    previous_owner.distance(desired_owner)
        <= RUN_AIRBORNE_OWNER_TARGET_SPEED * delta_seconds.max(0.0) + 0.0001
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn retarget_unacquired_run_contact_for_descent(
    previous_owner: Option<Vec3>,
    fixed_contact: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    side: f32,
    upper_root: Vec3,
    maximum_reach: f32,
    delta_seconds: f32,
    terrain_height_at: impl Fn(Vec2) -> Option<f32>,
) -> Option<Vec3> {
    let previous_owner = previous_owner?;
    let fixed_within_motion = run_contact_within_follower_motion_step(
        Some(previous_owner),
        fixed_contact,
        rig_origin,
        rig_rotation,
        delta_seconds,
    );
    let fixed_within_reach = fixed_contact.distance(upper_root) <= maximum_reach + 0.001;
    if fixed_within_motion && fixed_within_reach {
        return None;
    }

    // At 5.5 m/s a fixed world contact recedes 8.6 cm in owner space per
    // sample. Combining that with the final 5 cm descent exceeds the 9 cm
    // continuity budget. A downhill target may instead be inside that motion
    // budget but a few millimetres beyond the analytic leg reach. Start from
    // the visible owner XZ for the first case or the frozen contact for the
    // second, then terrain-resample and project into current reach before
    // freezing the final acquired footprint.
    let start_world = rig_origin + rig_rotation * previous_owner;
    let maximum_motion = RUN_AIRBORNE_OWNER_TARGET_SPEED * delta_seconds.max(0.0);
    let mut transported_contact = if fixed_within_motion {
        fixed_contact
    } else {
        start_world
    };
    for _ in 0..4 {
        transported_contact =
            constrain_foot_to_track(transported_contact, rig_origin, rig_rotation, side);
        let height = terrain_height_at(transported_contact.xz())?;
        transported_contact.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
        let leg_vertical = transported_contact.y - upper_root.y;
        let leg_horizontal = (maximum_reach * maximum_reach - leg_vertical * leg_vertical)
            .max(0.0)
            .sqrt();
        let motion_vertical = transported_contact.y - start_world.y;
        let motion_horizontal = (maximum_motion * maximum_motion
            - motion_vertical * motion_vertical)
            .max(0.0)
            .sqrt();
        let projected = project_point_into_two_disks(
            transported_contact.xz(),
            [
                (upper_root.xz(), leg_horizontal),
                (start_world.xz(), motion_horizontal),
            ],
        );
        transported_contact.x = projected.x;
        transported_contact.z = projected.y;
    }
    transported_contact =
        constrain_foot_to_track(transported_contact, rig_origin, rig_rotation, side);
    let height = terrain_height_at(transported_contact.xz())?;
    transported_contact.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    let accepted = transported_contact.distance(upper_root) <= maximum_reach + 0.001
        && run_contact_within_follower_motion_step(
            Some(previous_owner),
            transported_contact,
            rig_origin,
            rig_rotation,
            delta_seconds,
        );
    if !accepted {
        return None;
    }
    Some(transported_contact)
}

#[cfg(test)]
fn project_point_into_two_disks(mut point: Vec2, disks: [(Vec2, f32); 2]) -> Vec2 {
    let mut corrections = [Vec2::ZERO; 2];
    for _ in 0..24 {
        for (index, (center, radius)) in disks.into_iter().enumerate() {
            let corrected = point + corrections[index];
            let projected = center + (corrected - center).clamp_length_max(radius.max(0.0));
            corrections[index] = corrected - projected;
            point = projected;
        }
    }
    point
}

fn ordinary_contact_target(
    rig_origin: Vec3,
    rig_rotation: Quat,
    projected_com: Vec3,
    velocity: Vec3,
    speed: f32,
    phase_to_contact: f32,
    side: f32,
) -> Vec3 {
    let direction = velocity
        .with_y(0.0)
        .try_normalize()
        .unwrap_or(rig_rotation * Vec3::NEG_Z);
    // One complete phase contains two ordinary steps. Predicting by the
    // remaining phase makes the world landing nearly stationary as the root
    // advances, instead of recomputing a target that chases the body.
    let remaining_travel = phase_to_contact * ordinary_step_distance(speed) * 2.0;
    plan_settle_landing(
        rig_origin,
        rig_rotation,
        projected_com + direction * remaining_travel,
        direction,
        side,
    )
}

#[allow(clippy::too_many_arguments)]
fn reachable_run_contact_target(
    mut candidate: Vec3,
    current_upper_root: Vec3,
    velocity: Vec3,
    speed: f32,
    phase_to_contact: f32,
    contact_ready_phase: f32,
    maximum_reach: f32,
    terrain_height_at: impl Fn(Vec2) -> Option<f32>,
) -> Vec3 {
    let direction = velocity.with_y(0.0).try_normalize().unwrap_or(Vec3::NEG_Z);
    let support_radius = (contact_ready_phase - RUN_CONTACT_CHAIN_SETTLE_PHASE).max(0.0);
    let travel_per_phase = ordinary_step_distance(speed) * 2.0;
    let current_terrain_height = terrain_height_at(current_upper_root.xz());
    let predicted_upper_roots = [
        phase_to_contact - support_radius,
        phase_to_contact,
        phase_to_contact + support_radius,
    ]
    .map(|remaining_phase| {
        let mut root =
            current_upper_root + direction * (remaining_phase.max(0.0) * travel_per_phase);
        if let (Some(current_height), Some(predicted_height)) =
            (current_terrain_height, terrain_height_at(root.xz()))
        {
            root.y += predicted_height - current_height;
        }
        root - Vec3::Y * RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP
    });
    // The world footprint must remain reachable for the whole stance, not
    // merely at entry. Project its XZ into the intersection of the predicted
    // entry/center/exit reach disks. Dykstra's deterministic projection keeps
    // an already feasible desired footprint unchanged and finds the closest
    // point in the convex intersection otherwise. Resampling between passes
    // accounts for the changing vertical budget on sloped terrain.
    for _ in 0..4 {
        if let Some(height) = terrain_height_at(candidate.xz()) {
            candidate.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
        }
        candidate = project_run_contact_into_reach_intersection(
            candidate,
            predicted_upper_roots,
            maximum_reach,
        );
    }
    if let Some(height) = terrain_height_at(candidate.xz()) {
        candidate.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    }
    if !run_contact_reachable_through_stance(candidate, predicted_upper_roots, maximum_reach) {
        // A cliff-like sample can have no shared stance footprint even with
        // the bounded pelvis allowance. Keep support entry continuous; the
        // reach-release latch will end the stance later without reacquiring
        // the same lobe if the hip path still diverges.
        for _ in 0..3 {
            candidate = project_run_contact_into_reach_intersection(
                candidate,
                [predicted_upper_roots[0]; 3],
                maximum_reach,
            );
            if let Some(height) = terrain_height_at(candidate.xz()) {
                candidate.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
            }
        }
    }
    candidate
}

fn project_run_contact_into_reach_intersection(
    mut candidate: Vec3,
    predicted_upper_roots: [Vec3; 3],
    maximum_reach: f32,
) -> Vec3 {
    let horizontal_reaches = predicted_upper_roots.map(|root| {
        let vertical_delta = candidate.y - root.y;
        (maximum_reach * maximum_reach - vertical_delta * vertical_delta)
            .max(0.0)
            .sqrt()
    });
    let mut point = candidate.xz();
    let mut corrections = [Vec2::ZERO; 3];
    for _ in 0..24 {
        for (index, root) in predicted_upper_roots.iter().enumerate() {
            let corrected = point + corrections[index];
            let offset = corrected - root.xz();
            let projected = root.xz() + offset.clamp_length_max(horizontal_reaches[index]);
            corrections[index] = corrected - projected;
            point = projected;
        }
    }
    candidate.x = point.x;
    candidate.z = point.y;
    candidate
}

fn run_contact_reachable_through_stance(
    candidate: Vec3,
    predicted_upper_roots: [Vec3; 3],
    maximum_reach: f32,
) -> bool {
    predicted_upper_roots
        .into_iter()
        .all(|root| candidate.distance(root) <= maximum_reach + 0.001)
}

fn acquisition_planted_target(
    planted_target: Vec3,
    upper_root: Vec3,
    maximum_reach: f32,
    gait: LocomotionGait,
    acquired: bool,
) -> Vec3 {
    if acquired || gait == LocomotionGait::Run {
        planted_target
    } else {
        constrain_target_to_reach(planted_target, upper_root, maximum_reach)
    }
}

fn advance_scalar_at_speed(current: f32, desired: f32, delta_seconds: f32, speed: f32) -> f32 {
    let maximum_step = speed.max(0.0) * delta_seconds.max(0.0);
    current + (desired - current).clamp(-maximum_step, maximum_step)
}

fn advance_run_airborne_world_target(
    previous_owner: Option<Vec3>,
    desired_world: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    delta_seconds: f32,
    speed: f32,
    minimum_ankle_y_at: impl Fn(Vec2) -> Option<f32>,
) -> Vec3 {
    let Some(previous_owner) = previous_owner.filter(|target| target.is_finite()) else {
        let mut target = desired_world;
        if let Some(minimum_y) = minimum_ankle_y_at(target.xz()) {
            target.y = target.y.max(minimum_y);
        }
        return target;
    };
    let start_world = rig_origin + rig_rotation * previous_owner;
    let maximum_step = speed.max(0.0) * delta_seconds.max(0.0);
    let cleared_at = |progress: f32| {
        let mut target = start_world.lerp(desired_world, progress.clamp(0.0, 1.0));
        if let Some(minimum_y) = minimum_ankle_y_at(target.xz()) {
            target.y = target.y.max(minimum_y);
        }
        target
    };
    let desired = cleared_at(1.0);
    if desired.distance(start_world) <= maximum_step {
        return desired;
    }

    // Clearance and the 3D budget are solved jointly. On an uphill release,
    // both full owner transport (terrain rise) and literal world hold (root
    // displacement) can be outside the sphere while an intermediate XZ is
    // feasible. That set is not monotone from either endpoint, so first scan
    // deterministically for its farthest feasible interval, then refine only
    // the local exit boundary.
    const SEARCH_SAMPLES: usize = 64;
    let mut best = None;
    let mut best_progress = 0.0;
    for index in 0..=SEARCH_SAMPLES {
        let progress = index as f32 / SEARCH_SAMPLES as f32;
        let candidate = cleared_at(progress);
        if candidate.distance(start_world) <= maximum_step + 0.000001 {
            best = Some(candidate);
            best_progress = progress;
        }
    }
    let Some(mut best) = best else {
        // The straight owner-transport-to-world-plant segment can cross a
        // locally high triangle even when a small lateral/fore-aft detour is
        // inside the same 3D motion sphere. Search that complete horizontal
        // disk before accepting an over-budget fallback. This matters at
        // toe-off: the semantic sole floor and a terrain rise can make both
        // line endpoints invalid while a nearby downhill point remains valid.
        if let Some(feasible) = terrain_feasible_target_in_step(
            start_world,
            desired_world,
            maximum_step,
            &minimum_ankle_y_at,
        ) {
            return feasible;
        }
        // A true vertical discontinuity can leave no valid point in the full
        // disk. Keep clearance truthful and choose the least-discontinuous
        // line sample; support/reach ownership handles that rare fallback.
        let fallback = (0..=SEARCH_SAMPLES)
            .map(|index| cleared_at(index as f32 / SEARCH_SAMPLES as f32))
            .min_by(|left, right| {
                left.distance_squared(start_world)
                    .total_cmp(&right.distance_squared(start_world))
            })
            .unwrap_or_else(|| cleared_at(0.0));
        return fallback;
    };
    let mut low = best_progress;
    let mut high = (best_progress + 1.0 / SEARCH_SAMPLES as f32).min(1.0);
    for _ in 0..12 {
        let middle = (low + high) * 0.5;
        let candidate = cleared_at(middle);
        if candidate.distance(start_world) <= maximum_step + 0.000001 {
            low = middle;
            best = candidate;
        } else {
            high = middle;
        }
    }
    best
}

fn terrain_feasible_target_in_step(
    start_world: Vec3,
    desired_world: Vec3,
    maximum_step: f32,
    minimum_ankle_y_at: &impl Fn(Vec2) -> Option<f32>,
) -> Option<Vec3> {
    if maximum_step <= f32::EPSILON {
        return None;
    }
    let mut best: Option<(f32, Vec3)> = None;
    let mut center = start_world.xz();
    let mut radius = maximum_step;
    // A deterministic square-disk search followed by local refinements finds
    // terrain-feasible points off the direct swing chord without introducing
    // frame-rate-dependent iteration or lateral drift.
    for refinement in 0..4 {
        const HALF_GRID: i32 = 12;
        let spacing = radius / HALF_GRID as f32;
        for x in -HALF_GRID..=HALF_GRID {
            for z in -HALF_GRID..=HALF_GRID {
                let xz = center + Vec2::new(x as f32 * spacing, z as f32 * spacing);
                let offset = xz - start_world.xz();
                if offset.length_squared() > maximum_step * maximum_step + 0.000001 {
                    continue;
                }
                let chord = desired_world.xz() - start_world.xz();
                let chord_progress = if chord.length_squared() > f32::EPSILON {
                    offset.dot(chord) / chord.length_squared()
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);
                let nearest_chord = start_world.xz() + chord * chord_progress;
                if xz.distance(nearest_chord) > 0.04 {
                    continue;
                }
                let minimum_y = minimum_ankle_y_at(xz)?;
                let candidate = Vec3::new(xz.x, desired_world.y.max(minimum_y), xz.y);
                if candidate.distance(start_world) > maximum_step + 0.000001 {
                    continue;
                }
                let score = candidate.distance_squared(desired_world);
                if best.is_none_or(|(best_score, _)| score < best_score) {
                    best = Some((score, candidate));
                }
            }
        }
        let Some((_, candidate)) = best else {
            break;
        };
        if refinement < 3 {
            center = candidate.xz();
            radius = spacing * 2.0;
        }
    }
    best.map(|(_, candidate)| candidate)
}

pub(super) fn settle_swing_target(start: Vec3, landing: Vec3, progress: f32) -> Vec3 {
    let progress = progress.clamp(0.0, 1.0);
    let horizontal = smoothstep(0.0, 1.0, progress);
    let mut target = start.lerp(landing, horizontal);
    target.y += (std::f32::consts::PI * progress).sin() * SETTLE_STEP_CLEARANCE_METRES;
    target
}

fn toe_aware_minimum_ankle_y(
    rendered_ankle: Vec3,
    rendered_toe: Vec3,
    desired_ankle_xz: Vec2,
    minimum_toe_clearance: f32,
    terrain_height_at: impl Fn(Vec2) -> Option<f32>,
) -> Option<f32> {
    let ankle_clearance = rendered_ankle.y - terrain_height_at(rendered_ankle.xz())?;
    let toe_clearance = rendered_toe.y - terrain_height_at(rendered_toe.xz())?;
    let ankle_above_toe = ankle_clearance - toe_clearance;
    let desired_height = terrain_height_at(desired_ankle_xz)?;
    Some(desired_height + ankle_above_toe + minimum_toe_clearance)
        .filter(|height| height.is_finite())
}

fn transition_toe_clearance_with_rotation_margin(
    rendered_ankle: Vec3,
    rendered_toe: Vec3,
    delta_seconds: f32,
) -> f32 {
    // The cached foot chain may rotate by up to nine degrees after the ankle
    // target is selected. Reserve the maximum vertical motion of the visible
    // ankle-to-toe lever so a target that was toe-safe before finalization is
    // still toe-safe in the propagated pose.
    let angular_step = (AIRBORNE_FOOT_ROTATION_SPEED_DEGREES * delta_seconds.max(0.0))
        .min(90.0)
        .to_radians();
    TERRAIN_TRANSITION_FLIGHT_TOE_CLEARANCE_METRES
        + rendered_ankle.distance(rendered_toe) * angular_step.sin()
}

fn terrain_maximum_reach(upper_length: f32, lower_length: f32) -> f32 {
    (upper_length * upper_length
        + lower_length * lower_length
        + 2.0 * upper_length * lower_length * MIN_TERRAIN_KNEE_FLEXION.cos())
    .sqrt()
}

/// World-space plant confidence used by diagnostics. Procedural guard movement
/// has exactly one support foot while the other follows its clearance arc.
pub(crate) fn locomotion_support_weights(skeleton: &SkeletonState) -> (f32, f32) {
    let speed = skeleton.animation_speed();
    if !skeleton.is_grounded() {
        return (0.0, 0.0);
    }
    if !matches!(
        skeleton.action_kind(),
        SkeletonAction::None | SkeletonAction::Attack | SkeletonAction::Block
    ) {
        return (0.0, 0.0);
    }
    if speed <= 0.05 {
        return (1.0, 1.0);
    }
    if skeleton.weapon_guard() == WeaponGuardState::Raised
        && skeleton.raised_locomotion().is_moving()
    {
        let swing_left = skeleton.raised_locomotion().swing_foot() == Some(LeadFoot::Left);
        ((!swing_left) as u8 as f32, swing_left as u8 as f32)
    } else {
        let profile = locomotion_profile(skeleton);
        let (left, right) = gait_support_weights(profile, skeleton.gait_phase);
        if profile.gait == LocomotionGait::Run {
            (contact_support_weight(left), contact_support_weight(right))
        } else {
            exclusive_ground_support(left, right, skeleton.gait_phase)
        }
    }
}

fn exclusive_ground_support(left: f32, right: f32, phase: f32) -> (f32, f32) {
    if left <= f32::EPSILON {
        return (0.0, contact_support_weight(right));
    }
    if right <= f32::EPSILON {
        return (contact_support_weight(left), 0.0);
    }
    if left > right || ((left - right).abs() <= f32::EPSILON && phase.rem_euclid(1.0) >= 0.75) {
        (contact_support_weight(left), 0.0)
    } else {
        (0.0, contact_support_weight(right))
    }
}

fn contact_support_weight(weight: f32) -> f32 {
    // Preserve the complete profile-owned support window. Thresholding this
    // confidence delayed the effective contact edge and lengthened a 5.5 m/s
    // run flight from about 100 ms to roughly 140 ms.
    weight.clamp(0.0, 1.0)
}

pub(super) fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn anatomical_side(rig_rotation: Quat, rig_origin: Vec3, hip: Vec3, left: bool) -> f32 {
    let hip_x = (rig_rotation.inverse() * (hip - rig_origin)).x;
    if hip_x.abs() > 0.001 {
        hip_x.signum()
    } else if left {
        -1.0
    } else {
        1.0
    }
}

pub(super) fn constrain_foot_to_track(
    world: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    side: f32,
) -> Vec3 {
    let mut local = rig_rotation.inverse() * (world - rig_origin);
    let signed_x = (local.x * side).clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    local.x = signed_x * side;
    rig_origin + rig_rotation * local
}
pub(super) fn plan_guard_step_endpoint(
    step_origin: Vec3,
    step_rotation: Quat,
    mut stance_local: Vec3,
    local_direction: Vec2,
    step_length: f32,
    left: bool,
    opposite_plant: Vec3,
) -> Vec3 {
    // Cascadeur's authored lateral axis is opposite the conventional Bevy
    // anatomical assumption. Derive the corridor from the actual pose rather
    // than assigning a sign from the semantic bone name.
    let side = if stance_local.x.abs() > 0.001 {
        stance_local.x.signum()
    } else if left {
        -1.0
    } else {
        1.0
    };
    let lateral_travel = local_direction.x * step_length;
    let authored_track = (stance_local.x * side)
        .abs()
        .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    let moving_toward_side = lateral_travel * side > 0.001;
    let mut track = if lateral_travel.abs() <= 0.001 {
        authored_track
    } else if moving_toward_side {
        (lateral_travel.abs() + FOOT_TRACK_INNER).min(FOOT_TRACK_OUTER)
    } else {
        FOOT_TRACK_INNER
    };
    let future_origin = step_origin
        + step_rotation * Vec3::new(local_direction.x, 0.0, local_direction.y) * step_length;
    let opposite_local = step_rotation.inverse() * (opposite_plant - future_origin);
    // Separation is an anatomical lateral-track contract. Fore/aft spacing
    // must not be credited toward it or feet can converge onto one tightrope.
    let separation_track = opposite_local.x * side + GUARD_TARGET_INTER_FOOT_SEPARATION;
    track = track
        .max(separation_track)
        .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    stance_local.x = track * side;
    future_origin + step_rotation * stance_local
}

#[allow(clippy::too_many_arguments)]
fn body_relative_guard_gait_targets(
    gait_origin: Vec3,
    gait_lateral: Vec3,
    stance_local: Vec3,
    world_direction: Vec3,
    step_length: f32,
    half_step_progress: f32,
    swing_left: bool,
    terrain_enabled: bool,
    terrain: Option<&SceneTerrain>,
) -> (Vec3, Vec3) {
    let progress = half_step_progress.clamp(0.0, 1.0);
    let direction = world_direction.with_y(0.0).normalize_or_zero();
    let lateral = gait_lateral.with_y(0.0).normalize_or_zero();
    let support_offset = direction * step_length * (0.5 - progress);
    let swing_offset = direction * step_length * contact_matched_guard_swing_offset(progress);
    let stance_track = if stance_local.x.abs() > 0.001 {
        stance_local
            .x
            .abs()
            .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER)
    } else {
        FOOT_TRACK_INNER + 0.04
    };
    let contact = |side: f32, offset: Vec3| {
        let mut target = gait_origin + lateral * (side * stance_track) + offset;
        // Some production scenes expose terrain through the controller but
        // not through a unique SceneTerrain query. Retain the acquired
        // morphology/ground-relative ankle height in that case; using the rig
        // origin plus a global sole constant lifted both feet by the rig's
        // authored root offset.
        target.y = gait_origin.y + stance_local.y;
        if terrain_enabled {
            terrain_conformed_guard_target(
                target,
                terrain.and_then(|terrain| terrain.height_at(target.xz())),
            )
        } else {
            target
        }
    };
    let mut left = contact(
        -1.0,
        if swing_left {
            swing_offset
        } else {
            support_offset
        },
    );
    let mut right = contact(
        1.0,
        if swing_left {
            support_offset
        } else {
            swing_offset
        },
    );
    let lift = c2_swing_arch(progress) * GUARD_PIVOT_LIFT_METRES;
    if swing_left {
        left.y += lift;
    } else {
        right.y += lift;
    }
    (left, right)
}

fn guard_gait_targets_with_latched_support(
    generated: (Vec3, Vec3),
    plants: (Vec3, Vec3),
    swing_left: bool,
    terrain_enabled: bool,
    terrain: Option<&SceneTerrain>,
) -> (Vec3, Vec3) {
    let conform = |plant: Vec3| {
        if terrain_enabled {
            terrain_conformed_guard_target(
                plant,
                terrain.and_then(|terrain| terrain.height_at(plant.xz())),
            )
        } else {
            plant
        }
    };
    if swing_left {
        (generated.0, conform(plants.1))
    } else {
        (conform(plants.0), generated.1)
    }
}

fn overgrowth_guard_step_rate(speed: f32, morphology_scale: f32) -> f32 {
    (speed.max(0.0) / morphology_scale.max(0.01) * 1.5 + 1.0).max(2.0)
}

fn overgrowth_guard_stance_error(morphology_scale: f32) -> f32 {
    0.10 * morphology_scale.clamp(0.25, 4.0)
}

fn overgrowth_guard_foot_targets(
    current_hip_center: Vec3,
    gait_lateral: Vec3,
    stance_local: Vec3,
    world_velocity: Vec3,
    step_rate: f32,
    terrain_enabled: bool,
    terrain: Option<&SceneTerrain>,
) -> (Vec3, Vec3) {
    let lateral = gait_lateral.with_y(0.0).normalize_or_zero();
    let track = stance_local
        .x
        .abs()
        .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    let target_center =
        current_hip_center + world_velocity.with_y(0.0) / step_rate.max(f32::EPSILON);
    let target = |side: f32| {
        let mut target = target_center + lateral * (side * track);
        target.y = current_hip_center.y + stance_local.y;
        if terrain_enabled {
            terrain_conformed_guard_target(
                target,
                terrain.and_then(|terrain| terrain.height_at(target.xz())),
            )
        } else {
            target
        }
    };
    (target(-1.0), target(1.0))
}

/// Normalized body-relative swing displacement from the rear contact to the
/// forward contact. With a truly planted support foot, the swing must be at
/// rest at lift-off and landing rather than matching the obsolete sliding
/// support velocity.
fn contact_matched_guard_swing_offset(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    quintic_progress(progress) - 0.5
}

fn constrain_guard_gait_target_to_reach(target: Vec3, hip: Vec3, reach: f32) -> Vec3 {
    if !target.is_finite() || !hip.is_finite() || !reach.is_finite() || reach <= 0.0 {
        return target;
    }
    let vertical = target.y - hip.y;
    let planar_limit = (reach * reach - vertical * vertical).max(0.0).sqrt();
    let planar = target.xz() - hip.xz();
    if planar.length_squared() <= planar_limit * planar_limit {
        target
    } else {
        let constrained = hip.xz() + planar.normalize_or_zero() * planar_limit;
        Vec3::new(constrained.x, target.y, constrained.y)
    }
}

pub(super) fn guard_step_sequence_delta(previous: u32, current: u32) -> u32 {
    current.wrapping_sub(previous)
}

fn guard_cadence_may_turnover(
    advances: bool,
    sequence_delta: u32,
    release_owner_active: bool,
    segment_active: bool,
) -> bool {
    advances && sequence_delta == 1 && !release_owner_active && !segment_active
}

fn guard_recovery_may_rebase_on_semantic_edge(
    sequence_delta: u32,
    recovery_owner_active: bool,
    on_time_contact_must_complete: bool,
) -> bool {
    sequence_delta == 1 && recovery_owner_active && !on_time_contact_must_complete
}

fn complete_guard_segment_semantics(
    footwork: &mut RaisedFootworkState,
    segment: PlannedGuardFootSegment,
    current_swing_left: bool,
    current_step_sequence: u32,
    await_next_cadence: bool,
) {
    retain_or_defer_guard_cadence_identity(
        footwork,
        current_swing_left,
        current_step_sequence,
        await_next_cadence,
    );
    let endpoint = segment.end.position();
    if segment.end.is_contact() {
        // Retain the completed typed owner until the authoritative cadence
        // edge consumes it. This keeps the exact endpoint, zero derivatives,
        // and contact-envelope completion visible to diagnostics on the
        // contact tick and on repeated evaluation of that tick.
        footwork.swing_replan_segment = await_next_cadence.then_some(segment);
        footwork.swing_release_owner_active = false;
        footwork.swing_emergency_brake = None;
        if footwork.swing_left {
            footwork.left_plant = endpoint;
            if !await_next_cadence {
                footwork.left_support_weight = 1.0;
            }
        } else {
            footwork.right_plant = endpoint;
            if !await_next_cadence {
                footwork.right_support_weight = 1.0;
            }
        }
    } else {
        footwork.swing_replan_segment = None;
        // A reach-safe release endpoint is not a promised contact. Keep the
        // release owner alive at its analytic zero-derivative endpoint until
        // the next authoritative cadence edge can select and prove a contact.
        footwork.swing_release_owner_active = true;
        footwork.swing_emergency_brake = Some(EmergencyFootBrake {
            stationary_ideal: endpoint,
            owner_local_ideal: None,
        });
    }
    footwork.swing_start = endpoint;
    // Do not mutate the presented solve target or analytic ideal history here.
    // The stateful follower still has one terminal integration to perform with
    // the segment's explicit zero endpoint derivatives. Its normal publication
    // below records the actual rendered ankle and completed history together.
}

fn complete_contact_driven_guard_segment(
    footwork: &mut RaisedFootworkState,
    segment: PlannedGuardFootSegment,
) {
    let endpoint = segment.end.position();
    if segment.end.is_contact() {
        // Keep the terminal owner for one semantic tick. The next advancing
        // evaluation consumes the physical landing and starts the opposite
        // swing; replicated cadence is not required to unlock it.
        footwork.swing_replan_segment = Some(segment);
        footwork.swing_release_owner_active = false;
        footwork.swing_emergency_brake = None;
        footwork.awaiting_step_sequence = false;
        footwork.pending_cadence_edge = None;
        if footwork.swing_left {
            footwork.left_plant = endpoint;
            footwork.left_support_weight = 1.0;
        } else {
            footwork.right_plant = endpoint;
            footwork.right_support_weight = 1.0;
        }
    } else {
        // A grounded contact-first cadence never promotes an airborne release
        // to locomotion ownership. Leave the current swing unplanted and let
        // the next evaluation plan another terrain contact.
        footwork.swing_replan_segment = None;
        footwork.swing_release_owner_active = false;
        footwork.swing_emergency_brake = None;
        footwork.awaiting_step_sequence = false;
    }
    footwork.swing_start = endpoint;
}

fn install_guard_support_release(
    footwork: &mut RaisedFootworkState,
    left: bool,
    presented: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    trajectory: Option<PredictedHipTrajectory>,
) {
    let segment = trajectory
        .and_then(|trajectory| {
            plan_c2_release_segment(
                presented,
                velocity,
                acceleration,
                GUARD_FORCED_RELEASE_SECONDS,
                trajectory,
            )
        })
        .map(|segment| segment.with_owner_epoch(footwork.evaluation_tick.unwrap_or(0)));
    let emergency = segment.is_none().then_some(EmergencyFootBrake {
        stationary_ideal: presented,
        owner_local_ideal: None,
    });
    let (ideal_velocity, ideal_acceleration) = if segment.is_some() {
        (velocity, acceleration)
    } else {
        (Vec3::ZERO, Vec3::ZERO)
    };
    if left {
        footwork.left_support_release_owner = segment
            .map(SupportReleaseOwner::Segment)
            .or_else(|| emergency.map(SupportReleaseOwner::EmergencyBrake));
        footwork.left_desired_target = Some(presented);
        footwork.left_ideal_velocity = ideal_velocity;
        footwork.left_ideal_acceleration = ideal_acceleration;
        footwork.left_ideal_history_valid = true;
    } else {
        footwork.right_support_release_owner = segment
            .map(SupportReleaseOwner::Segment)
            .or_else(|| emergency.map(SupportReleaseOwner::EmergencyBrake));
        footwork.right_desired_target = Some(presented);
        footwork.right_ideal_velocity = ideal_velocity;
        footwork.right_ideal_acceleration = ideal_acceleration;
        footwork.right_ideal_history_valid = true;
    }
}

fn complete_support_release_if_settled(
    footwork: &mut RaisedFootworkState,
    left: bool,
    presented: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    current_swing_left: bool,
    current_step_sequence: u32,
    await_next_cadence: bool,
) {
    let owner = if left {
        footwork.left_support_release_owner
    } else {
        footwork.right_support_release_owner
    };
    let terminal = match owner {
        Some(SupportReleaseOwner::TerminalHold { endpoint }) => {
            presented.distance(endpoint) <= 0.001
                && emergency_brake_is_settled(velocity, acceleration)
        }
        Some(SupportReleaseOwner::EmergencyBrake(_)) => {
            emergency_brake_is_settled(velocity, acceleration)
        }
        _ => false,
    };
    if !terminal {
        return;
    }
    if left {
        footwork.left_support_release_owner = None;
        footwork.left_plant = presented;
        footwork.left_solve_target = Some(presented);
        footwork.left_desired_target = Some(presented);
        footwork.left_target_velocity = Vec3::ZERO;
        footwork.left_target_acceleration = Vec3::ZERO;
        footwork.left_ideal_velocity = Vec3::ZERO;
        footwork.left_ideal_acceleration = Vec3::ZERO;
        footwork.left_ideal_history_valid = true;
    } else {
        footwork.right_support_release_owner = None;
        footwork.right_plant = presented;
        footwork.right_solve_target = Some(presented);
        footwork.right_desired_target = Some(presented);
        footwork.right_target_velocity = Vec3::ZERO;
        footwork.right_target_acceleration = Vec3::ZERO;
        footwork.right_ideal_velocity = Vec3::ZERO;
        footwork.right_ideal_acceleration = Vec3::ZERO;
        footwork.right_ideal_history_valid = true;
    }
    retain_or_defer_guard_cadence_identity(
        footwork,
        current_swing_left,
        current_step_sequence,
        await_next_cadence,
    );
}

fn supersede_support_segment_with_emergency(
    footwork: &mut RaisedFootworkState,
    left: bool,
    presented: Vec3,
    owner_local_ideal: Option<Vec3>,
) -> bool {
    let owner = if left {
        footwork.left_support_release_owner
    } else {
        footwork.right_support_release_owner
    };
    if !matches!(owner, Some(SupportReleaseOwner::Segment(_))) {
        return false;
    }
    let emergency = Some(SupportReleaseOwner::EmergencyBrake(EmergencyFootBrake {
        stationary_ideal: presented,
        owner_local_ideal,
    }));
    if left {
        footwork.left_support_release_owner = emergency;
        footwork.left_desired_target = Some(presented);
        footwork.left_ideal_velocity = Vec3::ZERO;
        footwork.left_ideal_acceleration = Vec3::ZERO;
        footwork.left_ideal_history_valid = true;
    } else {
        footwork.right_support_release_owner = emergency;
        footwork.right_desired_target = Some(presented);
        footwork.right_ideal_velocity = Vec3::ZERO;
        footwork.right_ideal_acceleration = Vec3::ZERO;
        footwork.right_ideal_history_valid = true;
    }
    true
}

fn guard_stationary_owns_pose(stationary_requested: bool, segment_active: bool) -> bool {
    stationary_requested && !segment_active
}

fn guard_pelvis_blocks_stationary_pivot(acquisition: Option<GuardPelvisAcquisition>) -> bool {
    acquisition.is_some_and(|acquisition| {
        acquisition.target.is_some() && acquisition.advance_authorized && acquisition.progress < 1.0
    })
}

fn adopt_guard_movement_identity(
    footwork: &mut RaisedFootworkState,
    replicated_swing_left: bool,
    replicated_step_sequence: u32,
    visible_left: Vec3,
    visible_right: Vec3,
) {
    footwork.swing_left = replicated_swing_left;
    footwork.step_sequence = replicated_step_sequence;
    footwork.left_plant = visible_left;
    footwork.right_plant = visible_right;
    footwork.swing_start = if replicated_swing_left {
        visible_left
    } else {
        visible_right
    };
    footwork.swing_replan_segment = None;
    footwork.swing_emergency_brake = None;
    footwork.pending_cadence_edge = None;
    footwork.swing_release_owner_active = false;
    footwork.awaiting_step_sequence = false;
}

#[allow(clippy::too_many_arguments)]
fn begin_next_guard_swing_after_contact(
    footwork: &mut RaisedFootworkState,
    moving: bool,
    sequence_delta: u32,
    visible_left: Vec3,
    visible_right: Vec3,
    half_step: u8,
    rig_origin: Vec3,
    rig_rotation: Quat,
    left_authored: Vec3,
    right_authored: Vec3,
) -> bool {
    let completed_contact = footwork
        .swing_replan_segment
        .is_some_and(|segment| segment.end.is_contact() && segment.timing.is_complete());
    if !moving
        || sequence_delta != 0
        || !completed_contact
        || footwork.pending_cadence_edge.is_some()
    {
        return false;
    }

    let landed_left = footwork.swing_left;
    let next_swing_left = !landed_left;
    let retained_sequence = footwork.step_sequence;
    adopt_guard_movement_identity(
        footwork,
        next_swing_left,
        retained_sequence,
        visible_left,
        visible_right,
    );
    footwork.half_step = half_step;
    footwork.step_origin = rig_origin;
    footwork.step_rotation = rig_rotation;
    footwork.swing_stance_local = rig_rotation.inverse()
        * ((if next_swing_left {
            left_authored
        } else {
            right_authored
        }) - rig_origin);
    footwork.left_support_weight = if next_swing_left { 0.0 } else { 1.0 };
    footwork.right_support_weight = if next_swing_left { 1.0 } else { 0.0 };
    true
}

#[allow(clippy::too_many_arguments)]
fn begin_next_contact_driven_guard_swing(
    footwork: &mut RaisedFootworkState,
    moving: bool,
    visible_left: Vec3,
    visible_right: Vec3,
    half_step: u8,
    rig_origin: Vec3,
    rig_rotation: Quat,
    left_authored: Vec3,
    right_authored: Vec3,
) -> bool {
    let completed_contact = footwork
        .swing_replan_segment
        .is_some_and(|segment| segment.end.is_contact() && segment.timing.is_complete());
    if !moving || !completed_contact {
        return false;
    }

    let next_swing_left = !footwork.swing_left;
    let local_sequence = footwork.step_sequence.wrapping_add(1);
    adopt_guard_movement_identity(
        footwork,
        next_swing_left,
        local_sequence,
        visible_left,
        visible_right,
    );
    footwork.half_step = half_step;
    footwork.step_origin = rig_origin;
    footwork.step_rotation = rig_rotation;
    footwork.swing_stance_local = rig_rotation.inverse()
        * ((if next_swing_left {
            left_authored
        } else {
            right_authored
        }) - rig_origin);
    // Selecting the next side is not itself a motion owner. Retain both
    // completed contacts until planning atomically installs the next C2
    // segment; otherwise the pelvis drops a real support during proof gaps.
    footwork.left_support_weight = 1.0;
    footwork.right_support_weight = 1.0;
    true
}

fn confirm_preemptive_guard_cadence_edge(
    footwork: &mut RaisedFootworkState,
    sequence_delta: u32,
    replicated_swing_left: bool,
    replicated_sequence: u32,
) -> bool {
    if sequence_delta != 1 || footwork.swing_left != replicated_swing_left {
        return false;
    }
    let Some(mut segment) = footwork.swing_replan_segment else {
        return false;
    };
    let GuardSegmentReachProof::Exact(mut proof) = segment.reach else {
        return false;
    };
    if !proof.accepts_preemptive_cadence_confirmation
        || proof.sequence.wrapping_add(1) != replicated_sequence
    {
        return false;
    }

    proof.sequence = replicated_sequence;
    proof.trajectory_signature.sequence = replicated_sequence;
    proof.accepts_preemptive_cadence_confirmation = false;
    segment.reach = GuardSegmentReachProof::Exact(proof);
    footwork.swing_replan_segment = Some(segment);
    footwork.step_sequence = replicated_sequence;
    footwork.pending_cadence_edge = None;
    footwork.awaiting_step_sequence = false;
    true
}

#[allow(clippy::too_many_arguments)]
fn consume_pending_guard_cadence_edge(
    footwork: &mut RaisedFootworkState,
    pending_swing_left: bool,
    pending_sequence: u32,
    visible_left: Vec3,
    visible_right: Vec3,
    half_step: u8,
    rig_origin: Vec3,
    rig_rotation: Quat,
    left_authored: Vec3,
    right_authored: Vec3,
) {
    footwork.left_support_release_owner = None;
    footwork.right_support_release_owner = None;
    adopt_guard_movement_identity(
        footwork,
        pending_swing_left,
        pending_sequence,
        visible_left,
        visible_right,
    );
    footwork.half_step = half_step;
    footwork.step_origin = rig_origin;
    footwork.step_rotation = rig_rotation;
    footwork.swing_stance_local = rig_rotation.inverse()
        * ((if pending_swing_left {
            left_authored
        } else {
            right_authored
        }) - rig_origin);
    reseed_guard_cadence_ideal_history(footwork, visible_left, visible_right);
}

fn guard_motion_can_transfer_to_cadence(
    presented: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    reach: Option<FootReachEnvelope>,
    delta_seconds: f32,
    sample: ReachMotionSample,
) -> bool {
    let Some(reach) = reach else {
        return false;
    };
    if !presented.is_finite()
        || !velocity.is_finite()
        || !acceleration.is_finite()
        || !delta_seconds.is_finite()
        || delta_seconds <= f32::EPSILON
    {
        return false;
    }
    let root = match sample {
        ReachMotionSample::Current => reach.current_root(),
        ReachMotionSample::Next => reach.next_root(),
    };
    let radial = (presented - root).normalize_or_zero();
    let root_velocity = (reach.next_root() - reach.current_root()) / delta_seconds;
    let outward_velocity = (velocity - root_velocity).dot(radial);
    let outward_acceleration = acceleration.dot(radial);
    let stopping_distance = jerk_limited_stopping_distance(
        outward_velocity,
        outward_acceleration,
        FOOT_FOLLOWER_MAXIMUM_ACCELERATION,
        FOOT_FOLLOWER_MAXIMUM_JERK,
    );
    presented.distance(root) + stopping_distance <= reach.warning_reach()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReachMotionSample {
    Current,
    Next,
}

fn non_support_guard_target_requires_release(
    presented: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    body_relative_target: Vec3,
    reach: Option<FootReachEnvelope>,
    deadline_seconds: f32,
    delta_seconds: f32,
) -> bool {
    let anatomical_error = MaximumPoseError::new(0.05)
        .is_some_and(|maximum| presented.distance(body_relative_target) > maximum.metres());
    let stopping_reach_is_unsafe = !guard_motion_can_transfer_to_cadence(
        presented,
        velocity,
        acceleration,
        reach,
        delta_seconds,
        ReachMotionSample::Next,
    );
    let misses_deadline = !deadline_seconds.is_finite()
        || deadline_seconds <= 0.0
        || minimum_c2_segment_duration(presented, velocity, acceleration, body_relative_target)
            > deadline_seconds;
    anatomical_error || stopping_reach_is_unsafe || misses_deadline
}

fn emergency_brake_is_settled(velocity: Vec3, acceleration: Vec3) -> bool {
    velocity.is_finite()
        && acceleration.is_finite()
        && velocity.length() <= 0.001
        && acceleration.length() <= 0.01
}

pub(super) fn constrain_guard_swing_to_live_corridor(
    target: Vec3,
    support: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    side: f32,
) -> Vec3 {
    let mut local = rig_rotation.inverse() * (target - rig_origin);
    let support_local = rig_rotation.inverse() * (support - rig_origin);
    let required_track = support_local.x * side + GUARD_TARGET_INTER_FOOT_SEPARATION;
    let signed_track = (local.x * side)
        .max(FOOT_TRACK_INNER)
        .max(required_track)
        .min(FOOT_TRACK_OUTER);
    local.x = signed_track * side;
    rig_origin + rig_rotation * local
}

#[allow(clippy::too_many_arguments)]
fn guard_contact_candidate_at_progress(
    presented: Vec3,
    requested: Vec3,
    progress: f32,
    support: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    side: f32,
    terrain_enabled: bool,
    terrain: Option<&SceneTerrain>,
) -> Option<Vec3> {
    let candidate = presented.lerp(requested, progress.clamp(0.0, 1.0));
    let constrained =
        constrain_guard_swing_to_live_corridor(candidate, support, rig_origin, rig_rotation, side);
    if terrain_enabled {
        let height = terrain?.height_at(constrained.xz())?;
        Some(terrain_conformed_guard_target(constrained, Some(height)))
    } else {
        Some(constrained)
    }
}

pub(super) fn terrain_conformed_guard_target(
    mut flat_target: Vec3,
    terrain_height: Option<f32>,
) -> Vec3 {
    if let Some(height) = terrain_height {
        flat_target.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    }
    flat_target
}

fn align_foot_to_slope(
    foot: Entity,
    sole_up_local: Vec3,
    normal: Vec3,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let Some(snapshot) = snapshot(foot, parents, &transforms.p0()) else {
        return;
    };
    let world = slope_aligned_world_rotation(snapshot.global.rotation(), sole_up_local, normal);
    let Some(world) = world else { return };
    let Some(local) = local_rotation_for_world(snapshot.parent_rotation, world) else {
        return;
    };
    if let Ok(mut transform) = transforms.p1().get_mut(foot) {
        transform.rotation = local;
    }
}

fn advance_airborne_foot_rotation(
    previous: Option<Quat>,
    desired: Quat,
    delta_seconds: f32,
    maximum_speed_degrees: f32,
) -> Quat {
    let Some(previous) = previous.filter(|rotation| rotation.is_finite()) else {
        return desired;
    };
    if !desired.is_finite() {
        return previous;
    }
    let angle = previous.angle_between(desired);
    let maximum_step = maximum_speed_degrees.max(0.0).to_radians() * delta_seconds.max(0.0);
    if maximum_step <= f32::EPSILON {
        return previous;
    }
    if angle <= maximum_step || angle <= f32::EPSILON {
        desired
    } else {
        previous.slerp(desired, maximum_step / angle).normalize()
    }
}

fn previous_airborne_foot_orientation(
    analytic_previous: Option<Quat>,
    propagated_previous: Option<Quat>,
    just_released: bool,
) -> Option<Quat> {
    if just_released {
        // The pre-propagation analytic orientation can differ from the foot
        // orientation that the player saw after the full hierarchy settled.
        // Toe-off begins from that propagated pose so a nominally stationary
        // ankle cannot lever the toe through the continuity budget.
        propagated_previous.or(analytic_previous)
    } else {
        analytic_previous
    }
}

/// Phase-aware sagittal foot roll for running. Negative phase is the approach
/// to this foot's contact and positive phase is its stance/release. The curve
/// arrives with a modest dorsiflexed heel presentation, flattens early in
/// stance, then plantar-flexes into toe-off before returning to neutral during
/// swing. Terrain-normal alignment remains the base orientation.
fn run_foot_roll_degrees(skeleton: &SkeletonState, left: bool) -> f32 {
    if locomotion_profile(skeleton).gait != LocomotionGait::Run
        || skeleton.action_kind() != SkeletonAction::None
        || skeleton.weapon_guard() != WeaponGuardState::Lowered
        || skeleton.animation_speed() <= 0.05
    {
        return 0.0;
    }
    let contact = if left { 0.0 } else { 0.5 };
    let signed = (skeleton.gait_phase - contact + 0.5).rem_euclid(1.0) - 0.5;
    let radius = locomotion_profile(skeleton).support_phase_radius;
    if signed < -radius {
        // Prepare the heel during the latter half of flight.
        8.0 * smoothstep(-0.25, -radius, signed)
    } else if signed < -0.05 {
        8.0 * (1.0 - smoothstep(-radius, -0.05, signed))
    } else if signed <= 0.06 {
        0.0
    } else if signed <= radius {
        -8.0 * smoothstep(0.06, radius, signed)
    } else {
        // Release the toe smoothly instead of carrying a pointed foot through
        // the whole airborne arc.
        -8.0 * (1.0 - smoothstep(radius, 0.25, signed))
    }
}

fn finalize_leg_rotation_chains(
    rig: &HumanoidRig,
    skeleton: &SkeletonState,
    rig_rotation: Quat,
    memory: &mut LegIkMemory,
    evaluation_advances: bool,
    delta_seconds: f32,
    airborne_orientation_owned: [bool; 2],
    airborne_just_released: [bool; 2],
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (leg_index, (upper_role, lower_role, foot_role, left)) in [
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
    ]
    .into_iter()
    .enumerate()
    {
        let (Some(&upper), Some(&lower), Some(&foot)) = (
            rig.get(&upper_role),
            rig.get(&lower_role),
            rig.get(&foot_role),
        ) else {
            continue;
        };
        let current = {
            let query = transforms.p1();
            let (Ok(upper), Ok(lower), Ok(foot)) =
                (query.get(upper), query.get(lower), query.get(foot))
            else {
                continue;
            };
            LegRotationChain {
                upper: upper.rotation,
                lower: lower.rotation,
                foot: foot.rotation,
            }
        };
        let cached = if left {
            memory.left_rotation_chain
        } else {
            memory.right_rotation_chain
        };
        let contact_blend_active = if left {
            memory.left_contact_orientation_blend_active
        } else {
            memory.right_contact_orientation_blend_active
        };
        let mut resolved = final_leg_rotation_chain(cached, current, evaluation_advances);
        {
            let mut query = transforms.p1();
            if let Ok(mut transform) = query.get_mut(upper) {
                transform.rotation = resolved.upper;
            }
            if let Ok(mut transform) = query.get_mut(lower) {
                transform.rotation = resolved.lower;
            }
            if let Ok(mut transform) = query.get_mut(foot) {
                transform.rotation = resolved.foot;
            }
        }
        if evaluation_advances
            && let Some(foot_snapshot) = snapshot(foot, parents, &transforms.p0())
        {
            let base_world = foot_snapshot.global.rotation();
            let roll_degrees = run_foot_roll_degrees(skeleton, left);
            let desired_world = if roll_degrees.abs() > f32::EPSILON {
                let lateral = (rig_rotation * Vec3::X).normalize_or_zero();
                Quat::from_axis_angle(lateral, roll_degrees.to_radians()) * base_world
            } else {
                base_world
            };
            let previous_world = if left {
                previous_airborne_foot_orientation(
                    memory.left_foot_orientation_world,
                    memory.left_last_rendered_foot_rotation_world,
                    airborne_just_released[leg_index],
                )
            } else {
                previous_airborne_foot_orientation(
                    memory.right_foot_orientation_world,
                    memory.right_last_rendered_foot_rotation_world,
                    airborne_just_released[leg_index],
                )
            };
            let final_world = if airborne_orientation_owned[leg_index] || contact_blend_active {
                let angular_speed = if locomotion_profile(skeleton).gait == LocomotionGait::Run
                    && airborne_just_released[leg_index]
                {
                    FIRST_RUN_RELEASE_FOOT_ROTATION_SPEED_DEGREES
                } else {
                    AIRBORNE_FOOT_ROTATION_SPEED_DEGREES
                };
                let bounded_world = advance_airborne_foot_rotation(
                    previous_world,
                    desired_world,
                    delta_seconds,
                    angular_speed,
                );
                if let Some(local) =
                    local_rotation_for_world(foot_snapshot.parent_rotation, bounded_world)
                {
                    if let Ok(mut transform) = transforms.p1().get_mut(foot) {
                        transform.rotation = local;
                    }
                    resolved.foot = local;
                }
                bounded_world
            } else {
                desired_world
            };
            if contact_blend_active
                && final_world.angle_between(desired_world) <= 0.001_f32.to_radians()
            {
                if left {
                    memory.left_contact_orientation_blend_active = false;
                } else {
                    memory.right_contact_orientation_blend_active = false;
                }
            }
            if left {
                memory.left_foot_orientation_world = Some(final_world);
            } else {
                memory.right_foot_orientation_world = Some(final_world);
            }
        }
        if left {
            memory.left_rotation_chain = Some(resolved);
        } else {
            memory.right_rotation_chain = Some(resolved);
        }
    }
}

fn final_leg_rotation_chain(
    cached: Option<LegRotationChain>,
    current: LegRotationChain,
    evaluation_advances: bool,
) -> LegRotationChain {
    if evaluation_advances {
        current
    } else {
        cached.unwrap_or(current)
    }
}

fn local_rotation_for_world(parent_world: Quat, desired_world: Quat) -> Option<Quat> {
    let local = parent_world.inverse() * desired_world;
    if local.is_finite() {
        Some(local.normalize())
    } else {
        None
    }
}

fn clear_slope_rotation_cache(memory: &mut LegIkMemory) {
    memory.left_rotation_chain = None;
    memory.right_rotation_chain = None;
    memory.slope_alignment_mode = None;
}

fn prepare_slope_rotation_cache(memory: &mut LegIkMemory, mode: SlopeAlignmentMode) {
    if memory.slope_alignment_mode != Some(mode) {
        clear_slope_rotation_cache(memory);
        memory.slope_alignment_mode = Some(mode);
    }
}

pub(super) fn slope_aligned_world_rotation(
    current_world: Quat,
    sole_up_local: Vec3,
    terrain_normal: Vec3,
) -> Option<Quat> {
    let normal = terrain_normal.try_normalize()?;
    let tilt_angle = Vec3::Y.angle_between(normal).min(28.0_f32.to_radians());
    let bounded_normal = Vec3::Y
        .cross(normal)
        .try_normalize()
        .map_or(Vec3::Y, |axis| {
            Quat::from_axis_angle(axis, tilt_angle) * Vec3::Y
        });
    let current_up = (current_world * sole_up_local).try_normalize()?;
    let correction = Quat::from_rotation_arc(current_up, bounded_normal);
    Some((correction * current_world).normalize())
}

#[cfg(test)]
mod slope_cache_tests {
    use super::*;

    #[test]
    fn anatomical_cone_yields_at_a_straight_leg_singularity() {
        let upper_length = 0.45;
        assert!(!anatomical_pole_is_well_conditioned(
            Vec3::X * 0.00099,
            upper_length
        ));
        assert!(anatomical_pole_is_well_conditioned(
            Vec3::X * 0.02,
            upper_length
        ));
    }

    #[test]
    fn anatomical_handoff_interpolates_antipodal_poles_with_a_stable_sign() {
        let halfway = interpolate_anatomical_pole_signed(Vec3::Z, Vec3::NEG_Z, Vec3::Y, 0.5);
        assert!(halfway.is_finite());
        assert!(halfway.length() > 0.99);
        assert!(halfway.dot(Vec3::Z).abs() < 0.001);
        assert_eq!(
            interpolate_anatomical_pole_signed(Vec3::Z, Vec3::NEG_Z, Vec3::Y, 0.0),
            Vec3::Z
        );
        assert!(
            interpolate_anatomical_pole_signed(Vec3::Z, Vec3::NEG_Z, Vec3::Y, 1.0,)
                .distance(Vec3::NEG_Z)
                < 0.0001
        );
    }

    #[test]
    fn anatomical_pole_tracking_bounds_signed_acceleration_and_jerk() {
        let dt = 1.0 / 64.0;
        let axis = Vec3::Y;
        let target = Quat::from_axis_angle(axis, 30.0_f32.to_radians()) * Vec3::Z;
        let mut pole = Vec3::Z;
        let mut velocity = 0.0;
        let mut acceleration = 0.0;
        let mut previous_angle = 0.0;
        for _ in 0..16 {
            let previous_acceleration = acceleration;
            (pole, velocity, acceleration) =
                track_anatomical_pole_signed(pole, target, axis, velocity, acceleration, dt);
            let angle = axis.dot(Vec3::Z.cross(pole)).atan2(Vec3::Z.dot(pole));
            assert!((angle - previous_angle).abs().to_degrees() < 4.0);
            assert!(acceleration.abs() <= ANATOMICAL_POLE_MAXIMUM_ACCELERATION + 0.001);
            assert!(
                ((acceleration - previous_acceleration) / dt).abs()
                    <= ANATOMICAL_POLE_MAXIMUM_JERK + 0.001
            );
            previous_angle = angle;
        }
        assert!(previous_angle > 0.0);
        assert!((30.0_f32.to_radians() - previous_angle).abs() < 30.0_f32.to_radians());
    }

    #[test]
    fn quickstep_contact_discards_the_landing_handoff_regardless_of_speed() {
        let mut memory = LegIkMemory {
            quickstep_handoff_pending: true,
            quickstep_guard_stance_held: true,
            quickstep_left_landing_local: Some(Vec3::X),
            quickstep_right_landing_local: Some(Vec3::NEG_X),
            left_foot_world_target: Some(Vec3::new(4.0, 0.0, 0.0)),
            right_foot_world_target: Some(Vec3::new(-4.0, 0.0, 0.0)),
            left_foot_plant_acquired: true,
            right_foot_plant_acquired: true,
            measured_owner_planar_speed: 5.0,
            ..default()
        };

        discard_quickstep_contact_handoff(&mut memory);

        assert!(!memory.quickstep_handoff_pending);
        assert!(!memory.quickstep_guard_stance_held);
        assert!(memory.quickstep_left_landing_local.is_none());
        assert!(memory.quickstep_right_landing_local.is_none());
        assert!(memory.left_foot_world_target.is_none());
        assert!(memory.right_foot_world_target.is_none());
        assert!(!memory.left_foot_plant_acquired);
        assert!(!memory.right_foot_plant_acquired);
        assert_eq!(memory.measured_owner_planar_speed, 5.0);
    }

    #[test]
    fn quickstep_handoff_reseeds_stale_guard_solves_from_the_visible_landing() {
        let visible_left = Vec3::new(1.7458137, 1.9343445, -10.977574);
        let visible_right = Vec3::new(2.2413998, 1.9726762, -10.548973);
        let left_velocity = Vec3::new(4.9, 0.2, 0.0);
        let memory = LegIkMemory {
            left_foot_follower: FootFollowerState::from_presented_pose(
                visible_left,
                left_velocity,
                Vec3::X,
                visible_left,
                left_velocity,
                Vec3::ZERO,
            ),
            left_terrain_pole_world: Some(Vec3::Z),
            left_terrain_end_direction: Some(Vec3::NEG_Y),
            ..default()
        };
        let mut footwork = RaisedFootworkState {
            initialized: true,
            left_solve_target: Some(Vec3::new(1.057, 2.645, -10.933)),
            right_solve_target: Some(Vec3::new(3.1, 2.4, -10.2)),
            left_target_velocity: Vec3::splat(100.0),
            ..default()
        };

        reseed_raised_from_quickstep_handoff(&mut footwork, &memory, visible_left, visible_right);

        assert_eq!(footwork.left_solve_target, Some(visible_left));
        assert_eq!(footwork.right_solve_target, Some(visible_right));
        assert_eq!(footwork.left_target_velocity, left_velocity);
        assert!(footwork.left_ideal_history_valid);
        assert_eq!(footwork.left_knee_bend_world, Some(Vec3::Z));
        assert_eq!(footwork.left_end_direction, Some(Vec3::NEG_Y));
        assert!(
            footwork
                .left_solve_target
                .unwrap()
                .distance(Vec3::new(1.057, 2.645, -10.933))
                > 0.98
        );
    }

    #[test]
    fn guard_cadence_edge_reseeds_the_new_path_without_erasing_follower_derivatives() {
        let left = Vec3::new(-0.12, 1.94, -0.4);
        let right = Vec3::new(0.12, 1.94, -0.5);
        let velocity = Vec3::new(1.2, 0.1, -0.4);
        let acceleration = Vec3::new(2.0, -1.0, 0.5);
        let mut footwork = RaisedFootworkState {
            left_solve_target: Some(Vec3::splat(4.0)),
            right_solve_target: Some(Vec3::splat(-4.0)),
            left_target_velocity: velocity,
            left_target_acceleration: acceleration,
            left_desired_target: Some(Vec3::splat(8.0)),
            left_ideal_velocity: Vec3::splat(20.0),
            left_ideal_acceleration: Vec3::splat(40.0),
            ..default()
        };

        reseed_guard_cadence_ideal_history(&mut footwork, left, right);

        assert_eq!(footwork.left_solve_target, Some(left));
        assert_eq!(footwork.right_solve_target, Some(right));
        assert_eq!(footwork.left_desired_target, Some(left));
        assert_eq!(footwork.right_desired_target, Some(right));
        assert_eq!(footwork.left_ideal_velocity, Vec3::ZERO);
        assert_eq!(footwork.left_ideal_acceleration, Vec3::ZERO);
        assert!(footwork.left_ideal_history_valid && footwork.right_ideal_history_valid);
        assert_eq!(footwork.left_target_velocity, velocity);
        assert_eq!(footwork.left_target_acceleration, acceleration);
    }

    #[test]
    fn slope_rotation_cache_is_preserved_within_tick_and_cleared_between_modes() {
        let cached = LegRotationChain {
            upper: Quat::from_rotation_x(0.2),
            lower: Quat::from_rotation_z(-0.3),
            foot: Quat::from_rotation_y(0.4),
        };
        let mut memory = LegIkMemory {
            left_rotation_chain: Some(cached),
            slope_alignment_mode: Some(SlopeAlignmentMode::Raised),
            ..default()
        };

        prepare_slope_rotation_cache(&mut memory, SlopeAlignmentMode::Raised);
        assert_eq!(memory.left_rotation_chain, Some(cached));

        prepare_slope_rotation_cache(&mut memory, SlopeAlignmentMode::Ordinary);
        assert_eq!(memory.left_rotation_chain, None);
        assert_eq!(memory.right_rotation_chain, None);
        assert_eq!(
            memory.slope_alignment_mode,
            Some(SlopeAlignmentMode::Ordinary)
        );

        memory.right_rotation_chain = Some(cached);
        clear_slope_rotation_cache(&mut memory);
        assert_eq!(memory.right_rotation_chain, None);
        assert_eq!(memory.slope_alignment_mode, None);
    }

    #[test]
    fn repeated_evaluation_restores_the_exact_cached_leg_chain() {
        let cached = LegRotationChain {
            upper: Quat::from_rotation_x(0.2),
            lower: Quat::from_rotation_z(-0.3),
            foot: Quat::from_rotation_y(0.4),
        };
        let perturbed_by_second_solve = LegRotationChain {
            upper: Quat::from_rotation_x(-0.5),
            lower: Quat::from_rotation_z(0.6),
            foot: Quat::from_rotation_y(-0.7),
        };

        assert_eq!(
            final_leg_rotation_chain(Some(cached), perturbed_by_second_solve, false),
            cached
        );
        assert_eq!(
            final_leg_rotation_chain(Some(cached), perturbed_by_second_solve, true),
            perturbed_by_second_solve
        );
        assert_eq!(
            final_leg_rotation_chain(None, perturbed_by_second_solve, false),
            perturbed_by_second_solve
        );
    }

    #[test]
    fn airborne_foot_orientation_releases_at_a_bounded_angular_speed() {
        let previous = Quat::IDENTITY;
        let desired = Quat::from_rotation_x(90.0_f32.to_radians());
        let advanced = advance_airborne_foot_rotation(
            Some(previous),
            desired,
            1.0 / 64.0,
            AIRBORNE_FOOT_ROTATION_SPEED_DEGREES,
        );

        assert!((previous.angle_between(advanced).to_degrees() - 9.0).abs() < 0.0001);
        assert!(advanced.angle_between(desired) < previous.angle_between(desired));
        assert_eq!(
            advance_airborne_foot_rotation(
                Some(advanced),
                desired,
                0.0,
                AIRBORNE_FOOT_ROTATION_SPEED_DEGREES,
            ),
            advanced
        );
        assert_eq!(
            advance_airborne_foot_rotation(
                None,
                desired,
                1.0 / 64.0,
                AIRBORNE_FOOT_ROTATION_SPEED_DEGREES,
            ),
            desired
        );
    }

    #[test]
    fn run_contact_approach_reaches_the_plant_at_support_entry() {
        let radius = RUN_LOCOMOTION_PROFILE.support_phase_radius;
        let ready = radius + RUN_CONTACT_CHAIN_SETTLE_PHASE;
        assert_eq!(
            run_contact_approach_progress(
                RUN_CONTACT_APPROACH_PHASE,
                RUN_CONTACT_APPROACH_PHASE,
                ready,
            ),
            0.0
        );
        assert_eq!(
            run_contact_approach_progress(ready, RUN_CONTACT_APPROACH_PHASE, ready),
            1.0
        );
        assert_eq!(
            run_contact_approach_progress(radius, RUN_CONTACT_APPROACH_PHASE, ready),
            1.0
        );
        let middle = run_contact_approach_progress(
            (RUN_CONTACT_APPROACH_PHASE + ready) * 0.5,
            RUN_CONTACT_APPROACH_PHASE,
            ready,
        );
        assert!((middle - 0.5).abs() < 0.0001);
        let release_finished_phase = 0.81;
        assert_eq!(
            run_contact_approach_progress(release_finished_phase, release_finished_phase, ready,),
            0.0
        );
        assert!(run_swing_clearance(radius, Some(1.0)) <= f32::EPSILON);
        assert!(run_swing_clearance(0.3375, Some(0.5)) > 0.08);

        let phase_step = gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, 5.5, 1.0 / 64.0);
        let mut phase_to_contact = RUN_CONTACT_APPROACH_PHASE;
        let mut previous_progress =
            run_contact_approach_progress(phase_to_contact, RUN_CONTACT_APPROACH_PHASE, ready);
        while phase_to_contact > ready {
            phase_to_contact = (phase_to_contact - phase_step).max(ready);
            let progress =
                run_contact_approach_progress(phase_to_contact, RUN_CONTACT_APPROACH_PHASE, ready);
            let three_metre_world_step = 3.0 * (progress - previous_progress);
            let root_step = 5.5 / 64.0;
            assert!((three_metre_world_step - root_step).abs() <= 0.095);
            previous_progress = progress;
        }
    }

    #[test]
    fn planned_run_contact_anticipates_a_bounded_pelvis_reach_drop() {
        let radius = RUN_LOCOMOTION_PROFILE.support_phase_radius;
        let ready = radius + RUN_CONTACT_CHAIN_SETTLE_PHASE;
        let early = run_contact_approach_progress(
            RUN_CONTACT_APPROACH_PHASE,
            RUN_CONTACT_APPROACH_PHASE,
            ready,
        );
        let late = run_contact_approach_progress(ready, RUN_CONTACT_APPROACH_PHASE, ready);
        assert_eq!(early, 0.0);
        assert_eq!(late, 1.0);

        let required_reach_shift = -0.11;
        let early_target =
            (required_reach_shift * early).clamp(-RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP, 0.0);
        let late_target =
            (required_reach_shift * late).clamp(-RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP, 0.0);
        assert_eq!(early_target, 0.0);
        assert_eq!(late_target, required_reach_shift);
        assert!(
            advance_scalar_at_speed(0.0, late_target, 1.0 / 64.0, RUN_PELVIS_CORRECTION_SPEED,)
                .abs()
                <= 0.01
        );
    }

    #[test]
    fn frozen_run_contact_is_reachable_through_predicted_downhill_stance() {
        // Production-sized 0.523 m + 0.430 m leg and the captured downhill
        // plan geometry that previously froze an unreachable -6.117 m plant.
        let upper = Vec3::new(0.1, 3.109, -2.847);
        let velocity = Vec3::NEG_Z * 5.5;
        let ready = RUN_LOCOMOTION_PROFILE.support_phase_radius + RUN_CONTACT_CHAIN_SETTLE_PHASE;
        let reach = 0.953;
        let phase_to_contact = 0.744;
        let travel_per_phase = ordinary_step_distance(5.5) * 2.0;
        let downhill = |xz: Vec2| Some(2.38 + xz.y * 0.08);
        let current_height = downhill(upper.xz()).unwrap();
        let predicted_roots = [
            phase_to_contact - RUN_LOCOMOTION_PROFILE.support_phase_radius,
            phase_to_contact,
            phase_to_contact + RUN_LOCOMOTION_PROFILE.support_phase_radius,
        ]
        .map(|remaining_phase| {
            let mut root = upper + Vec3::NEG_Z * (remaining_phase * travel_per_phase);
            root.y += downhill(root.xz()).unwrap() - current_height;
            root - Vec3::Y * RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP
        });
        let candidate = Vec3::new(0.1, 0.0, -6.117);
        let frozen = reachable_run_contact_target(
            candidate,
            upper,
            velocity,
            5.5,
            phase_to_contact,
            ready,
            reach,
            downhill,
        );
        assert!(frozen.is_finite());
        for predicted_root in predicted_roots {
            assert!(frozen.distance(predicted_root) <= reach + 0.001);
        }
        assert_eq!(
            frozen,
            reachable_run_contact_target(
                candidate,
                upper,
                velocity,
                5.5,
                phase_to_contact,
                ready,
                reach,
                downhill,
            )
        );

        let flat_predicted_center = upper + Vec3::NEG_Z * (phase_to_contact * travel_per_phase)
            - Vec3::Y * RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP;
        let flat_candidate = flat_predicted_center + Vec3::new(0.1, -0.5, 0.0);
        let flat_height = flat_candidate.y - MEASURED_ANKLE_SOLE_OFFSET_METRES;
        let flat = reachable_run_contact_target(
            flat_candidate,
            upper,
            velocity,
            5.5,
            phase_to_contact,
            ready,
            reach,
            |_| Some(flat_height),
        );
        assert!(flat.distance(flat_candidate) <= 0.0001);
    }

    #[test]
    fn run_swing_end_and_first_support_sample_share_target_and_pole() {
        let planted = Vec3::new(0.1, 1.97, -7.477);
        let authored_upper = Vec3::new(0.1, 3.04, -6.25);
        let pelvis_shift = (0..20).fold(0.0, |shift, _| {
            advance_scalar_at_speed(
                shift,
                -RUN_MAXIMUM_PLANNED_REACH_PELVIS_DROP,
                1.0 / 64.0,
                RUN_PELVIS_CORRECTION_SPEED,
            )
        });
        let upper = authored_upper + Vec3::Y * pelvis_shift;
        let reach = 0.953;
        let swing_end =
            acquisition_planted_target(planted, upper, reach, LocomotionGait::Run, false);
        let first_acquired =
            acquisition_planted_target(planted, upper, reach, LocomotionGait::Run, true);
        assert_eq!(swing_end, planted);
        assert_eq!(first_acquired, swing_end);

        let authored_knee = upper + Vec3::new(0.0, -0.52, -0.05);
        let authored_foot = authored_knee + Vec3::new(0.0, -0.43, -0.04);
        let pole = Vec3::NEG_Z;
        let before = solve_two_bone_with_reach(
            upper,
            authored_knee,
            authored_foot,
            swing_end,
            0.523,
            0.430,
            pole,
            reach,
        )
        .unwrap();
        let after = solve_two_bone_with_reach(
            upper,
            authored_knee,
            authored_foot,
            first_acquired,
            0.523,
            0.430,
            pole,
            reach,
        )
        .unwrap();
        assert!(before.knee.distance(after.knee) <= f32::EPSILON);
        assert!(before.end.distance(after.end) <= f32::EPSILON);
    }

    #[test]
    fn shallow_acquisition_pole_survives_support_confidence_ramp() {
        let canonical = Vec3::new(-0.177153, 0.0, -0.984183);
        let shallow = Vec3::new(-0.999273, 0.038100, -0.001613);
        assert!(shallow.normalize().dot(canonical) < 0.2);
        let retained = retained_terrain_pole(shallow, canonical).unwrap();
        assert!(retained.dot(canonical) > 0.0);

        let first_root = Vec3::new(-0.100270, 2.863136, -10.316_13);
        let next_root = Vec3::new(-0.100349, 2.875328, -10.407523);
        let target = Vec3::new(-0.120271, 2.308135, -11.034_69);
        let authored_knee = first_root + Vec3::new(0.0, -0.52, -0.05);
        let authored_foot = authored_knee + Vec3::new(0.0, -0.43, -0.04);
        let terrain_reach = terrain_maximum_reach(0.523, 0.430);
        let first = solve_two_bone_with_reach(
            first_root,
            authored_knee,
            authored_foot,
            target,
            0.523,
            0.430,
            retained,
            terrain_reach,
        )
        .unwrap();
        let next = solve_two_bone_with_reach(
            next_root,
            authored_knee + (next_root - first_root),
            authored_foot + (next_root - first_root),
            target,
            0.523,
            0.430,
            retained,
            terrain_reach,
        )
        .unwrap();
        let root_relative_step = (next.knee - next_root).distance(first.knee - first_root);
        assert!(root_relative_step <= 0.10);

        let previous_direction = (target - first_root).normalize();
        let next_direction = (target - next_root).normalize();
        let transported = transported_terrain_pole(
            Some(retained),
            Some(previous_direction),
            next_direction,
            canonical,
        )
        .unwrap();
        assert!(
            transported.dot(next_direction).abs()
                <= retained.dot(previous_direction).abs() + 0.0001
        );
    }

    #[test]
    fn attack_knee_bend_parallel_transports_with_the_leg() {
        let previous_end = Vec3::NEG_Y;
        let remembered = Vec3::Z;
        let next_end = Vec3::X;
        let expected = Quat::from_rotation_arc(previous_end, next_end) * remembered;
        let pole = stabilized_knee_pole(
            Some(remembered),
            Some(previous_end),
            Vec3::ZERO,
            Vec3::new(0.0, -0.5, 0.1),
            next_end,
            expected,
            None,
        )
        .unwrap();

        assert!(pole.dot(expected) > 0.999);
        assert!(pole.dot(next_end).abs() < 0.0001);
    }

    #[test]
    fn attack_knee_bend_survives_a_straight_leg_singularity() {
        let previous_end = Vec3::NEG_Y;
        let remembered = Vec3::Z;
        let next_target = Vec3::new(0.02, -1.0, 0.0).normalize();
        let expected = Quat::from_rotation_arc(previous_end, next_target) * remembered;
        let pole = stabilized_knee_pole(
            Some(remembered),
            Some(previous_end),
            Vec3::ZERO,
            next_target * 0.5,
            next_target,
            Vec3::Z,
            None,
        )
        .unwrap();

        assert!(pole.dot(expected) > 0.999);
        assert!(pole.dot(Vec3::Z) > 0.0);
    }

    #[test]
    fn attack_knee_bend_rejects_an_inward_authored_pole() {
        let pole = stabilized_knee_pole(
            None,
            None,
            Vec3::ZERO,
            Vec3::new(0.0, -0.5, -0.2),
            Vec3::NEG_Y,
            Vec3::Z,
            None,
        )
        .unwrap();

        assert!(pole.dot(Vec3::Z) > 0.999);
    }

    #[test]
    fn attack_knee_bend_retains_the_pre_attack_rendered_pole() {
        let remembered = Vec3::new(0.3, 0.0, 0.95).normalize();
        let pole = stabilized_knee_pole(
            Some(remembered),
            None,
            Vec3::ZERO,
            Vec3::new(0.0, -0.5, 0.4),
            Vec3::NEG_Y,
            Vec3::Z,
            None,
        )
        .unwrap();

        assert!(pole.dot(remembered) > 0.999);
    }

    #[test]
    fn knee_pole_is_clamped_to_pi_over_eight_from_foot_facing() {
        let leg_direction = Vec3::NEG_Y;
        let foot_facing = Vec3::Z;
        let pole = constrain_knee_pole_to_foot_facing(
            Vec3::X,
            leg_direction,
            foot_facing,
            std::f32::consts::FRAC_PI_8,
        )
        .unwrap();

        assert!(pole.dot(leg_direction).abs() < 0.0001);
        assert!(pole.xz().angle_to(foot_facing.xz()).abs() <= std::f32::consts::FRAC_PI_8 + 0.0001);
    }

    #[test]
    fn diagonal_leg_cannot_rotate_clamped_pole_yaw_sideways() {
        let leg_direction = Vec3::new(0.0, -0.3, 0.954).normalize();
        let foot_facing = Vec3::Z;
        let pole = constrain_knee_pole_to_foot_facing(
            Vec3::X,
            leg_direction,
            foot_facing,
            std::f32::consts::FRAC_PI_8,
        )
        .unwrap();

        assert!(pole.dot(leg_direction).abs() < 0.0001);
        assert!(pole.xz().angle_to(foot_facing.xz()).abs() <= std::f32::consts::FRAC_PI_8 + 0.0001);
    }

    #[test]
    fn knee_pole_near_the_center_of_the_foot_facing_cone_is_stable() {
        let leg_direction = Vec3::NEG_Y;
        let expected = Quat::from_rotation_y(0.02) * Vec3::Z;
        let pole = constrain_knee_pole_to_foot_facing(
            expected,
            leg_direction,
            Vec3::Z,
            std::f32::consts::FRAC_PI_8,
        )
        .unwrap();

        assert!(pole.dot(expected) > 0.9999);
    }

    #[test]
    fn knee_pole_crosses_horizontal_leg_axis_without_a_hemisphere_flip() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut presented = Vec3::X;
        let mut velocity = 0.0;
        let mut acceleration = 0.0;
        for sample in 0..=128 {
            let y = -0.20 + 0.40 * sample as f32 / 128.0;
            let axis = Vec3::new(0.0, y, 1.0).normalize();
            let target = constrain_knee_pole_to_foot_facing(
                Vec3::X,
                axis,
                Vec3::Z,
                KNEE_POLE_MAX_FOOT_FACING_OFFSET_RADIANS,
            )
            .unwrap();
            let previous_presented = presented;
            let previous_acceleration = acceleration;
            (presented, velocity, acceleration) =
                track_anatomical_pole_signed(presented, target, axis, velocity, acceleration, dt);
            assert!(acceleration.abs() <= ANATOMICAL_POLE_MAXIMUM_ACCELERATION + 0.0001);
            assert!(
                ((acceleration - previous_acceleration) / dt).abs()
                    <= ANATOMICAL_POLE_MAXIMUM_JERK + 0.0001
            );
            assert!(previous_presented.dot(presented) >= 0.0);
        }
    }

    #[test]
    fn knee_pole_cone_limit_is_smooth_and_never_exceeds_the_limit() {
        let maximum = std::f32::consts::FRAC_PI_8;
        let epsilon = 0.000_1;
        let sample = maximum * 0.8;
        let derivative_before = (smooth_cone_limit(sample, maximum)
            - smooth_cone_limit(sample - epsilon, maximum))
            / epsilon;
        let derivative_after = (smooth_cone_limit(sample + epsilon, maximum)
            - smooth_cone_limit(sample, maximum))
            / epsilon;
        assert!((derivative_before - derivative_after).abs() < 0.01);
        assert_eq!(smooth_cone_limit(maximum * 0.5, maximum), maximum * 0.5);
        assert!(smooth_cone_limit(maximum * 20.0, maximum) <= maximum);
        assert!(smooth_cone_limit(-maximum * 20.0, maximum) >= -maximum);
    }

    #[test]
    fn guard_target_tracker_bounds_jerk_and_reuses_the_same_fixed_tick_output() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut track = GuardTargetTrack {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            ideal_velocity: Vec3::ZERO,
            ideal_acceleration: Vec3::ZERO,
            ideal_history_valid: true,
            replan: None,
        };
        let desired = Vec3::new(0.4, 0.1, -0.3);
        for _ in 0..24 {
            let previous_acceleration = track.acceleration;
            track = advance_guard_foot_target(
                Some(track.position),
                track.velocity,
                track.acceleration,
                Some(desired),
                Vec3::ZERO,
                Vec3::ZERO,
                true,
                desired,
                dt,
                true,
            );
            let jerk = (track.acceleration - previous_acceleration) / dt;
            assert!(jerk.length() <= FOOT_FOLLOWER_MAXIMUM_JERK + 0.001);
        }
        let repeated = advance_guard_foot_target(
            Some(track.position),
            track.velocity,
            track.acceleration,
            Some(desired),
            Vec3::ZERO,
            Vec3::ZERO,
            track.ideal_history_valid,
            desired,
            dt,
            false,
        );
        assert_eq!(repeated.position, track.position);
        assert_eq!(repeated.velocity, track.velocity);
        assert_eq!(repeated.acceleration, track.acceleration);
        assert!(track.position.distance(desired) < 0.2);
    }

    #[test]
    fn guard_target_tracker_follows_a_lawfully_ramped_sprint_target() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut desired = Vec3::ZERO;
        let mut previous_desired = desired;
        let mut track = GuardTargetTrack {
            position: desired,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            ideal_velocity: Vec3::ZERO,
            ideal_acceleration: Vec3::ZERO,
            ideal_history_valid: true,
            replan: None,
        };
        let mut target_speed = 0.0_f32;
        for _ in 0..512 {
            let previous_position = track.position;
            let previous_acceleration = track.acceleration;
            target_speed = (target_speed + 2.0 * dt).min(5.5);
            desired.x += target_speed * dt;
            track = advance_guard_foot_target(
                Some(track.position),
                track.velocity,
                track.acceleration,
                Some(previous_desired),
                track.ideal_velocity,
                track.ideal_acceleration,
                track.ideal_history_valid,
                desired,
                dt,
                true,
            );
            let jerk = (track.acceleration - previous_acceleration) / dt;
            assert!(jerk.length() <= FOOT_FOLLOWER_MAXIMUM_JERK + 0.001);
            assert!(track.acceleration.length() <= FOOT_FOLLOWER_MAXIMUM_ACCELERATION + 0.001);
            assert!(track.position.distance(desired) <= 0.05);
            assert!(track.position.x >= previous_position.x);
            assert!(
                track.position.x <= desired.x + 0.001,
                "follower {} crossed ideal {} at speed {target_speed}",
                track.position.x,
                desired.x,
            );
            previous_desired = desired;
        }
        assert!(track.position.distance(desired) <= 0.025);
    }

    #[test]
    fn typed_pose_error_rejects_invalid_values() {
        assert!(MaximumPoseError::new(0.05).is_some());
        assert!(MaximumPoseError::new(-0.001).is_none());
        assert!(MaximumPoseError::new(f32::NAN).is_none());
        assert!(MaximumPoseError::new(f32::INFINITY).is_none());
    }

    #[test]
    fn sprint_follower_catches_up_without_velocity_clamping_at_64_and_128_hz() {
        for sample_hz in [64.0_f32, 128.0] {
            let dt = 1.0 / sample_hz;
            let speed = 5.5;
            let mut ideal_position = Vec3::ZERO;
            let mut state = FootFollowerState::from_presented_pose(
                Vec3::new(-0.04, 0.0, 0.0),
                Vec3::X * speed,
                Vec3::ZERO,
                ideal_position,
                Vec3::X * speed,
                Vec3::ZERO,
            )
            .unwrap();
            let mut maximum_speed = speed;
            for _ in 0..(sample_hz as usize * 2) {
                ideal_position.x += speed * dt;
                let sample =
                    WorldFootTargetSample::new(ideal_position, Vec3::X * speed, Vec3::ZERO)
                        .unwrap();
                let previous_acceleration = state.acceleration;
                let outcome = advance_foot_follower(
                    state,
                    IdealFootTarget::WorldSwing(sample),
                    FootFollowerLimits::animation(None, None),
                    dt,
                );
                state = outcome.presented_state().unwrap();
                let jerk = (state.acceleration - previous_acceleration) / dt;
                assert!(jerk.length() <= FOOT_FOLLOWER_MAXIMUM_JERK + 0.001);
                assert!(state.acceleration.length() <= FOOT_FOLLOWER_MAXIMUM_ACCELERATION + 0.001);
                assert!(state.position.x <= ideal_position.x + 0.001);
                assert!(state.position.distance(ideal_position) <= 0.05);
                maximum_speed = maximum_speed.max(state.velocity.length());
            }
            assert!(state.position.distance(ideal_position) <= 0.025);
            assert!(maximum_speed > 5.6);
        }
    }

    #[test]
    fn follower_reports_reach_warning_before_hard_limit() {
        let current = FootFollowerState::from_presented_pose(
            Vec3::X * 0.81,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X * 0.81,
            Vec3::ZERO,
            Vec3::ZERO,
        )
        .unwrap();
        let reach = FootReachEnvelope::new(Vec3::ZERO, Vec3::ZERO, 0.8, 0.9).unwrap();
        let outcome = advance_foot_follower(
            current,
            IdealFootTarget::world_plant(current.position).unwrap(),
            FootFollowerLimits::animation(Some(reach), None),
            1.0 / 64.0,
        );
        assert!(matches!(
            outcome,
            FootFollowOutcome::NeedsReleaseOrReplan {
                reason: FootFollowReason::ReachWarning,
                ..
            }
        ));
    }

    #[test]
    fn hard_reach_continues_a_jerk_bounded_inward_recovery_for_semantic_release() {
        let current = FootFollowerState::from_presented_pose(
            Vec3::X * 0.91,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X * 0.91,
            Vec3::ZERO,
            Vec3::ZERO,
        )
        .unwrap();
        let reach = FootReachEnvelope::new(Vec3::ZERO, Vec3::ZERO, 0.8, 0.9).unwrap();
        let outcome = advance_foot_follower(
            current,
            IdealFootTarget::world_plant(current.position).unwrap(),
            FootFollowerLimits::animation(Some(reach), None),
            1.0 / 64.0,
        );
        assert!(matches!(
            outcome,
            FootFollowOutcome::NeedsReleaseOrReplan {
                presented_state,
                reason: FootFollowReason::ReachHardLimit,
                ..
            } if presented_state.position.x < current.position.x
                && presented_state.acceleration.length()
                    <= FOOT_FOLLOWER_MAXIMUM_JERK / CONTINUITY_SAMPLE_HZ + 0.001
        ));
    }

    #[test]
    fn hard_reach_emergency_recovery_may_advance_only_toward_the_hip() {
        let current = FootFollowerState::from_presented_pose(
            Vec3::X * 0.91,
            Vec3::NEG_X,
            Vec3::ZERO,
            Vec3::X * 0.765625,
            Vec3::NEG_X,
            Vec3::ZERO,
        )
        .unwrap();
        let reach = FootReachEnvelope::new(Vec3::ZERO, Vec3::ZERO, 0.8, 0.9).unwrap();
        let inward = WorldFootTargetSample::new(Vec3::X * 0.75, Vec3::NEG_X, Vec3::ZERO)
            .map(IdealFootTarget::WorldSwing)
            .unwrap();
        let outcome = advance_foot_follower(
            current,
            inward,
            FootFollowerLimits::animation(Some(reach), None),
            1.0 / 64.0,
        );
        assert!(matches!(
            outcome,
            FootFollowOutcome::NeedsReleaseOrReplan {
                presented_state,
                reason: FootFollowReason::ReachHardLimit,
                ..
            } if presented_state.position.length() < current.position.length()
        ));
    }

    #[test]
    fn deadline_accounts_for_acceleration_away_from_contact() {
        let ideal_position = Vec3::X * 0.04;
        let current = FootFollowerState::from_presented_pose(
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::NEG_X * 10.0,
            ideal_position,
            Vec3::ZERO,
            Vec3::ZERO,
        )
        .unwrap();
        let outcome = advance_foot_follower(
            current,
            IdealFootTarget::world_plant(ideal_position).unwrap(),
            FootFollowerLimits::animation(None, Some(0.01)),
            1.0 / 64.0,
        );
        assert!(matches!(
            outcome,
            FootFollowOutcome::NeedsReleaseOrReplan {
                reason: FootFollowReason::ContactDeadline,
                ..
            }
        ));
    }

    #[test]
    fn discontinuous_runtime_target_invalidates_history_then_resyncs_once() {
        let mut memory = LegIkMemory {
            left_foot_follower: FootFollowerState::from_presented_pose(
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
            ),
            ..default()
        };
        let target = Vec3::X * 0.2;
        let ideal = WorldFootTargetSample::new(target, Vec3::ZERO, Vec3::ZERO)
            .map(IdealFootTarget::WorldSwing)
            .unwrap();
        let (_, first) = advance_runtime_foot_target(
            &mut memory,
            true,
            Vec3::ZERO,
            ideal,
            Vec3::ZERO,
            None,
            None,
            1.0 / 64.0,
            true,
        );
        assert!(matches!(
            first,
            Some((FootFollowReason::DiscontinuousTarget, _))
        ));
        assert!(memory.left_foot_follower.is_none());
        let (_, second) = advance_runtime_foot_target(
            &mut memory,
            true,
            Vec3::ZERO,
            ideal,
            Vec3::ZERO,
            None,
            None,
            1.0 / 64.0,
            true,
        );
        assert!(!matches!(
            second,
            Some((FootFollowReason::DiscontinuousTarget, _))
        ));
        assert!(memory.left_foot_follower.is_some());
    }

    #[test]
    fn runtime_follower_never_replaces_a_different_presented_handoff_pose() {
        let presented = Vec3::new(2.2413998, 1.9726762, -10.548973);
        let stale = Vec3::new(1.12, 2.40, -10.49);
        let mut memory = LegIkMemory {
            right_foot_follower: FootFollowerState::from_presented_pose(
                stale,
                Vec3::splat(8.0),
                Vec3::splat(16.0),
                stale,
                Vec3::splat(8.0),
                Vec3::splat(16.0),
            ),
            ..default()
        };
        let ideal = IdealFootTarget::world_plant(presented).unwrap();

        let (tracked, _) = advance_runtime_foot_target(
            &mut memory,
            false,
            presented,
            ideal,
            Vec3::ZERO,
            None,
            None,
            1.0 / CONTINUITY_SAMPLE_HZ,
            true,
        );

        assert_eq!(tracked, presented);
        assert_eq!(memory.right_foot_follower.unwrap().position, presented);
        assert!(presented.distance(stale) > 1.0);
    }

    #[test]
    fn lead_only_guard_handoff_retains_target_derivatives() {
        assert!(retain_guard_tracker_on_reinitialization(
            true, true, false, false
        ));
        assert!(!retain_guard_tracker_on_reinitialization(
            true, true, true, false
        ));
        assert!(!retain_guard_tracker_on_reinitialization(
            true, true, false, true
        ));
    }

    #[test]
    fn airborne_terrain_reset_preserves_fixed_tick_ownership_and_anatomical_diagnostics() {
        let mut memory = LegIkMemory {
            evaluation_tick: Some(40),
            knee_yaw_evaluation_tick: Some(41),
            left_knee_foot_yaw_offset_degrees: 3.0,
            right_knee_foot_yaw_offset_degrees: -4.0,
            left_anatomical_pole_world: Some(Vec3::Z),
            right_anatomical_pole_world: Some(Vec3::NEG_Z),
            left_foot_world_target: Some(Vec3::ONE),
            ..default()
        };
        reset_terrain_ik_preserving_anatomical_evaluation(&mut memory);
        assert_eq!(memory.evaluation_tick, Some(40));
        assert_eq!(memory.knee_yaw_evaluation_tick, Some(41));
        assert_eq!(memory.left_knee_foot_yaw_offset_degrees, 3.0);
        assert_eq!(memory.right_knee_foot_yaw_offset_degrees, -4.0);
        assert_eq!(memory.left_anatomical_pole_world, Some(Vec3::Z));
        assert_eq!(memory.right_anatomical_pole_world, Some(Vec3::NEG_Z));
        assert_eq!(memory.left_foot_world_target, None);
    }

    #[test]
    fn invalid_terrain_posture_yields_shared_diagnostics_to_quickstep_only() {
        assert!(invalid_terrain_posture_has_downstream_leg_owner(
            SkeletonAction::Dodge,
            false,
        ));
        for action in [
            SkeletonAction::None,
            SkeletonAction::Attack,
            SkeletonAction::Block,
        ] {
            assert!(!invalid_terrain_posture_has_downstream_leg_owner(
                action, false
            ));
        }
        assert!(invalid_terrain_posture_has_downstream_leg_owner(
            SkeletonAction::None,
            true,
        ));
    }

    #[test]
    fn propagated_raised_support_cannot_overwrite_quickstep_ownership() {
        let mut skeleton = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        let mut raised = RaisedFootworkState {
            initialized: true,
            ..default()
        };
        assert!(raised_refresh_owns_support(&skeleton, &raised));

        skeleton
            .begin_dodge(DodgeSpec { direction: Vec2::Y }, 4, 5)
            .unwrap();
        assert!(!raised_refresh_owns_support(&skeleton, &raised));
        raised.release_handoff_active = true;
        assert!(!raised_refresh_owns_support(&skeleton, &raised));

        skeleton.transition_body(BodyState::Prone);
        assert!(!raised_refresh_owns_support(&skeleton, &raised));
    }

    #[test]
    fn guard_pivot_follows_an_arc_around_the_body() {
        let origin = Vec3::ZERO;
        let start = Vec3::new(-0.3, 0.1, 0.0);
        let end = Vec3::new(0.0, 0.1, 0.3);
        let support = Vec3::new(0.3, 0.1, 0.0);
        let midpoint = guard_pivot_target(start, end, origin, support, 0.5);

        assert!((midpoint.xz().length() - 0.3).abs() < 0.0001);
        assert!(midpoint.y > start.y);
        assert!(midpoint.x < 0.0 && midpoint.z > 0.0);
        assert!(midpoint.xz().distance(support.xz()) >= GUARD_TARGET_INTER_FOOT_SEPARATION);
    }

    #[test]
    fn repeated_stationary_pivots_latch_slope_contacts_without_height_accumulation() {
        let origin = Vec3::new(3.0, 1.0, -2.0);
        let rotation = Quat::from_rotation_y(0.45);
        let terrain_height = |xz: Vec2| 0.35 + xz.x * 0.07 - xz.y * 0.04;
        let contact = |xz: Vec2| {
            Vec3::new(
                xz.x,
                terrain_height(xz) + MEASURED_ANKLE_SOLE_OFFSET_METRES,
                xz.y,
            )
        };
        let mut left = contact((origin + rotation * Vec3::new(-0.24, 0.0, 0.08)).xz());
        let mut right = contact((origin + rotation * Vec3::new(0.24, 0.0, -0.08)).xz());

        for index in 0..12 {
            let pivot_left = index % 2 == 0;
            let support = if pivot_left { right } else { left };
            let side = if pivot_left { -1.0 } else { 1.0 };
            let authored = origin
                + rotation
                    * Vec3::new(
                        side * (0.30 + index as f32 * 0.002),
                        0.42,
                        0.18 - index as f32 * 0.01,
                    );
            let constrained =
                constrain_guard_swing_to_live_corridor(authored, support, origin, rotation, side);
            let height = terrain_height(constrained.xz());
            let endpoint = stationary_guard_pivot_endpoint(constrained, Some(height))
                .expect("sampled slope admits a stationary contact");

            assert!(sole_is_at_contact(endpoint.y, height));
            assert!(sole_is_at_contact(support.y, terrain_height(support.xz())));
            assert!(
                (endpoint.y - height - MEASURED_ANKLE_SOLE_OFFSET_METRES).abs() <= f32::EPSILON
            );

            let start = if pivot_left { left } else { right };
            let mut presented = start;
            let mut previous_acceleration = Vec3::ZERO;
            let mut landed = false;
            for tick in 1..=96 {
                let progress = (tick as f32 / (GUARD_PIVOT_STEP_SECONDS * CONTINUITY_SAMPLE_HZ))
                    .clamp(0.0, 1.0);
                let sample = guard_boundary_quintic_sample(
                    start,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    endpoint,
                    progress,
                    GUARD_PIVOT_STEP_SECONDS,
                    GUARD_PIVOT_LIFT_METRES,
                );
                let jerk = (sample.acceleration - previous_acceleration) * CONTINUITY_SAMPLE_HZ;
                assert!(
                    sample.acceleration.length()
                        <= GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION + 0.01
                );
                assert!(jerk.length() <= GUARD_ACTION_SEGMENT_MAXIMUM_JERK + 1.0);
                presented = sample.position;
                previous_acceleration = sample.acceleration;

                // The opposite planted foot remains truthful support while
                // the pivot foot follows its directly presented analytic arc;
                // dual-zero support is never published. Promotion waits for
                // the propagated result to return its sole to terrain.
                assert!(sole_is_at_contact(support.y, terrain_height(support.xz())));
                landed = progress >= 1.0
                    && stationary_guard_pivot_has_landed(
                        presented,
                        endpoint,
                        Some(terrain_height(presented.xz())),
                    );
                if landed {
                    break;
                }
            }
            assert!(
                landed,
                "pivot {index} never returned its rendered sole to contact"
            );
            if pivot_left {
                left = presented;
            } else {
                right = presented;
            }
        }

        assert!(stationary_guard_pivot_endpoint(left, None).is_none());
        assert!(sole_is_at_contact(left.y, terrain_height(left.xz())));
        assert!(sole_is_at_contact(right.y, terrain_height(right.xz())));
        // A subsequent moving acquisition therefore inherits two grounded
        // visible plants rather than the elevated authored pivot targets.
        assert!(raised_support_is_actual(
            true,
            true,
            left.y,
            terrain_height(left.xz())
        ));
        assert!(raised_support_is_actual(
            true,
            true,
            right.y,
            terrain_height(right.xz())
        ));
    }

    #[test]
    fn stationary_guard_recovers_the_airborne_foot_before_repositioning_a_contact() {
        assert!(stationary_guard_pivot_side(
            0.24, 0.11, false, true, 0.40, 0.30,
        ));
        assert!(!stationary_guard_pivot_side(
            0.24, 0.11, true, false, 0.40, 0.30,
        ));
        assert!(
            stationary_guard_pivot_side(0.24, 0.11, false, false, 0.40, 0.30),
            "when both contacts are absent the ordinary balance ordering remains deterministic",
        );
    }

    #[test]
    fn guard_swing_arch_has_bounded_c2_cadence_endpoints() {
        assert_eq!(c2_swing_arch(0.0), 0.0);
        assert_eq!(c2_swing_arch(1.0), 0.0);
        assert!((c2_swing_arch(0.5) - 1.0).abs() < 0.000001);
        let sample = 1.0 / CONTINUITY_SAMPLE_HZ;
        assert!(c2_swing_arch(sample) * 0.10 < 0.001);
        assert!(c2_swing_arch(1.0 - sample) * 0.10 < 0.001);
    }

    fn stationary_hip_trajectory(
        reach: FootReachEnvelope,
        delta_seconds: f32,
    ) -> PredictedHipTrajectory {
        PredictedHipTrajectory::from_retained_motion(
            reach,
            None,
            Vec3::ZERO,
            delta_seconds,
            0.0,
            0.0,
        )
        .unwrap()
    }

    #[test]
    fn guard_replan_resets_a_local_segment_instead_of_promoting_a_waypoint_to_endpoint() {
        let presented = Vec3::new(-0.2, 1.9, -0.4);
        let original_endpoint = Vec3::new(-0.26, 1.94, -0.58);
        let bounded_next_waypoint = presented + Vec3::new(-0.01, 0.0, -0.02);
        let root = Vec3::new(0.0, 1.2, -0.5);
        let reach = FootReachEnvelope::new(root, root, 0.95, 0.96).unwrap();
        let FootEndpointPlan::Segment(segment) = guard_swing_replan_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            original_endpoint,
            0.0,
            Some(stationary_hip_trajectory(reach, 1.0 / CONTINUITY_SAMPLE_HZ)),
        ) else {
            panic!("expected feasible fixed endpoint");
        };

        assert_eq!(segment.start, presented);
        assert_eq!(segment.end.position(), original_endpoint);
        assert_ne!(segment.end.position(), bounded_next_waypoint);
        assert_eq!(segment.timing.progress(), 0.0);
        assert_eq!(
            segment.start.lerp(
                segment.end.position(),
                quintic_progress(segment.timing.progress()),
            ),
            presented
        );
        assert!(segment.timing.duration_seconds() >= GUARD_FORCED_RELEASE_SECONDS);
    }

    #[test]
    fn active_guard_segment_owns_the_cadence_edge_until_its_terminal_sample() {
        assert!(!guard_cadence_may_turnover(true, 1, false, true));
        assert!(!guard_cadence_may_turnover(true, 1, true, false));
        assert!(guard_cadence_may_turnover(true, 1, false, false));
        assert!(!guard_stationary_owns_pose(true, true));
        assert!(guard_stationary_owns_pose(true, false));
    }

    #[test]
    fn guard_movement_acquisition_uses_replicated_swing_not_stale_pivot_side() {
        let left = Vec3::new(-0.2, 0.1, 0.0);
        let right = Vec3::new(0.2, 0.1, 0.0);
        for replicated_swing_left in [false, true] {
            for pivot_active in [false, true] {
                for stale_pivot_left in [false, true] {
                    let mut footwork = RaisedFootworkState {
                        initialized: true,
                        pivot_active,
                        pivot_left: stale_pivot_left,
                        swing_left: !replicated_swing_left,
                        step_sequence: 17,
                        swing_release_owner_active: true,
                        awaiting_step_sequence: true,
                        swing_emergency_brake: Some(EmergencyFootBrake {
                            stationary_ideal: Vec3::splat(9.0),
                            owner_local_ideal: None,
                        }),
                        ..default()
                    };
                    adopt_guard_movement_identity(
                        &mut footwork,
                        replicated_swing_left,
                        17,
                        left,
                        right,
                    );
                    assert_eq!(footwork.swing_left, replicated_swing_left);
                    assert_eq!(
                        footwork.swing_start,
                        if replicated_swing_left { left } else { right }
                    );
                    assert_eq!(footwork.step_sequence, 17);
                    assert!(!footwork.awaiting_step_sequence);
                    assert!(!footwork.swing_release_owner_active);
                    assert!(footwork.swing_emergency_brake.is_none());
                }
            }
        }
    }

    #[test]
    fn semantic_clock_and_c2_span_are_independent_of_render_frequency() {
        fn simulate(render_hz: f32) -> (u64, u32) {
            let mut clock = ProceduralAnimationClock::default();
            let mut segment = C2FootSegment {
                start: Vec3::ZERO,
                start_velocity: Vec3::ZERO,
                start_acceleration: Vec3::ZERO,
                end: FootSegmentEndpoint::Release(
                    FeasibleReleaseEndpoint::for_predicted_release(
                        Vec3::X,
                        stationary_hip_trajectory(
                            FootReachEnvelope::new(Vec3::ZERO, Vec3::ZERO, 2.0, 2.1).unwrap(),
                            1.0 / CONTINUITY_SAMPLE_HZ,
                        ),
                        1.0,
                    )
                    .unwrap(),
                ),
                timing: SegmentTickSpan::new(64).unwrap(),
                owner_epoch: 0,
            };
            let frames = render_hz.round() as usize;
            let mut previous_tick = 0;
            for _ in 0..frames {
                clock.advance_gameplay(1.0 / render_hz);
                let tick = clock.semantic_step().0;
                let advances = tick != previous_tick;
                advance_c2_segment_tick(&mut segment, advances, tick);
                previous_tick = tick;
            }
            (clock.semantic_step().0, segment.timing.elapsed_ticks)
        }
        for render_hz in [30.0, 60.0, 144.0] {
            assert_eq!(simulate(render_hz), (64, 64));
        }
    }

    #[test]
    fn moving_emergency_recovery_is_body_relative_not_a_trailing_world_hold() {
        let local_stance = Vec3::new(-0.2, -0.85, 0.1);
        let brake = EmergencyFootBrake {
            stationary_ideal: Vec3::splat(99.0),
            owner_local_ideal: Some(local_stance),
        };
        let rotation = Quat::from_rotation_y(0.4);
        let first_root = Vec3::new(1.0, 2.0, 3.0);
        let second_root = first_root + Vec3::new(0.0, 0.0, -0.25);
        let first = brake.target(first_root, rotation);
        let second = brake.target(second_root, rotation);
        assert!((second - first).abs_diff_eq(second_root - first_root, 0.000_001));
        assert!((rotation.inverse() * (second - second_root)).abs_diff_eq(local_stance, 0.000_001));
        assert!(second.distance(second_root) < 0.94);
    }

    #[test]
    fn terminal_guard_segment_defers_presented_and_ideal_history_publication() {
        let previous_solve = Vec3::new(-0.8, 2.5, -10.7);
        let previous_ideal = Vec3::new(-0.9, 2.4, -10.8);
        let previous_velocity = Vec3::new(-3.0, -3.5, -5.0);
        let endpoint = Vec3::new(-0.4, 2.3, -10.3);
        let trajectory = stationary_hip_trajectory(
            FootReachEnvelope::new(
                Vec3::new(-0.3, 2.8, -10.2),
                Vec3::new(-0.3, 2.8, -10.2),
                0.95,
                0.96,
            )
            .unwrap(),
            1.0 / CONTINUITY_SAMPLE_HZ,
        );
        let motion = C2FootSegment {
            start: previous_ideal,
            start_velocity: previous_velocity,
            start_acceleration: Vec3::X,
            end: FootSegmentEndpoint::Contact(
                FeasibleFootEndpoint::for_predicted_terrain_contact(endpoint, trajectory, 0.2)
                    .unwrap(),
            ),
            timing: SegmentTickSpan {
                elapsed_ticks: 13,
                total_ticks: NonZeroU32::new(13).unwrap(),
            },
            owner_epoch: 7,
        };
        let segment = PlannedGuardFootSegment {
            motion,
            reach: GuardSegmentReachProof::Retained(trajectory),
            recovery_to_contact: false,
        };
        let mut footwork = RaisedFootworkState {
            swing_left: true,
            swing_release_owner_active: true,
            swing_replan_segment: Some(segment),
            left_solve_target: Some(previous_solve),
            left_desired_target: Some(previous_ideal),
            left_target_velocity: previous_velocity,
            left_ideal_velocity: previous_velocity,
            left_ideal_acceleration: Vec3::X,
            left_ideal_history_valid: true,
            ..default()
        };

        complete_guard_segment_semantics(&mut footwork, segment, true, 24, true);

        assert_eq!(footwork.left_plant, endpoint);
        assert_eq!(footwork.swing_start, endpoint);
        assert_eq!(footwork.left_solve_target, Some(previous_solve));
        assert_eq!(footwork.left_desired_target, Some(previous_ideal));
        assert_eq!(footwork.left_target_velocity, previous_velocity);
        assert_eq!(footwork.left_ideal_velocity, previous_velocity);
        assert_eq!(footwork.left_ideal_acceleration, Vec3::X);
        assert!(footwork.left_ideal_history_valid);
        assert!(footwork.awaiting_step_sequence);
        assert!(!footwork.swing_release_owner_active);
        assert_eq!(
            footwork.swing_replan_segment.map(|planned| planned.motion),
            Some(segment.motion)
        );
    }

    #[test]
    fn cadence_takeover_requires_radial_stopping_reserve() {
        let reach = FootReachEnvelope::new(Vec3::ZERO, Vec3::ZERO, 0.92, 0.94).unwrap();
        let presented = Vec3::X * 0.80;
        assert!(!guard_motion_can_transfer_to_cadence(
            presented,
            Vec3::X * 6.0,
            Vec3::ZERO,
            Some(reach),
            1.0 / CONTINUITY_SAMPLE_HZ,
            ReachMotionSample::Current,
        ));
        assert!(guard_motion_can_transfer_to_cadence(
            presented,
            Vec3::X * 0.5,
            Vec3::ZERO,
            Some(reach),
            1.0 / CONTINUITY_SAMPLE_HZ,
            ReachMotionSample::Current,
        ));
    }

    #[test]
    fn support_releases_have_independent_reach_proven_c2_owners() {
        let reach = FootReachEnvelope::new(Vec3::Y, Vec3::Y, 0.92, 0.94).unwrap();
        let trajectory = stationary_hip_trajectory(reach, 1.0 / CONTINUITY_SAMPLE_HZ);
        let mut footwork = RaisedFootworkState::default();
        let left = Vec3::new(-0.2, 0.25, 0.1);
        let right = Vec3::new(0.2, 0.25, 0.1);
        let left_velocity = Vec3::new(0.4, -0.2, 0.1);
        let right_velocity = Vec3::new(-0.3, -0.1, 0.2);

        install_guard_support_release(
            &mut footwork,
            true,
            left,
            left_velocity,
            Vec3::ZERO,
            Some(trajectory),
        );
        install_guard_support_release(
            &mut footwork,
            false,
            right,
            right_velocity,
            Vec3::ZERO,
            Some(trajectory),
        );

        let Some(SupportReleaseOwner::Segment(left_segment)) = footwork.left_support_release_owner
        else {
            panic!("left support release must have a C2 owner");
        };
        let Some(SupportReleaseOwner::Segment(right_segment)) =
            footwork.right_support_release_owner
        else {
            panic!("right support release must have a C2 owner");
        };
        assert!(footwork.left_support_release_owner.is_some());
        assert!(footwork.right_support_release_owner.is_some());
        assert_eq!(left_segment.start, left);
        assert_eq!(right_segment.start, right);
        assert_eq!(left_segment.start_velocity, left_velocity);
        assert_eq!(right_segment.start_velocity, right_velocity);
        assert!(trajectory.contains_quintic_path(
            c2_segment_position_control(left_segment),
            left_segment.timing.duration_seconds(),
            true,
        ));
        assert!(trajectory.contains_quintic_path(
            c2_segment_position_control(right_segment),
            right_segment.timing.duration_seconds(),
            true,
        ));

        let hard_boundary = left + Vec3::X * 0.01;
        assert!(supersede_support_segment_with_emergency(
            &mut footwork,
            true,
            hard_boundary,
            None,
        ));
        assert!(!supersede_support_segment_with_emergency(
            &mut footwork,
            true,
            hard_boundary + Vec3::X,
            None,
        ));
        assert_eq!(
            footwork.left_support_release_owner,
            Some(SupportReleaseOwner::EmergencyBrake(EmergencyFootBrake {
                stationary_ideal: hard_boundary,
                owner_local_ideal: None,
            }))
        );
    }

    #[test]
    fn support_release_owner_blocks_cadence_until_terminal_motion_settles() {
        let endpoint = Vec3::new(0.2, 0.3, -0.4);
        let mut footwork = RaisedFootworkState {
            step_sequence: 8,
            left_support_release_owner: Some(SupportReleaseOwner::TerminalHold { endpoint }),
            ..default()
        };
        assert!(!guard_cadence_may_turnover(true, 1, true, false));

        complete_support_release_if_settled(
            &mut footwork,
            true,
            endpoint,
            Vec3::X,
            Vec3::ZERO,
            false,
            8,
            true,
        );
        assert!(footwork.left_support_release_owner.is_some());

        complete_support_release_if_settled(
            &mut footwork,
            true,
            endpoint,
            Vec3::ZERO,
            Vec3::ZERO,
            false,
            8,
            true,
        );
        assert!(footwork.left_support_release_owner.is_none());
        assert_eq!(footwork.left_plant, endpoint);
        assert_eq!(footwork.step_sequence, 8);
        assert!(footwork.awaiting_step_sequence);

        let mut stationary = RaisedFootworkState {
            right_support_release_owner: Some(SupportReleaseOwner::EmergencyBrake(
                EmergencyFootBrake {
                    stationary_ideal: endpoint,
                    owner_local_ideal: None,
                },
            )),
            awaiting_step_sequence: true,
            ..default()
        };
        complete_support_release_if_settled(
            &mut stationary,
            false,
            endpoint,
            Vec3::ZERO,
            Vec3::ZERO,
            false,
            0,
            false,
        );
        assert!(stationary.right_support_release_owner.is_none());
        assert!(!stationary.awaiting_step_sequence);
    }

    #[test]
    fn proven_contact_starts_the_opposite_swing_before_the_replicated_edge() {
        let left = Vec3::new(-0.18, 0.08, -0.25);
        let right = Vec3::new(0.18, 0.08, 0.12);
        let hip = Vec3::Y * 0.8;
        let reach = FootReachEnvelope::new(hip, hip, 0.92, 0.94).unwrap();
        let mut timing = SegmentTickSpan::new(4).unwrap();
        timing.elapsed_ticks = 4;
        let contact = PlannedGuardFootSegment {
            motion: C2FootSegment {
                start: right,
                start_velocity: Vec3::ZERO,
                start_acceleration: Vec3::ZERO,
                end: FootSegmentEndpoint::Contact(FeasibleFootEndpoint::from_proven_guard_contact(
                    right,
                )),
                timing,
                owner_epoch: 10,
            },
            reach: GuardSegmentReachProof::Retained(stationary_hip_trajectory(
                reach,
                1.0 / CONTINUITY_SAMPLE_HZ,
            )),
            recovery_to_contact: false,
        };
        let mut footwork = RaisedFootworkState {
            initialized: true,
            was_moving: true,
            swing_left: false,
            step_sequence: 4,
            swing_replan_segment: Some(contact),
            // A low render rate may coalesce over the exact semantic contact
            // tick, leaving the legacy edge-wait flag unset even though the
            // analytic owner is visibly and physically terminal.
            awaiting_step_sequence: false,
            left_plant: left,
            right_plant: right,
            ..default()
        };

        assert!(begin_next_guard_swing_after_contact(
            &mut footwork,
            true,
            0,
            left,
            right,
            0,
            Vec3::ZERO,
            Quat::IDENTITY,
            left,
            right,
        ));
        assert!(footwork.swing_left);
        assert_eq!(footwork.step_sequence, 4);
        assert_eq!(footwork.left_support_weight, 0.0);
        assert_eq!(footwork.right_support_weight, 1.0);
        assert_eq!(footwork.swing_start, left);
        assert!(footwork.swing_replan_segment.is_none());
        assert!(!footwork.awaiting_step_sequence);

        let current_edge = guard_cadence_contact_tick_span(0.49, 1.4).unwrap();
        let following_edge = guard_following_cadence_contact_tick_span(0.49, 1.4).unwrap();
        assert!(current_edge.total_ticks.get() <= 2);
        assert!(following_edge.total_ticks.get() >= 12);

        let mut proof = stationary_exact_guard_proof(Vec3::Y, 0.92, 0.94, 16);
        proof.sequence = 4;
        proof.trajectory_signature.sequence = 4;
        proof.swing_left = true;
        proof.accepts_preemptive_cadence_confirmation = true;
        footwork.swing_replan_segment = Some(PlannedGuardFootSegment {
            motion: C2FootSegment {
                timing: SegmentTickSpan::new(16).unwrap(),
                ..contact.motion
            },
            reach: GuardSegmentReachProof::Exact(proof),
            recovery_to_contact: false,
        });
        assert!(confirm_preemptive_guard_cadence_edge(
            &mut footwork,
            1,
            true,
            5,
        ));
        assert_eq!(footwork.step_sequence, 5);
        assert_eq!(footwork.pending_cadence_edge, None);
        let GuardSegmentReachProof::Exact(confirmed) = footwork.swing_replan_segment.unwrap().reach
        else {
            unreachable!()
        };
        assert_eq!(confirmed.sequence, 5);
        assert!(!confirmed.accepts_preemptive_cadence_confirmation);

        // The physical contact edge cannot consume an early or unrelated
        // authoritative identity transition.
        assert!(!begin_next_guard_swing_after_contact(
            &mut footwork,
            true,
            1,
            left,
            right,
            1,
            Vec3::ZERO,
            Quat::IDENTITY,
            left,
            right,
        ));
    }

    #[test]
    fn on_time_contact_is_not_cleared_by_a_coexisting_support_recovery() {
        assert!(!guard_recovery_may_rebase_on_semantic_edge(1, true, true));
        assert!(guard_recovery_may_rebase_on_semantic_edge(1, true, false));

        let old_contact = Vec3::new(0.2, 0.0, -0.3);
        let new_swing = Vec3::new(-0.2, 0.0, 0.1);
        let mut footwork = RaisedFootworkState {
            initialized: true,
            swing_left: false,
            step_sequence: 10,
            pending_cadence_edge: Some((true, 11)),
            left_support_release_owner: Some(SupportReleaseOwner::TerminalHold {
                endpoint: new_swing,
            }),
            right_plant: old_contact,
            ..default()
        };

        // The contact tick retains its terminal publication. The following
        // advancing semantic tick consumes the typed pending edge exactly
        // once and rebases the next swing from the visible pose.
        assert_eq!(footwork.pending_cadence_edge, Some((true, 11)));
        consume_pending_guard_cadence_edge(
            &mut footwork,
            true,
            11,
            new_swing,
            old_contact,
            1,
            Vec3::ZERO,
            Quat::IDENTITY,
            new_swing,
            old_contact,
        );
        assert_eq!(footwork.pending_cadence_edge, None);
        assert_eq!(footwork.step_sequence, 11);
        assert!(footwork.swing_left);
        assert_eq!(footwork.swing_start, new_swing);
        assert!(footwork.left_support_release_owner.is_none());
    }

    #[test]
    fn early_contact_edge_keeps_identity_atomic_until_recovery_settles() {
        let old_visible = Vec3::new(0.2, 0.3, -0.4);
        let new_visible = Vec3::new(-0.2, 0.3, 0.4);
        let mut footwork = RaisedFootworkState {
            initialized: true,
            swing_left: false,
            step_sequence: 10,
            swing_emergency_brake: Some(EmergencyFootBrake {
                stationary_ideal: old_visible,
                owner_local_ideal: Some(old_visible),
            }),
            swing_release_owner_active: true,
            ..default()
        };

        defer_guard_cadence_edge_for_contact_recovery(&mut footwork, true, 11);

        assert_eq!(footwork.step_sequence, 10);
        assert!(!footwork.swing_left);
        assert_eq!(footwork.pending_cadence_edge, Some((true, 11)));
        assert!(!guard_pending_cadence_edge_can_be_consumed(&footwork));

        footwork.swing_emergency_brake = None;
        footwork.swing_release_owner_active = false;
        assert!(guard_pending_cadence_edge_can_be_consumed(&footwork));
        consume_pending_guard_cadence_edge(
            &mut footwork,
            true,
            11,
            new_visible,
            old_visible,
            1,
            Vec3::ZERO,
            Quat::IDENTITY,
            new_visible,
            old_visible,
        );

        assert_eq!(footwork.step_sequence, 11);
        assert!(footwork.swing_left);
        assert_eq!(footwork.pending_cadence_edge, None);
        assert_eq!(footwork.swing_start, new_visible);
    }

    #[test]
    fn release_settlement_cannot_advance_sequence_without_swing_side() {
        let mut footwork = RaisedFootworkState {
            initialized: true,
            step_sequence: 10,
            swing_left: false,
            ..default()
        };

        retain_or_defer_guard_cadence_identity(&mut footwork, true, 11, false);

        assert_eq!(footwork.step_sequence, 10);
        assert!(!footwork.swing_left);
        assert!(footwork.awaiting_step_sequence);
        assert_eq!(footwork.pending_cadence_edge, Some((true, 11)));
    }

    #[test]
    fn ground_safety_slide_preserves_support_and_scales_with_leg_reach() {
        let owner_local = Vec3::new(-0.15, -0.8, 0.32);
        let owner = Some(SupportReleaseOwner::GroundSafetySlide { owner_local });
        let footwork = RaisedFootworkState {
            initialized: true,
            swing_left: false,
            left_support_weight: 1.0,
            left_support_release_owner: owner,
            ..default()
        };

        assert!(support_owner_preserves_contact(owner));
        assert!(!support_owner_blocks_cadence(owner));
        assert!(raised_leg_contributes_pelvis_reach(&footwork, true, true));

        let hip = Vec3::new(0.0, 0.9, 0.0);
        let presented = Vec3::new(-0.2, 0.1, 0.7);
        let warning_reach = 0.82;
        let endpoint = ground_safety_slide_endpoint(presented, hip, warning_reach, None);
        assert!(endpoint.distance(hip) <= warning_reach + 0.0001);

        let scaled =
            ground_safety_slide_endpoint(presented * 2.0, hip * 2.0, warning_reach * 2.0, None);
        assert!(scaled.distance(endpoint * 2.0) <= 0.0001);
    }

    #[test]
    fn infeasible_guard_contact_retains_one_plant_and_one_bounded_swing_owner() {
        let left = Vec3::new(-0.2, 0.1, 0.1);
        let right = Vec3::new(0.2, 0.1, -0.1);
        let mut footwork = RaisedFootworkState {
            initialized: true,
            swing_left: true,
            awaiting_step_sequence: true,
            swing_release_owner_active: true,
            swing_emergency_brake: Some(EmergencyFootBrake {
                stationary_ideal: right,
                owner_local_ideal: None,
            }),
            ..default()
        };

        install_guard_swing_fallback(
            &mut footwork,
            GuardFootReleasePlan::EmergencyBrake { presented: left },
            41,
            Vec3::ZERO,
            Quat::IDENTITY,
            left,
            right,
        );

        assert!(footwork.awaiting_step_sequence);
        assert!(footwork.swing_release_owner_active);
        assert!(footwork.swing_emergency_brake.is_some());
        assert!(footwork.swing_replan_segment.is_none());
        assert_eq!(footwork.left_support_weight, 0.0);
        assert_eq!(footwork.right_support_weight, 1.0);
        assert!(footwork.left_support_release_owner.is_none());
        assert!(footwork.right_support_release_owner.is_none());
        assert_eq!(footwork.left_motion_owner_epoch, 41);
        assert_eq!(footwork.right_motion_owner_epoch, 0);
        assert!(!clear_ownerless_guard_wait(&mut footwork));
    }

    #[test]
    fn predicted_hip_uncertainty_expands_with_the_owner_horizon() {
        let root = Vec3::ZERO;
        let reach = FootReachEnvelope::new(root, root, 0.95, 0.96).unwrap();
        let trajectory = PredictedHipTrajectory::from_retained_motion(
            reach,
            None,
            Vec3::ZERO,
            1.0 / CONTINUITY_SAMPLE_HZ,
            0.01,
            PELVIS_CORRECTION_SPEED,
        )
        .unwrap();
        let endpoint = Vec3::X * 0.8;
        assert!(trajectory.contains_warning_at(endpoint, 0.0));
        assert!(!trajectory.contains_warning_at(endpoint, 0.2));
    }

    #[test]
    fn guard_contact_span_uses_the_earliest_accelerating_cadence_edge() {
        let timing = guard_cadence_contact_tick_span(0.527, 2.0).unwrap();
        let expected = (((1.0 - (0.527_f32 * 2.0).fract()) * guard_step_length(2.0) / 2.0)
            * CONTINUITY_SAMPLE_HZ)
            .floor() as u32;
        assert_eq!(timing.total_ticks.get(), expected);
        assert_ne!(timing.total_ticks.get(), 21);
        assert_eq!(
            guard_cadence_contact_tick_span(0.527, 0.28)
                .unwrap()
                .total_ticks,
            timing.total_ticks,
            "acceleration onset must not promise a later contact than full guard speed",
        );
    }

    #[test]
    fn guard_contact_planner_chooses_farthest_dynamics_proven_progress() {
        let presented = Vec3::new(-0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0);
        let requested = presented + Vec3::NEG_Z * 0.55;
        let timing = SegmentTickSpan::new(18).unwrap();
        let hip = Vec3::new(0.0, 0.9, 0.0);
        let proof = stationary_exact_guard_proof(hip, 1.1, 1.2, 18);
        let GuardFootEndpointPlan::Segment(segment) = plan_guard_c2_contact_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            timing,
            Some(proof),
            |progress| Some(presented.lerp(requested, progress)),
        ) else {
            panic!("expected a shortened, fully proven semantic contact");
        };
        let travelled = segment.end.position().distance(presented);
        assert!(travelled > 0.01);
        assert!(
            (travelled - presented.distance(requested)).abs() <= 0.0001,
            "an ordinary 55 cm guard swing must now fit its real cadence edge",
        );
        assert!(c2_segment_dynamics_are_bounded(
            segment.motion,
            GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
            GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
        ));
        assert!(contact_error_envelope_is_proven(segment.motion));
        assert!(guard_exact_hip_path_contains_segment_with_limit(
            proof,
            segment.motion,
            false,
        ));
    }

    #[test]
    fn native_scale_guard_contact_fits_eighteen_tick_deadline() {
        let presented = Vec3::new(-0.10, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.27);
        let requested = Vec3::new(-0.25, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.57);
        let timing = SegmentTickSpan::new(18).unwrap();
        let proof = stationary_exact_guard_proof(Vec3::new(-0.10, 0.91, -0.20), 1.05, 1.10, 18);
        let GuardFootEndpointPlan::Segment(segment) = plan_guard_c2_contact_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            timing,
            Some(proof),
            |progress| Some(presented.lerp(requested, progress)),
        ) else {
            panic!("the production-scale alternate contact must fit its cadence edge");
        };
        assert!(segment.end.position().distance(presented) > 0.80);
        assert!(c2_segment_dynamics_are_bounded(
            segment.motion,
            GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
            GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
        ));
        assert_eq!(segment.motion.timing.total_ticks.get(), 18);
    }

    #[test]
    fn contact_driven_guard_landing_advances_local_owner_without_cadence_wait() {
        let presented = Vec3::new(-0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0);
        let endpoint = presented + Vec3::NEG_Z * 0.24;
        let proof = stationary_exact_guard_proof(Vec3::new(0.0, 0.88, -0.1), 1.1, 1.2, 18);
        let GuardFootEndpointPlan::Segment(mut segment) = plan_guard_c2_contact_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            SegmentTickSpan::new(18).unwrap(),
            Some(proof),
            |_| Some(endpoint),
        ) else {
            panic!("expected a contact owner");
        };
        segment.motion.timing.elapsed_ticks = segment.motion.timing.total_ticks.get();
        let mut footwork = RaisedFootworkState {
            initialized: true,
            swing_left: true,
            step_sequence: 9,
            left_support_weight: 0.0,
            right_support_weight: 1.0,
            awaiting_step_sequence: true,
            ..default()
        };

        complete_contact_driven_guard_segment(&mut footwork, segment);
        assert!(!footwork.awaiting_step_sequence);
        assert_eq!(footwork.left_plant, endpoint);
        assert_eq!(footwork.left_support_weight, 1.0);
        assert!(
            footwork
                .swing_replan_segment
                .is_some_and(|owner| owner.timing.is_complete())
        );

        assert!(begin_next_contact_driven_guard_swing(
            &mut footwork,
            true,
            endpoint,
            Vec3::new(0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0),
            1,
            Vec3::ZERO,
            Quat::IDENTITY,
            endpoint,
            Vec3::new(0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0),
        ));
        assert!(!footwork.swing_left);
        assert_eq!(footwork.step_sequence, 10);
        assert_eq!(footwork.left_support_weight, 1.0);
        assert_eq!(footwork.right_support_weight, 1.0);
        assert!(footwork.swing_replan_segment.is_none());
        assert!(footwork.pending_cadence_edge.is_none());
    }

    #[test]
    fn contact_driven_guard_planner_scales_with_speed_and_leg_length() {
        for scale in [0.65_f32, 1.0, 1.6] {
            for speed in [0.45_f32, 1.4, 2.8] {
                let presented = Vec3::new(
                    -0.12 * scale,
                    MEASURED_ANKLE_SOLE_OFFSET_METRES * scale,
                    0.12 * scale,
                );
                let requested = presented + Vec3::NEG_Z * ((0.16 + 0.10 * speed).min(0.44) * scale);
                let hip = Vec3::new(0.0, 0.78 * scale, -0.03 * scale);
                // Sparse presentation may observe the authored cadence only a
                // few ticks before its edge. Contact ownership must still be
                // able to choose the longer morphology/dynamics-safe landing.
                let proof = stationary_exact_guard_proof(hip, 0.95 * scale, 1.0 * scale, 4);
                let plan = plan_contact_driven_guard_segment(
                    presented,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    requested,
                    64,
                    Some(proof),
                    None,
                    |progress| Some(presented.lerp(requested, progress)),
                );
                let GuardFootEndpointPlan::Segment(segment) = plan else {
                    panic!("scale={scale} speed={speed} must retain a terrain contact");
                };
                assert!(segment.end.is_contact());
                assert!(segment.end.position().distance(presented) > 0.001);
                let GuardSegmentReachProof::Exact(contact_proof) = segment.reach else {
                    panic!("contact-driven guard requires an exact hip proof");
                };
                assert!(guard_contact_has_terminal_support_reserve(
                    contact_proof,
                    segment,
                ));
                assert!(c2_segment_dynamics_are_bounded(
                    segment.motion,
                    GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
                    GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
                ));
            }
        }
    }

    #[test]
    fn contact_driven_guard_duration_is_bounded_by_planted_reach() {
        let support = Vec3::new(0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0);
        let mut proof = stationary_exact_guard_proof(Vec3::new(0.18, 0.88, 0.0), 0.92, 0.95, 64);
        proof.trajectory_signature.world_velocity = Vec3::NEG_Z * 1.4;
        proof.trajectory_signature.command_velocity = Vec3::NEG_Z * 1.4;
        let budget = contact_driven_guard_support_tick_budget(proof, support);
        assert!(budget > 1 && budget < 64, "budget={budget}");

        let presented = Vec3::new(-0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0);
        let requested = presented + Vec3::NEG_Z * 0.40;
        let swing_proof = stationary_exact_guard_proof(Vec3::new(-0.18, 0.88, 0.0), 0.92, 0.95, 4);
        let GuardFootEndpointPlan::Segment(segment) = plan_contact_driven_guard_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            requested,
            budget,
            Some(swing_proof),
            None,
            |progress| Some(presented.lerp(requested, progress)),
        ) else {
            panic!("expected a shortened terrain contact");
        };
        assert!(segment.timing.total_ticks.get() <= budget);
        assert!(segment.end.position().distance(presented) > 0.001);
    }

    #[test]
    fn contact_driven_guard_retains_a_typed_owner_outside_warning_reach() {
        let hip = Vec3::Y * 0.88;
        let presented = hip + Vec3::X * 0.93;
        let requested = presented + Vec3::X * 0.25;
        let proof = stationary_exact_guard_proof(hip, 0.92, 0.95, 32);
        let plan = plan_contact_driven_guard_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            requested,
            32,
            Some(proof),
            None,
            |progress| Some(presented.lerp(requested, progress)),
        );
        let GuardFootEndpointPlan::Segment(segment) = plan else {
            panic!("warning-reach recovery must retain a spatial C2 owner: {plan:?}");
        };
        assert!(segment.end.is_contact());
        assert!(segment.recovery_to_contact);
        assert!(segment.end.position().distance(hip) < presented.distance(hip));
    }

    #[test]
    fn body_relative_guard_gait_is_continuous_across_cadence_edges() {
        let rig_origin = Vec3::new(3.0, 0.0, -2.0);
        let stance_local = Vec3::new(-0.22, 0.09, 0.0);
        let rotation = Quat::from_rotation_y(0.41);
        let direction = (rotation * Vec3::new(0.35, 0.0, -0.94)).normalize();
        let lateral = rotation * Vec3::X;
        let step_length = 0.46;
        let (left_before, right_before) = body_relative_guard_gait_targets(
            rig_origin,
            lateral,
            stance_local,
            direction,
            step_length,
            1.0,
            true,
            false,
            None,
        );
        let (left_after, right_after) = body_relative_guard_gait_targets(
            rig_origin,
            lateral,
            stance_local,
            direction,
            step_length,
            0.0,
            false,
            false,
            None,
        );
        assert!(left_before.distance(left_after) <= 1.0e-6);
        assert!(right_before.distance(right_after) <= 1.0e-6);
        assert!((left_before.y - (rig_origin.y + stance_local.y)).abs() <= 1.0e-6);
        assert!((right_before.y - (rig_origin.y + stance_local.y)).abs() <= 1.0e-6);

        let (left_mid, right_mid) = body_relative_guard_gait_targets(
            rig_origin,
            lateral,
            stance_local,
            direction,
            step_length,
            0.5,
            true,
            false,
            None,
        );
        assert!(left_mid.y > rig_origin.y + stance_local.y);
        assert!((right_mid.y - (rig_origin.y + stance_local.y)).abs() <= 1.0e-6);
        assert!(
            left_mid.xz().distance(rig_origin.xz()) <= stance_local.x.abs() + step_length * 0.51
        );
        assert!(
            right_mid.xz().distance(rig_origin.xz()) <= stance_local.x.abs() + step_length * 0.51
        );

        let relative_velocity = |progress: f32| {
            30.0 * progress.powi(2) - 60.0 * progress.powi(3) + 30.0 * progress.powi(4)
        };
        assert_eq!(relative_velocity(0.0), 0.0);
        assert_eq!(relative_velocity(1.0), 0.0);

        for scale in [0.5, 1.0, 2.0] {
            let hip = Vec3::new(0.0, 0.92, 0.0) * scale;
            let target = Vec3::new(0.7, 0.09, 0.4) * scale;
            let reach = 0.88 * scale;
            let constrained = constrain_guard_gait_target_to_reach(target, hip, reach);
            assert!(constrained.distance(hip) <= reach + 1.0e-5);
            assert!((constrained.y - target.y).abs() <= 1.0e-6);
        }
    }

    #[test]
    fn body_relative_guard_gait_keeps_the_support_contact_latched() {
        let origin = Vec3::ZERO;
        let lateral = Vec3::X;
        let stance = Vec3::new(-0.22, 0.085, 0.0);
        let direction = Vec3::Z;
        let plants = (Vec3::new(-0.22, 0.085, -0.11), Vec3::new(0.22, 0.085, 0.13));
        for tick in 0..=32 {
            let progress = tick as f32 / 32.0;
            let generated = body_relative_guard_gait_targets(
                origin, lateral, stance, direction, 0.46, progress, true, false, None,
            );
            let (left, right) =
                guard_gait_targets_with_latched_support(generated, plants, true, false, None);
            assert_eq!(right, plants.1);
            assert!(left.is_finite());
        }
    }

    #[test]
    fn overgrowth_guard_target_tracks_future_hips_and_morphology() {
        let current_hips = Vec3::new(4.0, 0.0, -3.0);
        let velocity = Vec3::new(0.0, 0.4, 1.4);
        let rate = overgrowth_guard_step_rate(1.4, 1.0);
        let generated = overgrowth_guard_foot_targets(
            current_hips,
            Vec3::X,
            Vec3::new(-0.22, 0.085, 0.0),
            velocity,
            rate,
            false,
            None,
        );
        assert!((generated.0.z - (current_hips.z + 1.4 / rate)).abs() <= 1.0e-6);
        assert!((generated.0.x - 3.78).abs() <= 1.0e-6);
        assert!((generated.1.x - 4.22).abs() <= 1.0e-6);
        assert!((overgrowth_guard_stance_error(2.0) - 0.2).abs() <= 1.0e-6);
    }

    #[test]
    fn overgrowth_guard_step_rate_increases_with_speed_and_normalizes_scale() {
        assert_eq!(overgrowth_guard_step_rate(0.0, 1.0), 2.0);
        assert!(overgrowth_guard_step_rate(4.0, 1.0) > overgrowth_guard_step_rate(1.0, 1.0));
        assert_eq!(
            overgrowth_guard_step_rate(2.0, 2.0),
            overgrowth_guard_step_rate(1.0, 1.0)
        );
    }

    #[test]
    fn stationary_guard_contact_candidates_bootstrap_pelvis_reach() {
        let mut footwork = RaisedFootworkState {
            initialized: true,
            left_support_weight: 0.0,
            right_support_weight: 0.0,
            ..default()
        };
        assert!(raised_leg_is_stationary_contact_candidate(
            &footwork, false, true
        ));
        assert!(raised_leg_is_stationary_contact_candidate(
            &footwork, false, false
        ));
        assert!(!raised_leg_is_stationary_contact_candidate(
            &footwork, true, true
        ));
        footwork.pivot_active = true;
        footwork.pivot_left = true;
        assert!(raised_leg_is_stationary_contact_candidate(
            &footwork, false, true
        ));
        assert!(!raised_leg_is_stationary_contact_candidate(
            &footwork, false, false
        ));
    }

    #[test]
    fn stopping_guard_extends_contact_instead_of_creating_airborne_release() {
        let presented = Vec3::new(0.10, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.20);
        let endpoint = Vec3::new(0.12, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.48);
        let proof = stationary_exact_guard_proof(Vec3::new(0.0, 0.90, -0.15), 1.05, 1.10, 64);
        let mut segment = plan_guard_contact_recovery_without_cadence_deadline(
            presented,
            Vec3::new(0.0, 0.0, -2.5),
            Vec3::new(0.0, 0.0, -12.0),
            endpoint,
            Some(proof),
            None,
        )
        .expect("a stopped guard has time to complete a bounded terrain contact");
        assert!(segment.end.is_contact());
        assert!(segment.timing.total_ticks.get() > 6);
        assert!(c2_segment_dynamics_are_bounded(
            segment.motion,
            GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
            GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
        ));
        segment.motion.timing.elapsed_ticks = segment.motion.timing.total_ticks.get();
        let mut footwork = RaisedFootworkState {
            swing_left: false,
            step_sequence: 2,
            swing_replan_segment: Some(segment),
            awaiting_step_sequence: true,
            ..default()
        };
        complete_guard_segment_semantics(&mut footwork, segment, false, 2, false);
        assert!(footwork.swing_replan_segment.is_none());
        assert!(!footwork.awaiting_step_sequence);
        assert_eq!(footwork.right_plant, segment.end.position());
        assert_eq!(footwork.right_support_weight, 1.0);
    }

    #[test]
    fn guard_contact_can_extend_once_and_retain_the_deferred_cadence_identity() {
        let presented = Vec3::new(-0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0);
        let requested = presented + Vec3::NEG_Z * 0.38;
        let hip = Vec3::new(0.0, 0.9, 0.0);
        let mut proof = stationary_exact_guard_proof(hip, 1.1, 1.2, 10);
        proof.sequence = 7;
        proof.swing_left = true;
        proof.trajectory_signature.sequence = 7;

        let nominal = plan_guard_c2_contact_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            SegmentTickSpan::new(1).unwrap(),
            Some(proof),
            |progress| Some(presented.lerp(requested, progress)),
        );
        assert!(matches!(
            nominal,
            GuardFootEndpointPlan::MustReleaseOrReplan(_)
        ));
        let extended = (2..=17)
            .filter_map(|ticks| {
                let candidate = plan_guard_c2_contact_segment(
                    presented,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    SegmentTickSpan::new(ticks).unwrap(),
                    Some(proof),
                    |progress| Some(presented.lerp(requested, progress)),
                );
                let GuardFootEndpointPlan::Segment(segment) = candidate else {
                    return None;
                };
                Some((ticks, segment))
            })
            .max_by(|(_, left), (_, right)| {
                left.end
                    .position()
                    .distance_squared(presented)
                    .total_cmp(&right.end.position().distance_squared(presented))
            })
            .map(|(ticks, segment)| (ticks, GuardFootEndpointPlan::Segment(segment)));
        let (ticks, GuardFootEndpointPlan::Segment(segment)) =
            extended.expect("one bounded extra half-step should admit a real contact")
        else {
            unreachable!()
        };
        assert!(ticks > 1);
        assert!(segment.end.position().distance(presented) > 0.01);

        let GuardSegmentReachProof::Exact(extended_proof) = segment.reach else {
            unreachable!()
        };
        let mut next_signature = proof.trajectory_signature;
        next_signature.sequence = 8;
        assert_eq!(
            normalize_deferred_guard_signature(extended_proof, Some((false, 8)), next_signature,)
                .sequence,
            7,
        );
        assert_eq!(
            normalize_deferred_guard_signature(proof, Some((true, 8)), next_signature).sequence,
            8,
            "a deferred edge for the wrong swing side must not mask identity drift",
        );
        let mut preemptive = proof;
        preemptive.accepts_preemptive_cadence_confirmation = true;
        assert_eq!(
            normalize_deferred_guard_signature(preemptive, Some((true, 8)), next_signature)
                .sequence,
            7,
            "a contact-driven swing accepts the later matching cadence identity",
        );
    }

    #[test]
    fn exact_guard_proof_persists_across_sparse_speed_refresh_and_rejects_semantic_change() {
        let mut proof = stationary_exact_guard_proof(Vec3::new(0.0, 0.9, 0.0), 1.1, 1.2, 18);
        proof.trajectory_signature.command_velocity = Vec3::new(0.0, 0.0, -1.586);
        let expected = proof.sample(7).unwrap();
        let mut same_content_refresh = proof.trajectory_signature;
        same_content_refresh.source_tick = 99;
        same_content_refresh.presentation_tick = 7;
        same_content_refresh.world_velocity = Vec3::new(0.0, 0.0, -0.6);
        same_content_refresh.world_acceleration = Vec3::new(0.0, 0.0, -28.0);
        same_content_refresh.command_velocity = Vec3::new(0.0, 0.0, -0.9177);
        assert!(guard_exact_proof_matches_live(
            proof,
            Some(same_content_refresh),
            1,
            true,
            7,
            Some(expected),
            true,
        ));

        let mut changed_command = same_content_refresh;
        changed_command.command_velocity = Vec3::X;
        assert!(!guard_exact_proof_matches_live(
            proof,
            Some(changed_command),
            1,
            true,
            7,
            Some(expected),
            true,
        ));
        // Prediction error alone does not reset the analytic owner. The
        // separately tested live-reach gate rejects an unsafe sample before
        // publication, while harmless sparse convergence keeps elapsed time.
        assert!(guard_exact_proof_matches_live(
            proof,
            Some(same_content_refresh),
            1,
            true,
            7,
            Some(expected + Vec3::Y * 0.02),
            true,
        ));
        assert!(!guard_exact_proof_matches_live(
            proof,
            Some(same_content_refresh),
            2,
            false,
            7,
            Some(expected),
            true,
        ));
        assert!(!guard_exact_proof_matches_live(
            proof,
            Some(same_content_refresh),
            1,
            true,
            7,
            Some(expected),
            false,
        ));

        let mut on_time_edge = same_content_refresh;
        on_time_edge.sequence = 2;
        on_time_edge.presentation_tick = 18;
        let terminal_hip = proof.sample(18).unwrap();
        assert!(guard_exact_proof_matches_live(
            proof,
            Some(on_time_edge),
            1,
            true,
            18,
            Some(terminal_hip),
            true,
        ));
        // A conservative contact may finish before a slower authoritative
        // cadence. Its terminal sample remains a valid grounded owner until
        // that later N -> N+1 edge arrives.
        on_time_edge.presentation_tick = 20;
        assert!(guard_exact_proof_matches_live(
            proof,
            Some(on_time_edge),
            1,
            true,
            20,
            Some(terminal_hip),
            true,
        ));
        assert!(exact_guard_sample_is_live_reachable(
            proof,
            Vec3::new(0.0, 0.0, 0.0),
            20,
            Some(terminal_hip),
            false,
        ));
        on_time_edge.presentation_tick = 17;
        // Replicated cadence may lead a locally contact-driven landing. It is
        // synchronization evidence, not a reason to discard a still-safe
        // foot trajectory before physical contact.
        assert!(guard_exact_proof_matches_live(
            proof,
            Some(on_time_edge),
            1,
            true,
            17,
            proof.sample(17),
            true,
        ));
    }

    #[test]
    fn exact_guard_sample_rejects_live_reach_excess_inside_signature_tolerance() {
        let proof = stationary_exact_guard_proof(Vec3::ZERO, 0.92, 0.95, 18);
        let sample = Vec3::new(0.92, 0.0, 0.0);
        assert!(proof.permits(sample, 7, false));
        assert!(!exact_guard_sample_is_live_reachable(
            proof,
            sample,
            7,
            Some(Vec3::new(-0.01, 0.0, 0.0)),
            false,
        ));
    }

    #[test]
    fn contact_driven_guard_retains_safe_owner_across_prediction_refresh() {
        let proof = stationary_exact_guard_proof(Vec3::ZERO, 0.92, 0.95, 18);
        let safe_sample = Vec3::new(0.70, 0.0, 0.0);
        assert!(contact_driven_guard_owner_is_live(
            proof,
            proof.sequence,
            proof.swing_left,
            Some(Vec3::ZERO),
            true,
        ));
        assert!(contact_driven_guard_sample_is_live_reachable(
            proof,
            safe_sample,
            Some(Vec3::ZERO),
            false,
        ));
        assert!(!contact_driven_guard_sample_is_live_reachable(
            proof,
            Vec3::new(0.93, 0.0, 0.0),
            Some(Vec3::ZERO),
            false,
        ));
        assert!(contact_driven_guard_sample_is_live_reachable(
            proof,
            Vec3::new(0.93, 0.0, 0.0),
            Some(Vec3::ZERO),
            true,
        ));
    }

    #[test]
    fn exact_guard_release_uses_the_same_hard_reach_proof() {
        let presented = Vec3::new(-0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.35);
        let proof = stationary_exact_guard_proof(Vec3::new(0.0, 0.9, 0.0), 0.92, 0.95, 24);
        let timing = SegmentTickSpan::new(24).unwrap();
        let GuardFootEndpointPlan::MustReleaseOrReplan(GuardFootReleasePlan::Segment(segment)) =
            plan_exact_guard_release(presented, Vec3::ZERO, Vec3::ZERO, timing, proof)
        else {
            panic!("expected an exact hard-reach release owner");
        };
        assert!(!segment.end.is_contact());
        assert!(guard_exact_hip_path_contains_segment_with_limit(
            proof,
            segment.motion,
            true,
        ));
        assert!(matches!(segment.reach, GuardSegmentReachProof::Exact(_)));
    }

    #[test]
    fn exact_guard_contact_may_recover_from_warning_band_but_lands_inside_warning() {
        let hip = Vec3::new(0.0, 0.9, 0.0);
        let presented = Vec3::new(0.0, 0.0, 0.23);
        let endpoint = Vec3::new(0.0, 0.0, 0.12);
        let proof = stationary_exact_guard_proof(hip, 0.921, 0.939, 18);
        assert!(presented.distance(hip) > proof.warning_reach);
        assert!(presented.distance(hip) < proof.hard_reach);

        let GuardFootEndpointPlan::Segment(segment) = plan_guard_c2_contact_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            SegmentTickSpan::new(18).unwrap(),
            Some(proof),
            |_| Some(endpoint),
        ) else {
            panic!("a reachable swing must recover from warning to a planted contact");
        };
        assert!(guard_exact_hip_path_contains_segment(
            &(proof.start_tick..=proof.contact_tick)
                .map(|tick| {
                    (
                        proof.sample(tick).unwrap(),
                        proof.warning_reach,
                        proof.hard_reach,
                    )
                })
                .collect::<Vec<_>>(),
            segment.motion,
        ));
        assert!(segment.end.position().distance(hip) <= proof.warning_reach);
    }

    #[test]
    fn active_pelvis_owner_defers_cadence_identity_without_releasing_new_support() {
        let mut footwork = RaisedFootworkState {
            initialized: true,
            swing_left: false,
            step_sequence: 7,
            pelvis_acquisition: Some(GuardPelvisAcquisition {
                start: Transform::IDENTITY,
                target: Some(Transform::from_translation(Vec3::Y * 0.09)),
                start_sequence: 7,
                progress: 0.4,
                duration_seconds: 0.32,
                advance_authorized: true,
                start_velocity: Vec3::ZERO,
                start_acceleration: Vec3::ZERO,
                trajectory_signature: GuardPelvisTrajectorySignature {
                    sequence: 7,
                    ..default()
                },
            }),
            left_support_weight: 1.0,
            right_support_weight: 0.0,
            ..default()
        };
        assert!(defer_guard_cadence_edge_for_active_pelvis(
            &mut footwork,
            true,
            8,
        ));
        assert_eq!(footwork.pending_cadence_edge, Some((true, 8)));
        assert_eq!(footwork.step_sequence, 7);
        assert!(!footwork.swing_left);
        assert_eq!(footwork.left_support_weight, 1.0);
        assert!(footwork.left_support_release_owner.is_none());
        assert_eq!(footwork.pelvis_acquisition.unwrap().start_sequence, 8);
    }

    #[test]
    fn deferred_pelvis_edge_publishes_old_contact_before_adopting_new_cadence() {
        let endpoint = Vec3::new(0.2, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.25);
        let mut proof = stationary_exact_guard_proof(Vec3::new(0.0, 0.9, 0.0), 1.1, 1.2, 1);
        proof.start_tick = 17;
        proof.contact_tick = 18;
        proof.sequence = 7;
        proof.swing_left = false;
        proof.trajectory_signature.sequence = 7;
        proof.trajectory_signature.presentation_tick = 17;
        let segment = PlannedGuardFootSegment {
            motion: C2FootSegment {
                start: endpoint,
                start_velocity: Vec3::ZERO,
                start_acceleration: Vec3::ZERO,
                end: FootSegmentEndpoint::Contact(FeasibleFootEndpoint::from_proven_guard_contact(
                    endpoint,
                )),
                timing: SegmentTickSpan::new(1).unwrap(),
                owner_epoch: 17,
            },
            reach: GuardSegmentReachProof::Exact(proof),
            recovery_to_contact: false,
        };
        let mut footwork = RaisedFootworkState {
            initialized: true,
            swing_left: false,
            step_sequence: 7,
            swing_replan_segment: Some(segment),
            left_plant: Vec3::new(-0.2, MEASURED_ANKLE_SOLE_OFFSET_METRES, 0.0),
            right_plant: endpoint,
            left_support_weight: 1.0,
            pelvis_acquisition: Some(GuardPelvisAcquisition {
                start: Transform::IDENTITY,
                target: Some(Transform::from_translation(Vec3::Y * 0.09)),
                start_sequence: 7,
                progress: 0.4,
                duration_seconds: 0.32,
                advance_authorized: true,
                start_velocity: Vec3::ZERO,
                start_acceleration: Vec3::ZERO,
                trajectory_signature: GuardPelvisTrajectorySignature {
                    sequence: 7,
                    ..default()
                },
            }),
            ..default()
        };
        assert!(defer_guard_cadence_edge_for_active_pelvis(
            &mut footwork,
            true,
            8,
        ));
        assert_eq!(footwork.step_sequence, 7);
        let mut current_signature = proof.trajectory_signature;
        current_signature.sequence = 8;
        current_signature.presentation_tick = 18;
        assert!(guard_exact_proof_matches_live(
            proof,
            Some(current_signature),
            footwork.step_sequence,
            footwork.swing_left,
            18,
            proof.sample(18),
            true,
        ));
        let mut terminal = footwork.swing_replan_segment.unwrap();
        advance_c2_segment_tick(&mut terminal.motion, true, 18);
        assert!(terminal.timing.is_complete());
        complete_guard_segment_semantics(&mut footwork, terminal, true, 8, true);
        assert!(footwork.swing_emergency_brake.is_none());
        assert!(
            footwork
                .swing_replan_segment
                .is_some_and(|owner| owner.end.is_contact() && owner.timing.is_complete())
        );
        let left_plant = footwork.left_plant;
        let right_plant = footwork.right_plant;
        consume_pending_guard_cadence_edge(
            &mut footwork,
            true,
            8,
            left_plant,
            right_plant,
            0,
            Vec3::ZERO,
            Quat::IDENTITY,
            left_plant,
            right_plant,
        );
        assert_eq!(footwork.step_sequence, 8);
        assert!(footwork.swing_left);
        assert!(footwork.swing_replan_segment.is_none());
    }

    #[test]
    fn emergency_release_brakes_retained_motion_instead_of_zeroing_derivatives() {
        let presented = Vec3::new(0.1, 0.2, 0.3);
        let FootEndpointPlan::MustReleaseOrReplan(FootReleasePlan::EmergencyBrake {
            presented: held,
        }) = release_plan_or_hold(None, presented)
        else {
            panic!("expected typed emergency brake");
        };
        let velocity = Vec3::X * 0.5;
        let acceleration = Vec3::X * 0.25;
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let tracked = advance_guard_foot_target_sample_with_reach(
            Some(presented),
            velocity,
            acceleration,
            Some(held),
            Vec3::ZERO,
            Vec3::ZERO,
            true,
            held,
            Some((Vec3::ZERO, Vec3::ZERO)),
            dt,
            true,
            None,
        );
        assert!(tracked.velocity.length() > 0.0);
        assert!(
            (tracked.acceleration - acceleration).length()
                <= FOOT_FOLLOWER_MAXIMUM_JERK * dt + 0.001
        );
        assert!(!emergency_brake_is_settled(
            tracked.velocity,
            tracked.acceleration
        ));
        assert!(emergency_brake_is_settled(
            Vec3::X * 0.0005,
            Vec3::X * 0.005
        ));
    }

    #[test]
    fn exact_guard_departure_replans_from_presented_motion_to_original_contact_tick() {
        let presented = Vec3::new(-0.18, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.28);
        let endpoint = presented + Vec3::new(0.0, 0.0, -0.06);
        let proof = stationary_exact_guard_proof(Vec3::new(0.0, 0.9, 0.0), 1.1, 1.2, 18);
        let GuardFootEndpointPlan::Segment(mut original) = plan_guard_c2_contact_segment(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            SegmentTickSpan::new(18).unwrap(),
            Some(proof),
            |_| Some(endpoint),
        ) else {
            panic!("expected the original exact contact plan");
        };
        original.motion.timing.elapsed_ticks = 5;
        let current = guard_swing_replan_sample(original.motion);
        let mut refreshed = proof;
        refreshed.start_tick = 5;
        refreshed.trajectory_signature.source_tick = 99;
        let GuardFootEndpointPlan::Segment(replanned) = replan_invalid_exact_guard_segment(
            original,
            current.position,
            current.velocity,
            current.acceleration,
            5,
            Some(refreshed),
        ) else {
            panic!("the remaining exact polynomial should be replannable");
        };
        let GuardSegmentReachProof::Exact(replanned_proof) = replanned.reach else {
            panic!("the replacement must retain an exact guard proof");
        };
        assert_eq!(replanned.motion.timing.total_ticks.get(), 13);
        assert_eq!(replanned_proof.start_tick, 5);
        assert_eq!(replanned_proof.contact_tick, 18);
        assert!(replanned.start.distance(current.position) <= 1e-6);
        assert!(replanned.start_velocity.distance(current.velocity) <= 1e-5);
        assert!(replanned.start_acceleration.distance(current.acceleration) <= 1e-4);
        assert_eq!(replanned.end.position(), endpoint);
    }

    #[test]
    fn guard_replan_analytic_motion_does_not_repeatedly_invalidate_history() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let start = Vec3::new(-0.2, 1.9, -0.4);
        let root = Vec3::new(0.0, 1.2, -0.5);
        let reach = FootReachEnvelope::new(root, root, 0.95, 0.96).unwrap();
        let FootEndpointPlan::Segment(mut segment) = guard_swing_replan_segment(
            start,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::new(-0.26, 1.94, -0.58),
            0.0,
            Some(stationary_hip_trajectory(reach, dt)),
        ) else {
            panic!("expected feasible fixed endpoint");
        };
        let mut position = start;
        let mut velocity = Vec3::ZERO;
        let mut acceleration = Vec3::ZERO;
        let mut previous_ideal = start;
        let mut previous_ideal_velocity = Vec3::ZERO;
        let mut previous_ideal_acceleration = Vec3::ZERO;
        for _ in 0..8 {
            segment.timing.advance();
            let sample = guard_swing_replan_sample(segment);
            let tracked = advance_guard_foot_target_sample_with_reach(
                Some(position),
                velocity,
                acceleration,
                Some(previous_ideal),
                previous_ideal_velocity,
                previous_ideal_acceleration,
                true,
                sample.position,
                Some((sample.velocity, sample.acceleration)),
                dt,
                true,
                None,
            );
            assert!(!tracked.replan.is_some_and(|(reason, _)| matches!(
                reason,
                FootFollowReason::DiscontinuousTarget | FootFollowReason::InvalidInput
            )));
            assert!(tracked.ideal_history_valid);
            position = tracked.position;
            velocity = tracked.velocity;
            acceleration = tracked.acceleration;
            previous_ideal = sample.position;
            previous_ideal_velocity = sample.velocity;
            previous_ideal_acceleration = sample.acceleration;
        }
    }

    #[test]
    fn c2_foot_segment_preserves_presented_derivatives_and_stops_at_contact() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let start = Vec3::ZERO;
        let velocity = Vec3::X * 0.1;
        let acceleration = Vec3::X * 0.2;
        let endpoint = Vec3::X * 0.1;
        let root = Vec3::Y * 0.5;
        let reach = FootReachEnvelope::new(root, root, 0.8, 0.9).unwrap();
        let FootEndpointPlan::Segment(segment) = guard_swing_replan_segment(
            start,
            velocity,
            acceleration,
            endpoint,
            0.0,
            Some(stationary_hip_trajectory(reach, dt)),
        ) else {
            panic!("expected feasible fixed endpoint");
        };

        let onset = guard_swing_replan_sample(segment);
        let contact = guard_swing_replan_sample_at_progress(segment, 1.0);
        assert!(onset.position.distance(start) <= 0.000001);
        assert!(onset.velocity.distance(velocity) <= 0.000001);
        assert!(onset.acceleration.distance(acceleration) <= 0.00001);
        assert!(contact.position.distance(endpoint) <= 0.00001);
        assert!(contact.velocity.length() <= 0.00001);
        assert!(contact.acceleration.length() <= 0.0001);
        assert_eq!(onset, guard_swing_replan_sample(segment));
        assert!(contact_error_envelope_is_proven(segment));
        assert_eq!(
            segment.timing.total_ticks.get(),
            (0.32 * CONTINUITY_SAMPLE_HZ).ceil() as u32
        );
    }

    #[test]
    fn contact_error_envelope_reaches_exact_zero_at_64_and_128_hz() {
        let start = Vec3::ZERO;
        let endpoint = Vec3::new(0.18, 0.0, 0.0);
        let hip = Vec3::Y * 0.55;
        for sample_hz in [64.0_f32, 128.0] {
            let dt = sample_hz.recip();
            let reach = FootReachEnvelope::new(hip, hip, 0.90, 0.94).unwrap();
            let FootEndpointPlan::Segment(segment) = guard_swing_replan_segment(
                start,
                Vec3::X * 0.08,
                Vec3::X * 0.1,
                endpoint,
                0.0,
                Some(stationary_hip_trajectory(reach, dt)),
            ) else {
                panic!("expected an admitted contact segment at {sample_hz} Hz");
            };
            let envelope = contact_error_envelope(segment).unwrap();
            let sample_count = (segment.timing.duration_seconds() * sample_hz).round() as usize;
            let mut previous_permitted = envelope.initial_lag;
            for tick in 0..=sample_count {
                let progress = tick as f32 / sample_count as f32;
                let sample = guard_swing_replan_sample_at_progress(segment, progress);
                let permitted = envelope.permitted_lag(progress);
                assert!(permitted <= previous_permitted + 0.000001);
                assert!(sample.position.distance(endpoint) <= permitted + 0.000001);
                previous_permitted = permitted;
            }
            let contact = guard_swing_replan_sample_at_progress(segment, 1.0);
            let numeric_tolerance = f32::EPSILON * 256.0;
            assert!(contact.position.distance(endpoint) <= numeric_tolerance);
            assert!(contact.velocity.length() <= numeric_tolerance);
            assert!(contact.acceleration.length() <= numeric_tolerance);
        }
    }

    #[test]
    fn outward_contact_motion_that_breaks_the_lag_envelope_replans_before_deadline() {
        let hip = Vec3::Y * 0.55;
        let reach = FootReachEnvelope::new(hip, hip, 0.90, 0.94).unwrap();
        assert!(matches!(
            plan_c2_foot_segment(
                Vec3::ZERO,
                Vec3::NEG_X * 2.0,
                Vec3::NEG_X * 4.0,
                Vec3::X * 0.18,
                0.25,
                Some(stationary_hip_trajectory(reach, 1.0 / CONTINUITY_SAMPLE_HZ,)),
            ),
            FootEndpointPlan::MustReleaseOrReplan(_)
        ));
    }

    fn sample_guard_segment_chain(sample_hz: f32) -> (f32, f32, f32, f32, GuardSwingSample) {
        let dt = sample_hz.recip();
        let hip = Vec3::Y * 0.55;
        let start = Vec3::ZERO;
        let endpoint = Vec3::new(0.18, 0.0, 0.0);
        let start_velocity = Vec3::X * 0.08;
        let start_acceleration = Vec3::X * 0.1;
        let reach = FootReachEnvelope::new(hip, hip, 0.90, 0.94).unwrap();
        let FootEndpointPlan::Segment(segment) = guard_swing_replan_segment(
            start,
            start_velocity,
            start_acceleration,
            endpoint,
            0.0,
            Some(stationary_hip_trajectory(reach, dt)),
        ) else {
            panic!("expected the lawful guard contact to have a fixed C2 segment");
        };
        assert!(c2_segment_dynamics_are_bounded(
            segment,
            GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION,
            GUARD_ACTION_SEGMENT_MAXIMUM_JERK,
        ));

        let upper_length = 0.523;
        let lower_length = 0.430;
        let maximum_target_reach = maximum_reach(upper_length, lower_length);
        let initial_solution = solve_two_bone_with_reach(
            hip,
            Vec3::new(0.0, 0.27, 0.25),
            start,
            start,
            upper_length,
            lower_length,
            Vec3::Z,
            maximum_target_reach,
        )
        .expect("the presented onset must be solvable");
        let mut previous_knee = initial_solution.knee;
        let mut previous_end = initial_solution.end;
        let mut maximum_knee_step: f32 = 0.0;
        let mut maximum_end_step: f32 = 0.0;
        let mut maximum_published_acceleration: f32 = 0.0;
        let mut maximum_published_jerk: f32 = 0.0;
        let mut published_positions = vec![start];
        let mut tracked_velocity = start_velocity;
        let mut tracked_acceleration = start_acceleration;
        let sample_count = (segment.timing.duration_seconds() * sample_hz).ceil() as usize;
        let mut final_sample = guard_swing_replan_sample(segment);
        for index in 1..=sample_count {
            let progress = ((index as f32 * dt) / segment.timing.duration_seconds()).min(1.0);
            let sample = guard_swing_replan_sample_at_progress(segment, progress);
            let tracked = direct_c2_guard_target_sample(
                sample.position,
                sample.velocity,
                sample.acceleration,
            );
            assert!(tracked.replan.is_none());
            assert!(tracked.ideal_history_valid);
            let solution = solve_two_bone_with_reach(
                hip,
                previous_knee,
                previous_end,
                tracked.position,
                upper_length,
                lower_length,
                Vec3::Z,
                maximum_target_reach,
            )
            .expect("the admitted ankle path must remain solvable");
            maximum_knee_step = maximum_knee_step.max(previous_knee.distance(solution.knee));
            maximum_end_step = maximum_end_step.max(previous_end.distance(solution.end));
            published_positions.push(tracked.position);
            if published_positions.len() >= 3 {
                let length = published_positions.len();
                let acceleration = (published_positions[length - 1]
                    - published_positions[length - 2] * 2.0
                    + published_positions[length - 3])
                    / dt.powi(2);
                maximum_published_acceleration =
                    maximum_published_acceleration.max(acceleration.length());
                if published_positions.len() >= 4 {
                    let previous_acceleration = (published_positions[length - 2]
                        - published_positions[length - 3] * 2.0
                        + published_positions[length - 4])
                        / dt.powi(2);
                    maximum_published_jerk = maximum_published_jerk
                        .max(((acceleration - previous_acceleration) / dt).length());
                }
            }
            let along = (tracked.position - start).dot((endpoint - start).normalize());
            assert!(along >= -0.00001 && along <= start.distance(endpoint) + 0.00001);
            previous_knee = solution.knee;
            previous_end = solution.end;
            tracked_velocity = tracked.velocity;
            tracked_acceleration = tracked.acceleration;
            final_sample = sample;
        }

        assert!(final_sample.position.distance(endpoint) <= 0.00001);
        assert!(
            previous_end.distance(endpoint) <= 0.001,
            "published={previous_end:?} endpoint={endpoint:?} distance={}",
            previous_end.distance(endpoint)
        );
        assert!(emergency_brake_is_settled(
            tracked_velocity,
            tracked_acceleration
        ));
        (
            maximum_knee_step,
            maximum_end_step,
            maximum_published_acceleration,
            maximum_published_jerk,
            GuardSwingSample {
                position: previous_end,
                velocity: tracked_velocity,
                acceleration: tracked_acceleration,
            },
        )
    }

    #[test]
    fn guard_action_segment_chain_arrives_without_sample_rate_dependent_steps() {
        let (knee_step_64, end_step_64, acceleration_64, jerk_64, final_64) =
            sample_guard_segment_chain(64.0);
        let (knee_step_128, end_step_128, acceleration_128, jerk_128, final_128) =
            sample_guard_segment_chain(128.0);

        assert!(knee_step_64 <= MAX_KNEE_STEP_METRES);
        assert!(knee_step_128 <= knee_step_64 + 0.00001);
        assert!(end_step_128 <= end_step_64 + 0.00001);
        assert!(acceleration_64 <= GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION + 0.1);
        assert!(acceleration_128 <= GUARD_ACTION_SEGMENT_MAXIMUM_ACCELERATION + 0.1);
        assert!(jerk_64 <= GUARD_ACTION_SEGMENT_MAXIMUM_JERK + 1.0);
        assert!(jerk_128 <= GUARD_ACTION_SEGMENT_MAXIMUM_JERK + 1.0);
        assert!(final_64.position.distance(final_128.position) <= 0.001);
    }

    #[test]
    fn infeasible_contact_deadline_returns_typed_release_outcome() {
        let root = Vec3::Y * 0.5;
        let reach = FootReachEnvelope::new(root, root, 0.8, 0.9).unwrap();
        assert!(matches!(
            guard_swing_replan_segment(
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::X,
                0.99,
                Some(stationary_hip_trajectory(reach, 1.0 / CONTINUITY_SAMPLE_HZ,)),
            ),
            FootEndpointPlan::MustReleaseOrReplan(_)
        ));
    }

    #[test]
    fn release_to_planned_contact_starts_at_the_visible_solve_target() {
        let visible_release = Vec3::new(0.1, 0.25, -6.094);
        let restored_authored = Vec3::new(0.1, 0.5, -6.9);
        let start = planned_contact_start(None, Some(visible_release), restored_authored);
        assert_eq!(start, visible_release);
        assert_eq!(
            start.lerp(Vec3::new(0.1, 0.085, -8.0), 0.0),
            visible_release
        );

        let retained = Vec3::new(0.1, 0.3, -6.2);
        assert_eq!(
            planned_contact_start(Some(retained), Some(visible_release), restored_authored),
            retained
        );
    }

    #[test]
    fn new_run_plan_transports_in_progress_release_start_with_owner() {
        // Captured right release-to-plan seam f71->72. Holding the f71 ankle
        // in world space moved it 8.6 cm relative to the advancing hip while
        // Hermite progress was still zero, amplifying into a 13.7 cm knee
        // step. The seed must retain the same owner-local point instead.
        let previous_root = Vec3::new(0.0, 2.8301053, -6.1015625);
        let current_root = Vec3::new(0.0, 2.8237216, -6.1875);
        let previous_ankle = Vec3::new(0.12985985, 2.1838071, -5.3937254);
        let previous_owner = previous_ankle - previous_root;
        let stale_analytic_owner = previous_owner + Vec3::new(0.0, -0.06, -0.11);
        assert_eq!(
            run_previous_owner_target(
                LocomotionGait::Run,
                Some(previous_owner),
                Some(stale_analytic_owner),
            ),
            Some(previous_owner)
        );
        assert_eq!(
            run_previous_owner_target(
                LocomotionGait::Walk,
                Some(previous_owner),
                Some(stale_analytic_owner),
            ),
            Some(stale_analytic_owner)
        );
        let transported = run_plan_visible_start(
            LocomotionGait::Run,
            true,
            true,
            Some(previous_owner),
            current_root,
            Quat::IDENTITY,
            Some(previous_ankle),
        )
        .unwrap();
        assert!((transported - current_root - previous_owner).length() < 0.0001);
        assert!((transported - previous_ankle - (current_root - previous_root)).length() < 0.0001);
        assert!((transported - current_root).distance(previous_ankle - previous_root) < 0.0001);

        // Retained plans keep their original frozen start, and walk/stop keep
        // world-hold semantics rather than inheriting Run's owner transport.
        assert_eq!(
            run_plan_visible_start(
                LocomotionGait::Run,
                false,
                true,
                Some(previous_owner),
                current_root,
                Quat::IDENTITY,
                Some(previous_ankle),
            ),
            Some(previous_ankle)
        );
        assert_eq!(
            run_plan_visible_start(
                LocomotionGait::Walk,
                true,
                true,
                Some(previous_owner),
                current_root,
                Quat::IDENTITY,
                Some(previous_ankle),
            ),
            Some(previous_ankle)
        );
    }

    #[test]
    fn new_run_plan_prefers_last_propagated_ankle_over_stale_solve() {
        let stale_solve = Vec3::new(0.1, 2.1, -0.767);
        let rendered_ankle = Vec3::new(0.1, 2.1, -1.749);
        let visible = Some(rendered_ankle).or(Some(stale_solve));
        assert_eq!(
            planned_contact_start(None, visible, Vec3::ZERO),
            rendered_ankle
        );
    }

    #[test]
    fn cold_start_run_plan_is_bounded_over_the_remaining_approach() {
        // Captured hard-start geometry: the right plan first became airborne
        // late in the approach and previously tried to cover 1.525 m in four
        // presentation samples.
        let start = Vec3::new(0.1, 2.1, -0.304);
        let desired = Vec3::new(0.1, 2.0, -1.829);
        let phase_to_contact = 0.418;
        assert!(late_run_plan_requires_bound(None, phase_to_contact));
        assert!(!late_run_plan_requires_bound(None, 0.75));
        assert!(!late_run_plan_requires_bound(
            Some(desired),
            phase_to_contact
        ));
        let ready = RUN_LOCOMOTION_PROFILE.support_phase_radius + RUN_CONTACT_CHAIN_SETTLE_PHASE;
        let bounded = bound_late_run_contact(start, desired, 5.5, phase_to_contact, ready);
        assert!(bounded.xz().distance(desired.xz()) > 0.5);

        let phase_step =
            gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, 5.5, 1.0 / CONTINUITY_SAMPLE_HZ);
        let first_progress =
            run_contact_approach_progress(phase_to_contact, phase_to_contact, ready);
        let second_progress =
            run_contact_approach_progress(phase_to_contact - phase_step, phase_to_contact, ready);
        assert_eq!(start.lerp(bounded, first_progress).xz(), start.xz());
        let first_step = start
            .lerp(bounded, second_progress)
            .xz()
            .distance(start.xz());
        let root_step = 5.5 / CONTINUITY_SAMPLE_HZ;
        assert!(first_step - root_step <= MAX_RUN_SWING_ROOT_RELATIVE_STEP_METRES + 0.0001);
    }

    #[test]
    fn reach_released_support_lobe_cannot_reenter_before_true_flight() {
        let (still_exhausted, effective_support) = support_after_exhausted_lobe(true, 0.4);
        assert!(still_exhausted);
        assert_eq!(effective_support, 0.0);
        assert!(!run_planned_contact_allowed(still_exhausted, 0.2, 0.75));

        let visible_release = Vec3::new(0.1, 0.2, -8.757);
        let stale_same_lobe_plan = Vec3::new(0.1, 0.08, -10.203);
        let followed = advance_foot_target_at_speed(
            Some(visible_release),
            stale_same_lobe_plan,
            1.0 / CONTINUITY_SAMPLE_HZ,
            AIRBORNE_RELEASE_TARGET_SPEED,
        );
        assert!(
            followed.distance(visible_release)
                <= AIRBORNE_RELEASE_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );

        let (cleared, flight_support) = support_after_exhausted_lobe(true, 0.0);
        assert!(!cleared);
        assert_eq!(flight_support, 0.0);
        assert!(run_planned_contact_allowed(cleared, 0.75, 0.75));
    }

    #[test]
    fn unplanned_run_support_lobe_waits_for_true_flight() {
        assert!(unplanned_run_support_requires_flight(
            LocomotionGait::Run,
            5.5,
            0.8,
            false,
            None,
        ));
        assert!(!unplanned_run_support_requires_flight(
            LocomotionGait::Run,
            5.5,
            0.8,
            true,
            None,
        ));
        assert!(!unplanned_run_support_requires_flight(
            LocomotionGait::Run,
            5.5,
            0.8,
            false,
            Some(Vec3::NEG_Z),
        ));
        assert!(!unplanned_run_support_requires_flight(
            LocomotionGait::Run,
            0.0,
            0.8,
            false,
            None,
        ));
    }

    #[test]
    fn newly_acquired_contact_keeps_orientation_blending_until_converged() {
        assert!(update_contact_orientation_blend(false, Some(0.0), 1.0));
        assert!(update_contact_orientation_blend(true, Some(1.0), 1.0));
        assert!(!update_contact_orientation_blend(false, Some(1.0), 1.0));
        assert!(!update_contact_orientation_blend(true, Some(1.0), 0.0));

        let airborne = Quat::IDENTITY;
        let contact = Quat::from_rotation_x(63.54_f32.to_radians());
        let first_contact = advance_airborne_foot_rotation(
            Some(airborne),
            contact,
            1.0 / CONTINUITY_SAMPLE_HZ,
            AIRBORNE_FOOT_ROTATION_SPEED_DEGREES,
        );
        assert!(
            airborne.angle_between(first_contact).to_degrees()
                <= AIRBORNE_FOOT_ROTATION_SPEED_DEGREES / CONTINUITY_SAMPLE_HZ + 0.0001
        );
        assert!(first_contact.angle_between(contact) < airborne.angle_between(contact));
    }

    #[test]
    fn run_foot_roll_has_heel_flat_and_toe_off_beats() {
        let mut run = SkeletonState::default();
        run.local_velocity = Vec3::new(0.0, 0.0, -5.5);
        run.world_velocity = Vec3::new(0.0, 0.0, -5.5);
        run.gait_phase = 0.84;
        assert!(run_foot_roll_degrees(&run, true) > 0.0, "heel prepares");
        run.gait_phase = 0.0;
        assert_eq!(run_foot_roll_degrees(&run, true), 0.0, "flat stance");
        run.gait_phase = 0.15;
        assert!(run_foot_roll_degrees(&run, true) < 0.0, "toe off");
        run.gait_phase = 0.25;
        assert_eq!(run_foot_roll_degrees(&run, true), 0.0, "neutral swing");
        run.gait_phase = 0.5;
        assert_eq!(run_foot_roll_degrees(&run, false), 0.0, "mirrored contact");
    }

    #[test]
    fn release_target_cap_preserves_the_knee_continuity_budget() {
        let maximum_target_step = AIRBORNE_RELEASE_TARGET_SPEED / CONTINUITY_SAMPLE_HZ;
        assert!(maximum_target_step * MAX_KNEE_TARGET_AMPLIFICATION < MAX_KNEE_STEP_METRES);
        assert!(maximum_target_step < 3.4 / CONTINUITY_SAMPLE_HZ);
    }

    #[test]
    fn raised_support_requires_rendered_sole_contact() {
        let terrain_height = 0.0;
        assert!(raised_support_is_actual(
            true,
            false,
            MEASURED_ANKLE_SOLE_OFFSET_METRES + SOLE_CONTACT_TOLERANCE_METRES - 0.001,
            terrain_height,
        ));
        assert!(!raised_support_is_actual(
            true,
            false,
            MEASURED_ANKLE_SOLE_OFFSET_METRES + 0.023,
            terrain_height,
        ));
        assert!(!raised_support_is_actual(
            false,
            true,
            MEASURED_ANKLE_SOLE_OFFSET_METRES,
            terrain_height,
        ));
        assert!(raised_support_is_actual(
            true,
            true,
            MEASURED_ANKLE_SOLE_OFFSET_METRES + SOLE_CONTACT_TOLERANCE_METRES + 0.004,
            terrain_height,
        ));
        assert!(!raised_support_has_toe_clearance(
            true,
            Some(-SOLE_CONTACT_TOLERANCE_METRES - 0.001),
            Some(terrain_height),
        ));
        assert!(raised_support_has_toe_clearance(
            true,
            Some(-SOLE_CONTACT_TOLERANCE_METRES + 0.001),
            Some(terrain_height),
        ));
        assert!(!raised_support_has_toe_clearance(
            false,
            Some(terrain_height),
            Some(terrain_height),
        ));
    }

    #[test]
    fn raised_stop_handoff_preserves_visible_targets_in_owner_space() {
        let rig_origin = Vec3::new(4.0, 0.0, -2.0);
        let rig_rotation = Quat::from_rotation_y(0.7);
        let left = Vec3::new(3.8, 0.1, -2.4);
        let right = Vec3::new(4.3, 0.1, -1.8);
        let raised = RaisedFootworkState {
            initialized: true,
            left_solve_target: Some(left),
            right_solve_target: Some(right),
            left_support_weight: 1.0,
            right_support_weight: 0.0,
            ..default()
        };
        let mut memory = LegIkMemory {
            left_support_weight: Some(1.0),
            right_support_weight: Some(0.0),
            ..default()
        };

        preserve_raised_handoff_targets(&mut memory, raised, rig_origin, rig_rotation);

        assert_eq!(memory.left_foot_world_target, Some(left));
        assert_eq!(memory.right_foot_world_target, Some(right));
        assert_eq!(memory.left_foot_plant, Some(left));
        assert_eq!(memory.right_foot_plant, Some(right));
        assert!(memory.left_foot_plant_acquired);
        assert!(!memory.right_foot_plant_acquired);
        assert_eq!(memory.left_transition_support_weight, Some(1.0));
        assert_eq!(memory.right_transition_support_weight, Some(0.0));
        assert!(memory.left_release_active && memory.right_release_active);
        let restored_left =
            rig_origin + rig_rotation * memory.left_foot_target.expect("left owner target");
        let restored_right =
            rig_origin + rig_rotation * memory.right_foot_target.expect("right owner target");
        assert!(restored_left.distance(left) < 0.000001);
        assert!(restored_right.distance(right) < 0.000001);
    }

    #[test]
    fn raised_release_handoff_retires_at_its_exact_terminal_sample() {
        let mut footwork = RaisedFootworkState {
            initialized: true,
            release_handoff_active: true,
            release_handoff_progress: 0.999,
            ..default()
        };

        assert!(!raised_release_handoff_is_complete(footwork));
        footwork.release_handoff_progress = 1.0;
        assert!(raised_release_handoff_is_complete(footwork));

        footwork.release_handoff_active = false;
        assert!(!raised_release_handoff_is_complete(footwork));
    }

    #[test]
    fn guard_release_retains_then_quinticly_releases_visible_pelvis_height() {
        let previous_visible = Vec3::new(0.0, -0.22, 0.01);
        let ordinary_authored = Vec3::new(0.0, 0.02, 0.0);
        let scalar_owner = Vec3::Y * -0.06;
        let decomposed_visible = previous_visible - scalar_owner;
        let offset = guard_release_pelvis_offset(Some(decomposed_visible), ordinary_authored);

        assert!((offset.y + 0.18).abs() < 0.000_001);
        assert_eq!(retained_guard_release_pelvis_offset(offset, 0.0), offset);
        assert_eq!(ordinary_authored + offset + scalar_owner, previous_visible);
        let midpoint = retained_guard_release_pelvis_offset(offset, 0.5);
        assert!((midpoint.y + 0.09).abs() < 0.000_001);
        assert_eq!(
            retained_guard_release_pelvis_offset(offset, 1.0),
            Vec3::ZERO
        );
        assert_eq!(
            guard_release_pelvis_offset(None, ordinary_authored),
            Vec3::ZERO
        );
    }

    #[test]
    fn raised_diagnostic_refresh_advances_once_per_fixed_tick() {
        assert!(raised_refresh_advances(None, 305));
        assert!(!raised_refresh_advances(Some(305), 305));
        assert!(raised_refresh_advances(Some(305), 306));
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // Locks the authored continuity budget constants.
    fn raised_stop_settle_keeps_terrain_ik_alive_across_ticks() {
        let mut settle = LocomotionSettleState {
            support_left: true,
            swing_start: Vec3::new(0.2, 0.1, 0.0),
            capture_point: Vec3::ZERO,
            landing_target: Vec3::new(-0.2, 0.1, -0.3),
            progress: 0.0,
            elapsed_seconds: 0.0,
            raised_handoff: true,
            stateful_follower: false,
        };

        assert!(terrain_ik_is_required(false, false, true));
        for tick in 0..4 {
            settle = advance_settle_state(settle, 1.0 / CONTINUITY_SAMPLE_HZ);
            assert!(terrain_ik_is_required(false, true, false), "tick {tick}");
            assert!(settle.progress > 0.0 && settle.progress < 1.0);
        }
        assert!(
            (settle.progress - 4.0 / CONTINUITY_SAMPLE_HZ / SETTLE_STEP_SECONDS).abs() < 0.0001
        );
        assert_eq!(settle_target_speed(settle), RAISED_SETTLE_TARGET_SPEED);
        assert!(RAISED_SETTLE_TARGET_SPEED < AIRBORNE_RELEASE_TARGET_SPEED);
        assert!(
            RAISED_SETTLE_TARGET_SPEED / CONTINUITY_SAMPLE_HZ * MAX_KNEE_TARGET_AMPLIFICATION
                + RAISED_SETTLE_PELVIS_KNEE_BUDGET_METRES
                < MAX_KNEE_STEP_METRES
        );
        assert!(!terrain_ik_is_required(false, false, false));
    }

    #[test]
    fn immediate_restart_cancels_settle_without_waiting_for_release_targets() {
        let settle = LocomotionSettleState {
            support_left: false,
            swing_start: Vec3::ZERO,
            capture_point: Vec3::Z,
            landing_target: Vec3::NEG_Z,
            progress: 0.4,
            elapsed_seconds: 0.1,
            raised_handoff: false,
            stateful_follower: false,
        };
        let mut memory = LegIkMemory {
            settle: Some(settle),
            left_foot_plant: Some(Vec3::new(-0.1, 0.085, -0.8)),
            right_foot_plant: Some(Vec3::new(0.1, 0.085, -0.9)),
            left_last_rendered_world: Some(Vec3::new(-0.1, 0.14, -0.4)),
            right_last_rendered_world: Some(Vec3::new(0.1, 0.15, -0.5)),
            left_last_rendered_owner: Some(Vec3::new(-0.1, -0.8, -0.4)),
            right_last_rendered_owner: Some(Vec3::new(0.1, -0.79, -0.5)),
            left_release_active: true,
            right_release_active: true,
            ..default()
        };
        let restarted_velocity = Vec3::new(2.0, 4.0, -3.0);

        cancel_settle_for_restart(&mut memory, restarted_velocity);

        assert!(memory.settle.is_none());
        assert_eq!(
            memory.recent_movement_velocity,
            restarted_velocity.with_y(0.0)
        );
        assert!(memory.left_release_active && memory.right_release_active);
        assert!(memory.left_foot_plant.is_none() && memory.right_foot_plant.is_none());
        assert_eq!(
            memory.left_foot_world_target,
            memory.left_last_rendered_world
        );
        assert_eq!(
            memory.right_foot_world_target,
            memory.right_last_rendered_world
        );
        assert_eq!(memory.left_foot_target, memory.left_last_rendered_owner);
        assert_eq!(memory.right_foot_target, memory.right_last_rendered_owner);
        assert_eq!(memory.left_transition_support_weight, Some(0.0));
        assert_eq!(memory.right_transition_support_weight, Some(0.0));
    }

    #[test]
    fn owner_discontinuity_clears_both_plans_and_all_frozen_trajectory_metadata() {
        let mut memory = LegIkMemory {
            left_planned_contact: Some(Vec3::new(-0.1, 0.2, -1.0)),
            right_planned_contact: Some(Vec3::new(0.1, 0.3, -2.0)),
            left_planned_contact_start: Some(Vec3::new(-0.1, 0.8, 0.0)),
            right_planned_contact_start: Some(Vec3::new(0.1, 0.7, -0.5)),
            left_planned_contact_phase_start: Some(0.8),
            right_planned_contact_phase_start: Some(0.3),
            ..default()
        };

        clear_all_planned_contact_metadata(&mut memory);

        assert!(memory.left_planned_contact.is_none());
        assert!(memory.right_planned_contact.is_none());
        assert!(memory.left_planned_contact_start.is_none());
        assert!(memory.right_planned_contact_start.is_none());
        assert!(memory.left_planned_contact_phase_start.is_none());
        assert!(memory.right_planned_contact_phase_start.is_none());
    }

    #[test]
    fn cancelled_settle_returns_to_run_inside_the_existing_knee_budget() {
        assert_eq!(
            run_airborne_owner_target_speed_for_sample(false, true),
            AIRBORNE_RELEASE_TARGET_SPEED
        );
        assert_eq!(
            run_airborne_owner_target_speed_for_sample(false, false),
            RUN_AIRBORNE_OWNER_TARGET_SPEED
        );

        // Native terrain-tap-restart-crossfade frames 39 -> 40: the settle
        // swing is cancelled as the owner resumes 5.5 m/s. The ordinary Run
        // budget moved the reachable ankle only 9.3 cm but amplified its
        // near-extension knee by 12.8 cm. The first-sample settle budget keeps
        // the transported analytic chain below the same 10 cm contract.
        let previous_root = Vec3::new(0.0, 3.0130908, -1.71875);
        let current_root = Vec3::new(0.0, 3.017059, -1.8046875);
        let previous_hip = Vec3::new(0.10195288, 3.057775, -1.7341061);
        let previous_knee = Vec3::new(0.13492808, 2.5361009, -1.7145816);
        let previous_ankle = Vec3::new(0.13445835, 2.1369128, -1.554793);
        let current_hip = Vec3::new(0.10195502, 3.0623627, -1.817662);
        let desired_ankle = Vec3::new(0.13222283, 2.1976547, -1.6857854);
        let previous_owner = previous_ankle - previous_root;
        let resolved_ankle = advance_run_airborne_world_target(
            Some(previous_owner),
            desired_ankle,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            run_airborne_owner_target_speed_for_sample(false, true),
            |_| Some(-100.0),
        );
        assert!(
            (resolved_ankle - current_root).distance(previous_owner)
                <= AIRBORNE_RELEASE_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );

        let upper_length = previous_hip.distance(previous_knee);
        let lower_length = previous_knee.distance(previous_ankle);
        let previous_end_direction = (previous_ankle - previous_hip).normalize();
        let previous_pole = (previous_knee - previous_hip)
            .reject_from_normalized(previous_end_direction)
            .normalize();
        let next_end_direction = (resolved_ankle - current_hip).normalize();
        let pole = transported_terrain_pole(
            Some(previous_pole),
            Some(previous_end_direction),
            next_end_direction,
            previous_pole,
        )
        .expect("the settle knee pole remains transportable on restart");
        let solution = solve_two_bone_with_reach(
            current_hip,
            previous_knee,
            previous_ankle,
            resolved_ankle,
            upper_length,
            lower_length,
            pole,
            maximum_reach(upper_length, lower_length),
        )
        .expect("the bounded restart target remains reachable");
        let knee_root_relative_step =
            (solution.knee - current_root).distance(previous_knee - previous_root);
        assert!(knee_root_relative_step <= MAX_KNEE_STEP_METRES);
    }

    #[test]
    fn toe_aware_settle_height_couples_ankle_clearance_to_the_visible_toe_lever() {
        // Native stop frame 25 had an 11.54 cm ankle clearance but a -1.72 cm
        // toe clearance. Preserve that measured 13.26 cm lever while asking
        // the next target for the strict +1.1 cm transition toe floor.
        let rendered_ankle = Vec3::new(0.14, 0.1154449, -1.5);
        let rendered_toe = Vec3::new(0.14, -0.017214656, -1.62);
        let minimum = toe_aware_minimum_ankle_y(
            rendered_ankle,
            rendered_toe,
            Vec2::new(0.14, -1.7),
            TERRAIN_TRANSITION_FLIGHT_TOE_CLEARANCE_METRES,
            |_| Some(0.0),
        )
        .unwrap();
        assert!((minimum - 0.14365956).abs() <= 0.000001);
        let rotation_safe_clearance = transition_toe_clearance_with_rotation_margin(
            rendered_ankle,
            rendered_toe,
            1.0 / CONTINUITY_SAMPLE_HZ,
        );
        assert!(rotation_safe_clearance > 0.03);
        let resolved = advance_run_airborne_world_target(
            Some(rendered_ankle),
            Vec3::new(0.14, 0.05, -1.55),
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            AIRBORNE_RELEASE_TARGET_SPEED,
            |_| Some(minimum),
        );
        assert!(resolved.y + 0.000001 >= minimum);
        assert!(
            resolved.distance(rendered_ankle)
                <= AIRBORNE_RELEASE_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );

        let contact_minimum = toe_aware_minimum_ankle_y(
            Vec3::new(0.21, 0.085, 0.0),
            Vec3::new(0.21, -0.015733838, -0.1),
            Vec2::new(0.21, 0.0),
            TERRAIN_CONTACT_TOE_CLEARANCE_METRES,
            |_| Some(0.0),
        )
        .unwrap();
        assert!(contact_minimum > MEASURED_ANKLE_SOLE_OFFSET_METRES);
        assert!((contact_minimum - 0.09173384).abs() <= 0.000001);
    }

    #[test]
    fn airborne_settle_support_lands_atomically_once_contact_is_reachable() {
        let contact = Vec3::new(0.1, 0.09173384, -0.5);
        let previous = contact + Vec3::Y * 0.04;
        let contact_candidate = advance_run_airborne_world_target(
            Some(previous),
            contact,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            AIRBORNE_RELEASE_TARGET_SPEED,
            |_| Some(MEASURED_ANKLE_SOLE_OFFSET_METRES),
        );
        assert!(contact_candidate.distance_squared(contact) <= 0.000001);

        let flight_candidate = advance_run_airborne_world_target(
            Some(previous),
            contact,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            AIRBORNE_RELEASE_TARGET_SPEED,
            |_| Some(0.14365956),
        );
        assert!(flight_candidate.distance_squared(contact) > 0.000001);
        // The production branch selects contact_candidate in this state, so
        // the same sample can report truthful support instead of reclamping
        // to the airborne floor forever.
        assert_eq!(contact_candidate, contact);
    }

    #[test]
    fn terminal_contact_preparation_preserves_the_visible_pelvis_shift() {
        let left = Vec3::new(-0.1, 0.085, 0.0);
        let right = Vec3::new(0.1, 0.085, -0.4);
        let mut memory = LegIkMemory {
            settle: Some(LocomotionSettleState {
                support_left: true,
                swing_start: right,
                capture_point: Vec3::NEG_Z,
                landing_target: right,
                progress: 1.0,
                elapsed_seconds: 0.5,
                raised_handoff: false,
                stateful_follower: false,
            }),
            pelvis_shift: -0.21,
            left_last_rendered_world: Some(left),
            right_last_rendered_world: Some(right),
            ..default()
        };

        assert!(prepare_terminal_settle_contacts(
            &mut memory,
            Vec3::ZERO,
            Quat::IDENTITY,
            |_| Some(0.0),
        ));
        assert_eq!(memory.terminal_reach_shift, -0.21);
        assert!(memory.terminal_reach_target_shift.is_none());
    }

    #[test]
    fn completed_settle_promotes_both_targets_to_stable_idle_plants() {
        let mut memory = LegIkMemory {
            settle: Some(LocomotionSettleState {
                support_left: true,
                swing_start: Vec3::ZERO,
                capture_point: Vec3::NEG_Z,
                landing_target: Vec3::new(0.2, 0.085, -0.4),
                progress: 1.0,
                elapsed_seconds: 0.4,
                raised_handoff: false,
                stateful_follower: false,
            }),
            recent_movement_velocity: Vec3::NEG_Z * 5.5,
            left_foot_plant: Some(Vec3::NEG_Z),
            left_foot_world_target: Some(Vec3::new(-0.2, 0.085, 0.0)),
            right_foot_world_target: Some(Vec3::new(0.2, 0.085, -0.5)),
            left_release_active: true,
            right_release_active: true,
            left_support_exhausted_until_flight: true,
            left_terrain_pole_world: Some(Vec3::Z),
            ..default()
        };

        finish_settle_for_idle(&mut memory);

        assert!(memory.settle.is_none());
        assert_eq!(memory.recent_movement_velocity, Vec3::ZERO);
        assert_eq!(memory.left_foot_plant, memory.left_foot_world_target);
        assert_eq!(memory.right_foot_plant, memory.right_foot_world_target);
        assert!(memory.left_foot_plant_acquired && memory.right_foot_plant_acquired);
        assert_eq!(memory.left_transition_support_weight, Some(1.0));
        assert_eq!(memory.right_transition_support_weight, Some(1.0));
        assert!(!memory.left_support_exhausted_until_flight);
        assert!(!memory.right_support_exhausted_until_flight);
        assert!(!memory.left_release_active && !memory.right_release_active);
        assert_eq!(memory.left_terrain_pole_world, Some(Vec3::Z));
    }

    #[test]
    fn terminal_settle_with_idle_followers_finishes_on_dual_terrain_contacts() {
        let settle = advance_settle_state(
            LocomotionSettleState {
                support_left: true,
                swing_start: Vec3::ZERO,
                capture_point: Vec3::NEG_Z,
                landing_target: Vec3::new(0.2, 0.085, -0.4),
                progress: 0.99,
                elapsed_seconds: 0.4,
                raised_handoff: false,
                stateful_follower: false,
            },
            1.0 / CONTINUITY_SAMPLE_HZ,
        );
        let mut memory = LegIkMemory {
            settle: Some(settle),
            left_foot_world_target: Some(Vec3::new(-0.12, 0.160, -0.2)),
            right_foot_world_target: Some(Vec3::new(0.12, 0.080, -0.5)),
            left_release_active: false,
            right_release_active: false,
            ..default()
        };
        assert!(settle_is_terminal(&memory));
        assert!(prepare_terminal_settle_contacts(
            &mut memory,
            Vec3::ZERO,
            Quat::IDENTITY,
            |_| Some(0.0),
        ));
        assert!(memory.settle.is_some());
        assert!(!terminal_settle_contacts_are_rendered(&memory, |_| Some(
            0.0
        ),));
        memory.left_last_rendered_world = memory.left_foot_world_target;
        memory.right_last_rendered_world = memory.right_foot_world_target;
        memory.left_last_rendered_toe_world = Some(Vec3::new(-0.12, 0.005, -0.3));
        memory.right_last_rendered_toe_world = Some(Vec3::new(0.12, 0.005, -0.6));
        assert!(terminal_settle_contacts_are_rendered(&memory, |_| Some(
            0.0
        ),));
        finish_settle_for_idle(&mut memory);
        assert!(memory.settle.is_none());
        assert_eq!(
            memory.left_foot_plant.unwrap().y,
            MEASURED_ANKLE_SOLE_OFFSET_METRES
        );
        assert_eq!(
            memory.right_foot_plant.unwrap().y,
            MEASURED_ANKLE_SOLE_OFFSET_METRES
        );
        assert_eq!(memory.left_support_weight, Some(1.0));
        assert_eq!(memory.right_support_weight, Some(1.0));
    }

    #[test]
    fn terminal_settle_lowers_shared_root_until_both_contacts_are_reachable() {
        // Production-like geometry from the stop capture: the ankle target is
        // at terrain contact, but the restored idle hip leaves the chain more
        // than eight centimetres short. Terminal settle must keep requesting
        // a bounded shared-root drop instead of promoting false support.
        let upper = Vec3::new(-0.10, 3.08, -1.00);
        let target = Vec3::new(-0.12, 2.13, -1.38);
        let reach = 0.953;
        let required = required_hip_shift_for_reach(upper, target, reach).clamp(-0.25, 0.0);
        assert!(required < -0.05);

        let mut shift = 0.0;
        let base_root = Vec3::new(0.0, 1.0, 0.0);
        for _ in 0..16 {
            let next = advance_pelvis_shift(shift, required, 1.0 / CONTINUITY_SAMPLE_HZ);
            assert!((next - shift).abs() <= MAX_PELVIS_CORRECTION_STEP + 0.0001);
            shift = next;
            // Sparse idle FK may preserve the preceding procedural local.
            // Absolute application from the frozen base must still converge,
            // rather than repeatedly adding the retained scalar.
            let applied_root = base_root + Vec3::Y * shift;
            assert!((applied_root.y - (base_root.y + shift)).abs() <= 0.0001);
        }
        assert!((shift - required).abs() <= 0.0001);
        let applied_root = base_root + Vec3::Y * shift;
        assert!((applied_root.y - (base_root.y + required)).abs() <= 0.0001);

        let lowered_upper = upper + Vec3::Y * shift;
        assert!(lowered_upper.distance(target) <= reach + 0.0001);

        let memory = LegIkMemory {
            settle: Some(LocomotionSettleState {
                support_left: true,
                swing_start: target,
                capture_point: target,
                landing_target: target,
                progress: 1.0,
                elapsed_seconds: 1.0,
                raised_handoff: false,
                stateful_follower: false,
            }),
            left_foot_world_target: Some(target),
            right_foot_world_target: Some(target + Vec3::X * 0.24),
            left_last_rendered_world: Some(target + Vec3::Y * 0.075),
            right_last_rendered_world: Some(target + Vec3::X * 0.24),
            left_last_rendered_toe_world: Some(target + Vec3::Y * 0.075),
            right_last_rendered_toe_world: Some(target + Vec3::X * 0.24),
            ..default()
        };
        assert!(!terminal_settle_contacts_are_rendered(&memory, |_| Some(
            2.045
        )));
    }

    #[test]
    fn shared_pelvis_follower_bounds_acceleration_and_jerk_through_owner_reversal() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut state = PelvisFollowerState::default();
        let mut previous = state;
        for sample in 0..96 {
            let desired = if sample < 32 { -0.22 } else { 0.08 };
            state = advance_pelvis_follower(state, desired, dt);
            let measured_jerk = (state.acceleration - previous.acceleration) / dt;
            assert!(state.acceleration.abs() <= PELVIS_FOLLOWER_MAXIMUM_ACCELERATION + 0.0001);
            assert!(measured_jerk.abs() <= PELVIS_FOLLOWER_MAXIMUM_JERK + 0.0001);
            assert!((state.position - previous.position).abs() < 0.02);
            previous = state;
        }
        assert!((state.position - 0.08).abs() < 0.01);

        // Ownership transfers the complete motion state, not only its scalar
        // position, so the receiver's first step observes the same bounds.
        let handed_off = state;
        let next = advance_pelvis_follower(handed_off, 0.0, dt);
        assert!(
            ((next.acceleration - handed_off.acceleration) / dt).abs()
                <= PELVIS_FOLLOWER_MAXIMUM_JERK + 0.0001
        );
    }

    #[test]
    fn raised_pelvis_owner_seeds_the_complete_ordinary_motion_state_once() {
        let ordinary = PelvisFollowerState {
            position: -0.14,
            velocity: -0.35,
            acceleration: 2.25,
        };
        let mut memory = LegIkMemory {
            pelvis_shift: ordinary.position,
            pelvis_shift_velocity: ordinary.velocity,
            pelvis_shift_acceleration: ordinary.acceleration,
            ..default()
        };
        assert_eq!(raised_pelvis_follower_seed(memory), ordinary);

        memory.raised_pelvis_shift = -0.10;
        memory.raised_pelvis_shift_velocity = 0.2;
        memory.raised_pelvis_shift_acceleration = -1.0;
        memory.raised_pelvis_follower_valid = true;
        assert_eq!(
            raised_pelvis_follower_seed(memory),
            PelvisFollowerState {
                position: -0.10,
                velocity: 0.2,
                acceleration: -1.0,
            }
        );
    }

    #[test]
    fn stationary_guard_acquisition_consumes_last_truthful_dual_support() {
        let rig_origin = Vec3::new(3.0, 1.2, -4.0);
        let first_rotation = Quat::from_rotation_y(0.25);
        let left = rig_origin + first_rotation * Vec3::new(-0.12, -0.91, -0.08);
        let right = rig_origin + first_rotation * Vec3::new(0.12, -0.91, -0.08);
        let pelvis = PelvisFollowerState {
            position: -0.06045,
            velocity: 0.03,
            acceleration: -0.4,
        };
        let mut memory = LegIkMemory {
            left_foot_plant: Some(left),
            right_foot_plant: Some(right),
            left_foot_plant_acquired: true,
            right_foot_plant_acquired: true,
            left_support_weight: Some(1.0),
            right_support_weight: Some(1.0),
            pelvis_shift: pelvis.position,
            pelvis_shift_velocity: pelvis.velocity,
            pelvis_shift_acceleration: pelvis.acceleration,
            ..default()
        };
        retain_last_dual_support_handoff(&mut memory, rig_origin, first_rotation);

        // The authored guard clip owns one frame before raised IK and reports
        // floating, unsupported feet. It may not erase the retained contact.
        memory.left_foot_world_target = Some(left + Vec3::Y * 0.05);
        memory.right_foot_world_target = Some(right + Vec3::Y * 0.03);
        memory.left_foot_plant = None;
        memory.right_foot_plant = None;
        memory.left_foot_plant_acquired = false;
        memory.right_foot_plant_acquired = false;
        memory.left_support_weight = Some(0.0);
        memory.right_support_weight = Some(0.0);
        memory.pelvis_shift = 0.0;

        let next_origin = rig_origin + Vec3::new(0.02, 0.0, -0.01);
        let next_rotation = Quat::from_rotation_y(0.30);
        assert!(restore_stationary_guard_handoff(
            &mut memory,
            next_origin,
            next_rotation,
        ));
        assert_eq!(memory.left_support_weight, Some(1.0));
        assert_eq!(memory.right_support_weight, Some(1.0));
        assert!(memory.left_foot_plant_acquired && memory.right_foot_plant_acquired);
        assert_eq!(raised_pelvis_follower_seed(memory), pelvis);
        assert_eq!(
            memory.left_foot_plant,
            memory
                .last_dual_support_left_owner
                .map(|local| next_origin + next_rotation * local)
        );
        clear_last_dual_support_handoff(&mut memory);
        assert!(!restore_stationary_guard_handoff(
            &mut memory,
            next_origin + Vec3::new(8.0, 0.0, 0.0),
            next_rotation,
        ));
    }

    #[test]
    fn rejected_stationary_pivot_cannot_leave_ownerless_cadence_awaiting() {
        let presented = Vec3::new(-0.2, 0.084, 0.1);
        let mut footwork = RaisedFootworkState {
            initialized: true,
            pivot_active: true,
            pivot_left: true,
            pivot_progress: 0.4,
            swing_release_owner_active: true,
            swing_emergency_brake: Some(EmergencyFootBrake {
                stationary_ideal: presented,
                owner_local_ideal: None,
            }),
            awaiting_step_sequence: true,
            ..default()
        };
        assert!(cancel_rejected_stationary_pivot(&mut footwork, presented));
        assert_eq!(footwork.left_plant, presented);
        assert!(!footwork.pivot_active);
        assert!(!footwork.awaiting_step_sequence);
        assert!(!footwork.swing_release_owner_active);
        assert!(footwork.swing_emergency_brake.is_none());
    }

    #[test]
    fn settled_swing_brake_cannot_leave_ownerless_cadence_awaiting() {
        assert!(!guard_emergency_settlement_awaits_cadence(false, false));
        assert!(guard_emergency_settlement_awaits_cadence(true, false));
        assert!(!guard_emergency_settlement_awaits_cadence(true, true));
        let replicated_moving = true;
        let quickstep_handoff_active = true;
        let cadence_can_advance = replicated_moving && !quickstep_handoff_active;
        assert!(!guard_emergency_settlement_awaits_cadence(
            cadence_can_advance,
            false
        ));

        let mut footwork = RaisedFootworkState {
            initialized: true,
            awaiting_step_sequence: true,
            ..default()
        };
        assert!(clear_ownerless_guard_wait(&mut footwork));
        assert!(!footwork.awaiting_step_sequence);

        // The same invariant applies on the sequence-zero movement onset: a
        // same-tick settlement cannot turn a rejected acquisition into a bare
        // wait for the first half-step edge.
        footwork.awaiting_step_sequence = true;
        assert!(clear_ownerless_guard_wait(&mut footwork));
        assert!(!footwork.awaiting_step_sequence);

        footwork.awaiting_step_sequence = true;
        let fixed_world_brake = EmergencyFootBrake {
            stationary_ideal: Vec3::ZERO,
            owner_local_ideal: None,
        };
        footwork.swing_emergency_brake = Some(fixed_world_brake);
        assert!(!clear_ownerless_guard_wait(&mut footwork));
        assert!(footwork.awaiting_step_sequence);
        assert!(guard_emergency_brake_has_settled(
            fixed_world_brake,
            false,
            Vec3::ZERO,
            Vec3::ZERO
        ));

        let moving_body_relative_brake = EmergencyFootBrake {
            stationary_ideal: Vec3::ZERO,
            owner_local_ideal: Some(Vec3::new(0.2, -0.9, 0.1)),
        };
        assert!(!guard_emergency_brake_has_settled(
            moving_body_relative_brake,
            false,
            Vec3::ZERO,
            Vec3::ZERO
        ));
        assert!(guard_emergency_brake_has_settled(
            moving_body_relative_brake,
            true,
            Vec3::splat(10.0),
            Vec3::splat(10.0)
        ));

        // A brake retained from the preceding tick suppresses the stationary
        // pose branch, but it does not make the replicated planted cadence a
        // moving cadence. Settlement must use semantic intent rather than the
        // precomputed pose-owner predicate.
        assert!(!guard_stationary_owns_pose(true, true));
        footwork.swing_emergency_brake = None;
        footwork.swing_release_owner_active = false;
        footwork.awaiting_step_sequence = guard_emergency_settlement_awaits_cadence(false, false);
        assert!(!footwork.awaiting_step_sequence);
        assert!(!clear_ownerless_guard_wait(&mut footwork));
    }

    #[test]
    fn raised_pelvis_reach_uses_only_the_true_contact_owner() {
        let mut footwork = RaisedFootworkState {
            initialized: true,
            swing_left: true,
            left_support_weight: 1.0,
            right_support_weight: 1.0,
            ..default()
        };
        assert!(!raised_leg_contributes_pelvis_reach(&footwork, true, true));
        assert!(raised_leg_contributes_pelvis_reach(&footwork, true, false));

        footwork.right_support_release_owner =
            Some(SupportReleaseOwner::EmergencyBrake(EmergencyFootBrake {
                stationary_ideal: Vec3::ZERO,
                owner_local_ideal: None,
            }));
        assert!(!raised_leg_contributes_pelvis_reach(&footwork, true, false));
        footwork.right_support_release_owner = None;
        footwork.release_handoff_active = true;
        assert!(!raised_leg_contributes_pelvis_reach(&footwork, true, false));
    }

    #[test]
    fn moving_guard_pelvis_acquisition_holds_contact_then_converges_c2() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let start = Transform::from_translation(Vec3::new(0.01, 0.10, 0.91));
        let authored = Transform::from_translation(Vec3::new(0.01, 0.01, 1.00));
        let mut footwork = RaisedFootworkState {
            initialized: true,
            was_moving: false,
            visible_pelvis_local_transform: Some(start),
            ..default()
        };

        let onset = condition_guard_pelvis_acquisition(
            &mut footwork,
            authored,
            Some(Vec3::ZERO),
            true,
            0,
            GuardPelvisTrajectorySignature::default(),
            true,
            dt,
        );
        assert_eq!(onset, start);
        footwork.was_moving = true;
        for _ in 0..8 {
            assert_eq!(
                condition_guard_pelvis_acquisition(
                    &mut footwork,
                    authored,
                    Some(Vec3::ZERO),
                    true,
                    0,
                    GuardPelvisTrajectorySignature::default(),
                    true,
                    dt,
                ),
                start
            );
        }

        let mut previous_position = start.translation;
        let mut previous_velocity = Vec3::ZERO;
        let mut previous_acceleration = Vec3::ZERO;
        let mut final_transform = start;
        for _ in 0..256 {
            final_transform = condition_guard_pelvis_acquisition(
                &mut footwork,
                authored,
                Some(Vec3::ZERO),
                true,
                1,
                GuardPelvisTrajectorySignature::default(),
                true,
                dt,
            );
            authorize_guard_pelvis_acquisition(&mut footwork, true);
            let velocity = (final_transform.translation - previous_position) / dt;
            let acceleration = (velocity - previous_velocity) / dt;
            let jerk = (acceleration - previous_acceleration) / dt;
            assert!(
                acceleration.length() <= PELVIS_FOLLOWER_MAXIMUM_ACCELERATION + 0.6,
                "acceleration={acceleration:?}"
            );
            assert!(
                jerk.length() <= PELVIS_FOLLOWER_MAXIMUM_JERK + 20.0,
                "jerk={jerk:?}"
            );
            assert!(final_transform.translation.y <= start.translation.y + 0.000001);
            assert!(final_transform.translation.y >= authored.translation.y - 0.000001);
            previous_position = final_transform.translation;
            previous_velocity = velocity;
            previous_acceleration = acceleration;
            if footwork.pelvis_acquisition.is_none() {
                break;
            }
        }
        assert!(footwork.pelvis_acquisition.is_none());
        assert!(final_transform.translation.distance(authored.translation) <= 0.000001);
    }

    #[test]
    fn combined_guard_pelvis_owner_keeps_support_inside_warning_reach() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let start = Transform::IDENTITY;
        let authored = Transform::from_translation(Vec3::Y * 0.09);
        let mut footwork = RaisedFootworkState {
            initialized: true,
            left_support_weight: 1.0,
            right_support_weight: 1.0,
            visible_pelvis_local_transform: Some(start),
            ..default()
        };
        assert_eq!(
            condition_guard_pelvis_acquisition(
                &mut footwork,
                authored,
                Some(Vec3::ZERO),
                true,
                0,
                GuardPelvisTrajectorySignature::default(),
                true,
                dt,
            ),
            start
        );
        footwork.was_moving = true;

        let upper_length = 0.52308;
        let lower_length = 0.42998;
        let warning = guard_warning_reach(upper_length, lower_length);
        let ankle = Vec3::ZERO;
        let resting_hip = Vec3::Y * 0.886;
        let mut scalar = PelvisFollowerState::default();
        let mut recovery = None;
        for _ in 0..256 {
            let residual = condition_guard_pelvis_acquisition(
                &mut footwork,
                authored,
                Some(Vec3::Y * scalar.position),
                true,
                1,
                GuardPelvisTrajectorySignature::default(),
                true,
                dt,
            );
            let uncorrected_hip = resting_hip + Vec3::Y * residual.translation.y;
            let future_authored_delta = authored.translation - residual.translation;
            let desired = required_hip_shift_for_reach(
                uncorrected_hip + future_authored_delta,
                ankle,
                warning,
            )
            .clamp(-0.25, 0.0);
            scalar = advance_pelvis_follower_with_recovery(scalar, &mut recovery, desired, dt);
            authorize_guard_pelvis_acquisition(
                &mut footwork,
                (scalar.position - desired).abs() <= 0.000001
                    && scalar.velocity.abs() <= 0.000001
                    && scalar.acceleration.abs() <= 0.000001,
            );
            let presented_hip = uncorrected_hip + Vec3::Y * scalar.position;
            assert!(
                presented_hip.distance(ankle) <= warning + 0.0005,
                "hip={presented_hip:?} scalar={scalar:?} desired={desired}"
            );
            assert_eq!(footwork.left_support_weight, 1.0);
            assert_eq!(footwork.right_support_weight, 1.0);
            assert!(footwork.swing_replan_segment.is_none());
            assert!(footwork.swing_emergency_brake.is_none());
            assert!(!footwork.awaiting_step_sequence);
            if footwork.pelvis_acquisition.is_none() {
                break;
            }
        }
        assert!(footwork.pelvis_acquisition.is_none());
    }

    #[test]
    fn guard_pelvis_acquisition_retargets_from_terminal_without_an_authored_seam() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let start = Transform::from_translation(Vec3::new(0.0, 0.0, 0.9));
        let mut footwork = RaisedFootworkState {
            initialized: true,
            visible_pelvis_local_transform: Some(start),
            ..default()
        };
        let first_authored = Transform {
            translation: Vec3::new(0.0, 0.09, 0.98),
            rotation: Quat::from_rotation_y(0.12),
            scale: Vec3::splat(1.02),
        };
        let onset = condition_guard_pelvis_acquisition(
            &mut footwork,
            first_authored,
            Some(Vec3::ZERO),
            true,
            0,
            GuardPelvisTrajectorySignature::default(),
            true,
            dt,
        );
        assert_eq!(onset.translation, start.translation);
        assert_eq!(onset.rotation, start.rotation);
        assert_eq!(onset.scale, start.scale);
        footwork.was_moving = true;

        let mut previous = start;
        let mut final_authored = first_authored;
        for tick in 0..256 {
            final_authored = if tick < 80 {
                Transform {
                    translation: Vec3::new(0.0, 0.09 - tick as f32 * 0.0005, 0.98),
                    rotation: Quat::from_rotation_y(0.12 - tick as f32 * 0.0004),
                    scale: Vec3::splat(1.02 - tick as f32 * 0.00005),
                }
            } else {
                Transform {
                    translation: Vec3::new(0.0, 0.05, 0.98),
                    rotation: Quat::from_rotation_y(0.088),
                    scale: Vec3::splat(1.016),
                }
            };
            let presented = condition_guard_pelvis_acquisition(
                &mut footwork,
                final_authored,
                Some(Vec3::ZERO),
                true,
                1,
                GuardPelvisTrajectorySignature::default(),
                true,
                dt,
            );
            authorize_guard_pelvis_acquisition(&mut footwork, true);
            assert!(presented.translation.is_finite());
            assert!(presented.rotation.is_finite());
            assert!(presented.scale.is_finite());
            assert!(presented.translation.distance(previous.translation) < 0.02);
            previous = presented;
            if footwork.pelvis_acquisition.is_none() {
                break;
            }
        }
        assert!(footwork.pelvis_acquisition.is_none());
        assert!(
            previous
                .translation
                .abs_diff_eq(final_authored.translation, 0.000001)
        );
        assert_eq!(previous.rotation, final_authored.rotation);
        assert_eq!(previous.scale, final_authored.scale);
        let retired = condition_guard_pelvis_acquisition(
            &mut footwork,
            final_authored,
            Some(Vec3::ZERO),
            true,
            1,
            GuardPelvisTrajectorySignature::default(),
            true,
            dt,
        );
        assert!(
            retired
                .translation
                .abs_diff_eq(previous.translation, 0.000001)
        );
    }

    #[test]
    fn guard_pelvis_command_change_reproves_the_active_translation_without_restarting_it() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let start = Transform::from_translation(Vec3::Y * 0.02);
        let authored = Transform::from_translation(Vec3::Y * 0.11);
        let first_signature = GuardPelvisTrajectorySignature {
            command_velocity: Vec3::Z,
            sequence: 1,
            ..default()
        };
        let changed_signature = GuardPelvisTrajectorySignature {
            command_velocity: Vec3::X,
            sequence: 1,
            source_tick: 5,
            presentation_tick: 1,
            ..default()
        };
        let mut footwork = RaisedFootworkState {
            initialized: true,
            visible_pelvis_local_transform: Some(start),
            ..default()
        };
        condition_guard_pelvis_acquisition(
            &mut footwork,
            authored,
            Some(Vec3::ZERO),
            true,
            0,
            first_signature,
            false,
            dt,
        );
        footwork.was_moving = true;
        condition_guard_pelvis_acquisition(
            &mut footwork,
            authored,
            Some(Vec3::ZERO),
            true,
            1,
            first_signature,
            false,
            dt,
        );
        authorize_guard_pelvis_acquisition(&mut footwork, true);
        for _ in 0..5 {
            condition_guard_pelvis_acquisition(
                &mut footwork,
                authored,
                Some(Vec3::ZERO),
                true,
                1,
                first_signature,
                true,
                dt,
            );
        }
        let before = guard_pelvis_translation_sample(
            footwork.pelvis_acquisition.expect("active acquisition"),
        );

        let prepared = condition_guard_pelvis_acquisition(
            &mut footwork,
            authored,
            Some(Vec3::ZERO),
            true,
            1,
            changed_signature,
            false,
            dt,
        );
        let reproved = footwork.pelvis_acquisition.expect("active acquisition");
        assert!(prepared.translation.abs_diff_eq(before.position, 0.000001));
        assert!(reproved.progress > 0.0);
        assert_eq!(reproved.start.translation, start.translation);
        assert_eq!(reproved.start_velocity, Vec3::ZERO);
        assert_eq!(reproved.start_acceleration, Vec3::ZERO);
        assert!(!reproved.advance_authorized);

        authorize_guard_pelvis_acquisition(&mut footwork, true);
        let advanced = condition_guard_pelvis_acquisition(
            &mut footwork,
            authored,
            Some(Vec3::ZERO),
            true,
            1,
            changed_signature,
            true,
            dt,
        );
        assert!(advanced.translation.is_finite());
        assert_ne!(advanced.translation, prepared.translation);
    }

    #[test]
    fn guard_pelvis_replan_duration_proves_nonzero_boundary_dynamics() {
        let start = Vec3::new(0.01, 0.04, -0.02);
        let velocity = Vec3::new(0.08, -0.16, 0.04);
        let acceleration = Vec3::new(-1.2, 0.8, 0.3);
        let end = Vec3::new(0.03, 0.11, 0.01);
        let duration = guard_pelvis_replan_duration(start, velocity, acceleration, end);
        assert!(quintic_vector_dynamics_are_bounded(
            start,
            velocity,
            acceleration,
            end,
            duration,
            PELVIS_FOLLOWER_MAXIMUM_ACCELERATION,
            PELVIS_FOLLOWER_MAXIMUM_JERK,
        ));
        if duration > GUARD_PELVIS_ACQUISITION_SECONDS + 1.0 / CONTINUITY_SAMPLE_HZ {
            assert!(!quintic_vector_dynamics_are_bounded(
                start,
                velocity,
                acceleration,
                end,
                duration - 1.0 / CONTINUITY_SAMPLE_HZ,
                PELVIS_FOLLOWER_MAXIMUM_ACCELERATION,
                PELVIS_FOLLOWER_MAXIMUM_JERK,
            ));
        }
    }

    #[test]
    fn guard_pelvis_cadence_admission_uses_exact_integer_ticks() {
        let acquisition = GuardPelvisAcquisition {
            start: Transform::IDENTITY,
            start_velocity: Vec3::ZERO,
            start_acceleration: Vec3::ZERO,
            target: Some(Transform::from_translation(Vec3::Y * 0.09)),
            start_sequence: 1,
            progress: 0.0,
            duration_seconds: 20.0 / CONTINUITY_SAMPLE_HZ,
            advance_authorized: false,
            trajectory_signature: GuardPelvisTrajectorySignature::default(),
        };
        assert!(guard_pelvis_segment_fits_remaining_cadence_ticks(
            acquisition,
            0.32,
        ));
        assert!(guard_pelvis_segment_fits_remaining_cadence_ticks(
            GuardPelvisAcquisition {
                duration_seconds: 21.0 / CONTINUITY_SAMPLE_HZ,
                ..acquisition
            },
            0.32,
        ));
        assert!(!guard_pelvis_segment_fits_remaining_cadence_ticks(
            GuardPelvisAcquisition {
                duration_seconds: 22.0 / CONTINUITY_SAMPLE_HZ,
                ..acquisition
            },
            0.32,
        ));
        assert!(guard_pelvis_segment_fits_remaining_cadence_ticks(
            acquisition,
            (1.0 - 0.017 * 2.0) * 0.32,
        ));

        let mut footwork = RaisedFootworkState {
            was_moving: true,
            pelvis_acquisition: Some(acquisition),
            ..default()
        };
        let authored = Transform::from_translation(Vec3::Y * 0.085);
        let presented = condition_guard_pelvis_acquisition(
            &mut footwork,
            authored,
            Some(Vec3::ZERO),
            true,
            2,
            GuardPelvisTrajectorySignature {
                sequence: 2,
                presentation_tick: 18,
                ..default()
            },
            false,
            1.0 / CONTINUITY_SAMPLE_HZ,
        );
        let rebased = footwork.pelvis_acquisition.expect("rebased rest owner");
        assert_eq!(rebased.start_sequence, 2);
        assert_eq!(rebased.progress, 0.0);
        assert_eq!(presented, acquisition.start);
        assert_eq!(rebased.target, Some(authored));
    }

    #[test]
    fn recovered_guard_pelvis_reproof_advances_the_existing_bounded_path() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut acquisition = GuardPelvisAcquisition {
            start: Transform::from_translation(Vec3::Y * 0.02),
            start_velocity: Vec3::ZERO,
            start_acceleration: Vec3::ZERO,
            target: Some(Transform::from_translation(Vec3::Y * 0.11)),
            start_sequence: 1,
            progress: 0.28,
            duration_seconds: 0.32,
            advance_authorized: true,
            trajectory_signature: GuardPelvisTrajectorySignature {
                sequence: 2,
                presentation_tick: 9,
                ..default()
            },
        };
        let boundary = guard_pelvis_translation_sample(acquisition);
        assert!(boundary.velocity.length() > 0.0);
        let mut footwork = RaisedFootworkState {
            pelvis_acquisition: Some(acquisition),
            was_moving: true,
            ..default()
        };
        let signature = GuardPelvisTrajectorySignature {
            sequence: 2,
            presentation_tick: 9,
            ..default()
        };
        let presented = condition_guard_pelvis_acquisition(
            &mut footwork,
            acquisition.target.unwrap(),
            Some(Vec3::ZERO),
            true,
            2,
            signature,
            true,
            dt,
        );
        acquisition = footwork.pelvis_acquisition.expect("active owner");
        assert!(acquisition.advance_authorized);
        assert!(acquisition.progress > 0.28);
        assert_eq!(acquisition.start.translation, Vec3::Y * 0.02);
        assert!(quintic_vector_dynamics_are_bounded(
            acquisition.start.translation,
            acquisition.start_velocity,
            acquisition.start_acceleration,
            acquisition.target.unwrap().translation,
            acquisition.duration_seconds,
            PELVIS_FOLLOWER_MAXIMUM_ACCELERATION,
            PELVIS_FOLLOWER_MAXIMUM_JERK,
        ));
        assert_ne!(presented.translation, boundary.position);
    }

    #[test]
    fn only_an_advancing_pelvis_owner_blocks_stationary_foot_recentering() {
        let mut acquisition = GuardPelvisAcquisition {
            start: Transform::IDENTITY,
            start_velocity: Vec3::ZERO,
            start_acceleration: Vec3::ZERO,
            target: Some(Transform::from_translation(Vec3::Y * 0.1)),
            start_sequence: 3,
            progress: 0.4,
            duration_seconds: 0.32,
            advance_authorized: false,
            trajectory_signature: GuardPelvisTrajectorySignature::default(),
        };
        assert!(!guard_pelvis_blocks_stationary_pivot(Some(acquisition)));
        acquisition.advance_authorized = true;
        assert!(guard_pelvis_blocks_stationary_pivot(Some(acquisition)));
        acquisition.progress = 1.0;
        assert!(!guard_pelvis_blocks_stationary_pivot(Some(acquisition)));

        let hip = Vec3::Y * 0.8;
        let reach = FootReachEnvelope::new(hip, hip, 0.92, 0.94).unwrap();
        let comfort = stationary_guard_comfort_endpoint(Vec3::X * 2.0, Some(reach), None);
        assert!(comfort.distance(reach.current_root()) <= reach.warning_reach() * 0.94 + 0.0001);
    }

    #[test]
    fn guard_pelvis_turn_prediction_replays_the_bounded_facing_command() {
        let current = Quat::from_rotation_y(-0.2);
        let target = Quat::from_rotation_y(1.1);
        let signature = GuardPelvisTrajectorySignature {
            body_rotation: current,
            body_target_rotation: target,
            ..default()
        };
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let predicted = guard_predicted_body_rotation(signature, dt);
        let expected = advance_body_facing(
            current,
            Quat::from_rotation_y(1.1 + std::f32::consts::PI),
            Vec3::ZERO,
            SkeletonAction::None,
            WeaponGuardState::Raised,
            dt,
        );
        assert!(predicted.angle_between(expected) <= 0.000001);
        assert!(guard_predicted_body_rotation(signature, 1.0).angle_between(target) <= 0.000001);
    }

    #[test]
    fn continuous_aim_correction_does_not_abort_a_live_reach_checked_guard_step() {
        let admitted = GuardPelvisTrajectorySignature {
            command_velocity: Vec3::NEG_Z,
            sequence: 7,
            body_rotation: Quat::IDENTITY,
            body_target_rotation: Quat::from_rotation_y(0.1),
            ..default()
        };
        let corrected_aim = GuardPelvisTrajectorySignature {
            body_target_rotation: Quat::from_rotation_y(0.35),
            ..admitted
        };
        assert!(!guard_controller_command_changed(
            admitted,
            corrected_aim,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));

        let changed_cadence = GuardPelvisTrajectorySignature {
            sequence: 8,
            ..corrected_aim
        };
        assert!(guard_controller_command_changed(
            admitted,
            changed_cadence,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
        let reversed = GuardPelvisTrajectorySignature {
            command_velocity: Vec3::Z,
            ..admitted
        };
        assert!(guard_controller_command_changed(
            admitted,
            reversed,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
    }

    #[test]
    fn raised_pelvis_recovery_is_monotone_and_never_overshoots_authored_height() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut state = PelvisFollowerState {
            position: -0.25,
            velocity: 0.0,
            acceleration: 0.0,
        };
        let mut recovery = None;
        for _ in 0..256 {
            let previous = state;
            state = advance_pelvis_follower_with_recovery(state, &mut recovery, 0.0, dt);
            assert!(
                state.position >= previous.position - 0.000001,
                "previous={previous:?} state={state:?} recovery={recovery:?}"
            );
            assert!(state.position <= 0.000001);
        }
        assert_eq!(state, PelvisFollowerState::default());
        assert!(recovery.is_none());
    }

    #[test]
    fn raised_pelvis_recovery_brakes_adverse_motion_without_overshooting() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut state = PelvisFollowerState {
            position: -0.18,
            velocity: -0.12,
            acceleration: -1.5,
        };
        let mut recovery = None;
        let mut moving_toward_authored = false;
        for _ in 0..512 {
            let previous = state;
            state = advance_pelvis_follower_with_recovery(state, &mut recovery, 0.0, dt);
            assert!(state.position <= 0.000001);
            assert!(state.acceleration.abs() <= PELVIS_FOLLOWER_MAXIMUM_ACCELERATION + 0.0001);
            assert!(
                ((state.acceleration - previous.acceleration) / dt).abs()
                    <= PELVIS_FOLLOWER_MAXIMUM_JERK + 0.001
            );
            if moving_toward_authored {
                assert!(state.position >= previous.position - 0.000001);
            }
            moving_toward_authored |= state.velocity >= -0.000001;
        }
        assert_eq!(state, PelvisFollowerState::default());
        assert!(recovery.is_none());
    }

    #[test]
    fn raised_pelvis_recovery_replans_when_support_reverses_the_target() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut state = PelvisFollowerState {
            position: -0.08,
            velocity: 0.0,
            acceleration: 0.0,
        };
        let mut recovery = None;
        for _ in 0..8 {
            state = advance_pelvis_follower_with_recovery(state, &mut recovery, 0.0, dt);
        }
        assert!(state.velocity > 0.0);
        let previous = state;
        let mut lowered = advance_pelvis_follower_with_recovery(state, &mut recovery, -0.12, dt);
        assert!(lowered.position.is_finite());
        assert!(lowered.acceleration.abs() <= PELVIS_FOLLOWER_MAXIMUM_ACCELERATION + 0.0001);
        assert!(
            ((lowered.acceleration - previous.acceleration) / dt).abs()
                <= PELVIS_FOLLOWER_MAXIMUM_JERK + 0.001
        );
        for _ in 0..128 {
            lowered = advance_pelvis_follower_with_recovery(lowered, &mut recovery, -0.12, dt);
        }
        assert!(lowered.position < previous.position);
    }

    #[test]
    fn raised_pelvis_lowering_respects_the_admitted_position_envelope() {
        let dt = 1.0 / CONTINUITY_SAMPLE_HZ;
        let mut state = PelvisFollowerState::default();
        let mut recovery = None;
        for tick in 0..384 {
            let desired = if tick < 96 {
                -(tick as f32 / 95.0) * 0.25
            } else {
                0.0
            };
            let previous = state;
            state = advance_pelvis_follower_with_recovery(state, &mut recovery, desired, dt);
            assert!(
                state.position >= -0.250001 && state.position <= 0.000001,
                "tick={tick} desired={desired} state={state:?} recovery={recovery:?}"
            );
            assert!(state.acceleration.abs() <= PELVIS_FOLLOWER_MAXIMUM_ACCELERATION + 0.0001);
            assert!(
                ((state.acceleration - previous.acceleration) / dt).abs()
                    <= PELVIS_FOLLOWER_MAXIMUM_JERK + 0.001
            );
        }
        assert_eq!(state, PelvisFollowerState::default());
        assert!(recovery.is_none());
    }

    #[test]
    fn stale_non_support_guard_target_requires_a_persistent_release_before_deadline() {
        let presented = Vec3::new(0.0, 0.0, 0.0);
        let body_relative_target = Vec3::new(0.12, 0.0, 0.0);
        let reach = FootReachEnvelope::new(
            Vec3::new(0.0, 0.9, 0.0),
            Vec3::new(0.01, 0.9, 0.0),
            0.92,
            0.94,
        );
        assert!(non_support_guard_target_requires_release(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            body_relative_target,
            reach,
            0.32,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));

        let body_relative_target = Vec3::new(0.01, 0.0, 0.0);
        assert!(!non_support_guard_target_requires_release(
            presented,
            Vec3::ZERO,
            Vec3::ZERO,
            body_relative_target,
            reach,
            0.32,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
    }

    #[test]
    fn terminal_prepared_contacts_own_both_solves_despite_zero_idle_cadence() {
        let left = Vec3::new(-0.12, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.2);
        let right = Vec3::new(0.12, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.5);
        for plant in [left, right] {
            let (logical_weight, solve_plant) =
                terminal_contact_solve_ownership(true, 0.0, Some(plant));
            assert_eq!(logical_weight, 1.0);
            assert_eq!(solve_plant, Some(plant));

            let restored_idle_fk = plant + Vec3::new(0.0, 0.12, 0.4);
            assert!(!ordinary_plant_requires_clear(
                logical_weight,
                true,
                solve_plant,
                restored_idle_fk,
            ));
            let (_, next_tick_plant) = terminal_contact_solve_ownership(true, 0.0, solve_plant);
            assert_eq!(next_tick_plant, Some(plant));
            assert_eq!(next_tick_plant.unwrap().distance(plant), 0.0);
        }

        assert_eq!(
            terminal_contact_solve_ownership(false, 0.0, Some(left)),
            (0.0, Some(left))
        );
    }

    #[test]
    fn terminal_contact_preparation_prefers_last_rendered_stance_over_stale_solve() {
        let stale_left = Vec3::new(-0.12, 0.4, -1.245);
        let stale_right = Vec3::new(0.12, 0.4, -0.900);
        let visible_left = Vec3::new(-0.116, 0.3, -1.342);
        let visible_right = Vec3::new(0.118, 0.3, -0.784);
        let mut memory = LegIkMemory {
            settle: Some(LocomotionSettleState {
                support_left: true,
                swing_start: visible_right,
                capture_point: Vec3::ZERO,
                landing_target: stale_right,
                progress: 1.0,
                elapsed_seconds: 1.0,
                raised_handoff: false,
                stateful_follower: false,
            }),
            left_foot_world_target: Some(stale_left),
            right_foot_world_target: Some(stale_right),
            left_last_rendered_world: Some(visible_left),
            right_last_rendered_world: Some(visible_right),
            ..default()
        };

        assert!(prepare_terminal_settle_contacts(
            &mut memory,
            Vec3::ZERO,
            Quat::IDENTITY,
            |_| Some(0.0),
        ));
        let left = memory.left_foot_world_target.unwrap();
        let right = memory.right_foot_world_target.unwrap();
        assert_eq!(left.xz(), visible_left.xz());
        assert_eq!(right.xz(), visible_right.xz());
        assert_eq!(left.y, MEASURED_ANKLE_SOLE_OFFSET_METRES);
        assert_eq!(right.y, MEASURED_ANKLE_SOLE_OFFSET_METRES);
        assert_eq!(memory.left_foot_plant, Some(left));
        assert_eq!(memory.right_foot_plant, Some(right));

        memory.left_last_rendered_world = Some(visible_left + Vec3::Z * 0.2);
        memory.right_last_rendered_world = Some(visible_right - Vec3::Z * 0.2);
        assert!(prepare_terminal_settle_contacts(
            &mut memory,
            Vec3::ZERO,
            Quat::IDENTITY,
            |_| Some(0.0),
        ));
        assert_eq!(memory.left_foot_world_target, Some(left));
        assert_eq!(memory.right_foot_world_target, Some(right));
    }

    #[test]
    fn finished_terminal_reach_persists_through_held_idle() {
        let left = Vec3::new(-0.12, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.2);
        let right = Vec3::new(0.12, MEASURED_ANKLE_SOLE_OFFSET_METRES, -0.5);
        let mut memory = LegIkMemory {
            settle: Some(LocomotionSettleState {
                support_left: true,
                swing_start: right,
                capture_point: Vec3::ZERO,
                landing_target: right,
                progress: 1.0,
                elapsed_seconds: 1.0,
                raised_handoff: false,
                stateful_follower: false,
            }),
            left_foot_world_target: Some(left),
            right_foot_world_target: Some(right),
            left_foot_plant: Some(left),
            right_foot_plant: Some(right),
            terminal_contacts_prepared: true,
            terminal_reach_shift: -0.08,
            terminal_reach_target_shift: Some(-0.08),
            ..default()
        };

        finish_settle_for_idle(&mut memory);
        assert_eq!(memory.pelvis_shift, -0.08);
        for _ in 0..20 {
            memory.pelvis_shift =
                advance_pelvis_shift(memory.pelvis_shift, -0.08, 1.0 / CONTINUITY_SAMPLE_HZ);
            assert_eq!(memory.pelvis_shift, -0.08);
            assert!(memory.settle.is_none());
            assert_eq!(memory.left_foot_plant, Some(left));
            assert_eq!(memory.right_foot_plant, Some(right));
            assert_eq!(memory.left_support_weight, Some(1.0));
            assert_eq!(memory.right_support_weight, Some(1.0));
        }
    }

    #[test]
    fn stop_settle_seeds_from_visible_reach_limited_feet() {
        let invisible_goal = Vec3::new(-0.178, 1.934, 0.0);
        let prior_rendered = Vec3::new(-0.178, 1.934, -0.253);
        let restored_idle_fk = Vec3::new(-0.178, 1.934, -1.255);
        let landing = Vec3::new(-0.099, 2.085, -0.871);
        let mut memory = LegIkMemory {
            left_foot_world_target: Some(invisible_goal),
            left_foot_target: Some(invisible_goal),
            left_last_rendered_world: Some(prior_rendered),
            left_release_active: true,
            ..default()
        };

        let visible = settle_visible_foot(memory.left_last_rendered_world, Some(restored_idle_fk));

        seed_settle_from_rendered_feet(&mut memory, visible, None, Vec3::ZERO, Quat::IDENTITY);
        assert_eq!(visible, Some(prior_rendered));
        assert_eq!(memory.left_foot_world_target, Some(prior_rendered));
        let next = advance_foot_target_at_speed(
            memory.left_foot_world_target,
            landing,
            1.0 / CONTINUITY_SAMPLE_HZ,
            AIRBORNE_RELEASE_TARGET_SPEED,
        );
        assert!(
            next.distance(prior_rendered)
                <= AIRBORNE_RELEASE_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );
        assert!(next.distance_squared(landing) > 0.000001);
    }

    #[test]
    fn stop_settle_retains_the_selected_rendered_support() {
        let left = Vec3::new(-0.1, 2.085, -0.262);
        let right = Vec3::new(0.1, 2.085, -0.643);
        let stale_plan = Vec3::new(-0.1, 2.085, -1.829);
        let mut memory = LegIkMemory {
            left_planned_contact: Some(stale_plan),
            right_planned_contact: Some(stale_plan),
            ..default()
        };

        seed_settle_from_rendered_feet(
            &mut memory,
            Some(left),
            Some(right),
            Vec3::ZERO,
            Quat::IDENTITY,
        );
        retain_settle_support(&mut memory, false, Some(left), Some(right), true);

        assert_eq!(memory.right_foot_plant, Some(right));
        assert!(memory.right_foot_plant_acquired);
        assert_eq!(memory.right_transition_support_weight, Some(1.0));
        assert!(memory.left_planned_contact.is_none());
        assert!(memory.right_planned_contact.is_none());
    }

    #[test]
    fn stop_settle_reseeds_stale_follower_from_the_rendered_ankle() {
        let rendered = Vec3::new(2.2413998, 1.9726762, -10.548973);
        let stale = Vec3::new(1.12, 2.40, -10.49);
        let mut memory = LegIkMemory {
            right_foot_follower: FootFollowerState::from_presented_pose(
                stale,
                Vec3::splat(8.0),
                Vec3::splat(16.0),
                stale,
                Vec3::splat(8.0),
                Vec3::splat(16.0),
            ),
            ..default()
        };

        seed_settle_from_rendered_feet(
            &mut memory,
            None,
            Some(rendered),
            Vec3::ZERO,
            Quat::IDENTITY,
        );

        let follower = memory.right_foot_follower.unwrap();
        assert_eq!(follower.position, rendered);
        assert_eq!(follower.velocity, Vec3::ZERO);
        assert_eq!(follower.acceleration, Vec3::ZERO);
        assert_eq!(follower.previous_ideal, rendered);
        assert!(rendered.distance(stale) > 1.0);
    }

    #[test]
    fn stop_settle_visible_airborne_support_remains_unacquired() {
        let airborne_right = Vec3::new(0.1, 2.16, -0.64);
        let mut memory = LegIkMemory {
            right_support_weight: Some(0.0),
            right_foot_plant_acquired: false,
            ..default()
        };

        retain_settle_support(&mut memory, false, None, Some(airborne_right), false);

        assert_eq!(memory.right_foot_plant, Some(airborne_right));
        assert!(!memory.right_foot_plant_acquired);
        assert_eq!(memory.right_transition_support_weight, Some(1.0));
    }

    #[test]
    fn stop_settle_uses_current_fk_only_without_a_rendered_snapshot() {
        let restored_idle_fk = Vec3::new(0.1, 2.085, -0.422);
        assert_eq!(
            settle_visible_foot(None, Some(restored_idle_fk)),
            Some(restored_idle_fk)
        );
    }

    #[test]
    fn truthful_reported_support_does_not_erase_solver_ownership() {
        let mut memory = LegIkMemory {
            left_support_weight: Some(1.0),
            left_transition_support_weight: Some(1.0),
            ..default()
        };
        memory.left_support_weight = Some(0.0);
        assert_eq!(memory.left_support_weight, Some(0.0));
        assert_eq!(memory.left_transition_support_weight, Some(1.0));
    }

    #[test]
    fn repeated_fixed_tick_leaves_advanced_ik_memory_identical() {
        let mut memory = LegIkMemory {
            left_foot_plant: Some(Vec3::new(-0.1, 0.085, -2.0)),
            left_foot_plant_acquired: true,
            left_foot_world_target: Some(Vec3::new(-0.1, 0.085, -2.0)),
            left_support_weight: Some(0.4),
            left_transition_support_weight: Some(0.4),
            left_release_active: false,
            evaluation_tick: Some(91),
            ..default()
        };
        let advanced = memory;
        if !repeated_fixed_tick_skips_ik(true, false) {
            memory.left_foot_plant = None;
            memory.left_support_weight = Some(0.0);
            memory.left_transition_support_weight = Some(0.0);
            memory.left_release_active = true;
        }
        assert_eq!(memory, advanced);
        assert!(!repeated_fixed_tick_skips_ik(true, true));
        assert!(!repeated_fixed_tick_skips_ik(false, false));
    }

    #[test]
    fn post_anatomical_chain_marks_the_fixed_tick_as_presentable() {
        let memory = LegIkMemory {
            evaluation_tick: Some(90),
            knee_yaw_evaluation_tick: Some(91),
            left_rotation_chain: Some(LegRotationChain {
                upper: Quat::from_rotation_z(0.4),
                lower: Quat::from_rotation_x(-0.7),
                foot: Quat::from_rotation_y(0.2),
            }),
            ..default()
        };
        assert!(leg_ik_was_evaluated_at(memory, 91));
        assert!(!leg_ik_was_evaluated_at(memory, 92));
    }

    #[test]
    fn acquired_plant_survives_authored_fk_divergence_until_support_exit() {
        let plant = Vec3::new(-0.1, 0.1, -2.0);
        let divergent_authored_swing = Vec3::new(-0.1, 0.6, 0.5);
        assert!(!ordinary_plant_requires_clear(
            0.2,
            true,
            Some(plant),
            divergent_authored_swing,
        ));
        assert!(ordinary_plant_requires_clear(
            0.0,
            true,
            Some(plant),
            divergent_authored_swing,
        ));
        assert!(ordinary_plant_requires_clear(
            0.2,
            false,
            Some(plant),
            divergent_authored_swing,
        ));
    }

    #[test]
    fn acquired_support_waits_for_replacement_contact_not_phase_exit() {
        let plant = Vec3::new(-0.1, 0.085, -2.0);
        let authored_swing = Vec3::new(-0.1, 0.5, -1.0);

        let retained = coordinated_support_weight(LocomotionGait::Walk, 0.0, true, false);
        assert_eq!(retained, 1.0);
        assert!(!ordinary_plant_requires_clear(
            retained,
            true,
            Some(plant),
            authored_swing,
        ));

        let handed_off = coordinated_support_weight(LocomotionGait::Walk, 0.0, true, true);
        assert_eq!(handed_off, 0.0);
        assert!(ordinary_plant_requires_clear(
            handed_off,
            true,
            Some(plant),
            authored_swing,
        ));

        // Explicit reach failure clears the plant before coordination, so the
        // phase-independent owner cannot retain an unreachable footprint.
        let reach_released = coordinated_support_weight(LocomotionGait::Walk, 0.0, false, false);
        assert_eq!(reach_released, 0.0);
        assert!(ordinary_plant_requires_clear(
            reach_released,
            true,
            None,
            authored_swing,
        ));

        let run_flight = coordinated_support_weight(LocomotionGait::Run, 0.0, true, false);
        assert_eq!(run_flight, 0.0);
        assert!(ordinary_plant_requires_clear(
            run_flight,
            true,
            Some(plant),
            authored_swing,
        ));

        assert_eq!(
            run_toe_off_support_weight(LocomotionGait::Run, 0.773, true, false),
            (false, 0.773)
        );
        assert_eq!(
            run_toe_off_support_weight(LocomotionGait::Run, 0.0, true, true),
            (true, 0.0)
        );
        assert_eq!(
            run_retained_support_through_lobe_edge(LocomotionGait::Run, 0.0, true, false,),
            1.0
        );
        assert_eq!(
            run_retained_support_through_lobe_edge(LocomotionGait::Run, 0.26, true, false,),
            1.0
        );
        assert_eq!(
            run_retained_support_through_lobe_edge(LocomotionGait::Run, 0.0, true, true,),
            0.0
        );
        assert!(run_swing_clearance(0.82, Some(0.0)) >= 0.05);
        let phase_step =
            gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, 5.5, 1.0 / CONTINUITY_SAMPLE_HZ);
        let samples_to_opposite_acquisition = ((0.891_f32 - 0.698) / phase_step).ceil();
        let unsupported_seconds =
            (samples_to_opposite_acquisition - 1.0).max(0.0) / CONTINUITY_SAMPLE_HZ;
        assert!(unsupported_seconds <= 0.12);
        assert_eq!(
            run_toe_off_support_weight(LocomotionGait::Run, 0.260, false, true),
            (false, 0.260)
        );
        assert_eq!(
            run_toe_off_support_weight(LocomotionGait::Walk, 0.260, true, true),
            (false, 0.260)
        );
        for rising_phase in [0.853, 0.877, 0.901, 0.926] {
            assert!(!run_is_at_support_exit(
                rising_phase,
                true,
                RUN_LOCOMOTION_PROFILE.support_phase_radius,
            ));
            assert_eq!(
                run_toe_off_support_weight(LocomotionGait::Run, 0.21, true, false),
                (false, 0.21)
            );
        }
        for retained_phase in [0.602, 0.626, 0.650] {
            assert!(!run_is_at_support_exit(
                retained_phase,
                false,
                RUN_LOCOMOTION_PROFILE.support_phase_radius,
            ));
        }
        assert!(!run_is_at_support_exit(
            0.674,
            false,
            RUN_LOCOMOTION_PROFILE.support_phase_radius,
        ));
        assert!(run_is_at_support_exit(
            0.698,
            false,
            RUN_LOCOMOTION_PROFILE.support_phase_radius,
        ));
        assert!(run_release_edge(false, true));
        assert!(run_release_edge(true, false));
        assert!(!run_release_edge(false, false));
        assert_eq!(
            run_toe_off_support_weight(LocomotionGait::Run, 0.0, true, true),
            (true, 0.0)
        );
        let (still_exhausted, suppressed_reentry) = support_after_exhausted_lobe(true, 0.2);
        assert!(still_exhausted);
        assert_eq!(suppressed_reentry, 0.0);
        let (cleared_in_flight, flight_weight) = support_after_exhausted_lobe(true, 0.0);
        assert!(!cleared_in_flight);
        assert_eq!(flight_weight, 0.0);
    }

    #[test]
    fn cold_start_clearance_solve_reports_procedural_release_ownership() {
        let authored = Vec3::new(-0.09, 1.90, -0.20);
        let terrain_cleared = authored + Vec3::Y * 0.095;
        assert!(unplanned_terrain_solve_requires_release(
            None,
            terrain_cleared,
            authored,
        ));
        assert!(!unplanned_terrain_solve_requires_release(
            Some(terrain_cleared),
            terrain_cleared,
            authored,
        ));
        assert!(!unplanned_terrain_solve_requires_release(
            None,
            authored + Vec3::Y * 0.02,
            authored,
        ));
    }

    #[test]
    fn frozen_plan_survives_support_entry_until_actual_acquisition() {
        let plan = Some(Vec3::new(0.1, 2.062, -5.548));
        assert!(!acquired_plan_can_clear(false));
        assert!(!acquisition_lobe_exited_without_contact(
            plan,
            false,
            Some(0.2),
            0.8,
        ));
        assert!(acquired_plan_can_clear(true));
        assert!(!acquisition_lobe_exited_without_contact(
            plan,
            true,
            Some(0.2),
            0.0,
        ));
        assert!(acquisition_lobe_exited_without_contact(
            plan,
            false,
            Some(0.2),
            0.0,
        ));
    }

    #[test]
    fn expired_late_plan_replaces_all_frozen_swing_metadata() {
        let mut contact = Some(Vec3::new(0.1, 2.06, -0.607));
        let mut start = Some(Vec3::new(0.1, 2.1, -0.268));
        let mut phase_start = Some(0.418);
        clear_planned_contact_metadata(&mut contact, &mut start, &mut phase_start);
        assert!(contact.is_none() && start.is_none() && phase_start.is_none());

        let visible = Vec3::new(0.1, 2.12, -2.3);
        let replacement = Vec3::new(0.1, 2.06, -5.548);
        // The .18 readiness boundary gives this metadata-only full-cycle
        // fixture a matching .866 start, preserving its approach span while
        // isolating frozen-state replacement from cadence tuning.
        let replacement_phase = 0.866;
        contact = Some(replacement);
        start = contact.map(|_| planned_contact_start(start, Some(visible), visible));
        phase_start = contact.map(|_| phase_start.unwrap_or(replacement_phase));
        assert_eq!(start, Some(visible));
        assert_eq!(phase_start, Some(replacement_phase));

        let ready = RUN_LOCOMOTION_PROFILE.support_phase_radius + RUN_CONTACT_CHAIN_SETTLE_PHASE;
        let first = run_contact_approach_progress(replacement_phase, phase_start.unwrap(), ready);
        let phase_step =
            gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, 5.5, 1.0 / CONTINUITY_SAMPLE_HZ);
        let second = run_contact_approach_progress(
            replacement_phase - phase_step,
            phase_start.unwrap(),
            ready,
        );
        assert_eq!(visible.lerp(replacement, first), visible);
        let world_step = visible.lerp(replacement, second).distance(visible);
        let root_step = 5.5 / CONTINUITY_SAMPLE_HZ;
        assert!(world_step - root_step <= MAX_RUN_SWING_ROOT_RELATIVE_STEP_METRES + 0.0001);
    }

    #[test]
    fn full_cycle_run_plan_has_no_progress_velocity_seam() {
        let start = Vec3::new(0.1163, 2.1378, -5.5478);
        let endpoint = Vec3::new(0.1199, 2.1157, -9.2572);
        let mut phase_to_contact = 0.856;
        let phase_start = phase_to_contact;
        let ready = RUN_LOCOMOTION_PROFILE.support_phase_radius + RUN_CONTACT_CHAIN_SETTLE_PHASE;
        let phase_step =
            gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, 5.5, 1.0 / CONTINUITY_SAMPLE_HZ);
        let root_step = 5.5 / CONTINUITY_SAMPLE_HZ;
        let mut previous = start;
        while phase_to_contact > ready {
            phase_to_contact = (phase_to_contact - phase_step).max(ready);
            let progress = run_contact_approach_progress(phase_to_contact, phase_start, ready);
            let target = start.lerp(endpoint, progress);
            let root_relative_step = (target.distance(previous) - root_step).max(0.0);
            assert!(root_relative_step <= 0.095);
            previous = target;
        }
        assert!(previous.distance(endpoint) < 0.0001);
    }

    #[test]
    fn run_toe_off_plan_survives_same_lobe_tail_and_next_ticks() {
        let start = Vec3::new(-0.1208, 1.9523, -7.4717);
        let endpoint = Vec3::new(-0.1210, 2.3074, -11.0308);
        let phase_start = 0.8674;
        let ready = RUN_LOCOMOTION_PROFILE.support_phase_radius + RUN_CONTACT_CHAIN_SETTLE_PHASE;
        let phase_step =
            gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, 5.5, 1.0 / CONTINUITY_SAMPLE_HZ);
        assert_eq!(
            run_toe_off_support_weight(LocomotionGait::Run, 0.773, true, false),
            (false, 0.773)
        );
        let (toe_off, first_weight) =
            run_toe_off_support_weight(LocomotionGait::Run, 0.0, true, true);
        assert!(toe_off);
        assert_eq!(first_weight, 0.0);
        assert!(run_swing_clearance(0.86, Some(0.0)) >= 0.05);

        let frozen = (Some(endpoint), Some(start), Some(phase_start));
        let mut exhausted = toe_off;
        let mut previous = start;
        for (index, raw_support) in [0.0, 0.0, 0.0].into_iter().enumerate() {
            let (next_exhausted, effective) = support_after_exhausted_lobe(exhausted, raw_support);
            exhausted = next_exhausted;
            assert_eq!(effective, 0.0);
            assert_eq!(frozen, (Some(endpoint), Some(start), Some(phase_start)));
            let phase = phase_start - phase_step * (index as f32 + 1.0);
            let progress = run_contact_approach_progress(phase, phase_start, ready);
            let target = start.lerp(endpoint, progress);
            let root_relative = (target.distance(previous) - 5.5 / CONTINUITY_SAMPLE_HZ).max(0.0);
            assert!(root_relative <= 0.095);
            previous = target;
        }
    }

    #[test]
    fn raw_run_cycle_clears_toe_off_latch_and_reacquires_rising_plan() {
        let profile = RUN_LOCOMOTION_PROFILE;
        let radius = profile.support_phase_radius;
        let endpoint = Vec3::new(0.1, MEASURED_ANKLE_SOLE_OFFSET_METRES, -9.256);

        // The acquired right foot owns the post-contact shoulder until its
        // signed support exit, where toe-off exhausts only this lobe.
        let exit_phase = 0.698;
        let (_, exit_raw) = gait_support_weights(profile, exit_phase);
        assert!(run_is_at_support_exit(exit_phase, false, radius));
        let (mut exhausted, effective) =
            run_toe_off_support_weight(LocomotionGait::Run, exit_raw, true, true);
        assert!(exhausted);
        assert_eq!(effective, 0.0);

        // The raw cadence, not the support value suppressed by the latch,
        // proves that this foot crossed flight and begins a fresh cycle.
        let flight_phase = 0.75;
        let (_, flight_raw) = gait_support_weights(profile, flight_phase);
        assert!(!terrain_leg_has_support(flight_raw));
        exhausted = exhausted_latch_after_raw_cadence(exhausted, flight_raw);
        assert!(!exhausted);

        // At the next rising shoulder the frozen endpoint has caught up in XZ
        // and sits on the semantic 5 cm flight floor. Unsuppressed raw support
        // makes the final bounded descent eligible, so contact can be acquired
        // by phase .35-.40 instead of remaining pinned above terrain.
        let rising_phase = 0.36;
        let (_, rising_raw) = gait_support_weights(profile, rising_phase);
        assert!(terrain_leg_has_support(rising_raw));
        assert!(run_plan_is_on_rising_support(
            LocomotionGait::Run,
            rising_phase,
            false,
            radius,
            rising_raw,
            Some(endpoint),
            false,
        ));
        let carried = exhausted_latch_after_raw_cadence(exhausted, rising_raw);
        let (mut next_exhausted, mut effective_support) =
            support_after_exhausted_lobe(carried, rising_raw);
        if run_plan_is_on_rising_support(
            LocomotionGait::Run,
            rising_phase,
            false,
            radius,
            rising_raw,
            Some(endpoint),
            false,
        ) {
            next_exhausted = false;
            effective_support = rising_raw;
        }
        assert!(!next_exhausted);
        assert!(terrain_leg_has_support(effective_support));

        let prior_floor = endpoint + Vec3::Y * RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES;
        let reachable = run_contact_within_follower_step(
            Some(prior_floor),
            endpoint,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
        );
        assert!(reachable);
        let eligible = run_support_eligible_for_descent(
            LocomotionGait::Run,
            rising_phase,
            false,
            radius,
            rising_raw,
            reachable,
        );
        assert!(eligible);
        assert!(
            run_airborne_clearance(
                phase_to_next_contact(rising_phase, false),
                Some(1.0),
                eligible,
            ) <= f32::EPSILON
        );
        let lowered_y = run_clearance_target_height(prior_floor.y, endpoint.y, eligible);
        assert!(lowered_y < prior_floor.y);
        assert!((lowered_y - endpoint.y).abs() <= f32::EPSILON);
        let descended = advance_run_airborne_world_target(
            Some(prior_floor),
            Vec3::new(endpoint.x, lowered_y, endpoint.z),
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            |_| Some(endpoint.y),
        );
        assert!(descended.y < prior_floor.y);
        assert!(descended.distance(endpoint) <= 0.0001);
        assert_eq!(
            run_clearance_target_height(endpoint.y, prior_floor.y, false),
            prior_floor.y
        );
        let (_, post_contact_raw) = gait_support_weights(profile, 0.65);
        assert!(!run_support_eligible_for_descent(
            LocomotionGait::Run,
            0.65,
            false,
            radius,
            post_contact_raw,
            true,
        ));

        // Even if a low-rate consumer skipped the explicit flight sample, the
        // signed rising shoulder is an unambiguous new-lobe boundary.
        let (mut stale_latch, mut stale_support) = support_after_exhausted_lobe(true, rising_raw);
        assert!(stale_latch);
        if run_plan_is_on_rising_support(
            LocomotionGait::Run,
            rising_phase,
            false,
            radius,
            rising_raw,
            Some(endpoint),
            false,
        ) {
            stale_latch = false;
            stale_support = rising_raw;
        }
        assert!(!stale_latch);
        assert!(terrain_leg_has_support(stale_support));
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // Locks the authored run-release speed envelope.
    fn run_release_follows_root_once_and_lifts_only_clearance_floor() {
        let release_clearance = run_airborne_clearance_for_sample(true, 0.81, None, false);
        assert_eq!(release_clearance, RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES);
        assert!(run_airborne_clearance_for_sample(false, 0.81, None, false) > release_clearance);
        let previous_root = Vec3::new(0.0, 3.10, -4.2109);
        let next_root = previous_root + Vec3::NEG_Z * (5.5 / CONTINUITY_SAMPLE_HZ);
        let planted_world = Vec3::new(-0.12, 2.25, -3.668);
        let previous_owner = planted_world - previous_root;
        let owner = release_start_owner_target(
            LocomotionGait::Run,
            Some(previous_owner),
            Some(planted_world),
            next_root,
            Quat::IDENTITY,
            Vec3::ZERO,
        );
        let transported = next_root + owner;
        let lifted = transported + Vec3::Y * RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES;
        let root_delta = next_root - previous_root;
        let root_relative_step = (lifted - planted_world - root_delta).length();
        assert!(root_relative_step <= RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES + 0.0001);
        assert!(root_relative_step <= 0.095);
        assert!(root_relative_step <= MAX_KNEE_STEP_METRES);

        // Captured uphill release f49->50: neither full owner transport nor a
        // literal world hold can combine terrain rise and 5 cm clearance under
        // the 9 cm 3D owner budget. The joint projection selects an
        // intermediate XZ that satisfies both instead of violating continuity.
        let uphill_previous_root = Vec3::new(0.0, 3.103686, -4.2109375);
        let uphill_next_root = Vec3::new(0.0, 3.096167, -4.296875);
        let uphill_plant = Vec3::new(-0.11504457, 2.2510452, -3.7630615);
        let uphill_owner = uphill_plant - uphill_previous_root;
        let uphill_minimum_y = |xz: Vec2| {
            Some(
                uphill_plant.y
                    + RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
                    + (uphill_plant.z - xz.y).max(0.0) * 0.475,
            )
        };
        let uphill_release = advance_run_airborne_world_target(
            Some(uphill_owner),
            uphill_plant,
            uphill_next_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            run_airborne_owner_target_speed(true),
            uphill_minimum_y,
        );
        let uphill_release_owner = uphill_release - uphill_next_root;
        assert!(
            uphill_release_owner.distance(uphill_owner)
                <= RUN_FIRST_RELEASE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );
        assert!(uphill_release.y + 0.0001 >= uphill_minimum_y(uphill_release.xz()).unwrap());
        assert!(uphill_release.y - uphill_minimum_y(uphill_release.xz()).unwrap() <= 0.0001);
        assert!(uphill_release.z < uphill_plant.z);
        assert!(uphill_release.z > uphill_plant.z - 5.5 / CONTINUITY_SAMPLE_HZ);
        let captured_toe_offset = Vec3::new(-0.0108, 0.0007, -0.1370);
        let uphill_previous_toe = uphill_plant + captured_toe_offset;
        let uphill_release_toe = uphill_release + captured_toe_offset;
        let toe_root_relative_step =
            (uphill_release_toe - uphill_previous_toe - (uphill_next_root - uphill_previous_root))
                .length();
        assert!(toe_root_relative_step <= 0.095);
        assert!(run_airborne_owner_target_speed(true) / CONTINUITY_SAMPLE_HZ < 0.095);
        assert_eq!(
            run_airborne_owner_target_speed(false),
            RUN_AIRBORNE_OWNER_TARGET_SPEED
        );
        assert!(RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ > 5.5 / 64.0);
        assert!(RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ < 0.09);

        let previous_rotation = Quat::IDENTITY;
        let desired_rotation = Quat::from_rotation_x(30.0_f32.to_radians());
        let released_rotation = advance_airborne_foot_rotation(
            Some(previous_rotation),
            desired_rotation,
            1.0 / CONTINUITY_SAMPLE_HZ,
            FIRST_RUN_RELEASE_FOOT_ROTATION_SPEED_DEGREES,
        );
        assert!(
            previous_rotation
                .angle_between(released_rotation)
                .to_degrees()
                <= f32::EPSILON
        );

        // Walk/stop continue to hold a world plant on release.
        let walk_owner = release_start_owner_target(
            LocomotionGait::Walk,
            Some(previous_owner),
            Some(planted_world),
            next_root,
            Quat::IDENTITY,
            Vec3::ZERO,
        );
        assert!((next_root + walk_owner).distance(planted_world) < 0.0001);
    }

    #[test]
    fn unreachable_run_contact_keeps_flight_floor_until_chain_can_land() {
        let upper_root = Vec3::new(-0.10032953, 2.5767426, -6.794999);
        let contact = Vec3::new(-0.12013094, 1.902308, -7.4767027);
        let reach = terrain_maximum_reach(0.5230801, 0.42998108);
        assert!(!run_contact_within_leg_reach(contact, upper_root, reach));

        let flight_floor = contact + Vec3::Y * RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES;
        assert!(run_contact_within_leg_reach(
            flight_floor,
            upper_root,
            reach,
        ));
        assert_eq!(
            run_airborne_clearance_for_sample(false, 0.133, Some(1.0), false),
            RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
        );
    }

    #[test]
    fn captured_run_swing_step_keeps_target_inside_knee_budget_margin() {
        let previous_root = Vec3::new(0.0, 3.0811288, -4.46875);
        let next_root = Vec3::new(0.0, 3.0736096, -4.5546875);
        let previous_target = Vec3::new(-0.11504456, 2.3028326, -3.8614511);
        let desired_target = Vec3::new(-0.1152586, 2.310206, -4.0351343);
        let previous_owner = previous_target - previous_root;
        let advanced = advance_run_airborne_world_target(
            Some(previous_owner),
            desired_target,
            next_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            |_| Some(f32::NEG_INFINITY),
        );
        let target_step = (advanced - next_root).distance(previous_owner);
        assert!(target_step <= 0.0875 + 0.0001);
        assert!(target_step > 5.5 / CONTINUITY_SAMPLE_HZ);
        assert!(target_step < 0.089);
    }

    #[test]
    fn first_run_release_uses_last_propagated_foot_orientation() {
        let analytic = Quat::from_rotation_x(0.18);
        let propagated = Quat::from_rotation_x(-0.07);
        assert_eq!(
            previous_airborne_foot_orientation(Some(analytic), Some(propagated), true),
            Some(propagated)
        );
        assert_eq!(
            previous_airborne_foot_orientation(Some(analytic), Some(propagated), false),
            Some(analytic)
        );
        assert_eq!(
            advance_airborne_foot_rotation(
                previous_airborne_foot_orientation(Some(analytic), Some(propagated), true),
                Quat::IDENTITY,
                1.0 / CONTINUITY_SAMPLE_HZ,
                FIRST_RUN_RELEASE_FOOT_ROTATION_SPEED_DEGREES,
            ),
            propagated
        );
    }

    #[test]
    fn first_run_release_searches_off_chord_for_terrain_clearance() {
        let start = Vec3::ZERO;
        let desired = Vec3::new(0.0, 0.0, 0.08);
        let maximum_step = 0.094;
        let minimum_y = |xz: Vec2| {
            // The direct chord is a raised ridge; a lateral point within the
            // same motion sphere satisfies both clearance and continuity.
            Some(if xz.x.abs() < 0.02 { 0.12 } else { 0.02 })
        };
        let target = advance_run_airborne_world_target(
            Some(start),
            desired,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            maximum_step,
            minimum_y,
        );
        assert!(target.x.abs() >= 0.02);
        assert!(target.y + 0.0001 >= minimum_y(target.xz()).unwrap());
        assert!(target.distance(start) <= maximum_step + 0.0001);
    }

    #[test]
    fn airborne_run_limiter_bounds_combined_horizontal_and_clearance_motion() {
        let mut owner_target = Vec3::ZERO;
        let desired_samples = [
            Vec3::new(0.0, 0.05, -0.08),
            Vec3::new(0.0, 0.08, -0.17),
            Vec3::new(0.0, 0.10, -0.26),
            Vec3::new(0.0, 0.08, -0.35),
        ];
        for desired in desired_samples {
            let next = advance_foot_target_at_speed(
                Some(owner_target),
                desired,
                1.0 / CONTINUITY_SAMPLE_HZ,
                RUN_AIRBORNE_OWNER_TARGET_SPEED,
            );
            assert!(
                next.distance(owner_target)
                    <= RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
            );
            owner_target = next;
        }

        let endpoint = Vec3::new(0.0, 0.0, -0.45);
        for _ in 0..8 {
            owner_target = advance_foot_target_at_speed(
                Some(owner_target),
                endpoint,
                1.0 / CONTINUITY_SAMPLE_HZ,
                RUN_AIRBORNE_OWNER_TARGET_SPEED,
            );
        }
        assert!(owner_target.distance(endpoint) < 0.0001);
    }

    #[test]
    fn high_speed_unplanned_release_uses_run_budget_before_gait_style_catches_up() {
        let before_root = Vec3::new(0.0, 2.831712, -0.171875);
        let after_root = Vec3::new(0.0, 2.84709, -0.2578125);
        let before_solve = Vec3::new(-0.092886, 1.965967, -0.204507);
        let desired_solve = Vec3::new(-0.120672, 1.962317, -0.195115);
        let before_owner = before_solve - before_root;
        let desired_owner = desired_solve - after_root;
        assert!(before_owner.distance(desired_owner) > 0.095);
        let measured_speed = update_measured_owner_planar_speed(
            0.0,
            Some(before_root),
            after_root,
            1.0 / CONTINUITY_SAMPLE_HZ,
            true,
            false,
        );
        assert!((measured_speed - 5.5).abs() <= 0.0001);
        assert!(uses_run_airborne_motion_budget(
            LocomotionGait::Walk,
            0.5_f32.max(measured_speed),
        ));
        assert!(!uses_run_airborne_motion_budget(LocomotionGait::Walk, 2.0));
        assert_eq!(
            update_measured_owner_planar_speed(
                measured_speed,
                Some(after_root),
                after_root + Vec3::X,
                1.0 / CONTINUITY_SAMPLE_HZ,
                false,
                false,
            ),
            measured_speed,
        );
        assert_eq!(
            update_measured_owner_planar_speed(
                measured_speed,
                Some(after_root),
                after_root + Vec3::X,
                1.0 / CONTINUITY_SAMPLE_HZ,
                true,
                true,
            ),
            0.0,
        );

        let resolved = advance_run_airborne_world_target(
            Some(before_owner),
            desired_solve,
            after_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            |_| Some(-100.0),
        );
        let resolved_owner = resolved - after_root;
        assert!(
            resolved_owner.distance(before_owner)
                <= RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );
        assert!(resolved_owner.distance(before_owner) <= 0.095);

        let support_path = bound_unacquired_run_support_release_target(
            true,
            false,
            false,
            true,
            Some(before_owner),
            desired_solve,
            after_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            |_| Some(-100.0),
        );
        assert!(
            (support_path - after_root).distance(before_owner)
                <= RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );
        assert_eq!(
            bound_unacquired_run_support_release_target(
                true,
                false,
                true,
                true,
                Some(before_owner),
                desired_solve,
                after_root,
                Quat::IDENTITY,
                1.0 / CONTINUITY_SAMPLE_HZ,
                |_| Some(-100.0),
            ),
            desired_solve,
        );
        let bounded_owner = support_path - after_root;
        assert_eq!(
            support_release_diagnostic_goal(true, true, bounded_owner, desired_owner,),
            Some(bounded_owner),
        );
        assert_eq!(
            support_release_diagnostic_goal(true, false, bounded_owner, desired_owner,),
            Some(desired_owner),
        );
        assert_eq!(
            support_release_diagnostic_goal(false, true, bounded_owner, desired_owner,),
            None,
        );

        let steady_before_root = Vec3::new(0.0, 2.8317122, -0.171875);
        let steady_before_end = Vec3::new(0.21052803, 2.0040245, -0.00043848384);
        let steady_after_root = Vec3::new(0.0, 2.8470902, -0.2578125);
        let steady_after_end = Vec3::new(0.20848821, 2.103109, -0.11178008);
        let authored = Vec3::new(0.200671, 1.9489093, -0.12319517);
        let preliminary_target = authored + Vec3::X * 0.01;
        let planted_target = authored + Vec3::NEG_Z * 0.20;
        assert!(unplanned_support_release_is_owned(
            false,
            Some(0.0),
            Some(1.0),
            None,
            preliminary_target,
            planted_target,
            authored,
        ));
        assert!(!unplanned_support_release_is_owned(
            false,
            Some(0.0),
            Some(0.0),
            None,
            authored,
            authored,
            authored,
        ));
        assert!(unplanned_support_release_is_owned(
            false,
            Some(0.0),
            Some(1.0),
            None,
            authored,
            authored,
            authored,
        ));
        let (stored_world, stored_owner) = resolved_unacquired_support_release_ownership(
            true,
            steady_before_end,
            steady_before_root,
            Quat::IDENTITY,
        )
        .unwrap();
        assert_eq!(stored_world, steady_before_end);
        let mut memory = LegIkMemory {
            right_foot_world_target: Some(Vec3::new(9.0, 9.0, 9.0)),
            right_foot_target: Some(Vec3::new(8.0, 8.0, 8.0)),
            right_release_target: Some(Vec3::new(7.0, 7.0, 7.0)),
            right_release_active: true,
            rig_origin: Some(steady_before_root),
            rig_rotation: Some(Quat::IDENTITY),
            ..default()
        };
        assert!(airborne_unplanned_release_uses_resolved_end(
            true, None, true
        ));
        assert!(!airborne_unplanned_release_uses_resolved_end(
            true,
            Some(planted_target),
            true,
        ));
        commit_resolved_unplanned_airborne_release(
            &mut memory,
            false,
            true,
            None,
            true,
            steady_before_end,
            steady_before_root,
            Quat::IDENTITY,
        );
        assert_eq!(memory.right_foot_world_target, Some(stored_world));
        assert_eq!(memory.right_foot_target, Some(stored_owner));
        assert_eq!(memory.right_release_target, Some(stored_owner));
        let diagnostics = LegIkState(memory).diagnostics();
        let diagnostic_solve = diagnostics
            .right_solve_target
            .expect("the resolved support solve remains diagnostic state");
        let diagnostic_release = diagnostics
            .right_release_target
            .expect("the resolved support release remains diagnostic state");
        assert!(diagnostic_solve.is_finite());
        assert!(diagnostic_release.is_finite());
        assert!(diagnostic_solve.distance(steady_before_end) <= 0.000001);
        assert!(diagnostic_release.distance(steady_before_end) <= 0.000001);
        assert!(diagnostic_release.distance(diagnostic_solve) <= 0.000001);
        assert_eq!(
            run_previous_owner_target(LocomotionGait::Run, None, memory.right_foot_target,),
            Some(stored_owner),
        );
        let (_, next_owner) = resolved_unacquired_support_release_ownership(
            true,
            steady_after_end,
            steady_after_root,
            Quat::IDENTITY,
        )
        .unwrap();
        assert!(next_owner.distance(stored_owner) <= 0.095);
    }

    #[test]
    fn selected_settle_support_keeps_its_stateful_owner_after_speed_reaches_zero() {
        let settle = LocomotionSettleState {
            support_left: true,
            swing_start: Vec3::ZERO,
            capture_point: Vec3::NEG_Z,
            landing_target: Vec3::NEG_Z,
            progress: 0.5,
            elapsed_seconds: 0.1,
            raised_handoff: false,
            stateful_follower: true,
        };
        assert!(uses_stateful_support_follower(
            Some(settle),
            LocomotionGait::Walk,
            0.0,
        ));
        assert!(!uses_stateful_support_follower(
            Some(LocomotionSettleState {
                stateful_follower: false,
                ..settle
            }),
            LocomotionGait::Run,
            5.5,
        ));
    }

    #[test]
    fn uphill_airborne_projection_preserves_clearance_and_step_budget() {
        let previous_owner = Vec3::new(0.0, 0.15, 0.0);
        let desired = Vec3::new(0.0, 0.2, -0.3);
        let minimum_y = |xz: Vec2| Some(0.15 + (-xz.y).max(0.0) * 0.4);
        let resolved = advance_run_airborne_world_target(
            Some(previous_owner),
            desired,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            minimum_y,
        );
        assert!(resolved.is_finite());
        assert!(resolved.y + 0.000001 >= minimum_y(resolved.xz()).unwrap());
        assert!(
            resolved.distance(previous_owner)
                <= RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );
    }

    #[test]
    fn unacquired_run_support_entry_keeps_using_bounded_follower() {
        let previous_owner = Vec3::new(0.1, 0.15, -0.5);
        let frozen_plant = Vec3::new(0.1, 0.085, -0.8);
        let resolved = advance_run_airborne_world_target(
            Some(previous_owner),
            frozen_plant,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            |_| Some(0.085),
        );
        assert!(
            resolved.distance(previous_owner)
                <= RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );
        assert!(resolved.distance(frozen_plant) > 0.1);

        // A completed plan remains on the 5 cm semantic floor throughout raw
        // flight, then may descend to exact contact on the first eligible
        // support sample without bypassing the follower above.
        assert_eq!(
            run_airborne_clearance(0.34, Some(1.0), false),
            RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
        );
        assert_eq!(
            run_airborne_clearance(0.17, Some(1.0), false),
            RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
        );
        assert!(run_airborne_clearance(0.17, Some(1.0), true) <= f32::EPSILON);
    }

    #[test]
    fn run_follower_can_converge_on_fixed_world_contact_at_full_speed() {
        let previous_root = Vec3::new(0.0, 2.0, -4.0);
        let fixed_contact = Vec3::new(0.1, 0.085, -4.5);
        let previous_owner = fixed_contact - previous_root;
        let current_root = previous_root + Vec3::NEG_Z * (5.5 / CONTINUITY_SAMPLE_HZ);
        assert!(run_contact_within_follower_step(
            Some(previous_owner),
            fixed_contact,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
        let resolved = advance_run_airborne_world_target(
            Some(previous_owner),
            fixed_contact,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            |_| Some(0.085),
        );
        assert!(resolved.distance(fixed_contact) < 0.0001);

        let far_contact = fixed_contact + Vec3::NEG_Z * 0.3;
        assert!(!run_contact_within_follower_motion_step(
            Some(previous_owner),
            far_contact,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
        assert_eq!(
            run_airborne_clearance(0.17, Some(1.0), false),
            RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
        );
    }

    #[test]
    fn final_run_descent_transports_unacquired_footprint_then_freezes_it() {
        let previous_root = Vec3::new(0.0, 0.0, -4.0);
        let current_root = previous_root + Vec3::NEG_Z * (5.5 / CONTINUITY_SAMPLE_HZ);
        let fixed_contact = Vec3::new(0.1, MEASURED_ANKLE_SOLE_OFFSET_METRES, -4.5);
        let prior_floor = fixed_contact + Vec3::Y * RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES;
        let previous_owner = prior_floor - previous_root;

        // Root travel plus the contact descent is 9.94 cm, so the stationary
        // footprint cannot be reached inside the 9 cm target budget.
        assert!(!run_contact_within_follower_motion_step(
            Some(previous_owner),
            fixed_contact,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
        let transported = retarget_unacquired_run_contact_for_descent(
            Some(previous_owner),
            fixed_contact,
            current_root,
            Quat::IDENTITY,
            1.0,
            Vec3::new(0.1, 0.9, current_root.z - 0.5),
            1.0,
            1.0 / CONTINUITY_SAMPLE_HZ,
            |_| Some(0.0),
        )
        .expect("the owner-local footprint should remain reachable after its 5 cm descent");
        assert!((transported.z - (fixed_contact.z - 5.5 / CONTINUITY_SAMPLE_HZ)).abs() < 0.0001);
        assert_eq!(transported.y, MEASURED_ANKLE_SOLE_OFFSET_METRES);
        assert!(run_contact_within_follower_step(
            Some(previous_owner),
            transported,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
        let landed = advance_run_airborne_world_target(
            Some(previous_owner),
            transported,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            |_| Some(MEASURED_ANKLE_SOLE_OFFSET_METRES),
        );
        assert!(landed.distance(transported) < 0.0001);
        assert!(
            landed.distance(current_root + previous_owner)
                <= RUN_AIRBORNE_OWNER_TARGET_SPEED / CONTINUITY_SAMPLE_HZ + 0.0001
        );

        // Acquired support bypasses all airborne retargeting and retains the
        // resulting world footprint exactly on subsequent samples.
        let acquired_world_plant = transported;
        assert_eq!(acquired_world_plant, transported);
    }

    #[test]
    fn downhill_rising_contact_retargets_inside_current_leg_reach() {
        // Captured left landing at phase .867: the follower had reached its
        // frozen endpoint inside the motion budget, but the endpoint remained
        // about 1 cm beyond the current analytic leg reach. The rendered sole
        // consequently stayed 1.7 cm high until the following sample.
        let previous_root = Vec3::new(0.0, 2.7854202, -6.703125);
        let current_root = Vec3::new(0.0, 2.7790365, -6.7890625);
        let previous_ankle = Vec3::new(-0.11826715, 1.9728086, -7.4084473);
        let previous_owner = previous_ankle - previous_root;
        let upper_root = Vec3::new(-0.10032953, 2.5767426, -6.794999);
        let frozen_contact = Vec3::new(-0.12020548, 1.9023025, -7.475421);
        let solve_reach = maximum_reach(0.523, 0.430);
        assert!(run_contact_within_follower_motion_step(
            Some(previous_owner),
            frozen_contact,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
        assert!(frozen_contact.distance(upper_root) > solve_reach + 0.001);

        let terrain_height = frozen_contact.y - MEASURED_ANKLE_SOLE_OFFSET_METRES;
        let reachable_contact = retarget_unacquired_run_contact_for_descent(
            Some(previous_owner),
            frozen_contact,
            current_root,
            Quat::IDENTITY,
            -1.0,
            upper_root,
            solve_reach,
            1.0 / CONTINUITY_SAMPLE_HZ,
            |_| Some(terrain_height),
        )
        .expect("the final footprint should move just inside current downhill reach");
        assert!(reachable_contact.xz().distance(frozen_contact.xz()) > 0.001);
        assert!(reachable_contact.distance(upper_root) <= solve_reach + 0.001);
        assert!(run_contact_within_follower_motion_step(
            Some(previous_owner),
            reachable_contact,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
        ));
        assert_eq!(
            reachable_contact.y,
            terrain_height + MEASURED_ANKLE_SOLE_OFFSET_METRES
        );
        let landed = advance_run_airborne_world_target(
            Some(previous_owner),
            reachable_contact,
            current_root,
            Quat::IDENTITY,
            1.0 / CONTINUITY_SAMPLE_HZ,
            RUN_AIRBORNE_OWNER_TARGET_SPEED,
            |_| Some(reachable_contact.y),
        );
        assert!(landed.distance(reachable_contact) < 0.0001);
    }

    #[test]
    fn attack_uses_the_live_guard_support_weights() {
        let mut skeleton = SkeletonState::default()
            .with_weapon_guard(WeaponGuardState::Raised)
            .with_local_velocity(Vec3::NEG_Z * 2.0)
            .with_raised_locomotion(RaisedLocomotionIntent::moving(
                Vec2::NEG_Y,
                2.0,
                LeadFoot::Left,
                7,
            ));
        let guard_weights = locomotion_support_weights(&skeleton);
        skeleton
            .begin_attack(AttackSpec::new(AttackAnimation::Swing), 10, 20)
            .unwrap();
        assert_eq!(locomotion_support_weights(&skeleton), guard_weights);
    }

    #[test]
    fn diagnostic_raised_source_requires_exact_production_owner_and_tick() {
        let mut footwork = RaisedFootworkState {
            initialized: true,
            evaluation_tick: Some(41),
            raised_motion_owned_this_tick: true,
            ..default()
        };
        assert!(footwork.diagnostic_is_motion_owner(41));
        assert!(!footwork.diagnostic_is_motion_owner(40));
        footwork.raised_motion_owned_this_tick = false;
        assert!(!footwork.diagnostic_is_motion_owner(41));
    }
}
