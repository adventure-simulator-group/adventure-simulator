//! Rules-v8 immutable settlement services and stock availability.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use adventuresim_world_schema::{
    AgriculturalCommodity, CURRENT_INFERENCE_RULES_VERSION, CompiledWorld, ConstructionCommodity,
    DerivedIndustry, ForestCommodity, IndustryEvidence, ProductionScale,
    ProfileFactProvenance as Provenance, ProsperityTier, SettlementEconomyProfile,
    SettlementService as Service, SettlementStock, StockCategory as Stock,
};

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
        .map_err(Error::Validation)?;
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
        settlement.economy.validate().map_err(Error::Validation)?;
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
        .map_err(Error::Validation)?;
        if settlement.economy != expected {
            return Err(Error::Validation(format!(
                "settlement {} economy is not canonical",
                settlement.id
            )));
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn infer(
    population_level: i32,
    population: u32,
    routes: u16,
    documented_town: bool,
    industries: &[IndustryEvidence],
) -> Result<SettlementEconomyProfile> {
    let industrial = industries
        .iter()
        .map(|e| match e {
            IndustryEvidence::Derived(v) => match v.scale() {
                ProductionScale::Marginal => 12,
                ProductionScale::Local => 28,
                ProductionScale::Regional => 48,
            },
            IndustryEvidence::Fallback(_) => 5,
        })
        .sum::<u16>()
        .min(260);
    let population_points = ((population.max(1) as f64).log10() * 95.0) as u16;
    let score = (u16::try_from(population_level.max(1)).unwrap_or(1) * 85)
        .saturating_add(population_points)
        .saturating_add(industrial)
        .saturating_add(routes.min(8) * 18)
        .saturating_add(u16::from(documented_town) * 55)
        .min(1_000);
    let tier = match score {
        0..=249 => ProsperityTier::Subsistence,
        250..=419 => ProsperityTier::Modest,
        420..=599 => ProsperityTier::Comfortable,
        600..=779 => ProsperityTier::Prosperous,
        _ => ProsperityTier::Wealthy,
    };

    let mut services = BTreeSet::from([Service::Inn]);
    if population_level <= 1 && population < 250 {
        services.insert(Service::GeneralStore);
    } else {
        services.extend([Service::GeneralStore, Service::Market, Service::Temple]);
        if population_level <= 2 || score < 470 {
            services.insert(Service::GeneralBlacksmith);
        } else {
            services.extend([Service::Weaponsmith, Service::Armorer]);
        }
        if population_level >= 3 || score >= 500 {
            services.insert(Service::Tailor);
        }
        if population_level >= 3 || industries.iter().any(is_forest_or_peat) {
            services.insert(Service::Herbalist);
        }
    }

    let mut stock = BTreeMap::<Stock, SettlementStock>::new();
    let mut specializations = BTreeSet::new();
    let mut add = |category, abundance, provenance| {
        stock
            .entry(category)
            .and_modify(|v| v.abundance = v.abundance.max(abundance))
            .or_insert(SettlementStock {
                category,
                abundance,
                provenance,
            });
        if abundance >= 4 {
            specializations.insert(category);
        }
    };
    for evidence in industries {
        let IndustryEvidence::Derived(industry) = evidence else {
            continue;
        };
        let abundance = match industry.scale() {
            ProductionScale::Marginal => 2,
            ProductionScale::Local => 4,
            ProductionScale::Regional => 5,
        };
        let category = match industry {
            DerivedIndustry::Agriculture(v) => match v.commodity {
                AgriculturalCommodity::Grain => Stock::Grain,
                AgriculturalCommodity::Flax | AgriculturalCommodity::Wool => Stock::Cloth,
                AgriculturalCommodity::Dairy => Stock::Dairy,
                AgriculturalCommodity::Hides => Stock::Hides,
            },
            DerivedIndustry::Fishing(_) => Stock::Fish,
            DerivedIndustry::Quarrying(_) => Stock::Stone,
            DerivedIndustry::Mining(_) => Stock::Fuel,
            DerivedIndustry::Pottery(_) => Stock::Pottery,
            DerivedIndustry::PeatCutting(_) | DerivedIndustry::CharcoalBurning(_) => Stock::Fuel,
            DerivedIndustry::Forestry(v) => match v.commodity {
                ForestCommodity::Fuelwood => Stock::Fuel,
                _ => Stock::Timber,
            },
            DerivedIndustry::Saltmaking(_) => Stock::Salt,
            DerivedIndustry::Construction(v) => match v.commodity {
                ConstructionCommodity::Timber => Stock::Timber,
                ConstructionCommodity::Brick | ConstructionCommodity::RoofTile => Stock::Pottery,
                _ => Stock::Stone,
            },
        };
        add(
            category,
            abundance,
            Provenance::DerivedFromCanonicalEvidence,
        );
    }
    let gap = Provenance::DeterministicGapFill;
    add(
        Stock::GeneralGoods,
        if population_level <= 1 { 3 } else { 2 },
        gap,
    );
    if industries.iter().any(|e| matches!(e, IndustryEvidence::Derived(DerivedIndustry::Agriculture(v)) if matches!(v.commodity, AgriculturalCommodity::Dairy | AgriculturalCommodity::Hides))) { add(Stock::Meat, 3, Provenance::DerivedFromCanonicalEvidence); }
    if services.contains(&Service::GeneralBlacksmith) || services.contains(&Service::Weaponsmith) {
        add(Stock::Metalwares, if score >= 600 { 4 } else { 2 }, gap);
    }
    if services.contains(&Service::Weaponsmith) {
        add(Stock::Weapons, if score >= 700 { 4 } else { 2 }, gap);
    }
    if services.contains(&Service::Armorer) {
        add(Stock::Armor, if score >= 700 { 4 } else { 2 }, gap);
    }
    if services.contains(&Service::Herbalist) {
        add(
            Stock::Herbs,
            2 + u8::from(industries.iter().any(is_forest_or_peat)),
            gap,
        );
    }

    let profile = SettlementEconomyProfile {
        rules_version: CURRENT_INFERENCE_RULES_VERSION,
        prosperity_score: score,
        prosperity_tier: tier,
        services: services.into_iter().collect(),
        specializations: specializations.into_iter().collect(),
        stock: stock.into_values().collect(),
    };
    profile.validate().map_err(Error::Validation)?;
    Ok(profile)
}

fn is_forest_or_peat(e: &IndustryEvidence) -> bool {
    matches!(
        e,
        IndustryEvidence::Derived(DerivedIndustry::Forestry(_) | DerivedIndustry::PeatCutting(_))
    )
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
    use super::*;
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
