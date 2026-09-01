use super::*;

pub(in crate::iteration) fn record_attack_committed_to_defense(
    event: On<crate::combat::MeleeAttackCommittedToDefense>,
    clock: Res<IterationClock>,
    players: Query<&Player>,
    mut log: ResMut<IterationLog>,
) {
    let Ok(defender) = players.get(event.defender) else {
        return;
    };
    log.decision_events.push(TacticalDecisionLogEntry {
        tick: clock.tick,
        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
        combatant: defender.name.clone(),
        decision: TacticalDecision::Attack,
        status: TacticalDecisionStatus::CanceledForDefense,
        target: players
            .get(event.incoming_attacker)
            .ok()
            .map(|attacker| attacker.name.clone()),
        center_separation_metres: None,
        preferred_melee_measure_metres: None,
        attack_key: Some(event.canceled_attack_key),
        cause: Some(match event.response {
            DefenderResponse::Parry { .. } => "parry",
            DefenderResponse::Block { .. } => "block",
            DefenderResponse::Dodge { .. } => "dodge",
            DefenderResponse::None => "none",
        }),
    });
}

pub(in crate::iteration) fn record_attack_transformed_by_defense(
    event: On<crate::combat::MeleeAttackTransformedByDefense>,
    clock: Res<IterationClock>,
    players: Query<&Player>,
    mut log: ResMut<IterationLog>,
) {
    let Ok(defender) = players.get(event.defender) else {
        return;
    };
    log.decision_events.push(TacticalDecisionLogEntry {
        tick: clock.tick,
        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
        combatant: defender.name.clone(),
        decision: TacticalDecision::Attack,
        status: TacticalDecisionStatus::TransformedByDefense,
        target: players
            .get(event.incoming_attacker)
            .ok()
            .map(|attacker| attacker.name.clone()),
        center_separation_metres: None,
        preferred_melee_measure_metres: None,
        attack_key: Some(event.attack_key),
        cause: Some("offhand_block_reduced_attack_power"),
    });
}

pub(in crate::iteration) fn record_continuation_decision(
    event: On<crate::bot::BotContinuationDecisionEvent>,
    clock: Res<IterationClock>,
    players: Query<&Player>,
    mut log: ResMut<IterationLog>,
) {
    let Ok(combatant) = players.get(event.combatant) else {
        return;
    };
    let (decision, status) = match event.decision {
        crate::bot::BotContinuationDecision::Withdraw => {
            (TacticalDecision::Withdraw, TacticalDecisionStatus::Started)
        }
        crate::bot::BotContinuationDecision::Yield => {
            (TacticalDecision::Yield, TacticalDecisionStatus::Accepted)
        }
    };
    log.decision_events.push(TacticalDecisionLogEntry {
        tick: clock.tick,
        elapsed_seconds: clock.tick as f32 * TACTICAL_TICK_SECONDS,
        combatant: combatant.name.clone(),
        decision,
        status,
        target: None,
        center_separation_metres: None,
        preferred_melee_measure_metres: None,
        attack_key: None,
        cause: Some("unable_to_continue"),
    });
}
