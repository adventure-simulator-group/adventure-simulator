#[test]
fn action_graph_covers_all_methods_and_enforces_authoritative_boundaries() {
    let source = INVESTIGATION_SOURCE;
    let graph = source
        .split("fn issue_rumor_action_graph")
        .nth(1)
        .and_then(|tail| tail.split("fn skill_bps").next())
        .expect("action graph");
    for method in [
        "InspectSite",
        "SearchArea",
        "FollowTracks",
        "ReacquireTracks",
        "LocateContact",
        "Watch",
        "Patrol",
        "LayAmbush",
        "ApproachLead",
    ] {
        assert!(graph.contains(method), "missing action method {method}");
    }
    assert!(graph.contains("validate_newly_issued_action_route_graph"));
    assert!(source.contains("require_party_ready(ctx, party_id)?"));
    assert!(source.contains("require_no_unresolved_encounter(ctx, party_id)?"));
    assert!(source.contains("synchronize_party_activity_time"));
    assert!(source.contains("started_at % 1_440 < 360"));
    assert!(source.contains("started_at % 1_440 >= 1_200"));
    assert!(source.contains("validate_pickup_custody"));
    assert!(source.contains("current.holder_kind != CustodyHolderKind::Site"));
    assert!(source.contains("resolution.risk_triggered"));
    let production = source
        .split("#[cfg(test)]")
        .next()
        .expect("production source");
    assert!(!production.contains("ResolveHostileGroup"));
    assert!(!production.contains("commit_hostile_battle_resolution"));
    assert!(!production.contains("ensure_bound_mission_authority"));
    assert!(!production.contains("HostileResolutionKind::DrivenOff"));
    assert!(!production.contains("HostileResolutionKind::Captured"));
    let position = production
        .split("fn validate_action_position")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
        .expect("position authority");
    assert!(position.contains("settlement_resident_presence()"));
    assert!(position.contains("actor.current_settlement_id.as_deref()"));
    assert!(position.contains("presence.settlement_id.as_str()"));
    assert!(position.contains("validate_tracking_action_origin"));
    assert!(position.contains("validate_action_position("));
    assert!(position.contains("coordinate_area_contains_e7("));
    assert!(position.contains("area.coordinates_are_geographic"));
    assert!(position.contains("site.coordinates_are_geographic"));
    assert!(position.contains("site.case_id == area.case_id"));
    assert!(position.contains("The party must occupy the action's authoritative site"));
    let reducer = production
        .split("pub(crate) fn perform_investigation_action_authorized")
        .nth(1)
        .expect("action reducer");
    let position_check = reducer
        .find("validate_live_action_prerequisites")
        .expect("position check");
    let time_advance = reducer
        .find("advance_investigation_time")
        .expect("time advance");
    let lead_write = reducer
        .find("persist_action_result_lead")
        .expect("lead write");
    assert!(position_check < time_advance);
    assert!(position_check < lead_write);
}

#[test]
fn generated_physical_and_social_reveals_execute_from_known_origins() {
    let source = INVESTIGATION_SOURCE;
    let position = source
        .split("fn validate_action_position")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
        .expect("real position validator");
    assert!(position.contains("\"site\" =>"));
    assert!(position.contains("InvestigationActionKind::FollowTracks"));
    assert!(position.contains("InvestigationActionKind::ReacquireTracks"));
    assert!(position.contains("validate_tracking_action_origin"));
    assert!(position.contains("\"tracks\" | \"route\" =>"));
    assert!(position.contains("validate_action_position("));
    let generated = concat!(
        include_str!("../../../../adventuresim-core/src/quest_generation/model.rs"),
        include_str!("../../../../adventuresim-core/src/quest_generation/projection.rs"),
        include_str!("../../../../adventuresim-core/src/quest_generation/solver.rs"),
        include_str!("../../../../adventuresim-core/src/quest_generation/assembly.rs"),
        include_str!("../../../../adventuresim-core/src/quest_generation/validation.rs"),
        include_str!("../../../../adventuresim-core/src/quest_generation/audit.rs"),
    );
    let disappearance = generated
        .split("TemplateFamily::DisappearanceOrLoss => vec![")
        .nth(1)
        .and_then(|tail| tail.split("pub fn generate").next())
        .expect("generated disappearance graph");
    assert!(disappearance.contains("\"locate_contact\""));
    assert!(disappearance.contains("GeneratedDestinationStage::ApproximateArea"));
    assert!(disappearance.contains("\"approach_social\""));
    assert!(disappearance.contains("\"route\""));
    assert!(disappearance.contains("GeneratedDestinationStage::Exact"));
    assert!(disappearance.contains("\"resolve_social\""));
    assert!(disappearance.contains("\"site\""));
}

