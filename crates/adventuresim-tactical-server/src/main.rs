//! Tactical Server - Replicon + Aeronet websocket game server.

mod bot;
mod combat;
mod doors;
mod equipment;
mod mission;
mod player_projection;
mod stdb;
mod terrain_collision;

use std::{net::SocketAddr, num::NonZeroU32, path::PathBuf};

use adventuresim_building_generator::{
    BuildingCollision, compile_building_collision, generate as generate_building,
};
use adventuresim_stdb_client::*;
use adventuresim_tactical_core::{physics::AdventureSimulatorPhysicsPlugin, prelude::*};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::LocalAddr,
    bevy_replicon::prelude::{Replicated, ServerState},
    prelude::{AdventureSimulatorNetPlugins, AdventureSimulatorServer, SceneVistaBundle},
};
#[cfg(feature = "debug")]
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::FromClient,
    prelude::{DebugDumpWorldRequest, DebugGameTimeScaleRequest},
};
use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
#[cfg(feature = "debug")]
use bevy::world_serialization::DynamicWorldBuilder;
use clap::{ArgAction, Parser};

#[cfg(feature = "debug")]
use crate::player_projection::{
    bind_dumped_character_on_join, mark_loaded_items_replicated, on_client_disconnected_standalone,
    on_join_request_standalone,
};
use crate::{
    combat::CombatSet,
    mission::{
        MissionState, check_mission_timeout, check_terminal_combat_outcome,
        fail_stalled_terminal_submission, finish_terminal_presentation,
        process_terminal_submission_results,
    },
    player_projection::{
        PlayerProjectionSet, expire_disconnected_players, on_client_disconnected, on_join_request,
        on_player_added, on_player_input, restore_authoritative_movement_intent,
        spawn_connected_players, trace_authoritative_quickstep_after_collision,
        update_attack_facing_targets, update_character_motion_snapshots,
        update_skeleton_locomotion,
    },
    stdb::{SpacetimeDb, SpacetimeDbReady},
};

const MISSION_TIMEOUT_SECS: f32 = 300.0;
const DEFAULT_SCENE_INPUT: &str = "dense-woodland";
const DEFAULT_COMBAT_CONFIG: &str = "content/tactical/combat.yaml";

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
    #[arg(long, value_parser = bot::resolve_scene_fixture)]
    scene_input: Option<PathBuf>,
    /// Versioned tactical combat tuning loaded once for this server process.
    #[arg(long)]
    combat_config: Option<PathBuf>,
    /// Standalone enemy roster; selected independently from the scene input.
    #[arg(long, value_parser = bot::load_enemy_fixture)]
    enemy_fixture: Option<adventuresim_core::tactical_fixture::TacticalEnemyFixture>,
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
    /// Port to expose the Bevy Remote Protocol (BRP) HTTP JSON-RPC endpoint
    /// on for CLI-driven inspection/testing. Disabled unless set.
    #[cfg(feature = "debug")]
    #[arg(long)]
    brp_port: Option<u16>,
    /// Path to a `.scn.ron` world dump (see `DebugDumpWorldRequest`) to load
    /// at startup in place of generating fresh procedural terrain.
    #[cfg(feature = "debug")]
    #[arg(long)]
    world_dump: Option<std::path::PathBuf>,
}

fn default_scene_input_path() -> PathBuf {
    bot::resolve_scene_fixture(DEFAULT_SCENE_INPUT).expect("fixture path resolution is infallible")
}

