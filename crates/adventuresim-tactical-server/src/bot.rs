use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::FromClient,
    message::{DefendRequest, MeleeActionPhase, MeleeActionRequest},
};
use bevy::prelude::*;
use std::cmp::Ordering;

use crate::{
    MissionState,
    combat::{
        Incapacitated, MeleeAttackIntent, MeleeAttackStartedIntent, PendingDefenderResponse,
        RangedAttackIntent, RangedAttackStartedIntent, TacticalCombatSide,
        TacticalCombatantDefeated,
    },
};

/// Chance that a bot notices an incoming attack in time to parry it.
const PARRY_CHANCE: f64 = 0.2;
/// Chance that a bot notices an incoming attack in time to dodge it.
const DODGE_CHANCE: f64 = 0.2;
/// Flanking values at or below this are considered "facing each other" (see
/// [`flanking_from_dir`]), which is the only case a bot can react at all.
const FRONTAL_FLANKING_MAX: f32 = 0.01;
/// Range (in seconds) a bot's reaction to a noticed attack is delayed by,
/// simulating varying skill/reflexes between bots. A bot that rolls a long
/// delay may end up committing its reaction only after the attack has
/// already been resolved, i.e. reacting too late to matter.
const REACTION_DELAY_SECS: std::ops::Range<f32> = 0.05..0.6;
/// AI attacks are intentionally deterministic until animation-driven precision
/// can be authored for server-controlled actors.
const AI_HIT_PRECISION: f32 = 1.0;
const AI_BODY_PART: BodyPart = BodyPart::Chest;
const AI_WINDUP_SECS: f32 = 0.5;
const AI_COOLDOWN_SECS: f32 = 1.0;
const AI_RANGED_MIN_STANDOFF: f32 = 1.5;
const AI_RANGED_MAX_STANDOFF: f32 = 12.0;
const AI_RANGED_STANDOFF_SLOP: f32 = 0.5;
const ARROW_ITEM_ID: &str = "arrow";

fn ranged_weapon_needs_ammo_lookup(weapon_is_ranged: bool, weapon_reach: f32) -> bool {
    weapon_is_ranged && weapon_reach.is_finite() && weapon_reach > 0.0
}

/// Marks a server-controlled bot filling in for a temporary (non-connected)
/// mission character.
#[derive(Component)]
pub struct MissionEnemy;

/// Enables server-owned offensive control, preferring ranged fire while a
/// usable ranged weapon and arrows are available and otherwise using melee.
#[derive(Component, Debug)]
pub struct OffensiveMeleeAi {
    target: Option<Entity>,
    phase: OffensiveMeleePhase,
}

impl Default for OffensiveMeleeAi {
    fn default() -> Self {
        Self {
            target: None,
            phase: OffensiveMeleePhase::Pursuing,
        }
    }
}

#[derive(Debug)]
enum OffensiveMeleePhase {
    Pursuing,
    MeleeWindup(Timer),
    RangedWindup(Timer),
    Cooldown(Timer),
}

#[derive(Component)]
pub struct CountedEnemyDefeat;

/// A bot's yet-to-commit reaction to a noticed attack. Ticks down for
/// [`REACTION_DELAY_SECS`] before becoming a [`PendingDefenderResponse`],
/// simulating the bot's reflexes.
#[derive(Component)]
struct PendingBotReaction {
    timer: Timer,
    choice: DefendRequest,
}

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_tactical_combatant_defeated)
            .add_observer(on_attack_started)
            .add_observer(on_targeted_attack_started)
            .add_observer(on_targeted_ranged_attack_started)
            .add_systems(Update, (drive_offensive_melee_ai, tick_bot_reactions));
    }
}

fn on_tactical_combatant_defeated(
    defeated: On<TacticalCombatantDefeated>,
    enemies: Query<(), (With<MissionEnemy>, Without<CountedEnemyDefeat>)>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
) {
    let entity = defeated.0;
    if enemies.get(entity).is_err() {
        return;
    }
    commands.entity(entity).insert(CountedEnemyDefeat);
    state.enemies_defeated = state.enemies_defeated.saturating_add(1);
}

