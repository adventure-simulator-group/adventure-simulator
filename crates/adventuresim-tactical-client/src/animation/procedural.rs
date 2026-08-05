use std::collections::BTreeMap;

use adventuresim_tactical_core::prelude::*;
use bevy::{math::Affine3A, prelude::*};

use super::{
    AnimationPlayback, AnimationRigScene, AuthoredBindTransform, ImpactReaction, PresentedSkeleton,
};

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
    owners: Query<(&CharacterLook, &PresentedSkeleton)>,
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

/// Applies pack fallback reflection and constructs the opposite gait half-cycle.
/// Whole-body reflection includes the central chain and swaps every bilateral
/// limb; gait reflection is limited to bilateral limbs. Applying both is an
/// involution, so their bilateral weights compose as an XOR blend.
///
/// Gait mirroring transitions only around the authored passing pose, where the
/// limbs are nearest neutral; interpolating throughout contact folds a planted
/// stride through itself. The playback field retains its historical
/// `lower_body_mirror` name, but drives arms and legs together.
pub(super) fn apply_gait_mirroring(
    playbacks: Query<&AnimationPlayback>,
    rig_scenes: Query<(Entity, &AnimationRigScene)>,
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
    let mut rig_globals = BTreeMap::<Entity, GlobalTransform>::new();
    {
        let helper = transforms.p1();
        for (entity, scene) in &rig_scenes {
            if let Ok(global) = helper.compute_global_transform(entity) {
                rig_globals.insert(scene.0, global);
            }
        }
        for (entity, bone, local) in locals {
            let Ok(global) = helper.compute_global_transform(entity) else {
                continue;
            };
            let parent = parents.get(entity).ok().map(ChildOf::parent);
            let parent_global = parent
                .and_then(|parent| helper.compute_global_transform(parent).ok())
                .unwrap_or(GlobalTransform::IDENTITY);
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
        let whole_body_weight = playback.whole_body_mirror.clamp(0.0, 1.0);
        let gait_weight = playback.lower_body_mirror.clamp(0.0, 1.0);
        if whole_body_weight <= f32::EPSILON && gait_weight <= f32::EPSILON {
            continue;
        }
        let Some(rig_global) = rig_globals.get(&owner) else {
            continue;
        };
        let mut desired_globals = BTreeMap::<Entity, Affine3A>::new();
        let mut mirror_weights = BTreeMap::<Entity, f32>::new();
        if whole_body_weight > f32::EPSILON {
            for role in [
                BoneRole::Root,
                BoneRole::Pelvis,
                BoneRole::StomachOne,
                BoneRole::StomachTwo,
                BoneRole::Chest,
                BoneRole::NeckOne,
                BoneRole::NeckTwo,
                BoneRole::Head,
            ] {
                let Some(bone) = rig.get(&role) else {
                    continue;
                };
                desired_globals.insert(
                    bone.entity,
                    mirrored_global_affine(bone.global, *rig_global),
                );
                mirror_weights.insert(bone.entity, whole_body_weight);
            }
        }
        let bilateral_weight =
            whole_body_weight + gait_weight - 2.0 * whole_body_weight * gait_weight;
        for (left_role, right_role) in [
            (BoneRole::ClavicleLeft, BoneRole::ClavicleRight),
            (BoneRole::UpperArmLeft, BoneRole::UpperArmRight),
            (BoneRole::UpperArmTwistLeft, BoneRole::UpperArmTwistRight),
            (BoneRole::ForearmLeft, BoneRole::ForearmRight),
            (BoneRole::ForearmTwistLeft, BoneRole::ForearmTwistRight),
            (BoneRole::HandLeft, BoneRole::HandRight),
            (BoneRole::WeaponLeft, BoneRole::WeaponRight),
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
                mirrored_global_affine(right.global, *rig_global),
            );
            desired_globals.insert(
                right.entity,
                mirrored_global_affine(left.global, *rig_global),
            );
            mirror_weights.insert(left.entity, bilateral_weight);
            mirror_weights.insert(right.entity, bilateral_weight);
        }
        let mut bones = transforms.p2();
        for bone in rig.values() {
            let Some(&desired_global) = desired_globals.get(&bone.entity) else {
                continue;
            };
            let weight = mirror_weights[&bone.entity];
            if weight <= f32::EPSILON {
                continue;
            }
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

const HEIGHT_TRANSITION_SPEED_METRES_PER_SECOND: f32 = 0.4;
const LOCOMOTION_STOP_HEIGHT_SPEED_METRES_PER_SECOND: f32 = 0.8;
const NORMALIZATION_TRANSITION_PER_SECOND: f32 = 8.0;
const SUPPORT_GROUNDING_TRANSITION_METRES_PER_SECOND: f32 = 0.8;
const MINIMUM_SUPPORT_GROUNDING_OFFSET_METRES: f32 = -0.18;
const MAXIMUM_SUPPORT_GROUNDING_OFFSET_METRES: f32 = 0.08;
// The upright lowered-guard humanoid_unarmed root/pelvis rotations lift its
// pelvis by about 33 mm at passing even after local Y is normalized. This is a
// measured state-and-pack calibration, not a safe assumption elsewhere.
const AUTHORED_ORDINARY_PASSING_RISE_METRES: f32 = 0.033;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LocomotionHeightState {
    initialized: bool,
    amplitude: f32,
    authored_rise_compensation: f32,
    displayed_wave: f32,
    wave_transition_offset: f32,
    normalization_weight: f32,
    pub(crate) landing_compression: f32,
    landing_recovery_metres_per_second: f32,
    landing_left_foot_target: Option<Vec3>,
    landing_right_foot_target: Option<Vec3>,
    landing_plant_owner_position: Option<Vec3>,
    landing_plant_tick: Option<u64>,
    landing_plant_resync_tick: Option<u64>,
    last_landing_sequence: u64,
    last_guard: Option<WeaponGuardState>,
    last_posture: Option<Posture>,
    last_action: Option<SkeletonAction>,
    last_grounded: Option<bool>,
    evaluation_tick: Option<u64>,
}

/// Presentation-only vertical calibration sampled at authoritative contacts.
/// The correction moves the complete authored rig and never reconstructs a
/// knee or changes the server-owned controller transform.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct SupportFootGroundingState {
    initialized: bool,
    current_offset_metres: f32,
    target_offset_metres: f32,
    contact_sequence: u64,
    evaluation_tick: Option<u64>,
}

/// One phase-owned vertical waveform with contacts at 0/.5 and passing or
/// flight peaks at .25/.75. This is presentation-only and is never applied to
/// the authoritative owner/controller transform.
fn grounded_height_wave(phase: f32, amplitude: f32) -> f32 {
    let sine = (phase.rem_euclid(1.0) * std::f32::consts::TAU).sin();
    amplitude * sine * sine
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LocomotionBodyResponseState {
    pub(crate) pitch_radians: f32,
    pub(crate) roll_radians: f32,
    target_pitch_radians: f32,
    target_roll_radians: f32,
    last_tick: Option<u64>,
    last_posture: Option<Posture>,
    last_action: Option<SkeletonAction>,
    last_grounded: Option<bool>,
}

pub(crate) fn locomotion_height_wave(skeleton: &SkeletonState) -> f32 {
    if !skeleton.grounded || skeleton.action != SkeletonAction::None {
        return 0.0;
    }
    let speed = skeleton.animation_speed();
    if speed <= 0.05 {
        return 0.0;
    }
    let profile = locomotion_profile(skeleton);
    let moving_weight = smoothstep(0.05, 0.75, speed);
    let grounded = grounded_height_wave(skeleton.gait_phase, profile.bounce_metres);
    if profile.flight_apex_metres <= f32::EPSILON {
        return grounded * moving_weight;
    }
    let half_step = (skeleton.gait_phase.rem_euclid(0.5) * 2.0).clamp(0.0, 1.0);
    let flight = 4.0 * half_step * (1.0 - half_step) * profile.flight_apex_metres;
    (grounded + flight) * moving_weight
}

fn authored_height_compensation(skeleton: &SkeletonState) -> f32 {
    if !skeleton.grounded
        || skeleton.action != SkeletonAction::None
        || skeleton.animation_pack != "humanoid_unarmed"
        || skeleton.posture != Posture::Upright
        || skeleton.weapon_guard != WeaponGuardState::Lowered
    {
        return 0.0;
    }
    AUTHORED_ORDINARY_PASSING_RISE_METRES * smoothstep(0.05, 0.75, skeleton.animation_speed())
}

fn locomotion_normalization_target(skeleton: &SkeletonState) -> f32 {
    (skeleton.grounded
        && skeleton.action == SkeletonAction::None
        && matches!(skeleton.posture, Posture::Upright | Posture::Crouched)
        && skeleton.animation_speed() > 0.05) as u8 as f32
}

fn advance_towards(current: f32, target: f32, maximum_delta: f32) -> f32 {
    current + (target - current).clamp(-maximum_delta.max(0.0), maximum_delta.max(0.0))
}

fn landing_compression_for_impact(profile: LocomotionProfile, impact_speed: f32) -> f32 {
    if impact_speed < 1.0 {
        0.0
    } else {
        (impact_speed * profile.landing.compression_per_metre_per_second).clamp(
            profile.landing.minimum_compression_metres,
            profile.landing.maximum_compression_metres,
        )
    }
}

/// Normalizes authored root/pelvis height before applying the single gait
/// waveform. XZ translation, rotations, and all authored limb transforms are
/// retained. Action and airborne transitions blend back to authored central Y.
pub(super) fn stabilize_locomotion_torso(
    mut commands: Commands,
    mut owners: Query<(
        Entity,
        &PresentedSkeleton,
        Option<&mut LocomotionHeightState>,
    )>,
    mut bones: Query<(&HumanoidBone, &AuthoredBindTransform, &mut Transform)>,
) {
    let mut heights = BTreeMap::new();
    for (owner, skeleton, state) in &mut owners {
        let target_wave = locomotion_height_wave(skeleton);
        let target_authored_compensation = authored_height_compensation(skeleton);
        let target_normalization = locomotion_normalization_target(skeleton);
        let mut next = state.as_deref().copied().unwrap_or_default();
        if !next.initialized {
            next.initialized = true;
            next.amplitude = target_wave;
            next.authored_rise_compensation = target_authored_compensation;
            next.normalization_weight = target_normalization;
            next.last_guard = Some(skeleton.weapon_guard);
            next.last_posture = Some(skeleton.posture);
            next.last_action = Some(skeleton.action);
            next.last_grounded = Some(skeleton.grounded);
            next.last_landing_sequence = skeleton.landing_sequence;
        }
        let tick_delta =
            presentation_tick_delta(next.evaluation_tick, skeleton.locomotion_sample_tick)
                .unwrap_or_default();
        next.evaluation_tick = Some(skeleton.locomotion_sample_tick);
        let delta_seconds = tick_delta as f32 / LOCOMOTION_SAMPLE_HZ;
        let ordinary_stop = skeleton.grounded
            && skeleton.action == SkeletonAction::None
            && matches!(skeleton.posture, Posture::Upright | Posture::Crouched)
            && skeleton.animation_speed() <= 0.05;
        next.amplitude = if ordinary_stop {
            advance_towards(
                next.amplitude,
                0.0,
                LOCOMOTION_STOP_HEIGHT_SPEED_METRES_PER_SECOND * delta_seconds,
            )
        } else {
            target_wave
        };
        next.authored_rise_compensation = advance_towards(
            next.authored_rise_compensation,
            target_authored_compensation,
            HEIGHT_TRANSITION_SPEED_METRES_PER_SECOND * delta_seconds,
        );
        let retained_normalization =
            target_normalization.max((next.amplitude.abs() > 0.001) as u8 as f32);
        next.normalization_weight = advance_towards(
            next.normalization_weight,
            retained_normalization,
            NORMALIZATION_TRANSITION_PER_SECOND * delta_seconds,
        );
        let landing_delta = skeleton
            .landing_sequence
            .checked_sub(next.last_landing_sequence)
            .filter(|delta| *delta <= 8);
        let landed = landing_delta.is_some_and(|delta| delta > 0);
        if skeleton.landing_sequence != next.last_landing_sequence {
            next.last_landing_sequence = skeleton.landing_sequence;
        }
        if landed {
            next.landing_compression = landing_compression_for_impact(
                locomotion_profile(skeleton),
                skeleton.landing_impact_speed,
            );
            next.landing_recovery_metres_per_second =
                next.landing_compression / locomotion_profile(skeleton).landing.recovery_seconds;
            next.landing_left_foot_target = None;
            next.landing_right_foot_target = None;
            next.landing_plant_owner_position = None;
            next.landing_plant_tick = None;
            next.landing_plant_resync_tick = None;
        }
        if !landed {
            next.landing_compression = advance_towards(
                next.landing_compression,
                0.0,
                next.landing_recovery_metres_per_second * delta_seconds,
            );
        }
        if !skeleton.grounded
            || skeleton.action != SkeletonAction::None
            || next.landing_compression <= 0.001
        {
            clear_landing_foot_plants(&mut next);
        }
        let compensation =
            grounded_height_wave(skeleton.gait_phase, next.authored_rise_compensation);
        let raw_wave = next.amplitude - compensation;
        let state_changed = next.last_guard != Some(skeleton.weapon_guard)
            || next.last_posture != Some(skeleton.posture)
            || next.last_action != Some(skeleton.action)
            || next.last_grounded != Some(skeleton.grounded);
        if state_changed {
            next.wave_transition_offset = next.displayed_wave - raw_wave;
        } else {
            next.wave_transition_offset = advance_towards(
                next.wave_transition_offset,
                0.0,
                HEIGHT_TRANSITION_SPEED_METRES_PER_SECOND * delta_seconds,
            );
        }
        next.displayed_wave = raw_wave + next.wave_transition_offset;
        next.last_guard = Some(skeleton.weapon_guard);
        next.last_posture = Some(skeleton.posture);
        next.last_action = Some(skeleton.action);
        next.last_grounded = Some(skeleton.grounded);
        heights.insert(owner, next);
        if let Some(mut state) = state {
            *state = next;
        } else {
            commands.entity(owner).insert(next);
        }
    }

    for (bone, bind, mut transform) in &mut bones {
        let Some(&height) = heights.get(&bone.owner) else {
            continue;
        };
        if height.normalization_weight <= f32::EPSILON
            && height.amplitude <= f32::EPSILON
            && height.landing_compression <= f32::EPSILON
        {
            continue;
        }
        let translation_limit = match bone.role {
            BoneRole::Root => Vec3::new(0.02, 0.02, 0.025),
            BoneRole::Pelvis => Vec3::new(0.035, 0.04, 0.045),
            BoneRole::StomachOne | BoneRole::StomachTwo | BoneRole::Chest => Vec3::splat(0.012),
            BoneRole::NeckOne | BoneRole::NeckTwo | BoneRole::Head => Vec3::splat(0.008),
            _ => continue,
        };
        let authored_translation = transform.translation;
        let mut normalized_translation = bind.local.translation
            + (transform.translation - bind.local.translation)
                .clamp(-translation_limit, translation_limit);
        match bone.role {
            BoneRole::Root => {
                normalized_translation.y = bind.local.translation.y + height.displayed_wave;
            }
            BoneRole::Pelvis => {
                normalized_translation.y = bind.local.translation.y;
            }
            _ => {}
        }
        transform.translation = authored_translation.lerp(
            normalized_translation,
            height.normalization_weight.clamp(0.0, 1.0),
        );
    }
}

fn ordinary_support_grounding_is_active(skeleton: &SkeletonState) -> bool {
    skeleton.grounded
        && skeleton.action == SkeletonAction::None
        && skeleton.weapon_guard == WeaponGuardState::Lowered
        && matches!(skeleton.posture, Posture::Upright | Posture::Crouched)
        && skeleton.animation_speed() > 0.05
}

fn support_grounding_target(foot_height: f32, floor_height: f32, sole_offset: f32) -> f32 {
    (floor_height + sole_offset - foot_height).clamp(
        MINIMUM_SUPPORT_GROUNDING_OFFSET_METRES,
        MAXIMUM_SUPPORT_GROUNDING_OFFSET_METRES,
    )
}

/// Grounds ordinary locomotion by translating the complete visual rig from
/// its supported sole. The correction is acquired only at a contact edge and
/// held through swing/flight, preserving authored leg geometry and the shared
/// phase-owned height curve without enabling analytic leg IK.
pub(super) fn apply_support_foot_grounding(
    mut commands: Commands,
    mut owners: Query<(&PresentedSkeleton, Option<&mut SupportFootGroundingState>)>,
    rig_scenes: Query<(Entity, &AnimationRigScene)>,
    bones: Query<(Entity, &HumanoidBone)>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let mut rigs = BTreeMap::<Entity, BTreeMap<BoneRole, Entity>>::new();
    for (entity, bone) in &bones {
        rigs.entry(bone.owner)
            .or_default()
            .insert(bone.role, entity);
    }
    let scene_roots = rig_scenes
        .iter()
        .map(|(entity, rig)| (rig.0, entity))
        .collect::<BTreeMap<_, _>>();

    for (owner, rig) in rigs {
        let Ok((skeleton, state)) = owners.get_mut(owner) else {
            continue;
        };
        let Some(&root) = rig.get(&BoneRole::Root) else {
            continue;
        };
        let active = ordinary_support_grounding_is_active(skeleton);
        let mut next = state.as_deref().copied().unwrap_or_default();
        let tick_delta =
            presentation_tick_delta(next.evaluation_tick, skeleton.locomotion_sample_tick)
                .unwrap_or_default();
        let new_contact = !next.initialized || next.contact_sequence != skeleton.contact_sequence;
        if active && new_contact {
            let foot_role = match skeleton.contact_foot {
                LeadFoot::Left => BoneRole::FootLeft,
                LeadFoot::Right => BoneRole::FootRight,
            };
            if let (Some(&foot), Some(&scene_root)) = (rig.get(&foot_role), scene_roots.get(&owner))
            {
                let foot_global = transforms.p0().compute_global_transform(foot).ok();
                let scene_global = transforms.p0().compute_global_transform(scene_root).ok();
                if let (Some(foot_global), Some(scene_global)) = (foot_global, scene_global) {
                    next.target_offset_metres = support_grounding_target(
                        foot_global.translation().y,
                        scene_global.translation().y,
                        ankle_sole_offset(foot_global.rotation()),
                    );
                    next.contact_sequence = skeleton.contact_sequence;
                    if !next.initialized {
                        next.current_offset_metres = next.target_offset_metres;
                    }
                }
            }
        } else if !active {
            next.target_offset_metres = 0.0;
        }
        next.initialized = true;
        next.evaluation_tick = Some(skeleton.locomotion_sample_tick);
        if tick_delta > 0 {
            next.current_offset_metres = advance_towards(
                next.current_offset_metres,
                next.target_offset_metres,
                SUPPORT_GROUNDING_TRANSITION_METRES_PER_SECOND * tick_delta as f32
                    / LOCOMOTION_SAMPLE_HZ,
            );
        }

        if next.current_offset_metres.abs() > 0.0001 {
            let local_delta = parents
                .get(root)
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
                        .transform_vector3(Vec3::Y * next.current_offset_metres)
                })
                .unwrap_or(Vec3::Y * next.current_offset_metres);
            if local_delta.is_finite()
                && let Ok(mut transform) = transforms.p1().get_mut(root)
            {
                transform.translation += local_delta;
            }
        }
        if let Some(mut state) = state {
            *state = next;
        } else {
            commands.entity(owner).insert(next);
        }
    }
}

/// Preserves both world-space feet during the short landing-only pelvis
/// compression by flexing the actual hip/knee chains. This is independent of
/// the opt-in terrain solver and never translates or stretches thigh roots.
pub(super) fn apply_landing_leg_compression(
    mut owners: Query<
        (
            &PresentedSkeleton,
            &mut LocomotionHeightState,
            &GlobalTransform,
        ),
        Without<HumanoidBone>,
    >,
    bones: Query<(Entity, &HumanoidBone, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let mut rigs = BTreeMap::<Entity, BTreeMap<BoneRole, Entity>>::new();
    let mut previous_world_positions = BTreeMap::<Entity, Vec3>::new();
    for (entity, bone, global) in &bones {
        rigs.entry(bone.owner)
            .or_default()
            .insert(bone.role, entity);
        previous_world_positions.insert(entity, global.translation());
    }
    for (owner, rig) in rigs {
        let Ok((skeleton, mut height, owner_transform)) = owners.get_mut(owner) else {
            continue;
        };
        if !skeleton.grounded
            || skeleton.action != SkeletonAction::None
            || height.landing_compression <= 0.001
        {
            clear_landing_foot_plants(&mut height);
            continue;
        }
        let owner_position = owner_transform.translation();
        if height.landing_plant_resync_tick == Some(skeleton.locomotion_sample_tick) {
            continue;
        }
        height.landing_plant_resync_tick = None;
        let discontinuous = landing_plant_is_discontinuous(
            height.landing_plant_tick,
            skeleton.locomotion_sample_tick,
            height.landing_plant_owner_position,
            owner_position,
        );
        if discontinuous {
            clear_landing_foot_plants(&mut height);
            height.landing_plant_resync_tick = Some(skeleton.locomotion_sample_tick);
            continue;
        }
        height.landing_plant_tick = Some(skeleton.locomotion_sample_tick);
        height.landing_plant_owner_position = Some(owner_position);
        let landing_compression = height.landing_compression;
        if let Some(&pelvis) = rig.get(&BoneRole::Pelvis) {
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
                        .transform_vector3(Vec3::NEG_Y * landing_compression)
                })
                .unwrap_or(Vec3::NEG_Y * landing_compression);
            if local_delta.is_finite()
                && let Ok(mut transform) = transforms.p1().get_mut(pelvis)
            {
                transform.translation += local_delta;
            }
        }
        for (upper_role, lower_role, foot_role, left) in [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                false,
            ),
        ] {
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
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot
                .global
                .translation()
                .distance(foot_snapshot.global.translation());
            let target = if left {
                retain_landing_foot_target(
                    &mut height.landing_left_foot_target,
                    previous_world_positions[&foot],
                )
            } else {
                retain_landing_foot_target(
                    &mut height.landing_right_foot_target,
                    previous_world_positions[&foot],
                )
            };
            let side = if left { -1.0 } else { 1.0 };
            let pole = owner_transform.rotation() * canonical_knee_pole(side);
            if let Some(solution) = solve_landing_two_bone(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                foot_snapshot.global.translation(),
                target,
                upper_length,
                lower_length,
                pole,
                landing_compression,
            ) {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
            }
        }
    }
}

