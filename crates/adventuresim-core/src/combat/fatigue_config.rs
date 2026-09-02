use serde::{Deserialize, Serialize};

/// Authored conversion from physical work into the single visible fatigue pool.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatFatigueParameters {
    pub mechanical_efficiency: f32,
    pub capacity_joules_per_endurance: f32,
    pub recovery_watts_per_endurance: f32,
    pub aerobic_watts_per_endurance: f32,
    pub carry_watts_per_kg: f32,
    pub minimum_endurance: f32,
    pub minimum_action_seconds: f32,
    pub maximum_action_seconds: f32,
    pub attack_travel_metres: f32,
    pub defense_travel_metres: f32,
    pub dodge_speed_metres_per_second: f32,
}

impl CombatFatigueParameters {
    pub fn validate(self) -> Result<(), &'static str> {
        let positive = [
            self.mechanical_efficiency,
            self.capacity_joules_per_endurance,
            self.recovery_watts_per_endurance,
            self.aerobic_watts_per_endurance,
            self.carry_watts_per_kg,
            self.minimum_endurance,
            self.minimum_action_seconds,
            self.maximum_action_seconds,
            self.attack_travel_metres,
            self.defense_travel_metres,
            self.dodge_speed_metres_per_second,
        ];
        if positive
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
            || self.mechanical_efficiency > 1.0
            || self.minimum_action_seconds > self.maximum_action_seconds
        {
            return Err("fatigue work parameters must be finite and physically positive");
        }
        Ok(())
    }
}
