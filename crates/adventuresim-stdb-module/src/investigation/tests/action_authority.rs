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
    assert!(source.contains("started_at % adventuresim_core::strategic_time::MINUTES_PER_DAY"));
    assert!(source.contains("validate_pickup_custody"));
    assert!(source.contains("current.holder_kind != CustodyHolderKind::Site"));
    assert!(source.contains("resolution.risk_triggered"));
    let production = source;
    assert!(!production.contains("ResolveHostileGroup"));
    assert!(!production.contains("commit_hostile_battle_resolution"));
    assert!(!production.contains("ensure_bound_mission_authority"));
    assert!(!production.contains("HostileResolutionKind::DrivenOff"));
    assert!(!production.contains("HostileResolutionKind::Captured"));
    let position = production
        .split("fn validate_action_position")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_generated_pattern_condition").next())
        .expect("position authority");
    assert!(position.contains("settlement_resident_presence()"));
    assert!(position.contains("actor.current_settlement_id.as_deref()"));
    assert!(position.contains("presence.settlement_id.as_str()"));
    assert!(position.contains("validate_tracking_action_origin"));
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
    assert!(reducer.contains("let Some(started_at) = synchronize_party_activity_time"));
    assert!(reducer.contains("let mut interval_completed = true"));
    assert!(reducer.contains("interval_completed &="));
    assert!(reducer.contains("advance_investigation_time"));
    let terminal_gate = reducer
        .find("if !interval_completed")
        .expect("terminal interval gate");
    let consequence = reducer
        .find("commit_action_consequence")
        .expect("action consequence");
    let result_lead = reducer
        .find("persist_action_result_lead")
        .expect("result lead");
    assert!(terminal_gate < consequence && terminal_gate < result_lead);
    let incomplete = reducer
        .split("if !interval_completed")
        .nth(1)
        .and_then(|tail| {
            tail.split("crate::strategic::normalize_and_elect_party_leader")
                .next()
        })
        .expect("incomplete interval branch");
    assert!(incomplete.contains("authoritative writes"));
    let terminal_branch = reducer
        .split("if !interval_completed")
        .nth(1)
        .and_then(|tail| {
            tail.split("crate::strategic::normalize_and_elect_party_leader(ctx, &party_id)?")
                .next()
        })
        .expect("terminal branch before completed-interval path");
    assert!(terminal_branch.contains("let _ = crate::strategic::normalize_and_elect_party_leader"));
    assert!(
        terminal_branch.contains("let _ = crate::strategic::reconcile_party_objective_continuity")
    );
    assert!(terminal_branch.contains("return Ok(())"));
    assert!(!terminal_branch.contains("commit_action_consequence"));
    assert!(!terminal_branch.contains("persist_action_result_lead"));
    assert!(terminal_branch.contains("investigation_action_attempt()"));
    assert!(terminal_branch.contains("private_interrupted_action_resolution_json"));
    assert!(reducer.contains("require_living_character(ctx, normalized_party.leader_id)"));
    assert!(reducer.contains(".find(normalized_party.leader_id)"));
}

#[test]
fn referred_contact_completion_materializes_its_authored_outputs() {
    use adventuresim_core::investigation_action::ActionResultKind;

    let resolution = successful_referred_contact_resolution(4_321);
    assert!(resolution.success);
    assert_eq!(resolution.result, ActionResultKind::ContactLocated);
    assert_eq!(resolution.resulting_uncertainty_bps, 4_321);
    assert_eq!(resolution.cost.minutes, 0);
    assert_eq!(resolution.cost.fatigue, 0);
    assert_eq!(resolution.cost.food_units, 0);
    assert_eq!(resolution.cost.water_units, 0);
    assert_eq!(resolution.risk_bps, 0);
    assert!(!resolution.risk_triggered);

    let completion = INVESTIGATION_SOURCE
        .split("fn complete_referred_contact_action")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn successful_referred_contact_resolution")
                .next()
        })
        .expect("referred-contact completion");
    assert!(completion.contains("if attempt_success"));
    assert!(completion.contains("persist_action_result_lead("));
    assert!(
        completion.find("investigation_action_attempt()").unwrap()
            < completion.find("persist_action_result_lead(").unwrap()
    );
    assert!(
        completion.find("persist_action_result_lead(").unwrap()
            < completion.find("capability.active = false").unwrap()
    );
}

