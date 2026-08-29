#[test]
fn lifecycle_queue_has_explicit_cross_kind_order() {
    let mut events = [
        DueLifecycleEvent::Birth {
            effective_minute: 20,
            id: "birth-b".into(),
            mother_id: 2,
        },
        DueLifecycleEvent::Wedding {
            effective_minute: 20,
            id: "wedding-z".into(),
            participant_id: 3,
        },
        DueLifecycleEvent::Wedding {
            effective_minute: 10,
            id: "wedding-a".into(),
            participant_id: 1,
        },
    ];
    events.sort_by(|left, right| left.stable_key().cmp(&right.stable_key()));
    assert!(matches!(
        events[0],
        DueLifecycleEvent::Wedding {
            effective_minute: 10,
            ..
        }
    ));
    assert!(matches!(events[1], DueLifecycleEvent::Wedding { .. }));
    assert!(matches!(events[2], DueLifecycleEvent::Birth { .. }));
}

#[test]
fn global_lifecycle_selection_is_stable_non_starving_and_poison_tolerant() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let queue = source
        .split("pub fn settle_due_lifecycle_events_global")
        .nth(1)
        .unwrap()
        .split("fn socializing_id")
        .next()
        .unwrap();
    assert!(queue.contains(".effective_minute()"));
    assert!(queue.contains(".due_minute()"));
    assert!(queue.contains("due.sort_by"));
    assert!(queue.contains("due.retain(|event| event.processable(ctx))"));
    assert!(queue.contains("due.truncate(limit)"));
    assert!(queue.contains("record_lifecycle_failure"));
    assert!(queue.contains("quarantine_invalid_birth"));
    assert!(queue.contains("validate_due_birth"));
    assert!(queue.contains("settle_due_births(ctx, mother_id, effective_minute)?"));
}

#[test]
fn birth_and_discovery_wait_for_authoritative_personal_frontiers() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let birth = source
        .split("pub fn settle_due_births")
        .nth(1)
        .unwrap()
        .split("fn socializing_id")
        .next()
        .unwrap();
    assert!(birth.contains("mother_frontier < pregnancy.due_minute"));
    assert!(!birth.contains("advance_npc_personal_time"));
    let discovery = source
        .split("pub fn settle_secret_courtship_discovery_for_pair")
        .nth(1)
        .unwrap()
        .split("fn personality_disposition")
        .next()
        .unwrap();
    assert!(discovery.contains("first_frontier / MINUTES_PER_DAY < day"));
    assert!(discovery.contains("canonical_now(ctx, baseline.observer_id)?"));
    assert!(discovery.contains("courtship_observer_baseline()"));
    assert!(!discovery.contains("no-observation"));
}

#[test]
fn death_releases_future_relationship_and_pregnancy_state() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let cleanup = source
        .split("pub(crate) fn settle_relationship_lifecycle_for_death")
        .nth(1)
        .unwrap()
        .split("/// Reserve two people")
        .next()
        .unwrap();
    assert!(cleanup.contains("CommitmentTerminalReason::ParticipantDead"));
    assert!(cleanup.contains("CourtshipTerminalReason::PartnerUnavailable"));
    assert!(cleanup.contains("pregnancy.status = PregnancyStatus::Ended"));
    assert!(cleanup.contains("active_pregnancy().mother_id().delete"));
    assert!(cleanup.contains("child_identity_reservation()"));

    let death = include_str!("../../character.rs")
        .split("pub fn transition_character_to_dead_at")
        .nth(1)
        .unwrap()
        .split("/// [`Character`] attributes")
        .next()
        .unwrap();
    assert!(death.contains("settle_relationship_lifecycle_for_death"));
}

#[test]
fn dead_fiance_is_processable_without_reaching_the_ceremony() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let wedding = source
        .split("pub fn settle_due_weddings")
        .nth(1)
        .unwrap()
        .split("pub fn settle_due_weddings_global")
        .next()
        .unwrap();
    assert!(wedding.contains("participant_death_minute"));
    assert!(wedding.contains("CommitmentTerminalReason::ParticipantDead"));

    let queue = source
        .split("fn processable(&self")
        .nth(1)
        .unwrap()
        .split("fn record_lifecycle_failure")
        .next()
        .unwrap();
    assert!(queue.contains("participant_died_before_ceremony"));
    assert!(queue.contains("death.strategic_minute <= *effective_minute"));
}
