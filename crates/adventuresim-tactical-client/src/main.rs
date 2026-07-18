//! Adventure Simulator shared browser renderer and tactical client.
//!
//! A Bevy-based 3D game client that runs in the browser (WASM).
//! Features:
//! - WASD movement with a capsule character
//! - Camera follow system
//! - Ground plane and skybox
//! - Ready for Lightyear networking integration

use adventuresim_render_contracts::{StartupConfig, StartupMode};
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
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "debug")]
mod debug;
mod player;
mod strategic;
mod ui;

static SUSPENDED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, States)]
enum RendererMode {
    StrategicMap,
    StrategicScene,
    #[default]
    Tactical,
}
#[derive(Resource, Clone)]
struct RendererConfig(StartupConfig);

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
pub fn wasm_set_suspended(suspended: bool) {
    SUSPENDED.store(suspended, Ordering::Relaxed);
}

/// Reserved command boundary for an eventual in-page strategic-to-tactical handoff.
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_tactical_handoff(_player_id: u64, _server_url: &str) -> Result<(), JsValue> {
    Err(JsValue::from_str(
        "hot tactical handoff is not enabled yet; use the tactical page fallback",
    ))
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
    .add_systems(Update, apply_lifecycle)
    .insert_resource(RendererConfig(config.clone()));

    if mode == RendererMode::Tactical {
        let StartupMode::Tactical {
            player_id,
            server_url,
        } = config.startup
        else {
            unreachable!()
        };
        app.add_plugins((
            AdventureSimulatorCorePlugins
                .build()
                .set(AdventureSimulatorPhysicsPlugin {
                    enable_simulation: false,
                }),
            AdventureSimulatorNetPlugins,
        ))
        .add_input_context::<Player>()
        .add_plugins((ui::UiPlugin, player::PlayerPlugin))
        .add_systems(Startup, (setup_scene, setup_client))
        .add_systems(
            Update,
            (
                capture_cursor.run_if(
                    input_just_pressed(MouseButton::Left)
                        .and(any_with_component::<CharacterController>),
                ),
                release_cursor.run_if(
                    input_just_pressed(KeyCode::Escape)
                        .and(any_with_component::<CharacterController>),
                ),
            ),
        )
        .add_observer(on_game_scene_added_hook)
        .insert_resource(Args {
            id: player_id,
            server_addr: server_url,
        });
    } else {
        app.add_plugins(strategic::StrategicRendererPlugin);
    }

    #[cfg(feature = "debug")]
    app.add_plugins(debug::DebugPlugin);

    app.run();
}

fn apply_lifecycle(mut time: ResMut<Time<Virtual>>) {
    if SUSPENDED.load(Ordering::Relaxed) {
        time.pause();
    } else {
        time.unpause();
    }
}

fn setup_client(mut commands: Commands, args: Res<Args>) {
    commands.spawn(AdventureSimulatorClient {
        player_id: args.id,
        server_url: args.server_addr.clone(),
        ..default()
    });
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
    ));
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
