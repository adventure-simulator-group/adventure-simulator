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
    DbConnection, TacticalServerRequestTableAccess, authorize_tactical_server_claim,
    revoke_tactical_server_claim, tactical_server_requestQueryTableAccess,
};
use clap::Parser;
use sha2::{Digest, Sha256};
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

    /// Auth token for the registered strategic gateway identity.
    #[arg(long, env = "SPACETIMEDB_TOKEN")]
    spacetimedb_token: String,

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
        .with_token(Some(args.spacetimedb_token.clone()))
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
            if spawned_clone.lock().unwrap().contains(&request.mission_id) {
                return;
            }

            let port = {
                let mut p = next_port_clone.lock().unwrap();
                let port = *p;
                *p += 1;
                port
            };

            let claim_bytes: [u8; 32] = rand::random();
            let claim = claim_bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let claim_hash = Sha256::digest(claim.as_bytes()).to_vec();
            let mission_id = request.mission_id.clone();
            let scene_key = request.scene_key.clone();
            let expected_party_members = request.expected_party_members.to_string();
            let required_enemy_kills = request.required_enemy_kills.to_string();
            let enemy_combat_scale_bps = request.enemy_combat_scale_bps.to_string();
            let spawned = spawned_clone.clone();
            let bin = bin.clone();
            let stdb_url = stdb_url.clone();
            let stdb_module = stdb_module.clone();
            if let Err(error) = _ctx.reducers.authorize_tactical_server_claim_then(
                mission_id.clone(),
                claim_hash,
                move |_ctx, result| match result {
                    Ok(Ok(())) => {
                        info!(
                            "Spawning tactical-server for mission {} (scene: {}) on port {}",
                            mission_id, scene_key, port
                        );
                        match Command::new(&bin)
                            .env_remove("SPACETIMEDB_TOKEN")
                            .env("ADVENTURESIM_TACTICAL_CLAIM", &claim)
                            .args([
                                "--mission-id",
                                &mission_id,
                                "--scene-key",
                                &scene_key,
                                "--expected-party-members",
                                &expected_party_members,
                                "--required-enemy-kills",
                                &required_enemy_kills,
                                "--enemy-combat-scale-bps",
                                &enemy_combat_scale_bps,
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
                                spawned.lock().unwrap().insert(mission_id);
                            }
                            Err(error) => {
                                error!("Failed to spawn tactical-server: {error}");
                                if let Err(revoke_error) = _ctx
                                    .reducers
                                    .revoke_tactical_server_claim(mission_id.clone())
                                {
                                    error!(
                                        "Failed to revoke unused tactical claim: {revoke_error}"
                                    );
                                }
                            }
                        }
                    }
                    Ok(Err(error)) => error!("Tactical claim rejected: {error}"),
                    Err(error) => error!("Tactical claim reducer failed: {error}"),
                },
            ) {
                error!("Failed to authorize tactical claim: {error}");
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

#[cfg(test)]
mod tests {
    #[test]
    fn child_spawn_waits_for_claim_and_drops_gateway_token() {
        let source = include_str!("main.rs");
        let callback = source
            .split("authorize_tactical_server_claim_then")
            .nth(1)
            .expect("claim completion callback");
        assert!(callback.contains("Ok(Ok(()))"));
        assert!(callback.contains(".env_remove(\"SPACETIMEDB_TOKEN\")"));
        assert!(callback.contains(".env(\"ADVENTURESIM_TACTICAL_CLAIM\""));
    }
}
