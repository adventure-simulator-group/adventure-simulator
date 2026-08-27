#[test]
fn startup_registers_and_resubscribes_gateway_before_seeding() {
    let source = LIVE_CORE_SOURCE;
    let claim = source.find("\"claim_simulation_run\"").unwrap();
    let register = source.find("\"register_strategic_gateway\"").unwrap();
    let resubscribe = source
        .find("gateway_subscription_rx")
        .expect("post-registration gateway subscription");
    let seed = source.find("\"seed_simulation_world\"").unwrap();
    assert!(claim < register && register < resubscribe && resubscribe < seed);
    let gateway_surface = &source[resubscribe..seed];
    let contracts = gateway_surface
        .find(".backend_contracts()")
        .expect("post-registration subscription must include offered contracts");
    let subscribe = gateway_surface
        .find(".subscribe()")
        .expect("post-registration subscription must be applied");
    assert!(
        contracts < subscribe,
        "offered contracts must be part of the applied gateway subscription"
    );
    for component in [
        ".backend_characters()",
        ".backend_character_capabilities()",
        ".backend_character_needs()",
        ".backend_character_stats()",
        ".backend_character_strategic_conditions()",
        ".backend_character_times()",
        ".backend_character_training_schedules()",
        ".backend_physiology_charts()",
    ] {
        assert!(
            gateway_surface
                .find(component)
                .is_some_and(|index| index < subscribe),
            "post-registration subscription must include {component}"
        );
    }
    let initial_subscription = source
        .split("subscription_builder()")
        .nth(1)
        .and_then(|tail| tail.split("\"claim_simulation_run\"").next())
        .expect("pre-registration subscription");
    assert!(initial_subscription.contains(".backend_character_stats()"));
    assert!(!initial_subscription.contains("backend_physiology_charts"));
    for forbidden in [
        ".infection_episode()",
        ".character_stats()",
        ".character_attribute()",
    ] {
        assert!(
            !initial_subscription.contains(forbidden)
                && !gateway_surface[..subscribe].contains(forbidden),
            "subscription must not expose private table {forbidden}"
        );
    }
}

#[test]
fn live_schedule_reallocates_disabled_tactical_crime_to_legal_labor() {
    let mut profile = generate_profile(42, 0);
    profile.schedule.combat_training_minutes = 17;
    profile.schedule.apprenticeship_minutes = 60;
    profile.schedule.profession_practice_minutes = 60;
    profile.schedule.labor = 30;
    profile.schedule.prayer = 45;
    profile.schedule.thievery = 60;
    profile.schedule.raiding = 60;
    let schedule = live_schedule(&profile);
    assert_eq!(schedule.combat_training_minutes, 15);
    assert_eq!(schedule.apprenticeship_minutes, 0);
    assert_eq!(schedule.profession_practice_minutes, 0);
    assert_eq!(schedule.labor_minutes, 150);
    assert_eq!(schedule.prayer_minutes, 45);
    assert_eq!(schedule.thievery_minutes, 0);
    assert_eq!(schedule.raiding_minutes, 0);
}

#[test]
fn disabled_crime_reallocation_leaves_labor_and_prayer_unchanged_without_crime() {
    let mut schedule = medical_rest_schedule();
    schedule.labor_minutes = 480;
    schedule.prayer_minutes = 60;
    assert_eq!(
        reallocate_disabled_crime_to_labor(schedule.clone()),
        schedule
    );
}

#[test]
fn settlement_activity_venue_prefers_fed_temple_then_reserve_aware_inn() {
    assert_eq!(
        select_settlement_activity_venue(true, true, true, 2, 0, Some(2)),
        Some(DomainSettlementActionService::Temple)
    );
    assert_eq!(
        select_settlement_activity_venue(true, true, false, 4, 2, Some(2)),
        Some(DomainSettlementActionService::Inn)
    );
    assert_eq!(
        select_settlement_activity_venue(false, true, false, 0, 0, Some(2)),
        None
    );
    assert_eq!(
        select_settlement_activity_venue(true, true, false, 3, 2, Some(2)),
        None
    );
    assert_eq!(
        select_settlement_activity_venue(true, false, false, 3, 2, Some(2)),
        None
    );
    assert_eq!(
        select_settlement_activity_venue(true, false, false, 4, 2, Some(2)),
        Some(DomainSettlementActionService::Inn)
    );
}

#[test]
fn insufficient_settlement_resources_defer_to_installed_labor_without_free_service() {
    let source = LIVE_CORE_SOURCE;
    let activity = source
        .split("fn settlement_activity_day")
        .nth(1)
        .and_then(|tail| tail.split("/// NPCs use the same custody").next())
        .expect("settlement activity policy");
    let install = activity.find("install_activity_schedule").unwrap();
    let venue = activity.find("settlement_activity_venue").unwrap();
    let deferred = activity.find("format_deferred_activity_detail").unwrap();
    let rest = activity.find("rest_at_settlement_hours_then").unwrap();
    assert!(install < venue);
    assert!(venue < deferred);
    assert!(deferred < rest);
    let no_venue = activity
        .split("let Some(venue)")
        .nth(1)
        .and_then(|tail| tail.split("let result = reducer_call!").next())
        .expect("no-venue deferral branch");
    assert!(no_venue.contains("format_deferred_activity_detail"));
    assert!(no_venue.contains("continue;"));
    assert!(!no_venue.contains("rest_at_settlement_hours_then"));
    assert!(source.contains("outcome=deferred;reason=insufficient_visible_resources"));

    let loop_source = source
        .split("for cycle in 0..config.cycles")
        .nth(1)
        .expect("bounded live loop");
    assert!(loop_source.contains("advance_simulation_world_time_then"));
    assert!(!activity.contains("spend_private_settlement_downtime"));
}

