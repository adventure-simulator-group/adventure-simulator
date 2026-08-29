#[test]
fn courtship_thresholds_use_opinion_at_the_effective_minute() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let projection = source
        .split("fn affinity_at")
        .nth(1)
        .unwrap()
        .split("fn active_romantic_partners")
        .next()
        .unwrap();
    assert!(projection.contains("row.anchor_minute <= minute"));
    assert!(projection.contains("settle_affinity"));
    assert!(source.matches("affinity_at(ctx, father, suitor_id").count() >= 3);
    assert!(
        source
            .matches("affinity_at(ctx, partner_id, suitor_id")
            .count()
            >= 3
    );
}

#[test]
fn formal_route_uses_living_father_and_retry_is_explicit() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let formal = source
        .split("pub fn begin_formal_courtship")
        .nth(1)
        .unwrap()
        .split("#[reducer]\npub fn begin_informal_courtship")
        .next()
        .unwrap();
    assert!(formal.contains("father_of_at(ctx, partner_id, minute)"));
    let establishment = source
        .split("fn establish_courtship")
        .nth(1)
        .unwrap()
        .split("#[reducer]\npub fn begin_formal_courtship")
        .next()
        .unwrap();
    assert!(establishment.contains("active courtship of another kind"));
    assert!(establishment.contains("Ended courtship history is final"));
}

#[test]
fn player_courtship_rejections_carry_stable_typed_codes() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    for reducer in [
        "pub fn begin_formal_courtship",
        "pub fn begin_informal_courtship",
        "pub fn schedule_wedding",
    ] {
        let body = source
            .split(reducer)
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("courtship reducer body");
        assert!(body.contains("CourtshipRejectionCode"));
    }
    assert!(source.contains("CourtshipRejectionCode::MutualAttraction"));
    assert!(source.contains("CourtshipRejectionCode::ExclusiveCommitment"));
    assert!(source.contains("CourtshipRejectionCode::FatherApproval"));
}
