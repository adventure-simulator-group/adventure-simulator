use std::collections::BTreeMap;

use adventuresim_tactical_core::prelude::*;
use bevy::{math::Affine3A, prelude::*};

use super::{AnimationPlayback, AnimationRigScene, AuthoredBindTransform, ImpactReaction};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HumanoidBone {
    pub(crate) owner: Entity,
    pub(crate) role: BoneRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BoneRole {
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

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct SoleUpAxis(Vec3);

/// Captures the foot's bind-space sole normal from the authored global bind
/// transform. The Cascadeur rig's local +Y points ankle-to-toe, so assuming a
/// cardinal local up axis would pitch the feet even on flat terrain.
pub(super) fn capture_humanoid_rig_axes(
    mut commands: Commands,
    feet: Query<(Entity, &HumanoidBone), (Added<HumanoidBone>, Without<SoleUpAxis>)>,
    helper: TransformHelper,
) {
    for (entity, bone) in &feet {
        if !matches!(bone.role, BoneRole::FootLeft | BoneRole::FootRight) {
            continue;
        }
        let Ok(global) = helper.compute_global_transform(entity) else {
            continue;
        };
        let axis = sole_up_axis_from_bind(global.rotation());
        if let Some(axis) = axis.try_normalize() {
            commands.entity(entity).insert(SoleUpAxis(axis));
        }
    }
}

fn sole_up_axis_from_bind(bind_world_rotation: Quat) -> Vec3 {
    bind_world_rotation.inverse() * Vec3::Y
}

fn pole_to_world(owner_rotation: Quat, owner_local_pole: Vec3) -> Vec3 {
    owner_rotation * owner_local_pole
}

fn pole_to_owner(owner_rotation: Quat, world_pole: Vec3) -> Vec3 {
    owner_rotation.inverse() * world_pole
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
        let pitch = look.pitch.clamp(-0.65, 0.65);
        let directional_yaw = skeleton.action_direction.x.clamp(-1.0, 1.0) * 0.35;
        let weight = match bone.role {
            BoneRole::StomachOne => 0.08,
            BoneRole::StomachTwo => 0.12,
            BoneRole::Chest => 0.16,
            BoneRole::NeckOne => 0.18,
            BoneRole::NeckTwo => 0.2,
            BoneRole::Head => 0.26,
            _ => continue,
        };
        // Owner yaw is already on the character transform. Only the bounded
        // local action offset and vertical look are distributed here.
        transform.rotation *=
            Quat::from_euler(EulerRot::YXZ, directional_yaw * weight, pitch * weight, 0.0);
    }
}

/// Constructs the opposite gait half-cycle without mirroring handed upper-body
/// carriage. Gait mirroring transitions only around the authored passing pose,
/// where the legs are nearest one another; interpolating throughout contact
/// folds the planted stride through itself.
pub(super) fn apply_lower_body_mirroring(
    playbacks: Query<&AnimationPlayback>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(
        Query<(Entity, &HumanoidBone, &Transform)>,
        TransformHelper,
        Query<&mut Transform>,
    )>,
) {
    let locals = {
        let bones = transforms.p0();
        bones
            .iter()
            .map(|(entity, bone, transform)| (entity, *bone, *transform))
            .collect::<Vec<_>>()
    };
    let mut rigs = BTreeMap::<Entity, BTreeMap<BoneRole, MirrorBone>>::new();
    let mut owner_globals = BTreeMap::<Entity, GlobalTransform>::new();
    {
        let helper = transforms.p1();
        for (entity, bone, local) in locals {
            let Ok(global) = helper.compute_global_transform(entity) else {
                continue;
            };
            let parent = parents.get(entity).ok().map(ChildOf::parent);
            let parent_global = parent
                .and_then(|parent| helper.compute_global_transform(parent).ok())
                .unwrap_or(GlobalTransform::IDENTITY);
            if !owner_globals.contains_key(&bone.owner)
                && let Ok(owner_global) = helper.compute_global_transform(bone.owner)
            {
                owner_globals.insert(bone.owner, owner_global);
            }
            rigs.entry(bone.owner).or_default().insert(
                bone.role,
                MirrorBone {
                    entity,
                    local,
                    global,
                    parent,
                    parent_global,
                },
            );
        }
    }
    for (owner, rig) in rigs {
        let Ok(playback) = playbacks.get(owner) else {
            continue;
        };
        let weight = playback.lower_body_mirror;
        if weight <= f32::EPSILON {
            continue;
        }
        let Some(owner_global) = owner_globals.get(&owner) else {
            continue;
        };
        let mut desired_globals = BTreeMap::<Entity, Affine3A>::new();
        for (left_role, right_role) in [
            (BoneRole::ThighLeft, BoneRole::ThighRight),
            (BoneRole::ThighTwistLeft, BoneRole::ThighTwistRight),
            (BoneRole::ShinLeft, BoneRole::ShinRight),
            (BoneRole::ShinTwistLeft, BoneRole::ShinTwistRight),
            (BoneRole::FootLeft, BoneRole::FootRight),
            (BoneRole::ToeLeft, BoneRole::ToeRight),
        ] {
            let (Some(left), Some(right)) = (rig.get(&left_role), rig.get(&right_role)) else {
                continue;
            };
            desired_globals.insert(
                left.entity,
                mirrored_global_affine(right.global, *owner_global),
            );
            desired_globals.insert(
                right.entity,
                mirrored_global_affine(left.global, *owner_global),
            );
        }
        let mut bones = transforms.p2();
        for bone in rig.values() {
            let Some(&desired_global) = desired_globals.get(&bone.entity) else {
                continue;
            };
            let desired_parent = bone
                .parent
                .and_then(|parent| desired_globals.get(&parent).copied())
                .unwrap_or_else(|| bone.parent_global.affine());
            let (scale, rotation, translation) =
                (desired_parent.inverse() * desired_global).to_scale_rotation_translation();
            if !translation.is_finite() || !rotation.is_finite() || !scale.is_finite() {
                continue;
            }
            if let Ok(mut transform) = bones.get_mut(bone.entity) {
                transform.translation = bone.local.translation.lerp(translation, weight);
                transform.rotation = bone.local.rotation.slerp(rotation, weight);
                transform.scale = bone.local.scale.lerp(scale, weight);
            }
        }
    }
}

/// Removes exporter-scale torso excursions from ordinary locomotion while
/// retaining a small authored weight shift. Gameplay root travel remains on
/// the owner entity, and look is applied after this bounded pass.
pub(super) fn stabilize_locomotion_torso(
    owners: Query<&SkeletonState>,
    mut bones: Query<(&HumanoidBone, &AuthoredBindTransform, &mut Transform)>,
) {
    for (bone, bind, mut transform) in &mut bones {
        let Ok(skeleton) = owners.get(bone.owner) else {
            continue;
        };
        if !skeleton.grounded
            || skeleton.action != SkeletonAction::None
            || skeleton.local_velocity.xz().length() <= 0.05
            || !matches!(skeleton.posture, Posture::Upright | Posture::Crouched)
        {
            continue;
        }
        let (translation_limit, rotation_limit) = match bone.role {
            BoneRole::Root => (Vec3::new(0.02, 0.02, 0.025), 6.0_f32.to_radians()),
            BoneRole::Pelvis => (Vec3::new(0.035, 0.04, 0.045), 10.0_f32.to_radians()),
            BoneRole::StomachOne | BoneRole::StomachTwo | BoneRole::Chest => {
                (Vec3::splat(0.012), 12.0_f32.to_radians())
            }
            BoneRole::NeckOne | BoneRole::NeckTwo | BoneRole::Head => {
                (Vec3::splat(0.008), 10.0_f32.to_radians())
            }
            _ => continue,
        };
        transform.translation = bind.local.translation
            + (transform.translation - bind.local.translation)
                .clamp(-translation_limit, translation_limit);
        transform.rotation =
            clamp_rotation_from_bind(bind.local.rotation, transform.rotation, rotation_limit);
        if bone.role == BoneRole::Root {
            let speed = skeleton.local_velocity.xz().length();
            let run = locomotion_run_weight(speed);
            let flight = (skeleton.gait_phase.rem_euclid(1.0) * std::f32::consts::TAU)
                .sin()
                .abs();
            transform.translation.y += 0.085 * run * flight;
        }
    }
}

fn clamp_rotation_from_bind(bind: Quat, animated: Quat, maximum_angle: f32) -> Quat {
    let delta = (bind.inverse() * animated).normalize();
    let angle = Quat::IDENTITY.angle_between(delta);
    if angle <= maximum_angle || angle <= f32::EPSILON {
        animated.normalize()
    } else {
        (bind * Quat::IDENTITY.slerp(delta, maximum_angle / angle)).normalize()
    }
}

#[derive(Clone, Copy)]
struct MirrorBone {
    entity: Entity,
    local: Transform,
    global: GlobalTransform,
    parent: Option<Entity>,
    parent_global: GlobalTransform,
}

fn mirrored_global_affine(source: GlobalTransform, owner: GlobalTransform) -> Affine3A {
    let owner_affine = owner.affine();
    let relative = owner_affine.inverse() * source.affine();
    let (scale, rotation, translation) = relative.to_scale_rotation_translation();
    let mirrored = mirrored_across_anatomical_center(Transform {
        translation,
        rotation,
        scale,
    });
    owner_affine * mirrored.compute_affine()
}

fn mirrored_across_anatomical_center(mut transform: Transform) -> Transform {
    transform.translation.x = -transform.translation.x;
    let rotation = transform.rotation;
    transform.rotation = Quat::from_xyzw(rotation.x, -rotation.y, -rotation.z, rotation.w);
    transform
}

pub(super) fn apply_impact_reaction(
    reactions: Query<&ImpactReaction>,
    mut bones: Query<(&HumanoidBone, &mut Transform)>,
) {
    for (bone, mut transform) in &mut bones {
        let Ok(reaction) = reactions.get(bone.owner) else {
            continue;
        };
        if !matches!(
            bone.role,
            BoneRole::Chest | BoneRole::NeckTwo | BoneRole::Head
        ) {
            continue;
        }
        let progress = 1.0 - (reaction.remaining / reaction.duration).clamp(0.0, 1.0);
        let pulse = (progress * std::f32::consts::PI).sin() * reaction.strength;
        let scale = if bone.role == BoneRole::Head {
            0.12
        } else {
            0.2
        };
        transform.rotation *= Quat::from_rotation_x(-pulse * scale);
    }
}

#[derive(Clone, Copy, Debug)]
struct BoneSnapshot {
    entity: Entity,
    global: GlobalTransform,
    parent_rotation: Quat,
}

#[derive(Debug, Clone, Copy, Default)]
struct PoleMemory {
    left_leg: Option<Vec3>,
    right_leg: Option<Vec3>,
    left_foot_plant: Option<Vec3>,
    right_foot_plant: Option<Vec3>,
    left_arm: Option<Vec3>,
    right_arm: Option<Vec3>,
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub(super) struct ProceduralIkState(PoleMemory);

/// Client-only world-space target for a hand. It is presentation data and is
/// deliberately absent from replicated `SkeletonState`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HandIkTarget {
    pub translation: Vec3,
    pub rotation: Option<Quat>,
    pub weight: f32,
}

/// Optional client-only direct hand targets.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct HumanoidIkTargets {
    pub left: Option<HandIkTarget>,
    pub right: Option<HandIkTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandSide {
    Left,
    Right,
}

/// Constrains a client-side held item to an authored weapon socket. The
/// optional point is in weapon-local space and becomes an off-hand IK target.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct HeldWeaponConstraint {
    pub owner: Entity,
    pub primary_hand: HandSide,
    pub secondary_grip_local: Option<Vec3>,
}

