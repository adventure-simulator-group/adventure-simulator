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

/// Adds bounded inertial body response from server-observed world
/// acceleration transformed through the current presentation body frame.
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
    mut bones: Query<(&HumanoidBone, &mut Transform), Without<PresentedSkeleton>>,
) {
    let mut responses = BTreeMap::new();
    for (owner, skeleton, owner_transform, state) in &mut owners {
        let mut next = state.as_deref().copied().unwrap_or_default();
        let tick = skeleton.locomotion_sample_tick;
        let tick_delta = presentation_tick_delta(next.last_tick, tick);
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
        if discontinuous
            || !skeleton.is_grounded()
            || skeleton.is_posture_transitioning()
            || skeleton.action_kind() != SkeletonAction::None
        {
            next.pitch_radians = 0.0;
            next.roll_radians = 0.0;
            next.target_pitch_radians = 0.0;
            next.target_roll_radians = 0.0;
        } else if let Some(tick_delta @ 1..) = tick_delta {
            let delta_seconds = tick_delta as f32 / LOCOMOTION_SAMPLE_HZ;
            let body_acceleration =
                owner_transform.rotation.inverse() * skeleton.world_acceleration;
            if body_acceleration.xz().length() > 0.5 {
                let combined = body_response_target(body_acceleration);
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
    for (bone, mut transform) in &mut bones {
        let Some(response) = responses.get(&bone.owner) else {
            continue;
        };
        let weight = match bone.role {
            BoneRole::Pelvis => 0.20,
            BoneRole::StomachOne => 0.25,
            BoneRole::StomachTwo => 0.25,
            BoneRole::Chest => 0.30,
            _ => continue,
        };
        transform.rotation *= Quat::from_euler(
            EulerRot::XYZ,
            response.pitch_radians * weight,
            0.0,
            response.roll_radians * weight,
        );
    }
}

pub(in crate::animation::procedural) fn body_response_target(acceleration: Vec3) -> Vec2 {
    // Tactical body forward is local +Z (the authored rig carries its own
    // facing correction), so positive Z acceleration pitches into travel.
    let pitch = if acceleration.z > 0.0 {
        (-acceleration.z / 12.0 * 10.0_f32.to_radians()).clamp(-12.0_f32.to_radians(), 0.0)
    } else {
        (-acceleration.z / 12.0 * 8.0_f32.to_radians()).clamp(0.0, 10.0_f32.to_radians())
    };
    let roll = (-acceleration.x / 10.0 * 8.0_f32.to_radians())
        .clamp(-10.0_f32.to_radians(), 10.0_f32.to_radians());
    let response = Vec2::new(pitch, roll);
    // Leave a sub-milliradian numerical margin so degree conversion cannot
    // report a value microscopically above the documented 15-degree cap.
    let maximum_response = 15.0_f32.to_radians() - 0.000001;
    if response.length_squared() > maximum_response * maximum_response {
        response.normalize_or_zero() * maximum_response
    } else {
        response
    }
}
