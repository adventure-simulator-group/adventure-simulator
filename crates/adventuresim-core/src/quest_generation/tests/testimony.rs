#[test]
fn every_report_description_has_ambiguous_natural_testimony() {
    let reports = crate::quest_catalog::catalog()
        .documents
        .iter()
        .flat_map(|document| &document.descriptions)
        .map(|description| ReportDescription::try_new(&description.id).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 8);
    for report in reports {
        let prose = ambiguous_report_description(report);
        let claim = ambiguous_visual_claim(report, "the old bridge");
        assert!(!prose.is_empty());
        assert!(
            !claim.contains(&format!("{report:?}")),
            "{report:?} leaked its Rust identifier"
        );
        assert!(claim.starts_with("It looked like "));
        assert!(claim.ends_with(", near the old bridge."));
        assert!(
            !claim.to_ascii_lowercase().contains("definitely"),
            "eyewitness prose must remain uncertain"
        );
    }
}

#[test]
fn account_presentation_candidates_do_not_encode_reliability() {
    for circumstance in [
        Circumstance::SecretRiversideMeeting,
        Circumstance::NightWindow,
        Circumstance::RoadJourney,
    ] {
        let baseline = account_style_candidates(Reliability::Truthful, circumstance)
            .into_iter()
            .map(|candidate| {
                (
                    candidate.id,
                    candidate.value,
                    candidate.weight,
                    candidate.impossible,
                )
            })
            .collect::<Vec<_>>();
        for reliability in [
            Reliability::PartlyTruthful,
            Reliability::Mistaken,
            Reliability::Evasive,
            Reliability::Deceptive,
        ] {
            let other = account_style_candidates(reliability, circumstance)
                .into_iter()
                .map(|candidate| {
                    (
                        candidate.id,
                        candidate.value,
                        candidate.weight,
                        candidate.impossible,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(baseline, other);
        }
    }
}

#[test]
fn generated_location_testimony_has_one_public_grant_shape() {
    for seed in 0..256 {
        let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
        let draft = &generated.witnesses[0].testimony[0];
        assert_eq!(
            draft.destination_stage,
            DestinationKnowledgeStage::RouteSegment
        );
        assert_eq!(draft.referred_witness_ids.len(), 1);
        assert!(!draft.spoken_text.is_empty());
        assert!(!draft.spoken_text.to_ascii_lowercase().contains("truthful"));
        assert!(!draft.spoken_text.to_ascii_lowercase().contains("lying"));
    }
}

#[test]
fn claim_authority_separates_accuracy_from_demeanor_and_ignores_presentation_wording() {
    let generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let mut draft = generated.witnesses[0].testimony[2].clone();
    assert_ne!(draft.spoken_text, draft.truthful_text);
    assert_eq!(
        testimony_claim_authority(&draft),
        TestimonyClaimAuthority {
            factually_accurate: true,
            demeanor_truth_signal: 1.0,
        }
    );

    for delivery in [TestimonyDelivery::Volunteered, TestimonyDelivery::Withheld] {
        draft.delivery = delivery;
        for (reliability, expected) in [
            (Reliability::Truthful, (true, 1.0)),
            (Reliability::Mistaken, (false, 1.0)),
            (Reliability::Evasive, (false, 0.0)),
            (Reliability::Deceptive, (false, -1.0)),
            (Reliability::PartlyTruthful, (false, 0.0)),
        ] {
            draft.reliability = reliability;
            let authority = testimony_claim_authority(&draft);
            assert_eq!(
                (
                    authority.factually_accurate,
                    authority.demeanor_truth_signal
                ),
                expected
            );
        }
    }
}

#[test]
fn private_concern_never_changes_complete_initial_dialogue_shape() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        for seed in 0..256 {
            let mut visible_outputs = BTreeSet::new();
            let mut concern_states = BTreeSet::new();
            for entropy in 0..64 {
                let mut source = context(seed, family);
                source.observer_entropy_hi = entropy;
                source.observer_entropy_lo = entropy.rotate_left(29);
                let generated = generate(&source).unwrap();
                validate(&generated).unwrap();
                let primary = &generated.witnesses[0];
                let visible = initial_testimony_projection(primary)
                    .into_iter()
                    .map(|(index, draft)| {
                        (
                            primary.resident_character_id,
                            primary.display_name.clone(),
                            index,
                            draft.spoken_text.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(visible.len(), 3);
                assert_eq!(visible[1].2, 1);
                assert_eq!(
                    primary.testimony[1].delivery,
                    TestimonyDelivery::Volunteered
                );
                visible_outputs.insert(visible);
                concern_states.insert(
                    primary
                        .testimony
                        .iter()
                        .any(|draft| draft.delivery == TestimonyDelivery::Withheld),
                );
            }
            assert_eq!(
                visible_outputs.len(),
                1,
                "observer-private concern entropy changed the full initial output"
            );
            assert_eq!(concern_states, BTreeSet::from([false, true]));
        }
    }
}

#[test]
fn private_concern_is_reliability_independent_and_pipeline_solvable() {
    use crate::investigation::process_report;

    let mut concern_reliability_states = BTreeSet::new();
    let mut checked_private_route_guard = false;
    for seed in 0..4_096 {
        let source = context(seed, TemplateFamily::RecurringDepredation);
        let generated = generate(&source).unwrap();
        let primary = &generated.witnesses[0];
        let truthful = primary.testimony[0].reliability == Reliability::Truthful;
        let concern = primary
            .testimony
            .iter()
            .position(|draft| draft.delivery == TestimonyDelivery::Withheld);
        concern_reliability_states.insert((truthful, concern.is_some()));
        if let Some(index) = concern {
            let (_, pipeline) = generated_testimony_pipeline(
                &source,
                17,
                &generated,
                primary,
                index,
                source.now_minute,
            )
            .unwrap();
            let (_, _, claim) = process_report(pipeline).unwrap();
            assert!(claim.is_some());
            if !checked_private_route_guard {
                let mut invalid = generated.clone();
                invalid.witnesses[0].testimony[index].site_id = Some(invalid.sites[0].id.clone());
                assert!(validate(&invalid).unwrap_err().iter().any(|error| {
                    error.contains("hides route authority behind a private concern")
                }));
                checked_private_route_guard = true;
            }
        }
    }
    assert!(checked_private_route_guard);
    assert_eq!(
        concern_reliability_states,
        BTreeSet::from([(false, false), (false, true), (true, false), (true, true)])
    );
}

#[test]
fn generated_physical_trails_are_opaque_contiguous_two_segment_chains() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        let generated = generate(&context(73, family)).unwrap();
        let [trail] = generated.track_trails.as_slice() else {
            panic!("generated case must have one physical trail");
        };
        assert_eq!(trail.segment_ids.len(), 2);
        let first = generated
            .track_segments
            .iter()
            .find(|segment| segment.id == trail.segment_ids[0])
            .unwrap();
        let final_segment = generated
            .track_segments
            .iter()
            .find(|segment| segment.id == trail.segment_ids[1])
            .unwrap();
        assert_eq!(first.ordinal, 0);
        assert_eq!(first.predecessor, None);
        assert_eq!(first.next.as_ref(), Some(&final_segment.id));
        assert_eq!(final_segment.ordinal, 1);
        assert_eq!(final_segment.predecessor.as_ref(), Some(&first.id));
        assert_eq!(final_segment.next, None);

        let first_action = generated
            .actions
            .iter()
            .find(|action| action.track_segment_id.as_ref() == Some(&first.id))
            .unwrap();
        let final_action = generated
            .actions
            .iter()
            .find(|action| action.track_segment_id.as_ref() == Some(&final_segment.id))
            .unwrap();
        assert_eq!(final_action.prerequisite.as_ref(), Some(&first_action.id));
        assert!(first_action.outputs.iter().any(|output| matches!(
            output,
            GeneratedActionOutput::Destination {
                stage: DestinationKnowledgeStage::RouteSegment,
                site_id: None,
            }
        )));
        assert!(!first_action.outputs.iter().any(|output| matches!(
            output,
            GeneratedActionOutput::Destination {
                stage: DestinationKnowledgeStage::ExactBelieved,
                ..
            }
        )));
        assert!(final_action.outputs.iter().any(|output| matches!(
            output,
            GeneratedActionOutput::Destination {
                stage: DestinationKnowledgeStage::ExactBelieved,
                site_id: Some(_),
            }
        )));
    }
}

#[test]
fn track_validator_rejects_broken_links_skips_and_early_exact_locations() {
    let generated = generate(&context(91, TemplateFamily::RecurringDepredation)).unwrap();

    let mut broken_link = generated.clone();
    broken_link.track_segments[0].next = None;
    assert!(validate(&broken_link).is_err());

    let mut skipped = generated.clone();
    let final_id = skipped.track_segments[1].id.clone();
    let final_action = skipped
        .actions
        .iter_mut()
        .find(|action| action.track_segment_id.as_ref() == Some(&final_id))
        .unwrap();
    final_action.prerequisite = None;
    assert!(validate(&skipped).is_err());

    let mut crossed = generated.clone();
    let area_action_id = crossed
        .actions
        .iter()
        .find(|action| {
            action.kind == InvestigationActionKind::SearchArea && action.target_kind == "area"
        })
        .unwrap()
        .id
        .clone();
    crossed
        .actions
        .iter_mut()
        .find(|action| action.track_segment_id.as_ref() == Some(&final_id))
        .unwrap()
        .prerequisite = Some(area_action_id);
    assert!(
        validate(&crossed)
            .unwrap_err()
            .iter()
            .any(|error| { error.contains("incoherent physical tracking predecessor") })
    );

    let mut leaked = generated.clone();
    let first_id = leaked.track_segments[0].id.clone();
    let true_site = leaked
        .sites
        .iter()
        .find(|site| site.is_true_location)
        .unwrap()
        .id
        .clone();
    leaked
        .actions
        .iter_mut()
        .find(|action| action.track_segment_id.as_ref() == Some(&first_id))
        .unwrap()
        .outputs
        .push(GeneratedActionOutput::Destination {
            stage: DestinationKnowledgeStage::ExactBelieved,
            site_id: Some(true_site),
        });
    assert!(validate(&leaked).is_err());
}

fn inn_only_settlement_witnesses() -> (
    Vec<crate::settlement_economy::SettlementResidentTab>,
    Vec<WitnessCandidate>,
) {
    let profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
    let tabs =
        crate::settlement_economy::player_visible_npc_tabs(&profile, false, "fixture-no-orgs");
    let mut candidates = test_witnesses();
    for (candidate, location) in candidates.iter_mut().zip(["residences", "overview", "inn"]) {
        candidate.expected_location = location.into();
        candidate.expected_location_label.clear();
    }
    let mut unavailable_armourer = candidates[2].clone();
    unavailable_armourer.resident_character_id =
        crate::settlement_population::stable_hash("test:hidden-armourer") | (1u64 << 63);
    unavailable_armourer.profession = "armourer".into();
    unavailable_armourer.expected_location = "armoury".into();
    unavailable_armourer.expected_location_label.clear();
    candidates.push(unavailable_armourer);
    (tabs, candidates)
}

fn context(seed: u64, family: TemplateFamily) -> GenerationContext {
    GenerationContext {
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
        incident_weather: crate::weather::Precipitation::Clear,
        requested_family: Some(family),
        witness_candidates: test_witnesses(),
    }
}

fn case_with_primary_location_accuracy(family: TemplateFamily, truthful: bool) -> GeneratedCase {
    (0..4_096)
            .find_map(|seed| {
                let generated = generate(&context(seed, family)).ok()?;
                let is_truthful = generated.witnesses[0].testimony[0].reliability
                    == Reliability::Truthful;
                (is_truthful == truthful).then_some(generated)
            })
            .unwrap_or_else(|| {
                panic!(
                    "bounded generation sweep did not find a {} primary location account for {family:?}",
                    if truthful { "truthful" } else { "unreliable" }
                )
            })
}

#[test]
fn witness_described_places_are_neutral_and_attributed() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        let generated = generate(&context(17, family)).unwrap();
        let primary = &generated.witnesses[0];
        let described_place = generated
            .sites
            .iter()
            .find(|site| site.role == SiteRole::Decoy)
            .unwrap();

        assert_eq!(
            described_place.safe_label,
            format!("Place {} described", primary.display_name)
        );
        let lower = described_place.safe_label.to_ascii_lowercase();
        assert!(!lower.contains("plausible"));
        assert!(!lower.contains("confirmed"));
    }
}

