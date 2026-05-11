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
use adventuresim_stdb_client::{DbConnection, TacticalServerRequestTableAccess};
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

    /// Base port for tactical servers (incremented for each new server).
    ///
    /// Browsers commonly block port 6000, so the default starts at 6001.
    #[arg(long, default_value = "6001")]
    base_port: u16,

    /// Host/IP the spawned tactical servers bind to.
    #[arg(long, default_value = "0.0.0.0")]
    bind_host: IpAddr,

    /// Public host clients use to connect. Use a DNS name or public IP on a VPS.
    #[arg(long, default_value = "127.0.0.1")]
    public_host: String,
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
        .with_module_name(&args.spacetimedb_module)
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
        .subscribe(["SELECT * FROM tactical_server_request"]);

    // Set up callback for new tactical server requests
    let spawned_clone = spawned.clone();
    let next_port_clone = next_port.clone();
    let bin = args.tactical_server_bin.clone();
    let stdb_url = args.spacetimedb_url.clone();
    let stdb_module = args.spacetimedb_module.clone();
    let bind_host = args.bind_host;
    let public_host = args.public_host.clone();

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

            let bind_addr = SocketAddr::new(bind_host, port).to_string();
            let public_addr = format!("{public_host}:{port}");

            match Command::new(&bin)
                .args([
                    "--requested",
                    "--mission-id",
                    &request.mission_id,
                    "--scene-key",
                    &request.scene_key,
                    "--addr",
                    &bind_addr,
                    "--public-addr",
                    &public_addr,
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
