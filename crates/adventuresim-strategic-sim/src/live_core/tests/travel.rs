#[test]
fn all_nonterminal_encounters_follow_authoritative_public_post_state() {
    let resumable = PublicPostEncounterJourneyState {
        unresolved_encounter: false,
        active_destination: true,
        journey_count: 1,
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
        .and_then(|tail| tail.split("if self.public_journey_camp_state").next())
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
        "projected_active_camp_interval_count",
        "classify_post_encounter_journey",
    ] {
        assert!(resume_projection.contains(public_state), "{public_state}");
    }
}

#[test]
fn narrative_encounter_policy_uses_safe_meaningful_choices_and_keeps_ignore_fallback() {
    let mut profile = generate_profile(42, 0);
    profile.personality = adventuresim_core::personality::Personality::neutral();
    let presentation = adventuresim_core::road_encounter_catalog::EncounterPresentation {
        cast: Vec::new(),
        opening: Vec::new(),
        choices: vec![
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "expose_charter".into(),
                label: "Attempt a difficult checked action".into(),
                available: true,
            },
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "challenge_to_arms".into(),
                label: "Start a fight".into(),
                available: true,
            },
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "ignore".into(),
                label: "Continue on the road".into(),
                available: true,
            },
        ],
        response: Vec::new(),
    };
    let json = serde_json::to_string(&presentation).unwrap();
    assert_eq!(
        select_public_narrative_encounter_choice(&json, &profile)
            .unwrap()
            .map(|choice| choice.choice),
        Some("ignore".into())
    );

    let unavailable_ignore = adventuresim_core::road_encounter_catalog::EncounterPresentation {
        cast: Vec::new(),
        opening: Vec::new(),
        choices: vec![
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "expose_charter".into(),
                label: "Attempt a difficult checked action".into(),
                available: true,
            },
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "ignore".into(),
                label: "Continue on the road".into(),
                available: false,
            },
        ],
        response: Vec::new(),
    };
    assert_eq!(
        select_public_narrative_encounter_choice(
            &serde_json::to_string(&unavailable_ignore).unwrap(),
            &profile,
        )
        .unwrap(),
        None
    );
    let missing_ignore = adventuresim_core::road_encounter_catalog::EncounterPresentation {
        cast: Vec::new(),
        opening: Vec::new(),
        choices: vec![
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "expose_charter".into(),
                label: "Attempt a difficult checked action".into(),
                available: true,
            },
        ],
        response: Vec::new(),
    };
    assert_eq!(
        select_public_narrative_encounter_choice(
            &serde_json::to_string(&missing_ignore).unwrap(),
            &profile,
        )
        .unwrap(),
        None
    );
    assert!(select_public_narrative_encounter_choice("not-json", &profile).is_err());

    let available_barter = adventuresim_core::road_encounter_catalog::EncounterPresentation {
        cast: Vec::new(),
        opening: Vec::new(),
        choices: vec![
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "barter_rations".into(),
                label: "Barter rations".into(),
                available: true,
            },
            adventuresim_core::road_encounter_catalog::PresentationChoice {
                id: "ignore".into(),
                label: "Continue on the road".into(),
                available: true,
            },
        ],
        response: Vec::new(),
    };
    let selected = select_public_narrative_encounter_choice(
        &serde_json::to_string(&available_barter).unwrap(),
        &profile,
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.choice, "ignore");
    assert_eq!(
        selected.reason,
        "unconditional_check_free_noncombat_fallback"
    );
    assert!(selected.eligible_meaningful_alternatives.is_empty());
    assert_eq!(
        selected.visible_alternatives,
        vec!["barter_rations", "ignore"]
    );

    let selector = LIVE_CORE_SOURCE
        .split("fn select_public_narrative_encounter_choice")
        .nth(1)
        .and_then(|tail| tail.split("struct PublicCombatFingerprint").next())
        .expect("public narrative selector");
    assert!(selector.contains("definitions()"));
    assert!(selector.contains("!choice.checks.is_empty()"));
    assert!(selector.contains("EncounterTransition::StartCombat"));
}

