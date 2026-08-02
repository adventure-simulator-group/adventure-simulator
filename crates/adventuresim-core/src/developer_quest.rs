//! Typed developer authoring surface for complete generated investigation cases.
//!
//! This module deliberately keeps author input separate from `GeneratedCase`.
//! Observer-facing root IDs and settlement/time authority always come from the
//! server-owned generation context. The definition itself is persisted beside
//! that context so authority validation can replay it exactly.

use crate::{
    bestiary::ThreatId,
    case::{AssetId, ObjectiveExpression, ObjectiveId, ObjectiveRequirement, SubjectId},
    quest_generation::{
        self as qg, CanonicalCause, CanonicalEvent, CausalBridge, ConsequenceProfile,
        GeneratedAction, GeneratedArea, GeneratedCase, GeneratedDialogueProducer,
        GeneratedEvidence, GeneratedFinale, GeneratedPatternTarget, GeneratedSite,
        GenerationContext, SiteId, TrackSegment, TrackTrail, WitnessBinding,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DEVELOPER_QUEST_JSON_BYTES: usize = 512 * 1024;
pub const MAX_DEVELOPER_COLLECTION_ITEMS: usize = 64;
pub const MAX_DEVELOPER_TOTAL_ITEMS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticTier {
    Structural,
    Compatibility,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeveloperQuestDiagnostic {
    pub path: String,
    pub code: String,
    pub message: String,
    pub tier: DiagnosticTier,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeveloperGenerationContext {
    pub base: GenerationContext,
    pub definition: DeveloperQuestDefinition,
    pub allow_implausible: bool,
}

/// Complete practical declarative input for a generated investigation.
///
/// IDs inside repeated sections are author-local stable IDs. Root case,
/// problem, and public IDs are always minted from private observer entropy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeveloperQuestDefinition {
    pub template_id: String,
    pub configured_routes: Vec<String>,
    pub configured_objectives: Vec<String>,
    pub incident_interval_minutes: u64,
    pub maximum_incidents: u16,
    pub family: qg::TemplateFamily,
    pub cause: CanonicalCause,
    pub canonical_events: Vec<CanonicalEvent>,
    pub consequence: ConsequenceProfile,
    pub outbreak: Option<qg::GeneratedOutbreak>,
    pub sites: Vec<GeneratedSite>,
    pub areas: Vec<GeneratedArea>,
    pub witnesses: Vec<WitnessBinding>,
    pub pattern_targets: Vec<GeneratedPatternTarget>,
    pub evidence: Vec<GeneratedEvidence>,
    pub track_trails: Vec<TrackTrail>,
    pub track_segments: Vec<TrackSegment>,
    pub actions: Vec<GeneratedAction>,
    pub objectives: ObjectiveExpression,
    pub custody: Vec<(String, SiteId)>,
    pub hostile_groups: Vec<(String, SiteId, ThreatId, u32)>,
    pub finales: Vec<GeneratedFinale>,
    pub dialogue_producers: Vec<GeneratedDialogueProducer>,
    pub bridges: Vec<CausalBridge>,
}

impl DeveloperQuestDefinition {
    pub fn from_generated(case: GeneratedCase) -> Self {
        Self {
            template_id: case.template_id,
            configured_routes: case.configured_routes,
            configured_objectives: case.configured_objectives,
            incident_interval_minutes: case.incident_interval_minutes,
            maximum_incidents: case.maximum_incidents,
            family: case.family,
            cause: case.cause,
            canonical_events: case.canonical_events,
            consequence: case.consequence,
            outbreak: case.outbreak,
            sites: case.sites,
            areas: case.areas,
            witnesses: case.witnesses,
            pattern_targets: case.pattern_targets,
            evidence: case.evidence,
            track_trails: case.track_trails,
            track_segments: case.track_segments,
            actions: case.actions,
            objectives: case.objectives,
            custody: case.custody,
            hostile_groups: case.hostile_groups,
            finales: case.finales,
            dialogue_producers: case.dialogue_producers,
            bridges: case.bridges,
        }
    }
}

fn diagnostic(
    path: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
    tier: DiagnosticTier,
) -> DeveloperQuestDiagnostic {
    DeveloperQuestDiagnostic {
        path: path.into(),
        code: code.into(),
        message: message.into(),
        tier,
    }
}

pub fn parse_definition_json(
    json_text: &str,
) -> Result<DeveloperQuestDefinition, Vec<DeveloperQuestDiagnostic>> {
    if json_text.len() > MAX_DEVELOPER_QUEST_JSON_BYTES {
        return Err(vec![diagnostic(
            "$",
            "payload_too_large",
            format!(
                "Definition exceeds the {} byte limit",
                MAX_DEVELOPER_QUEST_JSON_BYTES
            ),
            DiagnosticTier::Structural,
        )]);
    }
    serde_json::from_str(json_text).map_err(|error| {
        vec![diagnostic(
            "$",
            "invalid_json",
            error.to_string(),
            DiagnosticTier::Structural,
        )]
    })
}

fn bounded_collection(diagnostics: &mut Vec<DeveloperQuestDiagnostic>, path: &str, len: usize) {
    if len > MAX_DEVELOPER_COLLECTION_ITEMS {
        diagnostics.push(diagnostic(
            path,
            "collection_too_large",
            format!("{path} contains {len} items; the limit is {MAX_DEVELOPER_COLLECTION_ITEMS}"),
            DiagnosticTier::Structural,
        ));
    }
}

fn validate_recursive_bounds(
    definition: &DeveloperQuestDefinition,
    diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
) {
    fn walk(
        value: &Value,
        path: &str,
        total: &mut usize,
        diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
    ) {
        match value {
            Value::Array(values) => {
                *total = total.saturating_add(values.len());
                bounded_collection(diagnostics, path, values.len());
                for (index, value) in values.iter().enumerate() {
                    walk(value, &format!("{path}.{index}"), total, diagnostics);
                }
            }
            Value::Object(values) => {
                for (key, value) in values {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    walk(value, &child, total, diagnostics);
                }
            }
            _ => {}
        }
    }
    match serde_json::to_value(definition) {
        Ok(value) => {
            let mut total = 0;
            walk(&value, "", &mut total, diagnostics);
            if total > MAX_DEVELOPER_TOTAL_ITEMS {
                diagnostics.push(diagnostic(
                    "$",
                    "definition_too_complex",
                    format!(
                        "Definition contains {total} repeated items; the total limit is {MAX_DEVELOPER_TOTAL_ITEMS}"
                    ),
                    DiagnosticTier::Structural,
                ));
            }
        }
        Err(error) => diagnostics.push(diagnostic(
            "$",
            "definition_encoding_failed",
            error.to_string(),
            DiagnosticTier::Structural,
        )),
    }
}

fn check_relation(
    diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
    catalog: &crate::quest_catalog::Catalog,
    supplied_bridges: &BTreeSet<&str>,
    path: String,
    relation_id: String,
    candidate_id: &str,
) {
    let Some(relation) = catalog.relation(&relation_id) else {
        return;
    };
    let Some(candidate) = relation
        .candidates
        .iter()
        .find(|candidate| candidate.id == candidate_id)
    else {
        diagnostics.push(diagnostic(
            path,
            "unauthored_catalog_combination",
            format!("{candidate_id} is not authored in relation {relation_id}"),
            DiagnosticTier::Compatibility,
        ));
        return;
    };
    if let Some(reason) = &candidate.hard_zero_reason {
        diagnostics.push(diagnostic(
            path.clone(),
            "catalog_hard_zero",
            reason,
            DiagnosticTier::Compatibility,
        ));
    } else if candidate.plausibility == 0 || candidate.curation == 0 {
        diagnostics.push(diagnostic(
            path.clone(),
            "zero_weight_combination",
            format!("{candidate_id} has zero authored weight in {relation_id}"),
            DiagnosticTier::Compatibility,
        ));
    }
    if let Some(required) = candidate.required_bridge.as_deref()
        && !supplied_bridges.contains(required)
    {
        diagnostics.push(diagnostic(
            path,
            "missing_required_bridge",
            format!("This combination requires causal bridge {required}"),
            DiagnosticTier::Compatibility,
        ));
    }
}

const INTERNAL_ID_PREFIXES: &[&str] = &[
    "action:",
    "area:",
    "asset:",
    "bridge:",
    "cohort:",
    "event:",
    "evidence:",
    "finale:",
    "group:",
    "hostile-group:",
    "objective:",
    "proposition:",
    "site:",
    "subject:",
    "track-segment:",
    "track-trail:",
    "witness:",
];

fn valid_local_id(value: &str) -> bool {
    value.len() <= 160
        && INTERNAL_ID_PREFIXES
            .iter()
            .any(|prefix| value.len() > prefix.len() && value.starts_with(prefix))
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b':' | b'-' | b'_' | b'.')
        })
}

