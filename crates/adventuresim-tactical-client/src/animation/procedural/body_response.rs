//! Bounded locomotion body response applied after authored pose evaluation.

use super::*;

fn body_response_tuning() -> BodyResponseConfig {
    runtime_animation_config().procedural.body_response
}

pub(in crate::animation::procedural) fn presentation_tick_delta(
    previous: Option<u64>,
    current: u64,
) -> Option<u64> {
    match previous {
        None => Some(0),
        Some(previous) => {
            let delta = current.wrapping_sub(previous);
            (delta <= body_response_tuning().maximum_presentation_sample_gap_ticks).then_some(delta)
        }
    }
}

/// Adds bounded travel and inertial body response from server-observed world
/// velocity and acceleration transformed through the current presentation body
/// frame.
/// Retained angles are client presentation only.
pub(in crate::animation) fn apply_locomotion_body_response(
    time: Res<Time>,
    procedural_clock: Res<ProceduralAnimationClock>,
    mut commands: Commands,
    mut owners: Query<
        (
            Entity,
            &PresentedSkeleton,
            &Transform,
            Option<&mut LocomotionBodyResponseState>,
        ),
        Without<HumanoidBone>,
    >,
    mut bones: Query<
        (&HumanoidBone, &AuthoredBindTransform, &mut Transform),
        Without<PresentedSkeleton>,
    >,
) {
    let _spike = crate::animation::diagnostics::SpikeGuard::new("apply_locomotion_body_response");
    let render_delta_seconds = time
        .delta_secs()
        .clamp(0.0, body_response_tuning().maximum_frame_seconds);
    let mut responses = BTreeMap::new();
    for (owner, skeleton, owner_transform, state) in &mut owners {
        let mut next = state.as_deref().copied().unwrap_or_default();
        let tick = skeleton.locomotion_sample_tick;
        let tick_delta = presentation_tick_delta(next.last_tick, tick);
        let delta_seconds = match procedural_clock.fixed_step() {
            Some((fixed_tick, _)) if next.last_tick == Some(fixed_tick) => 0.0,
            Some((_, fixed_delta_seconds)) => fixed_delta_seconds,
            None => render_delta_seconds,
        };
        let inverse_body_rotation = owner_transform.rotation.inverse();
        let body_velocity = inverse_body_rotation * skeleton.world_velocity;
        let body_acceleration = inverse_body_rotation * skeleton.world_acceleration;
        let discontinuous = tick_delta.is_none();
        let source_changed = matches!(tick_delta, Some(1..));
        if skeleton.is_posture_transitioning()
            || skeleton.action_kind() != SkeletonAction::None
            || !skeleton.is_grounded()
            || skeleton.weapon_guard() == WeaponGuardState::Raised
            || discontinuous
        {
            next.pitch_radians = 0.0;
            next.roll_radians = 0.0;
            next.angular_velocity_radians_per_second = Vec2::ZERO;
            next.target_pitch_radians = 0.0;
            next.target_roll_radians = 0.0;
            next.smoothed_body_acceleration = Vec3::ZERO;
        } else {
            // Authoritative acceleration is a discrete finite difference and
            // remains unchanged between replicated samples. Filter that held
            // value in render time so packet delivery cannot create an
            // inertial impulse. Releasing the impulse more slowly also avoids
            // a second head snap when the motor reaches its target speed.
            next.smoothed_body_acceleration = smooth_acceleration(
                next.smoothed_body_acceleration,
                body_acceleration,
                delta_seconds,
            );
            let braking_scale = deceleration_lean_scale(
                next.last_body_velocity,
                body_velocity,
                next.smoothed_body_acceleration,
            );
            let combined = body_response_target(
                body_velocity,
                next.smoothed_body_acceleration,
                braking_scale,
            );
            next.target_pitch_radians = combined.x;
            next.target_roll_radians = combined.y;
            let current = Vec2::new(next.pitch_radians, next.roll_radians);
            let target = Vec2::new(next.target_pitch_radians, next.target_roll_radians);
            let (advanced, angular_velocity) = advance_body_response(
                current,
                next.angular_velocity_radians_per_second,
                target,
                delta_seconds,
            );
            next.pitch_radians = advanced.x;
            next.roll_radians = advanced.y;
            next.angular_velocity_radians_per_second = angular_velocity;
        }
        if next.last_tick.is_none() || discontinuous || source_changed {
            next.last_body_velocity = body_velocity;
        }
        next.last_tick = Some(tick);
        next.last_posture = Some(skeleton.posture());
        next.last_action = Some(skeleton.action_kind());
        next.last_grounded = Some(skeleton.is_grounded());
        responses.insert(owner, next);
        if let Some(mut state) = state {
            *state = next;
        } else {
            commands.entity(owner).insert(next);
        }
    }
    let mut leg_compensations = BTreeMap::new();
    for (bone, bind, mut transform) in &mut bones {
        let Some(response) = responses.get(&bone.owner) else {
            continue;
        };
        if bone.role != BoneRole::Pelvis {
            continue;
        }
        let response_rotation = Quat::from_euler(
            EulerRot::XYZ,
            response.pitch_radians,
            0.0,
            response.roll_radians,
        );
        let (pelvis_rotation, leg_compensation) =
            stable_pelvis_response(transform.rotation, bind.local.rotation, response_rotation);
        // Apply the response in the pelvis's stable authored reference frame.
        // Post-multiplying it into the live pelvis frame made ordinary gait
        // twist steer forward pitch alternately left and right.
        transform.rotation = pelvis_rotation;
        leg_compensations.insert(bone.owner, leg_compensation);
    }
    for (bone, _, mut transform) in &mut bones {
        if !matches!(bone.role, BoneRole::ThighLeft | BoneRole::ThighRight) {
            continue;
        }
        let Some(&compensation) = leg_compensations.get(&bone.owner) else {
            continue;
        };
        // The leg solver follows this pass. Exactly cancel the inherited
        // parent-space response at each hip so the authored leg pose and IK
        // targets are unchanged by torso lean.
        transform.rotation = compensation * transform.rotation;
    }
}

