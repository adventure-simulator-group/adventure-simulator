#[test]
fn settlement_arrival_only_catches_up_to_local_time_of_day() {
    assert_eq!(settlement_arrival_downtime(600, 600), 0);
    assert_eq!(settlement_arrival_downtime(600, 660), 60);
    assert_eq!(settlement_arrival_downtime(1_380, 60), 120);
    assert!(settlement_arrival_downtime(0, 1_439) < MINUTES_PER_DAY);
}

#[test]
fn tent_validation_precedes_every_explicit_rest_mutation() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let rest = source
        .split("pub fn rest_at_camp")
        .nth(1)
        .and_then(|tail| tail.split("fn party_fatigue_summary").next())
        .unwrap();
    let validation = rest.find("\"field_shelter\"").unwrap();
    assert!(validation < rest.find("wash_party_before_explicit_rest").unwrap());
    assert!(rest.contains("FieldShelter::Tent"));
}

#[test]
fn settlement_rest_uses_indoor_exposure() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let rest = source
        .split("fn rest_for_minutes")
        .nth(1)
        .and_then(|tail| tail.split("fn inn_stay_cost").next())
        .expect("settlement rest implementation");
    assert!(rest.contains("ExposureShelter::Indoor"));
    assert!(!rest.contains("FieldShelter::Tent"));
}

#[test]
fn settlement_wait_and_downtime_use_indoor_exposure() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    for (start, end) in [
        ("pub fn advance_character_wait_time", "fn default_schedule"),
        (
            "pub fn perform_immediate_activity",
            "fn apply_organization_outcomes",
        ),
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
        assert!(body.contains("ExposureShelter::Indoor"), "{start}");
        assert!(body.contains("FieldShelter::Bivouac"), "{start}");
    }
}

#[test]
fn settlement_rest_rejects_unavailable_inn_and_temple_services() {
    use adventuresim_world_schema::{SettlementActionService, SettlementService};

    let mut profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    assert!(require_settlement_rest_service(&profile, SettlementActionService::Inn).is_ok());
    assert!(
        require_settlement_rest_service(&profile, SettlementActionService::Temple).is_err()
    );
    profile.services.clear();
    assert!(require_settlement_rest_service(&profile, SettlementActionService::Inn).is_err());
    profile.services.push(SettlementService::Temple);
    assert!(require_settlement_rest_service(&profile, SettlementActionService::Temple).is_ok());
}

#[test]
fn settlement_rest_accepts_exact_wake_minutes_with_bounded_duration() {
    assert!(validate_settlement_rest_minutes(36 * 60 + 32).is_ok());
    assert!(validate_settlement_rest_minutes(2 * MINUTES_PER_DAY).is_ok());
    assert!(validate_settlement_rest_minutes(MIN_SETTLEMENT_REST_MINUTES).is_ok());
    assert!(validate_settlement_rest_minutes(MAX_SETTLEMENT_REST_MINUTES).is_ok());
    assert!(validate_settlement_rest_minutes(MIN_SETTLEMENT_REST_MINUTES - 1).is_err());
    assert!(validate_settlement_rest_minutes(MAX_SETTLEMENT_REST_MINUTES + 1).is_err());
}

#[test]
fn inn_stay_cost_only_rounds_up_partial_days() {
    assert_eq!(inn_stay_cost(0), Ok(0));
    assert_eq!(inn_stay_cost(MINUTES_PER_DAY), Ok(2));
    assert_eq!(inn_stay_cost(2 * MINUTES_PER_DAY), Ok(4));
    assert_eq!(inn_stay_cost(1), Ok(2));
    assert_eq!(inn_stay_cost(MINUTES_PER_DAY + 1), Ok(4));
}

#[test]
fn sponsored_inn_rest_is_one_day_exact_cost_and_never_transfers_coin() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let sponsored = source
        .split("pub fn sponsor_party_member_inn_rest")
        .nth(1)
        .and_then(|tail| tail.split("fn require_settlement_rest_service").next())
        .expect("sponsored rest reducer");
    for gate in [
        "require_strategic_character_authority(ctx, payer_id)",
        "payer_id == patient_id",
        "same party",
        "current party membership",
        "named settlement",
        "require_character_rest_service(ctx, patient_id, SettlementActionService::Inn)",
        "patient_publicly_needs_rest(ctx, patient_id)",
        "expected_cost != authoritative_cost",
        "Patient can afford ordinary inn rest",
        "payer_id,",
    ] {
        assert!(sponsored.contains(gate), "missing sponsorship gate {gate}");
    }
    assert!(sponsored.contains("MINUTES_PER_DAY"));
    assert!(sponsored.contains("sponsorship_gap"));
    assert!(sponsored.contains("Some(payer_id)"));
    assert!(!sponsored.contains("credit_personal_currency"));
    assert!(!sponsored.contains("party_inventory_item().insert"));

    let rest = source
        .split("fn rest_for_minutes")
        .nth(1)
        .and_then(|tail| tail.split("fn inn_stay_cost").next())
        .expect("settlement rest payment boundary");
    assert!(rest.contains("patient_contribution"));
    assert!(rest.contains("sponsor_contribution"));
    assert!(rest.contains("consume_personal_currency(ctx, character_id"));
    assert!(rest.contains("consume_personal_currency(ctx, sponsor_id"));
}

