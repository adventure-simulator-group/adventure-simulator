#[test]
fn contract_and_religion_lifecycle_guards_are_explicit() {
    let strategic = STRATEGIC_SOURCE;
    let disband = strategic
        .split("pub fn disband_party")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("disband reducer");
    assert!(disband.contains("ContractStatus::ReadyToReport"));
    let normalize = strategic
        .split("fn normalize_and_elect_party_leader")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("leadership normalization");
    assert!(normalize.contains("ContractStatus::Accepted | ContractStatus::ReadyToReport"));
    assert!(!normalize.contains("ContractStatus::Paid"));

    let condition = include_str!("../../condition.rs");
    let religion = condition
        .split("pub fn set_character_religion")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("religion reducer");
    assert!(religion.contains("require_strategic_character_authority"));
}

#[test]
fn encounter_body_weight_is_authoritative_but_sanitized() {
    assert_eq!(sanitized_encounter_body_weight(55.0), 55.0);
    assert_eq!(sanitized_encounter_body_weight(300.0), 300.0);
    assert_eq!(sanitized_encounter_body_weight(0.0), 70.0);
    assert_eq!(sanitized_encounter_body_weight(f32::NAN), 70.0);
}

#[test]
fn unready_members_keep_their_burden_but_lose_carrying_capacity() {
    assert_eq!(carrying_capacity_multiplier_for_condition("ready"), 1.0);
    assert_eq!(carrying_capacity_multiplier_for_condition("staggered"), 0.5);
    assert_eq!(
        carrying_capacity_multiplier_for_condition("incapacitated"),
        0.0
    );
    assert_eq!(
        carrying_capacity_multiplier_for_condition("unavailable"),
        0.0
    );

    let unchanged_body_and_load_burden = 95.0;
    let ready_capacity = 100.0 * carrying_capacity_multiplier_for_condition("ready");
    let incapacitated_capacity =
        100.0 * carrying_capacity_multiplier_for_condition("incapacitated");
    assert!(ready_capacity >= unchanged_body_and_load_burden);
    assert!(incapacitated_capacity < unchanged_body_and_load_burden);
}

#[test]
fn forged_recruitment_mutations_must_cross_character_authority() {
    let source = STRATEGIC_SOURCE;
    for function in [
        "create_recruitment_role",
        "update_recruitment_role",
        "delete_recruitment_role",
        "save_recruitment_role",
        "rename_saved_recruitment_role",
        "delete_saved_recruitment_role",
        "request_to_join_party",
        "request_general_party_join",
        "accept_party_join_request",
        "reject_party_join_request",
        "update_party_check_targets",
    ] {
        let body = source
            .split(&format!("pub fn {function}"))
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .unwrap_or_else(|| panic!("{function} reducer body"));
        assert!(
            body.contains("require_strategic_character_authority"),
            "{function} trusts a caller-provided character ID"
        );
    }
}

#[test]
fn incident_sources_are_retry_stable_and_group_resolution_is_exact() {
    let first = activity_incident_source_id("raiding", "party", "town", 7, 1440);
    let retry = activity_incident_source_id("raiding", "party", "town", 7, 1440);
    let next = activity_incident_source_id("raiding", "party", "town", 7, 1441);
    assert_eq!(first, retry);
    assert_ne!(first, next);
    assert!(incident_group_matches(
        IncidentStatus::Pending,
        "group:a",
        "group:a"
    ));
    assert!(!incident_group_matches(
        IncidentStatus::Pending,
        "group:a",
        "group:b"
    ));
    assert!(!incident_group_matches(
        IncidentStatus::Resolved,
        "group:a",
        "group:a"
    ));
}

#[test]
fn activity_incidents_are_ordered_and_use_retry_stable_entropy() {
    let source = STRATEGIC_SOURCE;
    let reducer = source
        .split("pub(crate) fn maybe_trigger_activity_incident")
        .nth(1)
        .and_then(|tail| tail.split("fn finish_strategic_incident").next())
        .expect("activity incident reducer");
    let retaliation = reducer.find("risks.raiding_retaliation").unwrap();
    let discovery = reducer.find("risks.thievery_discovery").unwrap();
    let disorder = reducer.find("risks.carousing_disorder").unwrap();
    assert!(retaliation < discovery && discovery < disorder);
    assert!(reducer.contains("adventuresim.activity-incident.v1"));
    assert!(reducer.contains("IncidentKind::CarousingDisorder"));
    assert!(reducer.contains("activity_incident_entropy()"));
    assert!(reducer.contains("private_seed.to_le_bytes()"));
    assert!(reducer.contains("current_case_site_id.is_none()"));
    assert!(reducer.contains("&& has_charge"));
    assert!(reducer.contains("origin_settlement_id"));
    assert!(reducer.contains("current_case_site_id.as_deref()"));
    assert!(source.contains("let current_site = current_case_site_id"));
    assert!(source.contains("site.longitude_e7"));
    assert!(source.contains("site.distance_m"));
    assert!(reducer.contains("snapshot_arrest_charges"));
}

