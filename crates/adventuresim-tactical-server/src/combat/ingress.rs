use super::*;

#[derive(Clone, Copy)]
struct EntityMeleeLungeRequest {
    attacker: Entity,
    target: Entity,
    body_part: BodyPart,
    weapon_reach_metres: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct MeleeLungeRequest<'a> {
    pub(crate) attacker_position: Vec3,
    pub(crate) attacker_collider: &'a Collider,
    pub(crate) attacker_dimensions: CharacterDimensions,
    pub(crate) target_transform: &'a Transform,
    pub(crate) target_collider: &'a Collider,
    pub(crate) target_body_part: BodyPart,
    pub(crate) weapon_reach_metres: f32,
    pub(crate) quickstep_distance_metres: f32,
}

pub(super) fn on_defender_response_request(
    event: On<FromClient<DefendRequest>>,
    mut cmd: Commands,
) {
    let Some(defender) = event.client_id.entity() else {
        warn!(
            "Got defender response from an unknown client: {:?}",
            event.client_id
        );
        return;
    };
    cmd.trigger(DefendIntent {
        defender,
        choice: **event,
    });
}

pub(crate) fn apply_defend_intent(
    event: On<DefendIntent>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    states: Query<&TacticalCombatState>,
    mut skeletons: Query<(
        &mut SkeletonState,
        &mut QuickstepPush,
        &CharacterLook,
        Option<&Transform>,
    )>,
    config: Res<TacticalCombatConfig>,
) {
    let Ok(combat_state) = states.get(event.defender) else {
        return;
    };
    if combat_state.is_incapacitated() {
        return;
    }

    let Ok((mut skeleton, mut quickstep_push, look, transform)) = skeletons.get_mut(event.defender)
    else {
        return;
    };
    let start = animation_tick(&time);
    let accepted = match event.choice {
        DefendRequest::Dodge { direction } if DodgeSpec::quickstep(direction).is_none() => false,
        DefendRequest::Dodge { .. } if skeleton.action_kind() == SkeletonAction::Dodge => true,
        DefendRequest::Dodge { direction } => begin_authoritative_quickstep(
            &mut skeleton,
            &mut quickstep_push,
            direction,
            look.to_quat(),
            transform.map_or(Vec3::ZERO, |transform| transform.translation),
            &config,
        ),
        DefendRequest::Roll if !accepts_roll_dodge(&skeleton) => return,
        DefendRequest::Roll => true,
        DefendRequest::Parry => skeleton
            .begin_block(
                BlockSpec::default(),
                start,
                start
                    + duration_ticks(CombatDuration::from_secs_f32(
                        config.presentation.block_seconds,
                    )),
            )
            .is_ok(),
    };
    if !accepted {
        return;
    }

    cmd.entity(event.defender).insert(PendingDefenderResponse {
        choice: event.choice,
        set_at: CombatInstant::from_elapsed(&time),
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each observer resource and query as an independent system parameter"
)]
pub(crate) fn on_melee_attack_started(
    event: On<MeleeAttackStartedIntent>,
    mut commands: Commands,
    mut authorities: Query<&mut MeleeAttackAuthority>,
    mut skeletons: Query<&mut SkeletonState>,
    transforms: Query<&Transform>,
    dimensions: Query<&CharacterDimensions>,
    colliders: Query<&Collider>,
    viewer: TacticalPlayerViewer,
    time: Res<Time<()>>,
    config: Res<TacticalCombatConfig>,
) {
    let Ok(mut skeleton) = skeletons.get_mut(event.attacker) else {
        return;
    };
    let Some(strike_family) = skeleton.available_strike_family(event.strike_family) else {
        return;
    };
    let Some(spec) = (match event.hand {
        AttackHand::Main => skeleton.select_main_attack(strike_family),
        AttackHand::Offhand => skeleton.select_offhand_attack(strike_family),
    }) else {
        return;
    };
    let Ok(mut authority) = authorities.get_mut(event.attacker) else {
        return;
    };
    let (spec, recovery) = viewer
        .get_for_attack(event.attacker, event.hand)
        .map(|view| {
            (
                configure_attack_curve(spec, &view, &config.presentation.attack_curve),
                CombatDuration::from_secs_f32(attack_recovery_secs(
                    &view,
                    event.strike_family.melee_style(),
                    spec.continuation,
                )),
            )
        })
        .unwrap_or((spec, event.windup));
    let start = animation_tick(&time);
    let weapon_reach = viewer
        .get_for_attack(event.attacker, event.hand)
        .map_or(0.0, |view| view.weapon_reach());
    let lunge_delay = event
        .target
        .zip(event.body_part)
        .and_then(|(target, body_part)| {
            planned_melee_lunge_for_entities(
                EntityMeleeLungeRequest {
                    attacker: event.attacker,
                    target,
                    body_part,
                    weapon_reach_metres: weapon_reach,
                },
                &transforms,
                &dimensions,
                &colliders,
                &config,
            )
        })
        .map_or(0.0, |movement| {
            melee_lunge_movement_delay(movement, &config)
        });
    let sequence_start = if spec.continuation {
        skeleton.action_end_tick().unwrap_or(start)
    } else {
        start
    };
    let (animation_start_tick, contact_tick, recovery_tick) =
        delayed_melee_timing_ticks(sequence_start, event.windup, lunge_delay, recovery);
    let contact_windup = CombatDuration::from_secs_f32(
        contact_tick.saturating_sub(start) as f32 / locomotion_sample_hz(),
    );
    if skeleton
        .begin_attack_timed(spec, animation_start_tick, contact_tick, recovery_tick)
        .is_err()
    {
        return;
    }
    info!(attack_key = start, attacker = ?event.attacker, target = ?event.target, body_part = ?event.body_part, strike_family = ?event.strike_family, hand = ?event.hand, "melee_attack_started");
    begin_attack_facing(
        &mut commands,
        event.attacker,
        event.target,
        contact_tick,
        &transforms,
    );
    if let (Some(target), Some(body_part)) = (event.target, event.body_part) {
        begin_melee_lunge(
            &mut commands,
            EntityMeleeLungeRequest {
                attacker: event.attacker,
                target,
                body_part,
                weapon_reach_metres: weapon_reach,
            },
            animation_start_tick,
            &transforms,
            &dimensions,
            &colliders,
            &config,
        );
    } else {
        commands
            .entity(event.attacker)
            .remove::<MeleeLungeMovement>();
        info!(attack_key = start, attacker = ?event.attacker, target = ?event.target, body_part = ?event.body_part, outcome = "untargeted_no_movement", "melee_lunge_planned");
    }
    let now = CombatInstant::from_elapsed(&time);
    authority.observe(
        start,
        event.target,
        event.body_part,
        now,
        contact_windup,
        CombatDuration::from_secs_f32(config.realtime_authority.melee.completion_allowance_seconds),
    );
    commands.entity(event.attacker).insert(PendingMeleeContact {
        attack_key: start,
        target: event.target,
        body_part: event.body_part,
        resolve_at: now + contact_windup,
        reported_precision: event.reported_precision,
        strike_family: event.strike_family,
        hand: event.hand,
    });
}

pub(crate) fn resolve_pending_melee_contacts(
    mut commands: Commands,
    time: Res<Time<()>>,
    pending: Query<(Entity, &PendingMeleeContact)>,
    mut authorities: Query<&mut MeleeAttackAuthority>,
) {
    let now = CombatInstant::from_elapsed(&time);
    for (attacker, contact) in &pending {
        if now < contact.resolve_at {
            continue;
        }
        commands.entity(attacker).remove::<PendingMeleeContact>();
        let (Some(target), Some(body_part)) = (contact.target, contact.body_part) else {
            if let Ok(mut authority) = authorities.get_mut(attacker) {
                authority.complete_miss();
            }
            info!(attack_key = contact.attack_key, attacker = ?attacker, target = ?contact.target, body_part = ?contact.body_part, outcome = "miss", reason = "untargeted", "melee_attack_resolved");
            continue;
        };
        commands.trigger(MeleeAttackIntent {
            attacker,
            target,
            body_part,
            reported_precision: contact.reported_precision,
            strike_family: contact.strike_family,
            hand: contact.hand,
        });
    }
}

fn begin_melee_lunge(
    commands: &mut Commands,
    request: EntityMeleeLungeRequest,
    start_tick: u64,
    transforms: &Query<&Transform>,
    dimensions: &Query<&CharacterDimensions>,
    colliders: &Query<&Collider>,
    config: &TacticalCombatConfig,
) {
    commands
        .entity(request.attacker)
        .remove::<MeleeLungeMovement>();
    let Ok([attacker_transform, target_transform]) =
        transforms.get_many([request.attacker, request.target])
    else {
        return;
    };
    let Ok([attacker_collider, target_collider]) =
        colliders.get_many([request.attacker, request.target])
    else {
        return;
    };
    let dimensions = dimensions
        .get(request.attacker)
        .copied()
        .unwrap_or_default();
    let leg_length = dimensions.leg_length_metres;
    let quickstep_distance =
        quickstep_target_displacement_metres(leg_length, &config.movement.motor);
    let reach = melee_interaction_range(dimensions.arm_reach_metres, request.weapon_reach_metres);
    let maximum_travel = quickstep_distance.min(melee_collision_clearance(
        attacker_transform.translation,
        attacker_collider,
        target_transform,
        target_collider,
    ));
    let Some(movement) = planned_melee_lunge(
        MeleeLungeRequest {
            attacker_position: attacker_transform.translation,
            attacker_collider,
            attacker_dimensions: dimensions,
            target_transform,
            target_collider,
            target_body_part: request.body_part,
            weapon_reach_metres: request.weapon_reach_metres,
            quickstep_distance_metres: quickstep_distance,
        },
        config,
    ) else {
        let direction = (target_transform.translation - attacker_transform.translation)
            .xz()
            .normalize_or_zero();
        let closure = configured_body_part_strike_point(
            melee_attack_origin(
                attacker_transform.translation,
                attacker_collider,
                dimensions,
            ),
            direction,
            target_transform,
            request.body_part,
            reach,
            maximum_travel,
            config,
        )
        .map(|(_, closure)| closure);
        let outcome =
            if closure.is_some_and(|value| value <= melee_lunge_range_window_metres() + 1.0e-5) {
                "already_in_window"
            } else {
                "unreachable_no_movement"
            };
        info!(attack_key = start_tick, attacker = ?request.attacker, target = ?request.target, body_part = ?request.body_part, outcome, reach_metres = reach, closure_metres = closure.unwrap_or(f32::INFINITY), maximum_travel_metres = maximum_travel, "melee_lunge_planned");
        return;
    };
    info!(attack_key = start_tick, attacker = ?request.attacker, target = ?request.target, body_part = ?request.body_part, outcome = if movement.quickstep { "quickstep" } else { "forward" }, reach_metres = reach, planned_distance_metres = movement.distance_metres, maximum_travel_metres = maximum_travel, "melee_lunge_planned");
    commands.entity(request.attacker).insert(movement);
    if movement.quickstep {
        commands.entity(request.attacker).insert(QuickstepPush {
            start_tick,
            direction: Vec2::new(movement.direction.x, -movement.direction.y),
            orientation: Quat::IDENTITY,
            origin: movement.origin,
            active: true,
        });
    }
}

fn planned_melee_lunge_for_entities(
    request: EntityMeleeLungeRequest,
    transforms: &Query<&Transform>,
    dimensions: &Query<&CharacterDimensions>,
    colliders: &Query<&Collider>,
    config: &TacticalCombatConfig,
) -> Option<MeleeLungeMovement> {
    let [attacker_transform, target_transform] = transforms
        .get_many([request.attacker, request.target])
        .ok()?;
    let [attacker_collider, target_collider] = colliders
        .get_many([request.attacker, request.target])
        .ok()?;
    let dimensions = dimensions
        .get(request.attacker)
        .copied()
        .unwrap_or_default();
    planned_melee_lunge(
        MeleeLungeRequest {
            attacker_position: attacker_transform.translation,
            attacker_collider,
            attacker_dimensions: dimensions,
            target_transform,
            target_collider,
            target_body_part: request.body_part,
            weapon_reach_metres: request.weapon_reach_metres,
            quickstep_distance_metres: quickstep_target_displacement_metres(
                dimensions.leg_length_metres,
                &config.movement.motor,
            ),
        },
        config,
    )
}

fn melee_lunge_movement_delay(movement: MeleeLungeMovement, config: &TacticalCombatConfig) -> f32 {
    let lunge = if movement.quickstep {
        MeleeLunge::Quickstep {
            distance_metres: movement.distance_metres,
        }
    } else {
        MeleeLunge::Forward {
            distance_metres: movement.distance_metres,
        }
    };
    melee_lunge_delay_seconds(
        lunge,
        config.movement.speeds_metres_per_second.run,
        conservative_forward_lunge_acceleration(&config.movement.motor),
        config.movement.maneuvers.quickstep_duration_seconds,
    )
}

fn planned_melee_lunge(
    request: MeleeLungeRequest<'_>,
    config: &TacticalCombatConfig,
) -> Option<MeleeLungeMovement> {
    let direction = (request.target_transform.translation - request.attacker_position)
        .xz()
        .normalize_or_zero();
    if direction == Vec2::ZERO {
        return None;
    }
    let arm_reach = request.attacker_dimensions.arm_reach_metres;
    let origin = melee_attack_origin(
        request.attacker_position,
        request.attacker_collider,
        request.attacker_dimensions,
    );
    let maximum_travel = request
        .quickstep_distance_metres
        .min(melee_collision_clearance(
            request.attacker_position,
            request.attacker_collider,
            request.target_transform,
            request.target_collider,
        ));
    let (strike_point, closure) = configured_body_part_strike_point(
        origin,
        direction,
        request.target_transform,
        request.target_body_part,
        melee_interaction_range(arm_reach, request.weapon_reach_metres),
        maximum_travel,
        config,
    )?;
    let _ = strike_point;
    let (distance_metres, quickstep) = match melee_lunge(
        melee_interaction_range(arm_reach, request.weapon_reach_metres) + closure,
        arm_reach,
        request.weapon_reach_metres,
        maximum_travel,
    ) {
        MeleeLunge::None => return None,
        MeleeLunge::Forward { distance_metres } => (distance_metres, false),
        MeleeLunge::Quickstep { distance_metres } => (distance_metres, true),
    };
    Some(MeleeLungeMovement {
        origin: request.attacker_position,
        direction,
        distance_metres,
        quickstep,
    })
}

pub(crate) fn melee_attack_origin(
    attacker_position: Vec3,
    attacker_collider: &Collider,
    dimensions: CharacterDimensions,
) -> Vec3 {
    let base = attacker_collider
        .aabb(attacker_position, Rotation::default())
        .min
        .y;
    attacker_position.with_y(base + dimensions.body_height_metres * (2.0 / 3.0))
}

pub(crate) fn melee_collision_clearance(
    attacker_position: Vec3,
    attacker_collider: &Collider,
    target_transform: &Transform,
    target_collider: &Collider,
) -> f32 {
    let a = attacker_collider.aabb(attacker_position, Rotation::default());
    let b = target_collider.aabb(
        target_transform.translation,
        Rotation(target_transform.rotation),
    );
    let ar = ((a.max - a.min) * 0.5).xz().max_element();
    let br = ((b.max - b.min) * 0.5).xz().max_element();
    (attacker_position
        .xz()
        .distance(target_transform.translation.xz())
        - ar
        - br)
        .max(0.0)
}

pub(crate) fn configured_body_part_strike_point(
    origin: Vec3,
    direction: Vec2,
    target_transform: &Transform,
    body_part: BodyPart,
    reach: f32,
    maximum_travel: f32,
    config: &TacticalCombatConfig,
) -> Option<(Vec3, f32)> {
    let hitbox = config
        .targeting
        .body_part_hitboxes
        .iter()
        .find(|hitbox| hitbox.body_part == body_part)?;
    let half = Vec3::from_array(hitbox.half_extents_metres);
    let collider = Collider::cuboid(half.x * 2.0, half.y * 2.0, half.z * 2.0);
    let translation = target_transform.translation
        + target_transform.rotation * Vec3::from_array(hitbox.center_metres);
    reachable_melee_strike_point(
        &collider,
        translation,
        target_transform.rotation,
        origin,
        direction,
        reach,
        maximum_travel,
    )
}

pub(crate) fn configured_body_part_surface_distance(
    origin: Vec3,
    target_transform: &Transform,
    body_part: BodyPart,
    config: &TacticalCombatConfig,
) -> Option<f32> {
    let hitbox = config
        .targeting
        .body_part_hitboxes
        .iter()
        .find(|h| h.body_part == body_part)?;
    let half = Vec3::from_array(hitbox.half_extents_metres);
    let collider = Collider::cuboid(half.x * 2.0, half.y * 2.0, half.z * 2.0);
    let translation = target_transform.translation
        + target_transform.rotation * Vec3::from_array(hitbox.center_metres);
    Some(collider.distance_to_point(
        translation,
        Rotation(target_transform.rotation),
        origin,
        true,
    ))
}

pub(crate) fn melee_body_part_reachable(
    request: MeleeLungeRequest<'_>,
    config: &TacticalCombatConfig,
) -> bool {
    let direction = (request.target_transform.translation - request.attacker_position)
        .xz()
        .normalize_or_zero();
    let maximum_travel = request
        .quickstep_distance_metres
        .min(melee_collision_clearance(
            request.attacker_position,
            request.attacker_collider,
            request.target_transform,
            request.target_collider,
        ));
    configured_body_part_strike_point(
        melee_attack_origin(
            request.attacker_position,
            request.attacker_collider,
            request.attacker_dimensions,
        ),
        direction,
        request.target_transform,
        request.target_body_part,
        melee_interaction_range(
            request.attacker_dimensions.arm_reach_metres,
            request.weapon_reach_metres,
        ),
        maximum_travel,
        config,
    )
    .is_some()
}

pub(crate) fn melee_body_part_lunge_delay(
    request: MeleeLungeRequest<'_>,
    config: &TacticalCombatConfig,
) -> Option<f32> {
    if !melee_body_part_reachable(request, config) {
        return None;
    }
    Some(
        planned_melee_lunge(request, config)
            .map_or(0.0, |movement| melee_lunge_movement_delay(movement, config)),
    )
}

pub(super) fn resolve_defender_response(
    pending: Option<&PendingDefenderResponse>,
    time: &Time<()>,
    defender_view: &TacticalPlayerView,
    config: &DefenseAuthorityConfig,
) -> DefenderResponse {
    let Some(pending) = pending else {
        return DefenderResponse::None;
    };

    let elapsed = CombatInstant::from_elapsed(time).elapsed_since(pending.set_at);
    let reflex_window = std::time::Duration::from_secs_f32(config.reflex_window_seconds);
    if elapsed > reflex_window {
        return DefenderResponse::None;
    }

    let input_reflex = (1.0 - elapsed.as_secs_f32() / reflex_window.as_secs_f32()).clamp(0.0, 1.0);

    match pending.choice {
        DefendRequest::Dodge { .. } => DefenderResponse::Dodge { input_reflex },
        DefendRequest::Roll => DefenderResponse::Dodge {
            input_reflex: roll_dodge_reflex(input_reflex, config.roll_dodge_effectiveness),
        },
        DefendRequest::Parry => {
            if defender_view.shield_block_bonus() > 0.0 {
                DefenderResponse::Parry { input_reflex }
            } else {
                DefenderResponse::None
            }
        }
    }
}

fn roll_dodge_reflex(input_reflex: f32, effectiveness: f32) -> f32 {
    input_reflex.clamp(0.0, 1.0) * effectiveness
}

fn accepts_roll_dodge(skeleton: &SkeletonState) -> bool {
    skeleton.body().is_downed()
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "roll behavior tests stay next to the roll policy they specify"
)]
mod roll_tests {
    use super::*;
    use std::time::Duration;