fn declared_ids(definition: &DeveloperQuestDefinition) -> Vec<(String, &str)> {
    let mut ids = Vec::new();
    ids.extend(
        definition
            .track_trails
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("track_trails.{index}.id"), item.id.0.as_str())),
    );
    ids.extend(
        definition
            .track_segments
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("track_segments.{index}.id"), item.id.0.as_str())),
    );
    ids.extend(
        definition
            .canonical_events
            .iter()
            .enumerate()
            .flat_map(|(index, event)| {
                [
                    (format!("canonical_events.{index}.id"), event.id.as_str()),
                    (
                        format!("canonical_events.{index}.proposition_id"),
                        event.proposition_id.as_str(),
                    ),
                ]
            }),
    );
    ids.extend(
        definition
            .sites
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("sites.{index}.id"), item.id.0.as_str())),
    );
    ids.extend(
        definition
            .areas
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("areas.{index}.id"), item.id.as_str())),
    );
    ids.extend(
        definition
            .witnesses
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("witnesses.{index}.id"), item.id.0.as_str())),
    );
    ids.extend(
        definition
            .evidence
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("evidence.{index}.id"), item.id.0.as_str())),
    );
    ids.extend(
        definition
            .actions
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("actions.{index}.id"), item.id.0.as_str())),
    );
    let mut declared_subjects = BTreeSet::new();
    let mut declared_assets = BTreeSet::new();
    for (path_index, path) in definition.objectives.alternatives.iter().enumerate() {
        ids.extend(path.objectives.iter().enumerate().map(|(index, item)| {
            (
                format!("objectives.alternatives.{path_index}.objectives.{index}.id"),
                item.id.as_str(),
            )
        }));
    }
    ids.extend(
        definition
            .hostile_groups
            .iter()
            .enumerate()
            .map(|(index, (id, _, _, _))| (format!("hostile_groups.{index}.0"), id.as_str())),
    );
    ids.extend(
        definition
            .finales
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("finales.{index}.id"), item.id.0.as_str())),
    );
    ids.extend(
        definition
            .pattern_targets
            .iter()
            .enumerate()
            .map(|(index, item)| {
                (
                    format!("pattern_targets.{index}.cohort_id"),
                    item.cohort_id.as_str(),
                )
            }),
    );
    for (path_index, path) in definition.objectives.alternatives.iter().enumerate() {
        for (index, objective) in path.objectives.iter().enumerate() {
            let path =
                format!("objectives.alternatives.{path_index}.objectives.{index}.requirement");
            match &objective.requirement {
                ObjectiveRequirement::Capture { subject_id }
                | ObjectiveRequirement::Rescue { subject_id }
                | ObjectiveRequirement::EscortTo { subject_id, .. }
                | ObjectiveRequirement::Protect { subject_id, .. }
                | ObjectiveRequirement::Release { subject_id } => {
                    if declared_subjects.insert(subject_id.as_str()) {
                        ids.push((format!("{path}.subject_id"), subject_id.as_str()));
                    }
                }
                ObjectiveRequirement::Retrieve { asset_id }
                | ObjectiveRequirement::Return { asset_id, .. }
                | ObjectiveRequirement::Exchange { asset_id, .. } => {
                    if declared_assets.insert(asset_id.as_str()) {
                        ids.push((format!("{path}.asset_id"), asset_id.as_str()));
                    }
                }
                _ => {}
            }
        }
    }
    ids
}

#[derive(Default)]
struct DeclaredIdSets {
    all: BTreeSet<String>,
    actions: BTreeSet<String>,
    areas: BTreeSet<String>,
    assets: BTreeSet<String>,
    cohorts: BTreeSet<String>,
    events: BTreeSet<String>,
    evidence: BTreeSet<String>,
    finales: BTreeSet<String>,
    groups: BTreeSet<String>,
    objectives: BTreeSet<String>,
    propositions: BTreeSet<String>,
    sites: BTreeSet<String>,
    subjects: BTreeSet<String>,
    track_segments: BTreeSet<String>,
    track_trails: BTreeSet<String>,
    witnesses: BTreeSet<String>,
}

impl DeclaredIdSets {
    fn from_definition(definition: &DeveloperQuestDefinition) -> Self {
        let mut ids = Self::default();
        for event in &definition.canonical_events {
            ids.events.insert(event.id.clone());
            ids.propositions.insert(event.proposition_id.clone());
        }
        ids.sites
            .extend(definition.sites.iter().map(|item| item.id.0.clone()));
        ids.areas
            .extend(definition.areas.iter().map(|item| item.id.clone()));
        ids.witnesses
            .extend(definition.witnesses.iter().map(|item| item.id.0.clone()));
        ids.propositions.extend(
            definition
                .witnesses
                .iter()
                .flat_map(|witness| &witness.testimony)
                .map(|testimony| testimony.proposition_id.clone()),
        );
        ids.evidence
            .extend(definition.evidence.iter().map(|item| item.id.0.clone()));
        ids.propositions.extend(
            definition
                .evidence
                .iter()
                .map(|item| item.proposition_id.clone()),
        );
        ids.actions
            .extend(definition.actions.iter().map(|item| item.id.0.clone()));
        ids.track_trails
            .extend(definition.track_trails.iter().map(|item| item.id.0.clone()));
        ids.track_segments.extend(
            definition
                .track_segments
                .iter()
                .map(|item| item.id.0.clone()),
        );
        ids.groups.extend(
            definition
                .hostile_groups
                .iter()
                .map(|(id, _, _, _)| id.clone()),
        );
        ids.finales
            .extend(definition.finales.iter().map(|item| item.id.0.clone()));
        ids.cohorts.extend(
            definition
                .pattern_targets
                .iter()
                .map(|item| item.cohort_id.clone()),
        );
        if let Some(outbreak) = &definition.outbreak {
            ids.all.extend(
                outbreak
                    .exposure_chronology
                    .iter()
                    .map(|exposure| exposure.patient_ref.clone()),
            );
        }
        for path in &definition.objectives.alternatives {
            for objective in &path.objectives {
                ids.objectives.insert(objective.id.as_str().to_owned());
                match &objective.requirement {
                    ObjectiveRequirement::Capture { subject_id }
                    | ObjectiveRequirement::Rescue { subject_id }
                    | ObjectiveRequirement::EscortTo { subject_id, .. }
                    | ObjectiveRequirement::Protect { subject_id, .. }
                    | ObjectiveRequirement::Release { subject_id } => {
                        ids.subjects.insert(subject_id.as_str().to_owned());
                    }
                    ObjectiveRequirement::Retrieve { asset_id }
                    | ObjectiveRequirement::Return { asset_id, .. }
                    | ObjectiveRequirement::Exchange { asset_id, .. } => {
                        ids.assets.insert(asset_id.as_str().to_owned());
                    }
                    _ => {}
                }
            }
        }
        for set in [
            &ids.actions,
            &ids.areas,
            &ids.assets,
            &ids.cohorts,
            &ids.events,
            &ids.evidence,
            &ids.finales,
            &ids.groups,
            &ids.objectives,
            &ids.propositions,
            &ids.sites,
            &ids.subjects,
            &ids.track_segments,
            &ids.track_trails,
            &ids.witnesses,
        ] {
            ids.all.extend(set.iter().cloned());
        }
        ids
    }
}

fn missing_reference(
    diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
    path: impl Into<String>,
    value: &str,
    expected: &BTreeSet<String>,
    kind: &str,
) {
    if !expected.contains(value) {
        diagnostics.push(diagnostic(
            path,
            "unknown_reference",
            format!("{kind} reference {value} is not declared"),
            DiagnosticTier::Structural,
        ));
    }
}