fn retain_landing_foot_target(
    retained: &mut Option<Vec3>,
    pre_landing_foot_position: Vec3,
) -> Vec3 {
    *retained.get_or_insert(pre_landing_foot_position)
}

fn landing_plant_is_discontinuous(
    previous_tick: Option<u64>,
    current_tick: u64,
    previous_owner_position: Option<Vec3>,
    current_owner_position: Vec3,
) -> bool {
    presentation_tick_delta(previous_tick, current_tick).is_none()
        || previous_owner_position
            .is_some_and(|previous| previous.distance_squared(current_owner_position) > 4.0)
}

fn clear_landing_foot_plants(height: &mut LocomotionHeightState) {
    height.landing_left_foot_target = None;
    height.landing_right_foot_target = None;
    height.landing_plant_owner_position = None;
    height.landing_plant_tick = None;
    height.landing_plant_resync_tick = None;
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
    left_foot_target: Option<Vec3>,
    right_foot_target: Option<Vec3>,
    left_foot_world_target: Option<Vec3>,
    right_foot_world_target: Option<Vec3>,
    left_support_weight: Option<f32>,
    right_support_weight: Option<f32>,
    left_release_active: bool,
    right_release_active: bool,
    left_arm: Option<Vec3>,
    right_arm: Option<Vec3>,
    pelvis_shift: f32,
    raised_pelvis_shift: f32,
    evaluation_tick: Option<u64>,
}

