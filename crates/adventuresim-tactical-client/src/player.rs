use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    message::{DefendRequest, MeleeActionRequest, RangedActionRequest},
};
use bevy::prelude::*;

use crate::animation::spawn_fallback_t_pose;

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
        // Replication supplies Transform but not a render hierarchy root.
        // Require visibility before Add<Player> observers attach mesh children
        // so authored rigs cannot inherit from a component-less parent.
        app.register_required_components_with::<Player, _>(|| Visibility::Inherited)
            .add_observer(on_new_player_added_hook)
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

/// Identifies which replicated character receives local controls and the
/// gameplay camera. Kept separate from transport CLI arguments so local
/// presentation fixtures use the exact same player-spawn observer.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalCharacterId(pub u64);

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
    pub ranged: bool,
}

impl AttackState {
    pub fn new(pre_hit_delay: f32, reach: f32, ranged: bool) -> Self {
        let pre_hit_timer = Timer::from_seconds(pre_hit_delay, TimerMode::Once);
        Self {
            pre_hit_timer,
            reach,
            ranged,
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
    query: Query<(&Player, &CharacterId)>,
    local_character: Res<LocalCharacterId>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let (Player { name }, id) = query.get(event.entity)?;
    info!(entity = ?event.entity, id = id.0, "Added new player {name}");

    let is_client_player = local_character.0 == id.0;
    if is_client_player {
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
    }

    commands.entity(event.entity).with_children(|parent| {
        spawn_fallback_t_pose(
            parent,
            event.entity,
            id.color(),
            &mut meshes,
            &mut materials,
        );

        if !is_client_player {
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
        }
    });

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
        let reach = if state.ranged {
            state.reach
        } else {
            melee_interaction_range(state.reach)
        };

        if let Some(hit) = spatial.cast_ray(origin, direction, reach, true, &filter) {
            let Ok((target, body_part)) = q_collider.get(hit.entity).map(|(c, h)| (c.body, h.0))
            else {
                break;
            };

            if state.ranged {
                cmd.client_trigger(RangedActionRequest::CompleteHit {
                    target,
                    body_part,
                    reported_precision: HIT_PRECISION,
                });
            } else {
                cmd.client_trigger(MeleeActionRequest::Complete {
                    target,
                    body_part,
                    reported_precision: HIT_PRECISION,
                });
            }
            cmd.trigger(HitPerformed {
                entity: attacker,
                direction,
                origin,
                length: hit.distance,
            });
        } else {
            if state.ranged {
                cmd.client_trigger(RangedActionRequest::CompleteMiss);
            }
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
    mut q_character: Query<(Has<AttackState>, &mut SkeletonState)>,
    viewer: TacticalPlayerViewer,
    time: Res<Time>,
) {
    let Ok((attacking, mut skeleton)) = q_character.get_mut(event.context) else {
        return;
    };
    if attacking {
        // already in attack
        return;
    }
    let Ok((reach, ranged, melee)) = viewer.get(event.context).map(|character| {
        (
            character.weapon_reach(),
            character.weapon_is_ranged(),
            character.weapon_is_melee(),
        )
    }) else {
        warn!("Trying to attack, but can't get weapon reach. Not holding any weapons ?");
        return;
    };

    if reach <= 0.0 || (!ranged && !melee) {
        warn!("Trying to attack without a usable equipped weapon");
        return;
    }
    cmd.entity(event.context)
        .insert(AttackState::new(PRE_HIT_DELAY, reach, ranged));
    let start = (time.elapsed_secs_f64() * 64.0).round() as u64;
    skeleton.begin_action(SkeletonAction::Attack, start, start + 19);
    if ranged {
        cmd.client_trigger(RangedActionRequest::Start);
    } else {
        cmd.client_trigger(MeleeActionRequest::Start);
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

fn on_dodge_fired(
    event: On<Fire<Dodge>>,
    mut cmd: Commands,
    time: Res<Time>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    if let Ok(mut skeleton) = skeletons.get_mut(event.context) {
        let start = (time.elapsed_secs_f64() * 64.0).round() as u64;
        skeleton.begin_action(SkeletonAction::Dodge, start, start + 8);
    }
    cmd.client_trigger(DefendRequest::Dodge);
}

fn on_parry_fired(
    event: On<Fire<Parry>>,
    mut cmd: Commands,
    time: Res<Time>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    if let Ok(mut skeleton) = skeletons.get_mut(event.context) {
        let start = (time.elapsed_secs_f64() * 64.0).round() as u64;
        skeleton.begin_action(SkeletonAction::Block, start, start + 8);
    }
    cmd.client_trigger(DefendRequest::Parry);
}