    #[derive(Resource, Default)]
    struct ScheduledContactResults(Vec<Result<(), MeleeIntentRejection>>);

    fn record_scheduled_contact_geometry(
        event: On<MeleeAttackIntent>,
        transforms: Query<&Transform>,
        colliders: Query<&Collider>,
        dimensions: Query<&CharacterDimensions>,
        config: Res<TacticalCombatConfig>,
        mut results: ResMut<ScheduledContactResults>,
    ) {
        let result = (|| {
            let attacker_transform = transforms
                .get(event.attacker)
                .map_err(|_| MeleeIntentRejection::OutOfRange)?;
            let target_transform = transforms
                .get(event.target)
                .map_err(|_| MeleeIntentRejection::OutOfRange)?;
            let attacker_collider = colliders
                .get(event.attacker)
                .map_err(|_| MeleeIntentRejection::OutOfRange)?;
            let dimensions = dimensions
                .get(event.attacker)
                .map_err(|_| MeleeIntentRejection::OutOfRange)?;
            let surface_distance = configured_body_part_surface_distance(
                melee_attack_origin(
                    attacker_transform.translation,
                    attacker_collider,
                    *dimensions,
                ),
                target_transform,
                event.body_part,
                &config,
            )
            .unwrap_or(f32::INFINITY);
            validate_melee_intent_cheap(MeleeIntentFacts {
                attacker: event.attacker,
                target: event.target,
                attacker_side: Some(TacticalCombatSide::Party),
                target_side: Some(TacticalCombatSide::Enemy),
                attacker_incapacitated: Some(false),
                target_incapacitated: Some(false),
                reported_precision: event.reported_precision,
                arm_reach: dimensions.arm_reach_metres,
                weapon_reach: 0.0,
                range_latency_tolerance: 0.0,
                separation: surface_distance,
                authority_permits: true,
                body_part: event.body_part,
                attacker_position: attacker_transform.translation,
                target_position: target_transform.translation,
                attacker_yaw: 0.0,
                target_yaw: 0.0,
            })
            .map(|_| ())
        })();
        results.0.push(result);
    }

