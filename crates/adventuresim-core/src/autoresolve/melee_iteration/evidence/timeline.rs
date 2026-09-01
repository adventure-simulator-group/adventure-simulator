use super::*;

pub(super) fn autoresolve_timeline_evidence(
    john: &MeleeIterationBuild,
    veteran: &MeleeIterationBuild,
    hammer: &MeleeIterationBuild,
) -> Result<AutoresolveTimelineEvidence, String> {
    const SEED: u64 = 1;
    let forward = resolve_battle(
        vec![john.combatant.clone()],
        vec![hammer.combatant.clone()],
        SEED,
        BattleOpening::Normal,
    );
    let reversed = resolve_battle(
        vec![hammer.combatant.clone()],
        vec![john.combatant.clone()],
        SEED,
        BattleOpening::Normal,
    );
    let without_terminal = |outcome: &BattleOutcome| {
        outcome
            .timeline
            .iter()
            .filter(|event| event.kind != MeleeTimelineKind::Terminal)
            .cloned()
            .collect::<Vec<_>>()
    };
    let side_swap_forward = without_terminal(&forward);
    let side_swap_reversed = without_terminal(&reversed);
    let polearm = resolve_battle(
        vec![john.combatant.clone()],
        vec![veteran.combatant.clone()],
        SEED,
        BattleOpening::Normal,
    );
    let first_contact = polearm
        .timeline
        .iter()
        .position(|event| event.kind == MeleeTimelineKind::Contact)
        .map_or(polearm.timeline.len(), |index| index + 1);
    let polearm_opening_measure = polearm.timeline[..first_contact].to_vec();
    let simultaneous_contacts = simultaneous_contact_evidence(john, hammer, SEED);
    let cancellation = (SEED..=64)
        .map(|seed| {
            resolve_battle(
                vec![john.combatant.clone()],
                vec![hammer.combatant.clone()],
                seed,
                BattleOpening::Normal,
            )
        })
        .find(|outcome| {
            outcome
                .timeline
                .iter()
                .any(|event| event.kind == MeleeTimelineKind::AttackCanceled)
        })
        .ok_or("bounded hammer seeds produced no cancellation evidence")?;
    let canceled_attack_ids = cancellation
        .timeline
        .iter()
        .filter(|event| event.kind == MeleeTimelineKind::AttackCanceled)
        .filter_map(|event| event.affected_attack_id)
        .collect::<Vec<_>>();
    let contacted = cancellation
        .timeline
        .iter()
        .filter(|event| event.kind == MeleeTimelineKind::Contact)
        .filter_map(|event| event.attack_id)
        .collect::<Vec<_>>();
    let canceled_attack_ids_that_contacted = canceled_attack_ids
        .iter()
        .filter(|id| contacted.contains(id))
        .copied()
        .collect();
    let cancellation_sequence = cancellation
        .timeline
        .into_iter()
        .filter(|event| {
            matches!(
                event.kind,
                MeleeTimelineKind::AttackStarted
                    | MeleeTimelineKind::Response
                    | MeleeTimelineKind::AttackCanceled
                    | MeleeTimelineKind::AttackTransformed
                    | MeleeTimelineKind::Contact
                    | MeleeTimelineKind::Terminal
            )
        })
        .collect();
    Ok(AutoresolveTimelineEvidence {
        normalized_nonterminal_sequences_equal: side_swap_forward == side_swap_reversed,
        side_swap_forward,
        side_swap_reversed,
        polearm_opening_measure,
        simultaneous_contacts,
        cancellation_sequence,
        canceled_attack_ids,
        canceled_attack_ids_that_contacted,
    })
}

fn simultaneous_contact_evidence(
    john: &MeleeIterationBuild,
    hammer: &MeleeIterationBuild,
    seed: u64,
) -> Vec<MeleeTimelineEvent> {
    let mut allies = vec![john.combatant.clone()];
    let mut enemies = vec![hammer.combatant.clone()];
    allies[0].melee_engagement_target = Some(enemies[0].id);
    enemies[0].melee_engagement_target = Some(allies[0].id);
    allies[0].melee_engagement_distance_metres = 0.4;
    enemies[0].melee_engagement_distance_metres = 0.4;
    let contact = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS.melee_windup_seconds;
    for combatant in [&mut allies[0], &mut enemies[0]] {
        combatant.melee_attack_started_at_seconds = Some(0.0);
        combatant.melee_attack_contact_at_seconds = Some(contact);
        combatant.melee_recovery_until_seconds = contact + 0.5;
    }
    let mut recorder = BattleRecorder::default();
    resolve_joint_melee_round(
        &mut allies,
        &mut enemies,
        1,
        &mut SplitMix64::new(seed),
        &mut recorder,
        crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS,
    );
    recorder
        .timeline
        .into_iter()
        .filter(|event| event.kind == MeleeTimelineKind::Contact)
        .collect()
}
