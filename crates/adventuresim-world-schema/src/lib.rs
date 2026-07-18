//! Stable, source-independent types at the world compiler/database boundary.
//!
//! Keep this crate lightweight. Readers for CSV, raster, and vector formats
//! belong in `adventuresim-world-import`, not here or in the database module.

use serde::{Deserialize, Serialize};

pub const WORLD_SCHEMA_VERSION: u32 = 6;

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum TravelRoute {
    Land { bridge: Option<EdgeEndpoint> },
    Ferry,
}

impl TravelRoute {
    pub const fn kind(self) -> TravelEdgeKind {
        match self {
            Self::Land { .. } => TravelEdgeKind::Land,
            Self::Ferry => TravelEdgeKind::Ferry,
        }
    }

    pub const fn has_crossing(self) -> bool {
        matches!(self, Self::Land { bridge: Some(_) } | Self::Ferry)
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorldBuildReport {
    pub nodes: usize,
    pub edges: usize,
    pub settlements: usize,
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
    pub excluded_edges: std::collections::BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompiledWorld {
    pub metadata: WorldMetadata,
    pub nodes: Vec<WorldNodeImport>,
    pub edges: Vec<TravelEdgeImport>,
    pub settlements: Vec<SettlementImport>,
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
    pub scene_key: String,
    pub religion_id: String,
}

#[cfg(test)]
mod tests {
    use super::{
        CanopyDensity, ElevationBand, ElevationMeters, EuroVegMapUnitCode, ForestCover,
        HumanLandUseIntensity, LandUseFraction, LandUseProfile, MappedPotentialVegetation,
        PotentialVegetationFormation,
    };

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
}
