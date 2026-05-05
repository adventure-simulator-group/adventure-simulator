use avian3d::collision::collider::LayerMask;
use avian3d::prelude::*;
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

        #[cfg(feature = "avian_debug")]
        app.add_plugins(PhysicsDebugPlugin)
            .insert_gizmo_config(
                PhysicsGizmos::default(),
                GizmoConfig {
                    depth_bias: -1.0,
                    ..default()
                },
            )
            .init_resource::<PhysicsLengthUnit>()
            .insert_resource(PhysicsDebugRenderConfig {
                enable_axes: true,
                enable_colliders: true,
                enable_aabb: false,
                enable_bvh: false,
                enable_contacts: false,
                enable_joints: false,
                enable_raycasts: false,
                enable_shapecasts: false,
                enable_islands: false,
            });
    }
}