#[test]
fn generated_pattern_actions_require_the_exact_earned_clue() {
    let source = INVESTIGATION_SOURCE;
    let validator = source
        .split("fn validate_generated_pattern_condition")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
        .expect("pattern-condition validator");
    assert!(validator.contains("GeneratedActionOutput::PatternCondition"));
    assert!(validator.contains("investigation_evidence_knowledge()"));
    assert!(validator.contains("knowledge.evidence_id.as_str() == evidence_id.as_str()"));
    assert!(validator.contains("started_at % 1_440"));
    assert!(validator.contains("capability.target_kind != \"route\""));
    assert!(validator.contains("InvestigationActionKind::SearchArea"));
    assert!(validator.contains("investigation_pattern_target_authority()"));
    assert!(validator.contains("pattern_target_matches"));
    assert!(validator.contains("generated_npc_presence_version"));
    assert!(validator.contains("developer_npc_witness_candidate"));
    assert!(validator.contains("target.sex.is_empty()"));
    assert!(validator.contains("npc_is_present"));
    assert!(validator.contains("capability.target_id != *cohort_id"));
    assert!(
        !source.contains("#[table(accessor = investigation_pattern_target_authority, public)]")
    );
    let generated_client = include_str!("../../adventuresim-stdb-client/src/mod.rs");
    assert!(!generated_client.contains("investigation_pattern_target_authority_table"));
    let performer = source
        .split("pub(crate) fn perform_investigation_action_authorized")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("authorized action performer");
    assert_eq!(
        performer
            .matches("validate_generated_pattern_condition")
            .count(),
        2,
        "pattern authority is checked before resolution and at the mutation boundary"
    );
}

#[test]
fn pattern_route_support_requires_exact_observer_clue_knowledge() {
    use adventuresim_core::quest_generation::{
        EvidenceId, GeneratedActionOutput, GeneratedPatternCondition,
    };

    let outputs_json = serde_json::to_string(&[GeneratedActionOutput::PatternCondition {
        evidence_id: EvidenceId("pattern-clue".into()),
        condition: GeneratedPatternCondition::BroadSurvey,
    }])
    .unwrap();
    assert_eq!(
        generated_pattern_evidence_id(&outputs_json).unwrap(),
        Some("pattern-clue".into())
    );
    let learned = InvestigationEvidenceKnowledge {
        id: "knowledge".into(),
        owner_character_id: 7,
        case_id: "case".into(),
        evidence_id: "pattern-clue".into(),
        source_id: "search-attempt".into(),
        learned_at: 50_000,
    };
    assert!(observer_pattern_route_has_live_corroborated_clue(
        7,
        "case",
        "pattern-clue",
        [learned.clone()],
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        7,
        "case",
        "pattern-clue",
        Vec::<InvestigationEvidenceKnowledge>::new(),
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        8,
        "case",
        "pattern-clue",
        [learned.clone()],
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        7,
        "other-case",
        "pattern-clue",
        [learned.clone()],
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        7,
        "case",
        "other-clue",
        [learned],
    ));

    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("fn capability_has_live_support_view")
        .nth(1)
        .and_then(|tail| tail.split("fn exact_action_site_for_observer").next())
        .expect("pattern projection support");
    assert!(projection.contains("capability_has_live_pattern_support_view"));
    let recovery = source
        .split("fn capability_has_live_support_reducer")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn capability_has_live_pattern_support_reducer")
                .next()
        })
        .expect("pattern recovery support");
    assert!(recovery.contains("capability_has_live_pattern_support_reducer"));
    let execution = source
        .split("fn validate_generated_pattern_condition")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
        .expect("pattern execution support");
    assert!(execution.contains("observer_pattern_route_has_live_corroborated_clue"));
    assert!(execution.contains("The selected pattern has not been corroborated yet"));
    let live_execution = source
        .split("fn validate_live_action_prerequisites")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn case_objective_contains_custody_target")
                .next()
        })
        .expect("live execution support");
    assert!(live_execution.contains("capability_has_live_support_reducer"));
    let reducer_support = source
        .split("fn capability_has_live_support_reducer")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn capability_has_live_pattern_support_reducer")
                .next()
        })
        .expect("combined reducer support");
    assert!(
        reducer_support.find("required_action_id").unwrap()
            < reducer_support
                .find("capability_has_live_pattern_support_reducer")
                .unwrap()
    );
}

