#[test]
fn generated_runner_subscribes_only_to_public_projection_inventory() {
    let source = LIVE_CORE_SOURCE;
    let production = source.split("#[cfg(test)]").next().unwrap();
    for required in [
        ".backend_settlement_residents()",
        ".settlement_resident_presence()",
        ".backend_dialogue_sessions()",
        ".backend_dialogue_topic_options()",
        ".backend_investigation_cases()",
        ".backend_investigation_journal()",
        ".backend_investigation_leads()",
        ".backend_investigation_actions()",
        ".backend_investigation_action_outcomes()",
        ".backend_case_site_pins()",
        ".party_journey()",
        ".inventory_object()",
        ".inventory_containment()",
        ".inventory_item_amount()",
        ".party_item_amount()",
        ".container_liquid()",
    ] {
        assert!(
            production.contains(required),
            "missing safe projection {required}"
        );
    }
    for forbidden in [
        ".quest_generation_authority()",
        ".case_authority()",
        ".case_finale_authority()",
        ".hostile_group_authority()",
        ".mission_authority()",
        ".party_journey_route()",
        ".case_outcome()",
        ".case_outcome_fact()",
    ] {
        assert!(
            !production.contains(forbidden),
            "runner must not import private authority {forbidden}"
        );
    }
    assert!(!production.contains("receive_local_problem_rumor_then"));
}

#[test]
fn activity_detail_exposes_public_pre_post_values_and_signed_deltas() {
    let mut schedule = medical_rest_schedule();
    schedule.labor_minutes = 480;
    let before = ActivityObservation {
        personal_gold_coin: 4,
        condition_status: DomainIncapacitationStatus::Ready,
        hunger: 0.125,
        thirst: 0.25,
        food_days: 1.0,
        water_days: 2.0,
        visible_food_kcal: 2_000.0,
        visible_water_ml: 4_000.0,
        elapsed_minutes: 1_440,
    };
    let after = ActivityObservation {
        personal_gold_coin: 9,
        condition_status: DomainIncapacitationStatus::Staggered,
        hunger: 0.5,
        thirst: 0.125,
        food_days: 0.0,
        water_days: 0.25,
        visible_food_kcal: 0.0,
        visible_water_ml: 500.0,
        elapsed_minutes: 2_880,
    };
    let diagnostic = ActivityExecutionDiagnostic {
        plan: ActivityPlanDiagnostic {
            preferred_activity: "Thievery",
            effective_activity: "Labor",
            schedule: &schedule,
            fallback_reason: "crime_disabled_to_labor",
            committed_reserve: 2,
        },
        venue: DomainSettlementActionService::Temple,
    };
    assert_eq!(
        format_activity_detail(diagnostic, &before, &after),
        "outcome=completed;preferred=Thievery;effective=Labor;fallback=crime_disabled_to_labor;venue=temple;committed_reserve=2;schedule=combat:0,carousing:0,apprenticeship:0,profession:0,labor:480,prayer:0,thievery:0,raiding:0;purse_before=4;purse_after=9;purse_delta=+5;condition_before=ready;condition_after=staggered;hunger_before=0.125;hunger_after=0.500;hunger_delta=+0.375;thirst_before=0.250;thirst_after=0.125;thirst_delta=-0.125;food_kcal_before=2000;food_kcal_after=0;food_kcal_delta=-2000.000;water_ml_before=4000;water_ml_after=500;water_ml_delta=-3500.000;elapsed_before=1440;elapsed_after=2880;elapsed_delta=+1440"
    );
    assert_eq!(
        format_failed_activity_detail(diagnostic, &before, "insufficient_visible_resources",),
        "outcome=failed;stage=rest_at_settlement;error_category=insufficient_visible_resources;preferred=Thievery;effective=Labor;fallback=crime_disabled_to_labor;venue=temple;committed_reserve=2;schedule=combat:0,carousing:0,apprenticeship:0,profession:0,labor:480,prayer:0,thievery:0,raiding:0;requested_minutes=1440;purse_before=4;condition_before=ready;hunger_before=0.125;thirst_before=0.250;food_kcal_before=2000;water_ml_before=4000;elapsed_before=1440"
    );
}

