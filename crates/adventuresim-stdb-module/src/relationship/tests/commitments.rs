#[test]
fn pair_id_is_order_independent() {
    assert_eq!(commitment_id(1, 9), commitment_id(9, 1));
}

#[test]
fn engagement_is_one_year_notice() {
    assert_eq!(
        WEDDING_NOTICE_MINUTES,
        adventuresim_core::strategic_time::MINUTES_PER_YEAR
    );
}

#[test]
fn every_commitment_terminal_transition_releases_reservations_and_audits() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let transition = source
        .split("fn transition_commitment_terminal")
        .nth(1)
        .unwrap()
        .split("/// Reserve two people")
        .next()
        .unwrap();
    assert!(transition.contains("exclusive_commitment_participant()"));
    assert!(transition.contains(".delete(character_id)"));
    assert!(transition.contains("record_commitment_event"));
    for status in ["Cancelled", "Expired", "Ended"] {
        assert!(source.contains(&format!("CommitmentStatus::{status}")));
    }
}
