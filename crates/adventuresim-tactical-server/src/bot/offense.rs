use bevy::ecs::entity::MapEntities;
use bevy::ecs::system::SystemParam;

use super::*;

mod facts;
pub(super) use facts::compare_target;
#[cfg(test)]
pub(super) use facts::ranged_weapon_needs_ammo_lookup;
use facts::{OffensiveFacts, offensive_facts};
mod ranged;
use ranged::{RangedPursuit, drive_ranged_pursuit};
mod tactics;
use tactics::*;

type OffensiveAiQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static Transform,
        &'static TacticalCombatSide,
        &'static mut CharacterLook,
        &'static mut AuthoritativeMovementIntent,
        &'static mut OffensiveCombatAi,
        &'static TacticalCombatState,
        &'static SkeletonState,
        Option<&'static CombatantYielded>,
    ),
>;

type OffensiveCandidateQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static Transform,
        &'static TacticalCombatSide,
        &'static TacticalCombatState,
        &'static SkeletonState,
        Option<&'static CombatantYielded>,
    ),
    With<Player>,
>;

#[derive(SystemParam)]
pub(super) struct OffensiveAiContext<'w, 's> {
    ai: OffensiveAiQuery<'w, 's>,
    colliders: Query<'w, 's, &'static Collider>,
    dimensions: Query<'w, 's, &'static CharacterDimensions>,
    combat_config: Res<'w, TacticalCombatConfig>,
}

/// Enables server-owned offensive control, preferring ranged fire while a
/// usable ranged weapon and arrows are available and otherwise using melee.
#[derive(Component, Debug, Reflect, MapEntities)]
#[reflect(Component)]
pub struct OffensiveCombatAi {
    #[entities]
    target: Option<Entity>,
    phase: OffensiveCombatPhase,
}

impl Default for OffensiveCombatAi {
    fn default() -> Self {
        Self {
            target: None,
            phase: OffensiveCombatPhase::Pursuing,
        }
    }
}

#[cfg(test)]
impl OffensiveCombatAi {
    pub(super) fn target(&self) -> Option<Entity> {
        self.target
    }

    pub(super) fn is_assessing(&self) -> bool {
        matches!(self.phase, OffensiveCombatPhase::Assessing(_))
    }

    pub(super) fn guard_committed_threat_for_test(&mut self, target: Entity) {
        self.target = Some(target);
        self.phase = OffensiveCombatPhase::GuardingCommittedThreat;
    }
}

#[derive(Debug, Reflect)]
enum OffensiveCombatPhase {
    Assessing(Timer),
    GuardingCommittedThreat,
    Pursuing,
    MeleeWindup {
        timer: Timer,
        strike_family: StrikeFamily,
        recovery_seconds: f32,
    },
    RangedWindup(Timer),
    Cooldown(Timer),
    WithdrawingUnableToContinue(Timer),
}