#[test]
fn authority_enforcement_is_charge_scoped_and_resistance_is_retry_stable() {
    let source = STRATEGIC_SOURCE;
    let surrender = source
        .split("pub fn surrender_to_authority")
        .nth(1)
        .and_then(|tail| tail.split("fn finish_strategic_incident").next())
        .expect("surrender reducer");
    assert!(surrender.contains("unsettled_arrest_charges"));
    assert!(surrender.contains("authority_fine"));
    assert!(surrender.contains("settle_offenses(ctx, offenses)"));
    assert!(!surrender.contains("local_reputation"));

    let completion = source
        .split("pub(crate) fn finish_incident_for_hostile_group")
        .nth(1)
        .and_then(|tail| tail.split("fn incident_group_matches").next())
        .expect("incident completion");
    assert!(completion.contains("IncidentKind::AuthorityArrest"));
    assert!(completion.contains("resist-authority:"));
    assert!(completion.contains("resisting_authority"));
}

#[test]
fn case_reputation_separates_canonical_and_public_battle_identity() {
    let source = STRATEGIC_SOURCE;
    let finale = source
        .rsplit("fn execute_case_finale")
        .next()
        .and_then(|tail| tail.split("fn hostile_resolution_for_objective").next())
        .expect("case finale");
    assert!(finale.contains("crate::reputation::award_case_resolution"));
    assert!(finale.contains("&case.id"));
    assert!(finale.contains("&authority.public_case_id"));
    assert!(finale.contains("&authority.settlement_id"));
}

#[test]
fn autoresolve_uses_explicit_mission_and_exactly_once_source_authority() {
    let source = STRATEGIC_SOURCE;
    let body = source
        .split("pub fn autoresolve_mission")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("autoresolve reducer");
    assert!(body.contains("ensure_bound_mission_authority("));
    assert!(body.contains("complete_bound_mission_success("));
    assert!(body.contains("autoresolve_report()"));
    assert!(!body.contains("record_battle_result("));
    let binding = source
        .split("pub(crate) fn ensure_bound_mission_authority")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("mission binding");
    assert!(binding.contains("generated_case_site_combat_eligible("));
    assert!(binding.contains("ContractStatus::Accepted"));
    assert!(binding.contains("party.active_contract_id"));
    let projection = crate::investigation::INVESTIGATION_SOURCE
        .split("fn case_site_presentation_view")
        .nth(1)
        .and_then(|tail| tail.split("fn lead_projects_exact_case_site_pin").next())
        .expect("generated case-site projection");
    assert!(projection.contains("generated_case_site_combat_eligible("));
}

#[test]
fn mission_binding_entropy_and_terminal_replays_fail_closed() {
    let source = STRATEGIC_SOURCE;
    let binding = source
        .split("pub(crate) fn ensure_bound_mission_authority")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("mission binding");
    assert!(binding.contains("existing.status == MissionAttemptStatus::Bound"));
    assert!(binding.contains("already terminal and cannot be reused"));
    assert!(binding.contains("outcome_entropy: ctx.random()"));
    assert!(binding.contains("mission_approach_capability_id("));

    let cancel = source
        .split("pub fn cancel_mission_request")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("cancel reducer");
    let inspect = cancel
        .find("mission_authority()")
        .expect("mission inspected first");
    let request = cancel
        .find("tactical_server_request_authority()")
        .expect("request inspected");
    assert!(inspect < request);
    assert!(cancel.contains("MissionAttemptStatus::Cancelled => return Ok(())"));
    assert!(cancel.contains("mission.status = MissionAttemptStatus::Cancelled"));
}

