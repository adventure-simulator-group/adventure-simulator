//! Strategic physiology primitives.
//!
//! Private meters are the simulation vocabulary. The four humours are a
//! deliberately lossy, public presentation derived from those meters; they are
//! never inputs to treatment selection.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::strategic_time::MINUTES_PER_DAY;
use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;

pub const PHYSIOLOGY_RULESET_VERSION: u16 = 1;
pub const PHENOTYPE_KEY_VERSION: u16 = 1;
pub const METER_COUNT: usize = 10;
pub const REGION_COUNT: usize = 7;
pub const HUMOUR_COUNT: usize = 4;

/// Validated living character body mass in kilograms.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct BodyMassKg(f32);

impl BodyMassKg {
    pub const MIN: Self = Self(20.0);
    pub const MAX: Self = Self(300.0);
    pub const DEFAULT: Self = Self(70.0);

    pub fn try_new(kilograms: f32) -> Result<Self, BodyMassError> {
        if !kilograms.is_finite() {
            return Err(BodyMassError::NonFinite);
        }
        if !(Self::MIN.0..=Self::MAX.0).contains(&kilograms) {
            return Err(BodyMassError::OutOfRange);
        }
        Ok(Self(kilograms))
    }

    pub const fn kilograms(self) -> f32 {
        self.0
    }

