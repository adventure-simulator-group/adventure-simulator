use super::*;

pub(super) fn record_autoresolve_telemetry(
    attacker: &mut AutoresolveCombatantCausal,
    defender: &mut AutoresolveCombatantCausal,
    telemetry: &adventuresim_core::autoresolve::MeleeContactTelemetry,
) {
    *attacker
        .contact_classifications
        .entry(format!("{:?}", telemetry.contact_classification))
        .or_default() += 1;
    record_contact_measures(attacker, telemetry);
    attacker.full_energy_intended_contacts_inside_ten_centimetres += u64::from(
        telemetry.actual_contact_measure_metres < 0.1
            && telemetry.contact_classification == MeleeContactClassification::IntendedSurface
            && telemetry.contact_energy_fraction >= 0.999,
    );
    *defender
        .anatomical_subregions_received
        .entry(format!("{:?}", telemetry.anatomical_subregion))
        .or_default() += 1;
    for layer in &telemetry.armor_layer_chain {
        let key = format!("{:?}:{:?}", layer.inventory_item_id, layer.material);
        let distribution = if layer.intersected {
            &mut defender.armor_layers_intersected
        } else {
            &mut defender.armor_layers_missed
        };
        *distribution.entry(key).or_default() += 1;
    }
    attacker.attack_samples += 1;
    attacker.mean_attack_interval_seconds += f64::from(telemetry.attack_interval_seconds);
    attacker.mean_attack_power_multiplier += f64::from(telemetry.attack_power_multiplier);
    let performance = f64::from(telemetry.attacker_fatigue_performance);
    if attacker.attack_samples == 1 {
        attacker.minimum_attack_fatigue_performance = performance;
    } else {
        attacker.minimum_attack_fatigue_performance =
            attacker.minimum_attack_fatigue_performance.min(performance);
    }
}

pub(super) fn record_timeline_event(
    combatant: &mut AutoresolveCombatantCausal,
    event: &adventuresim_core::autoresolve::MeleeTimelineEvent,
) {
    match event.kind {
        MeleeTimelineKind::Movement => record_movement_event(combatant, event),
        MeleeTimelineKind::AttackStarted => combatant.attack_starts += 1,
        MeleeTimelineKind::Response => record_response_event(combatant, event),
        MeleeTimelineKind::AttackCanceled => combatant.committed_attacks_canceled += 1,
        MeleeTimelineKind::AttackTransformed => combatant.committed_attacks_transformed += 1,
        MeleeTimelineKind::Contact if event.simultaneous_members.len() > 1 => {
            combatant.simultaneous_contacts += 1;
        }
        MeleeTimelineKind::Contact | MeleeTimelineKind::Terminal => {}
    }
}

fn record_movement_event(
    combatant: &mut AutoresolveCombatantCausal,
    event: &adventuresim_core::autoresolve::MeleeTimelineEvent,
) {
    if let Some(action) = event.movement_action {
        *combatant
            .movement_actions
            .entry(format!("{action:?}"))
            .or_default() += 1;
    }
    let elapsed = event.movement_elapsed_seconds.unwrap_or_default();
    let displacement = event.movement_displacement_metres.unwrap_or_default().abs();
    let velocity = event
        .movement_velocity_before_metres_per_second
        .unwrap_or_default()
        .abs()
        .max(
            event
                .movement_velocity_after_metres_per_second
                .unwrap_or_default()
                .abs(),
        );
    combatant.movement_elapsed_seconds += f64::from(elapsed);
    combatant.movement_segments += 1;
    combatant.movement_absolute_displacement_metres += f64::from(displacement);
    combatant.maximum_movement_speed_metres_per_second = combatant
        .maximum_movement_speed_metres_per_second
        .max(f64::from(velocity));
    combatant.maximum_movement_segment_seconds = combatant
        .maximum_movement_segment_seconds
        .max(f64::from(elapsed));
    combatant.movement_displacement_limit_failures +=
        u64::from(displacement > velocity * elapsed + 1.0e-5);
    let distance_delta = event
        .engagement_distance_before_metres
        .zip(event.engagement_distance_after_metres)
        .map_or(0.0, |(before, after)| (after - before).abs());
    combatant.movement_nonzero_delta_zero_elapsed +=
        u64::from(distance_delta > f32::EPSILON && elapsed <= 0.0);
}

