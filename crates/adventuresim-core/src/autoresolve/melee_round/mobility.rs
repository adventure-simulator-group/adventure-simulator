use super::*;

pub(in crate::autoresolve) fn melee_effective_reach(combatant: &Combatant) -> f32 {
    HUMANOID_REFERENCE_ARM_REACH_METRES + combatant.equipment.weapon_reach().max(0.0)
}

pub(in crate::autoresolve) fn available_attack_start(
    window_start: f32,
    window_end: f32,
    measure_at: f32,
    recovery: f32,
) -> Option<f32> {
    let start = window_start.max(measure_at).max(recovery);
    (start <= window_end).then_some(start)
}

pub(super) fn movement_intent(
    combatant: &Combatant,
    distance: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> MovementIntent {
    let reach = melee_effective_reach(combatant);
    let preferred = preferred_melee_measure(combatant, parameters);
    if combatant.equipment.weapon_reach() >= parameters.long_weapon_measure_threshold_metres
        && distance < preferred
    {
        MovementIntent::Retreat
    } else if distance > reach
        || combatant.melee_attack_started_at_seconds.is_some() && distance >= preferred
    {
        MovementIntent::Close
    } else {
        MovementIntent::Hold
    }
}

pub(super) fn maximum_melee_pair_surface_separation(
    first: &Combatant,
    second: &Combatant,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    parameters
        .formation_spacing_metres
        .max(melee_effective_reach(first))
        .max(melee_effective_reach(second))
}

pub(in crate::autoresolve) fn preview_melee_pair_movement(
    first: &Combatant,
    second: &Combatant,
    elapsed: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> (OpposedMovement, MovementIntent, MovementIntent) {
    let surface = first
        .melee_engagement_distance_metres
        .min(second.melee_engagement_distance_metres)
        .max(0.0);
    let first_intent = movement_intent(first, surface, parameters);
    let second_intent = movement_intent(second, surface, parameters);
    let speed = |combatant: &Combatant| {
        combatant
            .movement_speed_meters_per_second(parameters.minimum_movement_speed_metres_per_second)
            .min(if combatant.melee_attack_started_at_seconds.is_some() {
                parameters.melee_lunge_speed_metres_per_second
            } else {
                parameters.guarded_movement_speed_metres_per_second
            })
    };
    let acceleration = |combatant: &Combatant| {
        ground_drive_acceleration(
            parameters.reference_ground_drive_force_newtons,
            combatant.attributes.limb_attr_by_weight_by_parts(
                LimbAttribute::Strength,
                &combatant.body,
                LimbWeights::both_legs(),
            ),
            parameters.reference_leg_strength,
            combatant.body.weight_kg,
            combatant.equipment.inventory_weight,
            parameters.gravity_metres_per_second_squared,
            parameters.traction_coefficient,
        )
    };
    let maximum_surface_separation =
        maximum_melee_pair_surface_separation(first, second, parameters);
    let mut movement = integrate_opposed_movement(
        surface + HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
        first.melee_separation_velocity_metres_per_second,
        first_intent,
        speed(first),
        second.melee_separation_velocity_metres_per_second,
        second_intent,
        speed(second),
        acceleration(first),
        acceleration(second),
        elapsed,
        HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
        maximum_surface_separation + HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
    );
    movement.distance_before_metres = (movement.distance_before_metres
        - HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES)
        .max(0.0);
    movement.distance_after_metres =
        (movement.distance_after_metres - HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES).max(0.0);
    (movement, first_intent, second_intent)
}

pub(in crate::autoresolve) fn preferred_melee_measure(
    combatant: &Combatant,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    let reach = melee_effective_reach(combatant);
    combatant.equipment.melee_weapon.map_or(
        reach * parameters.melee_measure_reach_fraction,
        |weapon| {
            preferred_melee_striking_measure(
                reach,
                weapon.grip_to_tip_m,
                weapon.striking_head_length_m,
                weapon.distal_headed,
                parameters.melee_measure_reach_fraction,
            )
        },
    )
}

fn movement_action(intent: MovementIntent) -> MeleeMovementAction {
    match intent {
        MovementIntent::Close => MeleeMovementAction::Close,
        MovementIntent::Hold => MeleeMovementAction::Hold,
        MovementIntent::Retreat => MeleeMovementAction::Retreat,
    }
}

fn movement_phase(combatant: &Combatant, time: f32) -> MeleeTimelinePhase {
    if combatant.melee_attack_started_at_seconds.is_some() {
        MeleeTimelinePhase::Windup
    } else if combatant.melee_recovery_until_seconds > time {
        MeleeTimelinePhase::Recovery
    } else {
        MeleeTimelinePhase::NeutralGuard
    }
}

fn record_movement(
    recorder: &mut BattleRecorder,
    combatant: &Combatant,
    target_id: u64,
    movement: OpposedMovement,
    axis: AxisMotion,
    intent: MovementIntent,
    time: f32,
) {
    let phase = movement_phase(combatant, time);
    let mut event = MeleeTimelineEvent::at(MeleeTimelineKind::Movement, time);
    event.combatant_id = Some(combatant.id);
    event.target_id = Some(target_id);
    event.engagement_distance_before_metres = Some(movement.distance_before_metres);
    event.engagement_distance_after_metres = Some(movement.distance_after_metres);
    event.movement_action = Some(movement_action(intent));
    event.movement_elapsed_seconds = Some(movement.elapsed_seconds);
    event.movement_displacement_metres = Some(axis.displacement_metres);
    event.movement_velocity_before_metres_per_second = Some(axis.velocity_before_metres_per_second);
    event.movement_velocity_after_metres_per_second = Some(axis.velocity_after_metres_per_second);
    event.movement_speed_limit_metres_per_second = Some(axis.speed_limit_metres_per_second);
    event.readiness_before_seconds = Some(combatant.melee_recovery_until_seconds);
    event.readiness_after_seconds = event.readiness_before_seconds;
    event.phase_before = Some(phase);
    event.phase_after = Some(phase);
    recorder.record_timeline(event);
}

pub(in crate::autoresolve) fn advance_melee_pair_movement(
    first: &mut Combatant,
    second: &mut Combatant,
    interval_start: f32,
    elapsed: f32,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    if elapsed <= 0.0 || first.is_defeated() || second.is_defeated() {
        return;
    }
    if first.melee_engagement_target != Some(second.id)
        || second.melee_engagement_target != Some(first.id)
    {
        let initial_separation = maximum_melee_pair_surface_separation(first, second, parameters);
        first.melee_engagement_target = Some(second.id);
        second.melee_engagement_target = Some(first.id);
        first.melee_engagement_distance_metres = initial_separation;
        second.melee_engagement_distance_metres = initial_separation;
        first.melee_separation_velocity_metres_per_second = 0.0;
        second.melee_separation_velocity_metres_per_second = 0.0;
    }
    let (movement, first_intent, second_intent) =
        preview_melee_pair_movement(first, second, elapsed, parameters);
    first.melee_engagement_distance_metres = movement.distance_after_metres;
    second.melee_engagement_distance_metres = movement.distance_after_metres;
    first.melee_separation_velocity_metres_per_second =
        movement.first.velocity_after_metres_per_second;
    second.melee_separation_velocity_metres_per_second =
        movement.second.velocity_after_metres_per_second;
    let time = interval_start + elapsed;
    let (a, a_axis, a_intent, b, b_axis, b_intent) = if first.id <= second.id {
        (
            &*first,
            movement.first,
            first_intent,
            &*second,
            movement.second,
            second_intent,
        )
    } else {
        (
            &*second,
            movement.second,
            second_intent,
            &*first,
            movement.first,
            first_intent,
        )
    };
    record_movement(recorder, a, b.id, movement, a_axis, a_intent, time);
    record_movement(recorder, b, a.id, movement, b_axis, b_intent, time);
}
