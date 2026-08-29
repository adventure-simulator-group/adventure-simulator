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
        CombatDuration, CombatSet, DefendIntent, MeleeAttackStartedIntent, RangedAttackIntent,
        RangedAttackStartedIntent, ReportedPrecision, TacticalCombatSide,
    },
    player_projection::begin_get_up_transition_configured,
};
pub use defense::{DefenseChances, ReactiveDefenseAi};
use defense::{
    on_attack_started, on_targeted_attack_started, on_targeted_ranged_attack_started,
    tick_bot_reactions,
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

/// Declarative capabilities that can be composed into a combatant. Higher
/// level tactics such as flanking or ambushing can add packages later without
/// turning the bot controller into one mutually-exclusive behavior enum.
#[derive(Reflect, Debug, Clone, Copy, PartialEq)]
pub enum CombatantBehaviorPackage {
    OffensiveCombat,
    RaisedGuard,
    AimAtNearestOpponent,
    RecoverToUpright,
    ReactiveDefense {
        chances: DefenseChances,
        requires_facing: bool,
    },
}

#[derive(Component, Reflect, Debug, Clone, Default, PartialEq)]
#[reflect(Component)]
pub struct CombatantBehaviorPackages(pub Vec<CombatantBehaviorPackage>);

impl CombatantBehaviorPackages {
    #[must_use]
    pub fn standard_combat(config: &TacticalCombatConfig) -> Self {
        let defense = &config.ai.ordinary.defense;
        Self(vec![
            CombatantBehaviorPackage::RecoverToUpright,
            CombatantBehaviorPackage::OffensiveCombat,
            CombatantBehaviorPackage::ReactiveDefense {
                chances: DefenseChances {
                    parry_chance: defense.parry_chance,
                    dodge_chance: defense.dodge_chance,
                },
                requires_facing: true,
            },
        ])
    }

    #[must_use]
    pub fn passive() -> Self {
        Self(vec![CombatantBehaviorPackage::RecoverToUpright])
    }

    #[must_use]
    pub fn always_block_without_facing() -> Self {
        Self(vec![
            CombatantBehaviorPackage::RecoverToUpright,
            CombatantBehaviorPackage::RaisedGuard,
            CombatantBehaviorPackage::ReactiveDefense {
                chances: DefenseChances {
                    parry_chance: 1.0,
                    dodge_chance: 0.0,
                },
                requires_facing: false,
            },
        ])
    }

    #[must_use]
    pub fn always_dodge() -> Self {
        Self(vec![
            CombatantBehaviorPackage::RecoverToUpright,
            CombatantBehaviorPackage::RaisedGuard,
            CombatantBehaviorPackage::AimAtNearestOpponent,
            CombatantBehaviorPackage::ReactiveDefense {
                chances: DefenseChances {
                    parry_chance: 0.0,
                    dodge_chance: 1.0,
                },
                requires_facing: true,
            },
        ])
    }
}

fn materialize_behavior_packages(
    mut cmd: Commands,
    packages: Query<(Entity, &CombatantBehaviorPackages), Added<CombatantBehaviorPackages>>,
) {
    for (entity, packages) in &packages {
        let mut entity = cmd.entity(entity);
        for package in &packages.0 {
            match package {
                CombatantBehaviorPackage::OffensiveCombat => {
                    entity.insert(OffensiveCombatAi::default());
                }
                CombatantBehaviorPackage::RaisedGuard => {
                    entity.insert(RaisedGuardAi);
                }
                CombatantBehaviorPackage::AimAtNearestOpponent => {
                    entity.insert(AimAtNearestOpponentAi);
                }
                CombatantBehaviorPackage::RecoverToUpright => {
                    entity.insert(RecoverToUprightAi);
                }
                CombatantBehaviorPackage::ReactiveDefense {
                    chances,
                    requires_facing,
                } => {
                    entity.insert((
                        ReactiveDefenseAi {
                            requires_facing: *requires_facing,
                        },
                        *chances,
                    ));
                }
            }
        }
    }
}

/// Maintains the same authoritative raised-guard state that player input
/// projects into [`SkeletonState`]. It intentionally does not imply facing or
/// target selection; those are separate behavior capabilities.
#[derive(Component, Reflect, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[reflect(Component)]
pub struct RaisedGuardAi;

#[derive(Component, Reflect, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[reflect(Component)]
pub struct AimAtNearestOpponentAi;

#[derive(Component, Reflect, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[reflect(Component)]
pub struct RecoverToUprightAi;

fn maintain_guard_stance(
    mut guarded: Query<(&TacticalCombatState, &mut SkeletonState), With<RaisedGuardAi>>,
) {
    for (state, mut skeleton) in &mut guarded {
        let guard = if state.is_incapacitated() {
            WeaponGuardState::Lowered
        } else {
            WeaponGuardState::Raised
        };
        set_weapon_guard(&mut skeleton, guard);
    }
}

fn aim_at_nearest_opponent(
    candidates: Query<
        (
            Entity,
            &Transform,
            &TacticalCombatSide,
            &TacticalCombatState,
        ),
        With<Player>,
    >,
    mut aiming: Query<
        (
            Entity,
            &Transform,
            &TacticalCombatSide,
            &TacticalCombatState,
            &mut CharacterLook,
        ),
        With<AimAtNearestOpponentAi>,
    >,
) {
    for (entity, transform, side, state, mut look) in &mut aiming {
        if state.is_incapacitated() {
            continue;
        }
        let nearest = candidates
            .iter()
            .filter(|(candidate, _, candidate_side, candidate_state)| {
                *candidate != entity
                    && **candidate_side != *side
                    && !candidate_state.is_incapacitated()
            })
            .min_by(|(a, a_transform, _, _), (b, b_transform, _, _)| {
                compare_target(transform, a_transform, *a, b_transform, *b)
            });
        let Some((_, target_transform, _, _)) = nearest else {
            continue;
        };
        let offset = target_transform.translation.xz() - transform.translation.xz();
        if offset.length_squared() > f32::EPSILON {
            look.yaw = (-offset.x).atan2(-offset.y);
        }
    }
}

fn recover_to_upright(
    mut recovering: Query<(&TacticalCombatState, &mut SkeletonState), With<RecoverToUprightAi>>,
    config: Res<TacticalCombatConfig>,
) {
    for (state, mut skeleton) in &mut recovering {
        if !state.is_incapacitated() && !skeleton.is_posture_transitioning() {
            begin_get_up_transition_configured(&mut skeleton, &config);
        }
    }
}

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TacticalCombatConfig>()
            .add_observer(on_attack_started)
            .add_observer(on_targeted_attack_started)
            .add_observer(on_targeted_ranged_attack_started)
            .add_systems(
                Update,
                (
                    materialize_behavior_packages,
                    maintain_guard_stance,
                    aim_at_nearest_opponent,
                    recover_to_upright,
                    drive_offensive_combat_ai,
                    tick_bot_reactions,
                )
                    .chain()
                    .after(CombatSet::Condition),
            );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::defense::PendingBotReaction;
    use super::*;
    use crate::combat::{MeleeAttackAuthority, MeleeAttackIntent, PendingDefenderResponse};
    use crate::player_projection::AuthoritativePostureIntent;

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
                SkeletonState::default(),
                CharacterDimensions::default(),
                Collider::cylinder(0.4, 1.9),
                MeleeAttackAuthority::default(),
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
            grip_to_tip_m: KATZBALGER_REACH,
            moment_of_inertia_kg_m2: 0.0,
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
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                side,
                CharacterLook::default(),
                input::AccumulatedInput::default(),
                OffensiveCombatAi::default(),
                TacticalCombatState::default(),
                SkeletonState::default(),
                CharacterDimensions::default(),
                Collider::cylinder(0.4, 1.9),
                MeleeAttackAuthority::default(),
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
            grip_to_tip_m: 1.0,
            moment_of_inertia_kg_m2: 0.0,
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
                    id: ARROW_ID.to_owned(),
                    weight: 0.05,
                },
                TacticalItemQuantity::default(),
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
                Collider::cylinder(0.4, 1.9),
            ))
            .id()
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<TacticalCombatConfig>()
            .init_resource::<RecordedAttacks>()
            .init_resource::<RecordedRangedAttacks>()
            .add_observer(record_attack)
            .add_observer(record_ranged_attack)
            .add_observer(crate::combat::on_melee_attack_started)
            .add_systems(
                Update,
                (
                    drive_offensive_combat_ai,
                    crate::combat::resolve_pending_melee_contacts,
                )
                    .chain(),
            );
        app
    }

    #[test]
    fn behavior_packages_compose_runtime_capabilities() {
        let mut app = App::new();
        app.init_resource::<TacticalCombatConfig>()
            .add_systems(Update, materialize_behavior_packages);
        let blocker = app
            .world_mut()
            .spawn(CombatantBehaviorPackages::always_block_without_facing())
            .id();
        let standard = app
            .world_mut()
            .spawn(CombatantBehaviorPackages::standard_combat(
                &TacticalCombatConfig::default(),
            ))
            .id();
        let dodger = app
            .world_mut()
            .spawn(CombatantBehaviorPackages::always_dodge())
            .id();

        app.update();

        let blocker = app.world().entity(blocker);
        assert!(!blocker.contains::<OffensiveCombatAi>());
        assert!(blocker.contains::<RaisedGuardAi>());
        assert!(blocker.contains::<RecoverToUprightAi>());
        assert!(!blocker.get::<ReactiveDefenseAi>().unwrap().requires_facing);
        assert_eq!(
            blocker.get::<DefenseChances>(),
            Some(&DefenseChances {
                parry_chance: 1.0,
                dodge_chance: 0.0,
            })
        );

        let standard = app.world().entity(standard);
        assert!(standard.contains::<OffensiveCombatAi>());
        assert!(standard.get::<ReactiveDefenseAi>().unwrap().requires_facing);
        assert!(standard.contains::<RecoverToUprightAi>());
        assert_eq!(
            standard.get::<DefenseChances>(),
            Some(&DefenseChances::default())
        );
        assert!(!standard.contains::<RaisedGuardAi>());

        let dodger = app.world().entity(dodger);
        assert!(dodger.contains::<RaisedGuardAi>());
        assert!(dodger.contains::<AimAtNearestOpponentAi>());
        assert!(dodger.get::<ReactiveDefenseAi>().unwrap().requires_facing);
        assert!(dodger.contains::<RecoverToUprightAi>());
    }

    #[test]
    fn raised_guard_package_uses_shared_skeleton_guard_state() {
        let mut app = App::new();
        app.add_systems(
            Update,
            (materialize_behavior_packages, maintain_guard_stance).chain(),
        );
        let blocker = app
            .world_mut()
            .spawn((
                CombatantBehaviorPackages::always_block_without_facing(),
                TacticalCombatState::default(),
                SkeletonState::default(),
            ))
            .id();

        app.update();

        let skeleton = app.world().get::<SkeletonState>(blocker).unwrap();
        assert_eq!(skeleton.weapon_guard(), WeaponGuardState::Raised);
    }

    #[test]
    fn untargeted_windup_only_reacts_on_the_nearest_enemy() {
        let mut app = App::new();
        app.init_resource::<TacticalCombatConfig>();
        app.add_observer(on_attack_started);
        let attacker = app
            .world_mut()
            .spawn((
                CharacterLook::default(),
                Transform::default(),
                TacticalCombatSide::Party,
            ))
            .id();
        let passive = app
            .world_mut()
            .spawn((
                MissionEnemy,
                CharacterLook::default(),
                Transform::from_xyz(0.0, 0.0, 1.0),
                TacticalCombatSide::Enemy,
                TacticalCombatState::default(),
            ))
            .id();
        let blocker = app
            .world_mut()
            .spawn((
                MissionEnemy,
                CharacterLook::default(),
                Transform::from_xyz(0.0, 0.0, 2.0),
                TacticalCombatSide::Enemy,
                TacticalCombatState::default(),
                ReactiveDefenseAi {
                    requires_facing: false,
                },
                DefenseChances {
                    parry_chance: 1.0,
                    dodge_chance: 0.0,
                },
            ))
            .id();
        let start = FromClient {
            client_id: adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId::Client(
                attacker,
            ),
            message: MeleeActionRequest {
                strike_family: StrikeFamily::Swing,
                hand: AttackHand::Main,
                target: None,
                body_part: None,
            },
        };

        app.world_mut().trigger(start);
        app.world_mut().flush();
        assert!(!app.world().entity(blocker).contains::<PendingBotReaction>());

        app.world_mut().despawn(passive);
        app.world_mut().trigger(start);
        app.world_mut().flush();
        assert!(app.world().entity(blocker).contains::<PendingBotReaction>());
    }

    #[test]
    fn completed_bot_reaction_enters_the_shared_block_animation() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<TacticalCombatConfig>()
            .add_observer(on_attack_started)
            .add_observer(crate::combat::apply_defend_intent)
            .add_systems(Update, tick_bot_reactions);
        let attacker = app
            .world_mut()
            .spawn((
                CharacterLook::default(),
                Transform::default(),
                TacticalCombatSide::Party,
            ))
            .id();
        let blocker = app
            .world_mut()
            .spawn((
                MissionEnemy,
                CharacterLook::default(),
                Transform::from_xyz(0.0, 0.0, 1.0),
                TacticalCombatSide::Enemy,
                TacticalCombatState::default(),
                SkeletonState::default(),
                AuthoritativePostureIntent::default(),
                QuickstepPush::default(),
                ReactiveDefenseAi {
                    requires_facing: false,
                },
                DefenseChances {
                    parry_chance: 1.0,
                    dodge_chance: 0.0,
                },
            ))
            .id();
        app.world_mut().trigger(FromClient {
            client_id: adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId::Client(
                attacker,
            ),
            message: MeleeActionRequest {
                strike_family: StrikeFamily::Swing,
                hand: AttackHand::Main,
                target: None,
                body_part: None,
            },
        });
        app.world_mut().flush();
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs(1));

        app.update();
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .get::<SkeletonState>(blocker)
                .unwrap()
                .action_kind(),
            SkeletonAction::Block
        );
        assert!(
            app.world()
                .get::<PendingDefenderResponse>(blocker)
                .is_some()
        );
    }

    #[test]
    fn aiming_package_faces_the_nearest_opponent() {
        let mut app = App::new();
        app.add_systems(Update, aim_at_nearest_opponent);
        let dodger = app
            .world_mut()
            .spawn((
                Player::default(),
                Transform::default(),
                TacticalCombatSide::Enemy,
                TacticalCombatState::default(),
                CharacterLook::default(),
                AimAtNearestOpponentAi,
            ))
            .id();
        app.world_mut().spawn((
            Player::default(),
            Transform::from_xyz(2.0, 0.0, 0.0),
            TacticalCombatSide::Party,
            TacticalCombatState::default(),
        ));

        app.update();

        let look = app.world().get::<CharacterLook>(dodger).unwrap();
        assert!((look.yaw + std::f32::consts::FRAC_PI_2).abs() < 0.0001);
    }

    #[test]
    fn recovery_package_starts_authored_get_up_without_skipping_upright() {
        let mut app = App::new();
        app.init_resource::<TacticalCombatConfig>();
        app.add_systems(Update, recover_to_upright);
        let prone = app
            .world_mut()
            .spawn((
                TacticalCombatState::default(),
                SkeletonState::default().with_body_state(BodyState::Prone),
                RecoverToUprightAi,
            ))
            .id();

        app.update();

        let skeleton = app.world().get::<SkeletonState>(prone).unwrap();
        assert_eq!(skeleton.body(), BodyState::Prone);
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::ProneToUpright
        );
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

        for _ in 0..8 {
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
        let combat_config = TacticalCombatConfig::default();
        let quickstep_distance = quickstep_target_displacement_metres(
            combat_config
                .movement
                .motor
                .reference_quickstep_leg_length_metres,
            &combat_config.movement.motor,
        );
        assert!(
            separation
                <= maximum_melee_lunge_range(
                    CharacterDimensions::default().arm_reach_metres,
                    KATZBALGER_REACH,
                    quickstep_distance,
                ),
            "AI stopped outside reachable lunge range: {separation}"
        );
        assert!(
            separation
                > melee_interaction_range(
                    CharacterDimensions::default().arm_reach_metres,
                    KATZBALGER_REACH,
                ),
            "AI should commit its attack while the lunge still has a gap to close: {separation}"
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
            .init_resource::<TacticalCombatConfig>()
            .init_resource::<RecordedAttacks>()
            .add_observer(record_attack)
            .add_observer(apply_deterministic_test_hit)
            .add_observer(crate::combat::on_melee_attack_started)
            .add_systems(
                Update,
                (
                    crate::combat::update_tactical_combat_state,
                    drive_offensive_combat_ai,
                    crate::combat::resolve_pending_melee_contacts,
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
                incapacitated_enemies: u32::from(enemy_defeated),
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
