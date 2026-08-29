#[test]
fn non_exact_rows_are_sanitized_without_coordinates() {
    let row = InvestigationLead {
        id: "lead".into(),
        owner_character_id: 1,
        case_id: "case".into(),
        proposition_id: String::new(),
        summary: "somewhere north".into(),
        source_label: "witness".into(),
        confidence_bps: 5000,
        destination_stage: DestinationKnowledgeStage::ApproximateArea,
        directions: "north wood".into(),
        exact_location_id: "hidden-cave".into(),
        latitude_e7: 12,
        longitude_e7: 34,
        witness_name: String::new(),
        witness_description: String::new(),
        witness_occupation_or_relationship: String::new(),
        expected_location: String::new(),
        current_learned_location: String::new(),
        contradiction_group: String::new(),
        corrected_by: String::new(),
        recorded_at: 1,
    };
    let safe = sanitize_lead(row, None);
    assert!(safe.exact_location_id.is_empty());
    assert_eq!((safe.latitude_e7, safe.longitude_e7), (0, 0));
}

#[test]
fn journal_cross_scope_references_degrade_to_safe_chronology() {
    let lead = InvestigationLead {
        id: "lead:one".into(),
        owner_character_id: 7,
        case_id: "case:public".into(),
        proposition_id: String::new(),
        summary: "An uncertain account".into(),
        source_label: "the miller".into(),
        confidence_bps: 5000,
        destination_stage: DestinationKnowledgeStage::Textual,
        directions: String::new(),
        exact_location_id: String::new(),
        latitude_e7: 0,
        longitude_e7: 0,
        witness_name: String::new(),
        witness_description: String::new(),
        witness_occupation_or_relationship: String::new(),
        expected_location: String::new(),
        current_learned_location: String::new(),
        contradiction_group: String::new(),
        corrected_by: "lead:later".into(),
        recorded_at: 1,
    };
    let mut correction = lead.clone();
    correction.id = "lead:later".into();
    correction.corrected_by.clear();
    correction.witness_name = "Greta".into();
    assert_eq!(safe_correction_label(&lead, Some(&correction)), "Greta");
    correction.witness_name.clear();
    correction.source_label = "another witness".into();
    assert_eq!(
        safe_correction_label(&lead, Some(&correction)),
        "another witness"
    );
    correction.owner_character_id = 8;
    assert_eq!(
        safe_correction_label(&lead, Some(&correction)),
        "a later account"
    );
    correction.owner_character_id = 7;
    correction.case_id = "case:other".into();
    assert_eq!(
        safe_correction_label(&lead, Some(&correction)),
        "a later account"
    );
    assert_eq!(safe_correction_label(&lead, None), "a later account");

    let revision = InvestigationBeliefRevision {
        id: "revision:two".into(),
        owner_character_id: 7,
        belief_id: "belief:one".into(),
        revision: 2,
        statement: "Later account".into(),
        confidence_bps: 6000,
        provenance_kind: "witness".into(),
        provenance_label: "Greta".into(),
        supersedes: "revision:one".into(),
        recorded_at: 2,
    };
    let mut earlier = revision.clone();
    earlier.id = "revision:one".into();
    earlier.revision = 1;
    earlier.provenance_label = "the miller".into();
    earlier.supersedes.clear();
    assert_eq!(
        safe_superseded_revision_label(&revision, Some(&earlier)),
        "revision 1 from the miller"
    );
    earlier.belief_id = "belief:other".into();
    assert_eq!(
        safe_superseded_revision_label(&revision, Some(&earlier)),
        "an earlier account"
    );
    earlier.belief_id = revision.belief_id.clone();
    earlier.owner_character_id = 8;
    assert_eq!(
        safe_superseded_revision_label(&revision, Some(&earlier)),
        "an earlier account"
    );
    assert_eq!(
        safe_superseded_revision_label(&revision, None),
        "an earlier account"
    );
}

#[test]
fn destination_validation_is_bidirectional_and_bounded() {
    assert!(
        validate_destination(
            DestinationKnowledgeStage::ExactBelieved,
            "cave",
            900_000_000,
            -1_800_000_000,
        )
        .is_ok()
    );
    assert!(validate_destination(DestinationKnowledgeStage::Visited, "", 1, 2).is_err());
    assert!(
        validate_destination(
            DestinationKnowledgeStage::ExactBelieved,
            "cave",
            900_000_001,
            0,
        )
        .is_err()
    );
    assert!(
        validate_destination(
            DestinationKnowledgeStage::ExactBelieved,
            "cave",
            0,
            -1_800_000_001,
        )
        .is_err()
    );
    assert!(
        validate_destination(DestinationKnowledgeStage::ApproximateArea, "hidden", 0, 0,).is_err()
    );
    assert!(validate_destination(DestinationKnowledgeStage::Textual, "", 0, 0).is_ok());
}