#[test]
fn effective_activity_distinguishes_authored_policy_from_safe_fallback() {
    let mut labor = generate_profile(42, 0);
    labor.preferred_activity = ActivityPreference::Labor;
    labor.schedule.labor = 480;
    labor.schedule.prayer = 0;
    let (_, effective, fallback) = activity_schedule_plan(&labor, true, 0, 0, Some(2));
    assert_eq!((effective, fallback), ("Labor", "none"));

    labor.preferred_activity = ActivityPreference::Thievery;
    labor.schedule.labor = 0;
    labor.schedule.thievery = 480;
    let (_, effective, fallback) = activity_schedule_plan(&labor, true, 0, 0, Some(2));
    assert_eq!((effective, fallback), ("Labor", "crime_disabled_to_labor"));
}

#[test]
fn failed_activity_error_classification_never_echoes_raw_backend_text() {
    let raw = "Not enough coin: secret internal reducer context";
    let projection = project_core_loop_failure(&CoreLoopError::Other(raw.into()));
    let category = projection.category.as_str();
    let schedule = medical_rest_schedule();
    let detail = format_failed_activity_detail(
        ActivityExecutionDiagnostic {
            plan: ActivityPlanDiagnostic {
                preferred_activity: "Prayer",
                effective_activity: "Prayer",
                schedule: &schedule,
                fallback_reason: "none",
                committed_reserve: 0,
            },
            venue: DomainSettlementActionService::Temple,
        },
        &ActivityObservation {
            personal_gold_coin: 0,
            condition_status: DomainIncapacitationStatus::Ready,
            hunger: 0.0,
            thirst: 0.0,
            food_days: 0.0,
            water_days: 0.0,
            visible_food_kcal: 0.0,
            visible_water_ml: 0.0,
            elapsed_minutes: 0,
        },
        category,
    );
    assert!(detail.contains("error_category=core_loop_error"));
    assert!(!detail.contains("secret internal reducer context"));
    assert_eq!(
        project_core_loop_failure(&CoreLoopError::Other(
            "simulation settlement offers neither an Inn nor a Temple".into()
        ))
        .category
        .as_str(),
        "core_loop_error"
    );
}

#[test]
fn equipment_trade_failure_is_classified_without_backend_text() {
    let error = CoreLoopError::operation(
        ReducerOperation::PurchasePersonalStorefrontWithPartyStake,
        "hidden provider authority changed",
    );
    let projection = project_core_loop_failure(&error);
    assert_eq!(projection.category.as_str(), "equipment_purchase_failed");
    assert_eq!(
        projection.operation,
        Some(ReducerOperation::PurchasePersonalStorefrontWithPartyStake)
    );
    assert_eq!(
        projection.reason.as_str(),
        "equipment_storefront_trade_failed"
    );
    assert!(!projection.message.contains("hidden provider authority"));
}

#[test]
fn failure_artifact_version_nine_has_an_exact_full_wire_golden() {
    let artifact = CoreLoopFailureArtifact {
        schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
        category: "investigation_temporally_unavailable".into(),
        message: "A projected investigation action was attempted outside its learned time window."
            .into(),
        operation: Some("perform_investigation_action".into()),
        reason_code: "investigation_night_window".into(),
        fixture_disease: DEFAULT_SIMULATION_DISEASE.into(),
        metrics: CoreLoopMetrics::default(),
        quest_coverage: None,
        total_event_count: 1,
        trace_truncated: false,
        trace: vec![CoreLoopEvent {
            sequence: 1,
            agent_id: 0,
            kind: CoreLoopEventKind::QuestDecision,
            detail: "quest_path=generated_discovery;fallback=none".into(),
        }],
        final_agents: Vec::new(),
    };
    let value = serde_json::to_value(artifact).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/core-loop-failure-v9.json")).unwrap();
    assert_eq!(value, expected);
}

