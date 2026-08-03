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
    ecs::schedule::common_conditions::any_with_component,
    input::common_conditions::input_just_pressed,
    window::{CursorGrabMode, CursorOptions},
};
use clap::Parser;
#[cfg(target_family = "wasm")]
use console_error_panic_hook;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

mod animation;
mod camera;
#[cfg(feature = "debug")]
mod debug;
mod player;
mod presentation;
mod ui;

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

fn run(args: Args) {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Adventure Simulator - Tactical".into(),
                canvas: Some("#game-canvas".into()),
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
        presentation::TacticalPresentationPlugin,
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
    .insert_resource(args);

    #[cfg(feature = "debug")]
    app.add_plugins(debug::DebugPlugin);

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
