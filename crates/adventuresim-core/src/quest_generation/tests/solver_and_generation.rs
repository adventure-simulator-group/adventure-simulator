#[test]
fn deterministic_and_counterfactual() {
    let a = generate(&context(41, TemplateFamily::DisappearanceOrLoss)).unwrap();
    assert_eq!(
        a,
        generate(&context(41, TemplateFamily::DisappearanceOrLoss)).unwrap()
    );
    let b = generate(&context(42, TemplateFamily::DisappearanceOrLoss)).unwrap();
    assert_eq!(a.consequence.symptom, b.consequence.symptom);
    assert_ne!((a.cause, a.sites[0].kind), (b.cause, b.sites[0].kind));
}
#[test]
fn descriptions_are_ambiguous() {
    for seed in 0..256 {
        let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
        assert!(
            crate::bestiary::ambiguous_description_cardinality(generated.witnesses[0].description)
                >= 2
        );
    }
}

#[test]
fn evidence_descriptions_do_not_expose_internal_variant_names() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        for seed in 0..256 {
            let generated = generate(&context(seed, family)).unwrap();
            for evidence in generated.evidence {
                assert!(
                    !evidence
                        .safe_description
                        .contains(&format!("{:?}", evidence.kind)),
                    "seed {seed} exposed {:?} in player-facing evidence",
                    evidence.kind
                );
            }
        }
    }
}

#[test]
fn follow_up_incidents_reuse_conditional_evidence_likelihoods() {
    let skeleton = (0..10_000)
        .filter_map(|entropy| {
            select_follow_up_evidence(
                CanonicalCause::Hostile(ThreatId::Skeleton),
                SiteKind::Crypt,
                crate::settlement_population::stable_hash(&format!("skeleton:{entropy}")),
            )
        })
        .filter(|kind| *kind == EvidenceKind::BoneDust)
        .count();
    let bandit = (0..10_000)
        .filter_map(|entropy| {
            select_follow_up_evidence(
                CanonicalCause::Hostile(ThreatId::Bandit),
                SiteKind::Crypt,
                crate::settlement_population::stable_hash(&format!("bandit:{entropy}")),
            )
        })
        .filter(|kind| *kind == EvidenceKind::BoneDust)
        .count();
    assert!(skeleton > bandit);
    assert!((0..10_000).all(|entropy| {
        select_follow_up_evidence(
            CanonicalCause::FabricatedClaim,
            SiteKind::OccupiedHouse,
            entropy,
        ) != Some(EvidenceKind::BloodlessCorpse)
    }));
}

#[test]
fn hard_zero_and_rare_rules_are_auditable() {
    let mut trace = Vec::new();
    let _ = choose(
        1,
        "module.site",
        "relation.site.cause",
        &site_candidates(CanonicalCause::Hostile(ThreatId::Skeleton)),
        &mut trace,
    )
    .unwrap();
    let house = trace
        .iter()
        .find(|t| t.candidate_id == "occupied_house")
        .unwrap();
    assert_eq!(house.plausibility, 3);
    assert_eq!(
        house.required_bridge.as_ref().unwrap().0,
        "skeletons_occupied_house"
    );
    let wolf_crypt = site_candidates(CanonicalCause::Hostile(ThreatId::Wolf))
        .into_iter()
        .find(|c| c.id == "crypt")
        .unwrap();
    assert_eq!(wolf_crypt.weight.plausibility, 0);
    assert!(wolf_crypt.impossible.is_some());
}
#[test]
fn child_adult_venue_is_rare_but_bridged() {
    let adult = circumstance_candidates(WitnessDemographic::Child)
        .into_iter()
        .find(|c| c.value == Circumstance::AdultVenue)
        .unwrap();
    assert_eq!(adult.weight.plausibility, 2);
    assert_eq!(adult.bridge, Some("child_at_adult_venue"));
}

