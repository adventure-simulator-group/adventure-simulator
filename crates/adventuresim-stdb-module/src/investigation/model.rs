use crate::{
    character::{
        character, character__view, character_attributes, character_limbs, character_skills,
        character_stats,
    },
    condition::character_strategic_condition__view,
    local_problem::local_problem_receipt,
    settlement_population::{
        settlement_npc, settlement_npc__view, settlement_npc_presence,
        settlement_npc_presence__view,
    },
    strategic::{
        CustodyHolderKind, CustodyObjectKind, case_authority, case_authority__view, case_custody,
        case_finale_authority__view, case_outcome__view, case_outcome_fact__view,
        generated_case_site_combat_eligible, generated_case_site_combat_group_id,
        hostile_group_authority__view,
        living_party_member_ids, party_authority, party_authority__view, party_journey_authority,
        party_member__view, quest_generation_authority, quest_generation_authority__view,
        require_no_unresolved_encounter, require_party_ready,
        require_strategic_character_authority, require_strategic_gateway, settlement,
        strategic_gateway_authority__view, validate_quest_generation_authority,
    },
    time::{
        advance_investigation_time, character_time, character_time__view,
        synchronize_party_activity_time, world_clock, world_clock__view,
    },
};
use adventuresim_core::investigation as inv;
use adventuresim_core::investigation_action as action;
use adventuresim_core::skill::{PlayerSkills, Skill};
use serde::{Deserialize, Serialize};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};
use std::collections::{BTreeMap, BTreeSet, HashSet};

const MAX_TEXT: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SpacetimeType)]
pub struct CaseSiteId {
    pub value: String,
}

impl CaseSiteId {
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl From<String> for CaseSiteId {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl std::ops::Deref for CaseSiteId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for CaseSiteId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(formatter)
    }
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_case_authority)]
pub struct InvestigationCaseAuthority {
    #[primary_key]
    pub id: String,
    pub problem_id: String,
    pub hidden_target_json: String,
    pub generation_explanation_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_event_authority)]
pub struct InvestigationEventAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub canonical_propositions_json: String,
    pub occurred_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_observation)]
pub struct InvestigationObservation {
    #[primary_key]
    pub id: String,
    pub event_id: String,
    pub observer_ref: String,
    pub proposition_id: String,
    pub stage_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_recollection)]
pub struct InvestigationRecollection {
    #[primary_key]
    pub id: String,
    pub observation_id: String,
    pub witness_ref: String,
    pub proposition_id: String,
    pub stage_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_claim)]
pub struct InvestigationClaim {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub proposition_id: String,
    pub hidden_speaker_ref: String,
    pub statement: String,
    pub confidence_bps: u16,
    pub disclosure_stage: String,
    pub transmission_stage: String,
    pub received_at: u64,
    pub public_case_id: String,
    pub safe_source_label: String,
    pub conflict_group: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_evidence_authority)]
pub struct InvestigationEvidenceAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub proposition_id: String,
    pub presentation_kind: EvidencePresentationKind,
    pub authority_json: String,
    pub hidden_coordinates_json: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum EvidencePresentationKind {
    Physical,
    Informational,
}

/// Private, source-attributed proof custody/knowledge. Merely having a hidden
/// evidence-authority row is never enough to present that proof.
#[derive(Clone, Debug)]
#[table(accessor = investigation_evidence_knowledge)]
pub struct InvestigationEvidenceKnowledge {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    pub evidence_id: String,
    pub source_id: String,
    pub learned_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = physical_evidence_inspection_attempt)]
pub struct PhysicalEvidenceInspectionAttempt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub evidence_id: String,
    pub topic_id: String,
    pub stat_label: String,
    pub passed: bool,
    pub narration: String,
    /// Observer-safe successes only. Hidden lore thresholds and failed checks
    /// are never persisted into the projected payload.
    pub bestiary_results_json: String,
    pub attempted_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = physical_evidence_inspection_action_receipt)]
pub struct PhysicalEvidenceInspectionActionReceipt {
    #[primary_key]
    pub action_id: String,
    pub owner_character_id: u64,
    pub evidence_id: String,
    pub topic_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedBestiaryLoreResult {
    diagnostic_kind: String,
    interpretation: String,
}

/// A report exists only after this observer actually receives testimony from a
/// generated witness whose private manifest has been revalidated.
#[derive(Clone, Debug)]
#[table(accessor = investigation_bestiary_report_receipt)]
pub struct InvestigationBestiaryReportReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub public_case_id: String,
    pub description_id: String,
    pub source_label: String,
    pub received_at: u64,
}

