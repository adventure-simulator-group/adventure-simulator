use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendMode, ServerTriggerExt, ToClients},
    message::{
        DefendRequest, MeleeActionPhase, MeleeActionRequest, RangedActionPhase,
        RangedActionRequest, SuccessfulAttackResponse,
    },
};
use bevy::prelude::*;
use std::{collections::HashMap, num::NonZeroU32};

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
    pub character_id: u64,
    pub inventory_item_id: u64,
    pub contact_stress: f32,
    pub defender_equipment: bool,
}

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct TacticalConsequenceAccumulator {
    pub party: HashMap<u64, AccumulatedPartyConsequence>,
    pub equipment_contacts: Vec<AccumulatedEquipmentContact>,
}

/// Maximum window (in seconds) after pressing dodge/parry that the response
/// is still considered valid. A fresh press gives `input_reflex = 1.0`;
/// a press older than this window is treated as no response.
const MAX_REFLEX_WINDOW: f32 = 0.5;
const CLIENT_MELEE_WINDUP_SECS: f32 = 0.3;
const MELEE_COOLDOWN_SECS: f32 = 0.3;
/// Completion must arrive within this bounded ordered-network allowance after
/// the windup becomes ready; old starts cannot authorize replayed completions.
const MELEE_WINDUP_NETWORK_ALLOWANCE_SECS: f32 = 1.0;
/// Allows bounded movement between the authoritative physics snapshot and an
/// ordered attack request arriving at the server.
const MELEE_RANGE_LATENCY_TOLERANCE: f32 = 0.25;
const CLIENT_RANGED_WINDUP_SECS: f32 = 0.3;
const RANGED_COOLDOWN_SECS: f32 = 0.6;
const RANGED_NETWORK_ALLOWANCE_SECS: f32 = 1.0;
const RANGED_RANGE_LATENCY_TOLERANCE: f32 = 0.5;
/// The server owns yaw but not full skeletal/secondary animation. Permit a
/// small network/input cone while still rejecting targets behind the shooter.
const RANGED_AIM_CONE_DEGREES: f32 = 20.0;
const ARROW_ITEM_ID: &str = "arrow";

/// Stores the defender's most recent dodge/parry choice along with the
/// server timestamp when it was received. Consumed on each attack resolution.
///
/// Set either by [`on_defender_response`] for a real player's key press, or by
/// [`crate::bot`]'s AI standing in for a bot's reaction.
#[derive(Component)]
pub struct PendingDefenderResponse {
    pub choice: DefendRequest,
    pub set_at: f32,
}

/// Transient allegiance used by the tactical server to identify opponents.
///
/// This is deliberately independent from player connectivity and bot control:
/// tests and future mission types can put server-controlled combatants on
/// either side.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TacticalCombatSide {
    Party,
    Enemy,
}

#[derive(Debug)]
struct ObservedMeleeWindup {
    target: Option<Entity>,
    ready_at: f32,
    expires_at: f32,
}

#[derive(Component, Debug, Default)]
pub(crate) struct MeleeAttackAuthority {
    windup: Option<ObservedMeleeWindup>,
    cooldown_until: f32,
}

#[derive(Debug)]
struct ObservedRangedWindup {
    ready_at: f32,
    expires_at: f32,
}

#[derive(Component, Debug, Default)]
pub(crate) struct RangedAttackAuthority {
    windup: Option<ObservedRangedWindup>,
    cooldown_until: f32,
}

fn consume_ranged_authority(authority: &mut RangedAttackAuthority, now: f32) -> bool {
    let valid = authority.windup.as_ref().is_some_and(|windup| {
        now >= windup.ready_at && now <= windup.expires_at && now >= authority.cooldown_until
    });
    if valid {
        authority.windup = None;
        authority.cooldown_until = now + RANGED_COOLDOWN_SECS;
    }
    valid
}

fn remaining_ammo_after_shot(quantity: NonZeroU32) -> Option<NonZeroU32> {
    NonZeroU32::new(quantity.get() - 1)
}

fn consume_melee_authority(authority: &mut MeleeAttackAuthority, target: Entity, now: f32) -> bool {
    let valid = authority.windup.as_ref().is_some_and(|windup| {
        windup.target.map_or(true, |observed| observed == target)
            && now >= windup.ready_at
            && now <= windup.expires_at
            && now >= authority.cooldown_until
    });
    if valid {
        authority.windup = None;
        authority.cooldown_until = now + MELEE_COOLDOWN_SECS;
    }
    valid
}

/// Present while transient combat effects meet the incapacitation threshold.
#[derive(Component, Debug)]
pub struct Incapacitated;

/// Emitted once per transition from active to incapacitated. Mission systems
/// decide whether that transition constitutes a counted tactical defeat.
#[derive(Event, Clone, Copy, Debug)]
pub struct TacticalCombatantDefeated(pub Entity);

/// A server-internal melee request. Both network clients and server-owned AI
/// enter combat resolution through this seam.
#[derive(Event, Clone, Copy, Debug)]
pub struct MeleeAttackIntent {
    pub attacker: Entity,
    pub target: Entity,
    pub body_part: BodyPart,
    pub hit_precision: f32,
}

/// Announces a targeted windup so the intended defender alone can react.
#[derive(Event, Clone, Copy, Debug)]
pub struct MeleeAttackStartedIntent {
    pub attacker: Entity,
    pub target: Entity,
    pub windup_secs: f32,
}