#[test]
fn mission_gateway_reducers_require_character_authority() {
    let source = STRATEGIC_SOURCE;
    for function in [
        "store_battle_loot",
        "autoresolve_mission",
        "cancel_mission_request",
    ] {
        let body = source
            .split(&format!("pub fn {function}"))
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("reducer body");
        assert!(
            body.contains("require_strategic_character_authority(ctx, character_id)?"),
            "{function} lacks gateway authority"
        );
    }
}

#[test]
fn loot_transfer_rejects_duplicates_and_has_no_unchecked_subtraction() {
    let source = STRATEGIC_SOURCE;
    let body = source
        .split("pub fn store_battle_loot")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("loot reducer");
    assert!(body.contains("Duplicate battle loot IDs"));
    assert!(body.contains("checked_sub"));
    assert!(!body.contains(".unwrap()"));
}

#[test]
fn tracking_is_presentation_only_and_travel_revalidates_exact_knowledge() {
    let source = STRATEGIC_SOURCE;
    let tracking = source
        .split("pub fn track_case_site")
        .nth(1)
        .and_then(|tail| tail.split("pub fn abandon_contract").next())
        .expect("tracking reducer");
    assert!(tracking.contains("exact_case_site_for_observer"));
    assert!(tracking.contains("party_case_site_tracking"));
    assert!(!tracking.contains("accept_contract("));
    assert!(!tracking.contains("active_quest_id"));
    assert!(!tracking.contains("gold_reward"));

    let travel = source
        .split("fn travel_to_case_site_impl")
        .nth(1)
        .and_then(|tail| tail.split("pub fn travel_to_settlement").next())
        .expect("case-site travel implementation");
    assert_eq!(travel.matches("exact_case_site_for_observer").count(), 2);
    assert!(travel.contains("\"case_site\""));
    assert!(travel.contains("expected_settlement_id = party.current_settlement_id.clone()"));
    assert!(travel.contains("expected_case_site_id = party.current_case_site_id.clone()"));
    assert!(travel.contains("JourneyEndpoint::Settlement"));
    assert!(travel.contains("JourneyEndpoint::CaseSite"));
    assert!(travel.contains("origin_coordinates"));
    assert!(travel.contains("departing_settlement"));
    assert!(!travel.contains("site.origin_settlement_id"));
    assert!(!travel.contains("ctx.db.quest().id().find(&case_site_id)"));

    let continuation = source
        .split("pub fn continue_camp_travel")
        .nth(1)
        .and_then(|tail| tail.split("fn ingest_hostile_group_defeat_fact").next())
        .expect("camp travel continuation");
    let case_site_arrival = continuation
        .split("JourneyEndpoint::CaseSite(endpoint)")
        .nth(1)
        .expect("case-site arrival branch");
    assert!(case_site_arrival.contains("set_character_case_site("));
    assert!(case_site_arrival.contains("Some(destination_id.clone())"));
    assert!(case_site_arrival.contains("party.current_case_site_id = Some"));
    assert!(continuation.contains("finish_party_journey(ctx, &party_id)"));
}

#[test]
fn case_contract_and_tactical_authority_are_separated() {
    let source = STRATEGIC_SOURCE;
    let accept = source
        .split("pub fn accept_contract")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("accept contract reducer");
    assert!(accept.contains("contract_authority()"));
    assert!(!accept.contains("case_authority().insert"));
    assert!(!accept.contains("case_authority().delete"));
    assert!(!accept.contains("gold_reward.max"));

    let battle = source
        .split("pub(crate) fn commit_hostile_battle_resolution")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("battle commit");
    assert!(battle.contains("ingest_hostile_group_defeat_fact"));
    assert!(!battle.contains("report_contract("));
    assert!(!battle.contains("credit_party_currency("));
}

#[test]
fn hostile_resolution_contract_distinguishes_results_subjects_and_loot() {
    use super::HostileResolutionKind as H;
    assert!(
        super::validate_hostile_resolution_contract(
            Some(H::Defeated),
            None,
            H::Defeated,
            None,
            true,
        )
        .is_ok()
    );
    assert!(
        super::validate_hostile_resolution_contract(
            Some(H::DrivenOff),
            None,
            H::DrivenOff,
            None,
            false,
        )
        .is_ok()
    );
    assert!(
        super::validate_hostile_resolution_contract(
            Some(H::Captured),
            Some("subject"),
            H::Captured,
            Some("subject"),
            false,
        )
        .is_ok()
    );
    assert!(
        super::validate_hostile_resolution_contract(
            Some(H::Captured),
            Some("subject"),
            H::Defeated,
            None,
            false,
        )
        .is_err()
    );
    assert!(
        super::validate_hostile_resolution_contract(
            Some(H::Captured),
            Some("subject"),
            H::Captured,
            Some("other"),
            false,
        )
        .is_err()
    );
    assert!(
        super::validate_hostile_resolution_contract(
            Some(H::DrivenOff),
            None,
            H::DrivenOff,
            None,
            true,
        )
        .is_err()
    );
    assert!(
        super::validate_hostile_resolution_contract(
            Some(H::Captured),
            Some("subject"),
            H::CaptureTargetKilled,
            Some("subject"),
            false,
        )
        .is_err()
    );
}

