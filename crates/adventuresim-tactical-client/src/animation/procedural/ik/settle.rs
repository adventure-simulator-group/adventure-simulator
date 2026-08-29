//! Stop/restart balance capture and terminal contact convergence.

use super::*;

// Returning the raised pelvis consumes about 2 cm of the knee's 10 cm frame
// budget. Reserve that motion only for the raised-to-settle handoff; ordinary
// swing and settle targets retain the faster general cap.
pub(super) const RAISED_SETTLE_PELVIS_KNEE_BUDGET_METRES: f32 = 0.02;
pub(super) const RAISED_SETTLE_TARGET_SPEED: f32 =
    (MAX_KNEE_STEP_METRES - RAISED_SETTLE_PELVIS_KNEE_BUDGET_METRES) * CONTINUITY_SAMPLE_HZ
        / MAX_KNEE_TARGET_AMPLIFICATION
        * 0.98;
pub(super) const SETTLE_STEP_SECONDS: f32 = 0.28;
pub(super) const SETTLE_CAPTURE_POINT_MARGIN_METRES: f32 = 0.12;
pub(super) const ASSUMED_COM_HEIGHT_METRES: f32 = 1.0;
pub(super) const MAX_SETTLE_CAPTURE_SPEED: f32 = 1.1;

pub(in crate::animation::procedural) fn preserve_raised_handoff_targets(
    memory: &mut LegIkMemory,
    raised: RaisedFootworkState,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    let retained = raised.step.retained_targets();
    let left = raised
        .left_solve_target
        .or_else(|| retained.map(|targets| targets.0));
    let right = raised
        .right_solve_target
        .or_else(|| retained.map(|targets| targets.1));
    let (Some(left), Some(right)) = (left, right) else {
        return;
    };
    memory.left_foot_world_target = Some(left);
    memory.right_foot_world_target = Some(right);
    memory.left_foot_target = Some(rig_rotation.inverse() * (left - rig_origin));
    memory.right_foot_target = Some(rig_rotation.inverse() * (right - rig_origin));
    memory.left_release_active = true;
    memory.right_release_active = true;
    memory.left_release_target = None;
    memory.right_release_target = None;
}

pub(in crate::animation::procedural) fn terrain_ik_is_required(
    enabled: bool,
    settle_active: bool,
    raised_handoff: bool,
) -> bool {
    enabled || settle_active || raised_handoff
}

pub(in crate::animation::procedural) fn advance_settle_state(
    mut settle: LocomotionSettleState,
    delta_seconds: f32,
) -> LocomotionSettleState {
    let delta_seconds = delta_seconds.max(0.0);
    settle.elapsed_seconds += delta_seconds;
    settle.progress = (settle.progress + delta_seconds / SETTLE_STEP_SECONDS).min(1.0);
    settle
}

pub(in crate::animation::procedural) fn settle_target_speed(settle: LocomotionSettleState) -> f32 {
    if settle.raised_handoff {
        RAISED_SETTLE_TARGET_SPEED
    } else {
        AIRBORNE_RELEASE_TARGET_SPEED
    }
}

pub(in crate::animation::procedural) fn cancel_settle_for_restart(
    memory: &mut LegIkMemory,
    planar_velocity: Vec3,
) {
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

pub(in crate::animation::procedural) fn reset_terminal_settle_reach(memory: &mut LegIkMemory) {
    memory.terminal_contacts_prepared = false;
    memory.terminal_root_base_translation = None;
    memory.terminal_reach_shift = 0.0;
    memory.terminal_reach_target_shift = None;
}

pub(in crate::animation::procedural) fn finish_settle_for_idle(memory: &mut LegIkMemory) {
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

pub(in crate::animation::procedural) fn settle_is_terminal(memory: &LegIkMemory) -> bool {
    memory.settle.is_some_and(|settle| settle.progress >= 1.0)
        && !memory.left_release_active
        && !memory.right_release_active
}

pub(in crate::animation::procedural) fn prepare_terminal_settle_contacts(
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

pub(in crate::animation::procedural) fn terminal_settle_contacts_are_rendered(
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

pub(in crate::animation::procedural) fn required_hip_shift_for_reach(
    upper: Vec3,
    target: Vec3,
    reach: f32,
) -> f32 {
    let horizontal_distance = (target - upper).xz().length();
    let maximum_vertical = (reach * reach - horizontal_distance * horizontal_distance)
        .max(0.0)
        .sqrt();
    target.y + maximum_vertical - upper.y
}

pub(in crate::animation::procedural) fn terminal_contact_solve_ownership(
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

pub(in crate::animation::procedural) fn seed_settle_from_rendered_feet(
    memory: &mut LegIkMemory,
    left: Option<Vec3>,
    right: Option<Vec3>,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    reset_terminal_settle_reach(memory);
    if let Some(left) = left.filter(|target| target.is_finite()) {
        memory.left_foot_world_target = Some(left);
        memory.left_foot_target = Some(rig_rotation.inverse() * (left - rig_origin));
        memory.left_foot_plant = None;
        memory.left_foot_plant_acquired = false;
    }
    if let Some(right) = right.filter(|target| target.is_finite()) {
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

pub(in crate::animation::procedural) fn settle_visible_foot(
    last_rendered_world: Option<Vec3>,
    current_authored_world: Option<Vec3>,
) -> Option<Vec3> {
    last_rendered_world
        .filter(|target| target.is_finite())
        .or_else(|| current_authored_world.filter(|target| target.is_finite()))
}

pub(in crate::animation::procedural) fn retain_settle_support(
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

pub(in crate::animation::procedural) fn settle_stance_is_safe(
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

pub(in crate::animation::procedural) fn sole_is_at_contact(
    ankle_y: f32,
    terrain_height: f32,
) -> bool {
    (ankle_y - terrain_height - MEASURED_ANKLE_SOLE_OFFSET_METRES).abs()
        <= SOLE_CONTACT_TOLERANCE_METRES
}

pub(in crate::animation::procedural) fn raised_support_is_actual(
    nominal_support: bool,
    ankle_y: f32,
    terrain_height: f32,
) -> bool {
    nominal_support && sole_is_at_contact(ankle_y, terrain_height)
}

pub(in crate::animation::procedural) fn balance_recovery_direction(
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

pub(in crate::animation::procedural) fn projected_capture_point(
    com: Vec3,
    velocity: Vec3,
    com_height: f32,
) -> Vec3 {
    let omega = (9.81 / com_height.max(0.25)).sqrt();
    com + velocity.with_y(0.0) / omega
}

pub(in crate::animation::procedural) fn choose_settle_support(
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

pub(in crate::animation::procedural) fn plan_settle_landing(
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

pub(in crate::animation::procedural) fn settle_swing_side(
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
