use adventuresim_world_schema::{
    CompiledWorld, MAX_FAULT_GEOMETRY_POINTS, MAX_FAULT_LINE_POINTS, PLAYABLE_BOUNDS,
    TerrainFeature, WorldBuildReport,
};

use crate::{Error, Result};

pub(crate) fn validate_world_semantics(world: &CompiledWorld) -> Result<()> {
    crate::sources::industries::validate_semantics(world)?;
    crate::sources::economies::validate_semantics(world)?;
    validate(&world.terrain_features, &world.report)
}

pub(crate) fn validate(features: &[TerrainFeature], report: &WorldBuildReport) -> Result<()> {
    let [west, south, east, north] = PLAYABLE_BOUNDS;
    let point_count = features
        .iter()
        .map(|feature| feature.geometry().len())
        .sum::<usize>();
    let invalid_geometry = features.iter().any(|feature| {
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
    if features.len() != report.fault_traces_imported
        || point_count != report.fault_geometry_points
        || point_count > MAX_FAULT_GEOMETRY_POINTS
        || features.windows(2).any(|pair| pair[0].id() >= pair[1].id())
        || invalid_geometry
    {
        return Err(Error::Validation(
            "fault geometry is unbounded, non-canonical, or inconsistent with the build report"
                .into(),
        ));
    }
    Ok(())
}