/// A server-internal fired shot. `target == None` is an authoritative miss
/// that still consumes one projectile after the firing gate succeeds.
#[derive(Event, Clone, Copy, Debug)]
pub struct RangedAttackIntent {
    pub attacker: Entity,
    pub target: Option<Entity>,
    pub body_part: BodyPart,
    pub hit_precision: f32,
}

/// Announces a server-observed ranged windup. Clients and server-owned AI use
/// this same seam before completing a shot.
#[derive(Event, Clone, Copy, Debug)]
pub struct RangedAttackStartedIntent {
    pub attacker: Entity,
    pub target: Option<Entity>,
    pub windup_secs: f32,
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
    NonFinitePrecision,
    NotRanged,
    OutOfRange,
    OutsideAimCone,
    Windup,
    Cooldown,
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
    hit_precision: f32,
    weapon_is_ranged: bool,
    weapon_range: f32,
    separation: Option<f32>,
    target_in_aim_cone: Option<bool>,
    windup_ready: bool,
    windup_unexpired: bool,
    cooldown_ready: bool,
}

fn validate_ranged_intent(facts: RangedIntentFacts) -> Result<(), RangedIntentRejection> {
    if facts.target == Some(facts.attacker) {
        return Err(RangedIntentRejection::SelfTarget);
    }
    let Some(attacker_side) = facts.attacker_side else {
        return Err(RangedIntentRejection::MissingSide);
    };
    let Some(attacker_incapacitated) = facts.attacker_incapacitated else {
        return Err(RangedIntentRejection::MissingCombatState);
    };
    if attacker_incapacitated {
        return Err(RangedIntentRejection::Incapacitated);
    }
    if !facts.hit_precision.is_finite() {
        return Err(RangedIntentRejection::NonFinitePrecision);
    }
    if !facts.weapon_is_ranged || !facts.weapon_range.is_finite() || facts.weapon_range <= 0.0 {
        return Err(RangedIntentRejection::NotRanged);
    }
    if let Some(_) = facts.target {
        let Some(target_side) = facts.target_side else {
            return Err(RangedIntentRejection::MissingSide);
        };
        let Some(target_incapacitated) = facts.target_incapacitated else {
            return Err(RangedIntentRejection::MissingCombatState);
        };
        if attacker_side == target_side {
            return Err(RangedIntentRejection::FriendlyTarget);
        }
        if target_incapacitated {
            return Err(RangedIntentRejection::Incapacitated);
        }
        if !facts.separation.is_some_and(|distance| {
            distance.is_finite() && distance <= facts.weapon_range + RANGED_RANGE_LATENCY_TOLERANCE
        }) {
            return Err(RangedIntentRejection::OutOfRange);
        }
        if facts.target_in_aim_cone != Some(true) {
            return Err(RangedIntentRejection::OutsideAimCone);
        }
    }
    if !facts.windup_ready || !facts.windup_unexpired {
        return Err(RangedIntentRejection::Windup);
    }
    if !facts.cooldown_ready {
        return Err(RangedIntentRejection::Cooldown);
    }
    Ok(())
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
    NonFinitePrecision,
    Unarmed,
    OutOfRange,
    Windup,
    Cooldown,
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
    hit_precision: f32,
    weapon_reach: f32,
    separation: f32,
    windup_target: Option<Option<Entity>>,
    windup_ready: bool,
    windup_unexpired: bool,
    cooldown_ready: bool,
}

fn validate_melee_intent_cheap(facts: MeleeIntentFacts) -> Result<(), MeleeIntentRejection> {
    if facts.attacker == facts.target {
        return Err(MeleeIntentRejection::SelfTarget);
    }
    let (Some(attacker_side), Some(target_side)) = (facts.attacker_side, facts.target_side) else {
        return Err(MeleeIntentRejection::MissingSide);
    };
    if attacker_side == target_side {
        return Err(MeleeIntentRejection::FriendlyTarget);
    }
    let (Some(attacker_incapacitated), Some(target_incapacitated)) =
        (facts.attacker_incapacitated, facts.target_incapacitated)
    else {
        return Err(MeleeIntentRejection::MissingCombatState);
    };
    if attacker_incapacitated || target_incapacitated {
        return Err(MeleeIntentRejection::Incapacitated);
    }
    if !facts.hit_precision.is_finite() {
        return Err(MeleeIntentRejection::NonFinitePrecision);
    }
    if !facts.weapon_reach.is_finite() || facts.weapon_reach <= 0.0 {
        return Err(MeleeIntentRejection::Unarmed);
    }
    if !facts.separation.is_finite()
        || facts.separation
            > melee_interaction_range(facts.weapon_reach) + MELEE_RANGE_LATENCY_TOLERANCE
    {
        return Err(MeleeIntentRejection::OutOfRange);
    }
    let Some(windup_target) = facts.windup_target else {
        return Err(MeleeIntentRejection::Windup);
    };
    if windup_target.is_some_and(|target| target != facts.target)
        || !facts.windup_ready
        || !facts.windup_unexpired
    {
        return Err(MeleeIntentRejection::Windup);
    }
    if !facts.cooldown_ready {
        return Err(MeleeIntentRejection::Cooldown);
    }
    Ok(())
}