#[test]
fn semantic_event_dedup_uses_typed_subject_not_detail_prose() {
    let first = CoreLoopEventPayload::direct_contract(
        CoreLoopEventKind::AcceptContract,
        "party:1",
        "contract:1",
        "first wording",
    );
    let reworded = CoreLoopEventPayload::direct_contract(
        CoreLoopEventKind::AcceptContract,
        "party:1",
        "contract:1",
        "entirely different wording",
    );
    let other_contract = CoreLoopEventPayload::direct_contract(
        CoreLoopEventKind::AcceptContract,
        "party:1",
        "contract:2",
        "first wording",
    );
    let first_key = first.semantic_key(7);
    assert!(is_duplicate_semantic_event(
        Some(&first_key),
        &reworded.semantic_key(7)
    ));
    assert!(!is_duplicate_semantic_event(
        Some(&first_key),
        &other_contract.semantic_key(7)
    ));

    let medical_suppression = CoreLoopEventPayload::agent(
        CoreLoopEventKind::QuestSuppressed,
        "medical wording",
    )
    .semantic_key(7);
    let generated_case_suppression = CoreLoopEventPayload::generated_case(
        CoreLoopEventKind::QuestSuppressed,
        "party:1",
        "case:1",
        "generated-case wording",
    )
    .semantic_key(7);
    assert!(!is_duplicate_semantic_event(
        Some(&medical_suppression),
        &generated_case_suppression
    ));

    let repeatable = CoreLoopEventPayload::direct_contract(
        CoreLoopEventKind::Travel,
        "party:1",
        "contract:1",
        "outbound",
    )
    .semantic_key(7);
    assert!(!is_duplicate_semantic_event(Some(&repeatable), &repeatable));
}

#[test]
fn typed_event_payload_preserves_public_event_wire() {
    let event = CoreLoopEventPayload::generated_case(
        CoreLoopEventKind::GeneratedQuestDiscovered,
        "party:1",
        "case:1",
        "display wording",
    )
    .into_public(11, 4);
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        serde_json::json!({
            "sequence": 11,
            "agent_id": 4,
            "kind": "generated_quest_discovered",
            "detail": "display wording"
        })
    );
}

