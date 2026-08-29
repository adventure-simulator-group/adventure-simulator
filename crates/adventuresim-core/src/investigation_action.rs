//! Deterministic strategic investigation actions.
//!
//! This module deliberately accepts no canonical case truth. The server binds a
//! private target to an opaque capability and supplies only authoritative
//! environmental and party inputs.

use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use fabelgeist_determinism::mix64;
use serde::{Deserialize, Serialize};

use crate::{
    physical_object::CustodyCharacterId,
    rights::{
        CanonicalRightsQuestionDigest, DecisionProvenance, DomainJurisdiction,
        DomainRightsOperation, DomainRightsResource, DomainRightsSubject, PrivateRightsDecision,
        RightsDecisionKind, RightsJurisdiction, RightsOperation, RightsQuestion,
        RightsQuestionError, RightsResource, RightsRevision, RightsSubject,
    },
    strategic_action::{
        ActionCoordinates, ActionEffect, ActionRequirement, AuthoritativeSnapshot,
        CalculatedAction, DomainCapability, DomainEffect, DomainInterruption, DomainRequirement,
        DomainTarget, PlanInput, PlanProvenance, PlanningOutcome, PublicPreview, PublicRejection,
        RequestedDuration, RequirementCheck, TimeBoundaries, build_plan,
    },
    strategic_place::StrategicPlaceId,
};

const INVESTIGATION_ACTION_ROLL_DOMAIN: u64 = 0x494e_5645_5354_4143;
const INVESTIGATION_ATTEMPT_SHIFT: u32 = 17;
const INVESTIGATION_KIND_SHIFT: u32 = 41;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationActionKind {
    InspectSite,
    SearchArea,
    FollowTracks,
    ReacquireTracks,
    LocateContact,
    Watch,
    Patrol,
    LayAmbush,
    ApproachLead,
}

/// Stable observer-facing result of evaluating whether an investigation action
/// can begin now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "snake_case")]
pub enum InvestigationActionUnavailableReason {
    PartyNotReady,
    TravelRequired,
    NightWindow,
    TargetChanged,
    ContactScheduleWindow,
    ContactNotPresent,
    CharacterUnavailable,
    PartyRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationActionAvailability {
    Available,
    Unavailable {
        reason: InvestigationActionUnavailableReason,
        can_travel_to_required_site: bool,
        wait_minutes: u32,
    },
}

#[cfg(feature = "spacetimedb")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, spacetimedb::SpacetimeType)]
#[sats(name = "InvestigationActionUnavailableFields")]
struct InvestigationActionUnavailableFieldsSats {
    reason: InvestigationActionUnavailableReason,
    can_travel_to_required_site: bool,
    wait_minutes: u32,
}

#[cfg(feature = "spacetimedb")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, spacetimedb::SpacetimeType)]
#[sats(name = "InvestigationActionAvailability")]
enum InvestigationActionAvailabilitySats {
    Available,
    Unavailable(InvestigationActionUnavailableFieldsSats),
}

#[cfg(feature = "spacetimedb")]
impl From<InvestigationActionAvailability> for InvestigationActionAvailabilitySats {
    fn from(availability: InvestigationActionAvailability) -> Self {
        match availability {
            InvestigationActionAvailability::Available => Self::Available,
            InvestigationActionAvailability::Unavailable {
                reason,
                can_travel_to_required_site,
                wait_minutes,
            } => Self::Unavailable(InvestigationActionUnavailableFieldsSats {
                reason,
                can_travel_to_required_site,
                wait_minutes,
            }),
        }
    }
}

#[cfg(feature = "spacetimedb")]
impl From<InvestigationActionAvailabilitySats> for InvestigationActionAvailability {
    fn from(availability: InvestigationActionAvailabilitySats) -> Self {
        match availability {
            InvestigationActionAvailabilitySats::Available => Self::Available,
            InvestigationActionAvailabilitySats::Unavailable(
                InvestigationActionUnavailableFieldsSats {
                    reason,
                    can_travel_to_required_site,
                    wait_minutes,
                },
            ) => Self::Unavailable {
                reason,
                can_travel_to_required_site,
                wait_minutes,
            },
        }
    }
}

#[cfg(feature = "spacetimedb")]
impl spacetimedb::SpacetimeType for InvestigationActionAvailability {
    fn make_type<S: spacetimedb::sats::typespace::TypespaceBuilder>(
        typespace: &mut S,
    ) -> spacetimedb::spacetimedb_lib::AlgebraicType {
        InvestigationActionAvailabilitySats::make_type(typespace)
    }
}

#[cfg(feature = "spacetimedb")]
impl spacetimedb::Serialize for InvestigationActionAvailability {
    fn serialize<S: spacetimedb::spacetimedb_lib::ser::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        spacetimedb::Serialize::serialize(
            &InvestigationActionAvailabilitySats::from(*self),
            serializer,
        )
    }
}

#[cfg(feature = "spacetimedb")]
impl<'de> spacetimedb::Deserialize<'de> for InvestigationActionAvailability {
    fn deserialize<D: spacetimedb::spacetimedb_lib::de::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        <InvestigationActionAvailabilitySats as spacetimedb::Deserialize<'de>>::deserialize(
            deserializer,
        )
        .map(Self::from)
    }
}

impl InvestigationActionAvailability {
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }

    pub const fn unavailable(
        reason: InvestigationActionUnavailableReason,
        can_travel_to_required_site: bool,
        wait_minutes: u32,
    ) -> Self {
        Self::Unavailable {
            reason,
            can_travel_to_required_site,
            wait_minutes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "snake_case")]
