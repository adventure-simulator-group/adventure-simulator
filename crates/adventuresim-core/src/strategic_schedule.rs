//! Pure settlement schedule progression shared by the authoritative module and tools.

use crate::attribute::PlayerAttributes;
use crate::equipment::WeaponSkillDistribution;
use crate::organization::{OrganizationDefinition, TrainingTarget};
use crate::skill::{Skill, apply_direct_training};
use crate::{
    activity::*,
    strategic_time::{MINUTES_PER_DAY, training_hours_increment},
};
use adventuresim_world_schema::{BestiaryHours, OfficialReligion, ReligionHours};

/// Stable order used by reports and schedule arrays.
pub const SKILL_COUNT: usize = 32;
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
    pub insight: f32,
    pub charm: f32,
    pub command: f32,
    pub deception: f32,
    pub physiology: f32,
    pub cooking: f32,
    pub herbalism: f32,
    pub religion: ReligionHours,
    pub bestiary: BestiaryHours,
    pub stealth: f32,
    pub balance: f32,
    pub surgery: f32,
    pub terrain_plains: f32,
    pub terrain_forest: f32,
    pub terrain_hills: f32,
    pub terrain_wetlands: f32,
    pub terrain_urban: f32,
    pub terrain_snow: f32,
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
            self.insight,
            self.charm,
            self.command,
            self.deception,
            self.physiology,
            self.cooking,
            self.herbalism,
            self.religion.total_direct(),
            self.bestiary.total_direct(),
            self.stealth,
            self.balance,
            self.surgery,
            self.terrain_plains,
            self.terrain_forest,
            self.terrain_hills,
            self.terrain_wetlands,
            self.terrain_urban,
            self.terrain_snow,
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
            && self
                .bestiary
                .direct_values()
                .all(|(_, hours)| hours.is_finite())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailySchedule {
    /// Quiet study from a personally carried book or an on-site bookstore.
    pub reading_minutes: u16,
    /// Structured combat practice, including weapon drills, will, and balance.
    pub combat_training_minutes: u16,
    /// Social recreation which trains Charm at the activity rate.
    pub carousing_minutes: u16,
    /// Deliberate conversation with one selected relationship target. Its
    /// relationship resolution is strategic-module state, but it consumes
    /// discretionary time and therefore cannot also be restorative Leisure.
    pub socializing_minutes: u16,
    /// Supervised work in an unlocked profession.
    pub apprenticeship_minutes: u16,
    /// Independent paid professional work, available at Journeyman rank.
    pub profession_practice_minutes: u16,
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
            self.socializing_minutes,
            self.apprenticeship_minutes,
            self.profession_practice_minutes,
            self.reading_minutes,
        ]
        .into_iter()
        .map(u64::from)
        .sum()
    }
}

/// Deterministically projects the unallocated share of a repeating daily
/// schedule onto an absolute interval. Cumulative integer arithmetic makes
/// adjacent chunks telescope exactly without persisting a fractional remainder.
pub fn restorative_leisure_minutes(
    schedule: DailySchedule,
    interval_start_minute: u64,
    elapsed_minutes: u64,
) -> u64 {
    let leisure = MINUTES_PER_DAY.saturating_sub(schedule.allocated_minutes());
    let cumulative = |minute: u64| {
        minute
            .saturating_mul(leisure)
            .checked_div(MINUTES_PER_DAY)
            .unwrap_or(0)
    };
    cumulative(interval_start_minute.saturating_add(elapsed_minutes))
        .saturating_sub(cumulative(interval_start_minute))
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
    attributes: &impl PlayerAttributes,
) -> f32 {
    let mut excess = 0.0;
    let mut award = |stored: &mut f32, skill: Skill, real_hours: f32| {
        excess +=
            apply_direct_training(skill, stored, real_hours, attributes).excess_effective_hours;
    };
    let increment = |minutes| training_hours_increment(elapsed_minutes, minutes);
    award(
        &mut skills.charm,
        Skill::Charm,
        increment(schedule.carousing_minutes) * ACTIVITY_TRAINING_RATE,
    );
    award(
        &mut skills.will,
        Skill::Will,
        increment(schedule.labor) * ACTIVITY_TRAINING_RATE,
    );
    award(
        &mut skills.stealth,
        Skill::Stealth,
        increment(schedule.thievery) * ACTIVITY_TRAINING_RATE,
    );
    // Combat Training and Raiding conserve their activity training budget
    // across equipped weapon leaves and all four Defense leaves.
    let combat_training_hours = increment(schedule.combat_training_minutes)
        + increment(schedule.raiding) * ACTIVITY_TRAINING_RATE;
    if combat_training_hours > 0.0 {
        let combat_weights = profile.combat.weights();
        let total_weight = combat_weights.into_iter().sum::<f32>();
        for ((hours, skill), weight) in [
            (&mut skills.polearm, Skill::Polearm),
            (&mut skills.axe, Skill::Axe),
            (&mut skills.bludgeon, Skill::Bludgeon),
            (&mut skills.sword, Skill::Sword),
            (&mut skills.knife, Skill::Knife),
            (&mut skills.bow, Skill::Bow),
            (&mut skills.crossbow, Skill::Crossbow),
            (&mut skills.firearm, Skill::Firearm),
            (&mut skills.throw, Skill::Throw),
            (&mut skills.dodge, Skill::Dodge),
            (&mut skills.block, Skill::Block),
            (&mut skills.balance, Skill::Balance),
            (&mut skills.will, Skill::Will),
        ]
        .into_iter()
        .zip(combat_weights)
        {
            award(hours, skill, combat_training_hours * weight / total_weight);
        }
    }
    excess
}

