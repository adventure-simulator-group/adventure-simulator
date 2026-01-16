//! Tactical Spawner - Watches SpacetimeDB and spawns tactical-server processes
//!
//! Subscribes to the tactical_server table and spawns a server process
//! whenever a new "pending" mission appears.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::process::Command;
use std::sync::{Arc, Mutex};

use clap::Parser;
use strategic_db_client::spacetimedb_sdk::{DbContext, Table};
use strategic_db_client::{DbConnection, TacticalServerTableAccess, TacticalStatus};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "tactical-spawner")]
#[command(about = "Spawns tactical-server processes for pending missions")]
struct Args {
    /// SpacetimeDB URL
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, default_value = "strategic-db")]
    spacetimedb_module: String,

    /// Path to tactical-server binary
    #[arg(long, default_value = "tactical-server")]
    tactical_server_bin: String,

    /// Base port for tactical servers (incremented for each new server)
    #[arg(long, default_value = "6000")]
    base_port: u16,

    /// Public host for clients to connect to
    #[arg(long, default_value_t = Ipv4Addr::LOCALHOST)]
    host: Ipv4Addr,
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

    // Subscribe to tactical_server table
    conn.subscription_builder()
        .on_applied(|ctx| {
            info!("Subscription applied, checking existing pending missions...");
            for server in ctx.db.tactical_server().iter() {
                if server.status == TacticalStatus::Pending {
                    info!("Found existing pending mission: {}", server.mission_id);
                }
            }
        })
        .subscribe(["SELECT * FROM tactical_server"]);

    // Set up callback for new pending missions
    let spawned_clone = spawned.clone();
    let next_port_clone = next_port.clone();
    let bin = args.tactical_server_bin.clone();
    let stdb_url = args.spacetimedb_url.clone();
    let stdb_module = args.spacetimedb_module.clone();
    let public_host = args.host.clone();

    conn.db.tactical_server().on_insert(move |_ctx, server| {
        if server.status != TacticalStatus::Pending {
            return;
        }

        let mut spawned = spawned_clone.lock().unwrap();
        if spawned.contains(&server.mission_id) {
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
            server.mission_id, server.scene_key, port
        );

        match Command::new(&bin)
            .args([
                "--mission-id",
                &server.mission_id,
                "--scene-key",
                &server.scene_key,
                "--addr",
                &SocketAddr::new(public_host.into(), port).to_string(),
                "--spacetimedb-url",
                &stdb_url,
                "--spacetimedb-module",
                &stdb_module,
            ])
            .spawn()
        {
            Ok(child) => {
                info!("Spawned tactical-server (pid {})", child.id());
                spawned.insert(server.mission_id.clone());
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
