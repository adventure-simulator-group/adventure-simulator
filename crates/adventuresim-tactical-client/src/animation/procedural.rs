use std::collections::BTreeMap;

use adventuresim_tactical_core::prelude::*;
use bevy::{math::Affine3A, prelude::*};

use super::{AnimationPlayback, AuthoredBindTransform, ImpactReaction, PresentedSkeleton};

mod rig;
pub(crate) use rig::*;

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct ProceduralLookState {
    base_rotation: Quat,
    applied_rotation: Quat,
    evaluation_tick: u64,
}

#[derive(Resource, Debug, Default)]
pub(super) struct FixedTickPoseCache {
    tick: Option<u64>,
    bones: BTreeMap<Entity, Transform>,
}

/// Tools can render gameplay, side, and front views from one logical sample.
/// Preserve the first complete post-procedural local pose for that fixed tick
/// and restore it before propagation on every repeated render. Live gameplay
/// leaves the fixed clock unset and never enters this path.
pub(super) fn stabilize_repeated_fixed_tick_pose(
    clock: Res<ProceduralAnimationClock>,
    mut cache: ResMut<FixedTickPoseCache>,
    mut bones: Query<(Entity, &mut Transform), With<HumanoidBone>>,
) {
    let Some((tick, _)) = clock.fixed_step() else {
        cache.tick = None;
        cache.bones.clear();
        return;
    };
    if cache.tick != Some(tick) {
        cache.tick = Some(tick);
        cache.bones.clear();
        cache.bones.extend(
            bones
                .iter_mut()
                .map(|(entity, transform)| (entity, *transform)),
        );
        return;
    }
    for (entity, mut transform) in &mut bones {
        if let Some(cached) = cache.bones.get(&entity) {
            *transform = *cached;
        }
    }
}

fn additive_look_rotation(
    current: Quat,
    previous: Option<ProceduralLookState>,
    evaluation_tick: u64,
    offset: Quat,
) -> (Quat, ProceduralLookState) {
    let base_rotation = previous.map_or(current, |previous| {
        if previous.evaluation_tick == evaluation_tick
            || current.angle_between(previous.applied_rotation) <= 0.000_01
        {
            previous.base_rotation
        } else {
            current
        }
    });
    let applied_rotation = (base_rotation * offset).normalize();
    (
        applied_rotation,
        ProceduralLookState {
            base_rotation,
            applied_rotation,
            evaluation_tick,
        },
    )
}