/// Apply prayer activity training after the caller resolves the settlement tradition.
pub fn apply_religion_training(
    religion_hours: &mut ReligionHours,
    elapsed_minutes: u64,
    prayer_religion: Option<OfficialReligion>,
    prayer_minutes: u16,
    attributes: &impl PlayerAttributes,
) -> f32 {
    if let Some(religion) = prayer_religion {
        return apply_direct_training(
            Skill::Religion,
            religion_hours.direct_mut(religion),
            training_hours_increment(elapsed_minutes, prayer_minutes) * ACTIVITY_TRAINING_RATE,
            attributes,
        )
        .excess_effective_hours;
    }
    0.0
}

/// Apply one organization's authored curriculum using the same aptitude-aware
/// learning and caps as ordinary schedule training. Written-language leaves
/// are returned to the caller because they are intentionally stored outside
/// [`SkillHours`].
pub fn apply_organization_training(
    hours: &mut SkillHours,
    work_hours: f32,
    definition: &OrganizationDefinition,
    activities: ActivityTrainingProfile,
    attributes: &impl PlayerAttributes,
) -> (f32, Vec<(adventuresim_world_schema::WrittenLanguage, f32)>) {
    apply_curriculum_training(
        hours,
        work_hours,
        &definition.activity.training,
        activities,
        attributes,
    )
}

/// Apply authored curriculum entries without requiring database or clock state.
pub fn apply_curriculum_training(
    hours: &mut SkillHours,
    work_hours: f32,
    entries: &[crate::organization::TrainingEntry],
    activities: ActivityTrainingProfile,
    attributes: &impl PlayerAttributes,
) -> (f32, Vec<(adventuresim_world_schema::WrittenLanguage, f32)>) {
    let mut excess = 0.0;
    let mut written = Vec::new();
    let mut award_direct = |skill: Skill, stored: &mut f32, real_hours: f32| {
        excess +=
            apply_direct_training(skill, stored, real_hours, attributes).excess_effective_hours;
    };
    for entry in entries {
        let award = work_hours * entry.weight;
        match &entry.target {
            TrainingTarget::FixedSkill { skill } => {
                if let Some((kind, stored)) = fixed_skill_target(hours, skill) {
                    award_direct(kind, stored, award);
                }
            }
            TrainingTarget::Religion { religion } => {
                if let Some(religion) = OfficialReligion::from_id(religion) {
                    award_direct(Skill::Religion, hours.religion.direct_mut(religion), award);
                }
            }
            TrainingTarget::Bestiary { category } => {
                if let Some(category) = adventuresim_world_schema::BestiaryCategory::ALL
                    .into_iter()
                    .find(|value| format!("{value:?}").eq_ignore_ascii_case(category))
                {
                    award_direct(Skill::Bestiary, hours.bestiary.direct_mut(category), award);
                }
            }
            TrainingTarget::Terrain { terrain } => {
                if let Some((kind, stored)) = terrain_target(hours, terrain) {
                    award_direct(kind, stored, award);
                }
            }
            TrainingTarget::EquippedWeaponSkills => {
                let weights = activities.combat.weapons.weights();
                let total = weights.into_iter().sum::<f32>();
                if total > 0.0 {
                    for ((skill, target), weight) in [
                        (Skill::Polearm, &mut hours.polearm),
                        (Skill::Axe, &mut hours.axe),
                        (Skill::Bludgeon, &mut hours.bludgeon),
                        (Skill::Sword, &mut hours.sword),
                        (Skill::Knife, &mut hours.knife),
                        (Skill::Bow, &mut hours.bow),
                        (Skill::Crossbow, &mut hours.crossbow),
                        (Skill::Firearm, &mut hours.firearm),
                        (Skill::Throw, &mut hours.throw),
                    ]
                    .into_iter()
                    .zip(weights)
                    {
                        award_direct(skill, target, award * weight / total);
                    }
                }
            }
            TrainingTarget::Written { language } => written.push((*language, award)),
        }
    }
    (excess, written)
}

