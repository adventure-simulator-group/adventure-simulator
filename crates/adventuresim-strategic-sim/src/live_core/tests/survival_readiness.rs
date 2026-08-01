#[test]
fn departure_readiness_applies_continuous_load_thermal_and_ammo_floors() {
    assert_eq!(public_encumbrance_remaining_bps(0.0, 100.0), 10_000);
    assert_eq!(public_encumbrance_remaining_bps(80.0, 100.0), 2_000);
    assert_eq!(public_encumbrance_remaining_bps(81.0, 100.0), 1_900);
    assert!(survival_equipment_ready(
        "ready",
        MAX_DEPARTURE_WETNESS_BPS,
        MAX_DEPARTURE_ABS_THERMAL_STRAIN as i32,
        true,
        RANGED_AMMUNITION_FLOOR,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
    assert!(!survival_equipment_ready(
        "ready",
        MAX_DEPARTURE_WETNESS_BPS,
        0,
        true,
        RANGED_AMMUNITION_FLOOR - 1,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
    assert!(!survival_equipment_ready(
        "ready",
        MAX_DEPARTURE_WETNESS_BPS + 1,
        0,
        false,
        0,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
}

#[test]
fn unsafe_immediate_route_can_wait_for_a_safe_next_walking_window() {
    assert_eq!(
        safe_departure_wait_minutes(false, true, Some(260)),
        Some(260)
    );
    assert_eq!(safe_departure_wait_minutes(true, true, Some(260)), None);
    assert_eq!(safe_departure_wait_minutes(false, false, Some(260)), None);
}

#[test]
fn safe_departure_wait_is_bounded_to_one_day() {
    assert_eq!(safe_departure_wait_minutes(false, true, Some(60)), Some(60));
    assert_eq!(
        safe_departure_wait_minutes(false, true, Some(1_440)),
        Some(1_440)
    );
    assert_eq!(safe_departure_wait_minutes(false, true, Some(0)), None);
    assert_eq!(safe_departure_wait_minutes(false, true, Some(59)), None);
    assert_eq!(safe_departure_wait_minutes(false, true, Some(1_441)), None);

    let source = LIVE_CORE_SOURCE;
    assert!(source.contains("minutes_until_next_walking_start"));
    assert!(source.contains("rest_at_settlement_hours_then(character_id, wait_minutes"));
    assert!(source.contains("CoreLoopEventKind::SafeDepartureWait"));
}

#[test]
fn next_window_wait_respects_authoritative_settlement_rest_minimum() {
    assert_eq!(representable_safe_departure_wait_minutes(1), Some(60));
    assert_eq!(representable_safe_departure_wait_minutes(59), Some(60));
    assert_eq!(representable_safe_departure_wait_minutes(60), Some(60));
    assert_eq!(representable_safe_departure_wait_minutes(260), Some(260));
    assert_eq!(
        representable_safe_departure_wait_minutes(1_440),
        Some(1_440)
    );
    assert_eq!(representable_safe_departure_wait_minutes(1_441), None);
}

#[test]
fn round_trip_schedule_uses_smallest_hour_breakpoint_that_fits() {
    assert_eq!(round_trip_walking_window_minutes(480, 260, 67), Some(600));
    assert_eq!(round_trip_walking_window_minutes(720, 260, 67), Some(720));
    assert_eq!(round_trip_walking_window_minutes(480, 700, 41), None);

    let source = LIVE_CORE_SOURCE;
    assert!(source.contains("action.case_id == pin.case_id"));
    assert!(source.contains("set_party_travel_itinerary_then"));
}

#[test]
fn combined_case_site_search_can_skip_a_thermal_unsafe_fatigue_safe_mode() {
    let windows = generated_action_walking_windows(480, 260, 67);
    assert_eq!(&windows[..4], &[480, 600, 327, 260]);
    assert!(
        windows.len()
            <= usize::from(adventuresim_core::strategic_time::MAX_WALKING_MINUTES_PER_DAY)
    );
    assert_eq!(windows.last(), Some(&1));
    let mut evaluated = Vec::new();
    let selected = select_generated_case_site_plan(
        480,
        260,
        67,
        false,
        600,
        |minutes, travel_at_night, wait_minutes| {
            let fatigue_safe = true;
            let thermal_safe = travel_at_night;
            evaluated.push((minutes, travel_at_night, wait_minutes, thermal_safe));
            (fatigue_safe && thermal_safe).then_some((minutes, travel_at_night, wait_minutes))
        },
    );
    assert!(evaluated.first().is_some_and(|candidate| !candidate.3));
    assert!(selected.is_some_and(|(_, travel_at_night, _)| travel_at_night));
    let departure = include_str!("../survival.rs")
        .split("pub(super) fn validate_case_site_thermal_readiness")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn field_recovery_rest_thermal_safe")
                .next()
        })
        .expect("combined case-site planner");
    assert!(departure.contains("let candidate_start ="));
    let thermal_candidate = departure
        .split("let Some(thermal_safe) = projected_round_trip_thermal_safe(")
        .nth(1)
        .and_then(|tail| tail.split(") else").next())
        .expect("candidate sequential thermal projection");
    assert!(thermal_candidate.contains("candidate_start,"));
    assert!(departure.contains("&& thermal_safe"));
}

#[test]
fn combined_case_site_search_reports_no_plan_when_every_joint_candidate_is_unsafe() {
    assert_eq!(
        select_generated_case_site_plan(480, 260, 67, false, 600, |_, _, _| None::<()>),
        None
    );
    assert_eq!(
        joint_case_site_plan_failure_reason(true, false),
        "route_action_plan_intrinsic"
    );
    assert_eq!(
        joint_case_site_plan_failure_reason(false, true),
        "route_weather_projection_unavailable"
    );
}

#[test]
fn case_site_recovery_ceil_safely_clears_a_weak_member_in_261_minutes() {
    let calories = adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY * 260.0 / 1_440.0;
    let member = adventuresim_core::strategic_time::ItineraryMember {
        fatigue_capacity: 6_000.0,
        calories_used: calories,
        camp_schedule: Default::default(),
    };
    assert_eq!(
        adventuresim_core::strategic_time::common_fatigue_clear_minutes(&[member]),
        261
    );
    assert_eq!(
        adventuresim_core::strategic_time::camp_fatigue_after(calories, 261, Default::default()),
        0.0
    );
    assert_eq!(
        classify_on_site_action_decision(false, true, 261, true),
        OnSiteActionDecision::RestThenRetry(261)
    );
}

#[test]
fn thermally_unsafe_case_site_recovery_is_refused_before_return_policy() {
    assert_eq!(
        classify_on_site_action_decision(false, false, 260, true),
        OnSiteActionDecision::ReturnNow
    );
    assert_eq!(
        classify_on_site_action_decision(false, false, 260, false),
        OnSiteActionDecision::Hold
    );
    let survival = include_str!("../survival.rs");
    assert!(survival.contains("projected_stationary_field_thermal_state("));
    let generated = include_str!("../generated_cases.rs");
    assert!(generated.contains("OnSiteActionDecision::RestThenRetry(minutes)"));
    assert!(generated.contains("generated_case_site_planned_recovery"));
    assert!(generated.contains("let post_recovery ="));
    assert!(generated.contains("generated_case_site_recoveries.contains(&recovery_key)"));
    assert!(generated.contains("planned_case_site_recovery_exhausted"));
    assert!(generated.contains("planned_case_site_recovery_minutes"));
    assert!(generated.contains("saturating_add(u64::from(action.duration_max_minutes))"));
    let travel = include_str!("../travel.rs");
    assert!(travel.contains("minutes.checked_add(additional_plan_minutes)"));
}

#[test]
fn successful_planned_case_site_recovery_continues_before_yielding_the_turn() {
    let generated = include_str!("../generated_cases.rs");
    let arrival_recovery = generated
        .split("if planned_case_site_recovery_minutes > 0")
        .nth(1)
        .and_then(|tail| tail.split("if let Some((action, wait_minutes))").next())
        .expect("post-arrival planned recovery flow");
    let authoritative_recheck = arrival_recovery
        .find("let post_recovery = self.generated_action_return_thermal_decision(")
        .expect("authoritative post-recovery decision");
    let ready_continues = arrival_recovery
        .find("OnSiteActionDecision::Ready => {}")
        .expect("ready post-recovery continuation");
    let same_turn_loop = arrival_recovery
        .rfind("continue;")
        .expect("same-turn generated-case loop continuation");
    assert!(authoritative_recheck < ready_continues && ready_continues < same_turn_loop);
    let ready_arm = arrival_recovery
        .split("OnSiteActionDecision::Ready => {}")
        .nth(1)
        .and_then(|tail| tail.split("OnSiteActionDecision::ReturnNow").next())
        .unwrap();
    assert!(!ready_arm.contains("return Ok(false)"));
}

#[test]
fn one_generated_action_attempt_takes_its_reserved_return_before_another_attempt() {
    let generated = include_str!("../generated_cases.rs");
    let attempted_action = generated
        .split("perform_investigation_action_then(")
        .nth(1)
        .and_then(|tail| {
            tail.split(
                "if let Some(action) = actions.iter().find(|row| row.can_travel_to_required_site)",
            )
            .next()
        })
        .expect("single generated action attempt and return boundary");
    let action_event = attempted_action
        .find("CoreLoopEventKind::GeneratedInvestigationAction")
        .expect("public action progress event");
    let return_safety = attempted_action
        .find("generated_action_return_thermal_decision(party_id, &return_pin, 0)")
        .expect("authoritative reserved-return safety recheck");
    let return_reducer = attempted_action
        .find("evacuate_generated_party_to_origin(")
        .expect("reserved return execution");
    let progress_exit = attempted_action
        .rfind("return Ok(true)")
        .expect("public-progress exit after reserved return");
    assert!(action_event < return_safety && return_safety < return_reducer);
    assert!(return_reducer < progress_exit);
    assert!(!attempted_action.contains("generated_actor_ready_after_time"));
    assert!(!attempted_action.contains("continue;"));
    assert!(attempted_action.contains("plan=one_attempt_then_reserved_return"));
    let evacuation = generated
        .split("fn evacuate_generated_party_to_origin")
        .nth(1)
        .and_then(|tail| tail.split("pub(super) fn advance_generated_case").next())
        .expect("reserved-return helper");
    assert!(evacuation.contains("generated_case_site_recoveries"));
    assert!(evacuation.contains("stored_case != case_id"));
}

#[test]
fn every_on_site_action_reserves_fatigue_and_thermal_capacity_before_execution() {
    let generated = include_str!("../generated_cases.rs");
    let available = generated
        .split("if let Some(action) = actions.iter().find(|row| row.available).cloned()")
        .nth(1)
        .expect("available generated action branch");
    let selection = generated
        .split("let ready_action_index = actions.iter().position")
        .nth(1)
        .and_then(|tail| {
            tail.split("if let Some(action) = actions.iter().find(|row| row.available).cloned()")
                .next()
        })
        .expect("safety-aware on-site action selection");
    assert!(selection.contains("OnSiteActionDecision::Ready"));
    assert!(selection.contains("actions.swap(0, index)"));
    let reserve = available
        .find("generated_action_return_thermal_decision")
        .expect("on-site action and return reserve decision");
    let reducer = available
        .find("perform_investigation_action_then")
        .expect("investigation action reducer");
    assert!(reserve < reducer);
    assert!(available.contains("OnSiteActionDecision::ReturnNow"));
    assert!(available.contains("self.evacuate_generated_party_to_origin("));
    assert!(available.contains("duration_max_minutes"));
    let evacuation = generated
        .split("fn evacuate_generated_party_to_origin")
        .nth(1)
        .and_then(|tail| tail.split("pub(super) fn advance_generated_case").next())
        .expect("generated party reserve evacuation");
    assert!(evacuation.contains("incapacitation_reserve_evacuation"));
    let survival = include_str!("../survival.rs");
    assert!(survival.contains("projected_action_ready("));
    assert!(survival.contains("projected_itinerary_thermal_safe"));
}

#[test]
fn on_site_reserve_requires_a_ready_actor_but_allows_survivable_stagger() {
    let survival = include_str!("../survival.rs");
    let on_site = survival
        .split("pub(super) fn generated_action_return_thermal_decision")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn prepare_party_for_departure")
                .next()
        })
        .expect("on-site action reserve");
    assert!(on_site.contains("stats.calories_used"));
    assert!(on_site.contains("calories_after_strenuous_action("));
    assert!(on_site.contains("condition.status == \"ready\""));
    assert!(on_site.contains("let action_survivable = projected_action_survivable("));
    let model = include_str!("../model.rs");
    let action_cost = model
        .split("fn calories_after_strenuous_action")
        .nth(1)
        .and_then(|tail| tail.split("fn round_trip_walking_window_minutes").next())
        .expect("shared strenuous action cost");
    assert!(action_cost.contains("action_minutes as f32 / 1_440.0"));
    assert!(action_cost.contains("STRATEGIC_TRAVEL_KCAL_PER_DAY"));
}

#[test]
fn on_site_reserve_allows_a_survivable_return_that_ends_staggered() {
    let survival = include_str!("../survival.rs");
    let immediate_return = survival
        .split("return_now_safe &= projected_itinerary_thermal_safe(")
        .nth(1)
        .and_then(|tail| tail.split("let Some(action)").next())
        .expect("immediate return safety branch");
    assert!(immediate_return.contains("projected_itinerary_survivable("));
    assert!(
        !immediate_return.contains("!medically_critical"),
        "critical illness must block investigation, not a survivable immediate evacuation"
    );
    assert!(survival.contains("character_illness_status()"));
    assert!(!immediate_return.contains("projected_total(&return_now) <= 0.5"));
    let action_return = survival
        .split("action_return_safe &= action.safe")
        .nth(1)
        .and_then(|tail| tail.split("if action_return_safe").next())
        .expect("mission-ready action and return branch");
    assert!(action_return.contains("&& actor_ready_before_action"));
    assert!(action_return.contains("&& action_survivable"));
    assert!(!action_return.contains("projected_total(&return_after_action)"));
    assert!(projected_action_ready(0.1, 3_000.0, 6_000.0));
    assert!(
        !projected_action_ready(0.1, 5_400.0, 6_000.0),
        "the actor must be ready before starting another action"
    );
    assert!(
        projected_action_survivable(0.0, 5_400.0, 6_000.0),
        "an action may leave a zero-fatigue fixture actor staggered but not incapacitated"
    );
    assert!(
        !projected_action_survivable(0.0, 6_000.0, 6_000.0),
        "an action projected to incapacitate a member must be blocked"
    );
}

#[test]
fn departure_and_on_site_forecasts_share_observed_fatigue_and_action_cost() {
    let expected =
        2_000.0 + 67.0 / 1_440.0 * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY;
    assert_eq!(calories_after_strenuous_action(2_000.0, 67), expected);
    let survival = include_str!("../survival.rs");
    let departure = survival
        .split("pub(super) fn validate_case_site_thermal_readiness")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn field_recovery_rest_thermal_safe")
                .next()
        })
        .expect("departure round-trip forecast");
    assert!(departure.contains("calories_used: stats.calories_used"));
    assert!(departure.contains("calories_after_strenuous_action("));
    let candidate_window = departure
        .find("select_generated_case_site_plan(")
        .expect("bounded candidate-specific walking-window search");
    let candidate_outbound = departure
        .find("let Some(candidate_outbound)")
        .expect("candidate-specific outbound forecast");
    let candidate_readiness = departure
        .find("candidate_outbound.member_final_fatigue[member_index]")
        .expect("candidate-specific action readiness");
    assert!(candidate_window < candidate_outbound && candidate_outbound < candidate_readiness);
    assert!(departure.contains("representable_safe_departure_wait_minutes"));
    assert!(departure.contains("nonfatigue_incapacitation"));
    assert!(departure.contains("projected_action_ready("));
    assert!(departure.contains("projected_action_survivable("));
    assert!(departure.contains("route_fatigue_risk"));
    let on_site = survival
        .split("pub(super) fn generated_action_return_thermal_decision")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn prepare_party_for_departure")
                .next()
        })
        .expect("on-site action and return forecast");
    assert!(on_site.contains("calories_after_strenuous_action("));
}

