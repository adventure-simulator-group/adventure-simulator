use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendMode, ServerTriggerExt, ToClients},
    message::{DefendRequest, SuccessfulAttackResponse},
    prelude::AttackRequest,
};
use bevy::prelude::*;

/// Maximum window (in seconds) after pressing dodge/parry that the response
/// is still considered valid. A fresh press gives `input_reflex = 1.0`;
/// a press older than this window is treated as no response.
const MAX_REFLEX_WINDOW: f32 = 0.5;

/// Stores the defender's most recent dodge/parry choice along with the
/// server timestamp when it was received. Consumed on each attack resolution.
#[derive(Component)]
struct PendingDefenderResponse {
    choice: DefendRequest,
    set_at: f32,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_attack_action_triggered)
            .add_observer(on_defender_response);
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

    // TODO: apply damage
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
        },
    });
}