/// A diagnostic clue exists only after the owning observer passes its private
/// Bestiary check. Failed checks never create this authority.
#[derive(Clone, Debug)]
#[table(accessor = investigation_bestiary_diagnostic_receipt)]
pub struct InvestigationBestiaryDiagnosticReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub public_case_id: String,
    pub diagnostic_kind: String,
    pub interpretation: String,
    pub learned_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_bestiary_deduction)]
pub struct InvestigationBestiaryDeduction {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub public_case_id: String,
    pub threat_id: String,
    pub support_band: String,
    pub provenance_json: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendBestiaryDeduction {
    pub owner_character_id: u64,
    pub case_id: String,
    pub monster_kind: String,
    pub support_band: String,
    pub provenance_json: String,
    pub updated_at: u64,
}

fn parse_bestiary_lore_results(payload: &str) -> Result<Vec<PersistedBestiaryLoreResult>, String> {
    let results: Vec<PersistedBestiaryLoreResult> = serde_json::from_str(payload)
        .map_err(|_| "Stored Bestiary inspection results are invalid")?;
    let mut kinds = BTreeSet::new();
    if results.iter().any(|result| {
        !kinds.insert(result.diagnostic_kind.clone())
            || adventuresim_core::bestiary::EvidenceKind::try_new(&result.diagnostic_kind).is_err()
            || result.interpretation.trim().is_empty()
            || result.interpretation.len() > 1_024
    }) {
        return Err("Stored Bestiary inspection results are invalid".into());
    }
    Ok(results)
}

fn deduction_support_rank(value: adventuresim_core::investigation::DeductionSupport) -> u8 {
    match value {
        adventuresim_core::investigation::DeductionSupport::Weak => 0,
        adventuresim_core::investigation::DeductionSupport::Plausible => 1,
        adventuresim_core::investigation::DeductionSupport::Strong => 2,
    }
}

fn deduction_support_label(
    value: adventuresim_core::investigation::DeductionSupport,
) -> &'static str {
    match value {
        adventuresim_core::investigation::DeductionSupport::Weak => "weak",
        adventuresim_core::investigation::DeductionSupport::Plausible => "plausible",
        adventuresim_core::investigation::DeductionSupport::Strong => "strong",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SafeBestiaryDeduction {
    threat_id: adventuresim_core::bestiary::ThreatId,
    support_band: &'static str,
    provenance: Vec<String>,
}

fn derive_bestiary_deductions(
    reports: &[(adventuresim_core::bestiary::ReportDescription, String)],
    diagnostics: &[(adventuresim_core::bestiary::EvidenceKind, String, String)],
) -> Result<Vec<SafeBestiaryDeduction>, String> {
    let mut evidence = diagnostics
        .iter()
        .map(|(kind, receipt_id, _)| {
            Ok(adventuresim_core::investigation::VisibleEvidence {
                kind: *kind,
                evidence_id: adventuresim_core::investigation::EvidenceId::new(
                    receipt_id.to_owned(),
                )
                .map_err(|_| "Bestiary diagnostic receipt identity is invalid")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    evidence.sort_by(|left, right| left.evidence_id.cmp(&right.evidence_id));
    evidence.dedup_by(|left, right| left.evidence_id == right.evidence_id);
    let mut merged = BTreeMap::new();
    for (description, source_label) in reports {
        let inference = adventuresim_core::investigation::infer_threats(
            adventuresim_core::investigation::InferenceInput {
                report: adventuresim_core::investigation::VisibleReport {
                    description: *description,
                    visibility: adventuresim_core::bestiary::ObservationVisibility::Dim,
                    distance: adventuresim_core::bestiary::ObservationDistance::Medium,
                    capability: adventuresim_core::bestiary::WitnessCapability::Ordinary,
                    source_label: source_label.clone(),
                },
                evidence: evidence.clone(),
                region: adventuresim_core::bestiary::RegionalContext::NorthernGermany1544,
            },
        )
        .map_err(|_| "Bestiary inference inputs are invalid")?;
        for deduction in adventuresim_core::investigation::qualitative_deductions(&inference) {
            let candidate = (
                deduction.support,
                source_label.clone(),
                diagnostics
                    .iter()
                    .map(|(_, _, interpretation)| interpretation.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>(),
            );
            merged
                .entry(deduction.threat_id)
                .and_modify(
                    |existing: &mut (
                        adventuresim_core::investigation::DeductionSupport,
                        String,
                        Vec<String>,
                    )| {
                        if deduction_support_rank(candidate.0) > deduction_support_rank(existing.0)
                        {
                            *existing = candidate.clone();
                        }
                    },
                )
                .or_insert(candidate);
        }
    }
    Ok(merged
        .into_iter()
        .map(
            |(threat_id, (support, source_label, diagnostic_provenance))| {
                let mut provenance = vec![format!("received report from {source_label}")];
                provenance.extend(
                    diagnostic_provenance
                        .into_iter()
                        .map(|text| format!("learned diagnostic clue: {text}")),
                );
                SafeBestiaryDeduction {
                    threat_id,
                    support_band: deduction_support_label(support),
                    provenance,
                }
            },
        )
        .collect())
}

fn rebuild_bestiary_deductions(
    ctx: &ReducerContext,
    owner_character_id: u64,
    public_case_id: &str,
    now: u64,
) -> Result<(), String> {
    let reports = ctx
        .db
        .investigation_bestiary_report_receipt()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|receipt| receipt.public_case_id == public_case_id)
        .map(|receipt| {
            Ok((
                adventuresim_core::bestiary::ReportDescription::try_new(&receipt.description_id)
                    .map_err(|_| "Stored Bestiary report is invalid")?,
                receipt.source_label,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let diagnostics = ctx
        .db
        .investigation_bestiary_diagnostic_receipt()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|receipt| receipt.public_case_id == public_case_id)
        .map(|receipt| {
            Ok((
                adventuresim_core::bestiary::EvidenceKind::try_new(&receipt.diagnostic_kind)
                    .map_err(|_| "Stored Bestiary diagnostic is invalid")?,
                receipt.id,
                receipt.interpretation,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let deductions = derive_bestiary_deductions(&reports, &diagnostics)?;
    let old_ids = ctx
        .db
        .investigation_bestiary_deduction()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|row| row.public_case_id == public_case_id)
        .map(|row| row.id)
        .collect::<Vec<_>>();
    for id in old_ids {
        ctx.db.investigation_bestiary_deduction().id().delete(id);
    }
    for deduction in deductions {
        ctx.db
            .investigation_bestiary_deduction()
            .insert(InvestigationBestiaryDeduction {
                id: inv::compound_id(&[
                    "bestiary-deduction",
                    &owner_character_id.to_string(),
                    public_case_id,
                    deduction.threat_id.as_str(),
                ]),
                owner_character_id,
                public_case_id: public_case_id.into(),
                threat_id: deduction.threat_id.as_str().into(),
                support_band: deduction.support_band.into(),
                provenance_json: serde_json::to_string(&deduction.provenance)
                    .map_err(|_| "Bestiary provenance could not be persisted")?,
                updated_at: now,
            });
    }
    Ok(())
}

fn inspection_action_receipt_matches(
    receipt: &PhysicalEvidenceInspectionActionReceipt,
    owner_character_id: u64,
    evidence_id: &str,
    topic_id: &str,
) -> bool {
    receipt.owner_character_id == owner_character_id
        && receipt.evidence_id == evidence_id
        && receipt.topic_id == topic_id
}

fn merge_bestiary_lore_results(
    existing: Vec<PersistedBestiaryLoreResult>,
    newly_successful: Vec<PersistedBestiaryLoreResult>,
) -> (Vec<PersistedBestiaryLoreResult>, bool) {
    let mut by_kind = existing
        .into_iter()
        .map(|result| (result.diagnostic_kind.clone(), result))
        .collect::<BTreeMap<_, _>>();
    let before = by_kind.len();
    for result in newly_successful {
        by_kind
            .entry(result.diagnostic_kind.clone())
            .or_insert(result);
    }
    let changed = by_kind.len() != before;
    (by_kind.into_values().collect(), changed)
}

fn successful_bestiary_lore_results(
    implications: &[adventuresim_core::quest_generation::BestiaryEvidenceImplication],
    mut check_passes: impl FnMut(adventuresim_world_schema::BestiaryCategory, u16) -> bool,
) -> Vec<PersistedBestiaryLoreResult> {
    implications
        .iter()
        .filter_map(|implication| {
            (check_passes(implication.category, implication.lore_difficulty_milli))
                .then(|| {
                    implication.diagnostic_kind.as_ref().map(|diagnostic_kind| {
                        PersistedBestiaryLoreResult {
                            diagnostic_kind: diagnostic_kind.clone(),
                            interpretation: implication.interpretation.clone(),
                        }
                    })
                })
                .flatten()
        })
        .collect()
}

fn augment_physical_evidence_inspection(
    mut previous: PhysicalEvidenceInspectionAttempt,
    mut newly_successful: Vec<PersistedBestiaryLoreResult>,
) -> Result<(PhysicalEvidenceInspectionAttempt, bool), String> {
    if !previous.passed {
        newly_successful.clear();
    }
    let existing = parse_bestiary_lore_results(&previous.bestiary_results_json)?;
    let (merged, changed) = merge_bestiary_lore_results(existing, newly_successful);
    previous.bestiary_results_json = serde_json::to_string(&merged)
        .map_err(|_| "Bestiary inspection results could not be persisted")?;
    Ok((previous, changed))
}

fn bestiary_lore_results(
    ctx: &ReducerContext,
    character_id: u64,
    implications: &[adventuresim_core::quest_generation::BestiaryEvidenceImplication],
) -> Result<Vec<PersistedBestiaryLoreResult>, String> {
    if implications.is_empty() {
        return Ok(Vec::new());
    }
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Inspecting character has no attributes")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Inspecting character has no skills")?;
    let stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Inspecting character has no current stats")?;
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Inspecting character has no body state")?;
    Ok(successful_bestiary_lore_results(
        implications,
        |category, lore_difficulty_milli| {
            let check = adventuresim_core::capability::bestiary_knowledge_check(
                skills.bestiary_hours.effective(category),
                attributes.instinct,
                attributes.intelligence,
                stats.focus,
                limbs.head_health,
            );
            inspection_stat_milli(check).is_ok_and(|value| value >= lore_difficulty_milli)
        },
    ))
}

#[allow(dead_code)] // Owning investigation actions call this as evidence types are added.
pub(crate) fn record_evidence_knowledge(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
    evidence_id: &str,
    source_id: &str,
) -> Result<(), String> {
    let evidence = ctx
        .db
        .investigation_evidence_authority()
        .id()
        .find(&evidence_id.to_string())
        .ok_or("Evidence does not exist")?;
    if evidence.case_id != case_id {
        return Err("Evidence belongs to another case".into());
    }
    let id = inv::compound_id(&[
        "evidence-knowledge",
        &owner_character_id.to_string(),
        case_id,
        evidence_id,
    ]);
    if let Some(existing) = ctx.db.investigation_evidence_knowledge().id().find(&id) {
        return if existing.source_id == source_id {
            Ok(())
        } else {
            Err("Evidence knowledge has conflicting provenance".into())
        };
    }
    ctx.db
        .investigation_evidence_knowledge()
        .insert(InvestigationEvidenceKnowledge {
            id,
            owner_character_id,
            case_id: case_id.into(),
            evidence_id: evidence_id.into(),
            source_id: source_id.into(),
            learned_at: official_minute(ctx),
        });
    Ok(())
}

fn inspection_stat_milli(value: f32) -> Result<u16, String> {
    if !value.is_finite() || value < 0.0 {
        return Err("Inspection stat is invalid".into());
    }
    Ok((value * 1_000.0).round().clamp(0.0, f32::from(u16::MAX)) as u16)
}

#[reducer]
pub fn inspect_physical_evidence(
    ctx: &ReducerContext,
    character_id: u64,
    evidence_id: String,
    topic_id: String,
    action_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    if action_id.is_empty()
        || action_id.len() > 160
        || evidence_id.is_empty()
        || evidence_id.len() > 256
        || topic_id.is_empty()
        || topic_id.len() > 128
    {
        return Err("Physical-evidence inspection identifiers are invalid".into());
    }
    if let Some(existing) = ctx
        .db
        .physical_evidence_inspection_action_receipt()
        .action_id()
        .find(&action_id)
    {
        return if inspection_action_receipt_matches(
            &existing,
            character_id,
            &evidence_id,
            &topic_id,
        ) {
            Ok(())
        } else {
            Err("Physical-evidence inspection action ID was reused".into())
        };
    }
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .filter(|character| character.alive)
        .ok_or("Inspecting character does not exist or is dead")?;
    let authority = ctx
        .db
        .investigation_evidence_authority()
        .id()
        .find(&evidence_id)
        .filter(|row| row.presentation_kind == EvidencePresentationKind::Physical)
        .ok_or("Physical evidence does not exist")?;
    let generated = serde_json::from_str::<adventuresim_core::quest_generation::GeneratedEvidence>(
        &authority.authority_json,
    )
    .map_err(|_| "Physical evidence authority is invalid")?;
    if generated.id.0 != authority.id || generated.proposition_id != authority.proposition_id {
        return Err("Physical evidence authority does not match its generated manifest".into());
    }
    if character_case_site_id(ctx, actor.id).as_deref() != Some(generated.site_id.0.as_str()) {
        return Err("The party must occupy the evidence's authoritative site".into());
    }
    let topic = generated
        .inspection_topics
        .iter()
        .find(|topic| topic.id == topic_id)
        .ok_or("Unknown physical-evidence inspection topic")?;
    let inspection_id = inv::compound_id(&[
        "physical-evidence-inspection",
        &character_id.to_string(),
        &evidence_id,
        &topic_id,
    ]);
    let attempted_at = official_minute(ctx);
    let inspection = if let Some(previous) = ctx
        .db
        .physical_evidence_inspection_attempt()
        .id()
        .find(&inspection_id)
    {
        let newly_successful = if previous.passed {
            bestiary_lore_results(ctx, character_id, &topic.bestiary)?
        } else {
            Vec::new()
        };
        let (updated, changed) = augment_physical_evidence_inspection(previous, newly_successful)?;
        if changed {
            ctx.db
                .physical_evidence_inspection_attempt()
                .id()
                .update(updated.clone());
        }
        updated
    } else {
        let (stat_label, passed, narration) = match &topic.check {
            None => (String::new(), true, topic.inspection_description.clone()),
            Some(check) => {
                use adventuresim_core::quest_generation::EvidenceCheckStat;
                let attributes = ctx
                    .db
                    .character_attributes()
                    .character_id()
                    .find(character_id)
                    .ok_or("Inspecting character has no attributes")?;
                let value = match check.stat {
                    EvidenceCheckStat::Eyesight => attributes.eyesight,
                    EvidenceCheckStat::Intelligence => attributes.intelligence,
                    EvidenceCheckStat::Instinct => attributes.instinct,
                };
                let passed = adventuresim_core::quest_generation::evidence_check_passes(
                    inspection_stat_milli(value)?,
                    check.difficulty_milli,
                );
                let narration = if passed {
                    format!(
                        "{} check passed: {}",
                        check.stat.label(),
                        check.success_description
                    )
                } else {
                    format!(
                        "{} check failed: You cannot make out anything more.",
                        check.stat.label()
                    )
                };
                (check.stat.label().into(), passed, narration)
            }
        };
        let bestiary_results = if passed {
            bestiary_lore_results(ctx, character_id, &topic.bestiary)?
        } else {
            Vec::new()
        };
        let bestiary_results_json = serde_json::to_string(&bestiary_results)
            .map_err(|_| "Bestiary inspection results could not be persisted")?;
        ctx.db
            .physical_evidence_inspection_attempt()
            .insert(PhysicalEvidenceInspectionAttempt {
                id: inspection_id,
                owner_character_id: character_id,
                evidence_id: evidence_id.clone(),
                topic_id: topic_id.clone(),
                stat_label,
                passed,
                narration,
                bestiary_results_json,
                attempted_at,
            })
    };
    let reveals_clue = topic.check.as_ref().is_some_and(|check| check.reveals_clue);
    let has_bestiary_results =
        !parse_bestiary_lore_results(&inspection.bestiary_results_json)?.is_empty();
    if inspection.passed && has_bestiary_results {
        let public_case_id = ctx
            .db
            .quest_generation_authority()
            .case_id()
            .find(&authority.case_id)
            .map_or_else(|| authority.case_id.clone(), |row| row.public_case_id);
        for learned in parse_bestiary_lore_results(&inspection.bestiary_results_json)? {
            let id = inv::compound_id(&[
                "bestiary-diagnostic",
                &character_id.to_string(),
                &public_case_id,
                &evidence_id,
                &learned.diagnostic_kind,
            ]);
            if ctx
                .db
                .investigation_bestiary_diagnostic_receipt()
                .id()
                .find(&id)
                .is_none()
            {
                ctx.db.investigation_bestiary_diagnostic_receipt().insert(
                    InvestigationBestiaryDiagnosticReceipt {
                        id,
                        owner_character_id: character_id,
                        public_case_id: public_case_id.clone(),
                        diagnostic_kind: learned.diagnostic_kind,
                        interpretation: learned.interpretation,
                        learned_at: inspection.attempted_at,
                    },
                );
            }
        }
        rebuild_bestiary_deductions(ctx, character_id, &public_case_id, inspection.attempted_at)?;
    }
    if inspection.passed && reveals_clue {
        let source_id = format!("evidence-inspection:{evidence_id}:{topic_id}");
        record_evidence_knowledge(
            ctx,
            character_id,
            &authority.case_id,
            &evidence_id,
            &source_id,
        )?;
    }
    if inspection.passed && (reveals_clue || has_bestiary_results) {
        let source_id = format!("evidence-inspection:{evidence_id}:{topic_id}");
        let public_case_id = ctx
            .db
            .quest_generation_authority()
            .case_id()
            .find(&authority.case_id)
            .map_or_else(|| authority.case_id.clone(), |row| row.public_case_id);
        record_physical_evidence_journal_notice(
            ctx,
            character_id,
            &public_case_id,
            &source_id,
            &inspection.narration,
            &format!("physical evidence: {}", generated.portrait_label),
            inspection.attempted_at,
        )?;
    }
    ctx.db.physical_evidence_inspection_action_receipt().insert(
        PhysicalEvidenceInspectionActionReceipt {
            action_id,
            owner_character_id: character_id,
            evidence_id,
            topic_id,
        },
    );
    Ok(())
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_belief)]
pub struct InvestigationBelief {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    pub proposition_id: String,
    pub current_revision_id: String,
    pub statement: String,
    pub confidence_bps: u16,
    pub conflict_group: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_belief_revision)]
pub struct InvestigationBeliefRevision {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub belief_id: String,
    pub revision: u16,
    pub statement: String,
    pub confidence_bps: u16,
    pub provenance_kind: String,
    pub provenance_label: String,
    pub supersedes: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_lead)]
pub struct InvestigationLead {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    pub proposition_id: String,
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

/// Private physical authority for a strategic investigation site. Coordinates
/// never appear in a public table; observer-safe exact pins are projected by
/// the gateway view below only after an explicit exact disclosure.
#[derive(Clone, Debug)]
#[table(accessor = case_site_authority)]
pub struct CaseSiteAuthority {
    #[primary_key]
    pub id_key: String,
    pub id: CaseSiteId,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub origin_settlement_id: String,
    pub name: String,
    pub description: String,
    pub scene_key: String,
    pub longitude_e7: i32,
    pub latitude_e7: i32,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
}

/// Private geometry for an imprecise lead. Its canonical center and radius
/// never cross the gateway boundary as a map pin.
#[derive(Clone, Debug)]
#[table(accessor = investigation_area_authority)]
pub struct InvestigationAreaAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub origin_settlement_id: String,
    pub safe_label: String,
    pub center_longitude_e7: i32,
    pub center_latitude_e7: i32,
    pub radius_m: u32,
    pub coordinates_are_geographic: bool,
    pub terrain: String,
}

/// Observer-bound private action authority. `target_*`, the resolution seed,
/// and the server-authored consequence are intentionally absent from public
/// projections.
#[derive(Clone, Debug)]
#[table(accessor = investigation_action_capability)]
pub struct InvestigationActionCapability {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    /// Immutable private provenance. Generated capabilities never fall back to
    /// manual semantics when their generation authority is damaged or absent.
    pub provenance_kind: String,
    pub generated_case_id: String,
    pub method: String,
    pub version: u32,
    pub target_kind: String,
    pub target_id: String,
    pub target_terrain: String,
    pub seed: u64,
    pub evidence_age_origin_minute: u64,
    pub uncertainty_bps: u16,
    pub safe_summary: String,
    pub known_prerequisites: String,
    pub safe_result_on_success: String,
    pub consequence_json: String,
    pub required_action_id: String,
    pub alternate_route_action_id: String,
    pub active: bool,
}

/// Private typed output blueprint for generated capabilities. Free-form
/// result wording never grants evidence, custody, or destination knowledge.
#[derive(Clone, Debug)]
#[table(accessor = investigation_generated_action_output)]
pub struct InvestigationGeneratedActionOutput {
    #[primary_key]
    pub capability_id: String,
    pub outputs_json: String,
}

/// Private binding from an opaque learned cohort to one persistent NPC and
/// the exact demographic/presence facts authored at generation time.
#[derive(Clone, Debug)]
#[table(accessor = investigation_pattern_target_authority)]
pub struct InvestigationPatternTargetAuthority {
    #[primary_key]
    pub cohort_id: String,
    #[index(btree)]
    pub case_id: String,
    pub npc_id: String,
    pub demographic: String,
    pub age_band: String,
    pub sex: String,
    pub profession: String,
    pub expected_settlement_id: String,
    pub expected_location: String,
    pub presence_version: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_action_attempt)]
pub struct InvestigationActionAttempt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub capability_id: String,
    pub owner_character_id: u64,
    pub expected_version: u32,
    pub method: String,
    pub started_at: u64,
    pub completed_at: u64,
    pub duration_minutes: u32,
    pub success: bool,
    pub resulting_uncertainty_bps: u16,
    pub private_resolution_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_action_outcome)]
pub struct InvestigationActionOutcome {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub case_id: String,
    pub capability_id: String,
    /// Exact completed attempt that produced this outcome. Empty for
    /// non-attempt events such as dialogue contact and stale-capability refresh.
    pub attempt_id: String,
    pub safe_wording: String,
    /// Observer chronology used by owner-facing investigation projections.
    pub recorded_at: u64,
    /// Authoritative world chronology used only by server-side fairness rules.
    pub official_recorded_at: u64,
}

/// Private per-party presentation choice. Tracking does not accept a contract,
/// disclose knowledge, move a party, satisfy an objective, or award anything.
#[derive(Clone, Debug)]
#[table(accessor = party_case_site_tracking)]
pub struct PartyCaseSiteTracking {
    #[primary_key]
    pub party_id: String,
    pub observer_character_id: u64,
    pub case_site_id: CaseSiteId,
    pub tracked_at: u64,
}

/// Private physical occupancy. Public character rows deliberately contain no
/// case-site identifier.
#[derive(Clone, Debug)]
#[table(accessor = character_case_site_occupancy)]
pub struct CharacterCaseSiteOccupancy {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub case_site_id: CaseSiteId,
}

pub(crate) fn character_case_site_id(ctx: &ReducerContext, character_id: u64) -> Option<String> {
    ctx.db
        .character_case_site_occupancy()
        .character_id()
        .find(character_id)
        .map(|row| row.case_site_id.value)
}

pub(crate) fn set_character_case_site(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: Option<String>,
) {
    crate::outbreak::record_case_site_presence_transition(
        ctx,
        character_id,
        case_site_id.as_deref(),
    );
    if ctx
        .db
        .character_case_site_occupancy()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db
            .character_case_site_occupancy()
            .character_id()
            .delete(character_id);
    }
    if let Some(value) = case_site_id {
        ctx.db
            .character_case_site_occupancy()
            .insert(CharacterCaseSiteOccupancy {
                character_id,
                gateway_bucket: 0,
                case_site_id: CaseSiteId { value },
            });
    }
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_sharing_receipt)]
pub struct InvestigationSharingReceipt {
    #[primary_key]
    pub id: String,
    pub sender_id: u64,
    pub recipient_id: u64,
    pub source_record_id: String,
    pub payload_fingerprint: String,
    pub shared_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_action_receipt)]
pub struct InvestigationActionReceipt {
    #[primary_key]
    pub id: String,
    pub actor_id: u64,
    pub action_kind: String,
    pub canonical_payload: String,
    pub applied_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_safe_claim_receipt)]
pub struct InvestigationSafeClaimReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub claim_id: String,
    pub public_case_id: String,
    pub proposition_id: String,
    pub statement: String,
    pub safe_source_label: String,
    pub confidence_bps: u16,
    pub conflict_group: String,
    pub correction_of_belief_id: String,
    pub consumed_by: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_received_testimony)]
