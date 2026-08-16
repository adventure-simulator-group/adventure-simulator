use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    client::{DirectControlState, WeaponGuardInputState},
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
const HIT_PRECISION: f32 = 1.0;
/// Seconds a dead bot's detached visual takes to fade to transparent. Purely
/// a client-side presentation detail: the server despawns the authoritative
/// entity immediately on death and has no notion of this delay.
const ENEMY_DEATH_FADE_SECONDS: f32 = 2.0;
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
                    (
                        apply_direct_combat_controls,
                        update_attack_state_system.run_if(any_with_component::<AttackState>),
                        flush_buffered_melee_attacks,
                    )
                        .chain(),
                    start_fade_on_incapacitation,
                    tick_fade_out.run_if(any_with_component::<FadingOut>),
                ),
            )
            .add_systems(
                PostUpdate,
                predict_local_body_facing.before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

/// Identifies which replicated character receives local controls and the
/// gameplay camera. Kept separate from transport CLI arguments so local
/// presentation fixtures use the exact same player-spawn observer.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Resource)]
pub struct LocalCharacterId(pub u64);

#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct ClientPlayer;

/// Render-frame facing state for the locally controlled character. Replicated
/// transforms remain authoritative; this state hides the gaps between their
/// rotation samples for both camera-facing guard and velocity-facing travel.
#[derive(Component, Debug, Clone, Copy, Default)]
struct LocalBodyFacing {
    rotation: Quat,
    initialized: bool,
}

#[derive(EntityEvent)]
pub struct HitPerformed {
    pub entity: Entity,
    pub direction: Dir3,
    pub origin: Vec3,
    pub length: f32,
}

#[derive(Component, Clone, Copy)]
pub struct LimbHitbox(pub BodyPart);

/// A standalone, client-only entity holding a dead player/bot's detached body
/// meshes, fading them to transparent over [`ENEMY_DEATH_FADE_SECONDS`] before
/// despawning itself. Spawned by [`start_fade_on_incapacitation`] the moment
/// [`TacticalCombatState`] is seen to be `Incapacitated`, since by that point the
/// server may despawn (and replication may remove) the real player entity at
/// any time — this entity has no server counterpart and outlives it on
/// purpose so the fade has something to animate.
#[derive(Component)]
struct FadingOut {
    timer: Timer,
}

#[derive(Component, Default)]
pub struct AttackState {
    pub pre_hit_timer: Timer,
    pub reach: f32,
    pub ranged: bool,
}

#[derive(Component, Debug, Clone, Copy)]
struct BufferedMeleeAttack(StrikeFamily);

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
            LocalBodyFacing::default(),
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

/// Detaches another player/bot's body meshes onto a standalone [`FadingOut`]
/// corpse entity the first time their replicated [`TacticalCombatState`] is
/// seen to be `Incapacitated`. The server despawns the real entity immediately
/// on death with no fade delay of its own (see the tactical server's
/// `bot::on_authoritative_enemy_death`), so the meshes have to be moved off
/// of it before that despawn — which recursively despawns children — can take
/// them with it. Excludes the locally controlled player, which never spawns
/// any body meshes to fade (see [`on_new_player_added_hook`]).
fn start_fade_on_incapacitation(
    mut commands: Commands,
    q: Query<
        (&TacticalCombatState, &Transform, &Children),
        (Changed<TacticalCombatState>, Without<ClientPlayer>),
    >,
    q_mesh: Query<(), With<MeshMaterial3d<StandardMaterial>>>,
) {
    for (state, transform, children) in &q {
        if state.incapacitation_status() != IncapacitationStatus::Incapacitated {
            continue;
        }

        let meshes: Vec<Entity> = children
            .iter()
            .filter(|&child| q_mesh.contains(child))
            .collect();
        if meshes.is_empty() {
            // Already detached by an earlier `TacticalCombatState` change
            // (e.g. the continuing imbalance-recovery ticks), or nothing to
            // fade.
            continue;
        }

        let corpse = commands
            .spawn((
                *transform,
                Visibility::default(),
                FadingOut {
                    timer: Timer::from_seconds(ENEMY_DEATH_FADE_SECONDS, TimerMode::Once),
                },
            ))
            .id();
        for mesh in meshes {
            commands.entity(mesh).insert(ChildOf(corpse));
        }
    }
}

/// Fades a [`FadingOut`] corpse's detached meshes toward transparent and
/// despawns the corpse once the timer finishes — nothing server-side ever
/// removes it, since it has no networked counterpart. Each material handle is
/// unique per body part per player (see [`on_new_player_added_hook`]), so
/// mutating alpha here can't bleed into any other entity's appearance.
fn tick_fade_out(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q_fading: Query<(Entity, &mut FadingOut, &Children)>,
    q_material: Query<&MeshMaterial3d<StandardMaterial>>,
) {
    for (corpse, mut fading, children) in &mut q_fading {
        fading.timer.tick(time.delta());
        let alpha = (1.0 - fading.timer.fraction()).clamp(0.0, 1.0);

        for child in children.iter() {
            let Ok(material_handle) = q_material.get(child) else {
                continue;
            };
            if let Some(mut material) = materials.get_mut(&material_handle.0) {
                material.alpha_mode = AlphaMode::Blend;
                material.base_color = material.base_color.with_alpha(alpha);
            }
        }

        if fading.timer.is_finished() {
            commands.entity(corpse).despawn();
        }
    }
}

fn predict_local_body_facing(
    time: Res<Time>,
    guard: Res<WeaponGuardInputState>,
    mut players: Query<
        (
            &CharacterControllerCamera,
            &SkeletonState,
            &mut Transform,
            &mut LocalBodyFacing,
        ),
        With<ClientPlayer>,
    >,
    cameras: Query<&Transform, (With<Camera3d>, Without<ClientPlayer>)>,
) {
    for (camera, skeleton, mut transform, mut facing) in &mut players {
        if skeleton.body().is_downed() || skeleton.is_posture_transitioning() {
            facing.rotation = transform.rotation;
            facing.initialized = false;
            continue;
        }

        let Ok(camera_transform) = cameras.get(camera.get()) else {
            continue;
        };
        if !facing.initialized {
            facing.rotation = transform.rotation;
            facing.initialized = true;
        }

        facing.rotation = advance_body_facing(
            facing.rotation,
            camera_transform.rotation,
            skeleton.world_velocity,
            skeleton.action_kind(),
            guard.desired,
            time.delta_secs(),
        );
        transform.rotation = facing.rotation;
    }
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
    viewer: TacticalPlayerViewer,
    time: Res<Time>,
) {
    try_start_attack(
        event.context,
        false,
        &mut cmd,
        &mut q_character,
        &viewer,
        &time,
    );
}

fn try_start_attack(
    entity: Entity,
    alternate_attack: bool,
    cmd: &mut Commands,
    q_character: &mut Query<(Has<AttackState>, &mut SkeletonState)>,
    viewer: &TacticalPlayerViewer,
    time: &Time,
) {
    let Ok((attacking, mut skeleton)) = q_character.get_mut(entity) else {
        return;
    };
    let Ok((reach, ranged, melee, windup_secs, preferred_style)) =
        viewer.get(entity).map(|character| {
            (
                character.weapon_reach(),
                character.weapon_is_ranged(),
                character.weapon_is_melee(),
                character.weapon_windup_secs(),
                character.weapon_preferred_melee_style(),
            )
        })
    else {
        warn!("Trying to attack, but can't get weapon reach. Not holding any weapons ?");
        return;
    };

    if reach <= 0.0 || (!ranged && !melee) {
        warn!("Trying to attack without a usable equipped weapon");
        return;
    }
    let start = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
    if ranged {
        if attacking {
            return;
        }
        if skeleton
            .begin_attack(AttackSpec::default(), start, start + 19)
            .is_err()
        {
            return;
        }
        cmd.entity(entity)
            .insert(AttackState::new(windup_secs, reach, true));
        cmd.client_trigger(RangedActionRequest::Start);
    } else {
        let preferred_family = StrikeFamily::from_melee_style(preferred_style);
        let requested_family = if alternate_attack {
            preferred_family.alternate()
        } else {
            preferred_family
        };
        let Some(strike_family) = skeleton.available_strike_family(requested_family) else {
            return;
        };
        let Some(animation) = (!attacking)
            .then(|| skeleton.select_attack_animation(strike_family))
            .flatten()
        else {
            cmd.entity(entity)
                .insert(BufferedMeleeAttack(strike_family));
            return;
        };
        if skeleton
            .begin_attack(AttackSpec::new(animation), start, start + 19)
            .is_err()
        {
            return;
        }
        cmd.entity(entity)
            .insert(AttackState::new(windup_secs, reach, false))
            .remove::<BufferedMeleeAttack>();
        cmd.client_trigger(MeleeActionRequest::Start { strike_family });
    }
}

fn flush_buffered_melee_attacks(
    mut cmd: Commands,
    mut characters: Query<
        (
            Entity,
            &BufferedMeleeAttack,
            Has<AttackState>,
            &mut SkeletonState,
        ),
        With<ControlledPlayer>,
    >,
    viewer: TacticalPlayerViewer,
    time: Res<Time>,
) {
    for (entity, buffered, attacking, mut skeleton) in &mut characters {
        if attacking {
            continue;
        }
        let Some(animation) = skeleton.select_attack_animation(buffered.0) else {
            continue;
        };
        let Ok((reach, windup_secs)) = viewer
            .get(entity)
            .map(|character| (character.weapon_reach(), character.weapon_windup_secs()))
        else {
            continue;
        };
        let start = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
        if skeleton
            .begin_attack(AttackSpec::new(animation), start, start + 19)
            .is_err()
        {
            continue;
        }
        cmd.entity(entity)
            .insert(AttackState::new(windup_secs, reach, false))
            .remove::<BufferedMeleeAttack>();
        cmd.client_trigger(MeleeActionRequest::Start {
            strike_family: buffered.0,
        });
    }
}

fn apply_direct_combat_controls(
    controls: Res<DirectControlState>,
    mut cmd: Commands,
    players: Query<Entity, With<ControlledPlayer>>,
    mut q_character: Query<(Has<AttackState>, &mut SkeletonState)>,
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
                &viewer,
                &time,
            );
        }
        if controls.dodge_just_pressed {
            if let Ok((_, mut skeleton)) = q_character.get_mut(entity) {
                let start = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
                let admitted = skeleton.begin_dodge(
                    DodgeSpec {
                        direction: controls.quickstep_direction,
                    },
                    start,
                    start + 20,
                );
                if admitted.is_err() {
                    continue;
                }
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
        if skeleton
            .begin_dodge(DodgeSpec::default(), start, start + 8)
            .is_err()
        {
            return;
        }
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
        if skeleton
            .begin_block(BlockSpec::default(), start, start + 8)
            .is_err()
        {
            return;
        }
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

    #[test]
    fn local_aim_facing_advances_on_every_render_frame() {
        let camera = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let target = advance_body_facing(
            Quat::IDENTITY,
            camera,
            Vec3::ZERO,
            SkeletonAction::None,
            WeaponGuardState::Raised,
            1.0,
        );
        let mut facing = Quat::IDENTITY;
        let mut previous_distance = facing.angle_between(target);

        for _ in 0..4 {
            facing = advance_body_facing(
                facing,
                camera,
                Vec3::ZERO,
                SkeletonAction::None,
                WeaponGuardState::Raised,
                1.0 / 60.0,
            );
            let distance = facing.angle_between(target);
            assert!(distance < previous_distance);
            previous_distance = distance;
        }

        assert!(previous_distance > 0.0);
    }

    #[test]
    fn local_travel_facing_advances_toward_world_velocity() {
        let velocity = Vec3::X * 3.0;
        let target = advance_body_facing(
            Quat::IDENTITY,
            Quat::IDENTITY,
            velocity,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            1.0,
        );
        let next = advance_body_facing(
            Quat::IDENTITY,
            Quat::IDENTITY,
            velocity,
            SkeletonAction::None,
            WeaponGuardState::Lowered,
            1.0 / 60.0,
        );

        assert!(next.angle_between(target) < Quat::IDENTITY.angle_between(target));
        assert!(next.angle_between(target) > 0.0);
    }
}