#[test]
fn typed_failure_recorder_preserves_v9_wire_without_detail_prose() {
    let path = std::env::temp_dir().join(format!(
        "adventuresim-typed-failure-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = std::fs::remove_file(&path);
    let recorder = FailureRecorder::new(Some(path.clone()), DEFAULT_SIMULATION_DISEASE.into());
    let coded = adventuresim_core::reducer_error::coded_reducer_error(
        ReducerErrorCode::InvestigationNightWindow,
        "secret wording that may change freely",
    );
    let error =
        CoreLoopError::reducer_rejected(ReducerOperation::PerformInvestigationAction, coded);
    recorder.record(error.clone());
    recorder.write(&error.to_string()).unwrap();

    let artifact: CoreLoopFailureArtifact =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(artifact.schema_version, 9);
    assert_eq!(artifact.category, "investigation_temporally_unavailable");
    assert_eq!(
        artifact.operation.as_deref(),
        Some("perform_investigation_action")
    );
    assert_eq!(artifact.reason_code, "investigation_night_window");
    assert!(!artifact.message.contains("secret wording"));
}

fn quest_coverage_report() -> CoreLoopReport {
    let metrics = CoreLoopMetrics {
        quests_attempted: 2,
        direct_contracts_attempted: 1,
        direct_contracts_completed: 1,
        generated_case_intakes: 1,
        generated_discovery_actions_fruitful: 1,
        generated_quests_discovered: 1,
        ..CoreLoopMetrics::default()
    };
    CoreLoopReport {
        format_version: crate::FORMAT_VERSION,
        backend_kind: "spacetimedb".into(),
        seed: 42,
        server_origin: "http://127.0.0.1:3000".into(),
        database: "adventuresim-sim-test".into(),
        run_nonce: "quest-coverage-test-0001".into(),
        fixture_disease: DEFAULT_SIMULATION_DISEASE.into(),
        deployment_identity_note: "test".into(),
        world_artifact_id: None,
        world_manifest_digest: None,
        starting_settlement_id: "settlement:test".into(),
        profiles: Vec::new(),
        metrics,
        quest_coverage: Some(QuestCoverageEvidence {
            direct_contract_id: "contract:fixture".into(),
            generated_case_id: "case:fixture".into(),
            direct_leader_id: 1,
            generated_leader_id: 2,
            direct_party_id: "party:direct".into(),
            generated_party_id: "party:generated".into(),
            direct_accepted: true,
            direct_traveled: true,
            direct_encountered: true,
            direct_reported: true,
            direct_safely_abandoned: false,
            generated_intake: true,
            generated_discovered: true,
            generated_completed: true,
        }),
        trace: Vec::new(),
        trace_truncated: false,
        total_event_count: 0,
        final_agents: vec![FinalAgentState {
            agent_id: 0,
            character_id: 1,
            gold: 0,
            equipment_item_ids: Vec::new(),
            capability_summary: "ready".into(),
            condition_status: DomainIncapacitationStatus::Ready,
            thermal: 0.0,
            wetness_bps: 0,
            thermal_strain: 0,
            ammunition: 20,
            carried_load_kg: 0.0,
            carry_capacity_kg: 20.0,
            encumbrance_remaining_bps: 10_000,
            equipment_ready: true,
            party_tent_quantity: 1,
            worst_equipment_condition: 1.0,
            outstanding_repair_orders: 0,
            alive: true,
            elapsed_minutes: 0,
            personal_gold_coin: 0,
            party_treasury: 0,
            party_stake: 0,
            hunger: 0.0,
            thirst: 0.0,
            food_days: 1.0,
            water_days: 1.0,
            visible_food_kcal: 2_000.0,
            visible_water_ml: 2_000.0,
            settlement_id: Some("settlement:test".into()),
            current_case_site_id: None,
            journey_destination: None,
            symptomatic: false,
            critical: false,
            settlement_services: vec!["Inn".into()],
            visible_herbalist_quote: None,
            visible_inn_full_board_cost: Some(1),
        }],
        elapsed_game_minutes: 0,
        policy_seed_note: "test".into(),
    }
}

#[test]
fn quest_coverage_gate_requires_both_paths_and_safe_final_state() {
    let mut report = quest_coverage_report();
    assert_eq!(validate_quest_coverage(&report), Ok(()));

    report.metrics.quests_attempted = 1;
    assert_eq!(
        validate_quest_coverage(&report).unwrap_err().metric(),
        QuestCoverageMetric::QuestsAttempted
    );
    report.metrics.quests_attempted = 2;
    report.quest_coverage.as_mut().unwrap().generated_completed = false;
    assert_eq!(
        validate_quest_coverage(&report).unwrap_err().metric(),
        QuestCoverageMetric::FixtureGeneratedCompleted
    );
    report.quest_coverage.as_mut().unwrap().generated_completed = true;
    report.final_agents[0].journey_destination = Some("case-site:test".into());
    assert_eq!(
        validate_quest_coverage(&report).unwrap_err().metric(),
        QuestCoverageMetric::FinalAgentsNotStranded
    );
}

#[test]
fn safe_abandonment_resolves_the_direct_lane_but_not_generated_completion() {
    let mut report = quest_coverage_report();
    report.metrics.direct_contracts_completed = 10;
    report.metrics.generated_quests_completed = 10;
    let coverage = report.quest_coverage.as_mut().unwrap();
    coverage.direct_reported = false;
    coverage.direct_safely_abandoned = true;
    coverage.generated_completed = false;
    assert_eq!(
        validate_quest_coverage(&report).unwrap_err().metric(),
        QuestCoverageMetric::FixtureGeneratedCompleted
    );
}

#[test]
fn quest_coverage_failure_artifact_names_unmet_metric() {
    let report = quest_coverage_report();
    let path = std::env::temp_dir().join(format!(
        "adventuresim-quest-coverage-{}-{}.json",
        std::process::id(),
        report.seed
    ));
    let _ = std::fs::remove_file(&path);
    let mut failed_report = report.clone();
    failed_report
        .quest_coverage
        .as_mut()
        .unwrap()
        .generated_intake = false;
    let error = validate_quest_coverage(&failed_report).unwrap_err();
    write_quest_coverage_failure(&report, &path, &error).unwrap();
    let artifact: CoreLoopFailureArtifact =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(artifact.category, "quest_coverage_acceptance");
    assert_eq!(artifact.reason_code, "fixture_generated_intake");
    assert_eq!(artifact.message, error.to_string());
    assert_eq!(artifact.metrics, report.metrics);
}

#[test]
fn expected_investigation_failure_is_allowlisted_without_raw_text() {
    let coded = adventuresim_core::reducer_error::coded_reducer_error(
        ReducerErrorCode::InvestigationNightWindow,
        "hidden authority with freely changeable wording",
    );
    let error =
        CoreLoopError::reducer_rejected(ReducerOperation::PerformInvestigationAction, coded);
    let projection = project_core_loop_failure(&error);
    assert_eq!(
        projection.category.as_str(),
        "investigation_temporally_unavailable"
    );
    assert_eq!(
        projection.operation,
        Some(ReducerOperation::PerformInvestigationAction)
    );
    assert_eq!(projection.reason.as_str(), "investigation_night_window");
    assert!(!projection.message.contains("hidden authority"));
}

#[test]
fn invalid_investigation_route_is_allowlisted_without_raw_text() {
    let coded = adventuresim_core::reducer_error::coded_reducer_error(
        ReducerErrorCode::InvestigationRouteInvalid,
        "hidden canonical action with freely changeable wording",
    );
    let error =
        CoreLoopError::reducer_rejected(ReducerOperation::PerformInvestigationAction, coded);
    let projection = project_core_loop_failure(&error);
    assert_eq!(projection.category.as_str(), "invalid_investigation_route");
    assert_eq!(
        projection.operation,
        Some(ReducerOperation::PerformInvestigationAction)
    );
    assert_eq!(projection.reason.as_str(), "invalid_investigation_route");
    assert!(!projection.message.contains("hidden canonical action"));
    assert!(!projection.message.contains(&error.to_string()));
}

#[test]
fn victim_cohort_state_changes_are_narrowly_classified_without_raw_text() {
    for detail in [
        "Victim cohort authority no longer exists",
        "Victim cohort target is unavailable",
        "Victim cohort target moved from the learned location",
        "Victim cohort profile no longer matches its authority",
        "Victim cohort NPC no longer exists",
        "Victim cohort NPC no longer has a visible demographic",
        "Victim cohort target moved, changed, or is unavailable",
    ] {
        let coded = adventuresim_core::reducer_error::coded_reducer_error(
            ReducerErrorCode::VictimCohortStateChanged,
            detail,
        );
        let error =
            CoreLoopError::reducer_rejected(ReducerOperation::PerformInvestigationAction, coded);
        let projection = project_core_loop_failure(&error);
        assert_eq!(projection.category.as_str(), "investigation_state_changed");
        assert_eq!(
            projection.operation,
            Some(ReducerOperation::PerformInvestigationAction)
        );
        assert_eq!(
            projection.reason.as_str(),
            "investigation_victim_cohort_state_changed"
        );
        assert!(!projection.message.contains(detail));
    }
    assert_eq!(
        parse_reducer_error(
            "perform_investigation_action failed: Victim cohort belongs to another case"
        ),
        None
    );
    assert_eq!(
        parse_reducer_error("choose_dialogue_topic failed: Victim cohort target is unavailable"),
        None
    );
}

#[test]
fn generated_action_trace_uses_subject_and_public_attempt_evidence() {
    let source = LIVE_CORE_SOURCE;
    let production = source
        .split("#[cfg(test)]")
        .next()
        .expect("production live core");
    let advance = source
        .split("fn advance_generated_case_inner")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(super) fn turn_in_ready_direct_contract")
                .next()
        })
        .expect("generated case driver");
    assert!(advance.contains("emit_generated_investigation_attempt"));
    assert!(source.contains(
        "actor_time={actor_time};party_time_min={party_time_min};party_time_max={party_time_max}"
    ));
    assert!(advance.contains("outcome_class={outcome_class}"));
    assert!(advance.contains("requested_min_minutes={}"));
    assert!(advance.contains("actual_minutes={actual_minutes}"));
    assert!(!advance.contains("\"case={};title={};action={};method={};summary={};outcome={}\""));
    assert!(advance.contains("self.call(result)?"));
    assert!(!advance.contains("victim_cohort_state_changed_failure"));
    assert!(!advance.contains("bounded_retry"));
    assert!(!production.contains("generated_investigation_retries"));
}

