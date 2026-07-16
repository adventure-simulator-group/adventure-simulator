use std::collections::HashSet;

use adventuresim_world_schema::{
    CompiledWorld, TravelCrossing, TravelEdgeKind, WORLD_SCHEMA_VERSION,
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
        match (edge.kind, edge.crossing) {
            (TravelEdgeKind::Ferry, Some(TravelCrossing::Ferry))
            | (TravelEdgeKind::Land, Some(TravelCrossing::Bridge))
            | (TravelEdgeKind::Land, None) => {}
            (TravelEdgeKind::Ferry, _) => {
                return Err(Error::Validation(format!(
                    "ferry edge {} must have a ferry crossing",
                    edge.id
                )));
            }
            (TravelEdgeKind::Land, Some(TravelCrossing::Ferry)) => {
                return Err(Error::Validation(format!(
                    "land edge {} cannot have a ferry crossing",
                    edge.id
                )));
            }
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
                .filter(|edge| edge.crossing.is_some())
                .count()
        || world.report.toll_edges != world.edges.iter().filter(|edge| edge.has_toll).count()
    {
        return Err(Error::Validation(
            "build report counts do not match the compiled world".into(),
        ));
    }
    Ok(())
}