fn fixed_skill_target<'a>(hours: &'a mut SkillHours, skill: &str) -> Option<(Skill, &'a mut f32)> {
    Some(match skill {
        "will" => (Skill::Will, &mut hours.will),
        "insight" => (Skill::Insight, &mut hours.insight),
        "charm" => (Skill::Charm, &mut hours.charm),
        "command" => (Skill::Command, &mut hours.command),
        "deception" => (Skill::Deception, &mut hours.deception),
        "physiology" => (Skill::Physiology, &mut hours.physiology),
        "cooking" => (Skill::Cooking, &mut hours.cooking),
        "herbalism" => (Skill::Herbalism, &mut hours.herbalism),
        "surgery" => (Skill::Surgery, &mut hours.surgery),
        "polearm" => (Skill::Polearm, &mut hours.polearm),
        "axe" => (Skill::Axe, &mut hours.axe),
        "bludgeon" => (Skill::Bludgeon, &mut hours.bludgeon),
        "sword" => (Skill::Sword, &mut hours.sword),
        "knife" => (Skill::Knife, &mut hours.knife),
        "bow" => (Skill::Bow, &mut hours.bow),
        "crossbow" => (Skill::Crossbow, &mut hours.crossbow),
        "firearm" => (Skill::Firearm, &mut hours.firearm),
        "throw" => (Skill::Throw, &mut hours.throw),
        "block" => (Skill::Block, &mut hours.block),
        "dodge" => (Skill::Dodge, &mut hours.dodge),
        "stealth" => (Skill::Stealth, &mut hours.stealth),
        "balance" => (Skill::Balance, &mut hours.balance),
        "terrain_plains" => (Skill::TerrainPlains, &mut hours.terrain_plains),
        "terrain_forest" => (Skill::TerrainForest, &mut hours.terrain_forest),
        "terrain_hills" => (Skill::TerrainHills, &mut hours.terrain_hills),
        "terrain_wetlands" => (Skill::TerrainWetlands, &mut hours.terrain_wetlands),
        "terrain_urban" => (Skill::TerrainUrban, &mut hours.terrain_urban),
        "terrain_snow" => (Skill::TerrainSnow, &mut hours.terrain_snow),
        "tailoring" => (Skill::Tailoring, &mut hours.tailoring),
        "smithing" => (Skill::Smithing, &mut hours.smithing),
        _ => return None,
    })
}

