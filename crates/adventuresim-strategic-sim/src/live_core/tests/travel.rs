#[test]
fn all_nonterminal_encounters_follow_authoritative_public_post_state() {
    let resumable = PublicPostEncounterJourneyState {
        unresolved_encounter: false,
        active_destination: true,
        journey_count: 1,
        itinerary_count: 1,
        destination_matches: true,
        active_interval_count: 0,
        actionable_actor: true,
        unsafe_member_count: 0,
        evacuation: false,
    };
    for resolved_choice in ["sneak", "surrender", "attack", "detour", "run"] {
        assert_eq!(
            classify_post_encounter_journey(resumable),
            Ok(PostEncounterJourneyAction::ContinueTravel),
            "{resolved_choice}"
        );
    }
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            active_interval_count: 1,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::HandleActiveCamp)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            unsafe_member_count: 1,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::HoldForRecovery)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            unsafe_member_count: 1,
            evacuation: true,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::ContinueTravel)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            active_interval_count: 1,
            unsafe_member_count: 1,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::HoldForRecovery)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            active_interval_count: 1,
            unsafe_member_count: 1,
            evacuation: true,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::HandleActiveCamp)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            actionable_actor: false,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::HoldNoActionableActor)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            active_destination: false,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::ReclassifyPublicState)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            unresolved_encounter: true,
            ..resumable
        }),
        Ok(PostEncounterJourneyAction::ReclassifyPublicState)
    );
    assert_eq!(
        classify_post_encounter_journey(PublicPostEncounterJourneyState {
            active_interval_count: 2,
            ..resumable
        }),
        Err("post_encounter_overlapping_active_camps")
    );

    let source = LIVE_CORE_SOURCE;
    let travel = source
        .split("fn travel_camps")
        .nth(1)
        .and_then(|tail| tail.split("fn continue_public_active_journey").next())
        .expect("travel camp driver");
    let encounter = travel
        .split("if let Some(encounter) = pending_encounter")
        .nth(1)
        .and_then(|tail| tail.split("let camp = self").next())
        .expect("encounter branch");
    let resolve = encounter
        .find(".resolve_strategic_encounter_then(")
        .expect("encounter resolution");
    let projected_recheck = encounter
        .find("public_post_encounter_journey_action")
        .expect("public journey recheck");
    let continuation = encounter
        .find(".continue_camp_travel_then(")
        .expect("journey continuation");
    assert!(resolve < projected_recheck && projected_recheck < continuation);
    assert_eq!(
        encounter
            .matches(".resolve_strategic_encounter_then(")
            .count(),
        1
    );
    assert_eq!(encounter.matches(".continue_camp_travel_then(").count(), 1);
    assert!(encounter.contains("let encounter_revision = encounter.revision"));
    assert!(encounter.contains("strategic-sim:{encounter_id}:{encounter_revision}:{choice}"));
    assert!(!encounter.contains("self.metrics.camp_stops"));
    assert!(!encounter.contains("purchase_journey_provisions"));
    for observation in [
        "leg_members_before",
        "leg_supplies_before",
        "leg_members_after",
        "leg_supplies_after",
        "self.observe_deaths()",
        "self.expedition_recovery_actor(party_id)",
        "self.unsafe_party_agents",
        "PostEncounterJourneyAction::HandleActiveCamp",
        "PostEncounterJourneyAction::HoldForRecovery",
    ] {
        assert!(encounter.contains(observation), "{observation}");
    }
    assert!(
        travel
            .find(".continue_camp_travel_then(")
            .expect("post-encounter continuation")
            < travel
                .find("public_active_camp_observation(party_id)")
                .expect("camp observation")
    );
    let resume_projection = source
        .split("fn public_post_encounter_journey_action")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn party_has_unresolved_public_encounter")
                .next()
        })
        .expect("post-encounter public projection check");
    for public_state in [
        "party_has_unresolved_public_encounter",
        "party_by_id",
        ".party_journey()",
        ".party_journey_itinerary()",
        "projected_active_camp_interval_count",
        "classify_post_encounter_journey",
    ] {
        assert!(resume_projection.contains(public_state), "{public_state}");
    }
}

