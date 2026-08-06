use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    message::{DefendRequest, MeleeActionRequest},
};
use bevy::prelude::*;

use crate::Args;

const BODY_PART_HITBOXES: &[(BodyPart, Vec3, Vec3)] = &[
    (
        BodyPart::Head,
        Vec3::new(0.0, 0.92, 0.0),
        Vec3::new(0.27, 0.23, 0.22),
    ),
    (
        BodyPart::Chest,
        Vec3::new(0.0, 0.49, 0.0),
        Vec3::new(0.33, 0.23, 0.29),
    ),
    (
        BodyPart::Stomach,
        Vec3::new(0.0, 0.17, 0.0),
        Vec3::new(0.25, 0.12, 0.25),
    ),
    (
        BodyPart::LeftArm,
        Vec3::new(-0.40, 0.25, 0.0),
        Vec3::new(0.1, 0.5, 0.1),
    ),
    (
        BodyPart::RightArm,
        Vec3::new(0.40, 0.25, 0.0),
        Vec3::new(0.1, 0.5, 0.1),
    ),
    (
        BodyPart::LeftLeg,
        Vec3::new(-0.16, -0.40, 0.0),
        Vec3::new(0.15, 0.5, 0.15),
    ),
    (
        BodyPart::RightLeg,
        Vec3::new(0.16, -0.40, 0.0),
        Vec3::new(0.15, 0.5, 0.15),
    ),
];
const HITBOX_LAYER: LayerMask = LayerMask(1 << 1);
const PRE_HIT_DELAY: f32 = 0.3;
const HIT_PRECISION: f32 = 1.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_new_player_added_hook)
            .add_observer(on_attack_fired_hook)
            .add_observer(on_dodge_fired)
            .add_observer(on_parry_fired)
            .add_systems(
                Update,
                (
                    update_character_look_rotation.run_if(any_with_component::<CharacterLook>),
                    update_attack_state_system.run_if(any_with_component::<AttackState>),
                ),
            );
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct ClientPlayer;

#[derive(EntityEvent)]
pub struct HitPerformed {
    pub entity: Entity,
    pub direction: Dir3,
    pub origin: Vec3,
    pub length: f32,
}

#[derive(Component, Clone, Copy)]
pub struct LimbHitbox(pub BodyPart);

#[derive(Component, Default)]
pub struct AttackState {
    pub pre_hit_timer: Timer,
    pub reach: f32,
}

impl AttackState {
    pub fn new(pre_hit_delay: f32, reach: f32) -> Self {
        let pre_hit_timer = Timer::from_seconds(pre_hit_delay, TimerMode::Once);
        Self {
            pre_hit_timer,
            reach,
        }
    }

