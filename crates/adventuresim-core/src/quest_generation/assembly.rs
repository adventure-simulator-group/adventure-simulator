pub fn generate(context: &GenerationContext) -> Result<GeneratedCase, GenerationError> {
    if context.requested_family == Some(TemplateFamily::Outbreak)
        || (context.requested_family.is_none() && context.seed % 7 == 0)
    {
        return generate_outbreak(context);
    }
    let mut trace = Vec::new();
    let solved = solve_variables(context, &mut trace)?;
    let SolvedVariables {
        family,
        cause,
        site,
        demographic,
        circumstance,
        description,
        family_bridge,
        cause_bridge,
        site_bridge,
        circumstance_bridge: circ_bridge,
        description_bridge,
        primary_witness,
        secondary_witness,
    } = solved;
    let primary = &context.witness_candidates[primary_witness];
    let secondary = &context.witness_candidates[secondary_witness];
    let prefix = observer_scope(context);
    let investigability = match cause {
        CanonicalCause::Hostile(threat) => {
            crate::bestiary::profile(threat)
                .investigation
                .investigability
        }
        _ => 50,
    };
    let (reliability, reliability_bridge) = choose(
        context.seed.rotate_left(5),
        "module.reliability",
        "relation.reliability.context",
        &reliability_candidates(demographic, circumstance, cause),
        &mut trace,
    )?;
    let (secondary_site_kind, secondary_site_bridge) = choose(
        context.seed.rotate_left(11),
        "module.secondary_site",
        "relation.site.cause",
        &secondary_site_candidates(cause, site),
        &mut trace,
    )?;
    let (secondary_circumstance, secondary_circumstance_bridge) = choose(
        context.seed.rotate_left(13),
        "module.secondary_circumstance",
        "relation.circumstance.npc_fact",
        &secondary_circumstance_candidates(secondary, circumstance),
        &mut trace,
    )?;
    let (evidence_kind, evidence_bridge) = choose(
        context.seed.rotate_left(17),
        "module.evidence",
        "relation.evidence.cause_site",
        &evidence_candidates(cause, site),
        &mut trace,
    )?;
    let (account_style, account_bridge) = choose(
        context.seed.rotate_left(23),
        "module.account",
        "relation.account.reliability_circumstance",
        &account_style_candidates(reliability, circumstance),
        &mut trace,
    )?;
    let (route_variant, route_bridge) = choose(
        context.seed.rotate_left(31),
        "module.route",
        "relation.route.family",
        &route_variant_candidates(family),
        &mut trace,
    )?;
    let mut victim_target_candidates = (0..context.witness_candidates.len())
        .filter(|index| *index != primary_witness && *index != secondary_witness)
        .collect::<Vec<_>>();
    victim_target_candidates.sort_by_key(|index| {
        hash(
            context.seed.rotate_left(41),
            &format!(
                "victim-target:{}",
                context.witness_candidates[*index].resident_character_id
            ),
        )
    });
    let (attack_pattern, pattern_bridge) = choose(
        context.seed.rotate_left(37),
        "module.attack_pattern",
        "relation.pattern.family",
        &attack_pattern_candidates(family, !victim_target_candidates.is_empty()),
        &mut trace,
    )?;
    let pattern_target = (attack_pattern == AttackPattern::VictimSpecific).then(|| {
        let candidate = &context.witness_candidates[*victim_target_candidates
            .first()
            .expect("victim pattern hard-zeroed without a target")];
        GeneratedPatternTarget {
            cohort_id: scoped_id(&prefix, "cohort", "victim-profile"),
            resident_character_id: candidate.resident_character_id.clone(),
            demographic: candidate.demographic,
            age_band: candidate.age_band.clone(),
            sex: candidate.sex.clone(),
            profession: candidate.profession.clone(),
            expected_settlement_id: context.settlement_id.clone(),
            expected_location: candidate.expected_location.clone(),
            expected_location_label: candidate.expected_location_label.clone(),
            presence_version: candidate.presence_version,
        }
    });
    let canonical_case_id = format!(
        "case:{:016x}",
        hash(
            context.seed,
            &format!("{}:{}", context.settlement_id, context.ordinal)
        )
    );
    let public_case_id = scoped_id(&prefix, "journal", "case");
    let problem_id = scoped_id(&prefix, "problem", "settlement");
    let finale_site = SiteId::new(scoped_id(&prefix, "site", "finale"));
    let evidence_site = SiteId::new(scoped_id(&prefix, "site", "evidence"));
    let decoy_site = SiteId::new(scoped_id(&prefix, "site", "decoy"));
    let witness1 = WitnessId::new(scoped_id(&prefix, "witness", "primary"));
    let witness2 = WitnessId::new(scoped_id(&prefix, "witness", "corroborating"));
    let npc1 = primary.resident_character_id.clone();
    let npc2 = secondary.resident_character_id.clone();
    let presented_site_kind = if reliability == Reliability::Truthful {
        site
    } else {
        secondary_site_kind
    };
    let (presented_location_statement, presented_location_challenge, presented_location_responses) =
        match account_style {
            AccountStyle::VisualClaim => {
                let claim = format!(
                    "{}, near {}",
                    ambiguous_report_description(description),
                    label(presented_site_kind)
                );
                (
                    format!("It looked like {claim}."),
                    claim,
                    TestimonyChallengeResponses {
                        charm: Some("Your eye was keen. What made the shape seem so?".into()),
                        command: Some("Name what you truly saw, without embellishment.".into()),
                        bluff: Some("That shape was seen elsewhere; amend your account.".into()),
                    },
                )
            }
            AccountStyle::HeardOnly => {
                let claim = format!("something moving near {}", label(presented_site_kind));
                (
                    format!("I only heard {claim}; I never saw it clearly."),
                    claim,
                    TestimonyChallengeResponses {
                        charm: Some("Describe the sound as carefully as you can.".into()),
                        command: Some(
                            "Tell me exactly what you heard and from which direction.".into(),
                        ),
                        bluff: Some(
                            "Others heard a different sound there; account for that.".into(),
                        ),
                    },
                )
            }
            AccountStyle::TracksAndMovement => {
                let claim = format!(
                    "The trail and movement seemed to point toward {}",
                    label(presented_site_kind)
                );
                (
                    format!("{claim}."),
                    claim,
                    TestimonyChallengeResponses {
                        charm: Some("Help me follow how those signs led you that way.".into()),
                        command: Some(
                            "Separate the tracks you saw from the course you inferred.".into(),
                        ),
                        bluff: Some(
                            "That trail turns elsewhere on my map; explain your route.".into(),
                        ),
                    },
                )
            }
        };
    let true_statement = format!(
        "I saw signs pointing toward {}, but I could not identify the culprit.",
        label(site)
    );
    let description_prop = scoped_id(&prefix, "proposition", "description");
    let correction_prop = scoped_id(&prefix, "proposition", "location:corrected");
    let pattern_prop = scoped_id(&prefix, "proposition", "attack-pattern");
    let private_pattern_prop = scoped_id(&prefix, "proposition", "private-pattern-detail");
    let pattern_evidence_id = EvidenceId::new(scoped_id(&prefix, "evidence", "attack-pattern"));
    let pattern_truth = match attack_pattern {
        AttackPattern::Nightly => "The incidents cluster after nightfall.".to_owned(),
        AttackPattern::Roadside => {
            "The incidents cluster along the road used by passing traffic.".to_owned()
        }
        AttackPattern::VictimSpecific => {
            let target = pattern_target
                .as_ref()
                .expect("victim-specific pattern has a bound cohort");
            format!(
                "The incidents disproportionately affect people connected with the {} trade near {}.",
                target.profession, target.expected_location_label
            )
        }
        AttackPattern::Irregular => {
            "The incidents have no reliable time, place, or victim schedule.".to_owned()
        }
    };
    // Every primary witness volunteers this reliability-neutral account. The
    // optional exact detail is a separate private concern, so its existence
    // cannot change any part of the initial dialogue projection.
    let uncorroborated_pattern_claim =
        "There may be a pattern, but I cannot tell which details matter.".to_owned();
    let has_private_pattern_detail = hash(
        context.observer_entropy_hi ^ context.observer_entropy_lo.rotate_left(17),
        "testimony-concern:private-pattern-detail",
    ) % 2
        == 0;
    let evidence_site_label = if family == TemplateFamily::RecurringDepredation {
        "the latest incident site"
    } else {
        "the last-known place"
    };
    let primary_evidence_id = EvidenceId::new(scoped_id(&prefix, "evidence", "tracks"));
    let primary_evidence_reference = evidence_reference(evidence_kind);
    let sites = vec![
        GeneratedSite {
            id: finale_site.clone(),
            kind: site,
            role: SiteRole::Finale,
            terrain: terrain(site),
            safe_label: label(site).into(),
            exact_location_initially_known: false,
            is_true_location: true,
        },
        GeneratedSite {
            id: evidence_site.clone(),
            kind: if family == TemplateFamily::RecurringDepredation {
                SiteKind::Roadside
            } else {
                SiteKind::OccupiedHouse
            },
            role: if family == TemplateFamily::RecurringDepredation {
                SiteRole::Evidence
            } else {
                SiteRole::LastKnown
            },
            terrain: Terrain::Settlement,
            safe_label: evidence_site_label.into(),
            exact_location_initially_known: true,
            is_true_location: false,
        },
        GeneratedSite {
            id: decoy_site.clone(),
            kind: secondary_site_kind,
            role: SiteRole::Decoy,
            terrain: terrain(secondary_site_kind),
            safe_label: format!("Place {} described", primary.display_name),
            exact_location_initially_known: false,
            is_true_location: false,
        },
    ];
    let mut primary_testimony = vec![
        TestimonyDraft {
            proposition_id: description_prop.clone(),
            reliability,
            delivery: TestimonyDelivery::Volunteered,
            truthful_text: true_statement.clone(),
            // Presentation and grant shape cannot reveal reliability.
            // Private authority still binds the proposition to the
            // place the witness actually believes they described.
            spoken_text: presented_location_statement,
            challenge_text: presented_location_challenge,
            challenge_responses: presented_location_responses,
            destination_stage: "route_segment".into(),
            site_id: Some(if reliability == Reliability::Truthful {
                finale_site.clone()
            } else {
                decoy_site.clone()
            }),
            corrects_proposition_id: None,
            referred_witness_ids: vec![witness2.clone()],
        },
        TestimonyDraft {
            proposition_id: pattern_prop.clone(),
            reliability,
            delivery: TestimonyDelivery::Volunteered,
            truthful_text: pattern_truth.clone(),
            spoken_text: uncorroborated_pattern_claim,
            challenge_text: "I cannot tell which details matter".into(),
            challenge_responses: TestimonyChallengeResponses {
                charm: Some("Take your time—which detail first suggested a pattern?".into()),
                command: Some("Separate what you observed from what you merely suppose.".into()),
                bluff: Some("I know which detail matters; tell me what you withheld.".into()),
            },
            destination_stage: "textual".into(),
            site_id: None,
            corrects_proposition_id: None,
            referred_witness_ids: vec![],
        },
        TestimonyDraft {
            proposition_id: correction_prop.clone(),
            reliability: Reliability::Truthful,
            delivery: TestimonyDelivery::Volunteered,
            truthful_text: format!(
                "I noticed {primary_evidence_reference} worth inspecting at {evidence_site_label}."
            ),
            spoken_text: format!(
                "I noticed {primary_evidence_reference} worth inspecting at {evidence_site_label}. You may examine it yourself."
            ),
            challenge_text: format!(
                "{primary_evidence_reference} worth inspecting at {evidence_site_label}"
            ),
            challenge_responses: TestimonyChallengeResponses {
                charm: Some("Show me how you came upon that clue.".into()),
                command: Some("State exactly where and when you found it.".into()),
                bluff: Some("The site was searched already; tell me what I will find.".into()),
            },
            destination_stage: "exact_believed".into(),
            site_id: Some(evidence_site.clone()),
            corrects_proposition_id: None,
            referred_witness_ids: vec![],
        },
    ];
    if has_private_pattern_detail {
        primary_testimony.push(TestimonyDraft {
            proposition_id: private_pattern_prop,
            reliability: Reliability::Truthful,
            delivery: TestimonyDelivery::Withheld,
            truthful_text: pattern_truth.clone(),
            spoken_text: format!("What I held back is this: {pattern_truth}"),
            challenge_text: pattern_truth.trim_end_matches('.').into(),
            challenge_responses: TestimonyChallengeResponses {
                charm: Some("Thank you for saying it. What else attends that detail?".into()),
                command: Some("Give the whole account now.".into()),
                bluff: Some("That confirms what I heard elsewhere; continue.".into()),
            },
            destination_stage: "textual".into(),
            site_id: None,
            corrects_proposition_id: None,
            referred_witness_ids: vec![],
        });
    }
    let (
        secondary_truthful_text,
        secondary_spoken_text,
        secondary_challenge_text,
        secondary_corrects_proposition_id,
    ) = if reliability == Reliability::Truthful {
        let route = format!(
            "tracks continue toward {}, consistent with the earlier account",
            label(site)
        );
        (
            format!("The {route}."),
            format!("Those {route}."),
            route,
            None,
        )
    } else {
        let route = format!(
            "tracks turn away before reaching {} and continue elsewhere",
            label(secondary_site_kind)
        );
        (
            "The earlier location does not fit the tracks; they lead elsewhere.".into(),
            format!("Those {route}."),
            route,
            Some(description_prop.clone()),
        )
    };
    let witnesses = vec![
        WitnessBinding {
            id: witness1.clone(),
            resident_character_id: npc1,
            display_name: primary.display_name.clone(),
            demographic,
            circumstance,
            description,
            expected_location: primary.expected_location.clone(),
            expected_location_label: primary.expected_location_label.clone(),
            visible_description: primary.visible_description.clone(),
            testimony: primary_testimony,
        },
        WitnessBinding {
            id: witness2.clone(),
            resident_character_id: npc2,
            display_name: secondary.display_name.clone(),
            demographic: secondary.demographic,
            circumstance: secondary_circumstance,
            description,
            expected_location: secondary.expected_location.clone(),
            expected_location_label: secondary.expected_location_label.clone(),
            visible_description: secondary.visible_description.clone(),
            testimony: vec![TestimonyDraft {
                proposition_id: description_prop.clone(),
                reliability: Reliability::Truthful,
                delivery: TestimonyDelivery::Volunteered,
                truthful_text: secondary_truthful_text,
                spoken_text: secondary_spoken_text,
                challenge_text: secondary_challenge_text,
                challenge_responses: TestimonyChallengeResponses {
                    charm: Some("Help me understand how the tracks establish that course.".into()),
                    command: Some("Point out their exact course.".into()),
                    bluff: Some(
                        "I followed part of that trail already; complete the route.".into(),
                    ),
                },
                destination_stage: "route_segment".into(),
                site_id: Some(finale_site.clone()),
                corrects_proposition_id: secondary_corrects_proposition_id,
                referred_witness_ids: vec![],
            }],
        },
    ];
    let mut evidence = vec![
        generated_evidence(
            primary_evidence_id,
            evidence_kind,
            correction_prop.clone(),
            evidence_site.clone(),
            "This clue preserves a useful lead without identifying the culprit outright.".into(),
            Some(scoped_id(&prefix, "proposition", "description")),
            investigability,
        ),
        generated_evidence(
            EvidenceId::new(scoped_id(&prefix, "evidence", "token")),
            EvidenceKind::DroppedToken,
            scoped_id(&prefix, "proposition", "association"),
            decoy_site.clone(),
            "A dropped token links the report to another person, not necessarily the culprit."
                .into(),
            None,
            investigability,
        ),
        generated_evidence(
            pattern_evidence_id.clone(),
            EvidenceKind::LedgerEntry,
            pattern_prop,
            evidence_site.clone(),
            format!("Corroborated accounts show: {pattern_truth}"),
            None,
            investigability,
        ),
    ];
    let area_id = scoped_id(&prefix, "area", "incident");
    let hostile_id = scoped_id(&prefix, "hostile-group", "finale");
    let subject =
        SubjectId::new(scoped_id(&prefix, "subject", "missing-person")).expect("generated subject");
    let asset =
        AssetId::new(scoped_id(&prefix, "asset", "missing-property")).expect("generated asset");
    let trail_id = TrackTrailId::new(scoped_id(&prefix, "track-trail", "physical"));
    let first_segment_id = TrackSegmentId::new(scoped_id(&prefix, "track-segment", "physical:0"));
    let final_segment_id = TrackSegmentId::new(scoped_id(&prefix, "track-segment", "physical:1"));
    let track_segments = vec![
        TrackSegment {
            id: first_segment_id.clone(),
            trail_id: trail_id.clone(),
            ordinal: 0,
            terrain: Terrain::Settlement,
            safe_finding:
                "The impressions continue beyond the broken ground in a consistent direction."
                    .into(),
            predecessor: None,
            next: Some(final_segment_id.clone()),
        },
        TrackSegment {
            id: final_segment_id.clone(),
            trail_id: trail_id.clone(),
            ordinal: 1,
            terrain: terrain(site),
            safe_finding: "The freshest impressions converge on one occupied site.".into(),
            predecessor: Some(first_segment_id.clone()),
            next: None,
        },
    ];
    let track_trails = vec![TrackTrail {
        id: trail_id,
        segment_ids: vec![first_segment_id, final_segment_id],
    }];
    let mut actions = build_actions(
        &prefix,
        family,
        &finale_site,
        &area_id,
        primary.resident_character_id,
        route_variant,
        attack_pattern,
        pattern_target.as_ref(),
        &pattern_evidence_id,
        &track_segments,
    );
    let issuer = context
        .witness_candidates
        .get(2)
        .unwrap_or(secondary)
        .resident_character_id
        .clone();
    let (objectives, finales, custody, dialogue_producers) = match family {
        TemplateFamily::RecurringDepredation => {
            let hostile_character_id = crate::settlement_population::stable_hash(&format!("field-character:{hostile_id}:0")) | (1u64 << 63);
            let inspect_finale=ActionId::new(scoped_id(&prefix,"action","inspect_finale"));
            let ambush=ActionId::new(scoped_id(&prefix,"action","ambush"));
            for action in actions.iter_mut().filter(|a|a.id==inspect_finale||a.id==ambush){action.outputs.push(GeneratedActionOutput::SystemicOutcome{outcome:GeneratedSystemicOutcome::Surrender{character_id:hostile_character_id,context_id:hostile_id.clone()}});}
            (
            ObjectiveExpression::new(vec![
                ObjectivePath {
                    objectives: vec![Objective {
                        id: ObjectiveId::new(scoped_id(&prefix, "objective", "defeat")).unwrap(),
                        requirement: ObjectiveRequirement::Defeat {
                            hostile_group_id: hostile_id.clone(),
                            count: 1,
                        },
                    }],
                },
                ObjectivePath { objectives: vec![Objective { id: ObjectiveId::new(scoped_id(&prefix,"objective","surrender")).unwrap(), requirement: ObjectiveRequirement::Surrender { character_id: hostile_character_id, context_id: hostile_id.clone() } }] },
                ObjectivePath {
                    objectives: vec![Objective {
                        id: ObjectiveId::new(scoped_id(&prefix, "objective", "driveoff")).unwrap(),
                        requirement: ObjectiveRequirement::DriveOff {
                            hostile_group_id: hostile_id.clone(),
                        },
                    }],
                },
            ])
            .expect("generated objective"),
            vec![
                GeneratedFinale {
                    id: FinaleId::new(scoped_id(&prefix, "finale", "defeat")),
                    kind: FinaleKind::Defeat,
                    site_id: finale_site.clone(),
                    hostile_group_id: Some(hostile_id.clone()),
                    subject_id: None,
                    asset_id: None,
                    strategic_outcome_compatible: true,
                },
                GeneratedFinale {
                    id: FinaleId::new(scoped_id(&prefix, "finale", "driveoff")),
                    kind: FinaleKind::DriveOff,
                    site_id: finale_site.clone(),
                    hostile_group_id: Some(hostile_id.clone()),
                    subject_id: None,
                    asset_id: None,
                    strategic_outcome_compatible: true,
                },
            ],
            vec![],
            vec![],
        )},
        TemplateFamily::DisappearanceOrLoss => match cause {
            CanonicalCause::Hostile(_) | CanonicalCause::ConcealmentByWitness => {
                let objective_id =
                    ObjectiveId::new(scoped_id(&prefix, "objective", "rescue")).unwrap();
                let physical_resolution =
                    ActionId::new(scoped_id(&prefix, "action", "resolve_physical"));
                let social_resolution =
                    ActionId::new(scoped_id(&prefix, "action", "resolve_social"));
                for action in actions.iter_mut().filter(|action| {
                    action.id == physical_resolution || action.id == social_resolution
                }) {
                    action.outputs.push(GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RescueSubject {
                            subject_id: subject.as_str().into(),
                            next_version: 1,
                        },
                    });
                }
                (
                    ObjectiveExpression::new(vec![ObjectivePath {
                        objectives: vec![Objective {
                            id: objective_id,
                            requirement: ObjectiveRequirement::Rescue {
                                subject_id: subject.clone(),
                            },
                        }],
                    }])
                    .expect("generated rescue objective"),
                    vec![GeneratedFinale {
                        id: FinaleId::new(scoped_id(&prefix, "finale", "rescue")),
                        kind: FinaleKind::Rescue,
                        site_id: finale_site.clone(),
                        hostile_group_id: matches!(cause, CanonicalCause::Hostile(_))
                            .then_some(hostile_id.clone()),
                        subject_id: Some(subject.as_str().into()),
                        asset_id: None,
                        strategic_outcome_compatible: true,
                    }],
                    vec![(subject.as_str().into(), finale_site.clone())],
                    vec![],
                )
            }
            CanonicalCause::IncidentalLoss => {
                let physical_resolution =
                    ActionId::new(scoped_id(&prefix, "action", "resolve_physical"));
                let social_resolution =
                    ActionId::new(scoped_id(&prefix, "action", "resolve_social"));
                for action in actions.iter_mut().filter(|action| {
                    action.id == physical_resolution || action.id == social_resolution
                }) {
                    action.outputs.push(GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RetrieveAsset {
                            asset_id: asset.as_str().into(),
                            next_version: 1,
                        },
                    });
                }
                let retrieve_id =
                    ObjectiveId::new(scoped_id(&prefix, "objective", "retrieve")).unwrap();
                let return_id =
                    ObjectiveId::new(scoped_id(&prefix, "objective", "return")).unwrap();
                (
                    ObjectiveExpression::new(vec![ObjectivePath {
                        objectives: vec![
                            Objective {
                                id: retrieve_id,
                                requirement: ObjectiveRequirement::Retrieve {
                                    asset_id: asset.clone(),
                                },
                            },
                            Objective {
                                id: return_id.clone(),
                                requirement: ObjectiveRequirement::Return {
                                    asset_id: asset.clone(),
                                    custodian_id: issuer.to_string(),
                                },
                            },
                        ],
                    }])
                    .expect("generated recovery objective"),
                    vec![GeneratedFinale {
                        id: FinaleId::new(scoped_id(&prefix, "finale", "return")),
                        kind: FinaleKind::RetrieveReturn,
                        site_id: finale_site.clone(),
                        hostile_group_id: None,
                        subject_id: None,
                        asset_id: Some(asset.as_str().into()),
                        strategic_outcome_compatible: false,
                    }],
                    vec![(asset.as_str().into(), finale_site.clone())],
                    vec![GeneratedDialogueProducer {
                        action: GeneratedDialogueAction::ReturnAsset,
                        objective_id: return_id,
                        recipient_resident_character_id: issuer.clone(),
                        subject_ref: None,
                        asset_id: Some(asset.as_str().into()),
                    }],
                )
            }
            CanonicalCause::FabricatedClaim => {
                let objective_id =
                    ObjectiveId::new(scoped_id(&prefix, "objective", "expose")).unwrap();
                (
                    ObjectiveExpression::new(vec![ObjectivePath {
                        objectives: vec![Objective {
                            id: objective_id.clone(),
                            requirement: ObjectiveRequirement::Expose {
                                subject_ref: description_prop.clone(),
                            },
                        }],
                    }])
                    .expect("generated exposure objective"),
                    vec![GeneratedFinale {
                        id: FinaleId::new(scoped_id(&prefix, "finale", "expose")),
                        kind: FinaleKind::Expose,
                        site_id: finale_site.clone(),
                        hostile_group_id: None,
                        subject_id: Some(description_prop.clone()),
                        asset_id: None,
                        strategic_outcome_compatible: false,
                    }],
                    vec![],
                    vec![GeneratedDialogueProducer {
                        action: GeneratedDialogueAction::Expose,
                        objective_id,
                        recipient_resident_character_id: issuer.clone(),
                        subject_ref: Some(description_prop.clone()),
                        asset_id: None,
                    }],
                )
            }
            CanonicalCause::VoluntaryDisappearance => unreachable!(
                "voluntary disappearance is excluded until locate/report producers exist"
            ),
        },
        TemplateFamily::Outbreak => unreachable!("outbreak uses its dedicated typed assembler"),
    };
    let template_id = match family {
        TemplateFamily::RecurringDepredation => "recurring_depredation",
        TemplateFamily::DisappearanceOrLoss => "disappearance_or_loss",
        TemplateFamily::Outbreak => "outbreak",
    };
    let cause_key = match cause {
        CanonicalCause::Hostile(_) => "hostile",
        CanonicalCause::ConcealmentByWitness => "concealment",
        CanonicalCause::IncidentalLoss => "incidental_loss",
        CanonicalCause::FabricatedClaim => "fabricated",
        CanonicalCause::VoluntaryDisappearance => "voluntary_disappearance",
    };
    let template = crate::quest_catalog::catalog()
        .template(template_id)
        .unwrap();
    let configured_finales = template
        .cause_finales
        .get(cause_key)
        .or_else(|| template.cause_finales.get("*"))
        .expect("validated template cause/finale coverage");
    assert_eq!(
        finales
            .iter()
            .map(|finale| match finale.kind {
                FinaleKind::Defeat => "defeat",
                FinaleKind::DriveOff => "drive_off",
                FinaleKind::Capture => "capture",
                FinaleKind::Rescue => "rescue",
                FinaleKind::RetrieveReturn => "retrieve_return",
                FinaleKind::Expose => "expose",
                FinaleKind::Negotiate => "negotiate",
            })
            .collect::<Vec<_>>(),
        *configured_finales,
        "typed objective assembler must implement the YAML finale plan"
    );
    let mut bridges = Vec::new();
    for key in [
        family_bridge,
        cause_bridge,
        site_bridge,
        circ_bridge,
        description_bridge,
        reliability_bridge,
        evidence_bridge,
        account_bridge,
        route_bridge,
        pattern_bridge,
        secondary_site_bridge,
        secondary_circumstance_bridge,
    ]
    .into_iter()
    .flatten()
    {
        if !bridges.iter().any(|b: &CausalBridge| b.id.0 == key) {
            bridges.push(bridge(key, &prefix, family, context.now_minute));
        }
    }
    for item in &bridges {
        let bridge_proposition_id =
            scoped_id(&prefix, "proposition", &format!("bridge:{}", item.id.0));
        if !evidence
            .iter()
            .any(|candidate| candidate.id == item.evidence_id)
        {
            evidence.push(generated_evidence(
                item.evidence_id.clone(),
                EvidenceKind::DroppedToken,
                bridge_proposition_id,
                evidence_site.clone(),
                item.lead_summary.clone(),
                None,
                investigability,
            ));
        }
        if let Some(action) = actions
            .iter_mut()
            .find(|action| action.id == item.action_id)
            && !action.outputs.iter().any(|output| {
                matches!(
                    output,
                    GeneratedActionOutput::Evidence { evidence_id }
                        if evidence_id == &item.evidence_id
                )
            })
        {
            action.outputs.push(GeneratedActionOutput::Evidence {
                evidence_id: item.evidence_id.clone(),
            });
        }
    }
    let canonical_events = vec![CanonicalEvent {
        id: scoped_id(&prefix, "event", "incident"),
        proposition_id: scoped_id(&prefix, "proposition", "truth"),
        subject: format!("{cause:?}"),
        predicate: "caused".into(),
        object: format!(
            "{attack_pattern:?}:{:?}",
            consequence(cause, template).symptom
        ),
        occurred_at: context.now_minute.saturating_sub(180),
    }]
    .into_iter()
    .chain(bridges.iter().map(|b| CanonicalEvent {
        id: b.event_id.clone(),
        proposition_id: scoped_id(&prefix, "proposition", &format!("bridge:{}", b.id.0)),
        subject: "causal bridge".into(),
        predicate: "explains".into(),
        object: b.explanation.clone(),
        occurred_at: context.now_minute.saturating_sub(120),
    }))
    .collect();
    if trace.len() > MAX_FACTOR_TRACE_RECORDS
        || trace
            .iter()
            .map(|item| {
                item.candidate_id.len()
                    + item.hard_zero_reason.as_deref().map_or(0, str::len)
                    + item.module_id.0.len()
                    + item.relation_id.0.len()
            })
            .sum::<usize>()
            > MAX_FACTOR_TRACE_BYTES
    {
        return Err(GenerationError::CandidateLimit);
    }
    let manifest = GeneratedCase {
        catalog_revision: CATALOG_REVISION.into(),
        generation_seed: context.seed,
        template_id: template.id.clone(),
        configured_routes: template.routes.clone(),
        configured_objectives: template.objectives.clone(),
        incident_interval_minutes: template.incident_interval_minutes,
        maximum_incidents: u16::from(template.maximum_incidents),
        family,
        canonical_case_id,
        public_case_id,
        problem_id,
        cause,
        canonical_events,
        consequence: consequence(cause, template),
        outbreak: None,
        sites,
        areas: vec![GeneratedArea {
            id: area_id,
            safe_label: "the area described by local accounts".into(),
            terrain: Terrain::Settlement,
            contains_site_ids: vec![evidence_site.clone(), decoy_site],
        }],
        witnesses,
        pattern_targets: pattern_target.into_iter().collect(),
        evidence,
        track_trails,
        track_segments,
        actions,
        objectives,
        custody,
        hostile_groups: match cause {
            CanonicalCause::Hostile(threat) => vec![(hostile_id, finale_site, threat, 1)],
            _ => vec![],
        },
        finales,
        dialogue_producers,
        bridges,
        factor_trace: trace,
    };
    validate(&manifest).map_err(GenerationError::InvalidManifest)?;
    Ok(manifest)
}

