#[test]
fn qualitative_deductions_depend_only_on_visible_reports_and_learned_diagnostics() {
    use adventuresim_core::bestiary::{EvidenceKind, ReportDescription};
    let reports = [(ReportDescription::DoglikeBeast, "the shepherd".into())];
    let diagnostics = [
        (
            EvidenceKind::Pawprints,
            "observer-receipt:paws".into(),
            "canine pawprints".into(),
        ),
        (
            EvidenceKind::Pawprints,
            "observer-receipt:paws".into(),
            "canine pawprints".into(),
        ),
    ];
    let first = derive_bestiary_deductions(&reports, &diagnostics).unwrap();
    let same_visible_inputs_under_another_hidden_cause =
        derive_bestiary_deductions(&reports, &diagnostics).unwrap();
    assert_eq!(first, same_visible_inputs_under_another_hidden_cause);
    assert!(!first.is_empty());
    let serialized = format!("{first:?}");
    assert!(!serialized.contains("score"));
    assert!(!serialized.contains("canonical"));
}

#[test]
fn same_inspection_action_retry_is_idempotent_and_scope_checked() {
    let receipt = PhysicalEvidenceInspectionActionReceipt {
        action_id: "same-action".into(),
        owner_character_id: 7,
        evidence_id: "pawprint".into(),
        topic_id: "edges".into(),
    };
    assert!(inspection_action_receipt_matches(
        &receipt, 7, "pawprint", "edges"
    ));
    assert!(!inspection_action_receipt_matches(
        &receipt, 8, "pawprint", "edges"
    ));
    assert!(!inspection_action_receipt_matches(
        &receipt, 7, "other", "edges"
    ));
}

#[test]
fn later_revisit_augments_one_stable_observation_without_duplicates() {
    let first_results = serde_json::to_string(&vec![PersistedBestiaryLoreResult {
        diagnostic_kind: "pawprints".into(),
        interpretation: "This appears to be a canine print.".into(),
    }])
    .unwrap();
    let first = PhysicalEvidenceInspectionAttempt {
        id: "canonical-inspection".into(),
        owner_character_id: 7,
        evidence_id: "pawprint".into(),
        topic_id: "edges".into(),
        stat_label: "Eyesight".into(),
        passed: true,
        narration: "Eyesight check passed: This appears to be a canine print.".into(),
        bestiary_results_json: first_results.clone(),
        attempted_at: 10,
    };

    let (augmented, changed) = augment_physical_evidence_inspection(
        first,
        vec![
            PersistedBestiaryLoreResult {
                diagnostic_kind: "pawprints".into(),
                interpretation: "This duplicate must not create another chip.".into(),
            },
            PersistedBestiaryLoreResult {
                diagnostic_kind: "claw_marks".into(),
                interpretation: "The print could have been made by a transformed werekin.".into(),
            },
        ],
    )
    .unwrap();

    assert!(changed);
    assert_eq!(augmented.id, "canonical-inspection");
    assert_eq!(augmented.attempted_at, 10);
    assert_eq!(augmented.stat_label, "Eyesight");
    assert_eq!(
        augmented.narration,
        "Eyesight check passed: This appears to be a canine print."
    );
    let learned = parse_bestiary_lore_results(&augmented.bestiary_results_json).unwrap();
    assert_eq!(learned.len(), 2);
    assert_eq!(
        learned
            .iter()
            .find(|result| result.diagnostic_kind == "pawprints")
            .unwrap()
            .interpretation,
        "This appears to be a canine print."
    );

    let (unchanged, changed_again) =
        augment_physical_evidence_inspection(augmented, learned.clone()).unwrap();
    assert!(!changed_again);
    assert_eq!(
        parse_bestiary_lore_results(&unchanged.bestiary_results_json)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn failed_physical_observation_can_never_gain_bestiary_results() {
    let failed = PhysicalEvidenceInspectionAttempt {
        id: "failed-inspection".into(),
        owner_character_id: 7,
        evidence_id: "pawprint".into(),
        topic_id: "edges".into(),
        stat_label: "Eyesight".into(),
        passed: false,
        narration: "Eyesight check failed: You cannot make out anything more.".into(),
        bestiary_results_json: "[]".into(),
        attempted_at: 10,
    };
    let (still_failed, changed) = augment_physical_evidence_inspection(
        failed,
        vec![PersistedBestiaryLoreResult {
            diagnostic_kind: "pawprints".into(),
            interpretation: "Must remain private because physical inspection failed.".into(),
        }],
    )
    .unwrap();
    assert!(!changed);
    assert_eq!(still_failed.bestiary_results_json, "[]");
}

#[test]
fn category_gate_persists_successes_only_and_each_result_is_atomic() {
    let implications = [
        BestiaryEvidenceImplication {
            category: BestiaryCategory::Beast,
            lore_difficulty_milli: 1_000,
            diagnostic_kind: Some("pawprints".into()),
            interpretation: "This appears to be a canine print.".into(),
        },
        BestiaryEvidenceImplication {
            category: BestiaryCategory::Werekin,
            lore_difficulty_milli: 2_000,
            diagnostic_kind: Some("pawprints".into()),
            interpretation: "A transformed werekin is possible.".into(),
        },
    ];
    let results = successful_bestiary_lore_results(&implications, |category, _| {
        category == BestiaryCategory::Beast
    });
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].diagnostic_kind, "pawprints");
    let serialized = serde_json::to_string(&results).unwrap();
    assert!(!serialized.contains("difficulty"));
    assert!(!serialized.contains("werekin"));
}

