use adventure_simulator_core::prelude::*;
use bevy::prelude::*;

use crate::Args;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_new_player_added_hook);
        app.add_systems(Update, (setup_controlled_player, update_character_look_rotation));
    }
}

/// Marker component to defer controlled player setup to the next frame.
/// This avoids deep observer nesting when `with_related::<ActionOf<Player>>`
/// triggers lightyear's input replication observers during command flush.
#[derive(Component)]
struct PendingControlledPlayer;

fn on_new_player_added_hook(
    event: On<Add, Player>,
    mut commands: Commands,
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

        commands
            .entity(event.entity)
            .insert(PendingControlledPlayer);
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

fn setup_controlled_player(
    mut commands: Commands,
    query: Query<Entity, With<PendingControlledPlayer>>,
    camera: Single<Entity, With<Camera3d>>,
) {
    let camera_entity = camera.into_inner();
    for entity in &query {
        commands
            .entity(entity)
            .remove::<PendingControlledPlayer>()
            .insert(CharacterController::default())
            .with_related::<ActionOf<Player>>((
                Action::<input::Movement>::new(),
                DeadZone::default(),
                Bindings::spawn((Cardinal::wasd_keys(), Axial::left_stick())),
            ))
            .with_related::<ActionOf<Player>>((
                Action::<input::Jump>::new(),
                bindings![KeyCode::Space, GamepadButton::South],
            ))
            .with_related::<ActionOf<Player>>((
                Action::<input::RotateCamera>::new(),
                Bindings::spawn((
                    Spawn((Binding::mouse_motion(), Scale::splat(0.15))),
                    Axial::right_stick().with((Scale::splat(4.0), DeadZone::default())),
                )),
            ));

        commands
            .entity(camera_entity)
            .insert(CharacterControllerCameraOf::new(entity));
    }
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