/// Procedural facing is an additive post-FK layer. Sparse authored clips do not
/// necessarily rewrite every torso bone, so retain the pre-look local rotation
/// and reuse it when the same logical pose reaches this pass again.
pub(super) fn apply_head_and_torso_look(
    mut commands: Commands,
    owners: Query<(&CharacterLook, &PresentedSkeleton)>,
    mut bones: Query<(
        Entity,
        &HumanoidBone,
        &mut Transform,
        Option<&mut ProceduralLookState>,
    )>,
) {
    for (entity, bone, mut transform, state) in &mut bones {
        let Ok((look, skeleton)) = owners.get(bone.owner) else {
            continue;
        };
        let pitch = look.pitch.clamp(-0.65, 0.65);
        let directional_yaw = skeleton.action_direction().x.clamp(-1.0, 1.0) * 0.35;
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
        let offset = Quat::from_euler(EulerRot::YXZ, directional_yaw * weight, pitch * weight, 0.0);
        let previous = state.as_deref().copied();
        let (rotation, next) = additive_look_rotation(
            transform.rotation,
            previous,
            skeleton.locomotion_sample_tick,
            offset,
        );
        transform.rotation = rotation;
        if let Some(mut state) = state {
            *state = next;
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

/// Applies whole-body reflection for same-pack semantic pose fallbacks.
/// Ordinary gait parity is already baked into distinct endpoint clips before
/// FK blending and must never pass through this post-blend operation.
pub(super) fn apply_pose_mirroring(
    playbacks: Query<&AnimationPlayback>,
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(Query<&Transform>, TransformHelper, Query<&mut Transform>)>,
) {
    for (owner, topology) in &rigs {
        let Ok(playback) = playbacks.get(owner) else {
            continue;
        };
        let whole_body_weight = playback.whole_body_mirror.clamp(0.0, 1.0);
        if whole_body_weight <= f32::EPSILON {
            continue;
        }
        let Some(rig_scene) = topology.rig_scene() else {
            continue;
        };
        let local_snapshots = {
            let locals = transforms.p0();
            BoneRole::ALL
                .into_iter()
                .filter_map(|role| {
                    let entity = *topology.get(&role)?;
                    Some((role, entity, *locals.get(entity).ok()?))
                })
                .collect::<Vec<_>>()
        };
        let (rig, rig_global) = {
            let helper = transforms.p1();
            let Ok(rig_global) = helper.compute_global_transform(rig_scene) else {
                continue;
            };
            let mut rig = [None; BoneRole::COUNT];
            for (role, entity, local) in local_snapshots {
                let Ok(global) = helper.compute_global_transform(entity) else {
                    continue;
                };
                let parent = parents.get(entity).ok().map(ChildOf::parent);
                let parent_global = parent
                    .and_then(|parent| helper.compute_global_transform(parent).ok())
                    .unwrap_or(GlobalTransform::IDENTITY);
                rig[role.index()] = Some(MirrorBone {
                    entity,
                    local,
                    global,
                    parent,
                    parent_global,
                });
            }
            (rig, rig_global)
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
                let Some(bone) = rig[role.index()].as_ref() else {
                    continue;
                };
                desired_globals
                    .insert(bone.entity, mirrored_global_affine(bone.global, rig_global));
                mirror_weights.insert(bone.entity, whole_body_weight);
            }
        }
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
            let (Some(left), Some(right)) = (
                rig[left_role.index()].as_ref(),
                rig[right_role.index()].as_ref(),
            ) else {
                continue;
            };
            desired_globals.insert(
                left.entity,
                mirrored_global_affine(right.global, rig_global),
            );
            desired_globals.insert(
                right.entity,
                mirrored_global_affine(left.global, rig_global),
            );
            mirror_weights.insert(left.entity, whole_body_weight);
            mirror_weights.insert(right.entity, whole_body_weight);
        }
        let mut bones = transforms.p2();
        for bone in rig.iter().flatten() {
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
    if !skeleton.is_grounded() || skeleton.action_kind() != SkeletonAction::None {
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
    let flight = (half_step * std::f32::consts::PI).sin().powi(2) * profile.flight_apex_metres;
    (grounded + flight) * moving_weight
}

fn authored_height_compensation(skeleton: &SkeletonState) -> f32 {
    if !skeleton.is_grounded()
        || skeleton.action_kind() != SkeletonAction::None
        || skeleton.animation_pack != "humanoid_unarmed"
        || skeleton.posture() != Posture::Upright
        || skeleton.weapon_guard() != WeaponGuardState::Lowered
    {
        return 0.0;
    }
    AUTHORED_ORDINARY_PASSING_RISE_METRES * smoothstep(0.05, 0.75, skeleton.animation_speed())
}

fn locomotion_normalization_target(skeleton: &SkeletonState) -> f32 {
    (skeleton.is_grounded()
        && skeleton.action_kind() == SkeletonAction::None
        && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
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
            next.last_guard = Some(skeleton.weapon_guard());
            next.last_posture = Some(skeleton.posture());
            next.last_action = Some(skeleton.action_kind());
            next.last_grounded = Some(skeleton.is_grounded());
            next.last_landing_sequence = skeleton.landing_sequence;
        }
        let tick_delta =
            presentation_tick_delta(next.evaluation_tick, skeleton.locomotion_sample_tick)
                .unwrap_or_default();
        next.evaluation_tick = Some(skeleton.locomotion_sample_tick);
        let delta_seconds = tick_delta as f32 / LOCOMOTION_SAMPLE_HZ;
        let ordinary_stop = skeleton.is_grounded()
            && skeleton.action_kind() == SkeletonAction::None
            && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
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
        if !skeleton.is_grounded()
            || skeleton.action_kind() != SkeletonAction::None
            || next.landing_compression <= 0.001
        {
            clear_landing_foot_plants(&mut next);
        }
        let compensation =
            grounded_height_wave(skeleton.gait_phase, next.authored_rise_compensation);
        let raw_wave = next.amplitude - compensation;
        let state_changed = next.last_guard != Some(skeleton.weapon_guard())
            || next.last_posture != Some(skeleton.posture())
            || next.last_action != Some(skeleton.action_kind())
            || next.last_grounded != Some(skeleton.is_grounded());
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
        next.last_guard = Some(skeleton.weapon_guard());
        next.last_posture = Some(skeleton.posture());
        next.last_action = Some(skeleton.action_kind());
        next.last_grounded = Some(skeleton.is_grounded());
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
    skeleton.is_grounded()
        && skeleton.action_kind() == SkeletonAction::None
        && skeleton.weapon_guard() == WeaponGuardState::Lowered
        && matches!(skeleton.posture(), Posture::Upright | Posture::Crouched)
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
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (owner, rig) in &rigs {
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
            if let (Some(&foot), Some(scene_root)) = (rig.get(&foot_role), rig.rig_scene()) {
                let foot_global = transforms.p0().compute_global_transform(foot).ok();
                let scene_global = transforms.p0().compute_global_transform(scene_root).ok();
                if let (Some(foot_global), Some(scene_global)) = (foot_global, scene_global) {
                    next.target_offset_metres = support_grounding_target(
                        foot_global.translation().y,
                        scene_global.translation().y,
                        MEASURED_ANKLE_SOLE_OFFSET_METRES,
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
    rigs: Query<(Entity, &HumanoidRig)>,
    bones: Query<(Entity, &GlobalTransform), With<HumanoidBone>>,
    parents: Query<&ChildOf>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let mut previous_world_positions = BTreeMap::<Entity, Vec3>::new();
    for (entity, global) in &bones {
        previous_world_positions.insert(entity, global.translation());
    }
    for (owner, rig) in &rigs {
        let Ok((skeleton, mut height, owner_transform)) = owners.get_mut(owner) else {
            continue;
        };
        if !skeleton.is_grounded()
            || skeleton.action_kind() != SkeletonAction::None
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

mod ik;
pub(crate) use ik::{
    ArmIkState, AttackFootworkState, HandIkTarget, HandSide, HeldWeaponConstraint,
    HumanoidIkTargets, LegIkDiagnostics, LegIkState, MEASURED_ANKLE_SOLE_OFFSET_METRES,
    ProceduralAnimationClock, RaisedFootworkState, SOLE_CONTACT_TOLERANCE_METRES,
    locomotion_support_weights,
};
#[cfg(test)]
use ik::{
    FOOT_TRACK_INNER, FOOT_TRACK_OUTER, GUARD_TARGET_INTER_FOOT_SEPARATION,
    MAX_PELVIS_CORRECTION_STEP, MIN_INTER_FOOT_SEPARATION, TwoBoneSolution,
    advance_foot_target_at_speed, advance_pelvis_shift, authored_knee_pole_world,
    balance_recovery_direction, body_response_target, constrain_foot_to_track,
    constrain_guard_swing_to_live_corridor, constrain_target_to_reach, guard_step_sequence_delta,
    landing_maximum_reach, maximum_reach, plan_guard_step_endpoint, plan_settle_landing,
    plant_is_continuous, projected_capture_point, raised_footwork_posture_is_valid,
    retained_plant_requires_release, secondary_grip_world, settle_swing_side, settle_swing_target,
    slope_aligned_world_rotation, sole_is_at_contact, solve_two_bone,
    terrain_conformed_guard_target, terrain_ik_posture_is_valid, terrain_leg_has_support,
};
pub(super) use ik::{
    apply_arm_and_weapon_constraints, apply_locomotion_body_response, apply_terrain_leg_ik,
    refresh_raised_support_after_propagation,
};
use ik::{
    apply_two_bone_solution, canonical_knee_pole, presentation_tick_delta, smoothstep,
    snapshot_chain, solve_landing_two_bone,
};

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn measured_sole_offset_is_shared_by_grounding_and_ik() {
        const GROUNDING_TOLERANCE_METRES: f32 = 0.000_001;

        assert!((MEASURED_ANKLE_SOLE_OFFSET_METRES - 0.085).abs() < f32::EPSILON);
        let target = support_grounding_target(1.135, 1.0, MEASURED_ANKLE_SOLE_OFFSET_METRES);
        assert!(
            (target - -0.05).abs() <= GROUNDING_TOLERANCE_METRES,
            "grounding target {target} differed from the expected offset"
        );
    }

    #[test]
    fn presentation_tick_delta_handles_wrap_and_rejects_large_gaps() {
        assert_eq!(presentation_tick_delta(Some(u64::MAX), 0), Some(1));
        assert_eq!(presentation_tick_delta(Some(1), 100), None);
    }
}

#[cfg(test)]
mod legacy_tests {
    use super::*;

    fn apply_test_two_bone(
        In((upper, lower, end, solution)): In<(Entity, Entity, Entity, TwoBoneSolution)>,
        parents: Query<&ChildOf>,
        mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    ) {
        apply_two_bone_solution(upper, lower, end, solution, &parents, &mut transforms);
    }

    fn test_joint_pose(
        In((lower, end)): In<(Entity, Entity)>,
        helper: TransformHelper,
    ) -> (Vec3, Vec3, Quat) {
        (
            helper
                .compute_global_transform(lower)
                .unwrap()
                .translation(),
            helper.compute_global_transform(end).unwrap().translation(),
            helper.compute_global_transform(end).unwrap().rotation(),
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
            let state = SkeletonState::default()
                .with_local_velocity(Vec3::NEG_Z * 5.5)
                .with_gait_phase(phase);
            assert!(locomotion_height_wave(&state).abs() < 0.0001);
        }
        for phase in [0.25, 0.75] {
            let state = SkeletonState::default()
                .with_local_velocity(Vec3::NEG_Z * 5.5)
                .with_gait_phase(phase);
            assert!(
                (locomotion_height_wave(&state) - RUN_LOCOMOTION_PROFILE.flight_apex_metres).abs()
                    < 0.0001
            );
        }
    }

    #[test]
    fn locomotion_height_amplitude_covers_walk_run_guard_and_crouch() {
        let moving = |speed, posture, guard| {
            SkeletonState::default()
                .with_local_velocity(Vec3::NEG_Z * speed)
                .with_body_state(match posture {
                    Posture::Crouched => BodyState::Grounded(GroundedPosture::Crouched),
                    Posture::Airborne => BodyState::Airborne,
                    _ => BodyState::Grounded(GroundedPosture::Upright),
                })
                .with_weapon_guard(guard)
                .with_raised_locomotion(if guard == WeaponGuardState::Raised {
                    RaisedLocomotionIntent::moving(Vec2::NEG_Y, speed, LeadFoot::Left, 0)
                } else {
                    RaisedLocomotionIntent::default()
                })
        };
        assert!(
            (locomotion_height_wave(
                &moving(2.0, Posture::Upright, WeaponGuardState::Lowered,).with_gait_phase(0.25)
            ) - WALK_LOCOMOTION_PROFILE.bounce_metres)
                .abs()
                < 0.0001
        );
        assert!(
            (locomotion_height_wave(
                &moving(5.5, Posture::Upright, WeaponGuardState::Lowered,).with_gait_phase(0.25)
            ) - RUN_LOCOMOTION_PROFILE.flight_apex_metres)
                .abs()
                < 0.0001
        );
        assert!(
            (locomotion_height_wave(
                &moving(2.0, Posture::Upright, WeaponGuardState::Raised,).with_gait_phase(0.25)
            ) - RAISED_GUARD_LOCOMOTION_PROFILE.bounce_metres)
                .abs()
                < 0.0001
        );
        assert!(
            (locomotion_height_wave(
                &moving(1.5, Posture::Crouched, WeaponGuardState::Lowered,).with_gait_phase(0.25)
            ) - CROUCH_LOCOMOTION_PROFILE.bounce_metres)
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
        let moving = SkeletonState::default().with_local_velocity(Vec3::NEG_Z * 2.0);
        assert!(ordinary_support_grounding_is_active(&moving));
        assert!(!ordinary_support_grounding_is_active(
            &moving.clone().with_weapon_guard(WeaponGuardState::Raised)
        ));
        let mut airborne = moving.clone();
        project_skeleton_locomotion(
            &mut airborne,
            SkeletonLocomotionInput {
                orientation: Quat::IDENTITY,
                linear_velocity: Vec3::NEG_Z * 2.0,
                grounded: false,
                crouching: false,
                delta_seconds: 1.0 / LOCOMOTION_SAMPLE_HZ,
                tick: 1,
            },
        );
        assert!(!ordinary_support_grounding_is_active(&airborne));
        let mut action = moving;
        action.begin_attack(AttackSpec::default(), 0, 1);
        assert!(!ordinary_support_grounding_is_active(&action));
    }

    #[test]
    fn central_height_normalization_applies_only_during_active_locomotion() {
        let moving = SkeletonState::default().with_local_velocity(Vec3::NEG_Z * 2.0);
        assert_eq!(locomotion_normalization_target(&moving), 1.0);
        assert_eq!(
            locomotion_normalization_target(&SkeletonState::default()),
            0.0
        );
        assert_eq!(
            locomotion_normalization_target(
                &SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised)
            ),
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
    fn repeated_look_evaluation_reuses_the_pre_look_rotation() {
        let base = Quat::from_rotation_z(0.17);
        let offset = Quat::from_rotation_x(0.2 * 0.16);
        let (first, state) = additive_look_rotation(base, None, 41, offset);
        let (repeated, repeated_state) = additive_look_rotation(first, Some(state), 41, offset);
        assert!(first.angle_between(repeated) <= 0.000_001);

        // A sparse clip can also leave the bone untouched on the next tick.
        let (next_tick, _) = additive_look_rotation(repeated, Some(repeated_state), 42, offset);
        assert!(first.angle_between(next_tick) <= 0.000_001);

        let authored_next = Quat::from_rotation_z(0.24);
        let (updated, _) = additive_look_rotation(authored_next, Some(repeated_state), 42, offset);
        assert!((authored_next * offset).angle_between(updated) <= 0.000_001);
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
            let skeleton = SkeletonState::default()
                .with_lead_foot(lead)
                .with_gait_phase(phase)
                .with_local_velocity(Vec3::NEG_Z * 2.0)
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_raised_locomotion(RaisedLocomotionIntent::moving(Vec2::NEG_Y, 2.0, lead, 0));
            let (left, right) = locomotion_support_weights(&skeleton);
            assert_eq!(left + right, 1.0);
            let expected_swing_left = lead == LeadFoot::Left;
            assert_eq!(left, (!expected_swing_left) as u8 as f32);
            assert_eq!(right, expected_swing_left as u8 as f32);
        }
        let idle = SkeletonState::default()
            .with_local_velocity(Vec3::ZERO)
            .with_weapon_guard(WeaponGuardState::Raised);
        assert_eq!(locomotion_support_weights(&idle), (1.0, 1.0));
    }

    #[test]
    fn ordinary_swing_support_is_not_filled_in_by_low_speed() {
        let idle = SkeletonState::default().with_gait_phase(0.25);
        assert_eq!(locomotion_support_weights(&idle), (1.0, 1.0));

        let stopping = SkeletonState::default()
            .with_gait_phase(0.25)
            .with_local_velocity(Vec3::NEG_Z * 0.2);
        let (left, right) = locomotion_support_weights(&stopping);
        let (raw_left, raw_right) =
            gait_support_weights(locomotion_profile(&stopping), stopping.gait_phase);
        assert!(left <= raw_left && right <= raw_right);
        assert!(left <= 0.05 || right <= 0.05);
    }

    #[test]
    fn settle_landing_is_ahead_of_the_capture_point() {
        let com = Vec3::new(0.0, 1.0, 0.0);
        let velocity = Vec3::new(0.8, 0.0, -2.0);
        let direction = velocity.normalize();
        let capture = projected_capture_point(com, velocity, 1.0);
        let landing = plan_settle_landing(Vec3::ZERO, Quat::IDENTITY, capture, direction, -1.0);
        assert!((landing - capture).dot(direction) >= 0.119);
        assert!(landing.is_finite());
    }

    #[test]
    fn settle_swing_leaves_and_returns_to_ground_only_at_contact() {
        let start = Vec3::new(-0.12, 0.085, 0.25);
        let landing = Vec3::new(-0.12, 0.085, -0.35);
        assert_eq!(settle_swing_target(start, landing, 0.0), start);
        assert!(settle_swing_target(start, landing, 0.5).y > start.y + 0.09);
        assert!(settle_swing_target(start, landing, 0.75).z < start.z);
        assert!(settle_swing_target(start, landing, 1.0).abs_diff_eq(landing, 0.0001));
    }

    #[test]
    fn airborne_release_is_bounded_to_fifty_three_millimetres_per_tick() {
        let previous = Vec3::ZERO;
        let desired = Vec3::Z;
        let next = advance_foot_target_at_speed(Some(previous), desired, 1.0 / 64.0, 3.4);
        assert!(next.distance(previous) <= 0.053_126);
        assert!(next.distance(desired) < previous.distance(desired));
        assert_eq!(
            advance_foot_target_at_speed(None, desired, 1.0 / 64.0, 3.4),
            desired
        );
        assert_eq!(
            advance_foot_target_at_speed(Some(previous), Vec3::NAN, 1.0 / 64.0, 3.4),
            previous
        );
    }

    #[test]
    fn rendered_sole_contact_uses_the_shared_hierarchy_tolerance() {
        let terrain_height = 2.0;
        let exact_ankle = terrain_height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
        assert!(sole_is_at_contact(exact_ankle, terrain_height));
        assert!(sole_is_at_contact(
            exact_ankle + SOLE_CONTACT_TOLERANCE_METRES - 0.00001,
            terrain_height
        ));
        assert!(!sole_is_at_contact(
            exact_ankle + SOLE_CONTACT_TOLERANCE_METRES + 0.0001,
            terrain_height
        ));
    }

    #[test]
    fn settle_landing_retains_the_actual_swing_track() {
        let origin = Vec3::new(2.0, 0.0, -3.0);
        let rotation = Quat::from_rotation_y(0.7);
        for local_x in [-0.14_f32, 0.14_f32] {
            let swing_start = origin + rotation * Vec3::new(local_x, 0.085, 0.2);
            let side = settle_swing_side(origin, rotation, swing_start, -local_x.signum());
            assert_eq!(side, local_x.signum());
            let capture = origin + rotation * Vec3::NEG_Z * 0.2;
            let landing =
                plan_settle_landing(origin, rotation, capture, rotation * Vec3::NEG_Z, side);
            let landing_local = rotation.inverse() * (landing - origin);
            assert!(landing_local.x * local_x.signum() >= FOOT_TRACK_INNER);
        }
    }

    #[test]
    fn unsupported_idle_recovers_from_feet_toward_projected_com() {
        let com = Vec3::new(0.0, 1.0, -0.45);
        let left = Vec3::new(-0.12, 0.085, 0.0);
        let right = Vec3::new(0.12, 0.085, 0.1);
        let direction = balance_recovery_direction(com, Some(left), Some(right), Vec3::Z);
        assert!(direction.dot(Vec3::NEG_Z) > 0.95);
        let landing = plan_settle_landing(Vec3::ZERO, Quat::IDENTITY, com, direction, -1.0);
        assert!((landing - com).dot(direction) >= 0.119);
    }

    #[test]
    fn exhausted_support_releases_before_the_plant_can_skate() {
        let plant = Vec3::new(0.1, 0.085, -0.4);
        assert!(!retained_plant_requires_release(
            plant,
            plant + Vec3::X * 0.014
        ));
        assert!(retained_plant_requires_release(
            plant,
            plant + Vec3::Z * 0.016
        ));
    }

    #[test]
    fn first_support_solve_seeds_the_authored_knee_in_the_canonical_hemisphere() {
        let hip = Vec3::ZERO;
        let target = Vec3::NEG_Y * 1.8;
        let authored_knee = Vec3::new(0.0, -0.9, 0.2);
        let seeded = authored_knee_pole_world(hip, authored_knee, target, Vec3::Z)
            .expect("authored bend shares the canonical hemisphere");
        assert!(seeded.dot(Vec3::Z) > 0.99);
        assert!(authored_knee_pole_world(hip, authored_knee, target, Vec3::NEG_Z).is_none());
    }

    #[test]
    fn stay_attacks_plant_both_feet_while_unsupported_actions_preserve_authored_fk() {
        let mut attack = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 5.5)
            .with_gait_phase(0.0);
        attack.begin_attack(AttackSpec::default(), 0, 1);
        assert_eq!(locomotion_support_weights(&attack), (1.0, 1.0));

        let mut dodge = SkeletonState::default();
        dodge.begin_dodge(DodgeSpec::default(), 0, 1);
        assert_eq!(locomotion_support_weights(&dodge), (0.0, 0.0));
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
        for body in [
            BodyState::Airborne,
            BodyState::Grounded(GroundedPosture::Crouched),
            BodyState::Prone,
        ] {
            let skeleton = SkeletonState::default()
                .with_body_state(body)
                .with_weapon_guard(WeaponGuardState::Raised);
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
    fn slope_alignment_is_idempotent_across_repeated_evaluation() {
        let bind = Quat::from_xyzw(0.8856122, 0.00000032, 0.00000032, 0.46442544).normalize();
        let sole_up = sole_up_axis_from_bind(bind).normalize();
        let current = Quat::from_rotation_y(0.7) * bind;
        let steep_normal = Vec3::new(0.8, 0.3, -0.4).normalize();

        let once = slope_aligned_world_rotation(current, sole_up, steep_normal).unwrap();
        let twice = slope_aligned_world_rotation(once, sole_up, steep_normal).unwrap();

        assert!(once.angle_between(twice).to_degrees() < 0.0001);
        assert!((once * sole_up).angle_between(Vec3::Y).to_degrees() <= 28.0001);
    }

    #[test]
    fn terrain_ik_accepts_grounded_crouch_but_not_airborne() {
        let crouched = SkeletonState::default()
            .with_body_state(BodyState::Grounded(GroundedPosture::Crouched));
        assert!(terrain_ik_posture_is_valid(&crouched));

        let airborne = SkeletonState::default().with_body_state(BodyState::Airborne);
        assert!(!terrain_ik_posture_is_valid(&airborne));
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
        let authored_foot_rotation = Quat::from_euler(EulerRot::YXZ, 0.35, -0.45, 0.2).normalize();
        let end = world
            .spawn(Transform::from_xyz(0.0, -0.5, 0.0).with_rotation(authored_foot_rotation))
            .id();
        world.entity_mut(upper).add_child(upper_twist);
        world.entity_mut(upper_twist).add_child(lower);
        world.entity_mut(lower).add_child(lower_twist);
        world.entity_mut(lower_twist).add_child(end);
        let upper_twist_bind = *world.get::<Transform>(upper_twist).unwrap();
        let lower_twist_bind = *world.get::<Transform>(lower_twist).unwrap();
        let (_, _, authored_foot_world_rotation) = world
            .run_system_cached_with(test_joint_pose, (lower, end))
            .unwrap();
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
        let (knee, ankle, solved_foot_world_rotation) = world
            .run_system_cached_with(test_joint_pose, (lower, end))
            .unwrap();
        assert!(knee.abs_diff_eq(solution.knee, 0.0002));
        assert!(ankle.abs_diff_eq(solution.end, 0.0002));
        assert!(
            authored_foot_world_rotation
                .angle_between(solved_foot_world_rotation)
                .to_degrees()
                < 0.0001
        );
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
    fn pose_mirror_is_an_involution() {
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
    fn pelvis_smoothing_depends_on_elapsed_time_not_evaluation_count() {
        fn simulate(step_seconds: f32, evaluations: usize) -> f32 {
            (0..evaluations).fold(0.0, |current, _| {
                advance_pelvis_shift(current, -1.0, step_seconds)
            })
        }

        let at_64_hz = simulate(1.0 / 64.0, 16);
        let at_128_hz = simulate(1.0 / 128.0, 32);
        assert!((at_64_hz - at_128_hz).abs() < 0.0001);
        assert!((at_64_hz + 0.3).abs() < 0.0001);

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
