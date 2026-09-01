use super::authority::AuthorizedMeleeAttack;
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
    mut q_states: Query<&mut TacticalCombatState>,
    mut q_skeletons: Query<&mut SkeletonState>,
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
    let grip_to_tip_metres = attacker_view.weapon_grip_to_tip();
    let striking_head_length_metres = attacker_view.weapon_striking_head_length();
    let body_material = attacker_view.weapon_body_material();
    let striking_material = attacker_view.weapon_striking_material();
    let contact_at_time = resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
        scheduled_measure_metres: authority
            .scheduled_measure_metres()
            .unwrap_or(surface_distance),
        actual_measure_metres: surface_distance,
        effective_reach_metres: reach,
        grip_to_tip_metres,
        total_length_metres: attacker_view.weapon_total_length(),
        striking_head_length_metres,
        distal_headed: adventuresim_core::combat::has_distal_striking_surface(
            grip_to_tip_metres,
            striking_head_length_metres,
            body_material,
            striking_material,
        ),
        attack_style,
        body_material,
        striking_material,
    });
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
        attack_capability: adventuresim_core::combat::melee_attack_capability(
            &attacker_view,
            &attacker_view,
        ),
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
    let defense_alignment_sample = event.defense_alignment_sample;
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
    let attacker_performance = q_states.get(attack.attacker()).map_or(1.0, |state| {
        combat_fatigue_performance(
            state.oxygen_debt_joules,
            state.local_action_fatigue,
            attacker_view.raw_single_body_part_attr(SimpleAttribute::Endurance),
        )
    });

    let Some((attempted_defender_response, dodge_geometry)) = resolve_active_defense(
        &attack,
        &attacker_view,
        &defender_view,
        attacker_transform,
        defender_transform,
        attacker_performance,
        attack_style,
        contact_sample,
        &q_pending,
        &mut q_authorities,
        &mut q_skeletons,
        &mut q_states,
        &time,
        &config,
    ) else {
        return;
    };

    // Consume the pending response so it is not reused.
    cmd.entity(attack.target())
        .remove::<PendingDefenderResponse>();

    let fallback_categories = BestiaryCategories::default();
    let defender_categories = q_bestiary_categories
        .get(attack.target())
        .unwrap_or(&fallback_categories);

    let fatigued_precision =
        ReportedPrecision::new(attack.reported_precision().get() * attacker_performance)
            .expect("fatigue preserves finite bounded precision");
    let defense_alignment = attempted_defender_response.is_weapon_contact().then(|| {
        let attack_value = adventuresim_core::combat::melee_attack_value_by_parts(
            &attacker_view,
            &attacker_view,
            &attacker_view,
            &attacker_view,
            &attacker_view,
            attacker_side,
            attack_style,
            fatigued_precision.get(),
            flanking,
            attempted_defender_response,
            &defender_view,
            &defender_view,
            &defender_view,
            &defender_view,
            &defender_view,
        );
        resolve_weapon_defense_alignment(
            attempted_defender_response,
            attack_value,
            defense_alignment_sample,
        )
    });
    let effective_defender_response = defense_alignment.map_or_else(
        || {
            if matches!(attempted_defender_response, DefenderResponse::Dodge { .. })
                && dodge_geometry.is_some_and(|geometry| geometry.contacted_body_part.is_some())
            {
                DefenderResponse::None
            } else {
                attempted_defender_response
            }
        },
        |alignment| alignment.effective,
    );
    commit_defense_during_attack(
        &mut cmd,
        &attack,
        attempted_defender_response,
        effective_defender_response,
        defense_alignment.map_or(1.0, |alignment| alignment.engagement),
        &defender_view,
        &mut q_authorities,
        &mut q_skeletons,
    );
    let redirected_body_part = dodge_geometry.and_then(|geometry| geometry.contacted_body_part);
    let (contact, result) =
        if dodge_geometry.is_some_and(|geometry| geometry.contacted_body_part.is_none()) {
            (
                MeleeContactLocation::new(
                    attack.body_part(),
                    anatomical_subregion(attack.body_part(), 1.0),
                    1.0,
                    None,
                ),
                AttackResult::ToAttacker {
                    balance_damage: 0.0,
                    contact_force: 0.0,
                    physical_contact: false,
                },
            )
        } else {
            super::contact::resolve_melee_contact(
                &attacker_view,
                &defender_view,
                &defender_categories.0,
                config.resolution,
                attacker_side,
                attack_style,
                effective_defender_response,
                fatigued_precision,
                flanking,
                contact_sample,
                redirected_body_part,
                contact_at_time,
            )
        };
    let result = result * attacker_performance * attack.power_multiplier();
    let Some(attacker_weapon_slot) = weapon_slot_for_side(Some(attacker_side)) else {
        info!(attack_key, attacker = ?attack.attacker(), target = ?attack.target(), body_part = ?attack.body_part(), outcome = "failed", reason = "ambiguous_striking_side", "melee_attack_resolved");
        return;
    };
    let defender_blocking_slot = defender_blocking_slot(
        attempted_defender_response,
        defender_view.shield_holding_side(),
        defender_view.weapon_holding_side(),
    );
    let (impact_recipient, impact_velocity_change, impact_point, impact_normal) =
        authoritative_impact(
            result,
            attack.attacker(),
            attacker_transform.translation,
            attacker_view.body_weight() + attacker_view.inventory_weight(),
            attack.target(),
            defender_transform,
            defender_view.body_weight() + defender_view.inventory_weight(),
            contact.body_part,
            &config,
        );
    let impact_effects = authoritative_impact_effects(&viewer.inventory, entity, hand, result);

    cmd.trigger(ApplyMeleeAttackResult {
        attacker: attack.attacker(),
        target: attack.target(),
        body_part: contact.body_part,
        anatomical_subregion: contact.anatomical_subregion,
        surface_coordinate: contact.surface_coordinate,
        result,
        defender_response: attempted_defender_response,
        defense_success_probability: defense_alignment
            .map(|alignment| alignment.success_probability),
        defense_alignment_sample: defense_alignment.map(|alignment| alignment.alignment_sample),
        defense_engagement: defense_alignment.map(|alignment| alignment.engagement),
        attacker_weapon_slot,
        defender_blocking_slot,
        attacker_weapon_contact: attacker_has_weapon,
        impact_recipient,
        impact_velocity_change,
        closest_approach_metres: dodge_geometry.map(|geometry| geometry.closest_approach_metres),
        redirected_from: redirected_body_part
            .filter(|body_part| *body_part != attack.body_part())
            .map(|_| attack.body_part()),
        contact_at_time,
    });

    log_melee_result(
        attack_key,
        entity,
        attack.target(),
        contact.body_part,
        result,
    );

    cmd.server_trigger(ToClients {
        targets: SendTargets::All,
        message: SuccessfulAttackResponse {
            attacker: attack.attacker(),
            hit: vec![attack.target()],
            body_part: contact.body_part,
            result,
            flanking,
            defender_response: attempted_defender_response,
            impact_recipient,
            impact_velocity_change,
            impact_point,
            impact_normal,
            impact_effects,
        },
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "active defense bridges live authority, animation, physiology, and geometry"
)]
fn resolve_active_defense(
    attack: &AuthorizedMeleeAttack,
    attacker_view: &TacticalPlayerView<'_, '_, '_>,
    defender_view: &TacticalPlayerView<'_, '_, '_>,
    attacker_transform: &Transform,
    defender_transform: &Transform,
    attacker_performance: f32,
    attack_style: MeleeAttackStyle,
    contact_sample: f32,
    q_pending: &Query<&PendingDefenderResponse>,
    q_authorities: &mut Query<&mut MeleeAttackAuthority>,
    q_skeletons: &mut Query<&mut SkeletonState>,
    q_states: &mut Query<&mut TacticalCombatState>,
    time: &Time<()>,
    config: &TacticalCombatConfig,
) -> Option<(DefenderResponse, Option<MeleeDodgeGeometry>)> {
    let defender_skeleton = q_skeletons.get(attack.target()).ok()?;
    let pending = q_pending.get(attack.target()).ok();
    let (defender_incapacitation, defender_fatigue_performance) =
        q_states.get(attack.target()).map_or((0.0, 1.0), |state| {
            (
                state.incapacitation,
                combat_fatigue_performance(
                    state.oxygen_debt_joules,
                    state.local_action_fatigue,
                    defender_view.raw_single_body_part_attr(SimpleAttribute::Endurance),
                ),
            )
        });
    let response = {
        let mut defender_authority = q_authorities.get_mut(attack.target()).ok();
        resolve_melee_defender_response(
            pending,
            time,
            defender_view,
            defender_skeleton,
            defender_authority.as_deref_mut(),
            defender_incapacitation,
            defender_fatigue_performance,
            attack.attacker(),
            attack.started_at(),
            &config.realtime_authority.defense,
        )
    }
    .scaled_for_performance(defender_fatigue_performance);
    let attacker_side = attacker_view.weapon_holding_side()?;
    let preview_contact = attacker_view.melee_contact_location(
        attacker_side,
        attack_style,
        defender_view,
        attack.reported_precision().get() * attacker_performance,
        contact_sample,
    );
    let response = shield_aligned_response(
        response,
        defender_view.shield_holding_side(),
        preview_contact,
    );
    let dodge = matches!(response, DefenderResponse::Dodge { .. }).then(|| {
        let intended_target = pending.map_or(defender_transform.translation, |value| value.origin);
        let attack_origin = attacker_transform.translation.xz();
        let intended_target = intended_target.xz();
        let displaced_target = defender_transform.translation.xz();
        let defender_leg_agility = defender_view.limb_attr_by_weight_by_parts(
            LimbAttribute::Agility,
            defender_view,
            LimbWeights::both_legs(),
        );
        let attacker_arm_agility = attacker_view.limb_attr_by_weight_by_parts(
            LimbAttribute::Agility,
            attacker_view,
            LimbWeights::both_arms(),
        );
        let displacement_time_seconds = pending.map_or(0.0, |value| {
            CombatInstant::from_elapsed(time)
                .elapsed_since(value.set_at)
                .as_secs_f32()
        });
        resolve_melee_dodge_geometry(
            (attack_origin.x, attack_origin.y),
            (intended_target.x, intended_target.y),
            (displaced_target.x, displaced_target.y),
            attack.body_part(),
            MeleeDodgeKinematics {
                defender_leg_agility,
                defender_fatigue_performance: q_states.get(attack.target()).map_or(1.0, |state| {
                    combat_fatigue_performance(
                        state.oxygen_debt_joules,
                        state.local_action_fatigue,
                        defender_view.raw_single_body_part_attr(SimpleAttribute::Endurance),
                    )
                }),
                defender_body_mass_kg: defender_view.body_weight(),
                defender_equipment_mass_kg: defender_view.inventory_weight(),
                displacement_time_seconds,
                attacker_tracking: (attacker_arm_agility / 5.0).clamp(0.0, 1.0)
                    * attacker_performance
                    / (1.0 + attacker_view.weapon_moment_of_inertia().max(0.0) * 2.0),
                weapon_reach_metres: attacker_view.weapon_reach().max(0.4),
                committed_arc_radians: match attack_style {
                    MeleeAttackStyle::Swing => 0.8,
                    MeleeAttackStyle::Stab => 0.25,
                },
            },
        )
    });
    if matches!(
        response,
        DefenderResponse::Block { .. } | DefenderResponse::Parry { .. }
    ) && let Ok(mut state) = q_states.get_mut(attack.target())
    {
        let state = &mut *state;
        let workload = combat_action_workload(
            CombatActionWork::WeaponDefense,
            config.realtime_authority.defense.reflex_window_seconds,
            defender_view.weapon_weight(),
            defender_view.weapon_moment_of_inertia(),
            defender_view.inventory_weight(),
            defender_view.body_weight(),
            defender_view.raw_single_body_part_attr(SimpleAttribute::Endurance),
        );
        apply_combat_workload(
            &mut state.oxygen_debt_joules,
            &mut state.local_action_fatigue,
            workload,
            defender_view.raw_single_body_part_attr(SimpleAttribute::Endurance),
        );
    }
    Some((response, dodge))
}