#[test]
fn unaffordable_generated_window_wait_defers_without_camp_or_reducer_error_allowlist() {
    let source = LIVE_CORE_SOURCE;
    let wait = source
        .split("fn wait_for_generated_investigation_window")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn return_completed_generated_party_to_origin")
                .next()
        })
        .expect("generated night-window wait");
    let unavailable = wait.find("settlement_venue.is_none()").unwrap();
    let suppressed = wait.find("reason=insufficient_visible_resources").unwrap();
    let field_rest = wait.find("rest_at_camp_with_party_shelter").unwrap();
    assert!(unavailable < suppressed);
    assert!(suppressed < field_rest);
    assert!(wait[unavailable..field_rest].contains("return Ok(false)"));
    assert!(!wait.contains("safe_core_loop_failure"));
}

#[test]
fn temple_viability_depends_on_visible_food_not_carried_water() {
    assert!(temple_food_covers_one_day(
        adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
    ));
    assert!(!temple_food_covers_one_day(
        adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY - 1.0
    ));
}

#[test]
fn committed_reserve_keeps_visible_medical_cost_and_attainable_cash_target() {
    assert_eq!(
        visible_activity_committed_reserve(9, 200, Some(6), Some(2)),
        7
    );
    assert_eq!(
        visible_activity_committed_reserve(250, 200, Some(6), Some(2)),
        206
    );
}

#[test]
fn prayer_switches_to_installed_labor_plan_under_reserve_pressure() {
    let mut profile = generate_profile(42, 0);
    profile.preferred_activity = ActivityPreference::Prayer;
    profile.schedule.labor = 0;
    profile.schedule.thievery = 0;
    profile.schedule.raiding = 0;
    profile.schedule.prayer = 480;
    let (schedule, effective, fallback) = activity_schedule_plan(&profile, false, 2, 1, Some(2));
    assert_eq!(schedule.labor_minutes, 480);
    assert_eq!(schedule.prayer_minutes, 0);
    assert_eq!(effective, "Labor");
    assert_eq!(fallback, "subsistence_reserve_to_labor");

    let (fed_schedule, fed_effective, fed_fallback) =
        activity_schedule_plan(&profile, true, 2, 1, Some(2));
    assert_eq!(fed_schedule.prayer_minutes, 480);
    assert_eq!(fed_schedule.labor_minutes, 0);
    assert_eq!(fed_effective, "Prayer");
    assert_eq!(fed_fallback, "none");
}

#[test]
fn activity_schedule_is_installed_before_the_logged_rest_attempt() {
    let source = LIVE_CORE_SOURCE;
    let start = source
        .find("fn settlement_activity_day")
        .expect("activity policy");
    let block = &source[start
        ..source[start..]
            .find("/// NPCs use the same custody")
            .map(|offset| start + offset)
            .expect("activity policy end")];
    let install = block
        .find("install_activity_schedule")
        .expect("authoritative schedule installation");
    let rest = block
        .find("rest_at_settlement_hours_then")
        .expect("authoritative activity rest");
    assert!(install < rest);
}

#[test]
fn settlement_activity_stops_when_an_incident_relocates_the_party() {
    let source = LIVE_CORE_SOURCE;
    let activity = source
        .split("fn settlement_activity_day")
        .nth(1)
        .and_then(|tail| tail.split("/// NPCs use the same custody").next())
        .expect("settlement activity policy");
    let member_loop = activity.find("for agent in self.party_agents").unwrap();
    let first_location_check = activity
        .find("party_is_still_at_original_settlement")
        .unwrap();
    let first_medical = activity.find("ensure_medically_safe(agent)").unwrap();
    assert!(member_loop < first_location_check && first_location_check < first_medical);

    let activity_rest = activity.find("rest_at_settlement_hours_then").unwrap();
    let post_rest_location_check = activity[activity_rest..]
        .find("party_is_still_at_original_settlement")
        .map(|offset| activity_rest + offset)
        .unwrap();
    let post_activity_observation = activity
        .find("let after = self.activity_observation")
        .unwrap();
    assert!(activity_rest < post_rest_location_check);
    assert!(post_rest_location_check < post_activity_observation);
    assert!(
        activity
            .matches("party_is_still_at_original_settlement")
            .count()
            >= 6
    );
    assert!(activity.contains("return Ok(())"));
    assert!(!activity.contains("simulation character is not at a settlement"));
    assert!(!activity.contains("travel_to_settlement"));
    assert!(activity.contains("observed_activity_site_origins"));
    assert!(activity.contains("(party_id.to_owned(), case_site_id.value)"));
    assert!(activity.contains("original_settlement_id.to_owned()"));
}

