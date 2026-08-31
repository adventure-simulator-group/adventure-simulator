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

include!(concat!(env!("OUT_DIR"), "/combat_resolution_config.rs"));