#[test]
fn truthful_spoken_location_matches_its_bound_site() {
    for seed in 0..128 {
        let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
        let statement = &generated.witnesses[0].testimony[0];
        if statement.reliability != Reliability::Truthful {
            continue;
        }
        let site_id = statement.site_id.as_ref().unwrap();
        let site = generated
            .sites
            .iter()
            .find(|candidate| candidate.id == *site_id)
            .unwrap();
        assert!(
            statement.spoken_text.contains(&site.safe_label),
            "truthful spoken site {:?} did not match bound site {:?}",
            statement.spoken_text,
            site.safe_label
        );
    }
}

#[test]
fn selected_yaml_bridge_materializes_complete_reachable_authority() {
    let generated = (0..4_096)
        .find_map(|seed| {
            let generated = generate(&context(seed, TemplateFamily::DisappearanceOrLoss)).ok()?;
            (!generated.bridges.is_empty()).then_some(generated)
        })
        .expect("bounded seeds must select a YAML-authored bridge");
    validate(&generated).unwrap();

    let mut reachable = generated
        .actions
        .iter()
        .filter(|action| action.active_initially)
        .map(|action| action.id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let before = reachable.len();
        for action in &generated.actions {
            if action
                .prerequisite
                .as_ref()
                .is_none_or(|required| reachable.contains(required))
            {
                reachable.insert(action.id.clone());
            }
        }
        if reachable.len() == before {
            break;
        }
    }

    for bridge in &generated.bridges {
        assert!(
            generated
                .canonical_events
                .iter()
                .any(|event| event.id == bridge.event_id)
        );
        assert!(
            generated
                .evidence
                .iter()
                .any(|evidence| evidence.id == bridge.evidence_id)
        );
        assert!(reachable.contains(&bridge.action_id));
        let action = generated
            .actions
            .iter()
            .find(|action| action.id == bridge.action_id)
            .unwrap();
        assert!(action.outputs.iter().any(|output| {
            matches!(
                output,
                GeneratedActionOutput::Evidence { evidence_id }
                    if evidence_id == &bridge.evidence_id
            )
        }));
    }
}

