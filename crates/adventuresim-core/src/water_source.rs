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
use std::collections::BTreeMap;

/// Small deterministic state machine used by the strategic simulator to prove
/// the complete private-material path without requiring a database runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutbreakWaterFlow {
    pub source_open: bool,
    pub source_ml: u64,
    pub holding_ml: u64,
    pub holding_load_microunits: u64,
    pub food_ml: u64,
    pub food_load_microunits: u64,
    pub consumed_load_microunits: u64,
    pub contribution_digest: Option<String>,
    receipts: BTreeMap<String, (u64, u64, u64)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutbreakWaterFlowError {
    Closed,
    InsufficientWater,
    ReplayCollision,
    InvalidFraction,
}

impl OutbreakWaterFlow {
    pub fn new(source_ml: u64) -> Self {
        Self {
            source_open: true,
            source_ml,
            holding_ml: 0,
            holding_load_microunits: 0,
            food_ml: 0,
            food_load_microunits: 0,
            consumed_load_microunits: 0,
            contribution_digest: None,
            receipts: BTreeMap::new(),
        }
    }

    pub fn public_projection(&self) -> (bool, u64, u64, u64) {
        (
            self.source_open,
            self.source_ml,
            self.holding_ml,
            self.food_ml,
        )
    }

    pub fn draw(
        &mut self,
        request_id: &str,
        amount_ml: u64,
        load_microunits: u64,
    ) -> Result<(u64, u64, u64), OutbreakWaterFlowError> {
        if let Some(receipt) = self.receipts.get(request_id) {
            return (receipt.0 == amount_ml && receipt.1 == load_microunits)
                .then_some(*receipt)
                .ok_or(OutbreakWaterFlowError::ReplayCollision);
        }
        if !self.source_open {
            return Err(OutbreakWaterFlowError::Closed);
        }
        let Some((source_after, holding_after)) =
            conserved_collection(self.source_ml, self.holding_ml, amount_ml)
        else {
            return Err(OutbreakWaterFlowError::InsufficientWater);
        };
        self.source_ml = source_after;
        self.holding_ml = holding_after;
        self.holding_load_microunits = self
            .holding_load_microunits
            .checked_add(load_microunits)
            .ok_or(OutbreakWaterFlowError::InsufficientWater)?;
        let receipt = (amount_ml, load_microunits, holding_after);
        self.receipts.insert(request_id.into(), receipt);
        Ok(receipt)
    }

    pub fn transfer_all(&mut self, destination: &mut Self) {
        destination.holding_ml += self.holding_ml;
        destination.holding_load_microunits += self.holding_load_microunits;
        self.holding_ml = 0;
        self.holding_load_microunits = 0;
    }

    pub fn cook_all(&mut self, kill_numerator: u64, kill_denominator: u64) {
        self.food_ml += self.holding_ml;
        self.food_load_microunits +=
            self.holding_load_microunits * kill_numerator / kill_denominator;
        self.holding_ml = 0;
        self.holding_load_microunits = 0;
        self.contribution_digest = Some(flow_digest(self.food_ml, self.food_load_microunits));
    }

    pub fn split_food(
        &mut self,
        numerator: u64,
        denominator: u64,
    ) -> Result<Self, OutbreakWaterFlowError> {
        if denominator == 0 || numerator > denominator {
            return Err(OutbreakWaterFlowError::InvalidFraction);
        }
        let child_ml = self.food_ml * numerator / denominator;
        let child_load = self.food_load_microunits * numerator / denominator;
        self.food_ml -= child_ml;
        self.food_load_microunits -= child_load;
        self.contribution_digest = Some(flow_digest(self.food_ml, self.food_load_microunits));
        let mut child = Self::new(0);
        child.source_open = false;
        child.food_ml = child_ml;
        child.food_load_microunits = child_load;
        child.contribution_digest = Some(flow_digest(child_ml, child_load));
        Ok(child)
    }

    pub fn consume_all(&mut self) -> Option<crate::world_event::WorldEventEnvelope> {
        let digest = self.contribution_digest.clone()?;
        let dose = self.food_load_microunits;
        self.consumed_load_microunits += dose;
        self.food_ml = 0;
        self.food_load_microunits = 0;
        Some(crate::world_event::WorldEventEnvelope {
            schema_revision: crate::world_event::WORLD_EVENT_SCHEMA_REVISION,
            id: "food-water-infection:1:simulated-consumption".into(),
            source: crate::world_event::WorldEventSource::FoodWaterExposure {
                consumption_id: "simulated-consumption".into(),
            },
            actor: crate::world_event::WorldEventActor::Character { character_id: 1 },
            subjects: vec![crate::world_event::WorldEventSubject::Character { character_id: 1 }],
            place: crate::world_event::WorldEventPlace::Strategic {
                place_id: "simulated-well".into(),
            },
            occurred_at_minute: 1,
            payload: crate::world_event::WorldEventPayloadRef::FoodWaterInfection {
                carrier_id: 1,
                contribution_digest: digest,
                dose_microunits: dose,
                protected_dose_microunits: dose,
                immunity_milli: 3_000,
                prior_immunity_milli: 0,
                consumed_fraction_bps: 10_000,
                disease_id: "dysentery".into(),
            },
        })
    }

    pub fn close_source(&mut self) {
        self.source_open = false;
    }
}

fn flow_digest(amount_ml: u64, load_microunits: u64) -> String {
    let digest = Sha256::digest([amount_ml.to_le_bytes(), load_microunits.to_le_bytes()].concat());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

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

/// Sample one private material contribution by the same fraction as the
/// public holding transfer. Integer remainders stay with the source.
pub fn proportional_material_transfer(
    public_total_microliters: u64,
    moved_microliters: u64,
    contribution_microliters: u64,
    contaminant_load_microunits: u64,
) -> Option<(u64, u64)> {
    if public_total_microliters == 0 || moved_microliters > public_total_microliters {
        return None;
    }
    if moved_microliters == public_total_microliters {
        return Some((contribution_microliters, contaminant_load_microunits));
    }
    let amount = (u128::from(contribution_microliters) * u128::from(moved_microliters)
        / u128::from(public_total_microliters)) as u64;
    let load = if amount == contribution_microliters {
        contaminant_load_microunits
    } else if contribution_microliters == 0 {
        0
    } else {
        (u128::from(contaminant_load_microunits) * u128::from(amount)
            / u128::from(contribution_microliters)) as u64
    };
    Some((amount, load))
}

#[cfg(test)]
mod tests {
    use super::{conserved_collection, proportional_material_transfer};

    #[test]
    fn collection_conserves_exact_integer_volume() {
        assert_eq!(conserved_collection(10_000, 250, 750), Some((9_250, 1_000)));
        assert_eq!(conserved_collection(100, 0, 101), None);
        assert_eq!(conserved_collection(100, 0, 0), None);
    }

    #[test]
    fn transfer_samples_tainted_and_implicit_clean_water_proportionally() {
        assert_eq!(
            proportional_material_transfer(1_000_000, 100_000, 100_000, 12_000_000),
            Some((10_000, 1_200_000))
        );
        assert_eq!(
            proportional_material_transfer(900_000, 900_000, 10_000, 1_200_000),
            Some((10_000, 1_200_000))
        );
    }
}
