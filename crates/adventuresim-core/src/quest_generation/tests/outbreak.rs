fn outbreak(seed: u64) -> GeneratedCase {
    generate(&context(seed, TemplateFamily::Outbreak)).expect("valid outbreak")
}

#[test]
fn outbreak_catalog_covers_sources_and_all_initial_diseases() {
    use crate::disease::DiseaseId;

    let cases = (0..10).map(outbreak).collect::<Vec<_>>();
    let diseases = cases
        .iter()
        .map(|case| case.outbreak.as_ref().unwrap().disease)
        .collect::<Vec<_>>();
    for disease in [
        DiseaseId::Influenza,
        DiseaseId::Mahrdruck,
        DiseaseId::ShroudFever,
        DiseaseId::Bilwisschuss,
        DiseaseId::Kobeldunst,
    ] {
        assert!(diseases.contains(&disease));
    }
    assert!(cases.iter().any(|case| matches!(
        case.outbreak.as_ref().unwrap().source,
        OutbreakSource::Sanitation { .. }
    )));
    assert!(cases.iter().any(|case| matches!(
        case.outbreak.as_ref().unwrap().source,
        OutbreakSource::Behavior { .. }
    )));
    assert!(cases.iter().any(|case| matches!(
        case.outbreak.as_ref().unwrap().source,
        OutbreakSource::ThreatVector { .. }
    )));
    assert!(cases.iter().any(|case| matches!(
        case.outbreak.as_ref().unwrap().source,
        OutbreakSource::Environmental { .. }
    )));
}

#[test]
fn different_hidden_causes_can_have_the_same_early_presentation() {
    let sanitation = outbreak(0);
    let elemental = outbreak(2);
    assert_ne!(sanitation.outbreak, elemental.outbreak);
    assert_eq!(
        sanitation.consequence.public_summary,
        elemental.consequence.public_summary
    );
    assert_eq!(
        sanitation.witnesses[0].testimony[0].spoken_text,
        elemental.witnesses[0].testimony[0].spoken_text
    );
    assert_eq!(
        sanitation
            .actions
            .iter()
            .filter(|action| action.active_initially)
            .map(|action| action.safe_summary.as_str())
            .collect::<Vec<_>>(),
        elemental
            .actions
            .iter()
            .filter(|action| action.active_initially)
            .map(|action| action.safe_summary.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn every_outbreak_has_two_routes_and_a_complete_non_corpse_path() {
    for seed in 0..64 {
        let case = outbreak(seed);
        let routes = case
            .actions
            .iter()
            .map(|action| action.route)
            .collect::<BTreeSet<_>>();
        assert!(routes.contains(&RouteClass::PhysicalTrail));
        assert!(routes.contains(&RouteClass::SocialInquiry));
        assert!(
            case.actions
                .iter()
                .all(|action| action.target_kind != "corpse")
        );
        assert!(validate(&case).is_ok());
    }
}

#[test]
fn outbreak_needs_physical_evidence_but_not_fabricated_tracks() {
    let case = outbreak(0);
    assert!(case.track_trails.is_empty());
    assert!(case.track_segments.is_empty());
    assert!(validate(&case).is_ok());

    let mut missing_physical_evidence = case;
    let inspection = missing_physical_evidence
        .actions
        .iter_mut()
        .find(|action| {
            action.active_initially
                && action.route == RouteClass::PhysicalTrail
                && action.kind == InvestigationActionKind::InspectSite
        })
        .unwrap();
    inspection
        .outputs
        .retain(|output| !matches!(output, GeneratedActionOutput::Evidence { .. }));
    assert!(
        validate(&missing_physical_evidence)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("physical inspection route"))
    );
}

#[test]
fn outbreak_validator_rejects_incoherent_private_truth_and_routes() {
    let mut chronology = outbreak(0);
    chronology.outbreak.as_mut().unwrap().exposure_chronology[0].exposed_at =
        chronology.outbreak.as_ref().unwrap().exposure_chronology[0]
            .became_symptomatic_at
            .saturating_add(1);
    assert!(
        validate(&chronology)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("chronology"))
    );

    let mut incompatible = outbreak(0);
    incompatible.outbreak.as_mut().unwrap().transmission_route =
        crate::disease::TransmissionVector::Environmental;
    assert!(
        validate(&incompatible)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("transmission route"))
    );

    let mut one_route = outbreak(0);
    one_route
        .actions
        .retain(|action| action.route == RouteClass::PhysicalTrail);
    one_route.actions[0].alternate = one_route.actions[0].id.clone();
    assert!(
        validate(&one_route)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("independent physical"))
    );

    let mut wrong_remediation = outbreak(0);
    wrong_remediation.outbreak.as_mut().unwrap().remediation = OutbreakRemediation::Behavior {
        action: OutbreakBehaviorAction::IsolatePatients,
    };
    assert!(
        validate(&wrong_remediation)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("remediation"))
    );
}