#[test]
fn graph_keeps_both_routes_reachable_from_authored_entries() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        let case = generate(&context(7, family)).unwrap();
        validate(&case).unwrap();
        let routes = case
            .actions
            .iter()
            .map(|a| a.route)
            .collect::<BTreeSet<_>>();
        assert_eq!(routes.len(), 2);
        if family == TemplateFamily::RecurringDepredation {
            let roots = case
                .actions
                .iter()
                .filter(|action| action.active_initially)
                .collect::<Vec<_>>();
            assert_eq!(roots.len(), 1);
            assert_eq!(roots[0].kind, InvestigationActionKind::LocateContact);
            let unlocked = case
                .actions
                .iter()
                .filter(|action| action.prerequisite.as_ref() == Some(&roots[0].id))
                .map(|action| action.route)
                .collect::<BTreeSet<_>>();
            assert_eq!(unlocked, routes);
        } else {
            for route in routes {
                assert!(
                    case.actions
                        .iter()
                        .any(|action| action.route != route && action.active_initially)
                );
            }
        }
    }
}
#[test]
fn disappearance_truth_selects_only_compatible_targets_and_producers() {
    for seed in 0..256 {
        let generated = generate(&context(seed, TemplateFamily::DisappearanceOrLoss)).unwrap();
        assert_ne!(generated.cause, CanonicalCause::VoluntaryDisappearance);
        validate(&generated).unwrap();
        match generated.cause {
            CanonicalCause::Hostile(_) | CanonicalCause::ConcealmentByWitness => {
                assert_eq!(generated.finales.len(), 1);
                assert_eq!(generated.finales[0].kind, FinaleKind::Rescue);
                let subjects = generated
                    .actions
                    .iter()
                    .flat_map(|action| &action.outputs)
                    .filter_map(|output| match output {
                        GeneratedActionOutput::Consequence {
                            consequence:
                                GeneratedActionConsequence::RescueSubject { subject_id, .. },
                        } => Some(subject_id),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(subjects.len(), 2);
                assert_eq!(subjects[0], subjects[1]);
                assert!(
                    generated
                        .actions
                        .iter()
                        .filter(|action| {
                            action.outputs.iter().any(|output| {
                                matches!(output, GeneratedActionOutput::Consequence { .. })
                            })
                        })
                        .all(|action| {
                            action.kind == InvestigationActionKind::InspectSite
                                && action.target_kind == "site"
                        })
                );
            }
            CanonicalCause::IncidentalLoss => {
                assert_eq!(generated.finales[0].kind, FinaleKind::RetrieveReturn);
                assert!(
                    generated.dialogue_producers.iter().any(|producer| {
                        producer.action == GeneratedDialogueAction::ReturnAsset
                    })
                );
            }
            CanonicalCause::FabricatedClaim => {
                assert_eq!(generated.finales[0].kind, FinaleKind::Expose);
                assert!(
                    generated
                        .dialogue_producers
                        .iter()
                        .any(|producer| { producer.action == GeneratedDialogueAction::Expose })
                );
            }
            CanonicalCause::VoluntaryDisappearance => unreachable!(),
        }
    }
}
#[test]
fn secondary_location_testimony_corresponds_to_the_primary_presented_site() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        for truthful in [true, false] {
            let generated = case_with_primary_location_accuracy(family, truthful);
            validate(&generated).unwrap();

            let primary = &generated.witnesses[0].testimony[0];
            let secondary = &generated.witnesses[1].testimony[0];
            let primary_site = generated
                .sites
                .iter()
                .find(|site| Some(&site.id) == primary.site_id.as_ref())
                .expect("primary presented site is bound in the manifest");
            let finale_site = generated
                .sites
                .iter()
                .find(|site| site.role == SiteRole::Finale)
                .expect("generated case has a finale site");

            assert_eq!(secondary.proposition_id, primary.proposition_id);
            assert_eq!(secondary.site_id.as_ref(), Some(&finale_site.id));
            assert!(
                primary.spoken_text.contains(label(primary_site.kind)),
                "primary wording does not name its presented site kind"
            );

            if truthful {
                assert_eq!(primary_site.id, finale_site.id);
                assert_eq!(secondary.corrects_proposition_id, None);
                assert!(
                    secondary.spoken_text.contains(label(primary_site.kind))
                        && secondary.spoken_text.contains("continue toward")
                        && secondary
                            .spoken_text
                            .contains("consistent with the earlier account")
                        && !secondary.spoken_text.contains("turn away"),
                    "truthful branch did not continue toward the primary's true site: {:?}",
                    secondary.spoken_text
                );
            } else {
                assert_eq!(primary_site.role, SiteRole::Decoy);
                assert_ne!(primary_site.id, finale_site.id);
                assert_eq!(
                    secondary.corrects_proposition_id.as_deref(),
                    Some(primary.proposition_id.as_str())
                );
                assert!(
                    secondary.spoken_text.contains(label(primary_site.kind))
                        && secondary.spoken_text.contains("turn away before reaching")
                        && secondary.spoken_text.contains("continue elsewhere"),
                    "mistaken branch did not correct away from the primary's presented decoy: {:?}",
                    secondary.spoken_text
                );
            }
        }
    }
}
#[test]
fn marginal_sweep_is_bounded_and_has_both_templates() {
    let result = audit(256);
    assert!(result[&TemplateFamily::RecurringDepredation] > 80);
    assert!(result[&TemplateFamily::DisappearanceOrLoss] > 80);
}
#[test]
fn public_identity_is_opaque_and_generated_cases_have_no_contract() {
    let case = generate(&context(88, TemplateFamily::RecurringDepredation)).unwrap();
    assert_ne!(case.canonical_case_id, case.public_case_id);
    let public = serde_json::json!({
        "case_id": case.public_case_id,
        "problem_id": case.problem_id,
        "actions": case.actions,
        "witnesses": case.witnesses,
        "evidence": case.evidence,
        "areas": case.areas,
    });
    let json = serde_json::to_string(&public).unwrap();
    assert!(!json.contains(&case.canonical_case_id));
    assert!(!json.contains("\"contract\""));
    assert!(!json.contains("scope:"));
}

