use super::*;

pub(in crate::autoresolve) fn schedule_side_melee_attacks(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<ScheduledMeleeAttack> {
    let start = round.saturating_sub(1) as f32 * parameters.combat_round_seconds;
    schedule_side_melee_attacks_in_window(
        attackers,
        defenders,
        start,
        parameters.combat_round_seconds,
        random,
        recorder,
        parameters,
    )
}

pub(in crate::autoresolve) fn schedule_side_melee_attacks_in_window(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    window_start: f32,
    window_seconds: f32,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<ScheduledMeleeAttack> {
    let mut scheduled = Vec::new();
    for index in 0..attackers.len() {
        if let Some(attack) = schedule_attacker(
            index,
            attackers,
            defenders,
            window_start,
            window_seconds,
            random,
            recorder,
            parameters,
        ) {
            scheduled.push(attack);
        }
    }
    scheduled
}

#[expect(
    clippy::too_many_arguments,
    reason = "one actor scheduling joins both sides and deterministic state"
)]
fn schedule_attacker(
    index: usize,
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    window_start: f32,
    window_seconds: f32,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) -> Option<ScheduledMeleeAttack> {
    if attackers[index].is_defeated()
        || side_defeated(defenders)
        || preferred_attack_mode(&attackers[index]) != AttackMode::Melee
    {
        return None;
    }
    if !attackers[index].can_attack_melee() {
        attackers[index].yielded = true;
        return None;
    }
    let (target_index, flanking) = melee_assignment(index, attackers, defenders, parameters);
    let window_end = window_start + window_seconds;
    if let (Some(started_at_seconds), Some(contact_at_seconds)) = (
        attackers[index].melee_attack_started_at_seconds,
        attackers[index].melee_attack_contact_at_seconds,
    ) {
        return (contact_at_seconds <= window_end).then(|| ScheduledMeleeAttack {
            attacker_index: index,
            target_index,
            flanking,
            attack_timing: ScheduledMeleeTiming {
                started_at_seconds,
                contact_at_seconds,
                recovery_until_seconds: attackers[index].melee_recovery_until_seconds,
            },
        });
    }
    let (attacker_id, target_id, distance) =
        establish_engagement(index, target_index, attackers, defenders, parameters);
    let reach = melee_effective_reach(&attackers[index]);
    if distance > reach + parameters.melee_lunge_maximum_travel_metres {
        return None;
    }
    let readiness = attackers[index].melee_recovery_until_seconds
        + attackers[index].melee_phase_adaptation_delay_seconds;
    let started = available_attack_start(window_start, window_end, window_start, readiness)?;
    let interval = attack_interval(&attackers[index], parameters);
    let timing = ScheduledMeleeTiming {
        started_at_seconds: started,
        contact_at_seconds: started + parameters.melee_windup_seconds,
        recovery_until_seconds: started + interval,
    };
    let adaptation = attackers[index].melee_phase_adaptation_delay_seconds;
    attackers[index].melee_interval_jitter_seconds =
        random.unit_f32() * parameters.melee_cadence_jitter_seconds;
    attackers[index].melee_attack_started_at_seconds = Some(timing.started_at_seconds);
    attackers[index].melee_attack_contact_at_seconds = Some(timing.contact_at_seconds);
    attackers[index].melee_attack_scheduled_measure_metres = Some(distance);
    attackers[index].melee_recovery_until_seconds = timing.recovery_until_seconds;
    attackers[index].melee_phase_adaptation_delay_seconds = 0.0;
    record_attack_start(
        recorder,
        AttackStartTrace {
            attacker_id,
            target_id,
            distance,
            readiness,
            window_start,
            adaptation,
            timing,
        },
    );
    (timing.contact_at_seconds <= window_end).then_some(ScheduledMeleeAttack {
        attacker_index: index,
        target_index,
        flanking,
        attack_timing: timing,
    })
}

fn attack_interval(attacker: &Combatant, parameters: crate::combat::AutoresolveParameters) -> f32 {
    (attacker
        .equipment
        .melee_weapon
        .map_or(parameters.reference_melee_attack_seconds, |weapon| {
            weapon.attack_interval_seconds
        })
        .max(parameters.minimum_attack_interval_seconds)
        + attacker.melee_interval_jitter_seconds)
        / attacker.fatigue_performance()
}

fn establish_engagement(
    index: usize,
    target: usize,
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    parameters: crate::combat::AutoresolveParameters,
) -> (u64, u64, f32) {
    let attacker_id = attackers[index].id;
    let target_id = defenders[target].id;
    let initial_separation =
        maximum_melee_pair_surface_separation(&attackers[index], &defenders[target], parameters);
    if attackers[index].melee_engagement_target != Some(target_id) {
        attackers[index].melee_engagement_target = Some(target_id);
        attackers[index].melee_engagement_distance_metres = initial_separation;
    }
    if defenders[target].melee_engagement_target != Some(attacker_id) {
        defenders[target].melee_engagement_target = Some(attacker_id);
        defenders[target].melee_engagement_distance_metres = initial_separation;
    }
    let distance = attackers[index]
        .melee_engagement_distance_metres
        .min(defenders[target].melee_engagement_distance_metres)
        .max(0.0);
    (attacker_id, target_id, distance)
}

