use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "combat resolution receives explicit attacker and defender facets"
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
    contact_at_time: MeleeContactAtTime,
    defender_response: DefenderResponse,
    defender_skills: &impl PlayerSkills,
    defender_attr: &impl PlayerAttributes,
    defender_body: &impl PlayerBody,
    defender_essentials: &impl PlayerEssentials,
    defender_equip: &impl PlayerEquipment,
) -> AttackResult {
    if let Some(miss) = invalidated_contact_miss(contact_at_time) {
        return miss;
    }
    let hit_precision = melee_measure_adjusted_precision(hit_precision, contact_at_time);
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
    let armor_surface = contact.armor_surface;
    match attack {
        ..0.0 => avoided_contact(
            accuracy,
            attacker_attr,
            attacker_body,
            attacker_equip,
            parameters,
            defender_response,
            contact_at_time,
        ),
        1.0.. if armor_surface.is_none() && attacker_equip.weapon_is_precise() => {
            calculate_damage(
                1.0,
                attacker_attr,
                attacker_body,
                attacker_equip,
                contact.body_part,
                defender_body,
                defender_equip,
                None,
                parameters,
                contact_at_time,
            ) * precision_damage_multiplier(
                attack - 1.0 - whole_body_armor_coverage(defender_equip),
                precision_damage_multiplier_cap,
            )
        }
        _ => calculate_damage(
            attack,
            attacker_attr,
            attacker_body,
            attacker_equip,
            contact.body_part,
            defender_body,
            defender_equip,
            armor_surface,
            parameters,
            contact_at_time,
        ),
    }
}

fn invalidated_contact_miss(contact: MeleeContactAtTime) -> Option<AttackResult> {
    (contact.classification == MeleeContactClassification::InvalidatedMiss).then_some(
        AttackResult::ToAttacker {
            balance_damage: 0.0,
            contact_force: 0.0,
            physical_contact: false,
        },
    )
}

fn avoided_contact(
    accuracy: f32,
    attacker_attr: &impl PlayerAttributes,
    attacker_body: &impl PlayerBody,
    attacker_equip: &impl PlayerEquipment,
    parameters: CombatResolutionParameters,
    response: DefenderResponse,
    contact: MeleeContactAtTime,
) -> AttackResult {
    AttackResult::ToAttacker {
        balance_damage: avoided_attack_balance_damage(
            accuracy,
            attacker_attr,
            attacker_body,
            attacker_equip,
            parameters,
            response,
        ) * contact.energy_fraction,
        contact_force: if response.is_weapon_contact() {
            attack_force(attacker_attr, attacker_body, attacker_equip, parameters)
                * accuracy.clamp(0.0, 1.0)
                * contact.energy_fraction
        } else {
            0.0
        },
        physical_contact: response.is_weapon_contact(),
    }
}
