//! Tactical Server - Minimal Lightyear game server
//!
//! Simple flow:
//! 1. Started by spawner with mission_id arg
//! 2. Calls tactical_server_ready reducer with connection info
//! 3. Runs game for clients
//! 4. Calls commit_mission reducer on timeout/exit

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use adventure_simulator_net::prelude::*;
use adventure_simulator_net::protocol::WebTransportCertificateSettings;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use clap::Parser;
use strategic_db_client::{commit_mission, DbConnection, tactical_server_ready};

/// Mission timeout in seconds (how long the server stays up waiting for players)
const MISSION_TIMEOUT_SECS: f32 = 300.0; // 5 minutes

#[derive(Parser, Debug, Clone, Resource)]
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

    /// SpacetimeDB URI (e.g., http://localhost:3000)
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
    let args = Args::parse();

    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(LogPlugin {
            filter: "tactical_server=info,bevy_app=warn,bevy_ecs=warn".to_string(),
            ..default()
        })
        .add_plugins(AdventureSimulatorNetPlugins)
        .insert_resource(args)
        .insert_resource(MissionState {
            timeout: Timer::from_seconds(MISSION_TIMEOUT_SECS, TimerMode::Once),
            enemies_killed: 0,
            committed: false,
        })
        .add_systems(Startup, (connect_spacetimedb, setup_server).chain())
        .add_systems(Update, check_mission_timeout)
        .run();
}

#[derive(Resource)]
struct SpacetimeConn(DbConnection);

#[derive(Resource)]
struct MissionState {
    timeout: Timer,
    enemies_killed: u32,
    committed: bool,
}

fn connect_spacetimedb(mut commands: Commands, args: Res<Args>) {
    info!("Starting tactical server for mission {}", args.mission_id);
    info!("Scene: {}, Port: {}", args.scene_key, args.port);

    info!("Connecting to SpacetimeDB: {}", args.spacetimedb_url);
    let conn = DbConnection::builder()
        .with_uri(&args.spacetimedb_url)
        .with_module_name(&args.spacetimedb_module)
        .build()
        .expect("Failed to connect to SpacetimeDB");

    info!("Calling tactical_server_ready reducer");
    conn.reducers
        .tactical_server_ready(
            args.mission_id.clone(),
            args.public_host.clone(),
            args.port,
            String::new(), // cert_digest
        )
        .expect("Failed to call tactical_server_ready");

    conn.frame_tick().ok();
    info!("SpacetimeDB notified - server marked as ready");

    commands.insert_resource(SpacetimeConn(conn));
}

fn setup_server(mut commands: Commands, args: Res<Args>) {
    info!("=== Tactical Server Ready ===");
    info!("Mission: {}", args.mission_id);
    info!("Scene: {}", args.scene_key);

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), args.port);

    commands.spawn(AdventureSimulatorServer {
        addr,
        protocol: ServerProtocol::WebTransport {
            certificate: WebTransportCertificateSettings::AutoSelfSigned(Default::default()),
        },
        protocol_settings: ProtocolSettings::default(),
    });

    info!("Listening on port {} (WebTransport)", args.port);
    info!("Will timeout in {} seconds", MISSION_TIMEOUT_SECS);
}

fn check_mission_timeout(
    time: Res<Time>,
    conn: Res<SpacetimeConn>,
    args: Res<Args>,
    mut state: ResMut<MissionState>,
    mut exit: MessageWriter<AppExit>,
) {
    state.timeout.tick(time.delta());

    if state.timeout.just_finished() && !state.committed {
        info!("Mission timeout, committing results...");
        state.committed = true;

        let success = state.enemies_killed > 0;
        let xp_gained = (state.enemies_killed * 25) as i32;

        match conn
            .0
            .reducers
            .commit_mission(args.mission_id.clone(), success, xp_gained)
        {
            Ok(_) => {
                conn.0.frame_tick().ok();
                info!("Mission committed successfully");
            }
            Err(e) => {
                error!("Failed to commit mission: {}", e);
            }
        }

        info!("Shutting down");
        exit.write(AppExit::Success);
    }
}
