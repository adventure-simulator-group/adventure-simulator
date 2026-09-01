use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AutoresolveMeleeDefenderDecision {
    pub(super) response: DefenderResponse,
    pub(super) committed: Option<CommittedThreatDecision>,
}

impl AutoresolveMeleeDefenderDecision {
    const fn response(response: DefenderResponse) -> Self {
        Self {
            response,
            committed: None,
        }
    }
}

pub(super) fn autoresolve_melee_defender_response(
    defender: &Combatant,
    reaction_sample: f32,
    reaction_timing_sample: f32,
    commitment_sample: f32,
    incoming: ScheduledMeleeTiming,
    defender_phase: MeleeDefenderPhase,
    parameters: crate::combat::AutoresolveParameters,
) -> AutoresolveMeleeDefenderDecision {
    let can_block =
        defender.equipment.melee_weapon.is_some() || defender.equipment.shield_block_bonus > 0.0;
    if reaction_sample < parameters.melee_dodge_reaction_chance {
        return AutoresolveMeleeDefenderDecision::response(DefenderResponse::Dodge {
            input_reflex: autoresolve_melee_input_reflex(reaction_timing_sample, parameters),
        });
    }
    if can_block
        && let MeleeDefenderPhase::CommittedAttack(defender_attack) = defender_phase
        && let Some(decision) = committed_attack_response(
            defender,
            defender_attack,
            incoming,
            commitment_sample,
            parameters,
        )
    {
        return decision;
    }
    if let MeleeDefenderPhase::OccupiedRecovery { until_seconds } = defender_phase
        && until_seconds > incoming.contact_at_seconds
    {
        return recovering_response(can_block, until_seconds, incoming, parameters);
    }
    AutoresolveMeleeDefenderDecision::response(if can_block {
        DefenderResponse::Block { effectiveness: 1.0 }
    } else {
        DefenderResponse::None
    })
}

fn committed_attack_response(
    defender: &Combatant,
    defender_attack: ScheduledMeleeTiming,
    incoming: ScheduledMeleeTiming,
    commitment_sample: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> Option<AutoresolveMeleeDefenderDecision> {
    let response_delay = defender_attack.started_at_seconds - incoming.started_at_seconds;
    let intercept_window = defender_attack.started_at_seconds > incoming.started_at_seconds
        && defender_attack.started_at_seconds <= incoming.contact_at_seconds
        && response_delay <= parameters.melee_reflex_window_seconds;
    if !intercept_window {
        return (defender_attack.started_at_seconds <= incoming.contact_at_seconds
            && defender_attack.recovery_until_seconds > incoming.contact_at_seconds)
            .then(|| AutoresolveMeleeDefenderDecision::response(DefenderResponse::None));
    }
    let weapon = defender.equipment.melee_weapon.unwrap_or_default();
    let equipment = defender.equipment.for_melee();
    let skill = weapon.skills.weighted_check(|skill| {
        defender.skills.skill_check_by_parts(
            skill,
            &defender.attributes,
            &defender.body,
            &defender.essentials,
            &equipment,
            LimbWeights::all_equal(),
        )
    });
    let timing = (1.0 - response_delay / parameters.melee_reflex_window_seconds).clamp(0.0, 1.0);
    let engagement = (timing
        * ((skill + defender.attributes.instinct) / 10.0).clamp(0.0, 1.0)
        * defender.fatigue_performance())
    .clamp(0.0, 1.0);
    let committed = choose_committed_threat_response(CommittedThreatFacts {
        own_contact_after_incoming_seconds: defender_attack.contact_at_seconds
            - incoming.contact_at_seconds,
        own_windup_seconds: defender_attack.contact_at_seconds - defender_attack.started_at_seconds,
        expected_intercept_engagement: engagement,
        incapacitation: defender.incapacitation(),
        weapon_moment_of_inertia_kg_m2: weapon.moment_of_inertia_kg_m2,
        weapon_recovery_seconds: (weapon.attack_interval_seconds - parameters.melee_windup_seconds)
            .max(0.0),
        consecutive_intercepts: defender.melee_consecutive_intercepts,
        decision_sample: commitment_sample,
    });
    let response = if committed.choice == CommittedThreatChoice::FinishTrade {
        DefenderResponse::None
    } else {
        reciprocal_intercept_response(
            timing,
            parameters.maximum_hit_precision,
            defender.equipment.shield_block_bonus,
        )
    };
    Some(AutoresolveMeleeDefenderDecision {
        response,
        committed: Some(committed),
    })
}

fn recovering_response(
    can_block: bool,
    until_seconds: f32,
    incoming: ScheduledMeleeTiming,
    parameters: crate::combat::AutoresolveParameters,
) -> AutoresolveMeleeDefenderDecision {
    let effectiveness = recovery_guard_effectiveness(
        until_seconds,
        incoming.contact_at_seconds,
        parameters.melee_windup_seconds,
    );
    if can_block && effectiveness > 0.0 {
        AutoresolveMeleeDefenderDecision::response(DefenderResponse::Block { effectiveness })
    } else {
        AutoresolveMeleeDefenderDecision::response(DefenderResponse::None)
    }
}

pub(super) fn recovery_guard_effectiveness(recovery: f32, contact: f32, windup: f32) -> f32 {
    (1.0 - (recovery - contact).max(0.0) / windup.max(f32::EPSILON)).clamp(0.0, 1.0)
}

pub(super) fn autoresolve_melee_input_reflex(
    timing_sample: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> f32 {
    autoresolve_melee_reaction_timing(timing_sample, parameters).input_reflex
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ScheduledMeleeTiming {
    pub started_at_seconds: f32,
    pub contact_at_seconds: f32,
    pub recovery_until_seconds: f32,
}

impl ScheduledMeleeTiming {
    pub(super) fn attack_id(self, attacker_id: u64) -> u64 {
        attacker_id.wrapping_shl(32) ^ MeleeTimelineEvent::tick_at(self.started_at_seconds)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MeleeDefenderPhase {
    NeutralGuard,
    CommittedAttack(ScheduledMeleeTiming),
    OccupiedRecovery { until_seconds: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AutoresolveMeleeReactionTiming {
    pub(super) input_reflex: f32,
    pub(super) displacement_time_seconds: f32,
}

pub(super) fn autoresolve_melee_reaction_timing(
    timing_sample: f32,
    parameters: crate::combat::AutoresolveParameters,
) -> AutoresolveMeleeReactionTiming {
    let windup = parameters.melee_windup_seconds;
    let reaction_delay = parameters.melee_reaction_delay_min_seconds
        + (parameters.melee_reaction_delay_max_seconds
            - parameters.melee_reaction_delay_min_seconds)
            * timing_sample.clamp(0.0, 1.0);
    let elapsed_after_input = (windup - reaction_delay).max(0.0);
    AutoresolveMeleeReactionTiming {
        input_reflex: (1.0 - reaction_delay / parameters.melee_reflex_window_seconds)
            .clamp(parameters.minimum_melee_input_reflex, 1.0),
        displacement_time_seconds: elapsed_after_input,
    }
}
