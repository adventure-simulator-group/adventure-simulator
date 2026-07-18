//! Adventure Simulator shared browser renderer and tactical client.
//!
//! A Bevy-based 3D game client that runs in the browser (WASM).
//! Features:
//! - WASD movement with a capsule character
//! - Camera follow system
//! - Ground plane and skybox
//! - Ready for Lightyear networking integration

use adventuresim_render_contracts::{StartupConfig, StartupMode, TacticalHandoffCommand};
use adventuresim_tactical_core::avian3d::schedule::PhysicsTime;
use adventuresim_tactical_core::physics::AdventureSimulatorPhysicsPlugin;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::*;
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::input_focus::InputDispatchPlugin;
use bevy::light::AtmosphereEnvironmentMapLight;
use bevy::light::light_consts::lux;
use bevy::pbr::{Atmosphere, ScatteringMedium, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy::{
    ecs::schedule::common_conditions::any_with_component,
    input::common_conditions::input_just_pressed,
    window::{CursorGrabMode, CursorOptions},
};
use clap::Parser;
#[cfg(target_family = "wasm")]
use console_error_panic_hook;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "debug")]
mod debug;
mod player;
mod strategic;
mod ui;

static SUSPENDED: AtomicBool = AtomicBool::new(false);
static HANDOFF: Mutex<Option<TacticalHandoffCommand>> = Mutex::new(None);
static HANDOFF_ACCEPTED: AtomicBool = AtomicBool::new(false);
static RENDERER_STATUS: AtomicU8 = AtomicU8::new(0);
static MARKER_SELECTION: Mutex<Option<String>> = Mutex::new(None);

#[cfg(target_family = "wasm")]
const STATUS_STRATEGIC: u8 = 0;
#[cfg(target_family = "wasm")]
const STATUS_ALLOCATING: u8 = 1;
const STATUS_TACTICAL_CONNECTING: u8 = 2;
const STATUS_TACTICAL_CONNECTED: u8 = 3;
const STATUS_TACTICAL_FAILED: u8 = 4;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, States)]
pub(crate) enum RendererMode {
    StrategicMap,
    StrategicScene,
    #[default]
    Tactical,
}
#[derive(Resource, Clone)]
pub(crate) struct RendererConfig(StartupConfig);

#[derive(Component)]
pub(crate) struct TacticalEntity;

#[derive(Component)]
struct TacticalTerrain;

#[derive(Component)]
pub(crate) struct TacticalDerivedEntity;

#[derive(Resource, Clone, Copy)]
struct StrategicReturnMode(Option<RendererMode>);

#[derive(Parser, Debug, Resource)]
#[command(version, about)]
struct Args {
    /// Client ID
    #[arg(long)]
    id: u64,
    /// Server URL or host:port
    #[arg(long)]
    server_addr: String,
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run(Args::parse());
}