#[test]
fn each_active_cycle_advances_world_time_before_refreshing_npc_activity() {
    let source = LIVE_CORE_SOURCE;
    let loop_start = source
        .find("for cycle in 0..config.cycles")
        .expect("core-loop cycle");
    let loop_end = source[loop_start..]
        .find("// Bounded final settlement cleanup")
        .map(|offset| loop_start + offset)
        .expect("core-loop cleanup");
    let active_block = &source[loop_start..loop_end];
    let advance = active_block
        .find("\"advance_simulation_world_time\"")
        .expect("simulation clock advance");
    assert!(
        active_block[advance..].contains("\"ensure_settlement_activity\""),
        "settlement activity must refresh after the simulation clock advances"
    );
}

#[test]
fn quest_decision_detail_is_bounded_and_stably_formatted() {
    assert_eq!(
        format_quest_decision_detail(QuestDecisionObservation {
            cycle: 7,
            wants_quest: true,
            selector: 0.25,
            quest_propensity: 0.75,
            settlement_id: Some("lubeck"),
            offered_contracts: 2,
            safe_offered_contracts: 1,
            open_generated_cases: 1,
            projected_investigation_actions: 3,
            quest_path: "generated_open_case",
            quest_intended: true,
            quest_selected: true,
            selection_reason: "none",
        }),
        "cycle=7;wants_quest=true;selector=0.250000;quest_propensity=0.750000;settlement=lubeck;offered_contracts=2;safe_offered_contracts=1;open_generated_cases=1;projected_investigation_actions=3;quest_path=generated_open_case;quest_intended=true;quest_selected=true;selection_reason=none"
    );
    assert_eq!(
        format_quest_decision_detail(QuestDecisionObservation {
            cycle: 8,
            wants_quest: false,
            selector: 0.25,
            quest_propensity: 0.75,
            settlement_id: None,
            offered_contracts: 0,
            safe_offered_contracts: 0,
            open_generated_cases: 0,
            projected_investigation_actions: 0,
            quest_path: "activity",
            quest_intended: false,
            quest_selected: false,
            selection_reason: "policy_prefers_activity",
        }),
        "cycle=8;wants_quest=false;selector=0.250000;quest_propensity=0.750000;settlement=none;offered_contracts=0;safe_offered_contracts=0;open_generated_cases=0;projected_investigation_actions=0;quest_path=activity;quest_intended=false;quest_selected=false;selection_reason=policy_prefers_activity"
    );
}

#[test]
fn quest_selection_trace_precedes_discovery_reducers() {
    let source = LIVE_CORE_SOURCE;
    let loop_body = source
        .split("for cycle in 0..config.cycles")
        .nth(1)
        .expect("active core-loop body");
    let selection = loop_body
        .find("CoreLoopEventKind::QuestDecision")
        .expect("pre-action quest selection");
    let discovery = loop_body
        .find("runner.discover_generated_case")
        .expect("generated discovery reducer path");
    assert!(
        selection < discovery,
        "quest selection must be recorded before discovery dialogue"
    );
}

#[test]
fn generated_case_views_filter_by_owner_and_sort_stably() {
    let rows = vec![
        (9, "case-b".into(), "B".into(), "open".into(), 10),
        (7, "case-z".into(), "Z".into(), "open".into(), 1),
        (9, "case-a".into(), "A".into(), "open".into(), 20),
        (9, "case-c".into(), "C".into(), "completed".into(), 0),
    ];
    assert_eq!(
        stable_owned_open_cases(9, rows),
        vec![
            ("case-b".to_owned(), "B".to_owned()),
            ("case-a".to_owned(), "A".to_owned())
        ]
    );
}

#[test]
fn generated_case_fairness_continues_actionable_progress_then_round_robins() {
    let cases = vec![
        ("oldest".to_owned(), "Oldest".to_owned()),
        ("newer".to_owned(), "Newer".to_owned()),
    ];
    assert_eq!(fair_open_case_index(&cases, None, false, None), 0);
    assert_eq!(
        fair_open_case_index(&cases, Some("oldest"), true, Some("oldest")),
        0,
        "a progressing actionable case retains focus"
    );
    assert_eq!(
        fair_open_case_index(&cases, Some("oldest"), false, Some("oldest")),
        1,
        "a no-progress case yields to the next public case"
    );
    assert_eq!(
        fair_open_case_index(&cases, None, false, Some("newer")),
        0,
        "round robin returns to the oldest case instead of starving it"
    );
}

#[test]
fn generated_case_selection_is_public_chronological_and_shared_by_both_paths() {
    let source = LIVE_CORE_SOURCE;
    let stable = source
        .split("fn stable_owned_open_cases")
        .nth(1)
        .and_then(|tail| tail.split("enum GeneratedClosureAttribution").next())
        .expect("stable public case ordering");
    assert!(stable.contains("latest_update_at"));
    assert!(!stable.contains("fixture"));
    let bootstrap = source
        .split("let mut open_generated_cases = runner.owned_open_generated_cases")
        .nth(1)
        .expect("core-loop case selection");
    assert_eq!(
        bootstrap
            .matches("select_owned_open_generated_case(leader)")
            .count(),
        2,
        "continuation and post-discovery must share the same fair selector"
    );
    assert!(!bootstrap.contains("open_generated_cases[0]"));
    assert!(!bootstrap.contains("owned_open_generated_cases(leader).into_iter().next()"));
}

