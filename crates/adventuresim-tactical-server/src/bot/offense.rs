use bevy::ecs::entity::MapEntities;

use super::*;

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
}

#[derive(Debug, Reflect)]
enum OffensiveCombatPhase {
    Pursuing,
    MeleeWindup {
        timer: Timer,
        strike_family: StrikeFamily,
    },
    RangedWindup(Timer),
    Cooldown(Timer),
}

pub(super) fn ranged_weapon_needs_ammo_lookup(weapon_is_ranged: bool, weapon_reach: f32) -> bool {
    weapon_is_ranged && weapon_reach.is_finite() && weapon_reach > 0.0
}

pub(super) fn compare_target(
    origin: &Transform,
    a_transform: &Transform,
    a: Entity,
    b_transform: &Transform,
    b: Entity,
) -> Ordering {
    let a_distance_squared = origin
        .translation
        .xz()
        .distance_squared(a_transform.translation.xz());
    let b_distance_squared = origin
        .translation
        .xz()
        .distance_squared(b_transform.translation.xz());
    a_distance_squared
        .total_cmp(&b_distance_squared)
        .then_with(|| a.to_bits().cmp(&b.to_bits()))
}

pub(super) fn drive_offensive_combat_ai(
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
    candidates: Query<
        (
            Entity,
            &Transform,
            &TacticalCombatSide,
            &TacticalCombatState,
        ),
        With<Player>,
    >,
    mut ai: Query<(
        Entity,
        &Transform,
        &TacticalCombatSide,
        &mut CharacterLook,
        &mut input::AccumulatedInput,
        &mut OffensiveCombatAi,
        &TacticalCombatState,
        &SkeletonState,
    )>,
    colliders: Query<&Collider>,
    dimensions: Query<&CharacterDimensions>,
    combat_config: Res<TacticalCombatConfig>,
) {
    let config = &combat_config.ai.ordinary.offense;
    for (entity, transform, side, mut look, mut input, mut controller, state, skeleton) in &mut ai {
        if state.is_incapacitated() {
            input.last_movement = None;
            continue;
        }
        let target = candidates
            .iter()
            .filter(|(candidate, _, candidate_side, candidate_state)| {
                *candidate != entity
                    && **candidate_side != *side
                    && !candidate_state.is_incapacitated()
            })
            .min_by(|(a, a_transform, _, _), (b, b_transform, _, _)| {
                compare_target(transform, a_transform, *a, b_transform, *b)
            })
            .map(|(target, _, _, _)| target);

        if target != controller.target {
            controller.target = target;
            controller.phase = OffensiveCombatPhase::Pursuing;
        }
        let Some(target) = target else {
            input.last_movement = None;
            continue;
        };
        let Ok((_, target_transform, _, _)) = candidates.get(target) else {
            continue;
        };

        let offset = target_transform.translation.xz() - transform.translation.xz();
        let distance = offset.length();
        if distance > f32::EPSILON {
            look.yaw = (-offset.x).atan2(-offset.y);
        }
        let (weapon_reach, weapon_is_melee, weapon_is_ranged, strike_family) = viewer
            .get(entity)
            .map(|view| {
                (
                    view.weapon_reach(),
                    view.weapon_is_melee(),
                    view.weapon_is_ranged(),
                    StrikeFamily::from_melee_style(view.weapon_preferred_melee_style()),
                )
            })
            .unwrap_or((0.0, false, false, StrikeFamily::Thrust));
        let has_ammo = ranged_weapon_needs_ammo_lookup(weapon_is_ranged, weapon_reach)
            && viewer.inventory.get(entity).has_item_id(ARROW_ID);
        let use_ranged = weapon_is_ranged && weapon_reach > 0.0 && has_ammo;
        let dimensions = dimensions.get(entity).copied().unwrap_or_default();
        let leg_length = dimensions.leg_length_metres;
        let quickstep_distance =
            quickstep_target_displacement_metres(leg_length, &combat_config.movement.motor);
        let melee_lunge_delay = colliders
            .get(entity)
            .ok()
            .zip(colliders.get(target).ok())
            .and_then(|(attacker_collider, target_collider)| {
                crate::combat::melee_body_part_lunge_delay(
                    transform,
                    attacker_collider,
                    dimensions,
                    target_transform,
                    target_collider,
                    config.target_body_part,
                    weapon_reach,
                    quickstep_distance,
                    &combat_config,
                )
            });
        let melee_target_reachable = melee_lunge_delay.is_some();

        let abort_windup = matches!(
            &controller.phase,
            OffensiveCombatPhase::MeleeWindup { .. }
                if !weapon_is_melee
                    || dimensions.arm_reach_metres <= 0.0
                    || !melee_target_reachable
        ) || matches!(
            &controller.phase,
            OffensiveCombatPhase::RangedWindup(_) if !use_ranged || distance > weapon_reach
        );
        if abort_windup {
            controller.phase = OffensiveCombatPhase::Pursuing;
        }

        match &mut controller.phase {
            OffensiveCombatPhase::Pursuing if use_ranged => {
                let standoff = (weapon_reach * config.ranged_reach_fraction)
                    .clamp(
                        config.ranged_standoff_min_metres,
                        config.ranged_standoff_max_metres,
                    )
                    .min(weapon_reach);
                if distance > weapon_reach
                    || distance > standoff + config.ranged_standoff_slop_metres
                {
                    input.last_movement = Some(Vec2::Y);
                } else if distance + config.ranged_standoff_slop_metres < standoff {
                    input.last_movement = Some(-Vec2::Y);
                } else {
                    input.last_movement = None;
                    let windup = CombatDuration::from_secs_f32(config.windup_seconds);
                    cmd.trigger(RangedAttackStartedIntent {
                        attacker: entity,
                        target: Some(target),
                        animation_windup: windup,
                        minimum_windup: windup,
                    });
                    controller.phase = OffensiveCombatPhase::RangedWindup(Timer::from_seconds(
                        config.windup_seconds,
                        TimerMode::Once,
                    ));
                }
            }
            OffensiveCombatPhase::Pursuing
                if weapon_is_melee
                    && dimensions.arm_reach_metres > 0.0
                    && melee_target_reachable =>
            {
                input.last_movement = None;
                let Some(strike_family) = skeleton.available_strike_family(strike_family) else {
                    continue;
                };
                cmd.trigger(MeleeAttackStartedIntent {
                    attacker: entity,
                    target,
                    body_part: config.target_body_part,
                    windup: CombatDuration::from_secs_f32(config.windup_seconds),
                    strike_family,
                    hand: AttackHand::Main,
                });
                controller.phase = OffensiveCombatPhase::MeleeWindup {
                    timer: Timer::from_seconds(
                        config
                            .windup_seconds
                            .max(melee_lunge_delay.unwrap_or_default()),
                        TimerMode::Once,
                    ),
                    strike_family,
                };
            }
            OffensiveCombatPhase::Pursuing => {
                input.last_movement = Some(Vec2::Y);
            }
            OffensiveCombatPhase::MeleeWindup {
                timer,
                strike_family,
            } => {
                input.last_movement = None;
                timer.tick(time.delta());
                if timer.is_finished() {
                    cmd.trigger(MeleeAttackIntent {
                        attacker: entity,
                        target,
                        body_part: config.target_body_part,
                        reported_precision: ReportedPrecision::new(config.hit_precision)
                            .expect("AI precision is finite"),
                        strike_family: *strike_family,
                        hand: AttackHand::Main,
                    });
                    controller.phase = OffensiveCombatPhase::Cooldown(Timer::from_seconds(
                        config.cooldown_seconds,
                        TimerMode::Once,
                    ));
                }
            }
            OffensiveCombatPhase::RangedWindup(timer) => {
                input.last_movement = None;
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
                        config.cooldown_seconds,
                        TimerMode::Once,
                    ));
                }
            }
            OffensiveCombatPhase::Cooldown(timer) => {
                input.last_movement = None;
                timer.tick(time.delta());
                if timer.is_finished() {
                    controller.phase = OffensiveCombatPhase::Pursuing;
                }
            }
        }
    }
}
