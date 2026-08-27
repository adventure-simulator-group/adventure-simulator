use std::collections::BTreeMap;

use adventuresim_tactical_core::prelude::*;
use bevy::{math::Affine3A, prelude::*};

use super::{AnimationPlayback, AuthoredBindTransform, PresentedSkeleton};

mod rig;
pub(crate) use rig::*;

pub(super) fn authored_locomotion_ik_owns(skeleton: &SkeletonState) -> bool {
    ik::authored_locomotion_owns(skeleton)
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct ProceduralLookState {
    base_rotation: Quat,
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct ProceduralJumpAnticipationState {
    base: Transform,
    applied: Transform,
    amount: f32,
    pelvis_amount: f32,
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
    mut bones: Query<(Entity, &mut Transform), With<MhrBone>>,
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

fn additive_look_rotation(current: Quat, offset: Quat) -> (Quat, ProceduralLookState) {
    let applied_rotation = (current * offset).normalize();
    (
        applied_rotation,
        ProceduralLookState {
            base_rotation: current,
        },
    )
}

/// Remove last frame's procedural look before authored and other procedural
/// layers evaluate. This makes the look pass an explicit base-plus-offset
/// operation instead of inferring whether an incoming rotation already
/// contains its own previous output.
pub(super) fn restore_procedural_look_base(
    mut bones: Query<(&mut Transform, &ProceduralLookState)>,
) {
    for (mut transform, state) in &mut bones {
        transform.rotation = state.base_rotation;
    }
}

/// Procedural facing is an additive post-FK layer. Sparse authored clips do not
/// necessarily rewrite every torso bone, so retain the pre-look local rotation
/// and reuse it when the same logical pose reaches this pass again.
pub(super) fn apply_head_and_torso_look(
    mut commands: Commands,
    owners: Query<(Entity, &CharacterLook, &PresentedSkeleton)>,
    mut transforms: ParamSet<(
        Query<&Transform>,
        Query<(
            Entity,
            &HumanoidBone,
            &mut Transform,
            Option<&mut ProceduralLookState>,
        )>,
    )>,
) {
    let owner_look = owners
        .iter()
        .filter_map(|(owner, look, skeleton)| {
            // This pass runs before transform propagation. GlobalTransform is
            // therefore one frame behind the current replicated root and
            // would reapply that root yaw across the neck chain.
            let owner_rotation = transforms.p0().get(owner).ok()?.rotation;
            Some((
                owner,
                (
                    guarded_camera_look(
                        look,
                        owner_rotation,
                        skeleton.weapon_guard(),
                        skeleton.is_posture_transitioning(),
                    ),
                    skeleton.action_direction().x.clamp(-1.0, 1.0) * 0.35,
                ),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    for (entity, bone, mut transform, state) in &mut transforms.p1() {
        let Some(&(aim, directional_yaw)) = owner_look.get(&bone.owner) else {
            continue;
        };
        let weight = match bone.role {
            BoneRole::StomachOne => 0.08,
            BoneRole::StomachTwo => 0.12,
            BoneRole::StomachThree => 0.16,
            BoneRole::Chest => 0.18,
            BoneRole::NeckOne => 0.2,
            BoneRole::Head => 0.26,
            _ => continue,
        };
        let aim_offset = match (aim, bone.role) {
            (Some(offset), BoneRole::NeckOne | BoneRole::Head) => offset,
            _ => Vec2::ZERO,
        };
        // Owner yaw is already on the character transform. Only the bounded
        // residual camera direction and local action offset are distributed
        // here. Camera pitch belongs exclusively to held aim/block; ordinary
        // camera look must not nod the presented head and neck.
        let offset = Quat::from_euler(
            EulerRot::YXZ,
            directional_yaw * weight + aim_offset.x,
            aim_offset.y,
            0.0,
        );
        let (rotation, next) = additive_look_rotation(transform.rotation, offset);
        transform.rotation = rotation;
        if let Some(mut state) = state {
            *state = next;
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

fn guarded_camera_look(
    look: &CharacterLook,
    owner_rotation: Quat,
    weapon_guard: WeaponGuardState,
    posture_transitioning: bool,
) -> Option<Vec2> {
    (weapon_guard == WeaponGuardState::Raised && !posture_transitioning)
        .then(|| constrained_camera_look(look, owner_rotation))
}

fn constrained_camera_look(look: &CharacterLook, owner_rotation: Quat) -> Vec2 {
    const JOINT_LIMIT: f32 = std::f32::consts::PI / 8.0;
    const JOINT_COUNT: f32 = 3.0;
    let camera_forward = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0) * Vec3::NEG_Z;
    let local = owner_rotation.inverse() * camera_forward;
    // Directly behind the body there is no anatomically preferable left/right
    // twist. Using signed Z puts atan2's branch cut at exactly that ambiguous
    // direction and can flip the neck chain between both joint limits. Fold
    // the rear hemisphere onto neutral instead: side look remains signed and
    // continuous, while a camera directly behind leaves the neck untwisted.
    let yaw = local.x.atan2(local.z.abs());
    let pitch = local.y.atan2(local.xz().length().max(f32::EPSILON));
    Vec2::new(
        (yaw / JOINT_COUNT).clamp(-JOINT_LIMIT, JOINT_LIMIT),
        (pitch / JOINT_COUNT).clamp(-JOINT_LIMIT, JOINT_LIMIT),
    )
}

fn jump_charge_pelvis_target(charging: bool, guard: WeaponGuardState) -> f32 {
    (charging && guard == WeaponGuardState::Lowered) as u8 as f32
}

const DIVE_PELVIS_LEAN_RADIANS: f32 = 40.0_f32.to_radians();

fn dive_pelvis_lean(direction: DiveDirection, amount: f32) -> Quat {
    let angle = DIVE_PELVIS_LEAN_RADIANS * amount.clamp(0.0, 1.0);
    match direction {
        DiveDirection::Forward => Quat::from_rotation_x(angle),
        DiveDirection::Backward => Quat::from_rotation_x(-angle),
        DiveDirection::Left => Quat::from_rotation_z(-angle),
        DiveDirection::Right => Quat::from_rotation_z(angle),
    }
}

fn procedural_dive_pelvis_rotation(
    authored: Quat,
    bind: Quat,
    direction: DiveDirection,
    amount: f32,
) -> Quat {
    let amount = amount.clamp(0.0, 1.0);
    let forward_facing = authored.slerp(bind, amount).normalize();
    (dive_pelvis_lean(direction, amount) * forward_facing).normalize()
}

/// Tilts the pelvis and therefore its complete descendant hierarchy toward
/// dive travel. Dive clips are upper-body-only; their root/pelvis/leg tracks
/// are masked out before this pass. The directional load supplies the lower
/// launch pose and blends to the authored ground contact only after impact.
pub(super) fn apply_procedural_dive_lower_body(
    owners: Query<&PresentedSkeleton>,
    mut bones: Query<(&HumanoidBone, &AuthoredBindTransform, &mut Transform)>,
) {
    for (bone, bind, mut transform) in &mut bones {
        if bone.role != BoneRole::Pelvis {
            continue;
        }
        let Ok(skeleton) = owners.get(bone.owner) else {
            continue;
        };
        let Some(transition) = skeleton.posture_transition() else {
            continue;
        };
        let PostureTransitionKind::DiveToDowned { direction, .. } = transition.kind() else {
            continue;
        };
        let phase = transition.phase().clamp(0.0, 1.0);
        let amount = if phase <= 0.5 {
            smoothstep(0.0, 0.5, phase)
        } else {
            1.0 - smoothstep(0.5, 1.0, phase)
        };
        transform.rotation = procedural_dive_pelvis_rotation(
            transform.rotation,
            bind.local.rotation,
            direction,
            amount,
        );
    }
}

/// Space begins an upright jump with a small procedural anticipation rather
/// than launching on the press edge. FK remains authored; this layer lowers
/// the pelvis and shares a modest forward fold across the spine.
pub(super) fn apply_jump_anticipation(
    mut commands: Commands,
    owners: Query<&PresentedSkeleton>,
    mut bones: Query<(
        Entity,
        &HumanoidBone,
        &mut Transform,
        Option<&mut ProceduralJumpAnticipationState>,
    )>,
) {
    for (entity, bone, mut transform, state) in &mut bones {
        let Ok(skeleton) = owners.get(bone.owner) else {
            continue;
        };
        let charging =
            skeleton.jump_anticipation() == JumpAnticipation::Charging && !skeleton.is_quickstep();
        let previous = state.as_deref().copied();
        let base = previous.map_or(*transform, |previous| {
            if previous.evaluation_tick == skeleton.locomotion_sample_tick
                || transform
                    .translation
                    .abs_diff_eq(previous.applied.translation, 0.000_01)
                    && transform.rotation.angle_between(previous.applied.rotation) <= 0.000_01
            {
                previous.base
            } else {
                *transform
            }
        });
        let previous_amount = previous.map_or(0.0, |previous| previous.amount);
        let previous_pelvis_amount = previous.map_or(0.0, |previous| previous.pelvis_amount);
        let amount = if skeleton.is_quickstep() {
            0.0
        } else if previous
            .is_some_and(|previous| previous.evaluation_tick == skeleton.locomotion_sample_tick)
        {
            previous_amount
        } else {
            let target = charging as u8 as f32;
            previous_amount + (target - previous_amount).clamp(-0.125, 0.125)
        };
        let pelvis_amount = if skeleton.is_quickstep() {
            0.0
        } else if previous
            .is_some_and(|previous| previous.evaluation_tick == skeleton.locomotion_sample_tick)
        {
            previous_pelvis_amount
        } else {
            let target = jump_charge_pelvis_target(charging, skeleton.weapon_guard());
            previous_pelvis_amount + (target - previous_pelvis_amount).clamp(-0.125, 0.125)
        };
        let mut applied = base;
        if amount > f32::EPSILON {
            match bone.role {
                BoneRole::Pelvis => applied.translation.y -= 0.12 * pelvis_amount,
                BoneRole::StomachOne => {
                    applied.rotation =
                        (applied.rotation * Quat::from_rotation_x(0.075 * amount)).normalize()
                }
                BoneRole::StomachTwo => {
                    applied.rotation =
                        (applied.rotation * Quat::from_rotation_x(0.06 * amount)).normalize()
                }
                BoneRole::StomachThree => {
                    applied.rotation =
                        (applied.rotation * Quat::from_rotation_x(0.05 * amount)).normalize()
                }
                BoneRole::Chest => {
                    applied.rotation =
                        (applied.rotation * Quat::from_rotation_x(0.04 * amount)).normalize()
                }
                _ => continue,
            }
        } else if !matches!(
            bone.role,
            BoneRole::Pelvis
                | BoneRole::StomachOne
                | BoneRole::StomachTwo
                | BoneRole::StomachThree
                | BoneRole::Chest
        ) {
            continue;
        }
        *transform = applied;
        let next = ProceduralJumpAnticipationState {
            base,
            applied,
            amount,
            pelvis_amount,
            evaluation_tick: skeleton.locomotion_sample_tick,
        };
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
        let mirror_entities = topology
            .mirror_centers()
            .iter()
            .copied()
            .chain(
                topology
                    .mirror_pairs()
                    .iter()
                    .flat_map(|&(left, right)| [left, right]),
            )
            .collect::<Vec<_>>();
        let local_snapshots = {
            let locals = transforms.p0();
            mirror_entities
                .iter()
                .filter_map(|&entity| Some((entity, *locals.get(entity).ok()?)))
                .collect::<Vec<_>>()
        };
        let (rig, rig_global) = {
            let helper = transforms.p1();
            let Ok(rig_global) = helper.compute_global_transform(rig_scene) else {
                continue;
            };
            let mut rig = BTreeMap::new();
            for (entity, local) in local_snapshots {
                let Ok(global) = helper.compute_global_transform(entity) else {
                    continue;
                };
                let parent = parents.get(entity).ok().map(ChildOf::parent);
                let parent_global = parent
                    .and_then(|parent| helper.compute_global_transform(parent).ok())
                    .unwrap_or(GlobalTransform::IDENTITY);
                rig.insert(
                    entity,
                    MirrorBone {
                        entity,
                        local,
                        global,
                        parent,
                        parent_global,
                    },
                );
            }
            (rig, rig_global)
        };
        let mut desired_globals = BTreeMap::<Entity, Affine3A>::new();
        let mut mirror_weights = BTreeMap::<Entity, f32>::new();
        for &entity in topology.mirror_centers() {
            let Some(bone) = rig.get(&entity) else {
                continue;
            };
            desired_globals.insert(bone.entity, mirrored_global_affine(bone.global, rig_global));
            mirror_weights.insert(bone.entity, whole_body_weight);
        }
        for &(left_entity, right_entity) in topology.mirror_pairs() {
            let (Some(left), Some(right)) = (rig.get(&left_entity), rig.get(&right_entity)) else {
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
        for bone in rig.values() {
            let Some(&desired_global) = desired_globals.get(&bone.entity) else {
                continue;
            };
            let Some(&weight) = mirror_weights.get(&bone.entity) else {
                continue;
            };
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
// The upright lowered-guard humanoid_unarmed root/pelvis rotations lift its
// pelvis by about 33 mm at passing. Subtract that measured authored rise only
// from the additive run-flight treatment; authored walk/pelvis translation
// remains untouched.
const AUTHORED_ORDINARY_PASSING_RISE_METRES: f32 = 0.033;

#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct LocomotionHeightState {
    initialized: bool,
    amplitude: f32,
    authored_rise_compensation: f32,
    displayed_wave: f32,
    wave_transition_offset: f32,
    pub(crate) landing_compression: f32,
    landing_compression_target: f32,
    landing_absorption_metres_per_second: f32,
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

/// Additive run-flight waveform with contacts at 0/.5 and flight peaks at
/// .25/.75. Authored joint translation remains the base pose. This is
/// presentation-only and is never applied to the authoritative owner/controller
/// transform.
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
    last_body_velocity: Vec3,
    last_tick: Option<u64>,
    last_posture: Option<Posture>,
    last_action: Option<SkeletonAction>,
    last_grounded: Option<bool>,
}

pub(crate) fn locomotion_height_wave(skeleton: &SkeletonState) -> f32 {
    if !skeleton.is_grounded()
        || skeleton.is_posture_transitioning()
        || skeleton.action_kind() != SkeletonAction::None
    {
        return 0.0;
    }
    if skeleton.weapon_guard() == WeaponGuardState::Raised && !skeleton.guarded_sprint_locomotion()
    {
        return 0.0;
    }
    let speed = skeleton.animation_speed();
    if speed <= 0.05 {
        return 0.0;
    }
    let profile = locomotion_profile(skeleton);
    let moving_weight = smoothstep(0.05, 0.75, speed);
    if profile.flight_apex_metres <= f32::EPSILON {
        return 0.0;
    }
    let half_step = (skeleton.gait_phase.rem_euclid(0.5) * 2.0).clamp(0.0, 1.0);
    let flight = (half_step * std::f32::consts::PI).sin().powi(2) * profile.flight_apex_metres;
    flight * moving_weight
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
    let run_weight =
        locomotion_profile(skeleton).flight_apex_metres / RUN_LOCOMOTION_PROFILE.flight_apex_metres;
    AUTHORED_ORDINARY_PASSING_RISE_METRES
        * run_weight.clamp(0.0, 1.0)
        * smoothstep(0.05, 0.75, skeleton.animation_speed())
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

fn landing_compression_for_action(
    profile: LocomotionProfile,
    impact_speed: f32,
    _previous_action: Option<SkeletonAction>,
) -> f32 {
    landing_compression_for_impact(profile, impact_speed)
}

/// Preserves authored joint translation and adds only the run-flight height
/// that is absent from the run clip. Action and airborne transitions blend the
/// additive offset back to zero without changing the authored base pose.
pub(super) fn apply_locomotion_height(
    mut commands: Commands,
    mut owners: Query<(
        Entity,
        &PresentedSkeleton,
        Option<&mut LocomotionHeightState>,
    )>,
    mut bones: Query<(&HumanoidBone, &mut Transform)>,
) {
    let mut heights = BTreeMap::new();
    for (owner, skeleton, state) in &mut owners {
        if skeleton.is_quickstep() {
            let mut cleared = state.as_deref().copied().unwrap_or_default();
            cleared.amplitude = 0.0;
            cleared.authored_rise_compensation = 0.0;
            cleared.displayed_wave = 0.0;
            cleared.wave_transition_offset = 0.0;
            cleared.landing_compression = 0.0;
            cleared.landing_compression_target = 0.0;
            clear_landing_foot_plants(&mut cleared);
            if let Some(mut state) = state {
                *state = cleared;
            } else {
                commands.entity(owner).insert(cleared);
            }
            continue;
        }
        let target_wave = locomotion_height_wave(skeleton);
        let target_authored_compensation = authored_height_compensation(skeleton);
        let mut next = state.as_deref().copied().unwrap_or_default();
        if !next.initialized {
            next.initialized = true;
            next.amplitude = target_wave;
            next.authored_rise_compensation = target_authored_compensation;
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
            && skeleton.posture() == Posture::Upright
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
        let landing_delta = skeleton
            .landing_sequence
            .checked_sub(next.last_landing_sequence)
            .filter(|delta| *delta <= 8);
        let landed = landing_delta.is_some_and(|delta| delta > 0);
        if skeleton.landing_sequence != next.last_landing_sequence {
            next.last_landing_sequence = skeleton.landing_sequence;
        }
        if landed {
            if skeleton.is_posture_transitioning() {
                // Authored dive/get-up recovery owns the complete skeleton.
                // Do not retain airborne feet or layer ordinary upright
                // landing compression over its terrain-timed contact blend.
                next.landing_compression = 0.0;
                next.landing_compression_target = 0.0;
                next.landing_absorption_metres_per_second = 0.0;
                next.landing_recovery_metres_per_second = 0.0;
            } else {
                next.landing_compression_target = landing_compression_for_action(
                    locomotion_profile(skeleton),
                    skeleton.landing_impact_speed,
                    next.last_action,
                );
                // Continue the downward impact velocity into the pelvis. The
                // legs reach peak flexion over physical time instead of
                // changing their bend plane by the full compression in one
                // presentation sample.
                next.landing_absorption_metres_per_second =
                    skeleton.landing_impact_speed.max(0.001);
                next.landing_recovery_metres_per_second = next.landing_compression_target
                    / locomotion_profile(skeleton).landing.recovery_seconds;
            }
            next.landing_left_foot_target = None;
            next.landing_right_foot_target = None;
            next.landing_plant_owner_position = None;
            next.landing_plant_tick = None;
            next.landing_plant_resync_tick = None;
        }
        if next.landing_compression < next.landing_compression_target {
            next.landing_compression = advance_towards(
                next.landing_compression,
                next.landing_compression_target,
                next.landing_absorption_metres_per_second * delta_seconds,
            );
            if (next.landing_compression - next.landing_compression_target).abs() <= 0.0001 {
                next.landing_compression_target = 0.0;
            }
        } else if !landed {
            next.landing_compression = advance_towards(
                next.landing_compression,
                0.0,
                next.landing_recovery_metres_per_second * delta_seconds,
            );
        }
        if !skeleton.is_grounded()
            || skeleton.is_posture_transitioning()
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

    for (bone, mut transform) in &mut bones {
        let Some(&height) = heights.get(&bone.owner) else {
            continue;
        };
        if height.displayed_wave.abs() <= f32::EPSILON {
            continue;
        }
        if bone.role == BoneRole::Root {
            transform.translation.y += height.displayed_wave;
        }
    }
}

/// Turns authored walk/run legs into the movement direction while preserving
/// the weapon layer's world-facing torso. The pelvis owns the yaw and the
/// first spine bone cancels it, so descendants on either side of the layer
/// boundary remain independent.
pub(super) fn orient_guarded_run_lower_body(
    owners: Query<&PresentedSkeleton>,
    rigs: Query<(Entity, &HumanoidRig)>,
    mut transforms: Query<&mut Transform>,
) {
    for (owner, rig) in &rigs {
        let Ok(skeleton) = owners.get(owner) else {
            continue;
        };
        if skeleton.is_quickstep()
            || !skeleton.guarded_sprint_locomotion()
            || !skeleton.raised_locomotion().is_moving()
        {
            continue;
        }
        let direction = skeleton.raised_locomotion().local_direction();
        let Some((pelvis_yaw, spine_counter_yaw)) = guarded_run_split_yaw(direction) else {
            continue;
        };
        let (Some(&pelvis), Some(&spine)) =
            (rig.get(&BoneRole::Pelvis), rig.get(&BoneRole::StomachOne))
        else {
            continue;
        };
        let Ok([mut pelvis, mut spine]) = transforms.get_many_mut([pelvis, spine]) else {
            continue;
        };
        pelvis.rotation = (pelvis.rotation * pelvis_yaw).normalize();
        spine.rotation = (spine_counter_yaw * spine.rotation).normalize();
    }
}

fn guarded_run_split_yaw(direction: Vec2) -> Option<(Quat, Quat)> {
    let direction = direction.try_normalize()?;
    // Local movement uses -Y for character-forward, corresponding to -Z in
    // the authored rig. Positive X is character-right.
    let angle = -direction.x.atan2(-direction.y);
    let pelvis = Quat::from_rotation_y(angle);
    Some((pelvis, pelvis.inverse()))
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
            || skeleton.is_posture_transitioning()
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
            let pole = constrain_rendered_leg_pole(
                rig,
                left,
                upper_snapshot.global.translation(),
                foot_snapshot.global.translation(),
                target,
                pole,
                &parents,
                &transforms.p0(),
            );
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

#[derive(Clone, Copy, Debug)]
struct BoneSnapshot {
    entity: Entity,
    global: GlobalTransform,
    parent_rotation: Quat,
}

mod ik;
pub(crate) use ik::{
    ArmIkState, HandIkTarget, HandSide, HeldWeaponConstraint, HumanoidIkTargets, LegIkDiagnostics,
    LegIkState, MEASURED_ANKLE_SOLE_OFFSET_METRES, ProceduralAnimationClock, RaisedFootworkState,
    SOLE_CONTACT_TOLERANCE_METRES, locomotion_support_weights,
};
#[cfg(test)]
use ik::{
    FOOT_TRACK_INNER, MAX_PELVIS_CORRECTION_STEP, MIN_INTER_FOOT_SEPARATION, TwoBoneSolution,
    advance_foot_target_at_speed, advance_pelvis_shift, authored_knee_pole_world,
    balance_recovery_direction, body_response_target, constrain_foot_to_track,
    constrain_target_to_reach, landing_maximum_reach, maximum_reach, plan_settle_landing,
    plant_is_continuous, projected_capture_point, raised_footwork_posture_is_valid,
    retained_plant_requires_release, secondary_grip_world, settle_swing_side, settle_swing_target,
    slope_aligned_world_rotation, sole_is_at_contact, solve_two_bone,
    terrain_conformed_guard_target, terrain_ik_posture_is_valid, terrain_leg_has_support,
};
pub(super) use ik::{
    apply_arm_and_weapon_constraints, apply_locomotion_body_response, apply_terrain_leg_ik,
    enforce_anatomical_knee_yaw, refresh_raised_support_after_propagation,
};
use ik::{
    apply_two_bone_solution, canonical_knee_pole, constrain_rendered_leg_pole,
    presentation_tick_delta, smoothstep, snapshot_chain, solve_landing_two_bone,
};

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn walk_preserves_authored_root_and_pelvis_translation() {
        let mut app = App::new();
        app.add_systems(Update, apply_locomotion_height);
        let state = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * WALK_LOCOMOTION_PROFILE.reference_speed)
            .with_gait_phase(0.25)
            .with_locomotion_sample_tick(1);
        let owner = app
            .world_mut()
            .spawn(PresentedSkeleton::new(state, None))
            .id();
        let authored_root = Transform::from_xyz(0.03, 0.81, -0.04);
        let authored_pelvis = Transform::from_xyz(-0.02, 0.92, 0.05);
        let root = app
            .world_mut()
            .spawn((
                HumanoidBone {
                    owner,
                    role: BoneRole::Root,
                },
                authored_root,
            ))
            .id();
        let pelvis = app
            .world_mut()
            .spawn((
                HumanoidBone {
                    owner,
                    role: BoneRole::Pelvis,
                },
                authored_pelvis,
            ))
            .id();

        app.update();

        assert_eq!(*app.world().get::<Transform>(root).unwrap(), authored_root);
        assert_eq!(
            *app.world().get::<Transform>(pelvis).unwrap(),
            authored_pelvis
        );
    }

    #[test]
    fn run_flight_is_additive_without_replacing_authored_pelvis_translation() {
        let mut app = App::new();
        app.add_systems(Update, apply_locomotion_height);
        let state = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * RUN_LOCOMOTION_PROFILE.reference_speed)
            .with_gait_phase(0.25)
            .with_locomotion_sample_tick(1);
        let owner = app
            .world_mut()
            .spawn(PresentedSkeleton::new(state, None))
            .id();
        let authored_root = Transform::from_xyz(0.03, 0.81, -0.04);
        let authored_pelvis = Transform::from_xyz(-0.02, 0.92, 0.05);
        let root = app
            .world_mut()
            .spawn((
                HumanoidBone {
                    owner,
                    role: BoneRole::Root,
                },
                authored_root,
            ))
            .id();
        let pelvis = app
            .world_mut()
            .spawn((
                HumanoidBone {
                    owner,
                    role: BoneRole::Pelvis,
                },
                authored_pelvis,
            ))
            .id();

        app.update();

        let expected_flight =
            RUN_LOCOMOTION_PROFILE.flight_apex_metres - AUTHORED_ORDINARY_PASSING_RISE_METRES;
        let displayed_root = app.world().get::<Transform>(root).unwrap();
        assert!(
            (displayed_root.translation.y - (authored_root.translation.y + expected_flight)).abs()
                <= 0.0001
        );
        assert_eq!(
            displayed_root.translation.xz(),
            authored_root.translation.xz()
        );
        assert_eq!(
            *app.world().get::<Transform>(pelvis).unwrap(),
            authored_pelvis
        );
    }

    #[test]
    fn measured_sole_offset_matches_the_authored_rig() {
        assert!((MEASURED_ANKLE_SOLE_OFFSET_METRES - 0.085).abs() < f32::EPSILON);
    }

    #[test]
    fn presentation_tick_delta_handles_wrap_and_rejects_large_gaps() {
        assert_eq!(presentation_tick_delta(Some(u64::MAX), 0), Some(1));
        assert_eq!(presentation_tick_delta(Some(1), 100), None);
    }

    #[test]
    fn guarded_run_pelvis_yaw_is_cancelled_at_the_upper_body_boundary() {
        for direction in [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X] {
            let (pelvis, spine) = guarded_run_split_yaw(direction).unwrap();
            assert!((pelvis * spine).abs_diff_eq(Quat::IDENTITY, 0.0001));
            let expected = Vec3::new(direction.x, 0.0, direction.y);
            assert!((pelvis * Vec3::NEG_Z).abs_diff_eq(expected, 0.0001));
        }
    }

    #[test]
    fn raised_guard_never_adds_jump_charge_pelvis_lowering() {
        assert_eq!(
            jump_charge_pelvis_target(true, WeaponGuardState::Raised),
            0.0
        );
        assert_eq!(
            jump_charge_pelvis_target(true, WeaponGuardState::Lowered),
            1.0
        );
    }

    #[test]
    fn dive_pelvis_lean_tips_up_axis_exactly_toward_travel() {
        let forward = dive_pelvis_lean(DiveDirection::Forward, 1.0) * Vec3::Y;
        let backward = dive_pelvis_lean(DiveDirection::Backward, 1.0) * Vec3::Y;
        let left = dive_pelvis_lean(DiveDirection::Left, 1.0) * Vec3::Y;
        let right = dive_pelvis_lean(DiveDirection::Right, 1.0) * Vec3::Y;
        assert!(forward.z > 0.64 && forward.y > 0.76);
        assert!(backward.z < -0.64 && backward.y > 0.76);
        assert!(left.x > 0.64 && left.y > 0.76);
        assert!(right.x < -0.64 && right.y > 0.76);
    }

    #[test]
    fn airborne_dive_pelvis_faces_forward_independently_of_guard_rotation() {
        let bind = Quat::from_rotation_y(0.17);
        let guard = Quat::from_euler(EulerRot::YXZ, -0.8, 0.1, -0.2);
        for direction in [
            DiveDirection::Forward,
            DiveDirection::Backward,
            DiveDirection::Left,
            DiveDirection::Right,
        ] {
            let actual = procedural_dive_pelvis_rotation(guard, bind, direction, 1.0);
            let expected = (dive_pelvis_lean(direction, 1.0) * bind).normalize();
            assert!(actual.angle_between(expected) < 0.0001);
        }
    }

    #[test]
    fn dive_pelvis_override_releases_exactly_to_authored_contact() {
        let contact = Quat::from_euler(EulerRot::YXZ, 0.3, -1.2, 0.4);
        let actual =
            procedural_dive_pelvis_rotation(contact, Quat::IDENTITY, DiveDirection::Backward, 0.0);
        assert!(actual.angle_between(contact) < 0.0001);
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
    fn canonical_mhr_role_names_are_recognized() {
        for name in [
            "body_world",
            "root",
            "c_spine0",
            "c_spine1",
            "c_spine2",
            "c_spine3",
            "c_neck",
            "c_head",
            "c_camera",
            "l_upleg",
            "r_lowleg",
            "l_lowarm",
            "r_weapon",
            "l_ball",
        ] {
            assert!(BoneRole::from_name(name).is_some(), "missing {name}");
        }
        assert_eq!(BoneRole::from_name("l_upleg_twist2_proc"), None);
        assert_eq!(BoneRole::from_name("Cylinder"), None);
    }

    #[test]
    fn raised_camera_look_is_shared_across_three_heavily_limited_joints() {
        let look = CharacterLook {
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 1.2,
        };
        let per_joint = constrained_camera_look(&look, Quat::IDENTITY);
        assert!(per_joint.x.abs() <= std::f32::consts::PI / 8.0 + 0.000_01);
        assert!(per_joint.y.abs() <= std::f32::consts::PI / 8.0 + 0.000_01);
        assert!(per_joint.x.abs() > 0.3);
        assert!(per_joint.y.abs() > 0.3);
    }

    #[test]
    fn camera_pitch_reaches_head_and_neck_only_while_guard_is_raised() {
        let look = CharacterLook {
            yaw: 0.0,
            pitch: 0.6,
        };
        assert_eq!(
            guarded_camera_look(&look, Quat::IDENTITY, WeaponGuardState::Lowered, false),
            None
        );
        assert!(
            guarded_camera_look(&look, Quat::IDENTITY, WeaponGuardState::Raised, false)
                .unwrap()
                .y
                .abs()
                > 0.1
        );
        assert_eq!(
            guarded_camera_look(&look, Quat::IDENTITY, WeaponGuardState::Raised, true),
            None,
            "authored posture transitions exclusively own the spine and head"
        );
    }

    #[test]
    fn current_root_facing_removes_camera_yaw_from_the_neck_chain() {
        let look = CharacterLook {
            yaw: 0.7,
            pitch: 0.0,
        };
        let camera_forward = Quat::from_rotation_y(look.yaw) * Vec3::NEG_Z;
        let current_root = Quat::from_rotation_y(camera_forward.x.atan2(camera_forward.z));

        assert!(constrained_camera_look(&look, Quat::IDENTITY).x.abs() > 0.1);
        assert!(constrained_camera_look(&look, current_root).x.abs() <= 0.000_01);
    }

    #[test]
    fn rear_camera_crossing_cannot_flip_between_neck_joint_limits() {
        let left_of_rear = CharacterLook {
            yaw: -0.001,
            pitch: 0.0,
        };
        let right_of_rear = CharacterLook {
            yaw: 0.001,
            pitch: 0.0,
        };
        let left = constrained_camera_look(&left_of_rear, Quat::IDENTITY).x;
        let right = constrained_camera_look(&right_of_rear, Quat::IDENTITY).x;
        assert!(left.abs() < 0.001);
        assert!(right.abs() < 0.001);
        assert!((left - right).abs() < 0.001);
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
    fn additive_locomotion_height_covers_run_only() {
        let moving = |speed, posture, guard| {
            SkeletonState::default()
                .with_local_velocity(Vec3::NEG_Z * speed)
                .with_body_state(match posture {
                    Posture::Airborne => BodyState::Airborne,
                    _ => BodyState::Grounded(GroundedPosture::Upright),
                })
                .with_weapon_guard(guard)
                .with_raised_locomotion(if guard == WeaponGuardState::Raised {
                    RaisedLocomotionIntent::moving(Vec2::NEG_Y, speed)
                } else {
                    RaisedLocomotionIntent::default()
                })
        };
        assert_eq!(
            locomotion_height_wave(
                &moving(2.0, Posture::Upright, WeaponGuardState::Lowered,).with_gait_phase(0.25)
            ),
            0.0
        );
        assert!(
            (locomotion_height_wave(
                &moving(5.5, Posture::Upright, WeaponGuardState::Lowered,).with_gait_phase(0.25)
            ) - RUN_LOCOMOTION_PROFILE.flight_apex_metres)
                .abs()
                < 0.0001
        );
        let raised = moving(2.0, Posture::Upright, WeaponGuardState::Raised).with_gait_phase(0.25);
        assert_eq!(locomotion_height_wave(&raised), 0.0);
        assert_eq!(
            locomotion_height_wave(&raised.with_guarded_sprint_locomotion(true)),
            0.0
        );
        assert_eq!(
            authored_height_compensation(
                &moving(2.0, Posture::Upright, WeaponGuardState::Lowered,)
            ),
            0.0
        );
        assert!(
            (authored_height_compensation(&moving(
                5.5,
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
        let mut specialized = moving(2.0, Posture::Upright, WeaponGuardState::Lowered);
        specialized.animation_pack = "humanoid_sword_and_shield".to_owned();
        assert_eq!(authored_height_compensation(&specialized), 0.0);
    }

    #[test]
    fn body_response_and_landing_compression_are_bounded() {
        let steady_run = body_response_target(Vec3::Z * 5.5, Vec3::ZERO, 1.0);
        let forward = body_response_target(Vec3::Z * 5.5, Vec3::Z * 12.0, 1.0);
        let braking = body_response_target(Vec3::Z * 5.5, Vec3::NEG_Z * 12.0, 1.0);
        let walking_braking = body_response_target(
            Vec3::Z * WALK_LOCOMOTION_PROFILE.reference_speed,
            Vec3::NEG_Z * 12.0,
            WALK_LOCOMOTION_PROFILE.reference_speed / RUN_LOCOMOTION_PROFILE.reference_speed,
        );
        let stopped_braking = body_response_target(Vec3::ZERO, Vec3::NEG_Z * 12.0, 0.0);
        let lateral = body_response_target(Vec3::ZERO, Vec3::X * 12.0, 1.0);
        assert!((11.9..=12.1).contains(&steady_run.x.to_degrees()));
        assert!((28.0..=30.0).contains(&forward.x.to_degrees()));
        assert!((-3.0..=-1.0).contains(&braking.x.to_degrees()));
        assert!((-1.0..=0.0).contains(&walking_braking.x.to_degrees()));
        assert!(stopped_braking.x.abs() <= f32::EPSILON);
        assert!((11.0..=12.0).contains(&lateral.y.abs().to_degrees()));
        assert!(
            body_response_target(Vec3::ZERO, Vec3::new(40.0, 0.0, 40.0), 1.0)
                .length()
                .to_degrees()
                <= 30.0
        );
        assert_eq!(
            landing_compression_for_impact(WALK_LOCOMOTION_PROFILE, 0.5),
            0.0
        );
        assert!((0.04..=0.08).contains(&landing_compression_for_impact(
            WALK_LOCOMOTION_PROFILE,
            4.5,
        )));
        assert!(
            landing_compression_for_action(
                WALK_LOCOMOTION_PROFILE,
                4.5,
                Some(SkeletonAction::Dodge),
            ) > 0.0
        );
        assert!(landing_compression_for_action(WALK_LOCOMOTION_PROFILE, 4.5, None) > 0.0);
        assert_eq!(presentation_tick_delta(Some(10), 10), Some(0));
        assert_eq!(presentation_tick_delta(Some(10), 14), Some(4));
        assert_eq!(presentation_tick_delta(Some(14), 2), None);
        assert_eq!(presentation_tick_delta(Some(2), 40), None);
    }

    #[test]
    fn look_evaluation_is_a_strict_base_plus_offset() {
        let base = Quat::from_rotation_z(0.17);
        let offset = Quat::from_rotation_x(0.2 * 0.16);
        let (applied, state) = additive_look_rotation(base, offset);
        assert!((base * offset).angle_between(applied) <= 0.000_001);
        assert!(state.base_rotation.angle_between(base) <= 0.000_001);
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
        for contact in [LeadFoot::Left, LeadFoot::Right] {
            let mut skeleton = SkeletonState::default()
                .with_local_velocity(Vec3::NEG_Z * 2.0)
                .with_weapon_guard(WeaponGuardState::Raised)
                .with_raised_locomotion(RaisedLocomotionIntent::moving(Vec2::NEG_Y, 2.0));
            skeleton.contact_foot = contact;
            let support = locomotion_support_weights(&skeleton);
            assert_eq!(support.0 + support.1, 1.0);
            assert_eq!(
                support,
                match contact {
                    LeadFoot::Left => (1.0, 0.0),
                    LeadFoot::Right => (0.0, 1.0),
                }
            );
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
    fn attacks_keep_live_locomotion_support_while_unsupported_actions_preserve_authored_fk() {
        let mut attack = SkeletonState::default()
            .with_local_velocity(Vec3::NEG_Z * 5.5)
            .with_gait_phase(0.0);
        let locomotion = locomotion_support_weights(&attack);
        attack.begin_attack(AttackSpec::default(), 0, 1).unwrap();
        assert_eq!(locomotion_support_weights(&attack), locomotion);

        let mut dodge = SkeletonState::default();
        dodge.begin_dodge(DodgeSpec::default(), 0, 1).unwrap();
        assert_eq!(locomotion_support_weights(&dodge), (0.0, 0.0));
        assert!(!terrain_leg_has_support(0.0));
        assert!(!terrain_leg_has_support(0.01));
        assert!(terrain_leg_has_support(0.1));
    }

    #[test]
    fn guard_stride_scales_with_leg_length_from_the_five_eleven_reference() {
        let reference_leg_length = 1.8034 * 0.860 / 1.821;
        assert!(guard_maximum_foot_separation(0.75) < guard_maximum_foot_separation(0.90));
        assert!((guard_maximum_foot_separation(reference_leg_length) - 0.9144).abs() < 0.0001);
        assert!((guard_rear_contact_separation(reference_leg_length) - 0.0762).abs() < 0.0001);
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
        for body in [BodyState::Airborne, BodyState::Prone] {
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
    fn terrain_ik_accepts_grounded_upright_but_not_airborne() {
        let upright = SkeletonState::default();
        assert!(terrain_ik_posture_is_valid(&upright));

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
    fn standing_knee_yaw_constraint_preserves_downed_and_transition_poses() {
        assert!(ik::anatomical_knee_yaw_posture_is_valid(
            &SkeletonState::default()
        ));
        assert!(!ik::anatomical_knee_yaw_posture_is_valid(
            &SkeletonState::default().with_body_state(BodyState::Prone)
        ));
        let mut transition = SkeletonState::default();
        assert!(transition.begin_posture_transition(PostureTransitionKind::UprightToProne, 0, 10,));
        assert!(!ik::anatomical_knee_yaw_posture_is_valid(&transition));
        let mut quickstep = SkeletonState::default();
        quickstep
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 100)
            .unwrap();
        quickstep.advance_action(150);
        assert!(!ik::anatomical_knee_yaw_posture_is_valid(&quickstep));
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