fn validate_melee_line_of_sight(line_of_sight: bool) -> Result<(), MeleeIntentRejection> {
    line_of_sight
        .then_some(())
        .ok_or(MeleeIntentRejection::BlockedLineOfSight)
}

pub struct CombatPlugin;

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
            .add_systems(Update, update_tactical_combat_state);
    }
}

fn on_defender_response(
    event: On<FromClient<DefendRequest>>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    states: Query<&TacticalCombatState>,
) {
    let Some(entity) = event.client_id.entity() else {
        warn!(
            "Got defender response from an unknown client: {:?}",
            event.client_id
        );
        return;
    };

    if states.get(entity).is_ok_and(|state| state.incapacitated) {
        return;
    }

    cmd.entity(entity).insert(PendingDefenderResponse {
        choice: **event,
        set_at: time.elapsed_secs(),
    });
}

fn on_melee_attack_started(
    event: On<MeleeAttackStartedIntent>,
    mut authorities: Query<&mut MeleeAttackAuthority>,
    time: Res<Time<()>>,
) {
    let Ok(mut authority) = authorities.get_mut(event.attacker) else {
        return;
    };
    authority.windup = Some(ObservedMeleeWindup {
        target: Some(event.target),
        ready_at: time.elapsed_secs() + event.windup_secs.max(0.0),
        expires_at: time.elapsed_secs()
            + event.windup_secs.max(0.0)
            + MELEE_WINDUP_NETWORK_ALLOWANCE_SECS,
    });
}

fn resolve_defender_response(
    pending: Option<&PendingDefenderResponse>,
    time: &Time<()>,
    defender_view: &TacticalPlayerView,
) -> DefenderResponse {
    let Some(pending) = pending else {
        return DefenderResponse::None;
    };

    let elapsed = time.elapsed_secs() - pending.set_at;
    if elapsed > MAX_REFLEX_WINDOW {
        return DefenderResponse::None;
    }

    let input_reflex = (1.0 - elapsed / MAX_REFLEX_WINDOW).clamp(0.0, 1.0);

    match pending.choice {
        DefendRequest::Dodge => DefenderResponse::Dodge { input_reflex },
        DefendRequest::Parry => {
            if defender_view.shield_block_bonus() > 0.0 {
                DefenderResponse::Parry { input_reflex }
            } else {
                DefenderResponse::None
            }
        }
    }
}

fn on_melee_action_request(
    event: On<FromClient<MeleeActionRequest>>,
    mut cmd: Commands,
    time: Res<Time<()>>,
    mut authorities: Query<&mut MeleeAttackAuthority>,
) {
    let Some(attacker) = event.client_id.entity() else {
        debug!(
            "Ignoring melee action from unknown client: {:?}",
            event.client_id
        );
        return;
    };
    match event.phase {
        MeleeActionPhase::Start => {
            let Ok(mut authority) = authorities.get_mut(attacker) else {
                return;
            };
            let ready_at = time.elapsed_secs() + CLIENT_MELEE_WINDUP_SECS;
            authority.windup = Some(ObservedMeleeWindup {
                target: None,
                ready_at,
                expires_at: ready_at + MELEE_WINDUP_NETWORK_ALLOWANCE_SECS,
            });
        }
        MeleeActionPhase::Complete => {
            let Some(target) = event.target else {
                debug!("Ignoring malformed melee completion from {attacker:?}");
                return;
            };
            // Finite precision is intentionally accepted as reported. Full
            // animation and secondary physics remain client-owned.
            cmd.trigger(MeleeAttackIntent {
                attacker,
                target,
                body_part: event.body_part,
                hit_precision: event.hit_precision,
            });
        }
    }
}

fn on_ranged_action_request(event: On<FromClient<RangedActionRequest>>, mut cmd: Commands) {
    let Some(attacker) = event.client_id.entity() else {
        debug!(
            "Ignoring ranged action from unknown client: {:?}",
            event.client_id
        );
        return;
    };
    match event.phase {
        RangedActionPhase::Start => {
            cmd.trigger(RangedAttackStartedIntent {
                attacker,
                target: None,
                windup_secs: CLIENT_RANGED_WINDUP_SECS,
            });
        }
        RangedActionPhase::Complete => {
            // Finite precision is deliberately trusted. Animation and
            // secondary physics remain client-owned and non-deterministic.
            cmd.trigger(RangedAttackIntent {
                attacker,
                target: event.target,
                body_part: event.body_part,
                hit_precision: event.hit_precision,
            });
        }
    }
}

fn on_ranged_attack_started(
    event: On<RangedAttackStartedIntent>,
    mut authorities: Query<&mut RangedAttackAuthority>,
    time: Res<Time<()>>,
) {
    let Ok(mut authority) = authorities.get_mut(event.attacker) else {
        return;
    };
    let ready_at = time.elapsed_secs() + event.windup_secs.max(0.0);
    authority.windup = Some(ObservedRangedWindup {
        ready_at,
        expires_at: ready_at + RANGED_NETWORK_ALLOWANCE_SECS,
    });
}

fn authoritative_line_of_sight(
    spatial: &SpatialQuery,
    attacker: Entity,
    target: Entity,
    origin: Vec3,
    target_position: Vec3,
) -> bool {
    let offset = target_position - origin;
    let distance = offset.length();
    let Ok(direction) = Dir3::new(offset) else {
        return false;
    };
    let filter = SpatialQueryFilter::from_excluded_entities([attacker]);
    spatial
        .cast_ray(origin, direction, distance, true, &filter)
        .is_some_and(|hit| hit.entity == target)
}

