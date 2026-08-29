//! Shared physical-preparation and tincture timing constants.

pub const BASE_CUT_MINUTES: u32 = 10;
pub const BASE_GRIND_MINUTES: u32 = 20;
pub const CHECK_TIME_REDUCTION_PER_RANK: f32 = 0.06;
pub const GRINDING_TOOL_TIME_FACTOR: f32 = 0.50;
pub const POPPY_TINCTURE_MATURATION_MINUTES: u64 = 60_480;
pub const POPPY_TINCTURE_HERB_GRAMS: u32 = 50;
pub const POPPY_TINCTURE_SPIRIT_ML: u32 = 150;

use crate::{
    material::{
        DomainConservationPolicy, DomainContaminant, DomainMaterialComponent,
        DomainMaterialProcess, DomainMaterialReceipt, DomainPreparation,
        PublicMaterialPresentation,
    },
    physical_object::CustodyCharacterId,
    rights::{
        CanonicalRightsQuestionDigest, DecisionProvenance, DomainJurisdiction,
        DomainRightsOperation, DomainRightsResource, DomainRightsSubject, PrivateRightsDecision,
        RightsDecisionKind, RightsJurisdiction, RightsOperation, RightsQuestion, RightsResource,
        RightsRevision, RightsSubject,
    },
    strategic_action::{
        ActionCoordinates, ActionEffect, ActionRequirement, AuthoritativeSnapshot,
        CalculatedAction, DomainCapability, DomainEffect, DomainInterruption, DomainRequirement,
        DomainTarget, PlanInput, PlanProvenance, PlanningOutcome, PublicPreview, PublicRejection,
        RequestedDuration, RequirementCheck, TimeBoundaries, build_plan,
    },
    strategic_place::StrategicPlaceId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationAction {
    Cut,
    Grind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngredientMaterialPreparation {}
impl DomainPreparation for IngredientMaterialPreparation {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngredientMaterialProcess {}
impl DomainMaterialProcess for IngredientMaterialProcess {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MedicinalMaterialComponent {
    pub intervention_profile_id: String,
    pub profile_version: u16,
}
impl DomainMaterialComponent for MedicinalMaterialComponent {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngredientContaminant {
    Microbial,
}
impl DomainContaminant for IngredientContaminant {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationConservationPolicy {
    Exact,
}
impl DomainConservationPolicy for PreparationConservationPolicy {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparationMaterialReceipt {
    pub action: PreparationAction,
}
impl DomainMaterialReceipt for PreparationMaterialReceipt {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparationMaterialPresentation {
    pub action: PreparationAction,
    pub expected_revision: u64,
}
impl PublicMaterialPresentation for PreparationMaterialPresentation {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationPlanTarget {
    IngredientLot,
}
impl DomainTarget for PreparationPlanTarget {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationPlanRequirement {
    StableObjectCustody,
    LotRevisionCurrent,
    PreparationTransition,
    RequiredTool,
    RightsAllowed,
}
impl DomainRequirement for PreparationPlanRequirement {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationPlanCapability {
    GoverningSkill,
}
impl DomainCapability for PreparationPlanCapability {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationPlanInterruption {
    CharacterBoundary,
}
impl DomainInterruption for PreparationPlanInterruption {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparationPlanEffect {
    AttemptWait {
        actor: CustodyCharacterId,
        requested_minutes: u64,
    },
    CommitPreparation {
        action: PreparationAction,
        expected_revision: u64,
        next_display_name: String,
    },
}
impl DomainEffect for PreparationPlanEffect {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparationPublicPreview {
    pub action: PreparationAction,
    pub expected_revision: u64,
    pub duration_minutes: u32,
}
impl PublicPreview for PreparationPublicPreview {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationRightsSubject {}
impl DomainRightsSubject for PreparationRightsSubject {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationRightsResource {}
impl DomainRightsResource for PreparationRightsResource {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationRightsOperation {}
impl DomainRightsOperation for PreparationRightsOperation {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationRightsJurisdiction {}
impl DomainJurisdiction for PreparationRightsJurisdiction {}

pub type PreparationRightsQuestion = RightsQuestion<
    PreparationRightsSubject,
    PreparationRightsResource,
    PreparationRightsOperation,
    PreparationRightsJurisdiction,
>;

pub fn preparation_rights_question(
    actor: CustodyCharacterId,
    object_id: crate::physical_object::PhysicalObjectId,
    place: StrategicPlaceId,
) -> Result<PreparationRightsQuestion, crate::rights::RightsQuestionError> {
    RightsQuestion::try_new(
        RightsSubject::Character(actor),
        RightsResource::Object(object_id),
        RightsOperation::Alter,
        RightsJurisdiction::Place(place),
    )
}

pub fn decide_preparation_rights(
    question: &PreparationRightsQuestion,
    custody_matches: bool,
    revision: u64,
) -> PrivateRightsDecision<()> {
    let provenance = DecisionProvenance {
        evidence_revision: RightsRevision(revision),
        question_digest: preparation_rights_question_digest(question),
    };
    if custody_matches {
        PrivateRightsDecision::allowed(Vec::new(), None, provenance)
    } else {
        PrivateRightsDecision::denied(Vec::new(), provenance)
    }
}

fn preparation_rights_question_digest(question: &PreparationRightsQuestion) -> [u8; 32] {
    let mut digest = CanonicalRightsQuestionDigest::new(b"ingredient-preparation");
    digest.frame_subject(question.subject(), |_, subject| match *subject {});
    digest.frame_resource(question.resource(), |_, resource| match *resource {});
    digest.frame_operation(question.operation(), |_, operation| match *operation {});
    digest.frame_jurisdiction(
        question.jurisdiction(),
        |_, jurisdiction| match *jurisdiction {},
    );
    digest.finish()
}

pub struct PreparationPlanAuthority {
    pub coordinates: ActionCoordinates<PreparationPlanTarget>,
    pub provenance: PlanProvenance,
    pub snapshot: AuthoritativeSnapshot,
    pub current_minute: u64,
    pub duration: RequestedDuration,
    /// Exact terminal boundary hydrated by the authoritative persistence
    /// adapter. A boundary inside the requested interval makes the plan
    /// wait-only; completion effects must never be invented by the reducer.
    pub terminal_minute: Option<u64>,
    pub rights: PrivateRightsDecision<()>,
    pub custody_matches: bool,
    pub revision_current: bool,
    pub transition_allowed: bool,
    pub required_tool_available: bool,
    pub action: PreparationAction,
    pub expected_revision: u64,
    pub next_display_name: String,
}

pub type PreparationPlanningOutcome = PlanningOutcome<
    PreparationPlanTarget,
    PreparationPlanRequirement,
    PreparationPlanCapability,
    PreparationPlanInterruption,
    PreparationPlanEffect,
    PreparationPublicPreview,
>;

pub fn build_preparation_plan(authority: PreparationPlanAuthority) -> PreparationPlanningOutcome {
    let requirements = [
        (
            PreparationPlanRequirement::StableObjectCustody,
            authority.custody_matches,
        ),
        (
            PreparationPlanRequirement::LotRevisionCurrent,
            authority.revision_current,
        ),
        (
            PreparationPlanRequirement::PreparationTransition,
            authority.transition_allowed,
        ),
        (
            PreparationPlanRequirement::RequiredTool,
            authority.required_tool_available,
        ),
        (
            PreparationPlanRequirement::RightsAllowed,
            authority.rights.kind() == RightsDecisionKind::Allowed,
        ),
    ]
    .into_iter()
    .map(|(requirement, satisfied)| RequirementCheck {
        requirement: ActionRequirement::Domain(requirement),
        satisfied,
    })
    .collect();
    let actor = authority.coordinates.actor();
    let action = authority.action;
    let revision = authority.expected_revision;
    let duration = authority.duration.minutes();
    let next_display_name = authority.next_display_name;
    build_plan(
        PlanInput {
            coordinates: authority.coordinates,
            provenance: authority.provenance,
            snapshot: authority.snapshot,
            current_minute: authority.current_minute,
            duration: authority.duration,
            boundaries: TimeBoundaries {
                terminal_minute: authority.terminal_minute,
                interruption: None,
            },
            requirements,
            sanitized_rejection: PublicRejection::Unavailable,
        },
        move |_, time| {
            let mut effects = vec![ActionEffect::Domain(PreparationPlanEffect::AttemptWait {
                actor,
                requested_minutes: duration,
            })];
            if time.permits_completion_effects() {
                effects.push(ActionEffect::Domain(
                    PreparationPlanEffect::CommitPreparation {
                        action,
                        expected_revision: revision,
                        next_display_name,
                    },
                ));
            }
            CalculatedAction {
                effects,
                public_preview: PreparationPublicPreview {
                    action,
                    expected_revision: revision,
                    duration_minutes: duration.min(u64::from(u32::MAX)) as u32,
                },
            }
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalPreparation {
    Cut,
    Ground,
}

pub fn physical_preparation_minutes(
    preparation: PhysicalPreparation,
    governing_check: f32,
    has_grinding_tool: bool,
) -> u32 {
    let base = match preparation {
        PhysicalPreparation::Cut => BASE_CUT_MINUTES,
        PhysicalPreparation::Ground => BASE_GRIND_MINUTES,
    };
    let check = if governing_check.is_finite() {
        governing_check.clamp(0.0, 5.0)
    } else {
        0.0
    };
    let tool = if preparation == PhysicalPreparation::Ground && has_grinding_tool {
        GRINDING_TOOL_TIME_FACTOR
    } else {
        1.0
    };
    ((base as f32) * (1.0 - CHECK_TIME_REDUCTION_PER_RANK * check) * tool)
        .ceil()
        .max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grinding_tool_halves_time_and_skill_uses_canonical_check() {
        assert_eq!(
            physical_preparation_minutes(PhysicalPreparation::Ground, 0.0, false),
            20
        );
        assert_eq!(
            physical_preparation_minutes(PhysicalPreparation::Ground, 0.0, true),
            10
        );
        assert!(
            physical_preparation_minutes(PhysicalPreparation::Cut, 5.0, false) < BASE_CUT_MINUTES
        );
    }

    #[test]
    fn terminal_preparation_plan_is_wait_only() {
        use crate::{
            physical_object::{CustodyCharacterId, PhysicalObjectId},
            strategic_action::{
                ActionCoordinates, ActionDefinitionId, ActionRequestId, ActionTarget,
                AuthoritativeSnapshot, AuthorityBinding, PlanProvenance, PlanningOutcome,
                RequestedDuration, SnapshotDigest, SnapshotRevision,
            },
            strategic_place::StrategicPlaceId,
        };

        let actor = CustodyCharacterId::try_new(1).unwrap();
        let object = PhysicalObjectId::try_new(2).unwrap();
        let place = StrategicPlaceId::settlement("ironforge").unwrap();
        let coordinates = ActionCoordinates::try_new(
            actor,
            ActionTarget::Object(object),
            place.clone(),
            None,
            Vec::new(),
        )
        .unwrap();
        let question = preparation_rights_question(actor, object, place).unwrap();
        assert_eq!(
            preparation_rights_question_digest(&question)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "1220f57a7a34db9665407f6ebb16a1f04f07db5f0869defa47536fe492183613"
        );
        let digest = [3; 32];
        let PlanningOutcome::Ready(plan) = build_preparation_plan(PreparationPlanAuthority {
            coordinates,
            provenance: PlanProvenance {
                request_id: ActionRequestId::try_new("terminal-preparation").unwrap(),
                action_id: ActionDefinitionId::try_new("ingredient-preparation:cut").unwrap(),
                input_digest: SnapshotDigest(digest),
                authority_binding: AuthorityBinding(digest),
            },
            snapshot: AuthoritativeSnapshot {
                revision: SnapshotRevision(1),
                digest: SnapshotDigest(digest),
            },
            current_minute: 100,
            duration: RequestedDuration::try_new(10).unwrap(),
            terminal_minute: Some(105),
            rights: decide_preparation_rights(&question, true, 1),
            custody_matches: true,
            revision_current: true,
            transition_allowed: true,
            required_tool_available: true,
            action: PreparationAction::Cut,
            expected_revision: 1,
            next_display_name: "Cut willow bark".into(),
        }) else {
            panic!("terminal preparation should remain a valid wait-only plan");
        };
        assert!(plan.effects().iter().any(|effect| matches!(
            effect,
            crate::strategic_action::ActionEffect::Domain(
                PreparationPlanEffect::AttemptWait { .. }
            )
        )));
        assert!(!plan.effects().iter().any(|effect| matches!(
            effect,
            crate::strategic_action::ActionEffect::Domain(
                PreparationPlanEffect::CommitPreparation { .. }
            )
        )));
    }
}