#[test]
fn generated_graph_issues_owner_scoped_initial_site_knowledge() {
    use adventuresim_core::{
        local_problem::Scope,
        quest_generation::{GenerationContext, TemplateFamily, generate, test_witnesses},
    };

    let manifest = generate(&GenerationContext {
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
        requested_family: Some(TemplateFamily::Outbreak),
        witness_candidates: test_witnesses(),
    })
    .unwrap();
    let initially_known = generated_initially_known_site_ids(&manifest).collect::<Vec<_>>();
    assert!(!initially_known.is_empty());
    assert!(manifest.sites.iter().all(|site| {
        initially_known.contains(&site.id.0.as_str()) == site.exact_location_initially_known
    }));

    let graph = INVESTIGATION_SOURCE
        .split("fn issue_rumor_action_graph")
        .nth(1)
        .and_then(|tail| tail.split("fn skill_bps").next())
        .expect("generated action graph issuance");
    assert_eq!(
        graph
            .matches("disclose_generated_initial_site_knowledge(")
            .count(),
        2,
        "initial knowledge must be restored for both existing and newly issued graphs"
    );
    let disclosure = INVESTIGATION_SOURCE
        .split("fn disclose_generated_initial_site_knowledge")
        .nth(1)
        .and_then(|tail| tail.split("fn issue_rumor_action_graph").next())
        .expect("generated initial-site disclosure");
    assert!(disclosure.contains("owner_character_id"));
    assert!(disclosure.contains("&manifest.public_case_id"));
    assert!(disclosure.contains("disclose_exact_case_site("));
}

#[test]
fn generated_physical_and_social_reveals_execute_from_known_origins() {
    let source = INVESTIGATION_SOURCE;
    let position = source
        .split("fn validate_action_position")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_generated_pattern_condition").next())
        .expect("real position validator");
    assert!(position.contains("action::InvestigationTargetKind::Site =>"));
    assert!(position.contains("InvestigationActionKind::FollowTracks"));
    assert!(position.contains("InvestigationActionKind::ReacquireTracks"));
    assert!(position.contains("validate_tracking_action_origin"));
    assert!(position.contains(
        "action::InvestigationTargetKind::Tracks | action::InvestigationTargetKind::Route"
    ));
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
    assert!(disappearance.contains("DestinationKnowledgeStage::ApproximateArea"));
    assert!(disappearance.contains("\"approach_social\""));
    assert!(disappearance.contains("InvestigationTargetKind::Route"));
    assert!(disappearance.contains("DestinationKnowledgeStage::ExactBelieved"));
    assert!(disappearance.contains("\"resolve_social\""));
    assert!(disappearance.contains("InvestigationTargetKind::Site"));
}

