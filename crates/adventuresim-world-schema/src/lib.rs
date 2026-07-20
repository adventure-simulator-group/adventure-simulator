//! Stable, source-independent types at the world compiler/database boundary.
//!
//! Keep this crate lightweight. Readers for CSV, raster, and vector formats
//! belong in `adventuresim-world-import`, not here or in the database module.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

pub const WORLD_SCHEMA_VERSION: u32 = 23;
pub const CURRENT_INFERENCE_RULES_VERSION: u32 = 7;
pub const MAX_SOURCES_MARKDOWN_CHARS: usize = 32_768;

/// Source and inference notes are deliberately unstructured Markdown for a
/// future debug view. Keep the payload bounded even though the contents are
/// not parsed into canonical provenance types.
pub fn valid_sources_markdown(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_SOURCES_MARKDOWN_CHARS
        && !value.contains('\0')
}

pub const SETTLEMENT_ALIAS_NAME_MAX_BYTES: usize = 256;
pub const SETTLEMENT_ALIAS_PREFIX_MAX_BYTES: usize = 128;
pub const SETTLEMENT_DESCRIPTION_MAX_BYTES: usize = 8_192;

/// Validates bounded, canonical external text before it enters compiled world data.
pub fn valid_bounded_source_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value == value.trim() && value.len() <= max_bytes && !value.contains('\0')
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum OfficialReligion {
    RomanCatholic,
    Lutheran,
    Reformed,
    Anglican,
    EasternOrthodox,
    Islamic,
    Judaism,
}

impl OfficialReligion {
    pub const ALL: [Self; 7] = [
        Self::RomanCatholic,
        Self::Lutheran,
        Self::Reformed,
        Self::Anglican,
        Self::EasternOrthodox,
        Self::Islamic,
        Self::Judaism,
    ];