#[expect(
    clippy::too_many_arguments,
    reason = "phase execution needs actor, target, authored config, and mutable bot controls"
)]
fn drive_pursuit_phase(
    cmd: &mut Commands,
    entity: Entity,
    target: Entity,
    distance: f32,
    input: &mut AuthoritativeMovementIntent,
    controller: &mut OffensiveCombatAi,
    skeleton: &SkeletonState,
    target_skeleton: &SkeletonState,
    facts: &OffensiveFacts,
    config: &AiOffenseConfig,
    random: &mut CombatRandom,
) {
    if facts.use_ranged {
        drive_ranged_pursuit(
            cmd,
            RangedPursuit {
                entity,
                target,
                distance,
            },
            input,
            controller,
            facts,
            config,
        );
    } else if facts.weapon_is_melee
        && facts.melee_lunge_delay.is_some()
        && target_skeleton.action_kind() == SkeletonAction::Attack
        && target_skeleton.action_phase() < 0.5
        && random.unit_f32()
            < committed_threat_recognition_probability(
                target_skeleton.action_phase(),
                facts.instinct,
            )
    {
        // Do not knowingly begin a fresh committed strike into an attack that
        // is already on its way. Hold the guard through the threat, then use a
        // seeded perception/initiative delay before looking for a counter.
        // Existing attacks may still overlap; this only prevents deterministic
        // cadence from repeatedly assigning initiative to the same actor.
        input.0 = None;
        controller.phase = OffensiveCombatPhase::GuardingCommittedThreat;
    } else if facts.weapon_is_melee
        && below_preferred_long_weapon_measure(
            facts.weapon_reach,
            facts.preferred_melee_measure,
            distance,
            config.long_weapon_measure_threshold_metres,
        )
    {
        input.0 = Some(-Vec2::Y);
    } else if facts.weapon_is_melee
        && facts.melee_attack_available
        && facts.dimensions.arm_reach_metres > 0.0
        && facts.melee_lunge_delay.is_some()
    {
        input.0 = None;
        let Some(strike_family) = skeleton.available_strike_family(facts.strike_family) else {
            return;
        };
        cmd.trigger(MeleeAttackStartedIntent {
            attacker: entity,
            target: Some(target),
            windup: CombatDuration::from_secs_f32(config.windup_seconds),
            reported_precision: ReportedPrecision::new(config.hit_precision)
                .expect("AI precision is finite"),
            strike_family,
            hand: AttackHand::Main,
        });
        controller.phase = OffensiveCombatPhase::MeleeWindup {
            timer: Timer::from_seconds(
                config
                    .windup_seconds
                    .max(facts.melee_lunge_delay.unwrap_or_default()),
                TimerMode::Once,
            ),
            strike_family,
            recovery_seconds: facts.melee_recovery_seconds,
        };
    } else if facts.weapon_is_melee && !facts.melee_attack_available {
        input.0 = Some(-Vec2::Y);
        controller.phase = OffensiveCombatPhase::WithdrawingUnableToContinue(Timer::from_seconds(
            1.0,
            TimerMode::Once,
        ));
        cmd.trigger(BotContinuationDecisionEvent {
            combatant: entity,
            decision: BotContinuationDecision::Withdraw,
        });
    } else {
        input.0 = Some(Vec2::Y);
    }
}

fn maintain_committed_threat_guard(
    input: &mut AuthoritativeMovementIntent,
    controller: &mut OffensiveCombatAi,
    target_skeleton: &SkeletonState,
    random: &mut CombatRandom,
    facts: &OffensiveFacts,
    config: &AiOffenseConfig,
) {
    input.0 = None;
    if target_skeleton.action_kind() != SkeletonAction::Attack
        || target_skeleton.action_phase() >= 0.5
    {
        controller.phase = OffensiveCombatPhase::Assessing(Timer::from_seconds(
            initiative_delay_seconds(random, facts.instinct, config),
            TimerMode::Once,
        ));
    }
}

fn hold_and_tick(input: &mut AuthoritativeMovementIntent, timer: &mut Timer, time: &Time<()>) {
    input.0 = None;
    timer.tick(time.delta());
}