fn terrain_target<'a>(hours: &'a mut SkillHours, terrain: &str) -> Option<(Skill, &'a mut f32)> {
    fixed_skill_target(hours, &format!("terrain_{terrain}"))
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
    pub infamy_gained: f32,
    pub thievery_discovery_chance: f32,
    pub raiding_retaliation_chance: f32,
    pub labor_hours: f32,
    pub thievery_hours: f32,
    pub raiding_hours: f32,
    pub carousing_morale: f32,
    pub carousing_disorder_chance: f32,
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
        infamy_gained: thievery_infamy(
            thievery_hours,
            inputs.population_scale,
            inputs.stealth_check,
        ) + raiding_infamy(raiding_hours),
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
        carousing_disorder_chance: 1.0 - (-0.025 * carousing_hours).exp(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::BodyPart;
    use crate::prelude::{LimbAttribute, SimpleAttribute};
    use crate::strategic_time::MINUTES_PER_DAY;

    struct NeutralAttributes;
    impl PlayerAttributes for NeutralAttributes {
        fn raw_limb_attr(&self, _attr: LimbAttribute, _limb: BodyPart) -> f32 {
            2.5
        }
        fn raw_single_body_part_attr(&self, _attr: SimpleAttribute) -> f32 {
            2.5
        }
    }

    #[test]
    fn new_activities_conserve_training() {
        let mut skills = SkillHours::default();
        let schedule = DailySchedule {
            combat_training_minutes: 360,
            carousing_minutes: 240,
            ..Default::default()
        };
        apply_schedule_training(
            &mut skills,
            schedule,
            MINUTES_PER_DAY,
            ActivityTrainingProfile::default(),
            &NeutralAttributes,
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
        assert!((combat_total - 6.0).abs() < 0.001);
        assert!((skills.charm - 1.0).abs() < 0.001);
        assert_eq!(skills.physiology, 0.0);
        assert_eq!(skills.surgery, 0.0);
        assert_eq!(skills.knife, 0.0);
        assert_eq!(skills.tailoring, 0.0);
    }

    #[test]
    fn restorative_leisure_is_proportional_and_chunk_invariant() {
        let none = DailySchedule::default();
        assert_eq!(restorative_leisure_minutes(none, 0, 1_440), 1_440);
        let full = DailySchedule {
            labor: 1_440,
            ..Default::default()
        };
        assert_eq!(restorative_leisure_minutes(full, 0, 1_440), 0);
        let half = DailySchedule {
            labor: 720,
            ..Default::default()
        };
        assert_eq!(restorative_leisure_minutes(half, 0, 1_440), 720);
        let bulk = restorative_leisure_minutes(half, 17, 1_000);
        let first = restorative_leisure_minutes(half, 17, 333);
        let second = restorative_leisure_minutes(half, 350, 667);
        assert_eq!(bulk, first + second);
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
        apply_schedule_training(
            &mut bulk,
            schedule,
            30 * MINUTES_PER_DAY,
            profile,
            &NeutralAttributes,
        );
        for _ in 0..30 {
            apply_schedule_training(
                &mut chunked,
                schedule,
                MINUTES_PER_DAY,
                profile,
                &NeutralAttributes,
            );
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
            &NeutralAttributes,
        );
        let mut daily = SkillHours::default();
        for _ in 0..30 {
            apply_schedule_training(
                &mut daily,
                schedule,
                MINUTES_PER_DAY,
                ActivityTrainingProfile::default(),
                &NeutralAttributes,
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
            &NeutralAttributes,
        );
        assert_eq!(hours.lutheran, 0.25);
        assert_eq!(hours.total_direct(), 0.25);
        assert_eq!(hours.effective(OfficialReligion::RomanCatholic), 0.0);

        let mut meditation = ReligionHours::default();
        apply_religion_training(
            &mut meditation,
            MINUTES_PER_DAY,
            None,
            60,
            &NeutralAttributes,
        );
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
        assert!(outcome.infamy_gained > 0.0);
        assert!((0.0..=1.0).contains(&outcome.thievery_discovery_chance));
    }

    #[test]
    fn ordinary_carousing_has_incident_risk_but_no_direct_infamy() {
        let outcome = settlement_activity_outcome(
            DailySchedule {
                carousing_minutes: 8 * 60,
                ..Default::default()
            },
            MINUTES_PER_DAY,
            ActivityOutcomeInputs::default(),
        );
        assert_eq!(outcome.infamy_gained, 0.0);
        assert!(outcome.carousing_disorder_chance > 0.0);
        assert!(outcome.carousing_disorder_chance < 1.0);
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
        assert_eq!(aggregate.gold_earned, 38);
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

    #[test]
    fn herbalism_is_in_the_stable_schedule_projection_and_roundtrips() {
        let hours = SkillHours {
            herbalism: 321.0,
            ..Default::default()
        };
        assert_eq!(hours.values().len(), SKILL_COUNT);
        assert!(hours.values().contains(&321.0));
        let json = serde_json::to_string(&hours).unwrap();
        let decoded: SkillHours = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.herbalism, 321.0);
    }
}
