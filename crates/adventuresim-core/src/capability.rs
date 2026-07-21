//! Shared automatic capability tags used by strategic recruitment and tactical UI.

use serde::{Deserialize, Serialize};

use crate::prelude::*;

pub const HEAVY_WEAPON_MIN_WEIGHT: f32 = 4.0;
pub const HEAVY_WEAPON_MIN_ARM_STRENGTH: f32 = 3.0;
pub const DEFAULT_NUMERIC_REQUIREMENT: u8 = 3;
pub const PARTY_CHARISMA_BASELINE: f32 = 2.5;
pub const PARTY_CHARISMA_SUPPORT_WEIGHT: f32 = 0.5;
pub const PARTY_CHARISMA_COORDINATION_LIMIT: f32 = 1.125;
pub const PARTY_CHARISMA_COORDINATION_DECAY: f32 = 1.0 / 3.0;
pub const MAX_PARTY_CHECK: f32 = 5.0;
/// Each medical supporter has half the influence of the previous contributor.
pub const BOUNDED_PARTY_CHECK_SUPPORT_DECAY: f32 = 0.5;

/// Combines party-wide skill checks with diminishing returns. The strongest
/// contributor counts fully, the next at half value, the third at one third,
/// and so on, matching the aggregate check described by the morale design.
pub fn aggregate_party_check(values: impl IntoIterator<Item = f32>) -> f32 {
    let mut values: Vec<f32> = values
        .into_iter()
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect();
    values.sort_by(|left, right| right.total_cmp(left));
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| value / (index + 1) as f32)
        .sum()
}

/// The authoritative trained-mental Religion check for one tradition.
/// Religion has no equipment or fatigue term, so the public strategic fields
/// are sufficient to reproduce the same check for a schedule preview.
pub fn religion_knowledge_check(
    effective_hours: f32,
    instinct: f32,
    intelligence: f32,
    focus: f32,
    head_health: f32,
) -> f32 {
    let health = if head_health.is_finite() {
        head_health.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let attribute_check = health * (instinct + intelligence * focus);
    Skill::Religion
        .training_rank(effective_hours)
        .min(attribute_check)
        .max(0.0)
}

pub fn aggregate_party_contribution(current: &[f32], candidate: f32) -> f32 {
    let before = aggregate_party_check(current.iter().copied());
    let after = aggregate_party_check(current.iter().copied().chain([candidate]));
    (after - before).max(0.0)
}

/// Combines a lead practitioner's check with geometrically diminishing support.
///
/// Values are sorted strongest-first. The leader has full weight, followed by
/// weights of 1/2, 1/4, 1/8, and so on. Combining the contributors as
/// complementary fractions of the five-point scale keeps the result bounded
/// without clamping it. With the 1/2 decay, all supporters together can have
/// at most the influence of one additional copy of the leader.
pub fn aggregate_bounded_party_check(values: impl IntoIterator<Item = f32>) -> f32 {
    let mut values: Vec<f32> = values
        .into_iter()
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.min(MAX_PARTY_CHECK))
        .collect();
    values.sort_by(|left, right| right.total_cmp(left));

    let unfilled_fraction = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let weight = BOUNDED_PARTY_CHECK_SUPPORT_DECAY.powi(index as i32);
            (1.0 - value / MAX_PARTY_CHECK).powf(weight)
        })
        .product::<f32>();
    MAX_PARTY_CHECK * (1.0 - unfilled_fraction)
}

pub fn aggregate_bounded_party_contribution(current: &[f32], candidate: f32) -> f32 {
    let before = aggregate_bounded_party_check(current.iter().copied());
    let after = aggregate_bounded_party_check(current.iter().copied().chain([candidate]));
    (after - before).max(0.0)
}

/// Aggregate Charisma as a lead speaker supported (or burdened) by the party.
///
/// The strongest member establishes the base check. Additional members provide
/// a rapidly saturating coordination benefit, then help or hinder according to
/// how far their individual check is above or below the neutral 2.5 baseline.
pub fn aggregate_party_charisma(values: impl IntoIterator<Item = f32>) -> f32 {
    let mut values: Vec<f32> = values
        .into_iter()
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 5.0))
        .collect();
    values.sort_by(|left, right| right.total_cmp(left));
    let Some((&leader, supporters)) = values.split_first() else {
        return 0.0;
    };
    if supporters.is_empty() {
        return leader;
    }
    let coordination = PARTY_CHARISMA_COORDINATION_LIMIT
        * (1.0 - PARTY_CHARISMA_COORDINATION_DECAY.powi(supporters.len() as i32));
    let support: f32 = supporters
        .iter()
        .map(|value| PARTY_CHARISMA_SUPPORT_WEIGHT * (value - PARTY_CHARISMA_BASELINE))
        .sum();
    (leader + coordination + support).clamp(0.0, 5.0)
}