#[test]
fn generated_case_no_progress_is_bounded_and_publicly_diagnosable() {
    let diagnostic = LIVE_CORE_SOURCE
        .split("fn emit_generated_case_no_progress")
        .nth(1)
        .and_then(|tail| tail.split("pub(super) fn generated_case_status").next())
        .expect("generated-case no-progress diagnostic");
    assert!(diagnostic.contains("row.owner_character_id == character_id"));
    assert!(diagnostic.contains("row.case_id == case_id"));
    assert!(diagnostic.contains("visible_npc_candidates"));
    assert!(diagnostic.contains("action_versions.truncate(8)"));
    assert!(diagnostic.contains("len().min(64)"));
    assert!(diagnostic.contains("CoreLoopEventKind::GeneratedInvestigationReplan"));
    assert!(!diagnostic.contains("fixture"));
    assert!(!diagnostic.contains("canonical_case"));
    let driver = LIVE_CORE_SOURCE
        .split("pub(super) fn advance_generated_case")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn turn_in_ready_direct_contract")
                .next()
        })
        .expect("generated-case driver");
    assert!(driver.contains(
        "emit_generated_case_no_progress(character_id, agent, cycle, case_id, &actions)"
    ));
}

#[test]
fn generated_case_tries_bounded_distinct_public_witnesses_before_stalling() {
    let driver = LIVE_CORE_SOURCE
        .split("pub(super) fn advance_generated_case")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn turn_in_ready_direct_contract")
                .next()
        })
        .expect("generated-case driver");
    let witness_pass = driver
        .split("let mut witnesses")
        .nth(1)
        .and_then(|tail| tail.split("if witness_progressed").next())
        .expect("bounded public witness pass");
    assert!(witness_pass.contains("row.owner_character_id == character_id"));
    assert!(witness_pass.contains("row.case_id == case_id"));
    assert!(witness_pass.contains("row.corrected_by.is_empty()"));
    assert!(witness_pass.contains("witnesses.sort_by_key"));
    assert!(witness_pass.contains("attempted_witnesses.len() >= 8"));
    assert!(witness_pass.contains("try_generated_dialogue_topic"));
    assert!(!witness_pass.contains("max_by_key"));
    assert!(!witness_pass.contains("fixture"));
}

#[test]
fn ambiguous_public_npc_candidates_remain_bounded_and_stable() {
    let candidates = vec![
        PublicNpcCandidate {
            resident_character_id: 9_007_199_254_741_100,
            name: "Marta".into(),
            profession: "Baker".into(),
            conversation_id: "local-resident".into(),
            location_id: "market".into(),
        },
        PublicNpcCandidate {
            resident_character_id: 9_007_199_254_740_993,
            name: "Marta".into(),
            profession: "Baker".into(),
            conversation_id: "local-resident".into(),
            location_id: "market".into(),
        },
    ];
    let sorted = stable_public_npc_candidates(candidates, Some("Marta"), Some("market"));
    assert_eq!(
        sorted
            .iter()
            .map(|candidate| candidate.resident_character_id)
            .collect::<Vec<_>>(),
        vec![9_007_199_254_740_993, 9_007_199_254_741_100]
    );
    assert_eq!(
        sorted.len(),
        2,
        "ambiguity must be resolved only by projected topic eligibility, not guessed identity"
    );
}

#[test]
fn inn_only_dialogue_candidates_exclude_hidden_service_locations() {
    let profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    let projected = SettlementEconomyProfile {
        rules_version: profile.rules_version,
        prosperity_score: profile.prosperity_score,
        prosperity_tier: ProsperityTier::Subsistence,
        services: vec![SettlementService::Inn],
        specializations: vec![],
        stock: vec![SettlementStock {
            category: StockCategory::GeneralGoods,
            abundance: 1,
            provenance: ProfileFactProvenance::DeterministicGapFill,
        }],
    };
    assert_eq!(
        public_settlement_economy_profile(&projected),
        Some(profile.clone())
    );

    let mut unsupported_projection = projected.clone();
    unsupported_projection.rules_version = u32::MAX;
    assert_eq!(
        public_settlement_economy_profile(&unsupported_projection),
        None
    );
    let candidate = |id: u64, location: &str| PublicNpcCandidate {
        resident_character_id: id,
        name: format!("Resident {id}"),
        profession: "Resident".into(),
        conversation_id: "local-resident".into(),
        location_id: location.into(),
    };
    let locations = [
        "overview",
        "residences",
        "inn",
        "keep",
        "market",
        "forge",
        "armoury",
        "tailor",
        "herbalist",
        "church",
        "bookstore",
    ];
    let retained = retain_navigable_public_npc_candidates(
        locations
            .iter()
            .enumerate()
            .map(|(index, location)| candidate(index as u64 + 1, location))
            .collect(),
        &profile,
        false,
        "ironforge",
    );
    assert_eq!(
        retained
            .iter()
            .map(|candidate| candidate.location_id.as_str())
            .collect::<Vec<_>>(),
        vec!["overview", "residences", "inn"]
    );
}