#[test]
fn authored_narrative_ignore_choices_are_unconditionally_resolvable() {
    let definitions = adventuresim_core::road_encounter_catalog::definitions();
    assert!(!definitions.is_empty());
    for definition in definitions {
        let ignores = definition
            .choices
            .iter()
            .filter(|choice| choice.id == "ignore")
            .collect::<Vec<_>>();
        assert_eq!(ignores.len(), 1, "{}", definition.id);
        let ignore = ignores[0];
        assert!(ignore.requirements.is_empty(), "{}", definition.id);
        assert!(ignore.checks.is_empty(), "{}", definition.id);
        let publicly_available_by_construction = ignore.requirements.is_empty();
        assert!(publicly_available_by_construction, "{}", definition.id);
    }
}

#[test]
fn post_rest_progress_accepts_only_explained_boundaries() {
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_469, 2_000, false, false),
        Ok(PostRestProgress::Exact {
            actual_rest_minutes: 469,
        })
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_360, 2_000, true, false),
        Ok(PostRestProgress::InterruptedShort {
            actual_rest_minutes: 360,
        })
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_360, 2_000, false, false),
        Err("post_rest_short_without_interruption")
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_000, 2_000, true, false),
        Err("post_rest_zero_progress")
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_470, 2_000, true, true),
        Err("post_rest_overshot_request")
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 2_001, 2_000, true, true),
        Err("post_rest_completed_after_total")
    );
}

#[test]
fn terminal_member_transitions_reclassify_short_and_zero_rest() {
    let companion_death =
        public_alive_to_dead_ids(&[(10, true), (20, true)], &[(10, true), (20, false)]);
    assert_eq!(companion_death, vec![20]);
    assert_eq!(
        public_terminal_rest_elapsed(
            &companion_death,
            &[(10, 1_000), (20, 1_000)],
            &[(10, 1_360), (20, 1_360)],
        ),
        Some(360)
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_360, 2_000, false, true),
        Ok(PostRestProgress::TerminalBoundary {
            actual_rest_minutes: 360,
        })
    );

    let leader_death_with_successor =
        public_alive_to_dead_ids(&[(10, true), (20, true)], &[(10, false), (20, true)]);
    assert_eq!(leader_death_with_successor, vec![10]);
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_469, 2_000, false, true),
        Ok(PostRestProgress::TerminalBoundary {
            actual_rest_minutes: 469,
        })
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_000, 2_000, false, true),
        Ok(PostRestProgress::TerminalBoundary {
            actual_rest_minutes: 0,
        })
    );

    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_360, 2_000, false, false),
        Err("post_rest_short_without_interruption")
    );
    assert_eq!(
        classify_post_rest_progress(1_000, 469, 1_000, 2_000, false, false),
        Err("post_rest_zero_progress")
    );
}

