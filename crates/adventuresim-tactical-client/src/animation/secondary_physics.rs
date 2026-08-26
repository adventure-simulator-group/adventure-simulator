//! Client-only secondary joint dynamics.
//!
//! Gameplay state never flows back through this module. Each major rendered
//! joint follows the final authored/procedural pose through a damped angular
//! motor, then the solved rotation is blended by a semantic baseline plus the
//! authoritative incapacitation fraction.

use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

use super::{
    ImpactReaction, PresentedSkeleton,
    procedural::{BoneRole, HumanoidBone},
};

const MOTOR_FREQUENCY_HZ: f32 = 4.25;
const MOTOR_DAMPING_RATIO: f32 = 0.78;
const MAX_ANGULAR_SPEED_RADIANS_PER_SECOND: f32 = 18.0;
const IMPACT_ANGULAR_SPEED_PER_METRE_PER_SECOND: f32 = 0.85;
const MAX_LOCOMOTION_ACCELERATION_METRES_PER_SECOND_SQUARED: f32 = 24.0;
const RAGDOLL_MOTOR_FREQUENCY_HZ: f32 = 0.7;
const RAGDOLL_GRAVITY_TORQUE: f32 = 8.0;
const WEIGHT_RESPONSE_PER_SECOND: f32 = 12.0;

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct SecondaryBoneDynamics {
    simulated_rotation: Quat,
    angular_velocity: Vec3,
    previous_impact_remaining: f32,
    blend_weight: f32,
    initialized: bool,
}