/// Optional deterministic clock for tools that render the same simulation
/// tick more than once. Gameplay leaves the override unset and advances from
/// Bevy's render delta.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub(crate) struct ProceduralAnimationClock {
    fixed_tick: Option<(u64, f32)>,
}

impl ProceduralAnimationClock {
    pub(crate) fn set_fixed_tick(&mut self, tick: u64, delta_seconds: f32) {
        self.fixed_tick = Some((tick, delta_seconds.max(0.0)));
    }

    pub(crate) fn fixed_step(&self) -> Option<(u64, f32)> {
        self.fixed_tick
    }
}

const MIN_INTER_FOOT_SEPARATION: f32 = 0.16;
// Cascadeur's final ankle bones sit about 15 mm inside analytic targets after
// the complete hierarchy solve. Keep a measured planning allowance so the
// rendered bones, not merely abstract targets, retain the 0.16 m contract.
const GUARD_TARGET_INTER_FOOT_SEPARATION: f32 = MIN_INTER_FOOT_SEPARATION + 0.04;
const FOOT_TRACK_INNER: f32 = MIN_INTER_FOOT_SEPARATION * 0.5;
const FOOT_TRACK_OUTER: f32 = 0.55;
const MAX_PLANT_DISCONTINUITY: f32 = 2.0;
const MAX_FOOT_TARGET_SPEED: f32 = 12.0;
const MAX_FOOT_TARGET_STEP: f32 = 0.2;
const PELVIS_CORRECTION_SPEED: f32 = 1.6;
const MAX_PELVIS_CORRECTION_STEP: f32 = 0.05;
const MIN_KNEE_FLEXION: f32 = 20.0_f32.to_radians();
// Keep the normal knee reserve while a landing visibly carries weight, then
// release it before the pelvis reaches the authored height. The released
// reach remains capped at the authored leg extension, preventing a final
// recovery-frame foot lift or snap without introducing a straight-leg target.
const LANDING_KNEE_RESERVE_RELEASE_COMPRESSION: f32 = 0.012;
const LANDING_KNEE_RESERVE_FULL_COMPRESSION: f32 = 0.04;
const RAISED_GUARD_PELVIS_DROP: f32 = 0.14;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct ProceduralIkState(PoleMemory);

/// Client-only world-space plants for combat-stance locomotion. The replicated
/// skeleton chooses cadence and direction; exact feet remain presentation
/// state so they never become tactical authority.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RaisedFootworkState {
    initialized: bool,
    half_step: u8,
    lead: LeadFoot,
    swing_left: bool,
    step_origin: Vec3,
    step_rotation: Quat,
    swing_stance_local: Vec3,
    swing_start: Vec3,
    swing_end: Vec3,
    left_plant: Vec3,
    right_plant: Vec3,
    evaluation_tick: Option<u64>,
    step_sequence: u32,
    pub(crate) left_solve_target: Option<Vec3>,
    pub(crate) right_solve_target: Option<Vec3>,
}

