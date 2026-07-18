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
        /// Physical force exchanged with a successful block or parry.
        contact_force: f32,
        /// False for a clean miss or dodge; true when equipment intercepted the attack.
        physical_contact: bool,
    },
    ToDefender {
        cut_damage: f32,
        blunt_damage: f32,
        balance_damage: f32,
        /// Physical force delivered at contact before armor absorption.
        contact_force: f32,
        /// Whether this contact actually intersected the armor coverage roll.
        armor_contact: bool,
    },
}

impl Mul<f32> for AttackResult {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        match self {
            Self::ToAttacker {
                balance_damage,
                contact_force,
                physical_contact,
            } => Self::ToAttacker {
                balance_damage: balance_damage * rhs,
                contact_force: contact_force * rhs,
                physical_contact,
            },
            Self::ToDefender {
                cut_damage,
                blunt_damage,
                balance_damage,
                contact_force,
                armor_contact,
            } => Self::ToDefender {
                cut_damage: cut_damage * rhs,
                blunt_damage: blunt_damage * rhs,
                balance_damage: balance_damage * rhs,
                contact_force: contact_force * rhs,
                armor_contact,
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
            contact_force: if matches!(defender_response, DefenderResponse::Parry { .. }) {
                attack_force(attacker_attr, attacker_body, attacker_equip)
                    * accuracy.clamp(0.0, 1.0)
            } else {
                0.0
            },
            physical_contact: matches!(defender_response, DefenderResponse::Parry { .. }),
        },
        // (9) Critical attack for precise weapons
        1.0.. if attacker_equip.weapon_is_precise() => {
            let critical_attack =
                (attack - 1.0 - defender_equip.armor_coverage(defender_body_part)).max(0.0);

            if critical_attack > 0.0 {
                calculate_damage(
                    1.0,
                    attacker_attr,
                    attacker_body,
                    attacker_equip,
                    defender_body_part,
                    defender_body,
                    defender_equip,
                    false,
                ) * critical_attack
            } else {
                calculate_damage(
                    1.0,
                    attacker_attr,
                    attacker_body,
                    attacker_equip,
                    defender_body_part,
                    defender_body,
                    defender_equip,
                    true,
                )
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
            true,
        ),
    }
}

/// Resolve a ranged attack using the same defense, armor, and damage model as
/// melee combat. Ranged accuracy uses the attacker's Ranged check and the
/// weapon's projectile energy rather than muscular striking force.
pub fn resolve_ranged_attack_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
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
    let defender_response = if matches!(defender_response, DefenderResponse::Parry { .. })
        && defender_equip.shield_block_bonus() <= 0.0
    {
        DefenderResponse::None
    } else {
        defender_response
    };
    let accuracy = attacker_skills.skill_check_by_parts(
        Skill::Ranged,
        attacker_attr,
        attacker_body,
        attacker_essentials,
        attacker_equip,
        LimbWeights::both_arms(),
    ) * attacker_equip.weapon_accuracy()
        * hit_precision.clamp(0.0, 1.0);

    let defense = defense_by_parts(
        defender_response,
        defender_skills,
        defender_attr,
        defender_body,
        defender_essentials,
        defender_equip,
    ) * (1.0 - flanking).clamp(0.0, 1.0);
    let attack = accuracy - defense;

    if attack < 0.0 {
        return AttackResult::ToAttacker {
            // Missing with a projectile does not physically unbalance the
            // attacker. Keep the common result shape for callers.
            balance_damage: 0.0,
            contact_force: if matches!(defender_response, DefenderResponse::Parry { .. }) {
                attacker_equip.weapon_ranged_force_joules() * accuracy.clamp(0.0, 1.0)
            } else {
                0.0
            },
            physical_contact: matches!(defender_response, DefenderResponse::Parry { .. }),
        };
    }

    if attack > 1.0 && attacker_equip.weapon_is_precise() {
        let critical = (attack - 1.0 - defender_equip.armor_coverage(defender_body_part)).max(0.0);
        if critical > 0.0 {
            return calculate_damage_from_force(
                1.0,
                attacker_equip.weapon_ranged_force_joules(),
                attacker_equip,
                defender_body_part,
                defender_body,
                defender_equip,
                false,
            ) * critical;
        }
    }
    calculate_damage_from_force(
        attack.min(1.0),
        attacker_equip.weapon_ranged_force_joules(),
        attacker_equip,
        defender_body_part,
        defender_body,
        defender_equip,
        true,
    )
}

