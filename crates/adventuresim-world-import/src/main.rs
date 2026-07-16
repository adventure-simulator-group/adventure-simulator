use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

use adventuresim_world_import::{Error, Result, WorldBuilder};
use adventuresim_world_schema::{
    AgriculturalLimitation, AvailableWaterCapacity, CompiledWorld, DominantLeafType, EdgeEndpoint,
    ForestCover, GeologicAgeEvidence, GeologicEra, GeologicLithologyEvidence, IgneousRock,
    MetamorphicRock, MineralSoilTexture, MixedLithology, NativeRangeEvidence, PotentialVegetation,
    PotentialVegetationFormation, SedimentaryRock, SettlementImport, SoilDepth, SoilProfile,
    SoilSubstrate, SoilWaterRegime, SurfaceGeology, SurfaceLithology, TopsoilOrganicCarbon,
    TravelEdgeImport, TravelRoute, TreeSpeciesProfile, UnconsolidatedDeposit, WorldNodeImport,
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
    #[arg(long, default_value_os_t = default_soil_directory())]
    soil_dir: PathBuf,
    #[arg(long, default_value_os_t = default_geology_geopackage())]
    geology_geopackage: PathBuf,
    #[arg(long, default_value_t = WORLD_YEAR)]
    year: i32,
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
    let world = WorldBuilder::new(args.year).build_from_sources(
        &args.viabundus_dir,
        &args.elevation_dir,
        &args.land_use_dir,
        &args.forest_cover_dir,
        &args.potential_vegetation_dir,
        &args.tree_species_archive,
        &args.soil_dir,
        &args.geology_geopackage,
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
    println!("Wrote compiled world to {}", output.display());
    if args.load {
        load_world(&world, &artifact_id, &args)?;
    }
    Ok(())
}

fn load_world(world: &CompiledWorld, artifact_id: &str, args: &Args) -> Result<()> {
    call_reducer(
        args,
        "begin_world_data_import",
        &[json!(world.metadata.schema_version), json!(artifact_id)],
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

fn serialize_batches<T>(
    rows: &[T],
    batch_size: usize,
    encode: fn(&T) -> Result<Value>,
) -> Result<Vec<Vec<Value>>> {
    let rows = rows.iter().map(encode).collect::<Result<Vec<_>>>()?;
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut characters = 2;
    for row in rows {
        let row_characters = row.to_string().chars().count() + usize::from(!batch.is_empty());
        if !batch.is_empty()
            && (batch.len() == batch_size
                || characters + row_characters > MAX_REDUCER_ARGUMENT_CHARS)
        {
            batches.push(std::mem::take(&mut batch));
            characters = 2;
        }
        characters += row_characters;
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
    }))
}

fn encode_travel_edge(edge: &TravelEdgeImport) -> Result<Value> {
    let route = match edge.route {
        TravelRoute::Land { bridge } => {
            json!({ "Land": encode_endpoint(bridge) })
        }
        TravelRoute::Ferry => json!({ "Ferry": [] }),
    };
    Ok(json!({
        "id": edge.id,
        "from_node_id": edge.from_node_id,
        "to_node_id": edge.to_node_id,
        "route": route,
        "toll": encode_endpoint(edge.toll),
        "length_m": edge.length_m,
        "slope_multiplier": edge.slope_multiplier,
        "certainty": edge.certainty,
        "section": edge.section,
    }))
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
        "tree_species": encode_tree_species(&settlement.tree_species),
        "soil": encode_soil(&settlement.soil),
        "geology": encode_geology(&settlement.geology),
        "scene_key": settlement.scene_key,
        "religion_id": settlement.religion_id,
    }))
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
        SurfaceGeology::Inferred(inferred) => json!({ "Inferred": setting(inferred) }),
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
        GeologicEra::Cretaceous => "Cretaceous",
        GeologicEra::Jurassic => "Jurassic",
        GeologicEra::Triassic => "Triassic",
        GeologicEra::Permian => "Permian",
        GeologicEra::Carboniferous => "Carboniferous",
        GeologicEra::Devonian => "Devonian",
        GeologicEra::Silurian => "Silurian",
        GeologicEra::Ordovician => "Ordovician",
        GeologicEra::Cambrian => "Cambrian",
        GeologicEra::Precambrian => "Precambrian",
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
    match profile {
        SoilProfile::Mapped(mapped) => json!({ "Mapped": {
            "mapping_unit": { "smu": mapped.mapping_unit.smu(), "dominant_stu": mapped.mapping_unit.dominant_stu(), "dominance_percent": mapped.mapping_unit.dominance_percent() },
            "wrb_group": enum_unit(wrb_name(mapped.wrb_group)),
            "parent_material": { "code": mapped.parent_material.as_str() },
            "properties": properties(&mapped.properties),
        }}),
        SoilProfile::Inferred(inferred) => json!({ "Inferred": properties(inferred) }),
    }
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
        W::Umbrisol => "Umbrisol",
        W::Vertisol => "Vertisol",
    }
}

