//! Pure settlement schedule progression shared by the authoritative module and tools.

use crate::{activity::*, strategic_time::training_hours_increment};

/// Stable order used by reports and schedule arrays.
pub const SKILL_COUNT: usize = 12;
/// Ordinary sleep pressure accumulated over a full day without tiring activity.
pub const BASELINE_FATIGUE_PER_DAY: f32 = 600.0;
/// Fatigue added by an hour of sustained ordinary labor.
pub const LABOR_FATIGUE_PER_HOUR: f32 = 50.0;
/// Fatigue removed by an hour of Leisure; six hours offsets baseline wakefulness.
pub const LEISURE_FATIGUE_RECOVERY_PER_HOUR: f32 = 100.0;
/// Maximum daily morale from Leisure left after all fatigue has been removed.
pub const LEISURE_MORALE_LIMIT: f32 = 4.0;
/// Surplus recovery producing about 63% of the Leisure morale limit.
pub const LEISURE_MORALE_SCALE_FATIGUE: f32 = 200.0;
/// Reservoir units represented by one compact Fatigue point in schedule previews.
pub const FATIGUE_RESERVOIR_PER_PREVIEW_POINT: f32 = 100.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillHours {
    pub melee: f32,
    pub dodge: f32,
    pub block: f32,
    pub ranged: f32,
    pub will: f32,
    pub charisma: f32,
    pub medicine: f32,
    pub faith: f32,
    pub stealth: f32,
    pub balance: f32,
    pub surgeon: f32,
    pub smithing: f32,
}

impl SkillHours {
    pub fn values(self) -> [f32; SKILL_COUNT] {
        [
            self.melee,
            self.dodge,
            self.block,
            self.ranged,
            self.will,
            self.charisma,
            self.medicine,
            self.faith,
            self.stealth,
            self.balance,
            self.surgeon,
            self.smithing,
        ]
    }