fn resolve_melee_attack(
    event: On<MeleeAttackIntent>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    spatial: SpatialQuery,
    q_character: Query<(&CharacterLook, &Transform)>,
    q_sides: Query<&TacticalCombatSide>,
    q_states: Query<&TacticalCombatState>,
    mut q_authorities: Query<&mut MeleeAttackAuthority>,
    q_bestiary_categories: Query<&BestiaryCategories>,
    q_pending: Query<&PendingDefenderResponse>,
    time: Res<Time<()>>,
) {
    let entity = event.attacker;

    let Ok(attacker_view) = viewer.get(entity).inspect_err(|err| {
        debug!("Rejected attacker view for {entity:?}: {err}");
    }) else {
        return;
    };
    let Ok(defender_view) = viewer.get(event.target).inspect_err(|err| {
        debug!("Rejected defender view for {:?}: {err}", event.target);
    }) else {
        return;
    };

    let Ok([attacker_character, defender_character]) = q_character
        .get_many([entity, event.target])
        .inspect_err(|err| {
            debug!("Rejected attacker/defender transform: {err}");
        })
    else {
        return;
    };
    let (attacker_look, attacker_transform) = attacker_character;
    let (defender_look, defender_transform) = defender_character;

    let weapon_reach = attacker_view.weapon_reach();
    let now = time.elapsed_secs();
    let Ok(mut authority) = q_authorities.get_mut(entity) else {
        return;
    };
    let windup = authority.windup.as_ref();
    let facts = MeleeIntentFacts {
        attacker: entity,
        target: event.target,
        attacker_side: q_sides.get(entity).ok().copied(),
        target_side: q_sides.get(event.target).ok().copied(),
        attacker_incapacitated: q_states.get(entity).ok().map(|state| state.incapacitated),
        target_incapacitated: q_states
            .get(event.target)
            .ok()
            .map(|state| state.incapacitated),
        hit_precision: event.hit_precision,
        weapon_reach,
        separation: attacker_transform
            .translation
            .distance(defender_transform.translation),
        windup_target: windup.map(|windup| windup.target),
        windup_ready: windup.is_some_and(|windup| now >= windup.ready_at),
        windup_unexpired: windup.is_some_and(|windup| now <= windup.expires_at),
        cooldown_ready: now >= authority.cooldown_until,
    };
    if let Err(reason) = validate_melee_intent_cheap(facts) {
        debug!(
            "Rejected melee intent from {entity:?} to {:?}: {reason:?}",
            event.target
        );
        return;
    }
    let line_of_sight = authoritative_line_of_sight(
        &spatial,
        entity,
        event.target,
        attacker_transform.translation,
        defender_transform.translation,
    );
    if let Err(reason) = validate_melee_line_of_sight(line_of_sight) {
        debug!(
            "Rejected melee intent from {entity:?} to {:?}: {reason:?}",
            event.target
        );
        return;
    }
    // Mutate the pre-existing authority component synchronously. A later
    // completion in this same message flush observes the consumed windup and
    // active cooldown instead of reusing deferred Commands state.
    if !consume_melee_authority(&mut authority, event.target, now) {
        debug!("Rejected already-consumed melee authorization for {entity:?}");
        return;
    }
    let (a2, a1) = attacker_look.yaw.sin_cos();
    let (d2, d1) = defender_look.yaw.sin_cos();
    let flanking = flanking_from_dir((a1, a2), (d1, d2));

    let Some(attacker_side) = attacker_view.weapon_holding_side() else {
        debug!("Rejected attacker without a held weapon");
        return;
    };

    let pending = q_pending.get(event.target).ok();
    let defender_response = resolve_defender_response(pending, &time, &defender_view);

    // Consume the pending response so it is not reused.
    cmd.entity(event.target).remove::<PendingDefenderResponse>();

    let fallback_categories = BestiaryCategories::default();
    let defender_categories = q_bestiary_categories
        .get(event.target)
        .unwrap_or(&fallback_categories);

    let result = attacker_view.resolve_melee_attack(
        attacker_side,
        &defender_view,
        &defender_categories.0,
        defender_response,
        event.hit_precision,
        flanking,
        event.body_part,
    );
    let attacker_weapon_slot = match attacker_side {
        BodySide::Left => EquipSlot::HoldingLeft,
        BodySide::Right => EquipSlot::HoldingRight,
        BodySide::Both => return,
    };
    let defender_parry_slot = matches!(defender_response, DefenderResponse::Parry { .. })
        .then(|| defender_view.shield_holding_side())
        .flatten()
        .and_then(|side| match side {
            BodySide::Left => Some(EquipSlot::HoldingLeft),
            BodySide::Right => Some(EquipSlot::HoldingRight),
            BodySide::Both => None,
        });

    cmd.trigger(ApplyMeleeAttackResult {
        attacker: entity,
        target: event.target,
        body_part: event.body_part,
        result,
        attacker_weapon_slot,
        defender_parry_slot,
        attacker_weapon_contact: true,
    });

    match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            info!(
                "{entity:?} failed to hit {:?} on {:?} and receiver {balance_damage:.1} balance damage",
                event.target, event.body_part,
            );
        }
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } => {
            info!(
                "{entity:?} hit {:?} on {:?} for {:.1} damage ({cut_damage:.1} cut + {blunt_damage:.1} blunt) and {balance_damage:.1} balance damage",
                event.target,
                event.body_part,
                cut_damage + blunt_damage
            );
        }
    }

    cmd.server_trigger(ToClients {
        mode: SendMode::CLIENTS_ONLY,
        message: SuccessfulAttackResponse {
            attacker: entity,
            hit: vec![event.target],
            body_part: event.body_part,
            result,
            flanking,
            defender_response,
        },
    });
}

