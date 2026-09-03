use super::*;

mod assignment;
mod commitment;
mod mobility;
mod scheduling;

pub(in crate::autoresolve) use assignment::*;
pub(in crate::autoresolve) use commitment::*;
pub(in crate::autoresolve) use mobility::*;
pub(in crate::autoresolve) use scheduling::*;

#[expect(
    clippy::too_many_arguments,
    reason = "the discrete-event contact boundary joins both mutable sides, seeded sampling, and exact scheduled identity"
)]
pub(super) fn resolve_melee_turn(
    attacker_index: usize,
    target_index: usize,
    flanking: f32,
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
    attack_timing: ScheduledMeleeTiming,
    contact_batch: MeleeContactBatch,
) {
    let prepared = prepare_melee_exchange(
        &mut attackers[attacker_index],
        &defenders[target_index],
        flanking,
        random,
        parameters,
        attack_timing,
    );
    let PreparedMeleeExchange {
        attack_power_multiplier,
        contact_at_time,
        scheduled_defender_timing_before,
        defender_phase,
        defender_decision,
        response,
        exchange,
        result,
        part,
        attacker_incapacitation_performance,
        attack_duration,
    } = prepared;
    attackers[attacker_index].charge_action_work(CombatActionWork::Attack, attack_duration);
    let defense_commitment = apply_defender_response(
        &mut defenders[target_index],
        attackers[attacker_index].id,
        attackers[attacker_index].melee_engagement_distance_metres,
        scheduled_defender_timing_before,
        defender_phase,
        defender_decision,
        response,
        exchange.effective_response,
        attack_timing,
        parameters,
        recorder,
    );
    record_melee_result(
        &mut attackers[attacker_index],
        &mut defenders[target_index],
        round,
        response,
        result,
        part,
        &exchange,
        contact_at_time,
        defense_commitment,
        attack_power_multiplier,
        attacker_incapacitation_performance,
        attack_duration,
        attack_timing,
        contact_batch,
        recorder,
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "recording retains all causal contact facts"
)]
fn record_melee_result(
    attacker: &mut Combatant,
    defender: &mut Combatant,
    round: usize,
    response: DefenderResponse,
    result: AttackResult,
    part: BodyPart,
    exchange: &MeleeExchangeOutcome,
    contact: MeleeContactAtTime,
    commitment: DefenseCommitment,
    power: f32,
    performance: f32,
    duration: f32,
    timing: ScheduledMeleeTiming,
    batch: MeleeContactBatch,
    recorder: &mut BattleRecorder,
) {
    let effect = apply_attack_result(attacker, defender, result, part);
    recorder.record_attack(
        "main",
        round,
        attacker.id,
        defender.id,
        AttackMode::Melee,
        attacker.equipment.melee_weapon_id,
        None,
        melee_defender_contact_item_id(result, response, &defender.equipment),
        response_choice(response),
        part,
        result,
        effect,
        Some(MeleeContactTelemetry {
            anatomical_subregion: exchange.contact.anatomical_subregion,
            surface_coordinate: exchange.contact.surface_coordinate,
            armor_layer_chain: autoresolve_armor_layer_chain(&defender.equipment, exchange.contact),
            scheduled_contact_measure_metres: contact.scheduled_measure_metres,
            ideal_contact_measure_metres: contact.ideal_measure_metres,
            actual_contact_measure_metres: contact.actual_measure_metres,
            actual_center_separation_metres: contact.actual_measure_metres
                + HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
            contact_classification: contact.classification,
            contact_lever_arm_metres: contact.lever_arm_metres,
            contact_energy_fraction: contact.energy_fraction,
            measure_accuracy_multiplier: contact.measure_accuracy_multiplier,
            contact_invalidation_cause: contact.invalidation_cause,
            contact_material: contact.contact_material,
            defense_success_probability: exchange
                .defense_alignment
                .map(|alignment| alignment.success_probability),
            defense_alignment_sample: exchange
                .defense_alignment
                .map(|alignment| alignment.alignment_sample),
            defense_engagement: exchange
                .defense_alignment
                .map(|alignment| alignment.engagement),
            effective_defender_response: response_name(exchange.effective_response),
            defender_attack_commitment: commitment.kind.as_str(),
            defender_retained_attack_power: commitment.retained_power,
            attack_power_multiplier: power,
            attacker_incapacitation_performance: performance,
            attack_interval_seconds: duration / performance,
        }),
    );
    record_contact_timeline(recorder, attacker, defender.id, timing, batch);
}

