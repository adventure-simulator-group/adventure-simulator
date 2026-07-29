pub fn generated_testimony_pipeline(
    context: &GenerationContext,
    character_id: u64,
    generated: &GeneratedCase,
    witness: &WitnessBinding,
    index: usize,
    received_at: u64,
) -> Result<(String, crate::investigation::PipelineInput), crate::investigation::ValidationError> {
    use crate::investigation::{
        AtomicProposition, CaseId, DisclosureMode, EventId, MemoryCondition, PerceptionCondition,
        PipelineInput, PropositionId, TransmissionCondition,
    };
    let draft = witness
        .testimony
        .get(index)
        .ok_or(crate::investigation::ValidationError::InvalidId)?;
    let receipt_id = observer_scoped_id(
        context,
        "testimony",
        &format!("{character_id}:{}:{index}", witness.id.0),
    );
    let pipeline = PipelineInput {
        case_id: CaseId::new(&generated.canonical_case_id)?,
        event_id: EventId::new(crate::investigation::compound_id(&[
            "event",
            &witness.id.0,
            &index.to_string(),
        ]))?,
        proposition: AtomicProposition::new(
            PropositionId::new(draft.proposition_id.clone())?,
            &witness.npc_id,
            "reported",
            &draft.truthful_text,
        )?,
        observer_ref: witness.npc_id.clone(),
        speaker_ref: witness.npc_id.clone(),
        receipt_identity: receipt_id.clone(),
        recollection_revision: 1,
        perceived_text: draft.truthful_text.clone(),
        recalled_text: match draft.reliability {
            Reliability::Truthful | Reliability::PartlyTruthful => draft.truthful_text.clone(),
            _ => draft.spoken_text.clone(),
        },
        disclosed_text: Some(draft.spoken_text.clone()),
        transmitted_text: draft.spoken_text.clone(),
        perception: match context.incident_weather {
            crate::weather::Precipitation::Clear => {
                if matches!(witness.circumstance, Circumstance::NightWindow) {
                    PerceptionCondition::Darkness
                } else {
                    PerceptionCondition::Clear
                }
            }
            crate::weather::Precipitation::Rain | crate::weather::Precipitation::Snow => {
                PerceptionCondition::PoorPerception
            }
        },
        memory: if draft.reliability == Reliability::Mistaken {
            MemoryCondition::Confused
        } else {
            MemoryCondition::Accurate
        },
        disclosure: match draft.reliability {
            Reliability::Deceptive => DisclosureMode::Distort,
            Reliability::Evasive => DisclosureMode::Conceal,
            _ => DisclosureMode::Disclose,
        },
        transmission: TransmissionCondition::Clear,
        received_at,
    };
    Ok((receipt_id, pipeline))
}