pub struct InvestigationReceivedTestimony {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub public_case_id: String,
    pub claim_id: String,
    pub witness_ref: String,
    pub source_receipt_id: String,
    pub received_at: u64,
}

/// Private observer knowledge that one exact generated witness has been
/// explicitly referred. Manifest membership alone never grants dialogue access.
#[derive(Clone, Debug)]
#[table(accessor = investigation_witness_referral)]
pub struct InvestigationWitnessReferral {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub canonical_case_id: String,
    pub public_case_id: String,
    pub witness_npc_id: String,
    pub expected_settlement_id: String,
    pub expected_location_id: String,
    pub grant_kind: String,
    pub source_receipt_id: String,
    pub source_witness_id: String,
    pub source_witness_npc_id: String,
    pub source_testimony_index: u32,
    pub source_proposition_id: String,
    pub catalog_revision: String,
    pub granted_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_testimony_bundle)]
pub struct InvestigationTestimonyBundle {
    #[primary_key]
    pub id: String,
    pub case_id: String,
    pub witness_ref: String,
    pub reliability_json: String,
    pub stages_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_safe_lead_receipt)]
pub struct InvestigationSafeLeadReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub public_case_id: String,
    pub summary: String,
    pub safe_source_label: String,
    pub confidence_bps: u16,
    pub destination_stage: String,
    pub directions: String,
    pub exact_location_id: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub conflict_group: String,
    pub correction_of_lead_id: String,
    pub consumed_by: String,
}

