use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_attack_action_triggered);
    }
}

fn on_attack_action_triggered(event: On<Start<Attack>>, viewer: TacticalPlayerViewer) {
    let entity = event.context;
    if let Some(player_info) = viewer.get(entity) {
        let skill_check = player_info.skill_check(Skill::Melee);
        info!("Melee skill check for {entity:?}: {skill_check}");
    }
}