    pub fn estimated_blood_milliliters(self) -> f32 {
        const BLOOD_MILLILITERS_PER_KILOGRAM: f32 = 70.0;
        self.0 * BLOOD_MILLILITERS_PER_KILOGRAM
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyMassError {
    NonFinite,
    OutOfRange,
}

/// Fixed-point medicinal dose where 1,000 milliunits is one standard dose.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DoseMilliunits(u32);

impl DoseMilliunits {
    pub const MILLIUNITS_PER_STANDARD_DOSE: u32 = 1_000;
    pub const MAX: Self = Self(8 * Self::MILLIUNITS_PER_STANDARD_DOSE);
    pub const MINIMUM_NONZERO: Self = Self(1);
    pub const ZERO: Self = Self(0);
    pub const STANDARD: Self = Self(Self::MILLIUNITS_PER_STANDARD_DOSE);

    pub const fn try_new(milliunits: u32) -> Result<Self, DoseError> {
        if milliunits > Self::MAX.0 {
            return Err(DoseError::ExceedsMaximum);
        }
        Ok(Self(milliunits))
    }

    pub fn try_from_standard_doses_rounded(standard_doses: f32) -> Result<Self, DoseError> {
        if !standard_doses.is_finite() || standard_doses < 0.0 {
            return Err(DoseError::InvalidMagnitude);
        }
        let milliunits = (standard_doses * Self::MILLIUNITS_PER_STANDARD_DOSE as f32).round();
        if milliunits > Self::MAX.0 as f32 {
            return Err(DoseError::ExceedsMaximum);
        }
        Ok(Self(milliunits as u32))
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn as_standard_doses(self) -> f32 {
        self.0 as f32 / Self::MILLIUNITS_PER_STANDARD_DOSE as f32
    }
}

impl TryFrom<u32> for DoseMilliunits {
    type Error = DoseError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl From<DoseMilliunits> for u32 {
    fn from(value: DoseMilliunits) -> Self {
        value.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoseError {
    InvalidMagnitude,
    ExceedsMaximum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Meter {
    Oxygenation,
    Perfusion,
    Hydration,
    Temperature,
    Inflammation,
    Coagulation,
    Nutrition,
    Neurologic,
    RenalClearance,
    TissueIntegrity,
}

impl Meter {
    pub const ALL: [Self; METER_COUNT] = [
        Self::Oxygenation,
        Self::Perfusion,
        Self::Hydration,
        Self::Temperature,
        Self::Inflammation,
        Self::Coagulation,
        Self::Nutrition,
        Self::Neurologic,
        Self::RenalClearance,
        Self::TissueIntegrity,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    /// A loss of `1.0` is terminal. Values above one are retained internally so
    /// combined causes can be compared, but authored/evaluated values are
    /// bounded to protect simulation arithmetic.
    pub const fn terminal_loss(self) -> f32 {
        1.0
    }

    pub const fn public_name(self) -> &'static str {
        match self {
            Self::Oxygenation => "Oxygenation",
            Self::Perfusion => "Perfusion",
            Self::Hydration => "Hydration",
            Self::Temperature => "Temperature regulation",
            Self::Inflammation => "Inflammatory load",
            Self::Coagulation => "Coagulation",
            Self::Nutrition => "Nutritional reserve",
            Self::Neurologic => "Neurologic function",
            Self::RenalClearance => "Renal clearance",
            Self::TissueIntegrity => "Tissue integrity",
        }
    }

    pub const fn interpretation(self) -> &'static str {
        match self {
            Self::Oxygenation => "loss of useful oxygen exchange",
            Self::Perfusion => "loss of effective blood flow",
            Self::Hydration => "loss of water and electrolyte balance",
            Self::Temperature => "loss of temperature control",
            Self::Inflammation => "harmful inflammatory burden",
            Self::Coagulation => "loss of safe clotting balance",
            Self::Nutrition => "loss of usable energy and nutrients",
            Self::Neurologic => "loss of nervous-system function",
            Self::RenalClearance => "loss of waste and fluid clearance",
            Self::TissueIntegrity => "loss of intact, functioning tissue",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum BodyRegion {
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Abdomen,
    Head,
}

impl std::fmt::Display for BodyRegion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl BodyRegion {
    pub const ALL: [Self; REGION_COUNT] = [
        Self::LeftArm,
        Self::RightArm,
        Self::LeftLeg,
        Self::RightLeg,
        Self::Chest,
        Self::Abdomen,
        Self::Head,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeftArm => "left_arm",
            Self::RightArm => "right_arm",
            Self::LeftLeg => "left_leg",
            Self::RightLeg => "right_leg",
            Self::Chest => "chest",
            Self::Abdomen => "abdomen",
            Self::Head => "head",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::LeftArm => "left-arm",
            Self::RightArm => "right-arm",
            Self::LeftLeg => "left-leg",
            Self::RightLeg => "right-leg",
            Self::Chest => "chest",
            Self::Abdomen => "abdomen",
            Self::Head => "head",
        }
    }

    pub fn parse_slug(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|region| region.slug() == value)
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|region| region.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Humour {
    Sanguine,
    Phlegmatic,
    Choleric,
    Melancholic,
}

impl Humour {
    pub const ALL: [Self; HUMOUR_COUNT] = [
        Self::Sanguine,
        Self::Phlegmatic,
        Self::Choleric,
        Self::Melancholic,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn public_name(self) -> &'static str {
        match self {
            Self::Sanguine => "Sanguine",
            Self::Phlegmatic => "Phlegmatic",
            Self::Choleric => "Choleric",
            Self::Melancholic => "Melancholic",
        }
    }
}

pub fn humour_disclosure(humour: Humour) -> String {
    let weights = Meter::ALL
        .into_iter()
        .filter_map(|meter| {
            let weight = HUMOUR_WEIGHTS[meter.index()][humour.index()];
            (weight > 0.0)
                .then(|| format!("{} {:.0}%", meter.public_name(), f64::from(weight) * 100.0))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} is a clamped weighted sum: {weights}. It loses distinctions between private causes, interactions, phenotype, and regional detail; equal readings need not have equal causes.",
        humour.public_name()
    )
}

/// Relationship portion of intervention authorization. Identity authority and
/// living-state checks remain reducer-owned; this predicate makes the
/// direct-caller party and co-location boundary independently testable.
#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub fn intervention_relationship_allowed(
    actor_id: u64,
    patient_id: u64,
    actor_party_id: Option<&str>,
    patient_party_id: Option<&str>,
    actor_settlement_id: Option<&str>,
    patient_settlement_id: Option<&str>,
    actor_case_site_id: Option<&str>,
    patient_case_site_id: Option<&str>,
) -> bool {
    actor_id == patient_id
        || (actor_party_id.is_some()
            && actor_party_id == patient_party_id
            && actor_settlement_id == patient_settlement_id
            && actor_case_site_id == patient_case_site_id)
}

/// Public, stable and intentionally many-to-one. Rows are private meters;
/// columns are Sanguine, Phlegmatic, Choleric and Melancholic loss.
pub const HUMOUR_WEIGHTS: [[f32; HUMOUR_COUNT]; METER_COUNT] = [
    [0.10, 0.75, 0.05, 0.10], // oxygenation
    [0.75, 0.10, 0.10, 0.05], // perfusion
    [0.10, 0.05, 0.80, 0.05], // hydration
    [0.10, 0.25, 0.55, 0.10], // temperature
    [0.10, 0.25, 0.55, 0.10], // inflammation
    [0.65, 0.05, 0.20, 0.10], // coagulation
    [0.15, 0.10, 0.55, 0.20], // nutrition
    [0.05, 0.10, 0.10, 0.75], // neurologic
    [0.10, 0.10, 0.65, 0.15], // renal clearance
    [0.55, 0.05, 0.25, 0.15], // tissue integrity
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterVector(pub [f32; METER_COUNT]);

impl MeterVector {
    pub const ZERO: Self = Self([0.0; METER_COUNT]);

    pub fn from_entries(entries: &[(Meter, f32)]) -> Self {
        let mut values = [0.0; METER_COUNT];
        for (meter, value) in entries {
            values[meter.index()] =
                (values[meter.index()] + finite_or_zero(*value)).clamp(-1.0, 2.0);
        }
        Self(values)
    }

    pub fn get(self, meter: Meter) -> f32 {
        self.0[meter.index()]
    }

    pub fn add_bounded(&mut self, other: Self) {
        for meter in Meter::ALL {
            let i = meter.index();
            self.0[i] = (finite_or_zero(self.0[i]) + finite_or_zero(other.0[i])).clamp(-1.0, 2.0);
        }
    }

    pub fn scaled(self, amount: f32) -> Self {
        let amount = finite_or_zero(amount).clamp(-16.0, 16.0);
        Self(
            self.0
                .map(|value| (finite_or_zero(value) * amount).clamp(-1.0, 2.0)),
        )
    }

    pub fn terminal(self) -> Option<Meter> {
        Meter::ALL
            .into_iter()
            .filter(|meter| self.get(*meter) >= meter.terminal_loss())
            // Meter order is the canonical deterministic tie-break.
            .min()
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

pub fn humours(meters: MeterVector) -> [f32; HUMOUR_COUNT] {
    let mut result = [0.0; HUMOUR_COUNT];
    for meter in Meter::ALL {
        let deviation = meters.get(meter);
        for humour in 0..HUMOUR_COUNT {
            result[humour] += deviation * HUMOUR_WEIGHTS[meter.index()][humour];
        }
    }
    result.map(|value| value.clamp(-1.0, 1.0))
}

pub fn regional_humours(
    regional_meters: &[MeterVector; REGION_COUNT],
) -> [[f32; HUMOUR_COUNT]; REGION_COUNT] {
    regional_meters.map(humours)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurvePoint {
    pub minute: u64,
    pub loss: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MeterCurve {
    pub meter: Meter,
    /// Relative disease minutes, strictly increasing. Values between points
    /// are linearly interpolated at integer strategic minutes.
    pub points: &'static [CurvePoint],
    /// Fractions assigned to the seven public body regions. They are
    /// normalized during projection.
    pub regional_weights: [f32; REGION_COUNT],
}

pub fn piecewise(curve: &MeterCurve, age: u64) -> f32 {
    let Some(first) = curve.points.first() else {
        return 0.0;
    };
    if age <= first.minute {
        return finite_or_zero(first.loss);
    }
    for pair in curve.points.windows(2) {
        let [left, right] = pair else { unreachable!() };
        if age <= right.minute {
            let span = right.minute.saturating_sub(left.minute);
            if span == 0 {
                return finite_or_zero(right.loss);
            }
            let fraction = age.saturating_sub(left.minute) as f64 / span as f64;
            return (left.loss as f64 + (right.loss - left.loss) as f64 * fraction) as f32;
        }
    }
    finite_or_zero(curve.points.last().expect("nonempty curve").loss)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "snake_case")]
pub enum InterventionRoute {
    Oral,
    Topical,
    Inhaled,
    Injected,
}

impl InterventionRoute {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oral => "oral",
            Self::Topical => "topical",
            Self::Inhaled => "inhaled",
            Self::Injected => "injected",
        }
    }
}

impl std::fmt::Display for InterventionRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InterventionProfile {
    pub preparation_id: &'static str,
    pub version: u16,
    pub route: InterventionRoute,
    pub duration_minutes: u64,
    pub loss_delta_per_unit: MeterVector,
    pub adverse_delta_per_unit: MeterVector,
}

/// Versioned, generic physiology effects for bounded concrete preparations.
/// Herbalism chooses one of these identities; Physiology administers it
/// without knowing or matching a disease key.
pub const INTERVENTION_PROFILES: [InterventionProfile; 14] = [
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[0],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 8 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::Hydration, -0.18),
            (Meter::RenalClearance, -0.04),
        ]),
        adverse_delta_per_unit: MeterVector::from_const([(Meter::Hydration, 0.03)]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[1],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 6 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::Temperature, -0.07),
            (Meter::Inflammation, -0.03),
        ]),
        adverse_delta_per_unit: MeterVector::from_const([(Meter::Coagulation, 0.02)]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[2],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 6 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::Temperature, -0.12),
            (Meter::Inflammation, -0.06),
        ]),
        adverse_delta_per_unit: MeterVector::from_const([(Meter::Coagulation, 0.04)]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[3],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 6 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::Temperature, -0.17),
            (Meter::Inflammation, -0.09),
        ]),
        adverse_delta_per_unit: MeterVector::from_const([(Meter::Coagulation, 0.07)]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[4],
        version: 1,
        route: InterventionRoute::Topical,
        duration_minutes: 12 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::TissueIntegrity, -0.05),
            (Meter::Inflammation, -0.02),
        ]),
        adverse_delta_per_unit: MeterVector::ZERO,
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[5],
        version: 1,
        route: InterventionRoute::Topical,
        duration_minutes: 12 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::TissueIntegrity, -0.10),
            (Meter::Inflammation, -0.05),
        ]),
        adverse_delta_per_unit: MeterVector::ZERO,
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[6],
        version: 1,
        route: InterventionRoute::Topical,
        duration_minutes: 12 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::TissueIntegrity, -0.15),
            (Meter::Inflammation, -0.07),
        ]),
        adverse_delta_per_unit: MeterVector::ZERO,
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[7],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 4 * 60,
        loss_delta_per_unit: MeterVector::from_const([(Meter::Neurologic, -0.08)]),
        adverse_delta_per_unit: MeterVector::from_const([
            (Meter::Oxygenation, 0.04),
            (Meter::RenalClearance, 0.03),
        ]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[8],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 4 * 60,
        loss_delta_per_unit: MeterVector::from_const([(Meter::Neurologic, -0.15)]),
        adverse_delta_per_unit: MeterVector::from_const([
            (Meter::Oxygenation, 0.09),
            (Meter::RenalClearance, 0.06),
        ]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[9],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 4 * 60,
        loss_delta_per_unit: MeterVector::from_const([(Meter::Neurologic, -0.23)]),
        adverse_delta_per_unit: MeterVector::from_const([
            (Meter::Oxygenation, 0.16),
            (Meter::RenalClearance, 0.11),
        ]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[10],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 3 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::Inflammation, -0.03),
            (Meter::Neurologic, -0.02),
        ]),
        adverse_delta_per_unit: MeterVector::from_const([(Meter::Hydration, 0.02)]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[11],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 3 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::Inflammation, -0.06),
            (Meter::Neurologic, -0.04),
        ]),
        adverse_delta_per_unit: MeterVector::from_const([(Meter::Hydration, 0.04)]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[12],
        version: 1,
        route: InterventionRoute::Oral,
        duration_minutes: 3 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::Inflammation, -0.09),
            (Meter::Neurologic, -0.06),
        ]),
        adverse_delta_per_unit: MeterVector::from_const([(Meter::Hydration, 0.06)]),
    },
    InterventionProfile {
        preparation_id: crate::item_references::MEDICATION_IDS[13],
        version: 1,
        route: InterventionRoute::Topical,
        duration_minutes: 12 * 60,
        loss_delta_per_unit: MeterVector::from_const([
            (Meter::TissueIntegrity, -0.10),
            (Meter::Inflammation, -0.05),
        ]),
        adverse_delta_per_unit: MeterVector::ZERO,
    },
];

