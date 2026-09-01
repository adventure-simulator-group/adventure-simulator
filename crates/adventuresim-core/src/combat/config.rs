use serde::{Deserialize, Serialize};

/// Physical tuning projected from the canonical tactical combat configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatResolutionParameters {
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
    pub minimum_attack_interval_seconds: f32,
    pub minimum_melee_input_reflex: f32,
    pub minimum_hit_precision: f32,
    pub maximum_hit_precision: f32,
    pub outnumbered_flanking: f32,
    pub ranged_defense_input_reflex: f32,
}

impl CombatResolutionParameters {
    pub fn validate(self) -> Result<(), &'static str> {
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
            self.minimum_attack_interval_seconds,
        ];
        if !positive
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            || ![
                self.minimum_hit_precision,
                self.maximum_hit_precision,
                self.minimum_melee_input_reflex,
                self.outnumbered_flanking,
                self.ranged_defense_input_reflex,
            ]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            || self.minimum_hit_precision > self.maximum_hit_precision
        {
            return Err("autoresolve values must be finite, positive, and ordered");
        }
        Ok(())
    }
}

include!(concat!(env!("OUT_DIR"), "/combat_resolution_config.rs"));