#[test]
fn camp_coherence_diagnostic_distinguishes_missing_and_overlapping_intervals() {
    let single = JourneyCampInterval {
        movement_minute: 480,
        elapsed_start_minute: 480,
        elapsed_minutes: 960,
        average_fatigue_start: 0.5,
        average_fatigue_end: 0.0,
        maximum_fatigue_end: 0.0,
    };
    assert_eq!(
        projected_active_camp_interval_count(1_500, 2_500, std::slice::from_ref(&single)),
        0
    );
    let overlapping = JourneyCampInterval {
        movement_minute: 500,
        elapsed_start_minute: 500,
        elapsed_minutes: 120,
        ..single.clone()
    };
    assert_eq!(
        projected_active_camp_interval_count(500, 2_500, &[single, overlapping]),
        2
    );
    assert_eq!(
        bounded_public_journey_diagnostic(u64::MAX),
        MAX_PUBLIC_JOURNEY_DIAGNOSTIC_MINUTES
    );
    assert_eq!(
        bounded_public_forecast_count(usize::MAX),
        MAX_PUBLIC_JOURNEY_DIAGNOSTIC_INTERVALS
    );

    let source = LIVE_CORE_SOURCE;
    let diagnostic = source
        .split("fn public_camp_coherence_error")
        .nth(1)
        .and_then(|tail| tail.split("fn public_post_encounter_journey_action").next())
        .expect("public camp coherence diagnostic");
    for field in [
        "active_interval_count=",
        "completed_elapsed=",
        "total_elapsed=",
        "forecast_count=",
        "journey_count=",
        "itinerary_count=",
    ] {
        assert!(diagnostic.contains(field), "{field}");
    }
    assert!(diagnostic.contains("bounded_public_journey_diagnostic"));
    assert!(diagnostic.contains("bounded_public_forecast_count"));
    assert_eq!(
        safe_failure_operation(
            "travel_camps failed: journey camp projection is incoherent: active_interval_count=0"
        ),
        Some("travel_camps")
    );
}

#[test]
fn travel_driver_uses_public_itinerary_and_observer_safe_provisioning() {
    let source = LIVE_CORE_SOURCE;
    let travel = source
        .split("fn travel_camps")
        .nth(1)
        .and_then(|tail| tail.split("fn choose_quest").next())
        .expect("travel camp driver");
    assert!(travel.contains("public_active_camp_observation(party_id)"));
    assert!(travel.contains("row.party_id == party_id"));
    assert!(!travel.contains("projected_camp_rest_minutes("));
    let coherent_camp = source
        .split("fn public_active_camp_observation")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn party_has_unresolved_public_encounter")
                .next()
        })
        .expect("shared coherent public camp helper");
    for public_projection in [".party()", ".party_journey()", ".party_journey_itinerary()"] {
        assert!(
            coherent_camp.contains(public_projection),
            "{public_projection}"
        );
    }
    assert!(coherent_camp.contains("let [journey] = journeys.as_slice()"));
    assert!(coherent_camp.contains("let [itinerary] = itineraries.as_slice()"));
    assert!(coherent_camp.contains("&journey.destination != camp_destination"));
    assert!(
        coherent_camp
            .contains("journey.completed_elapsed_minutes >= journey.total_elapsed_minutes")
    );
    assert!(coherent_camp.contains("&itinerary.forecast_camp_intervals"));
    assert!(coherent_camp.contains("projected_camp_rest_minutes("));
    assert!(
        travel
            .contains("let Some((travel_actor, _, _)) = self.expedition_recovery_actor(party_id)")
    );
    assert!(travel.contains(".rest_at_camp_then(travel_actor, rest_minutes"));
    assert!(!travel.contains(".rest_at_camp_then(travel_actor, 1_440"));
    assert!(source.contains(
        "fn travel_camps(&mut self, party_id: &str) -> Result<JourneyTravelOutcome, String>"
    ));
    for outcome in [
        "JourneyTravelOutcome::Completed",
        "JourneyTravelOutcome::HeldNoActionableActor",
        "JourneyTravelOutcome::HeldForRecovery",
    ] {
        assert!(source.contains(outcome), "{outcome}");
    }
    for hold in [
        "\"journey_stalled\"",
        "\"journey_stalled_after_encounter\"",
        "\"journey_stalled_after_rest\"",
    ] {
        assert!(travel.contains(hold), "{hold}");
    }
    assert!(
        !travel.contains("return Err(\"journey has no ready, asymptomatic, noncritical actor\"")
    );
    assert!(
        !travel.contains("return Err(\"camp rest left no ready, asymptomatic, noncritical actor\"")
    );
    for phase in ["phase=pre_rest", "phase=post_rest", "phase=post_continue"] {
        assert!(travel.contains(phase));
    }

    let recovery_actor = source
        .split("fn expedition_recovery_actor")
        .nth(1)
        .and_then(|tail| tail.split("fn public_expedition_return_settlement").next())
        .expect("public recovery actor selection");
    assert!(recovery_actor.contains("expedition_member_observations(party_id)"));
    assert!(recovery_actor.contains("member.condition_status == \"ready\""));
    assert!(recovery_actor.contains("!member.symptomatic"));
    assert!(recovery_actor.contains("!member.critical"));
    assert!(recovery_actor.contains("ready.sort_by_key"));
    assert!(recovery_actor.contains("ready.into_iter().next()"));
    assert!(!recovery_actor.contains("infection_episode"));

    let provisioning = source
        .split("fn provision_case_site_journey")
        .nth(1)
        .and_then(|tail| tail.split("fn travel_camps").next())
        .expect("journey provisioner");
    for public_surface in [
        ".backend_character_needs()",
        ".inventory_item()",
        ".party_inventory_item()",
        ".food_lot()",
        ".settlement()",
        ".backend_settlement_residents()",
        ".settlement_resident_presence()",
        ".backend_local_problem_trade_effects()",
        ".party_stake()",
        "SettlementService::Market | SettlementService::GeneralStore",
        "finalize_merchant_trade_then(",
    ] {
        assert!(provisioning.contains(public_surface), "{public_surface}");
    }
    assert!(provisioning.contains("target_surplus_days: TRAVEL_PROVISION_RESERVE_DAYS"));
    assert!(provisioning.contains("payer_options"));
    assert!(provisioning.contains("payer_minute"));
    assert!(provisioning.contains("merchant_count != 1"));
    assert!(provisioning.contains("journey_finance_backoff"));
    assert!(provisioning.contains("(party_id.to_owned(), leader, finance_key.to_owned())"));
    assert!(!provisioning.contains(".map_or(0, |row| row.buy_bps)"));
    assert!(!provisioning.contains("party_journey_route"));
}