fn smooth_acceleration(current: Vec3, target: Vec3, delta_seconds: f32) -> Vec3 {
    let response_per_second = if target.xz().length_squared() > current.xz().length_squared() {
        body_response_tuning().acceleration_attack_response_per_second
    } else {
        body_response_tuning().acceleration_release_response_per_second
    };
    let response = 1.0 - (-response_per_second * delta_seconds.max(0.0)).exp();
    current.lerp(target, response)
}

fn advance_body_response(
    current: Vec2,
    velocity: Vec2,
    target: Vec2,
    delta_seconds: f32,
) -> (Vec2, Vec2) {
    let delta_seconds = delta_seconds.max(0.0);
    if delta_seconds == 0.0 {
        return (current, velocity);
    }

    // An implicit critically damped spring retains angular velocity across
    // target changes. Unlike a first-order slew limiter, it cannot consume a
    // small accumulated target error as a separate one-frame lean when the
    // presented motor velocity locks to its sprint plateau.
    let omega = 2.0 / body_response_tuning().smooth_time_seconds;
    let omega_squared = omega * omega;
    let denominator =
        1.0 + 2.0 * delta_seconds * omega + delta_seconds * delta_seconds * omega_squared;
    let next_velocity =
        (velocity + delta_seconds * omega_squared * (target - current)) / denominator;
    let maximum_speed = body_response_tuning().degrees_per_second.to_radians();
    let next_velocity = next_velocity.clamp_length_max(maximum_speed);
    let next = current + next_velocity * delta_seconds;
    (next, next_velocity)
}

fn stable_pelvis_response(
    authored_rotation: Quat,
    reference_rotation: Quat,
    response_rotation: Quat,
) -> (Quat, Quat) {
    let parent_space_response =
        (reference_rotation * response_rotation * reference_rotation.inverse()).normalize();
    let leg_compensation =
        (authored_rotation.inverse() * parent_space_response.inverse() * authored_rotation)
            .normalize();
    (
        (parent_space_response * authored_rotation).normalize(),
        leg_compensation,
    )
}

