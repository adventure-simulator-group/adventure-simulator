//! Pure planning contracts for strategic actions.
//!
//! This module has no serialization or persistence surface. Domain reducers
//! own their closed requirement/effect enums, gather authoritative state,
//! construct plans here, and apply effects transactionally only after a fresh
//! replan passes [`validate_commit`]. A plan is evidence, never authority.

use std::{collections::BTreeSet, fmt, num::NonZeroU64};

use crate::{
    physical_object::{
        CustodyCharacterId, CustodyIdentityError, ObjectCustody, OperationalCustody,
        PhysicalObjectId,
    },
    strategic_place::{StrategicFixtureId, StrategicPlaceId},
};

/// The small, public answer to a contextual Character interaction request.
///
/// Domains remain responsible for proving presence, privacy, and their own
/// emergency doctrine. This type deliberately does not grow into a universal
/// permission or policy engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextualActionDecision {
    Allowed(ContextualActionReason),
    Refused,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextualActionReason {
    SelfAction,
    TargetPermission,
    EmergencyMedicalNecessity,
}

impl ContextualActionDecision {
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed(_))
    }
}

/// Resolve an already-authoritatively gathered contextual answer. A target's
/// explicit refusal always wins; emergency authority can only fill an absent
/// (`Unavailable`) permission.
pub const fn decide_contextual_action(
    self_action: bool,
    target_answer: ContextualActionDecision,
    emergency_medical_necessity: bool,
) -> ContextualActionDecision {
    if self_action {
        ContextualActionDecision::Allowed(ContextualActionReason::SelfAction)
    } else {
        match target_answer {
            ContextualActionDecision::Refused => ContextualActionDecision::Refused,
            ContextualActionDecision::Allowed(reason) => ContextualActionDecision::Allowed(reason),
            ContextualActionDecision::Unavailable if emergency_medical_necessity => {
                ContextualActionDecision::Allowed(ContextualActionReason::EmergencyMedicalNecessity)
            }
            ContextualActionDecision::Unavailable => ContextualActionDecision::Unavailable,
        }
    }
}