    fn scheduled_contact_fixture() -> (App, Entity, Entity, Vec3) {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<TacticalCombatConfig>()
            .init_resource::<ScheduledContactResults>()
            .add_observer(record_scheduled_contact_geometry)
            .add_systems(Update, resolve_pending_melee_contacts);
        let collider = Collider::cylinder(0.4, 1.9);
        let dimensions = CharacterDimensions::default();
        let config = app.world().resource::<TacticalCombatConfig>().clone();
        let target_transform = Transform::from_xyz(1.25, 0.0, 0.0);
        let movement = planned_melee_lunge(
            MeleeLungeRequest {
                attacker_position: Vec3::ZERO,
                attacker_collider: &collider,
                attacker_dimensions: dimensions,
                target_transform: &target_transform,
                target_collider: &collider,
                target_body_part: BodyPart::Chest,
                weapon_reach_metres: 0.0,
                quickstep_distance_metres: 1.0,
            },
            &config,
        )
        .expect("fist attack should plan a stationary lunge");
        let arrived = Vec3::new(
            movement.direction.x * movement.distance_metres,
            0.0,
            movement.direction.y * movement.distance_metres,
        );
        let target = app
            .world_mut()
            .spawn((target_transform, collider.clone()))
            .id();
        let resolve_at =
            CombatInstant::default() + CombatDuration::from_duration(Duration::from_millis(100));
        let attacker = app
            .world_mut()
            .spawn((
                Transform::from_translation(arrived),
                collider,
                dimensions,
                MeleeAttackAuthority::default(),
                PendingMeleeContact {
                    attack_key: 42,
                    target: Some(target),
                    body_part: Some(BodyPart::Chest),
                    resolve_at,
                    reported_precision: ReportedPrecision::new(1.0).unwrap(),
                    strike_family: StrikeFamily::Thrust,
                    hand: AttackHand::Main,
                },
            ))
            .id();
        (app, attacker, target, arrived)
    }

