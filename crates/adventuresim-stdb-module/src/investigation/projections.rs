/// Sanitized journal row. It contains no hidden threat, sincerity, coordinates
/// below exact knowledge, private NPC identifiers, hidden likelihoods, or
/// bridges. Authored observer-learned Bestiary support is explicitly safe.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationJournalEntry {
    pub owner_character_id: u64,
    pub case_id: String,
    pub record_id: String,
    pub kind: String,
    pub summary: String,
    pub source_label: String,
    pub confidence_bps: u16,
    pub contradiction_group: String,
    pub corrected_by: String,
    pub supersedes: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationLead {
    pub owner_character_id: u64,
    pub case_id: String,
    pub lead_id: String,
    pub summary: String,
    pub source_label: String,
    pub confidence_bps: u16,
    pub destination_stage: String,
    pub directions: String,
    pub exact_location_id: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub witness_name: String,
    pub witness_description: String,
    pub witness_occupation_or_relationship: String,
    pub expected_location: String,
    pub current_learned_location: String,
    pub contradiction_group: String,
    pub corrected_by: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationAction {
    pub owner_character_id: u64,
    /// Observer-safe public case identifier. Generated canonical identifiers
    /// never cross the gateway projection.
    pub case_id: String,
    pub action_id: String,
    pub method: String,
    pub expected_version: u32,
    pub summary: String,
    pub known_prerequisites: String,
    pub duration_min_minutes: u32,
    pub duration_max_minutes: u32,
    pub uncertainty_bps: u16,
    pub skill_contributions: String,
    pub weather_available: bool,
    pub required_case_site_id: String,
    pub available: bool,
    pub can_travel_to_required_site: bool,
    /// Stable machine-readable reason for unavailability. Empty when available.
    pub unavailable_reason_code: String,
    pub unavailable_reason: String,
    /// Exact bounded wait until a learned temporal condition next permits the action.
    pub wait_minutes: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationActionOutcome {
    pub owner_character_id: u64,
    /// Observer-safe public case identifier correlated to `action_id`.
    pub case_id: String,
    pub outcome_id: String,
    pub action_id: String,
    pub wording: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationCaseSummary {
    pub owner_character_id: u64,
    pub case_id: String,
    /// Immutable observer-safe subject established by the first journal entry.
    /// Later leads and journal headlines never replace it.
    pub subject: String,
    pub status: String,
    pub latest_update_at: u64,
}

fn journal_case_resolution(ctx: &ViewContext, public_case_id: &str) -> (String, u64) {
    let mut canonical_matches: Vec<_> = ctx
        .db
        .quest_generation_authority()
        .public_case_id()
        .filter(public_case_id)
        .filter_map(|authority| {
            validate_quest_generation_authority(&authority)
                .ok()
                .filter(|validated| validated.manifest.public_case_id == public_case_id)
                .map(|validated| validated.manifest.canonical_case_id)
        })
        .collect();
    canonical_matches.sort();
    canonical_matches.dedup();
    let canonical_case_id = match canonical_matches.as_slice() {
        [canonical] => canonical.clone(),
        [] => public_case_id.to_owned(),
        _ => return ("open".into(), 0),
    };
    let Some(case) = ctx.db.case_authority().id().find(canonical_case_id.clone()) else {
        return ("open".into(), 0);
    };
    let status = match case.resolution_status {
        crate::strategic::CaseResolutionStatus::Open => "open",
        crate::strategic::CaseResolutionStatus::Resolved => "completed",
        crate::strategic::CaseResolutionStatus::Failed => "failed",
    };
    let resolved_at = ctx
        .db
        .case_outcome()
        .case_id()
        .find(canonical_case_id)
        .map_or(0, |outcome| outcome.resolved_at_minute);
    (status.into(), resolved_at)
}

#[view(accessor = backend_investigation_cases, public)]
pub fn backend_investigation_cases(ctx: &ViewContext) -> Vec<BackendInvestigationCaseSummary> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    let journal = backend_investigation_journal(ctx);
    let leads = backend_investigation_leads(ctx);
    let mut cases: BTreeMap<(u64, String), (u64, String, u64)> = BTreeMap::new();
    for (owner_character_id, case_id, summary, recorded_at) in journal.into_iter().map(|row| {
        (
            row.owner_character_id,
            row.case_id,
            row.summary,
            row.recorded_at,
        )
    }) {
        let case = cases.entry((owner_character_id, case_id)).or_insert((
            u64::MAX,
            "Unlabelled problem".into(),
            0,
        ));
        case.2 = case.2.max(recorded_at);
        if recorded_at < case.0 {
            case.0 = recorded_at;
            case.1 = summary;
        }
    }
    for lead in leads {
        if let Some(case) = cases.get_mut(&(lead.owner_character_id, lead.case_id)) {
            case.2 = case.2.max(lead.recorded_at);
        }
    }
    cases
        .into_iter()
        .map(
            |((owner_character_id, case_id), (_subject_at, subject, visible_update_at))| {
                let (status, status_update_at) = journal_case_resolution(ctx, &case_id);
                BackendInvestigationCaseSummary {
                    owner_character_id,
                    case_id,
                    subject,
                    status,
                    latest_update_at: visible_update_at.max(status_update_at),
                }
            },
        )
        .collect()
}

/// Dedicated observer-safe map/travel projection. Unlike a raw lead, every
/// row has been joined to a server-issued site and is currently exact for the
/// named observer. The strategic web must additionally filter by session
/// owner before rendering it.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCaseSitePin {
    pub owner_character_id: u64,
    pub case_id: String,
    pub case_site_id: String,
    pub origin_settlement_id: String,
    pub name: String,
    pub description: String,
    pub scene_key: String,
    pub longitude_e7: i32,
    pub latitude_e7: i32,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
    pub knowledge_stage: String,
    pub tracked: bool,
    /// Observer-safe problem wording from a fully validated generated manifest,
    /// or the ordinary site name for a manual case.
    pub display_title: String,
    /// Generated presentation is deliberately independent of contract state.
    pub generated_case: bool,
    /// Observer-safe completion state for the generated case.
    pub case_resolved: bool,
    /// This reveals only that combat is currently a permitted onsite action.
    pub combat_available: bool,
    /// Present only with `combat_available`; aggregate observer-safe strength
    /// of the exact generated hostile group, never hostile identity.
    pub opposition_count: Option<u32>,
    pub opposition_combat_power: Option<u64>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCharacterCaseSiteLocation {
    pub character_id: u64,
    pub case_site_id: CaseSiteId,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendPhysicalEvidence {
    pub owner_character_id: u64,
    pub evidence_id: String,
    pub case_id: String,
    pub case_site_id: String,
    pub label: String,
    pub portrait_icon: String,
    pub description: String,
    /// Observer-safe topic IDs and labels only. Check stats and fixed
    /// difficulties remain private authority.
    pub topics_json: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendPhysicalEvidenceInspection {
    pub attempt_id: String,
    pub owner_character_id: u64,
    pub evidence_id: String,
    pub topic_id: String,
    pub stat_label: String,
    pub passed: bool,
    pub narration: String,
    pub attempted_at: u64,
}

fn is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

#[view(accessor = backend_bestiary_deductions, public)]
pub fn backend_bestiary_deductions(ctx: &ViewContext) -> Vec<BackendBestiaryDeduction> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .investigation_bestiary_deduction()
        .owner_character_id()
        .filter(0u64..)
        .filter_map(|row| {
            let threat = row
                .threat_id
                .parse::<adventuresim_core::bestiary::ThreatId>()
                .ok()?;
            matches!(row.support_band.as_str(), "strong" | "plausible" | "weak").then(|| {
                BackendBestiaryDeduction {
                    owner_character_id: row.owner_character_id,
                    case_id: row.public_case_id,
                    monster_kind: threat.profile().display_name.into(),
                    support_band: row.support_band,
                    provenance_json: row.provenance_json,
                    updated_at: row.updated_at,
                }
            })
        })
        .collect()
}

#[view(accessor = backend_physical_evidence, public)]
pub fn backend_physical_evidence(ctx: &ViewContext) -> Vec<BackendPhysicalEvidence> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    let pins = backend_case_site_pins(ctx);
    let mut rows = Vec::new();
    for pin in pins {
        let mut authorities = ctx
            .db
            .quest_generation_authority()
            .public_case_id()
            .filter(&pin.case_id)
            .filter_map(|authority| validate_quest_generation_authority(&authority).ok())
            .collect::<Vec<_>>();
        authorities.sort_by(|left, right| {
            left.manifest
                .canonical_case_id
                .cmp(&right.manifest.canonical_case_id)
        });
        authorities.dedup_by(|left, right| {
            left.manifest.canonical_case_id == right.manifest.canonical_case_id
        });
        let [validated] = authorities.as_slice() else {
            continue;
        };
        for generated in &validated.manifest.evidence {
            if generated.site_id.0 != pin.case_site_id {
                continue;
            }
            let Some(authority) = ctx
                .db
                .investigation_evidence_authority()
                .id()
                .find(&generated.id.0)
                .filter(|row| {
                    row.case_id == validated.manifest.canonical_case_id
                        && row.presentation_kind == EvidencePresentationKind::Physical
                })
            else {
                continue;
            };
            let topics = generated
                .inspection_topics
                .iter()
                .map(|topic| {
                    serde_json::json!({
                        "id": topic.id,
                        "label": topic.label,
                    })
                })
                .collect::<Vec<_>>();
            let Ok(topics_json) = serde_json::to_string(&topics) else {
                continue;
            };
            rows.push(BackendPhysicalEvidence {
                owner_character_id: pin.owner_character_id,
                evidence_id: authority.id,
                case_id: pin.case_id.clone(),
                case_site_id: pin.case_site_id.clone(),
                label: generated.portrait_label.clone(),
                portrait_icon: generated.portrait_icon.clone(),
                description: generated.base_description.clone(),
                topics_json,
            });
        }
    }
    rows.sort_by(|left, right| {
        (
            left.owner_character_id,
            &left.case_site_id,
            &left.evidence_id,
        )
            .cmp(&(
                right.owner_character_id,
                &right.case_site_id,
                &right.evidence_id,
            ))
    });
    rows
}

#[view(accessor = backend_physical_evidence_inspections, public)]
pub fn backend_physical_evidence_inspections(
    ctx: &ViewContext,
) -> Vec<BackendPhysicalEvidenceInspection> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = ctx
        .db
        .physical_evidence_inspection_attempt()
        .owner_character_id()
        .filter(0u64..)
        .map(|attempt| BackendPhysicalEvidenceInspection {
            attempt_id: attempt.id,
            owner_character_id: attempt.owner_character_id,
            evidence_id: attempt.evidence_id,
            topic_id: attempt.topic_id,
            stat_label: attempt.stat_label,
            passed: attempt.passed,
            narration: attempt.narration,
            attempted_at: attempt.attempted_at,
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| (row.attempted_at, row.attempt_id.clone()));
    rows
}

#[view(accessor = backend_investigation_journal, public)]
pub fn backend_investigation_journal(ctx: &ViewContext) -> Vec<BackendInvestigationJournalEntry> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = Vec::new();
    rows.extend(
        ctx.db
            .investigation_belief_revision()
            .owner_character_id()
            .filter(0u64..)
            .filter_map(|r| {
                let belief = ctx.db.investigation_belief().id().find(&r.belief_id)?;
                if belief.owner_character_id != r.owner_character_id {
                    return None;
                }
                let supersedes = safe_superseded_revision_label(
                    &r,
                    (!r.supersedes.is_empty())
                        .then(|| {
                            ctx.db
                                .investigation_belief_revision()
                                .id()
                                .find(&r.supersedes)
                        })
                        .flatten()
                        .as_ref(),
                );
                Some(BackendInvestigationJournalEntry {
                    owner_character_id: r.owner_character_id,
                    case_id: belief.case_id,
                    record_id: r.id,
                    kind: "belief_revision".into(),
                    summary: r.statement,
                    source_label: r.provenance_label,
                    confidence_bps: r.confidence_bps,
                    contradiction_group: belief.conflict_group,
                    corrected_by: String::new(),
                    supersedes,
                    recorded_at: r.recorded_at,
                })
            }),
    );
    rows.extend(
        ctx.db
            .investigation_journal_notice()
            .owner_character_id()
            .filter(0u64..)
            .map(|notice| BackendInvestigationJournalEntry {
                owner_character_id: notice.owner_character_id,
                case_id: notice.public_case_id,
                record_id: notice.id,
                kind: "news".into(),
                summary: notice.summary,
                source_label: notice.source_label,
                confidence_bps: 10_000,
                contradiction_group: String::new(),
                corrected_by: String::new(),
                supersedes: String::new(),
                recorded_at: notice.recorded_at,
            }),
    );
    rows.sort_by_key(|row| {
        (
            row.owner_character_id,
            row.recorded_at,
            row.record_id.clone(),
        )
    });
    rows
}

#[view(accessor = backend_investigation_leads, public)]
pub fn backend_investigation_leads(ctx: &ViewContext) -> Vec<BackendInvestigationLead> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .investigation_lead()
        .owner_character_id()
        .filter(0u64..)
        .map(|lead| {
            let correction = (!lead.corrected_by.is_empty())
                .then(|| ctx.db.investigation_lead().id().find(&lead.corrected_by))
                .flatten();
            sanitize_lead(lead, correction.as_ref())
        })
        .collect()
}

