#[derive(Clone, Copy)]
struct SolvedVariables {
    family: TemplateFamily,
    cause: CanonicalCause,
    site: SiteKind,
    demographic: WitnessDemographic,
    circumstance: Circumstance,
    description: ReportDescription,
    family_bridge: Option<&'static str>,
    cause_bridge: Option<&'static str>,
    site_bridge: Option<&'static str>,
    circumstance_bridge: Option<&'static str>,
    description_bridge: Option<&'static str>,
    primary_witness: usize,
    secondary_witness: usize,
}

fn solve_variables(
    context: &GenerationContext,
    trace: &mut Vec<FactorTrace>,
) -> Result<SolvedVariables, GenerationError> {
    if context.witness_candidates.len() < 2 {
        return Err(GenerationError::InvalidManifest(vec![
            "generation requires two real persistent witness candidates".into(),
        ]));
    }
    if context.witness_candidates.len() > MAX_SOLVER_CANDIDATES
        || context
            .witness_candidates
            .iter()
            .map(|witness| {
                witness.npc_id.len()
                    + witness.profession.len()
                    + witness.visible_description.len()
                    + witness.expected_location.len()
                    + witness.expected_location_label.len()
            })
            .sum::<usize>()
            > 64 * 1024
    {
        return Err(GenerationError::CandidateLimit);
    }
    let families = family_candidates();
    let family_indices = if let Some(requested) = context.requested_family {
        families
            .iter()
            .position(|c| c.value == requested)
            .into_iter()
            .collect()
    } else {
        weighted_order(context.seed, "family", &families)?
    };
    let witnesses = deterministic_witness_order(context);
    let mut visited = 0usize;
    for family_index in family_indices {
        let family = families[family_index].value;
        let causes = cause_candidates(family);
        for cause_index in weighted_order(context.seed.rotate_left(3), "cause", &causes)? {
            let cause = causes[cause_index].value;
            let sites = site_candidates(cause);
            for site_index in weighted_order(context.seed.rotate_left(7), "site", &sites)? {
                let site = sites[site_index].value;
                for &primary_index in &witnesses {
                    let witness = &context.witness_candidates[primary_index];
                    let circumstances = circumstance_candidates(witness.demographic);
                    for circumstance_index in weighted_order(
                        context.seed.rotate_left(19),
                        "circumstance",
                        &circumstances,
                    )? {
                        visited += 1;
                        if visited > MAX_SOLVER_VISITED_NODES {
                            return Err(GenerationError::CandidateLimit);
                        }
                        let circumstance = circumstances[circumstance_index].value;
                        if !witness.allowed_circumstances.contains(&circumstance) {
                            trace.push(FactorTrace {
                                module_id: ModuleId::new("module.circumstance"),
                                relation_id: RelationId::new("relation.circumstance.npc_fact"),
                                factor_ids: vec![FactorId::new("factor.witness.actual_schedule")],
                                candidate_id: format!("{}:{circumstance:?}", witness.npc_id),
                                plausibility: 0,
                                curation: 0,
                                accepted: false,
                                hard_zero_reason: Some(
                                    "persistent NPC facts do not permit this circumstance".into(),
                                ),
                                required_bridge: None,
                                decision: TraceDecision::ForwardRejected,
                            });
                            continue;
                        }
                        let descriptions = description_candidates(cause);
                        let Some(description_index) = weighted_order(
                            context.seed.rotate_left(29),
                            "description",
                            &descriptions,
                        )?
                        .first()
                        .copied() else {
                            trace.push(FactorTrace {
                                module_id: ModuleId::new("module.description"),
                                relation_id: RelationId::new("relation.description.cause"),
                                factor_ids: vec![FactorId::new("factor.description.forward_check")],
                                candidate_id: format!("{cause:?}"),
                                plausibility: 0,
                                curation: 0,
                                accepted: false,
                                hard_zero_reason: Some(
                                    "cause has no possible bestiary report".into(),
                                ),
                                required_bridge: None,
                                decision: TraceDecision::Backtracked,
                            });
                            continue;
                        };
                        let secondary_index = witnesses
                            .iter()
                            .copied()
                            .find(|index| *index != primary_index)
                            .expect("two witnesses checked");
                        for (module, id, bridge_id, factors) in [
                            (
                                "module.template",
                                format!("{family:?}"),
                                families[family_index].bridge,
                                families[family_index].factors.clone(),
                            ),
                            (
                                "module.cause",
                                format!("{cause:?}"),
                                causes[cause_index].bridge,
                                causes[cause_index].factors.clone(),
                            ),
                            (
                                "module.site",
                                format!("{site:?}"),
                                sites[site_index].bridge,
                                sites[site_index].factors.clone(),
                            ),
                            (
                                "module.witness",
                                witness.npc_id.clone(),
                                None,
                                vec!["factor.witness.actual_population"],
                            ),
                            (
                                "module.circumstance",
                                format!("{circumstance:?}"),
                                circumstances[circumstance_index].bridge,
                                circumstances[circumstance_index].factors.clone(),
                            ),
                            (
                                "module.description",
                                format!("{:?}", descriptions[description_index].value),
                                descriptions[description_index].bridge,
                                descriptions[description_index].factors.clone(),
                            ),
                        ] {
                            trace.push(FactorTrace {
                                module_id: ModuleId::new(module),
                                relation_id: RelationId::new("relation.solver.binding"),
                                factor_ids: factors.into_iter().map(FactorId::new).collect(),
                                candidate_id: id,
                                plausibility: 100,
                                curation: 100,
                                accepted: true,
                                hard_zero_reason: None,
                                required_bridge: bridge_id.map(BridgeId::new),
                                decision: TraceDecision::Bound,
                            });
                        }
                        return Ok(SolvedVariables {
                            family,
                            cause,
                            site,
                            demographic: witness.demographic,
                            circumstance,
                            description: descriptions[description_index].value,
                            family_bridge: families[family_index].bridge,
                            cause_bridge: causes[cause_index].bridge,
                            site_bridge: sites[site_index].bridge,
                            circumstance_bridge: circumstances[circumstance_index].bridge,
                            description_bridge: descriptions[description_index].bridge,
                            primary_witness: primary_index,
                            secondary_witness: secondary_index,
                        });
                    }
                    trace.push(FactorTrace {
                        module_id: ModuleId::new("module.witness"),
                        relation_id: RelationId::new("relation.solver.backtrack"),
                        factor_ids: vec![FactorId::new("factor.no_valid_circumstance")],
                        candidate_id: witness.npc_id.clone(),
                        plausibility: 0,
                        curation: 0,
                        accepted: false,
                        hard_zero_reason: Some("witness has no compatible circumstance".into()),
                        required_bridge: None,
                        decision: TraceDecision::Backtracked,
                    });
                }
            }
        }
    }
    Err(GenerationError::NoCandidates {
        module: ModuleId::new("module.quest"),
        diagnostics: trace.clone(),
    })
}