#[test]
fn heterogeneous_members_use_their_own_final_fatigue_not_the_party_maximum() {
    assert!(projected_action_ready(0.0, 5_100.0, 6_000.0));
    assert!(projected_action_ready(0.1, 7_200.0, 12_000.0));
    assert!(
        !projected_action_ready(0.1, 10_200.0, 12_000.0),
        "assigning the weaker member's maximum ratio to the stronger member is a false rejection"
    );
    let survival = include_str!("../survival.rs");
    assert!(survival.contains("candidate_outbound.member_final_fatigue[member_index]"));
    assert!(survival.contains("itinerary.member_final_fatigue[member_index]"));
    assert!(!survival.contains("maximum_fatigue_end"));
}

#[test]
fn generated_case_site_sync_is_forecast_before_both_committed_catchups() {
    let generated = include_str!("../generated_cases.rs");
    let synchronization = generated
        .split("pub(super) fn synchronize_generated_party_for_action")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn emit_generated_investigation_attempt")
                .next()
        })
        .expect("generated party synchronization");
    assert!(synchronization.contains("let at_case_site ="));
    assert_eq!(
        synchronization
            .matches("generated_case_site_sync_safe")
            .count(),
        2
    );
    assert_eq!(
        synchronization
            .matches("synchronize_party_for_activity_then")
            .count(),
        2
    );
}

