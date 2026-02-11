//! Tactical Server - Minimal Lightyear game server
//!
//! Simple flow:
//! 1. Started by spawner with mission_id arg
//! 2. Calls tactical_server_ready reducer with connection info
//! 3. Runs game for clients
//! 4. Calls commit_mission reducer on timeout/exit

use std::net::SocketAddr;

use adventure_simulator_core::prelude::*;
use adventure_simulator_net::{
    lightyear::prelude::{
        server::{self, ClientOf},
        *,
    },
    prelude::*,
    protocol::SEND_INTERVAL,
};
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
    #[arg(long, default_value = "127.0.0.1:6000")]
    addr: SocketAddr,

    /// Unique mission instance ID
    #[arg(long, env = "MISSION_ID")]
    mission_id: String,

    /// Scene key (e.g., "town_a", "town_b")
    #[arg(long, env = "SCENE_KEY")]
    scene_key: String,

    /// SpacetimeDB URI (e.g., http://localhost:3000)
    #[arg(long, env = "SPACETIMEDB_URL", default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, env = "SPACETIMEDB_MODULE", default_value = "strategic-db")]
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

/// Detect Edgegap environment and override network bind address from env vars.
/// Called before Bevy logging is initialized, so uses eprintln.
fn apply_edgegap_overrides(args: &mut Args) {
    let Some(public_ip) = std::env::var("ARBITRIUM_PUBLIC_IP").ok() else {
        return;
    };

    eprintln!("[edgegap] Detected ARBITRIUM_PUBLIC_IP={}", public_ip);

    // Find the game port from ARBITRIUM_PORT_* env vars
    // Edgegap sets e.g. ARBITRIUM_PORT_GAME=12345
    let external_port = std::env::vars()
        .find_map(|(key, value)| {
            if key.starts_with("ARBITRIUM_PORT_") {
                value.parse::<u16>().ok()
            } else {
                None
            }
        })
        .unwrap_or(6000);

    // Override addr: bind to 0.0.0.0 internally, advertise public address separately
    args.addr = format!("0.0.0.0:{}", external_port)
        .parse()
        .unwrap_or(args.addr);

    eprintln!(
        "[edgegap] Listening on {}, public IP {}:{}",
        args.addr, public_ip, external_port
    );

    // Mission metadata is injected by the deployment request as standard env vars:
    // MISSION_ID, SCENE_KEY, SPACETIMEDB_URL, SPACETIMEDB_MODULE.
}

/// Stored alongside Args so connect_spacetimedb can advertise the public address
#[derive(Resource)]
struct EdgegapPublicAddr(Option<String>);

fn main() {
    let mut args = Args::parse();
    apply_edgegap_overrides(&mut args);

    // Store the public address for advertising to SpacetimeDB
    let public_addr = std::env::var("ARBITRIUM_PUBLIC_IP").ok().map(|ip| {
        let port = args.addr.port();
        format!("{}:{}", ip, port)
    });

    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(LogPlugin {
            filter: "tactical_server=info,bevy_app=warn,bevy_ecs=warn".to_string(),
            ..default()
        })
        .add_plugins((AdventureSimulatorCorePlugins, AdventureSimulatorNetPlugins))
        .insert_resource(MissionState {
            timeout: (!args.no_timeout)
                .then_some(args.timeout)
                .map(|duration| Timer::from_seconds(duration, TimerMode::Once)),
            enemies_killed: 0,
            committed: false,
        })
        .insert_resource(EdgegapPublicAddr(public_addr))
        .insert_resource(args)
        .add_systems(Startup, (connect_spacetimedb, setup_server).chain())
        .add_systems(Update, check_mission_timeout)
        .add_observer(on_new_client_added_hook)
        .add_observer(on_new_client_connected_hook)
        .add_observer(on_server_started_hook)
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

fn connect_spacetimedb(
    mut commands: Commands,
    args: Res<Args>,
    public_addr: Res<EdgegapPublicAddr>,
) -> Result {
    info!("Starting tactical server for mission {}", args.mission_id);
    info!("Scene: {}, Address: {}", args.scene_key, args.addr);

    info!("Connecting to SpacetimeDB: {}", args.spacetimedb_url);
    let conn = DbConnection::builder()
        .with_uri(&args.spacetimedb_url)
        .with_module_name(&args.spacetimedb_module)
        .build()
        .expect("Failed to connect to SpacetimeDB");

    // Advertise the public address (Edgegap) or local address
    let advertised_addr = public_addr
        .0
        .as_deref()
        .unwrap_or(&args.addr.to_string())
        .to_string();

    info!("Calling tactical_server_ready (addr={})", advertised_addr);
    conn.reducers
        .tactical_server_ready(args.mission_id.clone(), advertised_addr, default())
        .expect("Failed to call tactical_server_ready");

    conn.frame_tick()?;
    info!("SpacetimeDB notified - server marked as ready");

    commands.insert_resource(SpacetimeConn(conn));

    Ok(())
}

fn setup_server(mut commands: Commands, args: Res<Args>) -> Result {
    info!("=== Tactical Server Ready ===");
    info!("Mission: {}", args.mission_id);
    info!("Scene: {}", args.scene_key);

    commands.spawn(AdventureSimulatorServer {
        addr: args.addr,
        protocol_settings: ProtocolSettings::default(),
    });

    if !args.no_timeout {
        info!("Will timeout in {} seconds", args.timeout);
    }

    Ok(())
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

fn on_server_started_hook(
    _event: On<Add, server::Started>,
    args: Res<Args>,
    mut commands: Commands,
) {
    // Spawn physical scene
    commands.spawn((
        GameSceneId(args.scene_key.clone()),
        RigidBody::Static,
        Collider::half_space(Vec3::Y),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Replicate::to_clients(NetworkTarget::All),
    ));

    info!("Creating a game scene for {}", args.scene_key);
}

fn on_new_client_added_hook(event: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(event.entity).insert((
        ReplicationSender::new(SEND_INTERVAL, SendUpdatesMode::SinceLastAck, false),
        ReplicationReceiver::default(),
    ));

    info!(
        "New link added: {:?} is now a replication sender/receiver.",
        event.entity
    );
}

fn on_new_client_connected_hook(
    event: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
) -> Result {
    let client_id = query.get(event.entity)?;
    let client_id = client_id.0;
    let entity = commands
        .spawn((
            Player,
            PlayerId(client_id.to_bits()),
            CharacterController::default(),
            Collider::cylinder(0.4, 1.2),
            Transform::from_xyz(0.0, 200.0, 0.0),
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                owner: event.entity,
                lifetime: default(),
            },
        ))
        .id();

    // bevy_ahoy's CharacterControllerCamera doesn't actually has to be camera,
    // it's more of a visor entity -- it will receive RotateCamera inputs,
    // required for correct movement
    commands.spawn((
        CharacterControllerCameraOf::new(entity),
        ControlledBy {
            owner: event.entity,
            lifetime: default(),
        },
    ));

    info!(
        "Create player entity {:?} for client {:?}",
        entity, client_id
    );
    Ok(())
}