pub(in crate::animation::procedural) fn body_response_target(
    velocity: Vec3,
    acceleration: Vec3,
    braking_scale: f32,
) -> Vec2 {
    // Tactical body forward is local +Z (the authored rig carries its own
    // facing correction). Authored locomotion keeps a straight back, so steady
    // travel supplies a pronounced base lean and acceleration adds a somewhat
    // stronger combined startup pose. Keeping most of the lean in travel avoids
    // a large initial snap followed by an almost upright steady run.
    let travel_pitch = (velocity.z / run_locomotion_profile().reference_speed
        * body_response_tuning()
            .steady_travel_lean_degrees
            .to_radians())
    .clamp(-18.0_f32.to_radians(), 18.0_f32.to_radians());
    let travel_roll = (-velocity.x / run_locomotion_profile().reference_speed
        * body_response_tuning()
            .steady_travel_lean_degrees
            .to_radians())
    .clamp(-18.0_f32.to_radians(), 18.0_f32.to_radians());
    // Acceleration is an early accent rather than a second full pose. Fade it
    // almost completely into the stable velocity posture, so reaching target
    // speed cannot release a late, visibly separate lean.
    let speed_fraction =
        (velocity.xz().length() / run_locomotion_profile().reference_speed).clamp(0.0, 1.0);
    let startup_inertial_scale = body_response_tuning().startup_inertial_lean_scale.lerp(
        body_response_tuning().sustained_inertial_lean_scale,
        speed_fraction,
    );
    let inertial_pitch = if acceleration.z > 0.0 {
        (acceleration.z / 12.0
            * body_response_tuning()
                .forward_acceleration_lean_degrees
                .to_radians())
        .clamp(0.0, 14.0_f32.to_radians())
            * startup_inertial_scale
    } else {
        (acceleration.z / 12.0 * 18.0_f32.to_radians()).clamp(-22.0_f32.to_radians(), 0.0)
            * braking_scale.clamp(0.0, 1.0)
    };
    // Turning should read clearly without the extreme motorcycle-like bank of
    // the first stronger-lean pass. Keep lateral travel posture unchanged and
    // scale only acceleration-driven turning response to 60% of that tuning.
    let inertial_roll = (-acceleration.x / 10.0
        * body_response_tuning()
            .lateral_acceleration_lean_degrees
            .to_radians())
    .clamp(-8.0_f32.to_radians(), 8.0_f32.to_radians())
        * startup_inertial_scale;
    let pitch = travel_pitch + inertial_pitch;
    let roll = travel_roll + inertial_roll;
    let response = Vec2::new(pitch, roll);
    // Leave a sub-milliradian numerical margin so degree conversion cannot
    // report a value microscopically above the documented 30-degree cap.
    let maximum_response = 30.0_f32.to_radians() - 0.000001;
    if response.length_squared() > maximum_response * maximum_response {
        response.normalize_or_zero() * maximum_response
    } else {
        response
    }
}