#[test]
fn referrals_only_use_advertised_tabs_across_families_and_many_seeds() {
    let (tabs, candidates) = inn_only_settlement_witnesses();
    let candidates = retain_navigable_witnesses(candidates, &tabs);

    assert_eq!(candidates.len(), 3);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.expected_location != "armoury"),
        "an unavailable Armour service must be a hard-zero witness location"
    );
    assert!(
        crate::settlement_economy::visible_npc_tab(&tabs, "armoury").is_none(),
        "the generated settlement fixture must not advertise Armour"
    );

    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        for seed in 0..128 {
            let mut generation_context = context(seed, family);
            generation_context.witness_candidates = candidates.clone();
            let generated = generate(&generation_context).unwrap_or_else(|error| {
                panic!("{family:?} seed {seed} failed generation: {error:?}")
            });
            for witness in &generated.witnesses {
                let tab =
                    crate::settlement_economy::visible_npc_tab(&tabs, &witness.expected_location)
                        .unwrap_or_else(|| {
                            panic!(
                                "{family:?} seed {seed} referred {} to hidden location {}",
                                witness.resident_character_id, witness.expected_location
                            )
                        });
                assert_eq!(
                    witness.expected_location_label, tab.label,
                    "{family:?} seed {seed} did not use the exact advertised tab label"
                );
                assert_eq!(referral_display_location(witness), tab.label);
            }
        }
    }
}
#[test]
fn golden_seeds_cover_both_families() {
    assert_eq!(
        generate(&context(7, TemplateFamily::RecurringDepredation))
            .unwrap()
            .family,
        TemplateFamily::RecurringDepredation
    );
    assert_eq!(
        generate(&context(7, TemplateFamily::DisappearanceOrLoss))
            .unwrap()
            .family,
        TemplateFamily::DisappearanceOrLoss
    );
}