#[allow(clippy::too_many_arguments)]
fn resolve_ranged_attack(
    event: On<RangedAttackIntent>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    spatial: SpatialQuery,
    q_character: Query<(&CharacterLook, &Transform)>,
    q_sides: Query<&TacticalCombatSide>,
    q_states: Query<&TacticalCombatState>,
    mut q_authorities: Query<&mut RangedAttackAuthority>,
    q_bestiary_categories: Query<&BestiaryCategories>,
    q_pending: Query<&PendingDefenderResponse>,
    q_ammo: Query<(Entity, &ItemOf, &ItemProperties, &ItemQuantity)>,
    q_ids: Query<&PlayerId>,
    mut consequences: ResMut<TacticalConsequenceAccumulator>,
    time: Res<Time<()>>,
) {
    let attacker = event.attacker;
    let Ok(attacker_view) = viewer.get(attacker) else {
        return;
    };
    let Ok((attacker_look, attacker_transform)) = q_character.get(attacker) else {
        return;
    };
    let target_character = event.target.and_then(|target| q_character.get(target).ok());
    let now = time.elapsed_secs();
    let Ok(mut authority) = q_authorities.get_mut(attacker) else {
        return;
    };
    let windup = authority.windup.as_ref();
    let facts = RangedIntentFacts {
        attacker,
        target: event.target,
        attacker_side: q_sides.get(attacker).ok().copied(),
        target_side: event
            .target
            .and_then(|target| q_sides.get(target).ok().copied()),
        attacker_incapacitated: q_states.get(attacker).ok().map(|state| state.incapacitated),
        target_incapacitated: event
            .target
            .and_then(|target| q_states.get(target).ok().map(|state| state.incapacitated)),
        hit_precision: event.hit_precision,
        weapon_is_ranged: attacker_view.weapon_is_ranged(),
        weapon_range: attacker_view.weapon_reach(),
        separation: target_character.map(|(_, transform)| {
            attacker_transform
                .translation
                .distance(transform.translation)
        }),
        target_in_aim_cone: target_character.map(|(_, transform)| {
            ranged_target_in_aim_cone(
                attacker_look.yaw,
                attacker_transform.translation,
                transform.translation,
            )
        }),
        windup_ready: windup.is_some_and(|windup| now >= windup.ready_at),
        windup_unexpired: windup.is_some_and(|windup| now <= windup.expires_at),
        cooldown_ready: now >= authority.cooldown_until,
    };
    if let Err(reason) = validate_ranged_intent(facts) {
        if !matches!(
            reason,
            RangedIntentRejection::Windup | RangedIntentRejection::Cooldown
        ) {
            debug!("Rejected ranged intent from {attacker:?}: {reason:?}");
        }
        return;
    }
    if let (Some(target), Some((_, target_transform))) = (event.target, target_character) {
        let line_of_sight = authoritative_line_of_sight(
            &spatial,
            attacker,
            target,
            attacker_transform.translation,
            target_transform.translation,
        );
        if !line_of_sight {
            debug!(
                "Rejected ranged intent from {attacker:?}: {:?}",
                RangedIntentRejection::BlockedLineOfSight
            );
            return;
        }
    }
    if !consume_ranged_authority(&mut authority, now) {
        return;
    }

    // Only an otherwise-authorized shot reaches the global inventory scan.
    // A dry fire still consumes its windup/cooldown, bounding repeated scans.
    let ammo = q_ammo.iter().find(|(_, owner, properties, quantity)| {
        owner.0 == attacker && properties.id == ARROW_ITEM_ID && quantity.0.get() > 0
    });
    let Some((ammo_entity, _, _, quantity)) = ammo else {
        return;
    };
    if let Some(remaining) = remaining_ammo_after_shot(quantity.0) {
        cmd.entity(ammo_entity).insert(ItemQuantity(remaining));
    } else {
        cmd.entity(ammo_entity).despawn();
    }
    if q_sides
        .get(attacker)
        .is_ok_and(|side| *side == TacticalCombatSide::Party)
        && let Ok(player_id) = q_ids.get(attacker)
    {
        record_party_ammunition_use(&mut consequences, player_id.0);
    }

    let Some(target) = event.target else {
        return;
    };
    let Ok(defender_view) = viewer.get(target) else {
        return;
    };
    let Some((defender_look, _)) = target_character else {
        return;
    };
    let (a2, a1) = attacker_look.yaw.sin_cos();
    let (d2, d1) = defender_look.yaw.sin_cos();
    let flanking = flanking_from_dir((a1, a2), (d1, d2));
    let defender_response =
        resolve_defender_response(q_pending.get(target).ok(), &time, &defender_view);
    cmd.entity(target).remove::<PendingDefenderResponse>();
    let fallback_categories = BestiaryCategories::default();
    let defender_categories = q_bestiary_categories
        .get(target)
        .unwrap_or(&fallback_categories);
    let result = attacker_view.resolve_ranged_attack(
        &defender_view,
        &defender_categories.0,
        defender_response,
        event.hit_precision,
        flanking,
        event.body_part,
    );
    let defender_parry_slot = matches!(defender_response, DefenderResponse::Parry { .. })
        .then(|| defender_view.shield_holding_side())
        .flatten()
        .and_then(|side| match side {
            BodySide::Left => Some(EquipSlot::HoldingLeft),
            BodySide::Right => Some(EquipSlot::HoldingRight),
            BodySide::Both => None,
        });
    let attacker_weapon_slot = match attacker_view.weapon_holding_side() {
        Some(BodySide::Left) => EquipSlot::HoldingLeft,
        _ => EquipSlot::HoldingRight,
    };
    cmd.trigger(ApplyMeleeAttackResult {
        attacker,
        target,
        body_part: event.body_part,
        result,
        attacker_weapon_slot,
        defender_parry_slot,
        attacker_weapon_contact: false,
    });
    cmd.server_trigger(ToClients {
        mode: SendMode::CLIENTS_ONLY,
        message: SuccessfulAttackResponse {
            attacker,
            hit: vec![target],
            body_part: event.body_part,
            result,
            flanking,
            defender_response,
        },
    });
}

