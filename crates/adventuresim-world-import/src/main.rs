use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

use adventuresim_world_import::{Error, Result, WorldBuilder};
use adventuresim_world_schema::{
    CompiledWorld, EdgeEndpoint, SettlementImport, TravelEdgeImport, TravelRoute, WorldNodeImport,
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
    serde_json::to_value(settlement).map_err(Error::from)
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

fn default_output(year: i32) -> PathBuf {
    repository_root().join(format!("target/world-{year}.json"))
}

#[cfg(test)]
mod tests {
    use adventuresim_world_schema::{EdgeEndpoint, TravelEdgeImport, TravelRoute};

    use super::{
        MAX_REDUCER_ARGUMENT_CHARS, default_output, encode_travel_edge, serialize_batches,
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
