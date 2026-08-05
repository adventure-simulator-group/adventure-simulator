//! Adventure Simulator - WASM Tactical Client
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
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::input_focus::InputDispatchPlugin;
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy::{
    asset::io::AssetSourceBuilder,
    ecs::schedule::common_conditions::any_with_component,
    input::common_conditions::input_just_pressed,
    window::{CursorGrabMode, CursorOptions},
};
use clap::{Parser, ValueEnum};
#[cfg(target_family = "wasm")]
use console_error_panic_hook;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

mod animation;
mod camera;
#[cfg(feature = "debug")]
mod debug;
#[cfg(not(target_family = "wasm"))]
mod diagnostics;
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
    /// Close the native client shortly after the final scripted command.
    #[arg(long, requires = "input_script")]
    exit_after_script: bool,
    /// Rendering cost preset for diagnostics and low-power GPUs.
    #[arg(long, value_enum, default_value_t)]
    graphics_preset: GraphicsPreset,
    /// Swapchain presentation strategy for frame-pacing diagnostics.
    #[arg(long, value_enum, default_value_t)]
    present_mode: ClientPresentMode,
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
    NoSsao,
    NoBloom,
    NoAtmosphere,
    NoEnvironmentLight,
    Minimal,
}

impl GraphicsPreset {
    fn presentation(self) -> presentation::TacticalPresentationPlugin {
        presentation::TacticalPresentationPlugin {
            shadows_enabled: !matches!(self, Self::NoShadows | Self::Minimal),
            atmosphere_enabled: !matches!(self, Self::NoAtmosphere | Self::Minimal),
            environment_light_enabled: !matches!(
                self,
                Self::NoAtmosphere | Self::NoEnvironmentLight | Self::Minimal
            ),
            environment_map_size: 64,
            bloom_enabled: !matches!(self, Self::NoBloom | Self::Minimal),
            ssao_enabled: !matches!(self, Self::NoSsao | Self::Minimal),
        }
    }
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

fn run(args: Args) {
    let mut app = App::new();
    #[cfg(not(target_family = "wasm"))]
    app.register_asset_source(
        "workspace",
        AssetSourceBuilder::platform_default(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets")
                .to_string_lossy(),
            None,
        ),
    );
    let default_plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Adventure Simulator - Tactical".into(),
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
        InputDispatchPlugin,
    ))
    .add_plugins((
        AdventureSimulatorCorePlugins
            .build()
            .set(AdventureSimulatorPhysicsPlugin {
                enable_simulation: false,
            }),
        AdventureSimulatorNetPlugins,
    ))
    .add_input_context::<Player>()
    .add_plugins((
        ui::UiPlugin,
        player::PlayerPlugin,
        animation::TacticalAnimationPlugin,
        camera::TacticalCameraPlugin,
        args.graphics_preset.presentation(),
    ))
    .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
    .add_systems(Startup, setup_client)
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
    .insert_resource(player::LocalCharacterId(args.id))
    .insert_resource(args.clone());

    #[cfg(feature = "debug")]
    app.add_plugins(debug::DebugPlugin);

    #[cfg(not(target_family = "wasm"))]
    app.add_plugins(
        diagnostics::DiagnosticPlugin::new(
            args.input_script.as_deref(),
            args.animation_log.as_deref(),
            args.exit_after_script,
        )
        .unwrap_or_else(|error| panic!("invalid tactical client diagnostics: {error}")),
    );

    app.run();
}

fn setup_client(mut commands: Commands, args: Res<Args>) {
    commands.spawn(AdventureSimulatorClient {
        player_id: args.id,
        server_url: args.server_addr.clone(),
        ..default()
    });
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

    #[test]
    fn individual_presets_disable_only_the_requested_effect() {
        let no_ssao = GraphicsPreset::NoSsao.presentation();
        assert!(no_ssao.shadows_enabled);
        assert!(no_ssao.atmosphere_enabled);
        assert!(no_ssao.environment_light_enabled);
        assert!(no_ssao.bloom_enabled);
        assert!(!no_ssao.ssao_enabled);

        let no_atmosphere = GraphicsPreset::NoAtmosphere.presentation();
        assert!(no_atmosphere.shadows_enabled);
        assert!(!no_atmosphere.atmosphere_enabled);
        assert!(!no_atmosphere.environment_light_enabled);
        assert!(no_atmosphere.bloom_enabled);
        assert!(no_atmosphere.ssao_enabled);
    }

    #[test]
    fn minimal_disables_every_optional_gpu_effect() {
        let minimal = GraphicsPreset::Minimal.presentation();
        assert!(!minimal.shadows_enabled);
        assert!(!minimal.atmosphere_enabled);
        assert!(!minimal.environment_light_enabled);
        assert!(!minimal.bloom_enabled);
        assert!(!minimal.ssao_enabled);
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