#[cfg(target_family = "wasm")]
fn main() {
    // Set up panic hook for better WASM error messages
    console_error_panic_hook::set_once();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_run(args: Vec<String>) {
    run(Args::parse_from(args));
}

/// Starts the one production WASM artifact in one of its versioned modes.
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_run_config(json: &str) -> Result<(), JsValue> {
    let config: StartupConfig =
        serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    config
        .validate()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    run_config(config);
    Ok(())
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_validate_manifest(json: &str) -> Result<(), JsValue> {
    let manifest: adventuresim_render_contracts::MapManifest =
        serde_json::from_str(json).map_err(|error| JsValue::from_str(&error.to_string()))?;
    manifest
        .validate()
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_set_suspended(suspended: bool) {
    SUSPENDED.store(suspended, Ordering::Relaxed);
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_tactical_handoff(json: &str) -> Result<(), JsValue> {
    let command: TacticalHandoffCommand =
        serde_json::from_str(json).map_err(|error| JsValue::from_str(&error.to_string()))?;
    command
        .validate()
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    queue_handoff(command).map_err(JsValue::from_str)
}

#[cfg(any(target_family = "wasm", test))]
fn queue_handoff(command: TacticalHandoffCommand) -> Result<(), &'static str> {
    if HANDOFF_ACCEPTED.swap(true, Ordering::AcqRel) {
        return Err("a tactical handoff is already pending or active");
    }
    let mut mailbox = HANDOFF.lock().map_err(|_| "handoff mailbox unavailable")?;
    if mailbox.is_some() {
        HANDOFF_ACCEPTED.store(false, Ordering::Release);
        return Err("a tactical handoff is already queued");
    }
    *mailbox = Some(command);
    Ok(())
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_renderer_status() -> String {
    match RENDERER_STATUS.load(Ordering::Acquire) {
        STATUS_ALLOCATING => "allocating",
        STATUS_TACTICAL_CONNECTING => "tactical_connecting",
        STATUS_TACTICAL_CONNECTED => "tactical_connected",
        STATUS_TACTICAL_FAILED => "tactical_failed",
        _ => "strategic",
    }
    .into()
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_set_allocating(allocating: bool) {
    if allocating {
        RENDERER_STATUS.store(STATUS_ALLOCATING, Ordering::Release);
    } else {
        let _ = RENDERER_STATUS.compare_exchange(
            STATUS_ALLOCATING,
            STATUS_STRATEGIC,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_take_marker_selection() -> Option<String> {
    MARKER_SELECTION.lock().ok()?.take()
}

fn run(args: Args) {
    run_config(StartupConfig {
        renderer_schema: adventuresim_render_contracts::RENDER_SCHEMA_VERSION,
        canvas_selector: "#game-canvas".into(),
        startup: StartupMode::Tactical {
            player_id: args.id,
            server_url: args.server_addr,
        },
    });
}

fn run_config(config: StartupConfig) {
    let canvas = config.canvas_selector.clone();
    let mode = match &config.startup {
        StartupMode::StrategicMap { .. } => RendererMode::StrategicMap,
        StartupMode::StrategicScene { .. } => RendererMode::StrategicScene,
        StartupMode::Tactical { .. } => RendererMode::Tactical,
    };
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Adventure Simulator - Tactical".into(),
                canvas: Some(canvas),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                present_mode: PresentMode::AutoVsync,
                decorations: false,
                ..default()
            }),
            ..default()
        }),
        FrameTimeDiagnosticsPlugin::default(),
        EnhancedInputPlugin,
        InputDispatchPlugin,
    ))
    .insert_state(mode)
    .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
    .add_systems(
        Update,
        (
            apply_lifecycle,
            drain_handoff,
            publish_connection_status,
            monitor_connection_failure,
        ),
    )
    .insert_resource(RendererConfig(config.clone()));

    let (player_id, server_addr) = match &config.startup {
        StartupMode::Tactical {
            player_id,
            server_url,
        } => (*player_id, server_url.clone()),
        _ => (0, String::new()),
    };
    app.add_plugins((
        AdventureSimulatorCorePlugins
            .build()
            .set(AdventureSimulatorPhysicsPlugin {
                enable_simulation: true,
            }),
        AdventureSimulatorNetPlugins,
        strategic::StrategicRendererPlugin,
        ui::UiPlugin,
        player::PlayerPlugin,
    ))
    .add_input_context::<Player>()
    .add_systems(OnEnter(RendererMode::StrategicMap), pause_physics)
    .add_systems(OnEnter(RendererMode::StrategicScene), pause_physics)
    .add_systems(OnEnter(RendererMode::Tactical), resume_physics)
    .add_systems(OnEnter(RendererMode::Tactical), (setup_scene, setup_client))
    .add_systems(OnExit(RendererMode::Tactical), cleanup_tactical)
    .add_systems(
        Update,
        (
            capture_cursor.run_if(
                input_just_pressed(MouseButton::Left)
                    .and(any_with_component::<CharacterController>),
            ),
            release_cursor.run_if(
                input_just_pressed(KeyCode::Escape).and(any_with_component::<CharacterController>),
            ),
        ),
    )
    .add_observer(on_game_scene_added_hook)
    .insert_resource(Args {
        id: player_id,
        server_addr,
    })
    .insert_resource(StrategicReturnMode(match mode {
        RendererMode::StrategicMap | RendererMode::StrategicScene => Some(mode),
        RendererMode::Tactical => None,
    }));

    if mode == RendererMode::Tactical {
        app.world_mut().resource_mut::<Time<Physics>>().unpause();
    } else {
        app.world_mut().resource_mut::<Time<Physics>>().pause();
    }

    #[cfg(feature = "debug")]
    if mode == RendererMode::Tactical {
        app.add_plugins(debug::DebugPlugin);
    }

    app.run();
}

fn apply_lifecycle(mut time: ResMut<Time<Virtual>>) {
    if SUSPENDED.load(Ordering::Relaxed) {
        time.pause();
    } else {
        time.unpause();
    }
}

fn pause_physics(mut time: ResMut<Time<Physics>>) {
    time.pause();
}
fn resume_physics(mut time: ResMut<Time<Physics>>) {
    time.unpause();
}

fn renderer_suspended() -> bool {
    SUSPENDED.load(Ordering::Relaxed)
}

fn setup_client(mut commands: Commands, args: Res<Args>) {
    RENDERER_STATUS.store(STATUS_TACTICAL_CONNECTING, Ordering::Release);
    commands.spawn((
        AdventureSimulatorClient {
            player_id: args.id,
            server_url: args.server_addr.clone(),
            ..default()
        },
        TacticalEntity,
    ));
}

fn setup_scene(mut commands: Commands, mut scattering_mediums: ResMut<Assets<ScatteringMedium>>) {
    // Spawn a directional light
    commands.spawn((
        Transform::from_xyz(200.0, 1000.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
        DirectionalLight {
            shadows_enabled: true,
            illuminance: lux::DIRECT_SUNLIGHT,
            ..default()
        },
        TacticalEntity,
    ));

    // Camera
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 80.0_f32.to_radians(),
            ..default()
        }),
        Atmosphere::earthlike(scattering_mediums.add(ScatteringMedium::default())),
        AtmosphereEnvironmentMapLight::default(),
        Exposure::SUNLIGHT,
        Tonemapping::AcesFitted,
        Bloom::NATURAL,
        Msaa::Off,
        ScreenSpaceAmbientOcclusion::default(),
        TacticalEntity,
    ));
}

