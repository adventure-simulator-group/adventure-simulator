use super::*;

const MAX_PRESENTATION_SAMPLE_GAP: u64 = 32;

pub(in crate::animation::procedural) fn presentation_tick_delta(
    previous: Option<u64>,
    current: u64,
) -> Option<u64> {
    match previous {
        None => Some(0),
        Some(previous) => {
            let delta = current.wrapping_sub(previous);
            (delta <= MAX_PRESENTATION_SAMPLE_GAP).then_some(delta)
        }
    }
}

/// Adds bounded travel and inertial body response from server-observed world
/// velocity and acceleration transformed through the current presentation body
/// frame.
/// Retained angles are client presentation only.
pub(in crate::animation) fn apply_locomotion_body_response(
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
    let mut responses = BTreeMap::new();
    for (owner, skeleton, owner_transform, state) in &mut owners {
        let mut next = state.as_deref().copied().unwrap_or_default();
        let tick = skeleton.locomotion_sample_tick;
        let tick_delta = presentation_tick_delta(next.last_tick, tick);
        let inverse_body_rotation = owner_transform.rotation.inverse();
        let body_velocity = inverse_body_rotation * skeleton.world_velocity;
        let body_acceleration = inverse_body_rotation * skeleton.world_acceleration;
        let discontinuous = tick_delta.is_none()
            || next
                .last_posture
                .is_some_and(|value| value != skeleton.posture())
            || next
                .last_action
                .is_some_and(|value| value != skeleton.action_kind())
            || next
                .last_grounded
                .is_some_and(|value| value != skeleton.is_grounded());
        let quickstep_target = (skeleton.action_kind() == SkeletonAction::Dodge)
            .then(|| quickstep_lean_target(skeleton.action_direction(), skeleton.action_phase()));
        if skeleton.is_posture_transitioning()
            || (skeleton.action_kind() != SkeletonAction::None && quickstep_target.is_none())
            || (!skeleton.is_grounded() && quickstep_target.is_none())
        {
            next.pitch_radians = 0.0;
            next.roll_radians = 0.0;
            next.target_pitch_radians = 0.0;
            next.target_roll_radians = 0.0;
        } else if let Some(target) = quickstep_target {
            next.target_pitch_radians = target.x;
            next.target_roll_radians = target.y;
            if tick_delta != Some(0) {
                let maximum_step = 2.0_f32.to_radians() * tick_delta.unwrap_or(1).max(1) as f32;
                let current = Vec2::new(next.pitch_radians, next.roll_radians);
                let advanced = current + (target - current).clamp_length_max(maximum_step);
                next.pitch_radians = advanced.x;
                next.roll_radians = advanced.y;
            }
        } else if discontinuous {
            next.pitch_radians = 0.0;
            next.roll_radians = 0.0;
            next.target_pitch_radians = 0.0;
            next.target_roll_radians = 0.0;
        } else if let Some(tick_delta @ 1..) = tick_delta {
            let delta_seconds = tick_delta as f32 / LOCOMOTION_SAMPLE_HZ;
            if body_velocity.xz().length() > 0.05 || body_acceleration.xz().length() > 0.5 {
                let braking_scale = deceleration_lean_scale(
                    next.last_body_velocity,
                    body_velocity,
                    body_acceleration,
                );
                let combined =
                    body_response_target(body_velocity, body_acceleration, braking_scale);
                next.target_pitch_radians = combined.x;
                next.target_roll_radians = combined.y;
            } else {
                let target_decay = 10.0_f32.to_radians() / 0.4 * delta_seconds;
                next.target_pitch_radians =
                    advance_towards(next.target_pitch_radians, 0.0, target_decay);
                next.target_roll_radians =
                    advance_towards(next.target_roll_radians, 0.0, target_decay);
            }
            let maximum_step = 2.0_f32.to_radians() * tick_delta as f32;
            let current = Vec2::new(next.pitch_radians, next.roll_radians);
            let target = Vec2::new(next.target_pitch_radians, next.target_roll_radians);
            let advanced = current + (target - current).clamp_length_max(maximum_step);
            next.pitch_radians = advanced.x;
            next.roll_radians = advanced.y;
        }
        if next.last_tick.is_none() || discontinuous || matches!(tick_delta, Some(1..)) {
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

fn quickstep_lean_target(direction: Vec2, action_phase: f32) -> Vec2 {
    let direction = direction.normalize_or_zero();
    let envelope =
        smoothstep(0.0, 0.18, action_phase) * (1.0 - smoothstep(0.65, 1.0, action_phase));
    Vec2::new(direction.y, -direction.x) * 18.0_f32.to_radians() * envelope
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
    // travel supplies a pronounced base lean and acceleration adds a stronger
    // short-lived inertial response, following Overgrowth's division of work.
    let travel_pitch = (velocity.z / RUN_LOCOMOTION_PROFILE.reference_speed
        * 12.0_f32.to_radians())
    .clamp(-14.0_f32.to_radians(), 14.0_f32.to_radians());
    let travel_roll = (-velocity.x / RUN_LOCOMOTION_PROFILE.reference_speed
        * 12.0_f32.to_radians())
    .clamp(-14.0_f32.to_radians(), 14.0_f32.to_radians());
    let inertial_pitch = if acceleration.z > 0.0 {
        (acceleration.z / 12.0 * 18.0_f32.to_radians()).clamp(0.0, 22.0_f32.to_radians())
    } else {
        (acceleration.z / 12.0 * 14.0_f32.to_radians()).clamp(-18.0_f32.to_radians(), 0.0)
            * braking_scale.clamp(0.0, 1.0)
    };
    // Turning should read clearly without the extreme motorcycle-like bank of
    // the first stronger-lean pass. Keep lateral travel posture unchanged and
    // scale only acceleration-driven turning response to 60% of that tuning.
    let inertial_roll = (-acceleration.x / 10.0 * 9.6_f32.to_radians())
        .clamp(-12.0_f32.to_radians(), 12.0_f32.to_radians());
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
        (planar.length() / RUN_LOCOMOTION_PROFILE.reference_speed).clamp(0.0, 1.0)
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
    fn quickstep_lean_follows_the_dodge_direction_and_recovers() {
        let forward = quickstep_lean_target(Vec2::Y, 0.4);
        let right = quickstep_lean_target(Vec2::X, 0.4);
        assert!(forward.x > 15.0_f32.to_radians());
        assert!(forward.y.abs() <= f32::EPSILON);
        assert!(right.y < -15.0_f32.to_radians());
        assert!(right.x.abs() <= f32::EPSILON);
        assert_eq!(quickstep_lean_target(Vec2::Y, 1.0), Vec2::ZERO);
    }

    #[test]
    fn deceleration_lean_follows_current_planar_speed_only_while_braking() {
        let walking = Vec3::Z * WALK_LOCOMOTION_PROFILE.reference_speed;
        let walking_scale = deceleration_lean_scale(walking, walking, Vec3::NEG_Z * 12.0);
        assert!(
            (walking_scale
                - WALK_LOCOMOTION_PROFILE.reference_speed / RUN_LOCOMOTION_PROFILE.reference_speed)
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
}
