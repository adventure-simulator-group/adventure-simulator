use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    client::DirectControlState,
    message::{DefendRequest, MeleeActionRequest, RangedActionRequest},
};
use bevy::prelude::*;

use crate::{
    animation::spawn_fallback_t_pose, camera::CameraAimState, presentation::GrassInteractor,
};

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
const GAMEPAD_LOOK_SCALE: Vec3 = Vec3::new(4.0, -4.0, 4.0);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // Replication supplies Transform but not a render hierarchy root.
        // Require visibility before Add<Player> observers attach mesh children
        // so authored rigs cannot inherit from a component-less parent.
        app.init_resource::<DirectControlState>()
            .register_required_components_with::<Player, _>(|| Visibility::Inherited)
            .add_observer(on_new_player_added_hook)
            .add_observer(on_attack_fired_hook)
            .add_observer(on_dodge_fired)
            .add_observer(on_parry_fired)
            .add_systems(
                Update,
                (
                    apply_direct_combat_controls,
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
            tactical_character_controller(),
            ClientPlayer,
            GrassInteractor,
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
                    Action::<input::RotateCamera>::new(),
                    Bindings::spawn((
                        Spawn((Binding::mouse_motion(), Scale::splat(0.15))),
                        Axial::right_stick().with((
                            Scale::new(GAMEPAD_LOOK_SCALE),
                            DeadZone::default(),
                        )),
                    ))
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
    aim: Res<CameraAimState>,
    mut q_attacker: Query<(
        Entity,
        &Transform,
        &mut AttackState,
        &CharacterControllerCamera,
    )>,
    q_camera: Query<&Transform>,
    q_collider: Query<(&ColliderOf, &LimbHitbox)>,
    q_scene_items: Query<Entity, With<TacticalSceneItem>>,
) {
    for (attacker, attacker_transform, mut state, camera) in &mut q_attacker {
        state.pre_hit_timer.tick(time.delta());
        if !state.pre_hit_timer.is_finished() {
            continue;
        }

        cmd.entity(attacker).remove::<AttackState>();

        let Ok(camera_transform) = q_camera.get(camera.get()) else {
            warn!("Can't get camera transform to calculate attack ray");
            continue;
        };

        let camera_origin = camera_transform.translation;
        let camera_direction = camera_transform.forward();
        let target_filter = SpatialQueryFilter::from_mask(HITBOX_LAYER);
        let reach = if state.ranged {
            state.reach
        } else {
            melee_interaction_range(state.reach)
        };
        let selection_distance = camera_origin.distance(attacker_transform.translation) + reach;

        let intended = spatial.cast_ray(
            camera_origin,
            camera_direction,
            selection_distance,
            true,
            &target_filter,
        );
        let Some(intended) = intended else {
            if state.ranged {
                cmd.client_trigger(RangedActionRequest::CompleteMiss);
            }
            cmd.trigger(HitPerformed {
                entity: attacker,
                direction: camera_direction,
                origin: camera_origin,
                length: selection_distance,
            });
            continue;
        };
        let Ok((target, body_part)) = q_collider
            .get(intended.entity)
            .map(|(collider, limb)| (collider.body, limb.0))
        else {
            continue;
        };
        let intended_point = camera_origin + *camera_direction * intended.distance;
        let origin = if state.ranged && aim.active {
            aim.muzzle_origin
        } else {
            attacker_transform.translation + Vec3::Y * 0.5
        };
        let delta = intended_point - origin;
        let direction = Dir3::new(delta).unwrap_or(camera_direction);
        let excluded: Vec<_> = q_scene_items.iter().chain([attacker]).collect();
        let obstruction_filter = SpatialQueryFilter::from_excluded_entities(excluded);
        let obstruction = spatial.cast_ray(
            origin,
            direction,
            delta.length().min(reach),
            true,
            &obstruction_filter,
        );
        let unobstructed = obstruction.is_some_and(|hit| {
            hit.entity == target
                || q_collider
                    .get(hit.entity)
                    .is_ok_and(|(collider, _)| collider.body == target)
        });

        if unobstructed {
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
                length: obstruction.map_or(delta.length(), |hit| hit.distance),
            });
        } else {
            if state.ranged {
                cmd.client_trigger(RangedActionRequest::CompleteMiss);
            }
            cmd.trigger(HitPerformed {
                entity: attacker,
                direction,
                origin,
                length: obstruction.map_or(delta.length().min(reach), |hit| hit.distance),
            });
        }
    }
}

