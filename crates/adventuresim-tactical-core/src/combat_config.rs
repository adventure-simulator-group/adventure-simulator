use adventuresim_core::body::BodyPart;
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const TACTICAL_COMBAT_CONFIG_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Debug, PartialEq, Resource, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalCombatConfig {
    pub schema_version: u16,
    pub realtime_authority: RealtimeAuthorityConfig,
    pub movement: TacticalMovementConfig,
    pub ai: TacticalAiConfig,
    pub client_input: ClientInputConfig,
    pub targeting: TargetingConfig,
    pub presentation: CombatPresentationConfig,
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

impl TacticalCombatConfig {
    pub fn validate(&self) -> Result<(), TacticalCombatConfigError> {
        if self.schema_version != TACTICAL_COMBAT_CONFIG_SCHEMA_VERSION {
            return Err(TacticalCombatConfigError::SchemaVersion);
        }
        let finite_nonnegative = |value: f32| value.is_finite() && value >= 0.0;
        let authority = &self.realtime_authority;
        let authority_values = [
            authority.defense.reflex_window_seconds,
            authority.defense.roll_dodge_effectiveness,
            authority.melee.replay_cooldown_seconds,
            authority.melee.completion_allowance_seconds,
            authority.melee.range_latency_tolerance_metres,
            authority.melee.windup_jitter_fraction,
            authority.melee.maximum_windup_jitter_seconds,
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
                    walk: 1.4,
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
                    gravity_metres_per_second_squared:
                        crate::physics::TACTICAL_GRAVITY_METRES_PER_SECOND_SQUARED,
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
                    maximum_step_height_metres: crate::physics::TACTICAL_MAXIMUM_STEP_HEIGHT_METRES,
                    maximum_walkable_slope_degrees:
                        crate::physics::TACTICAL_MAXIMUM_WALKABLE_SLOPE_DEGREES,
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
    fn nonmonotonic_agility_turn_curve_is_rejected() {
        let mut config = TacticalCombatConfig::default();
        config.movement.motor.agility_one_sprint_turn_radius_metres = 4.0;
        config.movement.motor.agility_five_sprint_turn_radius_metres = 2.9;
        assert!(config.validate().is_err());
    }
}