fn validate_references(
    definition: &DeveloperQuestDefinition,
    witness_candidates: &[qg::WitnessCandidate],
    ids: &DeclaredIdSets,
    diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
) {
    let resident_character_ids = witness_candidates
        .iter()
        .map(|candidate| candidate.resident_character_id.to_string())
        .collect::<BTreeSet<_>>();
    for (index, area) in definition.areas.iter().enumerate() {
        for (site_index, site_id) in area.contains_site_ids.iter().enumerate() {
            missing_reference(
                diagnostics,
                format!("areas.{index}.contains_site_ids.{site_index}"),
                &site_id.0,
                &ids.sites,
                "site",
            );
        }
    }
    for (witness_index, witness) in definition.witnesses.iter().enumerate() {
        for (testimony_index, testimony) in witness.testimony.iter().enumerate() {
            let base = format!("witnesses.{witness_index}.testimony.{testimony_index}");
            missing_reference(
                diagnostics,
                format!("{base}.proposition_id"),
                &testimony.proposition_id,
                &ids.propositions,
                "proposition",
            );
            if let Some(site_id) = &testimony.site_id {
                missing_reference(
                    diagnostics,
                    format!("{base}.site_id"),
                    &site_id.0,
                    &ids.sites,
                    "site",
                );
            }
            if let Some(proposition_id) = &testimony.corrects_proposition_id {
                missing_reference(
                    diagnostics,
                    format!("{base}.corrects_proposition_id"),
                    proposition_id,
                    &ids.propositions,
                    "proposition",
                );
            }
            for (referral_index, witness_id) in testimony.referred_witness_ids.iter().enumerate() {
                missing_reference(
                    diagnostics,
                    format!("{base}.referred_witness_ids.{referral_index}"),
                    &witness_id.0,
                    &ids.witnesses,
                    "witness",
                );
            }
        }
    }
    for (index, evidence) in definition.evidence.iter().enumerate() {
        missing_reference(
            diagnostics,
            format!("evidence.{index}.proposition_id"),
            &evidence.proposition_id,
            &ids.propositions,
            "proposition",
        );
        missing_reference(
            diagnostics,
            format!("evidence.{index}.site_id"),
            &evidence.site_id.0,
            &ids.sites,
            "site",
        );
        if let Some(proposition_id) = &evidence.corrects_proposition_id {
            missing_reference(
                diagnostics,
                format!("evidence.{index}.corrects_proposition_id"),
                proposition_id,
                &ids.propositions,
                "proposition",
            );
        }
    }
    for (index, trail) in definition.track_trails.iter().enumerate() {
        for (segment_index, segment_id) in trail.segment_ids.iter().enumerate() {
            missing_reference(
                diagnostics,
                format!("track_trails.{index}.segment_ids.{segment_index}"),
                &segment_id.0,
                &ids.track_segments,
                "track segment",
            );
        }
    }
    for (index, segment) in definition.track_segments.iter().enumerate() {
        missing_reference(
            diagnostics,
            format!("track_segments.{index}.trail_id"),
            &segment.trail_id.0,
            &ids.track_trails,
            "track trail",
        );
        for (field, linked) in [
            ("predecessor", segment.predecessor.as_ref()),
            ("next", segment.next.as_ref()),
        ] {
            if let Some(linked) = linked {
                missing_reference(
                    diagnostics,
                    format!("track_segments.{index}.{field}"),
                    &linked.0,
                    &ids.track_segments,
                    "track segment",
                );
            }
        }
    }
    for (index, action) in definition.actions.iter().enumerate() {
        let base = format!("actions.{index}");
        if let Some(segment_id) = &action.track_segment_id {
            missing_reference(
                diagnostics,
                format!("{base}.track_segment_id"),
                &segment_id.0,
                &ids.track_segments,
                "track segment",
            );
        }
        if let Some(prerequisite) = &action.prerequisite {
            missing_reference(
                diagnostics,
                format!("{base}.prerequisite"),
                &prerequisite.0,
                &ids.actions,
                "action",
            );
        }
        missing_reference(
            diagnostics,
            format!("{base}.alternate"),
            &action.alternate.0,
            &ids.actions,
            "action",
        );
        match action.target_kind.as_str() {
            "site" => missing_reference(
                diagnostics,
                format!("{base}.target_id"),
                &action.target_id,
                &ids.sites,
                "site",
            ),
            "area" => missing_reference(
                diagnostics,
                format!("{base}.target_id"),
                &action.target_id,
                &ids.areas,
                "area",
            ),
            "contact" => missing_reference(
                diagnostics,
                format!("{base}.target_id"),
                &action.target_id,
                &resident_character_ids,
                "NPC",
            ),
            "cohort" => missing_reference(
                diagnostics,
                format!("{base}.target_id"),
                &action.target_id,
                &ids.cohorts,
                "cohort",
            ),
            "route" => missing_reference(
                diagnostics,
                format!("{base}.target_id"),
                &action.target_id,
                &ids.sites,
                "site",
            ),
            _ => diagnostics.push(diagnostic(
                format!("{base}.target_kind"),
                "unknown_target_kind",
                "Action target_kind must be site, area, contact, cohort, or route",
                DiagnosticTier::Structural,
            )),
        }
        for (output_index, output) in action.outputs.iter().enumerate() {
            let path = format!("{base}.outputs.{output_index}");
            match output {
                qg::GeneratedActionOutput::Destination { site_id, .. } => {
                    if let Some(site_id) = site_id {
                        missing_reference(
                            diagnostics,
                            format!("{path}.site_id"),
                            &site_id.0,
                            &ids.sites,
                            "site",
                        );
                    }
                }
                qg::GeneratedActionOutput::Evidence { evidence_id } => missing_reference(
                    diagnostics,
                    format!("{path}.evidence_id"),
                    &evidence_id.0,
                    &ids.evidence,
                    "evidence",
                ),
                qg::GeneratedActionOutput::PatternCondition {
                    evidence_id,
                    condition,
                } => {
                    missing_reference(
                        diagnostics,
                        format!("{path}.evidence_id"),
                        &evidence_id.0,
                        &ids.evidence,
                        "evidence",
                    );
                    if let qg::GeneratedPatternCondition::VictimProfile { cohort_id, .. } =
                        condition
                    {
                        missing_reference(
                            diagnostics,
                            format!("{path}.condition.cohort_id"),
                            cohort_id,
                            &ids.cohorts,
                            "cohort",
                        );
                    }
                }
                qg::GeneratedActionOutput::TrackFinding { segment_id, .. } => {
                    missing_reference(
                        diagnostics,
                        format!("{path}.segment_id"),
                        &segment_id.0,
                        &ids.track_segments,
                        "track segment",
                    );
                }
                qg::GeneratedActionOutput::Consequence { consequence } => match consequence {
                    qg::GeneratedActionConsequence::RetrieveAsset { asset_id, .. } => {
                        missing_reference(
                            diagnostics,
                            format!("{path}.consequence.asset_id"),
                            asset_id,
                            &ids.assets,
                            "asset",
                        );
                    }
                    qg::GeneratedActionConsequence::RescueSubject { subject_id, .. } => {
                        missing_reference(
                            diagnostics,
                            format!("{path}.consequence.subject_id"),
                            subject_id,
                            &ids.subjects,
                            "subject",
                        );
                    }
                },
                qg::GeneratedActionOutput::AmbushReady
                | qg::GeneratedActionOutput::Remediation { .. }
                | qg::GeneratedActionOutput::SystemicOutcome { .. } => {}
            }
        }
    }
    for (path_index, path) in definition.objectives.alternatives.iter().enumerate() {
        for (index, objective) in path.objectives.iter().enumerate() {
            let base =
                format!("objectives.alternatives.{path_index}.objectives.{index}.requirement");
            match &objective.requirement {
                ObjectiveRequirement::Defeat {
                    hostile_group_id, ..
                }
                | ObjectiveRequirement::DriveOff { hostile_group_id } => missing_reference(
                    diagnostics,
                    format!("{base}.hostile_group_id"),
                    hostile_group_id,
                    &ids.groups,
                    "hostile group",
                ),
                ObjectiveRequirement::SurviveWindow { site_id, .. } => missing_reference(
                    diagnostics,
                    format!("{base}.site_id"),
                    site_id,
                    &ids.sites,
                    "site",
                ),
                ObjectiveRequirement::EscortTo { site_id, .. } => missing_reference(
                    diagnostics,
                    format!("{base}.site_id"),
                    site_id,
                    &ids.sites,
                    "site",
                ),
                ObjectiveRequirement::Locate { subject_ref }
                | ObjectiveRequirement::Identify { subject_ref }
                | ObjectiveRequirement::Expose { subject_ref }
                | ObjectiveRequirement::Negotiate { subject_ref } => missing_reference(
                    diagnostics,
                    format!("{base}.subject_ref"),
                    subject_ref,
                    &ids.all,
                    "internal subject",
                ),
                ObjectiveRequirement::RemediateSource { .. } => {}
                ObjectiveRequirement::PresentProof { evidence_id, .. } => missing_reference(
                    diagnostics,
                    format!("{base}.evidence_id"),
                    evidence_id,
                    &ids.evidence,
                    "evidence",
                ),
                ObjectiveRequirement::PresentTestimony { witness_id, .. } => missing_reference(
                    diagnostics,
                    format!("{base}.witness_id"),
                    witness_id,
                    &ids.witnesses,
                    "witness",
                ),
                _ => {}
            }
        }
    }
    for (index, (object_id, site_id)) in definition.custody.iter().enumerate() {
        missing_reference(
            diagnostics,
            format!("custody.{index}.0"),
            object_id,
            &ids.all,
            "custody object",
        );
        missing_reference(
            diagnostics,
            format!("custody.{index}.1"),
            &site_id.0,
            &ids.sites,
            "site",
        );
    }
    for (index, (_, site_id, _, _)) in definition.hostile_groups.iter().enumerate() {
        missing_reference(
            diagnostics,
            format!("hostile_groups.{index}.1"),
            &site_id.0,
            &ids.sites,
            "site",
        );
    }
    for (index, finale) in definition.finales.iter().enumerate() {
        missing_reference(
            diagnostics,
            format!("finales.{index}.site_id"),
            &finale.site_id.0,
            &ids.sites,
            "site",
        );
        if let Some(group_id) = &finale.hostile_group_id {
            missing_reference(
                diagnostics,
                format!("finales.{index}.hostile_group_id"),
                group_id,
                &ids.groups,
                "hostile group",
            );
        }
        if let Some(subject_id) = &finale.subject_id {
            missing_reference(
                diagnostics,
                format!("finales.{index}.subject_id"),
                subject_id,
                &ids.subjects,
                "subject",
            );
        }
        if let Some(asset_id) = &finale.asset_id {
            missing_reference(
                diagnostics,
                format!("finales.{index}.asset_id"),
                asset_id,
                &ids.assets,
                "asset",
            );
        }
    }
    for (index, producer) in definition.dialogue_producers.iter().enumerate() {
        missing_reference(
            diagnostics,
            format!("dialogue_producers.{index}.objective_id"),
            producer.objective_id.as_str(),
            &ids.objectives,
            "objective",
        );
        let recipient_character_id = producer.recipient_resident_character_id.to_string();
        missing_reference(
            diagnostics,
            format!("dialogue_producers.{index}.recipient_resident_character_id"),
            &recipient_character_id,
            &resident_character_ids,
            "NPC",
        );
        if let Some(subject_ref) = &producer.subject_ref {
            missing_reference(
                diagnostics,
                format!("dialogue_producers.{index}.subject_ref"),
                subject_ref,
                &ids.all,
                "internal subject",
            );
        }
        if let Some(asset_id) = &producer.asset_id {
            missing_reference(
                diagnostics,
                format!("dialogue_producers.{index}.asset_id"),
                asset_id,
                &ids.assets,
                "asset",
            );
        }
    }
    for (index, bridge) in definition.bridges.iter().enumerate() {
        missing_reference(
            diagnostics,
            format!("bridges.{index}.event_id"),
            &bridge.event_id,
            &ids.events,
            "event",
        );
        missing_reference(
            diagnostics,
            format!("bridges.{index}.evidence_id"),
            &bridge.evidence_id.0,
            &ids.evidence,
            "evidence",
        );
        missing_reference(
            diagnostics,
            format!("bridges.{index}.action_id"),
            &bridge.action_id.0,
            &ids.actions,
            "action",
        );
    }
}

fn validate_local_ids(
    definition: &DeveloperQuestDefinition,
    diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
) {
    let mut seen = BTreeMap::<&str, String>::new();
    for (path, id) in declared_ids(definition) {
        if !valid_local_id(id) {
            diagnostics.push(diagnostic(
                path.clone(),
                "invalid_local_id",
                "Author-local IDs require a supported kind prefix and bounded lowercase identifier characters",
                DiagnosticTier::Structural,
            ));
        }
        if let Some(first_path) = seen.insert(id, path.clone()) {
            diagnostics.push(diagnostic(
                path,
                "duplicate_local_id",
                format!("ID duplicates {first_path}"),
                DiagnosticTier::Structural,
            ));
        }
    }
    for (witness_index, witness) in definition.witnesses.iter().enumerate() {
        for (testimony_index, testimony) in witness.testimony.iter().enumerate() {
            if !valid_local_id(&testimony.proposition_id) {
                diagnostics.push(diagnostic(
                    format!(
                        "witnesses.{witness_index}.testimony.{testimony_index}.proposition_id"
                    ),
                    "invalid_local_id",
                    "Author-local proposition IDs require a supported kind prefix and bounded lowercase identifier characters",
                    DiagnosticTier::Structural,
                ));
            }
        }
    }
    for (index, evidence) in definition.evidence.iter().enumerate() {
        if !valid_local_id(&evidence.proposition_id) {
            diagnostics.push(diagnostic(
                format!("evidence.{index}.proposition_id"),
                "invalid_local_id",
                "Author-local proposition IDs require a supported kind prefix and bounded lowercase identifier characters",
                DiagnosticTier::Structural,
            ));
        }
    }
}

