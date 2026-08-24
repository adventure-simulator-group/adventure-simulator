//! Fabelgeist - WASM Tactical Client
//!
//! A Bevy-based 3D game client that runs in the browser (WASM).
//! Features:
//! - WASD movement with a capsule character
//! - Camera follow system
//! - Ground plane and skybox
//! - Uses the shared Aeronet/Replicon WebSocket netcode

use adventuresim_tactical_core::physics::AdventureSimulatorPhysicsPlugin;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::*;
#[cfg(target_family = "wasm")]
use bevy::asset::AssetMetaCheck;
use bevy::asset::AssetPlugin;
#[cfg(not(target_family = "wasm"))]
use bevy::asset::io::AssetSourceBuilder;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
#[cfg(not(target_family = "wasm"))]
use bevy::image::BevyDefault;
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy::{
    ecs::schedule::common_conditions::any_with_component,
    input::common_conditions::input_just_pressed,
    window::{CursorGrabMode, CursorOptions},
};
use clap::{Parser, ValueEnum};
#[cfg(target_family = "wasm")]
use console_error_panic_hook;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;
use web_time::Instant;

#[allow(dead_code)] // This binary shares viewer/editor animation APIs that other bins exercise.
mod animation;
#[cfg(target_family = "wasm")]
mod browser_runtime;
#[allow(dead_code)] // Viewer-only camera diagnostics are compiled into this binary.
mod camera;
#[cfg(feature = "debug")]
mod debug;
#[cfg(not(target_family = "wasm"))]
mod diagnostics;
mod equipment;
#[allow(dead_code)] // Viewer-only input diagnostics are compiled into this binary.
mod player;
mod presentation;
mod ui;

