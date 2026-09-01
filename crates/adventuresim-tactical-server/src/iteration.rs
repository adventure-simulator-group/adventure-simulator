//! In-process, fixed-step tactical-server melee duel harness.

use std::time::Duration;

use adventuresim_core::{
    autoresolve::{Combatant, MeleeIterationBuild, body_part_index},
    combat::HUMANOID_COLLISION_RADIUS_METRES,
};
#[cfg(test)]
use adventuresim_core::{
    combat::MeleeContactClassification, item_catalog_schema::EquipmentMaterial,
};
use adventuresim_tactical_core::{physics::AdventureSimulatorPhysicsPlugin, prelude::*};
use adventuresim_tactical_netcode::replication::AdventureSimulatorReplicationPlugin;
use bevy::{prelude::*, time::TimeUpdateStrategy};

mod equipment;
mod logging;
use equipment::spawn_equipment;
pub use logging::*;

use crate::{
    bot::{BotPlugin, CombatantBehaviorPackages, MissionEnemy},
    combat::CombatPlugin,
    equipment::TacticalEquipmentPlugin,
    player_projection::{
        AuthoritativeMovementIntent, AuthoritativePostureIntent,
        restore_authoritative_movement_intent, trace_authoritative_quickstep_after_collision,
        update_attack_facing_targets, update_character_motion_snapshots,
        update_skeleton_locomotion,
    },
};

const TACTICAL_TICK_HZ: u64 = 64;
const TACTICAL_TICK_SECONDS: f32 = 1.0 / TACTICAL_TICK_HZ as f32;
const MAX_DUEL_SECONDS: u64 = 180;

pub fn resolve_tactical_server_melee_duel(
    left: &MeleeIterationBuild,
    right: &MeleeIterationBuild,
    seed: u64,
) -> TacticalMeleeOutcome {
    let mut app = tactical_iteration_app(seed);
    let left_entity = spawn_combatant(
        app.world_mut(),
        left,
        TacticalCombatSide::Party,
        Vec3::new(0.0, 0.95, -2.0),
    );
    let right_entity = spawn_combatant(
        app.world_mut(),
        right,
        TacticalCombatSide::Enemy,
        Vec3::new(0.0, 0.95, 2.0),
    );
    let maximum_ticks = MAX_DUEL_SECONDS * TACTICAL_TICK_HZ;
    let mut final_tick = maximum_ticks;
    for tick in 0..maximum_ticks {
        app.world_mut().resource_mut::<IterationClock>().tick = tick;
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs_f32(TACTICAL_TICK_SECONDS));
        app.update();
        let left_down = combatant_defeated(app.world(), left_entity);
        let right_down = combatant_defeated(app.world(), right_entity);
        if left_down || right_down {
            final_tick = tick;
            break;
        }
    }
    let left_down = combatant_defeated(app.world(), left_entity);
    let right_down = combatant_defeated(app.world(), right_entity);
    let resolution = tactical_duel_resolution(left_down, right_down, left.name, right.name);
    let final_center_separation_metres = app
        .world()
        .get::<Transform>(left_entity)
        .zip(app.world().get::<Transform>(right_entity))
        .map_or(f32::NAN, |(left, right)| {
            left.translation.xz().distance(right.translation.xz())
        });
    let log = app.world_mut().remove_resource::<IterationLog>().unwrap();
    TacticalMeleeOutcome {
        seed,
        resolution,
        simulated_ticks: final_tick + 1,
        simulated_seconds: (final_tick + 1) as f32 * TACTICAL_TICK_SECONDS,
        initial_center_separation_metres: 4.0,
        final_center_separation_metres,
        attack_starts: log.attack_starts,
        resolved_attacks: log.events.len() as u32,
        events: log.events,
        decision_events: log.decision_events,
        condition_events: log.condition_events,
        wound_events: log.wound_events,
    }
}

fn tactical_duel_resolution(
    left_down: bool,
    right_down: bool,
    left_name: &str,
    right_name: &str,
) -> TacticalDuelResolution {
    match (left_down, right_down) {
        (false, true) => TacticalDuelResolution::Victory {
            victor: left_name.to_owned(),
        },
        (true, false) => TacticalDuelResolution::Victory {
            victor: right_name.to_owned(),
        },
        (true, true) => TacticalDuelResolution::MutualIncapacitation,
        (false, false) => TacticalDuelResolution::Timeout,
    }
}

fn combatant_defeated(world: &World, entity: Entity) -> bool {
    world
        .get::<TacticalCombatState>(entity)
        .is_some_and(TacticalCombatState::is_incapacitated)
        || world.get::<crate::bot::CombatantYielded>(entity).is_some()
}

