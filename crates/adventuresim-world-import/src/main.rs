use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

use adventuresim_world_import::{Error, Result, WorldBuilder};
use adventuresim_world_schema::{
    AgriculturalLimitation, AvailableWaterCapacity, CationExchangeCapacity, CompiledWorld,
    CrossingTraversal, CrossingWatercourse, DerivedHistoricalVegetationCover,
    DirectHistoricalVegetationCover, DominantLeafType, DroughtHistory, DroughtProfile,
    EdgeEndpoint, FallbackHistoricalVegetationCover, FerryWaterway, FlowPersistence,
    FlowingWaterAccess, ForestCover, GeologicAgeEvidence, GeologicEra, GeologicLithologyEvidence,
    GridCellSizeMeters, HistoricalVegetation, IgneousRock, InlandWaterSize, MarineWaterAccess,
    MetamorphicRock, MineralSoilTexture, MixedLithology, NativeRangeEvidence, PotentialVegetation,
    PotentialVegetationClass, SedimentaryRock, SettlementAliasImport, SettlementDescriptionImport,
    SettlementDescriptionKind, SettlementImport, SettlementReligiousStatus, SoilAcidity, SoilDepth,
    SoilEvidence, SoilFertility, SoilProfile, SoilSubstrate, SoilWaterRegime, SpatialGridSpec,
    SurfaceGeology, SurfaceLithology, TopsoilOrganicCarbon, TravelEdgeImport, TravelRoute,
    TreeSpeciesProfile, UnconsolidatedDeposit, WesternChristianArrangement, WorldNodeImport,
    WrbReferenceGroup,
};
use clap::Parser;
use serde_json::{Value, json};

const WORLD_YEAR: i32 = 1544;
// `spacetime call` transports reducer arguments on the command line. Keep a
// safety margin below Windows' 32,767-character process command-line limit.
const MAX_REDUCER_ARGUMENT_CHARS: usize = 24_000;

#[derive(Debug, Parser)]
#[command(about = "Compile source datasets into the Adventure Simulator strategic world")]
struct Args {
    #[arg(long, alias = "raw-dir", default_value_os_t = default_viabundus_directory())]
    viabundus_dir: PathBuf,
    #[arg(long, default_value_os_t = default_elevation_directory())]
    elevation_dir: PathBuf,
    #[arg(long, default_value_os_t = default_land_use_directory())]
    land_use_dir: PathBuf,
    #[arg(long, default_value_os_t = default_forest_cover_directory())]
    forest_cover_dir: PathBuf,
    #[arg(long, default_value_os_t = default_potential_vegetation_directory())]
    potential_vegetation_dir: PathBuf,
    #[arg(long, default_value_os_t = default_tree_species_archive())]
    tree_species_archive: PathBuf,
    #[arg(long, default_value_os_t = default_soilgrids_directory())]
    soilgrids_dir: PathBuf,
    #[arg(long, default_value_os_t = default_geology_geopackage())]
    geology_geopackage: PathBuf,
    #[arg(long, default_value_os_t = default_religion_regions())]
    religion_regions: PathBuf,
    #[arg(long, default_value_os_t = default_drought_netcdf())]
    drought_netcdf: PathBuf,
    #[arg(long, default_value_os_t = default_hydrology_directory())]
    hydrology_dir: PathBuf,
    #[arg(long, default_value_t = WORLD_YEAR)]
    year: i32,
    #[arg(long, default_value_t = GridCellSizeMeters::default())]
    grid_cell_size_meters: GridCellSizeMeters,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    load: bool,
    #[arg(long, default_value = "spacetime")]
    spacetime: String,
    #[arg(long, default_value = "http://localhost:3000")]
    server: String,
    #[arg(long, default_value = "adventuresim-stdb-module")]
    database: String,
    #[arg(long, default_value_t = 100)]
    batch_size: usize,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<()> {
    if args.batch_size == 0 {
        return Err(Error::Validation("batch size must be positive".into()));
    }
    let world = WorldBuilder::new(args.year)
        .with_spatial_grid(SpatialGridSpec::new(args.grid_cell_size_meters))
        .build_from_sources(
            &args.viabundus_dir,
            &args.elevation_dir,
            &args.land_use_dir,
            &args.forest_cover_dir,
            &args.potential_vegetation_dir,
            &args.tree_species_archive,
            &args.soilgrids_dir,
            &args.geology_geopackage,
            &args.religion_regions,
            &args.drought_netcdf,
            &args.hydrology_dir,
        )?;
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| default_output(args.year));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut artifact = serde_json::to_vec(&world)?;
    let artifact_id = blake3::hash(&artifact).to_hex().to_string();
    artifact.push(b'\n');
    std::fs::write(&output, artifact)?;
    println!("{}", serde_json::to_string_pretty(&world.report)?);
    println!(
        "Source manifest {} ({} distributions):",
        world.metadata.manifest_digest,
        world.metadata.sources.len()
    );
    for source in &world.metadata.sources {
        println!(
            "  {}: {} [{}]",
            source.id,
            source.name,
            if source.content_identity.is_reproducible() {
                "reproducible"
            } else {
                "not reproducible"
            }
        );
    }
    println!("Wrote compiled world to {}", output.display());
    if args.load {
        load_world(&world, &artifact_id, &args)?;
    }
    Ok(())
}

fn load_world(world: &CompiledWorld, artifact_id: &str, args: &Args) -> Result<()> {
    let sources = manifest_audit_markdown(world)?;
    call_reducer(
        args,
        "begin_world_data_import",
        &[
            json!(world.metadata.schema_version),
            json!(artifact_id),
            json!(world.metadata.manifest_digest),
            json!(sources),
        ],
    )?;

    for (label, reducer, batches) in [
        (
            "nodes",
            "import_world_nodes",
            serialize_batches(&world.nodes, args.batch_size, encode_world_node)?,
        ),
        (
            "edges",
            "import_travel_edges",
            serialize_batches(&world.edges, args.batch_size, encode_travel_edge)?,
        ),
        (
            "settlements",
            "import_settlements",
            serialize_batches(&world.settlements, args.batch_size, encode_settlement)?,
        ),
        (
            "settlement aliases",
            "import_settlement_aliases",
            serialize_batches(
                &world.settlement_aliases,
                args.batch_size,
                encode_settlement_alias,
            )?,
        ),
        (
            "settlement descriptions",
            "import_settlement_descriptions",
            serialize_batches(
                &world.settlement_descriptions,
                args.batch_size,
                encode_settlement_description,
            )?,
        ),
    ] {
        let total = batches.iter().map(Vec::len).sum::<usize>();
        for (index, batch) in batches.into_iter().enumerate() {
            println!(
                "Loading {label}: batch {} ({} rows)",
                index + 1,
                batch.len()
            );
            call_reducer(args, reducer, &[Value::Array(batch)])?;
        }
        println!("Loaded {total} {label}.");
    }
    call_reducer(args, "finish_world_data_import", &[json!(artifact_id)])?;
    Ok(())
}

fn manifest_audit_markdown(world: &CompiledWorld) -> Result<String> {
    let mut output = String::new();
    for source in &world.metadata.sources {
        let canonical = serde_json::to_string(source)?;
        let section = format!(
            "## [{}]({})\n\nContent status: **{}**\n\n```json\n{}\n```\n\n",
            source.name,
            source.canonical_url,
            if source.content_identity.is_reproducible() {
                "reproducible"
            } else {
                "not reproducible"
            },
            canonical,
        );
        output.push_str(&section);
        if !adventuresim_world_schema::valid_sources_markdown(&output)
            || serde_json::to_string(&output)?.encode_utf16().count() > MAX_REDUCER_ARGUMENT_CHARS
        {
            return Err(Error::Validation(format!(
                "complete serialized source-manifest audit exceeds the {MAX_REDUCER_ARGUMENT_CHARS}-character reducer transport budget; refusing to omit or truncate operational notices"
            )));
        }
    }
    if !adventuresim_world_schema::valid_sources_markdown(&output) {
        return Err(Error::Validation(
            "source-manifest audit Markdown is empty or invalid".into(),
        ));
    }
    Ok(output)
}