/// Dry observer-safe news about a case the character already knew. These rows
/// contain no inferred cause, probability, or suggested next action.
#[derive(Clone, Debug, PartialEq, Eq)]
#[table(accessor = investigation_journal_notice)]
pub struct InvestigationJournalNotice {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub public_case_id: String,
    pub source_id: String,
    pub summary: String,
    pub source_label: String,
    pub recorded_at: u64,
}

pub(crate) fn record_journal_notice(
    ctx: &ReducerContext,
    owner_character_id: u64,
    public_case_id: &str,
    source_id: &str,
    summary: &str,
    source_label: &str,
    recorded_at: u64,
) -> Result<(), String> {
    if public_case_id.is_empty()
        || source_id.is_empty()
        || summary.is_empty()
        || summary.len() > 1_024
        || source_label.is_empty()
        || source_label.len() > 160
    {
        return Err("Investigation journal notice is invalid".into());
    }
    let id = format!(
        "journal-notice:{owner_character_id}:{}",
        adventuresim_core::settlement_population::stable_hash(source_id)
    );
    if let Some(existing) = ctx.db.investigation_journal_notice().id().find(&id) {
        return if existing.owner_character_id == owner_character_id
            && existing.public_case_id == public_case_id
            && existing.source_id == source_id
            && existing.summary == summary
            && existing.source_label == source_label
            && existing.recorded_at == recorded_at
        {
            Ok(())
        } else {
            Err("Conflicting retry for investigation journal notice".into())
        };
    }
    ctx.db
        .investigation_journal_notice()
        .insert(InvestigationJournalNotice {
            id,
            owner_character_id,
            public_case_id: public_case_id.into(),
            source_id: source_id.into(),
            summary: summary.into(),
            source_label: source_label.into(),
            recorded_at,
        });
    Ok(())
}