fn validate_custody(
    definition: &DeveloperQuestDefinition,
    diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
) {
    use crate::case::ObjectiveRequirement as R;
    let mut required = BTreeMap::<String, &'static str>::new();
    for objective in definition
        .objectives
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let object = match &objective.requirement {
            R::Retrieve { asset_id }
            | R::Return { asset_id, .. }
            | R::Exchange { asset_id, .. } => Some((asset_id.as_str(), "asset")),
            R::Capture { subject_id }
            | R::Rescue { subject_id }
            | R::EscortTo { subject_id, .. }
            | R::Protect { subject_id, .. }
            | R::Release { subject_id } => Some((subject_id.as_str(), "subject")),
            _ => None,
        };
        if let Some((id, kind)) = object {
            if let Some(existing) = required.insert(id.to_owned(), kind)
                && existing != kind
            {
                diagnostics.push(diagnostic(
                    "objectives",
                    "ambiguous_custody_kind",
                    format!("{id} is used as both an asset and subject"),
                    DiagnosticTier::Structural,
                ));
            }
        }
    }
    let mut supplied = BTreeSet::new();
    for (index, (object_id, _)) in definition.custody.iter().enumerate() {
        if !supplied.insert(object_id.as_str()) {
            diagnostics.push(diagnostic(
                format!("custody.{index}.0"),
                "duplicate_custody_object",
                "Custody contains the same object more than once",
                DiagnosticTier::Structural,
            ));
        }
        if !required.contains_key(object_id) {
            diagnostics.push(diagnostic(
                format!("custody.{index}.0"),
                "custody_object_without_objective",
                "Custody object is not typed by any objective leaf",
                DiagnosticTier::Structural,
            ));
        }
    }
    for object_id in required.keys() {
        if !supplied.contains(object_id.as_str()) {
            diagnostics.push(diagnostic(
                "custody",
                "missing_custody_object",
                format!("Objective custody object {object_id} has no authored starting site"),
                DiagnosticTier::Structural,
            ));
        }
    }
}

fn reject_unsupported_challenge_objectives(
    definition: &DeveloperQuestDefinition,
    diagnostics: &mut Vec<DeveloperQuestDiagnostic>,
) {
    for (path_index, path) in definition.objectives.alternatives.iter().enumerate() {
        for (objective_index, objective) in path.objectives.iter().enumerate() {
            if matches!(
                &objective.requirement,
                ObjectiveRequirement::SolveChallenge { .. }
            ) {
                diagnostics.push(diagnostic(
                    format!(
                        "objectives.alternatives.{path_index}.objectives.{objective_index}.requirement"
                    ),
                    "unsupported_challenge_objective",
                    "Investigation developer quests cannot author challenge objectives until challenge declarations and materialization are supported",
                    DiagnosticTier::Structural,
                ));
            }
        }
    }
}

fn namespace_definition(
    base: &GenerationContext,
    definition: &DeveloperQuestDefinition,
) -> Result<DeveloperQuestDefinition, DeveloperQuestDiagnostic> {
    fn remap(value: &mut String, replacements: &BTreeMap<String, String>) {
        if let Some(replacement) = replacements.get(value) {
            *value = replacement.clone();
        }
    }
    fn mapped(
        value: &str,
        replacements: &BTreeMap<String, String>,
    ) -> Result<String, DeveloperQuestDiagnostic> {
        replacements.get(value).cloned().ok_or_else(|| {
            diagnostic(
                "$",
                "definition_namespacing_failed",
                format!("Validated internal ID {value} has no namespace mapping"),
                DiagnosticTier::Structural,
            )
        })
    }

    let ids = DeclaredIdSets::from_definition(definition);
    let replacements = ids
        .all
        .iter()
        .map(|id| {
            let kind = id.split_once(':').map_or("internal", |(kind, _)| kind);
            let replacement = qg::observer_scoped_id(base, kind, &id);
            (id.clone(), replacement)
        })
        .collect::<BTreeMap<_, _>>();
    let mut materialized = definition.clone();

    if let Some(outbreak) = &mut materialized.outbreak {
        remap(&mut outbreak.physical_source_site.0, &replacements);
        remap(&mut outbreak.patient_presentation_site.0, &replacements);
        for exposure in &mut outbreak.exposure_chronology {
            remap(&mut exposure.patient_ref, &replacements);
        }
        if let qg::OutbreakRemediation::ResolveCarrierThreat {
            hostile_group_id, ..
        } = &mut outbreak.remediation
        {
            remap(hostile_group_id, &replacements);
        }
    }

    for event in &mut materialized.canonical_events {
        remap(&mut event.id, &replacements);
        remap(&mut event.proposition_id, &replacements);
        // These are semantic event operands rather than prose. Only declared
        // internal operands are scoped; arbitrary canonical values are kept.
        remap(&mut event.subject, &replacements);
        remap(&mut event.object, &replacements);
    }
    for site in &mut materialized.sites {
        remap(&mut site.id.0, &replacements);
    }
    for area in &mut materialized.areas {
        remap(&mut area.id, &replacements);
        for site_id in &mut area.contains_site_ids {
            remap(&mut site_id.0, &replacements);
        }
    }
    for witness in &mut materialized.witnesses {
        remap(&mut witness.id.0, &replacements);
        for testimony in &mut witness.testimony {
            remap(&mut testimony.proposition_id, &replacements);
            if let Some(site_id) = &mut testimony.site_id {
                remap(&mut site_id.0, &replacements);
            }
            if let Some(proposition_id) = &mut testimony.corrects_proposition_id {
                remap(proposition_id, &replacements);
            }
            for witness_id in &mut testimony.referred_witness_ids {
                remap(&mut witness_id.0, &replacements);
            }
        }
    }
    for target in &mut materialized.pattern_targets {
        remap(&mut target.cohort_id, &replacements);
    }
    for evidence in &mut materialized.evidence {
        remap(&mut evidence.id.0, &replacements);
        remap(&mut evidence.proposition_id, &replacements);
        remap(&mut evidence.site_id.0, &replacements);
        if let Some(proposition_id) = &mut evidence.corrects_proposition_id {
            remap(proposition_id, &replacements);
        }
    }
    for trail in &mut materialized.track_trails {
        remap(&mut trail.id.0, &replacements);
        for segment_id in &mut trail.segment_ids {
            remap(&mut segment_id.0, &replacements);
        }
    }
    for segment in &mut materialized.track_segments {
        remap(&mut segment.id.0, &replacements);
        remap(&mut segment.trail_id.0, &replacements);
        if let Some(predecessor) = &mut segment.predecessor {
            remap(&mut predecessor.0, &replacements);
        }
        if let Some(next) = &mut segment.next {
            remap(&mut next.0, &replacements);
        }
    }
    for action in &mut materialized.actions {
        remap(&mut action.id.0, &replacements);
        if matches!(
            action.target_kind.as_str(),
            "site" | "area" | "cohort" | "route"
        ) {
            remap(&mut action.target_id, &replacements);
        }
        if let Some(prerequisite) = &mut action.prerequisite {
            remap(&mut prerequisite.0, &replacements);
        }
        remap(&mut action.alternate.0, &replacements);
        if let Some(segment_id) = &mut action.track_segment_id {
            remap(&mut segment_id.0, &replacements);
        }
        for output in &mut action.outputs {
            match output {
                qg::GeneratedActionOutput::Destination { site_id, .. } => {
                    if let Some(site_id) = site_id {
                        remap(&mut site_id.0, &replacements);
                    }
                }
                qg::GeneratedActionOutput::Evidence { evidence_id } => {
                    remap(&mut evidence_id.0, &replacements);
                }
                qg::GeneratedActionOutput::PatternCondition {
                    evidence_id,
                    condition,
                } => {
                    remap(&mut evidence_id.0, &replacements);
                    if let qg::GeneratedPatternCondition::VictimProfile { cohort_id, .. } =
                        condition
                    {
                        remap(cohort_id, &replacements);
                    }
                }
                qg::GeneratedActionOutput::TrackFinding { segment_id, .. } => {
                    remap(&mut segment_id.0, &replacements);
                }
                qg::GeneratedActionOutput::Consequence { consequence } => match consequence {
                    qg::GeneratedActionConsequence::RetrieveAsset { asset_id, .. } => {
                        remap(asset_id, &replacements);
                    }
                    qg::GeneratedActionConsequence::RescueSubject { subject_id, .. } => {
                        remap(subject_id, &replacements);
                    }
                },
                qg::GeneratedActionOutput::AmbushReady => {}
                qg::GeneratedActionOutput::Remediation { remediation_id } => {
                    remap(remediation_id, &replacements);
                }
                qg::GeneratedActionOutput::SystemicOutcome { outcome } => match outcome {
                    qg::GeneratedSystemicOutcome::Surrender { context_id, .. } => {
                        remap(context_id, &replacements)
                    }
                    qg::GeneratedSystemicOutcome::RecruitOrDefect { party_id, .. } => {
                        remap(party_id, &replacements)
                    }
                    qg::GeneratedSystemicOutcome::Ransom { recipient_id, .. } => {
                        remap(recipient_id, &replacements)
                    }
                    qg::GeneratedSystemicOutcome::CustodyHandoff { custodian_id, .. } => {
                        remap(custodian_id, &replacements)
                    }
                    qg::GeneratedSystemicOutcome::EscapeCustody { .. } => {}
                    qg::GeneratedSystemicOutcome::TransferOwnership {
                        property_id,
                        owner_id,
                    } => {
                        remap(property_id, &replacements);
                        remap(owner_id, &replacements);
                    }
                    qg::GeneratedSystemicOutcome::Theft {
                        property_id,
                        victim_id,
                    } => {
                        remap(property_id, &replacements);
                        remap(victim_id, &replacements);
                    }
                },
            }
        }
    }
    for path in &mut materialized.objectives.alternatives {
        for objective in &mut path.objectives {
            objective.id = ObjectiveId::new(mapped(objective.id.as_str(), &replacements)?)
                .map_err(|_| {
                    diagnostic(
                        "$",
                        "definition_namespacing_failed",
                        "Namespaced objective ID was invalid",
                        DiagnosticTier::Structural,
                    )
                })?;
            match &mut objective.requirement {
                ObjectiveRequirement::Defeat {
                    hostile_group_id, ..
                }
                | ObjectiveRequirement::DriveOff { hostile_group_id } => {
                    remap(hostile_group_id, &replacements);
                }
                ObjectiveRequirement::Capture { subject_id }
                | ObjectiveRequirement::Rescue { subject_id }
                | ObjectiveRequirement::Protect { subject_id, .. }
                | ObjectiveRequirement::Release { subject_id } => {
                    *subject_id = SubjectId::new(mapped(subject_id.as_str(), &replacements)?)
                        .map_err(|_| {
                            diagnostic(
                                "$",
                                "definition_namespacing_failed",
                                "Namespaced subject ID was invalid",
                                DiagnosticTier::Structural,
                            )
                        })?;
                }
                ObjectiveRequirement::EscortTo {
                    subject_id,
                    site_id,
                } => {
                    *subject_id = SubjectId::new(mapped(subject_id.as_str(), &replacements)?)
                        .map_err(|_| {
                            diagnostic(
                                "$",
                                "definition_namespacing_failed",
                                "Namespaced subject ID was invalid",
                                DiagnosticTier::Structural,
                            )
                        })?;
                    remap(site_id, &replacements);
                }
                ObjectiveRequirement::Retrieve { asset_id }
                | ObjectiveRequirement::Return { asset_id, .. }
                | ObjectiveRequirement::Exchange { asset_id, .. } => {
                    *asset_id =
                        AssetId::new(mapped(asset_id.as_str(), &replacements)?).map_err(|_| {
                            diagnostic(
                                "$",
                                "definition_namespacing_failed",
                                "Namespaced asset ID was invalid",
                                DiagnosticTier::Structural,
                            )
                        })?;
                }
                ObjectiveRequirement::SurviveWindow { site_id, .. } => {
                    remap(site_id, &replacements);
                }
                ObjectiveRequirement::Locate { subject_ref }
                | ObjectiveRequirement::Identify { subject_ref }
                | ObjectiveRequirement::Expose { subject_ref }
                | ObjectiveRequirement::Negotiate { subject_ref } => {
                    remap(subject_ref, &replacements);
                }
                ObjectiveRequirement::RemediateSource { remediation_id } => {
                    remap(remediation_id, &replacements);
                }
                // Rejected structurally before materialization: developer
                // investigation definitions cannot declare challenge authority.
                ObjectiveRequirement::SolveChallenge { .. } => {}
                ObjectiveRequirement::PresentProof { evidence_id, .. } => {
                    remap(evidence_id, &replacements);
                }
                ObjectiveRequirement::PresentTestimony { witness_id, .. } => {
                    remap(witness_id, &replacements);
                }
                ObjectiveRequirement::ReportToIssuer { .. } => {}
                ObjectiveRequirement::Surrender { context_id, .. } => {
                    remap(context_id, &replacements)
                }
                ObjectiveRequirement::RecruitOrDefect { party_id, .. } => {
                    remap(party_id, &replacements)
                }
                ObjectiveRequirement::Ransom { recipient_id, .. } => {
                    remap(recipient_id, &replacements)
                }
                ObjectiveRequirement::CustodyHandoff { custodian_id, .. } => {
                    remap(custodian_id, &replacements)
                }
                ObjectiveRequirement::EscapeCustody { .. } => {}
                ObjectiveRequirement::TransferOwnership {
                    property_id,
                    owner_id,
                } => {
                    remap(property_id, &replacements);
                    remap(owner_id, &replacements);
                }
                ObjectiveRequirement::CommitTheft {
                    property_id,
                    victim_id,
                } => {
                    remap(property_id, &replacements);
                    remap(victim_id, &replacements);
                }
            }
        }
    }
    for (object_id, site_id) in &mut materialized.custody {
        remap(object_id, &replacements);
        remap(&mut site_id.0, &replacements);
    }
    for (group_id, site_id, _, _) in &mut materialized.hostile_groups {
        remap(group_id, &replacements);
        remap(&mut site_id.0, &replacements);
    }
    for finale in &mut materialized.finales {
        remap(&mut finale.id.0, &replacements);
        remap(&mut finale.site_id.0, &replacements);
        if let Some(group_id) = &mut finale.hostile_group_id {
            remap(group_id, &replacements);
        }
        if let Some(subject_id) = &mut finale.subject_id {
            remap(subject_id, &replacements);
        }
        if let Some(asset_id) = &mut finale.asset_id {
            remap(asset_id, &replacements);
        }
    }
    for producer in &mut materialized.dialogue_producers {
        producer.objective_id =
            ObjectiveId::new(mapped(producer.objective_id.as_str(), &replacements)?).map_err(
                |_| {
                    diagnostic(
                        "$",
                        "definition_namespacing_failed",
                        "Namespaced objective ID was invalid",
                        DiagnosticTier::Structural,
                    )
                },
            )?;
        if let Some(subject_ref) = &mut producer.subject_ref {
            remap(subject_ref, &replacements);
        }
        if let Some(asset_id) = &mut producer.asset_id {
            remap(asset_id, &replacements);
        }
    }
    for bridge in &mut materialized.bridges {
        // bridge.id is catalog identity and deliberately remains stable.
        remap(&mut bridge.event_id, &replacements);
        remap(&mut bridge.evidence_id.0, &replacements);
        remap(&mut bridge.action_id.0, &replacements);
    }
    Ok(materialized)
}

