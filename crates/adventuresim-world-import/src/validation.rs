use std::collections::HashSet;

use adventuresim_world_schema::{CompiledWorld, TreeSpeciesProfile, WORLD_SCHEMA_VERSION};

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
    {
        return Err(Error::Validation(
            "build report counts do not match the compiled world".into(),
        ));
    }
    Ok(())
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
    use super::tree_species_counts_are_consistent;

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
}
