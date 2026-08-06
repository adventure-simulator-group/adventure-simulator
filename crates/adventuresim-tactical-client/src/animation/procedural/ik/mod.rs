use super::*;

mod body_response;
mod hands;
mod solver;

pub(in crate::animation) use body_response::apply_locomotion_body_response;
#[cfg(test)]
pub(super) use body_response::body_response_target;
pub(super) use body_response::presentation_tick_delta;
pub(in crate::animation) use hands::apply_arm_and_weapon_constraints;
#[cfg(test)]
pub(super) use hands::secondary_grip_world;
pub(super) use solver::*;

#[derive(Debug, Clone, Copy)]
struct LocomotionSettleState {
    support_left: bool,
    swing_start: Vec3,
    capture_point: Vec3,
    landing_target: Vec3,
    progress: f32,
    elapsed_seconds: f32,
    cancelled_by_restart: bool,
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

#[derive(Debug, Clone, Copy, Default)]
struct LegIkMemory {
    left_leg: Option<Vec3>,
    right_leg: Option<Vec3>,
    left_terrain_pole_world: Option<Vec3>,
    right_terrain_pole_world: Option<Vec3>,
    left_rotation_chain: Option<LegRotationChain>,
    right_rotation_chain: Option<LegRotationChain>,
    left_foot_orientation_world: Option<Quat>,
    right_foot_orientation_world: Option<Quat>,
    left_contact_orientation_blend_active: bool,
    right_contact_orientation_blend_active: bool,
    slope_alignment_mode: Option<SlopeAlignmentMode>,
    left_foot_plant: Option<Vec3>,
    right_foot_plant: Option<Vec3>,
    left_foot_target: Option<Vec3>,
    right_foot_target: Option<Vec3>,
    left_foot_world_target: Option<Vec3>,
    right_foot_world_target: Option<Vec3>,
    left_authored_world_target: Option<Vec3>,
    right_authored_world_target: Option<Vec3>,
    left_planned_contact: Option<Vec3>,
    right_planned_contact: Option<Vec3>,
    left_support_weight: Option<f32>,
    right_support_weight: Option<f32>,
    left_release_active: bool,
    right_release_active: bool,
    left_release_target: Option<Vec3>,
    right_release_target: Option<Vec3>,
    pelvis_shift: f32,
    raised_pelvis_shift: f32,
    terrain_blend: f32,
    rig_origin: Option<Vec3>,
    rig_rotation: Option<Quat>,
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
const AIRBORNE_FOOT_ROTATION_SPEED_DEGREES: f32 = 1440.0;
const MAX_RETAINED_PLANT_REACH_CORRECTION: f32 = 0.015;
const PELVIS_CORRECTION_SPEED: f32 = 1.6;
pub(super) const MAX_PELVIS_CORRECTION_STEP: f32 = 0.05;
const TERRAIN_IK_BLEND_SPEED: f32 = 4.0;
const MIN_KNEE_FLEXION: f32 = 20.0_f32.to_radians();
const MIN_TERRAIN_KNEE_FLEXION: f32 = 8.0_f32.to_radians();
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
pub(crate) const SOLE_CONTACT_TOLERANCE_METRES: f32 = 0.012;
const SWING_SOLE_CLEARANCE_METRES: f32 = 0.02;
const ORDINARY_SWING_SOLE_CLEARANCE_METRES: f32 = 0.05;
const SETTLE_STEP_SECONDS: f32 = 0.28;
const SETTLE_STEP_CLEARANCE_METRES: f32 = 0.10;
const SETTLE_CAPTURE_POINT_MARGIN_METRES: f32 = 0.12;
const ASSUMED_COM_HEIGHT_METRES: f32 = 1.0;
const MAX_SETTLE_CAPTURE_SPEED: f32 = 1.1;
const ATTACK_AIRBORNE_LUNGE_MIN_SPEED: f32 = 3.5;
const ATTACK_AIRBORNE_LUNGE_CLEARANCE: f32 = 0.16;
const ATTACK_FLAT_SOLE_CLEARANCE: f32 = 0.01;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LegIkState(LegIkMemory);

#[derive(Debug, Clone, Copy, Default)]
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
        LegIkDiagnostics {
            left_authored_target: self.0.left_authored_world_target,
            right_authored_target: self.0.right_authored_world_target,
            left_planned_contact: settle
                .filter(|state| !state.cancelled_by_restart && !state.support_left)
                .map(|state| state.landing_target)
                .or(self.0.left_planned_contact),
            right_planned_contact: settle
                .filter(|state| !state.cancelled_by_restart && state.support_left)
                .map(|state| state.landing_target)
                .or(self.0.right_planned_contact),
            settle_capture_point: settle
                .filter(|state| !state.cancelled_by_restart)
                .map(|state| state.capture_point),
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
    initialized: bool,
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
    pub(crate) left_solve_target: Option<Vec3>,
    pub(crate) right_solve_target: Option<Vec3>,
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
    swing_left: bool,
    last_origin: Vec3,
    last_rotation: Quat,
    start_rotation: Quat,
    swing_start: Vec3,
    swing_end: Vec3,
    left_stance_local: Vec3,
    right_stance_local: Vec3,
    swing_end_local: Vec3,
    airborne_lunge: bool,
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
}

impl Default for RaisedFootworkState {
    fn default() -> Self {
        Self {
            initialized: false,
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
            left_solve_target: None,
            right_solve_target: None,
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
        let raised_guard_follower = raised_footwork_posture_is_valid(skeleton)
            && skeleton.weapon_guard() == WeaponGuardState::Raised
            && skeleton.action_kind() == SkeletonAction::None
            && skeleton.raised_locomotion().is_moving();
        let raised_footwork_was_active = raised_states
            .get(owner)
            .is_ok_and(|state| state.initialized);
        let raised_footwork_handoff = !raised_guard_follower && raised_footwork_was_active;
        let (mut left_weight, mut right_weight) = locomotion_support_weights(skeleton);
        let attack_step_active = skeleton.action_kind() == SkeletonAction::Attack;
        if !attack_step_active && let Ok(mut attack) = attack_states.get_mut(owner) {
            *attack = AttackFootworkState::default();
        }
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
        if evaluation_advances {
            clear_slope_rotation_cache(&mut memory);
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
            let owner_discontinuous = memory.rig_origin.is_some_and(|previous| {
                previous.distance(rig_origin) > MAX_OWNER_TRANSLATION_PER_TICK
            }) || memory.rig_rotation.is_some_and(|previous| {
                previous.angle_between(rig_rotation).to_degrees()
                    > MAX_OWNER_ROTATION_PER_TICK_DEGREES
            });
            if owner_discontinuous {
                memory.left_foot_plant = None;
                memory.right_foot_plant = None;
                memory.left_foot_target = None;
                memory.right_foot_target = None;
                memory.left_foot_world_target = None;
                memory.right_foot_world_target = None;
                memory.left_authored_world_target = None;
                memory.right_authored_world_target = None;
                memory.left_support_weight = None;
                memory.right_support_weight = None;
                memory.left_terrain_pole_world = None;
                memory.right_terrain_pole_world = None;
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
                    memory.left_foot_world_target,
                    memory.right_foot_world_target,
                    projected_com,
                    direction,
                );
                let swing_start = if support_left {
                    memory.right_foot_world_target
                } else {
                    memory.left_foot_world_target
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
                memory.settle = Some(LocomotionSettleState {
                    support_left,
                    swing_start,
                    capture_point,
                    landing_target,
                    progress: 0.0,
                    elapsed_seconds: 0.0,
                    cancelled_by_restart: false,
                    raised_handoff: raised_footwork_handoff,
                });
            }
        }
        if ordinary_lowered
            && skeleton.animation_speed() > 0.05
            && let Some(settle) = memory.settle.as_mut()
        {
            settle.cancelled_by_restart = true;
        }
        let mut settle_ready_for_contact = false;
        if let Some(mut settle) = memory.settle {
            if state_delta_seconds > 0.0 {
                settle = advance_settle_state(settle, state_delta_seconds);
            }
            settle_ready_for_contact = settle.progress >= 1.0;
            if !settle.cancelled_by_restart {
                if settle.support_left {
                    left_weight = 1.0;
                    right_weight = 0.0;
                } else {
                    left_weight = 0.0;
                    right_weight = 1.0;
                }
                legs[0].3 = left_weight;
                legs[1].3 = right_weight;
            }
            memory.settle = Some(settle);
        }
        let desired_raised_pelvis_shift = if raised_guard_follower || attack_step_active {
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
        if attack_step_active {
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
                    left_solve_target: None,
                    right_solve_target: None,
                };
            } else if advances && sequence_delta == 1 {
                if footwork.swing_left {
                    footwork.left_plant = footwork.left_solve_target.unwrap_or(footwork.swing_end);
                } else {
                    footwork.right_plant =
                        footwork.right_solve_target.unwrap_or(footwork.swing_end);
                }
                footwork.half_step = half_step;
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
            let step_progress = (phase * 2.0).fract();
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
            if footwork.swing_left {
                left_target = swing_target;
            } else {
                right_target = swing_target;
            }

            let mut airborne_orientation_owned = [true; 2];
            for (leg_index, (upper, lower, foot, target, left, support)) in [
                (
                    left_upper,
                    left_lower,
                    left_foot,
                    left_target,
                    true,
                    !footwork.swing_left,
                ),
                (
                    right_upper,
                    right_lower,
                    right_foot,
                    right_target,
                    false,
                    footwork.swing_left,
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
                    memory.left_leg
                } else {
                    memory.right_leg
                };
                let canonical_pole = canonical_knee_pole(side);
                let remembered = remembered.filter(|pole| pole.dot(canonical_pole) > 0.2);
                let pole = pole_to_world(rig_rotation, remembered.unwrap_or(canonical_pole));
                if let Some(solution) = solve_two_bone_with_reach(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_snapshot.global.translation(),
                    target,
                    upper_length,
                    lower_length,
                    pole,
                    (upper_length + lower_length) * 0.999,
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
                        } else {
                            memory.right_leg = Some(pole_to_owner(rig_rotation, valid));
                        }
                    }
                }
                let rendered_ankle = snapshot(foot, &parents, &transforms.p0())
                    .map(|rendered| rendered.global.translation());
                let reported_support = rendered_ankle.is_some_and(|ankle| {
                    terrain
                        .and_then(|terrain| terrain.height_at(ankle.xz()))
                        .is_some_and(|height| raised_support_is_actual(support, ankle.y, height))
                });
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
                }
            }
            finalize_leg_rotation_chains(
                rig,
                &mut memory,
                evaluation_advances,
                state_delta_seconds,
                airborne_orientation_owned,
                &parents,
                &mut transforms,
            );
            // Classify support and retain handoff targets only after the final
            // cached-chain/orientation seam. This is the same local-transform
            // state that transform propagation exposes to viewer telemetry.
            for (foot, left, nominal_support) in [
                (left_foot, true, !footwork.swing_left),
                (right_foot, false, footwork.swing_left),
            ] {
                let Some(rendered) = snapshot(foot, &parents, &transforms.p0()) else {
                    continue;
                };
                let ankle = rendered.global.translation();
                let reported_support = terrain
                    .and_then(|terrain| terrain.height_at(ankle.xz()))
                    .is_some_and(|height| {
                        raised_support_is_actual(nominal_support, ankle.y, height)
                    });
                if left {
                    footwork.left_solve_target = Some(ankle);
                    memory.left_foot_world_target = Some(ankle);
                    memory.left_support_weight = Some(reported_support as u8 as f32);
                } else {
                    footwork.right_solve_target = Some(ankle);
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
            memory.left_foot_target = None;
            memory.right_foot_target = None;
            memory.left_foot_world_target = None;
            memory.right_foot_world_target = None;
            memory.left_authored_world_target = None;
            memory.right_authored_world_target = None;
            memory.left_support_weight = None;
            memory.right_support_weight = None;
            memory.left_terrain_pole_world = None;
            memory.right_terrain_pole_world = None;
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
            let Some(plant) = plant else { continue };
            // A remembered plant is world-space. Do not reproject it through
            // the rotating/moving anatomical corridor every frame: that made
            // a visibly planted foot skate during turns. New contacts are
            // constrained when acquired, and reach limiting below remains the
            // only reason an established plant may yield.
            let horizontal_target = plant;
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
            let horizontal_distance = (horizontal_target - upper_snapshot.global.translation())
                .xz()
                .length();
            let maximum_vertical = (reach * reach - horizontal_distance * horizontal_distance)
                .max(0.0)
                .sqrt();
            let reach_shift = target_y + maximum_vertical - upper_snapshot.global.translation().y;
            desired_hip_shift = desired_hip_shift.min((reach_shift * weight).clamp(-0.25, 0.0));
        }
        desired_hip_shift *= terrain_blend;
        // Couple both legs through one bounded, continuous pelvis correction.
        // The authored pose is restored each frame, so this retained scalar is
        // the only temporal state and cannot accumulate transform drift.
        if memory_was_missing {
            memory.pelvis_shift = desired_hip_shift;
        } else if state_delta_seconds > 0.0 {
            memory.pelvis_shift =
                advance_pelvis_shift(memory.pelvis_shift, desired_hip_shift, state_delta_seconds);
        }
        let hip_shift = memory.pelvis_shift;
        if hip_shift < -0.001
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
                        .transform_vector3(Vec3::Y * hip_shift)
                })
                .unwrap_or(Vec3::Y * hip_shift);
            if local_delta.is_finite()
                && let Ok(mut transform) = transforms.p1().get_mut(pelvis)
            {
                transform.translation += local_delta;
            }
        }
        let mut airborne_orientation_owned = [false; 2];
        for (leg_index, (upper_role, lower_role, foot_role, weight, left)) in
            legs.into_iter().enumerate()
        {
            let mut weight = weight;
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
            if terrain_leg_has_support(weight)
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
                }
            }
            if weight <= 0.05
                || plant.is_some_and(|position| !plant_is_continuous(position, foot_position))
            {
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
                    let phase_to_contact = phase_to_next_contact(skeleton.gait_phase, left);
                    let retained_contact = if left {
                        memory.left_planned_contact
                    } else {
                        memory.right_planned_contact
                    };
                    let planned_contact = (phase_to_contact <= 0.12)
                        .then(|| {
                            retained_contact.unwrap_or_else(|| {
                                ordinary_contact_target(
                                    rig_origin,
                                    rig_rotation,
                                    projected_body_center(rig, &transforms.p0())
                                        .unwrap_or(rig_origin),
                                    planar_velocity,
                                    skeleton.animation_speed(),
                                    phase_to_contact,
                                    side,
                                )
                            })
                        })
                        .filter(|_| ordinary_lowered);
                    if left {
                        memory.left_planned_contact = planned_contact;
                    } else {
                        memory.right_planned_contact = planned_contact;
                    }
                    let desired_target = planned_contact.map_or(foot_position, |mut contact| {
                        if let Some(height) = terrain.height_at(contact.xz()) {
                            contact.y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
                        }
                        foot_position.lerp(contact, smoothstep(0.12, 0.0, phase_to_contact))
                    });
                    let desired_owner_target =
                        rig_rotation.inverse() * (desired_target - rig_origin);
                    let (previous_owner_target, previous_support, was_releasing, previous_goal) =
                        if left {
                            (
                                memory.left_foot_target,
                                memory.left_support_weight,
                                memory.left_release_active,
                                memory.left_release_target,
                            )
                        } else {
                            (
                                memory.right_foot_target,
                                memory.right_support_weight,
                                memory.right_release_active,
                                memory.right_release_target,
                            )
                        };
                    // Support loss releases in owner space at a bounded speed.
                    // This remains a purely airborne solve: there is no plant,
                    // terrain projection, or clearance floor. Once converged,
                    // authored FK owns the swing again until final acquisition.
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
                    let next_release_goal = if reached_goal
                        && owner_target.distance_squared(desired_owner_target) > 0.000001
                    {
                        Some(desired_owner_target)
                    } else if reached_goal {
                        None
                    } else {
                        Some(release_goal)
                    };
                    let release_active = next_release_goal.is_some();
                    let target = rig_origin + rig_rotation * owner_target;
                    let canonical_world = pole_to_world(rig_rotation, canonical_knee_pole(side));
                    let pole = (if left {
                        memory.left_terrain_pole_world
                    } else {
                        memory.right_terrain_pole_world
                    })
                    .filter(|pole| pole.dot(canonical_world) > 0.2)
                    .unwrap_or(canonical_world);
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
                        apply_two_bone_solution(
                            upper,
                            lower,
                            foot,
                            solution,
                            &parents,
                            &mut transforms,
                        );
                    }
                    if left {
                        memory.left_foot_plant = None;
                        memory.left_foot_target = Some(owner_target);
                        memory.left_foot_world_target = Some(target);
                        memory.left_support_weight = Some(0.0);
                        memory.left_release_active = release_active;
                        memory.left_release_target = next_release_goal;
                    } else {
                        memory.right_foot_plant = None;
                        memory.right_foot_target = Some(owner_target);
                        memory.right_foot_world_target = Some(target);
                        memory.right_support_weight = Some(0.0);
                        memory.right_release_active = release_active;
                        memory.right_release_target = next_release_goal;
                    }
                    continue;
                }
                let settle = settle_swing.expect("settle swing was checked above");
                let mut desired_target = if settle.cancelled_by_restart {
                    foot_position
                } else {
                    settle_swing_target(settle.swing_start, settle.landing_target, settle.progress)
                };
                if !settle.cancelled_by_restart
                    && let Some(height) = terrain.height_at(desired_target.xz())
                {
                    let minimum_ankle_y = height
                        + MEASURED_ANKLE_SOLE_OFFSET_METRES
                        + SWING_SOLE_CLEARANCE_METRES * (1.0 - settle.progress);
                    desired_target.y = desired_target
                        .y
                        .max(foot_position.y.lerp(minimum_ankle_y, terrain_blend));
                }
                let desired_owner_target = rig_rotation.inverse() * (desired_target - rig_origin);
                let previous_owner_target = if left {
                    memory.left_foot_target
                } else {
                    memory.right_foot_target
                };
                let owner_target = advance_foot_target_at_speed(
                    previous_owner_target,
                    desired_owner_target,
                    state_delta_seconds,
                    settle_target_speed(settle),
                );
                let release_active = owner_target.distance_squared(desired_owner_target) > 0.000001;
                // `desired_target` already includes its terrain-clearance
                // requirement. Keep this exact rate-limited point in memory;
                // projecting Y after the cap would make the visible target
                // differ from the point used as next frame's starting state.
                let target = rig_origin + rig_rotation * owner_target;
                let canonical_pole = canonical_knee_pole(side);
                let canonical_world = pole_to_world(rig_rotation, canonical_pole);
                let remembered = if left {
                    memory.left_terrain_pole_world
                } else {
                    memory.right_terrain_pole_world
                }
                .filter(|pole| pole.dot(canonical_world) > 0.2);
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
                    settle_contact_reached = !settle.cancelled_by_restart
                        && settle.progress >= 1.0
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
                }
                if left {
                    memory.left_foot_plant = None;
                    memory.left_foot_target = Some(owner_target);
                    memory.left_foot_world_target = Some(target);
                    memory.left_support_weight = Some(0.0);
                    memory.left_release_active = release_active;
                    memory.left_release_target = release_active.then_some(desired_owner_target);
                } else {
                    memory.right_foot_plant = None;
                    memory.right_foot_target = Some(owner_target);
                    memory.right_foot_world_target = Some(target);
                    memory.right_support_weight = Some(0.0);
                    memory.right_release_active = release_active;
                    memory.right_release_target = release_active.then_some(desired_owner_target);
                }
                continue;
            }
            // Do not memorize a footprint while the swing foot is merely
            // approaching the ground. Capturing that stale position early
            // makes the pelvis outrun it, forcing the reach limiter to drag a
            // fully weighted foot and drive the knee toward extension.
            if left {
                memory.left_planned_contact = None;
            } else {
                memory.right_planned_contact = None;
            }
            let ordinary_planned_contact = (ordinary_lowered
                && skeleton.animation_speed() > 0.05
                && planar_velocity.length_squared() > 0.0025)
                .then(|| {
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
                });
            if plant.is_none()
                && let Some(planned_contact) = ordinary_planned_contact
            {
                // Freeze the next contact as soon as the contact ramp begins.
                // Recomputing it from the advancing COM every tick would make
                // a nominally supported foot chase the body instead of land.
                plant = Some(constrain_foot_to_track(
                    planned_contact,
                    rig_origin,
                    rig_rotation,
                    side,
                ));
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
            // A turning or advancing pelvis can make an otherwise valid plant
            // unreachable before its support weight releases. Slide that
            // target only as far as anatomical reach requires instead of
            // dropping and reacquiring it in one frame. Re-store the adjusted
            // target so successive turns follow the side corridor continuously.
            planted_target = constrain_target_to_reach(
                planted_target,
                upper_snapshot.global.translation(),
                terrain_maximum_reach(upper_length, lower_length),
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
            plant = plant.map(|_| horizontal_target);
            if left {
                memory.left_foot_plant = plant;
            } else {
                memory.right_foot_plant = plant;
            }
            // Sparse authored locomotion poses can move the swing foot much
            // farther than one rendered frame should permit when support is
            // released. Follow that desired pose at a bounded velocity so the
            // final IK target cannot teleport, while still converging all the
            // way back to the unconstrained authored swing during flight.
            // Keep the foot fully pinned throughout the viewer's supported
            // interval, then ease it into authored swing. Blending directly by
            // the raw support weight began dragging a nominally planted foot
            // as soon as confidence dipped below one.
            let solve_weight = smoothstep(0.05, 0.9, weight) * terrain_blend;
            let mut desired_target = foot_position.lerp(planted_target, solve_weight);
            // An unloaded sparse swing pose can dip below uneven terrain,
            // especially when the forward gait is reused in reverse. Preserve
            // exact stance contact while giving the free foot a small
            // support-weighted clearance floor.
            if let Some(height) = terrain.height_at(desired_target.xz()) {
                desired_target.y = desired_target.y.max(
                    height
                        + MEASURED_ANKLE_SOLE_OFFSET_METRES
                        + ORDINARY_SWING_SOLE_CLEARANCE_METRES * (1.0 - solve_weight),
                );
            }
            let desired_owner_target = rig_rotation.inverse() * (desired_target - rig_origin);
            let release_target_speed = memory
                .settle
                .map(settle_target_speed)
                .unwrap_or(AIRBORNE_RELEASE_TARGET_SPEED);
            let (previous_owner_target, previous_support, mut release_active) = if left {
                (
                    memory.left_foot_target,
                    memory.left_support_weight,
                    memory.left_release_active,
                )
            } else {
                (
                    memory.right_foot_target,
                    memory.right_support_weight,
                    memory.right_release_active,
                )
            };
            if let Some(previous_support) = previous_support {
                if weight + 0.001 < previous_support {
                    release_active = true;
                } else if weight > previous_support + 0.001 {
                    // Normal contact acquisition is already close enough to
                    // lock in one tick. A hard stop can instead change both
                    // support weights from zero to one while the authored idle
                    // foot is far away; keep that exceptional acquisition
                    // bounded rather than teleporting to the new plant.
                    let maximum_step = release_target_speed * state_delta_seconds.max(0.0);
                    release_active = previous_owner_target.is_some_and(|previous| {
                        previous.distance(desired_owner_target) > maximum_step + 0.001
                    });
                }
            }
            let maximum_step = release_target_speed * state_delta_seconds.max(0.0);
            if previous_owner_target.is_some_and(|previous| {
                previous.distance(desired_owner_target) > maximum_step + 0.001
            }) {
                // Reach correction can move a nominally planted target when a
                // sharp turn carries the hip past it. Bound that correction
                // just like a sparse authored swing or hard-stop acquisition.
                release_active = true;
            }
            let owner_target = if release_active {
                advance_foot_target_at_speed(
                    previous_owner_target,
                    desired_owner_target,
                    state_delta_seconds,
                    release_target_speed,
                )
            } else {
                desired_owner_target
            };
            if owner_target.distance_squared(desired_owner_target) <= 0.000001 {
                release_active = false;
            }
            if left {
                memory.left_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.left_support_weight.is_none() {
                    memory.left_support_weight = Some(weight);
                }
                memory.left_release_active = release_active;
                memory.left_release_target = release_active.then_some(desired_owner_target);
            } else {
                memory.right_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.right_support_weight.is_none() {
                    memory.right_support_weight = Some(weight);
                }
                memory.right_release_active = release_active;
                memory.right_release_target = release_active.then_some(desired_owner_target);
            }
            // Terrain clearance was folded into `desired_owner_target` before
            // rate limiting. The retained owner-space point and the world
            // solve target must remain the same point across frame boundaries.
            let target = rig_origin + rig_rotation * owner_target;
            if left {
                memory.left_foot_world_target = Some(target);
            } else {
                memory.right_foot_world_target = Some(target);
            }
            let canonical_pole = canonical_knee_pole(side);
            let canonical_world = pole_to_world(rig_rotation, canonical_pole);
            let remembered = if left {
                memory.left_terrain_pole_world
            } else {
                memory.right_terrain_pole_world
            }
            .filter(|pole| pole.dot(canonical_world) > 0.2);
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
            let solution = if skeleton.posture() == Posture::Crouched {
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
                        && solution.end.xz().distance(horizontal_target.xz()) <= 0.02
                });
                if sole_at_contact {
                    reported_support_weight = weight;
                }
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
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
            &mut memory,
            evaluation_advances,
            state_delta_seconds,
            airborne_orientation_owned,
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
        let restarted_settle_released = memory.settle.is_some_and(|settle| {
            settle.cancelled_by_restart
                && !memory.left_release_active
                && !memory.right_release_active
        });
        if (settle_ready_for_contact && settle_contact_reached)
            || safe_settle_fallback
            || restarted_settle_released
        {
            memory.settle = None;
            memory.recent_movement_velocity = Vec3::ZERO;
        }
        if let Ok(mut state) = ik_states.get_mut(owner) {
            state.0 = memory;
        } else {
            commands.entity(owner).insert(LegIkState(memory));
        }
    }
}

