use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    client::{DirectControlState, WeaponGuardInputState},
    message::{DefendRequest, MeleeActionRequest, RangedActionRequest},
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::{
    animation::spawn_fallback_t_pose,
    presentation::{GrassInteractor, TacticalGameplayCamera},
    targeting::auto_aim_candidate,
};

const HITBOX_LAYER: LayerMask = LayerMask(1 << 1);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // Replication supplies Transform but not a render hierarchy root.
        // Require visibility before Add<Player> observers attach mesh children
        // so authored rigs cannot inherit from a component-less parent.
        app.init_resource::<DirectControlState>()
            .init_resource::<TacticalCombatConfig>()
            .register_required_components_with::<Player, _>(|| Visibility::Inherited)
            .add_observer(on_new_player_added_hook)
            .add_observer(on_attack_fired_hook)
            .add_observer(on_parry_fired)
            .add_systems(
                Update,
                ((
                    apply_direct_combat_controls,
                    update_attack_state_system.run_if(any_with_component::<AttackState>),
                    flush_buffered_melee_attacks,
                )
                    .chain(),),
            )
            .add_systems(Update, trace_local_quickstep_state)
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

#[derive(Component, Debug, Clone, Copy, Default)]
struct LocalQuickstepTrace {
    initialized: bool,
    dodge_action: bool,
    push_active: bool,
    push_start_tick: u64,
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

#[derive(Component)]
pub struct AttackState {
    pub pre_hit_timer: Timer,
    facing_timer: Timer,
    pub reach: f32,
    pub ranged: bool,
    target: Option<AttackTarget>,
    target_position: Option<Vec3>,
    aim_direction: Dir3,
}

#[derive(Debug, Clone, Copy)]
struct AttackTarget {
    body: Entity,
    hitbox: Entity,
    body_part: BodyPart,
    local_strike_point: Vec3,
    contact_delay_seconds: f32,
}

#[derive(Clone, Copy)]
struct BodyStableChoice<T> {
    body: Entity,
    resolved: Option<(T, Vec3)>,
    ideal_position: Vec3,
    stable_key: u64,
}

fn body_stable_auto_aim<T: Copy>(
    camera_origin: Vec3,
    camera_forward: Vec3,
    actor_origin: Vec3,
    maximum_distance: f32,
    choices: &[BodyStableChoice<T>],
) -> Option<(T, Vec3)> {
    let mut exhausted = Vec::new();
    loop {
        let body = auto_aim_candidate(
            camera_origin,
            camera_forward,
            actor_origin,
            maximum_distance,
            choices
                .iter()
                .filter(|choice| !exhausted.contains(&choice.body))
                .map(|choice| (choice.body, choice.ideal_position, choice.stable_key)),
        )?;
        if let Some(resolved) = auto_aim_candidate(
            camera_origin,
            camera_forward,
            actor_origin,
            maximum_distance,
            choices
                .iter()
                .filter(|choice| choice.body == body)
                .filter_map(|choice| {
                    choice
                        .resolved
                        .map(|resolved| (resolved, resolved.1, choice.stable_key))
                }),
        ) {
            return Some(resolved);
        }
        exhausted.push(body);
    }
}

#[derive(SystemParam)]
struct CombatTargeting<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    attackers: Query<
        'w,
        's,
        (
            &'static Transform,
            &'static Collider,
            &'static CharacterDimensions,
            &'static TacticalCombatSide,
            &'static CharacterControllerCamera,
        ),
    >,
    cameras: Query<'w, 's, &'static Transform, With<TacticalGameplayCamera>>,
    hitboxes: Query<
        'w,
        's,
        (
            Entity,
            &'static GlobalTransform,
            &'static Collider,
            &'static ColliderOf,
            &'static LimbHitbox,
        ),
    >,
    combatants: Query<
        'w,
        's,
        (
            &'static TacticalCombatSide,
            &'static TacticalCombatState,
            &'static Transform,
            &'static Collider,
        ),
    >,
    scene_items: Query<'w, 's, Entity, With<TacticalSceneItem>>,
}

#[derive(Component, Debug, Clone, Copy)]
struct BufferedMeleeAttack {
    family: StrikeFamily,
    hand: AttackHand,
}

impl AttackState {
    fn new(
        pre_hit_delay: f32,
        facing_delay: f32,
        reach: f32,
        ranged: bool,
        target: Option<AttackTarget>,
        target_position: Option<Vec3>,
        aim_direction: Dir3,
    ) -> Self {
        let pre_hit_timer = Timer::from_seconds(pre_hit_delay, TimerMode::Once);
        let facing_timer = Timer::from_seconds(facing_delay, TimerMode::Once);
        Self {
            pre_hit_timer,
            facing_timer,
            reach,
            ranged,
            target,
            target_position,
            aim_direction,
        }
    }