fn record_contact_timeline(
    recorder: &mut BattleRecorder,
    attacker: &Combatant,
    defender_id: u64,
    timing: ScheduledMeleeTiming,
    batch: MeleeContactBatch,
) {
    let mut event = MeleeTimelineEvent::at(MeleeTimelineKind::Contact, timing.contact_at_seconds);
    event.combatant_id = Some(attacker.id);
    event.target_id = Some(defender_id);
    event.attack_id = Some(timing.attack_id(attacker.id));
    event.attack_started_tick = Some(MeleeTimelineEvent::tick_at(timing.started_at_seconds));
    event.attack_contact_tick = Some(MeleeTimelineEvent::tick_at(timing.contact_at_seconds));
    event.attack_recovery_tick = Some(MeleeTimelineEvent::tick_at(timing.recovery_until_seconds));
    event.simultaneous_batch_id = Some(batch.id);
    event.simultaneous_members = batch.members;
    event.simultaneous_order = Some(batch.order);
    event.phase_before = Some(MeleeTimelinePhase::Windup);
    event.phase_after = Some(MeleeTimelinePhase::Recovery);
    event.engagement_distance_before_metres = Some(attacker.melee_engagement_distance_metres);
    event.engagement_distance_after_metres = event.engagement_distance_before_metres;
    event.readiness_before_seconds = Some(timing.recovery_until_seconds);
    event.readiness_after_seconds = Some(timing.recovery_until_seconds);
    recorder.record_timeline(event);
}

#[expect(
    clippy::too_many_arguments,
    reason = "defense commitment joins exact scheduled and sampled response facts"
)]
fn apply_defender_response(
    defender: &mut Combatant,
    attacker_id: u64,
    engagement_distance: f32,
    scheduled_before: Option<ScheduledMeleeTiming>,
    phase: MeleeDefenderPhase,
    decision: AutoresolveMeleeDefenderDecision,
    response: DefenderResponse,
    effective_response: DefenderResponse,
    incoming: ScheduledMeleeTiming,
    parameters: crate::combat::AutoresolveParameters,
    recorder: &mut BattleRecorder,
) -> DefenseCommitment {
    let readiness_before = scheduled_before
        .map_or(defender.melee_recovery_until_seconds, |timing| {
            timing.recovery_until_seconds
        });
    let commitment = commit_defensive_action(defender, response, effective_response, phase);
    let adaptation = (commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon)
        .then(|| phase_adaptation_delay(phase, incoming, defender, parameters));
    record_response_timeline(
        recorder,
        defender,
        attacker_id,
        engagement_distance,
        readiness_before,
        phase,
        decision,
        response,
        incoming,
        commitment,
        adaptation,
    );
    if response != DefenderResponse::None {
        defender.melee_recovery_until_seconds = defender
            .melee_recovery_until_seconds
            .max(incoming.contact_at_seconds + commitment.recovery_seconds_after_contact);
    }
    if commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon {
        defender.melee_consecutive_intercepts =
            defender.melee_consecutive_intercepts.saturating_add(1);
        defender.melee_phase_adaptation_delay_seconds = adaptation.unwrap_or(0.0);
        defender.melee_attack_started_at_seconds = None;
        defender.melee_attack_contact_at_seconds = None;
        defender.melee_attack_scheduled_measure_metres = None;
    } else if decision
        .committed
        .is_some_and(|choice| choice.choice == CommittedThreatChoice::FinishTrade)
    {
        defender.melee_consecutive_intercepts =
            defender.melee_consecutive_intercepts.saturating_sub(1);
    }
    charge_defensive_work(defender, response);
    commitment
}

