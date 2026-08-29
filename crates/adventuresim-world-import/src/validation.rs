use std::collections::{HashMap, HashSet};

use adventuresim_world_schema::{
    CURRENT_INFERENCE_RULES_VERSION, CompiledWorld, DerivedHistoricalVegetationMethod,
    DroughtProfile, HistoricalVegetation, MAX_EDGE_GEOMETRY_POINTS, MAX_FAULT_GEOMETRY_POINTS,
    MAX_FAULT_LINE_POINTS, MAX_WORLD_GEOMETRY_POINTS, PLAYABLE_BOUNDS, SettlementHydrology,
    SettlementImport, SoilEvidence, SurfaceGeology, TravelEdgeProvenance, TravelRoute,
    TreeSpeciesProfile, WORLD_SCHEMA_VERSION, historical_vegetation_matches_context,
    valid_sources_markdown,
};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

fn geodesic_segment_m(
    a: adventuresim_world_schema::TravelGeometryPoint,
    b: adventuresim_world_schema::TravelGeometryPoint,
) -> u32 {
    let dlat = (b.latitude() - a.latitude()).to_radians();
    let dlon = (b.longitude() - a.longitude()).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.latitude().to_radians().cos()
            * b.latitude().to_radians().cos()
            * (dlon / 2.0).sin().powi(2);
    (6_371_000.0 * 2.0 * h.sqrt().asin()).round() as u32
}

fn validate_inferred_geometry(
    edge: &adventuresim_world_schema::TravelEdgeImport,
    node_coordinates: &HashMap<u64, (f64, f64)>,
) -> Result<()> {
    if edge.geometry.len() < 2 || edge.geometry.len() > MAX_EDGE_GEOMETRY_POINTS {
        return Err(Error::Validation(format!(
            "inferred edge {} has invalid geometry size",
            edge.id
        )));
    }
    let [west, south, east, north] = PLAYABLE_BOUNDS;
    if edge.geometry.iter().any(|point| {
        let (lat, lon) = (point.latitude(), point.longitude());
        lon < west || lon > east || lat < south || lat > north
    }) || edge.geometry.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(Error::Validation(format!(
            "inferred edge {} has out-of-bounds or duplicate geometry points",
            edge.id
        )));
    }
    let endpoint_matches =
        |point: adventuresim_world_schema::TravelGeometryPoint, node_id: u64| -> bool {
            let Some(&(lat, lon)) = node_coordinates.get(&node_id) else {
                return false;
            };
            let Ok(expected) = adventuresim_world_schema::TravelGeometryPoint::new(lon, lat) else {
                return false;
            };
            point.longitude_e7.abs_diff(expected.longitude_e7) <= 1
                && point.latitude_e7.abs_diff(expected.latitude_e7) <= 1
        };
    if !endpoint_matches(edge.geometry[0], edge.from_node_id)
        || !endpoint_matches(*edge.geometry.last().unwrap(), edge.to_node_id)
    {
        return Err(Error::Validation(format!(
            "inferred edge {} geometry endpoints do not match its nodes",
            edge.id
        )));
    }
    let length = edge
        .geometry
        .windows(2)
        .map(|pair| geodesic_segment_m(pair[0], pair[1]) as u64)
        .sum::<u64>();
    if length.abs_diff(u64::from(edge.length_m)) > 2 {
        return Err(Error::Validation(format!(
            "inferred edge {} geometry length does not reconcile",
            edge.id
        )));
    }
    Ok(())
}