#[test]
fn settlement_rest_consumes_elapsed_needs_once_in_terminal_safe_order() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let rest = source
        .split("fn rest_for_minutes")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_settlement_rest_minutes").next())
        .expect("settlement rest implementation");
    assert_eq!(rest.matches("inn_stay_cost(requested_minutes)?").count(), 1);
    assert_eq!(rest.matches("inn_stay_cost(elapsed)?").count(), 1);
    assert!(
        rest.find("personal_currency_total").unwrap()
            < rest.find("preview_injury_boundary").unwrap()
    );
    assert!(rest.contains("InjuryRecoveryMinutes::new(requested_recovery)"));
    assert!(
        rest.find("inn_stay_cost(elapsed)?").unwrap()
            < rest
                .find("crate::condition::apply_settlement_rest_elapsed_needs(")
                .unwrap()
    );
    let needs = "crate::condition::apply_settlement_rest_elapsed_needs(";
    assert_eq!(rest.matches(needs).count(), 1);
    assert!(rest.find("settle_shared_party_time").unwrap() < rest.find(needs).unwrap());
    assert!(rest.find(needs).unwrap() < rest.find("finish_disease_interval").unwrap());
    assert!(
        rest.find("finish_disease_interval").unwrap()
            < rest.find("terminal.is_some()").unwrap()
    );
    assert!(
        rest.find("terminal.is_some()").unwrap() < rest.find("clear_stomach_fullness").unwrap()
    );
}

#[test]
fn automatic_social_chats_run_only_after_positive_discretionary_downtime() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let rest = source
        .split("fn rest_for_minutes")
        .nth(1)
        .and_then(|tail| tail.split("fn inn_stay_cost").next())
        .expect("settlement rest implementation");
    assert!(rest.contains("if training_elapsed > 0"));
    assert!(rest.contains("apply_automatic_social_chats(ctx, character_id,"));

    let camp = source
        .split("pub fn rest_at_camp")
        .nth(1)
        .and_then(|tail| tail.split("fn party_fatigue_summary").next())
        .expect("camp downtime implementation");
    assert!(camp.contains("if downtime > 0"));
    assert!(camp.contains("apply_automatic_social_chats(ctx, member_id,"));

    for (start, end) in [
        (
            "pub fn advance_travel_time",
            "pub fn advance_character_wait_time",
        ),
        ("pub fn advance_character_wait_time", "fn default_schedule"),
    ] {
        let ordinary = source
            .split(start)
            .nth(1)
            .and_then(|tail| tail.split(end).next())
            .expect("non-downtime interval");
        assert!(!ordinary.contains("apply_automatic_social_chats"));
    }
}

#[test]
fn camp_schedule_excludes_organization_training_and_activity() {
    let schedule = ScheduleAllocation {
        apprenticeship_minutes: 120,
        apprenticeship_organization_id: Some("lodge_hart_king".into()),
        profession_practice_minutes: 180,
        practice_organization_id: Some("lodge_hart_king".into()),
        prayer_minutes: 60,
        ..Default::default()
    };
    let allowed = allowed_camp_schedule(&schedule);
    assert_eq!(allowed.apprenticeship_minutes, 0);
    assert!(allowed.apprenticeship_organization_id.is_none());
    assert_eq!(allowed.profession_practice_minutes, 0);
    assert!(allowed.practice_organization_id.is_none());
    assert_eq!(allowed.prayer_minutes, 60);
}

#[test]
fn physiology_check_sets_the_daily_healing_rate() {
    assert!((health_recovered_per_day(0.0) - 0.01).abs() < f32::EPSILON);
    assert!((health_recovered_per_day(2.5) - 0.035).abs() < f32::EPSILON);
    assert!((health_recovered_per_day(5.0) - 0.06).abs() < f32::EPSILON);
    assert!((health_recovered_per_day(8.0) - 0.06).abs() < f32::EPSILON);
}