#[test]
fn discovery_contact_failures_are_sanitized_without_reducer_text() {
    let error = CoreLoopError::operation(
        ReducerOperation::StartDiscoveryDialogue,
        "public discovery contact failed; hidden authority",
    );
    let projection = project_core_loop_failure(&error);
    assert_eq!(projection.category.as_str(), "discovery_contact_failed");
    assert_eq!(
        projection.operation,
        Some(ReducerOperation::StartDiscoveryDialogue)
    );
    assert_eq!(projection.reason.as_str(), "discovery_contact_failed");
    assert!(!projection.message.contains("hidden authority"));
}

#[test]
fn journey_camp_failures_are_allowlisted_without_raw_text() {
    let coded = adventuresim_core::reducer_error::coded_reducer_error(
        ReducerErrorCode::JourneyDaylightWindowRequired,
        "hidden route authority with freely changeable wording",
    );
    let temporal = CoreLoopError::reducer_rejected(ReducerOperation::ContinueCampTravel, coded);
    let projection = project_core_loop_failure(&temporal);
    assert_eq!(
        projection.category.as_str(),
        "journey_temporally_unavailable"
    );
    assert_eq!(
        projection.operation,
        Some(ReducerOperation::ContinueCampTravel)
    );
    assert_eq!(
        project_core_loop_failure(&CoreLoopError::operation(
            ReducerOperation::RestAtCamp,
            "journey camp projection is incoherent; hidden details"
        ))
        .operation,
        Some(ReducerOperation::RestAtCamp)
    );
    assert_eq!(
        projection.reason.as_str(),
        "journey_daylight_window_rest_required"
    );
    assert!(!projection.message.contains("hidden route authority"));

    let incoherent = CoreLoopError::Other(
        "journey camp projection is incoherent: hidden itinerary implementation".into(),
    );
    let projection = project_core_loop_failure(&incoherent);
    assert_eq!(projection.category.as_str(), "core_loop_error");
    assert_eq!(projection.reason.as_str(), "unclassified_core_loop_error");
    assert!(
        !projection
            .message
            .contains("hidden itinerary implementation")
    );

    let purchase = CoreLoopError::operation(
        ReducerOperation::PurchaseJourneyProvisions,
        "Merchant service provider is not available; hidden provider",
    );
    let projection = project_core_loop_failure(&purchase);
    assert_eq!(
        projection.category.as_str(),
        "journey_provision_purchase_failed"
    );
    assert_eq!(
        projection.operation,
        Some(ReducerOperation::PurchaseJourneyProvisions)
    );
    assert_eq!(
        projection.reason.as_str(),
        "journey_provision_purchase_failed"
    );
    assert!(!projection.message.contains("hidden provider"));

    let held = CoreLoopError::operation(
        ReducerOperation::TravelCamps,
        "journey has no ready, asymptomatic, noncritical actor; hidden health authority",
    );
    let projection = project_core_loop_failure(&held);
    assert_eq!(projection.category.as_str(), "journey_travel_failed");
    assert_eq!(projection.operation, Some(ReducerOperation::TravelCamps));
    assert_eq!(projection.reason.as_str(), "journey_travel_reducer_failed");
    assert!(!projection.message.contains("hidden health authority"));

    let travel = CoreLoopError::operation(
        ReducerOperation::ReturnToSettlement,
        "hidden journey authority panic",
    );
    let projection = project_core_loop_failure(&travel);
    assert_eq!(projection.category.as_str(), "journey_travel_failed");
    assert_eq!(
        projection.operation,
        Some(ReducerOperation::ReturnToSettlement)
    );
    assert_eq!(projection.reason.as_str(), "journey_travel_reducer_failed");
    assert!(
        !projection
            .message
            .contains("hidden journey authority panic")
    );
}

