#[test]
fn explicit_stationary_frontiers_still_reject_retroactive_targets() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let advance = source
        .split("pub(crate) fn advance_stationary_character_to(")
        .nth(1)
        .and_then(|tail| tail.split("pub fn update_training_schedule").next())
        .expect("explicit stationary advancement");
    assert!(advance.contains("if target_minutes < character_time.minutes"));
    assert!(advance.contains("Character time cannot be advanced retroactively"));
}

#[test]
fn every_authoritative_clock_commit_has_one_exposure_application() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    for (start, end) in [
        (
            "pub fn advance_character_time",
            "pub fn preview_travel_time",
        ),
        ("pub fn advance_character_wait_time", "fn default_schedule"),
        (
            "pub fn perform_immediate_activity",
            "fn apply_organization_outcomes",
        ),
        ("fn rest_for_minutes", "fn validate_settlement_rest_minutes"),
        ("pub fn rest_at_camp", "fn party_fatigue_summary"),
        (
            "pub(crate) fn advance_stationary_character_to",
            "pub fn update_training_schedule",
        ),
    ] {
        let body = source
            .split(start)
            .nth(1)
            .and_then(|tail| tail.split(end).next())
            .expect(start);
        assert_eq!(
            body.matches("apply_weather_exposure(").count(),
            1,
            "{start} must apply exposure exactly once"
        );
        assert!(
            body.find("update(character_time)")
                .or_else(|| body.find("update(time)"))
                .unwrap()
                < body.find("apply_weather_exposure(").unwrap()
        );
    }
}

#[test]
fn authoritative_time_paths_split_at_lifecycle_boundaries() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    for (start, end) in [
        (
            "pub fn advance_character_time",
            "fn advance_character_time_in_plan",
        ),
        (
            "fn advance_character_time_in_plan",
            "/// Actual strategic movement",
        ),
        (
            "pub fn advance_character_wait_time",
            "pub fn advance_character_wait_time_in_plan",
        ),
        (
            "pub fn advance_character_wait_time_in_plan",
            "fn default_schedule",
        ),
    ] {
        let path = source
            .split(start)
            .nth(1)
            .and_then(|tail| tail.split(end).next())
            .expect("time advancement path");
        assert!(path.contains("next_lifecycle_boundary"));
        assert!(path.contains("minutes.saturating_sub(first)"));
        assert!(path.contains("settle_lifecycle_after_character_time_write"));
    }
}

#[test]
fn party_activity_uses_authoritative_current_case_site_occupancy() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let synchronization = source
        .split("pub fn synchronize_party_for_activity")
        .nth(1)
        .and_then(|tail| tail.split("/// Neutral/location-appropriate").next())
        .expect("party activity synchronization");
    assert!(synchronization.contains("party"));
    assert!(synchronization.contains(".current_case_site_id"));
    assert_eq!(
        synchronization
            .matches("current_character_case_site_occupancy")
            .count(),
        2
    );
    assert!(synchronization.contains("leader.case_site_id == *case_site_id"));
    assert!(synchronization.contains("member.case_site_id == *case_site_id"));
    assert!(synchronization.contains("characters_are_contextually_present"));
}
