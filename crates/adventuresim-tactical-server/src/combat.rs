mod authority;
mod condition;
mod consequence;
mod ingress;
mod melee;
mod protocol;
mod ragdoll;
mod ranged;

use adventuresim_core::item_references::ARROW_ID;
pub(crate) use adventuresim_tactical_core::player::TacticalCombatSide;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendTargets, ServerTriggerExt, ToClients},
    message::{DefendRequest, MeleeActionRequest, RangedActionRequest, SuccessfulAttackResponse},
};
use bevy::prelude::*;
use std::{collections::HashMap, num::NonZeroU32};

use crate::player_projection::{
    AuthoritativeMovementIntent, PlayerProjectionSet, begin_attack_facing,
    begin_authoritative_quickstep,
};
pub(crate) use authority::{
    CombatDuration, CombatInstant, MeleeAttackAuthority, RangedAttackAuthority, ReportedPrecision,
};
use authority::{ValidatedRangedImpact, validate_melee_intent_cheap, validate_ranged_intent};
pub(crate) use condition::update_tactical_combat_state;
#[cfg(test)]
pub(crate) use consequence::apply_transient_attack_result;
use consequence::{apply_melee_attack_result, record_party_ammunition_use};
#[cfg(test)]
use consequence::{
    attacker_weapon_contact_matches, defender_equipment_contact_matches, record_party_injury,
};
pub(crate) use ingress::apply_defend_intent;
pub(crate) use ingress::melee_body_part_lunge_delay;
use ingress::{
    authoritative_line_of_sight, on_defender_response_request, on_melee_action_request,
    on_ranged_action_request, on_ranged_attack_started, resolve_defender_response,
};
pub(crate) use ingress::{on_melee_attack_started, resolve_pending_melee_contacts};
use melee::resolve_melee_attack;
pub(crate) use protocol::{
    DefendIntent, MeleeAttackIntent, MeleeAttackStartedIntent, PendingDefenderResponse,
    PendingMeleeContact, RangedAttackIntent, RangedAttackStartedIntent,
};
use ragdoll::update_authoritative_ragdoll_lifecycle;
use ranged::resolve_ranged_attack;

