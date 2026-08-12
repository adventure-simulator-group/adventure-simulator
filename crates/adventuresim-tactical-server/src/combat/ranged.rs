use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_ranged_attack(
    event: On<RangedAttackIntent>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    spatial: SpatialQuery,
    q_character: Query<(&CharacterLook, &Transform)>,
    q_sides: Query<&TacticalCombatSide>,
    q_states: Query<&TacticalCombatState>,
    mut q_authorities: Query<&mut RangedAttackAuthority>,
    q_bestiary_categories: Query<&BestiaryCategories>,
    q_pending: Query<&PendingDefenderResponse>,
    q_scene_items: Query<Entity, With<TacticalSceneItem>>,
    q_ammo: Query<(Entity, &ItemOf, &ItemProperties, &ItemQuantity)>,
    q_ids: Query<&CharacterId>,
    mut consequences: ResMut<TacticalConsequenceAccumulator>,
    time: Res<Time<()>>,
) {
    let attacker = event.attacker;
    let Ok(attacker_view) = viewer.get(attacker) else {
        return;
    };
    let Ok((attacker_look, attacker_transform)) = q_character.get(attacker) else {
        return;
    };
    let target_character = event.target.and_then(|target| q_character.get(target).ok());
    let now = CombatInstant::from_elapsed(&time);
    let Ok(mut authority) = q_authorities.get_mut(attacker) else {
        return;
    };
    let facts = RangedIntentFacts {
        attacker,
        target: event.target,
        attacker_side: q_sides.get(attacker).ok().copied(),
        target_side: event
            .target
            .and_then(|target| q_sides.get(target).ok().copied()),
        attacker_incapacitated: q_states
            .get(attacker)
            .ok()
            .map(TacticalCombatState::is_incapacitated),
        target_incapacitated: event.target.and_then(|target| {
            q_states
                .get(target)
                .ok()
                .map(TacticalCombatState::is_incapacitated)
        }),
        reported_precision: event.reported_precision,
        weapon_is_ranged: attacker_view.weapon_is_ranged(),
        weapon_range: attacker_view.weapon_reach(),
        separation: target_character.map(|(_, transform)| {
            attacker_transform
                .translation
                .distance(transform.translation)
        }),
        target_in_aim_cone: target_character.map(|(_, transform)| {
            ranged_target_in_aim_cone(
                attacker_look.yaw,
                attacker_transform.translation,
                transform.translation,
            )
        }),
        authority_permits: authority.permits(now),
        body_part: event.body_part,
        attacker_position: attacker_transform.translation,
        target_position: target_character.map(|(_, transform)| transform.translation),
        attacker_yaw: attacker_look.yaw,
        target_yaw: target_character.map(|(look, _)| look.yaw),
    };
    let validated = match validate_ranged_intent(facts) {
        Ok(validated) => validated,
        Err(reason) => {
            if !matches!(reason, RangedIntentRejection::Windup) {
                debug!("Rejected ranged intent from {attacker:?}: {reason:?}");
            }
            return;
        }
    };
    if let ValidatedRangedImpact::Hit {
        target,
        target_position,
        ..
    } = validated.impact()
    {
        let line_of_sight = authoritative_line_of_sight(
            &spatial,
            &q_scene_items,
            validated.attacker(),
            target,
            validated.attacker_position(),
            target_position,
        );
        if !line_of_sight {
            debug!(
                "Rejected ranged intent from {attacker:?}: {:?}",
                RangedIntentRejection::BlockedLineOfSight
            );
            return;
        }
    }
    let Some(authorized) = authority.authorize_shot(validated, now, RANGED_COOLDOWN) else {
        return;
    };
    let shot = authorized;

    // Only an otherwise-authorized shot reaches the global inventory scan.
    // A dry fire still consumes its windup/cooldown, bounding repeated scans.
    let ammo = q_ammo.iter().find(|(_, owner, properties, quantity)| {
        owner.0 == shot.attacker() && properties.id == ARROW_ID && quantity.0.get() > 0
    });
    let Some((ammo_entity, _, _, quantity)) = ammo else {
        return;
    };
    if let Some(remaining) = remaining_ammo_after_shot(quantity.0) {
        cmd.entity(ammo_entity).insert(ItemQuantity(remaining));
    } else {
        cmd.entity(ammo_entity).despawn();
    }
    if shot.attacker_side() == TacticalCombatSide::Party
        && let Ok(player_id) = q_ids.get(shot.attacker())
    {
        record_party_ammunition_use(&mut consequences, *player_id);
    }

    let ValidatedRangedImpact::Hit {
        target,
        body_part,
        target_yaw,
        ..
    } = shot.impact()
    else {
        return;
    };
    let Ok(defender_view) = viewer.get(target) else {
        return;
    };
    let (a2, a1) = shot.attacker_yaw().sin_cos();
    let (d2, d1) = target_yaw.sin_cos();
    let flanking = flanking_from_dir((a1, a2), (d1, d2));
    let defender_response =
        resolve_defender_response(q_pending.get(target).ok(), &time, &defender_view);
    cmd.entity(target).remove::<PendingDefenderResponse>();
    let fallback_categories = BestiaryCategories::default();
    let defender_categories = q_bestiary_categories
        .get(target)
        .unwrap_or(&fallback_categories);
    let result = attacker_view.resolve_ranged_attack(
        &defender_view,
        &defender_categories.0,
        defender_response,
        shot.reported_precision().get(),
        flanking,
        body_part,
    );
    let defender_parry_slot = matches!(defender_response, DefenderResponse::Parry { .. })
        .then(|| defender_view.shield_holding_side())
        .flatten()
        .and_then(|side| match side {
            BodySide::Left => Some(EquipSlot::HoldingLeft),
            BodySide::Right => Some(EquipSlot::HoldingRight),
            BodySide::Both => None,
        });
    let attacker_weapon_slot = match attacker_view.weapon_holding_side() {
        Some(BodySide::Left) => EquipSlot::HoldingLeft,
        _ => EquipSlot::HoldingRight,
    };
    cmd.trigger(ApplyMeleeAttackResult {
        attacker: shot.attacker(),
        target,
        body_part,
        result,
        attacker_weapon_slot,
        defender_parry_slot,
        attacker_weapon_contact: false,
    });
    cmd.server_trigger(ToClients {
        targets: SendTargets::All,
        message: SuccessfulAttackResponse {
            attacker: shot.attacker(),
            hit: vec![target],
            body_part,
            result,
            flanking,
            defender_response,
        },
    });
}
