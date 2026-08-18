mod defense;
mod offense;

use adventuresim_core::item_references::ARROW_ID;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::FromClient,
    message::{DefendRequest, MeleeActionRequest},
};
use bevy::prelude::*;
use std::cmp::Ordering;

use crate::{
    combat::{
        CombatDuration, CombatInstant, CombatSet, MeleeAttackIntent, MeleeAttackStartedIntent,
        PendingDefenderResponse, RangedAttackIntent, RangedAttackStartedIntent, ReportedPrecision,
        TacticalCombatSide, TacticalCombatantDefeated,
    },
    mission::MissionState,
};
#[cfg(test)]
use defense::CountedEnemyDefeat;
pub use defense::DefenseChances;
use defense::{
    on_attack_started, on_tactical_combatant_defeated, on_targeted_attack_started,
    on_targeted_ranged_attack_started, tick_bot_reactions,
};
pub use offense::OffensiveCombatAi;
#[cfg(test)]
use offense::ranged_weapon_needs_ammo_lookup;
use offense::{compare_target, drive_offensive_combat_ai};

/// Marks a server-controlled bot filling in for a temporary (non-connected)
/// mission character.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct MissionEnemy;

/// Real-time grace period between a bot dying and its ECS despawn. See
/// [`PendingRemoval`] for why this exists; it has nothing to do with the
/// client's (much longer, purely cosmetic) fade-out duration.
const DESPAWN_REPLICATION_GRACE_SECONDS: f32 = 0.3;

/// Marks a dead bot for removal after [`DESPAWN_REPLICATION_GRACE_SECONDS`]
/// of real time, rather than despawning it the instant it dies.
///
/// This has nothing to do with the client's cosmetic fade (that lives
/// entirely in the tactical client's `player::start_fade_on_incapacitation`
/// and is invisible to the server, and lasts far longer). It exists because
/// bevy_replicon sends this bot's killing-blow `SuccessfulAttackResponse` and
/// its eventual despawn as separate messages, and an event referencing an
/// already-despawned entity fails to deserialize client-side ("unable to map
/// entities ... from the server"). Rather than relying on exact same-frame
/// scheduling order against bevy_replicon's internal tick/send machinery (an
/// assumption two earlier attempts got wrong in practice), this just waits
/// long enough in wall-clock time that several replication sends have
/// unambiguously happened first, however that machinery is actually timed.
#[derive(Component)]
struct PendingRemoval {
    timer: Timer,
}

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_tactical_combatant_defeated)
            .add_observer(on_attack_started)
            .add_observer(on_targeted_attack_started)
            .add_observer(on_targeted_ranged_attack_started)
            .add_systems(
                Update,
                (
                    drive_offensive_combat_ai,
                    tick_bot_reactions,
                    despawn_pending_removals,
                )
                    .after(CombatSet::Condition),
            );
    }
}

/// Despawns bots marked [`PendingRemoval`] once their grace timer elapses.
fn despawn_pending_removals(
    mut commands: Commands,
    time: Res<Time<()>>,
    mut q: Query<(Entity, &mut PendingRemoval)>,
) {
    for (entity, mut pending) in &mut q {
        pending.timer.tick(time.delta());
        if pending.timer.is_finished() {
            commands.entity(entity).despawn();
        }
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
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                side,
                CharacterLook::default(),
                input::AccumulatedInput::default(),
                OffensiveCombatAi::default(),
                TacticalCombatState::default(),
            ))
            .id();
        let weapon = world.spawn(ItemOf(actor)).id();
        // Match production's insertion order: the equip hook must see both
        // the owning inventory relationship and the weapon classification.
        world.entity_mut(weapon).insert(WeaponItem {
            skill_weights: [0.0; 9],
            accuracy: 1.0,
            swing_precision: 0.45,
            stab_precision: 0.6,
            prefers_stab: false,
            penetration: 1.0,
            reach: KATZBALGER_REACH,
            balance: 0.0,
            precise: false,
            melee: true,
            ranged: false,
            blunt: false,
            slash: true,
            pierce: false,
            windup_secs: 0.3,
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
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                side,
                CharacterLook::default(),
                input::AccumulatedInput::default(),
                OffensiveCombatAi::default(),
                TacticalCombatState::default(),
            ))
            .id();
        let weapon = world.spawn(ItemOf(actor)).id();
        world.entity_mut(weapon).insert(WeaponItem {
            skill_weights: [0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            accuracy: 1.0,
            swing_precision: 0.45,
            stab_precision: 0.6,
            prefers_stab: false,
            penetration: 1.0,
            reach: TEST_WEAPON_REACH,
            balance: 0.0,
            precise: false,
            melee: true,
            ranged: true,
            blunt: false,
            slash: false,
            pierce: true,
            windup_secs: 0.3,
        });
        world.entity_mut(weapon).insert(EquipSlot::HoldingRight);
        let ammo = world
            .spawn((
                ItemOf(actor),
                ItemProperties {
                    id: ARROW_ID.to_owned(),
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
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
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
            .add_systems(Update, drive_offensive_combat_ai);
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
        app.insert_resource(MissionState::new(None, 1, NonZeroU32::new(1).unwrap()))
            .add_observer(on_tactical_combatant_defeated);
        let enemy = app.world_mut().spawn(MissionEnemy).id();

        app.world_mut().trigger(TacticalCombatantDefeated(enemy));
        app.update();
        app.world_mut().trigger(TacticalCombatantDefeated(enemy));
        app.update();

        assert_eq!(app.world().resource::<MissionState>().enemies_defeated(), 1);
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
            .get::<OffensiveCombatAi>()
            .unwrap()
            .target();
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
                    crate::combat::update_tactical_combat_state,
                    drive_offensive_combat_ai,
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
                    .is_some_and(TacticalCombatState::is_incapacitated)
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
                    .is_incapacitated()
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
            .is_incapacitated();
        let enemy_defeated = app
            .world()
            .entity(enemy)
            .get::<TacticalCombatState>()
            .unwrap()
            .is_incapacitated();
        let resolution =
            crate::mission::terminal_resolution(crate::mission::TerminalCombatSnapshot {
                required_enemies: 1,
                loaded_enemies: 1,
                defeated_enemies: u32::from(enemy_defeated),
                loaded_party: 1,
                incapacitated_party: u32::from(party_defeated),
                enrollment_sealed: true,
            });
        let expected = if party_defeated {
            Some(adventuresim_stdb_client::TacticalMissionResolution::Failed)
        } else {
            Some(adventuresim_stdb_client::TacticalMissionResolution::Defeated)
        };
        assert_eq!(resolution, expected);
    }
}
