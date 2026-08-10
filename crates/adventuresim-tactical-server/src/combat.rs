mod authority;
mod condition;
mod consequence;
mod ingress;
mod melee;
mod protocol;
mod ranged;

use adventuresim_core::item_references::ARROW_ID;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendTargets, ServerTriggerExt, ToClients},
    message::{DefendRequest, MeleeActionRequest, RangedActionRequest, SuccessfulAttackResponse},
};
use bevy::prelude::*;
use std::{collections::HashMap, num::NonZeroU32, time::Duration};

use crate::player_projection::PlayerProjectionSet;
pub(crate) use authority::{
    CombatDuration, CombatInstant, MeleeAttackAuthority, RangedAttackAuthority, ReportedPrecision,
};
use authority::{ValidatedRangedImpact, validate_melee_intent_cheap, validate_ranged_intent};
pub(crate) use condition::update_tactical_combat_state;
pub(crate) use consequence::apply_transient_attack_result;
use consequence::{apply_melee_attack_result, record_party_ammunition_use};
#[cfg(test)]
use consequence::{
    attacker_weapon_contact_matches, defender_equipment_contact_matches, record_party_injury,
};
use ingress::{
    authoritative_line_of_sight, on_defender_response, on_melee_action_request,
    on_melee_attack_started, on_ranged_action_request, on_ranged_attack_started,
    resolve_defender_response,
};
use melee::resolve_melee_attack;
pub(crate) use protocol::{
    MeleeAttackIntent, MeleeAttackStartedIntent, PendingDefenderResponse, RangedAttackIntent,
    RangedAttackStartedIntent, TacticalCombatSide, TacticalCombatantDefeated,
};
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

/// Maximum window (in seconds) after pressing dodge/parry that the response
/// is still considered valid. A fresh press gives `input_reflex = 1.0`;
/// a press older than this window is treated as no response.
const MAX_REFLEX_WINDOW: Duration = Duration::from_millis(500);
const CLIENT_MELEE_WINDUP: CombatDuration =
    CombatDuration::from_duration(Duration::from_millis(300));
const MELEE_COOLDOWN: CombatDuration = CombatDuration::from_duration(Duration::from_millis(300));
/// Completion must arrive within this bounded ordered-network allowance after
/// the windup becomes ready; old starts cannot authorize replayed completions.
const MELEE_WINDUP_NETWORK_ALLOWANCE: CombatDuration =
    CombatDuration::from_duration(Duration::from_secs(1));
/// Allows bounded movement between the authoritative physics snapshot and an
/// ordered attack request arriving at the server.
const MELEE_RANGE_LATENCY_TOLERANCE: f32 = 0.25;
const CLIENT_RANGED_WINDUP: CombatDuration =
    CombatDuration::from_duration(Duration::from_millis(300));
const RANGED_COOLDOWN: CombatDuration = CombatDuration::from_duration(Duration::from_millis(600));
const RANGED_NETWORK_ALLOWANCE: CombatDuration =
    CombatDuration::from_duration(Duration::from_secs(1));
const RANGED_RANGE_LATENCY_TOLERANCE: f32 = 0.5;
/// The server owns yaw but not full skeletal/secondary animation. Permit a
/// small network/input cone while still rejecting targets behind the shooter.
const RANGED_AIM_CONE_DEGREES: f32 = 20.0;

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
    separation: Option<f32>,
    target_in_aim_cone: Option<bool>,
    authority_permits: bool,
    body_part: BodyPart,
    attacker_position: Vec3,
    target_position: Option<Vec3>,
    attacker_yaw: f32,
    target_yaw: Option<f32>,
}

