//! Tactical Server - Replicon + Aeronet websocket game server.

mod bot;
mod combat;
mod mission;
mod player_projection;
mod stdb;
mod terrain;

use std::{net::SocketAddr, num::NonZeroU32};

use adventuresim_stdb_client::*;
use adventuresim_tactical_core::{physics::AdventureSimulatorPhysicsPlugin, prelude::*};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::LocalAddr,
    bevy_replicon::prelude::{Replicated, ServerState},
    prelude::{AdventureSimulatorNetPlugins, AdventureSimulatorServer},
};
use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
use clap::{ArgAction, Parser};

use crate::{
    combat::CombatSet,
    mission::{
        MissionState, check_mission_timeout, check_terminal_combat_outcome,
        fail_stalled_terminal_submission, finish_terminal_presentation,
        process_terminal_submission_results,
    },
    player_projection::{
        PlayerProjectionSet, on_client_disconnected, on_join_request, on_player_input,
        spawn_connected_players, update_skeleton_locomotion,
    },
    stdb::{SpacetimeDb, SpacetimeDbReady},
    terrain::TerrainGenerator,
};

const MISSION_TIMEOUT_SECS: f32 = 300.0;
const TERRAIN_SIZE: usize = 100;

#[derive(Parser, Debug, Clone, Resource)]
#[command(name = "adventuresim-tactical-server")]
#[command(about = "Tactical mission server for Adventure Simulator")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:6000")]
    addr: SocketAddr,
    #[arg(long)]
    mission_id: String,
    #[arg(long, env = "ADVENTURESIM_TACTICAL_CLAIM", hide_env_values = true)]
    tactical_claim: String,
    #[arg(long)]
    scene_key: String,
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_width: usize,
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_depth: usize,
    #[arg(long)]
    required_enemy_kills: u32,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    expected_party_members: u32,
    #[arg(long)]
    enemy_combat_scale_bps: u32,
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,
    #[arg(long, default_value = "adventuresim-stdb-module")]
    spacetimedb_module: String,
    #[arg(long, default_value_t = MISSION_TIMEOUT_SECS)]
    timeout: f32,
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "timeout")]
    no_timeout: bool,
}

fn main() {
    let args = Args::parse();
    App::new()
        .add_plugins(DefaultPlugins.set(bevy::log::LogPlugin {
            filter: "tactical_server=info,bevy_app=warn,bevy_ecs=warn".to_string(),
            ..default()
        }))
        .add_plugins((
            AdventureSimulatorCorePlugins
                .build()
                .set(AdventureSimulatorPhysicsPlugin {
                    enable_simulation: true,
                }),
            AdventureSimulatorNetPlugins,
        ))
        .add_plugins((
            stdb::SpacetimeDbPlugin,
            combat::CombatPlugin,
            bot::BotPlugin,
        ))
        .insert_resource(MissionState::new(
            (!args.no_timeout)
                .then_some(args.timeout)
                .map(|duration| Timer::from_seconds(duration, TimerMode::Once)),
            args.required_enemy_kills,
            NonZeroU32::new(args.expected_party_members)
                .expect("clap validates at least one expected party member"),
        ))
        .insert_resource(args)
        .add_systems(
            Update,
            (
                (check_terminal_combat_outcome, check_mission_timeout)
                    .chain()
                    .after(CombatSet::Condition)
                    .after(spawn_connected_players)
                    .after(process_terminal_submission_results),
                process_terminal_submission_results.after(stdb::update_spacetimedb),
                fail_stalled_terminal_submission
                    .after(process_terminal_submission_results)
                    .before(check_terminal_combat_outcome),
                finish_terminal_presentation.after(check_mission_timeout),
                (spawn_connected_players, ApplyDeferred)
                    .chain()
                    .in_set(PlayerProjectionSet::Spawn)
                    .after(stdb::update_spacetimedb),
                (setup_server, setup_stdb_callbacks).run_if(resource_added::<SpacetimeDbReady>),
            ),
        )
        .add_systems(OnEnter(ServerState::Running), on_server_started)
        .add_systems(FixedPostUpdate, update_skeleton_locomotion)
        .add_observer(on_join_request)
        .add_observer(on_player_input)
        .add_observer(on_client_disconnected)
        .run();
}

fn setup_server(mut commands: Commands, args: Res<Args>) {
    info!(
        "Starting tactical server for mission '{}'...",
        args.mission_id
    );
    info!("Scene: {}, Address: {}", args.scene_key, args.addr);
    info!(
        "Enemy objective: count={}, scale={} bps",
        args.required_enemy_kills, args.enemy_combat_scale_bps
    );
    commands.spawn(AdventureSimulatorServer { addr: args.addr });
    if !args.no_timeout {
        info!("Will timeout in {} seconds", args.timeout);
    }
}

fn setup_stdb_callbacks(conn: Res<SpacetimeDb>) {
    conn.subscribe_connected_players();
}

fn on_server_started(
    args: Res<Args>,
    conn: Res<SpacetimeDb>,
    mut commands: Commands,
    server_addr: Single<&LocalAddr, With<AdventureSimulatorServer>>,
) -> Result {
    info!("Server opened on {:?}", **server_addr);
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
    commands.spawn((
        Replicated,
        SceneId(args.scene_key.clone()),
        terrain,
        RigidBody::Static,
        terrain_collider,
        Transform::default(),
    ));
    let scene_width = args.scene_width as f32;
    let scene_depth = args.scene_depth as f32;
    commands.spawn((
        RigidBody::Static,
        Transform::default(),
        children![
            (
                Collider::half_space(Vec3::X),
                Transform::from_xyz(-scene_width * 0.5, 0.0, 0.0)
            ),
            (
                Collider::half_space(Vec3::NEG_X),
                Transform::from_xyz(scene_width * 0.5, 0.0, 0.0)
            ),
            (
                Collider::half_space(Vec3::Z),
                Transform::from_xyz(0.0, 0.0, -scene_depth * 0.5)
            ),
            (
                Collider::half_space(Vec3::NEG_Z),
                Transform::from_xyz(0.0, 0.0, scene_depth * 0.5)
            )
        ],
    ));
    info!("Creating tactical server in stdb...");
    conn.reducers().create_tactical_server_for_request(
        args.mission_id.clone(),
        args.tactical_claim.clone(),
        args.addr.to_string(),
        default(),
    )?;
    // Strategic authority enrolls the mission's exact durable enemy roster as
    // part of server creation. ConnectedPlayer delivery spawns those rows.
    Ok(())
}