#[test]
fn generated_pattern_authority_fails_closed_and_manual_actions_remain_permissive() {
    use adventuresim_core::{
        local_problem::Scope,
        quest_generation::{
            GeneratedActionOutput, GenerationContext, TemplateFamily, generate, observer_scoped_id,
            test_witnesses,
        },
    };
    let context = GenerationContext {
        seed: 7,
        observer_entropy_hi: 11,
        observer_entropy_lo: 13,
        settlement_id: "lubeck".into(),
        settlement_name: "Lubeck".into(),
        scope: Scope::Settlement {
            settlement_id: "lubeck".into(),
        },
        ordinal: 0,
        now_minute: 50_000,
        incident_weather: adventuresim_core::weather::Precipitation::Clear,
        requested_family: Some(TemplateFamily::RecurringDepredation),
        witness_candidates: test_witnesses(),
    };
    let manifest = generate(&context).unwrap();
    let generated = manifest
        .actions
        .iter()
        .find(|action| {
            action
                .outputs
                .iter()
                .any(|output| matches!(output, GeneratedActionOutput::PatternCondition { .. }))
        })
        .unwrap();
    let (known_prerequisites, safe_result_on_success) =
        generated_capability_safe_text(&manifest, generated);
    let capability = InvestigationActionCapability {
        id: observer_scoped_id(&context, "capability", &format!("7:{}", generated.id.0)),
        owner_character_id: 7,
        case_id: manifest.public_case_id.clone(),
        provenance_kind: "generated".into(),
        generated_case_id: manifest.canonical_case_id.clone(),
        method: action_method(generated.kind).into(),
        version: 0,
        target_kind: generated.target_kind.clone(),
        target_id: generated.target_id.clone(),
        target_terrain: format!(
            "{:?}",
            manifest
                .sites
                .iter()
                .find(|site| site.id.0 == generated.target_id)
                .map(|site| site.terrain)
                .or_else(|| {
                    manifest
                        .areas
                        .iter()
                        .find(|area| area.id == generated.target_id)
                        .map(|area| area.terrain)
                })
                .unwrap_or(action::Terrain::Settlement)
        )
        .to_ascii_lowercase(),
        seed: 1,
        evidence_age_origin_minute: 0,
        uncertainty_bps: 0,
        safe_summary: generated.safe_summary.clone(),
        known_prerequisites,
        safe_result_on_success,
        consequence_json: serde_json::to_string(&InvestigationActionConsequence::None).unwrap(),
        required_action_id: generated
            .prerequisite
            .as_ref()
            .map_or_else(String::new, |id| {
                observer_scoped_id(&context, "capability", &format!("7:{}", id.0))
            }),
        alternate_route_action_id: observer_scoped_id(
            &context,
            "capability",
            &format!("7:{}", generated.alternate.0),
        ),
        active: true,
    };
    let manifest_json = serde_json::to_string(&manifest).unwrap();
    let context_json = serde_json::to_string(&context).unwrap();
    let authority = Some((manifest_json.as_str(), context_json.as_str()));
    let exact_outputs = serde_json::to_string(&generated.outputs).unwrap();
    assert!(matches!(
        generated_pattern_authority(&capability, authority, Some(&exact_outputs)),
        GeneratedPatternAuthority::Pattern { .. }
    ));
    assert_eq!(
        generated_pattern_authority(&capability, authority, None),
        GeneratedPatternAuthority::Invalid
    );
    assert_eq!(
        generated_pattern_authority(&capability, None, None),
        GeneratedPatternAuthority::Invalid
    );
    let mut absent_action_manifest = manifest.clone();
    absent_action_manifest
        .actions
        .retain(|action| action.id != generated.id);
    let absent_action_json = serde_json::to_string(&absent_action_manifest).unwrap();
    let absent_action_authority = Some((absent_action_json.as_str(), context_json.as_str()));
    assert_eq!(
        generated_pattern_authority(&capability, absent_action_authority, None),
        GeneratedPatternAuthority::Invalid
    );
    assert_eq!(
        generated_pattern_authority(&capability, absent_action_authority, Some(&exact_outputs)),
        GeneratedPatternAuthority::Invalid
    );
    let mismatched = serde_json::to_string(&vec![GeneratedActionOutput::AmbushReady]).unwrap();
    assert_eq!(
        generated_pattern_authority(&capability, authority, Some(&mismatched)),
        GeneratedPatternAuthority::Invalid
    );
    let mut wrong_evidence_outputs = generated.outputs.clone();
    let pattern = wrong_evidence_outputs
        .iter_mut()
        .find_map(|output| match output {
            GeneratedActionOutput::PatternCondition { evidence_id, .. } => Some(evidence_id),
            _ => None,
        })
        .unwrap();
    pattern.0 = "wrong-evidence".into();
    let wrong_evidence = serde_json::to_string(&wrong_evidence_outputs).unwrap();
    assert_eq!(
        generated_pattern_authority(&capability, authority, Some(&wrong_evidence)),
        GeneratedPatternAuthority::Invalid
    );
    assert_eq!(
        generated_pattern_authority(&capability, authority, Some("not-json")),
        GeneratedPatternAuthority::Invalid
    );

    let mut wrong_observer = capability.clone();
    wrong_observer.owner_character_id = 8;
    assert_eq!(
        generated_pattern_authority(&wrong_observer, authority, Some(&exact_outputs)),
        GeneratedPatternAuthority::Invalid
    );
    let mut wrong_case = capability.clone();
    wrong_case.case_id = "another-case".into();
    assert_eq!(
        generated_pattern_authority(&wrong_case, authority, Some(&exact_outputs)),
        GeneratedPatternAuthority::Invalid
    );
    for mutate in 0..10 {
        let mut changed = capability.clone();
        match mutate {
            0 => changed.method = "watch".into(),
            1 => changed.target_kind = "site".into(),
            2 => changed.target_id = "wrong-target".into(),
            3 => changed.target_terrain = "water".into(),
            4 => changed.required_action_id = "wrong-required".into(),
            5 => changed.alternate_route_action_id = "wrong-alternate".into(),
            6 => changed.safe_summary = "wrong summary".into(),
            7 => changed.consequence_json = r#"{"kind":"rescue_subject"}"#.into(),
            8 => changed.known_prerequisites = "wrong prerequisites".into(),
            _ => changed.safe_result_on_success = "wrong result".into(),
        }
        assert_eq!(
            generated_pattern_authority(&changed, authority, Some(&exact_outputs)),
            GeneratedPatternAuthority::Invalid,
            "blueprint mutation {mutate} unexpectedly remained valid"
        );
    }
    let mut night_manifest = manifest.clone();
    let night_action = night_manifest
        .actions
        .iter_mut()
        .find(|action| action.id == generated.id)
        .unwrap();
    for output in &mut night_action.outputs {
        if let GeneratedActionOutput::PatternCondition { condition, .. } = output {
            *condition =
                adventuresim_core::quest_generation::GeneratedPatternCondition::NightWindow;
        }
    }
    let night_outputs = serde_json::to_string(&night_action.outputs).unwrap();
    let night_manifest_json = serde_json::to_string(&night_manifest).unwrap();
    let mut wrong_night_geography = capability.clone();
    wrong_night_geography.target_terrain = "water".into();
    assert_eq!(
        generated_pattern_authority(
            &wrong_night_geography,
            Some((night_manifest_json.as_str(), context_json.as_str())),
            Some(&night_outputs),
        ),
        GeneratedPatternAuthority::Invalid
    );
    let manual = InvestigationActionCapability {
        id: "manual".into(),
        case_id: "manual-case".into(),
        provenance_kind: "manual".into(),
        generated_case_id: String::new(),
        ..capability
    };
    assert_eq!(
        generated_pattern_authority(&manual, None, None),
        GeneratedPatternAuthority::Manual
    );
    assert_eq!(
        exactly_one_generated_authority(Vec::<(String, String, String)>::new()),
        Ok(None)
    );
    assert_eq!(
        exactly_one_generated_authority([("case".into(), "manifest".into(), "context".into())]),
        Ok(Some(("manifest".into(), "context".into())))
    );
    // Canonical and public lookup paths can return the same row; row
    // identity deduplicates it rather than manufacturing ambiguity.
    assert_eq!(
        exactly_one_generated_authority([
            ("same-case".into(), "manifest".into(), "context".into()),
            ("same-case".into(), "manifest".into(), "context".into()),
        ]),
        Ok(Some(("manifest".into(), "context".into())))
    );
    // Canonical/public and public/public collisions both contain distinct
    // private authority rows and therefore fail closed.
    assert_eq!(
        exactly_one_generated_authority([
            ("case-a".into(), "manifest-a".into(), "context-a".into()),
            ("case-b".into(), "manifest-b".into(), "context-b".into()),
        ]),
        Err(())
    );
    assert_eq!(
        exactly_one_generated_authority([
            ("public-a".into(), "manifest-a".into(), "context-a".into()),
            ("public-b".into(), "manifest-b".into(), "context-b".into()),
        ]),
        Err(())
    );

    let source = INVESTIGATION_SOURCE;
    for boundary in [
        "fn capability_has_live_pattern_support_view",
        "fn capability_has_live_pattern_support_reducer",
        "fn validate_generated_pattern_condition",
    ] {
        let body = source.split(boundary).nth(1).unwrap();
        assert!(body.contains("generated_pattern_authority"));
    }
    for lookup in [
        "fn generated_authority_view",
        "fn generated_authority_reducer",
    ] {
        let body = source
            .split(lookup)
            .nth(1)
            .and_then(|tail| tail.split("\nfn ").next())
            .unwrap();
        assert!(body.contains(".case_id()"));
        assert!(body.contains(".public_case_id()"));
        assert!(body.contains(".filter("));
        assert!(body.contains("exactly_one_generated_authority"));
        assert!(!body.contains(".iter()"));
    }
    let candidate_validator = source
        .split("fn validated_generated_authority_candidate")
        .nth(1)
        .and_then(|tail| tail.split("fn generated_authority_view").next())
        .unwrap();
    assert!(candidate_validator.contains("validate_quest_generation_authority"));
    let observer_ids = source
        .split("fn generated_observer_id")
        .nth(1)
        .and_then(|tail| tail.split("fn set_action_active").next())
        .unwrap();
    assert!(observer_ids.contains("validate_quest_generation_authority"));
    let issuer = source
        .split("fn issue_rumor_action_graph")
        .nth(1)
        .and_then(|tail| tail.split("fn issue_investigation_actions").next())
        .unwrap();
    assert!(issuer.contains("generated_action_graph_is_complete"));
    assert!(issuer.contains("for capability in &existing_capabilities"));
    assert!(issuer.contains("validate_capability_blueprint_reducer(ctx, capability)?"));
    assert!(issuer.contains("return validate_action_route_graph"));
    assert!(issuer.contains("generated_capability_safe_text(&manifest, generated)"));
}

