    #[test]
    fn refuses_non_loopback_and_shared_database() {
        let mut config = CoreLoopConfig {
            host: "https://example.com".into(),
            database: "adventuresim-stdb-module".into(),
            seed: 1,
            population: 2,
            cycles: 1,
            duration_days: 1,
            party_size: 2,
            run_nonce: "unit-test-nonce-0001".into(),
            use_imported_world: false,
            expected_world_manifest_digest: None,
            failure_output: None,
        };
        assert!(config.validate().is_err());
        config.host = "http://127.0.0.1:3000".into();
        assert!(config.validate().is_err());
        config.database = "adventuresim-sim-test-1".into();
        assert!(config.validate().is_ok());
        for spoofed in [
            "http://localhost.example.com:3000",
            "http://127.0.0.1@evil.example:3000",
            "http://localhost:3000/path",
            "http://localhost:3000?database=shared",
            "http://user:pass@localhost:3000",
            "https://localhost:3000",
        ] {
            config.host = spoofed.into();
            assert!(config.validate().is_err(), "accepted spoofed URL {spoofed}");
        }
    }

    #[test]
    fn bootstrap_token_is_required_and_bounded() {
        assert!(bootstrap_token_from_environment(None).is_err());
        assert!(bootstrap_token_from_environment(Some("short".into())).is_err());
        assert!(bootstrap_token_from_environment(Some("z".repeat(64))).is_err());
        assert_eq!(
            bootstrap_token_from_environment(Some("a".repeat(64))).unwrap(),
            "a".repeat(64)
        );
    }

    #[test]
    fn dead_or_replaced_leader_is_never_a_policy_actor() {
        assert!(leader_is_actionable("party", 7, 7, true, Some("party")));
        assert!(!leader_is_actionable("party", 7, 7, false, Some("party")));
        assert!(!leader_is_actionable("party", 8, 7, true, Some("party")));
        assert!(!leader_is_actionable("party", 7, 7, true, Some("other")));
    }

    #[test]
    fn medical_rest_schedule_suspends_but_does_not_replace_profile_policy() {
        let profile = generate_profile(42, 0);
        let saved = live_schedule(&profile);
        let rest = medical_rest_schedule();
        assert_eq!(
            [
                rest.combat_training_minutes,
                rest.carousing_minutes,
                rest.apprenticeship_minutes,
                rest.profession_practice_minutes,
                rest.labor_minutes,
                rest.prayer_minutes,
                rest.thievery_minutes,
                rest.raiding_minutes,
            ]
            .into_iter()
            .sum::<u16>(),
            0
        );
        assert_eq!(live_schedule(&profile), saved);
        assert_ne!(saved, rest);
    }

    #[test]
    fn unaffordable_symptomatic_treatment_falls_back_to_natural_rest() {
        assert_eq!(
            choose_medical_action("recovering", true, true, true, 6, Some(5), Some(true), None),
            (MedicalChoice::RestNaturally, "observable_care_unaffordable")
        );
    }

    #[test]
    fn nonsymptomatic_convalescence_does_not_buy_medication() {
        assert_eq!(
            choose_medical_action(
                "recovering",
                false,
                true,
                true,
                100,
                Some(5),
                Some(false),
                Some(false)
            ),
            (
                MedicalChoice::RestNaturally,
                "convalescing_without_symptoms"
            )
        );
    }

    #[test]
    fn affordable_symptomatic_treatment_buys_then_rests() {
        assert_eq!(
            choose_medical_action(
                "recovering",
                true,
                true,
                true,
                7,
                Some(5),
                Some(true),
                Some(true)
            ),
            (MedicalChoice::BuyAndRest, "symptomatic_and_affordable")
        );
    }

    #[test]
    fn equipment_spending_reserves_one_observable_medical_course() {
        assert_eq!(spending_budget_after_medical_reserve(20, Some(7)), 13);
        assert_eq!(spending_budget_after_medical_reserve(5, Some(7)), 0);
        assert_eq!(spending_budget_after_medical_reserve(20, None), 20);
        assert!(equipment_spend_is_still_affordable(20, Some(7), 13));
        assert!(!equipment_spend_is_still_affordable(19, Some(7), 13));
        assert!(!equipment_spend_is_still_affordable(20, Some(8), 13));
    }

    #[test]
    fn medical_rest_venue_accounts_for_visible_inn_cost() {
        assert_eq!(
            affordable_medical_rest_venue(true, false, false, 7, 5),
            Some(true)
        );
        assert_eq!(
            affordable_medical_rest_venue(true, false, false, 6, 5),
            None
        );
        assert_eq!(
            affordable_medical_rest_venue(true, true, true, 5, 5),
            Some(false),
            "the free temple is preferred when both public venues exist"
        );
        assert_eq!(
            affordable_medical_rest_venue(true, true, false, 7, 5),
            Some(true),
            "an inn is required when visible supplies do not cover a temple rest"
        );
    }

    #[test]
    fn settlement_rest_sponsorship_is_public_bounded_and_self_payment_first() {
        let source = LIVE_CORE_SOURCE;
        let selector = source
            .split("fn settlement_rest_sponsor")
            .nth(1)
            .and_then(|tail| tail.split("fn activity_observation").next())
            .expect("settlement rest sponsor selector");
        for public_input in [
            "personal_gold(patient_id)",
            ".party_member()",
            ".character()",
            "current_settlement_id",
            "observable_medical_reserve",
            ".party_inventory_item()",
            ".party_stake()",
        ] {
            assert!(selector.contains(public_input), "missing {public_input}");
        }
        assert!(selector.contains("spendable >= sponsor_quote"));
        assert!(selector.contains("Reverse(option.spendable)"));
        assert!(!selector.contains("infection_episode"));

        let recovery = source
            .split("fn ensure_medically_safe")
            .nth(1)
            .and_then(|tail| tail.split("fn settlement_activity_day").next())
            .expect("medical recovery driver");
        assert!(recovery.contains(".sponsor_party_member_inn_rest_then("));
        assert!(recovery.contains("sponsored_settlement_rest=completed"));
        assert!(recovery.contains("exposure=not_publicly_projected"));
        assert!(recovery.contains("emergency_temple_rest"));
        assert!(recovery.contains("actual_elapsed_minutes={actual_rest_minutes}"));
        assert!(recovery.contains("saturating_sub(rest_started_at)"));
        assert!(recovery.contains("sponsored_settlement_rest_requested_minutes"));
        assert!(recovery.contains("sponsored_settlement_rest_elapsed_minutes"));
        assert!(recovery.contains("MedicalChoice::RestNaturally => natural_rest_venue"));
        assert!(recovery.contains("MedicalChoice::BuyAndRest => medicated_rest_venue"));
        assert!(recovery.contains("selected_rest_venue.map_or(\"unavailable\""));
        assert!(!recovery.contains("infection_episode"));
    }

    #[test]
    fn medical_quote_requires_player_visible_herbalist_stock() {
        assert!(observable_herbalist_stocks_medication(true, true, true));
        assert!(!observable_herbalist_stocks_medication(false, true, true));
        assert!(!observable_herbalist_stocks_medication(true, false, true));
        assert!(!observable_herbalist_stocks_medication(true, true, false));
    }

    #[test]
    fn smithing_decisions_quantize_float_noise_at_one_thousandth() {
        assert_eq!(quantize_smithing_condition(0.020_000_1), 20);
        assert_eq!(quantize_smithing_condition(0.019_999_9), 20);
        assert_eq!(quantize_smithing_condition(f32::NAN), 0);
        assert_eq!(quantize_smithing_condition(f32::INFINITY), 1_000);
    }

    #[test]
    fn report_metrics_expose_encounter_frequency_choices_losses_and_wipes() {
        let metrics = CoreLoopMetrics {
            direct_contracts_attempted: 4,
            direct_contracts_completed: 2,
            generated_case_intakes: 3,
            generated_case_continuations: 1,
            generated_quests_discovered: 3,
            generated_quests_completed: 2,
            generated_quests_closed_externally: 1,
            generated_investigation_actions: 7,
            generated_investigation_waits: 2,
            generated_investigation_wait_minutes: 480,
            generated_investigation_replans: 2,
            generated_witness_dialogues: 4,
            generated_discovery_actions_attempted: 8,
            generated_discovery_actions_fruitful: 3,
            generated_discovery_decisions_unproductive: 2,
            generated_discovery_public_backoff_suppressions: 5,
            expedition_recovery_plans: 2,
            expedition_recovery_rests: 3,
            expedition_evacuations: 1,
            expedition_resumes: 1,
            expedition_holds: 2,
            expedition_passive_rest_attempts: 2,
            expedition_passive_rest_minutes: 1_500,
            generated_unique_party_cases_discovered: 3,
            generated_exact_site_ready: 2,
            generated_finance_blocked_cycles: 5,
            generated_case_site_traveled: 1,
            journey_provision_purchases: 1,
            journey_provision_party_gold_spent: 115,
            sponsored_settlement_rests: 2,
            sponsored_settlement_rest_gold_spent: 4,
            sponsored_settlement_rest_requested_minutes: 2_880,
            sponsored_settlement_rest_elapsed_minutes: 2_100,
            encounters: 5,
            encounter_sneaks: 1,
            encounter_detours: 1,
            encounter_attacks: 1,
            encounter_runs: 1,
            encounter_surrenders: 1,
            encounter_escape_eligible: 3,
            encounter_escape_ineligible: 2,
            encounter_surrender_items_lost: 4,
            encounter_surrender_value_lost: 90,
            encounter_defeats: 2,
            encounter_wipes: 1,
            ..CoreLoopMetrics::default()
        };
        let value = serde_json::to_value(metrics).unwrap();
        for field in [
            "direct_contracts_attempted",
            "direct_contracts_completed",
            "generated_case_intakes",
            "generated_case_continuations",
            "generated_quests_discovered",
            "generated_quests_completed",
            "generated_quests_closed_externally",
            "generated_investigation_actions",
            "generated_investigation_waits",
            "generated_investigation_wait_minutes",
            "generated_investigation_replans",
            "generated_witness_dialogues",
            "generated_discovery_actions_attempted",
            "generated_discovery_actions_fruitful",
            "generated_discovery_decisions_unproductive",
            "generated_discovery_public_backoff_suppressions",
            "expedition_recovery_plans",
            "expedition_recovery_rests",
            "expedition_evacuations",
            "expedition_resumes",
            "expedition_holds",
            "expedition_passive_rest_attempts",
            "expedition_passive_rest_minutes",
            "generated_unique_party_cases_discovered",
            "generated_exact_site_ready",
            "generated_finance_blocked_cycles",
            "generated_case_site_traveled",
            "journey_provision_purchases",
            "journey_provision_party_gold_spent",
            "sponsored_settlement_rests",
            "sponsored_settlement_rest_gold_spent",
            "sponsored_settlement_rest_requested_minutes",
            "sponsored_settlement_rest_elapsed_minutes",
            "encounters",
            "encounter_sneaks",
            "encounter_detours",
            "encounter_attacks",
            "encounter_runs",
            "encounter_surrenders",
            "encounter_escape_eligible",
            "encounter_escape_ineligible",
            "encounter_surrender_items_lost",
            "encounter_surrender_value_lost",
            "encounter_defeats",
            "encounter_wipes",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
    }