fn record_response_event(
    combatant: &mut AutoresolveCombatantCausal,
    event: &adventuresim_core::autoresolve::MeleeTimelineEvent,
) {
    if let Some(availability) = event.response_availability {
        *combatant
            .response_availability
            .entry(format!("{availability:?}"))
            .or_default() += 1;
    }
    if let Some(choice) = event.response_choice {
        *combatant
            .response_choices
            .entry(format!("{choice:?}"))
            .or_default() += 1;
    }
    if let Some(delay) = event.phase_adaptation_delay_seconds
        && delay > 0.0
    {
        combatant.phase_adaptation_events += 1;
        combatant.phase_adaptation_delay_seconds += f64::from(delay);
    }
}

fn record_contact_measures(
    attacker: &mut AutoresolveCombatantCausal,
    telemetry: &adventuresim_core::autoresolve::MeleeContactTelemetry,
) {
    attacker.contact_measure_samples += 1;
    attacker.mean_scheduled_contact_measure_metres +=
        f64::from(telemetry.scheduled_contact_measure_metres);
    attacker.mean_actual_contact_measure_metres +=
        f64::from(telemetry.actual_contact_measure_metres);
    if attacker.contact_measure_samples == 1 {
        attacker.minimum_actual_contact_measure_metres =
            f64::from(telemetry.actual_contact_measure_metres);
        attacker.maximum_actual_contact_measure_metres =
            f64::from(telemetry.actual_contact_measure_metres);
        attacker.minimum_actual_center_separation_metres =
            f64::from(telemetry.actual_center_separation_metres);
    } else {
        attacker.minimum_actual_contact_measure_metres = attacker
            .minimum_actual_contact_measure_metres
            .min(f64::from(telemetry.actual_contact_measure_metres));
        attacker.maximum_actual_contact_measure_metres = attacker
            .maximum_actual_contact_measure_metres
            .max(f64::from(telemetry.actual_contact_measure_metres));
        attacker.minimum_actual_center_separation_metres = attacker
            .minimum_actual_center_separation_metres
            .min(f64::from(telemetry.actual_center_separation_metres));
    }
}

pub(super) fn record_tactical_first_contact(
    summary: &mut TacticalCausalSummary,
    outcome: &TacticalMeleeOutcome,
    john_name: &str,
) {
    if let Some(first_contact) = outcome.events.first() {
        if first_contact.attacker == john_name {
            summary.john_first_contacts += 1;
        } else {
            summary.opponent_first_contacts += 1;
        }
    }
}

pub(super) fn record_tactical_decisions(
    summary: &mut TacticalCausalSummary,
    outcome: &TacticalMeleeOutcome,
    john_name: &str,
) {
    let mut previous_starts = [None, None];
    for decision in &outcome.decision_events {
        let john = decision.combatant == john_name;
        record_summary_decision(summary, decision.decision, decision.status, john);
        let combatant = if john {
            &mut summary.john
        } else {
            &mut summary.opponent
        };
        let previous = &mut previous_starts[usize::from(!john)];
        record_combatant_decision(combatant, decision, previous);
    }
}

fn record_summary_decision(
    summary: &mut TacticalCausalSummary,
    decision: TacticalDecision,
    status: TacticalDecisionStatus,
    john: bool,
) {
    let pair = if john {
        (
            &mut summary.john_attack_starts,
            &mut summary.john_canceled_for_defense,
            &mut summary.john_transformed_by_defense,
        )
    } else {
        (
            &mut summary.opponent_attack_starts,
            &mut summary.opponent_canceled_for_defense,
            &mut summary.opponent_transformed_by_defense,
        )
    };
    match (decision, status) {
        (TacticalDecision::Attack, TacticalDecisionStatus::Started) => *pair.0 += 1,
        (TacticalDecision::Attack, TacticalDecisionStatus::CanceledForDefense) => *pair.1 += 1,
        (TacticalDecision::Attack, TacticalDecisionStatus::TransformedByDefense) => *pair.2 += 1,
        _ => {}
    }
}