/// Places the planted foot on the terrain with an analytic two-bone solve,
/// then lowers the hips by the bounded residual. Weapon/hand constraints use
/// the same final-pose seam when authored held-item rigs arrive.
pub(super) fn apply_terrain_leg_ik(
    enabled: Res<super::TerrainIkEnabled>,
    terrain: Query<&SceneTerrain>,
    owners: Query<&SkeletonState>,
    bones: Query<(Entity, &HumanoidBone, Option<&SoleUpAxis>)>,
    parents: Query<&ChildOf>,
    mut ik_states: Query<&mut ProceduralIkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    if !enabled.0 {
        // Discard plant targets while disabled so re-enabling IK starts from
        // the current authored pose instead of snapping to a stale footprint.
        for mut state in &mut ik_states {
            state.0 = PoleMemory::default();
        }
        return;
    }
    let Some(terrain) = terrain.iter().next() else {
        return;
    };
    let mut rigs = BTreeMap::<Entity, BTreeMap<BoneRole, Entity>>::new();
    let mut sole_axes = BTreeMap::<Entity, Vec3>::new();
    {
        for (entity, bone, sole_axis) in &bones {
            rigs.entry(bone.owner)
                .or_default()
                .insert(bone.role, entity);
            if let Some(axis) = sole_axis {
                sole_axes.insert(entity, axis.0);
            }
        }
    }

    for (owner, rig) in rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        if !skeleton.grounded || matches!(skeleton.posture, Posture::Prone | Posture::Supine) {
            continue;
        }
        let phase = skeleton.gait_phase.rem_euclid(1.0);
        let ground_speed = skeleton.local_velocity.xz().length();
        let (left_weight, right_weight) = gait_support_weights(phase, ground_speed);
        let legs = [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                left_weight,
                Vec3::Z,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                right_weight,
                Vec3::Z,
                false,
            ),
        ];
        let mut hip_shift = 0.0_f32;
        for (_, _, foot_role, weight, _, _) in legs {
            let Some(&foot) = rig.get(&foot_role) else {
                continue;
            };
            let Ok(global) = transforms.p0().compute_global_transform(foot) else {
                continue;
            };
            let position = global.translation();
            if let Some(height) = terrain.height_at(position.xz()) {
                let desired_ankle = height + ankle_sole_offset(global.rotation());
                hip_shift =
                    hip_shift.min(((desired_ankle - position.y) * weight).clamp(-0.18, 0.0));
            }
        }
        if hip_shift < -0.001
            && let Some(&pelvis) = rig.get(&BoneRole::Pelvis)
        {
            let local_delta = parents
                .get(pelvis)
                .ok()
                .and_then(|parent| {
                    transforms
                        .p0()
                        .compute_global_transform(parent.parent())
                        .ok()
                })
                .map(|parent| {
                    parent
                        .affine()
                        .inverse()
                        .transform_vector3(Vec3::Y * hip_shift)
                })
                .unwrap_or(Vec3::Y * hip_shift);
            if local_delta.is_finite()
                && let Ok(mut transform) = transforms.p1().get_mut(pelvis)
            {
                transform.translation += local_delta;
            }
        }
        let owner_rotation = transforms
            .p0()
            .compute_global_transform(owner)
            .map(|global| global.rotation())
            .unwrap_or(Quat::IDENTITY);
        let mut memory = ik_states
            .get_mut(owner)
            .map(|state| state.0)
            .unwrap_or_default();
        for (upper_role, lower_role, foot_role, weight, owner_pole, left) in legs {
            let (Some(&upper), Some(&lower), Some(&foot)) = (
                rig.get(&upper_role),
                rig.get(&lower_role),
                rig.get(&foot_role),
            ) else {
                continue;
            };
            let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
                snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
            else {
                continue;
            };
            let foot_position = foot_snapshot.global.translation();
            let plant = if left {
                &mut memory.left_foot_plant
            } else {
                &mut memory.right_foot_plant
            };
            if weight <= 0.05
                || plant.is_some_and(|position| position.distance(foot_position) > 0.75)
            {
                *plant = None;
            }
            if weight >= 0.55 && plant.is_none() {
                *plant = Some(foot_position);
            }
            let horizontal_target = plant.unwrap_or(foot_position);
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            let desired_y = height + ankle_sole_offset(foot_snapshot.global.rotation());
            let planted_target = Vec3::new(horizontal_target.x, desired_y, horizontal_target.z);
            let target = foot_position.lerp(planted_target, weight);
            let remembered = if left {
                memory.left_leg
            } else {
                memory.right_leg
            };
            let pole = pole_to_world(owner_rotation, remembered.unwrap_or(owner_pole));
            if let Some(solution) = solve_two_bone(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                foot_position,
                target,
                upper_snapshot
                    .global
                    .translation()
                    .distance(lower_snapshot.global.translation()),
                lower_snapshot.global.translation().distance(foot_position),
                pole,
            ) {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
                let bend = (solution.knee - upper_snapshot.global.translation())
                    .reject_from_normalized(solution.end_direction);
                if let Some(valid) = bend.try_normalize() {
                    if left {
                        memory.left_leg = Some(pole_to_owner(owner_rotation, valid));
                    } else {
                        memory.right_leg = Some(pole_to_owner(owner_rotation, valid));
                    }
                }
            }
            if weight > 0.001
                && let Some(normal) = terrain.normal_at(horizontal_target.xz())
                && let Some(&sole_axis) = sole_axes.get(&foot)
            {
                align_foot_to_slope(foot, sole_axis, normal, weight, &parents, &mut transforms);
            }
        }
        if let Ok(mut state) = ik_states.get_mut(owner) {
            state.0 = memory;
        } else {
            commands.entity(owner).insert(ProceduralIkState(memory));
        }
    }
}