#[test]
fn generated_pattern_actions_require_the_exact_earned_clue() {
    let source = INVESTIGATION_SOURCE;
    let validator = source
        .split("fn validate_generated_pattern_condition")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
        .expect("pattern-condition validator");
    assert!(validator.contains("generated_pattern_authority("));
    assert!(validator.contains("GeneratedPatternAuthority::Pattern"));
    assert!(validator.contains("investigation_evidence_knowledge()"));
    let clue_authority = source
        .split("fn observer_pattern_route_has_live_corroborated_clue")
        .nth(1)
        .and_then(|tail| tail.split("fn capability_has_live_pattern_support_view").next())
        .expect("typed corroborated-clue authority");
    assert!(clue_authority.contains("proposition.case_id.as_str() == case_id"));
    assert!(clue_authority.contains("proposition.evidence_id.as_str() == evidence_id"));
    assert!(validator.contains("started_at % adventuresim_core::strategic_time::MINUTES_PER_DAY"));
    assert!(validator.contains(
        "capability.target_kind != action::InvestigationTargetKind::Route"
    ));
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
    let generated_client = crate::production_source(include_str!("../../../../adventuresim-stdb-client/src/mod.rs"));
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
        50_000,
        [learned.clone()],
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        7,
        "case",
        "pattern-clue",
        49_999,
        [learned.clone()],
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        7,
        "case",
        "pattern-clue",
        50_000,
        Vec::<InvestigationEvidenceKnowledge>::new(),
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        8,
        "case",
        "pattern-clue",
        50_000,
        [learned.clone()],
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        7,
        "other-case",
        "pattern-clue",
        50_000,
        [learned.clone()],
    ));
    assert!(!observer_pattern_route_has_live_corroborated_clue(
        7,
        "case",
        "other-clue",
        50_000,
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
        provenance_kind: InvestigationProvenanceKind::Generated,
        generated_case_id: manifest.canonical_case_id.clone(),
        method: action_method(generated.kind).into(),
        version: 0,
        target_kind: generated.target_kind,
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
            1 => changed.target_kind = action::InvestigationTargetKind::Site,
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
        provenance_kind: InvestigationProvenanceKind::Manual,
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
        assert!(!body.contains(".quest_generation_authority()\n            .iter()"));
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
    assert!(generated.contains(
        "let exact = draft.destination_stage == DestinationKnowledgeStage::ExactBelieved"
    ));
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
    assert!(!preflight.contains("Generated contact root has no authored successors"));
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
    draft.destination_stage = DestinationKnowledgeStage::ExactBelieved;
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
    non_exact.destination_stage = DestinationKnowledgeStage::ApproximateArea;
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

#[test]
fn exact_site_actions_replan_typed_effects_without_replacing_replay_or_private_receipts() {
    let reducer = INVESTIGATION_SOURCE
        .split("pub(crate) fn perform_investigation_action_authorized")
        .nth(1)
        .expect("action reducer");
    let replay = reducer
        .find("investigation_action_attempt().id().find")
        .unwrap();
    let living_actor = reducer.find("require_living_character").unwrap();
    let rights = reducer.find("decide_investigation_rights").unwrap();
    let plan = reducer.find("site_bound_investigation_plan").unwrap();
    let revalidate = reducer
        .find("// This is the final mutation-boundary validation")
        .unwrap();
    let commit = reducer.find("validate_commit").unwrap();
    let interval = reducer.find("advance_investigation_time").unwrap();
    let receipt = reducer.find("private_resolution_json").unwrap();
    assert!(replay < living_actor && replay < rights);
    assert!(rights < plan && plan < revalidate && revalidate < commit);
    assert!(!reducer.contains("party.leader_id != actor_id"));
    assert!(reducer.contains("rights.kind()"));
    assert!(commit < interval && interval < receipt);
    assert!(reducer.contains("InvestigationPlanEffect::AttemptPartyInterval"));
    assert!(reducer.contains("InvestigationPlanEffect::CommitResolution"));
    assert!(reducer.contains("interval_completed &="));
    let clipped = reducer
        .split("if !interval_completed")
        .nth(1)
        .and_then(|tail| tail.split("if !permits_resolution").next())
        .unwrap();
    assert!(clipped.contains("private_interrupted_action_resolution_json"));
    assert!(clipped.contains("investigation_action_attempt()"));

    let adapter = INVESTIGATION_SOURCE
        .split("fn site_bound_investigation_plan")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn perform_investigation_action_authorized")
                .next()
        })
        .unwrap();
    assert!(adapter.contains(
        "capability.target_kind != action::InvestigationTargetKind::Site"
    ));
    assert!(adapter.contains("InvestigationActionKind::InspectSite"));
    assert!(adapter.contains("investigation-plan-snapshot-v2"));
    assert!(adapter.contains("resolution_input"));
    assert!(adapter.contains(".evidence()"));
    assert!(adapter.contains("rights: rights.clone()"));
    assert!(adapter.contains("question_digest"));

    let knowledge = INVESTIGATION_SOURCE
        .split("fn observer_pattern_route_has_live_corroborated_clue")
        .nth(1)
        .and_then(|tail| tail.split("fn capability_has_live_pattern_support_view").next())
        .expect("knowledge authority");
    assert!(knowledge.contains("row.owner_character_id != owner_character_id"));
    assert!(knowledge.contains("proposition.evidence_id.as_str() == evidence_id"));
    assert!(knowledge.contains("adapt_evidence_knowledge"));
    assert!(knowledge.contains("observer_personal_minute"));
    assert!(!knowledge.contains("u64::MAX"));
}
#[test]
fn referred_contact_position_races_are_coded_as_action_unavailable() {
    let position = INVESTIGATION_SOURCE
        .split("fn validate_action_position")
        .nth(1)
        .and_then(|tail| tail.split("fn validate_generated_pattern_condition").next())
        .expect("action position validation");
    assert!(position.contains("ReducerErrorCode::InvestigationActionUnavailable"));
    assert!(position.contains("unavailable(\"The referred contact is not currently present\")"));
}