fn record_combatant_decision(
    combatant: &mut TacticalCombatantCausal,
    decision: &adventuresim_tactical_server::iteration::TacticalDecisionLogEntry,
    previous: &mut Option<f32>,
) {
    match (decision.decision, decision.status) {
        (TacticalDecision::Attack, TacticalDecisionStatus::Started) => {
            combatant.attack_starts += 1;
            if let Some(previous_seconds) = previous.replace(decision.elapsed_seconds) {
                combatant.attack_interval_samples += 1;
                combatant.mean_attack_start_interval_seconds +=
                    f64::from(decision.elapsed_seconds - previous_seconds);
            }
        }
        (TacticalDecision::Attack, TacticalDecisionStatus::CanceledForDefense) => {
            combatant.committed_attacks_canceled += 1;
        }
        (TacticalDecision::Attack, TacticalDecisionStatus::TransformedByDefense) => {
            combatant.committed_attacks_transformed += 1;
        }
        _ => {}
    }
}

pub(super) fn record_tactical_wounds(
    summary: &mut TacticalCausalSummary,
    outcome: &TacticalMeleeOutcome,
    john_name: &str,
) {
    for wound in &outcome.wound_events {
        let combatant = if wound.combatant == john_name {
            &mut summary.john
        } else {
            &mut summary.opponent
        };
        match wound.kind {
            adventuresim_core::combat::CombatWoundKind::Open => {
                summary.open_wounds += 1;
                combatant.open_wounds_received += 1;
            }
            adventuresim_core::combat::CombatWoundKind::Internal => {
                summary.internal_wounds += 1;
                combatant.internal_wounds_received += 1;
            }
        }
    }
}

pub(super) fn record_tactical_event(
    summary: &mut TacticalCausalSummary,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
    john_name: &str,
) {
    let attacker_is_john = event.attacker == john_name;
    record_tactical_attacker(summary, event, attacker_is_john);
    record_tactical_defender(summary, event, attacker_is_john);
    record_tactical_totals(summary, event);
}

fn record_tactical_attacker(
    summary: &mut TacticalCausalSummary,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
    john: bool,
) {
    let attacker = if john {
        summary.john_resolved_attacks += 1;
        &mut summary.john
    } else {
        summary.opponent_resolved_attacks += 1;
        &mut summary.opponent
    };
    attacker.resolved_attacks += 1;
    attacker.contacts_dealt += u64::from(event.contact_energy_joules > 0.0);
    attacker.contact_energy_samples += 1;
    attacker.mean_contact_energy_joules += f64::from(event.contact_energy_joules);
    attacker.maximum_oxygen_debt_joules = attacker
        .maximum_oxygen_debt_joules
        .max(event.attacker_incapacitation.oxygen_debt_joules);
    attacker.maximum_local_action_fatigue = attacker
        .maximum_local_action_fatigue
        .max(event.attacker_incapacitation.local_action_fatigue);
}

fn record_tactical_defender(
    summary: &mut TacticalCausalSummary,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
    attacker_is_john: bool,
) {
    let defender = if attacker_is_john {
        &mut summary.opponent
    } else {
        &mut summary.john
    };
    defender.attacks_received += 1;
    defender.maximum_oxygen_debt_joules = defender
        .maximum_oxygen_debt_joules
        .max(event.defender_incapacitation.oxygen_debt_joules);
    defender.maximum_local_action_fatigue = defender
        .maximum_local_action_fatigue
        .max(event.defender_incapacitation.local_action_fatigue);
    *defender
        .anatomical_subregions_received
        .entry(event.anatomical_subregion.clone())
        .or_default() += 1;
    match event.defender_decision {
        TacticalDecision::Block => defender.block_attempts += 1,
        TacticalDecision::Parry => defender.parry_attempts += 1,
        TacticalDecision::Dodge => defender.dodge_attempts += 1,
        _ => {}
    }
    if event.defender_decision == TacticalDecision::Dodge {
        defender.dodge_avoids += u64::from(event.outcome == TacticalContactOutcome::Avoided);
        defender.dodge_contacts += u64::from(event.outcome != TacticalContactOutcome::Avoided);
    }
    record_defensive_implement(defender, event);
    record_defender_armor(defender, event);
}

fn record_defensive_implement(
    defender: &mut TacticalCombatantCausal,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
) {
    if event.defensive_implement_kind == Some(TacticalDefenseImplement::Shield) {
        defender.shield_block_attempts += 1;
        defender.effective_shield_blocks +=
            u64::from(event.outcome == TacticalContactOutcome::Defended);
    } else if matches!(
        event.defender_decision,
        TacticalDecision::Block | TacticalDecision::Parry
    ) && event.outcome == TacticalContactOutcome::Defended
    {
        defender.effective_weapon_contacts += 1;
    }
}

