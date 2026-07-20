//! Tactical Spawner - Watches SpacetimeDB and spawns tactical-server processes
//!
//! Subscribes to the tactical_server_request table and spawns a server process
//! whenever a new request appears. The spawned server will then call
//! create_tactical_server_for_request to register itself.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::process::Command;
use std::sync::{Arc, Mutex};

use adventuresim_stdb_client::spacetimedb_sdk::{DbContext, Table};
use adventuresim_stdb_client::{
    DbConnection, TacticalServerRequestTableAccess, tactical_server_requestQueryTableAccess,
};
use clap::Parser;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "adventuresim-tactical-server-dispatcher")]
#[command(about = "Spawns adventuresim-tactical-server processes for pending missions")]
struct Args {
    /// SpacetimeDB URL
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, default_value = "adventuresim-stdb-module")]
    spacetimedb_module: String,

    /// Path to tactical-server binary
    #[arg(long, default_value = "adventuresim-tactical-server")]
    tactical_server_bin: String,

    /// Base port for tactical servers (incremented for each new server)
    #[arg(long, default_value = "6000")]
    base_port: u16,

    /// Public host for clients to connect to
    #[arg(long, default_value = "0.0.0.0")]
    host: IpAddr,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tactical_spawner=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    info!("Starting tactical spawner");
    info!(
        "SpacetimeDB: {}/{}",
        args.spacetimedb_url, args.spacetimedb_module
    );
    info!("Tactical server binary: {}", args.tactical_server_bin);

    // Shared state for tracking spawned missions and port allocation
    let spawned: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let next_port: Arc<Mutex<u16>> = Arc::new(Mutex::new(args.base_port));

    // Connect to SpacetimeDB
    let conn = DbConnection::builder()
        .with_uri(&args.spacetimedb_url)
        .with_database_name(&args.spacetimedb_module)
        .on_connect(|_ctx, _identity, _address| {
            info!("Connected to SpacetimeDB");
        })
        .on_connect_error(|_ctx, err| {
            error!("Connection error: {:?}", err);
        })
        .build()
        .expect("Failed to connect to SpacetimeDB");

    // Subscribe to tactical_server_request table
    conn.subscription_builder()
        .on_applied(|ctx| {
            info!("Subscription applied, checking existing pending requests...");
            for request in ctx.db.tactical_server_request().iter() {
                info!(
                    "Found pending request: {} for scene {}",
                    request.mission_id, request.scene_key
                );
            }
        })
        .add_query(|query| query.from.tactical_server_request())
        .subscribe();

    // Set up callback for new tactical server requests
    let spawned_clone = spawned.clone();
    let next_port_clone = next_port.clone();
    let bin = args.tactical_server_bin.clone();
    let stdb_url = args.spacetimedb_url.clone();
    let stdb_module = args.spacetimedb_module.clone();
    let host = args.host.clone();

    conn.db
        .tactical_server_request()
        .on_insert(move |_ctx, request| {
            let mut spawned = spawned_clone.lock().unwrap();
            if spawned.contains(&request.mission_id) {
                return;
            }

            let port = {
                let mut p = next_port_clone.lock().unwrap();
                let port = *p;
                *p += 1;
                port
            };

            info!(
                "Spawning tactical-server for mission {} (scene: {}) on port {}",
                request.mission_id, request.scene_key, port
            );

            let required_enemy_kills = request.required_enemy_kills.to_string();

            match Command::new(&bin)
                .args([
                    "--mission-id",
                    &request.mission_id,
                    "--scene-key",
                    &request.scene_key,
                    "--required-enemy-kills",
                    &required_enemy_kills,
                    "--addr",
                    &SocketAddr::new(host, port).to_string(),
                    "--spacetimedb-url",
                    &stdb_url,
                    "--spacetimedb-module",
                    &stdb_module,
                ])
                .spawn()
            {
                Ok(child) => {
                    info!("Spawned tactical-server (pid {})", child.id());
                    spawned.insert(request.mission_id.clone());
                }
                Err(e) => {
                    error!("Failed to spawn tactical-server: {}", e);
                }
            }
        });

    info!("Listening for pending missions...");

    // Run the connection loop, listening to WebSocket events
    loop {
        match conn.frame_tick() {
            Ok(_) => {}
            Err(e) => {
                warn!("Connection error: {}", e);
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
