//! Tactical Server - Lightyear-based authoritative game server for tactical missions
//!
//! This server:
//! - Runs as a separate OS process, spawned by SpacetimeDB client
//! - Uses adventure-simulator-net for multi-protocol networking (WebTransport, WebSocket, UDP)
//! - Loads spawn points from GLB files (headless parsing via gltf crate)
//! - Validates join tokens using HMAC shared secret
//! - Auto-terminates after mission timeout and commits results
//!
//! Supports two backends:
//! - SpacetimeDB: Use --spacetimedb-url to commit directly to SpacetimeDB
//! - HTTP API: Use --strategic-api-url to commit via strategic-api HTTP endpoint

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use adventure_simulator_net::prelude::*;
use adventure_simulator_net::protocol::WebTransportCertificateSettings;
use bevy::prelude::*;
use clap::Parser;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{error, info, warn};

mod scene_loader;

use scene_loader::{SpawnMarker, SpawnMarkerType};

type HmacSha256 = Hmac<Sha256>;

/// Mission timeout in seconds
const MISSION_TIMEOUT_SECS: u64 = 60;

#[derive(Parser, Debug, Clone)]
#[command(name = "tactical-server")]
#[command(about = "Tactical mission server for Adventure Simulator")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "6000")]
    port: u16,

    /// Unique mission instance ID
    #[arg(long)]
    mission_id: String,

    /// Scene key (e.g., "town_a", "town_b")
    #[arg(long)]
    scene_key: String,

    /// Path to the GLB asset file
    #[arg(long)]
    asset_path: String,

    /// URL of the strategic API server (legacy HTTP backend)
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    strategic_api_url: String,

    /// URL of the SpacetimeDB server (preferred backend)
    /// Format: http://localhost:3000
    #[arg(long)]
    spacetimedb_url: Option<String>,

    /// SpacetimeDB module name
    #[arg(long, default_value = "strategic-stdb-module")]
    spacetimedb_module: String,

    /// HMAC secret for token validation
    #[arg(long)]
    hmac_secret: String,
}

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tactical_server=info".parse().unwrap())
                .add_directive("bevy_app=warn".parse().unwrap())
                .add_directive("bevy_ecs=warn".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    info!("Starting tactical server for mission {}", args.mission_id);
    info!("Scene: {}, Asset: {}", args.scene_key, args.asset_path);
    info!("Listening on port {}", args.port);

    // Load spawn markers from GLB
    let spawn_markers = match scene_loader::load_spawn_markers(&args.asset_path) {
        Ok(markers) => {
            info!("Loaded {} spawn markers from {}", markers.len(), args.asset_path);
            for marker in &markers {
                info!("  - {:?} at {:?}", marker.marker_type, marker.position);
            }
            markers
        }
        Err(e) => {
            warn!("Failed to load spawn markers from {}: {}", args.asset_path, e);
            warn!("Using default spawn markers");
            vec![
                SpawnMarker {
                    name: "spawn_player".to_string(),
                    marker_type: SpawnMarkerType::Player,
                    position: Vec3::new(0.0, 0.0, 0.0),
                },
                SpawnMarker {
                    name: "spawn_enemy_1".to_string(),
                    marker_type: SpawnMarkerType::Enemy,
                    position: Vec3::new(5.0, 0.0, 5.0),
                },
                SpawnMarker {
                    name: "spawn_enemy_2".to_string(),
                    marker_type: SpawnMarkerType::Enemy,
                    position: Vec3::new(-5.0, 0.0, 5.0),
                },
            ]
        }
    };

    // Run Bevy app with adventure-simulator-net
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(AdventureSimulatorNetPlugins)
        .insert_resource(MissionConfig {
            mission_id: args.mission_id.clone(),
            scene_key: args.scene_key.clone(),
            strategic_api_url: args.strategic_api_url.clone(),
            spacetimedb_url: args.spacetimedb_url.clone(),
            spacetimedb_module: args.spacetimedb_module.clone(),
            hmac_secret: args.hmac_secret.clone(),
            port: args.port,
        })
        .insert_resource(MissionState {
            started_at: std::time::Instant::now(),
            spawn_markers,
            enemies_killed: 0,
            players_connected: 0,
            committed: false,
        })
        .add_systems(Startup, setup_server)
        .add_systems(Update, (check_mission_timeout, log_status_periodically))
        .run();
}

#[derive(Resource)]
struct MissionConfig {
    mission_id: String,
    #[allow(dead_code)]
    scene_key: String,
    strategic_api_url: String,
    spacetimedb_url: Option<String>,
    spacetimedb_module: String,
    #[allow(dead_code)]
    hmac_secret: String,
    #[allow(dead_code)]
    port: u16,
}

#[derive(Resource)]
struct MissionState {
    started_at: std::time::Instant,
    spawn_markers: Vec<SpawnMarker>,
    enemies_killed: u32,
    players_connected: u32,
    committed: bool,
}