/// Predicts whether the nearest opposing AI facing a client attacker notices
/// the untargeted client windup and decides to dodge or parry it.
///
/// A bot has no real reflexes: it only ever gets a chance to react when it is
/// facing its attacker (`flanking <= FRONTAL_FLANKING_MAX`), and even then it
/// correctly reads the attack only some of the time. A decision to react is
/// committed only after a random delay (see [`REACTION_DELAY_SECS`]).
fn on_attack_started(
    event: On<FromClient<MeleeActionRequest>>,
    mut cmd: Commands,
    q_character: Query<(&CharacterLook, &Transform, &TacticalCombatSide)>,
    q_bots: Query<
        (Entity, &CharacterLook, &Transform, &TacticalCombatSide),
        (With<OffensiveMeleeAi>, Without<Incapacitated>),
    >,
) {
    if event.phase != MeleeActionPhase::Start {
        return;
    }
    let Some(attacker) = event.client_id.entity() else {
        return;
    };
    let Ok((attacker_look, attacker_transform, attacker_side)) = q_character.get(attacker) else {
        return;
    };
    let nearest = q_bots
        .iter()
        .filter(|(_, _, _, side)| **side != *attacker_side)
        .min_by(|(a, _, a_transform, _), (b, _, b_transform, _)| {
            compare_target(attacker_transform, a_transform, *a, b_transform, *b)
        });
    let Some((bot, bot_look, _, _)) = nearest else {
        return;
    };
    try_start_reaction(&mut cmd, bot, attacker_look, bot_look);
}

fn on_targeted_attack_started(
    event: On<MeleeAttackStartedIntent>,
    mut cmd: Commands,
    q_character: Query<&CharacterLook>,
    q_ai: Query<&CharacterLook, (With<OffensiveMeleeAi>, Without<Incapacitated>)>,
) {
    let Ok([attacker_look, defender_look]) = q_character.get_many([event.attacker, event.target])
    else {
        return;
    };
    if q_ai.get(event.target).is_ok() {
        try_start_reaction(&mut cmd, event.target, attacker_look, defender_look);
    }
}

fn on_targeted_ranged_attack_started(
    event: On<RangedAttackStartedIntent>,
    mut cmd: Commands,
    q_character: Query<&CharacterLook>,
    q_ai: Query<&CharacterLook, (With<OffensiveMeleeAi>, Without<Incapacitated>)>,
) {
    let Some(target) = event.target else {
        return;
    };
    let Ok([attacker_look, defender_look]) = q_character.get_many([event.attacker, target]) else {
        return;
    };
    if q_ai.get(target).is_ok() {
        try_start_reaction(&mut cmd, target, attacker_look, defender_look);
    }
}

fn try_start_reaction(
    cmd: &mut Commands,
    defender: Entity,
    attacker_look: &CharacterLook,
    defender_look: &CharacterLook,
) {
    let (a2, a1) = attacker_look.yaw.sin_cos();
    let (d2, d1) = defender_look.yaw.sin_cos();
    if flanking_from_dir((a1, a2), (d1, d2)) > FRONTAL_FLANKING_MAX {
        return;
    }
    let Some(choice) = roll_defend_choice() else {
        return;
    };
    cmd.entity(defender).insert(PendingBotReaction {
        timer: Timer::from_seconds(rand::random_range(REACTION_DELAY_SECS), TimerMode::Once),
        choice,
    });
}

