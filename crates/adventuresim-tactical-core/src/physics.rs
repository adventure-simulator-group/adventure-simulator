use adventuresim_core::prelude::*;
use avian3d::{collider_tree::ColliderTreeSystems, prelude::*};
use bevy::prelude::*;
use bevy_ahoy::{
    AhoyPlugins, AhoySystems, CharacterController, CharacterControllerState, CharacterLook,
    camera::AhoyCameraPlugin, input::AccumulatedInput,
};

use crate::{
    animation::{LOCOMOTION_SAMPLE_HZ, SkeletonState, WeaponGuardState},
    player::{CharacterDimensions, TacticalPlayerViewer},
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
const MINIMUM_ORDINARY_TURN_RADIUS_METRES: f32 = 0.25;
// A trailing-leg guard contact does not actively brake at the motor's full
// stopping force, but it now uses most of that reserve so movement-direction
// momentum falls promptly while the leading foot is airborne.

/// Authored ordinary-run reference speed and fallback for entities without
/// character attributes, in metres per second.
pub const TACTICAL_RUN_SPEED_METRES_PER_SECOND: f32 = 5.5;

/// Maximum server-authoritative movement speed while the weapon guard is raised.
pub const TACTICAL_GUARD_SPEED_METRES_PER_SECOND: f32 = 2.0;

/// Deliberate prone crawl speed used by the ordinary walking control mode.
pub const TACTICAL_PRONE_WALK_SPEED_METRES_PER_SECOND: f32 = 0.45;
/// Maximum urgent unencumbered crawl speed used while sprint is held.
pub const TACTICAL_PRONE_SPEED_METRES_PER_SECOND: f32 = 2.0;
/// Body-local lateral crawl is intentionally slower than longitudinal travel.
pub const TACTICAL_PRONE_LATERAL_SPEED_SCALE: f32 = 0.375;
/// Crawling costs roughly three times the effort of upright travel at the same
/// physical speed. Dividing the character's neutral jog by the same factor
/// gives prone movement a breath-neutral middle pace.
const TACTICAL_PRONE_EFFORT_SCALE: f32 = 3.0;
pub const TACTICAL_ROLL_SPEED_METRES_PER_SECOND: f32 = 1.3;
pub const TACTICAL_JUMP_HEIGHT_METRES: f32 = 0.30;
pub const TACTICAL_DIVE_JUMP_HEIGHT_METRES: f32 = 0.20;
pub const TACTICAL_DIVE_HORIZONTAL_SPEED_METRES_PER_SECOND: f32 = 7.0;
pub const TACTICAL_QUICKSTEP_SPEED_METRES_PER_SECOND: f32 = 3.5;
pub const TACTICAL_GRAVITY_METRES_PER_SECOND_SQUARED: f32 = 9.81;
pub const TACTICAL_MAXIMUM_STEP_HEIGHT_METRES: f32 = 0.35;
pub const TACTICAL_MAXIMUM_WALKABLE_SLOPE_DEGREES: f32 = 40.0;

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
    body: crate::animation::BodyState,
    endurance: f32,
    sprint_speed: f32,
) -> f32 {
    let jog_speed = tactical_jog_speed(endurance);
    let effort_speed = if body == crate::animation::BodyState::Prone {
        tactical_prone_movement_speed_for_pace(movement, pace, jog_speed)
            * TACTICAL_PRONE_EFFORT_SCALE
    } else {
        tactical_movement_speed_for_pace(movement, pace, weapon_guard, jog_speed, sprint_speed)
    };
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
        // Horizontal velocity is owned by the Fabelgeist motor below. Ahoy
        // remains the collision-constraint solver and must not accelerate or
        // brake the same state a second time.
        acceleration_hz: 0.0,
        air_acceleration_hz: 0.0,
        water_acceleration_hz: 0.0,
        friction_hz: 0.0,
        stop_speed: 0.0,
        gravity: TACTICAL_GRAVITY_METRES_PER_SECOND_SQUARED,
        step_size: TACTICAL_MAXIMUM_STEP_HEIGHT_METRES,
        min_walk_cos: TACTICAL_MAXIMUM_WALKABLE_SLOPE_DEGREES.to_radians().cos(),
        jump_height: TACTICAL_JUMP_HEIGHT_METRES,
        ..default()
    }
}

/// Compact rollback boundary produced after an authoritative controller step.
/// Clients can restore this state at `acknowledged_input_tick` and replay newer
/// locally buffered input without treating a render transform as simulation
/// authority.
#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Reflect, serde::Serialize, serde::Deserialize,
)]
#[reflect(Component)]
pub struct CharacterMotionSnapshot {
    pub acknowledged_input_tick: u32,
    pub translation: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub grounded: bool,
    pub quickstep_push: QuickstepPush,
}

/// Rollback-safe state for a quickstep's finite, foot-supported push.
///
/// This is deliberately a force actuator, not a pending velocity change. The
/// direction and start tick are committed when the input edge is accepted;
/// the shared fixed-tick motor then integrates the same force curve on an
/// authoritative server or a predicting client.
#[derive(
    Component, Debug, Clone, Copy, Default, PartialEq, Reflect, serde::Serialize, serde::Deserialize,
)]
#[reflect(Component)]
pub struct QuickstepPush {
    pub start_tick: u64,
    pub direction: Vec2,
    pub orientation: Quat,
    pub origin: Vec3,
    pub active: bool,
}

#[derive(Component, Debug, Clone, Copy, Default)]
struct QuickstepMismatchLogged;

impl QuickstepPush {
    pub fn begin(&mut self, start_tick: u64, direction: Vec2, orientation: Quat, origin: Vec3) {
        self.start_tick = start_tick;
        self.direction = direction.normalize_or_zero();
        self.orientation = orientation;
        self.origin = origin;
        self.active = self.direction != Vec2::ZERO;
    }

    pub fn cancel(&mut self) {
        self.active = false;
    }
}

const QUICKSTEP_FORCE_CURVE_RAMP_FRACTION: f32 = 0.31;
/// Smooth rise, maximal mid-push force, and smooth release. Its normalized
/// area is 0.69, matching the force-time profile used to calibrate the
/// biomechanical values in `combat.yaml`.
pub fn quickstep_force_curve(normalized_time: f32) -> f32 {
    let t = normalized_time.clamp(0.0, 1.0);
    let edge = t.min(1.0 - t);
    if edge >= QUICKSTEP_FORCE_CURVE_RAMP_FRACTION {
        1.0
    } else {
        let x = edge / QUICKSTEP_FORCE_CURVE_RAMP_FRACTION;
        x * x * (3.0 - 2.0 * x)
    }
}

pub fn quickstep_push_seconds(
    leg_agility: f32,
    motor: &crate::combat_config::CharacterMotorConfig,
) -> f32 {
    (motor.reference_quickstep_push_seconds
        - (leg_agility - motor.reference_leg_agility)
            * motor.quickstep_push_seconds_reduction_per_agility)
        .max(1.0 / LOCOMOTION_SAMPLE_HZ)
}