    pub fn is_attacking(&self) -> bool {
        !self.pre_hit_timer.is_paused() && !self.pre_hit_timer.is_finished()
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
                (
                    Action::<Dodge>::new(),
                    bindings![KeyCode::KeyF],
                ),
                (
                    Action::<Parry>::new(),
                    bindings![KeyCode::KeyG],
                ),
            ]),
        ));

        #[cfg(feature = "debug")]
        commands.entity(event.entity).insert(DebugRender::none());

        commands
            .entity(camera.into_inner())
            .insert(CharacterControllerCameraOf::new(event.entity));
    } else {
        commands.entity(event.entity).with_children(|parent| {
            for &(body_part, offset, half_extents) in BODY_PART_HITBOXES {
                let color = match body_part {
                    BodyPart::Head => id.color().lighter(0.2).rotate_hue(30.0),
                    BodyPart::LeftArm | BodyPart::RightArm => {
                        id.color().lighter(0.1).rotate_hue(-20.0)
                    }
                    BodyPart::LeftLeg | BodyPart::RightLeg => {
                        id.color().darker(0.1).rotate_hue(-20.0)
                    }
                    _ => id.color(),
                };
                parent.spawn((
                    Mesh3d(meshes.add(Capsule3d::new(
                        half_extents.x,
                        (half_extents.y - half_extents.x).max(0.0) * 2.0,
                    ))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: color,
                        metallic: 0.0,
                        perceptual_roughness: 1.0,
                        ..default()
                    })),
                    Transform::from_translation(offset),
                ));

                if body_part == BodyPart::Head {
                    parent.spawn((
                        Mesh3d(meshes.add(Cuboid::from_size(Vec3::new(0.3, 0.10, 0.1)))),
                        MeshMaterial3d(materials.add(StandardMaterial {
                            base_color: color,
                            metallic: 0.5,
                            perceptual_roughness: 0.5,
                            ..default()
                        })),
                        Transform::from_translation(
                            offset + Vec3::new(0.0, 0.05, half_extents.x * 0.9),
                        ),
                    ));
                }
            }

            for &(body_part, offset, half_extents) in BODY_PART_HITBOXES {
                parent.spawn((
                    LimbHitbox(body_part),
                    Collider::cuboid(
                        half_extents.x * 2.0,
                        half_extents.y * 2.0,
                        half_extents.z * 2.0,
                    ),
                    CollisionLayers::new(HITBOX_LAYER, LayerMask::ALL),
                    Transform::from_translation(offset),
                ));
            }
        });
    }

    Ok(())
}

fn update_attack_state_system(
    mut cmd: Commands,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut q_attacker: Query<(Entity, &mut AttackState, &CharacterControllerCamera)>,
    q_camera: Query<&Transform>,
    q_collider: Query<(&ColliderOf, &LimbHitbox)>,
) {
    for (attacker, mut state, camera) in &mut q_attacker {
        state.pre_hit_timer.tick(time.delta());
        if !state.pre_hit_timer.is_finished() {
            continue;
        }

        cmd.entity(attacker).remove::<AttackState>();

        let Ok(camera_transform) = q_camera.get(camera.get()) else {
            warn!("Can't get camera transform to calculate attack ray");
            continue;
        };

        let origin = camera_transform.translation;
        let direction = camera_transform.forward();
        let filter = SpatialQueryFilter::from_mask(HITBOX_LAYER);
        let reach = melee_interaction_range(state.reach);

        if let Some(hit) = spatial.cast_ray(origin, direction, reach, true, &filter) {
            let Ok((target, body_part)) = q_collider.get(hit.entity).map(|(c, h)| (c.body, h.0))
            else {
                break;
            };

            cmd.client_trigger(MeleeActionRequest::complete(
                target,
                body_part,
                HIT_PRECISION,
            ));
            cmd.trigger(HitPerformed {
                entity: attacker,
                direction,
                origin,
                length: hit.distance,
            });
        } else {
            cmd.trigger(HitPerformed {
                entity: attacker,
                direction,
                origin,
                length: reach,
            });
        }
    }
}

fn on_attack_fired_hook(
    event: On<Fire<Attack>>,
    mut cmd: Commands,
    q_character: Query<Has<AttackState>>,
    viewer: TacticalPlayerViewer,
) {
    if q_character.get(event.context).unwrap_or_default() {
        // already in attack
        return;
    }
    let Ok(reach) = viewer
        .get(event.context)
        .map(|character| character.weapon_reach())
    else {
        warn!("Trying to attack, but can't get weapon reach. Not holding any weapons ?");
        return;
    };

    cmd.entity(event.context)
        .insert(AttackState::new(PRE_HIT_DELAY, reach));
    cmd.client_trigger(MeleeActionRequest::start());
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

fn on_dodge_fired(_event: On<Fire<Dodge>>, mut cmd: Commands) {
    cmd.client_trigger(DefendRequest::Dodge);
}

fn on_parry_fired(_event: On<Fire<Parry>>, mut cmd: Commands) {
    cmd.client_trigger(DefendRequest::Parry);
}
