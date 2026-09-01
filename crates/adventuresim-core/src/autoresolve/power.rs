use super::*;

const AUTORESOLVE_HEALTH_POWER_SCALE: f64 = 1_000_000.0;

/// Observer-safe aggregate strength derived from the same complete combatant
/// snapshot consumed by autoresolve. This intentionally includes equipped
/// weapon accuracy, trained weapon/dodge/block/balance/will checks, armor,
/// current limb health, fatigue/encumbrance, and strategic incapacitation.
/// It is a conservative decision aid, not an outcome oracle.
pub(super) fn finite_log_component(value: f32, weight: f64) -> f64 {
    let bounded = if value.is_nan() || value <= 0.0 {
        0.0
    } else if value.is_infinite() {
        f32::MAX
    } else {
        value
    };
    f64::from(bounded).ln_1p() * weight
}

fn weapon_output(
    weapon: CombatWeapon,
    check: f32,
    arm_strength: f32,
    minimum_attack_interval_seconds: f32,
) -> f32 {
    let tempo = weapon
        .attack_interval_seconds
        .max(minimum_attack_interval_seconds)
        .sqrt()
        .recip();
    let contact = if weapon.ranged {
        weapon.ranged_force_joules.max(0.0).sqrt() / 5.0
    } else {
        let striking_mass =
            weapon.weight.max(0.0) * (1.0 + weapon.balance.max(0.0) * weapon.melee_reach.max(0.0));
        (arm_strength.max(0.0) * (1.0 + striking_mass)).sqrt()
    };
    let penetration = if weapon.slash || weapon.pierce {
        1.0 + weapon.penetration.max(0.0).sqrt() * 0.25
    } else {
        1.0
    };
    let reach = if weapon.melee {
        1.0 + weapon.melee_reach.max(0.0).sqrt() * 0.15
    } else {
        1.0
    };
    check * tempo * contact.max(0.25) * penetration * reach
}

fn armor_power(combatant: &Combatant) -> f32 {
    BodyPart::FULL_BODY
        .iter()
        .map(|part| {
            let armor = combatant.equipment.armor[body_part_index(part)];
            let covered = armor.coverage.clamp(0.0, 1.0);
            let edge_resistance =
                armor.resistance.max(0.0) * (1.0 - 0.5 * armor.flexibility.clamp(0.0, 1.0));
            (edge_resistance + armor.padding.max(0.0)) * covered
        })
        .sum::<f32>()
        / BodyPart::FULL_BODY.len() as f32
}

fn ranged_opening_power(combatant: &Combatant, minimum_attack_interval_seconds: f32) -> f32 {
    combatant.equipment.ranged_weapon.map_or(0.0, |weapon| {
        if combatant.equipment.ammunition == 0 {
            0.0
        } else {
            (weapon.ranged_range.max(0.0)
                / weapon
                    .attack_interval_seconds
                    .max(minimum_attack_interval_seconds))
            .sqrt()
            .min(5.0)
        }
    })
}

pub fn autoresolve_combat_power(combatant: &Combatant) -> u64 {
    let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
    let attack_check = |equipment: &CombatEquipment, weights: LimbWeights| {
        equipment
            .weapon_skill_distribution()
            .weighted_check(|skill| {
                combatant.skills.skill_check_by_parts(
                    skill,
                    &combatant.attributes,
                    &combatant.body,
                    &combatant.essentials,
                    equipment,
                    weights,
                )
            })
            * equipment.weapon_accuracy().max(0.0)
    };
    let melee = combatant.equipment.for_melee();
    let ranged = combatant.equipment.for_ranged();
    let arm_strength = combatant.attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Strength,
        &combatant.body,
        LimbWeights::both_arms(),
    );
    let melee_check = combatant.equipment.melee_weapon.map_or(0.0, |weapon| {
        weapon_output(
            weapon,
            attack_check(&melee, LimbWeights::both_arms()),
            arm_strength,
            parameters.minimum_attack_interval_seconds,
        )
    });
    let ranged_check = combatant
        .equipment
        .ranged_weapon
        .filter(|_| combatant.equipment.ammunition > 0)
        .map_or(0.0, |weapon| {
            weapon_output(
                weapon,
                attack_check(&ranged, LimbWeights::both_arms()),
                arm_strength,
                parameters.minimum_attack_interval_seconds,
            )
        });
    let skill_check = |skill, weights| {
        combatant.skills.skill_check_by_parts(
            skill,
            &combatant.attributes,
            &combatant.body,
            &combatant.essentials,
            &combatant.equipment,
            weights,
        )
    };
    let dodge = skill_check(Skill::Dodge, LimbWeights::all_equal());
    let block = skill_check(Skill::Block, LimbWeights::all_equal())
        * (1.0 + combatant.equipment.shield_block_bonus.max(0.0));
    let balance = skill_check(Skill::Balance, LimbWeights::both_legs());
    let will = skill_check(Skill::Will, LimbWeights::all_equal());
    let armor = armor_power(combatant);
    let health =
        combatant.body.health.iter().copied().sum::<f32>() / combatant.body.health.len() as f32;
    let ranged_opening =
        ranged_opening_power(combatant, parameters.minimum_attack_interval_seconds);
    let raw = finite_log_component(melee_check.max(ranged_check), 2_000_000.0)
        + finite_log_component(dodge, 900_000.0)
        + finite_log_component(block, 900_000.0)
        + finite_log_component(balance, 500_000.0)
        + finite_log_component(will, 500_000.0)
        + finite_log_component(combatant.attributes.endurance, 500_000.0)
        + finite_log_component(armor, 4_000_000.0)
        + if health.is_finite() {
            f64::from(health.clamp(0.0, 1.0)) * AUTORESOLVE_HEALTH_POWER_SCALE
        } else {
            0.0
        }
        + finite_log_component(ranged_opening, 500_000.0);
    let incapacitation = combatant.incapacitation();
    let readiness = if incapacitation.is_finite() {
        f64::from((1.0 - incapacitation).clamp(0.0, 1.0))
    } else {
        0.0
    };
    (raw * readiness).round().clamp(0.0, u64::MAX as f64) as u64
}
