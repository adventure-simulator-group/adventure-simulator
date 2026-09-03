use crate::prelude::*;

/// The remaining combat effectiveness after all wheel contributions, not a
/// separate fatigue penalty. Limb injury effects remain independently modeled.
#[must_use]
pub fn combat_incapacitation_performance(incapacitation: f32) -> f32 {
    1.0 - incapacitation.clamp(0.0, 1.0)
}

#[must_use]
pub fn incapacitation_adjusted_recovery_seconds(
    base_recovery_seconds: f32,
    performance: f32,
) -> f32 {
    base_recovery_seconds.max(0.0) / performance.clamp(f32::EPSILON, 1.0)
}

/// Burden uses the existing carrying-capacity rule, but is now an explicit
/// combat incapacitation source rather than a hidden physical-skill multiplier.
#[must_use]
pub fn combat_encumbrance_incapacitation(
    attributes: &impl PlayerAttributes,
    body: &impl PlayerBody,
    equipment: &impl PlayerEquipment,
) -> f32 {
    1.0 - equipment.encumbrance_penalty_by_parts(attributes, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performance_depends_on_total_not_the_source() {
        let fatigue = 0.25;
        let burden = 0.15;
        let injury = 0.10;
        assert_eq!(
            combat_incapacitation_performance(fatigue + burden + injury),
            0.5
        );
        assert_eq!(combat_incapacitation_performance(1.2), 0.0);
        assert_eq!(combat_incapacitation_performance(-0.2), 1.0);
        assert_eq!(incapacitation_adjusted_recovery_seconds(0.4, 0.5), 0.8);
    }
}