impl MeterVector {
    const fn from_const<const N: usize>(entries: [(Meter, f32); N]) -> Self {
        let mut values = [0.0; METER_COUNT];
        let mut i = 0;
        while i < N {
            values[entries[i].0 as usize] = entries[i].1;
            i += 1;
        }
        Self(values)
    }
}

pub fn intervention_profile(id: &str, version: u16) -> Option<&'static InterventionProfile> {
    INTERVENTION_PROFILES
        .iter()
        .find(|profile| profile.preparation_id == id && profile.version == version)
}

/// Resolve the latest authored profile for a concrete preparation.
///
/// Trusted callers use this when starting a new course. Durable
/// administrations continue to pin their exact version for replay.
pub fn current_intervention_profile(id: &str) -> Option<&'static InterventionProfile> {
    INTERVENTION_PROFILES
        .iter()
        .filter(|profile| profile.preparation_id == id)
        .max_by_key(|profile| profile.version)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Administration {
    pub id: u64,
    pub patient_id: u64,
    pub preparation_id: String,
    pub profile_version: u16,
    pub route: InterventionRoute,
    pub dose: DoseMilliunits,
    pub region: Option<BodyRegion>,
    pub administered_at: u64,
    pub stopped_at: Option<u64>,
    /// Secret-derived bounded sensitivity, persisted for replay without
    /// persisting or exposing the phenotype itself.
    pub sensitivity_bps: i16,
    pub adverse_bps: u16,
}