#[test]
fn failed_action_wording_counts_any_other_live_supported_route() {
    let source = INVESTIGATION_SOURCE;
    let recovery = source
        .split("fn activate_action_successors")
        .nth(1)
        .and_then(|tail| {
            tail.split("\nfn capability_has_live_support_reducer")
                .next()
        })
        .expect("failed route recovery");
    assert!(recovery.contains("candidate.id != capability.id"));
    assert!(recovery.contains("candidate.active"));
    assert!(recovery.contains("capability_has_live_support_reducer"));
    assert!(recovery.contains("set_action_active"));
    assert!(
        recovery.rfind("set_action_active").unwrap()
            < recovery
                .rfind("capability_has_live_support_reducer")
                .unwrap(),
        "availability must be computed after successor state updates"
    );
}

#[test]
fn capability_randomness_is_private_persisted_and_attempt_domain_separated() {
    let source = INVESTIGATION_SOURCE;
    let issuer = source
        .split("pub(crate) fn issue_investigation_action_capability")
        .nth(1)
        .and_then(|tail| tail.split("fn character_strategic_minute").next())
        .expect("capability issuer");
    assert!(issuer.contains("seed,"));
    assert!(issuer.contains("InvestigationActionCapability"));
    let generated_issuer = source
        .split("fn issue_rumor_action_graph")
        .nth(1)
        .and_then(|tail| tail.split("let area_id =").next())
        .expect("generated capability issuer");
    assert!(generated_issuer.contains("ctx.random::<u64>()"));
    let performer = source
        .split("pub(crate) fn perform_investigation_action_authorized")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("authorized performer");
    assert!(performer.contains("&expected_version.to_string()"));
    assert!(performer.contains("if let Some(attempt)"));
    assert!(performer.contains("seed: capability.seed"));
    assert!(performer.contains("attempt_index: expected_version"));
    assert!(!source.contains("stable_action_seed"));
}

