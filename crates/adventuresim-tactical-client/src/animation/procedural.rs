use std::collections::BTreeMap;

use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

use super::AnimationRigScene;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct HumanoidBone {
    owner: Entity,
    role: BoneRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BoneRole {
    Root,
    Pelvis,
    StomachOne,
    StomachTwo,
    Chest,
    NeckOne,
    NeckTwo,
    Head,
    ClavicleLeft,
    ClavicleRight,
    ThighLeft,
    ThighTwistLeft,
    ShinLeft,
    ShinTwistLeft,
    FootLeft,
    ToeLeft,
    ThighRight,
    ThighTwistRight,
    ShinRight,
    ShinTwistRight,
    FootRight,
    ToeRight,
    UpperArmLeft,
    UpperArmTwistLeft,
    ForearmLeft,
    ForearmTwistLeft,
    HandLeft,
    WeaponLeft,
    UpperArmRight,
    UpperArmTwistRight,
    ForearmRight,
    ForearmTwistRight,
    HandRight,
    WeaponRight,
}

impl BoneRole {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "root" => Self::Root,
            "pelvis" => Self::Pelvis,
            "stomach_01" => Self::StomachOne,
            "stomach_02" => Self::StomachTwo,
            "chest" => Self::Chest,
            "neck_01" => Self::NeckOne,
            "neck_02" => Self::NeckTwo,
            "head" => Self::Head,
            "clavicle.L" => Self::ClavicleLeft,
            "clavicle.R" => Self::ClavicleRight,
            "thigh.L" => Self::ThighLeft,
            "thigh_twist.L" => Self::ThighTwistLeft,
            "shin.L" => Self::ShinLeft,
            "shin_twist.L" => Self::ShinTwistLeft,
            "foot.L" => Self::FootLeft,
            "toe.L" => Self::ToeLeft,
            "thigh.R" => Self::ThighRight,
            "thigh_twist.R" => Self::ThighTwistRight,
            "shin.R" => Self::ShinRight,
            "shin_twist.R" => Self::ShinTwistRight,
            "foot.R" => Self::FootRight,
            "toe.R" => Self::ToeRight,
            "upper_arm.L" => Self::UpperArmLeft,
            "upper_arm_twist.L" => Self::UpperArmTwistLeft,
            "forearm.L" => Self::ForearmLeft,
            "forearm_twist.L" => Self::ForearmTwistLeft,
            "hand.L" => Self::HandLeft,
            "weapon.L" => Self::WeaponLeft,
            "upper_arm.R" => Self::UpperArmRight,
            "upper_arm_twist.R" => Self::UpperArmTwistRight,
            "forearm.R" => Self::ForearmRight,
            "forearm_twist.R" => Self::ForearmTwistRight,
            "hand.R" => Self::HandRight,
            "weapon.R" => Self::WeaponRight,
            _ => return None,
        })
    }
}