    pub fn is_finite(self) -> bool {
        self.values().into_iter().all(f32::is_finite)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailySchedule {
    pub melee: u16,
    pub dodge: u16,
    pub block: u16,
    pub ranged: u16,
    pub will: u16,
    pub charisma: u16,
    pub medicine: u16,
    pub faith: u16,
    pub stealth: u16,
    pub balance: u16,
    pub surgeon: u16,
    pub smithing: u16,
    pub labor: u16,
    pub prayer: u16,
    pub thievery: u16,
    pub raiding: u16,
}

impl DailySchedule {
    pub fn allocated_minutes(self) -> u64 {
        [
            self.melee,
            self.dodge,
            self.block,
            self.ranged,
            self.will,
            self.charisma,
            self.medicine,
            self.faith,
            self.stealth,
            self.balance,
            self.surgeon,
            self.smithing,
            self.labor,
            self.prayer,
            self.thievery,
            self.raiding,
        ]
        .into_iter()
        .map(u64::from)
        .sum()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeisureOutcome {
    /// Signed change to the fatigue reservoir; negative values are recovery.
    pub fatigue_delta: f32,
    pub morale: f32,
    /// Portion of the interval after carried and newly generated fatigue was
    /// fully cleared, during which Leisure was actually earning morale.
    pub morale_earning_minutes: f32,
    pub leisure_hours: f32,
}

/// Resolve settlement Leisure against baseline wakefulness, scheduled exertion,
/// and fatigue already carried into the interval. Recovery beyond all fatigue
/// becomes a bounded morale benefit instead of driving fatigue below zero.
pub fn settlement_leisure_outcome(
    schedule: DailySchedule,
    elapsed_minutes: u64,
    current_fatigue: f32,
) -> LeisureOutcome {
    if elapsed_minutes == 0 {
        return LeisureOutcome::default();
    }
    let days = elapsed_minutes as f32 / crate::strategic_time::MINUTES_PER_DAY as f32;
    let leisure_hours_per_day = (crate::strategic_time::MINUTES_PER_DAY
        .saturating_sub(schedule.allocated_minutes())) as f32
        / 60.0;
    let labor_hours = days * f32::from(schedule.labor) / 60.0;
    let generated = days * BASELINE_FATIGUE_PER_DAY + labor_hours * LABOR_FATIGUE_PER_HOUR;
    let recovery = days * leisure_hours_per_day * LEISURE_FATIGUE_RECOVERY_PER_HOUR;
    let fatigue_before_recovery = current_fatigue.max(0.0) + generated;
    let fatigue_after = (fatigue_before_recovery - recovery).max(0.0);
    let surplus_recovery_per_day = (leisure_hours_per_day * LEISURE_FATIGUE_RECOVERY_PER_HOUR
        - BASELINE_FATIGUE_PER_DAY
        - f32::from(schedule.labor) / 60.0 * LABOR_FATIGUE_PER_HOUR)
        .max(0.0);
    let time_to_clear_fatigue = if surplus_recovery_per_day > 0.0 {
        current_fatigue.max(0.0) / surplus_recovery_per_day
    } else {
        f32::INFINITY
    };
    let qualifying_days = (days - time_to_clear_fatigue).max(0.0);
    let daily_morale_quality = LEISURE_MORALE_LIMIT
        * (1.0
            - (-surplus_recovery_per_day / LEISURE_MORALE_SCALE_FATIGUE.max(f32::EPSILON)).exp());
    LeisureOutcome {
        fatigue_delta: fatigue_after - current_fatigue.max(0.0),
        // Morale begins only at the point within the interval where carried
        // fatigue reaches zero. Both that crossing and the daily quality are
        // rates, making the earned total independent of interval partitioning.
        morale: qualifying_days * daily_morale_quality,
        morale_earning_minutes: qualifying_days * crate::strategic_time::MINUTES_PER_DAY as f32,
        leisure_hours: days * leisure_hours_per_day,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityTrainingProfile {
    pub raiding_melee: bool,
    pub raiding_ranged: bool,
    pub raiding_block: bool,
    pub raiding_dodge: bool,
}

/// Apply one elapsed interval. Calling this once for N days or N times for one day
/// produces the same training total apart from normal floating-point rounding.
pub fn apply_schedule_training(
    skills: &mut SkillHours,
    schedule: DailySchedule,
    elapsed_minutes: u64,
    profile: ActivityTrainingProfile,
) {
    let increment = |minutes| training_hours_increment(elapsed_minutes, minutes);
    skills.melee += increment(schedule.melee);
    skills.dodge += increment(schedule.dodge);
    skills.block += increment(schedule.block);
    skills.ranged += increment(schedule.ranged);
    skills.will += increment(schedule.will);
    skills.charisma += increment(schedule.charisma);
    skills.medicine += increment(schedule.medicine);
    skills.faith += increment(schedule.faith);
    skills.stealth += increment(schedule.stealth);
    skills.balance += increment(schedule.balance);
    skills.surgeon += increment(schedule.surgeon);
    skills.smithing += increment(schedule.smithing);
    skills.faith += increment(schedule.prayer) * ACTIVITY_TRAINING_RATE;
    skills.will += increment(schedule.labor) * ACTIVITY_TRAINING_RATE;
    skills.stealth += increment(schedule.thievery) * ACTIVITY_TRAINING_RATE;
    let raiding = increment(schedule.raiding) * ACTIVITY_TRAINING_RATE;
    if profile.raiding_ranged {
        skills.ranged += raiding;
    } else if profile.raiding_melee {
        skills.melee += raiding;
    }
    if profile.raiding_block {
        skills.block += raiding * 0.5;
    }
    if profile.raiding_dodge {
        skills.dodge += raiding * 0.5;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityOutcomeInputs {
    pub strength_check: f32,
    pub endurance_check: f32,
    pub stealth_check: f32,
    pub combat_check: f32,
    pub population_scale: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityOutcome {
    pub gold_earned: u32,
    pub notoriety_gained: f32,
    pub thievery_discovery_chance: f32,
    pub raiding_retaliation_chance: f32,
    pub labor_hours: f32,
    pub thievery_hours: f32,
    pub raiding_hours: f32,
}

/// Calculate authoritative economic and risk results for one settlement interval.
pub fn settlement_activity_outcome(
    schedule: DailySchedule,
    elapsed_minutes: u64,
    inputs: ActivityOutcomeInputs,
) -> ActivityOutcome {
    let days = elapsed_minutes as f32 / crate::strategic_time::MINUTES_PER_DAY as f32;
    let hours = |minutes: u16| days * f32::from(minutes) / 60.0;
    let labor_hours = hours(schedule.labor);
    let thievery_hours = hours(schedule.thievery);
    let raiding_hours = hours(schedule.raiding);
    ActivityOutcome {
        gold_earned: labor_gold(labor_hours, inputs.strength_check, inputs.endurance_check)
            .saturating_add(thievery_gold(
                thievery_hours,
                inputs.population_scale,
                inputs.stealth_check,
            ))
            .saturating_add(raiding_gold(raiding_hours, inputs.combat_check)),
        notoriety_gained: thievery_notoriety(
            thievery_hours,
            inputs.population_scale,
            inputs.stealth_check,
        ) + raiding_notoriety(raiding_hours),
        thievery_discovery_chance: thievery_discovery_chance(
            thievery_hours,
            inputs.population_scale,
            inputs.stealth_check,
        ),
        raiding_retaliation_chance: raiding_retaliation_chance(raiding_hours),
        labor_hours,
        thievery_hours,
        raiding_hours,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategic_time::MINUTES_PER_DAY;

    #[test]
    fn chunked_and_daily_progression_agree() {
        let schedule = DailySchedule {
            melee: 120,
            labor: 480,
            thievery: 60,
            ..Default::default()
        };
        let mut chunked = SkillHours::default();
        apply_schedule_training(
            &mut chunked,
            schedule,
            30 * MINUTES_PER_DAY,
            ActivityTrainingProfile::default(),
        );
        let mut daily = SkillHours::default();
        for _ in 0..30 {
            apply_schedule_training(
                &mut daily,
                schedule,
                MINUTES_PER_DAY,
                ActivityTrainingProfile::default(),
            );
        }
        for (a, b) in chunked.values().into_iter().zip(daily.values()) {
            assert!((a - b).abs() < 0.001);
        }
    }

    #[test]
    fn settlement_outcome_exposes_economic_source_and_risk() {
        let outcome = settlement_activity_outcome(
            DailySchedule {
                labor: 480,
                thievery: 60,
                ..Default::default()
            },
            MINUTES_PER_DAY,
            ActivityOutcomeInputs {
                strength_check: 3.0,
                endurance_check: 2.0,
                stealth_check: 2.0,
                combat_check: 0.0,
                population_scale: 2.0,
            },
        );
        assert_eq!(outcome.labor_hours, 8.0);
        assert_eq!(outcome.thievery_hours, 1.0);
        assert!(outcome.gold_earned > 0);
        assert!(outcome.notoriety_gained > 0.0);
        assert!((0.0..=1.0).contains(&outcome.thievery_discovery_chance));
    }

    #[test]
    fn rounded_activity_income_documents_aggregate_interval_difference() {
        let schedule = DailySchedule {
            labor: 60,
            ..Default::default()
        };
        let inputs = ActivityOutcomeInputs {
            strength_check: 3.0,
            endurance_check: 2.0,
            ..Default::default()
        };
        let aggregate = settlement_activity_outcome(schedule, 30 * MINUTES_PER_DAY, inputs);
        let repeated = (0..30)
            .map(|_| settlement_activity_outcome(schedule, MINUTES_PER_DAY, inputs).gold_earned)
            .sum::<u32>();
        assert_ne!(aggregate.gold_earned, repeated);
        assert_eq!(aggregate.gold_earned, 19);
        assert_eq!(repeated, 30);
    }

    #[test]
    fn six_hours_of_leisure_exactly_offsets_baseline_fatigue() {
        let six_hours = DailySchedule {
            melee: 18 * 60,
            ..Default::default()
        };
        assert_eq!(
            settlement_leisure_outcome(six_hours, MINUTES_PER_DAY, 0.0).fatigue_delta,
            0.0
        );
        let five_hours = DailySchedule {
            melee: 19 * 60,
            ..Default::default()
        };
        assert_eq!(
            settlement_leisure_outcome(five_hours, MINUTES_PER_DAY, 0.0).fatigue_delta,
            100.0
        );
    }

    #[test]
    fn leisure_offsets_labor_before_granting_morale() {
        let exactly_offsets_labor = DailySchedule {
            labor: 4 * 60,
            melee: 12 * 60,
            ..Default::default()
        };
        let offset = settlement_leisure_outcome(exactly_offsets_labor, MINUTES_PER_DAY, 0.0);
        assert_eq!(offset.leisure_hours, 8.0);
        assert_eq!(offset.fatigue_delta, 0.0);
        assert_eq!(offset.morale, 0.0);

        let surplus = settlement_leisure_outcome(
            DailySchedule {
                melee: 15 * 60,
                ..Default::default()
            },
            MINUTES_PER_DAY,
            0.0,
        );
        assert_eq!(surplus.leisure_hours, 9.0);
        assert_eq!(surplus.fatigue_delta, 0.0);
        assert!(surplus.morale > 0.0 && surplus.morale < LEISURE_MORALE_LIMIT);
    }

    #[test]
    fn leisure_removes_carried_fatigue_before_morale() {
        let schedule = DailySchedule {
            melee: 16 * 60,
            ..Default::default()
        };
        let outcome = settlement_leisure_outcome(schedule, MINUTES_PER_DAY, 200.0);
        assert_eq!(outcome.fatigue_delta, -200.0);
        assert_eq!(outcome.morale, 0.0);

        let next_day = settlement_leisure_outcome(schedule, MINUTES_PER_DAY, 0.0);
        assert_eq!(next_day.fatigue_delta, 0.0);
        assert!(next_day.morale > 0.0);
    }

    #[test]
    fn leisure_morale_is_proportional_to_elapsed_time() {
        let schedule = DailySchedule {
            melee: 15 * 60,
            ..Default::default()
        };
        let daily = settlement_leisure_outcome(schedule, MINUTES_PER_DAY, 0.0);
        let hourly = settlement_leisure_outcome(schedule, 60, 0.0);

        assert!((daily.morale - hourly.morale * 24.0).abs() < 0.001);
    }

    #[test]
    fn leisure_morale_crossing_carried_fatigue_is_partition_independent() {
        let schedule = DailySchedule {
            melee: 16 * 60,
            ..Default::default()
        };
        let starting_fatigue = 350.0;
        let aggregate = settlement_leisure_outcome(schedule, 4 * MINUTES_PER_DAY, starting_fatigue);

        let mut daily_fatigue = starting_fatigue;
        let mut daily_morale = 0.0;
        for _ in 0..4 {
            let outcome = settlement_leisure_outcome(schedule, MINUTES_PER_DAY, daily_fatigue);
            daily_fatigue += outcome.fatigue_delta;
            daily_morale += outcome.morale;
        }

        let mut hourly_fatigue = starting_fatigue;
        let mut hourly_morale = 0.0;
        for _ in 0..(4 * 24) {
            let outcome = settlement_leisure_outcome(schedule, 60, hourly_fatigue);
            hourly_fatigue += outcome.fatigue_delta;
            hourly_morale += outcome.morale;
        }

        assert!((aggregate.morale - daily_morale).abs() < 0.001);
        assert!((aggregate.morale - hourly_morale).abs() < 0.001);
        assert!((starting_fatigue + aggregate.fatigue_delta - hourly_fatigue).abs() < 0.001);
    }

    #[test]
    fn leisure_fatigue_is_chunk_invariant() {
        let schedule = DailySchedule {
            labor: 4 * 60,
            melee: 12 * 60,
            ..Default::default()
        };
        let aggregate = settlement_leisure_outcome(schedule, 30 * MINUTES_PER_DAY, 500.0);
        let mut repeated_fatigue = 500.0;
        for _ in 0..30 {
            repeated_fatigue +=
                settlement_leisure_outcome(schedule, MINUTES_PER_DAY, repeated_fatigue)
                    .fatigue_delta;
        }
        assert!((500.0 + aggregate.fatigue_delta - repeated_fatigue).abs() < 0.001);
        let aggregate_projection =
            settlement_leisure_outcome(schedule, MINUTES_PER_DAY, 500.0 + aggregate.fatigue_delta);
        let repeated_projection =
            settlement_leisure_outcome(schedule, MINUTES_PER_DAY, repeated_fatigue);
        assert!((aggregate_projection.morale - repeated_projection.morale).abs() < 0.001);
    }
}
