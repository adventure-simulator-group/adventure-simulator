#[test]
fn wedding_contract_uses_effective_history_and_records_one_dowry_outcome() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let wedding = source
        .split("pub fn settle_due_weddings")
        .nth(1)
        .unwrap()
        .split("pub fn establish_pregnancy")
        .next()
        .unwrap();
    assert!(wedding.contains("ParticipantUnderage"));
    assert!(wedding.contains("character_alive_at"));
    assert!(wedding.contains("holding.acquired_minute <= effective_minute"));
    assert!(wedding.contains("resolved > effective_minute"));
    assert!(wedding.contains("move_residence_occupant_effective"));
    assert!(wedding.contains("dowry_escrow()"));
    assert!(wedding.contains("dowry_outcome()"));
    assert!(wedding.contains("commitment_id()"));
    assert!(wedding.contains("MarriageParticipant"));
}

#[test]
fn wedding_resolution_uses_the_scheduled_effective_minute() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let wedding = source
        .split("pub fn settle_due_weddings")
        .nth(1)
        .unwrap()
        .split("pub fn settle_due_weddings_global")
        .next()
        .unwrap();
    assert!(wedding.contains("let effective_minute = commitment.effective_minute"));
    assert!(!wedding.contains("WeddingCompleted,\n            now"));
}

#[test]
fn marriage_cleanup_releases_household_and_guest_occupancy() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let resolution = source
        .split("fn resolve_marriage")
        .nth(1)
        .unwrap()
        .split("#[reducer]\npub fn end_marriage")
        .next()
        .unwrap();
    assert!(resolution.contains("leave_household"));
    assert!(resolution.contains("member.joined_minute <= minute"));
    assert!(resolution.contains("remove_nonowned_occupancy_effective"));
    assert!(source.contains("#[unique]\n    pub character_id: u64"));
}

#[test]
fn effective_history_remains_authoritative_after_marker_cleanup() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let conflicts = source
        .split("fn relationship_conflicts_at")
        .nth(1)
        .unwrap()
        .split("fn formal_dowry_amount")
        .next()
        .unwrap();
    assert!(conflicts.contains("courtship().iter()"));
    assert!(conflicts.contains("exclusive_commitment().iter()"));
    assert!(conflicts.contains("marriage().iter()"));
    assert!(conflicts.matches("resolved > minute").count() >= 3);
}

#[test]
fn dowry_is_escrowed_when_the_wedding_is_reserved_and_refunded_on_failure() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let reservation = source
        .split("pub(crate) fn reserve_wedding")
        .nth(1)
        .unwrap()
        .split("fn kinship_id")
        .next()
        .unwrap();
    assert!(reservation.contains("consume_personal_currency"));
    assert!(reservation.contains("dowry_escrow().insert"));
    assert!(reservation.contains("reserved_minute: scheduled_from_minute"));

    let terminal = source
        .split("fn transition_commitment_terminal")
        .nth(1)
        .unwrap()
        .split("/// Reserve two people")
        .next()
        .unwrap();
    assert!(terminal.contains("status != CommitmentStatus::Fulfilled"));
    assert!(terminal.contains("credit_personal_currency"));
    assert!(terminal.contains("dowry_escrow()"));
}

#[test]
fn cancellation_only_applies_to_future_reserved_weddings() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let cancel = source
        .split("pub fn cancel_wedding")
        .nth(1)
        .unwrap()
        .split("pub fn expire_wedding_reservation")
        .next()
        .unwrap();
    assert!(cancel.contains("commitment.status != CommitmentStatus::Reserved"));
    assert!(cancel.contains("minute >= commitment.effective_minute"));
}
