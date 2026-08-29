#[test]
fn bestiary_deduction_projection_is_observer_scoped_and_score_free() {
    let source = INVESTIGATION_SOURCE;
    let projection = source
        .split("pub struct BackendBestiaryDeduction")
        .nth(1)
        .and_then(|tail| tail.split("fn parse_bestiary_lore_results").next())
        .expect("Bestiary deduction projection");
    assert!(projection.contains("owner_character_id"));
    assert!(projection.contains("support_band"));
    assert!(projection.contains("provenance_json"));
    assert!(!projection.contains("score"));
    assert!(!projection.contains("canonical"));
    assert!(!projection.contains("receipt_id"));
    let rebuild = source
        .split("fn rebuild_bestiary_deductions")
        .nth(1)
        .and_then(|tail| tail.split("fn inspection_action_receipt_matches").next())
        .expect("Bestiary deduction rebuild");
    assert!(rebuild.contains(".filter(owner_character_id)"));
}

#[test]
fn bounded_progress_history_is_exact_contiguous_and_nontransferable() {
    let exact = vec![
        failed_attempt("a0", "cap-a", 7, "reacquire_tracks", 0, false),
        failed_attempt("a1", "cap-a", 7, "reacquire_tracks", 1, false),
        failed_attempt("a2", "cap-a", 7, "reacquire_tracks", 2, false),
    ];
    assert_eq!(
        contiguous_failed_attempts("cap-a", 7, "reacquire_tracks", 3, exact.clone()),
        3
    );
    assert_eq!(
        contiguous_failed_attempts("cap-b", 7, "reacquire_tracks", 3, exact.clone()),
        0
    );
    assert_eq!(
        contiguous_failed_attempts("cap-a", 8, "reacquire_tracks", 3, exact.clone()),
        0
    );
    assert_eq!(
        contiguous_failed_attempts("cap-a", 7, "search_area", 3, exact.clone()),
        0
    );
    assert_eq!(
        contiguous_failed_attempts(
            "cap-a",
            7,
            "reacquire_tracks",
            3,
            [exact[0].clone(), exact[2].clone()]
        ),
        1
    );
    let mut success_break = exact;
    success_break[2].success = true;
    assert_eq!(
        contiguous_failed_attempts("cap-a", 7, "reacquire_tracks", 3, success_break),
        0
    );
    let mut interrupted = failed_attempt("a2", "cap-a", 7, "reacquire_tracks", 2, false);
    interrupted.private_resolution_json = serde_json::json!({
        "status": "interrupted",
        "requested_minutes": 120,
        "completion_effects_applied": false,
    })
    .to_string();
    assert_eq!(
        contiguous_failed_attempts(
            "cap-a",
            7,
            "reacquire_tracks",
            3,
            [
                failed_attempt("a0", "cap-a", 7, "reacquire_tracks", 0, false),
                failed_attempt("a1", "cap-a", 7, "reacquire_tracks", 1, false),
                interrupted,
            ],
        ),
        0,
        "an interrupted terminal receipt breaks rather than advances bounded progress"
    );
    let mut malformed = failed_attempt("a2", "cap-a", 7, "reacquire_tracks", 2, false);
    malformed.private_resolution_json = "{}".into();
    assert_eq!(
        contiguous_failed_attempts("cap-a", 7, "reacquire_tracks", 3, [malformed],),
        0,
        "malformed private receipts fail closed"
    );
}

#[test]
fn bounded_failure_wording_snapshots_progress_and_live_alternate_truthfully() {
    let input = action::ResolutionInput {
        seed: 1,
        attempt_index: 0,
        kind: action::InvestigationActionKind::ReacquireTracks,
        terrain: action::Terrain::Road,
        target_terrain: action::Terrain::Forest,
        time_of_day: action::TimeOfDay::Day,
        evidence_age_minutes: 600_000,
        current_uncertainty_bps: 10_000,
        skills: action::SkillContribution {
            terrain_bps: 0,
            awareness_bps: 0,
            stealth_bps: 0,
            assistance_bps: 0,
            familiarity_bps: 0,
        },
        weather: action::WeatherAuthority::Clear { snow_cover_bps: 0 },
    };
    let progress = (0..u64::MAX)
        .find_map(|seed| {
            let progress =
                action::resolve_with_bounded_progress(action::ResolutionInput { seed, ..input }, 0);
            (!progress.resolution.success).then_some(progress)
        })
        .unwrap();
    let without = bounded_failure_wording(progress, false);
    assert!(without.contains("attempt 1 of 6"));
    assert!(without.contains("uncertainty fell to 97.00%"));
    assert!(without.contains("No alternate route is currently supported"));
    let with = bounded_failure_wording(progress, true);
    assert!(with.contains("Another currently supported route is also available"));
    assert!(!with.contains("No alternate route"));
    let bare = private_action_resolution_json(progress.resolution, None).unwrap();
    assert_eq!(
        serde_json::from_str::<action::Resolution>(&bare).unwrap(),
        progress.resolution
    );
    let bounded = private_action_resolution_json(progress.resolution, Some(progress)).unwrap();
    let bounded: serde_json::Value = serde_json::from_str(&bounded).unwrap();
    assert_eq!(bounded["attempt_number"], 1);
    assert_eq!(bounded["guaranteed_by_attempt"], 6);
}