#[test]
fn projected_night_wait_hints_are_strictly_bounded() {
    assert_eq!(
        projected_investigation_wait_minutes(
            InvestigationActionUnavailableReason::NightWindow,
            840,
        ),
        Some(840)
    );
    assert_eq!(
        projected_investigation_wait_minutes(
            InvestigationActionUnavailableReason::ContactScheduleWindow,
            420,
        ),
        Some(420)
    );
    assert_eq!(
        projected_investigation_wait_minutes(
            InvestigationActionUnavailableReason::TravelRequired,
            840,
        ),
        None
    );
    assert_eq!(
        projected_investigation_wait_minutes(InvestigationActionUnavailableReason::NightWindow, 0),
        None
    );
    assert_eq!(
        projected_investigation_wait_minutes(
            InvestigationActionUnavailableReason::NightWindow,
            1_441,
        ),
        None
    );

    let stable_keys = [
        (
            InvestigationActionUnavailableReason::PartyNotReady,
            "party_not_ready",
        ),
        (
            InvestigationActionUnavailableReason::TravelRequired,
            "travel_required",
        ),
        (
            InvestigationActionUnavailableReason::NightWindow,
            "night_window",
        ),
        (
            InvestigationActionUnavailableReason::TargetChanged,
            "target_changed",
        ),
        (
            InvestigationActionUnavailableReason::ContactScheduleWindow,
            "contact_schedule_window",
        ),
        (
            InvestigationActionUnavailableReason::ContactNotPresent,
            "contact_not_present",
        ),
        (
            InvestigationActionUnavailableReason::CharacterUnavailable,
            "character_unavailable",
        ),
        (
            InvestigationActionUnavailableReason::PartyRequired,
            "party_required",
        ),
    ];
    for (reason, expected) in stable_keys {
        assert_eq!(investigation_unavailable_reason_key(reason), expected);
    }
}