fn family_candidates() -> Vec<Candidate<TemplateFamily>> {
    crate::quest_catalog::catalog()
        .relation("family")
        .expect("validated catalog family relation")
        .candidates
        .iter()
        .map(|candidate| Candidate {
            id: match candidate.id.as_str() {
                "recurring_depredation" => "family.recurring_depredation",
                "disappearance_or_loss" => "family.disappearance_or_loss",
                _ => unreachable!("startup validation rejects unknown family"),
            },
            value: match candidate.id.as_str() {
                "recurring_depredation" => TemplateFamily::RecurringDepredation,
                "disappearance_or_loss" => TemplateFamily::DisappearanceOrLoss,
                _ => unreachable!("startup validation rejects unknown family"),
            },
            weight: Weight::new(candidate.plausibility, candidate.curation),
            bridge: candidate.required_bridge.as_deref(),
            impossible: candidate
                .hard_zero_reason
                .as_deref()
                .map(|_| "catalog-authored hard zero"),
            factors: vec!["factor.family.rotation"],
        })
        .collect()
}

fn cause_candidates(family: TemplateFamily) -> Vec<Candidate<CanonicalCause>> {
    let relation = match family {
        TemplateFamily::RecurringDepredation => "cause.recurring_depredation",
        TemplateFamily::DisappearanceOrLoss => "cause.disappearance_or_loss",
    };
    crate::quest_catalog::catalog()
        .relation(relation)
        .expect("validated catalog cause relation")
        .candidates
        .iter()
        .map(|candidate| {
            let value = match candidate.id.as_str() {
                "concealment" => CanonicalCause::ConcealmentByWitness,
                "incidental_loss" => CanonicalCause::IncidentalLoss,
                "fabricated" => CanonicalCause::FabricatedClaim,
                threat => CanonicalCause::Hostile(
                    threat
                        .parse()
                        .expect("catalog monster has a supported mechanics adapter"),
                ),
            };
            Candidate {
                id: candidate.id.as_str(),
                value,
                weight: Weight::new(candidate.plausibility, candidate.curation),
                bridge: candidate.required_bridge.as_deref(),
                impossible: candidate
                    .hard_zero_reason
                    .as_deref()
                    .map(|_| "catalog-authored hard zero"),
                factors: vec![if matches!(value, CanonicalCause::Hostile(_)) {
                    "factor.cause.bestiary"
                } else {
                    "factor.cause.nonhostile"
                }],
            }
        })
        .collect()
}