#[test]
fn public_ids_use_private_entropy_and_do_not_collide_across_cases() {
    let mut first = context(91, TemplateFamily::RecurringDepredation);
    first.observer_entropy_hi = 0x4341_4e4f_4e49_4341;
    first.observer_entropy_lo = 0x4c2d_5345_4e54_494e;
    let mut second = first.clone();
    second.observer_entropy_hi ^= 1;
    second.ordinal = 1;
    let a = generate(&first).unwrap();
    let b = generate(&second).unwrap();
    let ids = |case: &GeneratedCase| {
        case.actions
            .iter()
            .map(|action| action.id.0.clone())
            .chain(case.witnesses.iter().map(|witness| witness.id.0.clone()))
            .chain(
                case.evidence
                    .iter()
                    .map(|evidence| evidence.proposition_id.clone()),
            )
            .chain(case.track_trails.iter().map(|trail| trail.id.0.clone()))
            .chain(
                case.track_segments
                    .iter()
                    .map(|segment| segment.id.0.clone()),
            )
            .chain(std::iter::once(case.public_case_id.clone()))
            .collect::<BTreeSet<_>>()
    };
    let a_ids = ids(&a);
    let b_ids = ids(&b);
    assert!(a_ids.is_disjoint(&b_ids));
    let serialized = serde_json::to_string(&(a_ids, b_ids)).unwrap();
    for sentinel in [
        &a.canonical_case_id,
        &b.canonical_case_id,
        "CANONICAL-SENTINEL",
        "scope:",
    ] {
        assert!(!serialized.contains(sentinel));
    }
}

#[test]
fn every_route_reveals_then_requires_occupied_site_resolution() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        let case = generate(&context(7, family)).unwrap();
        for route in case
            .actions
            .iter()
            .map(|action| action.route)
            .collect::<BTreeSet<_>>()
        {
            let mut route_actions = case.actions.iter().filter(|action| action.route == route);
            let exact = route_actions
                .clone()
                .find(|action| {
                    action.outputs.iter().any(|output| {
                        matches!(
                            output,
                            GeneratedActionOutput::Destination {
                                stage: DestinationKnowledgeStage::ExactBelieved,
                                site_id: Some(_),
                            }
                        )
                    })
                })
                .expect("travel-capable exact-location reveal");
            assert!(matches!(
                exact.kind,
                InvestigationActionKind::FollowTracks
                    | InvestigationActionKind::ReacquireTracks
                    | InvestigationActionKind::ApproachLead
                    | InvestigationActionKind::Patrol
                    | InvestigationActionKind::SearchArea
            ));
            let occupied = route_actions
                .find(|action| {
                    action.target_kind == "site" && action.prerequisite.as_ref() == Some(&exact.id)
                })
                .expect("separate occupied-site resolution");
            assert!(matches!(
                occupied.kind,
                InvestigationActionKind::InspectSite | InvestigationActionKind::LayAmbush
            ));
            assert!(!exact.outputs.iter().any(|output| matches!(
                output,
                GeneratedActionOutput::Consequence { .. }
                    | GeneratedActionOutput::AmbushReady
                    | GeneratedActionOutput::Remediation { .. }
            )));
        }
    }
}