#[test]
fn reporting_is_ready_only_and_paid_once() {
    let source = STRATEGIC_SOURCE;
    let report = source
        .split("pub fn report_contract")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("report contract reducer");
    assert!(report.contains("ContractStatus::ReadyToReport"));
    assert!(report.contains("paid_at_minute.is_some()"));
    assert!(report.contains("ContractStatus::Paid"));
    assert!(report.contains("paid_at_minute = Some"));
}

#[test]
fn private_objective_authority_has_only_a_gateway_projection() {
    let source = STRATEGIC_SOURCE;
    for schema in [
        "case_authority",
        "case_outcome",
        "case_outcome_fact",
        "case_custody",
        "contract_authority",
        "mission_authority",
        "mission_approach_capability",
        "mission_outcome_candidate",
    ] {
        assert!(
            source.contains(&format!("#[table(accessor = {schema})]")),
            "{schema} must remain private"
        );
        assert!(
            !source.contains(&format!("#[table(accessor = {schema}, public)]")),
            "{schema} leaked as public"
        );
    }
    assert!(source.contains("#[view(accessor = backend_contracts, public)]"));
    assert!(source.contains("strategic_view_is_gateway(ctx)"));
}

#[test]
fn case_blocker_authority_paths_remain_reachable_and_private() {
    let source = STRATEGIC_SOURCE;
    assert!(source.contains("#[table(accessor = backend_case_battle_authority)]"));
    assert!(source.contains("#[view(accessor = backend_case_battles, public)]"));
    assert!(!source.contains("#[table(accessor = backend_case_battle, public)]"));
    assert!(!source.contains("pub fn interact_with_contract_issuer("));
    let simulated_interaction = source
        .split("pub fn simulate_contract_issuer_interaction")
        .nth(1)
        .and_then(|tail| tail.split("fn consume_contract_interaction").next())
        .unwrap();
    assert!(simulated_interaction.contains("sender_owns_simulation_character(ctx, character_id)"));
    let dialogue_effect = source
        .split("fn apply_dialogue_effect")
        .nth(1)
        .and_then(|tail| tail.split("fn dialogue_service_id").next())
        .unwrap();
    assert!(dialogue_effect.contains("record_dialogue_contract_issuer_interaction"));
    let receipt = source
        .split("pub struct ContractIssuerInteractionReceipt")
        .nth(1)
        .and_then(|tail| tail.split("pub enum IncidentKind").next())
        .unwrap();
    assert!(receipt.contains("pub dialogue_session_id: String"));
    assert!(receipt.contains("pub dialogue_action_id: String"));
    assert!(receipt.contains("pub dialogue_revision: u64"));
    assert!(receipt.contains("pub location_id: String"));

    let accept = source
        .split("pub fn accept_contract")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .unwrap();
    let report = source
        .split("pub fn report_contract")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .unwrap();
    assert!(accept.contains("ContractInteractionStage::Accept"));
    assert!(report.contains("ContractInteractionStage::Report"));
    assert!(report.contains("xp_reward"));
}