pub fn aggregate_party_charisma_contribution(current: &[f32], candidate: f32) -> f32 {
    aggregate_party_charisma(current.iter().copied().chain([candidate]))
        - aggregate_party_charisma(current.iter().copied())
}
pub const FULL_ARMOR_MIN_REGION_COVERAGE: f32 = 0.75;
pub const WEAPON_PRECISION_CLUB: f32 = 0.5;
pub const WEAPON_PRECISION_AXE: f32 = 1.0;
pub const WEAPON_PRECISION_SWORD: f32 = 1.5;
pub const WEAPON_PRECISION_RAPIER: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterCapabilities {
    pub melee: bool,
    pub ranged: bool,
    pub weapon_precision: f32,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub athletics: f32,
    pub endurance: f32,
    pub medicine: f32,
    pub surgery: f32,
    pub charisma: f32,
    pub religion: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoleRequirements {
    pub melee: bool,
    pub ranged: bool,
    pub weapon_precision: f32,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub athletics: u8,
    pub endurance: u8,
    pub medicine: u8,
    pub surgery: u8,
    pub charisma: u8,
    pub religion: u8,
}

impl CharacterCapabilities {
    pub fn meets(self, requirements: RoleRequirements) -> bool {
        (!requirements.melee || self.melee)
            && (!requirements.ranged || self.ranged)
            && self.weapon_precision >= requirements.weapon_precision
            && (!requirements.heavy || self.heavy)
            && (!requirements.quarter_armor || self.quarter_armor)
            && (!requirements.half_armor || self.half_armor)
            && (!requirements.three_quarter_armor || self.three_quarter_armor)
            && (!requirements.full_armor || self.full_armor)
            && rating(self.athletics) >= requirements.athletics
            && rating(self.endurance) >= requirements.endurance
            && rating(self.medicine) >= requirements.medicine
            && rating(self.surgery) >= requirements.surgery
            && rating(self.charisma) >= requirements.charisma
            && rating(self.religion) >= requirements.religion
    }
}

pub fn rating(value: f32) -> u8 {
    value.round().clamp(0.0, 5.0) as u8
}

pub fn weapon_precision_tier_label(value: f32) -> Option<&'static str> {
    if value >= WEAPON_PRECISION_RAPIER {
        Some("Rapier/bodkin precision")
    } else if value >= WEAPON_PRECISION_SWORD {
        Some("Sword/spear precision")
    } else if value >= WEAPON_PRECISION_AXE {
        Some("Axe precision")
    } else if value >= WEAPON_PRECISION_CLUB {
        Some("Club/hammer precision")
    } else {
        None
    }
}

pub fn legacy_weapon_precision(precise: bool, blunt: bool, slash: bool, pierce: bool) -> f32 {
    if precise {
        WEAPON_PRECISION_RAPIER
    } else if slash || pierce {
        WEAPON_PRECISION_SWORD
    } else if blunt {
        WEAPON_PRECISION_CLUB
    } else {
        0.0
    }
}

pub fn armor_tiers(equipment: &impl PlayerEquipment) -> (bool, bool, bool, bool) {
    let protected = |part| equipment.armor_coverage(part) > 0.0;
    let cuirass = protected(BodyPart::Chest) && protected(BodyPart::Stomach);
    let helmet = protected(BodyPart::Head);
    let shield = equipment.shield_block_bonus() > 0.0;
    let both_arms = protected(BodyPart::LeftArm) && protected(BodyPart::RightArm);
    let both_legs = protected(BodyPart::LeftLeg) && protected(BodyPart::RightLeg);
    // A pikeman's half armor adds tassets to helmet and cuirass; a shield is the
    // alternate lighter loadout. Three-quarter armor adds complete arm defenses
    // and thigh/knee defenses. Until the anatomy distinguishes upper and lower
    // limbs, high per-region coverage is the proxy for a head-to-toe harness.
    let quarter = (cuirass && helmet) || shield;
    let half = cuirass && helmet && (both_legs || shield);
    let three_quarter = cuirass && helmet && both_arms && both_legs;
    let full = three_quarter
        && BodyPart::FULL_BODY
            .iter()
            .all(|part| equipment.armor_coverage(part) >= FULL_ARMOR_MIN_REGION_COVERAGE);
    (quarter, half, three_quarter, full)
}

