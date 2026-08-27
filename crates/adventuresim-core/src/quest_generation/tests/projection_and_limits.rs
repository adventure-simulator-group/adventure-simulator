#[test]
fn observer_text_avoids_generator_authority_and_internal_demographics() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        for seed in 0..512 {
            let case = generate(&context(seed, family)).unwrap();
            for testimony in case.witnesses.iter().flat_map(|witness| &witness.testimony) {
                let text = testimony.spoken_text.to_ascii_lowercase();
                assert!(!text.contains("true site"), "{text}");
                assert!(!text.contains("(adult, unspecified"), "{text}");
                assert!(!text.contains("laborer people"), "{text}");
            }
            for action in &case.actions {
                let text = action.safe_summary.to_ascii_lowercase();
                assert!(!text.contains("true site"), "{text}");
                assert!(!text.contains("unspecified"), "{text}");
                assert!(!text.contains("laborer people"), "{text}");
            }
        }
    }
}

#[test]
fn every_pattern_becomes_an_earned_observer_clue_and_executable_condition() {
    let expected = [
        (
            AttackPattern::Nightly,
            "Nightly",
            "nighttime",
            GeneratedPatternCondition::NightWindow,
        ),
        (
            AttackPattern::Roadside,
            "Roadside",
            "roadside",
            GeneratedPatternCondition::RoadRoute,
        ),
        (
            AttackPattern::VictimSpecific,
            "VictimSpecific",
            "victims",
            GeneratedPatternCondition::VictimProfile {
                cohort_id: String::new(),
                demographic: WitnessDemographic::Merchant,
                age_band: String::new(),
                sex: String::new(),
                profession: String::new(),
            },
        ),
        (
            AttackPattern::Irregular,
            "Irregular",
            "no reliable schedule",
            GeneratedPatternCondition::BroadSurvey,
        ),
    ];
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        let mut prelearning_blueprints = BTreeSet::new();
        for (pattern, event_prefix, summary_fragment, condition_shape) in expected.clone() {
            let case = (0..4_096)
                .map(|seed| generate(&context(seed, family)).unwrap())
                .find(|case| case.canonical_events[0].object.starts_with(event_prefix))
                .expect("pattern must be reachable");
            let pattern_evidence = case
                .evidence
                .iter()
                .find(|evidence| {
                    evidence
                        .safe_description
                        .starts_with("Corroborated accounts")
                })
                .expect("observer-safe pattern evidence");
            assert!(
                case.witnesses[0]
                    .testimony
                    .iter()
                    .any(|draft| draft.proposition_id == pattern_evidence.proposition_id)
            );
            let producer = case
                .actions
                .iter()
                .find(|action| {
                    action.outputs.iter().any(|output| {
                        matches!(
                            output,
                            GeneratedActionOutput::Evidence { evidence_id }
                                if evidence_id == &pattern_evidence.id
                        )
                    })
                })
                .expect("generic action earns the clue");
            match family {
                TemplateFamily::RecurringDepredation => {
                    let contact = case
                        .actions
                        .iter()
                        .find(|action| action.kind == InvestigationActionKind::LocateContact)
                        .expect("recurring case contact entry");
                    assert!(!producer.active_initially);
                    assert_eq!(producer.prerequisite.as_ref(), Some(&contact.id));
                }
                TemplateFamily::DisappearanceOrLoss => {
                    assert!(producer.active_initially);
                }
                TemplateFamily::Outbreak => unreachable!("outbreak is outside this test matrix"),
            }
            prelearning_blueprints.insert(format!(
                "{:?}:{}:{}",
                producer.kind, producer.target_kind, producer.safe_summary
            ));
            let consumer = case
                .actions
                .iter()
                .find(|action| {
                    action.outputs.iter().any(|output| {
                        matches!(
                            output,
                            GeneratedActionOutput::PatternCondition { evidence_id, .. }
                                if evidence_id == &pattern_evidence.id
                        )
                    })
                })
                .expect("successor consumes the clue");
            assert_eq!(consumer.prerequisite.as_ref(), Some(&producer.id));
            assert!(!consumer.active_initially);
            assert!(
                consumer.safe_summary.contains(summary_fragment),
                "{family:?} {pattern:?}: expected {summary_fragment:?} in {:?}",
                consumer.safe_summary
            );
            let selected_condition = consumer
                .outputs
                .iter()
                .find_map(|output| match output {
                    GeneratedActionOutput::PatternCondition { condition, .. } => Some(condition),
                    _ => None,
                })
                .unwrap();
            match (&condition_shape, selected_condition) {
                (
                    GeneratedPatternCondition::VictimProfile { .. },
                    GeneratedPatternCondition::VictimProfile {
                        cohort_id,
                        demographic,
                        age_band,
                        sex,
                        profession,
                    },
                ) => {
                    let target = case
                        .pattern_targets
                        .iter()
                        .find(|target| target.cohort_id == *cohort_id)
                        .unwrap();
                    assert_eq!(*demographic, target.demographic);
                    assert_eq!(age_band, &target.age_band);
                    assert_eq!(sex, &target.sex);
                    assert_eq!(profession, &target.profession);
                    assert_eq!(consumer.target_kind, "cohort");
                    assert_eq!(consumer.target_id, target.cohort_id);
                }
                (expected, actual) => assert_eq!(expected, actual),
            }
            let learned_projection = serde_json::to_string(&(pattern_evidence, consumer)).unwrap();
            assert!(learned_projection.contains(summary_fragment));
            assert!(!learned_projection.contains("\"resident_character_id\""));
            for target in &case.pattern_targets {
                assert!(
                    !consumer
                        .safe_summary
                        .contains(&target.resident_character_id.to_string())
                );
            }
            match pattern {
                AttackPattern::Roadside => assert_eq!(consumer.target_kind, "route"),
                AttackPattern::Irregular => {
                    assert_eq!(consumer.kind, InvestigationActionKind::SearchArea)
                }
                _ => assert_eq!(consumer.kind, InvestigationActionKind::Patrol),
            }
        }
        assert_eq!(
            prelearning_blueprints.len(),
            1,
            "the initially visible action must not reveal the selected pattern"
        );
    }
}