fn locomotion_run_weight(speed: f32) -> f32 {
    ((speed - 2.0) / (5.5 - 2.0)).clamp(0.0, 1.0)
}

pub(crate) fn gait_support_weights(phase: f32, speed: f32) -> (f32, f32) {
    let run_weight = locomotion_run_weight(speed);
    let locomotion = smoothstep(0.0, 0.75, speed);
    let gait = |phase| foot_support_weight(phase, true, run_weight);
    (
        1.0_f32.lerp(gait(phase.rem_euclid(1.0)), locomotion),
        1.0_f32.lerp(gait((phase + 0.5).rem_euclid(1.0)), locomotion),
    )
}

fn foot_support_weight(phase: f32, moving: bool, run_weight: f32) -> f32 {
    if !moving {
        return 1.0;
    }
    let run_weight = run_weight.clamp(0.0, 1.0);
    // The canonical leg owns support from contact through passing, then hands
    // off to its mirrored counterpart. This keeps the passing swing foot free
    // instead of constraining both feet to a misleading 0.6 weight.
    let walk_support = (1.0 - smoothstep(0.38, 0.50, phase)).max(smoothstep(0.88, 1.0, phase));

    // A running foot supports only a compact interval around its contact.
    // In particular, both legs must be free during the quarter-cycle flight
    // phases; forcing a minimum IK weight there turns the authored swing into
    // a visible kick/pop.
    let distance_to_contact = phase.min(1.0 - phase);
    let run_support = 1.0 - smoothstep(0.08, 0.20, distance_to_contact);
    walk_support.lerp(run_support, run_weight).clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn ankle_sole_offset(_rotation: Quat) -> f32 {
    0.085
}

#[derive(Debug, Clone, Copy)]
struct TwoBoneSolution {
    knee: Vec3,
    end: Vec3,
    end_direction: Vec3,
}

fn solve_two_bone(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
) -> Option<TwoBoneSolution> {
    if !root.is_finite() || !target.is_finite() || upper_length <= 0.0001 || lower_length <= 0.0001
    {
        return None;
    }
    let target_offset = target - root;
    let target_direction = target_offset
        .try_normalize()
        .or_else(|| (current_end - root).try_normalize())
        .unwrap_or(Vec3::NEG_Y);
    let distance = target_offset.length().clamp(
        (upper_length - lower_length).abs() + 0.0001,
        upper_length + lower_length - 0.0001,
    );
    let end = root + target_direction * distance;
    let along = (upper_length * upper_length - lower_length * lower_length + distance * distance)
        / (2.0 * distance);
    let height = (upper_length * upper_length - along * along)
        .max(0.0)
        .sqrt();
    let bend = (current_knee - root)
        .reject_from_normalized(target_direction)
        .try_normalize()
        .or_else(|| {
            pole_direction
                .reject_from_normalized(target_direction)
                .try_normalize()
        })
        .or_else(|| target_direction.any_orthonormal_vector().try_normalize())?;
    let knee = root + target_direction * along + bend * height;
    (knee.is_finite() && end.is_finite()).then_some(TwoBoneSolution {
        knee,
        end,
        end_direction: target_direction,
    })
}

fn snapshot(
    entity: Entity,
    parents: &Query<&ChildOf>,
    helper: &TransformHelper,
) -> Option<BoneSnapshot> {
    let global = helper.compute_global_transform(entity).ok()?;
    let parent_rotation = parents
        .get(entity)
        .ok()
        .and_then(|parent| helper.compute_global_transform(parent.parent()).ok())
        .map(|global| global.rotation())
        .unwrap_or(Quat::IDENTITY);
    Some(BoneSnapshot {
        entity,
        global,
        parent_rotation,
    })
}

fn snapshot_chain(
    upper: Entity,
    lower: Entity,
    end: Entity,
    parents: &Query<&ChildOf>,
    helper: &TransformHelper,
) -> Option<(BoneSnapshot, BoneSnapshot, BoneSnapshot)> {
    Some((
        snapshot(upper, parents, helper)?,
        snapshot(lower, parents, helper)?,
        snapshot(end, parents, helper)?,
    ))
}

fn aim_world_rotation(current: BoneSnapshot, from: Vec3, to: Vec3) -> Option<Quat> {
    let from = from.try_normalize()?;
    let to = to.try_normalize()?;
    let world = Quat::from_rotation_arc(from, to) * current.global.rotation();
    let local = current.parent_rotation.inverse() * world;
    local.is_finite().then_some(local.normalize())
}

fn apply_two_bone_solution(
    upper: Entity,
    lower: Entity,
    end: Entity,
    solution: TwoBoneSolution,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let Some((upper_before, lower_before, _)) =
        snapshot_chain(upper, lower, end, parents, &transforms.p0())
    else {
        return;
    };
    let Some(rotation) = aim_world_rotation(
        upper_before,
        lower_before.global.translation() - upper_before.global.translation(),
        solution.knee - upper_before.global.translation(),
    ) else {
        return;
    };
    if let Ok(mut transform) = transforms.p1().get_mut(upper_before.entity) {
        transform.rotation = rotation;
    }

    // Recompute through the actual twist hierarchy after rotating the major
    // upper bone. The twist local transforms remain untouched.
    let Some((_, lower_after, end_after)) =
        snapshot_chain(upper, lower, end, parents, &transforms.p0())
    else {
        return;
    };
    let Some(rotation) = aim_world_rotation(
        lower_after,
        end_after.global.translation() - lower_after.global.translation(),
        solution.end - solution.knee,
    ) else {
        return;
    };
    if let Ok(mut transform) = transforms.p1().get_mut(lower_after.entity) {
        transform.rotation = rotation;
    }
}

fn align_foot_to_slope(
    foot: Entity,
    sole_up_local: Vec3,
    normal: Vec3,
    weight: f32,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let Some(snapshot) = snapshot(foot, parents, &transforms.p0()) else {
        return;
    };
    let Some(normal) = normal.try_normalize() else {
        return;
    };
    let current_up = snapshot.global.rotation() * sole_up_local;
    let angle = current_up.angle_between(normal).min(28.0_f32.to_radians()) * weight;
    let axis = current_up.cross(normal).try_normalize();
    let Some(axis) = axis else { return };
    let world = Quat::from_axis_angle(axis, angle) * snapshot.global.rotation();
    let local = snapshot.parent_rotation.inverse() * world;
    if local.is_finite()
        && let Ok(mut transform) = transforms.p1().get_mut(foot)
    {
        transform.rotation = local.normalize();
    }
}

/// Applies optional client-only hand targets and held-item constraints. Missing
/// targets, sockets, or arm bones are intentionally inert.
pub(super) fn apply_arm_and_weapon_constraints(
    bones: Query<(Entity, &HumanoidBone)>,
    parents: Query<&ChildOf>,
    targets: Query<&HumanoidIkTargets>,
    mut ik_states: Query<&mut ProceduralIkState>,
    weapon_constraints: Query<(Entity, &HeldWeaponConstraint)>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let mut rigs = BTreeMap::<Entity, BTreeMap<BoneRole, Entity>>::new();
    for (entity, bone) in &bones {
        rigs.entry(bone.owner)
            .or_default()
            .insert(bone.role, entity);
    }

    // Move an explicitly targeted primary hand first. The socket is a child of
    // that hand, so the weapon placement below observes the same-frame result.
    for (_, constraint) in &weapon_constraints {
        let Some(rig) = rigs.get(&constraint.owner) else {
            continue;
        };
        let explicit = targets.get(constraint.owner).copied().unwrap_or_default();
        let (target, roles, left) = match constraint.primary_hand {
            HandSide::Left => (
                explicit.left,
                (
                    BoneRole::UpperArmLeft,
                    BoneRole::ForearmLeft,
                    BoneRole::HandLeft,
                ),
                true,
            ),
            HandSide::Right => (
                explicit.right,
                (
                    BoneRole::UpperArmRight,
                    BoneRole::ForearmRight,
                    BoneRole::HandRight,
                ),
                false,
            ),
        };
        let Some(target) = target else { continue };
        let mut memory = ik_states
            .get_mut(constraint.owner)
            .map(|state| state.0)
            .unwrap_or_default();
        apply_hand_target(
            constraint.owner,
            rig,
            roles,
            target,
            left,
            &mut memory,
            &parents,
            &mut transforms,
        );
        if let Ok(mut state) = ik_states.get_mut(constraint.owner) {
            state.0 = memory;
        }
    }

    let mut derived_targets = BTreeMap::<Entity, HumanoidIkTargets>::new();
    for (weapon, constraint) in &weapon_constraints {
        let Some(rig) = rigs.get(&constraint.owner) else {
            continue;
        };
        let socket_role = match constraint.primary_hand {
            HandSide::Left => BoneRole::WeaponLeft,
            HandSide::Right => BoneRole::WeaponRight,
        };
        let Some(&socket) = rig.get(&socket_role) else {
            continue;
        };
        let Ok(socket_global) = transforms.p0().compute_global_transform(socket) else {
            continue;
        };
        set_world_transform(
            weapon,
            socket_global.compute_transform(),
            &parents,
            &mut transforms,
        );
        if let Some(grip) = constraint.secondary_grip_local {
            let Ok(weapon_global) = transforms.p0().compute_global_transform(weapon) else {
                continue;
            };
            let target = HandIkTarget {
                translation: secondary_grip_world(weapon_global, grip),
                rotation: None,
                weight: 1.0,
            };
            let entry = derived_targets.entry(constraint.owner).or_default();
            match constraint.primary_hand {
                HandSide::Left => entry.right = Some(target),
                HandSide::Right => entry.left = Some(target),
            }
        }
    }

    for (owner, rig) in rigs {
        let explicit = targets.get(owner).copied().unwrap_or_default();
        let derived = derived_targets.get(&owner).copied().unwrap_or_default();
        let combined = HumanoidIkTargets {
            left: derived.left.or(explicit.left),
            right: derived.right.or(explicit.right),
        };
        let mut memory = ik_states
            .get_mut(owner)
            .map(|state| state.0)
            .unwrap_or_default();
        for (upper_role, lower_role, hand_role, target, left) in [
            (
                BoneRole::UpperArmLeft,
                BoneRole::ForearmLeft,
                BoneRole::HandLeft,
                combined.left,
                true,
            ),
            (
                BoneRole::UpperArmRight,
                BoneRole::ForearmRight,
                BoneRole::HandRight,
                combined.right,
                false,
            ),
        ] {
            let Some(target) = target else { continue };
            apply_hand_target(
                owner,
                &rig,
                (upper_role, lower_role, hand_role),
                target,
                left,
                &mut memory,
                &parents,
                &mut transforms,
            );
        }
        if let Ok(mut state) = ik_states.get_mut(owner) {
            state.0 = memory;
        } else {
            commands.entity(owner).insert(ProceduralIkState(memory));
        }
    }
}

fn apply_hand_target(
    owner: Entity,
    rig: &BTreeMap<BoneRole, Entity>,
    (upper_role, lower_role, hand_role): (BoneRole, BoneRole, BoneRole),
    target: HandIkTarget,
    left: bool,
    memory: &mut PoleMemory,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let weight = target.weight.clamp(0.0, 1.0);
    if weight <= f32::EPSILON {
        return;
    }
    let (Some(&upper), Some(&lower), Some(&hand)) = (
        rig.get(&upper_role),
        rig.get(&lower_role),
        rig.get(&hand_role),
    ) else {
        return;
    };
    let Some((upper_snapshot, lower_snapshot, hand_snapshot)) =
        snapshot_chain(upper, lower, hand, parents, &transforms.p0())
    else {
        return;
    };
    let blended_target = hand_snapshot
        .global
        .translation()
        .lerp(target.translation, weight);
    let remembered = if left {
        memory.left_arm
    } else {
        memory.right_arm
    };
    let owner_rotation = transforms
        .p0()
        .compute_global_transform(owner)
        .map(|global| global.rotation())
        .unwrap_or(Quat::IDENTITY);
    if let Some(solution) = solve_two_bone(
        upper_snapshot.global.translation(),
        lower_snapshot.global.translation(),
        hand_snapshot.global.translation(),
        blended_target,
        upper_snapshot
            .global
            .translation()
            .distance(lower_snapshot.global.translation()),
        lower_snapshot
            .global
            .translation()
            .distance(hand_snapshot.global.translation()),
        pole_to_world(owner_rotation, remembered.unwrap_or(Vec3::NEG_Y)),
    ) {
        apply_two_bone_solution(upper, lower, hand, solution, parents, transforms);
        let bend = (solution.knee - upper_snapshot.global.translation())
            .reject_from_normalized(solution.end_direction);
        if let Some(valid) = bend.try_normalize() {
            if left {
                memory.left_arm = Some(pole_to_owner(owner_rotation, valid));
            } else {
                memory.right_arm = Some(pole_to_owner(owner_rotation, valid));
            }
        }
        if let Some(rotation) = target.rotation {
            set_bone_world_rotation(hand, rotation, parents, transforms);
        }
    }
}

fn secondary_grip_world(weapon: GlobalTransform, local_grip: Vec3) -> Vec3 {
    weapon.transform_point(local_grip)
}

fn set_bone_world_rotation(
    entity: Entity,
    world_rotation: Quat,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let parent_rotation = parents
        .get(entity)
        .ok()
        .and_then(|parent| {
            transforms
                .p0()
                .compute_global_transform(parent.parent())
                .ok()
        })
        .map(|global| global.rotation())
        .unwrap_or(Quat::IDENTITY);
    let local = parent_rotation.inverse() * world_rotation;
    if local.is_finite()
        && let Ok(mut transform) = transforms.p1().get_mut(entity)
    {
        transform.rotation = local.normalize();
    }
}

fn set_world_transform(
    entity: Entity,
    world: Transform,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let local = parents
        .get(entity)
        .ok()
        .and_then(|parent| {
            transforms
                .p0()
                .compute_global_transform(parent.parent())
                .ok()
        })
        .map(|parent| GlobalTransform::from(world).reparented_to(&parent))
        .unwrap_or(world);
    if local.translation.is_finite()
        && local.rotation.is_finite()
        && let Ok(mut transform) = transforms.p1().get_mut(entity)
    {
        *transform = local;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_test_two_bone(
        In((upper, lower, end, solution)): In<(Entity, Entity, Entity, TwoBoneSolution)>,
        parents: Query<&ChildOf>,
        mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    ) {
        apply_two_bone_solution(upper, lower, end, solution, &parents, &mut transforms);
    }

    fn test_joint_positions(
        In((lower, end)): In<(Entity, Entity)>,
        helper: TransformHelper,
    ) -> (Vec3, Vec3) {
        (
            helper
                .compute_global_transform(lower)
                .unwrap()
                .translation(),
            helper.compute_global_transform(end).unwrap().translation(),
        )
    }

    #[test]
    fn two_bone_solver_preserves_segment_lengths_and_reaches_target() {
        let root = Vec3::ZERO;
        let knee = Vec3::new(0.0, -1.0, 0.15);
        let end = Vec3::new(0.0, -2.0, 0.0);
        let target = Vec3::new(0.3, -1.85, 0.0);
        let solved = solve_two_bone(root, knee, end, target, 1.0, 1.0, Vec3::NEG_Z).unwrap();
        assert!((root.distance(solved.knee) - 1.0).abs() < 0.0001);
        assert!((solved.knee.distance(solved.end) - 1.0).abs() < 0.0001);
        assert!(solved.end.abs_diff_eq(target, 0.0001));
    }

    #[test]
    fn two_bone_solver_clamps_unreachable_target_without_nan() {
        let solved = solve_two_bone(
            Vec3::ZERO,
            Vec3::new(0.0, -1.0, 0.1),
            Vec3::new(0.0, -2.0, 0.0),
            Vec3::new(0.0, -20.0, 0.0),
            1.0,
            1.0,
            Vec3::NEG_Z,
        )
        .unwrap();
        assert!(solved.knee.is_finite() && solved.end.is_finite());
        assert!(solved.end.length() < 2.0);
    }

    #[test]
    fn straight_chain_uses_rig_bind_space_knee_pole() {
        let solved = solve_two_bone(
            Vec3::ZERO,
            Vec3::NEG_Y,
            Vec3::NEG_Y * 2.0,
            Vec3::new(0.0, -1.8, 0.0),
            1.0,
            1.0,
            Vec3::Z,
        )
        .unwrap();
        assert!(solved.knee.z > 0.0);
        assert!(solved.knee.is_finite());
    }

    #[test]
    fn authored_knee_bend_wins_over_fallback_pole() {
        let solved = solve_two_bone(
            Vec3::ZERO,
            Vec3::new(0.0, -1.0, 0.1),
            Vec3::NEG_Y * 2.0,
            Vec3::new(0.0, -1.8, 0.0),
            1.0,
            1.0,
            Vec3::NEG_Z,
        )
        .unwrap();
        assert!(solved.knee.z > 0.0);
    }

    #[test]
    fn actual_twist_hierarchy_names_are_recognized() {
        for name in [
            "pelvis",
            "stomach_01",
            "neck_02",
            "thigh_twist.L",
            "shin_twist.R",
            "forearm_twist.L",
            "weapon.R",
            "toe.L",
        ] {
            assert!(BoneRole::from_name(name).is_some(), "missing {name}");
        }
        assert_eq!(BoneRole::from_name("weapon"), None);
        assert_eq!(BoneRole::from_name("Cylinder"), None);
    }

    #[test]
    fn idle_plants_both_feet_and_gait_weight_is_continuous() {
        assert_eq!(foot_support_weight(0.25, false, 1.0), 1.0);
        assert_eq!(foot_support_weight(0.75, false, 1.0), 1.0);
        let before = foot_support_weight(0.5 - 0.0001, true, 0.0);
        let after = foot_support_weight(0.5 + 0.0001, true, 0.0);
        assert!((before - after).abs() < 0.001);
    }

    #[test]
    fn run_has_unconstrained_flight_but_walk_retains_support() {
        for phase in [0.25, 0.75] {
            assert_eq!(foot_support_weight(phase, true, 1.0), 0.0);
            let (left, right) = gait_support_weights(phase, 2.0);
            assert!((left.max(right) - 1.0).abs() < 0.0001);
            assert!(left.min(right) <= 0.0001);
        }
        assert_eq!(foot_support_weight(0.0, true, 1.0), 1.0);
        assert_eq!(locomotion_run_weight(2.0), 0.0);
        assert_eq!(locomotion_run_weight(5.5), 1.0);
    }

    #[test]
    fn cascadeur_foot_bind_axis_keeps_flat_ground_unchanged() {
        // Actual left-foot global bind rotation from assets_src/base.glb.
        let bind = Quat::from_xyzw(0.8856122, 0.00000032, 0.00000032, 0.46442544).normalize();
        let sole_up = sole_up_axis_from_bind(bind).normalize();
        assert!(sole_up.y < -0.5 && sole_up.z < -0.8);
        assert!((bind * sole_up).abs_diff_eq(Vec3::Y, 0.0001));
        assert!((bind * Vec3::Y).dot(Vec3::Y) < 0.0);
    }

    #[test]
    fn remembered_pole_follows_owner_yaw() {
        let original_yaw = Quat::from_rotation_y(0.3);
        let owner_local = Vec3::new(0.2, -0.1, -0.97).normalize();
        let saved = pole_to_owner(original_yaw, pole_to_world(original_yaw, owner_local));
        assert!(saved.abs_diff_eq(owner_local, 0.0001));
        let new_yaw = Quat::from_rotation_y(1.4);
        let expected = new_yaw * owner_local;
        assert!(pole_to_world(new_yaw, saved).abs_diff_eq(expected, 0.0001));
    }

    #[test]
    fn secondary_grip_uses_final_weapon_transform() {
        let before = GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0));
        let after = GlobalTransform::from(
            Transform::from_xyz(2.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(0.5)),
        );
        let grip = Vec3::new(0.0, 0.0, 0.5);
        assert_ne!(
            secondary_grip_world(before, grip),
            secondary_grip_world(after, grip)
        );
        assert!(secondary_grip_world(after, grip).abs_diff_eq(after.transform_point(grip), 0.0001));
    }

    #[test]
    fn lower_joint_solves_through_twist_intermediate_parent() {
        let mut world = World::new();
        let upper = world.spawn(Transform::default()).id();
        let upper_twist = world.spawn(Transform::from_xyz(0.0, -0.5, 0.0)).id();
        let lower = world.spawn(Transform::from_xyz(0.0, -0.5, 0.0)).id();
        let lower_twist = world.spawn(Transform::from_xyz(0.0, -0.5, 0.0)).id();
        let end = world.spawn(Transform::from_xyz(0.0, -0.5, 0.0)).id();
        world.entity_mut(upper).add_child(upper_twist);
        world.entity_mut(upper_twist).add_child(lower);
        world.entity_mut(lower).add_child(lower_twist);
        world.entity_mut(lower_twist).add_child(end);
        let upper_twist_bind = *world.get::<Transform>(upper_twist).unwrap();
        let lower_twist_bind = *world.get::<Transform>(lower_twist).unwrap();
        let solution = solve_two_bone(
            Vec3::ZERO,
            Vec3::NEG_Y,
            Vec3::NEG_Y * 2.0,
            Vec3::new(0.45, -1.75, 0.0),
            1.0,
            1.0,
            Vec3::NEG_Z,
        )
        .unwrap();
        world
            .run_system_cached_with(apply_test_two_bone, (upper, lower, end, solution))
            .unwrap();
        let (knee, ankle) = world
            .run_system_cached_with(test_joint_positions, (lower, end))
            .unwrap();
        assert!(knee.abs_diff_eq(solution.knee, 0.0002));
        assert!(ankle.abs_diff_eq(solution.end, 0.0002));
        assert_eq!(
            *world.get::<Transform>(upper_twist).unwrap(),
            upper_twist_bind
        );
        assert_eq!(
            *world.get::<Transform>(lower_twist).unwrap(),
            lower_twist_bind
        );
    }

    #[test]
    fn lower_body_mirror_is_an_involution() {
        let original = Transform::from_xyz(0.3, -0.8, 0.2).with_rotation(Quat::from_euler(
            EulerRot::XYZ,
            0.2,
            -0.3,
            0.4,
        ));
        let twice = mirrored_across_anatomical_center(mirrored_across_anatomical_center(original));
        assert!(twice.translation.abs_diff_eq(original.translation, 0.0001));
        assert!(twice.rotation.abs_diff_eq(original.rotation, 0.0001));
    }

    #[test]
    fn anatomical_side_does_not_collapse_during_mirror_blend() {
        let left = Transform::from_xyz(0.18, -0.1, 0.25);
        let right = Transform::from_xyz(-0.18, -0.1, -0.25);
        let mirrored_right = mirrored_across_anatomical_center(right);
        let midpoint_x = left.translation.lerp(mirrored_right.translation, 0.5).x;
        assert!((midpoint_x - 0.18).abs() < 0.0001);
    }

    #[test]
    fn locomotion_rotation_clamp_stays_near_bind() {
        let bind = Quat::from_rotation_y(0.2);
        let animated = bind * Quat::from_rotation_x(1.0);
        let bounded = clamp_rotation_from_bind(bind, animated, 0.15);
        assert!(bind.angle_between(bounded) <= 0.1501);
    }

    #[test]
    fn global_mirror_uses_character_space_under_owner_rotation() {
        let owner = GlobalTransform::from(
            Transform::from_xyz(4.0, 2.0, -3.0).with_rotation(Quat::from_rotation_y(1.1)),
        );
        let relative = Transform::from_xyz(0.4, -0.7, 0.2).with_rotation(Quat::from_euler(
            EulerRot::XYZ,
            0.2,
            -0.3,
            0.4,
        ));
        let source = owner.mul_transform(relative);
        let mirrored = owner.affine().inverse() * mirrored_global_affine(source, owner);
        let (scale, rotation, translation) = mirrored.to_scale_rotation_translation();
        let expected = mirrored_across_anatomical_center(relative);

        assert!(translation.abs_diff_eq(expected.translation, 0.0001));
        assert!(rotation.abs_diff_eq(expected.rotation, 0.0001));
        assert!(scale.abs_diff_eq(expected.scale, 0.0001));
    }
}