#[test]
fn hidden_preferred_witness_cannot_reach_dialogue_selection() {
    let profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    let candidates = retain_navigable_public_npc_candidates(
        vec![
            PublicNpcCandidate {
                resident_character_id: 41,
                name: "Hidden Witness".into(),
                profession: "Armorer".into(),
                conversation_id: "local-resident".into(),
                location_id: "armoury".into(),
            },
            PublicNpcCandidate {
                resident_character_id: 42,
                name: "Visible Resident".into(),
                profession: "Innkeeper".into(),
                conversation_id: "local-resident".into(),
                location_id: "inn".into(),
            },
        ],
        &profile,
        false,
        "ironforge",
    );
    let mut preferred =
        stable_public_npc_candidates(candidates, Some("Hidden Witness"), Some("armoury"));
    preferred.retain(|candidate| candidate.name.eq_ignore_ascii_case("Hidden Witness"));
    assert!(preferred.is_empty());
}

#[test]
fn dialogue_candidates_are_filtered_by_the_authoritative_public_navigation_rule() {
    let source = LIVE_CORE_SOURCE;
    let candidates = source
        .split("pub(super) fn visible_npc_candidates")
        .nth(1)
        .and_then(|tail| tail.split("pub(super) fn start_public_dialogue").next())
        .expect("public NPC candidate projection");
    assert!(candidates.contains("public_settlement_economy_profile"));
    assert!(candidates.contains("retain_navigable_public_npc_candidates"));
    assert_eq!(
        source.matches("self.start_public_dialogue(").count(),
        2,
        "discovery and case continuation must share the filtered candidate source"
    );

    let navigation = source
        .split("fn retain_navigable_public_npc_candidates")
        .nth(1)
        .and_then(|tail| tail.split("const PUBLIC_DISCOVERY_BACKOFF_MINUTES").next())
        .expect("public NPC navigation boundary");
    assert!(navigation.contains("settlement_economy::npc_location_is_navigable"));

    let case_dialogue = source
        .split("pub(super) fn try_generated_dialogue_topic")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn public_dialogue_progress_fingerprint")
                .next()
        })
        .expect("generated-case dialogue selection");
    assert!(case_dialogue.contains("self.visible_npc_candidates"));
    let preferred_filter = case_dialogue
        .find("candidates.retain(|candidate| candidate.name.eq_ignore_ascii_case(name))")
        .expect("preferred witnesses are filtered from publicly visible candidates");
    let candidate_loop = case_dialogue
        .find("for candidate in candidates.into_iter()")
        .expect("dialogue attempts iterate the filtered candidates");
    assert!(preferred_filter < candidate_loop);

    let normalized = case_dialogue
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        normalized.ends_with("Ok(false) }"),
        "no visible preferred witness must safely decline dialogue"
    );
}

#[test]
fn generated_discovery_cycles_stably_through_valid_public_contacts() {
    let candidate = |resident_character_id: u64, name: &str, location: &str| PublicNpcCandidate {
        resident_character_id,
        name: name.into(),
        profession: "Resident".into(),
        conversation_id: "local-resident".into(),
        location_id: location.into(),
    };
    let inn_candidates = vec![
        candidate(9_007_199_254_741_100, "Zelda", "inn"),
        candidate(9_007_199_254_740_993, "Agnes", "inn"),
        candidate(9_007_199_254_741_050, "Otto", "overview"),
        candidate(9_007_199_254_741_025, "Marta", "market"),
    ];
    let inn =
        stable_discovery_action_candidate(inn_candidates.clone(), None).expect("first inn contact");
    assert_eq!(
        (inn.location_id.as_str(), inn.resident_character_id),
        ("inn", 9_007_199_254_740_993)
    );
    let inn_identity = public_discovery_contact_identity(&inn);
    let next_inn = stable_discovery_action_candidate(inn_candidates.clone(), Some(&inn_identity))
        .expect("next inn contact");
    assert_eq!(next_inn.resident_character_id, 9_007_199_254_741_100);
    let next_inn_identity = public_discovery_contact_identity(&next_inn);
    let wrapped_inn = stable_discovery_action_candidate(inn_candidates, Some(&next_inn_identity))
        .expect("wrapped inn contact");
    assert_eq!(wrapped_inn.resident_character_id, inn.resident_character_id);

    let overview = stable_discovery_action_candidate(
        vec![
            candidate(9_007_199_254_741_050, "Otto", "overview"),
            candidate(9_007_199_254_741_000, "Bertha", "overview"),
            candidate(9_007_199_254_741_025, "Marta", "market"),
        ],
        None,
    )
    .expect("overview fallback representative");
    assert_eq!(
        (
            overview.location_id.as_str(),
            overview.resident_character_id
        ),
        ("overview", 9_007_199_254_741_000)
    );

    assert!(
        stable_discovery_action_candidate(
            vec![candidate(9_007_199_254_741_025, "Marta", "market")],
            None
        )
        .is_none()
    );
}

#[test]
fn generated_discovery_outcomes_do_not_conflate_selection_with_success() {
    assert!(GeneratedDiscoveryOutcome::Discovered.case_discovered());
    assert!(!GeneratedDiscoveryOutcome::NoVisibleContacts.case_discovered());
    assert!(!GeneratedDiscoveryOutcome::NoPublicRumor.case_discovered());
    assert!(!GeneratedDiscoveryOutcome::PublicBackoff.case_discovered());
}