fn serialize_batches<T>(
    rows: &[T],
    batch_size: usize,
    encode: fn(&T) -> Result<Value>,
) -> Result<Vec<Vec<Value>>> {
    let rows = rows.iter().map(encode).collect::<Result<Vec<_>>>()?;
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut code_units = 2;
    for row in rows {
        let encoded = row.to_string();
        let row_code_units = encoded.encode_utf16().count();
        if row_code_units + 2 > MAX_REDUCER_ARGUMENT_CHARS {
            return Err(Error::Validation(format!(
                "a serialized import row requires {} UTF-16 code units; maximum is {}",
                row_code_units + 2,
                MAX_REDUCER_ARGUMENT_CHARS
            )));
        }
        let row_code_units = row_code_units + usize::from(!batch.is_empty());
        if !batch.is_empty()
            && (batch.len() == batch_size
                || code_units + row_code_units > MAX_REDUCER_ARGUMENT_CHARS)
        {
            batches.push(std::mem::take(&mut batch));
            code_units = 2;
        }
        code_units += row_code_units;
        batch.push(row);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    Ok(batches)
}

fn encode_world_node(node: &WorldNodeImport) -> Result<Value> {
    let parent_node_id = match node.parent_node_id {
        Some(parent) => json!({ "some": parent }),
        None => json!({ "none": [] }),
    };
    Ok(json!({
        "id": node.id,
        "parent_node_id": parent_node_id,
        "latitude": node.latitude,
        "longitude": node.longitude,
        "is_settlement": node.is_settlement,
        "is_town": node.is_town,
        "is_ferry": node.is_ferry,
        "is_harbour": node.is_harbour,
        "sources": node.sources,
    }))
}

fn encode_travel_edge(edge: &TravelEdgeImport) -> Result<Value> {
    let terrain = encode_route_terrain(&edge.terrain);
    let route = match &edge.route {
        TravelRoute::Land(route) => json!({ "Land": {
            "bridge": encode_endpoint(route.bridge),
            "water_crossings": route.water_crossings.iter().map(|crossing| json!({
                "position": { "permille": crossing.position.get() },
                "watercourse": encode_crossing_watercourse(crossing.watercourse),
                "traversal": enum_unit(match crossing.traversal { CrossingTraversal::Bridge => "Bridge", CrossingTraversal::Ford => "Ford" }),
            })).collect::<Vec<_>>()
        }}),
        TravelRoute::Ferry(route) => json!({ "Ferry": {
            "waterway": encode_ferry_waterway(route.waterway)
        }}),
    };
    Ok(json!({
        "id": edge.id,
        "from_node_id": edge.from_node_id,
        "to_node_id": edge.to_node_id,
        "route": route,
        "toll": encode_endpoint(edge.toll),
        "length_m": edge.length_m,
        "slope_multiplier": edge.slope_multiplier,
        "terrain": terrain,
        "certainty": edge.certainty,
        "section": edge.section,
        "sources": edge.sources,
    }))
}

fn encode_route_terrain(terrain: &adventuresim_world_schema::RouteTerrain) -> Value {
    use adventuresim_world_schema::{
        DominantAspect, RouteEncounterTag, RouteLandformKind, RouteRiskSeverity,
        RouteSeasonalHazard, RouteTerrainClass, RouteWaterFeatureKind,
    };
    let named = |name: &str| enum_unit(name);
    json!({
        "elevation_profile": { "samples": terrain.elevation_profile.samples().iter().map(|sample| json!({
            "progress": { "permille": sample.progress.get() },
            "elevation": { "meters": sample.elevation.get() },
        })).collect::<Vec<_>>() },
        "ascent": { "meters": terrain.ascent.get() },
        "descent": { "meters": terrain.descent.get() },
        "max_uphill_grade": { "permille": terrain.max_uphill_grade.get() },
        "max_downhill_grade": { "permille": terrain.max_downhill_grade.get() },
        "mean_slope": { "permille": terrain.mean_slope.get() },
        "max_slope": { "permille": terrain.max_slope.get() },
        "dominant_aspect": named(match terrain.dominant_aspect { DominantAspect::Flat=>"Flat", DominantAspect::North=>"North", DominantAspect::NorthEast=>"NorthEast", DominantAspect::East=>"East", DominantAspect::SouthEast=>"SouthEast", DominantAspect::South=>"South", DominantAspect::SouthWest=>"SouthWest", DominantAspect::West=>"West", DominantAspect::NorthWest=>"NorthWest" }),
        "roughness": { "meters": terrain.roughness.get() },
        "relief": { "meters": terrain.relief.get() },
        "landforms": terrain.landforms.iter().map(|v| json!({ "progress": { "permille": v.progress.get() }, "kind": named(match v.kind { RouteLandformKind::Ridge=>"Ridge", RouteLandformKind::Valley=>"Valley", RouteLandformKind::LikelyPass=>"LikelyPass" }) })).collect::<Vec<_>>(),
        "class": named(match terrain.class { RouteTerrainClass::Flat=>"Flat", RouteTerrainClass::Rolling=>"Rolling", RouteTerrainClass::Hilly=>"Hilly", RouteTerrainClass::Mountainous=>"Mountainous" }),
        "water_adjacencies": terrain.water_adjacencies.iter().map(|v| json!({ "feature": named(match v.feature { RouteWaterFeatureKind::River=>"River", RouteWaterFeatureKind::Canal=>"Canal", RouteWaterFeatureKind::Ditch=>"Ditch", RouteWaterFeatureKind::Inland=>"Inland", RouteWaterFeatureKind::Tidal=>"Tidal", RouteWaterFeatureKind::Coastal=>"Coastal" }), "distance": { "meters": v.distance.get() } })).collect::<Vec<_>>(),
        "seasonal_risks": terrain.seasonal_risks.iter().map(|v| json!({ "hazard": named(match v.hazard { RouteSeasonalHazard::SpringFlood=>"SpringFlood", RouteSeasonalHazard::AutumnMud=>"AutumnMud", RouteSeasonalHazard::WinterIce=>"WinterIce", RouteSeasonalHazard::WinterSnow=>"WinterSnow" }), "severity": named(match v.severity { RouteRiskSeverity::Low=>"Low", RouteRiskSeverity::Medium=>"Medium", RouteRiskSeverity::High=>"High" }) })).collect::<Vec<_>>(),
        "encounter_tags": terrain.encounter_tags.iter().map(|v| named(match v { RouteEncounterTag::Flat=>"Flat", RouteEncounterTag::Rolling=>"Rolling", RouteEncounterTag::Hilly=>"Hilly", RouteEncounterTag::Mountainous=>"Mountainous", RouteEncounterTag::Steep=>"Steep", RouteEncounterTag::Rough=>"Rough", RouteEncounterTag::Ridge=>"Ridge", RouteEncounterTag::Valley=>"Valley", RouteEncounterTag::LikelyPass=>"LikelyPass", RouteEncounterTag::Bridge=>"Bridge", RouteEncounterTag::Ford=>"Ford", RouteEncounterTag::Ferry=>"Ferry", RouteEncounterTag::Riverbank=>"Riverbank", RouteEncounterTag::CanalBank=>"CanalBank", RouteEncounterTag::Lakeshore=>"Lakeshore", RouteEncounterTag::TidalShore=>"TidalShore", RouteEncounterTag::Coast=>"Coast", RouteEncounterTag::SpringFlood=>"SpringFlood", RouteEncounterTag::AutumnMud=>"AutumnMud", RouteEncounterTag::WinterIce=>"WinterIce", RouteEncounterTag::WinterSnow=>"WinterSnow" })).collect::<Vec<_>>(),
    })
}

fn encode_crossing_watercourse(watercourse: CrossingWatercourse) -> Value {
    match watercourse {
        CrossingWatercourse::River(river) => json!({ "River": {
            "order": { "order": river.order.get() },
            "persistence": encode_flow_persistence(river.persistence),
        }}),
        CrossingWatercourse::Canal(canal) => json!({ "Canal": { "navigable": canal.navigable } }),
        CrossingWatercourse::Ditch => json!({ "Ditch": [] }),
    }
}

fn encode_ferry_waterway(waterway: FerryWaterway) -> Value {
    match waterway {
        FerryWaterway::River(river) => json!({ "River": {
            "order": { "order": river.order.get() },
            "persistence": encode_flow_persistence(river.persistence),
        }}),
        FerryWaterway::InlandWater => json!({ "InlandWater": [] }),
        FerryWaterway::TidalWater => json!({ "TidalWater": [] }),
        FerryWaterway::CoastalWater => json!({ "CoastalWater": [] }),
    }
}

fn encode_flow_persistence(persistence: FlowPersistence) -> Value {
    enum_unit(match persistence {
        FlowPersistence::Perennial => "Perennial",
        FlowPersistence::Intermittent => "Intermittent",
        FlowPersistence::Ephemeral => "Ephemeral",
    })
}

fn encode_endpoint(endpoint: Option<EdgeEndpoint>) -> Value {
    match endpoint {
        Some(EdgeEndpoint::From) => json!({ "some": { "From": [] } }),
        Some(EdgeEndpoint::To) => json!({ "some": { "To": [] } }),
        Some(EdgeEndpoint::Both) => json!({ "some": { "Both": [] } }),
        None => json!({ "none": [] }),
    }
}

fn encode_settlement(settlement: &SettlementImport) -> Result<Value> {
    Ok(json!({
        "id": settlement.id,
        "source_node_id": settlement.source_node_id,
        "name": settlement.name,
        "longitude": settlement.longitude,
        "latitude": settlement.latitude,
        "population_level": settlement.population_level,
        "population_estimate": settlement.population_estimate,
        "elevation": settlement.elevation,
        "land_use": settlement.land_use,
        "forest_cover": encode_forest_cover(settlement.forest_cover),
        "potential_vegetation": encode_potential_vegetation(&settlement.potential_vegetation),
        "historical_vegetation": encode_historical_vegetation(settlement.historical_vegetation),
        "tree_species": encode_tree_species(&settlement.tree_species),
        "soil": encode_soil(&settlement.soil),
        "geology": encode_geology(&settlement.geology),
        "religious_status": encode_religious_status(settlement.religious_status),
        "drought": encode_drought(settlement.drought),
        "hydrology": encode_hydrology(settlement.hydrology),
        "industries": encode_industries(&settlement.industries),
        "scene_key": settlement.scene_key,
        "sources": settlement.sources,
    }))
}

fn encode_industries(profile: &adventuresim_world_schema::InferredIndustryProfile) -> Value {
    use adventuresim_world_schema::{
        AgriculturalCommodity as A, ConstructionCommodity as C, DerivedIndustry as D,
        FallbackIndustry as F, FishCommodity as Fish, ForestCommodity as Forest,
        IndustryEvidence as E, MinedCommodity, PotteryCommodity as P, ProductionScale as S,
        QuarryCommodity as Q, SaltSource as Salt,
    };
    let unit = |name: &str| enum_unit(name);
    let scale = |v| {
        unit(match v {
            S::Marginal => "Marginal",
            S::Local => "Local",
            S::Regional => "Regional",
        })
    };
    let outputs = profile.outputs().iter().map(|e| match e {
        E::Fallback(v) => json!({ "Fallback": unit(match v { F::FreshwaterFishing=>"FreshwaterFishing", F::GrazingDairy=>"GrazingDairy", F::CroplandGrain=>"CroplandGrain", F::WoodlandFuelwood=>"WoodlandFuelwood", F::CommonAggregate=>"CommonAggregate" }) }),
        E::Derived(v) => {
            let inner = match v {
                D::Agriculture(v) => json!({"Agriculture":{"commodity":unit(match v.commodity { A::Grain=>"Grain", A::Flax=>"Flax", A::Wool=>"Wool", A::Dairy=>"Dairy", A::Hides=>"Hides"}),"scale":scale(v.scale)}}),
                D::Fishing(v) => json!({"Fishing":{"commodity":unit(match v.commodity { Fish::Freshwater=>"Freshwater", Fish::Estuarine=>"Estuarine", Fish::Marine=>"Marine"}),"scale":scale(v.scale)}}),
                D::Quarrying(v) => json!({"Quarrying":{"commodity":unit(match v.commodity { Q::Limestone=>"Limestone",Q::Chalk=>"Chalk",Q::Sandstone=>"Sandstone",Q::Slate=>"Slate",Q::Granite=>"Granite",Q::Basalt=>"Basalt",Q::Marble=>"Marble",Q::Quartzite=>"Quartzite",Q::OtherHardStone=>"OtherHardStone"}),"scale":scale(v.scale)}}),
                D::Mining(v) => json!({"Mining":{"commodity":unit(match v.commodity { MinedCommodity::Coal=>"Coal"}),"scale":scale(v.scale)}}),
                D::Pottery(v) => json!({"Pottery":{"commodity":unit(match v.commodity { P::Clay=>"Clay",P::Earthenware=>"Earthenware"}),"scale":scale(v.scale)}}),
                D::PeatCutting(v) => json!({"PeatCutting":{"scale":scale(v.scale)}}),
                D::Forestry(v) => json!({"Forestry":{"commodity":unit(match v.commodity { Forest::Hardwood=>"Hardwood",Forest::Softwood=>"Softwood",Forest::Mixed=>"Mixed",Forest::Fuelwood=>"Fuelwood"}),"scale":scale(v.scale)}}),
                D::CharcoalBurning(v) => json!({"CharcoalBurning":{"scale":scale(v.scale)}}),
                D::Saltmaking(v) => json!({"Saltmaking":{"source":unit(match v.source { Salt::Evaporite=>"Evaporite",Salt::SalineSoil=>"SalineSoil",Salt::CoastalBrine=>"CoastalBrine"}),"scale":scale(v.scale)}}),
                D::Construction(v) => json!({"Construction":{"commodity":unit(match v.commodity { C::DimensionStone=>"DimensionStone",C::Sand=>"Sand",C::Gravel=>"Gravel",C::Brick=>"Brick",C::RoofTile=>"RoofTile",C::Timber=>"Timber"}),"scale":scale(v.scale)}}),
            };
            json!({"Derived": inner})
        }
    }).collect::<Vec<_>>();
    json!({"outputs": outputs})
}

fn encode_hydrology(hydrology: adventuresim_world_schema::SettlementHydrology) -> Value {
    let distance =
        |value: adventuresim_world_schema::WaterDistanceMeters| json!({ "meters": value.get() });
    let flowing = match hydrology.flowing {
        Some(FlowingWaterAccess::River(river)) => json!({ "some": { "River": {
            "distance": distance(river.distance), "order": { "order": river.order.get() },
            "persistence": encode_flow_persistence(river.persistence)
        }}}),
        Some(FlowingWaterAccess::RiverAndCanal(access)) => json!({ "some": { "RiverAndCanal": {
            "river": { "distance": distance(access.river.distance), "order": { "order": access.river.order.get() }, "persistence": encode_flow_persistence(access.river.persistence) },
            "canal_distance": distance(access.canal_distance), "canal_navigable": access.canal_navigable
        }}}),
        None => json!({ "none": [] }),
    };
    let inland = match hydrology.inland {
        Some(inland) => {
            json!({ "some": { "distance": distance(inland.distance), "size": enum_unit(match inland.size {
                InlandWaterSize::Pond => "Pond", InlandWaterSize::Lake => "Lake", InlandWaterSize::GreatLake => "GreatLake"
            })}})
        }
        None => json!({ "none": [] }),
    };
    let marine = match hydrology.marine {
        Some(MarineWaterAccess::Tidal(value)) => json!({ "some": { "Tidal": distance(value) }}),
        Some(MarineWaterAccess::OpenCoast(value)) => {
            json!({ "some": { "OpenCoast": distance(value) }})
        }
        None => json!({ "none": [] }),
    };
    json!({ "flowing": flowing, "inland": inland, "marine": marine })
}

fn encode_drought(profile: DroughtProfile) -> Value {
    let history = |history: DroughtHistory| {
        json!({
            "current_summer": { "milli_units": history.current_summer().milli_units() },
            "twenty_year_mean": { "milli_units": history.twenty_year_mean().milli_units() },
            "drought_summers": history.drought_summers(),
            "wet_summers": history.wet_summers(),
        })
    };
    match profile {
        DroughtProfile::Reconstructed(value) => json!({ "Reconstructed": history(value) }),
        DroughtProfile::Inferred(value) => json!({ "Inferred": history(value) }),
    }
}

fn encode_religious_status(status: SettlementReligiousStatus) -> Value {
    let religion = |value| {
        enum_unit(match value {
            adventuresim_world_schema::OfficialReligion::RomanCatholic => "RomanCatholic",
            adventuresim_world_schema::OfficialReligion::Lutheran => "Lutheran",
            adventuresim_world_schema::OfficialReligion::Reformed => "Reformed",
            adventuresim_world_schema::OfficialReligion::Anglican => "Anglican",
            adventuresim_world_schema::OfficialReligion::EasternOrthodox => "EasternOrthodox",
            adventuresim_world_schema::OfficialReligion::Islamic => "Islamic",
            adventuresim_world_schema::OfficialReligion::Judaism => "Judaism",
        })
    };
    let arrangement = |value| match value {
        WesternChristianArrangement::CatholicLutheran { church } => json!({
            "CatholicLutheran": { "church": enum_unit(match church {
                adventuresim_world_schema::CatholicLutheranChurch::RomanCatholic => "RomanCatholic",
                adventuresim_world_schema::CatholicLutheranChurch::Lutheran => "Lutheran",
            }) }
        }),
        WesternChristianArrangement::CatholicReformed { church } => json!({
            "CatholicReformed": { "church": enum_unit(match church {
                adventuresim_world_schema::CatholicReformedChurch::RomanCatholic => "RomanCatholic",
                adventuresim_world_schema::CatholicReformedChurch::Reformed => "Reformed",
            }) }
        }),
        WesternChristianArrangement::LutheranReformed { church } => json!({
            "LutheranReformed": { "church": enum_unit(match church {
                adventuresim_world_schema::LutheranReformedChurch::Lutheran => "Lutheran",
                adventuresim_world_schema::LutheranReformedChurch::Reformed => "Reformed",
            }) }
        }),
    };
    match status {
        SettlementReligiousStatus::Established { religion: value } => {
            json!({ "Established": { "religion": religion(value) } })
        }
        SettlementReligiousStatus::Parity { arrangement: value } => {
            json!({ "Parity": { "arrangement": arrangement(value) } })
        }
        SettlementReligiousStatus::MultiConfessional { arrangement: value } => {
            json!({ "MultiConfessional": { "arrangement": arrangement(value) } })
        }
        SettlementReligiousStatus::LocallyDetermined { church } => {
            json!({ "LocallyDetermined": { "church": religion(church) } })
        }
    }
}

fn encode_geology(profile: &SurfaceGeology) -> Value {
    let setting = |setting: &adventuresim_world_schema::GeologicSetting| {
        json!({
            "lithology": match setting.lithology {
                GeologicLithologyEvidence::Mapped(value) => json!({ "Mapped": encode_lithology(value) }),
                GeologicLithologyEvidence::Inferred(value) => json!({ "Inferred": encode_lithology(value) }),
            },
            "age": match setting.age {
                GeologicAgeEvidence::Mapped(age) => json!({ "Mapped": enum_unit(geologic_era_name(age)) }),
                GeologicAgeEvidence::Inferred(age) => json!({ "Inferred": enum_unit(geologic_era_name(age)) }),
            },
        })
    };
    match profile {
        SurfaceGeology::Mapped(mapped) => json!({ "Mapped": {
            "unit": { "value": mapped.unit.as_str() },
            "setting": setting(&mapped.setting),
        }}),
        SurfaceGeology::Inferred(inferred) => json!({ "Inferred": {
            "lithology": encode_lithology(inferred.lithology),
            "age": enum_unit(geologic_era_name(inferred.age)),
        }}),
    }
}

fn encode_lithology(lithology: SurfaceLithology) -> Value {
    let named = |variant: &str, name: &'static str| json!({ (variant): enum_unit(name) });
    match lithology {
        SurfaceLithology::Unconsolidated(value) => named(
            "Unconsolidated",
            match value {
                UnconsolidatedDeposit::Clay => "Clay",
                UnconsolidatedDeposit::Silt => "Silt",
                UnconsolidatedDeposit::Sand => "Sand",
                UnconsolidatedDeposit::Gravel => "Gravel",
                UnconsolidatedDeposit::Till => "Till",
                UnconsolidatedDeposit::Peat => "Peat",
                UnconsolidatedDeposit::Alluvium => "Alluvium",
                UnconsolidatedDeposit::Loess => "Loess",
                UnconsolidatedDeposit::VolcanicAsh => "VolcanicAsh",
                UnconsolidatedDeposit::MixedSediment => "MixedSediment",
            },
        ),
        SurfaceLithology::Sedimentary(value) => named(
            "Sedimentary",
            match value {
                SedimentaryRock::Limestone => "Limestone",
                SedimentaryRock::Dolostone => "Dolostone",
                SedimentaryRock::Chalk => "Chalk",
                SedimentaryRock::Marl => "Marl",
                SedimentaryRock::Sandstone => "Sandstone",
                SedimentaryRock::Siltstone => "Siltstone",
                SedimentaryRock::Mudstone => "Mudstone",
                SedimentaryRock::Shale => "Shale",
                SedimentaryRock::Conglomerate => "Conglomerate",
                SedimentaryRock::Evaporite => "Evaporite",
                SedimentaryRock::Coal => "Coal",
                SedimentaryRock::Chert => "Chert",
                SedimentaryRock::MixedSedimentary => "MixedSedimentary",
            },
        ),
        SurfaceLithology::Igneous(value) => named(
            "Igneous",
            match value {
                IgneousRock::Granite => "Granite",
                IgneousRock::Granitoid => "Granitoid",
                IgneousRock::Diorite => "Diorite",
                IgneousRock::Gabbro => "Gabbro",
                IgneousRock::Basalt => "Basalt",
                IgneousRock::Andesite => "Andesite",
                IgneousRock::Rhyolite => "Rhyolite",
                IgneousRock::Tuff => "Tuff",
                IgneousRock::OtherPlutonic => "OtherPlutonic",
                IgneousRock::OtherVolcanic => "OtherVolcanic",
                IgneousRock::OtherIgneous => "OtherIgneous",
            },
        ),
        SurfaceLithology::Metamorphic(value) => named(
            "Metamorphic",
            match value {
                MetamorphicRock::Slate => "Slate",
                MetamorphicRock::Schist => "Schist",
                MetamorphicRock::Gneiss => "Gneiss",
                MetamorphicRock::Quartzite => "Quartzite",
                MetamorphicRock::Marble => "Marble",
                MetamorphicRock::Phyllite => "Phyllite",
                MetamorphicRock::Amphibolite => "Amphibolite",
                MetamorphicRock::OtherMetamorphic => "OtherMetamorphic",
            },
        ),
        SurfaceLithology::Mixed(value) => named(
            "Mixed",
            match value {
                MixedLithology::Breccia => "Breccia",
                MixedLithology::Melange => "Melange",
                MixedLithology::MixedRock => "MixedRock",
            },
        ),
    }
}

const fn geologic_era_name(era: GeologicEra) -> &'static str {
    match era {
        GeologicEra::Quaternary => "Quaternary",
        GeologicEra::Neogene => "Neogene",
        GeologicEra::Paleogene => "Paleogene",
        GeologicEra::Cenozoic => "Cenozoic",
        GeologicEra::Cretaceous => "Cretaceous",
        GeologicEra::Jurassic => "Jurassic",
        GeologicEra::Triassic => "Triassic",
        GeologicEra::Mesozoic => "Mesozoic",
        GeologicEra::Permian => "Permian",
        GeologicEra::Carboniferous => "Carboniferous",
        GeologicEra::Devonian => "Devonian",
        GeologicEra::Silurian => "Silurian",
        GeologicEra::Ordovician => "Ordovician",
        GeologicEra::Cambrian => "Cambrian",
        GeologicEra::Paleozoic => "Paleozoic",
        GeologicEra::Precambrian => "Precambrian",
        GeologicEra::Phanerozoic => "Phanerozoic",
    }
}