#[test]
fn referred_witness_pipeline_fits_every_stable_id_budget_in_both_families() {
    use crate::investigation::{ValidationError, compound_id, process_report};
    for (seed, family) in [
        (7, TemplateFamily::RecurringDepredation),
        (11, TemplateFamily::DisappearanceOrLoss),
    ] {
        let mut context = context(seed, family);
        for (index, witness) in context.witness_candidates.iter_mut().enumerate() {
            witness.resident_character_id = 9_007_199_254_740_993 + seed * 16 + index as u64;
        }
        let generated = generate(&context).unwrap();
        validate(&generated).unwrap();
        let character_id = 17_849_106_825_763_413_937;
        let mut claim_count = 0;
        for witness in &generated.witnesses {
            for index in 0..witness.testimony.len() {
                let (receipt_id, pipeline) = generated_testimony_pipeline(
                    &context,
                    character_id,
                    &generated,
                    witness,
                    index,
                    50_000,
                )
                .unwrap();
                assert!(receipt_id.starts_with("testimony:"));
                let (observation, recollection, claim) = process_report(pipeline.clone()).unwrap();
                let claim = claim.unwrap();
                for id in [
                    observation.id.as_str(),
                    recollection.id.as_str(),
                    claim.id.as_str(),
                ] {
                    assert!(id.len() <= 256);
                }
                claim_count += 1;
                if claim_count == 1 {
                    let mut invalid = pipeline;
                    let oversized_generic_component = "x".repeat(257);
                    invalid.receipt_identity =
                        compound_id(&["generated-testimony", &oversized_generic_component]);
                    assert_eq!(
                        process_report(invalid).unwrap_err(),
                        ValidationError::InvalidId
                    );
                }
            }
        }
        assert!(claim_count >= generated.witnesses.len());
    }
}