#[test]
fn public_discovery_backoff_expires_or_invalidates_on_public_change() {
    let initial = PublicDiscoveryFingerprint {
        settlement_id: "settlement-a".into(),
        contacts: vec![PublicDiscoveryContactIdentity {
            resident_character_id: 9_007_199_254_740_993,
            conversation_id: "conversation-a".into(),
            location_id: "inn".into(),
        }],
        active_symptoms: vec![(
            "missing livestock".into(),
            "Several goats have vanished.".into(),
            1_000,
            20_000,
        )],
    };
    let backoff = PublicDiscoveryBackoff {
        fingerprint: initial.clone(),
        last_contact: initial.contacts[0].clone(),
        retry_at: 3_880,
    };
    assert!(public_discovery_backoff_active(&backoff, &initial, 3_879));
    assert!(!public_discovery_backoff_active(&backoff, &initial, 3_880));
    assert_eq!(
        public_discovery_previous_contact(Some(&backoff), &initial),
        Some(&backoff.last_contact)
    );

    let mut changed = initial;
    changed.contacts[0].location_id = "overview".into();
    assert!(!public_discovery_backoff_active(&backoff, &changed, 2_000));
    assert_eq!(
        public_discovery_previous_contact(Some(&backoff), &changed),
        None,
        "a public fingerprint change resets exploration to the first stable contact"
    );
}

#[test]
fn discovery_prioritizes_new_referrals_and_retries_unresolved_public_referrals() {
    let referral = |recorded_at, corrected_by: &str| PublicDiscoveryReferral {
        owner_character_id: 7,
        case_id: "journal:case".into(),
        lead_id: "lead:referral".into(),
        summary: "A witness may know more.".into(),
        witness_name: "Agnes".into(),
        expected_location: "inn".into(),
        current_learned_location: String::new(),
        corrected_by: corrected_by.into(),
        recorded_at,
    };
    let original = referral(10, "");
    let before = HashMap::from([(original.lead_id.clone(), original.clone())]);

    assert_eq!(
        public_discovery_referral_to_follow(7, &before, &HashSet::new(), [original.clone()]),
        Some(original.clone()),
        "an unresolved public referral remains actionable on a later cycle"
    );
    assert!(
        public_discovery_referral_to_follow(
            7,
            &before,
            &HashSet::from([original.case_id.clone()]),
            [original.clone()],
        )
        .is_none(),
        "an already-open case does not repeat its discovery referral"
    );
    let updated = referral(11, "");
    let mut newer_unresolved = referral(12, "");
    newer_unresolved.case_id = "journal:other-case".into();
    newer_unresolved.lead_id = "lead:other-referral".into();
    let priority_before = HashMap::from([
        (original.lead_id.clone(), original),
        (newer_unresolved.lead_id.clone(), newer_unresolved.clone()),
    ]);
    assert_eq!(
        public_discovery_referral_to_follow(
            7,
            &priority_before,
            &HashSet::new(),
            [newer_unresolved, updated.clone()],
        ),
        Some(updated),
        "a newly updated referral takes priority over a newer unchanged referral"
    );
    assert!(
        public_discovery_referral_to_follow(
            7,
            &HashMap::new(),
            &HashSet::new(),
            [referral(12, "replacement")],
        )
        .is_none(),
        "corrected referrals are not actionable"
    );
    assert!(
        public_discovery_referral_to_follow(
            8,
            &HashMap::new(),
            &HashSet::new(),
            [referral(12, "")],
        )
        .is_none(),
        "another owner's referral is not actionable"
    );
}

fn semantic_dialogue_lead() -> PublicDialogueLeadSemantic {
    PublicDialogueLeadSemantic {
        summary: "A witness may know more.".into(),
        source_label: "local testimony".into(),
        confidence_bps: 6_000,
        destination_stage: CoreDestinationKnowledgeStage::Textual,
        directions: "Ask at the inn.".into(),
        exact_location_id: String::new(),
        latitude_e7: 0,
        longitude_e7: 0,
        witness_name: "Agnes".into(),
        witness_description: "A local weaver.".into(),
        witness_occupation_or_relationship: "weaver".into(),
        expected_location: "inn".into(),
        current_learned_location: String::new(),
        contradiction_group: "witness:agnes".into(),
        corrected_by: String::new(),
    }
}

#[test]
fn generated_destination_stages_cross_into_the_canonical_domain_type() {
    assert_eq!(
        core_destination_knowledge_stage(DestinationKnowledgeStage::ExactBelieved),
        CoreDestinationKnowledgeStage::ExactBelieved
    );
    assert_eq!(
        core_destination_knowledge_stage(DestinationKnowledgeStage::Visited),
        CoreDestinationKnowledgeStage::Visited
    );
}

fn semantic_dialogue_action() -> PublicDialogueActionSemantic {
    PublicDialogueActionSemantic {
        action_id: "action:inspect".into(),
        method: "inspect".into(),
        summary: "Inspect the mill.".into(),
        known_prerequisites: "Bring light.".into(),
        duration_min_minutes: 30,
        duration_max_minutes: 60,
        uncertainty_bps: 2_000,
        skill_contributions: "observation".into(),
        weather_available: true,
        required_case_site_id: "site:mill".into(),
        available: true,
        can_travel_to_required_site: true,
        unavailable_reason_code: String::new(),
    }
}

