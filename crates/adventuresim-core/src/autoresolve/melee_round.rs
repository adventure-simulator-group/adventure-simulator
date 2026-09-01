use super::*;

fn available_attack_start(
    window_start_seconds: f32,
    window_end_seconds: f32,
    measure_reached_at_seconds: f32,
    recovery_until_seconds: f32,
) -> Option<f32> {
    let start = window_start_seconds
        .max(measure_reached_at_seconds)
        .max(recovery_until_seconds);
    (start <= window_end_seconds).then_some(start)
}

fn movement_intent(
    combatant: &Combatant,
    distance_metres: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> MovementIntent {
    let reach = combatant.equipment.weapon_reach().max(0.4);
    let preferred_measure = preferred_melee_measure(combatant, parameters);
    if reach >= parameters.long_weapon_measure_threshold_metres
        && distance_metres < preferred_measure
    {
        MovementIntent::Retreat
    } else if distance_metres > reach
        || combatant.melee_attack_started_at_seconds.is_some()
            && distance_metres >= preferred_measure
    {
        // A committed attack retains physically bounded pursuit/forward-step
        // tracking instead of freezing while its target moves.
        MovementIntent::Close
    } else {
        MovementIntent::Hold
    }
}

fn preferred_melee_measure(
    combatant: &Combatant,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    let reach = combatant.equipment.weapon_reach().max(0.4);
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

fn timeline_movement_action(intent: MovementIntent) -> MeleeMovementAction {
    match intent {
        MovementIntent::Close => MeleeMovementAction::Close,
        MovementIntent::Hold => MeleeMovementAction::Hold,
        MovementIntent::Retreat => MeleeMovementAction::Retreat,
    }
}

fn movement_phase(combatant: &Combatant, time_seconds: f32) -> MeleeTimelinePhase {
    if combatant.melee_attack_started_at_seconds.is_some() {
        MeleeTimelinePhase::Windup
    } else if combatant.melee_recovery_until_seconds > time_seconds {
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
    time_seconds: f32,
) {
    let phase = movement_phase(combatant, time_seconds);
    let mut event = MeleeTimelineEvent::at(MeleeTimelineKind::Movement, time_seconds);
    event.combatant_id = Some(combatant.id);
    event.target_id = Some(target_id);
    event.engagement_distance_before_metres = Some(movement.distance_before_metres);
    event.engagement_distance_after_metres = Some(movement.distance_after_metres);
    event.movement_action = Some(timeline_movement_action(intent));
    event.movement_elapsed_seconds = Some(movement.elapsed_seconds);
    event.movement_displacement_metres = Some(axis.displacement_metres);
    event.movement_velocity_before_metres_per_second = Some(axis.velocity_before_metres_per_second);
    event.movement_velocity_after_metres_per_second = Some(axis.velocity_after_metres_per_second);
    event.readiness_before_seconds = Some(combatant.melee_recovery_until_seconds);
    event.readiness_after_seconds = event.readiness_before_seconds;
    event.phase_before = Some(phase);
    event.phase_after = Some(phase);
    recorder.record_timeline(event);
}

pub(super) fn advance_melee_pair_movement(
    first: &mut Combatant,
    second: &mut Combatant,
    interval_start_seconds: f32,
    elapsed_seconds: f32,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    if elapsed_seconds <= 0.0 || first.is_defeated() || second.is_defeated() {
        return;
    }
    if first.melee_engagement_target != Some(second.id)
        || second.melee_engagement_target != Some(first.id)
    {
        first.melee_engagement_target = Some(second.id);
        second.melee_engagement_target = Some(first.id);
        first.melee_engagement_distance_metres = parameters.formation_spacing_metres;
        second.melee_engagement_distance_metres = parameters.formation_spacing_metres;
        first.melee_separation_velocity_metres_per_second = 0.0;
        second.melee_separation_velocity_metres_per_second = 0.0;
    }
    let (movement, first_intent, second_intent) =
        preview_melee_pair_movement(first, second, elapsed_seconds, parameters);
    first.melee_engagement_distance_metres = movement.distance_after_metres;
    second.melee_engagement_distance_metres = movement.distance_after_metres;
    first.melee_separation_velocity_metres_per_second =
        movement.first.velocity_after_metres_per_second;
    second.melee_separation_velocity_metres_per_second =
        movement.second.velocity_after_metres_per_second;
    let event_time = interval_start_seconds + elapsed_seconds;
    if first.id <= second.id {
        record_movement(
            recorder,
            first,
            second.id,
            movement,
            movement.first,
            first_intent,
            event_time,
        );
        record_movement(
            recorder,
            second,
            first.id,
            movement,
            movement.second,
            second_intent,
            event_time,
        );
    } else {
        record_movement(
            recorder,
            second,
            first.id,
            movement,
            movement.second,
            second_intent,
            event_time,
        );
        record_movement(
            recorder,
            first,
            second.id,
            movement,
            movement.first,
            first_intent,
            event_time,
        );
    }
}

fn preview_melee_pair_movement(
    first: &Combatant,
    second: &Combatant,
    elapsed_seconds: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> (OpposedMovement, MovementIntent, MovementIntent) {
    // The combat state and contact telemetry express measure from the weapon
    // origin to the target body's near surface. The locomotion integrator owns
    // center separation, so add the two authoritative collider radii for the
    // physical constraint and project the result back to surface measure.
    let surface_distance = first
        .melee_engagement_distance_metres
        .min(second.melee_engagement_distance_metres)
        .max(0.0);
    let center_distance = surface_distance + HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES;
    let first_intent = movement_intent(first, surface_distance, parameters);
    let second_intent = movement_intent(second, surface_distance, parameters);
    let mut movement = integrate_opposed_movement(
        center_distance,
        first.melee_separation_velocity_metres_per_second,
        first_intent,
        first
            .movement_speed_meters_per_second(parameters.minimum_movement_speed_metres_per_second)
            .min(if first.melee_attack_started_at_seconds.is_some() {
                parameters.melee_lunge_speed_metres_per_second
            } else {
                parameters.guarded_movement_speed_metres_per_second
            }),
        second.melee_separation_velocity_metres_per_second,
        second_intent,
        second
            .movement_speed_meters_per_second(parameters.minimum_movement_speed_metres_per_second)
            .min(if second.melee_attack_started_at_seconds.is_some() {
                parameters.melee_lunge_speed_metres_per_second
            } else {
                parameters.guarded_movement_speed_metres_per_second
            }),
        ground_drive_acceleration(
            parameters.reference_ground_drive_force_newtons,
            first.attributes.limb_attr_by_weight_by_parts(
                LimbAttribute::Strength,
                &first.body,
                LimbWeights::both_legs(),
            ),
            parameters.reference_leg_strength,
            first.body.weight_kg,
            first.equipment.inventory_weight,
            parameters.gravity_metres_per_second_squared,
            parameters.traction_coefficient,
        ),
        ground_drive_acceleration(
            parameters.reference_ground_drive_force_newtons,
            second.attributes.limb_attr_by_weight_by_parts(
                LimbAttribute::Strength,
                &second.body,
                LimbWeights::both_legs(),
            ),
            parameters.reference_leg_strength,
            second.body.weight_kg,
            second.equipment.inventory_weight,
            parameters.gravity_metres_per_second_squared,
            parameters.traction_coefficient,
        ),
        elapsed_seconds,
        HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
        parameters.formation_spacing_metres + HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
    );
    movement.distance_before_metres = (movement.distance_before_metres
        - HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES)
        .max(0.0);
    movement.distance_after_metres =
        (movement.distance_after_metres - HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES).max(0.0);
    (movement, first_intent, second_intent)
}

fn swept_entry_seconds(
    first: &Combatant,
    second: &Combatant,
    attacker_is_first: bool,
    maximum_elapsed_seconds: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> Option<f32> {
    let attacker = if attacker_is_first { first } else { second };
    attacker.melee_attack_started_at_seconds?;
    let reach = attacker.equipment.weapon_reach().max(0.4);
    let distance_before = first
        .melee_engagement_distance_metres
        .min(second.melee_engagement_distance_metres);
    if distance_before <= reach {
        return None;
    }
    let (full, _, _) =
        preview_melee_pair_movement(first, second, maximum_elapsed_seconds, parameters);
    if full.distance_after_metres > reach {
        return None;
    }
    let mut lower = 0.0;
    let mut upper = maximum_elapsed_seconds;
    for _ in 0..24 {
        let middle = (lower + upper) * 0.5;
        let (movement, _, _) = preview_melee_pair_movement(first, second, middle, parameters);
        if movement.distance_after_metres <= reach {
            upper = middle;
        } else {
            lower = middle;
        }
    }
    Some(upper)
}

pub(super) fn reschedule_swept_pair_contacts(
    first: &mut Combatant,
    second: &mut Combatant,
    interval_start_seconds: f32,
    elapsed_seconds: f32,
    parameters: crate::combat::AutoresolveParameters,
) {
    if elapsed_seconds <= 0.0 {
        return;
    }
    let first_entry = swept_entry_seconds(first, second, true, elapsed_seconds, parameters);
    let second_entry = swept_entry_seconds(first, second, false, elapsed_seconds, parameters);
    let interval_end_seconds = interval_start_seconds + elapsed_seconds;
    if let Some(entry) = first_entry {
        let swept_contact = interval_start_seconds + entry;
        if first
            .melee_attack_contact_at_seconds
            .is_some_and(|nominal| swept_contact < nominal && swept_contact <= interval_end_seconds)
        {
            first.melee_attack_contact_at_seconds = Some(swept_contact);
        }
    }
    if let Some(entry) = second_entry {
        let swept_contact = interval_start_seconds + entry;
        if second
            .melee_attack_contact_at_seconds
            .is_some_and(|nominal| swept_contact < nominal && swept_contact <= interval_end_seconds)
        {
            second.melee_attack_contact_at_seconds = Some(swept_contact);
        }
    }
}

pub(super) fn schedule_side_melee_attacks(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<ScheduledMeleeAttack> {
    let round_start_seconds = round.saturating_sub(1) as f32 * parameters.combat_round_seconds;
    schedule_side_melee_attacks_in_window(
        attackers,
        defenders,
        round_start_seconds,
        parameters.combat_round_seconds,
        random,
        recorder,
        parameters,
    )
}

pub(super) fn schedule_side_melee_attacks_in_window(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    window_start_seconds: f32,
    window_seconds: f32,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<ScheduledMeleeAttack> {
    let mut scheduled = Vec::new();
    for attacker_index in 0..attackers.len() {
        if attackers[attacker_index].is_defeated() || side_defeated(defenders) {
            continue;
        }
        if preferred_attack_mode(&attackers[attacker_index]) != AttackMode::Melee {
            continue;
        }
        if !attackers[attacker_index].can_attack_melee() {
            attackers[attacker_index].yielded = true;
            continue;
        }
        let (target_index, flanking) =
            melee_assignment(attacker_index, attackers, defenders, parameters);
        let window_end_seconds = window_start_seconds + window_seconds;
        if let (Some(started_at_seconds), Some(contact_at_seconds)) = (
            attackers[attacker_index].melee_attack_started_at_seconds,
            attackers[attacker_index].melee_attack_contact_at_seconds,
        ) {
            if contact_at_seconds <= window_end_seconds {
                scheduled.push(ScheduledMeleeAttack {
                    attacker_index,
                    target_index,
                    flanking,
                    attack_timing: MeleeAttackTiming {
                        started_at_seconds,
                        contact_at_seconds,
                        recovery_until_seconds: attackers[attacker_index]
                            .melee_recovery_until_seconds,
                    },
                });
            }
            continue;
        }
        let performance = attackers[attacker_index].fatigue_performance();
        let interval = (attackers[attacker_index]
            .equipment
            .melee_weapon
            .map_or(parameters.reference_melee_attack_seconds, |weapon| {
                weapon.attack_interval_seconds
            })
            .max(parameters.minimum_attack_interval_seconds)
            + attackers[attacker_index].melee_interval_jitter_seconds)
            / performance;
        let attacker_reach = attackers[attacker_index].equipment.weapon_reach().max(0.4);
        let target_id = defenders[target_index].id;
        let attacker_id = attackers[attacker_index].id;
        if attackers[attacker_index].melee_engagement_target != Some(target_id) {
            attackers[attacker_index].melee_engagement_target = Some(target_id);
            attackers[attacker_index].melee_engagement_distance_metres =
                parameters.formation_spacing_metres;
        }
        if defenders[target_index].melee_engagement_target != Some(attacker_id) {
            defenders[target_index].melee_engagement_target = Some(attacker_id);
            defenders[target_index].melee_engagement_distance_metres =
                parameters.formation_spacing_metres;
        }
        let distance = attackers[attacker_index]
            .melee_engagement_distance_metres
            .min(defenders[target_index].melee_engagement_distance_metres)
            .max(0.0);
        let readiness_before_seconds = attackers[attacker_index].melee_recovery_until_seconds
            + attackers[attacker_index].melee_phase_adaptation_delay_seconds;
        let movement_phase = Some(
            if attackers[attacker_index]
                .melee_attack_started_at_seconds
                .is_some()
            {
                MeleeTimelinePhase::Windup
            } else if readiness_before_seconds > window_start_seconds {
                MeleeTimelinePhase::Recovery
            } else {
                MeleeTimelinePhase::NeutralGuard
            },
        );
        if distance > attacker_reach + parameters.melee_lunge_maximum_travel_metres {
            continue;
        }
        let Some(started_at_seconds) = available_attack_start(
            window_start_seconds,
            window_end_seconds,
            window_start_seconds,
            readiness_before_seconds,
        ) else {
            continue;
        };
        let contact_at_seconds = started_at_seconds + parameters.melee_windup_seconds;
        let attack_timing = MeleeAttackTiming {
            started_at_seconds,
            contact_at_seconds,
            recovery_until_seconds: started_at_seconds + interval,
        };
        attackers[attacker_index].melee_interval_jitter_seconds =
            random.unit_f32() * parameters.melee_cadence_jitter_seconds;
        let phase_adaptation_delay_seconds =
            attackers[attacker_index].melee_phase_adaptation_delay_seconds;
        attackers[attacker_index].melee_attack_started_at_seconds =
            Some(attack_timing.started_at_seconds);
        attackers[attacker_index].melee_attack_contact_at_seconds =
            Some(attack_timing.contact_at_seconds);
        attackers[attacker_index].melee_attack_scheduled_measure_metres = Some(distance);
        attackers[attacker_index].melee_recovery_until_seconds =
            attack_timing.recovery_until_seconds;
        attackers[attacker_index].melee_phase_adaptation_delay_seconds = 0.0;
        let mut started =
            MeleeTimelineEvent::at(MeleeTimelineKind::AttackStarted, started_at_seconds);
        started.combatant_id = Some(attacker_id);
        started.target_id = Some(target_id);
        started.engagement_distance_before_metres = Some(distance);
        started.engagement_distance_after_metres = Some(distance);
        started.readiness_before_seconds = Some(readiness_before_seconds);
        started.readiness_after_seconds = Some(attack_timing.recovery_until_seconds);
        started.attack_id = Some(attack_timing.attack_id(attacker_id));
        started.attack_started_tick = Some(MeleeTimelineEvent::tick_at(started_at_seconds));
        started.attack_contact_tick = Some(MeleeTimelineEvent::tick_at(contact_at_seconds));
        started.attack_recovery_tick = Some(MeleeTimelineEvent::tick_at(
            attack_timing.recovery_until_seconds,
        ));
        started.phase_before = movement_phase;
        started.phase_after = Some(MeleeTimelinePhase::Windup);
        started.phase_adaptation_delay_seconds = Some(phase_adaptation_delay_seconds);
        recorder.record_timeline(started);
        if contact_at_seconds <= window_end_seconds {
            scheduled.push(ScheduledMeleeAttack {
                attacker_index,
                target_index,
                flanking,
                attack_timing,
            });
        }
    }
    scheduled
}

pub(super) fn scheduled_side_contacts_in_window(
    attackers: &[Combatant],
    defenders: &[Combatant],
    window_start_seconds: f32,
    window_end_seconds: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<ScheduledMeleeAttack> {
    attackers
        .iter()
        .enumerate()
        .filter_map(|(attacker_index, attacker)| {
            let started_at_seconds = attacker.melee_attack_started_at_seconds?;
            let contact_at_seconds = attacker.melee_attack_contact_at_seconds?;
            if contact_at_seconds < window_start_seconds || contact_at_seconds > window_end_seconds
            {
                return None;
            }
            let (assigned_target, flanking) =
                melee_assignment(attacker_index, attackers, defenders, parameters);
            let target_index = attacker
                .melee_engagement_target
                .and_then(|target_id| defenders.iter().position(|target| target.id == target_id))
                .unwrap_or(assigned_target);
            Some(ScheduledMeleeAttack {
                attacker_index,
                target_index,
                flanking,
                attack_timing: MeleeAttackTiming {
                    started_at_seconds,
                    contact_at_seconds,
                    recovery_until_seconds: attacker.melee_recovery_until_seconds,
                },
            })
        })
        .collect()
}

pub(super) fn take_side_turns(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    let scheduled =
        schedule_side_melee_attacks(attackers, defenders, round, random, recorder, parameters);
    for attack in scheduled {
        if !scheduled_attack_is_current(&attackers[attack.attacker_index], attack.attack_timing) {
            continue;
        }
        let attack_id = attack
            .attack_timing
            .attack_id(attackers[attack.attacker_index].id);
        resolve_melee_turn(
            attack.attacker_index,
            attack.target_index,
            attack.flanking,
            attackers,
            defenders,
            round,
            random,
            recorder,
            parameters,
            attack.attack_timing,
            MeleeContactBatch {
                id: attack_id,
                members: vec![attack_id],
                order: 0,
            },
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScheduledMeleeAttack {
    pub attacker_index: usize,
    pub target_index: usize,
    pub flanking: f32,
    pub attack_timing: MeleeAttackTiming,
}

#[derive(Clone, Debug)]
pub(super) struct MeleeContactBatch {
    pub id: u64,
    pub members: Vec<u64>,
    pub order: u32,
}

pub(super) fn scheduled_attack_is_current(attacker: &Combatant, timing: MeleeAttackTiming) -> bool {
    attacker.melee_attack_started_at_seconds == Some(timing.started_at_seconds)
        && attacker.melee_attack_contact_at_seconds == Some(timing.contact_at_seconds)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the discrete-event contact boundary joins both mutable sides, seeded sampling, and exact scheduled identity"
)]
pub(super) fn resolve_melee_turn(
    attacker_index: usize,
    target_index: usize,
    flanking: f32,
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
    attack_timing: MeleeAttackTiming,
    contact_batch: MeleeContactBatch,
) {
    let attack_power_multiplier = std::mem::replace(
        &mut attackers[attacker_index].melee_attack_power_multiplier,
        1.0,
    );
    let scheduled_measure_metres = attackers[attacker_index]
        .melee_attack_scheduled_measure_metres
        .unwrap_or(attackers[attacker_index].melee_engagement_distance_metres);
    let actual_measure_metres = attackers[attacker_index]
        .melee_engagement_distance_metres
        .min(defenders[target_index].melee_engagement_distance_metres)
        .max(0.0);
    let melee_equipment = attackers[attacker_index].equipment.for_melee();
    let contact_at_time = resolve_melee_contact_at_time(MeleeContactAtTimeFacts {
        scheduled_measure_metres,
        actual_measure_metres,
        effective_reach_metres: melee_equipment.weapon_reach().max(0.4),
        grip_to_tip_metres: melee_equipment.weapon_grip_to_tip(),
        total_length_metres: melee_equipment.weapon_total_length(),
        striking_head_length_metres: melee_equipment.weapon_striking_head_length(),
        distal_headed: melee_equipment
            .weapon
            .is_some_and(|weapon| weapon.distal_headed),
        attack_style: melee_equipment.weapon_preferred_melee_style(),
        body_material: melee_equipment.weapon_body_material(),
        striking_material: melee_equipment.weapon_striking_material(),
    });
    let hit_precision = autoresolve_hit_precision(random, parameters);
    let reaction_timing_sample = random.unit_f32();
    let scheduled_defender_timing_before = defenders[target_index]
        .melee_attack_started_at_seconds
        .zip(defenders[target_index].melee_attack_contact_at_seconds)
        .map(
            |(started_at_seconds, contact_at_seconds)| MeleeAttackTiming {
                started_at_seconds,
                contact_at_seconds,
                recovery_until_seconds: defenders[target_index].melee_recovery_until_seconds,
            },
        );
    let defender_phase = defender_phase_at_contact(
        &defenders[target_index],
        attackers[attacker_index].id,
        attack_timing,
    );
    let defender_decision = autoresolve_melee_defender_response(
        &defenders[target_index],
        random.unit_f32(),
        reaction_timing_sample,
        random.unit_f32(),
        attack_timing,
        defender_phase,
        parameters,
    );
    let response = defender_decision
        .response
        .scaled_for_performance(defenders[target_index].fatigue_performance());
    let contact_sample = random.unit_f32();
    let contact = autoresolve_melee_contact_location(
        &attackers[attacker_index],
        &defenders[target_index],
        hit_precision,
        contact_sample,
    );
    let response = shield_aligned_response(
        response,
        defenders[target_index].equipment.shield_holding_side(),
        contact,
    );
    let exchange = melee_exchange_at_contact(
        &attackers[attacker_index],
        &defenders[target_index],
        hit_precision,
        flanking,
        contact_sample,
        response,
        random.unit_f32(),
        autoresolve_melee_reaction_timing(reaction_timing_sample, parameters)
            .displacement_time_seconds,
        actual_measure_metres,
        contact_at_time,
    );
    let result = exchange.result * attack_power_multiplier;
    let part = exchange.contact.body_part;
    let attacker_fatigue_performance = attackers[attacker_index].fatigue_performance();
    let attack_duration = attackers[attacker_index]
        .equipment
        .melee_weapon
        .map_or(parameters.reference_melee_attack_seconds, |weapon| {
            weapon.attack_interval_seconds
        });
    attackers[attacker_index].charge_action_work(CombatActionWork::Attack, attack_duration);
    let defender_readiness_before_seconds = scheduled_defender_timing_before.map_or(
        defenders[target_index].melee_recovery_until_seconds,
        |timing| timing.recovery_until_seconds,
    );
    let defense_commitment = commit_defensive_action(
        &mut defenders[target_index],
        response,
        exchange.effective_response,
        defender_phase,
    );
    let defender_id = defenders[target_index].id;
    let affected_attack_id = match defender_phase {
        MeleeDefenderPhase::CommittedAttack(timing) => Some(timing.attack_id(defender_id)),
        MeleeDefenderPhase::NeutralGuard | MeleeDefenderPhase::OccupiedRecovery { .. } => None,
    };
    let phase_before = timeline_phase(defender_phase);
    let phase_after = timeline_phase_after_commitment(defense_commitment.kind, defender_phase);
    let phase_adaptation_delay_seconds =
        (defense_commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon).then(|| {
            phase_adaptation_delay(
                defender_phase,
                attack_timing,
                &defenders[target_index],
                parameters,
            )
        });
    let mut response_event = MeleeTimelineEvent::at(
        MeleeTimelineKind::Response,
        attack_timing.contact_at_seconds,
    );
    response_event.combatant_id = Some(defender_id);
    response_event.target_id = Some(attackers[attacker_index].id);
    response_event.attack_id = Some(attack_timing.attack_id(attackers[attacker_index].id));
    let consecutive_intercepts_before = defenders[target_index].melee_consecutive_intercepts;
    response_event.response_choice = Some(
        if defender_decision
            .committed
            .is_some_and(|decision| decision.choice == CommittedThreatChoice::FinishTrade)
        {
            MeleeResponseChoice::FinishTrade
        } else {
            response_choice(response)
        },
    );
    if let Some(committed) = defender_decision.committed {
        response_event.committed_finish_trade_probability =
            Some(committed.finish_trade_probability);
        response_event.committed_completed_work_fraction = Some(committed.completed_work_fraction);
        response_event.committed_expected_intercept_benefit =
            Some(committed.expected_intercept_benefit);
        response_event.consecutive_intercepts_before = Some(consecutive_intercepts_before);
        response_event.consecutive_intercepts_after = Some(
            if defense_commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon {
                consecutive_intercepts_before.saturating_add(1)
            } else if committed.choice == CommittedThreatChoice::FinishTrade {
                consecutive_intercepts_before.saturating_sub(1)
            } else {
                consecutive_intercepts_before
            },
        );
        response_event.phase_adaptation_delay_seconds = phase_adaptation_delay_seconds;
    }
    response_event.response_availability = Some(response_availability(
        &defenders[target_index],
        response,
        defender_phase,
        attack_timing,
    ));
    response_event.phase_before = Some(phase_before);
    response_event.phase_after = Some(phase_after);
    response_event.affected_attack_id = affected_attack_id;
    response_event.engagement_distance_before_metres =
        Some(attackers[attacker_index].melee_engagement_distance_metres);
    response_event.engagement_distance_after_metres =
        response_event.engagement_distance_before_metres;
    response_event.readiness_before_seconds = Some(defender_readiness_before_seconds);
    let defender_readiness_after_seconds = defender_readiness_before_seconds
        .max(attack_timing.contact_at_seconds + defense_commitment.recovery_seconds_after_contact);
    response_event.readiness_after_seconds = Some(defender_readiness_after_seconds);
    recorder.record_timeline(response_event);
    if matches!(
        defense_commitment.kind,
        MeleeDefenseCommitmentKind::CanceledSameWeapon
            | MeleeDefenseCommitmentKind::TransformedOffhand
    ) {
        let kind = if defense_commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon {
            MeleeTimelineKind::AttackCanceled
        } else {
            MeleeTimelineKind::AttackTransformed
        };
        let mut transformation = MeleeTimelineEvent::at(kind, attack_timing.contact_at_seconds);
        transformation.combatant_id = Some(defender_id);
        transformation.target_id = Some(attackers[attacker_index].id);
        transformation.attack_id = Some(attack_timing.attack_id(attackers[attacker_index].id));
        transformation.affected_attack_id = affected_attack_id;
        transformation.phase_before = Some(phase_before);
        transformation.phase_after = Some(phase_after);
        transformation.readiness_before_seconds = Some(defender_readiness_before_seconds);
        transformation.readiness_after_seconds = Some(defender_readiness_after_seconds);
        recorder.record_timeline(transformation);
    }
    if response != DefenderResponse::None {
        defenders[target_index].melee_recovery_until_seconds =
            defenders[target_index].melee_recovery_until_seconds.max(
                attack_timing.contact_at_seconds
                    + defense_commitment.recovery_seconds_after_contact,
            );
    }
    if defense_commitment.kind == MeleeDefenseCommitmentKind::CanceledSameWeapon {
        defenders[target_index].melee_consecutive_intercepts = defenders[target_index]
            .melee_consecutive_intercepts
            .saturating_add(1);
        defenders[target_index].melee_phase_adaptation_delay_seconds =
            phase_adaptation_delay_seconds.unwrap_or(0.0);
        defenders[target_index].melee_attack_started_at_seconds = None;
        defenders[target_index].melee_attack_contact_at_seconds = None;
        defenders[target_index].melee_attack_scheduled_measure_metres = None;
    } else if defender_decision
        .committed
        .is_some_and(|decision| decision.choice == CommittedThreatChoice::FinishTrade)
    {
        defenders[target_index].melee_consecutive_intercepts = defenders[target_index]
            .melee_consecutive_intercepts
            .saturating_sub(1);
    }
    charge_defensive_work(&mut defenders[target_index], response);
    let effect = apply_attack_result(
        &mut attackers[attacker_index],
        &mut defenders[target_index],
        result,
        part,
    );
    recorder.record_attack(
        "main",
        round,
        attackers[attacker_index].id,
        defenders[target_index].id,
        AttackMode::Melee,
        attackers[attacker_index].equipment.melee_weapon_id,
        None,
        melee_defender_contact_item_id(result, response, &defenders[target_index].equipment),
        match response {
            DefenderResponse::None => "none",
            DefenderResponse::Block { .. } => "block",
            DefenderResponse::Parry { .. } => "parry",
            DefenderResponse::Dodge { .. } => "dodge",
        },
        part,
        result,
        effect,
        Some(MeleeContactTelemetry {
            anatomical_subregion: exchange.contact.anatomical_subregion,
            surface_coordinate: exchange.contact.surface_coordinate,
            armor_layer_chain: autoresolve_armor_layer_chain(
                &defenders[target_index].equipment,
                exchange.contact,
            ),
            redirected_from: exchange.redirected_from,
            dodge_closest_approach_metres: exchange
                .dodge_geometry
                .map(|geometry| geometry.closest_approach_metres),
            dodge_displacement_time_seconds: exchange.dodge_geometry.map(|_| {
                autoresolve_melee_reaction_timing(reaction_timing_sample, parameters)
                    .displacement_time_seconds
            }),
            dodge_contacted_body_part: exchange
                .dodge_geometry
                .and_then(|geometry| geometry.contacted_body_part),
            scheduled_contact_measure_metres: contact_at_time.scheduled_measure_metres,
            actual_contact_measure_metres: contact_at_time.actual_measure_metres,
            actual_center_separation_metres: contact_at_time.actual_measure_metres
                + HUMANOID_MELEE_MINIMUM_CENTER_SEPARATION_METRES,
            contact_classification: contact_at_time.classification,
            contact_lever_arm_metres: contact_at_time.lever_arm_metres,
            contact_energy_fraction: contact_at_time.energy_fraction,
            contact_invalidation_cause: contact_at_time.invalidation_cause,
            contact_material: contact_at_time.contact_material,
            defense_success_probability: exchange
                .defense_alignment
                .map(|alignment| alignment.success_probability),
            defense_alignment_sample: exchange
                .defense_alignment
                .map(|alignment| alignment.alignment_sample),
            defense_engagement: exchange
                .defense_alignment
                .map(|alignment| alignment.engagement),
            effective_defender_response: response_name(exchange.effective_response),
            defender_attack_commitment: defense_commitment.kind.as_str(),
            defender_retained_attack_power: defense_commitment.retained_power,
            attack_power_multiplier,
            attacker_fatigue_performance,
            attack_interval_seconds: attack_duration / attacker_fatigue_performance,
        }),
    );
    let mut contact_event =
        MeleeTimelineEvent::at(MeleeTimelineKind::Contact, attack_timing.contact_at_seconds);
    contact_event.combatant_id = Some(attackers[attacker_index].id);
    contact_event.target_id = Some(defender_id);
    contact_event.attack_id = Some(attack_timing.attack_id(attackers[attacker_index].id));
    contact_event.attack_started_tick = Some(MeleeTimelineEvent::tick_at(
        attack_timing.started_at_seconds,
    ));
    contact_event.attack_contact_tick = Some(MeleeTimelineEvent::tick_at(
        attack_timing.contact_at_seconds,
    ));
    contact_event.attack_recovery_tick = Some(MeleeTimelineEvent::tick_at(
        attack_timing.recovery_until_seconds,
    ));
    contact_event.simultaneous_batch_id = Some(contact_batch.id);
    contact_event.simultaneous_members = contact_batch.members;
    contact_event.simultaneous_order = Some(contact_batch.order);
    contact_event.phase_before = Some(MeleeTimelinePhase::Windup);
    contact_event.phase_after = Some(MeleeTimelinePhase::Recovery);
    contact_event.engagement_distance_before_metres =
        Some(attackers[attacker_index].melee_engagement_distance_metres);
    contact_event.engagement_distance_after_metres =
        contact_event.engagement_distance_before_metres;
    contact_event.readiness_before_seconds = Some(attack_timing.recovery_until_seconds);
    contact_event.readiness_after_seconds = Some(attack_timing.recovery_until_seconds);
    recorder.record_timeline(contact_event);
}

fn phase_adaptation_delay(
    defender_phase: MeleeDefenderPhase,
    incoming: MeleeAttackTiming,
    defender: &Combatant,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    let phase_gap_seconds = match defender_phase {
        MeleeDefenderPhase::CommittedAttack(timing) => {
            (timing.contact_at_seconds - incoming.contact_at_seconds).max(0.0)
        }
        MeleeDefenderPhase::NeutralGuard | MeleeDefenderPhase::OccupiedRecovery { .. } => 0.0,
    };
    phase_gap_seconds.min(
        defender
            .equipment
            .melee_weapon
            .map_or(parameters.melee_windup_seconds, |weapon| {
                weapon.attack_interval_seconds
            }),
    )
}

fn timeline_phase(phase: MeleeDefenderPhase) -> MeleeTimelinePhase {
    match phase {
        MeleeDefenderPhase::NeutralGuard => MeleeTimelinePhase::NeutralGuard,
        MeleeDefenderPhase::CommittedAttack(_) => MeleeTimelinePhase::Windup,
        MeleeDefenderPhase::OccupiedRecovery { .. } => MeleeTimelinePhase::Recovery,
    }
}

fn timeline_phase_after_commitment(
    commitment: MeleeDefenseCommitmentKind,
    phase: MeleeDefenderPhase,
) -> MeleeTimelinePhase {
    match commitment {
        MeleeDefenseCommitmentKind::CanceledSameWeapon
        | MeleeDefenseCommitmentKind::DefenseRecovery => MeleeTimelinePhase::Recovery,
        MeleeDefenseCommitmentKind::TransformedOffhand => MeleeTimelinePhase::Windup,
        MeleeDefenseCommitmentKind::None | MeleeDefenseCommitmentKind::NeutralGuardRecovery => {
            timeline_phase(phase)
        }
    }
}

fn response_availability(
    defender: &Combatant,
    response: DefenderResponse,
    phase: MeleeDefenderPhase,
    incoming: MeleeAttackTiming,
) -> MeleeResponseAvailability {
    if matches!(response, DefenderResponse::Dodge { .. }) {
        return MeleeResponseAvailability::DodgeChosen;
    }
    match phase {
        MeleeDefenderPhase::CommittedAttack(timing)
            if timing.started_at_seconds > incoming.started_at_seconds
                && timing.started_at_seconds <= incoming.contact_at_seconds =>
        {
            MeleeResponseAvailability::ReciprocalWindup
        }
        MeleeDefenderPhase::CommittedAttack(_) => {
            MeleeResponseAvailability::OccupiedByEarlierAttack
        }
        MeleeDefenderPhase::OccupiedRecovery { .. } => MeleeResponseAvailability::OccupiedRecovery,
        MeleeDefenderPhase::NeutralGuard
            if defender.equipment.melee_weapon.is_none()
                && defender.equipment.shield_block_bonus <= 0.0 =>
        {
            MeleeResponseAvailability::NoImplement
        }
        MeleeDefenderPhase::NeutralGuard => MeleeResponseAvailability::NeutralGuard,
    }
}

pub(super) fn defender_phase_at_contact(
    defender: &Combatant,
    attacker_id: u64,
    incoming: MeleeAttackTiming,
) -> MeleeDefenderPhase {
    if defender
        .melee_engagement_target
        .is_some_and(|target| target != attacker_id)
    {
        return MeleeDefenderPhase::NeutralGuard;
    }
    if let (Some(started_at_seconds), Some(contact_at_seconds)) = (
        defender.melee_attack_started_at_seconds,
        defender.melee_attack_contact_at_seconds,
    ) {
        let timing = MeleeAttackTiming {
            started_at_seconds,
            contact_at_seconds,
            recovery_until_seconds: defender.melee_recovery_until_seconds,
        };
        return MeleeDefenderPhase::CommittedAttack(timing);
    }
    if defender.melee_recovery_until_seconds > incoming.contact_at_seconds {
        return MeleeDefenderPhase::OccupiedRecovery {
            until_seconds: defender.melee_recovery_until_seconds,
        };
    }
    MeleeDefenderPhase::NeutralGuard
}

fn response_name(response: DefenderResponse) -> &'static str {
    match response {
        DefenderResponse::None => "none",
        DefenderResponse::Block { .. } => "block",
        DefenderResponse::Parry { .. } => "parry",
        DefenderResponse::Dodge { .. } => "dodge",
    }
}

fn response_choice(response: DefenderResponse) -> MeleeResponseChoice {
    match response {
        DefenderResponse::None => MeleeResponseChoice::None,
        DefenderResponse::Block { .. } => MeleeResponseChoice::Block,
        DefenderResponse::Parry { .. } => MeleeResponseChoice::Parry,
        DefenderResponse::Dodge { .. } => MeleeResponseChoice::Dodge,
    }
}

fn autoresolve_armor_layer_chain(
    equipment: &CombatEquipment,
    contact: MeleeContactLocation,
) -> Vec<ArmorLayerTelemetry> {
    let armor = equipment.armor[body_part_index(contact.body_part)];
    let geometry = armor.coverage_geometry;
    let span = geometry
        .map(|geometry| geometry.span)
        .or(armor.coverage_span)
        .unwrap_or_else(|| ArmorCoverageSpan::centered(armor.coverage));
    let intersected = span.contains(contact.surface_coordinate);
    vec![ArmorLayerTelemetry {
        inventory_item_id: armor.inventory_item_id,
        material: armor.material,
        geometry,
        intersected,
        selected: intersected,
    }]
}

fn charge_defensive_work(defender: &mut Combatant, response: DefenderResponse) {
    match response {
        DefenderResponse::None => {}
        DefenderResponse::Block { .. } | DefenderResponse::Parry { .. } => {
            defender.charge_action_work(CombatActionWork::WeaponDefense, 0.5)
        }
        DefenderResponse::Dodge { .. } => {
            defender.charge_action_work(CombatActionWork::ExplosiveDodge, 0.5)
        }
    }
}

pub(super) fn commit_defensive_action(
    defender: &mut Combatant,
    attempted_response: DefenderResponse,
    effective_response: DefenderResponse,
    defender_phase: MeleeDefenderPhase,
) -> DefenseCommitment {
    let defense_seconds = match attempted_response {
        DefenderResponse::None => return DefenseCommitment::NONE,
        DefenderResponse::Block { .. } | DefenderResponse::Parry { .. } => 0.5,
        DefenderResponse::Dodge { .. } => 0.5,
    };
    if let DefenderResponse::Block { effectiveness } = effective_response
        && defender.equipment.shield_block_bonus > 0.0
        && matches!(defender_phase, MeleeDefenderPhase::CommittedAttack(_))
    {
        // An off-hand shield can intercept during an already prepared weapon
        // attack. It preserves the attack's progress but the bound posture
        // reduces the next contact's power, matching tactical authority.
        defender.melee_attack_power_multiplier *=
            (1.0 - 0.4 * effectiveness.clamp(0.0, 1.0)).clamp(0.2, 1.0);
        return DefenseCommitment {
            kind: MeleeDefenseCommitmentKind::TransformedOffhand,
            retained_power: Some(defender.melee_attack_power_multiplier),
            recovery_seconds_after_contact: 0.0,
        };
    }
    if matches!(defender_phase, MeleeDefenderPhase::NeutralGuard) {
        return DefenseCommitment {
            kind: MeleeDefenseCommitmentKind::NeutralGuardRecovery,
            retained_power: None,
            recovery_seconds_after_contact: if matches!(
                attempted_response,
                DefenderResponse::Dodge { .. }
            ) {
                defense_seconds
            } else {
                0.0
            },
        };
    }
    let canceled = attempted_response.is_weapon_contact()
        && matches!(defender_phase, MeleeDefenderPhase::CommittedAttack(_));
    DefenseCommitment {
        kind: if canceled {
            MeleeDefenseCommitmentKind::CanceledSameWeapon
        } else {
            MeleeDefenseCommitmentKind::DefenseRecovery
        },
        retained_power: None,
        recovery_seconds_after_contact: defense_seconds,
    }
}

#[derive(Clone, Copy)]
pub(super) struct DefenseCommitment {
    pub kind: MeleeDefenseCommitmentKind,
    pub retained_power: Option<f32>,
    recovery_seconds_after_contact: f32,
}

impl DefenseCommitment {
    const NONE: Self = Self {
        kind: MeleeDefenseCommitmentKind::None,
        retained_power: None,
        recovery_seconds_after_contact: 0.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MeleeDefenseCommitmentKind {
    None,
    NeutralGuardRecovery,
    CanceledSameWeapon,
    TransformedOffhand,
    DefenseRecovery,
}

impl MeleeDefenseCommitmentKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NeutralGuardRecovery => "neutral_guard_recovery",
            Self::CanceledSameWeapon => "canceled_for_same_weapon_defense",
            Self::TransformedOffhand => "transformed_by_offhand_defense",
            Self::DefenseRecovery => "defense_recovery",
        }
    }
}

pub(super) fn melee_assignment(
    attacker_index: usize,
    attackers: &[Combatant],
    defenders: &[Combatant],
    parameters: crate::combat::AutoresolveParameters,
) -> (usize, f32) {
    let mut ordered_defenders = active_melee_indices(defenders);
    ordered_defenders.extend(active_ranged_indices(defenders));
    for index in active_indices(defenders) {
        if !ordered_defenders.contains(&index) {
            ordered_defenders.push(index);
        }
    }
    debug_assert!(!ordered_defenders.is_empty());
    let melee_rank = attackers[..=attacker_index]
        .iter()
        .filter(|combatant| {
            !combatant.is_defeated()
                && combatant.can_attack_melee()
                && preferred_attack_mode(combatant) == AttackMode::Melee
        })
        .count()
        .saturating_sub(1);
    let target = ordered_defenders[melee_rank % ordered_defenders.len()];
    let flanking = if melee_rank >= ordered_defenders.len() {
        parameters.outnumbered_flanking
    } else {
        0.0
    };
    (target, flanking)
}

pub(super) fn active_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| (!combatant.is_defeated()).then_some(index))
        .collect()
}

pub(super) fn active_melee_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| {
            (!combatant.is_defeated()
                && combatant.can_attack_melee()
                && preferred_attack_mode(combatant) == AttackMode::Melee)
                .then_some(index)
        })
        .collect()
}

pub(super) fn active_ranged_indices(side: &[Combatant]) -> Vec<usize> {
    side.iter()
        .enumerate()
        .filter_map(|(index, combatant)| {
            (!combatant.is_defeated() && combatant.can_attack_ranged()).then_some(index)
        })
        .collect()
}

pub(super) fn prioritized_ranged_targets(side: &[Combatant]) -> Vec<usize> {
    let ranged = active_ranged_indices(side);
    if ranged.is_empty() {
        active_indices(side)
    } else {
        ranged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed_fighter(id: u64, reach: f32, distance: f32) -> Combatant {
        let mut fighter = Combatant::new(id);
        fighter.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: reach,
            ..CombatWeapon::default()
        });
        fighter.equipment.weapon = fighter.equipment.melee_weapon;
        fighter.melee_attack_started_at_seconds = Some(0.0);
        fighter.melee_attack_contact_at_seconds = Some(1.0);
        fighter.melee_engagement_distance_metres = distance;
        fighter.melee_separation_velocity_metres_per_second = -1.0;
        fighter
    }

    #[test]
    fn exact_reach_boundary_is_attackable_but_outside_measure_closes() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let mut fighter = Combatant::new(1);
        fighter.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 1.0,
            ..CombatWeapon::default()
        });
        fighter.equipment.weapon = fighter.equipment.melee_weapon;
        assert_eq!(
            movement_intent(&fighter, 1.0, parameters),
            MovementIntent::Hold
        );
        assert_eq!(
            movement_intent(&fighter, 1.01, parameters),
            MovementIntent::Close
        );
    }

    #[test]
    fn committed_short_weapon_tracks_while_long_weapon_seeks_measure() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let mut short = Combatant::new(1);
        short.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 1.0,
            ..CombatWeapon::default()
        });
        short.equipment.weapon = short.equipment.melee_weapon;
        short.melee_attack_started_at_seconds = Some(0.0);
        let mut long = Combatant::new(2);
        long.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 2.0,
            ..CombatWeapon::default()
        });
        long.equipment.weapon = long.equipment.melee_weapon;
        assert_eq!(
            movement_intent(&short, 0.9, parameters),
            MovementIntent::Close
        );
        assert_eq!(
            movement_intent(&long, 0.9, parameters),
            MovementIntent::Retreat
        );
    }

    #[test]
    fn distal_headed_weapon_seeks_the_center_of_its_authored_head_band() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let mut polearm = Combatant::new(2);
        polearm.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            melee_reach: 2.0,
            grip_to_tip_m: 1.9,
            total_length_m: 2.2,
            striking_head_length_m: 0.16,
            distal_headed: true,
            ..CombatWeapon::default()
        });
        polearm.equipment.weapon = polearm.equipment.melee_weapon;
        let preferred = preferred_melee_measure(&polearm, parameters);
        assert!((preferred - 1.92).abs() < 1.0e-6);
        assert_eq!(
            movement_intent(&polearm, 1.8, parameters),
            MovementIntent::Retreat
        );
        assert_eq!(
            movement_intent(&polearm, preferred, parameters),
            MovementIntent::Hold
        );
    }

    #[test]
    fn movement_and_recovery_gate_one_exact_attack_start() {
        let whole = available_attack_start(0.0, 1.0, 0.3, 0.4);
        let first_half = available_attack_start(0.0, 0.5, 0.3, 0.4);
        assert_eq!(whole, Some(0.4));
        assert_eq!(first_half, whole);
    }

    #[test]
    fn attack_start_waits_for_later_of_measure_and_recovery() {
        assert_eq!(available_attack_start(0.0, 0.5, 0.6, 0.4), None);
        assert_eq!(available_attack_start(0.5, 1.0, 0.6, 0.8), Some(0.8));
    }

    #[test]
    fn swept_entry_finds_first_reach_crossing_without_tunneling() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let first = committed_fighter(1, 1.25, 1.27);
        let mut second = Combatant::new(2);
        second.melee_engagement_distance_metres = 1.27;
        second.melee_separation_velocity_metres_per_second = -1.0;
        let crossing = swept_entry_seconds(&first, &second, true, 0.5, parameters)
            .expect("mutual closure should enter sword reach");
        let (before, _, _) =
            preview_melee_pair_movement(&first, &second, (crossing - 0.000_1).max(0.0), parameters);
        let (at, _, _) = preview_melee_pair_movement(&first, &second, crossing, parameters);
        assert!(before.distance_after_metres > 1.25);
        assert!(at.distance_after_metres <= 1.25 + 1.0e-5);
    }

    #[test]
    fn swept_entry_is_dt_invariant_and_side_symmetric() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let first = committed_fighter(1, 1.25, 1.27);
        let mut second = Combatant::new(2);
        second.melee_engagement_distance_metres = 1.27;
        second.melee_separation_velocity_metres_per_second = -1.0;
        let short = swept_entry_seconds(&first, &second, true, 0.3, parameters)
            .expect("crossing occurs in the shorter interval");
        let long = swept_entry_seconds(&first, &second, true, 0.6, parameters)
            .expect("crossing occurs in the longer interval");
        let swapped = swept_entry_seconds(&second, &first, false, 0.6, parameters)
            .expect("actor identity swap preserves the same physical crossing");
        assert!((short - long).abs() < 1.0e-5);
        assert!((long - swapped).abs() < 1.0e-5);
    }

    #[test]
    fn retreating_path_does_not_fabricate_a_swept_entry() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let mut first = committed_fighter(1, 1.25, 1.5);
        let mut second = Combatant::new(2);
        second.melee_engagement_distance_metres = 1.5;
        first.melee_separation_velocity_metres_per_second = 2.0;
        second.melee_separation_velocity_metres_per_second = 2.0;
        assert_eq!(
            swept_entry_seconds(&first, &second, true, 0.01, parameters),
            None
        );
    }

    #[test]
    fn simultaneous_equal_reach_entries_have_the_same_contact_time() {
        let parameters = crate::combat::EMBEDDED_AUTORESOLVE_PARAMETERS;
        let first = committed_fighter(1, 1.25, 1.27);
        let second = committed_fighter(2, 1.25, 1.27);
        let first_entry = swept_entry_seconds(&first, &second, true, 0.5, parameters).unwrap();
        let second_entry = swept_entry_seconds(&first, &second, false, 0.5, parameters).unwrap();
        assert!((first_entry - second_entry).abs() < 1.0e-6);
    }
}