#[test]
fn route_plan_persists_the_same_selected_schedule_used_for_both_legs() {
    let survival = include_str!("../survival.rs");
    let route = survival
        .split("pub(super) fn validate_case_site_thermal_readiness")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn field_recovery_rest_thermal_safe")
                .next()
        })
        .unwrap();
    assert!(route.contains("selected_case_site_plan = Some(selected_plan)"));
    assert!(route.contains("selected_plan.outbound.total_elapsed_minutes"));
    assert!(route.contains("selected_plan.returned.total_elapsed_minutes"));
    assert!(route.contains("wait_minutes: selected_plan.departure_wait_minutes"));
    assert!(route.contains("travel_at_night: selected_plan.travel_at_night"));
    assert!(route.contains("DepartureReadiness::ReadyWithItinerary"));
    let generated = include_str!("../generated_cases.rs");
    let cycle = include_str!("../cycle.rs");
    assert!(generated.contains("configure_safe_departure_itinerary("));
    assert!(cycle.contains("configure_safe_departure_itinerary("));
}

#[test]
fn route_survivability_uses_per_member_peak_and_critical_illness_can_return_now() {
    let model = include_str!("../model.rs");
    let peak_survivability = model
        .split("fn projected_itinerary_survivable")
        .nth(1)
        .and_then(|tail| tail.split("fn round_trip_walking_window_minutes").next())
        .expect("per-member peak-fatigue survivability helper");
    assert!(peak_survivability.contains(".member_maximum_fatigue"));
    assert!(peak_survivability.contains(".get(member_index)"));
    assert!(peak_survivability.contains("fatigue * fatigue_capacity"));
    let survival = include_str!("../survival.rs");
    let on_site = survival
        .split("pub(super) fn generated_action_return_thermal_decision")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn prepare_party_for_departure")
                .next()
        })
        .unwrap();
    let return_now = on_site
        .split("return_now_safe &=")
        .nth(1)
        .and_then(|tail| tail.split("let Some(action)").next())
        .unwrap();
    assert!(return_now.contains("projected_itinerary_survivable("));
    assert!(!return_now.contains("!medically_critical"));
    let action_return = on_site.split("action_return_safe &=").nth(1).unwrap();
    assert!(action_return.contains("!medically_critical"));
    let generated = include_str!("../generated_cases.rs");
    let preflight = generated
        .split("pub(super) fn synchronize_generated_party_for_action")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn emit_generated_investigation_attempt")
                .next()
        })
        .unwrap();
    let critical = preflight.find("if medically_critical").unwrap();
    let sync = preflight
        .find("synchronize_party_for_activity_then")
        .unwrap();
    assert!(critical < sync);
    assert!(preflight.contains("OnSiteActionDecision::ReturnNow"));
    assert!(preflight.contains("evacuate_generated_party_to_origin("));
}

