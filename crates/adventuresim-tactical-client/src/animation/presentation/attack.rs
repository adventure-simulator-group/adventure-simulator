use super::*;

/// Advance an already-authoritative ordinary attack on render time. Replicated
/// fixed-tick phases can repeat for one or more render frames; copying them
/// directly produces a visible stop followed by a phase jump at contact.
pub(super) fn advance_presented_attack(
    previous: &SkeletonState,
    authoritative: &SkeletonState,
    next: &mut SkeletonState,
    presented: &mut Option<PresentedAttackPhase>,
    source_changed: bool,
    delta_seconds: f32,
) {
    let stale_attack_payload = reconcile_attack_source(previous, authoritative, next);
    let source = next.clone();
    if source.action_kind() != SkeletonAction::Attack {
        if complete_attack_tail(previous, next, presented, delta_seconds) {
            return;
        }
        *presented = None;
        return;
    }
    let Some(start_tick) = source.action_start_tick() else {
        *presented = None;
        return;
    };
    let animation = source.attack_animation().unwrap_or(AttackAnimation::Thrust);
    if !presented.is_some_and(|phase| phase.matches(&source)) {
        *presented = Some(PresentedAttackPhase {
            start_tick,
            animation,
            hand: source.attack_hand(),
            continuation: source.attack_is_continuation(),
            phase: source.action_phase().clamp(0.0, 1.0),
            error_remaining: 0.0,
        });
        return;
    }

    advance_attack_phase(
        previous,
        &source,
        next,
        presented.as_mut().expect("matching attack phase exists"),
        source_changed && !stale_attack_payload,
        delta_seconds,
    );
}

fn reconcile_attack_source(
    previous: &SkeletonState,
    authoritative: &SkeletonState,
    next: &mut SkeletonState,
) -> bool {
    let same_initial_attack = authoritative.action_kind() == SkeletonAction::Attack
        && !authoritative.attack_is_continuation()
        && authoritative.attack_animation() == previous.attack_animation()
        && authoritative.attack_hand() == previous.attack_hand();
    let stale_missing_queue = previous.attack_has_queued_continuation()
        && same_initial_attack
        && !authoritative.attack_has_queued_continuation()
        && previous.action_start_tick() == authoritative.action_start_tick()
        && previous
            .attack_continuation_tick()
            .is_some_and(|transition_tick| {
                authoritative.locomotion_sample_tick
                    <= transition_tick.saturating_add(playback_tuning().maximum_source_gap_ticks)
            });
    let stale_initial_attack = previous.attack_is_continuation()
        && same_initial_attack
        && authoritative.attack_continuation_tick() == previous.action_start_tick();
    let stale_attack_payload = stale_missing_queue || stale_initial_attack;
    if stale_attack_payload {
        *next = previous.clone();
        next.advance_action(authoritative.locomotion_sample_tick);
    } else if authoritative.attack_has_queued_continuation() {
        next.advance_action(authoritative.locomotion_sample_tick);
    }
    stale_attack_payload
}

fn complete_attack_tail(
    previous: &SkeletonState,
    next: &mut SkeletonState,
    presented: &mut Option<PresentedAttackPhase>,
    delta_seconds: f32,
) -> bool {
    let Some(phase) = presented.as_mut() else {
        return false;
    };
    if previous.action_kind() != SkeletonAction::Attack || phase.phase >= 1.0 {
        return false;
    }
    let recovery_ticks = previous.action_recovery_ticks().unwrap_or(1).max(1);
    let delta = presented_attack_phase_delta(previous, phase.phase, delta_seconds, recovery_ticks);
    phase.phase = (phase.phase + delta).min(1.0);
    phase.error_remaining = 0.0;
    *next = previous.clone();
    next.set_presentation_action_phase(phase.phase);
    true
}

fn advance_attack_phase(
    previous: &SkeletonState,
    source: &SkeletonState,
    next: &mut SkeletonState,
    phase: &mut PresentedAttackPhase,
    measure_source: bool,
    delta_seconds: f32,
) {
    let duration_ticks = if phase.phase < 0.5 {
        source.action_preparation_ticks()
    } else {
        source.action_recovery_ticks()
    }
    .unwrap_or(1)
    .max(1);
    let predicted = (phase.phase
        + presented_attack_phase_delta(source, phase.phase, delta_seconds, duration_ticks))
    .min(1.0);
    let presentation_owned_recovery = source.attack_is_continuation() && phase.phase >= 0.5;
    if measure_source && !presentation_owned_recovery {
        let measured_error = source.action_phase().clamp(0.0, 1.0) - predicted;
        let measured_drift = if measured_error.abs() <= presentation_phase_drift_deadband() {
            0.0
        } else {
            measured_error - measured_error.signum() * presentation_phase_drift_deadband()
        };
        phase.error_remaining += (measured_drift - phase.error_remaining)
            * playback_tuning().phase_drift_measurement_blend;
    } else if presentation_owned_recovery {
        phase.error_remaining = 0.0;
    }
    let maximum_correction = presentation_phase_correction_rate_per_second() * delta_seconds;
    let correction = phase
        .error_remaining
        .clamp(-maximum_correction, maximum_correction);
    phase.phase = (predicted + correction).clamp(phase.phase, 1.0);
    phase.error_remaining -= phase.phase - predicted;
    *next = source.clone();
    next.set_presentation_action_phase(phase.phase);
    debug_assert!(
        previous.action_kind() != SkeletonAction::Attack || phase.phase >= previous.action_phase()
    );
}

fn attack_phase_delta(delta_seconds: f32, duration_ticks: u64) -> f32 {
    delta_seconds.max(0.0) * locomotion_sample_hz() * 0.5 / duration_ticks.max(1) as f32
}

fn presented_attack_phase_delta(
    source: &SkeletonState,
    phase: f32,
    delta_seconds: f32,
    duration_ticks: u64,
) -> f32 {
    let recovery_time_scale = if source.attack_is_continuation() && phase >= 0.5 {
        1.0 + source.attack_curve().overshoot
    } else {
        1.0
    };
    attack_phase_delta(delta_seconds, duration_ticks) / recovery_time_scale
}