#[test]
fn travel_subscribes_resolves_and_rechecks_public_narrative_interruptions() {
    let source = LIVE_CORE_SOURCE;
    let subscription = source
        .split("fn run_core_loop_inner")
        .nth(1)
        .and_then(|tail| tail.split("connection.run_threaded()").next())
        .expect("live subscription");
    assert!(subscription.contains("query.from.backend_road_challenges()"));

    let travel = source
        .split("fn travel_camps")
        .nth(1)
        .and_then(|tail| tail.split("fn continue_public_active_journey").next())
        .expect("travel camp driver");
    let narrative = travel
        .find("active_public_narrative_challenge(party.leader_id)")
        .expect("loop-top narrative interruption");
    let combat = travel
        .find("let pending_encounter")
        .expect("combat interruption");
    assert!(narrative < combat);
    assert!(travel.contains(".resolve_errantry_road_challenge_then("));
    assert!(travel.contains("narrative_encounter_has_no_available_public_choice"));
    assert!(travel.contains("continue;"));

    let post_rest = travel
        .split("let after_rest_party")
        .nth(1)
        .and_then(|tail| tail.split("phase=post_continue").next())
        .expect("post-rest continuation boundary");
    let completed_camp = post_rest
        .find("self.metrics.camp_stops")
        .expect("completed camp accounting");
    let interruption = post_rest
        .find("let interrupted")
        .expect("post-rest public interruption recheck");
    let strict_progress = post_rest
        .find("classify_post_rest_progress")
        .expect("strict post-rest progress classification");
    let continuation = post_rest
        .find(".continue_camp_travel_then(")
        .expect("post-rest continuation");
    for baseline_coherence in [
        "after_rest_journeys.as_slice()",
        "after_rest_party.camp_destination.as_ref()",
        "&after_rest_journey.destination == after_rest_destination",
        "after_rest_journey.total_elapsed_minutes",
    ] {
        assert!(
            post_rest.contains(baseline_coherence),
            "{baseline_coherence}"
        );
    }
    assert!(interruption < strict_progress);
    assert!(strict_progress < completed_camp && completed_camp < continuation);
    assert!(post_rest.contains("requested_rest_minutes={rest_minutes}"));
    assert!(post_rest.contains("actual_rest_minutes={actual_rest_minutes}"));
    assert!(post_rest.contains("terminal_state_change={terminal_state_change}"));
    assert!(post_rest.contains("leader_before={leader_before_rest}"));
    assert!(post_rest.contains("leader_after={}"));

    let terminal_reclassification = post_rest
        .rfind("if terminal_state_change")
        .expect("terminal reclassification branch");
    assert!(terminal_reclassification < continuation);
    let all_dead_teardown = post_rest
        .find("(None, []) if terminal_state_change")
        .expect("all-dead public teardown projection");
    let all_dead_hold = post_rest
        .find("journey_stalled_after_terminal_rest")
        .expect("all-dead terminal hold");
    assert!(all_dead_teardown < strict_progress && strict_progress < all_dead_hold);

    let classifier = source
        .split("fn classify_post_rest_progress")
        .nth(1)
        .and_then(|tail| tail.split("fn classify_public_journey_camp_state").next())
        .expect("post-rest progress classifier");
    let interrupted_short = classifier
        .find("if interrupted")
        .expect("interrupted short-rest branch");
    let unexplained_short = classifier
        .find("post_rest_short_without_interruption")
        .expect("strict unexplained-short failure");
    assert!(interrupted_short < unexplained_short);
}

#[test]
fn between_camp_movement_is_validated_and_continues_until_a_recovery_boundary() {
    assert_eq!(
        classify_public_journey_camp_state(0),
        Ok(PublicJourneyCampState::BetweenCamps)
    );
    assert_eq!(
        classify_public_journey_camp_state(1),
        Ok(PublicJourneyCampState::ActiveCamp)
    );
    assert_eq!(
        classify_public_journey_camp_state(2),
        Err("overlapping_active_public_camps")
    );

    let source = LIVE_CORE_SOURCE;
    let projection = source
        .split("fn public_journey_camp_state")
        .nth(1)
        .and_then(|tail| tail.split("fn public_camp_coherence_error").next())
        .expect("public journey camp state");
    for fail_closed_boundary in [
        "let [journey] = journeys.as_slice()",
        "&journey.destination != destination",
        "journey.completed_elapsed_minutes >= journey.total_elapsed_minutes",
        "party.camp_remaining_minutes == 0",
        "&journey.forecast_camp_intervals",
    ] {
        assert!(
            projection.contains(fail_closed_boundary),
            "{fail_closed_boundary}"
        );
    }
    assert!(projection.contains("classify_public_journey_camp_state("));

    let travel = source
        .split("fn travel_camps")
        .nth(1)
        .and_then(|tail| tail.split("fn continue_public_active_journey").next())
        .expect("travel camp driver");
    let between = travel
        .split("== PublicJourneyCampState::BetweenCamps")
        .nth(1)
        .and_then(|tail| tail.split("let camp = self").next())
        .expect("between-camps continuation");
    assert!(between.contains(".continue_camp_travel_then("));
    assert!(between.contains("continue_between_forecast_camps"));
    assert!(between.contains("continue;"));
    assert!(travel.contains("for _ in 0..MAX_CAMPS_PER_LEG"));

    let recovery = source
        .split("fn recover_or_evacuate_off_settlement")
        .nth(1)
        .and_then(|tail| tail.split("fn generated_case_status").next())
        .expect("off-settlement recovery");
    assert!(
        recovery
            .contains("let can_attempt_field_recovery = (camp_state.is_some() || at_case_site)")
    );
    assert!(recovery.contains("let at_case_site = party.current_case_site_id.is_some()"));
    assert!(recovery.contains("self.expedition_recovery_rest_actor(party_id)"));
    assert!(recovery.contains("self.perform_expedition_recovery_rest(rest_actor)"));
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
    ] {
        assert!(diagnostic.contains(field), "{field}");
    }
    assert!(diagnostic.contains("bounded_public_journey_diagnostic"));
    assert!(diagnostic.contains("bounded_public_forecast_count"));
    assert_eq!(
        safe_failure_operation(
            "travel_camps failed: journey camp projection is incoherent: active_interval_count=0"
        ),
        Some(FailureOperation::TravelCamps)
    );
}

