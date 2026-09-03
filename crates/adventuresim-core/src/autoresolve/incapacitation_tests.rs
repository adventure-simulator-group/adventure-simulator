use super::*;

#[test]
fn fatigue_and_other_sources_produce_identical_combat_performance() {
    let (john, opponents) = melee_iteration_roster().unwrap();
    let mut tired = john.combatant.clone();
    let mut otherwise_impaired = john.combatant;
    tired.fatigue += 0.2;
    otherwise_impaired.starting_incapacitation += 0.2;
    assert!(
        (tired.incapacitation_performance() - otherwise_impaired.incapacitation_performance())
            .abs()
            < 0.00001
    );
    let attack = |attacker: &Combatant| {
        melee_exchange(
            attacker,
            &opponents[0].combatant,
            0.8,
            0.0,
            DefenderResponse::None,
            MeleeExchangeSamples {
                contact: 0.4,
                defense_alignment: 0.4,
            },
        )
        .result
    };
    assert_eq!(
        serde_json::to_value(attack(&tired)).unwrap(),
        serde_json::to_value(attack(&otherwise_impaired)).unwrap()
    );
}

#[test]
fn fatigue_does_not_slow_autoresolve_movement() {
    let (john, _) = melee_iteration_roster().unwrap();
    let mut combatant = john.combatant;
    let fresh_speed = combatant.movement_speed_meters_per_second(0.1);
    combatant.fatigue = 0.6;
    assert_eq!(combatant.movement_speed_meters_per_second(0.1), fresh_speed);
}

#[test]
fn burden_is_visible_and_not_also_subtracted_from_physical_skills() {
    let (john, _) = melee_iteration_roster().unwrap();
    let mut burdened = john.combatant.clone();
    burdened.equipment.inventory_weight += 30.0;
    let skill = |combatant: &Combatant| {
        combatant.skills.skill_check_by_parts(
            Skill::Sword,
            &combatant.attributes,
            &combatant.body,
            &combatant.essentials,
            &combatant.equipment,
            LimbWeights::both_arms(),
        )
    };
    assert_eq!(skill(&john.combatant), skill(&burdened));
    let burden = |combatant: &Combatant| {
        combat_encumbrance_incapacitation(
            &combatant.attributes,
            &combatant.body,
            &combatant.equipment,
        )
    };
    let added = burden(&burdened) - burden(&john.combatant);
    assert!(added > 0.0);
    assert!((burdened.incapacitation() - john.combatant.incapacitation() - added).abs() < 0.00001);
}
