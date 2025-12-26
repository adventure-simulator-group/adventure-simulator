//! Tactical Spawner - Watches SpacetimeDB and spawns tactical-server processes
//!
//! Polls for "pending" missions and starts a tactical-server process for each one.
//! Simple and minimal - just enough to demonstrate the architecture.

use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;

use clap::Parser;
use serde::Deserialize;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "tactical-spawner")]
#[command(about = "Spawns tactical-server processes for pending missions")]
struct Args {
    /// SpacetimeDB URL
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, default_value = "strategic-stdb-module")]
    spacetimedb_module: String,

    /// Path to tactical-server binary
    #[arg(long, default_value = "tactical-server")]
    tactical_server_bin: String,

    /// Base port for tactical servers (incremented for each new server)
    #[arg(long, default_value = "6000")]
    base_port: u16,

    /// Poll interval in milliseconds
    #[arg(long, default_value = "1000")]
    poll_interval_ms: u64,

    /// Public host for clients to connect to
    #[arg(long, default_value = "localhost")]
    public_host: String,
}

#[derive(Debug, Deserialize)]
struct TacticalServer {
    mission_id: String,
    scene_key: String,
    status: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tactical_spawner=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    info!("Starting tactical spawner");
    info!("SpacetimeDB: {}/{}", args.spacetimedb_url, args.spacetimedb_module);
    info!("Tactical server binary: {}", args.tactical_server_bin);

    let mut spawned_missions: HashSet<String> = HashSet::new();
    let mut next_port = args.base_port;

    loop {
        match poll_pending_missions(&args).await {
            Ok(pending) => {
                for mission in pending {
                    if spawned_missions.contains(&mission.mission_id) {
                        continue;
                    }

                    info!(
                        "Spawning tactical-server for mission {} (scene: {})",
                        mission.mission_id, mission.scene_key
                    );

                    let port = next_port;
                    next_port += 1;

                    // Spawn the tactical-server process
                    match Command::new(&args.tactical_server_bin)
                        .args([
                            "--mission-id",
                            &mission.mission_id,
                            "--scene-key",
                            &mission.scene_key,
                            "--port",
                            &port.to_string(),
                            "--spacetimedb-url",
                            &args.spacetimedb_url,
                            "--spacetimedb-module",
                            &args.spacetimedb_module,
                            "--public-host",
                            &args.public_host,
                        ])
                        .spawn()
                    {
                        Ok(child) => {
                            info!(
                                "Spawned tactical-server (pid {}) on port {}",
                                child.id(),
                                port
                            );
                            spawned_missions.insert(mission.mission_id);
                        }
                        Err(e) => {
                            error!("Failed to spawn tactical-server: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to poll SpacetimeDB: {}", e);
            }
        }

        tokio::time::sleep(Duration::from_millis(args.poll_interval_ms)).await;
    }
}

async fn poll_pending_missions(args: &Args) -> Result<Vec<TacticalServer>, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/database/{}/sql",
        args.spacetimedb_url, args.spacetimedb_module
    );

    let resp = client
        .post(&url)
        .header("Content-Type", "text/plain")
        .body("SELECT * FROM tactical_server WHERE status = 'pending'")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // Parse SpacetimeDB response format
    let mut missions = Vec::new();
    if let Some(results) = data.as_array() {
        for result in results {
            let schema = result.get("schema").and_then(|s| s.get("elements"));
            let rows = result.get("rows").and_then(|r| r.as_array());

            if let (Some(schema), Some(rows)) = (schema, rows) {
                let cols: Vec<&str> = schema
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                e.get("name")
                                    .and_then(|n| n.get("some").or(Some(n)))
                                    .and_then(|n| n.as_str())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                for row in rows {
                    if let Some(row_arr) = row.as_array() {
                        let mut mission_id = String::new();
                        let mut scene_key = String::new();
                        let mut status = String::new();

                        for (i, col) in cols.iter().enumerate() {
                            if let Some(val) = row_arr.get(i) {
                                match *col {
                                    "mission_id" => {
                                        mission_id = val.as_str().unwrap_or("").to_string()
                                    }
                                    "scene_key" => {
                                        scene_key = val.as_str().unwrap_or("").to_string()
                                    }
                                    "status" => status = val.as_str().unwrap_or("").to_string(),
                                    _ => {}
                                }
                            }
                        }

                        if !mission_id.is_empty() && status == "pending" {
                            missions.push(TacticalServer {
                                mission_id,
                                scene_key,
                                status,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(missions)
}