fn ranged_target_in_aim_cone(yaw: f32, attacker: Vec3, target: Vec3) -> bool {
    let offset = target.xz() - attacker.xz();
    let Some(direction) = offset.try_normalize() else {
        return false;
    };
    let forward = Vec2::new(-yaw.sin(), -yaw.cos());
    direction.dot(forward) >= RANGED_AIM_CONE_DEGREES.to_radians().cos()
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
    weapon_reach: f32,
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
            .add_observer(on_melee_action_request)
            .add_observer(on_ranged_action_request)
            .add_observer(on_ranged_attack_started)
            .add_observer(on_melee_attack_started)
            .add_observer(resolve_melee_attack)
            .add_observer(resolve_ranged_attack)
            .add_observer(apply_melee_attack_result)
            .add_observer(on_defender_response)
            .configure_sets(Update, CombatSet::Condition)
            .add_systems(
                Update,
                update_tactical_combat_state
                    .in_set(CombatSet::Condition)
                    .after(PlayerProjectionSet::Spawn),
            );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId;

    #[derive(Resource)]
    struct BatchedCompletions {
        attacker: Entity,
        target: Entity,
    }

    #[derive(Resource, Default)]
    struct AcceptedCompletions(u32);

    fn emit_batched_completions(mut cmd: Commands, batch: Res<BatchedCompletions>) {
        for _ in 0..3 {
            cmd.trigger(FromClient {
                client_id: ClientId::Client(batch.attacker),
                message: MeleeActionRequest::Complete {
                    target: batch.target,
                    body_part: BodyPart::Chest,
                    reported_precision: 1.0,
                },
            });
        }
    }

    fn apply_if_authorized(
        event: On<MeleeAttackIntent>,
        time: Res<Time<()>>,
        mut authorities: Query<&mut MeleeAttackAuthority>,
        mut limbs: Query<&mut Limbs>,
        mut accepted: ResMut<AcceptedCompletions>,
    ) {
        let Ok(mut authority) = authorities.get_mut(event.attacker) else {
            return;
        };
        let Some(validated) = validate_melee_intent_cheap(MeleeIntentFacts {
            attacker: event.attacker,
            target: event.target,
            attacker_side: Some(TacticalCombatSide::Party),
            target_side: Some(TacticalCombatSide::Enemy),
            attacker_incapacitated: Some(false),
            target_incapacitated: Some(false),
            reported_precision: event.reported_precision,
            weapon_reach: 1.0,
            separation: 1.0,
            authority_permits: authority.permits(event.target, CombatInstant::from_elapsed(&time)),
            body_part: event.body_part,
            attacker_position: Vec3::ZERO,
            target_position: Vec3::X,
            attacker_yaw: 0.0,
            target_yaw: 0.0,
        })
        .ok() else {
            return;
        };
        if authority
            .authorize_attack(
                validated,
                CombatInstant::from_elapsed(&time),
                MELEE_COOLDOWN,
            )
            .is_some()
        {
            accepted.0 += 1;
            if let Ok(mut limbs) = limbs.get_mut(event.target) {
                limbs.chest -= 0.1;
            }
        }
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
            weapon_reach: 0.8,
            separation: 2.0,
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
        assert!(ranged_target_in_aim_cone(0.0, origin, Vec3::NEG_Z));
        assert!(!ranged_target_in_aim_cone(0.0, origin, Vec3::Z));

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
        assert!((consequence.blood_loss_fraction - 0.25).abs() < 0.0001);
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
        assert!((recover_combat_imbalance(0.5, 2.0, 2.0) - 0.38).abs() < 0.0001);
        assert_eq!(recover_combat_imbalance(0.01, 5.0, 1.0), 0.0);
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
                    ..default()
                },
                TacticalCombatState {
                    exhaustion: 0.25,
                    ..default()
                },
                LinearVelocity(Vec3::new(jog_speed, 0.0, 0.0)),
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

        app.world_mut()
            .entity_mut(actor)
            .insert(LinearVelocity(Vec3::new(8.0, 0.0, 0.0)));
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
            .insert(LinearVelocity::ZERO);
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
    fn batched_completions_consume_one_windup_once() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<AcceptedCompletions>()
            .add_observer(on_melee_action_request)
            .add_observer(apply_if_authorized);
        let attacker = app.world_mut().spawn(MeleeAttackAuthority::default()).id();
        let target = app.world_mut().spawn(Limbs::default()).id();
        app.world_mut().trigger(FromClient {
            client_id: ClientId::Client(attacker),
            message: MeleeActionRequest::Start {
                strike_family: StrikeFamily::Thrust,
                footwork: Footwork::Stay,
            },
        });
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs_f32(CLIENT_MELEE_WINDUP.as_secs_f32()));
        app.insert_resource(BatchedCompletions { attacker, target })
            .add_systems(Update, emit_batched_completions);
        app.update();

        assert_eq!(app.world().resource::<AcceptedCompletions>().0, 1);
        assert!((app.world().entity(target).get::<Limbs>().unwrap().chest - 0.9).abs() < 0.0001);
    }
}
