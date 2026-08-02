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
    Outbreak,
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
    /// Private incident-time precipitation snapshot committed with generation.
    pub incident_weather: crate::weather::Precipitation,
    pub requested_family: Option<TemplateFamily>,
    pub witness_candidates: Vec<WitnessCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessCandidate {
    pub resident_character_id: u64,
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

/// Player-visible settlement NPC and presence facts used by developer quest
/// preview, compilation, and later live-target validation.
///
/// `presentation` participates in the commitment but is never converted back
/// into private demographic sex.
#[derive(Clone, Copy, Debug)]
pub struct VisibleWitnessCandidateInput<'a> {
    pub resident_character_id: u64,
    pub display_name: &'a str,
    pub age_band: &'a str,
    pub presentation: &'a str,
    pub height: &'a str,
    pub build: &'a str,
    pub hair: &'a str,
    pub clothing: &'a str,
    pub profession: &'a str,
    pub local_role: &'a str,
    pub settlement_id: &'a str,
    pub location_id: &'a str,
    pub start_minute: u16,
    pub end_minute: u16,
    pub is_default: bool,
}

pub fn visible_witness_presence_version(input: &VisibleWitnessCandidateInput<'_>) -> u64 {
    let commitment = serde_json::to_string(&(
        "visible-witness-presence-v1",
        input.resident_character_id,
        input.age_band.to_ascii_lowercase(),
        input.presentation.to_ascii_lowercase(),
        input.profession,
        input.local_role,
        input.settlement_id,
        input.location_id,
        input.start_minute,
        input.end_minute,
        input.is_default,
    ))
    .expect("visible witness commitment tuple is serializable");
    crate::settlement_population::stable_hash(&commitment)
}

/// Build the exact witness candidate available to the developer quest UI.
///
/// The empty sex selector is intentional. The current catalog has no
/// sex-specific demographic rules, and future rules must fall back rather than
/// turn visible presentation into private sex.
pub fn visible_witness_candidate(
    input: VisibleWitnessCandidateInput<'_>,
) -> Option<WitnessCandidate> {
    let age_band = input.age_band.to_ascii_lowercase();
    let authored = crate::quest_catalog::catalog().witness_demographic_for(
        &age_band,
        "",
        input.profession,
        input.local_role,
    )?;
    let demographic = WitnessDemographic::try_new(&authored.id).ok()?;
    let mut allowed_circumstances = BTreeSet::from([
        Circumstance::NightWindow,
        Circumstance::RoadJourney,
        Circumstance::LivestockWatch,
    ]);
    if input.location_id == "church" {
        allowed_circumstances.insert(Circumstance::GraveDuty);
    }
    if input.location_id == "adult_venue" || demographic != WitnessDemographic::Child {
        allowed_circumstances.insert(Circumstance::AdultVenue);
    }
    if demographic != WitnessDemographic::Child {
        allowed_circumstances.insert(Circumstance::SecretRiversideMeeting);
    }
    Some(WitnessCandidate {
        resident_character_id: input.resident_character_id,
        display_name: input.display_name.into(),
        demographic,
        age_band,
        sex: String::new(),
        profession: input.profession.into(),
        visible_description: format!(
            "{}, {}, with {}, wearing {}",
            input.height, input.build, input.hair, input.clothing
        ),
        expected_location: input.location_id.into(),
        expected_location_label: String::new(),
        presence_version: visible_witness_presence_version(&input),
        allowed_circumstances,
    })
}