#[view(accessor = backend_investigation_actions, public)]
pub fn backend_investigation_actions(ctx: &ViewContext) -> Vec<BackendInvestigationAction> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .investigation_action_capability()
        .owner_character_id()
        .filter(0u64..)
        .filter(|capability| capability.active)
        .filter_map(|capability| {
            let kind = parse_action_kind(&capability.method).ok()?;
            if capability_has_successful_attempt_view(ctx, &capability.id)
                || !capability_has_live_support_view(ctx, &capability, kind)
            {
                return None;
            }
            let cost = action::base_cost(kind);
            let required_case_site_id =
                exact_action_site_for_observer(ctx, &capability, kind).unwrap_or_default();
            let availability =
                action_unavailable_reason_view(ctx, &capability, kind, &required_case_site_id);
            let case_id = projected_action_public_case_id(ctx, &capability)?;
            Some(BackendInvestigationAction {
                owner_character_id: capability.owner_character_id,
                case_id,
                action_id: capability.id,
                method: capability.method,
                expected_version: capability.version,
                summary: capability.safe_summary,
                known_prerequisites: capability.known_prerequisites,
                duration_min_minutes: (cost.minutes / 2).max(15),
                duration_max_minutes: cost.minutes.saturating_mul(3) / 2,
                uncertainty_bps: capability.uncertainty_bps,
                skill_contributions:
                    "terrain, awareness, stealth, local familiarity, and bounded party assistance"
                        .into(),
                weather_available: true,
                required_case_site_id,
                available: availability.unavailable_reason.is_none(),
                can_travel_to_required_site: availability.can_travel_to_required_site,
                unavailable_reason_code: availability.unavailable_reason_code,
                unavailable_reason: availability.unavailable_reason.unwrap_or_default(),
                wait_minutes: availability.wait_minutes,
            })
        })
        .collect()
}

