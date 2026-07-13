//! Shared automatic capability tags used by strategic recruitment and tactical UI.

use serde::{Deserialize, Serialize};

use crate::prelude::*;

pub const HEAVY_WEAPON_MIN_WEIGHT: f32 = 4.0;
pub const HEAVY_WEAPON_MIN_ARM_STRENGTH: f32 = 3.0;
pub const ARMORED_MIN_AVERAGE_COVERAGE: f32 = 0.25;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterCapabilities {
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub armored: bool,
    pub shield: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub climb: f32,
    pub swim: f32,
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
    pub armored: bool,
    pub shield: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub climb: u8,
    pub swim: u8,
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
            && (!requirements.armored || self.armored)
            && (!requirements.shield || self.shield)
            && (!requirements.blunt || self.blunt)
            && (!requirements.slash || self.slash)
            && (!requirements.pierce || self.pierce)
            && rating(self.climb) >= requirements.climb
            && rating(self.swim) >= requirements.swim
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
    let average_coverage = BodyPart::FULL_BODY
        .iter()
        .map(|part| equipment.armor_coverage(part))
        .sum::<f32>()
        / BodyPart::FULL_BODY.len() as f32;

    CharacterCapabilities {
        melee: equipment.weapon_is_melee(),
        ranged: equipment.weapon_is_ranged(),
        precise: equipment.weapon_is_precise(),
        heavy: equipment.weapon_weight() >= HEAVY_WEAPON_MIN_WEIGHT
            && arm_strength >= HEAVY_WEAPON_MIN_ARM_STRENGTH,
        armored: average_coverage >= ARMORED_MIN_AVERAGE_COVERAGE,
        shield: equipment.shield_block_bonus() > 0.0,
        blunt: equipment.weapon_does_blunt(),
        slash: equipment.weapon_does_slash(),
        pierce: equipment.weapon_does_pierce(),
        climb: climb.clamp(0.0, 5.0),
        swim: swim.clamp(0.0, 5.0),
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

    #[test]
    fn recommendations_combine_tags_and_rounded_ratings() {
        let capabilities = CharacterCapabilities {
            melee: true,
            armored: true,
            endurance: 2.49,
            medicine: 3.5,
            ..Default::default()
        };
        assert!(capabilities.meets(RoleRequirements {
            melee: true,
            armored: true,
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
}
