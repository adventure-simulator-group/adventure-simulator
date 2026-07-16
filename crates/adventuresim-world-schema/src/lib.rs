//! Stable, source-independent types at the world compiler/database boundary.
//!
//! Keep this crate lightweight. Readers for CSV, raster, and vector formats
//! belong in `adventuresim-world-import`, not here or in the database module.

use serde::{Deserialize, Serialize};

pub const WORLD_SCHEMA_VERSION: u32 = 3;

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
    pub scene_key: String,
    pub religion_id: String,
}

#[cfg(test)]
mod tests {
    use super::{ElevationBand, ElevationMeters};

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
}