/// Refresh raised-footwork diagnostics from propagated globals. The IK pass
/// runs before transform propagation, while viewer/gameplay consumers observe
/// the propagated hierarchy; twist/intermediate bones can make those positions
/// differ by centimetres near extension.
pub(in crate::animation) fn refresh_raised_support_after_propagation(
    terrain: Query<&SceneTerrain>,
    rigs: Query<(Entity, &HumanoidRig)>,
    globals: Query<&GlobalTransform>,
    mut ik_states: Query<&mut LegIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
) {
    let Some(terrain) = terrain.single().ok() else {
        return;
    };
    for (owner, rig) in &rigs {
        let Ok(mut raised) = raised_states.get_mut(owner) else {
            continue;
        };
        if !raised.initialized {
            continue;
        }
        let Ok(mut state) = ik_states.get_mut(owner) else {
            continue;
        };
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
    let start_tick = skeleton.action_start_tick().unwrap_or_default();
    let step = skeleton.attack_step();
    let start_lead = skeleton.attack_start_lead();
    let swing_left = match step {
        AttackStep::Stay => false,
        // Advancing moves the rear foot; retreating moves the lead foot.
        AttackStep::Forward => start_lead == LeadFoot::Right,
        AttackStep::Backward => start_lead == LeadFoot::Left,
    };
    let mut state = states
        .get_mut(owner)
        .map(|state| *state)
        .unwrap_or_default();
    let phase = skeleton.action_phase().clamp(0.0, 1.0);
    let replacement = !state.initialized
        || state.start_tick != start_tick
        || state.lead != start_lead
        || state.step != step
        || (state.initialized
            && (rig_origin.distance(state.last_origin) > MAX_OWNER_TRANSLATION_PER_TICK
                || rig_rotation.angle_between(state.last_rotation).to_degrees()
                    > MAX_OWNER_ROTATION_PER_TICK_DEGREES))
        || phase + 0.001 < state.previous_phase;
    if replacement {
        let swing_start = if swing_left {
            visible_left
        } else {
            visible_right
        };
        let support = if swing_left {
            visible_right
        } else {
            visible_left
        };
        let stance_local = rig_rotation.inverse() * (swing_start - rig_origin);
        let rig_direction = match step {
            AttackStep::Stay => Vec2::ZERO,
            AttackStep::Forward => Vec2::Y,
            AttackStep::Backward => Vec2::NEG_Y,
        };
        let airborne_lunge = step != AttackStep::Stay
            && skeleton.attack_step_speed() >= ATTACK_AIRBORNE_LUNGE_MIN_SPEED;
        let mut swing_end = if step == AttackStep::Stay {
            swing_start
        } else {
            plan_guard_step_endpoint(
                rig_origin,
                rig_rotation,
                stance_local,
                rig_direction,
                guard_step_length(skeleton.attack_step_speed()),
                swing_left,
                support,
            )
        };
        if step != AttackStep::Stay && !airborne_lunge {
            let travel_direction = skeleton.world_velocity.xz().normalize_or_zero();
            let preparation_seconds =
                skeleton.action_preparation_ticks().unwrap_or(1) as f32 / LOCOMOTION_SAMPLE_HZ;
            let contact_displacement = Vec3::new(travel_direction.x, 0.0, travel_direction.y)
                * skeleton.attack_movement().map_or(0.0, |(_, speed)| speed)
                * preparation_seconds;
            // The controller carries captured velocity through the lunge and
            // stops translating at contact. Recovery therefore happens around
            // this single world-space plant instead of requiring another step.
            // The authored opposite guard already advances the new lead foot;
            // reserve part of controller travel for that authored stance
            // change instead of double-counting it procedurally. Retaining
            // four fifths keeps maximum visible extension at exact contact
            // even when the planted leg yields a few centimetres to reach.
            swing_end += contact_displacement * 0.8;
        }
        let left_stance_local = rig_rotation.inverse() * (visible_left - rig_origin);
        let right_stance_local = rig_rotation.inverse() * (visible_right - rig_origin);
        state = AttackFootworkState {
            initialized: true,
            start_tick,
            lead: start_lead,
            step,
            swing_left,
            last_origin: rig_origin,
            last_rotation: rig_rotation,
            start_rotation: rig_rotation,
            swing_start,
            swing_end,
            left_stance_local,
            right_stance_local,
            swing_end_local: rig_rotation.inverse() * (swing_end - rig_origin),
            airborne_lunge,
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
        };
    }
    let advances = match tick {
        Some(tick) => state.evaluation_tick != Some(tick),
        None => state_delta_seconds > 0.0,
    };
    if advances {
        if state.step != AttackStep::Stay && state.previous_phase < 0.5 && phase >= 0.5 {
            state.support_handoffs = state.support_handoffs.saturating_add(1);
            if state.airborne_lunge {
                // Unsupported flight may deliberately exceed planted reach;
                // landing and recovery own the grounded reach contract.
                state.maximum_reach_yield = 0.0;
            }
        }
        state.previous_phase = phase;
        state.last_origin = rig_origin;
        state.last_rotation = rig_rotation;
    }
    state.evaluation_tick = tick;

    // Bias the swing toward the strike so maximum extension lands on the
    // authored contact frame even if the support leg yields slightly near its
    // reach limit.
    let preparation = smoothstep(0.0, 0.5, phase).powi(2);
    let recovery = smoothstep(0.5, 1.0, phase);
    let mut left_target = if step == AttackStep::Stay {
        state.left_plant
    } else {
        state.left_plant.lerp(left_authored, recovery)
    };
    let mut right_target = if step == AttackStep::Stay {
        state.right_plant
    } else {
        state.right_plant.lerp(right_authored, recovery)
    };
    let mut swing_target = state.swing_start.lerp(state.swing_end, preparation);
    if phase >= 0.5 && !state.airborne_lunge {
        // Contact is the unique maximum extension. From there both feet only
        // remain on the new support plant while the original support foot
        // settles into the opposite guard. No second lunge is planned.
        swing_target = state.swing_end;
        if state
            .start_rotation
            .angle_between(rig_rotation)
            .to_degrees()
            > 45.0
        {
            let authored = if state.swing_left {
                left_authored
            } else {
                right_authored
            };
            // A large facing change cannot preserve the old world plant and
            // also finish in an anatomically coherent guard. Treat the
            // recovery as an in-place pivot of the lunge foot, not a second
            // translational step.
            swing_target = state.swing_end.lerp(authored, recovery);
        }
    }
    if step != AttackStep::Stay {
        if state.swing_left {
            left_target = swing_target;
        } else {
            right_target = swing_target;
        }
    }
    if state.airborne_lunge {
        // A full-speed root can travel farther than one planted leg can
        // reach during the windup. Treat that case as one owner-relative
        // airborne lunge: both feet leave the ground, the selected foot still
        // reaches maximum extension at contact, and both settle continuously
        // into the opposite guard at recovery rather than dragging a stale
        // world plant or snapping when the action ends.
        let left_authored_local = rig_rotation.inverse() * (left_authored - rig_origin);
        let right_authored_local = rig_rotation.inverse() * (right_authored - rig_origin);
        let moving_local = if phase < 0.5 {
            let start = if state.swing_left {
                state.left_stance_local
            } else {
                state.right_stance_local
            };
            start.lerp(state.swing_end_local, preparation)
        } else {
            state.swing_end_local
        };
        let support_local = if state.swing_left {
            state
                .right_stance_local
                .lerp(right_authored_local, recovery)
        } else {
            state.left_stance_local.lerp(left_authored_local, recovery)
        };
        left_target = rig_origin
            + rig_rotation
                * (if state.swing_left {
                    moving_local
                } else {
                    support_local
                });
        right_target = rig_origin
            + rig_rotation
                * (if state.swing_left {
                    support_local
                } else {
                    moving_local
                });
        let clearance = if phase < 0.5 {
            (std::f32::consts::PI * phase * 2.0).sin().max(0.0) * ATTACK_AIRBORNE_LUNGE_CLEARANCE
        } else {
            0.0
        };
        left_target.y += clearance;
        right_target.y += clearance;
    }
    if terrain_enabled && let Some(terrain) = terrain {
        let airborne_clearance = if state.airborne_lunge {
            if phase < 0.5 {
                (std::f32::consts::PI * phase * 2.0).sin().max(0.0)
                    * ATTACK_AIRBORNE_LUNGE_CLEARANCE
            } else {
                0.0
            }
        } else {
            0.0
        };
        left_target =
            terrain_conformed_guard_target(left_target, terrain.height_at(left_target.xz()));
        right_target =
            terrain_conformed_guard_target(right_target, terrain.height_at(right_target.xz()));
        left_target.y += airborne_clearance;
        right_target.y += airborne_clearance;
    } else {
        left_target.y += ATTACK_FLAT_SOLE_CLEARANCE;
        right_target.y += ATTACK_FLAT_SOLE_CLEARANCE;
    }
    if step != AttackStep::Stay && phase < 0.5 && !state.airborne_lunge {
        let lift = (std::f32::consts::PI * preparation).sin() * 0.10;
        if state.swing_left {
            left_target.y += lift;
        } else {
            right_target.y += lift;
        }
    }

    let handoff = if step == AttackStep::Stay {
        0.0
    } else {
        smoothstep(0.45, 0.55, phase)
    };
    let (left_support, right_support) = if step == AttackStep::Stay {
        (1.0, 1.0)
    } else if state.airborne_lunge {
        let launch = 1.0 - smoothstep(0.0, 0.12, phase);
        // Flight ends at the authored strike extent: the lunging foot owns
        // contact support, then the original foot rejoins during recovery.
        let landing = smoothstep(0.44, 0.5, phase);
        let recovery_support = smoothstep(0.88, 1.0, phase);
        if state.swing_left {
            (landing, launch.max(recovery_support))
        } else {
            (launch.max(recovery_support), landing)
        }
    } else if state.swing_left {
        (handoff, 1.0 - handoff)
    } else {
        (1.0 - handoff, handoff)
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
        let target =
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
            memory.left_leg
        } else {
            memory.right_leg
        };
        let canonical = canonical_knee_pole(side);
        let pole = pole_to_world(
            rig_rotation,
            remembered
                .filter(|pole| pole.dot(canonical) > 0.2)
                .unwrap_or(canonical),
        );
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
                } else {
                    memory.right_leg = Some(pole_to_owner(rig_rotation, valid));
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

pub(super) fn settle_swing_target(start: Vec3, landing: Vec3, progress: f32) -> Vec3 {
    let progress = progress.clamp(0.0, 1.0);
    let horizontal = smoothstep(0.0, 1.0, progress);
    let mut target = start.lerp(landing, horizontal);
    target.y += (std::f32::consts::PI * progress).sin() * SETTLE_STEP_CLEARANCE_METRES;
    target
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
        let step = skeleton.attack_step();
        if step == AttackStep::Stay {
            return (1.0, 1.0);
        }
        let swing_left = match step {
            AttackStep::Forward => skeleton.attack_start_lead() == LeadFoot::Right,
            AttackStep::Backward => skeleton.attack_start_lead() == LeadFoot::Left,
            AttackStep::Stay => unreachable!(),
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
        let (left, right) = gait_support_weights(locomotion_profile(skeleton), skeleton.gait_phase);
        (contact_support_weight(left), contact_support_weight(right))
    }
}

fn contact_support_weight(weight: f32) -> f32 {
    if weight < 0.5 {
        0.0
    } else {
        smoothstep(0.5, 1.0, weight)
    }
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

pub(super) fn guard_step_sequence_delta(previous: u32, current: u32) -> u32 {
    current.wrapping_sub(previous)
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

fn finalize_leg_rotation_chains(
    rig: &HumanoidRig,
    memory: &mut LegIkMemory,
    evaluation_advances: bool,
    delta_seconds: f32,
    airborne_orientation_owned: [bool; 2],
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
            let desired_world = foot_snapshot.global.rotation();
            let previous_world = if left {
                memory.left_foot_orientation_world
            } else {
                memory.right_foot_orientation_world
            };
            let contact_blend_active = if left {
                memory.left_contact_orientation_blend_active
            } else {
                memory.right_contact_orientation_blend_active
            };
            let final_world = if airborne_orientation_owned[leg_index] || contact_blend_active {
                let bounded_world = advance_airborne_foot_rotation(
                    previous_world,
                    desired_world,
                    delta_seconds,
                    AIRBORNE_FOOT_ROTATION_SPEED_DEGREES,
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

        assert!((previous.angle_between(advanced).to_degrees() - 22.5).abs() < 0.0001);
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
            cancelled_by_restart: false,
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
}