    pub fn is_attacking(&self) -> bool {
        !self.pre_hit_timer.is_paused() && !self.pre_hit_timer.is_finished()
    }
}

impl CombatTargeting<'_, '_> {
    fn arm_reach(&self, attacker: Entity) -> f32 {
        self.attackers
            .get(attacker)
            .map_or(0.0, |(_, _, dimensions, _, _)| dimensions.arm_reach_metres)
    }

    fn melee_acquisition_range(
        &self,
        attacker: Entity,
        weapon_reach: f32,
        config: &TacticalCombatConfig,
    ) -> (f32, f32) {
        let leg_length = self.attackers.get(attacker).map_or(
            config.movement.motor.reference_quickstep_leg_length_metres,
            |(_, _, dimensions, _, _)| dimensions.leg_length_metres,
        );
        (
            melee_interaction_range(self.arm_reach(attacker), weapon_reach),
            quickstep_target_displacement_metres(leg_length, &config.movement.motor),
        )
    }

    fn acquire(
        &self,
        attacker: Entity,
        reach: (f32, f32),
        surface_range: bool,
        lunge_timing: (f32, f32, f32),
    ) -> (Option<AttackTarget>, Option<Vec3>, Dir3) {
        let Ok((transform, collider, dimensions, attacker_side, camera)) =
            self.attackers.get(attacker)
        else {
            return (None, None, Dir3::NEG_Z);
        };
        let Ok(camera_transform) = self.cameras.get(camera.get()) else {
            return (None, None, Dir3::NEG_Z);
        };
        let origin = attack_origin(transform, collider, *dimensions);
        let excluded: Vec<_> = self.scene_items.iter().chain([attacker]).collect();
        let obstruction_filter = SpatialQueryFilter::from_excluded_entities(excluded);
        let choices: Vec<_> = self
            .hitboxes
            .iter()
            .filter_map(
                |(hitbox, target_transform, hitbox_collider, collider_of, limb)| {
                    let target = collider_of.body;
                    let Ok((target_side, target_state, body_transform, body_collider)) =
                        self.combatants.get(target)
                    else {
                        return None;
                    };
                    if target_side == attacker_side || target_state.is_incapacitated() {
                        return None;
                    }
                    let position = target_transform.translation();
                    let sight = position - origin;
                    let distance = sight.length();
                    let direction = Dir3::new(sight).ok()?;
                    let impact = (distance > f32::EPSILON)
                        .then(|| {
                            self.spatial.cast_ray(
                                origin,
                                direction,
                                distance,
                                true,
                                &obstruction_filter,
                            )
                        })
                        .flatten()?;
                    let visible = impact.entity == target
                        || impact.entity == hitbox
                        || self.hitboxes.get(impact.entity).is_ok_and(
                            |(_, _, _, impact_collider, _)| impact_collider.body == target,
                        );
                    let surface_distance = impact.distance;
                    let resolved_strike = if surface_range {
                        let maximum_travel = reach.1.min(horizontal_collider_clearance(
                            transform.translation,
                            collider,
                            body_transform,
                            body_collider,
                        ));
                        let travel_direction =
                            (body_transform.translation - transform.translation).xz();
                        reachable_melee_strike_point(
                            hitbox_collider,
                            target_transform.translation(),
                            target_transform.rotation(),
                            origin,
                            travel_direction,
                            reach.0,
                            maximum_travel,
                        )
                        .map(|(point, closure)| {
                            let lunge = melee_lunge(reach.0 + closure, reach.0, 0.0, reach.1);
                            (
                                point,
                                melee_lunge_delay_seconds(
                                    lunge,
                                    lunge_timing.0,
                                    lunge_timing.1,
                                    lunge_timing.2,
                                ),
                            )
                        })
                    } else {
                        Some((position, 0.0))
                    };
                    if !attack_target_within_angular_threshold(
                        camera_transform.forward().as_vec3(),
                        position - camera_transform.translation,
                        surface_distance,
                    ) {
                        return None;
                    }
                    visible.then_some(BodyStableChoice {
                        body: target,
                        resolved: resolved_strike.map(|(strike_point, contact_delay_seconds)| {
                            (
                                AttackTarget {
                                    body: target,
                                    hitbox,
                                    body_part: limb.0,
                                    local_strike_point: target_transform
                                        .affine()
                                        .inverse()
                                        .transform_point3(strike_point),
                                    contact_delay_seconds,
                                },
                                strike_point,
                            )
                        }),
                        ideal_position: position,
                        stable_key: hitbox.to_bits(),
                    })
                },
            )
            .collect();
        let selected = body_stable_auto_aim(
            camera_transform.translation,
            camera_transform.forward().as_vec3(),
            origin,
            if surface_range { f32::MAX } else { reach.0 },
            &choices,
        );
        match selected {
            Some((target, position)) => (Some(target), Some(position), camera_transform.forward()),
            None => (None, None, camera_transform.forward()),
        }
    }
}

