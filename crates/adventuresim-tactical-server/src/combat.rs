use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendMode, ServerTriggerExt, ToClients},
    message::{DefendRequest, SuccessfulAttackResponse},
    prelude::AttackRequest,
};
use bevy::prelude::*;

use crate::bot::AuthoritativeEnemyDeath;

/// Maximum window (in seconds) after pressing dodge/parry that the response
/// is still considered valid. A fresh press gives `input_reflex = 1.0`;
/// a press older than this window is treated as no response.
const MAX_REFLEX_WINDOW: f32 = 0.5;

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

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_attack_action_triggered)
            .add_observer(on_defender_response)
            .add_observer(on_apply_melee_damage)
            .add_systems(Update, recover_imbalance);
    }
}

/// Rate, per point of Balance skill check, that imbalance recovers per
/// second. Mirrors `Combatant::recover_balance` in
/// `adventuresim_core::autoresolve`, which drains `0.03 * balance` once per
/// one-second main round; here it is a continuous per-second rate instead.
const IMBALANCE_RECOVERY_PER_BALANCE_PER_SECOND: f32 = 0.03;

/// Carries the already-resolved attack result and skill check needed to
/// apply it, so [`on_apply_melee_damage`] can mutate [`Limbs`]/[`CombatState`]
/// without re-borrowing the read-only [`TacticalPlayerViewer`] mutably in the
/// same system (it internally holds a `Query<&Limbs, ..>`).
#[derive(Event, Clone, Copy)]
struct ApplyMeleeDamage {
    /// The single entity this result affects: the attacker on a miss
    /// (unbalance penalty), the defender on a hit.
    entity: Entity,
    result: AttackResult,
    body_part: BodyPart,
    will_check: f32,
}

/// Applies a resolved melee attack's damage/imbalance to the affected
/// entity's [`Limbs`] and [`CombatState`], following the same math as
/// `apply_attack_result` in `adventuresim_core::autoresolve`.
fn on_apply_melee_damage(
    event: On<ApplyMeleeDamage>,
    mut q: Query<(&mut Limbs, &mut CombatState)>,
    mut commands: Commands,
) {
    let Ok((mut limbs, mut state)) = q.get_mut(event.entity) else {
        return;
    };

    match event.result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            state.imbalance += balance_damage.max(0.0);
        }
        AttackResult::ToDefender { balance_damage, .. } => {
            let damage = health_damage_from_attack(event.result, event.body_part);
            let applied = limbs.apply_damage(event.body_part, damage);
            state.imbalance += balance_damage.max(0.0);
            state.blood_loss_fraction += applied * BLOOD_LOSS_PER_HEALTH_DAMAGE;
        }
    }

    state.recompute(limbs.total_damage(), event.will_check);

    // `AuthoritativeEnemyDeath`'s handler only acts on `MissionEnemy` entities
    // (and only once, via its own `Without<CountedEnemyDeath>` guard), so it's
    // harmless to trigger this for any entity that becomes incapacitated.
    if state.status() == IncapacitationStatus::Incapacitated {
        commands.trigger(AuthoritativeEnemyDeath(event.entity));
    }
}

/// Continuously drains imbalance back toward zero, at a rate proportional to
/// each player's Balance skill check.
fn recover_imbalance(
    viewer: TacticalPlayerViewer,
    mut q_combat_state: Query<(Entity, &mut CombatState)>,
    time: Res<Time<()>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut state) in &mut q_combat_state {
        if state.imbalance <= 0.0 {
            continue;
        }
        let Ok(view) = viewer.get(entity) else {
            continue;
        };
        let balance = view.skill_check(Skill::Balance, LimbWeights::both_legs());
        state.imbalance = (state.imbalance
            - IMBALANCE_RECOVERY_PER_BALANCE_PER_SECOND * balance.max(0.25) * dt)
            .max(0.0);
    }
}

fn on_defender_response(
    event: On<FromClient<DefendRequest>>,
    mut cmd: Commands,
    time: Res<Time<()>>,
) {
    let Some(entity) = event.client_id.entity() else {
        warn!(
            "Got defender response from an unknown client: {:?}",
            event.client_id
        );
        return;
    };

    cmd.entity(entity).insert(PendingDefenderResponse {
        choice: **event,
        set_at: time.elapsed_secs(),
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

fn on_attack_action_triggered(
    event: On<FromClient<AttackRequest>>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    q_character: Query<&CharacterLook>,
    q_bestiary_categories: Query<&BestiaryCategories>,
    q_pending: Query<&PendingDefenderResponse>,
    time: Res<Time<()>>,
) {
    let Some(entity) = event.client_id.entity() else {
        warn!(
            "Got attack request from an unknown client: {:?}",
            event.client_id
        );
        return;
    };

    let Ok(attacker_view) = viewer.get(entity).inspect_err(|err| {
        error!("Can't get a view for attacker {entity:?}: {err}",);
    }) else {
        return;
    };
    let Ok(defender_view) = viewer.get(event.target).inspect_err(|err| {
        error!("Can't get a view for defender {:?}: {err}", event.target);
    }) else {
        return;
    };

    let Ok([attacker_look, defender_look]) = q_character
        .get_many([entity, event.target])
        .inspect_err(|err| {
            error!("Can't get character look for attacker/defender: {err}",);
        })
    else {
        return;
    };
    let (a2, a1) = attacker_look.yaw.sin_cos();
    let (d2, d1) = defender_look.yaw.sin_cos();
    let flanking = flanking_from_dir((a1, a2), (d1, d2));

    let Some(attacker_side) = attacker_view.weapon_holding_side() else {
        warn!("Attacker isn't holding any weapon!");
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

    let (apply_to, will_check) = match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            info!(
                "{entity:?} failed to hit {:?} on {:?} and receiver {balance_damage:.1} balance damage",
                event.target, event.body_part,
            );
            (
                entity,
                attacker_view.skill_check(Skill::Will, LimbWeights::all_equal()),
            )
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
            (
                event.target,
                defender_view.skill_check(Skill::Will, LimbWeights::all_equal()),
            )
        }
    };
    cmd.trigger(ApplyMeleeDamage {
        entity: apply_to,
        result,
        body_part: event.body_part,
        will_check,
    });

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