    /// Stable identifier used by the current single-church gameplay systems.
    pub const fn religion_id(self) -> &'static str {
        match self {
            Self::RomanCatholic => "roman_catholic",
            Self::Lutheran => "lutheran",
            Self::Reformed => "reformed",
            Self::Anglican => "anglican",
            Self::EasternOrthodox => "eastern_orthodox",
            Self::Islamic => "islamic",
            Self::Judaism => "judaism",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::RomanCatholic => "Roman Catholicism",
            Self::Lutheran => "Lutheranism",
            Self::Reformed => "Reformed Christianity",
            Self::Anglican => "Anglicanism",
            Self::EasternOrthodox => "Eastern Orthodoxy",
            Self::Islamic => "Islam",
            Self::Judaism => "Judaism",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|religion| religion.religion_id() == id)
    }

    pub const fn index(self) -> usize {
        match self {
            Self::RomanCatholic => 0,
            Self::Lutheran => 1,
            Self::Reformed => 2,
            Self::Anglican => 3,
            Self::EasternOrthodox => 4,
            Self::Islamic => 5,
            Self::Judaism => 6,
        }
    }

    pub const fn correlation(self, other: Self) -> f32 {
        const C: [[f32; 7]; 7] = [
            [1.0, 0.80, 0.75, 0.80, 0.65, 0.10, 0.10],
            [0.80, 1.0, 0.90, 0.85, 0.50, 0.10, 0.10],
            [0.75, 0.90, 1.0, 0.85, 0.45, 0.10, 0.10],
            [0.80, 0.85, 0.85, 1.0, 0.55, 0.10, 0.10],
            [0.65, 0.50, 0.45, 0.55, 1.0, 0.15, 0.10],
            [0.10, 0.10, 0.10, 0.10, 0.15, 1.0, 0.35],
            [0.10, 0.10, 0.10, 0.10, 0.10, 0.35, 1.0],
        ];
        C[self.index()][other.index()]
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct ReligionHours {
    pub roman_catholic: f32,
    pub lutheran: f32,
    pub reformed: f32,
    pub anglican: f32,
    pub eastern_orthodox: f32,
    pub islamic: f32,
    pub judaism: f32,
}

impl ReligionHours {
    pub fn direct_values(self) -> impl Iterator<Item = (OfficialReligion, f32)> {
        OfficialReligion::ALL
            .into_iter()
            .map(move |religion| (religion, self.direct(religion)))
    }

    pub fn direct_fields_valid(self, maximum: f32) -> bool {
        maximum.is_finite()
            && maximum >= 0.0
            && self
                .direct_values()
                .all(|(_, hours)| hours.is_finite() && (0.0..=maximum).contains(&hours))
    }

    pub fn direct(self, religion: OfficialReligion) -> f32 {
        match religion {
            OfficialReligion::RomanCatholic => self.roman_catholic,
            OfficialReligion::Lutheran => self.lutheran,
            OfficialReligion::Reformed => self.reformed,
            OfficialReligion::Anglican => self.anglican,
            OfficialReligion::EasternOrthodox => self.eastern_orthodox,
            OfficialReligion::Islamic => self.islamic,
            OfficialReligion::Judaism => self.judaism,
        }
    }

    pub fn direct_mut(&mut self, religion: OfficialReligion) -> &mut f32 {
        match religion {
            OfficialReligion::RomanCatholic => &mut self.roman_catholic,
            OfficialReligion::Lutheran => &mut self.lutheran,
            OfficialReligion::Reformed => &mut self.reformed,
            OfficialReligion::Anglican => &mut self.anglican,
            OfficialReligion::EasternOrthodox => &mut self.eastern_orthodox,
            OfficialReligion::Islamic => &mut self.islamic,
            OfficialReligion::Judaism => &mut self.judaism,
        }
    }

    pub fn effective(self, religion: OfficialReligion) -> f32 {
        OfficialReligion::ALL
            .into_iter()
            .map(|studied| {
                let direct = self.direct(studied);
                (if direct.is_finite() {
                    direct.max(0.0)
                } else {
                    0.0
                }) * religion.correlation(studied)
            })
            .sum()
    }

    pub fn total_direct(self) -> f32 {
        OfficialReligion::ALL
            .into_iter()
            .map(|r| {
                let hours = self.direct(r);
                if hours.is_finite() {
                    hours.max(0.0)
                } else {
                    0.0
                }
            })
            .sum()
    }

    pub fn maximum_effective(self) -> f32 {
        OfficialReligion::ALL
            .into_iter()
            .map(|r| self.effective(r))
            .fold(0.0, f32::max)
    }

    pub fn add_direct(&mut self, religion: OfficialReligion, hours: f32) {
        if hours.is_finite() && hours > 0.0 {
            let direct = self.direct_mut(religion);
            if !direct.is_finite() || *direct < 0.0 {
                *direct = 0.0;
            }
            *direct += hours;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct ReligionMinutes {
    pub roman_catholic: u16,
    pub lutheran: u16,
    pub reformed: u16,
    pub anglican: u16,
    pub eastern_orthodox: u16,
    pub islamic: u16,
    pub judaism: u16,
}

impl ReligionMinutes {
    pub const fn get(self, religion: OfficialReligion) -> u16 {
        match religion {
            OfficialReligion::RomanCatholic => self.roman_catholic,
            OfficialReligion::Lutheran => self.lutheran,
            OfficialReligion::Reformed => self.reformed,
            OfficialReligion::Anglican => self.anglican,
            OfficialReligion::EasternOrthodox => self.eastern_orthodox,
            OfficialReligion::Islamic => self.islamic,
            OfficialReligion::Judaism => self.judaism,
        }
    }

    pub fn total(self) -> u64 {
        OfficialReligion::ALL
            .into_iter()
            .map(|r| u64::from(self.get(r)))
            .sum()
    }

    pub fn split_evenly(total: u16, targets: &[OfficialReligion]) -> Self {
        let mut result = Self::default();
        let mut targets = targets.to_vec();
        targets.sort_by_key(|religion| religion.index());
        targets.dedup();
        if targets.is_empty() {
            return result;
        }
        let count = targets.len() as u16;
        let base = total / count;
        let remainder = total % count;
        for (index, religion) in targets.into_iter().enumerate() {
            let value = base + u16::from(index < usize::from(remainder));
            match religion {
                OfficialReligion::RomanCatholic => result.roman_catholic = value,
                OfficialReligion::Lutheran => result.lutheran = value,
                OfficialReligion::Reformed => result.reformed = value,
                OfficialReligion::Anglican => result.anglican = value,
                OfficialReligion::EasternOrthodox => result.eastern_orthodox = value,
                OfficialReligion::Islamic => result.islamic = value,
                OfficialReligion::Judaism => result.judaism = value,
            }
        }
        result
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

    /// Religions legally represented by this settlement, in stable enum order.
    pub fn represented_religions(self) -> Vec<OfficialReligion> {
        let mut religions = match self {
            Self::Established { religion } => vec![religion],
            Self::LocallyDetermined { church } => vec![church],
            Self::Parity { arrangement } | Self::MultiConfessional { arrangement } => {
                match arrangement {
                    WesternChristianArrangement::CatholicLutheran { .. } => {
                        vec![OfficialReligion::RomanCatholic, OfficialReligion::Lutheran]
                    }
                    WesternChristianArrangement::CatholicReformed { .. } => {
                        vec![OfficialReligion::RomanCatholic, OfficialReligion::Reformed]
                    }
                    WesternChristianArrangement::LutheranReformed { .. } => {
                        vec![OfficialReligion::Lutheran, OfficialReligion::Reformed]
                    }
                }
            }
        };
        religions.sort_by_key(|religion| religion.index());
        religions
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum PotentialVegetationClass {
    WoodlandAndForest,
    HeathlandAndShrub,
    Grassland,
    SparselyVegetatedAreas,
    Wetlands,
    MarineInletsAndTransitionalWaters,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SuitabilityBasisPoints {
    basis_points: u16,
}

impl SuitabilityBasisPoints {
    pub const fn new(value: u16) -> Option<Self> {
        if value <= 10_000 {
            Some(Self {
                basis_points: value,
            })
        } else {
            None
        }
    }
    pub const fn get(self) -> u16 {
        self.basis_points
    }
}

impl<'de> Deserialize<'de> for SuitabilityBasisPoints {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            basis_points: u16,
        }
        let value = Wire::deserialize(deserializer)?.basis_points;
        Self::new(value)
            .ok_or_else(|| serde::de::Error::custom("suitability exceeds 10000 basis points"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct PotentialVegetationPosterior {
    pub woodland_and_forest: SuitabilityBasisPoints,
    pub heathland_and_shrub: SuitabilityBasisPoints,
    pub grassland: SuitabilityBasisPoints,
    pub sparsely_vegetated_areas: SuitabilityBasisPoints,
    pub wetlands: SuitabilityBasisPoints,
    pub marine_inlets_and_transitional_waters: SuitabilityBasisPoints,
}

impl PotentialVegetationPosterior {
    pub fn dominant_class(&self) -> PotentialVegetationClass {
        use PotentialVegetationClass::*;
        let values = [
            (WoodlandAndForest, self.woodland_and_forest.get()),
            (HeathlandAndShrub, self.heathland_and_shrub.get()),
            (Grassland, self.grassland.get()),
            (SparselyVegetatedAreas, self.sparsely_vegetated_areas.get()),
            (Wetlands, self.wetlands.get()),
            (
                MarineInletsAndTransitionalWaters,
                self.marine_inlets_and_transitional_waters.get(),
            ),
        ];
        let mut dominant = values[0];
        for candidate in values.into_iter().skip(1) {
            if candidate.1 > dominant.1 {
                dominant = candidate;
            }
        }
        dominant.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum PotentialVegetation {
    Posterior(PotentialVegetationPosterior),
    Categorical(PotentialVegetationClass),
    Inferred(PotentialVegetationClass),
}

impl PotentialVegetation {
    pub fn class(&self) -> PotentialVegetationClass {
        match self {
            Self::Posterior(values) => values.dominant_class(),
            Self::Categorical(class) | Self::Inferred(class) => *class,
        }
    }
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
    Stagnosol,
    Umbrisol,
    Vertisol,
}

/// A bounded probability or confidence value in basis points.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SoilBasisPoints {
    value: u16,
}

impl SoilBasisPoints {
    pub const fn new(value: u16) -> Option<Self> {
        if value <= 10_000 {
            Some(Self { value })
        } else {
            None
        }
    }
    pub const fn get(self) -> u16 {
        self.value
    }
}

impl<'de> Deserialize<'de> for SoilBasisPoints {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            value: u16,
        }
        Self::new(Wire::deserialize(deserializer)?.value)
            .ok_or_else(|| serde::de::Error::custom("soil basis points must be 0..=10000"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SoilAcidity {
    StronglyAcid,
    Acid,
    Neutral,
    Alkaline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum CationExchangeCapacity {
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SoilFertility {
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SoilEvidence {
    SoilGridsPrediction,
    DeterministicInference,
}

/// Source prediction retained through geology and hydrology finalization.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SoilPrediction {
    pub wrb_group: WrbReferenceGroup,
    pub histosol_probability: SoilBasisPoints,
    pub leptosol_probability: SoilBasisPoints,
    pub texture: MineralSoilTexture,
    pub available_water: AvailableWaterCapacity,
    pub organic_carbon: TopsoilOrganicCarbon,
    pub stones: StoneContentPercent,
    pub acidity: SoilAcidity,
    pub cation_exchange_capacity: CationExchangeCapacity,
    pub fertility: SoilFertility,
    pub confidence: SoilBasisPoints,
    pub evidence: SoilEvidence,
}

/// Canonical soil after predictions are resolved against geology and hydrology.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SoilProfile {
    pub wrb_group: WrbReferenceGroup,
    pub parent_material: SurfaceLithology,
    pub properties: SoilProperties,
    pub acidity: SoilAcidity,
    pub cation_exchange_capacity: CationExchangeCapacity,
    pub fertility: SoilFertility,
    pub confidence: SoilBasisPoints,
    pub evidence: SoilEvidence,
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

/// Reconstructed dominant surface cover near a settlement in the world year.
/// This is deliberately distinct from [`PotentialVegetation`], which describes
/// the modern-climate ecological envelope in the absence of historical land use.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct BuiltSettlementCover {
    pub built_fraction: LandUseFraction,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct CroplandCover {
    pub cultivated_fraction: LandUseFraction,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct PastureCover {
    pub grazing_fraction: LandUseFraction,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct HistoricalWoodland {
    pub canopy: CanopyDensity,
    pub dominant: DominantLeafType,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct HistoricalWetland {
    pub water_regime: SoilWaterRegime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DirectHistoricalVegetationCover {
    BuiltSettlement(BuiltSettlementCover),
    Cropland(CroplandCover),
    Pasture(PastureCover),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DerivedHistoricalVegetationCover {
    Woodland(HistoricalWoodland),
    HeathAndShrub,
    Grassland,
    Sparse,
    Wetland(HistoricalWetland),
    TransitionalWater,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum FallbackHistoricalVegetationCover {
    Woodland(HistoricalWoodland),
    HeathAndShrub,
    Grassland,
    Sparse,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DirectHistoricalVegetationMethod {
    Hyde35DominantLandUse,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DerivedHistoricalVegetationMethod {
    MultiSourceRulesV4,
    MultiSourceRulesV4TieBreak,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum FallbackHistoricalVegetationMethod {
    PotentialEnvelopeV4,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct DirectHistoricalVegetation {
    pub cover: DirectHistoricalVegetationCover,
    pub method: DirectHistoricalVegetationMethod,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct DerivedHistoricalVegetation {
    pub cover: DerivedHistoricalVegetationCover,
    pub method: DerivedHistoricalVegetationMethod,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct FallbackHistoricalVegetation {
    pub cover: FallbackHistoricalVegetationCover,
    pub method: FallbackHistoricalVegetationMethod,
}

/// Closed evidence-bearing reconstruction. Variant-specific methods prevent a
/// serialized confidence blob or an incompatible evidence/method combination.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum HistoricalVegetation {
    Direct(DirectHistoricalVegetation),
    Derived(DerivedHistoricalVegetation),
    Fallback(FallbackHistoricalVegetation),
}

impl HistoricalVegetation {
    pub const fn evidence(self) -> HistoricalVegetationEvidence {
        match self {
            Self::Direct(_) => HistoricalVegetationEvidence::Direct,
            Self::Derived(_) => HistoricalVegetationEvidence::Derived,
            Self::Fallback(_) => HistoricalVegetationEvidence::Fallback,
        }
    }
}

/// Cross-field invariants shared by offline validation and the strategic import
/// boundary. Evidence-specific enums make invalid category pairings
/// unrepresentable; this predicate validates the surrounding source context.
pub fn historical_vegetation_matches_context(
    historical: HistoricalVegetation,
    land_use: LandUseProfile,
    potential: &PotentialVegetation,
    soil: SoilProfile,
    hydrology: SettlementHydrology,
) -> bool {
    match historical {
        HistoricalVegetation::Direct(value) => {
            let crop = land_use.cropland().basis_points();
            let grazing = land_use.grazing().basis_points();
            let built = land_use.built_up().basis_points();
            // Stable tie order is built, cropland, pasture.
            if built >= crop && built >= grazing {
                matches!(value.cover, DirectHistoricalVegetationCover::BuiltSettlement(v) if v.built_fraction == land_use.built_up() && built >= 1_000)
            } else if crop >= grazing {
                matches!(value.cover, DirectHistoricalVegetationCover::Cropland(v) if v.cultivated_fraction == land_use.cropland() && crop >= 3_500)
            } else {
                matches!(value.cover, DirectHistoricalVegetationCover::Pasture(v) if v.grazing_fraction == land_use.grazing() && grazing >= 3_500)
            }
        }
        HistoricalVegetation::Derived(value) => match value.cover {
            DerivedHistoricalVegetationCover::Wetland(wetland) => {
                wetland_context_is_convergent(potential, soil, hydrology)
                    && wetland.water_regime == soil.properties.water_regime
            }
            DerivedHistoricalVegetationCover::TransitionalWater => {
                matches!(hydrology.marine, Some(MarineWaterAccess::Tidal(_)))
            }
            _ => true,
        },
        HistoricalVegetation::Fallback(_) => true,
    }
}

pub fn wetland_context_is_convergent(
    potential: &PotentialVegetation,
    soil: SoilProfile,
    hydrology: SettlementHydrology,
) -> bool {
    potential.class() == PotentialVegetationClass::Wetlands
        && matches!(
            soil.properties.water_regime,
            SoilWaterRegime::LongSeasonWet | SoilWaterRegime::PermanentlyWet
        )
        && (hydrology.has_freshwater()
            || matches!(hydrology.marine, Some(MarineWaterAccess::Tidal(_))))
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HistoricalVegetationEvidence {
    Direct,
    Derived,
    Fallback,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum ProductionScale {
    Marginal,
    Local,
    Regional,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum AgriculturalCommodity {
    Grain,
    Flax,
    Wool,
    Dairy,
    Hides,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum FishCommodity {
    Freshwater,
    Estuarine,
    Marine,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum QuarryCommodity {
    Limestone,
    Chalk,
    Sandstone,
    Slate,
    Granite,
    Basalt,
    Marble,
    Quartzite,
    OtherHardStone,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum MinedCommodity {
    Coal,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum PotteryCommodity {
    Clay,
    Earthenware,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum ForestCommodity {
    Hardwood,
    Softwood,
    Mixed,
    Fuelwood,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SaltSource {
    Evaporite,
    SalineSoil,
    CoastalBrine,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum ConstructionCommodity {
    DimensionStone,
    Sand,
    Gravel,
    Brick,
    RoofTile,
    Timber,
}

macro_rules! commodity_industry_record {
    ($name:ident, $field:ident, $commodity:ty) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
        #[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
        pub struct $name {
            pub $field: $commodity,
            pub scale: ProductionScale,
        }
    };
}
commodity_industry_record!(AgricultureIndustry, commodity, AgriculturalCommodity);
commodity_industry_record!(FishingIndustry, commodity, FishCommodity);
commodity_industry_record!(QuarryingIndustry, commodity, QuarryCommodity);
commodity_industry_record!(MiningIndustry, commodity, MinedCommodity);
commodity_industry_record!(PotteryIndustry, commodity, PotteryCommodity);
commodity_industry_record!(ForestryIndustry, commodity, ForestCommodity);
commodity_industry_record!(SaltmakingIndustry, source, SaltSource);
commodity_industry_record!(ConstructionIndustry, commodity, ConstructionCommodity);
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct PeatCuttingIndustry {
    pub scale: ProductionScale,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct CharcoalBurningIndustry {
    pub scale: ProductionScale,
}

/// Evidence-bearing production output. Commodity domains are nested in their
/// industries, so impossible industry/commodity pairs cannot be represented.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DerivedIndustry {
    Agriculture(AgricultureIndustry),
    Fishing(FishingIndustry),
    Quarrying(QuarryingIndustry),
    Mining(MiningIndustry),
    Pottery(PotteryIndustry),
    PeatCutting(PeatCuttingIndustry),
    Forestry(ForestryIndustry),
    CharcoalBurning(CharcoalBurningIndustry),
    Saltmaking(SaltmakingIndustry),
    Construction(ConstructionIndustry),
}

impl DerivedIndustry {
    pub const fn scale(self) -> ProductionScale {
        match self {
            Self::Agriculture(v) => v.scale,
            Self::Fishing(v) => v.scale,
            Self::Quarrying(v) => v.scale,
            Self::Mining(v) => v.scale,
            Self::Pottery(v) => v.scale,
            Self::PeatCutting(v) => v.scale,
            Self::Forestry(v) => v.scale,
            Self::CharcoalBurning(v) => v.scale,
            Self::Saltmaking(v) => v.scale,
            Self::Construction(v) => v.scale,
        }
    }
}

/// Restricted last-resort outputs. A fallback cannot claim arbitrary scale,
/// deposits, or unsupported metal production.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum FallbackIndustry {
    FreshwaterFishing,
    GrazingDairy,
    CroplandGrain,
    WoodlandFuelwood,
    CommonAggregate,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum IndustryEvidence {
    Derived(DerivedIndustry),
    Fallback(FallbackIndustry),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct InferredIndustryProfile {
    outputs: Vec<IndustryEvidence>,
}

impl InferredIndustryProfile {
    pub const MAX_OUTPUTS: usize = 24;

    pub fn new(mut outputs: Vec<IndustryEvidence>) -> Option<Self> {
        if outputs.is_empty() || outputs.len() > Self::MAX_OUTPUTS {
            return None;
        }
        outputs.sort_unstable();
        if outputs.windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        Some(Self { outputs })
    }

    pub fn outputs(&self) -> &[IndustryEvidence] {
        &self.outputs
    }

    /// Constructor-independent validation for raw SpacetimeDB decoding.
    pub fn validate(&self) -> Result<(), String> {
        if self.outputs.is_empty() {
            return Err("industry profile is empty".into());
        }
        if self.outputs.len() > Self::MAX_OUTPUTS {
            return Err("industry profile exceeds 24 outputs".into());
        }
        if self.outputs.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("industry outputs are duplicated or not canonically ordered".into());
        }
        if self.outputs.iter().any(|v| matches!(v, IndustryEvidence::Derived(d) if d.scale() == ProductionScale::Regional && matches!(d, DerivedIndustry::PeatCutting(_)))) {
            return Err("regional peat cutting is outside the rules-v6 model".into());
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for InferredIndustryProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct IndustryOutputSequence(Vec<IndustryEvidence>);
        impl<'de> Deserialize<'de> for IndustryOutputSequence {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_seq(ProfileVisitor).map(Self)
            }
        }
        struct ProfileVisitor;
        impl<'de> serde::de::Visitor<'de> for ProfileVisitor {
            type Value = Vec<IndustryEvidence>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an industry output sequence of at most 24 values")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut outputs = Vec::with_capacity(
                    seq.size_hint()
                        .unwrap_or(0)
                        .min(InferredIndustryProfile::MAX_OUTPUTS),
                );
                while let Some(value) = seq.next_element()? {
                    if outputs.len() == InferredIndustryProfile::MAX_OUTPUTS {
                        return Err(serde::de::Error::custom(
                            "industry profile exceeds 24 outputs",
                        ));
                    }
                    outputs.push(value);
                }
                Ok(outputs)
            }
        }
        #[derive(Deserialize)]
        struct Wire {
            outputs: IndustryOutputSequence,
        }
        let wire = Wire::deserialize(deserializer)?;
        let profile = InferredIndustryProfile {
            outputs: wire.outputs.0,
        };
        profile.validate().map_err(serde::de::Error::custom)?;
        Ok(profile)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IndustryInferenceContext<'a> {
    pub elevation: ElevationMeters,
    pub drought: DroughtProfile,
    pub land_use: LandUseProfile,
    pub historical_vegetation: HistoricalVegetation,
    pub soil: SoilProfile,
    pub geology: &'a SurfaceGeology,
    pub hydrology: SettlementHydrology,
    pub population_estimate: u32,
    pub max_scale: ProductionScale,
}

/// Single canonical rules-v6 implementation. Compilation and both import
/// boundaries compare the complete result, including order and scales.
pub fn infer_industries(
    c: IndustryInferenceContext<'_>,
) -> Result<InferredIndustryProfile, String> {
    let mut out = Vec::new();
    let add = |out: &mut Vec<_>, value| out.push(IndustryEvidence::Derived(value));
    let crop = c.land_use.cropland().basis_points();
    let grazing = c.land_use.grazing().basis_points();
    let built = c.land_use.built_up().basis_points();
    let wet = matches!(
        c.soil.properties.water_regime,
        SoilWaterRegime::LongSeasonWet | SoilWaterRegime::PermanentlyWet
    );
    let mean = match c.drought {
        DroughtProfile::Reconstructed(v) | DroughtProfile::Inferred(v) => {
            v.twenty_year_mean().milli_units()
        }
    };
    let dry = mean <= -2_000 || c.soil.properties.water_regime == SoilWaterRegime::UsuallyDry;
    let fertile = !matches!(
        c.soil.fertility,
        SoilFertility::VeryLow | SoilFertility::Low
    );
    if crop >= 1_500
        && fertile
        && !dry
        && c.soil.properties.agricultural_limitation != AgriculturalLimitation::Flooded
    {
        add(
            &mut out,
            DerivedIndustry::Agriculture(AgricultureIndustry {
                commodity: AgriculturalCommodity::Grain,
                scale: canonical_industry_scale(
                    crop.saturating_add(industry_pop_score(c.population_estimate)),
                    c.max_scale,
                ),
            }),
        );
        if crop >= 3_000
            && matches!(
                c.soil.fertility,
                SoilFertility::High | SoilFertility::VeryHigh
            )
            && !wet
            && c.elevation.get() <= 300
        {
            add(
                &mut out,
                DerivedIndustry::Agriculture(AgricultureIndustry {
                    commodity: AgriculturalCommodity::Flax,
                    scale: canonical_industry_scale(
                        crop.saturating_add(industry_pop_score(c.population_estimate))
                            .saturating_sub(500),
                        c.max_scale,
                    ),
                }),
            );
        }
    }
    if grazing >= 1_500 {
        let scale = canonical_industry_scale(
            grazing.saturating_add(industry_pop_score(c.population_estimate)),
            c.max_scale,
        );
        for commodity in [
            AgriculturalCommodity::Wool,
            AgriculturalCommodity::Dairy,
            AgriculturalCommodity::Hides,
        ] {
            add(
                &mut out,
                DerivedIndustry::Agriculture(AgricultureIndustry { commodity, scale }),
            );
        }
    }
    let mut freshwater = false;
    if let Some(flowing) = c.hydrology.flowing {
        let river = match flowing {
            FlowingWaterAccess::River(v) => v,
            FlowingWaterAccess::RiverAndCanal(v) => v.river,
        };
        if river.distance.get() <= 5_000 {
            add(
                &mut out,
                DerivedIndustry::Fishing(FishingIndustry {
                    commodity: FishCommodity::Freshwater,
                    scale: canonical_industry_scale(
                        u16::from(river.order.get())
                            .saturating_mul(1_000)
                            .saturating_add(industry_pop_score(c.population_estimate)),
                        c.max_scale,
                    ),
                }),
            );
            freshwater = true;
        }
    }
    if !freshwater && let Some(inland) = c.hydrology.inland.filter(|v| v.distance.get() <= 10_000) {
        let size = match inland.size {
            InlandWaterSize::Pond => 1_500,
            InlandWaterSize::Lake => 4_000,
            InlandWaterSize::GreatLake => 7_000,
        };
        add(
            &mut out,
            DerivedIndustry::Fishing(FishingIndustry {
                commodity: FishCommodity::Freshwater,
                scale: canonical_industry_scale(
                    size + industry_pop_score(c.population_estimate),
                    c.max_scale,
                ),
            }),
        );
    }
    match c.hydrology.marine {
        Some(MarineWaterAccess::Tidal(d)) if d.get() <= 10_000 => add(
            &mut out,
            DerivedIndustry::Fishing(FishingIndustry {
                commodity: FishCommodity::Estuarine,
                scale: canonical_industry_scale(
                    5_000 + industry_pop_score(c.population_estimate),
                    c.max_scale,
                ),
            }),
        ),
        Some(MarineWaterAccess::OpenCoast(d)) if d.get() <= 10_000 => add(
            &mut out,
            DerivedIndustry::Fishing(FishingIndustry {
                commodity: FishCommodity::Marine,
                scale: canonical_industry_scale(
                    5_000 + industry_pop_score(c.population_estimate),
                    c.max_scale,
                ),
            }),
        ),
        _ => {}
    }
    let lith = canonical_industry_lithology(c.geology);
    if let Some(commodity) = canonical_quarry(lith) {
        add(
            &mut out,
            DerivedIndustry::Quarrying(QuarryingIndustry {
                commodity,
                scale: canonical_industry_scale(
                    5_000 + industry_pop_score(c.population_estimate),
                    c.max_scale,
                ),
            }),
        );
    }
    if lith == SurfaceLithology::Sedimentary(SedimentaryRock::Coal) {
        add(
            &mut out,
            DerivedIndustry::Mining(MiningIndustry {
                commodity: MinedCommodity::Coal,
                scale: canonical_industry_scale(
                    6_000 + industry_pop_score(c.population_estimate),
                    c.max_scale,
                ),
            }),
        );
    }
    let clay = matches!(c.soil.properties.substrate, SoilSubstrate::Mineral(v) if matches!(v.texture, MineralSoilTexture::Fine | MineralSoilTexture::VeryFine))
        || matches!(
            lith,
            SurfaceLithology::Unconsolidated(
                UnconsolidatedDeposit::Clay | UnconsolidatedDeposit::Alluvium
            ) | SurfaceLithology::Sedimentary(SedimentaryRock::Mudstone | SedimentaryRock::Marl)
        );
    if clay {
        add(
            &mut out,
            DerivedIndustry::Pottery(PotteryIndustry {
                commodity: if built >= 500 || c.population_estimate >= 1_000 {
                    PotteryCommodity::Earthenware
                } else {
                    PotteryCommodity::Clay
                },
                scale: canonical_industry_scale(
                    3_500 + industry_pop_score(c.population_estimate),
                    c.max_scale,
                ),
            }),
        );
        if built >= 500 {
            add(
                &mut out,
                DerivedIndustry::Construction(ConstructionIndustry {
                    commodity: ConstructionCommodity::Brick,
                    scale: canonical_industry_scale(
                        4_000 + industry_pop_score(c.population_estimate),
                        c.max_scale,
                    ),
                }),
            );
        }
        if built >= 1_000 {
            add(
                &mut out,
                DerivedIndustry::Construction(ConstructionIndustry {
                    commodity: ConstructionCommodity::RoofTile,
                    scale: canonical_industry_scale(
                        4_000 + industry_pop_score(c.population_estimate),
                        c.max_scale,
                    ),
                }),
            );
        }
    }
    let peat_parent = matches!(c.soil.wrb_group, WrbReferenceGroup::Histosol)
        || matches!(c.soil.properties.substrate, SoilSubstrate::Organic(_))
        || lith == SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Peat);
    if peat_parent
        && wet
        && (c.hydrology.has_freshwater() || canonical_historical_wet(c.historical_vegetation))
    {
        add(
            &mut out,
            DerivedIndustry::PeatCutting(PeatCuttingIndustry {
                scale: canonical_industry_scale(3_500, c.max_scale).min(ProductionScale::Local),
            }),
        );
    }
    if let Some(leaf) = canonical_historical_woodland(c.historical_vegetation) {
        let commodity = match leaf {
            DominantLeafType::Broadleaf => ForestCommodity::Hardwood,
            DominantLeafType::Coniferous => ForestCommodity::Softwood,
            DominantLeafType::Mixed => ForestCommodity::Mixed,
        };
        let scale = canonical_industry_scale(
            4_500 + industry_pop_score(c.population_estimate),
            c.max_scale,
        );
        add(
            &mut out,
            DerivedIndustry::Forestry(ForestryIndustry { commodity, scale }),
        );
        add(
            &mut out,
            DerivedIndustry::Forestry(ForestryIndustry {
                commodity: ForestCommodity::Fuelwood,
                scale,
            }),
        );
        add(
            &mut out,
            DerivedIndustry::Construction(ConstructionIndustry {
                commodity: ConstructionCommodity::Timber,
                scale,
            }),
        );
        if built >= 500 || c.population_estimate >= 1_000 {
            add(
                &mut out,
                DerivedIndustry::CharcoalBurning(CharcoalBurningIndustry {
                    scale: canonical_industry_scale(
                        4_000 + industry_pop_score(c.population_estimate),
                        c.max_scale,
                    ),
                }),
            );
        }
    }
    let fuel = out.iter().any(|v| {
        matches!(
            v,
            IndustryEvidence::Derived(
                DerivedIndustry::Forestry(ForestryIndustry {
                    commodity: ForestCommodity::Fuelwood,
                    ..
                }) | DerivedIndustry::PeatCutting(_)
            )
        )
    });
    let salt_source = if lith == SurfaceLithology::Sedimentary(SedimentaryRock::Evaporite) {
        Some((SaltSource::Evaporite, 6_000))
    } else if c.soil.properties.agricultural_limitation == AgriculturalLimitation::Saline {
        Some((SaltSource::SalineSoil, 3_500))
    } else if fuel
        && matches!(c.hydrology.marine, Some(MarineWaterAccess::OpenCoast(d)) if d.get() <= 5_000)
    {
        Some((SaltSource::CoastalBrine, 3_500))
    } else {
        None
    };
    if let Some((source, score)) = salt_source {
        add(
            &mut out,
            DerivedIndustry::Saltmaking(SaltmakingIndustry {
                source,
                scale: canonical_industry_scale(score, c.max_scale),
            }),
        );
    }
    let construction_scale = canonical_industry_scale(
        4_000 + industry_pop_score(c.population_estimate),
        c.max_scale,
    );
    match lith {
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Sand) => add(
            &mut out,
            DerivedIndustry::Construction(ConstructionIndustry {
                commodity: ConstructionCommodity::Sand,
                scale: construction_scale,
            }),
        ),
        SurfaceLithology::Unconsolidated(
            UnconsolidatedDeposit::Gravel
            | UnconsolidatedDeposit::MixedSediment
            | UnconsolidatedDeposit::Alluvium,
        ) => add(
            &mut out,
            DerivedIndustry::Construction(ConstructionIndustry {
                commodity: ConstructionCommodity::Gravel,
                scale: construction_scale,
            }),
        ),
        _ if canonical_quarry(lith).is_some() => add(
            &mut out,
            DerivedIndustry::Construction(ConstructionIndustry {
                commodity: ConstructionCommodity::DimensionStone,
                scale: construction_scale,
            }),
        ),
        _ => {}
    }
    if out.is_empty() {
        out.push(IndustryEvidence::Fallback(
            if c.hydrology.has_freshwater() {
                FallbackIndustry::FreshwaterFishing
            } else if grazing > 0 {
                FallbackIndustry::GrazingDairy
            } else if crop > 0 {
                FallbackIndustry::CroplandGrain
            } else if canonical_historical_woodland(c.historical_vegetation).is_some() {
                FallbackIndustry::WoodlandFuelwood
            } else {
                FallbackIndustry::CommonAggregate
            },
        ));
    }
    InferredIndustryProfile::new(out)
        .ok_or_else(|| "canonical industry inference produced invalid output".into())
}

pub fn industry_profile_is_canonical(
    profile: &InferredIndustryProfile,
    context: IndustryInferenceContext<'_>,
) -> bool {
    infer_industries(context).is_ok_and(|expected| expected == *profile)
}
fn industry_pop_score(population: u32) -> u16 {
    u16::try_from(population / 5).unwrap_or(u16::MAX).min(2_000)
}
fn canonical_industry_scale(score: u16, cap: ProductionScale) -> ProductionScale {
    (if score >= 7_000 {
        ProductionScale::Regional
    } else if score >= 4_000 {
        ProductionScale::Local
    } else {
        ProductionScale::Marginal
    })
    .min(cap)
}
fn canonical_industry_lithology(g: &SurfaceGeology) -> SurfaceLithology {
    match g {
        SurfaceGeology::Mapped(v) => match v.setting.lithology {
            GeologicLithologyEvidence::Mapped(l) | GeologicLithologyEvidence::Inferred(l) => l,
        },
        SurfaceGeology::Inferred(v) => v.lithology,
    }
}
fn canonical_quarry(l: SurfaceLithology) -> Option<QuarryCommodity> {
    match l {
        SurfaceLithology::Sedimentary(SedimentaryRock::Limestone | SedimentaryRock::Dolostone) => {
            Some(QuarryCommodity::Limestone)
        }
        SurfaceLithology::Sedimentary(SedimentaryRock::Chalk) => Some(QuarryCommodity::Chalk),
        SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone) => {
            Some(QuarryCommodity::Sandstone)
        }
        SurfaceLithology::Metamorphic(MetamorphicRock::Slate) => Some(QuarryCommodity::Slate),
        SurfaceLithology::Igneous(IgneousRock::Granite | IgneousRock::Granitoid) => {
            Some(QuarryCommodity::Granite)
        }
        SurfaceLithology::Igneous(IgneousRock::Basalt) => Some(QuarryCommodity::Basalt),
        SurfaceLithology::Metamorphic(MetamorphicRock::Marble) => Some(QuarryCommodity::Marble),
        SurfaceLithology::Metamorphic(MetamorphicRock::Quartzite) => {
            Some(QuarryCommodity::Quartzite)
        }
        SurfaceLithology::Igneous(_)
        | SurfaceLithology::Metamorphic(_)
        | SurfaceLithology::Mixed(_) => Some(QuarryCommodity::OtherHardStone),
        _ => None,
    }
}
fn canonical_historical_wet(v: HistoricalVegetation) -> bool {
    matches!(v, HistoricalVegetation::Derived(d) if matches!(d.cover, DerivedHistoricalVegetationCover::Wetland(_) | DerivedHistoricalVegetationCover::TransitionalWater))
}
fn canonical_historical_woodland(v: HistoricalVegetation) -> Option<DominantLeafType> {
    match v {
        HistoricalVegetation::Derived(d) => match d.cover {
            DerivedHistoricalVegetationCover::Woodland(v) => Some(v.dominant),
            _ => None,
        },
        HistoricalVegetation::Fallback(f) => match f.cover {
            FallbackHistoricalVegetationCover::Woodland(v) => Some(v.dominant),
            _ => None,
        },
        HistoricalVegetation::Direct(_) => None,
    }
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

/// One bounded sample of a route's straight endpoint-to-endpoint DEM profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RouteElevationSample {
    pub progress: EdgeProgressPermille,
    pub elevation: ElevationMeters,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RouteElevationProfile {
    samples: Vec<RouteElevationSample>,
}

struct BoundedVec<T, const MAX: usize>(Vec<T>);

impl<'de, T: Deserialize<'de>, const MAX: usize> Deserialize<'de> for BoundedVec<T, MAX> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BoundedVisitor<T, const MAX: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const MAX: usize> serde::de::Visitor<'de>
            for BoundedVisitor<T, MAX>
        {
            type Value = BoundedVec<T, MAX>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "a sequence with at most {MAX} elements")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                if sequence.size_hint().is_some_and(|hint| hint > MAX) {
                    return Err(serde::de::Error::custom(format_args!(
                        "sequence exceeds {MAX} elements"
                    )));
                }
                let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX));
                while let Some(value) = sequence.next_element()? {
                    if values.len() == MAX {
                        return Err(serde::de::Error::custom(format_args!(
                            "sequence exceeds {MAX} elements"
                        )));
                    }
                    values.push(value);
                }
                Ok(BoundedVec(values))
            }
        }
        deserializer.deserialize_seq(BoundedVisitor::<T, MAX>(std::marker::PhantomData))
    }
}

impl RouteElevationProfile {
    pub const MAX_SAMPLES: usize = 1_001;

    pub fn new(samples: Vec<RouteElevationSample>) -> Result<Self, String> {
        if samples.len() < 2 || samples.len() > Self::MAX_SAMPLES {
            return Err(format!(
                "route elevation profile must contain 2..={} samples",
                Self::MAX_SAMPLES
            ));
        }
        if samples.first().map(|v| v.progress.get()) != Some(0)
            || samples.last().map(|v| v.progress.get()) != Some(1_000)
            || samples
                .windows(2)
                .any(|pair| pair[0].progress >= pair[1].progress)
        {
            return Err(
                "route elevation profile must have sorted unique progress and 0/1000 endpoints"
                    .into(),
            );
        }
        Ok(Self { samples })
    }

    pub fn samples(&self) -> &[RouteElevationSample] {
        &self.samples
    }

    fn validate_raw(&self) -> Result<(), String> {
        if self.samples.len() < 2 || self.samples.len() > Self::MAX_SAMPLES {
            return Err(format!(
                "route elevation profile must contain 2..={} samples",
                Self::MAX_SAMPLES
            ));
        }
        for sample in &self.samples {
            EdgeProgressPermille::new(sample.progress.get())?;
            if ElevationMeters::new(sample.elevation.get()).is_none() {
                return Err("route elevation profile contains an out-of-range elevation".into());
            }
        }
        if self.samples.first().map(|v| v.progress.get()) != Some(0)
            || self.samples.last().map(|v| v.progress.get()) != Some(1_000)
            || self
                .samples
                .windows(2)
                .any(|p| p[0].progress.get() >= p[1].progress.get())
        {
            return Err(
                "route elevation profile must have sorted unique progress and 0/1000 endpoints"
                    .into(),
            );
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for RouteElevationProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            samples: BoundedVec<RouteElevationSample, 1_001>,
        }
        Self::new(Wire::deserialize(deserializer)?.samples.0).map_err(serde::de::Error::custom)
    }
}

macro_rules! bounded_route_metric {
    ($name:ident, $field:ident, $inner:ty, $min:expr, $max:expr) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
        #[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
        pub struct $name {
            $field: $inner,
        }
        impl $name {
            pub const MIN: $inner = $min;
            pub const MAX: $inner = $max;
            pub fn new(value: $inner) -> Result<Self, String> {
                if (Self::MIN..=Self::MAX).contains(&value) {
                    Ok(Self { $field: value })
                } else {
                    Err(format!(
                        "{} {} is outside {}..={}",
                        stringify!($name),
                        value,
                        Self::MIN,
                        Self::MAX
                    ))
                }
            }
            pub const fn get(self) -> $inner {
                self.$field
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                struct Wire {
                    $field: $inner,
                }
                Self::new(Wire::deserialize(deserializer)?.$field).map_err(serde::de::Error::custom)
            }
        }
    };
}

bounded_route_metric!(RouteVerticalMeters, meters, u32, 0, 100_000);
bounded_route_metric!(RouteSignedGradePermille, permille, i16, -10_000, 10_000);
bounded_route_metric!(RouteSlopePermille, permille, u16, 0, 10_000);
bounded_route_metric!(RouteRoughnessMeters, meters, u16, 0, 9_500);
bounded_route_metric!(RouteReliefMeters, meters, u16, 0, 9_500);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum DominantAspect {
    Flat,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum RouteLandformKind {
    Ridge,
    Valley,
    LikelyPass,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct LocatedRouteLandform {
    pub progress: EdgeProgressPermille,
    pub kind: RouteLandformKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum RouteTerrainClass {
    Flat,
    Rolling,
    Hilly,
    Mountainous,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum RouteWaterFeatureKind {
    River,
    Canal,
    Ditch,
    Inland,
    Tidal,
    Coastal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RouteWaterAdjacency {
    pub feature: RouteWaterFeatureKind,
    pub distance: WaterDistanceMeters,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum RouteRiskSeverity {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum RouteSeasonalHazard {
    SpringFlood,
    AutumnMud,
    WinterIce,
    WinterSnow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RouteSeasonalRisk {
    pub hazard: RouteSeasonalHazard,
    pub severity: RouteRiskSeverity,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum RouteEncounterTag {
    Flat,
    Rolling,
    Hilly,
    Mountainous,
    Steep,
    Rough,
    Ridge,
    Valley,
    LikelyPass,
    Bridge,
    Ford,
    Ferry,
    Riverbank,
    CanalBank,
    Lakeshore,
    TidalShore,
    Coast,
    SpringFlood,
    AutumnMud,
    WinterIce,
    WinterSnow,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct RouteTerrain {
    pub elevation_profile: RouteElevationProfile,
    pub ascent: RouteVerticalMeters,
    pub descent: RouteVerticalMeters,
    pub max_uphill_grade: RouteSignedGradePermille,
    pub max_downhill_grade: RouteSignedGradePermille,
    pub mean_slope: RouteSlopePermille,
    pub max_slope: RouteSlopePermille,
    pub dominant_aspect: DominantAspect,
    pub roughness: RouteRoughnessMeters,
    pub relief: RouteReliefMeters,
    pub landforms: Vec<LocatedRouteLandform>,
    pub class: RouteTerrainClass,
    pub water_adjacencies: Vec<RouteWaterAdjacency>,
    pub seasonal_risks: Vec<RouteSeasonalRisk>,
    pub encounter_tags: Vec<RouteEncounterTag>,
}

impl RouteTerrain {
    pub const MAX_LANDFORMS: usize = RouteElevationProfile::MAX_SAMPLES;
    pub const MAX_WATER_ADJACENCIES: usize = 6;
    pub const MAX_SEASONAL_RISKS: usize = 4;
    pub const MAX_ENCOUNTER_TAGS: usize = 21;
    /// Valid placeholder used only between the hydrology and route-terrain
    /// typestate stages; it is overwritten before canonical validation.
    pub fn stage_placeholder() -> Self {
        Self {
            elevation_profile: RouteElevationProfile::new(vec![
                RouteElevationSample {
                    progress: EdgeProgressPermille::new(0).unwrap(),
                    elevation: ElevationMeters::new(0).unwrap(),
                },
                RouteElevationSample {
                    progress: EdgeProgressPermille::new(1_000).unwrap(),
                    elevation: ElevationMeters::new(0).unwrap(),
                },
            ])
            .unwrap(),
            ascent: RouteVerticalMeters::new(0).unwrap(),
            descent: RouteVerticalMeters::new(0).unwrap(),
            max_uphill_grade: RouteSignedGradePermille::new(0).unwrap(),
            max_downhill_grade: RouteSignedGradePermille::new(0).unwrap(),
            mean_slope: RouteSlopePermille::new(0).unwrap(),
            max_slope: RouteSlopePermille::new(0).unwrap(),
            dominant_aspect: DominantAspect::Flat,
            roughness: RouteRoughnessMeters::new(0).unwrap(),
            relief: RouteReliefMeters::new(0).unwrap(),
            landforms: vec![],
            class: RouteTerrainClass::Flat,
            water_adjacencies: vec![],
            seasonal_risks: vec![],
            encounter_tags: vec![RouteEncounterTag::Flat],
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.elevation_profile.validate_raw()?;
        RouteVerticalMeters::new(self.ascent.get())?;
        RouteVerticalMeters::new(self.descent.get())?;
        RouteSignedGradePermille::new(self.max_uphill_grade.get())?;
        RouteSignedGradePermille::new(self.max_downhill_grade.get())?;
        RouteSlopePermille::new(self.mean_slope.get())?;
        RouteSlopePermille::new(self.max_slope.get())?;
        RouteRoughnessMeters::new(self.roughness.get())?;
        RouteReliefMeters::new(self.relief.get())?;
        for value in &self.landforms {
            EdgeProgressPermille::new(value.progress.get())?;
        }
        for value in &self.water_adjacencies {
            WaterDistanceMeters::new(value.distance.get())?;
        }
        if self.max_uphill_grade.get() < 0 || self.max_downhill_grade.get() > 0 {
            return Err("route grade extrema have contradictory signs".into());
        }
        if self.mean_slope > self.max_slope {
            return Err("route mean slope exceeds maximum slope".into());
        }
        if (self.mean_slope.get() < 10) != (self.dominant_aspect == DominantAspect::Flat) {
            return Err("route dominant aspect contradicts the mean-slope flat threshold".into());
        }
        if self.class != Self::class_for(self.max_slope.get(), self.relief.get()) {
            return Err("route terrain class contradicts slope/relief thresholds".into());
        }
        if self.landforms.len() > Self::MAX_LANDFORMS
            || self.water_adjacencies.len() > Self::MAX_WATER_ADJACENCIES
            || self.seasonal_risks.len() > Self::MAX_SEASONAL_RISKS
            || self.encounter_tags.len() > Self::MAX_ENCOUNTER_TAGS
        {
            return Err("route terrain collection exceeds its closed bound".into());
        }
        if self
            .landforms
            .windows(2)
            .any(|p| p[0].progress >= p[1].progress)
            || self
                .water_adjacencies
                .windows(2)
                .any(|p| p[0].feature >= p[1].feature)
            || self
                .seasonal_risks
                .windows(2)
                .any(|p| p[0].hazard >= p[1].hazard)
            || self.encounter_tags.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(
                "route terrain collections are not canonically ordered by logical key".into(),
            );
        }
        Ok(())
    }

    pub const fn class_for(max_slope: u16, relief: u16) -> RouteTerrainClass {
        if max_slope < 30 && relief < 30 {
            RouteTerrainClass::Flat
        } else if max_slope < 80 && relief < 100 {
            RouteTerrainClass::Rolling
        } else if max_slope < 150 && relief < 300 {
            RouteTerrainClass::Hilly
        } else {
            RouteTerrainClass::Mountainous
        }
    }

    pub fn validate_context(&self, route: &TravelRoute, length_m: u32) -> Result<(), String> {
        self.validate()?;
        if length_m == 0 {
            return Err("route terrain cannot describe a zero-length edge".into());
        }
        let mut ascent = 0u32;
        let mut descent = 0u32;
        let mut uphill = 0i16;
        let mut downhill = 0i16;
        for pair in self.elevation_profile.samples().windows(2) {
            let dz = i32::from(pair[1].elevation.get()) - i32::from(pair[0].elevation.get());
            if dz >= 0 {
                ascent = ascent
                    .checked_add(dz as u32)
                    .ok_or("route ascent overflow")?;
            } else {
                descent = descent
                    .checked_add((-dz) as u32)
                    .ok_or("route descent overflow")?;
            }
            let dp = u32::from(pair[1].progress.get() - pair[0].progress.get());
            let grade = route_grade_permille(dz, length_m, dp)?;
            uphill = uphill.max(grade);
            downhill = downhill.min(grade);
        }
        if self.ascent.get() != ascent.min(RouteVerticalMeters::MAX)
            || self.descent.get() != descent.min(RouteVerticalMeters::MAX)
            || self.max_uphill_grade.get() != uphill
            || self.max_downhill_grade.get() != downhill
        {
            return Err("route profile contradicts ascent/descent or grade extrema".into());
        }
        let min = self
            .elevation_profile
            .samples()
            .iter()
            .map(|v| v.elevation.get())
            .min()
            .unwrap();
        let max = self
            .elevation_profile
            .samples()
            .iter()
            .map(|v| v.elevation.get())
            .max()
            .unwrap();
        if self.relief.get() != (i32::from(max) - i32::from(min)).clamp(0, 9_500) as u16 {
            return Err("route relief contradicts its elevation profile".into());
        }
        let risks = expected_route_seasonal_risks(
            route,
            self.class,
            &self.water_adjacencies,
            &self.landforms,
            max,
        );
        if self.seasonal_risks != risks {
            return Err("route seasonal risks contradict route context".into());
        }
        let tags = expected_route_encounter_tags(
            route,
            self.class,
            self.max_slope.get(),
            self.roughness.get(),
            &self.landforms,
            &self.water_adjacencies,
            &self.seasonal_risks,
        );
        if self.encounter_tags != tags {
            return Err("route encounter tags contradict route context".into());
        }
        Ok(())
    }
}

pub fn route_grade_permille(dz: i32, length_m: u32, progress_delta: u32) -> Result<i16, String> {
    if length_m == 0 || progress_delta == 0 {
        return Err("route grade requires positive length and progress delta".into());
    }
    let numerator = i64::from(dz) * 1_000_000;
    let denominator = i64::from(length_m)
        .checked_mul(i64::from(progress_delta))
        .ok_or("route grade denominator overflow")?;
    let magnitude = (numerator.unsigned_abs() + denominator as u64 / 2) / denominator as u64;
    let signed = if numerator < 0 {
        -(magnitude as i64)
    } else {
        magnitude as i64
    };
    Ok(signed.clamp(-10_000, 10_000) as i16)
}

pub fn expected_route_seasonal_risks(
    route: &TravelRoute,
    class: RouteTerrainClass,
    water: &[RouteWaterAdjacency],
    landforms: &[LocatedRouteLandform],
    max_elevation: i16,
) -> Vec<RouteSeasonalRisk> {
    let ford = matches!(route, TravelRoute::Land(v) if v.water_crossings.iter().any(|c| c.traversal == CrossingTraversal::Ford));
    let ferry = matches!(route, TravelRoute::Ferry(_));
    let valley = landforms
        .iter()
        .any(|v| v.kind == RouteLandformKind::Valley);
    let within = |distance, kinds: &[RouteWaterFeatureKind]| {
        water
            .iter()
            .any(|v| kinds.contains(&v.feature) && v.distance.get() <= distance)
    };
    let mut values = std::collections::BTreeSet::new();
    if ford {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::SpringFlood,
            severity: RouteRiskSeverity::High,
        });
    } else if valley
        && within(
            500,
            &[
                RouteWaterFeatureKind::River,
                RouteWaterFeatureKind::Canal,
                RouteWaterFeatureKind::Ditch,
                RouteWaterFeatureKind::Inland,
                RouteWaterFeatureKind::Tidal,
            ],
        )
    {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::SpringFlood,
            severity: RouteRiskSeverity::Medium,
        });
    }
    if ford {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::AutumnMud,
            severity: RouteRiskSeverity::Medium,
        });
    } else if matches!(class, RouteTerrainClass::Flat | RouteTerrainClass::Rolling)
        && within(
            250,
            &[
                RouteWaterFeatureKind::River,
                RouteWaterFeatureKind::Canal,
                RouteWaterFeatureKind::Ditch,
                RouteWaterFeatureKind::Inland,
            ],
        )
    {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::AutumnMud,
            severity: RouteRiskSeverity::Low,
        });
    }
    if ferry {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterIce,
            severity: RouteRiskSeverity::High,
        });
    } else if ford {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterIce,
            severity: RouteRiskSeverity::Medium,
        });
    } else if within(
        250,
        &[RouteWaterFeatureKind::Inland, RouteWaterFeatureKind::Tidal],
    ) {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterIce,
            severity: RouteRiskSeverity::Low,
        });
    }
    if class == RouteTerrainClass::Mountainous || max_elevation >= 1_000 {
        values.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterSnow,
            severity: RouteRiskSeverity::Medium,
        });
    }
    values.into_iter().collect()
}

pub fn expected_route_encounter_tags(
    route: &TravelRoute,
    class: RouteTerrainClass,
    max_slope: u16,
    roughness: u16,
    landforms: &[LocatedRouteLandform],
    water: &[RouteWaterAdjacency],
    risks: &[RouteSeasonalRisk],
) -> Vec<RouteEncounterTag> {
    let mut values = std::collections::BTreeSet::new();
    values.insert(match class {
        RouteTerrainClass::Flat => RouteEncounterTag::Flat,
        RouteTerrainClass::Rolling => RouteEncounterTag::Rolling,
        RouteTerrainClass::Hilly => RouteEncounterTag::Hilly,
        RouteTerrainClass::Mountainous => RouteEncounterTag::Mountainous,
    });
    if max_slope >= 150 {
        values.insert(RouteEncounterTag::Steep);
    }
    if roughness >= 20 {
        values.insert(RouteEncounterTag::Rough);
    }
    for v in landforms {
        values.insert(match v.kind {
            RouteLandformKind::Ridge => RouteEncounterTag::Ridge,
            RouteLandformKind::Valley => RouteEncounterTag::Valley,
            RouteLandformKind::LikelyPass => RouteEncounterTag::LikelyPass,
        });
    }
    match route {
        TravelRoute::Ferry(_) => {
            values.insert(RouteEncounterTag::Ferry);
        }
        TravelRoute::Land(v) => {
            for c in &v.water_crossings {
                values.insert(if c.traversal == CrossingTraversal::Bridge {
                    RouteEncounterTag::Bridge
                } else {
                    RouteEncounterTag::Ford
                });
            }
        }
    }
    for v in water {
        values.insert(match v.feature {
            RouteWaterFeatureKind::River | RouteWaterFeatureKind::Ditch => {
                RouteEncounterTag::Riverbank
            }
            RouteWaterFeatureKind::Canal => RouteEncounterTag::CanalBank,
            RouteWaterFeatureKind::Inland => RouteEncounterTag::Lakeshore,
            RouteWaterFeatureKind::Tidal => RouteEncounterTag::TidalShore,
            RouteWaterFeatureKind::Coastal => RouteEncounterTag::Coast,
        });
    }
    for v in risks {
        values.insert(match v.hazard {
            RouteSeasonalHazard::SpringFlood => RouteEncounterTag::SpringFlood,
            RouteSeasonalHazard::AutumnMud => RouteEncounterTag::AutumnMud,
            RouteSeasonalHazard::WinterIce => RouteEncounterTag::WinterIce,
            RouteSeasonalHazard::WinterSnow => RouteEncounterTag::WinterSnow,
        });
    }
    values.into_iter().collect()
}

impl<'de> Deserialize<'de> for RouteTerrain {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            elevation_profile: RouteElevationProfile,
            ascent: RouteVerticalMeters,
            descent: RouteVerticalMeters,
            max_uphill_grade: RouteSignedGradePermille,
            max_downhill_grade: RouteSignedGradePermille,
            mean_slope: RouteSlopePermille,
            max_slope: RouteSlopePermille,
            dominant_aspect: DominantAspect,
            roughness: RouteRoughnessMeters,
            relief: RouteReliefMeters,
            landforms: BoundedVec<LocatedRouteLandform, 1_001>,
            class: RouteTerrainClass,
            water_adjacencies: BoundedVec<RouteWaterAdjacency, 6>,
            seasonal_risks: BoundedVec<RouteSeasonalRisk, 4>,
            encounter_tags: BoundedVec<RouteEncounterTag, 21>,
        }
        let w = Wire::deserialize(deserializer)?;
        let value = Self {
            elevation_profile: w.elevation_profile,
            ascent: w.ascent,
            descent: w.descent,
            max_uphill_grade: w.max_uphill_grade,
            max_downhill_grade: w.max_downhill_grade,
            mean_slope: w.mean_slope,
            max_slope: w.max_slope,
            dominant_aspect: w.dominant_aspect,
            roughness: w.roughness,
            relief: w.relief,
            landforms: w.landforms.0,
            class: w.class,
            water_adjacencies: w.water_adjacencies.0,
            seasonal_risks: w.seasonal_risks.0,
            encounter_tags: w.encounter_tags.0,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
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

/// The only canonical projected coordinate system used by world-data stages.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SpatialGridCrs {
    /// EPSG:3035, ETRS89 / LAEA Europe.
    Etrs89LaeaEurope,
}

/// Validated square grid cell size in integer metres.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct GridCellSizeMeters(u32);

impl GridCellSizeMeters {
    pub const MIN: u32 = 250;
    pub const MAX: u32 = 100_000;
    pub const DEFAULT: u32 = 1_000;

    pub fn new(meters: u32) -> Result<Self, SpatialGridSpecError> {
        if !(Self::MIN..=Self::MAX).contains(&meters) || !meters.is_multiple_of(250) {
            return Err(SpatialGridSpecError::InvalidCellSize(meters));
        }
        Ok(Self(meters))
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for GridCellSizeMeters {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl FromStr for GridCellSizeMeters {
    type Err = SpatialGridSpecError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let meters = value
            .parse::<u32>()
            .map_err(|_| SpatialGridSpecError::InvalidCellSizeText(value.into()))?;
        Self::new(meters)
    }
}

impl fmt::Display for GridCellSizeMeters {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for GridCellSizeMeters {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpatialGridSpecError {
    InvalidCellSize(u32),
    InvalidCellSizeText(String),
    NonCanonicalOrigin { easting_m: i64, northing_m: i64 },
    RectangularCells { width_m: u32, height_m: u32 },
}

impl fmt::Display for SpatialGridSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCellSize(value) => write!(
                formatter,
                "grid cell size {value} must be 250..=100000 metres and divisible by 250"
            ),
            Self::InvalidCellSizeText(value) => write!(
                formatter,
                "grid cell size {value:?} is not an unsigned integer number of metres"
            ),
            Self::NonCanonicalOrigin {
                easting_m,
                northing_m,
            } => write!(
                formatter,
                "grid origin ({easting_m}, {northing_m}) must be the canonical (0, 0) metres"
            ),
            Self::RectangularCells { width_m, height_m } => write!(
                formatter,
                "grid cells must be square, not {width_m} by {height_m} metres"
            ),
        }
    }
}

impl std::error::Error for SpatialGridSpecError {}

/// Complete identity of the canonical, origin-aligned square spatial grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SpatialGridSpec {
    crs: SpatialGridCrs,
    origin_easting_m: i64,
    origin_northing_m: i64,
    cell_width_m: GridCellSizeMeters,
    cell_height_m: GridCellSizeMeters,
}

impl SpatialGridSpec {
    pub fn new(cell_size: GridCellSizeMeters) -> Self {
        Self {
            crs: SpatialGridCrs::Etrs89LaeaEurope,
            origin_easting_m: 0,
            origin_northing_m: 0,
            cell_width_m: cell_size,
            cell_height_m: cell_size,
        }
    }

    pub const fn crs(self) -> SpatialGridCrs {
        self.crs
    }
    pub const fn origin_easting_m(self) -> i64 {
        self.origin_easting_m
    }
    pub const fn origin_northing_m(self) -> i64 {
        self.origin_northing_m
    }
    pub const fn cell_size_meters(self) -> GridCellSizeMeters {
        self.cell_width_m
    }
}

impl Default for SpatialGridSpec {
    fn default() -> Self {
        Self::new(GridCellSizeMeters::default())
    }
}

impl<'de> Deserialize<'de> for SpatialGridSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            crs: SpatialGridCrs,
            origin_easting_m: i64,
            origin_northing_m: i64,
            cell_width_m: GridCellSizeMeters,
            cell_height_m: GridCellSizeMeters,
        }
        let wire = Wire::deserialize(deserializer)?;
        if wire.crs != SpatialGridCrs::Etrs89LaeaEurope {
            return Err(serde::de::Error::custom("unsupported spatial grid CRS"));
        }
        if (wire.origin_easting_m, wire.origin_northing_m) != (0, 0) {
            return Err(serde::de::Error::custom(
                SpatialGridSpecError::NonCanonicalOrigin {
                    easting_m: wire.origin_easting_m,
                    northing_m: wire.origin_northing_m,
                },
            ));
        }
        if wire.cell_width_m != wire.cell_height_m {
            return Err(serde::de::Error::custom(
                SpatialGridSpecError::RectangularCells {
                    width_m: wire.cell_width_m.get(),
                    height_m: wire.cell_height_m.get(),
                },
            ));
        }
        Ok(Self::new(wire.cell_width_m))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorldMetadata {
    pub schema_version: u32,
    pub inference_rules_version: u32,
    pub spatial_grid: SpatialGridSpec,
    pub world_year: i32,
    /// BLAKE3 of the canonical schema/rules/year/grid/source-manifest tuple.
    pub manifest_digest: String,
    pub sources: Vec<SourceProvenance>,
    pub road_types: Vec<TravelEdgeKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProvenance {
    /// Stable machine identifier. This is also the canonical sort key.
    pub id: String,
    pub name: String,
    pub release: SourceRelease,
    pub canonical_url: String,
    pub doi: Option<String>,
    pub license: SourceLicense,
    /// Exact operational attribution, change, endorsement, liability, and
    /// redistribution notices which must accompany this distribution.
    pub required_notices: Vec<String>,
    pub access: SourceAccess,
    pub spatial: SourceSpatialCoverage,
    pub temporal: SourceTemporalCoverage,
    pub preparation: SourcePreparation,
    pub content_identity: SourceContentIdentity,
    /// Human-facing Markdown retained alongside the typed manifest.
    pub notes_markdown: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SourceRelease {
    Immutable { version: String, released: String },
    Curated { revision: String },
    Rolling { observed_at: String },
    ReleaseBlocked { reason: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceLicense {
    CcBy3_0,
    CcBy4_0,
    CcBySa4_0,
    Cc0_1_0,
    CopernicusDem,
    CopernicusClms,
    NoaaPublicAccess,
    RightsReserved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SourceAccess {
    AnonymousDownload,
    AuthenticatedDownload,
    CuratedRepositoryAsset,
    ManualPreparation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SourceSpatialCoverage {
    NotApplicable,
    Geographic {
        crs: String,
        resolution: String,
        coverage: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SourceTemporalCoverage {
    Timeless,
    Year(i32),
    Years { first: i32, last: i32 },
    ModernProxy { year: i32 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePreparation {
    pub recipe: String,
    pub version: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SourceContentIdentity {
    RawSha256 { sha256: String },
    PreparedSnapshotSha256 { sha256: String },
    CuratedRevision { revision: String, sha256: String },
    UnpinnedRollingObservation { observed_at: String },
    ReleaseBlocked { reason: String },
}

impl SourceContentIdentity {
    pub const fn is_reproducible(&self) -> bool {
        matches!(
            self,
            Self::RawSha256 { .. }
                | Self::PreparedSnapshotSha256 { .. }
                | Self::CuratedRevision { .. }
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WorldBuildReport {
    pub nodes: usize,
    pub edges: usize,
    pub settlements: usize,
    pub settlement_aliases: usize,
    pub settlement_descriptions: usize,
    pub deferred_settlement_descriptions: std::collections::BTreeMap<String, usize>,
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
    pub potential_vegetation_raster_files_read: usize,
    pub potential_vegetation_samples: usize,
    pub potential_vegetation_posterior_samples: usize,
    pub potential_vegetation_categorical_samples: usize,
    pub potential_vegetation_inferred_samples: usize,
    pub tree_species_rasters_read: usize,
    pub tree_species_samples: usize,
    pub tree_species_fallback_samples: usize,
    pub tree_species_candidates: usize,
    pub soil_rasters_read: usize,
    pub soil_depth_layers_read: usize,
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
    pub historical_vegetation_direct_samples: usize,
    pub historical_vegetation_derived_samples: usize,
    pub historical_vegetation_fallback_samples: usize,
    pub historical_vegetation_tie_breaks: usize,
    pub route_terrain_edges: usize,
    pub route_terrain_dem_samples: usize,
    pub route_terrain_dem_fallbacks: usize,
    pub route_terrain_water_adjacencies: usize,
    pub route_terrain_landforms: usize,
    pub route_terrain_seasonal_risks: usize,
    pub route_terrain_encounter_tags: usize,
    pub industry_settlements: usize,
    pub industry_derived_outputs: usize,
    pub industry_fallback_settlements: usize,
    pub industry_fallback_outputs: usize,
    pub industry_agriculture_outputs: usize,
    pub industry_fishing_outputs: usize,
    pub industry_quarrying_outputs: usize,
    pub industry_mining_outputs: usize,
    pub industry_pottery_outputs: usize,
    pub industry_peat_outputs: usize,
    pub industry_forestry_outputs: usize,
    pub industry_charcoal_outputs: usize,
    pub industry_saltmaking_outputs: usize,
    pub industry_construction_outputs: usize,
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
    /// Viabundus's source cost hint remains distinct from DEM-derived grade.
    pub terrain: RouteTerrain,
    pub certainty: u8,
    pub section: String,
    pub sources: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct SettlementImport {
    pub id: String,
    pub source_node_id: u64,
    #[serde(deserialize_with = "deserialize_settlement_name")]
    pub name: String,
    pub longitude: f64,
    pub latitude: f64,
    pub population_level: i32,
    pub population_estimate: u32,
    pub elevation: ElevationMeters,
    pub land_use: LandUseProfile,
    pub forest_cover: ForestCover,
    pub potential_vegetation: PotentialVegetation,
    pub historical_vegetation: HistoricalVegetation,
    pub tree_species: TreeSpeciesProfile,
    pub soil: SoilProfile,
    pub geology: SurfaceGeology,
    pub religious_status: SettlementReligiousStatus,
    pub drought: DroughtProfile,
    pub hydrology: SettlementHydrology,
    pub industries: InferredIndustryProfile,
    pub scene_key: String,
    pub sources: String,
}

pub const MAX_SETTLEMENT_NAME_CHARS: usize = 160;

pub fn valid_settlement_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_SETTLEMENT_NAME_CHARS
        && !value.chars().any(char::is_control)
}

fn deserialize_settlement_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if valid_settlement_name(&value) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "settlement name must contain 1 to 160 non-control characters",
        ))
    }
}

#[cfg(test)]
mod settlement_name_tests {
    use super::*;

    #[test]
    fn settlement_names_are_bounded_and_reject_controls() {
        assert!(valid_settlement_name("Lübeck"));
        assert!(!valid_settlement_name(""));
        assert!(!valid_settlement_name("bad\nname"));
        assert!(!valid_settlement_name(
            &"x".repeat(MAX_SETTLEMENT_NAME_CHARS + 1)
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        CanopyDensity, DroughtHistory, ElevationBand, ElevationMeters, ForestCover, GeologicUnitId,
        HabitatSuitability, HumanLandUseIntensity, InferredTreeSpeciesProfile, LandUseFraction,
        LandUseProfile, LanguageCode, ModeledTreeSpecies, ModeledTreeSpeciesProfile,
        NativeRangeEvidence, OfficialReligion, PalmerDroughtSeverityIndex, PotentialVegetation,
        PotentialVegetationClass, PotentialVegetationPosterior, ReligionHours, ReligionMinutes,
        SettlementReligiousStatus, SoilBasisPoints, StoneContentPercent, SuitabilityBasisPoints,
        SummerHydroclimate, TreeSpeciesId,
    };

    #[test]
    fn religion_correlations_are_identity_symmetric_and_non_recursive() {
        for left in OfficialReligion::ALL {
            assert_eq!(left.correlation(left), 1.0);
            for right in OfficialReligion::ALL {
                assert_eq!(left.correlation(right), right.correlation(left));
            }
        }
        assert_eq!(
            OfficialReligion::RomanCatholic.correlation(OfficialReligion::Lutheran),
            0.80
        );
        assert_eq!(
            OfficialReligion::Islamic.correlation(OfficialReligion::Judaism),
            0.35
        );
        let hours = ReligionHours {
            roman_catholic: 1.0,
            ..Default::default()
        };
        assert_eq!(hours.effective(OfficialReligion::Lutheran), 0.8);
        // The derived Lutheran knowledge does not feed back into Catholicism.
        assert_eq!(hours.effective(OfficialReligion::RomanCatholic), 1.0);
        assert_eq!(hours.total_direct(), 1.0);
    }

    #[test]
    fn religion_minutes_split_evenly_with_stable_remainders() {
        let split = ReligionMinutes::split_evenly(
            61,
            &[OfficialReligion::Reformed, OfficialReligion::RomanCatholic],
        );
        assert_eq!(split.roman_catholic, 31);
        assert_eq!(split.reformed, 30);
        assert_eq!(split.total(), 61);
        assert_eq!(ReligionMinutes::split_evenly(60, &[]).total(), 0);
    }

    #[test]
    fn religion_direct_field_validation_rejects_each_invalid_shape() {
        let valid = ReligionHours {
            judaism: 12.0,
            ..Default::default()
        };
        assert!(valid.direct_fields_valid(100.0));
        for invalid in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 100.1] {
            let hours = ReligionHours {
                judaism: invalid,
                ..valid
            };
            assert!(!hours.direct_fields_valid(100.0), "accepted {invalid:?}");
        }
    }

    #[test]
    fn adding_direct_hours_repairs_poisoned_canonical_field() {
        let mut hours = ReligionHours {
            islamic: f32::NAN,
            ..Default::default()
        };
        hours.add_direct(OfficialReligion::Islamic, 0.5);
        assert_eq!(hours.islamic, 0.5);
    }

    #[test]
    fn industry_profiles_are_nonempty_bounded_unique_and_canonical() {
        use super::{FallbackIndustry as F, IndustryEvidence as E, InferredIndustryProfile as P};
        assert!(P::new(vec![]).is_none());
        assert!(P::new(vec![E::Fallback(F::CommonAggregate); 25]).is_none());
        assert!(
            P::new(vec![
                E::Fallback(F::CommonAggregate),
                E::Fallback(F::CommonAggregate)
            ])
            .is_none()
        );
        let p = P::new(vec![
            E::Fallback(F::WoodlandFuelwood),
            E::Fallback(F::CommonAggregate),
        ])
        .unwrap();
        assert_eq!(
            p.outputs(),
            &[
                E::Fallback(F::WoodlandFuelwood),
                E::Fallback(F::CommonAggregate)
            ]
        );
    }

    #[test]
    fn industry_profile_serde_rejects_constructor_bypasses() {
        use super::{FallbackIndustry as F, IndustryEvidence as E, InferredIndustryProfile as P};
        let value = serde_json::json!({"outputs": [E::Fallback(F::CommonAggregate)]});
        assert!(serde_json::from_value::<P>(value).is_ok());
        assert!(serde_json::from_value::<P>(serde_json::json!({"outputs": []})).is_err());
        let duplicate = serde_json::json!({"outputs": [E::Fallback(F::CommonAggregate), E::Fallback(F::CommonAggregate)]});
        assert!(serde_json::from_value::<P>(duplicate).is_err());
        let reversed = serde_json::json!({"outputs": [E::Fallback(F::CommonAggregate), E::Fallback(F::WoodlandFuelwood)]});
        assert!(serde_json::from_value::<P>(reversed).is_err());
        let mut raw = P { outputs: vec![] };
        assert!(raw.validate().is_err());
        raw.outputs = vec![E::Fallback(F::CommonAggregate); 25];
        assert!(raw.validate().is_err());
    }

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
    fn external_settlement_text_is_bounded_and_nul_free() {
        let limit = super::SETTLEMENT_ALIAS_NAME_MAX_BYTES;
        assert!(super::valid_bounded_source_text(&"a".repeat(limit), limit));
        assert!(!super::valid_bounded_source_text(
            &"a".repeat(limit + 1),
            limit
        ));
        assert!(!super::valid_bounded_source_text("name\0hidden", limit));
        assert!(!super::valid_bounded_source_text(" padded ", limit));
    }

    #[test]
    fn religious_status_derives_the_single_church_religion() {
        let status = SettlementReligiousStatus::MultiConfessional {
            arrangement: super::WesternChristianArrangement::CatholicLutheran {
                church: super::CatholicLutheranChurch::Lutheran,
            },
        };
        assert_eq!(status.church(), OfficialReligion::Lutheran);
        assert_eq!(status.church().religion_id(), "lutheran");
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
    fn historical_vegetation_wire_rejects_invalid_dependent_fields() {
        let invalid = serde_json::json!({
            "Derived": {
                "cover": { "Woodland": { "canopy": { "percent": 0 }, "dominant": "Mixed" } },
                "method": "MultiSourceRulesV4"
            }
        });
        assert!(serde_json::from_value::<super::HistoricalVegetation>(invalid).is_err());
        let invalid_fraction = serde_json::json!({
            "Direct": {
                "cover": { "Cropland": { "cultivated_fraction": { "basis_points": 10001 } } },
                "method": "Hyde35DominantLandUse"
            }
        });
        assert!(serde_json::from_value::<super::HistoricalVegetation>(invalid_fraction).is_err());
        for invalid in [
            serde_json::json!({ "Fallback": { "cover": { "BuiltSettlement": { "built_fraction": { "basis_points": 1000 } } }, "method": "PotentialEnvelopeV4" } }),
            serde_json::json!({ "Derived": { "cover": { "Cropland": { "cultivated_fraction": { "basis_points": 4000 } } }, "method": "MultiSourceRulesV4" } }),
            serde_json::json!({ "Direct": { "cover": { "Wetland": { "water_regime": "LongSeasonWet" } }, "method": "Hyde35DominantLandUse" } }),
        ] {
            assert!(serde_json::from_value::<super::HistoricalVegetation>(invalid).is_err());
        }
    }

    #[test]
    fn historical_context_enforces_direct_fractions_and_wetland_convergence() {
        use super::*;
        let profile = |crop, grazing, built| {
            LandUseProfile::new(
                LandUseFraction::new(crop).unwrap(),
                LandUseFraction::new(grazing).unwrap(),
                LandUseFraction::new(built).unwrap(),
                LandUseFraction::new(10_000 - crop - grazing - built).unwrap(),
            )
            .unwrap()
        };
        let soil = |water_regime| SoilProfile {
            wrb_group: WrbReferenceGroup::Cambisol,
            parent_material: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
            properties: SoilProperties {
                substrate: SoilSubstrate::Mineral(MineralSoil {
                    texture: MineralSoilTexture::Medium,
                    depth: SoilDepth::Deep,
                    available_water: AvailableWaterCapacity::Medium,
                    organic_carbon: TopsoilOrganicCarbon::Medium,
                    stones: StoneContentPercent::new(0).unwrap(),
                }),
                water_regime,
                agricultural_limitation: AgriculturalLimitation::None,
            },
            acidity: SoilAcidity::Neutral,
            cation_exchange_capacity: CationExchangeCapacity::Medium,
            fertility: SoilFertility::Medium,
            confidence: SoilBasisPoints::new(5_000).unwrap(),
            evidence: SoilEvidence::SoilGridsPrediction,
        };
        let dry_soil = soil(SoilWaterRegime::SeasonallyWet);
        let pnv = PotentialVegetation::Categorical(PotentialVegetationClass::Grassland);
        let direct = |cover| {
            HistoricalVegetation::Direct(DirectHistoricalVegetation {
                cover,
                method: DirectHistoricalVegetationMethod::Hyde35DominantLandUse,
            })
        };
        assert!(!historical_vegetation_matches_context(
            direct(DirectHistoricalVegetationCover::BuiltSettlement(
                BuiltSettlementCover {
                    built_fraction: LandUseFraction::new(1).unwrap()
                }
            )),
            profile(0, 0, 1),
            &pnv,
            dry_soil,
            SettlementHydrology::default()
        ));
        assert!(!historical_vegetation_matches_context(
            direct(DirectHistoricalVegetationCover::BuiltSettlement(
                BuiltSettlementCover {
                    built_fraction: LandUseFraction::new(1_001).unwrap()
                }
            )),
            profile(0, 0, 1_000),
            &pnv,
            dry_soil,
            SettlementHydrology::default()
        ));
        for (historical, land) in [
            (
                direct(DirectHistoricalVegetationCover::BuiltSettlement(
                    BuiltSettlementCover {
                        built_fraction: LandUseFraction::new(1_000).unwrap(),
                    },
                )),
                profile(0, 0, 1_000),
            ),
            (
                direct(DirectHistoricalVegetationCover::Cropland(CroplandCover {
                    cultivated_fraction: LandUseFraction::new(4_000).unwrap(),
                })),
                profile(4_000, 0, 0),
            ),
            (
                direct(DirectHistoricalVegetationCover::Pasture(PastureCover {
                    grazing_fraction: LandUseFraction::new(4_000).unwrap(),
                })),
                profile(0, 4_000, 0),
            ),
        ] {
            assert!(historical_vegetation_matches_context(
                historical,
                land,
                &pnv,
                dry_soil,
                SettlementHydrology::default()
            ));
        }

        let wet = |regime| {
            HistoricalVegetation::Derived(DerivedHistoricalVegetation {
                cover: DerivedHistoricalVegetationCover::Wetland(HistoricalWetland {
                    water_regime: regime,
                }),
                method: DerivedHistoricalVegetationMethod::MultiSourceRulesV4,
            })
        };
        let wet_pnv = PotentialVegetation::Categorical(PotentialVegetationClass::Wetlands);
        let fresh = SettlementHydrology {
            inland: Some(InlandWaterAccess {
                distance: WaterDistanceMeters::new(10).unwrap(),
                size: InlandWaterSize::Pond,
            }),
            ..Default::default()
        };
        let natural = profile(0, 0, 0);
        assert!(!historical_vegetation_matches_context(
            wet(SoilWaterRegime::SeasonallyWet),
            natural,
            &wet_pnv,
            soil(SoilWaterRegime::SeasonallyWet),
            fresh
        ));
        assert!(!historical_vegetation_matches_context(
            wet(SoilWaterRegime::LongSeasonWet),
            natural,
            &wet_pnv,
            soil(SoilWaterRegime::LongSeasonWet),
            SettlementHydrology::default()
        ));
        assert!(!historical_vegetation_matches_context(
            wet(SoilWaterRegime::LongSeasonWet),
            natural,
            &pnv,
            soil(SoilWaterRegime::LongSeasonWet),
            fresh
        ));
        assert!(historical_vegetation_matches_context(
            wet(SoilWaterRegime::LongSeasonWet),
            natural,
            &wet_pnv,
            soil(SoilWaterRegime::LongSeasonWet),
            fresh
        ));
    }

    #[test]
    fn potential_vegetation_is_bounded_and_has_stable_ties() {
        assert!(SuitabilityBasisPoints::new(10_000).is_some());
        assert!(SuitabilityBasisPoints::new(10_001).is_none());
        assert!(
            serde_json::from_str::<SuitabilityBasisPoints>(r#"{"basis_points":10001}"#).is_err()
        );
        let q = SuitabilityBasisPoints::new(5_000).unwrap();
        let vegetation = PotentialVegetation::Posterior(PotentialVegetationPosterior {
            woodland_and_forest: q,
            heathland_and_shrub: q,
            grassland: q,
            sparsely_vegetated_areas: q,
            wetlands: q,
            marine_inlets_and_transitional_waters: q,
        });
        assert_eq!(
            vegetation.class(),
            PotentialVegetationClass::WoodlandAndForest
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
    fn soil_uncertainty_and_percentages_are_bounded() {
        assert_eq!(SoilBasisPoints::new(10_000).unwrap().get(), 10_000);
        assert!(SoilBasisPoints::new(10_001).is_none());
        assert!(StoneContentPercent::new(100).is_some());
        assert!(StoneContentPercent::new(101).is_none());
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

    #[test]
    fn language_codes_are_parsed_into_a_closed_representation() {
        assert_eq!(LanguageCode::from_str("deu").unwrap().as_str(), "deu");
        assert!(LanguageCode::from_str("DE").is_err());
        assert!(serde_json::from_str::<LanguageCode>("\"english\"").is_err());
    }

    #[test]
    fn spatial_grid_sizes_cover_only_the_canonical_range_and_increment() {
        use super::GridCellSizeMeters;
        assert_eq!(GridCellSizeMeters::default().get(), 1_000);
        assert_eq!(GridCellSizeMeters::new(250).unwrap().get(), 250);
        assert_eq!(GridCellSizeMeters::new(100_000).unwrap().get(), 100_000);
        for invalid in [0, 249, 251, 99_999, 100_001] {
            assert!(GridCellSizeMeters::new(invalid).is_err());
        }
        assert!(serde_json::from_str::<GridCellSizeMeters>("251").is_err());
    }

    #[test]
    fn spatial_grid_wire_values_cannot_bypass_invariants() {
        use super::SpatialGridSpec;
        let canonical = serde_json::to_value(SpatialGridSpec::default()).unwrap();
        assert_eq!(
            serde_json::from_value::<SpatialGridSpec>(canonical.clone()).unwrap(),
            SpatialGridSpec::default()
        );

        let mutate = |field: &str, value| {
            let mut wire = canonical.clone();
            wire[field] = value;
            serde_json::from_value::<SpatialGridSpec>(wire)
        };
        assert!(mutate("crs", serde_json::json!("Wgs84")).is_err());
        assert!(mutate("origin_easting_m", serde_json::json!(1)).is_err());
        assert!(mutate("origin_northing_m", serde_json::json!(-1)).is_err());
        assert!(mutate("cell_height_m", serde_json::json!(2_000)).is_err());
        assert!(mutate("cell_width_m", serde_json::json!(0)).is_err());
    }

    #[test]
    fn old_world_metadata_cannot_default_new_identity_fields() {
        let old = serde_json::json!({
            "schema_version": 13,
            "world_year": 1544,
            "sources": [],
            "road_types": []
        });
        assert!(serde_json::from_value::<super::WorldMetadata>(old).is_err());
    }

    #[test]
    fn route_terrain_wire_rejects_bounds_order_and_duplicates() {
        let terrain = super::RouteTerrain::stage_placeholder();
        let mut wire = serde_json::to_value(&terrain).unwrap();
        wire["elevation_profile"]["samples"] = serde_json::json!([]);
        assert!(serde_json::from_value::<super::RouteTerrain>(wire).is_err());

        let mut wire = serde_json::to_value(&terrain).unwrap();
        wire["encounter_tags"] = serde_json::json!(["Flat", "Flat"]);
        assert!(serde_json::from_value::<super::RouteTerrain>(wire).is_err());

        assert!(super::RouteSlopePermille::new(10_001).is_err());
        assert!(super::RouteSignedGradePermille::new(-10_001).is_err());
        assert!(super::RouteVerticalMeters::new(100_001).is_err());

        for (field, count, value) in [
            (
                "landforms",
                1_002,
                serde_json::json!({"progress":{"permille":1},"kind":"Ridge"}),
            ),
            (
                "water_adjacencies",
                7,
                serde_json::json!({"feature":"River","distance":{"meters":1}}),
            ),
            (
                "seasonal_risks",
                5,
                serde_json::json!({"hazard":"SpringFlood","severity":"Low"}),
            ),
            ("encounter_tags", 22, serde_json::json!("Flat")),
        ] {
            let mut wire = serde_json::to_value(&terrain).unwrap();
            wire[field] = serde_json::Value::Array(vec![value; count]);
            assert!(
                serde_json::from_value::<super::RouteTerrain>(wire).is_err(),
                "{field}"
            );
        }
        let mut wire = serde_json::to_value(&terrain).unwrap();
        wire["elevation_profile"]["samples"] = serde_json::Value::Array(vec![
            serde_json::json!({"progress":{"permille":0},"elevation":{"meters":0}});
            1_002
        ]);
        assert!(serde_json::from_value::<super::RouteTerrain>(wire).is_err());

        let mut duplicate_key = super::RouteTerrain::stage_placeholder();
        duplicate_key.water_adjacencies = vec![
            super::RouteWaterAdjacency {
                feature: super::RouteWaterFeatureKind::River,
                distance: super::WaterDistanceMeters::new(1).unwrap(),
            },
            super::RouteWaterAdjacency {
                feature: super::RouteWaterFeatureKind::River,
                distance: super::WaterDistanceMeters::new(2).unwrap(),
            },
        ];
        assert!(duplicate_key.validate().is_err());
        let mut duplicate_progress = super::RouteTerrain::stage_placeholder();
        duplicate_progress.landforms = vec![
            super::LocatedRouteLandform {
                progress: super::EdgeProgressPermille::new(10).unwrap(),
                kind: super::RouteLandformKind::Ridge,
            },
            super::LocatedRouteLandform {
                progress: super::EdgeProgressPermille::new(10).unwrap(),
                kind: super::RouteLandformKind::Valley,
            },
        ];
        assert!(duplicate_progress.validate().is_err());
    }

    #[test]
    fn route_terrain_context_rejects_semantic_contradictions() {
        let route = super::TravelRoute::Land(super::LandRoute {
            bridge: None,
            water_crossings: vec![],
        });
        let valid = super::RouteTerrain::stage_placeholder();
        valid.validate_context(&route, 1_000).unwrap();
        for mutate in [
            |v: &mut super::RouteTerrain| v.class = super::RouteTerrainClass::Mountainous,
            |v: &mut super::RouteTerrain| {
                v.max_uphill_grade = super::RouteSignedGradePermille::new(-1).unwrap()
            },
            |v: &mut super::RouteTerrain| v.mean_slope = super::RouteSlopePermille::new(1).unwrap(),
            |v: &mut super::RouteTerrain| v.ascent = super::RouteVerticalMeters::new(1).unwrap(),
        ] {
            let mut changed = valid.clone();
            mutate(&mut changed);
            assert!(changed.validate_context(&route, 1_000).is_err());
        }
        let mut bad_tags = valid.clone();
        bad_tags.encounter_tags = vec![super::RouteEncounterTag::Rolling];
        assert!(bad_tags.validate_context(&route, 1_000).is_err());
        let mut bad_risk = valid;
        bad_risk.seasonal_risks = vec![super::RouteSeasonalRisk {
            hazard: super::RouteSeasonalHazard::WinterSnow,
            severity: super::RouteRiskSeverity::High,
        }];
        assert!(bad_risk.validate_context(&route, 1_000).is_err());

        let mut flat_with_nonflat_aspect = super::RouteTerrain::stage_placeholder();
        flat_with_nonflat_aspect.dominant_aspect = super::DominantAspect::North;
        assert!(
            flat_with_nonflat_aspect
                .validate_context(&route, 1_000)
                .is_err()
        );
        let mut sloped_with_flat_aspect = super::RouteTerrain::stage_placeholder();
        sloped_with_flat_aspect.mean_slope = super::RouteSlopePermille::new(10).unwrap();
        sloped_with_flat_aspect.max_slope = super::RouteSlopePermille::new(10).unwrap();
        assert!(
            sloped_with_flat_aspect
                .validate_context(&route, 1_000)
                .is_err()
        );
    }

    #[test]
    fn raw_route_terrain_validation_is_panic_free_and_rechecks_wrappers() {
        let route = super::TravelRoute::Land(super::LandRoute {
            bridge: None,
            water_crossings: vec![],
        });
        let mut empty = super::RouteTerrain::stage_placeholder();
        empty.elevation_profile = super::RouteElevationProfile { samples: vec![] };
        assert!(
            std::panic::catch_unwind(|| empty.validate_context(&route, 1_000))
                .unwrap()
                .is_err()
        );

        let mut raw_slope = super::RouteTerrain::stage_placeholder();
        raw_slope.max_slope = super::RouteSlopePermille { permille: 10_001 };
        assert!(
            std::panic::catch_unwind(|| raw_slope.validate_context(&route, 1_000))
                .unwrap()
                .is_err()
        );

        let mut raw_elevation = super::RouteTerrain::stage_placeholder();
        raw_elevation.elevation_profile = super::RouteElevationProfile {
            samples: vec![
                super::RouteElevationSample {
                    progress: super::EdgeProgressPermille { permille: 0 },
                    elevation: super::ElevationMeters { meters: 9_001 },
                },
                super::RouteElevationSample {
                    progress: super::EdgeProgressPermille { permille: 1_000 },
                    elevation: super::ElevationMeters { meters: 0 },
                },
            ],
        };
        assert!(
            std::panic::catch_unwind(|| raw_elevation.validate_context(&route, 1_000))
                .unwrap()
                .is_err()
        );
        assert!(super::route_grade_permille(1, 0, 1).is_err());
        assert!(super::route_grade_permille(1, 1, 0).is_err());
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
