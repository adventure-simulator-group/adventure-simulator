use super::*;

mod body_response;
mod hands;
mod locomotion;
mod solver;

pub(in crate::animation) use body_response::apply_locomotion_body_response;
#[cfg(test)]
pub(super) use body_response::body_response_target;
pub(super) use body_response::presentation_tick_delta;
pub(in crate::animation) use hands::apply_arm_and_weapon_constraints;
#[cfg(test)]
pub(super) use hands::secondary_grip_world;
pub(in crate::animation) use locomotion::apply as apply_ordinary_locomotion_ik;
pub(super) use solver::*;

#[derive(Debug, Clone, Copy, PartialEq)]
struct LocomotionSettleState {
    support_left: bool,
    swing_start: Vec3,
    capture_point: Vec3,
    landing_target: Vec3,
    progress: f32,
    elapsed_seconds: f32,
    raised_handoff: bool,
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
    attack_stance_close: Option<bool>,
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
    left_support_exhausted_until_flight: bool,
    right_support_exhausted_until_flight: bool,
    left_release_active: bool,
    right_release_active: bool,
    left_release_target: Option<Vec3>,
    right_release_target: Option<Vec3>,
    pelvis_shift: f32,
    // Terminal stop correction is an absolute offset from the local rig-root
    // pose captured when dual-contact convergence begins. Sparse idle clips
    // do not necessarily rewrite that root every tick, so adding the retained
    // ordinary pelvis scalar repeatedly stalls or double-applies correction.
    terminal_contacts_prepared: bool,
    terminal_root_base_translation: Option<Vec3>,
    terminal_reach_shift: f32,
    terminal_reach_target_shift: Option<f32>,
    raised_pelvis_shift: f32,
    terrain_blend: f32,
    rig_origin: Option<Vec3>,
    rig_rotation: Option<Quat>,
    measured_owner_planar_speed: f32,
    evaluation_tick: Option<u64>,
    recent_movement_velocity: Vec3,
    settle: Option<LocomotionSettleState>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ArmIkMemory {
    left_arm: Option<Vec3>,
    right_arm: Option<Vec3>,
}

/// Optional deterministic clock for tools that render the same simulation
/// tick more than once. Gameplay leaves the override unset and advances from
/// Bevy's render delta.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub(crate) struct ProceduralAnimationClock {
    fixed_tick: Option<(u64, f32)>,
}

impl ProceduralAnimationClock {
    #[allow(dead_code)] // Used by the standalone animation viewer and unit fixtures.
    pub(crate) fn set_fixed_tick(&mut self, tick: u64, delta_seconds: f32) {
        self.fixed_tick = Some((tick, delta_seconds.max(0.0)));
    }