fn attack_origin(
    transform: &Transform,
    collider: &Collider,
    dimensions: CharacterDimensions,
) -> Vec3 {
    let body_base = collider
        .aabb(transform.translation, Rotation(transform.rotation))
        .min
        .y;
    transform
        .translation
        .with_y(body_base + dimensions.body_height_metres * (2.0 / 3.0))
}

fn horizontal_collider_clearance(
    attacker: Vec3,
    attacker_collider: &Collider,
    target_transform: &Transform,
    target_collider: &Collider,
) -> f32 {
    let attacker_bounds = attacker_collider.aabb(attacker, Rotation::default());
    let target_bounds = target_collider.aabb(
        target_transform.translation,
        Rotation(target_transform.rotation),
    );
    let attacker_radius = ((attacker_bounds.max - attacker_bounds.min) * 0.5)
        .xz()
        .max_element();
    let target_radius = ((target_bounds.max - target_bounds.min) * 0.5)
        .xz()
        .max_element();
    (attacker.xz().distance(target_transform.translation.xz()) - attacker_radius - target_radius)
        .max(0.0)
}

fn attack_target_angular_threshold_degrees(surface_distance_metres: f32) -> f32 {
    90.0 / surface_distance_metres.max(0.5)
}

fn attack_target_within_angular_threshold(
    camera_forward: Vec3,
    direction_to_candidate: Vec3,
    surface_distance_metres: f32,
) -> bool {
    let Some(camera_forward) = camera_forward.try_normalize() else {
        return false;
    };
    let Some(direction) = direction_to_candidate.try_normalize() else {
        return false;
    };
    let threshold = attack_target_angular_threshold_degrees(surface_distance_metres).to_radians();
    camera_forward.dot(direction).clamp(-1.0, 1.0) >= threshold.cos()
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects player ownership, camera, mesh, material, and combat configuration state independently"
)]
fn on_new_player_added_hook(
    event: On<Add, Player>,
    mut commands: Commands,
    camera: Single<Entity, With<TacticalGameplayCamera>>,
    query: Query<(&Player, &CharacterId)>,
    local_character: Res<LocalCharacterId>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    combat_config: Res<TacticalCombatConfig>,
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
            LocalQuickstepTrace::default(),
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
                            Scale::new(Vec3::from_array(
                                combat_config.client_input.gamepad_look_scale,
                            )),
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
            for hitbox in &combat_config.targeting.body_part_hitboxes {
                let half_extents = Vec3::from_array(hitbox.half_extents_metres);
                parent.spawn((
                    LimbHitbox(hitbox.body_part),
                    Collider::cuboid(
                        half_extents.x * 2.0,
                        half_extents.y * 2.0,
                        half_extents.z * 2.0,
                    ),
                    CollisionLayers::new(HITBOX_LAYER, LayerMask::ALL),
                    Transform::from_translation(Vec3::from_array(hitbox.center_metres)),
                ));
            }
        }
    });

    Ok(())
}

fn trace_local_quickstep_state(
    mut players: Query<
        (
            Entity,
            &SkeletonState,
            &CharacterMotionSnapshot,
            &Transform,
            &mut LocalQuickstepTrace,
        ),
        With<ControlledPlayer>,
    >,
) {
    for (entity, skeleton, snapshot, transform, mut trace) in &mut players {
        let dodge_action = skeleton.action_kind() == SkeletonAction::Dodge;
        let push = snapshot.quickstep_push;
        let changed = !trace.initialized
            || trace.dodge_action != dodge_action
            || trace.push_active != push.active
            || trace.push_start_tick != push.start_tick;
        if !changed {
            continue;
        }

        let attack_lunge = snapshot.melee_lunge.is_some_and(|lunge| lunge.quickstep);
        if (dodge_action || attack_lunge) != push.active {
            warn!(
                target: "quickstep_trace",
                ?entity,
                dodge_action,
                action_direction = ?skeleton.action_direction(),
                skeleton_tick = skeleton.locomotion_sample_tick,
                push_active = push.active,
                push_start_tick = push.start_tick,
                push_direction = ?push.direction,
                acknowledged_input_tick = snapshot.acknowledged_input_tick,
                render_translation = ?transform.translation,
                snapshot_translation = ?snapshot.translation,
                "[quickstep][client-state] animation/actuator discrepancy"
            );
        } else if dodge_action || push.active || trace.dodge_action || trace.push_active {
            info!(
                target: "quickstep_trace",
                ?entity,
                dodge_action,
                action_direction = ?skeleton.action_direction(),
                skeleton_tick = skeleton.locomotion_sample_tick,
                push_active = push.active,
                push_start_tick = push.start_tick,
                push_direction = ?push.direction,
                acknowledged_input_tick = snapshot.acknowledged_input_tick,
                render_translation = ?transform.translation,
                snapshot_translation = ?snapshot.translation,
                "[quickstep][client-state] transition"
            );
        }

        trace.initialized = true;
        trace.dodge_action = dodge_action;
        trace.push_active = push.active;
        trace.push_start_tick = push.start_tick;
    }
}