impl Administration {
    pub fn effect_at(&self, now: u64) -> MeterVector {
        let Some(profile) = intervention_profile(&self.preparation_id, self.profile_version) else {
            return MeterVector::ZERO;
        };
        if self.route != profile.route || now < self.administered_at {
            return MeterVector::ZERO;
        }
        let end = self
            .stopped_at
            .unwrap_or_else(|| {
                self.administered_at
                    .saturating_add(profile.duration_minutes)
            })
            .min(
                self.administered_at
                    .saturating_add(profile.duration_minutes),
            );
        if now >= end {
            return MeterVector::ZERO;
        }
        let dose = self.dose.as_standard_doses()
            * (1.0
                + self.sensitivity_bps.clamp(-2_500, 2_500) as f32
                    / f32::from(BASIS_POINTS_PER_WHOLE));
        let adverse = dose * self.adverse_bps.min(2_500) as f32 / f32::from(BASIS_POINTS_PER_WHOLE);
        let mut result = profile.loss_delta_per_unit.scaled(dose);
        result.add_bounded(profile.adverse_delta_per_unit.scaled(adverse));
        result
    }
}

/// Secret-keyed relative phenotype. The returned multipliers have mean one,
/// so this changes which meters dominate rather than merely scaling severity.
pub fn phenotype_multipliers(
    secret: &[u8],
    key_version: u16,
    character_id: u64,
    episode_id: u64,
) -> [f32; METER_COUNT] {
    let mut raw = [0.0; METER_COUNT];
    for meter in Meter::ALL {
        let hash = keyed_hash(secret, key_version, character_id, episode_id, meter as u8);
        let unit = (hash >> 11) as f64 / (1u64 << 53) as f64;
        raw[meter.index()] = 0.72 + unit as f32 * 0.56;
    }
    let mean = raw.iter().sum::<f32>() / METER_COUNT as f32;
    raw.map(|value| value / mean)
}

