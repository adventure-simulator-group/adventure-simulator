//! Adventure Simulator - WASM Tactical Client
//!
//! A Bevy-based 3D game client that runs in the browser (WASM).
//! Features:
//! - WASD movement with a capsule character
//! - Camera follow system
//! - Ground plane and skybox
//! - Ready for Lightyear networking integration

use adventure_simulator_core::physics::AdventureSimulatorPhysicsPlugin;
use adventure_simulator_core::prelude::*;
use adventure_simulator_net::lightyear::prelude::{LinkOf, ReplicationSender, SendUpdatesMode};
use adventure_simulator_net::prelude::*;
use adventure_simulator_net::protocol::SEND_INTERVAL;
use bevy::prelude::*;
use bevy::{
    input::common_conditions::input_just_pressed,
    window::{CursorGrabMode, CursorOptions},
};
use clap::Parser;
#[cfg(target_family = "wasm")]
use console_error_panic_hook;
use std::net::SocketAddr;
#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[derive(Parser, Debug, Resource)]
#[command(version, about)]
struct Args {
    /// Client ID
    #[arg(long)]
    id: u64,
    /// Server addr
    #[arg(long)]
    server_addr: SocketAddr,
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
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Adventure Simulator - Tactical".into(),
                canvas: Some("#game-canvas".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            AdventureSimulatorCorePlugins
                .build()
                .set(AdventureSimulatorPhysicsPlugin {
                    enable_simulation: false,
                }),
            AdventureSimulatorNetPlugins,
        ))
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
        .add_systems(Startup, (setup_scene, setup_client))
        .add_systems(
            Update,
            (
                update_ui,
                capture_cursor.run_if(input_just_pressed(MouseButton::Left)),
                release_cursor.run_if(input_just_pressed(KeyCode::Escape)),
            ),
        )
        .add_observer(on_new_player_added_hook)
        .add_observer(on_game_scene_added_hook)
        .insert_resource(args)
        .run();
}

/// UI text for displaying controls
#[derive(Component)]
struct ControlsText;

fn setup_client(mut commands: Commands, args: Res<Args>) {
    commands.spawn((
        AdventureSimulatorClient {
            id: args.id,
            server_addr: args.server_addr.clone(),
            ..default()
        },
        ReplicationSender::new(SEND_INTERVAL, SendUpdatesMode::SinceLastAck, false),
    ));
}

fn setup_scene(mut commands: Commands) {
    // Spawn a directional light
    commands.spawn((
        Transform::from_xyz(0.0, 1.0, 0.0).looking_at(vec3(1.0, -2.0, -2.0), Vec3::Y),
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.4, 0.4, 0.6),
        brightness: 500.0,
        affects_lightmapped_meshes: true,
    });

    // Camera
    commands.spawn((
        Camera3d::default(),
        // Transform::from_xyz(0.0, CAMERA_HEIGHT, CAMERA_DISTANCE).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // UI - Controls text
    commands.spawn((
        ControlsText,
        Text::new("WASD to move | Space to jump | Mouse to look around"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(20.0),
            left: Val::Px(20.0),
            ..default()
        },
    ));

    // Position indicator
    commands.spawn((
        Text::new("Position: ..."),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(20.0),
            left: Val::Px(20.0),
            ..default()
        },
    ));
}