fn apply_melee_attack_result(
    event: On<ApplyMeleeAttackResult>,
    mut combatants: Query<(&mut Limbs, &mut TacticalCombatState)>,
    metadata: Query<(&TacticalCombatSide, &PlayerId)>,
    mut consequences: ResMut<TacticalConsequenceAccumulator>,
    items: Query<(
        &ItemOf,
        &EquipSlot,
        &crate::TacticalInventoryItemId,
        Option<&WeaponItem>,
        Option<&ShieldItem>,
        Option<&ArmorItem>,
    )>,
) {
    let Ok([attacker, defender]) = combatants.get_many_mut([event.attacker, event.target]) else {
        return;
    };
    let (_, mut attacker_state) = attacker;
    let (mut defender_limbs, mut defender_state) = defender;
    let applied = apply_transient_attack_result(
        &mut attacker_state,
        &mut defender_limbs,
        &mut defender_state,
        event.result,
        event.body_part,
    );
    let attacker_metadata = metadata.get(event.attacker).ok();
    let defender_metadata = metadata.get(event.target).ok();
    if defender_metadata.is_some_and(|(side, _)| *side == TacticalCombatSide::Party)
        && let Some((cut_damage, blunt_damage)) = applied
    {
        let defender_id = defender_metadata.unwrap().1;
        record_party_injury(
            &mut consequences,
            defender_id.0,
            event.body_part,
            cut_damage,
            blunt_damage,
        );
    }
    if attacker_metadata.is_some_and(|(side, _)| *side == TacticalCombatSide::Party) {
        let attacker_id = attacker_metadata.unwrap().1;
        let contact_stress = match event.result {
            AttackResult::ToAttacker { contact_force, .. }
            | AttackResult::ToDefender { contact_force, .. } => contact_force.max(0.0),
        };
        if event.attacker_weapon_contact
            && contact_stress > 0.0
            && let Some((_, _, provenance, _, _, _)) = items.iter().find(|row| {
                let (owner, slot, _, weapon, _, _) = row;
                attacker_weapon_contact_matches(
                    event.attacker,
                    owner.0,
                    **slot,
                    event.attacker_weapon_slot,
                    weapon.is_some(),
                )
            })
        {
            record_equipment_contact(
                &mut consequences,
                attacker_id.0,
                provenance.0,
                contact_stress,
                false,
            );
        }
    }
    if defender_metadata.is_some_and(|(side, _)| *side == TacticalCombatSide::Party) {
        let defender_id = defender_metadata.unwrap().1;
        let (contact_stress, defender_slot, require_shield, require_armor) = match event.result {
            AttackResult::ToDefender {
                contact_force,
                armor_contact,
                ..
            } if armor_contact => (
                contact_force.max(0.0),
                Some(EquipSlot::from_armor_body_part(event.body_part)),
                false,
                true,
            ),
            AttackResult::ToAttacker {
                contact_force,
                physical_contact: true,
                ..
            } if event.defender_parry_slot.is_some() => (
                contact_force.max(0.0),
                event.defender_parry_slot,
                true,
                false,
            ),
            _ => (0.0, None, false, false),
        };
        if contact_stress > 0.0
            && let Some((_, _, provenance, _, _, _)) = items.iter().find(|row| {
                let (owner, slot, _, _, shield, armor) = row;
                defender_equipment_contact_matches(
                    event.target,
                    owner.0,
                    **slot,
                    defender_slot,
                    shield.is_some(),
                    armor.is_some(),
                    require_shield,
                    require_armor,
                )
            })
        {
            record_equipment_contact(
                &mut consequences,
                defender_id.0,
                provenance.0,
                contact_stress,
                true,
            );
        }
    }
}

