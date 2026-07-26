//! Morale and strategic incapacitation calculations.
//!
//! The strategic layer persists the inputs to these calculations. Short-lived
//! combat imbalance, breath exhaustion, and knockdown remain tactical state.

/// Minimum Will check used as a divisor for negative morale.
pub const MINIMUM_WILL_CHECK: f32 = 0.25;
/// Surplus morale which produces roughly 63% of a character's maximum ally bonus.
pub const MORALE_BONUS_CURVE_SCALE: f32 = 10.0;
/// Maximum ally morale restored per point of the speaker's Command check.
pub const MORALE_BONUS_PER_COMMAND: f32 = 0.05;
/// Raw interreligious discord created by each point of foreign conviction pressure
/// which the party's social leadership cannot absorb.
pub const RELIGIOUS_DISCORD_SEVERITY: f32 = 3.0;
/// Pressure above this baseline begins to register as visible fervor.
pub const FERVOR_PRESSURE_BASELINE: f32 = 2.5;
/// Pressure required to reach roughly 63% on the Fervor meter.
pub const FERVOR_CURVE_SCALE: f32 = 5.0;
/// Maximum raw morale cost of neglecting a conviction demand.
pub const MAX_RELIGIOUS_NEGLECT_MORALE: f32 = 8.0;
/// Raw neglect morale removed per point of aggregate party Command.
pub const RELIGIOUS_NEGLECT_COMMAND_RELIEF: f32 = 1.6;
/// Losing this fraction of maximum blood volume fully incapacitates a character.
pub const BLOOD_LOSS_INCAPACITATION_FRACTION: f32 = 0.30;
/// Shared upper bound for enjoyment from training already-mastered skills.
pub const MASTERY_ENJOYMENT_LIMIT: f32 = 4.0;
/// Excess effective hours needed to traverse one e-fold of the mastery curve.
pub const MASTERY_ENJOYMENT_EFOLD_HOURS: f32 = 40.0;

/// Advance shared mastery enjoyment through one logical training interval.
///
/// Existing enjoyment first decays through the full interval. The combined
/// rejected-hour award then saturates at the interval endpoint, so award order
/// within one logical clock advance cannot affect the result.
pub fn mastery_enjoyment_after_interval(
    starting_morale: f32,
    excess_effective_hours: f32,
    elapsed_minutes: u64,
    duration_minutes: u64,
) -> f32 {
    let starting = if starting_morale.is_finite() {
        starting_morale.clamp(0.0, MASTERY_ENJOYMENT_LIMIT)
    } else {
        0.0
    };
    let excess = if excess_effective_hours.is_finite() {
        excess_effective_hours.max(0.0)
    } else {
        0.0
    };
    let decayed = if duration_minutes == 0 {
        0.0
    } else {
        starting * (1.0 - elapsed_minutes as f32 / duration_minutes as f32).clamp(0.0, 1.0)
    };
    MASTERY_ENJOYMENT_LIMIT
        - (MASTERY_ENJOYMENT_LIMIT - decayed)
            * (-excess / MASTERY_ENJOYMENT_EFOLD_HOURS).exp()
}

pub fn mastery_enjoyment_decay(age_minutes: u64, duration_minutes: u64) -> f32 {
    if duration_minutes == 0 || age_minutes >= duration_minutes {
        0.0
    } else {
        1.0 - age_minutes as f32 / duration_minutes as f32
    }
}

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
    pub hunger: f32,
    pub thirst: f32,
}

