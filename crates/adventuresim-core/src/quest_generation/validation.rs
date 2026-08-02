fn validate_track_trails(case: &GeneratedCase, errors: &mut Vec<String>) {
    let trail_ids = case
        .track_trails
        .iter()
        .map(|trail| trail.id.clone())
        .collect::<BTreeSet<_>>();
    let segment_ids = case
        .track_segments
        .iter()
        .map(|segment| segment.id.clone())
        .collect::<BTreeSet<_>>();
    if trail_ids.len() != case.track_trails.len() {
        errors.push("track trails require unique identities".into());
    }
    if segment_ids.len() != case.track_segments.len() {
        errors.push("track segments require unique identities".into());
    }
    let uses_tracking = case.actions.iter().any(|action| {
        action.track_segment_id.is_some()
            || matches!(
                action.kind,
                InvestigationActionKind::FollowTracks | InvestigationActionKind::ReacquireTracks
            )
            || action
                .outputs
                .iter()
                .any(|output| matches!(output, GeneratedActionOutput::TrackFinding { .. }))
    });
    if uses_tracking && case.track_trails.is_empty() {
        errors.push("generated case requires an immutable physical trail".into());
    }
    if !uses_tracking && (!case.track_trails.is_empty() || !case.track_segments.is_empty()) {
        errors.push("non-tracking case carries unbound immutable trail authority".into());
    }
    for segment in &case.track_segments {
        if !trail_ids.contains(&segment.trail_id) {
            errors.push(format!(
                "track segment {} belongs to a missing trail",
                segment.id.0
            ));
        }
    }
    let mut bound_segments = BTreeMap::<TrackSegmentId, &GeneratedAction>::new();
    for action in &case.actions {
        let findings = action
            .outputs
            .iter()
            .filter_map(|output| match output {
                GeneratedActionOutput::TrackFinding {
                    segment_id,
                    finding,
                } => Some((segment_id, finding)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let Some(segment_id) = &action.track_segment_id else {
            if !findings.is_empty() {
                errors.push(format!(
                    "{} exposes a track finding without segment authority",
                    action.id.0
                ));
            }
            continue;
        };
        let Some(segment) = case
            .track_segments
            .iter()
            .find(|segment| &segment.id == segment_id)
        else {
            errors.push(format!(
                "{} binds missing track segment {}",
                action.id.0, segment_id.0
            ));
            continue;
        };
        if bound_segments.insert(segment_id.clone(), action).is_some() {
            errors.push(format!(
                "track segment {} is bound by multiple actions",
                segment_id.0
            ));
        }
        if action.route != RouteClass::PhysicalTrail
            || !matches!(
                action.kind,
                InvestigationActionKind::FollowTracks | InvestigationActionKind::ReacquireTracks
            )
        {
            errors.push(format!(
                "{} binds a track segment without a physical tracking action",
                action.id.0
            ));
        }
        if findings.len() != 1
            || findings[0].0 != segment_id
            || findings[0].1 != &segment.safe_finding
        {
            errors.push(format!(
                "{} does not emit its exact safe track finding",
                action.id.0
            ));
        }
    }
    for trail in &case.track_trails {
        if !(2..=4).contains(&trail.segment_ids.len()) {
            errors.push(format!(
                "track trail {} is not a short segment chain",
                trail.id.0
            ));
            continue;
        }
        let owned = case
            .track_segments
            .iter()
            .filter(|segment| segment.trail_id == trail.id)
            .collect::<Vec<_>>();
        if owned.len() != trail.segment_ids.len()
            || trail
                .segment_ids
                .iter()
                .any(|id| !owned.iter().any(|segment| &segment.id == id))
        {
            errors.push(format!(
                "track trail {} does not own exactly its declared segments",
                trail.id.0
            ));
            continue;
        }
        for (ordinal, segment_id) in trail.segment_ids.iter().enumerate() {
            let Some(segment) = owned.iter().find(|segment| &segment.id == segment_id) else {
                continue;
            };
            let predecessor = ordinal
                .checked_sub(1)
                .and_then(|index| trail.segment_ids.get(index));
            let next = trail.segment_ids.get(ordinal + 1);
            if usize::from(segment.ordinal) != ordinal
                || segment.predecessor.as_ref() != predecessor
                || segment.next.as_ref() != next
                || segment.safe_finding.trim().is_empty()
                || segment.safe_finding.chars().count() > 512
            {
                errors.push(format!(
                    "track segment {} breaks trail continuity",
                    segment.id.0
                ));
            }
            let Some(action) = bound_segments.get(&segment.id).copied() else {
                errors.push(format!(
                    "track segment {} has no owning action",
                    segment.id.0
                ));
                continue;
            };
            if let Some(predecessor_id) = predecessor {
                let predecessor_action = bound_segments.get(predecessor_id).copied();
                if predecessor_action.map(|item| &item.id) != action.prerequisite.as_ref() {
                    errors.push(format!(
                        "{} can skip its preceding track segment",
                        action.id.0
                    ));
                }
            }
            let destinations = action
                .outputs
                .iter()
                .filter_map(|output| match output {
                    GeneratedActionOutput::Destination { stage, site_id } => {
                        Some((*stage, site_id.as_ref()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let is_final = next.is_none();
            let valid_destination = destinations.len() == 1
                && destinations.iter().any(|(stage, site_id)| {
                    if is_final {
                        *stage == GeneratedDestinationStage::Exact
                            && site_id.is_some_and(|site_id| {
                                case.sites
                                    .iter()
                                    .any(|site| site.id == *site_id && site.is_true_location)
                            })
                    } else {
                        *stage == GeneratedDestinationStage::RouteSegment && site_id.is_none()
                    }
                });
            if !valid_destination {
                errors.push(format!(
                    "{} has an invalid destination for its track segment",
                    action.id.0
                ));
            }
        }
    }
}

pub fn validate(case: &GeneratedCase) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if case.catalog_revision != CATALOG_REVISION {
        errors.push("catalog revision mismatch".into());
    }
    let true_sites: Vec<_> = case.sites.iter().filter(|s| s.is_true_location).collect();
    if true_sites.len() != 1 {
        errors.push("case must have exactly one canonical finale location".into());
    }
    validate_track_trails(case, &mut errors);
    let route_classes: BTreeSet<_> = case.actions.iter().map(|a| a.route).collect();
    if route_classes.len() < 2 {
        errors.push("case requires two materially different route classes".into());
    }
    let initial_actions = case
        .actions
        .iter()
        .filter(|action| action.active_initially)
        .collect::<Vec<_>>();
    match case.family {
        TemplateFamily::RecurringDepredation => {
            let valid_contact_entry = initial_actions.first().is_some_and(|entry| {
                let successors = case
                    .actions
                    .iter()
                    .filter(|action| action.prerequisite.as_ref() == Some(&entry.id))
                    .collect::<Vec<_>>();
                initial_actions.len() == 1
                    && entry.kind == InvestigationActionKind::LocateContact
                    && entry.route == RouteClass::PatternSurveillance
                    && entry.target_kind == "contact"
                    && entry.prerequisite.is_none()
                    && case
                        .witnesses
                        .iter()
                        .any(|witness| witness.resident_character_id.to_string() == entry.target_id)
                    && successors.len() == 2
                    && successors.iter().all(|action| !action.active_initially)
                    && successors.iter().any(|action| {
                        action.kind == InvestigationActionKind::ApproachLead
                            && action.route == RouteClass::PhysicalTrail
                            && action.target_kind == "area"
                            && case.areas.iter().any(|area| area.id == action.target_id)
                            && action.alternate
                                == successors
                                    .iter()
                                    .find(|other| {
                                        other.kind == InvestigationActionKind::Watch
                                            && other.route == RouteClass::PatternSurveillance
                                    })
                                    .map_or_else(
                                        || action.alternate.clone(),
                                        |other| other.id.clone(),
                                    )
                    })
                    && successors.iter().any(|action| {
                        action.kind == InvestigationActionKind::Watch
                            && action.route == RouteClass::PatternSurveillance
                            && action.target_kind == "contact"
                            && case.witnesses.iter().any(|witness| {
                                witness.resident_character_id.to_string() == action.target_id
                            })
                            && action.alternate
                                == successors
                                    .iter()
                                    .find(|other| {
                                        other.kind == InvestigationActionKind::ApproachLead
                                            && other.route == RouteClass::PhysicalTrail
                                    })
                                    .map_or_else(
                                        || action.alternate.clone(),
                                        |other| other.id.clone(),
                                    )
                    })
            });
            if !valid_contact_entry {
                errors.push(
                    "recurring cases require one exact contact entry unlocking inactive approach and watch routes"
                        .into(),
                );
            }
        }
        TemplateFamily::DisappearanceOrLoss => {
            let physical = initial_actions.iter().find(|action| {
                action.kind == InvestigationActionKind::SearchArea
                    && action.route == RouteClass::PhysicalTrail
                    && action.target_kind == "area"
                    && action.prerequisite.is_none()
                    && case.areas.iter().any(|area| area.id == action.target_id)
            });
            let social = initial_actions.iter().find(|action| {
                action.kind == InvestigationActionKind::LocateContact
                    && action.route == RouteClass::SocialInquiry
                    && action.target_kind == "contact"
                    && action.prerequisite.is_none()
                    && case.witnesses.iter().any(|witness| {
                        witness.resident_character_id.to_string() == action.target_id
                    })
            });
            if initial_actions.len() != 2
                || physical.is_none()
                || social.is_none()
                || physical.is_some_and(|action| action.alternate != social.unwrap().id)
                || social.is_some_and(|action| action.alternate != physical.unwrap().id)
            {
                errors.push(
                    "disappearance cases require independent physical and witness entry routes"
                        .into(),
                );
            }
        }
        TemplateFamily::Outbreak => {
            let physical = initial_actions.iter().find(|action| {
                action.route == RouteClass::PhysicalTrail
                    && action.kind == InvestigationActionKind::InspectSite
            });
            let social = initial_actions.iter().find(|action| {
                action.route == RouteClass::SocialInquiry
                    && action.kind == InvestigationActionKind::LocateContact
            });
            if initial_actions.len() < 2 || physical.is_none() || social.is_none() {
                errors.push(
                    "outbreak cases require independent physical and non-corpse social routes"
                        .into(),
                );
            }
            let Some(outbreak) = &case.outbreak else {
                errors.push("outbreak family lacks private typed outbreak truth".into());
                return Err(errors);
            };
            if !crate::disease::definition(outbreak.disease).supports(outbreak.transmission_route) {
                errors.push("outbreak disease does not support its transmission route".into());
            }
            if !case
                .sites
                .iter()
                .any(|site| site.id == outbreak.physical_source_site)
            {
                errors.push("outbreak physical source site is not materialized".into());
            }
            if !case.sites.iter().any(|site| {
                site.id == outbreak.patient_presentation_site && site.exact_location_initially_known
            }) {
                errors.push("outbreak patient presentation site is not initially reachable".into());
            }
            let physical_path_is_complete = physical.is_some_and(|inspection| {
                let reachable_root = case.sites.iter().any(|site| {
                    site.id.0 == inspection.target_id && site.exact_location_initially_known
                });
                let observed = inspection.outputs.iter().find_map(|output| match output {
                    GeneratedActionOutput::Evidence { evidence_id } => Some(evidence_id),
                    _ => None,
                });
                let source_lead = inspection.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Destination {
                            stage: GeneratedDestinationStage::Exact,
                            site_id: Some(site_id),
                        } if site_id == &outbreak.physical_source_site
                    )
                });
                reachable_root
                    && source_lead
                    && observed.is_some_and(|evidence_id| {
                        case.evidence.iter().any(|evidence| {
                            &evidence.id == evidence_id
                                && evidence.site_id.0 == inspection.target_id
                        })
                    })
            });
            if !physical_path_is_complete {
                errors.push(
                    "outbreak physical inspection route must start at a known patient/material site and reach evidence plus the exact source lead"
                        .into(),
                );
            }
            let patient_refs = outbreak
                .exposure_chronology
                .iter()
                .map(|exposure| exposure.patient_ref.as_str())
                .collect::<BTreeSet<_>>();
            if outbreak.exposure_chronology.len() < 2
                || patient_refs.len() != outbreak.exposure_chronology.len()
                || outbreak.exposure_chronology.iter().any(|exposure| {
                    let episode = crate::disease::InfectionEpisode {
                        id: exposure.episode_id,
                        character_id: exposure.patient_character_id,
                        disease_id: outbreak.disease,
                        contracted_at: exposure.exposed_at,
                        ruleset_version: crate::physiology::PHYSIOLOGY_RULESET_VERSION,
                        phenotype_key_version: crate::physiology::PHENOTYPE_KEY_VERSION,
                    };
                    let definition = crate::disease::definition(outbreak.disease);
                    let course_end = exposure
                        .exposed_at
                        .saturating_add(definition.incubation_minutes)
                        .saturating_add(definition.rise_minutes)
                        .saturating_add(definition.peak_minutes)
                        .saturating_add(definition.recovery_minutes);
                    let terminal = crate::disease::first_combined_terminal(
                        &[episode],
                        exposure.exposed_at,
                        course_end,
                        0.0,
                    );
                    let death_is_coherent = match exposure.death_kind {
                        Some(OutbreakPatientDeathKind::Disease) => {
                            terminal.map(|value| value.0) == exposure.died_at
                        }
                        Some(OutbreakPatientDeathKind::CarrierAttack) => {
                            matches!(&outbreak.source, OutbreakSource::ThreatVector { .. })
                                && exposure.died_at.is_some_and(|died_at| {
                                    terminal.is_none_or(|(terminal_at, _)| died_at < terminal_at)
                                })
                        }
                        None => exposure.died_at.is_none(),
                    };
                    exposure.patient_ref.is_empty()
                        || exposure.patient_character_id == 0
                        || !case.witnesses.iter().any(|witness| {
                            witness.resident_character_id
                                == exposure.patient_character_id
                        })
                        || exposure.became_symptomatic_at
                            != exposure
                                .exposed_at
                                .saturating_add(definition.incubation_minutes)
                        || !death_is_coherent
                        || exposure.exposed_at > exposure.became_symptomatic_at
                        || exposure
                            .died_at
                            .is_some_and(|died| died < exposure.became_symptomatic_at)
                })
                || outbreak
                    .exposure_chronology
                    .windows(2)
                    .any(|pair| pair[0].exposed_at > pair[1].exposed_at)
            {
                errors.push("outbreak exposure chronology is incoherent".into());
            }
            let fatal_patients = outbreak
                .exposure_chronology
                .iter()
                .filter(|exposure| exposure.died_at.is_some())
                .map(|exposure| exposure.patient_character_id.to_string())
                .collect::<BTreeSet<_>>();
            if social.is_some_and(|action| fatal_patients.contains(&action.target_id)) {
                errors.push("outbreak social route must target a surviving witness".into());
            }
            let source_is_compatible = match (&outbreak.source, outbreak.transmission_route) {
                (
                    OutbreakSource::Sanitation {
                        practice: OutbreakSanitationPractice::UnwashedSharedBedding,
                    },
                    crate::disease::TransmissionVector::CloseContact,
                )
                | (
                    OutbreakSource::Behavior {
                        practice:
                            OutbreakBehaviorPractice::CrowdedSleeping
                            | OutbreakBehaviorPractice::HandlingTheSick
                            | OutbreakBehaviorPractice::ReusingSoiledLinen,
                    },
                    crate::disease::TransmissionVector::CloseContact,
                )
                | (
                    OutbreakSource::Sanitation {
                        practice:
                            OutbreakSanitationPractice::ContaminatedWell
                            | OutbreakSanitationPractice::WasteNearWater
                            | OutbreakSanitationPractice::TaintedFoodStorage,
                    },
                    crate::disease::TransmissionVector::FoodWater,
                )
                | (
                    OutbreakSource::ThreatVector { .. },
                    crate::disease::TransmissionVector::Vermin,
                )
                | (
                    OutbreakSource::Environmental { .. },
                    crate::disease::TransmissionVector::Environmental,
                ) => true,
                _ => false,
            };
            if !source_is_compatible {
                errors.push("outbreak source is incompatible with its transmission route".into());
            }
            let remediation_matches = match (&outbreak.source, &outbreak.remediation) {
                (
                    OutbreakSource::Sanitation {
                        practice: OutbreakSanitationPractice::UnwashedSharedBedding,
                    },
                    OutbreakRemediation::Sanitation {
                        action: OutbreakSanitationAction::LaunderBedding,
                    },
                )
                | (
                    OutbreakSource::Behavior {
                        practice: OutbreakBehaviorPractice::CrowdedSleeping,
                    },
                    OutbreakRemediation::Behavior {
                        action: OutbreakBehaviorAction::SeparateSleepers,
                    },
                ) => true,
                (
                    OutbreakSource::Environmental { reservoir: left },
                    OutbreakRemediation::RemoveEnvironmentalSource { reservoir: right },
                ) if left == right => true,
                (
                    OutbreakSource::ThreatVector { threat },
                    OutbreakRemediation::ResolveCarrierThreat {
                        hostile_group_id,
                        accepted_outcomes,
                    },
                ) => {
                    outbreak.carrier_threat == Some(*threat)
                        && !hostile_group_id.is_empty()
                        && !accepted_outcomes.is_empty()
                        && case.hostile_groups.iter().any(|(id, site, kind, _)| {
                            id == hostile_group_id
                                && site == &outbreak.physical_source_site
                                && kind == threat
                        })
                }
                _ => false,
            };
            if !remediation_matches {
                errors.push("outbreak remediation does not match its exact source".into());
            }
            let direct_remediations = case.actions.iter().filter(|action| {
                action
                    .outputs
                    .iter()
                    .any(|output| matches!(output, GeneratedActionOutput::Remediation { .. }))
            });
            if matches!(
                &outbreak.remediation,
                OutbreakRemediation::ResolveCarrierThreat { .. }
            ) {
                if direct_remediations.count() != 0 {
                    errors.push(
                        "carrier outbreaks must resolve only through accepted hostile outcomes"
                            .into(),
                    );
                }
            } else if !case.actions.iter().any(|action| {
                action
                    .outputs
                    .iter()
                    .any(|output| matches!(output, GeneratedActionOutput::Remediation { .. }))
            }) {
                errors.push("non-carrier outbreak has no direct source intervention".into());
            }
            if case
                .actions
                .iter()
                .any(|action| action.target_kind == "corpse")
            {
                errors.push("outbreak graph has no complete non-corpse route".into());
            }
        }
    }
    if case.family != TemplateFamily::Outbreak && case.outbreak.is_some() {
        errors.push("non-outbreak case carries private outbreak truth".into());
    }
    let action_ids: BTreeSet<_> = case.actions.iter().map(|a| a.id.clone()).collect();
    let mut reachable: BTreeSet<ActionId> = case
        .actions
        .iter()
        .filter(|action| action.active_initially)
        .map(|action| action.id.clone())
        .collect();
    loop {
        let before = reachable.len();
        for action in &case.actions {
            if action
                .prerequisite
                .as_ref()
                .is_some_and(|required| reachable.contains(required))
            {
                reachable.insert(action.id.clone());
            }
        }
        if reachable.len() == before {
            break;
        }
    }
    for action in &case.actions {
        if !action_ids.contains(&action.alternate) {
            errors.push(format!("{} has no recovery route", action.id.0));
        }
        if action
            .prerequisite
            .as_ref()
            .is_some_and(|required| !action_ids.contains(required))
        {
            errors.push(format!("{} has a missing prerequisite", action.id.0));
        }
        if matches!(
            action.kind,
            InvestigationActionKind::FollowTracks | InvestigationActionKind::ReacquireTracks
        ) {
            let coherent = action
                .prerequisite
                .as_ref()
                .and_then(|required| {
                    case.actions
                        .iter()
                        .find(|candidate| candidate.id == *required)
                })
                .is_some_and(|predecessor| {
                    crate::investigation_action::tracking_route_edge_is_coherent(
                        action.kind,
                        &action.target_kind,
                        predecessor.kind,
                        &predecessor.target_kind,
                    )
                });
            if !coherent {
                errors.push(format!(
                    "{} has an incoherent physical tracking predecessor",
                    action.id.0
                ));
            }
        }
        if action.prerequisite.as_ref() == Some(&action.id) {
            errors.push(format!("{} dominates itself", action.id.0));
        }
        if !reachable.contains(&action.id) {
            errors.push(format!(
                "{} is unreachable from a family entry",
                action.id.0
            ));
        }
        let target_exists = match action.target_kind.as_str() {
            "site" => case.sites.iter().any(|site| site.id.0 == action.target_id),
            "area" => case.areas.iter().any(|area| area.id == action.target_id),
            "contact" => case
                .witnesses
                .iter()
                .any(|witness| witness.resident_character_id.to_string() == action.target_id),
            "cohort" => case
                .pattern_targets
                .iter()
                .any(|target| target.cohort_id == action.target_id),
            "route" => case.sites.iter().any(|site| site.id.0 == action.target_id),
            _ => false,
        };
        if !target_exists {
            errors.push(format!(
                "{} references missing {} authority {}",
                action.id.0, action.target_kind, action.target_id
            ));
        }
        for (evidence_id, condition) in action.outputs.iter().filter_map(|output| match output {
            GeneratedActionOutput::PatternCondition {
                evidence_id,
                condition,
            } => Some((evidence_id, condition)),
            _ => None,
        }) {
            if action.active_initially {
                errors.push(format!(
                    "{} exposes a pattern condition before its clue is learned",
                    action.id.0
                ));
            }
            let prerequisite_produces_clue = action.prerequisite.as_ref().is_some_and(|required| {
                case.actions.iter().any(|candidate| {
                    candidate.id == *required
                        && candidate.outputs.iter().any(|output| {
                            matches!(
                                output,
                                GeneratedActionOutput::Evidence { evidence_id: produced }
                                    if produced == evidence_id
                            )
                        })
                })
            });
            if !prerequisite_produces_clue {
                errors.push(format!(
                    "{} does not consume its exact learned pattern clue",
                    action.id.0
                ));
            }
            if !case
                .evidence
                .iter()
                .any(|evidence| evidence.id == *evidence_id)
            {
                errors.push(format!(
                    "{} references missing pattern evidence {}",
                    action.id.0, evidence_id.0
                ));
            }
            if let GeneratedPatternCondition::VictimProfile {
                cohort_id,
                demographic,
                age_band,
                sex,
                profession,
            } = condition
            {
                let exact_target = case.pattern_targets.iter().any(|target| {
                    target.cohort_id == *cohort_id
                        && action.target_kind == "cohort"
                        && action.target_id == target.cohort_id
                        && target.demographic == *demographic
                        && target.age_band == *age_band
                        && target.sex == *sex
                        && target.profession == *profession
                });
                if !exact_target {
                    errors.push(format!(
                        "{} has a victim profile without exact cohort authority",
                        action.id.0
                    ));
                }
            }
        }
    }
    let witness_positions = case
        .witnesses
        .iter()
        .enumerate()
        .map(|(index, witness)| (witness.id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut referral_edges = BTreeMap::<WitnessId, BTreeSet<WitnessId>>::new();
    let mut authored_challenge_responses = BTreeSet::<String>::new();
    for (source_index, witness) in case.witnesses.iter().enumerate() {
        if witness.resident_character_id == 0
            || witness.expected_location.is_empty()
            || witness.expected_location_label.is_empty()
            || witness.visible_description.is_empty()
        {
            errors.push(format!("{} lacks persistent referral data", witness.id.0));
        }
        if witness.testimony.is_empty()
            || !witness
                .testimony
                .iter()
                .any(|draft| draft.delivery == TestimonyDelivery::Volunteered)
        {
            errors.push(format!(
                "{} has no initially visible testimony",
                witness.id.0
            ));
        }
        for draft in &witness.testimony {
            let challenge = draft.challenge_text.as_str();
            let normalized_claim = challenge
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if challenge.is_empty() || challenge != challenge.trim() {
                errors.push(format!(
                    "{} testimony challenge text must be nonempty and already trimmed",
                    witness.id.0
                ));
            } else if draft.spoken_text.match_indices(challenge).count() != 1 {
                errors.push(format!(
                    "{} projected testimony must contain its exact challenge text once",
                    witness.id.0
                ));
            }
            for response in [
                &draft.challenge_responses.charm,
                &draft.challenge_responses.command,
                &draft.challenge_responses.bluff,
            ]
            .into_iter()
            .flatten()
            {
                let normalized = response
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                if response.is_empty() || response != response.trim() {
                    errors.push(format!(
                        "{} has an empty or untrimmed authored challenge response",
                        witness.id.0
                    ));
                } else if !normalized_claim.is_empty() && normalized.contains(&normalized_claim) {
                    errors.push(format!(
                        "{} authored challenge response repeats its claim text",
                        witness.id.0
                    ));
                } else if !authored_challenge_responses.insert(normalized) {
                    errors.push(format!(
                        "{} reuses authored challenge response text",
                        witness.id.0
                    ));
                }
            }
        }
        for draft in witness
            .testimony
            .iter()
            .filter(|draft| draft.delivery == TestimonyDelivery::Withheld)
        {
            if draft.destination_stage != "textual"
                || draft.site_id.is_some()
                || !draft.referred_witness_ids.is_empty()
            {
                errors.push(format!(
                    "{} hides route authority behind a private concern",
                    witness.id.0
                ));
            }
        }
        for referred in witness
            .testimony
            .iter()
            .flat_map(|draft| &draft.referred_witness_ids)
        {
            let Some(target_index) = witness_positions.get(referred) else {
                errors.push(format!(
                    "{} refers to missing witness {}",
                    witness.id.0, referred.0
                ));
                continue;
            };
            if *target_index <= source_index {
                errors.push(format!(
                    "{} has a cyclic or backward witness referral to {}",
                    witness.id.0, referred.0
                ));
            }
            if !referral_edges
                .entry(witness.id.clone())
                .or_default()
                .insert(referred.clone())
            {
                errors.push(format!(
                    "{} repeats witness referral {}",
                    witness.id.0, referred.0
                ));
            }
        }
    }
    if let Some(primary) = case.witnesses.first() {
        let mut reachable = BTreeSet::from([primary.id.clone()]);
        let mut frontier = vec![primary.id.clone()];
        while let Some(source) = frontier.pop() {
            for target in referral_edges.get(&source).into_iter().flatten() {
                if reachable.insert(target.clone()) {
                    frontier.push(target.clone());
                }
            }
        }
        for witness in case.witnesses.iter().skip(1) {
            let route_required = witness
                .testimony
                .iter()
                .any(|draft| draft.corrects_proposition_id.is_some())
                || case.actions.iter().any(|action| {
                    action.target_kind == "contact"
                        && action.target_id == witness.resident_character_id.to_string()
                });
            if route_required && !reachable.contains(&witness.id) {
                errors.push(format!(
                    "{} is not reachable from the primary witness through authored referrals",
                    witness.id.0
                ));
            }
        }
    }
    for target in &case.pattern_targets {
        if target.expected_location.is_empty() || target.expected_location_label.is_empty() {
            errors.push(format!(
                "{} lacks persistent pattern-target location data",
                target.cohort_id
            ));
        }
    }
    for t in &case.factor_trace {
        if t.accepted && t.plausibility > 0 && t.plausibility < 5 && t.required_bridge.is_none() {
            errors.push(format!(
                "rare candidate {} lacks causal bridge",
                t.candidate_id
            ));
        }
        if !t.accepted && t.hard_zero_reason.is_none() {
            errors.push(format!(
                "rejected candidate {} lacks diagnostic",
                t.candidate_id
            ));
        }
    }
    for bridge in &case.bridges {
        if !case
            .canonical_events
            .iter()
            .any(|e| e.id == bridge.event_id)
        {
            errors.push(format!("bridge {} has no event", bridge.id.0));
        }
        if !case.evidence.iter().any(|e| e.id == bridge.evidence_id) {
            errors.push(format!("bridge {} has no evidence authority", bridge.id.0));
        }
        if !reachable.contains(&bridge.action_id)
            || !case.actions.iter().any(|action| {
                action.id == bridge.action_id
                    && action.outputs.iter().any(|output| {
                        matches!(
                            output,
                            GeneratedActionOutput::Evidence { evidence_id }
                                if evidence_id == &bridge.evidence_id
                        )
                    })
            })
        {
            errors.push(format!(
                "bridge {} has no exact reachable evidence output",
                bridge.id.0
            ));
        }
        if bridge.lead_summary.is_empty() {
            errors.push(format!("bridge {} has no lead", bridge.id.0));
        }
    }
    let finale_sites: BTreeSet<_> = case.finales.iter().map(|f| f.site_id.clone()).collect();
    if finale_sites
        .iter()
        .any(|id| !case.sites.iter().any(|s| &s.id == id))
    {
        errors.push("finale references missing site".into());
    }
    let true_site = true_sites.first().map(|site| &site.id);
    for route in &route_classes {
        if !case
            .actions
            .iter()
            .filter(|action| &action.route == route && reachable.contains(&action.id))
            .any(|action| {
                action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Destination {
                            stage: GeneratedDestinationStage::Exact,
                            site_id: Some(site_id),
                        } if Some(site_id) == true_site
                    )
                })
            })
        {
            errors.push(format!("{route:?} has no exact finale-site output"));
        }
    }
    for finale in &case.finales {
        let produced = match finale.kind {
            FinaleKind::Defeat | FinaleKind::DriveOff => case
                .hostile_groups
                .iter()
                .any(|(id, site, _, _)| {
                    finale.hostile_group_id.as_deref() == Some(id) && site == &finale.site_id
                }),
            FinaleKind::Rescue => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| {
                    matches!(
                        output,
                        GeneratedActionOutput::Consequence {
                            consequence: GeneratedActionConsequence::RescueSubject { subject_id, .. }
                        } if finale.subject_id.as_deref() == Some(subject_id)
                    )
                })
            }),
            FinaleKind::RetrieveReturn => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| matches!(
                    output,
                    GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RetrieveAsset { asset_id, .. }
                    } if finale.asset_id.as_deref() == Some(asset_id)
                ))
                    && case.dialogue_producers.iter().any(|producer| {
                        producer.action == GeneratedDialogueAction::ReturnAsset
                            && producer.asset_id.as_deref() == finale.asset_id.as_deref()
                    })
            }),
            FinaleKind::Expose => case.dialogue_producers.iter().any(|producer| {
                producer.action == GeneratedDialogueAction::Expose
                    && producer.subject_ref.as_deref() == finale.subject_id.as_deref()
            }),
            FinaleKind::Negotiate | FinaleKind::Capture => false,
        };
        if !produced {
            errors.push(format!("{:?} has no concrete owning producer", finale.kind));
        }
    }
    let objective_ids: BTreeSet<_> = case
        .objectives
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .map(|objective| objective.id.clone())
        .collect();
    for producer in &case.dialogue_producers {
        if !objective_ids.contains(&producer.objective_id) {
            errors.push(format!(
                "dialogue producer references missing objective {}",
                producer.objective_id.as_str()
            ));
        }
        if producer.recipient_resident_character_id == 0 {
            errors.push("dialogue producer has no recipient".into());
        }
    }
    for objective in case
        .objectives
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let produced = match &objective.requirement {
            ObjectiveRequirement::Defeat {
                hostile_group_id, ..
            }
            | ObjectiveRequirement::DriveOff { hostile_group_id } => case
                .hostile_groups
                .iter()
                .any(|(id, _, _, _)| id == hostile_group_id),
            ObjectiveRequirement::Rescue { subject_id } => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| matches!(
                    output,
                    GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RescueSubject { subject_id: produced, .. }
                    } if produced == subject_id.as_str()
                ))
            }),
            ObjectiveRequirement::Retrieve { asset_id } => case.actions.iter().any(|action| {
                action.outputs.iter().any(|output| matches!(
                    output,
                    GeneratedActionOutput::Consequence {
                        consequence: GeneratedActionConsequence::RetrieveAsset { asset_id: produced, .. }
                    } if produced == asset_id.as_str()
                ))
            }),
            ObjectiveRequirement::Return {
                asset_id,
                custodian_id,
            } => case.dialogue_producers.iter().any(|producer| {
                producer.objective_id == objective.id
                    && producer.action == GeneratedDialogueAction::ReturnAsset
                    && producer.asset_id.as_deref() == Some(asset_id.as_str())
                    && producer.recipient_resident_character_id.to_string() == *custodian_id
            }),
            ObjectiveRequirement::Expose { subject_ref } => {
                case.dialogue_producers.iter().any(|producer| {
                    producer.objective_id == objective.id
                        && producer.action == GeneratedDialogueAction::Expose
                        && producer.subject_ref.as_deref() == Some(subject_ref)
                })
            }
            ObjectiveRequirement::RemediateSource { remediation_id }
                if case.family == TemplateFamily::Outbreak =>
            {
                case.outbreak.as_ref().is_some_and(|outbreak| match &outbreak.remediation {
                    OutbreakRemediation::ResolveCarrierThreat {
                        hostile_group_id,
                        accepted_outcomes,
                    } => {
                        !accepted_outcomes.is_empty()
                            && case.hostile_groups.iter().any(|(id, site, threat, _)| {
                                id == hostile_group_id
                                    && site == &outbreak.physical_source_site
                                    && Some(*threat) == outbreak.carrier_threat
                            })
                    }
                    _ => case.actions.iter().any(|action| {
                        action.outputs.iter().any(|output| {
                            matches!(
                                output,
                                GeneratedActionOutput::Remediation {
                                    remediation_id: produced
                                } if produced == remediation_id
                            )
                        })
                    }),
                })
            }
            ObjectiveRequirement::Surrender { character_id, context_id } => case.actions.iter().any(|action| action.outputs.iter().any(|output| matches!(output, GeneratedActionOutput::SystemicOutcome { outcome: GeneratedSystemicOutcome::Surrender { character_id: produced, context_id: produced_context } } if produced == character_id && produced_context == context_id))),
            ObjectiveRequirement::RecruitOrDefect { character_id, party_id } => case.actions.iter().any(|action| action.outputs.iter().any(|output| matches!(output, GeneratedActionOutput::SystemicOutcome { outcome: GeneratedSystemicOutcome::RecruitOrDefect { character_id: produced, party_id: produced_party } } if produced == character_id && produced_party == party_id))),
            ObjectiveRequirement::Ransom { character_id, recipient_id } => case.actions.iter().any(|action| action.outputs.iter().any(|output| matches!(output, GeneratedActionOutput::SystemicOutcome { outcome: GeneratedSystemicOutcome::Ransom { character_id: produced, recipient_id: produced_recipient } } if produced == character_id && produced_recipient == recipient_id))),
            ObjectiveRequirement::CustodyHandoff { character_id, custodian_id } => case.actions.iter().any(|action| action.outputs.iter().any(|output| matches!(output, GeneratedActionOutput::SystemicOutcome { outcome: GeneratedSystemicOutcome::CustodyHandoff { character_id: produced, custodian_id: produced_custodian } } if produced == character_id && produced_custodian == custodian_id))),
            ObjectiveRequirement::EscapeCustody { character_id } => case.actions.iter().any(|action| action.outputs.iter().any(|output| matches!(output, GeneratedActionOutput::SystemicOutcome { outcome: GeneratedSystemicOutcome::EscapeCustody { character_id: produced } } if produced == character_id))),
            ObjectiveRequirement::TransferOwnership { property_id, owner_id } => case.actions.iter().any(|action| action.outputs.iter().any(|output| matches!(output, GeneratedActionOutput::SystemicOutcome { outcome: GeneratedSystemicOutcome::TransferOwnership { property_id: produced, owner_id: produced_owner } } if produced == property_id && produced_owner == owner_id))),
            ObjectiveRequirement::CommitTheft { property_id, victim_id } => case.actions.iter().any(|action| action.outputs.iter().any(|output| matches!(output, GeneratedActionOutput::SystemicOutcome { outcome: GeneratedSystemicOutcome::Theft { property_id: produced, victim_id: produced_victim } } if produced == property_id && produced_victim == victim_id))),
            _ => false,
        };
        if !produced {
            errors.push(format!(
                "objective {} has no concrete owning producer",
                objective.id.as_str()
            ));
        }
    }
    let expected_finale = match (case.family, case.cause) {
        (TemplateFamily::RecurringDepredation, CanonicalCause::Hostile(_)) => {
            case.finales.iter().all(|finale| {
                matches!(finale.kind, FinaleKind::Defeat | FinaleKind::DriveOff)
                    && finale.hostile_group_id.is_some()
            })
        }
        (
            TemplateFamily::DisappearanceOrLoss,
            CanonicalCause::Hostile(_) | CanonicalCause::ConcealmentByWitness,
        ) => {
            case.finales.len() == 1
                && case.finales[0].kind == FinaleKind::Rescue
                && case.finales[0].subject_id.is_some()
        }
        (TemplateFamily::DisappearanceOrLoss, CanonicalCause::IncidentalLoss) => {
            case.finales.len() == 1
                && case.finales[0].kind == FinaleKind::RetrieveReturn
                && case.finales[0].asset_id.is_some()
        }
        (TemplateFamily::DisappearanceOrLoss, CanonicalCause::FabricatedClaim) => {
            case.finales.len() == 1 && case.finales[0].kind == FinaleKind::Expose
        }
        (TemplateFamily::Outbreak, _) => case.outbreak.is_some() && case.finales.is_empty(),
        _ => false,
    };
    if !expected_finale {
        errors.push("canonical cause is incompatible with generated objective/finale".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