pub fn compile(
    context: &DeveloperGenerationContext,
) -> Result<GeneratedCase, Vec<DeveloperQuestDiagnostic>> {
    let definition = &context.definition;
    let catalog = crate::quest_catalog::catalog();
    let mut diagnostics = Vec::new();
    validate_local_ids(definition, &mut diagnostics);
    let declared = DeclaredIdSets::from_definition(definition);
    validate_references(
        definition,
        &context.base.witness_candidates,
        &declared,
        &mut diagnostics,
    );
    validate_recursive_bounds(definition, &mut diagnostics);
    validate_custody(definition, &mut diagnostics);
    reject_unsupported_challenge_objectives(definition, &mut diagnostics);
    let supplied_bridges: BTreeSet<_> = definition
        .bridges
        .iter()
        .map(|bridge| bridge.id.0.as_str())
        .collect();

    let family_id = match definition.family {
        qg::TemplateFamily::RecurringDepredation => "recurring_depredation",
        qg::TemplateFamily::DisappearanceOrLoss => "disappearance_or_loss",
        qg::TemplateFamily::Outbreak => "outbreak",
    };
    let cause_id = match &definition.cause {
        CanonicalCause::Hostile(threat) => threat.as_str(),
        CanonicalCause::VoluntaryDisappearance | CanonicalCause::ConcealmentByWitness => {
            "concealment"
        }
        CanonicalCause::IncidentalLoss => "incidental_loss",
        CanonicalCause::FabricatedClaim => "fabricated",
    };
    if let Some(template) = catalog.template(&definition.template_id) {
        if definition.template_id != family_id {
            diagnostics.push(diagnostic(
                "family",
                "template_family_mismatch",
                "Selected template and family do not match",
                DiagnosticTier::Compatibility,
            ));
        }
        if definition.configured_routes != template.routes {
            diagnostics.push(diagnostic(
                "configured_routes",
                "template_routes_changed",
                "Configured routes differ from the selected catalog template",
                DiagnosticTier::Compatibility,
            ));
        }
        if definition.configured_objectives != template.objectives {
            diagnostics.push(diagnostic(
                "configured_objectives",
                "template_objectives_changed",
                "Configured objective vocabulary differs from the selected catalog template",
                DiagnosticTier::Compatibility,
            ));
        }
        let finale_key = if matches!(definition.cause, CanonicalCause::Hostile(_)) {
            "hostile"
        } else {
            cause_id
        };
        let allowed_finales = template
            .cause_finales
            .get(finale_key)
            .or_else(|| template.cause_finales.get("*"));
        for (index, finale) in definition.finales.iter().enumerate() {
            let finale_id = match finale.kind {
                qg::FinaleKind::Defeat => "defeat",
                qg::FinaleKind::DriveOff => "drive_off",
                qg::FinaleKind::Capture => "capture",
                qg::FinaleKind::Rescue => "rescue",
                qg::FinaleKind::RetrieveReturn => "retrieve_return",
                qg::FinaleKind::Expose => "expose",
                qg::FinaleKind::Negotiate => "negotiate",
            };
            if allowed_finales.is_none_or(|allowed| !allowed.iter().any(|item| item == finale_id)) {
                diagnostics.push(diagnostic(
                    format!("finales.{index}.kind"),
                    "template_finale_mismatch",
                    "Finale is not curated for this template and canonical cause",
                    DiagnosticTier::Compatibility,
                ));
            }
        }
    } else {
        diagnostics.push(diagnostic(
            "template_id",
            "unknown_catalog_id",
            "Template is not present in the startup quest catalog",
            DiagnosticTier::Structural,
        ));
    }
    check_relation(
        &mut diagnostics,
        catalog,
        &supplied_bridges,
        "cause".into(),
        format!("cause.{family_id}"),
        cause_id,
    );
    if definition.incident_interval_minutes == 0 || definition.maximum_incidents == 0 {
        diagnostics.push(diagnostic(
            "incident_interval_minutes",
            "invalid_incident_schedule",
            "Incident interval and maximum incidents must both be non-zero",
            DiagnosticTier::Structural,
        ));
    }
    for (index, site) in definition.sites.iter().enumerate() {
        if catalog.site(site.kind.as_str()).is_none() {
            diagnostics.push(diagnostic(
                format!("sites.{index}.kind"),
                "unknown_catalog_id",
                "Site kind is not present in the startup quest catalog",
                DiagnosticTier::Structural,
            ));
        }
    }
    for (index, evidence) in definition.evidence.iter().enumerate() {
        if catalog.evidence(evidence.kind.as_str()).is_none() {
            diagnostics.push(diagnostic(
                format!("evidence.{index}.kind"),
                "unknown_catalog_id",
                "Evidence kind is not present in the startup quest catalog",
                DiagnosticTier::Structural,
            ));
        }
    }

    let candidates: BTreeMap<_, _> = context
        .base
        .witness_candidates
        .iter()
        .map(|candidate| (candidate.resident_character_id, candidate))
        .collect();
    for (index, witness) in definition.witnesses.iter().enumerate() {
        check_relation(
            &mut diagnostics,
            catalog,
            &supplied_bridges,
            format!("witnesses.{index}.circumstance"),
            format!("circumstance.{}", witness.demographic.as_str()),
            witness.circumstance.as_str(),
        );
        for (testimony_index, testimony) in witness.testimony.iter().enumerate() {
            check_relation(
                &mut diagnostics,
                catalog,
                &supplied_bridges,
                format!("witnesses.{index}.testimony.{testimony_index}.reliability"),
                format!("reliability.{}", witness.demographic.as_str()),
                match testimony.reliability {
                    qg::Reliability::Truthful => "truthful",
                    qg::Reliability::Mistaken => "mistaken",
                    qg::Reliability::Evasive => "evasive",
                    qg::Reliability::Deceptive => "deceptive",
                    qg::Reliability::PartlyTruthful => "partly_truthful",
                },
            );
        }
        let Some(candidate) = candidates.get(&witness.resident_character_id) else {
            diagnostics.push(diagnostic(
                format!("witnesses.{index}.resident_character_id"),
                "missing_navigable_witness",
                "Witness is not a current, persistent, navigable NPC in this settlement",
                DiagnosticTier::Structural,
            ));
            continue;
        };
        if !candidate
            .allowed_circumstances
            .contains(&witness.circumstance)
        {
            diagnostics.push(diagnostic(
                format!("witnesses.{index}.circumstance"),
                "impossible_witness_circumstance",
                "This witness cannot occupy the selected circumstance",
                DiagnosticTier::Structural,
            ));
        }
        if witness.expected_location != candidate.expected_location
            || witness.expected_location_label != candidate.expected_location_label
            || witness.display_name != candidate.display_name
            || witness.demographic != candidate.demographic
            || witness.visible_description != candidate.visible_description
        {
            diagnostics.push(diagnostic(
                format!("witnesses.{index}"),
                "stale_witness_binding",
                "Witness identity, description, demographic, or location does not match current settlement presence",
                DiagnosticTier::Structural,
            ));
        }
    }
    for (index, target) in definition.pattern_targets.iter().enumerate() {
        let matches_current =
            candidates
                .get(&target.resident_character_id)
                .is_some_and(|candidate| {
                    qg::pattern_target_matches(target, candidate, &context.base.settlement_id)
                });
        if !matches_current {
            diagnostics.push(diagnostic(
                format!("pattern_targets.{index}"),
                "stale_pattern_target",
                "Pattern target is not the same current, persistent settlement NPC",
                DiagnosticTier::Structural,
            ));
        }
    }

    if let CanonicalCause::Hostile(threat) = definition.cause {
        let Some(monster) = catalog.monster(threat.as_str()) else {
            diagnostics.push(diagnostic(
                "cause",
                "unknown_catalog_id",
                "Hostile threat is not present in the startup quest catalog",
                DiagnosticTier::Structural,
            ));
            return Err(diagnostics);
        };
        for (index, site) in definition
            .sites
            .iter()
            .enumerate()
            .filter(|(_, site)| site.role == qg::SiteRole::Finale && site.is_true_location)
        {
            check_relation(
                &mut diagnostics,
                catalog,
                &supplied_bridges,
                format!("sites.{index}.kind"),
                format!("site.{}", threat.as_str()),
                site.kind.as_str(),
            );
            let habitat = catalog
                .site(site.kind.as_str())
                .map(|site| site.habitat.as_str())
                .unwrap_or_default();
            if !monster
                .investigation
                .habitats
                .iter()
                .any(|candidate| candidate == habitat)
            {
                diagnostics.push(diagnostic(
                    format!("sites.{index}.kind"),
                    "implausible_threat_site",
                    format!("{} is not authored for the {habitat} habitat", monster.name),
                    DiagnosticTier::Compatibility,
                ));
            }
        }
    }
    for (index, evidence) in definition.evidence.iter().enumerate() {
        check_relation(
            &mut diagnostics,
            catalog,
            &supplied_bridges,
            format!("evidence.{index}.kind"),
            format!("evidence.{cause_id}"),
            evidence.kind.as_str(),
        );
    }

    if diagnostics
        .iter()
        .any(|item| item.tier == DiagnosticTier::Structural)
        || (!context.allow_implausible && !diagnostics.is_empty())
    {
        return Err(diagnostics);
    }
    let materialized =
        namespace_definition(&context.base, definition).map_err(|item| vec![item])?;

    let generated = GeneratedCase {
        catalog_revision: qg::CATALOG_REVISION.into(),
        generation_seed: context.base.seed,
        template_id: materialized.template_id.clone(),
        configured_routes: materialized.configured_routes.clone(),
        configured_objectives: materialized.configured_objectives.clone(),
        incident_interval_minutes: materialized.incident_interval_minutes,
        maximum_incidents: materialized.maximum_incidents,
        family: materialized.family,
        canonical_case_id: qg::observer_scoped_id(&context.base, "case", "developer"),
        public_case_id: qg::observer_scoped_id(&context.base, "public-case", "developer"),
        problem_id: qg::observer_scoped_id(&context.base, "problem", "developer"),
        cause: materialized.cause,
        canonical_events: materialized.canonical_events,
        consequence: materialized.consequence,
        outbreak: materialized.outbreak,
        sites: materialized.sites,
        areas: materialized.areas,
        witnesses: materialized.witnesses,
        pattern_targets: materialized.pattern_targets,
        evidence: materialized.evidence,
        track_trails: materialized.track_trails,
        track_segments: materialized.track_segments,
        actions: materialized.actions,
        objectives: materialized.objectives,
        custody: materialized.custody,
        hostile_groups: materialized.hostile_groups,
        finales: materialized.finales,
        dialogue_producers: materialized.dialogue_producers,
        bridges: materialized.bridges,
        factor_trace: Vec::new(),
    };
    if let Err(errors) = qg::validate(&generated) {
        diagnostics.extend(errors.into_iter().map(|message| {
            diagnostic(
                "$",
                "invalid_generated_case",
                message,
                DiagnosticTier::Structural,
            )
        }));
        return Err(diagnostics);
    }
    Ok(generated)
}

