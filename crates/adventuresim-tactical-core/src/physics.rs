use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_ahoy::{
    AhoyPlugins, AhoySystems, CharacterController, camera::AhoyCameraPlugin,
    input::AccumulatedInput,
};

/// Maximum ordinary tactical movement speed, in metres per second.
pub const TACTICAL_RUN_SPEED_METRES_PER_SECOND: f32 = 5.5;

/// Builds the shared tactical controller instead of inheriting Ahoy's much
/// faster general-purpose default.
pub fn tactical_character_controller() -> CharacterController {
    CharacterController {
        speed: TACTICAL_RUN_SPEED_METRES_PER_SECOND,
        ..default()
    }
}

/// Preserves the analogue movement magnitude after Ahoy normalizes its wish
/// direction. Keyboard input has magnitude one; a half-deflected stick sets a
/// half-speed controller target.
pub fn tactical_movement_speed(movement: Option<Vec2>) -> f32 {
    TACTICAL_RUN_SPEED_METRES_PER_SECOND
        * movement.map_or(0.0, |movement| movement.length().clamp(0.0, 1.0))
}

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
            ))
            .add_systems(
                FixedPostUpdate,
                apply_analogue_movement_speed.before(AhoySystems::MoveCharacters),
            );
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

fn apply_analogue_movement_speed(
    mut controllers: Query<(&AccumulatedInput, &mut CharacterController)>,
) {
    for (input, mut controller) in &mut controllers {
        controller.speed = tactical_movement_speed(input.last_movement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_speed_preserves_stick_magnitude_and_caps_diagonals() {
        assert_eq!(tactical_movement_speed(None), 0.0);
        assert_eq!(
            tactical_movement_speed(Some(Vec2::new(0.3, 0.4))),
            TACTICAL_RUN_SPEED_METRES_PER_SECOND * 0.5
        );
        assert_eq!(
            tactical_movement_speed(Some(Vec2::ONE)),
            TACTICAL_RUN_SPEED_METRES_PER_SECOND
        );
    }
}