#[test]
fn persisted_bestiary_lore_contains_only_successful_diagnostic_provenance() {
    let serialized = serde_json::to_string(&PersistedBestiaryLoreResult {
        diagnostic_kind: "pawprints".into(),
        interpretation: "This appears to be a canine print.".into(),
    })
    .unwrap();

    assert!(serialized.contains("\"diagnostic_kind\":\"pawprints\""));
    assert!(!serialized.contains("support_bps"));
    assert!(!serialized.contains("category"));
    assert!(!serialized.contains("difficulty"));
    assert!(!serialized.contains("threat"));
    assert!(!serialized.contains("failed"));
    assert!(
        parse_bestiary_lore_results(
            r#"[{"diagnostic_kind":"pawprints","interpretation":"print","difficulty_milli":1}]"#
        )
        .is_err()
    );
}

fn failed_attempt(
    id: &str,
    capability_id: &str,
    owner_character_id: u64,
    method: &str,
    expected_version: u32,
    success: bool,
) -> InvestigationActionAttempt {
    InvestigationActionAttempt {
        id: id.into(),
        capability_id: capability_id.into(),
        owner_character_id,
        expected_version,
        method: method.into(),
        started_at: 0,
        completed_at: 1,
        duration_minutes: 1,
        success,
        resulting_uncertainty_bps: 9_000,
        private_resolution_json: format!(
            r#"{{"result":"no_new_information","success":{success},"cost":{{"minutes":1,"fatigue":0,"food_units":0,"water_units":0}},"resulting_uncertainty_bps":9000,"risk_bps":0,"risk_triggered":false,"effective_skill_bps":0}}"#
        ),
    }
}

fn exact_lead(owner: u64, case_id: &str, site_id: &str) -> InvestigationLead {
    InvestigationLead {
        id: "lead".into(),
        owner_character_id: owner,
        case_id: case_id.into(),
        proposition_id: "proposition".into(),
        summary: "Exact lead".into(),
        source_label: "witness".into(),
        confidence_bps: 8_000,
        destination_stage: DestinationKnowledgeStage::ExactBelieved,
        directions: String::new(),
        exact_location_id: site_id.into(),
        latitude_e7: 1,
        longitude_e7: 2,
        witness_name: String::new(),
        witness_description: String::new(),
        witness_occupation_or_relationship: String::new(),
        expected_location: String::new(),
        current_learned_location: String::new(),
        contradiction_group: "group".into(),
        corrected_by: String::new(),
        recorded_at: 0,
    }
}

fn exact_capability(owner: u64, case_id: &str, site_id: &str) -> InvestigationActionCapability {
    InvestigationActionCapability {
        id: "capability".into(),
        owner_character_id: owner,
        case_id: case_id.into(),
        provenance_kind: InvestigationProvenanceKind::Generated,
        generated_case_id: "canonical-case".into(),
        method: "inspect_site".into(),
        version: 1,
        target_kind: "site".into(),
        target_id: site_id.into(),
        target_terrain: "forest".into(),
        seed: 1,
        evidence_age_origin_minute: 0,
        uncertainty_bps: 9_000,
        safe_summary: "Inspect".into(),
        known_prerequisites: String::new(),
        safe_result_on_success: "Found".into(),
        consequence_json: "{}".into(),
        required_action_id: String::new(),
        alternate_route_action_id: String::new(),
        active: true,
    }
}