#[test]
fn bounded_progress_covers_every_generated_kind_and_replay_precedes_history() {
    let source = INVESTIGATION_SOURCE;
    let reducer = source
        .split("pub(crate) fn perform_investigation_action_authorized")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .unwrap();
    assert!(reducer.contains("capability_uses_bounded_progress"));
    assert!(
        reducer
            .find("investigation_action_attempt().id().find")
            .unwrap()
            < reducer.find("contiguous_failed_attempts").unwrap()
    );
    let generated = source
        .split("fn generated_progress_kind")
        .nth(1)
        .and_then(|tail| tail.split("fn contiguous_failed_attempts").next())
        .unwrap();
    for kind in [
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
        assert!(generated.contains(kind));
    }
    assert!(!generated.contains("_ =>"));
    for kind in [
        action::InvestigationActionKind::InspectSite,
        action::InvestigationActionKind::SearchArea,
        action::InvestigationActionKind::FollowTracks,
        action::InvestigationActionKind::ReacquireTracks,
        action::InvestigationActionKind::LocateContact,
        action::InvestigationActionKind::Watch,
        action::InvestigationActionKind::Patrol,
        action::InvestigationActionKind::LayAmbush,
        action::InvestigationActionKind::ApproachLead,
    ] {
        assert!(capability_uses_bounded_progress(
            InvestigationProvenanceKind::Generated,
            kind,
        ));
        assert!(!capability_uses_bounded_progress(
            InvestigationProvenanceKind::Manual,
            kind,
        ));
    }
    assert!(reducer.contains("private_action_resolution_json"));
    let receipt = source
        .split("fn private_action_resolution_json")
        .nth(1)
        .and_then(|tail| tail.split("fn capability_progress_depends_on_exact_lead").next())
        .expect("bounded progress receipt serializer");
    assert!(receipt.contains("\"attempt_number\""));
    assert!(receipt.contains("\"persistent_progress_bps\""));
    assert!(receipt.contains("\"success_threshold_bps\""));
    assert!(receipt.contains("\"guaranteed_by_attempt\""));
}

#[test]
fn exact_site_projection_and_travel_require_explicit_case_provenance() {
    let source = INVESTIGATION_SOURCE;
    let compact_source: String = source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let forbidden_view_scan = ["quest_generation_authority()", ".iter()"].concat();
    assert!(!compact_source.contains(&forbidden_view_scan));
    let provenance = source
        .split("fn validated_case_site_aliases")
        .nth(1)
        .and_then(|tail| tail.split("fn case_site_provenance_view").next())
        .unwrap();
    for required in [
        "InvestigationProvenanceKind::Manual",
        "case.generated_case_id.is_empty() && authorities.is_empty()",
        "InvestigationProvenanceKind::Generated",
        "case.generated_case_id == case.id && authorities.len() == 1",
        "validate_quest_generation_authority",
        "validated.manifest.canonical_case_id == case.id",
    ] {
        assert!(provenance.contains(required), "{required}");
    }
    let pins = source
        .split("pub fn backend_case_site_pins")
        .nth(1)
        .and_then(|tail| tail.split("fn lead_projects_exact_case_site_pin").next())
        .unwrap();
    let travel = source
        .split("pub(crate) fn exact_case_site_for_observer_at")
        .nth(1)
        .and_then(|tail| tail.split("pub(crate) fn case_site_presence_for_observer").next())
        .unwrap();
    assert!(pins.contains("case_site_provenance_view"));
    assert!(travel.contains("case_site_provenance_reducer"));
    for consumer in [
        "fn exact_action_site_for_observer",
        "fn exact_action_case_site_for_observer",
    ] {
        let body = source.split(consumer).nth(1).unwrap();
        assert!(body.contains("case_site_provenance_"));
    }
}

