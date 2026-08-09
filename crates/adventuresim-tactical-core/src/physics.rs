use adventuresim_core::prelude::*;
use avian3d::{collider_tree::ColliderTreeSystems, prelude::*};
use bevy::prelude::*;
use bevy_ahoy::{
    AhoyPlugins, AhoySystems, CharacterController, camera::AhoyCameraPlugin,
    input::AccumulatedInput,
};

use crate::{
    animation::{SkeletonState, WeaponGuardState},
    player::TacticalPlayerViewer,
};

#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Reflect,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum MovementPace {
    #[default]
    Walk,
    Jog,
    Sprint,
}

pub const TACTICAL_WALK_SPEED_METRES_PER_SECOND: f32 = 2.0;
pub const BREATH_RECOVERY_PER_ENDURANCE_PER_SECOND: f32 = 0.0031875;
pub const BREATH_PER_METRE_PER_SECOND: f32 = 0.0034;
const REFERENCE_LEG_STRENGTH: f32 = 3.0;
const REFERENCE_BURDEN_KG: f32 = 70.0;

/// Maximum ordinary tactical movement speed, in metres per second.
pub const TACTICAL_RUN_SPEED_METRES_PER_SECOND: f32 = 5.5;

/// Maximum server-authoritative movement speed while the weapon guard is raised.
pub const TACTICAL_GUARD_SPEED_METRES_PER_SECOND: f32 = 2.0;

/// Ahoy multiplies requested speed by this frequency to obtain ground
/// acceleration. Scale the raised-guard frequency against its lower speed cap
/// so full and partial analogue input accelerate exactly as quickly as normal
/// running while retaining the 2 m/s maximum.
pub const TACTICAL_RUN_ACCELERATION_HZ: f32 = 8.0;
pub const TACTICAL_GROUND_ACCELERATION_METRES_PER_SECOND_SQUARED: f32 =
    TACTICAL_RUN_SPEED_METRES_PER_SECOND * TACTICAL_RUN_ACCELERATION_HZ;

pub fn tactical_jog_speed(endurance: f32) -> f32 {
    (endurance.max(0.0) * BREATH_RECOVERY_PER_ENDURANCE_PER_SECOND / BREATH_PER_METRE_PER_SECOND)
        .clamp(
            TACTICAL_WALK_SPEED_METRES_PER_SECOND,
            TACTICAL_RUN_SPEED_METRES_PER_SECOND,
        )
}

pub fn tactical_sprint_speed(
    left_leg_strength: f32,
    right_leg_strength: f32,
    left_leg_health: f32,
    right_leg_health: f32,
    burden_kg: f32,
) -> f32 {
    let effective_strength = ((left_leg_strength * left_leg_health.max(0.0)
        + right_leg_strength * right_leg_health.max(0.0))
        * 0.5)
        .max(0.0);
    let strength_ratio = effective_strength / REFERENCE_LEG_STRENGTH;
    let burden_ratio = REFERENCE_BURDEN_KG / burden_kg.max(1.0);
    (TACTICAL_RUN_SPEED_METRES_PER_SECOND * (strength_ratio * burden_ratio).sqrt())
        .clamp(TACTICAL_WALK_SPEED_METRES_PER_SECOND, 8.0)
}

/// Builds the shared tactical controller instead of inheriting Ahoy's much
/// faster general-purpose default.
pub fn tactical_character_controller() -> CharacterController {
    CharacterController {
        speed: TACTICAL_RUN_SPEED_METRES_PER_SECOND,
        acceleration_hz: TACTICAL_RUN_ACCELERATION_HZ,
        ..default()
    }
}

pub fn tactical_movement_acceleration_hz_for_guard(weapon_guard: WeaponGuardState) -> f32 {
    let speed_cap = match weapon_guard {
        WeaponGuardState::Lowered => TACTICAL_RUN_SPEED_METRES_PER_SECOND,
        WeaponGuardState::Raised => TACTICAL_GUARD_SPEED_METRES_PER_SECOND,
    };
    TACTICAL_GROUND_ACCELERATION_METRES_PER_SECOND_SQUARED / speed_cap
}