#[test]
fn incident_weather_changes_perception_without_changing_reliability_stages() {
    use crate::investigation::PerceptionCondition;
    let clear_context = context(7, TemplateFamily::RecurringDepredation);
    let generated = generate(&clear_context).unwrap();
    let witness = &generated.witnesses[0];
    let (_, clear) =
        generated_testimony_pipeline(&clear_context, 1, &generated, witness, 0, 50_000).unwrap();
    let mut rainy_context = clear_context.clone();
    rainy_context.incident_weather = crate::weather::Precipitation::Rain;
    let (_, rainy) =
        generated_testimony_pipeline(&rainy_context, 1, &generated, witness, 0, 50_000).unwrap();
    assert_eq!(rainy.perception, PerceptionCondition::PoorPerception);
    assert_eq!(rainy.memory, clear.memory);
    assert_eq!(rainy.disclosure, clear.disclosure);
    assert_eq!(rainy.transmitted_text, clear.transmitted_text);
}

#[test]
fn secondary_witnesses_require_explicit_acyclic_referral_edges() {
    let generated = (0..4_096)
        .find_map(|seed| {
            let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).ok()?;
            let secondary = generated.witnesses.get(1)?;
            (!secondary.testimony.is_empty()
                && generated.witnesses[0]
                    .testimony
                    .iter()
                    .any(|draft| draft.referred_witness_ids.contains(&secondary.id)))
            .then_some(generated)
        })
        .expect("bounded seeds include an explicit secondary-witness referral");
    let secondary_id = generated.witnesses[1].id.clone();
    assert!(
        generated.witnesses[0]
            .testimony
            .iter()
            .any(|draft| draft.referred_witness_ids.contains(&secondary_id))
    );

    let mut unreachable = generated.clone();
    for draft in &mut unreachable.witnesses[0].testimony {
        draft.referred_witness_ids.clear();
    }
    assert!(
        validate(&unreachable)
            .unwrap_err()
            .iter()
            .any(|error| { error.contains("not reachable from the primary witness") })
    );

    let mut cyclic = generated;
    let primary_id = cyclic.witnesses[0].id.clone();
    cyclic.witnesses[1].testimony[0]
        .referred_witness_ids
        .push(primary_id);
    assert!(
        validate(&cyclic)
            .unwrap_err()
            .iter()
            .any(|error| { error.contains("cyclic or backward witness referral") })
    );
}