fn tactical_iteration_app(seed: u64) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::transform::TransformPlugin,
        AdventureSimulatorReplicationPlugin,
        AdventureSimulatorPhysicsPlugin {
            enable_simulation: true,
            enable_presentation_simulation: false,
        },
        CombatPlugin,
        TacticalEquipmentPlugin,
        BotPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        TACTICAL_TICK_SECONDS,
    )))
    .insert_resource(Time::<()>::default())
    .insert_resource(IterationClock { tick: 0 })
    .insert_resource(IterationLog::default())
    .insert_resource(crate::bot::CombatRandom::seeded(seed))
    .add_observer(crate::player_projection::on_player_added)
    .add_observer(record_attack_start)
    .add_observer(record_attack_committed_to_defense)
    .add_observer(record_attack_transformed_by_defense)
    .add_observer(record_continuation_decision)
    .add_observer(record_defense_resolution)
    .add_observer(record_resolved_attack)
    .add_systems(
        FixedPostUpdate,
        (
            restore_authoritative_movement_intent
                .before(AdventureSimulatorPhysicsSet::ApplyCharacterMotor),
            (
                trace_authoritative_quickstep_after_collision,
                update_attack_facing_targets,
                update_skeleton_locomotion,
                update_character_motion_snapshots,
            )
                .chain()
                .after(AhoySystems::MoveCharacters),
        ),
    )
    .add_systems(
        Update,
        record_condition_changes.after(crate::combat::CombatSet::Condition),
    );
    app.finish();
    app.cleanup();
    app.world_mut().spawn((
        Name::new("Melee iteration ground"),
        RigidBody::Static,
        Collider::cuboid(40.0, 0.5, 40.0),
        Transform::from_xyz(0.0, -0.25, 0.0),
    ));
    app
}

fn spawn_combatant(
    world: &mut World,
    build: &MeleeIterationBuild,
    side: TacticalCombatSide,
    position: Vec3,
) -> Entity {
    let source = &build.combatant;
    let entity = world
        .spawn((
            Player {
                name: build.name.to_owned(),
            },
            CharacterId(source.id),
            tactical_skills(source),
            tactical_limbs(source),
            TacticalAttributes(source.attributes.clone()),
            Stats {
                calories_used: source.essentials.calories_used_today,
                focus: source.essentials.focus_level,
            },
            TacticalCombatState {
                starting_incapacitation: source.starting_incapacitation,
                starting_blood_fraction: source.starting_blood_fraction,
                ..default()
            },
            crate::combat::TacticalWounds::default(),
            side,
            Transform::from_translation(position),
            CharacterLook::default(),
            input::AccumulatedInput::default(),
            SkeletonState::default(),
            CharacterDimensions::default(),
        ))
        .id();
    world.entity_mut(entity).insert((
        Collider::cylinder(HUMANOID_COLLISION_RADIUS_METRES, 1.9),
        LinearVelocity::ZERO,
        crate::combat::MeleeAttackAuthority::default(),
        AuthoritativeMovementIntent::default(),
        AuthoritativePostureIntent::default(),
        QuickstepPush::default(),
        MovementPace::default(),
        MissionEnemy,
        CombatantBehaviorPackages::standard_combat(&TacticalCombatConfig::default()),
    ));
    spawn_equipment(world, entity, build);
    entity
}

fn tactical_skills(source: &Combatant) -> Skills {
    let skills = &source.skills;
    Skills {
        polearm_hours: skills.polearm_hours,
        axe_hours: skills.axe_hours,
        bludgeon_hours: skills.bludgeon_hours,
        sword_hours: skills.sword_hours,
        knife_hours: skills.knife_hours,
        dodge_hours: skills.dodge_hours,
        block_hours: skills.block_hours,
        bow_hours: 0.0,
        crossbow_hours: 0.0,
        firearm_hours: 0.0,
        throw_hours: skills.throw_hours,
        will_hours: skills.will_hours,
        insight_hours: skills.insight_hours,
        charm_hours: skills.charm_hours,
        command_hours: skills.command_hours,
        deception_hours: skills.deception_hours,
        physiology_hours: skills.physiology_hours,
        religion_hours: skills.religion_hours,
        bestiary_human_hours: skills.bestiary_hours.human,
        surgery_hours: skills.surgery_hours,
        stealth_hours: skills.stealth_hours,
        balance_hours: skills.balance_hours,
        tailoring_hours: skills.tailoring_hours,
        smithing_hours: skills.smithing_hours,
        ..Skills::default()
    }
}