#[test]
fn tracking_chain_requires_completed_same_case_area_route_site_provenance() {
    let area = InvestigationActionCapability {
        id: "search-area".into(),
        method: "search_area".into(),
        target_kind: "area".into(),
        target_id: "area-a".into(),
        ..exact_capability(7, "case-a", "site-a")
    };
    let route = InvestigationActionCapability {
        id: "reacquire-route".into(),
        method: "reacquire_tracks".into(),
        target_kind: "route".into(),
        target_id: "route-a".into(),
        required_action_id: area.id.clone(),
        ..area.clone()
    };
    let site = InvestigationActionCapability {
        id: "follow-site".into(),
        method: "follow_tracks".into(),
        target_kind: "site".into(),
        target_id: "site-a".into(),
        required_action_id: route.id.clone(),
        ..route.clone()
    };
    let capabilities = BTreeMap::from([
        (area.id.clone(), area.clone()),
        (route.id.clone(), route.clone()),
        (site.id.clone(), site.clone()),
    ]);
    let completed = BTreeSet::from([area.id.clone(), route.id.clone()]);
    assert!(tracking_capability_chain_is_coherent(
        &site,
        action::InvestigationActionKind::FollowTracks,
        |id| capabilities.get(id).cloned(),
        |id| completed.contains(id),
    ));

    assert!(!tracking_capability_chain_is_coherent(
        &site,
        action::InvestigationActionKind::FollowTracks,
        |id| capabilities.get(id).cloned(),
        |id| id == area.id,
    ));

    let crossed_route = InvestigationActionCapability {
        owner_character_id: 8,
        ..route.clone()
    };
    let crossed = BTreeMap::from([
        (area.id.clone(), area.clone()),
        (route.id.clone(), crossed_route),
    ]);
    assert!(!tracking_capability_chain_is_coherent(
        &site,
        action::InvestigationActionKind::FollowTracks,
        |id| crossed.get(id).cloned(),
        |id| completed.contains(id),
    ));

    let direct_site = InvestigationActionCapability {
        required_action_id: area.id.clone(),
        ..site
    };
    assert!(!tracking_capability_chain_is_coherent(
        &direct_site,
        action::InvestigationActionKind::FollowTracks,
        |id| capabilities.get(id).cloned(),
        |id| completed.contains(id),
    ));
}

#[test]
fn testimony_correction_resets_only_exact_dependent_progress_chain() {
    let lead = exact_lead(7, "public-case", "site-a");
    let capability = exact_capability(7, "canonical-case", "site-a");
    assert!(capability_progress_depends_on_exact_lead(
        &capability,
        &lead,
        Some(("canonical-case", "public-case")),
    ));
    for unrelated in [
        InvestigationActionCapability {
            id: "other-capability".into(),
            target_id: "site-b".into(),
            ..capability.clone()
        },
        InvestigationActionCapability {
            owner_character_id: 8,
            ..capability.clone()
        },
        InvestigationActionCapability {
            case_id: "other-case".into(),
            ..capability.clone()
        },
        InvestigationActionCapability {
            active: false,
            ..capability.clone()
        },
        InvestigationActionCapability {
            provenance_kind: InvestigationProvenanceKind::Manual,
            generated_case_id: String::new(),
            ..capability.clone()
        },
    ] {
        assert!(!capability_progress_depends_on_exact_lead(
            &unrelated,
            &lead,
            Some(("canonical-case", "public-case")),
        ));
    }
    let failure = failed_attempt("attempt-0", "capability", 7, "inspect_site", 0, false);
    assert_eq!(
        contiguous_failed_attempts("capability", 7, "inspect_site", 1, [failure.clone()]),
        1
    );
    // Correction bumps version without manufacturing an attempt. Relearning
    // the same site leaves that gap intact, so progress restarts at attempt 1.
    assert_eq!(
        contiguous_failed_attempts("capability", 7, "inspect_site", 2, [failure]),
        0
    );
    let restarted = (2..7)
        .map(|version| {
            failed_attempt(
                &format!("attempt-{version}"),
                "capability",
                7,
                "inspect_site",
                version,
                false,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        contiguous_failed_attempts("capability", 7, "inspect_site", 7, restarted),
        action::GENERATED_ACTION_ATTEMPT_BOUND - 1
    );
    assert!(!exact_site_knowledge_is_live(
        "public-case",
        "site-a",
        "public-case",
        "site-a",
        DestinationKnowledgeStage::ExactBelieved,
        "correction-lead",
        "canonical-case",
        "site-a",
        true,
        Some("canonical-case"),
        Some("public-case"),
    ));
}

#[test]
fn correction_paths_reset_after_invalidation_and_replay_before_mutation() {
    let source = INVESTIGATION_SOURCE;
    let generated = source
        .split("pub(crate) fn persist_generated_testimony")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .unwrap();
    assert!(
        generated.find("investigation_safe_claim_receipt").unwrap()
            < generated
                .find("dependent_capability_ids_for_exact_lead")
                .unwrap()
    );
    assert!(
        generated
            .find("prior.corrected_by = lead_id.clone()")
            .unwrap()
            < generated
                .rfind("reset_unsupported_capability_progress")
                .unwrap()
    );
    let generic = source
        .split("pub fn discover_investigation_lead")
        .nth(1)
        .and_then(|tail| tail.split("fn same_place").next())
        .unwrap();
    assert!(
        generic.find("idempotent(").unwrap()
            < generic
                .find("reset_unsupported_capability_progress")
                .unwrap()
    );
    assert!(generic.contains("invalidated_live_support"));
    assert!(
        generic.find("investigation_lead().insert").unwrap()
            < generic
                .find("reset_unsupported_capability_progress")
                .unwrap()
    );
    assert!(
        generic
            .find("reset_unsupported_capability_progress")
            .unwrap()
            < generic.find("receipt.consumed_by").unwrap()
    );
    let reset_revision = source
        .split("fn reset_capability_progress_if_unsupported")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn reset_unsupported_capability_progress")
                .next()
        })
        .unwrap();
    assert!(reset_revision.contains("capability.version.saturating_add(1)"));
    assert!(reset_revision.contains("capability.seed = replacement_seed()"));
    let reset = source
        .split("fn reset_unsupported_capability_progress")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn perform_investigation_action_authorized")
                .next()
        })
        .unwrap();
    assert!(reset.contains("unique_capability_ids"));
    assert!(reset.contains("reset_capability_progress_if_unsupported"));
    assert!(reset.contains("exact_action_case_site_for_observer"));
    assert!(reset.contains("ctx.random"));
}

