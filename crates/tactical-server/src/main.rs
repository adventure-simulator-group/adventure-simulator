//! Tactical Server - Minimal Lightyear game server
//!
//! Simple flow:
//! 1. Started by spawner with mission_id arg
//! 2. Calls tactical_server_ready reducer with connection info
//! 3. Runs game for clients
//! 4. Calls commit_mission reducer on timeout/exit

use std::net::SocketAddr;

use adventure_simulator_net::prelude::*;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use clap::{ArgAction, Parser};
use strategic_db_client::{commit_mission, tactical_server_ready, DbConnection};

/// Default [`Args::timeout`] time.
const MISSION_TIMEOUT_SECS: f32 = 300.0; // 5 minutes

#[derive(Parser, Debug, Clone, Resource)]
#[command(name = "tactical-server")]
#[command(about = "Tactical mission server for Adventure Simulator")]
struct Args {
    /// Address to listen on
    #[arg(long, default_value = "0.0.0.0:6000")]
    addr: SocketAddr,

    /// Unique mission instance ID
    #[arg(long)]
    mission_id: String,

    /// Scene key (e.g., "town_a", "town_b")
    #[arg(long)]
    scene_key: String,

    /// SpacetimeDB URI (e.g., http://localhost:3000)
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, default_value = "strategic-stdb-module")]
    spacetimedb_module: String,

    /// Mission timeout in seconds (how long the server stays up waiting for players)
    #[arg(long, default_value_t = MISSION_TIMEOUT_SECS)]
    timeout: f32,

    /// Disable the timeout entirely
    #[arg(
        long,
        action = ArgAction::SetTrue,
        conflicts_with = "timeout"
    )]
    no_timeout: bool,
}

fn main() {
    let args = Args::parse();

    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(LogPlugin {
            filter: "tactical_server=info,bevy_app=warn,bevy_ecs=warn".to_string(),
            ..default()
        })
        .add_plugins(AdventureSimulatorNetPlugins)
        .insert_resource(MissionState {
            timeout: (!args.no_timeout)
                .then_some(args.timeout)
                .map(|duration| Timer::from_seconds(duration, TimerMode::Once)),
            enemies_killed: 0,
            committed: false,
        })
        .insert_resource(args)
        .add_systems(Startup, (connect_spacetimedb, setup_server).chain())
        .add_systems(Update, check_mission_timeout)
        .run();
}

#[derive(Resource)]
struct SpacetimeConn(DbConnection);

#[derive(Resource)]
struct MissionState {
    timeout: Option<Timer>,
    enemies_killed: u32,
    committed: bool,
}

fn connect_spacetimedb(mut commands: Commands, args: Res<Args>) -> Result {
    info!("Starting tactical server for mission {}", args.mission_id);
    info!("Scene: {}, Address: {}", args.scene_key, args.addr);

    info!("Connecting to SpacetimeDB: {}", args.spacetimedb_url);
    let conn = DbConnection::builder()
        .with_uri(&args.spacetimedb_url)
        .with_module_name(&args.spacetimedb_module)
        .build()
        .expect("Failed to connect to SpacetimeDB");

    info!("Calling tactical_server_ready reducer");
    conn.reducers
        .tactical_server_ready(args.mission_id.clone(), args.addr.to_string(), default())
        .expect("Failed to call tactical_server_ready");

    conn.frame_tick()?;
    info!("SpacetimeDB notified - server marked as ready");

    commands.insert_resource(SpacetimeConn(conn));

    Ok(())
}

fn setup_server(mut commands: Commands, args: Res<Args>) {
    info!("=== Tactical Server Ready ===");
    info!("Mission: {}", args.mission_id);
    info!("Scene: {}", args.scene_key);

    commands.spawn(AdventureSimulatorServer {
        addr: args.addr,
        protocol: ServerProtocol::WebSocket,
        protocol_settings: ProtocolSettings::default(),
    });

    info!("Listening on {} (WebSocket)", args.addr);
    if !args.no_timeout {
        info!("Will timeout in {} seconds", args.timeout);
    }
}

fn check_mission_timeout(
    time: Res<Time>,
    conn: Res<SpacetimeConn>,
    args: Res<Args>,
    mut state: ResMut<MissionState>,
    mut exit: MessageWriter<AppExit>,
) -> Result {
    let is_timeout = match state.timeout {
        Some(ref mut timer) => {
            timer.tick(time.delta());
            timer.is_finished()
        }
        None => false,
    };

    if !is_timeout || state.committed {
        return Ok(());
    }

    info!("Mission timeout, committing results...");
    state.committed = true;

    let success = state.enemies_killed > 0;
    let xp_gained = (state.enemies_killed * 25) as i32;

    conn.0
        .reducers
        .commit_mission(args.mission_id.clone(), success, xp_gained)?;
    conn.0.frame_tick()?;
    info!("Mission committed successfully");

    info!("Shutting down");
    exit.write(AppExit::Success);
    Ok(())
}