pub(super) fn bind_humanoid_bones(
    mut commands: Commands,
    bones: Query<(Entity, &Name), (Added<Name>, Without<HumanoidBone>)>,
    parents: Query<&ChildOf>,
    roots: Query<&AnimationRigScene>,
) {
    for (entity, name) in &bones {
        let Some(role) = BoneRole::from_name(name.as_str()) else {
            continue;
        };
        let mut current = entity;
        for _ in 0..64 {
            if let Ok(root) = roots.get(current) {
                commands.entity(entity).insert(HumanoidBone {
                    owner: root.0,
                    role,
                });
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
    }
}

/// Procedural facing is an additive post-FK layer. Animation evaluation writes
/// these local transforms again on the next frame, so the offsets do not drift.
pub(super) fn apply_head_and_torso_look(
    owners: Query<(&CharacterLook, &SkeletonState)>,
    mut bones: Query<(&HumanoidBone, &mut Transform)>,
) {
    for (bone, mut transform) in &mut bones {
        let Ok((look, skeleton)) = owners.get(bone.owner) else {
            continue;
        };
        let pitch = look.pitch.clamp(-0.75, 0.75);
        let directional_yaw = skeleton.action_direction.x.clamp(-1.0, 1.0) * 0.35;
        match bone.role {
            BoneRole::Head => {
                transform.rotation *=
                    Quat::from_euler(EulerRot::YXZ, directional_yaw, pitch * 0.7, 0.0);
            }
            BoneRole::Chest => {
                transform.rotation *=
                    Quat::from_euler(EulerRot::YXZ, directional_yaw * 0.35, pitch * 0.2, 0.0);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
struct BoneSnapshot {
    entity: Entity,
    global: GlobalTransform,
    parent_rotation: Quat,
}

/// Places the planted foot on the terrain with an analytic two-bone solve,
/// then lowers the hips by the bounded residual. Weapon/hand constraints use
/// the same final-pose seam when authored held-item rigs arrive.
pub(super) fn apply_terrain_leg_ik(
    terrain: Query<&SceneTerrain>,
    owners: Query<&SkeletonState>,
    bones: Query<(Entity, &HumanoidBone)>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let Some(terrain) = terrain.iter().next() else {
        return;
    };
    let mut rigs = BTreeMap::<Entity, BTreeMap<BoneRole, BoneSnapshot>>::new();
    {
        let helper = transforms.p0();
        for (entity, bone) in &bones {
            let Ok(global) = helper.compute_global_transform(entity) else {
                continue;
            };
            rigs.entry(bone.owner).or_default().insert(
                bone.role,
                BoneSnapshot {
                    entity,
                    global,
                    parent_rotation: parents
                        .get(entity)
                        .ok()
                        .and_then(|parent| helper.compute_global_transform(parent.parent()).ok())
                        .map(|global| global.rotation())
                        .unwrap_or(Quat::IDENTITY),
                },
            );
        }
    }

    for (owner, rig) in rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        if !skeleton.grounded || matches!(skeleton.posture, Posture::Prone | Posture::Supine) {
            continue;
        }
        let plant_left = skeleton.gait_phase.rem_euclid(1.0) < 0.5;
        for (upper_role, lower_role, foot_role, planted) in [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                plant_left,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                !plant_left,
            ),
        ] {
            let (Some(upper), Some(lower), Some(foot)) = (
                rig.get(&upper_role),
                rig.get(&lower_role),
                rig.get(&foot_role),
            ) else {
                continue;
            };
            let foot_position = foot.global.translation();
            let Some(height) = terrain.height_at(foot_position.xz()) else {
                continue;
            };
            let weight = if planted { 1.0 } else { 0.35 };
            let correction = (height - foot_position.y).clamp(-0.35, 0.35) * weight;
            if correction.abs() < 0.001 {
                continue;
            }
            let target = foot_position + Vec3::Y * correction;
            solve_and_apply_leg(*upper, *lower, *foot, target, &mut transforms.p1());
        }
    }
}

fn solve_and_apply_leg(
    upper: BoneSnapshot,
    lower: BoneSnapshot,
    foot: BoneSnapshot,
    target: Vec3,
    transforms: &mut Query<&mut Transform>,
) {
    let root = upper.global.translation();
    let knee = lower.global.translation();
    let end = foot.global.translation();
    let upper_length = root.distance(knee);
    let lower_length = knee.distance(end);
    let Some(knee_target) =
        solve_two_bone_knee(root, knee, end, target, upper_length, lower_length)
    else {
        return;
    };

    let current_upper = knee - root;
    let desired_upper = knee_target - root;
    let (Some(current_upper), Some(desired_upper)) =
        (current_upper.try_normalize(), desired_upper.try_normalize())
    else {
        return;
    };
    let upper_delta = Quat::from_rotation_arc(current_upper, desired_upper);
    let upper_world = upper.global.rotation();
    let new_upper_world = upper_delta * upper_world;
    if let Ok(mut transform) = transforms.get_mut(upper.entity) {
        transform.rotation = upper.parent_rotation.inverse() * new_upper_world;
    }

    let current_lower = upper_delta * (end - knee);
    let desired_lower = target - knee_target;
    let (Some(current_lower), Some(desired_lower)) =
        (current_lower.try_normalize(), desired_lower.try_normalize())
    else {
        return;
    };
    let lower_delta = Quat::from_rotation_arc(current_lower, desired_lower);
    let new_lower_world = lower_delta * upper_delta * lower.global.rotation();
    if let Ok(mut transform) = transforms.get_mut(lower.entity) {
        transform.rotation = new_upper_world.inverse() * new_lower_world;
    }
}

fn solve_two_bone_knee(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
) -> Option<Vec3> {
    if upper_length <= f32::EPSILON || lower_length <= f32::EPSILON {
        return None;
    }
    let target_offset = target - root;
    let raw_distance = target_offset.length();
    let target_direction = target_offset.try_normalize()?;
    let distance = raw_distance.clamp(
        (upper_length - lower_length).abs() + 0.0001,
        upper_length + lower_length - 0.0001,
    );
    let along = (upper_length * upper_length - lower_length * lower_length + distance * distance)
        / (2.0 * distance);
    let height = (upper_length * upper_length - along * along)
        .max(0.0)
        .sqrt();
    let current_plane_normal = (current_knee - root).cross(current_end - current_knee);
    let plane_normal = current_plane_normal.try_normalize().unwrap_or(Vec3::X);
    let bend = plane_normal
        .cross(target_direction)
        .try_normalize()
        .unwrap_or(Vec3::Z);
    let sign = if (current_knee - root).dot(bend) < 0.0 {
        -1.0
    } else {
        1.0
    };
    Some(root + target_direction * along + bend * height * sign)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_bone_solver_preserves_segment_lengths_and_reaches_target() {
        let root = Vec3::ZERO;
        let knee = Vec3::new(0.0, -1.0, 0.15);
        let end = Vec3::new(0.0, -2.0, 0.0);
        let target = Vec3::new(0.3, -1.85, 0.0);
        let solved = solve_two_bone_knee(root, knee, end, target, 1.0, 1.0).unwrap();
        assert!((root.distance(solved) - 1.0).abs() < 0.0001);
        assert!((solved.distance(target) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn two_bone_solver_clamps_unreachable_target_without_nan() {
        let solved = solve_two_bone_knee(
            Vec3::ZERO,
            Vec3::new(0.0, -1.0, 0.1),
            Vec3::new(0.0, -2.0, 0.0),
            Vec3::new(0.0, -20.0, 0.0),
            1.0,
            1.0,
        )
        .unwrap();
        assert!(solved.is_finite());
    }
}