/// Catalog-derived editor metadata. Open content IDs are never duplicated in
/// the web client. Closed mechanics are declared here in one core-owned list.
pub fn schema_json(witness_candidates: &[qg::WitnessCandidate]) -> Value {
    let catalog = crate::quest_catalog::catalog();
    let configured_routes = catalog
        .templates()
        .flat_map(|template| template.routes.iter().cloned())
        .collect::<BTreeSet<_>>();
    let configured_objectives = catalog
        .templates()
        .flat_map(|template| template.objectives.iter().cloned())
        .collect::<BTreeSet<_>>();
    let demographics = catalog
        .documents
        .iter()
        .flat_map(|document| &document.witness_demographics)
        .collect::<Vec<_>>();
    let circumstances = catalog
        .documents
        .iter()
        .flat_map(|document| &document.circumstances);
    let descriptions = catalog
        .documents
        .iter()
        .flat_map(|document| &document.descriptions);
    let options = |values: Vec<(String, String)>| {
        values
            .into_iter()
            .map(|(value, label)| json!({ "value": value, "label": label }))
            .collect::<Vec<_>>()
    };
    json!({
        "catalog_revision": qg::CATALOG_REVISION,
        "limits": {
            "payload_bytes": MAX_DEVELOPER_QUEST_JSON_BYTES,
            "collection_items": MAX_DEVELOPER_COLLECTION_ITEMS,
            "total_items": MAX_DEVELOPER_TOTAL_ITEMS
        },
        "options": {
            "templates": catalog.templates().map(|template| json!({
                "value": template.id,
                "label": template.label,
                "binding": {
                    "routes": template.routes,
                    "objectives": template.objectives,
                }
            })).collect::<Vec<_>>(),
            "configured_routes": options(configured_routes.into_iter().map(|id| {
                let label = id.replace('_', " ");
                (id, label)
            }).collect()),
            "configured_objectives": options(configured_objectives.into_iter().map(|id| {
                let label = id.replace('_', " ");
                (id, label)
            }).collect()),
            "threats": options(catalog.monsters().map(|x| (x.id.clone(), x.name.clone())).collect()),
            "sites": options(catalog.sites().map(|x| (x.id.clone(), x.label.clone())).collect()),
            "evidence": options(catalog.evidence_definitions().map(|x| (x.id.clone(), x.portrait_label.clone())).collect()),
            "witnesses": witness_candidates.iter().map(|candidate| json!({
                "value": candidate.resident_character_id,
                "label": format!("{} — {}", candidate.display_name, candidate.expected_location_label),
                "binding": {
                    "resident_character_id": candidate.resident_character_id,
                    "display_name": candidate.display_name,
                    "demographic": candidate.demographic,
                    "age_band": candidate.age_band,
                    "sex": candidate.sex,
                    "profession": candidate.profession,
                    "visible_description": candidate.visible_description,
                    "expected_location": candidate.expected_location,
                    "expected_location_label": candidate.expected_location_label,
                    "presence_version": candidate.presence_version,
                    "allowed_circumstances": candidate.allowed_circumstances,
                }
            })).collect::<Vec<_>>(),
            "witness_demographics": options(demographics.iter().map(|x| (x.id.clone(), x.label.clone())).collect()),
            "circumstances": options(circumstances.map(|x| (x.id.clone(), x.statement.clone())).collect()),
            "descriptions": options(descriptions.map(|x| (x.id.clone(), x.text.clone())).collect()),
            "template_families": ["recurring_depredation", "disappearance_or_loss", "outbreak"],
            "site_roles": ["finale", "evidence", "decoy", "last_known"],
            "reliabilities": ["truthful", "mistaken", "evasive", "deceptive", "partly_truthful"],
            "evidence_check_stats": ["eyesight", "intelligence", "instinct"],
            "route_classes": ["physical_trail", "pattern_surveillance", "social_inquiry"],
            "destination_stages": ["unknown", "textual", "landmark", "approximate_area", "route_segment", "exact"],
            "finale_kinds": ["defeat", "drive_off", "capture", "rescue", "retrieve_return", "expose", "negotiate"],
            "dialogue_actions": ["expose", "return_asset"]
            ,"investigation_actions": ["inspect_site", "search_area", "follow_tracks", "reacquire_tracks", "locate_contact", "watch", "patrol", "lay_ambush", "approach_lead"]
            ,"terrains": ["road", "settlement", "plains", "forest", "hills", "marsh", "ruins", "underground"]
            ,"symptoms": ["missing_caravans", "night_screams", "sick_locals", "empty_stalls", "vanished_livestock"]
            ,"encounter_archetypes": ["bandits", "goblins", "undead"]
            ,"action_output_kinds": ["destination", "evidence", "pattern_condition", "track_finding", "ambush_ready", "consequence"]
            ,"pattern_condition_kinds": ["night_window", "road_route", "victim_profile", "broad_survey"]
            ,"action_consequence_kinds": ["retrieve_asset", "rescue_subject"]
            ,"objective_requirements": ["Defeat", "DriveOff", "Capture", "SurviveWindow", "Rescue", "EscortTo", "Retrieve", "Return", "Locate", "Identify", "Expose", "PresentProof", "PresentTestimony", "Protect", "Negotiate", "Release", "Exchange", "ReportToIssuer"]
            ,"cause_kinds": ["hostile", "voluntary_disappearance", "concealment_by_witness", "incidental_loss", "fabricated_claim"]
        },
        "constructors": {
            "optional": {
                "evidence_check": {
                    "stat": "eyesight",
                    "difficulty_milli": 1000,
                    "success_description": "You notice a significant detail.",
                    "reveals_clue": true
                }
            },
            "variants": {
                "cause": {
                    "hostile": {"hostile": catalog.monsters().next().map(|monster| monster.id.clone()).unwrap_or_default()},
                    "voluntary_disappearance": "voluntary_disappearance",
                    "concealment_by_witness": "concealment_by_witness",
                    "incidental_loss": "incidental_loss",
                    "fabricated_claim": "fabricated_claim"
                },
                "action_output": {
                    "destination": {"kind":"destination","stage":"unknown","site_id":null},
                    "evidence": {"kind":"evidence","evidence_id":"evidence:new"},
                    "pattern_condition": {"kind":"pattern_condition","evidence_id":"evidence:new","condition":{"kind":"night_window"}},
                    "track_finding": {"kind":"track_finding","segment_id":"track-segment:new","finding":"The trail continues across this ground."},
                    "ambush_ready": {"kind":"ambush_ready"},
                    "consequence": {"kind":"consequence","consequence":{"kind":"retrieve_asset","asset_id":"asset:new","next_version":1}}
                },
                "pattern_condition": {
                    "night_window":{"kind":"night_window"},
                    "road_route":{"kind":"road_route"},
                    "victim_profile":{"kind":"victim_profile","cohort_id":"cohort:new","demographic":demographics.first().map(|item| item.id.clone()).unwrap_or_default(),"age_band":"adult","sex":"female","profession":""},
                    "broad_survey":{"kind":"broad_survey"}
                },
                "action_consequence": {
                    "retrieve_asset":{"kind":"retrieve_asset","asset_id":"asset:new","next_version":1},
                    "rescue_subject":{"kind":"rescue_subject","subject_id":"subject:new","next_version":1}
                },
                "objective_requirement": {
                    "Defeat":{"Defeat":{"hostile_group_id":"group:new","count":1}},
                    "DriveOff":{"DriveOff":{"hostile_group_id":"group:new"}},
                    "Capture":{"Capture":{"subject_id":"subject:new"}},
                    "SurviveWindow":{"SurviveWindow":{"site_id":"site:new","through_minute":0}},
                    "Rescue":{"Rescue":{"subject_id":"subject:new"}},
                    "EscortTo":{"EscortTo":{"subject_id":"subject:new","site_id":"site:new"}},
                    "Retrieve":{"Retrieve":{"asset_id":"asset:new"}},
                    "Return":{"Return":{"asset_id":"asset:new","custodian_id":""}},
                    "Locate":{"Locate":{"subject_ref":"subject:new"}},
                    "Identify":{"Identify":{"subject_ref":"proposition:new"}},
                    "Expose":{"Expose":{"subject_ref":"proposition:new"}},
                    "PresentProof":{"PresentProof":{"evidence_id":"evidence:new","recipient_id":""}},
                    "PresentTestimony":{"PresentTestimony":{"witness_id":"witness:new","recipient_id":""}},
                    "Protect":{"Protect":{"subject_id":"subject:new","through_minute":0}},
                    "Negotiate":{"Negotiate":{"subject_ref":"subject:new"}},
                    "Release":{"Release":{"subject_id":"subject:new"}},
                    "Exchange":{"Exchange":{"asset_id":"asset:new","recipient_id":""}},
                    "ReportToIssuer":{"ReportToIssuer":{"issuer_id":""}}
                }
            }
        },
        "sections": [
            {"path":"identity", "label":"Template, canonical cause, incidents and consequences"},
            {"path":"sites", "label":"Sites, hideout and areas", "repeatable":true},
            {"path":"witnesses", "label":"Witnesses, testimony and referrals", "repeatable":true},
            {"path":"evidence", "label":"Physical evidence and inspection topics", "repeatable":true},
            {"path":"track_trails", "label":"Track trail chains", "repeatable":true},
            {"path":"track_segments", "label":"Track segment authority", "repeatable":true},
            {"path":"actions", "label":"Routes, prerequisites, alternates and outputs", "repeatable":true},
            {"path":"objectives", "label":"DNF objectives, custody and hostiles", "repeatable":true},
            {"path":"finales", "label":"Finales and dialogue producers", "repeatable":true},
            {"path":"canonical_events", "label":"Canonical events and bridges", "repeatable":true}
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_problem::Scope;
    use crate::quest_generation::{SiteKind, TemplateFamily, test_witnesses};

    fn context() -> GenerationContext {
        GenerationContext {
            seed: 7,
            observer_entropy_hi: 8,
            observer_entropy_lo: 9,
            settlement_id: "riverdale".into(),
            settlement_name: "Riverdale".into(),
            scope: Scope::Settlement {
                settlement_id: "riverdale".into(),
            },
            ordinal: 0,
            now_minute: 100,
            incident_weather: crate::weather::Precipitation::Clear,
            requested_family: Some(TemplateFamily::RecurringDepredation),
            witness_candidates: test_witnesses(),
        }
    }

    #[test]
    fn schema_uses_embedded_catalog_and_declares_bounds() {
        let schema = schema_json(&context().witness_candidates);
        assert_eq!(schema["catalog_revision"], qg::CATALOG_REVISION);
        assert!(schema["options"]["threats"].as_array().unwrap().len() > 10);
        assert!(
            !schema["options"]["configured_routes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            !schema["options"]["configured_objectives"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            schema["limits"]["collection_items"],
            MAX_DEVELOPER_COLLECTION_ITEMS
        );
        assert!(schema["constructors"]["variants"]["action_output"].is_object());
        assert!(schema["options"]["witnesses"][0]["binding"]["allowed_circumstances"].is_array());
        let check: qg::EvidenceInspectionCheck =
            serde_json::from_value(schema["constructors"]["optional"]["evidence_check"].clone())
                .unwrap();
        assert_eq!(check.difficulty_milli, 1_000);
        let output: qg::GeneratedActionOutput = serde_json::from_value(
            schema["constructors"]["variants"]["action_output"]["pattern_condition"].clone(),
        )
        .unwrap();
        assert!(matches!(
            output,
            qg::GeneratedActionOutput::PatternCondition { .. }
        ));
        let track_output: qg::GeneratedActionOutput = serde_json::from_value(
            schema["constructors"]["variants"]["action_output"]["track_finding"].clone(),
        )
        .unwrap();
        assert!(matches!(
            track_output,
            qg::GeneratedActionOutput::TrackFinding { .. }
        ));
        let requirement: crate::case::ObjectiveRequirement = serde_json::from_value(
            schema["constructors"]["variants"]["objective_requirement"]["Rescue"].clone(),
        )
        .unwrap();
        assert!(matches!(
            requirement,
            crate::case::ObjectiveRequirement::Rescue { .. }
        ));
    }

    #[test]
    fn parse_is_bounded_and_never_panics() {
        let error =
            parse_definition_json(&"x".repeat(MAX_DEVELOPER_QUEST_JSON_BYTES + 1)).unwrap_err();
        assert_eq!(error[0].code, "payload_too_large");
        let error = parse_definition_json("{").unwrap_err();
        assert_eq!(error[0].code, "invalid_json");
    }

    fn generated_definition() -> DeveloperQuestDefinition {
        DeveloperQuestDefinition::from_generated(qg::generate(&context()).unwrap())
    }

    fn generated_definition_with_custody() -> DeveloperQuestDefinition {
        for seed in 0..256 {
            let mut candidate = context();
            candidate.seed = seed;
            candidate.requested_family = Some(TemplateFamily::DisappearanceOrLoss);
            if let Ok(generated) = qg::generate(&candidate)
                && !generated.custody.is_empty()
            {
                return DeveloperQuestDefinition::from_generated(generated);
            }
        }
        panic!("fixture search should find a custody-bearing generated definition")
    }

    #[test]
    fn complete_definition_replays_deterministically() {
        let developer = DeveloperGenerationContext {
            base: context(),
            definition: generated_definition(),
            allow_implausible: true,
        };
        let first = compile(&developer).unwrap();
        let second = compile(&developer).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.sites.len(), developer.definition.sites.len());
        assert_eq!(first.areas.len(), developer.definition.areas.len());
        assert_eq!(first.witnesses.len(), developer.definition.witnesses.len());
        assert_eq!(first.evidence.len(), developer.definition.evidence.len());
        assert_eq!(
            first.track_trails.len(),
            developer.definition.track_trails.len()
        );
        assert_eq!(
            first.track_segments.len(),
            developer.definition.track_segments.len()
        );
        assert_eq!(first.actions.len(), developer.definition.actions.len());
        assert_ne!(first.sites[0].id, developer.definition.sites[0].id);
        assert_eq!(first.consequence, developer.definition.consequence);
    }

    #[test]
    fn separate_spawns_namespace_every_internal_id_disjointly() {
        let definition = generated_definition();
        let first = compile(&DeveloperGenerationContext {
            base: context(),
            definition: definition.clone(),
            allow_implausible: true,
        })
        .unwrap();
        let mut other = context();
        other.seed = 77;
        other.observer_entropy_hi = 88;
        other.observer_entropy_lo = 99;
        other.ordinal = 1;
        let second = compile(&DeveloperGenerationContext {
            base: other,
            definition,
            allow_implausible: true,
        })
        .unwrap();
        let first_json = serde_json::to_string(&first).unwrap();
        for id in [
            &second.sites[0].id.0,
            &second.witnesses[0].id.0,
            &second.evidence[0].id.0,
            &second.track_trails[0].id.0,
            &second.track_segments[0].id.0,
            &second.actions[0].id.0,
            second.objectives.alternatives[0].objectives[0].id.as_str(),
            &second.finales[0].id.0,
        ] {
            assert!(!first_json.contains(id), "{id} was shared across spawns");
        }
    }

    #[test]
    fn duplicate_local_ids_are_structural_path_errors() {
        let mut definition = generated_definition();
        definition.sites[1].id = definition.sites[0].id.clone();
        let diagnostics = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.code == "duplicate_local_id"
                && item.path == "sites.1.id"
                && item.tier == DiagnosticTier::Structural
        }));
    }

    #[test]
    fn challenge_objectives_are_rejected_until_the_editor_can_materialize_them() {
        let mut definition = generated_definition();
        definition.objectives.alternatives[0].objectives[0].requirement =
            ObjectiveRequirement::SolveChallenge {
                challenge_id: "challenge:not-declared".into(),
            };
        let diagnostics = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.code == "unsupported_challenge_objective"
                && item.path == "objectives.alternatives.0.objectives.0.requirement"
                && item.tier == DiagnosticTier::Structural
        }));
    }

    #[test]
    fn switching_to_another_schema_candidate_compiles_with_atomic_binding() {
        let mut definition = generated_definition();
        let current_npc = definition.witnesses[0].resident_character_id.clone();
        let replacement = context()
            .witness_candidates
            .into_iter()
            .find(|candidate| candidate.resident_character_id != current_npc)
            .unwrap();
        let witness = &mut definition.witnesses[0];
        witness.resident_character_id = replacement.resident_character_id.clone();
        witness.display_name = replacement.display_name.clone();
        witness.demographic = replacement.demographic;
        witness.expected_location = replacement.expected_location.clone();
        witness.expected_location_label = replacement.expected_location_label.clone();
        witness.visible_description = replacement.visible_description.clone();
        witness.circumstance = *replacement.allowed_circumstances.iter().next().unwrap();
        for action in &mut definition.actions {
            if action.target_kind == "contact" && action.target_id == current_npc.to_string() {
                action.target_id = replacement.resident_character_id.to_string();
            }
        }
        for producer in &mut definition.dialogue_producers {
            if producer.recipient_resident_character_id == current_npc {
                producer.recipient_resident_character_id =
                    replacement.resident_character_id.clone();
            }
        }
        let generated = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap();
        assert_eq!(
            generated.witnesses[0].resident_character_id,
            replacement.resident_character_id
        );
        assert_eq!(
            generated.witnesses[0].expected_location,
            replacement.expected_location
        );
    }

    #[test]
    fn typed_optional_check_constructor_mutation_serializes_and_compiles() {
        let mut definition = generated_definition();
        let schema = schema_json(&context().witness_candidates);
        let check: qg::EvidenceInspectionCheck =
            serde_json::from_value(schema["constructors"]["optional"]["evidence_check"].clone())
                .unwrap();
        definition.evidence[0].inspection_topics[0].check = Some(check.clone());
        let serialized = serde_json::to_string(&definition).unwrap();
        assert!(serialized.contains("\"difficulty_milli\":1000"));
        let generated = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap();
        assert_eq!(
            generated.evidence[0].inspection_topics[0].check,
            Some(check)
        );
    }

    #[test]
    fn authored_custody_site_is_preserved_instead_of_forced_to_finale() {
        let mut definition = generated_definition_with_custody();
        let non_finale_index = definition
            .sites
            .iter()
            .position(|site| site.role != qg::SiteRole::Finale)
            .unwrap();
        definition.custody[0].1 = definition.sites[non_finale_index].id.clone();
        let generated = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap();
        assert_eq!(generated.custody[0].1, generated.sites[non_finale_index].id);
    }

    fn assert_unknown_reference(definition: DeveloperQuestDefinition, expected_path: &str) {
        let diagnostics = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap_err();
        assert!(
            diagnostics.iter().any(|item| {
                item.code == "unknown_reference"
                    && item.path == expected_path
                    && item.tier == DiagnosticTier::Structural
            }),
            "missing structural diagnostic at {expected_path}: {diagnostics:#?}"
        );
    }

    #[test]
    fn missing_custody_site_is_rejected_before_namespacing() {
        let mut definition = generated_definition_with_custody();
        definition.custody[0].1 = SiteId::try_new("site:missing").unwrap();
        assert_unknown_reference(definition, "custody.0.1");
    }

    #[test]
    fn missing_evidence_site_is_rejected_before_namespacing() {
        let mut definition = generated_definition();
        definition.evidence[0].site_id = SiteId::try_new("site:missing").unwrap();
        assert_unknown_reference(definition, "evidence.0.site_id");
    }

    #[test]
    fn typed_reference_tables_reject_representative_dangling_edges() {
        let mut area = generated_definition();
        area.areas[0]
            .contains_site_ids
            .push(SiteId::try_new("site:missing").unwrap());
        let site_index = area.areas[0].contains_site_ids.len() - 1;
        assert_unknown_reference(area, &format!("areas.0.contains_site_ids.{site_index}"));

        let mut referral = generated_definition();
        referral.witnesses[0].testimony[0]
            .referred_witness_ids
            .push(qg::WitnessId::try_new("witness:missing").unwrap());
        let referral_index = referral.witnesses[0].testimony[0]
            .referred_witness_ids
            .len()
            - 1;
        assert_unknown_reference(
            referral,
            &format!("witnesses.0.testimony.0.referred_witness_ids.{referral_index}"),
        );

        let mut action = generated_definition();
        action.actions[0].alternate = qg::ActionId::try_new("action:missing").unwrap();
        assert_unknown_reference(action, "actions.0.alternate");

        let mut segment_action = generated_definition();
        let action_index = segment_action
            .actions
            .iter()
            .position(|action| action.track_segment_id.is_some())
            .unwrap();
        segment_action.actions[action_index].track_segment_id =
            Some(qg::TrackSegmentId::try_new("track-segment:missing").unwrap());
        assert_unknown_reference(
            segment_action,
            &format!("actions.{action_index}.track_segment_id"),
        );

        let mut finale = generated_definition();
        finale.finales[0].site_id = SiteId::try_new("site:missing").unwrap();
        assert_unknown_reference(finale, "finales.0.site_id");

        let mut dialogue = generated_definition();
        if let Some(producer) = dialogue.dialogue_producers.first_mut() {
            producer.objective_id = ObjectiveId::new("objective:missing").unwrap();
            assert_unknown_reference(dialogue, "dialogue_producers.0.objective_id");
        }
    }

    #[test]
    fn namespacing_does_not_rewrite_prose_that_looks_like_an_id() {
        let mut definition = generated_definition();
        definition.track_segments[0].safe_finding = "track-segment:foo".into();
        let segment_id = definition.track_segments[0].id.clone();
        if let qg::GeneratedActionOutput::TrackFinding { finding, .. } = definition
            .actions
            .iter_mut()
            .find(|action| action.track_segment_id.as_ref() == Some(&segment_id))
            .unwrap()
            .outputs
            .iter_mut()
            .find(|output| {
                matches!(
                    output,
                    qg::GeneratedActionOutput::TrackFinding { segment_id: id, .. }
                        if id == &segment_id
                )
            })
            .unwrap()
        {
            *finding = "track-segment:foo".into();
        }
        let mut prose_site = definition.sites[0].clone();
        prose_site.id = SiteId::try_new("site:foo").unwrap();
        prose_site.safe_label = "site:foo".into();
        prose_site.exact_location_initially_known = false;
        prose_site.is_true_location = false;
        definition.sites.push(prose_site);

        let generated = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap();
        let site = generated
            .sites
            .iter()
            .find(|site| site.safe_label == "site:foo")
            .expect("ID-shaped prose is unchanged");
        assert_ne!(site.id.0, "site:foo");
        assert_eq!(site.safe_label.as_bytes(), b"site:foo");
        assert_eq!(
            generated.track_segments[0].safe_finding,
            "track-segment:foo"
        );
    }

    #[test]
    fn compatibility_can_be_overridden_but_structural_errors_cannot() {
        let mut definition = generated_definition();
        let CanonicalCause::Hostile(threat) = definition.cause else {
            panic!("fixture is hostile")
        };
        let monster = crate::quest_catalog::catalog()
            .monster(threat.as_str())
            .unwrap();
        let incompatible = crate::quest_catalog::catalog()
            .sites()
            .find(|site| !monster.investigation.habitats.contains(&site.habitat))
            .unwrap();
        for site in &mut definition.sites {
            if site.role == qg::SiteRole::Finale {
                site.kind = SiteKind::try_new(&incompatible.id).unwrap();
                site.terrain = match incompatible.terrain.as_str() {
                    "road" => crate::investigation_action::Terrain::Road,
                    "settlement" => crate::investigation_action::Terrain::Settlement,
                    "plains" => crate::investigation_action::Terrain::Plains,
                    "forest" => crate::investigation_action::Terrain::Forest,
                    "hills" => crate::investigation_action::Terrain::Hills,
                    "marsh" => crate::investigation_action::Terrain::Marsh,
                    "ruins" => crate::investigation_action::Terrain::Ruins,
                    "underground" => crate::investigation_action::Terrain::Underground,
                    _ => panic!("validated terrain"),
                };
            }
        }
        let mut developer = DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: false,
        };
        let diagnostics = compile(&developer).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.tier == DiagnosticTier::Compatibility)
        );
        developer.allow_implausible = true;
        assert!(compile(&developer).is_ok());

        developer.definition.sites.clear();
        let diagnostics = compile(&developer).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.tier == DiagnosticTier::Structural)
        );
    }

    #[test]
    fn repeated_collections_are_bounded() {
        let mut definition = generated_definition();
        definition.canonical_events =
            vec![definition.canonical_events[0].clone(); MAX_DEVELOPER_COLLECTION_ITEMS + 1];
        let diagnostics = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.code == "collection_too_large")
        );
    }

    #[test]
    fn nested_collections_are_bounded_with_stable_paths() {
        let mut definition = generated_definition();
        definition.witnesses[0].testimony[0].referred_witness_ids =
            vec![definition.witnesses[0].id.clone(); MAX_DEVELOPER_COLLECTION_ITEMS + 1];
        let diagnostics = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: true,
        })
        .unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.code == "collection_too_large"
                && item.path == "witnesses.0.testimony.0.referred_witness_ids"
        }));
    }

    #[test]
    fn catalog_hard_zero_is_a_structured_compatibility_diagnostic() {
        let mut definition = generated_definition();
        definition.cause = CanonicalCause::Hostile(ThreatId::Wolf);
        for (_, _, threat, _) in &mut definition.hostile_groups {
            *threat = ThreatId::Wolf;
        }
        for site in &mut definition.sites {
            if site.role == qg::SiteRole::Finale {
                site.kind = SiteKind::Crypt;
                site.terrain = crate::investigation_action::Terrain::Underground;
            }
        }
        let diagnostics = compile(&DeveloperGenerationContext {
            base: context(),
            definition,
            allow_implausible: false,
        })
        .unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.code == "catalog_hard_zero" && item.tier == DiagnosticTier::Compatibility
        }));
    }
}
