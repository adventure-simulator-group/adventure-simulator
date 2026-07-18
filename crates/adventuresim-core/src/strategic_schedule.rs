//! Pure settlement schedule progression shared by the authoritative module and tools.

use crate::{activity::*, strategic_time::training_hours_increment};

/// Stable order used by reports and schedule arrays.
pub const SKILL_COUNT: usize = 11;

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
}