/// Preserves the analogue movement magnitude after Ahoy normalizes its wish
/// direction. Keyboard input has magnitude one; a half-deflected stick sets a
/// half-speed controller target.
pub fn tactical_movement_speed(movement: Option<Vec2>) -> f32 {
    tactical_movement_speed_for_guard(movement, WeaponGuardState::Lowered)
}

/// Resolves the controller target speed at the server-side Ahoy seam. The
/// radial input magnitude is retained after Ahoy normalizes its wish direction.
pub fn tactical_movement_speed_for_guard(
    movement: Option<Vec2>,
    weapon_guard: WeaponGuardState,
) -> f32 {
    let cap = match weapon_guard {
        WeaponGuardState::Lowered => TACTICAL_RUN_SPEED_METRES_PER_SECOND,
        WeaponGuardState::Raised => TACTICAL_GUARD_SPEED_METRES_PER_SECOND,
    };
    cap * movement.map_or(0.0, |movement| movement.length().clamp(0.0, 1.0))
}

pub fn tactical_movement_speed_for_pace(
    movement: Option<Vec2>,
    pace: MovementPace,
    weapon_guard: WeaponGuardState,
    jog_speed: f32,
    sprint_speed: f32,
) -> f32 {
    let requested = match pace {
        MovementPace::Walk => TACTICAL_WALK_SPEED_METRES_PER_SECOND,
        MovementPace::Jog => jog_speed,
        MovementPace::Sprint => sprint_speed,
    };
    let cap = if weapon_guard == WeaponGuardState::Raised {
        match pace {
            MovementPace::Sprint => jog_speed,
            _ => requested.min(TACTICAL_GUARD_SPEED_METRES_PER_SECOND),
        }
    } else {
        requested
    };
    cap * movement.map_or(0.0, |movement| movement.length().clamp(0.0, 1.0))
}

pub struct AdventureSimulatorPhysicsPlugin {
    pub enable_simulation: bool,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdventureSimulatorPhysicsSet {
    ApplyMovementSpeed,
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
                apply_analogue_movement_speed
                    .in_set(AdventureSimulatorPhysicsSet::ApplyMovementSpeed)
                    .before(AhoySystems::MoveCharacters),
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
            // `SolverSystems::Finalize` is normally nested by Avian's solver
            // plugin. This read-only fixture omits the solver, so retain the
            // collider-tree completion step in the equivalent physics phase.
            .configure_sets(
                PhysicsSchedule,
                ColliderTreeSystems::EndOptimize.in_set(PhysicsStepSystems::Finalize),
            )
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
            .init_resource::<PhysicsLengthUnit>();
    }
}

