#[test]
fn exact_referred_witness_projects_clues_and_completes_contact_root_idempotently() {
    use crate::investigation::process_report;

    let mut source = context(11, TemplateFamily::DisappearanceOrLoss);
    for (index, witness) in source.witness_candidates.iter_mut().enumerate() {
        witness.resident_character_id = 9_007_199_254_740_993 + index as u64;
    }
    let generated = generate(&source).expect("generated disappearance case");
    let witness = generated.witnesses.first().expect("exact referred witness");
    let same_name_other_id = 9_007_199_254_741_993;
    let witness_name = "Hans Wagner";
    let other_name = "Hans Wagner";
    assert_eq!(witness_name, other_name);
    assert!(exact_referral_contact(
        witness.resident_character_id,
        witness.resident_character_id
    ));
    assert!(!exact_referral_contact(
        witness.resident_character_id,
        same_name_other_id
    ));

    let projection_plan =
        generated_testimony_projection_plan(witness).expect("public testimony projections");
    let character_id = 17_849_106_825_763_413_937;
    let mut public_testimony = Vec::new();
    let mut public_leads = Vec::new();
    for (index, draft) in projection_plan.iter().enumerate() {
        let (_, pipeline) = generated_testimony_pipeline(
            &source,
            character_id,
            &generated,
            witness,
            index,
            source.now_minute,
        )
        .expect("private testimony pipeline");
        let (_, _, claim) = process_report(pipeline).expect("production claim processing");
        let claim = claim.expect("generated testimony is disclosed");
        public_testimony.push((claim.proposition_id.as_str().to_string(), claim.statement));
        public_leads.push((draft.proposition_id.clone(), draft.spoken_text.clone()));
    }
    assert_eq!(public_testimony.len(), witness.testimony.len());
    assert_eq!(public_leads.len(), witness.testimony.len());
    assert!(
        public_testimony
            .iter()
            .all(|(_, statement)| !statement.is_empty())
    );
    assert!(public_leads.iter().all(|(_, summary)| !summary.is_empty()));

    let method = |kind| match kind {
        InvestigationActionKind::InspectSite => "inspect_site",
        InvestigationActionKind::SearchArea => "search_area",
        InvestigationActionKind::FollowTracks => "follow_tracks",
        InvestigationActionKind::ReacquireTracks => "reacquire_tracks",
        InvestigationActionKind::LocateContact => "locate_contact",
        InvestigationActionKind::Watch => "watch",
        InvestigationActionKind::Patrol => "patrol",
        InvestigationActionKind::LayAmbush => "lay_ambush",
        InvestigationActionKind::ApproachLead => "approach_lead",
    };
    let mut states: Vec<_> = generated
        .actions
        .iter()
        .map(|action| ReferredContactActionState {
            id: action.id.0.clone(),
            owner_character_id: character_id,
            case_id: generated.canonical_case_id.clone(),
            method: method(action.kind).into(),
            target_kind: action.target_kind,
            target_id: action.target_id.clone(),
            required_action_id: action
                .prerequisite
                .as_ref()
                .map_or_else(String::new, |id| id.0.clone()),
            active: action.active_initially,
            version: 0,
            successful_attempt: false,
        })
        .collect();
    let applied = transition_referred_contact_action(
        &mut states,
        character_id,
        &generated.canonical_case_id,
        witness.resident_character_id,
    )
    .expect("exact witness transition");
    let ReferredContactTransition::Applied {
        root_id,
        expected_version,
        next_version,
        activated_successor_ids,
        attempt_success,
        outcome_wording,
    } = applied
    else {
        panic!("exact witness should complete the active contact root");
    };
    assert_eq!(expected_version, 0);
    assert_eq!(next_version, 1);
    assert!(attempt_success);
    assert!(!outcome_wording.is_empty());
    let root = states.iter().find(|state| state.id == root_id).unwrap();
    assert!(!root.active);
    assert!(root.successful_attempt);
    assert_eq!(root.version, 1);
    assert!(!activated_successor_ids.is_empty());
    assert!(activated_successor_ids.iter().all(|id| {
        states
            .iter()
            .find(|state| state.id == *id)
            .is_some_and(|state| state.active && state.required_action_id == root_id)
    }));

    assert_eq!(
        transition_referred_contact_action(
            &mut states,
            character_id,
            &generated.canonical_case_id,
            witness.resident_character_id,
        )
        .expect("idempotent replay"),
        ReferredContactTransition::Replay
    );
    let root_after_replay = states.iter().find(|state| state.id == root_id).unwrap();
    assert_eq!(root_after_replay.version, 1);
}