#[test]
fn travel_driver_uses_public_journey_and_observer_safe_provisioning() {
    let source = LIVE_CORE_SOURCE;
    let contributions = source
        .split("fn contribute_party_journey_currency")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn public_party_matchup_assessment")
                .next()
        })
        .expect("party journey contribution policy");
    assert!(contributions.contains("DomainIncapacitationStatus::Ready"));
    assert!(contributions.contains("!row.symptomatic && !row.critical"));
    assert!(contributions.contains("observable_medical_reserve"));
    assert!(contributions.contains("deposit_party_inventory_item_then"));
    assert!(!contributions.contains("withdraw_party_inventory_item_then"));
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
    for public_projection in [".party()", ".party_journey()"] {
        assert!(
            coherent_camp.contains(public_projection),
            "{public_projection}"
        );
    }
    assert!(coherent_camp.contains("let [journey] = journeys.as_slice()"));
    assert!(coherent_camp.contains("&journey.destination != camp_destination"));
    assert!(
        coherent_camp
            .contains("journey.completed_elapsed_minutes >= journey.total_elapsed_minutes")
    );
    assert!(coherent_camp.contains("&journey.forecast_camp_intervals"));
    assert!(coherent_camp.contains("projected_camp_rest_minutes("));
    assert!(travel.contains("let Some((travel_actor, travel_agent, _)) ="));
    assert!(travel.contains("self.expedition_recovery_actor(party_id)"));
    assert!(travel.contains("CoreLoopEventKind::Travel"));
    assert!(travel.contains("travel_agent,"));
    assert!(travel.contains("rest_at_camp_with_party_shelter"));
    assert!(!travel.contains(
        "rest_at_camp_with_party_shelter(\n                travel_actor,\n                MINUTES_PER_DAY"
    ));
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
    assert!(recovery_actor.contains("Some(DomainIncapacitationStatus::Ready)"));
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
    assert!(provisioning.contains("party_personal_funds"));
    assert!(provisioning.contains("contribute_party_journey_currency"));
    assert!(provisioning.contains("funded_party_coin < upper_bound_cost"));
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
    assert!(!hold.contains("infection_episode"));
    assert!(!hold.contains("disease"));

    let generated = source
        .split("fn advance_generated_case_inner")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn turn_in_ready_direct_contract")
                .next()
        })
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
fn direct_contract_provisions_before_acceptance_then_abandons_after_one_defeat() {
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
    assert!(quest.contains(".min_by_key(|site| (site.distance_m, site.case_site_id.clone()))"));
    assert!(quest.matches("provision_case_site_journey").count() >= 2);
    assert!(quest.contains("accepted contract provisioning projection changed after disclosure"));
    assert!(quest.contains("refreshed_safe_party_for_owner(party_id, quest_owner)"));
    assert_eq!(quest.matches("autoresolve_mission_then").count(), 1);
    assert!(!quest.contains("retry_travel_to_case_site"));
    assert!(quest.contains("abandon_defeated_quest"));
    assert!(quest.contains("reason=unchanged_defeated_threat"));
}