impl StrategicIncapacitation {
    pub fn total(self) -> f32 {
        self.pain + self.blood_loss + self.fear + self.fatigue + self.hunger + self.thirst
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
pub fn morale_bonus_fraction(surplus_morale: f32, command_check: f32) -> f32 {
    let surplus = surplus_morale.max(0.0);
    let command = command_check.max(0.0);
    let saturation = 1.0 - (-surplus / MORALE_BONUS_CURVE_SCALE).exp();
    saturation * MORALE_BONUS_PER_COMMAND * command
}

/// Raw negative morale from mixed-faith tension. Party Command is subtracted
/// from foreign faith pressure so sufficiently capable leadership removes the
/// penalty entirely instead of merely reducing it proportionally.
pub fn religious_discord(foreign_conviction_pressure: f32, party_command: f32) -> f32 {
    RELIGIOUS_DISCORD_SEVERITY
        * (foreign_conviction_pressure.max(0.0) - party_command.max(0.0)).max(0.0)
}

/// Bounded religious pressure used by the strategic Fervor meter.
pub fn fervor_fraction(
    individual_conviction: f32,
    same_religion_conviction: f32,
    surplus_morale: f32,
    party_command: f32,
) -> f32 {
    let pressure = (individual_conviction.max(0.0)
        + same_religion_conviction.max(0.0)
        + surplus_morale.max(0.0) / MORALE_BONUS_CURVE_SCALE
        - party_command.max(0.0)
        - FERVOR_PRESSURE_BASELINE)
        .max(0.0);
    1.0 - (-pressure / FERVOR_CURVE_SCALE).exp()
}

/// Whether a continuous Fervor probability succeeds for a normalized roll.
pub fn fervor_event_occurs(fervor: f32, roll: f32) -> bool {
    roll.clamp(0.0, 1.0) < fervor.clamp(0.0, 1.0)
}

/// Raw morale cost of declining prayer or holy-day observance. Command is
/// subtractive and can eliminate the cost entirely.
pub fn religious_neglect_morale(fervor: f32, party_command: f32) -> f32 {
    (MAX_RELIGIOUS_NEGLECT_MORALE * fervor.clamp(0.0, 1.0)
        - RELIGIOUS_NEGLECT_COMMAND_RELIEF * party_command.clamp(0.0, 5.0))
    .max(0.0)
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

/// Hunger begins only after the body's short-term energy reserve is exhausted
/// and reaches full incapacitation after three unsupported marching days.
pub fn hunger_incapacitation(food_balance_kcal: f32, travel_kcal_per_day: f32) -> f32 {
    if travel_kcal_per_day <= 0.0 {
        return 0.0;
    }
    ((-food_balance_kcal).max(0.0) / (travel_kcal_per_day * 3.0)).powi(2)
}

/// Thirst escalates much faster than hunger and reaches full incapacitation
/// after one unsupported marching day beyond the body's short-term reserve.
pub fn thirst_incapacitation(water_balance_ml: f32, travel_water_ml_per_day: f32) -> f32 {
    if travel_water_ml_per_day <= 0.0 {
        return 0.0;
    }
    ((-water_balance_ml).max(0.0) / travel_water_ml_per_day).powi(2)
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
    fn morale_bonus_approaches_five_percent_per_command() {
        let at_scale = morale_bonus_fraction(MORALE_BONUS_CURVE_SCALE, 4.0);
        assert!((at_scale - 0.126_424).abs() < 0.000_01);
        assert_eq!(morale_bonus_fraction(0.0, 5.0), 0.0);
        assert!((morale_bonus_fraction(1_000.0, 5.0) - 0.25).abs() < 0.000_01);
    }

    #[test]
    fn party_command_prevents_independent_bonus_stacking() {
        let party_command = crate::capability::aggregate_party_command([4.0; 5]);
        assert_eq!(party_command, 5.0);
        let shared_cap = MORALE_BONUS_PER_COMMAND * party_command;
        assert!((shared_cap - 0.25).abs() < 0.000_01);
        assert!(shared_cap < 5.0 * (MORALE_BONUS_PER_COMMAND * 4.0));
    }

    #[test]
    fn command_subtracts_from_mixed_faith_pressure() {
        assert_eq!(religious_discord(4.0, 1.0), 9.0);
        assert_eq!(religious_discord(4.0, 4.0), 0.0);
        assert_eq!(religious_discord(4.0, 5.0), 0.0);
    }

    #[test]
    fn fervor_is_bounded_and_command_mitigates_it() {
        let unmitigated = fervor_fraction(5.0, 5.0, 10.0, 0.0);
        let led = fervor_fraction(5.0, 5.0, 10.0, 5.0);
        assert!(unmitigated > led);
        assert!((0.0..1.0).contains(&unmitigated));
        assert_eq!(fervor_fraction(1.0, 1.0, 0.0, 5.0), 0.0);
    }

    #[test]
    fn fervor_events_scale_continuously() {
        assert!(!fervor_event_occurs(0.25, 0.25));
        assert!(fervor_event_occurs(0.26, 0.25));
        assert!(!fervor_event_occurs(0.0, 0.0));
        assert!(fervor_event_occurs(1.0, 0.999));
    }

    #[test]
    fn command_can_eliminate_religious_neglect() {
        assert!((religious_neglect_morale(0.75, 1.0) - 4.4).abs() < 0.001);
        assert_eq!(religious_neglect_morale(0.75, 3.75), 0.0);
        assert_eq!(religious_neglect_morale(1.0, 5.0), 0.0);
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

    #[test]
    fn hunger_and_thirst_begin_after_reserves_are_exhausted() {
        assert_eq!(hunger_incapacitation(1.0, 6_000.0), 0.0);
        assert_eq!(thirst_incapacitation(1.0, 4_000.0), 0.0);
        assert!((hunger_incapacitation(-18_000.0, 6_000.0) - 1.0).abs() < 0.0001);
        assert!((thirst_incapacitation(-4_000.0, 4_000.0) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn mastery_enjoyment_is_bounded_aggregated_and_continuously_decayed() {
        let duration = 7 * 24 * 60;
        let combined = mastery_enjoyment_after_interval(0.0, 80.0, 1_440, duration);
        assert!(combined > 0.0 && combined < MASTERY_ENJOYMENT_LIMIT);
        assert!(mastery_enjoyment_after_interval(0.0, 0.01, 1_440, duration) > 0.0);
        assert!(
            mastery_enjoyment_after_interval(0.0, 10_000.0, 1_440, duration)
                <= MASTERY_ENJOYMENT_LIMIT
        );
        // Schedule/language and terrain/oral awards use this same aggregation:
        // combining sources before the shared update cannot multiply morale.
        let combined_sources = mastery_enjoyment_after_interval(0.0, 12.0 + 8.0, 1_440, duration);
        let sequential_sources = mastery_enjoyment_after_interval(
            mastery_enjoyment_after_interval(0.0, 12.0, 1_440, duration),
            8.0,
            0,
            duration,
        );
        assert!((combined_sources - sequential_sources).abs() < 0.0001);
        let partially_aged =
            mastery_enjoyment_after_interval(2.0, 0.01, duration / 2, duration);
        let decayed = 1.0;
        let expected = MASTERY_ENJOYMENT_LIMIT
            - (MASTERY_ENJOYMENT_LIMIT - decayed)
                * (-0.01 / MASTERY_ENJOYMENT_EFOLD_HOURS).exp();
        let frozen_then_awarded = MASTERY_ENJOYMENT_LIMIT
            - (MASTERY_ENJOYMENT_LIMIT - 2.0)
                * (-0.01 / MASTERY_ENJOYMENT_EFOLD_HOURS).exp();
        assert!((partially_aged - expected).abs() < 0.0001);
        assert!(partially_aged < frozen_then_awarded);
        assert_eq!(
            mastery_enjoyment_after_interval(combined, 0.0, duration, duration),
            0.0
        );
    }
}