#[test]
fn victim_cohort_binding_accepts_exact_authority_and_rejects_drift() {
    for family in [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ] {
        let (source, case) = (0..4_096)
            .map(|seed| {
                let source = context(seed, family);
                let case = generate(&source).unwrap();
                (source, case)
            })
            .find(|(_, case)| {
                case.canonical_events[0]
                    .object
                    .starts_with("VictimSpecific")
            })
            .expect("victim-specific pattern must be reachable");
        let target = case.pattern_targets.first().unwrap();
        let current = source
            .witness_candidates
            .iter()
            .find(|candidate| candidate.resident_character_id == target.resident_character_id)
            .unwrap();
        assert!(pattern_target_matches(
            target,
            current,
            &source.settlement_id
        ));
        assert_eq!(
            target.expected_location_label,
            current.expected_location_label
        );
        assert!(
            case.witnesses
                .iter()
                .flat_map(|witness| &witness.testimony)
                .any(|draft| draft
                    .truthful_text
                    .contains(&target.expected_location_label))
        );
        if target.expected_location != target.expected_location_label {
            assert!(
                case.witnesses
                    .iter()
                    .flat_map(|witness| &witness.testimony)
                    .all(|draft| !draft
                        .truthful_text
                        .contains(&format!("near {}.", target.expected_location)))
            );
        }
        let mut wrong_demographic = current.clone();
        wrong_demographic.demographic = if current.demographic == WitnessDemographic::Child {
            WitnessDemographic::Guard
        } else {
            WitnessDemographic::Child
        };
        assert!(!pattern_target_matches(
            target,
            &wrong_demographic,
            &source.settlement_id
        ));
        let mut moved = current.clone();
        moved.expected_location.push_str("-moved");
        assert!(!pattern_target_matches(
            target,
            &moved,
            &source.settlement_id
        ));
        let mut stale = current.clone();
        stale.presence_version ^= 1;
        assert!(!pattern_target_matches(
            target,
            &stale,
            &source.settlement_id
        ));
    }
}