pub fn baseline_meters(secret: &[u8], key_version: u16, character_id: u64) -> MeterVector {
    let mut values = [0.0; METER_COUNT];
    for meter in Meter::ALL {
        let hash = keyed_hash(secret, key_version, character_id, 0, meter as u8);
        let unit = (hash >> 11) as f64 / (1u64 << 53) as f64;
        // Healthy individual variation remains well below terminal.
        values[meter.index()] = (unit as f32 - 0.5) * 0.04;
    }
    MeterVector(values)
}

fn keyed_hash(
    secret: &[u8],
    key_version: u16,
    character_id: u64,
    episode_id: u64,
    discriminator: u8,
) -> u64 {
    // HMAC-SHA256 keeps persisted key material private while producing stable
    // replayable phenotype components. The implementation is local to avoid
    // adding a second cryptography dependency to the simulation core.
    const BLOCK: usize = 64;
    let mut key = [0u8; BLOCK];
    if secret.len() > BLOCK {
        key[..32].copy_from_slice(&Sha256::digest(secret));
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }
    let mut inner_pad = [0x36u8; BLOCK];
    let mut outer_pad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] ^= key[index];
        outer_pad[index] ^= key[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(b"adventuresim/physiology/phenotype");
    inner.update(key_version.to_le_bytes());
    inner.update(character_id.to_le_bytes());
    inner.update(episode_id.to_le_bytes());
    inner.update([discriminator]);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    let digest = outer.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix"))
}

