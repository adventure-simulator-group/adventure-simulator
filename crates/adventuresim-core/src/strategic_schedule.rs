//! Pure settlement schedule progression shared by the authoritative module and tools.

use crate::equipment::WeaponSkillDistribution;
use crate::profession::ProfessionId;
use crate::{activity::*, strategic_time::training_hours_increment};
use adventuresim_world_schema::{OfficialReligion, ReligionHours};

/// Stable order used by reports and schedule arrays.
pub const SKILL_COUNT: usize = 20;
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
    pub polearm: f32,
    pub axe: f32,
    pub bludgeon: f32,
    pub sword: f32,
    pub knife: f32,
    pub dodge: f32,
    pub block: f32,
    pub bow: f32,
    pub crossbow: f32,
    pub firearm: f32,
    pub throw: f32,
    pub will: f32,
    pub charisma: f32,
    pub medicine: f32,
    pub religion: ReligionHours,
    pub stealth: f32,
    pub balance: f32,
    pub anatomy: f32,
    pub tailoring: f32,
    pub smithing: f32,
}

impl SkillHours {
    pub fn values(self) -> [f32; SKILL_COUNT] {
        [
            self.polearm,
            self.axe,
            self.bludgeon,
            self.sword,
            self.knife,
            self.dodge,
            self.block,
            self.bow,
            self.crossbow,
            self.firearm,
            self.throw,
            self.will,
            self.charisma,
            self.medicine,
            self.religion.total_direct(),
            self.stealth,
            self.balance,
            self.anatomy,
            self.tailoring,
            self.smithing,
        ]
    }