fn encode_soil(profile: &SoilProfile) -> Value {
    let properties = |properties: &adventuresim_world_schema::SoilProperties| {
        json!({
            "substrate": encode_soil_substrate(properties.substrate),
            "water_regime": enum_unit(match properties.water_regime { SoilWaterRegime::UsuallyDry => "UsuallyDry", SoilWaterRegime::SeasonallyWet => "SeasonallyWet", SoilWaterRegime::LongSeasonWet => "LongSeasonWet", SoilWaterRegime::PermanentlyWet => "PermanentlyWet" }),
            "agricultural_limitation": enum_unit(match properties.agricultural_limitation {
                AgriculturalLimitation::None => "None", AgriculturalLimitation::Gravelly => "Gravelly",
                AgriculturalLimitation::Stony => "Stony", AgriculturalLimitation::ShallowRock => "ShallowRock",
                AgriculturalLimitation::Concretionary => "Concretionary", AgriculturalLimitation::CementedCalcic => "CementedCalcic",
                AgriculturalLimitation::Saline => "Saline", AgriculturalLimitation::Sodic => "Sodic",
                AgriculturalLimitation::GlacierOrSnow => "GlacierOrSnow", AgriculturalLimitation::Disturbed => "Disturbed",
                AgriculturalLimitation::Fragic => "Fragic", AgriculturalLimitation::Drained => "Drained",
                AgriculturalLimitation::Flooded => "Flooded", AgriculturalLimitation::Eroded => "Eroded",
                AgriculturalLimitation::ShallowWaterTable => "ShallowWaterTable",
            }),
        })
    };
    json!({
        "wrb_group": enum_unit(wrb_name(profile.wrb_group)),
        "parent_material": encode_lithology(profile.parent_material),
        "properties": properties(&profile.properties),
        "acidity": enum_unit(match profile.acidity { SoilAcidity::StronglyAcid => "StronglyAcid", SoilAcidity::Acid => "Acid", SoilAcidity::Neutral => "Neutral", SoilAcidity::Alkaline => "Alkaline" }),
        "cation_exchange_capacity": enum_unit(match profile.cation_exchange_capacity { CationExchangeCapacity::VeryLow => "VeryLow", CationExchangeCapacity::Low => "Low", CationExchangeCapacity::Medium => "Medium", CationExchangeCapacity::High => "High", CationExchangeCapacity::VeryHigh => "VeryHigh" }),
        "fertility": enum_unit(match profile.fertility { SoilFertility::VeryLow => "VeryLow", SoilFertility::Low => "Low", SoilFertility::Medium => "Medium", SoilFertility::High => "High", SoilFertility::VeryHigh => "VeryHigh" }),
        "confidence": { "value": profile.confidence.get() },
        "evidence": enum_unit(match profile.evidence { SoilEvidence::SoilGridsPrediction => "SoilGridsPrediction", SoilEvidence::DeterministicInference => "DeterministicInference" }),
    })
}