fn site_candidates(cause: CanonicalCause) -> Vec<Candidate<SiteKind>> {
    crate::quest_catalog::catalog()
        .documents
        .iter()
        .flat_map(|document| &document.sites)
        .map(|authored_site| {
            let site = SiteKind::try_new(&authored_site.id).expect("validated open site ID");
            let relation_id = match cause {
                CanonicalCause::Hostile(threat) => Some(format!("site.{}", threat.as_str())),
                _ => None,
            };
            let relation = relation_id
                .as_deref()
                .and_then(|id| crate::quest_catalog::catalog().relation(id))
                .and_then(|relation| {
                    relation
                        .candidates
                        .iter()
                        .find(|item| item.id == authored_site.id)
                });
            let natural = match cause {
                CanonicalCause::Hostile(threat) => crate::quest_catalog::catalog()
                    .monster(threat.as_str())
                    .is_some_and(|monster| {
                        monster
                            .investigation
                            .habitats
                            .contains(&authored_site.habitat)
                    }),
                _ => false,
            };
            let p = relation.map_or(if natural { 80 } else { 20 }, |item| item.plausibility);
            let curation = relation.map_or(70, |item| item.curation);
            let impossible = relation
                .and_then(|item| item.hard_zero_reason.as_deref())
                .map(|_| "catalog-authored hard zero");
            let bridge = relation.and_then(|item| item.required_bridge.as_deref());
            Candidate {
                id: authored_site.id.as_str(),
                value: site,
                weight: Weight::new(p, curation),
                bridge,
                impossible,
                factors: vec![if relation.is_some() {
                    "factor.site.catalog_relation"
                } else if natural {
                    "factor.site.catalog_habitat"
                } else {
                    "factor.site.catalog_baseline"
                }],
            }
        })
        .collect()
}

fn secondary_site_candidates(cause: CanonicalCause, primary: SiteKind) -> Vec<Candidate<SiteKind>> {
    site_candidates(cause)
        .into_iter()
        .map(|mut candidate| {
            candidate.factors.push("factor.site.secondary_distinct");
            if candidate.value == primary {
                candidate.weight = Weight::new(0, 0);
                candidate.impossible = Some("secondary site must differ from the finale site");
            }
            candidate
        })
        .collect()
}

fn secondary_circumstance_candidates(
    witness: &WitnessCandidate,
    primary: Circumstance,
) -> Vec<Candidate<Circumstance>> {
    circumstance_candidates(witness.demographic)
        .into_iter()
        .map(|mut candidate| {
            candidate
                .factors
                .push("factor.circumstance.secondary_witness");
            if candidate.value == primary {
                candidate.weight = Weight::new(0, 0);
                candidate.impossible = Some("the corroborating account must arise independently");
            } else if !witness.allowed_circumstances.contains(&candidate.value) {
                candidate.weight = Weight::new(0, 0);
                candidate.impossible = Some("persistent NPC facts do not permit this circumstance");
            }
            candidate
        })
        .collect()
}

fn circumstance_candidates(demo: WitnessDemographic) -> Vec<Candidate<Circumstance>> {
    let relation_id = format!("circumstance.{}", demo.as_str());
    let relation = crate::quest_catalog::catalog().relation(&relation_id);
    crate::quest_catalog::catalog()
        .documents
        .iter()
        .flat_map(|document| &document.circumstances)
        .map(|authored| {
            let circ = Circumstance::try_new(&authored.id).expect("validated open circumstance ID");
            let relation_candidate = relation
                .and_then(|items| items.candidates.iter().find(|item| item.id == authored.id));
            let p = relation_candidate.map_or(35, |item| item.plausibility);
            let curation = relation_candidate.map_or(70, |item| item.curation);
            let impossible = relation_candidate
                .and_then(|item| item.hard_zero_reason.as_deref())
                .map(|_| "catalog-authored hard zero");
            let bridge = relation_candidate.and_then(|item| item.required_bridge.as_deref());
            Candidate {
                id: authored.id.as_str(),
                value: circ,
                weight: Weight::new(p, curation),
                bridge,
                impossible,
                factors: vec!["factor.witness.catalog"],
            }
        })
        .collect()
}

