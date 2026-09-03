use serde::{Deserialize, Serialize};

use crate::{GeologicUnitId, SurfaceLithology};

pub const MAX_GEOLOGIC_WINDOWS: usize = 50_000;
pub const MAX_GEOLOGIC_WINDOW_SIDE_METRES: i32 = 2_000;

/// A rectangle proven wholly contained in one mapped EGDI polygon, including
/// exclusion of its holes. Coordinates are EPSG:3034 metres. This establishes
/// mapped lithology coverage, not the presence of a particular landform.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MappedGeologicWindow {
    pub id: String,
    pub unit: GeologicUnitId,
    pub lithology: SurfaceLithology,
    /// West, south, east, north in EPSG:3034, rounded inward by the compiler.
    pub bounds_metres: [i32; 4],
}

impl MappedGeologicWindow {
    pub fn is_valid(&self) -> bool {
        let [west, south, east, north] = self.bounds_metres.map(i64::from);
        !self.id.is_empty()
            && self.id.len() <= 256
            && east > west
            && north > south
            && east - west <= i64::from(MAX_GEOLOGIC_WINDOW_SIDE_METRES)
            && north - south <= i64::from(MAX_GEOLOGIC_WINDOW_SIDE_METRES)
    }

    pub fn contains(&self, point: [f64; 2]) -> bool {
        let [west, south, east, north] = self.bounds_metres.map(f64::from);
        point[0] >= west && point[0] <= east && point[1] >= south && point[1] <= north
    }
}