fn default_combat_config_path() -> PathBuf {
    let working_directory_path = PathBuf::from(DEFAULT_COMBAT_CONFIG);
    if working_directory_path.is_file() {
        return working_directory_path;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(DEFAULT_COMBAT_CONFIG)
}

fn load_combat_config(path: &std::path::Path) -> Result<TacticalCombatConfig, String> {
    const MAX_COMBAT_CONFIG_BYTES: u64 = 64 * 1024;
    let length = std::fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?
        .len();
    if length == 0 || length > MAX_COMBAT_CONFIG_BYTES {
        return Err("combat config must contain between 1 byte and 64 KiB".into());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let config: TacticalCombatConfig = serde_saphyr::from_str(&text)
        .map_err(|error| format!("{} is not valid YAML: {error}", path.display()))?;
    config
        .validate()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(config)
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

    #[test]
    fn committed_combat_config_matches_canonical_defaults() {
        let loaded = load_combat_config(&default_combat_config_path())
            .expect("committed tactical combat config should remain valid");
        assert_eq!(loaded, TacticalCombatConfig::default());
    }

    #[test]
    fn combat_tuning_is_read_from_the_runtime_yaml_file() {
        let canonical = std::fs::read_to_string(default_combat_config_path())
            .expect("committed tactical combat config should be readable");
        let modified = canonical.replacen(
            "armed_attack_energy_transfer: 0.4",
            "armed_attack_energy_transfer: 0.35",
            1,
        );
        assert_ne!(modified, canonical, "test must modify combat resolution");
        let path = std::env::temp_dir().join(format!(
            "fabelgeist-combat-config-runtime-{}.yaml",
            std::process::id()
        ));
        std::fs::write(&path, modified).expect("temporary combat config should be writable");
        let loaded = load_combat_config(&path).expect("modified runtime YAML should load");
        std::fs::remove_file(&path).expect("temporary combat config should be removable");

        assert_eq!(loaded.resolution.armed_attack_energy_transfer, 0.35);
        assert_ne!(loaded, TacticalCombatConfig::default());
    }
}

fn main() {
    let args = bot::apply_enemy_fixture(Args::parse());
    #[cfg(feature = "debug")]
    let brp_port = args.brp_port;
    #[cfg(feature = "debug")]
    let standalone = args.world_dump.is_some();
    #[cfg(not(feature = "debug"))]
    let standalone = false;

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
    let combat_config_path = args
        .combat_config
        .clone()
        .unwrap_or_else(default_combat_config_path);
    let combat_config = match load_combat_config(&combat_config_path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("refusing invalid tactical combat config: {error}");
            std::process::exit(2);
        }
    };
    combat_config
        .install_runtime_snapshot()
        .expect("loaded tactical combat config was validated");
    let combat_config_digest = combat_config
        .digest()
        .expect("loaded tactical combat config was validated");
    eprintln!(
        "[startup] tactical combat config path={} digest={combat_config_digest}",
        combat_config_path.display()
    );
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
        distant_buildings: loaded_scene_input.distant_buildings.clone(),
        lods: loaded_scene_input.vista.lods.clone(),
    });
    let mut app = App::new();
    app.insert_resource(combat_config);
    // Registered separately from Aeronet's session-despawn observer. The
    // reconnect snapshot is owned before either observer's commands apply, so
    // correctness does not depend on their registration order.
    if !standalone {
        app.add_observer(on_client_disconnected);
    }
    app.add_plugins(
        DefaultPlugins.set(bevy::log::LogPlugin {
            filter:
                "adventuresim_tactical_server=info,quickstep_trace=info,bevy_app=warn,bevy_ecs=warn"
                    .to_string(),
            ..default()
        }),
    )
    .add_plugins((
        AdventureSimulatorCorePlugins
            .build()
            .set(AdventureSimulatorPhysicsPlugin {
                enable_simulation: true,
                enable_presentation_simulation: false,
            }),
        AdventureSimulatorNetPlugins,
    ))
    .add_plugins((
        combat::CombatPlugin,
        equipment::TacticalEquipmentPlugin,
        (bot::BotPlugin, doors::DoorServerPlugin),
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
        FixedPostUpdate,
        (
            restore_authoritative_movement_intent
                .before(AdventureSimulatorPhysicsSet::ApplyCharacterMotor),
            (
                trace_authoritative_quickstep_after_collision,
                update_attack_facing_targets,
                update_skeleton_locomotion,
                update_character_motion_snapshots,
            )
                .chain()
                .after(AhoySystems::MoveCharacters),
        ),
    )
    .add_systems(OnEnter(ServerState::Running), on_server_started)
    .add_observer(on_player_input)
    .add_observer(on_player_added)
    .add_observer(on_scene_terrain_added)
    .add_observer(on_scene_building_added);

    // Standalone (`--world-dump`) runs never touch SpacetimeDB: a loaded
    // dump already carries every bit of gameplay state a live stdb
    // connection would otherwise provide (identity, skills, position, live
    // combat state, bot AI markers), so the whole strategic-authority round
    // trip (join authorization, mission outcome submission, bot/character
    // spawning from `ConnectedPlayer` rows) is replaced by purely local
    // logic instead.
    if standalone {
        #[cfg(feature = "debug")]
        app.add_systems(Startup, setup_server)
            .add_systems(
                Update,
                (
                    (check_terminal_combat_outcome, check_mission_timeout)
                        .chain()
                        .after(CombatSet::Condition),
                    fail_stalled_terminal_submission.before(check_terminal_combat_outcome),
                    finish_terminal_presentation.after(check_mission_timeout),
                    bind_dumped_character_on_join
                        .in_set(PlayerProjectionSet::Spawn)
                        .before(check_terminal_combat_outcome),
                ),
            )
            .add_observer(on_join_request_standalone)
            .add_observer(on_client_disconnected_standalone);
    } else {
        app.add_plugins(stdb::SpacetimeDbPlugin)
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
            .add_observer(on_join_request);
    }

    #[cfg(feature = "debug")]
    app.add_observer(on_debug_game_time_scale_request);
    #[cfg(feature = "debug")]
    app.add_observer(on_debug_dump_world_request);
    #[cfg(feature = "debug")]
    if let Some(port) = brp_port {
        app.add_plugins((
            bevy::remote::RemotePlugin::default(),
            bevy::remote::http::RemoteHttpPlugin::default().with_port(port),
        ));
    }
    #[cfg(feature = "debug")]
    app.add_systems(
        OnEnter(ServerState::Running),
        load_world_dump.after(on_server_started),
    );
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

/// Serializes only the "core" reflectable character/level components (see
/// [`on_player_added`](player_projection::on_player_added) and
/// [`on_scene_terrain_added`] for the corresponding derive-the-rest hooks)
/// on every entity to a `.scn.ron` file under `world_dumps/`.
///
/// Deliberately an *allowlist*, not "every reflected component/resource on
/// every entity" - `reflect_auto_register` registers third-party types for
/// all sorts of unrelated reasons (BRP inspection, entity mapping, engine
/// bookkeeping), and any of them landing in the dump breaks the *entire*
/// dump if it lacks full reflection-based serialization support, not just
/// that one field. Both `Time<Real>` (a resource `.extract_resources()`
/// pulled in from `TimePlugin`) and `aeronet_io::Session` (a component on
/// every connected client's entity) hit exactly this - each contains a
/// `bevy_platform::time::Instant` with no `ReflectSerialize` registered.
/// Neither is reachable from a bare `App::new()` (what this file's own
/// tests use), which is why this took two rounds to actually surface.
#[cfg(feature = "debug")]
fn on_debug_dump_world_request(_request: On<FromClient<DebugDumpWorldRequest>>, world: &World) {
    let entities: Vec<Entity> = world
        .archetypes()
        .iter()
        .flat_map(|archetype| archetype.entities().iter().map(|entity| entity.id()))
        .collect();
    let registry = world.resource::<AppTypeRegistry>().read();
    // The filter must be set up *before* `extract_entities` - it's applied
    // immediately as entities are extracted, not lazily at `build()`.
    let scene = DynamicWorldBuilder::from_world(world, &registry)
        .deny_all_components()
        .allow_component::<Player>()
        .allow_component::<CharacterId>()
        .allow_component::<Skills>()
        .allow_component::<Limbs>()
        .allow_component::<TacticalAttributes>()
        .allow_component::<Stats>()
        .allow_component::<TacticalCombatState>()
        .allow_component::<TacticalCombatSide>()
        .allow_component::<Transform>()
        .allow_component::<SceneId>()
        .allow_component::<SceneTerrain>()
        .allow_component::<SceneBuilding>()
        .allow_component::<crate::bot::MissionEnemy>()
        .allow_component::<crate::bot::OffensiveCombatAi>()
        .allow_component::<crate::bot::CombatantBehaviorPackages>()
        .allow_component::<crate::bot::ReactiveDefenseAi>()
        .allow_component::<crate::bot::DefenseChances>()
        .allow_component::<crate::bot::RaisedGuardAi>()
        .allow_component::<crate::bot::AimAtNearestOpponentAi>()
        .allow_component::<crate::bot::RecoverToUprightAi>()
        // Inventory items are separate entities (linked back to their
        // owning character via `ItemOf`), not components on the character
        // itself - without these, a dumped/loaded character's equipment is
        // silently empty. `InventoryItems` (the reverse side of the
        // `ItemOf` relationship) MUST be captured too: scene loading
        // applies components with `RelationshipHookMode::Skip`, so nothing
        // reconstructs the reverse side on load - a dump carries both sides
        // of the relationship verbatim, exactly like bevy's own
        // `ChildOf`/`Children` pair in dynamic scenes.
        .allow_component::<InventoryItems>()
        .allow_component::<ItemOf>()
        .allow_component::<TacticalItemQuantity>()
        .allow_component::<ItemProperties>()
        .allow_component::<WeaponItem>()
        .allow_component::<ShieldItem>()
        .allow_component::<ArmorItem>()
        .allow_component::<EquipmentTopology>()
        .allow_component::<EquipSlot>()
        .extract_entities(entities.into_iter())
        .build();
    let ron = match scene.serialize(&registry) {
        Ok(ron) => ron,
        Err(error) => {
            error!(?error, "Failed to serialize world dump");
            return;
        }
    };
    drop(registry);

    let dir = std::path::Path::new("world_dumps");
    if let Err(error) = std::fs::create_dir_all(dir) {
        error!(?error, "Failed to create world_dumps directory");
        return;
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("world_dump_{timestamp}.scn.ron"));
    match std::fs::write(&path, ron) {
        Ok(()) => info!(path = %path.display(), "Dumped world state"),
        Err(error) => error!(?error, path = %path.display(), "Failed to write world dump"),
    }
}

/// [`WorldDeserializer`](bevy::world_serialization::serde::WorldDeserializer)
/// requires a [`LoadFromPath`](bevy::asset::LoadFromPath) to resolve any
/// `Handle<T>` fields found while deserializing. None of the allowlisted
/// components in [`on_debug_dump_world_request`] hold asset handles, so this
/// should never actually be called.
#[cfg(feature = "debug")]
struct NoAssetHandlesInDump;

#[cfg(feature = "debug")]
impl bevy::asset::LoadFromPath for NoAssetHandlesInDump {
    fn load_from_path_erased(
        &mut self,
        _type_id: std::any::TypeId,
        _path: bevy::asset::AssetPath<'static>,
    ) -> bevy::asset::UntypedHandle {
        unimplemented!("world dumps do not contain asset handles")
    }
}

/// Loads a `.scn.ron` file written by [`on_debug_dump_world_request`] and
/// applies its reflected entities/resources to the running world, mirroring
/// `bevy_world_serialization::WorldAssetLoader` but reading directly from
/// disk instead of through `AssetServer` (dumps live outside the `assets/`
/// root). A no-op unless `--world-dump` was passed.
#[cfg(feature = "debug")]
fn load_world_dump(world: &mut World) {
    let Some(path) = world.resource::<Args>().world_dump.clone() else {
        return;
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            error!(?error, path = %path.display(), "Failed to read world dump");
            return;
        }
    };
    let registry = world.resource::<AppTypeRegistry>().0.clone();
    let scene = {
        let registry = registry.read();
        let mut deserializer = match ron::de::Deserializer::from_bytes(&bytes) {
            Ok(deserializer) => deserializer,
            Err(error) => {
                error!(?error, path = %path.display(), "Failed to parse world dump RON");
                return;
            }
        };
        let scene_deserializer = bevy::world_serialization::serde::WorldDeserializer {
            type_registry: &registry,
            load_from_path: &mut NoAssetHandlesInDump,
        };
        match serde::de::DeserializeSeed::deserialize(scene_deserializer, &mut deserializer)
            .map_err(|error| deserializer.span_error(error))
        {
            Ok(scene) => scene,
            Err(error) => {
                error!(?error, path = %path.display(), "Failed to deserialize world dump");
                return;
            }
        }
    };

    let mut entity_map = bevy::ecs::entity::EntityHashMap::default();
    let result = scene.write_to_world(world, &mut entity_map);
    // Every loaded entity that had `Player` (or `SceneTerrain`) just
    // triggered `on_player_added`/`on_scene_terrain_added`, which queue
    // their derived components via `Commands` - flush now so the world is
    // fully ready before anything else runs this frame.
    world.flush();
    mark_loaded_items_replicated(world, entity_map.values());
    match result {
        Ok(()) => info!(
            path = %path.display(),
            entities = entity_map.len(),
            "Loaded world dump"
        ),
        Err(error) => error!(?error, path = %path.display(), "Failed to apply world dump"),
    }
}

