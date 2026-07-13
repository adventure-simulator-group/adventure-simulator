//! Shared automatic capability tags used by strategic recruitment and tactical UI.

use serde::{Deserialize, Serialize};

use crate::prelude::*;

pub const HEAVY_WEAPON_MIN_WEIGHT: f32 = 4.0;
pub const HEAVY_WEAPON_MIN_ARM_STRENGTH: f32 = 3.0;
pub const DEFAULT_NUMERIC_REQUIREMENT: u8 = 3;
pub const FULL_ARMOR_MIN_REGION_COVERAGE: f32 = 0.75;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterCapabilities {
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: f32,
    pub endurance: f32,
    pub medicine: f32,
    pub surgery: f32,
    pub charisma: f32,
    pub faith: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoleRequirements {
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: u8,
    pub endurance: u8,
    pub medicine: u8,
    pub surgery: u8,
    pub charisma: u8,
    pub faith: u8,
}

impl CharacterCapabilities {
    pub fn meets(self, requirements: RoleRequirements) -> bool {
        (!requirements.melee || self.melee)
            && (!requirements.ranged || self.ranged)
            && (!requirements.precise || self.precise)
            && (!requirements.heavy || self.heavy)
            && (!requirements.quarter_armor || self.quarter_armor)
            && (!requirements.half_armor || self.half_armor)
            && (!requirements.three_quarter_armor || self.three_quarter_armor)
            && (!requirements.full_armor || self.full_armor)
            && (!requirements.blunt || self.blunt)
            && (!requirements.slash || self.slash)
            && (!requirements.pierce || self.pierce)
            && rating(self.athletics) >= requirements.athletics
            && rating(self.endurance) >= requirements.endurance
            && rating(self.medicine) >= requirements.medicine
            && rating(self.surgery) >= requirements.surgery
            && rating(self.charisma) >= requirements.charisma
            && rating(self.faith) >= requirements.faith
    }
}

pub fn rating(value: f32) -> u8 {
    value.round().clamp(0.0, 5.0) as u8
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
    let quarter = cuirass && (helmet || shield);
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
        precise: equipment.weapon_is_precise(),
        heavy: equipment.weapon_weight() >= HEAVY_WEAPON_MIN_WEIGHT
            && arm_strength >= HEAVY_WEAPON_MIN_ARM_STRENGTH,
        quarter_armor,
        half_armor,
        three_quarter_armor,
        full_armor,
        blunt: equipment.weapon_does_blunt(),
        slash: equipment.weapon_does_slash(),
        pierce: equipment.weapon_does_pierce(),
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
        surgery: skills
            .skill_check_by_parts(
                Skill::Surgeon,
                attributes,
                body,
                essentials,
                equipment,
                LimbWeights::both_arms(),
            )
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
        faith: skills
            .skill_check_by_parts(
                Skill::Faith,
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
        let capabilities = CharacterCapabilities {
            melee: true,
            quarter_armor: true,
            endurance: 2.49,
            medicine: 3.5,
            ..Default::default()
        };
        assert!(capabilities.meets(RoleRequirements {
            melee: true,
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
            endurance: 3,
            ..Default::default()
        }));
    }

    #[test]
    fn armor_tiers_require_a_cuirass_and_increasing_body_coverage() {
        let no_cuirass = TestArmor {
            coverage: [0.9; 7],
            shield: true,
        };
        let mut no_cuirass = no_cuirass;
        no_cuirass.coverage[4] = 0.0;
        assert_eq!(armor_tiers(&no_cuirass), (false, false, false, false));

        let quarter = TestArmor {
            coverage: [0.0, 0.0, 0.0, 0.0, 0.4, 0.4, 0.0],
            shield: true,
        };
        assert_eq!(armor_tiers(&quarter), (true, false, false, false));

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
}