fn on_attack_fired_hook(
    event: On<Fire<Attack>>,
    mut cmd: Commands,
    mut q_character: Query<(Has<AttackState>, &mut SkeletonState)>,
    q_leg_ik: Query<&crate::animation::LegIkState>,
    viewer: TacticalPlayerViewer,
    time: Res<Time>,
) {
    try_start_attack(
        event.context,
        false,
        &mut cmd,
        &mut q_character,
        &q_leg_ik,
        &viewer,
        &time,
    );
}

fn try_start_attack(
    entity: Entity,
    alternate_attack: bool,
    cmd: &mut Commands,
    q_character: &mut Query<(Has<AttackState>, &mut SkeletonState)>,
    q_leg_ik: &Query<&crate::animation::LegIkState>,
    viewer: &TacticalPlayerViewer,
    time: &Time,
) {
    let Ok((attacking, mut skeleton)) = q_character.get_mut(entity) else {
        return;
    };
    if attacking {
        return;
    }
    let Ok((reach, ranged, melee, preferred_style)) = viewer.get(entity).map(|character| {
        (
            character.weapon_reach(),
            character.weapon_is_ranged(),
            character.weapon_is_melee(),
            character.weapon_preferred_melee_style(),
        )
    }) else {
        warn!("Trying to attack, but can't get weapon reach. Not holding any weapons ?");
        return;
    };

    if reach <= 0.0 || (!ranged && !melee) {
        warn!("Trying to attack without a usable equipped weapon");
        return;
    }
    cmd.entity(entity)
        .insert(AttackState::new(PRE_HIT_DELAY, reach, ranged));
    let start = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
    let predicted = if ranged {
        AttackSpec::default()
    } else {
        // Predict only from the last physical velocity already present in the
        // replicated skeleton. The authority repeats the same typed
        // classification from its observation; no input vector crosses the
        // reliable action boundary.
        let preferred_family = StrikeFamily::from_melee_style(preferred_style);
        let strike_family = if alternate_attack {
            match preferred_family {
                StrikeFamily::Slash => StrikeFamily::Thrust,
                StrikeFamily::Thrust => StrikeFamily::Slash,
            }
        } else {
            preferred_family
        };
        let moving = skeleton.local_velocity.xz().length() > 0.05;
        let feet_close = q_leg_ik
            .get(entity)
            .ok()
            .and_then(crate::animation::LegIkState::feet_are_close_for_attack)
            .unwrap_or(false);
        let footwork = if moving {
            if feet_close {
                Footwork::Stay
            } else {
                Footwork::Switch
            }
        } else if strike_family == StrikeFamily::Slash {
            Footwork::Switch
        } else {
            Footwork::Stay
        };
        AttackSpec::melee_from_local_velocity_and_style(
            skeleton.local_velocity,
            strike_family,
            footwork,
        )
    };
    skeleton.begin_attack(predicted, start, start + 19);
    if ranged {
        cmd.client_trigger(RangedActionRequest::Start);
    } else {
        cmd.client_trigger(MeleeActionRequest::Start {
            strike_family: predicted.strike_family,
            footwork: predicted.footwork,
        });
    }
}

fn apply_direct_combat_controls(
    controls: Res<DirectControlState>,
    mut cmd: Commands,
    players: Query<Entity, With<ControlledPlayer>>,
    mut q_character: Query<(Has<AttackState>, &mut SkeletonState)>,
    q_leg_ik: Query<&crate::animation::LegIkState>,
    viewer: TacticalPlayerViewer,
    time: Res<Time>,
) {
    for entity in &players {
        if controls.attack_just_pressed {
            try_start_attack(
                entity,
                controls.alternate_attack,
                &mut cmd,
                &mut q_character,
                &q_leg_ik,
                &viewer,
                &time,
            );
        }
        if controls.dodge_just_pressed {
            if let Ok((_, mut skeleton)) = q_character.get_mut(entity) {
                let start = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
                skeleton.begin_dodge(DodgeSpec::default(), start, start + 8);
            }
            cmd.client_trigger(DefendRequest::Dodge);
        }
        if controls.roll_just_pressed {
            cmd.client_trigger(DefendRequest::Roll);
        }
    }
}

fn on_dodge_fired(
    event: On<Fire<Dodge>>,
    mut cmd: Commands,
    time: Res<Time>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    if let Ok(mut skeleton) = skeletons.get_mut(event.context) {
        let start = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
        skeleton.begin_dodge(DodgeSpec::default(), start, start + 8);
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
        let start = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
        skeleton.begin_block(BlockSpec::default(), start, start + 8);
    }
    cmd.client_trigger(DefendRequest::Parry);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamepad_look_keeps_horizontal_and_reverses_vertical_input() {
        assert!(GAMEPAD_LOOK_SCALE.x.is_sign_positive());
        assert!(GAMEPAD_LOOK_SCALE.y.is_sign_negative());
    }
}