struct AttackStartTrace {
    attacker_id: u64,
    target_id: u64,
    distance: f32,
    readiness: f32,
    window_start: f32,
    adaptation: f32,
    timing: ScheduledMeleeTiming,
}

fn record_attack_start(recorder: &mut BattleRecorder, trace: AttackStartTrace) {
    let AttackStartTrace {
        attacker_id,
        target_id,
        distance,
        readiness,
        window_start,
        adaptation,
        timing,
    } = trace;
    let mut event =
        MeleeTimelineEvent::at(MeleeTimelineKind::AttackStarted, timing.started_at_seconds);
    event.combatant_id = Some(attacker_id);
    event.target_id = Some(target_id);
    event.engagement_distance_before_metres = Some(distance);
    event.engagement_distance_after_metres = Some(distance);
    event.readiness_before_seconds = Some(readiness);
    event.readiness_after_seconds = Some(timing.recovery_until_seconds);
    event.attack_id = Some(timing.attack_id(attacker_id));
    event.attack_started_tick = Some(MeleeTimelineEvent::tick_at(timing.started_at_seconds));
    event.attack_contact_tick = Some(MeleeTimelineEvent::tick_at(timing.contact_at_seconds));
    event.attack_recovery_tick = Some(MeleeTimelineEvent::tick_at(timing.recovery_until_seconds));
    event.phase_before = Some(if readiness > window_start {
        MeleeTimelinePhase::Recovery
    } else {
        MeleeTimelinePhase::NeutralGuard
    });
    event.phase_after = Some(MeleeTimelinePhase::Windup);
    event.phase_adaptation_delay_seconds = Some(adaptation);
    recorder.record_timeline(event);
}

pub(in crate::autoresolve) fn scheduled_side_contacts_in_window(
    attackers: &[Combatant],
    defenders: &[Combatant],
    start: f32,
    end: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> Vec<ScheduledMeleeAttack> {
    attackers
        .iter()
        .enumerate()
        .filter_map(|(index, attacker)| {
            let started = attacker.melee_attack_started_at_seconds?;
            let contact = attacker.melee_attack_contact_at_seconds?;
            if contact < start || contact > end {
                return None;
            }
            let (assigned, flanking) = melee_assignment(index, attackers, defenders, parameters);
            let target = attacker
                .melee_engagement_target
                .and_then(|id| defenders.iter().position(|candidate| candidate.id == id))
                .unwrap_or(assigned);
            Some(ScheduledMeleeAttack {
                attacker_index: index,
                target_index: target,
                flanking,
                attack_timing: ScheduledMeleeTiming {
                    started_at_seconds: started,
                    contact_at_seconds: contact,
                    recovery_until_seconds: attacker.melee_recovery_until_seconds,
                },
            })
        })
        .collect()
}

pub(in crate::autoresolve) fn take_side_turns(
    attackers: &mut [Combatant],
    defenders: &mut [Combatant],
    round: usize,
    random: &mut SplitMix64,
    recorder: &mut BattleRecorder,
    parameters: crate::combat::AutoresolveParameters,
) {
    for attack in
        schedule_side_melee_attacks(attackers, defenders, round, random, recorder, parameters)
    {
        if !scheduled_attack_is_current(&attackers[attack.attacker_index], attack.attack_timing) {
            continue;
        }
        let id = attack
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
                id,
                members: vec![id],
                order: 0,
            },
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::autoresolve) struct ScheduledMeleeAttack {
    pub(in crate::autoresolve) attacker_index: usize,
    pub(in crate::autoresolve) target_index: usize,
    pub(in crate::autoresolve) flanking: f32,
    pub(in crate::autoresolve) attack_timing: ScheduledMeleeTiming,
}

#[derive(Clone, Debug)]
pub(in crate::autoresolve) struct MeleeContactBatch {
    pub(in crate::autoresolve) id: u64,
    pub(in crate::autoresolve) members: Vec<u64>,
    pub(in crate::autoresolve) order: u32,
}

pub(in crate::autoresolve) fn scheduled_attack_is_current(
    attacker: &Combatant,
    timing: ScheduledMeleeTiming,
) -> bool {
    attacker.melee_attack_started_at_seconds == Some(timing.started_at_seconds)
        && attacker.melee_attack_contact_at_seconds == Some(timing.contact_at_seconds)
}
