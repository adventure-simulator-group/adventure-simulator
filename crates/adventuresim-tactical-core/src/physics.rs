use avian3d::collision::collider::LayerMask;
use avian3d::PhysicsPlugins;
use bevy::prelude::*;
use bevy_ahoy::{camera::AhoyCameraPlugin, AhoyPlugins};

pub const HITBOX_LAYER: LayerMask = LayerMask(1 << 1);
pub const HITREG_LAYER: LayerMask = LayerMask(1 << 2);

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
            app.add_plugins((PhysicsPlugins::default(), AhoyPlugins::default()));
        } else {
            app.add_plugins((AhoyCameraPlugin,));
        }
    }
}
