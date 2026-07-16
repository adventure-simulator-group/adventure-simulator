//! Stable, source-independent types at the world compiler/database boundary.
//!
//! Keep this crate lightweight. Readers for CSV, raster, and vector formats
//! belong in `adventuresim-world-import`, not here or in the database module.

use serde::{Deserialize, Serialize};

pub const WORLD_SCHEMA_VERSION: u32 = 2;

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
    pub scene_key: String,
    pub religion_id: String,
}
