use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

use crate::Args;

#[derive(Component, Debug, Clone, Copy)]
pub struct ClientPlayer;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_new_player_added_hook);
        app.add_systems(Update, update_character_look_rotation);
    }
}

fn on_new_player_added_hook(
    event: On<Add, Player>,
    mut commands: Commands,
    camera: Single<Entity, With<Camera3d>>,
    query: Query<(&Player, &PlayerId)>,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let (Player { name }, id) = query.get(event.entity)?;
    info!(entity = ?event.entity, id = id.0, "Added new player {name}");

    if args.id == id.0 {
        info!(
            entity = ?event.entity,
            "New player is assigned to this client. Assuming control...",
        );

        commands.entity(event.entity).insert((
            // Adding character controller to sync the camera, but this component
            // requires a bunch of physics components. The actual physic simulation
            // is done on the server, so it shouldn't do much harm.
            CharacterController::default(),
            ClientPlayer,
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
                (
                    Action::<Attack>::new(),
                    bindings![MouseButton::Left],
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

fn update_character_look_rotation(
    mut q_characters: Query<
        (&mut Transform, &CharacterLook),
        (Changed<CharacterLook>, Without<ControlledPlayer>),
    >,
) {
    for (mut transform, look) in &mut q_characters {
        transform.rotation = Quat::from_rotation_y(look.yaw + std::f32::consts::PI);
    }
}
