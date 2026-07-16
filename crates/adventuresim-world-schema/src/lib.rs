//! Stable, source-independent types at the world compiler/database boundary.
//!
//! Keep this crate lightweight. Readers for CSV, raster, and vector formats
//! belong in `adventuresim-world-import`, not here or in the database module.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

pub const WORLD_SCHEMA_VERSION: u32 = 13;
pub const MAX_SOURCES_MARKDOWN_CHARS: usize = 32_768;

/// Source and inference notes are deliberately unstructured Markdown for a
/// future debug view. Keep the payload bounded even though the contents are
/// not parsed into canonical provenance types.
pub fn valid_sources_markdown(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_SOURCES_MARKDOWN_CHARS
        && !value.contains('\0')
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum OfficialReligion {
    RomanCatholic,
    Lutheran,
    Reformed,
    Anglican,
    ProtestantUnspecified,
    EasternOrthodox,
    Islamic,
}

impl OfficialReligion {
    /// Stable identifier used by the current single-church gameplay systems.
    pub const fn faith_id(self) -> &'static str {
        match self {
            Self::RomanCatholic => "roman_catholic",
            Self::Lutheran => "lutheran",
            Self::Reformed => "reformed",
            Self::Anglican => "anglican",
            Self::ProtestantUnspecified => "protestant",
            Self::EasternOrthodox => "eastern_orthodox",
            Self::Islamic => "islamic",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum CatholicLutheranChurch {
    RomanCatholic,
    Lutheran,
}

impl CatholicLutheranChurch {
    pub const fn religion(self) -> OfficialReligion {
        match self {
            Self::RomanCatholic => OfficialReligion::RomanCatholic,
            Self::Lutheran => OfficialReligion::Lutheran,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum CatholicReformedChurch {
    RomanCatholic,
    Reformed,
}

impl CatholicReformedChurch {
    pub const fn religion(self) -> OfficialReligion {
        match self {
            Self::RomanCatholic => OfficialReligion::RomanCatholic,
            Self::Reformed => OfficialReligion::Reformed,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum LutheranReformedChurch {
    Lutheran,
    Reformed,
}

impl LutheranReformedChurch {
    pub const fn religion(self) -> OfficialReligion {
        match self {
            Self::Lutheran => OfficialReligion::Lutheran,
            Self::Reformed => OfficialReligion::Reformed,
        }
    }
}

/// A bounded set of legally recognized western confessions and the church
/// currently represented by the settlement's single-church gameplay model.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum WesternChristianArrangement {
    CatholicLutheran { church: CatholicLutheranChurch },
    CatholicReformed { church: CatholicReformedChurch },
    LutheranReformed { church: LutheranReformedChurch },
}

impl WesternChristianArrangement {
    pub const fn church(self) -> OfficialReligion {
        match self {
            Self::CatholicLutheran { church } => church.religion(),
            Self::CatholicReformed { church } => church.religion(),
            Self::LutheranReformed { church } => church.religion(),
        }
    }
}

/// Official legal status reconstructed for the territory, not a claim about
/// every inhabitant's private belief.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SettlementReligiousStatus {
    Established {
        religion: OfficialReligion,
    },
    Parity {
        arrangement: WesternChristianArrangement,
    },
    MultiConfessional {
        arrangement: WesternChristianArrangement,
    },
    LocallyDetermined {
        church: OfficialReligion,
    },
}

impl SettlementReligiousStatus {
    pub const fn church(self) -> OfficialReligion {
        match self {
            Self::Established { religion } => religion,
            Self::Parity { arrangement } | Self::MultiConfessional { arrangement } => {
                arrangement.church()
            }
            Self::LocallyDetermined { church } => church,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct PalmerDroughtSeverityIndex {
    milli_units: i16,
}

impl PalmerDroughtSeverityIndex {
    pub const MIN: i16 = -15_000;
    pub const MAX: i16 = 15_000;

    pub const fn new(milli_units: i16) -> Option<Self> {
        if milli_units >= Self::MIN && milli_units <= Self::MAX {
            Some(Self { milli_units })
        } else {
            None
        }
    }

    pub const fn milli_units(self) -> i16 {
        self.milli_units
    }

    pub const fn condition(self) -> SummerHydroclimate {
        match self.milli_units {
            ..=-4_000 => SummerHydroclimate::ExtremeDrought,
            -3_999..=-3_000 => SummerHydroclimate::SevereDrought,
            -2_999..=-2_000 => SummerHydroclimate::ModerateDrought,
            -1_999..=-1_000 => SummerHydroclimate::MildDrought,
            -999..=999 => SummerHydroclimate::NearNormal,
            1_000..=1_999 => SummerHydroclimate::MildlyWet,
            2_000..=2_999 => SummerHydroclimate::ModeratelyWet,
            3_000..=3_999 => SummerHydroclimate::VeryWet,
            _ => SummerHydroclimate::ExtremelyWet,
        }
    }
}

impl<'de> Deserialize<'de> for PalmerDroughtSeverityIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            milli_units: i16,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.milli_units).ok_or_else(|| {
            serde::de::Error::custom(format_args!(
                "PDSI {} is outside {}..={}",
                wire.milli_units,
                Self::MIN,
                Self::MAX
            ))
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SummerHydroclimate {
    ExtremeDrought,
    SevereDrought,
    ModerateDrought,
    MildDrought,
    NearNormal,
    MildlyWet,
    ModeratelyWet,
    VeryWet,
    ExtremelyWet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct DroughtHistory {
    current_summer: PalmerDroughtSeverityIndex,
    twenty_year_mean: PalmerDroughtSeverityIndex,
    drought_summers: u8,
    wet_summers: u8,
}

impl DroughtHistory {
    pub const WINDOW_YEARS: u8 = 20;

    pub fn new(
        current_summer: PalmerDroughtSeverityIndex,
        twenty_year_mean: PalmerDroughtSeverityIndex,
        drought_summers: u8,
        wet_summers: u8,
    ) -> Option<Self> {
        if drought_summers > Self::WINDOW_YEARS
            || wet_summers > Self::WINDOW_YEARS
            || drought_summers + wet_summers > Self::WINDOW_YEARS
        {
            return None;
        }
        let mut drought_remaining = i32::from(drought_summers);
        let mut wet_remaining = i32::from(wet_summers);
        let mut normal_remaining = i32::from(Self::WINDOW_YEARS - drought_summers - wet_summers);
        let current = i32::from(current_summer.milli_units());
        if current <= -2_000 {
            if drought_remaining == 0 {
                return None;
            }
            drought_remaining -= 1;
        } else if current >= 2_000 {
            if wet_remaining == 0 {
                return None;
            }
            wet_remaining -= 1;
        } else {
            if normal_remaining == 0 {
                return None;
            }
            normal_remaining -= 1;
        }
        let minimum_sum = current + drought_remaining * i32::from(PalmerDroughtSeverityIndex::MIN)
            - normal_remaining * 1_999
            + wet_remaining * 2_000;
        let maximum_sum = current - drought_remaining * 2_000
            + normal_remaining * 1_999
            + wet_remaining * i32::from(PalmerDroughtSeverityIndex::MAX);
        let minimum_mean = (f64::from(minimum_sum) / f64::from(Self::WINDOW_YEARS)).round() as i16;
        let maximum_mean = (f64::from(maximum_sum) / f64::from(Self::WINDOW_YEARS)).round() as i16;
        if !(minimum_mean..=maximum_mean).contains(&twenty_year_mean.milli_units()) {
            return None;
        }
        Some(Self {
            current_summer,
            twenty_year_mean,
            drought_summers,
            wet_summers,
        })
    }

    pub const fn current_summer(self) -> PalmerDroughtSeverityIndex {
        self.current_summer
    }

    pub const fn twenty_year_mean(self) -> PalmerDroughtSeverityIndex {
        self.twenty_year_mean
    }

    pub const fn drought_summers(self) -> u8 {
        self.drought_summers
    }

    pub const fn wet_summers(self) -> u8 {
        self.wet_summers
    }
}

impl<'de> Deserialize<'de> for DroughtHistory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            current_summer: PalmerDroughtSeverityIndex,
            twenty_year_mean: PalmerDroughtSeverityIndex,
            drought_summers: u8,
            wet_summers: u8,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.current_summer,
            wire.twenty_year_mean,
            wire.drought_summers,
            wire.wet_summers,
        )
        .ok_or_else(|| serde::de::Error::custom("invalid twenty-year drought/wet summer counts"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DroughtProfile {
    Reconstructed(DroughtHistory),
    Inferred(DroughtHistory),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct GeologicUnitId {
    value: String,
}

impl GeologicUnitId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()
            && value.len() <= 255
            && value == value.trim()
            && !value.chars().any(char::is_control))
        .then_some(Self { value })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<'de> Deserialize<'de> for GeologicUnitId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            value: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.value)
            .ok_or_else(|| serde::de::Error::custom("invalid EGDI geologic unit identifier"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum UnconsolidatedDeposit {
    Clay,
    Silt,
    Sand,
    Gravel,
    Till,
    Peat,
    Alluvium,
    Loess,
    VolcanicAsh,
    MixedSediment,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SedimentaryRock {
    Limestone,
    Dolostone,
    Chalk,
    Marl,
    Sandstone,
    Siltstone,
    Mudstone,
    Shale,
    Conglomerate,
    Evaporite,
    Coal,
    Chert,
    MixedSedimentary,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum IgneousRock {
    Granite,
    Granitoid,
    Diorite,
    Gabbro,
    Basalt,
    Andesite,
    Rhyolite,
    Tuff,
    OtherPlutonic,
    OtherVolcanic,
    OtherIgneous,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum MetamorphicRock {
    Slate,
    Schist,
    Gneiss,
    Quartzite,
    Marble,
    Phyllite,
    Amphibolite,
    OtherMetamorphic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum MixedLithology {
    Breccia,
    Melange,
    MixedRock,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SurfaceLithology {
    Unconsolidated(UnconsolidatedDeposit),
    Sedimentary(SedimentaryRock),
    Igneous(IgneousRock),
    Metamorphic(MetamorphicRock),
    Mixed(MixedLithology),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum GeologicEra {
    Quaternary,
    Neogene,
    Paleogene,
    Cenozoic,
    Cretaceous,
    Jurassic,
    Triassic,
    Mesozoic,
    Permian,
    Carboniferous,
    Devonian,
    Silurian,
    Ordovician,
    Cambrian,
    Paleozoic,
    Precambrian,
    Phanerozoic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum GeologicAgeEvidence {
    Mapped(GeologicEra),
    Inferred(GeologicEra),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum GeologicLithologyEvidence {
    Mapped(SurfaceLithology),
    Inferred(SurfaceLithology),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct GeologicSetting {
    pub lithology: GeologicLithologyEvidence,
    pub age: GeologicAgeEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct MappedSurfaceGeology {
    pub unit: GeologicUnitId,
    pub setting: GeologicSetting,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct InferredGeologicSetting {
    pub lithology: SurfaceLithology,
    pub age: GeologicEra,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SurfaceGeology {
    Mapped(MappedSurfaceGeology),
    Inferred(InferredGeologicSetting),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct EuroVegMapUnitCode {
    code: String,
}

impl EuroVegMapUnitCode {
    pub fn new(code: impl Into<String>) -> Option<Self> {
        let code = code.into();
        let mut chars = code.chars();
        let first = chars.next()?;
        if code.len() <= 20
            && first.is_ascii_uppercase()
            && chars.all(|character| character.is_ascii_alphanumeric() || character == '/')
        {
            Some(Self { code })
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.code
    }
}

impl<'de> Deserialize<'de> for EuroVegMapUnitCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            code: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.code).ok_or_else(|| serde::de::Error::custom("invalid EuroVegMap unit code"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum PotentialVegetationFormation {
    PolarDesertAndNival,
    TundraAndAlpine,
    OpenWoodlandAndSubalpine,
    ConiferousAndMixedForest,
    Heath,
    DeciduousAndMixedForest,
    ThermophilousBroadleafForest,
    HygroThermophilousBroadleafForest,
    MediterraneanSclerophyll,
    XerophyticConiferAndScrub,
    ForestSteppe,
    Steppe,
    Oroxerophytic,
    Desert,
    CoastalAndHalophytic,
    AquaticAndReed,
    Mire,
    SwampAndFenForest,
    FloodplainAndWetland,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct MappedPotentialVegetation {
    unit: EuroVegMapUnitCode,
    formation: PotentialVegetationFormation,
}

impl MappedPotentialVegetation {
    pub fn new(unit: EuroVegMapUnitCode, formation: PotentialVegetationFormation) -> Option<Self> {
        (formation_for_unit(unit.as_str()) == Some(formation)).then_some(Self { unit, formation })
    }

    pub fn unit(&self) -> &EuroVegMapUnitCode {
        &self.unit
    }

    pub const fn formation(&self) -> PotentialVegetationFormation {
        self.formation
    }
}

impl<'de> Deserialize<'de> for MappedPotentialVegetation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            unit: EuroVegMapUnitCode,
            formation: PotentialVegetationFormation,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.unit, wire.formation).ok_or_else(|| {
            serde::de::Error::custom("EuroVegMap unit code and formation do not agree")
        })
    }
}

fn formation_for_unit(code: &str) -> Option<PotentialVegetationFormation> {
    use PotentialVegetationFormation as F;
    match code {
        "Glacier" => return Some(F::PolarDesertAndNival),
        // EuroVegMap 2.1 assigns its descriptive River unit to formation F.
        "River" => return Some(F::DeciduousAndMixedForest),
        _ => {}
    }
    match code.as_bytes().first().copied()? {
        b'A' => Some(F::PolarDesertAndNival),
        b'B' => Some(F::TundraAndAlpine),
        b'C' => Some(F::OpenWoodlandAndSubalpine),
        b'D' => Some(F::ConiferousAndMixedForest),
        b'E' => Some(F::Heath),
        b'F' => Some(F::DeciduousAndMixedForest),
        b'G' => Some(F::ThermophilousBroadleafForest),
        b'H' => Some(F::HygroThermophilousBroadleafForest),
        b'J' => Some(F::MediterraneanSclerophyll),
        b'K' => Some(F::XerophyticConiferAndScrub),
        b'L' => Some(F::ForestSteppe),
        b'M' => Some(F::Steppe),
        b'N' => Some(F::Oroxerophytic),
        b'O' => Some(F::Desert),
        b'P' => Some(F::CoastalAndHalophytic),
        b'R' => Some(F::AquaticAndReed),
        b'S' => Some(F::Mire),
        b'T' => Some(F::SwampAndFenForest),
        b'U' => Some(F::FloodplainAndWetland),
        _ => None,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum PotentialVegetation {
    Mapped(MappedPotentialVegetation),
    Inferred(PotentialVegetationFormation),
}

pub const MAX_MODELED_TREE_SPECIES: usize = 12;
pub const MAX_INFERRED_TREE_SPECIES: usize = 4;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct TreeSpeciesId {
    scientific_name: String,
}

impl TreeSpeciesId {
    pub fn new(scientific_name: impl Into<String>) -> Option<Self> {
        let scientific_name = scientific_name.into();
        let (genus, epithet) = scientific_name.split_once('_')?;
        if epithet.contains('_') || !valid_genus(genus) || !valid_epithet(epithet) {
            return None;
        }
        Some(Self { scientific_name })
    }

    pub fn as_str(&self) -> &str {
        &self.scientific_name
    }
}

fn valid_genus(value: &str) -> bool {
    let mut characters = value.bytes();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|character| character.is_ascii_lowercase())
}

fn valid_epithet(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|character| character.is_ascii_lowercase())
}

impl<'de> Deserialize<'de> for TreeSpeciesId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            scientific_name: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.scientific_name)
            .ok_or_else(|| serde::de::Error::custom("invalid EU-Trees4F scientific name"))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct HabitatSuitability {
    score: u16,
}

impl HabitatSuitability {
    pub const MAX: u16 = 1_000;

    pub const fn new(score: u16) -> Option<Self> {
        if score <= Self::MAX {
            Some(Self { score })
        } else {
            None
        }
    }

    pub const fn score(self) -> u16 {
        self.score
    }
}

impl<'de> Deserialize<'de> for HabitatSuitability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            score: u16,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.score)
            .ok_or_else(|| serde::de::Error::custom("tree habitat suitability must be 0..=1000"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum NativeRangeEvidence {
    WithinNativeRange,
    OutsideNativeRange,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct ModeledTreeSpecies {
    pub species: TreeSpeciesId,
    pub suitability: HabitatSuitability,
    pub native_range: NativeRangeEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct ModeledTreeSpeciesProfile {
    candidates: Vec<ModeledTreeSpecies>,
}

impl ModeledTreeSpeciesProfile {
    pub fn new(mut candidates: Vec<ModeledTreeSpecies>) -> Option<Self> {
        if candidates.is_empty() || candidates.len() > MAX_MODELED_TREE_SPECIES {
            return None;
        }
        let unique_species = candidates
            .iter()
            .map(|candidate| &candidate.species)
            .collect::<std::collections::BTreeSet<_>>();
        if unique_species.len() != candidates.len() {
            return None;
        }
        candidates.sort_by(|left, right| {
            right
                .suitability
                .cmp(&left.suitability)
                .then_with(|| left.species.cmp(&right.species))
        });
        Some(Self { candidates })
    }

    pub fn candidates(&self) -> &[ModeledTreeSpecies] {
        &self.candidates
    }
}

impl<'de> Deserialize<'de> for ModeledTreeSpeciesProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            candidates: Vec<ModeledTreeSpecies>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.candidates)
            .ok_or_else(|| serde::de::Error::custom("invalid modeled tree-species profile"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct InferredTreeSpeciesProfile {
    species: Vec<TreeSpeciesId>,
}

impl InferredTreeSpeciesProfile {
    pub fn new(mut species: Vec<TreeSpeciesId>) -> Option<Self> {
        if species.is_empty() || species.len() > MAX_INFERRED_TREE_SPECIES {
            return None;
        }
        species.sort();
        if species.windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        Some(Self { species })
    }

    pub fn species(&self) -> &[TreeSpeciesId] {
        &self.species
    }
}

impl<'de> Deserialize<'de> for InferredTreeSpeciesProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            species: Vec<TreeSpeciesId>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.species)
            .ok_or_else(|| serde::de::Error::custom("invalid inferred tree-species profile"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum TreeSpeciesProfile {
    Modeled(ModeledTreeSpeciesProfile),
    Inferred(InferredTreeSpeciesProfile),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum MineralSoilTexture {
    Coarse,
    Medium,
    MediumFine,
    Fine,
    VeryFine,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct MineralSoil {
    pub texture: MineralSoilTexture,
    pub depth: SoilDepth,
    pub available_water: AvailableWaterCapacity,
    pub organic_carbon: TopsoilOrganicCarbon,
    pub stones: StoneContentPercent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct OrganicSoil {
    pub depth: SoilDepth,
    pub available_water: AvailableWaterCapacity,
    pub stones: StoneContentPercent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RockOutcropSoil {
    pub stones: StoneContentPercent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct OtherNonTexturedSoil {
    pub depth: SoilDepth,
    pub available_water: AvailableWaterCapacity,
    pub organic_carbon: TopsoilOrganicCarbon,
    pub stones: StoneContentPercent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SoilSubstrate {
    Mineral(MineralSoil),
    Organic(OrganicSoil),
    RockOutcrop(RockOutcropSoil),
    OtherNonTextured(OtherNonTexturedSoil),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SoilDepth {
    Shallow,
    Moderate,
    Deep,
    VeryDeep,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum AvailableWaterCapacity {
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum TopsoilOrganicCarbon {
    VeryLow,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SoilWaterRegime {
    UsuallyDry,
    SeasonallyWet,
    LongSeasonWet,
    PermanentlyWet,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum AgriculturalLimitation {
    None,
    Gravelly,
    Stony,
    ShallowRock,
    Concretionary,
    CementedCalcic,
    Saline,
    Sodic,
    GlacierOrSnow,
    Disturbed,
    Fragic,
    Drained,
    Flooded,
    Eroded,
    ShallowWaterTable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct StoneContentPercent {
    percent: u8,
}

impl StoneContentPercent {
    pub const fn new(percent: u8) -> Option<Self> {
        if percent <= 100 {
            Some(Self { percent })
        } else {
            None
        }
    }

    pub const fn percent(self) -> u8 {
        self.percent
    }
}

impl<'de> Deserialize<'de> for StoneContentPercent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            percent: u8,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.percent)
            .ok_or_else(|| serde::de::Error::custom("stone content must be 0..=100 percent"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SoilProperties {
    pub substrate: SoilSubstrate,
    pub water_regime: SoilWaterRegime,
    pub agricultural_limitation: AgriculturalLimitation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SoilMappingUnit {
    smu: u32,
    dominant_stu: u32,
    dominance_percent: u8,
}

impl SoilMappingUnit {
    pub const fn new(smu: u32, dominant_stu: u32, dominance_percent: u8) -> Option<Self> {
        if smu > 0 && dominant_stu > 0 && dominance_percent >= 1 && dominance_percent <= 100 {
            Some(Self {
                smu,
                dominant_stu,
                dominance_percent,
            })
        } else {
            None
        }
    }

    pub const fn smu(self) -> u32 {
        self.smu
    }

    pub const fn dominant_stu(self) -> u32 {
        self.dominant_stu
    }

    pub const fn dominance_percent(self) -> u8 {
        self.dominance_percent
    }
}

impl<'de> Deserialize<'de> for SoilMappingUnit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            smu: u32,
            dominant_stu: u32,
            dominance_percent: u8,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.smu, wire.dominant_stu, wire.dominance_percent)
            .ok_or_else(|| serde::de::Error::custom("invalid ESDB soil mapping unit"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum WrbReferenceGroup {
    Albeluvisol,
    Acrisol,
    Alisol,
    Andosol,
    Arenosol,
    Anthrosol,
    Chernozem,
    Calcisol,
    Cambisol,
    Cryosol,
    Durisol,
    Fluvisol,
    Ferralsol,
    Gleysol,
    Gypsisol,
    Histosol,
    Kastanozem,
    Leptosol,
    Luvisol,
    Lixisol,
    Nitisol,
    Phaeozem,
    Planosol,
    Plinthosol,
    Podzol,
    Regosol,
    Solonchak,
    Solonetz,
    Umbrisol,
    Vertisol,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct ParentMaterialCode {
    code: String,
}

impl ParentMaterialCode {
    pub fn new(code: impl Into<String>) -> Option<Self> {
        let code = code.into();
        (!code.is_empty()
            && code.len() <= 16
            && code.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then_some(Self { code })
    }

    pub fn as_str(&self) -> &str {
        &self.code
    }
}

impl<'de> Deserialize<'de> for ParentMaterialCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            code: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.code)
            .ok_or_else(|| serde::de::Error::custom("invalid ESDB parent-material code"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct MappedSoilProfile {
    pub mapping_unit: SoilMappingUnit,
    pub wrb_group: WrbReferenceGroup,
    pub parent_material: ParentMaterialCode,
    pub properties: SoilProperties,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SoilProfile {
    Mapped(MappedSoilProfile),
    Inferred(SoilProperties),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct CanopyDensity {
    percent: u8,
}

impl CanopyDensity {
    pub const fn new(percent: u8) -> Option<Self> {
        if percent >= 1 && percent <= 100 {
            Some(Self { percent })
        } else {
            None
        }
    }

    pub const fn percent(self) -> u8 {
        self.percent
    }
}

impl<'de> Deserialize<'de> for CanopyDensity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            percent: u8,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.percent).ok_or_else(|| {
            serde::de::Error::custom("wooded canopy density must be 1..=100 percent")
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DominantLeafType {
    Broadleaf,
    Coniferous,
    Mixed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct Woodland {
    pub density: CanopyDensity,
    pub dominant: DominantLeafType,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum ForestCover {
    Open,
    Wooded(Woodland),
}

pub const LAND_USE_BASIS_POINTS: u16 = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct LandUseFraction {
    basis_points: u16,
}

impl LandUseFraction {
    pub const fn new(basis_points: u16) -> Option<Self> {
        if basis_points <= LAND_USE_BASIS_POINTS {
            Some(Self { basis_points })
        } else {
            None
        }
    }

    pub const fn basis_points(self) -> u16 {
        self.basis_points
    }
}

impl<'de> Deserialize<'de> for LandUseFraction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireFraction {
            basis_points: u16,
        }

        let wire = WireFraction::deserialize(deserializer)?;
        Self::new(wire.basis_points).ok_or_else(|| {
            serde::de::Error::custom("land-use fraction exceeds 10,000 basis points")
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum HumanLandUseIntensity {
    Wild,
    Sparse,
    Rural,
    Intensive,
    Urban,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct LandUseProfile {
    cropland: LandUseFraction,
    grazing: LandUseFraction,
    built_up: LandUseFraction,
    natural: LandUseFraction,
}

impl LandUseProfile {
    pub const fn new(
        cropland: LandUseFraction,
        grazing: LandUseFraction,
        built_up: LandUseFraction,
        natural: LandUseFraction,
    ) -> Option<Self> {
        if cropland.basis_points() as u32
            + grazing.basis_points() as u32
            + built_up.basis_points() as u32
            + natural.basis_points() as u32
            == LAND_USE_BASIS_POINTS as u32
        {
            Some(Self {
                cropland,
                grazing,
                built_up,
                natural,
            })
        } else {
            None
        }
    }

    pub const fn cropland(self) -> LandUseFraction {
        self.cropland
    }
    pub const fn grazing(self) -> LandUseFraction {
        self.grazing
    }
    pub const fn built_up(self) -> LandUseFraction {
        self.built_up
    }
    pub const fn natural(self) -> LandUseFraction {
        self.natural
    }

    pub const fn intensity(self) -> HumanLandUseIntensity {
        let managed = self.cropland.basis_points() as u32
            + self.grazing.basis_points() as u32
            + self.built_up.basis_points() as u32;
        if self.built_up.basis_points() >= 1_000 {
            HumanLandUseIntensity::Urban
        } else {
            match managed {
                0..=499 => HumanLandUseIntensity::Wild,
                500..=1_999 => HumanLandUseIntensity::Sparse,
                2_000..=4_999 => HumanLandUseIntensity::Rural,
                _ => HumanLandUseIntensity::Intensive,
            }
        }
    }
}

impl<'de> Deserialize<'de> for LandUseProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireProfile {
            cropland: LandUseFraction,
            grazing: LandUseFraction,
            built_up: LandUseFraction,
            natural: LandUseFraction,
        }
        let wire = WireProfile::deserialize(deserializer)?;
        Self::new(wire.cropland, wire.grazing, wire.built_up, wire.natural)
            .ok_or_else(|| serde::de::Error::custom("land-use fractions do not sum to 10,000"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct ElevationMeters {
    meters: i16,
}

impl ElevationMeters {
    pub const MIN: i16 = -500;
    pub const MAX: i16 = 9_000;

    pub const fn new(meters: i16) -> Option<Self> {
        if meters >= Self::MIN && meters <= Self::MAX {
            Some(Self { meters })
        } else {
            None
        }
    }

    pub const fn get(self) -> i16 {
        self.meters
    }

    pub const fn band(self) -> ElevationBand {
        match self.meters {
            ..=-1 => ElevationBand::BelowSeaLevel,
            0..=299 => ElevationBand::Lowland,
            300..=999 => ElevationBand::Upland,
            1_000..=1_999 => ElevationBand::Highland,
            _ => ElevationBand::Alpine,
        }
    }
}

impl<'de> Deserialize<'de> for ElevationMeters {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireElevation {
            meters: i16,
        }

        let wire = WireElevation::deserialize(deserializer)?;
        Self::new(wire.meters).ok_or_else(|| {
            serde::de::Error::custom(format_args!(
                "elevation {} is outside {}..={}",
                wire.meters,
                Self::MIN,
                Self::MAX
            ))
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum ElevationBand {
    BelowSeaLevel,
    Lowland,
    Upland,
    Highland,
    Alpine,
}

/// An ISO 639-3 language code parsed at the source boundary.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct LanguageCode {
    code: String,
}

impl LanguageCode {
    pub fn as_str(&self) -> &str {
        &self.code
    }
}

impl FromStr for LanguageCode {
    type Err = InvalidLanguageCode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_lowercase()) {
            Ok(Self { code: value.into() })
        } else {
            Err(InvalidLanguageCode(value.into()))
        }
    }
}

impl TryFrom<String> for LanguageCode {
    type Error = InvalidLanguageCode;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<LanguageCode> for String {
    fn from(value: LanguageCode) -> Self {
        value.code
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidLanguageCode(String);

impl fmt::Display for InvalidLanguageCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "expected a lowercase ISO 639-3 code, got {:?}",
            self.0
        )
    }
}

impl std::error::Error for InvalidLanguageCode {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "lowercase")]
pub enum TravelEdgeKind {
    Land,
    Ferry,
}

impl TravelEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Land => "land",
            Self::Ferry => "ferry",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "lowercase")]
pub enum EdgeEndpoint {
    From,
    To,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct WaterDistanceMeters {
    meters: u16,
}

impl WaterDistanceMeters {
    pub const MAX: u16 = 10_000;

    pub fn new(meters: u16) -> Result<Self, String> {
        if meters <= Self::MAX {
            Ok(Self { meters })
        } else {
            Err(format!(
                "water distance {meters} exceeds {} meters",
                Self::MAX
            ))
        }
    }

    pub const fn get(self) -> u16 {
        self.meters
    }
}

impl<'de> Deserialize<'de> for WaterDistanceMeters {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            meters: u16,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.meters).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct StrahlerOrder {
    order: u8,
}

impl StrahlerOrder {
    pub const MAX: u8 = 12;

    pub fn new(order: u8) -> Result<Self, String> {
        if (1..=Self::MAX).contains(&order) {
            Ok(Self { order })
        } else {
            Err(format!(
                "Strahler order {order} is outside 1..={}",
                Self::MAX
            ))
        }
    }

    pub const fn get(self) -> u8 {
        self.order
    }
}

impl<'de> Deserialize<'de> for StrahlerOrder {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            order: u8,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.order).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum FlowPersistence {
    Perennial,
    Intermittent,
    Ephemeral,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RiverAccess {
    pub distance: WaterDistanceMeters,
    pub order: StrahlerOrder,
    pub persistence: FlowPersistence,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RiverAndCanalAccess {
    pub river: RiverAccess,
    pub canal_distance: WaterDistanceMeters,
    pub canal_navigable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum FlowingWaterAccess {
    River(RiverAccess),
    RiverAndCanal(RiverAndCanalAccess),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum InlandWaterSize {
    Pond,
    Lake,
    GreatLake,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct InlandWaterAccess {
    pub distance: WaterDistanceMeters,
    pub size: InlandWaterSize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum MarineWaterAccess {
    Tidal(WaterDistanceMeters),
    OpenCoast(WaterDistanceMeters),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SettlementHydrology {
    pub flowing: Option<FlowingWaterAccess>,
    pub inland: Option<InlandWaterAccess>,
    pub marine: Option<MarineWaterAccess>,
}

impl SettlementHydrology {
    pub const fn has_freshwater(self) -> bool {
        self.flowing.is_some() || self.inland.is_some()
    }

    pub const fn has_saltwater(self) -> bool {
        self.marine.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct EdgeProgressPermille {
    permille: u16,
}

impl EdgeProgressPermille {
    pub const MAX: u16 = 1_000;

    pub fn new(value: u16) -> Result<Self, String> {
        if value <= Self::MAX {
            Ok(Self { permille: value })
        } else {
            Err(format!("edge progress {value} exceeds {}", Self::MAX))
        }
    }

    pub const fn get(self) -> u16 {
        self.permille
    }
}

impl<'de> Deserialize<'de> for EdgeProgressPermille {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            permille: u16,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.permille).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum CrossingWatercourse {
    River(RiverWatercourse),
    Canal(CanalWatercourse),
    Ditch,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RiverWatercourse {
    pub order: StrahlerOrder,
    pub persistence: FlowPersistence,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct CanalWatercourse {
    pub navigable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum CrossingTraversal {
    Bridge,
    Ford,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct LandWaterCrossing {
    pub position: EdgeProgressPermille,
    pub watercourse: CrossingWatercourse,
    pub traversal: CrossingTraversal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum FerryWaterway {
    River(RiverWatercourse),
    InlandWater,
    TidalWater,
    CoastalWater,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum TravelRoute {
    Land(LandRoute),
    Ferry(FerryRoute),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct LandRoute {
    pub bridge: Option<EdgeEndpoint>,
    pub water_crossings: Vec<LandWaterCrossing>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct FerryRoute {
    pub waterway: FerryWaterway,
}

impl TravelRoute {
    pub const fn kind(&self) -> TravelEdgeKind {
        match self {
            Self::Land(_) => TravelEdgeKind::Land,
            Self::Ferry(_) => TravelEdgeKind::Ferry,
        }
    }

    pub const fn has_crossing(&self) -> bool {
        match self {
            Self::Land(route) => route.bridge.is_some() || !route.water_crossings.is_empty(),
            Self::Ferry(_) => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorldMetadata {
    pub schema_version: u32,
    pub world_year: i32,
    pub sources: Vec<SourceProvenance>,
    pub road_types: Vec<TravelEdgeKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceProvenance {
    pub name: String,
    pub url: String,
    pub license: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WorldBuildReport {
    pub nodes: usize,
    pub edges: usize,
    pub settlements: usize,
    pub settlement_aliases: usize,
    pub settlement_descriptions: usize,
    pub settlements_connected_to_road_network: usize,
    pub route_crossings: usize,
    pub toll_edges: usize,
    pub contradictory_feature_dates: usize,
    pub elevation_tiles_read: usize,
    pub elevation_samples: usize,
    pub elevation_fallback_samples: usize,
    pub land_use_rasters_read: usize,
    pub land_use_samples: usize,
    pub land_use_fallback_samples: usize,
    pub land_use_normalized_samples: usize,
    pub forest_tiles_read: usize,
    pub forest_samples: usize,
    pub forest_fallback_samples: usize,
    pub potential_vegetation_polygons_read: usize,
    pub potential_vegetation_samples: usize,
    pub potential_vegetation_fallback_samples: usize,
    pub tree_species_rasters_read: usize,
    pub tree_species_samples: usize,
    pub tree_species_fallback_samples: usize,
    pub tree_species_candidates: usize,
    pub soil_polygons_read: usize,
    pub soil_attribute_rows_read: usize,
    pub soil_samples: usize,
    pub soil_fallback_samples: usize,
    pub geology_features_read: usize,
    pub geology_samples: usize,
    pub geology_fallback_samples: usize,
    pub religion_regions_read: usize,
    pub religion_samples: usize,
    pub religion_fallback_samples: usize,
    pub drought_grid_cells_read: usize,
    pub drought_samples: usize,
    pub drought_neighbor_samples: usize,
    pub drought_fallback_samples: usize,
    pub hydrology_files_read: usize,
    pub hydrology_features_read: usize,
    pub hydrology_settlement_samples: usize,
    pub hydrology_landlocked_settlements: usize,
    pub hydrology_edge_crossings: usize,
    pub hydrology_inferred_ferry_waterways: usize,
    pub excluded_edges: std::collections::BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompiledWorld {
    pub metadata: WorldMetadata,
    pub nodes: Vec<WorldNodeImport>,
    pub edges: Vec<TravelEdgeImport>,
    pub settlements: Vec<SettlementImport>,
    pub settlement_aliases: Vec<SettlementAliasImport>,
    pub settlement_descriptions: Vec<SettlementDescriptionImport>,
    pub report: WorldBuildReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct WorldNodeImport {
    pub id: u64,
    pub parent_node_id: Option<u64>,
    pub latitude: f64,
    pub longitude: f64,
    pub is_settlement: bool,
    pub is_town: bool,
    pub is_ferry: bool,
    pub is_harbour: bool,
    pub sources: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct TravelEdgeImport {
    pub id: u64,
    pub from_node_id: u64,
    pub to_node_id: u64,
    pub route: TravelRoute,
    pub toll: Option<EdgeEndpoint>,
    pub length_m: u32,
    pub slope_multiplier: f32,
    pub certainty: u8,
    pub section: String,
    pub sources: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SettlementImport {
    pub id: String,
    pub source_node_id: u64,
    pub name: String,
    pub longitude: f64,
    pub latitude: f64,
    pub population_level: i32,
    pub population_estimate: u32,
    pub elevation: ElevationMeters,
    pub land_use: LandUseProfile,
    pub forest_cover: ForestCover,
    pub potential_vegetation: PotentialVegetation,
    pub tree_species: TreeSpeciesProfile,
    pub soil: SoilProfile,
    pub geology: SurfaceGeology,
    pub religious_status: SettlementReligiousStatus,
    pub drought: DroughtProfile,
    pub hydrology: SettlementHydrology,
    pub scene_key: String,
    pub sources: String,
}

#[cfg(test)]
mod tests {
    use super::{
        CanopyDensity, DroughtHistory, ElevationBand, ElevationMeters, EuroVegMapUnitCode,
        ForestCover, GeologicUnitId, HabitatSuitability, HumanLandUseIntensity,
        InferredTreeSpeciesProfile, LandUseFraction, LandUseProfile, MappedPotentialVegetation,
        ModeledTreeSpecies, ModeledTreeSpeciesProfile, NativeRangeEvidence, OfficialReligion,
        PalmerDroughtSeverityIndex, ParentMaterialCode, PotentialVegetationFormation,
        SettlementReligiousStatus, SoilMappingUnit, StoneContentPercent, SummerHydroclimate,
        TreeSpeciesId,
    };

    #[test]
    fn source_markdown_is_nonempty_nul_free_and_bounded() {
        assert!(super::valid_sources_markdown("- **Source:** Direct value."));
        assert!(!super::valid_sources_markdown("   \n"));
        assert!(!super::valid_sources_markdown("- Source\0hidden"));
        assert!(!super::valid_sources_markdown(
            &"x".repeat(super::MAX_SOURCES_MARKDOWN_CHARS + 1)
        ));
    }

    #[test]
    fn religious_status_derives_the_single_church_faith() {
        let status = SettlementReligiousStatus::MultiConfessional {
            arrangement: super::WesternChristianArrangement::CatholicLutheran {
                church: super::CatholicLutheranChurch::Lutheran,
            },
        };
        assert_eq!(status.church(), OfficialReligion::Lutheran);
        assert_eq!(status.church().faith_id(), "lutheran");
    }

    #[test]
    fn pdsi_is_bounded_and_classifies_hydroclimate() {
        assert!(PalmerDroughtSeverityIndex::new(-15_001).is_none());
        assert_eq!(
            PalmerDroughtSeverityIndex::new(-4_000).unwrap().condition(),
            SummerHydroclimate::ExtremeDrought
        );
        assert_eq!(
            PalmerDroughtSeverityIndex::new(0).unwrap().condition(),
            SummerHydroclimate::NearNormal
        );
        assert_eq!(
            PalmerDroughtSeverityIndex::new(4_000).unwrap().condition(),
            SummerHydroclimate::ExtremelyWet
        );
    }

    #[test]
    fn drought_history_rejects_impossible_twenty_year_counts() {
        let normal = PalmerDroughtSeverityIndex::new(0).unwrap();
        let drought = PalmerDroughtSeverityIndex::new(-4_000).unwrap();
        let impossible_mean = PalmerDroughtSeverityIndex::new(5_000).unwrap();
        assert!(DroughtHistory::new(drought, normal, 10, 10).is_some());
        assert!(DroughtHistory::new(normal, normal, 11, 10).is_none());
        assert!(DroughtHistory::new(normal, normal, 21, 0).is_none());
        assert!(DroughtHistory::new(drought, normal, 0, 0).is_none());
        assert!(DroughtHistory::new(normal, impossible_mean, 0, 0).is_none());
    }

    #[test]
    fn elevations_parse_into_bounded_values_and_bands() {
        assert!(ElevationMeters::new(ElevationMeters::MIN - 1).is_none());
        assert!(ElevationMeters::new(ElevationMeters::MAX + 1).is_none());
        assert_eq!(
            ElevationMeters::new(-1).unwrap().band(),
            ElevationBand::BelowSeaLevel
        );
        assert_eq!(
            ElevationMeters::new(0).unwrap().band(),
            ElevationBand::Lowland
        );
        assert_eq!(
            ElevationMeters::new(300).unwrap().band(),
            ElevationBand::Upland
        );
        assert_eq!(
            ElevationMeters::new(1_000).unwrap().band(),
            ElevationBand::Highland
        );
        assert_eq!(
            ElevationMeters::new(2_000).unwrap().band(),
            ElevationBand::Alpine
        );
        assert!(serde_json::from_str::<ElevationMeters>(r#"{"meters":9001}"#).is_err());
    }

    #[test]
    fn land_use_profiles_are_exhaustive_and_derive_intensity() {
        let profile = LandUseProfile::new(
            LandUseFraction::new(3_000).unwrap(),
            LandUseFraction::new(2_000).unwrap(),
            LandUseFraction::new(100).unwrap(),
            LandUseFraction::new(4_900).unwrap(),
        )
        .unwrap();
        assert_eq!(profile.intensity(), HumanLandUseIntensity::Intensive);
        assert!(
            LandUseProfile::new(
                LandUseFraction::new(3_000).unwrap(),
                LandUseFraction::new(2_000).unwrap(),
                LandUseFraction::new(100).unwrap(),
                LandUseFraction::new(4_800).unwrap(),
            )
            .is_none()
        );
        assert!(serde_json::from_str::<LandUseFraction>(r#"{"basis_points":10001}"#).is_err());
        assert!(serde_json::from_str::<LandUseProfile>(
            r#"{"cropland":{"basis_points":3000},"grazing":{"basis_points":2000},"built_up":{"basis_points":100},"natural":{"basis_points":4800}}"#
        )
        .is_err());
    }

    #[test]
    fn forest_cover_cannot_attach_zero_density_to_woodland() {
        assert!(CanopyDensity::new(0).is_none());
        assert!(CanopyDensity::new(101).is_none());
        assert!(CanopyDensity::new(50).is_some());
        assert!(serde_json::from_str::<CanopyDensity>(r#"{"percent":0}"#).is_err());
        assert_eq!(ForestCover::Open, ForestCover::Open);
    }

    #[test]
    fn eurovegmap_codes_parse_into_a_bounded_source_identifier() {
        assert_eq!(EuroVegMapUnitCode::new("F27").unwrap().as_str(), "F27");
        assert_eq!(
            EuroVegMapUnitCode::new("S18/19").unwrap().as_str(),
            "S18/19"
        );
        assert!(EuroVegMapUnitCode::new("").is_none());
        assert!(EuroVegMapUnitCode::new("future-code").is_none());
        assert!(serde_json::from_str::<EuroVegMapUnitCode>(r#"{"code":"bad code"}"#).is_err());
    }

    #[test]
    fn mapped_vegetation_parses_unit_and_formation_as_one_invariant() {
        let mapped = MappedPotentialVegetation::new(
            EuroVegMapUnitCode::new("F27").unwrap(),
            PotentialVegetationFormation::DeciduousAndMixedForest,
        )
        .unwrap();
        assert_eq!(mapped.unit().as_str(), "F27");
        assert_eq!(
            mapped.formation(),
            PotentialVegetationFormation::DeciduousAndMixedForest
        );
        assert!(
            MappedPotentialVegetation::new(
                EuroVegMapUnitCode::new("F27").unwrap(),
                PotentialVegetationFormation::Steppe,
            )
            .is_none()
        );
        assert!(
            serde_json::from_str::<MappedPotentialVegetation>(
                r#"{"unit":{"code":"F27"},"formation":"Steppe"}"#
            )
            .is_err()
        );
        assert!(
            MappedPotentialVegetation::new(
                EuroVegMapUnitCode::new("Glacier").unwrap(),
                PotentialVegetationFormation::PolarDesertAndNival,
            )
            .is_some()
        );
        assert!(
            MappedPotentialVegetation::new(
                EuroVegMapUnitCode::new("River").unwrap(),
                PotentialVegetationFormation::DeciduousAndMixedForest,
            )
            .is_some()
        );
    }

    #[test]
    fn tree_species_profiles_are_nonempty_unique_and_canonically_ranked() {
        let oak = TreeSpeciesId::new("Quercus_robur").unwrap();
        let beech = TreeSpeciesId::new("Fagus_sylvatica").unwrap();
        assert!(TreeSpeciesId::new("quercus robur").is_none());
        assert!(HabitatSuitability::new(1_001).is_none());
        let profile = ModeledTreeSpeciesProfile::new(vec![
            ModeledTreeSpecies {
                species: oak.clone(),
                suitability: HabitatSuitability::new(400).unwrap(),
                native_range: NativeRangeEvidence::WithinNativeRange,
            },
            ModeledTreeSpecies {
                species: beech,
                suitability: HabitatSuitability::new(800).unwrap(),
                native_range: NativeRangeEvidence::OutsideNativeRange,
            },
        ])
        .unwrap();
        assert_eq!(profile.candidates()[0].suitability.score(), 800);
        assert!(
            ModeledTreeSpeciesProfile::new(vec![
                ModeledTreeSpecies {
                    species: oak.clone(),
                    suitability: HabitatSuitability::new(400).unwrap(),
                    native_range: NativeRangeEvidence::WithinNativeRange,
                },
                ModeledTreeSpecies {
                    species: oak,
                    suitability: HabitatSuitability::new(500).unwrap(),
                    native_range: NativeRangeEvidence::OutsideNativeRange,
                },
            ])
            .is_none()
        );
        assert!(InferredTreeSpeciesProfile::new(Vec::new()).is_none());
        assert!(serde_json::from_str::<ModeledTreeSpeciesProfile>(r#"{"candidates":[]}"#).is_err());
    }

    #[test]
    fn soil_source_identifiers_and_percentages_are_bounded() {
        assert!(SoilMappingUnit::new(1, 2, 75).is_some());
        assert!(SoilMappingUnit::new(0, 2, 75).is_none());
        assert!(SoilMappingUnit::new(1, 2, 0).is_none());
        assert!(SoilMappingUnit::new(1, 2, 101).is_none());
        assert!(StoneContentPercent::new(100).is_some());
        assert!(StoneContentPercent::new(101).is_none());
        assert!(ParentMaterialCode::new("110").is_some());
        assert!(ParentMaterialCode::new("bad-code").is_none());
    }

    #[test]
    fn geologic_unit_identifiers_are_bounded_source_values() {
        assert_eq!(
            GeologicUnitId::new("FR-BRGM.1953.72852").unwrap().as_str(),
            "FR-BRGM.1953.72852"
        );
        assert!(GeologicUnitId::new("").is_none());
        assert!(GeologicUnitId::new(" leading-space").is_none());
        assert!(GeologicUnitId::new("x".repeat(256)).is_none());
    }

    #[test]
    fn hydrology_wire_values_cannot_bypass_bounded_constructors() {
        assert!(serde_json::from_str::<super::WaterDistanceMeters>(r#"{"meters":10000}"#).is_ok());
        assert!(serde_json::from_str::<super::WaterDistanceMeters>(r#"{"meters":10001}"#).is_err());
        assert!(serde_json::from_str::<super::StrahlerOrder>(r#"{"order":1}"#).is_ok());
        assert!(serde_json::from_str::<super::StrahlerOrder>(r#"{"order":0}"#).is_err());
        assert!(
            serde_json::from_str::<super::EdgeProgressPermille>(r#"{"permille":1000}"#).is_ok()
        );
        assert!(
            serde_json::from_str::<super::EdgeProgressPermille>(r#"{"permille":1001}"#).is_err()
        );
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettlementAliasImport {
    pub id: String,
    pub settlement_id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub language: Option<LanguageCode>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "lowercase")]
pub enum SettlementDescriptionKind {
    Settlement,
    City,
}

impl SettlementDescriptionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Settlement => "settlement",
            Self::City => "city",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettlementDescriptionImport {
    pub id: String,
    pub settlement_id: String,
    pub kind: SettlementDescriptionKind,
    pub language: Option<LanguageCode>,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::LanguageCode;

    #[test]
    fn language_codes_are_parsed_into_a_closed_representation() {
        assert_eq!(LanguageCode::from_str("deu").unwrap().as_str(), "deu");
        assert!(LanguageCode::from_str("DE").is_err());
        assert!(serde_json::from_str::<LanguageCode>("\"english\"").is_err());
    }
}