#[test]
fn patient_courses_and_bindings_are_exact_and_carriers_have_no_direct_fix() {
    for seed in 0..20 {
        let case = outbreak(seed);
        let truth = case.outbreak.as_ref().unwrap();
        let refs = truth
            .exposure_chronology
            .iter()
            .map(|exposure| exposure.patient_ref.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(refs.len(), truth.exposure_chronology.len());
        let character_ids = truth
            .exposure_chronology
            .iter()
            .map(|exposure| exposure.patient_character_id)
            .collect::<BTreeSet<_>>();
        let episode_ids = truth
            .exposure_chronology
            .iter()
            .map(|exposure| exposure.episode_id)
            .collect::<BTreeSet<_>>();
        assert_eq!(character_ids.len(), truth.exposure_chronology.len());
        assert_eq!(episode_ids.len(), truth.exposure_chronology.len());
        for exposure in &truth.exposure_chronology {
            let definition = crate::disease::definition(truth.disease);
            assert_eq!(
                exposure.became_symptomatic_at,
                exposure.exposed_at + definition.incubation_minutes
            );
            assert_ne!(exposure.patient_character_id, 0);
            assert!(
                case.witnesses
                    .iter()
                    .any(|witness| witness.resident_character_id
                        == exposure.patient_character_id)
            );
            assert_ne!(
                exposure.family_resident_character_id,
                Some(exposure.patient_character_id)
            );
            if let Some(family_resident_character_id) = &exposure.family_resident_character_id {
                assert!(
                    case.witnesses
                        .iter()
                        .any(|witness| witness.resident_character_id
                            == *family_resident_character_id)
                );
            }
        }
        let physical = case
            .actions
            .iter()
            .find(|action| action.active_initially && action.route == RouteClass::PhysicalTrail)
            .unwrap();
        assert_eq!(physical.target_id, truth.patient_presentation_site.0);
        assert!(case.sites.iter().any(|site| {
            site.id == truth.patient_presentation_site && site.exact_location_initially_known
        }));
        if matches!(
            truth.remediation,
            OutbreakRemediation::ResolveCarrierThreat { .. }
        ) {
            assert!(case.actions.iter().all(|action| {
                action
                    .outputs
                    .iter()
                    .all(|output| !matches!(output, GeneratedActionOutput::Remediation { .. }))
            }));
        }
        assert!(validate(&case).is_ok());
    }
}

#[test]
fn only_the_exact_source_remediation_fact_satisfies_the_case() {
    use crate::case::{CaseId, EvaluationState, OutcomeFact, OutcomeFactId, OutcomeFactKind};

    let case = outbreak(0);
    let case_id = CaseId::new(case.canonical_case_id.clone()).unwrap();
    let remediation_id = match &case.objectives.alternatives[0].objectives[0].requirement {
        ObjectiveRequirement::RemediateSource { remediation_id } => remediation_id.clone(),
        other => panic!("unexpected outbreak objective: {other:?}"),
    };
    let fact = |id: &str, remediation_id: &str| OutcomeFact {
        id: OutcomeFactId::new(format!("fact:{id}")).unwrap(),
        case_id: case_id.clone(),
        party_id: "party:test".into(),
        source_id: format!("source:{id}"),
        happened_at: 10,
        kind: OutcomeFactKind::SourceRemediated {
            remediation_id: remediation_id.into(),
        },
    };
    assert_eq!(
        case.objectives
            .evaluate(
                &case_id,
                "party:test",
                &[fact("wrong", "outbreak-remediation:wrong")]
            )
            .state,
        EvaluationState::Pending
    );
    assert_eq!(
        case.objectives
            .evaluate(&case_id, "party:test", &[fact("right", &remediation_id)])
            .state,
        EvaluationState::Satisfied
    );
}