fn encode_soil_substrate(substrate: SoilSubstrate) -> Value {
    let depth = |value| {
        enum_unit(match value {
            SoilDepth::Shallow => "Shallow",
            SoilDepth::Moderate => "Moderate",
            SoilDepth::Deep => "Deep",
            SoilDepth::VeryDeep => "VeryDeep",
        })
    };
    let water = |value| {
        enum_unit(match value {
            AvailableWaterCapacity::VeryLow => "VeryLow",
            AvailableWaterCapacity::Low => "Low",
            AvailableWaterCapacity::Medium => "Medium",
            AvailableWaterCapacity::High => "High",
            AvailableWaterCapacity::VeryHigh => "VeryHigh",
        })
    };
    let carbon = |value| {
        enum_unit(match value {
            TopsoilOrganicCarbon::VeryLow => "VeryLow",
            TopsoilOrganicCarbon::Low => "Low",
            TopsoilOrganicCarbon::Medium => "Medium",
            TopsoilOrganicCarbon::High => "High",
        })
    };
    let texture = |value| {
        enum_unit(match value {
            MineralSoilTexture::Coarse => "Coarse",
            MineralSoilTexture::Medium => "Medium",
            MineralSoilTexture::MediumFine => "MediumFine",
            MineralSoilTexture::Fine => "Fine",
            MineralSoilTexture::VeryFine => "VeryFine",
        })
    };
    match substrate {
        SoilSubstrate::Mineral(soil) => json!({ "Mineral": {
            "texture": texture(soil.texture), "depth": depth(soil.depth), "available_water": water(soil.available_water), "organic_carbon": carbon(soil.organic_carbon), "stones": { "percent": soil.stones.percent() }
        }}),
        SoilSubstrate::Organic(soil) => json!({ "Organic": {
            "depth": depth(soil.depth), "available_water": water(soil.available_water), "stones": { "percent": soil.stones.percent() }
        }}),
        SoilSubstrate::RockOutcrop(soil) => {
            json!({ "RockOutcrop": { "stones": { "percent": soil.stones.percent() } }})
        }
        SoilSubstrate::OtherNonTextured(soil) => json!({ "OtherNonTextured": {
            "depth": depth(soil.depth), "available_water": water(soil.available_water), "organic_carbon": carbon(soil.organic_carbon), "stones": { "percent": soil.stones.percent() }
        }}),
    }
}

