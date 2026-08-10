use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_ahoy::{
    AhoyPlugins, AhoySystems, CharacterController, camera::AhoyCameraPlugin,
    input::AccumulatedInput,
};

use crate::{
    animation::{SkeletonState, WeaponGuardState},
    player::{Attributes, MovementGait, TacticalCombatState},
};

/// Deliberate walking pace, independent of athletic conditioning.
pub const TACTICAL_WALK_SPEED_METRES_PER_SECOND: f32 = 1.4;

/// Elite marathon pace used to anchor Endurance 5 sustainable jogging.
pub const ELITE_MARATHON_SPEED_METRES_PER_SECOND: f32 = 5.83;

/// Breath gained per second for each metre per second above sustainable pace.
const BREATH_EXHAUSTION_PER_EXCESS_METRE_SECOND: f32 = 0.01;
/// Recovery is faster than accumulation for the same distance below the
/// sustainable pace, keeping short bursts useful in tactical combat.
const BREATH_RECOVERY_PER_SPARE_METRE_SECOND: f32 = 0.02;

/// Maximum server-authoritative movement speed while the weapon guard is raised.
pub const TACTICAL_GUARD_SPEED_METRES_PER_SECOND: f32 = 2.0;

/// Builds the shared tactical controller instead of inheriting Ahoy's much
/// faster general-purpose default.
pub fn tactical_character_controller() -> CharacterController {
    CharacterController {
        speed: sustainable_jog_speed(3.0),
        ..default()
    }
}

/// Sustainable locomotion pace for a given Endurance score.
///
/// Endurance 1 is a speed-walk, 2 is the ordinary walk/run transition, and 5
/// is elite marathon pace. The convex curve reserves exceptional distance
/// speed for exceptional Endurance while remaining continuous.
#[must_use]
pub fn sustainable_jog_speed(endurance: f32) -> f32 {
    let endurance = if endurance.is_finite() {
        endurance.max(0.0)
    } else {
        0.0
    };
    if endurance <= 1.0 {
        let t = endurance;
        return 1.8 * t * t * (3.0 - 2.0 * t);
    }
    let t = ((endurance.min(5.0) - 1.0) / 4.0).clamp(0.0, 1.0);
    1.8 + (ELITE_MARATHON_SPEED_METRES_PER_SECOND - 1.8) * t.powf(2.166)
}

/// Maximum sprint pace from the mean leg Strength score.
///
/// The curve is continuously differentiable at Strength 1, then maps ordinary
/// adult male Strength 3 to 8 m/s and Olympic Strength 5 to 12 m/s.
#[must_use]
pub fn tactical_sprint_speed(average_leg_strength: f32) -> f32 {
    let strength = if average_leg_strength.is_finite() {
        average_leg_strength.max(0.0)
    } else {
        0.0
    };
    if strength < 1.0 {
        -6.0 * strength.powi(3) + 10.0 * strength.powi(2)
    } else {
        2.0 + 2.0 * strength
    }
}

