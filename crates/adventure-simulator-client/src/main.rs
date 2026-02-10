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
use adventure_simulator_net::lightyear::prelude::{ReplicationSender, SendUpdatesMode};
use adventure_simulator_net::prelude::*;
use adventure_simulator_net::protocol::SEND_INTERVAL;
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::light_consts::lux;
use bevy::light::AtmosphereEnvironmentMapLight;
use bevy::pbr::{Atmosphere, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
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

mod ui;

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
        .add_plugins((ui::UiPlugin,))
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
        .add_systems(Startup, (setup_scene, setup_client))
        .add_systems(
            Update,
            (
                capture_cursor.run_if(input_just_pressed(MouseButton::Left)),
                release_cursor.run_if(input_just_pressed(KeyCode::Escape)),
            ),
        )
        .add_observer(on_new_player_added_hook)
        .add_observer(on_game_scene_added_hook)
        .insert_resource(args)
        .run();
}

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
        Atmosphere::EARTH,
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
    info!("Spawning a scene {id:?}");

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

    // Some obstacles/props for visual interest
    let mut spawn_prop = |pos: Vec2, terrain: &SceneTerrain, color: Color| {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                metallic: 0.5,
                perceptual_roughness: 0.5,
                ..default()
            })),
            Transform::from_translation(Vec3::new(
                pos.x,
                terrain.height_at(pos).unwrap_or_default() + 1.0,
                pos.y,
            )),
        ));
    };
    spawn_prop(Vec2::new(5.0, 5.0), terrain, Color::srgb(0.4, 0.4, 0.8));
    spawn_prop(Vec2::new(-5.0, 5.0), terrain, Color::srgb(0.8, 0.4, 0.4));
    spawn_prop(Vec2::new(5.0, -5.0), terrain, Color::srgb(0.4, 0.8, 0.4));
    spawn_prop(Vec2::new(-5.0, -5.0), terrain, Color::srgb(0.8, 0.8, 0.4));
    spawn_prop(Vec2::new(10.0, 0.0), terrain, Color::srgb(0.6, 0.3, 0.6));
    spawn_prop(Vec2::new(-10.0, 0.0), terrain, Color::srgb(0.3, 0.6, 0.6));

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
    info!("Added new player {:?}/{id:?}", event.entity);

    if args.id == id.0 {
        info!(
            "New player {:?}/{id:?} is assigned to this client. Assuming control...",
            event.entity
        );

        commands.entity(event.entity).insert((
            // Adding character controller to sync the camera, but this component
            // requires a bunch of physics components. The actual physic simulation
            // is done on the server, so it shouldn't do much harm.
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
                base_color: id.color(),
                metallic: 0.0,
                perceptual_roughness: 1.0,
                ..default()
            })),
            children![(
                Mesh3d(meshes.add(Cuboid::new(0.6, 0.3, 0.4))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: id.color().lighter(0.2),
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
