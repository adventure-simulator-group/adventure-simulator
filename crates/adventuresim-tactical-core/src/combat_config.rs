use adventuresim_core::{
    body::BodyPart,
    combat::{AutoresolveParameters, CombatResolutionParameters},
};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::physics::{
    TACTICAL_GRAVITY_METRES_PER_SECOND_SQUARED as GRAVITY,
    TACTICAL_MAXIMUM_STEP_HEIGHT_METRES as MAXIMUM_STEP_HEIGHT,
    TACTICAL_MAXIMUM_WALKABLE_SLOPE_DEGREES as MAXIMUM_WALKABLE_SLOPE,
};

mod runtime;
pub use runtime::{
    runtime_animation_config, runtime_combat_presentation_config, runtime_melee_authority_config,
};

pub const TACTICAL_COMBAT_CONFIG_SCHEMA_VERSION: u16 = 5;

#[derive(Clone, Debug, PartialEq, Resource, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalCombatConfig {
    pub schema_version: u16,
    pub resolution: CombatResolutionParameters,
    pub autoresolve: AutoresolveParameters,
    pub realtime_authority: RealtimeAuthorityConfig,
    pub movement: TacticalMovementConfig,
    pub ai: TacticalAiConfig,
    pub client_input: ClientInputConfig,
    pub targeting: TargetingConfig,
    pub presentation: CombatPresentationConfig,
    pub animation: TacticalAnimationConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalAnimationConfig {
    pub locomotion: AnimationLocomotionConfig,
    pub guard_footwork: GuardFootworkConfig,
    pub state_transitions: AnimationStateTransitionConfig,
    pub playback: AnimationPlaybackConfig,
    pub pose_buffer: PoseBufferConfig,
    pub procedural: ProceduralAnimationConfig,
    pub secondary_physics: SecondaryPhysicsConfig,
    pub inverse_kinematics: InverseKinematicsConfig,
    pub full_ragdoll: FullRagdollConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationLocomotionConfig {
    pub sample_hz: f32,
    pub landing: AnimationLandingConfig,
    pub walk: AnimationGaitConfig,
    pub run: AnimationGaitConfig,
    pub raised_guard: AnimationGaitConfig,
    pub prone: AnimationGaitConfig,
    pub supine: AnimationGaitConfig,
    pub blend_speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationLandingConfig {
    pub compression_per_metre_per_second: f32,
    pub minimum_compression_metres: f32,
    pub maximum_compression_metres: f32,
    pub recovery_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationGaitConfig {
    pub reference_speed_metres_per_second: f32,
    pub step_distance_metres: f32,
    pub support_phase_radius: f32,
    pub bounce_metres: f32,
    pub flight_apex_metres: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardFootworkConfig {
    pub default_half_width_metres: f32,
    pub contact_margin_metres: f32,
    pub minimum_step_seconds: f32,
    pub maximum_step_seconds: f32,
    pub planning_reach_metres: f32,
    pub reference_leg_length_metres: f32,
    pub maximum_unsupported_contact_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationStateTransitionConfig {
    pub dive_root_handoff_start_fraction: f32,
    pub downed_facing_sector_half_width: f32,
    pub downed_facing_edge_stickiness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationPlaybackConfig {
    pub frames_per_second: f32,
    pub player_visual_y_offset_metres: f32,
    pub velocity_response_per_second: f32,
    pub phase_correction_rate_per_second: f32,
    pub phase_drift_deadband: f32,
    pub phase_drift_measurement_blend: f32,
    pub phase_snap_error: f32,
    pub maximum_source_gap_ticks: u64,
    pub maximum_authored_step_cadence_per_second: f32,
    pub maximum_authored_stance_slip_metres: f32,
    pub authored_cadence_cap_transition_width: f32,
    pub maximum_coalesced_events: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseBufferConfig {
    pub sample_hz: f32,
    pub inertial_halflife_seconds: f32,
    pub cull_distance_metres: f32,
    pub cull_radius_metres: f32,
    pub authored_contact_plant_limit_metres: f32,
    pub authored_contact_height_fraction: f32,
    pub authored_contact_minimum_height_window_metres: f32,
    pub authored_contact_maximum_height_window_metres: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProceduralAnimationConfig {
    pub guarded_look_joint_limit_degrees: f32,
    pub guarded_look_joint_count: f32,
    pub dive_pelvis_lean_degrees: f32,
    pub height_transition_speed_metres_per_second: f32,
    pub locomotion_stop_height_speed_metres_per_second: f32,
    pub authored_ordinary_passing_rise_metres: f32,
    pub body_response: BodyResponseConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyResponseConfig {
    pub maximum_presentation_sample_gap_ticks: u64,
    pub steady_travel_lean_degrees: f32,
    pub forward_acceleration_lean_degrees: f32,
    pub lateral_acceleration_lean_degrees: f32,
    pub startup_inertial_lean_scale: f32,
    pub sustained_inertial_lean_scale: f32,
    pub degrees_per_second: f32,
    pub smooth_time_seconds: f32,
    pub acceleration_attack_response_per_second: f32,
    pub acceleration_release_response_per_second: f32,
    pub maximum_frame_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecondaryPhysicsConfig {
    pub motor_frequency_hz: f32,
    pub motor_damping_ratio: f32,
    pub maximum_angular_speed_radians_per_second: f32,
    pub impact_angular_speed_per_metre_per_second: f32,
    pub maximum_locomotion_acceleration_metres_per_second_squared: f32,
    pub ragdoll_motor_frequency_hz: f32,
    pub ragdoll_gravity_torque: f32,
    pub weight_response_per_second: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InverseKinematicsConfig {
    pub minimum_inter_foot_separation_metres: f32,
    pub outer_foot_track_metres: f32,
    pub maximum_plant_discontinuity_metres: f32,
    pub maximum_owner_translation_per_tick_metres: f32,
    pub maximum_owner_rotation_per_tick_degrees: f32,
    pub maximum_knee_target_amplification: f32,
    pub maximum_knee_step_metres: f32,
    pub continuity_sample_hz: f32,
    pub run_airborne_owner_step_metres: f32,
    pub run_first_release_owner_step_metres: f32,
    pub airborne_release_step_metres: f32,
    pub raised_settle_pelvis_knee_budget_metres: f32,
    pub raised_settle_step_metres: f32,
    pub guard_reach_pelvis_drop_metres: f32,
    pub stationary_turn_foot_limit_metres: f32,
    pub stationary_turn_step_seconds: f32,
    pub knee_pole_maximum_foot_facing_offset_degrees: f32,
    pub airborne_foot_rotation_speed_degrees_per_second: f32,
    pub first_run_release_foot_rotation_speed_degrees_per_second: f32,
    pub maximum_retained_plant_reach_correction_metres: f32,
    pub pelvis_correction_speed_metres_per_second: f32,
    pub run_pelvis_correction_speed_metres_per_second: f32,
    pub maximum_pelvis_correction_step_metres: f32,
    pub terrain_blend_speed_per_second: f32,
    pub minimum_knee_flexion_degrees: f32,
    pub minimum_terrain_knee_flexion_degrees: f32,
    pub landing_knee_reserve_release_compression_metres: f32,
    pub landing_knee_reserve_full_compression_metres: f32,
    pub measured_ankle_sole_offset_metres: f32,
    pub sole_contact_tolerance_metres: f32,
    pub swing_sole_clearance_metres: f32,
    pub run_swing_sole_clearance_metres: f32,
    pub terrain_transition_flight_toe_clearance_metres: f32,
    pub terrain_contact_toe_clearance_metres: f32,
    pub run_swing_minimum_sole_clearance_metres: f32,
    pub run_contact_approach_phase: f32,
    pub run_contact_chain_settle_phase: f32,
    pub run_maximum_planned_reach_pelvis_drop_metres: f32,
    pub late_run_contact_plan_phase: f32,
    pub maximum_run_swing_root_relative_step_metres: f32,
    pub settle_step_seconds: f32,
    pub settle_step_clearance_metres: f32,
    pub settle_capture_point_margin_metres: f32,
    pub assumed_center_of_mass_height_metres: f32,
    pub maximum_settle_capture_speed_metres_per_second: f32,
    pub maximum_hip_drop_metres: f32,
    pub sole_contact_margin_metres: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullRagdollConfig {
    pub pelvis: RagdollCapsuleConfig,
    pub chest: RagdollCapsuleConfig,
    pub head: RagdollCapsuleConfig,
    pub thigh: RagdollCapsuleConfig,
    pub shin: RagdollCapsuleConfig,
    pub foot: RagdollCapsuleConfig,
    pub upper_arm: RagdollCapsuleConfig,
    pub forearm: RagdollCapsuleConfig,
    pub hand: RagdollCapsuleConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RagdollCapsuleConfig {
    pub radius_metres: f32,
    pub length_metres: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RealtimeAuthorityConfig {
    pub defense: DefenseAuthorityConfig,
    pub melee: MeleeAuthorityConfig,
    pub ranged: RangedAuthorityConfig,
    pub impact: ImpactConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseAuthorityConfig {
    pub reflex_window_seconds: f32,
    pub roll_dodge_effectiveness: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeAuthorityConfig {
    pub replay_cooldown_seconds: f32,
    pub completion_allowance_seconds: f32,
    pub range_latency_tolerance_metres: f32,
    pub windup_jitter_fraction: f32,
    pub maximum_windup_jitter_seconds: f32,
    pub lunge_range_window_metres: f32,
    pub lunge_quickstep_threshold_metres: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangedAuthorityConfig {
    pub cooldown_seconds: f32,
    pub completion_allowance_seconds: f32,
    pub range_latency_tolerance_metres: f32,
    pub aim_half_angle_degrees: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactConfig {
    pub whole_body_velocity_scale: f32,
    pub maximum_velocity_change_metres_per_second: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalMovementConfig {
    pub speeds_metres_per_second: MovementSpeedsConfig,
    pub jump_heights_metres: JumpHeightsConfig,
    pub prone_lateral_speed_scale: f32,
    pub prone_effort_scale: f32,
    pub motor: CharacterMotorConfig,
    pub maneuvers: ManeuverTimingConfig,
}

/// Mechanical parameters for the velocity-based kinematic character motor.
///
/// Locomotion changes velocity through bounded forces. The resulting velocity
/// is then resolved by Ahoy's swept collision constraints; Ahoy does not apply
/// a second horizontal acceleration or friction model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterMotorConfig {
    /// Acceleration due to gravity used by ordinary movement and jump-height
    /// conversion.
    pub gravity_metres_per_second_squared: f32,
    /// Total mass used when a controller has no projected character body.
    pub fallback_character_mass_kg: f32,
    /// Maximum horizontal drive force at the reference leg strength.
    pub reference_ground_drive_force_newtons: f32,
    /// Maximum deliberate braking force at the reference leg strength.
    pub reference_ground_braking_force_newtons: f32,
    /// Peak lateral ground-reaction force for a Strength 3 quickstep,
    /// expressed in multiples of the character's biological body weight.
    pub reference_quickstep_peak_horizontal_force_bodyweights: f32,
    /// Additional peak lateral force, in bodyweights, per point of Strength
    /// above the reference value (and correspondingly less below it).
    pub quickstep_peak_horizontal_force_bodyweights_per_strength: f32,
    /// Supported push duration for an Agility 3 character.
    pub reference_quickstep_push_seconds: f32,
    /// Seconds removed from the supported push for every Agility point above
    /// the reference value (and added below it).
    pub quickstep_push_seconds_reduction_per_agility: f32,
    /// Maximum planar root travel while both quickstep feet remain planted.
    /// Reaching this extension releases support even if force time remains.
    pub quickstep_maximum_supported_root_displacement_metres: f32,
    /// Intended planar displacement for the reference leg length.
    pub reference_quickstep_target_displacement_metres: f32,
    /// Hip-to-ankle chain length for the reference quickstep distance.
    pub reference_quickstep_leg_length_metres: f32,
    /// Normalized authored root displacement at frames 0, 3, 6, 9, and 12.
    /// Runtime clips retain the poses but remove this lateral translation.
    pub quickstep_authored_displacement_profile: [f32; 5],
    /// Upward angle of the propulsive quickstep force above horizontal. The
    /// separate baseline normal force supports body weight while planted.
    pub quickstep_takeoff_angle_degrees: f32,
    /// Reference strength corresponding to the configured forces.
    pub reference_leg_strength: f32,
    /// Reference agility corresponding to the configured lateral control.
    pub reference_leg_agility: f32,
    /// Sustained lateral acceleration available at the reference agility,
    /// expressed as a multiple of gravity.
    pub reference_lateral_acceleration_gravities: f32,
    /// Full-sprint turn radius at the reference Agility 3 anchor. Slower
    /// movement turns more tightly through the shared centripetal model.
    pub reference_sprint_turn_radius_metres: f32,
    /// Full-sprint turn radius for Agility 1, the valid attribute minimum.
    pub agility_one_sprint_turn_radius_metres: f32,
    /// Full-sprint turn radius for Agility 5, the valid attribute maximum.
    pub agility_five_sprint_turn_radius_metres: f32,
    /// Upper bound on ground force imposed by available traction, expressed as
    /// a multiple of normal force.
    pub traction_coefficient: f32,
    /// Kinetic drag on a grounded body slide, expressed as a multiple of
    /// normal force. It begins only at the authored body-contact point.
    pub slide_drag_coefficient: f32,
    /// Fraction of ground drive force available without support.
    pub air_control_force_scale: f32,
    /// Maximum vertical obstacle height treated as a deliberate step.
    pub maximum_step_height_metres: f32,
    /// Steepest surface that can supply ordinary standing support.
    pub maximum_walkable_slope_degrees: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementSpeedsConfig {
    pub walk: f32,
    pub run: f32,
    pub raised_guard: f32,
    pub prone_walk: f32,
    pub prone: f32,
    pub roll: f32,
    pub dive: f32,
    pub quickstep: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JumpHeightsConfig {
    pub ordinary: f32,
    pub dive: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManeuverTimingConfig {
    pub get_up_seconds: f32,
    pub roll_seconds: f32,
    pub dive_seconds: f32,
    pub backward_dive_seconds: f32,
    /// Full sprint-slide duration. Its authored midpoint is body contact;
    /// only the second half is subject to slide drag.
    pub slide_seconds: f32,
    /// Complete input-to-recovery quickstep duration. `ActionTimeline` places
    /// semantic contact halfway through this interval.
    pub quickstep_duration_seconds: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalAiConfig {
    pub ordinary: OrdinaryAiConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryAiConfig {
    pub offense: AiOffenseConfig,
    pub defense: AiDefenseConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiOffenseConfig {
    pub hit_precision: f32,
    pub target_body_part: BodyPart,
    pub windup_seconds: f32,
    pub cooldown_seconds: f32,
    pub ranged_standoff_min_metres: f32,
    pub ranged_standoff_max_metres: f32,
    pub ranged_standoff_slop_metres: f32,
    pub ranged_reach_fraction: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiDefenseConfig {
    pub parry_chance: f64,
    pub dodge_chance: f64,
    pub reaction_delay_min_seconds: f32,
    pub reaction_delay_max_seconds: f32,
    pub frontal_flanking_max: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientInputConfig {
    pub alternate_attack_hold_seconds: f32,
    pub sprint_hold_seconds: f32,
    pub movement_deadzone: f32,
    pub roll_deadzone: f32,
    pub quickstep_threshold: f32,
    pub aim_trigger_threshold: f32,
    pub gamepad_look_scale: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetingConfig {
    pub reported_hit_precision: f32,
    pub body_part_hitboxes: Vec<BodyPartHitboxConfig>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyPartHitboxConfig {
    pub body_part: BodyPart,
    pub center_metres: [f32; 3],
    pub half_extents_metres: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPresentationConfig {
    pub block_seconds: f32,
    pub dodge_seconds: f32,
    pub body_turn_seconds_per_half_turn: f32,
    pub downed_turn_radians_per_second: f32,
    pub attack_curve: AttackCurveConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackCurveConfig {
    pub inertia_characteristic: f32,
    pub inertia_weight: f32,
    pub skill_weight: f32,
    pub tell_base: f32,
    pub tell_span: f32,
    pub drawback_base: f32,
    pub drawback_span: f32,
    pub follow_through_base: f32,
    pub follow_through_span: f32,
    pub overshoot_base: f32,
    pub overshoot_span: f32,
    pub maximum_drawback: f32,
    pub maximum_overshoot: f32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TacticalCombatConfigError {
    #[error("incompatible tactical combat config schema version")]
    SchemaVersion,
    #[error("invalid tactical combat config: {0}")]
    Validation(&'static str),
    #[error("could not serialize validated tactical combat config: {0}")]
    Serialization(String),
}

impl From<&'static str> for TacticalCombatConfigError {
    fn from(message: &'static str) -> Self {
        Self::Validation(message)
    }
}

fn validate_schema_version(schema_version: u16) -> Result<(), TacticalCombatConfigError> {
    (schema_version == TACTICAL_COMBAT_CONFIG_SCHEMA_VERSION)
        .then_some(())
        .ok_or(TacticalCombatConfigError::SchemaVersion)
}

impl TacticalCombatConfig {
    pub fn validate(&self) -> Result<(), TacticalCombatConfigError> {
        validate_schema_version(self.schema_version)?;
        let finite_nonnegative = |value: f32| value.is_finite() && value >= 0.0;
        self.resolution.validate()?;
        self.autoresolve.validate()?;
        let authority = &self.realtime_authority;
        let authority_values = [
            authority.defense.reflex_window_seconds,
            authority.defense.roll_dodge_effectiveness,
            authority.melee.replay_cooldown_seconds,
            authority.melee.completion_allowance_seconds,
            authority.melee.range_latency_tolerance_metres,
            authority.melee.windup_jitter_fraction,
            authority.melee.maximum_windup_jitter_seconds,
            authority.melee.lunge_range_window_metres,
            authority.melee.lunge_quickstep_threshold_metres,
            authority.ranged.cooldown_seconds,
            authority.ranged.completion_allowance_seconds,
            authority.ranged.range_latency_tolerance_metres,
            authority.ranged.aim_half_angle_degrees,
            authority.impact.whole_body_velocity_scale,
            authority.impact.maximum_velocity_change_metres_per_second,
        ];
        if !authority_values.into_iter().all(finite_nonnegative) {
            return Err(TacticalCombatConfigError::Validation(
                "authority values must be finite and non-negative",
            ));
        }
        if authority.defense.roll_dodge_effectiveness > 1.0
            || authority.melee.windup_jitter_fraction > 1.0
            || authority.ranged.aim_half_angle_degrees > 180.0
            || authority.melee.completion_allowance_seconds > 5.0
            || authority.ranged.completion_allowance_seconds > 5.0
        {
            return Err(TacticalCombatConfigError::Validation(
                "authority values exceed compiled safety bounds",
            ));
        }
        let movement = &self.movement;
        let movement_values = [
            movement.speeds_metres_per_second.walk,
            movement.speeds_metres_per_second.run,
            movement.speeds_metres_per_second.raised_guard,
            movement.speeds_metres_per_second.prone_walk,
            movement.speeds_metres_per_second.prone,
            movement.speeds_metres_per_second.roll,
            movement.speeds_metres_per_second.dive,
            movement.speeds_metres_per_second.quickstep,
            movement.jump_heights_metres.ordinary,
            movement.jump_heights_metres.dive,
            movement.prone_lateral_speed_scale,
            movement.prone_effort_scale,
            movement.motor.gravity_metres_per_second_squared,
            movement.motor.fallback_character_mass_kg,
            movement.motor.reference_ground_drive_force_newtons,
            movement.motor.reference_ground_braking_force_newtons,
            movement
                .motor
                .reference_quickstep_peak_horizontal_force_bodyweights,
            movement
                .motor
                .quickstep_peak_horizontal_force_bodyweights_per_strength,
            movement.motor.reference_quickstep_push_seconds,
            movement.motor.quickstep_push_seconds_reduction_per_agility,
            movement
                .motor
                .quickstep_maximum_supported_root_displacement_metres,
            movement
                .motor
                .reference_quickstep_target_displacement_metres,
            movement.motor.reference_quickstep_leg_length_metres,
            movement.motor.quickstep_takeoff_angle_degrees,
            movement.motor.reference_leg_strength,
            movement.motor.reference_leg_agility,
            movement.motor.reference_lateral_acceleration_gravities,
            movement.motor.reference_sprint_turn_radius_metres,
            movement.motor.agility_one_sprint_turn_radius_metres,
            movement.motor.agility_five_sprint_turn_radius_metres,
            movement.motor.traction_coefficient,
            movement.motor.slide_drag_coefficient,
            movement.motor.maximum_step_height_metres,
            movement.motor.maximum_walkable_slope_degrees,
            movement.maneuvers.get_up_seconds,
            movement.maneuvers.roll_seconds,
            movement.maneuvers.dive_seconds,
            movement.maneuvers.backward_dive_seconds,
            movement.maneuvers.slide_seconds,
            movement.maneuvers.quickstep_duration_seconds,
        ];
        if !movement_values
            .into_iter()
            .all(|value| finite_nonnegative(value) && value > 0.0)
        {
            return Err(TacticalCombatConfigError::Validation(
                "movement values must be finite and positive",
            ));
        }
        if !finite_nonnegative(movement.motor.air_control_force_scale)
            || movement.motor.air_control_force_scale > 1.0
            || movement.motor.reference_lateral_acceleration_gravities > 2.0
            || movement.motor.traction_coefficient > 2.0
            || movement.motor.slide_drag_coefficient > 2.0
            || movement.motor.quickstep_takeoff_angle_degrees >= 45.0
            || movement.motor.maximum_step_height_metres > 1.0
            || movement.motor.maximum_walkable_slope_degrees >= 90.0
        {
            return Err(TacticalCombatConfigError::Validation(
                "character motor values exceed compiled safety bounds",
            ));
        }
        let quickstep_profile = movement.motor.quickstep_authored_displacement_profile;
        if quickstep_profile[0].abs() > 1.0e-6
            || (quickstep_profile[4] - 1.0).abs() > 1.0e-6
            || !quickstep_profile
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            || !quickstep_profile.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(TacticalCombatConfigError::Validation(
                "quickstep authored displacement profile must increase from zero to one",
            ));
        }
        let low_agility_radius = movement.motor.agility_one_sprint_turn_radius_metres;
        let reference_radius = movement.motor.reference_sprint_turn_radius_metres;
        let high_agility_radius = movement.motor.agility_five_sprint_turn_radius_metres;
        let turn_curve_linear = (high_agility_radius - low_agility_radius) * 0.5;
        let turn_curve_quadratic =
            (high_agility_radius + low_agility_radius) * 0.5 - reference_radius;
        if !(low_agility_radius > reference_radius && reference_radius > high_agility_radius)
            || turn_curve_linear + 2.0 * turn_curve_quadratic.abs() >= 0.0
        {
            return Err(TacticalCombatConfigError::Validation(
                "agility turn radii must define a strictly decreasing quadratic",
            ));
        }

        let ai = &self.ai.ordinary;
        if ![ai.defense.parry_chance, ai.defense.dodge_chance]
            .into_iter()
            .all(|chance| chance.is_finite() && (0.0..=1.0).contains(&chance))
            || ai.defense.parry_chance + ai.defense.dodge_chance > 1.0
        {
            return Err(TacticalCombatConfigError::Validation(
                "AI defense probabilities must be valid and sum to at most one",
            ));
        }
        if !ai.defense.reaction_delay_min_seconds.is_finite()
            || ai.defense.reaction_delay_min_seconds < 0.0
            || ai.defense.reaction_delay_max_seconds < ai.defense.reaction_delay_min_seconds
            || !ai.defense.reaction_delay_max_seconds.is_finite()
            || !ai.offense.ranged_standoff_min_metres.is_finite()
            || ai.offense.ranged_standoff_max_metres < ai.offense.ranged_standoff_min_metres
        {
            return Err(TacticalCombatConfigError::Validation("invalid AI interval"));
        }

        let input = &self.client_input;
        if ![
            input.alternate_attack_hold_seconds,
            input.sprint_hold_seconds,
            input.movement_deadzone,
            input.roll_deadzone,
            input.quickstep_threshold,
            input.aim_trigger_threshold,
        ]
        .into_iter()
        .all(finite_nonnegative)
            || !input.gamepad_look_scale.into_iter().all(f32::is_finite)
            || input.alternate_attack_hold_seconds == 0.0
            || input.sprint_hold_seconds == 0.0
            || input.movement_deadzone > 1.0
            || input.roll_deadzone > 1.0
            || input.quickstep_threshold > 1.0
            || input.aim_trigger_threshold > 1.0
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid client input value",
            ));
        }

        if !self.targeting.reported_hit_precision.is_finite()
            || self.targeting.body_part_hitboxes.len() != 7
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid targeting config",
            ));
        }
        let mut seen = [false; 7];
        for hitbox in &self.targeting.body_part_hitboxes {
            let index = body_part_index(hitbox.body_part);
            if seen[index]
                || !hitbox.center_metres.into_iter().all(f32::is_finite)
                || !hitbox
                    .half_extents_metres
                    .into_iter()
                    .all(|value| value.is_finite() && value > 0.0)
            {
                return Err(TacticalCombatConfigError::Validation(
                    "invalid body-part hitbox",
                ));
            }
            seen[index] = true;
        }

        let presentation = &self.presentation;
        if ![
            presentation.block_seconds,
            presentation.dodge_seconds,
            presentation.body_turn_seconds_per_half_turn,
            presentation.downed_turn_radians_per_second,
        ]
        .into_iter()
        .all(|value| finite_nonnegative(value) && value > 0.0)
        {
            return Err(TacticalCombatConfigError::Validation(
                "presentation timing must be finite and positive",
            ));
        }
        let curve = &presentation.attack_curve;
        if ![
            curve.inertia_characteristic,
            curve.inertia_weight,
            curve.skill_weight,
            curve.tell_base,
            curve.tell_span,
            curve.drawback_base,
            curve.drawback_span,
            curve.follow_through_base,
            curve.follow_through_span,
            curve.overshoot_base,
            curve.overshoot_span,
            curve.maximum_drawback,
            curve.maximum_overshoot,
        ]
        .into_iter()
        .all(finite_nonnegative)
            || curve.inertia_characteristic == 0.0
            || (curve.inertia_weight + curve.skill_weight - 1.0).abs() > 1.0e-6
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid attack curve",
            ));
        }

        let animation = self.animation;
        let locomotion = animation.locomotion;
        let gait_values = [
            locomotion.walk,
            locomotion.run,
            locomotion.raised_guard,
            locomotion.prone,
            locomotion.supine,
        ];
        if !locomotion.sample_hz.is_finite()
            || locomotion.sample_hz <= 0.0
            || !locomotion.blend_speed.is_finite()
            || locomotion.blend_speed <= 0.0
            || gait_values.into_iter().any(|gait| {
                ![
                    gait.reference_speed_metres_per_second,
                    gait.step_distance_metres,
                    gait.support_phase_radius,
                    gait.bounce_metres,
                    gait.flight_apex_metres,
                ]
                .into_iter()
                .all(finite_nonnegative)
                    || gait.reference_speed_metres_per_second == 0.0
                    || gait.step_distance_metres == 0.0
                    || !(0.0..0.5).contains(&gait.support_phase_radius)
            })
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid animation locomotion tuning",
            ));
        }
        let landing = locomotion.landing;
        if ![
            landing.compression_per_metre_per_second,
            landing.minimum_compression_metres,
            landing.maximum_compression_metres,
            landing.recovery_seconds,
        ]
        .into_iter()
        .all(finite_nonnegative)
            || landing.maximum_compression_metres < landing.minimum_compression_metres
            || landing.recovery_seconds == 0.0
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid animation landing tuning",
            ));
        }
        let guard = animation.guard_footwork;
        if ![
            guard.default_half_width_metres,
            guard.contact_margin_metres,
            guard.minimum_step_seconds,
            guard.maximum_step_seconds,
            guard.planning_reach_metres,
            guard.reference_leg_length_metres,
            guard.maximum_unsupported_contact_seconds,
        ]
        .into_iter()
        .all(|value| finite_nonnegative(value) && value > 0.0)
            || guard.maximum_step_seconds < guard.minimum_step_seconds
            || guard.planning_reach_metres <= guard.contact_margin_metres
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid guard footwork tuning",
            ));
        }
        let playback = animation.playback;
        if ![
            playback.frames_per_second,
            playback.velocity_response_per_second,
            playback.phase_correction_rate_per_second,
            playback.phase_drift_deadband,
            playback.phase_drift_measurement_blend,
            playback.phase_snap_error,
            playback.maximum_authored_step_cadence_per_second,
            playback.maximum_authored_stance_slip_metres,
            playback.authored_cadence_cap_transition_width,
        ]
        .into_iter()
        .all(finite_nonnegative)
            || !playback.player_visual_y_offset_metres.is_finite()
            || playback.frames_per_second == 0.0
            || playback.maximum_source_gap_ticks == 0
            || playback.maximum_coalesced_events == 0
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid animation playback tuning",
            ));
        }
        let pose = animation.pose_buffer;
        if ![
            pose.sample_hz,
            pose.inertial_halflife_seconds,
            pose.cull_distance_metres,
            pose.cull_radius_metres,
            pose.authored_contact_plant_limit_metres,
            pose.authored_contact_height_fraction,
            pose.authored_contact_minimum_height_window_metres,
            pose.authored_contact_maximum_height_window_metres,
        ]
        .into_iter()
        .all(|value| finite_nonnegative(value) && value > 0.0)
            || pose.authored_contact_maximum_height_window_metres
                < pose.authored_contact_minimum_height_window_metres
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid pose buffer tuning",
            ));
        }
        let ragdoll = animation.full_ragdoll;
        if [
            ragdoll.pelvis,
            ragdoll.chest,
            ragdoll.head,
            ragdoll.thigh,
            ragdoll.shin,
            ragdoll.foot,
            ragdoll.upper_arm,
            ragdoll.forearm,
            ragdoll.hand,
        ]
        .into_iter()
        .any(|capsule| {
            !capsule.radius_metres.is_finite()
                || !capsule.length_metres.is_finite()
                || capsule.radius_metres <= 0.0
                || capsule.length_metres <= 0.0
        }) {
            return Err(TacticalCombatConfigError::Validation(
                "invalid full-ragdoll capsule tuning",
            ));
        }
        let procedural = animation.procedural;
        let response = procedural.body_response;
        if ![
            procedural.guarded_look_joint_limit_degrees,
            procedural.guarded_look_joint_count,
            procedural.dive_pelvis_lean_degrees,
            procedural.height_transition_speed_metres_per_second,
            procedural.locomotion_stop_height_speed_metres_per_second,
            procedural.authored_ordinary_passing_rise_metres,
            response.steady_travel_lean_degrees,
            response.forward_acceleration_lean_degrees,
            response.lateral_acceleration_lean_degrees,
            response.startup_inertial_lean_scale,
            response.sustained_inertial_lean_scale,
            response.degrees_per_second,
            response.smooth_time_seconds,
            response.acceleration_attack_response_per_second,
            response.acceleration_release_response_per_second,
            response.maximum_frame_seconds,
        ]
        .into_iter()
        .all(finite_nonnegative)
            || procedural.guarded_look_joint_count == 0.0
            || response.maximum_presentation_sample_gap_ticks == 0
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid procedural animation tuning",
            ));
        }
        let secondary = animation.secondary_physics;
        if ![
            secondary.motor_frequency_hz,
            secondary.motor_damping_ratio,
            secondary.maximum_angular_speed_radians_per_second,
            secondary.impact_angular_speed_per_metre_per_second,
            secondary.maximum_locomotion_acceleration_metres_per_second_squared,
            secondary.ragdoll_motor_frequency_hz,
            secondary.ragdoll_gravity_torque,
            secondary.weight_response_per_second,
        ]
        .into_iter()
        .all(finite_nonnegative)
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid secondary animation physics tuning",
            ));
        }
        let ik = animation.inverse_kinematics;
        if ![
            ik.minimum_inter_foot_separation_metres,
            ik.outer_foot_track_metres,
            ik.maximum_plant_discontinuity_metres,
            ik.maximum_owner_translation_per_tick_metres,
            ik.maximum_owner_rotation_per_tick_degrees,
            ik.maximum_knee_target_amplification,
            ik.maximum_knee_step_metres,
            ik.continuity_sample_hz,
            ik.run_airborne_owner_step_metres,
            ik.run_first_release_owner_step_metres,
            ik.airborne_release_step_metres,
            ik.raised_settle_pelvis_knee_budget_metres,
            ik.raised_settle_step_metres,
            ik.guard_reach_pelvis_drop_metres,
            ik.stationary_turn_foot_limit_metres,
            ik.stationary_turn_step_seconds,
            ik.knee_pole_maximum_foot_facing_offset_degrees,
            ik.airborne_foot_rotation_speed_degrees_per_second,
            ik.first_run_release_foot_rotation_speed_degrees_per_second,
            ik.maximum_retained_plant_reach_correction_metres,
            ik.pelvis_correction_speed_metres_per_second,
            ik.run_pelvis_correction_speed_metres_per_second,
            ik.maximum_pelvis_correction_step_metres,
            ik.terrain_blend_speed_per_second,
            ik.minimum_knee_flexion_degrees,
            ik.minimum_terrain_knee_flexion_degrees,
            ik.landing_knee_reserve_release_compression_metres,
            ik.landing_knee_reserve_full_compression_metres,
            ik.measured_ankle_sole_offset_metres,
            ik.sole_contact_tolerance_metres,
            ik.swing_sole_clearance_metres,
            ik.run_swing_sole_clearance_metres,
            ik.terrain_transition_flight_toe_clearance_metres,
            ik.terrain_contact_toe_clearance_metres,
            ik.run_swing_minimum_sole_clearance_metres,
            ik.run_contact_approach_phase,
            ik.run_contact_chain_settle_phase,
            ik.run_maximum_planned_reach_pelvis_drop_metres,
            ik.late_run_contact_plan_phase,
            ik.maximum_run_swing_root_relative_step_metres,
            ik.settle_step_seconds,
            ik.settle_step_clearance_metres,
            ik.settle_capture_point_margin_metres,
            ik.assumed_center_of_mass_height_metres,
            ik.maximum_settle_capture_speed_metres_per_second,
            ik.maximum_hip_drop_metres,
            ik.sole_contact_margin_metres,
        ]
        .into_iter()
        .all(f32::is_finite)
            || ik.minimum_inter_foot_separation_metres <= 0.0
            || ik.outer_foot_track_metres <= ik.minimum_inter_foot_separation_metres * 0.5
            || ik.maximum_knee_target_amplification <= 0.0
            || ik.maximum_knee_step_metres <= 0.0
            || ik.continuity_sample_hz <= 0.0
            || ik.measured_ankle_sole_offset_metres <= 0.0
            || ik.sole_contact_tolerance_metres <= 0.0
            || !(0.0..=1.0).contains(&ik.run_contact_approach_phase)
            || !(0.0..=1.0).contains(&ik.run_contact_chain_settle_phase)
            || !(0.0..=1.0).contains(&ik.late_run_contact_plan_phase)
        {
            return Err(TacticalCombatConfigError::Validation(
                "invalid inverse-kinematics tuning",
            ));
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<String, TacticalCombatConfigError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| TacticalCombatConfigError::Serialization(error.to_string()))?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

const fn body_part_index(part: BodyPart) -> usize {
    match part {
        BodyPart::LeftArm => 0,
        BodyPart::RightArm => 1,
        BodyPart::LeftLeg => 2,
        BodyPart::RightLeg => 3,
        BodyPart::Chest => 4,
        BodyPart::Stomach => 5,
        BodyPart::Head => 6,
    }
}

impl Default for TacticalCombatConfig {
    fn default() -> Self {
        Self {
            schema_version: TACTICAL_COMBAT_CONFIG_SCHEMA_VERSION,
            resolution: adventuresim_core::combat::EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            autoresolve: adventuresim_core::combat::EMBEDDED_AUTORESOLVE_PARAMETERS,
            realtime_authority: RealtimeAuthorityConfig {
                defense: DefenseAuthorityConfig {
                    reflex_window_seconds: 0.5,
                    roll_dodge_effectiveness: 0.35,
                },
                melee: MeleeAuthorityConfig {
                    replay_cooldown_seconds: 0.08,
                    completion_allowance_seconds: 1.0,
                    range_latency_tolerance_metres: 0.25,
                    windup_jitter_fraction: 0.10,
                    maximum_windup_jitter_seconds: 0.025,
                    lunge_range_window_metres: 0.10,
                    lunge_quickstep_threshold_metres: 0.50,
                },
                ranged: RangedAuthorityConfig {
                    cooldown_seconds: 0.6,
                    completion_allowance_seconds: 1.0,
                    range_latency_tolerance_metres: 0.5,
                    aim_half_angle_degrees: 20.0,
                },
                impact: ImpactConfig {
                    whole_body_velocity_scale: 1.93,
                    maximum_velocity_change_metres_per_second: 12.0,
                },
            },
            movement: TacticalMovementConfig {
                speeds_metres_per_second: MovementSpeedsConfig {
                    walk: 1.7,
                    run: 5.5,
                    raised_guard: 2.0,
                    prone_walk: 0.45,
                    prone: 2.0,
                    roll: 1.3,
                    dive: 7.0,
                    quickstep: 3.5,
                },
                jump_heights_metres: JumpHeightsConfig {
                    ordinary: 0.30,
                    dive: 0.20,
                },
                prone_lateral_speed_scale: 0.375,
                prone_effort_scale: 3.0,
                motor: CharacterMotorConfig {
                    gravity_metres_per_second_squared: GRAVITY,
                    fallback_character_mass_kg: 80.0,
                    reference_ground_drive_force_newtons: 1_000.0,
                    reference_ground_braking_force_newtons: 1_200.0,
                    reference_quickstep_peak_horizontal_force_bodyweights: 4.50,
                    quickstep_peak_horizontal_force_bodyweights_per_strength: 0.60,
                    reference_quickstep_push_seconds: 0.40,
                    quickstep_push_seconds_reduction_per_agility: 0.02,
                    quickstep_maximum_supported_root_displacement_metres: 0.25,
                    reference_quickstep_target_displacement_metres: 1.0,
                    reference_quickstep_leg_length_metres: 0.840_348,
                    quickstep_authored_displacement_profile: [
                        0.0,
                        0.202_263_49,
                        0.509_264_1,
                        0.789_125_3,
                        1.0,
                    ],
                    quickstep_takeoff_angle_degrees: 9.0,
                    reference_leg_strength: 3.0,
                    reference_leg_agility: 3.0,
                    reference_lateral_acceleration_gravities: 0.45,
                    reference_sprint_turn_radius_metres: 3.0,
                    agility_one_sprint_turn_radius_metres: 4.5,
                    agility_five_sprint_turn_radius_metres: 2.2,
                    traction_coefficient: 0.9,
                    slide_drag_coefficient: 0.65,
                    air_control_force_scale: 0.08,
                    maximum_step_height_metres: MAXIMUM_STEP_HEIGHT,
                    maximum_walkable_slope_degrees: MAXIMUM_WALKABLE_SLOPE,
                },
                maneuvers: ManeuverTimingConfig {
                    get_up_seconds: 51.0 / 64.0,
                    roll_seconds: 26.0 / 64.0,
                    dive_seconds: 20.0 / 64.0,
                    backward_dive_seconds: 32.0 / 64.0,
                    slide_seconds: 48.0 / 64.0,
                    quickstep_duration_seconds: 0.50,
                },
            },
            ai: TacticalAiConfig {
                ordinary: OrdinaryAiConfig {
                    offense: AiOffenseConfig {
                        hit_precision: 1.0,
                        target_body_part: BodyPart::Chest,
                        windup_seconds: 0.65,
                        cooldown_seconds: 0.25,
                        ranged_standoff_min_metres: 1.5,
                        ranged_standoff_max_metres: 12.0,
                        ranged_standoff_slop_metres: 0.5,
                        ranged_reach_fraction: 0.5,
                    },
                    defense: AiDefenseConfig {
                        parry_chance: 0.2,
                        dodge_chance: 0.2,
                        reaction_delay_min_seconds: 0.20,
                        reaction_delay_max_seconds: 0.27,
                        frontal_flanking_max: 0.01,
                    },
                },
            },
            client_input: ClientInputConfig {
                alternate_attack_hold_seconds: 0.2,
                sprint_hold_seconds: 0.25,
                movement_deadzone: 0.01,
                roll_deadzone: 0.35,
                quickstep_threshold: 0.7,
                aim_trigger_threshold: 0.95,
                gamepad_look_scale: [4.0, -4.0, 4.0],
            },
            targeting: TargetingConfig {
                reported_hit_precision: 1.0,
                body_part_hitboxes: vec![
                    hitbox(BodyPart::Head, [0.0, 0.92, 0.0], [0.27, 0.23, 0.22]),
                    hitbox(BodyPart::Chest, [0.0, 0.49, 0.0], [0.33, 0.23, 0.29]),
                    hitbox(BodyPart::Stomach, [0.0, 0.17, 0.0], [0.25, 0.12, 0.25]),
                    hitbox(BodyPart::LeftArm, [-0.40, 0.25, 0.0], [0.1, 0.5, 0.1]),
                    hitbox(BodyPart::RightArm, [0.40, 0.25, 0.0], [0.1, 0.5, 0.1]),
                    hitbox(BodyPart::LeftLeg, [-0.16, -0.40, 0.0], [0.15, 0.5, 0.15]),
                    hitbox(BodyPart::RightLeg, [0.16, -0.40, 0.0], [0.15, 0.5, 0.15]),
                ],
            },
            presentation: CombatPresentationConfig {
                block_seconds: 8.0 / 64.0,
                dodge_seconds: 20.0 / 64.0,
                body_turn_seconds_per_half_turn: 0.25,
                downed_turn_radians_per_second: std::f32::consts::FRAC_PI_2,
                attack_curve: AttackCurveConfig {
                    inertia_characteristic: 0.45,
                    inertia_weight: 0.55,
                    skill_weight: 0.45,
                    tell_base: 0.32,
                    tell_span: 0.28,
                    drawback_base: 0.16,
                    drawback_span: 0.42,
                    follow_through_base: 0.18,
                    follow_through_span: 0.22,
                    overshoot_base: 0.08,
                    overshoot_span: 0.38,
                    maximum_drawback: 0.65,
                    maximum_overshoot: 0.55,
                },
            },
            animation: TacticalAnimationConfig {
                locomotion: AnimationLocomotionConfig {
                    sample_hz: 64.0,
                    landing: AnimationLandingConfig {
                        compression_per_metre_per_second: 0.012,
                        minimum_compression_metres: 0.04,
                        maximum_compression_metres: 0.08,
                        recovery_seconds: 0.16,
                    },
                    walk: AnimationGaitConfig {
                        reference_speed_metres_per_second: 2.0,
                        step_distance_metres: 1.22,
                        support_phase_radius: 0.28,
                        bounce_metres: 0.04,
                        flight_apex_metres: 0.0,
                    },
                    run: AnimationGaitConfig {
                        reference_speed_metres_per_second: 5.5,
                        step_distance_metres: 1.78,
                        support_phase_radius: 0.175,
                        bounce_metres: 0.0,
                        flight_apex_metres: 0.12,
                    },
                    raised_guard: AnimationGaitConfig {
                        reference_speed_metres_per_second: 2.0,
                        step_distance_metres: 0.38,
                        support_phase_radius: 0.25,
                        bounce_metres: 0.03,
                        flight_apex_metres: 0.0,
                    },
                    prone: AnimationGaitConfig {
                        reference_speed_metres_per_second: 1.0,
                        step_distance_metres: 0.60,
                        support_phase_radius: 0.30,
                        bounce_metres: 0.0,
                        flight_apex_metres: 0.0,
                    },
                    supine: AnimationGaitConfig {
                        reference_speed_metres_per_second: 0.8,
                        step_distance_metres: 1.028,
                        support_phase_radius: 0.30,
                        bounce_metres: 0.0,
                        flight_apex_metres: 0.0,
                    },
                    blend_speed: 0.75,
                },
                guard_footwork: GuardFootworkConfig {
                    default_half_width_metres: 0.15,
                    contact_margin_metres: 0.08,
                    minimum_step_seconds: 0.10,
                    maximum_step_seconds: 0.32,
                    planning_reach_metres: 0.80,
                    reference_leg_length_metres: 0.840_348,
                    maximum_unsupported_contact_seconds: 0.35,
                },
                state_transitions: AnimationStateTransitionConfig {
                    dive_root_handoff_start_fraction: 0.18,
                    downed_facing_sector_half_width: 0.25,
                    downed_facing_edge_stickiness: 1.0 / 18.0,
                },
                playback: AnimationPlaybackConfig {
                    frames_per_second: 30.0,
                    player_visual_y_offset_metres: -0.95,
                    velocity_response_per_second: 18.0,
                    phase_correction_rate_per_second: 0.05,
                    phase_drift_deadband: 0.04,
                    phase_drift_measurement_blend: 0.15,
                    phase_snap_error: 0.20,
                    maximum_source_gap_ticks: 32,
                    maximum_authored_step_cadence_per_second: 5.0,
                    maximum_authored_stance_slip_metres: 0.03,
                    authored_cadence_cap_transition_width: 1.0,
                    maximum_coalesced_events: 8,
                },
                pose_buffer: PoseBufferConfig {
                    sample_hz: 30.0,
                    inertial_halflife_seconds: 0.10,
                    cull_distance_metres: 100.0,
                    cull_radius_metres: 2.0,
                    authored_contact_plant_limit_metres: 0.14,
                    authored_contact_height_fraction: 0.12,
                    authored_contact_minimum_height_window_metres: 0.015,
                    authored_contact_maximum_height_window_metres: 0.05,
                },
                procedural: ProceduralAnimationConfig {
                    guarded_look_joint_limit_degrees: 22.5,
                    guarded_look_joint_count: 3.0,
                    dive_pelvis_lean_degrees: 40.0,
                    height_transition_speed_metres_per_second: 0.4,
                    locomotion_stop_height_speed_metres_per_second: 0.8,
                    authored_ordinary_passing_rise_metres: 0.033,
                    body_response: BodyResponseConfig {
                        maximum_presentation_sample_gap_ticks: 32,
                        steady_travel_lean_degrees: 16.0,
                        forward_acceleration_lean_degrees: 10.0,
                        lateral_acceleration_lean_degrees: 6.4,
                        startup_inertial_lean_scale: 0.25,
                        sustained_inertial_lean_scale: 0.18,
                        degrees_per_second: 128.0,
                        smooth_time_seconds: 0.10,
                        acceleration_attack_response_per_second: 16.0,
                        acceleration_release_response_per_second: 7.0,
                        maximum_frame_seconds: 1.0 / 30.0,
                    },
                },
                secondary_physics: SecondaryPhysicsConfig {
                    motor_frequency_hz: 4.25,
                    motor_damping_ratio: 0.78,
                    maximum_angular_speed_radians_per_second: 18.0,
                    impact_angular_speed_per_metre_per_second: 0.85,
                    maximum_locomotion_acceleration_metres_per_second_squared: 24.0,
                    ragdoll_motor_frequency_hz: 0.7,
                    ragdoll_gravity_torque: 8.0,
                    weight_response_per_second: 12.0,
                },
                inverse_kinematics: InverseKinematicsConfig {
                    minimum_inter_foot_separation_metres: 0.16,
                    outer_foot_track_metres: 0.55,
                    maximum_plant_discontinuity_metres: 2.0,
                    maximum_owner_translation_per_tick_metres: 0.5,
                    maximum_owner_rotation_per_tick_degrees: 120.0,
                    maximum_knee_target_amplification: 2.05,
                    maximum_knee_step_metres: 0.10,
                    continuity_sample_hz: 64.0,
                    run_airborne_owner_step_metres: 0.0875,
                    run_first_release_owner_step_metres: 0.094,
                    airborne_release_step_metres: 0.047_804_88,
                    raised_settle_pelvis_knee_budget_metres: 0.02,
                    raised_settle_step_metres: 0.038_243_9,
                    guard_reach_pelvis_drop_metres: 0.12,
                    stationary_turn_foot_limit_metres: 0.14,
                    stationary_turn_step_seconds: 0.22,
                    knee_pole_maximum_foot_facing_offset_degrees: 22.5,
                    airborne_foot_rotation_speed_degrees_per_second: 576.0,
                    first_run_release_foot_rotation_speed_degrees_per_second: 0.0,
                    maximum_retained_plant_reach_correction_metres: 0.015,
                    pelvis_correction_speed_metres_per_second: 1.2,
                    run_pelvis_correction_speed_metres_per_second: 0.4,
                    maximum_pelvis_correction_step_metres: 0.05,
                    terrain_blend_speed_per_second: 4.0,
                    minimum_knee_flexion_degrees: 20.0,
                    minimum_terrain_knee_flexion_degrees: 12.0,
                    landing_knee_reserve_release_compression_metres: 0.012,
                    landing_knee_reserve_full_compression_metres: 0.04,
                    measured_ankle_sole_offset_metres: 0.085,
                    sole_contact_tolerance_metres: 0.01,
                    swing_sole_clearance_metres: 0.02,
                    run_swing_sole_clearance_metres: 0.08,
                    terrain_transition_flight_toe_clearance_metres: 0.011,
                    terrain_contact_toe_clearance_metres: -0.009,
                    run_swing_minimum_sole_clearance_metres: 0.051,
                    run_contact_approach_phase: 0.95,
                    run_contact_chain_settle_phase: 0.18,
                    run_maximum_planned_reach_pelvis_drop_metres: 0.25,
                    late_run_contact_plan_phase: 0.5,
                    maximum_run_swing_root_relative_step_metres: 0.068,
                    settle_step_seconds: 0.28,
                    settle_step_clearance_metres: 0.10,
                    settle_capture_point_margin_metres: 0.12,
                    assumed_center_of_mass_height_metres: 1.0,
                    maximum_settle_capture_speed_metres_per_second: 1.1,
                    maximum_hip_drop_metres: 0.18,
                    sole_contact_margin_metres: 0.001,
                },
                full_ragdoll: FullRagdollConfig {
                    pelvis: RagdollCapsuleConfig {
                        radius_metres: 0.18,
                        length_metres: 0.24,
                    },
                    chest: RagdollCapsuleConfig {
                        radius_metres: 0.18,
                        length_metres: 0.28,
                    },
                    head: RagdollCapsuleConfig {
                        radius_metres: 0.15,
                        length_metres: 0.16,
                    },
                    thigh: RagdollCapsuleConfig {
                        radius_metres: 0.10,
                        length_metres: 0.36,
                    },
                    shin: RagdollCapsuleConfig {
                        radius_metres: 0.085,
                        length_metres: 0.34,
                    },
                    foot: RagdollCapsuleConfig {
                        radius_metres: 0.09,
                        length_metres: 0.20,
                    },
                    upper_arm: RagdollCapsuleConfig {
                        radius_metres: 0.075,
                        length_metres: 0.27,
                    },
                    forearm: RagdollCapsuleConfig {
                        radius_metres: 0.065,
                        length_metres: 0.25,
                    },
                    hand: RagdollCapsuleConfig {
                        radius_metres: 0.07,
                        length_metres: 0.14,
                    },
                },
            },
        }
    }
}

fn hitbox(
    body_part: BodyPart,
    center_metres: [f32; 3],
    half_extents_metres: [f32; 3],
) -> BodyPartHitboxConfig {
    BodyPartHitboxConfig {
        body_part,
        center_metres,
        half_extents_metres,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_config_is_valid_and_digestible() {
        let config = TacticalCombatConfig::default();
        config.validate().unwrap();
        assert_eq!(config.digest().unwrap().len(), 64);
    }

    #[test]
    fn duplicate_hitbox_is_rejected() {
        let mut config = TacticalCombatConfig::default();
        config.targeting.body_part_hitboxes[1].body_part = BodyPart::Head;
        assert!(config.validate().is_err());
    }

    #[test]
    fn nonphysical_combat_resolution_is_rejected() {
        let mut config = TacticalCombatConfig::default();
        config.resolution.armed_attack_energy_transfer = 0.0;
        assert!(config.validate().is_err());

        let mut config = TacticalCombatConfig::default();
        config.resolution.stagger_resistance_joules_per_kg = f32::INFINITY;
        assert!(config.validate().is_err());
    }

    #[test]
    fn invalid_autoresolve_parameters_are_rejected() {
        let mut config = TacticalCombatConfig::default();
        config.autoresolve.minimum_hit_precision = 1.1;
        assert!(config.validate().is_err());

        let mut config = TacticalCombatConfig::default();
        config.autoresolve.minimum_hit_precision = 0.9;
        config.autoresolve.maximum_hit_precision = 0.8;
        assert!(config.validate().is_err());
    }

    #[test]
    fn nonmonotonic_agility_turn_curve_is_rejected() {
        let mut config = TacticalCombatConfig::default();
        config.movement.motor.agility_one_sprint_turn_radius_metres = 4.0;
        config.movement.motor.agility_five_sprint_turn_radius_metres = 2.9;
        assert!(config.validate().is_err());
    }
}