fn record_party_injury(
    consequences: &mut TacticalConsequenceAccumulator,
    character_id: u64,
    body_part: BodyPart,
    cut_damage: f32,
    blunt_damage: f32,
) {
    let consequence = consequences.party.entry(character_id).or_default();
    if let Some(injury) = consequence
        .injuries
        .iter_mut()
        .find(|injury| injury.body_part == body_part)
    {
        injury.cut_damage += cut_damage;
        injury.blunt_damage += blunt_damage;
        injury.max_single_hit_blunt_damage = injury.max_single_hit_blunt_damage.max(blunt_damage);
    } else {
        consequence.injuries.push(AppliedTacticalInjury {
            body_part,
            cut_damage,
            blunt_damage,
            max_single_hit_blunt_damage: blunt_damage,
        });
    }
    consequence.blood_loss_fraction = (consequence.blood_loss_fraction
        + (cut_damage + blunt_damage) * BLOOD_LOSS_PER_HEALTH_DAMAGE)
        .clamp(0.0, 1.0);
}

fn record_party_ammunition_use(
    consequences: &mut TacticalConsequenceAccumulator,
    character_id: u64,
) {
    let consequence = consequences.party.entry(character_id).or_default();
    consequence.ammunition_used = consequence
        .ammunition_used
        .saturating_add(1)
        .min(adventuresim_core::mission::MAX_TACTICAL_AMMUNITION_USED);
}

fn attacker_weapon_contact_matches(
    attacker: Entity,
    owner: Entity,
    slot: EquipSlot,
    authoritative_slot: EquipSlot,
    is_weapon: bool,
) -> bool {
    owner == attacker && slot == authoritative_slot && is_weapon
}

#[allow(clippy::too_many_arguments)]
fn defender_equipment_contact_matches(
    defender: Entity,
    owner: Entity,
    slot: EquipSlot,
    required_slot: Option<EquipSlot>,
    is_shield: bool,
    is_armor: bool,
    require_shield: bool,
    require_armor: bool,
) -> bool {
    owner == defender
        && required_slot.is_none_or(|expected| slot == expected)
        && (!require_shield || is_shield)
        && (!require_armor || is_armor)
}

fn record_equipment_contact(
    consequences: &mut TacticalConsequenceAccumulator,
    character_id: u64,
    inventory_item_id: u64,
    contact_stress: f32,
    defender_equipment: bool,
) {
    if let Some(existing) = consequences
        .equipment_contacts
        .iter_mut()
        .find(|contact| contact.inventory_item_id == inventory_item_id)
    {
        existing.contact_stress = (existing.contact_stress + contact_stress)
            .min(adventuresim_core::mission::MAX_TACTICAL_CONTACT_STRESS);
    } else if consequences.equipment_contacts.len()
        < adventuresim_core::mission::MAX_TACTICAL_EQUIPMENT_CONTACTS
    {
        consequences
            .equipment_contacts
            .push(AccumulatedEquipmentContact {
                character_id,
                inventory_item_id,
                contact_stress: contact_stress
                    .min(adventuresim_core::mission::MAX_TACTICAL_CONTACT_STRESS),
                defender_equipment,
            });
    }
}

pub(crate) fn apply_transient_attack_result(
    attacker_state: &mut TacticalCombatState,
    defender_limbs: &mut Limbs,
    defender_state: &mut TacticalCombatState,
    result: AttackResult,
    body_part: BodyPart,
) -> Option<(f32, f32)> {
    match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            attacker_state.imbalance += balance_damage.max(0.0);
            None
        }
        AttackResult::ToDefender {
            cut_damage,
            blunt_damage,
            balance_damage,
            ..
        } => {
            defender_state.imbalance += balance_damage.max(0.0);
            let damage = health_damage_from_attack(result, body_part);
            let applied = apply_clamped_limb_damage(defender_limbs.health_mut(body_part), damage);
            defender_state.blood_loss_fraction = (defender_state.blood_loss_fraction
                + applied * BLOOD_LOSS_PER_HEALTH_DAMAGE)
                .clamp(0.0, 1.0);
            let raw_total = (cut_damage + blunt_damage).max(0.0);
            if applied > 0.0 && raw_total > 0.0 {
                Some((
                    applied * cut_damage.max(0.0) / raw_total,
                    applied * blunt_damage.max(0.0) / raw_total,
                ))
            } else {
                None
            }
        }
    }
}