#[test]
fn correction_reset_waits_for_final_replacement_support() {
    let failure = failed_attempt("attempt-0", "capability", 7, "inspect_site", 0, false);
    let mut same_site_replacement = exact_capability(7, "public-case", "site-a");
    assert_eq!(same_site_replacement.version, 1);
    assert_eq!(
        contiguous_failed_attempts(
            "capability",
            7,
            "inspect_site",
            same_site_replacement.version,
            [failure.clone()]
        ),
        1
    );
    assert!(!reset_capability_progress_if_unsupported(
        &mut same_site_replacement,
        true,
        || panic!("a supported correction must not consume a replacement seed")
    ));
    assert_eq!(same_site_replacement.version, 1);
    assert_eq!(same_site_replacement.seed, 1);
    assert_eq!(
        contiguous_failed_attempts(
            "capability",
            7,
            "inspect_site",
            same_site_replacement.version,
            [failure.clone()]
        ),
        1
    );

    let mut unsupported_replacement = exact_capability(7, "public-case", "site-a");
    assert!(reset_capability_progress_if_unsupported(
        &mut unsupported_replacement,
        false,
        || 77
    ));
    assert_eq!(unsupported_replacement.version, 2);
    assert_eq!(unsupported_replacement.seed, 77);
    assert_eq!(
        contiguous_failed_attempts(
            "capability",
            7,
            "inspect_site",
            unsupported_replacement.version,
            [failure]
        ),
        0
    );
    assert!(!reset_capability_progress_if_unsupported(
        &mut unsupported_replacement,
        true,
        || panic!("replay must not consume a replacement seed")
    ));
    assert_eq!(unsupported_replacement.version, 2);
    assert_eq!(unsupported_replacement.seed, 77);
}

#[test]
fn testimony_correction_dedupes_caps_and_preserves_replacement_support() {
    let unique = unique_capability_ids([
        "cap-a".to_string(),
        "cap-a".to_string(),
        "cap-b".to_string(),
        "cap-b".to_string(),
    ]);
    assert_eq!(
        unique,
        BTreeSet::from(["cap-a".to_string(), "cap-b".to_string()])
    );
    let mut reset_counts = BTreeMap::<String, u32>::new();
    for capability_id in unique {
        *reset_counts.entry(capability_id).or_default() += 1;
    }
    assert_eq!(reset_counts["cap-a"], 1);
    assert_eq!(reset_counts["cap-b"], 1);
    assert!(correction_requires_progress_reset(false));
    assert!(!correction_requires_progress_reset(true));

    let mut first_lead = exact_lead(7, "public-case", "site-a");
    first_lead.id = "lead-a".into();
    let mut second_lead = first_lead.clone();
    second_lead.id = "lead-b".into();
    let first_capability = exact_capability(7, "canonical-case", "site-a");
    let mut second_capability = first_capability.clone();
    second_capability.id = "cap-b".into();
    let corrected_leads = [first_lead, second_lead];
    let dependent_ids = corrected_leads.iter().flat_map(|lead| {
        [&first_capability, &second_capability]
            .into_iter()
            .filter(|&capability| {
                capability_progress_depends_on_exact_lead(
                    capability,
                    lead,
                    Some(("canonical-case", "public-case")),
                )
            })
            .map(|capability| capability.id.clone())
    });
    assert_eq!(
        unique_capability_ids(dependent_ids),
        BTreeSet::from(["capability".to_string(), "cap-b".to_string()])
    );
}