#[test]
fn simulator_weather_forecast_matches_authority_zero_elevation_route() {
    let survival = include_str!("../survival.rs");
    assert!(!survival.contains("elevation_m: origin_row.elevation.meters"));
    assert!(!survival.contains("elevation_m: row.elevation.meters"));
    assert!(survival.contains("Match the authoritative straight-line route weather model"));
}

#[test]
fn no_joint_case_site_candidate_is_stably_deferred_without_recovery_looping() {
    let survival = include_str!("../survival.rs");
    let departure = survival
        .split("pub(super) fn validate_case_site_thermal_readiness")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn field_recovery_rest_thermal_safe")
                .next()
        })
        .expect("case-site departure readiness");
    assert!(departure.contains("candidate_complete_projection |= projection_available"));
    assert!(departure.contains("joint_case_site_plan_failure_reason("));
    let no_candidate = departure
        .split("let Some(selected_action)")
        .nth(1)
        .and_then(|tail| {
            tail.split("if selected_action.required_case_site_id")
                .next()
        })
        .expect("combined candidate failure classification");
    assert!(!no_candidate.contains("DepartureReadiness::WaitForQuestRecovery"));
    assert!(
        departure
            .contains("DepartureReadiness::Deferred(\"route_weather_projection_unavailable\")")
    );
    let generated = include_str!("../generated_cases.rs");
    assert!(!generated.contains("DepartureReadiness::WaitForQuestRecovery"));
}