fn on_game_scene_added_hook(
    event: On<Add, GameSceneId>,
    mut commands: Commands,
    query: Query<&GameSceneId>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let id = query.get(event.entity)?;
    info!("Spawning a scene {id:?}");

    let floor_color = match id.0.as_str() {
        "town_a" => Color::srgb_u8(96, 108, 56),
        "town_b" => Color::srgb_u8(221, 161, 94),
        id => {
            warn!("Unknown scene: {id}");
            Color::BLACK
        }
    };

    // Ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(100.0, 100.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: floor_color,
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    // Grid lines for visual reference
    for i in -5..=5 {
        let x = i as f32 * 5.0;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(0.05, 0.02, 50.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.4, 0.4, 0.4, 0.5),
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(x, 0.01, 0.0),
        ));
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(50.0, 0.02, 0.05))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.4, 0.4, 0.4, 0.5),
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.01, x),
        ));
    }

    // Some obstacles/props for visual interest
    let mut spawn_prop = |position: Vec3, color: Color| {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(1.0, position.y * 2.0, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                metallic: 0.2,
                perceptual_roughness: 0.7,
                ..default()
            })),
            Transform::from_translation(position),
        ));
    };
    spawn_prop(Vec3::new(5.0, 0.5, 5.0), Color::srgb(0.4, 0.4, 0.8));
    spawn_prop(Vec3::new(-5.0, 0.5, 5.0), Color::srgb(0.8, 0.4, 0.4));
    spawn_prop(Vec3::new(5.0, 0.5, -5.0), Color::srgb(0.4, 0.8, 0.4));
    spawn_prop(Vec3::new(-5.0, 0.5, -5.0), Color::srgb(0.8, 0.8, 0.4));
    spawn_prop(Vec3::new(10.0, 0.75, 0.0), Color::srgb(0.6, 0.3, 0.6));
    spawn_prop(Vec3::new(-10.0, 0.75, 0.0), Color::srgb(0.3, 0.6, 0.6));

    Ok(())
}

fn on_new_player_added_hook(
    event: On<Add, Player>,
    mut commands: Commands,
    camera: Single<Entity, With<Camera3d>>,
    query: Query<&PlayerId, With<Player>>,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let id = query.get(event.entity)?;
    info!("Added new player {:?}+{id:?}", event.entity);

    if args.id == id.0 {
        info!(
            "New player {:?}+{id:?} is assigned to this client. Assuming control...",
            event.entity
        );

        commands.entity(event.entity).insert((
            // Adding character controller to sync the camera, but this component
            // requires a bunch of physics components. The actual physic simulation
            // is done on the server, so it doesn't do much harm.
            CharacterController::default(),
            actions!(Player[
                (
                    Action::<input::Movement>::new(),
                    DeadZone::default(),
                    Bindings::spawn((
                        Cardinal::wasd_keys(),
                        Axial::left_stick()
                    ))
                ),
                (
                    Action::<input::Jump>::new(),
                    bindings![KeyCode::Space, GamepadButton::South],
                ),
                (
                    Action::<input::RotateCamera>::new(),
                    Bindings::spawn((
                        Spawn((Binding::mouse_motion(), Scale::splat(0.15))),
                        Axial::right_stick().with((Scale::splat(4.0), DeadZone::default())),
                    ))
                ),
            ]),
        ));

        commands
            .entity(camera.into_inner())
            .insert(CharacterControllerCameraOf::new(event.entity));
    } else {
        commands.entity(event.entity).insert((
            Mesh3d(meshes.add(Capsule3d::new(0.4, 1.2))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb_u8(33, 158, 188),
                metallic: 0.0,
                perceptual_roughness: 1.0,
                ..default()
            })),
            children![(
                Mesh3d(meshes.add(Cuboid::new(0.6, 0.3, 0.4))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb_u8(255, 183, 3),
                    metallic: 0.0,
                    perceptual_roughness: 1.0,
                    ..default()
                })),
                Transform::from_xyz(0.0, 0.6, 0.2)
            )],
        ));
    }

    Ok(())
}

fn update_ui(
    player: Single<(&Transform, &PlayerId), With<CharacterController>>,
    mut text_query: Query<&mut Text, Without<ControlsText>>,
) {
    let (Transform { translation, .. }, &PlayerId(player_id)) = player.into_inner();

    for mut text in &mut text_query {
        text.0 = format!(
            "Position: x: {:.1}, y: {:.1}, z: {:.1}\nPlayer ID: {player_id}",
            translation.x, translation.y, translation.z
        );
    }
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