#[expect(
    clippy::type_complexity,
    reason = "the Bevy query selects the complete local-facing state while excluding the gameplay camera from player transforms"
)]
fn predict_local_body_facing(
    time: Res<Time>,
    guard: Res<WeaponGuardInputState>,
    combat_config: Res<TacticalCombatConfig>,
    mut players: Query<
        (
            &CharacterControllerCamera,
            &SkeletonState,
            Option<&AttackState>,
            &mut Transform,
            &mut LocalBodyFacing,
        ),
        With<ClientPlayer>,
    >,
    cameras: Query<&Transform, (With<TacticalGameplayCamera>, Without<ClientPlayer>)>,
) {
    for (camera, skeleton, attack, mut transform, mut facing) in &mut players {
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

        facing.rotation = if let Some((target_position, remaining_seconds)) =
            attack.and_then(|state| {
                state
                    .target_position
                    .map(|target| (target, state.facing_timer.remaining_secs()))
            }) {
            let desired_forward = target_position - transform.translation;
            let turn_speed = body_turn_speed_for_deadline(
                facing.rotation,
                desired_forward,
                remaining_seconds,
                time.delta_secs(),
            );
            advance_body_facing_toward(
                facing.rotation,
                desired_forward,
                time.delta_secs(),
                turn_speed,
            )
        } else {
            advance_body_facing_with_speed(
                facing.rotation,
                camera_transform.rotation,
                skeleton.world_velocity,
                skeleton.action_kind(),
                guard.desired,
                time.delta_secs(),
                std::f32::consts::PI / combat_config.presentation.body_turn_seconds_per_half_turn,
            )
        };
        transform.rotation = facing.rotation;
    }
}