#[test]
fn generated_advance_classification_consumes_recovery_without_claiming_public_progress() {
    assert_eq!(
        classify_generated_advance(true, true),
        GeneratedAdvanceResult::Progressed
    );
    assert_eq!(
        classify_generated_advance(false, true),
        GeneratedAdvanceResult::RecoveryCommitted
    );
    assert_eq!(
        classify_generated_advance(false, false),
        GeneratedAdvanceResult::NoProgress
    );
    let bootstrap = include_str!("../bootstrap.rs");
    let attempt = bootstrap
        .split("let advance_result = runner.advance_generated_case(")
        .nth(1)
        .and_then(|tail| tail.split("\"direct_contract\"").next())
        .unwrap();
    let record = attempt.find("record_generated_case_attempt").unwrap();
    let fallback = attempt
        .find("advance_result == GeneratedAdvanceResult::NoProgress")
        .unwrap();
    assert!(record < fallback);
}

#[test]
fn public_encumbrance_counts_carried_mass_but_not_body_mass() {
    let production = LIVE_CORE_SOURCE.split("#[cfg(test)]").next().unwrap();
    let load = production
        .split("fn public_personal_load_kg")
        .nth(1)
        .and_then(|tail| tail.split("fn public_character_capacity_kg").next())
        .expect("public personal-load projection");
    assert!(load.contains("carried_water_ml"));
    assert!(load.contains("inventory_weight"));
    assert!(load.contains("water_weight + inventory_weight"));
    assert!(!load.contains("body_weight_kg"));
}

#[test]
fn every_field_rest_uses_the_single_party_shelter_boundary() {
    let production = LIVE_CORE_SOURCE.split("#[cfg(test)]").next().unwrap();
    assert_eq!(production.matches(".rest_at_camp_then(").count(), 1);
    assert!(
        production
            .matches("rest_at_camp_with_party_shelter(")
            .count()
            > 1
    );
    for operation in [
        "expedition_recovery_rest",
        "wait_for_investigation_window_camp",
        "generated_case_site_planned_recovery",
        "\"rest_at_camp\"",
    ] {
        assert!(
            production.contains(operation),
            "missing field-rest path: {operation}"
        );
    }
    let helper = production
        .split("fn rest_at_camp_with_party_shelter")
        .nth(1)
        .expect("party shelter helper");
    assert!(helper.contains("FieldShelter::Tent"));
    assert!(helper.contains("PARTY_TENT_ITEM_ID"));
    assert!(helper.contains("tent_field_rests"));
    assert!(helper.contains("tent_field_rest_failures"));
}