impl Default for SecondaryBoneDynamics {
    fn default() -> Self {
        Self {
            simulated_rotation: Quat::IDENTITY,
            angular_velocity: Vec3::ZERO,
            previous_impact_remaining: 0.0,
            blend_weight: 0.0,
            initialized: false,
        }
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub(crate) struct SecondaryPhysicsTelemetry {
    pub(crate) simulated_upper_body_bones: u32,
    pub(crate) mean_upper_body_blend_weight: f32,
    pub(crate) maximum_pose_lag_degrees: f32,
    pub(crate) maximum_inertial_acceleration_radians_per_second_squared: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecondaryMotionClass {
    Relaxed,
    Moving,
    Running,
    Guarded,
    CommittedAction,
    Airborne,
    Downed,
    Ragdolled,
}

fn motion_class(skeleton: &PresentedSkeleton) -> SecondaryMotionClass {
    match skeleton.body() {
        BodyState::Ragdolled => SecondaryMotionClass::Ragdolled,
        BodyState::Airborne => SecondaryMotionClass::Airborne,
        BodyState::Prone | BodyState::Supine => SecondaryMotionClass::Downed,
        BodyState::Grounded(_) => {
            if skeleton.action_kind() != SkeletonAction::None {
                SecondaryMotionClass::CommittedAction
            } else if skeleton.weapon_guard() == WeaponGuardState::Raised {
                SecondaryMotionClass::Guarded
            } else if skeleton.animation_speed() > 3.2 {
                SecondaryMotionClass::Running
            } else if skeleton.animation_speed() > 0.08 {
                SecondaryMotionClass::Moving
            } else {
                SecondaryMotionClass::Relaxed
            }
        }
    }
}

fn is_above_pelvis(role: BoneRole) -> bool {
    matches!(
        role,
        BoneRole::StomachOne
            | BoneRole::StomachTwo
            | BoneRole::StomachThree
            | BoneRole::Chest
            | BoneRole::NeckOne
            | BoneRole::Head
            | BoneRole::ClavicleLeft
            | BoneRole::ClavicleRight
            | BoneRole::UpperArmLeft
            | BoneRole::ForearmLeft
            | BoneRole::HandLeft
            | BoneRole::UpperArmRight
            | BoneRole::ForearmRight
            | BoneRole::HandRight
    )
}

fn baseline_weight(class: SecondaryMotionClass, role: BoneRole) -> f32 {
    use BoneRole::*;
    let arm = matches!(
        role,
        ClavicleLeft
            | ClavicleRight
            | UpperArmLeft
            | ForearmLeft
            | HandLeft
            | UpperArmRight
            | ForearmRight
            | HandRight
    );
    let distal_arm = matches!(role, ForearmLeft | HandLeft | ForearmRight | HandRight);
    let axial = matches!(
        role,
        StomachOne | StomachTwo | StomachThree | Chest | NeckOne | Head
    );
    let leg = matches!(role, ThighLeft | ShinLeft | ThighRight | ShinRight);

    match class {
        SecondaryMotionClass::Relaxed if distal_arm => 0.44,
        SecondaryMotionClass::Relaxed if arm => 0.36,
        SecondaryMotionClass::Moving if distal_arm => 0.40,
        SecondaryMotionClass::Moving if arm => 0.32,
        // Overgrowth stiffens ordinary arms as running speed rises.
        SecondaryMotionClass::Running if distal_arm => 0.34,
        SecondaryMotionClass::Running if arm => 0.26,
        SecondaryMotionClass::Airborne if distal_arm => 0.52,
        SecondaryMotionClass::Airborne if arm => 0.44,
        SecondaryMotionClass::Guarded if arm => 0.10,
        SecondaryMotionClass::CommittedAction if arm => 0.12,
        SecondaryMotionClass::Downed if arm => 0.16,
        SecondaryMotionClass::Ragdolled if !matches!(role, Root | Pelvis) => 1.0,
        SecondaryMotionClass::Running if axial => 0.38,
        SecondaryMotionClass::Moving if axial => 0.30,
        SecondaryMotionClass::Airborne if axial => 0.46,
        SecondaryMotionClass::CommittedAction if axial => 0.32,
        SecondaryMotionClass::Guarded if axial => 0.22,
        _ if axial => 0.16,
        _ if leg => 0.025,
        _ if role == Pelvis => 0.015,
        _ if matches!(role, FootLeft | ToeLeft | FootRight | ToeRight) => 0.01,
        _ => 0.0,
    }
}

/// Presentation-only pseudo torque produced by acceleration of the
/// authoritative root. The spring still targets the authored local pose; this
/// term gives the simulated upper body the lag that a point mass above each
/// joint would have while the controller accelerates or changes direction.
fn locomotion_inertial_acceleration(
    world_acceleration: Vec3,
    owner_rotation: Quat,
    role: BoneRole,
) -> Vec3 {
    use BoneRole::*;
    let gain = match role {
        StomachOne => 0.12,
        StomachTwo => 0.18,
        StomachThree => 0.24,
        Chest => 0.30,
        NeckOne => 0.22,
        Head => 0.34,
        ClavicleLeft | ClavicleRight => 0.28,
        UpperArmLeft | UpperArmRight => 0.34,
        ForearmLeft | ForearmRight => 0.42,
        HandLeft | HandRight => 0.48,
        _ => 0.0,
    };
    let local = owner_rotation.inverse() * world_acceleration;
    let local = Vec3::new(local.x, 0.0, local.z)
        .clamp_length_max(MAX_LOCOMOTION_ACCELERATION_METRES_PER_SECOND_SQUARED);
    // Horizontal acceleration produces the opposite apparent lean: local +X
    // drives +Z-side lag (roll), while local +Z drives -Z-side lag (pitch).
    Vec3::new(-local.z, 0.0, local.x) * gain
}

pub(super) fn secondary_physics_weight(
    baseline: f32,
    incapacitation: f32,
    above_pelvis: bool,
) -> f32 {
    let baseline = baseline.clamp(0.0, 1.0);
    if !above_pelvis {
        return baseline;
    }
    baseline + incapacitation.clamp(0.0, 1.0) * (1.0 - baseline)
}

fn impact_affinity(body_part: BodyPart, role: BoneRole) -> f32 {
    match (body_part, role) {
        (BodyPart::Head, BoneRole::Head | BoneRole::NeckOne) => 1.0,
        (
            BodyPart::Chest | BodyPart::Stomach,
            BoneRole::StomachOne
            | BoneRole::StomachTwo
            | BoneRole::StomachThree
            | BoneRole::Chest
            | BoneRole::NeckOne
            | BoneRole::Head,
        ) => 1.0,
        (
            BodyPart::LeftArm,
            BoneRole::ClavicleLeft
            | BoneRole::UpperArmLeft
            | BoneRole::ForearmLeft
            | BoneRole::HandLeft,
        ) => 1.0,
        (
            BodyPart::RightArm,
            BoneRole::ClavicleRight
            | BoneRole::UpperArmRight
            | BoneRole::ForearmRight
            | BoneRole::HandRight,
        ) => 1.0,
        (BodyPart::LeftLeg, BoneRole::ThighLeft | BoneRole::ShinLeft | BoneRole::FootLeft) => 1.0,
        (BodyPart::RightLeg, BoneRole::ThighRight | BoneRole::ShinRight | BoneRole::FootRight) => {
            1.0
        }
        (_, role) if is_above_pelvis(role) => 0.35,
        _ => 0.15,
    }
}

pub(super) fn apply_secondary_bone_physics(
    time: Res<Time>,
    mut telemetry: ResMut<SecondaryPhysicsTelemetry>,
    owners: Query<
        (
            &PresentedSkeleton,
            Option<&TacticalCombatState>,
            &Transform,
            Option<&ImpactReaction>,
        ),
        (With<Player>, Without<HumanoidBone>),
    >,
    mut bones: Query<(&HumanoidBone, &mut Transform, &mut SecondaryBoneDynamics), Without<Player>>,
) {
    *telemetry = SecondaryPhysicsTelemetry::default();
    let mut upper_body_weight_sum = 0.0;
    let delta_seconds = time.delta_secs().clamp(0.0, 1.0 / 30.0);
    for (bone, mut transform, mut dynamics) in &mut bones {
        let Ok((skeleton, combat, owner_transform, impact)) = owners.get(bone.owner) else {
            continue;
        };
        let target = transform.rotation.normalize();
        if !dynamics.initialized || !dynamics.simulated_rotation.is_finite() {
            dynamics.simulated_rotation = target;
            dynamics.angular_velocity = Vec3::ZERO;
            dynamics.initialized = true;
        }

        if let Some(impact) = impact
            && impact.remaining > dynamics.previous_impact_remaining + 1.0e-5
        {
            let local_direction = impact.velocity_change.normalize_or_zero();
            let tumble_axis = Vec3::new(
                local_direction.z,
                -0.2 * local_direction.x,
                -local_direction.x,
            )
            .normalize_or_zero();
            dynamics.angular_velocity += tumble_axis
                * impact.velocity_change.length()
                * impact_affinity(impact.body_part, bone.role)
                * IMPACT_ANGULAR_SPEED_PER_METRE_PER_SECOND;
        }
        dynamics.previous_impact_remaining = impact.map_or(0.0, |impact| impact.remaining);

        if delta_seconds > 0.0 {
            let error = (target * dynamics.simulated_rotation.inverse()).to_scaled_axis();
            let ragdolled = skeleton.body() == BodyState::Ragdolled;
            let motor_frequency = if ragdolled {
                RAGDOLL_MOTOR_FREQUENCY_HZ
            } else {
                MOTOR_FREQUENCY_HZ
            };
            let omega = std::f32::consts::TAU * motor_frequency;
            let mut acceleration = error * omega * omega
                - dynamics.angular_velocity * (2.0 * MOTOR_DAMPING_RATIO * omega);
            if !ragdolled {
                let inertial_acceleration = locomotion_inertial_acceleration(
                    skeleton.world_acceleration,
                    owner_transform.rotation,
                    bone.role,
                );
                telemetry.maximum_inertial_acceleration_radians_per_second_squared = telemetry
                    .maximum_inertial_acceleration_radians_per_second_squared
                    .max(inertial_acceleration.length());
                acceleration += inertial_acceleration;
            }
            if ragdolled && !matches!(bone.role, BoneRole::Root | BoneRole::Pelvis) {
                let gravity_local = owner_transform.rotation.inverse() * Vec3::NEG_Y;
                acceleration +=
                    Vec3::new(gravity_local.z, 0.0, -gravity_local.x) * RAGDOLL_GRAVITY_TORQUE;
            }
            dynamics.angular_velocity = (dynamics.angular_velocity + acceleration * delta_seconds)
                .clamp_length_max(MAX_ANGULAR_SPEED_RADIANS_PER_SECOND);
            dynamics.simulated_rotation =
                (Quat::from_scaled_axis(dynamics.angular_velocity * delta_seconds)
                    * dynamics.simulated_rotation)
                    .normalize();
            let maximum_deviation = maximum_joint_deviation(bone.role, ragdolled);
            let deviation = target.angle_between(dynamics.simulated_rotation);
            if deviation > maximum_deviation {
                dynamics.simulated_rotation =
                    target.slerp(dynamics.simulated_rotation, maximum_deviation / deviation);
                dynamics.angular_velocity *= 0.35;
            }
        }

        let target_weight = secondary_physics_weight(
            baseline_weight(motion_class(skeleton), bone.role),
            combat.map_or(0.0, |combat| combat.incapacitation),
            is_above_pelvis(bone.role),
        );
        let weight_response = 1.0 - (-WEIGHT_RESPONSE_PER_SECOND * delta_seconds).exp();
        dynamics.blend_weight += (target_weight - dynamics.blend_weight) * weight_response;
        transform.rotation = target
            .slerp(dynamics.simulated_rotation, dynamics.blend_weight)
            .normalize();
        if is_above_pelvis(bone.role) {
            telemetry.simulated_upper_body_bones += 1;
            upper_body_weight_sum += dynamics.blend_weight;
            telemetry.maximum_pose_lag_degrees = telemetry
                .maximum_pose_lag_degrees
                .max(target.angle_between(transform.rotation).to_degrees());
        }
    }
    if telemetry.simulated_upper_body_bones > 0 {
        telemetry.mean_upper_body_blend_weight =
            upper_body_weight_sum / telemetry.simulated_upper_body_bones as f32;
    }
}

fn maximum_joint_deviation(role: BoneRole, ragdolled: bool) -> f32 {
    if !ragdolled {
        return 0.65;
    }
    match role {
        BoneRole::StomachOne | BoneRole::StomachTwo | BoneRole::StomachThree | BoneRole::Chest => {
            0.75
        }
        BoneRole::NeckOne | BoneRole::Head => 1.0,
        BoneRole::UpperArmLeft
        | BoneRole::ForearmLeft
        | BoneRole::HandLeft
        | BoneRole::UpperArmRight
        | BoneRole::ForearmRight
        | BoneRole::HandRight => 2.35,
        BoneRole::ThighLeft
        | BoneRole::ShinLeft
        | BoneRole::FootLeft
        | BoneRole::ThighRight
        | BoneRole::ShinRight
        | BoneRole::FootRight => 1.75,
        _ => 0.9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incapacitation_uses_remaining_range_beyond_baseline() {
        assert!((secondary_physics_weight(0.2, 0.01, true) - 0.208).abs() < 1.0e-6);
        assert_eq!(secondary_physics_weight(0.2, 1.0, true), 1.0);
        assert_eq!(secondary_physics_weight(0.2, 1.0, false), 0.2);
    }

    #[test]
    fn running_arms_are_stiffer_than_relaxed_arms() {
        assert!(
            baseline_weight(SecondaryMotionClass::Running, BoneRole::HandLeft)
                < baseline_weight(SecondaryMotionClass::Relaxed, BoneRole::HandLeft)
        );
        assert!(
            baseline_weight(SecondaryMotionClass::Airborne, BoneRole::HandLeft)
                > baseline_weight(SecondaryMotionClass::Relaxed, BoneRole::HandLeft)
        );
    }

    #[test]
    fn locomotion_acceleration_drives_the_whole_upper_body_but_not_the_legs() {
        let world_acceleration = Vec3::new(12.0, 0.0, -8.0);
        for role in [
            BoneRole::StomachOne,
            BoneRole::Chest,
            BoneRole::Head,
            BoneRole::UpperArmLeft,
            BoneRole::HandRight,
        ] {
            assert!(
                locomotion_inertial_acceleration(world_acceleration, Quat::IDENTITY, role).length()
                    > 0.5,
                "{role:?} should receive inertial motor forcing"
            );
        }
        assert_eq!(
            locomotion_inertial_acceleration(
                world_acceleration,
                Quat::IDENTITY,
                BoneRole::ShinLeft,
            ),
            Vec3::ZERO
        );
    }

    #[test]
    fn running_motors_have_visible_upper_body_ownership() {
        assert!(baseline_weight(SecondaryMotionClass::Running, BoneRole::Chest) >= 0.35);
        assert!(baseline_weight(SecondaryMotionClass::Running, BoneRole::ForearmLeft) >= 0.30);
        assert!(baseline_weight(SecondaryMotionClass::Running, BoneRole::Chest) < 0.5);
    }
}
