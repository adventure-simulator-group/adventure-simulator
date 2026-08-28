use bevy::{
    math::{Vec3, Vec3Swizzles},
    prelude::Vec2,
};
use bevy_enhanced_input::prelude::InputAction;

/// Melee attacks already this close to their ordinary interaction range do not
/// spend movement correcting the final few centimetres.
pub const MELEE_LUNGE_RANGE_WINDOW_METRES: f32 = 0.10;
/// Gaps beyond this distance use quickstep-speed movement instead of ordinary
/// forward movement.
pub const MELEE_LUNGE_QUICKSTEP_THRESHOLD_METRES: f32 = 0.50;
const MELEE_LUNGE_DISTANCE_EPSILON_METRES: f32 = 1.0e-5;
#[must_use]
pub fn melee_interaction_range(arm_reach: f32, weapon_reach: f32) -> f32 {
    arm_reach.max(0.0) + weapon_reach.max(0.0)
}

/// Horizontal travel required for the attack origin to reach a selected strike
/// point. Unlike a flat body-to-body separation, this preserves the vertical
/// cost of aiming at elevated limbs such as the head.
#[must_use]
pub fn melee_horizontal_closure(
    origin: Vec3,
    strike_point: Vec3,
    travel_direction: Vec2,
    reach: f32,
) -> Option<f32> {
    let delta = strike_point - origin;
    if !delta.is_finite() || !reach.is_finite() || reach < 0.0 {
        return None;
    }
    let direction = travel_direction.normalize_or_zero();
    if direction == Vec2::ZERO {
        return None;
    }
    let along = delta.xz().dot(direction);
    let lateral = delta.xz() - direction * along;
    let perpendicular_squared = lateral.length_squared() + delta.y * delta.y;
    if perpendicular_squared > reach * reach {
        return None;
    }
    let along_in_reach = (reach * reach - perpendicular_squared).max(0.0).sqrt();
    if along < 0.0 {
        return (delta.length() <= reach).then_some(0.0);
    }
    Some((along - along_in_reach).max(0.0))
}