fn projected_action_public_case_id(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
) -> Option<String> {
    match capability.provenance_kind.as_str() {
        "manual" if capability.generated_case_id.is_empty() => Some(capability.case_id.clone()),
        "generated" if !capability.generated_case_id.is_empty() => {
            let (manifest_json, _) = generated_authority_view(ctx, capability).ok()??;
            let manifest =
                serde_json::from_str::<adventuresim_core::quest_generation::GeneratedCase>(
                    &manifest_json,
                )
                .ok()?;
            (manifest.canonical_case_id == capability.generated_case_id
                && (capability.case_id == manifest.canonical_case_id
                    || capability.case_id == manifest.public_case_id))
                .then_some(manifest.public_case_id)
        }
        _ => None,
    }
}

fn capability_has_successful_attempt_view(ctx: &ViewContext, capability_id: &str) -> bool {
    ctx.db
        .investigation_action_attempt()
        .capability_id()
        .filter(capability_id)
        .any(|attempt| attempt.success)
}

fn lead_is_live_contact_referral(
    lead: &InvestigationLead,
    owner_character_id: u64,
    case_id: &str,
) -> bool {
    lead.owner_character_id == owner_character_id
        && lead.case_id == case_id
        && !lead.witness_name.is_empty()
        && lead.corrected_by.is_empty()
}