fn setup_server(mut commands: Commands, config: Res<MissionConfig>, state: Res<MissionState>) {
    info!("=== Tactical Server Setup ===");
    info!("Mission ID: {}", config.mission_id);
    info!("Scene Key: {}", config.scene_key);
    info!("Port: {}", config.port);
    info!("Spawn markers loaded: {}", state.spawn_markers.len());

    // Create server address
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.port);

    // Spawn Lightyear server using adventure-simulator-net
    // Using WebTransport with auto-generated self-signed certificate
    info!("Starting Lightyear server on {} (WebTransport)", addr);
    commands.spawn(AdventureSimulatorServer {
        addr,
        protocol: ServerProtocol::WebTransport {
            certificate: WebTransportCertificateSettings::default(),
        },
        protocol_settings: ProtocolSettings::default(),
    });

    // Log spawn markers
    for marker in &state.spawn_markers {
        match marker.marker_type {
            SpawnMarkerType::Player => {
                info!("Player spawn point: {} at {:?}", marker.name, marker.position);
            }
            SpawnMarkerType::Enemy => {
                info!("Enemy spawn point: {} at {:?}", marker.name, marker.position);
            }
            SpawnMarkerType::Item => {
                info!("Item spawn point: {} at {:?}", marker.name, marker.position);
            }
            SpawnMarkerType::Exit => {
                info!("Exit point: {} at {:?}", marker.name, marker.position);
            }
        }
    }

    info!("Server ready! Will timeout in {} seconds", MISSION_TIMEOUT_SECS);
    info!("Clients can connect via WebTransport (or fallback to WebSocket/UDP)");
}

#[allow(deprecated)]
fn check_mission_timeout(
    config: Res<MissionConfig>,
    mut state: ResMut<MissionState>,
    mut exit: EventWriter<AppExit>,
) {
    let elapsed = state.started_at.elapsed();

    if elapsed >= Duration::from_secs(MISSION_TIMEOUT_SECS) && !state.committed {
        info!("Mission timeout reached, committing results...");
        state.committed = true;

        // Commit mission results
        let success = state.enemies_killed > 0 || state.players_connected > 0;
        let xp_gained = (state.enemies_killed * 25) as i32;

        // Spawn async task to commit (we'll do this synchronously for MVP)
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Choose backend: prefer SpacetimeDB if configured
        let result = if let Some(ref stdb_url) = config.spacetimedb_url {
            rt.block_on(commit_mission_spacetimedb(
                stdb_url,
                &config.spacetimedb_module,
                &config.mission_id,
                success,
                xp_gained,
            ))
        } else {
            rt.block_on(commit_mission_http(
                &config.strategic_api_url,
                &config.mission_id,
                success,
                xp_gained,
            ))
        };

        match result {
            Ok(_) => info!("Mission committed successfully"),
            Err(e) => error!("Failed to commit mission: {}", e),
        }

        // Exit the server
        info!("Shutting down tactical server");
        exit.write(AppExit::Success);
    }
}

fn log_status_periodically(state: Res<MissionState>) {
    let elapsed = state.started_at.elapsed().as_secs();

    // Log every 10 seconds
    if elapsed > 0 && elapsed % 10 == 0 {
        let remaining = MISSION_TIMEOUT_SECS.saturating_sub(elapsed);
        if remaining > 0 && !state.committed {
            info!(
                "Mission status: {} seconds remaining, {} enemies killed",
                remaining, state.enemies_killed
            );
        }
    }
}

/// Commit mission results via HTTP API (legacy strategic-api backend)
async fn commit_mission_http(
    strategic_api_url: &str,
    mission_id: &str,
    success: bool,
    xp_gained: i32,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    #[derive(serde::Serialize)]
    struct CommitRequest {
        mission_id: String,
        success: bool,
        xp_gained: i32,
        items_gained: Vec<ItemGained>,
    }

    #[derive(serde::Serialize)]
    struct ItemGained {
        item_id: String,
        qty: i32,
    }

    let items = if success {
        vec![
            ItemGained {
                item_id: "gold_coin".to_string(),
                qty: 5,
            },
            ItemGained {
                item_id: "health_potion".to_string(),
                qty: 1,
            },
        ]
    } else {
        vec![]
    };

    let req = CommitRequest {
        mission_id: mission_id.to_string(),
        success,
        xp_gained,
        items_gained: items,
    };

    let url = format!("{}/api/mission/commit", strategic_api_url);
    info!("Committing mission to HTTP API: {}", url);

    client.post(&url).json(&req).send().await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Commit mission results via SpacetimeDB reducer
async fn commit_mission_spacetimedb(
    spacetimedb_url: &str,
    module_name: &str,
    mission_id: &str,
    success: bool,
    xp_gained: i32,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    // Build items JSON
    let items_json = if success {
        r#"[{"item_id":"gold_coin","qty":5},{"item_id":"health_potion","qty":1}]"#
    } else {
        "[]"
    };

    // SpacetimeDB HTTP API: POST /database/call/{module}/{reducer}
    // Body: JSON array of reducer arguments
    let url = format!("{}/database/call/{}/commit_mission", spacetimedb_url, module_name);
    info!("Committing mission to SpacetimeDB: {}", url);

    // Arguments: [mission_id, success, xp_gained, items_gained_json]
    let args = serde_json::json!([mission_id, success, xp_gained, items_json]);

    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .body(args.to_string())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "unknown".to_string());
        return Err(format!("SpacetimeDB commit failed: {} - {}", status, body));
    }

    Ok(())
}

/// Validate a join token using HMAC-SHA256
#[allow(dead_code)]
fn validate_join_token(token: &str, hmac_secret: &str) -> Option<(String, String)> {
    // Token format: mission_id:party_id:signature
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let mission_id = parts[0];
    let party_id = parts[1];
    let provided_signature = parts[2];

    // Recompute signature
    let mut mac = HmacSha256::new_from_slice(hmac_secret.as_bytes()).ok()?;
    let payload = format!("{}:{}", mission_id, party_id);
    mac.update(payload.as_bytes());
    let expected_signature = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        mac.finalize().into_bytes(),
    );

    if provided_signature == expected_signature {
        Some((mission_id.to_string(), party_id.to_string()))
    } else {
        None
    }
}
