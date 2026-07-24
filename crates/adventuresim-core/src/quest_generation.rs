//! Typed, deterministic generation for investigation-led quests.
//!
//! The catalog is deliberately static Rust data rather than an interpreted
//! rules language.  Relations are defined once, in the direction in which the
//! solver consumes them.  Diagnostic traces contain canonical truth and must
//! remain private to strategic authority and developer tools.

use crate::{
    bestiary::{ALL_REPORTS, ReportDescription, ThreatId, description_likelihood},
    case::{
        AssetId, Objective, ObjectiveExpression, ObjectiveId, ObjectivePath, ObjectiveRequirement,
        SubjectId,
    },
    investigation_action::{InvestigationActionKind, Terrain},
    local_problem::{Effects, EncounterArchetype, Scope, Symptom},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOG_REVISION: &str = "questgen-2026-07-24.1";
pub const MAX_SOLVER_CANDIDATES: usize = 4_096;
pub const MAX_SOLVER_VISITED_NODES: usize = 16_384;
pub const MAX_FACTOR_TRACE_RECORDS: usize = 32_768;
pub const MAX_FACTOR_TRACE_BYTES: usize = 1_048_576;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);
        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, &'static str> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 256
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.')
                    })
                {
                    return Err("invalid bounded quest-generation ID");
                }
                Ok(Self(value))
            }
            fn new(value: impl Into<String>) -> Self {
                Self::try_new(value).expect("static/generated quest ID")
            }
        }
    };
}
id_type!(ModuleId);
id_type!(RelationId);
id_type!(FactorId);
id_type!(BridgeId);
id_type!(SiteId);
id_type!(WitnessId);
id_type!(EvidenceId);
id_type!(ActionId);
id_type!(FinaleId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateFamily {
    RecurringDepredation,
    DisappearanceOrLoss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalCause {
    Hostile(ThreatId),
    VoluntaryDisappearance,
    ConcealmentByWitness,
    IncidentalLoss,
    FabricatedClaim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteKind {
    Cave,
    Crypt,
    ForestCamp,
    OccupiedHouse,
    Riverside,
    Graveyard,
    Roadside,
    AbandonedFarm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRole {
    Finale,
    Evidence,
    Decoy,
    LastKnown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WitnessDemographic {
    Child,
    Laborer,
    Merchant,
    Cleric,
    Guard,
    Noble,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Circumstance {
    NightWindow,
    SecretRiversideMeeting,
    AdultVenue,
    RoadJourney,
    GraveDuty,
    LivestockWatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reliability {
    Truthful,
    Mistaken,
    Evasive,
    Deceptive,
    PartlyTruthful,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Footprints,
    ClothScrap,
    BoneDust,
    BloodlessCorpse,
    DroppedToken,
    DragMarks,
    LedgerEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteClass {
    PhysicalTrail,
    PatternSurveillance,
    SocialInquiry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinaleKind {
    Defeat,
    DriveOff,
    Capture,
    Rescue,
    RetrieveReturn,
    Expose,
    Negotiate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedDialogueAction {
    Expose,
    ReturnAsset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weight {
    pub plausibility: u32,
    pub curation: u32,
}
impl Weight {
    pub const fn new(plausibility: u32, curation: u32) -> Self {
        Self {
            plausibility,
            curation,
        }
    }
    pub fn combined(self) -> u64 {
        u64::from(self.plausibility) * u64::from(self.curation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationContext {
    pub seed: u64,
    /// Independently sampled, private entropy used only to mint observer-facing IDs.
    pub observer_entropy_hi: u64,
    pub observer_entropy_lo: u64,
    pub settlement_id: String,
    pub settlement_name: String,
    pub scope: Scope,
    pub ordinal: u16,
    pub now_minute: u64,
    pub requested_family: Option<TemplateFamily>,
    pub witness_candidates: Vec<WitnessCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessCandidate {
    pub npc_id: String,
    pub demographic: WitnessDemographic,
    pub age_band: String,
    pub sex: String,
    pub profession: String,
    pub visible_description: String,
    pub expected_location: String,
    pub expected_location_label: String,
    pub presence_version: u64,
    pub allowed_circumstances: BTreeSet<Circumstance>,
}

/// Removes witness/location combinations that the player cannot reach through
/// the settlement UI. Absence from `visible_tabs` is an authoritative hard
/// zero, not a low-probability candidate.
pub fn retain_navigable_witnesses(
    candidates: Vec<WitnessCandidate>,
    visible_tabs: &[crate::settlement_economy::SettlementNpcTab],
) -> Vec<WitnessCandidate> {
    candidates
        .into_iter()
        .filter_map(|mut candidate| {
            let tab = crate::settlement_economy::visible_npc_tab(
                visible_tabs,
                &candidate.expected_location,
            )?;
            candidate.expected_location_label = tab.label.to_owned();
            Some(candidate)
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedPatternTarget {
    pub cohort_id: String,
    pub npc_id: String,
    pub demographic: WitnessDemographic,
    pub age_band: String,
    pub sex: String,
    pub profession: String,
    pub expected_settlement_id: String,
    pub expected_location: String,
    pub expected_location_label: String,
    pub presence_version: u64,
}

pub fn pattern_target_matches(
    expected: &GeneratedPatternTarget,
    current: &WitnessCandidate,
    current_settlement_id: &str,
) -> bool {
    expected.npc_id == current.npc_id
        && expected.demographic == current.demographic
        && expected.age_band == current.age_band
        && expected.sex == current.sex
        && expected.profession == current.profession
        && expected.expected_settlement_id == current_settlement_id
        && expected.expected_location == current.expected_location
        && expected.presence_version == current.presence_version
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactorTrace {
    pub module_id: ModuleId,
    pub relation_id: RelationId,
    pub factor_ids: Vec<FactorId>,
    pub candidate_id: String,
    pub plausibility: u32,
    pub curation: u32,
    pub accepted: bool,
    pub hard_zero_reason: Option<String>,
    pub required_bridge: Option<BridgeId>,
    pub decision: TraceDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceDecision {
    Candidate,
    Bound,
    ForwardRejected,
    Backtracked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CausalBridge {
    pub id: BridgeId,
    pub explanation: String,
    pub event_id: String,
    pub evidence_id: EvidenceId,
    pub action_id: ActionId,
    pub lead_summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub id: String,
    pub proposition_id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub occurred_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceProfile {
    pub symptom: Symptom,
    pub effects: Effects,
    pub public_summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedSite {
    pub id: SiteId,
    pub kind: SiteKind,
    pub role: SiteRole,
    pub terrain: Terrain,
    pub safe_label: String,
    pub exact_location_initially_known: bool,
    pub is_true_location: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedArea {
    pub id: String,
    pub safe_label: String,
    pub terrain: Terrain,
    pub contains_site_ids: Vec<SiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestimonyDraft {
    pub proposition_id: String,
    pub reliability: Reliability,
    pub truthful_text: String,
    pub spoken_text: String,
    pub destination_stage: String,
    pub site_id: Option<SiteId>,
    /// Proposition superseded by this claim. Set only on the later correction.
    pub corrects_proposition_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessBinding {
    pub id: WitnessId,
    pub npc_id: String,
    pub demographic: WitnessDemographic,
    pub circumstance: Circumstance,
    pub description: ReportDescription,
    pub expected_location: String,
    pub expected_location_label: String,
    pub visible_description: String,
    pub testimony: Vec<TestimonyDraft>,
}

/// Exact player-visible tab label for every referral projection. The raw
/// location ID remains separate authority for presence checks.
pub fn referral_display_location(witness: &WitnessBinding) -> &str {
    &witness.expected_location_label
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedEvidence {
    pub id: EvidenceId,
    pub kind: EvidenceKind,
    pub proposition_id: String,
    pub site_id: SiteId,
    pub safe_description: String,
    pub corrects_proposition_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedAction {
    pub id: ActionId,
    pub kind: InvestigationActionKind,
    pub route: RouteClass,
    pub target_kind: String,
    pub target_id: String,
    pub prerequisite: Option<ActionId>,
    pub alternate: ActionId,
    pub active_initially: bool,
    pub safe_summary: String,
    pub outputs: Vec<GeneratedActionOutput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferredContactActionState {
    pub id: String,
    pub owner_character_id: u64,
    pub case_id: String,
    pub method: String,
    pub target_kind: String,
    pub target_id: String,
    pub required_action_id: String,
    pub active: bool,
    pub version: u32,
    pub successful_attempt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferredContactTransition {
    NotApplicable,
    Replay,
    Applied {
        root_id: String,
        expected_version: u32,
        next_version: u32,
        activated_successor_ids: Vec<String>,
        attempt_success: bool,
        outcome_wording: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailedActionAlternateTransition {
    Activated { alternate_id: String },
    Unavailable,
}

pub fn transition_failed_action_alternate(
    states: &mut [ReferredContactActionState],
    owner_character_id: u64,
    canonical_case_id: &str,
    alternate_id: &str,
) -> Result<FailedActionAlternateTransition, &'static str> {
    let Some(alternate_index) = states.iter().position(|candidate| {
        candidate.id == alternate_id
            && candidate.owner_character_id == owner_character_id
            && candidate.case_id == canonical_case_id
    }) else {
        return Err("Investigation recovery route no longer matches its case");
    };
    if states[alternate_index].successful_attempt {
        return Ok(FailedActionAlternateTransition::Unavailable);
    }
    let prerequisite_id = states[alternate_index].required_action_id.clone();
    if !prerequisite_id.is_empty()
        && !states.iter().any(|candidate| {
            candidate.id == prerequisite_id
                && candidate.owner_character_id == owner_character_id
                && candidate.case_id == canonical_case_id
                && candidate.successful_attempt
        })
    {
        return Ok(FailedActionAlternateTransition::Unavailable);
    }
    states[alternate_index].active = true;
    Ok(FailedActionAlternateTransition::Activated {
        alternate_id: alternate_id.into(),
    })
}

pub const fn failed_action_outcome_wording(alternate_available: bool) -> &'static str {
    if alternate_available {
        "No conclusive result. Time passed, but another supported route remains available."
    } else {
        "No conclusive result. Time passed, and no alternate route is currently supported by the leads in your journal."
    }
}

pub fn exact_referral_contact(expected_npc_id: &str, addressed_npc_id: &str) -> bool {
    expected_npc_id == addressed_npc_id
}

pub fn generated_testimony_projection_plan(
    witness: &WitnessBinding,
) -> Result<Vec<TestimonyDraft>, &'static str> {
    if witness.testimony.is_empty() {
        Err("Generated witness has no proposition testimony")
    } else {
        Ok(witness.testimony.clone())
    }
}

pub fn transition_referred_contact_action(
    states: &mut [ReferredContactActionState],
    owner_character_id: u64,
    canonical_case_id: &str,
    witness_npc_id: &str,
) -> Result<ReferredContactTransition, &'static str> {
    let matches: Vec<_> = states
        .iter()
        .enumerate()
        .filter(|(_, capability)| {
            capability.owner_character_id == owner_character_id
                && capability.case_id == canonical_case_id
                && capability.method == "locate_contact"
                && capability.target_kind == "contact"
                && capability.target_id == witness_npc_id
        })
        .map(|(index, _)| index)
        .collect();
    if matches.len() > 1 {
        return Err("Referred witness matches multiple contact actions");
    }
    let Some(root_index) = matches.first().copied() else {
        return Ok(ReferredContactTransition::NotApplicable);
    };
    if !states[root_index].active {
        return Ok(if states[root_index].successful_attempt {
            ReferredContactTransition::Replay
        } else {
            ReferredContactTransition::NotApplicable
        });
    }
    let root_id = states[root_index].id.clone();
    let expected_version = states[root_index].version;
    let successor_indices: Vec<_> = states
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.owner_character_id == owner_character_id
                && candidate.case_id == canonical_case_id
                && candidate.required_action_id == root_id
        })
        .map(|(index, _)| index)
        .collect();
    let activated_successor_ids = successor_indices
        .iter()
        .map(|index| states[*index].id.clone())
        .collect();
    states[root_index].active = false;
    states[root_index].version = states[root_index].version.saturating_add(1);
    states[root_index].successful_attempt = true;
    for index in successor_indices {
        states[index].active = true;
    }
    Ok(ReferredContactTransition::Applied {
        root_id,
        expected_version,
        next_version: states[root_index].version,
        activated_successor_ids,
        attempt_success: true,
        outcome_wording: "The referred witness gave their account.".into(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedActionOutput {
    Destination {
        stage: GeneratedDestinationStage,
        site_id: Option<SiteId>,
    },
    Evidence {
        evidence_id: EvidenceId,
    },
    PatternCondition {
        evidence_id: EvidenceId,
        condition: GeneratedPatternCondition,
    },
    AmbushReady,
    Consequence {
        consequence: GeneratedActionConsequence,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedPatternCondition {
    NightWindow,
    RoadRoute,
    VictimProfile {
        cohort_id: String,
        demographic: WitnessDemographic,
        age_band: String,
        sex: String,
        profession: String,
    },
    BroadSurvey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedDestinationStage {
    Unknown,
    Textual,
    Landmark,
    ApproximateArea,
    RouteSegment,
    Exact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedActionConsequence {
    RetrieveAsset {
        asset_id: String,
        next_version: u32,
    },
    RescueSubject {
        subject_id: String,
        next_version: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedFinale {
    pub id: FinaleId,
    pub kind: FinaleKind,
    pub site_id: SiteId,
    pub hostile_group_id: Option<String>,
    pub subject_id: Option<String>,
    pub asset_id: Option<String>,
    pub strategic_outcome_compatible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedDialogueProducer {
    pub action: GeneratedDialogueAction,
    pub objective_id: ObjectiveId,
    pub recipient_npc_id: String,
    pub subject_ref: Option<String>,
    pub asset_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedCase {
    pub catalog_revision: String,
    pub generation_seed: u64,
    pub family: TemplateFamily,
    pub canonical_case_id: String,
    pub public_case_id: String,
    pub problem_id: String,
    pub cause: CanonicalCause,
    pub canonical_events: Vec<CanonicalEvent>,
    pub consequence: ConsequenceProfile,
    pub sites: Vec<GeneratedSite>,
    pub areas: Vec<GeneratedArea>,
    pub witnesses: Vec<WitnessBinding>,
    pub pattern_targets: Vec<GeneratedPatternTarget>,
    pub evidence: Vec<GeneratedEvidence>,
    pub actions: Vec<GeneratedAction>,
    pub objectives: ObjectiveExpression,
    pub custody: Vec<(String, SiteId)>,
    pub hostile_groups: Vec<(String, SiteId, ThreatId, u32)>,
    pub finales: Vec<GeneratedFinale>,
    pub dialogue_producers: Vec<GeneratedDialogueProducer>,
    pub bridges: Vec<CausalBridge>,
    /// Private diagnostic authority. Never place this in a public table/view.
    pub factor_trace: Vec<FactorTrace>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationError {
    NoCandidates {
        module: ModuleId,
        diagnostics: Vec<FactorTrace>,
    },
    CandidateLimit,
    InvalidManifest(Vec<String>),
}

#[derive(Clone)]
struct Candidate<T> {
    id: &'static str,
    value: T,
    weight: Weight,
    bridge: Option<&'static str>,
    impossible: Option<&'static str>,
    factors: Vec<&'static str>,
}

fn hash(seed: u64, domain: &str) -> u64 {
    domain.bytes().fold(seed ^ 0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

fn scoped_id(scope: &str, kind: &str, name: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"adventuresim.quest.observer-id.v1\0");
    digest.update(scope.as_bytes());
    digest.update([0]);
    digest.update(kind.as_bytes());
    digest.update([0]);
    digest.update(name.as_bytes());
    format!("{kind}:{}", &format!("{:x}", digest.finalize())[..24])
}

fn observer_scope(context: &GenerationContext) -> String {
    let mut digest = Sha256::new();
    digest.update(b"adventuresim.quest.observer-scope.v1\0");
    digest.update(context.observer_entropy_hi.to_le_bytes());
    digest.update(context.observer_entropy_lo.to_le_bytes());
    digest.update(context.ordinal.to_le_bytes());
    digest.update(context.settlement_id.as_bytes());
    format!("{:x}", digest.finalize())
}

/// Mints an opaque observer-facing identifier from private persisted entropy.
/// The caller must never expose the generation context itself.
pub fn observer_scoped_id(context: &GenerationContext, kind: &str, name: &str) -> String {
    scoped_id(&observer_scope(context), kind, name)
}

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
        perception: PerceptionCondition::Clear,
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
    [
        Reliability::Truthful,
        Reliability::Mistaken,
        Reliability::Evasive,
        Reliability::Deceptive,
        Reliability::PartlyTruthful,
    ]
    .into_iter()
    .map(|value| {
        let (p, impossible, factor) = match (demographic, circumstance, cause, value) {
            (WitnessDemographic::Child, _, _, Reliability::Deceptive) => (
                0,
                Some("the initial child account is not authored as deliberate fabrication"),
                "factor.reliability.child_hard_zero",
            ),
            (
                _,
                Circumstance::SecretRiversideMeeting | Circumstance::AdultVenue,
                _,
                Reliability::Evasive,
            ) => (75, None, "factor.reliability.embarrassing_context"),
            (_, _, CanonicalCause::FabricatedClaim, Reliability::Deceptive) => {
                (80, None, "factor.reliability.fabricated_claim")
            }
            (_, Circumstance::NightWindow, _, Reliability::Mistaken) => {
                (70, None, "factor.reliability.darkness")
            }
            (_, _, _, Reliability::Truthful) => (65, None, "factor.reliability.baseline"),
            (_, _, _, Reliability::PartlyTruthful) => (45, None, "factor.reliability.partial"),
            _ => (25, None, "factor.reliability.possible"),
        };
        Candidate {
            id: match value {
                Reliability::Truthful => "reliability.truthful",
                Reliability::Mistaken => "reliability.mistaken",
                Reliability::Evasive => "reliability.evasive",
                Reliability::Deceptive => "reliability.deceptive",
                Reliability::PartlyTruthful => "reliability.partly_truthful",
            },
            value,
            weight: Weight::new(p, 70),
            bridge: None,
            impossible,
            factors: vec![factor],
        }
    })
    .collect()
}

fn evidence_candidates(cause: CanonicalCause, site: SiteKind) -> Vec<Candidate<EvidenceKind>> {
    [
        EvidenceKind::Footprints,
        EvidenceKind::ClothScrap,
        EvidenceKind::BoneDust,
        EvidenceKind::BloodlessCorpse,
        EvidenceKind::DroppedToken,
        EvidenceKind::DragMarks,
        EvidenceKind::LedgerEntry,
    ]
    .into_iter()
    .map(|value| {
        let (p, impossible, factor) = match (cause, site, value) {
            (CanonicalCause::Hostile(ThreatId::Skeleton), _, EvidenceKind::BoneDust) => {
                (90, None, "factor.evidence.skeleton")
            }
            (CanonicalCause::IncidentalLoss, _, EvidenceKind::LedgerEntry) => {
                (70, None, "factor.evidence.asset_record")
            }
            (CanonicalCause::FabricatedClaim, _, EvidenceKind::BloodlessCorpse) => (
                0,
                Some("a fabricated loss cannot create a canonical corpse clue"),
                "factor.evidence.fabrication_hard_zero",
            ),
            (_, SiteKind::Roadside | SiteKind::Riverside, EvidenceKind::Footprints) => {
                (75, None, "factor.evidence.trackable_ground")
            }
            (_, _, EvidenceKind::DroppedToken | EvidenceKind::ClothScrap) => {
                (45, None, "factor.evidence.portable")
            }
            _ => (25, None, "factor.evidence.possible"),
        };
        Candidate {
            id: match value {
                EvidenceKind::Footprints => "evidence.footprints",
                EvidenceKind::ClothScrap => "evidence.cloth",
                EvidenceKind::BoneDust => "evidence.bone_dust",
                EvidenceKind::BloodlessCorpse => "evidence.bloodless_corpse",
                EvidenceKind::DroppedToken => "evidence.token",
                EvidenceKind::DragMarks => "evidence.drag_marks",
                EvidenceKind::LedgerEntry => "evidence.ledger",
            },
            value,
            weight: Weight::new(p, 70),
            bridge: None,
            impossible,
            factors: vec![factor],
        }
    })
    .collect()
}

fn account_style_candidates(
    reliability: Reliability,
    circumstance: Circumstance,
) -> Vec<Candidate<AccountStyle>> {
    [
        AccountStyle::VisualClaim,
        AccountStyle::HeardOnly,
        AccountStyle::TracksAndMovement,
    ]
    .into_iter()
    .map(|value| {
        let (p, impossible, factor) = match (circumstance, reliability, value) {
            (Circumstance::NightWindow, _, AccountStyle::HeardOnly) => {
                (80, None, "factor.account.darkness")
            }
            (_, Reliability::Mistaken, AccountStyle::TracksAndMovement) => (
                0,
                Some("a mistaken eyewitness does not provide the precise track account"),
                "factor.account.mistaken_hard_zero",
            ),
            (_, _, AccountStyle::VisualClaim) => (60, None, "factor.account.visual"),
            (_, _, AccountStyle::TracksAndMovement) => (45, None, "factor.account.tracks"),
            _ => (30, None, "factor.account.possible"),
        };
        Candidate {
            id: match value {
                AccountStyle::VisualClaim => "account.visual",
                AccountStyle::HeardOnly => "account.heard",
                AccountStyle::TracksAndMovement => "account.tracks",
            },
            value,
            weight: Weight::new(p, 70),
            bridge: None,
            impossible,
            factors: vec![factor],
        }
    })
    .collect()
}

fn route_variant_candidates(family: TemplateFamily) -> [Candidate<RouteVariant>; 2] {
    [
        Candidate {
            id: "route.direct",
            value: RouteVariant::Direct,
            weight: Weight::new(
                if family == TemplateFamily::RecurringDepredation {
                    70
                } else {
                    55
                },
                70,
            ),
            bridge: None,
            impossible: None,
            factors: vec!["factor.route.direct"],
        },
        Candidate {
            id: "route.cautious",
            value: RouteVariant::Cautious,
            weight: Weight::new(
                if family == TemplateFamily::RecurringDepredation {
                    45
                } else {
                    75
                },
                70,
            ),
            bridge: None,
            impossible: None,
            factors: vec!["factor.route.cautious"],
        },
    ]
}

fn attack_pattern_candidates(
    family: TemplateFamily,
    has_victim_target: bool,
) -> [Candidate<AttackPattern>; 4] {
    [
        Candidate {
            id: "pattern.nightly",
            value: AttackPattern::Nightly,
            weight: Weight::new(60, 70),
            bridge: None,
            impossible: None,
            factors: vec!["factor.pattern.nightly"],
        },
        Candidate {
            id: "pattern.roadside",
            value: AttackPattern::Roadside,
            weight: Weight::new(55, 70),
            bridge: None,
            impossible: None,
            factors: vec!["factor.pattern.roadside"],
        },
        Candidate {
            id: "pattern.victim_specific",
            value: AttackPattern::VictimSpecific,
            weight: Weight::new(
                if !has_victim_target {
                    0
                } else if family == TemplateFamily::DisappearanceOrLoss {
                    70
                } else {
                    30
                },
                65,
            ),
            bridge: None,
            impossible: (!has_victim_target)
                .then_some("no unused persistent NPC can anchor the victim cohort"),
            factors: vec!["factor.pattern.victim", "factor.pattern.persistent_cohort"],
        },
        Candidate {
            id: "pattern.irregular",
            value: AttackPattern::Irregular,
            weight: Weight::new(35, 55),
            bridge: None,
            impossible: None,
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

#[derive(Clone, Copy)]
struct SolvedVariables {
    family: TemplateFamily,
    cause: CanonicalCause,
    site: SiteKind,
    demographic: WitnessDemographic,
    circumstance: Circumstance,
    description: ReportDescription,
    site_bridge: Option<&'static str>,
    circumstance_bridge: Option<&'static str>,
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
                                None,
                                families[family_index].factors.clone(),
                            ),
                            (
                                "module.cause",
                                format!("{cause:?}"),
                                None,
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
                                None,
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
                            site_bridge: sites[site_index].bridge,
                            circumstance_bridge: circumstances[circumstance_index].bridge,
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

fn family_candidates() -> [Candidate<TemplateFamily>; 2] {
    [
        Candidate {
            id: "family.recurring_depredation",
            value: TemplateFamily::RecurringDepredation,
            weight: Weight::new(100, 100),
            bridge: None,
            impossible: None,
            factors: vec!["factor.family.rotation"],
        },
        Candidate {
            id: "family.disappearance_or_loss",
            value: TemplateFamily::DisappearanceOrLoss,
            weight: Weight::new(100, 100),
            bridge: None,
            impossible: None,
            factors: vec!["factor.family.rotation"],
        },
    ]
}

fn cause_candidates(family: TemplateFamily) -> Vec<Candidate<CanonicalCause>> {
    let loss = family == TemplateFamily::DisappearanceOrLoss;
    let mut values = vec![
        (ThreatId::Bandit, 75, 80),
        (ThreatId::Goblin, if loss { 30 } else { 70 }, 75),
        (ThreatId::Ghoul, 40, 70),
        (ThreatId::Skeleton, 35, 70),
        (ThreatId::Werewolf, if loss { 25 } else { 45 }, 60),
        (ThreatId::Smuggler, if loss { 60 } else { 25 }, 65),
        (ThreatId::Wolf, if loss { 20 } else { 65 }, 65),
    ]
    .into_iter()
    .map(|(threat, p, c)| Candidate {
        id: threat.as_str(),
        value: CanonicalCause::Hostile(threat),
        weight: Weight::new(p, c),
        bridge: None,
        impossible: None,
        factors: vec!["factor.cause.bestiary"],
    })
    .collect::<Vec<_>>();
    if loss {
        values.extend([
            Candidate {
                id: "cause.concealment",
                value: CanonicalCause::ConcealmentByWitness,
                weight: Weight::new(35, 75),
                bridge: None,
                impossible: None,
                factors: vec!["factor.cause.nonhostile"],
            },
            Candidate {
                id: "cause.incidental_loss",
                value: CanonicalCause::IncidentalLoss,
                weight: Weight::new(40, 65),
                bridge: None,
                impossible: None,
                factors: vec!["factor.cause.nonhostile"],
            },
            Candidate {
                id: "cause.fabricated",
                value: CanonicalCause::FabricatedClaim,
                weight: Weight::new(20, 55),
                bridge: None,
                impossible: None,
                factors: vec!["factor.cause.nonhostile"],
            },
        ]);
    }
    values
}

fn site_candidates(cause: CanonicalCause) -> Vec<Candidate<SiteKind>> {
    use SiteKind as S;
    [
        S::Cave,
        S::Crypt,
        S::ForestCamp,
        S::OccupiedHouse,
        S::Riverside,
        S::Graveyard,
        S::Roadside,
        S::AbandonedFarm,
    ]
    .into_iter()
    .map(|site| {
        let (p, bridge, impossible, factor) = match (cause, site) {
            (CanonicalCause::Hostile(ThreatId::Skeleton), S::Crypt)
            | (CanonicalCause::Hostile(ThreatId::Ghoul), S::Graveyard) => {
                (95, None, None, "factor.site.natural_habitat")
            }
            (CanonicalCause::Hostile(ThreatId::Bandit), S::ForestCamp)
            | (CanonicalCause::Hostile(ThreatId::Goblin), S::Cave)
            | (CanonicalCause::Hostile(ThreatId::Wolf), S::AbandonedFarm) => {
                (80, None, None, "factor.site.common")
            }
            (CanonicalCause::Hostile(ThreatId::Werewolf), S::OccupiedHouse)
            | (CanonicalCause::Hostile(ThreatId::Smuggler), S::Riverside) => {
                (90, None, None, "factor.site.concealment")
            }
            (CanonicalCause::Hostile(ThreatId::Skeleton), S::OccupiedHouse) => (
                3,
                Some("bridge.skeletons_occupied_house"),
                None,
                "factor.site.rare_bridge",
            ),
            (CanonicalCause::Hostile(ThreatId::Wolf), S::Crypt) => (
                0,
                None,
                Some("quadruped pack cannot maintain a sealed crypt"),
                "factor.site.impossible",
            ),
            (
                CanonicalCause::VoluntaryDisappearance | CanonicalCause::ConcealmentByWitness,
                S::OccupiedHouse | S::Riverside,
            ) => (85, None, None, "factor.site.social"),
            (CanonicalCause::IncidentalLoss, S::Roadside | S::Riverside) => {
                (80, None, None, "factor.site.accident")
            }
            (CanonicalCause::FabricatedClaim, S::OccupiedHouse) => {
                (80, None, None, "factor.site.fabrication")
            }
            (_, S::OccupiedHouse) => (12, None, None, "factor.site.unusual"),
            (_, S::Roadside) => (25, None, None, "factor.site.transit"),
            _ => (20, None, None, "factor.site.possible"),
        };
        Candidate {
            id: site_id(site),
            value: site,
            weight: Weight::new(p, 70),
            bridge,
            impossible,
            factors: vec![factor],
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
    use Circumstance as C;
    [
        C::NightWindow,
        C::SecretRiversideMeeting,
        C::AdultVenue,
        C::RoadJourney,
        C::GraveDuty,
        C::LivestockWatch,
    ]
    .into_iter()
    .map(|circ| {
        let (p, bridge, impossible, factor) = match (demo, circ) {
            (WitnessDemographic::Child, C::AdultVenue) => (
                2,
                Some("bridge.child_at_adult_venue"),
                None,
                "factor.witness.rare_venue",
            ),
            (WitnessDemographic::Cleric, C::AdultVenue) => (
                0,
                None,
                Some("assigned cleric witness is not present in the adult venue"),
                "factor.witness.impossible_venue",
            ),
            (WitnessDemographic::Child, C::NightWindow) => {
                (90, None, None, "factor.witness.household")
            }
            (_, C::RoadJourney) => (55, None, None, "factor.witness.travel"),
            (_, C::SecretRiversideMeeting) => (25, None, None, "factor.witness.private"),
            _ => (35, None, None, "factor.witness.general"),
        };
        Candidate {
            id: circumstance_id(circ),
            value: circ,
            weight: Weight::new(p, 70),
            bridge,
            impossible,
            factors: vec![factor],
        }
    })
    .collect()
}

fn description_candidates(cause: CanonicalCause) -> Vec<Candidate<ReportDescription>> {
    ALL_REPORTS
        .iter()
        .copied()
        .map(|report| {
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
                id: report_id(report),
                value: report,
                weight: Weight::new(p, 80),
                bridge: None,
                impossible: (p == 0).then_some("bestiary forward description likelihood is zero"),
                factors: vec!["factor.description.bestiary_forward_likelihood"],
            }
        })
        .collect()
}

fn site_id(site: SiteKind) -> &'static str {
    match site {
        SiteKind::Cave => "site.cave",
        SiteKind::Crypt => "site.crypt",
        SiteKind::ForestCamp => "site.forest_camp",
        SiteKind::OccupiedHouse => "site.occupied_house",
        SiteKind::Riverside => "site.riverside",
        SiteKind::Graveyard => "site.graveyard",
        SiteKind::Roadside => "site.roadside",
        SiteKind::AbandonedFarm => "site.abandoned_farm",
    }
}
fn circumstance_id(v: Circumstance) -> &'static str {
    match v {
        Circumstance::NightWindow => "circumstance.night_window",
        Circumstance::SecretRiversideMeeting => "circumstance.secret_riverside",
        Circumstance::AdultVenue => "circumstance.adult_venue",
        Circumstance::RoadJourney => "circumstance.road",
        Circumstance::GraveDuty => "circumstance.grave_duty",
        Circumstance::LivestockWatch => "circumstance.livestock_watch",
    }
}
fn report_id(v: ReportDescription) -> &'static str {
    match v {
        ReportDescription::ArmedPeople => "description.armed_people",
        ReportDescription::SmallUprightFigures => "description.small_upright",
        ReportDescription::LargeUprightBeast => "description.large_upright",
        ReportDescription::GauntHuman => "description.gaunt_human",
        ReportDescription::WalkingDead => "description.walking_dead",
        ReportDescription::LargeAnimal => "description.large_animal",
        ReportDescription::DoglikeBeast => "description.doglike",
        ReportDescription::UnseenNightVisitor => "description.unseen",
    }
}

fn ambiguous_report_description(v: ReportDescription) -> &'static str {
    match v {
        ReportDescription::ArmedPeople => "a group of armed figures",
        ReportDescription::SmallUprightFigures => "several small figures moving upright",
        ReportDescription::LargeUprightBeast => "a large shape that seemed to stand upright",
        ReportDescription::GauntHuman => "a gaunt, human-shaped figure",
        ReportDescription::WalkingDead => "a person moving with a stiff, shambling gait",
        ReportDescription::LargeAnimal => "the silhouette of a large animal",
        ReportDescription::DoglikeBeast => "a low, dog-shaped beast",
        ReportDescription::UnseenNightVisitor => "something hidden in the darkness",
    }
}

fn ambiguous_visual_claim(v: ReportDescription, place: &str) -> String {
    format!(
        "It looked like {}, near {place}.",
        ambiguous_report_description(v)
    )
}

fn terrain(site: SiteKind) -> Terrain {
    match site {
        SiteKind::Cave | SiteKind::Crypt => Terrain::Underground,
        SiteKind::ForestCamp | SiteKind::AbandonedFarm => Terrain::Forest,
        SiteKind::OccupiedHouse | SiteKind::Graveyard => Terrain::Settlement,
        SiteKind::Riverside | SiteKind::Roadside => Terrain::Road,
    }
}
fn label(site: SiteKind) -> &'static str {
    match site {
        SiteKind::Cave => "a cave beyond the fields",
        SiteKind::Crypt => "the old crypt",
        SiteKind::ForestCamp => "a camp in the woods",
        SiteKind::OccupiedHouse => "an occupied house",
        SiteKind::Riverside => "a secluded bend in the river",
        SiteKind::Graveyard => "the old graveyard",
        SiteKind::Roadside => "a lonely stretch of road",
        SiteKind::AbandonedFarm => "an abandoned farm",
    }
}

fn bridge(id: &str, prefix: &str, family: TemplateFamily, _now: u64) -> CausalBridge {
    let action_name = match (id, family) {
        ("bridge.skeletons_occupied_house", TemplateFamily::RecurringDepredation) => "search",
        ("bridge.skeletons_occupied_house", TemplateFamily::DisappearanceOrLoss) => {
            "inspect_last_known"
        }
        ("bridge.child_at_adult_venue", TemplateFamily::RecurringDepredation) => "watch",
        ("bridge.child_at_adult_venue", TemplateFamily::DisappearanceOrLoss) => "locate_contact",
        (_, _) => "approach",
    };
    let action_id = ActionId::new(scoped_id(prefix, "action", action_name));
    match id {
        "bridge.skeletons_occupied_house" => CausalBridge {
            id: BridgeId::new(id),
            explanation: "A graverobber moved animated remains into a shuttered house.".into(),
            event_id: scoped_id(prefix, "event", "bridge:skeleton_house"),
            evidence_id: EvidenceId::new(scoped_id(prefix, "evidence", "grave_clay")),
            action_id,
            lead_summary: "Grave clay and cart ruts connect the house to the crypt.".into(),
        },
        "bridge.child_at_adult_venue" => CausalBridge {
            id: BridgeId::new(id),
            explanation: "The child was fetching an adult relative from outside the venue.".into(),
            event_id: scoped_id(prefix, "event", "bridge:child_venue"),
            evidence_id: EvidenceId::new(scoped_id(prefix, "evidence", "errand_token")),
            action_id,
            lead_summary: "An errand token corroborates why the child waited outside.".into(),
        },
        _ => CausalBridge {
            id: BridgeId::new(id),
            explanation: "A rare causal link makes the combination possible.".into(),
            event_id: scoped_id(prefix, "event", "bridge"),
            evidence_id: EvidenceId::new(scoped_id(prefix, "evidence", "bridge")),
            action_id,
            lead_summary: "A corroborating clue explains the unusual combination.".into(),
        },
    }
}

fn consequence(cause: CanonicalCause, family: TemplateFamily) -> ConsequenceProfile {
    let (symptom, effects, summary) = match (family, cause) {
        (
            TemplateFamily::RecurringDepredation,
            CanonicalCause::Hostile(ThreatId::Ghoul | ThreatId::Werewolf),
        ) => (
            Symptom::NightScreams,
            Effects {
                buy_bps: 400,
                sell_penalty_bps: 200,
                encounter_frequency_bps: 700,
                encounter_archetype: Some(EncounterArchetype::Undead),
                disease_intensity: 180,
            },
            "Locals report troubling sounds and disappearances after dark.",
        ),
        (
            TemplateFamily::RecurringDepredation,
            CanonicalCause::Hostile(ThreatId::Wolf | ThreatId::Goblin),
        ) => (
            Symptom::VanishedLivestock,
            Effects {
                buy_bps: 700,
                sell_penalty_bps: 300,
                encounter_frequency_bps: 1000,
                encounter_archetype: Some(EncounterArchetype::Goblins),
                disease_intensity: 0,
            },
            "Livestock have been disappearing from nearby holdings.",
        ),
        (TemplateFamily::RecurringDepredation, _) => (
            Symptom::MissingCaravans,
            Effects {
                buy_bps: 1200,
                sell_penalty_bps: 500,
                encounter_frequency_bps: 1500,
                encounter_archetype: Some(EncounterArchetype::Bandits),
                disease_intensity: 0,
            },
            "Several expected caravans have not arrived.",
        ),
        (TemplateFamily::DisappearanceOrLoss, _) => (
            Symptom::EmptyStalls,
            Effects {
                buy_bps: 900,
                sell_penalty_bps: 400,
                encounter_frequency_bps: 500,
                encounter_archetype: None,
                disease_intensity: 0,
            },
            "A disappearance has disrupted work and trade, but nobody agrees on the cause.",
        ),
    };
    ConsequenceProfile {
        symptom,
        effects,
        public_summary: summary.into(),
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
) -> Vec<GeneratedAction> {
    let trail_summary = match route_variant {
        RouteVariant::Direct => "Follow the physical trail directly.",
        RouteVariant::Cautious => "Reacquire and follow the trail cautiously.",
    };
    let trail_kind = match route_variant {
        RouteVariant::Direct => InvestigationActionKind::FollowTracks,
        RouteVariant::Cautious => InvestigationActionKind::ReacquireTracks,
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
                "Watch likely {:?} victims ({}, {}, {}) near the learned location.",
                target.demographic, target.age_band, target.sex, target.profession
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
        outputs,
    };
    match family {
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
                "follow",
                trail_kind,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("search"),
                "reveal_route",
                false,
                trail_summary,
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
                trail_kind,
                RouteClass::PatternSurveillance,
                "route",
                finale.0.clone(),
                Some("patrol"),
                "follow",
                false,
                trail_summary,
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
                "follow",
                trail_kind,
                RouteClass::PhysicalTrail,
                "site",
                finale.0.clone(),
                Some("inspect_last_known"),
                "approach_social",
                false,
                trail_summary,
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
    }
}

pub fn generate(context: &GenerationContext) -> Result<GeneratedCase, GenerationError> {
    let mut trace = Vec::new();
    let solved = solve_variables(context, &mut trace)?;
    let SolvedVariables {
        family,
        cause,
        site,
        demographic,
        circumstance,
        description,
        site_bridge,
        circumstance_bridge: circ_bridge,
        primary_witness,
        secondary_witness,
    } = solved;
    let primary = &context.witness_candidates[primary_witness];
    let secondary = &context.witness_candidates[secondary_witness];
    let prefix = observer_scope(context);
    let (reliability, _) = choose(
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
    let (evidence_kind, _) = choose(
        context.seed.rotate_left(17),
        "module.evidence",
        "relation.evidence.cause_site",
        &evidence_candidates(cause, site),
        &mut trace,
    )?;
    let (account_style, _) = choose(
        context.seed.rotate_left(23),
        "module.account",
        "relation.account.reliability_circumstance",
        &account_style_candidates(reliability, circumstance),
        &mut trace,
    )?;
    let (route_variant, _) = choose(
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
                context.witness_candidates[*index].npc_id
            ),
        )
    });
    let (attack_pattern, _) = choose(
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
            npc_id: candidate.npc_id.clone(),
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
    let npc1 = primary.npc_id.clone();
    let npc2 = secondary.npc_id.clone();
    let unreliable_statement = match account_style {
        AccountStyle::VisualClaim => {
            ambiguous_visual_claim(description, label(secondary_site_kind))
        }
        AccountStyle::HeardOnly => format!(
            "I only heard it moving near {}; I never saw it clearly.",
            label(secondary_site_kind)
        ),
        AccountStyle::TracksAndMovement => format!(
            "Its trail and movement seemed to point toward {}.",
            label(secondary_site_kind)
        ),
    };
    let true_statement = format!(
        "I saw signs pointing toward {}, but I could not identify the culprit.",
        label(site)
    );
    let description_prop = scoped_id(&prefix, "proposition", "description");
    let correction_prop = scoped_id(&prefix, "proposition", "location:corrected");
    let pattern_prop = scoped_id(&prefix, "proposition", "attack-pattern");
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
                "The incidents disproportionately affect {:?} people ({}, {}, {}) near {}.",
                target.demographic,
                target.age_band,
                target.sex,
                target.profession,
                target.expected_location_label
            )
        }
        AttackPattern::Irregular => {
            "The incidents have no reliable time, place, or victim schedule.".to_owned()
        }
    };
    let uncorroborated_pattern_claim = if reliability == Reliability::Truthful {
        pattern_truth.clone()
    } else {
        match attack_pattern {
            AttackPattern::Nightly => "I think it happens at all hours.".to_owned(),
            AttackPattern::Roadside => "I doubt the road has anything to do with it.".to_owned(),
            AttackPattern::VictimSpecific => "The victims seem entirely random to me.".to_owned(),
            AttackPattern::Irregular => "I am sure it always happens just after dusk.".to_owned(),
        }
    };
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
            safe_label: if family == TemplateFamily::RecurringDepredation {
                "the latest incident site".into()
            } else {
                "the last-known place".into()
            },
            exact_location_initially_known: true,
            is_true_location: false,
        },
        GeneratedSite {
            id: decoy_site.clone(),
            kind: secondary_site_kind,
            role: SiteRole::Decoy,
            terrain: terrain(secondary_site_kind),
            safe_label: format!(
                "a plausible but unconfirmed place near {}",
                label(secondary_site_kind)
            ),
            exact_location_initially_known: false,
            is_true_location: false,
        },
    ];
    let witnesses = vec![
        WitnessBinding {
            id: witness1.clone(),
            npc_id: npc1,
            demographic,
            circumstance,
            description,
            expected_location: primary.expected_location.clone(),
            expected_location_label: primary.expected_location_label.clone(),
            visible_description: primary.visible_description.clone(),
            testimony: vec![
                TestimonyDraft {
                    proposition_id: description_prop.clone(),
                    reliability,
                    truthful_text: true_statement.clone(),
                    spoken_text: if reliability == Reliability::Truthful {
                        true_statement
                    } else {
                        unreliable_statement
                    },
                    destination_stage: if reliability == Reliability::Truthful {
                        "approximate_area"
                    } else {
                        "exact_believed"
                    }
                    .into(),
                    site_id: Some(if reliability == Reliability::Truthful {
                        finale_site.clone()
                    } else {
                        decoy_site.clone()
                    }),
                    corrects_proposition_id: None,
                },
                TestimonyDraft {
                    proposition_id: pattern_prop.clone(),
                    reliability,
                    truthful_text: pattern_truth.clone(),
                    spoken_text: uncorroborated_pattern_claim,
                    destination_stage: "textual".into(),
                    site_id: None,
                    corrects_proposition_id: None,
                },
            ],
        },
        WitnessBinding {
            id: witness2.clone(),
            npc_id: npc2,
            demographic: secondary.demographic,
            circumstance: secondary_circumstance,
            description,
            expected_location: secondary.expected_location.clone(),
            expected_location_label: secondary.expected_location_label.clone(),
            visible_description: secondary.visible_description.clone(),
            testimony: vec![TestimonyDraft {
                proposition_id: description_prop.clone(),
                reliability: Reliability::Truthful,
                truthful_text: "The earlier location does not fit the tracks; they lead elsewhere."
                    .into(),
                spoken_text: format!(
                    "Those tracks turn away from {} and continue toward the true site.",
                    label(secondary_site_kind)
                ),
                destination_stage: "route_segment".into(),
                site_id: Some(finale_site.clone()),
                corrects_proposition_id: Some(description_prop.clone()),
            }],
        },
    ];
    let mut evidence = vec![
        GeneratedEvidence {
            id: EvidenceId::new(scoped_id(&prefix, "evidence", "tracks")),
            kind: evidence_kind,
            proposition_id: correction_prop.clone(),
            site_id: evidence_site.clone(),
            safe_description: format!(
                "This {:?} clue preserves a useful lead without identifying the culprit outright.",
                evidence_kind,
            ),
            corrects_proposition_id: Some(scoped_id(&prefix, "proposition", "description")),
        },
        GeneratedEvidence {
            id: EvidenceId::new(scoped_id(&prefix, "evidence", "token")),
            kind: EvidenceKind::DroppedToken,
            proposition_id: scoped_id(&prefix, "proposition", "association"),
            site_id: decoy_site.clone(),
            safe_description:
                "A dropped token links the report to another person, not necessarily the culprit."
                    .into(),
            corrects_proposition_id: None,
        },
        GeneratedEvidence {
            id: pattern_evidence_id.clone(),
            kind: EvidenceKind::LedgerEntry,
            proposition_id: pattern_prop,
            site_id: evidence_site.clone(),
            safe_description: format!("Corroborated accounts show: {pattern_truth}"),
            corrects_proposition_id: None,
        },
    ];
    let area_id = scoped_id(&prefix, "area", "incident");
    let hostile_id = scoped_id(&prefix, "hostile-group", "finale");
    let subject =
        SubjectId::new(scoped_id(&prefix, "subject", "missing-person")).expect("generated subject");
    let asset =
        AssetId::new(scoped_id(&prefix, "asset", "missing-property")).expect("generated asset");
    let mut actions = build_actions(
        &prefix,
        family,
        &finale_site,
        &area_id,
        &primary.npc_id,
        route_variant,
        attack_pattern,
        pattern_target.as_ref(),
        &pattern_evidence_id,
    );
    let issuer = context
        .witness_candidates
        .get(2)
        .unwrap_or(secondary)
        .npc_id
        .clone();
    let (objectives, finales, custody, dialogue_producers) = match family {
        TemplateFamily::RecurringDepredation => (
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
        ),
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
                                    custodian_id: issuer.clone(),
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
                        recipient_npc_id: issuer.clone(),
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
                        recipient_npc_id: issuer.clone(),
                        subject_ref: Some(description_prop.clone()),
                        asset_id: None,
                    }],
                )
            }
            CanonicalCause::VoluntaryDisappearance => unreachable!(
                "voluntary disappearance is excluded until locate/report producers exist"
            ),
        },
    };
    let mut bridges = Vec::new();
    for key in [
        site_bridge,
        circ_bridge,
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
            evidence.push(GeneratedEvidence {
                id: item.evidence_id.clone(),
                kind: EvidenceKind::DroppedToken,
                proposition_id: bridge_proposition_id,
                site_id: evidence_site.clone(),
                safe_description: item.lead_summary.clone(),
                corrects_proposition_id: None,
            });
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
            consequence(cause, family).symptom
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
        family,
        canonical_case_id,
        public_case_id,
        problem_id,
        cause,
        canonical_events,
        consequence: consequence(cause, family),
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

pub fn validate(case: &GeneratedCase) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if case.catalog_revision != CATALOG_REVISION {
        errors.push("catalog revision mismatch".into());
    }
    let true_sites: Vec<_> = case.sites.iter().filter(|s| s.is_true_location).collect();
    if true_sites.len() != 1 {
        errors.push("case must have exactly one canonical finale location".into());
    }
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
                    && entry.target_kind == "contact"
                    && entry.prerequisite.is_none()
                    && case
                        .witnesses
                        .iter()
                        .any(|witness| witness.npc_id == entry.target_id)
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
                            && case
                                .witnesses
                                .iter()
                                .any(|witness| witness.npc_id == action.target_id)
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
                    && case
                        .witnesses
                        .iter()
                        .any(|witness| witness.npc_id == action.target_id)
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
        if action.prerequisite.as_ref() == Some(&action.id) {
            errors.push(format!("{} dominates itself", action.id.0));
        }
        let target_exists = match action.target_kind.as_str() {
            "site" => case.sites.iter().any(|site| site.id.0 == action.target_id),
            "area" => case.areas.iter().any(|area| area.id == action.target_id),
            "contact" => case
                .witnesses
                .iter()
                .any(|witness| witness.npc_id == action.target_id),
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
    for witness in &case.witnesses {
        if witness.npc_id.is_empty()
            || witness.expected_location.is_empty()
            || witness.expected_location_label.is_empty()
            || witness.visible_description.is_empty()
        {
            errors.push(format!("{} lacks persistent referral data", witness.id.0));
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
            .filter(|action| &action.route == route)
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
        if producer.recipient_npc_id.is_empty() {
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
                    && producer.recipient_npc_id == *custodian_id
            }),
            ObjectiveRequirement::Expose { subject_ref } => {
                case.dialogue_producers.iter().any(|producer| {
                    producer.objective_id == objective.id
                        && producer.action == GeneratedDialogueAction::Expose
                        && producer.subject_ref.as_deref() == Some(subject_ref)
                })
            }
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

pub fn audit(seeds: u64) -> BTreeMap<TemplateFamily, u64> {
    let mut out = BTreeMap::new();
    for seed in 0..seeds {
        let context = GenerationContext {
            seed,
            observer_entropy_hi: seed ^ 0x6f62_7365_7276_6572,
            observer_entropy_lo: seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
            settlement_id: "audit".into(),
            settlement_name: "Audit".into(),
            scope: Scope::Settlement {
                settlement_id: "audit".into(),
            },
            ordinal: 0,
            now_minute: 1_000,
            requested_family: None,
            witness_candidates: test_witnesses(),
        };
        if let Ok(case) = generate(&context) {
            *out.entry(case.family).or_default() += 1;
        }
    }
    out
}

pub fn test_witnesses() -> Vec<WitnessCandidate> {
    vec![
        WitnessCandidate {
            npc_id: "npc:a".into(),
            demographic: WitnessDemographic::Child,
            age_band: "child".into(),
            sex: "female".into(),
            profession: "apprentice".into(),
            visible_description: "a short, fair-haired apprentice".into(),
            expected_location: "residences".into(),
            expected_location_label: "Residences".into(),
            presence_version: 11,
            allowed_circumstances: BTreeSet::from([
                Circumstance::NightWindow,
                Circumstance::AdultVenue,
            ]),
        },
        WitnessCandidate {
            npc_id: "npc:b".into(),
            demographic: WitnessDemographic::Guard,
            age_band: "adult".into(),
            sex: "male".into(),
            profession: "guard".into(),
            visible_description: "a tall guard with dark hair".into(),
            expected_location: "keep".into(),
            expected_location_label: "Keep".into(),
            presence_version: 12,
            allowed_circumstances: BTreeSet::from([
                Circumstance::RoadJourney,
                Circumstance::GraveDuty,
            ]),
        },
        WitnessCandidate {
            npc_id: "npc:c".into(),
            demographic: WitnessDemographic::Merchant,
            age_band: "elder".into(),
            sex: "female".into(),
            profession: "merchant".into(),
            visible_description: "a broad merchant with grey hair".into(),
            expected_location: "market".into(),
            expected_location_label: "General Market".into(),
            presence_version: 13,
            allowed_circumstances: BTreeSet::from([
                Circumstance::RoadJourney,
                Circumstance::SecretRiversideMeeting,
            ]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_report_description_has_ambiguous_natural_testimony() {
        assert_eq!(ALL_REPORTS.len(), 8);
        for report in ALL_REPORTS {
            let prose = ambiguous_report_description(*report);
            let claim = ambiguous_visual_claim(*report, "the old bridge");
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

    fn inn_only_settlement_witnesses() -> (
        Vec<crate::settlement_economy::SettlementNpcTab>,
        Vec<WitnessCandidate>,
    ) {
        let profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        let tabs = crate::settlement_economy::player_visible_npc_tabs(&profile, false);
        let mut candidates = test_witnesses();
        for (candidate, location) in candidates.iter_mut().zip(["residences", "overview", "inn"]) {
            candidate.expected_location = location.into();
            candidate.expected_location_label.clear();
        }
        let mut unavailable_armourer = candidates[2].clone();
        unavailable_armourer.npc_id = "npc:hidden-armourer".into();
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
            requested_family: Some(family),
            witness_candidates: test_witnesses(),
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
                    let tab = crate::settlement_economy::visible_npc_tab(
                        &tabs,
                        &witness.expected_location,
                    )
                    .unwrap_or_else(|| {
                        panic!(
                            "{family:?} seed {seed} referred {} to hidden location {}",
                            witness.npc_id, witness.expected_location
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
                witness.npc_id = format!("npc:riverdale:residences:{index}");
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
                    let (observation, recollection, claim) =
                        process_report(pipeline.clone()).unwrap();
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
                        invalid.receipt_identity = compound_id(&[
                            "generated-testimony",
                            &character_id.to_string(),
                            &witness.id.0,
                            &index.to_string(),
                        ]);
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
    fn exact_referred_witness_projects_clues_and_completes_contact_root_idempotently() {
        use crate::investigation::process_report;

        let mut source = context(11, TemplateFamily::DisappearanceOrLoss);
        for (index, witness) in source.witness_candidates.iter_mut().enumerate() {
            witness.npc_id = format!("npc:riverdale:{index}");
        }
        let generated = generate(&source).expect("generated disappearance case");
        let witness = generated.witnesses.first().expect("exact referred witness");
        let same_name_other_id = "npc:riverdale:same-name-collision";
        let witness_name = "Hans Wagner";
        let other_name = "Hans Wagner";
        assert_eq!(witness_name, other_name);
        assert!(exact_referral_contact(&witness.npc_id, &witness.npc_id));
        assert!(!exact_referral_contact(&witness.npc_id, same_name_other_id));

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
                target_kind: action.target_kind.clone(),
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
            &witness.npc_id,
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
                &witness.npc_id,
            )
            .expect("idempotent replay"),
            ReferredContactTransition::Replay
        );
        let root_after_replay = states.iter().find(|state| state.id == root_id).unwrap();
        assert_eq!(root_after_replay.version, 1);
    }

    #[test]
    fn failed_route_does_not_revive_a_completed_contact_alternate() {
        let mut states = vec![
            ReferredContactActionState {
                id: "search".into(),
                owner_character_id: 7,
                case_id: "case".into(),
                method: "search_area".into(),
                target_kind: "area".into(),
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
                target_kind: "contact".into(),
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
                    target_kind: action.target_kind.clone(),
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
                    &generated.witnesses[0].npc_id,
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
            let mut generated =
                generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
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
                2 => root.target_kind = "area".into(),
                _ => root.prerequisite = Some(ActionId::new("substituted")),
            }
            assert!(validate(&generated).is_err());
        }
        for mutate in 0..4 {
            let mut generated =
                generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
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
                2 => successor.target_kind = "contact".into(),
                _ => successor.prerequisite = Some(ActionId::new("substituted")),
            }
            assert!(validate(&generated).is_err());
        }
        for mutate in 0..4 {
            let mut generated =
                generate(&context(11, TemplateFamily::DisappearanceOrLoss)).unwrap();
            let physical = generated
                .actions
                .iter_mut()
                .find(|action| action.active_initially && action.route == RouteClass::PhysicalTrail)
                .unwrap();
            match mutate {
                0 => physical.kind = InvestigationActionKind::LocateContact,
                1 => physical.route = RouteClass::SocialInquiry,
                2 => physical.target_kind = "contact".into(),
                _ => physical.prerequisite = Some(ActionId::new("substituted")),
            }
            assert!(validate(&generated).is_err());
        }
    }

    #[test]
    fn deterministic_and_counterfactual() {
        let a = generate(&context(41, TemplateFamily::DisappearanceOrLoss)).unwrap();
        assert_eq!(
            a,
            generate(&context(41, TemplateFamily::DisappearanceOrLoss)).unwrap()
        );
        let b = generate(&context(42, TemplateFamily::DisappearanceOrLoss)).unwrap();
        assert_eq!(a.consequence.symptom, b.consequence.symptom);
        assert_ne!((a.cause, a.sites[0].kind), (b.cause, b.sites[0].kind));
    }
    #[test]
    fn descriptions_are_ambiguous() {
        for seed in 0..256 {
            let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
            assert!(
                crate::bestiary::ambiguous_description_cardinality(
                    generated.witnesses[0].description
                ) >= 2
            );
        }
    }
    #[test]
    fn hard_zero_and_rare_rules_are_auditable() {
        let mut trace = Vec::new();
        let _ = choose(
            1,
            "module.site",
            "relation.site.cause",
            &site_candidates(CanonicalCause::Hostile(ThreatId::Skeleton)),
            &mut trace,
        )
        .unwrap();
        let house = trace
            .iter()
            .find(|t| t.candidate_id == "site.occupied_house")
            .unwrap();
        assert_eq!(house.plausibility, 3);
        assert_eq!(
            house.required_bridge.as_ref().unwrap().0,
            "bridge.skeletons_occupied_house"
        );
        let wolf_crypt = site_candidates(CanonicalCause::Hostile(ThreatId::Wolf))
            .into_iter()
            .find(|c| c.id == "site.crypt")
            .unwrap();
        assert_eq!(wolf_crypt.weight.plausibility, 0);
        assert!(wolf_crypt.impossible.is_some());
    }
    #[test]
    fn child_adult_venue_is_rare_but_bridged() {
        let adult = circumstance_candidates(WitnessDemographic::Child)
            .into_iter()
            .find(|c| c.value == Circumstance::AdultVenue)
            .unwrap();
        assert_eq!(adult.weight.plausibility, 2);
        assert_eq!(adult.bridge, Some("bridge.child_at_adult_venue"));
    }
    #[test]
    fn graph_keeps_both_routes_reachable_from_authored_entries() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            let case = generate(&context(7, family)).unwrap();
            validate(&case).unwrap();
            let routes = case
                .actions
                .iter()
                .map(|a| a.route)
                .collect::<BTreeSet<_>>();
            assert_eq!(routes.len(), 2);
            if family == TemplateFamily::RecurringDepredation {
                let roots = case
                    .actions
                    .iter()
                    .filter(|action| action.active_initially)
                    .collect::<Vec<_>>();
                assert_eq!(roots.len(), 1);
                assert_eq!(roots[0].kind, InvestigationActionKind::LocateContact);
                let unlocked = case
                    .actions
                    .iter()
                    .filter(|action| action.prerequisite.as_ref() == Some(&roots[0].id))
                    .map(|action| action.route)
                    .collect::<BTreeSet<_>>();
                assert_eq!(unlocked, routes);
            } else {
                for route in routes {
                    assert!(
                        case.actions
                            .iter()
                            .any(|action| action.route != route && action.active_initially)
                    );
                }
            }
        }
    }
    #[test]
    fn disappearance_truth_selects_only_compatible_targets_and_producers() {
        for seed in 0..256 {
            let generated = generate(&context(seed, TemplateFamily::DisappearanceOrLoss)).unwrap();
            assert_ne!(generated.cause, CanonicalCause::VoluntaryDisappearance);
            validate(&generated).unwrap();
            match generated.cause {
                CanonicalCause::Hostile(_) | CanonicalCause::ConcealmentByWitness => {
                    assert_eq!(generated.finales.len(), 1);
                    assert_eq!(generated.finales[0].kind, FinaleKind::Rescue);
                    let subjects = generated
                        .actions
                        .iter()
                        .flat_map(|action| &action.outputs)
                        .filter_map(|output| match output {
                            GeneratedActionOutput::Consequence {
                                consequence:
                                    GeneratedActionConsequence::RescueSubject { subject_id, .. },
                            } => Some(subject_id),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(subjects.len(), 2);
                    assert_eq!(subjects[0], subjects[1]);
                    assert!(
                        generated
                            .actions
                            .iter()
                            .filter(|action| {
                                action.outputs.iter().any(|output| {
                                    matches!(output, GeneratedActionOutput::Consequence { .. })
                                })
                            })
                            .all(|action| {
                                action.kind == InvestigationActionKind::InspectSite
                                    && action.target_kind == "site"
                            })
                    );
                }
                CanonicalCause::IncidentalLoss => {
                    assert_eq!(generated.finales[0].kind, FinaleKind::RetrieveReturn);
                    assert!(generated.dialogue_producers.iter().any(|producer| {
                        producer.action == GeneratedDialogueAction::ReturnAsset
                    }));
                }
                CanonicalCause::FabricatedClaim => {
                    assert_eq!(generated.finales[0].kind, FinaleKind::Expose);
                    assert!(
                        generated
                            .dialogue_producers
                            .iter()
                            .any(|producer| { producer.action == GeneratedDialogueAction::Expose })
                    );
                }
                CanonicalCause::VoluntaryDisappearance => unreachable!(),
            }
        }
    }
    #[test]
    fn correction_reuses_the_proposition_it_corrects() {
        let generated = generate(&context(19, TemplateFamily::DisappearanceOrLoss)).unwrap();
        let initial = &generated.witnesses[0].testimony[0];
        let correction = &generated.witnesses[1].testimony[0];
        assert_eq!(initial.proposition_id, correction.proposition_id);
        assert_eq!(
            correction.corrects_proposition_id.as_deref(),
            Some(initial.proposition_id.as_str())
        );
    }
    #[test]
    fn marginal_sweep_is_bounded_and_has_both_templates() {
        let result = audit(256);
        assert!(result[&TemplateFamily::RecurringDepredation] > 80);
        assert!(result[&TemplateFamily::DisappearanceOrLoss] > 80);
    }
    #[test]
    fn public_identity_is_opaque_and_generated_cases_have_no_contract() {
        let case = generate(&context(88, TemplateFamily::RecurringDepredation)).unwrap();
        assert_ne!(case.canonical_case_id, case.public_case_id);
        let public = serde_json::json!({
            "case_id": case.public_case_id,
            "problem_id": case.problem_id,
            "actions": case.actions,
            "witnesses": case.witnesses,
            "evidence": case.evidence,
            "areas": case.areas,
        });
        let json = serde_json::to_string(&public).unwrap();
        assert!(!json.contains(&case.canonical_case_id));
        assert!(!json.contains("\"contract\""));
        assert!(!json.contains("scope:"));
    }

    #[test]
    fn public_ids_use_private_entropy_and_do_not_collide_across_cases() {
        let mut first = context(91, TemplateFamily::RecurringDepredation);
        first.observer_entropy_hi = 0x4341_4e4f_4e49_4341;
        first.observer_entropy_lo = 0x4c2d_5345_4e54_494e;
        let mut second = first.clone();
        second.observer_entropy_hi ^= 1;
        second.ordinal = 1;
        let a = generate(&first).unwrap();
        let b = generate(&second).unwrap();
        let ids = |case: &GeneratedCase| {
            case.actions
                .iter()
                .map(|action| action.id.0.clone())
                .chain(case.witnesses.iter().map(|witness| witness.id.0.clone()))
                .chain(
                    case.evidence
                        .iter()
                        .map(|evidence| evidence.proposition_id.clone()),
                )
                .chain(std::iter::once(case.public_case_id.clone()))
                .collect::<BTreeSet<_>>()
        };
        let a_ids = ids(&a);
        let b_ids = ids(&b);
        assert!(a_ids.is_disjoint(&b_ids));
        let serialized = serde_json::to_string(&(a_ids, b_ids)).unwrap();
        for sentinel in [
            &a.canonical_case_id,
            &b.canonical_case_id,
            "CANONICAL-SENTINEL",
            "scope:",
        ] {
            assert!(!serialized.contains(sentinel));
        }
    }

    #[test]
    fn every_route_reveals_then_requires_occupied_site_resolution() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            let case = generate(&context(7, family)).unwrap();
            for route in case
                .actions
                .iter()
                .map(|action| action.route)
                .collect::<BTreeSet<_>>()
            {
                let mut route_actions = case.actions.iter().filter(|action| action.route == route);
                let exact = route_actions
                    .clone()
                    .find(|action| {
                        action.outputs.iter().any(|output| {
                            matches!(
                                output,
                                GeneratedActionOutput::Destination {
                                    stage: GeneratedDestinationStage::Exact,
                                    site_id: Some(_),
                                }
                            )
                        })
                    })
                    .expect("travel-capable exact-location reveal");
                assert!(matches!(
                    exact.kind,
                    InvestigationActionKind::FollowTracks
                        | InvestigationActionKind::ReacquireTracks
                        | InvestigationActionKind::ApproachLead
                        | InvestigationActionKind::Patrol
                        | InvestigationActionKind::SearchArea
                ));
                let occupied = route_actions
                    .find(|action| {
                        action.target_kind == "site"
                            && action.prerequisite.as_ref() == Some(&exact.id)
                    })
                    .expect("separate occupied-site resolution");
                assert!(matches!(
                    occupied.kind,
                    InvestigationActionKind::InspectSite | InvestigationActionKind::LayAmbush
                ));
                assert!(!exact.outputs.iter().any(|output| matches!(
                    output,
                    GeneratedActionOutput::Consequence { .. } | GeneratedActionOutput::AmbushReady
                )));
            }
        }
    }

    #[test]
    fn typed_catalogs_condition_and_enforce_hard_zeros() {
        let child = reliability_candidates(
            WitnessDemographic::Child,
            Circumstance::NightWindow,
            CanonicalCause::Hostile(ThreatId::Goblin),
        );
        assert!(
            child
                .iter()
                .find(|c| c.value == Reliability::Deceptive)
                .unwrap()
                .impossible
                .is_some()
        );
        let fabricated =
            evidence_candidates(CanonicalCause::FabricatedClaim, SiteKind::OccupiedHouse);
        assert!(
            fabricated
                .iter()
                .find(|c| c.value == EvidenceKind::BloodlessCorpse)
                .unwrap()
                .impossible
                .is_some()
        );
        let mistaken = account_style_candidates(Reliability::Mistaken, Circumstance::RoadJourney);
        assert!(
            mistaken
                .iter()
                .find(|c| c.value == AccountStyle::TracksAndMovement)
                .unwrap()
                .impossible
                .is_some()
        );
        assert_ne!(
            reliability_candidates(
                WitnessDemographic::Guard,
                Circumstance::AdultVenue,
                CanonicalCause::Hostile(ThreatId::Bandit),
            )
            .iter()
            .find(|c| c.value == Reliability::Evasive)
            .unwrap()
            .weight,
            reliability_candidates(
                WitnessDemographic::Guard,
                Circumstance::RoadJourney,
                CanonicalCause::Hostile(ThreatId::Bandit),
            )
            .iter()
            .find(|c| c.value == Reliability::Evasive)
            .unwrap()
            .weight
        );
    }

    #[test]
    fn modular_marginals_vary_without_cause_site_fingerprints() {
        let mut reliabilities = BTreeSet::new();
        let mut secondary_sites = BTreeSet::new();
        let mut secondary_circumstances = BTreeSet::new();
        let mut evidence_kinds = BTreeSet::new();
        let mut account_wordings = BTreeSet::new();
        let mut route_kinds = BTreeSet::new();
        let mut patterns = BTreeSet::new();
        let mut fingerprints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for seed in 0..512 {
            let case = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
            reliabilities.insert(case.witnesses[0].testimony[0].reliability);
            secondary_sites.insert(case.sites[2].kind);
            secondary_circumstances.insert(case.witnesses[1].circumstance);
            evidence_kinds.insert(case.evidence[0].kind);
            account_wordings.insert(
                case.witnesses[0].testimony[0]
                    .spoken_text
                    .split_whitespace()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            route_kinds.extend(
                case.actions
                    .iter()
                    .map(|action| format!("{:?}", action.kind)),
            );
            let pattern = case.canonical_events[0]
                .object
                .split(':')
                .next()
                .unwrap()
                .to_owned();
            patterns.insert(pattern.clone());
            fingerprints
                .entry(format!("{:?}:{:?}", case.cause, case.sites[0].kind))
                .or_default()
                .insert(format!(
                    "{:?}:{:?}:{:?}:{pattern}",
                    case.witnesses[0].testimony[0].reliability,
                    case.sites[2].kind,
                    case.evidence[0].kind
                ));
        }
        for (name, cardinality) in [
            ("reliability", reliabilities.len()),
            ("secondary site", secondary_sites.len()),
            ("secondary circumstance", secondary_circumstances.len()),
            ("evidence", evidence_kinds.len()),
            ("account wording", account_wordings.len()),
            ("route behavior", route_kinds.len()),
            ("attack pattern", patterns.len()),
        ] {
            assert!(cardinality >= 2, "{name} collapsed to a fingerprint");
        }
        assert!(
            fingerprints.values().any(|values| values.len() >= 2),
            "cause/site pairs must not determine all downstream modules"
        );
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
                assert!(consumer.safe_summary.contains(summary_fragment));
                let selected_condition = consumer
                    .outputs
                    .iter()
                    .find_map(|output| match output {
                        GeneratedActionOutput::PatternCondition { condition, .. } => {
                            Some(condition)
                        }
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
                let learned_projection =
                    serde_json::to_string(&(pattern_evidence, consumer)).unwrap();
                assert!(learned_projection.contains(summary_fragment));
                for target in &case.pattern_targets {
                    assert!(!learned_projection.contains(&target.npc_id));
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
                .find(|candidate| candidate.npc_id == target.npc_id)
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
}