fn defense_by_parts(
    response: DefenderResponse,
    skills: &impl PlayerSkills,
    attr: &impl PlayerAttributes,
    body: &impl PlayerBody,
    essentials: &impl PlayerEssentials,
    equip: &impl PlayerEquipment,
) -> f32 {
    let base = match response {
        DefenderResponse::None => 0.0,
        DefenderResponse::Parry { .. } => {
            let block = skills.skill_check_by_parts(
                Skill::Block,
                attr,
                body,
                essentials,
                equip,
                LimbWeights::all_equal(),
            );
            5.0 * (1.0 - (-(equip.shield_block_bonus() + block) / 2.0).exp())
        }
        DefenderResponse::Dodge { .. } => {
            let dodge = skills.skill_check_by_parts(
                Skill::Dodge,
                attr,
                body,
                essentials,
                equip,
                LimbWeights {
                    left_arm: 0.1,
                    right_arm: 0.1,
                    left_leg: 0.4,
                    right_leg: 0.4,
                },
            );
            dodge
                * equip.armor_penalty(BodyPart::FULL_BODY)
                * equip.encumbrance_penalty_by_parts(attr, body)
        }
    };
    base * response.factor()
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
    armor_applies: bool,
) -> AttackResult {
    calculate_damage_from_force(
        attack,
        attack_force(attacker_attr, attacker_body, attacker_equip),
        attacker_equip,
        defender_body_part,
        defender_body,
        defender_equip,
        armor_applies,
    )
}

fn calculate_damage_from_force(
    attack: f32,
    full_force: f32,
    attacker_equip: &impl PlayerEquipment,
    defender_body_part: BodyPart,
    defender_body: &impl PlayerBody,
    defender_equip: &impl PlayerEquipment,
    armor_applies: bool,
) -> AttackResult {
    let attack = attack.clamp(0.0, 1.0);

    let defender_resistance = if armor_applies {
        let resistance = defender_equip.armor_resistance(defender_body_part);
        let flexibility = defender_equip.armor_flexibility(defender_body_part);
        let penetration = attacker_equip.weapon_penetration();
        (resistance - flexibility * resistance * penetration).max(0.0)
    } else {
        0.0
    };
    let defender_padding = if armor_applies {
        defender_equip.armor_padding(defender_body_part)
    } else {
        0.0
    };
    let defender_stagger_resistance = STAGGER_RESISTANCE_JOULES_PER_KG
        * (defender_equip.inventory_weight() + defender_body.body_weight());

    let attack_force = full_force.max(0.0) * attack;
    let penetrated_force = (attack_force - defender_resistance).max(0.0);
    let absorbed_force = (attack_force - penetrated_force).max(0.0);

    // Wider cutting surfaces make poor penetrators but damage more tissue once
    // through. A zero coefficient is treated as a purely blunt weapon.
    let penetration = attacker_equip.weapon_penetration().max(0.0);
    let cut_damage = if penetration > 0.0
        && (attacker_equip.weapon_does_slash() || attacker_equip.weapon_does_pierce())
    {
        penetrated_force / penetration
    } else {
        0.0
    };
    let blunt_force = absorbed_force * 0.5
        + if attacker_equip.weapon_does_blunt() {
            penetrated_force
        } else {
            0.0
        };
    let blunt_damage = (blunt_force - defender_padding).max(0.0);
    let balance_damage = (absorbed_force * 0.5) / defender_stagger_resistance;
    AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        balance_damage,
        contact_force: attack_force,
        armor_contact: armor_applies,
    }
}

/// Convert physical damage energy into the strategic 0-1 body-part health
/// scale. The calibration follows Combat.md: roughly 20 J to an unarmored arm
/// is disabling, while larger body regions absorb proportionally more energy.
pub fn health_damage_from_attack(result: AttackResult, part: BodyPart) -> f32 {
    let AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        ..
    } = result
    else {
        return 0.0;
    };
    let capacity_joules = match part {
        BodyPart::LeftArm | BodyPart::RightArm => 20.0,
        BodyPart::LeftLeg | BodyPart::RightLeg => 35.0,
        BodyPart::Head => 20.0,
        BodyPart::Chest => 80.0,
        BodyPart::Stomach => 55.0,
    };
    ((cut_damage + blunt_damage * 0.75) / capacity_joules).max(0.0)
}
