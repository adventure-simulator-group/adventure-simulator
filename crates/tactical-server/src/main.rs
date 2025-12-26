//! Tactical Server - Minimal Lightyear game server
//!
//! Simple flow:
//! 1. Started by spawner with mission_id arg
//! 2. Calls tactical_server_ready reducer with connection info
//! 3. Runs game for clients
//! 4. Calls commit_mission reducer on timeout/exit

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use adventure_simulator_net::prelude::*;
use adventure_simulator_net::protocol::WebTransportCertificateSettings;
use bevy::prelude::*;
use clap::Parser;
use tracing::{error, info};

/// Mission timeout in seconds
const MISSION_TIMEOUT_SECS: u64 = 120;

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

    /// SpacetimeDB URL (e.g., http://localhost:3000)
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, default_value = "strategic-stdb-module")]
    spacetimedb_module: String,

    /// Public host for clients to connect to
    #[arg(long, default_value = "localhost")]
    public_host: String,
}

fn main() {
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
    info!("Scene: {}, Port: {}", args.scene_key, args.port);

    // Notify SpacetimeDB that we're ready (blocking before app starts)
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(e) = rt.block_on(notify_server_ready(&args)) {
        error!("Failed to notify SpacetimeDB: {}", e);
        std::process::exit(1);
    }

    // Run Bevy app
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(AdventureSimulatorNetPlugins)
        .insert_resource(MissionConfig {
            mission_id: args.mission_id.clone(),
            scene_key: args.scene_key.clone(),
            spacetimedb_url: args.spacetimedb_url.clone(),
            spacetimedb_module: args.spacetimedb_module.clone(),
            port: args.port,
        })
        .insert_resource(MissionState {
            started_at: std::time::Instant::now(),
            enemies_killed: 0,
            committed: false,
        })
        .add_systems(Startup, setup_server)
        .add_systems(Update, check_mission_timeout)
        .run();
}

#[derive(Resource)]
struct MissionConfig {
    mission_id: String,
    scene_key: String,
    spacetimedb_url: String,
    spacetimedb_module: String,
    port: u16,
}

#[derive(Resource)]
struct MissionState {
    started_at: std::time::Instant,
    enemies_killed: u32,
    committed: bool,
}

fn setup_server(mut commands: Commands, config: Res<MissionConfig>) {
    info!("=== Tactical Server Ready ===");
    info!("Mission: {}", config.mission_id);
    info!("Scene: {}", config.scene_key);

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), config.port);

    // Start Lightyear server with auto self-signed cert
    commands.spawn(AdventureSimulatorServer {
        addr,
        protocol: ServerProtocol::WebTransport {
            certificate: WebTransportCertificateSettings::AutoSelfSigned(Default::default()),
        },
        protocol_settings: ProtocolSettings::default(),
    });

    info!("Listening on port {} (WebTransport)", config.port);
    info!("Will timeout in {} seconds", MISSION_TIMEOUT_SECS);
}

#[allow(deprecated)]
fn check_mission_timeout(
    config: Res<MissionConfig>,
    mut state: ResMut<MissionState>,
    mut exit: EventWriter<AppExit>,
) {
    let elapsed = state.started_at.elapsed();

    if elapsed >= Duration::from_secs(MISSION_TIMEOUT_SECS) && !state.committed {
        info!("Mission timeout, committing results...");
        state.committed = true;

        let success = state.enemies_killed > 0;
        let xp_gained = (state.enemies_killed * 25) as i32;

        let rt = tokio::runtime::Runtime::new().unwrap();
        if let Err(e) = rt.block_on(commit_mission(&config, success, xp_gained)) {
            error!("Failed to commit mission: {}", e);
        }

        info!("Shutting down");
        exit.write(AppExit::Success);
    }
}

/// Notify SpacetimeDB that we're ready
async fn notify_server_ready(args: &Args) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/database/{}/call/tactical_server_ready",
        args.spacetimedb_url, args.spacetimedb_module
    );

    // Args: [mission_id, host, port, cert_digest]
    // For now, cert_digest is empty (auto self-signed certs change each run)
    let body = serde_json::json!([
        args.mission_id,
        args.public_host,
        args.port,
        "" // cert_digest - would need to extract from wtransport
    ]);

    info!("Notifying SpacetimeDB: {}", url);
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("{}: {}", status, text));
    }

    info!("SpacetimeDB notified - server marked as ready");
    Ok(())
}

/// Commit mission results to SpacetimeDB
async fn commit_mission(config: &MissionConfig, success: bool, xp_gained: i32) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/database/{}/call/commit_mission",
        config.spacetimedb_url, config.spacetimedb_module
    );

    // Args: [mission_id, success, xp_gained]
    let body = serde_json::json!([config.mission_id, success, xp_gained]);

    info!("Committing mission to SpacetimeDB");
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("{}: {}", status, text));
    }

    info!("Mission committed successfully");
    Ok(())
}
