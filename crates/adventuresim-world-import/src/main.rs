use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

use adventuresim_world_import::{Error, Result, WorldBuilder};
use adventuresim_world_schema::{CompiledWorld, WORLD_SCHEMA_VERSION};
use clap::Parser;
use serde::Serialize;
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
    #[arg(long, default_value_t = WORLD_YEAR)]
    year: i32,
    #[arg(long, default_value_os_t = default_output())]
    output: PathBuf,
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
    let world = WorldBuilder::new(args.year).build_from_viabundus(&args.viabundus_dir)?;
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut artifact = serde_json::to_vec(&world)?;
    artifact.push(b'\n');
    std::fs::write(&args.output, artifact)?;
    println!("{}", serde_json::to_string_pretty(&world.report)?);
    println!("Wrote compiled world to {}", args.output.display());
    if args.load {
        load_world(&world, &args)?;
    }
    Ok(())
}

fn load_world(world: &CompiledWorld, args: &Args) -> Result<()> {
    call_reducer(
        args,
        "begin_world_data_import",
        &[json!(WORLD_SCHEMA_VERSION)],
    )?;

    for (label, reducer, batches) in [
        (
            "nodes",
            "import_world_nodes",
            serialize_batches(&world.nodes, args.batch_size, encode_node_options)?,
        ),
        (
            "edges",
            "import_travel_edges",
            serialize_batches(&world.edges, args.batch_size, encode_edge_kinds)?,
        ),
        (
            "settlements",
            "import_settlements",
            serialize_batches(&world.settlements, args.batch_size, identity)?,
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
    Ok(())
}

fn serialize_batches<T: Serialize>(
    rows: &[T],
    batch_size: usize,
    transform: fn(Value) -> Value,
) -> Result<Vec<Vec<Value>>> {
    let rows = rows
        .iter()
        .map(|row| {
            serde_json::to_value(row)
                .map(transform)
                .map_err(Error::from)
        })
        .collect::<Result<Vec<_>>>()?;
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

fn encode_node_options(mut node: Value) -> Value {
    let parent = node["parent_node_id"].take();
    node["parent_node_id"] = if parent.is_null() {
        json!({ "none": [] })
    } else {
        json!({ "some": parent })
    };
    node
}

fn encode_edge_kinds(mut edge: Value) -> Value {
    let kind = edge["kind"].take();
    let variant = match kind.as_str().expect("serialized edge kinds are strings") {
        "land" => "Land",
        "ferry" => "Ferry",
        other => panic!("unsupported serialized edge kind {other}"),
    };
    let mut encoded = serde_json::Map::new();
    encoded.insert(variant.into(), json!([]));
    edge["kind"] = Value::Object(encoded);
    edge
}

fn identity(value: Value) -> Value {
    value
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

fn default_output() -> PathBuf {
    repository_root().join("target/world-1544.json")
}

#[cfg(test)]
mod tests {
    use adventuresim_world_schema::{TravelEdgeImport, TravelEdgeKind};

    use super::{MAX_REDUCER_ARGUMENT_CHARS, encode_edge_kinds, serialize_batches};

    #[test]
    fn encodes_shared_enum_for_spacetimedb_sats_json() {
        let edge = TravelEdgeImport {
            id: 1,
            from_node_id: 2,
            to_node_id: 3,
            kind: TravelEdgeKind::Ferry,
            length_m: 4,
            slope_multiplier: 1.0,
            certainty: 1,
            section: String::new(),
        };
        let batches = serialize_batches(&[edge], 100, encode_edge_kinds).unwrap();
        assert_eq!(batches[0][0]["kind"], serde_json::json!({ "Ferry": [] }));
    }

    #[test]
    fn batches_are_bounded_for_windows_process_arguments() {
        let rows = vec!["x".repeat(1_000); 100];
        let batches = serialize_batches(&rows, 100, super::identity).unwrap();
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|batch| {
            serde_json::Value::Array(batch.clone())
                .to_string()
                .chars()
                .count()
                <= MAX_REDUCER_ARGUMENT_CHARS
        }));
    }
}
