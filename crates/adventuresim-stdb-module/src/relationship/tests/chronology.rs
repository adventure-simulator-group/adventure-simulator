#[test]
fn npc_policy_uses_the_single_character_clock_and_central_lifecycle_hook() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    assert!(!source.contains("struct NpcPersonalTime"));
    assert!(!source.contains("npc_personal_time()"));
    let advancement = source
        .split("pub fn advance_npc_personal_time")
        .nth(1)
        .unwrap()
        .split("fn canonical_pair")
        .next()
        .unwrap();
    let clock_write = advancement
        .find("character_time().character_id().update(time)")
        .unwrap();
    assert!(
        clock_write
            < advancement
                .find("settle_lifecycle_after_character_time_write")
                .unwrap()
    );
    assert!(advancement.contains("target_minute < time.minutes"));
}

#[test]
fn soft_scope_never_reads_or_synchronizes_the_target_clock() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let guard = source
        .split("pub fn enforce_temporal_scope")
        .nth(1)
        .unwrap()
        .split("pub enum KinshipKind")
        .next()
        .unwrap();
    let soft = guard
        .split("TemporalScope::ActorLocal")
        .nth(1)
        .unwrap()
        .split("TemporalScope::NpcCanonical")
        .next()
        .unwrap();
    assert!(!soft.contains("canonical_now(ctx, target_id)"));
    assert!(!soft.contains("synchronize"));
}

#[test]
fn exclusive_scope_and_weddings_use_canonical_world_time() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let guard = source
        .split("pub fn enforce_temporal_scope")
        .nth(1)
        .unwrap()
        .split("pub enum KinshipKind")
        .next()
        .unwrap();
    assert!(guard.contains("crate::time::refresh_clock(ctx)"));
    assert!(!guard.contains("actor_minute.max(target_minute)"));
    let wedding = source
        .split("pub fn settle_due_weddings")
        .nth(1)
        .unwrap()
        .split("pub fn settle_due_weddings_global")
        .next()
        .unwrap();
    assert!(!wedding.contains("advance_npc_personal_time"));
    assert!(wedding.contains("let now = crate::time::refresh_clock(ctx)?"));
    assert!(!wedding.contains("all_participants_reached_ceremony"));
}

#[test]
fn representative_soft_actions_guard_scope_without_target_clock_writes() {
    for source in [
        include_str!("../../strategic/dialogue_sessions.rs"),
        include_str!("../../organization.rs"),
        include_str!("../../strategic/inventory_trade.rs"),
        include_str!("../../residence.rs"),
    ] {
        assert!(source.contains("enforce_temporal_scope"));
    }
    let dialogue = crate::production_source(include_str!("../../strategic/dialogue_sessions.rs"));
    let guarded = dialogue
        .split("pub fn start_dialogue")
        .nth(1)
        .unwrap()
        .split("pub fn answer_dialogue_prompt")
        .next()
        .unwrap_or(dialogue);
    assert!(guarded.contains("TemporalScope::PairwiseSoft"));
    assert!(!guarded.contains("character_time().character_id().update"));
}
