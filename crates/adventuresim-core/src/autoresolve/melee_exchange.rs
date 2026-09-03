use super::*;

pub(super) struct MeleeExchangeOutcome {
    pub(super) result: AttackResult,
    pub(super) contact: MeleeContactLocation,
    pub(super) defense_alignment: Option<WeaponDefenseAlignment>,
    pub(super) effective_response: DefenderResponse,
}

#[expect(
    clippy::too_many_arguments,
    reason = "the pure exchange boundary receives independently sampled attack facts"
)]
pub(super) fn melee_exchange_at_contact(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    flanking: f32,
    contact_sample: f32,
    response: DefenderResponse,
    defense_alignment_sample: f32,
    contact_at_time: MeleeContactAtTime,
) -> MeleeExchangeOutcome {
    let performance = attacker.incapacitation_performance();
    let attacker_equipment = attacker.equipment.for_melee();
    let attacker_view = attacker.view_with_equipment(&attacker_equipment);
    let defender_view = defender.view_with_equipment(&defender.equipment);
    let contact = attacker_view.melee_contact_location(
        attacker.equipment.melee_holding_side,
        attacker_equipment.weapon_preferred_melee_style(),
        &defender_view,
        precision * performance,
        contact_sample,
    );
    let defense_alignment = response.is_weapon_contact().then(|| {
        let attack_value = melee_attack_value_by_parts(
            &attacker.skills,
            &attacker.attributes,
            &attacker.body,
            &attacker.essentials,
            &attacker_view,
            attacker.equipment.melee_holding_side,
            attacker_equipment.weapon_preferred_melee_style(),
            melee_measure_adjusted_precision(precision * performance, contact_at_time),
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
        effective_response,
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) struct MeleeExchangeSamples {
    pub contact: f32,
    pub defense_alignment: f32,
}

#[cfg(test)]
pub(super) fn melee_exchange(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    flanking: f32,
    response: DefenderResponse,
    samples: MeleeExchangeSamples,
) -> MeleeExchangeOutcome {
    let measure = melee_effective_reach(attacker);
    melee_exchange_at_contact(
        attacker,
        defender,
        precision,
        flanking,
        samples.contact,
        response,
        samples.defense_alignment,
        MeleeContactAtTime::intended(measure),
    )
}

pub(super) fn autoresolve_melee_contact_location(
    attacker: &Combatant,
    defender: &Combatant,
    precision: f32,
    contact_sample: f32,
) -> MeleeContactLocation {
    let performance = attacker.incapacitation_performance();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hit(result: AttackResult) -> bool {
        matches!(result, AttackResult::ToDefender { .. })
    }

    fn contact_for(attacker: &Combatant, actual: f32, scheduled: f32) -> MeleeContactAtTime {
        let equipment = attacker.equipment.for_melee();
        let reach = melee_effective_reach(attacker);
        let grip = equipment.weapon_grip_to_tip();
        let head = equipment.weapon_striking_head_length();
        let distal = equipment.weapon.is_some_and(|weapon| weapon.distal_headed);
        resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
            scheduled_measure_metres: scheduled,
            actual_measure_metres: actual,
            ideal_measure_metres: preferred_melee_striking_measure(
                reach,
                grip,
                head,
                distal,
                EMBEDDED_AUTORESOLVE_PARAMETERS.melee_measure_reach_fraction,
            ),
            effective_reach_metres: reach,
            grip_to_tip_metres: grip,
            total_length_metres: equipment.weapon_total_length(),
            striking_head_length_metres: head,
            distal_headed: distal,
            attack_style: equipment.weapon_preferred_melee_style(),
            body_material: equipment.weapon_body_material(),
            striking_material: equipment.weapon_striking_material(),
        })
    }

    fn set_style(combatant: &mut Combatant, style: MeleeAttackStyle) {
        for weapon in [
            combatant.equipment.weapon.as_mut(),
            combatant.equipment.melee_weapon.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            weapon.preferred_melee_style = style;
        }
    }

    #[test]
    fn tactical_and_autoresolve_projections_exactly_follow_the_shared_hit_equation() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let templates = [
            john.combatant,
            opponents
                .iter()
                .find(|build| build.key == "hammer_brute")
                .unwrap()
                .combatant
                .clone(),
            opponents
                .iter()
                .find(|build| build.key == "polearm_veteran")
                .unwrap()
                .combatant
                .clone(),
        ];
        let mut checked = 0;
        for mut attacker in templates {
            for style in [MeleeAttackStyle::Swing, MeleeAttackStyle::Stab] {
                set_style(&mut attacker, style);
                for fatigue in [0.0, 0.55] {
                    attacker.fatigue = fatigue;
                    let performance = attacker.incapacitation_performance();
                    for dodge_hours in [0.0, 2_000.0] {
                        for inventory_weight in [5.0, 45.0] {
                            let mut defender = opponents[0].combatant.clone();
                            defender.skills.dodge_hours = dodge_hours;
                            defender.equipment.inventory_weight = inventory_weight;
                            for (precision, reflex, flanking) in
                                [(0.45, 0.2, 0.0), (0.8, 0.65, 0.35), (1.0, 1.0, 1.0)]
                            {
                                let reach = melee_effective_reach(&attacker);
                                for actual in [0.0, reach * 0.7, reach] {
                                    let contact = contact_for(&attacker, actual, reach * 0.2);
                                    let response = DefenderResponse::Dodge {
                                        input_reflex: reflex,
                                    };
                                    let exchange = melee_exchange_at_contact(
                                        &attacker, &defender, precision, flanking, 0.43, response,
                                        0.5, contact,
                                    );
                                    let attacker_equipment = attacker.equipment.for_melee();
                                    let attacker_view =
                                        attacker.view_with_equipment(&attacker_equipment);
                                    let defender_view =
                                        defender.view_with_equipment(&defender.equipment);
                                    let adjusted = melee_measure_adjusted_precision(
                                        precision * performance,
                                        contact,
                                    );
                                    let margin = melee_attack_value_by_parts(
                                        &attacker.skills,
                                        &attacker.attributes,
                                        &attacker.body,
                                        &attacker.essentials,
                                        &attacker_view,
                                        attacker.equipment.melee_holding_side,
                                        style,
                                        adjusted,
                                        flanking,
                                        response,
                                        &defender.skills,
                                        &defender.attributes,
                                        &defender.body,
                                        &defender.essentials,
                                        &defender_view,
                                    );
                                    let tactical_projection = attacker_view.resolve_melee_attack(
                                        EMBEDDED_COMBAT_RESOLUTION_PARAMETERS,
                                        attacker.equipment.melee_holding_side,
                                        style,
                                        &defender_view,
                                        &defender.bestiary_categories,
                                        response,
                                        precision * performance,
                                        flanking,
                                        exchange.contact,
                                        contact,
                                    );
                                    assert_eq!(is_hit(exchange.result), margin >= 0.0);
                                    assert_eq!(is_hit(tactical_projection), margin >= 0.0);
                                    checked += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 432);
    }

    #[test]
    fn outside_absolute_reach_is_contactless_even_for_maximum_attack_quality() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let reach = melee_effective_reach(&john.combatant);
        let contact = contact_for(&john.combatant, reach + 0.01, reach);
        let exchange = melee_exchange_at_contact(
            &john.combatant,
            &opponents[0].combatant,
            1.0,
            1.0,
            0.5,
            DefenderResponse::None,
            0.5,
            contact,
        );
        assert!(matches!(
            exchange.result,
            AttackResult::ToAttacker {
                physical_contact: false,
                contact_force: 0.0,
                ..
            }
        ));
    }
}