#[expect(
    clippy::too_many_arguments,
    reason = "timeline event records the complete response transition"
)]
fn record_response_timeline(
    recorder: &mut BattleRecorder,
    defender: &Combatant,
    attacker_id: u64,
    distance: f32,
    readiness: f32,
    phase: MeleeDefenderPhase,
    decision: AutoresolveMeleeDefenderDecision,
    response: DefenderResponse,
    incoming: ScheduledMeleeTiming,
    commitment: DefenseCommitment,
    adaptation: Option<f32>,
) {
    let affected = match phase {
        MeleeDefenderPhase::CommittedAttack(timing) => Some(timing.attack_id(defender.id)),
        _ => None,
    };
    let before = timeline_phase(phase);
    let after = timeline_phase_after_commitment(commitment.kind, phase);
    let after_readiness =
        readiness.max(incoming.contact_at_seconds + commitment.recovery_seconds_after_contact);
    let mut event =
        MeleeTimelineEvent::at(MeleeTimelineKind::Response, incoming.contact_at_seconds);
    event.combatant_id = Some(defender.id);
    event.target_id = Some(attacker_id);
    event.attack_id = Some(incoming.attack_id(attacker_id));
    event.response_choice = Some(
        if decision
            .committed
            .is_some_and(|choice| choice.choice == CommittedThreatChoice::FinishTrade)
        {
            MeleeResponseChoice::FinishTrade
        } else {
            response_choice(response)
        },
    );
    if let Some(choice) = decision.committed {
        event.committed_finish_trade_probability = Some(choice.finish_trade_probability);
        event.committed_completed_work_fraction = Some(choice.completed_work_fraction);
        event.committed_expected_intercept_benefit = Some(choice.expected_intercept_benefit);
        event.consecutive_intercepts_before = Some(defender.melee_consecutive_intercepts);
        event.consecutive_intercepts_after = Some(
            if commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon {
                defender.melee_consecutive_intercepts.saturating_add(1)
            } else if choice.choice == CommittedThreatChoice::FinishTrade {
                defender.melee_consecutive_intercepts.saturating_sub(1)
            } else {
                defender.melee_consecutive_intercepts
            },
        );
        event.phase_adaptation_delay_seconds = adaptation;
    }
    event.response_availability = Some(response_availability(defender, response, phase, incoming));
    event.phase_before = Some(before);
    event.phase_after = Some(after);
    event.affected_attack_id = affected;
    event.engagement_distance_before_metres = Some(distance);
    event.engagement_distance_after_metres = Some(distance);
    event.readiness_before_seconds = Some(readiness);
    event.readiness_after_seconds = Some(after_readiness);
    recorder.record_timeline(event);
    if matches!(
        commitment.kind,
        MeleeDefenseCommitmentKind::CanceledSameWeapon
            | MeleeDefenseCommitmentKind::TransformedOffhand
    ) {
        let kind = if commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon {
            MeleeTimelineKind::AttackCanceled
        } else {
            MeleeTimelineKind::AttackTransformed
        };
        let mut transformation = MeleeTimelineEvent::at(kind, incoming.contact_at_seconds);
        transformation.combatant_id = Some(defender.id);
        transformation.target_id = Some(attacker_id);
        transformation.attack_id = Some(incoming.attack_id(attacker_id));
        transformation.affected_attack_id = affected;
        transformation.phase_before = Some(before);
        transformation.phase_after = Some(after);
        transformation.readiness_before_seconds = Some(readiness);
        transformation.readiness_after_seconds = Some(after_readiness);
        recorder.record_timeline(transformation);
    }
}

struct PreparedMeleeExchange {
    attack_power_multiplier: f32,
    contact_at_time: MeleeContactAtTime,
    scheduled_defender_timing_before: Option<ScheduledMeleeTiming>,
    defender_phase: MeleeDefenderPhase,
    defender_decision: AutoresolveMeleeDefenderDecision,
    response: DefenderResponse,
    exchange: MeleeExchangeOutcome,
    result: AttackResult,
    part: BodyPart,
    attacker_incapacitation_performance: f32,
    attack_duration: f32,
}

