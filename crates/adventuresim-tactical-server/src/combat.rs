use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendMode, ServerTriggerExt, ToClients},
    message::{DefendRequest, MeleeActionPhase, MeleeActionRequest, SuccessfulAttackResponse},
};
use bevy::prelude::*;

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

#[derive(Event, Clone, Copy, Debug)]
struct ApplyMeleeAttackResult {
    attacker: Entity,
    target: Entity,
    body_part: BodyPart,
    result: AttackResult,
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
        app.add_observer(on_melee_action_request)
            .add_observer(on_melee_attack_started)
            .add_observer(resolve_melee_attack)
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

    cmd.trigger(ApplyMeleeAttackResult {
        attacker: entity,
        target: event.target,
        body_part: event.body_part,
        result,
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

fn apply_melee_attack_result(
    event: On<ApplyMeleeAttackResult>,
    mut combatants: Query<(&mut Limbs, &mut TacticalCombatState)>,
) {
    let Ok([attacker, defender]) = combatants.get_many_mut([event.attacker, event.target]) else {
        return;
    };
    let (_, mut attacker_state) = attacker;
    let (mut defender_limbs, mut defender_state) = defender;
    apply_transient_attack_result(
        &mut attacker_state,
        &mut defender_limbs,
        &mut defender_state,
        event.result,
        event.body_part,
    );
}

pub(crate) fn apply_transient_attack_result(
    attacker_state: &mut TacticalCombatState,
    defender_limbs: &mut Limbs,
    defender_state: &mut TacticalCombatState,
    result: AttackResult,
    body_part: BodyPart,
) {
    match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            attacker_state.imbalance += balance_damage.max(0.0);
        }
        AttackResult::ToDefender { balance_damage, .. } => {
            defender_state.imbalance += balance_damage.max(0.0);
            let damage = health_damage_from_attack(result, body_part);
            let applied = apply_clamped_limb_damage(defender_limbs.health_mut(body_part), damage);
            defender_state.blood_loss_fraction = (defender_state.blood_loss_fraction
                + applied * BLOOD_LOSS_PER_HEALTH_DAMAGE)
                .clamp(0.0, 1.0);
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
            cmd.entity(entity)
                .remove::<PendingDefenderResponse>()
                .insert(Incapacitated);
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
    fn damage_clamps_limb_and_accumulates_blood_and_imbalance() {
        let mut attacker = TacticalCombatState::default();
        let mut defender = TacticalCombatState::default();
        let mut limbs = Limbs {
            chest: 0.2,
            ..default()
        };
        apply_transient_attack_result(
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
