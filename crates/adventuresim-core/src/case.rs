//! Strategic case, objective, contract, and custody rules.
//!
//! A [`Case`] exists because something happened in the world. A [`Contract`]
//! is merely one party's agreement to help with it. Neither tactical combat
//! nor contract acceptance resolves a case directly: authenticated
//! [`OutcomeFact`]s are reduced through the objective expression.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_ID: usize = 160;
const MAX_PATHS: usize = 16;
const MAX_LEAVES: usize = 32;

macro_rules! id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
                let value = value.into();
                if value.len() <= $prefix.len()
                    || value.len() > MAX_ID
                    || !value.starts_with($prefix)
                    || !value.bytes().all(|b| {
                        b.is_ascii_lowercase()
                            || b.is_ascii_digit()
                            || matches!(b, b':' | b'-' | b'_' | b'.')
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

id!(CaseId, "case:");
id!(ContractId, "contract:");
id!(ObjectiveId, "objective:");
id!(OutcomeFactId, "fact:");
id!(AssetId, "asset:");
id!(SubjectId, "subject:");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    InvalidId,
    EmptyExpression,
    EmptyPath,
    TooManyPaths,
    TooManyLeaves,
    DuplicateObjective,
    InvalidQuantity,
}

/// Private strategic authority. The investigation case with the same ID owns
/// hidden narrative truth; this type owns only resolution state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub id: CaseId,
    pub investigation_case_id: String,
    pub local_problem_id: Option<String>,
    pub status: CaseStatus,
    pub resolution: ObjectiveExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseStatus {
    Open,
    Resolved,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractStatus {
    Offered,
    Accepted,
    ReadyToReport,
    Paid,
    Withdrawn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub case_id: CaseId,
    pub party_id: Option<String>,
    pub status: ContractStatus,
    pub reward_minor: u64,
}

/// Disjunction of conjunctions: any path may resolve the case, but every leaf
/// in the selected path must be satisfied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveExpression {
    pub alternatives: Vec<ObjectivePath>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectivePath {
    pub objectives: Vec<Objective>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Objective {
    pub id: ObjectiveId,
    pub requirement: ObjectiveRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectiveRequirement {
    Defeat {
        hostile_group_id: String,
        count: u32,
    },
    DriveOff {
        hostile_group_id: String,
    },
    Capture {
        subject_id: SubjectId,
    },
    SurviveWindow {
        site_id: String,
        through_minute: u64,
    },
    Rescue {
        subject_id: SubjectId,
    },
    EscortTo {
        subject_id: SubjectId,
        site_id: String,
    },
    Retrieve {
        asset_id: AssetId,
    },
    Return {
        asset_id: AssetId,
        custodian_id: String,
    },
    Locate {
        subject_ref: String,
    },
    Identify {
        subject_ref: String,
    },
    /// The exact physical intervention required by private outbreak authority.
    RemediateSource {
        remediation_id: String,
    },
    SolveChallenge {
        challenge_id: String,
    },
    Expose {
        subject_ref: String,
    },
    PresentProof {
        evidence_id: String,
        recipient_id: String,
    },
    PresentTestimony {
        witness_id: String,
        recipient_id: String,
    },
    Protect {
        subject_id: SubjectId,
        through_minute: u64,
    },
    Negotiate {
        subject_ref: String,
    },
    Release {
        subject_id: SubjectId,
    },
    Exchange {
        asset_id: AssetId,
        recipient_id: String,
    },
    ReportToIssuer {
        issuer_id: String,
    },
    Surrender {
        character_id: u64,
        context_id: String,
    },
    RecruitOrDefect {
        character_id: u64,
        party_id: String,
    },
    Ransom {
        character_id: u64,
        recipient_id: String,
    },
    CustodyHandoff {
        character_id: u64,
        custodian_id: String,
    },
    EscapeCustody {
        character_id: u64,
    },
    TransferOwnership {
        property_id: String,
        owner_id: String,
    },
    CommitTheft {
        property_id: String,
        victim_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeFact {
    /// Stable source-scoped ID; inserting the same fact twice is idempotent.
    pub id: OutcomeFactId,
    pub case_id: CaseId,
    pub party_id: String,
    pub source_id: String,
    pub happened_at: u64,
    pub kind: OutcomeFactKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeFactKind {
    HostilesDefeated {
        hostile_group_id: String,
        count: u32,
    },
    HostilesDrivenOff {
        hostile_group_id: String,
    },
    SubjectCaptured {
        subject_id: SubjectId,
    },
    WindowSurvived {
        site_id: String,
        through_minute: u64,
    },
    SubjectRescued {
        subject_id: SubjectId,
    },
    SubjectEscorted {
        subject_id: SubjectId,
        site_id: String,
    },
    AssetRetrieved {
        asset_id: AssetId,
    },
    AssetReturned {
        asset_id: AssetId,
        custodian_id: String,
    },
    Located {
        subject_ref: String,
    },
    Identified {
        subject_ref: String,
    },
    SourceRemediated {
        remediation_id: String,
    },
    ChallengeSolved {
        challenge_id: String,
    },
    Exposed {
        subject_ref: String,
    },
    ProofPresented {
        evidence_id: String,
        recipient_id: String,
    },
    TestimonyPresented {
        witness_id: String,
        recipient_id: String,
    },
    SubjectProtected {
        subject_id: SubjectId,
        through_minute: u64,
    },
    Negotiated {
        subject_ref: String,
    },
    SubjectReleased {
        subject_id: SubjectId,
    },
    AssetExchanged {
        asset_id: AssetId,
        recipient_id: String,
    },
    Reported {
        issuer_id: String,
    },
    CharacterSurrendered {
        character_id: u64,
        context_id: String,
    },
    CharacterRecruited {
        character_id: u64,
        party_id: String,
    },
    RansomPaid {
        character_id: u64,
        recipient_id: String,
    },
    CustodyHandedOff {
        character_id: u64,
        custodian_id: String,
    },
    CharacterEscaped {
        character_id: u64,
    },
    OwnershipTransferred {
        property_id: String,
        owner_id: String,
    },
    TheftCommitted {
        property_id: String,
        victim_id: String,
    },
    /// Authoritative failure can make a leaf impossible. It is deliberately
    /// objective-specific so one failed route does not poison an alternative.
    ObjectiveImpossible {
        objective_id: ObjectiveId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvaluationState {
    Pending,
    Satisfied,
    Impossible,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveProgress {
    pub objective_id: ObjectiveId,
    pub state: EvaluationState,
    pub current: u32,
    pub required: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evaluation {
    pub state: EvaluationState,
    pub alternatives: Vec<Vec<ObjectiveProgress>>,
}

impl ObjectiveExpression {
    pub fn new(alternatives: Vec<ObjectivePath>) -> Result<Self, ValidationError> {
        if alternatives.is_empty() {
            return Err(ValidationError::EmptyExpression);
        }
        if alternatives.len() > MAX_PATHS {
            return Err(ValidationError::TooManyPaths);
        }
        let mut ids = BTreeSet::new();
        let mut leaves = 0;
        for path in &alternatives {
            if path.objectives.is_empty() {
                return Err(ValidationError::EmptyPath);
            }
            leaves += path.objectives.len();
            for objective in &path.objectives {
                if !ids.insert(&objective.id) {
                    return Err(ValidationError::DuplicateObjective);
                }
                if matches!(
                    objective.requirement,
                    ObjectiveRequirement::Defeat { count: 0, .. }
                ) {
                    return Err(ValidationError::InvalidQuantity);
                }
            }
        }
        if leaves > MAX_LEAVES {
            return Err(ValidationError::TooManyLeaves);
        }
        Ok(Self { alternatives })
    }

    pub fn evaluate(&self, case_id: &CaseId, party_id: &str, facts: &[OutcomeFact]) -> Evaluation {
        // Filtering here is a second boundary after persistence validation:
        // unrelated battles, parties, and cases contribute no progress.
        let facts: Vec<_> = facts
            .iter()
            .filter(|fact| &fact.case_id == case_id && fact.party_id == party_id)
            .collect();
        let alternatives: Vec<_> = self
            .alternatives
            .iter()
            .map(|path| {
                path.objectives
                    .iter()
                    .map(|objective| progress(objective, &facts))
                    .collect::<Vec<_>>()
            })
            .collect();
        let state = if alternatives
            .iter()
            .any(|path| path.iter().all(|p| p.state == EvaluationState::Satisfied))
        {
            EvaluationState::Satisfied
        } else if alternatives
            .iter()
            .all(|path| path.iter().any(|p| p.state == EvaluationState::Impossible))
        {
            EvaluationState::Impossible
        } else {
            EvaluationState::Pending
        };
        Evaluation {
            state,
            alternatives,
        }
    }
}

fn progress(objective: &Objective, facts: &[&OutcomeFact]) -> ObjectiveProgress {
    if facts.iter().any(|fact| {
        matches!(
            &fact.kind,
            OutcomeFactKind::ObjectiveImpossible { objective_id } if objective_id == &objective.id
        )
    }) {
        return ObjectiveProgress {
            objective_id: objective.id.clone(),
            state: EvaluationState::Impossible,
            current: 0,
            required: required_count(&objective.requirement),
        };
    }
    let required = required_count(&objective.requirement);
    let current = facts
        .iter()
        .filter_map(|fact| match_fact(&objective.requirement, &fact.kind))
        .fold(0_u32, u32::saturating_add)
        .min(required);
    ObjectiveProgress {
        objective_id: objective.id.clone(),
        state: if current >= required {
            EvaluationState::Satisfied
        } else {
            EvaluationState::Pending
        },
        current,
        required,
    }
}

fn required_count(requirement: &ObjectiveRequirement) -> u32 {
    match requirement {
        ObjectiveRequirement::Defeat { count, .. } => *count,
        _ => 1,
    }
}

fn match_fact(requirement: &ObjectiveRequirement, fact: &OutcomeFactKind) -> Option<u32> {
    use ObjectiveRequirement as R;
    use OutcomeFactKind as F;
    let yes = match (requirement, fact) {
        (
            R::Defeat {
                hostile_group_id: a,
                ..
            },
            F::HostilesDefeated {
                hostile_group_id: b,
                count,
            },
        ) if a == b => return Some(*count),
        (
            R::DriveOff {
                hostile_group_id: a,
            },
            F::HostilesDrivenOff {
                hostile_group_id: b,
            },
        ) if a == b => true,
        (R::Capture { subject_id: a }, F::SubjectCaptured { subject_id: b })
        | (R::Rescue { subject_id: a }, F::SubjectRescued { subject_id: b })
        | (R::Release { subject_id: a }, F::SubjectReleased { subject_id: b })
            if a == b =>
        {
            true
        }
        (
            R::SurviveWindow {
                site_id: a,
                through_minute: required,
            },
            F::WindowSurvived {
                site_id: b,
                through_minute: actual,
            },
        ) if a == b && actual >= required => true,
        (
            R::EscortTo {
                subject_id: a,
                site_id: sa,
            },
            F::SubjectEscorted {
                subject_id: b,
                site_id: sb,
            },
        ) if a == b && sa == sb => true,
        (R::Retrieve { asset_id: a }, F::AssetRetrieved { asset_id: b }) if a == b => true,
        (
            R::Return {
                asset_id: a,
                custodian_id: ca,
            },
            F::AssetReturned {
                asset_id: b,
                custodian_id: cb,
            },
        ) if a == b && ca == cb => true,
        (R::Locate { subject_ref: a }, F::Located { subject_ref: b })
        | (R::Identify { subject_ref: a }, F::Identified { subject_ref: b })
        | (R::Expose { subject_ref: a }, F::Exposed { subject_ref: b })
        | (R::Negotiate { subject_ref: a }, F::Negotiated { subject_ref: b })
            if a == b =>
        {
            true
        }
        (R::RemediateSource { remediation_id: a }, F::SourceRemediated { remediation_id: b })
            if a == b =>
        {
            true
        }
        (R::SolveChallenge { challenge_id: a }, F::ChallengeSolved { challenge_id: b })
            if a == b =>
        {
            true
        }
        (
            R::PresentProof {
                evidence_id: a,
                recipient_id: ra,
            },
            F::ProofPresented {
                evidence_id: b,
                recipient_id: rb,
            },
        ) if a == b && ra == rb => true,
        (
            R::PresentTestimony {
                witness_id: a,
                recipient_id: ra,
            },
            F::TestimonyPresented {
                witness_id: b,
                recipient_id: rb,
            },
        ) if a == b && ra == rb => true,
        (
            R::Protect {
                subject_id: a,
                through_minute: required,
            },
            F::SubjectProtected {
                subject_id: b,
                through_minute: actual,
            },
        ) if a == b && actual >= required => true,
        (
            R::Exchange {
                asset_id: a,
                recipient_id: ra,
            },
            F::AssetExchanged {
                asset_id: b,
                recipient_id: rb,
            },
        ) if a == b && ra == rb => true,
        (R::ReportToIssuer { issuer_id: a }, F::Reported { issuer_id: b }) if a == b => true,
        (
            R::Surrender {
                character_id: a,
                context_id: ca,
            },
            F::CharacterSurrendered {
                character_id: b,
                context_id: cb,
            },
        ) if a == b && ca == cb => true,
        (
            R::RecruitOrDefect {
                character_id: a,
                party_id: pa,
            },
            F::CharacterRecruited {
                character_id: b,
                party_id: pb,
            },
        ) if a == b && pa == pb => true,
        (
            R::Ransom {
                character_id: a,
                recipient_id: ra,
            },
            F::RansomPaid {
                character_id: b,
                recipient_id: rb,
            },
        ) if a == b && ra == rb => true,
        (
            R::CustodyHandoff {
                character_id: a,
                custodian_id: ca,
            },
            F::CustodyHandedOff {
                character_id: b,
                custodian_id: cb,
            },
        ) if a == b && ca == cb => true,
        (R::EscapeCustody { character_id: a }, F::CharacterEscaped { character_id: b })
            if a == b =>
        {
            true
        }
        (
            R::TransferOwnership {
                property_id: a,
                owner_id: oa,
            },
            F::OwnershipTransferred {
                property_id: b,
                owner_id: ob,
            },
        ) if a == b && oa == ob => true,
        (
            R::CommitTheft {
                property_id: a,
                victim_id: va,
            },
            F::TheftCommitted {
                property_id: b,
                victim_id: vb,
            },
        ) if a == b && va == vb => true,
        _ => false,
    };
    yes.then_some(1)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustodyHolder {
    Site(String),
    Party(String),
    Character(u64),
    Npc(String),
    Destroyed,
    Released,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyRecord {
    pub case_id: CaseId,
    pub object: CustodyObject,
    pub holder: CustodyHolder,
    pub version: u32,
    pub source_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CustodyObject {
    Asset(AssetId),
    Subject(SubjectId),
}

/// Applies source-idempotent custody transitions while enforcing one current
/// holder per asset or subject.
pub fn apply_custody(
    records: &mut BTreeMap<CustodyObject, CustodyRecord>,
    next: CustodyRecord,
) -> Result<bool, &'static str> {
    if let Some(current) = records.get(&next.object) {
        if current.source_id == next.source_id {
            return Ok(false);
        }
        if next.version != current.version.saturating_add(1) {
            return Err("custody transition has stale or skipped version");
        }
        if current.case_id != next.case_id {
            return Err("custody object cannot move between cases");
        }
    } else if next.version != 0 {
        return Err("initial custody version must be zero");
    }
    records.insert(next.object.clone(), next);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(value: &str) -> ObjectiveId {
        ObjectiveId::new(value).unwrap()
    }
    fn cid(value: &str) -> CaseId {
        CaseId::new(value).unwrap()
    }
    fn fact(id: &str, case: &str, party: &str, kind: OutcomeFactKind) -> OutcomeFact {
        OutcomeFact {
            id: OutcomeFactId::new(id).unwrap(),
            case_id: cid(case),
            party_id: party.into(),
            source_id: id.into(),
            happened_at: 1,
            kind,
        }
    }

    #[test]
    fn dnf_tracks_partial_progress_and_accepts_an_alternative() {
        let expression = ObjectiveExpression::new(vec![
            ObjectivePath {
                objectives: vec![Objective {
                    id: oid("objective:fight"),
                    requirement: ObjectiveRequirement::Defeat {
                        hostile_group_id: "hostile-group:wolves".into(),
                        count: 3,
                    },
                }],
            },
            ObjectivePath {
                objectives: vec![
                    Objective {
                        id: oid("objective:talk"),
                        requirement: ObjectiveRequirement::Negotiate {
                            subject_ref: "wolf-keeper".into(),
                        },
                    },
                    Objective {
                        id: oid("objective:report"),
                        requirement: ObjectiveRequirement::ReportToIssuer {
                            issuer_id: "reeve".into(),
                        },
                    },
                ],
            },
        ])
        .unwrap();
        let facts = vec![fact(
            "fact:battle-1",
            "case:wolves",
            "party:a",
            OutcomeFactKind::HostilesDefeated {
                hostile_group_id: "hostile-group:wolves".into(),
                count: 2,
            },
        )];
        let partial = expression.evaluate(&cid("case:wolves"), "party:a", &facts);
        assert_eq!(partial.state, EvaluationState::Pending);
        assert_eq!(partial.alternatives[0][0].current, 2);

        let negotiated = vec![
            fact(
                "fact:talk",
                "case:wolves",
                "party:a",
                OutcomeFactKind::Negotiated {
                    subject_ref: "wolf-keeper".into(),
                },
            ),
            fact(
                "fact:report",
                "case:wolves",
                "party:a",
                OutcomeFactKind::Reported {
                    issuer_id: "reeve".into(),
                },
            ),
        ];
        assert_eq!(
            expression
                .evaluate(&cid("case:wolves"), "party:a", &negotiated)
                .state,
            EvaluationState::Satisfied
        );
    }

    #[test]
    fn challenge_fact_satisfies_only_its_typed_challenge_objective() {
        let expression = ObjectiveExpression::new(vec![ObjectivePath {
            objectives: vec![Objective {
                id: oid("objective:solve"),
                requirement: ObjectiveRequirement::SolveChallenge {
                    challenge_id: "challenge:five-signs".into(),
                },
            }],
        }])
        .unwrap();
        let wrong = fact(
            "fact:wrong",
            "case:trial",
            "party:knights",
            OutcomeFactKind::Negotiated {
                subject_ref: "challenge:five-signs".into(),
            },
        );
        assert_eq!(
            expression
                .evaluate(&cid("case:trial"), "party:knights", &[wrong])
                .state,
            EvaluationState::Pending
        );
        let solved = fact(
            "fact:solved",
            "case:trial",
            "party:knights",
            OutcomeFactKind::ChallengeSolved {
                challenge_id: "challenge:five-signs".into(),
            },
        );
        assert_eq!(
            expression
                .evaluate(&cid("case:trial"), "party:knights", &[solved])
                .state,
            EvaluationState::Satisfied
        );
    }

    #[test]
    fn optional_preliminary_challenge_does_not_gate_finale_defeat() {
        let expression = ObjectiveExpression::new(vec![ObjectivePath {
            objectives: vec![Objective {
                id: oid("objective:finale"),
                requirement: ObjectiveRequirement::Defeat {
                    hostile_group_id: "hostile-group:errantry-finale".into(),
                    count: 4,
                },
            }],
        }])
        .unwrap();
        let finale = fact(
            "fact:finale",
            "case:errantry",
            "party:knights",
            OutcomeFactKind::HostilesDefeated {
                hostile_group_id: "hostile-group:errantry-finale".into(),
                count: 4,
            },
        );
        assert_eq!(
            expression
                .evaluate(&cid("case:errantry"), "party:knights", &[finale])
                .state,
            EvaluationState::Satisfied
        );
    }

    #[test]
    fn impossible_path_does_not_poison_viable_alternative() {
        let expression = ObjectiveExpression::new(vec![
            ObjectivePath {
                objectives: vec![Objective {
                    id: oid("objective:capture"),
                    requirement: ObjectiveRequirement::Capture {
                        subject_id: SubjectId::new("subject:bandit").unwrap(),
                    },
                }],
            },
            ObjectivePath {
                objectives: vec![Objective {
                    id: oid("objective:expose"),
                    requirement: ObjectiveRequirement::Expose {
                        subject_ref: "bandit".into(),
                    },
                }],
            },
        ])
        .unwrap();
        let impossible = fact(
            "fact:dead",
            "case:bandit",
            "party:a",
            OutcomeFactKind::ObjectiveImpossible {
                objective_id: oid("objective:capture"),
            },
        );
        assert_eq!(
            expression
                .evaluate(&cid("case:bandit"), "party:a", &[impossible])
                .state,
            EvaluationState::Pending
        );
    }

    #[test]
    fn wrong_case_party_and_group_cannot_progress() {
        let expression = ObjectiveExpression::new(vec![ObjectivePath {
            objectives: vec![Objective {
                id: oid("objective:defeat"),
                requirement: ObjectiveRequirement::Defeat {
                    hostile_group_id: "hostile-group:right".into(),
                    count: 1,
                },
            }],
        }])
        .unwrap();
        let facts = vec![
            fact(
                "fact:wrong-case",
                "case:other",
                "party:a",
                OutcomeFactKind::HostilesDefeated {
                    hostile_group_id: "hostile-group:right".into(),
                    count: 1,
                },
            ),
            fact(
                "fact:wrong-party",
                "case:right",
                "party:b",
                OutcomeFactKind::HostilesDefeated {
                    hostile_group_id: "hostile-group:right".into(),
                    count: 1,
                },
            ),
            fact(
                "fact:wrong-group",
                "case:right",
                "party:a",
                OutcomeFactKind::HostilesDefeated {
                    hostile_group_id: "hostile-group:wrong".into(),
                    count: 1,
                },
            ),
        ];
        assert_eq!(
            expression
                .evaluate(&cid("case:right"), "party:a", &facts)
                .state,
            EvaluationState::Pending
        );
    }

    #[test]
    fn custody_is_unique_versioned_and_source_idempotent() {
        let object = CustodyObject::Asset(AssetId::new("asset:seal").unwrap());
        let mut records = BTreeMap::new();
        let initial = CustodyRecord {
            case_id: cid("case:seal"),
            object: object.clone(),
            holder: CustodyHolder::Site("crypt".into()),
            version: 0,
            source_id: "seed".into(),
        };
        assert!(apply_custody(&mut records, initial.clone()).unwrap());
        assert!(!apply_custody(&mut records, initial).unwrap());
        assert!(
            apply_custody(
                &mut records,
                CustodyRecord {
                    case_id: cid("case:seal"),
                    object: object.clone(),
                    holder: CustodyHolder::Party("party:a".into()),
                    version: 2,
                    source_id: "pickup".into(),
                }
            )
            .is_err()
        );
        assert!(
            apply_custody(
                &mut records,
                CustodyRecord {
                    case_id: cid("case:seal"),
                    object: object.clone(),
                    holder: CustodyHolder::Party("party:a".into()),
                    version: 1,
                    source_id: "pickup".into(),
                }
            )
            .unwrap()
        );
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn systemic_character_fact_satisfies_only_exact_typed_objective() {
        let expression = ObjectiveExpression::new(vec![ObjectivePath {
            objectives: vec![Objective {
                id: ObjectiveId::new("objective:surrender").unwrap(),
                requirement: ObjectiveRequirement::Surrender {
                    character_id: 42,
                    context_id: "hostile:test".into(),
                },
            }],
        }])
        .unwrap();
        let matching = fact(
            "fact:surrender",
            "case:test",
            "party:a",
            OutcomeFactKind::CharacterSurrendered {
                character_id: 42,
                context_id: "hostile:test".into(),
            },
        );
        let wrong = fact(
            "fact:wrong-surrender",
            "case:test",
            "party:a",
            OutcomeFactKind::CharacterSurrendered {
                character_id: 41,
                context_id: "hostile:test".into(),
            },
        );
        assert_eq!(
            expression
                .evaluate(&cid("case:test"), "party:a", &[wrong])
                .state,
            EvaluationState::Pending
        );
        assert_eq!(
            expression
                .evaluate(&cid("case:test"), "party:a", &[matching])
                .state,
            EvaluationState::Satisfied
        );
    }
}