#[test]
fn readiness_buys_shared_tent_and_personal_ammunition_through_ordinary_trade() {
    let source = LIVE_CORE_SOURCE;
    let tent = source
        .split("fn ensure_party_tent")
        .nth(1)
        .and_then(|tail| tail.split("fn ensure_ranged_ammunition").next())
        .expect("tent readiness");
    assert!(tent.contains("finalize_merchant_trade_then"));
    assert!(tent.contains("PARTY_TENT_ITEM_ID.to_owned()"));
    assert!(tent.contains("party tent purchase completed without party custody"));
    assert!(tent.contains("true,"), "tent purchase must use party scope");
    assert!(tent.contains("public_general_storefront_exists"));
    assert!(tent.contains("tent_provider_unavailable_bivouac"));
    assert!(tent.contains("shelter=bivouac"));
    assert!(tent.contains("return Ok(DepartureReadiness::Ready)"));

    let ammo = source
        .split("fn ensure_ranged_ammunition")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_party_departure_readiness").next())
        .expect("ammunition readiness");
    assert!(ammo.contains("capabilities()"));
    assert!(ammo.contains("RANGED_AMMUNITION_FLOOR"));
    assert!(ammo.contains("withdraw_stake_for_personal_purchase"));
    assert!(ammo.contains("finalize_merchant_trade_then"));
    assert!(ammo.contains("false,"), "arrows must remain personal");
    assert!(ammo.contains("ammo_before"));
    assert!(ammo.contains("ammo_after"));
}

#[test]
fn preparation_is_party_wide_and_rejects_overweight_upgrades() {
    let source = LIVE_CORE_SOURCE;
    let preparation = source
        .split("fn prepare_party_for_departure")
        .nth(1)
        .and_then(|tail| tail.split("fn rest_at_camp_with_party_shelter").next())
        .expect("party readiness preparation");
    assert!(preparation.contains("for agent in party_agents"));
    assert!(preparation.contains("self.try_upgrade(agent"));
    assert!(preparation.contains("ensure_ranged_ammunition"));
    assert!(preparation.contains("validate_party_departure_readiness"));

    let upgrades = source
        .split("fn try_upgrade")
        .nth(1)
        .expect("upgrade policy");
    assert!(upgrades.contains("public_party_load_and_capacity"));
    assert!(upgrades.contains("MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS"));
    assert!(upgrades.contains("public_equipment_storefront_offer"));
    assert!(upgrades.contains("purchase_personal_storefront_with_party_stake_then"));
    assert!(upgrades.contains("earned_shortfall"));
    assert!(upgrades.contains("medical_reserve"));

    let storefront = source
        .split("fn public_equipment_storefront_offer")
        .nth(1)
        .and_then(|tail| tail.split("fn withdraw_stake_for_personal_purchase").next())
        .expect("equipment storefront routing");
    for route in [
        "Storefront::Weapons",
        "Storefront::Armor",
        "Storefront::Clothing",
    ] {
        assert!(
            storefront.contains(route),
            "missing storefront route {route}"
        );
    }
    assert!(storefront.contains("public_storefront_available"));
    assert!(upgrades.contains("storefront_offer_unchanged"));
    assert!(upgrades.contains("stake_before_trade"));
    assert!(upgrades.contains("stake_after_trade"));
}

#[test]
fn resident_presence_cannot_create_an_unavailable_equipment_storefront() {
    let canonical = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    let profile = SettlementEconomyProfile {
        rules_version: canonical.rules_version,
        prosperity_score: 0,
        prosperity_tier: ProsperityTier::Subsistence,
        services: vec![SettlementService::Inn],
        specializations: vec![],
        stock: vec![],
    };
    assert!(!public_storefront_available(
        &profile,
        adventuresim_core::settlement_economy::Storefront::Weapons,
    ));
}

#[test]
fn equipment_storefront_requires_service_and_matching_stock_category() {
    let canonical = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    let mut profile = SettlementEconomyProfile {
        rules_version: canonical.rules_version,
        prosperity_score: 0,
        prosperity_tier: ProsperityTier::Subsistence,
        services: vec![SettlementService::Weaponsmith],
        specializations: vec![],
        stock: vec![SettlementStock {
            category: StockCategory::GeneralGoods,
            abundance: 1,
            provenance: ProfileFactProvenance::DeterministicGapFill,
        }],
    };
    let storefront = adventuresim_core::settlement_economy::Storefront::Weapons;
    let projected = public_settlement_economy_profile(&profile).unwrap();
    assert!(!adventuresim_core::settlement_economy::storefront_stocks(
        &projected,
        storefront,
        "club",
        adventuresim_core::settlement_economy::CatalogKind::Weapon,
    ));
    profile.stock.insert(
        0,
        SettlementStock {
            category: StockCategory::Weapons,
            abundance: 1,
            provenance: ProfileFactProvenance::DeterministicGapFill,
        },
    );
    let projected = public_settlement_economy_profile(&profile).unwrap();
    assert!(adventuresim_core::settlement_economy::storefront_stocks(
        &projected,
        storefront,
        "club",
        adventuresim_core::settlement_economy::CatalogKind::Weapon,
    ));
}