#[test]
fn raw_tables_are_private_and_views_fail_closed() {
    let source = INVESTIGATION_SOURCE;
    for table in [
        "case_site_authority",
        "party_case_site_tracking",
        "investigation_case_authority",
        "investigation_event_authority",
        "investigation_observation",
        "investigation_recollection",
        "investigation_claim",
        "investigation_evidence_authority",
        "investigation_evidence_knowledge",
        "investigation_belief",
        "investigation_belief_revision",
        "investigation_lead",
        "investigation_sharing_receipt",
        "investigation_area_authority",
        "investigation_action_capability",
        "investigation_pattern_target_authority",
        "investigation_action_attempt",
        "investigation_action_outcome",
    ] {
        let declaration = format!("#[table(accessor = {table})]");
        assert!(source.contains(&declaration));
        assert!(!source.contains(&format!("#[table(accessor = {table}, public)]")));
    }
    assert_eq!(
        source.matches("if !is_gateway(ctx)").count(),
        source.matches("#[view(accessor =").count(),
    );
    let public_types = source
        .split("pub struct BackendInvestigationJournalEntry")
        .nth(1)
        .and_then(|tail| tail.split("fn journal_case_resolution").next())
        .expect("public investigation projection types");
    assert!(!public_types.contains("hidden_target"));
}

#[test]
fn case_site_projection_requires_exact_unrevised_observer_knowledge() {
    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("pub fn backend_case_site_pins")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn exact_case_site_for_observer")
                .next()
        })
        .expect("case-site projection body");
    assert!(projection.contains("lead.corrected_by.is_empty()"));
    assert!(projection.contains("destination_stage.is_exact()"));
    assert!(projection.contains("owner_character_id: lead.owner_character_id"));
    assert!(projection.contains("case_site_authority()"));
    assert!(!projection.contains("destination_stage.as_str(), \"textual\""));
    assert!(!projection.contains("destination_stage.as_str(), \"approximate_area\""));
}

#[test]
fn generated_case_site_presentation_is_validated_and_action_only() {
    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("fn case_site_presentation_view")
        .nth(1)
        .and_then(|tail| tail.split("fn lead_projects_exact_case_site_pin").next())
        .expect("generated case-site presentation helper");
    let projected_type = source
        .split("pub struct BackendCaseSitePin")
        .nth(1)
        .and_then(|tail| tail.split("pub struct BackendCharacterCaseSiteLocation").next())
        .expect("case-site pin projection type");
    for required in [
        "validate_quest_generation_authority",
        "canonical_case_site_place(&generated_site.id.0)",
        ".map(|generated| (generated, site.id.to_place()))",
        "generated == persisted",
        "generated_site.safe_label != site.name",
        "generated_case_site_combat_eligible",
        "find(owner_character_id)",
        "case_outcome_fact()",
        "consequence.public_summary",
    ] {
        assert!(projection.contains(required), "{required}");
    }
    for forbidden in ["enemy_type", "cause", "factor_trace", "manifest_json"] {
        assert!(!projected_type.contains(forbidden), "{forbidden} leaked");
    }
}

#[test]
fn exact_witness_belief_projects_a_pin_without_route_completion() {
    let site = CaseSiteAuthority {
        id_key: "site:opaque".into(),
        id: CaseSiteId::try_new("site:opaque").unwrap(),
        case_id: "canonical-case".into(),
        origin_settlement_id: "settlement".into(),
        name: "The abandoned croft".into(),
        description: "A roofless croft beyond the mill.".into(),
        scene_key: "ruins".into(),
        longitude_e7: 110_000_000,
        latitude_e7: 532_000_000,
        coordinates_are_geographic: true,
        distance_m: 8_000,
    };
    let lead = InvestigationLead {
        id: "witness-lead".into(),
        owner_character_id: 7,
        case_id: "public-case".into(),
        proposition_id: "reported-place".into(),
        summary: "I saw it enter the old croft.".into(),
        source_label: "the referred local witness".into(),
        confidence_bps: 5_000,
        destination_stage: DestinationKnowledgeStage::ExactBelieved,
        directions: String::new(),
        exact_location_id: site.id.as_str().to_owned(),
        latitude_e7: site.latitude_e7,
        longitude_e7: site.longitude_e7,
        witness_name: "Marta".into(),
        witness_description: "A short, dark-haired miller.".into(),
        witness_occupation_or_relationship: "miller".into(),
        expected_location: "The mill".into(),
        current_learned_location: site.name.clone(),
        contradiction_group: "reported-place".into(),
        corrected_by: String::new(),
        recorded_at: 1,
    };
    assert!(lead_projects_exact_case_site_pin(
        &lead,
        &site,
        Some("public-case")
    ));
    let mut approximate = lead.clone();
    approximate.destination_stage = DestinationKnowledgeStage::ApproximateArea;
    assert!(!lead_projects_exact_case_site_pin(
        &approximate,
        &site,
        Some("public-case")
    ));
    let mut corrected = lead;
    corrected.corrected_by = "later-testimony".into();
    assert!(!lead_projects_exact_case_site_pin(
        &corrected,
        &site,
        Some("public-case")
    ));
}