/// Resolves the server-authoritative controller target speed. Analogue input
/// magnitude is retained after Ahoy normalizes its wish direction.
#[must_use]
pub fn tactical_movement_speed(
    movement: Option<Vec2>,
    gait: MovementGait,
    weapon_guard: WeaponGuardState,
    endurance: f32,
    average_leg_strength: f32,
    breath_exhaustion: f32,
) -> f32 {
    let jog = sustainable_jog_speed(endurance);
    let gait_cap = match gait {
        MovementGait::Walk => TACTICAL_WALK_SPEED_METRES_PER_SECOND,
        MovementGait::Jog => jog,
        MovementGait::Sprint => {
            let sprint = tactical_sprint_speed(average_leg_strength).max(jog);
            sprint.lerp(jog, breath_exhaustion.clamp(0.0, 1.0))
        }
    };
    let cap = match weapon_guard {
        WeaponGuardState::Lowered => gait_cap,
        WeaponGuardState::Raised => gait_cap.min(TACTICAL_GUARD_SPEED_METRES_PER_SECOND),
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
            )
            .add_systems(
                FixedPostUpdate,
                update_breath_exhaustion.after(AhoySystems::MoveCharacters),
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
    mut controllers: Query<(
        &AccumulatedInput,
        &mut CharacterController,
        Option<&MovementGait>,
        Option<&Attributes>,
        Option<&TacticalCombatState>,
        Option<&SkeletonState>,
    )>,
) {
    for (input, mut controller, gait, attributes, combat, skeleton) in &mut controllers {
        let gait = gait.copied().unwrap_or_default();
        let endurance = attributes.map_or(3.0, |attributes| attributes.endurance);
        let leg_strength = attributes.map_or(3.0, |attributes| {
            (attributes.left_leg_strength + attributes.right_leg_strength) * 0.5
        });
        controller.speed = tactical_movement_speed(
            input.last_movement,
            gait,
            skeleton.map_or(WeaponGuardState::Lowered, SkeletonState::weapon_guard),
            endurance,
            leg_strength,
            combat.map_or(0.0, |combat| combat.breath_exhaustion),
        );
    }
}

fn update_breath_exhaustion(
    time: Res<Time<Fixed>>,
    mut players: Query<(&LinearVelocity, &Attributes, &mut TacticalCombatState)>,
) {
    for (velocity, attributes, mut combat) in &mut players {
        let horizontal_speed = Vec2::new(velocity.x, velocity.z).length();
        combat.breath_exhaustion = breath_exhaustion_after(
            combat.breath_exhaustion,
            horizontal_speed,
            attributes.endurance,
            time.delta_secs(),
        );
    }
}

fn breath_exhaustion_after(current: f32, speed: f32, endurance: f32, seconds: f32) -> f32 {
    let spare_speed = sustainable_jog_speed(endurance) - speed.max(0.0);
    let rate = if spare_speed >= 0.0 {
        -BREATH_RECOVERY_PER_SPARE_METRE_SECOND * spare_speed
    } else {
        BREATH_EXHAUSTION_PER_EXCESS_METRE_SECOND * -spare_speed
    };
    (current + rate * seconds.max(0.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_approx(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
    }

    #[test]
    fn endurance_curve_hits_walk_run_and_elite_marathon_anchors() {
        assert_approx(sustainable_jog_speed(0.0), 0.0);
        assert_approx(sustainable_jog_speed(1.0), 1.8);
        assert_approx(sustainable_jog_speed(2.0), 2.0);
        assert_approx(sustainable_jog_speed(3.0), 2.7);
        assert_approx(sustainable_jog_speed(5.0), 5.83);
    }

    #[test]
    fn sprint_curve_hits_child_average_man_and_olympic_anchors() {
        assert_approx(tactical_sprint_speed(0.0), 0.0);
        assert_approx(tactical_sprint_speed(1.0), 4.0);
        assert_approx(tactical_sprint_speed(3.0), 8.0);
        assert_approx(tactical_sprint_speed(5.0), 12.0);
    }

    #[test]
    fn movement_speed_preserves_analogue_magnitude_guard_and_breath_caps() {
        let half = Some(Vec2::new(0.3, 0.4));
        assert_eq!(
            tactical_movement_speed(
                half,
                MovementGait::Walk,
                WeaponGuardState::Lowered,
                3.0,
                3.0,
                0.0,
            ),
            TACTICAL_WALK_SPEED_METRES_PER_SECOND * 0.5
        );
        assert_eq!(
            tactical_movement_speed(
                half,
                MovementGait::Sprint,
                WeaponGuardState::Raised,
                3.0,
                3.0,
                0.0,
            ),
            TACTICAL_GUARD_SPEED_METRES_PER_SECOND * 0.5
        );
        assert_approx(
            tactical_movement_speed(
                Some(Vec2::X),
                MovementGait::Sprint,
                WeaponGuardState::Lowered,
                3.0,
                3.0,
                1.0,
            ),
            sustainable_jog_speed(3.0),
        );
    }

    #[test]
    fn jog_is_breath_neutral_while_sprint_exhausts_and_walking_recovers() {
        let jog = sustainable_jog_speed(3.0);
        assert_approx(breath_exhaustion_after(0.4, jog, 3.0, 10.0), 0.4);
        assert!(breath_exhaustion_after(0.4, 8.0, 3.0, 1.0) > 0.4);
        assert!(breath_exhaustion_after(0.4, 1.4, 3.0, 1.0) < 0.4);
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
            world.get::<CharacterController>(generic).unwrap().speed,
            sustainable_jog_speed(3.0)
        );
    }
}
