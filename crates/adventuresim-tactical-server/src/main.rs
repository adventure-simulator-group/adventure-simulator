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
const DEFAULT_SCENE_INPUT: &str = "assets/tactical-scenes/dense-woodland.json";

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
    #[arg(long, default_value = "woodland")]
    scene_key: String,
    /// Exact versioned scene input. Defaults to the committed dense woodland
    /// fixture for standalone tactical development.
    #[arg(long)]
    scene_input: Option<PathBuf>,
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

fn default_scene_input_path() -> PathBuf {
    let working_directory_path = PathBuf::from(DEFAULT_SCENE_INPUT);
    if working_directory_path.is_file() {
        return working_directory_path;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(DEFAULT_SCENE_INPUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_default_is_the_dense_woodland_fixture() {
        let input = TacticalSceneInput::load(&default_scene_input_path())
            .expect("default tactical scene input should remain valid");

        assert_eq!(input.scene_key, "woodland");
        assert_eq!(
            input.source,
            SceneSource::SyntheticFixture("dense-woodland".into())
        );
    }
}

fn main() {
    let args = Args::parse();
    let scene_input_path = args
        .scene_input
        .clone()
        .unwrap_or_else(default_scene_input_path);
    let loaded_scene_input = match TacticalSceneInput::load(&scene_input_path) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("refusing invalid tactical scene input: {error}");
            std::process::exit(2);
        }
    };
    let scene_vista_bundle = Some(SceneVistaBundle {
        scene_digest: loaded_scene_input
            .digest()
            .expect("loaded scene input was validated"),
        playable_half_extent_metres: Vec2::new(
            f32::from(loaded_scene_input.playable.width.saturating_sub(1))
                * loaded_scene_input.playable.spacing_metres
                * 0.5,
            f32::from(loaded_scene_input.playable.depth.saturating_sub(1))
                * loaded_scene_input.playable.spacing_metres
                * 0.5,
        ),
        lods: loaded_scene_input.vista.lods.clone(),
    });
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(bevy::log::LogPlugin {
        filter: "adventuresim_tactical_server=info,bevy_app=warn,bevy_ecs=warn".to_string(),
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
struct LoadedSceneInput(TacticalSceneInput);

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
    let input = &scene_input.0;
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
    let scene_id = input.scene_key.clone();
    let terrain = generated.terrain;
    let ground = generated.ground;
    let environment = input.environment_snapshot(generated.digest);
    let obstacles = generated.obstacles;
    let obstacle_spacing = input.playable.spacing_metres;
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
            GeneratedObstacle::Rock { x, z, recipe } => (
                x,
                z,
                SceneObstacle::Rock(recipe),
                Collider::sphere(recipe.collision_radius_metres()),
                recipe.collision_radius_metres(),
                "rock",
            ),
        };
        let x = f32::from(grid_x) * obstacle_spacing - terrain.width() * 0.5;
        let z = f32::from(grid_z) * obstacle_spacing - terrain.depth() * 0.5;
        let y = terrain.height_at(Vec2::new(x, z)).unwrap_or_default() + height_offset;
        let yaw = match kind {
            SceneObstacle::Rock(recipe) => {
                (recipe.seed >> 40) as f32 / ((1_u32 << 24) - 1) as f32 * core::f32::consts::TAU
            }
            SceneObstacle::Tree => 0.0,
        };
        commands.spawn((
            Replicated,
            Name::new(format!("Tactical scene {label}")),
            kind,
            RigidBody::Static,
            CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
            collider,
            Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(yaw)),
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
    scene.insert(environment);
    let scene_width =
        f32::from(input.playable.width.saturating_sub(1)) * input.playable.spacing_metres;
    let scene_depth =
        f32::from(input.playable.depth.saturating_sub(1)) * input.playable.spacing_metres;
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
