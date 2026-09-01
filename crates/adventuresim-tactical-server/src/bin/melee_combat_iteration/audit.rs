use super::*;

pub(super) fn build_acceptance_audit(
    summaries: &[MatchupSummary],
    evidence: &adventuresim_core::autoresolve::MeleeIterationAcceptanceEvidence,
) -> AcceptanceAudit {
    AcceptanceAudit {
        no_tactical_timeouts: summaries
            .iter()
            .all(|summary| summary.tactical_timeouts == 0),
        no_autoresolve_timeouts: summaries
            .iter()
            .all(|summary| summary.autoresolve_timeouts == 0),
        tactical_energy_conservation_holds: summaries
            .iter()
            .all(|summary| summary.tactical_causal.energy_partition_failures == 0),
        side_swap_nonterminal_timeline_equal: evidence
            .autoresolve_timeline
            .normalized_nonterminal_sequences_equal,
        simultaneous_contacts_preserved: evidence.autoresolve_timeline.simultaneous_contacts.len()
            == 2,
        canceled_attacks_emit_no_ghost_contacts: evidence
            .autoresolve_timeline
            .canceled_attack_ids_that_contacted
            .is_empty(),
        autoresolve_movement_elapsed_matches_distance_delta: movement_elapsed_matches_distance(
            summaries,
        ),
        autoresolve_movement_respects_tick_and_speed_limits: movement_respects_limits(summaries),
        polearm_contact_revalidation_holds: polearm_contact_revalidation_holds(evidence),
        all_weapon_swept_contact_contract_holds: swept_contact_contract_holds(summaries, evidence),
        matchups: matchup_audits(summaries),
    }
}

fn movement_elapsed_matches_distance(summaries: &[MatchupSummary]) -> bool {
    summaries.iter().all(|summary| {
        summary
            .autoresolve_causal
            .john
            .movement_nonzero_delta_zero_elapsed
            == 0
            && summary
                .autoresolve_causal
                .opponent
                .movement_nonzero_delta_zero_elapsed
                == 0
    })
}

fn movement_respects_limits(summaries: &[MatchupSummary]) -> bool {
    let tick_limit = f64::from(1.0_f32 / 64.0 + 1.0e-6);
    summaries.iter().all(|summary| {
        summary
            .autoresolve_causal
            .john
            .movement_displacement_limit_failures
            == 0
            && summary
                .autoresolve_causal
                .opponent
                .movement_displacement_limit_failures
                == 0
            && summary
                .autoresolve_causal
                .john
                .maximum_movement_segment_seconds
                <= tick_limit
            && summary
                .autoresolve_causal
                .opponent
                .maximum_movement_segment_seconds
                <= tick_limit
    })
}

fn swept_contact_contract_holds(
    summaries: &[MatchupSummary],
    evidence: &adventuresim_core::autoresolve::MeleeIterationAcceptanceEvidence,
) -> bool {
    let minimum = adventuresim_core::combat::HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES;
    evidence.all_weapon_contact_bands.iter().all(|contact| {
        contact.center_separation_metres >= minimum
            && contact.transformed_energy_joules <= contact.incident_energy_joules + 1.0e-4
    }) && evidence.all_weapon_contact_bands.iter().any(|contact| {
        contact.weapon == "war_hammer"
            && contact.classification == MeleeContactClassification::Pommel
    }) && summaries.iter().all(summary_contact_contract_holds)
}

fn summary_contact_contract_holds(summary: &MatchupSummary) -> bool {
    let minimum =
        f64::from(adventuresim_core::combat::HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES);
    let john = &summary.autoresolve_causal.john;
    let opponent = &summary.autoresolve_causal.opponent;
    john.full_energy_intended_contacts_inside_ten_centimetres == 0
        && opponent.full_energy_intended_contacts_inside_ten_centimetres == 0
        && (john.contact_measure_samples == 0
            || john.minimum_actual_center_separation_metres >= minimum)
        && (opponent.contact_measure_samples == 0
            || opponent.minimum_actual_center_separation_metres >= minimum)
}

pub(super) fn polearm_contact_revalidation_holds(
    evidence: &adventuresim_core::autoresolve::MeleeIterationAcceptanceEvidence,
) -> bool {
    evidence.polearm_contact_revalidation.len() == 6
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::IntendedSurface
                && contact.contact_material == Some(EquipmentMaterial::RoughSteel)
                && contact.transformed_energy_joules == contact.incident_energy_joules
                && contact.invalidation_cause.is_none()
        })
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::Haft
                && contact.contact_material == Some(EquipmentMaterial::Hardwood)
                && contact.transformed_energy_joules < contact.incident_energy_joules
                && !contact.edge_contact
                && contact.invalidation_cause.is_none()
        })
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::Pommel
                && contact.contact_material == Some(EquipmentMaterial::Hardwood)
                && contact.transformed_energy_joules < contact.incident_energy_joules
                && !contact.edge_contact
                && contact.invalidation_cause.is_none()
        })
        && evidence.polearm_contact_revalidation.iter().any(|contact| {
            contact.classification == MeleeContactClassification::InvalidatedMiss
                && contact.transformed_energy_joules == 0.0
                && contact.invalidation_cause == Some(MeleeContactInvalidationCause::OutsideReach)
        })
}

pub(super) fn matchup_audits(summaries: &[MatchupSummary]) -> Vec<MatchupCausalAudit> {
    summaries
        .iter()
        .map(|summary| {
            let john_responses = summary
                .autoresolve_causal
                .john
                .response_availability
                .values()
                .sum();
            let opponent_responses = summary
                .autoresolve_causal
                .opponent
                .response_availability
                .values()
                .sum();
            MatchupCausalAudit {
                opponent: summary.opponent.clone(),
                tactical_first_contacts: [
                    summary.tactical_causal.john_first_contacts,
                    summary.tactical_causal.opponent_first_contacts,
                ],
                autoresolve_first_contacts: [
                    summary.autoresolve_causal.john_first_contacts,
                    summary.autoresolve_causal.opponent_first_contacts,
                ],
                autoresolve_attack_starts: [
                    summary.autoresolve_causal.john.attack_starts,
                    summary.autoresolve_causal.opponent.attack_starts,
                ],
                autoresolve_resolved_attacks: [
                    summary.autoresolve_causal.john_attacks,
                    summary.autoresolve_causal.opponent_attacks,
                ],
                autoresolve_cancellations: [
                    summary.autoresolve_causal.john.committed_attacks_canceled,
                    summary
                        .autoresolve_causal
                        .opponent
                        .committed_attacks_canceled,
                ],
                autoresolve_response_events: [john_responses, opponent_responses],
                response_event_for_every_incoming_contact: john_responses
                    == summary.autoresolve_causal.opponent_attacks
                    && opponent_responses == summary.autoresolve_causal.john_attacks,
            }
        })
        .collect()
}
