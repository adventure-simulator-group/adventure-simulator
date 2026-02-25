use adventure_simulator_core::prelude::*;
use bevy::prelude::*;
use lightyear::{
    input::config::InputConfig,
    prelude::{
        input::{bei::InputPlugin, InputRegistryExt},
        AppComponentExt, InterpolationRegistrationExt, TransformLinearInterpolation,
    },
};

#[derive(Default)]
pub struct AdventureSimulatorNetcodePlugin;

impl Plugin for AdventureSimulatorNetcodePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputPlugin::<Player>::new(InputConfig::<Player> {
            rebroadcast_inputs: false,
            ignore_rollbacks: true,
            ..default()
        }));
        app.register_input_action::<input::Movement>();
        app.register_input_action::<input::Jump>();

        #[cfg(feature = "server")]
        app.add_plugins(lightyear::avian3d::plugin::LightyearAvianPlugin {
            replication_mode: lightyear::avian3d::plugin::AvianReplicationMode::Transform,
            ..default()
        });

        app.register_component::<Player>();
        app.register_component::<PlayerId>();
        app.register_component::<Transform>()
            .add_interpolation_with(TransformLinearInterpolation::lerp);
        app.register_component::<CharacterLook>();

        app.register_component::<SceneId>();
        app.register_component::<SceneTerrain>();
    }
}