#[test]
fn dialogue_topics_suppress_no_progress_and_reenable_after_public_change() {
    let initial = PublicDialogueProgressFingerprint {
        cases: vec![("journal:case".into(), "open".into())],
        leads: vec![semantic_dialogue_lead()],
        actions: Vec::new(),
        outcomes: Vec::new(),
        sites: Vec::new(),
    };
    assert!(public_dialogue_topic_attempt_allowed(None, &initial));
    assert!(!public_dialogue_topic_made_progress(&initial, &initial));
    assert!(!public_dialogue_topic_attempt_allowed(
        Some(&initial),
        &initial
    ));

    let mut progressed = initial.clone();
    progressed.actions.push(semantic_dialogue_action());
    assert!(public_dialogue_topic_made_progress(&initial, &progressed));
    assert!(public_dialogue_topic_attempt_allowed(
        Some(&initial),
        &progressed
    ));
}

#[test]
fn public_dialogue_presence_rejects_suppression_and_replans_authority_races() {
    assert!(npc_is_publicly_present(480, 1_020, false, false, 900));
    assert!(!npc_is_publicly_present(480, 1_020, true, false, 900));
    assert!(!npc_is_publicly_present(480, 1_020, false, true, 900));
    assert!(dialogue_contact_presence_changed(
        "start_dialogue failed: Dialogue actor is not present at this time"
    ));
    assert!(!dialogue_contact_presence_changed(
        "start_dialogue failed: Dialogue conversation is not valid for this NPC"
    ));
}

#[test]
fn repeated_testimony_metadata_does_not_count_as_dialogue_progress() {
    fn semantic_fingerprint(
        case_updated_at: u64,
        lead_id: &str,
        lead_recorded_at: u64,
    ) -> PublicDialogueProgressFingerprint {
        let _publication_metadata = (case_updated_at, lead_id, lead_recorded_at);
        PublicDialogueProgressFingerprint {
            cases: vec![("journal:case".into(), "open".into())],
            leads: vec![semantic_dialogue_lead()],
            actions: Vec::new(),
            outcomes: Vec::new(),
            sites: Vec::new(),
        }
    }

    let original = semantic_fingerprint(10, "lead:first", 10);
    let republished = semantic_fingerprint(20, "lead:duplicate", 20);

    assert!(!public_dialogue_topic_made_progress(
        &original,
        &republished
    ));
    assert!(!public_dialogue_topic_attempt_allowed(
        Some(&original),
        &republished
    ));

    let fingerprint_source = LIVE_CORE_SOURCE
        .split("pub(super) fn public_dialogue_progress_fingerprint")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn generated_actor_ready_after_time")
                .next()
        })
        .expect("public dialogue progress fingerprint");
    for publication_field in [
        "latest_update_at",
        "recorded_at",
        "expected_version",
        "wait_minutes",
    ] {
        assert!(
            !fingerprint_source.contains(publication_field),
            "{publication_field} is publication metadata, not semantic investigation progress"
        );
    }
}

#[test]
fn policy_relevant_lead_and_action_changes_count_as_dialogue_progress() {
    let baseline = PublicDialogueProgressFingerprint {
        cases: vec![("journal:case".into(), "open".into())],
        leads: vec![semantic_dialogue_lead()],
        actions: vec![semantic_dialogue_action()],
        outcomes: Vec::new(),
        sites: Vec::new(),
    };
    let mut changed_lead = baseline.clone();
    changed_lead.leads[0].confidence_bps = 8_000;
    assert!(public_dialogue_topic_made_progress(
        &baseline,
        &changed_lead
    ));

    let mut changed_action = baseline.clone();
    changed_action.actions[0].weather_available = false;
    assert!(public_dialogue_topic_made_progress(
        &baseline,
        &changed_action
    ));
}

#[test]
fn generated_action_preflight_suppresses_public_party_clock_skew() {
    assert!(public_party_clocks_aligned(
        &[7, 8],
        [(7, 340_929), (8, 340_929)]
    ));
    assert!(!public_party_clocks_aligned(
        &[7, 8],
        [(7, 339_489), (8, 340_929)]
    ));
    assert!(!public_party_clocks_aligned(&[7, 8], [(7, 340_929)]));
}

#[test]
fn public_symptom_diagnostics_are_coarse() {
    assert_eq!(public_count_bucket(0), "0");
    assert_eq!(public_count_bucket(1), "1");
    assert_eq!(public_count_bucket(3), "2_to_3");
    assert_eq!(public_count_bucket(99), "4_plus");
    assert_eq!(public_symptom_age_bucket(None), "none");
    assert_eq!(public_symptom_age_bucket(Some(1_439)), "under_1_day");
    assert_eq!(public_symptom_age_bucket(Some(1_440)), "1_to_2_days");
    assert_eq!(public_symptom_age_bucket(Some(4_320)), "3_to_7_days");
    assert_eq!(public_symptom_age_bucket(Some(11_520)), "8_plus_days");
}