    pub(crate) fn fixed_step(&self) -> Option<(u64, f32)> {
        self.fixed_tick
    }
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
const RAISED_SWING_TARGET_SPEED: f32 = 0.12 * CONTINUITY_SAMPLE_HZ;
const GUARD_PIVOT_TRIGGER_METRES: f32 = 0.04;
const GUARD_PIVOT_STEP_SECONDS: f32 = 0.16;
const GUARD_PIVOT_LIFT_METRES: f32 = 0.08;
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
const RAISED_GUARD_PELVIS_DROP: f32 = 0.14;
/// Measured vertical distance from the Cascadeur ankle bone to its sole.
pub(crate) const MEASURED_ANKLE_SOLE_OFFSET_METRES: f32 = 0.085;
/// Maximum rendered ankle-to-terrain residual that still represents sole
/// contact after the complete analytic and scene-hierarchy solve.
pub(crate) const SOLE_CONTACT_TOLERANCE_METRES: f32 = 0.01;
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
const ATTACK_AIRBORNE_LUNGE_MIN_SPEED: f32 = 3.5;
const ATTACK_FLAT_SOLE_CLEARANCE: f32 = 0.01;
const ATTACK_SETTLE_SPEED_METRES_PER_SECOND: f32 = 1.5;
const ATTACK_SWITCH_PASS_DISTANCE_METRES: f32 = 0.08;
const ATTACK_SETTLE_MAXIMUM_PHASE: f32 = 0.2;
const ATTACK_RECOVERY_NO_STEP_DISTANCE_METRES: f32 = 0.075;
const ATTACK_RECOVERY_COMPLETE_DISTANCE_METRES: f32 = 0.005;
const ATTACK_RECOVERY_STEP_SPEED_METRES_PER_SECOND: f32 = 5.0;
const ATTACK_RECOVERY_MINIMUM_STEP_SECONDS: f32 = 0.12;
const ATTACK_RECOVERY_MAXIMUM_STEP_SECONDS: f32 = 0.30;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LegIkState(LegIkMemory);

impl LegIkState {
    pub(crate) fn feet_are_close_for_attack(&self) -> Option<bool> {
        self.0.attack_stance_close
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct LegIkDiagnostics {
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
}

impl LegIkState {
    pub(crate) fn diagnostics(&self) -> LegIkDiagnostics {
        let settle = self.0.settle;
        let diagnostics = LegIkDiagnostics {
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
        };
        diagnostics
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
    left_plant: Vec3,
    right_plant: Vec3,
    evaluation_tick: Option<u64>,
    step_sequence: u32,
    pub(crate) left_support_weight: f32,
    pub(crate) right_support_weight: f32,
    pub(crate) left_solve_target: Option<Vec3>,
    pub(crate) right_solve_target: Option<Vec3>,
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
}

/// One client-only, world-space attack step. The replicated state supplies a
/// stable start tick and typed direction; exact plants remain presentation
/// details and never move the controller or alter combat reach.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct AttackFootworkState {
    pub(crate) initialized: bool,
    start_tick: u64,
    lead: LeadFoot,
    step: AttackStep,
    footwork: Footwork,
    swing_left: bool,
    last_origin: Vec3,
    last_rotation: Quat,
    swing_start: Vec3,
    swing_end: Vec3,
    settle_end_phase: f32,
    settled_swing_start: Vec3,
    recovering: bool,
    recovery_step_active: bool,
    recovery_step_lift: bool,
    recovery_step_progress: f32,
    recovery_step_duration: f32,
    recovery_step_start: Vec3,
    recovery_step_end: Vec3,
    recovery_steps_completed: u8,
    recovery_left_adjusted: bool,
    recovery_right_adjusted: bool,
    left_ball_plant: Option<Vec3>,
    right_ball_plant: Option<Vec3>,
    left_plant: Vec3,
    right_plant: Vec3,
    evaluation_tick: Option<u64>,
    previous_phase: f32,
    pub(crate) support_handoffs: u8,
    pub(crate) left_support_weight: f32,
    pub(crate) right_support_weight: f32,
    pub(crate) left_requested_target: Option<Vec3>,
    pub(crate) right_requested_target: Option<Vec3>,
    pub(crate) left_solve_target: Option<Vec3>,
    pub(crate) right_solve_target: Option<Vec3>,
    pub(crate) maximum_reach_yield: f32,
    left_knee_bend_world: Option<Vec3>,
    right_knee_bend_world: Option<Vec3>,
    left_end_direction: Option<Vec3>,
    right_end_direction: Option<Vec3>,
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
            left_plant: Vec3::ZERO,
            right_plant: Vec3::ZERO,
            evaluation_tick: None,
            step_sequence: 0,
            left_support_weight: 0.0,
            right_support_weight: 0.0,
            left_solve_target: None,
            right_solve_target: None,
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
        }
    }
}

fn preserve_raised_handoff_targets(
    memory: &mut LegIkMemory,
    raised: RaisedFootworkState,
    rig_origin: Vec3,
    rig_rotation: Quat,
) {
    let left = raised.left_solve_target.unwrap_or(raised.left_plant);
    let right = raised.right_solve_target.unwrap_or(raised.right_plant);
    memory.left_foot_world_target = Some(left);
    memory.right_foot_world_target = Some(right);
    memory.left_foot_target = Some(rig_rotation.inverse() * (left - rig_origin));
    memory.right_foot_target = Some(rig_rotation.inverse() * (right - rig_origin));
    memory.left_release_active = true;
    memory.right_release_active = true;
    memory.left_release_target = None;
    memory.right_release_target = None;
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
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut ik_states: Query<&mut LegIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
    mut attack_states: Query<&mut AttackFootworkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let terrain = terrain.single().ok();
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        if locomotion::owns(skeleton) {
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = RaisedFootworkState::default();
            }
            if let Ok(mut state) = attack_states.get_mut(owner) {
                *state = AttackFootworkState::default();
            }
            continue;
        }
        if !terrain_ik_posture_is_valid(skeleton) {
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = LegIkMemory::default();
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = RaisedFootworkState::default();
            }
            if let Ok(mut state) = attack_states.get_mut(owner) {
                *state = AttackFootworkState::default();
            }
            continue;
        }
        let attack_action_active = skeleton.action_kind() == SkeletonAction::Attack;
        let attack_recovery_active = !attack_action_active
            && attack_states
                .get(owner)
                .is_ok_and(|state| state.initialized);
        let attack_footwork_active = attack_action_active || attack_recovery_active;
        let raised_guard_follower = !attack_footwork_active
            && raised_footwork_posture_is_valid(skeleton)
            && skeleton.weapon_guard() == WeaponGuardState::Raised
            && !skeleton.guarded_sprint_locomotion()
            && skeleton.action_kind() == SkeletonAction::None;
        let raised_footwork_was_active = raised_states
            .get(owner)
            .is_ok_and(|state| state.initialized);
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
        let desired_raised_pelvis_shift = if raised_guard_follower || attack_footwork_active {
            -RAISED_GUARD_PELVIS_DROP
        } else {
            0.0
        };
        if state_delta_seconds > 0.0 {
            memory.raised_pelvis_shift = advance_pelvis_shift(
                memory.raised_pelvis_shift,
                desired_raised_pelvis_shift,
                state_delta_seconds,
            );
        }
        let raised_pelvis_shift = memory.raised_pelvis_shift;
        if raised_pelvis_shift < -0.001
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
                        .transform_vector3(Vec3::Y * raised_pelvis_shift)
                })
                .unwrap_or(Vec3::Y * raised_pelvis_shift);
            if let Ok(mut transform) = transforms.p1().get_mut(pelvis) {
                transform.translation += local_delta;
            }
        }
        if attack_footwork_active {
            apply_attack_step(
                owner,
                skeleton,
                rig,
                terrain,
                enabled.0,
                rig_origin,
                rig_rotation,
                state_delta_seconds,
                clock.fixed_tick.map(|(tick, _)| tick),
                &parents,
                &mut attack_states,
                &mut raised_states,
                &mut memory,
                &mut transforms,
                &mut commands,
            );
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = memory;
            } else {
                commands.entity(owner).insert(LegIkState(memory));
            }
            continue;
        }
        if raised_guard_follower {
            prepare_slope_rotation_cache(&mut memory, SlopeAlignmentMode::Raised);
            // The authored guard is nearly straight-legged. Smoothly lower its
            // pelvis so a world-planted support foot remains within physical
            // reach without a one-frame stance-height snap at starts or stops.
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
            let tick = clock.fixed_tick.map(|(tick, _)| tick);
            let advances = match tick {
                Some(tick) => footwork.evaluation_tick != Some(tick),
                None => state_delta_seconds > 0.0,
            };
            if let Some(tick) = tick {
                footwork.evaluation_tick = Some(tick);
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
            let visible_left = memory.left_foot_world_target.unwrap_or(left_authored);
            let visible_right = memory.right_foot_world_target.unwrap_or(right_authored);
            memory.attack_stance_close = attack_stance_is_close(
                visible_left,
                visible_right,
                left_authored,
                right_authored,
                rig_rotation,
            );
            let discontinuous =
                footwork.initialized && rig_origin.distance_squared(footwork.step_origin) > 4.0;
            let sequence_delta = guard_step_sequence_delta(
                footwork.step_sequence,
                skeleton.raised_locomotion().step_sequence(),
            );
            let skipped_handoff = footwork.initialized && sequence_delta > 1;
            if !footwork.initialized
                || footwork.lead != skeleton.lead_foot
                || discontinuous
                || skipped_handoff
            {
                footwork = RaisedFootworkState {
                    initialized: true,
                    was_moving: skeleton.raised_locomotion().is_moving(),
                    awaiting_step_sequence: false,
                    half_step,
                    lead: skeleton.lead_foot,
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
                    left_plant: visible_left,
                    right_plant: visible_right,
                    evaluation_tick: tick,
                    step_sequence: skeleton.raised_locomotion().step_sequence(),
                    left_support_weight: 0.0,
                    right_support_weight: 0.0,
                    left_solve_target: None,
                    right_solve_target: None,
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
                };
            } else if advances && sequence_delta == 1 {
                if footwork.swing_left {
                    footwork.left_plant = footwork.left_solve_target.unwrap_or(footwork.swing_end);
                } else {
                    footwork.right_plant =
                        footwork.right_solve_target.unwrap_or(footwork.swing_end);
                }
                footwork.half_step = half_step;
                footwork.awaiting_step_sequence = false;
                footwork.step_sequence = skeleton.raised_locomotion().step_sequence();
                footwork.swing_left = swing_left;
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
            }
            let local_direction = skeleton
                .raised_locomotion()
                .local_direction()
                .normalize_or_zero();
            // Semantic controller axes are opposite the authored rig's X/Z
            // axes. The owner carries the single 180-degree body conversion.
            let rig_local_direction = -local_direction;
            let latched_speed = skeleton.raised_locomotion().speed();
            let live_speed = skeleton.world_velocity.with_y(0.0).length();
            let live_step_scale = (live_speed / latched_speed.max(0.01)).clamp(0.0, 1.0);
            let step_length = guard_step_length(latched_speed) * live_step_scale;
            let stationary_guard = !skeleton.raised_locomotion().is_moving();
            if !stationary_guard && !footwork.was_moving {
                // Begin a new cadence from the feet that were actually
                // rendered during idle. A stationary pivot or initial guard
                // acquisition may have moved them away from the older cadence
                // seed even when no pivot remains active.
                footwork.left_plant = visible_left;
                footwork.right_plant = visible_right;
                footwork.step_origin = rig_origin;
                footwork.step_rotation = rig_rotation;
                footwork.swing_stance_local = rig_rotation.inverse()
                    * ((if footwork.swing_left {
                        visible_left
                    } else {
                        visible_right
                    }) - rig_origin);
                footwork.swing_start = if footwork.swing_left {
                    visible_left
                } else {
                    visible_right
                };
                footwork.pivot_active = false;
                footwork.pivot_progress = 0.0;
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
            let horizontal_progress = smoothstep(0.0, 1.0, step_progress);
            let mut swing_target = footwork
                .swing_start
                .lerp(footwork.swing_end, horizontal_progress);
            let mut left_target = footwork.left_plant;
            let mut right_target = footwork.right_plant;
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
            let mut terrain_swing_end = footwork.swing_end;
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
            swing_target.y += (std::f32::consts::PI * step_progress).sin() * 0.10;
            let previous = if footwork.swing_left {
                footwork.left_solve_target
            } else {
                footwork.right_solve_target
            }
            .unwrap_or(footwork.swing_start);
            swing_target =
                limit_raised_swing_target(previous, swing_target, advances, state_delta_seconds);
            if footwork.swing_left {
                left_target = swing_target;
            } else {
                right_target = swing_target;
            }

            if stationary_guard {
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
                if advances {
                    if footwork.pivot_active {
                        footwork.pivot_progress = (footwork.pivot_progress
                            + state_delta_seconds.max(0.0) / GUARD_PIVOT_STEP_SECONDS)
                            .min(1.0);
                        if footwork.pivot_progress >= 1.0 {
                            if footwork.pivot_left {
                                footwork.left_plant = footwork.pivot_end;
                            } else {
                                footwork.right_plant = footwork.pivot_end;
                            }
                            footwork.pivot_active = false;
                        }
                    }
                    if !footwork.pivot_active {
                        let left_error = (left_authored - footwork.left_plant).xz().length();
                        let right_error = (right_authored - footwork.right_plant).xz().length();
                        if left_error.max(right_error) > GUARD_PIVOT_TRIGGER_METRES {
                            footwork.pivot_active = true;
                            let left_separation =
                                (left_authored - footwork.right_plant).xz().length();
                            let right_separation =
                                (right_authored - footwork.left_plant).xz().length();
                            footwork.pivot_left = if left_error <= GUARD_PIVOT_TRIGGER_METRES {
                                false
                            } else if right_error <= GUARD_PIVOT_TRIGGER_METRES {
                                true
                            } else {
                                left_separation >= right_separation
                            };
                            footwork.pivot_progress = 0.0;
                            footwork.pivot_origin = rig_origin;
                            footwork.pivot_start = if footwork.pivot_left {
                                footwork.left_plant
                            } else {
                                footwork.right_plant
                            };
                            let authored_end = if footwork.pivot_left {
                                left_authored
                            } else {
                                right_authored
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
                            footwork.pivot_end = constrain_guard_swing_to_live_corridor(
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
                        }
                    }
                }
                left_target = footwork.left_plant;
                right_target = footwork.right_plant;
                if footwork.pivot_active {
                    let progress = smoothstep(0.0, 1.0, footwork.pivot_progress);
                    let pivot_target = guard_pivot_target(
                        footwork.pivot_start,
                        footwork.pivot_end,
                        footwork.pivot_origin,
                        if footwork.pivot_left {
                            footwork.right_plant
                        } else {
                            footwork.left_plant
                        },
                        progress,
                    );
                    if footwork.pivot_left {
                        left_target = pivot_target;
                    } else {
                        right_target = pivot_target;
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
            footwork.was_moving = !stationary_guard;

            let mut airborne_orientation_owned = [true; 2];
            for (leg_index, (upper, lower, foot, target, left, support)) in [
                (
                    left_upper,
                    left_lower,
                    left_foot,
                    left_target,
                    true,
                    if stationary_guard {
                        !footwork.pivot_active || !footwork.pivot_left
                    } else {
                        footwork.awaiting_step_sequence || !footwork.swing_left
                    },
                ),
                (
                    right_upper,
                    right_lower,
                    right_foot,
                    right_target,
                    false,
                    if stationary_guard {
                        !footwork.pivot_active || footwork.pivot_left
                    } else {
                        footwork.awaiting_step_sequence || footwork.swing_left
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
                let pole = stabilized_knee_pole(
                    remembered,
                    previous_end_direction,
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    target,
                    canonical_world,
                )
                .unwrap_or(canonical_world);
                if let Some(solution) = solve_two_bone_with_reach(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_snapshot.global.translation(),
                    target,
                    upper_length,
                    lower_length,
                    pole,
                    maximum_reach(upper_length, lower_length),
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
                    if stationary_guard {
                        !footwork.pivot_active || !footwork.pivot_left
                    } else {
                        !footwork.swing_left
                    },
                ),
                (
                    right_foot,
                    false,
                    if stationary_guard {
                        !footwork.pivot_active || footwork.pivot_left
                    } else {
                        footwork.swing_left
                    },
                ),
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
                if let Some(solution) = solve_two_bone_preserving_with_reach(
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
                            height + MEASURED_ANKLE_SOLE_OFFSET_METRES + clearance,
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
                                    height + MEASURED_ANKLE_SOLE_OFFSET_METRES + clearance
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
                                + MEASURED_ANKLE_SOLE_OFFSET_METRES
                                + TERRAIN_TRANSITION_FLIGHT_TOE_CLEARANCE_METRES
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
                    RUN_SWING_MINIMUM_SOLE_CLEARANCE_METRES
                };
                let chosen = advance_run_airborne_world_target(
                    previous_owner_target,
                    planted_target,
                    rig_origin,
                    rig_rotation,
                    state_delta_seconds,
                    RUN_AIRBORNE_OWNER_TARGET_SPEED,
                    |xz| {
                        terrain.height_at(xz).map(|height| {
                            height + MEASURED_ANKLE_SOLE_OFFSET_METRES + acquisition_clearance
                        })
                    },
                );
                chosen
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
                        .map(|height| height + MEASURED_ANKLE_SOLE_OFFSET_METRES)
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
            let solution =
                if skeleton.posture() == Posture::Crouched || skeleton.animation_speed() <= 0.05 {
                    solve_two_bone_preserving_with_reach(
                        upper_snapshot.global.translation(),
                        lower_snapshot.global.translation(),
                        foot_position,
                        target,
                        upper_length,
                        lower_length,
                        pole,
                        terrain_maximum_reach(upper_length, lower_length),
                    )
                } else {
                    solve_two_bone_with_reach(
                        upper_snapshot.global.translation(),
                        lower_snapshot.global.translation(),
                        foot_position,
                        target,
                        upper_length,
                        lower_length,
                        pole,
                        maximum_reach(upper_length, lower_length),
                    )
                };
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
        if locomotion::owns(skeleton) {
            continue;
        }
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
        if !raised.initialized {
            continue;
        }
        for (role, left, nominal_support) in [
            (BoneRole::FootLeft, true, !raised.swing_left),
            (BoneRole::FootRight, false, raised.swing_left),
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

#[allow(clippy::too_many_arguments)]
fn apply_attack_step(
    owner: Entity,
    skeleton: &SkeletonState,
    rig: &HumanoidRig,
    terrain: Option<&SceneTerrain>,
    terrain_enabled: bool,
    rig_origin: Vec3,
    rig_rotation: Quat,
    state_delta_seconds: f32,
    tick: Option<u64>,
    parents: &Query<&ChildOf>,
    states: &mut Query<&mut AttackFootworkState>,
    raised_states: &mut Query<&mut RaisedFootworkState>,
    memory: &mut LegIkMemory,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
    commands: &mut Commands,
) {
    let (Some(&left_upper), Some(&left_lower), Some(&left_foot)) = (
        rig.get(&BoneRole::ThighLeft),
        rig.get(&BoneRole::ShinLeft),
        rig.get(&BoneRole::FootLeft),
    ) else {
        return;
    };
    let (Some(&right_upper), Some(&right_lower), Some(&right_foot)) = (
        rig.get(&BoneRole::ThighRight),
        rig.get(&BoneRole::ShinRight),
        rig.get(&BoneRole::FootRight),
    ) else {
        return;
    };
    let Some((_, _, left_snapshot)) =
        snapshot_chain(left_upper, left_lower, left_foot, parents, &transforms.p0())
    else {
        return;
    };
    let Some((_, _, right_snapshot)) = snapshot_chain(
        right_upper,
        right_lower,
        right_foot,
        parents,
        &transforms.p0(),
    ) else {
        return;
    };
    // Attack plants use the already lowered authored ankles. Undoing the
    // pelvis drop here would lift the planted sole during the action and force
    // a visible vertical snap when ordinary guard footwork resumes.
    let left_authored = left_snapshot.global.translation();
    let right_authored = right_snapshot.global.translation();
    let visible_left = memory.left_foot_world_target.unwrap_or(left_authored);
    let visible_right = memory.right_foot_world_target.unwrap_or(right_authored);
    let mut state = states
        .get_mut(owner)
        .map(|state| *state)
        .unwrap_or_default();
    let attack_active = skeleton.action_kind() == SkeletonAction::Attack;
    let start_tick = if attack_active {
        skeleton.action_start_tick().unwrap_or_default()
    } else {
        state.start_tick
    };
    let step = if attack_active {
        skeleton.attack_step()
    } else {
        state.step
    };
    let footwork = if attack_active {
        skeleton.footwork()
    } else {
        state.footwork
    };
    let start_lead = if attack_active {
        skeleton.attack_start_lead()
    } else {
        state.lead
    };
    let phase = if attack_active {
        skeleton.action_phase().clamp(0.0, 1.0)
    } else {
        1.0
    };
    let replacement = attack_active
        && (!state.initialized
            || state.start_tick != start_tick
            || state.lead != start_lead
            || state.step != step
            || state.footwork != footwork
            || (state.initialized
                && (rig_origin.distance(state.last_origin) > MAX_OWNER_TRANSLATION_PER_TICK
                    || rig_rotation.angle_between(state.last_rotation).to_degrees()
                        > MAX_OWNER_ROTATION_PER_TICK_DEGREES))
            || phase + 0.001 < state.previous_phase);
    if replacement {
        let swing_left = attack_swing_is_left(
            footwork,
            visible_left,
            visible_right,
            rig_origin,
            rig_rotation,
            skeleton.world_velocity,
            skeleton
                .attack_movement()
                .map(|(direction, _)| direction)
                .unwrap_or(Vec2::ZERO),
            start_lead,
        );
        let swing_start = if swing_left {
            visible_left
        } else {
            visible_right
        };
        let settled_swing_start = attack_grounded_target(swing_start, terrain);
        let preparation_seconds =
            skeleton.action_preparation_ticks().unwrap_or(1) as f32 / LOCOMOTION_SAMPLE_HZ;
        let settle_seconds = (swing_start.y - settled_swing_start.y).max(0.0)
            / ATTACK_SETTLE_SPEED_METRES_PER_SECOND;
        let settle_end_phase = attack_settle_end_phase(settle_seconds, preparation_seconds);
        let (movement_direction, movement_speed) =
            skeleton.attack_movement().unwrap_or((Vec2::Y, 0.0));
        let world_direction = (rig_rotation
            * Vec3::new(movement_direction.x, 0.0, movement_direction.y))
        .normalize_or_zero();
        let support = if swing_left {
            visible_right
        } else {
            visible_left
        };
        let step_distance = attack_step_contact_distance(
            footwork,
            settled_swing_start,
            support,
            world_direction,
            movement_speed,
            preparation_seconds,
        );
        let mut swing_end = settled_swing_start + world_direction * step_distance;
        // Authored attack clips are free to lift or delay their FK foot, but
        // procedural attack footwork must arrive on the ground with contact.
        // Use the planted ankle's height when the attack begins during an
        // airborne locomotion lobe. Otherwise the moving foot can reach its
        // horizontal contact endpoint while still visibly raised, making the
        // later recovery grounding read as the actual step.
        swing_end.y = support.y;
        state = AttackFootworkState {
            initialized: true,
            start_tick,
            lead: start_lead,
            step,
            footwork,
            swing_left,
            last_origin: rig_origin,
            last_rotation: rig_rotation,
            swing_start,
            swing_end,
            settle_end_phase,
            settled_swing_start,
            recovering: false,
            recovery_step_active: false,
            recovery_step_lift: false,
            recovery_step_progress: 0.0,
            recovery_step_duration: 0.0,
            recovery_step_start: Vec3::ZERO,
            recovery_step_end: Vec3::ZERO,
            recovery_steps_completed: 0,
            recovery_left_adjusted: false,
            recovery_right_adjusted: false,
            left_ball_plant: (!swing_left).then(|| {
                memory.left_last_rendered_toe_world.unwrap_or_else(|| {
                    rig.get(&BoneRole::ToeLeft)
                        .and_then(|toe| snapshot(*toe, parents, &transforms.p0()))
                        .map_or(visible_left, |toe| toe.global.translation())
                })
            }),
            right_ball_plant: swing_left.then(|| {
                memory.right_last_rendered_toe_world.unwrap_or_else(|| {
                    rig.get(&BoneRole::ToeRight)
                        .and_then(|toe| snapshot(*toe, parents, &transforms.p0()))
                        .map_or(visible_right, |toe| toe.global.translation())
                })
            }),
            left_plant: visible_left,
            right_plant: visible_right,
            evaluation_tick: tick,
            previous_phase: phase,
            support_handoffs: 0,
            left_support_weight: 1.0,
            right_support_weight: 1.0,
            left_requested_target: None,
            right_requested_target: None,
            left_solve_target: None,
            right_solve_target: None,
            maximum_reach_yield: 0.0,
            left_knee_bend_world: None,
            right_knee_bend_world: None,
            left_end_direction: None,
            right_end_direction: None,
        };
    }
    if !state.initialized {
        return;
    }
    let advances = match tick {
        Some(tick) => state.evaluation_tick != Some(tick),
        None => state_delta_seconds > 0.0,
    };
    if advances {
        if attack_active
            && state.footwork == Footwork::Switch
            && state.previous_phase < 0.5
            && phase >= 0.5
        {
            state.support_handoffs = state.support_handoffs.saturating_add(1);
        }
        state.previous_phase = phase;
        state.last_origin = rig_origin;
        state.last_rotation = rig_rotation;
    }
    state.evaluation_tick = tick;
    let mut finish_after_solve = false;
    let (left_target, right_target, left_support, right_support) = if attack_active {
        state.recovering = false;
        let takes_step = state.footwork == Footwork::Switch
            || skeleton
                .attack_movement()
                .is_some_and(|(_, speed)| speed > 0.05);
        if !takes_step {
            (state.left_plant, state.right_plant, 1.0, 1.0)
        } else {
            if phase <= 0.5 {
                // The attack can be latched before the ordinary locomotion IK
                // publishes its final support plant for that presentation
                // frame. Refresh only the endpoint height from the retained
                // planted foot; its horizontal strike endpoint stays fixed.
                state.swing_end.y = if state.swing_left {
                    state.right_solve_target.unwrap_or(state.right_plant).y
                } else {
                    state.left_solve_target.unwrap_or(state.left_plant).y
                };
            }
            let swing_target = if phase < state.settle_end_phase {
                let settle = smoothstep(0.0, state.settle_end_phase.max(f32::EPSILON), phase);
                state.swing_start.lerp(state.settled_swing_start, settle)
            } else if phase < 0.5 {
                let strike = smoothstep(state.settle_end_phase, 0.5, phase);
                let mut target = state.settled_swing_start.lerp(state.swing_end, strike);
                target.y += (std::f32::consts::PI * strike).sin() * 0.10;
                target
            } else {
                state.swing_end
            };
            if phase >= 0.5 {
                if state.swing_left {
                    state.left_plant = swing_target;
                } else {
                    state.right_plant = swing_target;
                }
            }
            let (left, right) = if state.swing_left {
                (swing_target, state.right_plant)
            } else {
                (state.left_plant, swing_target)
            };
            let landed = (phase >= 0.5) as u8 as f32;
            let (left_support, right_support) = if state.swing_left {
                (landed, 1.0)
            } else {
                (1.0, landed)
            };
            (left, right, left_support, right_support)
        }
    } else {
        if !state.recovering {
            // Recovery starts from the rendered result of the strike, not
            // from the pre-ball-correction ankle requests. The support ankle
            // may have moved substantially while its ball stayed planted;
            // treating the old request as its current plant would hide that
            // error and make locomotion inherit an elevated or twisted foot.
            state.left_plant = state.left_solve_target.unwrap_or(state.left_plant);
            state.right_plant = state.right_solve_target.unwrap_or(state.right_plant);
            state.left_ball_plant = None;
            state.right_ball_plant = None;
        }
        state.recovering = true;
        let ideal_left = attack_grounded_target(left_authored, terrain);
        let ideal_right = attack_grounded_target(right_authored, terrain);
        if !state.recovery_step_active {
            let left_error = state.left_plant.distance(ideal_left);
            let right_error = state.right_plant.distance(ideal_right);
            let moving_handoff = skeleton.raised_locomotion().is_moving()
                && state.recovery_left_adjusted
                && state.recovery_right_adjusted;
            if moving_handoff {
                // Once two bounded correction steps have re-established a
                // viable alternating stance, hand the still-moving character
                // to ordinary raised locomotion from these exact retained
                // plants. Chasing a continuously advancing authored guard to
                // zero error would otherwise retain attack ownership forever.
                finish_after_solve = true;
            } else if left_error.max(right_error) <= ATTACK_RECOVERY_COMPLETE_DISTANCE_METRES {
                state.left_plant = ideal_left;
                state.right_plant = ideal_right;
                finish_after_solve = true;
            } else {
                state.swing_left =
                    match (state.recovery_left_adjusted, state.recovery_right_adjusted) {
                        (true, false) => false,
                        (false, true) => true,
                        _ => left_error >= right_error,
                    };
                let (start, end, error) = if state.swing_left {
                    (state.left_plant, ideal_left, left_error)
                } else {
                    (state.right_plant, ideal_right, right_error)
                };
                state.recovery_step_active = true;
                state.recovery_step_lift = error > ATTACK_RECOVERY_NO_STEP_DISTANCE_METRES;
                state.recovery_step_progress = 0.0;
                state.recovery_step_duration =
                    (error / ATTACK_RECOVERY_STEP_SPEED_METRES_PER_SECOND).clamp(
                        ATTACK_RECOVERY_MINIMUM_STEP_SECONDS,
                        ATTACK_RECOVERY_MAXIMUM_STEP_SECONDS,
                    );
                state.recovery_step_start = start;
                state.recovery_step_end = end;
                if state.swing_left {
                    state.left_ball_plant = None;
                } else {
                    state.right_ball_plant = None;
                }
            }
        }
        if state.recovery_step_active && advances {
            state.recovery_step_progress = (state.recovery_step_progress
                + state_delta_seconds / state.recovery_step_duration.max(f32::EPSILON))
            .min(1.0);
        }
        let moving = if state.recovery_step_lift {
            settle_swing_target(
                state.recovery_step_start,
                state.recovery_step_end,
                state.recovery_step_progress,
            )
        } else {
            state.recovery_step_start.lerp(
                state.recovery_step_end,
                smoothstep(0.0, 1.0, state.recovery_step_progress),
            )
        };
        let (left, right, left_support, right_support) = if state.recovery_step_active {
            if state.swing_left {
                (moving, state.right_plant, 0.0, 1.0)
            } else {
                (state.left_plant, moving, 1.0, 0.0)
            }
        } else {
            (state.left_plant, state.right_plant, 1.0, 1.0)
        };
        if state.recovery_step_active && state.recovery_step_progress >= 1.0 {
            if state.swing_left {
                state.left_plant = state.recovery_step_end;
                state.recovery_left_adjusted = true;
            } else {
                state.right_plant = state.recovery_step_end;
                state.recovery_right_adjusted = true;
            }
            state.recovery_step_active = false;
            state.recovery_steps_completed = state.recovery_steps_completed.saturating_add(1);
        }
        (left, right, left_support, right_support)
    };
    state.left_support_weight = left_support;
    state.right_support_weight = right_support;

    for (upper, lower, foot, requested_target, left, support_weight) in [
        (
            left_upper,
            left_lower,
            left_foot,
            left_target,
            true,
            left_support,
        ),
        (
            right_upper,
            right_lower,
            right_foot,
            right_target,
            false,
            right_support,
        ),
    ] {
        let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
            snapshot_chain(upper, lower, foot, parents, &transforms.p0())
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
        let attack_reach = if terrain_enabled {
            // Cross-slope support needs the same small knee-reserve release as
            // ordinary terrain IK. Keeping the flat-ground reserve here made
            // the nominally planted foot creep as the pelvis crossed a slope.
            terrain_maximum_reach(upper_length, lower_length)
        } else {
            maximum_reach(upper_length, lower_length)
        };
        // Fast root continuation can carry the hip beyond a literal world
        // plant. Yield only the unreachable residual, giving a compressed
        // lunge instead of a straight knee, teleport, or owner translation.
        let constrained = constrain_target_to_reach(
            requested_target,
            upper_snapshot.global.translation(),
            attack_reach,
        );
        // The attack trajectory and controller root are already sampled at a
        // fixed rate. A second generic locomotion speed limit makes the solve
        // lag behind the requested strike and shifts maximum extension into
        // recovery. The analytic reach constraint below remains the safety
        // bound; use the attack sample directly so contact stays synchronized.
        let advanced = constrained;
        let mut target =
            constrain_target_to_reach(advanced, upper_snapshot.global.translation(), attack_reach);
        state.maximum_reach_yield = state
            .maximum_reach_yield
            .max(requested_target.distance(target));
        let side = anatomical_side(
            rig_rotation,
            rig_origin,
            upper_snapshot.global.translation(),
            left,
        );
        let remembered = if left {
            state.left_knee_bend_world
        } else {
            state.right_knee_bend_world
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
            state.left_end_direction
        } else {
            state.right_end_direction
        };
        let canonical = canonical_knee_pole(side);
        let canonical_world = pole_to_world(rig_rotation, canonical);
        let pole = stabilized_knee_pole(
            remembered,
            previous_end_direction,
            upper_snapshot.global.translation(),
            lower_snapshot.global.translation(),
            target,
            canonical_world,
        )
        .unwrap_or(canonical_world);
        if let Some(solution) = solve_two_bone_with_reach(
            upper_snapshot.global.translation(),
            lower_snapshot.global.translation(),
            foot_snapshot.global.translation(),
            target,
            upper_length,
            lower_length,
            pole,
            attack_reach,
        ) {
            apply_two_bone_solution(upper, lower, foot, solution, parents, transforms);
            let bend = (solution.knee - upper_snapshot.global.translation())
                .reject_from_normalized(solution.end_direction);
            if advances && let Some(valid) = bend.try_normalize() {
                if left {
                    memory.left_leg = Some(pole_to_owner(rig_rotation, valid));
                    state.left_knee_bend_world = Some(valid);
                    state.left_end_direction = Some(solution.end_direction);
                } else {
                    memory.right_leg = Some(pole_to_owner(rig_rotation, valid));
                    state.right_knee_bend_world = Some(valid);
                    state.right_end_direction = Some(solution.end_direction);
                }
            }
        }
        if terrain_enabled
            && support_weight >= 0.5
            && let Some(terrain) = terrain
            && let Some(normal) = terrain.normal_at(target.xz())
            && let Some(sole_axis) = rig.sole_axis(left)
        {
            align_foot_to_slope(foot, sole_axis, normal, parents, transforms);
        }
        let ball_plant = if left {
            state.left_ball_plant
        } else {
            state.right_ball_plant
        };
        if let Some(ball_plant) = ball_plant
            && let Some(&toe) = rig.get(if left {
                &BoneRole::ToeLeft
            } else {
                &BoneRole::ToeRight
            })
            && let Some(toe_snapshot) = snapshot(toe, parents, &transforms.p0())
        {
            // The ankle is only a convenient IK endpoint; visible contact is
            // owned by the ball/toe joint. Correct the solved ankle by the
            // ball residual after authored and terrain rotation so the foot
            // may pivot without sliding its contact point.
            let residual = ball_plant - toe_snapshot.global.translation();
            if residual.is_finite() && residual.length() <= 0.30 {
                target += residual;
                if let Some((corrected_upper, corrected_lower, corrected_foot)) =
                    snapshot_chain(upper, lower, foot, parents, &transforms.p0())
                {
                    target = constrain_target_to_reach(
                        target,
                        corrected_upper.global.translation(),
                        attack_reach,
                    );
                    if let Some(solution) = solve_two_bone_with_reach(
                        corrected_upper.global.translation(),
                        corrected_lower.global.translation(),
                        corrected_foot.global.translation(),
                        target,
                        upper_length,
                        lower_length,
                        pole,
                        attack_reach,
                    ) {
                        apply_two_bone_solution(upper, lower, foot, solution, parents, transforms);
                    }
                }
            }
        }
        if support_weight >= 0.95
            && let Some(&toe) = rig.get(if left {
                &BoneRole::ToeLeft
            } else {
                &BoneRole::ToeRight
            })
            && let Some(toe_snapshot) = snapshot(toe, parents, &transforms.p0())
        {
            if left && state.left_ball_plant.is_none() {
                state.left_ball_plant = Some(toe_snapshot.global.translation());
            } else if !left && state.right_ball_plant.is_none() {
                state.right_ball_plant = Some(toe_snapshot.global.translation());
            }
        }
        if left {
            state.left_requested_target = Some(requested_target);
            state.left_solve_target = Some(target);
            memory.left_foot_world_target = Some(target);
            memory.left_support_weight = Some(support_weight);
        } else {
            state.right_requested_target = Some(requested_target);
            state.right_solve_target = Some(target);
            memory.right_foot_world_target = Some(target);
            memory.right_support_weight = Some(support_weight);
        }
    }
    if finish_after_solve && skeleton.raised_locomotion().is_moving() {
        // A moving character can hand these exact plants to the ordinary
        // raised-step planner. At idle, keep attack recovery ownership so a
        // late graph transition into the resting guard is absorbed as another
        // bounded correction instead of snapping both feet on the next frame.
        let left_plant = state.left_solve_target.unwrap_or(state.left_plant);
        let right_plant = state.right_solve_target.unwrap_or(state.right_plant);
        let swing_left = skeleton.raised_locomotion().swing_foot() == Some(LeadFoot::Left);
        let visible_swing = if swing_left { left_plant } else { right_plant };
        let handoff = RaisedFootworkState {
            initialized: true,
            was_moving: skeleton.raised_locomotion().is_moving(),
            awaiting_step_sequence: true,
            half_step: (skeleton.gait_phase.rem_euclid(1.0) >= 0.5) as u8,
            lead: skeleton.lead_foot,
            swing_left,
            step_origin: rig_origin,
            step_rotation: rig_rotation,
            swing_stance_local: rig_rotation.inverse() * (visible_swing - rig_origin),
            swing_start: visible_swing,
            swing_end: visible_swing,
            left_plant,
            right_plant,
            evaluation_tick: tick,
            step_sequence: skeleton.raised_locomotion().step_sequence(),
            left_support_weight: 1.0,
            right_support_weight: 1.0,
            left_solve_target: Some(left_plant),
            right_solve_target: Some(right_plant),
            ..default()
        };
        if let Ok(mut raised) = raised_states.get_mut(owner) {
            *raised = handoff;
        } else {
            commands.entity(owner).insert(handoff);
        }
        state.initialized = false;
    }
    if let Ok(mut stored) = states.get_mut(owner) {
        *stored = state;
    } else {
        commands.entity(owner).insert(state);
    }
}

pub(super) fn raised_footwork_posture_is_valid(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded() && skeleton.posture() == Posture::Upright
}

pub(super) fn terrain_ik_posture_is_valid(skeleton: &SkeletonState) -> bool {
    skeleton.is_grounded()
        && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
        && matches!(
            skeleton.action_kind(),
            SkeletonAction::None | SkeletonAction::Attack
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
        start.y.lerp(end.y, progress)
            + (std::f32::consts::PI * progress).sin() * GUARD_PIVOT_LIFT_METRES,
        origin.z + planar.y,
    )
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

    transported.or(authored).or(Some(canonical_bend))
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

fn raised_support_is_actual(nominal_support: bool, ankle_y: f32, terrain_height: f32) -> bool {
    nominal_support && sole_is_at_contact(ankle_y, terrain_height)
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
    if skeleton.action_kind() == SkeletonAction::Attack {
        let takes_step = skeleton.footwork() == Footwork::Switch
            || skeleton
                .attack_movement()
                .is_some_and(|(_, speed)| speed > 0.05);
        if !takes_step {
            return (1.0, 1.0);
        }
        let swing_left = match skeleton.footwork() {
            Footwork::Stay => skeleton.attack_start_lead() == LeadFoot::Left,
            Footwork::Switch => skeleton.attack_start_lead() == LeadFoot::Right,
        };
        if skeleton.attack_step_speed() >= ATTACK_AIRBORNE_LUNGE_MIN_SPEED {
            let phase = skeleton.action_phase();
            let launch = 1.0 - smoothstep(0.0, 0.12, phase);
            let recovery = smoothstep(0.5, 1.0, phase);
            return if swing_left {
                (if phase < 0.5 { 0.0 } else { 1.0 }, launch.max(recovery))
            } else {
                (launch.max(recovery), if phase < 0.5 { 0.0 } else { 1.0 })
            };
        }
        let handoff = smoothstep(0.45, 0.55, skeleton.action_phase());
        return if swing_left {
            (handoff, 1.0 - handoff)
        } else {
            (1.0 - handoff, handoff)
        };
    }
    if skeleton.action_kind() != SkeletonAction::None {
        return (0.0, 0.0);
    }
    if speed <= 0.05 {
        return (1.0, 1.0);
    }
    if skeleton.weapon_guard() == WeaponGuardState::Raised
        && skeleton.action_kind() == SkeletonAction::None
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

fn attack_grounded_target(mut target: Vec3, terrain: Option<&SceneTerrain>) -> Vec3 {
    if let Some(height) = terrain.and_then(|terrain| terrain.height_at(target.xz())) {
        target.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
    } else {
        target.y += ATTACK_FLAT_SOLE_CLEARANCE;
    }
    target
}

fn attack_stance_is_close(
    visible_left: Vec3,
    visible_right: Vec3,
    guard_left: Vec3,
    guard_right: Vec3,
    rig_rotation: Quat,
) -> Option<bool> {
    let forward = (rig_rotation * Vec3::Z).xz().normalize_or_zero();
    if forward == Vec2::ZERO
        || !visible_left.is_finite()
        || !visible_right.is_finite()
        || !guard_left.is_finite()
        || !guard_right.is_finite()
    {
        return None;
    }
    let visible_separation = (visible_left - visible_right).xz().dot(forward).abs();
    let guard_separation = (guard_left - guard_right).xz().dot(forward).abs();
    (guard_separation > 0.01).then_some(visible_separation <= guard_separation * 0.5)
}

fn attack_swing_is_left(
    footwork: Footwork,
    left: Vec3,
    right: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    world_velocity: Vec3,
    local_movement_direction: Vec2,
    start_lead: LeadFoot,
) -> bool {
    let travel = world_velocity.xz().normalize_or_zero();
    let fallback_forward = (rig_rotation * Vec3::Z).xz().normalize_or_zero();
    let local_travel = (rig_rotation
        * Vec3::new(local_movement_direction.x, 0.0, local_movement_direction.y))
    .xz()
    .normalize_or_zero();
    let movement_forward = if travel != Vec2::ZERO {
        travel
    } else if local_travel != Vec2::ZERO {
        local_travel
    } else {
        fallback_forward
    };
    let body_forward = if footwork == Footwork::Stay {
        fallback_forward
    } else {
        movement_forward
    };
    let left_forward = (left - rig_origin).xz().dot(body_forward);
    let right_forward = (right - rig_origin).xz().dot(body_forward);
    if (left_forward - right_forward).abs() <= 0.005 {
        return if footwork == Footwork::Stay {
            start_lead == LeadFoot::Left
        } else {
            start_lead == LeadFoot::Right
        };
    }
    if footwork == Footwork::Stay {
        left_forward > right_forward
    } else {
        left_forward < right_forward
    }
}

fn attack_settle_end_phase(settle_seconds: f32, preparation_seconds: f32) -> f32 {
    if settle_seconds <= f32::EPSILON {
        0.0
    } else {
        (settle_seconds / preparation_seconds.max(1.0 / LOCOMOTION_SAMPLE_HZ) * 0.5)
            .clamp(0.0, ATTACK_SETTLE_MAXIMUM_PHASE)
    }
}

fn attack_step_contact_distance(
    footwork: Footwork,
    swing_start: Vec3,
    support: Vec3,
    world_direction: Vec3,
    movement_speed: f32,
    preparation_seconds: f32,
) -> f32 {
    let root_travel_to_contact = movement_speed.max(0.0) * preparation_seconds.max(0.0);
    let semantic_step = match footwork {
        Footwork::Stay => guard_step_length(movement_speed),
        Footwork::Switch => {
            let distance_to_support = (support - swing_start).dot(world_direction).max(0.0);
            distance_to_support + ATTACK_SWITCH_PASS_DISTANCE_METRES
        }
    };
    // The foot lands with the strike while remaining reachable from a root
    // that continues along the captured movement vector. Slow attacks still
    // read as a deliberate guard step; fast attacks cover their contact-time
    // root travel instead of leaving the foot to catch up during recovery.
    semantic_step.max(root_travel_to_contact)
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

pub(super) fn guard_step_sequence_delta(previous: u32, current: u32) -> u32 {
    current.wrapping_sub(previous)
}

fn limit_raised_swing_target(
    previous: Vec3,
    desired: Vec3,
    advances: bool,
    delta_seconds: f32,
) -> Vec3 {
    let maximum_step = if advances {
        RAISED_SWING_TARGET_SPEED * delta_seconds.max(0.0)
    } else {
        0.0
    };
    previous + (desired - previous).clamp_length_max(maximum_step)
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

        let first_root = Vec3::new(-0.100270, 2.863136, -10.316130);
        let next_root = Vec3::new(-0.100349, 2.875328, -10.407523);
        let target = Vec3::new(-0.120271, 2.308135, -11.034690);
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
        )
        .unwrap();

        assert!(pole.dot(remembered) > 0.999);
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
            MEASURED_ANKLE_SOLE_OFFSET_METRES + SOLE_CONTACT_TOLERANCE_METRES - 0.001,
            terrain_height,
        ));
        assert!(!raised_support_is_actual(
            true,
            MEASURED_ANKLE_SOLE_OFFSET_METRES + 0.023,
            terrain_height,
        ));
        assert!(!raised_support_is_actual(
            false,
            MEASURED_ANKLE_SOLE_OFFSET_METRES,
            terrain_height,
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
            ..default()
        };
        let mut memory = LegIkMemory::default();

        preserve_raised_handoff_targets(&mut memory, raised, rig_origin, rig_rotation);

        assert_eq!(memory.left_foot_world_target, Some(left));
        assert_eq!(memory.right_foot_world_target, Some(right));
        assert!(memory.left_release_active && memory.right_release_active);
        let restored_left =
            rig_origin + rig_rotation * memory.left_foot_target.expect("left owner target");
        let restored_right =
            rig_origin + rig_rotation * memory.right_foot_target.expect("right owner target");
        assert!(restored_left.distance(left) < 0.000001);
        assert!(restored_right.distance(right) < 0.000001);
    }

    #[test]
    fn raised_stop_settle_keeps_terrain_ik_alive_across_ticks() {
        let mut settle = LocomotionSettleState {
            support_left: true,
            swing_start: Vec3::new(0.2, 0.1, 0.0),
            capture_point: Vec3::ZERO,
            landing_target: Vec3::new(-0.2, 0.1, -0.3),
            progress: 0.0,
            elapsed_seconds: 0.0,
            raised_handoff: true,
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
    fn attack_swing_selection_uses_spatial_order_not_guard_label() {
        let back = Vec3::new(-0.2, 0.1, -0.4);
        let front = Vec3::new(0.2, 0.1, 0.4);
        for lead in [LeadFoot::Left, LeadFoot::Right] {
            assert!(attack_swing_is_left(
                Footwork::Switch,
                back,
                front,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::Z,
                Vec2::Y,
                lead,
            ));
            assert!(!attack_swing_is_left(
                Footwork::Switch,
                back,
                front,
                Vec3::ZERO,
                Quat::IDENTITY,
                -Vec3::Z,
                -Vec2::Y,
                lead,
            ));
        }
    }

    #[test]
    fn stay_attack_steps_the_body_forward_foot() {
        let back = Vec3::new(-0.2, 0.1, -0.1);
        let front = Vec3::new(0.2, 0.1, 0.1);
        for lead in [LeadFoot::Left, LeadFoot::Right] {
            assert!(!attack_swing_is_left(
                Footwork::Stay,
                back,
                front,
                Vec3::ZERO,
                Quat::IDENTITY,
                -Vec3::Z,
                -Vec2::Y,
                lead,
            ));
        }
    }

    #[test]
    fn attack_stance_close_threshold_is_half_guard_separation() {
        let guard_left = Vec3::new(-0.2, 0.0, -0.4);
        let guard_right = Vec3::new(0.2, 0.0, 0.4);
        let left = Vec3::new(-0.2, 0.0, -0.2);
        assert_eq!(
            attack_stance_is_close(
                left,
                Vec3::new(0.2, 0.0, 0.2),
                guard_left,
                guard_right,
                Quat::IDENTITY,
            ),
            Some(true)
        );
        assert_eq!(
            attack_stance_is_close(
                left,
                Vec3::new(0.2, 0.0, 0.201),
                guard_left,
                guard_right,
                Quat::IDENTITY,
            ),
            Some(false)
        );
    }

    #[test]
    fn moving_attack_step_reaches_the_contact_time_root_travel() {
        let speed = 2.0;
        let preparation_seconds = 0.30;
        let distance = attack_step_contact_distance(
            Footwork::Stay,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 0.3),
            Vec3::Z,
            speed,
            preparation_seconds,
        );
        assert_eq!(distance, speed * preparation_seconds);
    }

    #[test]
    fn stationary_switch_step_passes_the_planted_foot_before_contact() {
        let swing = Vec3::new(0.0, 0.0, -0.2);
        let support = Vec3::new(0.0, 0.0, 0.2);
        let distance =
            attack_step_contact_distance(Footwork::Switch, swing, support, Vec3::Z, 0.0, 0.30);
        assert_eq!(distance, 0.4 + ATTACK_SWITCH_PASS_DISTANCE_METRES);
    }

    #[test]
    fn airborne_attack_settle_is_bounded_to_early_preparation() {
        assert_eq!(attack_settle_end_phase(0.0, 0.25), 0.0);
        assert_eq!(
            attack_settle_end_phase(10.0, 0.25),
            ATTACK_SETTLE_MAXIMUM_PHASE
        );
        assert!(attack_settle_end_phase(0.02, 0.25) < 0.5);
    }

    #[test]
    fn ordinary_guard_swing_is_not_limited_below_its_contact_deadline() {
        let previous = Vec3::ZERO;
        let desired = Vec3::new(0.0, 0.02, 0.115);
        assert_eq!(
            limit_raised_swing_target(previous, desired, true, 1.0 / CONTINUITY_SAMPLE_HZ),
            desired
        );

        let distant_recovery = Vec3::new(0.0, 0.1, 0.25);
        let guarded_recovery =
            limit_raised_swing_target(previous, distant_recovery, true, 1.0 / CONTINUITY_SAMPLE_HZ);
        assert!(guarded_recovery.distance(previous) <= 0.12 + 0.0001);
        assert_eq!(
            limit_raised_swing_target(
                guarded_recovery,
                distant_recovery,
                false,
                1.0 / CONTINUITY_SAMPLE_HZ,
            ),
            guarded_recovery
        );
    }
}