fn wrb_name(group: WrbReferenceGroup) -> &'static str {
    use WrbReferenceGroup as W;
    match group {
        W::Albeluvisol => "Albeluvisol",
        W::Acrisol => "Acrisol",
        W::Alisol => "Alisol",
        W::Andosol => "Andosol",
        W::Arenosol => "Arenosol",
        W::Anthrosol => "Anthrosol",
        W::Chernozem => "Chernozem",
        W::Calcisol => "Calcisol",
        W::Cambisol => "Cambisol",
        W::Cryosol => "Cryosol",
        W::Durisol => "Durisol",
        W::Fluvisol => "Fluvisol",
        W::Ferralsol => "Ferralsol",
        W::Gleysol => "Gleysol",
        W::Gypsisol => "Gypsisol",
        W::Histosol => "Histosol",
        W::Kastanozem => "Kastanozem",
        W::Leptosol => "Leptosol",
        W::Luvisol => "Luvisol",
        W::Lixisol => "Lixisol",
        W::Nitisol => "Nitisol",
        W::Phaeozem => "Phaeozem",
        W::Planosol => "Planosol",
        W::Plinthosol => "Plinthosol",
        W::Podzol => "Podzol",
        W::Regosol => "Regosol",
        W::Solonchak => "Solonchak",
        W::Solonetz => "Solonetz",
        W::Stagnosol => "Stagnosol",
        W::Umbrisol => "Umbrisol",
        W::Vertisol => "Vertisol",
    }
}

fn enum_unit(name: &str) -> Value {
    json!({ (name): [] })
}

fn encode_potential_vegetation(vegetation: &PotentialVegetation) -> Value {
    match vegetation {
        PotentialVegetation::Posterior(values) => json!({
            "Posterior": {
                "woodland_and_forest": { "basis_points": values.woodland_and_forest.get() },
                "heathland_and_shrub": { "basis_points": values.heathland_and_shrub.get() },
                "grassland": { "basis_points": values.grassland.get() },
                "sparsely_vegetated_areas": { "basis_points": values.sparsely_vegetated_areas.get() },
                "wetlands": { "basis_points": values.wetlands.get() },
                "marine_inlets_and_transitional_waters": { "basis_points": values.marine_inlets_and_transitional_waters.get() },
            }
        }),
        PotentialVegetation::Categorical(class) => {
            json!({ "Categorical": encode_potential_class(*class) })
        }
        PotentialVegetation::Inferred(class) => {
            json!({ "Inferred": encode_potential_class(*class) })
        }
    }
}