pub fn validate(world: &CompiledWorld) -> Result<()> {
    crate::sources::industries::validate_semantics(world)?;
    crate::sources::economies::validate_semantics(world)?;
    if world.metadata.schema_version != WORLD_SCHEMA_VERSION {
        return Err(Error::Validation(format!(
            "schema version {} is not supported (expected {WORLD_SCHEMA_VERSION})",
            world.metadata.schema_version
        )));
    }
    if world.metadata.inference_rules_version != CURRENT_INFERENCE_RULES_VERSION {
        return Err(Error::Validation(format!(
            "inference rules version {} is not supported (expected {CURRENT_INFERENCE_RULES_VERSION})",
            world.metadata.inference_rules_version
        )));
    }
    for source in &world.metadata.sources {
        crate::manifest::validate_source(source)?;
    }
    if world
        .metadata
        .sources
        .windows(2)
        .any(|pair| pair[0].id >= pair[1].id)
    {
        return Err(Error::Validation(
            "source manifests are duplicated or not in canonical id order".into(),
        ));
    }
    let expected_manifest_digest = crate::manifest::digest(
        world.metadata.world_year,
        world.metadata.spatial_grid,
        &world.metadata.sources,
    )?;
    if world.metadata.manifest_digest != expected_manifest_digest {
        return Err(Error::Validation(
            "world metadata manifest digest does not match its canonical manifest".into(),
        ));
    }
    let report = &world.report;
    let mut expected = std::collections::BTreeSet::new();
    if !world.nodes.is_empty() || !world.edges.is_empty() || !world.settlements.is_empty() {
        expected.insert("viabundus-v2");
    }
    for (used, id) in [
        (report.elevation_tiles_read > 0, "copernicus-dem-glo30"),
        (report.land_use_rasters_read > 0, "hyde-3-5-c9"),
        (report.forest_tiles_read > 0, "clms-forest-2018"),
        (
            report.potential_vegetation_raster_files_read > 0,
            "jung-pnv-1-1",
        ),
        (report.tree_species_rasters_read > 0, "eu-trees4f-v2"),
        (report.soil_rasters_read > 0, "soilgrids-v2-rolling"),
        (report.geology_features_read > 0, "egdi-surface-geology-1m"),
        (report.fault_features_read > 0, "hike-fault-db-v17b"),
        (
            report.religion_regions_read > 0,
            "ieg-religion-1544-curated",
        ),
        (report.drought_grid_cells_read > 0, "noaa-owda-v1"),
        (
            report.hydrology_files_read > 0
                || report.hydrology_features_read > 0
                || report.hydrology_settlement_samples > 0
                || report.hydrology_edge_crossings > 0
                || report.hydrology_inferred_ferry_waterways > 0,
            "copernicus-eu-hydro-1-3",
        ),
    ] {
        if used {
            expected.insert(id);
        }
    }
    let actual = world
        .metadata
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected {
        return Err(Error::Validation(format!(
            "canonical source manifest set does not match build evidence (expected {expected:?}, got {actual:?})"
        )));
    }

    let [west, south, east, north] = PLAYABLE_BOUNDS;
    let fault_points = world
        .terrain_features
        .iter()
        .map(|feature| feature.geometry().len())
        .sum::<usize>();
    if world.terrain_features.len() != report.fault_traces_imported
        || fault_points != report.fault_geometry_points
        || fault_points > MAX_FAULT_GEOMETRY_POINTS
        || world
            .terrain_features
            .windows(2)
            .any(|pair| pair[0].id() >= pair[1].id())
        || world.terrain_features.iter().any(|feature| {
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
        })
    {
        return Err(Error::Validation(
            "fault geometry is unbounded, non-canonical, or inconsistent with the build report"
                .into(),
        ));
    }

    let node_ids: HashSet<_> = world.nodes.iter().map(|node| node.id).collect();
    let node_coordinates = world
        .nodes
        .iter()
        .map(|node| (node.id, (node.latitude, node.longitude)))
        .collect::<HashMap<_, _>>();
    if node_ids.len() != world.nodes.len() {
        return Err(Error::Validation("world node IDs are not unique".into()));
    }
    for node in &world.nodes {
        if !valid_sources_markdown(&node.sources) {
            return Err(Error::Validation(format!(
                "world node {} has invalid source notes",
                node.id
            )));
        }
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
    let total_geometry = world
        .edges
        .iter()
        .try_fold(0usize, |total, edge| total.checked_add(edge.geometry.len()))
        .ok_or_else(|| Error::Validation("world geometry point count overflowed".into()))?;
    if total_geometry > MAX_WORLD_GEOMETRY_POINTS {
        return Err(Error::Validation(
            "world geometry point count exceeds its bound".into(),
        ));
    }
    for edge in &world.edges {
        if !valid_sources_markdown(&edge.sources) {
            return Err(Error::Validation(format!(
                "travel edge {} has invalid source notes",
                edge.id
            )));
        }
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
        match edge.provenance {
            TravelEdgeProvenance::DocumentedViabundus if !edge.geometry.is_empty() => {
                return Err(Error::Validation(format!(
                    "documented edge {} unexpectedly embeds normalized geometry",
                    edge.id
                )));
            }
            TravelEdgeProvenance::InferredWalkingLink => {
                if edge.id >> 63 != 1 || !matches!(edge.route, TravelRoute::Land(_)) {
                    return Err(Error::Validation(format!(
                        "inferred edge {} has invalid identity or route",
                        edge.id
                    )));
                }
                validate_inferred_geometry(edge, &node_coordinates)?;
            }
            _ => {}
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
        edge.terrain
            .validate_context(&edge.route, edge.length_m)
            .map_err(|reason| {
                Error::Validation(format!(
                    "travel edge {} has invalid terrain: {reason}",
                    edge.id
                ))
            })?;
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
    let inferred = world
        .edges
        .iter()
        .filter(|edge| edge.provenance == TravelEdgeProvenance::InferredWalkingLink)
        .collect::<Vec<_>>();
    let geometry_bytes = serde_json::to_vec(
        &inferred
            .iter()
            .map(|edge| (&edge.id, &edge.geometry))
            .collect::<Vec<_>>(),
    )?;
    let geometry_sha = format!("{:x}", Sha256::digest(geometry_bytes));
    if world.report.inferred_road_edges != inferred.len()
        || (!inferred.is_empty()
            && (world.report.base_terrain_package_sha256.len() != 64
                || world.report.inferred_road_geometry_sha256 != geometry_sha))
    {
        return Err(Error::Validation(
            "inferred-road report identity does not reconcile".into(),
        ));
    }
    let sum = |f: fn(&adventuresim_world_schema::RouteTerrain) -> usize| {
        world.edges.iter().map(|e| f(&e.terrain)).sum::<usize>()
    };
    if report.route_terrain_edges != world.edges.len()
        || report.route_terrain_dem_samples
            != world
                .edges
                .iter()
                .map(|edge| edge.terrain.elevation_profile.samples().len() * 9)
                .sum::<usize>()
        || report.route_terrain_dem_fallbacks > report.route_terrain_dem_samples
        || report.route_terrain_water_adjacencies != sum(|t| t.water_adjacencies.len())
        || report.route_terrain_landforms != sum(|t| t.landforms.len())
        || report.route_terrain_seasonal_risks != sum(|t| t.seasonal_risks.len())
        || report.route_terrain_encounter_tags != sum(|t| t.encounter_tags.len())
    {
        return Err(Error::Validation(
            "route-terrain report counters do not reconcile with canonical edges".into(),
        ));
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
        settlement.industries.validate().map_err(|reason| {
            Error::Validation(format!(
                "settlement {} has invalid industries: {reason}",
                settlement.id
            ))
        })?;
        if !valid_sources_markdown(&settlement.sources) {
            return Err(Error::Validation(format!(
                "settlement {} has invalid source notes",
                settlement.id
            )));
        }
        if !node_ids.contains(&settlement.source_node_id) {
            return Err(Error::Validation(format!(
                "settlement {} references an unknown source node",
                settlement.id
            )));
        }
        if !adventuresim_world_schema::valid_settlement_name(&settlement.name) {
            return Err(Error::Validation(format!(
                "settlement {} has an invalid name",
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
        if !historical_vegetation_matches_context(
            settlement.historical_vegetation,
            settlement.land_use,
            &settlement.potential_vegetation,
            settlement.soil,
            settlement.hydrology,
        ) {
            return Err(Error::Validation(format!(
                "settlement {} has historical vegetation inconsistent with its evidence",
                settlement.id
            )));
        }
    }
    let mut categories = [0usize; 10];
    let mut derived = 0usize;
    let mut fallback_outputs = 0usize;
    let mut fallback_settlements = 0usize;
    for settlement in &world.settlements {
        let mut has_fallback = false;
        for output in settlement.industries.outputs() {
            match output {
                adventuresim_world_schema::IndustryEvidence::Fallback(_) => {
                    fallback_outputs += 1;
                    has_fallback = true;
                }
                adventuresim_world_schema::IndustryEvidence::Derived(value) => {
                    derived += 1;
                    categories[match value {
                        adventuresim_world_schema::DerivedIndustry::Agriculture(_) => 0,
                        adventuresim_world_schema::DerivedIndustry::Fishing(_) => 1,
                        adventuresim_world_schema::DerivedIndustry::Quarrying(_) => 2,
                        adventuresim_world_schema::DerivedIndustry::Mining(_) => 3,
                        adventuresim_world_schema::DerivedIndustry::Pottery(_) => 4,
                        adventuresim_world_schema::DerivedIndustry::PeatCutting(_) => 5,
                        adventuresim_world_schema::DerivedIndustry::Forestry(_) => 6,
                        adventuresim_world_schema::DerivedIndustry::CharcoalBurning(_) => 7,
                        adventuresim_world_schema::DerivedIndustry::Saltmaking(_) => 8,
                        adventuresim_world_schema::DerivedIndustry::Construction(_) => 9,
                    }] += 1;
                }
            }
        }
        fallback_settlements += usize::from(has_fallback);
    }
    if report.industry_settlements != world.settlements.len()
        || report.industry_derived_outputs != derived
        || report.industry_fallback_outputs != fallback_outputs
        || report.industry_fallback_settlements != fallback_settlements
        || [
            report.industry_agriculture_outputs,
            report.industry_fishing_outputs,
            report.industry_quarrying_outputs,
            report.industry_mining_outputs,
            report.industry_pottery_outputs,
            report.industry_peat_outputs,
            report.industry_forestry_outputs,
            report.industry_charcoal_outputs,
            report.industry_saltmaking_outputs,
            report.industry_construction_outputs,
        ] != categories
    {
        return Err(Error::Validation(
            "industry report counters do not reconcile with canonical settlements".into(),
        ));
    }
    let alias_ids: HashSet<_> = world
        .settlement_aliases
        .iter()
        .map(|alias| alias.id.as_str())
        .collect();
    if alias_ids.len() != world.settlement_aliases.len() {
        return Err(Error::Validation(
            "settlement alias IDs are not unique".into(),
        ));
    }
    for alias in &world.settlement_aliases {
        if alias.id.trim().is_empty() {
            return Err(Error::Validation(
                "settlement alias ID must not be empty".into(),
            ));
        }
        if !settlement_ids.contains(alias.settlement_id.as_str()) {
            return Err(Error::Validation(format!(
                "settlement alias {} references an unknown settlement",
                alias.id
            )));
        }
        if alias.name.trim().is_empty() {
            return Err(Error::Validation(format!(
                "settlement alias {} has no name",
                alias.id
            )));
        }
    }
    let description_ids: HashSet<_> = world
        .settlement_descriptions
        .iter()
        .map(|description| description.id.as_str())
        .collect();
    if description_ids.len() != world.settlement_descriptions.len() {
        return Err(Error::Validation(
            "settlement description IDs are not unique".into(),
        ));
    }
    for description in &world.settlement_descriptions {
        if description.id.trim().is_empty() {
            return Err(Error::Validation(
                "settlement description ID must not be empty".into(),
            ));
        }
        if !settlement_ids.contains(description.settlement_id.as_str()) {
            return Err(Error::Validation(format!(
                "settlement description {} references an unknown settlement",
                description.id
            )));
        }
        if description.body.trim().is_empty() {
            return Err(Error::Validation(format!(
                "settlement description {} has no body",
                description.id
            )));
        }
    }
    let actual_historical = actual_historical_counts(&world.settlements)?;
    if world.report.nodes != world.nodes.len()
        || world.report.edges != world.edges.len()
        || world.report.settlements != world.settlements.len()
        || world.report.settlement_aliases != world.settlement_aliases.len()
        || world.report.settlement_descriptions != world.settlement_descriptions.len()
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
            world.report.potential_vegetation_raster_files_read,
            world.report.potential_vegetation_samples,
            world.report.potential_vegetation_posterior_samples,
            world.report.potential_vegetation_categorical_samples,
            world.report.potential_vegetation_inferred_samples,
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
            world.report.soil_rasters_read,
            world.report.soil_depth_layers_read,
            world.report.soil_samples,
            world.report.soil_fallback_samples,
            world
                .settlements
                .iter()
                .filter(|settlement| {
                    settlement.soil.evidence == SoilEvidence::DeterministicInference
                })
                .count(),
            world.settlements.len(),
        )
        || !historical_report_matches(&world.report, actual_historical)
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
        let actual = serde_json::json!({
            "nodes": world.nodes.len(),
            "edges": world.edges.len(),
            "settlements": world.settlements.len(),
            "settlement_aliases": world.settlement_aliases.len(),
            "settlement_descriptions": world.settlement_descriptions.len(),
            "route_crossings": world.edges.iter().filter(|edge| edge.route.has_crossing()).count(),
            "toll_edges": world.edges.iter().filter(|edge| edge.toll.is_some()).count(),
            "tree_species_fallbacks": world.settlements.iter().filter(|settlement| matches!(settlement.tree_species, TreeSpeciesProfile::Inferred(_))).count(),
            "tree_species_candidates": world.settlements.iter().map(|settlement| match &settlement.tree_species {
                TreeSpeciesProfile::Modeled(profile) => profile.candidates().len(),
                TreeSpeciesProfile::Inferred(profile) => profile.species().len(),
            }).sum::<usize>(),
            "soil_fallbacks": world.settlements.iter().filter(|settlement| settlement.soil.evidence == SoilEvidence::DeterministicInference).count(),
            "geology_fallbacks": world.settlements.iter().filter(|settlement| matches!(settlement.geology, SurfaceGeology::Inferred(_))).count(),
            "drought_fallbacks": world.settlements.iter().filter(|settlement| matches!(settlement.drought, DroughtProfile::Inferred(_))).count(),
            "hydrology_landlocked_settlements": world.settlements.iter().filter(|settlement| settlement.hydrology == SettlementHydrology::default()).count(),
            "hydrology_edge_crossings": world.edges.iter().map(|edge| match &edge.route {
                TravelRoute::Land(route) => route.water_crossings.len(),
                TravelRoute::Ferry(_) => 0,
            }).sum::<usize>(),
            "ferry_edges": world.edges.iter().filter(|edge| matches!(edge.route, TravelRoute::Ferry(_))).count(),
        });
        return Err(Error::Validation(format!(
            "build report counts do not match the compiled world; report={} actual={actual}",
            serde_json::to_string(&world.report).expect("world build report serializes")
        )));
    }
    Ok(())
}

fn actual_historical_counts(
    settlements: &[SettlementImport],
) -> Result<(usize, usize, usize, usize)> {
    settlements.iter().try_fold(
        (0_usize, 0_usize, 0_usize, 0_usize),
        |mut counts, settlement| {
            let target = match settlement.historical_vegetation {
                HistoricalVegetation::Direct(_) => &mut counts.0,
                HistoricalVegetation::Derived(value) => {
                    if value.method == DerivedHistoricalVegetationMethod::MultiSourceRulesV4TieBreak
                    {
                        counts.3 = counts.3.checked_add(1).ok_or_else(|| {
                            Error::Validation("historical tie-break count overflow".into())
                        })?;
                    }
                    &mut counts.1
                }
                HistoricalVegetation::Fallback(_) => &mut counts.2,
            };
            *target = target
                .checked_add(1)
                .ok_or_else(|| Error::Validation("historical evidence count overflow".into()))?;
            Ok(counts)
        },
    )
}

fn historical_report_matches(
    report: &adventuresim_world_schema::WorldBuildReport,
    actual: (usize, usize, usize, usize),
) -> bool {
    actual
        == (
            report.historical_vegetation_direct_samples,
            report.historical_vegetation_derived_samples,
            report.historical_vegetation_fallback_samples,
            report.historical_vegetation_tie_breaks,
        )
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
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
    rasters: usize,
    depth_layers: usize,
    samples: usize,
    fallbacks: usize,
    actual_fallbacks: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && fallbacks == actual_fallbacks
        && depth_layers <= rasters
        && ((settlements == 0 && rasters == 0 && depth_layers == 0)
            || (settlements > 0 && rasters > 0 && depth_layers > 0))
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
    rasters: usize,
    samples: usize,
    posterior: usize,
    categorical: usize,
    inferred: usize,
    settlements: usize,
) -> bool {
    samples == settlements
        && posterior + categorical + inferred == samples
        && ((settlements == 0 && rasters == 0) || (settlements > 0 && rasters == 7))
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
    const HYDE_SOURCE_FILES: usize = 4;
    samples == settlements
        && fallbacks <= samples
        && normalized <= samples - fallbacks
        && ((settlements == 0 && (rasters == 0 || rasters == HYDE_SOURCE_FILES))
            || (settlements > 0 && rasters == HYDE_SOURCE_FILES))
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
    use adventuresim_world_schema::{
        CURRENT_INFERENCE_RULES_VERSION, CompiledWorld, LandRoute, RouteTerrain, SpatialGridSpec,
        TravelEdgeImport, TravelEdgeProvenance, TravelGeometryPoint, TravelRoute,
        WORLD_SCHEMA_VERSION, WorldBuildReport, WorldMetadata,
    };
    use std::collections::HashMap;

    use super::elevation_counts_are_consistent;
    use super::forest_counts_are_consistent;
    use super::land_use_counts_are_consistent;
    use super::potential_vegetation_counts_are_consistent;
    use super::{
        drought_counts_are_consistent, geology_counts_are_consistent,
        hydrology_counts_are_consistent, religion_counts_are_consistent,
        soil_counts_are_consistent, tree_species_counts_are_consistent,
    };

    fn empty_world(schema_version: u32, inference_rules_version: u32) -> CompiledWorld {
        let spatial_grid = SpatialGridSpec::default();
        let sources = Vec::new();
        let manifest_digest = crate::manifest::digest(1544, spatial_grid, &sources).unwrap();
        CompiledWorld {
            metadata: WorldMetadata {
                schema_version,
                inference_rules_version,
                spatial_grid,
                world_year: 1544,
                manifest_digest,
                sources,
                road_types: Vec::new(),
            },
            nodes: Vec::new(),
            edges: Vec::new(),
            settlements: Vec::new(),
            settlement_aliases: Vec::new(),
            settlement_descriptions: Vec::new(),
            terrain_features: Vec::new(),
            report: WorldBuildReport::default(),
        }
    }

    fn inferred_edge() -> (TravelEdgeImport, HashMap<u64, (f64, f64)>) {
        let nodes = HashMap::from([(1, (51.0, 9.0)), (2, (51.0, 9.01))]);
        let geometry = vec![
            TravelGeometryPoint::new(9.0, 51.0).unwrap(),
            TravelGeometryPoint::new(9.005, 51.0).unwrap(),
            TravelGeometryPoint::new(9.01, 51.0).unwrap(),
        ];
        let length_m = geometry
            .windows(2)
            .map(|p| super::geodesic_segment_m(p[0], p[1]) as u64)
            .sum::<u64>() as u32;
        (
            TravelEdgeImport {
                id: 1 << 63,
                from_node_id: 1,
                to_node_id: 2,
                route: TravelRoute::Land(LandRoute {
                    bridge: None,
                    water_crossings: Vec::new(),
                }),
                provenance: TravelEdgeProvenance::InferredWalkingLink,
                geometry,
                toll: None,
                length_m,
                slope_multiplier: 1.0,
                terrain: RouteTerrain::stage_placeholder(),
                certainty: 1,
                section: "test".into(),
                sources: "- test".into(),
            },
            nodes,
        )
    }

    #[test]
    fn inferred_geometry_rejects_swapped_floating_duplicate_and_length_tampering() {
        let (edge, nodes) = inferred_edge();
        assert!(super::validate_inferred_geometry(&edge, &nodes).is_ok());
        let mut swapped = edge.clone();
        swapped.geometry.swap(0, 2);
        assert!(super::validate_inferred_geometry(&swapped, &nodes).is_err());
        let mut floating = edge.clone();
        floating.geometry[0] = TravelGeometryPoint::new(9.0001, 51.0).unwrap();
        assert!(super::validate_inferred_geometry(&floating, &nodes).is_err());
        let mut duplicate = edge.clone();
        duplicate.geometry[1] = duplicate.geometry[0];
        assert!(super::validate_inferred_geometry(&duplicate, &nodes).is_err());
        let mut length = edge;
        length.length_m += 50;
        assert!(super::validate_inferred_geometry(&length, &nodes).is_err());
    }

    #[test]
    fn metadata_validation_rejects_wrong_schema_and_inference_versions() {
        assert!(
            super::validate(&empty_world(
                WORLD_SCHEMA_VERSION - 1,
                CURRENT_INFERENCE_RULES_VERSION
            ))
            .unwrap_err()
            .to_string()
            .contains("schema version")
        );
        assert!(
            super::validate(&empty_world(
                WORLD_SCHEMA_VERSION,
                CURRENT_INFERENCE_RULES_VERSION + 1
            ))
            .unwrap_err()
            .to_string()
            .contains("inference rules version")
        );
    }

    #[test]
    fn build_report_cannot_claim_an_absent_source_manifest() {
        let mut world = empty_world(WORLD_SCHEMA_VERSION, CURRENT_INFERENCE_RULES_VERSION);
        world.report.elevation_tiles_read = 1;
        world.report.elevation_samples = 1;
        assert!(
            super::validate(&world)
                .unwrap_err()
                .to_string()
                .contains("does not match build evidence")
        );
    }

    #[test]
    fn build_report_rejects_an_extra_source_manifest() {
        let mut world = empty_world(WORLD_SCHEMA_VERSION, CURRENT_INFERENCE_RULES_VERSION);
        world.metadata.sources.push(crate::manifest::hydrology());
        world.metadata.manifest_digest = crate::manifest::digest(
            world.metadata.world_year,
            world.metadata.spatial_grid,
            &world.metadata.sources,
        )
        .unwrap();
        assert!(
            super::validate(&world)
                .unwrap_err()
                .to_string()
                .contains("does not match build evidence")
        );
    }

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
    fn historical_synthesis_report_reconciles_all_evidence_methods() {
        let valid = WorldBuildReport::default();
        assert!(super::historical_report_matches(&valid, (0, 0, 0, 0)));
        for field in 0..4 {
            let mut mislabeled = WorldBuildReport::default();
            match field {
                0 => mislabeled.historical_vegetation_direct_samples = 1,
                1 => mislabeled.historical_vegetation_derived_samples = 1,
                2 => mislabeled.historical_vegetation_fallback_samples = 1,
                _ => mislabeled.historical_vegetation_tie_breaks = 1,
            }
            assert!(!super::historical_report_matches(&mislabeled, (0, 0, 0, 0)));
        }
        let overflow = WorldBuildReport {
            historical_vegetation_direct_samples: usize::MAX,
            historical_vegetation_derived_samples: usize::MAX,
            ..WorldBuildReport::default()
        };
        assert!(!super::historical_report_matches(&overflow, (0, 0, 0, 0)));
    }

    #[test]
    fn land_use_report_requires_all_source_rasters_and_samples() {
        assert!(land_use_counts_are_consistent(4, 3, 1, 1, 3));
        assert!(land_use_counts_are_consistent(0, 0, 0, 0, 0));
        assert!(land_use_counts_are_consistent(4, 0, 0, 0, 0));
        assert!(!land_use_counts_are_consistent(5, 3, 0, 0, 3));
        assert!(!land_use_counts_are_consistent(4, 2, 0, 0, 3));
        assert!(!land_use_counts_are_consistent(4, 3, 4, 0, 3));
        assert!(!land_use_counts_are_consistent(4, 3, 1, 3, 3));
    }

    #[test]
    fn potential_vegetation_report_requires_source_polygons_and_all_samples() {
        assert!(potential_vegetation_counts_are_consistent(7, 3, 1, 1, 1, 3));
        assert!(potential_vegetation_counts_are_consistent(0, 0, 0, 0, 0, 0));
        assert!(!potential_vegetation_counts_are_consistent(
            0, 3, 1, 1, 1, 3
        ));
        assert!(!potential_vegetation_counts_are_consistent(
            7, 2, 1, 0, 1, 3
        ));
        assert!(!potential_vegetation_counts_are_consistent(
            7, 3, 2, 2, 0, 3
        ));
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
    fn soil_report_requires_source_rasters_and_exact_fallback_count() {
        assert!(soil_counts_are_consistent(207, 204, 3, 1, 1, 3));
        assert!(soil_counts_are_consistent(0, 0, 0, 0, 0, 0));
        assert!(!soil_counts_are_consistent(0, 204, 3, 1, 1, 3));
        assert!(!soil_counts_are_consistent(207, 204, 3, 2, 1, 3));
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