fn compare_target(
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

fn drive_offensive_melee_ai(
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
    candidates: Query<
        (Entity, &Transform, &TacticalCombatSide),
        (With<Player>, Without<Incapacitated>),
    >,
    mut ai: Query<
        (
            Entity,
            &Transform,
            &TacticalCombatSide,
            &mut CharacterLook,
            &mut input::AccumulatedInput,
            &mut OffensiveMeleeAi,
        ),
        Without<Incapacitated>,
    >,
) {
    for (entity, transform, side, mut look, mut input, mut controller) in &mut ai {
        let target = candidates
            .iter()
            .filter(|(candidate, _, candidate_side)| {
                *candidate != entity && **candidate_side != *side
            })
            .min_by(|(a, a_transform, _), (b, b_transform, _)| {
                compare_target(transform, a_transform, *a, b_transform, *b)
            })
            .map(|(target, _, _)| target);

        if target != controller.target {
            controller.target = target;
            controller.phase = OffensiveMeleePhase::Pursuing;
        }
        let Some(target) = target else {
            input.last_movement = None;
            continue;
        };
        let Ok((_, target_transform, _)) = candidates.get(target) else {
            continue;
        };

        let offset = target_transform.translation.xz() - transform.translation.xz();
        let distance = offset.length();
        if distance > f32::EPSILON {
            look.yaw = (-offset.x).atan2(-offset.y);
        }
        let (weapon_reach, weapon_is_melee, weapon_is_ranged) = viewer
            .get(entity)
            .map(|view| {
                (
                    view.weapon_reach(),
                    view.weapon_is_melee(),
                    view.weapon_is_ranged(),
                )
            })
            .unwrap_or_default();
        let has_ammo = ranged_weapon_needs_ammo_lookup(weapon_is_ranged, weapon_reach)
            && viewer.inventory.get(entity).has_item_id(ARROW_ITEM_ID);
        let use_ranged = weapon_is_ranged && weapon_reach > 0.0 && has_ammo;
        let interaction_range = melee_interaction_range(weapon_reach);

        let abort_windup = matches!(
            &controller.phase,
            OffensiveMeleePhase::MeleeWindup(_)
                if !weapon_is_melee || weapon_reach <= 0.0 || distance > interaction_range
        ) || matches!(
            &controller.phase,
            OffensiveMeleePhase::RangedWindup(_) if !use_ranged || distance > weapon_reach
        );
        if abort_windup {
            controller.phase = OffensiveMeleePhase::Pursuing;
        }

        match &mut controller.phase {
            OffensiveMeleePhase::Pursuing if use_ranged => {
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
                        windup_secs: AI_WINDUP_SECS,
                    });
                    controller.phase = OffensiveMeleePhase::RangedWindup(Timer::from_seconds(
                        AI_WINDUP_SECS,
                        TimerMode::Once,
                    ));
                }
            }
            OffensiveMeleePhase::Pursuing
                if weapon_is_melee && weapon_reach > 0.0 && distance <= interaction_range =>
            {
                input.last_movement = None;
                cmd.trigger(MeleeAttackStartedIntent {
                    attacker: entity,
                    target,
                    windup_secs: AI_WINDUP_SECS,
                });
                controller.phase = OffensiveMeleePhase::MeleeWindup(Timer::from_seconds(
                    AI_WINDUP_SECS,
                    TimerMode::Once,
                ));
            }
            OffensiveMeleePhase::Pursuing => {
                input.last_movement = Some(Vec2::Y);
            }
            OffensiveMeleePhase::MeleeWindup(timer) => {
                input.last_movement = None;
                timer.tick(time.delta());
                if timer.is_finished() {
                    cmd.trigger(MeleeAttackIntent {
                        attacker: entity,
                        target,
                        body_part: AI_BODY_PART,
                        hit_precision: AI_HIT_PRECISION,
                    });
                    controller.phase = OffensiveMeleePhase::Cooldown(Timer::from_seconds(
                        AI_COOLDOWN_SECS,
                        TimerMode::Once,
                    ));
                }
            }
            OffensiveMeleePhase::RangedWindup(timer) => {
                input.last_movement = None;
                timer.tick(time.delta());
                if timer.is_finished() {
                    cmd.trigger(RangedAttackIntent {
                        attacker: entity,
                        target: Some(target),
                        body_part: AI_BODY_PART,
                        hit_precision: AI_HIT_PRECISION,
                    });
                    controller.phase = OffensiveMeleePhase::Cooldown(Timer::from_seconds(
                        AI_COOLDOWN_SECS,
                        TimerMode::Once,
                    ));
                }
            }
            OffensiveMeleePhase::Cooldown(timer) => {
                input.last_movement = None;
                timer.tick(time.delta());
                if timer.is_finished() {
                    controller.phase = OffensiveMeleePhase::Pursuing;
                }
            }
        }
    }
}

