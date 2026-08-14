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

pub const TACTICAL_WALK_SPEED_METRES_PER_SECOND: f32 = 1.4;
pub const BREATH_PER_METRE_PER_SECOND: f32 = 0.0034;
pub const TACTICAL_BREATH_RESPONSE_SCALE: f32 = 5.0;
const REFERENCE_LEG_STRENGTH: f32 = 3.0;
const REFERENCE_BURDEN_KG: f32 = 70.0;
const MINIMUM_JOG_SPEED_METRES_PER_SECOND: f32 = 1.8;
const ELITE_MARATHON_SPEED_METRES_PER_SECOND: f32 = 5.83;
const JOG_ENDURANCE_CURVE_EXPONENT: f32 = 1.873_873;
const REFERENCE_SPRINT_SPEED_METRES_PER_SECOND: f32 = 8.0;
const ELITE_SPRINT_SPEED_METRES_PER_SECOND: f32 = 12.4;

/// Authored ordinary-run reference speed and fallback for entities without
/// character attributes, in metres per second.
pub const TACTICAL_RUN_SPEED_METRES_PER_SECOND: f32 = 5.5;

/// Maximum server-authoritative movement speed while the weapon guard is raised.
pub const TACTICAL_GUARD_SPEED_METRES_PER_SECOND: f32 = 2.0;
pub const TACTICAL_PRONE_SPEED_METRES_PER_SECOND: f32 = 3.0;
pub const TACTICAL_SUPINE_SPEED_METRES_PER_SECOND: f32 = 2.4;
pub const TACTICAL_ROLL_SPEED_METRES_PER_SECOND: f32 = 0.65;
pub const TACTICAL_JUMP_HEIGHT_METRES: f32 = 1.8;
pub const TACTICAL_DIVE_JUMP_HEIGHT_METRES: f32 = 0.65;
pub const TACTICAL_DIVE_HORIZONTAL_SPEED_METRES_PER_SECOND: f32 = 7.0;

/// Ahoy multiplies requested speed by this frequency to obtain ground
/// acceleration. Scale the raised-guard frequency against its lower speed cap
/// so full and partial analogue input accelerate exactly as quickly as normal
/// running while retaining the 2 m/s maximum.
pub const TACTICAL_RUN_ACCELERATION_HZ: f32 = 8.0;
pub const TACTICAL_GROUND_ACCELERATION_METRES_PER_SECOND_SQUARED: f32 =
    TACTICAL_RUN_SPEED_METRES_PER_SECOND * TACTICAL_RUN_ACCELERATION_HZ;

pub fn tactical_jog_speed(endurance: f32) -> f32 {
    let endurance = endurance.clamp(0.0, 5.0);
    if endurance <= 1.0 {
        let smooth_endurance = endurance * endurance * (3.0 - 2.0 * endurance);
        return TACTICAL_WALK_SPEED_METRES_PER_SECOND
            .lerp(MINIMUM_JOG_SPEED_METRES_PER_SECOND, smooth_endurance);
    }

    let normalized = (endurance - 1.0) / 4.0;
    MINIMUM_JOG_SPEED_METRES_PER_SECOND
        + (ELITE_MARATHON_SPEED_METRES_PER_SECOND - MINIMUM_JOG_SPEED_METRES_PER_SECOND)
            * normalized.powf(JOG_ENDURANCE_CURVE_EXPONENT)
}

pub fn tactical_breath_recovery_per_second(endurance: f32) -> f32 {
    tactical_jog_speed(endurance) * BREATH_PER_METRE_PER_SECOND
}

/// Positive values accumulate tactical breath exhaustion; negative values
/// recover it. `effort_speed` describes intentional exertion rather than
/// physics velocity, so external impulses do not make a character winded.
pub fn tactical_exhaustion_change_per_second(effort_speed: f32, endurance: f32) -> f32 {
    let effort_speed = if effort_speed.is_finite() {
        effort_speed.max(0.0)
    } else {
        0.0
    };
    (effort_speed * BREATH_PER_METRE_PER_SECOND - tactical_breath_recovery_per_second(endurance))
        * TACTICAL_BREATH_RESPONSE_SCALE
}