#[test]
fn source_has_authorization_idempotency_and_no_implicit_sharing() {
    let source = INVESTIGATION_SOURCE;
    assert!(source.contains("require_strategic_gateway(ctx)?"));
    assert!(source.contains("different payload"));
    assert!(source.contains("co-located member"));
    assert!(source.contains("share_investigation_belief"));
    assert!(!source.contains("on_party_join"));
    assert!(source.contains("let canonical_case_id = receipt.opaque_case_ref.clone()"));
    assert!(source.contains("let case_id = generated.public_case_id"));
    assert!(!source.contains("let case_id = receipt.opaque_case_ref"));
    let rumor = source
        .split("fn receive_local_problem_rumor_impl")
        .nth(1)
        .and_then(|tail| tail.split("fn process_investigation_pipeline").next())
        .expect("local rumor reducer implementation");
    assert!(rumor.contains(".local_problem_receipt()"));
    assert!(rumor.contains(".id()"));
    assert!(rumor.contains(".find(&receipt_id)"));
    assert!(source.contains("Evidence knowledge has conflicting provenance"));
    assert!(!source.contains("#[table(accessor = investigation_evidence_knowledge, public)]"));
}

#[test]
fn action_projection_and_reducer_keep_hidden_authority_server_side() {
    let source = INVESTIGATION_SOURCE;
    let projected_type = source
        .split("pub struct BackendInvestigationAction")
        .nth(1)
        .and_then(|tail| tail.split("#[derive").next())
        .expect("action projection type");
    let projection = source
        .split("pub fn backend_investigation_actions")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub fn backend_investigation_action_outcomes")
                .next()
        })
        .expect("action projection body");
    for hidden in ["target_id", "resolution_seed", "success_threshold"] {
        assert!(
            !projected_type.contains(hidden),
            "{hidden} leaked into projection"
        );
    }
    assert!(projected_type.contains("case_id"));
    assert!(projected_type.contains("required_case_site_id"));
    assert!(projected_type.contains("availability"));
    assert!(projected_type.contains("InvestigationActionAvailability"));
    assert!(!projected_type.contains("unavailable_reason_code"));
    assert!(!projected_type.contains("can_travel_to_required_site"));
    assert!(!projected_type.contains("wait_minutes"));
    assert!(projection.contains("capability_has_successful_attempt_view"));
    assert!(projection.contains("capability_has_live_support_view"));
    assert!(projection.contains("action_unavailable_reason_view"));
    assert!(projection.contains("projected_night_window_wait_minutes"));
    assert!(projection.contains(".filter(|capability| capability.active)"));
    assert!(!projected_type.contains("track_segment"));

    let reducer = source
        .split("pub fn perform_investigation_action")
        .nth(1)
        .and_then(|tail| tail.split("pub fn receive_investigation_claim").next())
        .expect("action reducer body");
    assert!(reducer.contains("expected_version"));
    assert!(reducer.contains("perform_investigation_action_authorized"));
    assert!(!reducer.contains("stage_investigation_lead"));
}

