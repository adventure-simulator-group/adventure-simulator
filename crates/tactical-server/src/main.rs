//! Tactical Server - Minimal Lightyear game server
//!
//! Simple flow:
//! 1. Started by spawner with mission_id arg
//! 2. Calls tactical_server_ready reducer with connection info
//! 3. Runs game for clients
//! 4. Calls commit_mission reducer on timeout/exit

mod stdb;
mod terrain;

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
use strategic_db_client::*;

use crate::{stdb::SpacetimeDb, terrain::TerrainGenerator};

/// Default [`Args::timeout`] time.
const MISSION_TIMEOUT_SECS: f32 = 300.0; // 5 minutes

/// Level map size.
const TERRAIN_SIZE: usize = 100;

#[derive(Parser, Debug, Clone, Resource)]
#[command(name = "tactical-server")]
#[command(about = "Tactical mission server for Adventure Simulator")]
struct Args {
    /// Address to listen on
    #[arg(long, default_value = "127.0.0.1:6000")]
    addr: SocketAddr,

    /// Unique mission instance ID
    #[arg(long)]
    mission_id: String,

    /// Scene key (e.g., "hills", "desert")
    #[arg(long)]
    scene_key: String,

    /// Scene allowed physical width (x-size).
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_width: usize,

    /// Scene allowed physical depth (z-size).
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_depth: usize,

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
        .add_plugins((AdventureSimulatorCorePlugins, AdventureSimulatorNetPlugins))
        .add_plugins((stdb::SpacetimeDbPlugin,))
        .insert_resource(MissionState {
            timeout: (!args.no_timeout)
                .then_some(args.timeout)
                .map(|duration| Timer::from_seconds(duration, TimerMode::Once)),
            enemies_killed: 0,
            committed: false,
        })
        .insert_resource(args)
        .add_systems(
            Update,
            (
                check_mission_timeout,
                setup_server.run_if(resource_added::<SpacetimeDb>),
            ),
        )
        .add_observer(on_new_client_added_hook)
        .add_observer(on_new_client_connected_hook)
        .add_observer(on_server_started_hook)
        .run();
}

#[derive(Resource)]
struct MissionState {
    timeout: Option<Timer>,
    enemies_killed: u32,
    committed: bool,
}

fn setup_server(mut commands: Commands, args: Res<Args>) -> Result {
    info!(
        "Starting tactical server for mission '{}'...",
        args.mission_id
    );
    info!("Scene: {}, Address: {}", args.scene_key, args.addr);

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
    conn: Res<SpacetimeDb>,
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

    conn.reducers
        .commit_mission(args.mission_id.clone(), success, xp_gained)?;

    info!("Mission committed successfully");
    info!("Shutting down");
    exit.write(AppExit::Success);
    Ok(())
}

fn on_server_started_hook(
    _event: On<Add, server::Started>,
    args: Res<Args>,
    mut conn: ResMut<SpacetimeDb>,
    mut commands: Commands,
) -> Result {
    info!("Creating a game scene for {}", args.scene_key);
    let mut generator = TerrainGenerator::from_hash((&args.mission_id, &args.scene_key));
    let (scene_height, gen_period) = match args.scene_key.as_str() {
        "hills" => (30, 200.0),
        "desert" => (2, 30.0),
        id => {
            warn!("Unknown scene: {id}");
            (0, 1.0)
        }
    };
    generator.period = gen_period;
    let terrain = generator.generate(args.scene_width, scene_height, args.scene_depth);
    let terrain_collider = terrain.collider();

    // Spawn physical scene
    commands.spawn((
        SceneId(args.scene_key.clone()),
        terrain,
        RigidBody::Static,
        terrain_collider,
        Transform::from_xyz(0.0, 0.0, 0.0),
        Replicate::to_clients(NetworkTarget::All),
    ));

    // Spawn invisible walls at map ends
    let scene_width = args.scene_width as f32;
    let scene_depth = args.scene_depth as f32;
    commands.spawn((
        RigidBody::Static,
        Transform::default(),
        children![
            (
                Collider::half_space(Vec3::X),
                Transform::from_xyz(-scene_width * 0.5, 0.0, 0.0),
            ),
            (
                Collider::half_space(Vec3::NEG_X),
                Transform::from_xyz(scene_width * 0.5, 0.0, 0.0),
            ),
            (
                Collider::half_space(Vec3::Z),
                Transform::from_xyz(0.0, 0.0, -scene_depth * 0.5),
            ),
            (
                Collider::half_space(Vec3::NEG_Z),
                Transform::from_xyz(0.0, 0.0, scene_depth * 0.5),
            )
        ],
    ));

    info!("Notifying spacetimedb that the server is ready...");

    conn.reducers
        .create_tactical_server(args.mission_id.clone(), args.scene_key.clone())?;
    conn.reducers.tactical_server_ready(
        args.mission_id.clone(),
        args.addr.to_string(),
        default(),
    )?;

    // FIXME: maybe don't do raw sql please ?
    let _handle = conn.subscribe(format!(
        "SELECT * FROM tactical_server WHERE mission_id = '{}'",
        args.mission_id
    ));
    conn.on_insert(
        RemoteTables::tactical_server,
        move |_server: InRef<TacticalServer>, args: Res<Args>| {
            info!("=== Tactical Server Ready ===");
            info!("Mission: {}", args.mission_id);
            info!("Scene: {}", args.scene_key);
        },
    );

    Ok(())
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
    args: Res<Args>,
    mut conn: ResMut<SpacetimeDb>,
) -> Result {
    let client_id = query.get(event.entity)?;
    let client_id = client_id.0;
    let id = client_id.to_bits();
    let owner = event.entity;

    conn.reducers.create_character(id)?;
    conn.reducers.enter_mission(id, args.mission_id.clone())?;

    conn.subscribe(format!("SELECT * FROM character WHERE id = {id}"));
    conn.on_insert(
        RemoteTables::character,
        move |InRef(character): InRef<Character>, mut commands: Commands| {
            let entity = commands
                .spawn((
                    Player {
                        name: character.name.clone(),
                    },
                    PlayerId(id),
                    CharacterController::default(),
                    Collider::cylinder(0.4, 1.2),
                    CollisionMargin(0.01),
                    Transform::from_xyz(0.0, 50.0, 0.0),
                    Replicate::to_clients(NetworkTarget::All),
                    ControlledBy {
                        owner,
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
                    owner,
                    lifetime: default(),
                },
            ));

            info!(
                "Created player entity {:?} for client {:?}: {character:?}",
                entity, client_id
            );
        },
    );

    Ok(())
}
