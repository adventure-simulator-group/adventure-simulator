use super::*;

mod attack_start;
mod work;
use attack_start::*;
use work::*;

struct EntityMeleeLungeRequest {
    attacker: Entity,
    target: Entity,
    weapon_reach_metres: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct MeleeLungeRequest<'a> {
    pub(crate) attacker_position: Vec3,
    pub(crate) attacker_collider: &'a Collider,
    pub(crate) attacker_dimensions: CharacterDimensions,
    pub(crate) target_transform: &'a Transform,
    pub(crate) target_collider: &'a Collider,
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
    mut states: Query<&mut TacticalCombatState>,
    mut skeletons: Query<(
        &mut SkeletonState,
        &mut QuickstepPush,
        &CharacterLook,
        Option<&Transform>,
    )>,
    viewer: TacticalPlayerViewer,
    config: Res<TacticalCombatConfig>,
) {
    let Ok(combat_state) = states.get(event.defender) else {
        cmd.trigger(DefendIntentResolved {
            defender: event.defender,
            choice: event.choice,
            accepted: false,
        });
        return;
    };
    if combat_state.is_incapacitated() {
        cmd.trigger(DefendIntentResolved {
            defender: event.defender,
            choice: event.choice,
            accepted: false,
        });
        return;
    }

    let Ok((mut skeleton, mut quickstep_push, look, transform)) = skeletons.get_mut(event.defender)
    else {
        cmd.trigger(DefendIntentResolved {
            defender: event.defender,
            choice: event.choice,
            accepted: false,
        });
        return;
    };
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
        DefendRequest::Roll if !accepts_roll_dodge(&skeleton) => false,
        DefendRequest::Roll => true,
    };
    if !accepted {
        cmd.trigger(DefendIntentResolved {
            defender: event.defender,
            choice: event.choice,
            accepted: false,
        });
        return;
    }

    if let Ok(view) = viewer.get(event.defender)
        && let Ok(mut state) = states.get_mut(event.defender)
    {
        let state = &mut *state;
        let workload = combat_action_workload(
            CombatActionWork::ExplosiveDodge,
            config.movement.maneuvers.quickstep_duration_seconds,
            0.0,
            0.0,
            view.inventory_weight(),
            view.body_weight(),
            view.raw_single_body_part_attr(SimpleAttribute::Endurance),
        );
        apply_combat_workload(
            &mut state.oxygen_debt_joules,
            &mut state.local_action_fatigue,
            workload,
            view.raw_single_body_part_attr(SimpleAttribute::Endurance),
        );
    }

    cmd.entity(event.defender).insert(PendingDefenderResponse {
        choice: event.choice,
        set_at: CombatInstant::from_elapsed(&time),
    });
    cmd.trigger(DefendIntentResolved {
        defender: event.defender,
        choice: event.choice,
        accepted: true,
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
    mut combat_states: Query<&mut TacticalCombatState>,
    transforms: Query<&Transform>,
    dimensions: Query<&CharacterDimensions>,
    colliders: Query<&Collider>,
    viewer: TacticalPlayerViewer,
    time: Res<Time<()>>,
    config: Res<TacticalCombatConfig>,
    mut random: ResMut<crate::bot::CombatRandom>,
) {
    let Ok(mut skeleton) = skeletons.get_mut(event.attacker) else {
        return;
    };
    let Ok(attack_view) = viewer.get_for_attack(event.attacker, event.hand) else {
        return;
    };
    if !adventuresim_core::combat::melee_attack_capability(&attack_view, &attack_view)
        .is_available()
    {
        return;
    }
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
    let (spec, recovery) = (
        configure_attack_curve(spec, &attack_view, &config.presentation.attack_curve),
        CombatDuration::from_secs_f32(attack_recovery_secs(
            &attack_view,
            event.strike_family.melee_style(),
            spec.continuation,
        )),
    );
    let recovery = fatigue_adjusted_attack_recovery(
        event.attacker,
        event.hand,
        recovery,
        &combat_states,
        &viewer,
    );
    let start = animation_tick(&time);
    let initial_contact =
        super::contact::initial_melee_contact(&viewer, &event, strike_family, &mut random);
    let selected_body_part = initial_contact.body_part;
    let weapon_reach = initial_contact.weapon_reach;
    let lunge_delay = started_attack_lunge_delay(
        &event,
        weapon_reach,
        &transforms,
        &dimensions,
        &colliders,
        &config,
    );
    let sequence_start = if spec.continuation {
        skeleton.attack_continuation_tick().unwrap_or(start)
    } else {
        start
    };
    let (animation_start_tick, contact_tick, recovery_tick) =
        delayed_melee_timing_ticks(sequence_start, event.windup, lunge_delay, recovery);
    let contact_windup = super::contact::windup_duration(contact_tick, start);
    let scheduled_measure_metres = scheduled_attack_measure(&event, weapon_reach, &transforms);
    if skeleton
        .begin_attack_timed(spec, animation_start_tick, contact_tick, recovery_tick)
        .is_err()
    {
        return;
    }
    charge_started_attack_work(
        event.attacker,
        event.hand,
        contact_windup,
        recovery,
        &viewer,
        &mut combat_states,
    );
    begin_started_attack_movement(
        &mut commands,
        &event,
        selected_body_part,
        weapon_reach,
        start,
        animation_start_tick,
        contact_tick,
        &transforms,
        &dimensions,
        &colliders,
        &config,
    );
    authorize_started_attack(
        &mut commands,
        &mut authority,
        &event,
        selected_body_part,
        initial_contact.sample,
        initial_contact.defense_alignment_sample,
        start,
        contact_windup,
        scheduled_measure_metres,
        &time,
        &config,
    );
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
            contact_sample: contact.contact_sample,
            defense_alignment_sample: contact.defense_alignment_sample,
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
            weapon_reach_metres: request.weapon_reach_metres,
            quickstep_distance_metres: quickstep_distance,
        },
        config,
    ) else {
        let surface =
            melee_surface_measure(attacker_transform.translation, target_transform.translation);
        let closure = (surface - reach).max(0.0);
        let outcome = if closure <= 1.0e-5 {
            "already_in_reach"
        } else {
            "unreachable_no_movement"
        };
        info!(attack_key = start_tick, attacker = ?request.attacker, target = ?request.target, outcome, reach_metres = reach, closure_metres = closure, maximum_travel_metres = maximum_travel, "melee_lunge_planned");
        return;
    };
    info!(attack_key = start_tick, attacker = ?request.attacker, target = ?request.target, outcome = if movement.quickstep { "quickstep" } else { "forward" }, reach_metres = reach, planned_distance_metres = movement.distance_metres, maximum_travel_metres = maximum_travel, "melee_lunge_planned");
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
    _config: &TacticalCombatConfig,
) -> Option<MeleeLungeMovement> {
    let direction = (request.target_transform.translation - request.attacker_position)
        .xz()
        .normalize_or_zero();
    if direction == Vec2::ZERO {
        return None;
    }
    let arm_reach = request.attacker_dimensions.arm_reach_metres;
    let maximum_travel = request
        .quickstep_distance_metres
        .min(melee_collision_clearance(
            request.attacker_position,
            request.attacker_collider,
            request.target_transform,
            request.target_collider,
        ));
    let (distance_metres, quickstep) = match melee_lunge(
        melee_surface_measure(
            request.attacker_position,
            request.target_transform.translation,
        ),
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

pub(crate) fn melee_surface_measure(attacker_position: Vec3, target_position: Vec3) -> f32 {
    (attacker_position.xz().distance(target_position.xz())
        - adventuresim_core::combat::HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES)
        .max(0.0)
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

pub(crate) fn melee_target_reachable(request: MeleeLungeRequest<'_>) -> bool {
    let maximum_travel = request
        .quickstep_distance_metres
        .min(melee_collision_clearance(
            request.attacker_position,
            request.attacker_collider,
            request.target_transform,
            request.target_collider,
        ));
    melee_surface_measure(
        request.attacker_position,
        request.target_transform.translation,
    ) <= melee_interaction_range(
        request.attacker_dimensions.arm_reach_metres,
        request.weapon_reach_metres,
    ) + maximum_travel
        + 1.0e-5
}

pub(crate) fn melee_target_lunge_delay(
    request: MeleeLungeRequest<'_>,
    config: &TacticalCombatConfig,
) -> Option<f32> {
    if !melee_target_reachable(request) {
        return None;
    }
    Some(
        planned_melee_lunge(request, config)
            .map_or(0.0, |movement| melee_lunge_movement_delay(movement, config)),
    )
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
    use crate::combat::defense::roll_dodge_reflex;
    use std::time::Duration;

    #[derive(Resource, Default)]
    struct ScheduledContactResults(Vec<Result<(), MeleeIntentRejection>>);

    fn record_scheduled_contact_geometry(
        event: On<MeleeAttackIntent>,
        transforms: Query<&Transform>,
        dimensions: Query<&CharacterDimensions>,
        mut results: ResMut<ScheduledContactResults>,
    ) {
        let result = (|| {
            let attacker_transform = transforms
                .get(event.attacker)
                .map_err(|_| MeleeIntentRejection::OutOfRange)?;
            let target_transform = transforms
                .get(event.target)
                .map_err(|_| MeleeIntentRejection::OutOfRange)?;
            let dimensions = dimensions
                .get(event.attacker)
                .map_err(|_| MeleeIntentRejection::OutOfRange)?;
            let surface_distance =
                melee_surface_measure(attacker_transform.translation, target_transform.translation);
            validate_melee_intent_cheap(MeleeIntentFacts {
                attacker: event.attacker,
                target: event.target,
                attacker_side: Some(TacticalCombatSide::Party),
                target_side: Some(TacticalCombatSide::Enemy),
                attacker_incapacitated: Some(false),
                target_incapacitated: Some(false),
                attack_capability: MeleeAttackCapability::Available,
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
                weapon_reach_metres: 0.0,
                quickstep_distance_metres: 1.0,
            },
            &config,
        );
        let arrived = movement.map_or(Vec3::ZERO, |movement| {
            Vec3::new(
                movement.direction.x * movement.distance_metres,
                0.0,
                movement.direction.y * movement.distance_metres,
            )
        });
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
                    contact_sample: 0.5,
                    defense_alignment_sample: 0.5,
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
                    weapon_reach_metres: 0.8,
                    quickstep_distance_metres: 1.0,
                },
                &config,
            )
        };

        assert!(plan(2.4).is_some(), "reachable target should plan movement");
        assert!(
            plan(3.3).is_none(),
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
                weapon_reach_metres: 0.0,
                quickstep_distance_metres: 1.0,
            },
            &config,
        );
        assert!(
            fist.is_none(),
            "fist target is already within center-based reach"
        );
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
    fn stationary_defender_melee_range_matrix_reaches_equation_authority() {
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
            _body_part,
            expected_mode,
            expected_in_reach_at_contact,
            expected_server_acceptance,
        ) in cases
        {
            let reach = melee_interaction_range(dimensions.arm_reach_metres, weapon_reach);
            let attacker_origin = Vec3::ZERO;
            let mut low = 0.0;
            let mut high = 5.0;
            for _ in 0..40 {
                let mid = (low + high) * 0.5;
                let target = Transform::from_xyz(mid, 0.0, 0.0);
                let gap = melee_surface_measure(attacker_origin, target.translation) - reach;
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
            let contact_reachable = melee_surface_measure(attacker_origin, target.translation)
                <= reach + maximum_travel + 1.0e-5;
            let movement = planned_melee_lunge(
                MeleeLungeRequest {
                    attacker_position: Vec3::ZERO,
                    attacker_collider: &collider,
                    attacker_dimensions: dimensions,
                    target_transform: &target,
                    target_collider: &collider,
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
            let arrived_origin = arrived_position;
            let surface_distance = melee_surface_measure(arrived_origin, target.translation);
            let server_accepts = surface_distance
                <= reach
                    + config
                        .realtime_authority
                        .melee
                        .range_latency_tolerance_metres;
            let in_reach_at_contact =
                melee_surface_measure(arrived_origin, target.translation) <= reach + 1.0e-4;
            println!(
                "{label:>14} gap={desired_gap:.3} mode={actual_mode:?} planned={:.3} actual={actual_displacement:.3} reachable={contact_reachable} in_reach={in_reach_at_contact} server={server_accepts}",
                movement.map_or(0.0, |movement| movement.distance_metres),
            );
            assert_eq!(
                in_reach_at_contact, expected_in_reach_at_contact,
                "{label}: authoritative reach mismatch at fixed-tick contact (surface={surface_distance:.4}, reach={reach:.4}, actual travel={actual_displacement:.4})"
            );
            assert_eq!(
                server_accepts, expected_server_acceptance,
                "{label}: server validation mismatch at fixed-tick contact (surface={surface_distance:.4}, reach={reach:.4}, actual travel={actual_displacement:.4})"
            );
        }
    }

    #[test]
    fn selected_body_part_and_hitbox_configuration_do_not_change_melee_authority() {
        let collider = Collider::cylinder(0.4, 1.9);
        let dimensions = CharacterDimensions::default();
        let target = Transform::from_xyz(2.4, 0.0, 0.0);
        let request = MeleeLungeRequest {
            attacker_position: Vec3::ZERO,
            attacker_collider: &collider,
            attacker_dimensions: dimensions,
            target_transform: &target,
            target_collider: &collider,
            weapon_reach_metres: 0.8,
            quickstep_distance_metres: 1.0,
        };
        let base_config = TacticalCombatConfig::default();
        let baseline = planned_melee_lunge(request, &base_config).expect("reachable with lunge");
        let mut altered_config = base_config.clone();
        for (index, hitbox) in altered_config
            .targeting
            .body_part_hitboxes
            .iter_mut()
            .enumerate()
        {
            hitbox.center_metres = [index as f32 * 10.0, 100.0, -100.0];
            hitbox.half_extents_metres = [0.001, 50.0, 20.0];
        }
        let altered = planned_melee_lunge(request, &altered_config).expect("same center gap");
        assert_eq!(altered.quickstep, baseline.quickstep);
        assert!((altered.distance_metres - baseline.distance_metres).abs() < f32::EPSILON);

        let surface_measure = melee_surface_measure(Vec3::ZERO, target.translation);
        for body_part in [
            BodyPart::Head,
            BodyPart::Chest,
            BodyPart::LeftArm,
            BodyPart::RightLeg,
        ] {
            let result = validate_melee_intent_cheap(MeleeIntentFacts {
                attacker: Entity::from_bits(1),
                target: Entity::from_bits(2),
                attacker_side: Some(TacticalCombatSide::Party),
                target_side: Some(TacticalCombatSide::Enemy),
                attacker_incapacitated: Some(false),
                target_incapacitated: Some(false),
                attack_capability: MeleeAttackCapability::Available,
                reported_precision: ReportedPrecision::new(1.0).unwrap(),
                arm_reach: dimensions.arm_reach_metres,
                weapon_reach: 0.8,
                range_latency_tolerance: 0.0,
                separation: surface_measure,
                authority_permits: true,
                body_part,
                attacker_position: Vec3::ZERO,
                target_position: target.translation,
                attacker_yaw: 0.0,
                target_yaw: 0.0,
            });
            assert!(matches!(result, Err(MeleeIntentRejection::OutOfRange)));
        }
    }

    #[test]
    fn clients_and_server_ai_share_authoritative_dodge_transitions() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<TacticalCombatConfig>()
            .add_observer(on_defender_response_request)
            .add_observer(apply_defend_intent);
        let player = app
            .world_mut()
            .spawn((
                TacticalCombatState::default(),
                SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised),
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
            message: DefendRequest::Dodge {
                direction: Vec2::NEG_X,
            },
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
            SkeletonAction::Dodge
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
                choice: DefendRequest::Dodge {
                    direction: Vec2::NEG_X,
                },
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
    cmd.trigger(MeleeAttackStartedIntent {
        attacker,
        target: event.target,
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
