use std::{f32, ops::Mul};

use serde::{Deserialize, Serialize};

mod config;
pub(crate) mod targeting;

pub use config::*;
pub use targeting::{
    MeleeContactLocation, melee_attack_accuracy_by_parts, melee_attack_value_by_parts,
    melee_contact_location, whole_body_armor_coverage,
};

use crate::{
    body::{BodyPart, BodySide, LimbWeights, PlayerBody},
    morale::{blood_loss_incapacitation, pain_incapacitation},
    prelude::{LimbAttribute, PlayerAttributes, PlayerEquipment, PlayerEssentials},
    skill::{PlayerSkills, Skill},
};

/// Blood-volume fraction lost per point of cutting limb damage.
pub const CUT_BLOOD_LOSS_PER_HEALTH_DAMAGE: f32 = 0.5;
/// Empty-hand accuracy multipliers shared by tactical inventory and pure
/// matchup/autoresolve equipment.
pub const UNARMED_SWING_PRECISION: f32 = 0.2;
pub const UNARMED_STAB_PRECISION: f32 = 0.5;
/// Fraction of maximum balance recovered per second while combat continues.
pub const IMBALANCE_RECOVERY_PER_SECOND: f32 = 0.25;

#[must_use]
pub fn apply_clamped_limb_damage(health: &mut f32, damage: f32) -> f32 {
    let applied = damage.max(0.0).min(health.max(0.0));
    *health = (*health - applied).max(0.0);
    applied
}

#[must_use]
pub fn recover_combat_imbalance(imbalance: f32, seconds: f32) -> f32 {
    (imbalance - IMBALANCE_RECOVERY_PER_SECOND * seconds.max(0.0)).max(0.0)
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
/// Empty-hand contacts move a whole body much more readily than they cause
/// disabling tissue injury. This resistance puts canonical John Fabelgeist's
/// ordinary connected punch into a 70 kg opponent at roughly 40% imbalance.
const UNARMED_STAGGER_RESISTANCE_JOULES_PER_KG: f32 = 0.875;
const UNARMED_BLUNT_INJURY_SCALE: f32 = 0.2;
/// Avoiding a committed swing can pull the attacker off balance, but this is
/// a bounded physical consequence of their own momentum rather than the raw
/// (and unbounded) margin between two skill checks.
const DODGE_OVEREXTENSION_SCALE: f32 = 0.25;
const PARRY_REBOUND_SCALE: f32 = 0.5;
const MAX_AVOIDED_ATTACK_BALANCE_DAMAGE: f32 = 0.5;

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
#[expect(
    clippy::too_many_arguments,
    reason = "combat resolution receives independent attacker and defender facets"
)]
pub fn resolve_melee_attack_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    parameters: CombatResolutionParameters,
    attacker_side: BodySide,
    attack_style: crate::combat_style::MeleeAttackStyle,
    hit_precision: f32,
    precision_damage_multiplier_cap: f32,
    flanking: f32,
    contact: MeleeContactLocation,
    defender_response: DefenderResponse,
    defender_skills: &impl PlayerSkills,
    defender_attr: &impl PlayerAttributes,
    defender_body: &impl PlayerBody,
    defender_essentials: &impl PlayerEssentials,
    defender_equip: &impl PlayerEquipment,
) -> AttackResult {
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
    let attack = melee_attack_value_by_parts(
        attacker_skills,
        attacker_attr,
        attacker_body,
        attacker_essentials,
        attacker_equip,
        attacker_side,
        attack_style,
        hit_precision,
        flanking,
        defender_response,
        defender_skills,
        defender_attr,
        defender_body,
        defender_essentials,
        defender_equip,
    );
    let armor_contact = attack_reaches_armor(attack, contact, attacker_equip, defender_equip);
    match attack {
        // (7) Avoided attack, bounded overextension/rebound to attacker.
        ..0.0 => AttackResult::ToAttacker {
            balance_damage: avoided_attack_balance_damage(
                accuracy,
                attacker_attr,
                attacker_body,
                attacker_equip,
                parameters,
                defender_response,
            ),
            contact_force: if matches!(defender_response, DefenderResponse::Parry { .. }) {
                attack_force(attacker_attr, attacker_body, attacker_equip, parameters)
                    * accuracy.clamp(0.0, 1.0)
            } else {
                0.0
            },
            physical_contact: matches!(defender_response, DefenderResponse::Parry { .. }),
        },
        // Precise attacks that beat whole-body coverage reach the server-authored gap.
        1.0.. if !armor_contact && attacker_equip.weapon_is_precise() => {
            calculate_damage(
                1.0,
                attacker_attr,
                attacker_body,
                attacker_equip,
                contact.body_part,
                defender_body,
                defender_equip,
                false,
                parameters,
            ) * precision_damage_multiplier(
                attack - 1.0 - whole_body_armor_coverage(defender_equip),
                precision_damage_multiplier_cap,
            )
        }
        // (8) Simple connected attack
        _ => calculate_damage(
            attack,
            attacker_attr,
            attacker_body,
            attacker_equip,
            contact.body_part,
            defender_body,
            defender_equip,
            armor_contact,
            parameters,
        ),
    }
}

