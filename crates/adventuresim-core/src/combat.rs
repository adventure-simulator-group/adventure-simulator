use std::{f32, ops::Mul};

use serde::{Deserialize, Serialize};

use crate::{
    body::{BodyPart, BodySide, LimbWeights, PlayerBody},
    morale::{blood_loss_incapacitation, pain_incapacitation},
    prelude::{LimbAttribute, PlayerAttributes, PlayerEquipment, PlayerEssentials},
    skill::{PlayerSkills, Skill},
};

/// Blood-volume fraction lost per point of limb health removed by combat.
pub const BLOOD_LOSS_PER_HEALTH_DAMAGE: f32 = 0.5;
/// Combat recovers this much imbalance per second for each effective point of
/// Balance skill.
pub const BALANCE_RECOVERY_PER_SKILL_SECOND: f32 = 0.03;

#[must_use]
pub fn apply_clamped_limb_damage(health: &mut f32, damage: f32) -> f32 {
    let applied = damage.max(0.0).min(health.max(0.0));
    *health = (*health - applied).max(0.0);
    applied
}

#[must_use]
pub fn recover_combat_imbalance(imbalance: f32, balance_check: f32, seconds: f32) -> f32 {
    (imbalance - BALANCE_RECOVERY_PER_SKILL_SECOND * balance_check.max(0.25) * seconds.max(0.0))
        .max(0.0)
}

#[must_use]
pub fn combat_incapacitation(
    starting_incapacitation: f32,
    starting_blood_fraction: f32,
    blood_loss_fraction: f32,
    total_limb_damage: f32,
    will_check: f32,
    imbalance: f32,
) -> f32 {
    let pain = pain_incapacitation(total_limb_damage, will_check);
    let remaining_blood = (starting_blood_fraction - blood_loss_fraction).clamp(0.0, 1.0);
    starting_incapacitation.max(0.0)
        + pain
        + blood_loss_incapacitation(remaining_blood, 1.0)
        + imbalance.max(0.0)
}

/// Derives tactical/autoresolve starting state from the authoritative strategic
/// snapshot without double-counting pain or blood loss, which combat recomputes.
#[must_use]
pub fn derive_combat_starting_condition(
    strategic_incapacitation: f32,
    strategic_pain: f32,
    strategic_blood_loss: f32,
    current_blood: f32,
    maximum_blood: f32,
) -> (f32, f32) {
    let starting_incapacitation =
        (strategic_incapacitation - strategic_pain - strategic_blood_loss).max(0.0);
    let starting_blood_fraction = if maximum_blood.is_finite() && maximum_blood > 0.0 {
        (current_blood / maximum_blood).clamp(0.0, 1.0)
    } else {
        1.0
    };
    (starting_incapacitation, starting_blood_fraction)
}

const UPPER_MUSCLE_KG_PER_STRENGTH: f32 = 5.0;
const MUSCLE_KG_TO_JOULES: f32 = 2.0;
const UPPER_MUSCLE_KG_TO_PUNCH_KG: f32 = 0.1;
const STAGGER_RESISTANCE_JOULES_PER_KG: f32 = 10.0;

fn precision_damage_multiplier(excess_accuracy: f32, lore_cap: f32) -> f32 {
    excess_accuracy.max(0.0).min(lore_cap.max(2.0))
}

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

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
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
    precision_damage_multiplier_cap: f32,
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
    let weights = LimbWeights::arm(attacker_side, attacker_body.primary_side());
    let accuracy = attacker_equip
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
        * attacker_equip.weapon_accuracy()
        * hit_precision.clamp(0.0, 1.0);

    // (2)-(5) Calculate defense of the defender depending on the response
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
                ) * precision_damage_multiplier(critical_attack, precision_damage_multiplier_cap)
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
    precision_damage_multiplier_cap: f32,
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
    let accuracy = attacker_equip
        .weapon_skill_distribution()
        .weighted_check(|skill| {
            attacker_skills.skill_check_by_parts(
                skill,
                attacker_attr,
                attacker_body,
                attacker_essentials,
                attacker_equip,
                LimbWeights::both_arms(),
            )
        })
        * attacker_equip.weapon_accuracy()
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
            ) * precision_damage_multiplier(critical, precision_damage_multiplier_cap);
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

    let has_edge = attacker_equip.weapon_does_slash() || attacker_equip.weapon_does_pierce();
    let has_blunt = attacker_equip.weapon_does_blunt();
    let defender_resistance = if armor_applies && has_edge {
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

    // Resistance opposes an edge or point. Pure blunt contact transmits its
    // force directly to padding instead of pretending it must penetrate the
    // material's cut resistance. A mixed head divides penetrated force between
    // its edge and impact modes so the same energy is not counted twice.
    let penetration = attacker_equip.weapon_penetration().max(0.0);
    let (cut_force, transmitted_blunt_force) = match (has_edge, has_blunt) {
        (true, true) => (penetrated_force * 0.5, penetrated_force * 0.5),
        (true, false) => (penetrated_force, 0.0),
        (false, true) => (0.0, penetrated_force),
        (false, false) => (0.0, 0.0),
    };
    let cut_damage = if penetration > 0.0 && has_edge {
        cut_force / penetration
    } else {
        0.0
    };
    let blunt_force = absorbed_force * 0.5 + transmitted_blunt_force;
    let blunt_damage = (blunt_force - defender_padding).max(0.0);
    // A pure blunt impact still transfers momentum when there is no edge for
    // resistance to absorb. Edge and mixed contacts retain the absorbed-force
    // impulse used by the existing model.
    let stagger_impulse = if has_blunt && !has_edge {
        attack_force * 0.5
    } else {
        absorbed_force * 0.5
    };
    let balance_damage = stagger_impulse / defender_stagger_resistance;
    AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        balance_damage,
        contact_force: attack_force,
        armor_contact: armor_applies,
    }
}