pub fn quickstep_peak_horizontal_force_newtons(
    biological_mass_kg: f32,
    leg_strength: f32,
    motor: &crate::combat_config::CharacterMotorConfig,
) -> f32 {
    let bodyweights = (motor.reference_quickstep_peak_horizontal_force_bodyweights
        + (leg_strength - motor.reference_leg_strength)
            * motor.quickstep_peak_horizontal_force_bodyweights_per_strength)
        .max(0.0);
    biological_mass_kg.max(0.0) * motor.gravity_metres_per_second_squared * bodyweights
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuickstepMotionTarget {
    pub displacement_metres: f32,
    pub velocity_metres_per_second: f32,
    pub acceleration_metres_per_second_squared: f32,
}

/// Scales the authored reference travel by the character's hip-to-ankle chain.
pub fn quickstep_target_displacement_metres(
    leg_length_metres: f32,
    motor: &crate::combat_config::CharacterMotorConfig,
) -> f32 {
    let reference_leg_length = motor
        .reference_quickstep_leg_length_metres
        .max(f32::EPSILON);
    let leg_length = if leg_length_metres.is_finite() && leg_length_metres > 0.0 {
        leg_length_metres
    } else {
        reference_leg_length
    };
    motor.reference_quickstep_target_displacement_metres * leg_length / reference_leg_length
}

/// Samples a monotone cubic through the authored 0/3/6/9/12 root positions.
/// End tangents are zero so the physical body can enter and leave combat idle
/// without an instantaneous velocity change.
pub fn quickstep_motion_target(
    normalized_time: f32,
    target_displacement_metres: f32,
    duration_seconds: f32,
    profile: [f32; 5],
) -> QuickstepMotionTarget {
    let phase = normalized_time.clamp(0.0, 1.0);
    if phase >= 1.0 {
        return QuickstepMotionTarget {
            displacement_metres: target_displacement_metres,
            velocity_metres_per_second: 0.0,
            acceleration_metres_per_second_squared: 0.0,
        };
    }
    let segment = ((phase * 4.0).floor() as usize).min(3);
    let u = phase * 4.0 - segment as f32;
    let secants =
        std::array::from_fn::<_, 4, _>(|index| (profile[index + 1] - profile[index]) * 4.0);
    let mut tangents = [0.0; 5];
    for index in 1..4 {
        let before = secants[index - 1];
        let after = secants[index];
        if before > 0.0 && after > 0.0 {
            tangents[index] = 2.0 * before * after / (before + after);
        }
    }
    let p0 = profile[segment];
    let p1 = profile[segment + 1];
    let m0 = tangents[segment] * 0.25;
    let m1 = tangents[segment + 1] * 0.25;
    let u2 = u * u;
    let u3 = u2 * u;
    let position = (2.0 * u3 - 3.0 * u2 + 1.0) * p0
        + (u3 - 2.0 * u2 + u) * m0
        + (-2.0 * u3 + 3.0 * u2) * p1
        + (u3 - u2) * m1;
    let derivative_u = (6.0 * u2 - 6.0 * u) * p0
        + (3.0 * u2 - 4.0 * u + 1.0) * m0
        + (-6.0 * u2 + 6.0 * u) * p1
        + (3.0 * u2 - 2.0 * u) * m1;
    let second_derivative_u = (12.0 * u - 6.0) * p0
        + (6.0 * u - 4.0) * m0
        + (-12.0 * u + 6.0) * p1
        + (6.0 * u - 2.0) * m1;
    let duration = duration_seconds.max(f32::EPSILON);
    QuickstepMotionTarget {
        displacement_metres: position * target_displacement_metres,
        velocity_metres_per_second: derivative_u * 4.0 * target_displacement_metres / duration,
        acceleration_metres_per_second_squared: second_derivative_u
            * 16.0
            * target_displacement_metres
            / (duration * duration),
    }
}

/// Returns the bounded longitudinal ground-reaction force needed to follow the
/// authored trajectory. Position and velocity feedback correct collision and
/// integration error; neither term directly changes transform or velocity.
pub fn quickstep_tracking_force_newtons(
    displacement_metres: f32,
    velocity_metres_per_second: f32,
    target: QuickstepMotionTarget,
    mass_kg: f32,
    maximum_force_newtons: f32,
    delta_seconds: f32,
) -> f32 {
    const TRACKING_FREQUENCY_RADIANS_PER_SECOND: f32 = 12.0;
    // Ground projection can leave a few ten-thousandths of a metre per second
    // behind on an otherwise stationary character. Treat that contact noise as
    // rest: applying the reversal guard to it limits a launch to the tiny force
    // needed to reach exactly zero velocity, suppressing the entire quickstep.
    const VELOCITY_REVERSAL_GUARD_METRES_PER_SECOND: f32 = 0.02;
    let position_error = target.displacement_metres - displacement_metres;
    let velocity_error = target.velocity_metres_per_second - velocity_metres_per_second;
    let acceleration = target.acceleration_metres_per_second_squared
        + TRACKING_FREQUENCY_RADIANS_PER_SECOND.powi(2) * position_error
        + 2.0 * TRACKING_FREQUENCY_RADIANS_PER_SECOND * velocity_error;
    let mass = mass_kg.max(1.0);
    let force = (mass * acceleration).clamp(
        -maximum_force_newtons.max(0.0),
        maximum_force_newtons.max(0.0),
    );
    let stopping_force = -velocity_metres_per_second * mass / delta_seconds.max(f32::EPSILON);
    if velocity_metres_per_second > VELOCITY_REVERSAL_GUARD_METRES_PER_SECOND {
        force.max(stopping_force)
    } else if velocity_metres_per_second < -VELOCITY_REVERSAL_GUARD_METRES_PER_SECOND {
        force.min(stopping_force)
    } else {
        force
    }
}

/// Converts the configured complete quickstep duration into the midpoint tick
/// expected by the shared symmetric dodge timeline. Keeping this conversion
/// in one place prevents a contact duration from accidentally being doubled.
pub fn quickstep_action_contact_ticks(duration_seconds: f32) -> u64 {
    let total_ticks = (duration_seconds.max(2.0 / LOCOMOTION_SAMPLE_HZ) * LOCOMOTION_SAMPLE_HZ)
        .round()
        .max(2.0) as u64;
    (total_ticks / 2).max(1)
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

fn tactical_prone_movement_speed_for_pace(
    movement: Option<Vec2>,
    pace: MovementPace,
    upright_jog_speed: f32,
) -> f32 {
    let speed = tactical_prone_speed_for_pace(pace, upright_jog_speed);
    let movement = movement.unwrap_or_default();
    speed
        * Vec2::new(movement.x * TACTICAL_PRONE_LATERAL_SPEED_SCALE, movement.y)
            .length()
            .clamp(0.0, 1.0)
}

fn tactical_prone_speed_for_pace(pace: MovementPace, upright_jog_speed: f32) -> f32 {
    match pace {
        MovementPace::Walk => TACTICAL_PRONE_WALK_SPEED_METRES_PER_SECOND,
        MovementPace::Jog => upright_jog_speed / TACTICAL_PRONE_EFFORT_SCALE,
        MovementPace::Sprint => TACTICAL_PRONE_SPEED_METRES_PER_SECOND,
    }
}

pub struct AdventureSimulatorPhysicsPlugin {
    pub enable_simulation: bool,
    /// Runs Avian's solver for explicitly enabled client-only presentation
    /// bodies while keeping every ordinary replicated rigid body disabled.
    pub enable_presentation_simulation: bool,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdventureSimulatorPhysicsSet {
    ApplyCharacterMotor,
}

impl Default for AdventureSimulatorPhysicsPlugin {
    fn default() -> Self {
        Self {
            enable_simulation: true,
            enable_presentation_simulation: false,
        }
    }
}

impl Plugin for AdventureSimulatorPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::combat_config::TacticalCombatConfig>();
        if self.enable_simulation {
            app.add_plugins((
                PhysicsPlugins::new(FixedPostUpdate),
                AhoyPlugins::new(FixedPostUpdate),
            ))
            .add_systems(
                FixedPostUpdate,
                apply_character_motor
                    .in_set(AdventureSimulatorPhysicsSet::ApplyCharacterMotor)
                    .before(AhoySystems::MoveCharacters),
            );
        } else if self.enable_presentation_simulation {
            app.add_plugins((PhysicsPlugins::new(FixedPostUpdate), AhoyCameraPlugin))
                .register_required_components::<RigidBody, RigidBodyDisabled>();
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

fn apply_character_motor(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut controllers: Query<(
        Entity,
        &AccumulatedInput,
        &mut CharacterController,
        &CharacterControllerState,
        &mut LinearVelocity,
        Option<&mut Mass>,
        Option<&CharacterLook>,
        Option<&SkeletonState>,
        Option<&MovementPace>,
        Option<&mut QuickstepPush>,
        &Transform,
        Has<QuickstepMismatchLogged>,
    )>,
    viewer: TacticalPlayerViewer,
    dimensions: Query<&CharacterDimensions>,
    combat_config: Res<crate::combat_config::TacticalCombatConfig>,
) {
    let movement_config = &combat_config.movement;
    let speeds = &movement_config.speeds_metres_per_second;
    for (
        entity,
        input,
        mut controller,
        controller_state,
        mut velocity,
        mass_component,
        look,
        skeleton,
        pace,
        quickstep_push,
        transform,
        mismatch_logged,
    ) in &mut controllers
    {
        // Ahoy's crouch flag supplies the short collider used by downed and
        // posture-transition states. Their configured speeds are already the
        // final physical targets, so the collider must not scale speed again.
        controller.crouch_speed_scale = 1.0;
        let maneuver_jump_height = skeleton.and_then(|skeleton| {
            match skeleton
                .posture_transition()
                .map(|transition| transition.kind())
            {
                Some(crate::animation::PostureTransitionKind::DiveToDowned {
                    trajectory: crate::animation::DiveTrajectory::Airborne,
                    ..
                }) => Some(movement_config.jump_heights_metres.dive),
                Some(crate::animation::PostureTransitionKind::DiveToDowned {
                    trajectory: crate::animation::DiveTrajectory::GroundedSlide,
                    ..
                }) => Some(0.0),
                _ => None,
            }
        });
        let motor = &movement_config.motor;
        controller.acceleration_hz = 0.0;
        controller.air_acceleration_hz = 0.0;
        controller.water_acceleration_hz = 0.0;
        controller.friction_hz = 0.0;
        controller.stop_speed = 0.0;
        controller.gravity = motor.gravity_metres_per_second_squared;
        controller.step_size = motor.maximum_step_height_metres;
        controller.min_walk_cos = motor.maximum_walkable_slope_degrees.to_radians().cos();
        let guard = skeleton.map_or(WeaponGuardState::Lowered, SkeletonState::weapon_guard);
        let roll_motion = skeleton.map_or(0.0, SkeletonState::downed_lateral_motion);
        if roll_motion.abs() > f32::EPSILON {
            controller.speed = speeds.roll * roll_motion.abs();
        } else if skeleton.is_some_and(SkeletonState::is_posture_transitioning) {
            controller.speed = 0.0;
        } else if skeleton.is_some_and(|skeleton| {
            skeleton.action_kind() == crate::animation::SkeletonAction::Dodge
                && skeleton.action_direction() != Vec2::ZERO
        }) {
            controller.speed = speeds.quickstep;
        } else {
            let body = skeleton.map(SkeletonState::body);
            let input_magnitude = input
                .last_movement
                .map_or(0.0, |movement| movement.length().clamp(0.0, 1.0));
            let (jog, sprint) = viewer.get(entity).map_or((3.75, speeds.run), |player| {
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
            let requested = pace.map_or(speeds.run, |pace| match pace {
                MovementPace::Walk => speeds.walk,
                MovementPace::Jog => jog,
                MovementPace::Sprint => sprint,
            });
            let cap = if guard == WeaponGuardState::Raised {
                match pace {
                    Some(MovementPace::Sprint) => jog,
                    _ => requested.min(speeds.raised_guard),
                }
            } else {
                requested
            };
            controller.speed = cap * input_magnitude;
            if matches!(
                body,
                Some(crate::animation::BodyState::Prone | crate::animation::BodyState::Supine)
            ) {
                controller.speed = pace.map_or(speeds.prone_walk, |pace| match pace {
                    MovementPace::Walk => speeds.prone_walk,
                    MovementPace::Jog => jog / movement_config.prone_effort_scale,
                    MovementPace::Sprint => speeds.prone,
                }) * input_magnitude;
            }
        }

        let (mass_kg, biological_mass_kg, leg_strength, leg_agility) = viewer.get(entity).map_or(
            (
                motor.fallback_character_mass_kg,
                motor.fallback_character_mass_kg,
                motor.reference_leg_strength,
                motor.reference_leg_agility,
            ),
            |player| {
                let burden = player.body_weight() + player.inventory_weight();
                let left_health = player.body_part_health(BodyPart::LeftLeg).max(0.0);
                let right_health = player.body_part_health(BodyPart::RightLeg).max(0.0);
                (
                    burden.max(1.0),
                    player.body_weight().max(1.0),
                    (player.raw_limb_attr(LimbAttribute::Strength, BodyPart::LeftLeg)
                        * left_health
                        + player.raw_limb_attr(LimbAttribute::Strength, BodyPart::RightLeg)
                            * right_health)
                        * 0.5,
                    (player.raw_limb_attr(LimbAttribute::Agility, BodyPart::LeftLeg) * left_health
                        + player.raw_limb_attr(LimbAttribute::Agility, BodyPart::RightLeg)
                            * right_health)
                        * 0.5,
                )
            },
        );
        if let Some(mut mass) = mass_component {
            mass.0 = mass_kg;
        } else {
            commands.entity(entity).insert(Mass(mass_kg));
        }
        controller.jump_height = maneuver_jump_height.unwrap_or_else(|| {
            ordinary_jump_height(
                movement_config.jump_heights_metres.ordinary,
                mass_kg,
                leg_strength,
                leg_agility,
                motor,
            )
        });

        let orientation = look.map_or(controller_state.orientation, CharacterLook::to_quat);

        let quickstep_action = skeleton.is_some_and(|skeleton| {
            skeleton.action_kind() == crate::animation::SkeletonAction::Dodge
                && skeleton.action_direction() != Vec2::ZERO
        });
        let quickstep_push_present = quickstep_push.is_some();
        let quickstep_push_active = quickstep_push.as_ref().is_some_and(|push| push.active);
        if let Some(mut push) = quickstep_push {
            let world_direction = (push.orientation
                * Vec3::new(push.direction.x, 0.0, -push.direction.y))
            .xz()
            .normalize_or_zero();
            let elapsed_ticks = skeleton
                .map_or(push.start_tick, |skeleton| skeleton.locomotion_sample_tick)
                .saturating_sub(push.start_tick);
            let action_duration = movement_config.maneuvers.quickstep_duration_seconds;
            let action_ticks = (action_duration * LOCOMOTION_SAMPLE_HZ).round().max(1.0) as u64;
            let forward_velocity = velocity.xz().dot(world_direction);
            let bounded_recovery = !quickstep_action
                && elapsed_ticks <= action_ticks + 2
                && forward_velocity.abs() > 0.02;
            if push.active && (quickstep_action || bounded_recovery) {
                let velocity_before = velocity.0;
                if mismatch_logged {
                    commands.entity(entity).remove::<QuickstepMismatchLogged>();
                }
                let displacement = (transform.translation - push.origin)
                    .xz()
                    .dot(world_direction)
                    .max(0.0);
                // Target the state produced by this integration step. Sampling
                // the current tick leaves one full force step of momentum for
                // ordinary locomotion to clean up after the dodge has ended.
                let phase = (elapsed_ticks + 1) as f32 / action_ticks as f32;
                let leg_length = dimensions
                    .get(entity)
                    .map_or(motor.reference_quickstep_leg_length_metres, |dimensions| {
                        dimensions.leg_length_metres
                    });
                let target_displacement = quickstep_target_displacement_metres(leg_length, motor);
                let target = quickstep_motion_target(
                    phase,
                    target_displacement,
                    action_duration,
                    motor.quickstep_authored_displacement_profile,
                );
                let maximum_force = quickstep_peak_horizontal_force_newtons(
                    biological_mass_kg,
                    leg_strength,
                    motor,
                );
                let tracking_force = quickstep_tracking_force_newtons(
                    displacement,
                    forward_velocity,
                    target,
                    mass_kg,
                    maximum_force,
                    time.delta_secs(),
                );
                let planar_delta =
                    world_direction * tracking_force / mass_kg.max(1.0) * time.delta_secs();
                velocity.x += planar_delta.x;
                velocity.z += planar_delta.y;
                if !quickstep_action && velocity.xz().dot(world_direction).abs() <= 0.02 {
                    push.cancel();
                }

                let support_duration = quickstep_push_seconds(leg_agility, motor);
                let support_ticks =
                    (support_duration * LOCOMOTION_SAMPLE_HZ).ceil().max(1.0) as u64;
                let length_scale = target_displacement
                    / motor
                        .reference_quickstep_target_displacement_metres
                        .max(f32::EPSILON);
                if elapsed_ticks < support_ticks
                    && displacement
                        < motor.quickstep_maximum_supported_root_displacement_metres * length_scale
                {
                    // While the virtual legs remain in contact, their baseline
                    // normal force supports body weight. Ahoy therefore must
                    // not apply airborne gravity yet; only the excess vertical
                    // ground-reaction force changes vertical velocity.
                    controller.gravity = 0.0;
                    let support_phase = (elapsed_ticks as f32 + 0.5) / support_ticks as f32;
                    let force_scale = quickstep_force_curve(support_phase);
                    let horizontal_force = maximum_force * force_scale;
                    let vertical_net_force =
                        horizontal_force * motor.quickstep_takeoff_angle_degrees.to_radians().tan();
                    velocity.y += vertical_net_force / mass_kg * time.delta_secs();
                }
                info!(
                    target: "quickstep_trace",
                    ?entity,
                    skeleton_tick = skeleton.map_or(0, |skeleton| skeleton.locomotion_sample_tick),
                    push_start_tick = push.start_tick,
                    elapsed_ticks,
                    ?world_direction,
                    phase,
                    displacement,
                    forward_velocity,
                    target_displacement = target.displacement_metres,
                    target_velocity = target.velocity_metres_per_second,
                    target_acceleration = target.acceleration_metres_per_second_squared,
                    tracking_force,
                    maximum_force,
                    ?velocity_before,
                    velocity_after = ?velocity.0,
                    grounded = ?controller_state.grounded,
                    "[quickstep][motor-pre-collision] integrated force"
                );
                continue;
            }
            if push.active && !quickstep_action {
                push.cancel();
            }
        }
        if quickstep_action {
            if !mismatch_logged {
                warn!(
                    target: "quickstep_trace",
                    ?entity,
                    skeleton_tick = skeleton.map_or(0, |skeleton| skeleton.locomotion_sample_tick),
                    action_direction = ?skeleton.map_or(Vec2::ZERO, SkeletonState::action_direction),
                    push_present = quickstep_push_present,
                    push_active = quickstep_push_active,
                    translation = ?transform.translation,
                    linear_velocity = ?velocity.0,
                    "[quickstep][motor] dodge animation has no active force actuator"
                );
                commands.entity(entity).insert(QuickstepMismatchLogged);
            }
            // A missing actuator can occur only during partial state restore.
            // Stop through the same bounded leg force instead of snapping the
            // velocity so rollback recovery remains physically continuous.
            let maximum_force =
                quickstep_peak_horizontal_force_newtons(biological_mass_kg, leg_strength, motor);
            let requested_force = -velocity.xz() * mass_kg.max(1.0) / time.delta_secs();
            let force = requested_force.clamp_length_max(maximum_force);
            velocity.x += force.x / mass_kg.max(1.0) * time.delta_secs();
            velocity.z += force.y / mass_kg.max(1.0) * time.delta_secs();
            continue;
        }
        if mismatch_logged {
            commands.entity(entity).remove::<QuickstepMismatchLogged>();
        }
        if let Some(transition) = skeleton.and_then(SkeletonState::posture_transition)
            && matches!(
                transition.kind(),
                crate::animation::PostureTransitionKind::DiveToDowned {
                    trajectory: crate::animation::DiveTrajectory::GroundedSlide,
                    ..
                }
            )
        {
            // The authored first half lowers the body while retaining exact
            // sprint momentum. After body contact, integrate bounded kinetic
            // drag; do not hand the slide to the ordinary target-speed motor.
            if transition.phase() >= 0.5 && controller_state.grounded.is_some() {
                let next = slide_drag_velocity(velocity.xz(), motor, time.delta_secs());
                velocity.x = next.x;
                velocity.z = next.y;
            }
            continue;
        }
        let movement = input.last_movement.unwrap_or_default();
        let forward = (orientation * Vec3::NEG_Z).with_y(0.0).normalize_or_zero();
        let right = (orientation * Vec3::X).with_y(0.0).normalize_or_zero();
        let desired_direction = (movement.y * forward + movement.x * right).normalize_or_zero();
        let grounded = controller_state.grounded.is_some();
        let strength_scale = (leg_strength.max(0.0) / motor.reference_leg_strength).max(0.0);
        let agility_scale = (leg_agility.max(0.0) / motor.reference_leg_agility).max(0.0);
        let (drive_force, braking_force) =
            character_motor_force_limits(motor, mass_kg, strength_scale);
        let horizontal = velocity.xz();
        let target_speed = controller.speed;
        let target = desired_direction.xz() * target_speed;
        let next = if grounded {
            let traction_acceleration =
                motor.gravity_metres_per_second_squared * motor.traction_coefficient;
            let sustained_lateral_acceleration = (motor.gravity_metres_per_second_squared
                * motor.reference_lateral_acceleration_gravities
                * agility_scale)
                .min(traction_acceleration);
            let lateral_acceleration = cut_lateral_acceleration(
                horizontal,
                desired_direction.xz(),
                sustained_lateral_acceleration,
                traction_acceleration,
                leg_agility,
            );
            let reference_turn_radius = agility_sprint_turn_radius(leg_agility, motor);
            let turn_radius = ordinary_turn_radius(horizontal.length(), reference_turn_radius);
            approach_ground_velocity(
                horizontal,
                target,
                drive_force / mass_kg,
                braking_force / mass_kg,
                lateral_acceleration,
                traction_acceleration,
                turn_radius,
                time.delta_secs(),
            )
        } else {
            approach_velocity(
                horizontal,
                target,
                drive_force / mass_kg * motor.air_control_force_scale,
                time.delta_secs(),
            )
        };
        velocity.x = next.x;
        velocity.z = next.y;
    }
}

fn slide_drag_velocity(
    velocity: Vec2,
    motor: &crate::combat_config::CharacterMotorConfig,
    delta_seconds: f32,
) -> Vec2 {
    approach_velocity(
        velocity,
        Vec2::ZERO,
        motor.gravity_metres_per_second_squared * motor.slide_drag_coefficient,
        delta_seconds,
    )
}

fn character_motor_force_limits(
    motor: &crate::combat_config::CharacterMotorConfig,
    mass_kg: f32,
    strength_scale: f32,
) -> (f32, f32) {
    let traction_force =
        mass_kg.max(1.0) * motor.gravity_metres_per_second_squared * motor.traction_coefficient;
    (
        (motor.reference_ground_drive_force_newtons * strength_scale.max(0.0)).min(traction_force),
        (motor.reference_ground_braking_force_newtons * strength_scale.max(0.0))
            .min(traction_force),
    )
}

fn ordinary_jump_height(
    reference_height: f32,
    mass_kg: f32,
    leg_strength: f32,
    leg_agility: f32,
    motor: &crate::combat_config::CharacterMotorConfig,
) -> f32 {
    let athleticism = ((leg_strength / motor.reference_leg_strength
        + leg_agility / motor.reference_leg_agility)
        * 0.5)
        .max(0.0);
    // Attribute 3 is the reference healthy adult. Attribute 4 produces about
    // a 0.39 m unburdened rise and attribute 5 about 0.49 m; burden reduces
    // takeoff energy per unit mass without changing Earth-normal gravity.
    let takeoff_velocity_scale = 0.58 + 0.42 * athleticism;
    let burden_scale = (REFERENCE_BURDEN_KG / mass_kg.max(1.0)).clamp(0.25, 1.25);
    reference_height * takeoff_velocity_scale * takeoff_velocity_scale * burden_scale
}

/// Converts the configured full-sprint arc into a speed-dependent ordinary
/// turn. Below the reference sprint, constant centripetal acceleration gives
/// `r = v^2 / a`, so running and jogging turn progressively more tightly while
/// retaining their speed. The small floor represents the space required to
/// redirect a standing body instead of collapsing toward a zero-radius pivot.
fn ordinary_turn_radius(speed: f32, reference_sprint_radius: f32) -> f32 {
    let speed_fraction =
        (speed.max(0.0) / REFERENCE_SPRINT_SPEED_METRES_PER_SECOND).clamp(0.0, 1.0);
    (reference_sprint_radius * speed_fraction * speed_fraction)
        .max(MINIMUM_ORDINARY_TURN_RADIUS_METRES)
}

/// Smooth bounded response through the declarative Agility 1, 3, and 5 sprint
/// radii. In normalized attribute space `x = (agility - 3) / 2`, the unique
/// quadratic through the three anchors is `reference + linear*x + curve*x^2`.
/// Validation guarantees its derivative remains negative over `-1..=1`.
fn agility_sprint_turn_radius(
    leg_agility: f32,
    motor: &crate::combat_config::CharacterMotorConfig,
) -> f32 {
    let x = (leg_agility.clamp(1.0, 5.0) - 3.0) * 0.5;
    let low = motor.agility_one_sprint_turn_radius_metres;
    let reference = motor.reference_sprint_turn_radius_metres;
    let high = motor.agility_five_sprint_turn_radius_metres;
    let linear = (high - low) * 0.5;
    let curve = (high + low) * 0.5 - reference;
    reference + linear * x + curve * x * x
}

fn cut_lateral_acceleration(
    current: Vec2,
    desired_direction: Vec2,
    sustained_acceleration: f32,
    traction_acceleration: f32,
    leg_agility: f32,
) -> f32 {
    if current.length_squared() <= 1.0e-6 || desired_direction == Vec2::ZERO {
        return sustained_acceleration;
    }
    let cosine = current.normalize().dot(desired_direction).clamp(-1.0, 1.0);
    let lateral_demand = (1.0 - cosine * cosine).sqrt();
    // A hard plant can briefly approach the surface traction ceiling. Average
    // agility exploits part of that reserve; John (Agility 4) can use it all.
    let plant_skill = (leg_agility / 4.0).clamp(0.0, 1.0);
    sustained_acceleration.lerp(traction_acceleration, lateral_demand * plant_skill)
}

fn approach_ground_velocity(
    current: Vec2,
    target: Vec2,
    drive_acceleration: f32,
    braking_acceleration: f32,
    lateral_acceleration: f32,
    traction_acceleration: f32,
    turn_radius: f32,
    delta_seconds: f32,
) -> Vec2 {
    if current == target || delta_seconds <= 0.0 {
        return current;
    }
    let current_speed = current.length();
    let target_speed = target.length();
    if target_speed <= 1.0e-6 {
        return approach_velocity(
            current,
            Vec2::ZERO,
            braking_acceleration.min(traction_acceleration),
            delta_seconds,
        );
    }
    if current_speed <= 1.0e-6 {
        return approach_velocity(
            Vec2::ZERO,
            target,
            drive_acceleration.min(traction_acceleration),
            delta_seconds,
        );
    }

    let current_direction = current / current_speed;
    let target_direction = target / target_speed;
    let cosine = current_direction.dot(target_direction).clamp(-1.0, 1.0);
    // An exact reversal has no physically meaningful turn side. Brake to zero
    // before accelerating backward instead of choosing an arbitrary U-turn.
    if cosine < -0.999 {
        return approach_velocity(
            current,
            Vec2::ZERO,
            braking_acceleration.min(traction_acceleration),
            delta_seconds,
        );
    }

    let cross = current_direction.perp_dot(target_direction);
    let angle = cross.atan2(cosine);
    let radius_acceleration = current_speed * current_speed / turn_radius.max(0.01);
    let steering_acceleration = lateral_acceleration.max(radius_acceleration);
    let maximum_turn = steering_acceleration * delta_seconds / current_speed;
    let turn = angle.clamp(-maximum_turn, maximum_turn);
    let (sin, cos) = turn.sin_cos();
    let turned_direction = Vec2::new(
        current_direction.x * cos - current_direction.y * sin,
        current_direction.x * sin + current_direction.y * cos,
    );
    let speed_acceleration = if target_speed >= current_speed {
        drive_acceleration
    } else {
        braking_acceleration
    }
    .min(traction_acceleration)
    .max(0.0);
    let next_speed = current_speed
        + (target_speed - current_speed).clamp(
            -speed_acceleration * delta_seconds,
            speed_acceleration * delta_seconds,
        );
    turned_direction * next_speed
}

fn approach_velocity(current: Vec2, target: Vec2, acceleration: f32, delta_seconds: f32) -> Vec2 {
    let maximum_delta = (acceleration.max(0.0) * delta_seconds.max(0.0)).max(0.0);
    current + (target - current).clamp_length_max(maximum_delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_drag_is_bounded_and_preserves_heading() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        let incoming = Vec2::new(3.0, 4.0);
        let next = slide_drag_velocity(incoming, motor, 0.25);
        let expected_speed =
            5.0 - motor.gravity_metres_per_second_squared * motor.slide_drag_coefficient * 0.25;

        assert!((next.length() - expected_speed).abs() < 1.0e-5);
        assert!(next.normalize().abs_diff_eq(incoming.normalize(), 1.0e-6));
        assert_eq!(slide_drag_velocity(incoming, motor, 0.0), incoming);
        assert_eq!(slide_drag_velocity(Vec2::ZERO, motor, 1.0), Vec2::ZERO);
    }

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
        world.insert_resource(crate::combat_config::TacticalCombatConfig::default());
        let mut fixed_time = Time::<Fixed>::from_hz(64.0);
        fixed_time.advance_by(std::time::Duration::from_secs_f64(1.0 / 64.0));
        world.insert_resource(fixed_time);
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_character_motor);
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
            0.0
        );
        assert_eq!(
            world.get::<CharacterController>(generic).unwrap().speed,
            TACTICAL_RUN_SPEED_METRES_PER_SECOND
        );
        let controller = world.get::<CharacterController>(generic).unwrap();
        assert_eq!(controller.acceleration_hz, 0.0);
        assert_eq!(controller.friction_hz, 0.0);
        assert_eq!(controller.gravity, 9.81);
        assert_eq!(controller.step_size, 0.35);
        assert_eq!(world.get::<Mass>(generic), Some(&Mass(80.0)));
    }

    #[test]
    fn quickstep_support_uses_force_without_an_ahoy_jump_event() {
        let mut skeleton = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        skeleton
            .begin_dodge(
                crate::animation::DodgeSpec::quickstep(Vec2::Y).unwrap(),
                0,
                20,
            )
            .unwrap();
        let mut push = QuickstepPush::default();
        push.begin(0, Vec2::Y, Quat::IDENTITY, Vec3::ZERO);
        let mut world = World::new();
        let entity = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                skeleton,
                push,
            ))
            .id();
        world.insert_resource(crate::combat_config::TacticalCombatConfig::default());
        let mut fixed_time = Time::<Fixed>::from_hz(64.0);
        fixed_time.advance_by(std::time::Duration::from_secs_f64(1.0 / 64.0));
        world.insert_resource(fixed_time);
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_character_motor);
        schedule.run(&mut world);

        let controller = world.get::<CharacterController>(entity).unwrap();
        assert_eq!(controller.gravity, 0.0);
        assert_eq!(controller.speed, TACTICAL_QUICKSTEP_SPEED_METRES_PER_SECOND);
        assert!(
            world
                .get::<AccumulatedInput>(entity)
                .unwrap()
                .jumped
                .is_none()
        );
        let velocity = world.get::<LinearVelocity>(entity).unwrap();
        assert!(velocity.z < 0.0);
        assert!(velocity.y > 0.0);
    }

    #[test]
    fn motor_approaches_target_without_overshoot() {
        assert_eq!(
            approach_velocity(Vec2::ZERO, Vec2::X * 5.0, 8.0, 0.25),
            Vec2::X * 2.0
        );
        assert_eq!(
            approach_velocity(Vec2::X * 4.0, Vec2::ZERO, 8.0, 0.25),
            Vec2::X * 2.0
        );
        assert_eq!(approach_velocity(Vec2::ZERO, Vec2::X, 8.0, 0.25), Vec2::X);
    }

    fn integrated_right_angle_turn(speed: f32, agility: f32) -> (f32, f32, f32) {
        let grounded = avian3d::character_controller::move_and_slide::MoveHitData {
            entity: Entity::PLACEHOLDER,
            distance: 0.0,
            point1: Vec3::ZERO,
            point2: Vec3::ZERO,
            normal1: Vec3::Y,
            normal2: Vec3::NEG_Y,
            collision_distance: 0.0,
        };
        let mut config = crate::combat_config::TacticalCombatConfig::default();
        config.movement.speeds_metres_per_second.run = speed;
        let motor = &config.movement.motor;
        let turn_radius = ordinary_turn_radius(speed, agility_sprint_turn_radius(agility, motor));
        let mut attributes = crate::player::Attributes::default();
        attributes.left_leg_agility = agility;
        attributes.right_leg_agility = agility;
        let mut world = World::new();
        let entity = world
            .spawn((
                AccumulatedInput {
                    // Identity controller orientation maps backward local input
                    // to world +Z, ninety degrees left of the initial +X run.
                    last_movement: Some(-Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                CharacterControllerState {
                    grounded: Some(grounded),
                    ..default()
                },
                LinearVelocity(Vec3::X * speed),
                Mass(80.0),
                Transform::default(),
                crate::player::Limbs::default(),
                crate::player::Skills::default(),
                crate::player::Stats::default(),
                attributes,
            ))
            .id();
        let delta_seconds = 1.0 / LOCOMOTION_SAMPLE_HZ;
        world.insert_resource(config);
        let mut fixed_time = Time::<Fixed>::from_hz(LOCOMOTION_SAMPLE_HZ as f64);
        fixed_time.advance_by(std::time::Duration::from_secs_f32(delta_seconds));
        world.insert_resource(fixed_time);
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_character_motor);

        let centre = Vec2::Y * turn_radius;
        let mut maximum_radius_error = 0.0_f32;
        let mut minimum_speed = speed;

        for _ in 0..128 {
            schedule.run(&mut world);
            let velocity = world.get::<LinearVelocity>(entity).unwrap().0;
            world.get_mut::<Transform>(entity).unwrap().translation += velocity * delta_seconds;
            let position = world.get::<Transform>(entity).unwrap().translation.xz();
            let horizontal_velocity = velocity.xz();
            minimum_speed = minimum_speed.min(horizontal_velocity.length());
            maximum_radius_error =
                maximum_radius_error.max(((position - centre).length() - turn_radius).abs());
            if horizontal_velocity.normalize().dot(Vec2::Y) > 0.99999 {
                break;
            }
        }

        let position = world.get::<Transform>(entity).unwrap().translation.xz();
        (
            position.length() / std::f32::consts::SQRT_2,
            minimum_speed,
            maximum_radius_error,
        )
    }

    #[test]
    fn ordinary_turns_combine_bounded_agility_with_nonlinear_speed_scaling() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        assert_eq!(agility_sprint_turn_radius(1.0, motor), 4.5);
        assert_eq!(agility_sprint_turn_radius(3.0, motor), 3.0);
        assert!((agility_sprint_turn_radius(5.0, motor) - 2.2).abs() < 1.0e-6);
        for agility_tenths in 10..50 {
            let agility = agility_tenths as f32 * 0.1;
            assert!(
                agility_sprint_turn_radius(agility + 0.1, motor)
                    < agility_sprint_turn_radius(agility, motor)
            );
        }

        let speeds = [
            REFERENCE_SPRINT_SPEED_METRES_PER_SECOND,
            TACTICAL_RUN_SPEED_METRES_PER_SECOND,
            3.75,
        ];
        let agilities = [1.0, 3.0, 5.0];
        let mut measured_radii = [[0.0_f32; 3]; 3];
        for (agility_index, agility) in agilities.into_iter().enumerate() {
            let sprint_radius = agility_sprint_turn_radius(agility, motor);
            for (speed_index, speed) in speeds.into_iter().enumerate() {
                let expected_radius = ordinary_turn_radius(speed, sprint_radius);
                let (measured_radius, minimum_speed, maximum_radius_error) =
                    integrated_right_angle_turn(speed, agility);
                assert!(
                    (minimum_speed - speed).abs() < 1.0e-4,
                    "Agility {agility:.0} at {speed:.2} m/s fell to {minimum_speed:.3} m/s"
                );
                assert!(
                    (measured_radius - expected_radius).abs() < 0.06,
                    "Agility {agility:.0} at {speed:.2} m/s measured {measured_radius:.3} m instead of {expected_radius:.3} m"
                );
                assert!(
                    maximum_radius_error < speed / LOCOMOTION_SAMPLE_HZ + 0.01,
                    "Agility {agility:.0} at {speed:.2} m/s radius error was {maximum_radius_error:.3} m"
                );
                measured_radii[agility_index][speed_index] = measured_radius;
            }
        }

        for radii_at_agility in &measured_radii {
            assert!(radii_at_agility[0] > radii_at_agility[1]);
            assert!(radii_at_agility[1] > radii_at_agility[2]);
        }
        for ((average, john), olympian) in measured_radii[0]
            .iter()
            .zip(&measured_radii[1])
            .zip(&measured_radii[2])
        {
            assert!(average > john);
            assert!(john > olympian);
        }
    }

    #[test]
    fn stop_and_exact_reversal_still_brake_before_accelerating_backward() {
        let speed = REFERENCE_SPRINT_SPEED_METRES_PER_SECOND;
        let delta_seconds = 1.0 / LOCOMOTION_SAMPLE_HZ;
        let braking_acceleration = 9.0;
        let mut stopped = Vec2::X * speed;
        for _ in 0..128 {
            stopped = approach_ground_velocity(
                stopped,
                Vec2::ZERO,
                9.0,
                braking_acceleration,
                6.0,
                9.0,
                3.0,
                delta_seconds,
            );
        }
        assert_eq!(stopped, Vec2::ZERO);

        let first_reversal_tick = approach_ground_velocity(
            Vec2::X * speed,
            Vec2::Y,
            9.0,
            braking_acceleration,
            6.0,
            9.0,
            3.0,
            delta_seconds,
        );
        assert!(
            first_reversal_tick.y > 0.0,
            "a right-angle input should steer"
        );

        let exact_reversal_tick = approach_ground_velocity(
            Vec2::X * speed,
            Vec2::NEG_X * speed,
            9.0,
            braking_acceleration,
            6.0,
            9.0,
            3.0,
            delta_seconds,
        );
        assert!(exact_reversal_tick.x > 0.0 && exact_reversal_tick.x < speed);
        assert_eq!(exact_reversal_tick.y, 0.0);
    }

    #[test]
    fn ordinary_jump_height_maps_attributes_and_burden_to_ballistic_rise() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        let reference = config.movement.jump_heights_metres.ordinary;
        let average = ordinary_jump_height(reference, 70.0, 3.0, 3.0, motor);
        let john_unburdened = ordinary_jump_height(reference, 70.0, 4.0, 4.0, motor);
        let olympian_unburdened = ordinary_jump_height(reference, 70.0, 5.0, 5.0, motor);
        let john_burdened = ordinary_jump_height(reference, 90.0, 4.0, 4.0, motor);

        assert!((average - 0.30).abs() < 1.0e-6);
        assert!((john_unburdened - 0.39).abs() < 0.005);
        assert!((olympian_unburdened - 0.49).abs() < 0.005);
        assert!(john_burdened < john_unburdened);
    }

    #[test]
    fn quickstep_uses_a_finite_strength_and_mass_aware_force_curve() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        assert_eq!(quickstep_force_curve(0.0), 0.0);
        assert_eq!(quickstep_force_curve(0.5), 1.0);
        assert_eq!(quickstep_force_curve(1.0), 0.0);

        let reference_force = quickstep_peak_horizontal_force_newtons(70.0, 3.0, motor);
        let john_force = quickstep_peak_horizontal_force_newtons(70.0, 4.0, motor);
        let olympian_force = quickstep_peak_horizontal_force_newtons(70.0, 5.0, motor);
        assert!((reference_force / (70.0 * 9.81) - 4.5).abs() < 1.0e-5);
        assert!((john_force / (70.0 * 9.81) - 5.1).abs() < 1.0e-5);
        assert!((olympian_force / (70.0 * 9.81) - 5.7).abs() < 1.0e-5);
        assert!((quickstep_push_seconds(3.0, motor) - 0.40).abs() < 1.0e-6);
        assert!((quickstep_push_seconds(4.0, motor) - 0.38).abs() < 1.0e-6);
        assert!((quickstep_push_seconds(5.0, motor) - 0.36).abs() < 1.0e-6);

        let duration = quickstep_push_seconds(4.0, motor);
        let ticks = (duration * LOCOMOTION_SAMPLE_HZ).ceil() as u64;
        let force_time = (0..ticks)
            .map(|tick| {
                quickstep_force_curve((tick as f32 + 0.5) / ticks as f32) / LOCOMOTION_SAMPLE_HZ
            })
            .sum::<f32>();
        let unburdened_speed = john_force * force_time / 70.0;
        let burdened_speed = john_force * force_time / 93.9;
        assert!((unburdened_speed - 13.48).abs() < 0.05);
        assert!((burdened_speed - 10.05).abs() < 0.05);
        assert!(burdened_speed < unburdened_speed);
    }

    #[test]
    fn quickstep_force_follows_the_authored_profile_and_stops_at_scaled_distance() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        let target_distance = quickstep_target_displacement_metres(
            crate::player::CharacterDimensions::default().leg_length_metres,
            motor,
        );
        let duration = config.movement.maneuvers.quickstep_duration_seconds;
        let ticks = (duration * LOCOMOTION_SAMPLE_HZ).round() as usize;
        let delta_seconds = 1.0 / LOCOMOTION_SAMPLE_HZ;
        let maximum_force = quickstep_peak_horizontal_force_newtons(70.0, 3.0, motor);
        let mut displacement = 0.0;
        let mut velocity = 0.0;
        let mut keyed_displacements = Vec::new();
        for tick in 0..ticks {
            if tick % (ticks / 4) == 0 {
                keyed_displacements.push(displacement);
            }
            let target = quickstep_motion_target(
                (tick + 1) as f32 / ticks as f32,
                target_distance,
                duration,
                motor.quickstep_authored_displacement_profile,
            );
            let force = quickstep_tracking_force_newtons(
                displacement,
                velocity,
                target,
                70.0,
                maximum_force,
                delta_seconds,
            );
            velocity += force / 70.0 * delta_seconds;
            displacement += velocity * delta_seconds;
        }
        keyed_displacements.push(displacement);

        for (actual, normalized) in keyed_displacements
            .iter()
            .zip(motor.quickstep_authored_displacement_profile)
        {
            let expected = normalized * target_distance;
            assert!(
                (actual - expected).abs() <= 0.05,
                "actual={actual} expected={expected}"
            );
        }
        assert!(velocity.abs() <= 0.20, "terminal velocity={velocity}");
    }

    #[test]
    fn quickstep_launch_overcomes_adverse_ground_contact_drift() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        let duration = config.movement.maneuvers.quickstep_duration_seconds;
        let ticks = (duration * LOCOMOTION_SAMPLE_HZ).round() as usize;
        let delta_seconds = 1.0 / LOCOMOTION_SAMPLE_HZ;
        let mass_kg = 70.0;
        let maximum_force = quickstep_peak_horizontal_force_newtons(mass_kg, 3.0, motor);
        let target = quickstep_motion_target(
            1.0 / ticks as f32,
            quickstep_target_displacement_metres(
                crate::player::CharacterDimensions::default().leg_length_metres,
                motor,
            ),
            duration,
            motor.quickstep_authored_displacement_profile,
        );
        // This is larger than the adverse residuals observed after Ahoy ground
        // projection, while still being physically indistinguishable from rest.
        let adverse_velocity = -0.0001;
        let force = quickstep_tracking_force_newtons(
            0.0,
            adverse_velocity,
            target,
            mass_kg,
            maximum_force,
            delta_seconds,
        );
        let velocity_after_force = adverse_velocity + force / mass_kg * delta_seconds;

        assert!(
            force >= maximum_force * 0.99,
            "launch force {force} was suppressed below maximum {maximum_force}"
        );
        assert!(
            velocity_after_force > 0.5,
            "launch did not overcome adverse drift: {velocity_after_force}"
        );
    }

    #[test]
    fn quickstep_distance_scales_with_leg_length() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        let reference = motor.reference_quickstep_leg_length_metres;
        assert!((quickstep_target_displacement_metres(reference, motor) - 1.0).abs() < 1.0e-6);
        assert!(
            (quickstep_target_displacement_metres(reference * 1.2, motor) - 1.2).abs() < 1.0e-6
        );
        assert!(
            (quickstep_target_displacement_metres(reference * 0.8, motor) - 0.8).abs() < 1.0e-6
        );
    }

    #[test]
    fn motor_force_respects_mass_injury_and_traction() {
        let config = crate::combat_config::TacticalCombatConfig::default();
        let motor = &config.movement.motor;
        let (reference_drive, reference_brake) = character_motor_force_limits(motor, 80.0, 1.0);
        let traction = 80.0 * motor.gravity_metres_per_second_squared * motor.traction_coefficient;
        assert_eq!(reference_drive, traction);
        assert_eq!(reference_brake, traction);

        let (burdened_drive, _) = character_motor_force_limits(motor, 160.0, 1.0);
        assert_eq!(burdened_drive, motor.reference_ground_drive_force_newtons);
        assert!(burdened_drive / 160.0 < reference_drive / 80.0);

        let (injured_drive, _) = character_motor_force_limits(motor, 80.0, 0.5);
        assert_eq!(
            injured_drive,
            motor.reference_ground_drive_force_newtons * 0.5
        );
        assert!(injured_drive < reference_drive);
    }

    #[test]
    fn grounded_motor_accelerates_and_brakes_the_canonical_velocity() {
        let mut world = World::new();
        let grounded = avian3d::character_controller::move_and_slide::MoveHitData {
            entity: Entity::PLACEHOLDER,
            distance: 0.0,
            point1: Vec3::ZERO,
            point2: Vec3::ZERO,
            normal1: Vec3::Y,
            normal2: Vec3::NEG_Y,
            collision_distance: 0.0,
        };
        let entity = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                CharacterControllerState {
                    grounded: Some(grounded),
                    ..default()
                },
                LinearVelocity::default(),
                Mass(80.0),
            ))
            .id();
        world.insert_resource(crate::combat_config::TacticalCombatConfig::default());
        let mut fixed_time = Time::<Fixed>::from_hz(64.0);
        fixed_time.advance_by(std::time::Duration::from_secs_f64(1.0 / 64.0));
        world.insert_resource(fixed_time);
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_character_motor);

        schedule.run(&mut world);
        let first_speed = world.get::<LinearVelocity>(entity).unwrap().xz().length();
        assert!((0.13..0.15).contains(&first_speed));
        for _ in 0..63 {
            schedule.run(&mut world);
        }
        assert!(
            world.get::<LinearVelocity>(entity).unwrap().xz().length()
                <= TACTICAL_RUN_SPEED_METRES_PER_SECOND
        );

        world
            .get_mut::<AccumulatedInput>(entity)
            .unwrap()
            .last_movement = None;
        for _ in 0..64 {
            schedule.run(&mut world);
        }
        assert_eq!(
            world.get::<LinearVelocity>(entity).unwrap().xz(),
            Vec2::ZERO
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
                crate::animation::BodyState::default(),
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
                crate::animation::BodyState::default(),
                3.0,
                8.0,
            ) <= 0.0
        );
        assert!(
            tactical_movement_exhaustion_change_per_second(
                Some(Vec2::Y),
                MovementPace::Sprint,
                WeaponGuardState::Lowered,
                crate::animation::BodyState::default(),
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
    fn prone_walk_is_fixed_jog_is_neutral_and_sprint_costs_exhaustion() {
        let body = crate::animation::BodyState::Prone;
        for endurance in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0] {
            assert_eq!(
                tactical_prone_movement_speed_for_pace(
                    Some(Vec2::Y),
                    MovementPace::Walk,
                    tactical_jog_speed(endurance),
                ),
                TACTICAL_PRONE_WALK_SPEED_METRES_PER_SECOND
            );
            assert!(
                (tactical_prone_movement_speed_for_pace(
                    Some(Vec2::Y),
                    MovementPace::Jog,
                    tactical_jog_speed(endurance),
                ) - tactical_jog_speed(endurance) / TACTICAL_PRONE_EFFORT_SCALE)
                    .abs()
                    < f32::EPSILON
            );
            assert_eq!(
                tactical_prone_movement_speed_for_pace(
                    Some(Vec2::Y),
                    MovementPace::Sprint,
                    tactical_jog_speed(endurance),
                ),
                TACTICAL_PRONE_SPEED_METRES_PER_SECOND
            );
            assert_eq!(
                tactical_prone_movement_speed_for_pace(
                    Some(Vec2::X),
                    MovementPace::Sprint,
                    tactical_jog_speed(endurance),
                ),
                TACTICAL_PRONE_SPEED_METRES_PER_SECOND * TACTICAL_PRONE_LATERAL_SPEED_SCALE
            );
            assert_eq!(
                tactical_movement_exhaustion_change_per_second(
                    Some(Vec2::Y),
                    MovementPace::Jog,
                    WeaponGuardState::Lowered,
                    body,
                    endurance,
                    REFERENCE_SPRINT_SPEED_METRES_PER_SECOND,
                ),
                0.0
            );
            assert!(
                tactical_movement_exhaustion_change_per_second(
                    Some(Vec2::Y),
                    MovementPace::Walk,
                    WeaponGuardState::Lowered,
                    body,
                    endurance,
                    REFERENCE_SPRINT_SPEED_METRES_PER_SECOND,
                ) < 0.0
            );
            assert!(
                tactical_movement_exhaustion_change_per_second(
                    Some(Vec2::Y),
                    MovementPace::Sprint,
                    WeaponGuardState::Lowered,
                    body,
                    endurance,
                    REFERENCE_SPRINT_SPEED_METRES_PER_SECOND,
                ) > 0.0
            );
        }
    }

    #[test]
    fn prone_and_supine_share_selected_pace_after_directional_scaling() {
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
        let prone_walk = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                SkeletonState::default().with_body_state(crate::animation::BodyState::Prone),
                MovementPace::Walk,
            ))
            .id();
        let prone_half_input = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y * 0.5),
                    ..default()
                },
                CharacterController::default(),
                SkeletonState::default().with_body_state(crate::animation::BodyState::Prone),
                MovementPace::Walk,
            ))
            .id();
        let prone_jog = world
            .spawn((
                AccumulatedInput {
                    last_movement: Some(Vec2::Y),
                    ..default()
                },
                CharacterController::default(),
                SkeletonState::default().with_body_state(crate::animation::BodyState::Prone),
                MovementPace::Jog,
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
        world.insert_resource(crate::combat_config::TacticalCombatConfig::default());
        world.insert_resource(Time::<Fixed>::from_hz(64.0));
        let mut schedule = Schedule::default();
        schedule.add_systems(apply_character_motor);
        schedule.run(&mut world);

        assert_eq!(
            world.get::<CharacterController>(prone).unwrap().speed,
            TACTICAL_PRONE_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            world
                .get::<CharacterController>(prone)
                .unwrap()
                .crouch_speed_scale,
            1.0,
            "downed collision crouching must not reduce the final crawl target"
        );
        assert_eq!(
            world.get::<CharacterController>(supine).unwrap().speed,
            TACTICAL_PRONE_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            world.get::<CharacterController>(prone_walk).unwrap().speed,
            TACTICAL_PRONE_WALK_SPEED_METRES_PER_SECOND
        );
        assert_eq!(
            world
                .get::<CharacterController>(prone_half_input)
                .unwrap()
                .speed,
            TACTICAL_PRONE_WALK_SPEED_METRES_PER_SECOND * 0.5
        );
        assert_eq!(
            world.get::<CharacterController>(prone_jog).unwrap().speed,
            3.75 / TACTICAL_PRONE_EFFORT_SCALE
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
