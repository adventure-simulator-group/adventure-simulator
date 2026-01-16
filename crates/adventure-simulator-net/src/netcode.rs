use adventure_simulator_core::Player;
use bevy::prelude::*;
use lightyear::{input::config::InputConfig, prelude::input::bei::InputPlugin};

#[derive(Default)]
pub struct AdventureSimulatorNetcodePlugin;

impl Plugin for AdventureSimulatorNetcodePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputPlugin::<Player> {
            config: InputConfig::<Player> {
                rebroadcast_inputs: true,
                ..default()
            },
        });
    }
}
