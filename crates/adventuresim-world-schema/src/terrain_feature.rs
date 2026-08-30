use serde::{Deserialize, Serialize};

use crate::TravelGeometryPoint;

pub const MAX_FAULT_GEOMETRY_POINTS: usize = 100_000;
pub const MAX_FAULT_LINE_POINTS: usize = 4_096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum TerrainFeature {
    MappedFault(MappedFault),
}

impl TerrainFeature {
    pub fn id(&self) -> &str {
        match self {
            Self::MappedFault(fault) => &fault.id,
        }
    }

    pub fn geometry(&self) -> &[TravelGeometryPoint] {
        match self {
            Self::MappedFault(fault) => &fault.trace,
        }
    }
}

/// Modern mapped fault evidence retained as a terrain-generation prior. The
/// source activity fields do not imply historical activity in the game year.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MappedFault {
    /// Stable source-qualified identifier.
    pub id: String,
    pub local_name: Option<String>,
    pub classification: Option<String>,
    pub mapped_active: bool,
    pub mapped_capable: bool,
    pub trace: Vec<TravelGeometryPoint>,
}