#[derive(Parser, Debug, Clone, Resource)]
#[command(version, about)]
struct Args {
    /// Client ID
    #[arg(long)]
    id: u64,
    /// Server URL or host:port
    #[arg(long)]
    server_addr: String,
    /// JSON command sequence that replaces physical movement input.
    #[arg(long)]
    input_script: Option<String>,
    /// Write one JSON object per rendered animation frame.
    #[arg(long)]
    animation_log: Option<String>,
    /// Write compact per-frame timing telemetry without animation pose data.
    #[arg(long)]
    frame_timing_log: Option<String>,
    /// Record compact frame timing for this many seconds, then exit.
    #[arg(long, requires = "frame_timing_log")]
    frame_timing_seconds: Option<f64>,
    /// Wait this many seconds after the local player is ready before timing.
    #[arg(long, default_value_t = 5.0, requires = "frame_timing_log")]
    frame_timing_warmup_seconds: f64,
    /// Close the native client shortly after the final scripted command.
    #[arg(long, requires = "input_script")]
    exit_after_script: bool,
    /// Rendering cost preset for diagnostics and low-power GPUs.
    #[arg(long, value_enum, default_value_t)]
    graphics_preset: GraphicsPreset,
    /// Swapchain presentation strategy for frame-pacing diagnostics.
    #[arg(long, value_enum, default_value_t)]
    present_mode: ClientPresentMode,
    /// Run without opening an OS window, for CLI-driven automated testing.
    #[cfg(feature = "debug")]
    #[arg(long)]
    headless: bool,
    /// Port to expose the Bevy Remote Protocol (BRP) HTTP JSON-RPC endpoint
    /// on for CLI-driven inspection/testing. Disabled unless set.
    #[cfg(feature = "debug")]
    #[arg(long)]
    brp_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum ClientPresentMode {
    #[default]
    AutoVsync,
    AutoNoVsync,
    Fifo,
    FifoRelaxed,
    Mailbox,
    Immediate,
}

impl From<ClientPresentMode> for PresentMode {
    fn from(value: ClientPresentMode) -> Self {
        match value {
            ClientPresentMode::AutoVsync => Self::AutoVsync,
            ClientPresentMode::AutoNoVsync => Self::AutoNoVsync,
            ClientPresentMode::Fifo => Self::Fifo,
            ClientPresentMode::FifoRelaxed => Self::FifoRelaxed,
            ClientPresentMode::Mailbox => Self::Mailbox,
            ClientPresentMode::Immediate => Self::Immediate,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum GraphicsPreset {
    #[default]
    Default,
    NoShadows,
    NoAtmosphere,
    NoEnvironmentLight,
    Minimal,
}

impl GraphicsPreset {
    fn presentation(self) -> presentation::TacticalPresentationPlugin {
        presentation::TacticalPresentationPlugin {
            shadows_enabled: !matches!(self, Self::NoShadows | Self::Minimal),
            atmosphere_enabled: !matches!(self, Self::NoAtmosphere | Self::Minimal),
            celestial_enabled: !matches!(self, Self::Minimal),
            environment_light_enabled: !matches!(
                self,
                Self::NoAtmosphere | Self::NoEnvironmentLight | Self::Minimal
            ),
            environment_map_size: 64,
            max_vista_lods: if matches!(self, Self::Minimal) { 1 } else { 3 },
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run(Args::parse(), true);
}

#[cfg(target_family = "wasm")]
fn main() {
    // Set up panic hook for better WASM error messages
    console_error_panic_hook::set_once();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_run(args: Vec<String>) {
    run(Args::parse_from(args), true);
}

/// Start the persistent browser renderer without joining a tactical server.
/// JavaScript subsequently drives strategic scenes and tactical connections
/// through `wasm_command`; Bevy and its WebGPU device remain alive throughout.
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_boot() {
    run(
        Args {
            id: 0,
            server_addr: String::new(),
            input_script: None,
            animation_log: None,
            frame_timing_log: None,
            frame_timing_seconds: None,
            frame_timing_warmup_seconds: 5.0,
            exit_after_script: false,
            graphics_preset: GraphicsPreset::Default,
            present_mode: ClientPresentMode::AutoVsync,
            #[cfg(feature = "debug")]
            headless: false,
            #[cfg(feature = "debug")]
            brp_port: None,
        },
        false,
    );
}

/// Queue one browser-runtime command without borrowing Bevy's running world.
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_command(command: String) -> Result<(), JsValue> {
    browser_runtime::queue_json(&command).map_err(|error| JsValue::from_str(&error))
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_weapon_catalog() -> Result<String, JsValue> {
    browser_runtime::catalog_json().map_err(|error| JsValue::from_str(&error))
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_default_weapon_design(catalog_id: String) -> Result<String, JsValue> {
    browser_runtime::default_design_json(&catalog_id).map_err(|error| JsValue::from_str(&error))
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_encode_weapon_design(design_json: String) -> Result<Vec<u8>, JsValue> {
    browser_runtime::encode_design_json(&design_json).map_err(|error| JsValue::from_str(&error))
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_weapon_editor_fields(design_json: String) -> Result<String, JsValue> {
    browser_runtime::editor_fields_json(&design_json).map_err(|error| JsValue::from_str(&error))
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_quote_weapon_design(design_json: String) -> Result<String, JsValue> {
    browser_runtime::quote_design_json(&design_json).map_err(|error| JsValue::from_str(&error))
}

fn run(args: Args, initial_tactical: bool) {
    let startup_started_at = Instant::now();
    #[cfg(not(target_family = "wasm"))]
    eprintln!("[startup] native client process entry");
    let mut app = App::new();
    #[cfg(not(target_family = "wasm"))]
    let asset_root = native_asset_root();
    #[cfg(not(target_family = "wasm"))]
    validate_native_presentation_assets(&asset_root)
        .unwrap_or_else(|error| panic!("invalid tactical client asset root: {error}"));
    #[cfg(not(target_family = "wasm"))]
    app.register_asset_source(
        "workspace",
        AssetSourceBuilder::platform_default(&asset_root.to_string_lossy(), None),
    );
    #[cfg(feature = "debug")]
    let headless = args.headless;
    #[cfg(not(feature = "debug"))]
    let headless = false;
    #[cfg(not(target_family = "wasm"))]
    let default_plugins = {
        let with_asset_plugin = DefaultPlugins.set(AssetPlugin {
            file_path: asset_root.to_string_lossy().into_owned(),
            ..default()
        });
        if headless {
            with_asset_plugin
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>()
        } else {
            with_asset_plugin.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Fabelgeist - Tactical".into(),
                    canvas: Some("#game-canvas".into()),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: true,
                    present_mode: args.present_mode.into(),
                    decorations: false,
                    ..default()
                }),
                ..default()
            })
        }
    };
    #[cfg(target_family = "wasm")]
    let default_plugins = DefaultPlugins
        .set(AssetPlugin {
            file_path: "/tactical/assets".into(),
            meta_check: AssetMetaCheck::Never,
            ..default()
        })
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "Fabelgeist - Tactical".into(),
                canvas: Some("#game-canvas".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                present_mode: args.present_mode.into(),
                decorations: false,
                ..default()
            }),
            ..default()
        });
    app.add_plugins((
        default_plugins,
        FrameTimeDiagnosticsPlugin::default(),
        EnhancedInputPlugin,
    ))
    .add_plugins((
        AdventureSimulatorCorePlugins
            .build()
            .set(AdventureSimulatorPhysicsPlugin {
                enable_simulation: false,
                enable_presentation_simulation: true,
            }),
        AdventureSimulatorNetPlugins,
    ))
    .add_input_context::<Player>()
    .add_plugins((
        ui::UiPlugin,
        player::PlayerPlugin,
        equipment::TacticalEquipmentPlugin,
        animation::TacticalAnimationPlugin,
        camera::TacticalCameraPlugin,
        // Headless runs have no window/swapchain, and some render features
        // (e.g. the atmosphere environment probe) crash without one, so
        // force every optional GPU effect off regardless of the requested
        // preset - there's nothing to present them to anyway.
        if headless {
            GraphicsPreset::Minimal.presentation()
        } else {
            args.graphics_preset.presentation()
        },
    ))
    .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
    .insert_resource(presentation::ClientStartupTiming::new(startup_started_at))
    .add_systems(Startup, setup_initial_client)
    .add_systems(
        Update,
        (
            capture_cursor.run_if(
                input_just_pressed(MouseButton::Left)
                    .and_then(any_with_component::<CharacterController>),
            ),
            release_cursor.run_if(
                input_just_pressed(KeyCode::Escape)
                    .and_then(any_with_component::<CharacterController>),
            ),
        ),
    )
    .insert_resource(player::LocalCharacterId(args.id))
    .insert_resource(InitialTacticalMode(initial_tactical))
    .insert_resource(args.clone());

    #[cfg(target_family = "wasm")]
    app.add_plugins(browser_runtime::BrowserRuntimePlugin::new(initial_tactical));

    #[cfg(feature = "debug")]
    app.add_plugins(debug::DebugPlugin);

    #[cfg(not(target_family = "wasm"))]
    app.add_plugins(
        diagnostics::DiagnosticPlugin::new(
            args.input_script.as_deref(),
            args.animation_log.as_deref(),
            args.frame_timing_log.as_deref(),
            args.frame_timing_seconds,
            args.frame_timing_warmup_seconds,
            args.exit_after_script,
        )
        .unwrap_or_else(|error| panic!("invalid tactical client diagnostics: {error}")),
    );

    #[cfg(not(target_family = "wasm"))]
    if headless {
        app.add_plugins(bevy::app::ScheduleRunnerPlugin {
            run_mode: bevy::app::RunMode::Loop {
                wait: Some(std::time::Duration::from_secs_f64(1.0 / 60.0)),
            },
        });
    }

    #[cfg(all(not(target_family = "wasm"), feature = "debug"))]
    if let Some(port) = args.brp_port {
        app.add_plugins((
            bevy::remote::RemotePlugin::default(),
            bevy::remote::http::RemoteHttpPlugin::default().with_port(port),
        ));
    }

    // Headless has no window to screenshot (F12, see `debug.rs`), so point
    // the gameplay camera at an off-screen render target instead.
    #[cfg(all(not(target_family = "wasm"), feature = "debug"))]
    if headless {
        app.add_systems(Update, configure_headless_render_target);
    }

    #[cfg(not(target_family = "wasm"))]
    eprintln!(
        "[startup] native client app constructed elapsed_ms={}",
        startup_started_at.elapsed().as_millis()
    );
    app.run();
}

#[cfg(all(not(target_family = "wasm"), feature = "debug"))]
const HEADLESS_SCREENSHOT_SIZE: (u32, u32) = (1280, 720);

#[cfg(all(not(target_family = "wasm"), feature = "debug"))]
fn configure_headless_render_target(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, Added<Camera3d>>,
) {
    for entity in &cameras {
        let image = images.add(Image::new_target_texture(
            HEADLESS_SCREENSHOT_SIZE.0,
            HEADLESS_SCREENSHOT_SIZE.1,
            bevy::render::render_resource::TextureFormat::bevy_default(),
            None,
        ));
        commands
            .entity(entity)
            .insert(bevy::camera::RenderTarget::Image(image.clone().into()));
        commands.insert_resource(debug::HeadlessScreenshotTarget(image));
    }
}

#[cfg(not(target_family = "wasm"))]
fn native_asset_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .unwrap_or_else(|error| panic!("could not resolve native asset directory: {error}"))
}

#[cfg(not(target_family = "wasm"))]
fn validate_native_presentation_assets(asset_root: &std::path::Path) -> Result<(), String> {
    const REQUIRED_ASSETS: &[&str] = &[
        "shaders/tactical_foliage.wgsl",
        "shaders/tactical_clouds.wgsl",
        "shaders/tactical_moon.wgsl",
        "shaders/tactical_stars.wgsl",
        "shaders/tactical_terrain.wgsl",
        "shaders/tactical_weather.wgsl",
        "shaders/tactical_tree_impostor.wgsl",
        "shaders/tactical_tree_leaf_card.wgsl",
        "tactical-equipment-icons.png",
        "textures/moon/lroc_color_2k.jpg",
    ];
    let missing = REQUIRED_ASSETS
        .iter()
        .filter(|path| !asset_root.join(path).is_file())
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} is missing required presentation assets: {}",
            asset_root.display(),
            missing.join(", ")
        ))
    }
}