    #[test]
    fn stationary_lunge_resolves_from_server_schedule_without_client_completion() {
        let (mut app, attacker, _, _) = scheduled_contact_fixture();
        app.update();
        assert!(
            app.world()
                .resource::<ScheduledContactResults>()
                .0
                .is_empty()
        );
        assert!(app.world().get::<PendingMeleeContact>(attacker).is_some());

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(100));
        app.update();

        assert_eq!(
            app.world().resource::<ScheduledContactResults>().0,
            [Ok(())]
        );
        assert!(app.world().get::<PendingMeleeContact>(attacker).is_none());
    }

    #[test]
    fn defender_moving_out_of_range_before_server_contact_misses() {
        let (mut app, _, target, _) = scheduled_contact_fixture();
        app.world_mut()
            .get_mut::<Transform>(target)
            .unwrap()
            .translation
            .x += 2.0;
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(100));
        app.update();

        assert_eq!(
            app.world().resource::<ScheduledContactResults>().0,
            [Err(MeleeIntentRejection::OutOfRange)]
        );
    }

    #[test]
    fn roll_is_a_bounded_fraction_of_an_ordinary_dodge() {
        assert!((roll_dodge_reflex(1.0, 0.35) - 0.35).abs() < f32::EPSILON);
        assert_eq!(roll_dodge_reflex(-1.0, 0.35), 0.0);
        assert_eq!(roll_dodge_reflex(2.0, 0.35), 0.35);
    }

    #[test]
    fn roll_defense_is_restricted_to_prone_and_supine() {
        assert!(!accepts_roll_dodge(&SkeletonState::default()));
        assert!(accepts_roll_dodge(
            &SkeletonState::default().with_body_state(BodyState::Prone)
        ));
        assert!(accepts_roll_dodge(
            &SkeletonState::default().with_body_state(BodyState::Supine)
        ));
    }

    #[test]
    fn lunge_delays_contact_without_stretching_authored_attack_animation() {
        let authored = CombatDuration::from_duration(Duration::from_millis(300));
        let recovery = CombatDuration::from_duration(Duration::from_millis(200));
        let (animation_start, contact, recovery_end) =
            delayed_melee_timing_ticks(10, authored, 0.5, recovery);
        assert_eq!(animation_start, 23);
        assert_eq!(contact, 42);
        assert_eq!(contact - animation_start, duration_ticks(authored));
        assert_eq!(recovery_end - contact, duration_ticks(recovery));

        let (animation_start, contact, _) = delayed_melee_timing_ticks(10, authored, 0.0, recovery);
        assert_eq!(animation_start, 10);
        assert_eq!(contact, 10 + duration_ticks(authored));
    }

    #[test]
    fn surface_gap_plans_forward_quickstep_and_unreachable_attack_movement() {
        let collider = Collider::cylinder(0.4, 1.9);
        let dimensions = CharacterDimensions::default();
        let config = TacticalCombatConfig::default();
        let plan = |target_x| {
            planned_melee_lunge(
                MeleeLungeRequest {
                    attacker_position: Vec3::ZERO,
                    attacker_collider: &collider,
                    attacker_dimensions: dimensions,
                    target_transform: &Transform::from_xyz(target_x, 0.0, 0.0),
                    target_collider: &collider,
                    target_body_part: BodyPart::Chest,
                    weapon_reach_metres: 0.8,
                    quickstep_distance_metres: 1.0,
                },
                &config,
            )
        };

        assert!(plan(1.8).is_some(), "reachable chest should plan movement");
        assert!(
            plan(3.0).is_none(),
            "collision-limited unreachable target must not lunge"
        );

        let fist = planned_melee_lunge(
            MeleeLungeRequest {
                attacker_position: Vec3::ZERO,
                attacker_collider: &collider,
                attacker_dimensions: CharacterDimensions {
                    arm_reach_metres: 0.55,
                    ..dimensions
                },
                target_transform: &Transform::from_xyz(1.25, 0.0, 0.0),
                target_collider: &collider,
                target_body_part: BodyPart::Chest,
                weapon_reach_metres: 0.0,
                quickstep_distance_metres: 1.0,
            },
            &config,
        )
        .expect("fist attack should use its 55 cm anatomical reach");
        assert!(fist.distance_metres > 0.0);
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ExpectedLungeMode {
        None,
        Forward,
        Quickstep,
    }

    fn fixed_tick_lunge_displacement_at_contact(
        movement: MeleeLungeMovement,
        config: &TacticalCombatConfig,
    ) -> f32 {
        let dt = 1.0 / locomotion_sample_hz();
        let authored_windup_seconds = 0.18_f32;
        let contact_seconds =
            authored_windup_seconds.max(melee_lunge_movement_delay(movement, config));
        let contact_ticks = (contact_seconds * locomotion_sample_hz()).ceil() as u64;
        let motor = &config.movement.motor;
        let mass = motor.fallback_character_mass_kg;
        let mut displacement = 0.0;
        let mut velocity = 0.0;

        if movement.quickstep {
            let action_seconds = config.movement.maneuvers.quickstep_duration_seconds;
            let action_ticks = (action_seconds * locomotion_sample_hz()).round().max(1.0) as u64;
            let maximum_force = quickstep_peak_horizontal_force_newtons(70.0, 3.0, motor);
            for tick in 0..contact_ticks {
                if displacement >= movement.distance_metres {
                    break;
                }
                let target = adventuresim_tactical_core::physics::quickstep_motion_target(
                    (tick + 1) as f32 / action_ticks as f32,
                    movement.distance_metres,
                    action_seconds,
                    motor.quickstep_authored_displacement_profile,
                );
                let force = adventuresim_tactical_core::physics::quickstep_tracking_force_newtons(
                    displacement,
                    velocity,
                    target,
                    mass,
                    maximum_force,
                    dt,
                );
                velocity += force / mass * dt;
                displacement += velocity * dt;
            }
        } else {
            let drive_acceleration = (motor.reference_ground_drive_force_newtons / mass)
                .min(motor.gravity_metres_per_second_squared * motor.traction_coefficient);
            let run_speed = config.movement.speeds_metres_per_second.run;
            for _ in 0..contact_ticks {
                if displacement >= movement.distance_metres {
                    break;
                }
                velocity = (velocity + drive_acceleration * dt).min(run_speed);
                displacement += velocity * dt;
            }
        }

        displacement
    }

    #[test]
    fn stationary_defender_melee_range_matrix_reaches_client_ray_and_authority() {
        let collider = Collider::cylinder(0.4, 1.9);
        let dimensions = CharacterDimensions::default();
        let config = TacticalCombatConfig::default();
        let quickstep_distance = quickstep_target_displacement_metres(
            dimensions.leg_length_metres,
            &config.movement.motor,
        );
        let cases = [
            (
                "in-range",
                0.0,
                0.8,
                BodyPart::Chest,
                ExpectedLungeMode::None,
                true,
                true,
            ),
            (
                "window-outside",
                0.099,
                0.8,
                BodyPart::Chest,
                ExpectedLungeMode::None,
                false,
                true,
            ),
            (
                "over-window",
                0.101,
                0.8,
                BodyPart::Chest,
                ExpectedLungeMode::Forward,
                true,
                true,
            ),
            (
                "under-mode",
                0.499,
                0.8,
                BodyPart::Chest,
                ExpectedLungeMode::Forward,
                true,
                true,
            ),
            (
                "over-mode",
                0.501,
                0.8,
                BodyPart::Chest,
                ExpectedLungeMode::Quickstep,
                true,
                true,
            ),
            (
                "quickstep-max",
                quickstep_distance - 0.01,
                0.8,
                BodyPart::Chest,
                ExpectedLungeMode::Quickstep,
                true,
                true,
            ),
            (
                "quickstep-over",
                quickstep_distance + 0.01,
                0.8,
                BodyPart::Chest,
                ExpectedLungeMode::None,
                false,
                false,
            ),
            (
                "fist-close",
                0.0,
                0.0,
                BodyPart::Chest,
                ExpectedLungeMode::None,
                true,
                true,
            ),
            (
                "fist-forward",
                0.30,
                0.0,
                BodyPart::Chest,
                ExpectedLungeMode::Forward,
                true,
                true,
            ),
            (
                "fist-quickstep",
                0.70,
                0.0,
                BodyPart::Chest,
                ExpectedLungeMode::Quickstep,
                true,
                true,
            ),
            (
                "head-forward",
                0.30,
                0.8,
                BodyPart::Head,
                ExpectedLungeMode::Forward,
                true,
                true,
            ),
            (
                "head-quickstep",
                0.70,
                0.8,
                BodyPart::Head,
                ExpectedLungeMode::Quickstep,
                true,
                true,
            ),
            (
                "left-arm",
                0.30,
                0.8,
                BodyPart::LeftArm,
                ExpectedLungeMode::Forward,
                true,
                true,
            ),
        ];

        for (
            label,
            desired_gap,
            weapon_reach,
            body_part,
            expected_mode,
            expected_client_hit,
            expected_server_acceptance,
        ) in cases
        {
            let reach = melee_interaction_range(dimensions.arm_reach_metres, weapon_reach);
            let attacker_origin = melee_attack_origin(Vec3::ZERO, &collider, dimensions);
            let mut low = 0.0;
            let mut high = 5.0;
            for _ in 0..40 {
                let mid = (low + high) * 0.5;
                let target = Transform::from_xyz(mid, 0.0, 0.0);
                let gap = configured_body_part_surface_distance(
                    attacker_origin,
                    &target,
                    body_part,
                    &config,
                )
                .unwrap()
                    - reach;
                if gap < desired_gap {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            let target = Transform::from_xyz((low + high) * 0.5, 0.0, 0.0);
            let maximum_travel = quickstep_distance.min(melee_collision_clearance(
                Vec3::ZERO,
                &collider,
                &target,
                &collider,
            ));
            let strike_point = configured_body_part_strike_point(
                attacker_origin,
                Vec2::X,
                &target,
                body_part,
                reach,
                maximum_travel,
                &config,
            )
            .map(|(point, _)| point);
            let movement = planned_melee_lunge(
                MeleeLungeRequest {
                    attacker_position: Vec3::ZERO,
                    attacker_collider: &collider,
                    attacker_dimensions: dimensions,
                    target_transform: &target,
                    target_collider: &collider,
                    target_body_part: body_part,
                    weapon_reach_metres: weapon_reach,
                    quickstep_distance_metres: quickstep_distance,
                },
                &config,
            );
            let actual_mode = match movement {
                None => ExpectedLungeMode::None,
                Some(movement) if movement.quickstep => ExpectedLungeMode::Quickstep,
                Some(_) => ExpectedLungeMode::Forward,
            };
            assert_eq!(actual_mode, expected_mode, "{label}: wrong lunge mode");

            let actual_displacement = movement
                .map(|movement| fixed_tick_lunge_displacement_at_contact(movement, &config))
                .unwrap_or(0.0);
            let arrived_position = movement.map_or(Vec3::ZERO, |movement| {
                Vec3::new(
                    movement.direction.x * actual_displacement,
                    0.0,
                    movement.direction.y * actual_displacement,
                )
            });
            let arrived_origin = melee_attack_origin(arrived_position, &collider, dimensions);
            let surface_distance =
                configured_body_part_surface_distance(arrived_origin, &target, body_part, &config)
                    .unwrap();
            let server_accepts = surface_distance
                <= reach
                    + config
                        .realtime_authority
                        .melee
                        .range_latency_tolerance_metres;
            let client_ray_hits = strike_point.is_some_and(|strike_point| {
                arrived_origin.distance(strike_point) <= reach + 1.0e-4
            });
            println!(
                "{label:>14} gap={desired_gap:.3} mode={actual_mode:?} planned={:.3} actual={actual_displacement:.3} ray={client_ray_hits} server={server_accepts}",
                movement.map_or(0.0, |movement| movement.distance_metres),
            );
            assert_eq!(
                client_ray_hits, expected_client_hit,
                "{label}: client ray mismatch at fixed-tick contact (surface={surface_distance:.4}, reach={reach:.4}, actual travel={actual_displacement:.4})"
            );
            assert_eq!(
                server_accepts, expected_server_acceptance,
                "{label}: server validation mismatch at fixed-tick contact (surface={surface_distance:.4}, reach={reach:.4}, actual travel={actual_displacement:.4})"
            );
        }
    }

    #[test]
    fn clients_and_server_ai_share_authoritative_defense_transitions() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<TacticalCombatConfig>()
            .add_observer(on_defender_response_request)
            .add_observer(apply_defend_intent);
        let player = app
            .world_mut()
            .spawn((
                TacticalCombatState::default(),
                SkeletonState::default(),
                QuickstepPush::default(),
                CharacterLook::default(),
            ))
            .id();
        let bot = app
            .world_mut()
            .spawn((
                TacticalCombatState::default(),
                SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised),
                QuickstepPush::default(),
                CharacterLook::default(),
            ))
            .id();

        app.world_mut().trigger(FromClient {
            client_id: adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId::Client(
                player,
            ),
            message: DefendRequest::Parry,
        });
        app.world_mut().trigger(DefendIntent {
            defender: bot,
            choice: DefendRequest::Dodge { direction: Vec2::X },
        });
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .get::<SkeletonState>(player)
                .unwrap()
                .action_kind(),
            SkeletonAction::Block
        );
        assert_eq!(
            app.world().get::<SkeletonState>(bot).unwrap().action_kind(),
            SkeletonAction::Dodge
        );
        assert_eq!(
            app.world()
                .get::<SkeletonState>(bot)
                .unwrap()
                .action_direction(),
            Vec2::X
        );
        assert!(app.world().get::<QuickstepPush>(bot).unwrap().active);
        assert!(matches!(
            app.world().get::<PendingDefenderResponse>(player),
            Some(PendingDefenderResponse {
                choice: DefendRequest::Parry,
                ..
            })
        ));
        assert!(matches!(
            app.world().get::<PendingDefenderResponse>(bot),
            Some(PendingDefenderResponse {
                choice: DefendRequest::Dodge { direction: Vec2::X },
                ..
            })
        ));
    }

    #[test]
    fn stationary_dodge_request_is_rejected() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<TacticalCombatConfig>()
            .add_observer(apply_defend_intent);
        let defender = app
            .world_mut()
            .spawn((
                TacticalCombatState::default(),
                SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised),
                QuickstepPush::default(),
                CharacterLook::default(),
            ))
            .id();

        app.world_mut().trigger(DefendIntent {
            defender,
            choice: DefendRequest::Dodge {
                direction: Vec2::ZERO,
            },
        });
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .get::<SkeletonState>(defender)
                .unwrap()
                .action_kind(),
            SkeletonAction::None
        );
        assert!(!app.world().get::<QuickstepPush>(defender).unwrap().active);
        assert!(
            app.world()
                .get::<PendingDefenderResponse>(defender)
                .is_none()
        );
    }

    #[test]
    fn directional_dodge_request_without_raised_guard_is_rejected() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<TacticalCombatConfig>()
            .add_observer(apply_defend_intent);
        let defender = app
            .world_mut()
            .spawn((
                TacticalCombatState::default(),
                SkeletonState::default(),
                QuickstepPush::default(),
                CharacterLook::default(),
            ))
            .id();

        app.world_mut().trigger(DefendIntent {
            defender,
            choice: DefendRequest::Dodge { direction: Vec2::X },
        });
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .get::<SkeletonState>(defender)
                .unwrap()
                .action_kind(),
            SkeletonAction::None
        );
        assert!(!app.world().get::<QuickstepPush>(defender).unwrap().active);
        assert!(
            app.world()
                .get::<PendingDefenderResponse>(defender)
                .is_none()
        );
    }
}