#[cfg(all(test, feature = "debug"))]
mod debug_dump_world_tests {
    use std::{collections::HashSet, path::PathBuf};

    use adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId;

    use super::*;

    const DUMP_DIR: &str = "world_dumps";

    /// Both tests below write timestamped files into the same real
    /// `world_dumps/` directory, so they must not run concurrently with each
    /// other or one can mistake the other's file for its own.
    static DUMP_DIR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn dump_dir_snapshot() -> HashSet<PathBuf> {
        std::fs::read_dir(DUMP_DIR)
            .map(|entries| entries.filter_map(|entry| entry.ok().map(|entry| entry.path())))
            .into_iter()
            .flatten()
            .collect()
    }

    fn newest_dump_file(before: &HashSet<PathBuf>) -> PathBuf {
        let new_files: Vec<_> = std::fs::read_dir(DUMP_DIR)
            .expect("world_dumps directory should have been created")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| !before.contains(path))
            .collect();
        let [new_file] = new_files.as_slice() else {
            panic!("expected exactly one new dump file, got {new_files:?}");
        };
        new_file.clone()
    }

    fn test_args(world_dump: Option<PathBuf>) -> Args {
        Args {
            addr: "127.0.0.1:0".parse().unwrap(),
            mission_id: "mission:test".into(),
            tactical_claim: String::new(),
            scene_key: "woodland".into(),
            scene_input: None,
            enemy_fixture: None,
            required_enemy_kills: 1,
            expected_party_members: 1,
            enemy_combat_scale_bps: 0,
            spacetimedb_url: String::new(),
            spacetimedb_module: String::new(),
            timeout: 0.0,
            no_timeout: true,
            brp_port: None,
            world_dump,
            combat_config: None,
        }
    }

    #[test]
    fn dump_request_serializes_reflected_components_to_a_file() {
        let _guard = DUMP_DIR_LOCK.lock().unwrap();
        let mut app = App::new();
        app.add_observer(on_debug_dump_world_request);
        app.world_mut().spawn((
            Player {
                name: "Debug Dump Fixture".to_string(),
            },
            CharacterId(4242),
        ));

        let before = dump_dir_snapshot();
        app.world_mut().trigger(FromClient {
            client_id: ClientId::Server,
            message: DebugDumpWorldRequest,
        });
        let new_file = newest_dump_file(&before);

        let contents = std::fs::read_to_string(&new_file).expect("dump file should be readable");
        assert!(contents.contains("Debug Dump Fixture"));
        assert!(contents.contains("4242"));

        std::fs::remove_file(&new_file).ok();
    }

    #[test]
    fn dump_request_does_not_choke_on_built_in_engine_resources() {
        // A bare `App::new()` has no `Time<Real>` resource at all, so it
        // can't catch a scene builder that fails as soon as one exists -
        // `MinimalPlugins` (which every real server transitively includes
        // via `DefaultPlugins`) inserts it via `TimePlugin`, reproducing the
        // exact condition that broke every dump on a real running server.
        let _guard = DUMP_DIR_LOCK.lock().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_observer(on_debug_dump_world_request);
        app.world_mut().spawn((
            Player {
                name: "Real Server Fixture".to_string(),
            },
            CharacterId(1),
        ));

        let before = dump_dir_snapshot();
        app.world_mut().trigger(FromClient {
            client_id: ClientId::Server,
            message: DebugDumpWorldRequest,
        });
        let new_file = newest_dump_file(&before);

        let contents = std::fs::read_to_string(&new_file).expect("dump file should be readable");
        assert!(contents.contains("Real Server Fixture"));

        std::fs::remove_file(&new_file).ok();
    }

    #[test]
    fn dump_request_excludes_reflected_components_outside_the_core_allowlist() {
        // `Replicated` is real, already reflect-registered, and completely
        // unrelated to this dump's job (it's a replication-transport
        // concern, not character/level data) - a stand-in for the kind of
        // third-party-registered type (`aeronet_io::Session`, `Time<Real>`)
        // that breaks the *entire* dump if it ever gets captured despite
        // lacking full reflection-based serialization support. Confirms the
        // component allowlist actually excludes it rather than just
        // happening to work for this particular type.
        let _guard = DUMP_DIR_LOCK.lock().unwrap();
        let mut app = App::new();
        app.add_observer(on_debug_dump_world_request);
        app.world_mut().spawn((
            Player {
                name: "Allowlist Fixture".to_string(),
            },
            CharacterId(2),
            Replicated,
        ));

        let before = dump_dir_snapshot();
        app.world_mut().trigger(FromClient {
            client_id: ClientId::Server,
            message: DebugDumpWorldRequest,
        });
        let new_file = newest_dump_file(&before);

        let contents = std::fs::read_to_string(&new_file).expect("dump file should be readable");
        assert!(contents.contains("Allowlist Fixture"));
        assert!(
            !contents.contains("Replicated"),
            "components outside the core allowlist should not be dumped"
        );

        std::fs::remove_file(&new_file).ok();
    }

    #[test]
    fn world_dump_round_trips_through_load() {
        let _guard = DUMP_DIR_LOCK.lock().unwrap();
        let mut app = App::new();
        app.add_observer(on_debug_dump_world_request);
        let player_entity = app
            .world_mut()
            .spawn((
                Player {
                    name: "Round Trip Fixture".to_string(),
                },
                CharacterId(777),
            ))
            .id();
        // Inventory items are separate entities linked via `ItemOf`, not
        // components on the character - they must round-trip too. `ItemOf`
        // first, then `EquipSlot`, matching every live equip path, so the
        // equip hook derives `InventoryItems::holding_weapon` on the
        // capture side.
        let sword_entity = app
            .world_mut()
            .spawn((
                ItemOf(player_entity),
                TacticalItemQuantity::default(),
                ItemProperties {
                    id: "sword".to_string(),
                    weight: 1.2,
                },
                WeaponItem {
                    striking_material:
                        adventuresim_core::item_catalog_schema::EquipmentMaterial::RoughSteel,
                    skill_weights: [0.0; 9],
                    accuracy: 1.0,
                    penetration: 1.0,
                    reach: 0.8,
                    grip_to_tip_m: 0.8,
                    moment_of_inertia_kg_m2: 0.0,
                    precise: false,
                    melee: true,
                    ranged: false,
                    blunt: false,
                    slash: true,
                    pierce: false,
                    swing_precision: 0.0,
                    stab_precision: 0.0,
                    prefers_stab: false,
                },
            ))
            .id();
        app.world_mut()
            .entity_mut(sword_entity)
            .insert(EquipSlot::HoldingRight);

        let before = dump_dir_snapshot();
        app.world_mut().trigger(FromClient {
            client_id: ClientId::Server,
            message: DebugDumpWorldRequest,
        });
        let dump_path = newest_dump_file(&before);

        let mut load_app = App::new();
        load_app.insert_resource(test_args(Some(dump_path.clone())));
        load_world_dump(load_app.world_mut());

        let mut query = load_app
            .world_mut()
            .query::<(Entity, &Player, &CharacterId)>();
        let (loaded_entity, player, _) = query
            .iter(load_app.world())
            .find(|(_, _, id)| id.0 == 777)
            .expect("loaded world should contain the dumped entity");
        assert_eq!(player.name, "Round Trip Fixture");

        let mut items = load_app
            .world_mut()
            .query::<(Entity, &ItemOf, &ItemProperties)>();
        let (loaded_item, item_of, _) = items
            .iter(load_app.world())
            .find(|(_, _, properties)| properties.id == "sword")
            .expect("loaded world should contain the dumped inventory item");
        assert_eq!(
            item_of.0, loaded_entity,
            "the item's owner reference should point at the loaded character"
        );

        // Scene loading applies components with relationship hooks
        // silenced, so nothing rebuilds `InventoryItems` on load - the dump
        // must carry it, entity-mapped, like bevy's own `Children`. A
        // character loaded without it is silently naked and unarmed.
        let inventory = load_app
            .world()
            .entity(loaded_entity)
            .get::<InventoryItems>()
            .expect("the dumped character's InventoryItems should round-trip");
        assert!(
            inventory.iter().any(|item| item == loaded_item),
            "the loaded InventoryItems should reference the loaded (remapped) item entity"
        );
        assert_eq!(
            inventory.holding_weapon(),
            Some(loaded_item),
            "holding_weapon should round-trip and be remapped to the loaded item entity"
        );

        std::fs::remove_file(dump_path).ok();
    }
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