    pub fn is_finite(self) -> bool {
        self.values().into_iter().all(f32::is_finite)
            && self
                .religion
                .direct_values()
                .all(|(_, hours)| hours.is_finite())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailySchedule {
    /// Structured combat practice, including weapon drills, will, and balance.
    pub combat_training_minutes: u16,
    /// Social recreation which trains Charisma at the activity rate.
    pub carousing_minutes: u16,
    /// Supervised work in an unlocked profession.
    pub apprenticeship_minutes: u16,
    pub apprenticeship_service_id: Option<ProfessionId>,
    /// Independent paid professional work, available at Journeyman rank.
    pub profession_practice_minutes: u16,
    pub profession_service_id: Option<ProfessionId>,
    pub labor: u16,
    pub prayer: u16,
    pub thievery: u16,
    pub raiding: u16,
}

impl DailySchedule {
    pub fn allocated_minutes(&self) -> u64 {
        [
            self.labor,
            self.prayer,
            self.thievery,
            self.raiding,
            self.combat_training_minutes,
            self.carousing_minutes,
            self.apprenticeship_minutes,
            self.profession_practice_minutes,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityTrainingProfile {
    pub combat: CombatTrainingProfile,
}

/// Relative training demand for equipped weapon leaves and the four Defense leaves.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatTrainingProfile {
    pub weapons: WeaponSkillDistribution,
    pub dodge: f32,
    pub block: f32,
    pub balance: f32,
    pub will: f32,
}

impl Default for CombatTrainingProfile {
    fn default() -> Self {
        Self {
            weapons: WeaponSkillDistribution::default(),
            dodge: 1.0,
            block: 0.0,
            balance: 1.0,
            will: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EquippedCombatItem {
    pub weapons: WeaponSkillDistribution,
    pub shield: bool,
    pub balance: f32,
}

impl CombatTrainingProfile {
    pub fn from_equipped_hands(hands: impl IntoIterator<Item = EquippedCombatItem>) -> Self {
        let mut result = Self::default();
        let mut best_melee_balance: Option<f32> = None;
        for item in hands {
            for (target, weight) in [
                &mut result.weapons.polearm,
                &mut result.weapons.axe,
                &mut result.weapons.bludgeon,
                &mut result.weapons.sword,
                &mut result.weapons.knife,
                &mut result.weapons.bow,
                &mut result.weapons.crossbow,
                &mut result.weapons.firearm,
                &mut result.weapons.throw,
            ]
            .into_iter()
            .zip(item.weapons.weights())
            {
                *target = target.max(weight);
            }
            if item.shield {
                result.block = 1.0;
            }
            if item.weapons.melee_total() > 0.0 && item.weapons.ranged_total() == 0.0 {
                let balance = if item.balance.is_finite() {
                    item.balance.clamp(0.0, 1.0)
                } else {
                    1.0
                };
                best_melee_balance =
                    Some(best_melee_balance.map_or(balance, |old| old.min(balance)));
            }
        }
        if result.block < 1.0 {
            result.block = best_melee_balance.map_or(0.0, |balance| 1.0 - balance);
        }
        result
    }

    pub fn weights(self) -> [f32; 13] {
        let w = self.weapons.weights();
        [
            w[0],
            w[1],
            w[2],
            w[3],
            w[4],
            w[5],
            w[6],
            w[7],
            w[8],
            self.dodge,
            self.block,
            self.balance,
            self.will,
        ]
        .map(|weight| {
            if weight.is_finite() {
                weight.clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
    }
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
    skills.charisma += increment(schedule.carousing_minutes) * ACTIVITY_TRAINING_RATE;
    skills.will += increment(schedule.labor) * ACTIVITY_TRAINING_RATE;
    skills.stealth += increment(schedule.thievery) * ACTIVITY_TRAINING_RATE;
    // Combat Training and Raiding conserve their activity training budget
    // across equipped weapon leaves and all four Defense leaves.
    let combat_training_hours = increment(schedule.combat_training_minutes)
        + increment(schedule.raiding) * ACTIVITY_TRAINING_RATE;
    if combat_training_hours > 0.0 {
        let combat_weights = profile.combat.weights();
        let total_weight = combat_weights.into_iter().sum::<f32>();
        for (hours, weight) in [
            &mut skills.polearm,
            &mut skills.axe,
            &mut skills.bludgeon,
            &mut skills.sword,
            &mut skills.knife,
            &mut skills.bow,
            &mut skills.crossbow,
            &mut skills.firearm,
            &mut skills.throw,
            &mut skills.dodge,
            &mut skills.block,
            &mut skills.balance,
            &mut skills.will,
        ]
        .into_iter()
        .zip(combat_weights)
        {
            *hours += combat_training_hours * weight / total_weight;
        }
    }

    apply_profession_training(
        skills,
        schedule.apprenticeship_service_id,
        increment(schedule.apprenticeship_minutes),
    );
    apply_profession_training(
        skills,
        schedule.profession_service_id,
        increment(schedule.profession_practice_minutes),
    );
}

fn apply_profession_training(
    skills: &mut SkillHours,
    service_id: Option<ProfessionId>,
    hours: f32,
) {
    match service_id {
        Some(ProfessionId::Merchant | ProfessionId::Innkeeper) => skills.charisma += hours,
        Some(ProfessionId::Weaponsmith | ProfessionId::Armourer) => skills.smithing += hours,
        Some(ProfessionId::Tailor) => skills.tailoring += hours,
        Some(ProfessionId::Herbalist) => {
            skills.medicine += hours * 0.5;
            skills.anatomy += hours / 6.0;
            skills.knife += hours / 6.0;
            skills.tailoring += hours / 6.0;
        }
        // Religion is tradition-specific and is applied by the authoritative
        // caller after resolving the settlement tradition.
        _ => {}
    }
}

/// Apply prayer activity training after the caller resolves the settlement tradition.
pub fn apply_religion_training(
    religion_hours: &mut ReligionHours,
    elapsed_minutes: u64,
    prayer_religion: Option<OfficialReligion>,
    prayer_minutes: u16,
) {
    if let Some(religion) = prayer_religion {
        religion_hours.add_direct(
            religion,
            training_hours_increment(elapsed_minutes, prayer_minutes) * ACTIVITY_TRAINING_RATE,
        );
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
    pub carousing_morale: f32,
    pub virtue_lost: f32,
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
    let carousing_hours = hours(schedule.carousing_minutes);
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
        carousing_morale: days * carousing_morale_per_day(schedule.carousing_minutes),
        virtue_lost: carousing_hours * 0.125,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profession::ProfessionId;
    use crate::strategic_time::MINUTES_PER_DAY;

    #[test]
    fn new_activities_conserve_training_and_apply_profession_weights() {
        let mut skills = SkillHours::default();
        let schedule = DailySchedule {
            combat_training_minutes: 360,
            carousing_minutes: 240,
            apprenticeship_minutes: 120,
            apprenticeship_service_id: Some(ProfessionId::Herbalist),
            ..Default::default()
        };
        apply_schedule_training(
            &mut skills,
            schedule,
            MINUTES_PER_DAY,
            ActivityTrainingProfile::default(),
        );
        let combat_total = skills.polearm
            + skills.axe
            + skills.bludgeon
            + skills.sword
            + skills.knife
            + skills.bow
            + skills.crossbow
            + skills.firearm
            + skills.throw
            + skills.dodge
            + skills.block
            + skills.will
            + skills.balance;
        // Knife also receives one sixth of the two-hour herbalist activity.
        assert!((combat_total - skills.knife - 6.0).abs() < 0.001);
        assert!((skills.charisma - 1.0).abs() < 0.001);
        assert!((skills.medicine - 1.0).abs() < 0.001);
        assert!((skills.anatomy - 1.0 / 3.0).abs() < 0.001);
        assert!((skills.knife - 1.0 / 3.0).abs() < 0.001);
        assert!((skills.tailoring - 1.0 / 3.0).abs() < 0.001);
    }

    fn item(melee: bool, ranged: bool, shield: bool, balance: f32) -> EquippedCombatItem {
        EquippedCombatItem {
            weapons: WeaponSkillDistribution {
                sword: if melee { 1.0 } else { 0.0 },
                bow: if ranged { 1.0 } else { 0.0 },
                ..Default::default()
            },
            shield,
            balance,
        }
    }

    #[test]
    fn combat_relevance_uses_both_hands_and_sanitizes_balance() {
        assert_eq!(
            CombatTrainingProfile::from_equipped_hands([]),
            CombatTrainingProfile::default()
        );
        assert_eq!(
            CombatTrainingProfile::from_equipped_hands([item(false, true, false, 0.1)]).weights(),
            [
                0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0
            ]
        );
        assert_eq!(
            CombatTrainingProfile::from_equipped_hands([item(true, true, false, 0.0)]).weights(),
            [
                0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0
            ]
        );
        assert_eq!(
            CombatTrainingProfile::from_equipped_hands([
                item(false, true, false, 0.1),
                item(true, false, false, 0.3)
            ])
            .weights(),
            [
                0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.7, 1.0, 1.0
            ]
        );
        assert_eq!(
            CombatTrainingProfile::from_equipped_hands([
                item(true, false, false, 0.7),
                item(true, false, false, 0.2)
            ])
            .block,
            0.8
        );
        assert_eq!(
            CombatTrainingProfile::from_equipped_hands([
                item(true, false, false, 0.2),
                item(false, false, true, 0.0)
            ])
            .block,
            1.0
        );
        assert_eq!(
            CombatTrainingProfile::from_equipped_hands([item(true, false, false, f32::NAN)]).block,
            0.0
        );
    }

    #[test]
    fn combat_training_plus_raiding_is_bulk_chunk_equivalent() {
        let schedule = DailySchedule {
            combat_training_minutes: 180,
            raiding: 240,
            ..Default::default()
        };
        let profile = ActivityTrainingProfile {
            combat: CombatTrainingProfile {
                weapons: WeaponSkillDistribution {
                    sword: 1.0,
                    bow: 1.0,
                    ..Default::default()
                },
                dodge: 1.0,
                block: 0.6,
                balance: 1.0,
                will: 1.0,
            },
        };
        let mut bulk = SkillHours {
            sword: 8.0,
            bow: 2.0,
            ..Default::default()
        };
        let mut chunked = bulk;
        apply_schedule_training(&mut bulk, schedule, 30 * MINUTES_PER_DAY, profile);
        for _ in 0..30 {
            apply_schedule_training(&mut chunked, schedule, MINUTES_PER_DAY, profile);
        }
        for (left, right) in bulk.values().into_iter().zip(chunked.values()) {
            assert!((left - right).abs() < 0.002, "{left} != {right}");
        }
    }

    #[test]
    fn chunked_and_daily_progression_agree() {
        let schedule = DailySchedule {
            combat_training_minutes: 120,
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
    fn religion_training_comes_only_from_prayer_activity() {
        let mut hours = ReligionHours::default();
        apply_religion_training(
            &mut hours,
            MINUTES_PER_DAY,
            Some(OfficialReligion::Lutheran),
            60,
        );
        assert_eq!(hours.lutheran, 0.25);
        assert_eq!(hours.total_direct(), 0.25);
        assert!(hours.effective(OfficialReligion::RomanCatholic) > 0.0);

        let mut meditation = ReligionHours::default();
        apply_religion_training(&mut meditation, MINUTES_PER_DAY, None, 60);
        assert_eq!(meditation.total_direct(), 0.0);
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
            combat_training_minutes: 18 * 60,
            ..Default::default()
        };
        assert_eq!(
            settlement_leisure_outcome(six_hours, MINUTES_PER_DAY, 0.0).fatigue_delta,
            0.0
        );
        let five_hours = DailySchedule {
            combat_training_minutes: 19 * 60,
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
            combat_training_minutes: 12 * 60,
            ..Default::default()
        };
        let offset = settlement_leisure_outcome(exactly_offsets_labor, MINUTES_PER_DAY, 0.0);
        assert_eq!(offset.leisure_hours, 8.0);
        assert_eq!(offset.fatigue_delta, 0.0);
        assert_eq!(offset.morale, 0.0);

        let surplus = settlement_leisure_outcome(
            DailySchedule {
                combat_training_minutes: 15 * 60,
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
            combat_training_minutes: 16 * 60,
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
            combat_training_minutes: 15 * 60,
            ..Default::default()
        };
        let daily = settlement_leisure_outcome(schedule, MINUTES_PER_DAY, 0.0);
        let hourly = settlement_leisure_outcome(schedule, 60, 0.0);

        assert!((daily.morale - hourly.morale * 24.0).abs() < 0.001);
    }

    #[test]
    fn leisure_morale_crossing_carried_fatigue_is_partition_independent() {
        let schedule = DailySchedule {
            combat_training_minutes: 16 * 60,
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
            combat_training_minutes: 12 * 60,
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