fn update_attack_state_system(
    mut cmd: Commands,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut q_attacker: Query<(
        Entity,
        &Transform,
        &Collider,
        &CharacterDimensions,
        &mut AttackState,
    )>,
    q_collider: Query<(&GlobalTransform, &ColliderOf, &LimbHitbox)>,
    q_scene_items: Query<Entity, With<TacticalSceneItem>>,
    combat_config: Res<TacticalCombatConfig>,
) {
    for (attacker, attacker_transform, attacker_collider, dimensions, mut state) in &mut q_attacker
    {
        if let Some(target) = state.target
            && let Ok((transform, _, _)) = q_collider.get(target.hitbox)
        {
            state.target_position = Some(transform.transform_point(target.local_strike_point));
        }
        state.pre_hit_timer.tick(time.delta());
        state.facing_timer.tick(time.delta());
        if !state.pre_hit_timer.is_finished() {
            continue;
        }

        cmd.entity(attacker).remove::<AttackState>();
        let reach = if state.ranged {
            state.reach
        } else {
            melee_interaction_range(dimensions.arm_reach_metres, state.reach)
        };
        let origin = attack_origin(attacker_transform, attacker_collider, *dimensions);
        let Some(target) = state.target else {
            if state.ranged {
                cmd.client_trigger(RangedActionRequest::CompleteMiss);
            }
            cmd.trigger(HitPerformed {
                entity: attacker,
                direction: state.aim_direction,
                origin,
                length: reach,
            });
            continue;
        };
        let intended_point = state
            .target_position
            .unwrap_or(origin + *state.aim_direction * reach);
        let delta = intended_point - origin;
        let direction = Dir3::new(delta).unwrap_or(state.aim_direction);
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
            hit.entity == target.body
                || q_collider
                    .get(hit.entity)
                    .is_ok_and(|(_, collider, _)| collider.body == target.body)
        });

        if unobstructed {
            if state.ranged {
                cmd.client_trigger(RangedActionRequest::CompleteHit {
                    target: target.body,
                    body_part: target.body_part,
                    reported_precision: combat_config.targeting.reported_hit_precision,
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
    combat_config: Res<TacticalCombatConfig>,
    targeting: CombatTargeting,
) {
    try_start_attack(
        AttackStartRequest {
            entity: event.context,
            alternate_attack: false,
            hand: AttackHand::Main,
        },
        &mut cmd,
        &mut q_character,
        &viewer,
        &time,
        &combat_config,
        &targeting,
    );
}

#[derive(Debug, Clone, Copy)]
struct AttackStartRequest {
    entity: Entity,
    alternate_attack: bool,
    hand: AttackHand,
}

fn try_start_attack(
    request: AttackStartRequest,
    cmd: &mut Commands,
    q_character: &mut Query<(Has<AttackState>, &mut SkeletonState)>,
    viewer: &TacticalPlayerViewer,
    time: &Time,
    combat_config: &TacticalCombatConfig,
    targeting: &CombatTargeting,
) {
    let AttackStartRequest {
        entity,
        alternate_attack,
        hand,
    } = request;
    let Ok((attacking, mut skeleton)) = q_character.get_mut(entity) else {
        return;
    };
    let Ok((reach, ranged, melee, windup_secs, recovery_secs, curve, preferred_style)) =
        viewer.get_for_attack(entity, hand).map(|character| {
            (
                character.weapon_reach(),
                hand == AttackHand::Main && character.weapon_is_ranged(),
                character.weapon_is_melee(),
                character.weapon_ranged_windup_secs(),
                attack_recovery_secs(&character, character.weapon_preferred_melee_style(), false),
                configure_attack_curve(
                    AttackSpec::default(),
                    &character,
                    &combat_config.presentation.attack_curve,
                )
                .curve,
                character.weapon_preferred_melee_style(),
            )
        })
    else {
        warn!("Trying to attack, but can't get weapon reach. Not holding any weapons ?");
        return;
    };

    let arm_reach = targeting.arm_reach(entity);
    if !attack_reach_is_usable(ranged, melee, reach, arm_reach) {
        warn!("Trying to attack without a usable equipped weapon");
        return;
    }
    let start = (time.elapsed_secs_f64() * locomotion_sample_hz() as f64).round() as u64;
    if ranged {
        if attacking {
            return;
        }
        let spec = AttackSpec { curve, ..default() };
        if skeleton
            .begin_attack_timed(
                spec,
                start,
                start + animation_ticks(windup_secs),
                start + animation_ticks(windup_secs + recovery_secs),
            )
            .is_err()
        {
            return;
        }
        let (target, target_position, aim_direction) =
            targeting.acquire(entity, (reach, 0.0), false, (0.0, 0.0, 0.0));
        cmd.entity(entity).insert(AttackState::new(
            windup_secs,
            windup_secs,
            reach,
            true,
            target,
            target_position,
            aim_direction,
        ));
        cmd.client_trigger(RangedActionRequest::Start {
            target: target.map(|target| target.body),
        });
    } else {
        let preferred_family = StrikeFamily::from_melee_style(preferred_style);
        let requested_family = if alternate_attack && hand == AttackHand::Main {
            preferred_family.alternate()
        } else {
            preferred_family
        };
        let Some(strike_family) = skeleton.available_strike_family(requested_family) else {
            return;
        };
        let Some(mut spec) = (!attacking)
            .then(|| match hand {
                AttackHand::Main => skeleton.select_main_attack(strike_family),
                AttackHand::Offhand => skeleton.select_offhand_attack(strike_family),
            })
            .flatten()
        else {
            cmd.entity(entity).insert(BufferedMeleeAttack {
                family: strike_family,
                hand,
            });
            return;
        };
        let style = strike_family.melee_style();
        let Ok((windup_secs, recovery_secs, curve)) =
            viewer.get_for_attack(entity, hand).map(|character| {
                (
                    attack_preparation_secs(&character, style),
                    attack_recovery_secs(&character, style, spec.continuation),
                    configure_attack_curve(
                        AttackSpec::default(),
                        &character,
                        &combat_config.presentation.attack_curve,
                    )
                    .curve,
                )
            })
        else {
            return;
        };
        let acquisition_range = targeting.melee_acquisition_range(entity, reach, combat_config);
        let (target, target_position, aim_direction) = targeting.acquire(
            entity,
            acquisition_range,
            true,
            (
                combat_config.movement.speeds_metres_per_second.run,
                conservative_forward_lunge_acceleration(&combat_config.movement.motor),
                combat_config.movement.maneuvers.quickstep_duration_seconds,
            ),
        );
        let contact_delay = target.map_or(0.0, |target| target.contact_delay_seconds);
        let (animation_delay, contact_seconds) =
            delayed_melee_contact_seconds(windup_secs, contact_delay);
        spec.curve = curve;
        let sequence_start = if spec.continuation {
            skeleton.action_end_tick().unwrap_or(start)
        } else {
            start
        };
        let animation_start = sequence_start + delay_ticks(animation_delay);
        let contact_tick = sequence_start + animation_ticks(contact_seconds);
        let recovery_tick = sequence_start + animation_ticks(contact_seconds + recovery_secs);
        let contact_from_input = melee_contact_delay_from_input(
            &skeleton,
            spec.continuation,
            contact_tick,
            contact_seconds,
        );
        match skeleton.begin_attack_timed(spec, animation_start, contact_tick, recovery_tick) {
            Ok(()) => {}
            Err(ActionTransitionError::ActionBusy) => {
                cmd.entity(entity).insert(BufferedMeleeAttack {
                    family: strike_family,
                    hand,
                });
                return;
            }
            Err(ActionTransitionError::Downed | ActionTransitionError::PostureTransitionActive) => {
                return;
            }
        }
        cmd.entity(entity)
            .insert(AttackState::new(
                contact_from_input,
                contact_from_input,
                reach,
                false,
                target,
                target_position,
                aim_direction,
            ))
            .remove::<BufferedMeleeAttack>();
        cmd.client_trigger(MeleeActionRequest {
            strike_family,
            hand,
            target: target.map(|target| target.body),
            body_part: target.map(|target| target.body_part),
        });
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
    combat_config: Res<TacticalCombatConfig>,
    targeting: CombatTargeting,
) {
    for (entity, buffered, attacking, mut skeleton) in &mut characters {
        if attacking {
            continue;
        }
        let Some(spec) = (match buffered.hand {
            AttackHand::Main => skeleton.select_main_attack(buffered.family),
            AttackHand::Offhand => skeleton.select_offhand_attack(buffered.family),
        }) else {
            continue;
        };
        let Ok((reach, melee, windup_secs, recovery_secs, curve)) = viewer
            .get_for_attack(entity, buffered.hand)
            .map(|character| {
                (
                    character.weapon_reach(),
                    character.weapon_is_melee(),
                    attack_preparation_secs(&character, buffered.family.melee_style()),
                    attack_recovery_secs(
                        &character,
                        buffered.family.melee_style(),
                        spec.continuation,
                    ),
                    configure_attack_curve(
                        AttackSpec::default(),
                        &character,
                        &combat_config.presentation.attack_curve,
                    )
                    .curve,
                )
            })
        else {
            continue;
        };
        if !attack_reach_is_usable(false, melee, reach, targeting.arm_reach(entity)) {
            continue;
        }
        let start = (time.elapsed_secs_f64() * locomotion_sample_hz() as f64).round() as u64;
        let acquisition_range = targeting.melee_acquisition_range(entity, reach, &combat_config);
        let (target, target_position, aim_direction) = targeting.acquire(
            entity,
            acquisition_range,
            true,
            (
                combat_config.movement.speeds_metres_per_second.run,
                conservative_forward_lunge_acceleration(&combat_config.movement.motor),
                combat_config.movement.maneuvers.quickstep_duration_seconds,
            ),
        );
        let contact_delay = target.map_or(0.0, |target| target.contact_delay_seconds);
        let (animation_delay, contact_seconds) =
            delayed_melee_contact_seconds(windup_secs, contact_delay);
        let spec = AttackSpec { curve, ..spec };
        let sequence_start = if spec.continuation {
            skeleton.action_end_tick().unwrap_or(start)
        } else {
            start
        };
        let animation_start = sequence_start + delay_ticks(animation_delay);
        let contact_tick = sequence_start + animation_ticks(contact_seconds);
        let recovery_tick = sequence_start + animation_ticks(contact_seconds + recovery_secs);
        let contact_from_input = melee_contact_delay_from_input(
            &skeleton,
            spec.continuation,
            contact_tick,
            contact_seconds,
        );
        if skeleton
            .begin_attack_timed(spec, animation_start, contact_tick, recovery_tick)
            .is_err()
        {
            continue;
        }
        cmd.entity(entity)
            .insert(AttackState::new(
                contact_from_input,
                contact_from_input,
                reach,
                false,
                target,
                target_position,
                aim_direction,
            ))
            .remove::<BufferedMeleeAttack>();
        cmd.client_trigger(MeleeActionRequest {
            strike_family: buffered.family,
            hand: buffered.hand,
            target: target.map(|target| target.body),
            body_part: target.map(|target| target.body_part),
        });
    }
}

fn attack_reach_is_usable(ranged: bool, melee: bool, weapon_reach: f32, arm_reach: f32) -> bool {
    if ranged {
        weapon_reach.is_finite() && weapon_reach > 0.0
    } else {
        melee && arm_reach.is_finite() && arm_reach > 0.0
    }
}

fn animation_ticks(seconds: f32) -> u64 {
    (seconds.max(1.0 / locomotion_sample_hz()) * locomotion_sample_hz()).round() as u64
}

fn delay_ticks(seconds: f32) -> u64 {
    (seconds.max(0.0) * locomotion_sample_hz()).round() as u64
}

fn delayed_melee_contact_seconds(authored_windup: f32, predicted_arrival: f32) -> (f32, f32) {
    let contact = authored_windup.max(predicted_arrival).max(0.0);
    ((contact - authored_windup.max(0.0)).max(0.0), contact)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects direct controls, player queries, combat resources, and targeting state independently"
)]
fn apply_direct_combat_controls(
    controls: Res<DirectControlState>,
    mut cmd: Commands,
    players: Query<Entity, With<ControlledPlayer>>,
    mut q_character: Query<(Has<AttackState>, &mut SkeletonState)>,
    viewer: TacticalPlayerViewer,
    time: Res<Time>,
    combat_config: Res<TacticalCombatConfig>,
    targeting: CombatTargeting,
) {
    for entity in &players {
        if let Ok((_, mut skeleton)) = q_character.get_mut(entity)
            && let Ok(view) = viewer.get_for_attack(entity, AttackHand::Main)
        {
            let preferred = StrikeFamily::from_melee_style(view.weapon_preferred_melee_style());
            let resolve = |input: MeleePreparationInput| match input {
                MeleePreparationInput::Preferred => AttackAnimation::initial(
                    skeleton
                        .available_strike_family(preferred)
                        .unwrap_or(preferred),
                ),
                MeleePreparationInput::Alternate => AttackAnimation::initial(
                    skeleton
                        .available_strike_family(preferred.alternate())
                        .unwrap_or(preferred),
                ),
                MeleePreparationInput::Offhand
                    if skeleton.attack_animations.offhand_preparation =>
                {
                    AttackAnimation::Offhand
                }
                MeleePreparationInput::Offhand => AttackAnimation::initial(
                    skeleton
                        .available_strike_family(preferred)
                        .unwrap_or(preferred),
                ),
            };
            let from = resolve(controls.local_preparation_from);
            let to = resolve(controls.local_preparation_to);
            skeleton.set_attack_preparation(AttackPreparation {
                from,
                to,
                progress: controls.local_preparation_progress,
            });
        }
        if controls.attack_just_pressed {
            try_start_attack(
                AttackStartRequest {
                    entity,
                    alternate_attack: controls.alternate_attack,
                    hand: controls.attack_hand,
                },
                &mut cmd,
                &mut q_character,
                &viewer,
                &time,
                &combat_config,
                &targeting,
            );
        }
        // Quicksteps travel on the sequenced PlayerInputRequest stream. Do not
        // predict only the SkeletonState here: the client does not simulate the
        // matching QuickstepPush, so an authoritative rejection would otherwise
        // animate a dodge over a stationary replicated transform.
        if controls.roll_just_pressed {
            cmd.client_trigger(DefendRequest::Roll);
        }
    }
}

fn on_parry_fired(
    event: On<Fire<Parry>>,
    mut cmd: Commands,
    time: Res<Time>,
    mut skeletons: Query<&mut SkeletonState>,
) {
    if let Ok(mut skeleton) = skeletons.get_mut(event.context) {
        let start = (time.elapsed_secs_f64() * locomotion_sample_hz() as f64).round() as u64;
        let _ = skeleton.begin_block(BlockSpec::default(), start, start + 8);
    }
    cmd.client_trigger(DefendRequest::Parry);
}

fn melee_contact_delay_from_input(
    skeleton: &SkeletonState,
    continuation: bool,
    contact_tick: u64,
    ordinary_contact_seconds: f32,
) -> f32 {
    if continuation {
        contact_tick.saturating_sub(skeleton.locomotion_sample_tick) as f32 / locomotion_sample_hz()
    } else {
        ordinary_contact_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn melee_animation_overlaps_lunge_and_contact_waits_for_arrival() {
        assert_eq!(delayed_melee_contact_seconds(0.3, 0.1), (0.0, 0.3));
        let (animation_delay, contact) = delayed_melee_contact_seconds(0.3, 0.5);
        assert!((animation_delay - 0.2).abs() < 1.0e-6);
        assert!((contact - 0.5).abs() < 1.0e-6);
        assert!((contact - animation_delay - 0.3).abs() < 1.0e-6);

        let state = AttackState::new(
            contact,
            contact,
            0.5,
            false,
            None,
            Some(Vec3::NEG_Z),
            Dir3::NEG_Z,
        );
        assert_eq!(
            state.facing_timer.remaining(),
            state.pre_hit_timer.remaining(),
            "facing remains active from input through delayed contact"
        );
    }

    #[test]
    fn continuation_cooldown_uses_the_replicated_animation_clock() {
        let skeleton = SkeletonState::default().with_locomotion_sample_tick(700);

        assert_eq!(
            melee_contact_delay_from_input(&skeleton, true, 732, 0.3),
            0.5
        );
        assert_eq!(
            melee_contact_delay_from_input(&skeleton, false, 732, 0.3),
            0.3
        );
    }

    #[test]
    fn unreachable_preferred_head_falls_back_within_the_same_enemy() {
        let preferred = Entity::from_bits(1);
        let other = Entity::from_bits(2);
        let choices = [
            BodyStableChoice {
                body: preferred,
                resolved: None,
                ideal_position: Vec3::new(0.0, 1.8, -2.0),
                stable_key: 1,
            },
            BodyStableChoice {
                body: preferred,
                resolved: Some((BodyPart::Chest, Vec3::new(0.0, 1.2, -2.0))),
                ideal_position: Vec3::new(0.0, 1.2, -2.0),
                stable_key: 2,
            },
            BodyStableChoice {
                body: other,
                resolved: Some((BodyPart::Head, Vec3::new(0.05, 1.8, -1.8))),
                ideal_position: Vec3::new(0.05, 1.8, -1.8),
                stable_key: 3,
            },
        ];
        assert_eq!(
            body_stable_auto_aim(
                Vec3::new(0.0, 1.8, 0.0),
                Vec3::NEG_Z,
                Vec3::ZERO,
                10.0,
                &choices
            ),
            Some((BodyPart::Chest, Vec3::new(0.0, 1.2, -2.0)))
        );
    }

    #[test]
    fn fists_are_usable_from_anatomy_while_ranged_attacks_require_weapon_reach() {
        assert!(attack_reach_is_usable(false, true, 0.0, 0.526_801));
        assert!(attack_reach_is_usable(false, true, 0.8, 0.526_801));
        assert!(!attack_reach_is_usable(false, true, 0.0, 0.0));
        assert!(!attack_reach_is_usable(true, false, 0.0, 0.526_801));
        assert!(attack_reach_is_usable(true, false, 20.0, 0.526_801));
    }

    #[test]
    fn gamepad_look_keeps_horizontal_and_reverses_vertical_input() {
        let scale = Vec3::from_array(
            TacticalCombatConfig::default()
                .client_input
                .gamepad_look_scale,
        );
        assert!(scale.x.is_sign_positive());
        assert!(scale.y.is_sign_negative());
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

    #[test]
    fn attack_origin_is_two_thirds_of_body_height_above_collider_base() {
        let collider = Collider::cylinder(0.4, 1.8);
        let transform = Transform::from_xyz(2.0, 5.9, -3.0);
        let dimensions = CharacterDimensions {
            body_height_metres: 1.8,
            ..default()
        };

        assert_eq!(
            attack_origin(&transform, &collider, dimensions),
            Vec3::new(2.0, 6.2, -3.0)
        );
    }

    #[test]
    fn close_fist_ray_hits_collider_surface_before_out_of_range_centerline() {
        let dimensions = CharacterDimensions::default();
        let attacker_collider = Collider::cylinder(0.4, dimensions.body_height_metres);
        let attacker = Transform::from_xyz(0.0, dimensions.body_height_metres * 0.5, 0.0);
        let origin = attack_origin(&attacker, &attacker_collider, dimensions);
        let target_collider = Collider::cylinder(0.4, dimensions.body_height_metres);
        let target_position = Vec3::new(0.9, dimensions.body_height_metres * 0.5, 0.0);

        assert!(origin.distance(target_position) > dimensions.arm_reach_metres);
        let hit = target_collider.cast_ray(
            target_position,
            Rotation::default(),
            origin,
            Vec3::X,
            dimensions.arm_reach_metres,
            true,
        );
        let (surface_distance, _) = hit.expect("50 cm collider surface is within fist reach");
        assert!((surface_distance - 0.5).abs() < 1.0e-5);
    }

    #[test]
    fn attack_target_threshold_scales_with_surface_distance() {
        assert_eq!(attack_target_angular_threshold_degrees(0.0), 180.0);
        assert_eq!(attack_target_angular_threshold_degrees(0.5), 180.0);
        assert_eq!(attack_target_angular_threshold_degrees(1.0), 90.0);
        assert_eq!(attack_target_angular_threshold_degrees(2.0), 45.0);
        assert!((attack_target_angular_threshold_degrees(1_000.0) - 0.09).abs() < 1.0e-6);
    }

    #[test]
    fn attack_target_threshold_accepts_wide_nearby_and_narrow_distant_aim() {
        assert!(attack_target_within_angular_threshold(
            Vec3::NEG_Z,
            Vec3::Z,
            0.0,
        ));
        assert!(attack_target_within_angular_threshold(
            Vec3::NEG_Z,
            Vec3::X,
            1.0,
        ));
        assert!(attack_target_within_angular_threshold(
            Vec3::NEG_Z,
            Vec3::new(1.0, 0.0, -1.0),
            2.0,
        ));
        assert!(!attack_target_within_angular_threshold(
            Vec3::NEG_Z,
            Quat::from_rotation_y(0.1_f32.to_radians()) * Vec3::NEG_Z,
            1_000.0,
        ));
    }

    #[test]
    fn attack_facing_reaches_the_target_at_canonical_contact_without_snapping() {
        let desired = Vec3::X;
        let delta_seconds = 0.05;
        let contact_seconds = 0.5;
        let mut rotation = Quat::IDENTITY;

        for step in 0..10 {
            let remaining = contact_seconds - step as f32 * delta_seconds;
            let speed = body_turn_speed_for_deadline(rotation, desired, remaining, delta_seconds);
            let next = advance_body_facing_toward(rotation, desired, delta_seconds, speed);
            if step == 0 {
                assert!(next.angle_between(rotation) < std::f32::consts::FRAC_PI_2);
            }
            rotation = next;
        }

        assert!((rotation * Vec3::Z).angle_between(desired) < 1.0e-5);
    }
}
