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
use bevy::prelude::*;
use bevy::{
    ecs::schedule::common_conditions::any_with_component,
    input::common_conditions::input_just_pressed,
    window::{CursorGrabMode, CursorOptions},
};
use clap::Parser;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;
use web_time::Instant;

fn present_mode(value: presentation::PresentModeConfig) -> bevy::window::PresentMode {
    use presentation::PresentModeConfig;
    match value {
        PresentModeConfig::AutoVsync => bevy::window::PresentMode::AutoVsync,
        PresentModeConfig::AutoNoVsync => bevy::window::PresentMode::AutoNoVsync,
        PresentModeConfig::Fifo => bevy::window::PresentMode::Fifo,
        PresentModeConfig::FifoRelaxed => bevy::window::PresentMode::FifoRelaxed,
        PresentModeConfig::Mailbox => bevy::window::PresentMode::Mailbox,
        PresentModeConfig::Immediate => bevy::window::PresentMode::Immediate,
    }
}

#[cfg(not(target_family = "wasm"))]
fn window_mode(value: presentation::WindowModeConfig) -> bevy::window::WindowMode {
    match value {
        presentation::WindowModeConfig::Windowed => bevy::window::WindowMode::Windowed,
        presentation::WindowModeConfig::BorderlessFullscreen => {
            bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current)
        }
    }
}

#[expect(
    dead_code,
    reason = "this binary shares animation APIs whose remaining entry points belong to the capture viewer"
)]
mod animation;
#[cfg(target_family = "wasm")]
mod browser_runtime;
#[expect(
    dead_code,
    reason = "the shared camera module includes diagnostics consumed only by capture viewers"
)]
mod camera;
#[cfg(feature = "debug")]
mod debug;
#[cfg(not(target_family = "wasm"))]
mod diagnostics;
mod equipment;
#[expect(
    dead_code,
    reason = "the shared player module includes input diagnostics consumed only by capture viewers"
)]
mod player;
#[expect(
    dead_code,
    reason = "the gameplay binary shares presentation review data with the native capture viewers"
)]
mod presentation;
mod targeting;
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
    /// Tactical graphics YAML. Defaults to assets/config/tactical-graphics.yaml.
    #[arg(long)]
    graphics_config: Option<std::path::PathBuf>,
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

#[cfg(not(target_family = "wasm"))]
fn main() {
    let args = Args::parse();
    let asset_root = native_asset_root();
    let path = args
        .graphics_config
        .clone()
        .unwrap_or_else(|| asset_root.join("config/tactical-graphics.yaml"));
    let config = presentation::TacticalGraphicsConfig::load(&path)
        .unwrap_or_else(|error| panic!("invalid tactical graphics configuration: {error}"));
    run(args, true, config);
}

#[cfg(target_family = "wasm")]
fn main() {
    // Set up panic hook for better WASM error messages
    console_error_panic_hook::set_once();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_run(args: Vec<String>, graphics_yaml: String) {
    let config = presentation::TacticalGraphicsConfig::parse(&graphics_yaml)
        .unwrap_or_else(|error| panic!("invalid tactical graphics configuration: {error}"));
    run(Args::parse_from(args), true, config);
}

/// Start the persistent browser renderer without joining a tactical server.
/// JavaScript subsequently drives strategic scenes and tactical connections
/// through `wasm_command`; Bevy and its WebGPU device remain alive throughout.
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub fn wasm_boot(graphics_yaml: String) {
    let config = presentation::TacticalGraphicsConfig::parse(&graphics_yaml)
        .unwrap_or_else(|error| panic!("invalid tactical graphics configuration: {error}"));
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
            graphics_config: None,
            #[cfg(feature = "debug")]
            headless: false,
            #[cfg(feature = "debug")]
            brp_port: None,
        },
        false,
        config,
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

fn run(
    args: Args,
    initial_tactical: bool,
    mut graphics_config: presentation::TacticalGraphicsConfig,
) {
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
    if headless {
        graphics_config.rendering.shadows.enabled = false;
        graphics_config.rendering.bloom.enabled = false;
        graphics_config.rendering.atmosphere.enabled = false;
        graphics_config.rendering.atmosphere.environment_light = false;
        graphics_config.rendering.clouds.enabled = false;
        graphics_config.rendering.vista.maximum_lods = 1;
        graphics_config.rendering.anti_aliasing = presentation::AntiAliasingConfig::Off;
        graphics_config.grass.enabled = false;
    }
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
            let desktop = &graphics_config.desktop;
            with_asset_plugin.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Fabelgeist - Tactical".into(),
                    mode: window_mode(desktop.window.mode),
                    resolution: (desktop.window.width, desktop.window.height).into(),
                    resizable: desktop.window.resizable,
                    decorations: desktop.window.decorations,
                    present_mode: present_mode(desktop.present_mode),
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
                present_mode: present_mode(graphics_config.desktop.present_mode),
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
        presentation::TacticalPresentationPlugin {
            config: graphics_config,
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
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
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
mod graphics_config_tests {
    use super::*;

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn native_asset_root_contains_required_presentation_assets() {
        validate_native_presentation_assets(&native_asset_root()).unwrap();
    }

    #[test]
    fn shipped_config_launches_in_a_resizable_decorated_window() {
        let config = presentation::TacticalPresentationPlugin::default().config;
        assert_eq!(
            config.desktop.window.mode,
            presentation::WindowModeConfig::Windowed
        );
        assert!(config.desktop.window.resizable);
        assert!(config.desktop.window.decorations);
    }

    #[test]
    fn configured_present_modes_map_without_fallback_guessing() {
        assert_eq!(
            present_mode(presentation::PresentModeConfig::AutoVsync),
            bevy::window::PresentMode::AutoVsync
        );
        assert_eq!(
            present_mode(presentation::PresentModeConfig::AutoNoVsync),
            bevy::window::PresentMode::AutoNoVsync
        );
        assert_eq!(
            present_mode(presentation::PresentModeConfig::Mailbox),
            bevy::window::PresentMode::Mailbox
        );
        assert_eq!(
            present_mode(presentation::PresentModeConfig::Immediate),
            bevy::window::PresentMode::Immediate
        );
    }
}
