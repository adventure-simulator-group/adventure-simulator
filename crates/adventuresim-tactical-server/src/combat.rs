use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, ServerState},
    prelude::AttackCommand,
};
use bevy::prelude::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, on_attack_action_triggered.run_if(in_state(ServerState::Running)));
    }
}

fn on_attack_action_triggered(
    mut attacks: MessageReader<FromClient<AttackCommand>>,
    viewer: TacticalPlayerViewer,
) {
    for event in attacks.read() {
        let Some(entity) = event.client_id.entity() else {
            continue;
        };
        if let Some(player_info) = viewer.get(entity) {
            let skill_check = player_info.skill_check(Skill::Melee);
            info!("Melee skill check for {entity:?}: {skill_check}");
        }
    }
}
