use super::*;

pub(super) struct MeleeExchangeOutcome {
    pub(super) result: AttackResult,
    pub(super) contact: MeleeContactLocation,
    pub(super) defense_alignment: Option<WeaponDefenseAlignment>,
    pub(super) redirected_from: Option<BodyPart>,
    pub(super) effective_response: DefenderResponse,
    pub(super) dodge_geometry: Option<MeleeDodgeGeometry>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "the pure exchange boundary receives each independently sampled attack and dodge fact"
)]
pub(super) fn melee_exchange_at_contact(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    flanking: f32,
    contact_sample: f32,
    response: DefenderResponse,
    defense_alignment_sample: f32,
    dodge_displacement_time_seconds: f32,
    actual_measure_metres: f32,
    contact_at_time: MeleeContactAtTime,
) -> MeleeExchangeOutcome {
    let performance = attacker.fatigue_performance();
    let attacker_equipment = attacker.equipment.for_melee();
    let attacker_view = attacker.view_with_equipment(&attacker_equipment);
    let defender_view = defender.view_with_equipment(&defender.equipment);
    let mut contact = attacker_view.melee_contact_location(
        attacker.equipment.melee_holding_side,
        attacker_equipment.weapon_preferred_melee_style(),
        &defender_view,
        precision * performance,
        contact_sample,
    );
    if let DefenderResponse::Dodge { .. } = response {
        let geometry = dodge_contact_geometry(
            attacker,
            defender,
            &attacker_equipment,
            performance,
            dodge_displacement_time_seconds,
            contact.body_part,
            actual_measure_metres,
        );
        let Some(body_part) = geometry.contacted_body_part else {
            return MeleeExchangeOutcome {
                result: AttackResult::ToAttacker {
                    balance_damage: 0.0,
                    contact_force: 0.0,
                    physical_contact: false,
                },
                contact,
                defense_alignment: None,
                redirected_from: None,
                effective_response: response,
                dodge_geometry: Some(geometry),
            };
        };
        let intended = contact.body_part;
        let surface_coordinate = contact_sample.clamp(0.0, 1.0 - f32::EPSILON);
        contact = MeleeContactLocation::new(
            body_part,
            anatomical_subregion(body_part, surface_coordinate),
            surface_coordinate,
            defender_view.armor_surface(body_part, surface_coordinate),
        );
        let result = attacker_view.resolve_melee_attack(
            crate::combat::EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
            attacker.equipment.melee_holding_side,
            attacker_equipment.weapon_preferred_melee_style(),
            &defender_view,
            &defender.bestiary_categories,
            DefenderResponse::None,
            precision * performance,
            flanking,
            contact,
            contact_at_time,
        );
        return MeleeExchangeOutcome {
            result: result * performance,
            contact,
            defense_alignment: None,
            redirected_from: (body_part != intended).then_some(intended),
            effective_response: DefenderResponse::None,
            dodge_geometry: Some(geometry),
        };
    }
    let defense_alignment = response.is_weapon_contact().then(|| {
        let attack_value = melee_attack_value_by_parts(
            &attacker.skills,
            &attacker.attributes,
            &attacker.body,
            &attacker.essentials,
            &attacker_view,
            attacker.equipment.melee_holding_side,
            attacker_equipment.weapon_preferred_melee_style(),
            precision * performance,
            flanking,
            response,
            &defender.skills,
            &defender.attributes,
            &defender.body,
            &defender.essentials,
            &defender_view,
        );
        resolve_weapon_defense_alignment(response, attack_value, defense_alignment_sample)
    });
    let effective_response = defense_alignment.map_or(response, |alignment| alignment.effective);
    let result = attacker_view.resolve_melee_attack(
        crate::combat::EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
        attacker.equipment.melee_holding_side,
        attacker_equipment.weapon_preferred_melee_style(),
        &defender_view,
        &defender.bestiary_categories,
        effective_response,
        precision * performance,
        flanking,
        contact,
        contact_at_time,
    );
    MeleeExchangeOutcome {
        result: result * performance,
        contact,
        defense_alignment,
        redirected_from: None,
        effective_response,
        dodge_geometry: None,
    }
}

#[cfg(test)]
pub(super) fn melee_exchange(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    flanking: f32,
    contact_sample: f32,
    response: DefenderResponse,
    defense_alignment_sample: f32,
    dodge_displacement_time_seconds: f32,
) -> MeleeExchangeOutcome {
    let measure = attacker.equipment.weapon_reach().max(0.4);
    melee_exchange_at_contact(
        attacker,
        defender,
        precision,
        flanking,
        contact_sample,
        response,
        defense_alignment_sample,
        dodge_displacement_time_seconds,
        measure,
        MeleeContactAtTime::intended(measure),
    )
}

pub(super) fn autoresolve_melee_contact_location(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    contact_sample: f32,
) -> MeleeContactLocation {
    let performance = attacker.fatigue_performance();
    let attacker_equipment = attacker.equipment.for_melee();
    attacker
        .view_with_equipment(&attacker_equipment)
        .melee_contact_location(
            attacker.equipment.melee_holding_side,
            attacker_equipment.weapon_preferred_melee_style(),
            &defender.view_with_equipment(&defender.equipment),
            precision * performance,
            contact_sample,
        )
}

fn dodge_contact_geometry(
    attacker: &Combatant,
    defender: &Combatant,
    attacker_equipment: &CombatEquipment,
    attacker_performance: f32,
    displacement_time_seconds: f32,
    intended_body_part: BodyPart,
    contact_measure_metres: f32,
) -> MeleeDodgeGeometry {
    let defender_leg_agility = defender.attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Agility,
        &defender.body,
        LimbWeights::both_legs(),
    );
    let attacker_arm_agility = attacker.attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Agility,
        &attacker.body,
        LimbWeights::both_arms(),
    );
    let contact_measure_metres = contact_measure_metres.max(0.0);
    resolve_melee_dodge_geometry(
        (0.0, 0.0),
        (contact_measure_metres, 0.0),
        (contact_measure_metres, 0.9),
        intended_body_part,
        MeleeDodgeKinematics {
            defender_leg_agility,
            defender_fatigue_performance: defender.fatigue_performance(),
            defender_body_mass_kg: defender.body.weight_kg,
            defender_equipment_mass_kg: defender.equipment.inventory_weight,
            displacement_time_seconds,
            attacker_tracking: (attacker_arm_agility / 5.0).clamp(0.0, 1.0) * attacker_performance
                / (1.0 + attacker.equipment.weapon_moment_of_inertia().max(0.0) * 2.0),
            weapon_reach_metres: attacker.equipment.weapon_reach().max(0.4),
            committed_arc_radians: match attacker_equipment.weapon_preferred_melee_style() {
                MeleeAttackStyle::Swing => 0.8,
                MeleeAttackStyle::Stab => 0.25,
            },
        },
    )
}