#[test]
fn journey_holds_are_publicly_diagnosable_and_block_arrival_assumptions() {
    let source = LIVE_CORE_SOURCE;
    let hold = source
        .split("fn record_journey_hold")
        .nth(1)
        .and_then(|tail| tail.split("fn expedition_recovery_actor").next())
        .expect("journey hold diagnostics");
    for public_field in [
        "reason={}",
        "journey_completed_elapsed=",
        "journey_total_elapsed=",
        "journey_remaining_elapsed=",
        "journey_destination=",
        "camp_remaining_minutes=",
        "active_forecast_interval_start=",
        "active_forecast_interval_minutes=",
        "living_count=",
        "one_day_food_kcal_required=",
        "stored_food_kcal=",
        "one_day_water_ml_required=",
        "portable_water_ml=",
        "supplies_cover_one_rest_day=",
    ] {
        assert!(hold.contains(public_field), "{public_field}");
    }
    assert!(hold.contains("bounded_event_field(reason)"));
    assert!(hold.contains(".party_journey()"));
    assert!(hold.contains(".party_journey_itinerary()"));
    assert!(!hold.contains("infection_episode"));
    assert!(!hold.contains("disease"));

    let generated = source
        .split("fn advance_generated_case")
        .nth(1)
        .and_then(|tail| tail.split("fn cycle").next())
        .expect("generated case driver");
    let travel_guard = generated
        .find("journey_outcome != JourneyTravelOutcome::Completed")
        .expect("typed generated travel guard");
    let traveled_marker = generated
        .find("self.generated_traveled_cases.insert(funnel_key)")
        .expect("generated traveled marker");
    assert!(travel_guard < traveled_marker);

    let recovery = source
        .split("fn recover_or_evacuate_off_settlement")
        .nth(1)
        .and_then(|tail| tail.split("fn generated_case_status").next())
        .expect("expedition recovery driver");
    assert!(recovery.contains("\"recovery_plan\""));
    assert!(recovery.contains("\"evacuation_plan\""));
    assert!(recovery.contains("self.travel_camps(party_id)? != JourneyTravelOutcome::Completed"));
    assert!(recovery.contains("return Ok(ExpeditionRecoveryOutcome::Held)"));
}

#[test]
fn direct_contract_provisions_before_acceptance_and_never_defers_by_abandoning() {
    let source = LIVE_CORE_SOURCE;
    let quest = source
        .split("fn cycle")
        .nth(1)
        .and_then(|tail| tail.split("fn try_upgrade").next())
        .expect("direct contract driver");
    assert!(
        quest.find("provision_case_site_journey").unwrap()
            < quest.find("accept_contract_then").unwrap()
    );
    assert!(!quest.contains("defer_unprovisioned_contract"));
    assert!(quest.contains("failed_expedition_cannot_reprovision"));
    assert!(quest.contains(".min_by_key(|site| (site.distance_m, site.case_site_id.clone()))"));
    assert!(quest.matches("provision_case_site_journey").count() >= 2);
    assert!(quest.contains("accepted contract provisioning projection changed after disclosure"));
    assert!(quest.contains("refreshed_safe_party_for_owner(party_id, quest_owner)"));
}

#[test]
fn recovery_audit_separates_public_before_and_after_observations() {
    let source = LIVE_CORE_SOURCE;
    let recovery = source
        .split("fn ensure_medically_safe")
        .nth(1)
        .and_then(|tail| tail.split("fn settlement_activity_day").next())
        .expect("medical recovery driver");
    assert!(recovery.contains("let symptomatic_after ="));
    assert!(recovery.contains("recovery_context=public_symptoms"));
    assert!(recovery.contains("symptomatic_before={symptomatic}"));
    assert!(recovery.contains("symptomatic_after={symptomatic_after}"));
    assert!(!recovery.contains("cause=public_symptomatic_illness"));
}