#[test]
fn typed_catalogs_condition_and_enforce_hard_zeros() {
    let child = reliability_candidates(
        WitnessDemographic::Child,
        Circumstance::NightWindow,
        CanonicalCause::Hostile(ThreatId::Goblin),
    );
    assert!(
        child
            .iter()
            .find(|c| c.value == Reliability::Deceptive)
            .unwrap()
            .impossible
            .is_some()
    );
    let fabricated = evidence_candidates(CanonicalCause::FabricatedClaim, SiteKind::OccupiedHouse);
    assert!(
        fabricated
            .iter()
            .find(|c| c.value == EvidenceKind::BloodlessCorpse)
            .unwrap()
            .impossible
            .is_some()
    );
    let mistaken = account_style_candidates(Reliability::Mistaken, Circumstance::RoadJourney);
    let mistaken_tracks = mistaken
        .iter()
        .find(|candidate| candidate.value == AccountStyle::TracksAndMovement)
        .unwrap();
    let truthful_tracks =
        account_style_candidates(Reliability::Truthful, Circumstance::RoadJourney)
            .into_iter()
            .find(|candidate| candidate.value == AccountStyle::TracksAndMovement)
            .unwrap();
    assert_eq!(
        (mistaken_tracks.weight, mistaken_tracks.impossible),
        (truthful_tracks.weight, truthful_tracks.impossible),
        "account wording must not reveal hidden reliability"
    );
    assert_ne!(
        reliability_candidates(
            WitnessDemographic::Guard,
            Circumstance::AdultVenue,
            CanonicalCause::Hostile(ThreatId::Bandit),
        )
        .iter()
        .find(|c| c.value == Reliability::Evasive)
        .unwrap()
        .weight,
        reliability_candidates(
            WitnessDemographic::Guard,
            Circumstance::RoadJourney,
            CanonicalCause::Hostile(ThreatId::Bandit),
        )
        .iter()
        .find(|c| c.value == Reliability::Evasive)
        .unwrap()
        .weight
    );
}

#[test]
fn modular_marginals_vary_without_cause_site_fingerprints() {
    let mut reliabilities = BTreeSet::new();
    let mut secondary_sites = BTreeSet::new();
    let mut secondary_circumstances = BTreeSet::new();
    let mut evidence_kinds = BTreeSet::new();
    let mut account_wordings = BTreeSet::new();
    let mut route_kinds = BTreeSet::new();
    let mut patterns = BTreeSet::new();
    let mut fingerprints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for seed in 0..512 {
        let case = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
        reliabilities.insert(case.witnesses[0].testimony[0].reliability);
        secondary_sites.insert(case.sites[2].kind);
        secondary_circumstances.insert(case.witnesses[1].circumstance);
        evidence_kinds.insert(case.evidence[0].kind);
        account_wordings.insert(
            case.witnesses[0].testimony[0]
                .spoken_text
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" "),
        );
        route_kinds.extend(
            case.actions
                .iter()
                .map(|action| format!("{:?}", action.kind)),
        );
        let pattern = case.canonical_events[0]
            .object
            .split(':')
            .next()
            .unwrap()
            .to_owned();
        patterns.insert(pattern.clone());
        fingerprints
            .entry(format!("{:?}:{:?}", case.cause, case.sites[0].kind))
            .or_default()
            .insert(format!(
                "{:?}:{:?}:{:?}:{pattern}",
                case.witnesses[0].testimony[0].reliability,
                case.sites[2].kind,
                case.evidence[0].kind
            ));
    }
    for (name, cardinality) in [
        ("reliability", reliabilities.len()),
        ("secondary site", secondary_sites.len()),
        ("secondary circumstance", secondary_circumstances.len()),
        ("evidence", evidence_kinds.len()),
        ("account wording", account_wordings.len()),
        ("route behavior", route_kinds.len()),
        ("attack pattern", patterns.len()),
    ] {
        assert!(cardinality >= 2, "{name} collapsed to a fingerprint");
    }
    assert!(
        fingerprints.values().any(|values| values.len() >= 2),
        "cause/site pairs must not determine all downstream modules"
    );
}