pub enum InvestigationTargetKind {
    Site,
    Area,
    Contact,
    Cohort,
    Route,
    Tracks,
}

impl InvestigationTargetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Site => "site",
            Self::Area => "area",
            Self::Contact => "contact",
            Self::Cohort => "cohort",
            Self::Route => "route",
            Self::Tracks => "tracks",
        }
    }
}

impl std::fmt::Display for InvestigationTargetKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Observer-safe structural rule for one edge in a physical tracking chain.
///
/// This deliberately considers only projected action kinds and target kinds:
/// ownership, case correlation, successful completion, and current position
/// remain authoritative server checks.
pub fn tracking_route_edge_is_coherent(
    action_kind: InvestigationActionKind,
    action_target_kind: InvestigationTargetKind,
    predecessor_kind: InvestigationActionKind,
    predecessor_target_kind: InvestigationTargetKind,
) -> bool {
    match (action_kind, action_target_kind) {
        (
            InvestigationActionKind::ReacquireTracks,
            InvestigationTargetKind::Route
            | InvestigationTargetKind::Tracks
            | InvestigationTargetKind::Site,
        ) => predecessor_target_kind == InvestigationTargetKind::Area,
        (InvestigationActionKind::FollowTracks, InvestigationTargetKind::Site) => {
            matches!(
                predecessor_kind,
                InvestigationActionKind::FollowTracks | InvestigationActionKind::ReacquireTracks
            ) && matches!(
                predecessor_target_kind,
                InvestigationTargetKind::Route | InvestigationTargetKind::Tracks
            )
        }
        (
            InvestigationActionKind::FollowTracks,
            InvestigationTargetKind::Route | InvestigationTargetKind::Tracks,
        ) => {
            predecessor_target_kind == InvestigationTargetKind::Area
                || (matches!(
                    predecessor_kind,
                    InvestigationActionKind::FollowTracks
                        | InvestigationActionKind::ReacquireTracks
                ) && matches!(
                    predecessor_target_kind,
                    InvestigationTargetKind::Route | InvestigationTargetKind::Tracks
                ))
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    Road,
    Settlement,
    Plains,
    Forest,
    Hills,
    Marsh,
    Ruins,
    Underground,
}

impl Terrain {
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::Road => "road",
            Self::Settlement => "settlement",
            Self::Plains => "plains",
            Self::Forest => "forest",
            Self::Hills => "hills",
            Self::Marsh => "marsh",
            Self::Ruins => "ruins",
            Self::Underground => "underground",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Day,
    Night,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherAuthority {
    Clear {
        snow_cover_bps: u16,
    },
    Rain {
        intensity_bps: u16,
        snow_cover_bps: u16,
    },
    Snow {
        intensity_bps: u16,
        snow_cover_bps: u16,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillContribution {
    /// The best relevant terrain skill in basis points.
    pub terrain_bps: u16,
    /// Perception/awareness contribution in basis points.
    pub awareness_bps: u16,
    /// Stealth contribution used by watch and ambush actions.
    pub stealth_bps: u16,
    /// Bounded contribution from the rest of the party.
    pub assistance_bps: u16,
    /// Familiarity with this locality, in basis points.
    pub familiarity_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionPrerequisites {
    pub required_terrain: Option<Terrain>,
    pub minimum_party_members: u8,
    pub requires_tracks: bool,
    pub requires_contact_referral: bool,
    pub requires_approximate_destination: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategicCost {
    pub minutes: u32,
    pub fatigue: u16,
    pub food_units: u16,
    pub water_units: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionInput {
    pub seed: u64,
    pub attempt_index: u32,
    pub kind: InvestigationActionKind,
    pub terrain: Terrain,
    pub target_terrain: Terrain,
    pub time_of_day: TimeOfDay,
    pub evidence_age_minutes: u64,
    pub current_uncertainty_bps: u16,
    pub skills: SkillContribution,
    pub weather: WeatherAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionResultKind {
    EvidenceFound,
    AreaNarrowed,
    TracksFollowed,
    TracksReacquired,
    ContactLocated,
    ObservationMade,
    AmbushPrepared,
    LeadApproached,
    NoNewInformation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub result: ActionResultKind,
    pub success: bool,
    pub cost: StrategicCost,
    pub resulting_uncertainty_bps: u16,
    pub risk_bps: u16,
    pub risk_triggered: bool,
    pub effective_skill_bps: u16,
}

#[cfg(test)]
mod planning_adapter_tests {
    use super::*;
    use crate::strategic_action::{
        ActionDefinitionId, ActionRequestId, ActionTarget, AuthorityBinding, PlanProvenance,
        ScheduledInterruption, SnapshotDigest, SnapshotRevision,
    };

    fn authority(boundary: Option<u64>) -> InvestigationPlanAuthority {
        let actor = CustodyCharacterId::try_new(7).unwrap();
        let place = StrategicPlaceId::case_site("case:site:mill").unwrap();
        let question = investigation_rights_question(actor, Some(place.clone())).unwrap();
        let rights = decide_investigation_rights(&question, true, false, 3);
        InvestigationPlanAuthority {
            coordinates: ActionCoordinates::try_new(
                actor,
                ActionTarget::Place(place.clone()),
                place,
                None,
                vec![],
            )
            .unwrap(),
            provenance: PlanProvenance {
                request_id: ActionRequestId::try_new("attempt:7").unwrap(),
                action_id: ActionDefinitionId::try_new("investigation:inspect_site").unwrap(),
                input_digest: SnapshotDigest([2; 32]),
                authority_binding: AuthorityBinding([3; 32]),
            },
            snapshot: AuthoritativeSnapshot {
                revision: SnapshotRevision(3),
                digest: SnapshotDigest([2; 32]),
            },
            current_minute: 100,
            duration: RequestedDuration::try_new(45).unwrap(),
            boundaries: TimeBoundaries {
                terminal_minute: None,
                interruption: boundary.map(|at_minute| ScheduledInterruption {
                    at_minute,
                    cause: InvestigationPlanInterruption::ParticipantBoundary,
                }),
            },
            rights,
            capability_current: true,
            live_prerequisites: true,
            generated_condition: true,
            member_ids: vec![actor, CustodyCharacterId::try_new(8).unwrap()],
            resolution: Resolution {
                result: ActionResultKind::EvidenceFound,
                success: true,
                cost: StrategicCost {
                    minutes: 45,
                    fatigue: 80,
                    food_units: 0,
                    water_units: 1,
                },
                resulting_uncertainty_bps: 1_000,
                risk_bps: 0,
                risk_triggered: false,
                effective_skill_bps: 7_000,
            },
        }
    }

    #[test]
    fn participant_boundary_preserves_full_domain_interval_but_suppresses_commit() {
        let PlanningOutcome::Ready(plan) = build_investigation_plan(authority(Some(120))) else {
            panic!("authorized plan should be ready");
        };
        assert_eq!(plan.time().elapsed_minutes, 20);
        assert_eq!(plan.effects().len(), 1);
        assert!(matches!(
            &plan.effects()[0],
            ActionEffect::Domain(InvestigationPlanEffect::AttemptPartyInterval {
                requested_minutes: 45,
                ..
            })
        ));
    }

    #[test]
    fn private_leader_approval_is_required_without_becoming_preview_data() {
        let mut input = authority(None);
        let question = investigation_rights_question(
            CustodyCharacterId::try_new(7).unwrap(),
            Some(StrategicPlaceId::case_site("case:site:mill").unwrap()),
        )
        .unwrap();
        input.rights = decide_investigation_rights(&question, false, false, 3);
        let PlanningOutcome::Rejected(rejection) = build_investigation_plan(input) else {
            panic!("unauthorized plan should be rejected");
        };
        assert_eq!(rejection.sanitized(), PublicRejection::Unavailable);
    }

    #[test]
    fn rights_digest_binds_exact_place_and_resource() {
        let actor = CustodyCharacterId::try_new(7).unwrap();
        let first = investigation_rights_question(
            actor,
            Some(StrategicPlaceId::case_site("site:first").unwrap()),
        )
        .unwrap();
        let second = investigation_rights_question(
            actor,
            Some(StrategicPlaceId::case_site("site:second").unwrap()),
        )
        .unwrap();
        let other_resource = RightsQuestion::try_new(
            RightsSubject::Character(actor),
            RightsResource::Domain(InvestigationRightsResource::CaseAction),
            RightsOperation::Domain(InvestigationRightsOperation::Perform),
            RightsJurisdiction::Place(StrategicPlaceId::case_site("site:first").unwrap()),
        )
        .unwrap();
        assert_ne!(
            investigation_rights_question_digest(&first),
            investigation_rights_question_digest(&second)
        );
        assert_ne!(
            investigation_rights_question_digest(&first),
            investigation_rights_question_digest(&other_resource)
        );
        assert_ne!(
            decide_investigation_rights(&first, true, false, 3)
                .provenance()
                .question_digest,
            decide_investigation_rights(&second, true, false, 3)
                .provenance()
                .question_digest
        );
        assert_eq!(
            investigation_rights_question_digest(&first)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "a887f5a8e7ed99122bd0464481bfd8f0769fc9c561744c54dbde692b89326594"
        );
    }
}

/// Closed planner vocabulary for a generated investigation attempt.
///
/// Capability ids, private targets, seeds, and consequence authority stay in
/// reducer-owned provenance rather than becoming a serializable target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationPlanTarget {
    GeneratedRoute,
}
impl DomainTarget for InvestigationPlanTarget {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationPlanRequirement {
    CapabilityCurrent,
    PartyAuthorized,
    LivePrerequisites,
    GeneratedCondition,
}
impl DomainRequirement for InvestigationPlanRequirement {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationPlanCapability {
    RouteResolution,
}
impl DomainCapability for InvestigationPlanCapability {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationPlanInterruption {
    ParticipantBoundary,
}
impl DomainInterruption for InvestigationPlanInterruption {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvestigationPlanEffect {
    /// The domain reducer owns the existing per-member interval algorithm.
    AttemptPartyInterval {
        member_ids: Vec<CustodyCharacterId>,
        requested_minutes: u64,
    },
    CommitResolution(Resolution),
}
impl DomainEffect for InvestigationPlanEffect {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvestigationPublicPreview {
    pub cost: StrategicCost,
}
impl PublicPreview for InvestigationPublicPreview {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationRightsSubject {}
impl DomainRightsSubject for InvestigationRightsSubject {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationRightsResource {
    PartyAction,
    CaseAction,
}
impl DomainRightsResource for InvestigationRightsResource {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationRightsOperation {
    Perform,
}
impl DomainRightsOperation for InvestigationRightsOperation {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationRightsJurisdiction {}
impl DomainJurisdiction for InvestigationRightsJurisdiction {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestigationRightsEvidence {
    PartyLeader,
    LeaderApproval,
}

pub type InvestigationRightsQuestion = RightsQuestion<
    InvestigationRightsSubject,
    InvestigationRightsResource,
    InvestigationRightsOperation,
    InvestigationRightsJurisdiction,
>;

pub fn investigation_rights_question(
    actor: CustodyCharacterId,
    place: Option<StrategicPlaceId>,
) -> Result<InvestigationRightsQuestion, RightsQuestionError> {
    RightsQuestion::try_new(
        RightsSubject::Character(actor),
        RightsResource::Domain(InvestigationRightsResource::PartyAction),
        RightsOperation::Domain(InvestigationRightsOperation::Perform),
        place.map_or(RightsJurisdiction::Global, RightsJurisdiction::Place),
    )
}

pub fn investigation_rights_question_digest(question: &InvestigationRightsQuestion) -> [u8; 32] {
    let mut digest = CanonicalRightsQuestionDigest::new(b"investigation");
    digest.frame_subject(question.subject(), |_, subject| match *subject {});
    digest.frame_resource(question.resource(), |digest, resource| {
        digest.frame(match resource {
            InvestigationRightsResource::PartyAction => b"party-action",
            InvestigationRightsResource::CaseAction => b"case-action",
        });
    });
    digest.frame_operation(question.operation(), |digest, operation| match operation {
        InvestigationRightsOperation::Perform => digest.frame(b"perform"),
    });
    digest.frame_jurisdiction(
        question.jurisdiction(),
        |_, jurisdiction| match *jurisdiction {},
    );
    digest.finish()
}

pub fn decide_investigation_rights(
    question: &InvestigationRightsQuestion,
    is_party_leader: bool,
    leader_approved: bool,
    capability_revision: u32,
) -> PrivateRightsDecision<InvestigationRightsEvidence> {
    let provenance = DecisionProvenance {
        evidence_revision: RightsRevision(u64::from(capability_revision)),
        question_digest: investigation_rights_question_digest(question),
    };
    if is_party_leader {
        PrivateRightsDecision::allowed(
            vec![InvestigationRightsEvidence::PartyLeader],
            None,
            provenance,
        )
    } else if leader_approved {
        PrivateRightsDecision::allowed(
            vec![InvestigationRightsEvidence::LeaderApproval],
            None,
            provenance,
        )
    } else {
        PrivateRightsDecision::denied(Vec::new(), provenance)
    }
}

pub struct InvestigationPlanAuthority {
    pub coordinates: ActionCoordinates<InvestigationPlanTarget>,
    pub provenance: PlanProvenance,
    pub snapshot: AuthoritativeSnapshot,
    pub current_minute: u64,
    pub duration: RequestedDuration,
    pub boundaries: TimeBoundaries<InvestigationPlanInterruption>,
    pub rights: PrivateRightsDecision<InvestigationRightsEvidence>,
    pub capability_current: bool,
    pub live_prerequisites: bool,
    pub generated_condition: bool,
    pub member_ids: Vec<CustodyCharacterId>,
    pub resolution: Resolution,
}

pub type InvestigationPlanningOutcome = PlanningOutcome<
    InvestigationPlanTarget,
    InvestigationPlanRequirement,
    InvestigationPlanCapability,
    InvestigationPlanInterruption,
    InvestigationPlanEffect,
    InvestigationPublicPreview,
>;

pub fn build_investigation_plan(
    authority: InvestigationPlanAuthority,
) -> InvestigationPlanningOutcome {
    let requirements = [
        (
            InvestigationPlanRequirement::CapabilityCurrent,
            authority.capability_current,
        ),
        (
            InvestigationPlanRequirement::PartyAuthorized,
            authority.rights.kind() == RightsDecisionKind::Allowed,
        ),
        (
            InvestigationPlanRequirement::LivePrerequisites,
            authority.live_prerequisites,
        ),
        (
            InvestigationPlanRequirement::GeneratedCondition,
            authority.generated_condition,
        ),
    ]
    .into_iter()
    .map(|(requirement, satisfied)| RequirementCheck {
        requirement: ActionRequirement::Domain(requirement),
        satisfied,
    })
    .collect();
    let resolution = authority.resolution;
    let members = authority.member_ids;
    build_plan(
        PlanInput {
            coordinates: authority.coordinates,
            provenance: authority.provenance,
            snapshot: authority.snapshot,
            current_minute: authority.current_minute,
            duration: authority.duration,
            boundaries: authority.boundaries,
            requirements,
            sanitized_rejection: PublicRejection::Unavailable,
        },
        move |_, time| {
            let mut effects = vec![ActionEffect::Domain(
                InvestigationPlanEffect::AttemptPartyInterval {
                    member_ids: members,
                    requested_minutes: u64::from(resolution.cost.minutes),
                },
            )];
            if time.permits_completion_effects() {
                effects.push(ActionEffect::Domain(
                    InvestigationPlanEffect::CommitResolution(resolution),
                ));
            }
            CalculatedAction {
                effects,
                public_preview: InvestigationPublicPreview {
                    cost: resolution.cost,
                },
            }
        },
    )
}

/// A valid generated investigation route always resolves by this attempt when
/// its uninterrupted failure history remains intact.
pub const GENERATED_ACTION_ATTEMPT_BOUND: u32 = 6;
pub const GENERATED_ACTION_PROGRESS_BPS_PER_FAILURE: u16 = 1_900;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedProgressResolution {
    pub resolution: Resolution,
    pub attempt_number: u32,
    pub persistent_progress_bps: u16,
    pub success_threshold_bps: u16,
    pub guaranteed_by_attempt: u32,
}

/// Selects the skill that governs an investigation action.
///
/// Terrain expertise is reserved for reading and recovering physical tracks.
/// Other field actions are observation problems unless they explicitly combine
/// observation with stealth.
pub const fn primary_skill_bps(kind: InvestigationActionKind, skills: SkillContribution) -> u16 {
    match kind {
        InvestigationActionKind::FollowTracks | InvestigationActionKind::ReacquireTracks => {
            skills.terrain_bps
        }
        InvestigationActionKind::LayAmbush => {
            ((skills.awareness_bps as u32 + skills.stealth_bps as u32) / 2) as u16
        }
        InvestigationActionKind::InspectSite
        | InvestigationActionKind::SearchArea
        | InvestigationActionKind::LocateContact
        | InvestigationActionKind::Watch
        | InvestigationActionKind::Patrol
        | InvestigationActionKind::ApproachLead => skills.awareness_bps,
    }
}

pub fn prerequisites(kind: InvestigationActionKind) -> ActionPrerequisites {
    use InvestigationActionKind as K;
    ActionPrerequisites {
        required_terrain: None,
        minimum_party_members: if kind == K::Patrol { 2 } else { 1 },
        requires_tracks: matches!(kind, K::FollowTracks | K::ReacquireTracks),
        requires_contact_referral: kind == K::LocateContact,
        requires_approximate_destination: matches!(kind, K::SearchArea | K::ApproachLead),
    }
}

pub fn base_cost(kind: InvestigationActionKind) -> StrategicCost {
    use InvestigationActionKind as K;
    match kind {
        K::InspectSite => StrategicCost {
            minutes: 45,
            fatigue: 80,
            food_units: 0,
            water_units: 1,
        },
        K::SearchArea => StrategicCost {
            minutes: 180,
            fatigue: 240,
            food_units: 1,
            water_units: 2,
        },
        K::FollowTracks | K::ReacquireTracks => StrategicCost {
            minutes: 120,
            fatigue: 200,
            food_units: 1,
            water_units: 2,
        },
        K::LocateContact => StrategicCost {
            minutes: 60,
            fatigue: 40,
            food_units: 0,
            water_units: 0,
        },
        K::Watch | K::LayAmbush => StrategicCost {
            minutes: 240,
            fatigue: 160,
            food_units: 1,
            water_units: 1,
        },
        K::Patrol => StrategicCost {
            minutes: 180,
            fatigue: 260,
            food_units: 1,
            water_units: 2,
        },
        K::ApproachLead => StrategicCost {
            minutes: 90,
            fatigue: 140,
            food_units: 0,
            water_units: 1,
        },
    }
}

/// Resolve with a domain-separated deterministic roll. Assistance is capped at
/// 2,000 bps so one specialist remains important.
pub fn resolve(input: ResolutionInput) -> Resolution {
    let terrain_match = if input.terrain == input.target_terrain {
        1_500
    } else {
        0
    };
    let age_penalty = (input.evidence_age_minutes / 60).min(3_000) as i32;
    let night = match (input.kind, input.time_of_day) {
        (InvestigationActionKind::Watch | InvestigationActionKind::LayAmbush, TimeOfDay::Night) => {
            500
        }
        (_, TimeOfDay::Night) => -700,
        _ => 0,
    };
    let primary = primary_skill_bps(input.kind, input.skills);
    let assistance = input.skills.assistance_bps.min(2_000);
    let weather = weather_modifier_bps(input.kind, input.weather);
    let effective = (i32::from(primary)
        + i32::from(assistance)
        + i32::from(input.skills.familiarity_bps) / 3
        + terrain_match
        + night
        + weather
        - age_penalty)
        .clamp(500, 9_500) as u16;
    let roll = domain_roll(input.seed, input.attempt_index, input.kind);
    let success = roll < effective;
    let base = base_cost(input.kind);
    let skill_time_reduction = u32::from(effective) * base.minutes / 20_000;
    let mismatch_penalty = if input.terrain == input.target_terrain {
        0
    } else {
        base.minutes / 2
    };
    let cost = StrategicCost {
        minutes: base
            .minutes
            .saturating_sub(skill_time_reduction)
            .saturating_add(mismatch_penalty)
            .max(15),
        ..base
    };
    let change = if success {
        1_800u16.saturating_add(effective / 5)
    } else {
        // A failed search still maps ground but may broaden an overconfident area.
        300
    };
    let resulting_uncertainty_bps = if success {
        input.current_uncertainty_bps.saturating_sub(change)
    } else {
        input
            .current_uncertainty_bps
            .saturating_add(change)
            .min(BASIS_POINTS_PER_WHOLE)
    };
    let risk_bps = if success { 500 } else { 1_500 };
    Resolution {
        result: result_kind(input.kind, success),
        success,
        cost,
        resulting_uncertainty_bps,
        risk_bps,
        risk_triggered: domain_roll(
            input.seed ^ 0x5249_534b_5f52_4f4c,
            input.attempt_index,
            input.kind,
        ) < risk_bps,
        effective_skill_bps: effective,
    }
}

/// Action-specific precipitation effects. Snow cover helps read tracks, while
/// active heavy precipitation penalizes actions that depend on visibility.
/// Contact-finding and approach remain social/navigation actions.
pub fn weather_modifier_bps(kind: InvestigationActionKind, weather: WeatherAuthority) -> i32 {
    let (precipitation_penalty, snow_cover, snowfall) = match weather {
        WeatherAuthority::Clear { snow_cover_bps: 0 } => (0, 0, false),
        WeatherAuthority::Clear { snow_cover_bps } => {
            (0, snow_cover_bps.min(BASIS_POINTS_PER_WHOLE), false)
        }
        WeatherAuthority::Rain {
            intensity_bps,
            snow_cover_bps,
        } => (
            -i32::from(intensity_bps.min(BASIS_POINTS_PER_WHOLE)) * 1_200
                / i32::from(BASIS_POINTS_PER_WHOLE),
            snow_cover_bps.min(BASIS_POINTS_PER_WHOLE),
            false,
        ),
        WeatherAuthority::Snow {
            intensity_bps,
            snow_cover_bps,
        } => (
            -i32::from(intensity_bps.min(BASIS_POINTS_PER_WHOLE)) * 1_000
                / i32::from(BASIS_POINTS_PER_WHOLE),
            snow_cover_bps.min(BASIS_POINTS_PER_WHOLE),
            true,
        ),
    };
    match kind {
        InvestigationActionKind::FollowTracks | InvestigationActionKind::ReacquireTracks => {
            let cover_bonus = i32::from(snow_cover) * 900 / i32::from(BASIS_POINTS_PER_WHOLE);
            // Active snowfall obscures the older prints it also makes visible.
            cover_bonus
                + if snowfall {
                    precipitation_penalty / 2
                } else {
                    0
                }
        }
        InvestigationActionKind::InspectSite
        | InvestigationActionKind::SearchArea
        | InvestigationActionKind::Watch
        | InvestigationActionKind::Patrol
        | InvestigationActionKind::LayAmbush => precipitation_penalty,
        InvestigationActionKind::LocateContact | InvestigationActionKind::ApproachLead => 0,
    }
}

/// Preserve the ordinary resolution and costs while allowing repeated work on
/// one generated route to accumulate bounded, non-transferable progress.
pub fn resolve_with_bounded_progress(
    input: ResolutionInput,
    consecutive_failures: u32,
) -> BoundedProgressResolution {
    let mut resolution = resolve(input);
    let prior = consecutive_failures.min(GENERATED_ACTION_ATTEMPT_BOUND - 1);
    let progress = u32::from(GENERATED_ACTION_PROGRESS_BPS_PER_FAILURE).saturating_mul(prior);
    let success_threshold_bps = u32::from(resolution.effective_skill_bps)
        .saturating_add(progress)
        .min(u32::from(BASIS_POINTS_PER_WHOLE)) as u16;
    let success = domain_roll(input.seed, input.attempt_index, input.kind) < success_threshold_bps;
    if success != resolution.success {
        resolution.success = success;
        resolution.result = result_kind(input.kind, success);
        resolution.risk_bps = 500;
        resolution.risk_triggered = domain_roll(
            input.seed ^ 0x5249_534b_5f52_4f4c,
            input.attempt_index,
            input.kind,
        ) < resolution.risk_bps;
    }
    resolution.resulting_uncertainty_bps = if success {
        input
            .current_uncertainty_bps
            .saturating_sub(1_800u16.saturating_add(resolution.effective_skill_bps / 5))
    } else {
        // Even an inconclusive pass maps ground for this exact route.
        input.current_uncertainty_bps.saturating_sub(300)
    };
    BoundedProgressResolution {
        resolution,
        attempt_number: prior.saturating_add(1),
        persistent_progress_bps: progress.min(u32::from(BASIS_POINTS_PER_WHOLE)) as u16,
        success_threshold_bps,
        guaranteed_by_attempt: GENERATED_ACTION_ATTEMPT_BOUND,
    }
}

fn result_kind(kind: InvestigationActionKind, success: bool) -> ActionResultKind {
    if !success {
        return ActionResultKind::NoNewInformation;
    }
    use ActionResultKind as R;
    use InvestigationActionKind as K;
    match kind {
        K::InspectSite => R::EvidenceFound,
        K::SearchArea => R::AreaNarrowed,
        K::FollowTracks => R::TracksFollowed,
        K::ReacquireTracks => R::TracksReacquired,
        K::LocateContact => R::ContactLocated,
        K::Watch | K::Patrol => R::ObservationMade,
        K::LayAmbush => R::AmbushPrepared,
        K::ApproachLead => R::LeadApproached,
    }
}

fn domain_roll(seed: u64, attempt: u32, kind: InvestigationActionKind) -> u16 {
    (mix64(
        seed ^ INVESTIGATION_ACTION_ROLL_DOMAIN
            ^ (u64::from(attempt) << INVESTIGATION_ATTEMPT_SHIFT)
            ^ ((kind as u64) << INVESTIGATION_KIND_SHIFT),
    ) % u64::from(BASIS_POINTS_PER_WHOLE)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: InvestigationActionKind) -> ResolutionInput {
        ResolutionInput {
            seed: 42,
            attempt_index: 0,
            kind,
            terrain: Terrain::Forest,
            target_terrain: Terrain::Forest,
            time_of_day: TimeOfDay::Day,
            evidence_age_minutes: 60,
            current_uncertainty_bps: 8_000,
            skills: SkillContribution {
                terrain_bps: 7_000,
                awareness_bps: 6_000,
                stealth_bps: 5_000,
                assistance_bps: 9_000,
                familiarity_bps: 3_000,
            },
            weather: WeatherAuthority::Clear { snow_cover_bps: 0 },
        }
    }

    #[test]
    fn resolution_is_deterministic_and_domain_separated() {
        let a = resolve(input(InvestigationActionKind::SearchArea));
        assert_eq!(a, resolve(input(InvestigationActionKind::SearchArea)));
        assert_ne!(a, resolve(input(InvestigationActionKind::FollowTracks)));
    }

    #[test]
    fn clear_weather_has_parity_and_precipitation_matrix_is_bounded() {
        for kind in [
            InvestigationActionKind::InspectSite,
            InvestigationActionKind::SearchArea,
            InvestigationActionKind::FollowTracks,
            InvestigationActionKind::ReacquireTracks,
            InvestigationActionKind::LocateContact,
            InvestigationActionKind::Watch,
            InvestigationActionKind::Patrol,
            InvestigationActionKind::LayAmbush,
            InvestigationActionKind::ApproachLead,
        ] {
            let old = input(kind);
            let mut clear = old;
            clear.weather = WeatherAuthority::Clear { snow_cover_bps: 0 };
            assert_eq!(resolve(old), resolve(clear));
        }
        assert!(
            weather_modifier_bps(
                InvestigationActionKind::Watch,
                WeatherAuthority::Rain {
                    intensity_bps: 10_000,
                    snow_cover_bps: 0,
                },
            ) < 0
        );
        assert!(
            weather_modifier_bps(
                InvestigationActionKind::FollowTracks,
                WeatherAuthority::Clear {
                    snow_cover_bps: 10_000,
                },
            ) > 0
        );
        assert_eq!(
            weather_modifier_bps(
                InvestigationActionKind::LocateContact,
                WeatherAuthority::Snow {
                    intensity_bps: 10_000,
                    snow_cover_bps: 10_000,
                },
            ),
            0
        );
    }

    #[test]
    fn only_track_actions_are_governed_by_terrain_skill() {
        let skills = input(InvestigationActionKind::FollowTracks).skills;
        for kind in [
            InvestigationActionKind::FollowTracks,
            InvestigationActionKind::ReacquireTracks,
        ] {
            assert_eq!(primary_skill_bps(kind, skills), skills.terrain_bps);
        }
        for kind in [
            InvestigationActionKind::InspectSite,
            InvestigationActionKind::SearchArea,
            InvestigationActionKind::LocateContact,
            InvestigationActionKind::Watch,
            InvestigationActionKind::Patrol,
            InvestigationActionKind::ApproachLead,
        ] {
            assert_eq!(primary_skill_bps(kind, skills), skills.awareness_bps);
        }
        assert_eq!(
            primary_skill_bps(InvestigationActionKind::LayAmbush, skills),
            5_500
        );
    }

    #[test]
    fn bounded_progress_preserves_first_chance_and_guarantees_sixth_attempt() {
        for (high_skill, expected_effective) in [(false, 500), (true, 9_500)] {
            let mut candidate = input(InvestigationActionKind::ReacquireTracks);
            candidate.skills.terrain_bps = if high_skill { 9_500 } else { 0 };
            candidate.skills.assistance_bps = if high_skill { 2_000 } else { 0 };
            candidate.skills.familiarity_bps = if high_skill { 3_000 } else { 0 };
            candidate.terrain = if high_skill {
                Terrain::Forest
            } else {
                Terrain::Road
            };
            candidate.target_terrain = Terrain::Forest;
            candidate.evidence_age_minutes = if high_skill { 0 } else { 600_000 };
            let mut saw_ordinary_early_success = false;
            for seed in 0..128 {
                candidate.seed = seed;
                let ordinary = resolve(candidate);
                assert_eq!(ordinary.effective_skill_bps, expected_effective);
                saw_ordinary_early_success |= ordinary.success;
                let first = resolve_with_bounded_progress(candidate, 0);
                assert_eq!(first.resolution.success, ordinary.success);
                assert_eq!(first.success_threshold_bps, ordinary.effective_skill_bps);
                assert_eq!(first.persistent_progress_bps, 0);
                let sixth = resolve_with_bounded_progress(candidate, 5);
                assert!(sixth.resolution.success);
                assert_eq!(sixth.persistent_progress_bps, 9_500);
                assert_eq!(sixth.success_threshold_bps, 10_000);
                assert_eq!(sixth.guaranteed_by_attempt, GENERATED_ACTION_ATTEMPT_BOUND);
            }
            if high_skill {
                assert!(saw_ordinary_early_success);
            }
        }
    }

    #[test]
    fn every_action_kind_keeps_its_native_first_attempt_resolution() {
        for kind in [
            InvestigationActionKind::InspectSite,
            InvestigationActionKind::SearchArea,
            InvestigationActionKind::FollowTracks,
            InvestigationActionKind::ReacquireTracks,
            InvestigationActionKind::LocateContact,
            InvestigationActionKind::Watch,
            InvestigationActionKind::Patrol,
            InvestigationActionKind::LayAmbush,
            InvestigationActionKind::ApproachLead,
        ] {
            let candidate = input(kind);
            let ordinary = resolve(candidate);
            let bounded = resolve_with_bounded_progress(candidate, 0);
            assert_eq!(bounded.resolution, ordinary);
            assert_eq!(bounded.attempt_number, 1);
            assert_eq!(bounded.persistent_progress_bps, 0);
            assert_eq!(bounded.success_threshold_bps, ordinary.effective_skill_bps);
        }
    }

    #[test]
    fn failed_bounded_attempt_truthfully_reduces_uncertainty() {
        let mut candidate = input(InvestigationActionKind::ReacquireTracks);
        candidate.skills.terrain_bps = 0;
        candidate.skills.assistance_bps = 0;
        candidate.skills.familiarity_bps = 0;
        candidate.terrain = Terrain::Road;
        candidate.evidence_age_minutes = 600_000;
        let failing_seed = (0..u64::MAX)
            .find(|seed| {
                !resolve_with_bounded_progress(
                    ResolutionInput {
                        seed: *seed,
                        ..candidate
                    },
                    0,
                )
                .resolution
                .success
            })
            .unwrap();
        candidate.seed = failing_seed;
        let failed = resolve_with_bounded_progress(candidate, 0);
        assert!(!failed.resolution.success);
        assert_eq!(failed.resolution.resulting_uncertainty_bps, 7_700);
        assert_eq!(failed.attempt_number, 1);
    }

    #[test]
    fn terrain_and_age_change_skill_time_and_uncertainty() {
        let matched = resolve(input(InvestigationActionKind::SearchArea));
        let mut poor = input(InvestigationActionKind::SearchArea);
        poor.terrain = Terrain::Road;
        poor.evidence_age_minutes = 600_000;
        let poor = resolve(poor);
        assert!(matched.effective_skill_bps > poor.effective_skill_bps);
        assert!(matched.cost.minutes < poor.cost.minutes);
    }

    #[test]
    fn assistance_is_bounded_and_weather_is_explicitly_clear() {
        let capped = resolve(input(InvestigationActionKind::InspectSite));
        let mut exact_cap = input(InvestigationActionKind::InspectSite);
        exact_cap.skills.assistance_bps = 2_000;
        assert_eq!(capped, resolve(exact_cap));
        assert_eq!(
            exact_cap.weather,
            WeatherAuthority::Clear { snow_cover_bps: 0 }
        );
    }

    #[test]
    fn failures_never_delete_the_route() {
        let mut hard = input(InvestigationActionKind::ReacquireTracks);
        hard.seed = 1;
        hard.skills.terrain_bps = 0;
        hard.skills.assistance_bps = 0;
        hard.skills.familiarity_bps = 0;
        hard.evidence_age_minutes = u64::MAX;
        hard.current_uncertainty_bps = 9_800;
        let result = resolve(hard);
        assert!(!result.success);
        assert_eq!(result.result, ActionResultKind::NoNewInformation);
        assert!(result.resulting_uncertainty_bps <= 10_000);
        assert!(result.cost.minutes > 0);
    }

    #[test]
    fn risk_is_deterministic_and_distinct_from_action_success() {
        let input = input(InvestigationActionKind::Watch);
        let first = resolve(input);
        let second = resolve(input);
        assert_eq!(first.risk_triggered, second.risk_triggered);
        assert!(first.risk_bps <= 10_000);
    }

    #[test]
    fn tracking_edges_accept_area_to_route_to_site_chain() {
        assert!(tracking_route_edge_is_coherent(
            InvestigationActionKind::ReacquireTracks,
            InvestigationTargetKind::Route,
            InvestigationActionKind::SearchArea,
            InvestigationTargetKind::Area,
        ));
        assert!(tracking_route_edge_is_coherent(
            InvestigationActionKind::FollowTracks,
            InvestigationTargetKind::Site,
            InvestigationActionKind::ReacquireTracks,
            InvestigationTargetKind::Route,
        ));
    }

    #[test]
    fn tracking_edges_reject_crossed_route_provenance() {
        assert!(!tracking_route_edge_is_coherent(
            InvestigationActionKind::FollowTracks,
            InvestigationTargetKind::Site,
            InvestigationActionKind::SearchArea,
            InvestigationTargetKind::Area,
        ));
        assert!(!tracking_route_edge_is_coherent(
            InvestigationActionKind::ReacquireTracks,
            InvestigationTargetKind::Route,
            InvestigationActionKind::FollowTracks,
            InvestigationTargetKind::Site,
        ));
    }

    #[test]
    fn target_kind_serialization_is_canonical_and_rejects_unknown_values() {
        assert_eq!(
            serde_json::to_string(&InvestigationTargetKind::Tracks).unwrap(),
            "\"tracks\""
        );
        assert!(serde_json::from_str::<InvestigationTargetKind>("\"corpse\"").is_err());
    }

    #[test]
    fn action_availability_serialization_is_typed_and_canonical() {
        let unavailable = InvestigationActionAvailability::unavailable(
            InvestigationActionUnavailableReason::TravelRequired,
            true,
            0,
        );
        assert_eq!(
            serde_json::to_value(unavailable).unwrap(),
            serde_json::json!({
                "unavailable": {
                    "reason": "travel_required",
                    "can_travel_to_required_site": true,
                    "wait_minutes": 0
                }
            })
        );
        assert!(InvestigationActionAvailability::Available.is_available());
        assert!(!unavailable.is_available());
        assert_eq!(
            serde_json::from_value::<InvestigationActionAvailability>(
                serde_json::to_value(unavailable).unwrap()
            )
            .unwrap(),
            unavailable
        );
        assert!(
            serde_json::from_str::<InvestigationActionAvailability>("\"wording_changed\"").is_err()
        );
    }
}