#[test]
fn generated_testimony_persists_every_proposition_and_corrections_gate_pins() {
    let source = INVESTIGATION_SOURCE;
    let generated = source
        .split("pub(crate) fn persist_generated_testimony")
        .nth(1)
        .and_then(|tail| {
            tail.split("#[reducer]\npub fn receive_investigation_claim")
                .next()
        })
        .unwrap();
    assert!(generated.find(".witnesses").unwrap() < generated.find("for (index, draft)").unwrap());
    assert!(generated.contains("authoritative == witness"));
    assert!(generated.contains("for (index, draft) in projection_plan.iter().enumerate()"));
    assert!(generated.contains("draft.proposition_id.clone()"));
    assert!(generated.contains("draft.corrects_proposition_id"));
    assert!(generated.contains("belief.proposition_id == *proposition_id"));
    assert!(generated.contains("let exact = draft.destination_stage == \"exact_believed\""));
    assert!(generated.contains(".filter(|_| exact)"));
    assert!(generated.contains("prior.corrected_by = lead_id.clone()"));
    assert!(generated.contains("prior.proposition_id == *corrected_proposition"));
    assert!(
        generated.find("validate_generated_testimony_site").unwrap()
            < generated.find("for (index, draft)").unwrap()
    );
    let preflight = source
        .split("fn validate_referred_contact_authority")
        .nth(1)
        .and_then(|tail| tail.split("fn complete_referred_contact_action").next())
        .unwrap();
    assert!(preflight.contains("if roots.len() > 1"));
    assert!(preflight.contains("return Ok(false)"));
    assert!(preflight.contains("!root.required_action_id.is_empty()"));
    assert!(preflight.contains("validate_capability_blueprint_reducer(ctx, root)"));
    assert!(preflight.contains("expected_successors"));
    assert!(preflight.contains("Generated contact successor is missing"));
    assert!(preflight.contains("validate_capability_blueprint_reducer(ctx, &successor)"));
    let completion = source
        .split("fn complete_referred_contact_action")
        .nth(1)
        .and_then(|tail| tail.split("fn issue_rumor_action_graph").next())
        .unwrap();
    assert!(
        completion
            .find("validate_referred_contact_authority")
            .unwrap()
            < completion
                .find("investigation_action_attempt()\n        .insert")
                .unwrap()
    );
    assert!(completion.contains("return Ok(())"));
}