fn tactical_limbs(source: &Combatant) -> Limbs {
    Limbs {
        body_weight_kg: source.body.weight_kg,
        left_arm: source.body.health(BodyPart::LeftArm),
        right_arm: source.body.health(BodyPart::RightArm),
        left_leg: source.body.health(BodyPart::LeftLeg),
        right_leg: source.body.health(BodyPart::RightLeg),
        chest: source.body.health(BodyPart::Chest),
        stomach: source.body.health(BodyPart::Stomach),
        head: source.body.health(BodyPart::Head),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_core::autoresolve::melee_iteration_roster;

    #[test]
    fn duel_uses_production_bot_and_combat_observers() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let outcome = resolve_tactical_server_melee_duel(&john, &opponents[1], 2);
        assert!(outcome.attack_starts > 0, "{outcome:?}");
        assert!(outcome.resolved_attacks > 0);
        assert_eq!(outcome.resolved_attacks as usize, outcome.events.len());
        let attempted = outcome
            .decision_events
            .iter()
            .filter(|event| {
                event.decision == TacticalDecision::Dodge
                    && event.status == TacticalDecisionStatus::Attempted
            })
            .count();
        let validated = outcome
            .decision_events
            .iter()
            .filter(|event| {
                matches!(
                    event.status,
                    TacticalDecisionStatus::Accepted | TacticalDecisionStatus::Rejected
                ) && event.decision == TacticalDecision::Dodge
            })
            .count();
        assert!(attempted > 0);
        assert_eq!(attempted, validated);
        assert!(
            outcome
                .events
                .iter()
                .any(|event| { event.defender_decision != TacticalDecision::NoDefense })
        );
        assert!(outcome.initial_center_separation_metres > 3.0);
        assert!(
            outcome
                .condition_events
                .iter()
                .any(|event| { event.cause == "oxygen_debt_joules" && event.delta > 0.0 })
        );
        assert!(!outcome.wound_events.is_empty());
        assert!(
            outcome
                .condition_events
                .iter()
                .any(|event| { event.cause == "blood_loss" && event.delta > 0.0 })
        );
        assert!(outcome.decision_events.iter().any(|event| {
            event.decision == TacticalDecision::Attack
                && event.tick > 0
                && event
                    .center_separation_metres
                    .is_some_and(|distance| distance < outcome.initial_center_separation_metres)
        }));
    }

    #[test]
    fn fixed_step_advances_production_skeleton_projection() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let mut app = tactical_iteration_app(1);
        let john_entity = spawn_combatant(
            app.world_mut(),
            &john,
            TacticalCombatSide::Party,
            Vec3::new(0.0, 0.95, -2.0),
        );
        spawn_combatant(
            app.world_mut(),
            &opponents[0],
            TacticalCombatSide::Enemy,
            Vec3::new(0.0, 0.95, 2.0),
        );
        for tick in 0..4 {
            app.world_mut().resource_mut::<IterationClock>().tick = tick;
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(Duration::from_secs_f32(TACTICAL_TICK_SECONDS));
            app.update();
        }
        let skeleton = app.world().get::<SkeletonState>(john_entity).unwrap();
        assert!(skeleton.locomotion_sample_tick > 0);
    }

    #[test]
    fn fixed_seed_replays_identically() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let first = resolve_tactical_server_melee_duel(&john, &opponents[0], 22);
        let second = resolve_tactical_server_melee_duel(&john, &opponents[0], 22);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
    }

    #[test]
    fn parry_consumes_the_committed_attack_before_any_later_riposte() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let outcome = resolve_tactical_server_melee_duel(&john, &opponents[0], 4);
        let canceled = outcome
            .decision_events
            .iter()
            .find(|event| event.status == TacticalDecisionStatus::CanceledForDefense)
            .expect("the fixed duel includes a reciprocal parry");
        let next_start = outcome
            .decision_events
            .iter()
            .filter(|event| {
                event.combatant == canceled.combatant
                    && event.status == TacticalDecisionStatus::Started
                    && event.tick > canceled.tick
            })
            .map(|event| event.tick)
            .min()
            .unwrap_or(u64::MAX);
        assert!(outcome.events.iter().all(|event| {
            event.attacker != canceled.combatant
                || event.tick <= canceled.tick
                || event.tick >= next_start
        }));
        assert!(outcome.condition_events.iter().any(|event| {
            event.combatant == canceled.combatant
                && event.cause == "local_action_fatigue"
                && event.delta > 0.0
        }));
    }

    #[test]
    fn committed_sword_uses_buckler_and_is_transformed_before_contact() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let outcome = resolve_tactical_server_melee_duel(&john, &opponents[0], 1);
        assert!(outcome.events.iter().any(|event| {
            event.defender == "Shield Militiaman"
                && event.defensive_implement.as_deref() == Some("buckler")
                && event.defender_decision == TacticalDecision::Block
        }));
        let transformed = outcome
            .decision_events
            .iter()
            .find(|event| event.status == TacticalDecisionStatus::TransformedByDefense)
            .expect("the fixed overlap uses the offhand buckler");
        assert_eq!(transformed.combatant, "Shield Militiaman");
        assert_eq!(
            transformed.cause,
            Some("offhand_block_reduced_attack_power")
        );
        assert!(!outcome.decision_events.iter().any(|event| {
            event.attack_key == transformed.attack_key
                && event.status == TacticalDecisionStatus::CanceledForDefense
        }));
        assert!(outcome.events.iter().any(|event| {
            event.attacker == transformed.combatant && event.tick > transformed.tick
        }));
    }

    #[test]
    fn production_dodge_can_redirect_committed_path_to_a_limb() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        assert!((1..=8).any(|seed| {
            resolve_tactical_server_melee_duel(&john, &opponents[0], seed)
                .events
                .iter()
                .any(|event| {
                    event.defender_decision == TacticalDecision::Dodge
                        && event.redirected_from_body_part.is_some()
                        && event.contact_energy_joules > 0.0
                })
        }));
    }

    #[test]
    fn production_polearm_seeks_authored_head_band_and_uses_live_contact_revalidation() {
        let (john, opponents) = melee_iteration_roster().unwrap();
        let veteran = &opponents[2];
        let outcomes = (1..=8)
            .map(|seed| resolve_tactical_server_melee_duel(&john, veteran, seed))
            .collect::<Vec<_>>();
        let preferred_measures = outcomes
            .iter()
            .flat_map(|outcome| &outcome.decision_events)
            .filter(|decision| decision.combatant == veteran.name)
            .filter_map(|decision| decision.preferred_melee_measure_metres)
            .collect::<Vec<_>>();
        assert!(
            outcomes
                .iter()
                .flat_map(|outcome| &outcome.decision_events)
                .any(|decision| {
                    decision.combatant == veteran.name
                        && decision.decision == TacticalDecision::Attack
                        && decision.status == TacticalDecisionStatus::Started
                        && decision
                            .preferred_melee_measure_metres
                            .is_some_and(|measure| (measure - 1.92).abs() < 0.01)
                }),
            "{preferred_measures:?}"
        );
        assert!(
            outcomes
                .iter()
                .flat_map(|outcome| &outcome.events)
                .any(|event| {
                    event.attacker == veteran.name
                        && event.contact_classification
                            == MeleeContactClassification::IntendedSurface
                        && event.contact_material == Some(EquipmentMaterial::RoughSteel)
                        && event.contact_energy_fraction <= 1.0
                })
        );
    }

    #[test]
    fn pursued_melee_bot_with_disabled_weapon_arm_withdraws_then_yields() {
        let (mut john, opponents) = melee_iteration_roster().unwrap();
        john.combatant.body.health.fill(10.0);
        john.combatant.body.health[body_part_index(BodyPart::RightArm)] = 0.0;
        let outcome = resolve_tactical_server_melee_duel(&john, &opponents[0], 3);
        let decisions = outcome
            .decision_events
            .iter()
            .filter(|event| event.combatant == john.name)
            .map(|event| event.decision)
            .collect::<Vec<_>>();
        let withdraw = decisions
            .iter()
            .position(|decision| *decision == TacticalDecision::Withdraw)
            .expect("disabled bot withdraws");
        let yielded = decisions
            .iter()
            .position(|decision| *decision == TacticalDecision::Yield)
            .expect("continued pursuit causes yield");
        assert!(withdraw < yielded);
        assert_eq!(
            outcome.resolution,
            TacticalDuelResolution::Victory {
                victor: opponents[0].name.to_owned(),
            }
        );
        assert!(
            outcome
                .events
                .iter()
                .all(|event| event.attacker != john.name)
        );
    }

    #[test]
    fn simultaneous_terminal_state_is_a_mutual_incapacitation_not_a_timeout() {
        assert_eq!(
            tactical_duel_resolution(true, true, "left", "right"),
            TacticalDuelResolution::MutualIncapacitation
        );
    }

    #[test]
    fn unresolved_healthy_combatants_at_the_tick_limit_are_a_timeout() {
        assert_eq!(
            tactical_duel_resolution(false, false, "left", "right"),
            TacticalDuelResolution::Timeout
        );
    }
}