fn drain_handoff(
    mode: Res<State<RendererMode>>,
    mut next_mode: ResMut<NextState<RendererMode>>,
    mut args: ResMut<Args>,
) {
    if *mode.get() == RendererMode::Tactical {
        return;
    }
    let command = HANDOFF.lock().ok().and_then(|mut mailbox| mailbox.take());
    let Some(command) = command else {
        return;
    };
    args.id = command.player_id;
    args.server_addr = command.server_url;
    next_mode.set(RendererMode::Tactical);
}

fn publish_connection_status(
    mode: Res<State<RendererMode>>,
    client_state: Res<State<adventuresim_tactical_netcode::bevy_replicon::prelude::ClientState>>,
) {
    if *mode.get() == RendererMode::Tactical
        && *client_state.get()
            == adventuresim_tactical_netcode::bevy_replicon::prelude::ClientState::Connected
    {
        RENDERER_STATUS.store(STATUS_TACTICAL_CONNECTED, Ordering::Release);
    }
}

fn monitor_connection_failure(
    mode: Res<State<RendererMode>>,
    client_state: Res<State<adventuresim_tactical_netcode::bevy_replicon::prelude::ClientState>>,
    return_mode: Res<StrategicReturnMode>,
    mut next_mode: ResMut<NextState<RendererMode>>,
    mut saw_connecting: Local<bool>,
    mut reached_play: Local<bool>,
    controlled_players: Query<(), With<ControlledPlayer>>,
) {
    use adventuresim_tactical_netcode::bevy_replicon::prelude::ClientState;
    if *mode.get() != RendererMode::Tactical || return_mode.0.is_none() {
        return;
    }
    match client_state.get() {
        ClientState::Connecting => *saw_connecting = true,
        ClientState::Connected => {
            *saw_connecting = true;
            if !controlled_players.is_empty() {
                *reached_play = true;
            }
        }
        ClientState::Disconnected
            if *saw_connecting
                && !*reached_play
                && matches!(
                    RENDERER_STATUS.load(Ordering::Acquire),
                    STATUS_TACTICAL_CONNECTING | STATUS_TACTICAL_CONNECTED
                ) =>
        {
            RENDERER_STATUS.store(STATUS_TACTICAL_FAILED, Ordering::Release);
            HANDOFF_ACCEPTED.store(false, Ordering::Release);
            next_mode.set(return_mode.0.expect("checked above"));
            *saw_connecting = false;
        }
        _ => {}
    }
}

fn cleanup_tactical(
    mut commands: Commands,
    entities: Query<
        Entity,
        Or<(
            With<TacticalEntity>,
            With<adventuresim_tactical_netcode::bevy_replicon::prelude::Replicated>,
            With<TacticalDerivedEntity>,
            With<TacticalTerrain>,
        )>,
    >,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}

