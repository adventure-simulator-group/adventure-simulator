//! Pure physical-material identity, measurement, process, and conservation laws.
//!
//! This module has no persistence or transport surface. Reducers remain the
//! authority for custody, capacity, recipes, mutations, and private truth.

use std::{fmt, num::NonZeroU64};

use sha2::{Digest, Sha256};

use crate::physical_object::{
    CustodyIdentityError, ObjectCustody, OperationalCustody, PhysicalObjectId,
};

pub trait DomainPreparation: Clone + fmt::Debug + Eq {}
pub trait DomainMaterialProcess: Clone + fmt::Debug + Eq {}
pub trait DomainMaterialComponent: Clone + fmt::Debug + Eq {}
pub trait DomainContaminant: Clone + fmt::Debug + Eq {}
pub trait DomainConservationPolicy: Clone + fmt::Debug + Eq {}
pub trait DomainMaterialReceipt: Clone + fmt::Debug + Eq {}
pub trait PublicMaterialPresentation: Clone + fmt::Debug + Eq {}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MaterialLotId(NonZeroU64);

impl MaterialLotId {
    pub fn try_new(value: u64) -> Result<Self, MaterialError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(MaterialError::ZeroLotId)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MaterialProcessId(NonZeroU64);

impl MaterialProcessId {
    pub fn try_new(value: u64) -> Result<Self, MaterialError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(MaterialError::ZeroProcessId)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Stable material-lot identity connected to its stable inventory object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialIdentity {
    lot_id: MaterialLotId,
    object: ObjectCustody,
}

impl MaterialIdentity {
    pub fn try_new(
        lot_id: MaterialLotId,
        object_id: PhysicalObjectId,
        custody: OperationalCustody,
    ) -> Result<Self, CustodyIdentityError> {
        Ok(Self {
            lot_id,
            object: ObjectCustody::try_new(object_id, custody)?,
        })
    }

    pub const fn lot_id(&self) -> MaterialLotId {
        self.lot_id
    }
    pub const fn object_id(&self) -> PhysicalObjectId {
        self.object.object_id()
    }
    pub const fn custody(&self) -> &OperationalCustody {
        self.object.custody()
    }
}

/// Exact extensive totals in game-wide integer subunits.
///
/// Mass is milligrams and volume is microliters. A solid may have zero known
/// volume; a liquid still carries mass. A material lot cannot be empty.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaterialMeasure {
    mass_milligrams: u64,
    volume_microliters: u64,
}

impl MaterialMeasure {
    pub const ZERO: Self = Self {
        mass_milligrams: 0,
        volume_microliters: 0,
    };

    pub fn try_new(mass_milligrams: u64, volume_microliters: u64) -> Result<Self, MaterialError> {
        if mass_milligrams == 0 && volume_microliters == 0 {
            return Err(MaterialError::EmptyMeasure);
        }
        Ok(Self {
            mass_milligrams,
            volume_microliters,
        })
    }

    pub const fn mass_milligrams(self) -> u64 {
        self.mass_milligrams
    }
    pub const fn volume_microliters(self) -> u64 {
        self.volume_microliters
    }

