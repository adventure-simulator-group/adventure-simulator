use std::collections::HashSet;

use adventuresim_world_schema::{
    CompiledWorld, DroughtProfile, SettlementHydrology, SoilProfile, SurfaceGeology, TravelRoute,
    TreeSpeciesProfile, WORLD_SCHEMA_VERSION,
};

use crate::{Error, Result};

pub fn validate(world: &CompiledWorld) -> Result<()> {
    if world.metadata.schema_version != WORLD_SCHEMA_VERSION {
        return Err(Error::Validation(format!(
            "schema version {} is not supported (expected {WORLD_SCHEMA_VERSION})",
            world.metadata.schema_version
        )));
    }

    let node_ids: HashSet<_> = world.nodes.iter().map(|node| node.id).collect();
    if node_ids.len() != world.nodes.len() {
        return Err(Error::Validation("world node IDs are not unique".into()));
    }
    for node in &world.nodes {
        if !node.latitude.is_finite()
            || !(-90.0..=90.0).contains(&node.latitude)
            || !node.longitude.is_finite()
            || !(-180.0..=180.0).contains(&node.longitude)
        {
            return Err(Error::Validation(format!(
                "world node {} has invalid coordinates",
                node.id
            )));
        }
        if node
            .parent_node_id
            .is_some_and(|parent_id| !node_ids.contains(&parent_id))
        {
            return Err(Error::Validation(format!(
                "world node {} references an unknown parent",
                node.id
            )));
        }
    }

    let edge_ids: HashSet<_> = world.edges.iter().map(|edge| edge.id).collect();
    if edge_ids.len() != world.edges.len() {
        return Err(Error::Validation("travel edge IDs are not unique".into()));
    }
    for edge in &world.edges {
        if !node_ids.contains(&edge.from_node_id) || !node_ids.contains(&edge.to_node_id) {
            return Err(Error::Validation(format!(
                "travel edge {} references an unknown node",
                edge.id
            )));
        }
        if edge.from_node_id == edge.to_node_id {
            return Err(Error::Validation(format!(
                "travel edge {} connects a node to itself",
                edge.id
            )));
        }
        if edge.length_m == 0 {
            return Err(Error::Validation(format!(
                "travel edge {} has zero length",
                edge.id
            )));
        }
        if !edge.slope_multiplier.is_finite() || edge.slope_multiplier <= 0.0 {
            return Err(Error::Validation(format!(
                "travel edge {} has an invalid slope multiplier",
                edge.id
            )));
        }
        if let TravelRoute::Land(route) = &edge.route
            && route
                .water_crossings
                .windows(2)
                .any(|pair| pair[0].position > pair[1].position)
        {
            return Err(Error::Validation(format!(
                "travel edge {} has unsorted water crossings",
                edge.id
            )));
        }
    }

    let settlement_ids: HashSet<_> = world
        .settlements
        .iter()
        .map(|settlement| settlement.id.as_str())
        .collect();
    if settlement_ids.len() != world.settlements.len() {
        return Err(Error::Validation("settlement IDs are not unique".into()));
    }
    for settlement in &world.settlements {
        if !node_ids.contains(&settlement.source_node_id) {
            return Err(Error::Validation(format!(
                "settlement {} references an unknown source node",
                settlement.id
            )));
        }
        if settlement.name.trim().is_empty() {
            return Err(Error::Validation(format!(
                "settlement {} has no name",
                settlement.id
            )));
        }
        if !(1..=5).contains(&settlement.population_level) {
            return Err(Error::Validation(format!(
                "settlement {} has an invalid population level",
                settlement.id
            )));
        }
        let node = world
            .nodes
            .iter()
            .find(|node| node.id == settlement.source_node_id)
            .expect("source-node existence was checked above");
        if !node.is_settlement {
            return Err(Error::Validation(format!(
                "settlement {} points to a non-settlement world node",
                settlement.id
            )));
        }
        if node.latitude != settlement.latitude || node.longitude != settlement.longitude {
            return Err(Error::Validation(format!(
                "settlement {} coordinates differ from its source node",
                settlement.id
            )));
        }
    }
    if world.report.nodes != world.nodes.len()
        || world.report.edges != world.edges.len()
        || world.report.settlements != world.settlements.len()
        || world.report.route_crossings
            != world
                .edges
                .iter()
                .filter(|edge| edge.route.has_crossing())
                .count()
        || world.report.toll_edges
            != world
                .edges
                .iter()
                .filter(|edge| edge.toll.is_some())
                .count()
        || !elevation_counts_are_consistent(
            world.report.elevation_tiles_read,
            world.report.elevation_samples,
            world.report.elevation_fallback_samples,
            world.settlements.len(),
        )
        || !land_use_counts_are_consistent(
            world.report.land_use_rasters_read,
            world.report.land_use_samples,
            world.report.land_use_fallback_samples,
            world.report.land_use_normalized_samples,
            world.settlements.len(),
        )
        || !forest_counts_are_consistent(
            world.report.forest_tiles_read,
            world.report.forest_samples,
            world.report.forest_fallback_samples,
            world.settlements.len(),
        )
        || !potential_vegetation_counts_are_consistent(
            world.report.potential_vegetation_polygons_read,
            world.report.potential_vegetation_samples,
            world.report.potential_vegetation_fallback_samples,
            world.settlements.len(),
        )
        || !tree_species_counts_are_consistent(
            world.report.tree_species_rasters_read,
            world.report.tree_species_samples,
            world.report.tree_species_fallback_samples,
            world.report.tree_species_candidates,
            world
                .settlements
                .iter()
                .filter(|settlement| {
                    matches!(settlement.tree_species, TreeSpeciesProfile::Inferred(_))
                })
                .count(),
            world
                .settlements
                .iter()
                .map(|settlement| match &settlement.tree_species {
                    TreeSpeciesProfile::Modeled(profile) => profile.candidates().len(),
                    TreeSpeciesProfile::Inferred(profile) => profile.species().len(),
                })
                .sum(),
            world.settlements.len(),
        )
        || !soil_counts_are_consistent(
            world.report.soil_polygons_read,
            world.report.soil_attribute_rows_read,
            world.report.soil_samples,
            world.report.soil_fallback_samples,
            world
                .settlements
                .iter()
                .filter(|settlement| matches!(settlement.soil, SoilProfile::Inferred(_)))
                .count(),
            world.settlements.len(),
        )
        || !geology_counts_are_consistent(
            world.report.geology_features_read,
            world.report.geology_samples,
            world.report.geology_fallback_samples,
            world
                .settlements
                .iter()
                .filter(|settlement| matches!(settlement.geology, SurfaceGeology::Inferred(_)))
                .count(),
            world.settlements.len(),
        )
        || !religion_counts_are_consistent(
            world.report.religion_regions_read,
            world.report.religion_samples,
            world.report.religion_fallback_samples,
            world.settlements.len(),
        )
        || !drought_counts_are_consistent(
            world.report.drought_grid_cells_read,
            world.report.drought_samples,
            world.report.drought_neighbor_samples,
            world.report.drought_fallback_samples,
            world
                .settlements
                .iter()
                .filter(|settlement| matches!(settlement.drought, DroughtProfile::Inferred(_)))
                .count(),
            world.settlements.len(),
        )
        || !hydrology_counts_are_consistent(
            world.report.hydrology_files_read,
            world.report.hydrology_features_read,
            world.report.hydrology_settlement_samples,
            world.report.hydrology_landlocked_settlements,
            world.report.hydrology_edge_crossings,
            world.report.hydrology_inferred_ferry_waterways,
            world
                .settlements
                .iter()
                .filter(|settlement| settlement.hydrology == SettlementHydrology::default())
                .count(),
            world
                .edges
                .iter()
                .map(|edge| match &edge.route {
                    TravelRoute::Land(route) => route.water_crossings.len(),
                    TravelRoute::Ferry(_) => 0,
                })
                .sum(),
            world
                .edges
                .iter()
                .filter(|edge| matches!(edge.route, TravelRoute::Ferry(_)))
                .count(),
            world.settlements.len(),
        )
    {
        return Err(Error::Validation(
            "build report counts do not match the compiled world".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn hydrology_counts_are_consistent(
    files: usize,
    features: usize,
    samples: usize,
    landlocked: usize,
    crossings: usize,
    inferred_ferries: usize,
    actual_landlocked: usize,
    actual_crossings: usize,
    ferries: usize,
    settlements: usize,
) -> bool {
    files > 0
        && features > 0
        && samples == settlements
        && landlocked == actual_landlocked
        && crossings == actual_crossings
        && inferred_ferries <= ferries
}

fn drought_counts_are_consistent(
    cells: usize,
    samples: usize,
    neighbors: usize,
    fallbacks: usize,
    actual_fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks == actual_fallbacks
        && neighbors
            .checked_add(fallbacks)
            .is_some_and(|classified| classified <= samples)
        && cells > 0
}

fn religion_counts_are_consistent(
    regions: usize,
    samples: usize,
    fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks <= samples
        && ((settlements == 0 && regions == 0) || (settlements > 0 && regions > 0))
}

fn geology_counts_are_consistent(
    features: usize,
    samples: usize,
    fallbacks: usize,
    actual_fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks == actual_fallbacks
        && ((settlements == 0 && features == 0) || (settlements > 0 && features > 0))
}

fn soil_counts_are_consistent(
    polygons: usize,
    attribute_rows: usize,
    samples: usize,
    fallbacks: usize,
    actual_fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks == actual_fallbacks
        && ((settlements == 0 && polygons == 0 && attribute_rows == 0)
            || (settlements > 0 && polygons > 0 && attribute_rows > 0))
}

fn tree_species_counts_are_consistent(
    rasters: usize,
    samples: usize,
    fallbacks: usize,
    candidates: usize,
    actual_fallbacks: usize,
    actual_candidates: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks == actual_fallbacks
        && candidates == actual_candidates
        && candidates >= samples
        && candidates <= samples.saturating_mul(adventuresim_world_schema::MAX_MODELED_TREE_SPECIES)
        && ((settlements == 0 && rasters == 0) || (settlements > 0 && rasters == 201))
}

fn potential_vegetation_counts_are_consistent(
    polygons: usize,
    samples: usize,
    fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks <= samples
        && ((settlements == 0 && polygons == 0) || (settlements > 0 && polygons > 0))
}

fn forest_counts_are_consistent(
    tiles: usize,
    samples: usize,
    fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks <= samples
        && ((settlements == 0 && tiles == 0)
            || (settlements > 0 && tiles > 0 && tiles <= settlements))
}

fn land_use_counts_are_consistent(
    rasters: usize,
    samples: usize,
    fallbacks: usize,
    normalized: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks <= samples
        && normalized <= samples - fallbacks
        && ((settlements == 0 && rasters == 0) || (settlements > 0 && rasters == 7))
}

fn elevation_counts_are_consistent(
    tiles: usize,
    samples: usize,
    fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks <= samples
        && ((settlements == 0 && tiles == 0)
            || (settlements > 0 && tiles > 0 && tiles <= settlements))
}

#[cfg(test)]
mod tests {
    use super::elevation_counts_are_consistent;
    use super::forest_counts_are_consistent;
    use super::land_use_counts_are_consistent;
    use super::potential_vegetation_counts_are_consistent;
    use super::{
        drought_counts_are_consistent, geology_counts_are_consistent,
        hydrology_counts_are_consistent, religion_counts_are_consistent,
        soil_counts_are_consistent, tree_species_counts_are_consistent,
    };

    #[test]
    fn elevation_report_requires_complete_consistent_counts() {
        assert!(elevation_counts_are_consistent(2, 3, 1, 3));
        assert!(elevation_counts_are_consistent(0, 0, 0, 0));
        assert!(!elevation_counts_are_consistent(0, 3, 0, 3));
        assert!(!elevation_counts_are_consistent(2, 2, 0, 3));
        assert!(!elevation_counts_are_consistent(2, 3, 4, 3));
        assert!(!elevation_counts_are_consistent(4, 3, 0, 3));
    }

    #[test]
    fn land_use_report_requires_all_source_rasters_and_samples() {
        assert!(land_use_counts_are_consistent(7, 3, 1, 1, 3));
        assert!(land_use_counts_are_consistent(0, 0, 0, 0, 0));
        assert!(!land_use_counts_are_consistent(6, 3, 0, 0, 3));
        assert!(!land_use_counts_are_consistent(7, 2, 0, 0, 3));
        assert!(!land_use_counts_are_consistent(7, 3, 4, 0, 3));
        assert!(!land_use_counts_are_consistent(7, 3, 1, 3, 3));
    }

    #[test]
    fn potential_vegetation_report_requires_source_polygons_and_all_samples() {
        assert!(potential_vegetation_counts_are_consistent(19_059, 3, 1, 3));
        assert!(potential_vegetation_counts_are_consistent(0, 0, 0, 0));
        assert!(!potential_vegetation_counts_are_consistent(0, 3, 0, 3));
        assert!(!potential_vegetation_counts_are_consistent(19_059, 2, 0, 3));
        assert!(!potential_vegetation_counts_are_consistent(19_059, 3, 4, 3));
    }

    #[test]
    fn forest_report_requires_complete_consistent_counts() {
        assert!(forest_counts_are_consistent(2, 3, 1, 3));
        assert!(forest_counts_are_consistent(0, 0, 0, 0));
        assert!(!forest_counts_are_consistent(0, 3, 0, 3));
        assert!(!forest_counts_are_consistent(2, 2, 0, 3));
        assert!(!forest_counts_are_consistent(2, 3, 4, 3));
    }

    #[test]
    fn tree_species_report_requires_all_triplets_and_nonempty_profiles() {
        assert!(tree_species_counts_are_consistent(201, 3, 1, 12, 1, 12, 3));
        assert!(tree_species_counts_are_consistent(0, 0, 0, 0, 0, 0, 0));
        assert!(!tree_species_counts_are_consistent(200, 3, 0, 3, 0, 3, 3));
        assert!(!tree_species_counts_are_consistent(201, 2, 0, 3, 0, 3, 3));
        assert!(!tree_species_counts_are_consistent(201, 3, 4, 3, 3, 3, 3));
        assert!(!tree_species_counts_are_consistent(201, 3, 0, 2, 0, 3, 3));
        assert!(!tree_species_counts_are_consistent(201, 3, 0, 12, 1, 12, 3));
        assert!(!tree_species_counts_are_consistent(201, 3, 0, 4, 0, 3, 3));
    }

    #[test]
    fn soil_report_requires_source_tables_and_exact_fallback_count() {
        assert!(soil_counts_are_consistent(10, 20, 3, 1, 1, 3));
        assert!(soil_counts_are_consistent(0, 0, 0, 0, 0, 0));
        assert!(!soil_counts_are_consistent(0, 20, 3, 1, 1, 3));
        assert!(!soil_counts_are_consistent(10, 20, 3, 2, 1, 3));
    }

    #[test]
    fn geology_report_requires_source_features_and_exact_fallback_count() {
        assert!(geology_counts_are_consistent(243_092, 3, 1, 1, 3));
        assert!(geology_counts_are_consistent(0, 0, 0, 0, 0));
        assert!(!geology_counts_are_consistent(0, 3, 1, 1, 3));
        assert!(!geology_counts_are_consistent(243_092, 3, 2, 1, 3));
    }

    #[test]
    fn religion_report_requires_regions_and_bounded_fallbacks() {
        assert!(religion_counts_are_consistent(14, 3, 2, 3));
        assert!(religion_counts_are_consistent(0, 0, 0, 0));
        assert!(!religion_counts_are_consistent(0, 3, 2, 3));
        assert!(!religion_counts_are_consistent(14, 3, 4, 3));
    }

    #[test]
    fn drought_report_requires_cells_and_exact_fallbacks() {
        assert!(drought_counts_are_consistent(5_414, 3, 1, 1, 1, 3));
        assert!(drought_counts_are_consistent(5_414, 0, 0, 0, 0, 0));
        assert!(!drought_counts_are_consistent(0, 3, 1, 1, 1, 3));
        assert!(!drought_counts_are_consistent(0, 0, 0, 0, 0, 0));
        assert!(!drought_counts_are_consistent(5_414, 3, 2, 2, 2, 3));
        assert!(!drought_counts_are_consistent(5_414, 3, 1, 1, 0, 3));
    }

    #[test]
    fn hydrology_report_requires_source_and_exact_classifications() {
        assert!(hydrology_counts_are_consistent(
            2, 30, 3, 1, 4, 1, 1, 4, 2, 3
        ));
        assert!(!hydrology_counts_are_consistent(
            0, 30, 3, 1, 4, 1, 1, 4, 2, 3
        ));
        assert!(!hydrology_counts_are_consistent(
            2, 30, 3, 1, 4, 1, 0, 4, 2, 3
        ));
        assert!(!hydrology_counts_are_consistent(
            2, 30, 3, 1, 4, 3, 1, 4, 2, 3
        ));
    }
}