/// Convert physical damage energy into the strategic 0-1 body-part health
/// scale. The calibration follows combat.md: roughly 20 J to an unarmored arm
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresolve::{CombatArmor, CombatEquipment, CombatWeapon};
    use crate::stub::{StubAttributes, StubBody, StubEquipment, StubEssentials, StubSkills};

    #[test]
    fn no_defender_response_has_zero_defense() {
        let none = defense_by_parts(
            DefenderResponse::None,
            &StubSkills,
            &StubAttributes,
            &StubBody,
            &StubEssentials,
            &StubEquipment,
        );
        let parry = defense_by_parts(
            DefenderResponse::Parry { input_reflex: 1.0 },
            &StubSkills,
            &StubAttributes,
            &StubBody,
            &StubEssentials,
            &StubEquipment,
        );
        assert_eq!(none, 0.0);
        assert!(parry > 0.0);
    }

    #[test]
    fn blunt_force_bypasses_edge_resistance_but_not_padding() {
        let attacker = CombatEquipment {
            weapon: Some(CombatWeapon {
                blunt: true,
                penetration: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut defender = CombatEquipment::default();
        defender.armor.fill(CombatArmor {
            resistance: 1_000.0,
            padding: 20.0,
            flexibility: 0.5,
            range_of_motion: 1.0,
            coverage: 1.0,
        });

        let result = calculate_damage_from_force(
            1.0,
            100.0,
            &attacker,
            BodyPart::Chest,
            &StubBody,
            &defender,
            true,
        );
        let AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } = result
        else {
            panic!("an undefended hit must damage the defender");
        };
        assert_eq!(cut_damage, 0.0);
        assert_eq!(blunt_damage, 80.0);
        assert_eq!(balance_damage, 50.0 / (70.0 * 10.0));
    }

    #[test]
    fn penetration_can_cross_resistance_at_armor_limited_force() {
        let cutting = |penetration| CombatEquipment {
            weapon: Some(CombatWeapon {
                slash: true,
                penetration,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut defender = CombatEquipment::default();
        defender.armor.fill(CombatArmor::innate(150.0, 0.0));

        let lower = calculate_damage_from_force(
            1.0,
            100.0,
            &cutting(0.5),
            BodyPart::Chest,
            &StubBody,
            &defender,
            true,
        );
        let stronger = calculate_damage_from_force(
            1.0,
            100.0,
            &cutting(1.0),
            BodyPart::Chest,
            &StubBody,
            &defender,
            true,
        );

        assert!(matches!(
            lower,
            AttackResult::ToDefender {
                cut_damage: 0.0,
                ..
            }
        ));
        assert!(matches!(
            stronger,
            AttackResult::ToDefender {
                cut_damage: 25.0,
                ..
            }
        ));
    }

    #[test]
    fn mixed_contact_partitions_penetrated_force_without_double_counting() {
        let attacker = CombatEquipment {
            weapon: Some(CombatWeapon {
                blunt: true,
                slash: true,
                penetration: 1.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = calculate_damage_from_force(
            1.0,
            100.0,
            &attacker,
            BodyPart::Chest,
            &StubBody,
            &CombatEquipment::default(),
            true,
        );
        assert!(matches!(
            result,
            AttackResult::ToDefender {
                cut_damage: 50.0,
                blunt_damage: 50.0,
                ..
            }
        ));
    }

    #[test]
    fn anatomical_lore_clamps_excess_accuracy_with_a_two_x_floor() {
        assert_eq!(precision_damage_multiplier(6.0, 0.0), 2.0);
        assert_eq!(precision_damage_multiplier(6.0, 3.5), 3.5);
        assert_eq!(precision_damage_multiplier(1.25, 7.0), 1.25);
    }

    #[test]
    fn depleted_strategic_snapshot_excludes_recomputed_sources() {
        let (starting, blood_fraction) =
            derive_combat_starting_condition(1.35, 0.25, 0.5, 2_500.0, 5_000.0);
        assert!((starting - 0.6).abs() < 0.0001);
        assert!((blood_fraction - 0.5).abs() < 0.0001);
        assert_eq!(
            derive_combat_starting_condition(0.2, 0.3, 0.4, 1.0, 0.0),
            (0.0, 1.0)
        );
    }
}