fn prepare_melee_exchange(
    attacker: &mut Combatant,
    defender: &Combatant,
    flanking: f32,
    random: &mut SplitMix64,
    parameters: crate::combat::AutoresolveParameters,
    timing: ScheduledMeleeTiming,
) -> PreparedMeleeExchange {
    let power = std::mem::replace(&mut attacker.melee_attack_power_multiplier, 1.0);
    let scheduled_measure = attacker
        .melee_attack_scheduled_measure_metres
        .unwrap_or(attacker.melee_engagement_distance_metres);
    let actual_measure = attacker
        .melee_engagement_distance_metres
        .min(defender.melee_engagement_distance_metres)
        .max(0.0);
    let ideal_measure = mobility::preferred_melee_measure(attacker, parameters);
    let equipment = attacker.equipment.for_melee();
    let contact_at_time = resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
        scheduled_measure_metres: scheduled_measure,
        actual_measure_metres: actual_measure,
        ideal_measure_metres: ideal_measure,
        effective_reach_metres: melee_effective_reach(attacker),
        grip_to_tip_metres: equipment.weapon_grip_to_tip(),
        total_length_metres: equipment.weapon_total_length(),
        striking_head_length_metres: equipment.weapon_striking_head_length(),
        distal_headed: equipment.weapon.is_some_and(|weapon| weapon.distal_headed),
        attack_style: equipment.weapon_preferred_melee_style(),
        body_material: equipment.weapon_body_material(),
        striking_material: equipment.weapon_striking_material(),
    });
    let precision = autoresolve_hit_precision(random, parameters);
    let reaction_sample = random.unit_f32();
    let defender_timing = defender
        .melee_attack_started_at_seconds
        .zip(defender.melee_attack_contact_at_seconds)
        .map(
            |(started_at_seconds, contact_at_seconds)| ScheduledMeleeTiming {
                started_at_seconds,
                contact_at_seconds,
                recovery_until_seconds: defender.melee_recovery_until_seconds,
            },
        );
    let phase = defender_phase_at_contact(defender, attacker.id, timing);
    let decision = autoresolve_melee_defender_response(
        defender,
        random.unit_f32(),
        reaction_sample,
        random.unit_f32(),
        timing,
        phase,
        parameters,
    );
    let sample = random.unit_f32();
    let contact = autoresolve_melee_contact_location(attacker, defender, precision, sample);
    let response = shield_aligned_response(
        decision
            .response
            .scaled_for_performance(defender.incapacitation_performance()),
        defender.equipment.shield_holding_side(),
        contact,
    );
    let exchange = melee_exchange_at_contact(
        attacker,
        defender,
        precision,
        flanking,
        sample,
        response,
        random.unit_f32(),
        contact_at_time,
    );
    let result = exchange.result * power;
    PreparedMeleeExchange {
        attack_power_multiplier: power,
        contact_at_time,
        scheduled_defender_timing_before: defender_timing,
        defender_phase: phase,
        defender_decision: decision,
        response,
        part: exchange.contact.body_part,
        attacker_incapacitation_performance: attacker.incapacitation_performance(),
        attack_duration: attacker
            .equipment
            .melee_weapon
            .map_or(parameters.reference_melee_attack_seconds, |weapon| {
                weapon.attack_interval_seconds
            }),
        exchange,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_reach_boundary_is_attackable_but_outside_measure_closes() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let mut fighter = Combatant::new(1);
        fighter.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 1.0,
            ..CombatWeapon::default()
        });
        fighter.equipment.weapon = fighter.equipment.melee_weapon;
        let reach = melee_effective_reach(&fighter);
        assert_eq!(
            movement_intent(&fighter, reach, parameters),
            MovementIntent::Hold
        );
        assert_eq!(
            movement_intent(&fighter, reach + 0.01, parameters),
            MovementIntent::Close
        );
    }

    #[test]
    fn committed_short_weapon_tracks_while_long_weapon_seeks_measure() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let mut short = Combatant::new(1);
        short.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 1.0,
            ..CombatWeapon::default()
        });
        short.equipment.weapon = short.equipment.melee_weapon;
        short.melee_attack_started_at_seconds = Some(0.0);
        let mut long = Combatant::new(2);
        long.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 2.0,
            ..CombatWeapon::default()
        });
        long.equipment.weapon = long.equipment.melee_weapon;
        assert_eq!(
            movement_intent(&short, 1.2, parameters),
            MovementIntent::Close
        );
        assert_eq!(
            movement_intent(&long, 0.9, parameters),
            MovementIntent::Retreat
        );
    }

    #[test]
    fn distal_headed_weapon_seeks_the_center_of_its_authored_head_band() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let mut polearm = Combatant::new(2);
        polearm.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 2.0,
            grip_to_tip_m: 1.9,
            total_length_m: 2.2,
            striking_head_length_m: 0.16,
            distal_headed: true,
            ..CombatWeapon::default()
        });
        polearm.equipment.weapon = polearm.equipment.melee_weapon;
        let preferred = preferred_melee_measure(&polearm, parameters);
        let reach = melee_effective_reach(&polearm);
        let expected = preferred_melee_striking_measure(reach, 1.9, 0.16, true, 0.7);
        assert!((preferred - expected).abs() < 1.0e-6);
        assert_eq!(
            movement_intent(&polearm, 1.8, parameters),
            MovementIntent::Retreat
        );
        assert_eq!(
            movement_intent(&polearm, preferred, parameters),
            MovementIntent::Hold
        );

        let opponent = Combatant::new(1);
        assert!(
            mobility::maximum_melee_pair_surface_separation(&opponent, &polearm, parameters)
                >= reach - 1.0e-5
        );
    }

    #[test]
    fn movement_and_recovery_gate_one_exact_attack_start() {
        let whole = available_attack_start(0.0, 1.0, 0.3, 0.4);
        let first_half = available_attack_start(0.0, 0.5, 0.3, 0.4);
        assert_eq!(whole, Some(0.4));
        assert_eq!(first_half, whole);
    }

    #[test]
    fn attack_start_waits_for_later_of_measure_and_recovery() {
        assert_eq!(available_attack_start(0.0, 0.5, 0.6, 0.4), None);
        assert_eq!(available_attack_start(0.5, 1.0, 0.6, 0.8), Some(0.8));
    }
}
