//! Adventure Simulator - WASM Tactical Client
//!
//! A Bevy-based 3D game client that runs in the browser (WASM).
//! Features:
//! - WASD movement with a capsule character
//! - Camera follow system
//! - Ground plane and skybox
//! - Ready for Lightyear networking integration

use avian3d::{prelude::*, PhysicsPlugins};
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    input::common_conditions::input_just_pressed,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_ahoy::{
    camera::CharacterControllerCameraOf,
    input::{Jump, Movement, RotateCamera},
    AhoyPlugin, CharacterController,
};
use bevy_enhanced_input::{action::Action, actions, bindings, prelude::*, EnhancedInputPlugin};
#[cfg(target_family = "wasm")]
use console_error_panic_hook;

fn main() {
    // Set up panic hook for better WASM error messages
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();

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
            PhysicsPlugins::default(),
            EnhancedInputPlugin,
            AhoyPlugin::default(),
        ))
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
        .add_input_context::<PlayerInput>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                update_ui,
                capture_cursor.run_if(input_just_pressed(MouseButton::Left)),
                release_cursor.run_if(input_just_pressed(KeyCode::Escape)),
            ),
        )
        .run();
}

#[derive(Component, Default, Debug)]
struct Player;

#[derive(Component, Default, Debug)]
#[component(on_add = PlayerInput::on_add)]
struct PlayerInput;

impl PlayerInput {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        world
            .commands()
            .entity(ctx.entity)
            .insert(actions!(PlayerInput[
                (
                    Action::<Movement>::new(),
                    DeadZone::default(),
                    Bindings::spawn((
                        Cardinal::wasd_keys(),
                        Axial::left_stick()
                    ))
                ),
                (
                    Action::<Jump>::new(),
                    bindings![KeyCode::Space, GamepadButton::South],
                ),
                (
                    Action::<RotateCamera>::new(),
                    Bindings::spawn((
                        Spawn((Binding::mouse_motion(), Scale::splat(0.15))),
                        Axial::right_stick().with((Scale::splat(4.0), DeadZone::default())),
                    ))
                ),
            ]));
    }
}

/// UI text for displaying controls
#[derive(Component)]
struct ControlsText;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane
    commands.spawn((
        RigidBody::Static,
        // Collider::heightfield(vec![vec![0.0; 50]; 50], Vec3::splat(1.0)),
        Collider::half_space(Vec3::Y),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.5, 0.2),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
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

    // Player (capsule character)
    let player_entity = commands
        .spawn((
            Player,
            PlayerInput,
            CharacterController::default(),
            RigidBody::Kinematic,
            Collider::cylinder(0.4, 1.2),
            // Mesh3d(meshes.add(Capsule3d::new(0.4, 1.2))),
            // MeshMaterial3d(materials.add(StandardMaterial {
            //     base_color: Color::srgb(0.8, 0.3, 0.3),
            //     metallic: 0.3,
            //     perceptual_roughness: 0.5,
            //     ..default()
            // })),
            Transform::from_xyz(0.0, 10.0, 0.0),
        ))
        .id();

    // // Direction indicator (small cone on top of player)
    // commands.spawn((
    //     Mesh3d(meshes.add(Cone::new(0.2, 0.4))),
    //     MeshMaterial3d(materials.add(StandardMaterial {
    //         base_color: Color::srgb(1.0, 0.8, 0.0),
    //         emissive: LinearRgba::new(1.0, 0.8, 0.0, 1.0),
    //         ..default()
    //     })),
    //     Transform::from_xyz(0.0, 1.0, -0.3)
    //         .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    //     ChildOf(player_entity),
    // ));

    // Some obstacles/props for visual interest
    spawn_prop(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(5.0, 0.5, 5.0),
        Color::srgb(0.4, 0.4, 0.8),
    );
    spawn_prop(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(-5.0, 0.5, 5.0),
        Color::srgb(0.8, 0.4, 0.4),
    );
    spawn_prop(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(5.0, 0.5, -5.0),
        Color::srgb(0.4, 0.8, 0.4),
    );
    spawn_prop(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(-5.0, 0.5, -5.0),
        Color::srgb(0.8, 0.8, 0.4),
    );
    spawn_prop(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(10.0, 0.75, 0.0),
        Color::srgb(0.6, 0.3, 0.6),
    );
    spawn_prop(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(-10.0, 0.75, 0.0),
        Color::srgb(0.3, 0.6, 0.6),
    );

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
        CharacterControllerCameraOf::new(player_entity),
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
        Text::new("Position: (0.0, 0.0)"),
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

fn spawn_prop(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    position: Vec3,
    color: Color,
) {
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
}

fn update_ui(
    player: Single<&Transform, With<Player>>,
    mut text_query: Query<&mut Text, Without<ControlsText>>,
) {
    let player_transform = player.into_inner();

    for mut text in &mut text_query {
        text.0 = format!(
            "Position: ({:.1}, {:.1})",
            player_transform.translation.x, player_transform.translation.z
        );
    }
}

fn capture_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}