/// Resolves the closest point on a selected collider that can be struck after
/// bounded horizontal travel. The caller supplies collision-limited travel.
#[must_use]
pub fn reachable_melee_strike_point(
    collider: &avian3d::prelude::Collider,
    collider_translation: Vec3,
    collider_rotation: bevy::prelude::Quat,
    origin: Vec3,
    travel_direction: Vec2,
    reach: f32,
    maximum_travel: f32,
) -> Option<(Vec3, f32)> {
    let direction = travel_direction.normalize_or_zero();
    if direction == Vec2::ZERO {
        return None;
    }
    let prospective_origin = origin + Vec3::new(direction.x, 0.0, direction.y) * maximum_travel;
    let (strike_point, _) = collider.project_point(
        collider_translation,
        avian3d::prelude::Rotation(collider_rotation),
        prospective_origin,
        false,
    );
    let closure = melee_horizontal_closure(origin, strike_point, direction, reach)?;
    (closure <= maximum_travel + MELEE_LUNGE_DISTANCE_EPSILON_METRES)
        .then_some((strike_point, closure))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeleeLunge {
    None,
    Forward { distance_metres: f32 },
    Quickstep { distance_metres: f32 },
}

#[must_use]
pub fn melee_lunge_delay_seconds(
    lunge: MeleeLunge,
    forward_speed_metres_per_second: f32,
    forward_acceleration_metres_per_second_squared: f32,
    quickstep_duration_seconds: f32,
) -> f32 {
    let fixed_tick_safety = 1.0 / crate::animation::LOCOMOTION_SAMPLE_HZ;
    match lunge {
        MeleeLunge::None => 0.0,
        MeleeLunge::Forward { distance_metres } => {
            let speed = forward_speed_metres_per_second.max(f32::EPSILON);
            let acceleration = forward_acceleration_metres_per_second_squared.max(f32::EPSILON);
            let acceleration_seconds = speed / acceleration;
            let acceleration_distance = 0.5 * acceleration * acceleration_seconds.powi(2);
            let travel_seconds = if distance_metres <= acceleration_distance {
                (2.0 * distance_metres.max(0.0) / acceleration).sqrt()
            } else {
                acceleration_seconds + (distance_metres - acceleration_distance) / speed
            };
            travel_seconds + fixed_tick_safety
        }
        MeleeLunge::Quickstep { .. } => quickstep_duration_seconds.max(0.0) + fixed_tick_safety,
    }
}

#[must_use]
pub fn conservative_forward_lunge_acceleration(
    motor: &crate::combat_config::CharacterMotorConfig,
) -> f32 {
    let minimum_healthy_strength_scale = 1.0 / motor.reference_leg_strength.max(1.0);
    let drive = motor.reference_ground_drive_force_newtons * minimum_healthy_strength_scale
        / motor.fallback_character_mass_kg.max(1.0);
    drive.min(motor.gravity_metres_per_second_squared * motor.traction_coefficient)
}

/// Chooses the attack movement needed to bring a target into ordinary melee
/// range. Targets farther away than one complete quickstep remain valid attack
/// attempts, but do not cause movement.
#[must_use]
pub fn melee_lunge(
    separation_metres: f32,
    arm_reach: f32,
    weapon_reach: f32,
    quickstep_distance_metres: f32,
) -> MeleeLunge {
    if !separation_metres.is_finite() || !quickstep_distance_metres.is_finite() {
        return MeleeLunge::None;
    }
    let gap = (separation_metres - melee_interaction_range(arm_reach, weapon_reach)).max(0.0);
    if gap <= MELEE_LUNGE_RANGE_WINDOW_METRES + MELEE_LUNGE_DISTANCE_EPSILON_METRES
        || gap > quickstep_distance_metres.max(0.0) + MELEE_LUNGE_DISTANCE_EPSILON_METRES
    {
        MeleeLunge::None
    } else if gap > MELEE_LUNGE_QUICKSTEP_THRESHOLD_METRES + MELEE_LUNGE_DISTANCE_EPSILON_METRES {
        MeleeLunge::Quickstep {
            distance_metres: gap,
        }
    } else {
        MeleeLunge::Forward {
            distance_metres: gap,
        }
    }
}

#[must_use]
pub fn maximum_melee_lunge_range(
    arm_reach: f32,
    weapon_reach: f32,
    quickstep_distance_metres: f32,
) -> f32 {
    melee_interaction_range(arm_reach, weapon_reach) + quickstep_distance_metres.max(0.0)
}

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Attack;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Dodge;

#[derive(Debug, InputAction, Default)]
#[action_output(f32)]
pub struct Parry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn melee_lunge_respects_window_mode_threshold_and_maximum_travel() {
        let range = melee_interaction_range(0.55, 0.8);
        assert_eq!(melee_lunge(range + 0.10, 0.55, 0.8, 1.0), MeleeLunge::None);
        assert!(matches!(
            melee_lunge(range + 0.11, 0.55, 0.8, 1.0),
            MeleeLunge::Forward { distance_metres } if (distance_metres - 0.11).abs() < 1.0e-5
        ));
        assert!(matches!(
            melee_lunge(range + 0.51, 0.55, 0.8, 1.0),
            MeleeLunge::Quickstep { distance_metres } if (distance_metres - 0.51).abs() < 1.0e-5
        ));
        assert_eq!(melee_lunge(range + 1.01, 0.55, 0.8, 1.0), MeleeLunge::None);
        assert_eq!(maximum_melee_lunge_range(0.55, 0.8, 1.0), range + 1.0);
    }

    #[test]
    fn fist_and_weapon_ranges_add_anatomy_exactly_once() {
        assert!((melee_interaction_range(0.526_801, 0.0) - 0.526_801).abs() < 1.0e-6);
        assert!((melee_interaction_range(0.526_801, 0.8) - 1.326_801).abs() < 1.0e-6);
    }

    #[test]
    fn elevated_strike_point_requires_extra_horizontal_closure() {
        let origin = Vec3::new(0.0, 1.2, 0.0);
        let head = Vec3::new(1.0, 1.6, 0.0);
        let closure = melee_horizontal_closure(origin, head, Vec2::X, 0.5).unwrap();
        assert!((closure - 0.7).abs() < 1.0e-5);
        assert!(melee_horizontal_closure(origin, Vec3::new(1.0, 1.8, 0.0), Vec2::X, 0.5).is_none());
    }

    #[test]
    fn closest_limb_surface_preserves_a_reachable_diagonal_strike() {
        let head = avian3d::prelude::Collider::cuboid(0.5, 0.4, 0.4);
        let origin = Vec3::new(0.0, 1.2, 0.0);
        let (point, closure) = reachable_melee_strike_point(
            &head,
            Vec3::new(1.2, 1.65, 0.0),
            bevy::prelude::Quat::IDENTITY,
            origin,
            Vec2::X,
            0.55,
            0.8,
        )
        .expect("near head surface is reachable even when its center-directed point is not");
        let arrived = origin + Vec3::X * closure;
        assert!(arrived.distance(point) <= 0.55 + 1.0e-5);
        assert!(
            reachable_melee_strike_point(
                &head,
                Vec3::new(2.5, 1.65, 0.0),
                bevy::prelude::Quat::IDENTITY,
                origin,
                Vec2::X,
                0.55,
                0.8,
            )
            .is_none()
        );
    }

    #[test]
    fn lunge_delay_uses_existing_forward_and_quickstep_motion_timing() {
        assert_eq!(
            melee_lunge_delay_seconds(MeleeLunge::None, 4.0, 4.0, 0.5),
            0.0
        );
        assert!(
            (melee_lunge_delay_seconds(
                MeleeLunge::Forward {
                    distance_metres: 0.4
                },
                4.0,
                4.0,
                0.5,
            ) - ((0.2_f32).sqrt() + 1.0 / crate::animation::LOCOMOTION_SAMPLE_HZ))
                .abs()
                < 1.0e-6
        );
        assert!(
            (melee_lunge_delay_seconds(
                MeleeLunge::Quickstep {
                    distance_metres: 0.8
                },
                4.0,
                4.0,
                0.5,
            ) - (0.5 + 1.0 / crate::animation::LOCOMOTION_SAMPLE_HZ))
                .abs()
                < 1.0e-6
        );
        let config = crate::combat_config::TacticalCombatConfig::default();
        let default_acceleration = conservative_forward_lunge_acceleration(&config.movement.motor);
        for distance in [0.404, 0.441] {
            assert!(
                melee_lunge_delay_seconds(
                    MeleeLunge::Forward {
                        distance_metres: distance,
                    },
                    config.movement.speeds_metres_per_second.run,
                    default_acceleration,
                    config.movement.maneuvers.quickstep_duration_seconds,
                ) > 0.18
            );
        }
    }
}