#[test]
fn corrected_exact_site_knowledge_is_not_live_action_support() {
    assert!(exact_site_knowledge_is_live(
        "case",
        "site",
        "case",
        "site",
        DestinationKnowledgeStage::ExactBelieved,
        "",
        "case",
        "site",
        true,
        None,
        None,
    ));
    assert!(!exact_site_knowledge_is_live(
        "case",
        "site",
        "case",
        "site",
        DestinationKnowledgeStage::ExactBelieved,
        "newer-lead",
        "case",
        "site",
        true,
        None,
        None,
    ));
    assert!(exact_site_knowledge_is_live(
        "canonical",
        "site",
        "public",
        "site",
        DestinationKnowledgeStage::Visited,
        "",
        "canonical",
        "site",
        true,
        Some("canonical"),
        Some("public"),
    ));
    assert!(!exact_site_knowledge_is_live(
        "canonical",
        "site",
        "collision",
        "site",
        DestinationKnowledgeStage::Visited,
        "",
        "canonical",
        "site",
        true,
        Some("canonical"),
        Some("public"),
    ));
    assert!(!exact_site_knowledge_is_live(
        "canonical",
        "site",
        "public",
        "other-site",
        DestinationKnowledgeStage::Visited,
        "",
        "canonical",
        "site",
        true,
        Some("canonical"),
        Some("public"),
    ));
    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("fn capability_has_live_support_view")
        .nth(1)
        .and_then(|tail| tail.split("fn exact_action_site_for_observer").next())
        .expect("projection live-support predicate");
    assert!(projection.contains("exact_action_site_for_observer"));
    let recovery = source
        .split("fn activate_action_successors")
        .nth(1)
        .and_then(|tail| tail.split("fn complete_referred_contact_action").next())
        .expect("failed-alternate live-support predicate");
    assert!(recovery.contains("capability_has_live_support_reducer"));
    assert!(recovery.contains("exact_site_knowledge_is_live"));
    assert!(recovery.contains("exact_action_case_site_for_observer(ctx, capability)"));
    let legacy_site_lookup = source
        .split("pub(crate) fn exact_case_site_for_observer_at")
        .nth(1)
        .and_then(|tail| tail.split("pub(crate) fn case_site_presence_for_observer").next())
        .expect("stable travel and pin exact-site helper");
    assert!(legacy_site_lookup.contains("observer_character_id: u64"));
    assert!(legacy_site_lookup.contains("case_site_id: &str"));
    assert!(legacy_site_lookup.contains("Option<(CaseSiteAuthority, InvestigationLead)>"));
    assert!(legacy_site_lookup.contains("canonical_case_site_place(case_site_id)"));
    assert!(legacy_site_lookup.contains("site.id.to_place() != requested_place"));
    assert!(!legacy_site_lookup.contains("InvestigationActionCapability"));
}

#[test]
fn case_site_transport_and_presence_adapters_fail_closed() {
    assert!(CaseSiteId::try_new("case:valid").is_ok());
    assert!(CaseSiteId::try_new("malformed case site").is_err());

    let source = INVESTIGATION_SOURCE;
    let occupancy = source
        .split("pub(crate) fn case_site_presence_for_observer")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn case_context_presence_for_observer")
                .next()
        })
        .expect("observer-safe case occupancy adapter");
    assert!(occupancy.contains("exact_case_site_for_observer_at"));
    assert!(occupancy.contains("character_case_site_occupancy_at"));
    assert!(occupancy.contains("character_alive_at"));
    let context = source
        .split("pub(crate) fn case_context_presence_for_observer")
        .nth(1)
        .and_then(|tail| tail.split("pub(crate) fn disclose_exact_case_site").next())
        .expect("observer-safe case context adapter");
    assert!(context.contains("context_membership_valid_at"));
    assert!(context.contains("expected_membership_id"));
    assert!(context.contains("expected_revision"));
    assert!(context.contains("membership.revision"));
    assert!(context.contains("exact_case_site_for_observer_at"));

    let transition = source
        .split("pub(crate) fn set_character_case_site")
        .nth(1)
        .and_then(|tail| tail.split("pub struct InvestigationSharingReceipt").next())
        .expect("case-site transition authority");
    assert!(transition.contains("-> Result<(), String>"));
    assert!(transition.contains("CaseSiteId::try_new"));
    assert!(transition.contains("current.left_at = Some(minute)"));
}

