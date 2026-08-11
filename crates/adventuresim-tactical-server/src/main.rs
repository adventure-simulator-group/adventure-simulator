//! Tactical Server - Replicon + Aeronet websocket game server.

mod bot;
mod combat;
mod equipment;
mod mission;
mod player_projection;
mod stdb;

use std::{net::SocketAddr, num::NonZeroU32, path::PathBuf};

use adventuresim_stdb_client::*;
use adventuresim_tactical_core::{physics::AdventureSimulatorPhysicsPlugin, prelude::*};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::LocalAddr,
    bevy_replicon::prelude::{Replicated, ServerState},
    prelude::{AdventureSimulatorNetPlugins, AdventureSimulatorServer, SceneVistaBundle},
};
#[cfg(feature = "debug")]
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::FromClient, prelude::DebugGameTimeScaleRequest,
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
        PlayerProjectionSet, expire_disconnected_players, on_client_disconnected, on_join_request,
        on_player_input, restore_authoritative_movement_intent, spawn_connected_players,
        update_skeleton_locomotion,
    },
    stdb::{SpacetimeDb, SpacetimeDbReady},
};

const MISSION_TIMEOUT_SECS: f32 = 300.0;
const TERRAIN_SIZE: usize = 100;

#[derive(Parser, Debug, Clone, Resource)]
#[command(name = "adventuresim-tactical-server")]
#[command(about = "Tactical mission server for Fabelgeist")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:6000")]
    addr: SocketAddr,
    #[arg(long)]
    mission_id: String,
    #[arg(long, env = "ADVENTURESIM_TACTICAL_CLAIM", hide_env_values = true)]
    tactical_claim: String,
    #[arg(long)]
    scene_key: String,
    /// Exact versioned scene input. When omitted, the legacy synthetic scene
    /// remains available only for existing tactical development commands.
    #[arg(long)]
    scene_input: Option<PathBuf>,
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
    let loaded_scene_input = match args.scene_input.as_deref().map(TacticalSceneInput::load) {
        Some(Ok(input)) => Some(input),
        Some(Err(error)) => {
            eprintln!("refusing invalid tactical scene input: {error}");
            std::process::exit(2);
        }
        None => None,
    };
    let scene_vista_bundle = loaded_scene_input.as_ref().map(|input| SceneVistaBundle {
        scene_digest: input.digest().expect("loaded scene input was validated"),
        lods: input.vista.lods.clone(),
    });
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(bevy::log::LogPlugin {
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
        equipment::TacticalEquipmentPlugin,
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
    .insert_resource(SceneVistaBundleResource(scene_vista_bundle))
    .insert_resource(LoadedSceneInput(loaded_scene_input))
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
            expire_disconnected_players,
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
    .add_systems(
        FixedPostUpdate,
        (
            restore_authoritative_movement_intent
                .before(AdventureSimulatorPhysicsSet::ApplyMovementSpeed),
            update_skeleton_locomotion.after(AhoySystems::MoveCharacters),
        ),
    )
    .add_observer(on_join_request)
    .add_observer(on_player_input)
    .add_observer(on_client_disconnected);
    #[cfg(feature = "debug")]
    app.add_observer(on_debug_game_time_scale_request);
    app.run();
}

#[derive(Resource)]
struct LoadedSceneInput(Option<TacticalSceneInput>);

#[derive(Resource)]
pub(crate) struct SceneVistaBundleResource(pub(crate) Option<SceneVistaBundle>);

#[cfg(feature = "debug")]
fn on_debug_game_time_scale_request(
    request: On<FromClient<DebugGameTimeScaleRequest>>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let relative_speed = request.relative_speed();
    virtual_time.set_relative_speed(relative_speed);
    info!(relative_speed, "Debug tactical game speed changed");
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
    scene_input: Res<LoadedSceneInput>,
    conn: Res<SpacetimeDb>,
    mut commands: Commands,
    server_addr: Single<&LocalAddr, With<AdventureSimulatorServer>>,
) -> Result {
    info!("Server opened on {:?}", **server_addr);
    info!("Creating a game scene for {}", args.scene_key);
    let (scene_id, terrain, ground, environment, obstacles, obstacle_spacing) =
        if let Some(input) = &scene_input.0 {
            let generated = input.generate()?;
            info!(
                scene_digest = %generated.digest,
                schema_version = input.schema_version,
                generation_version = input.generation_version,
                source = ?input.source,
                obstacles = generated.obstacles.len(),
                upsampled_height_samples = generated.repairs.upsampled_height_samples,
                microrelief_adjusted_samples = generated.repairs.microrelief_adjusted_samples,
                adjusted_height_samples = generated.repairs.adjusted_height_samples,
                repaired_water_samples = generated.repairs.repaired_water_samples,
                removed_corridor_obstacles = generated.repairs.removed_corridor_obstacles,
                "Loaded deterministic tactical scene input"
            );
            (
                input.scene_key.clone(),
                generated.terrain,
                generated.ground,
                Some(input.environment_snapshot(generated.digest)),
                generated.obstacles,
                input.playable.spacing_metres,
            )
        } else {
            warn!("No --scene-input supplied; using legacy synthetic development terrain");
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
            let ground = SceneGround::uniform_for_terrain(
                &terrain,
                GroundSurface {
                    substrate: GroundSubstrate::Soil,
                    cover: GroundCover::TallGrass,
                    cover_density_bps: 9_000,
                    cover_height_cm: 82,
                },
            );
            (
                args.scene_key.clone(),
                terrain,
                ground,
                None,
                Vec::new(),
                1.0,
            )
        };
    for obstacle in obstacles {
        let (grid_x, grid_z, kind, collider, height_offset, label) = match obstacle {
            GeneratedObstacle::Tree { x, z } => (
                x,
                z,
                SceneObstacle::Tree,
                Collider::cylinder(TREE_TRUNK_RADIUS_METRES, TREE_TRUNK_HEIGHT_METRES),
                TREE_TRUNK_HEIGHT_METRES * 0.5,
                "tree trunk",
            ),
            GeneratedObstacle::Rock { x, z } => (
                x,
                z,
                SceneObstacle::Rock,
                Collider::sphere(ROCK_RADIUS_METRES),
                ROCK_RADIUS_METRES,
                "rock",
            ),
        };
        let x = f32::from(grid_x) * obstacle_spacing - terrain.width() * 0.5;
        let z = f32::from(grid_z) * obstacle_spacing - terrain.depth() * 0.5;
        let y = terrain.height_at(Vec2::new(x, z)).unwrap_or_default() + height_offset;
        commands.spawn((
            Replicated,
            Name::new(format!("Tactical scene {label}")),
            kind,
            RigidBody::Static,
            CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
            collider,
            Transform::from_xyz(x, y, z),
        ));
    }
    let terrain_collider = terrain.collider();
    let mut scene = commands.spawn((
        Replicated,
        SceneId(scene_id),
        terrain,
        ground,
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        terrain_collider,
        Transform::default(),
    ));
    if let Some(environment) = environment {
        scene.insert(environment);
    }
    let scene_width = scene_input
        .0
        .as_ref()
        .map_or(args.scene_width as f32, |input| {
            f32::from(input.playable.width.saturating_sub(1)) * input.playable.spacing_metres
        });
    let scene_depth = scene_input
        .0
        .as_ref()
        .map_or(args.scene_depth as f32, |input| {
            f32::from(input.playable.depth.saturating_sub(1)) * input.playable.spacing_metres
        });
    commands.spawn((
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
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