#[test]
fn direct_contract_acceptance_revalidates_the_publicly_present_issuer_before_spending_and_interaction()
 {
    let source = LIVE_CORE_SOURCE;
    let issuer = source
        .split("fn public_contract_issuer_available")
        .nth(1)
        .and_then(|tail| tail.split("fn defer_unavailable_contract_issuer").next())
        .expect("public contract issuer availability policy");
    assert!(issuer.contains("visible_npc_candidates(character_id, None, None)"));
    assert!(
        issuer.contains("candidate.resident_character_id == quest.issuer_resident_character_id")
    );

    let deferral = source
        .split("fn defer_unavailable_contract_issuer")
        .nth(1)
        .and_then(|tail| tail.split("fn abandon_unsafe_active_contract").next())
        .expect("contract issuer deferral policy");
    assert!(deferral.contains("reason=contract_issuer_unavailable"));
    assert!(deferral.contains("provenance"));
    assert!(deferral.contains("bounded_event_field(&quest.id)"));
    assert!(deferral.contains("current_settlement_id"));
    assert!(deferral.contains("Some(quest.settlement_id.as_str())"));
    assert!(deferral.contains("settlement_activity_day(leader_agent)"));
    assert!(!deferral.contains("simulate_contract_issuer_interaction"));

    let cycle = source
        .split("fn cycle")
        .nth(1)
        .and_then(|tail| tail.split("fn try_upgrade").next())
        .expect("direct contract driver");
    let issuer_checks = cycle
        .match_indices("public_contract_issuer_available(leader, &quest)")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(issuer_checks.len(), 2);
    let provision = cycle.find("provision_case_site_journey").unwrap();
    let interaction = cycle
        .find("simulate_contract_issuer_interaction_then")
        .unwrap();
    assert!(issuer_checks[0] < provision);
    assert!(provision < issuer_checks[1] && issuer_checks[1] < interaction);
}

#[test]
fn authoritative_contract_issuer_rejection_defers_accept_and_report_without_failure_metrics() {
    let coded = adventuresim_core::reducer_error::coded_reducer_error(
        ReducerErrorCode::ContractIssuerUnavailable,
        "wording is not part of the protocol",
    );
    assert!(contract_issuer_unavailable_failure(&format!(
        "interact_accept_contract failed: {coded}"
    )));
    assert!(contract_issuer_unavailable_failure(&format!(
        "interact_report_contract failed: {coded}"
    )));
    assert!(!contract_issuer_unavailable_failure(
        "interact_accept_contract failed: Contract is not offered"
    ));
    assert!(!contract_issuer_unavailable_failure(
        "accept_quest failed: Contract issuer is not available for interaction"
    ));
    assert!(!contract_issuer_unavailable_failure(
        "interact_accept_contract failed: Contract issuer is not available for interaction now"
    ));

    let source = LIVE_CORE_SOURCE;
    let accept = source
        .split("fn cycle")
        .nth(1)
        .and_then(|tail| tail.split("fn try_upgrade").next())
        .expect("direct contract acceptance driver");
    let interaction = accept
        .find("simulate_contract_issuer_interaction_then")
        .unwrap();
    let classifier = accept[interaction..]
        .find("contract_issuer_unavailable_failure(&error)")
        .unwrap()
        + interaction;
    let attempts = accept.find("self.metrics.quests_attempted += 1").unwrap();
    let accept_reducer = accept.find("accept_contract_then").unwrap();
    assert!(interaction < classifier && classifier < attempts && attempts < accept_reducer);
    assert!(accept.contains("authoritative_interaction_rejection"));
    assert!(accept.contains("return self.call(Err(error))"));

    let report = source
        .split("pub(super) fn turn_in_ready_direct_contract")
        .nth(1)
        .expect("direct contract report driver");
    let report_interaction = report
        .find("simulate_contract_issuer_interaction_then")
        .unwrap();
    let report_classifier = report
        .find("contract_issuer_unavailable_failure(&error)")
        .unwrap();
    let turn_in = report.find("report_contract_then").unwrap();
    let completion = report.find("self.metrics.quests_completed += 1").unwrap();
    assert!(report_interaction < report_classifier && report_classifier < turn_in);
    assert!(turn_in < completion);
    assert!(report.contains("authoritative_interaction_rejection"));
    assert!(report.contains("return self.call(Err(error))"));
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