#[test]
fn exact_generated_testimony_requires_matching_private_site_authority() {
    use adventuresim_core::{
        local_problem::Scope,
        quest_generation::{GenerationContext, SiteId, TemplateFamily, generate, test_witnesses},
    };
    let generated = generate(&GenerationContext {
        seed: 7,
        observer_entropy_hi: 11,
        observer_entropy_lo: 13,
        settlement_id: "lubeck".into(),
        settlement_name: "Lubeck".into(),
        scope: Scope::Settlement {
            settlement_id: "lubeck".into(),
        },
        ordinal: 0,
        now_minute: 50_000,
        incident_weather: adventuresim_core::weather::Precipitation::Clear,
        requested_family: Some(TemplateFamily::RecurringDepredation),
        witness_candidates: test_witnesses(),
    })
    .unwrap();
    let generated_site = &generated.sites[0];
    let mut draft = generated.witnesses[0].testimony[0].clone();
    draft.destination_stage = "exact_believed".into();
    draft.site_id = Some(generated_site.id.clone());
    let site = CaseSiteAuthority {
        id_key: generated_site.id.0.clone(),
        id: CaseSiteId::from(generated_site.id.0.clone()),
        case_id: generated.canonical_case_id.clone(),
        origin_settlement_id: "lubeck".into(),
        name: generated_site.safe_label.clone(),
        description: String::new(),
        scene_key: "generated".into(),
        longitude_e7: 100,
        latitude_e7: 200,
        coordinates_are_geographic: true,
        distance_m: 1_000,
    };
    assert!(validate_generated_testimony_site(&generated, &draft, Some(&site)).is_ok());
    assert!(validate_generated_testimony_site(&generated, &draft, None).is_err());
    let mut cross_case = site.clone();
    cross_case.case_id = "other-case".into();
    assert!(validate_generated_testimony_site(&generated, &draft, Some(&cross_case)).is_err());
    let mut wrong_identity = site.clone();
    wrong_identity.name = "wrong generated site".into();
    assert!(validate_generated_testimony_site(&generated, &draft, Some(&wrong_identity)).is_err());
    let mut wrong_geometry = site.clone();
    wrong_geometry.latitude_e7 = i32::MAX;
    assert!(validate_generated_testimony_site(&generated, &draft, Some(&wrong_geometry)).is_err());
    let mut missing_site = draft.clone();
    missing_site.site_id = Some(SiteId::try_new("missing-site").unwrap());
    assert!(validate_generated_testimony_site(&generated, &missing_site, Some(&site)).is_err());
    let mut non_exact = draft;
    non_exact.destination_stage = "approximate_area".into();
    non_exact.site_id = None;
    assert!(validate_generated_testimony_site(&generated, &non_exact, None).is_ok());
}

