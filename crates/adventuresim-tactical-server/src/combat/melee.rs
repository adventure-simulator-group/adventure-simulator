use super::*;

pub(super) fn resolve_melee_attack(
    event: On<MeleeAttackIntent>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    spatial: SpatialQuery,
    q_character: Query<(&CharacterLook, &Transform)>,
    q_sides: Query<&TacticalCombatSide>,
    q_states: Query<&TacticalCombatState>,
    mut q_authorities: Query<&mut MeleeAttackAuthority>,
    q_bestiary_categories: Query<&BestiaryCategories>,
    q_pending: Query<&PendingDefenderResponse>,
    q_scene_items: Query<Entity, With<TacticalSceneItem>>,
    time: Res<Time<()>>,
) {
    let attack_style = event.strike_family.melee_style();
    let entity = event.attacker;
    let hand = event.hand;

    let Ok(attacker_view) = viewer.get_for_attack(entity, hand).inspect_err(|err| {
        debug!("Rejected attacker view for {entity:?}: {err}");
    }) else {
        return;
    };
    let Ok(defender_view) = viewer.get(event.target).inspect_err(|err| {
        debug!("Rejected defender view for {:?}: {err}", event.target);
    }) else {
        return;
    };

    let Ok([attacker_character, defender_character]) = q_character
        .get_many([entity, event.target])
        .inspect_err(|err| {
            debug!("Rejected attacker/defender transform: {err}");
        })
    else {
        return;
    };
    let (attacker_look, attacker_transform) = attacker_character;
    let (defender_look, defender_transform) = defender_character;

    let weapon_reach = attacker_view.weapon_reach();
    let now = CombatInstant::from_elapsed(&time);
    let Ok(mut authority) = q_authorities.get_mut(entity) else {
        return;
    };
    let facts = MeleeIntentFacts {
        attacker: entity,
        target: event.target,
        attacker_side: q_sides.get(entity).ok().copied(),
        target_side: q_sides.get(event.target).ok().copied(),
        attacker_incapacitated: q_states
            .get(entity)
            .ok()
            .map(TacticalCombatState::is_incapacitated),
        target_incapacitated: q_states
            .get(event.target)
            .ok()
            .map(TacticalCombatState::is_incapacitated),
        reported_precision: event.reported_precision,
        weapon_reach,
        separation: attacker_transform
            .translation
            .distance(defender_transform.translation),
        authority_permits: authority.permits(event.target, now),
        body_part: event.body_part,
        attacker_position: attacker_transform.translation,
        target_position: defender_transform.translation,
        attacker_yaw: attacker_look.yaw,
        target_yaw: defender_look.yaw,
    };
    let validated = match validate_melee_intent_cheap(facts) {
        Ok(validated) => validated,
        Err(reason) => {
            debug!(
                "Rejected melee intent from {entity:?} to {:?}: {reason:?}",
                event.target
            );
            return;
        }
    };
    let line_of_sight = authoritative_line_of_sight(
        &spatial,
        &q_scene_items,
        validated.attacker(),
        validated.target(),
        validated.attacker_position(),
        validated.target_position(),
    );
    if let Err(reason) = validate_melee_line_of_sight(line_of_sight) {
        debug!(
            "Rejected melee intent from {entity:?} to {:?}: {reason:?}",
            event.target
        );
        return;
    }
    // Mutate the pre-existing authority component synchronously. A later
    // completion in this same message flush observes the consumed windup and
    // active cooldown instead of reusing deferred Commands state.
    let Some(authorized) = authority.authorize_attack(validated, now, MELEE_COOLDOWN) else {
        debug!("Rejected already-consumed melee authorization for {entity:?}");
        return;
    };
    let attack = authorized;
    let (a2, a1) = attack.attacker_yaw().sin_cos();
    let (d2, d1) = attack.target_yaw().sin_cos();
    let flanking = flanking_from_dir((a1, a2), (d1, d2));

    let Some(attacker_side) = attacker_view.weapon_holding_side() else {
        debug!("Rejected attacker without a usable striking side");
        return;
    };
    let attacker_has_weapon = viewer
        .inventory
        .get_for_attack(entity, hand)
        .has_striking_item();

    let pending = q_pending.get(attack.target()).ok();
    let defender_response = resolve_defender_response(pending, &time, &defender_view);

    // Consume the pending response so it is not reused.
    cmd.entity(attack.target())
        .remove::<PendingDefenderResponse>();

    let fallback_categories = BestiaryCategories::default();
    let defender_categories = q_bestiary_categories
        .get(attack.target())
        .unwrap_or(&fallback_categories);

    let result = attacker_view.resolve_melee_attack(
        attacker_side,
        attack_style,
        &defender_view,
        &defender_categories.0,
        defender_response,
        attack.reported_precision().get(),
        flanking,
        attack.body_part(),
    );
    let attacker_weapon_slot = match attacker_side {
        BodySide::Left => EquipSlot::HoldingLeft,
        BodySide::Right => EquipSlot::HoldingRight,
        BodySide::Both => return,
    };
    let defender_parry_slot = matches!(defender_response, DefenderResponse::Parry { .. })
        .then(|| defender_view.shield_holding_side())
        .flatten()
        .and_then(|side| match side {
            BodySide::Left => Some(EquipSlot::HoldingLeft),
            BodySide::Right => Some(EquipSlot::HoldingRight),
            BodySide::Both => None,
        });

    cmd.trigger(ApplyMeleeAttackResult {
        attacker: attack.attacker(),
        target: attack.target(),
        body_part: attack.body_part(),
        result,
        attacker_weapon_slot,
        defender_parry_slot,
        attacker_weapon_contact: attacker_has_weapon,
    });

    match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            info!(
                "{entity:?} failed to hit {:?} on {:?} and receiver {balance_damage:.1} balance damage",
                attack.target(),
                attack.body_part(),
            );
        }
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } => {
            info!(
                "{entity:?} hit {:?} on {:?} for {:.1} damage ({cut_damage:.1} cut + {blunt_damage:.1} blunt) and {balance_damage:.1} balance damage",
                attack.target(),
                attack.body_part(),
                cut_damage + blunt_damage
            );
        }
    }

    cmd.server_trigger(ToClients {
        targets: SendTargets::All,
        message: SuccessfulAttackResponse {
            attacker: attack.attacker(),
            hit: vec![attack.target()],
            body_part: attack.body_part(),
            result,
            flanking,
            defender_response,
        },
    });
}