#[test]
fn discovery_logging_uses_only_the_owner_visible_case_postcondition() {
    let source = LIVE_CORE_SOURCE;
    let discovery = source
        .split("fn discover_generated_case")
        .nth(1)
        .and_then(|tail| tail.split("fn try_generated_dialogue_topic").next())
        .expect("generated discovery policy");
    assert_eq!(discovery.matches("start_public_dialogue(").count(), 1);
    assert!(discovery.contains("owned_open_generated_cases(character_id)"));
    assert!(discovery.contains("backend_investigation_leads()"));
    assert!(discovery.contains("public_discovery_referral_to_follow"));
    assert!(discovery.contains("&[\"referred-testimony\"]"));
    let referral = discovery
        .find("public_discovery_referral_to_follow")
        .expect("public referral postcondition");
    let testimony = discovery[referral..]
        .find("try_generated_dialogue_topic")
        .map(|offset| referral + offset)
        .expect("referral testimony follow-through");
    let journal_after_testimony = discovery[testimony..]
        .find("owned_open_generated_cases(character_id)")
        .map(|offset| testimony + offset)
        .expect("journal postcondition after testimony");
    let fruitful = discovery
        .find("generated_discovery_actions_fruitful")
        .expect("fruitful discovery metric");
    assert!(referral < testimony);
    assert!(testimony < journal_after_testimony);
    assert!(journal_after_testimony < fruitful);
    assert!(discovery.contains("rumor_delivered=true"));
    assert!(discovery.contains("reason=rumor_delivered"));
    assert!(discovery.contains("reason=no_public_rumor_available"));
    assert!(source.contains(".add_query(|query| query.from.local_problem_symptom())"));
    assert!(source.contains(".add_query(|query| query.from.world_clock())"));
    assert!(discovery.contains("public_backoff=true"));
    assert!(!discovery.contains("local_problem_rumor_delivery"));
    assert!(!discovery.contains("local_problem_receipt"));
    assert!(!discovery.contains("npc_intervention"));
    assert!(!discovery.contains("quest_generation_authority"));
}

#[test]
fn dialogue_topic_policy_returns_progress_only_after_public_projection_change() {
    let source = LIVE_CORE_SOURCE;
    let dialogue = source
        .split("pub(super) fn try_generated_dialogue_topic")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn generated_actor_ready_after_time")
                .next()
        })
        .expect("generated dialogue topic policy");
    let before = dialogue
        .find("let public_before = self.public_dialogue_progress_fingerprint")
        .unwrap();
    let reducer = dialogue.find("choose_dialogue_topic_then").unwrap();
    let after = dialogue
        .find("let public_after = self.public_dialogue_progress_fingerprint")
        .unwrap();
    let progress = dialogue
        .find("public_dialogue_topic_made_progress")
        .unwrap();
    let success = dialogue.find("return Ok(true)").unwrap();
    assert!(before < reducer);
    assert!(reducer < after);
    assert!(after < progress);
    assert!(progress < success);
    assert!(dialogue.contains("generated_dialogue_no_progress"));
}

#[test]
fn generated_completion_is_attributed_only_to_the_immediate_own_transition() {
    assert_eq!(
        generated_closure_attribution("open", Some("completed"), true),
        GeneratedClosureAttribution::OwnImmediateTransition
    );
    assert_eq!(
        generated_closure_attribution("open", Some("completed"), false),
        GeneratedClosureAttribution::ExternalTransition
    );
    assert_eq!(
        generated_closure_attribution("open", Some("open"), true),
        GeneratedClosureAttribution::StillOpen
    );
}

#[test]
fn generated_projection_rows_require_exact_owner_and_public_case() {
    assert!(projected_case_row_matches(7, "public-a", 7, "public-a"));
    assert!(!projected_case_row_matches(7, "public-a", 8, "public-a"));
    assert!(!projected_case_row_matches(7, "public-a", 7, "public-b"));
}

#[test]
fn generated_site_selection_requires_the_exact_occupied_pin() {
    assert!(occupied_case_pin_matches(
        7, "public-a", "site-2", 7, "public-a", "site-2"
    ));
    assert!(!occupied_case_pin_matches(
        7, "public-a", "site-2", 7, "public-a", "site-1"
    ));
}

#[test]
fn case_site_duration_bounds_fatigue_expanded_round_trip_and_cycle16_shape() {
    assert_eq!(projected_case_site_journey_minutes(1_250, 480), Some(240));
    assert_eq!(
        projected_case_site_journey_minutes(20_000, 480),
        Some(7_680)
    );
    assert_eq!(1_503 * JOURNEY_PROVISION_ELAPSED_BOUND_FACTOR, 6_012);
    assert_eq!(projected_case_site_journey_minutes(20_000, 0), None);
    assert_eq!(projected_case_site_journey_minutes(0, 480), None);
}

#[test]
fn camp_rest_uses_only_remaining_active_public_forecast_interval() {
    let intervals = vec![
        JourneyCampInterval {
            movement_minute: 480,
            elapsed_start_minute: 480,
            elapsed_minutes: 960,
            average_fatigue_start: 0.5,
            average_fatigue_end: 0.0,
            maximum_fatigue_end: 0.0,
        },
        JourneyCampInterval {
            movement_minute: 960,
            elapsed_start_minute: 1_920,
            elapsed_minutes: 960,
            average_fatigue_start: 0.5,
            average_fatigue_end: 0.0,
            maximum_fatigue_end: 0.0,
        },
    ];
    assert_eq!(
        projected_camp_rest_minutes(480, 2_500, &intervals),
        Some((480, 960))
    );
    assert_eq!(
        projected_camp_rest_minutes(1_200, 2_500, &intervals),
        Some((1_200, 240))
    );
    assert_eq!(
        projected_camp_rest_minutes(1_920, 2_500, &intervals),
        Some((1_920, 580))
    );
    assert_eq!(projected_camp_rest_minutes(1_500, 2_500, &intervals), None);
}