#[cfg(test)]
fn generated_pattern_evidence_id(outputs_json: &str) -> Result<Option<String>, &'static str> {
    let outputs = serde_json::from_str::<
        Vec<adventuresim_core::quest_generation::GeneratedActionOutput>,
    >(outputs_json)
    .map_err(|_| "Generated action output authority is invalid")?;
    Ok(outputs.into_iter().find_map(|output| match output {
        adventuresim_core::quest_generation::GeneratedActionOutput::PatternCondition {
            evidence_id,
            ..
        } => Some(evidence_id.0),
        _ => None,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GeneratedPatternAuthority {
    Manual,
    GeneratedWithoutPattern,
    Pattern {
        evidence_id: String,
        condition: adventuresim_core::quest_generation::GeneratedPatternCondition,
    },
    Invalid,
}

fn generated_capability_safe_text(
    manifest: &adventuresim_core::quest_generation::GeneratedCase,
    generated: &adventuresim_core::quest_generation::GeneratedAction,
) -> (String, String) {
    let evidence_summary = |evidence_id: &str| {
        manifest
            .evidence
            .iter()
            .find(|evidence| evidence.id.0 == evidence_id)
            .map(|evidence| evidence.safe_description.clone())
    };
    let learned_condition = generated.outputs.iter().find_map(|output| match output {
        adventuresim_core::quest_generation::GeneratedActionOutput::PatternCondition {
            evidence_id,
            ..
        } => evidence_summary(&evidence_id.0),
        _ => None,
    });
    let earned_clue = generated.outputs.iter().find_map(|output| match output {
        adventuresim_core::quest_generation::GeneratedActionOutput::Evidence { evidence_id } => {
            evidence_summary(&evidence_id.0)
        }
        _ => None,
    });
    let track_finding = generated.outputs.iter().find_map(|output| match output {
        adventuresim_core::quest_generation::GeneratedActionOutput::TrackFinding {
            finding,
            ..
        } => Some(finding.clone()),
        _ => None,
    });
    (
        learned_condition.map_or_else(
            || {
                "Complete the preceding generated lead and remain with your ready, co-located party."
                    .into()
            },
            |clue| format!("First learn and retain this corroborated clue: {clue}"),
        ),
        track_finding.or(earned_clue).unwrap_or_else(|| {
            "The investigation produces a new, source-attributed lead.".into()
        }),
    )
}

fn generated_action_terrain(
    manifest: &adventuresim_core::quest_generation::GeneratedCase,
    generated: &adventuresim_core::quest_generation::GeneratedAction,
) -> action::Terrain {
    generated
        .track_segment_id
        .as_ref()
        .and_then(|segment_id| {
            manifest
                .track_segments
                .iter()
                .find(|segment| &segment.id == segment_id)
                .map(|segment| segment.terrain)
        })
        .or_else(|| {
            manifest
                .sites
                .iter()
                .find(|site| site.id.0 == generated.target_id)
                .map(|site| site.terrain)
        })
        .or_else(|| {
            manifest
                .areas
                .iter()
                .find(|area| area.id == generated.target_id)
                .map(|area| area.terrain)
        })
        .unwrap_or(action::Terrain::Settlement)
}

fn generated_pattern_authority(
    capability: &InvestigationActionCapability,
    authority: Option<(&str, &str)>,
    persisted_outputs_json: Option<&str>,
) -> GeneratedPatternAuthority {
    match capability.provenance_kind.as_str() {
        "manual" if capability.generated_case_id.is_empty() => {
            return if authority.is_none() && persisted_outputs_json.is_none() {
                GeneratedPatternAuthority::Manual
            } else {
                GeneratedPatternAuthority::Invalid
            };
        }
        "generated" if !capability.generated_case_id.is_empty() => {}
        _ => return GeneratedPatternAuthority::Invalid,
    }
    let Some((manifest_json, context_json)) = authority else {
        return GeneratedPatternAuthority::Invalid;
    };
    // `authority` is supplied only by the unique-row wrappers after the
    // centralized commitment, replay, and semantic validation succeeds.
    let Ok(manifest) =
        serde_json::from_str::<adventuresim_core::quest_generation::GeneratedCase>(manifest_json)
    else {
        return GeneratedPatternAuthority::Invalid;
    };
    let Ok(context) = serde_json::from_str::<adventuresim_core::quest_generation::GenerationContext>(
        context_json,
    ) else {
        return GeneratedPatternAuthority::Invalid;
    };
    if capability.case_id != manifest.canonical_case_id
        && capability.case_id != manifest.public_case_id
        || capability.generated_case_id != manifest.canonical_case_id
    {
        return GeneratedPatternAuthority::Invalid;
    }
    let generated = manifest.actions.iter().find(|action| {
        adventuresim_core::quest_generation::observer_scoped_id(
            &context,
            "capability",
            &format!("{}:{}", capability.owner_character_id, action.id.0),
        ) == capability.id
    });
    let Some(generated) = generated else {
        return GeneratedPatternAuthority::Invalid;
    };
    let remap = |id: &adventuresim_core::quest_generation::ActionId| {
        adventuresim_core::quest_generation::observer_scoped_id(
            &context,
            "capability",
            &format!("{}:{}", capability.owner_character_id, id.0),
        )
    };
    let expected_required = generated
        .prerequisite
        .as_ref()
        .map_or_else(String::new, remap);
    let expected_alternate = remap(&generated.alternate);
    let (expected_known_prerequisites, expected_safe_result) =
        generated_capability_safe_text(&manifest, generated);
    let expected_terrain = generated_action_terrain(&manifest, generated);
    if capability.method != action_method(generated.kind)
        || capability.target_kind != generated.target_kind
        || capability.target_id != generated.target_id
        || capability.target_terrain != format!("{expected_terrain:?}").to_ascii_lowercase()
        || capability.required_action_id != expected_required
        || capability.alternate_route_action_id != expected_alternate
        || capability.safe_summary != generated.safe_summary
        || capability.known_prerequisites != expected_known_prerequisites
        || capability.safe_result_on_success != expected_safe_result
    {
        return GeneratedPatternAuthority::Invalid;
    }
    let expected_consequence = generated
        .outputs
        .iter()
        .find_map(|output| match output {
            adventuresim_core::quest_generation::GeneratedActionOutput::Consequence {
                consequence:
                    adventuresim_core::quest_generation::GeneratedActionConsequence::RetrieveAsset {
                        asset_id,
                        next_version,
                    },
            } => Some(InvestigationActionConsequence::RetrieveAsset {
                asset_id: asset_id.clone(),
                version: *next_version,
            }),
            adventuresim_core::quest_generation::GeneratedActionOutput::Consequence {
                consequence:
                    adventuresim_core::quest_generation::GeneratedActionConsequence::RescueSubject {
                        subject_id,
                        next_version,
                    },
            } => Some(InvestigationActionConsequence::RescueSubject {
                subject_id: subject_id.clone(),
                version: *next_version,
            }),
            _ => None,
        })
        .unwrap_or(InvestigationActionConsequence::None);
    if serde_json::to_string(&expected_consequence).ok().as_deref()
        != Some(capability.consequence_json.as_str())
    {
        return GeneratedPatternAuthority::Invalid;
    }
    let Some(persisted_outputs_json) = persisted_outputs_json else {
        return GeneratedPatternAuthority::Invalid;
    };
    let Ok(persisted_outputs) = serde_json::from_str::<
        Vec<adventuresim_core::quest_generation::GeneratedActionOutput>,
    >(persisted_outputs_json) else {
        return GeneratedPatternAuthority::Invalid;
    };
    if persisted_outputs != generated.outputs {
        return GeneratedPatternAuthority::Invalid;
    }
    generated
        .outputs
        .iter()
        .find_map(|output| match output {
            adventuresim_core::quest_generation::GeneratedActionOutput::PatternCondition {
                evidence_id,
                condition,
            } => Some(GeneratedPatternAuthority::Pattern {
                evidence_id: evidence_id.0.clone(),
                condition: condition.clone(),
            }),
            _ => None,
        })
        .unwrap_or(GeneratedPatternAuthority::GeneratedWithoutPattern)
}

fn exactly_one_generated_authority(
    matches: impl IntoIterator<Item = (String, String, String)>,
) -> Result<Option<(String, String)>, ()> {
    let mut unique = BTreeMap::new();
    for (case_id, manifest, context) in matches {
        if unique
            .insert(case_id, (manifest.clone(), context.clone()))
            .is_some_and(|existing| existing != (manifest, context))
        {
            return Err(());
        }
    }
    let mut unique = unique.into_values();
    let Some(authority) = unique.next() else {
        return Ok(None);
    };
    if unique.next().is_some() {
        return Err(());
    }
    Ok(Some(authority))
}

fn validated_generated_authority_candidate(
    authority: crate::strategic::QuestGenerationAuthority,
) -> Result<(String, String, String), ()> {
    validate_quest_generation_authority(&authority).map_err(|_| ())?;
    Ok((
        authority.case_id,
        authority.manifest_json,
        authority.context_snapshot_json,
    ))
}

fn generated_authority_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
) -> Result<Option<(String, String)>, ()> {
    match capability.provenance_kind.as_str() {
        "manual" if capability.generated_case_id.is_empty() => return Ok(None),
        "generated" if !capability.generated_case_id.is_empty() => {}
        _ => return Err(()),
    }
    let mut candidates = Vec::new();
    for alias in [&capability.generated_case_id, &capability.case_id] {
        if candidates
            .iter()
            .any(|(queried, _, _, _): &(String, String, String, String)| queried == alias)
        {
            continue;
        }
        if let Some(authority) = ctx.db.quest_generation_authority().case_id().find(alias) {
            let candidate = validated_generated_authority_candidate(authority)?;
            candidates.push((alias.clone(), candidate.0, candidate.1, candidate.2));
        }
        for authority in ctx
            .db
            .quest_generation_authority()
            .public_case_id()
            .filter(alias)
        {
            let candidate = validated_generated_authority_candidate(authority)?;
            candidates.push((alias.clone(), candidate.0, candidate.1, candidate.2));
        }
    }
    exactly_one_generated_authority(
        candidates
            .into_iter()
            .map(|(_, case_id, manifest, context)| (case_id, manifest, context)),
    )
}

fn generated_authority_reducer(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
) -> Result<Option<(String, String)>, ()> {
    match capability.provenance_kind.as_str() {
        "manual" if capability.generated_case_id.is_empty() => return Ok(None),
        "generated" if !capability.generated_case_id.is_empty() => {}
        _ => return Err(()),
    }
    let mut candidates = Vec::new();
    for alias in [&capability.generated_case_id, &capability.case_id] {
        if candidates
            .iter()
            .any(|(queried, _, _, _): &(String, String, String, String)| queried == alias)
        {
            continue;
        }
        if let Some(authority) = ctx.db.quest_generation_authority().case_id().find(alias) {
            let candidate = validated_generated_authority_candidate(authority)?;
            candidates.push((alias.clone(), candidate.0, candidate.1, candidate.2));
        }
        for authority in ctx
            .db
            .quest_generation_authority()
            .public_case_id()
            .filter(alias)
        {
            let candidate = validated_generated_authority_candidate(authority)?;
            candidates.push((alias.clone(), candidate.0, candidate.1, candidate.2));
        }
    }
    exactly_one_generated_authority(
        candidates
            .into_iter()
            .map(|(_, case_id, manifest, context)| (case_id, manifest, context)),
    )
}

fn reducer_action_public_case_id(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
) -> Option<String> {
    match capability.provenance_kind.as_str() {
        "manual" if capability.generated_case_id.is_empty() => Some(capability.case_id.clone()),
        "generated" if !capability.generated_case_id.is_empty() => {
            let (manifest_json, _) = generated_authority_reducer(ctx, capability).ok()??;
            let manifest = serde_json::from_str::<
                adventuresim_core::quest_generation::GeneratedCase,
            >(&manifest_json)
            .ok()?;
            (manifest.canonical_case_id == capability.generated_case_id
                && (capability.case_id == manifest.canonical_case_id
                    || capability.case_id == manifest.public_case_id))
                .then_some(manifest.public_case_id)
        }
        _ => None,
    }
}

fn generated_investigability(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
) -> Option<u8> {
    let (_, manifest_json) = generated_authority_reducer(ctx, capability).ok()??;
    let manifest =
        serde_json::from_str::<adventuresim_core::quest_generation::GeneratedCase>(&manifest_json)
            .ok()?;
    let adventuresim_core::quest_generation::CanonicalCause::Hostile(threat) = manifest.cause
    else {
        return None;
    };
    Some(
        adventuresim_core::bestiary::profile(threat)
            .investigation
            .investigability,
    )
}

fn apply_investigability_to_route_skills(
    mut skills: action::SkillContribution,
    investigability: u8,
) -> action::SkillContribution {
    let modifier = i32::from(adventuresim_core::threat_escalation::check_modifier_milli(
        investigability,
    )) * 2;
    let adjust = |value: u16| (i32::from(value) + modifier).clamp(0, 10_000) as u16;
    skills.terrain_bps = adjust(skills.terrain_bps);
    skills.awareness_bps = adjust(skills.awareness_bps);
    skills.stealth_bps = adjust(skills.stealth_bps);
    skills
}

fn observer_pattern_route_has_live_corroborated_clue(
    owner_character_id: u64,
    case_id: &str,
    evidence_id: &str,
    observer_personal_minute: u64,
    knowledge: impl IntoIterator<Item = InvestigationEvidenceKnowledge>,
) -> bool {
    let mut numeric_ids = BTreeMap::new();
    let mut adapted = Vec::new();
    for row in knowledge {
        if row.owner_character_id != owner_character_id {
            continue;
        }
        let Ok(record) = inv::adapt_evidence_knowledge(
            &row.id,
            row.owner_character_id,
            &row.case_id,
            &row.evidence_id,
            &row.source_id,
            row.learned_at,
            observer_personal_minute,
        ) else {
            return false;
        };
        let numeric_id = record.record().envelope().record_id().get();
        if numeric_ids
            .insert(numeric_id, record.persisted_id().to_owned())
            .is_some_and(|prior| prior != record.persisted_id())
        {
            return false;
        }
        adapted.push(record);
    }
    adapted.into_iter().any(|record| {
        let proposition = record.record().envelope().proposition();
        proposition.case_id.as_str() == case_id && proposition.evidence_id.as_str() == evidence_id
    })
}

fn capability_has_live_pattern_support_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
) -> bool {
    let Some(observer_case_id) = projected_action_public_case_id(ctx, capability) else {
        return false;
    };
    let output = ctx
        .db
        .investigation_generated_action_output()
        .capability_id()
        .find(&capability.id);
    let Ok(authority) = generated_authority_view(ctx, capability) else {
        return false;
    };
    let evidence_id = match generated_pattern_authority(
        capability,
        authority
            .as_ref()
            .map(|(manifest, context)| (manifest.as_str(), context.as_str())),
        output.as_ref().map(|output| output.outputs_json.as_str()),
    ) {
        GeneratedPatternAuthority::Manual | GeneratedPatternAuthority::GeneratedWithoutPattern => {
            return true;
        }
        GeneratedPatternAuthority::Pattern { evidence_id, .. } => evidence_id,
        GeneratedPatternAuthority::Invalid => return false,
    };
    observer_pattern_route_has_live_corroborated_clue(
        capability.owner_character_id,
        &observer_case_id,
        &evidence_id,
        ctx.db
            .character_time()
            .character_id()
            .find(capability.owner_character_id)
            .map_or(0, |time| time.minutes),
        ctx.db
            .investigation_evidence_knowledge()
            .owner_character_id()
            .filter(capability.owner_character_id),
    )
}

fn tracking_capability_chain_is_coherent(
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
    mut capability_by_id: impl FnMut(&str) -> Option<InvestigationActionCapability>,
    mut predecessor_succeeded: impl FnMut(&str) -> bool,
) -> bool {
    fn visit(
        capability: &InvestigationActionCapability,
        kind: action::InvestigationActionKind,
        capability_by_id: &mut impl FnMut(&str) -> Option<InvestigationActionCapability>,
        predecessor_succeeded: &mut impl FnMut(&str) -> bool,
        visited: &mut HashSet<String>,
    ) -> bool {
        if !matches!(
            kind,
            action::InvestigationActionKind::FollowTracks
                | action::InvestigationActionKind::ReacquireTracks
        ) {
            return true;
        }
        if capability.required_action_id.is_empty()
            || !visited.insert(capability.id.clone())
            || !predecessor_succeeded(&capability.required_action_id)
        {
            return false;
        }
        let Some(predecessor) = capability_by_id(&capability.required_action_id) else {
            return false;
        };
        let Ok(predecessor_kind) = parse_action_kind(&predecessor.method) else {
            return false;
        };
        if predecessor.owner_character_id != capability.owner_character_id
            || predecessor.case_id != capability.case_id
            || !action::tracking_route_edge_is_coherent(
                kind,
                &capability.target_kind,
                predecessor_kind,
                &predecessor.target_kind,
            )
        {
            return false;
        }
        visit(
            &predecessor,
            predecessor_kind,
            capability_by_id,
            predecessor_succeeded,
            visited,
        )
    }

    visit(
        capability,
        kind,
        &mut capability_by_id,
        &mut predecessor_succeeded,
        &mut HashSet::new(),
    )
}

fn capability_has_live_support_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> bool {
    let Some(observer_case_id) = projected_action_public_case_id(ctx, capability) else {
        return false;
    };
    if !tracking_capability_chain_is_coherent(
        capability,
        kind,
        |id| {
            ctx.db
                .investigation_action_capability()
                .id()
                .find(id.to_owned())
        },
        |id| capability_has_successful_attempt_view(ctx, id),
    ) {
        return false;
    }
    if !capability.required_action_id.is_empty()
        && !capability_has_successful_attempt_view(ctx, &capability.required_action_id)
    {
        return false;
    }
    if !capability_has_live_pattern_support_view(ctx, capability) {
        return false;
    }
    if kind == action::InvestigationActionKind::InspectSite
        && capability.target_kind == "site"
        && exact_action_site_for_observer(ctx, capability, kind).is_none()
    {
        return false;
    }
    let prerequisites = action::prerequisites(kind);
    if prerequisites.requires_contact_referral
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead_is_live_contact_referral(
                    &lead,
                    capability.owner_character_id,
                    &observer_case_id,
                )
            })
    {
        return false;
    }
    if prerequisites.requires_approximate_destination
        && capability.target_kind != "area"
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead.case_id == observer_case_id
                    && lead.destination_stage == "approximate_area"
                    && lead.corrected_by.is_empty()
            })
    {
        return false;
    }
    !prerequisites.requires_tracks || !capability.required_action_id.is_empty()
}