pub fn evaluate_capabilities(
    attributes: &impl PlayerAttributes,
    body: &impl PlayerBody,
    essentials: &impl PlayerEssentials,
    equipment: &impl PlayerEquipment,
    skills: &impl PlayerSkills,
) -> CharacterCapabilities {
    let arm_strength = attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Strength,
        body,
        LimbWeights::both_arms(),
    );
    let arm_agility = attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Agility,
        body,
        LimbWeights::both_arms(),
    );
    let limb_agility = attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Agility,
        body,
        LimbWeights::all_equal(),
    );
    let endurance = attributes.attr_by_parts(SimpleAttribute::Endurance, body);
    let encumbrance = equipment.encumbrance_penalty_by_parts(attributes, body);
    let climb = ((arm_strength + arm_agility) * 0.5)
        * equipment.armor_penalty(BodyPart::UPPER_BODY)
        * encumbrance;
    let swim = ((endurance + limb_agility) * 0.5)
        * equipment.armor_penalty(BodyPart::FULL_BODY)
        * encumbrance;
    let (quarter_armor, half_armor, three_quarter_armor, full_armor) = armor_tiers(equipment);

    CharacterCapabilities {
        melee: equipment.weapon_is_melee(),
        ranged: equipment.weapon_is_ranged(),
        weapon_precision: equipment.weapon_accuracy().max(0.0),
        heavy: equipment.weapon_weight() >= HEAVY_WEAPON_MIN_WEIGHT
            && arm_strength >= HEAVY_WEAPON_MIN_ARM_STRENGTH,
        quarter_armor,
        half_armor,
        three_quarter_armor,
        full_armor,
        athletics: ((climb + swim) * 0.5).clamp(0.0, 5.0),
        endurance: endurance.clamp(0.0, 5.0),
        medicine: skills
            .skill_check_by_parts(
                Skill::Medicine,
                attributes,
                body,
                essentials,
                equipment,
                LimbWeights::all_equal(),
            )
            .clamp(0.0, 5.0),
        surgery: ([Skill::Anatomy, Skill::Knife, Skill::Tailoring]
            .into_iter()
            .map(|skill| {
                skills.skill_check_by_parts(
                    skill,
                    attributes,
                    body,
                    essentials,
                    equipment,
                    LimbWeights::both_arms(),
                )
            })
            .sum::<f32>()
            / 3.0)
            .clamp(0.0, 5.0),
        charisma: skills
            .skill_check_by_parts(
                Skill::Charisma,
                attributes,
                body,
                essentials,
                equipment,
                LimbWeights::all_equal(),
            )
            .clamp(0.0, 5.0),
        religion: skills
            .skill_check_by_parts(
                Skill::Religion,
                attributes,
                body,
                essentials,
                equipment,
                LimbWeights::all_equal(),
            )
            .clamp(0.0, 5.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestArmor {
        coverage: [f32; 7],
        shield: bool,
    }

    impl PlayerEquipment for TestArmor {
        fn weapon_accuracy(&self) -> f32 {
            0.0
        }
        fn weapon_weight(&self) -> f32 {
            0.0
        }
        fn weapon_penetration(&self) -> f32 {
            0.0
        }
        fn weapon_reach(&self) -> f32 {
            0.0
        }
        fn weapon_holding_side(&self) -> Option<BodySide> {
            None
        }
        fn weapon_is_precise(&self) -> bool {
            false
        }
        fn weapon_balance(&self) -> f32 {
            0.0
        }
        fn shield_block_bonus(&self) -> f32 {
            if self.shield { 1.0 } else { 0.0 }
        }
        fn armor_resistance(&self, _part: BodyPart) -> f32 {
            0.0
        }
        fn armor_padding(&self, _part: BodyPart) -> f32 {
            0.0
        }
        fn armor_flexibility(&self, _part: BodyPart) -> f32 {
            1.0
        }
        fn armor_range_of_motion(&self, _part: BodyPart) -> f32 {
            1.0
        }
        fn armor_coverage(&self, part: BodyPart) -> f32 {
            self.coverage[match part {
                BodyPart::LeftArm => 0,
                BodyPart::RightArm => 1,
                BodyPart::LeftLeg => 2,
                BodyPart::RightLeg => 3,
                BodyPart::Chest => 4,
                BodyPart::Stomach => 5,
                BodyPart::Head => 6,
            }]
        }
        fn inventory_weight(&self) -> f32 {
            0.0
        }
    }

    #[test]
    fn recommendations_combine_tags_and_rounded_ratings() {
        assert_eq!(
            legacy_weapon_precision(true, true, true, true),
            WEAPON_PRECISION_RAPIER
        );
        assert_eq!(
            legacy_weapon_precision(false, false, true, false),
            WEAPON_PRECISION_SWORD
        );
        assert_eq!(
            legacy_weapon_precision(false, true, false, false),
            WEAPON_PRECISION_CLUB
        );
        let capabilities = CharacterCapabilities {
            melee: true,
            weapon_precision: WEAPON_PRECISION_SWORD,
            quarter_armor: true,
            endurance: 2.49,
            medicine: 3.5,
            ..Default::default()
        };
        assert!(capabilities.meets(RoleRequirements {
            melee: true,
            weapon_precision: WEAPON_PRECISION_AXE,
            quarter_armor: true,
            endurance: 2,
            medicine: 4,
            ..Default::default()
        }));
        assert!(!capabilities.meets(RoleRequirements {
            ranged: true,
            ..Default::default()
        }));
        assert!(!capabilities.meets(RoleRequirements {
            weapon_precision: WEAPON_PRECISION_RAPIER,
            ..Default::default()
        }));
        assert!(!capabilities.meets(RoleRequirements {
            endurance: 3,
            ..Default::default()
        }));
    }

    #[test]
    fn armor_tiers_allow_a_shield_or_require_increasing_body_coverage() {
        let no_cuirass = TestArmor {
            coverage: [0.9; 7],
            shield: true,
        };
        let mut no_cuirass = no_cuirass;
        no_cuirass.coverage[4] = 0.0;
        assert_eq!(armor_tiers(&no_cuirass), (true, false, false, false));

        let quarter_by_shield = TestArmor {
            coverage: [0.0, 0.0, 0.0, 0.0, 0.4, 0.4, 0.0],
            shield: true,
        };
        assert_eq!(armor_tiers(&quarter_by_shield), (true, false, false, false));

        let helmet_without_cuirass = TestArmor {
            coverage: [0.9, 0.9, 0.9, 0.9, 0.0, 0.0, 0.9],
            shield: false,
        };
        assert_eq!(
            armor_tiers(&helmet_without_cuirass),
            (false, false, false, false)
        );

        let three_quarter = TestArmor {
            coverage: [0.4; 7],
            shield: false,
        };
        assert_eq!(armor_tiers(&three_quarter), (true, true, true, false));

        let full = TestArmor {
            coverage: [0.9; 7],
            shield: false,
        };
        assert_eq!(armor_tiers(&full), (true, true, true, true));
    }

    #[test]
    fn aggregate_checks_sort_contributors_and_apply_diminishing_returns() {
        let current = [2.0, 4.0, 3.0];
        assert!((aggregate_party_check(current) - (4.0 + 1.5 + 2.0 / 3.0)).abs() < 0.001);
        assert!((aggregate_party_contribution(&current, 5.0) - 7.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn religion_knowledge_check_is_bounded_by_training_and_healthy_focus() {
        assert_eq!(religion_knowledge_check(0.0, 5.0, 5.0, 1.0, 1.0), 0.0);
        let trained = Skill::Religion.training_rank(2_500.0);
        assert!((religion_knowledge_check(2_500.0, 5.0, 5.0, 1.0, 1.0) - trained).abs() < 0.001);
        assert!((religion_knowledge_check(100_000.0, 1.0, 1.0, 0.5, 0.5) - 0.75).abs() < 0.001);
    }

    #[test]
    fn bounded_checks_approach_quality_dependent_limits() {
        for (check, expected_limit) in [(1.0, 1.8), (2.0, 3.2), (3.0, 4.2), (4.0, 4.8)] {
            let large_party = aggregate_bounded_party_check([check; 64]);
            assert!((large_party - expected_limit).abs() < 0.001);
            assert!(large_party < MAX_PARTY_CHECK);
        }
    }

    #[test]
    fn bounded_checks_preserve_solo_values_and_reward_skilled_support() {
        assert!((aggregate_bounded_party_check([3.25]) - 3.25).abs() < 0.001);
        assert!((aggregate_bounded_party_check([4.0, 4.0]) - 4.5528).abs() < 0.001);
        assert!(aggregate_bounded_party_check([4.0, 2.0, 2.0, 2.0]) < 4.4);
        assert!(aggregate_bounded_party_contribution(&[4.0], 4.0) > 0.5);
    }

    #[test]
    fn party_charisma_matches_target_compositions() {
        for (party, expected) in [
            (vec![4.5], 4.5),
            (vec![3.0, 3.0, 3.0], 4.5),
            (vec![4.0, 2.0], 4.5),
        ] {
            assert!((aggregate_party_charisma(party) - expected).abs() < 0.001);
        }
    }

    #[test]
    fn low_charisma_party_size_cannot_brute_force_a_high_check() {
        assert!(aggregate_party_charisma([1.0; 100]) < 1.0);
        assert!(aggregate_party_charisma([2.0; 100]) < 1.0);
        assert!(aggregate_party_charisma([4.0; 1].into_iter().chain([2.0; 10])) < 3.0);
    }
}