fn record_defender_armor(
    defender: &mut TacticalCombatantCausal,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
) {
    if event.coverage_contact == TacticalCoverageContact::ArmorSurface {
        defender.armor_surface_contacts_received += 1;
        increment_armor_outcome(
            event.armor_outcome,
            &mut defender.armor_stopped,
            &mut defender.armor_deflected,
            &mut defender.armor_penetrated,
        );
    } else if event.coverage_contact == TacticalCoverageContact::Gap {
        defender.armor_gap_contacts_received += 1;
    }
    for layer in &event.armor_layer_chain {
        let key = format!("{}:{:?}", layer.item_id, layer.material);
        let distribution = if layer.intersected {
            &mut defender.armor_layers_intersected
        } else {
            &mut defender.armor_layers_missed
        };
        *distribution.entry(key).or_default() += 1;
    }
}

fn record_tactical_totals(
    summary: &mut TacticalCausalSummary,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
) {
    summary.minimum_contact_separation_metres = summary
        .minimum_contact_separation_metres
        .min(event.center_separation_metres);
    summary.maximum_contact_separation_metres = summary
        .maximum_contact_separation_metres
        .max(event.center_separation_metres);
    summary.maximum_oxygen_debt_joules = summary
        .maximum_oxygen_debt_joules
        .max(event.attacker_incapacitation.oxygen_debt_joules)
        .max(event.defender_incapacitation.oxygen_debt_joules);
    summary.maximum_local_action_fatigue = summary
        .maximum_local_action_fatigue
        .max(event.attacker_incapacitation.local_action_fatigue)
        .max(event.defender_incapacitation.local_action_fatigue);
    record_dodge_totals(summary, event);
    record_defense_totals(summary, event);
    record_armor_totals(summary, event);
}

fn record_dodge_totals(
    summary: &mut TacticalCausalSummary,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
) {
    if event.defender_decision != TacticalDecision::Dodge {
        return;
    }
    summary.dodge_attempts += 1;
    summary.dodge_avoids += u64::from(event.outcome == TacticalContactOutcome::Avoided);
    summary.dodge_contacts += u64::from(event.outcome != TacticalContactOutcome::Avoided);
}

fn record_defense_totals(
    summary: &mut TacticalCausalSummary,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
) {
    if event.defensive_implement_kind == Some(TacticalDefenseImplement::Shield) {
        summary.shield_block_attempts += 1;
        summary.shield_blocks_defended +=
            u64::from(event.outcome == TacticalContactOutcome::Defended);
        summary.shield_blocks_failed +=
            u64::from(event.outcome != TacticalContactOutcome::Defended);
    } else {
        summary.weapon_defense_contacts +=
            u64::from(event.outcome == TacticalContactOutcome::Defended);
    }
}

fn record_armor_totals(
    summary: &mut TacticalCausalSummary,
    event: &adventuresim_tactical_server::iteration::TacticalMeleeLogEntry,
) {
    if event.coverage_contact != TacticalCoverageContact::ArmorSurface {
        return;
    }
    summary.armor_contacts += 1;
    increment_armor_outcome(
        event.armor_outcome,
        &mut summary.armor_stopped,
        &mut summary.armor_deflected,
        &mut summary.armor_penetrated,
    );
    let partition = event.resisted_energy_joules
        + event.transmitted_energy_joules
        + event.penetrated_energy_joules;
    summary.energy_partition_failures +=
        u64::from((partition - event.contact_energy_joules).abs() > 0.01);
}

fn increment_armor_outcome(
    outcome: Option<adventuresim_core::combat::ArmorImpactOutcome>,
    stopped: &mut u64,
    deflected: &mut u64,
    penetrated: &mut u64,
) {
    match outcome {
        Some(adventuresim_core::combat::ArmorImpactOutcome::Stopped) => *stopped += 1,
        Some(adventuresim_core::combat::ArmorImpactOutcome::Deflected) => *deflected += 1,
        Some(adventuresim_core::combat::ArmorImpactOutcome::Penetrated) => *penetrated += 1,
        None => {}
    }
}