/// Fires whenever `SceneTerrain` lands on any entity - via fresh procedural
/// generation in `on_server_started` or a loaded world dump
/// (`load_world_dump`). Derives the physics collider from the heightmap and
/// adds the replication marker, since `avian3d::Collider` isn't reflectable
/// and so never survives a dump on its own; a dump only needs to carry the
/// "core" `SceneId`/`SceneTerrain`/`Transform`.
fn on_scene_terrain_added(
    event: On<Add, SceneTerrain>,
    mut commands: Commands,
    query: Query<(&SceneTerrain, Option<&FaultScarpRecipe>)>,
) -> Result {
    let (terrain, recipe) = query.get(event.entity)?;
    let collider = terrain_collision::collider(terrain, recipe)?;
    commands.entity(event.entity).insert((
        Replicated,
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        collider,
    ));
    Ok(())
}

fn on_scene_building_added(
    event: On<Add, SceneBuilding>,
    mut commands: Commands,
    buildings: Query<(&SceneBuilding, &Transform)>,
) -> Result {
    let (building, transform) = buildings.get(event.entity)?;
    let plan = generate_building(&building.program)?;
    let collision = compile_building_collision(&plan);
    doors::spawn_building_doors(
        &mut commands,
        event.entity,
        building,
        transform,
        &plan,
        &collision,
    );
    commands.entity(event.entity).insert((
        Replicated,
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        tactical_building_collider(&collision),
    ));
    Ok(())
}

