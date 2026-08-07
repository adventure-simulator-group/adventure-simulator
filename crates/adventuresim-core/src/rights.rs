//! Pure, typed questions and evidence for strategic rights decisions.
//!
//! These values do not grant authority and have no persistence or wire format.
//! Domain reducers gather private authoritative evidence, ask exact questions,
//! and remain responsible for transactional mutation and consequences.

use std::{fmt, num::NonZeroU64};

use crate::{
    physical_object::{
        CustodyCharacterId, CustodyPartyId, ObjectCustody, OperationalCustody, PhysicalObjectId,
    },
    strategic_place::{StrategicFixtureId, StrategicPlaceId},
};

pub trait DomainRightsSubject: Clone + fmt::Debug + Eq {}
pub trait DomainRightsResource: Clone + fmt::Debug + Eq {}
pub trait DomainRightsOperation: Clone + fmt::Debug + Eq {}
pub trait DomainJurisdiction: Clone + fmt::Debug + Eq {}
pub trait DomainGrantSource: Clone + fmt::Debug + Eq {}
pub trait DomainRightsEvidence: Clone + fmt::Debug + Eq {}
pub trait DomainObligationEvidence: Clone + fmt::Debug + Eq {}
pub trait DomainRightsCommitReceipt: Clone + fmt::Debug + Eq {}
pub trait OrganizationIdentity: Clone + fmt::Debug + Eq {}
pub trait OrganizationRole: Clone + fmt::Debug + Eq {}
pub trait OrganizationPresentation: Clone + fmt::Debug + Eq {}
pub trait OrganizationRecognition: Clone + fmt::Debug + Eq {}
pub trait OrganizationPrivilegeEvidenceValue: Clone + fmt::Debug + Eq {}
pub trait PublicRightsAllowance: Clone + fmt::Debug + Eq {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightsSubject<S: DomainRightsSubject> {
    Character(CustodyCharacterId),
    Party(CustodyPartyId),
    Domain(S),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightsResource<R: DomainRightsResource> {
    Object(PhysicalObjectId),
    Place(StrategicPlaceId),
    Fixture(StrategicFixtureId),
    Domain(R),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightsOperation<O: DomainRightsOperation> {
    Own,
    HoldCustody,
    Use,
    TransferCustody { destination: OperationalCustody },
    Alter,
    Access,
    ReceivePermission,
    Domain(O),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightsJurisdiction<J: DomainJurisdiction> {
    Global,
    Place(StrategicPlaceId),
    Domain(J),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RightsQuestion<
    S: DomainRightsSubject,
    R: DomainRightsResource,
    O: DomainRightsOperation,
    J: DomainJurisdiction,
> {
    subject: RightsSubject<S>,
    resource: RightsResource<R>,
    operation: RightsOperation<O>,
    jurisdiction: RightsJurisdiction<J>,
}

impl<
    S: DomainRightsSubject,
    R: DomainRightsResource,
    O: DomainRightsOperation,
    J: DomainJurisdiction,
> RightsQuestion<S, R, O, J>
{
    pub fn try_new(
        subject: RightsSubject<S>,
        resource: RightsResource<R>,
        operation: RightsOperation<O>,
        jurisdiction: RightsJurisdiction<J>,
    ) -> Result<Self, RightsQuestionError> {
        if let RightsJurisdiction::Place(expected) = &jurisdiction {
            match &resource {
                RightsResource::Place(actual) if actual != expected => {
                    return Err(RightsQuestionError::ResourceJurisdictionMismatch);
                }
                RightsResource::Fixture(actual) if actual.place() != expected => {
                    return Err(RightsQuestionError::ResourceJurisdictionMismatch);
                }
                _ => {}
            }
        }
        if let (
            RightsResource::Object(object_id),
            RightsOperation::TransferCustody {
                destination: OperationalCustody::Container(container_id),
            },
        ) = (&resource, &operation)
            && object_id == container_id
        {
            return Err(RightsQuestionError::SelfContainment);
        }
        Ok(Self {
            subject,
            resource,
            operation,
            jurisdiction,
        })
    }

    pub const fn subject(&self) -> &RightsSubject<S> {
        &self.subject
    }
    pub const fn resource(&self) -> &RightsResource<R> {
        &self.resource
    }
    pub const fn operation(&self) -> &RightsOperation<O> {
        &self.operation
    }
    pub const fn jurisdiction(&self) -> &RightsJurisdiction<J> {
        &self.jurisdiction
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightsQuestionError {
    ResourceJurisdictionMismatch,
    SelfContainment,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RightsGrantId(NonZeroU64);

impl RightsGrantId {
    pub fn try_new(value: u64) -> Result<Self, RightsIdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(RightsIdentityError::ZeroGrantId)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RightsRevision(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RightsAttemptId([u8; 32]);

impl RightsAttemptId {
    pub fn try_new(bytes: [u8; 32]) -> Result<Self, RightsIdentityError> {
        if bytes == [0; 32] {
            Err(RightsIdentityError::ZeroAttemptId)
        } else {
            Ok(Self(bytes))
        }
    }

    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RightsRequestId([u8; 32]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RightsActionId([u8; 32]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RightsInputDigest([u8; 32]);

macro_rules! digest_identity {
    ($name:ident, $error:ident) => {
        impl $name {
            pub fn try_new(bytes: [u8; 32]) -> Result<Self, RightsIdentityError> {
                if bytes == [0; 32] {
                    Err(RightsIdentityError::$error)
                } else {
                    Ok(Self(bytes))
                }
            }

            pub const fn bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

digest_identity!(RightsRequestId, ZeroRequestId);
digest_identity!(RightsActionId, ZeroActionId);
digest_identity!(RightsInputDigest, ZeroInputDigest);

/// Immutable, server-authored action identity used by rights consumption.
///
/// The action planner will bridge its canonical request/action IDs and input
/// snapshot digest into this value after the parallel foundations are stacked.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RightsActionProvenance {
    request_id: RightsRequestId,
    action_id: RightsActionId,
    input_digest: RightsInputDigest,
}

impl RightsActionProvenance {
    pub const fn new(
        request_id: RightsRequestId,
        action_id: RightsActionId,
        input_digest: RightsInputDigest,
    ) -> Self {
        Self {
            request_id,
            action_id,
            input_digest,
        }
    }

    pub const fn request_id(&self) -> RightsRequestId {
        self.request_id
    }
    pub const fn action_id(&self) -> RightsActionId {
        self.action_id
    }
    pub const fn input_digest(&self) -> RightsInputDigest {
        self.input_digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightsIdentityError {
    ZeroGrantId,
    ZeroAttemptId,
    ZeroRequestId,
    ZeroActionId,
    ZeroInputDigest,
    InvalidValidityWindow,
    InconsistentLifecycle,
    LifecycleRevisionPrecedesGrant,
    ReceiptProvenanceMismatch,
}

impl fmt::Display for RightsIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroGrantId => "Rights grant identity must be nonzero",
            Self::ZeroAttemptId => "Rights attempt identity must be nonzero",
            Self::ZeroRequestId => "Rights request identity must be nonzero",
            Self::ZeroActionId => "Rights action identity must be nonzero",
            Self::ZeroInputDigest => "Rights input digest must be nonzero",
            Self::InvalidValidityWindow => "Rights validity must not end before it begins",
            Self::InconsistentLifecycle => "Reusable rights cannot carry consumption state",
            Self::LifecycleRevisionPrecedesGrant => {
                "Rights lifecycle revision must not precede the grant revision"
            }
            Self::ReceiptProvenanceMismatch => {
                "Consumption receipt must match the grant identity and revision"
            }
        })
    }
}

impl std::error::Error for RightsIdentityError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightsValidity {
    pub valid_from_minute: u64,
    pub valid_through_minute: Option<u64>,
}

impl RightsValidity {
    pub fn try_new(
        valid_from_minute: u64,
        valid_through_minute: Option<u64>,
    ) -> Result<Self, RightsIdentityError> {
        if valid_through_minute.is_some_and(|through| through < valid_from_minute) {
            return Err(RightsIdentityError::InvalidValidityWindow);
        }
        Ok(Self {
            valid_from_minute,
            valid_through_minute,
        })
    }

    pub fn contains(self, minute: u64) -> bool {
        minute >= self.valid_from_minute
            && self
                .valid_through_minute
                .is_none_or(|through| minute <= through)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightsGrantSource<S: DomainRightsSubject, G: DomainGrantSource> {
    Owner(RightsSubject<S>),
    JurisdictionAuthority(G),
    Organization(G),
    Domain(G),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightsGrantMode {
    Reusable,
    SingleUse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightsConsumptionProposal {
    grant_id: RightsGrantId,
    grant_revision: RightsRevision,
    attempt_id: RightsAttemptId,
    action_provenance: RightsActionProvenance,
}

impl RightsConsumptionProposal {
    pub const fn grant_id(&self) -> RightsGrantId {
        self.grant_id
    }
    pub const fn grant_revision(&self) -> RightsRevision {
        self.grant_revision
    }
    pub const fn attempt_id(&self) -> RightsAttemptId {
        self.attempt_id
    }
    pub const fn action_provenance(&self) -> RightsActionProvenance {
        self.action_provenance
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RightsConsumptionReceipt<C: DomainRightsCommitReceipt> {
    proposal: RightsConsumptionProposal,
    committed_outcome: C,
}

impl<C: DomainRightsCommitReceipt> RightsConsumptionReceipt<C> {
    pub fn new(proposal: RightsConsumptionProposal, committed_outcome: C) -> Self {
        Self {
            proposal,
            committed_outcome,
        }
    }

    pub const fn proposal(&self) -> &RightsConsumptionProposal {
        &self.proposal
    }
    pub const fn committed_outcome(&self) -> &C {
        &self.committed_outcome
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RightsGrantState<C: DomainRightsCommitReceipt> {
    Active,
    Revoked { at_revision: RightsRevision },
    Consumed(RightsConsumptionReceipt<C>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionGrant<
    S: DomainRightsSubject,
    R: DomainRightsResource,
    O: DomainRightsOperation,
    J: DomainJurisdiction,
    G: DomainGrantSource,
    C: DomainRightsCommitReceipt,
> {
    id: RightsGrantId,
    revision: RightsRevision,
    question: RightsQuestion<S, R, O, J>,
    source: RightsGrantSource<S, G>,
    validity: RightsValidity,
    mode: RightsGrantMode,
    state: RightsGrantState<C>,
}

impl<
    S: DomainRightsSubject,
    R: DomainRightsResource,
    O: DomainRightsOperation,
    J: DomainJurisdiction,
    G: DomainGrantSource,
    C: DomainRightsCommitReceipt,
> PermissionGrant<S, R, O, J, G, C>
{
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        id: RightsGrantId,
        revision: RightsRevision,
        question: RightsQuestion<S, R, O, J>,
        source: RightsGrantSource<S, G>,
        validity: RightsValidity,
        mode: RightsGrantMode,
        state: RightsGrantState<C>,
    ) -> Result<Self, RightsIdentityError> {
        if matches!(mode, RightsGrantMode::Reusable)
            && matches!(&state, RightsGrantState::Consumed(_))
        {
            return Err(RightsIdentityError::InconsistentLifecycle);
        }
        if let RightsGrantState::Revoked { at_revision } = &state
            && *at_revision < revision
        {
            return Err(RightsIdentityError::LifecycleRevisionPrecedesGrant);
        }
        if let RightsGrantState::Consumed(receipt) = &state
            && (receipt.proposal.grant_id != id || receipt.proposal.grant_revision != revision)
        {
            return Err(RightsIdentityError::ReceiptProvenanceMismatch);
        }
        Ok(Self {
            id,
            revision,
            question,
            source,
            validity,
            mode,
            state,
        })
    }

    pub const fn id(&self) -> RightsGrantId {
        self.id
    }
    pub const fn revision(&self) -> RightsRevision {
        self.revision
    }
    pub const fn question(&self) -> &RightsQuestion<S, R, O, J> {
        &self.question
    }
    pub const fn source(&self) -> &RightsGrantSource<S, G> {
        &self.source
    }
    pub const fn validity(&self) -> RightsValidity {
        self.validity
    }
    pub const fn mode(&self) -> RightsGrantMode {
        self.mode
    }
    pub const fn state(&self) -> &RightsGrantState<C> {
        &self.state
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrantMismatch {
    Subject,
    Resource,
    Operation,
    Jurisdiction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrantUnavailable {
    NotYetValid,
    Expired,
    Revoked,
    Consumed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumptionDecision {
    Reusable,
    Consume(RightsConsumptionProposal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightsProvenanceCollision {
    AttemptReusedForDifferentAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GrantAssessment<C: DomainRightsCommitReceipt> {
    Usable(ConsumptionDecision),
    IdempotentReplay(RightsConsumptionReceipt<C>),
    ProvenanceCollision(RightsProvenanceCollision),
    NotApplicable(GrantMismatch),
    Unavailable(GrantUnavailable),
}

pub fn assess_permission_grant<
    S: DomainRightsSubject,
    R: DomainRightsResource,
    O: DomainRightsOperation,
    J: DomainJurisdiction,
    G: DomainGrantSource,
    C: DomainRightsCommitReceipt,
>(
    question: &RightsQuestion<S, R, O, J>,
    grant: &PermissionGrant<S, R, O, J, G, C>,
    current_minute: u64,
    attempt_id: RightsAttemptId,
    action_provenance: RightsActionProvenance,
) -> GrantAssessment<C> {
    if question.subject != grant.question.subject {
        return GrantAssessment::NotApplicable(GrantMismatch::Subject);
    }
    if question.resource != grant.question.resource {
        return GrantAssessment::NotApplicable(GrantMismatch::Resource);
    }
    if question.operation != grant.question.operation {
        return GrantAssessment::NotApplicable(GrantMismatch::Operation);
    }
    if question.jurisdiction != grant.question.jurisdiction {
        return GrantAssessment::NotApplicable(GrantMismatch::Jurisdiction);
    }
    match &grant.state {
        RightsGrantState::Revoked { .. } => GrantAssessment::Unavailable(GrantUnavailable::Revoked),
        RightsGrantState::Consumed(receipt)
            if receipt.proposal.attempt_id == attempt_id
                && receipt.proposal.action_provenance == action_provenance =>
        {
            GrantAssessment::IdempotentReplay(receipt.clone())
        }
        RightsGrantState::Consumed(receipt) if receipt.proposal.attempt_id == attempt_id => {
            GrantAssessment::ProvenanceCollision(
                RightsProvenanceCollision::AttemptReusedForDifferentAction,
            )
        }
        RightsGrantState::Consumed(_) => GrantAssessment::Unavailable(GrantUnavailable::Consumed),
        RightsGrantState::Active => {
            if current_minute < grant.validity.valid_from_minute {
                return GrantAssessment::Unavailable(GrantUnavailable::NotYetValid);
            }
            if grant
                .validity
                .valid_through_minute
                .is_some_and(|through| current_minute > through)
            {
                return GrantAssessment::Unavailable(GrantUnavailable::Expired);
            }
            match grant.mode {
                RightsGrantMode::Reusable => GrantAssessment::Usable(ConsumptionDecision::Reusable),
                RightsGrantMode::SingleUse => GrantAssessment::Usable(
                    ConsumptionDecision::Consume(RightsConsumptionProposal {
                        grant_id: grant.id,
                        grant_revision: grant.revision,
                        attempt_id,
                        action_provenance,
                    }),
                ),
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipEvidence<S: DomainRightsSubject, R: DomainRightsResource> {
    pub owner: RightsSubject<S>,
    pub resource: RightsResource<R>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationPrivilegeEvidence<
    Organization: OrganizationIdentity,
    Role: OrganizationRole,
    Presentation: OrganizationPresentation,
    Recognition: OrganizationRecognition,
    J: DomainJurisdiction,
> {
    pub organization: Organization,
    pub role: Role,
    pub presentation: Presentation,
    pub recognition: Recognition,
    pub jurisdiction: RightsJurisdiction<J>,
    pub revision: RightsRevision,
}

impl<
    Organization: OrganizationIdentity,
    Role: OrganizationRole,
    Presentation: OrganizationPresentation,
    Recognition: OrganizationRecognition,
    J: DomainJurisdiction,
> OrganizationPrivilegeEvidenceValue
    for OrganizationPrivilegeEvidence<Organization, Role, Presentation, Recognition, J>
{
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateRightsEvidence<
    S: DomainRightsSubject,
    R: DomainRightsResource,
    P: OrganizationPrivilegeEvidenceValue,
    B: DomainObligationEvidence,
    E: DomainRightsEvidence,
> {
    Ownership(OwnershipEvidence<S, R>),
    OperationalCustody(ObjectCustody),
    PermissionGrant(RightsGrantId),
    OrganizationPrivilege(P),
    Obligation(B),
    Domain(E),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightsDecisionKind {
    Allowed,
    Denied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionProvenance {
    pub evidence_revision: RightsRevision,
    pub question_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateRightsDecision<E: Clone + fmt::Debug + Eq> {
    kind: RightsDecisionKind,
    evidence: Vec<E>,
    consumption: Option<ConsumptionDecision>,
    provenance: DecisionProvenance,
}

impl<E: Clone + fmt::Debug + Eq> PrivateRightsDecision<E> {
    pub fn allowed(
        evidence: Vec<E>,
        consumption: Option<ConsumptionDecision>,
        provenance: DecisionProvenance,
    ) -> Self {
        Self {
            kind: RightsDecisionKind::Allowed,
            evidence,
            consumption,
            provenance,
        }
    }

    pub fn denied(evidence: Vec<E>, provenance: DecisionProvenance) -> Self {
        Self {
            kind: RightsDecisionKind::Denied,
            evidence,
            consumption: None,
            provenance,
        }
    }

    pub const fn kind(&self) -> RightsDecisionKind {
        self.kind
    }
    pub fn evidence(&self) -> &[E] {
        &self.evidence
    }
    pub const fn consumption(&self) -> Option<ConsumptionDecision> {
        self.consumption
    }
    pub const fn provenance(&self) -> &DecisionProvenance {
        &self.provenance
    }

    /// Projects only the decision kind. Private evidence, grants, obligations,
    /// and provenance cannot influence or enter the denial presentation.
    pub fn sanitized<P: PublicRightsAllowance>(&self, allowed: P) -> PublicRightsDecision<P> {
        match self.kind {
            RightsDecisionKind::Allowed => PublicRightsDecision::Allowed(allowed),
            RightsDecisionKind::Denied => {
                PublicRightsDecision::Denied(PublicRightsRejection::Unavailable)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicRightsRejection {
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicRightsDecision<P: PublicRightsAllowance> {
    Allowed(P),
    Denied(PublicRightsRejection),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategic_place::{SettlementVenueKind, StrategicFixtureId, StrategicPlaceId};

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Subject {
        Household(u64),
    }
    impl DomainRightsSubject for Subject {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Resource {
        Corpse(u64),
        RepairOrder(u64),
        Residence(u64),
    }
    impl DomainRightsResource for Resource {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Operation {
        ExamineCorpse,
        Repair,
    }
    impl DomainRightsOperation for Operation {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Jurisdiction {
        Territory(u64),
    }
    impl DomainJurisdiction for Jurisdiction {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Source {
        Magistrate,
    }
    impl DomainGrantSource for Source {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Evidence {
        FamilyConsent,
    }
    impl DomainRightsEvidence for Evidence {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Obligation {
        ReturnRepair,
    }
    impl DomainObligationEvidence for Obligation {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct CommittedOutcome(u64);
    impl DomainRightsCommitReceipt for CommittedOutcome {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Organization(u64);
    impl OrganizationIdentity for Organization {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Role(u64);
    impl OrganizationRole for Role {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Presentation(u64);
    impl OrganizationPresentation for Presentation {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Recognition(u64);
    impl OrganizationRecognition for Recognition {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Allowance;
    impl PublicRightsAllowance for Allowance {}

    type Question = RightsQuestion<Subject, Resource, Operation, Jurisdiction>;
    type Grant =
        PermissionGrant<Subject, Resource, Operation, Jurisdiction, Source, CommittedOutcome>;
    type Privilege =
        OrganizationPrivilegeEvidence<Organization, Role, Presentation, Recognition, Jurisdiction>;

    fn actor() -> RightsSubject<Subject> {
        RightsSubject::Character(CustodyCharacterId::try_new(7).unwrap())
    }

    fn object() -> RightsResource<Resource> {
        RightsResource::Object(PhysicalObjectId::try_new(11).unwrap())
    }

    fn question(
        operation: RightsOperation<Operation>,
        jurisdiction: RightsJurisdiction<Jurisdiction>,
    ) -> Question {
        RightsQuestion::try_new(actor(), object(), operation, jurisdiction).unwrap()
    }

    fn attempt(byte: u8) -> RightsAttemptId {
        RightsAttemptId::try_new([byte; 32]).unwrap()
    }

    fn action_provenance(byte: u8) -> RightsActionProvenance {
        RightsActionProvenance::new(
            RightsRequestId::try_new([byte; 32]).unwrap(),
            RightsActionId::try_new([byte.wrapping_add(1); 32]).unwrap(),
            RightsInputDigest::try_new([byte.wrapping_add(2); 32]).unwrap(),
        )
    }

    fn assess(
        question: &Question,
        grant: &Grant,
        current_minute: u64,
        attempt_id: RightsAttemptId,
    ) -> GrantAssessment<CommittedOutcome> {
        let byte = attempt_id.bytes()[0];
        assess_permission_grant(
            question,
            grant,
            current_minute,
            attempt_id,
            action_provenance(byte),
        )
    }

    fn grant(
        question: Question,
        mode: RightsGrantMode,
        state: RightsGrantState<CommittedOutcome>,
    ) -> Grant {
        PermissionGrant::try_new(
            RightsGrantId::try_new(3).unwrap(),
            RightsRevision(9),
            question,
            RightsGrantSource::JurisdictionAuthority(Source::Magistrate),
            RightsValidity::try_new(100, Some(200)).unwrap(),
            mode,
            state,
        )
        .unwrap()
    }

    #[test]
    fn built_in_questions_reject_impossible_place_and_custody_combinations() {
        let inn = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let square =
            StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::PublicSquare)
                .unwrap();
        assert_eq!(
            RightsQuestion::try_new(
                actor(),
                RightsResource::<Resource>::Place(inn.clone()),
                RightsOperation::<Operation>::Access,
                RightsJurisdiction::<Jurisdiction>::Place(square.clone()),
            ),
            Err(RightsQuestionError::ResourceJurisdictionMismatch)
        );
        let fireplace = StrategicFixtureId::fireplace(inn.clone()).unwrap();
        assert_eq!(
            RightsQuestion::try_new(
                actor(),
                RightsResource::<Resource>::Fixture(fireplace.clone()),
                RightsOperation::<Operation>::Use,
                RightsJurisdiction::<Jurisdiction>::Place(square),
            ),
            Err(RightsQuestionError::ResourceJurisdictionMismatch)
        );
        assert!(
            RightsQuestion::try_new(
                actor(),
                RightsResource::<Resource>::Fixture(fireplace),
                RightsOperation::<Operation>::Use,
                RightsJurisdiction::<Jurisdiction>::Global,
            )
            .is_ok()
        );

        let object_id = PhysicalObjectId::try_new(11).unwrap();
        assert_eq!(
            RightsQuestion::try_new(
                actor(),
                RightsResource::<Resource>::Object(object_id),
                RightsOperation::<Operation>::TransferCustody {
                    destination: OperationalCustody::Container(object_id),
                },
                RightsJurisdiction::<Jurisdiction>::Global,
            ),
            Err(RightsQuestionError::SelfContainment)
        );
    }

    #[test]
    fn revocation_revision_cannot_precede_grant_revision() {
        let query = question(RightsOperation::Use, RightsJurisdiction::Global);
        let before: Result<Grant, _> = PermissionGrant::try_new(
            RightsGrantId::try_new(3).unwrap(),
            RightsRevision(9),
            query.clone(),
            RightsGrantSource::JurisdictionAuthority(Source::Magistrate),
            RightsValidity::try_new(100, Some(200)).unwrap(),
            RightsGrantMode::SingleUse,
            RightsGrantState::Revoked {
                at_revision: RightsRevision(8),
            },
        );
        assert_eq!(
            before,
            Err(RightsIdentityError::LifecycleRevisionPrecedesGrant)
        );
        let boundary: Result<Grant, _> = PermissionGrant::try_new(
            RightsGrantId::try_new(3).unwrap(),
            RightsRevision(9),
            query,
            RightsGrantSource::JurisdictionAuthority(Source::Magistrate),
            RightsValidity::try_new(100, Some(200)).unwrap(),
            RightsGrantMode::SingleUse,
            RightsGrantState::Revoked {
                at_revision: RightsRevision(9),
            },
        );
        assert!(boundary.is_ok());
    }

    #[test]
    fn ownership_custody_and_permission_can_conflict_without_collapsing() {
        let owner = RightsSubject::Character(CustodyCharacterId::try_new(8).unwrap());
        let ownership = OwnershipEvidence {
            owner: owner.clone(),
            resource: object(),
        };
        let custody = ObjectCustody::try_new(
            PhysicalObjectId::try_new(11).unwrap(),
            OperationalCustody::character(7).unwrap(),
        )
        .unwrap();
        assert_ne!(owner, actor());
        assert_eq!(
            custody.custody(),
            &OperationalCustody::character(7).unwrap()
        );
        assert_eq!(ownership.resource, object());

        let use_question = question(RightsOperation::Use, RightsJurisdiction::Global);
        assert!(matches!(
            assess(
                &use_question,
                &grant(
                    use_question.clone(),
                    RightsGrantMode::Reusable,
                    RightsGrantState::Active
                ),
                150,
                attempt(1),
            ),
            GrantAssessment::Usable(ConsumptionDecision::Reusable)
        ));
    }

    #[test]
    fn permission_for_use_is_not_ownership() {
        let use_question = question(RightsOperation::Use, RightsJurisdiction::Global);
        let own_question = question(RightsOperation::Own, RightsJurisdiction::Global);
        let permission = grant(
            use_question,
            RightsGrantMode::Reusable,
            RightsGrantState::Active,
        );
        assert_eq!(
            assess(&own_question, &permission, 150, attempt(1)),
            GrantAssessment::NotApplicable(GrantMismatch::Operation)
        );
    }

    #[test]
    fn jurisdiction_is_exact_and_local_equipment_differs_from_global_foraging() {
        let inn = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        let local_jurisdiction = RightsJurisdiction::Place(inn);
        let local = question(RightsOperation::Use, local_jurisdiction.clone());
        let global = question(RightsOperation::Use, RightsJurisdiction::Global);
        let local_grant = grant(local, RightsGrantMode::Reusable, RightsGrantState::Active);
        assert_eq!(
            assess(&global, &local_grant, 150, attempt(1)),
            GrantAssessment::NotApplicable(GrantMismatch::Jurisdiction)
        );
        let local_equipment_privilege = Privilege {
            organization: Organization(1),
            role: Role(2),
            presentation: Presentation(3),
            recognition: Recognition(4),
            jurisdiction: local_jurisdiction,
            revision: RightsRevision(5),
        };
        let global_foraging_privilege = Privilege {
            jurisdiction: RightsJurisdiction::Global,
            ..local_equipment_privilege.clone()
        };
        assert_ne!(local_equipment_privilege, global_foraging_privilege);
    }

    #[test]
    fn validity_revocation_and_consumption_fail_closed() {
        let query = question(RightsOperation::Use, RightsJurisdiction::Global);
        let active = grant(
            query.clone(),
            RightsGrantMode::SingleUse,
            RightsGrantState::Active,
        );
        assert_eq!(
            assess(&query, &active, 99, attempt(1)),
            GrantAssessment::Unavailable(GrantUnavailable::NotYetValid)
        );
        assert_eq!(
            assess(&query, &active, 201, attempt(1)),
            GrantAssessment::Unavailable(GrantUnavailable::Expired)
        );
        let revoked = grant(
            query.clone(),
            RightsGrantMode::SingleUse,
            RightsGrantState::Revoked {
                at_revision: RightsRevision(10),
            },
        );
        assert_eq!(
            assess(&query, &revoked, 150, attempt(1)),
            GrantAssessment::Unavailable(GrantUnavailable::Revoked)
        );
        let GrantAssessment::Usable(ConsumptionDecision::Consume(proposal)) =
            assess(&query, &active, 150, attempt(1))
        else {
            panic!()
        };
        let receipt = RightsConsumptionReceipt::new(proposal, CommittedOutcome(41));
        let consumed = grant(
            query.clone(),
            RightsGrantMode::SingleUse,
            RightsGrantState::Consumed(receipt),
        );
        assert_eq!(
            assess(&query, &consumed, 150, attempt(2)),
            GrantAssessment::Unavailable(GrantUnavailable::Consumed)
        );
    }

    #[test]
    fn consumption_proposal_and_replay_share_exact_provenance() {
        let query = question(RightsOperation::Alter, RightsJurisdiction::Global);
        let active = grant(
            query.clone(),
            RightsGrantMode::SingleUse,
            RightsGrantState::Active,
        );
        let GrantAssessment::Usable(ConsumptionDecision::Consume(proposal)) =
            assess(&query, &active, 150, attempt(4))
        else {
            panic!()
        };
        let receipt = RightsConsumptionReceipt::new(proposal, CommittedOutcome(77));
        let consumed = grant(
            query.clone(),
            RightsGrantMode::SingleUse,
            RightsGrantState::Consumed(receipt.clone()),
        );
        assert_eq!(
            assess(&query, &consumed, 150, attempt(4)),
            GrantAssessment::IdempotentReplay(receipt.clone())
        );
        assert_eq!(
            assess(&query, &consumed, 201, attempt(4)),
            GrantAssessment::IdempotentReplay(receipt.clone())
        );
        assert_eq!(receipt.committed_outcome(), &CommittedOutcome(77));
        assert_eq!(
            assess_permission_grant(&query, &consumed, 150, attempt(4), action_provenance(9),),
            GrantAssessment::ProvenanceCollision(
                RightsProvenanceCollision::AttemptReusedForDifferentAction
            )
        );
        assert_eq!(
            assess(&query, &consumed, 150, attempt(5)),
            GrantAssessment::Unavailable(GrantUnavailable::Consumed)
        );
    }

    #[test]
    fn hidden_evidence_cannot_change_denial_presentation() {
        let provenance = DecisionProvenance {
            evidence_revision: RightsRevision(4),
            question_digest: [4; 32],
        };
        let first =
            PrivateRightsDecision::denied(vec![Evidence::FamilyConsent], provenance.clone());
        let second = PrivateRightsDecision::denied(Vec::<Evidence>::new(), provenance);
        assert_eq!(first.consumption(), None);
        assert_eq!(first.sanitized(Allowance), second.sanitized(Allowance));
        assert_eq!(
            first.sanitized(Allowance),
            PublicRightsDecision::Denied(PublicRightsRejection::Unavailable)
        );
    }

    #[test]
    fn domain_resources_and_obligations_remain_distinct_typed_evidence() {
        let corpse = RightsResource::Domain(Resource::Corpse(5));
        let repair = RightsResource::Domain(Resource::RepairOrder(5));
        let residence = RightsResource::Domain(Resource::Residence(5));
        assert_ne!(corpse, repair);
        assert_ne!(repair, residence);

        let corpse_question: Question = RightsQuestion::try_new(
            RightsSubject::Domain(Subject::Household(2)),
            corpse,
            RightsOperation::Domain(Operation::ExamineCorpse),
            RightsJurisdiction::Domain(Jurisdiction::Territory(4)),
        )
        .unwrap();
        let repair_question: Question = RightsQuestion::try_new(
            actor(),
            repair,
            RightsOperation::Domain(Operation::Repair),
            RightsJurisdiction::Global,
        )
        .unwrap();
        assert_ne!(corpse_question, repair_question);

        type PrivateEvidence =
            PrivateRightsEvidence<Subject, Resource, Privilege, Obligation, Evidence>;
        let obligation = PrivateEvidence::Obligation(Obligation::ReturnRepair);
        let permission = PrivateEvidence::Domain(Evidence::FamilyConsent);
        assert_ne!(obligation, permission);
    }
}