#[derive(Clone, Debug)]
pub(crate) struct AppliedTacticalInjury {
    pub body_part: BodyPart,
    pub cut_damage: f32,
    pub blunt_damage: f32,
    pub max_single_hit_blunt_damage: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AccumulatedPartyConsequence {
    pub injuries: Vec<AppliedTacticalInjury>,
    pub blood_loss_fraction: f32,
    pub ammunition_used: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct AccumulatedEquipmentContact {
    pub character_id: CharacterId,
    pub inventory_item_id: u64,
    pub contact_stress: f32,
    pub defender_equipment: bool,
}

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct TacticalConsequenceAccumulator {
    pub party: HashMap<CharacterId, AccumulatedPartyConsequence>,
    pub equipment_contacts: Vec<AccumulatedEquipmentContact>,
}

fn remaining_ammo_after_shot(quantity: NonZeroU32) -> Option<NonZeroU32> {
    NonZeroU32::new(quantity.get() - 1)
}

#[derive(Event, Clone, Copy, Debug)]
struct ApplyMeleeAttackResult {
    attacker: Entity,
    target: Entity,
    body_part: BodyPart,
    result: AttackResult,
    attacker_weapon_slot: EquipSlot,
    defender_parry_slot: Option<EquipSlot>,
    attacker_weapon_contact: bool,
    impact_recipient: Entity,
    impact_velocity_change: Vec3,
}

/// Contact energy describes the local collision, not whole-body kinetic
/// energy. This transfer scale makes John Fabelgeist's ordinary ~49.5 J punch
/// move an 80 kg equipped bandit about 0.25 m under the tactical controller's
/// standard grounded friction.
/// Converts combat contact energy into an explicit physical delta-v. Combat
/// resolution historically calls the energy-like value `contact_force`; this
/// seam prevents those joules from being mistaken for either newtons or an
/// already mass-normalized impulse.
fn hit_velocity_change(
    result: AttackResult,
    attacker_position: Vec3,
    defender_position: Vec3,
    attacker_mass_kg: f32,
    defender_mass_kg: f32,
    config: &ImpactConfig,
) -> (bool, Vec3) {
    let (hits_attacker, contact_energy, mass) = match result {
        AttackResult::ToAttacker { contact_force, .. } => {
            (true, contact_force.max(0.0), attacker_mass_kg)
        }
        AttackResult::ToDefender { contact_force, .. } => {
            (false, contact_force.max(0.0), defender_mass_kg)
        }
    };
    if !contact_energy.is_finite() || contact_energy <= f32::EPSILON {
        return (hits_attacker, Vec3::ZERO);
    }
    let horizontal = (defender_position - attacker_position)
        .xz()
        .normalize_or_zero();
    if horizontal == Vec2::ZERO {
        return (hits_attacker, Vec3::ZERO);
    }
    let horizontal = if hits_attacker {
        -horizontal
    } else {
        horizontal
    };
    let direction = Vec3::new(horizontal.x, 0.0, horizontal.y);
    let speed = ((2.0 * contact_energy / mass.max(1.0)).sqrt() * config.whole_body_velocity_scale)
        .min(config.maximum_velocity_change_metres_per_second);
    (hits_attacker, direction * speed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RangedIntentRejection {
    SelfTarget,
    MissingSide,
    MissingCombatState,
    FriendlyTarget,
    Incapacitated,
    NotRanged,
    OutOfRange,
    OutsideAimCone,
    Windup,
    BlockedLineOfSight,
}

#[derive(Clone, Copy)]
struct RangedIntentFacts {
    attacker: Entity,
    target: Option<Entity>,
    attacker_side: Option<TacticalCombatSide>,
    target_side: Option<TacticalCombatSide>,
    attacker_incapacitated: Option<bool>,
    target_incapacitated: Option<bool>,
    reported_precision: ReportedPrecision,
    weapon_is_ranged: bool,
    weapon_range: f32,
    range_latency_tolerance: f32,
    separation: Option<f32>,
    target_in_aim_cone: Option<bool>,
    authority_permits: bool,
    body_part: BodyPart,
    attacker_position: Vec3,
    target_position: Option<Vec3>,
    attacker_yaw: f32,
    target_yaw: Option<f32>,
}

fn ranged_target_in_aim_cone(
    yaw: f32,
    attacker: Vec3,
    target: Vec3,
    half_angle_degrees: f32,
) -> bool {
    let offset = target.xz() - attacker.xz();
    let Some(direction) = offset.try_normalize() else {
        return false;
    };
    let forward = Vec2::new(-yaw.sin(), -yaw.cos());
    direction.dot(forward) >= half_angle_degrees.to_radians().cos()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeleeIntentRejection {
    SelfTarget,
    MissingSide,
    MissingCombatState,
    FriendlyTarget,
    Incapacitated,
    Unarmed,
    OutOfRange,
    Windup,
    BlockedLineOfSight,
}

#[derive(Clone, Copy)]
struct MeleeIntentFacts {
    attacker: Entity,
    target: Entity,
    attacker_side: Option<TacticalCombatSide>,
    target_side: Option<TacticalCombatSide>,
    attacker_incapacitated: Option<bool>,
    target_incapacitated: Option<bool>,
    reported_precision: ReportedPrecision,
    arm_reach: f32,
    weapon_reach: f32,
    range_latency_tolerance: f32,
    separation: f32,
    authority_permits: bool,
    body_part: BodyPart,
    attacker_position: Vec3,
    target_position: Vec3,
    attacker_yaw: f32,
    target_yaw: f32,
}

fn validate_melee_line_of_sight(line_of_sight: bool) -> Result<(), MeleeIntentRejection> {
    line_of_sight
        .then_some(())
        .ok_or(MeleeIntentRejection::BlockedLineOfSight)
}

pub struct CombatPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CombatSet {
    Condition,
}

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TacticalConsequenceAccumulator>()
            .init_resource::<TacticalCombatConfig>()
            .add_observer(on_melee_action_request)
            .add_observer(on_ranged_action_request)
            .add_observer(on_ranged_attack_started)
            .add_observer(on_melee_attack_started)
            .add_observer(resolve_melee_attack)
            .add_observer(resolve_ranged_attack)
            .add_observer(apply_melee_attack_result)
            .add_observer(on_defender_response_request)
            .add_observer(apply_defend_intent)
            .configure_sets(Update, CombatSet::Condition)
            .add_systems(
                Update,
                (
                    update_tactical_combat_state
                        .in_set(CombatSet::Condition)
                        .after(PlayerProjectionSet::Spawn),
                    update_authoritative_ragdoll_lifecycle.after(CombatSet::Condition),
                    resolve_pending_melee_contacts.after(CombatSet::Condition),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    fn default_impact_config() -> ImpactConfig {
        TacticalCombatConfig::default().realtime_authority.impact
    }

    fn valid_facts(world: &mut World) -> MeleeIntentFacts {
        MeleeIntentFacts {
            attacker: world.spawn_empty().id(),
            target: world.spawn_empty().id(),
            attacker_side: Some(TacticalCombatSide::Party),
            target_side: Some(TacticalCombatSide::Enemy),
            attacker_incapacitated: Some(false),
            target_incapacitated: Some(false),
            reported_precision: ReportedPrecision::new(1.0).unwrap(),
            arm_reach: 0.55,
            weapon_reach: 0.8,
            range_latency_tolerance: 0.25,
            separation: 1.2,
            authority_permits: true,
            body_part: BodyPart::Chest,
            attacker_position: Vec3::ZERO,
            target_position: Vec3::X,
            attacker_yaw: 0.0,
            target_yaw: 0.0,
        }
    }

    fn valid_ranged_facts(world: &mut World) -> RangedIntentFacts {
        RangedIntentFacts {
            attacker: world.spawn_empty().id(),
            target: Some(world.spawn_empty().id()),
            attacker_side: Some(TacticalCombatSide::Party),
            target_side: Some(TacticalCombatSide::Enemy),
            attacker_incapacitated: Some(false),
            target_incapacitated: Some(false),
            reported_precision: ReportedPrecision::new(1.0).unwrap(),
            weapon_is_ranged: true,
            weapon_range: 120.0,
            range_latency_tolerance: 0.5,
            separation: Some(30.0),
            target_in_aim_cone: Some(true),
            authority_permits: true,
            body_part: BodyPart::Chest,
            attacker_position: Vec3::ZERO,
            target_position: Some(Vec3::X),
            attacker_yaw: 0.0,
            target_yaw: Some(0.0),
        }
    }

    #[test]
    fn authoritative_gate_rejects_invalid_relationship_state_and_geometry() {
        let mut world = World::new();
        let valid = valid_facts(&mut world);
        assert!(validate_melee_intent_cheap(valid).is_ok());
        assert!(
            validate_melee_intent_cheap(MeleeIntentFacts {
                weapon_reach: 0.0,
                separation: valid.arm_reach,
                ..valid
            })
            .is_ok(),
            "fists use anatomical arm reach with zero weapon contribution"
        );
        assert_eq!(
            validate_melee_intent_cheap(MeleeIntentFacts {
                weapon_reach: 0.0,
                separation: valid.arm_reach + valid.range_latency_tolerance + 0.01,
                ..valid
            })
            .unwrap_err(),
            MeleeIntentRejection::OutOfRange
        );
        assert_eq!(validate_melee_line_of_sight(true), Ok(()));
        assert_eq!(
            validate_melee_line_of_sight(false),
            Err(MeleeIntentRejection::BlockedLineOfSight)
        );

        let cases = [
            (
                MeleeIntentFacts {
                    target: valid.attacker,
                    ..valid
                },
                MeleeIntentRejection::SelfTarget,
            ),
            (
                MeleeIntentFacts {
                    attacker_side: None,
                    ..valid
                },
                MeleeIntentRejection::MissingSide,
            ),
            (
                MeleeIntentFacts {
                    target_side: valid.attacker_side,
                    ..valid
                },
                MeleeIntentRejection::FriendlyTarget,
            ),
            (
                MeleeIntentFacts {
                    attacker_incapacitated: None,
                    ..valid
                },
                MeleeIntentRejection::MissingCombatState,
            ),
            (
                MeleeIntentFacts {
                    target_incapacitated: Some(true),
                    ..valid
                },
                MeleeIntentRejection::Incapacitated,
            ),
            (
                MeleeIntentFacts {
                    separation: 4.0,
                    ..valid
                },
                MeleeIntentRejection::OutOfRange,
            ),
            (
                MeleeIntentFacts {
                    authority_permits: false,
                    ..valid
                },
                MeleeIntentRejection::Windup,
            ),
        ];
        for (facts, expected) in cases {
            assert_eq!(validate_melee_intent_cheap(facts).unwrap_err(), expected);
        }
    }

    #[test]
    fn ranged_gate_validates_authority_equipment_ammo_and_target() {
        let mut world = World::new();
        let valid = valid_ranged_facts(&mut world);
        assert!(validate_ranged_intent(valid).is_ok());
        assert!(
            validate_ranged_intent(RangedIntentFacts {
                reported_precision: ReportedPrecision::new(99.0).unwrap(),
                ..valid
            })
            .is_ok()
        );
        let cases = [
            (
                RangedIntentFacts {
                    target: Some(valid.attacker),
                    ..valid
                },
                RangedIntentRejection::SelfTarget,
            ),
            (
                RangedIntentFacts {
                    target_side: valid.attacker_side,
                    ..valid
                },
                RangedIntentRejection::FriendlyTarget,
            ),
            (
                RangedIntentFacts {
                    attacker_incapacitated: Some(true),
                    ..valid
                },
                RangedIntentRejection::Incapacitated,
            ),
            (
                RangedIntentFacts {
                    weapon_is_ranged: false,
                    ..valid
                },
                RangedIntentRejection::NotRanged,
            ),
            (
                RangedIntentFacts {
                    separation: Some(121.0),
                    ..valid
                },
                RangedIntentRejection::OutOfRange,
            ),
            (
                RangedIntentFacts {
                    authority_permits: false,
                    ..valid
                },
                RangedIntentRejection::Windup,
            ),
        ];
        for (facts, expected) in cases {
            assert_eq!(validate_ranged_intent(facts).unwrap_err(), expected);
        }

        assert!(
            validate_ranged_intent(RangedIntentFacts {
                target: None,
                target_side: None,
                target_incapacitated: None,
                separation: None,
                ..valid
            })
            .is_ok(),
            "a fired miss still passes the firing gate and consumes ammo"
        );
    }

    #[test]
    fn ranged_aim_cone_accepts_forward_and_rejects_behind() {
        let origin = Vec3::ZERO;
        assert!(ranged_target_in_aim_cone(0.0, origin, Vec3::NEG_Z, 20.0));
        assert!(!ranged_target_in_aim_cone(0.0, origin, Vec3::Z, 20.0));

        let mut world = World::new();
        let valid = valid_ranged_facts(&mut world);
        assert!(validate_ranged_intent(valid).is_ok());
        assert_eq!(
            validate_ranged_intent(RangedIntentFacts {
                target_in_aim_cone: Some(false),
                ..valid
            })
            .unwrap_err(),
            RangedIntentRejection::OutsideAimCone
        );
    }

    #[test]
    fn ranged_rejections_happen_before_ammo_scan() {
        let resolver = include_str!("combat/ranged.rs");
        let validation = resolver
            .find("validate_ranged_intent(facts)")
            .expect("cheap validation");
        let authorization = resolver
            .find("authority.authorize_shot")
            .expect("one-shot authorization");
        let ammo_scan = resolver.find("q_ammo.iter().find").expect("ammo scan");
        assert!(validation < authorization && authorization < ammo_scan);
    }

    #[test]
    fn resolvers_only_use_authorized_payloads_after_consuming_authority() {
        let melee = include_str!("combat/melee.rs");
        let ranged = include_str!("combat/ranged.rs");

        assert!(
            !melee
                .split("authorize_attack")
                .nth(1)
                .unwrap()
                .contains("event.")
        );
        assert!(
            !ranged
                .split("authorize_shot")
                .nth(1)
                .unwrap()
                .contains("event.")
        );
    }

    #[test]
    fn ranged_ammo_consumption_and_receipt_are_bounded() {
        assert_eq!(remaining_ammo_after_shot(NonZeroU32::new(1).unwrap()), None);
        assert_eq!(
            remaining_ammo_after_shot(NonZeroU32::new(3).unwrap()).map(NonZeroU32::get),
            Some(2)
        );
        let mut consequences = TacticalConsequenceAccumulator::default();
        for _ in 0..=adventuresim_core::mission::MAX_TACTICAL_AMMUNITION_USED {
            record_party_ammunition_use(&mut consequences, CharacterId(7));
        }
        assert_eq!(
            consequences.party[&CharacterId(7)].ammunition_used,
            adventuresim_core::mission::MAX_TACTICAL_AMMUNITION_USED
        );
    }

    #[test]
    fn damage_clamps_limb_and_accumulates_blood_and_imbalance() {
        let mut attacker = TacticalCombatState::default();
        let mut defender = TacticalCombatState::default();
        let mut limbs = Limbs {
            chest: 0.2,
            ..default()
        };
        let applied = apply_transient_attack_result(
            &mut attacker,
            &mut limbs,
            &mut defender,
            AttackResult::ToDefender {
                cut_damage: 100.0,
                blunt_damage: 0.0,
                balance_damage: 0.4,
                contact_force: 100.0,
                armor_contact: false,
            },
            BodyPart::Chest,
        );
        assert_eq!(applied, Some((0.2, 0.0)), "receipt excludes raw overkill");
        assert_eq!(limbs.chest, 0.0);
        assert!((defender.blood_loss_fraction - 0.1).abs() < 0.0001);
        assert!((defender.imbalance - 0.4).abs() < 0.0001);

        apply_transient_attack_result(
            &mut attacker,
            &mut limbs,
            &mut defender,
            AttackResult::ToAttacker {
                balance_damage: 0.25,
                contact_force: 0.0,
                physical_contact: false,
            },
            BodyPart::Chest,
        );
        assert!((attacker.imbalance - 0.25).abs() < 0.0001);
    }

    #[test]
    fn repeated_hits_are_losslessly_aggregated_per_limb() {
        let mut consequences = TacticalConsequenceAccumulator::default();
        for _ in 0..100 {
            record_party_injury(
                &mut consequences,
                CharacterId(7),
                BodyPart::Chest,
                0.003,
                0.002,
            );
        }
        let consequence = &consequences.party[&CharacterId(7)];
        assert_eq!(consequence.injuries.len(), 1);
        assert!((consequence.injuries[0].cut_damage - 0.3).abs() < 0.0001);
        assert!((consequence.injuries[0].blunt_damage - 0.2).abs() < 0.0001);
        assert!((consequence.injuries[0].max_single_hit_blunt_damage - 0.002).abs() < 0.0001);
        assert!((consequence.blood_loss_fraction - 0.1515).abs() < 0.0001);
    }

    #[test]
    fn contact_provenance_uses_actual_weapon_and_parry_shield() {
        let attacker = Entity::from_bits(1);
        let defender = Entity::from_bits(2);
        assert!(!attacker_weapon_contact_matches(
            attacker,
            attacker,
            EquipSlot::HoldingLeft,
            EquipSlot::HoldingRight,
            false,
        ));
        assert!(attacker_weapon_contact_matches(
            attacker,
            attacker,
            EquipSlot::HoldingRight,
            EquipSlot::HoldingRight,
            true,
        ));
        assert!(defender_equipment_contact_matches(
            defender,
            defender,
            EquipSlot::HoldingLeft,
            Some(EquipSlot::HoldingLeft),
            true,
            false,
            true,
            false,
        ));
        assert!(!defender_equipment_contact_matches(
            defender,
            defender,
            EquipSlot::HoldingRight,
            Some(EquipSlot::HoldingLeft),
            false,
            false,
            true,
            false,
        ));
    }

    #[test]
    fn blood_pain_imbalance_and_recovery_share_autoresolve_rules() {
        assert!(combat_incapacitation(0.0, 1.0, 0.3, 0.0, 1.0, 0.0) >= 1.0);
        assert!(combat_incapacitation(0.2, 1.0, 0.0, 1.0, 0.0, 0.0) >= 1.0);
        assert!(combat_incapacitation(0.0, 1.0, 0.0, 0.0, 1.0, 1.0) >= 1.0);
        assert_eq!(recover_combat_imbalance(0.75, 2.0), 0.25);
        assert_eq!(recover_combat_imbalance(0.01, 1.0), 0.0);
    }

    #[test]
    fn imbalance_only_incapacitation_recovers_and_removes_marker() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .add_systems(Update, update_tactical_combat_state);
        let actor = app
            .world_mut()
            .spawn((
                Player::default(),
                TacticalCombatState {
                    imbalance: 1.01,
                    ..default()
                },
                input::AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
            ))
            .id();
        app.update();
        assert!(
            app.world()
                .entity(actor)
                .get::<TacticalCombatState>()
                .unwrap()
                .is_incapacitated()
        );
        assert_eq!(
            app.world()
                .entity(actor)
                .get::<input::AccumulatedInput>()
                .unwrap()
                .last_movement,
            None
        );

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs(2));
        app.update();
        assert!(
            !app.world()
                .entity(actor)
                .get::<TacticalCombatState>()
                .unwrap()
                .is_incapacitated()
        );
    }

    #[test]
    fn jog_holds_exhaustion_sprint_adds_it_and_rest_recovers_it() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .add_systems(Update, update_tactical_combat_state);
        let endurance = 3.0;
        let jog_speed = tactical_jog_speed(endurance);
        let actor = app
            .world_mut()
            .spawn((
                Player::default(),
                Attributes {
                    endurance,
                    left_leg_strength: 3.0,
                    right_leg_strength: 3.0,
                    ..default()
                },
                TacticalCombatState {
                    exhaustion: 0.25,
                    ..default()
                },
                input::AccumulatedInput { ..default() },
                AuthoritativeMovementIntent(Some(Vec2::Y)),
                MovementPace::Jog,
                // External physics velocity must not affect movement exertion.
                LinearVelocity(Vec3::new(jog_speed + 10.0, 0.0, 0.0)),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        let jog_exhaustion = app
            .world()
            .entity(actor)
            .get::<TacticalCombatState>()
            .unwrap()
            .exhaustion;
        assert!((jog_exhaustion - 0.25).abs() < f32::EPSILON);

        *app.world_mut()
            .entity_mut(actor)
            .get_mut::<MovementPace>()
            .unwrap() = MovementPace::Sprint;
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        let sprint_exhaustion = app
            .world()
            .entity(actor)
            .get::<TacticalCombatState>()
            .unwrap()
            .exhaustion;
        assert!(sprint_exhaustion > jog_exhaustion);

        app.world_mut()
            .entity_mut(actor)
            .get_mut::<AuthoritativeMovementIntent>()
            .unwrap()
            .0 = None;
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        let resting_exhaustion = app
            .world()
            .entity(actor)
            .get::<TacticalCombatState>()
            .unwrap()
            .exhaustion;
        assert!(resting_exhaustion < sprint_exhaustion);
    }

    #[test]
    fn contact_energy_becomes_calibrated_mass_normalized_directional_velocity() {
        let result = AttackResult::ToDefender {
            cut_damage: 0.0,
            blunt_damage: 1.0,
            balance_damage: 0.0,
            contact_force: 140.0,
            armor_contact: false,
        };
        let (hits_attacker, velocity_change) = hit_velocity_change(
            result,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 2.0),
            70.0,
            70.0,
            &default_impact_config(),
        );
        assert!(!hits_attacker);
        assert!((velocity_change.length() - 3.86).abs() < 1.0e-4);
        assert!(velocity_change.z > 0.0);
        assert_eq!(velocity_change.y, 0.0);
    }

    fn default_ground_stopping_distance(mut speed: f32, mass_kg: f32) -> f32 {
        const TICK_SECONDS: f32 = 1.0 / 64.0;
        let config = TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        let braking_acceleration = (motor.reference_ground_braking_force_newtons / mass_kg)
            .min(motor.gravity_metres_per_second_squared * motor.traction_coefficient);
        let mut distance = 0.0;
        while speed >= 0.001 {
            speed = (speed - braking_acceleration * TICK_SECONDS).max(0.0);
            distance += speed * TICK_SECONDS;
        }
        distance
    }

    #[test]
    fn ordinary_unarmed_hit_moves_equipped_bandit_about_quarter_metre() {
        let result = AttackResult::ToDefender {
            cut_damage: 0.0,
            blunt_damage: 0.1,
            balance_damage: 0.4,
            contact_force: 49.4667,
            armor_contact: false,
        };
        let (_, velocity_change) = hit_velocity_change(
            result,
            Vec3::ZERO,
            Vec3::Z,
            80.0,
            80.0,
            &default_impact_config(),
        );
        let stopping_distance =
            default_ground_stopping_distance(velocity_change.xz().length(), 80.0);

        assert!(
            (0.23..=0.27).contains(&stopping_distance),
            "ordinary punch stopping distance was {stopping_distance:.3} m"
        );
    }

    #[test]
    fn parry_recoil_points_back_at_attacker() {
        let result = AttackResult::ToAttacker {
            balance_damage: 1.0,
            contact_force: 40.0,
            physical_contact: true,
        };
        let (hits_attacker, velocity_change) = hit_velocity_change(
            result,
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            80.0,
            60.0,
            &default_impact_config(),
        );
        assert!(hits_attacker);
        assert!(velocity_change.x < 0.0);
    }
}