/// Removes witness/location combinations that the player cannot reach through
/// the settlement UI. Absence from `visible_tabs` is an authoritative hard
/// zero, not a low-probability candidate.
pub fn retain_navigable_witnesses(
    candidates: Vec<WitnessCandidate>,
    visible_tabs: &[crate::settlement_economy::SettlementResidentTab],
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
    pub resident_character_id: u64,
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
    expected.resident_character_id == current.resident_character_id
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

/// Private canonical truth for an outbreak case.
///
/// This payload is persisted only inside generated-case authority. Public
/// projections must derive observer-owned claims from evidence and testimony
/// rather than exposing any field here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedOutbreak {
    pub disease: crate::disease::DiseaseId,
    pub transmission_route: crate::disease::TransmissionVector,
    pub source: OutbreakSource,
    pub physical_source_site: SiteId,
    pub patient_presentation_site: SiteId,
    pub responsible_npc: Option<ResponsibleOutbreakNpc>,
    pub carrier_threat: Option<ThreatId>,
    pub exposure_chronology: Vec<OutbreakExposure>,
    pub remediation: OutbreakRemediation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakCulpability {
    Innocent,
    Negligent,
    Reckless,
    Deliberate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsibleOutbreakNpc {
    pub resident_character_id: u64,
    pub culpability: OutbreakCulpability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutbreakSource {
    Sanitation {
        practice: OutbreakSanitationPractice,
    },
    Behavior {
        practice: OutbreakBehaviorPractice,
    },
    ThreatVector {
        threat: ThreatId,
    },
    Environmental {
        reservoir: OutbreakEnvironmentalReservoir,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakSanitationPractice {
    ContaminatedWell,
    WasteNearWater,
    TaintedFoodStorage,
    UnwashedSharedBedding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakBehaviorPractice {
    CrowdedSleeping,
    HandlingTheSick,
    ReusingSoiledLinen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakEnvironmentalReservoir {
    GraveMould,
    RyeGalls,
    OreBiofilm,
    HouseDust,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutbreakExposure {
    pub patient_ref: String,
    /// Canonical existing settlement resident who is the medical subject.
    pub patient_character_id: u64,
    /// Explicit authoritative kinship only. `None` means clergy/civic custody;
    /// generation must never infer family from witness adjacency.
    pub episode_id: u64,
    pub exposed_at: u64,
    pub became_symptomatic_at: u64,
    pub died_at: Option<u64>,
    pub death_kind: Option<OutbreakPatientDeathKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakPatientDeathKind {
    Disease,
    CarrierAttack,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutbreakRemediation {
    Sanitation {
        action: OutbreakSanitationAction,
    },
    Behavior {
        action: OutbreakBehaviorAction,
    },
    RemoveEnvironmentalSource {
        reservoir: OutbreakEnvironmentalReservoir,
    },
    ResolveCarrierThreat {
        hostile_group_id: String,
        accepted_outcomes: Vec<OutbreakCarrierOutcome>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakSanitationAction {
    CloseWell,
    MoveWasteDownstream,
    DestroyTaintedStores,
    LaunderBedding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakBehaviorAction {
    SeparateSleepers,
    IsolatePatients,
    BoilLinen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutbreakCarrierOutcome {
    Defeated,
    DrivenOff,
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
    pub resident_character_id: u64,
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

pub fn exact_referral_contact(
    expected_resident_character_id: u64,
    addressed_resident_character_id: u64,
) -> bool {
    expected_resident_character_id == addressed_resident_character_id
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
    witness_resident_character_id: u64,
) -> Result<ReferredContactTransition, &'static str> {
    let matches: Vec<_> = states
        .iter()
        .enumerate()
        .filter(|(_, capability)| {
            capability.owner_character_id == owner_character_id
                && capability.case_id == canonical_case_id
                && capability.method == "locate_contact"
                && capability.target_kind == "contact"
                && capability.target_id == witness_resident_character_id.to_string()
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
    /// Grants an authoritative attempt at one exact physical source
    /// intervention. The intervention reducer still verifies current source
    /// state before emitting `SourceRemediated`.
    Remediation {
        remediation_id: String,
    },
    Consequence {
        consequence: GeneratedActionConsequence,
    },
    /// Declares which owning systemic adapter can emit a typed objective fact.
    /// This is producer wiring, never permission for the browser to post truth.
    SystemicOutcome { outcome: GeneratedSystemicOutcome },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedSystemicOutcome {
    Surrender { character_id: u64, context_id: String },
    RecruitOrDefect { character_id: u64, party_id: String },
    Ransom { character_id: u64, recipient_id: String },
    CustodyHandoff { character_id: u64, custodian_id: String },
    EscapeCustody { character_id: u64 },
    TransferOwnership { property_id: String, owner_id: String },
    Theft { property_id: String, victim_id: String },
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
    pub recipient_resident_character_id: u64,
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
    pub outbreak: Option<GeneratedOutbreak>,
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
