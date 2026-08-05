//! Pure shared planning and conservation rules for collecting fixture water.

use crate::{
    material::MaterialLotId,
    physical_object::{CustodyCharacterId, PhysicalObjectId},
    rights::{
        DecisionProvenance, DomainJurisdiction, DomainRightsOperation, DomainRightsResource,
        DomainRightsSubject, PrivateRightsDecision, RightsJurisdiction, RightsOperation,
        RightsQuestion, RightsResource, RightsRevision, RightsSubject,
    },
    strategic_action::{
        ActionCoordinates, ActionEffect, ActionRequirement, CalculatedAction, DomainCapability,
        DomainEffect, DomainInterruption, DomainRequirement, DomainTarget, PlanInput,
        PlanningOutcome, PublicPreview, RequirementCheck, build_plan,
    },
    strategic_place::StrategicFixtureId,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterRightsSubject {}
impl DomainRightsSubject for WaterRightsSubject {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterRightsResource {}
impl DomainRightsResource for WaterRightsResource {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterRightsOperation {
    Collect,
}
impl DomainRightsOperation for WaterRightsOperation {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterRightsJurisdiction {}
impl DomainJurisdiction for WaterRightsJurisdiction {}

pub type WaterRightsQuestion = RightsQuestion<
    WaterRightsSubject,
    WaterRightsResource,
    WaterRightsOperation,
    WaterRightsJurisdiction,
>;

pub fn water_collection_question(
    actor: CustodyCharacterId,
    fixture: StrategicFixtureId,
) -> Result<WaterRightsQuestion, crate::rights::RightsQuestionError> {
    RightsQuestion::try_new(
        RightsSubject::Character(actor),
        RightsResource::Fixture(fixture.clone()),
        RightsOperation::Domain(WaterRightsOperation::Collect),
        RightsJurisdiction::Place(fixture.place().clone()),
    )
}

pub fn water_container_alter_question(
    actor: CustodyCharacterId,
    object: PhysicalObjectId,
    place: crate::strategic_place::StrategicPlaceId,
) -> Result<WaterRightsQuestion, crate::rights::RightsQuestionError> {
    RightsQuestion::try_new(
        RightsSubject::Character(actor),
        RightsResource::Object(object),
        RightsOperation::Alter,
        RightsJurisdiction::Place(place),
    )
}

pub fn decide_public_water_collection(
    question: &WaterRightsQuestion,
    source_open: bool,
    revision: u64,
) -> PrivateRightsDecision<()> {
    let mut hash = Sha256::new();
    hash.update(b"water-collection-rights-v1");
    hash.update(format!("{question:?}").as_bytes());
    let provenance = DecisionProvenance {
        evidence_revision: RightsRevision(revision),
        question_digest: hash.finalize().into(),
    };
    if source_open {
        PrivateRightsDecision::allowed(Vec::new(), None, provenance)
    } else {
        PrivateRightsDecision::denied(Vec::new(), provenance)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WaterCollectionTarget {}
impl DomainTarget for WaterCollectionTarget {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WaterCollectionRequirement {
    ExactPresence,
    SourceAvailable,
    ContainerCustody,
    Mutable,
    RightsAllowed,
    Capacity,
    MaterialCompatible,
}
impl DomainRequirement for WaterCollectionRequirement {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WaterCollectionCapability {}
impl DomainCapability for WaterCollectionCapability {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WaterCollectionInterruption {}
impl DomainInterruption for WaterCollectionInterruption {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaterCollectionEffect {
    pub container_object_id: PhysicalObjectId,
    pub material_lot_id: MaterialLotId,
    pub amount_ml: u64,
}
impl DomainEffect for WaterCollectionEffect {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaterCollectionPreview {
    pub amount_ml: u64,
}
impl PublicPreview for WaterCollectionPreview {}

pub type WaterCollectionPlanningOutcome = PlanningOutcome<
    WaterCollectionTarget,
    WaterCollectionRequirement,
    WaterCollectionCapability,
    WaterCollectionInterruption,
    WaterCollectionEffect,
    WaterCollectionPreview,
>;

pub struct WaterCollectionAuthority {
    pub coordinates: ActionCoordinates<WaterCollectionTarget>,
    pub plan: PlanInput<
        WaterCollectionTarget,
        WaterCollectionRequirement,
        WaterCollectionCapability,
        WaterCollectionInterruption,
    >,
    pub container_object_id: PhysicalObjectId,
    pub material_lot_id: MaterialLotId,
    pub amount_ml: u64,
    pub rights_allowed: bool,
    pub exact_presence: bool,
    pub source_available: bool,
    pub container_custody: bool,
    pub mutable: bool,
    pub capacity_available: bool,
    pub material_compatible: bool,
}

pub fn build_water_collection_plan(
    mut authority: WaterCollectionAuthority,
) -> WaterCollectionPlanningOutcome {
    authority.plan.coordinates = authority.coordinates;
    authority.plan.requirements = [
        (
            WaterCollectionRequirement::ExactPresence,
            authority.exact_presence,
        ),
        (
            WaterCollectionRequirement::SourceAvailable,
            authority.source_available,
        ),
        (
            WaterCollectionRequirement::ContainerCustody,
            authority.container_custody,
        ),
        (WaterCollectionRequirement::Mutable, authority.mutable),
        (
            WaterCollectionRequirement::RightsAllowed,
            authority.rights_allowed,
        ),
        (
            WaterCollectionRequirement::Capacity,
            authority.capacity_available,
        ),
        (
            WaterCollectionRequirement::MaterialCompatible,
            authority.material_compatible,
        ),
    ]
    .into_iter()
    .map(|(requirement, satisfied)| RequirementCheck {
        requirement: ActionRequirement::Domain(requirement),
        satisfied,
    })
    .collect();
    let effect = WaterCollectionEffect {
        container_object_id: authority.container_object_id,
        material_lot_id: authority.material_lot_id,
        amount_ml: authority.amount_ml,
    };
    build_plan(authority.plan, move |coordinates, time| CalculatedAction {
        effects: [
            Some(ActionEffect::AdvanceActorTime {
                actor: coordinates.actor(),
                from_minute: time.start_minute,
                to_minute: time.end_minute,
            }),
            time.permits_completion_effects()
                .then_some(ActionEffect::Domain(effect)),
        ]
        .into_iter()
        .flatten()
        .collect(),
        public_preview: WaterCollectionPreview {
            amount_ml: authority.amount_ml,
        },
    })
}

pub fn conserved_collection(
    source_before: u64,
    container_before: u64,
    amount: u64,
) -> Option<(u64, u64)> {
    (amount > 0 && amount <= source_before)
        .then(|| {
            container_before
                .checked_add(amount)
                .map(|after| (source_before - amount, after))
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::conserved_collection;

    #[test]
    fn collection_conserves_exact_integer_volume() {
        assert_eq!(conserved_collection(10_000, 250, 750), Some((9_250, 1_000)));
        assert_eq!(conserved_collection(100, 0, 101), None);
        assert_eq!(conserved_collection(100, 0, 0), None);
    }
}
