use adventuresim_world_schema::{MAX_FAULT_GEOMETRY_POINTS, MAX_FAULT_LINE_POINTS, TerrainFeature};

use crate::{Error, Result, TerrainPack};

impl TerrainPack {
    pub const fn cultivated_square_count(&self) -> u64 {
        self.manifest.cultivated_square_count
    }

    pub fn terrain_features(&self) -> &[TerrainFeature] {
        &self.manifest.terrain_features
    }
}

pub(crate) fn validate(features: &[TerrainFeature], bounds: [f64; 4]) -> Result<()> {
    let [west, south, east, north] = bounds;
    let point_count = features
        .iter()
        .map(|feature| feature.geometry().len())
        .sum::<usize>();
    let invalid_geometry = features.iter().any(|feature| {
        if let TerrainFeature::MappedGeology(window) = feature {
            return !window.is_valid();
        }
        feature.id().is_empty()
            || feature.id().len() > 256
            || feature.geometry().len() < 2
            || feature.geometry().len() > MAX_FAULT_LINE_POINTS
            || feature.geometry().windows(2).any(|pair| pair[0] == pair[1])
            || feature.geometry().iter().any(|point| {
                point.longitude() < west
                    || point.longitude() > east
                    || point.latitude() < south
                    || point.latitude() > north
            })
    });
    if features
        .iter()
        .filter(|f| matches!(f, TerrainFeature::MappedGeology(_)))
        .count()
        > adventuresim_world_schema::MAX_GEOLOGIC_WINDOWS
        || point_count > MAX_FAULT_GEOMETRY_POINTS
        || features.windows(2).any(|pair| pair[0].id() >= pair[1].id())
        || invalid_geometry
    {
        return Err(Error::Validation(
            "terrain feature geometry is unbounded or non-canonical".into(),
        ));
    }
    Ok(())
}