#[test]
fn secondary_testimony_without_a_contact_root_mutates_no_route() {
    let primary_witness = 9_007_199_254_740_993;
    let secondary_witness = 9_007_199_254_740_994;
    let mut states = vec![ReferredContactActionState {
        id: "primary-contact".into(),
        owner_character_id: 7,
        case_id: "case".into(),
        method: "locate_contact".into(),
        target_kind: InvestigationTargetKind::Contact,
        target_id: primary_witness.to_string(),
        required_action_id: String::new(),
        active: true,
        version: 0,
        successful_attempt: false,
    }];
    let before = states.clone();
    assert_eq!(
        transition_referred_contact_action(&mut states, 7, "case", secondary_witness).unwrap(),
        ReferredContactTransition::NotApplicable
    );
    assert_eq!(states, before);

    states.push(ReferredContactActionState {
        id: "duplicate-primary-contact".into(),
        ..states[0].clone()
    });
    assert_eq!(
        transition_referred_contact_action(&mut states, 7, "case", primary_witness).unwrap_err(),
        "Referred witness matches multiple contact actions"
    );
}

#[test]
fn terminal_referred_contact_completes_without_authored_successors() {
    let mut states = vec![ReferredContactActionState {
        id: "terminal-contact".into(),
        owner_character_id: 7,
        case_id: "case".into(),
        method: "locate_contact".into(),
        target_kind: InvestigationTargetKind::Contact,
        target_id: "42".into(),
        required_action_id: String::new(),
        active: true,
        version: 0,
        successful_attempt: false,
    }];
    let transition = transition_referred_contact_action(&mut states, 7, "case", 42).unwrap();
    let ReferredContactTransition::Applied {
        activated_successor_ids,
        attempt_success,
        ..
    } = transition
    else {
        panic!("terminal contact should complete its authored root")
    };
    assert!(activated_successor_ids.is_empty());
    assert!(attempt_success);
    assert!(!states[0].active);
    assert!(states[0].successful_attempt);
    assert_eq!(states[0].version, 1);
}

#[test]
fn failed_route_does_not_revive_a_completed_contact_alternate() {
    let mut states = vec![
        ReferredContactActionState {
            id: "search".into(),
            owner_character_id: 7,
            case_id: "case".into(),
            method: "search_area".into(),
            target_kind: InvestigationTargetKind::Area,
            target_id: "area".into(),
            required_action_id: String::new(),
            active: true,
            version: 1,
            successful_attempt: false,
        },
        ReferredContactActionState {
            id: "contact".into(),
            owner_character_id: 7,
            case_id: "case".into(),
            method: "locate_contact".into(),
            target_kind: InvestigationTargetKind::Contact,
            target_id: "witness".into(),
            required_action_id: String::new(),
            active: false,
            version: 1,
            successful_attempt: true,
        },
    ];

    assert_eq!(
        transition_failed_action_alternate(&mut states, 7, "case", "contact").unwrap(),
        FailedActionAlternateTransition::Unavailable
    );
    assert!(!states[1].active);
    assert_eq!(
        failed_action_outcome_wording(false),
        "No conclusive result. Time passed, and no alternate route is currently supported by the leads in your journal."
    );
}