#[test]
fn staggered_default_providers_remain_ambiguous_before_hours_filtering() {
    assert_eq!(
        visible_unique_default_provider(&[(7, 0, 720), (8, 720, 1_440)], 300),
        None
    );
    assert_eq!(
        visible_unique_default_provider(&[(7, 0, 720)], 300),
        Some(7)
    );
    assert_eq!(visible_unique_default_provider(&[(7, 0, 720)], 900), None);
}

#[test]
fn equipment_quote_revalidation_is_fail_closed() {
    let selected = ("weapons".to_string(), 7, 12);
    assert!(storefront_offer_unchanged(
        &selected,
        Some(selected.clone())
    ));
    assert!(!storefront_offer_unchanged(&selected, None));
    assert!(!storefront_offer_unchanged(
        &selected,
        Some(("armor".into(), 7, 12)),
    ));
    assert!(!storefront_offer_unchanged(
        &selected,
        Some(("weapons".into(), 8, 12)),
    ));
    assert!(!storefront_offer_unchanged(
        &selected,
        Some(("weapons".into(), 7, 13)),
    ));
}

#[test]
fn departure_checks_only_living_members_and_projects_public_route_weather() {
    let source = LIVE_CORE_SOURCE;
    let living = source
        .split("fn living_party_member_ids")
        .nth(1)
        .and_then(|tail| tail.split("fn item_definition").next())
        .expect("living party projection");
    assert!(living.contains(".filter(|row| row.party_id == party_id)"));
    assert!(living.contains("character.id == row.character_id"));
    assert!(living.contains("character.alive"));

    for (helper, next_helper) in [
        ("ensure_party_tent", "ensure_ranged_ammunition"),
        (
            "ensure_ranged_ammunition",
            "validate_party_departure_readiness",
        ),
        (
            "validate_party_departure_readiness",
            "prepare_party_for_departure",
        ),
    ] {
        let body = source
            .split(&format!("fn {helper}"))
            .nth(1)
            .and_then(|tail| tail.split(&format!("fn {next_helper}")).next())
            .expect(helper);
        assert!(
            body.contains("living_party_member_ids"),
            "{helper} must exclude dead members"
        );
    }
    assert!(source.contains("route_weather_projection=pending_exact_case_site"));
    assert!(source.contains("route_weather_projection=deterministic_public"));
    assert!(source.contains("weatherproofing=conservative_zero"));
    assert!(source.contains("backend_character_times()"));
    assert!(source.contains("character_equipped_item()"));
}

#[test]
fn public_route_thermal_forecast_rejects_the_observed_cold_long_leg() {
    let point = PublicRoutePoint {
        latitude_microdegrees: 53_000_000,
        longitude_microdegrees: 10_000_000,
        elevation_m: 0,
    };
    assert_eq!(
        projected_route_thermal_safe(
            332_661,
            4_944,
            point,
            point,
            adventuresim_core::survival::SurvivalState::default(),
            adventuresim_core::survival::MAX_CLOTHING_INSULATION_BPS,
        ),
        Some(false),
        "even maximum visible insulation cannot make the retained-run route safe"
    );
}

#[test]
fn public_route_thermal_forecast_is_bounded_and_gates_both_case_paths() {
    let point = PublicRoutePoint {
        latitude_microdegrees: 0,
        longitude_microdegrees: 0,
        elevation_m: 0,
    };
    assert_eq!(
        projected_route_thermal_safe(
            0,
            MAX_CASE_SITE_THERMAL_FORECAST_MINUTES + 1,
            point,
            point,
            adventuresim_core::survival::SurvivalState::default(),
            0,
        ),
        None
    );
    let source = LIVE_CORE_SOURCE;
    assert_eq!(
        source
            .matches("validate_case_site_thermal_readiness(")
            .count(),
        3,
        "one helper plus generated and direct case-site gates"
    );
    for branch in [
        "pub(super) fn advance_generated_case",
        "pub(super) fn cycle",
    ] {
        let body = source.split(branch).nth(1).expect(branch);
        let gate_offset = body
            .find("validate_case_site_thermal_readiness")
            .expect("case-site route thermal gate");
        let travel_offset = body
            .find(".travel_to_case_site_then(")
            .expect("case-site travel reducer");
        assert!(
            gate_offset < travel_offset,
            "{branch} travels before its gate"
        );
    }
}

