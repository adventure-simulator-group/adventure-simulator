use avian3d::{
    dynamics::solver::islands::IslandSleepingPlugin,
    prelude::{PhysicsInterpolationPlugin, PhysicsTransformPlugin},
    PhysicsPlugins,
};
use bevy::prelude::*;
use bevy_ahoy::{camera::AhoyCameraPlugin, AhoyPlugins};

pub struct AdventureSimulatorPhysicsPlugin {
    pub enable_simulation: bool,
}

impl Default for AdventureSimulatorPhysicsPlugin {
    fn default() -> Self {
        Self {
            enable_simulation: true,
        }
    }
}

impl Plugin for AdventureSimulatorPhysicsPlugin {
    fn build(&self, app: &mut App) {
        if self.enable_simulation {
            app.add_plugins((
                PhysicsPlugins::new(FixedPostUpdate)
                    .build()
                    .disable::<PhysicsTransformPlugin>()
                    .disable::<PhysicsInterpolationPlugin>()
                    .disable::<IslandSleepingPlugin>(),
                AhoyPlugins::default(),
            ));
        } else {
            app.add_plugins((AhoyCameraPlugin,));
        }
    }
}
