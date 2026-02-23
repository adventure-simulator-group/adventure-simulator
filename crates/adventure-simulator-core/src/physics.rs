use avian3d::{
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
                PhysicsPlugins::default()
                    .build()
                    .disable::<PhysicsTransformPlugin>()
                    .disable::<PhysicsInterpolationPlugin>(),
                AhoyPlugins::default(),
            ));
        } else {
            app.add_plugins((AhoyCameraPlugin,));
            // Lightyear's avian3d integration registers systems (transform_to_position)
            // that need these resources even when full physics simulation is disabled.
            app.init_resource::<avian3d::prelude::PhysicsLengthUnit>();
            app.init_resource::<avian3d::schedule::LastPhysicsTick>();
        }
    }
}