fn attack_reaches_armor(
    attack: f32,
    contact: MeleeContactLocation,
    attacker_equip: &impl PlayerEquipment,
    defender_equip: &impl PlayerEquipment,
) -> bool {
    contact.armor_contact
        && !(attacker_equip.weapon_is_precise()
            && attack > 1.0 + whole_body_armor_coverage(defender_equip))
}

fn avoided_attack_balance_damage(
    accuracy: f32,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_equip: &impl PlayerEquipment,
    parameters: CombatResolutionParameters,
    defender_response: DefenderResponse,
) -> f32 {
    let response_scale = match defender_response {
        DefenderResponse::None => 0.0,
        DefenderResponse::Dodge { .. } => DODGE_OVEREXTENSION_SCALE,
        DefenderResponse::Parry { .. } => PARRY_REBOUND_SCALE,
    };
    let resistance_per_kg = if attacker_equip.weapon_is_unarmed() {
        UNARMED_STAGGER_RESISTANCE_JOULES_PER_KG
    } else {
        parameters.stagger_resistance_joules_per_kg
    };
    let whole_body_mass = attacker_body.body_weight() + attacker_equip.inventory_weight();
    let resistance = resistance_per_kg * whole_body_mass.max(f32::EPSILON);
    let committed_impulse = attack_force(attacker_attr, attacker_body, attacker_equip, parameters)
        * accuracy.clamp(0.0, 1.0)
        * 0.5;
    (committed_impulse / resistance * response_scale).clamp(0.0, MAX_AVOIDED_ATTACK_BALANCE_DAMAGE)
}

/// Resolve a ranged attack using the same defense, armor, and damage model as
/// melee combat. Ranged accuracy uses the attacker's Ranged check and the
/// weapon's projectile energy rather than muscular striking force.
#[expect(
    clippy::too_many_arguments,
    reason = "combat resolution receives independent attacker and defender facets"
)]
pub fn resolve_ranged_attack_by_parts(
    attacker_skills: &impl PlayerSkills,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_essentials: &impl PlayerEssentials,
    attacker_equip: &impl PlayerEquipment,
    parameters: CombatResolutionParameters,
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
                parameters,
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
        parameters,
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
    parameters: CombatResolutionParameters,
) -> f32 {
    let strength =
        attr.limb_attr_by_weight_by_parts(LimbAttribute::Strength, body, LimbWeights::both_arms());
    let upper_muscle_kg = strength * UPPER_MUSCLE_KG_PER_STRENGTH;
    let punch_kg = UPPER_MUSCLE_KG_TO_PUNCH_KG * upper_muscle_kg;
    let striking_mass_kg =
        punch_kg + equip.weapon_weight() * (1.0 + equip.weapon_balance() * equip.weapon_reach());
    let weapon_transfer = if equip.weapon_is_unarmed() {
        1.0
    } else {
        parameters.armed_attack_energy_transfer
    };
    upper_muscle_kg * MUSCLE_KG_TO_JOULES * striking_mass_kg * weapon_transfer
}

#[expect(
    clippy::too_many_arguments,
    reason = "damage resolution receives independent attack and defense facets"
)]
fn calculate_damage(
    attack: f32,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_equip: &impl PlayerEquipment,
    defender_body_part: BodyPart,
    defender_body: &impl PlayerBody,
    defender_equip: &impl PlayerEquipment,
    armor_applies: bool,
    parameters: CombatResolutionParameters,
) -> AttackResult {
    calculate_damage_from_force(
        attack,
        attack_force(attacker_attr, attacker_body, attacker_equip, parameters),
        attacker_equip,
        defender_body_part,
        defender_body,
        defender_equip,
        armor_applies,
        parameters,
    )
}