#[test]
fn corrected_contact_referral_is_not_live_at_any_action_boundary() {
    let referral = |owner_character_id, case_id: &str, corrected_by: &str| InvestigationLead {
        id: "lead".into(),
        owner_character_id,
        case_id: case_id.into(),
        proposition_id: String::new(),
        summary: "Ask the cooper what she saw.".into(),
        source_label: "local rumor".into(),
        confidence_bps: 5_000,
        destination_stage: DestinationKnowledgeStage::Textual,
        directions: "Public square".into(),
        exact_location_id: String::new(),
        latitude_e7: 0,
        longitude_e7: 0,
        witness_name: "Greta".into(),
        witness_description: "A tall cooper.".into(),
        witness_occupation_or_relationship: "cooper".into(),
        expected_location: "Public square".into(),
        current_learned_location: String::new(),
        contradiction_group: String::new(),
        corrected_by: corrected_by.into(),
        recorded_at: 50_000,
    };
    let live = referral(7, "case", "");
    assert!(lead_is_live_contact_referral(&live, 7, "case"));
    let corrected = referral(7, "case", "replacement-lead");
    assert!(!lead_is_live_contact_referral(&corrected, 7, "case"));
    let mut retracted = referral(7, "case", "");
    retracted.witness_name.clear();
    assert!(!lead_is_live_contact_referral(&retracted, 7, "case"));
    assert!(!lead_is_live_contact_referral(&live, 8, "case"));
    assert!(!lead_is_live_contact_referral(&live, 7, "other-case"));

    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("fn capability_has_live_support_view")
        .nth(1)
        .and_then(|tail| tail.split("fn exact_action_site_for_observer").next())
        .expect("projection contact support");
    assert!(projection.contains("lead_is_live_contact_referral"));
    let recovery = source
        .split("fn capability_has_live_support_reducer")
        .nth(1)
        .and_then(|tail| tail.split("fn complete_referred_contact_action").next())
        .expect("recovery contact support");
    assert!(recovery.contains("lead_is_live_contact_referral"));
    let execution = source
        .split("fn validate_live_action_prerequisites")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn case_objective_contains_custody_target")
                .next()
        })
        .expect("execution contact support");
    assert!(execution.contains("lead_is_live_contact_referral"));
    assert!(execution.contains("No live witness referral supports this action"));
}

#[test]
fn generated_opposition_projection_is_checked_and_fail_closed() {
    assert_eq!(checked_generated_opposition(2, 125), Some((2, 250)));
    assert_eq!(checked_generated_opposition(0, 125), None);
    assert_eq!(checked_generated_opposition(2, 0), None);
    assert_eq!(checked_generated_opposition(u32::MAX, u64::MAX), None);
    let projection = crate::production_source(include_str!("../sites.rs"));
    assert!(projection.contains("combat_available: false"));
    assert!(projection.contains("opposition_count: None"));
    assert!(projection.contains("opposition_combat_power: None"));
}

#[test]
fn generated_live_support_uses_the_observer_safe_case_alias_at_every_boundary() {
    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("fn capability_has_live_support_view")
        .nth(1)
        .and_then(|tail| tail.split("fn exact_action_site_for_observer").next())
        .expect("projection live support");
    assert!(projection.contains("projected_action_public_case_id(ctx, capability)"));
    assert!(projection.contains("&observer_case_id"));
    assert!(projection.contains("lead.case_id == observer_case_id"));

    let recovery = source
        .split("fn capability_has_live_support_reducer")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn capability_has_live_pattern_support_reducer")
                .next()
        })
        .expect("reducer live support");
    assert!(recovery.contains("reducer_action_public_case_id(ctx, capability)"));
    assert!(recovery.contains("&observer_case_id"));
    assert!(recovery.contains("lead.case_id == observer_case_id"));

    let execution = source
        .split("fn validate_live_action_prerequisites")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn case_objective_contains_custody_target")
                .next()
        })
        .expect("action execution prerequisites");
    assert!(execution.contains("reducer_action_public_case_id(ctx, capability)"));
    assert!(execution.contains("&observer_case_id"));
    assert!(execution.contains("lead.case_id == observer_case_id"));

    let pattern_projection = source
        .split("fn capability_has_live_pattern_support_view")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn tracking_capability_chain_is_coherent")
                .next()
        })
        .expect("pattern projection support");
    assert!(pattern_projection.contains("projected_action_public_case_id(ctx, capability)"));
    assert!(pattern_projection.contains("&observer_case_id"));
    let pattern_recovery = source
        .split("fn capability_has_live_pattern_support_reducer")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn validate_capability_blueprint_reducer")
                .next()
        })
        .expect("pattern reducer support");
    assert!(pattern_recovery.contains("reducer_action_public_case_id(ctx, capability)"));
    assert!(pattern_recovery.contains("&observer_case_id"));
    let pattern_execution = source
        .split("fn validate_generated_pattern_condition")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
        .expect("pattern execution support");
    assert!(pattern_execution.contains("reducer_action_public_case_id(ctx, capability)"));
    assert!(pattern_execution.contains("&observer_case_id"));
    assert!(!projection.contains("fixture"));
    assert!(!recovery.contains("fixture"));
    assert!(!execution.contains("fixture"));
}

