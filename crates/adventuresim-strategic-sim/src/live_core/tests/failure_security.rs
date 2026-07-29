    #[test]
    fn generated_runner_subscribes_only_to_public_projection_inventory() {
        let source = LIVE_CORE_SOURCE;
        let production = source.split("#[cfg(test)]").next().unwrap();
        for required in [
            ".backend_settlement_npcs()",
            ".settlement_npc_presence()",
            ".backend_dialogue_sessions()",
            ".backend_dialogue_topic_options()",
            ".backend_investigation_cases()",
            ".backend_investigation_journal()",
            ".backend_investigation_leads()",
            ".backend_investigation_actions()",
            ".backend_investigation_action_outcomes()",
            ".backend_case_site_pins()",
            ".party_journey()",
            ".party_journey_itinerary()",
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
            condition_status: "ready".into(),
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
            condition_status: "recovering".into(),
            hunger: 0.5,
            thirst: 0.125,
            food_days: 0.0,
            water_days: 0.25,
            visible_food_kcal: 0.0,
            visible_water_ml: 500.0,
            elapsed_minutes: 2_880,
        };
        assert_eq!(
            format_activity_detail(
                "Thievery",
                "Labor",
                &schedule,
                SettlementActivityVenue::Temple,
                "crime_disabled_to_labor",
                2,
                &before,
                &after,
            ),
            "outcome=completed;preferred=Thievery;effective=Labor;fallback=crime_disabled_to_labor;venue=temple;committed_reserve=2;schedule=combat:0,carousing:0,apprenticeship:0,profession:0,labor:480,prayer:0,thievery:0,raiding:0;purse_before=4;purse_after=9;purse_delta=+5;condition_before=ready;condition_after=recovering;hunger_before=0.125;hunger_after=0.500;hunger_delta=+0.375;thirst_before=0.250;thirst_after=0.125;thirst_delta=-0.125;food_kcal_before=2000;food_kcal_after=0;food_kcal_delta=-2000.000;water_ml_before=4000;water_ml_after=500;water_ml_delta=-3500.000;elapsed_before=1440;elapsed_after=2880;elapsed_delta=+1440"
        );
        assert_eq!(
            format_failed_activity_detail(
                "Thievery",
                "Labor",
                &schedule,
                SettlementActivityVenue::Temple,
                "crime_disabled_to_labor",
                2,
                &before,
                "insufficient_visible_resources",
            ),
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
        let category = safe_core_loop_failure(raw).0;
        let detail = format_failed_activity_detail(
            "Prayer",
            "Prayer",
            &medical_rest_schedule(),
            SettlementActivityVenue::Temple,
            "none",
            0,
            &ActivityObservation {
                personal_gold_coin: 0,
                condition_status: "ready".into(),
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
        assert!(detail.contains("error_category=insufficient_visible_resources"));
        assert!(!detail.contains("secret internal reducer context"));
        assert_eq!(
            safe_core_loop_failure("simulation settlement offers neither an Inn nor a Temple").0,
            "rest_service_unavailable"
        );
    }

    #[test]
    fn failure_artifact_version_five_serializes_safe_operation_context() {
        let artifact = CoreLoopFailureArtifact {
            schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
            category: "investigation_temporally_unavailable".into(),
            message:
                "A projected investigation action was attempted outside its learned time window."
                    .into(),
            operation: Some("perform_investigation_action".into()),
            reason_code: "investigation_night_window".into(),
            metrics: CoreLoopMetrics::default(),
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
        assert_eq!(value["schema_version"], serde_json::json!(5));
        assert_eq!(
            value["operation"],
            serde_json::json!("perform_investigation_action")
        );
        assert_eq!(
            value["reason_code"],
            serde_json::json!("investigation_night_window")
        );
        assert_eq!(value["trace"][0]["kind"], "quest_decision");
    }

    #[test]
    fn expected_investigation_failure_is_allowlisted_without_raw_text() {
        let raw = "perform_investigation_action failed: The learned pattern requires acting during the nighttime window; hidden authority";
        let (category, message) = safe_core_loop_failure(raw);
        assert_eq!(category, "investigation_temporally_unavailable");
        assert_eq!(
            safe_failure_operation(raw),
            Some("perform_investigation_action")
        );
        assert_eq!(
            safe_failure_reason_code(raw, category),
            "investigation_night_window"
        );
        assert!(!message.contains("hidden authority"));
    }

    #[test]
    fn invalid_investigation_route_is_allowlisted_without_raw_text() {
        let raw = "perform_investigation_action failed: Investigation track origin no longer matches the projected route; hidden canonical action";
        let (category, message) = safe_core_loop_failure(raw);
        assert_eq!(category, "invalid_investigation_route");
        assert_eq!(
            safe_failure_operation(raw),
            Some("perform_investigation_action")
        );
        assert_eq!(
            safe_failure_reason_code(raw, category),
            "invalid_investigation_route"
        );
        assert!(!message.contains("hidden canonical action"));
        assert!(!message.contains(raw));
    }

    #[test]
    fn victim_cohort_state_changes_are_narrowly_classified_without_raw_text() {
        for detail in VICTIM_COHORT_STATE_CHANGED_DETAILS {
            let raw = format!("perform_investigation_action failed: {detail}");
            let (category, message) = safe_core_loop_failure(&raw);
            assert_eq!(category, "investigation_state_changed");
            assert_eq!(
                safe_failure_operation(&raw),
                Some("perform_investigation_action")
            );
            assert_eq!(
                safe_failure_reason_code(&raw, category),
                "investigation_victim_cohort_state_changed"
            );
            assert!(!message.contains(detail));
        }
        assert!(!victim_cohort_state_changed_failure(
            "perform_investigation_action failed: Victim cohort belongs to another case"
        ));
        assert!(!victim_cohort_state_changed_failure(
            "choose_dialogue_topic failed: Victim cohort target is unavailable"
        ));
    }

    #[test]
    fn generated_action_trace_uses_subject_and_public_attempt_evidence() {
        let source = LIVE_CORE_SOURCE;
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production live core");
        let advance = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        assert!(advance.contains("emit_generated_investigation_attempt"));
        assert!(source.contains("actor_time={actor_time};party_time_min={party_time_min};party_time_max={party_time_max}"));
        assert!(
            advance.contains("\"case={};subject={};action={};method={};summary={};outcome={}\"")
        );
        assert!(
            !advance.contains("\"case={};title={};action={};method={};summary={};outcome={}\"")
        );
        assert!(advance.contains("identical_pending_subscription"));
        assert!(advance.contains("Defer once so the next cycle chooses"));
        assert!(!advance.contains("bounded_retry"));
        assert!(!production.contains("generated_investigation_retries"));
    }

    #[test]
    fn discovery_contact_failures_are_sanitized_without_reducer_text() {
        let raw =
            "start_discovery_dialogue failed: public discovery contact failed; hidden authority";
        let (category, message) = safe_core_loop_failure(raw);
        assert_eq!(category, "discovery_contact_failed");
        assert_eq!(
            safe_failure_operation(raw),
            Some("start_discovery_dialogue")
        );
        assert_eq!(
            safe_failure_reason_code(raw, category),
            "discovery_contact_failed"
        );
        assert!(!message.contains("hidden authority"));
    }

    #[test]
    fn journey_camp_failures_are_allowlisted_without_raw_text() {
        let temporal = "continue_camp_travel failed: Rest until the party reaches its next daylight walking window; hidden route authority";
        let (category, message) = safe_core_loop_failure(temporal);
        assert_eq!(category, "journey_temporally_unavailable");
        assert_eq!(
            safe_failure_operation(temporal),
            Some("continue_camp_travel")
        );
        assert_eq!(
            safe_failure_operation(
                "rest_at_camp failed: journey camp projection is incoherent; hidden details"
            ),
            Some("rest_at_camp")
        );
        assert_eq!(
            safe_failure_reason_code(temporal, category),
            "journey_daylight_window_rest_required"
        );
        assert!(!message.contains("hidden route authority"));

        let incoherent = "journey camp projection is incoherent: hidden itinerary implementation";
        let (category, message) = safe_core_loop_failure(incoherent);
        assert_eq!(category, "journey_projection_inconsistent");
        assert_eq!(
            safe_failure_reason_code(incoherent, category),
            "journey_projection_inconsistent"
        );
        assert!(!message.contains("hidden itinerary implementation"));

        let purchase = "purchase_journey_provisions failed: Merchant service provider is not available; hidden provider";
        let (category, message) = safe_core_loop_failure(purchase);
        assert_eq!(category, "journey_provision_purchase_failed");
        assert_eq!(
            safe_failure_operation(purchase),
            Some("purchase_journey_provisions")
        );
        assert_eq!(
            safe_failure_reason_code(purchase, category),
            "journey_provision_purchase_failed"
        );
        assert!(!message.contains("hidden provider"));

        let held = "travel_camps failed: journey has no ready, asymptomatic, noncritical actor; hidden health authority";
        let (category, message) = safe_core_loop_failure(held);
        assert_eq!(category, "journey_held_no_actionable_actor");
        assert_eq!(safe_failure_operation(held), Some("travel_camps"));
        assert_eq!(
            safe_failure_reason_code(held, category),
            "journey_held_no_actionable_actor"
        );
        assert!(!message.contains("hidden health authority"));
    }

    #[test]
    fn projected_night_wait_hints_are_strictly_bounded() {
        assert_eq!(
            projected_investigation_wait_minutes("night_window", 840),
            Some(840)
        );
        assert_eq!(
            projected_investigation_wait_minutes("travel_required", 840),
            None
        );
        assert_eq!(
            projected_investigation_wait_minutes("night_window", 0),
            None
        );
        assert_eq!(
            projected_investigation_wait_minutes("night_window", 1_441),
            None
        );
    }

    #[test]
    fn repeated_daily_quest_decisions_are_not_semantic_duplicate_failures() {
        assert!(event_is_repeatable(&CoreLoopEventKind::QuestDecision));
        assert!(!event_is_repeatable(&CoreLoopEventKind::AcceptContract));
    }