#[expect(clippy::too_many_arguments, reason = "independent combat facets")]
fn calculate_damage_from_force(
    attack: f32,
    full_force: f32,
    attacker_equip: &impl PlayerEquipment,
    defender_body_part: BodyPart,
    defender_body: &impl PlayerBody,
    defender_equip: &impl PlayerEquipment,
    armor_applies: bool,
    parameters: CombatResolutionParameters,
) -> AttackResult {
    let attack = attack.clamp(0.0, 1.0);

    let unarmed = attacker_equip.weapon_is_unarmed();
    let has_edge = attacker_equip.weapon_does_slash() || attacker_equip.weapon_does_pierce();
    let has_blunt = unarmed || attacker_equip.weapon_does_blunt();
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
    let stagger_resistance_per_kg = if unarmed {
        UNARMED_STAGGER_RESISTANCE_JOULES_PER_KG
    } else {
        parameters.stagger_resistance_joules_per_kg
    };
    let defender_stagger_resistance = stagger_resistance_per_kg
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
    let blunt_force = (absorbed_force * 0.5 + transmitted_blunt_force)
        * if unarmed {
            UNARMED_BLUNT_INJURY_SCALE
        } else {
            1.0
        };
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

/// Partitions clamped limb-health damage according to each attack channel's
/// contribution to the health calculation.
#[must_use]
pub fn apportion_attack_health_damage(result: AttackResult, applied: f32) -> (f32, f32) {
    let AttackResult::ToDefender {
        cut_damage,
        blunt_damage,
        ..
    } = result
    else {
        return (0.0, 0.0);
    };
    let cut_weight = cut_damage.max(0.0);
    let blunt_weight = blunt_damage.max(0.0) * 0.75;
    let total_weight = cut_weight + blunt_weight;
    if applied <= 0.0 || total_weight <= 0.0 {
        return (0.0, 0.0);
    }
    let applied_cut = applied * cut_weight / total_weight;
    (applied_cut, applied - applied_cut)
}

/// Estimates immediate blood-volume loss from already-applied limb damage.
/// Cuts bleed substantially; blunt trauma contributes only modest internal
/// bleeding, with the head and abdomen carrying the greatest risk.
#[must_use]
pub fn blood_loss_from_applied_health_damage(
    part: BodyPart,
    applied_cut: f32,
    applied_blunt: f32,
) -> f32 {
    let blunt_coefficient = match part {
        BodyPart::Head => 0.015,
        BodyPart::Stomach => 0.01,
        BodyPart::Chest => 0.0075,
        BodyPart::LeftArm | BodyPart::RightArm | BodyPart::LeftLeg | BodyPart::RightLeg => 0.005,
    };
    applied_cut.max(0.0) * CUT_BLOOD_LOSS_PER_HEALTH_DAMAGE
        + applied_blunt.max(0.0) * blunt_coefficient
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoresolve::{CombatArmor, CombatEquipment, CombatWeapon};
    use crate::stub::{StubAttributes, StubBody, StubEquipment, StubEssentials, StubSkills};

    #[derive(Debug)]
    struct MatchupCombatant<'a> {
        name: &'a str,
        weight_kg: f32,
        will_check: f32,
    }

    impl PlayerBody for MatchupCombatant<'_> {
        fn body_part_health(&self, _part: BodyPart) -> f32 {
            1.0
        }

        fn body_weight(&self) -> f32 {
            self.weight_kg
        }

        fn primary_side(&self) -> BodySide {
            BodySide::Right
        }
    }

    fn assert_in_window(label: &str, actual: f32, expected: (f32, f32)) {
        assert!(
            (expected.0..=expected.1).contains(&actual),
            "{label}: {actual:.4} was outside {:.4}..={:.4}",
            expected.0,
            expected.1,
        );
    }

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
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
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
        assert_eq!(
            balance_damage,
            50.0 / (70.0 * EMBEDDED_COMBAT_RESOLUTION_PARAMETERS.stagger_resistance_joules_per_kg)
        );
    }

    #[test]
    fn johns_longsword_glances_off_munition_plate() {
        let john = crate::starting_character::default_character("combat-matchups");
        let john_body = MatchupCombatant {
            name: &john.name,
            weight_kg: 70.0,
            will_check: john.skills.will,
        };
        let longsword = CombatEquipment {
            weapon: Some(CombatWeapon {
                weight: 1.5,
                penetration: 1.0,
                melee_reach: 1.25,
                balance: 0.45,
                slash: true,
                pierce: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut munition_armor = CombatEquipment {
            inventory_weight: 15.95,
            ..Default::default()
        };
        munition_armor.armor[crate::autoresolve::body_part_index(BodyPart::Head)] = CombatArmor {
            resistance: 115.0,
            padding: 60.0,
            flexibility: 27.0 / 115.0,
            range_of_motion: 0.78,
            coverage: 0.7475,
        };

        let force = attack_force(
            &john.attributes,
            &john_body,
            &longsword,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
        );
        assert_in_window("John longsword energy", force, (69.0, 70.0));
        let result = calculate_damage_from_force(
            1.0,
            force,
            &longsword,
            BodyPart::Head,
            &john_body,
            &munition_armor,
            true,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
        );
        let AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } = result
        else {
            panic!("a plate contact must land on the defender");
        };
        assert_eq!(cut_damage, 0.0);
        assert_eq!(blunt_damage, 0.0);
        assert_in_window("munition plate imbalance", balance_damage, (0.09, 0.11));
        assert_eq!(health_damage_from_attack(result, BodyPart::Head), 0.0);
    }

    #[test]
    fn unarmed_matchups_land_in_realistic_outcome_windows() {
        struct Matchup<'a, 'b> {
            defender: &'a MatchupCombatant<'b>,
            target: BodyPart,
            contact_energy: (f32, f32),
            imbalance: (f32, f32),
            health_damage: (f32, f32),
            total_incapacitation: (f32, f32),
        }

        let john = crate::starting_character::default_character("combat-matchups");
        assert_eq!(john.name, crate::starting_character::DEFAULT_CHARACTER_NAME);
        let john_body = MatchupCombatant {
            name: &john.name,
            weight_kg: 70.0,
            will_check: john.skills.will,
        };
        let light_bandit = MatchupCombatant {
            name: "light bandit",
            weight_kg: 55.0,
            will_check: 1.5,
        };
        let average_bandit = MatchupCombatant {
            name: "average bandit",
            weight_kg: 70.0,
            will_check: 1.5,
        };
        let heavy_bandit = MatchupCombatant {
            name: "heavy bandit",
            weight_kg: 95.0,
            will_check: 1.5,
        };
        let unarmed = CombatEquipment::default();

        let matchups = [
            Matchup {
                defender: &average_bandit,
                target: BodyPart::Head,
                contact_energy: (48.0, 51.0),
                imbalance: (0.38, 0.45),
                health_damage: (0.25, 0.40),
                total_incapacitation: (0.55, 0.75),
            },
            Matchup {
                defender: &light_bandit,
                target: BodyPart::Chest,
                contact_energy: (48.0, 51.0),
                imbalance: (0.45, 0.60),
                health_damage: (0.06, 0.11),
                total_incapacitation: (0.50, 0.70),
            },
            Matchup {
                defender: &heavy_bandit,
                target: BodyPart::Stomach,
                contact_energy: (48.0, 51.0),
                imbalance: (0.25, 0.35),
                health_damage: (0.09, 0.15),
                total_incapacitation: (0.34, 0.48),
            },
        ];

        for matchup in matchups {
            let label = format!(
                "{} -> {} ({})",
                john_body.name, matchup.defender.name, matchup.target
            );
            let result = resolve_melee_attack_by_parts(
                &john.skills,
                &john.attributes,
                &john_body,
                &StubEssentials,
                &unarmed,
                EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
                BodySide::Right,
                crate::combat_style::MeleeAttackStyle::Swing,
                1.0,
                2.0,
                0.0,
                MeleeContactLocation {
                    body_part: matchup.target,
                    armor_contact: false,
                },
                DefenderResponse::None,
                &StubSkills,
                &StubAttributes,
                matchup.defender,
                &StubEssentials,
                &unarmed,
            );
            let AttackResult::ToDefender {
                balance_damage,
                contact_force,
                ..
            } = result
            else {
                panic!("{label}: undefended punch did not reach defender");
            };
            let health_damage = health_damage_from_attack(result, matchup.target);
            let (cut, blunt) = apportion_attack_health_damage(result, health_damage);
            let blood_loss = blood_loss_from_applied_health_damage(matchup.target, cut, blunt);
            let total_incapacitation = combat_incapacitation(
                0.0,
                1.0,
                blood_loss,
                health_damage,
                matchup.defender.will_check,
                balance_damage,
            );

            assert_in_window(
                &format!("{label} contact energy"),
                contact_force,
                matchup.contact_energy,
            );
            assert_in_window(
                &format!("{label} imbalance"),
                balance_damage,
                matchup.imbalance,
            );
            assert_in_window(
                &format!("{label} health damage"),
                health_damage,
                matchup.health_damage,
            );
            assert_in_window(
                &format!("{label} total incapacitation"),
                total_incapacitation,
                matchup.total_incapacitation,
            );
            assert!(
                blood_loss < 0.01,
                "{label}: blunt punch caused {blood_loss:.4} immediate blood loss"
            );
        }

        struct AvoidedMatchup {
            label: &'static str,
            response: DefenderResponse,
            defender_equipment: CombatEquipment,
            expected_imbalance: (f32, f32),
            expected_contact: bool,
        }
        let avoided_matchups = [
            AvoidedMatchup {
                label: "John punch cleanly dodged",
                response: DefenderResponse::Dodge { input_reflex: 1.0 },
                defender_equipment: CombatEquipment::default(),
                expected_imbalance: (0.08, 0.13),
                expected_contact: false,
            },
            AvoidedMatchup {
                label: "John punch caught by shield parry",
                response: DefenderResponse::Parry { input_reflex: 1.0 },
                defender_equipment: CombatEquipment {
                    shield_block_bonus: 5.0,
                    ..Default::default()
                },
                expected_imbalance: (0.17, 0.24),
                expected_contact: true,
            },
        ];

        for matchup in avoided_matchups {
            let result = resolve_melee_attack_by_parts(
                &john.skills,
                &john.attributes,
                &john_body,
                &StubEssentials,
                &unarmed,
                EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
                BodySide::Right,
                crate::combat_style::MeleeAttackStyle::Swing,
                1.0,
                2.0,
                0.0,
                MeleeContactLocation {
                    body_part: BodyPart::Chest,
                    armor_contact: false,
                },
                matchup.response,
                &john.skills,
                &john.attributes,
                &john_body,
                &StubEssentials,
                &matchup.defender_equipment,
            );
            let AttackResult::ToAttacker {
                balance_damage,
                physical_contact,
                ..
            } = result
            else {
                panic!("{}: defense did not avoid the punch", matchup.label);
            };
            assert_in_window(
                &format!("{} attacker imbalance", matchup.label),
                balance_damage,
                matchup.expected_imbalance,
            );
            assert_eq!(
                physical_contact, matchup.expected_contact,
                "{}",
                matchup.label
            );
        }
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
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
        );
        let stronger = calculate_damage_from_force(
            1.0,
            100.0,
            &cutting(1.0),
            BodyPart::Chest,
            &StubBody,
            &defender,
            true,
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
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
            EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
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
    fn whole_body_coverage_selects_protected_destinations_without_input_aiming() {
        let mut defender = CombatEquipment::default();
        defender.armor[crate::autoresolve::body_part_index(BodyPart::Chest)] = CombatArmor {
            resistance: 120.0,
            padding: 50.0,
            flexibility: 0.08,
            range_of_motion: 0.65,
            coverage: 1.0,
        };
        let attacker = CombatEquipment {
            weapon: Some(CombatWeapon {
                precise: false,
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!((whole_body_armor_coverage(&defender) - 0.22).abs() < 0.0001);
        for sample in [0.0, 0.25, 0.5, 0.75, 0.999] {
            assert_eq!(
                melee_contact_location(10.0, &attacker, &defender, sample),
                MeleeContactLocation {
                    body_part: BodyPart::Chest,
                    armor_contact: true,
                }
            );
        }
    }

    #[test]
    fn dodge_can_move_a_precise_destination_back_onto_armor() {
        let mut defender = CombatEquipment::default();
        defender.armor[crate::autoresolve::body_part_index(BodyPart::Chest)] = CombatArmor {
            resistance: 120.0,
            padding: 50.0,
            flexibility: 0.08,
            range_of_motion: 0.65,
            coverage: 1.0,
        };
        let attacker = CombatEquipment {
            weapon: Some(CombatWeapon {
                precise: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        let intended = melee_contact_location(1.5, &attacker, &defender, 0.5);
        assert!(!intended.armor_contact);
        assert_ne!(intended.body_part, BodyPart::Chest);

        let after_dodge = melee_contact_location(0.8, &attacker, &defender, 0.5);
        assert_eq!(
            after_dodge,
            MeleeContactLocation {
                body_part: BodyPart::Chest,
                armor_contact: true,
            }
        );
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
