use bevy_enhanced_input::prelude::InputAction;

const MELEE_LUNGE_DISTANCE_EPSILON_METRES: f32 = 1.0e-5;

pub fn melee_lunge_range_window_metres() -> f32 {
    crate::combat_config::runtime_melee_authority_config().lunge_range_window_metres
}

pub fn melee_lunge_quickstep_threshold_metres() -> f32 {
    crate::combat_config::runtime_melee_authority_config().lunge_quickstep_threshold_metres
}
#[must_use]
pub fn melee_interaction_range(arm_reach: f32, weapon_reach: f32) -> f32 {
    arm_reach.max(0.0) + weapon_reach.max(0.0)
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
    let fixed_tick_safety = 1.0 / crate::animation::locomotion_sample_hz();
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
    if gap <= melee_lunge_range_window_metres() + MELEE_LUNGE_DISTANCE_EPSILON_METRES
        || gap > quickstep_distance_metres.max(0.0) + MELEE_LUNGE_DISTANCE_EPSILON_METRES
    {
        MeleeLunge::None
    } else if gap > melee_lunge_quickstep_threshold_metres() + MELEE_LUNGE_DISTANCE_EPSILON_METRES {
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
            ) - ((0.2_f32).sqrt() + 1.0 / crate::animation::locomotion_sample_hz()))
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
            ) - (0.5 + 1.0 / crate::animation::locomotion_sample_hz()))
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