#[test]
fn exact_site_provenance_accepts_only_valid_manual_or_generated_tuples() {
    use adventuresim_core::{
        local_problem::Scope,
        quest_generation::{GenerationContext, TemplateFamily, generate, test_witnesses},
    };
    let context = GenerationContext {
        seed: 19,
        observer_entropy_hi: 23,
        observer_entropy_lo: 29,
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
    let context_snapshot_json = serde_json::to_string(&context).unwrap();
    let authority = crate::strategic::QuestGenerationAuthority {
        case_id: manifest.canonical_case_id.clone(),
        public_case_id: manifest.public_case_id.clone(),
        settlement_id: context.settlement_id.clone(),
        settlement_name: context.settlement_name.clone(),
        seed: context.seed,
        catalog_revision: manifest.catalog_revision.clone(),
        context_commitment: crate::strategic::quest_generation_context_commitment(
            &context_snapshot_json,
        )
        .unwrap(),
        context_snapshot_json,
        manifest_json: serde_json::to_string(&manifest).unwrap(),
        factor_trace_json: serde_json::to_string(&manifest.factor_trace).unwrap(),
    };
    let generated_case = crate::strategic::CaseAuthority {
        id: manifest.canonical_case_id.clone(),
        investigation_case_id: manifest.canonical_case_id.clone(),
        provenance_kind: InvestigationProvenanceKind::Generated,
        generated_case_id: manifest.canonical_case_id.clone(),
        local_problem_id: Some(manifest.problem_id.clone()),
        objective_expression_json: serde_json::to_string(&manifest.objectives).unwrap(),
        resolution_status: crate::strategic::CaseStatus::Open,
        resolved_by_party_id: None,
    };
    assert_eq!(
        validated_case_site_aliases(&generated_case, [authority.clone()]),
        Some(Some((
            manifest.canonical_case_id.clone(),
            manifest.public_case_id.clone()
        )))
    );
    assert_eq!(
        validated_case_site_aliases(&generated_case, std::iter::empty()),
        None
    );
    let mut corrupt = authority.clone();
    corrupt.context_commitment = "corrupt".into();
    assert_eq!(
        validated_case_site_aliases(&generated_case, [corrupt]),
        None
    );
    let manual = crate::strategic::CaseAuthority {
        id: "manual-case".into(),
        investigation_case_id: "manual-case".into(),
        provenance_kind: InvestigationProvenanceKind::Manual,
        generated_case_id: String::new(),
        local_problem_id: None,
        objective_expression_json: "{}".into(),
        resolution_status: crate::strategic::CaseStatus::Open,
        resolved_by_party_id: None,
    };
    assert_eq!(
        validated_case_site_aliases(&manual, std::iter::empty()),
        Some(None)
    );
    let mut collision = authority;
    collision.public_case_id = manual.id.clone();
    assert_eq!(validated_case_site_aliases(&manual, [collision]), None);
}

use super::*;

#[test]
fn explicit_secondary_referral_and_context_are_exact() {
    use adventuresim_core::{
        local_problem::Scope,
        quest_generation::{GenerationContext, TemplateFamily, generate, test_witnesses},
    };
    let generated = generate(&GenerationContext {
        seed: 7,
        observer_entropy_hi: 11,
        observer_entropy_lo: 13,
        settlement_id: "riverdale".into(),
        settlement_name: "Riverdale".into(),
        scope: Scope::Settlement {
            settlement_id: "riverdale".into(),
        },
        ordinal: 0,
        now_minute: 50_000,
        incident_weather: adventuresim_core::weather::Precipitation::Clear,
        requested_family: Some(TemplateFamily::RecurringDepredation),
        witness_candidates: test_witnesses(),
    })
    .unwrap();
    let primary = &generated.witnesses[0];
    let secondary = &generated.witnesses[1];
    let referred = authored_witness_referrals(&generated, primary, &primary.testimony[0])
        .expect("primary account has an authored referral");
    assert_eq!(referred, vec![secondary]);
    assert!(
        authored_witness_referrals(&generated, secondary, &secondary.testimony[0])
            .unwrap()
            .is_empty()
    );

    let referral = InvestigationWitnessReferral {
        id: "referral".into(),
        owner_character_id: 7,
        canonical_case_id: generated.canonical_case_id.clone(),
        public_case_id: generated.public_case_id.clone(),
        witness_resident_character_id: secondary.resident_character_id,
        expected_settlement_id: "riverdale".into(),
        expected_location_id: secondary.expected_location.clone(),
        grant_kind: "testimony".into(),
        source_receipt_id: "testimony-receipt".into(),
        source_witness_id: primary.id.0.clone(),
        source_witness_resident_character_id: primary.resident_character_id,
        source_testimony_index: 0,
        source_proposition_id: primary.testimony[0].proposition_id.clone(),
        catalog_revision: generated.catalog_revision.clone(),
        granted_at: 50_000,
    };
    assert!(witness_referral_context_matches(
        &referral,
        7,
        &referral.canonical_case_id,
        secondary.resident_character_id,
        "riverdale",
        &secondary.expected_location,
    ));
    for mismatch in [
        (
            8,
            secondary.resident_character_id,
            "riverdale",
            secondary.expected_location.as_str(),
        ),
        (
            7,
            primary.resident_character_id,
            "riverdale",
            secondary.expected_location.as_str(),
        ),
        (
            7,
            secondary.resident_character_id,
            "elsewhere",
            secondary.expected_location.as_str(),
        ),
        (7, secondary.resident_character_id, "riverdale", "wrong-tab"),
    ] {
        assert!(!witness_referral_context_matches(
            &referral,
            mismatch.0,
            &referral.canonical_case_id,
            mismatch.1,
            mismatch.2,
            mismatch.3,
        ));
    }

    assert!(validate_referral_manifest_provenance(&referral, &generated, secondary).is_ok());
    for malformed in [
        InvestigationWitnessReferral {
            source_witness_id: "witness:wrong".into(),
            ..referral.clone()
        },
        InvestigationWitnessReferral {
            source_testimony_index: 1,
            ..referral.clone()
        },
        InvestigationWitnessReferral {
            source_proposition_id: "proposition:wrong".into(),
            ..referral.clone()
        },
        InvestigationWitnessReferral {
            grant_kind: "unknown".into(),
            ..referral.clone()
        },
    ] {
        assert!(validate_referral_manifest_provenance(&malformed, &generated, secondary).is_err());
    }
    let mut absent_edge_case = generated.clone();
    absent_edge_case.witnesses[0].testimony[0]
        .referred_witness_ids
        .clear();
    assert!(
        validate_referral_manifest_provenance(
            &referral,
            &absent_edge_case,
            &absent_edge_case.witnesses[1],
        )
        .is_err()
    );
    let mut ambiguous_edge_case = generated.clone();
    ambiguous_edge_case.witnesses[0].testimony[0]
        .referred_witness_ids
        .push(secondary.id.clone());
    assert!(
        validate_referral_manifest_provenance(
            &referral,
            &ambiguous_edge_case,
            &ambiguous_edge_case.witnesses[1],
        )
        .is_err()
    );

    let initial = InvestigationWitnessReferral {
        grant_kind: "initial_rumor".into(),
        source_receipt_id: "receipt:primary".into(),
        source_witness_id: String::new(),
        source_witness_resident_character_id: 9_007_199_254_740_991,
        source_testimony_index: 0,
        source_proposition_id: String::new(),
        witness_resident_character_id: primary.resident_character_id,
        expected_location_id: primary.expected_location.clone(),
        ..referral.clone()
    };
    assert!(validate_referral_manifest_provenance(&initial, &generated, primary).is_ok());
    assert!(validate_referral_manifest_provenance(&initial, &generated, secondary).is_err());
}

#[test]
fn both_generated_families_issue_root_and_successor_action_text() {
    use adventuresim_core::{
        local_problem::Scope,
        quest_generation::{
            GenerationContext, TemplateFamily, generate, observer_scoped_id, test_witnesses,
        },
    };

    for (seed, family) in [
        (7, TemplateFamily::RecurringDepredation),
        (11, TemplateFamily::DisappearanceOrLoss),
    ] {
        let context = GenerationContext {
            seed,
            observer_entropy_hi: seed ^ 0x6f62_7365_7276_6572,
            observer_entropy_lo: seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
            settlement_id: "lubeck".into(),
            settlement_name: "Lubeck".into(),
            scope: Scope::Settlement {
                settlement_id: "lubeck".into(),
            },
            ordinal: 0,
            now_minute: 50_000,
            incident_weather: adventuresim_core::weather::Precipitation::Clear,
            requested_family: Some(family),
            witness_candidates: test_witnesses(),
        };
        let manifest = generate(&context).expect("family should generate");
        for generated_action in manifest
            .actions
            .iter()
            .filter(|action| action.track_segment_id.is_some())
        {
            let segment = manifest
                .track_segments
                .iter()
                .find(|segment| generated_action.track_segment_id.as_ref() == Some(&segment.id))
                .unwrap();
            assert_eq!(
                generated_action_terrain(&manifest, generated_action),
                segment.terrain
            );
        }
        let mut saw_root = false;
        let mut saw_successor = false;
        for action in &manifest.actions {
            let remap = |id: &adventuresim_core::quest_generation::ActionId| {
                observer_scoped_id(&context, "capability", &format!("1:{}", id.0))
            };
            let required_action_id = action.prerequisite.as_ref().map_or_else(String::new, remap);
            saw_root |= required_action_id.is_empty();
            saw_successor |= !required_action_id.is_empty();
            let (known_prerequisites, safe_result) =
                generated_capability_safe_text(&manifest, action);
            validate_investigation_action_text(
                &remap(&action.id),
                &manifest.public_case_id,
                &action.target_id,
                &action.safe_summary,
                &known_prerequisites,
                &safe_result,
                &required_action_id,
                &remap(&action.alternate),
            )
            .expect("generated issuance text should accept an absent root prerequisite");
        }
        assert!(saw_root, "{family:?} did not generate a root action");
        assert!(
            saw_successor,
            "{family:?} did not generate a successor action"
        );
    }
}

#[test]
fn root_rumor_then_every_referred_witness_pipeline_is_valid_in_both_families() {
    use adventuresim_core::{
        investigation::ValidationError,
        local_problem::Scope,
        quest_generation::{GenerationContext, TemplateFamily, generate, test_witnesses, validate},
    };

    for (seed, family) in [
        (7, TemplateFamily::RecurringDepredation),
        (11, TemplateFamily::DisappearanceOrLoss),
    ] {
        let mut context = GenerationContext {
            seed,
            observer_entropy_hi: seed ^ 0x6f62_7365_7276_6572,
            observer_entropy_lo: seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
            settlement_id: "lubeck".into(),
            settlement_name: "Lubeck".into(),
            scope: Scope::Settlement {
                settlement_id: "lubeck".into(),
            },
            ordinal: 0,
            now_minute: 50_000,
            incident_weather: adventuresim_core::weather::Precipitation::Clear,
            requested_family: Some(family),
            witness_candidates: test_witnesses(),
        };
        for (index, witness) in context.witness_candidates.iter_mut().enumerate() {
            witness.resident_character_id = 9_007_199_254_740_993 + index as u64;
        }
        let generated = generate(&context).expect("root rumor should materialize a case");
        validate(&generated).expect("generated action graph should remain valid");
        assert_ne!(generated.canonical_case_id, generated.public_case_id);
        assert!(
            generated.witnesses.len() >= 2,
            "the referral transition needs another authored local account"
        );

        let character_id = 17_849_106_825_763_413_937;
        let mut authored_claims = 0;
        for witness in &generated.witnesses {
            for index in 0..witness.testimony.len() {
                let (receipt_id, pipeline) =
                    adventuresim_core::quest_generation::generated_testimony_pipeline(
                        &context,
                        character_id,
                        &generated,
                        witness,
                        index,
                        50_000,
                    )
                    .expect("referred witness should produce a pipeline");
                assert!(receipt_id.starts_with("testimony:"));
                assert!(receipt_id.len() <= 256);
                let (observation, recollection, claim) =
                    process_investigation_pipeline(pipeline.clone())
                        .expect("every authored witness claim should persist");
                let claim = claim.expect("generated testimony is never omitted");
                for id in [
                    observation.id.as_str(),
                    recollection.id.as_str(),
                    claim.id.as_str(),
                ] {
                    assert!(id.len() <= 256, "pipeline id exceeds stable-id budget");
                    assert!(id.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.')
                    }));
                }
                authored_claims += 1;

                if authored_claims == 1 {
                    let mut invalid = pipeline;
                    invalid.receipt_identity =
                        inv::compound_id(&["generated-testimony", &"x".repeat(257)]);
                    assert_eq!(
                        inv::process_report(invalid.clone()).unwrap_err(),
                        ValidationError::InvalidId
                    );
                    assert_eq!(
                        process_investigation_pipeline(invalid).unwrap_err(),
                        "Invalid investigation pipeline at report processing: InvalidId"
                    );
                }
            }
        }
        assert!(authored_claims >= generated.witnesses.len());
    }
}
