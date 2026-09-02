use super::CombatFatigueParameters;
use serde::{Deserialize, Serialize};

/// Physical tuning projected from the canonical tactical combat configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResolutionParameters {
    pub fatigue: CombatFatigueParameters,
    /// Fraction of the gross muscular estimate delivered through a held weapon.
    pub armed_attack_energy_transfer: f32,
    /// Contact energy per kilogram needed to produce one point of imbalance.
    pub stagger_resistance_joules_per_kg: f32,
}

/// Strategic-only abstractions used around the shared physical resolver.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoresolveParameters {
    pub combat_round_seconds: f32,
    pub formation_spacing_metres: f32,
    pub reference_melee_attack_seconds: f32,
    pub minimum_movement_speed_metres_per_second: f32,
    /// Maximum guarded locomotion speed shared with the tactical controller.
    pub guarded_movement_speed_metres_per_second: f32,
    /// Forward lunge speed used by the tactical attack-movement planner.
    pub melee_lunge_speed_metres_per_second: f32,
    /// Maximum authored forward/quickstep travel available to a melee start.
    pub melee_lunge_maximum_travel_metres: f32,
    /// Authored ground-drive force shared with the tactical character motor.
    pub reference_ground_drive_force_newtons: f32,
    pub reference_leg_strength: f32,
    pub gravity_metres_per_second_squared: f32,
    pub traction_coefficient: f32,
    pub minimum_attack_interval_seconds: f32,
    pub minimum_melee_input_reflex: f32,
    pub melee_windup_seconds: f32,
    pub melee_reaction_delay_min_seconds: f32,
    pub melee_reaction_delay_max_seconds: f32,
    pub melee_dodge_reaction_chance: f32,
    pub melee_reflex_window_seconds: f32,
    pub melee_initiative_delay_min_seconds: f32,
    pub melee_initiative_delay_max_seconds: f32,
    pub melee_cadence_jitter_seconds: f32,
    pub long_weapon_measure_threshold_metres: f32,
    pub melee_measure_reach_fraction: f32,
    pub minimum_hit_precision: f32,
    pub maximum_hit_precision: f32,
    pub outnumbered_flanking: f32,
    pub ranged_defense_input_reflex: f32,
}

impl CombatResolutionParameters {
    pub fn validate(self) -> Result<(), &'static str> {
        self.fatigue.validate()?;
        if !self.armed_attack_energy_transfer.is_finite()
            || !(0.0..=1.0).contains(&self.armed_attack_energy_transfer)
            || self.armed_attack_energy_transfer == 0.0
            || !self.stagger_resistance_joules_per_kg.is_finite()
            || self.stagger_resistance_joules_per_kg <= 0.0
        {
            return Err("combat resolution values must be finite and physically positive");
        }
        Ok(())
    }
}

impl AutoresolveParameters {
    pub fn validate(self) -> Result<(), &'static str> {
        let positive = [
            self.combat_round_seconds,
            self.formation_spacing_metres,
            self.reference_melee_attack_seconds,
            self.minimum_movement_speed_metres_per_second,
            self.guarded_movement_speed_metres_per_second,
            self.melee_lunge_speed_metres_per_second,
            self.melee_lunge_maximum_travel_metres,
            self.reference_ground_drive_force_newtons,
            self.reference_leg_strength,
            self.gravity_metres_per_second_squared,
            self.traction_coefficient,
            self.minimum_attack_interval_seconds,
            self.melee_windup_seconds,
            self.melee_reaction_delay_min_seconds,
            self.melee_reaction_delay_max_seconds,
            self.melee_reflex_window_seconds,
            self.melee_initiative_delay_min_seconds,
            self.melee_initiative_delay_max_seconds,
            self.long_weapon_measure_threshold_metres,
        ];
        if !positive
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            || ![
                self.minimum_hit_precision,
                self.maximum_hit_precision,
                self.minimum_melee_input_reflex,
                self.melee_dodge_reaction_chance,
                self.outnumbered_flanking,
                self.ranged_defense_input_reflex,
                self.melee_cadence_jitter_seconds,
                self.melee_measure_reach_fraction,
            ]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            || self.minimum_hit_precision > self.maximum_hit_precision
            || self.melee_reaction_delay_min_seconds > self.melee_reaction_delay_max_seconds
            || self.melee_initiative_delay_min_seconds > self.melee_initiative_delay_max_seconds
        {
            return Err("autoresolve values must be finite, positive, and ordered");
        }
        Ok(())
    }
}

include!(concat!(env!("OUT_DIR"), "/combat_resolution_config.rs"));
