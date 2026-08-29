#[test]
fn socializing_receipts_are_actor_day_target_cumulative_and_party_safe() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let socializing = source
        .split("pub fn apply_scheduled_socializing")
        .nth(1)
        .unwrap()
        .split("pub fn settle_secret_courtship_discovery_for_pair")
        .next()
        .unwrap();
    assert!(source.contains("format!(\"socializing:{actor_id}:{day}:{target_id}\")"));
    assert!(socializing.contains("receipt.day == day"));
    assert!(socializing.contains(".max()"));
    assert!(source.contains("receipt.minutes.saturating_add(minutes)"));
    assert!(socializing.contains("apply_async_socializing_without_familiarity"));
    assert!(socializing.contains("socializing_target(ctx, actor_id, day, cursor)"));
    assert!(socializing.contains("private zero-minute watermark"));
    let target = source
        .split("fn socializing_target")
        .nth(1)
        .unwrap()
        .split("pub fn apply_scheduled_socializing")
        .next()
        .unwrap();
    assert!(target.contains("character_alive_at(ctx, candidate.id, effective_minute)"));
    assert!(target.contains("candidate_minute <= effective_minute"));
    assert!(target.contains("death.strategic_minute > effective_minute"));
    assert!(target.contains("npc_is_present(ctx, &presence, effective_minute)"));
    assert!(target.contains("select_daily_location_target"));
    assert!(!target.contains("canonical_now(ctx, actor_id)"));
}

#[test]
fn scheduled_socializing_splits_at_availability_boundaries() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let boundaries = source
        .split("fn next_socializing_boundary")
        .nth(1)
        .unwrap()
        .split("fn record_socializing_receipt")
        .next()
        .unwrap();
    assert!(boundaries.contains("presence.start_minute, presence.end_minute"));
    assert!(boundaries.contains("character_birth()"));
    assert!(boundaries.contains("character_death()"));
    assert!(boundaries.contains("courtship.started_minute"));
    assert!(boundaries.contains("courtship.resolved_minute"));

    let socializing = source
        .split("pub fn apply_scheduled_socializing")
        .nth(1)
        .unwrap()
        .split("pub fn settle_secret_courtship_discovery_for_pair")
        .next()
        .unwrap();
    assert!(socializing.contains("while cursor < end"));
    assert!(socializing.contains("next_socializing_boundary(ctx, actor_id, cursor, end)"));
    assert!(socializing.contains("allocation(slice_end).saturating_sub(allocation(cursor))"));
    assert!(socializing.contains("cursor = slice_end"));
}