#[test]
fn merchant_trade_is_bound_to_a_closed_storefront_and_persistent_provider() {
    use adventuresim_core::settlement_economy::Storefront;

    assert_eq!(
        merchant_storefront("merchants").unwrap(),
        (Storefront::General, "market")
    );
    assert_eq!(
        merchant_storefront("inn").unwrap(),
        (Storefront::Inn, "inn")
    );
    assert!(merchant_storefront("herbalist").is_err());
    assert!(merchant_storefront("../inn").is_err());

    let source = STRATEGIC_SOURCE;
    let trade = source
        .split("fn finalize_storefront_trade_impl")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]\npub fn leave_party").next())
        .expect("storefront trade implementation");
    for authority_check in [
        "character.current_settlement_id.as_deref() != Some(&settlement_id)",
        "storefront_available(",
        "provider.home_settlement_id != settlement_id",
        "provider.service_id != service_id",
        "provider_presence.location_id != location_id",
        "npc_is_present(&provider_presence, problem_minute)",
        "default_merchant_provider(ctx, &settlement_id, &service_id, location_id)?",
        "storefront_stocks(",
        "settlement_allowlist",
        "inventory_food_definition(Some(item.kind), item_id)?",
        "add_to_party_inventory_checked(",
        "add_inventory_item_checked(",
    ] {
        assert!(
            trade.contains(authority_check),
            "missing merchant authority check: {authority_check}"
        );
    }
    assert!(
        !trade.contains("let storefront = match catalog_kind"),
        "the reducer must not infer a different storefront from item kind"
    );

    let party_purchase = source
        .split("fn add_to_party_inventory_checked")
        .nth(1)
        .and_then(|tail| tail.split("fn credit_party_stake").next())
        .expect("party inventory purchase implementation");
    assert!(party_purchase.contains("inventory_food_definition(kind, item_id)?"));
    assert!(party_purchase.contains("for _ in 0..quantity"));
    assert!(party_purchase.contains("create_party_food_lot(ctx, row.id, item_id, 1, minute)"));
}

#[test]
fn merchant_provider_selection_rejects_ambiguous_defaults() {
    assert_eq!(unique_default_merchant_provider([41]).unwrap(), 41);
    assert!(unique_default_merchant_provider(Vec::<u64>::new()).is_err());
    assert!(unique_default_merchant_provider([41, 42]).is_err());
}

#[test]
fn dialogue_objectives_are_knowledge_bound_and_replay_safe() {
    let source = STRATEGIC_SOURCE;
    let issuer = source
        .split("fn issue_dialogue_investigation_bindings")
        .nth(1)
        .and_then(|tail| tail.split("fn case_has_exact_dialogue_provenance").next())
        .expect("eligibility-time dialogue binding issuer");
    let producer = source
        .split("fn apply_dialogue_investigation_action")
        .nth(1)
        .and_then(|tail| tail.split("fn same_location").next())
        .expect("private dialogue objective producer");
    assert!(issuer.contains("local_problem_rumor_delivery()"));
    assert!(issuer.contains("investigation_received_testimony()"));
    assert!(issuer.contains("exact_case_refs"));
    assert!(issuer.contains("dialogue_investigation_binding()"));
    assert!(producer.contains("has no pre-issued binding"));
    assert!(!producer.contains("case_authority().iter()"));
    assert!(!producer.contains(".insert(DialogueInvestigationBinding"));
    assert!(producer.contains("\"dialogue-objective:{}:{action_id}:{}\""));
    assert!(producer.contains("ingest_case_outcome_fact"));
    assert!(!source.contains("pub fn apply_dialogue_investigation_action"));
}

#[test]
fn unrelated_known_case_is_not_dialogue_provenance() {
    let refs = HashSet::from(["session-case".to_string()]);
    let matching = HashSet::from([
        "canonical-session-case".to_string(),
        "session-case".to_string(),
    ]);
    let unrelated = HashSet::from([
        "known-but-unrelated".to_string(),
        "private-known-but-unrelated".to_string(),
    ]);
    assert!(case_refs_have_exact_dialogue_provenance(&matching, &refs));
    assert!(!case_refs_have_exact_dialogue_provenance(&unrelated, &refs));
}

#[test]
fn physical_proof_and_participant_projection_fail_closed() {
    let source = STRATEGIC_SOURCE;
    let proof = source
        .split("fn evidence_can_be_presented")
        .nth(1)
        .and_then(|tail| tail.split("fn dialogue_objective_recipient").next())
        .expect("evidence presentation authority");
    assert!(proof.contains("EvidencePresentationKind::Physical"));
    assert!(proof.contains(".is_some_and(|custody|"));
    assert!(!proof.contains(".is_none_or("));
    assert!(proof.contains("EvidencePresentationKind::Informational"));
    assert!(proof.contains("investigation_evidence_knowledge()"));

    let viewers = player_participant_ids([Some(11), None, Some(22)].into_iter());
    assert!(viewers.contains(&11));
    assert!(
        viewers.contains(&22),
        "joined player must receive view rows"
    );
    assert!(!viewers.contains(&33), "outsider must receive no view rows");
}