#[test]
fn inspect_site_travel_requires_ready_off_site_party() {
    let site = CaseSiteId::from("site".to_owned());
    let ready_off_site = projected_action_availability(true, Some(&site), false, 0);
    assert!(ready_off_site.unavailable_reason.is_some());
    assert_eq!(
        ready_off_site.availability,
        action::InvestigationActionAvailability::Unavailable {
            reason: action::InvestigationActionUnavailableReason::TravelRequired,
            can_travel_to_required_site: true,
            wait_minutes: 0,
        }
    );

    let incapacitated_off_site = projected_action_availability(false, Some(&site), false, 0);
    assert!(incapacitated_off_site.unavailable_reason.is_some());
    assert!(matches!(
        incapacitated_off_site.availability,
        action::InvestigationActionAvailability::Unavailable {
            reason: action::InvestigationActionUnavailableReason::PartyNotReady,
            can_travel_to_required_site: false,
            wait_minutes: 0,
        }
    ));

    let incapacitated_on_site = projected_action_availability(false, Some(&site), true, 0);
    assert!(incapacitated_on_site.unavailable_reason.is_some());
    assert!(matches!(
        incapacitated_on_site.availability,
        action::InvestigationActionAvailability::Unavailable {
            reason: action::InvestigationActionUnavailableReason::PartyNotReady,
            can_travel_to_required_site: false,
            wait_minutes: 0,
        }
    ));

    let ready_on_site = projected_action_availability(true, Some(&site), true, 0);
    assert!(ready_on_site.unavailable_reason.is_none());
    assert_eq!(
        ready_on_site.availability,
        action::InvestigationActionAvailability::Available
    );
}

#[test]
fn changed_victim_cohort_projects_one_generic_observer_safe_reason() {
    let unavailable = projected_target_changed_availability();
    assert_eq!(
        unavailable.availability,
        action::InvestigationActionAvailability::Unavailable {
            reason: action::InvestigationActionUnavailableReason::TargetChanged,
            can_travel_to_required_site: false,
            wait_minutes: 0,
        }
    );
    let wording = unavailable.unavailable_reason.unwrap();
    for private_detail in [
        "victim",
        "cohort",
        "NPC",
        "demographic",
        "profile",
        "location",
    ] {
        assert!(
            !wording.contains(private_detail),
            "private predicate leaked through generic wording"
        );
    }

    let source = INVESTIGATION_SOURCE;
    let live_check = source
        .split("fn victim_cohort_is_current_view")
        .nth(1)
        .and_then(|tail| tail.split("fn action_unavailable_reason_view").next())
        .expect("victim cohort public availability check");
    for predicate in [
        "investigation_pattern_target_authority()",
        "target.case_id != capability.case_id",
        "resolve_settlement_resident_view(",
        "settlement_resident_presence()",
        "target.expected_settlement_id",
        "target.expected_location",
        "target.demographic",
        "target.age_band",
        "target.sex",
        "target.profession",
        "pattern_target_matches",
        "npc_presence_remaining_minutes",
    ] {
        assert!(live_check.contains(predicate), "missing {predicate}");
    }
    let availability = source
        .split("fn action_unavailable_reason_view")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub fn backend_investigation_action_outcomes")
                .next()
        })
        .expect("action availability projection");
    assert!(availability.contains("victim_cohort_is_current_view"));
    assert!(availability.contains("projected_target_changed_availability"));
}

#[test]
fn nighttime_projection_wait_is_exact_and_bounded() {
    assert_eq!(night_window_wait_minutes(0), 0);
    assert_eq!(night_window_wait_minutes(359), 0);
    assert_eq!(night_window_wait_minutes(360), 840);
    assert_eq!(night_window_wait_minutes(1_199), 1);
    assert_eq!(night_window_wait_minutes(1_200), 0);
    assert_eq!(night_window_wait_minutes(1_440 + 600), 600);

    let blocked = projected_action_availability(true, None, false, 37);
    assert_eq!(
        blocked.availability,
        action::InvestigationActionAvailability::Unavailable {
            reason: action::InvestigationActionUnavailableReason::NightWindow,
            can_travel_to_required_site: false,
            wait_minutes: 37,
        }
    );
    assert!(blocked.unavailable_reason.is_some());

    let source = INVESTIGATION_SOURCE;
    let projected_gate = source
        .split("fn projected_night_window_wait_minutes")
        .nth(1)
        .and_then(|tail| tail.split("fn action_unavailable_reason_view").next())
        .expect("projected nighttime gate");
    assert!(projected_gate.contains("GeneratedPatternCondition::NightWindow"));
    assert!(projected_gate.contains("observer_pattern_route_has_live_corroborated_clue"));
    assert!(!projected_gate.contains("canonical_events"));
}