fn deceleration_lean_scale(previous_velocity: Vec3, velocity: Vec3, acceleration: Vec3) -> f32 {
    let previous_planar = previous_velocity.xz();
    let planar = velocity.xz();
    let planar_acceleration = acceleration.xz();
    let is_decelerating = previous_planar.dot(planar_acceleration) < 0.0
        && planar.length_squared() <= previous_planar.length_squared() + 0.000_1;
    if is_decelerating {
        (planar.length() / run_locomotion_profile().reference_speed).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pelvis_response_uses_a_stable_reference_and_exactly_compensates_legs() {
        let reference = Quat::from_euler(EulerRot::XYZ, 0.12, -0.08, 0.04);
        let response = Quat::from_euler(EulerRot::XYZ, 0.21, 0.0, -0.09);
        let expected_parent_response = (reference * response * reference.inverse()).normalize();

        for authored in [
            reference,
            Quat::from_euler(EulerRot::XYZ, 0.35, 0.22, -0.18),
        ] {
            let (pelvis, leg_compensation) = stable_pelvis_response(authored, reference, response);
            assert!(
                (pelvis * authored.inverse()).angle_between(expected_parent_response) < 0.000_1
            );
            let compensation_error = (pelvis * leg_compensation).angle_between(authored);
            assert!(compensation_error < 0.001, "{compensation_error}");
        }
    }

    #[test]
    fn deceleration_lean_follows_current_planar_speed_only_while_braking() {
        let walking = Vec3::Z * walk_locomotion_profile().reference_speed;
        let walking_scale = deceleration_lean_scale(walking, walking, Vec3::NEG_Z * 12.0);
        assert!(
            (walking_scale
                - walk_locomotion_profile().reference_speed
                    / run_locomotion_profile().reference_speed)
                .abs()
                <= f32::EPSILON
        );
        assert_eq!(
            deceleration_lean_scale(Vec3::Z * 0.5, Vec3::ZERO, Vec3::NEG_Z * 12.0),
            0.0
        );
        assert_eq!(
            deceleration_lean_scale(Vec3::ZERO, Vec3::NEG_Z, Vec3::NEG_Z * 12.0),
            1.0
        );
        assert_eq!(
            deceleration_lean_scale(Vec3::Z * 5.5, Vec3::Z * 5.5, Vec3::NEG_Z * 12.0),
            1.0
        );
    }

    #[test]
    fn acceleration_response_has_smooth_attack_and_release() {
        let frame_seconds = 1.0 / 60.0;
        let raw = Vec3::Z * 12.0;
        let attacked = smooth_acceleration(Vec3::ZERO, raw, frame_seconds);
        assert!(attacked.z > 0.0 && attacked.z < raw.z);

        let released = smooth_acceleration(attacked, Vec3::ZERO, frame_seconds);
        assert!(released.z > 0.0 && released.z < attacked.z);
    }

    #[test]
    fn body_response_step_is_bounded_by_render_time() {
        let frame_seconds = 1.0 / 60.0;
        let current = Vec2::ZERO;
        let target = Vec2::splat(30.0_f32.to_radians());
        let (advanced, velocity) =
            advance_body_response(current, Vec2::ZERO, target, frame_seconds);
        let maximum_step = body_response_tuning().degrees_per_second.to_radians() * frame_seconds;
        assert!(advanced.length() <= maximum_step + 0.000_001);
        assert!(
            velocity.length() <= body_response_tuning().degrees_per_second.to_radians() + 0.000_001
        );
    }

    #[test]
    fn body_response_preserves_motion_through_a_target_step() {
        let frame_seconds = 1.0 / 60.0;
        let velocity = Vec2::new(28.0_f32.to_radians(), 0.0);
        let current = Vec2::new(14.0_f32.to_radians(), 0.0);
        let target = Vec2::new(16.0_f32.to_radians(), 0.0);
        let (advanced, next_velocity) =
            advance_body_response(current, velocity, target, frame_seconds);

        assert!(advanced.x > current.x);
        assert!(next_velocity.x > 0.0);
        assert!((advanced.x - current.x).to_degrees() < 0.75);
    }

    #[test]
    fn sprint_ramp_is_continuous_and_settles_below_its_startup_peak() {
        let frame_seconds = 1.0 / 64.0;
        let mut acceleration = Vec3::ZERO;
        let mut response = Vec2::ZERO;
        let mut velocity = Vec2::ZERO;
        let mut maximum_pitch = 0.0_f32;
        let mut maximum_step = 0.0_f32;

        for frame in 0..160 {
            let speed =
                (frame as f32 / 32.0).clamp(0.0, 1.0) * run_locomotion_profile().reference_speed;
            let raw_acceleration = if frame <= 32 {
                Vec3::Z * 11.0
            } else {
                Vec3::ZERO
            };
            acceleration = smooth_acceleration(acceleration, raw_acceleration, frame_seconds);
            let target = body_response_target(Vec3::Z * speed, acceleration, 1.0);
            let previous = response;
            (response, velocity) = advance_body_response(response, velocity, target, frame_seconds);
            maximum_pitch = maximum_pitch.max(response.x);
            maximum_step = maximum_step.max((response.x - previous.x).abs());
        }

        let stable_pitch = response.x.to_degrees();
        let startup_peak = maximum_pitch.to_degrees();
        assert!((15.9..=16.1).contains(&stable_pitch), "{stable_pitch}");
        assert!((16.25..=18.0).contains(&startup_peak), "{startup_peak}");
        assert!(maximum_step.to_degrees() <= 0.6);
    }
}
