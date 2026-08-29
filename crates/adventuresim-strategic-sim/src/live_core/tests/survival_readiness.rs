#[test]
fn departure_readiness_applies_continuous_load_thermal_and_ammo_floors() {
    assert_eq!(public_encumbrance_remaining_bps(0.0, 100.0), 10_000);
    assert_eq!(public_encumbrance_remaining_bps(80.0, 100.0), 2_000);
    assert_eq!(public_encumbrance_remaining_bps(81.0, 100.0), 1_900);
    assert!(survival_equipment_ready(
        DomainIncapacitationStatus::Ready,
        MAX_DEPARTURE_WETNESS_BPS,
        MAX_DEPARTURE_ABS_THERMAL_STRAIN as i32,
        true,
        RANGED_AMMUNITION_FLOOR,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
    assert!(!survival_equipment_ready(
        DomainIncapacitationStatus::Ready,
        MAX_DEPARTURE_WETNESS_BPS,
        0,
        true,
        RANGED_AMMUNITION_FLOOR - 1,
        MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS,
    ));
    assert!(!survival_equipment_ready(
        DomainIncapacitationStatus::Ready,
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
fn safe_departure_selects_a_journey_clock_without_synchronizing_characters() {
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
    assert!(source.contains("mode=journey_local_clock"));
    assert!(!source.contains("rest_at_settlement_hours_then(member_id, member_wait"));
    assert!(!source.contains("let actual_party_floor = self"));
    assert!(source.contains("CoreLoopEventKind::SafeDepartureWait"));
}

#[test]
fn safe_departure_configuration_does_not_rest_party_members() {
    let source = include_str!("../settlement.rs");
    let wait = source
        .split("pub(super) fn wait_for_safe_departure_at_settlement")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn configure_safe_departure_itinerary")
                .next()
        })
        .expect("safe-departure wait implementation");
    assert!(wait.contains("configure_safe_departure_itinerary"));
    assert!(wait.contains("journey_start_minute_of_day"));
    assert!(wait.contains("public_party_elapsed_max(&starting_party.id)"));
    assert!(!wait.contains("backend_character_times()"));
    assert!(wait.contains("mode=journey_local_clock"));
    assert!(!wait.contains("rest_at_settlement"));
    assert!(!wait.contains("let actual_party_floor"));
    assert!(!wait.contains("strategic_incident"));
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
fn generated_route_forecast_can_stage_a_week_ahead_but_public_waits_remain_daily() {
    assert_eq!(forecast_safe_departure_wait_minutes(1), Some(60));
    assert_eq!(forecast_safe_departure_wait_minutes(7_200), Some(7_200));
    assert_eq!(forecast_safe_departure_wait_minutes(10_080), Some(10_080));
    assert_eq!(forecast_safe_departure_wait_minutes(10_081), None);
}

#[test]
fn generated_route_search_finds_a_warmer_hour_inside_a_daily_walking_window() {
    let starting_minute = 600;
    let walking_minutes = 600;
    let daily_start = adventuresim_core::strategic_time::minutes_until_next_walking_start(
        starting_minute,
        walking_minutes,
        false,
    )
    .unwrap()
    .max(60);
    let waits = generated_safe_departure_waits(starting_minute, walking_minutes, false);
    let warmer_hour = waits
        .iter()
        .copied()
        .find(|wait| *wait > daily_start && *wait <= daily_start.saturating_add(180))
        .expect("hourly sample inside the first legal walking window");
    let mut evaluated_daily_start = false;
    let selected = select_generated_case_site_plan(
        walking_minutes,
        260,
        67,
        false,
        starting_minute,
        |candidate_walking_minutes, travel_at_night, wait| {
            if candidate_walking_minutes == walking_minutes
                && !travel_at_night
                && wait == daily_start
            {
                evaluated_daily_start = true;
            }
            (candidate_walking_minutes == walking_minutes
                && !travel_at_night
                && wait == warmer_hour)
                .then_some(wait)
        },
    );
    assert!(
        evaluated_daily_start,
        "the daily start was evaluated and unsafe"
    );
    assert_eq!(selected, Some(warmer_hour));
}

#[test]
fn delayed_route_recovery_is_real_bounded_and_revalidated_before_departure() {
    let survival = include_str!("../survival.rs");
    let departure = survival
        .split("pub(super) fn validate_case_site_thermal_readiness(")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn field_recovery_rest_thermal_safe")
                .next()
        })
        .expect("case-site departure planner");
    assert!(departure.contains("if candidate_wait == 0"));
    assert!(departure.contains("SurvivalState::default()"));
    assert!(departure.contains("calories_used: if candidate_wait == 0"));
    let generated = include_str!("../generated_cases.rs");
    let wait_follow_through = generated
        .split("self.wait_for_safe_departure_at_settlement(")
        .nth(1)
        .and_then(|tail| tail.split("DepartureReadiness::Deferred(reason)").next())
        .expect("bounded public settlement wait follow-through");
    assert!(wait_follow_through.contains("self.ensure_medically_safe(agent)?"));
    assert!(wait_follow_through.contains("self.validate_party_departure_readiness(party_id)"));
    assert!(wait_follow_through.contains("validate_case_site_thermal_readiness"));
    assert!(!generated.contains("generated_route_thermal_backoff"));
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
        .split("pub(super) fn validate_case_site_thermal_readiness(")
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
        joint_case_site_plan_failure_reason(3, 0, false, true, false),
        DepartureDeferralReason::RouteThermalUnsafeAllPublicWindows
    );
    assert_eq!(
        joint_case_site_plan_failure_reason(0, 0, true, false, false),
        DepartureDeferralReason::RouteWeatherProjectionUnavailable
    );
    assert_eq!(
        joint_case_site_plan_failure_reason(3, 1, false, true, false),
        DepartureDeferralReason::RouteFatigueRecoveryRequired
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
    assert!(generated.contains("ReducerOperation::GeneratedCaseSitePlannedRecovery"));
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
        .and_then(|tail| {
            tail.split("if let Some((action, reason, wait_minutes))")
                .next()
        })
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
fn generated_on_site_action_continues_only_with_same_site_full_return_reserve() {
    let generated = include_str!("../generated_cases.rs");
    let travel_selection_anchor = "if let Some(action) = actions.iter().find(|row| {\n                projected_investigation_action_state(&row.availability)\n                    == ProjectedInvestigationActionState::Travel";
    let attempted_action = generated
        .split("perform_investigation_action_then(")
        .nth(1)
        .and_then(|tail| tail.split(travel_selection_anchor).next())
        .expect("single generated action attempt and return boundary");
    let action_event = attempted_action
        .find("CoreLoopEventKind::GeneratedInvestigationAction")
        .expect("public action progress event");
    let settlement_continue = attempted_action
        .find("if at_settlement {")
        .expect("settlement action continuation branch");
    let occupied_site = attempted_action
        .find("let occupied_site_id")
        .expect("off-settlement occupancy boundary");
    let off_settlement_return = &attempted_action[occupied_site..];
    let return_safety = off_settlement_return
        .find("generated_action_return_thermal_decision(party_id, &return_pin, 0)")
        .expect("authoritative reserved-return safety recheck");
    let return_reducer = off_settlement_return
        .find("evacuate_generated_party_to_origin(")
        .expect("reserved return execution");
    let progress_exit = off_settlement_return
        .rfind("return Ok(true)")
        .expect("public-progress exit after reserved return");
    assert!(action_event < settlement_continue && settlement_continue < occupied_site);
    assert!(return_safety < return_reducer);
    assert!(return_reducer < progress_exit);
    assert!(attempted_action[settlement_continue..occupied_site].contains("continue;"));
    assert!(!off_settlement_return.contains("generated_actor_ready_after_time"));
    let continuation = off_settlement_return
        .find("plan=continue_same_site")
        .expect("conditional same-site continuation");
    let next_reserve = off_settlement_return[..continuation]
        .rfind("next_action.duration_max_minutes")
        .expect("next action and return reserve");
    assert!(
        off_settlement_return[..continuation]
            .contains("row.required_case_site_id.as_ref() == Some(&occupied_site_id)")
    );
    assert!(next_reserve < continuation);
    assert!(off_settlement_return[continuation..].contains("continue;"));
    assert!(off_settlement_return.contains("plan=one_attempt_then_reserved_return"));
    let evacuation = generated
        .split("fn evacuate_generated_party_to_origin")
        .nth(1)
        .and_then(|tail| tail.split("pub(super) fn advance_generated_case").next())
        .expect("reserved-return helper");
    assert!(evacuation.contains("generated_case_site_recoveries"));
    assert!(evacuation.contains("stored_case != case_id"));
}

#[test]
fn continued_on_site_loop_cannot_select_a_cross_site_action() {
    let generated = include_str!("../generated_cases.rs");
    let available_selection_anchor = "if let Some(action) = actions\n                .iter()\n                .find(|row| {\n                    projected_investigation_action_state(&row.availability)\n                        == ProjectedInvestigationActionState::Available";
    let travel_selection_anchor = "if let Some(action) = actions.iter().find(|row| {\n                projected_investigation_action_state(&row.availability)\n                    == ProjectedInvestigationActionState::Travel";
    let action_frontier = generated
        .split("sort_generated_actions(profile, &mut actions);")
        .nth(1)
        .and_then(|tail| tail.split(travel_selection_anchor).next())
        .expect("generated action selection frontier");
    let off_settlement = action_frontier
        .find("if !at_settlement {")
        .expect("off-settlement action-selection boundary");
    let occupied_site = action_frontier
        .find("let Some(occupied_site_id)")
        .expect("occupied site identity");
    let exact_site_filter = action_frontier
        .find("action.required_case_site_id.as_ref() == Some(&occupied_site_id)")
        .expect("exact occupied-site action filter");
    let ready_selection = action_frontier
        .find("let ready_action_index")
        .expect("thermally ready action selection");
    let available_selection = action_frontier
        .find(available_selection_anchor)
        .expect("available action selection");

    assert!(off_settlement < occupied_site);
    assert!(occupied_site < exact_site_filter);
    assert!(exact_site_filter < ready_selection);
    assert!(ready_selection < available_selection);

    let sorted_frontier = generated
        .find("sort_generated_actions(profile, &mut actions);")
        .expect("sorted generated action frontier");
    let full_available_selection = generated
        .find(available_selection_anchor)
        .expect("full available action selection");
    let travel_selection = generated
        .find(travel_selection_anchor)
        .expect("settlement-side travel selection");
    assert!(sorted_frontier < full_available_selection);
    assert!(full_available_selection < travel_selection);
}

#[test]
fn every_on_site_action_reserves_fatigue_and_thermal_capacity_before_execution() {
    let generated = include_str!("../generated_cases.rs");
    let available_selection_anchor = "if let Some(action) = actions\n                .iter()\n                .find(|row| {\n                    projected_investigation_action_state(&row.availability)\n                        == ProjectedInvestigationActionState::Available";
    let available = generated
        .split(available_selection_anchor)
        .nth(1)
        .expect("available generated action branch");
    let selection = generated
        .split("let ready_action_index = actions.iter().position")
        .nth(1)
        .and_then(|tail| tail.split(available_selection_anchor).next())
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
    assert!(on_site.contains("DomainIncapacitationStatus::Ready"));
    assert!(on_site.contains("let action_survivable = projected_action_survivable("));
    let model = include_str!("../model.rs");
    let action_cost = model
        .split("fn calories_after_strenuous_action")
        .nth(1)
        .and_then(|tail| tail.split("fn round_trip_walking_window_minutes").next())
        .expect("shared strenuous action cost");
    assert!(action_cost.contains("action_minutes as f32 / MINUTES_PER_DAY as f32"));
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
        .split("pub(super) fn validate_case_site_thermal_readiness(")
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
        .split("pub(super) fn validate_case_site_thermal_readiness(")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn field_recovery_rest_thermal_safe")
                .next()
        })
        .unwrap();
    assert!(route.contains("selected_case_site_plan = Some(selected_plan)"));
    assert!(route.contains("selected_plan.outbound.total_elapsed_minutes"));
    assert!(route.contains("selected_plan.returned.total_elapsed_minutes"));
    assert!(route.contains("selected_plan.departure_wait_minutes.min(MINUTES_PER_DAY)"));
    assert!(route.contains("DepartureDeferralReason::WaitTowardSafePublicRouteWindow"));
    assert!(route.contains("wait_minutes,"));
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
        .split("pub(super) fn validate_case_site_thermal_readiness(")
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
    assert!(departure.contains("DepartureDeferralReason::RouteWeatherProjectionUnavailable"));
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
    assert!(!load.contains("carried_water_ml"));
    assert!(load.contains("inventory_weight"));
    assert!(load.contains("public_contained_water_ml"));
    assert!(load.contains("public_measured_stack_weight_kg"));
    assert!(load.contains("public_row_is_carried"));
    assert!(!load.contains("body_weight_kg"));
}

#[test]
fn public_supply_and_load_accounting_include_nested_measured_objects() {
    let source = LIVE_CORE_SOURCE;
    assert!(!source.contains("location_kind"));
    assert!(!source.contains("location_owner"));
    assert!(!source.contains("pooled_water_ml"));
    for public_surface in [
        ".inventory_object()",
        ".inventory_containment()",
        ".inventory_item_amount()",
        ".party_item_amount()",
        ".container_liquid()",
    ] {
        assert!(source.contains(public_surface), "{public_surface}");
    }
    let supplies = source
        .split("fn expedition_supplies")
        .nth(1)
        .and_then(|tail| tail.split("fn emit_expedition_diagnostics").next())
        .expect("public expedition supply observation");
    assert!(supplies.contains("public_row_is_carried"));
    assert!(supplies.contains("contained_water_ml"));
    assert!(!supplies.contains("carried_water_ml"));
}

#[test]
fn generated_inventory_locations_require_exact_typed_custody_and_row() {
    let personal = InventoryLocation::Personal(PersonalInventoryLocation {
        character_id: 7,
        row_id: 11,
    });
    let personal_custody = OperationalCustody::character(7).unwrap();
    let other_character = OperationalCustody::character(8).unwrap();
    assert!(LiveRunner::public_inventory_location_matches_row(
        &personal,
        &personal_custody,
        11,
    ));
    assert!(!LiveRunner::public_inventory_location_matches_row(
        &personal,
        &personal_custody,
        12,
    ));
    assert!(!LiveRunner::public_inventory_location_matches_custody(
        &personal,
        &other_character,
    ));

    let party = InventoryLocation::Party(PartyInventoryLocation {
        party_id: "party:one".into(),
        row_id: 13,
    });
    let party_custody = OperationalCustody::party("party:one").unwrap();
    assert!(LiveRunner::public_inventory_location_matches_row(
        &party,
        &party_custody,
        13,
    ));
    assert!(!LiveRunner::public_inventory_location_matches_custody(
        &party,
        &personal_custody,
    ));

    let fireplace = InventoryLocation::Fireplace(FireplaceInventoryLocation {
        fixture_id: "fireplace:one".into(),
    });
    assert!(!LiveRunner::public_inventory_location_matches_custody(
        &fireplace,
        &party_custody,
    ));
}

#[test]
fn measured_inventory_amount_replaces_stack_quantity_at_canonical_scale() {
    use adventuresim_core::inventory_measurement::ConsumableFractionMicros;

    assert_eq!(public_effective_inventory_quantity(3, None), 3.0);
    assert_eq!(
        public_effective_inventory_quantity(3, Some(ConsumableFractionMicros::WHOLE.get())),
        1.0,
        "a measured row is one measured object, not quantity times its amount"
    );
    assert_eq!(
        public_effective_inventory_quantity(
            7,
            Some(ConsumableFractionMicros::whole_divided_by(2).get())
        ),
        0.5,
        "half of a measured object remains half regardless of its row quantity"
    );
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
    assert!(source.contains("shelter=bivouac"));
    assert!(tent.contains("tent_provider_unavailable_bivouac(event_agent)"));

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
fn coded_merchant_provider_races_reuse_public_unavailable_policy_without_purchase_metrics() {
    let coded = adventuresim_core::reducer_error::coded_reducer_error(
        ReducerErrorCode::MerchantProviderUnavailable,
        "wording is not part of the protocol",
    );
    for operation in [
        ReducerOperation::PurchasePartyTent,
        ReducerOperation::PurchaseJourneyProvisions,
        ReducerOperation::PurchaseAmmunition,
        ReducerOperation::PurchaseFirstAidMaterial,
        ReducerOperation::PurchasePersonalStorefrontWithPartyStake,
    ] {
        assert!(merchant_provider_unavailable_failure(
            &CoreLoopError::reducer_rejected(operation, coded.clone())
        ));
    }
    for (operation, detail) in [
        (
            ReducerOperation::PurchaseFromHerbalist,
            "Merchant service provider is not available",
        ),
        (
            ReducerOperation::PurchasePartyTent,
            "Merchant service provider is not available now",
        ),
        (
            ReducerOperation::PurchasePartyTent,
            "Merchant service provider is unavailable",
        ),
    ] {
        let error = CoreLoopError::reducer_rejected(operation, detail);
        assert!(!merchant_provider_unavailable_failure(&error), "{error}");
    }

    let source = LIVE_CORE_SOURCE;
    let tent = source
        .split("fn ensure_party_tent")
        .nth(1)
        .and_then(|tail| tail.split("fn ensure_ranged_ammunition").next())
        .unwrap();
    assert!(tent.contains("merchant_provider_unavailable_failure(&error)"));
    assert!(
        tent.find("merchant_provider_unavailable_failure(&error)")
            .unwrap()
            < tent.find("party_tents_purchased").unwrap()
    );

    let ammo = source
        .split("fn ensure_ranged_ammunition")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_party_departure_readiness").next())
        .unwrap();
    assert!(ammo.contains("DepartureDeferralReason::AmmunitionProviderProjectionUnavailable"));
    assert!(
        ammo.find("merchant_provider_unavailable_failure(&error)")
            .unwrap()
            < ammo.find("ammunition_purchases").unwrap()
    );

    let provisions = source
        .split("fn provision_case_site_journey")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn public_active_camp_observation")
                .next()
        })
        .unwrap();
    assert!(
        provisions.contains("TravelProvisionDeferralReason::PayerProviderProjectionUnavailable")
    );
    assert!(
        provisions
            .find("merchant_provider_unavailable_failure(&error)")
            .unwrap()
            < provisions.find("journey_provision_purchases").unwrap()
    );

    let first_aid = source
        .split("fn acquire_first_aid_material")
        .nth(1)
        .and_then(|tail| tail.split("fn apply_visible_first_aid").next())
        .unwrap();
    assert!(first_aid.contains("merchant_provider_unavailable_failure(&error)"));
    assert!(first_aid.contains("return Ok(false)"));

    let upgrade = source.split("fn try_upgrade").nth(1).unwrap();
    assert!(upgrade.contains("merchant_provider_unavailable_failure(&error)"));
    assert!(
        upgrade
            .find("merchant_provider_unavailable_failure(&error)")
            .unwrap()
            < upgrade.find("equipment_purchases += 1").unwrap()
    );

    let herbalist = source
        .split("debug_assert_eq!(choice, MedicalChoice::BuyAndRest)")
        .nth(1)
        .and_then(|tail| tail.split("administer_preparation_then").next())
        .unwrap();
    assert!(!herbalist.contains("merchant_provider_unavailable_failure"));
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
        visible_unique_default_provider(
            &[(7, 0, 720, false, false), (8, 720, 1_440, false, false)],
            300
        ),
        None
    );
    assert_eq!(
        visible_unique_default_provider(&[(7, 0, 720, false, false)], 300),
        Some(7)
    );
    assert_eq!(
        visible_unique_default_provider(&[(7, 0, 720, false, false)], 900),
        None
    );
    assert_eq!(
        visible_unique_default_provider(&[(7, 0, 720, true, false)], 300),
        None
    );
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
    let point = PublicRoutePoint::from_degrees(53.0, 10.0, 0).unwrap();
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
    let point = PublicRoutePoint::from_degrees(0.0, 0.0, 0).unwrap();
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
        "one helper plus generated initial and direct case-site gates"
    );
    assert!(source.contains("validate_case_site_thermal_readiness_at("));
    let generated = include_str!("../generated_cases.rs");
    assert_eq!(
        generated
            .matches("validate_case_site_thermal_readiness(")
            .count(),
        1,
        "generated routes gate once at the observed current clock"
    );
    assert_eq!(
        generated
            .matches("validate_case_site_thermal_readiness_at(")
            .count(),
        1,
        "generated routes revalidate once at the configured future start"
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
    let point = PublicRoutePoint::from_degrees(53.0, 10.0, 0).unwrap();
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
    let itinerary =
        adventuresim_core::strategic_time::forecast_itinerary(720, movement, 480, false, &members)
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
fn projected_route_thermal_safe(
    starting_minute: u64,
    elapsed_minutes: u64,
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
) -> Option<bool> {
    let itinerary = adventuresim_core::strategic_time::ItineraryForecast {
        segments: vec![adventuresim_core::strategic_time::ItinerarySegment {
            kind: adventuresim_core::strategic_time::ItinerarySegmentKind::Walking,
            elapsed_start: 0,
            elapsed_minutes,
            movement_start: 0,
            movement_minutes: elapsed_minutes,
            average_fatigue_start: 0.0,
            average_fatigue_end: 0.0,
            maximum_fatigue_end: 0.0,
            required_rest_minutes: 0,
        }],
        member_final_fatigue: vec![0.0],
        member_maximum_fatigue: vec![0.0],
        total_elapsed_minutes: elapsed_minutes,
        total_movement_minutes: elapsed_minutes,
        truncated: false,
    };
    projected_itinerary_thermal_safe(
        starting_minute,
        &itinerary,
        origin,
        destination,
        starting_state,
        insulation_bps,
        false,
    )
}
#[test]
fn specialist_repair_services_only_route_matching_item_kinds() {
    let services = [SettlementService::Weaponsmith];
    assert_eq!(
        settlement::repair_service_for_kind(&services, PersistedItemKind::Weapon),
        Some("weapons")
    );
    assert_eq!(
        settlement::repair_service_for_kind(&services, PersistedItemKind::Armor),
        None
    );
    assert_eq!(
        settlement::repair_service_for_kind(&services, PersistedItemKind::Clothing),
        None
    );
}

#[test]
fn general_blacksmith_routes_every_repairable_item_kind() {
    let services = [SettlementService::GeneralBlacksmith];
    assert_eq!(
        settlement::repair_service_for_kind(&services, PersistedItemKind::Weapon),
        Some("weapons")
    );
    assert_eq!(
        settlement::repair_service_for_kind(&services, PersistedItemKind::Armor),
        Some("armor")
    );
    assert_eq!(
        settlement::repair_service_for_kind(&services, PersistedItemKind::Clothing),
        Some("clothing")
    );
}