fn tactical_building_collider(collision: &BuildingCollision) -> Collider {
    let local_origin = collision.bounds.centre();
    Collider::compound(
        collision
            .cuboids
            .iter()
            .map(|cuboid| {
                let translation = cuboid.centre - local_origin;
                let rotation = Quat::from_rotation_y(cuboid.yaw_radians)
                    * Quat::from_rotation_x(cuboid.crossfall_radians)
                    * Quat::from_rotation_z(cuboid.longfall_radians);
                (
                    translation,
                    rotation,
                    Collider::cuboid(cuboid.size.x, cuboid.size.y, cuboid.size.z),
                )
            })
            .collect(),
    )
}

fn on_server_started(
    args: Res<Args>,
    scene_input: Res<LoadedSceneInput>,
    conn: Option<Res<SpacetimeDb>>,
    mut commands: Commands,
    server_addr: Single<&LocalAddr, With<AdventureSimulatorServer>>,
) -> Result {
    info!("Server opened on {:?}", **server_addr);

    #[cfg(feature = "debug")]
    let generate_terrain = args.world_dump.is_none();
    #[cfg(not(feature = "debug"))]
    let generate_terrain = true;

    let input = &scene_input.0;
    // World-bounds walls are a pure function of the scene input's playable
    // area, so they're always (re)created rather than carried by a dump.
    let scene_width =
        f32::from(input.playable.width.saturating_sub(1)) * input.playable.spacing_metres;
    let scene_depth =
        f32::from(input.playable.depth.saturating_sub(1)) * input.playable.spacing_metres;

    if generate_terrain {
        info!("Creating a game scene for {}", args.scene_key);
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
            levelled_building_samples = generated.repairs.levelled_building_samples,
            removed_building_obstacles = generated.repairs.removed_building_obstacles,
            "Loaded deterministic tactical scene input"
        );
        let scene_id = input.scene_key.clone();
        let terrain = generated.terrain;
        let terrain_patch = generated.terrain_patch;
        let ground = generated.ground;
        let environment = input.environment_snapshot(generated.digest);
        let obstacles = generated.obstacles;
        let buildings = generated.buildings;
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
        for building in buildings {
            let collision_centre = building.collision.bounds.centre();
            let local_floor_offset = collision_centre.y - building.collision.bounds.min.y;
            commands.spawn((
                Name::new(format!("Tactical building {}", building.placement.id)),
                SceneBuilding {
                    id: building.placement.id,
                    program: building.placement.program,
                    quarter_turns: building.placement.quarter_turns,
                },
                Transform::from_xyz(
                    building.placement.centre_metres.x,
                    building.pad_elevation_metres + local_floor_offset,
                    building.placement.centre_metres.y,
                )
                .with_rotation(Quat::from_rotation_y(
                    f32::from(building.placement.quarter_turns) * core::f32::consts::FRAC_PI_2,
                )),
            ));
        }
        terrain_collision::spawn_scene(
            &mut commands,
            scene_id,
            terrain,
            ground,
            environment,
            terrain_patch.as_ref(),
            input.fault_scarp,
        );
    }
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
    if let Some(conn) = conn {
        info!("Creating tactical server in stdb...");
        conn.reducers().create_tactical_server_for_request(
            args.mission_id.clone(),
            args.tactical_claim.clone(),
            args.addr.to_string(),
            default(),
        )?;
        // Strategic authority enrolls the mission's exact durable enemy
        // roster as part of server creation. ConnectedPlayer delivery
        // spawns those rows.
    }
    Ok(())
}