fn apply_analogue_movement_speed(
    mut controllers: Query<(
        Entity,
        &AccumulatedInput,
        &mut CharacterController,
        Option<&SkeletonState>,
        Option<&MovementPace>,
    )>,
    viewer: TacticalPlayerViewer,
) {
    for (entity, input, mut controller, skeleton, pace) in &mut controllers {
        let guard = skeleton.map_or(WeaponGuardState::Lowered, SkeletonState::weapon_guard);
        let Some(pace) = pace else {
            controller.speed = tactical_movement_speed_for_guard(input.last_movement, guard);
            controller.acceleration_hz = tactical_movement_acceleration_hz_for_guard(guard);
            continue;
        };
        let (jog, sprint) =
            viewer
                .get(entity)
                .map_or((3.75, TACTICAL_RUN_SPEED_METRES_PER_SECOND), |player| {
                    let burden = player.body_weight() + player.inventory_weight();
                    (
                        tactical_jog_speed(
                            player.raw_single_body_part_attr(SimpleAttribute::Endurance),
                        ),
                        tactical_sprint_speed(
                            player.raw_limb_attr(LimbAttribute::Strength, BodyPart::LeftLeg),
                            player.raw_limb_attr(LimbAttribute::Strength, BodyPart::RightLeg),
                            player.body_part_health(BodyPart::LeftLeg),
                            player.body_part_health(BodyPart::RightLeg),
                            burden,
                        ),
                    )
                });
        controller.speed =
            tactical_movement_speed_for_pace(input.last_movement, *pace, guard, jog, sprint);
        let magnitude = input
            .last_movement
            .map_or(0.0, |movement| movement.length().clamp(0.0, 1.0));
        controller.acceleration_hz = if magnitude > f32::EPSILON {
            TACTICAL_GROUND_ACCELERATION_METRES_PER_SECOND_SQUARED
                / (controller.speed / magnitude).max(0.01)
        } else {
            tactical_movement_acceleration_hz_for_guard(guard)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_speed_preserves_stick_magnitude_and_caps_diagonals() {
        for (guard, cap) in [
            (
                WeaponGuardState::Lowered,
                TACTICAL_RUN_SPEED_METRES_PER_SECOND,
            ),
            (
                WeaponGuardState::Raised,
                TACTICAL_GUARD_SPEED_METRES_PER_SECOND,
            ),
        ] {
            assert_eq!(tactical_movement_speed_for_guard(None, guard), 0.0);
            assert_eq!(
                tactical_movement_speed_for_guard(Some(Vec2::new(0.3, 0.4)), guard),
                cap * 0.5
            );
            assert_eq!(
                tactical_movement_speed_for_guard(Some(Vec2::ONE), guard),
                cap
            );
        }
        assert_eq!(
            tactical_movement_speed(Some(Vec2::new(0.3, 0.4))),
            TACTICAL_RUN_SPEED_METRES_PER_SECOND * 0.5
        );
    }

    #[test]
    fn controller_speed_system_uses_guard_state_and_lowered_fallback() {
        let mut world = World::new();
        let raised = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::new(0.3, 0.4)),
                    ..default()
                },
                CharacterController::default(),
                SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised),
            ))
            .id();
        let generic = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::ONE),
                    ..default()
                },
                CharacterController::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_analogue_movement_speed);
        schedule.run(&mut world);

        assert_eq!(
            world.get::<CharacterController>(raised).unwrap().speed,
            TACTICAL_GUARD_SPEED_METRES_PER_SECOND * 0.5
        );
        assert_eq!(
            world
                .get::<CharacterController>(raised)
                .unwrap()
                .acceleration_hz,
            TACTICAL_GROUND_ACCELERATION_METRES_PER_SECOND_SQUARED
                / TACTICAL_GUARD_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            world.get::<CharacterController>(generic).unwrap().speed,
            TACTICAL_RUN_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            world
                .get::<CharacterController>(generic)
                .unwrap()
                .acceleration_hz,
            TACTICAL_RUN_ACCELERATION_HZ
        );
    }

    #[test]
    fn raised_guard_and_running_have_equal_absolute_acceleration() {
        let running = TACTICAL_RUN_SPEED_METRES_PER_SECOND
            * tactical_movement_acceleration_hz_for_guard(WeaponGuardState::Lowered);
        let guarded = TACTICAL_GUARD_SPEED_METRES_PER_SECOND
            * tactical_movement_acceleration_hz_for_guard(WeaponGuardState::Raised);
        assert_eq!(running, guarded);
        assert_eq!(
            guarded,
            TACTICAL_GROUND_ACCELERATION_METRES_PER_SECOND_SQUARED
        );
    }

    #[test]
    fn character_paces_are_reference_normalized_and_guard_sprint_becomes_jog() {
        assert!((tactical_jog_speed(4.0) - 3.75).abs() < 0.0001);
        assert_eq!(
            tactical_sprint_speed(
                REFERENCE_LEG_STRENGTH,
                REFERENCE_LEG_STRENGTH,
                1.0,
                1.0,
                REFERENCE_BURDEN_KG,
            ),
            TACTICAL_RUN_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            tactical_movement_speed_for_pace(
                Some(Vec2::Y),
                MovementPace::Sprint,
                WeaponGuardState::Raised,
                3.75,
                6.5,
            ),
            3.75
        );
    }
}
