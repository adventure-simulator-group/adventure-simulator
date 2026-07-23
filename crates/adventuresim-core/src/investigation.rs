//! Deterministic, observer-safe investigation knowledge.
//!
//! Canonical truth is deliberately absent from [`InferenceInput`]. Knowledge is
//! proposition-granular: a speaker can conceal one fact, distort another, and
//! sincerely misremember a third without a witness-wide "lying" switch.

use crate::bestiary::{
    CandidateScore, EvidenceKind, ObservationDistance, ObservationVisibility, RegionalContext,
    ReportDescription, WitnessCapability, rank_candidates_in_region,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_TEXT: usize = 512;
pub const MAX_RECORDS: usize = 64;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 96
                    || !value.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')
                    })
                {
                    return Err(ValidationError::InvalidId);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

stable_id!(CaseId);
stable_id!(EventId);
stable_id!(PropositionId);
stable_id!(ObservationId);
stable_id!(RecollectionId);
stable_id!(ClaimId);
stable_id!(EvidenceId);
stable_id!(BeliefId);
stable_id!(LeadId);
stable_id!(RevisionId);
stable_id!(SharingReceiptId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasisPoints(u16);
impl BasisPoints {
    pub fn new(value: u16) -> Result<Self, ValidationError> {
        (value <= 10_000)
            .then_some(Self(value))
            .ok_or(ValidationError::OutOfRange)
    }
    pub const fn get(self) -> u16 {
        self.0
    }
    fn scaled(self, factor: u16) -> Self {
        Self(((u32::from(self.0) * u32::from(factor)) / 10_000).min(10_000) as u16)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    InvalidId,
    TextTooLong,
    OutOfRange,
    TooManyRecords,
    DuplicateRecord,
}

fn bounded_text(value: impl Into<String>) -> Result<String, ValidationError> {
    let value = value.into();
    (!value.is_empty() && value.len() <= MAX_TEXT)
        .then_some(value)
        .ok_or(ValidationError::TextTooLong)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicProposition {
    pub id: PropositionId,
    pub subject: String,
    pub predicate: String,
    pub object: String,
}
impl AtomicProposition {
    pub fn new(
        id: PropositionId,
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            id,
            subject: bounded_text(subject)?,
            predicate: bounded_text(predicate)?,
            object: bounded_text(object)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    pub id: EventId,
    pub case_id: CaseId,
    pub occurred_at: u64,
    pub propositions: Vec<AtomicProposition>,
}
impl CanonicalEvent {
    pub fn new(
        id: EventId,
        case_id: CaseId,
        occurred_at: u64,
        propositions: Vec<AtomicProposition>,
    ) -> Result<Self, ValidationError> {
        validate_unique(&propositions, |p| &p.id)?;
        Ok(Self {
            id,
            case_id,
            occurred_at,
            propositions,
        })
    }
}

fn validate_unique<T, K: Ord>(
    records: &[T],
    key: impl Fn(&T) -> &K,
) -> Result<(), ValidationError> {
    if records.len() > MAX_RECORDS {
        return Err(ValidationError::TooManyRecords);
    }
    let mut seen = BTreeSet::new();
    if records.iter().all(|record| seen.insert(key(record))) {
        Ok(())
    } else {
        Err(ValidationError::DuplicateRecord)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerceptionCondition {
    Clear,
    Darkness,
    PoorPerception,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryCondition {
    Accurate,
    Faded,
    Confused,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisclosureMode {
    Disclose,
    Omit,
    Conceal,
    Distort,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransmissionCondition {
    Clear,
    PoorTranslation,
    Hearsay,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub id: ObservationId,
    pub event_id: EventId,
    pub observer_ref: String,
    pub proposition_id: PropositionId,
    pub perceived_text: String,
    pub confidence: BasisPoints,
    pub condition: PerceptionCondition,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recollection {
    pub id: RecollectionId,
    pub observation_id: ObservationId,
    pub recalled_text: String,
    pub confidence: BasisPoints,
    pub condition: MemoryCondition,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    pub case_id: CaseId,
    pub proposition_id: PropositionId,
    pub speaker_ref: String,
    pub source_recollection_id: RecollectionId,
    pub statement: String,
    pub confidence: BasisPoints,
    pub disclosure: DisclosureMode,
    pub transmission: TransmissionCondition,
    pub received_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: EvidenceId,
    pub case_id: CaseId,
    pub proposition_id: PropositionId,
    pub description: String,
    pub confidence: BasisPoints,
    pub discovered_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    ReceivedClaim {
        claim_id: ClaimId,
        source: String,
    },
    DiscoveredEvidence {
        evidence_id: EvidenceId,
    },
    SharedBy {
        character_id: u64,
        source_id: String,
    },
    CorrectedBy {
        revision_id: RevisionId,
    },
    VisiblePrior {
        label: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeliefRevision {
    pub id: RevisionId,
    pub belief_id: BeliefId,
    pub revision: u16,
    pub statement: String,
    pub confidence: BasisPoints,
    pub provenance: Provenance,
    pub supersedes: Option<RevisionId>,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Belief {
    pub id: BeliefId,
    pub case_id: CaseId,
    pub owner_character_id: u64,
    pub proposition_id: PropositionId,
    pub current_revision: RevisionId,
    pub statement: String,
    pub confidence: BasisPoints,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DestinationKnowledge {
    Unknown,
    Textual {
        directions: String,
    },
    Landmark {
        landmark: String,
    },
    ApproximateArea {
        area: String,
    },
    RouteSegment {
        route: String,
    },
    ExactBelievedLocation {
        location_id: String,
        latitude_e7: i32,
        longitude_e7: i32,
    },
    Visited {
        location_id: String,
        latitude_e7: i32,
        longitude_e7: i32,
        visited_at: u64,
    },
}
impl DestinationKnowledge {
    pub fn exact_pin(&self) -> Option<(&str, i32, i32)> {
        match self {
            Self::ExactBelievedLocation {
                location_id,
                latitude_e7,
                longitude_e7,
            }
            | Self::Visited {
                location_id,
                latitude_e7,
                longitude_e7,
                ..
            } => Some((location_id, *latitude_e7, *longitude_e7)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lead {
    pub id: LeadId,
    pub case_id: CaseId,
    pub owner_character_id: u64,
    pub summary: String,
    pub source: Provenance,
    pub destination: DestinationKnowledge,
    pub witness_name: Option<String>,
    pub witness_description: Option<String>,
    pub witness_occupation_or_relationship: Option<String>,
    pub expected_location: Option<String>,
    pub current_learned_location: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharingReceipt {
    pub id: SharingReceiptId,
    pub sender_id: u64,
    pub recipient_id: u64,
    pub source_record_id: String,
    pub payload_fingerprint: String,
    pub shared_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineInput {
    pub case_id: CaseId,
    pub event_id: EventId,
    pub proposition: AtomicProposition,
    pub observer_ref: String,
    pub speaker_ref: String,
    pub perceived_text: String,
    pub recalled_text: String,
    pub disclosed_text: Option<String>,
    pub transmitted_text: String,
    pub perception: PerceptionCondition,
    pub memory: MemoryCondition,
    pub disclosure: DisclosureMode,
    pub transmission: TransmissionCondition,
    pub received_at: u64,
}

pub fn process_report(
    input: PipelineInput,
) -> Result<(Observation, Recollection, Option<Claim>), ValidationError> {
    let perception_factor = match input.perception {
        PerceptionCondition::Clear => 9_000,
        PerceptionCondition::Darkness => 4_500,
        PerceptionCondition::PoorPerception => 3_500,
    };
    let memory_factor = match input.memory {
        MemoryCondition::Accurate => 9_500,
        MemoryCondition::Faded => 6_500,
        MemoryCondition::Confused => 4_000,
    };
    let transmission_factor = match input.transmission {
        TransmissionCondition::Clear => 9_500,
        TransmissionCondition::PoorTranslation => 6_000,
        TransmissionCondition::Hearsay => 5_000,
    };
    let observation = Observation {
        id: ObservationId::new(format!("obs:{}", input.proposition.id.as_str()))?,
        event_id: input.event_id,
        observer_ref: bounded_text(input.observer_ref)?,
        proposition_id: input.proposition.id.clone(),
        perceived_text: bounded_text(input.perceived_text)?,
        confidence: BasisPoints::new(perception_factor)?,
        condition: input.perception,
    };
    let recollection = Recollection {
        id: RecollectionId::new(format!("memory:{}", input.proposition.id.as_str()))?,
        observation_id: observation.id.clone(),
        recalled_text: bounded_text(input.recalled_text)?,
        confidence: observation.confidence.scaled(memory_factor),
        condition: input.memory,
    };
    if input.disclosure == DisclosureMode::Omit {
        return Ok((observation, recollection, None));
    }
    let statement = input
        .disclosed_text
        .unwrap_or(input.transmitted_text.clone());
    let claim = Claim {
        id: ClaimId::new(format!(
            "claim:{}:{}",
            input.proposition.id.as_str(),
            input.received_at
        ))?,
        case_id: input.case_id,
        proposition_id: input.proposition.id,
        speaker_ref: bounded_text(input.speaker_ref)?,
        source_recollection_id: recollection.id.clone(),
        statement: bounded_text(if input.transmission == TransmissionCondition::Clear {
            statement
        } else {
            input.transmitted_text
        })?,
        confidence: recollection.confidence.scaled(transmission_factor),
        disclosure: input.disclosure,
        transmission: input.transmission,
        received_at: input.received_at,
    };
    Ok((observation, recollection, Some(claim)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleReport {
    pub description: ReportDescription,
    pub visibility: ObservationVisibility,
    pub distance: ObservationDistance,
    pub capability: WitnessCapability,
    pub source_label: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleEvidence {
    pub kind: EvidenceKind,
    pub evidence_id: EvidenceId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InferenceInput {
    pub report: VisibleReport,
    pub evidence: Vec<VisibleEvidence>,
    pub region: RegionalContext,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeInference {
    pub ranked: Vec<CandidateScore>,
    pub provenance: Vec<String>,
}
pub fn infer_threats(mut input: InferenceInput) -> Result<SafeInference, ValidationError> {
    if input.evidence.len() > 32 {
        return Err(ValidationError::TooManyRecords);
    }
    input
        .evidence
        .sort_by(|a, b| a.evidence_id.cmp(&b.evidence_id));
    input
        .evidence
        .dedup_by(|a, b| a.evidence_id == b.evidence_id);
    let evidence = input.evidence.iter().map(|e| e.kind).collect::<Vec<_>>();
    let ranked = rank_candidates_in_region(
        input.report.description,
        &evidence,
        input.report.visibility,
        input.report.distance,
        input.report.capability,
        input.region,
    );
    let mut provenance = vec![bounded_text(format!(
        "received report from {}",
        input.report.source_label
    ))?];
    provenance.extend(
        input
            .evidence
            .iter()
            .map(|e| format!("discovered evidence {}", e.evidence_id.as_str())),
    );
    provenance.push("regional prior: northern Germany, 1544".into());
    Ok(SafeInference { ranked, provenance })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bestiary::{
        ObservationDistance::Medium, ObservationVisibility::Dark,
        ReportDescription::LargeUprightBeast, WitnessCapability::Ordinary,
    };

    fn id<T>(make: impl FnOnce(String) -> Result<T, ValidationError>, value: &str) -> T {
        make(value.into()).unwrap()
    }
    fn pipeline(
        disclosure: DisclosureMode,
        perception: PerceptionCondition,
        memory: MemoryCondition,
        transmission: TransmissionCondition,
    ) -> (Observation, Recollection, Option<Claim>) {
        process_report(PipelineInput {
            case_id: id(CaseId::new, "case:1"),
            event_id: id(EventId::new, "event:1"),
            proposition: AtomicProposition::new(
                id(PropositionId::new, "prop:shape"),
                "creature",
                "shape",
                "upright",
            )
            .unwrap(),
            observer_ref: "witness:anna".into(),
            speaker_ref: "witness:anna".into(),
            perceived_text: "a dark upright shape".into(),
            recalled_text: if memory == MemoryCondition::Confused {
                "a man-shaped animal"
            } else {
                "a dark upright shape"
            }
            .into(),
            disclosed_text: (disclosure != DisclosureMode::Disclose).then(|| "only a man".into()),
            transmitted_text: if transmission == TransmissionCondition::Clear {
                "a dark upright shape"
            } else {
                "a black giant"
            }
            .into(),
            perception,
            memory,
            disclosure,
            transmission,
            received_at: 12,
        })
        .unwrap()
    }

    #[test]
    fn stages_model_darkness_memory_concealment_deception_and_language() {
        let dark = pipeline(
            DisclosureMode::Disclose,
            PerceptionCondition::Darkness,
            MemoryCondition::Accurate,
            TransmissionCondition::Clear,
        );
        assert_eq!(dark.0.confidence.get(), 4_500);
        let poor = pipeline(
            DisclosureMode::Disclose,
            PerceptionCondition::PoorPerception,
            MemoryCondition::Accurate,
            TransmissionCondition::Clear,
        );
        assert!(poor.0.confidence.get() < dark.0.confidence.get());
        let confused = pipeline(
            DisclosureMode::Disclose,
            PerceptionCondition::Clear,
            MemoryCondition::Confused,
            TransmissionCondition::Clear,
        );
        assert_eq!(confused.1.recalled_text, "a man-shaped animal");
        let omitted = pipeline(
            DisclosureMode::Omit,
            PerceptionCondition::Clear,
            MemoryCondition::Accurate,
            TransmissionCondition::Clear,
        );
        assert!(omitted.2.is_none());
        let concealed = pipeline(
            DisclosureMode::Conceal,
            PerceptionCondition::Clear,
            MemoryCondition::Accurate,
            TransmissionCondition::Clear,
        );
        assert_eq!(concealed.2.unwrap().statement, "only a man");
        let distorted = pipeline(
            DisclosureMode::Distort,
            PerceptionCondition::Clear,
            MemoryCondition::Accurate,
            TransmissionCondition::PoorTranslation,
        );
        assert_eq!(distorted.2.unwrap().statement, "a black giant");
        assert_eq!(
            dark,
            pipeline(
                DisclosureMode::Disclose,
                PerceptionCondition::Darkness,
                MemoryCondition::Accurate,
                TransmissionCondition::Clear
            )
        );
    }

    #[test]
    fn partial_truth_conflict_and_correction_never_mutate_canonical_event() {
        let truth = CanonicalEvent::new(
            id(EventId::new, "event:1"),
            id(CaseId::new, "case:1"),
            1,
            vec![
                AtomicProposition::new(
                    id(PropositionId::new, "prop:shape"),
                    "creature",
                    "shape",
                    "upright",
                )
                .unwrap(),
                AtomicProposition::new(
                    id(PropositionId::new, "prop:time"),
                    "attack",
                    "time",
                    "midnight",
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let snapshot = truth.clone();
        let a = pipeline(
            DisclosureMode::Disclose,
            PerceptionCondition::Darkness,
            MemoryCondition::Accurate,
            TransmissionCondition::Clear,
        )
        .2
        .unwrap();
        let b = pipeline(
            DisclosureMode::Distort,
            PerceptionCondition::Clear,
            MemoryCondition::Confused,
            TransmissionCondition::Clear,
        )
        .2
        .unwrap();
        assert_ne!(a.statement, b.statement);
        let correction = BeliefRevision {
            id: id(RevisionId::new, "revision:2"),
            belief_id: id(BeliefId::new, "belief:shape"),
            revision: 2,
            statement: "a dark upright shape".into(),
            confidence: BasisPoints::new(8_000).unwrap(),
            provenance: Provenance::CorrectedBy {
                revision_id: id(RevisionId::new, "revision:1"),
            },
            supersedes: Some(id(RevisionId::new, "revision:1")),
            recorded_at: 20,
        };
        assert_eq!(correction.revision, 2);
        assert_eq!(truth, snapshot);
    }

    #[test]
    fn inference_has_only_visible_inputs_and_safe_provenance() {
        let base = InferenceInput {
            report: VisibleReport {
                description: LargeUprightBeast,
                visibility: Dark,
                distance: Medium,
                capability: Ordinary,
                source_label: "the miller".into(),
            },
            evidence: vec![],
            region: RegionalContext::NorthernGermany1544,
        };
        let before = infer_threats(base.clone()).unwrap();
        let mut after_input = base;
        after_input.evidence.push(VisibleEvidence {
            kind: EvidenceKind::Pawprints,
            evidence_id: id(EvidenceId::new, "evidence:paws"),
        });
        let after = infer_threats(after_input).unwrap();
        assert_ne!(before.ranked, after.ranked);
        let provenance = after.provenance.join(" ");
        assert!(!provenance.contains("truth"));
        assert!(!provenance.contains("sincerity"));
        assert!(!provenance.contains("bridge"));
    }

    #[test]
    fn only_exact_or_visited_destinations_make_pins() {
        assert!(DestinationKnowledge::Unknown.exact_pin().is_none());
        assert!(
            DestinationKnowledge::Textual {
                directions: "north".into()
            }
            .exact_pin()
            .is_none()
        );
        assert!(
            DestinationKnowledge::ApproximateArea {
                area: "north wood".into()
            }
            .exact_pin()
            .is_none()
        );
        let mistaken = DestinationKnowledge::ExactBelievedLocation {
            location_id: "wrong-cave".into(),
            latitude_e7: 1,
            longitude_e7: 2,
        };
        assert_eq!(mistaken.exact_pin(), Some(("wrong-cave", 1, 2)));
        assert_eq!(
            mistaken,
            DestinationKnowledge::ExactBelievedLocation {
                location_id: "wrong-cave".into(),
                latitude_e7: 1,
                longitude_e7: 2
            }
        );
    }
}