fn enum_unit(name: &str) -> Value {
    json!({ (name): [] })
}

fn encode_potential_vegetation(vegetation: &PotentialVegetation) -> Value {
    match vegetation {
        PotentialVegetation::Mapped(mapped) => json!({
            "Mapped": {
                "unit": { "code": mapped.unit().as_str() },
                "formation": encode_potential_formation(mapped.formation()),
            }
        }),
        PotentialVegetation::Inferred(formation) => {
            json!({ "Inferred": encode_potential_formation(*formation) })
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

fn encode_potential_formation(formation: PotentialVegetationFormation) -> Value {
    let name = match formation {
        PotentialVegetationFormation::PolarDesertAndNival => "PolarDesertAndNival",
        PotentialVegetationFormation::TundraAndAlpine => "TundraAndAlpine",
        PotentialVegetationFormation::OpenWoodlandAndSubalpine => "OpenWoodlandAndSubalpine",
        PotentialVegetationFormation::ConiferousAndMixedForest => "ConiferousAndMixedForest",
        PotentialVegetationFormation::Heath => "Heath",
        PotentialVegetationFormation::DeciduousAndMixedForest => "DeciduousAndMixedForest",
        PotentialVegetationFormation::ThermophilousBroadleafForest => {
            "ThermophilousBroadleafForest"
        }
        PotentialVegetationFormation::HygroThermophilousBroadleafForest => {
            "HygroThermophilousBroadleafForest"
        }
        PotentialVegetationFormation::MediterraneanSclerophyll => "MediterraneanSclerophyll",
        PotentialVegetationFormation::XerophyticConiferAndScrub => "XerophyticConiferAndScrub",
        PotentialVegetationFormation::ForestSteppe => "ForestSteppe",
        PotentialVegetationFormation::Steppe => "Steppe",
        PotentialVegetationFormation::Oroxerophytic => "Oroxerophytic",
        PotentialVegetationFormation::Desert => "Desert",
        PotentialVegetationFormation::CoastalAndHalophytic => "CoastalAndHalophytic",
        PotentialVegetationFormation::AquaticAndReed => "AquaticAndReed",
        PotentialVegetationFormation::Mire => "Mire",
        PotentialVegetationFormation::SwampAndFenForest => "SwampAndFenForest",
        PotentialVegetationFormation::FloodplainAndWetland => "FloodplainAndWetland",
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
    repository_root().join("target/world-data-sources/raw/potential-vegetation/Maps")
}

fn default_tree_species_archive() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/tree-species/EU-Trees4F_ens-clim.zip")
}

fn default_soil_directory() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/soil/soilDB_shapefiles_and_attributes")
}

fn default_geology_geopackage() -> PathBuf {
    repository_root().join("target/world-data-sources/raw/geology/GeologicUnitView.gpkg")
}

fn default_output(year: i32) -> PathBuf {
    repository_root().join(format!("target/world-{year}.json"))
}

#[cfg(test)]
mod tests {
    use adventuresim_world_schema::{
        AgriculturalLimitation, AvailableWaterCapacity, CanopyDensity, DominantLeafType,
        EdgeEndpoint, ElevationMeters, EuroVegMapUnitCode, ForestCover, GeologicAgeEvidence,
        GeologicEra, GeologicLithologyEvidence, GeologicSetting, GeologicUnitId,
        HabitatSuitability, IgneousRock, InferredTreeSpeciesProfile, LandUseFraction,
        LandUseProfile, MappedPotentialVegetation, MappedSoilProfile, MappedSurfaceGeology,
        MineralSoil, MineralSoilTexture, ModeledTreeSpecies, ModeledTreeSpeciesProfile,
        NativeRangeEvidence, ParentMaterialCode, PotentialVegetation, PotentialVegetationFormation,
        SettlementImport, SoilDepth, SoilMappingUnit, SoilProfile, SoilProperties, SoilSubstrate,
        SoilWaterRegime, StoneContentPercent, SurfaceGeology, SurfaceLithology,
        TopsoilOrganicCarbon, TravelEdgeImport, TravelRoute, TreeSpeciesId, TreeSpeciesProfile,
        UnconsolidatedDeposit, Woodland, WrbReferenceGroup,
    };

    use super::{
        MAX_REDUCER_ARGUMENT_CHARS, default_output, encode_settlement, encode_travel_edge,
        serialize_batches,
    };

    #[test]
    fn encodes_shared_enum_for_spacetimedb_sats_json() {
        let edge = TravelEdgeImport {
            id: 1,
            from_node_id: 2,
            to_node_id: 3,
            route: TravelRoute::Land {
                bridge: Some(EdgeEndpoint::To),
            },
            toll: Some(EdgeEndpoint::From),
            length_m: 4,
            slope_multiplier: 1.0,
            certainty: 1,
            section: String::new(),
        };
        let batches = serialize_batches(&[edge], 100, encode_travel_edge).unwrap();
        assert_eq!(
            batches[0][0]["route"],
            serde_json::json!({ "Land": { "some": { "To": [] } } })
        );
        assert_eq!(
            batches[0][0]["toll"],
            serde_json::json!({ "some": { "From": [] } })
        );
    }

    #[test]
    fn encodes_all_forest_variants_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
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
    fn encodes_mapped_and_inferred_vegetation_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["potential_vegetation"],
            serde_json::json!({
                "Inferred": { "DeciduousAndMixedForest": [] }
            })
        );
        settlement.potential_vegetation = PotentialVegetation::Mapped(
            MappedPotentialVegetation::new(
                EuroVegMapUnitCode::new("F27").unwrap(),
                PotentialVegetationFormation::DeciduousAndMixedForest,
            )
            .unwrap(),
        );
        assert_eq!(
            encode_settlement(&settlement).unwrap()["potential_vegetation"],
            serde_json::json!({
                "Mapped": {
                    "unit": { "code": "F27" },
                    "formation": { "DeciduousAndMixedForest": [] }
                }
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
    fn encodes_mapped_and_inferred_soil_profiles_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["soil"]["Inferred"]["substrate"]["Mineral"]["texture"],
            serde_json::json!({ "Medium": [] })
        );
        let SoilProfile::Inferred(properties) = settlement.soil.clone() else {
            unreachable!()
        };
        settlement.soil = SoilProfile::Mapped(MappedSoilProfile {
            mapping_unit: SoilMappingUnit::new(10, 100, 75).unwrap(),
            wrb_group: WrbReferenceGroup::Cambisol,
            parent_material: ParentMaterialCode::new("110").unwrap(),
            properties,
        });
        assert_eq!(
            encode_settlement(&settlement).unwrap()["soil"]["Mapped"]["mapping_unit"],
            serde_json::json!({ "smu": 10, "dominant_stu": 100, "dominance_percent": 75 })
        );
        assert_eq!(
            encode_settlement(&settlement).unwrap()["soil"]["Mapped"]["wrb_group"],
            serde_json::json!({ "Cambisol": [] })
        );
    }

    #[test]
    fn encodes_mapped_and_inferred_geology_for_spacetimedb_sats_json() {
        let mut settlement = settlement(ForestCover::Open);
        assert_eq!(
            encode_settlement(&settlement).unwrap()["geology"],
            serde_json::json!({ "Inferred": {
                "lithology": { "Inferred": { "Unconsolidated": { "Alluvium": [] } } },
                "age": { "Inferred": { "Quaternary": [] } },
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
                PotentialVegetationFormation::DeciduousAndMixedForest,
            ),
            tree_species: TreeSpeciesProfile::Inferred(
                InferredTreeSpeciesProfile::new(vec![TreeSpeciesId::new("Quercus_robur").unwrap()])
                    .unwrap(),
            ),
            soil: SoilProfile::Inferred(SoilProperties {
                substrate: SoilSubstrate::Mineral(MineralSoil {
                    texture: MineralSoilTexture::Medium,
                    depth: SoilDepth::Deep,
                    available_water: AvailableWaterCapacity::Medium,
                    organic_carbon: TopsoilOrganicCarbon::Medium,
                    stones: StoneContentPercent::new(10).unwrap(),
                }),
                water_regime: SoilWaterRegime::SeasonallyWet,
                agricultural_limitation: AgriculturalLimitation::None,
            }),
            geology: SurfaceGeology::Inferred(GeologicSetting {
                lithology: GeologicLithologyEvidence::Inferred(SurfaceLithology::Unconsolidated(
                    UnconsolidatedDeposit::Alluvium,
                )),
                age: GeologicAgeEvidence::Inferred(GeologicEra::Quaternary),
            }),
            scene_key: "village".into(),
            religion_id: "catholic".into(),
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
                .chars()
                .count()
                <= MAX_REDUCER_ARGUMENT_CHARS
        }));
    }

    #[test]
    fn default_output_names_the_selected_world_year() {
        assert!(default_output(1600).ends_with("target/world-1600.json"));
    }
}