pub(crate) fn update_tactical_combat_state(
    mut cmd: Commands,
    time: Res<Time<()>>,
    viewer: TacticalPlayerViewer,
    limbs: Query<&Limbs>,
    mut states: Query<(
        Entity,
        &mut TacticalCombatState,
        Option<&mut input::AccumulatedInput>,
    )>,
) {
    for (entity, mut state, mut input) in &mut states {
        let was_incapacitated = state.incapacitated;
        let Ok(view) = viewer.get(entity) else {
            continue;
        };
        let balance = view.skill_check(Skill::Balance, LimbWeights::both_legs());
        state.imbalance = recover_combat_imbalance(state.imbalance, balance, time.delta_secs());
        let Ok(limbs) = limbs.get(entity) else {
            continue;
        };
        let will = view.skill_check(Skill::Will, LimbWeights::all_equal());
        state.incapacitation = combat_incapacitation(
            state.starting_incapacitation,
            state.starting_blood_fraction,
            state.blood_loss_fraction,
            limbs.total_damage(),
            will,
            state.imbalance,
        );
        state.incapacitated = state.incapacitation >= 1.0;
        if state.incapacitated {
            if let Some(input) = input.as_deref_mut() {
                input.last_movement = None;
                input.jumped = None;
            }
            if !was_incapacitated {
                cmd.entity(entity)
                    .remove::<PendingDefenderResponse>()
                    .insert(Incapacitated);
                cmd.trigger(TacticalCombatantDefeated(entity));
            }
        } else if was_incapacitated {
            cmd.entity(entity).remove::<Incapacitated>();
        }
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
                message: MeleeActionRequest::complete(batch.target, BodyPart::Chest, 1.0),
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
        if consume_melee_authority(&mut authority, event.target, time.elapsed_secs()) {
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
            hit_precision: 1.0,
            weapon_reach: 0.8,
            separation: 2.0,
            windup_target: Some(None),
            windup_ready: true,
            windup_unexpired: true,
            cooldown_ready: true,
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
            hit_precision: 1.0,
            weapon_is_ranged: true,
            weapon_range: 120.0,
            separation: Some(30.0),
            target_in_aim_cone: Some(true),
            windup_ready: true,
            windup_unexpired: true,
            cooldown_ready: true,
        }
    }

    #[test]
    fn authoritative_gate_rejects_invalid_relationship_state_and_geometry() {
        let mut world = World::new();
        let valid = valid_facts(&mut world);
        assert_eq!(validate_melee_intent_cheap(valid), Ok(()));
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
                    hit_precision: f32::NAN,
                    ..valid
                },
                MeleeIntentRejection::NonFinitePrecision,
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
                    windup_ready: false,
                    ..valid
                },
                MeleeIntentRejection::Windup,
            ),
            (
                MeleeIntentFacts {
                    windup_target: Some(Some(valid.attacker)),
                    ..valid
                },
                MeleeIntentRejection::Windup,
            ),
            (
                MeleeIntentFacts {
                    windup_unexpired: false,
                    ..valid
                },
                MeleeIntentRejection::Windup,
            ),
            (
                MeleeIntentFacts {
                    cooldown_ready: false,
                    ..valid
                },
                MeleeIntentRejection::Cooldown,
            ),
        ];
        for (facts, expected) in cases {
            assert_eq!(validate_melee_intent_cheap(facts), Err(expected));
        }
    }

    #[test]
    fn ranged_gate_validates_authority_equipment_ammo_and_target() {
        let mut world = World::new();
        let valid = valid_ranged_facts(&mut world);
        assert_eq!(validate_ranged_intent(valid), Ok(()));
        assert_eq!(
            validate_ranged_intent(RangedIntentFacts {
                hit_precision: 99.0,
                ..valid
            }),
            Ok(()),
            "finite client precision is intentionally trusted and core-clamped"
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
                    hit_precision: f32::NAN,
                    ..valid
                },
                RangedIntentRejection::NonFinitePrecision,
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
                    windup_ready: false,
                    ..valid
                },
                RangedIntentRejection::Windup,
            ),
            (
                RangedIntentFacts {
                    cooldown_ready: false,
                    ..valid
                },
                RangedIntentRejection::Cooldown,
            ),
        ];
        for (facts, expected) in cases {
            assert_eq!(validate_ranged_intent(facts), Err(expected));
        }

        assert_eq!(
            validate_ranged_intent(RangedIntentFacts {
                target: None,
                target_side: None,
                target_incapacitated: None,
                separation: None,
                ..valid
            }),
            Ok(()),
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
        assert_eq!(validate_ranged_intent(valid), Ok(()));
        assert_eq!(
            validate_ranged_intent(RangedIntentFacts {
                target_in_aim_cone: Some(false),
                ..valid
            }),
            Err(RangedIntentRejection::OutsideAimCone)
        );
    }

    #[test]
    fn ranged_rejections_happen_before_ammo_scan() {
        let source = include_str!("combat.rs");
        let resolver = source
            .split("fn resolve_ranged_attack(")
            .nth(1)
            .and_then(|tail| tail.split("fn apply_melee_attack_result(").next())
            .expect("ranged resolver body");
        let validation = resolver
            .find("validate_ranged_intent(facts)")
            .expect("cheap validation");
        let authorization = resolver
            .find("consume_ranged_authority")
            .expect("one-shot authorization");
        let ammo_scan = resolver.find("q_ammo.iter().find").expect("ammo scan");
        assert!(validation < authorization && authorization < ammo_scan);
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
            record_party_ammunition_use(&mut consequences, 7);
        }
        assert_eq!(
            consequences.party[&7].ammunition_used,
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
            record_party_injury(&mut consequences, 7, BodyPart::Chest, 0.003, 0.002);
        }
        let consequence = &consequences.party[&7];
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
                .incapacitated
        );
        assert!(app.world().entity(actor).contains::<Incapacitated>());
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
                .incapacitated
        );
        assert!(!app.world().entity(actor).contains::<Incapacitated>());
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
            message: MeleeActionRequest::start(),
        });
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs_f32(CLIENT_MELEE_WINDUP_SECS));
        app.insert_resource(BatchedCompletions { attacker, target })
            .add_systems(Update, emit_batched_completions);
        app.update();

        assert_eq!(app.world().resource::<AcceptedCompletions>().0, 1);
        assert!((app.world().entity(target).get::<Limbs>().unwrap().chest - 0.9).abs() < 0.0001);
    }
}
