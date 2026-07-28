//! Typed, deterministic generation for investigation-led quests.
//!
//! The catalog is deliberately static Rust data rather than an interpreted
//! rules language.  Relations are defined once, in the direction in which the
//! solver consumes them.  Diagnostic traces contain canonical truth and must
//! remain private to strategic authority and developer tools.

use crate::{
    bestiary::{ReportDescription, ThreatId, description_likelihood},
    case::{
        AssetId, Objective, ObjectiveExpression, ObjectiveId, ObjectivePath, ObjectiveRequirement,
        SubjectId,
    },
    investigation_action::{InvestigationActionKind, Terrain},
    local_problem::{Effects, EncounterArchetype, Scope, Symptom},
};
use adventuresim_world_schema::BestiaryCategory;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Content-addressed revision of the sorted startup-compiled YAML catalog.
pub const CATALOG_REVISION: &str = crate::quest_catalog::QUEST_CATALOG_DIGEST;
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
id_type!(TrackTrailId);
id_type!(TrackSegmentId);

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

macro_rules! open_catalog_id {
    ($name:ident { $($constant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name { len: u8, bytes: [u8; 63] }
        impl $name {
            $(#[allow(non_upper_case_globals)]
            pub const $constant: Self = Self::from_static($value);)+
            pub const fn from_static(value: &str) -> Self {
                let source = value.as_bytes();
                assert!(!source.is_empty() && source.len() <= 63);
                let mut bytes = [0; 63];
                let mut index = 0;
                while index < source.len() {
                    bytes[index] = source[index];
                    index += 1;
                }
                Self { len: source.len() as u8, bytes }
            }
            pub fn try_new(value: &str) -> Result<Self, &'static str> {
                if value.is_empty() || value.len() > 63 || !value.bytes().all(|byte|
                    byte.is_ascii_lowercase() || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-' | b'.' | b':'))
                {
                    return Err("invalid open catalog ID");
                }
                let mut bytes = [0; 63];
                bytes[..value.len()].copy_from_slice(value.as_bytes());
                Ok(Self { len: value.len() as u8, bytes })
            }
            pub fn as_str(&self) -> &str {
                core::str::from_utf8(&self.bytes[..usize::from(self.len)])
                    .expect("catalog IDs are validated ASCII")
            }
        }
        impl core::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::try_new(&value).map_err(serde::de::Error::custom)
            }
        }
    };
}

