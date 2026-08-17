use bevy::ecs::entity::MapEntities;

use super::*;

const AI_HIT_PRECISION: f32 = 1.0;
const AI_BODY_PART: BodyPart = BodyPart::Chest;
const AI_WINDUP_SECS: f32 = 0.5;
const AI_COOLDOWN_SECS: f32 = 1.0;
const AI_RANGED_MIN_STANDOFF: f32 = 1.5;
const AI_RANGED_MAX_STANDOFF: f32 = 12.0;
const AI_RANGED_STANDOFF_SLOP: f32 = 0.5;

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
) {
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
        let interaction_range = melee_interaction_range(weapon_reach);

        let abort_windup = matches!(
            &controller.phase,
            OffensiveCombatPhase::MeleeWindup { .. }
                if !weapon_is_melee || weapon_reach <= 0.0 || distance > interaction_range
        ) || matches!(
            &controller.phase,
            OffensiveCombatPhase::RangedWindup(_) if !use_ranged || distance > weapon_reach
        );
        if abort_windup {
            controller.phase = OffensiveCombatPhase::Pursuing;
        }

        match &mut controller.phase {
            OffensiveCombatPhase::Pursuing if use_ranged => {
                let standoff = (weapon_reach * 0.5)
                    .clamp(AI_RANGED_MIN_STANDOFF, AI_RANGED_MAX_STANDOFF)
                    .min(weapon_reach);
                if distance > weapon_reach || distance > standoff + AI_RANGED_STANDOFF_SLOP {
                    input.last_movement = Some(Vec2::Y);
                } else if distance + AI_RANGED_STANDOFF_SLOP < standoff {
                    input.last_movement = Some(-Vec2::Y);
                } else {
                    input.last_movement = None;
                    cmd.trigger(RangedAttackStartedIntent {
                        attacker: entity,
                        target: Some(target),
                        windup: CombatDuration::from_duration(std::time::Duration::from_millis(
                            500,
                        )),
                    });
                    controller.phase = OffensiveCombatPhase::RangedWindup(Timer::from_seconds(
                        AI_WINDUP_SECS,
                        TimerMode::Once,
                    ));
                }
            }
            OffensiveCombatPhase::Pursuing
                if weapon_is_melee && weapon_reach > 0.0 && distance <= interaction_range =>
            {
                input.last_movement = None;
                let Some(strike_family) = skeleton.available_strike_family(strike_family) else {
                    continue;
                };
                cmd.trigger(MeleeAttackStartedIntent {
                    attacker: entity,
                    target,
                    windup: CombatDuration::from_duration(std::time::Duration::from_millis(500)),
                    strike_family,
                });
                controller.phase = OffensiveCombatPhase::MeleeWindup {
                    timer: Timer::from_seconds(AI_WINDUP_SECS, TimerMode::Once),
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
                        body_part: AI_BODY_PART,
                        reported_precision: ReportedPrecision::new(AI_HIT_PRECISION)
                            .expect("AI precision is finite"),
                        strike_family: *strike_family,
                    });
                    controller.phase = OffensiveCombatPhase::Cooldown(Timer::from_seconds(
                        AI_COOLDOWN_SECS,
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
                        body_part: AI_BODY_PART,
                        reported_precision: ReportedPrecision::new(AI_HIT_PRECISION)
                            .expect("AI precision is finite"),
                    });
                    controller.phase = OffensiveCombatPhase::Cooldown(Timer::from_seconds(
                        AI_COOLDOWN_SECS,
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