#[derive(Resource)]
struct InitialTacticalMode(bool);

fn setup_initial_client(
    mut commands: Commands,
    args: Res<Args>,
    initial_tactical: Res<InitialTacticalMode>,
    startup: Res<presentation::ClientStartupTiming>,
) {
    if !initial_tactical.0 {
        return;
    }
    commands.spawn(AdventureSimulatorClient {
        player_id: args.id,
        server_url: args.server_addr.clone(),
    });
    startup.mark("startup schedule complete; connection requested");
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

#[cfg(test)]
mod graphics_preset_tests {
    use super::*;

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn native_asset_root_contains_required_presentation_assets() {
        validate_native_presentation_assets(&native_asset_root()).unwrap();
    }

    #[test]
    fn individual_presets_disable_only_the_requested_effect() {
        let no_atmosphere = GraphicsPreset::NoAtmosphere.presentation();
        assert!(no_atmosphere.shadows_enabled);
        assert!(!no_atmosphere.atmosphere_enabled);
        assert!(!no_atmosphere.environment_light_enabled);
    }

    #[test]
    fn minimal_disables_every_optional_gpu_effect() {
        let minimal = GraphicsPreset::Minimal.presentation();
        assert!(!minimal.shadows_enabled);
        assert!(!minimal.atmosphere_enabled);
        assert!(!minimal.environment_light_enabled);
    }

    #[test]
    fn environment_presets_preserve_the_atmosphere_sky() {
        let disabled = GraphicsPreset::NoEnvironmentLight.presentation();
        assert!(disabled.atmosphere_enabled);
        assert!(!disabled.environment_light_enabled);

        let normal = GraphicsPreset::Default.presentation();
        assert!(normal.atmosphere_enabled);
        assert!(normal.environment_light_enabled);
        assert_eq!(normal.environment_map_size, 64);
    }

    #[test]
    fn diagnostic_present_modes_map_without_fallback_guessing() {
        assert_eq!(
            PresentMode::from(ClientPresentMode::AutoVsync),
            PresentMode::AutoVsync
        );
        assert_eq!(
            PresentMode::from(ClientPresentMode::AutoNoVsync),
            PresentMode::AutoNoVsync
        );
        assert_eq!(
            PresentMode::from(ClientPresentMode::Mailbox),
            PresentMode::Mailbox
        );
        assert_eq!(
            PresentMode::from(ClientPresentMode::Immediate),
            PresentMode::Immediate
        );
    }
}