fn encode_historical_vegetation(vegetation: HistoricalVegetation) -> Value {
    use adventuresim_world_schema::{
        DerivedHistoricalVegetationMethod as D, DirectHistoricalVegetationMethod as R,
        FallbackHistoricalVegetationMethod as F,
    };
    let direct_cover = |value| match value {
        DirectHistoricalVegetationCover::BuiltSettlement(v) => {
            json!({ "BuiltSettlement": { "built_fraction": v.built_fraction } })
        }
        DirectHistoricalVegetationCover::Cropland(v) => {
            json!({ "Cropland": { "cultivated_fraction": v.cultivated_fraction } })
        }
        DirectHistoricalVegetationCover::Pasture(v) => {
            json!({ "Pasture": { "grazing_fraction": v.grazing_fraction } })
        }
    };
    let woodland = |v: adventuresim_world_schema::HistoricalWoodland| json!({ "Woodland": { "canopy": v.canopy, "dominant": enum_unit(match v.dominant { DominantLeafType::Broadleaf => "Broadleaf", DominantLeafType::Coniferous => "Coniferous", DominantLeafType::Mixed => "Mixed" }) } });
    let derived_cover = |value| match value {
        DerivedHistoricalVegetationCover::Woodland(v) => woodland(v),
        DerivedHistoricalVegetationCover::HeathAndShrub => enum_unit("HeathAndShrub"),
        DerivedHistoricalVegetationCover::Grassland => enum_unit("Grassland"),
        DerivedHistoricalVegetationCover::Sparse => enum_unit("Sparse"),
        DerivedHistoricalVegetationCover::Wetland(v) => {
            json!({ "Wetland": { "water_regime": enum_unit(match v.water_regime { SoilWaterRegime::UsuallyDry => "UsuallyDry", SoilWaterRegime::SeasonallyWet => "SeasonallyWet", SoilWaterRegime::LongSeasonWet => "LongSeasonWet", SoilWaterRegime::PermanentlyWet => "PermanentlyWet" }) } })
        }
        DerivedHistoricalVegetationCover::TransitionalWater => enum_unit("TransitionalWater"),
    };
    let fallback_cover = |value| match value {
        FallbackHistoricalVegetationCover::Woodland(v) => woodland(v),
        FallbackHistoricalVegetationCover::HeathAndShrub => enum_unit("HeathAndShrub"),
        FallbackHistoricalVegetationCover::Grassland => enum_unit("Grassland"),
        FallbackHistoricalVegetationCover::Sparse => enum_unit("Sparse"),
    };
    match vegetation {
        HistoricalVegetation::Direct(v) => {
            json!({ "Direct": { "cover": direct_cover(v.cover), "method": enum_unit(match v.method { R::Luh1DominantLandUse => "Luh1DominantLandUse" }) } })
        }
        HistoricalVegetation::Derived(v) => {
            json!({ "Derived": { "cover": derived_cover(v.cover), "method": enum_unit(match v.method { D::MultiSourceRulesV4 => "MultiSourceRulesV4", D::MultiSourceRulesV4TieBreak => "MultiSourceRulesV4TieBreak" }) } })
        }
        HistoricalVegetation::Fallback(v) => {
            json!({ "Fallback": { "cover": fallback_cover(v.cover), "method": enum_unit(match v.method { F::PotentialEnvelopeV4 => "PotentialEnvelopeV4" }) } })
        }
    }
}

fn encode_tree_species(profile: &TreeSpeciesProfile) -> Value {
    match profile {
        TreeSpeciesProfile::Modeled(profile) => json!({
            "Modeled": {
                "candidates": profile.candidates().iter().map(|candidate| json!({
                    "species": { "scientific_name": candidate.species.as_str() },
                    "suitability": { "score": candidate.suitability.score() },
                    "native_range": match candidate.native_range {
                        NativeRangeEvidence::WithinNativeRange => json!({ "WithinNativeRange": [] }),
                        NativeRangeEvidence::OutsideNativeRange => json!({ "OutsideNativeRange": [] }),
                    },
                })).collect::<Vec<_>>()
            }
        }),
        TreeSpeciesProfile::Inferred(profile) => json!({
            "Inferred": {
                "species": profile.species().iter().map(|species| {
                    json!({ "scientific_name": species.as_str() })
                }).collect::<Vec<_>>()
            }
        }),
    }
}

fn encode_potential_class(class: PotentialVegetationClass) -> Value {
    let name = match class {
        PotentialVegetationClass::WoodlandAndForest => "WoodlandAndForest",
        PotentialVegetationClass::HeathlandAndShrub => "HeathlandAndShrub",
        PotentialVegetationClass::Grassland => "Grassland",
        PotentialVegetationClass::SparselyVegetatedAreas => "SparselyVegetatedAreas",
        PotentialVegetationClass::Wetlands => "Wetlands",
        PotentialVegetationClass::MarineInletsAndTransitionalWaters => {
            "MarineInletsAndTransitionalWaters"
        }
    };
    json!({ (name): [] })
}

fn encode_forest_cover(cover: ForestCover) -> Value {
    match cover {
        ForestCover::Open => json!({ "Open": [] }),
        ForestCover::Wooded(woodland) => json!({
            "Wooded": {
                "density": woodland.density,
                "dominant": match woodland.dominant {
                    DominantLeafType::Broadleaf => json!({ "Broadleaf": [] }),
                    DominantLeafType::Coniferous => json!({ "Coniferous": [] }),
                    DominantLeafType::Mixed => json!({ "Mixed": [] }),
                },
            }
        }),
    }
}

fn encode_optional<T: serde::Serialize>(value: &Option<T>) -> Value {
    match value {
        Some(value) => json!({ "some": value }),
        None => json!({ "none": [] }),
    }
}

fn encode_settlement_alias(alias: &SettlementAliasImport) -> Result<Value> {
    Ok(json!({
        "id": alias.id,
        "settlement_id": alias.settlement_id,
        "name": alias.name,
        "prefix": encode_optional(&alias.prefix),
        "language": encode_optional(&alias.language.as_ref().map(|code| code.as_str())),
    }))
}

fn encode_settlement_description(description: &SettlementDescriptionImport) -> Result<Value> {
    let kind = match description.kind {
        SettlementDescriptionKind::Settlement => json!({ "Settlement": [] }),
        SettlementDescriptionKind::City => json!({ "City": [] }),
    };
    Ok(json!({
        "id": description.id,
        "settlement_id": description.settlement_id,
        "kind": kind,
        "language": encode_optional(&description.language.as_ref().map(|code| code.as_str())),
        "body": description.body,
    }))
}

fn call_reducer(args: &Args, reducer: &str, arguments: &[Value]) -> Result<()> {
    let status = Command::new(&args.spacetime)
        .args(["call", "--server", &args.server, &args.database, reducer])
        .args(arguments.iter().map(Value::to_string))
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Spacetime {
            reducer: reducer.into(),
            status: status.to_string(),
        })
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("world importer crate is in the workspace crates directory")
        .into()
}

fn default_viabundus_directory() -> PathBuf {
    repository_root().join("viabundus")
}

fn default_elevation_directory() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/elevation")
}

fn default_land_use_directory() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/historical-land-use")
}

fn default_forest_cover_directory() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/forest-cover")
}

fn default_potential_vegetation_directory() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/jung-pnv")
}

fn default_tree_species_archive() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/tree-species/EU-Trees4F_ens-clim.zip")
}

fn default_soilgrids_directory() -> PathBuf {
    repository_root().join("target/world-data-sources/prepared/soilgrids")
}

fn default_geology_geopackage() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/geology/GeologicUnitView.gpkg")
}

fn default_religion_regions() -> PathBuf {
    repository_root().join("assets/world-data/ieg-religion-1544.csv")
}

fn default_drought_netcdf() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/climate/owda.nc")
}

fn default_hydrology_directory() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/hydrology")
}

fn default_output(year: i32) -> PathBuf {
    repository_root().join(format!("target/world-{year}.json"))
}

#[cfg(test)]
mod tests {
    use adventuresim_world_schema::{
        AgriculturalLimitation, AvailableWaterCapacity, CURRENT_INFERENCE_RULES_VERSION,
        CanopyDensity, CationExchangeCapacity, CompiledWorld, DominantLeafType, DroughtHistory,
        DroughtProfile, EdgeEndpoint, ElevationMeters, FallbackHistoricalVegetationCover,
        ForestCover, GeologicAgeEvidence, GeologicEra, GeologicLithologyEvidence, GeologicSetting,
        GeologicUnitId, GridCellSizeMeters, HabitatSuitability, HistoricalVegetation, IgneousRock,
        InferredGeologicSetting, InferredTreeSpeciesProfile, LandRoute, LandUseFraction,
        LandUseProfile, LanguageCode, MappedSurfaceGeology, MineralSoil, MineralSoilTexture,
        ModeledTreeSpecies, ModeledTreeSpeciesProfile, NativeRangeEvidence, OfficialReligion,
        PalmerDroughtSeverityIndex, PotentialVegetation, PotentialVegetationClass,
        SettlementDescriptionImport, SettlementDescriptionKind, SettlementImport,
        SettlementReligiousStatus, SoilAcidity, SoilBasisPoints, SoilDepth, SoilEvidence,
        SoilFertility, SoilProfile, SoilProperties, SoilSubstrate, SoilWaterRegime, SourceAccess,
        SourceContentIdentity, SourceLicense, SourcePreparation, SourceProvenance, SourceRelease,
        SourceSpatialCoverage, SourceTemporalCoverage, SpatialGridSpec, StoneContentPercent,
        SurfaceGeology, SurfaceLithology, TopsoilOrganicCarbon, TravelEdgeImport, TravelRoute,
        TreeSpeciesId, TreeSpeciesProfile, UnconsolidatedDeposit, WORLD_SCHEMA_VERSION, Woodland,
        WorldBuildReport, WorldMetadata, WrbReferenceGroup,
    };
    use clap::Parser;