#[test]
fn recurring_routes_unlock_only_after_exact_referred_contact() {
    let method = |kind| match kind {
        InvestigationActionKind::InspectSite => "inspect_site",
        InvestigationActionKind::SearchArea => "search_area",
        InvestigationActionKind::FollowTracks => "follow_tracks",
        InvestigationActionKind::ReacquireTracks => "reacquire_tracks",
        InvestigationActionKind::LocateContact => "locate_contact",
        InvestigationActionKind::Watch => "watch",
        InvestigationActionKind::Patrol => "patrol",
        InvestigationActionKind::LayAmbush => "lay_ambush",
        InvestigationActionKind::ApproachLead => "approach_lead",
    };
    for seed in [0, 7, 41, 255] {
        let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
        let contact = generated
            .actions
            .iter()
            .find(|action| action.kind == InvestigationActionKind::LocateContact)
            .unwrap();
        let approach = generated
            .actions
            .iter()
            .find(|action| action.kind == InvestigationActionKind::ApproachLead)
            .unwrap();
        let watch = generated
            .actions
            .iter()
            .find(|action| action.kind == InvestigationActionKind::Watch)
            .unwrap();
        assert!(contact.active_initially);
        for successor in [approach, watch] {
            assert!(!successor.active_initially);
            assert_eq!(successor.prerequisite.as_ref(), Some(&contact.id));
        }

        let mut states = generated
            .actions
            .iter()
            .map(|action| ReferredContactActionState {
                id: action.id.0.clone(),
                owner_character_id: 7,
                case_id: generated.canonical_case_id.clone(),
                method: method(action.kind).into(),
                target_kind: action.target_kind,
                target_id: action.target_id.clone(),
                required_action_id: action
                    .prerequisite
                    .as_ref()
                    .map_or_else(String::new, |id| id.0.clone()),
                active: action.active_initially,
                version: 0,
                successful_attempt: false,
            })
            .collect::<Vec<_>>();
        for successor in [approach, watch] {
            let state = states
                .iter()
                .find(|state| state.id == successor.id.0)
                .unwrap();
            assert!(!state.active);
            assert!(!states.iter().any(|candidate| {
                candidate.id == state.required_action_id && candidate.successful_attempt
            }));
        }
        assert!(matches!(
            transition_referred_contact_action(
                &mut states,
                7,
                &generated.canonical_case_id,
                generated.witnesses[0].resident_character_id,
            )
            .unwrap(),
            ReferredContactTransition::Applied { .. }
        ));
        for successor in [approach, watch] {
            let state = states
                .iter()
                .find(|state| state.id == successor.id.0)
                .unwrap();
            assert!(state.active);
            assert!(states.iter().any(|candidate| {
                candidate.id == state.required_action_id && candidate.successful_attempt
            }));
        }
    }
}

#[test]
fn physical_evidence_has_deterministic_inspection_topics_and_hidden_difficulty() {
    assert!(!evidence_check_passes(2_499, 2_500));
    assert!(evidence_check_passes(2_500, 2_500));
    assert!(evidence_check_passes(4_000, 2_500));
    let first = generate(&context(41, TemplateFamily::RecurringDepredation)).unwrap();
    let replay = generate(&context(41, TemplateFamily::RecurringDepredation)).unwrap();
    assert_eq!(first.evidence, replay.evidence);
    assert!(first.evidence.iter().all(|evidence| {
        !evidence.portrait_label.is_empty()
            && !evidence.portrait_icon.is_empty()
            && !evidence.base_description.is_empty()
            && evidence.inspection_topics.len() >= 2
            && evidence
                .inspection_topics
                .iter()
                .any(|topic| topic.check.is_none())
            && evidence.inspection_topics.iter().any(|topic| {
                topic.check.as_ref().is_some_and(|check| {
                    (100..=5_500).contains(&check.difficulty_milli) && check.reveals_clue
                })
            })
    }));
}

#[test]
fn pawprint_bestiary_implications_are_atomic_and_do_not_reveal_ancestry() {
    let (_, _, _, topics) = evidence_presentation(
        EvidenceKind::Footprints,
        &EvidenceId("test-pawprint".into()),
        50,
    );
    let implications = &topics
        .iter()
        .find(|topic| topic.id == "edges")
        .unwrap()
        .bestiary;
    assert!(implications.iter().all(|implication| {
        matches!(
            implication.category,
            BestiaryCategory::Beast
                | BestiaryCategory::Werekin
                | BestiaryCategory::Spirit
                | BestiaryCategory::Undead
        )
    }));
    assert!(!implications.iter().any(|implication| {
        matches!(
            implication.category,
            BestiaryCategory::Human | BestiaryCategory::Elf | BestiaryCategory::Dwarf
        )
    }));
    assert!(implications.iter().all(|implication| {
        implication
            .diagnostic_kind
            .as_deref()
            .is_some_and(|kind| !kind.is_empty())
    }));
}

#[test]
fn arbitrary_single_root_graph_does_not_satisfy_entry_invariant() {
    let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let root = generated
        .actions
        .iter_mut()
        .find(|action| action.active_initially)
        .unwrap();
    root.kind = InvestigationActionKind::Watch;
    assert!(validate(&generated).is_err());

    let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let root_id = generated
        .actions
        .iter()
        .find(|action| action.active_initially)
        .unwrap()
        .id
        .clone();
    let watch = generated
        .actions
        .iter_mut()
        .find(|action| {
            action.kind == InvestigationActionKind::Watch
                && action.prerequisite.as_ref() == Some(&root_id)
        })
        .unwrap();
    watch.prerequisite = None;
    assert!(validate(&generated).is_err());
}