fn exact_action_site_for_observer(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> Option<String> {
    if kind != action::InvestigationActionKind::InspectSite || capability.target_kind != "site" {
        return None;
    }
    let lead = ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(capability.owner_character_id)
        .find(|lead| {
            lead.exact_location_id == capability.target_id
                && lead.corrected_by.is_empty()
                && matches!(
                    lead.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
        })?;
    let site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&lead.exact_location_id)?;
    let generated_aliases = case_site_provenance_view(ctx, &site)?;
    match (&generated_aliases, capability.provenance_kind.as_str()) {
        (None, "manual") if capability.generated_case_id.is_empty() => {}
        (Some((canonical, public)), "generated")
            if capability.generated_case_id == canonical.as_str()
                && (capability.case_id == canonical.as_str()
                    || capability.case_id == public.as_str()) => {}
        _ => return None,
    }
    exact_site_knowledge_is_live(
        &capability.case_id,
        &capability.target_id,
        &lead.case_id,
        &lead.exact_location_id,
        &lead.destination_stage,
        &lead.corrected_by,
        &site.case_id,
        &site.id.value,
        lead.latitude_e7 == site.latitude_e7 && lead.longitude_e7 == site.longitude_e7,
        generated_aliases.as_ref().map(|aliases| aliases.0.as_str()),
        generated_aliases.as_ref().map(|aliases| aliases.1.as_str()),
    )
    .then_some(lead.exact_location_id)
}

fn exact_site_knowledge_is_live(
    capability_case_id: &str,
    capability_target_id: &str,
    lead_case_id: &str,
    lead_exact_location_id: &str,
    lead_destination_stage: &str,
    lead_corrected_by: &str,
    authority_case_id: &str,
    authority_site_id: &str,
    coordinates_match: bool,
    generated_canonical_case_id: Option<&str>,
    generated_public_case_id: Option<&str>,
) -> bool {
    let cases_match = if let Some(canonical) = generated_canonical_case_id {
        let is_generated_alias = |candidate: &str| {
            candidate == canonical
                || generated_public_case_id.is_some_and(|public| candidate == public)
        };
        is_generated_alias(capability_case_id)
            && is_generated_alias(lead_case_id)
            && authority_case_id == canonical
    } else {
        capability_case_id == lead_case_id && capability_case_id == authority_case_id
    };
    cases_match
        && capability_target_id == lead_exact_location_id
        && capability_target_id == authority_site_id
        && matches!(lead_destination_stage, "exact_believed" | "visited")
        && lead_corrected_by.is_empty()
        && coordinates_match
}

struct ProjectedActionAvailability {
    unavailable_reason: Option<String>,
    unavailable_reason_code: String,
    can_travel_to_required_site: bool,
    wait_minutes: u32,
}

fn projected_action_availability(
    party_ready: bool,
    required_case_site_id: &str,
    occupying_required_site: bool,
    temporal_wait_minutes: u32,
) -> ProjectedActionAvailability {
    if !party_ready {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "An incapacitated party member must recover before the party can investigate."
                    .into(),
            ),
            unavailable_reason_code: "party_not_ready".into(),
            can_travel_to_required_site: false,
            wait_minutes: 0,
        };
    }
    if !required_case_site_id.is_empty() && !occupying_required_site {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "Travel to the known investigation site before inspecting it.".into(),
            ),
            unavailable_reason_code: "travel_required".into(),
            can_travel_to_required_site: true,
            wait_minutes: 0,
        };
    }
    if temporal_wait_minutes > 0 {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "Wait until the learned nighttime activity window begins.".into(),
            ),
            unavailable_reason_code: "night_window".into(),
            can_travel_to_required_site: false,
            wait_minutes: temporal_wait_minutes,
        };
    }
    ProjectedActionAvailability {
        unavailable_reason: None,
        unavailable_reason_code: String::new(),
        can_travel_to_required_site: false,
        wait_minutes: 0,
    }
}