    use super::{
        Args, MAX_REDUCER_ARGUMENT_CHARS, default_output, encode_settlement,
        encode_settlement_description, encode_travel_edge, manifest_audit_markdown,
        serialize_batches,
    };

    #[test]
    fn cli_validates_grid_cell_size_at_the_trust_boundary() {
        let args =
            Args::try_parse_from(["world-import", "--grid-cell-size-meters", "250"]).unwrap();
        assert_eq!(args.grid_cell_size_meters.get(), 250);
        for value in ["0", "251", "100001", "not-a-number"] {
            assert!(
                Args::try_parse_from(["world-import", "--grid-cell-size-meters", value]).is_err()
            );
        }
    }

    #[test]
    fn grid_spec_is_part_of_serialized_world_artifact_identity() {
        let world = |cell_size| CompiledWorld {
            metadata: WorldMetadata {
                schema_version: WORLD_SCHEMA_VERSION,
                inference_rules_version: CURRENT_INFERENCE_RULES_VERSION,
                spatial_grid: SpatialGridSpec::new(GridCellSizeMeters::new(cell_size).unwrap()),
                world_year: 1544,
                manifest_digest: "0".repeat(64),
                sources: Vec::new(),
                road_types: Vec::new(),
            },
            nodes: Vec::new(),
            edges: Vec::new(),
            settlements: Vec::new(),
            settlement_aliases: Vec::new(),
            settlement_descriptions: Vec::new(),
            report: WorldBuildReport::default(),
        };
        let default_bytes = serde_json::to_vec(&world(1_000)).unwrap();
        let fine_bytes = serde_json::to_vec(&world(250)).unwrap();
        assert_ne!(default_bytes, fine_bytes);
        assert_ne!(blake3::hash(&default_bytes), blake3::hash(&fine_bytes));
    }

    #[test]
    fn import_audit_includes_every_notice_and_refuses_truncation() {
        let source = SourceProvenance {
            id: "fixture".into(),
            name: "Fixture".into(),
            release: SourceRelease::Immutable {
                version: "1".into(),
                released: "2020".into(),
            },
            canonical_url: "https://example.invalid/source".into(),
            doi: Some("10.1/fixture".into()),
            license: SourceLicense::CcBy4_0,
            required_notices: vec!["first exact notice".into(), "second exact notice".into()],
            access: SourceAccess::AnonymousDownload,
            spatial: SourceSpatialCoverage::NotApplicable,
            temporal: SourceTemporalCoverage::Timeless,
            preparation: SourcePreparation {
                recipe: "fixture".into(),
                version: 1,
            },
            content_identity: SourceContentIdentity::RawSha256 {
                sha256: "a".repeat(64),
            },
            notes_markdown: "Fixture notes.".into(),
        };
        let world = CompiledWorld {
            metadata: WorldMetadata {
                schema_version: WORLD_SCHEMA_VERSION,
                inference_rules_version: CURRENT_INFERENCE_RULES_VERSION,
                spatial_grid: SpatialGridSpec::default(),
                world_year: 1544,
                manifest_digest: "0".repeat(64),
                sources: vec![source.clone()],
                road_types: vec![],
            },
            nodes: vec![],
            edges: vec![],
            settlements: vec![],
            settlement_aliases: vec![],
            settlement_descriptions: vec![],
            report: WorldBuildReport::default(),
        };
        let audit = manifest_audit_markdown(&world).unwrap();
        for text in [
            "fixture",
            "Fixture",
            "immutable",
            "cc-by4-0",
            "reproducible",
            "10.1/fixture",
            "first exact notice",
            "second exact notice",
            "Fixture notes.",
        ] {
            assert!(audit.contains(text));
        }
        for mutate in [
            |source: &mut SourceProvenance| source.access = SourceAccess::ManualPreparation,
            |source: &mut SourceProvenance| {
                source.spatial = SourceSpatialCoverage::Geographic {
                    crs: "EPSG:3035".into(),
                    resolution: "1 km".into(),
                    coverage: "Europe".into(),
                }
            },
            |source: &mut SourceProvenance| source.temporal = SourceTemporalCoverage::Year(1544),
            |source: &mut SourceProvenance| source.preparation.version = 2,
        ] {
            let mut changed = world.clone();
            mutate(&mut changed.metadata.sources[0]);
            assert_ne!(audit, manifest_audit_markdown(&changed).unwrap());
        }
        let mut oversized = world.clone();
        oversized.metadata.sources[0].required_notices = vec!["\\".repeat(13_000)];
        assert!(
            manifest_audit_markdown(&oversized)
                .unwrap_err()
                .to_string()
                .contains("refusing to omit or truncate")
        );
    }

    #[test]
    fn encodes_shared_enum_for_spacetimedb_sats_json() {
        let edge = TravelEdgeImport {
            id: 1,
            from_node_id: 2,
            to_node_id: 3,
            route: TravelRoute::Land(LandRoute {
                bridge: Some(EdgeEndpoint::To),
                water_crossings: Vec::new(),
            }),
            toll: Some(EdgeEndpoint::From),
            length_m: 4,
            slope_multiplier: 1.0,
            terrain: adventuresim_world_schema::RouteTerrain::stage_placeholder(),
            certainty: 1,
            section: String::new(),
            sources: "- Test source.".into(),
        };
        let batches = serialize_batches(&[edge], 100, encode_travel_edge).unwrap();
        assert_eq!(
            batches[0][0]["route"],
            serde_json::json!({ "Land": { "bridge": { "some": { "To": [] } }, "water_crossings": [] } })
        );
        assert_eq!(
            batches[0][0]["toll"],
            serde_json::json!({ "some": { "From": [] } })
        );
        assert_eq!(batches[0][0]["sources"], "- Test source.");
        assert_eq!(
            batches[0][0]["terrain"]["class"],
            serde_json::json!({ "Flat": [] })
        );
        assert_eq!(
            batches[0][0]["terrain"]["encounter_tags"][0],
            serde_json::json!({ "Flat": [] })
        );
    }