pub fn disease_meter_state(
    curves: &[MeterCurve],
    age: u64,
    phenotype: &[f32; METER_COUNT],
) -> MeterVector {
    let mut state = MeterVector::ZERO;
    for curve in curves {
        let weighted = piecewise(curve, age) * phenotype[curve.meter.index()];
        state.0[curve.meter.index()] = (state.0[curve.meter.index()] + weighted).clamp(-1.0, 2.0);
    }
    state
}

pub fn combined_meter_state(
    disease_states: impl IntoIterator<Item = MeterVector>,
    interventions: &[Administration],
    now: u64,
) -> MeterVector {
    let mut combined = MeterVector::ZERO;
    for disease in disease_states {
        combined.add_bounded(disease);
    }
    for administration in interventions {
        combined.add_bounded(administration.effect_at(now));
    }
    combined
}

/// Exact earliest integer-minute terminal crossing over authored structural
/// boundaries. Between adjacent boundaries every meter is linear, so endpoint
/// classification plus bounded binary search is exact and independent of
/// caller chunking without scanning every elapsed minute.
pub fn first_terminal_crossing(
    structural_minutes: &[u64],
    mut state_at: impl FnMut(u64) -> MeterVector,
) -> Option<(u64, Meter)> {
    let mut points = structural_minutes.to_vec();
    points.sort_unstable();
    points.dedup();
    for window in points.windows(2) {
        let (mut left, mut right) = (window[0], window[1]);
        if let Some(meter) = state_at(left).terminal() {
            return Some((left, meter));
        }
        if state_at(right).terminal().is_none() {
            continue;
        }
        while left + 1 < right {
            let middle = left + (right - left) / 2;
            if state_at(middle).terminal().is_some() {
                right = middle;
            } else {
                left = middle;
            }
        }
        if let Some(meter) = state_at(right).terminal() {
            return Some((right, meter));
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceSpan {
    pub observer_id: u64,
    pub patient_id: u64,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    /// Historical capability is pinned at the span boundary so later training
    /// cannot retroactively sharpen an old notebook.
    pub physiology_band: u8,
}

impl PresenceSpan {
    pub fn canonical(
        first_id: u64,
        second_id: u64,
        first_clock: u64,
        second_clock: u64,
        physiology_band: u8,
    ) -> Self {
        Self {
            observer_id: first_id,
            patient_id: second_id,
            started_at: first_clock.min(second_clock),
            ended_at: None,
            physiology_band: physiology_band.min(5),
        }
    }

    pub fn close(&mut self, first_clock: u64, second_clock: u64) {
        let end = first_clock.min(second_clock).max(self.started_at);
        self.ended_at = Some(end);
    }

    pub fn contains(&self, minute: u64) -> bool {
        minute >= self.started_at && self.ended_at.is_none_or(|end| minute <= end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartEntry {
    Reading {
        minute: u64,
        humour_deviations_bps: [i16; HUMOUR_COUNT],
        known_interventions: Vec<String>,
    },
    Gap {
        from: u64,
        to: u64,
    },
}

pub const fn observation_cadence_minutes(_physiology_band: u8) -> u64 {
    MINUTES_PER_DAY
}

pub fn quantize_humours(values: [f32; HUMOUR_COUNT], physiology_band: u8) -> [i16; HUMOUR_COUNT] {
    let step: i32 = match physiology_band.min(5) {
        0 => 1_000,
        1 => 500,
        2 => 250,
        3 => 100,
        4 => 50,
        _ => 25,
    };
    values.map(|value| {
        let bps = (finite_or_zero(value).clamp(-1.0, 1.0) * f32::from(BASIS_POINTS_PER_WHOLE))
            .round() as i32;
        let rounded = if bps >= 0 {
            (bps + step / 2) / step * step
        } else {
            (bps - step / 2) / step * step
        };
        rounded.clamp(
            -i32::from(BASIS_POINTS_PER_WHOLE),
            i32::from(BASIS_POINTS_PER_WHOLE),
        ) as i16
    })
}

/// Stable observer error that changes smoothly from one strategic day to the
/// next. It is presentation-only: the keyed wobble never changes private
/// meters, treatment effects, injury, or death.
#[expect(
    clippy::too_many_arguments,
    reason = "observer noise is keyed by each independent domain discriminator"
)]
pub fn observation_noise(
    secret: &[u8],
    key_version: u16,
    observer_id: u64,
    patient_id: u64,
    region: BodyRegion,
    humour: Humour,
    minute: u64,
    physiology_band: u8,
) -> f32 {
    let day = minute / MINUTES_PER_DAY;
    let fraction = (minute % MINUTES_PER_DAY) as f32 / MINUTES_PER_DAY as f32;
    let discriminator = (region as u8)
        .saturating_mul(HUMOUR_COUNT as u8)
        .saturating_add(humour as u8);
    let sample = |sample_day: u64| {
        let episode = patient_id ^ sample_day.rotate_left(19) ^ 0x6f62_7365_7276_6572;
        let hash = keyed_hash(secret, key_version, observer_id, episode, discriminator);
        ((hash >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32
    };
    let amplitude = [0.070, 0.060, 0.050, 0.040, 0.030, 0.020][usize::from(physiology_band.min(5))];
    (sample(day) + (sample(day.saturating_add(1)) - sample(day)) * fraction) * amplitude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_mass_has_one_inclusive_validated_range() {
        assert_eq!(BodyMassKg::try_new(20.0), Ok(BodyMassKg::MIN));
        assert_eq!(BodyMassKg::try_new(300.0), Ok(BodyMassKg::MAX));
        assert_eq!(BodyMassKg::try_new(19.99), Err(BodyMassError::OutOfRange));
        assert_eq!(BodyMassKg::try_new(f32::NAN), Err(BodyMassError::NonFinite));
        assert_eq!(BodyMassKg::DEFAULT.estimated_blood_milliliters(), 4_900.0);
    }

    #[test]
    fn medicinal_dose_uses_one_thousand_milliunits_per_standard_dose() {
        assert_eq!(DoseMilliunits::STANDARD.as_standard_doses(), 1.0);
        assert_eq!(
            DoseMilliunits::try_from_standard_doses_rounded(0.75),
            DoseMilliunits::try_new(750)
        );
        assert_eq!(
            DoseMilliunits::try_new(DoseMilliunits::MAX.get() + 1),
            Err(DoseError::ExceedsMaximum)
        );
    }

    #[test]
    fn catalogue_is_bounded_and_humours_are_many_to_one() {
        assert!((8..=12).contains(&METER_COUNT));
        assert_eq!(HUMOUR_WEIGHTS.len(), METER_COUNT);
        assert_eq!(
            humours(MeterVector::from_entries(&[(Meter::Temperature, 0.5)])),
            humours(MeterVector::from_entries(&[(Meter::Inflammation, 0.5)]))
        );
    }

    #[test]
    fn phenotype_is_keyed_deterministic_and_relative() {
        let a = phenotype_multipliers(b"one", 1, 4, 9);
        assert_eq!(a, phenotype_multipliers(b"one", 1, 4, 9));
        assert_ne!(a, phenotype_multipliers(b"two", 1, 4, 9));
        assert!((a.iter().sum::<f32>() / METER_COUNT as f32 - 1.0).abs() < 0.0001);
        assert!(a.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn intervention_is_profile_versioned_and_not_disease_keyed() {
        let dose = Administration {
            id: 1,
            patient_id: 7,
            preparation_id: "oral_rehydration_draught".into(),
            profile_version: 1,
            route: InterventionRoute::Oral,
            dose: DoseMilliunits::STANDARD,
            region: None,
            administered_at: 10,
            stopped_at: None,
            sensitivity_bps: 0,
            adverse_bps: 0,
        };
        assert!(dose.effect_at(11).get(Meter::Hydration) < 0.0);
        assert_eq!(dose.effect_at(10 + 8 * 60), MeterVector::ZERO);
    }

    #[test]
    fn crossing_is_exact_chunk_independent_and_ties_by_meter() {
        let state = |minute| {
            MeterVector::from_entries(&[
                (Meter::Perfusion, minute as f32 / 100.0),
                (Meter::Oxygenation, minute as f32 / 100.0),
            ])
        };
        let whole = first_terminal_crossing(&[0, 200], state);
        let chunks = (0..4)
            .find_map(|chunk| first_terminal_crossing(&[chunk * 50, (chunk + 1) * 50], state));
        assert_eq!(whole, Some((100, Meter::Oxygenation)));
        assert_eq!(whole, chunks);
    }

    #[test]
    fn presence_uses_asymmetric_min_clock_and_preserves_reentry() {
        let mut first = PresenceSpan::canonical(1, 2, 100, 80, 3);
        first.close(150, 120);
        let second = PresenceSpan::canonical(1, 2, 200, 190, 4);
        assert_eq!(first.started_at, 80);
        assert_eq!(first.ended_at, Some(120));
        assert_eq!(second.started_at, 190);
        assert!(!first.contains(190));
    }

    #[test]
    fn cadence_is_daily_while_quantization_improves_with_skill() {
        for band in 0..=5 {
            assert_eq!(observation_cadence_minutes(band), 1_440);
        }
        assert_ne!(
            quantize_humours([0.1234; 4], 0),
            quantize_humours([0.1234; 4], 5)
        );
        assert_eq!(
            quantize_humours([0.1234; 4], 3),
            quantize_humours([0.1234; 4], 3)
        );
        assert_eq!(
            quantize_humours([-0.1234, 0.1234, -2.0, 2.0], 3),
            [-1_200, 1_200, -10_000, 10_000]
        );
    }

    #[test]
    fn observer_noise_is_stable_day_scaled_and_narrows_with_skill() {
        let sample = |minute, band| {
            observation_noise(
                b"observer-secret",
                1,
                17,
                23,
                BodyRegion::Chest,
                Humour::Phlegmatic,
                minute,
                band,
            )
        };
        assert_eq!(sample(1_440, 2), sample(1_440, 2));
        assert_ne!(sample(1_440, 2), sample(2_880, 2));
        assert!(sample(1_440, 0).abs() <= 0.070);
        assert!(sample(1_440, 5).abs() <= 0.020);
    }

    #[test]
    fn body_regions_round_trip_through_the_canonical_persistence_name() {
        for region in BodyRegion::ALL {
            assert_eq!(BodyRegion::parse(region.as_str()), Some(region));
            assert_eq!(BodyRegion::parse_slug(region.slug()), Some(region));
        }
        assert_eq!(BodyRegion::parse("stomach"), None);
        assert_eq!(BodyRegion::parse_slug("stomach"), None);
        assert_eq!(BodyRegion::parse("Abdomen"), None);
    }

    #[test]
    fn direct_intervention_relationship_rejects_cross_party_or_remote_callers() {
        assert!(intervention_relationship_allowed(
            1, 1, None, None, None, None, None, None
        ));
        assert!(intervention_relationship_allowed(
            1,
            2,
            Some("7"),
            Some("7"),
            Some("basel"),
            Some("basel"),
            None,
            None,
        ));
        assert!(!intervention_relationship_allowed(
            1,
            2,
            Some("7"),
            Some("8"),
            Some("basel"),
            Some("basel"),
            None,
            None,
        ));
        assert!(!intervention_relationship_allowed(
            1,
            2,
            Some("7"),
            Some("7"),
            Some("basel"),
            Some("zurich"),
            None,
            None,
        ));
        assert!(!intervention_relationship_allowed(
            1,
            2,
            Some("7"),
            Some("7"),
            None,
            None,
            Some("site-a"),
            Some("site-b"),
        ));
    }
}