fn projected_target_changed_availability() -> ProjectedActionAvailability {
    ProjectedActionAvailability {
        unavailable_reason: Some(
            "The circumstances supporting this action changed. Replan before acting.".into(),
        ),
        unavailable_reason_code: "target_changed".into(),
        can_travel_to_required_site: false,
        wait_minutes: 0,
    }
}

fn public_contact_schedule_wait_minutes(
    presence: &crate::SettlementResidentPresence,
    minute: u64,
) -> Option<u32> {
    if crate::settlement_population::npc_presence_remaining_minutes(presence, minute).is_some() {
        return Some(0);
    }
    if presence.context_suppressed || presence.health_suppressed {
        return None;
    }
    let current = minute % 1_440;
    let start = u64::from(presence.start_minute);
    let wait = (start + 1_440 - current) % 1_440;
    Some(if wait == 0 { 1_440 } else { wait } as u32)
}

fn referred_contact_target_matches(
    expected: &adventuresim_core::quest_generation::WitnessCandidate,
    current: &adventuresim_core::quest_generation::WitnessCandidate,
    settlement_id: &str,
    expected_settlement_id: &str,
) -> bool {
    adventuresim_core::quest_generation::pattern_target_matches(
        &adventuresim_core::quest_generation::GeneratedPatternTarget {
            cohort_id: "referred-contact".into(),
            resident_character_id: expected.resident_character_id,
            demographic: expected.demographic,
            age_band: expected.age_band.clone(),
            sex: expected.sex.clone(),
            profession: expected.profession.clone(),
            expected_settlement_id: expected_settlement_id.into(),
            expected_location: expected.expected_location.clone(),
            expected_location_label: expected.expected_location_label.clone(),
            presence_version: expected.presence_version,
        },
        current,
        settlement_id,
    )
}

