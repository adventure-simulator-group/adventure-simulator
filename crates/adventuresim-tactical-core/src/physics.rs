use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_ahoy::{AhoyPlugins, camera::AhoyCameraPlugin};

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
                PhysicsPlugins::new(FixedPostUpdate),
                AhoyPlugins::new(FixedPostUpdate),
            ));
        } else {
            app.add_plugins((
                PhysicsSchedulePlugin::new(FixedPostUpdate),
                BroadPhaseCorePlugin,
                ColliderHierarchyPlugin,
                ColliderTransformPlugin::new(FixedPostUpdate),
                PhysicsTransformPlugin::new(FixedPostUpdate),
                ColliderBackendPlugin::<Collider>::new(FixedPostUpdate),
                ColliderTreePlugin::<Collider>::default(),
                AhoyCameraPlugin,
            ))
            .register_required_components::<RigidBody, RigidBodyDisabled>();
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
