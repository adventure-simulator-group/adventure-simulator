#[test]
fn discovery_attempts_use_frozen_observers_and_weaker_deception() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let discovery = source
        .split("pub fn settle_secret_courtship_discovery_for_pair")
        .nth(1)
        .unwrap()
        .split("fn personality_disposition")
        .next()
        .unwrap();
    assert!(discovery.contains("{observer_id}:{day}"));
    assert!(discovery.contains("courtship_observer_baseline()"));
    assert!(discovery.contains("courtship.weaker_deception_baseline"));
    assert!(discovery.contains("baseline.observer_insight"));
    assert!(discovery.contains("character_alive_at(ctx, baseline.observer_id"));
    assert!(discovery.contains("succeeded,"));
    assert!(discovery.contains("- 8.0"));
}

#[test]
fn secret_facade_is_daily_independent_and_stops_on_exposure() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let daily = source
        .split("pub fn settle_secret_courtship_discovery_for_character")
        .nth(1)
        .unwrap()
        .split("fn personality_disposition")
        .next()
        .unwrap();
    assert!(daily.contains("next_discovery_day"));
    assert!(daily.contains("CourtshipStatus::Active"));
    assert!(daily.contains("settle_secret_courtship_discovery_for_pair"));
    let lifecycle = crate::production_source(crate::time::TIME_SOURCE)
        .split("pub(crate) fn settle_lifecycle_after_character_time_write")
        .nth(1)
        .unwrap();
    assert!(lifecycle.contains("settle_secret_courtship_discovery_for_character"));
    let socializing = source
        .split("pub fn apply_scheduled_socializing")
        .nth(1)
        .unwrap()
        .split("pub fn settle_secret_courtship_discovery_for_pair")
        .next()
        .unwrap();
    assert!(!socializing.contains("settle_secret_courtship_discovery_for_pair"));
}

#[test]
fn delayed_discovery_penalty_uses_the_observer_current_anchor() {
    let source = crate::production_source(crate::relationship::RELATIONSHIP_SOURCE);
    let discovery = source
        .split("pub fn settle_secret_courtship_discovery_for_pair")
        .nth(1)
        .unwrap()
        .split("pub fn settle_secret_courtship_discovery_for_character")
        .next()
        .unwrap();
    assert!(discovery.contains("attempted_minute,"));
    assert!(discovery.contains("canonical_now(ctx, observer_id).unwrap_or(attempted_minute)"));
    assert!(!discovery.contains("let anchor_minute = attempted_minute;"));
}