#[test]
fn challenge_boundaries_and_optional_authored_responses_validate() {
    let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let first = &generated.witnesses[0].testimony[0];
    assert_eq!(
        first
            .spoken_text
            .match_indices(&first.challenge_text)
            .count(),
        1
    );

    generated.witnesses[0].testimony[0].challenge_responses = TestimonyChallengeResponses {
        charm: None,
        command: None,
        bluff: None,
    };
    validate(&generated).unwrap();

    let duplicate = generated.witnesses[0].testimony[1]
        .challenge_responses
        .charm
        .clone()
        .expect("generated response");
    generated.witnesses[0].testimony[2]
        .challenge_responses
        .bluff = Some(duplicate);
    assert!(
        validate(&generated)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("reuses authored challenge response text"))
    );
}

#[test]
fn challenge_validation_rejects_padded_challenge_text() {
    let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    generated.witnesses[0].testimony[0].challenge_text =
        format!(" {} ", generated.witnesses[0].testimony[0].challenge_text);
    assert!(
        validate(&generated)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("challenge text must be nonempty and already trimmed"))
    );
}

#[test]
fn challenge_validation_rejects_response_containing_normalized_claim_text() {
    let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let normalized_claim = generated.witnesses[0].testimony[0]
        .challenge_text
        .to_uppercase();
    generated.witnesses[0].testimony[0]
        .challenge_responses
        .charm = Some(format!("Press further about {normalized_claim}"));
    assert!(
        validate(&generated)
            .unwrap_err()
            .iter()
            .any(|error| error.contains("challenge response repeats its claim text"))
    );
}

#[test]
fn generated_claim_boundaries_exclude_narration_and_punctuation() {
    let generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let primary = &generated.witnesses[0].testimony;
    assert_eq!(
        primary[1].challenge_text,
        "I cannot tell which details matter"
    );
    assert!(primary[2].spoken_text.starts_with("I noticed "));
    assert!(!primary[2].challenge_text.starts_with("I noticed "));
    assert!(
        primary[2]
            .spoken_text
            .ends_with(". It may be examined firsthand.")
    );
    assert!(!primary[2].challenge_text.ends_with('.'));

    let visual = (0..1_000)
        .find_map(|seed| {
            let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).ok()?;
            generated.witnesses[0]
                .testimony
                .iter()
                .find(|draft| draft.spoken_text.starts_with("Methought it looked like "))
                .cloned()
        })
        .expect("golden range includes a visual claim");
    assert!(
        !visual
            .challenge_text
            .starts_with("Methought it looked like ")
    );
    assert!(!visual.challenge_text.ends_with('.'));
}