fn roll_defend_choice() -> Option<DefendRequest> {
    let roll: f64 = rand::random();
    if roll < PARRY_CHANCE {
        Some(DefendRequest::Parry)
    } else if roll < PARRY_CHANCE + DODGE_CHANCE {
        Some(DefendRequest::Dodge)
    } else {
        None
    }
}

fn tick_bot_reactions(
    mut cmd: Commands,
    time: Res<Time<()>>,
    mut q_reacting: Query<(Entity, &mut PendingBotReaction), Without<Incapacitated>>,
) {
    for (bot, mut reaction) in &mut q_reacting {
        reaction.timer.tick(time.delta());
        if !reaction.timer.is_finished() {
            continue;
        }

        cmd.entity(bot)
            .remove::<PendingBotReaction>()
            .insert(PendingDefenderResponse {
                choice: reaction.choice,
                set_at: time.elapsed_secs(),
            });
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU32, time::Duration};

    use super::*;

    #[derive(Resource, Default)]
    struct RecordedAttacks(Vec<(Entity, Entity)>);

    #[derive(Resource, Default)]
    struct RecordedRangedAttacks(Vec<(Entity, Entity)>);

    fn record_attack(event: On<MeleeAttackIntent>, mut attacks: ResMut<RecordedAttacks>) {
        attacks.0.push((event.attacker, event.target));
    }

    fn record_ranged_attack(
        event: On<RangedAttackIntent>,
        mut attacks: ResMut<RecordedRangedAttacks>,
    ) {
        if let Some(target) = event.target {
            attacks.0.push((event.attacker, target));
        }
    }

    fn apply_deterministic_test_hit(
        event: On<MeleeAttackIntent>,
        mut combatants: Query<(&mut Limbs, &mut TacticalCombatState)>,
    ) {
        let Ok([attacker, defender]) = combatants.get_many_mut([event.attacker, event.target])
        else {
            return;
        };
        let (_, mut attacker_state) = attacker;
        let (mut defender_limbs, mut defender_state) = defender;
        crate::combat::apply_transient_attack_result(
            &mut attacker_state,
            &mut defender_limbs,
            &mut defender_state,
            AttackResult::ToDefender {
                cut_damage: 160.0,
                blunt_damage: 0.0,
                balance_damage: 0.0,
                contact_force: 160.0,
                armor_contact: false,
            },
            BodyPart::Chest,
        );
    }

    fn spawn_test_ai(world: &mut World, side: TacticalCombatSide, position: Vec3) -> Entity {
        // Temporary characters receive this production loadout from
        // `insert_new_character` when no authored starting package is given.
        const KATZBALGER_REACH: f32 = 0.8;
        let actor = world
            .spawn((
                Player::default(),
                Transform::from_translation(position),
                side,
                CharacterLook::default(),
                input::AccumulatedInput::default(),
                OffensiveMeleeAi::default(),
                TacticalCombatState::default(),
            ))
            .id();
        let weapon = world.spawn(ItemOf(actor)).id();
        // Match production's insertion order: the equip hook must see both
        // the owning inventory relationship and the weapon classification.
        world.entity_mut(weapon).insert(WeaponItem {
            skill_weights: [0.0; 9],
            accuracy: 1.0,
            penetration: 1.0,
            reach: KATZBALGER_REACH,
            balance: 0.0,
            precise: false,
            melee: true,
            ranged: false,
            blunt: false,
            slash: true,
            pierce: false,
        });
        world.entity_mut(weapon).insert(EquipSlot::HoldingRight);
        actor
    }

    fn spawn_test_ranged_ai(
        world: &mut World,
        side: TacticalCombatSide,
        position: Vec3,
    ) -> (Entity, Entity) {
        const TEST_WEAPON_REACH: f32 = 8.0;
        let actor = world
            .spawn((
                Player::default(),
                Transform::from_translation(position),
                side,
                CharacterLook::default(),
                input::AccumulatedInput::default(),
                OffensiveMeleeAi::default(),
                TacticalCombatState::default(),
            ))
            .id();
        let weapon = world.spawn(ItemOf(actor)).id();
        world.entity_mut(weapon).insert(WeaponItem {
            skill_weights: [0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            accuracy: 1.0,
            penetration: 1.0,
            reach: TEST_WEAPON_REACH,
            balance: 0.0,
            precise: false,
            melee: true,
            ranged: true,
            blunt: false,
            slash: false,
            pierce: true,
        });
        world.entity_mut(weapon).insert(EquipSlot::HoldingRight);
        let ammo = world
            .spawn((
                ItemOf(actor),
                ItemProperties {
                    id: ARROW_ITEM_ID.to_owned(),
                    weight: 0.05,
                },
                ItemQuantity(NonZeroU32::new(1).unwrap()),
            ))
            .id();
        (actor, ammo)
    }

    fn spawn_test_target(world: &mut World, side: TacticalCombatSide, position: Vec3) -> Entity {
        world
            .spawn((
                Player::default(),
                Transform::from_translation(position),
                side,
                CharacterLook::default(),
                TacticalCombatState::default(),
            ))
            .id()
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<RecordedAttacks>()
            .init_resource::<RecordedRangedAttacks>()
            .add_observer(record_attack)
            .add_observer(record_ranged_attack)
            .add_systems(Update, drive_offensive_melee_ai);
        app
    }

    #[test]
    fn ranged_ai_fires_then_falls_back_to_melee_when_ammo_is_exhausted() {
        assert!(!ranged_weapon_needs_ammo_lookup(false, 1.0));
        assert!(!ranged_weapon_needs_ammo_lookup(true, 0.0));
        assert!(ranged_weapon_needs_ammo_lookup(true, 8.0));
        let mut app = test_app();
        let (actor, ammo) = spawn_test_ranged_ai(
            app.world_mut(),
            TacticalCombatSide::Enemy,
            Vec3::new(0.0, 0.0, 2.0),
        );
        let target = spawn_test_target(
            app.world_mut(),
            TacticalCombatSide::Party,
            Vec3::new(0.0, 0.0, -2.0),
        );

        for _ in 0..7 {
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(Duration::from_millis(100));
            app.update();
        }
        assert_eq!(
            app.world().resource::<RecordedRangedAttacks>().0,
            vec![(actor, target)]
        );
        assert!(app.world().resource::<RecordedAttacks>().0.is_empty());

        // Production consumes this through `resolve_ranged_attack`; removing
        // it here isolates deterministic controller selection from physics.
        app.world_mut().despawn(ammo);
        for _ in 0..20 {
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(Duration::from_millis(100));
            app.update();
        }

        assert!(
            app.world()
                .resource::<RecordedAttacks>()
                .0
                .contains(&(actor, target)),
            "ammo exhaustion should return a melee-capable weapon to melee cadence"
        );
        assert_eq!(app.world().resource::<RecordedRangedAttacks>().0.len(), 1);
    }

    #[test]
    fn enemy_defeat_is_counted_only_once() {
        let mut app = App::new();
        app.insert_resource(MissionState {
            timeout: None,
            enemies_defeated: 0,
            required_enemy_defeats: 1,
            expected_party_members: 1,
            seen_party_members: Default::default(),
            enrollment_begun: false,
            enrollment_sealed: false,
            abandonment_elapsed: Default::default(),
            terminal_retry_not_before: Default::default(),
            pending_resolution: None,
            pending_receipt: None,
            terminal_in_flight: false,
            terminal_ack_deadline: None,
            terminal_transport_failed: false,
            terminal_presentation: None,
            committed: false,
        })
        .add_observer(on_tactical_combatant_defeated);
        let enemy = app.world_mut().spawn(MissionEnemy).id();

        app.world_mut().trigger(TacticalCombatantDefeated(enemy));
        app.update();
        app.world_mut().trigger(TacticalCombatantDefeated(enemy));
        app.update();

        assert_eq!(app.world().resource::<MissionState>().enemies_defeated, 1);
        assert!(app.world().entity(enemy).contains::<CountedEnemyDefeat>());
    }

    #[test]
    fn opposing_ai_approach_face_stop_and_both_attack() {
        let mut app = test_app();
        let party = spawn_test_ai(
            app.world_mut(),
            TacticalCombatSide::Party,
            Vec3::new(0.0, 0.0, -3.0),
        );
        let enemy = spawn_test_ai(
            app.world_mut(),
            TacticalCombatSide::Enemy,
            Vec3::new(0.0, 0.0, 3.0),
        );

        // Simulate only the small, deterministic movement seam driven by the
        // existing controller input. No physics, wall clock, spawn randomness,
        // or global RNG is involved in this headless behavior test.
        for _ in 0..20 {
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(Duration::from_millis(100));
            app.update();

            let snapshots: Vec<_> = [party, enemy]
                .into_iter()
                .map(|actor| {
                    let entity = app.world().entity(actor);
                    (
                        actor,
                        entity
                            .get::<input::AccumulatedInput>()
                            .unwrap()
                            .last_movement,
                        entity.get::<CharacterLook>().unwrap().yaw,
                    )
                })
                .collect();
            for (actor, movement, yaw) in snapshots {
                if movement.is_some() {
                    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
                    app.world_mut()
                        .entity_mut(actor)
                        .get_mut::<Transform>()
                        .unwrap()
                        .translation += forward * 0.5;
                }
            }
        }

        let party_transform = app.world().entity(party).get::<Transform>().unwrap();
        let enemy_transform = app.world().entity(enemy).get::<Transform>().unwrap();
        let separation = party_transform
            .translation
            .xz()
            .distance(enemy_transform.translation.xz());
        const KATZBALGER_REACH: f32 = 0.8;
        const TWO_CHARACTER_COLLIDER_RADII: f32 = 0.8;
        assert!(
            separation <= melee_interaction_range(KATZBALGER_REACH),
            "AI stopped outside melee interaction range: {separation}"
        );
        assert!(
            separation >= TWO_CHARACTER_COLLIDER_RADII,
            "AI test movement collapsed production-sized character colliders: {separation}"
        );
        for (actor, target) in [(party, enemy), (enemy, party)] {
            let entity = app.world().entity(actor);
            let target_entity = app.world().entity(target);
            let yaw = entity.get::<CharacterLook>().unwrap().yaw;
            let forward = Vec2::new(-yaw.sin(), -yaw.cos());
            let toward_target = (target_entity.get::<Transform>().unwrap().translation.xz()
                - entity.get::<Transform>().unwrap().translation.xz())
            .normalize();
            let facing = forward.dot(toward_target);
            assert!(
                facing > 0.999,
                "AI {actor:?} yaw {yaw} faces {forward:?}, target direction {toward_target:?}, dot {facing}, separation {separation}"
            );
        }
        assert_eq!(
            app.world()
                .entity(party)
                .get::<input::AccumulatedInput>()
                .unwrap()
                .last_movement,
            None
        );
        assert_eq!(
            app.world()
                .entity(enemy)
                .get::<input::AccumulatedInput>()
                .unwrap()
                .last_movement,
            None
        );

        let attacks = &app.world().resource::<RecordedAttacks>().0;
        assert!(attacks.contains(&(party, enemy)));
        assert!(attacks.contains(&(enemy, party)));
    }

    #[test]
    fn nearest_target_ties_use_entity_identity() {
        let mut app = test_app();
        let actor = spawn_test_ai(app.world_mut(), TacticalCombatSide::Party, Vec3::ZERO);
        let first = spawn_test_ai(
            app.world_mut(),
            TacticalCombatSide::Enemy,
            Vec3::new(-2.0, 0.0, 0.0),
        );
        let second = spawn_test_ai(
            app.world_mut(),
            TacticalCombatSide::Enemy,
            Vec3::new(2.0, 0.0, 0.0),
        );
        app.update();

        let selected = app
            .world()
            .entity(actor)
            .get::<OffensiveMeleeAi>()
            .unwrap()
            .target;
        let expected = if first.to_bits() < second.to_bits() {
            first
        } else {
            second
        };
        assert_eq!(selected, Some(expected));
    }

    #[test]
    fn ai_duel_stops_incapacitated_combatants() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<RecordedAttacks>()
            .add_observer(record_attack)
            .add_observer(apply_deterministic_test_hit)
            .add_systems(
                Update,
                (
                    drive_offensive_melee_ai,
                    crate::combat::update_tactical_combat_state,
                )
                    .chain(),
            );
        let party = spawn_test_ai(
            app.world_mut(),
            TacticalCombatSide::Party,
            Vec3::new(0.0, 0.0, -3.0),
        );
        let enemy = spawn_test_ai(
            app.world_mut(),
            TacticalCombatSide::Enemy,
            Vec3::new(0.0, 0.0, 3.0),
        );

        for _ in 0..30 {
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(Duration::from_millis(100));
            app.update();
            let snapshots: Vec<_> = [party, enemy]
                .into_iter()
                .map(|actor| {
                    let entity = app.world().entity(actor);
                    (
                        actor,
                        entity
                            .get::<input::AccumulatedInput>()
                            .unwrap()
                            .last_movement,
                        entity.get::<CharacterLook>().unwrap().yaw,
                    )
                })
                .collect();
            for (actor, movement, yaw) in snapshots {
                if movement.is_some() {
                    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
                    app.world_mut()
                        .entity_mut(actor)
                        .get_mut::<Transform>()
                        .unwrap()
                        .translation += forward * 0.5;
                }
            }
            if [party, enemy].into_iter().any(|actor| {
                app.world()
                    .entity(actor)
                    .get::<TacticalCombatState>()
                    .is_some_and(|state| state.incapacitated)
            }) {
                break;
            }
        }

        let incapacitated: Vec<_> = [party, enemy]
            .into_iter()
            .filter(|actor| {
                app.world()
                    .entity(*actor)
                    .get::<TacticalCombatState>()
                    .unwrap()
                    .incapacitated
            })
            .collect();
        assert!(!incapacitated.is_empty());
        for actor in incapacitated {
            assert_eq!(
                app.world()
                    .entity(actor)
                    .get::<input::AccumulatedInput>()
                    .unwrap()
                    .last_movement,
                None
            );
        }
        assert!(!app.world().resource::<RecordedAttacks>().0.is_empty());

        let party_defeated = app
            .world()
            .entity(party)
            .get::<TacticalCombatState>()
            .unwrap()
            .incapacitated;
        let enemy_defeated = app
            .world()
            .entity(enemy)
            .get::<TacticalCombatState>()
            .unwrap()
            .incapacitated;
        let resolution = crate::terminal_resolution(crate::TerminalCombatSnapshot {
            required_enemies: 1,
            loaded_enemies: 1,
            defeated_enemies: u32::from(enemy_defeated),
            loaded_party: 1,
            incapacitated_party: u32::from(party_defeated),
            enrollment_sealed: true,
        });
        let expected = if party_defeated {
            Some(crate::TacticalMissionResolution::Failed)
        } else {
            Some(crate::TacticalMissionResolution::Defeated)
        };
        assert_eq!(resolution, expected);
    }
}
