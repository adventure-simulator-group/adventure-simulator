use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiOffenseConfig {
    pub hit_precision: f32,
    pub target_body_part: BodyPart,
    pub windup_seconds: f32,
    pub cooldown_seconds: f32,
    pub initiative_delay_min_seconds: f32,
    pub initiative_delay_max_seconds: f32,
    pub cadence_jitter_seconds: f32,
    pub long_weapon_measure_threshold_metres: f32,
    pub melee_measure_reach_fraction: f32,
    pub ranged_standoff_min_metres: f32,
    pub ranged_standoff_max_metres: f32,
    pub ranged_standoff_slop_metres: f32,
    pub ranged_reach_fraction: f32,
}

impl AiOffenseConfig {
    pub(super) fn has_valid_intervals(&self) -> bool {
        self.ranged_standoff_min_metres.is_finite()
            && self.ranged_standoff_max_metres >= self.ranged_standoff_min_metres
            && self.initiative_delay_min_seconds.is_finite()
            && self.initiative_delay_min_seconds >= 0.0
            && self.initiative_delay_max_seconds >= self.initiative_delay_min_seconds
            && self.cadence_jitter_seconds.is_finite()
            && self.cadence_jitter_seconds >= 0.0
            && self.long_weapon_measure_threshold_metres.is_finite()
            && self.long_weapon_measure_threshold_metres > 0.0
            && self.melee_measure_reach_fraction.is_finite()
            && (0.0..=1.0).contains(&self.melee_measure_reach_fraction)
    }
}

impl Default for AiOffenseConfig {
    fn default() -> Self {
        Self {
            hit_precision: 1.0,
            target_body_part: BodyPart::Chest,
            windup_seconds: 0.65,
            cooldown_seconds: 0.25,
            initiative_delay_min_seconds: 0.04,
            initiative_delay_max_seconds: 0.22,
            cadence_jitter_seconds: 0.16,
            long_weapon_measure_threshold_metres: 1.2,
            melee_measure_reach_fraction: 0.7,
            ranged_standoff_min_metres: 1.5,
            ranged_standoff_max_metres: 12.0,
            ranged_standoff_slop_metres: 0.5,
            ranged_reach_fraction: 0.5,
        }
    }
}