open_catalog_id!(SiteKind {
    Cave => "cave", Crypt => "crypt", ForestCamp => "forest_camp",
    OccupiedHouse => "occupied_house", Riverside => "riverside",
    Graveyard => "graveyard", Roadside => "roadside", AbandonedFarm => "abandoned_farm"
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRole {
    Finale,
    Evidence,
    Decoy,
    LastKnown,
}

open_catalog_id!(WitnessDemographic {
    Child => "child", Laborer => "laborer", Merchant => "merchant",
    Cleric => "cleric", Guard => "guard", Noble => "noble"
});

open_catalog_id!(Circumstance {
    NightWindow => "night_window", SecretRiversideMeeting => "secret_riverside",
    AdultVenue => "adult_venue", RoadJourney => "road",
    GraveDuty => "grave_duty", LivestockWatch => "livestock_watch"
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reliability {
    Truthful,
    Mistaken,
    Evasive,
    Deceptive,
    PartlyTruthful,
}

open_catalog_id!(EvidenceKind {
    Footprints => "footprints", ClothScrap => "cloth_scrap", BoneDust => "bone_dust",
    BloodlessCorpse => "bloodless_corpse", DroppedToken => "dropped_token",
    DragMarks => "drag_marks", LedgerEntry => "ledger_entry"
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCheckStat {
    Eyesight,
    Intelligence,
    Instinct,
}

impl EvidenceCheckStat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Eyesight => "Eyesight",
            Self::Intelligence => "Intelligence",
            Self::Instinct => "Instinct",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceInspectionCheck {
    pub stat: EvidenceCheckStat,
    /// Fixed-point attribute threshold, where 1,000 is an attribute value of 1.0.
    pub difficulty_milli: u16,
    pub success_description: String,
    pub reveals_clue: bool,
}

pub fn evidence_check_passes(value_milli: u16, difficulty_milli: u16) -> bool {
    value_milli >= difficulty_milli
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceInspectionTopic {
    pub id: String,
    pub label: String,
    pub inspection_description: String,
    pub check: Option<EvidenceInspectionCheck>,
    pub bestiary: Vec<BestiaryEvidenceImplication>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BestiaryEvidenceImplication {
    pub category: BestiaryCategory,
    /// Hidden fixed-point Bestiary threshold, where 1,000 is a check of 1.0.
    pub lore_difficulty_milli: u16,
    pub diagnostic_kind: Option<String>,
    pub interpretation: String,
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
    pub display_name: String,
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
pub struct TestimonyChallengeResponses {
    pub charm: Option<String>,
    pub command: Option<String>,
    pub bluff: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestimonyDraft {
    pub proposition_id: String,
    pub reliability: Reliability,
    pub delivery: TestimonyDelivery,
    pub truthful_text: String,
    pub spoken_text: String,
    /// Exact server-authored substring that may be questioned in dialogue.
    /// It must occur once within `spoken_text`; punctuation and surrounding
    /// narration deliberately remain outside the interactive claim.
    pub challenge_text: String,
    /// Required claim-specific lines authored alongside the testimony.
    /// The client has no generic fallback.
    pub challenge_responses: TestimonyChallengeResponses,
    pub destination_stage: String,
    pub site_id: Option<SiteId>,
    /// Proposition superseded by this claim. Set only on the later correction.
    pub corrects_proposition_id: Option<String>,
    /// Exact authored witnesses this account explicitly refers the observer to.
    pub referred_witness_ids: Vec<WitnessId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestimonyDelivery {
    Volunteered,
    Withheld,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessBinding {
    pub id: WitnessId,
    pub npc_id: String,
    pub display_name: String,
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
    pub portrait_label: String,
    pub portrait_icon: String,
    pub base_description: String,
    pub inspection_topics: Vec<EvidenceInspectionTopic>,
    pub safe_description: String,
    pub corrects_proposition_id: Option<String>,
}

/// Immutable private trail authority. Segment identities are observer-scoped;
/// public projections disclose only a successfully completed segment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackTrail {
    pub id: TrackTrailId,
    pub segment_ids: Vec<TrackSegmentId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackSegment {
    pub id: TrackSegmentId,
    pub trail_id: TrackTrailId,
    pub ordinal: u16,
    pub terrain: Terrain,
    pub safe_finding: String,
    pub predecessor: Option<TrackSegmentId>,
    pub next: Option<TrackSegmentId>,
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
    pub track_segment_id: Option<TrackSegmentId>,
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

/// The complete authored testimony visible on first contact, in presentation
/// order. Withheld details remain private manifest authority and cannot change
/// the initial dialogue's text, cardinality, ordering, or source.
pub fn initial_testimony_projection(witness: &WitnessBinding) -> Vec<(usize, &TestimonyDraft)> {
    witness
        .testimony
        .iter()
        .enumerate()
        .filter(|(_, draft)| draft.delivery == TestimonyDelivery::Volunteered)
        .collect()
}

/// Private authority used to assess and challenge an authored claim.
///
/// Presentation text may add framing or paraphrase the proposition, so display
/// string equality is never authority. Accuracy and demeanor are deliberately
/// separate: a mistaken witness can assert an inaccurate claim sincerely,
/// while evasive or partly truthful testimony provides no clean demeanor signal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TestimonyClaimAuthority {
    pub factually_accurate: bool,
    /// Signed private signal used by passive Insight: `-1` is deliberate
    /// deception, `1` is sincere conviction, and `0` is genuinely ambiguous.
    pub demeanor_truth_signal: f32,
}

pub const fn testimony_claim_authority(draft: &TestimonyDraft) -> TestimonyClaimAuthority {
    match draft.reliability {
        Reliability::Truthful => TestimonyClaimAuthority {
            factually_accurate: true,
            demeanor_truth_signal: 1.0,
        },
        Reliability::Mistaken => TestimonyClaimAuthority {
            factually_accurate: false,
            demeanor_truth_signal: 1.0,
        },
        Reliability::Evasive | Reliability::PartlyTruthful => TestimonyClaimAuthority {
            factually_accurate: false,
            demeanor_truth_signal: 0.0,
        },
        Reliability::Deceptive => TestimonyClaimAuthority {
            factually_accurate: false,
            demeanor_truth_signal: -1.0,
        },
    }
}

/// Testimony a player-like actor can legitimately hear by starting with the
/// public primary contact and following referrals disclosed by volunteered
/// statements. Withheld statements and unreferenced secondary witnesses never
/// enter the projection.
pub fn player_visible_testimony_sequence(
    generated: &GeneratedCase,
) -> Vec<(&WitnessBinding, &TestimonyDraft)> {
    let Some(primary) = generated.witnesses.first() else {
        return Vec::new();
    };
    let mut visible_witnesses = BTreeSet::from([primary.id.clone()]);
    let mut delivered_witnesses = BTreeSet::new();
    let mut output = Vec::new();
    loop {
        let Some(witness) = generated.witnesses.iter().find(|witness| {
            visible_witnesses.contains(&witness.id) && !delivered_witnesses.contains(&witness.id)
        }) else {
            break;
        };
        delivered_witnesses.insert(witness.id.clone());
        for (_, statement) in initial_testimony_projection(witness) {
            for referred in &statement.referred_witness_ids {
                if generated
                    .witnesses
                    .iter()
                    .any(|candidate| candidate.id == *referred)
                {
                    visible_witnesses.insert(referred.clone());
                }
            }
            output.push((witness, statement));
        }
    }
    output
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
    TrackFinding {
        segment_id: TrackSegmentId,
        finding: String,
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
    pub template_id: String,
    pub configured_routes: Vec<String>,
    pub configured_objectives: Vec<String>,
    pub incident_interval_minutes: u64,
    pub maximum_incidents: u16,
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
    pub track_trails: Vec<TrackTrail>,
    pub track_segments: Vec<TrackSegment>,
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
                    difficulty_milli: check.difficulty_min_milli
                        + (crate::settlement_population::stable_hash(&format!(
                            "{}:{}",
                            evidence_id.0, topic.id
                        )) % width) as u16,
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
                        lore_difficulty_milli: implication.lore_difficulty_milli,
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
) -> GeneratedEvidence {
    let (portrait_label, portrait_icon, base_description, inspection_topics) =
        evidence_presentation(kind, &id);
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
                context.witness_candidates[*index].npc_id
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
            npc_id: npc1,
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
            npc_id: npc2,
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
        ),
        generated_evidence(
            EvidenceId::new(scoped_id(&prefix, "evidence", "token")),
            EvidenceKind::DroppedToken,
            scoped_id(&prefix, "proposition", "association"),
            decoy_site.clone(),
            "A dropped token links the report to another person, not necessarily the culprit."
                .into(),
            None,
        ),
        generated_evidence(
            pattern_evidence_id.clone(),
            EvidenceKind::LedgerEntry,
            pattern_prop,
            evidence_site.clone(),
            format!("Corroborated accounts show: {pattern_truth}"),
            None,
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
        &primary.npc_id,
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
    let template_id = match family {
        TemplateFamily::RecurringDepredation => "recurring_depredation",
        TemplateFamily::DisappearanceOrLoss => "disappearance_or_loss",
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
    if case.track_trails.is_empty() {
        errors.push("generated case requires an immutable physical trail".into());
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
        if action
            .prerequisite
            .as_ref()
            .is_some_and(|required| !action_ids.contains(required))
        {
            errors.push(format!("{} has a missing prerequisite", action.id.0));
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
    let witness_positions = case
        .witnesses
        .iter()
        .enumerate()
        .map(|(index, witness)| (witness.id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut referral_edges = BTreeMap::<WitnessId, BTreeSet<WitnessId>>::new();
    let mut authored_challenge_responses = BTreeSet::<String>::new();
    for (source_index, witness) in case.witnesses.iter().enumerate() {
        if witness.npc_id.is_empty()
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
                    action.target_kind == "contact" && action.target_id == witness.npc_id
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
            display_name: "Anna Weber".into(),
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
            display_name: "Berthold Fischer".into(),
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
            display_name: "Clara Hoffmann".into(),
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
        let reports = crate::quest_catalog::catalog()
            .documents
            .iter()
            .flat_map(|document| &document.descriptions)
            .map(|description| ReportDescription::try_new(&description.id).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reports.len(), 8);
        for report in reports {
            let prose = ambiguous_report_description(report);
            let claim = ambiguous_visual_claim(report, "the old bridge");
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

    #[test]
    fn account_presentation_candidates_do_not_encode_reliability() {
        for circumstance in [
            Circumstance::SecretRiversideMeeting,
            Circumstance::NightWindow,
            Circumstance::RoadJourney,
        ] {
            let baseline = account_style_candidates(Reliability::Truthful, circumstance)
                .into_iter()
                .map(|candidate| {
                    (
                        candidate.id,
                        candidate.value,
                        candidate.weight,
                        candidate.impossible,
                    )
                })
                .collect::<Vec<_>>();
            for reliability in [
                Reliability::PartlyTruthful,
                Reliability::Mistaken,
                Reliability::Evasive,
                Reliability::Deceptive,
            ] {
                let other = account_style_candidates(reliability, circumstance)
                    .into_iter()
                    .map(|candidate| {
                        (
                            candidate.id,
                            candidate.value,
                            candidate.weight,
                            candidate.impossible,
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(baseline, other);
            }
        }
    }

    #[test]
    fn generated_location_testimony_has_one_public_grant_shape() {
        for seed in 0..256 {
            let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
            let draft = &generated.witnesses[0].testimony[0];
            assert_eq!(draft.destination_stage, "route_segment");
            assert_eq!(draft.referred_witness_ids.len(), 1);
            assert!(!draft.spoken_text.is_empty());
            assert!(!draft.spoken_text.to_ascii_lowercase().contains("truthful"));
            assert!(!draft.spoken_text.to_ascii_lowercase().contains("lying"));
        }
    }

    #[test]
    fn claim_authority_separates_accuracy_from_demeanor_and_ignores_presentation_wording() {
        let generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        let mut draft = generated.witnesses[0].testimony[2].clone();
        assert_ne!(draft.spoken_text, draft.truthful_text);
        assert_eq!(
            testimony_claim_authority(&draft),
            TestimonyClaimAuthority {
                factually_accurate: true,
                demeanor_truth_signal: 1.0,
            }
        );

        for delivery in [TestimonyDelivery::Volunteered, TestimonyDelivery::Withheld] {
            draft.delivery = delivery;
            for (reliability, expected) in [
                (Reliability::Truthful, (true, 1.0)),
                (Reliability::Mistaken, (false, 1.0)),
                (Reliability::Evasive, (false, 0.0)),
                (Reliability::Deceptive, (false, -1.0)),
                (Reliability::PartlyTruthful, (false, 0.0)),
            ] {
                draft.reliability = reliability;
                let authority = testimony_claim_authority(&draft);
                assert_eq!(
                    (
                        authority.factually_accurate,
                        authority.demeanor_truth_signal
                    ),
                    expected
                );
            }
        }
    }

    #[test]
    fn private_concern_never_changes_complete_initial_dialogue_shape() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            for seed in 0..256 {
                let mut visible_outputs = BTreeSet::new();
                let mut concern_states = BTreeSet::new();
                for entropy in 0..64 {
                    let mut source = context(seed, family);
                    source.observer_entropy_hi = entropy;
                    source.observer_entropy_lo = entropy.rotate_left(29);
                    let generated = generate(&source).unwrap();
                    validate(&generated).unwrap();
                    let primary = &generated.witnesses[0];
                    let visible = initial_testimony_projection(primary)
                        .into_iter()
                        .map(|(index, draft)| {
                            (
                                primary.npc_id.clone(),
                                primary.display_name.clone(),
                                index,
                                draft.spoken_text.clone(),
                            )
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(visible.len(), 3);
                    assert_eq!(visible[1].2, 1);
                    assert_eq!(
                        primary.testimony[1].delivery,
                        TestimonyDelivery::Volunteered
                    );
                    visible_outputs.insert(visible);
                    concern_states.insert(
                        primary
                            .testimony
                            .iter()
                            .any(|draft| draft.delivery == TestimonyDelivery::Withheld),
                    );
                }
                assert_eq!(
                    visible_outputs.len(),
                    1,
                    "observer-private concern entropy changed the full initial output"
                );
                assert_eq!(concern_states, BTreeSet::from([false, true]));
            }
        }
    }

    #[test]
    fn private_concern_is_reliability_independent_and_pipeline_solvable() {
        use crate::investigation::process_report;

        let mut concern_reliability_states = BTreeSet::new();
        let mut checked_private_route_guard = false;
        for seed in 0..4_096 {
            let source = context(seed, TemplateFamily::RecurringDepredation);
            let generated = generate(&source).unwrap();
            let primary = &generated.witnesses[0];
            let truthful = primary.testimony[0].reliability == Reliability::Truthful;
            let concern = primary
                .testimony
                .iter()
                .position(|draft| draft.delivery == TestimonyDelivery::Withheld);
            concern_reliability_states.insert((truthful, concern.is_some()));
            if let Some(index) = concern {
                let (_, pipeline) = generated_testimony_pipeline(
                    &source,
                    17,
                    &generated,
                    primary,
                    index,
                    source.now_minute,
                )
                .unwrap();
                let (_, _, claim) = process_report(pipeline).unwrap();
                assert!(claim.is_some());
                if !checked_private_route_guard {
                    let mut invalid = generated.clone();
                    invalid.witnesses[0].testimony[index].site_id =
                        Some(invalid.sites[0].id.clone());
                    assert!(validate(&invalid).unwrap_err().iter().any(|error| {
                        error.contains("hides route authority behind a private concern")
                    }));
                    checked_private_route_guard = true;
                }
            }
        }
        assert!(checked_private_route_guard);
        assert_eq!(
            concern_reliability_states,
            BTreeSet::from([(false, false), (false, true), (true, false), (true, true)])
        );
    }

    #[test]
    fn generated_physical_trails_are_opaque_contiguous_two_segment_chains() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            let generated = generate(&context(73, family)).unwrap();
            let [trail] = generated.track_trails.as_slice() else {
                panic!("generated case must have one physical trail");
            };
            assert_eq!(trail.segment_ids.len(), 2);
            let first = generated
                .track_segments
                .iter()
                .find(|segment| segment.id == trail.segment_ids[0])
                .unwrap();
            let final_segment = generated
                .track_segments
                .iter()
                .find(|segment| segment.id == trail.segment_ids[1])
                .unwrap();
            assert_eq!(first.ordinal, 0);
            assert_eq!(first.predecessor, None);
            assert_eq!(first.next.as_ref(), Some(&final_segment.id));
            assert_eq!(final_segment.ordinal, 1);
            assert_eq!(final_segment.predecessor.as_ref(), Some(&first.id));
            assert_eq!(final_segment.next, None);

            let first_action = generated
                .actions
                .iter()
                .find(|action| action.track_segment_id.as_ref() == Some(&first.id))
                .unwrap();
            let final_action = generated
                .actions
                .iter()
                .find(|action| action.track_segment_id.as_ref() == Some(&final_segment.id))
                .unwrap();
            assert_eq!(final_action.prerequisite.as_ref(), Some(&first_action.id));
            assert!(first_action.outputs.iter().any(|output| matches!(
                output,
                GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::RouteSegment,
                    site_id: None,
                }
            )));
            assert!(!first_action.outputs.iter().any(|output| matches!(
                output,
                GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    ..
                }
            )));
            assert!(final_action.outputs.iter().any(|output| matches!(
                output,
                GeneratedActionOutput::Destination {
                    stage: GeneratedDestinationStage::Exact,
                    site_id: Some(_),
                }
            )));
        }
    }

    #[test]
    fn track_validator_rejects_broken_links_skips_and_early_exact_locations() {
        let generated = generate(&context(91, TemplateFamily::RecurringDepredation)).unwrap();

        let mut broken_link = generated.clone();
        broken_link.track_segments[0].next = None;
        assert!(validate(&broken_link).is_err());

        let mut skipped = generated.clone();
        let final_id = skipped.track_segments[1].id.clone();
        let final_action = skipped
            .actions
            .iter_mut()
            .find(|action| action.track_segment_id.as_ref() == Some(&final_id))
            .unwrap();
        final_action.prerequisite = None;
        assert!(validate(&skipped).is_err());

        let mut leaked = generated.clone();
        let first_id = leaked.track_segments[0].id.clone();
        let true_site = leaked
            .sites
            .iter()
            .find(|site| site.is_true_location)
            .unwrap()
            .id
            .clone();
        leaked
            .actions
            .iter_mut()
            .find(|action| action.track_segment_id.as_ref() == Some(&first_id))
            .unwrap()
            .outputs
            .push(GeneratedActionOutput::Destination {
                stage: GeneratedDestinationStage::Exact,
                site_id: Some(true_site),
            });
        assert!(validate(&leaked).is_err());
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

    fn case_with_primary_location_accuracy(
        family: TemplateFamily,
        truthful: bool,
    ) -> GeneratedCase {
        (0..4_096)
            .find_map(|seed| {
                let generated = generate(&context(seed, family)).ok()?;
                let is_truthful = generated.witnesses[0].testimony[0].reliability
                    == Reliability::Truthful;
                (is_truthful == truthful).then_some(generated)
            })
            .unwrap_or_else(|| {
                panic!(
                    "bounded generation sweep did not find a {} primary location account for {family:?}",
                    if truthful { "truthful" } else { "unreliable" }
                )
            })
    }

    #[test]
    fn witness_described_places_are_neutral_and_attributed() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            let generated = generate(&context(17, family)).unwrap();
            let primary = &generated.witnesses[0];
            let described_place = generated
                .sites
                .iter()
                .find(|site| site.role == SiteRole::Decoy)
                .unwrap();

            assert_eq!(
                described_place.safe_label,
                format!("Place {} described", primary.display_name)
            );
            let lower = described_place.safe_label.to_ascii_lowercase();
            assert!(!lower.contains("plausible"));
            assert!(!lower.contains("confirmed"));
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
    fn secondary_witnesses_require_explicit_acyclic_referral_edges() {
        let generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        let secondary_id = generated.witnesses[1].id.clone();
        assert!(
            generated.witnesses[0]
                .testimony
                .iter()
                .any(|draft| draft.referred_witness_ids.contains(&secondary_id))
        );

        let mut unreachable = generated.clone();
        for draft in &mut unreachable.witnesses[0].testimony {
            draft.referred_witness_ids.clear();
        }
        assert!(
            validate(&unreachable)
                .unwrap_err()
                .iter()
                .any(|error| { error.contains("not reachable from the primary witness") })
        );

        let mut cyclic = generated;
        let primary_id = cyclic.witnesses[0].id.clone();
        cyclic.witnesses[1].testimony[0]
            .referred_witness_ids
            .push(primary_id);
        assert!(
            validate(&cyclic)
                .unwrap_err()
                .iter()
                .any(|error| { error.contains("cyclic or backward witness referral") })
        );
    }

    #[test]
    fn challenge_boundaries_and_optional_authored_responses_validate() {
        let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        let first = &generated.witnesses[0].testimony[0];
        assert_eq!(
            first
                .spoken_text
                .match_indices(&first.challenge_text)
                .count(),
            1
        );

        generated.witnesses[0].testimony[0].challenge_responses = TestimonyChallengeResponses {
            charm: None,
            command: None,
            bluff: None,
        };
        validate(&generated).unwrap();

        let duplicate = generated.witnesses[0].testimony[1]
            .challenge_responses
            .charm
            .clone()
            .expect("generated response");
        generated.witnesses[0].testimony[2]
            .challenge_responses
            .bluff = Some(duplicate);
        assert!(
            validate(&generated)
                .unwrap_err()
                .iter()
                .any(|error| error.contains("reuses authored challenge response text"))
        );
    }

    #[test]
    fn challenge_validation_rejects_padded_challenge_text() {
        let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        generated.witnesses[0].testimony[0].challenge_text =
            format!(" {} ", generated.witnesses[0].testimony[0].challenge_text);
        assert!(
            validate(&generated)
                .unwrap_err()
                .iter()
                .any(|error| error.contains("challenge text must be nonempty and already trimmed"))
        );
    }

    #[test]
    fn challenge_validation_rejects_response_containing_normalized_claim_text() {
        let mut generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        let normalized_claim = generated.witnesses[0].testimony[0]
            .challenge_text
            .to_uppercase();
        generated.witnesses[0].testimony[0]
            .challenge_responses
            .charm = Some(format!("Press further about {normalized_claim}"));
        assert!(
            validate(&generated)
                .unwrap_err()
                .iter()
                .any(|error| error.contains("challenge response repeats its claim text"))
        );
    }

    #[test]
    fn generated_claim_boundaries_exclude_narration_and_punctuation() {
        let generated = generate(&context(7, TemplateFamily::RecurringDepredation)).unwrap();
        let primary = &generated.witnesses[0].testimony;
        assert_eq!(
            primary[1].challenge_text,
            "I cannot tell which details matter"
        );
        assert!(primary[2].spoken_text.starts_with("I noticed "));
        assert!(!primary[2].challenge_text.starts_with("I noticed "));
        assert!(
            primary[2]
                .spoken_text
                .ends_with(". You may examine it yourself.")
        );
        assert!(!primary[2].challenge_text.ends_with('.'));

        let visual = (0..100)
            .find_map(|seed| {
                let generated =
                    generate(&context(seed, TemplateFamily::RecurringDepredation)).ok()?;
                let draft = generated.witnesses[0].testimony[0].clone();
                draft
                    .spoken_text
                    .starts_with("It looked like ")
                    .then_some(draft)
            })
            .expect("golden range includes a visual claim");
        assert!(!visual.challenge_text.starts_with("It looked like "));
        assert!(!visual.challenge_text.ends_with('.'));
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
    fn secondary_testimony_without_a_contact_root_mutates_no_route() {
        let mut states = vec![ReferredContactActionState {
            id: "primary-contact".into(),
            owner_character_id: 7,
            case_id: "case".into(),
            method: "locate_contact".into(),
            target_kind: "contact".into(),
            target_id: "primary-witness".into(),
            required_action_id: String::new(),
            active: true,
            version: 0,
            successful_attempt: false,
        }];
        let before = states.clone();
        assert_eq!(
            transition_referred_contact_action(&mut states, 7, "case", "secondary-witness")
                .unwrap(),
            ReferredContactTransition::NotApplicable
        );
        assert_eq!(states, before);

        states.push(ReferredContactActionState {
            id: "duplicate-primary-contact".into(),
            ..states[0].clone()
        });
        assert_eq!(
            transition_referred_contact_action(&mut states, 7, "case", "primary-witness")
                .unwrap_err(),
            "Referred witness matches multiple contact actions"
        );
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
                        (1_200..=4_000).contains(&check.difficulty_milli) && check.reveals_clue
                    })
                })
        }));
    }

    #[test]
    fn pawprint_bestiary_implications_are_atomic_and_do_not_reveal_ancestry() {
        let (_, _, _, topics) = evidence_presentation(
            EvidenceKind::Footprints,
            &EvidenceId("test-pawprint".into()),
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
            assert!(
                validate(&generated).is_err(),
                "recurring root mutation {mutate} unexpectedly remained valid"
            );
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
            assert!(
                validate(&generated).is_err(),
                "recurring successor mutation {mutate} unexpectedly remained valid"
            );
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
                                stage: GeneratedDestinationStage::Exact,
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
                                stage: GeneratedDestinationStage::Exact,
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
                                stage: GeneratedDestinationStage::Exact,
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
    fn evidence_descriptions_do_not_expose_internal_variant_names() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            for seed in 0..256 {
                let generated = generate(&context(seed, family)).unwrap();
                for evidence in generated.evidence {
                    assert!(
                        !evidence
                            .safe_description
                            .contains(&format!("{:?}", evidence.kind)),
                        "seed {seed} exposed {:?} in player-facing evidence",
                        evidence.kind
                    );
                }
            }
        }
    }

    #[test]
    fn follow_up_incidents_reuse_conditional_evidence_likelihoods() {
        let skeleton = (0..10_000)
            .filter_map(|entropy| {
                select_follow_up_evidence(
                    CanonicalCause::Hostile(ThreatId::Skeleton),
                    SiteKind::Crypt,
                    crate::settlement_population::stable_hash(&format!("skeleton:{entropy}")),
                )
            })
            .filter(|kind| *kind == EvidenceKind::BoneDust)
            .count();
        let bandit = (0..10_000)
            .filter_map(|entropy| {
                select_follow_up_evidence(
                    CanonicalCause::Hostile(ThreatId::Bandit),
                    SiteKind::Crypt,
                    crate::settlement_population::stable_hash(&format!("bandit:{entropy}")),
                )
            })
            .filter(|kind| *kind == EvidenceKind::BoneDust)
            .count();
        assert!(skeleton > bandit);
        assert!((0..10_000).all(|entropy| {
            select_follow_up_evidence(
                CanonicalCause::FabricatedClaim,
                SiteKind::OccupiedHouse,
                entropy,
            ) != Some(EvidenceKind::BloodlessCorpse)
        }));
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
            .find(|t| t.candidate_id == "occupied_house")
            .unwrap();
        assert_eq!(house.plausibility, 3);
        assert_eq!(
            house.required_bridge.as_ref().unwrap().0,
            "skeletons_occupied_house"
        );
        let wolf_crypt = site_candidates(CanonicalCause::Hostile(ThreatId::Wolf))
            .into_iter()
            .find(|c| c.id == "crypt")
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
        assert_eq!(adult.bridge, Some("child_at_adult_venue"));
    }

    #[test]
    fn truthful_spoken_location_matches_its_bound_site() {
        for seed in 0..128 {
            let generated = generate(&context(seed, TemplateFamily::RecurringDepredation)).unwrap();
            let statement = &generated.witnesses[0].testimony[0];
            if statement.reliability != Reliability::Truthful {
                continue;
            }
            let site_id = statement.site_id.as_ref().unwrap();
            let site = generated
                .sites
                .iter()
                .find(|candidate| candidate.id == *site_id)
                .unwrap();
            assert!(
                statement.spoken_text.contains(&site.safe_label),
                "truthful spoken site {:?} did not match bound site {:?}",
                statement.spoken_text,
                site.safe_label
            );
        }
    }

    #[test]
    fn selected_yaml_bridge_materializes_complete_reachable_authority() {
        let generated = generate(&context(44, TemplateFamily::DisappearanceOrLoss)).unwrap();
        assert!(
            !generated.bridges.is_empty(),
            "fixture seed must select a YAML-authored bridge"
        );
        validate(&generated).unwrap();

        let mut reachable = generated
            .actions
            .iter()
            .filter(|action| action.active_initially)
            .map(|action| action.id.clone())
            .collect::<BTreeSet<_>>();
        loop {
            let before = reachable.len();
            for action in &generated.actions {
                if action
                    .prerequisite
                    .as_ref()
                    .is_none_or(|required| reachable.contains(required))
                {
                    reachable.insert(action.id.clone());
                }
            }
            if reachable.len() == before {
                break;
            }
        }

        for bridge in &generated.bridges {
            assert!(
                generated
                    .canonical_events
                    .iter()
                    .any(|event| event.id == bridge.event_id)
            );
            assert!(
                generated
                    .evidence
                    .iter()
                    .any(|evidence| evidence.id == bridge.evidence_id)
            );
            assert!(reachable.contains(&bridge.action_id));
            let action = generated
                .actions
                .iter()
                .find(|action| action.id == bridge.action_id)
                .unwrap();
            assert!(action.outputs.iter().any(|output| {
                matches!(
                    output,
                    GeneratedActionOutput::Evidence { evidence_id }
                        if evidence_id == &bridge.evidence_id
                )
            }));
        }
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
    fn secondary_location_testimony_corresponds_to_the_primary_presented_site() {
        for family in [
            TemplateFamily::RecurringDepredation,
            TemplateFamily::DisappearanceOrLoss,
        ] {
            for truthful in [true, false] {
                let generated = case_with_primary_location_accuracy(family, truthful);
                validate(&generated).unwrap();

                let primary = &generated.witnesses[0].testimony[0];
                let secondary = &generated.witnesses[1].testimony[0];
                let primary_site = generated
                    .sites
                    .iter()
                    .find(|site| Some(&site.id) == primary.site_id.as_ref())
                    .expect("primary presented site is bound in the manifest");
                let finale_site = generated
                    .sites
                    .iter()
                    .find(|site| site.role == SiteRole::Finale)
                    .expect("generated case has a finale site");

                assert_eq!(secondary.proposition_id, primary.proposition_id);
                assert_eq!(secondary.site_id.as_ref(), Some(&finale_site.id));
                assert!(
                    primary.spoken_text.contains(label(primary_site.kind)),
                    "primary wording does not name its presented site kind"
                );

                if truthful {
                    assert_eq!(primary_site.id, finale_site.id);
                    assert_eq!(secondary.corrects_proposition_id, None);
                    assert!(
                        secondary.spoken_text.contains(label(primary_site.kind))
                            && secondary.spoken_text.contains("continue toward")
                            && secondary
                                .spoken_text
                                .contains("consistent with the earlier account")
                            && !secondary.spoken_text.contains("turn away"),
                        "truthful branch did not continue toward the primary's true site: {:?}",
                        secondary.spoken_text
                    );
                } else {
                    assert_eq!(primary_site.role, SiteRole::Decoy);
                    assert_ne!(primary_site.id, finale_site.id);
                    assert_eq!(
                        secondary.corrects_proposition_id.as_deref(),
                        Some(primary.proposition_id.as_str())
                    );
                    assert!(
                        secondary.spoken_text.contains(label(primary_site.kind))
                            && secondary.spoken_text.contains("turn away before reaching")
                            && secondary.spoken_text.contains("continue elsewhere"),
                        "mistaken branch did not correct away from the primary's presented decoy: {:?}",
                        secondary.spoken_text
                    );
                }
            }
        }
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
                .chain(case.track_trails.iter().map(|trail| trail.id.0.clone()))
                .chain(
                    case.track_segments
                        .iter()
                        .map(|segment| segment.id.0.clone()),
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
        let mistaken_tracks = mistaken
            .iter()
            .find(|candidate| candidate.value == AccountStyle::TracksAndMovement)
            .unwrap();
        let truthful_tracks =
            account_style_candidates(Reliability::Truthful, Circumstance::RoadJourney)
                .into_iter()
                .find(|candidate| candidate.value == AccountStyle::TracksAndMovement)
                .unwrap();
        assert_eq!(
            (mistaken_tracks.weight, mistaken_tracks.impossible),
            (truthful_tracks.weight, truthful_tracks.impossible),
            "account wording must not reveal hidden reliability"
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