impl Default for RaisedFootworkState {
    fn default() -> Self {
        Self {
            initialized: false,
            half_step: 0,
            lead: LeadFoot::Left,
            swing_left: false,
            step_origin: Vec3::ZERO,
            step_rotation: Quat::IDENTITY,
            swing_stance_local: Vec3::ZERO,
            swing_start: Vec3::ZERO,
            swing_end: Vec3::ZERO,
            left_plant: Vec3::ZERO,
            right_plant: Vec3::ZERO,
            evaluation_tick: None,
            step_sequence: 0,
            left_solve_target: None,
            right_solve_target: None,
        }
    }
}

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
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    terrain: Query<&SceneTerrain>,
    owners: Query<&PresentedSkeleton>,
    rig_scenes: Query<(Entity, &AnimationRigScene)>,
    bones: Query<(Entity, &HumanoidBone, Option<&SoleUpAxis>)>,
    parents: Query<&ChildOf>,
    mut ik_states: Query<&mut ProceduralIkState>,
    mut raised_states: Query<&mut RaisedFootworkState>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    let terrain = terrain.iter().next();
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
        if !raised_footwork_posture_is_valid(skeleton) {
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = PoleMemory::default();
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = RaisedFootworkState::default();
            }
            continue;
        }
        let raised_guard_follower = skeleton.weapon_guard == WeaponGuardState::Raised
            && skeleton.action == SkeletonAction::None
            && skeleton.raised_locomotion.active;
        if !raised_guard_follower && let Ok(mut raised) = raised_states.get_mut(owner) {
            *raised = RaisedFootworkState::default();
        }
        let (left_weight, right_weight) = locomotion_support_weights(skeleton);
        let legs = [
            (
                BoneRole::ThighLeft,
                BoneRole::ShinLeft,
                BoneRole::FootLeft,
                left_weight,
                true,
            ),
            (
                BoneRole::ThighRight,
                BoneRole::ShinRight,
                BoneRole::FootRight,
                right_weight,
                false,
            ),
        ];
        let mut memory = ik_states
            .get_mut(owner)
            .map(|state| state.0)
            .unwrap_or_default();
        let state_delta_seconds = match clock.fixed_tick {
            Some((tick, _)) if memory.evaluation_tick == Some(tick) => 0.0,
            Some((tick, delta_seconds)) => {
                memory.evaluation_tick = Some(tick);
                delta_seconds
            }
            None => time.delta_secs(),
        };
        let desired_raised_pelvis_shift = if raised_guard_follower {
            -RAISED_GUARD_PELVIS_DROP
        } else {
            0.0
        };
        if state_delta_seconds > 0.0 {
            memory.raised_pelvis_shift = advance_pelvis_shift(
                memory.raised_pelvis_shift,
                desired_raised_pelvis_shift,
                state_delta_seconds,
            );
        }
        let raised_pelvis_shift = memory.raised_pelvis_shift;
        if raised_pelvis_shift < -0.001
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
                        .transform_vector3(Vec3::Y * raised_pelvis_shift)
                })
                .unwrap_or(Vec3::Y * raised_pelvis_shift);
            if let Ok(mut transform) = transforms.p1().get_mut(pelvis) {
                transform.translation += local_delta;
            }
        }
        // Pole, plant, and pelvis reach all belong to the server-owned
        // authored-body frame. The child rig carries no locomotion yaw.
        let (rig_origin, rig_rotation) = rig_scenes
            .iter()
            .find(|(_, scene)| scene.0 == owner)
            .and_then(|(entity, _)| transforms.p0().compute_global_transform(entity).ok())
            .map(|global| (global.translation(), global.rotation()))
            .unwrap_or((Vec3::ZERO, Quat::IDENTITY));

        if raised_guard_follower {
            // The authored guard is nearly straight-legged. Smoothly lower its
            // pelvis so a world-planted support foot remains within physical
            // reach without a one-frame stance-height snap at starts or stops.
            let left = (
                rig.get(&BoneRole::ThighLeft),
                rig.get(&BoneRole::ShinLeft),
                rig.get(&BoneRole::FootLeft),
            );
            let right = (
                rig.get(&BoneRole::ThighRight),
                rig.get(&BoneRole::ShinRight),
                rig.get(&BoneRole::FootRight),
            );
            let (Some(&left_upper), Some(&left_lower), Some(&left_foot)) = left else {
                continue;
            };
            let (Some(&right_upper), Some(&right_lower), Some(&right_foot)) = right else {
                continue;
            };
            let Some((_, _, left_foot_snapshot)) = snapshot_chain(
                left_upper,
                left_lower,
                left_foot,
                &parents,
                &transforms.p0(),
            ) else {
                continue;
            };
            let Some((_, _, right_foot_snapshot)) = snapshot_chain(
                right_upper,
                right_lower,
                right_foot,
                &parents,
                &transforms.p0(),
            ) else {
                continue;
            };
            let mut footwork = raised_states
                .get_mut(owner)
                .map(|state| *state)
                .unwrap_or_default();
            let tick = clock.fixed_tick.map(|(tick, _)| tick);
            let advances = match tick {
                Some(tick) => footwork.evaluation_tick != Some(tick),
                None => state_delta_seconds > 0.0,
            };
            if let Some(tick) = tick {
                footwork.evaluation_tick = Some(tick);
            }
            let phase = skeleton.gait_phase.rem_euclid(1.0);
            let half_step = (phase >= 0.5) as u8;
            let swing_left = skeleton.raised_locomotion.swing_foot == LeadFoot::Left;
            // Pelvis lowering must not lower the semantic movement plane.
            // Recover the pre-drop authored ankle positions for persistent
            // flat plants; the analytic solve bends the lowered legs to them.
            let left_authored =
                left_foot_snapshot.global.translation() + Vec3::Y * -raised_pelvis_shift;
            let right_authored =
                right_foot_snapshot.global.translation() + Vec3::Y * -raised_pelvis_shift;
            let visible_left = memory.left_foot_world_target.unwrap_or(left_authored);
            let visible_right = memory.right_foot_world_target.unwrap_or(right_authored);
            let discontinuous =
                footwork.initialized && rig_origin.distance_squared(footwork.step_origin) > 4.0;
            let sequence_delta = guard_step_sequence_delta(
                footwork.step_sequence,
                skeleton.raised_locomotion.step_sequence,
            );
            let skipped_handoff = footwork.initialized && sequence_delta > 1;
            if !footwork.initialized
                || footwork.lead != skeleton.lead_foot
                || discontinuous
                || skipped_handoff
            {
                footwork = RaisedFootworkState {
                    initialized: true,
                    half_step,
                    lead: skeleton.lead_foot,
                    swing_left,
                    step_origin: rig_origin,
                    step_rotation: rig_rotation,
                    swing_stance_local: rig_rotation.inverse()
                        * ((if swing_left {
                            left_authored
                        } else {
                            right_authored
                        }) - rig_origin),
                    swing_start: if swing_left {
                        visible_left
                    } else {
                        visible_right
                    },
                    swing_end: if swing_left {
                        left_authored
                    } else {
                        right_authored
                    },
                    left_plant: visible_left,
                    right_plant: visible_right,
                    evaluation_tick: tick,
                    step_sequence: skeleton.raised_locomotion.step_sequence,
                    left_solve_target: None,
                    right_solve_target: None,
                };
            } else if advances && sequence_delta == 1 {
                if footwork.swing_left {
                    footwork.left_plant = footwork.left_solve_target.unwrap_or(footwork.swing_end);
                } else {
                    footwork.right_plant =
                        footwork.right_solve_target.unwrap_or(footwork.swing_end);
                }
                footwork.half_step = half_step;
                footwork.step_sequence = skeleton.raised_locomotion.step_sequence;
                footwork.swing_left = swing_left;
                footwork.step_origin = rig_origin;
                footwork.step_rotation = rig_rotation;
                footwork.swing_stance_local = rig_rotation.inverse()
                    * ((if swing_left {
                        left_authored
                    } else {
                        right_authored
                    }) - rig_origin);
                footwork.swing_start = if swing_left {
                    footwork.left_plant
                } else {
                    footwork.right_plant
                };
            }
            let local_direction = skeleton
                .raised_locomotion
                .local_direction
                .normalize_or_zero();
            // Semantic controller axes are opposite the authored rig's X/Z
            // axes. The owner carries the single 180-degree body conversion.
            let rig_local_direction = -local_direction;
            let step_length = guard_step_length(skeleton.raised_locomotion.speed);
            let opposite_plant = if footwork.swing_left {
                footwork.right_plant
            } else {
                footwork.left_plant
            };
            footwork.swing_end = plan_guard_step_endpoint(
                footwork.step_origin,
                footwork.step_rotation,
                footwork.swing_stance_local,
                rig_local_direction,
                step_length,
                footwork.swing_left,
                opposite_plant,
            );
            let step_progress = (phase * 2.0).fract();
            let horizontal_progress = smoothstep(0.0, 1.0, step_progress);
            let mut swing_target = footwork
                .swing_start
                .lerp(footwork.swing_end, horizontal_progress);
            let mut left_target = footwork.left_plant;
            let mut right_target = footwork.right_plant;
            let support_target = if footwork.swing_left {
                right_target
            } else {
                left_target
            };
            swing_target = constrain_guard_swing_to_live_corridor(
                swing_target,
                support_target,
                rig_origin,
                rig_rotation,
                footwork.swing_stance_local.x.signum(),
            );
            let mut terrain_swing_end = footwork.swing_end;
            if enabled.0
                && let Some(terrain) = terrain
            {
                left_target = terrain_conformed_guard_target(
                    left_target,
                    terrain.height_at(left_target.xz()),
                );
                right_target = terrain_conformed_guard_target(
                    right_target,
                    terrain.height_at(right_target.xz()),
                );
                terrain_swing_end = terrain_conformed_guard_target(
                    terrain_swing_end,
                    terrain.height_at(terrain_swing_end.xz()),
                );
                swing_target.y = footwork
                    .swing_start
                    .y
                    .lerp(terrain_swing_end.y, horizontal_progress);
            }
            swing_target.y += (std::f32::consts::PI * step_progress).sin() * 0.10;
            if footwork.swing_left {
                left_target = swing_target;
            } else {
                right_target = swing_target;
            }

            for (upper, lower, foot, target, left, support) in [
                (
                    left_upper,
                    left_lower,
                    left_foot,
                    left_target,
                    true,
                    !footwork.swing_left,
                ),
                (
                    right_upper,
                    right_lower,
                    right_foot,
                    right_target,
                    false,
                    footwork.swing_left,
                ),
            ] {
                let Some((upper_snapshot, lower_snapshot, foot_snapshot)) =
                    snapshot_chain(upper, lower, foot, &parents, &transforms.p0())
                else {
                    continue;
                };
                let upper_length = upper_snapshot
                    .global
                    .translation()
                    .distance(lower_snapshot.global.translation());
                let lower_length = lower_snapshot
                    .global
                    .translation()
                    .distance(foot_snapshot.global.translation());
                let side = anatomical_side(
                    rig_rotation,
                    rig_origin,
                    upper_snapshot.global.translation(),
                    left,
                );
                let remembered = if left {
                    memory.left_leg
                } else {
                    memory.right_leg
                };
                let canonical_pole = canonical_knee_pole(side);
                let remembered = remembered.filter(|pole| pole.dot(canonical_pole) > 0.2);
                let pole = pole_to_world(rig_rotation, remembered.unwrap_or(canonical_pole));
                if let Some(solution) = solve_two_bone_with_reach(
                    upper_snapshot.global.translation(),
                    lower_snapshot.global.translation(),
                    foot_snapshot.global.translation(),
                    target,
                    upper_length,
                    lower_length,
                    pole,
                    (upper_length + lower_length) * 0.999,
                ) {
                    apply_two_bone_solution(
                        upper,
                        lower,
                        foot,
                        solution,
                        &parents,
                        &mut transforms,
                    );
                    let bend = (solution.knee - upper_snapshot.global.translation())
                        .reject_from_normalized(solution.end_direction);
                    if let Some(valid) = bend.try_normalize() {
                        if left {
                            memory.left_leg = Some(pole_to_owner(rig_rotation, valid));
                        } else {
                            memory.right_leg = Some(pole_to_owner(rig_rotation, valid));
                        }
                    }
                }
                if enabled.0
                    && support
                    && let Some(terrain) = terrain
                    && let Some(normal) = terrain.normal_at(target.xz())
                    && let Some(&sole_axis) = sole_axes.get(&foot)
                {
                    align_foot_to_slope(foot, sole_axis, normal, 1.0, &parents, &mut transforms);
                }
                if left {
                    footwork.left_solve_target = Some(target);
                    memory.left_foot_world_target = Some(target);
                    memory.left_support_weight = Some(support as u8 as f32);
                } else {
                    footwork.right_solve_target = Some(target);
                    memory.right_foot_world_target = Some(target);
                    memory.right_support_weight = Some(support as u8 as f32);
                }
            }
            if let Ok(mut state) = raised_states.get_mut(owner) {
                *state = footwork;
            } else {
                commands.entity(owner).insert(footwork);
            }
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = memory;
            } else {
                commands.entity(owner).insert(ProceduralIkState(memory));
            }
            continue;
        }

        if !enabled.0 {
            // Terrain IK is opt-in. Clear only leg targets so enabling it later
            // cannot resurrect stale plants; arm pole continuity is unrelated.
            memory.left_foot_plant = None;
            memory.right_foot_plant = None;
            memory.left_foot_target = None;
            memory.right_foot_target = None;
            memory.left_foot_world_target = None;
            memory.right_foot_world_target = None;
            memory.left_support_weight = None;
            memory.right_support_weight = None;
            memory.left_release_active = false;
            memory.right_release_active = false;
            memory.pelvis_shift = 0.0;
            if let Ok(mut state) = ik_states.get_mut(owner) {
                state.0 = memory;
            } else {
                commands.entity(owner).insert(ProceduralIkState(memory));
            }
            continue;
        }
        let Some(terrain) = terrain else {
            continue;
        };
        let mut desired_hip_shift = 0.0_f32;
        for (upper_role, lower_role, foot_role, weight, left) in legs {
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
            let position = foot_snapshot.global.translation();
            if let Some(height) = terrain.height_at(position.xz()) {
                let desired_ankle = height + ankle_sole_offset(foot_snapshot.global.rotation());
                desired_hip_shift = desired_hip_shift
                    .min(((desired_ankle - position.y) * weight).clamp(-0.18, 0.0));
            }
            let plant = if left {
                memory.left_foot_plant
            } else {
                memory.right_foot_plant
            };
            let Some(plant) = plant else { continue };
            let side = anatomical_side(
                rig_rotation,
                rig_origin,
                upper_snapshot.global.translation(),
                left,
            );
            let horizontal_target = constrain_foot_to_track(plant, rig_origin, rig_rotation, side);
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            let target_y = height + ankle_sole_offset(foot_snapshot.global.rotation());
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot
                .global
                .translation()
                .distance(foot_snapshot.global.translation());
            let reach = maximum_reach(upper_length, lower_length);
            let horizontal_distance = (horizontal_target - upper_snapshot.global.translation())
                .xz()
                .length();
            let maximum_vertical = (reach * reach - horizontal_distance * horizontal_distance)
                .max(0.0)
                .sqrt();
            let reach_shift = target_y + maximum_vertical - upper_snapshot.global.translation().y;
            desired_hip_shift = desired_hip_shift.min((reach_shift * weight).clamp(-0.25, 0.0));
        }
        // Couple both legs through one bounded, continuous pelvis correction.
        // The authored pose is restored each frame, so this retained scalar is
        // the only temporal state and cannot accumulate transform drift.
        if state_delta_seconds > 0.0 {
            memory.pelvis_shift =
                advance_pelvis_shift(memory.pelvis_shift, desired_hip_shift, state_delta_seconds);
        }
        let hip_shift = memory.pelvis_shift;
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
        for (upper_role, lower_role, foot_role, weight, left) in legs {
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
            let mut plant = if left {
                memory.left_foot_plant
            } else {
                memory.right_foot_plant
            };
            let side = anatomical_side(
                rig_rotation,
                rig_origin,
                upper_snapshot.global.translation(),
                left,
            );
            let upper_length = upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation());
            let lower_length = lower_snapshot.global.translation().distance(foot_position);
            if weight <= 0.05
                || plant.is_some_and(|position| !plant_is_continuous(position, foot_position))
            {
                plant = None;
            }
            if !terrain_leg_has_support(weight) {
                if left {
                    memory.left_foot_plant = None;
                    memory.left_foot_target = None;
                    memory.left_foot_world_target = None;
                    memory.left_support_weight = Some(0.0);
                    memory.left_release_active = false;
                } else {
                    memory.right_foot_plant = None;
                    memory.right_foot_target = None;
                    memory.right_foot_world_target = None;
                    memory.right_support_weight = Some(0.0);
                    memory.right_release_active = false;
                }
                // A zero-support swing leg is already in its authored FK pose.
                // Solving it back to the same ankle target can still replace
                // the authored knee bend with the analytic pole solution.
                continue;
            }
            // Do not memorize a footprint while the swing foot is merely
            // approaching the ground. Capturing that stale position early
            // makes the pelvis outrun it, forcing the reach limiter to drag a
            // fully weighted foot and drive the knee toward extension.
            if weight >= 0.95 && plant.is_none() && !raised_guard_follower {
                let visible_contact = if left {
                    memory.left_foot_world_target
                } else {
                    memory.right_foot_world_target
                }
                .unwrap_or(foot_position);
                plant = Some(constrain_foot_to_track(
                    visible_contact,
                    rig_origin,
                    rig_rotation,
                    side,
                ));
            }
            let mut horizontal_target = constrain_foot_to_track(
                plant.unwrap_or(foot_position),
                rig_origin,
                rig_rotation,
                side,
            );
            let Some(height) = terrain.height_at(horizontal_target.xz()) else {
                continue;
            };
            let sole_offset = ankle_sole_offset(foot_snapshot.global.rotation());
            let mut planted_target = Vec3::new(
                horizontal_target.x,
                height + sole_offset,
                horizontal_target.z,
            );
            // A turning or advancing pelvis can make an otherwise valid plant
            // unreachable before its support weight releases. Slide that
            // target only as far as anatomical reach requires instead of
            // dropping and reacquiring it in one frame. Re-store the adjusted
            // target so successive turns follow the side corridor continuously.
            planted_target = constrain_target_to_reach(
                planted_target,
                upper_snapshot.global.translation(),
                maximum_reach(upper_length, lower_length),
            );
            horizontal_target.x = planted_target.x;
            horizontal_target.z = planted_target.z;
            plant = plant.map(|_| horizontal_target);
            if left {
                memory.left_foot_plant = plant;
            } else {
                memory.right_foot_plant = plant;
            }
            // Sparse authored locomotion poses can move the swing foot much
            // farther than one rendered frame should permit when support is
            // released. Follow that desired pose at a bounded velocity so the
            // final IK target cannot teleport, while still converging all the
            // way back to the unconstrained authored swing during flight.
            let mut desired_target = foot_position.lerp(planted_target, weight);
            // An unloaded sparse swing pose can dip below uneven terrain,
            // especially when the forward gait is reused in reverse. Preserve
            // exact stance contact while giving the free foot a small
            // support-weighted clearance floor.
            desired_target.y = desired_target
                .y
                .max(planted_target.y + 0.05 * (1.0 - weight));
            let desired_owner_target = rig_rotation.inverse() * (desired_target - rig_origin);
            let (previous_owner_target, previous_support, mut release_active) = if left {
                (
                    memory.left_foot_target,
                    memory.left_support_weight,
                    memory.left_release_active,
                )
            } else {
                (
                    memory.right_foot_target,
                    memory.right_support_weight,
                    memory.right_release_active,
                )
            };
            if let Some(previous_support) = previous_support {
                if weight + 0.001 < previous_support {
                    release_active = true;
                } else if weight > previous_support + 0.001 {
                    // Contact acquisition should lock immediately. The long
                    // swing interval has already brought the authored foot
                    // close to its next plant, and filtering acquisition is
                    // perceived as skating under load.
                    release_active = false;
                }
            }
            let owner_target = if release_active {
                advance_foot_target(
                    previous_owner_target,
                    desired_owner_target,
                    state_delta_seconds,
                )
            } else {
                desired_owner_target
            };
            if owner_target.distance_squared(desired_owner_target) <= 0.000001 {
                release_active = false;
            }
            if left {
                memory.left_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.left_support_weight.is_none() {
                    memory.left_support_weight = Some(weight);
                }
                memory.left_release_active = release_active;
            } else {
                memory.right_foot_target = Some(owner_target);
                if state_delta_seconds > 0.0 || memory.right_support_weight.is_none() {
                    memory.right_support_weight = Some(weight);
                }
                memory.right_release_active = release_active;
            }
            let target = rig_origin + rig_rotation * owner_target;
            if left {
                memory.left_foot_world_target = Some(target);
            } else {
                memory.right_foot_world_target = Some(target);
            }
            let remembered = if left {
                memory.left_leg
            } else {
                memory.right_leg
            };
            let canonical_pole = canonical_knee_pole(side);
            let remembered = remembered.filter(|pole| pole.dot(canonical_pole) > 0.2);
            let pole = pole_to_world(rig_rotation, remembered.unwrap_or(canonical_pole));
            if let Some(solution) = solve_two_bone(
                upper_snapshot.global.translation(),
                lower_snapshot.global.translation(),
                foot_position,
                target,
                upper_length,
                lower_length,
                pole,
            ) {
                apply_two_bone_solution(upper, lower, foot, solution, &parents, &mut transforms);
                let bend = (solution.knee - upper_snapshot.global.translation())
                    .reject_from_normalized(solution.end_direction);
                if let Some(valid) = bend.try_normalize() {
                    if left {
                        memory.left_leg = Some(pole_to_owner(rig_rotation, valid));
                    } else {
                        memory.right_leg = Some(pole_to_owner(rig_rotation, valid));
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

fn raised_footwork_posture_is_valid(skeleton: &SkeletonState) -> bool {
    skeleton.grounded && skeleton.posture == Posture::Upright
}

fn terrain_leg_has_support(weight: f32) -> bool {
    weight > 0.05
}

/// World-space plant confidence used by diagnostics. Procedural guard movement
/// has exactly one support foot while the other follows its clearance arc.
pub(crate) fn locomotion_support_weights(skeleton: &SkeletonState) -> (f32, f32) {
    let speed = skeleton.animation_speed();
    if !skeleton.grounded || skeleton.action != SkeletonAction::None {
        return (0.0, 0.0);
    }
    if speed <= 0.05 {
        return (1.0, 1.0);
    }
    if skeleton.weapon_guard == WeaponGuardState::Raised
        && skeleton.action == SkeletonAction::None
        && skeleton.raised_locomotion.active
    {
        let swing_left = skeleton.raised_locomotion.swing_foot == LeadFoot::Left;
        ((!swing_left) as u8 as f32, swing_left as u8 as f32)
    } else {
        let (left, right) = gait_support_weights(locomotion_profile(skeleton), skeleton.gait_phase);
        let moving = smoothstep(0.05, 0.75, speed);
        (1.0 - (1.0 - left) * moving, 1.0 - (1.0 - right) * moving)
    }
}

const MAX_PRESENTATION_SAMPLE_GAP: u64 = 32;

fn presentation_tick_delta(previous: Option<u64>, current: u64) -> Option<u64> {
    match previous {
        None => Some(0),
        Some(previous) if current >= previous => {
            let delta = current - previous;
            (delta <= MAX_PRESENTATION_SAMPLE_GAP).then_some(delta)
        }
        Some(_) => None,
    }
}

/// Adds bounded inertial body response from server-observed world
/// acceleration transformed through the current presentation body frame.
/// Retained angles are client presentation only.
pub(super) fn apply_locomotion_body_response(
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
                .is_some_and(|value| value != skeleton.posture)
            || next
                .last_action
                .is_some_and(|value| value != skeleton.action)
            || next
                .last_grounded
                .is_some_and(|value| value != skeleton.grounded);
        if discontinuous || !skeleton.grounded || skeleton.action != SkeletonAction::None {
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
        next.last_posture = Some(skeleton.posture);
        next.last_action = Some(skeleton.action);
        next.last_grounded = Some(skeleton.grounded);
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

fn body_response_target(acceleration: Vec3) -> Vec2 {
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

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn ankle_sole_offset(_rotation: Quat) -> f32 {
    0.085
}

fn anatomical_side(rig_rotation: Quat, rig_origin: Vec3, hip: Vec3, left: bool) -> f32 {
    let hip_x = (rig_rotation.inverse() * (hip - rig_origin)).x;
    if hip_x.abs() > 0.001 {
        hip_x.signum()
    } else if left {
        -1.0
    } else {
        1.0
    }
}

fn constrain_foot_to_track(world: Vec3, rig_origin: Vec3, rig_rotation: Quat, side: f32) -> Vec3 {
    let mut local = rig_rotation.inverse() * (world - rig_origin);
    let signed_x = (local.x * side).clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    local.x = signed_x * side;
    rig_origin + rig_rotation * local
}

fn plan_guard_step_endpoint(
    step_origin: Vec3,
    step_rotation: Quat,
    mut stance_local: Vec3,
    local_direction: Vec2,
    step_length: f32,
    left: bool,
    opposite_plant: Vec3,
) -> Vec3 {
    // Cascadeur's authored lateral axis is opposite the conventional Bevy
    // anatomical assumption. Derive the corridor from the actual pose rather
    // than assigning a sign from the semantic bone name.
    let side = if stance_local.x.abs() > 0.001 {
        stance_local.x.signum()
    } else if left {
        -1.0
    } else {
        1.0
    };
    let lateral_travel = local_direction.x * step_length;
    let authored_track = (stance_local.x * side)
        .abs()
        .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    let moving_toward_side = lateral_travel * side > 0.001;
    let mut track = if lateral_travel.abs() <= 0.001 {
        authored_track
    } else if moving_toward_side {
        (lateral_travel.abs() + FOOT_TRACK_INNER).min(FOOT_TRACK_OUTER)
    } else {
        FOOT_TRACK_INNER
    };
    let future_origin = step_origin
        + step_rotation * Vec3::new(local_direction.x, 0.0, local_direction.y) * step_length;
    let opposite_local = step_rotation.inverse() * (opposite_plant - future_origin);
    // Separation is an anatomical lateral-track contract. Fore/aft spacing
    // must not be credited toward it or feet can converge onto one tightrope.
    let separation_track = opposite_local.x * side + GUARD_TARGET_INTER_FOOT_SEPARATION;
    track = track
        .max(separation_track)
        .clamp(FOOT_TRACK_INNER, FOOT_TRACK_OUTER);
    stance_local.x = track * side;
    future_origin + step_rotation * stance_local
}

fn guard_step_sequence_delta(previous: u32, current: u32) -> u32 {
    current.wrapping_sub(previous)
}

fn constrain_guard_swing_to_live_corridor(
    target: Vec3,
    support: Vec3,
    rig_origin: Vec3,
    rig_rotation: Quat,
    side: f32,
) -> Vec3 {
    let mut local = rig_rotation.inverse() * (target - rig_origin);
    let support_local = rig_rotation.inverse() * (support - rig_origin);
    let required_track = support_local.x * side + GUARD_TARGET_INTER_FOOT_SEPARATION;
    let signed_track = (local.x * side)
        .max(FOOT_TRACK_INNER)
        .max(required_track)
        .min(FOOT_TRACK_OUTER);
    local.x = signed_track * side;
    rig_origin + rig_rotation * local
}

fn terrain_conformed_guard_target(mut flat_target: Vec3, terrain_height: Option<f32>) -> Vec3 {
    if let Some(height) = terrain_height {
        flat_target.y = height + 0.085;
    }
    flat_target
}

fn maximum_reach(upper_length: f32, lower_length: f32) -> f32 {
    (upper_length * upper_length
        + lower_length * lower_length
        + 2.0 * upper_length * lower_length * MIN_KNEE_FLEXION.cos())
    .sqrt()
}

fn landing_maximum_reach(
    upper_length: f32,
    lower_length: f32,
    authored_reach: f32,
    compression: f32,
) -> f32 {
    let reserved_reach = maximum_reach(upper_length, lower_length);
    let full_reach = upper_length + lower_length - 0.0001;
    let released_reach = authored_reach.clamp(reserved_reach, full_reach);
    let reserve_weight = smoothstep(
        LANDING_KNEE_RESERVE_RELEASE_COMPRESSION,
        LANDING_KNEE_RESERVE_FULL_COMPRESSION,
        compression,
    );
    released_reach.lerp(reserved_reach, reserve_weight)
}

fn constrain_target_to_reach(target: Vec3, root: Vec3, maximum_reach: f32) -> Vec3 {
    let vertical = target.y - root.y;
    let maximum_horizontal = (maximum_reach * maximum_reach - vertical * vertical)
        .max(0.0)
        .sqrt();
    let horizontal = (target - root).xz().clamp_length_max(maximum_horizontal);
    Vec3::new(root.x + horizontal.x, target.y, root.z + horizontal.y)
}

fn plant_is_continuous(plant: Vec3, current_foot: Vec3) -> bool {
    plant.is_finite()
        && current_foot.is_finite()
        && plant.distance(current_foot) <= MAX_PLANT_DISCONTINUITY
}

fn advance_foot_target(previous: Option<Vec3>, desired: Vec3, delta_seconds: f32) -> Vec3 {
    let Some(previous) = previous.filter(|position| position.is_finite()) else {
        return desired;
    };
    if !desired.is_finite() {
        return previous;
    }
    if previous.distance(desired) > MAX_PLANT_DISCONTINUITY {
        return desired;
    }
    let maximum_step = (MAX_FOOT_TARGET_SPEED * delta_seconds.max(0.0)).min(MAX_FOOT_TARGET_STEP);
    previous + (desired - previous).clamp_length_max(maximum_step)
}

fn advance_pelvis_shift(current: f32, desired: f32, delta_seconds: f32) -> f32 {
    let maximum_step =
        (PELVIS_CORRECTION_SPEED * delta_seconds.max(0.0)).min(MAX_PELVIS_CORRECTION_STEP);
    current + (desired - current).clamp(-maximum_step, maximum_step)
}

fn canonical_knee_pole(side: f32) -> Vec3 {
    (Vec3::Z + Vec3::X * side * 0.18).normalize()
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
    solve_two_bone_internal(
        root,
        current_knee,
        current_end,
        target,
        upper_length,
        lower_length,
        pole_direction,
        maximum_reach(upper_length, lower_length),
        true,
    )
}

fn solve_landing_two_bone(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
    compression: f32,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        root,
        current_knee,
        current_end,
        target,
        upper_length,
        lower_length,
        pole_direction,
        landing_maximum_reach(
            upper_length,
            lower_length,
            root.distance(current_end),
            compression,
        ),
        true,
    )
}

fn solve_two_bone_with_reach(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
    maximum_target_reach: f32,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        root,
        current_knee,
        current_end,
        target,
        upper_length,
        lower_length,
        pole_direction,
        maximum_target_reach,
        false,
    )
}

fn solve_two_bone_internal(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
    maximum_target_reach: f32,
    preserve_authored_bend: bool,
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
        maximum_target_reach.min(upper_length + lower_length - 0.0001),
    );
    let end = root + target_direction * distance;
    let along = (upper_length * upper_length - lower_length * lower_length + distance * distance)
        / (2.0 * distance);
    let height = (upper_length * upper_length - along * along)
        .max(0.0)
        .sqrt();
    let pole_bend = pole_direction
        .reject_from_normalized(target_direction)
        .try_normalize();
    let authored_bend = (current_knee - root)
        .reject_from_normalized(target_direction)
        .try_normalize();
    // Preserve authored continuity only while it remains in the anatomical
    // hemisphere. Never flip a valid authored bend through a straight-leg
    // singularity merely to satisfy a pole chosen on the opposite side.
    let stabilized_authored_bend = preserve_authored_bend
        .then_some(authored_bend)
        .flatten()
        .zip(pole_bend)
        .and_then(|(authored, pole)| {
            let alignment = authored.dot(pole);
            (alignment > 0.05)
                .then(|| {
                    pole.lerp(authored, smoothstep(0.05, 0.5, alignment))
                        .try_normalize()
                })
                .flatten()
        });
    let bend = stabilized_authored_bend
        .or(pole_bend)
        .or(preserve_authored_bend.then_some(authored_bend).flatten())
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
    fn stable_pole_overrides_an_authored_knee_in_the_opposite_hemisphere() {
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
        assert!(solved.knee.z < 0.0);
    }

    #[test]
    fn authored_knee_bend_is_preserved_within_the_stable_pole_hemisphere() {
        let solved = solve_two_bone(
            Vec3::ZERO,
            Vec3::new(0.1, -1.0, 0.1),
            Vec3::NEG_Y * 2.0,
            Vec3::new(0.0, -1.8, 0.0),
            1.0,
            1.0,
            Vec3::Z,
        )
        .unwrap();
        assert!(solved.knee.x > 0.0);
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
    fn run_has_unconstrained_flight_but_walk_retains_support() {
        for phase in [0.25, 0.75] {
            let (run_left, run_right) = gait_support_weights(RUN_LOCOMOTION_PROFILE, phase);
            assert_eq!((run_left, run_right), (0.0, 0.0));
            let (walk_left, walk_right) = gait_support_weights(WALK_LOCOMOTION_PROFILE, phase);
            assert!(walk_left + walk_right > 0.0);
        }

        let phase_step = gait_cycle_phase_delta(RUN_LOCOMOTION_PROFILE, 5.5, 1.0 / 64.0);
        let mut longest = 0_u32;
        let mut current = 0_u32;
        for frame in 0..=64 {
            let phase = frame as f32 * phase_step;
            let (left, right) = gait_support_weights(RUN_LOCOMOTION_PROFILE, phase);
            if left <= 0.001 && right <= 0.001 {
                current += 1;
                longest = longest.max(current);
            } else {
                current = 0;
            }
        }
        let observed_seconds = longest.saturating_sub(1) as f32 / 64.0;
        assert!((0.085..=0.12).contains(&observed_seconds));
    }

    #[test]
    fn phase_owned_height_has_two_contact_minima_and_two_equal_peaks() {
        for phase in [0.0, 0.5] {
            let state = SkeletonState {
                local_velocity: Vec3::NEG_Z * 5.5,
                gait_phase: phase,
                ..default()
            };
            assert!(locomotion_height_wave(&state).abs() < 0.0001);
        }
        for phase in [0.25, 0.75] {
            let state = SkeletonState {
                local_velocity: Vec3::NEG_Z * 5.5,
                gait_phase: phase,
                ..default()
            };
            assert!(
                (locomotion_height_wave(&state) - RUN_LOCOMOTION_PROFILE.flight_apex_metres).abs()
                    < 0.0001
            );
        }
    }

    #[test]
    fn locomotion_height_amplitude_covers_walk_run_guard_and_crouch() {
        let moving = |speed, posture, guard| SkeletonState {
            local_velocity: Vec3::NEG_Z * speed,
            posture,
            weapon_guard: guard,
            raised_locomotion: RaisedLocomotionIntent {
                active: guard == WeaponGuardState::Raised,
                local_direction: Vec2::NEG_Y,
                speed,
                ..default()
            },
            ..default()
        };
        assert!(
            (locomotion_height_wave(&SkeletonState {
                gait_phase: 0.25,
                ..moving(2.0, Posture::Upright, WeaponGuardState::Lowered,)
            }) - WALK_LOCOMOTION_PROFILE.bounce_metres)
                .abs()
                < 0.0001
        );
        assert!(
            (locomotion_height_wave(&SkeletonState {
                gait_phase: 0.25,
                ..moving(5.5, Posture::Upright, WeaponGuardState::Lowered,)
            }) - RUN_LOCOMOTION_PROFILE.flight_apex_metres)
                .abs()
                < 0.0001
        );
        assert!(
            (locomotion_height_wave(&SkeletonState {
                gait_phase: 0.25,
                ..moving(2.0, Posture::Upright, WeaponGuardState::Raised)
            }) - RAISED_GUARD_LOCOMOTION_PROFILE.bounce_metres)
                .abs()
                < 0.0001
        );
        assert!(
            (locomotion_height_wave(&SkeletonState {
                gait_phase: 0.25,
                ..moving(1.5, Posture::Crouched, WeaponGuardState::Lowered,)
            }) - CROUCH_LOCOMOTION_PROFILE.bounce_metres)
                .abs()
                < 0.0001
        );
        assert!(
            (authored_height_compensation(&moving(
                2.0,
                Posture::Upright,
                WeaponGuardState::Lowered,
            )) - AUTHORED_ORDINARY_PASSING_RISE_METRES)
                .abs()
                < 0.0001
        );
        assert_eq!(
            authored_height_compensation(&moving(2.0, Posture::Upright, WeaponGuardState::Raised,)),
            0.0
        );
        assert_eq!(
            authored_height_compensation(&moving(
                1.5,
                Posture::Crouched,
                WeaponGuardState::Lowered,
            )),
            0.0
        );
        let mut specialized = moving(2.0, Posture::Upright, WeaponGuardState::Lowered);
        specialized.animation_pack = "humanoid_sword_and_shield".to_owned();
        assert_eq!(authored_height_compensation(&specialized), 0.0);
    }

    #[test]
    fn support_grounding_places_the_sole_on_the_visual_floor() {
        assert!((support_grounding_target(1.135, 1.0, 0.085) + 0.05).abs() < 0.0001);
        assert_eq!(
            support_grounding_target(2.0, 1.0, 0.085),
            MINIMUM_SUPPORT_GROUNDING_OFFSET_METRES
        );
        assert_eq!(
            support_grounding_target(0.5, 1.0, 0.085),
            MAXIMUM_SUPPORT_GROUNDING_OFFSET_METRES
        );
    }

    #[test]
    fn support_grounding_is_limited_to_ordinary_grounded_locomotion() {
        let moving = SkeletonState {
            local_velocity: Vec3::NEG_Z * 2.0,
            ..default()
        };
        assert!(ordinary_support_grounding_is_active(&moving));
        assert!(!ordinary_support_grounding_is_active(&SkeletonState {
            weapon_guard: WeaponGuardState::Raised,
            ..moving.clone()
        }));
        assert!(!ordinary_support_grounding_is_active(&SkeletonState {
            grounded: false,
            ..moving.clone()
        }));
        assert!(!ordinary_support_grounding_is_active(&SkeletonState {
            action: SkeletonAction::Attack,
            ..moving
        }));
    }

    #[test]
    fn central_height_normalization_applies_only_during_active_locomotion() {
        let moving = SkeletonState {
            local_velocity: Vec3::NEG_Z * 2.0,
            ..default()
        };
        assert_eq!(locomotion_normalization_target(&moving), 1.0);
        assert_eq!(
            locomotion_normalization_target(&SkeletonState::default()),
            0.0
        );
        assert_eq!(
            locomotion_normalization_target(&SkeletonState {
                weapon_guard: WeaponGuardState::Raised,
                ..default()
            }),
            0.0
        );
    }

    #[test]
    fn body_response_and_landing_compression_are_bounded() {
        let forward = body_response_target(Vec3::Z * 12.0);
        let braking = body_response_target(Vec3::NEG_Z * 12.0);
        let lateral = body_response_target(Vec3::X * 12.0);
        assert!((-12.0..=-8.0).contains(&forward.x.to_degrees()));
        assert!((6.0..=10.0).contains(&braking.x.to_degrees()));
        assert!((6.0..=10.0).contains(&lateral.y.abs().to_degrees()));
        assert!(
            body_response_target(Vec3::new(40.0, 0.0, 40.0))
                .length()
                .to_degrees()
                <= 15.0
        );
        assert_eq!(
            landing_compression_for_impact(WALK_LOCOMOTION_PROFILE, 0.5),
            0.0
        );
        assert!((0.04..=0.08).contains(&landing_compression_for_impact(
            WALK_LOCOMOTION_PROFILE,
            4.5,
        )));
        assert_eq!(presentation_tick_delta(Some(10), 10), Some(0));
        assert_eq!(presentation_tick_delta(Some(10), 14), Some(4));
        assert_eq!(presentation_tick_delta(Some(14), 2), None);
        assert_eq!(presentation_tick_delta(Some(2), 40), None);
    }

    #[test]
    fn landing_leg_solve_preserves_the_foot_with_real_knee_flexion() {
        let root = Vec3::new(0.0, 1.75, 0.0);
        let knee = Vec3::new(0.0, 0.85, 0.08);
        let compressed_foot = Vec3::new(0.0, -0.05, 0.0);
        let target = compressed_foot + Vec3::Y * 0.05;
        let upper = root.distance(knee);
        let lower = knee.distance(compressed_foot);
        let solution = solve_two_bone(root, knee, compressed_foot, target, upper, lower, Vec3::Z)
            .expect("compressed leg should reach its pre-compression foot");
        let flexion = 180.0
            - (root - solution.knee)
                .angle_between(solution.end - solution.knee)
                .to_degrees();
        assert!(solution.end.distance(target) <= 0.0001);
        assert!(flexion >= 10.0);

        let mut retained = None;
        let first = retain_landing_foot_target(&mut retained, target);
        let recovered_frame =
            retain_landing_foot_target(&mut retained, Vec3::new(0.0, -0.02, 0.03));
        assert_eq!(first, target);
        assert_eq!(recovered_frame, first);
        assert!(!landing_plant_is_discontinuous(
            Some(10),
            10,
            Some(Vec3::ZERO),
            Vec3::ZERO,
        ));
        assert!(landing_plant_is_discontinuous(
            Some(10),
            9,
            Some(Vec3::ZERO),
            Vec3::ZERO,
        ));
        assert!(landing_plant_is_discontinuous(
            Some(10),
            11,
            Some(Vec3::ZERO),
            Vec3::X * 3.0,
        ));

        let mut height = LocomotionHeightState {
            landing_left_foot_target: retained,
            landing_right_foot_target: Some(first),
            landing_plant_owner_position: Some(Vec3::ZERO),
            landing_plant_tick: Some(10),
            landing_plant_resync_tick: Some(10),
            ..default()
        };
        clear_landing_foot_plants(&mut height);
        assert!(height.landing_left_foot_target.is_none());
        assert!(height.landing_right_foot_target.is_none());
        assert!(height.landing_plant_owner_position.is_none());
        assert!(height.landing_plant_tick.is_none());
        assert!(height.landing_plant_resync_tick.is_none());
    }

    #[test]
    fn landing_knee_reserve_releases_smoothly_for_late_recovery_reach() {
        let upper = 0.9;
        let lower = 0.9;
        let authored_reach = 1.79;
        let reserved_reach = maximum_reach(upper, lower);

        let peak_reach = landing_maximum_reach(upper, lower, authored_reach, 0.04);
        assert!((peak_reach - reserved_reach).abs() <= f32::EPSILON);
        let peak_flexion = 180.0
            - ((upper * upper + lower * lower - peak_reach * peak_reach) / (2.0 * upper * lower))
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
        assert!(peak_flexion >= 10.0);

        let release_samples = [0.04, 0.033, 0.026, 0.019, 0.012];
        let reaches = release_samples
            .map(|compression| landing_maximum_reach(upper, lower, authored_reach, compression));
        for pair in reaches.windows(2) {
            assert!(pair[1] >= pair[0]);
            assert!(pair[1] - pair[0] < 0.01);
        }
        assert!((reaches[4] - authored_reach).abs() <= 0.0001);

        let target = Vec3::ZERO;
        let current_knee = Vec3::new(0.0, 0.89, 0.1);
        for compression in [0.012, 0.008, 0.004, 0.001] {
            let root = Vec3::Y * (authored_reach - compression);
            let current_end = Vec3::NEG_Y * compression;
            let solution = solve_landing_two_bone(
                root,
                current_knee,
                current_end,
                target,
                upper,
                lower,
                Vec3::Z,
                compression,
            )
            .expect("late landing recovery should remain solvable");
            assert!(solution.end.distance(target) <= 0.0001);
        }
    }

    #[test]
    fn raised_guard_movement_reports_exactly_one_procedural_support_foot() {
        for (lead, phase) in [
            (LeadFoot::Left, 0.0),
            (LeadFoot::Left, 0.5),
            (LeadFoot::Right, 0.25),
            (LeadFoot::Right, 0.75),
        ] {
            let skeleton = SkeletonState {
                weapon_guard: WeaponGuardState::Raised,
                lead_foot: lead,
                gait_phase: phase,
                local_velocity: Vec3::NEG_Z * 2.0,
                raised_locomotion: RaisedLocomotionIntent {
                    active: true,
                    local_direction: Vec2::NEG_Y,
                    speed: 2.0,
                    swing_foot: lead,
                    step_sequence: 0,
                },
                ..default()
            };
            let (left, right) = locomotion_support_weights(&skeleton);
            assert_eq!(left + right, 1.0);
            let expected_swing_left = lead == LeadFoot::Left;
            assert_eq!(left, (!expected_swing_left) as u8 as f32);
            assert_eq!(right, expected_swing_left as u8 as f32);
        }
        let idle = SkeletonState {
            weapon_guard: WeaponGuardState::Raised,
            local_velocity: Vec3::ZERO,
            ..default()
        };
        assert_eq!(locomotion_support_weights(&idle), (1.0, 1.0));
    }

    #[test]
    fn ordinary_idle_and_stopping_restore_symmetric_terrain_support() {
        let idle = SkeletonState {
            gait_phase: 0.25,
            ..default()
        };
        assert_eq!(locomotion_support_weights(&idle), (1.0, 1.0));

        let stopping = SkeletonState {
            gait_phase: 0.25,
            local_velocity: Vec3::NEG_Z * 0.2,
            ..default()
        };
        let (left, right) = locomotion_support_weights(&stopping);
        let (raw_left, raw_right) =
            gait_support_weights(locomotion_profile(&stopping), stopping.gait_phase);
        assert!(left > raw_left && right > raw_right);
        assert!(left > 0.5 && right > 0.5);
    }

    #[test]
    fn actions_and_zero_weight_swing_legs_preserve_authored_fk() {
        let action = SkeletonState {
            action: SkeletonAction::Attack,
            local_velocity: Vec3::NEG_Z * 5.5,
            gait_phase: 0.0,
            ..default()
        };
        assert_eq!(locomotion_support_weights(&action), (0.0, 0.0));
        assert!(!terrain_leg_has_support(0.0));
        assert!(!terrain_leg_has_support(0.01));
        assert!(terrain_leg_has_support(0.1));
    }

    #[test]
    fn analog_guard_speed_scales_step_reach_without_unbounded_strides() {
        assert!(guard_step_length(1.0) < guard_step_length(2.0));
        assert_eq!(guard_step_length(0.0), 0.28);
        assert_eq!(guard_step_length(100.0), 0.42);
    }

    #[test]
    fn guard_step_planner_preserves_tracks_and_separation_in_all_directions() {
        let origin = Vec3::ZERO;
        let rotation = Quat::IDENTITY;
        let step = guard_step_length(2.0);
        for direction in [
            Vec2::X,
            Vec2::NEG_X,
            Vec2::Y,
            Vec2::NEG_Y,
            Vec2::ONE.normalize(),
            Vec2::new(-1.0, -1.0).normalize(),
        ] {
            for left in [true, false] {
                let side = if left { -1.0 } else { 1.0 };
                let stance = Vec3::new(0.12 * side, -0.85, if left { -0.2 } else { 0.2 });
                let opposite = Vec3::new(-0.12 * side, -0.85, -stance.z);
                let target = plan_guard_step_endpoint(
                    origin, rotation, stance, direction, step, left, opposite,
                );
                let future_origin = Vec3::new(direction.x, 0.0, direction.y) * step;
                let local = rotation.inverse() * (target - future_origin);
                assert!(local.x * side >= FOOT_TRACK_INNER - 0.0001);
                assert!(local.x * side <= FOOT_TRACK_OUTER + 0.0001);
                let separation = target.xz().distance(opposite.xz());
                assert!(
                    separation >= GUARD_TARGET_INTER_FOOT_SEPARATION - 0.0001,
                    "direction={direction:?} left={left} target={target:?} opposite={opposite:?} separation={separation}"
                );
                assert!(
                    (target.x - opposite.x).abs() >= GUARD_TARGET_INTER_FOOT_SEPARATION - 0.0001
                );
            }
        }
    }

    #[test]
    fn live_swing_corridor_preserves_separation_between_handoffs() {
        let root = Vec3::new(-0.2, 2.8, 0.0);
        let rotation = Quat::from_rotation_y(std::f32::consts::PI);
        let support = Vec3::new(0.21, 1.95, 0.02);
        let unconstrained = Vec3::new(0.18, 2.02, 0.02);
        let constrained =
            constrain_guard_swing_to_live_corridor(unconstrained, support, root, rotation, 1.0);
        let local = rotation.inverse() * (constrained - root);
        assert!(local.x >= FOOT_TRACK_INNER);
        assert!(
            constrained.xz().distance(support.xz()) >= GUARD_TARGET_INTER_FOOT_SEPARATION - 0.0001
        );
        assert!((constrained.x - support.x).abs() >= GUARD_TARGET_INTER_FOOT_SEPARATION - 0.0001);
    }

    #[test]
    fn skipped_full_cycle_has_distinct_semantic_step_identity() {
        assert_eq!(guard_step_sequence_delta(41, 42), 1);
        assert_eq!(guard_step_sequence_delta(41, 43), 2);
        assert_eq!(guard_step_sequence_delta(u32::MAX, 0), 1);
    }

    #[test]
    fn terrain_height_is_transient_during_an_active_guard_step() {
        let flat = Vec3::new(0.2, -0.85, 0.3);
        let elevated = terrain_conformed_guard_target(flat, Some(1.25));
        assert_eq!(elevated.y, 1.335);
        assert_eq!(terrain_conformed_guard_target(flat, None), flat);
        assert_eq!(flat.y, -0.85);
    }

    #[test]
    fn inactive_postures_require_raised_footwork_reset() {
        for (grounded, posture) in [
            (false, Posture::Airborne),
            (true, Posture::Crouched),
            (true, Posture::Prone),
        ] {
            let skeleton = SkeletonState {
                grounded,
                posture,
                weapon_guard: WeaponGuardState::Raised,
                ..default()
            };
            assert!(!raised_footwork_posture_is_valid(&skeleton));
        }
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
    fn gait_mirror_is_an_involution() {
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
    fn foot_tracks_prevent_crossing_and_keep_minimum_separation() {
        let rotation = Quat::from_rotation_y(0.6);
        let origin = Vec3::new(2.0, 0.0, -3.0);
        let left = constrain_foot_to_track(origin, origin, rotation, -1.0);
        let right = constrain_foot_to_track(origin, origin, rotation, 1.0);
        let left_local = rotation.inverse() * (left - origin);
        let right_local = rotation.inverse() * (right - origin);
        assert!(left_local.x < 0.0 && right_local.x > 0.0);
        assert!(right_local.x - left_local.x >= MIN_INTER_FOOT_SEPARATION - 0.0001);
    }

    #[test]
    fn plant_releases_only_on_a_real_discontinuity_and_reach_slides_continuously() {
        let foot = Vec3::new(-0.2, -1.8, 0.0);
        assert!(plant_is_continuous(foot, foot));
        assert!(!plant_is_continuous(Vec3::NAN, foot));
        assert!(!plant_is_continuous(foot + Vec3::X * 2.1, foot));

        let root = Vec3::Y;
        let target = Vec3::new(-0.2, 0.0, 3.0);
        let constrained = constrain_target_to_reach(target, root, 1.5);
        assert!(constrained.distance(root) <= 1.5001);
        assert_eq!(constrained.y, target.y);
    }

    #[test]
    fn sparse_swing_targets_advance_at_a_bounded_speed() {
        let previous = Vec3::ZERO;
        let desired = Vec3::X;
        let advanced = advance_foot_target(Some(previous), desired, 1.0 / 64.0);
        assert!((advanced.length() - 0.1875).abs() < 0.0001);
        let hitch_advanced = advance_foot_target(Some(previous), desired, 1.0);
        assert!((hitch_advanced.length() - MAX_FOOT_TARGET_STEP).abs() < 0.0001);
        assert_eq!(advance_foot_target(None, desired, 1.0 / 64.0), desired);
        assert_eq!(
            advance_foot_target(Some(previous), Vec3::NAN, 1.0 / 64.0),
            previous
        );
    }

    #[test]
    fn pelvis_smoothing_depends_on_elapsed_time_not_evaluation_count() {
        fn simulate(step_seconds: f32, evaluations: usize) -> f32 {
            (0..evaluations).fold(0.0, |current, _| {
                advance_pelvis_shift(current, -1.0, step_seconds)
            })
        }

        let at_64_hz = simulate(1.0 / 64.0, 16);
        let at_128_hz = simulate(1.0 / 128.0, 32);
        assert!((at_64_hz - at_128_hz).abs() < 0.0001);
        assert!((at_64_hz + 0.4).abs() < 0.0001);

        let after_hitch = advance_pelvis_shift(0.0, -1.0, 1.0);
        assert!((after_hitch + MAX_PELVIS_CORRECTION_STEP).abs() < 0.0001);
    }

    #[test]
    fn leg_solver_keeps_minimum_flexion_and_anatomical_hemisphere() {
        let pole = canonical_knee_pole(-1.0);
        let solved = solve_two_bone(
            Vec3::ZERO,
            Vec3::new(0.0, -1.0, -0.1),
            Vec3::NEG_Y * 2.0,
            Vec3::NEG_Y * 20.0,
            1.0,
            1.0,
            pole,
        )
        .unwrap();
        assert!(solved.end.length() <= maximum_reach(1.0, 1.0) + 0.0001);
        let bend = (solved.knee)
            .reject_from_normalized(solved.end_direction)
            .normalize();
        assert!(bend.dot(pole) > 0.0);
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