#[test]
fn contact_schedule_recheck_uses_typed_identity_and_public_presence() {
    let action = BackendInvestigationAction {
        owner_character_id: 1,
        case_id: "case".into(),
        action_id: "action".into(),
        method: "mutable wording is irrelevant".into(),
        expected_version: 0,
        summary: String::new(),
        known_prerequisites: String::new(),
        duration_min_minutes: 15,
        duration_max_minutes: 45,
        uncertainty_bps: 0,
        skill_contributions: String::new(),
        weather_available: true,
        contact_character_id: Some(9),
        required_case_site_id: None,
        availability: InvestigationActionAvailability::Available,
        unavailable_reason: String::new(),
    };
    let presence = SettlementResidentPresence {
        character_id: 9,
        settlement_id: "ironforge".into(),
        location_id: "keep".into(),
        start_minute: 240,
        end_minute: 960,
        is_default: true,
        context_suppressed: false,
        health_suppressed: false,
    };
    assert_eq!(
        current_contact_schedule_wait_minutes(&action, [presence.clone()], 77),
        Some(163)
    );
    assert_eq!(
        current_contact_schedule_wait_minutes(&action, [presence], 300),
        Some(0)
    );
}

#[test]
fn repeated_daily_quest_decisions_are_not_semantic_duplicate_failures() {
    assert!(event_is_repeatable(&CoreLoopEventKind::QuestDecision));
    assert!(!event_is_repeatable(&CoreLoopEventKind::AcceptContract));
}
#[test]
fn investigation_action_replans_depend_only_on_typed_codes() {
    for detail in ["contact absent", "wording changed completely"] {
        let coded = adventuresim_core::reducer_error::coded_reducer_error(
            ReducerErrorCode::InvestigationActionUnavailable,
            detail,
        );
        assert_eq!(
            investigation_action_replan_reason(&CoreLoopError::reducer_rejected(
                ReducerOperation::PerformInvestigationAction,
                coded,
            )),
            Some(InvestigationActionReplanReason::Unavailable)
        );
    }
    assert_eq!(
        investigation_action_replan_reason(&CoreLoopError::reducer_rejected(
            ReducerOperation::PerformInvestigationAction,
            "investigation_action_unavailable without a typed envelope",
        )),
        None
    );
}