fn description_candidates(cause: CanonicalCause) -> Vec<Candidate<ReportDescription>> {
    crate::quest_catalog::catalog()
        .documents
        .iter()
        .flat_map(|document| &document.descriptions)
        .map(|authored| {
            let report =
                ReportDescription::try_new(&authored.id).expect("validated open description ID");
            let relation_candidate = match cause {
                CanonicalCause::Hostile(threat) => crate::quest_catalog::catalog()
                    .relation(&format!("description.{}", authored.id))
                    .and_then(|relation| {
                        relation
                            .candidates
                            .iter()
                            .find(|candidate| candidate.id == threat.as_str())
                    }),
                _ => None,
            };
            let p = match cause {
                CanonicalCause::Hostile(threat) => description_likelihood(threat, report),
                CanonicalCause::VoluntaryDisappearance
                | CanonicalCause::ConcealmentByWitness
                | CanonicalCause::FabricatedClaim => {
                    if matches!(
                        report,
                        ReportDescription::ArmedPeople | ReportDescription::UnseenNightVisitor
                    ) {
                        55
                    } else {
                        5
                    }
                }
                CanonicalCause::IncidentalLoss => {
                    if report == ReportDescription::UnseenNightVisitor {
                        60
                    } else {
                        3
                    }
                }
            };
            Candidate {
                id: authored.id.as_str(),
                value: report,
                weight: Weight::new(
                    p,
                    relation_candidate.map_or(80, |candidate| candidate.curation),
                ),
                bridge: relation_candidate
                    .and_then(|candidate| candidate.required_bridge.as_deref()),
                impossible: relation_candidate
                    .and_then(|candidate| candidate.hard_zero_reason.as_deref())
                    .map(|_| "catalog-authored hard zero")
                    .or_else(|| {
                        (p == 0).then_some("bestiary forward description likelihood is zero")
                    }),
                factors: vec!["factor.description.bestiary_forward_likelihood"],
            }
        })
        .collect()
}

fn report_id(v: ReportDescription) -> &'static str {
    crate::quest_catalog::catalog()
        .documents
        .iter()
        .flat_map(|document| &document.descriptions)
        .find(|item| item.id == v.as_str())
        .expect("generated description exists")
        .id
        .as_str()
}

fn ambiguous_report_description(v: ReportDescription) -> &'static str {
    crate::quest_catalog::catalog()
        .description(report_catalog_id(v))
        .expect("validated description catalog covers closed mechanics adapter")
        .text
        .as_str()
}

#[cfg(test)]
fn ambiguous_visual_claim(v: ReportDescription, place: &str) -> String {
    format!(
        "It looked like {}, near {place}.",
        ambiguous_report_description(v)
    )
}

fn terrain(site: SiteKind) -> Terrain {
    match crate::quest_catalog::catalog()
        .site(site_catalog_id(site))
        .expect("validated site catalog covers closed mechanics adapter")
        .terrain
        .as_str()
    {
        "underground" => Terrain::Underground,
        "forest" => Terrain::Forest,
        "settlement" => Terrain::Settlement,
        "road" => Terrain::Road,
        _ => unreachable!("startup validation rejects unknown terrain"),
    }
}
fn label(site: SiteKind) -> &'static str {
    crate::quest_catalog::catalog()
        .site(site_catalog_id(site))
        .expect("validated site catalog covers closed mechanics adapter")
        .label
        .as_str()
}

fn site_catalog_id(site: SiteKind) -> &'static str {
    crate::quest_catalog::catalog()
        .site(site.as_str())
        .expect("generated site identity exists in catalog")
        .id
        .as_str()
}

fn report_catalog_id(report: ReportDescription) -> &'static str {
    report_id(report)
}