fn player_attack_windups(
    authored: CombatDuration,
    config: &MeleeAuthorityConfig,
) -> (CombatDuration, CombatDuration) {
    let tolerance = CombatDuration::from_secs_f32(
        (authored.as_secs_f32() * config.windup_jitter_fraction)
            .min(config.maximum_windup_jitter_seconds),
    );
    (authored, authored.saturating_sub(tolerance))
}

pub(super) fn on_melee_action_request(
    event: On<FromClient<MeleeActionRequest>>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    config: Res<TacticalCombatConfig>,
) {
    let Some(attacker) = event.client_id.entity() else {
        debug!(
            "Ignoring melee action from unknown client: {:?}",
            event.client_id
        );
        return;
    };
    let strike_family = event.strike_family;
    let hand = event.hand;
    let authored_windup = viewer
        .get_for_attack(attacker, hand)
        .map(|view| {
            CombatDuration::from_secs_f32(attack_preparation_secs(
                &view,
                strike_family.melee_style(),
            ))
        })
        .unwrap_or_default();
    let reported_precision = ReportedPrecision::new(config.targeting.reported_hit_precision)
        .expect("configured melee precision is finite");
    let (target, body_part) = match (event.target, event.body_part) {
        (Some(target), Some(body_part)) => (Some(target), Some(body_part)),
        _ => (None, None),
    };
    cmd.trigger(MeleeAttackStartedIntent {
        attacker,
        target,
        body_part,
        windup: authored_windup,
        reported_precision,
        strike_family,
        hand,
    });
}