fn generate_outbreak(context: &GenerationContext) -> Result<GeneratedCase, GenerationError> {
    use crate::disease::DiseaseId;

    if context.witness_candidates.len() < 2 {
        return Err(GenerationError::InvalidManifest(vec![
            "outbreak generation requires two persistent witnesses".into(),
        ]));
    }
    let prefix = observer_scope(context);
    let disease = [
        DiseaseId::Influenza,
        DiseaseId::Mahrdruck,
        DiseaseId::ShroudFever,
        DiseaseId::Bilwisschuss,
        DiseaseId::Kobeldunst,
    ][context.seed as usize % 5];
    let transmission_route = crate::disease::definition(disease).primary_community_vector;
    let carrier = ThreatId::Alp;
    let (site_kind, source, remediation, responsible_npc, carrier_threat) = match disease {
        DiseaseId::Influenza if (context.seed / 5) % 2 == 0 => (
            SiteKind::OccupiedHouse,
            OutbreakSource::Sanitation {
                practice: OutbreakSanitationPractice::UnwashedSharedBedding,
            },
            OutbreakRemediation::Sanitation {
                action: OutbreakSanitationAction::LaunderBedding,
            },
            Some(ResponsibleOutbreakNpc {
                resident_character_id: context.witness_candidates[1].resident_character_id.clone(),
                culpability: OutbreakCulpability::Negligent,
            }),
            None,
        ),
        DiseaseId::Influenza => (
            SiteKind::OccupiedHouse,
            OutbreakSource::Behavior {
                practice: OutbreakBehaviorPractice::CrowdedSleeping,
            },
            OutbreakRemediation::Behavior {
                action: OutbreakBehaviorAction::SeparateSleepers,
            },
            Some(ResponsibleOutbreakNpc {
                resident_character_id: context.witness_candidates[1].resident_character_id.clone(),
                culpability: OutbreakCulpability::Innocent,
            }),
            None,
        ),
        DiseaseId::Mahrdruck => (
            SiteKind::OccupiedHouse,
            OutbreakSource::ThreatVector { threat: carrier },
            OutbreakRemediation::ResolveCarrierThreat {
                hostile_group_id: scoped_id(&prefix, "hostile", "carrier"),
                accepted_outcomes: vec![
                    OutbreakCarrierOutcome::Defeated,
                    OutbreakCarrierOutcome::DrivenOff,
                ],
            },
            None,
            Some(carrier),
        ),
        DiseaseId::ShroudFever => (
            SiteKind::Graveyard,
            OutbreakSource::Environmental {
                reservoir: OutbreakEnvironmentalReservoir::GraveMould,
            },
            OutbreakRemediation::RemoveEnvironmentalSource {
                reservoir: OutbreakEnvironmentalReservoir::GraveMould,
            },
            None,
            None,
        ),
        DiseaseId::Bilwisschuss => (
            SiteKind::AbandonedFarm,
            OutbreakSource::Environmental {
                reservoir: OutbreakEnvironmentalReservoir::RyeGalls,
            },
            OutbreakRemediation::RemoveEnvironmentalSource {
                reservoir: OutbreakEnvironmentalReservoir::RyeGalls,
            },
            None,
            None,
        ),
        DiseaseId::Kobeldunst => (
            SiteKind::Cave,
            OutbreakSource::Environmental {
                reservoir: OutbreakEnvironmentalReservoir::OreBiofilm,
            },
            OutbreakRemediation::RemoveEnvironmentalSource {
                reservoir: OutbreakEnvironmentalReservoir::OreBiofilm,
            },
            None,
            None,
        ),
        _ => unreachable!("bounded outbreak disease catalog"),
    };
    debug_assert!(crate::disease::definition(disease).supports(transmission_route));

    let canonical_case_id = format!(
        "case:{:016x}",
        hash(
            context.seed,
            &format!("outbreak:{}:{}", context.settlement_id, context.ordinal)
        )
    );
    let public_case_id = scoped_id(&prefix, "journal", "case");
    let problem_id = scoped_id(&prefix, "problem", "outbreak");
    let source_site = SiteId::new(scoped_id(&prefix, "site", "source"));
    let patient_site = SiteId::new(scoped_id(&prefix, "site", "patient"));
    let evidence_id = EvidenceId::new(scoped_id(&prefix, "evidence", "pattern"));
    let pathology_evidence_id = EvidenceId::new(scoped_id(&prefix, "evidence", "pathology"));
    let physical_action = ActionId::new(scoped_id(&prefix, "action", "inspect-source"));
    let social_action = ActionId::new(scoped_id(&prefix, "action", "compare-patients"));
    let primary = &context.witness_candidates[0];
    let secondary = &context.witness_candidates[1];
    let witness1 = WitnessId::new(scoped_id(&prefix, "witness", "patient-carer"));
    let witness2 = WitnessId::new(scoped_id(&prefix, "witness", "family-member"));
    let common_claim = "Several households became feverish within the same few days";
    let testimony = |proposition_id: &str,
                     referred_witness_ids: Vec<WitnessId>,
                     corroborating: bool|
     -> TestimonyDraft {
        TestimonyDraft {
            proposition_id: proposition_id.into(),
            reliability: Reliability::Truthful,
            delivery: TestimonyDelivery::Volunteered,
            truthful_text: format!("{common_claim}."),
            spoken_text: format!("{common_claim}."),
            challenge_text: common_claim.into(),
            challenge_responses: TestimonyChallengeResponses {
                charm: Some(if corroborating {
                    "Tell me which visit you remember most clearly.".into()
                } else {
                    "Help me set the order of those illnesses carefully.".into()
                }),
                command: Some(if corroborating {
                    "Give the visits in order, omitting no household.".into()
                } else {
                    "Name each household and the day its sickness began.".into()
                }),
                bluff: Some(if corroborating {
                    "The household marks disagree with your order; account for it.".into()
                } else {
                    "Another account puts the first fever elsewhere; explain that.".into()
                }),
            },
            destination_stage: "textual".into(),
            site_id: None,
            corrects_proposition_id: None,
            referred_witness_ids,
        }
    };
    let witnesses = vec![
        WitnessBinding {
            id: witness1.clone(),
            resident_character_id: primary.resident_character_id.clone(),
            display_name: primary.display_name.clone(),
            demographic: primary.demographic,
            circumstance: primary
                .allowed_circumstances
                .iter()
                .next()
                .copied()
                .unwrap_or(Circumstance::NightWindow),
            description: ReportDescription::UnseenNightVisitor,
            expected_location: primary.expected_location.clone(),
            expected_location_label: primary.expected_location_label.clone(),
            visible_description: primary.visible_description.clone(),
            testimony: vec![testimony(
                "outbreak:early-pattern",
                vec![witness2.clone()],
                false,
            )],
        },
        WitnessBinding {
            id: witness2,
            resident_character_id: secondary.resident_character_id.clone(),
            display_name: secondary.display_name.clone(),
            demographic: secondary.demographic,
            circumstance: secondary
                .allowed_circumstances
                .iter()
                .next()
                .copied()
                .unwrap_or(Circumstance::NightWindow),
            description: ReportDescription::UnseenNightVisitor,
            expected_location: secondary.expected_location.clone(),
            expected_location_label: secondary.expected_location_label.clone(),
            visible_description: secondary.visible_description.clone(),
            testimony: vec![testimony("outbreak:second-pattern", Vec::new(), true)],
        },
    ];
    let sites = vec![
        GeneratedSite {
            id: source_site.clone(),
            kind: site_kind,
            role: SiteRole::Finale,
            terrain: Terrain::Settlement,
            safe_label: "a place associated with several afflicted households".into(),
            exact_location_initially_known: false,
            is_true_location: true,
        },
        GeneratedSite {
            id: patient_site.clone(),
            kind: SiteKind::OccupiedHouse,
            role: SiteRole::Evidence,
            terrain: Terrain::Settlement,
            safe_label: "the household where the first patient was reported".into(),
            exact_location_initially_known: true,
            is_true_location: false,
        },
    ];
    let evidence = vec![
        GeneratedEvidence {
            id: evidence_id.clone(),
            kind: EvidenceKind::LedgerEntry,
            proposition_id: "outbreak:exposure-order".into(),
            site_id: patient_site.clone(),
            portrait_label: "household sickness notes".into(),
            portrait_icon: "ledger".into(),
            base_description: "Dates and household marks record when illness was first noticed."
                .into(),
            inspection_topics: Vec::new(),
            safe_description:
                "The sequence links cases by place and time, but does not by itself name a disease."
                    .into(),
            corrects_proposition_id: None,
        },
        GeneratedEvidence {
            id: pathology_evidence_id,
            kind: EvidenceKind::BloodlessCorpse,
            proposition_id: "outbreak:systemic-pathology".into(),
            site_id: patient_site.clone(),
            portrait_label: "systemic changes in a victim".into(),
            portrait_icon: "corpse".into(),
            base_description:
                "A physician may preserve bounded systemic observations from the body.".into(),
            inspection_topics: Vec::new(),
            safe_description:
                "The observed systemic pattern supports several illnesses or exposures.".into(),
            corrects_proposition_id: None,
        },
    ];
    let exact = GeneratedActionOutput::Destination {
        stage: GeneratedDestinationStage::Exact,
        site_id: Some(source_site.clone()),
    };
    let mut actions = vec![
        GeneratedAction {
            id: physical_action.clone(),
            kind: InvestigationActionKind::InspectSite,
            route: RouteClass::PhysicalTrail,
            target_kind: "site".into(),
            target_id: patient_site.0.clone(),
            prerequisite: None,
            alternate: social_action.clone(),
            active_initially: true,
            safe_summary: "Inspect shared places and material traces.".into(),
            track_segment_id: None,
            outputs: vec![
                GeneratedActionOutput::Evidence {
                    evidence_id: evidence_id.clone(),
                },
                exact.clone(),
            ],
        },
        GeneratedAction {
            id: social_action.clone(),
            kind: InvestigationActionKind::LocateContact,
            route: RouteClass::SocialInquiry,
            target_kind: "contact".into(),
            target_id: secondary.resident_character_id.to_string(),
            prerequisite: None,
            alternate: physical_action,
            active_initially: true,
            safe_summary: "Compare household accounts and the order of symptoms.".into(),
            track_segment_id: None,
            outputs: vec![exact],
        },
    ];
    let remediation_ref = format!(
        "outbreak-remediation:{}",
        match &remediation {
            OutbreakRemediation::Sanitation { action } => format!("sanitation:{action:?}"),
            OutbreakRemediation::Behavior { action } => format!("behavior:{action:?}"),
            OutbreakRemediation::RemoveEnvironmentalSource { reservoir } =>
                format!("environment:{reservoir:?}"),
            OutbreakRemediation::ResolveCarrierThreat {
                hostile_group_id, ..
            } => format!("carrier:{hostile_group_id}"),
        }
        .to_ascii_lowercase()
    );
    let physical_remediation = ActionId::new(scoped_id(&prefix, "action", "remediate-physical"));
    let social_remediation = ActionId::new(scoped_id(&prefix, "action", "remediate-social"));
    if !matches!(
        &remediation,
        OutbreakRemediation::ResolveCarrierThreat { .. }
    ) {
        actions.extend([
            GeneratedAction {
                id: physical_remediation.clone(),
                kind: InvestigationActionKind::InspectSite,
                route: RouteClass::PhysicalTrail,
                target_kind: "site".into(),
                target_id: source_site.0.clone(),
                prerequisite: Some(actions[0].id.clone()),
                alternate: social_remediation.clone(),
                active_initially: false,
                safe_summary: "Apply the supported physical source intervention.".into(),
                track_segment_id: None,
                outputs: vec![GeneratedActionOutput::Remediation {
                    remediation_id: remediation_ref.clone(),
                }],
            },
            GeneratedAction {
                id: social_remediation,
                kind: InvestigationActionKind::InspectSite,
                route: RouteClass::SocialInquiry,
                target_kind: "site".into(),
                target_id: source_site.0.clone(),
                prerequisite: Some(actions[1].id.clone()),
                alternate: physical_remediation,
                active_initially: false,
                safe_summary: "Apply the supported physical source intervention.".into(),
                track_segment_id: None,
                outputs: vec![GeneratedActionOutput::Remediation {
                    remediation_id: remediation_ref.clone(),
                }],
            },
        ]);
    }
    let objective = Objective {
        id: ObjectiveId::new(scoped_id(&prefix, "objective", "remediate"))
            .expect("scoped objective id"),
        requirement: ObjectiveRequirement::RemediateSource {
            remediation_id: remediation_ref,
        },
    };
    let patient_ref = |name: &str| scoped_id(&prefix, "patient", name);
    let patient_course =
        |name: &str, resident_character_id: u64, immunity_milli: u16, carrier_death: bool| {
            let patient_ref = patient_ref(name);
            let definition = crate::disease::definition(disease);
            let course_duration = definition
                .incubation_minutes
                .saturating_add(definition.rise_minutes)
                .saturating_add(definition.peak_minutes)
                .saturating_add(definition.recovery_minutes);
            let exposed_at = context.now_minute.saturating_sub(course_duration);
            let episode_id = crate::disease::outbreak_exposure_seed(
                resident_character_id,
                &format!("{}:{patient_ref}", problem_id),
            );
            let episode = crate::disease::InfectionEpisode {
                id: episode_id,
                character_id: resident_character_id,
                disease_id: disease,
                contracted_at: exposed_at,
                ruleset_version: crate::physiology::PHYSIOLOGY_RULESET_VERSION,
                phenotype_key_version: crate::physiology::PHENOTYPE_KEY_VERSION,
            };
            let became_symptomatic_at = exposed_at.saturating_add(definition.incubation_minutes);
            let immunity = f32::from(immunity_milli) / 1_000.0;
            let terminal = crate::disease::first_combined_terminal(
                &[episode],
                exposed_at,
                exposed_at
                    .saturating_add(definition.incubation_minutes)
                    .saturating_add(definition.rise_minutes)
                    .saturating_add(definition.peak_minutes)
                    .saturating_add(definition.recovery_minutes),
                immunity,
            );
            let (died_at, death_kind) = if carrier_death {
                let attack_at = context
                    .now_minute
                    .saturating_sub(1_440)
                    .max(became_symptomatic_at);
                let attack_precedes_terminal =
                    terminal.is_none_or(|(terminal_at, _)| attack_at < terminal_at);
                if attack_at <= context.now_minute && attack_precedes_terminal {
                    (Some(attack_at), Some(OutbreakPatientDeathKind::CarrierAttack))
                } else {
                    (None, None)
                }
            } else {
                let past_terminal =
                    terminal.filter(|(terminal_at, _)| *terminal_at <= context.now_minute);
                (past_terminal.map(|value| value.0), past_terminal.map(|_| OutbreakPatientDeathKind::Disease))
            };
            OutbreakExposure {
                patient_ref,
                patient_character_id: resident_character_id,
                episode_id,
                exposed_at,
                became_symptomatic_at,
                died_at,
                death_kind,
            }
        };
    let first_patient_killed_by_carrier = matches!(&source, OutbreakSource::ThreatVector { .. });
    let outbreak = GeneratedOutbreak {
        disease,
        transmission_route,
        source,
        physical_source_site: source_site.clone(),
        patient_presentation_site: patient_site.clone(),
        responsible_npc,
        carrier_threat,
        exposure_chronology: vec![
            patient_course(
                "first",
                primary.resident_character_id,
                0,
                first_patient_killed_by_carrier,
            ),
            patient_course("living", secondary.resident_character_id, 5_000, false),
        ],
        remediation,
    };
    let hostile_groups = outbreak
        .carrier_threat
        .map(|threat| {
            let group_id = match &outbreak.remediation {
                OutbreakRemediation::ResolveCarrierThreat {
                    hostile_group_id, ..
                } => hostile_group_id.clone(),
                _ => unreachable!(),
            };
            vec![(group_id, source_site.clone(), threat, 2)]
        })
        .unwrap_or_default();
    let template = crate::quest_catalog::catalog()
        .template("outbreak")
        .expect("validated outbreak template");
    let manifest = GeneratedCase {
        catalog_revision: CATALOG_REVISION.into(),
        generation_seed: context.seed,
        template_id: template.id.clone(),
        configured_routes: template.routes.clone(),
        configured_objectives: template.objectives.clone(),
        incident_interval_minutes: template.incident_interval_minutes,
        maximum_incidents: u16::from(template.maximum_incidents),
        family: TemplateFamily::Outbreak,
        canonical_case_id,
        public_case_id,
        problem_id,
        cause: outbreak
            .carrier_threat
            .map_or(CanonicalCause::IncidentalLoss, CanonicalCause::Hostile),
        canonical_events: vec![CanonicalEvent {
            id: scoped_id(&prefix, "event", "first-cluster"),
            proposition_id: "outbreak:cluster-began".into(),
            subject: "several households".into(),
            predicate: "became ill during".into(),
            object: "the same few days".into(),
            occurred_at: context.now_minute.saturating_sub(3 * 1_440),
        }],
        consequence: ConsequenceProfile {
            symptom: Symptom::SickLocals,
            effects: Effects {
                buy_bps: 300,
                sell_penalty_bps: 150,
                encounter_frequency_bps: 0,
                encounter_archetype: None,
                disease_intensity: 360,
            },
            public_summary:
                "Several households report similar fevers, but their cause is uncertain.".into(),
        },
        outbreak: Some(outbreak),
        sites,
        areas: Vec::new(),
        witnesses,
        pattern_targets: Vec::new(),
        evidence,
        track_trails: Vec::new(),
        track_segments: Vec::new(),
        actions,
        objectives: ObjectiveExpression {
            alternatives: vec![ObjectivePath {
                objectives: vec![objective],
            }],
        },
        custody: Vec::new(),
        hostile_groups,
        finales: Vec::new(),
        dialogue_producers: Vec::new(),
        bridges: Vec::new(),
        factor_trace: vec![FactorTrace {
            module_id: ModuleId::new("module.outbreak"),
            relation_id: RelationId::new("relation.outbreak.disease-source"),
            factor_ids: vec![FactorId::new("factor.outbreak.compatibility")],
            candidate_id: format!("{disease:?}:{transmission_route:?}"),
            plausibility: 100,
            curation: 100,
            accepted: true,
            hard_zero_reason: None,
            required_bridge: None,
            decision: TraceDecision::Bound,
        }],
    };
    validate(&manifest).map_err(GenerationError::InvalidManifest)?;
    Ok(manifest)
}
