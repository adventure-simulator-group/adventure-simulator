use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendMode, ServerTriggerExt, ToClients},
    message::SuccessfulAttackResponse,
    prelude::AttackRequest,
};
use bevy::prelude::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_attack_action_triggered);
    }
}

fn on_attack_action_triggered(
    event: On<FromClient<AttackRequest>>,
    mut cmd: Commands,
    viewer: TacticalPlayerViewer,
    q_character: Query<&CharacterLook>,
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

    let result = attacker_view.resolve_melee_attack(
        attacker_side,
        &defender_view,
        event.hit_precision,
        flanking,
        event.body_part,
    );

    let cut = result.cut_damage;
    let blunt = result.blunt_damage;

    // TODO: apply damage

    info!(
        "{entity:?} hit {:?} on {:?} for {:.1} damage ({cut:.1} cut + {blunt:.1} blunt)",
        event.target,
        event.body_part,
        cut + blunt
    );

    cmd.server_trigger(ToClients {
        mode: SendMode::CLIENTS_ONLY,
        message: SuccessfulAttackResponse {
            attacker: entity,
            hit: vec![event.target],
            body_part: event.body_part,
            cut_damage: cut,
            blunt_damage: blunt,
            flanking,
        },
    });
}