#[test]
fn visible_developer_witnesses_preserve_all_presentations_and_pattern_targets() {
    let candidates = ["Man", "Woman", "Ambiguous"]
        .into_iter()
        .enumerate()
        .map(|(index, presentation)| {
            let resident_character_id = (1u64 << 53) + index as u64 + 1;
            let name = format!("Visible Witness {index}");
            let mut candidate = visible_witness_candidate(VisibleWitnessCandidateInput {
                resident_character_id,
                display_name: &name,
                age_band: "Adult",
                presentation,
                height: "average height",
                build: "sturdy",
                hair: "brown hair",
                clothing: "a wool coat",
                profession: "laborer",
                local_role: "resident",
                settlement_id: "riverdale",
                location_id: "market",
                start_minute: 480,
                end_minute: 1_020,
                is_default: true,
            })
            .unwrap();
            candidate.expected_location_label = "General Market".into();
            candidate
        })
        .collect::<Vec<_>>();
    assert!(candidates.iter().all(|candidate| candidate.sex.is_empty()));
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.demographic)
            .collect::<BTreeSet<_>>()
            .len(),
        1,
        "visible presentation must not alter demographic selection"
    );
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.presence_version)
            .collect::<BTreeSet<_>>()
            .len(),
        3,
        "presentation remains part of the visible commitment"
    );

    let (source, generated) = (0..4_096)
        .map(|seed| {
            let mut source = context(seed, TemplateFamily::RecurringDepredation);
            source.witness_candidates = candidates.clone();
            let generated = generate(&source).unwrap();
            (source, generated)
        })
        .find(|(_, generated)| !generated.pattern_targets.is_empty())
        .expect("visible candidates must support a victim-specific pattern");
    let definition = crate::developer_quest::DeveloperQuestDefinition::from_generated(generated);
    let compiled =
        crate::developer_quest::compile(&crate::developer_quest::DeveloperGenerationContext {
            base: source.clone(),
            definition,
            allow_implausible: true,
        })
        .unwrap();
    let target = compiled.pattern_targets.first().unwrap();
    let current = source
        .witness_candidates
        .iter()
        .find(|candidate| candidate.resident_character_id == target.resident_character_id)
        .unwrap();
    assert!(pattern_target_matches(
        target,
        current,
        &source.settlement_id
    ));
    assert!(target.sex.is_empty());
}

#[test]
fn victim_specific_is_hard_zero_without_an_unused_persistent_target() {
    let candidates = attack_pattern_candidates(TemplateFamily::RecurringDepredation, false);
    let victim = candidates
        .iter()
        .find(|candidate| candidate.value == AttackPattern::VictimSpecific)
        .unwrap();
    assert_eq!(victim.weight.plausibility, 0);
    assert!(victim.impossible.is_some());
    for seed in 0..256 {
        let mut source = context(seed, TemplateFamily::RecurringDepredation);
        source.witness_candidates.truncate(2);
        let case = generate(&source).unwrap();
        assert!(
            !case.canonical_events[0]
                .object
                .starts_with("VictimSpecific")
        );
        assert!(case.pattern_targets.is_empty());
    }
}

#[test]
fn oversized_candidate_domains_fail_before_ordering_or_tracing() {
    let candidates = vec![
        Candidate {
            id: "oversized",
            value: 1_u8,
            weight: Weight::new(1, 1),
            bridge: None,
            impossible: None,
            factors: vec![],
        };
        MAX_SOLVER_CANDIDATES + 1
    ];
    assert_eq!(
        weighted_order(1, "oversized", &candidates),
        Err(GenerationError::CandidateLimit)
    );
    let mut oversized = context(1, TemplateFamily::RecurringDepredation);
    oversized.witness_candidates = vec![test_witnesses()[0].clone(); MAX_SOLVER_CANDIDATES + 1];
    assert_eq!(generate(&oversized), Err(GenerationError::CandidateLimit));
    let mut oversized_bytes = context(1, TemplateFamily::RecurringDepredation);
    oversized_bytes.witness_candidates[0].visible_description = "x".repeat(65 * 1024);
    assert_eq!(
        generate(&oversized_bytes),
        Err(GenerationError::CandidateLimit)
    );
}