#[test]
fn outbound_safe_projection_can_still_reject_the_return_camp() {
    let point = PublicRoutePoint {
        latitude_microdegrees: 53_000_000,
        longitude_microdegrees: 10_000_000,
        elevation_m: 0,
    };
    let itinerary = |minutes| adventuresim_core::strategic_time::ItineraryForecast {
        segments: vec![adventuresim_core::strategic_time::ItinerarySegment {
            kind: adventuresim_core::strategic_time::ItinerarySegmentKind::Walking,
            elapsed_start: 0,
            elapsed_minutes: minutes,
            movement_start: 0,
            movement_minutes: minutes,
            average_fatigue_start: 0.0,
            average_fatigue_end: 0.0,
            maximum_fatigue_end: 0.0,
            required_rest_minutes: 0,
        }],
        member_final_fatigue: vec![0.0],
        member_maximum_fatigue: vec![0.0],
        total_elapsed_minutes: minutes,
        total_movement_minutes: minutes,
        truncated: false,
    };
    let outbound = projected_itinerary_thermal_state(
        332_661,
        &itinerary(1),
        point,
        point,
        adventuresim_core::survival::SurvivalState::default(),
        adventuresim_core::survival::MAX_CLOTHING_INSULATION_BPS,
        false,
    )
    .unwrap();
    assert!(outbound.safe);
    let return_camp = adventuresim_core::strategic_time::ItineraryForecast {
        segments: vec![adventuresim_core::strategic_time::ItinerarySegment {
            kind: adventuresim_core::strategic_time::ItinerarySegmentKind::Camp,
            elapsed_start: 0,
            elapsed_minutes: 4_944,
            movement_start: 0,
            movement_minutes: 0,
            average_fatigue_start: 0.0,
            average_fatigue_end: 0.0,
            maximum_fatigue_end: 0.0,
            required_rest_minutes: 4_944,
        }],
        member_final_fatigue: vec![0.0],
        member_maximum_fatigue: vec![0.0],
        total_elapsed_minutes: 4_944,
        total_movement_minutes: 1,
        truncated: false,
    };
    let returned = projected_itinerary_thermal_state(
        332_662,
        &return_camp,
        point,
        point,
        outbound.state,
        adventuresim_core::survival::MAX_CLOTHING_INSULATION_BPS,
        false,
    )
    .unwrap();
    assert!(!returned.safe);
}

#[test]
fn thermal_projection_uses_core_itinerary_movement_not_provisioning_reserve() {
    let movement = case_site_movement_minutes(1_250).unwrap();
    assert_eq!(movement, 60);
    assert_eq!(projected_case_site_journey_minutes(1_250, 480), Some(240));
    let members = [adventuresim_core::strategic_time::ItineraryMember {
        fatigue_capacity: 6_000.0,
        calories_used: 3_000.0,
        camp_schedule: Default::default(),
    }];
    let itinerary = adventuresim_core::strategic_time::forecast_itinerary(
        720,
        movement,
        480,
        false,
        adventuresim_core::strategic_time::CampDurationPolicy::Auto,
        &members,
    )
    .unwrap();
    assert_eq!(itinerary.total_movement_minutes, movement);
    assert!(itinerary.total_elapsed_minutes < 240);
    let source = LIVE_CORE_SOURCE;
    assert!(source.contains("projected_itinerary_thermal_safe"));
    assert!(source.contains("ItinerarySegmentKind::Camp"));
    assert!(source.contains("FieldShelter::Tent"));
}

#[test]
fn survival_report_schema_has_per_agent_aggregate_and_failure_context() {
    let metrics = serde_json::to_value(CoreLoopMetrics::default()).unwrap();
    for field in [
        "party_tents_purchased",
        "tent_field_rests",
        "tent_field_rest_failures",
        "ammunition_units_purchased",
        "ammunition_shortage_suppressions",
        "tent_provider_unavailable_bivouac_departures",
        "current_condition_readiness_suppressions",
        "route_weather_projection_unavailable_departures",
        "survival_observations",
        "max_party_carried_load_grams",
        "max_party_carry_capacity_grams",
        "min_party_encumbrance_remaining_bps",
        "max_observed_wetness_bps",
        "max_observed_abs_thermal_strain",
    ] {
        assert!(metrics.get(field).is_some(), "missing {field}");
    }
    let source = LIVE_CORE_SOURCE;
    let death = source
        .split("fn observe_deaths")
        .nth(1)
        .expect("death telemetry");
    for field in [
        "cause=",
        "source=",
        "source_id=",
        "strategic_minute=",
        "thermal=",
        "wetness_bps=",
        "ammo=",
        "carried_load_kg=",
        "equipment_ready=",
        "party_tent_quantity=",
    ] {
        assert!(death.contains(field), "missing death context {field}");
    }
    assert_eq!(CORE_LOOP_FAILURE_SCHEMA_VERSION, 9);
    assert_eq!(crate::FORMAT_VERSION, 9);
}