#[test]
fn locate_contact_projection_mirrors_public_scheduled_presence() {
    let presence = crate::SettlementResidentPresence {
        character_id: 9,
        settlement_id: "lubeck".into(),
        location_id: "market".into(),
        start_minute: 480,
        end_minute: 1_020,
        is_default: false,
        context_suppressed: false,
        health_suppressed: false,
    };
    assert_eq!(
        public_contact_schedule_wait_minutes(&presence, 600),
        Some(0)
    );
    assert_eq!(
        public_contact_schedule_wait_minutes(&presence, 1_020),
        Some(900)
    );

    let mut suppressed = presence.clone();
    suppressed.health_suppressed = true;
    assert_eq!(public_contact_schedule_wait_minutes(&suppressed, 600), None);

    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("fn projected_contact_presence_availability")
        .nth(1)
        .and_then(|tail| tail.split("fn night_window_wait_minutes").next())
        .expect("public contact presence projection");
    assert!(projection.contains("InvestigationActionKind::LocateContact"));
    assert!(projection.contains("settlement_resident_presence()"));
    assert!(projection.contains("InvestigationActionUnavailableReason::ContactScheduleWindow"));
    assert!(projection.contains("InvestigationActionUnavailableReason::ContactNotPresent"));

    let reducer = source
        .split("fn validate_action_position")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_generated_pattern_condition").next())
        .expect("reducer contact presence validation");
    assert!(reducer.contains("InvestigationActionKind::LocateContact"));
    assert!(reducer.contains("npc_is_present"));
}

#[test]
fn locate_contact_stale_target_beats_off_hours_schedule_wait() {
    use adventuresim_core::quest_generation::{WitnessCandidate, WitnessDemographic};

    let candidate = |location: &str, presence_version: u64| WitnessCandidate {
        resident_character_id: 9,
        display_name: "Ada".into(),
        demographic: WitnessDemographic::Laborer,
        age_band: "adult".into(),
        sex: "female".into(),
        profession: "weaver".into(),
        visible_description: String::new(),
        expected_location: location.into(),
        expected_location_label: location.into(),
        presence_version,
        allowed_circumstances: Default::default(),
    };
    let expected = candidate("market", 7);
    assert!(!referred_contact_target_matches(
        &expected,
        &candidate("inn", 7),
        "lubeck",
        "lubeck",
    ));
    assert!(!referred_contact_target_matches(
        &expected,
        &candidate("market", 8),
        "lubeck",
        "lubeck",
    ));

    let projection = INVESTIGATION_SOURCE
        .split("fn projected_contact_presence_availability")
        .nth(1)
        .and_then(|tail| tail.split("fn night_window_wait_minutes").next())
        .expect("locate-contact projection");
    let validates_target = projection.find("referred_contact_is_current_view").unwrap();
    let computes_wait = projection
        .find("projected_contact_schedule_wait_minutes")
        .unwrap();
    assert!(validates_target < computes_wait);
}

#[test]
fn progressed_single_patrol_frontier_is_valid_after_public_night_wait() {
    let mut inspect = InvestigationActionCapability {
        id: "inspect".into(),
        alternate_route_action_id: "patrol".into(),
        ..exact_capability(7, "case-a", "site-a")
    };
    let mut patrol = InvestigationActionCapability {
        id: "patrol".into(),
        method: "patrol".into(),
        target_kind: action::InvestigationTargetKind::Area,
        target_id: "area-a".into(),
        required_action_id: inspect.id.clone(),
        alternate_route_action_id: inspect.id.clone(),
        active: false,
        ..exact_capability(7, "case-a", "site-a")
    };

    assert_eq!(night_window_wait_minutes(360), 840);
    assert_eq!(night_window_wait_minutes(1_200), 0);
    assert_eq!(
        successful_action_successor_ids(&[inspect.clone(), patrol.clone()], &inspect),
        [patrol.id.clone()]
    );

    inspect.active = false;
    patrol.active = true;
    let progressed = [inspect, patrol];
    assert!(validate_action_route_graph_structure(&progressed).is_ok());
    assert!(
        validate_evolved_action_frontier(&progressed, |predecessor_id| {
            predecessor_id == "inspect"
        })
        .is_ok()
    );
    assert_eq!(
        validate_evolved_action_frontier(&progressed, |_| false).unwrap_err(),
        "Active investigation route predecessor has not succeeded"
    );
    let mut missing_predecessor = progressed.clone();
    missing_predecessor[1].required_action_id = "missing-inspect".into();
    assert_eq!(
        validate_evolved_action_frontier(&missing_predecessor, |_| true).unwrap_err(),
        "Active investigation route predecessor is missing"
    );
    assert_eq!(
        validate_initial_action_frontier(&progressed).unwrap_err(),
        "A single investigation entry must be an exact referred contact"
    );

    let source = INVESTIGATION_SOURCE;
    let execution = source
        .split("pub(crate) fn perform_investigation_action_authorized")
        .nth(1)
        .and_then(|tail| tail.split("pub fn perform_investigation_action").next())
        .expect("authorized action execution");
    assert!(execution.contains("validate_action_route_graph("));
    assert!(!execution.contains("validate_newly_issued_action_route_graph("));
}