    #[test]
    fn encodes_all_forest_variants_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["sources"],
            "- Test source."
        );
        assert_eq!(
            encode_settlement(&settlement).unwrap()["forest_cover"],
            serde_json::json!({ "Open": [] })
        );

        for (dominant, name) in [
            (DominantLeafType::Broadleaf, "Broadleaf"),
            (DominantLeafType::Coniferous, "Coniferous"),
            (DominantLeafType::Mixed, "Mixed"),
        ] {
            settlement.forest_cover = ForestCover::Wooded(Woodland {
                density: CanopyDensity::new(40).unwrap(),
                dominant,
            });
            assert_eq!(
                encode_settlement(&settlement).unwrap()["forest_cover"],
                serde_json::json!({
                    "Wooded": {
                        "density": { "percent": 40 },
                        "dominant": { (name): [] },
                    }
                })
            );
        }
    }

    #[test]
    fn encodes_categorical_and_inferred_vegetation_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["potential_vegetation"],
            serde_json::json!({
                "Inferred": { "WoodlandAndForest": [] }
            })
        );
        settlement.potential_vegetation =
            PotentialVegetation::Categorical(PotentialVegetationClass::Grassland);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["potential_vegetation"],
            serde_json::json!({
                "Categorical": { "Grassland": [] }
            })
        );
    }

    #[test]
    fn encodes_modeled_and_inferred_tree_profiles_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["tree_species"],
            serde_json::json!({
                "Inferred": {
                    "species": [{ "scientific_name": "Quercus_robur" }]
                }
            })
        );
        settlement.tree_species = TreeSpeciesProfile::Modeled(
            ModeledTreeSpeciesProfile::new(vec![ModeledTreeSpecies {
                species: TreeSpeciesId::new("Fagus_sylvatica").unwrap(),
                suitability: HabitatSuitability::new(875).unwrap(),
                native_range: NativeRangeEvidence::WithinNativeRange,
            }])
            .unwrap(),
        );
        assert_eq!(
            encode_settlement(&settlement).unwrap()["tree_species"],
            serde_json::json!({
                "Modeled": {
                    "candidates": [{
                        "species": { "scientific_name": "Fagus_sylvatica" },
                        "suitability": { "score": 875 },
                        "native_range": { "WithinNativeRange": [] }
                    }]
                }
            })
        );
    }

    #[test]
    fn encodes_final_soil_profile_for_spacetimedb_sats_json() {
        let settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["soil"]["properties"]["substrate"]["Mineral"]["texture"],
            serde_json::json!({ "Medium": [] })
        );
        assert_eq!(
            encode_settlement(&settlement).unwrap()["soil"]["parent_material"],
            serde_json::json!({ "Unconsolidated": { "Alluvium": [] } })
        );
        assert_eq!(
            encode_settlement(&settlement).unwrap()["soil"]["wrb_group"],
            serde_json::json!({ "Cambisol": [] })
        );
    }

    #[test]
    fn encodes_mapped_and_inferred_geology_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["geology"],
            serde_json::json!({ "Inferred": {
                "lithology": { "Unconsolidated": { "Alluvium": [] } },
                "age": { "Quaternary": [] },
            }})
        );
        settlement.geology = SurfaceGeology::Mapped(MappedSurfaceGeology {
            unit: GeologicUnitId::new("FR-BRGM.1953.72852").unwrap(),
            setting: GeologicSetting {
                lithology: GeologicLithologyEvidence::Mapped(SurfaceLithology::Igneous(
                    IgneousRock::Granite,
                )),
                age: GeologicAgeEvidence::Mapped(GeologicEra::Cambrian),
            },
        });
        assert_eq!(
            encode_settlement(&settlement).unwrap()["geology"]["Mapped"]["unit"],
            serde_json::json!({ "value": "FR-BRGM.1953.72852" })
        );
    }

    #[test]
    fn encodes_religious_status_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["religious_status"],
            serde_json::json!({
                "Established": { "religion": { "RomanCatholic": [] } }
            })
        );
        settlement.religious_status = SettlementReligiousStatus::MultiConfessional {
            arrangement: adventuresim_world_schema::WesternChristianArrangement::CatholicLutheran {
                church: adventuresim_world_schema::CatholicLutheranChurch::Lutheran,
            },
        };
        assert_eq!(
            encode_settlement(&settlement).unwrap()["religious_status"],
            serde_json::json!({
                "MultiConfessional": {
                    "arrangement": {
                        "CatholicLutheran": { "church": { "Lutheran": [] } }
                    }
                }
            })
        );
    }

    #[test]
    fn encodes_drought_profile_for_spacetimedb_sats_json() {
        let settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["drought"],
            serde_json::json!({
                "Inferred": {
                    "current_summer": { "milli_units": 0 },
                    "twenty_year_mean": { "milli_units": 0 },
                    "drought_summers": 0,
                    "wet_summers": 0,
                }
            })
        );
    }

    #[test]
    fn encodes_historical_vegetation_for_spacetimedb_sats_json() {
        let settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["historical_vegetation"],
            serde_json::json!({
                "Fallback": {
                    "cover": { "Woodland": {
                        "canopy": { "percent": 20 },
                        "dominant": { "Mixed": [] }
                    }},
                    "method": { "PotentialEnvelopeV4": [] }
                }
            })
        );
    }

    #[test]
    fn encodes_industries_for_spacetimedb_sats_json() {
        let settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["industries"],
            serde_json::json!({"outputs": [{"Fallback": {"CommonAggregate": []}}]})
        );
    }

    fn settlement(forest_cover: ForestCover) -> SettlementImport {
        SettlementImport {
            id: "test".into(),
            source_node_id: 1,
            name: "Test".into(),
            longitude: 0.0,
            latitude: 0.0,
            population_level: 1,
            population_estimate: 100,
            elevation: ElevationMeters::new(10).unwrap(),
            land_use: LandUseProfile::new(
                LandUseFraction::new(1_000).unwrap(),
                LandUseFraction::new(1_000).unwrap(),
                LandUseFraction::new(100).unwrap(),
                LandUseFraction::new(7_900).unwrap(),
            )
            .unwrap(),
            forest_cover,
            potential_vegetation: PotentialVegetation::Inferred(
                PotentialVegetationClass::WoodlandAndForest,
            ),
            historical_vegetation: HistoricalVegetation::Fallback(adventuresim_world_schema::FallbackHistoricalVegetation {
                cover: FallbackHistoricalVegetationCover::Woodland(adventuresim_world_schema::HistoricalWoodland {
                    canopy: CanopyDensity::new(20).unwrap(),
                    dominant: DominantLeafType::Mixed,
                }),
                method: adventuresim_world_schema::FallbackHistoricalVegetationMethod::PotentialEnvelopeV4,
            }),
            tree_species: TreeSpeciesProfile::Inferred(
                InferredTreeSpeciesProfile::new(vec![TreeSpeciesId::new("Quercus_robur").unwrap()])
                    .unwrap(),
            ),
            soil: SoilProfile {
                wrb_group: WrbReferenceGroup::Cambisol,
                parent_material: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                properties: SoilProperties {
                    substrate: SoilSubstrate::Mineral(MineralSoil {
                        texture: MineralSoilTexture::Medium,
                        depth: SoilDepth::Deep,
                        available_water: AvailableWaterCapacity::Medium,
                        organic_carbon: TopsoilOrganicCarbon::Medium,
                        stones: StoneContentPercent::new(10).unwrap(),
                    }),
                    water_regime: SoilWaterRegime::SeasonallyWet,
                    agricultural_limitation: AgriculturalLimitation::None,
                },
                acidity: SoilAcidity::Acid,
                cation_exchange_capacity: CationExchangeCapacity::Medium,
                fertility: SoilFertility::Medium,
                confidence: SoilBasisPoints::new(5_000).unwrap(),
                evidence: SoilEvidence::DeterministicInference,
            },
            geology: SurfaceGeology::Inferred(InferredGeologicSetting {
                lithology: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                age: GeologicEra::Quaternary,
            }),
            religious_status: SettlementReligiousStatus::Established {
                religion: OfficialReligion::RomanCatholic,
            },
            drought: DroughtProfile::Inferred(
                DroughtHistory::new(
                    PalmerDroughtSeverityIndex::new(0).unwrap(),
                    PalmerDroughtSeverityIndex::new(0).unwrap(),
                    0,
                    0,
                )
                .unwrap(),
            ),
            hydrology: adventuresim_world_schema::SettlementHydrology::default(),
            industries: adventuresim_world_schema::InferredIndustryProfile::new(vec![
                adventuresim_world_schema::IndustryEvidence::Fallback(
                    adventuresim_world_schema::FallbackIndustry::CommonAggregate,
                ),
            ]).unwrap(),
            scene_key: "village".into(),
            sources: "- Test source.".into(),
        }
    }

    #[test]
    fn batches_are_bounded_for_windows_process_arguments() {
        let rows = vec!["x".repeat(1_000); 100];
        let batches = serialize_batches(&rows, 100, |row| {
            serde_json::to_value(row).map_err(adventuresim_world_import::Error::from)
        })
        .unwrap();
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|batch| {
            serde_json::Value::Array(batch.clone())
                .to_string()
                .encode_utf16()
                .count()
                <= MAX_REDUCER_ARGUMENT_CHARS
        }));
    }

    #[test]
    fn rejects_a_single_row_that_exceeds_the_windows_command_line_bound() {
        let row = "😀".repeat(MAX_REDUCER_ARGUMENT_CHARS / 2);
        let error = serialize_batches(&[row], 100, |row| {
            serde_json::to_value(row).map_err(adventuresim_world_import::Error::from)
        })
        .unwrap_err();
        assert!(error.to_string().contains("UTF-16 code units"));
    }

    #[test]
    fn default_output_names_the_selected_world_year() {
        assert!(default_output(1600).ends_with("target/world-1600.json"));
    }

    #[test]
    fn encodes_description_domain_types_for_sats_json() {
        let description = SettlementDescriptionImport {
            id: "description-1".into(),
            settlement_id: "settlement-1".into(),
            kind: SettlementDescriptionKind::City,
            language: Some("deu".parse::<LanguageCode>().unwrap()),
            body: "Beschreibung".into(),
        };
        let encoded = encode_settlement_description(&description).unwrap();
        assert_eq!(encoded["kind"], serde_json::json!({ "City": [] }));
        assert_eq!(encoded["language"], serde_json::json!({ "some": "deu" }));
    }
}
