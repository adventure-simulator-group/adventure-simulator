use crate::{
    body::{BodyPart, BodySide, LimbWeights, PlayerBody},
    prelude::{LimbAttribute, PlayerAttributes, PlayerEquipment, PlayerEssentials},
    skill::{PlayerSkills, Skill},
};

const UPPER_MUSCLE_KG_PER_STRENGTH: f32 = 5.0;
const MUSCLE_KG_TO_JOULES: f32 = 2.0;
const UPPER_MUSCLE_KG_TO_PUNCH_KG: f32 = 0.1;

/// Result of resolving a melee attack.
#[derive(Debug, Clone, Copy)]
pub struct AttackResult {
    pub cut_damage: f32,
    pub blunt_damage: f32,
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
    defender_body_part: BodyPart,
    defender_skills: &impl PlayerSkills,
    defender_attr: &impl PlayerAttributes,
    defender_body: &impl PlayerBody,
    defender_essentials: &impl PlayerEssentials,
    defender_equip: &impl PlayerEquipment,
) -> AttackResult {
    // 1. Accuracy (modulated by hit precision)
    let accuracy = attacker_skills.skill_check_by_parts(
        Skill::Melee,
        attacker_attr,
        attacker_body,
        attacker_essentials,
        attacker_equip,
        LimbWeights::arm(attacker_side, attacker_body.primary_side()),
    ) * attacker_equip.weapon_accuracy()
        * hit_precision;

    // 2. Block defense (MVP: no dodge/parry mode)
    let block_skill = defender_skills.skill_check_by_parts(
        Skill::Block,
        defender_attr,
        defender_body,
        defender_essentials,
        defender_equip,
        LimbWeights::all_equal(),
    );
    let shield_bonus = defender_equip.shield_block_bonus();
    let block_defense = 5.0 * (1.0 - (-(shield_bonus + block_skill) / 2.0).exp());

    let defense = block_defense;

    // 3. Attack value
    let attack = accuracy - defense;

    if attack < 0.0 {
        return AttackResult {
            cut_damage: 0.0,
            blunt_damage: 0.0,
        };
    }

    let directness = attack.min(1.0);

    // 4. Calculate imparted joules
    let strength = attacker_attr.limb_attr_by_weight_by_parts(
        LimbAttribute::Strength,
        attacker_body,
        LimbWeights::both_arms(),
    );
    let upper_muscle_kg = strength * UPPER_MUSCLE_KG_PER_STRENGTH;
    let punch_kg = UPPER_MUSCLE_KG_TO_PUNCH_KG * upper_muscle_kg;
    let weapon_mass = attacker_equip.weapon_weight();
    let striking_mass_kg = punch_kg + weapon_mass;
    let joules = upper_muscle_kg * MUSCLE_KG_TO_JOULES * striking_mass_kg;
    let applied_joules = directness * joules;

    // 5. Armor penetration (per body part)
    let resistance = defender_equip.armor_resistance(defender_body_part);
    let flexibility = defender_equip.armor_flexibility(defender_body_part);
    let padding = defender_equip.armor_padding(defender_body_part);
    let penetration = attacker_equip.weapon_penetration();

    let final_resistance = resistance - flexibility * resistance * penetration;

    let (cut_damage, blunt_damage) = if applied_joules > final_resistance {
        let penetrating_joules = applied_joules - final_resistance;
        let cut = penetrating_joules / penetration;
        let blunt = (final_resistance - padding).max(0.0);
        (cut, blunt)
    } else {
        let blunt = (applied_joules - padding).max(0.0);
        (0.0, blunt)
    };

    AttackResult {
        cut_damage,
        blunt_damage,
    }
}