#[test]
fn coordinate_area_handles_both_modes_boundaries_and_invalid_geography() {
    // Geographic E7: roughly 500 m, 1,000 m, and 1,112 m at the equator.
    assert!(coordinate_area_contains_e7(
        0, 0, 1_000, true, 45_000, 0, true
    ));
    assert!(coordinate_area_contains_e7(
        0, 0, 1_000, true, 89_932, 0, true
    ));
    assert!(!coordinate_area_contains_e7(
        0, 0, 1_000, true, 100_000, 0, true
    ));
    // Abstract E7: one coordinate unit is one kilometer.
    assert!(coordinate_area_contains_e7(
        0, 0, 1_000, false, 5_000_000, 0, false
    ));
    assert!(coordinate_area_contains_e7(
        0, 0, 1_000, false, 10_000_000, 0, false
    ));
    assert!(!coordinate_area_contains_e7(
        0, 0, 1_000, false, 10_020_000, 0, false
    ));
    assert!(!coordinate_area_contains_e7(
        0, 0, 1_000, true, 45_000, 0, false
    ));
    assert!(!coordinate_area_contains_e7(
        0,
        0,
        1_000,
        true,
        i32::MAX,
        0,
        true
    ));
    assert!(!coordinate_area_contains_e7(
        0,
        0,
        1_000,
        true,
        0,
        i32::MAX,
        true
    ));
    // Valid near-antipodal geography must remain about 20,000 km away,
    // never wrap through NaN-to-integer conversion and appear as zero.
    assert!(!coordinate_area_contains_e7(
        0,
        0,
        5_000,
        true,
        1_799_999_999,
        0,
        true
    ));
}

#[test]
fn action_and_outcome_projections_correlate_public_cases_fail_closed() {
    let source = INVESTIGATION_SOURCE;
    let action_projection = source
        .split("pub fn backend_investigation_actions")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn capability_has_successful_attempt_view")
                .next()
        })
        .expect("action projection");
    assert!(action_projection.contains("projected_action_public_case_id"));
    assert!(action_projection.contains("case_id,"));

    let outcome_projection = source
        .split("pub fn backend_investigation_action_outcomes")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) enum InvestigationActionConsequence")
                .next()
        })
        .expect("outcome projection");
    assert!(outcome_projection.contains("capability.case_id != outcome.case_id"));
    assert!(outcome_projection.contains("projected_action_public_case_id"));
    assert!(outcome_projection.contains("filter_map"));
}

#[test]
fn case_summary_subject_comes_only_from_immutable_journal_history() {
    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("pub fn backend_investigation_cases")
        .nth(1)
        .and_then(|tail| tail.split("pub struct BackendCaseSitePin").next())
        .expect("case summary projection");
    assert!(projection.contains("for (owner_character_id, case_id, summary, recorded_at)"));
    assert!(projection.contains("for lead in leads"));
    assert!(projection.contains("case.2 = case.2.max(lead.recorded_at)"));
    assert!(!projection.contains("case.1 = lead.summary"));
}
