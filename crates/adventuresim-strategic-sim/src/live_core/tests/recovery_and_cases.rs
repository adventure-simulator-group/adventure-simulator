    #[test]
    fn off_settlement_recovery_is_bounded_public_and_precedes_quest_selection() {
        let recovering = ExpeditionMemberObservation {
            agent_id: 0,
            character_id: 7,
            alive: true,
            condition_status: "incapacitated".into(),
            hunger: 0.1,
            thirst: 0.2,
            food_days: 3.0,
            water_days: 3.0,
            thermal: 0.0,
            wetness_bps: 0,
            thermal_strain: 0,
            ammunition: 20,
            carried_load_kg: 80.0,
            carry_capacity_kg: 150.0,
            encumbrance_remaining_bps: 4_667,
            equipment_ready: true,
            party_tent_quantity: 1,
            symptomatic: false,
            critical: false,
            elapsed_minutes: 1_440,
        };
        assert!(expedition_member_needs_recovery(&recovering));
        assert!(!expedition_member_needs_recovery(
            &ExpeditionMemberObservation {
                condition_status: "ready".into(),
                ..recovering.clone()
            }
        ));
        assert_eq!(MAX_EXPEDITION_RECOVERY_RESTS, 2);
        assert!(!expedition_party_can_resume(&[recovering.clone()]));
        assert!(expedition_party_can_resume(&[
            ExpeditionMemberObservation {
                condition_status: "ready".into(),
                ..recovering.clone()
            }
        ]));
        assert!(!expedition_party_can_resume(&[]));
        let one_day = ExpeditionSuppliesObservation {
            stored_food_kcal: adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY,
            portable_water_ml: adventuresim_core::provisioning::STRATEGIC_TRAVEL_WATER_ML_PER_DAY,
        };
        assert!(expedition_supplies_cover_one_rest_day(
            &[recovering.clone()],
            one_day
        ));
        assert!(!expedition_supplies_cover_one_rest_day(
            &[recovering.clone(), recovering.clone()],
            one_day
        ));
        assert_eq!(
            select_expedition_encounter_choice(
                &["attack".into(), "run".into(), "detour".into()],
                true,
            )
            .map(|choice| choice.choice),
            Some("detour".into())
        );

        let source = LIVE_CORE_SOURCE;
        let production = source.split("#[cfg(test)]").next().unwrap();
        let cycle = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        assert!(cycle.find("recover_or_evacuate_off_settlement") < cycle.find("QuestDecision"));
        let recovery = production
            .split("fn recover_or_evacuate_off_settlement")
            .nth(1)
            .and_then(|tail| tail.split("fn owned_open_generated_cases").next())
            .expect("expedition recovery policy");
        for public_input in [
            "expedition_member_observations",
            "expedition_supplies",
            "public_expedition_return_settlement",
            "party_journey()",
            "backend_case_site_pins()",
        ] {
            assert!(production.contains(public_input));
        }
        assert!(recovery.contains("MAX_EXPEDITION_RECOVERY_RESTS"));
        assert!(recovery.contains("expedition_supplies_cover_one_rest_day"));
        assert!(recovery.contains("expedition_party_can_resume"));
        assert!(recovery.contains("evacuation_stalled"));
        assert!(production.contains("\"ready_companion\""));
        assert!(recovery.contains("travel_to_settlement_then"));
        assert!(!recovery.contains("infection_episode"));
        assert!(!recovery.contains("party_journey_route"));
        let actor = production
            .split("fn expedition_recovery_actor")
            .nth(1)
            .and_then(|tail| tail.split("fn public_expedition_return_settlement").next())
            .expect("recovery actor");
        assert!(!actor.contains("current_leader("));
        assert!(production.contains("final bounded rescue pass"));
    }

    #[test]
    fn passive_no_actionable_recovery_is_camp_only_typed_and_publicly_gated() {
        let staggered_leader = ExpeditionMemberObservation {
            agent_id: 0,
            character_id: 7,
            alive: true,
            condition_status: "staggered".into(),
            hunger: 0.1,
            thirst: 0.2,
            food_days: 3.0,
            water_days: 3.0,
            thermal: 0.0,
            wetness_bps: 0,
            thermal_strain: 0,
            ammunition: 20,
            carried_load_kg: 80.0,
            carry_capacity_kg: 150.0,
            encumbrance_remaining_bps: 4_667,
            equipment_ready: true,
            party_tent_quantity: 1,
            symptomatic: false,
            critical: false,
            elapsed_minutes: 1_440,
        };
        let incapacitated_companion = ExpeditionMemberObservation {
            agent_id: 1,
            character_id: 8,
            condition_status: "incapacitated".into(),
            ..staggered_leader.clone()
        };
        let members = [staggered_leader.clone(), incapacitated_companion.clone()];
        let supplies = ExpeditionSuppliesObservation {
            stored_food_kcal: 2.0 * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY,
            portable_water_ml: 2.0
                * adventuresim_core::provisioning::STRATEGIC_TRAVEL_WATER_ML_PER_DAY,
        };
        assert!(passive_no_actionable_rest_allowed(
            &members, supplies, true, true, 7, false
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, false, true, 7, false
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, true, false, 7, false
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, true, true, 99, false
        ));
        assert!(passive_no_actionable_rest_allowed(
            &[
                ExpeditionMemberObservation {
                    condition_status: "ready".into(),
                    symptomatic: true,
                    ..staggered_leader.clone()
                },
                incapacitated_companion.clone(),
            ],
            supplies,
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &[
                ExpeditionMemberObservation {
                    critical: true,
                    ..staggered_leader
                },
                incapacitated_companion,
            ],
            supplies,
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &[
                ExpeditionMemberObservation {
                    condition_status: "unavailable".into(),
                    ..members[0].clone()
                },
                members[1].clone(),
            ],
            supplies,
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members,
            ExpeditionSuppliesObservation {
                stored_food_kcal: supplies.stored_food_kcal - 1.0,
                ..supplies
            },
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, true, true, 7, true,
        ));

        let source = LIVE_CORE_SOURCE;
        let selector = source
            .split("fn expedition_recovery_rest_actor")
            .nth(1)
            .and_then(|tail| tail.split("fn perform_expedition_recovery_rest").next())
            .expect("typed recovery-rest actor selector");
        assert!(selector.contains("ExpeditionRecoveryRestActor::Actionable"));
        assert!(selector.contains("ExpeditionRecoveryRestActor::PassiveNoActionable"));
        assert!(selector.contains("passive_no_actionable_rest_allowed"));
        assert!(selector.contains("party_has_unresolved_public_encounter"));
        assert!(selector.contains("public_active_camp_observation"));

        let camp_predicate = source
            .split("fn public_active_camp_observation")
            .nth(1)
            .and_then(|tail| {
                tail.split("fn party_has_unresolved_public_encounter")
                    .next()
            })
            .expect("coherent public active-camp predicate");
        assert!(camp_predicate.contains("let [journey] = journeys.as_slice()"));
        assert!(camp_predicate.contains("let [itinerary] = itineraries.as_slice()"));
        assert!(camp_predicate.contains("&journey.destination != camp_destination"));
        assert!(
            camp_predicate
                .contains("journey.completed_elapsed_minutes >= journey.total_elapsed_minutes")
        );
        assert!(camp_predicate.contains("projected_camp_rest_minutes("));

        let passive_call_boundary = source
            .split("fn perform_expedition_recovery_rest")
            .nth(1)
            .and_then(|tail| tail.split("fn public_expedition_return_settlement").next())
            .expect("passive recovery-rest call boundary");
        assert!(passive_call_boundary.contains("PassiveNoActionable"));
        assert!(passive_call_boundary.contains("rest_at_camp_with_party_shelter"));
        for forbidden in [
            "continue_camp_travel",
            "resolve_strategic_encounter",
            "travel_to_case_site",
            "travel_to_settlement",
            "perform_investigation_action",
            "accept_contract",
            "report_contract",
            "vote_for_party_leader",
        ] {
            assert!(
                !passive_call_boundary.contains(forbidden),
                "passive actor reached {forbidden}"
            );
        }

        let recovery = source
            .split("fn recover_or_evacuate_off_settlement")
            .nth(1)
            .and_then(|tail| tail.split("fn generated_case_status").next())
            .expect("expedition recovery policy");
        assert!(recovery.contains("self.expedition_recovery_rest_actor(party_id)"));
        assert!(recovery.contains("self.perform_expedition_recovery_rest(rest_actor)"));
        assert!(recovery.contains("passive_no_actionable_rest_"));
        assert!(!recovery.contains(".rest_at_camp_then("));
        assert!(recovery.contains("journey_held_unresolved_encounter"));
        assert!(recovery.contains("journey_held_incoherent_public_camp"));

        let before = [
            ExpeditionMemberObservation {
                elapsed_minutes: 100,
                ..members[0].clone()
            },
            ExpeditionMemberObservation {
                elapsed_minutes: 120,
                ..members[1].clone()
            },
        ];
        let after = [
            ExpeditionMemberObservation {
                elapsed_minutes: 160,
                ..members[0].clone()
            },
            ExpeditionMemberObservation {
                elapsed_minutes: 180,
                ..members[1].clone()
            },
        ];
        assert_eq!(expedition_elapsed_delta(&before, &after), 60);
        assert!(recovery.contains("requested_minutes={EXPEDITION_RECOVERY_REST_MINUTES}"));
        assert!(recovery.contains("actual_elapsed_minutes={actual_elapsed_minutes}"));
        assert!(
            recovery.find("expedition_passive_rest_attempts").unwrap()
                < recovery
                    .find("perform_expedition_recovery_rest(rest_actor)")
                    .unwrap()
        );
        assert!(
            recovery.find("expedition_passive_rest_minutes").unwrap()
                > recovery.find("actual_elapsed_minutes =").unwrap()
        );
    }

    #[test]
    fn recovery_outcomes_resume_same_cycle_but_consume_evacuation_or_hold() {
        assert_ne!(
            ExpeditionRecoveryOutcome::Resumed,
            ExpeditionRecoveryOutcome::Evacuated
        );
        let source = LIVE_CORE_SOURCE;
        let production = source.split("#[cfg(test)]").next().unwrap();
        let cycle = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        assert!(cycle.contains("let recovery_outcome ="));
        assert!(cycle.contains("let recovery_started_in_budget ="));
        assert!(cycle.contains("if !recovery_started_in_budget"));
        assert!(cycle.contains("recovery_outcome == ExpeditionRecoveryOutcome::Resumed"));
        assert!(cycle.contains("&& recovery_started_in_budget"));
        assert!(cycle.contains("ExpeditionRecoveryOutcome::Evacuated =>"));
        assert!(cycle.contains("ExpeditionRecoveryOutcome::Held =>"));
        let resumed = cycle.find("ExpeditionRecoveryOutcome::Resumed").unwrap();
        let quest_decision = cycle.find("CoreLoopEventKind::QuestDecision").unwrap();
        assert!(resumed < quest_decision);
        assert!(
            cycle.find("if !recovery_started_in_budget").unwrap()
                < cycle.find("recover_or_evacuate_off_settlement").unwrap()
        );
    }

    #[test]
    fn public_journeys_resume_generated_and_direct_state_without_duplicate_metrics() {
        let source = LIVE_CORE_SOURCE;
        let production = source.split("#[cfg(test)]").next().unwrap();
        let core_loop = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        assert!(
            core_loop.find("continue_public_active_journey").unwrap()
                < core_loop.find("CoreLoopEventKind::QuestDecision").unwrap()
        );
        assert!(core_loop.contains("\"generated_open_case\""));
        assert!(core_loop.contains("\"direct_contract_continuation\""));
        assert!(core_loop.contains("runner.active_direct_contract(&party)"));

        let active_contract = production
            .split("fn active_direct_contract")
            .nth(1)
            .and_then(|tail| tail.split("fn personal_gold").next())
            .expect("public active contract selector");
        for public_identity in [
            "party.active_contract_id",
            ".backend_contracts()",
            "contract.accepted_by",
            "ContractStatus::Accepted | ContractStatus::ReadyToReport",
        ] {
            assert!(
                active_contract.contains(public_identity),
                "{public_identity}"
            );
        }

        let direct = production
            .split("fn cycle")
            .nth(1)
            .and_then(|tail| tail.split("fn try_upgrade").next())
            .expect("direct contract driver");
        let attempt_metrics = direct.find("self.metrics.quests_attempted += 1").unwrap();
        let new_contract_guard = direct.find("if !resuming_contract").unwrap();
        assert!(new_contract_guard < attempt_metrics);
        assert!(direct.contains("if quest.status == ContractStatus::ReadyToReport"));
        assert!(direct.contains("already_at_case_site"));

        let turn_in = production
            .split("fn turn_in_ready_direct_contract")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("direct contract turn-in");
        assert!(turn_in.contains("direct_contract_report_arrival_not_proven"));
        assert!(turn_in.contains("ContractStatus::ReadyToReport"));
        assert_eq!(
            turn_in
                .matches("self.metrics.quests_completed += 1")
                .count(),
            1
        );
        assert_eq!(
            turn_in
                .matches("self.metrics.direct_contracts_completed += 1")
                .count(),
            1
        );
    }

    #[test]
    fn recovery_reselects_each_rest_actor_and_held_only_cycles_do_not_advance_time() {
        let source = LIVE_CORE_SOURCE;
        let production = source.split("#[cfg(test)]").next().unwrap();
        let recovery = production
            .split("fn recover_or_evacuate_off_settlement")
            .nth(1)
            .and_then(|tail| tail.split("fn owned_open_generated_cases").next())
            .expect("expedition recovery policy");
        let loop_start = recovery
            .find("for rest_ordinal in 1..=MAX_EXPEDITION_RECOVERY_RESTS")
            .unwrap();
        let reselection = recovery[loop_start..]
            .find("self.expedition_recovery_rest_actor(party_id)")
            .unwrap();
        let rest_call = recovery[loop_start..]
            .find("self.perform_expedition_recovery_rest(rest_actor)")
            .unwrap();
        assert!(reselection < rest_call);
        assert!(recovery.contains("field_recovery_actor_reselection"));

        let core_loop = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        let held_branch = core_loop
            .split("ExpeditionRecoveryOutcome::Held =>")
            .nth(1)
            .and_then(|tail| tail.split("let Some((leader, leader_agent))").next())
            .expect("held branch");
        assert!(held_branch.contains("held = true"));
        assert!(!held_branch.contains("active = true;\n                    continue"));
        assert!(held_branch.contains("public_party_elapsed_max(party_id) > party_time_before"));
        assert!(core_loop.contains("if active {"));
        assert!(core_loop.contains("advance_simulation_world_time"));
        assert!(core_loop.contains("if !active && held"));
    }

    #[test]
    fn generated_case_tracking_is_owner_scoped_and_intake_drives_attempts() {
        let mut seen = HashSet::new();
        assert!(seen.insert((7_u64, "same-case".to_owned())));
        assert!(seen.insert((8_u64, "same-case".to_owned())));
        assert!(!seen.insert((7_u64, "same-case".to_owned())));
        let mut finance_blocks = HashMap::new();
        finance_blocks.insert(
            ("party".to_owned(), 7_u64, "same-case".to_owned()),
            (12_u64, 3_u64),
        );
        assert!(
            finance_blocks
                .get(&("party".to_owned(), 8_u64, "same-case".to_owned()))
                .is_none()
        );

        let source = LIVE_CORE_SOURCE;
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("generated_seen_cases: HashSet<(u64, String)>"));
        assert!(production.contains("generated_terminal_cases: HashSet<(u64, String)>"));
        assert!(
            production
                .contains("generated_finance_blocks: HashMap<(String, u64, String), (u64, u64)>")
        );
        assert!(production.contains("CoreLoopEventKind::GeneratedCaseIntake"));
        assert!(production.contains("source == \"owner_projection_continuation\""));
        assert!(production.contains(
            "self.metrics.quests_attempted = self.metrics.quests_attempted.saturating_add(1)"
        ));
        assert!(!production.contains("generated_case_owners: HashMap<String, u64>"));
    }

    #[test]
    fn quest_preparation_revalidates_public_owner_leadership_and_party_safety() {
        let source = LIVE_CORE_SOURCE;
        let helper = source
            .split("fn refreshed_safe_party_for_owner")
            .nth(1)
            .and_then(|tail| tail.split("fn emit_generated_investigation_attempt").next())
            .expect("post-preparation safety gate");
        assert!(helper.contains("self.observe_deaths()"));
        assert!(helper.contains("current_leader != owner_character_id"));
        assert!(helper.contains("self.unsafe_party_agents(&party_agents)"));
        assert!(helper.contains("party.id != party_id"));

        let preflight = source
            .split("fn synchronize_generated_party_for_action")
            .nth(1)
            .and_then(|tail| tail.split("fn refreshed_safe_party_for_owner").next())
            .expect("generated case preflight");
        assert!(preflight.contains(
            "refreshed_safe_party_for_owner(party_id, owner_character_id)"
        ));

        let generated = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        assert!(generated.contains("synchronize_generated_party_for_action"));
    }

    #[test]
    fn generated_actions_sync_then_recover_then_revalidate_public_clocks() {
        let source = LIVE_CORE_SOURCE;
        let preflight = source
            .split("fn synchronize_generated_party_for_action")
            .nth(1)
            .and_then(|tail| tail.split("fn refreshed_safe_party_for_owner").next())
            .expect("generated party action preflight");
        let sync = preflight
            .find("synchronize_party_for_activity_then")
            .unwrap();
        let deaths = preflight.find("self.observe_deaths()").unwrap();
        let medical = preflight.find("self.ensure_medically_safe").unwrap();
        let resync = preflight
            .find("resynchronize_party_after_generated_preflight")
            .unwrap();
        let safe = preflight.find("refreshed_safe_party_for_owner").unwrap();
        let clocks = preflight.find("public_party_clocks_aligned").unwrap();
        assert!(sync < deaths);
        assert!(deaths < medical);
        assert!(medical < resync);
        assert!(resync < safe);
        assert!(preflight.contains("let mut party_medically_ready = true"));
        assert!(preflight.contains("party_medically_ready = false"));
        assert!(preflight.contains("if !party_medically_ready"));
        assert!(medical < safe);
        assert!(safe < clocks);

        let driver = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        let initial_sync = driver
            .find("synchronize_generated_party_for_action")
            .unwrap();
        let action_projection = driver.find("backend_investigation_actions()").unwrap();
        let action_reducer = driver.find("perform_investigation_action_then").unwrap();
        assert!(initial_sync < action_projection);
        assert!(action_projection < action_reducer);
        assert!(!driver.contains("investigation_victim_cohort_state_changed"));
    }

    #[test]
    fn final_agent_diagnostics_expose_public_remote_and_illness_state() {
        let value = serde_json::to_value(CoreLoopFailureAgent {
            agent_id: 0,
            character_id: 7,
            alive: true,
            condition_status: "ready".into(),
            thermal: 0.0,
            wetness_bps: 250,
            thermal_strain: -120,
            ammunition: 20,
            carried_load_kg: 84.0,
            carry_capacity_kg: 150.0,
            encumbrance_remaining_bps: 4_400,
            equipment_ready: true,
            party_tent_quantity: 1,
            hunger: 0.0,
            thirst: 0.0,
            food_days: 0.0,
            water_days: 0.0,
            visible_food_kcal: 0.0,
            visible_water_ml: 0.0,
            personal_gold_coin: 0,
            settlement_id: None,
            current_case_site_id: Some("site:known".into()),
            journey_destination: Some("settlement:return".into()),
            symptomatic: true,
            critical: false,
            settlement_services: Vec::new(),
            visible_herbalist_quote: None,
            visible_inn_full_board_cost: None,
        })
        .unwrap();
        for field in [
            "current_case_site_id",
            "journey_destination",
            "symptomatic",
            "critical",
            "thermal",
            "wetness_bps",
            "thermal_strain",
            "ammunition",
            "carried_load_kg",
            "carry_capacity_kg",
            "encumbrance_remaining_bps",
            "equipment_ready",
            "party_tent_quantity",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
    }

    #[test]
    fn expedition_diagnostics_include_each_public_health_and_supply_boundary() {
        let source = LIVE_CORE_SOURCE;
        let diagnostics = source
            .split("fn emit_expedition_diagnostics")
            .nth(1)
            .and_then(|tail| tail.split("fn expedition_recovery_actor").next())
            .expect("expedition diagnostics");
        for field in [
            "condition_before",
            "condition_after",
            "hunger_before",
            "hunger_after",
            "thirst_before",
            "thirst_after",
            "symptomatic_before",
            "symptomatic_after",
            "critical_before",
            "critical_after",
            "thermal_before",
            "thermal_after",
            "wetness_bps_before",
            "wetness_bps_after",
            "thermal_strain_before",
            "thermal_strain_after",
            "ammo_before",
            "ammo_after",
            "carried_load_kg_before",
            "carry_capacity_kg_before",
            "equipment_ready_before",
            "party_tent_quantity_before",
            "elapsed_delta",
            "stored_food_kcal_consumed",
            "portable_water_ml_consumed",
        ] {
            assert!(diagnostics.contains(field), "missing {field}");
        }
        assert!(source.contains(
            "quest_suppressed_member_not_ready_after_leg;plan=off_settlement_recovery_next_cycle"
        ));
        assert!(source.contains(".any(expedition_member_needs_recovery)"));
    }

    #[test]
    fn generated_event_fields_are_single_line_and_bounded() {
        let field = bounded_event_field(&format!("title;\n{}", "x".repeat(400)));
        assert!(!field.contains(';'));
        assert!(!field.contains('\n'));
        assert_eq!(field.chars().count(), 240);
    }

    #[test]
    fn generated_case_state_machine_is_bounded_and_precedes_direct_contracts() {
        assert!(MAX_GENERATED_CASE_STEPS_PER_CYCLE <= 32);
        let source = LIVE_CORE_SOURCE;
        let loop_start = source.find("let quest_path = if").unwrap();
        let decision =
            &source[loop_start..source[loop_start..].find("runner.event(").unwrap() + loop_start];
        assert!(
            decision.find("!open_generated_cases.is_empty()").unwrap()
                < decision.find("direct_quest_chosen").unwrap()
        );
        let driver = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        assert!(driver.contains("action.unavailable_reason_code"));
        assert!(driver.contains("action.wait_minutes"));
        assert!(driver.contains("wait_for_generated_investigation_window"));
        assert!(!driver.contains("action.unavailable_reason.contains"));
    }
    #[test]
    fn settlement_recovery_treats_visible_nonhealing_wounds_and_bounds_paid_rest() {
        let production = LIVE_CORE_SOURCE.split("#[cfg(test)]").next().unwrap();
        let first_aid = production
            .split("fn apply_visible_first_aid")
            .nth(1)
            .and_then(|tail| tail.split("pub(super) fn personal_gold").next())
            .expect("visible first-aid policy");
        assert!(first_aid.contains("limb_injury()"));
        assert!(first_aid.contains("injury.cut_damage > 0.0 && !injury.bandaged"));
        assert!(first_aid.contains("injury.fracture_damage > 0.0"));
        assert!(first_aid.contains("treat_limb_then"));
        assert!(!first_aid.contains("infection_episode"));

        let recovery = production
            .split("pub(super) fn ensure_medically_safe")
            .nth(1)
            .expect("settlement recovery policy");
        assert!(recovery.contains("apply_visible_first_aid"));
        assert!(recovery.contains("withdraw_stake_for_personal_purchase"));
        assert!(recovery.contains("natural_rest_not_improving_public_condition"));
        assert!(recovery.contains("nonprogressing_natural_rests >= 2"));
    }
