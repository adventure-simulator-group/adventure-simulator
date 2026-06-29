use std::{f32, ops::Mul};

use serde::{Deserialize, Serialize};

use crate::{
    body::{BodyPart, BodySide, LimbWeights, PlayerBody},
    prelude::{LimbAttribute, PlayerAttributes, PlayerEquipment, PlayerEssentials},
    skill::{PlayerSkills, Skill},
};

const UPPER_MUSCLE_KG_PER_STRENGTH: f32 = 5.0;
const MUSCLE_KG_TO_JOULES: f32 = 2.0;
const UPPER_MUSCLE_KG_TO_PUNCH_KG: f32 = 0.1;
const STAGGER_RESISTANCE_JOULES_PER_KG: f32 = 10.0;

/// Result of resolving a melee attack.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AttackResult {
    ToAttacker {
        balance_damage: f32,
    },
    ToDefender {
        cut_damage: f32,
        blunt_damage: f32,
        balance_damage: f32,
    },
}

impl Mul<f32> for AttackResult {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        match self {
            Self::ToAttacker { balance_damage } => Self::ToAttacker {
                balance_damage: balance_damage * rhs,
            },
            Self::ToDefender {
                cut_damage,
                blunt_damage,
                balance_damage,
            } => Self::ToDefender {
                cut_damage: cut_damage * rhs,
                blunt_damage: blunt_damage * rhs,
                balance_damage: balance_damage * rhs,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DefenderResponse {
    #[default]
    None,
    Parry {
        input_reflex: f32,
    },
    Dodge {
        input_reflex: f32,
    },
}

impl DefenderResponse {
    pub fn factor(&self) -> f32 {
        match self {
            DefenderResponse::None => 1.0,
            &DefenderResponse::Parry { input_reflex } => 2.0 * input_reflex,
            &DefenderResponse::Dodge { input_reflex } => 1.5 * input_reflex,
        }
    }
}

pub fn flanking_from_dir(attacker_dir: (f32, f32), defender_dir: (f32, f32)) -> f32 {
    let dot = (attacker_dir.0 * defender_dir.0) + (attacker_dir.1 * defender_dir.1);

    let lower = -f32::consts::FRAC_1_SQRT_2; // dot of 135°
    let higher = f32::consts::FRAC_1_SQRT_2; // dot of 45°

    1.0 + (dot.clamp(lower, higher) + lower) / (higher - lower)
}

/// Resolve a melee attack between an attacker and a defender.
///
/// `hit_precision` is a value in [0.0, 1.0] where 0.0 is a complete miss
/// and 1.0 is a perfect hit.
///
/// Returns the outcome including damage values. Damage is not yet
/// applied to any body part — the caller is responsible for that.
pub fn resolve_melee_attack_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    attacker_side: BodySide,
    hit_precision: f32,
    flanking: f32,
    defender_body_part: BodyPart,
    defender_response: DefenderResponse,
    defender_skills: &impl PlayerSkills,
    defender_attr: &impl PlayerAttributes,
    defender_body: &impl PlayerBody,
    defender_essentials: &impl PlayerEssentials,
    defender_equip: &impl PlayerEquipment,
) -> AttackResult {
    // (1) Calculate accuracy of the attacker
    let accuracy = attacker_skills.skill_check_by_parts(
        Skill::Melee,
        attacker_attr,
        attacker_body,
        attacker_essentials,
        attacker_equip,
        LimbWeights::arm(attacker_side, attacker_body.primary_side()),
    ) * attacker_equip.weapon_accuracy()
        * hit_precision.clamp(0.0, 1.0);

    // (2)-(5) Calculate defense of the defender depending on the response
    let defense = match defender_response {
        DefenderResponse::None | DefenderResponse::Parry { .. } => {
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
            let armor_dodge = defender_equip.armor_penalty(BodyPart::FULL_BODY);
            let encumbrance =
                defender_equip.encumbrance_penalty_by_parts(defender_attr, defender_body);

            dodge_skill * armor_dodge * encumbrance
        }
    } * defender_response.factor()
        * (1.0 - flanking).clamp(0.0, 1.0);

    // (6) Total attack value of the combat exchange
    let attack = accuracy - defense;

    match attack {
        // (7) Missed the attack, unbalance damage to attacker
        ..0.0 => AttackResult::ToAttacker {
            balance_damage: attack.abs(),
        },
        // (9) Critical attack for precise weapons
        1.0.. if attacker_equip.weapon_is_precise() => {
            let critical_attack =
                (attack - 1.0 - defender_equip.armor_coverage(defender_body_part)).max(0.0);

            let damage = calculate_damage(
                1.0,
                attacker_attr,
                attacker_body,
                attacker_equip,
                defender_body_part,
                defender_body,
                defender_equip,
            );

            if critical_attack > 0.0 {
                damage * critical_attack
            } else {
                damage
            }
        }
        // (8) Simple connected attack
        _ => calculate_damage(
            attack,
            attacker_attr,
            attacker_body,
            attacker_equip,
            defender_body_part,
            defender_body,
            defender_equip,
        ),
    }
}

/// Calculate attack force in joules
fn attack_force(
    attr: &impl PlayerAttributes,
    body: &impl PlayerBody,
    equip: &impl PlayerEquipment,
) -> f32 {
    let strength =
        attr.limb_attr_by_weight_by_parts(LimbAttribute::Strength, body, LimbWeights::both_arms());
    let upper_muscle_kg = strength * UPPER_MUSCLE_KG_PER_STRENGTH;
    let punch_kg = UPPER_MUSCLE_KG_TO_PUNCH_KG * upper_muscle_kg;
    let striking_mass_kg =
        punch_kg + equip.weapon_weight() * (1.0 + equip.weapon_balance() * equip.weapon_reach());
    upper_muscle_kg * MUSCLE_KG_TO_JOULES * striking_mass_kg
}

fn calculate_damage(
    attack: f32,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_equip: &impl PlayerEquipment,
    defender_body_part: BodyPart,
    defender_body: &impl PlayerBody,
    defender_equip: &impl PlayerEquipment,
) -> AttackResult {
    let attack = attack.clamp(0.0, 1.0);

    let defender_resistance = {
        let resistance = defender_equip.armor_resistance(defender_body_part);
        let flexibility = defender_equip.armor_flexibility(defender_body_part);
        let penetration = attacker_equip.weapon_penetration();
        resistance - flexibility * resistance * penetration
    };
    let defender_padding = defender_equip.armor_padding(defender_body_part);
    let defender_stagger_resistance = STAGGER_RESISTANCE_JOULES_PER_KG
        * (defender_equip.inventory_weight() + defender_body.body_weight());

    let attack_force = attack_force(&attacker_attr, &attacker_body, &attacker_equip) * attack;
    let penetrated_force = (attack_force - defender_resistance).max(0.0);
    let absorbed_force = (attack_force - penetrated_force).max(0.0);

    let cut_damage = penetrated_force;
    let blunt_damage = (absorbed_force * 0.5 - defender_padding).max(0.0);
    let balance_damage = (absorbed_force * 0.5) / defender_stagger_resistance;
    AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        balance_damage,
    }
}