fn continue_withdrawal(
    cmd: &mut Commands,
    time: &Time<()>,
    entity: Entity,
    input: &mut AuthoritativeMovementIntent,
    timer: &mut Timer,
) {
    input.0 = Some(-Vec2::Y);
    timer.tick(time.delta());
    if timer.is_finished() {
        input.0 = None;
        cmd.entity(entity).insert(CombatantYielded);
        cmd.trigger(BotContinuationDecisionEvent {
            combatant: entity,
            decision: BotContinuationDecision::Yield,
        });
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "phase execution needs actor, target, authored config, and mutable bot controls"
)]
fn drive_offensive_phase(
    cmd: &mut Commands,
    time: &Time<()>,
    entity: Entity,
    target: Entity,
    distance: f32,
    input: &mut AuthoritativeMovementIntent,
    controller: &mut OffensiveCombatAi,
    skeleton: &SkeletonState,
    target_skeleton: &SkeletonState,
    facts: &OffensiveFacts,
    config: &AiOffenseConfig,
    random: &mut CombatRandom,
) {
    if matches!(controller.phase, OffensiveCombatPhase::Pursuing) {
        drive_pursuit_phase(
            cmd,
            entity,
            target,
            distance,
            input,
            controller,
            skeleton,
            target_skeleton,
            facts,
            config,
            random,
        );
        return;
    }
    match &mut controller.phase {
        OffensiveCombatPhase::Pursuing => unreachable!("pursuit was handled above"),
        OffensiveCombatPhase::GuardingCommittedThreat => {
            maintain_committed_threat_guard(
                input,
                controller,
                target_skeleton,
                random,
                facts,
                config,
            );
        }
        OffensiveCombatPhase::Assessing(timer) => {
            hold_and_tick(input, timer, time);
            if timer.is_finished() {
                controller.phase = OffensiveCombatPhase::Pursuing;
            }
        }
        OffensiveCombatPhase::MeleeWindup {
            timer,
            strike_family: _,
            recovery_seconds,
        } => {
            input.0 = long_weapon_windup_movement(
                facts.weapon_reach,
                facts.preferred_melee_measure,
                distance,
                config.long_weapon_measure_threshold_metres,
            );
            timer.tick(time.delta());
            if timer.is_finished() {
                controller.phase = OffensiveCombatPhase::Cooldown(Timer::from_seconds(
                    *recovery_seconds + random.range_f32(0.0, config.cadence_jitter_seconds),
                    TimerMode::Once,
                ));
            }
        }
        OffensiveCombatPhase::RangedWindup(timer) => {
            input.0 = None;
            timer.tick(time.delta());
            if timer.is_finished() {
                cmd.trigger(RangedAttackIntent {
                    attacker: entity,
                    target: Some(target),
                    body_part: config.target_body_part,
                    reported_precision: ReportedPrecision::new(config.hit_precision)
                        .expect("AI precision is finite"),
                });
                controller.phase = OffensiveCombatPhase::Cooldown(Timer::from_seconds(
                    config.cooldown_seconds + random.range_f32(0.0, config.cadence_jitter_seconds),
                    TimerMode::Once,
                ));
            }
        }
        OffensiveCombatPhase::Cooldown(timer) => {
            input.0 = None;
            timer.tick(time.delta());
            if timer.is_finished() {
                controller.phase = OffensiveCombatPhase::Pursuing;
            }
        }
        OffensiveCombatPhase::WithdrawingUnableToContinue(timer) => {
            continue_withdrawal(cmd, time, entity, input, timer);
        }
    }
}

