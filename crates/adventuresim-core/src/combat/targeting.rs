use crate::{
    body::{BodyPart, BodySide, LimbWeights, PlayerBody},
    combat::DefenderResponse,
    equipment::PlayerEquipment,
    prelude::{PlayerAttributes, PlayerEssentials},
    skill::{PlayerSkills, Skill},
};

const BODY_PART_CONTACT_WEIGHTS: [(BodyPart, f32); 7] = [
    (BodyPart::LeftArm, 0.12),
    (BodyPart::RightArm, 0.12),
    (BodyPart::LeftLeg, 0.12),
    (BodyPart::RightLeg, 0.12),
    (BodyPart::Chest, 0.22),
    (BodyPart::Stomach, 0.20),
    (BodyPart::Head, 0.10),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeContactLocation {
    pub body_part: BodyPart,
    pub armor_contact: bool,
}

impl MeleeContactLocation {
    pub const fn new(body_part: BodyPart, armor_contact: bool) -> Self {
        Self {
            body_part,
            armor_contact,
        }
    }

    pub(crate) fn for_equipment(body_part: BodyPart, equipment: &impl PlayerEquipment) -> Self {
        Self::new(body_part, equipment.armor_coverage(body_part) > 0.0)
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "combat accuracy receives independent attacker and defender facets"
)]
pub fn melee_attack_value_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    attacker_side: BodySide,
    attack_style: crate::combat_style::MeleeAttackStyle,
    hit_precision: f32,
    flanking: f32,
    defender_response: DefenderResponse,
    defender_skills: &impl PlayerSkills,
    defender_attr: &impl PlayerAttributes,
    defender_body: &impl PlayerBody,
    defender_essentials: &impl PlayerEssentials,
    defender_equip: &impl PlayerEquipment,
) -> f32 {
    let accuracy = melee_attack_accuracy_by_parts(
        attacker_skills,
        attacker_attr,
        attacker_body,
        attacker_essentials,
        attacker_equip,
        attacker_side,
        attack_style,
        hit_precision,
    );
    let defense = match defender_response {
        DefenderResponse::None => 0.0,
        DefenderResponse::Parry { .. } => {
            let block_skill = defender_skills.skill_check_by_parts(
                Skill::Block,
                defender_attr,
                defender_body,
                defender_essentials,
                defender_equip,
                LimbWeights::all_equal(),
            );
            let shield_bonus = defender_equip.shield_block_bonus();
            5.0 * (1.0 - (-(shield_bonus + block_skill) / 2.0).exp())
        }
        DefenderResponse::Dodge { .. } => {
            let dodge_skill = defender_skills.skill_check_by_parts(
                Skill::Dodge,
                defender_attr,
                defender_body,
                defender_essentials,
                defender_equip,
                LimbWeights {
                    left_arm: 1.0,
                    right_arm: 1.0,
                    left_leg: 0.4,
                    right_leg: 0.4,
                }
                .normalize(),
            );
            dodge_skill
                * defender_equip.armor_penalty(BodyPart::FULL_BODY)
                * defender_equip.encumbrance_penalty_by_parts(defender_attr, defender_body)
        }
    } * defender_response.factor()
        * (1.0 - flanking).clamp(0.0, 1.0);
    accuracy - defense
}

#[expect(
    clippy::too_many_arguments,
    reason = "combat accuracy receives independent character facets"
)]
pub fn melee_attack_accuracy_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    attacker_side: BodySide,
    attack_style: crate::combat_style::MeleeAttackStyle,
    hit_precision: f32,
) -> f32 {
    let weights = LimbWeights::arm(attacker_side, attacker_body.primary_side());
    attacker_equip
        .weapon_skill_distribution()
        .weighted_check(|skill| {
            attacker_skills.skill_check_by_parts(
                skill,
                attacker_attr,
                attacker_body,
                attacker_essentials,
                attacker_equip,
                weights,
            )
        })
        * attacker_equip.weapon_melee_precision(attack_style)
        * hit_precision.clamp(0.0, 1.0)
}

#[must_use]
pub fn whole_body_armor_coverage(equipment: &impl PlayerEquipment) -> f32 {
    BODY_PART_CONTACT_WEIGHTS
        .iter()
        .map(|(part, weight)| weight * equipment.armor_coverage(*part).clamp(0.0, 1.0))
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

#[must_use]
pub fn melee_contact_location(
    attack_value: f32,
    attacker: &impl PlayerEquipment,
    defender: &impl PlayerEquipment,
    sample: f32,
) -> MeleeContactLocation {
    let coverage = whole_body_armor_coverage(defender);
    let bypasses_armor = attacker.weapon_is_precise() && attack_value > 1.0 + coverage;
    let armor_contact = coverage > f32::EPSILON && !bypasses_armor;
    let sample = sample.clamp(0.0, 1.0 - f32::EPSILON);
    let total = BODY_PART_CONTACT_WEIGHTS
        .iter()
        .map(|(part, weight)| {
            let coverage = defender.armor_coverage(*part).clamp(0.0, 1.0);
            weight
                * if armor_contact {
                    coverage
                } else {
                    1.0 - coverage
                }
        })
        .sum::<f32>();
    let body_part = if total <= f32::EPSILON {
        weighted_body_part(sample, |_| 1.0)
    } else {
        weighted_body_part(sample, |part| {
            let coverage = defender.armor_coverage(part).clamp(0.0, 1.0);
            if armor_contact {
                coverage
            } else {
                1.0 - coverage
            }
        })
    };
    MeleeContactLocation {
        body_part,
        armor_contact,
    }
}

fn weighted_body_part(sample: f32, factor: impl Fn(BodyPart) -> f32) -> BodyPart {
    let total = BODY_PART_CONTACT_WEIGHTS
        .iter()
        .map(|(part, weight)| weight * factor(*part).max(0.0))
        .sum::<f32>();
    let mut cursor = sample * total;
    for (part, weight) in BODY_PART_CONTACT_WEIGHTS {
        let candidate_weight = weight * factor(part).max(0.0);
        if candidate_weight <= f32::EPSILON {
            continue;
        }
        if cursor < candidate_weight {
            return part;
        }
        cursor -= candidate_weight;
    }
    BodyPart::Head
}