fn bridge(id: &str, prefix: &str, family: TemplateFamily, _now: u64) -> CausalBridge {
    let catalog_id = id.trim_start_matches("bridge.");
    let authored = crate::quest_catalog::catalog()
        .bridge(catalog_id)
        .expect("validated bridge reference");
    let family_id = match family {
        TemplateFamily::RecurringDepredation => "recurring_depredation",
        TemplateFamily::DisappearanceOrLoss => "disappearance_or_loss",
    };
    let action_id = authored
        .action_ids
        .get(family_id)
        .expect("validated bridge action coverage");
    CausalBridge {
        id: BridgeId::new(id),
        explanation: authored.explanation.clone(),
        event_id: scoped_id(prefix, "event", &authored.event_suffix),
        evidence_id: EvidenceId::new(scoped_id(prefix, "evidence", &authored.evidence_id)),
        action_id: ActionId::new(scoped_id(prefix, "action", action_id)),
        lead_summary: authored.lead_summary.clone(),
    }
}

fn consequence(
    cause: CanonicalCause,
    template: &crate::quest_catalog::TemplateDefinition,
) -> ConsequenceProfile {
    let cause_id = match cause {
        CanonicalCause::Hostile(threat) => threat.as_str().to_owned(),
        CanonicalCause::VoluntaryDisappearance => "voluntary_disappearance".into(),
        CanonicalCause::ConcealmentByWitness => "concealment".into(),
        CanonicalCause::IncidentalLoss => "incidental_loss".into(),
        CanonicalCause::FabricatedClaim => "fabricated".into(),
    };
    let authored = crate::quest_catalog::catalog()
        .consequence(&template.consequence_profile, &cause_id)
        .expect("validated consequence coverage");
    ConsequenceProfile {
        symptom: match authored.symptom.as_str() {
            "night_screams" => Symptom::NightScreams,
            "vanished_livestock" => Symptom::VanishedLivestock,
            "missing_caravans" => Symptom::MissingCaravans,
            "empty_stalls" => Symptom::EmptyStalls,
            _ => unreachable!("validated consequence symptom"),
        },
        effects: Effects {
            buy_bps: authored.buy_bps,
            sell_penalty_bps: authored.sell_penalty_bps,
            encounter_frequency_bps: authored.encounter_frequency_bps,
            encounter_archetype: authored.encounter_archetype.as_deref().map(|id| match id {
                "undead" => EncounterArchetype::Undead,
                "goblins" => EncounterArchetype::Goblins,
                "bandits" => EncounterArchetype::Bandits,
                _ => unreachable!("validated encounter archetype"),
            }),
            disease_intensity: authored.disease_intensity,
        },
        public_summary: authored.public_summary.clone(),
    }
}

fn deterministic_witness_order(context: &GenerationContext) -> Vec<usize> {
    let mut indices = (0..context.witness_candidates.len()).collect::<Vec<_>>();
    indices.sort_by_key(|index| {
        hash(
            context.seed,
            &format!("witness:{}", context.witness_candidates[*index].npc_id),
        )
    });
    indices
}