pub fn emergency_bandage_is_necessary(
    incapacitated: bool,
    procedure: &str,
    selected_limb_cut_damage: f32,
    selected_limb_bandaged: bool,
) -> bool {
    incapacitated
        && procedure == "bandage"
        && selected_limb_cut_damage > 0.0
        && !selected_limb_bandaged
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NegotiatedWithdrawalAssessment {
    pub accepted: bool,
    pub score: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostileSurrenderAssessment {
    pub accepts_demand: bool,
    pub offers_surrender: bool,
    pub demand_score: f32,
}

/// Narrow authored policy for whole-group pre-combat surrender. Awareness and
/// low morale make capitulation more likely; language-scaled social ability and
/// spokesman affinity govern whether a player demand is accepted.
pub fn assess_hostile_surrender(
    social_ability: f32,
    shared_language_coefficient: f32,
    spokesman_affinity: f32,
    hostile_morale_percent: u8,
    public_awareness_bps: u16,
) -> HostileSurrenderAssessment {
    let language = shared_language_coefficient.clamp(0.0, 1.0);
    let social = social_ability.clamp(0.0, 5.0) * language;
    let relationship = spokesman_affinity.clamp(-100.0, 100.0) / 25.0;
    let morale_pressure = 100_u8.saturating_sub(hostile_morale_percent.min(100)) as f32 / 20.0;
    let awareness = f32::from(public_awareness_bps.min(10_000)) / 2_000.0;
    let demand_score = social + relationship + morale_pressure + awareness;
    HostileSurrenderAssessment {
        accepts_demand: language > 0.0 && demand_score >= 7.5,
        offers_surrender: language > 0.0
            && hostile_morale_percent <= 50
            && public_awareness_bps >= 5_000,
        demand_score,
    }
}

/// Deterministic pre-combat response. Social ability is language-scaled;
/// existing affinity and pressure from low morale affect the same
/// bounded score. A refusal changes no authority, so changed state can be
/// assessed again without a special cooldown or permanent refusal flag.
pub fn assess_negotiated_withdrawal(
    social_ability: f32,
    shared_language_coefficient: f32,
    spokesman_affinity: f32,
    hostile_morale_percent: u8,
) -> NegotiatedWithdrawalAssessment {
    let language = shared_language_coefficient.clamp(0.0, 1.0);
    let social = social_ability.clamp(0.0, 5.0) * language;
    let relationship = spokesman_affinity.clamp(-100.0, 100.0) / 20.0;
    let pressure = 100_u8.saturating_sub(hostile_morale_percent.min(100)) as f32 / 20.0;
    let score = social + relationship + pressure;
    NegotiatedWithdrawalAssessment {
        accepted: language > 0.0 && score >= 7.5,
        score,
    }
}

pub trait DomainTarget: Clone + fmt::Debug + Eq {}
pub trait DomainRequirement: Clone + fmt::Debug + Eq {}
pub trait DomainCapability: Clone + fmt::Debug + Eq {}
pub trait DomainInterruption: Clone + fmt::Debug + Eq {}
pub trait DomainEffect: Clone + fmt::Debug + Eq {}
pub trait PublicPreview: Clone + fmt::Debug + Eq {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionTarget<T: DomainTarget> {
    Character(CustodyCharacterId),
    Object(PhysicalObjectId),
    Place(StrategicPlaceId),
    Fixture(StrategicFixtureId),
    Domain(T),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolReference {
    custody: ObjectCustody,
}

impl ToolReference {
    pub fn try_new(
        object_id: PhysicalObjectId,
        expected_custody: OperationalCustody,
    ) -> Result<Self, CustodyIdentityError> {
        Ok(Self {
            custody: ObjectCustody::try_new(object_id, expected_custody)?,
        })
    }

    pub const fn object_id(&self) -> PhysicalObjectId {
        self.custody.object_id()
    }

    pub const fn expected_custody(&self) -> &OperationalCustody {
        self.custody.custody()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionCoordinates<T: DomainTarget> {
    actor: CustodyCharacterId,
    target: ActionTarget<T>,
    place: StrategicPlaceId,
    fixture: Option<StrategicFixtureId>,
    tools: Vec<ToolReference>,
}

impl<T: DomainTarget> ActionCoordinates<T> {
    pub fn try_new(
        actor: CustodyCharacterId,
        target: ActionTarget<T>,
        place: StrategicPlaceId,
        fixture: Option<StrategicFixtureId>,
        tools: Vec<ToolReference>,
    ) -> Result<Self, CoordinateError> {
        if fixture
            .as_ref()
            .is_some_and(|fixture| fixture.place() != &place)
        {
            return Err(CoordinateError::FixturePlaceMismatch);
        }
        match &target {
            ActionTarget::Place(target) if target != &place => {
                return Err(CoordinateError::TargetPlaceMismatch);
            }
            ActionTarget::Fixture(target) if target.place() != &place => {
                return Err(CoordinateError::TargetPlaceMismatch);
            }
            _ => {}
        }
        let unique_tools = tools
            .iter()
            .map(ToolReference::object_id)
            .collect::<BTreeSet<_>>();
        if unique_tools.len() != tools.len() {
            return Err(CoordinateError::DuplicateTool);
        }
        Ok(Self {
            actor,
            target,
            place,
            fixture,
            tools,
        })
    }

    pub const fn actor(&self) -> CustodyCharacterId {
        self.actor
    }
    pub const fn target(&self) -> &ActionTarget<T> {
        &self.target
    }
    pub const fn place(&self) -> &StrategicPlaceId {
        &self.place
    }
    pub const fn fixture(&self) -> Option<&StrategicFixtureId> {
        self.fixture.as_ref()
    }
    pub fn tools(&self) -> &[ToolReference] {
        &self.tools
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinateError {
    FixturePlaceMismatch,
    TargetPlaceMismatch,
    DuplicateTool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SnapshotRevision(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SnapshotDigest(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AuthoritativeSnapshot {
    pub revision: SnapshotRevision,
    pub digest: SnapshotDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilitySnapshot<C: DomainCapability> {
    pub capability: C,
    pub revision: SnapshotRevision,
    pub milli_value: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionRequirement<R: DomainRequirement, C: DomainCapability> {
    LivingActor,
    ActorAtExactPlace,
    TargetAvailable,
    FixtureAvailable(StrategicFixtureId),
    ToolInCustody(ToolReference),
    CapabilityAtLeast {
        snapshot: CapabilitySnapshot<C>,
        minimum_milli: i32,
    },
    /// Extension point for closed domain enums, including future typed rights
    /// questions. This is never a string kind or untyped payload.
    Domain(R),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementCheck<R: DomainRequirement, C: DomainCapability> {
    pub requirement: ActionRequirement<R, C>,
    pub satisfied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicRejection {
    Unavailable,
    InvalidRequest,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateRejection<R: DomainRequirement, C: DomainCapability> {
    failed: Vec<ActionRequirement<R, C>>,
    public: PublicRejection,
}

impl<R: DomainRequirement, C: DomainCapability> PrivateRejection<R, C> {
    pub fn failed_requirements(&self) -> &[ActionRequirement<R, C>] {
        &self.failed
    }
    pub const fn sanitized(&self) -> PublicRejection {
        self.public
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestedDuration(NonZeroU64);

impl RequestedDuration {
    pub fn try_new(minutes: u64) -> Result<Self, DurationError> {
        NonZeroU64::new(minutes)
            .map(Self)
            .ok_or(DurationError::ZeroDuration)
    }
    pub const fn minutes(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurationError {
    ZeroDuration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledInterruption<I: DomainInterruption> {
    pub at_minute: u64,
    pub cause: I,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeBoundaries<I: DomainInterruption> {
    pub terminal_minute: Option<u64>,
    pub interruption: Option<ScheduledInterruption<I>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimeOutcome<I: DomainInterruption> {
    Completed,
    TerminalBoundary,
    Interrupted(I),
    /// The requested positive duration cannot fit in the strategic clock.
    ClockExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeResolution<I: DomainInterruption> {
    pub start_minute: u64,
    pub requested_minutes: u64,
    pub elapsed_minutes: u64,
    pub end_minute: u64,
    pub outcome: TimeOutcome<I>,
}

impl<I: DomainInterruption> TimeResolution<I> {
    /// Only this outcome permits a domain to emit completion-only effects.
    pub const fn permits_completion_effects(&self) -> bool {
        matches!(self.outcome, TimeOutcome::Completed)
    }
}

pub fn resolve_time<I: DomainInterruption>(
    current_minute: u64,
    duration: RequestedDuration,
    boundaries: &TimeBoundaries<I>,
) -> TimeResolution<I> {
    let requested_end = current_minute.checked_add(duration.minutes());
    let latest_representable_end = requested_end.unwrap_or(u64::MAX);
    let terminal = boundaries
        .terminal_minute
        .map(|minute| minute.max(current_minute));
    let interruption = boundaries
        .interruption
        .as_ref()
        .map(|value| (value.at_minute.max(current_minute), value.cause.clone()));
    let (end_minute, outcome) = match (terminal, interruption) {
        (Some(terminal), Some((at, cause))) if at < terminal && at <= latest_representable_end => {
            (at, TimeOutcome::Interrupted(cause))
        }
        (Some(terminal), _) if terminal <= latest_representable_end => {
            (terminal, TimeOutcome::TerminalBoundary)
        }
        (None, Some((at, cause))) if at <= latest_representable_end => {
            (at, TimeOutcome::Interrupted(cause))
        }
        _ => match requested_end {
            Some(u64::MAX) => (u64::MAX, TimeOutcome::ClockExhausted),
            Some(end) => (end, TimeOutcome::Completed),
            None => (u64::MAX, TimeOutcome::ClockExhausted),
        },
    };
    TimeResolution {
        start_minute: current_minute,
        requested_minutes: duration.minutes(),
        elapsed_minutes: end_minute.saturating_sub(current_minute),
        end_minute,
        outcome,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionEffect<E: DomainEffect> {
    AdvanceActorTime {
        actor: CustodyCharacterId,
        from_minute: u64,
        to_minute: u64,
    },
    TransferObjectCustody(CustodyTransfer),
    Domain(E),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustodyTransfer {
    object_id: PhysicalObjectId,
    expected_from: OperationalCustody,
    destination: OperationalCustody,
}

impl CustodyTransfer {
    pub fn try_new(
        object_id: PhysicalObjectId,
        expected_from: OperationalCustody,
        destination: OperationalCustody,
    ) -> Result<Self, CustodyIdentityError> {
        ObjectCustody::try_new(object_id, expected_from.clone())?;
        ObjectCustody::try_new(object_id, destination.clone())?;
        Ok(Self {
            object_id,
            expected_from,
            destination,
        })
    }

    pub const fn object_id(&self) -> PhysicalObjectId {
        self.object_id
    }
    pub const fn expected_from(&self) -> &OperationalCustody {
        &self.expected_from
    }
    pub const fn destination(&self) -> &OperationalCustody {
        &self.destination
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalculatedAction<E: DomainEffect, P: PublicPreview> {
    pub effects: Vec<ActionEffect<E>>,
    pub public_preview: P,
}

macro_rules! canonical_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);
        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, ProvenanceError> {
                let value = value.into();
                if value.is_empty()
                    || value.trim() != value
                    || value.len() > 256
                    || value.chars().any(char::is_control)
                {
                    Err(ProvenanceError::InvalidId)
                } else {
                    Ok(Self(value))
                }
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

canonical_id!(ActionRequestId);
canonical_id!(ActionDefinitionId);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AuthorityBinding(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvenanceError {
    InvalidId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanProvenance {
    pub request_id: ActionRequestId,
    pub action_id: ActionDefinitionId,
    pub input_digest: SnapshotDigest,
    /// Private reducer-authored binding. It is deliberately not serializable.
    pub authority_binding: AuthorityBinding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanInput<
    T: DomainTarget,
    R: DomainRequirement,
    C: DomainCapability,
    I: DomainInterruption,
> {
    pub coordinates: ActionCoordinates<T>,
    pub provenance: PlanProvenance,
    pub snapshot: AuthoritativeSnapshot,
    pub current_minute: u64,
    pub duration: RequestedDuration,
    pub boundaries: TimeBoundaries<I>,
    pub requirements: Vec<RequirementCheck<R, C>>,
    pub sanitized_rejection: PublicRejection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicActionPlan<P: PublicPreview> {
    pub request_id: ActionRequestId,
    pub elapsed_minutes: u64,
    pub completed: bool,
    pub preview: P,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoritativeActionPlan<
    T: DomainTarget,
    R: DomainRequirement,
    C: DomainCapability,
    I: DomainInterruption,
    E: DomainEffect,
    P: PublicPreview,
> {
    coordinates: ActionCoordinates<T>,
    provenance: PlanProvenance,
    snapshot: AuthoritativeSnapshot,
    requirements: Vec<RequirementCheck<R, C>>,
    time: TimeResolution<I>,
    calculation: CalculatedAction<E, P>,
}

impl<
    T: DomainTarget,
    R: DomainRequirement,
    C: DomainCapability,
    I: DomainInterruption,
    E: DomainEffect,
    P: PublicPreview,
> AuthoritativeActionPlan<T, R, C, I, E, P>
{
    pub const fn coordinates(&self) -> &ActionCoordinates<T> {
        &self.coordinates
    }
    pub const fn provenance(&self) -> &PlanProvenance {
        &self.provenance
    }
    pub const fn snapshot(&self) -> AuthoritativeSnapshot {
        self.snapshot
    }
    pub fn requirements(&self) -> &[RequirementCheck<R, C>] {
        &self.requirements
    }
    pub const fn time(&self) -> &TimeResolution<I> {
        &self.time
    }
    pub fn effects(&self) -> &[ActionEffect<E>] {
        &self.calculation.effects
    }
    pub fn public_plan(&self) -> PublicActionPlan<P>
    where
        P: Clone,
    {
        PublicActionPlan {
            request_id: self.provenance.request_id.clone(),
            elapsed_minutes: self.time.elapsed_minutes,
            completed: self.time.permits_completion_effects(),
            preview: self.calculation.public_preview.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanningOutcome<
    T: DomainTarget,
    R: DomainRequirement,
    C: DomainCapability,
    I: DomainInterruption,
    E: DomainEffect,
    P: PublicPreview,
> {
    Ready(AuthoritativeActionPlan<T, R, C, I, E, P>),
    Rejected(PrivateRejection<R, C>),
}

pub fn build_plan<T, R, C, I, E, P>(
    input: PlanInput<T, R, C, I>,
    calculate: impl FnOnce(&ActionCoordinates<T>, &TimeResolution<I>) -> CalculatedAction<E, P>,
) -> PlanningOutcome<T, R, C, I, E, P>
where
    T: DomainTarget,
    R: DomainRequirement,
    C: DomainCapability,
    I: DomainInterruption,
    E: DomainEffect,
    P: PublicPreview,
{
    let failed = input
        .requirements
        .iter()
        .filter(|check| !check.satisfied)
        .map(|check| check.requirement.clone())
        .collect::<Vec<_>>();
    if !failed.is_empty() {
        return PlanningOutcome::Rejected(PrivateRejection {
            failed,
            public: input.sanitized_rejection,
        });
    }
    let time = resolve_time(input.current_minute, input.duration, &input.boundaries);
    let calculation = calculate(&input.coordinates, &time);
    PlanningOutcome::Ready(AuthoritativeActionPlan {
        coordinates: input.coordinates,
        provenance: input.provenance,
        snapshot: input.snapshot,
        requirements: input.requirements,
        time,
        calculation,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitAttempt {
    pub request_id: ActionRequestId,
    pub action_id: ActionDefinitionId,
    pub authority_binding: AuthorityBinding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    pub provenance: PlanProvenance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitDecision {
    Apply,
    IdempotentReplay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitRejection {
    ForgedPlan,
    IdempotencyConflict,
    StaleSnapshot,
    PrerequisitesChanged,
    CalculationChanged,
}

pub fn validate_commit<T, R, C, I, E, P>(
    planned: &AuthoritativeActionPlan<T, R, C, I, E, P>,
    replanned: &PlanningOutcome<T, R, C, I, E, P>,
    current_snapshot: AuthoritativeSnapshot,
    attempt: &CommitAttempt,
    prior_receipt: Option<&CommitReceipt>,
) -> Result<CommitDecision, CommitRejection>
where
    T: DomainTarget,
    R: DomainRequirement,
    C: DomainCapability,
    I: DomainInterruption,
    E: DomainEffect,
    P: PublicPreview,
{
    let provenance = &planned.provenance;
    let attempt_matches = attempt.request_id == provenance.request_id
        && attempt.action_id == provenance.action_id
        && attempt.authority_binding == provenance.authority_binding;
    if let Some(receipt) = prior_receipt
        && receipt.provenance.request_id == attempt.request_id
    {
        return if attempt_matches && receipt.provenance == *provenance {
            Ok(CommitDecision::IdempotentReplay)
        } else {
            Err(CommitRejection::IdempotencyConflict)
        };
    }
    if !attempt_matches || provenance.input_digest != planned.snapshot.digest {
        return Err(CommitRejection::ForgedPlan);
    }
    if planned.snapshot != current_snapshot {
        return Err(CommitRejection::StaleSnapshot);
    }
    let PlanningOutcome::Ready(replanned) = replanned else {
        return Err(CommitRejection::PrerequisitesChanged);
    };
    if replanned.snapshot != current_snapshot {
        return Err(CommitRejection::StaleSnapshot);
    }
    if planned.coordinates != replanned.coordinates
        || planned.provenance != replanned.provenance
        || planned.requirements != replanned.requirements
        || planned.time != replanned.time
        || planned.calculation != replanned.calculation
    {
        return Err(CommitRejection::CalculationChanged);
    }
    Ok(CommitDecision::Apply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategic_place::SettlementVenueKind;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Target {
        Evidence(u64),
    }
    impl DomainTarget for Target {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Req {
        KnowsCase,
    }
    impl DomainRequirement for Req {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Cap {
        Investigation,
    }
    impl DomainCapability for Cap {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Interrupt {
        Encounter,
    }
    impl DomainInterruption for Interrupt {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Effect {
        InspectEvidence(u64),
    }
    impl DomainEffect for Effect {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Preview {
        elapsed: u64,
    }
    impl PublicPreview for Preview {}

    fn snapshot(revision: u64, byte: u8) -> AuthoritativeSnapshot {
        AuthoritativeSnapshot {
            revision: SnapshotRevision(revision),
            digest: SnapshotDigest([byte; 32]),
        }
    }
    fn input(snapshot: AuthoritativeSnapshot) -> PlanInput<Target, Req, Cap, Interrupt> {
        let place = StrategicPlaceId::settlement_venue("lubeck", SettlementVenueKind::Inn).unwrap();
        PlanInput {
            coordinates: ActionCoordinates::try_new(
                CustodyCharacterId::try_new(7).unwrap(),
                ActionTarget::Domain(Target::Evidence(9)),
                place,
                None,
                vec![],
            )
            .unwrap(),
            provenance: PlanProvenance {
                request_id: ActionRequestId::try_new("request-1").unwrap(),
                action_id: ActionDefinitionId::try_new("investigate.inspect").unwrap(),
                input_digest: snapshot.digest,
                authority_binding: AuthorityBinding([3; 32]),
            },
            snapshot,
            current_minute: 100,
            duration: RequestedDuration::try_new(60).unwrap(),
            boundaries: TimeBoundaries {
                terminal_minute: None,
                interruption: None,
            },
            requirements: vec![RequirementCheck {
                requirement: ActionRequirement::Domain(Req::KnowsCase),
                satisfied: true,
            }],
            sanitized_rejection: PublicRejection::Unavailable,
        }
    }
    fn calculate(
        _: &ActionCoordinates<Target>,
        time: &TimeResolution<Interrupt>,
    ) -> CalculatedAction<Effect, Preview> {
        CalculatedAction {
            effects: vec![ActionEffect::Domain(Effect::InspectEvidence(9))],
            public_preview: Preview {
                elapsed: time.elapsed_minutes,
            },
        }
    }

    #[test]
    fn clipping_is_partition_invariant() {
        for boundary in 100..=220 {
            let whole = resolve_time(
                100,
                RequestedDuration::try_new(120).unwrap(),
                &TimeBoundaries::<Interrupt> {
                    terminal_minute: Some(boundary),
                    interruption: None,
                },
            );
            let mut cursor = 100;
            for part in [17, 31, 72] {
                let resolved = resolve_time(
                    cursor,
                    RequestedDuration::try_new(part).unwrap(),
                    &TimeBoundaries::<Interrupt> {
                        terminal_minute: Some(boundary),
                        interruption: None,
                    },
                );
                cursor = resolved.end_minute;
                if !matches!(resolved.outcome, TimeOutcome::Completed) {
                    break;
                }
            }
            assert_eq!(cursor, whole.end_minute);
        }
    }

    #[test]
    fn exact_endpoint_boundaries_never_enable_completion_effects_when_partitioned() {
        let duration = RequestedDuration::try_new(60).unwrap();
        let terminal = TimeBoundaries::<Interrupt> {
            terminal_minute: Some(160),
            interruption: None,
        };
        let terminal_whole = resolve_time(100, RequestedDuration::try_new(120).unwrap(), &terminal);
        let terminal_partition = resolve_time(100, duration, &terminal);
        assert_eq!(terminal_whole.end_minute, terminal_partition.end_minute);
        assert_eq!(terminal_partition.outcome, TimeOutcome::TerminalBoundary);
        assert!(!terminal_partition.permits_completion_effects());

        let interrupted = TimeBoundaries {
            terminal_minute: None,
            interruption: Some(ScheduledInterruption {
                at_minute: 160,
                cause: Interrupt::Encounter,
            }),
        };
        let interrupted_whole =
            resolve_time(100, RequestedDuration::try_new(120).unwrap(), &interrupted);
        let interrupted_partition = resolve_time(100, duration, &interrupted);
        assert_eq!(
            interrupted_whole.end_minute,
            interrupted_partition.end_minute
        );
        assert!(!emergency_bandage_is_necessary(
            false, "bandage", 0.4, false
        ));
        assert!(emergency_bandage_is_necessary(true, "bandage", 0.4, false));
        assert!(!emergency_bandage_is_necessary(true, "stitch", 0.4, false));
        assert!(!emergency_bandage_is_necessary(true, "bandage", 0.0, false));
        assert!(!emergency_bandage_is_necessary(true, "bandage", 0.4, true));
        assert_eq!(
            interrupted_partition.outcome,
            TimeOutcome::Interrupted(Interrupt::Encounter)
        );
        assert!(!interrupted_partition.permits_completion_effects());
    }

    #[test]
    fn clock_exhaustion_is_typed_and_partition_end_is_not_completion() {
        let whole = resolve_time(
            u64::MAX - 5,
            RequestedDuration::try_new(10).unwrap(),
            &TimeBoundaries::<Interrupt> {
                terminal_minute: None,
                interruption: None,
            },
        );
        assert_eq!(whole.elapsed_minutes, 5);
        assert_eq!(whole.end_minute, u64::MAX);
        assert_eq!(whole.outcome, TimeOutcome::ClockExhausted);
        assert!(!whole.permits_completion_effects());

        let first = resolve_time(
            u64::MAX - 5,
            RequestedDuration::try_new(5).unwrap(),
            &TimeBoundaries::<Interrupt> {
                terminal_minute: None,
                interruption: None,
            },
        );
        let second = resolve_time(
            first.end_minute,
            RequestedDuration::try_new(5).unwrap(),
            &TimeBoundaries::<Interrupt> {
                terminal_minute: None,
                interruption: None,
            },
        );
        assert_eq!(
            first.elapsed_minutes + second.elapsed_minutes,
            whole.elapsed_minutes
        );
        assert_eq!(first.outcome, TimeOutcome::ClockExhausted);
        assert!(!first.permits_completion_effects());
        assert_eq!(second.outcome, TimeOutcome::ClockExhausted);
        assert!(!second.permits_completion_effects());

        let at_maximum = resolve_time(
            u64::MAX,
            RequestedDuration::try_new(1).unwrap(),
            &TimeBoundaries::<Interrupt> {
                terminal_minute: None,
                interruption: None,
            },
        );
        assert_eq!(at_maximum.elapsed_minutes, 0);
        assert_eq!(at_maximum.outcome, TimeOutcome::ClockExhausted);
    }

    #[test]
    fn interruption_clips_elapsed_and_calculation_matches_preview_and_commit() {
        let snap = snapshot(1, 1);
        let mut first = input(snap);
        first.boundaries.interruption = Some(ScheduledInterruption {
            at_minute: 125,
            cause: Interrupt::Encounter,
        });
        let planned = build_plan(first.clone(), calculate);
        let replanned = build_plan(first, calculate);
        let PlanningOutcome::Ready(ref plan) = planned else {
            panic!()
        };
        assert_eq!(plan.time.elapsed_minutes, 25);
        assert_eq!(plan.public_plan().preview.elapsed, 25);
        let attempt = CommitAttempt {
            request_id: plan.provenance.request_id.clone(),
            action_id: plan.provenance.action_id.clone(),
            authority_binding: plan.provenance.authority_binding,
        };
        assert_eq!(
            validate_commit(plan, &replanned, snap, &attempt, None),
            Ok(CommitDecision::Apply)
        );
    }

    #[test]
    fn current_or_tied_terminal_boundary_clips_to_zero_deterministically() {
        let terminal = TimeBoundaries {
            terminal_minute: Some(100),
            interruption: Some(ScheduledInterruption {
                at_minute: 100,
                cause: Interrupt::Encounter,
            }),
        };
        let resolved = resolve_time(100, RequestedDuration::try_new(60).unwrap(), &terminal);
        assert_eq!(resolved.elapsed_minutes, 0);
        assert_eq!(resolved.end_minute, 100);
        assert_eq!(resolved.outcome, TimeOutcome::TerminalBoundary);
    }

    #[test]
    fn stale_replay_and_forgery_are_distinct() {
        let snap = snapshot(1, 1);
        let planned = build_plan(input(snap), calculate);
        let replanned = build_plan(input(snap), calculate);
        let PlanningOutcome::Ready(ref plan) = planned else {
            panic!()
        };
        let attempt = CommitAttempt {
            request_id: plan.provenance.request_id.clone(),
            action_id: plan.provenance.action_id.clone(),
            authority_binding: plan.provenance.authority_binding,
        };
        assert_eq!(
            validate_commit(plan, &replanned, snapshot(2, 2), &attempt, None),
            Err(CommitRejection::StaleSnapshot)
        );
        assert_eq!(
            validate_commit(plan, &replanned, snapshot(2, 1), &attempt, None),
            Err(CommitRejection::StaleSnapshot)
        );
        let receipt = CommitReceipt {
            provenance: plan.provenance.clone(),
        };
        assert_eq!(
            validate_commit(plan, &replanned, snap, &attempt, Some(&receipt)),
            Ok(CommitDecision::IdempotentReplay)
        );
        let forged = CommitAttempt {
            authority_binding: AuthorityBinding([9; 32]),
            ..attempt
        };
        assert_eq!(
            validate_commit(plan, &replanned, snap, &forged, None),
            Err(CommitRejection::ForgedPlan)
        );
        assert_eq!(
            validate_commit(plan, &replanned, snap, &forged, Some(&receipt)),
            Err(CommitRejection::IdempotencyConflict)
        );
    }

    #[test]
    fn private_prerequisites_do_not_change_public_rejection() {
        let mut one = input(snapshot(1, 1));
        one.requirements[0].satisfied = false;
        let mut two = one.clone();
        two.requirements.push(RequirementCheck {
            requirement: ActionRequirement::CapabilityAtLeast {
                snapshot: CapabilitySnapshot {
                    capability: Cap::Investigation,
                    revision: SnapshotRevision(1),
                    milli_value: 0,
                },
                minimum_milli: 2000,
            },
            satisfied: false,
        });
        let PlanningOutcome::Rejected(one) = build_plan(one, calculate) else {
            panic!()
        };
        let PlanningOutcome::Rejected(two) = build_plan(two, calculate) else {
            panic!()
        };
        assert_ne!(
            one.failed_requirements().len(),
            two.failed_requirements().len()
        );
        assert_eq!(one.sanitized(), two.sanitized());
    }

    #[test]
    fn commit_revalidation_rejects_changed_prerequisites_and_calculation() {
        let snap = snapshot(1, 1);
        let planned = build_plan(input(snap), calculate);
        let PlanningOutcome::Ready(ref plan) = planned else {
            panic!()
        };
        let attempt = CommitAttempt {
            request_id: plan.provenance.request_id.clone(),
            action_id: plan.provenance.action_id.clone(),
            authority_binding: plan.provenance.authority_binding,
        };

        let mut unavailable = input(snap);
        unavailable.requirements[0].satisfied = false;
        let unavailable = build_plan(unavailable, calculate);
        assert_eq!(
            validate_commit(plan, &unavailable, snap, &attempt, None),
            Err(CommitRejection::PrerequisitesChanged)
        );

        let changed = build_plan(input(snap), |_, time| CalculatedAction {
            effects: vec![],
            public_preview: Preview {
                elapsed: time.elapsed_minutes,
            },
        });
        assert_eq!(
            validate_commit(plan, &changed, snap, &attempt, None),
            Err(CommitRejection::CalculationChanged)
        );
    }

    #[test]
    fn effects_are_closed_typed_values_not_client_kinds() {
        let PlanningOutcome::Ready(plan) = build_plan(input(snapshot(1, 1)), calculate) else {
            panic!()
        };
        assert_eq!(
            plan.effects(),
            &[ActionEffect::Domain(Effect::InspectEvidence(9))]
        );
        assert_eq!(std::mem::size_of::<Effect>(), std::mem::size_of::<u64>());
    }

    #[test]
    fn contextual_decisions_keep_refusal_distinct_from_unavailability() {
        assert!(ContextualActionDecision::Allowed(ContextualActionReason::SelfAction).is_allowed());
        assert!(
            ContextualActionDecision::Allowed(ContextualActionReason::TargetPermission)
                .is_allowed()
        );
        assert!(
            ContextualActionDecision::Allowed(ContextualActionReason::EmergencyMedicalNecessity)
                .is_allowed()
        );
        assert!(!ContextualActionDecision::Refused.is_allowed());
        assert!(!ContextualActionDecision::Unavailable.is_allowed());
        assert_ne!(
            ContextualActionDecision::Refused,
            ContextualActionDecision::Unavailable
        );
        assert_eq!(
            decide_contextual_action(true, ContextualActionDecision::Refused, false),
            ContextualActionDecision::Allowed(ContextualActionReason::SelfAction)
        );
        assert_eq!(
            decide_contextual_action(false, ContextualActionDecision::Refused, true),
            ContextualActionDecision::Refused
        );
        assert_eq!(
            decide_contextual_action(false, ContextualActionDecision::Unavailable, true),
            ContextualActionDecision::Allowed(ContextualActionReason::EmergencyMedicalNecessity)
        );
    }

    #[test]
    fn negotiated_withdrawal_uses_live_social_scale_language_relationship_and_morale() {
        let refusal = assess_negotiated_withdrawal(5.0, 1.0, 0.0, 80);
        assert!(!refusal.accepted);
        let social_acceptance = assess_negotiated_withdrawal(5.0, 1.0, 0.0, 50);
        assert!(social_acceptance.accepted);
        let pressure_acceptance = assess_negotiated_withdrawal(5.0, 1.0, 20.0, 70);
        assert!(pressure_acceptance.accepted);
        assert_eq!(
            assess_negotiated_withdrawal(10.0, 1.0, 0.0, 50).score,
            social_acceptance.score
        );
        assert!(!assess_negotiated_withdrawal(10.0, 0.0, 100.0, 0).accepted);
    }

    #[test]
    fn surrender_policy_is_bounded_authored_and_language_gated() {
        let reliable_demo = assess_hostile_surrender(5.0, 1.0, 0.0, 50, 6_000);
        assert!(reliable_demo.accepts_demand);
        assert!(reliable_demo.offers_surrender);
        assert!(!assess_hostile_surrender(5.0, 0.0, 100.0, 0, 10_000).accepts_demand);
        assert!(!assess_hostile_surrender(5.0, 1.0, 0.0, 70, 10_000).offers_surrender);
    }

    #[test]
    fn tool_and_transfer_custody_cannot_encode_self_containment() {
        let object_id = PhysicalObjectId::try_new(17).unwrap();
        let self_custody = OperationalCustody::Container(object_id);
        assert_eq!(
            ToolReference::try_new(object_id, self_custody.clone()),
            Err(CustodyIdentityError::SelfContainment)
        );
        assert_eq!(
            CustodyTransfer::try_new(
                object_id,
                self_custody.clone(),
                OperationalCustody::character(7).unwrap(),
            ),
            Err(CustodyIdentityError::SelfContainment)
        );
        assert_eq!(
            CustodyTransfer::try_new(
                object_id,
                OperationalCustody::character(7).unwrap(),
                self_custody,
            ),
            Err(CustodyIdentityError::SelfContainment)
        );

        let valid = CustodyTransfer::try_new(
            object_id,
            OperationalCustody::character(7).unwrap(),
            OperationalCustody::party("party-red").unwrap(),
        )
        .unwrap();
        assert_eq!(valid.object_id(), object_id);
    }
}