    pub fn checked_add(self, other: Self) -> Result<Self, MaterialError> {
        Ok(Self {
            mass_milligrams: self
                .mass_milligrams
                .checked_add(other.mass_milligrams)
                .ok_or(MaterialError::Overflow)?,
            volume_microliters: self
                .volume_microliters
                .checked_add(other.volume_microliters)
                .ok_or(MaterialError::Overflow)?,
        })
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, MaterialError> {
        Ok(Self {
            mass_milligrams: self
                .mass_milligrams
                .checked_sub(other.mass_milligrams)
                .ok_or(MaterialError::Underflow)?,
            volume_microliters: self
                .volume_microliters
                .checked_sub(other.volume_microliters)
                .ok_or(MaterialError::Underflow)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Portion {
    numerator: NonZeroU64,
    denominator: NonZeroU64,
}

impl Portion {
    pub fn try_new(numerator: u64, denominator: u64) -> Result<Self, MaterialError> {
        let numerator = NonZeroU64::new(numerator).ok_or(MaterialError::ZeroPortion)?;
        let denominator = NonZeroU64::new(denominator).ok_or(MaterialError::ZeroDenominator)?;
        if numerator > denominator {
            return Err(MaterialError::PortionExceedsWhole);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub fn split_measure(
        self,
        whole: MaterialMeasure,
    ) -> Result<(MaterialMeasure, Option<MaterialMeasure>), MaterialError> {
        let take = MaterialMeasure {
            mass_milligrams: prorated_floor(
                whole.mass_milligrams,
                self.numerator.get(),
                self.denominator.get(),
            )?,
            volume_microliters: prorated_floor(
                whole.volume_microliters,
                self.numerator.get(),
                self.denominator.get(),
            )?,
        };
        if take == MaterialMeasure::ZERO {
            return Err(MaterialError::EmptyPortion);
        }
        let remainder = whole.checked_sub(take)?;
        Ok((
            take,
            (remainder != MaterialMeasure::ZERO).then_some(remainder),
        ))
    }
}

fn prorated_floor(value: u64, numerator: u64, denominator: u64) -> Result<u64, MaterialError> {
    let scaled = u128::from(value)
        .checked_mul(u128::from(numerator))
        .ok_or(MaterialError::Overflow)?
        / u128::from(denominator);
    u64::try_from(scaled).map_err(|_| MaterialError::Overflow)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterialPreparation<P: DomainPreparation> {
    Raw,
    Cut,
    Ground,
    Cooked(CookingLane),
    Tincture,
    Domain(P),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CookingLane {
    Roast,
    PanFry,
    Stew,
    Bake,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterialProcessKind<D: DomainMaterialProcess> {
    Cut,
    Grind,
    Combine,
    Cook(CookingLane),
    Tincture,
    Pour,
    Wash,
    Administer,
    Consume,
    Domain(D),
}

/// One stable vessel object, including its exact current custody and authored
/// liquid capacity. The vessel is never a parallel process-only identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialVessel {
    object: ObjectCustody,
    capacity_microliters: NonZeroU64,
}

impl MaterialVessel {
    pub fn try_new(
        object_id: PhysicalObjectId,
        custody: OperationalCustody,
        capacity_microliters: u64,
    ) -> Result<Self, MaterialError> {
        Ok(Self {
            object: ObjectCustody::try_new(object_id, custody)
                .map_err(MaterialError::InvalidCustody)?,
            capacity_microliters: NonZeroU64::new(capacity_microliters)
                .ok_or(MaterialError::ZeroCapacity)?,
        })
    }

    pub const fn object_id(&self) -> PhysicalObjectId {
        self.object.object_id()
    }
    pub const fn custody(&self) -> &OperationalCustody {
        self.object.custody()
    }
    pub const fn capacity_microliters(&self) -> u64 {
        self.capacity_microliters.get()
    }

    pub fn accepts(&self, direct_contents: &[MaterialMeasure]) -> Result<bool, MaterialError> {
        let volume = direct_contents.iter().try_fold(0_u64, |total, measure| {
            total
                .checked_add(measure.volume_microliters)
                .ok_or(MaterialError::Overflow)
        })?;
        Ok(volume <= self.capacity_microliters.get())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensiveComponent<C: DomainMaterialComponent> {
    pub component: C,
    pub magnitude: NonZeroU64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContaminantLoad<C: DomainContaminant> {
    pub contaminant: C,
    pub load: NonZeroU64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateMaterialTruth<C: DomainMaterialComponent, X: DomainContaminant> {
    components: Vec<ExtensiveComponent<C>>,
    contaminants: Vec<ContaminantLoad<X>>,
}

impl<C: DomainMaterialComponent, X: DomainContaminant> PrivateMaterialTruth<C, X> {
    pub fn try_new(
        components: Vec<ExtensiveComponent<C>>,
        contaminants: Vec<ContaminantLoad<X>>,
    ) -> Result<Self, MaterialError> {
        if has_duplicate_keys(&components, |value| &value.component)
            || has_duplicate_keys(&contaminants, |value| &value.contaminant)
        {
            return Err(MaterialError::DuplicatePrivateComponent);
        }
        Ok(Self {
            components,
            contaminants,
        })
    }

    pub fn components(&self) -> &[ExtensiveComponent<C>] {
        &self.components
    }
    pub fn contaminants(&self) -> &[ContaminantLoad<X>] {
        &self.contaminants
    }

    pub fn split(&self, portion: Portion) -> Result<(Self, Self), MaterialError> {
        let mut taken_components = Vec::new();
        let mut remainder_components = Vec::new();
        for value in &self.components {
            let taken = prorated_floor(
                value.magnitude.get(),
                portion.numerator.get(),
                portion.denominator.get(),
            )?;
            if let Some(magnitude) = NonZeroU64::new(taken) {
                taken_components.push(ExtensiveComponent {
                    component: value.component.clone(),
                    magnitude,
                });
            }
            if let Some(magnitude) = NonZeroU64::new(value.magnitude.get() - taken) {
                remainder_components.push(ExtensiveComponent {
                    component: value.component.clone(),
                    magnitude,
                });
            }
        }
        let mut taken_contaminants = Vec::new();
        let mut remainder_contaminants = Vec::new();
        for value in &self.contaminants {
            let taken = prorated_floor(
                value.load.get(),
                portion.numerator.get(),
                portion.denominator.get(),
            )?;
            if let Some(load) = NonZeroU64::new(taken) {
                taken_contaminants.push(ContaminantLoad {
                    contaminant: value.contaminant.clone(),
                    load,
                });
            }
            if let Some(load) = NonZeroU64::new(value.load.get() - taken) {
                remainder_contaminants.push(ContaminantLoad {
                    contaminant: value.contaminant.clone(),
                    load,
                });
            }
        }
        Ok((
            Self::try_new(taken_components, taken_contaminants)?,
            Self::try_new(remainder_components, remainder_contaminants)?,
        ))
    }
}

fn has_duplicate_keys<T, K: Eq>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.iter().enumerate().any(|(index, value)| {
        values[index + 1..]
            .iter()
            .any(|other| key(value) == key(other))
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateMaterialSnapshot<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
> {
    identity: MaterialIdentity,
    measure: MaterialMeasure,
    preparation: MaterialPreparation<P>,
    truth: PrivateMaterialTruth<C, X>,
    revision: u64,
}

impl<P: DomainPreparation, C: DomainMaterialComponent, X: DomainContaminant>
    PrivateMaterialSnapshot<P, C, X>
{
    pub fn try_new(
        identity: MaterialIdentity,
        measure: MaterialMeasure,
        preparation: MaterialPreparation<P>,
        truth: PrivateMaterialTruth<C, X>,
        revision: u64,
    ) -> Result<Self, MaterialError> {
        if measure == MaterialMeasure::ZERO {
            return Err(MaterialError::EmptyMeasure);
        }
        Ok(Self {
            identity,
            measure,
            preparation,
            truth,
            revision,
        })
    }

    pub const fn private_truth(&self) -> &PrivateMaterialTruth<C, X> {
        &self.truth
    }
    pub const fn identity(&self) -> &MaterialIdentity {
        &self.identity
    }
    pub const fn measure(&self) -> MaterialMeasure {
        self.measure
    }
    pub const fn preparation(&self) -> &MaterialPreparation<P> {
        &self.preparation
    }
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// This projection does not inspect private truth. The authoritative
    /// adapter remains responsible for supplying an observer-safe value that
    /// was not derived from facts the observer may not know.
    pub fn sanitized<V: PublicMaterialPresentation>(&self, public: V) -> PublicMaterialView<V> {
        PublicMaterialView {
            lot_id: self.identity.lot_id,
            object_id: self.identity.object_id(),
            measure: self.measure,
            presentation: public,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicMaterialView<V: PublicMaterialPresentation> {
    pub lot_id: MaterialLotId,
    pub object_id: PhysicalObjectId,
    pub measure: MaterialMeasure,
    pub presentation: V,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessTiming {
    started_minute: u64,
    target_minutes: NonZeroU64,
    ready_minute: u64,
}

impl ProcessTiming {
    pub fn try_new(started_minute: u64, target_minutes: u64) -> Result<Self, MaterialError> {
        let target_minutes = NonZeroU64::new(target_minutes).ok_or(MaterialError::ZeroDuration)?;
        let ready_minute = started_minute
            .checked_add(target_minutes.get())
            .ok_or(MaterialError::ClockOverflow)?;
        Ok(Self {
            started_minute,
            target_minutes,
            ready_minute,
        })
    }

    pub const fn ready_minute(self) -> u64 {
        self.ready_minute
    }
    pub const fn started_minute(self) -> u64 {
        self.started_minute
    }
    pub const fn target_minutes(self) -> u64 {
        self.target_minutes.get()
    }

    pub fn cooking_status(self, current_minute: u64) -> CookingTimingStatus {
        match current_minute.cmp(&self.ready_minute) {
            std::cmp::Ordering::Less => CookingTimingStatus::Early {
                elapsed_minutes: current_minute.saturating_sub(self.started_minute),
                remaining_minutes: self.ready_minute - current_minute,
            },
            std::cmp::Ordering::Equal => CookingTimingStatus::Ready,
            std::cmp::Ordering::Greater => CookingTimingStatus::Late {
                elapsed_minutes: current_minute - self.started_minute,
                late_minutes: current_minute - self.ready_minute,
            },
        }
    }

    pub fn passive_status(
        self,
        current_minute: u64,
        materialized_at: Option<u64>,
    ) -> Result<PassiveTimingStatus, MaterialError> {
        if let Some(materialized_at) = materialized_at {
            if materialized_at < self.ready_minute {
                return Err(MaterialError::MaterializedBeforeReady);
            }
            return Ok(PassiveTimingStatus::Materialized { materialized_at });
        }
        if current_minute >= self.ready_minute {
            Ok(PassiveTimingStatus::ReadyToMaterialize)
        } else {
            Ok(PassiveTimingStatus::Maturing {
                remaining_minutes: self.ready_minute - current_minute,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CookingTimingStatus {
    Early {
        elapsed_minutes: u64,
        remaining_minutes: u64,
    },
    Ready,
    Late {
        elapsed_minutes: u64,
        late_minutes: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PassiveTimingStatus {
    Maturing { remaining_minutes: u64 },
    ReadyToMaterialize,
    Materialized { materialized_at: u64 },
}

pub const MAX_ROUNDING_TOLERANCE_SUBUNITS: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoundingTolerance(MaterialMeasure);

impl RoundingTolerance {
    pub fn try_new(mass_milligrams: u64, volume_microliters: u64) -> Result<Self, MaterialError> {
        if mass_milligrams > MAX_ROUNDING_TOLERANCE_SUBUNITS
            || volume_microliters > MAX_ROUNDING_TOLERANCE_SUBUNITS
        {
            return Err(MaterialError::RoundingToleranceTooLarge);
        }
        Ok(Self(MaterialMeasure {
            mass_milligrams,
            volume_microliters,
        }))
    }

    pub const fn exact() -> Self {
        Self(MaterialMeasure::ZERO)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensiveChangeKind {
    Gain,
    Loss,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensiveChange<K: Clone + fmt::Debug + Eq> {
    pub property: K,
    pub kind: ExtensiveChangeKind,
    pub magnitude: NonZeroU64,
}

/// Reducer-owned, typed policy for one existing process family. Ordinary
/// callers cannot provide an unbounded rounding allowance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessConservationPolicy<
    D: DomainConservationPolicy,
    C: DomainMaterialComponent,
    X: DomainContaminant,
> {
    authority: D,
    bulk_loss: MaterialMeasure,
    rounding_tolerance: RoundingTolerance,
    component_changes: Vec<ExtensiveChange<C>>,
    contaminant_changes: Vec<ExtensiveChange<X>>,
}

impl<D: DomainConservationPolicy, C: DomainMaterialComponent, X: DomainContaminant>
    ProcessConservationPolicy<D, C, X>
{
    pub fn try_new(
        authority: D,
        bulk_loss: MaterialMeasure,
        rounding_tolerance: RoundingTolerance,
        component_changes: Vec<ExtensiveChange<C>>,
        contaminant_changes: Vec<ExtensiveChange<X>>,
    ) -> Result<Self, MaterialError> {
        if has_duplicate_keys(&component_changes, |value| &value.property)
            || has_duplicate_keys(&contaminant_changes, |value| &value.property)
        {
            return Err(MaterialError::DuplicateExtensiveChange);
        }
        Ok(Self {
            authority,
            bulk_loss,
            rounding_tolerance,
            component_changes,
            contaminant_changes,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConservationReceipt {
    pub inputs: MaterialMeasure,
    pub outputs: MaterialMeasure,
    pub accounted_loss: MaterialMeasure,
    pub rounding_tolerance: RoundingTolerance,
    pub rounding_difference: MaterialMeasure,
}

fn verify_conservation(
    inputs: &[MaterialMeasure],
    outputs: &[MaterialMeasure],
    accounted_loss: MaterialMeasure,
    rounding_tolerance: RoundingTolerance,
) -> Result<ConservationReceipt, MaterialError> {
    let inputs = checked_sum(inputs)?;
    let outputs = checked_sum(outputs)?;
    let expected_outputs = inputs
        .checked_sub(accounted_loss)
        .map_err(|_| MaterialError::ConservationViolation)?;
    let rounding_difference = expected_outputs
        .checked_sub(outputs)
        .map_err(|_| MaterialError::ConservationViolation)?;
    if rounding_difference.mass_milligrams > rounding_tolerance.0.mass_milligrams
        || rounding_difference.volume_microliters > rounding_tolerance.0.volume_microliters
    {
        return Err(MaterialError::ConservationViolation);
    }
    Ok(ConservationReceipt {
        inputs,
        outputs,
        accounted_loss,
        rounding_tolerance,
        rounding_difference,
    })
}

fn checked_sum(values: &[MaterialMeasure]) -> Result<MaterialMeasure, MaterialError> {
    values
        .iter()
        .try_fold(MaterialMeasure::ZERO, |total, value| {
            total.checked_add(*value)
        })
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MaterialRequestId([u8; 32]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MaterialInputDigest([u8; 32]);

macro_rules! material_digest {
    ($name:ident, $error:ident) => {
        impl $name {
            pub fn try_new(bytes: [u8; 32]) -> Result<Self, MaterialError> {
                if bytes == [0; 32] {
                    Err(MaterialError::$error)
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

material_digest!(MaterialRequestId, ZeroRequestId);

impl MaterialInputDigest {
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MaterialActionProvenance {
    pub request_id: MaterialRequestId,
    pub process_id: MaterialProcessId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialRemainder<C: DomainMaterialComponent, X: DomainContaminant> {
    measure: MaterialMeasure,
    truth: PrivateMaterialTruth<C, X>,
}

impl<C: DomainMaterialComponent, X: DomainContaminant> MaterialRemainder<C, X> {
    pub const fn measure(&self) -> MaterialMeasure {
        self.measure
    }
    pub const fn private_truth(&self) -> &PrivateMaterialTruth<C, X> {
        &self.truth
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLotContribution<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
> {
    source: PrivateMaterialSnapshot<P, C, X>,
    consumed: MaterialMeasure,
    consumed_truth: PrivateMaterialTruth<C, X>,
}

impl<P: DomainPreparation, C: DomainMaterialComponent, X: DomainContaminant>
    SourceLotContribution<P, C, X>
{
    pub fn from_snapshot(
        source: &PrivateMaterialSnapshot<P, C, X>,
        portion: Portion,
    ) -> Result<(Self, Option<MaterialRemainder<C, X>>), MaterialError> {
        let (consumed, remainder) = portion.split_measure(source.measure)?;
        let (consumed_truth, remainder_truth) = source.truth.split(portion)?;
        Ok((
            Self {
                source: source.clone(),
                consumed,
                consumed_truth,
            },
            remainder.map(|measure| MaterialRemainder {
                measure,
                truth: remainder_truth,
            }),
        ))
    }

    pub const fn identity(&self) -> &MaterialIdentity {
        &self.source.identity
    }
    pub const fn expected_revision(&self) -> u64 {
        self.source.revision
    }
    pub const fn source_measure(&self) -> MaterialMeasure {
        self.source.measure
    }
    pub const fn consumed(&self) -> MaterialMeasure {
        self.consumed
    }
    pub const fn consumed_truth(&self) -> &PrivateMaterialTruth<C, X> {
        &self.consumed_truth
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducedMaterial<P: DomainPreparation, C: DomainMaterialComponent, X: DomainContaminant>
{
    snapshot: PrivateMaterialSnapshot<P, C, X>,
}

impl<P: DomainPreparation, C: DomainMaterialComponent, X: DomainContaminant>
    ProducedMaterial<P, C, X>
{
    pub fn try_new(snapshot: PrivateMaterialSnapshot<P, C, X>) -> Result<Self, MaterialError> {
        if snapshot.measure == MaterialMeasure::ZERO {
            return Err(MaterialError::EmptyOutputContribution);
        }
        Ok(Self { snapshot })
    }

    pub const fn snapshot(&self) -> &PrivateMaterialSnapshot<P, C, X> {
        &self.snapshot
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialTransformationReceipt<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
    D: DomainConservationPolicy,
    O: DomainMaterialReceipt,
> {
    provenance: MaterialActionProvenance,
    input_digest: MaterialInputDigest,
    sources: Vec<SourceLotContribution<P, C, X>>,
    outputs: Vec<ProducedMaterial<P, C, X>>,
    policy: ProcessConservationPolicy<D, C, X>,
    conservation: ConservationReceipt,
    domain_outcome: O,
}

impl<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
    D: DomainConservationPolicy,
    O: DomainMaterialReceipt,
> MaterialTransformationReceipt<P, C, X, D, O>
{
    pub fn try_new(
        provenance: MaterialActionProvenance,
        sources: Vec<SourceLotContribution<P, C, X>>,
        outputs: Vec<ProducedMaterial<P, C, X>>,
        policy: ProcessConservationPolicy<D, C, X>,
        domain_outcome: O,
    ) -> Result<Self, MaterialError> {
        if sources.is_empty() {
            return Err(MaterialError::EmptySourceContribution);
        }
        if has_duplicate_keys(&sources, |value| &value.source.identity.lot_id) {
            return Err(MaterialError::DuplicateSourceLot);
        }
        if sources.iter().enumerate().any(|(index, value)| {
            sources[index + 1..]
                .iter()
                .any(|other| value.source.identity.object_id() == other.source.identity.object_id())
        }) {
            return Err(MaterialError::DuplicateSourceObject);
        }
        if has_duplicate_keys(&outputs, |value| &value.snapshot.identity.lot_id) {
            return Err(MaterialError::DuplicateOutputLot);
        }
        if outputs.iter().enumerate().any(|(index, value)| {
            outputs[index + 1..].iter().any(|other| {
                value.snapshot.identity.object_id() == other.snapshot.identity.object_id()
            })
        }) {
            return Err(MaterialError::DuplicateOutputObject);
        }
        let input_digest = canonical_input_digest(&sources);
        let input_measures = sources
            .iter()
            .map(|value| value.consumed)
            .collect::<Vec<_>>();
        let output_measures = outputs
            .iter()
            .map(|value| value.snapshot.measure)
            .collect::<Vec<_>>();
        let conservation = verify_conservation(
            &input_measures,
            &output_measures,
            policy.bulk_loss,
            policy.rounding_tolerance,
        )?;
        verify_private_conservation(&sources, &outputs, &policy)?;
        Ok(Self {
            provenance,
            input_digest,
            sources,
            outputs,
            policy,
            conservation,
            domain_outcome,
        })
    }

    pub const fn provenance(&self) -> MaterialActionProvenance {
        self.provenance
    }
    pub const fn input_digest(&self) -> MaterialInputDigest {
        self.input_digest
    }
    pub fn sources(&self) -> &[SourceLotContribution<P, C, X>] {
        &self.sources
    }
    pub fn outputs(&self) -> &[ProducedMaterial<P, C, X>] {
        &self.outputs
    }
    pub const fn conservation(&self) -> ConservationReceipt {
        self.conservation
    }
    pub const fn policy(&self) -> &ProcessConservationPolicy<D, C, X> {
        &self.policy
    }
    pub const fn domain_outcome(&self) -> &O {
        &self.domain_outcome
    }
}

fn canonical_input_digest<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
>(
    sources: &[SourceLotContribution<P, C, X>],
) -> MaterialInputDigest {
    let mut ordered = sources.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|value| value.source.identity.lot_id);
    let mut hash = Sha256::new();
    hash.update(b"adventuresim.material-input.v1");
    for source in ordered {
        hash.update(source.source.identity.lot_id.get().to_le_bytes());
        hash.update(source.source.identity.object_id().get().to_le_bytes());
        hash.update(source.source.revision.to_le_bytes());
        hash.update(source.source.measure.mass_milligrams.to_le_bytes());
        hash.update(source.source.measure.volume_microliters.to_le_bytes());
        hash.update(source.consumed.mass_milligrams.to_le_bytes());
        hash.update(source.consumed.volume_microliters.to_le_bytes());
        update_custody_digest(&mut hash, source.source.identity.custody());
    }
    MaterialInputDigest(hash.finalize().into())
}

fn update_custody_digest(hash: &mut Sha256, custody: &OperationalCustody) {
    match custody {
        OperationalCustody::Character(value) => {
            hash.update([0]);
            hash.update(value.get().to_le_bytes());
        }
        OperationalCustody::Party(value) => {
            hash.update([1]);
            update_digest_bytes(hash, value.as_str().as_bytes());
        }
        OperationalCustody::Container(value) => {
            hash.update([2]);
            hash.update(value.get().to_le_bytes());
        }
        OperationalCustody::Place(value) => {
            hash.update([3]);
            update_digest_bytes(hash, value.to_string().as_bytes());
        }
        OperationalCustody::Fixture(value) => {
            hash.update([4]);
            update_digest_bytes(hash, value.to_string().as_bytes());
        }
    }
}

fn update_digest_bytes(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn verify_private_conservation<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
    D: DomainConservationPolicy,
>(
    sources: &[SourceLotContribution<P, C, X>],
    outputs: &[ProducedMaterial<P, C, X>],
    policy: &ProcessConservationPolicy<D, C, X>,
) -> Result<(), MaterialError> {
    let input_components = sources
        .iter()
        .flat_map(|value| value.consumed_truth.components.iter())
        .map(|value| (value.component.clone(), value.magnitude.get()))
        .collect::<Vec<_>>();
    let output_components = outputs
        .iter()
        .flat_map(|value| value.snapshot.truth.components.iter())
        .map(|value| (value.component.clone(), value.magnitude.get()))
        .collect::<Vec<_>>();
    verify_extensive_values(
        &input_components,
        &output_components,
        &policy.component_changes,
    )?;

    let input_contaminants = sources
        .iter()
        .flat_map(|value| value.consumed_truth.contaminants.iter())
        .map(|value| (value.contaminant.clone(), value.load.get()))
        .collect::<Vec<_>>();
    let output_contaminants = outputs
        .iter()
        .flat_map(|value| value.snapshot.truth.contaminants.iter())
        .map(|value| (value.contaminant.clone(), value.load.get()))
        .collect::<Vec<_>>();
    verify_extensive_values(
        &input_contaminants,
        &output_contaminants,
        &policy.contaminant_changes,
    )
}

fn verify_extensive_values<K: Clone + fmt::Debug + Eq>(
    inputs: &[(K, u64)],
    outputs: &[(K, u64)],
    changes: &[ExtensiveChange<K>],
) -> Result<(), MaterialError> {
    let mut keys = Vec::<K>::new();
    for key in inputs
        .iter()
        .map(|value| &value.0)
        .chain(outputs.iter().map(|value| &value.0))
        .chain(changes.iter().map(|value| &value.property))
    {
        if !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    for key in keys {
        let input = inputs
            .iter()
            .filter(|value| value.0 == key)
            .try_fold(0_u64, |total, value| {
                total.checked_add(value.1).ok_or(MaterialError::Overflow)
            })?;
        let output = outputs
            .iter()
            .filter(|value| value.0 == key)
            .try_fold(0_u64, |total, value| {
                total.checked_add(value.1).ok_or(MaterialError::Overflow)
            })?;
        let expected = match changes.iter().find(|value| value.property == key) {
            None => input,
            Some(change) if change.kind == ExtensiveChangeKind::Gain => input
                .checked_add(change.magnitude.get())
                .ok_or(MaterialError::Overflow)?,
            Some(change) => input
                .checked_sub(change.magnitude.get())
                .ok_or(MaterialError::PrivateConservationViolation)?,
        };
        if output != expected {
            return Err(MaterialError::PrivateConservationViolation);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterialRetryDecision<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
    D: DomainConservationPolicy,
    O: DomainMaterialReceipt,
> {
    Apply,
    IdempotentReplay(Box<MaterialTransformationReceipt<P, C, X, D, O>>),
    ProvenanceCollision,
}

pub fn classify_material_retry<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
    D: DomainConservationPolicy,
    O: DomainMaterialReceipt,
>(
    proposed: MaterialActionProvenance,
    proposed_sources: &[SourceLotContribution<P, C, X>],
    prior: Option<&MaterialTransformationReceipt<P, C, X, D, O>>,
) -> MaterialRetryDecision<P, C, X, D, O> {
    let proposed_digest = canonical_input_digest(proposed_sources);
    match prior {
        None => MaterialRetryDecision::Apply,
        Some(prior)
            if prior.provenance == proposed
                && prior.input_digest == proposed_digest
                && exact_sources_equal(&prior.sources, proposed_sources) =>
        {
            MaterialRetryDecision::IdempotentReplay(Box::new(prior.clone()))
        }
        Some(_) => MaterialRetryDecision::ProvenanceCollision,
    }
}

fn exact_sources_equal<P: DomainPreparation, C: DomainMaterialComponent, X: DomainContaminant>(
    left: &[SourceLotContribution<P, C, X>],
    right: &[SourceLotContribution<P, C, X>],
) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut left = left.iter().collect::<Vec<_>>();
    let mut right = right.iter().collect::<Vec<_>>();
    left.sort_by_key(|value| value.source.identity.lot_id);
    right.sort_by_key(|value| value.source.identity.lot_id);
    left.into_iter()
        .zip(right)
        .all(|(left, right)| left == right)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterialCommitRejection {
    MissingSource,
    AmbiguousSource,
    IdentityOrCustodyMismatch,
    StaleRevision,
    Overconsume,
    SnapshotMismatch,
}

/// Revalidates every captured source against fresh authoritative state. A
/// reducer must call this in the same transaction immediately before applying
/// a non-replay receipt.
pub fn validate_material_commit<
    P: DomainPreparation,
    C: DomainMaterialComponent,
    X: DomainContaminant,
    D: DomainConservationPolicy,
    O: DomainMaterialReceipt,
>(
    receipt: &MaterialTransformationReceipt<P, C, X, D, O>,
    current_sources: &[PrivateMaterialSnapshot<P, C, X>],
) -> Result<(), MaterialCommitRejection> {
    for captured in &receipt.sources {
        let matching = current_sources
            .iter()
            .filter(|current| current.identity.lot_id == captured.source.identity.lot_id)
            .collect::<Vec<_>>();
        let current = match matching.as_slice() {
            [] => return Err(MaterialCommitRejection::MissingSource),
            [current] => *current,
            _ => return Err(MaterialCommitRejection::AmbiguousSource),
        };
        if current.identity != captured.source.identity {
            return Err(MaterialCommitRejection::IdentityOrCustodyMismatch);
        }
        if current.revision != captured.source.revision {
            return Err(MaterialCommitRejection::StaleRevision);
        }
        if captured.consumed.mass_milligrams > current.measure.mass_milligrams
            || captured.consumed.volume_microliters > current.measure.volume_microliters
        {
            return Err(MaterialCommitRejection::Overconsume);
        }
        if current.measure != captured.source.measure
            || current.preparation != captured.source.preparation
            || current.truth != captured.source.truth
        {
            return Err(MaterialCommitRejection::SnapshotMismatch);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterialError {
    ZeroLotId,
    ZeroProcessId,
    ZeroRequestId,
    EmptyMeasure,
    EmptyPortion,
    ZeroPortion,
    ZeroDenominator,
    PortionExceedsWhole,
    ZeroCapacity,
    ZeroDuration,
    ClockOverflow,
    MaterializedBeforeReady,
    Overflow,
    Underflow,
    ConservationViolation,
    PrivateConservationViolation,
    RoundingToleranceTooLarge,
    DuplicatePrivateComponent,
    DuplicateExtensiveChange,
    EmptySourceContribution,
    EmptyOutputContribution,
    DuplicateSourceLot,
    DuplicateSourceObject,
    DuplicateOutputLot,
    DuplicateOutputObject,
    InvalidCustody(CustodyIdentityError),
}

impl fmt::Display for MaterialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroLotId => "Material lot identity must be nonzero",
            Self::ZeroProcessId => "Material process identity must be nonzero",
            Self::ZeroRequestId => "Material request identity must be nonzero",
            Self::EmptyMeasure => "Material measure must contain mass or volume",
            Self::EmptyPortion => "Material portion is below the minimum measured subunit",
            Self::ZeroPortion => "Material portion numerator must be nonzero",
            Self::ZeroDenominator => "Material portion denominator must be nonzero",
            Self::PortionExceedsWhole => "Material portion cannot exceed the whole",
            Self::ZeroCapacity => "Material vessel capacity must be nonzero",
            Self::ZeroDuration => "Material process duration must be nonzero",
            Self::ClockOverflow => "Material process cannot fit on the strategic clock",
            Self::MaterializedBeforeReady => {
                "Passive process materialization cannot precede its ready boundary"
            }
            Self::Overflow => "Material arithmetic overflow",
            Self::Underflow => "Material arithmetic underflow",
            Self::ConservationViolation => "Material transformation violates conservation",
            Self::PrivateConservationViolation => {
                "Private material properties violate explicit conservation changes"
            }
            Self::RoundingToleranceTooLarge => {
                "Material rounding tolerance exceeds the one-subunit bound"
            }
            Self::DuplicatePrivateComponent => "Private material components must be unique",
            Self::DuplicateExtensiveChange => {
                "Each private material property may have only one explicit change"
            }
            Self::EmptySourceContribution => {
                "Material receipts require nonempty source contributions"
            }
            Self::EmptyOutputContribution => "Produced material must contain mass or volume",
            Self::DuplicateSourceLot => "Material receipt source lots must be unique",
            Self::DuplicateSourceObject => "Material receipt source objects must be unique",
            Self::DuplicateOutputLot => "Material receipt output lots must be unique",
            Self::DuplicateOutputObject => "Material receipt output objects must be unique",
            Self::InvalidCustody(_) => "Material object custody is invalid",
        })
    }
}

impl std::error::Error for MaterialError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Preparation {
        Washed,
    }
    impl DomainPreparation for Preparation {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Process {
        Steep,
    }
    impl DomainMaterialProcess for Process {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Component {
        Medicinal,
    }
    impl DomainMaterialComponent for Component {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Contaminant {
        Microbial,
    }
    impl DomainContaminant for Contaminant {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Policy {
        Exact,
        Cooking,
    }
    impl DomainConservationPolicy for Policy {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Outcome(u64);
    impl DomainMaterialReceipt for Outcome {}
    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Presentation(&'static str);
    impl PublicMaterialPresentation for Presentation {}

    fn measure(mass: u64, volume: u64) -> MaterialMeasure {
        MaterialMeasure::try_new(mass, volume).unwrap()
    }

    fn identity(lot: u64, object: u64, custody: OperationalCustody) -> MaterialIdentity {
        MaterialIdentity::try_new(
            MaterialLotId::try_new(lot).unwrap(),
            PhysicalObjectId::try_new(object).unwrap(),
            custody,
        )
        .unwrap()
    }

    fn provenance(byte: u8) -> MaterialActionProvenance {
        MaterialActionProvenance {
            request_id: MaterialRequestId::try_new([byte; 32]).unwrap(),
            process_id: MaterialProcessId::try_new(u64::from(byte)).unwrap(),
        }
    }

    fn truth(component: u64, contaminant: u64) -> PrivateMaterialTruth<Component, Contaminant> {
        PrivateMaterialTruth::try_new(
            NonZeroU64::new(component)
                .map(|magnitude| {
                    vec![ExtensiveComponent {
                        component: Component::Medicinal,
                        magnitude,
                    }]
                })
                .unwrap_or_default(),
            NonZeroU64::new(contaminant)
                .map(|load| {
                    vec![ContaminantLoad {
                        contaminant: Contaminant::Microbial,
                        load,
                    }]
                })
                .unwrap_or_default(),
        )
        .unwrap()
    }

    fn snapshot(
        lot: u64,
        object: u64,
        amount: MaterialMeasure,
        component: u64,
        contaminant: u64,
        revision: u64,
    ) -> PrivateMaterialSnapshot<Preparation, Component, Contaminant> {
        PrivateMaterialSnapshot::try_new(
            identity(lot, object, OperationalCustody::character(7).unwrap()),
            amount,
            MaterialPreparation::Raw,
            truth(component, contaminant),
            revision,
        )
        .unwrap()
    }

    fn full_source(
        snapshot: &PrivateMaterialSnapshot<Preparation, Component, Contaminant>,
    ) -> SourceLotContribution<Preparation, Component, Contaminant> {
        let (source, remainder) =
            SourceLotContribution::from_snapshot(snapshot, Portion::try_new(1, 1).unwrap())
                .unwrap();
        assert!(remainder.is_none());
        source
    }

    fn exact_policy() -> ProcessConservationPolicy<Policy, Component, Contaminant> {
        ProcessConservationPolicy::try_new(
            Policy::Exact,
            MaterialMeasure::ZERO,
            RoundingTolerance::exact(),
            vec![],
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn partial_measurements_conserve_mass_and_volume_exactly() {
        let whole = measure(1_001, 333);
        let portion = Portion::try_new(1, 3).unwrap();
        let (taken, remainder) = portion.split_measure(whole).unwrap();
        assert_eq!(taken, measure(333, 111));
        assert_eq!(taken.checked_add(remainder.unwrap()), Ok(whole));
        assert_eq!(
            Portion::try_new(1, 1).unwrap().split_measure(whole),
            Ok((whole, None))
        );

        let truth = PrivateMaterialTruth::try_new(
            vec![ExtensiveComponent {
                component: Component::Medicinal,
                magnitude: NonZeroU64::new(10).unwrap(),
            }],
            vec![ContaminantLoad {
                contaminant: Contaminant::Microbial,
                load: NonZeroU64::new(8).unwrap(),
            }],
        )
        .unwrap();
        let (taken_truth, remainder_truth) = truth.split(portion).unwrap();
        assert_eq!(taken_truth.components()[0].magnitude.get(), 3);
        assert_eq!(remainder_truth.components()[0].magnitude.get(), 7);
        assert_eq!(taken_truth.contaminants()[0].load.get(), 2);
        assert_eq!(remainder_truth.contaminants()[0].load.get(), 6);
    }

    #[test]
    fn conservation_requires_explicit_loss_and_bounded_rounding() {
        let input = measure(1_000, 1_000);
        let output = measure(899, 949);
        let receipt = verify_conservation(
            &[input],
            &[output],
            measure(100, 50),
            RoundingTolerance::try_new(1, 1).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt.rounding_difference, measure(1, 1));
        assert_eq!(
            verify_conservation(
                &[input],
                &[measure(898, 949)],
                measure(100, 50),
                RoundingTolerance::try_new(1, 1).unwrap(),
            ),
            Err(MaterialError::ConservationViolation)
        );
        assert_eq!(
            RoundingTolerance::try_new(2, 0),
            Err(MaterialError::RoundingToleranceTooLarge)
        );
    }

    #[test]
    fn stable_object_identity_carries_exact_container_custody() {
        let vessel_id = PhysicalObjectId::try_new(40).unwrap();
        let ingredient = identity(1, 10, OperationalCustody::Container(vessel_id));
        assert_eq!(ingredient.object_id().get(), 10);
        assert_eq!(
            ingredient.custody(),
            &OperationalCustody::Container(vessel_id)
        );

        let vessel = MaterialVessel::try_new(
            vessel_id,
            OperationalCustody::character(7).unwrap(),
            250_000,
        )
        .unwrap();
        assert!(vessel.accepts(&[measure(50_000, 150_000)]).unwrap());
        assert!(!vessel.accepts(&[measure(50_000, 250_001)]).unwrap());
    }

    #[test]
    fn cooking_and_passive_process_timing_preserve_boundary_semantics() {
        let timing = ProcessTiming::try_new(100, 60).unwrap();
        assert_eq!(
            timing.cooking_status(159),
            CookingTimingStatus::Early {
                elapsed_minutes: 59,
                remaining_minutes: 1,
            }
        );
        assert_eq!(timing.cooking_status(160), CookingTimingStatus::Ready);
        assert_eq!(
            timing.cooking_status(175),
            CookingTimingStatus::Late {
                elapsed_minutes: 75,
                late_minutes: 15,
            }
        );
        assert_eq!(
            timing.passive_status(90, Some(170)),
            Ok(PassiveTimingStatus::Materialized {
                materialized_at: 170
            })
        );
        assert_eq!(
            timing.passive_status(170, Some(159)),
            Err(MaterialError::MaterializedBeforeReady)
        );
        assert_eq!(
            ProcessTiming::try_new(u64::MAX, 1),
            Err(MaterialError::ClockOverflow)
        );
    }

    #[test]
    fn public_projection_omits_private_truth_for_adapter_safe_presentation() {
        let first = PrivateMaterialTruth::try_new(
            vec![ExtensiveComponent {
                component: Component::Medicinal,
                magnitude: NonZeroU64::new(5).unwrap(),
            }],
            vec![ContaminantLoad {
                contaminant: Contaminant::Microbial,
                load: NonZeroU64::new(9).unwrap(),
            }],
        )
        .unwrap();
        let second = PrivateMaterialTruth::try_new(vec![], vec![]).unwrap();
        let make = |truth| {
            PrivateMaterialSnapshot::try_new(
                identity(1, 10, OperationalCustody::character(7).unwrap()),
                measure(50_000, 0),
                MaterialPreparation::<Preparation>::Ground,
                truth,
                2,
            )
            .unwrap()
        };
        assert_eq!(
            make(first).sanitized(Presentation("ground herb")),
            make(second).sanitized(Presentation("ground herb"))
        );
    }

    #[test]
    fn combine_conserves_private_truth_and_retry_digest_is_authoritative() {
        let action = provenance(3);
        let first_snapshot = snapshot(1, 11, measure(100, 0), 5, 2, 7);
        let second_snapshot = snapshot(2, 12, measure(50, 0), 3, 1, 4);
        let first = full_source(&first_snapshot);
        let second = full_source(&second_snapshot);
        assert_eq!(first.identity().object_id().get(), 11);
        assert_eq!(first.expected_revision(), 7);
        let output = snapshot(3, 13, measure(150, 0), 8, 3, 1);
        let receipt = MaterialTransformationReceipt::try_new(
            action,
            vec![first.clone(), second.clone()],
            vec![ProducedMaterial::try_new(output).unwrap()],
            exact_policy(),
            Outcome(12),
        )
        .unwrap();
        assert_eq!(
            validate_material_commit(&receipt, &[first_snapshot.clone(), second_snapshot.clone()]),
            Ok(())
        );
        assert_eq!(
            classify_material_retry(action, &[second.clone(), first.clone()], Some(&receipt)),
            MaterialRetryDecision::IdempotentReplay(Box::new(receipt.clone()))
        );
        let changed_revision_snapshot = snapshot(1, 11, measure(100, 0), 5, 2, 8);
        let changed_revision_source = full_source(&changed_revision_snapshot);
        assert_eq!(
            classify_material_retry(
                action,
                &[changed_revision_source, second.clone()],
                Some(&receipt),
            ),
            MaterialRetryDecision::ProvenanceCollision
        );
        let changed_truth = snapshot(1, 11, measure(100, 0), 6, 2, 7);
        let changed_truth_source = full_source(&changed_truth);
        assert_eq!(
            classify_material_retry(
                action,
                &[changed_truth_source, second.clone()],
                Some(&receipt),
            ),
            MaterialRetryDecision::ProvenanceCollision
        );
        let changed_preparation = PrivateMaterialSnapshot::try_new(
            identity(1, 11, OperationalCustody::character(7).unwrap()),
            measure(100, 0),
            MaterialPreparation::Ground,
            truth(5, 2),
            7,
        )
        .unwrap();
        assert_eq!(
            classify_material_retry(
                action,
                &[full_source(&changed_preparation), second.clone()],
                Some(&receipt),
            ),
            MaterialRetryDecision::ProvenanceCollision
        );

        assert_eq!(
            validate_material_commit(
                &receipt,
                &[changed_revision_snapshot, second_snapshot.clone()]
            ),
            Err(MaterialCommitRejection::StaleRevision)
        );
        assert_eq!(
            validate_material_commit(&receipt, &[changed_truth, second_snapshot.clone()]),
            Err(MaterialCommitRejection::SnapshotMismatch)
        );
        let overconsumed = snapshot(1, 11, measure(90, 0), 5, 2, 7);
        assert_eq!(
            validate_material_commit(&receipt, &[overconsumed, second_snapshot.clone()]),
            Err(MaterialCommitRejection::Overconsume)
        );
        let wrong_object = snapshot(1, 99, measure(100, 0), 5, 2, 7);
        assert_eq!(
            validate_material_commit(&receipt, &[wrong_object, second_snapshot]),
            Err(MaterialCommitRejection::IdentityOrCustodyMismatch)
        );
        type Receipt =
            MaterialTransformationReceipt<Preparation, Component, Contaminant, Policy, Outcome>;
        assert_eq!(
            classify_material_retry(action, &[first, second], None::<&Receipt>),
            MaterialRetryDecision::Apply
        );
    }

    #[test]
    fn cooking_requires_explicit_bulk_component_and_contaminant_losses() {
        let source = snapshot(1, 11, measure(100, 0), 10, 10, 3);
        let source = full_source(&source);
        let cooked = snapshot(2, 12, measure(90, 0), 8, 1, 1);
        let policy = ProcessConservationPolicy::try_new(
            Policy::Cooking,
            measure(10, 0),
            RoundingTolerance::exact(),
            vec![ExtensiveChange {
                property: Component::Medicinal,
                kind: ExtensiveChangeKind::Loss,
                magnitude: NonZeroU64::new(2).unwrap(),
            }],
            vec![ExtensiveChange {
                property: Contaminant::Microbial,
                kind: ExtensiveChangeKind::Loss,
                magnitude: NonZeroU64::new(9).unwrap(),
            }],
        )
        .unwrap();
        assert!(
            MaterialTransformationReceipt::try_new(
                provenance(5),
                vec![source.clone()],
                vec![ProducedMaterial::try_new(cooked.clone()).unwrap()],
                policy,
                Outcome(2),
            )
            .is_ok()
        );
        assert_eq!(
            MaterialTransformationReceipt::try_new(
                provenance(5),
                vec![source],
                vec![ProducedMaterial::try_new(cooked).unwrap()],
                ProcessConservationPolicy::try_new(
                    Policy::Cooking,
                    measure(10, 0),
                    RoundingTolerance::exact(),
                    vec![],
                    vec![],
                )
                .unwrap(),
                Outcome(2),
            ),
            Err(MaterialError::PrivateConservationViolation)
        );
    }

    #[test]
    fn receipt_rejects_physical_object_aliases_across_distinct_custody_values() {
        let source_snapshot = snapshot(1, 11, measure(100, 0), 0, 0, 3);
        let source = full_source(&source_snapshot);
        let personal = snapshot(2, 20, measure(50, 0), 0, 0, 1);
        let party = PrivateMaterialSnapshot::try_new(
            identity(3, 20, OperationalCustody::party("party-7").unwrap()),
            measure(50, 0),
            MaterialPreparation::Raw,
            truth(0, 0),
            1,
        )
        .unwrap();
        assert_eq!(
            MaterialTransformationReceipt::try_new(
                provenance(8),
                vec![source],
                vec![
                    ProducedMaterial::try_new(personal).unwrap(),
                    ProducedMaterial::try_new(party).unwrap()
                ],
                exact_policy(),
                Outcome(1),
            ),
            Err(MaterialError::DuplicateOutputObject)
        );
        assert_eq!(
            Portion::try_new(2, 1),
            Err(MaterialError::PortionExceedsWhole)
        );
    }

    #[test]
    fn process_and_preparation_extensions_are_closed_typed_values() {
        let preparation = MaterialPreparation::Domain(Preparation::Washed);
        let process = MaterialProcessKind::Domain(Process::Steep);
        assert_ne!(preparation, MaterialPreparation::Raw);
        assert_ne!(process, MaterialProcessKind::Tincture);
    }
}