fn referred_contact_is_current_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    presence: &crate::SettlementResidentPresence,
) -> bool {
    let Ok(resident_character_id) = capability.target_id.parse::<u64>() else {
        return false;
    };
    let Some((_, context_json)) = generated_authority_view(ctx, capability).ok().flatten() else {
        return false;
    };
    let Ok(context) = serde_json::from_str::<
        adventuresim_core::quest_generation::GenerationContext,
    >(&context_json)
    else {
        return false;
    };
    let Some(expected) = context
        .witness_candidates
        .iter()
        .find(|candidate| candidate.resident_character_id == resident_character_id)
    else {
        return false;
    };
    let Some(npc) =
        crate::settlement_population::resolve_settlement_resident_view(ctx, resident_character_id)
    else {
        return false;
    };
    let Some(current) = (if expected.sex.is_empty() {
        crate::strategic::developer_npc_witness_candidate(&npc, presence)
    } else {
        Some(adventuresim_core::quest_generation::WitnessCandidate {
            resident_character_id: npc.character_id,
            display_name: npc.name.clone(),
            demographic: crate::strategic::generated_npc_demographic(&npc),
            age_band: format!("{:?}", npc.age_band).to_ascii_lowercase(),
            sex: format!("{:?}", npc.sex).to_ascii_lowercase(),
            profession: npc.profession.clone(),
            visible_description: String::new(),
            expected_location: presence.location_id.clone(),
            expected_location_label: presence.location_id.clone(),
            presence_version: crate::strategic::generated_npc_presence_version(&npc, presence),
            allowed_circumstances: Default::default(),
        })
    }) else {
        return false;
    };
    referred_contact_target_matches(
        expected,
        &current,
        &presence.settlement_id,
        &context.settlement_id,
    )
}

fn projected_contact_presence_availability(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
    settlement_id: Option<&str>,
    started_at: Option<u64>,
) -> Option<ProjectedActionAvailability> {
    if kind != action::InvestigationActionKind::LocateContact
        || capability.target_kind != "contact"
    {
        return None;
    }
    let presence = capability
        .target_id
        .parse::<u64>()
        .ok()
        .and_then(|character_id| {
            ctx.db
                .settlement_resident_presence()
                .character_id()
                .find(character_id)
        });
    let Some(presence) = presence else {
        return Some(projected_target_changed_availability());
    };
    if settlement_id != Some(presence.settlement_id.as_str())
        || !referred_contact_is_current_view(ctx, capability, &presence)
    {
        return Some(projected_target_changed_availability());
    }
    match started_at.and_then(|minute| public_contact_schedule_wait_minutes(&presence, minute)) {
        Some(0) => None,
        Some(wait_minutes) => Some(ProjectedActionAvailability {
            unavailable_reason: Some(
                "Wait until the referred contact's public schedule resumes.".into(),
            ),
            unavailable_reason_code: "contact_schedule_window".into(),
            can_travel_to_required_site: false,
            wait_minutes,
        }),
        None => Some(ProjectedActionAvailability {
            unavailable_reason: Some("The referred contact is not currently available.".into()),
            unavailable_reason_code: "contact_not_present".into(),
            can_travel_to_required_site: false,
            wait_minutes: 0,
        }),
    }
}

fn night_window_wait_minutes(minute: u64) -> u32 {
    let minute = minute % 1_440;
    if (360..1_200).contains(&minute) {
        (1_200 - minute) as u32
    } else {
        0
    }
}

fn projected_party_activity_minute(ctx: &ViewContext, party_id: &str) -> Option<u64> {
    let official_minute = ctx
        .db
        .world_clock()
        .id()
        .find(0)
        .map_or(0, |clock| clock.official_minutes);
    let living_party_minute = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .filter_map(|membership| ctx.db.character().id().find(membership.character_id))
        .filter(|member| member.alive)
        .filter_map(|member| {
            ctx.db
                .character_time()
                .character_id()
                .find(member.id)
                .map(|time| time.minutes)
        })
        .max()?;
    Some(official_minute.max(living_party_minute))
}

fn projected_night_window_wait_minutes(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    started_at: u64,
) -> u32 {
    let output = ctx
        .db
        .investigation_generated_action_output()
        .capability_id()
        .find(&capability.id);
    let authority = generated_authority_view(ctx, capability).ok().flatten();
    let GeneratedPatternAuthority::Pattern {
        evidence_id,
        condition,
    } = generated_pattern_authority(
        capability,
        authority
            .as_ref()
            .map(|(manifest, context)| (manifest.as_str(), context.as_str())),
        output.as_ref().map(|output| output.outputs_json.as_str()),
    )
    else {
        return 0;
    };
    if !matches!(
        condition,
        adventuresim_core::quest_generation::GeneratedPatternCondition::NightWindow
    ) || !observer_pattern_route_has_live_corroborated_clue(
        capability.owner_character_id,
        &capability.case_id,
        &evidence_id,
        started_at,
        ctx.db
            .investigation_evidence_knowledge()
            .owner_character_id()
            .filter(capability.owner_character_id),
    ) {
        return 0;
    }
    night_window_wait_minutes(started_at)
}