/// Movement's additive contribution to tactical exhaustion. A full jog is
/// explicitly capped at neutral, while partial jog input and walking recover
/// exhaustion and sprinting accumulates it. Other exhaustion sources can add
/// their own rates without being cleared by the selected movement pace.
pub fn tactical_movement_exhaustion_change_per_second(
    movement: Option<Vec2>,
    pace: MovementPace,
    weapon_guard: WeaponGuardState,
    endurance: f32,
    sprint_speed: f32,
) -> f32 {
    let jog_speed = tactical_jog_speed(endurance);
    let effort_speed =
        tactical_movement_speed_for_pace(movement, pace, weapon_guard, jog_speed, sprint_speed);
    let change = tactical_exhaustion_change_per_second(effort_speed, endurance);
    if pace == MovementPace::Jog {
        change.min(0.0)
    } else {
        change
    }
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
    let strength_speed = (REFERENCE_SPRINT_SPEED_METRES_PER_SECOND
        - TACTICAL_WALK_SPEED_METRES_PER_SECOND)
        * strength_ratio;
    (TACTICAL_WALK_SPEED_METRES_PER_SECOND + strength_speed * burden_ratio.sqrt()).clamp(
        TACTICAL_WALK_SPEED_METRES_PER_SECOND,
        ELITE_SPRINT_SPEED_METRES_PER_SECOND,
    )
}

