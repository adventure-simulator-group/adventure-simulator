//! Pure observer-specific knowledge and provenance contracts.
//!
//! Canonical truth is deliberately separate from observer records. This module
//! grants no disclosure authority, performs no inference, and has no wire or
//! persistence format.

use adventuresim_world_schema::UnitBasisPoints;
use std::{fmt, num::NonZeroU64};

use crate::{
    physical_object::{CustodyCharacterId, PhysicalObjectId},
    strategic_place::{StrategicFixtureId, StrategicPlaceId},
};

pub trait DomainKnowledgeSubject: Clone + fmt::Debug + Eq {}
pub trait DomainProposition: Clone + fmt::Debug + Eq {}
pub trait DomainKnowledgeSource: Clone + fmt::Debug + Eq {}
pub trait DomainVisibilityRule: Clone + fmt::Debug + Eq {}
pub trait DomainBelief: Clone + fmt::Debug + Eq {}
pub trait DomainTruth: Clone + fmt::Debug + Eq {}
pub trait DomainContradiction: Clone + fmt::Debug + Eq {}
pub trait PublicKnowledgePresentation: Clone + fmt::Debug + Eq {}
pub trait KnowledgeMutationInput: Clone + fmt::Debug + Eq {}
pub trait KnowledgeMutationOutcome: Clone + fmt::Debug + Eq {}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeRecordId(NonZeroU64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SharingReceiptId(NonZeroU64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContradictionId(NonZeroU64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeMutationRequestId(NonZeroU64);

macro_rules! nonzero_id {
    ($name:ident, $error:ident) => {
        impl $name {
            pub fn try_new(value: u64) -> Result<Self, KnowledgeError> {
                NonZeroU64::new(value)
                    .map(Self)
                    .ok_or(KnowledgeError::$error)
            }

            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
    };
}

nonzero_id!(KnowledgeRecordId, ZeroRecordId);
nonzero_id!(SharingReceiptId, ZeroSharingReceiptId);
nonzero_id!(ContradictionId, ZeroContradictionId);
nonzero_id!(KnowledgeMutationRequestId, ZeroMutationRequestId);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KnowledgeRevision(NonZeroU64);

impl KnowledgeRevision {
    pub fn try_new(value: u64) -> Result<Self, KnowledgeError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(KnowledgeError::ZeroRevision)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeConfidence(UnitBasisPoints);

impl KnowledgeConfidence {
    pub fn try_new(basis_points: u16) -> Result<Self, KnowledgeError> {
        UnitBasisPoints::new(basis_points)
            .map(Self)
            .ok_or(KnowledgeError::ConfidenceOutOfRange)
    }

    pub const fn basis_points(self) -> u16 {
        self.0.get()
    }

    pub const fn public_band(self) -> ConfidenceBand {
        match self.0.get() {
            0..=3_333 => ConfidenceBand::Weak,
            3_334..=6_666 => ConfidenceBand::Plausible,
            _ => ConfidenceBand::Strong,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfidenceBand {
    Weak,
    Plausible,
    Strong,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeSubject<S: DomainKnowledgeSubject> {
    Character(CustodyCharacterId),
    Object(PhysicalObjectId),
    Place(StrategicPlaceId),
    Fixture(StrategicFixtureId),
    Domain(S),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeSource<
    R: DomainKnowledgeSource,
    S: DomainKnowledgeSubject,
    P: DomainProposition,
    V: DomainVisibilityRule,
> {
    DirectObservation(R),
    Character(CustodyCharacterId),
    Object(PhysicalObjectId),
    Shared(SharedProvenance<S, P, V>),
    Domain(R),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedProvenance<
    S: DomainKnowledgeSubject,
    P: DomainProposition,
    V: DomainVisibilityRule,
> {
    receipt_id: SharingReceiptId,
    source_record_id: KnowledgeRecordId,
    source_revision: KnowledgeRevision,
    sharer: CustodyCharacterId,
    recipient: CustodyCharacterId,
    subject: KnowledgeSubject<S>,
    proposition: P,
    shared_minute: u64,
    rule: V,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeVisibility<V: DomainVisibilityRule> {
    ObserverPrivate,
    Shareable(V),
    PublicDisclosure(V),
    Domain(V),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeLineage {
    revision: KnowledgeRevision,
    supersedes: Option<KnowledgeRecordId>,
}

impl KnowledgeLineage {
    pub fn try_new(
        revision: KnowledgeRevision,
        supersedes: Option<KnowledgeRecordId>,
    ) -> Result<Self, KnowledgeError> {
        match (revision.get(), supersedes) {
            (1, None) | (2.., Some(_)) => Ok(Self {
                revision,
                supersedes,
            }),
            _ => Err(KnowledgeError::InvalidLineage),
        }
    }

    pub const fn revision(self) -> KnowledgeRevision {
        self.revision
    }
    pub const fn supersedes(self) -> Option<KnowledgeRecordId> {
        self.supersedes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeEnvelope<
    S: DomainKnowledgeSubject,
    P: DomainProposition,
    R: DomainKnowledgeSource,
    V: DomainVisibilityRule,
> {
    record_id: KnowledgeRecordId,
    observer: CustodyCharacterId,
    subject: KnowledgeSubject<S>,
    proposition: P,
    source: KnowledgeSource<R, S, P, V>,
    source_minute: u64,
    learned_minute: u64,
    confidence: KnowledgeConfidence,
    visibility: KnowledgeVisibility<V>,
    lineage: KnowledgeLineage,
}

impl<
    S: DomainKnowledgeSubject,
    P: DomainProposition,
    R: DomainKnowledgeSource,
    V: DomainVisibilityRule,
> KnowledgeEnvelope<S, P, R, V>
{
    #[expect(
        clippy::too_many_arguments,
        reason = "this domain boundary names each independent input explicitly"
    )]
    pub fn try_new(
        record_id: KnowledgeRecordId,
        observer: CustodyCharacterId,
        subject: KnowledgeSubject<S>,
        proposition: P,
        source: KnowledgeSource<R, S, P, V>,
        source_minute: u64,
        learned_minute: u64,
        observer_personal_minute: u64,
        confidence: KnowledgeConfidence,
        visibility: KnowledgeVisibility<V>,
        lineage: KnowledgeLineage,
    ) -> Result<Self, KnowledgeError> {
        if let KnowledgeSource::Shared(provenance) = &source {
            if observer != provenance.recipient
                || subject != provenance.subject
                || proposition != provenance.proposition
                || source_minute != provenance.shared_minute
            {
                return Err(KnowledgeError::InvalidSharedProvenance);
            }
            if !matches!(&visibility, KnowledgeVisibility::ObserverPrivate) {
                return Err(KnowledgeError::InvalidSharedVisibility);
            }
        }
        if source_minute > learned_minute {
            return Err(KnowledgeError::SourceAfterLearning);
        }
        if learned_minute > observer_personal_minute {
            return Err(KnowledgeError::BeyondObserverTime);
        }
        Ok(Self {
            record_id,
            observer,
            subject,
            proposition,
            source,
            source_minute,
            learned_minute,
            confidence,
            visibility,
            lineage,
        })
    }

    pub const fn record_id(&self) -> KnowledgeRecordId {
        self.record_id
    }
    pub const fn observer(&self) -> CustodyCharacterId {
        self.observer
    }
    pub const fn subject(&self) -> &KnowledgeSubject<S> {
        &self.subject
    }
    pub const fn proposition(&self) -> &P {
        &self.proposition
    }
    pub const fn source(&self) -> &KnowledgeSource<R, S, P, V> {
        &self.source
    }
    pub const fn source_minute(&self) -> u64 {
        self.source_minute
    }
    pub const fn learned_minute(&self) -> u64 {
        self.learned_minute
    }
    pub const fn confidence(&self) -> KnowledgeConfidence {
        self.confidence
    }
    pub const fn visibility(&self) -> &KnowledgeVisibility<V> {
        &self.visibility
    }
    pub const fn lineage(&self) -> KnowledgeLineage {
        self.lineage
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObserverKnowledgeRecord<
    S: DomainKnowledgeSubject,
    P: DomainProposition,
    R: DomainKnowledgeSource,
    V: DomainVisibilityRule,
    B: DomainBelief,
> {
    envelope: KnowledgeEnvelope<S, P, R, V>,
    belief: B,
}

impl<
    S: DomainKnowledgeSubject,
    P: DomainProposition,
    R: DomainKnowledgeSource,
    V: DomainVisibilityRule,
    B: DomainBelief,
> ObserverKnowledgeRecord<S, P, R, V, B>
{
    pub fn new(envelope: KnowledgeEnvelope<S, P, R, V>, belief: B) -> Self {
        Self { envelope, belief }
    }

    pub const fn envelope(&self) -> &KnowledgeEnvelope<S, P, R, V> {
        &self.envelope
    }
    pub const fn belief(&self) -> &B {
        &self.belief
    }

    pub fn project<T: PublicKnowledgePresentation>(
        &self,
        grant: &ProjectionGrant<V>,
        presentation: T,
    ) -> Result<KnownFactProjection<T>, ProjectionRejection> {
        let is_observer = grant.viewer == self.envelope.observer;
        let visible = if is_observer {
            matches!(grant.scope, ProjectionScope::Observer)
        } else {
            matches!(
                (&self.envelope.visibility, &grant.scope),
                (
                    KnowledgeVisibility::PublicDisclosure(expected),
                    ProjectionScope::PublicDisclosure(actual)
                ) if expected == actual
            )
        };
        if !visible {
            return Err(ProjectionRejection::NotVisible);
        }
        if self.envelope.learned_minute > grant.viewer_personal_minute {
            return Err(ProjectionRejection::BeyondViewerTime);
        }
        Ok(KnownFactProjection {
            reference: is_observer.then_some(KnownFactReference {
                observer: self.envelope.observer,
                record_id: self.envelope.record_id,
                revision: self.envelope.lineage.revision,
            }),
            learned_minute: self.envelope.learned_minute,
            confidence: self.envelope.confidence.public_band(),
            presentation,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProjectionScope<V: DomainVisibilityRule> {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "only trusted test fixtures currently mint observer projection grants"
        )
    )]
    Observer,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "only trusted test fixtures currently mint public-disclosure grants"
        )
    )]
    PublicDisclosure(V),
}

/// Opaque proof that trusted server code authenticated the viewer, personal
/// time frontier, and disclosure scope. Public callers cannot mint a grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionGrant<V: DomainVisibilityRule> {
    viewer: CustodyCharacterId,
    viewer_personal_minute: u64,
    scope: ProjectionScope<V>,
}

impl<V: DomainVisibilityRule> ProjectionGrant<V> {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the trusted observer-grant constructor is currently exercised by boundary tests"
        )
    )]
    pub(crate) fn for_authenticated_observer(
        viewer: CustodyCharacterId,
        viewer_personal_minute: u64,
    ) -> Self {
        Self {
            viewer,
            viewer_personal_minute,
            scope: ProjectionScope::Observer,
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the trusted disclosure-grant constructor is currently exercised by boundary tests"
        )
    )]
    pub(crate) fn for_authenticated_public_disclosure(
        viewer: CustodyCharacterId,
        viewer_personal_minute: u64,
        rule: V,
    ) -> Self {
        Self {
            viewer,
            viewer_personal_minute,
            scope: ProjectionScope::PublicDisclosure(rule),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionRejection {
    NotVisible,
    BeyondViewerTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnownFactProjection<T: PublicKnowledgePresentation> {
    pub reference: Option<KnownFactReference>,
    pub learned_minute: u64,
    pub confidence: ConfidenceBand,
    pub presentation: T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnownFactReference {
    pub observer: CustodyCharacterId,
    pub record_id: KnowledgeRecordId,
    pub revision: KnowledgeRevision,
}

/// Private canonical state. There is intentionally no conversion from truth to
/// observer knowledge or public presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoritativeTruth<S: DomainKnowledgeSubject, P: DomainProposition, T: DomainTruth> {
    subject: KnowledgeSubject<S>,
    proposition: P,
    truth: T,
    revision: KnowledgeRevision,
}

impl<S: DomainKnowledgeSubject, P: DomainProposition, T: DomainTruth> AuthoritativeTruth<S, P, T> {
    pub fn new(
        subject: KnowledgeSubject<S>,
        proposition: P,
        truth: T,
        revision: KnowledgeRevision,
    ) -> Self {
        Self {
            subject,
            proposition,
            truth,
            revision,
        }
    }

    pub const fn subject(&self) -> &KnowledgeSubject<S> {
        &self.subject
    }
    pub const fn proposition(&self) -> &P {
        &self.proposition
    }
    pub const fn private_truth(&self) -> &T {
        &self.truth
    }
    pub const fn revision(&self) -> KnowledgeRevision {
        self.revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupersessionRejection {
    RecordIdentityReused,
    ObserverMismatch,
    SubjectMismatch,
    PropositionMismatch,
    WrongPredecessor,
    NonMonotonicRevision,
    ChronologyRegression,
}

pub fn validate_supersession<
    S: DomainKnowledgeSubject,
    P: DomainProposition,
    R: DomainKnowledgeSource,
    V: DomainVisibilityRule,
>(
    prior: &KnowledgeEnvelope<S, P, R, V>,
    next: &KnowledgeEnvelope<S, P, R, V>,
) -> Result<(), SupersessionRejection> {
    if prior.record_id == next.record_id {
        return Err(SupersessionRejection::RecordIdentityReused);
    }
    if prior.observer != next.observer {
        return Err(SupersessionRejection::ObserverMismatch);
    }
    if prior.subject != next.subject {
        return Err(SupersessionRejection::SubjectMismatch);
    }
    if prior.proposition != next.proposition {
        return Err(SupersessionRejection::PropositionMismatch);
    }
    if next.lineage.supersedes != Some(prior.record_id) {
        return Err(SupersessionRejection::WrongPredecessor);
    }
    if prior.lineage.revision.get().checked_add(1) != Some(next.lineage.revision.get()) {
        return Err(SupersessionRejection::NonMonotonicRevision);
    }
    if next.learned_minute < prior.learned_minute {
        return Err(SupersessionRejection::ChronologyRegression);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharingReceipt<S: DomainKnowledgeSubject, P: DomainProposition, V: DomainVisibilityRule>
{
    id: SharingReceiptId,
    source_record_id: KnowledgeRecordId,
    source_revision: KnowledgeRevision,
    sharer: CustodyCharacterId,
    recipient: CustodyCharacterId,
    subject: KnowledgeSubject<S>,
    proposition: P,
    shared_minute: u64,
    rule: V,
}

impl<S: DomainKnowledgeSubject, P: DomainProposition, V: DomainVisibilityRule>
    SharingReceipt<S, P, V>
{
    pub fn try_new<R: DomainKnowledgeSource>(
        id: SharingReceiptId,
        source: &KnowledgeEnvelope<S, P, R, V>,
        recipient: CustodyCharacterId,
        shared_minute: u64,
        sharer_personal_minute: u64,
    ) -> Result<Self, KnowledgeError> {
        let KnowledgeVisibility::Shareable(rule) = &source.visibility else {
            return Err(KnowledgeError::NotShareable);
        };
        if recipient == source.observer {
            return Err(KnowledgeError::SelfSharing);
        }
        if shared_minute < source.learned_minute || shared_minute > sharer_personal_minute {
            return Err(KnowledgeError::InvalidSharingChronology);
        }
        Ok(Self {
            id,
            source_record_id: source.record_id,
            source_revision: source.lineage.revision,
            sharer: source.observer,
            recipient,
            subject: source.subject.clone(),
            proposition: source.proposition.clone(),
            shared_minute,
            rule: rule.clone(),
        })
    }

    pub const fn id(&self) -> SharingReceiptId {
        self.id
    }
    pub const fn source_record_id(&self) -> KnowledgeRecordId {
        self.source_record_id
    }
    pub const fn source_revision(&self) -> KnowledgeRevision {
        self.source_revision
    }
    pub const fn sharer(&self) -> CustodyCharacterId {
        self.sharer
    }
    pub const fn recipient(&self) -> CustodyCharacterId {
        self.recipient
    }
    pub const fn shared_minute(&self) -> u64 {
        self.shared_minute
    }
    pub const fn rule(&self) -> &V {
        &self.rule
    }

    pub fn try_recipient_envelope<R: DomainKnowledgeSource>(
        &self,
        record_id: KnowledgeRecordId,
        learned_minute: u64,
        recipient_personal_minute: u64,
        confidence: KnowledgeConfidence,
        lineage: KnowledgeLineage,
    ) -> Result<KnowledgeEnvelope<S, P, R, V>, KnowledgeError> {
        let provenance = SharedProvenance {
            receipt_id: self.id,
            source_record_id: self.source_record_id,
            source_revision: self.source_revision,
            sharer: self.sharer,
            recipient: self.recipient,
            subject: self.subject.clone(),
            proposition: self.proposition.clone(),
            shared_minute: self.shared_minute,
            rule: self.rule.clone(),
        };
        KnowledgeEnvelope::try_new(
            record_id,
            self.recipient,
            self.subject.clone(),
            self.proposition.clone(),
            KnowledgeSource::Shared(provenance),
            self.shared_minute,
            learned_minute,
            recipient_personal_minute,
            confidence,
            KnowledgeVisibility::ObserverPrivate,
            lineage,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContradictionRecord<C: DomainContradiction> {
    id: ContradictionId,
    observer: CustodyCharacterId,
    left: (KnowledgeRecordId, KnowledgeRevision),
    right: (KnowledgeRecordId, KnowledgeRevision),
    recorded_minute: u64,
    rationale: C,
}

impl<C: DomainContradiction> ContradictionRecord<C> {
    pub fn try_new<
        S1: DomainKnowledgeSubject,
        P1: DomainProposition,
        R1: DomainKnowledgeSource,
        V1: DomainVisibilityRule,
        S2: DomainKnowledgeSubject,
        P2: DomainProposition,
        R2: DomainKnowledgeSource,
        V2: DomainVisibilityRule,
    >(
        id: ContradictionId,
        left: &KnowledgeEnvelope<S1, P1, R1, V1>,
        right: &KnowledgeEnvelope<S2, P2, R2, V2>,
        recorded_minute: u64,
        observer_personal_minute: u64,
        rationale: C,
    ) -> Result<Self, KnowledgeError> {
        if left.record_id == right.record_id {
            return Err(KnowledgeError::SelfContradiction);
        }
        if left.observer != right.observer {
            return Err(KnowledgeError::ContradictionObserverMismatch);
        }
        if recorded_minute < left.learned_minute.max(right.learned_minute)
            || recorded_minute > observer_personal_minute
        {
            return Err(KnowledgeError::InvalidContradictionChronology);
        }
        Ok(Self {
            id,
            observer: left.observer,
            left: (left.record_id, left.lineage.revision),
            right: (right.record_id, right.lineage.revision),
            recorded_minute,
            rationale,
        })
    }

    pub const fn left(&self) -> (KnowledgeRecordId, KnowledgeRevision) {
        self.left
    }
    pub const fn right(&self) -> (KnowledgeRecordId, KnowledgeRevision) {
        self.right
    }
    pub const fn observer(&self) -> CustodyCharacterId {
        self.observer
    }
    pub const fn id(&self) -> ContradictionId {
        self.id
    }
    pub const fn recorded_minute(&self) -> u64 {
        self.recorded_minute
    }
    pub const fn rationale(&self) -> &C {
        &self.rationale
    }
}

/// Durable server-owned idempotency record for any typed knowledge mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeMutationReceipt<I: KnowledgeMutationInput, O: KnowledgeMutationOutcome> {
    request_id: KnowledgeMutationRequestId,
    exact_input: I,
    committed_outcome: O,
}

impl<I: KnowledgeMutationInput, O: KnowledgeMutationOutcome> KnowledgeMutationReceipt<I, O> {
    pub fn committed(
        request_id: KnowledgeMutationRequestId,
        exact_input: I,
        committed_outcome: O,
    ) -> Self {
        Self {
            request_id,
            exact_input,
            committed_outcome,
        }
    }

    pub const fn request_id(&self) -> KnowledgeMutationRequestId {
        self.request_id
    }
    pub const fn exact_input(&self) -> &I {
        &self.exact_input
    }
    pub const fn committed_outcome(&self) -> &O {
        &self.committed_outcome
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeMutationDecision<'a, O: KnowledgeMutationOutcome> {
    Apply,
    Replay(&'a O),
    Collision,
    AmbiguousPriorReceipt,
}

/// Classifies a request before any domain mutation is applied. `Replay` and
/// `Collision` are terminal top-level outcomes; only `Apply` may execute.
pub fn classify_knowledge_mutation<'a, I, O>(
    receipts: impl IntoIterator<Item = &'a KnowledgeMutationReceipt<I, O>>,
    request_id: KnowledgeMutationRequestId,
    exact_input: &I,
) -> KnowledgeMutationDecision<'a, O>
where
    I: KnowledgeMutationInput + 'a,
    O: KnowledgeMutationOutcome + 'a,
{
    let mut matching = receipts
        .into_iter()
        .filter(|receipt| receipt.request_id == request_id);
    let Some(receipt) = matching.next() else {
        return KnowledgeMutationDecision::Apply;
    };
    if matching.next().is_some() {
        return KnowledgeMutationDecision::AmbiguousPriorReceipt;
    }
    if &receipt.exact_input == exact_input {
        KnowledgeMutationDecision::Replay(&receipt.committed_outcome)
    } else {
        KnowledgeMutationDecision::Collision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeError {
    ZeroRecordId,
    ZeroSharingReceiptId,
    ZeroContradictionId,
    ZeroMutationRequestId,
    ZeroRevision,
    ConfidenceOutOfRange,
    InvalidLineage,
    SourceAfterLearning,
    BeyondObserverTime,
    InvalidSharedProvenance,
    InvalidSharedVisibility,
    NotShareable,
    SelfSharing,
    InvalidSharingChronology,
    SelfContradiction,
    ContradictionObserverMismatch,
    InvalidContradictionChronology,
}

impl fmt::Display for KnowledgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroRecordId => "Knowledge record identity must be nonzero",
            Self::ZeroSharingReceiptId => "Knowledge sharing receipt identity must be nonzero",
            Self::ZeroContradictionId => "Knowledge contradiction identity must be nonzero",
            Self::ZeroMutationRequestId => "Knowledge mutation request identity must be nonzero",
            Self::ZeroRevision => "Knowledge revision must be nonzero",
            Self::ConfidenceOutOfRange => "Knowledge confidence must be at most 10000 basis points",
            Self::InvalidLineage => {
                "Knowledge lineage must begin at one and name every predecessor"
            }
            Self::SourceAfterLearning => "Knowledge source minute cannot follow learning",
            Self::BeyondObserverTime => "Knowledge cannot exceed the observer personal time",
            Self::InvalidSharedProvenance => {
                "Shared knowledge must retain its exact fact, recipient, and sharing minute"
            }
            Self::InvalidSharedVisibility => {
                "Newly shared knowledge must begin as observer-private"
            }
            Self::NotShareable => "Knowledge visibility does not permit sharing",
            Self::SelfSharing => "Knowledge sharing requires a distinct recipient",
            Self::InvalidSharingChronology => "Knowledge sharing chronology is invalid",
            Self::SelfContradiction => "A knowledge record cannot contradict itself",
            Self::ContradictionObserverMismatch => {
                "Contradictions must link records owned by the same observer"
            }
            Self::InvalidContradictionChronology => "Knowledge contradiction chronology is invalid",
        })
    }
}

impl std::error::Error for KnowledgeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Subject {
        Outbreak(u64),
        Creature(u64),
    }
    impl DomainKnowledgeSubject for Subject {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Proposition {
        SourceLocation,
        CreatureWeakness,
    }
    impl DomainProposition for Proposition {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Source {
        Testimony(u64),
        Inspection(u64),
    }
    impl DomainKnowledgeSource for Source {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Visibility {
        Dialogue,
        ThreatNotice,
    }
    impl DomainVisibilityRule for Visibility {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Belief {
        Place(&'static str),
        Weakness(&'static str),
    }
    impl DomainBelief for Belief {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Truth {
        ActualPlace(&'static str),
        ActualWeakness(&'static str),
    }
    impl DomainTruth for Truth {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Contradiction {
        MutuallyExclusiveLocations,
    }
    impl DomainContradiction for Contradiction {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Presentation(&'static str);
    impl PublicKnowledgePresentation for Presentation {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum MutationInput {
        Record(KnowledgeRecordId),
        Share(SharingReceiptId),
        Contradiction(ContradictionId),
    }
    impl KnowledgeMutationInput for MutationInput {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum MutationOutcome {
        Record(KnowledgeRecordId),
        Share(SharingReceiptId),
        Contradiction(ContradictionId),
    }
    impl KnowledgeMutationOutcome for MutationOutcome {}

    type Envelope = KnowledgeEnvelope<Subject, Proposition, Source, Visibility>;
    type Record = ObserverKnowledgeRecord<Subject, Proposition, Source, Visibility, Belief>;

    fn observer(id: u64) -> CustodyCharacterId {
        CustodyCharacterId::try_new(id).unwrap()
    }

    fn observer_grant(id: u64, minute: u64) -> ProjectionGrant<Visibility> {
        ProjectionGrant::for_authenticated_observer(observer(id), minute)
    }

    fn public_grant(id: u64, minute: u64, rule: Visibility) -> ProjectionGrant<Visibility> {
        ProjectionGrant::for_authenticated_public_disclosure(observer(id), minute, rule)
    }

    fn envelope(
        id: u64,
        owner: u64,
        learned: u64,
        visibility: KnowledgeVisibility<Visibility>,
        revision: u64,
        supersedes: Option<u64>,
    ) -> Envelope {
        KnowledgeEnvelope::try_new(
            KnowledgeRecordId::try_new(id).unwrap(),
            observer(owner),
            KnowledgeSubject::Domain(Subject::Outbreak(4)),
            Proposition::SourceLocation,
            KnowledgeSource::DirectObservation(Source::Testimony(8)),
            learned - 1,
            learned,
            learned,
            KnowledgeConfidence::try_new(6_000).unwrap(),
            visibility,
            KnowledgeLineage::try_new(
                KnowledgeRevision::try_new(revision).unwrap(),
                supersedes.map(|value| KnowledgeRecordId::try_new(value).unwrap()),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn observer_isolation_and_personal_time_fail_closed() {
        let record = Record::new(
            envelope(1, 7, 100, KnowledgeVisibility::ObserverPrivate, 1, None),
            Belief::Place("the mill"),
        );
        assert_eq!(
            record.project(&observer_grant(8, 99), Presentation("mill")),
            Err(ProjectionRejection::NotVisible)
        );
        assert_eq!(
            record.project(&observer_grant(7, 99), Presentation("mill")),
            Err(ProjectionRejection::BeyondViewerTime)
        );
        let owner_projection = record
            .project(&observer_grant(7, 100), Presentation("mill"))
            .unwrap();
        assert_eq!(
            owner_projection.reference,
            Some(KnownFactReference {
                observer: observer(7),
                record_id: KnowledgeRecordId::try_new(1).unwrap(),
                revision: KnowledgeRevision::try_new(1).unwrap(),
            })
        );
    }

    #[test]
    fn public_disclosure_requires_exact_typed_authority() {
        let record = Record::new(
            envelope(
                1,
                7,
                100,
                KnowledgeVisibility::PublicDisclosure(Visibility::ThreatNotice),
                1,
                None,
            ),
            Belief::Place("near the walls"),
        );
        assert_eq!(
            record.project(
                &public_grant(8, 99, Visibility::Dialogue),
                Presentation("nearby"),
            ),
            Err(ProjectionRejection::NotVisible)
        );
        let public_projection = record
            .project(
                &public_grant(8, 100, Visibility::ThreatNotice),
                Presentation("nearby"),
            )
            .unwrap();
        assert_eq!(public_projection.reference, None);
    }

    #[test]
    fn creation_rejects_future_and_reversed_chronology() {
        let base = || {
            (
                KnowledgeRecordId::try_new(1).unwrap(),
                observer(7),
                KnowledgeSubject::Domain(Subject::Outbreak(4)),
                Proposition::SourceLocation,
                KnowledgeSource::DirectObservation(Source::Testimony(8)),
                KnowledgeConfidence::try_new(5_000).unwrap(),
                KnowledgeVisibility::<Visibility>::ObserverPrivate,
                KnowledgeLineage::try_new(KnowledgeRevision::try_new(1).unwrap(), None).unwrap(),
            )
        };
        let (id, owner, subject, proposition, source, confidence, visibility, lineage) = base();
        assert_eq!(
            KnowledgeEnvelope::try_new(
                id,
                owner,
                subject,
                proposition,
                source,
                101,
                100,
                100,
                confidence,
                visibility,
                lineage,
            ),
            Err(KnowledgeError::SourceAfterLearning)
        );
        let (id, owner, subject, proposition, source, confidence, visibility, lineage) = base();
        assert_eq!(
            KnowledgeEnvelope::try_new(
                id,
                owner,
                subject,
                proposition,
                source,
                99,
                101,
                100,
                confidence,
                visibility,
                lineage,
            ),
            Err(KnowledgeError::BeyondObserverTime)
        );
    }

    #[test]
    fn supersession_is_exact_monotonic_and_keeps_prior_immutable() {
        let prior = envelope(1, 7, 100, KnowledgeVisibility::ObserverPrivate, 1, None);
        let next = envelope(2, 7, 120, KnowledgeVisibility::ObserverPrivate, 2, Some(1));
        assert_eq!(validate_supersession(&prior, &next), Ok(()));
        assert_eq!(prior.record_id(), KnowledgeRecordId::try_new(1).unwrap());

        let reused_identity = envelope(1, 7, 120, KnowledgeVisibility::ObserverPrivate, 2, Some(1));
        assert_eq!(
            validate_supersession(&prior, &reused_identity),
            Err(SupersessionRejection::RecordIdentityReused)
        );

        let skipped = envelope(3, 7, 130, KnowledgeVisibility::ObserverPrivate, 3, Some(1));
        assert_eq!(
            validate_supersession(&prior, &skipped),
            Err(SupersessionRejection::NonMonotonicRevision)
        );

        let maximum = envelope(
            4,
            7,
            140,
            KnowledgeVisibility::ObserverPrivate,
            u64::MAX,
            Some(3),
        );
        let overflow = envelope(
            5,
            7,
            150,
            KnowledgeVisibility::ObserverPrivate,
            u64::MAX,
            Some(4),
        );
        assert_eq!(
            validate_supersession(&maximum, &overflow),
            Err(SupersessionRejection::NonMonotonicRevision)
        );
    }

    #[test]
    fn contradiction_links_both_immutable_provenances() {
        let left = envelope(1, 7, 100, KnowledgeVisibility::ObserverPrivate, 1, None);
        let right = envelope(2, 7, 110, KnowledgeVisibility::ObserverPrivate, 1, None);
        let contradiction = ContradictionRecord::try_new(
            ContradictionId::try_new(1).unwrap(),
            &left,
            &right,
            115,
            115,
            Contradiction::MutuallyExclusiveLocations,
        )
        .unwrap();
        assert_eq!(
            contradiction.left(),
            (left.record_id(), left.lineage().revision())
        );
        assert_eq!(
            contradiction.right(),
            (right.record_id(), right.lineage().revision())
        );

        let other_observer = envelope(3, 8, 110, KnowledgeVisibility::ObserverPrivate, 1, None);
        assert_eq!(
            ContradictionRecord::try_new(
                ContradictionId::try_new(2).unwrap(),
                &left,
                &other_observer,
                115,
                115,
                Contradiction::MutuallyExclusiveLocations,
            ),
            Err(KnowledgeError::ContradictionObserverMismatch)
        );
    }

    #[test]
    fn sharing_creates_attribution_not_transferable_truth() {
        let source = envelope(
            1,
            7,
            100,
            KnowledgeVisibility::Shareable(Visibility::Dialogue),
            1,
            None,
        );
        let receipt = SharingReceipt::try_new(
            SharingReceiptId::try_new(4).unwrap(),
            &source,
            observer(8),
            110,
            110,
        )
        .unwrap();
        let recipient = receipt
            .try_recipient_envelope::<Source>(
                KnowledgeRecordId::try_new(2).unwrap(),
                115,
                115,
                KnowledgeConfidence::try_new(5_000).unwrap(),
                KnowledgeLineage::try_new(KnowledgeRevision::try_new(1).unwrap(), None).unwrap(),
            )
            .unwrap();
        assert_eq!(recipient.observer(), observer(8));
        assert_eq!(recipient.subject(), source.subject());
        assert_eq!(recipient.proposition(), source.proposition());
        assert_eq!(recipient.source_minute(), 110);
        assert_eq!(
            recipient.visibility(),
            &KnowledgeVisibility::ObserverPrivate
        );
        let KnowledgeSource::Shared(shared) = recipient.source() else {
            panic!("recipient source must retain opaque sharing provenance");
        };
        assert_eq!(shared.receipt_id, SharingReceiptId::try_new(4).unwrap());
        assert_eq!(shared.source_record_id, source.record_id());
        assert_eq!(shared.source_revision, source.lineage().revision());
        assert_eq!(shared.sharer, observer(7));
        assert_eq!(shared.recipient, observer(8));

        assert_eq!(
            KnowledgeEnvelope::try_new(
                KnowledgeRecordId::try_new(3).unwrap(),
                observer(9),
                recipient.subject().clone(),
                recipient.proposition().clone(),
                KnowledgeSource::<Source, Subject, Proposition, Visibility>::Shared(shared.clone(),),
                110,
                115,
                115,
                KnowledgeConfidence::try_new(5_000).unwrap(),
                KnowledgeVisibility::ObserverPrivate,
                KnowledgeLineage::try_new(KnowledgeRevision::try_new(1).unwrap(), None).unwrap(),
            ),
            Err(KnowledgeError::InvalidSharedProvenance)
        );
        for visibility in [
            KnowledgeVisibility::Shareable(Visibility::Dialogue),
            KnowledgeVisibility::PublicDisclosure(Visibility::ThreatNotice),
        ] {
            assert_eq!(
                KnowledgeEnvelope::try_new(
                    KnowledgeRecordId::try_new(4).unwrap(),
                    observer(8),
                    recipient.subject().clone(),
                    recipient.proposition().clone(),
                    KnowledgeSource::<Source, Subject, Proposition, Visibility>::Shared(
                        shared.clone(),
                    ),
                    110,
                    115,
                    115,
                    KnowledgeConfidence::try_new(5_000).unwrap(),
                    visibility,
                    KnowledgeLineage::try_new(KnowledgeRevision::try_new(1).unwrap(), None)
                        .unwrap(),
                ),
                Err(KnowledgeError::InvalidSharedVisibility)
            );
        }
        assert_eq!(receipt.recipient(), observer(8));

        let private = envelope(2, 7, 100, KnowledgeVisibility::ObserverPrivate, 1, None);
        assert_eq!(
            SharingReceipt::try_new(
                SharingReceiptId::try_new(5).unwrap(),
                &private,
                observer(8),
                110,
                110,
            ),
            Err(KnowledgeError::NotShareable)
        );
    }

    #[test]
    fn canonical_truth_and_hidden_changes_do_not_enter_belief_projection() {
        let envelope = envelope(1, 7, 100, KnowledgeVisibility::ObserverPrivate, 1, None);
        let record = Record::new(envelope.clone(), Belief::Place("the mill"));
        let first = AuthoritativeTruth::new(
            envelope.subject().clone(),
            envelope.proposition().clone(),
            Truth::ActualPlace("the mill"),
            KnowledgeRevision::try_new(1).unwrap(),
        );
        let second = AuthoritativeTruth::new(
            envelope.subject().clone(),
            envelope.proposition().clone(),
            Truth::ActualPlace("the well"),
            KnowledgeRevision::try_new(2).unwrap(),
        );
        assert_ne!(first.private_truth(), second.private_truth());
        let projected = record
            .project(&observer_grant(7, 100), Presentation("the mill"))
            .unwrap();
        assert_eq!(projected.presentation, Presentation("the mill"));
        assert_eq!(projected.confidence, ConfidenceBand::Plausible);
    }

    #[test]
    fn mutation_receipts_make_replay_and_collision_top_level_decisions() {
        let record_input = MutationInput::Record(KnowledgeRecordId::try_new(1).unwrap());
        let record_outcome = MutationOutcome::Record(KnowledgeRecordId::try_new(1).unwrap());
        let share_input = MutationInput::Share(SharingReceiptId::try_new(2).unwrap());
        let share_outcome = MutationOutcome::Share(SharingReceiptId::try_new(2).unwrap());
        let contradiction_input =
            MutationInput::Contradiction(ContradictionId::try_new(3).unwrap());
        let contradiction_outcome =
            MutationOutcome::Contradiction(ContradictionId::try_new(3).unwrap());
        let receipts = vec![
            KnowledgeMutationReceipt::committed(
                KnowledgeMutationRequestId::try_new(10).unwrap(),
                record_input.clone(),
                record_outcome.clone(),
            ),
            KnowledgeMutationReceipt::committed(
                KnowledgeMutationRequestId::try_new(11).unwrap(),
                share_input.clone(),
                share_outcome.clone(),
            ),
            KnowledgeMutationReceipt::committed(
                KnowledgeMutationRequestId::try_new(12).unwrap(),
                contradiction_input.clone(),
                contradiction_outcome.clone(),
            ),
        ];

        assert_eq!(
            classify_knowledge_mutation(
                &receipts,
                KnowledgeMutationRequestId::try_new(10).unwrap(),
                &record_input,
            ),
            KnowledgeMutationDecision::Replay(&record_outcome)
        );
        assert_eq!(
            classify_knowledge_mutation(
                &receipts,
                KnowledgeMutationRequestId::try_new(11).unwrap(),
                &share_input,
            ),
            KnowledgeMutationDecision::Replay(&share_outcome)
        );
        assert_eq!(
            classify_knowledge_mutation(
                &receipts,
                KnowledgeMutationRequestId::try_new(12).unwrap(),
                &contradiction_input,
            ),
            KnowledgeMutationDecision::Replay(&contradiction_outcome)
        );
        assert_eq!(
            classify_knowledge_mutation(
                &receipts,
                KnowledgeMutationRequestId::try_new(10).unwrap(),
                &share_input,
            ),
            KnowledgeMutationDecision::Collision
        );
        assert_eq!(
            classify_knowledge_mutation(
                &receipts,
                KnowledgeMutationRequestId::try_new(13).unwrap(),
                &record_input,
            ),
            KnowledgeMutationDecision::Apply
        );

        let mut ambiguous = receipts.clone();
        ambiguous.push(receipts[0].clone());
        assert_eq!(
            classify_knowledge_mutation(
                &ambiguous,
                KnowledgeMutationRequestId::try_new(10).unwrap(),
                &record_input,
            ),
            KnowledgeMutationDecision::AmbiguousPriorReceipt
        );
    }

    #[test]
    fn domain_extensions_keep_bestiary_and_physic_notebook_facts_distinct() {
        let creature: KnowledgeSubject<Subject> = KnowledgeSubject::Domain(Subject::Creature(2));
        let outbreak: KnowledgeSubject<Subject> = KnowledgeSubject::Domain(Subject::Outbreak(2));
        assert_ne!(creature, outbreak);
        assert_ne!(Proposition::CreatureWeakness, Proposition::SourceLocation);
        assert_ne!(Belief::Weakness("fire"), Belief::Place("fire"));
        assert_ne!(
            Truth::ActualWeakness("silver"),
            Truth::ActualPlace("silver")
        );
        assert_ne!(Source::Inspection(1), Source::Testimony(1));
    }
}