pub(crate) fn publish_marker_selection(id: &str) {
    if let Ok(mut selection) = MARKER_SELECTION.lock() {
        *selection = Some(id.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_render_contracts::TACTICAL_HANDOFF_SCHEMA_VERSION;

    fn command() -> TacticalHandoffCommand {
        TacticalHandoffCommand {
            handoff_schema: TACTICAL_HANDOFF_SCHEMA_VERSION,
            player_id: 9,
            server_url: "127.0.0.1:6000".into(),
        }
    }

    #[test]
    fn handoff_mailbox_accepts_exactly_once() {
        *HANDOFF.lock().unwrap() = None;
        HANDOFF_ACCEPTED.store(false, Ordering::Release);
        assert_eq!(queue_handoff(command()), Ok(()));
        assert_eq!(
            queue_handoff(command()),
            Err("a tactical handoff is already pending or active")
        );
        assert_eq!(HANDOFF.lock().unwrap().as_ref().unwrap().player_id, 9);

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(RendererMode::StrategicScene)
            .insert_resource(Args {
                id: 0,
                server_addr: String::new(),
            })
            .add_systems(Update, drain_handoff)
            .add_systems(OnEnter(RendererMode::Tactical), setup_client);
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<AdventureSimulatorClient>>()
                .iter(app.world())
                .count(),
            0
        );
        app.update();
        app.update();
        assert_eq!(
            *app.world().resource::<State<RendererMode>>().get(),
            RendererMode::Tactical
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<AdventureSimulatorClient>>()
                .iter(app.world())
                .count(),
            1
        );
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<AdventureSimulatorClient>>()
                .iter(app.world())
                .count(),
            1
        );
        HANDOFF_ACCEPTED.store(false, Ordering::Release);
    }

    #[test]
    fn tactical_exit_removes_network_roots_terrain_and_derived_entities_only() {
        use adventuresim_tactical_netcode::bevy_replicon::prelude::Replicated;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(RendererMode::Tactical)
            .add_systems(OnExit(RendererMode::Tactical), cleanup_tactical);
        app.update();

        let strategic = app.world_mut().spawn(Name::new("strategic sentinel")).id();
        let replicated = app.world_mut().spawn(Replicated).id();
        let derived = app
            .world_mut()
            .spawn((
                TacticalDerivedEntity,
                Collider::cuboid(1.0, 1.0, 1.0),
                ChildOf(replicated),
            ))
            .id();
        let terrain = app
            .world_mut()
            .spawn((TacticalEntity, TacticalTerrain))
            .id();
        let tactical_root = app.world_mut().spawn(TacticalEntity).id();
        let tactical_child = app
            .world_mut()
            .spawn((TacticalDerivedEntity, ChildOf(tactical_root)))
            .id();

        app.world_mut()
            .resource_mut::<NextState<RendererMode>>()
            .set(RendererMode::StrategicScene);
        app.update();
        app.update();

        assert!(app.world().get_entity(strategic).is_ok());
        for removed in [replicated, derived, terrain, tactical_root, tactical_child] {
            assert!(app.world().get_entity(removed).is_err(), "{removed:?}");
        }
        let mut query = app.world_mut().query_filtered::<Entity, Or<(
            With<TacticalEntity>,
            With<Replicated>,
            With<TacticalDerivedEntity>,
            With<TacticalTerrain>,
        )>>();
        assert_eq!(query.iter(app.world()).count(), 0);
    }
}

fn on_game_scene_added_hook(
    event: On<Add, SceneId>,
    mut commands: Commands,
    query: Query<(&SceneId, &SceneTerrain)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let (id, terrain) = query.get(event.entity)?;
    info!(
        entity = ?event.entity,
        "Spawning a scene {id:?}"
    );

    let floor_color = match id.0.as_str() {
        "hills" => Color::srgb_u8(96, 108, 56),
        "desert" => Color::srgb_u8(221, 161, 94),
        id => {
            warn!("Unknown scene: {id}");
            Color::BLACK
        }
    };

    // Terrain mesh
    commands.spawn((
        Mesh3d(meshes.add(terrain.mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: floor_color,
            perceptual_roughness: 0.8,
            metallic: 0.0,
            ..default()
        })),
        TacticalEntity,
        TacticalTerrain,
    ));

    Ok(())
}

fn capture_cursor(
    mut commands: Commands,
    player: Single<Entity, With<CharacterController>>,
    mut cursor: Single<&mut CursorOptions>,
) {
    if !cursor.visible {
        return;
    }

    commands
        .entity(player.into_inner())
        .insert(ContextActivity::<Player>::ACTIVE);
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn release_cursor(
    mut commands: Commands,
    player: Single<Entity, With<CharacterController>>,
    mut cursor: Single<&mut CursorOptions>,
) {
    if cursor.visible {
        return;
    }

    commands
        .entity(player.into_inner())
        .insert(ContextActivity::<Player>::INACTIVE);
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}