#[test]
fn family_entry_validation_rejects_kind_route_target_and_prerequisite_substitutions() {
    for mutate in 0..4 {
        let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        let root_id = generated
            .actions
            .iter()
            .find(|action| action.active_initially)
            .unwrap()
            .id
            .clone();
        let root = generated
            .actions
            .iter_mut()
            .find(|action| action.id == root_id)
            .unwrap();
        match mutate {
            0 => root.kind = InvestigationActionKind::Watch,
            1 => root.route = RouteClass::PhysicalTrail,
            2 => root.target_kind = InvestigationTargetKind::Area,
            _ => root.prerequisite = Some(ActionId::new("substituted")),
        }
        assert!(
            validate(&generated).is_err(),
            "recurring root mutation {mutate} unexpectedly remained valid"
        );
    }
    for mutate in 0..4 {
        let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        let root_id = generated
            .actions
            .iter()
            .find(|action| action.active_initially)
            .unwrap()
            .id
            .clone();
        let successor = generated
            .actions
            .iter_mut()
            .find(|action| {
                action.kind == InvestigationActionKind::ApproachLead
                    && action.prerequisite.as_ref() == Some(&root_id)
            })
            .unwrap();
        match mutate {
            0 => successor.kind = InvestigationActionKind::SearchArea,
            1 => successor.route = RouteClass::PatternSurveillance,
            2 => successor.target_kind = InvestigationTargetKind::Contact,
            _ => successor.prerequisite = Some(ActionId::new("substituted")),
        }
        assert!(
            validate(&generated).is_err(),
            "recurring successor mutation {mutate} unexpectedly remained valid"
        );
    }
    for mutate in 0..4 {
        let mut generated = generate(&context(11, TemplateFamily::DisappearanceOrLoss)).unwrap();
        let physical = generated
            .actions
            .iter_mut()
            .find(|action| action.active_initially && action.route == RouteClass::PhysicalTrail)
            .unwrap();
        match mutate {
            0 => physical.kind = InvestigationActionKind::LocateContact,
            1 => physical.route = RouteClass::SocialInquiry,
            2 => physical.target_kind = InvestigationTargetKind::Contact,
            _ => physical.prerequisite = Some(ActionId::new("substituted")),
        }
        assert!(
            validate(&generated).is_err(),
            "disappearance root mutation {mutate} unexpectedly remained valid"
        );
    }
}

#[test]
fn action_graph_validation_rejects_missing_stranded_and_unreachable_exact_routes() {
    let mut missing = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    missing
        .actions
        .iter_mut()
        .find(|action| {
            action.route == RouteClass::PhysicalTrail
                && action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Destination {
                            stage: DestinationKnowledgeStage::ExactBelieved,
                            ..
                        }
                    )
                })
        })
        .unwrap()
        .prerequisite = Some(ActionId::new("missing-prerequisite"));
    assert!(validate(&missing).is_err());

    let mut missing_alternate =
        generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    missing_alternate.actions[0].alternate = ActionId::new("missing-alternate");
    assert!(validate(&missing_alternate).is_err());

    let mut stranded = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let search_id = stranded
        .actions
        .iter()
        .find(|action| action.kind == InvestigationActionKind::SearchArea)
        .unwrap()
        .id
        .clone();
    let follow_id = stranded
        .actions
        .iter()
        .find(|action| {
            action.route == RouteClass::PhysicalTrail
                && action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Destination {
                            stage: DestinationKnowledgeStage::ExactBelieved,
                            ..
                        }
                    )
                })
        })
        .unwrap()
        .id
        .clone();
    stranded
        .actions
        .iter_mut()
        .find(|action| action.id == search_id)
        .unwrap()
        .prerequisite = Some(follow_id);
    assert!(validate(&stranded).is_err());

    let mut exact = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
    let physical_resolution_id = exact
        .actions
        .iter()
        .find(|action| {
            action.route == RouteClass::PhysicalTrail
                && action.kind == InvestigationActionKind::InspectSite
        })
        .unwrap()
        .id
        .clone();
    exact
        .actions
        .iter_mut()
        .find(|action| {
            action.route == RouteClass::PhysicalTrail
                && action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Destination {
                            stage: DestinationKnowledgeStage::ExactBelieved,
                            ..
                        }
                    )
                })
        })
        .unwrap()
        .prerequisite = Some(physical_resolution_id);
    assert!(
        validate(&exact).is_err(),
        "a physical exact-site producer stranded in a route-local cycle remained valid"
    );
}