pub(crate) fn upsert_public_threat_journal_notice(
    ctx: &ReducerContext,
    owner_character_id: u64,
    public_case_id: &str,
    summary: &str,
    source_label: &str,
    recorded_at: u64,
) -> Result<(), String> {
    if public_case_id.is_empty()
        || summary.is_empty()
        || summary.len() > 1_024
        || source_label.is_empty()
        || source_label.len() > 160
    {
        return Err("Public threat journal notice is invalid".into());
    }
    let id = adventuresim_core::threat_escalation::public_threat_journal_id(
        owner_character_id,
        public_case_id,
    );
    let notice = InvestigationJournalNotice {
        id: id.clone(),
        owner_character_id,
        public_case_id: public_case_id.into(),
        source_id: format!("public-threat:{public_case_id}"),
        summary: summary.into(),
        source_label: source_label.into(),
        recorded_at,
    };
    if let Some(existing) = ctx.db.investigation_journal_notice().id().find(&id) {
        if existing.owner_character_id != owner_character_id
            || existing.public_case_id != public_case_id
            || existing.source_id != notice.source_id
        {
            return Err("Public threat journal identity conflicts with authority".into());
        }
        if existing != notice {
            ctx.db.investigation_journal_notice().id().update(notice);
        }
    } else {
        ctx.db.investigation_journal_notice().insert(notice);
    }
    Ok(())
}

fn record_physical_evidence_journal_notice(
    ctx: &ReducerContext,
    owner_character_id: u64,
    public_case_id: &str,
    source_id: &str,
    summary: &str,
    source_label: &str,
    recorded_at: u64,
) -> Result<(), String> {
    record_journal_notice(
        ctx,
        owner_character_id,
        public_case_id,
        source_id,
        summary,
        source_label,
        recorded_at,
    )
}