fn choose<T: Copy>(
    seed: u64,
    module: &str,
    relation: &str,
    candidates: &[Candidate<T>],
    trace: &mut Vec<FactorTrace>,
) -> Result<(T, Option<&'static str>), GenerationError> {
    if candidates.len() > MAX_SOLVER_CANDIDATES {
        return Err(GenerationError::CandidateLimit);
    }
    let mut total = 0u64;
    for c in candidates {
        let accepted = c.impossible.is_none() && c.weight.combined() > 0;
        trace.push(FactorTrace {
            module_id: ModuleId::new(module),
            relation_id: RelationId::new(relation),
            factor_ids: c.factors.iter().map(|f| FactorId::new(*f)).collect(),
            candidate_id: c.id.into(),
            plausibility: c.weight.plausibility,
            curation: c.weight.curation,
            accepted,
            hard_zero_reason: c.impossible.map(str::to_owned),
            required_bridge: c.bridge.map(BridgeId::new),
            decision: TraceDecision::Candidate,
        });
        if accepted {
            total = total.saturating_add(c.weight.combined());
        }
    }
    if total == 0 {
        return Err(GenerationError::NoCandidates {
            module: ModuleId::new(module),
            diagnostics: trace.clone(),
        });
    }
    let mut draw = hash(seed, module) % total;
    for c in candidates {
        let weight = if c.impossible.is_none() {
            c.weight.combined()
        } else {
            0
        };
        if draw < weight {
            return Ok((c.value, c.bridge));
        }
        draw -= weight;
    }
    unreachable!("bounded weighted draw must select")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountStyle {
    VisualClaim,
    HeardOnly,
    TracksAndMovement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RouteVariant {
    Direct,
    Cautious,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttackPattern {
    Nightly,
    Roadside,
    VictimSpecific,
    Irregular,
}

fn reliability_candidates(
    demographic: WitnessDemographic,
    circumstance: Circumstance,
    cause: CanonicalCause,
) -> Vec<Candidate<Reliability>> {
    let catalog = crate::quest_catalog::catalog();
    let baseline = catalog.relation("reliability.baseline").unwrap();
    baseline
        .candidates
        .iter()
        .map(|base| {
            let value = match base.id.as_str() {
                "truthful" => Reliability::Truthful,
                "mistaken" => Reliability::Mistaken,
                "evasive" => Reliability::Evasive,
                "deceptive" => Reliability::Deceptive,
                "partly_truthful" => Reliability::PartlyTruthful,
                _ => unreachable!("validated reliability mechanic"),
            };
            let contextual = [
                (
                    demographic == WitnessDemographic::Child,
                    "reliability.child",
                ),
                (
                    matches!(
                        circumstance,
                        Circumstance::SecretRiversideMeeting | Circumstance::AdultVenue
                    ),
                    "reliability.embarrassing_context",
                ),
                (
                    cause == CanonicalCause::FabricatedClaim,
                    "reliability.fabricated_claim",
                ),
                (
                    circumstance == Circumstance::NightWindow,
                    "reliability.night_window",
                ),
            ]
            .into_iter()
            .filter(|(applies, _)| *applies)
            .filter_map(|(_, id)| catalog.relation(id))
            .find_map(|relation| relation.candidates.iter().find(|item| item.id == base.id));
            let authored = contextual.unwrap_or(base);
            Candidate {
                id: base.id.as_str(),
                value,
                weight: Weight::new(authored.plausibility, authored.curation),
                bridge: authored.required_bridge.as_deref(),
                impossible: authored
                    .hard_zero_reason
                    .as_deref()
                    .map(|_| "catalog-authored hard zero"),
                factors: vec!["factor.reliability.catalog"],
            }
        })
        .collect()
}

fn evidence_candidates(cause: CanonicalCause, site: SiteKind) -> Vec<Candidate<EvidenceKind>> {
    let baseline = crate::quest_catalog::catalog()
        .relation("evidence.baseline")
        .expect("validated evidence relation");
    baseline
        .candidates
        .iter()
        .map(|authored| {
            let value = evidence_kind_from_catalog(&authored.id);
            let catalog = crate::quest_catalog::catalog();
            let relation_ids = [
                matches!(cause, CanonicalCause::Hostile(threat) if threat == ThreatId::Skeleton)
                    .then_some("evidence.skeleton"),
                (cause == CanonicalCause::IncidentalLoss).then_some("evidence.incidental_loss"),
                (cause == CanonicalCause::FabricatedClaim).then_some("evidence.fabricated_claim"),
                matches!(site, SiteKind::Roadside | SiteKind::Riverside)
                    .then_some("evidence.trackable_ground"),
            ];
            let selected = relation_ids
                .into_iter()
                .flatten()
                .filter_map(|id| catalog.relation(id))
                .find_map(|relation| {
                    relation
                        .candidates
                        .iter()
                        .find(|item| item.id == authored.id)
                })
                .unwrap_or(authored);
            Candidate {
                id: authored.id.as_str(),
                value,
                weight: Weight::new(selected.plausibility, selected.curation),
                bridge: selected.required_bridge.as_deref(),
                impossible: selected
                    .hard_zero_reason
                    .as_deref()
                    .map(|_| "catalog-authored hard zero"),
                factors: vec!["factor.evidence.catalog"],
            }
        })
        .collect()
}

fn evidence_kind_from_catalog(id: &str) -> EvidenceKind {
    EvidenceKind::try_new(id).expect("validated open evidence ID")
}

fn evidence_presentation(
    kind: EvidenceKind,
    evidence_id: &EvidenceId,
    investigability: u8,
) -> (String, String, String, Vec<EvidenceInspectionTopic>) {
    let definition = crate::quest_catalog::catalog()
        .evidence(evidence_catalog_id(kind))
        .expect("validated evidence catalog covers closed mechanics adapter");
    let topics = definition
        .topics
        .iter()
        .map(|topic| {
            let check = topic.check.as_ref().map(|check| {
                let stat = match check.stat.as_str() {
                    "eyesight" => EvidenceCheckStat::Eyesight,
                    "intelligence" => EvidenceCheckStat::Intelligence,
                    "instinct" => EvidenceCheckStat::Instinct,
                    _ => unreachable!("startup validation rejects unknown evidence stats"),
                };
                let width = u64::from(check.difficulty_max_milli - check.difficulty_min_milli) + 1;
                EvidenceInspectionCheck {
                    stat,
                    difficulty_milli: crate::threat_escalation::adjusted_difficulty_milli(
                        check.difficulty_min_milli
                            + (crate::settlement_population::stable_hash(&format!(
                                "{}:{}",
                                evidence_id.0, topic.id
                            )) % width) as u16,
                        investigability,
                    ),
                    success_description: check.success_description.clone(),
                    reveals_clue: check.reveals_clue,
                }
            });
            EvidenceInspectionTopic {
                id: topic.id.clone(),
                label: topic.label.clone(),
                inspection_description: topic.inspection_description.clone(),
                check,
                bestiary: topic
                    .bestiary
                    .iter()
                    .map(|implication| BestiaryEvidenceImplication {
                        category: implication.category,
                        lore_difficulty_milli: crate::threat_escalation::adjusted_difficulty_milli(
                            implication.lore_difficulty_milli,
                            investigability,
                        ),
                        diagnostic_kind: implication.diagnostic_kind.clone(),
                        interpretation: implication.interpretation.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    (
        definition.portrait_label.clone(),
        definition.portrait_icon.clone(),
        definition.base_description.clone(),
        topics,
    )
}

fn evidence_catalog_id(kind: EvidenceKind) -> &'static str {
    crate::quest_catalog::catalog()
        .evidence(kind.as_str())
        .expect("generated evidence identity exists in catalog")
        .id
        .as_str()
}

fn evidence_reference(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Footprints => "some footprints",
        EvidenceKind::ClothScrap => "a piece of torn cloth",
        EvidenceKind::BoneDust => "some bone dust",
        EvidenceKind::BloodlessCorpse => "a bloodless corpse",
        EvidenceKind::DroppedToken => "a dropped token",
        EvidenceKind::DragMarks => "some drag marks",
        EvidenceKind::LedgerEntry => "a ledger",
        _ => crate::quest_catalog::catalog()
            .evidence(kind.as_str())
            .expect("generated evidence exists")
            .portrait_label
            .as_str(),
    }
}

fn generated_evidence(
    id: EvidenceId,
    kind: EvidenceKind,
    proposition_id: String,
    site_id: SiteId,
    safe_description: String,
    corrects_proposition_id: Option<String>,
    investigability: u8,
) -> GeneratedEvidence {
    let (portrait_label, portrait_icon, base_description, inspection_topics) =
        evidence_presentation(kind, &id, investigability);
    GeneratedEvidence {
        id,
        kind,
        proposition_id,
        site_id,
        portrait_label,
        portrait_icon,
        base_description,
        inspection_topics,
        safe_description,
        corrects_proposition_id,
    }
}

/// Reuse the canonical cause/site likelihood table when a continuing case
/// produces fresh evidence after the initial manifest was written.
pub fn select_follow_up_evidence(
    cause: CanonicalCause,
    site: SiteKind,
    entropy: u64,
) -> Option<EvidenceKind> {
    let candidates: Vec<_> = evidence_candidates(cause, site)
        .into_iter()
        .filter(|candidate| candidate.impossible.is_none() && candidate.weight.combined() > 0)
        .collect();
    let total = candidates
        .iter()
        .map(|candidate| candidate.weight.combined())
        .sum::<u64>();
    if total == 0 {
        return None;
    }
    let mut draw = entropy % total;
    candidates.into_iter().find_map(|candidate| {
        let weight = candidate.weight.combined();
        if draw < weight {
            Some(candidate.value)
        } else {
            draw -= weight;
            None
        }
    })
}

fn account_style_candidates(
    _reliability: Reliability,
    circumstance: Circumstance,
) -> Vec<Candidate<AccountStyle>> {
    // Account presentation is deliberately independent of hidden reliability.
    // A source-aware player must not be able to classify a witness from the
    // wording family selected for their account.
    let relation = crate::quest_catalog::catalog()
        .relation(if circumstance == Circumstance::NightWindow {
            "account.night_window"
        } else {
            "account.baseline"
        })
        .unwrap();
    relation
        .candidates
        .iter()
        .map(|authored| {
            let value = match authored.id.as_str() {
                "visual" => AccountStyle::VisualClaim,
                "heard" => AccountStyle::HeardOnly,
                "tracks" => AccountStyle::TracksAndMovement,
                _ => unreachable!("validated account mechanic"),
            };
            Candidate {
                id: authored.id.as_str(),
                value,
                weight: Weight::new(authored.plausibility, authored.curation),
                bridge: authored.required_bridge.as_deref(),
                impossible: authored
                    .hard_zero_reason
                    .as_deref()
                    .map(|_| "catalog-authored hard zero"),
                factors: vec!["factor.account.catalog"],
            }
        })
        .collect()
}

fn route_variant_candidates(family: TemplateFamily) -> [Candidate<RouteVariant>; 2] {
    let relation = crate::quest_catalog::catalog()
        .relation(match family {
            TemplateFamily::RecurringDepredation => "route.recurring_depredation",
            TemplateFamily::DisappearanceOrLoss => "route.disappearance_or_loss",
        })
        .expect("validated route relation");
    let authored = |id: &str| {
        relation
            .candidates
            .iter()
            .find(|item| item.id == id)
            .unwrap()
    };
    let direct = authored("direct");
    let cautious = authored("cautious");
    [
        Candidate {
            id: "route.direct",
            value: RouteVariant::Direct,
            weight: Weight::new(direct.plausibility, direct.curation),
            bridge: direct.required_bridge.as_deref(),
            impossible: direct
                .hard_zero_reason
                .as_deref()
                .map(|_| "catalog-authored hard zero"),
            factors: vec!["factor.route.direct"],
        },
        Candidate {
            id: "route.cautious",
            value: RouteVariant::Cautious,
            weight: Weight::new(cautious.plausibility, cautious.curation),
            bridge: cautious.required_bridge.as_deref(),
            impossible: cautious
                .hard_zero_reason
                .as_deref()
                .map(|_| "catalog-authored hard zero"),
            factors: vec!["factor.route.cautious"],
        },
    ]
}

fn attack_pattern_candidates(
    family: TemplateFamily,
    has_victim_target: bool,
) -> [Candidate<AttackPattern>; 4] {
    let relation = crate::quest_catalog::catalog()
        .relation(match family {
            TemplateFamily::RecurringDepredation => "pattern.recurring_depredation",
            TemplateFamily::DisappearanceOrLoss => "pattern.disappearance_or_loss",
        })
        .expect("validated pattern relation");
    let authored = |id: &str| {
        relation
            .candidates
            .iter()
            .find(|item| item.id == id)
            .unwrap()
    };
    let nightly = authored("nightly");
    let roadside = authored("roadside");
    let victim = authored("victim_specific");
    let irregular = authored("irregular");
    [
        Candidate {
            id: "pattern.nightly",
            value: AttackPattern::Nightly,
            weight: Weight::new(nightly.plausibility, nightly.curation),
            bridge: nightly.required_bridge.as_deref(),
            impossible: nightly
                .hard_zero_reason
                .as_deref()
                .map(|_| "catalog-authored hard zero"),
            factors: vec!["factor.pattern.nightly"],
        },
        Candidate {
            id: "pattern.roadside",
            value: AttackPattern::Roadside,
            weight: Weight::new(roadside.plausibility, roadside.curation),
            bridge: roadside.required_bridge.as_deref(),
            impossible: roadside
                .hard_zero_reason
                .as_deref()
                .map(|_| "catalog-authored hard zero"),
            factors: vec!["factor.pattern.roadside"],
        },
        Candidate {
            id: "pattern.victim_specific",
            value: AttackPattern::VictimSpecific,
            weight: Weight::new(
                if !has_victim_target {
                    0
                } else {
                    victim.plausibility
                },
                victim.curation,
            ),
            bridge: victim.required_bridge.as_deref(),
            impossible: (!has_victim_target)
                .then_some("no unused persistent NPC can anchor the victim cohort")
                .or_else(|| {
                    victim
                        .hard_zero_reason
                        .as_deref()
                        .map(|_| "catalog-authored hard zero")
                }),
            factors: vec!["factor.pattern.victim", "factor.pattern.persistent_cohort"],
        },
        Candidate {
            id: "pattern.irregular",
            value: AttackPattern::Irregular,
            weight: Weight::new(irregular.plausibility, irregular.curation),
            bridge: irregular.required_bridge.as_deref(),
            impossible: irregular
                .hard_zero_reason
                .as_deref()
                .map(|_| "catalog-authored hard zero"),
            factors: vec!["factor.pattern.irregular"],
        },
    ]
}

fn weighted_order<T: Copy>(
    seed: u64,
    domain: &str,
    candidates: &[Candidate<T>],
) -> Result<Vec<usize>, GenerationError> {
    if candidates.len() > MAX_SOLVER_CANDIDATES {
        return Err(GenerationError::CandidateLimit);
    }
    let mut indices = (0..candidates.len())
        .filter(|index| {
            let c = &candidates[*index];
            c.impossible.is_none() && c.weight.combined() > 0
        })
        .collect::<Vec<_>>();
    // Integer-only deterministic weighted permutation. Larger weights yield a
    // smaller key on average, without duplicating inverse probability tables.
    indices.sort_by_key(|index| {
        let c = &candidates[*index];
        (
            hash(seed, &format!("{domain}:{}", c.id)) / c.weight.combined().max(1),
            c.id,
        )
    });
    Ok(indices)
}