#[test]
fn newly_issued_graph_rejects_a_sole_non_contact_root() {
    let patrol = InvestigationActionCapability {
        id: "patrol".into(),
        method: "patrol".into(),
        target_kind: action::InvestigationTargetKind::Area,
        target_id: "area-a".into(),
        alternate_route_action_id: "inspect".into(),
        ..exact_capability(7, "case-a", "site-a")
    };
    let inspect = InvestigationActionCapability {
        id: "inspect".into(),
        alternate_route_action_id: patrol.id.clone(),
        active: false,
        ..exact_capability(7, "case-a", "site-a")
    };
    let newly_issued = [patrol, inspect];

    assert!(validate_action_route_graph_structure(&newly_issued).is_ok());
    assert_eq!(
        validate_initial_action_frontier(&newly_issued).unwrap_err(),
        "A single investigation entry must be an exact referred contact"
    );
}

#[test]
fn complete_generated_graph_reissue_preserves_progressed_frontier() {
    let inspect = InvestigationActionCapability {
        id: "inspect".into(),
        version: 2,
        active: false,
        alternate_route_action_id: "patrol".into(),
        ..exact_capability(7, "case-a", "site-a")
    };
    let patrol = InvestigationActionCapability {
        id: "patrol".into(),
        method: "patrol".into(),
        version: 4,
        target_kind: action::InvestigationTargetKind::Area,
        target_id: "area-a".into(),
        required_action_id: inspect.id.clone(),
        alternate_route_action_id: inspect.id.clone(),
        active: true,
        ..exact_capability(7, "case-a", "site-a")
    };
    let expected_ids = vec![inspect.id.clone(), patrol.id.clone()];
    let existing = vec![inspect, patrol];
    let snapshot = existing
        .iter()
        .map(|capability| (capability.id.clone(), capability.version, capability.active))
        .collect::<Vec<_>>();

    let first_action_key = "receive-rumor:first";
    let distinct_action_key = "receive-rumor:reissue";
    assert_ne!(first_action_key, distinct_action_key);
    for _action_key in [first_action_key, distinct_action_key] {
        assert_eq!(
            generated_action_graph_is_complete(&expected_ids, &existing),
            Ok(true)
        );
    }
    assert_eq!(
        existing
            .iter()
            .map(|capability| { (capability.id.clone(), capability.version, capability.active) })
            .collect::<Vec<_>>(),
        snapshot
    );
    assert_eq!(
        generated_action_graph_is_complete(&expected_ids, &existing[..1]).unwrap_err(),
        "Generated investigation action graph is partial"
    );

    let source = INVESTIGATION_SOURCE;
    let issuer = source
        .split("fn issue_rumor_action_graph")
        .nth(1)
        .and_then(|tail| tail.split("let area_id =").next())
        .expect("generated graph issuer");
    let complete = issuer
        .find("generated_action_graph_is_complete")
        .expect("complete graph classification");
    let blueprint = issuer
        .find("validate_capability_blueprint_reducer")
        .expect("stored blueprint validation");
    let evolved = issuer
        .find("return validate_action_route_graph")
        .expect("evolved graph validation");
    let activation = issuer
        .find("set_action_active")
        .expect("fresh graph activation");
    assert!(complete < blueprint && blueprint < evolved && evolved < activation);
}
#[test]
fn case_site_disclosure_ids_reject_prefix_spoofs_and_malformed_revisions() {
    let base = "case-site-disclosure:7:site:ford";
    assert_eq!(
        CaseSiteDisclosureLeadId::parse(base, base),
        Some(CaseSiteDisclosureLeadId::Base)
    );
    assert_eq!(
        CaseSiteDisclosureLeadId::parse(
            "case-site-disclosure:7:site:ford:revision:00000042",
            base,
        ),
        Some(CaseSiteDisclosureLeadId::Revision(42))
    );
    for spoof in [
        "case-site-disclosure:7:site:ford-spoof",
        "case-site-disclosure:7:site:ford:revision:",
        "case-site-disclosure:7:site:ford:revision:42",
        "case-site-disclosure:7:site:ford:revision:00000000",
        "case-site-disclosure:7:site:ford:revision:0000004x",
        "case-site-disclosure:7:site:ford:revision:00000042:extra",
    ] {
        assert_eq!(CaseSiteDisclosureLeadId::parse(spoof, base), None, "{spoof}");
    }
}
