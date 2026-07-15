//! Morale and strategic incapacitation calculations.
//!
//! The strategic layer persists the inputs to these calculations. Short-lived
//! combat imbalance, breath exhaustion, and knockdown remain tactical state.

/// Minimum Will check used as a divisor for negative morale.
pub const MINIMUM_WILL_CHECK: f32 = 0.25;
/// Surplus morale which produces roughly 63% of a character's maximum ally bonus.
pub const MORALE_BONUS_CURVE_SCALE: f32 = 10.0;
/// Maximum ally morale restored per point of the speaker's Charisma check.
pub const MORALE_BONUS_PER_CHARISMA: f32 = 0.05;
/// Losing this fraction of maximum blood volume fully incapacitates a character.
pub const BLOOD_LOSS_INCAPACITATION_FRACTION: f32 = 0.30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncapacitationStatus {
    Ready,
    Staggered,
    Incapacitated,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StrategicIncapacitation {
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
}

impl StrategicIncapacitation {
    pub fn total(self) -> f32 {
        self.pain + self.blood_loss + self.fear + self.fatigue
    }

    pub fn status(self) -> IncapacitationStatus {
        match self.total() {
            total if total >= 1.0 => IncapacitationStatus::Incapacitated,
            total if total > 0.5 => IncapacitationStatus::Staggered,
            _ => IncapacitationStatus::Ready,
        }
    }

    /// Movement and attribute-check multiplier above the stagger threshold.
    pub fn check_multiplier(self) -> f32 {
        (1.0 - 2.0 * (self.total() - 0.5).max(0.0)).clamp(0.0, 1.0)
    }
}

/// Combine same-sign effects with harmonic diminishing returns.
pub fn cumulative_morale(effects: impl IntoIterator<Item = f32>) -> f32 {
    let mut effects: Vec<f32> = effects
        .into_iter()
        .filter(|effect| effect.is_finite() && *effect > 0.0)
        .collect();
    effects.sort_by(|left, right| right.total_cmp(left));
    effects
        .into_iter()
        .enumerate()
        .map(|(index, effect)| effect / (index + 1) as f32)
        .sum()
}

pub fn resolve_morale(
    positive_effects: impl IntoIterator<Item = f32>,
    negative_effects: impl IntoIterator<Item = f32>,
    will_check: f32,
) -> f32 {
    cumulative_morale(positive_effects)
        - cumulative_morale(negative_effects) / will_check.max(MINIMUM_WILL_CHECK)
}

/// Fraction of an ally's negative morale that this character can restore.
///
/// The bonus is based only on the speaker's pre-bonus surplus, which prevents
/// two positive characters from recursively increasing one another's output.
pub fn morale_bonus_fraction(surplus_morale: f32, charisma_check: f32) -> f32 {
    let surplus = surplus_morale.max(0.0);
    let charisma = charisma_check.max(0.0);
    let saturation = 1.0 - (-surplus / MORALE_BONUS_CURVE_SCALE).exp();
    saturation * MORALE_BONUS_PER_CHARISMA * charisma
}

/// Linear decay for recent morale events. `age` and `duration` use the same unit.
pub fn morale_event_decay(age: u64, duration: u64) -> f32 {
    if duration == 0 || age >= duration {
        0.0
    } else {
        1.0 - age as f32 / duration as f32
    }
}

/// Pain from aggregate body-part health deficit, mitigated by Will.
pub fn pain_incapacitation(total_damage: f32, will_check: f32) -> f32 {
    let damage = total_damage.max(0.0);
    let will = will_check.max(0.0);
    if damage == 0.0 {
        return 0.0;
    }
    damage / (damage + 0.5 * will) * (-0.2 * will).exp()
}

pub fn blood_loss_incapacitation(current_blood: f32, maximum_blood: f32) -> f32 {
    if maximum_blood <= 0.0 {
        return 0.0;
    }
    let lost_fraction = (1.0 - current_blood / maximum_blood).max(0.0);
    lost_fraction / BLOOD_LOSS_INCAPACITATION_FRACTION
}

pub fn fear_incapacitation(morale: f32) -> f32 {
    (-morale).max(0.0) / 100.0
}

/// Fatigue begins contributing after half of the character's daily capacity.
pub fn fatigue_incapacitation(fatigue_ratio: f32) -> f32 {
    ((fatigue_ratio - 0.5).max(0.0) / 0.5).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morale_effects_have_ranked_diminishing_returns() {
        assert_eq!(cumulative_morale([6.0, 3.0, 3.0]), 8.5);
    }

    #[test]
    fn will_only_mitigates_negative_morale() {
        assert_eq!(resolve_morale([4.0], [6.0], 2.0), 1.0);
        assert_eq!(resolve_morale([4.0], [6.0], 3.0), 2.0);
    }

    #[test]
    fn morale_bonus_approaches_five_percent_per_charisma() {
        let at_scale = morale_bonus_fraction(MORALE_BONUS_CURVE_SCALE, 4.0);
        assert!((at_scale - 0.126_424).abs() < 0.000_01);
        assert_eq!(morale_bonus_fraction(0.0, 5.0), 0.0);
        assert!((morale_bonus_fraction(1_000.0, 5.0) - 0.25).abs() < 0.000_01);
    }

    #[test]
    fn party_charisma_prevents_independent_bonus_stacking() {
        let party_charisma = crate::capability::aggregate_party_check([4.0; 5]);
        assert!((party_charisma - 9.133_333).abs() < 0.000_01);
        let shared_cap = MORALE_BONUS_PER_CHARISMA * party_charisma;
        assert!((shared_cap - 0.456_666_65).abs() < 0.000_01);
        assert!(shared_cap < 5.0 * (MORALE_BONUS_PER_CHARISMA * 4.0));
    }

    #[test]
    fn morale_events_decay_over_their_duration() {
        assert_eq!(morale_event_decay(0, 7), 1.0);
        assert_eq!(morale_event_decay(3, 6), 0.5);
        assert_eq!(morale_event_decay(7, 7), 0.0);
    }

    #[test]
    fn blood_loss_reaches_incapacitation_at_thirty_percent() {
        let factor = blood_loss_incapacitation(3_500.0, 5_000.0);
        assert!((factor - 1.0).abs() < 0.0001);
    }

    #[test]
    fn strategic_incapacitation_applies_thresholds_and_penalty() {
        let ready = StrategicIncapacitation {
            pain: 0.5,
            ..Default::default()
        };
        assert_eq!(ready.status(), IncapacitationStatus::Ready);
        assert_eq!(ready.check_multiplier(), 1.0);

        let staggered = StrategicIncapacitation {
            pain: 0.75,
            ..Default::default()
        };
        assert_eq!(staggered.status(), IncapacitationStatus::Staggered);
        assert_eq!(staggered.check_multiplier(), 0.5);

        let down = StrategicIncapacitation {
            pain: 0.6,
            fear: 0.4,
            ..Default::default()
        };
        assert_eq!(down.status(), IncapacitationStatus::Incapacitated);
        assert_eq!(down.check_multiplier(), 0.0);
    }
}
