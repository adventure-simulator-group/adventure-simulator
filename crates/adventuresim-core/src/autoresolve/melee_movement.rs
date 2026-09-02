#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MovementIntent {
    Close,
    Hold,
    Retreat,
}

impl MovementIntent {
    fn target_velocity(self, maximum_speed_metres_per_second: f32) -> f32 {
        match self {
            Self::Close => -maximum_speed_metres_per_second,
            Self::Hold => 0.0,
            Self::Retreat => maximum_speed_metres_per_second,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AxisMotion {
    pub velocity_before_metres_per_second: f32,
    pub velocity_after_metres_per_second: f32,
    pub speed_limit_metres_per_second: f32,
    pub displacement_metres: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OpposedMovement {
    pub distance_before_metres: f32,
    pub distance_after_metres: f32,
    pub first: AxisMotion,
    pub second: AxisMotion,
    pub elapsed_seconds: f32,
}

/// Integrates one actor's signed contribution to separation over real elapsed
/// time. Closing velocity is negative and retreat velocity is positive. The
/// trapezoidal result is exact while the authored acceleration limit is active
/// and while velocity is capped at guarded movement speed.
fn integrate_axis(
    velocity_metres_per_second: f32,
    intent: MovementIntent,
    maximum_speed_metres_per_second: f32,
    maximum_acceleration_metres_per_second_squared: f32,
    elapsed_seconds: f32,
) -> AxisMotion {
    let elapsed_seconds = elapsed_seconds.max(0.0);
    let maximum_speed = maximum_speed_metres_per_second.max(0.0);
    let acceleration = maximum_acceleration_metres_per_second_squared.max(f32::EPSILON);
    let before = velocity_metres_per_second.clamp(-maximum_speed, maximum_speed);
    let target = intent.target_velocity(maximum_speed);
    let delta = target - before;
    let acceleration_seconds = (delta.abs() / acceleration).min(elapsed_seconds);
    let acceleration_direction = delta.signum();
    let after_acceleration = before + acceleration_direction * acceleration * acceleration_seconds;
    let after = if acceleration_seconds < elapsed_seconds {
        target
    } else {
        after_acceleration.clamp(before.min(target), before.max(target))
    };
    let accelerating_displacement = (before + after_acceleration) * 0.5 * acceleration_seconds;
    let constant_displacement = target * (elapsed_seconds - acceleration_seconds);
    AxisMotion {
        velocity_before_metres_per_second: before,
        velocity_after_metres_per_second: after,
        speed_limit_metres_per_second: maximum_speed,
        displacement_metres: accelerating_displacement + constant_displacement,
    }
}

pub(super) fn ground_drive_acceleration(
    force_newtons: f32,
    leg_strength: f32,
    reference_leg_strength: f32,
    body_mass_kg: f32,
    equipment_mass_kg: f32,
    gravity_metres_per_second_squared: f32,
    traction_coefficient: f32,
) -> f32 {
    let mass = (body_mass_kg + equipment_mass_kg).max(1.0);
    let drive_force =
        force_newtons.max(0.0) * (leg_strength / reference_leg_strength.max(1.0)).max(0.0);
    (drive_force / mass)
        .min(gravity_metres_per_second_squared.max(0.0) * traction_coefficient.max(0.0))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the pure integrator receives both actors' independent physical bounds"
)]
pub(super) fn integrate_opposed_movement(
    distance_metres: f32,
    first_velocity_metres_per_second: f32,
    first_intent: MovementIntent,
    first_maximum_speed_metres_per_second: f32,
    second_velocity_metres_per_second: f32,
    second_intent: MovementIntent,
    second_maximum_speed_metres_per_second: f32,
    first_maximum_acceleration_metres_per_second_squared: f32,
    second_maximum_acceleration_metres_per_second_squared: f32,
    elapsed_seconds: f32,
    minimum_distance_metres: f32,
    maximum_distance_metres: f32,
) -> OpposedMovement {
    let mut first = integrate_axis(
        first_velocity_metres_per_second,
        first_intent,
        first_maximum_speed_metres_per_second,
        first_maximum_acceleration_metres_per_second_squared,
        elapsed_seconds,
    );
    let mut second = integrate_axis(
        second_velocity_metres_per_second,
        second_intent,
        second_maximum_speed_metres_per_second,
        second_maximum_acceleration_metres_per_second_squared,
        elapsed_seconds,
    );
    let raw_after = distance_metres + first.displacement_metres + second.displacement_metres;
    let bounded_after = raw_after.clamp(minimum_distance_metres, maximum_distance_metres);
    let raw_delta = raw_after - distance_metres;
    let bounded_delta = bounded_after - distance_metres;
    if (raw_after - bounded_after).abs() > f32::EPSILON && raw_delta.abs() > f32::EPSILON {
        let travel_fraction = (bounded_delta / raw_delta).clamp(0.0, 1.0);
        first.displacement_metres *= travel_fraction;
        second.displacement_metres *= travel_fraction;
        if raw_after < minimum_distance_metres {
            first.velocity_after_metres_per_second =
                first.velocity_after_metres_per_second.max(0.0);
            second.velocity_after_metres_per_second =
                second.velocity_after_metres_per_second.max(0.0);
        } else {
            first.velocity_after_metres_per_second =
                first.velocity_after_metres_per_second.min(0.0);
            second.velocity_after_metres_per_second =
                second.velocity_after_metres_per_second.min(0.0);
        }
    }
    OpposedMovement {
        distance_before_metres: distance_metres,
        distance_after_metres: bounded_after,
        first,
        second,
        elapsed_seconds: elapsed_seconds.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEED: f32 = 2.0;
    const ACCELERATION: f32 = 24.0;

    #[test]
    fn displacement_never_exceeds_speed_times_elapsed() {
        for intent in [
            MovementIntent::Close,
            MovementIntent::Hold,
            MovementIntent::Retreat,
        ] {
            let motion = integrate_axis(0.0, intent, SPEED, ACCELERATION, 0.25);
            assert!(motion.displacement_metres.abs() <= SPEED * 0.25 + 1.0e-6);
        }
    }

    #[test]
    fn acceleration_integration_is_substep_invariant() {
        let whole = integrate_axis(0.0, MovementIntent::Close, SPEED, ACCELERATION, 0.2);
        let first = integrate_axis(0.0, MovementIntent::Close, SPEED, ACCELERATION, 0.1);
        let second = integrate_axis(
            first.velocity_after_metres_per_second,
            MovementIntent::Close,
            SPEED,
            ACCELERATION,
            0.1,
        );
        assert!(
            (whole.displacement_metres - (first.displacement_metres + second.displacement_metres))
                .abs()
                < 1.0e-6
        );
        assert!(
            (whole.velocity_after_metres_per_second - second.velocity_after_metres_per_second)
                .abs()
                < 1.0e-6
        );
    }

    #[test]
    fn added_body_or_equipment_mass_reduces_ground_acceleration() {
        let light = ground_drive_acceleration(1_000.0, 2.0, 3.0, 70.0, 5.0, 9.81, 0.9);
        let armored = ground_drive_acceleration(1_000.0, 2.0, 3.0, 80.0, 25.0, 9.81, 0.9);
        assert!(light > armored);
    }

    #[test]
    fn opposed_intent_cannot_reverse_velocity_instantaneously() {
        let motion = integrate_axis(2.0, MovementIntent::Close, SPEED, 12.0, 0.01);
        assert!(motion.velocity_after_metres_per_second > 0.0);
        assert!(motion.displacement_metres > 0.0);
    }

    #[test]
    fn opposed_intents_use_signed_simultaneous_displacement() {
        let mutual_close = integrate_opposed_movement(
            2.0,
            0.0,
            MovementIntent::Close,
            SPEED,
            0.0,
            MovementIntent::Close,
            SPEED,
            ACCELERATION,
            ACCELERATION,
            0.25,
            0.0,
            3.0,
        );
        let chase = integrate_opposed_movement(
            2.0,
            0.0,
            MovementIntent::Close,
            SPEED,
            0.0,
            MovementIntent::Retreat,
            SPEED,
            ACCELERATION,
            ACCELERATION,
            0.25,
            0.0,
            3.0,
        );
        assert!(mutual_close.distance_after_metres < 2.0);
        assert!((chase.distance_after_metres - 2.0).abs() < 1.0e-6);
    }

    #[test]
    fn body_collision_prevents_center_overlap_and_stops_closing_velocity() {
        let movement = integrate_opposed_movement(
            0.81,
            -SPEED,
            MovementIntent::Close,
            SPEED,
            -SPEED,
            MovementIntent::Close,
            SPEED,
            ACCELERATION,
            ACCELERATION,
            0.1,
            0.8,
            3.0,
        );
        assert!((movement.distance_after_metres - 0.8).abs() < 1.0e-6);
        assert!(movement.first.velocity_after_metres_per_second >= 0.0);
        assert!(movement.second.velocity_after_metres_per_second >= 0.0);
        assert!(movement.first.displacement_metres.abs() <= SPEED * 0.1);
        assert!(movement.second.displacement_metres.abs() <= SPEED * 0.1);
    }

    #[test]
    fn body_collision_floor_is_substep_invariant_and_cannot_tunnel() {
        let whole = integrate_opposed_movement(
            1.0,
            -SPEED,
            MovementIntent::Close,
            SPEED,
            -SPEED,
            MovementIntent::Close,
            SPEED,
            ACCELERATION,
            ACCELERATION,
            0.2,
            0.8,
            3.0,
        );
        let first = integrate_opposed_movement(
            1.0,
            -SPEED,
            MovementIntent::Close,
            SPEED,
            -SPEED,
            MovementIntent::Close,
            SPEED,
            ACCELERATION,
            ACCELERATION,
            0.1,
            0.8,
            3.0,
        );
        let second = integrate_opposed_movement(
            first.distance_after_metres,
            first.first.velocity_after_metres_per_second,
            MovementIntent::Close,
            SPEED,
            first.second.velocity_after_metres_per_second,
            MovementIntent::Close,
            SPEED,
            ACCELERATION,
            ACCELERATION,
            0.1,
            0.8,
            3.0,
        );
        assert!((whole.distance_after_metres - 0.8).abs() < 1.0e-6);
        assert!((second.distance_after_metres - 0.8).abs() < 1.0e-6);
    }
}