pub(super) fn drive_offensive_combat_ai(
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
    candidates: OffensiveCandidateQuery<'_, '_>,
    context: OffensiveAiContext<'_, '_>,
    mut random: ResMut<CombatRandom>,
) {
    let OffensiveAiContext {
        mut ai,
        colliders,
        dimensions,
        combat_config,
    } = context;
    let config = &combat_config.ai.ordinary.offense;
    for (entity, transform, side, mut look, mut input, mut controller, state, skeleton, yielded) in
        &mut ai
    {
        if state.is_incapacitated() || yielded.is_some() {
            input.0 = None;
            continue;
        }
        let target = candidates
            .iter()
            .filter(
                |(candidate, _, candidate_side, candidate_state, _, candidate_yielded)| {
                    *candidate != entity
                        && **candidate_side != *side
                        && !candidate_state.is_incapacitated()
                        && candidate_yielded.is_none()
                },
            )
            .min_by(
                |(a, a_transform, _, _, _, _), (b, b_transform, _, _, _, _)| {
                    compare_target(transform, a_transform, *a, b_transform, *b)
                },
            )
            .map(|(target, _, _, _, _, _)| target);

        if target != controller.target {
            controller.target = target;
            let instinct = viewer.get(entity).map_or(3.0, |view| {
                view.raw_single_body_part_attr(SimpleAttribute::Instinct)
            });
            controller.phase = OffensiveCombatPhase::Assessing(Timer::from_seconds(
                initiative_delay_seconds(&mut random, instinct, config),
                TimerMode::Once,
            ));
        }
        let Some(target) = target else {
            input.0 = None;
            continue;
        };
        let Ok((_, target_transform, _, _, target_skeleton, _)) = candidates.get(target) else {
            continue;
        };
        let offset = target_transform.translation.xz() - transform.translation.xz();
        let distance = offset.length();
        if distance > f32::EPSILON {
            look.yaw = (-offset.x).atan2(-offset.y);
        }
        let OffensiveFacts {
            weapon_reach,
            preferred_melee_measure,
            weapon_is_melee,
            use_ranged,
            strike_family,
            melee_attack_available,
            melee_recovery_seconds,
            dimensions,
            melee_lunge_delay,
            instinct,
        } = offensive_facts(
            entity,
            target,
            transform,
            target_transform,
            state,
            &viewer,
            &dimensions,
            &colliders,
            &combat_config,
            config,
        );
        let melee_target_reachable = melee_lunge_delay.is_some();

        let abort_windup = matches!(
            &controller.phase,
            OffensiveCombatPhase::MeleeWindup { .. }
                if !weapon_is_melee
                    || !melee_attack_available
                    || dimensions.arm_reach_metres <= 0.0
                    || !melee_target_reachable
        ) || matches!(
            &controller.phase,
            OffensiveCombatPhase::RangedWindup(_) if !use_ranged || distance > weapon_reach
        );
        if abort_windup {
            controller.phase = OffensiveCombatPhase::Pursuing;
        }
        if matches!(
            controller.phase,
            OffensiveCombatPhase::WithdrawingUnableToContinue(_)
        ) && melee_attack_available
        {
            controller.phase = OffensiveCombatPhase::Pursuing;
        }

        drive_offensive_phase(
            &mut cmd,
            &time,
            entity,
            target,
            distance,
            &mut input,
            &mut controller,
            skeleton,
            target_skeleton,
            &OffensiveFacts {
                weapon_reach,
                preferred_melee_measure,
                weapon_is_melee,
                use_ranged,
                strike_family,
                melee_attack_available,
                melee_recovery_seconds,
                dimensions,
                melee_lunge_delay,
                instinct,
            },
            config,
            &mut random,
        );
    }
}

pub(super) fn on_attack_committed_to_defense(
    event: On<crate::combat::MeleeAttackCommittedToDefense>,
    viewer: TacticalPlayerViewer,
    config: Res<TacticalCombatConfig>,
    mut random: ResMut<CombatRandom>,
    mut controllers: Query<&mut OffensiveCombatAi>,
) {
    let Ok(mut controller) = controllers.get_mut(event.defender) else {
        return;
    };
    let instinct = viewer.get(event.defender).map_or(3.0, |view| {
        view.raw_single_body_part_attr(SimpleAttribute::Instinct)
    });
    controller.phase = OffensiveCombatPhase::Assessing(Timer::from_seconds(
        initiative_delay_seconds(&mut random, instinct, &config.ai.ordinary.offense),
        TimerMode::Once,
    ));
}

#[cfg(test)]
mod tests {
    use super::{below_preferred_long_weapon_measure, committed_threat_recognition_probability};

    #[test]
    fn threat_recognition_grows_with_visible_commitment_and_instinct() {
        let early = committed_threat_recognition_probability(0.05, 3.0);
        let late = committed_threat_recognition_probability(0.4, 3.0);
        let expert = committed_threat_recognition_probability(0.4, 5.0);
        assert!(early < late);
        assert!(late < expert);
        assert!((0.0..=1.0).contains(&early));
        assert!((0.0..=1.0).contains(&expert));
    }

    #[test]
    fn polearm_retreats_below_authored_head_band_center_not_generic_seventy_percent() {
        let preferred =
            adventuresim_core::combat::preferred_melee_striking_measure(2.0, 1.9, 0.16, true, 0.7);
        assert!((preferred - 1.92).abs() < 1.0e-6);
        assert!(below_preferred_long_weapon_measure(
            2.0, preferred, 2.6, 1.2
        ));
        assert!(!below_preferred_long_weapon_measure(
            2.0, preferred, 2.8, 1.2
        ));
    }
}