fn victim_cohort_is_current_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> bool {
    if capability.target_kind != "cohort" {
        return true;
    }
    let Some(actor) = ctx.db.character().id().find(capability.owner_character_id) else {
        return false;
    };
    let Some(party_id) = actor.party_id.as_deref() else {
        return false;
    };
    let Some(started_at) = projected_party_activity_minute(ctx, party_id) else {
        return false;
    };
    let Some(target) = ctx
        .db
        .investigation_pattern_target_authority()
        .cohort_id()
        .find(&capability.target_id)
    else {
        return false;
    };
    if target.case_id != capability.case_id {
        return false;
    }
    let Some(npc) = crate::settlement_population::resolve_settlement_resident_view(
        ctx,
        target.resident_character_id,
    ) else {
        return false;
    };
    let Some(presence) = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(target.resident_character_id)
    else {
        return false;
    };
    if actor.current_settlement_id.as_deref() != Some(presence.settlement_id.as_str())
        || presence.settlement_id != target.expected_settlement_id
        || presence.location_id != target.expected_location
    {
        return false;
    }

    let output = ctx
        .db
        .investigation_generated_action_output()
        .capability_id()
        .find(&capability.id);
    let authority = generated_authority_view(ctx, capability).ok().flatten();
    let GeneratedPatternAuthority::Pattern {
        condition:
            adventuresim_core::quest_generation::GeneratedPatternCondition::VictimProfile {
                cohort_id,
                demographic,
                age_band,
                sex,
                profession,
            },
        ..
    } = generated_pattern_authority(
        capability,
        authority
            .as_ref()
            .map(|(manifest, context)| (manifest.as_str(), context.as_str())),
        output.as_ref().map(|row| row.outputs_json.as_str()),
    )
    else {
        return true;
    };
    if kind != action::InvestigationActionKind::Patrol
        || capability.target_id != cohort_id
        || target.demographic != format!("{demographic:?}").to_ascii_lowercase()
        || target.age_band != age_band
        || target.sex != sex
        || target.profession != profession
    {
        return false;
    }
    let expected = adventuresim_core::quest_generation::GeneratedPatternTarget {
        cohort_id: target.cohort_id.clone(),
        resident_character_id: target.resident_character_id,
        demographic,
        age_band: target.age_band.clone(),
        sex: target.sex.clone(),
        profession: target.profession.clone(),
        expected_settlement_id: target.expected_settlement_id.clone(),
        expected_location: target.expected_location.clone(),
        expected_location_label: String::new(),
        presence_version: target.presence_version,
    };
    let Some(current) = (if target.sex.is_empty() {
        crate::strategic::developer_npc_witness_candidate(&npc, &presence)
    } else {
        Some(adventuresim_core::quest_generation::WitnessCandidate {
            resident_character_id: npc.character_id,
            display_name: npc.name.clone(),
            demographic: crate::strategic::generated_npc_demographic(&npc),
            age_band: format!("{:?}", npc.age_band).to_ascii_lowercase(),
            sex: format!("{:?}", npc.sex).to_ascii_lowercase(),
            profession: npc.profession.clone(),
            visible_description: String::new(),
            expected_location: presence.location_id.clone(),
            expected_location_label: presence.location_id.clone(),
            presence_version: crate::strategic::generated_npc_presence_version(&npc, &presence),
            allowed_circumstances: Default::default(),
        })
    }) else {
        return false;
    };
    adventuresim_core::quest_generation::pattern_target_matches(
        &expected,
        &current,
        &presence.settlement_id,
    ) && crate::settlement_population::npc_presence_remaining_minutes(&presence, started_at)
        .is_some()
}

fn action_unavailable_reason_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
    required_case_site_id: &str,
) -> ProjectedActionAvailability {
    let Some(character) = ctx.db.character().id().find(capability.owner_character_id) else {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "The investigating character is currently unavailable.".into(),
            ),
            unavailable_reason_code: "character_unavailable".into(),
            can_travel_to_required_site: false,
            wait_minutes: 0,
        };
    };
    if !character.alive {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "The investigating character is currently unavailable.".into(),
            ),
            unavailable_reason_code: "character_unavailable".into(),
            can_travel_to_required_site: false,
            wait_minutes: 0,
        };
    }
    let Some(party_id) = character.party_id else {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "Join or form a party before attempting this investigation.".into(),
            ),
            unavailable_reason_code: "party_required".into(),
            can_travel_to_required_site: false,
            wait_minutes: 0,
        };
    };
    let party_ready = !ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .filter_map(|membership| ctx.db.character().id().find(membership.character_id))
        .filter(|member| member.alive)
        .any(|member| {
            ctx.db
                .character_strategic_condition()
                .character_id()
                .find(member.id)
                .is_some_and(|condition| condition.status == "incapacitated")
        });
    let occupying_required_site = !required_case_site_id.is_empty()
        && ctx
            .db
            .party_authority()
            .id()
            .find(&party_id)
            .and_then(|party| party.current_case_site_id)
            .and_then(|site| site.to_place())
            .zip(canonical_case_site_place(required_case_site_id))
            .is_some_and(|(occupied, required)| occupied == required);
    let projected_started_at = projected_party_activity_minute(ctx, &party_id);
    if let Some(availability) = projected_contact_presence_availability(
        ctx,
        capability,
        kind,
        character.current_settlement_id.as_deref(),
        projected_started_at,
    ) {
        return availability;
    }
    let temporal_wait_minutes = projected_started_at
        .map_or(0, |started_at| {
            projected_night_window_wait_minutes(ctx, capability, started_at)
        });
    if !victim_cohort_is_current_view(ctx, capability, kind) {
        return projected_target_changed_availability();
    }
    projected_action_availability(
        party_ready,
        required_case_site_id,
        occupying_required_site,
        temporal_wait_minutes,
    )
}

#[view(accessor = backend_investigation_action_outcomes, public)]
pub fn backend_investigation_action_outcomes(
    ctx: &ViewContext,
) -> Vec<BackendInvestigationActionOutcome> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .investigation_action_outcome()
        .owner_character_id()
        .filter(0u64..)
        .filter_map(|outcome| {
            let capability = ctx
                .db
                .investigation_action_capability()
                .id()
                .find(&outcome.capability_id)?;
            if capability.owner_character_id != outcome.owner_character_id
                || capability.case_id != outcome.case_id
            {
                return None;
            }
            let case_id = projected_action_public_case_id(ctx, &capability)?;
            Some(BackendInvestigationActionOutcome {
                owner_character_id: outcome.owner_character_id,
                case_id,
                outcome_id: outcome.id,
                action_id: outcome.capability_id,
                wording: outcome.safe_wording,
                recorded_at: outcome.recorded_at,
            })
        })
        .collect()
}