#[expect(
    clippy::too_many_arguments,
    reason = "committed defense bridges attack authority, animation, and implement ownership"
)]
fn commit_defense_during_attack(
    cmd: &mut Commands,
    incoming: &AuthorizedMeleeAttack,
    attempted: DefenderResponse,
    effective: DefenderResponse,
    engagement: f32,
    defender_view: &TacticalPlayerView<'_, '_, '_>,
    q_authorities: &mut Query<&mut MeleeAttackAuthority>,
    q_skeletons: &mut Query<&mut SkeletonState>,
) {
    // A same-weapon parry necessarily redirects the committed attack even
    // when the intercept misses. Its engagement controls only the incoming
    // weapon interaction, not whether the defender spent their own attack.
    if matches!(attempted, DefenderResponse::Parry { .. })
        && let Ok(mut authority) = q_authorities.get_mut(incoming.target())
        && let Some(canceled_attack_key) = authority.commit_attack_to_defense()
    {
        if let Ok(mut skeleton) = q_skeletons.get_mut(incoming.target()) {
            skeleton.commit_attack_to_defense();
        }
        cmd.entity(incoming.target())
            .remove::<PendingMeleeContact>()
            .remove::<MeleeLungeMovement>();
        cmd.trigger(MeleeAttackCommittedToDefense {
            defender: incoming.target(),
            incoming_attacker: incoming.attacker(),
            canceled_attack_key,
            response: attempted,
            engagement,
        });
        return;
    }

    // An off-hand shield leaves the sword attack alive, but only the portion
    // of shield motion that actually engages the incoming line can bind and
    // reduce the outgoing strike. A grossly misaligned attempt therefore
    // charges work without granting a free full-strength transformation.
    if defender_view.shield_block_bonus() > 0.0
        && let DefenderResponse::Block { effectiveness } = effective
        && let Ok(mut authority) = q_authorities.get_mut(incoming.target())
        && let Some((attack_key, retained_power)) =
            authority.transform_attack_for_offhand_defense(effectiveness)
    {
        cmd.trigger(MeleeAttackTransformedByDefense {
            defender: incoming.target(),
            incoming_attacker: incoming.attacker(),
            attack_key,
            retained_power,
            response: attempted,
            engagement,
        });
    }
}

fn log_melee_result(
    attack_key: u64,
    attacker: Entity,
    target: Entity,
    body_part: BodyPart,
    result: AttackResult,
) {
    match result {
        AttackResult::ToAttacker { balance_damage, .. } => info!(
            attack_key,
            ?attacker,
            ?target,
            ?body_part,
            outcome = "failed",
            balance_damage,
            "melee_attack_resolved"
        ),
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } => info!(
            attack_key,
            ?attacker,
            ?target,
            ?body_part,
            outcome = "connected",
            total_damage = cut_damage + blunt_damage,
            cut_damage,
            blunt_damage,
            balance_damage,
            "melee_attack_resolved"
        ),
    }
}
