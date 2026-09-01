use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "the authority record retains the complete authored attack identity and timing"
)]
pub(super) fn authorize_started_attack(
    commands: &mut Commands,
    authority: &mut MeleeAttackAuthority,
    event: &MeleeAttackStartedIntent,
    selected_body_part: Option<BodyPart>,
    contact_sample: f32,
    defense_alignment_sample: f32,
    attack_key: u64,
    contact_windup: CombatDuration,
    scheduled_measure_metres: f32,
    time: &Time<()>,
    config: &TacticalCombatConfig,
) {
    let now = CombatInstant::from_elapsed(time);
    authority.observe(
        attack_key,
        event.target,
        selected_body_part,
        now,
        contact_windup,
        CombatDuration::from_secs_f32(config.realtime_authority.melee.completion_allowance_seconds),
        scheduled_measure_metres,
        event.reported_precision,
    );
    commands.entity(event.attacker).insert(PendingMeleeContact {
        attack_key,
        target: event.target,
        body_part: selected_body_part,
        contact_sample,
        defense_alignment_sample,
        resolve_at: now + contact_windup,
        reported_precision: event.reported_precision,
        strike_family: event.strike_family,
        hand: event.hand,
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "movement planning receives authored attack timing and production collision queries"
)]
pub(super) fn begin_started_attack_movement(
    commands: &mut Commands,
    event: &MeleeAttackStartedIntent,
    selected_body_part: Option<BodyPart>,
    weapon_reach: f32,
    attack_key: u64,
    animation_start_tick: u64,
    contact_tick: u64,
    transforms: &Query<&Transform>,
    dimensions: &Query<&CharacterDimensions>,
    colliders: &Query<&Collider>,
    config: &TacticalCombatConfig,
) {
    info!(attack_key, attacker = ?event.attacker, target = ?event.target, body_part = ?selected_body_part, strike_family = ?event.strike_family, hand = ?event.hand, "melee_attack_started");
    begin_attack_facing(
        commands,
        event.attacker,
        event.target,
        contact_tick,
        transforms,
    );
    if let (Some(target), Some(body_part)) = (event.target, selected_body_part) {
        begin_melee_lunge(
            commands,
            EntityMeleeLungeRequest {
                attacker: event.attacker,
                target,
                body_part,
                weapon_reach_metres: weapon_reach,
            },
            animation_start_tick,
            transforms,
            dimensions,
            colliders,
            config,
        );
    } else {
        commands
            .entity(event.attacker)
            .remove::<MeleeLungeMovement>();
        info!(attack_key, attacker = ?event.attacker, target = ?event.target, body_part = ?selected_body_part, outcome = "untargeted_no_movement", "melee_lunge_planned");
    }
}