/// Builds the shared tactical controller instead of inheriting Ahoy's much
/// faster general-purpose default.
pub fn tactical_character_controller() -> CharacterController {
    CharacterController {
        speed: TACTICAL_RUN_SPEED_METRES_PER_SECOND,
        acceleration_hz: TACTICAL_RUN_ACCELERATION_HZ,
        jump_height: TACTICAL_JUMP_HEIGHT_METRES,
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
        controller.jump_height = if skeleton.is_some_and(|skeleton| {
            matches!(
                skeleton
                    .posture_transition()
                    .map(|transition| transition.kind()),
                Some(crate::animation::PostureTransitionKind::DiveToDowned { .. })
            )
        }) {
            TACTICAL_DIVE_JUMP_HEIGHT_METRES
        } else {
            TACTICAL_JUMP_HEIGHT_METRES
        };
        let guard = skeleton.map_or(WeaponGuardState::Lowered, SkeletonState::weapon_guard);
        let roll_motion = skeleton.map_or(0.0, SkeletonState::downed_lateral_motion);
        if roll_motion.abs() > f32::EPSILON {
            controller.speed = TACTICAL_ROLL_SPEED_METRES_PER_SECOND * roll_motion.abs();
            controller.acceleration_hz =
                TACTICAL_GROUND_ACCELERATION_METRES_PER_SECOND_SQUARED / controller.speed.max(0.01);
            continue;
        }
        if skeleton.is_some_and(SkeletonState::is_posture_transitioning) {
            controller.speed = 0.0;
            continue;
        }
        let posture_cap = skeleton.and_then(|skeleton| match skeleton.body() {
            crate::animation::BodyState::Prone => Some(TACTICAL_PRONE_SPEED_METRES_PER_SECOND),
            crate::animation::BodyState::Supine => Some(TACTICAL_SUPINE_SPEED_METRES_PER_SECOND),
            _ => None,
        });
        let Some(pace) = pace else {
            controller.speed = tactical_movement_speed_for_guard(input.last_movement, guard);
            if let Some(cap) = posture_cap {
                controller.speed = controller.speed.min(cap);
            }
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
        if let Some(cap) = posture_cap {
            controller.speed = controller.speed.min(cap);
        }
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
        for (endurance, expected_speed) in [
            (0.0, TACTICAL_WALK_SPEED_METRES_PER_SECOND),
            (1.0, 1.8),
            (2.0, 2.10),
            (3.0, 2.90),
            (4.0, 4.15),
            (5.0, ELITE_MARATHON_SPEED_METRES_PER_SECOND),
        ] {
            assert!(
                (tactical_jog_speed(endurance) - expected_speed).abs() < 0.015,
                "endurance {endurance} should jog near {expected_speed} m/s"
            );
        }
        assert!(
            (tactical_breath_recovery_per_second(3.0)
                - tactical_jog_speed(3.0) * BREATH_PER_METRE_PER_SECOND)
                .abs()
                < f32::EPSILON
        );
        assert!(tactical_exhaustion_change_per_second(8.0, 3.0) > 0.0);
        assert!(
            (tactical_exhaustion_change_per_second(8.0, 3.0)
                - (8.0 - tactical_jog_speed(3.0))
                    * BREATH_PER_METRE_PER_SECOND
                    * TACTICAL_BREATH_RESPONSE_SCALE)
                .abs()
                < f32::EPSILON
        );
        assert!(
            tactical_exhaustion_change_per_second(tactical_jog_speed(3.0), 3.0).abs()
                < f32::EPSILON
        );
        assert!(tactical_exhaustion_change_per_second(0.0, 3.0) < 0.0);
        assert_eq!(
            tactical_movement_exhaustion_change_per_second(
                Some(Vec2::Y),
                MovementPace::Jog,
                WeaponGuardState::Lowered,
                3.0,
                8.0,
            ),
            0.0
        );
        assert!(
            tactical_movement_exhaustion_change_per_second(
                Some(Vec2::splat(0.71)),
                MovementPace::Jog,
                WeaponGuardState::Lowered,
                3.0,
                8.0,
            ) <= 0.0
        );
        assert!(
            tactical_movement_exhaustion_change_per_second(
                Some(Vec2::Y),
                MovementPace::Sprint,
                WeaponGuardState::Lowered,
                3.0,
                8.0,
            ) > 0.0
        );
        assert_eq!(
            tactical_sprint_speed(
                REFERENCE_LEG_STRENGTH,
                REFERENCE_LEG_STRENGTH,
                1.0,
                1.0,
                REFERENCE_BURDEN_KG,
            ),
            REFERENCE_SPRINT_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            tactical_sprint_speed(5.0, 5.0, 1.0, 1.0, REFERENCE_BURDEN_KG),
            ELITE_SPRINT_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            tactical_movement_speed_for_pace(
                Some(Vec2::Y),
                MovementPace::Sprint,
                WeaponGuardState::Raised,
                3.96,
                6.5,
            ),
            3.96
        );
    }

    #[test]
    fn sprint_speed_retains_injury_and_burden_penalties() {
        let healthy = tactical_sprint_speed(3.0, 3.0, 1.0, 1.0, REFERENCE_BURDEN_KG);
        let injured = tactical_sprint_speed(3.0, 3.0, 0.5, 0.5, REFERENCE_BURDEN_KG);
        let burdened = tactical_sprint_speed(3.0, 3.0, 1.0, 1.0, REFERENCE_BURDEN_KG * 2.0);
        assert!(injured < healthy);
        assert!(burdened < healthy);
    }

    #[test]
    fn prone_and_supine_ignore_faster_paces_and_transition_stops_motion() {
        let mut world = World::new();
        let prone = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                SkeletonState::default().with_body_state(crate::animation::BodyState::Prone),
                MovementPace::Sprint,
            ))
            .id();
        let supine = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                SkeletonState::default().with_body_state(crate::animation::BodyState::Supine),
                MovementPace::Sprint,
            ))
            .id();
        let mut transitioning = SkeletonState::default();
        assert!(transitioning.begin_posture_transition(
            crate::animation::PostureTransitionKind::UprightToProne,
            0,
            10,
        ));
        let transition = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                transitioning,
                MovementPace::Sprint,
            ))
            .id();
        let mut rolling =
            SkeletonState::default().with_body_state(crate::animation::BodyState::Prone);
        assert!(rolling.begin_posture_transition(
            crate::animation::PostureTransitionKind::ProneToSupine {
                direction: crate::animation::RollDirection::Left,
            },
            0,
            10,
        ));
        let roll = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(-Vec2::X),
                    ..default()
                },
                CharacterController::default(),
                rolling,
                MovementPace::Sprint,
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_analogue_movement_speed);
        schedule.run(&mut world);

        assert_eq!(
            world.get::<CharacterController>(prone).unwrap().speed,
            TACTICAL_PRONE_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            world.get::<CharacterController>(supine).unwrap().speed,
            TACTICAL_SUPINE_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            world.get::<CharacterController>(transition).unwrap().speed,
            0.0
        );
        assert_eq!(
            world.get::<CharacterController>(roll).unwrap().speed,
            TACTICAL_ROLL_SPEED_METRES_PER_SECOND
        );
    }
}
