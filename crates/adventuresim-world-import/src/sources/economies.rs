//! Rules-v8 immutable settlement services and stock availability.

use std::collections::HashMap;

use adventuresim_world_schema::CompiledWorld;

use crate::{Error, Result};

pub(crate) fn enrich(mut world: CompiledWorld) -> Result<CompiledWorld> {
    let mut route_counts = HashMap::<u64, u16>::new();
    for edge in &world.edges {
        *route_counts.entry(edge.from_node_id).or_default() += 1;
        *route_counts.entry(edge.to_node_id).or_default() += 1;
    }
    let towns = world
        .nodes
        .iter()
        .map(|n| (n.id, n.is_town))
        .collect::<HashMap<_, _>>();
    for settlement in &mut world.settlements {
        let routes = route_counts
            .get(&settlement.source_node_id)
            .copied()
            .unwrap_or(0);
        let profile = adventuresim_world_schema::infer_settlement_economy(
            settlement.population_level,
            settlement.population_estimate,
            routes,
            towns
                .get(&settlement.source_node_id)
                .copied()
                .unwrap_or(false),
            &settlement.industries,
        )
        .map_err(|error| settlement_economy_error(settlement, "inference", error))?;
        append_note(
            &mut settlement.sources,
            "**Settlement economy rules v8:** Prosperity, services, specializations, and bounded stock categories are deterministically derived from population, documented town status, finalized route access, and canonical local production. General goods, meat, metalwares, weapons, armor, and herbs may be explicit deterministic gap-fill and are never attributed to EGDI or another source.",
        )?;
        settlement.economy = profile;
    }
    Ok(world)
}

pub(crate) fn validate_semantics(world: &CompiledWorld) -> Result<()> {
    let mut route_counts = HashMap::<u64, u16>::new();
    for edge in &world.edges {
        *route_counts.entry(edge.from_node_id).or_default() += 1;
        *route_counts.entry(edge.to_node_id).or_default() += 1;
    }
    let towns = world
        .nodes
        .iter()
        .map(|n| (n.id, n.is_town))
        .collect::<HashMap<_, _>>();
    for settlement in &world.settlements {
        settlement
            .economy
            .validate()
            .map_err(|error| settlement_economy_error(settlement, "validation", error))?;
        let expected = adventuresim_world_schema::infer_settlement_economy(
            settlement.population_level,
            settlement.population_estimate,
            route_counts
                .get(&settlement.source_node_id)
                .copied()
                .unwrap_or(0),
            towns
                .get(&settlement.source_node_id)
                .copied()
                .unwrap_or(false),
            &settlement.industries,
        )
        .map_err(|error| settlement_economy_error(settlement, "canonical inference", error))?;
        if settlement.economy != expected {
            return Err(settlement_economy_error(
                settlement,
                "validation",
                "stored profile is not canonical",
            ));
        }
    }
    Ok(())
}

fn settlement_economy_error(
    settlement: &adventuresim_world_schema::SettlementImport,
    operation: &str,
    error: impl std::fmt::Display,
) -> Error {
    Error::Validation(format!(
        "settlement economy {operation} failed for id {:?}, name {:?}, source node {}: {error}",
        settlement.id, settlement.name, settlement.source_node_id
    ))
}

fn append_note(sources: &mut String, note: &str) -> Result<()> {
    let addition = format!("\n- {note}");
    if sources.chars().count() + addition.chars().count()
        > adventuresim_world_schema::MAX_SOURCES_MARKDOWN_CHARS
    {
        return Err(Error::Validation(
            "settlement has no room for economy provenance".into(),
        ));
    }
    sources.push_str(&addition);
    Ok(())
}

#[cfg(test)]
mod tests {
    use adventuresim_world_schema::SettlementService as Service;
    #[test]
    fn tiny_places_are_generalist_and_urban_places_split_specialists() {
        let fallback = adventuresim_world_schema::InferredIndustryProfile::new(vec![
            adventuresim_world_schema::IndustryEvidence::Fallback(
                adventuresim_world_schema::FallbackIndustry::CommonAggregate,
            ),
        ])
        .unwrap();
        let tiny = adventuresim_world_schema::infer_settlement_economy(1, 90, 1, false, &fallback)
            .unwrap();
        assert!(tiny.has_service(Service::GeneralStore));
        assert!(!tiny.has_service(Service::Weaponsmith));
        let city =
            adventuresim_world_schema::infer_settlement_economy(5, 20_000, 6, true, &fallback)
                .unwrap();
        assert!(city.has_service(Service::Weaponsmith));
        assert!(city.has_service(Service::Armorer));
        assert!(!city.has_service(Service::GeneralBlacksmith));
        assert!(city.prosperity_score > tiny.prosperity_score);
    }
}
