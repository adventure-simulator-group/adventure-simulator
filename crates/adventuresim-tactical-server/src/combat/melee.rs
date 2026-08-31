use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each observer resource and query as an independent system parameter"
)]
pub(super) fn resolve_melee_attack(
    event: On<MeleeAttackIntent>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    spatial: SpatialQuery,
    q_character: Query<(&CharacterLook, &Transform)>,
    q_colliders: Query<&Collider>,
    q_dimensions: Query<&CharacterDimensions>,
    q_sides: Query<&TacticalCombatSide>,
    q_states: Query<&TacticalCombatState>,
    mut q_authorities: Query<&mut MeleeAttackAuthority>,
    q_bestiary_categories: Query<&BestiaryCategories>,
    q_pending: Query<&PendingDefenderResponse>,
    q_scene_items: Query<Entity, With<TacticalSceneItem>>,
    time: Res<Time<()>>,
    config: Res<TacticalCombatConfig>,
) {
    let attack_style = event.strike_family.melee_style();
    let entity = event.attacker;
    let hand = event.hand;
    let Ok(mut authority) = q_authorities.get_mut(entity) else {
        info!(attacker = ?entity, reason = "missing_authority", "melee_completion_rejected");
        return;
    };
    let attack_key = authority.attack_key().unwrap_or_default();

    let Ok(attacker_view) = viewer.get_for_attack(entity, hand).inspect_err(|err| {
        info!(attack_key, attacker = ?entity, reason = %err, "melee_completion_rejected");
    }) else {
        return;
    };
    let Ok(defender_view) = viewer.get(event.target).inspect_err(|err| {
        info!(attack_key, attacker = ?entity, target = ?event.target, reason = %err, "melee_completion_rejected");
    }) else {
        return;
    };

    let Ok([attacker_character, defender_character]) = q_character
        .get_many([entity, event.target])
        .inspect_err(|err| {
            info!(attack_key, attacker = ?entity, target = ?event.target, reason = %err, "melee_completion_rejected");
        })
    else {
        return;
    };
    let (attacker_look, attacker_transform) = attacker_character;
    let (defender_look, defender_transform) = defender_character;

    let weapon_reach = attacker_view.weapon_reach();
    let Ok(attacker_dimensions) = q_dimensions.get(entity) else {
        info!(attack_key, attacker = ?entity, reason = "missing_attacker_dimensions", "melee_completion_rejected");
        return;
    };
    let Ok(attacker_collider) = q_colliders.get(entity) else {
        info!(attack_key, attacker = ?entity, reason = "missing_attacker_collider", "melee_completion_rejected");
        return;
    };
    let Ok(_defender_collider) = q_colliders.get(event.target) else {
        info!(attack_key, attacker = ?entity, target = ?event.target, reason = "missing_defender_collider", "melee_completion_rejected");
        return;
    };
    let now = CombatInstant::from_elapsed(&time);
    let reach = melee_interaction_range(attacker_dimensions.arm_reach_metres, weapon_reach);
    let surface_distance = super::ingress::configured_body_part_surface_distance(
        super::ingress::melee_attack_origin(
            attacker_transform.translation,
            attacker_collider,
            *attacker_dimensions,
        ),
        defender_transform,
        event.body_part,
        &config,
    )
    .unwrap_or(f32::INFINITY);
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
        arm_reach: attacker_dimensions.arm_reach_metres,
        weapon_reach,
        range_latency_tolerance: config
            .realtime_authority
            .melee
            .range_latency_tolerance_metres,
        separation: surface_distance,
        authority_permits: authority.permits(event.target, event.body_part, now),
        body_part: event.body_part,
        attacker_position: attacker_transform.translation,
        target_position: defender_transform.translation,
        attacker_yaw: attacker_look.yaw,
        target_yaw: defender_look.yaw,
    };
    let validated = match validate_melee_intent_cheap(facts) {
        Ok(validated) => validated,
        Err(reason) => {
            info!(attack_key, attacker = ?entity, target = ?event.target, body_part = ?event.body_part, reason = ?reason, surface_distance_metres = surface_distance, reach_metres = reach, "melee_completion_rejected");
            info!(attack_key, attacker = ?entity, target = ?event.target, body_part = ?event.body_part, outcome = "miss", reason = ?reason, "melee_attack_resolved");
            return;
        }
    };
    let contact_sample = event.contact_sample;
    let line_of_sight = authoritative_line_of_sight(
        &spatial,
        &q_scene_items,
        validated.attacker(),
        validated.target(),
        validated.attacker_position(),
        validated.target_position(),
    );
    if let Err(reason) = validate_melee_line_of_sight(line_of_sight) {
        info!(attack_key, attacker = ?entity, target = ?event.target, body_part = ?event.body_part, reason = ?reason, surface_distance_metres = surface_distance, reach_metres = reach, "melee_completion_rejected");
        info!(attack_key, attacker = ?entity, target = ?event.target, body_part = ?event.body_part, outcome = "miss", reason = ?reason, "melee_attack_resolved");
        return;
    }
    // Mutate the pre-existing authority component synchronously. A later
    // completion in this same message flush observes the consumed windup and
    // active cooldown instead of reusing deferred Commands state.
    let cooldown =
        CombatDuration::from_secs_f32(config.realtime_authority.melee.replay_cooldown_seconds);
    let Some(authorized) = authority.authorize_attack(validated, now, cooldown) else {
        info!(attack_key, attacker = ?validated.attacker(), target = ?validated.target(), body_part = ?validated.body_part(), reason = "authorization_consumed", surface_distance_metres = surface_distance, reach_metres = reach, "melee_completion_rejected");
        info!(attack_key, attacker = ?validated.attacker(), target = ?validated.target(), body_part = ?validated.body_part(), outcome = "miss", reason = "authorization_consumed", "melee_attack_resolved");
        return;
    };
    let attack = authorized;
    info!(attack_key, attacker = ?attack.attacker(), target = ?attack.target(), body_part = ?attack.body_part(), surface_distance_metres = surface_distance, reach_metres = reach, "melee_completion_accepted");
    let (a2, a1) = attack.attacker_yaw().sin_cos();
    let (d2, d1) = attack.target_yaw().sin_cos();
    let flanking = flanking_from_dir((a1, a2), (d1, d2));

    let Some(attacker_side) = attacker_view.weapon_holding_side() else {
        info!(attack_key, attacker = ?attack.attacker(), target = ?attack.target(), body_part = ?attack.body_part(), outcome = "failed", reason = "missing_striking_side", "melee_attack_resolved");
        return;
    };
    let attacker_has_weapon = super::contact::attacker_has_weapon(&viewer, entity, hand);

    let pending = q_pending.get(attack.target()).ok();
    let defender_response = resolve_defender_response(
        pending,
        &time,
        &defender_view,
        &config.realtime_authority.defense,
    );

    // Consume the pending response so it is not reused.
    cmd.entity(attack.target())
        .remove::<PendingDefenderResponse>();

    let fallback_categories = BestiaryCategories::default();
    let defender_categories = q_bestiary_categories
        .get(attack.target())
        .unwrap_or(&fallback_categories);

    let (contact, result) = super::contact::resolve_melee_contact(
        &attacker_view,
        &defender_view,
        &defender_categories.0,
        attacker_side,
        attack_style,
        defender_response,
        attack.reported_precision(),
        flanking,
        contact_sample,
    );
    let attacker_weapon_slot = match attacker_side {
        BodySide::Left => EquipSlot::HoldingLeft,
        BodySide::Right => EquipSlot::HoldingRight,
        BodySide::Both => {
            info!(attack_key, attacker = ?attack.attacker(), target = ?attack.target(), body_part = ?attack.body_part(), outcome = "failed", reason = "ambiguous_striking_side", "melee_attack_resolved");
            return;
        }
    };
    let defender_parry_slot = matches!(defender_response, DefenderResponse::Parry { .. })
        .then(|| defender_view.shield_holding_side())
        .flatten()
        .and_then(|side| match side {
            BodySide::Left => Some(EquipSlot::HoldingLeft),
            BodySide::Right => Some(EquipSlot::HoldingRight),
            BodySide::Both => None,
        });
    let (hits_attacker, impact_velocity_change) = hit_velocity_change(
        result,
        attacker_transform.translation,
        defender_transform.translation,
        attacker_view.body_weight() + attacker_view.inventory_weight(),
        defender_view.body_weight() + defender_view.inventory_weight(),
        &config.realtime_authority.impact,
    );
    let impact_recipient = if hits_attacker {
        attack.attacker()
    } else {
        attack.target()
    };

    cmd.trigger(ApplyMeleeAttackResult {
        attacker: attack.attacker(),
        target: attack.target(),
        body_part: contact.body_part,
        result,
        attacker_weapon_slot,
        defender_parry_slot,
        attacker_weapon_contact: attacker_has_weapon,
        impact_recipient,
        impact_velocity_change,
    });

    match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            info!(
                attack_key,
                attacker = ?entity,
                target = ?attack.target(),
                body_part = ?contact.body_part,
                outcome = "failed",
                balance_damage,
                "melee_attack_resolved"
            );
        }
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } => {
            info!(
                attack_key,
                attacker = ?entity,
                target = ?attack.target(),
                body_part = ?contact.body_part,
                outcome = "connected",
                total_damage = cut_damage + blunt_damage,
                cut_damage,
                blunt_damage,
                balance_damage,
                "melee_attack_resolved"
            );
        }
    }

    cmd.server_trigger(ToClients {
        targets: SendTargets::All,
        message: SuccessfulAttackResponse {
            attacker: attack.attacker(),
            hit: vec![attack.target()],
            body_part: contact.body_part,
            result,
            flanking,
            defender_response,
            impact_recipient,
            impact_velocity_change,
        },
    });
}
