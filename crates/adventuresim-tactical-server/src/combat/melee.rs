use super::authority::AuthorizedMeleeAttack;
use super::*;

mod active_defense;
use active_defense::{commit_defense_during_attack, resolve_active_defense};
mod outcome;
use outcome::emit_melee_outcome;

fn contact_at_completion(
    attacker: &TacticalPlayerView<'_, '_, '_>,
    authority: &MeleeAttackAuthority,
    surface_distance: f32,
    reach: f32,
    attack_style: MeleeAttackStyle,
    fallback_reach_fraction: f32,
) -> MeleeContactAtTime {
    let grip_to_tip_metres = attacker.weapon_grip_to_tip();
    let striking_head_length_metres = attacker.weapon_striking_head_length();
    let body_material = attacker.weapon_body_material();
    let striking_material = attacker.weapon_striking_material();
    let distal_headed = adventuresim_core::combat::has_distal_striking_surface(
        grip_to_tip_metres,
        striking_head_length_metres,
        body_material,
        striking_material,
    );
    resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
        scheduled_measure_metres: authority
            .scheduled_measure_metres()
            .unwrap_or(surface_distance),
        actual_measure_metres: surface_distance,
        ideal_measure_metres: adventuresim_core::combat::preferred_melee_striking_measure(
            reach,
            grip_to_tip_metres,
            striking_head_length_metres,
            distal_headed,
            fallback_reach_fraction,
        ),
        effective_reach_metres: reach,
        grip_to_tip_metres,
        total_length_metres: attacker.weapon_total_length(),
        striking_head_length_metres,
        distal_headed,
        attack_style,
        body_material,
        striking_material,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "validation facts join both live actor projections"
)]
fn completion_intent_facts(
    event: &MeleeAttackIntent,
    attacker: &TacticalPlayerView<'_, '_, '_>,
    authority: &MeleeAttackAuthority,
    attacker_look: &CharacterLook,
    attacker_transform: &Transform,
    defender_look: &CharacterLook,
    defender_transform: &Transform,
    attacker_dimensions: &CharacterDimensions,
    weapon_reach: f32,
    surface_distance: f32,
    now: CombatInstant,
    sides: &Query<&TacticalCombatSide>,
    states: &Query<&mut TacticalCombatState>,
    config: &TacticalCombatConfig,
) -> MeleeIntentFacts {
    MeleeIntentFacts {
        attacker: event.attacker,
        target: event.target,
        attacker_side: sides.get(event.attacker).ok().copied(),
        target_side: sides.get(event.target).ok().copied(),
        attacker_incapacitated: states
            .get(event.attacker)
            .ok()
            .map(TacticalCombatState::is_incapacitated),
        target_incapacitated: states
            .get(event.target)
            .ok()
            .map(TacticalCombatState::is_incapacitated),
        attack_capability: adventuresim_core::combat::melee_attack_capability(attacker, attacker),
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
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "authority validation consumes spatial and timing facts"
)]
fn authorize_completion(
    facts: MeleeIntentFacts,
    authority: &mut MeleeAttackAuthority,
    spatial: &SpatialQuery,
    scene_items: &Query<Entity, With<TacticalSceneItem>>,
    now: CombatInstant,
    config: &TacticalCombatConfig,
    attack_key: u64,
    surface_distance: f32,
    reach: f32,
) -> Option<AuthorizedMeleeAttack> {
    let validated = match validate_melee_intent_cheap(facts) {
        Ok(validated) => validated,
        Err(reason) => {
            info!(attack_key, attacker = ?facts.attacker, target = ?facts.target, body_part = ?facts.body_part, reason = ?reason, surface_distance_metres = surface_distance, reach_metres = reach, "melee_completion_rejected");
            info!(attack_key, attacker = ?facts.attacker, target = ?facts.target, body_part = ?facts.body_part, outcome = "miss", reason = ?reason, "melee_attack_resolved");
            return None;
        }
    };
    let line_of_sight = authoritative_line_of_sight(
        spatial,
        scene_items,
        validated.attacker(),
        validated.target(),
        validated.attacker_position(),
        validated.target_position(),
    );
    if let Err(reason) = validate_melee_line_of_sight(line_of_sight) {
        info!(attack_key, attacker = ?facts.attacker, target = ?facts.target, body_part = ?facts.body_part, reason = ?reason, surface_distance_metres = surface_distance, reach_metres = reach, "melee_completion_rejected");
        info!(attack_key, attacker = ?facts.attacker, target = ?facts.target, body_part = ?facts.body_part, outcome = "miss", reason = ?reason, "melee_attack_resolved");
        return None;
    }
    let cooldown =
        CombatDuration::from_secs_f32(config.realtime_authority.melee.replay_cooldown_seconds);
    authority.authorize_attack(validated, now, cooldown).or_else(|| {
        info!(attack_key, attacker = ?facts.attacker, target = ?facts.target, body_part = ?facts.body_part, reason = "authorization_consumed", surface_distance_metres = surface_distance, reach_metres = reach, "melee_completion_rejected");
        info!(attack_key, attacker = ?facts.attacker, target = ?facts.target, body_part = ?facts.body_part, outcome = "miss", reason = "authorization_consumed", "melee_attack_resolved");
        None
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "alignment compares complete attacker and defender views"
)]
fn weapon_defense_alignment(
    attacker: &TacticalPlayerView<'_, '_, '_>,
    defender: &TacticalPlayerView<'_, '_, '_>,
    attacker_side: BodySide,
    attack_style: MeleeAttackStyle,
    precision: ReportedPrecision,
    flanking: f32,
    response: DefenderResponse,
    sample: f32,
    contact_at_time: MeleeContactAtTime,
) -> Option<adventuresim_core::combat::WeaponDefenseAlignment> {
    response.is_weapon_contact().then(|| {
        let attack_value = adventuresim_core::combat::melee_attack_value_by_parts(
            attacker,
            attacker,
            attacker,
            attacker,
            attacker,
            attacker_side,
            attack_style,
            adventuresim_core::combat::melee_measure_adjusted_precision(
                precision.get(),
                contact_at_time,
            ),
            flanking,
            response,
            defender,
            defender,
            defender,
            defender,
            defender,
        );
        resolve_weapon_defense_alignment(response, attack_value, sample)
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "contact resolution consumes the defended strike projection"
)]
fn contact_after_defense(
    attacker: &TacticalPlayerView<'_, '_, '_>,
    defender: &TacticalPlayerView<'_, '_, '_>,
    defender_categories: &[BestiaryCategory],
    attacker_side: BodySide,
    attack_style: MeleeAttackStyle,
    response: DefenderResponse,
    precision: ReportedPrecision,
    flanking: f32,
    contact_sample: f32,
    contact_at_time: MeleeContactAtTime,
    config: &TacticalCombatConfig,
) -> (MeleeContactLocation, AttackResult) {
    super::contact::resolve_melee_contact(
        attacker,
        defender,
        defender_categories,
        config.resolution,
        attacker_side,
        attack_style,
        response,
        precision,
        flanking,
        contact_sample,
        None,
        contact_at_time,
    )
}

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
    let now = CombatInstant::from_elapsed(&time);
    let reach = melee_interaction_range(attacker_dimensions.arm_reach_metres, weapon_reach);
    let center_separation = attacker_transform
        .translation
        .xz()
        .distance(defender_transform.translation.xz());
    let surface_distance = (center_separation
        - adventuresim_core::combat::HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES)
        .max(0.0);
    let contact_at_time = contact_at_completion(
        &attacker_view,
        &authority,
        surface_distance,
        reach,
        attack_style,
        config.ai.ordinary.offense.melee_measure_reach_fraction,
    );
    let facts = completion_intent_facts(
        &event,
        &attacker_view,
        &authority,
        attacker_look,
        attacker_transform,
        defender_look,
        defender_transform,
        attacker_dimensions,
        weapon_reach,
        surface_distance,
        now,
        &q_sides,
        &q_states,
        &config,
    );
    let contact_sample = event.contact_sample;
    let defense_alignment_sample = event.defense_alignment_sample;
    let Some(attack) = authorize_completion(
        facts,
        &mut authority,
        &spatial,
        &q_scene_items,
        now,
        &config,
        attack_key,
        surface_distance,
        reach,
    ) else {
        return;
    };
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

    let Some(attempted_defender_response) = resolve_active_defense(
        &attack,
        &attacker_view,
        &defender_view,
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
    let defense_alignment = weapon_defense_alignment(
        &attacker_view,
        &defender_view,
        attacker_side,
        attack_style,
        fatigued_precision,
        flanking,
        attempted_defender_response,
        defense_alignment_sample,
        contact_at_time,
    );
    let effective_defender_response =
        defense_alignment.map_or(attempted_defender_response, |alignment| alignment.effective);
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
    let (contact, result) = contact_after_defense(
        &attacker_view,
        &defender_view,
        &defender_categories.0,
        attacker_side,
        attack_style,
        effective_defender_response,
        fatigued_precision,
        flanking,
        contact_sample,
        contact_at_time,
        &config,
    );
    let result = result * attacker_performance * attack.power_multiplier();
    emit_melee_outcome(
        &mut cmd,
        &attack,
        attack_key,
        entity,
        hand,
        attacker_side,
        attempted_defender_response,
        defense_alignment,
        contact_at_time,
        contact,
        result,
        flanking,
        attacker_has_weapon,
        &attacker_view,
        &defender_view,
        attacker_transform,
        defender_transform,
        &viewer,
        &config,
    );
}