fn build_actions(
    prefix: &str,
    family: TemplateFamily,
    finale: &SiteId,
    area_id: &str,
    witness_npc_id: &str,
    route_variant: RouteVariant,
    attack_pattern: AttackPattern,
    victim_target: Option<&GeneratedPatternTarget>,
    pattern_evidence_id: &EvidenceId,
    track_segments: &[TrackSegment],
) -> Vec<GeneratedAction> {
    let early_tracking_summary = match route_variant {
        RouteVariant::Direct => "Read the clearest section of the physical trail.",
        RouteVariant::Cautious => "Recover the trail cautiously across the changed ground.",
    };
    let [first_segment, final_segment] = track_segments else {
        unreachable!("generated physical trails always have two segments")
    };
    let pattern_condition = match attack_pattern {
        AttackPattern::Nightly => GeneratedPatternCondition::NightWindow,
        AttackPattern::Roadside => GeneratedPatternCondition::RoadRoute,
        AttackPattern::VictimSpecific => {
            let target = victim_target.expect("victim-specific pattern has a bound cohort");
            GeneratedPatternCondition::VictimProfile {
                cohort_id: target.cohort_id.clone(),
                demographic: target.demographic,
                age_band: target.age_band.clone(),
                sex: target.sex.clone(),
                profession: target.profession.clone(),
            }
        }
        AttackPattern::Irregular => GeneratedPatternCondition::BroadSurvey,
    };
    let pattern_action_summary = match attack_pattern {
        AttackPattern::Nightly => "Patrol during the learned nighttime window.".into(),
        AttackPattern::Roadside => "Patrol the learned roadside route.".into(),
        AttackPattern::VictimSpecific => {
            let target = victim_target.expect("victim-specific pattern has a bound cohort");
            format!(
                "Watch potential victims connected with the {} trade near the learned location.",
                target.profession
            )
        }
        AttackPattern::Irregular => {
            "Search broadly because the accounts reveal no reliable schedule.".into()
        }
    };
    let pattern_action_kind = if attack_pattern == AttackPattern::Irregular {
        InvestigationActionKind::SearchArea
    } else {
        InvestigationActionKind::Patrol
    };
    let pattern_target_kind = match attack_pattern {
        AttackPattern::Roadside => "route",
        AttackPattern::VictimSpecific => "cohort",
        _ => "area",
    };
    let pattern_target_id = match attack_pattern {
        AttackPattern::Roadside => finale.0.clone(),
        AttackPattern::VictimSpecific => victim_target
            .expect("victim-specific pattern has a bound cohort")
            .cohort_id
            .clone(),
        _ => area_id.into(),
    };
    let make = |name: &str,
                kind,
                route,
                target_kind: &str,
                target: String,
                prerequisite: Option<&str>,
                alternate: &str,
                active,
                summary: &str,
                outputs: Vec<GeneratedActionOutput>| GeneratedAction {
        id: ActionId::new(scoped_id(prefix, "action", name)),
        kind,
        route,
        target_kind: target_kind.into(),
        target_id: target,
        prerequisite: prerequisite.map(|p| ActionId::new(scoped_id(prefix, "action", p))),
        alternate: ActionId::new(scoped_id(prefix, "action", alternate)),
        active_initially: active,
        safe_summary: summary.into(),
        track_segment_id: None,
        outputs,
    };
    let mut actions = match family {
        TemplateFamily::RecurringDepredation => vec![
            make(
                "locate_contact",
                InvestigationActionKind::LocateContact,
                RouteClass::PatternSurveillance,
                "contact",
                witness_npc_id.into(),
                None,
                "approach",
                true,
                "Find the referred witness.",
                vec![],
            ),
            make(
                "approach",
                InvestigationActionKind::ApproachLead,
                RouteClass::PhysicalTrail,
                "area",
                area_id.into(),
                Some("locate_contact"),
                "watch",
                false,
                "Approach the last reported incident.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::ApproximateArea,
                    site_id: None,
                }],
            ),
            make(
                "search",
                InvestigationActionKind::SearchArea,
                RouteClass::PhysicalTrail,
                "area",
                area_id.into(),
                Some("approach"),
                "patrol",
                false,
                "Search for physical traces.",
                vec![GeneratedActionOutput::Evidence {
                    evidence_id: EvidenceId::new(scoped_id(prefix, "evidence", "tracks")),
                }],
            ),
            make(
                "reacquire",
                InvestigationActionKind::ReacquireTracks,
                RouteClass::PhysicalTrail,
                "route",
                finale.0.clone(),
                Some("search"),
                "reveal_route",
                false,
                early_tracking_summary,
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::RouteSegment,
                    site_id: None,
                }],
            ),
            make(
                "follow",
                InvestigationActionKind::FollowTracks,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("reacquire"),
                "reveal_route",
                false,
                "Follow the recovered trail to its source.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    site_id: Some(finale.clone()),
                }],
            ),
            make(
                "inspect_finale",
                InvestigationActionKind::InspectSite,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("follow"),
                "ambush",
                false,
                "Inspect the located lair from the occupied site.",
                vec![GeneratedActionOutput::AmbushReady],
            ),
            make(
                "watch",
                InvestigationActionKind::Watch,
                RouteClass::PatternSurveillance,
                "contact",
                witness_npc_id.into(),
                Some("locate_contact"),
                "approach",
                false,
                "Watch where incidents recur.",
                vec![
                    GeneratedActionOutput::Destination {
                        stage: GeneratedDestinationStage::Textual,
                        site_id: None,
                    },
                    GeneratedActionOutput::Evidence {
                        evidence_id: pattern_evidence_id.clone(),
                    },
                ],
            ),
            make(
                "patrol",
                pattern_action_kind,
                RouteClass::PatternSurveillance,
                pattern_target_kind,
                pattern_target_id.clone(),
                Some("watch"),
                "search",
                false,
                &pattern_action_summary,
                vec![
                    GeneratedActionOutput::PatternCondition {
                        evidence_id: pattern_evidence_id.clone(),
                        condition: pattern_condition.clone(),
                    },
                    GeneratedActionOutput::Destination {
                        stage: GeneratedDestinationStage::RouteSegment,
                        site_id: Some(finale.clone()),
                    },
                ],
            ),
            make(
                "reveal_route",
                InvestigationActionKind::ApproachLead,
                RouteClass::PatternSurveillance,
                "route",
                finale.0.clone(),
                Some("patrol"),
                "follow",
                false,
                "Approach the site along the learned route.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    site_id: Some(finale.clone()),
                }],
            ),
            make(
                "ambush",
                InvestigationActionKind::LayAmbush,
                RouteClass::PatternSurveillance,
                "site",
                finale.0.clone(),
                Some("reveal_route"),
                "inspect_finale",
                false,
                "Lay an ambush after occupying the located site.",
                vec![GeneratedActionOutput::AmbushReady],
            ),
        ],
        TemplateFamily::DisappearanceOrLoss => vec![
            make(
                "inspect_last_known",
                InvestigationActionKind::SearchArea,
                RouteClass::PhysicalTrail,
                "area",
                area_id.into(),
                None,
                "locate_contact",
                true,
                "Inspect the last-known place.",
                vec![GeneratedActionOutput::Evidence {
                    evidence_id: EvidenceId::new(scoped_id(prefix, "evidence", "tracks")),
                }],
            ),
            make(
                "resolve_physical",
                InvestigationActionKind::InspectSite,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("follow"),
                "resolve_social",
                false,
                "Inspect the located site and recover what is actually there.",
                vec![],
            ),
            make(
                "reacquire",
                InvestigationActionKind::ReacquireTracks,
                RouteClass::PhysicalTrail,
                "route",
                finale.0.clone(),
                Some("inspect_last_known"),
                "approach_social",
                false,
                early_tracking_summary,
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::RouteSegment,
                    site_id: None,
                }],
            ),
            make(
                "follow",
                InvestigationActionKind::FollowTracks,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("reacquire"),
                "approach_social",
                false,
                "Follow the recovered trail to its source.",
                vec![GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    site_id: Some(finale.clone()),
                }],
            ),
            make(
                "locate_contact",
                InvestigationActionKind::LocateContact,
                RouteClass::SocialInquiry,
                "contact",
                witness_npc_id.into(),
                None,
                "inspect_last_known",
                true,
                "Find the referred witness.",
                vec![
                    GeneratedActionOutput::Destination {
                        stage: GeneratedDestinationStage::ApproximateArea,
                        site_id: None,
                    },
                    GeneratedActionOutput::Evidence {
                        evidence_id: pattern_evidence_id.clone(),
                    },
                ],
            ),
            make(
                "approach_social",
                pattern_action_kind,
                RouteClass::SocialInquiry,
                pattern_target_kind,
                pattern_target_id,
                Some("locate_contact"),
                "follow",
                false,
                &pattern_action_summary,
                vec![
                    GeneratedActionOutput::PatternCondition {
                        evidence_id: pattern_evidence_id.clone(),
                        condition: pattern_condition,
                    },
                    GeneratedActionOutput::Destination {
                        stage: GeneratedDestinationStage::Exact,
                        site_id: Some(finale.clone()),
                    },
                ],
            ),
            make(
                "resolve_social",
                InvestigationActionKind::InspectSite,
                RouteClass::SocialInquiry,
                "site",
                finale.0.clone(),
                Some("approach_social"),
                "resolve_physical",
                false,
                "Enter the located site and resolve the social lead.",
                vec![],
            ),
        ],
    };
    let early = actions
        .iter_mut()
        .find(|action| action.id == ActionId::new(scoped_id(prefix, "action", "reacquire")))
        .expect("physical trail has an early segment action");
    early.track_segment_id = Some(first_segment.id.clone());
    early.outputs.push(GeneratedActionOutput::TrackFinding {
        segment_id: first_segment.id.clone(),
        finding: first_segment.safe_finding.clone(),
    });
    let final_action = actions
        .iter_mut()
        .find(|action| action.id == ActionId::new(scoped_id(prefix, "action", "follow")))
        .expect("physical trail has a final segment action");
    final_action.track_segment_id = Some(final_segment.id.clone());
    final_action
        .outputs
        .push(GeneratedActionOutput::TrackFinding {
            segment_id: final_segment.id.clone(),
            finding: final_segment.safe_finding.clone(),
        });
    actions
}