pub(super) fn on_ranged_action_request(
    event: On<FromClient<RangedActionRequest>>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    config: Res<TacticalCombatConfig>,
) {
    let Some(attacker) = event.client_id.entity() else {
        debug!(
            "Ignoring ranged action from unknown client: {:?}",
            event.client_id
        );
        return;
    };
    match **event {
        RangedActionRequest::Start { target } => {
            // Same per-weapon windup + jitter-tolerance treatment as the
            // melee path - see `on_melee_action_request`.
            let authored_windup = viewer
                .get(attacker)
                .map(|view| CombatDuration::from_secs_f32(view.weapon_ranged_windup_secs()))
                .unwrap_or_default();
            let (animation_windup, minimum_windup) =
                player_attack_windups(authored_windup, &config.realtime_authority.melee);
            cmd.trigger(RangedAttackStartedIntent {
                attacker,
                target,
                animation_windup,
                minimum_windup,
            });
        }
        RangedActionRequest::CompleteMiss => {
            cmd.trigger(RangedAttackIntent {
                attacker,
                target: None,
                body_part: BodyPart::Chest,
                reported_precision: ReportedPrecision::new(0.0).expect("zero is finite"),
            });
        }
        RangedActionRequest::CompleteHit {
            target,
            body_part,
            reported_precision,
        } => {
            let Some(reported_precision) = ReportedPrecision::new(reported_precision) else {
                debug!("Ignoring non-finite ranged precision from {attacker:?}");
                return;
            };
            // Finite precision is deliberately trusted. Animation and
            // secondary physics remain client-owned and non-deterministic.
            cmd.trigger(RangedAttackIntent {
                attacker,
                target: Some(target),
                body_part,
                reported_precision,
            });
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each observer resource and query as an independent system parameter"
)]
pub(super) fn on_ranged_attack_started(
    event: On<RangedAttackStartedIntent>,
    mut commands: Commands,
    mut authorities: Query<&mut RangedAttackAuthority>,
    mut skeletons: Query<&mut SkeletonState>,
    transforms: Query<&Transform>,
    viewer: TacticalPlayerViewer,
    time: Res<Time<()>>,
    config: Res<TacticalCombatConfig>,
) {
    let Ok(mut authority) = authorities.get_mut(event.attacker) else {
        return;
    };
    let Ok(mut skeleton) = skeletons.get_mut(event.attacker) else {
        return;
    };
    let start = animation_tick(&time);
    let (spec, recovery) = viewer
        .get(event.attacker)
        .map(|view| {
            (
                configure_attack_curve(
                    AttackSpec::default(),
                    &view,
                    &config.presentation.attack_curve,
                ),
                CombatDuration::from_secs_f32(attack_recovery_secs(
                    &view,
                    view.weapon_preferred_melee_style(),
                    false,
                )),
            )
        })
        .unwrap_or((AttackSpec::default(), event.animation_windup));
    if skeleton
        .begin_attack_timed(
            spec,
            start,
            start + duration_ticks(event.animation_windup),
            start
                .saturating_add(duration_ticks(event.animation_windup))
                .saturating_add(duration_ticks(recovery)),
        )
        .is_err()
    {
        return;
    }
    begin_attack_facing(
        &mut commands,
        event.attacker,
        event.target,
        start + duration_ticks(event.animation_windup),
        &transforms,
    );
    authority.observe(
        CombatInstant::from_elapsed(&time),
        event.minimum_windup,
        CombatDuration::from_secs_f32(
            config
                .realtime_authority
                .ranged
                .completion_allowance_seconds,
        ),
    );
}

fn animation_tick(time: &Time<()>) -> u64 {
    (time.elapsed_secs_f64() * locomotion_sample_hz() as f64).round() as u64
}

fn delayed_melee_timing_ticks(
    input_tick: u64,
    authored_windup: CombatDuration,
    lunge_delay_seconds: f32,
    recovery: CombatDuration,
) -> (u64, u64, u64) {
    let authored_ticks = duration_ticks(authored_windup);
    let arrival_ticks = (lunge_delay_seconds.max(0.0) * locomotion_sample_hz()).round() as u64;
    let contact = input_tick.saturating_add(authored_ticks.max(arrival_ticks));
    let animation_start = contact.saturating_sub(authored_ticks);
    let recovery_end = contact.saturating_add(duration_ticks(recovery));
    (animation_start, contact, recovery_end)
}

fn duration_ticks(duration: CombatDuration) -> u64 {
    (duration.as_secs_f32() * locomotion_sample_hz())
        .round()
        .max(1.0) as u64
}

pub(super) fn authoritative_line_of_sight(
    spatial: &SpatialQuery,
    scene_items: &Query<Entity, With<TacticalSceneItem>>,
    attacker: Entity,
    target: Entity,
    origin: Vec3,
    target_position: Vec3,
) -> bool {
    let offset = target_position - origin;
    let distance = offset.length();
    let Ok(direction) = Dir3::new(offset) else {
        return false;
    };
    let excluded: Vec<_> = scene_items.iter().chain([attacker]).collect();
    let filter = SpatialQueryFilter::from_excluded_entities(excluded);
    spatial
        .cast_ray(origin, direction, distance, true, &filter)
        .is_some_and(|hit| hit.entity == target)
}
