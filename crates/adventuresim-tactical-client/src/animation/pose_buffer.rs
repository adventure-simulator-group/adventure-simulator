//! Fixed-rate local-pose sampling and interruption-safe inertialization.
//!
//! This is deliberately downstream of semantic animation-pack resolution:
//! the semantic router produces an `AnimationPlayback` plan from the shared
//! `PresentedSkeleton`. This backend bakes clip curves onto a 30 Hz grid,
//! samples into per-character buffers, and writes the resulting local
//! transforms before the shared procedural passes.

use std::{collections::HashMap, sync::Arc};

use bevy::{
    animation::{AnimationTargetId, animated_field},
    asset::AssetId,
    camera::primitives::{Frustum, Sphere},
};

use super::*;

const SAMPLE_HZ: f32 = 30.0;
const SAMPLE_DT: f32 = 1.0 / SAMPLE_HZ;
const INERTIAL_HALFLIFE_SECONDS: f32 = 0.10;
const CULL_DISTANCE_METRES: f32 = 100.0;
const CULL_RADIUS_METRES: f32 = 2.0;

#[derive(Resource, Default, Debug, Clone, Copy, serde::Serialize)]
pub(crate) struct PoseBufferMetrics {
    pub(crate) baked_clip_bytes: usize,
    pub(crate) baked_clip_count: usize,
    pub(crate) sampled_pose_count: u64,
    pub(crate) culled_character_count: usize,
}

#[derive(Resource, Default)]
pub(super) struct RigDefinitions(HashMap<String, Arc<RigDefinition>>);

#[derive(Resource, Default)]
pub(super) struct BakedClipBank(HashMap<(String, AssetId<AnimationClip>), Arc<BakedClip>>);

struct RigDefinition {
    family: String,
    joints: Vec<RigJoint>,
}

#[derive(Clone)]
struct RigJoint {
    target: AnimationTargetId,
    bind: LocalPose,
    lower_body: bool,
    strips_root_translation: bool,
}

#[derive(Clone)]
struct BakedClip {
    duration: f32,
    frame_dt: f32,
    frames: usize,
    tracks: Vec<BoneTrack>,
}

#[derive(Clone)]
struct BoneTrack {
    translations: Vec<Vec3>,
    rotations: Vec<Quat>,
    scales: Vec<Vec3>,
}

impl BakedClip {
    fn sample(&self, joint: usize, time_seconds: f32) -> LocalPose {
        let frame =
            (time_seconds.clamp(0.0, self.duration) / self.frame_dt).min((self.frames - 1) as f32);
        let first = frame as usize;
        let second = (first + 1).min(self.frames - 1);
        let alpha = frame.fract();
        let track = &self.tracks[joint];
        LocalPose {
            translation: track.translations[first].lerp(track.translations[second], alpha),
            rotation: hemisphere_slerp(track.rotations[first], track.rotations[second], alpha),
            scale: track.scales[first].lerp(track.scales[second], alpha),
        }
    }

    fn memory_bytes(&self) -> usize {
        self.tracks
            .iter()
            .map(|track| {
                track.translations.len() * size_of::<Vec3>()
                    + track.rotations.len() * size_of::<Quat>()
                    + track.scales.len() * size_of::<Vec3>()
            })
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LocalPose {
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
}

impl LocalPose {
    fn from_transform(transform: Transform) -> Self {
        Self {
            translation: transform.translation,
            rotation: transform.rotation,
            scale: transform.scale,
        }
    }

    fn interpolate(self, next: Self, alpha: f32) -> Self {
        let alpha = alpha.clamp(0.0, 1.0);
        Self {
            translation: self.translation.lerp(next.translation, alpha),
            rotation: hemisphere_slerp(self.rotation, next.rotation, alpha),
            scale: self.scale.lerp(next.scale, alpha),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PosePlanKey {
    bind_pose: bool,
    clips: Vec<(AssetId<AnimationClip>, ClipLayer)>,
}

impl PosePlanKey {
    fn from_playback(playback: &AnimationPlayback) -> Self {
        let mut clips = playback
            .clips
            .iter()
            .map(|weighted| (weighted.clip.handle.id(), weighted.clip.layer))
            .collect::<Vec<_>>();
        clips.sort_by_key(|(id, layer)| (format!("{id:?}"), *layer as u8));
        clips.dedup();
        Self {
            bind_pose: playback.use_authored_bind_pose,
            clips,
        }
    }
}

#[derive(Component)]
pub(super) struct PoseBufferRig {
    definition: Arc<RigDefinition>,
    entities: Vec<Option<Entity>>,
    previous: Vec<LocalPose>,
    next: Vec<LocalPose>,
    sample_accumulator: f32,
    interpolation_alpha: f32,
    last_evaluation_tick: Option<u64>,
    decay_delta_seconds: f32,
    offsets: Vec<JointInertialOffset>,
    plan: Option<PosePlanKey>,
    active: bool,
    frozen: bool,
}

impl PoseBufferRig {
    fn displayed_pose(&self, joint: usize) -> LocalPose {
        let pose = self.previous[joint].interpolate(self.next[joint], self.interpolation_alpha);
        self.offsets[joint].peek(pose)
    }

    fn displayed_velocity(&self, joint: usize) -> (Vec3, Vec3) {
        (
            (self.next[joint].translation - self.previous[joint].translation) / SAMPLE_DT,
            quaternion_angular_velocity(
                self.next[joint].rotation,
                self.previous[joint].rotation,
                SAMPLE_DT,
            ),
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct JointInertialOffset {
    translation: Vec3,
    translation_velocity: Vec3,
    rotation: Quat,
    angular_velocity: Vec3,
}

impl Default for JointInertialOffset {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            translation_velocity: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            angular_velocity: Vec3::ZERO,
        }
    }
}

impl JointInertialOffset {
    fn capture(
        &mut self,
        displayed: LocalPose,
        displayed_linear_velocity: Vec3,
        displayed_angular_velocity: Vec3,
        target: LocalPose,
        target_linear_velocity: Vec3,
        target_angular_velocity: Vec3,
    ) {
        self.translation = displayed.translation - target.translation;
        self.translation_velocity = displayed_linear_velocity - target_linear_velocity;
        self.rotation = shortest_rotation(displayed.rotation * target.rotation.inverse());
        self.angular_velocity = displayed_angular_velocity - target_angular_velocity;
    }

    fn update(&mut self, input: LocalPose, delta_seconds: f32) -> LocalPose {
        decay_spring_vec3(
            &mut self.translation,
            &mut self.translation_velocity,
            INERTIAL_HALFLIFE_SECONDS,
            delta_seconds,
        );
        decay_spring_quaternion(
            &mut self.rotation,
            &mut self.angular_velocity,
            INERTIAL_HALFLIFE_SECONDS,
            delta_seconds,
        );
        self.peek(input)
    }

    fn peek(&self, input: LocalPose) -> LocalPose {
        LocalPose {
            translation: input.translation + self.translation,
            rotation: (self.rotation * input.rotation).normalize(),
            scale: input.scale,
        }
    }
}

pub(super) fn update_pose_buffers(
    mut commands: Commands,
    time: Res<Time>,
    procedural_clock: Res<ProceduralAnimationClock>,
    catalog: Res<AnimationPackCatalog>,
    clips: Res<Assets<AnimationClip>>,
    cameras: Query<
        (&GlobalTransform, &Frustum),
        (
            With<Camera3d>,
            Without<crate::presentation::TacticalCloudOffscreenCamera>,
        ),
    >,
    owners: Query<(
        Entity,
        &PresentedSkeleton,
        &AnimationPlayback,
        &GlobalTransform,
        Option<&mut PoseBufferRig>,
    )>,
    targets: Query<(
        Entity,
        &AnimationTargetId,
        &AuthoredBindTransform,
        Option<&Name>,
    )>,
    transforms: Query<&Transform>,
    mut definitions: ResMut<RigDefinitions>,
    mut bank: ResMut<BakedClipBank>,
    mut metrics: ResMut<PoseBufferMetrics>,
) {
    let _spike = crate::animation::diagnostics::SpikeGuard::new("update_pose_buffers");
    let camera = cameras.iter().next();
    metrics.culled_character_count = 0;

    for (owner, skeleton, playback, owner_transform, rig) in owners {
        let family = catalog
            .packs
            .get(&skeleton.animation_pack)
            .map(|pack| pack.skeleton_family.clone())
            .unwrap_or_else(|| "humanoid".to_owned());
        let mut rig = if let Some(rig) = rig {
            rig
        } else {
            let found = targets
                .iter()
                .filter(|(_, _, bind, _)| bind.owner == owner)
                .map(|(entity, target, bind, name)| {
                    (
                        entity,
                        *target,
                        LocalPose::from_transform(bind.local),
                        name.map(|name| name.as_str().to_owned()),
                    )
                })
                .collect::<Vec<_>>();
            if found.is_empty() {
                continue;
            }
            let definition = definitions
                .0
                .entry(family.clone())
                .or_insert_with(|| {
                    let mut ordered = found
                        .iter()
                        .map(|(_, target, bind, name)| (target, bind, name))
                        .collect::<Vec<_>>();
                    ordered.sort_by(|left, right| left.2.cmp(right.2));
                    Arc::new(RigDefinition {
                        family: family.clone(),
                        joints: ordered
                            .into_iter()
                            .map(|(target, bind, name)| RigJoint {
                                target: *target,
                                bind: *bind,
                                lower_body: name
                                    .as_deref()
                                    .is_some_and(is_lower_body_animation_target),
                                strips_root_translation: name
                                    .as_deref()
                                    .is_some_and(|name| name.eq_ignore_ascii_case("root")),
                            })
                            .collect(),
                    })
                })
                .clone();
            let by_target = found
                .into_iter()
                .map(|(entity, target, _, _)| (target, entity))
                .collect::<HashMap<_, _>>();
            let entities = definition
                .joints
                .iter()
                .map(|joint| by_target.get(&joint.target).copied())
                .collect::<Vec<_>>();
            let current = definition
                .joints
                .iter()
                .zip(&entities)
                .map(|(joint, entity)| {
                    entity
                        .and_then(|entity| transforms.get(entity).ok().copied())
                        .map(LocalPose::from_transform)
                        .unwrap_or(joint.bind)
                })
                .collect::<Vec<_>>();
            let joint_count = definition.joints.len();
            let pose_rig = PoseBufferRig {
                definition,
                entities,
                previous: current.clone(),
                next: current,
                sample_accumulator: 0.0,
                interpolation_alpha: 0.0,
                last_evaluation_tick: None,
                decay_delta_seconds: 0.0,
                offsets: vec![JointInertialOffset::default(); joint_count],
                plan: None,
                active: false,
                frozen: false,
            };
            commands.entity(owner).insert(pose_rig);
            continue;
        };

        let delta_seconds = match procedural_clock.fixed_step() {
            Some((tick, _)) if rig.last_evaluation_tick == Some(tick) => {
                rig.decay_delta_seconds = 0.0;
                continue;
            }
            Some((tick, delta_seconds)) => {
                rig.last_evaluation_tick = Some(tick);
                delta_seconds.clamp(0.0, 0.1)
            }
            None => {
                rig.last_evaluation_tick = None;
                time.delta_secs().clamp(0.0, 0.1)
            }
        };
        rig.decay_delta_seconds = delta_seconds;

        let position = owner_transform.translation();
        let frozen = camera.is_some_and(|(camera_transform, frustum)| {
            position.distance_squared(camera_transform.translation())
                > CULL_DISTANCE_METRES * CULL_DISTANCE_METRES
                || !frustum.intersects_sphere(
                    &Sphere {
                        center: position.into(),
                        radius: CULL_RADIUS_METRES,
                    },
                    false,
                )
        });
        if frozen {
            rig.frozen = true;
            metrics.culled_character_count += 1;
            continue;
        }
        if rig.frozen {
            rig.sample_accumulator = 0.0;
            rig.interpolation_alpha = 0.0;
        }
        rig.frozen = false;

        let key = PosePlanKey::from_playback(playback);
        let transition = !rig.active || rig.plan.as_ref() != Some(&key);
        rig.sample_accumulator += delta_seconds;
        let Some(target) = sample_plan(playback, &rig.definition, &clips, &mut bank, &mut metrics)
        else {
            continue;
        };
        if transition {
            let capture_displayed = rig.active;
            for (joint, target_pose) in target.iter().copied().enumerate() {
                let displayed = if capture_displayed {
                    rig.displayed_pose(joint)
                } else {
                    target_pose
                };
                let (linear_velocity, angular_velocity) = if capture_displayed {
                    rig.displayed_velocity(joint)
                } else {
                    (Vec3::ZERO, Vec3::ZERO)
                };
                rig.offsets[joint].capture(
                    displayed,
                    linear_velocity,
                    angular_velocity,
                    target_pose,
                    Vec3::ZERO,
                    Vec3::ZERO,
                );
            }
            rig.previous.clone_from(&target);
            rig.next = target;
            rig.sample_accumulator = 0.0;
            rig.interpolation_alpha = 1.0;
            rig.plan = Some(key);
            rig.active = true;
        } else {
            rig.previous = rig.next.clone();
            rig.next = target;
            let due = samples_due(rig.sample_accumulator);
            rig.sample_accumulator = (rig.sample_accumulator - SAMPLE_DT * due as f32).max(0.0);
            // The semantic plan is evaluated from the authoritative tactical
            // phase every fixed tick. Applying its current pose avoids adding
            // a second 30 Hz phase delay; BakedClip::sample performs the
            // render interpolation between the baked 30 Hz keyframes.
            rig.interpolation_alpha = 1.0;
        }
    }
}

pub(super) fn apply_pose_buffers(
    mut rigs: Query<&mut PoseBufferRig>,
    mut transforms: Query<&mut Transform, Without<PoseBufferRig>>,
) {
    let _spike = crate::animation::diagnostics::SpikeGuard::new("apply_pose_buffers");
    for mut rig in &mut rigs {
        if !rig.active || rig.frozen {
            continue;
        }
        let alpha = rig.interpolation_alpha;
        let delta_seconds = rig.decay_delta_seconds;
        for joint in 0..rig.entities.len() {
            let Some(entity) = rig.entities[joint] else {
                continue;
            };
            let input = rig.previous[joint].interpolate(rig.next[joint], alpha);
            let pose = if delta_seconds > 0.0 {
                rig.offsets[joint].update(input, delta_seconds)
            } else {
                rig.offsets[joint].peek(input)
            };
            if let Ok(mut transform) = transforms.get_mut(entity) {
                transform.translation = pose.translation;
                transform.rotation = pose.rotation;
                transform.scale = pose.scale;
            }
        }
    }
}

fn sample_plan(
    playback: &AnimationPlayback,
    definition: &RigDefinition,
    clips: &Assets<AnimationClip>,
    bank: &mut BakedClipBank,
    metrics: &mut PoseBufferMetrics,
) -> Option<Vec<LocalPose>> {
    if playback.use_authored_bind_pose || playback.clips.is_empty() {
        return Some(definition.joints.iter().map(|joint| joint.bind).collect());
    }
    let mut baked = Vec::with_capacity(playback.clips.len());
    for weighted in &playback.clips {
        let clip = get_or_bake(
            weighted.clip.handle.id(),
            &weighted.clip.handle,
            definition,
            clips,
            bank,
            metrics,
        )?;
        baked.push((weighted, clip));
    }
    let mut pose = Vec::with_capacity(definition.joints.len());
    for (joint_index, joint) in definition.joints.iter().enumerate() {
        let mut blended = joint.bind;
        let mut accumulated = 0.0_f32;
        for (weighted, clip) in &baked {
            let included = match weighted.clip.layer {
                ClipLayer::Whole => true,
                ClipLayer::Upper => !joint.lower_body,
                ClipLayer::Lower => joint.lower_body,
            };
            if !included || weighted.weight <= f32::EPSILON || !weighted.weight.is_finite() {
                continue;
            }
            let sample = sanitize_pose(clip.sample(joint_index, weighted.time_seconds), joint.bind);
            let next_total = accumulated + weighted.weight;
            let alpha = if next_total > f32::EPSILON {
                weighted.weight / next_total
            } else {
                0.0
            };
            blended = if accumulated <= f32::EPSILON {
                sample
            } else {
                blended.interpolate(sample, alpha)
            };
            accumulated = next_total;
        }
        pose.push(if accumulated > f32::EPSILON {
            blended
        } else {
            joint.bind
        });
    }
    metrics.sampled_pose_count = metrics.sampled_pose_count.saturating_add(1);
    Some(pose)
}

fn get_or_bake(
    id: AssetId<AnimationClip>,
    handle: &Handle<AnimationClip>,
    definition: &RigDefinition,
    clips: &Assets<AnimationClip>,
    bank: &mut BakedClipBank,
    metrics: &mut PoseBufferMetrics,
) -> Option<Arc<BakedClip>> {
    let key = (definition.family.clone(), id);
    if let Some(clip) = bank.0.get(&key) {
        return Some(clip.clone());
    }
    let clip = clips.get(handle)?;
    let baked = Arc::new(bake_clip(clip, definition));
    metrics.baked_clip_bytes = metrics
        .baked_clip_bytes
        .saturating_add(baked.memory_bytes());
    metrics.baked_clip_count = metrics.baked_clip_count.saturating_add(1);
    bank.0.insert(key, baked.clone());
    Some(baked)
}

fn bake_clip(clip: &AnimationClip, definition: &RigDefinition) -> BakedClip {
    let duration = clip.duration().max(0.001);
    let frames = (duration / SAMPLE_DT).ceil() as usize + 1;
    let translation_field = animated_field!(Transform::translation);
    let rotation_field = animated_field!(Transform::rotation);
    let scale_field = animated_field!(Transform::scale);
    let tracks = definition
        .joints
        .iter()
        .map(|joint| {
            let sample_time = |frame: usize| (frame as f32 * SAMPLE_DT).min(duration);
            let mut translations = (0..frames)
                .map(|frame| {
                    clip.sample_clamped(translation_field.clone(), joint.target, sample_time(frame))
                        .unwrap_or(joint.bind.translation)
                })
                .collect::<Vec<_>>();
            let rotations = (0..frames)
                .map(|frame| {
                    clip.sample_clamped(rotation_field.clone(), joint.target, sample_time(frame))
                        .unwrap_or(joint.bind.rotation)
                })
                .collect::<Vec<_>>();
            let scales = (0..frames)
                .map(|frame| {
                    clip.sample_clamped(scale_field.clone(), joint.target, sample_time(frame))
                        .unwrap_or(joint.bind.scale)
                })
                .collect::<Vec<_>>();
            if joint.strips_root_translation {
                translations.fill(joint.bind.translation);
            }
            BoneTrack {
                translations,
                rotations,
                scales,
            }
        })
        .collect();
    BakedClip {
        duration,
        frame_dt: SAMPLE_DT,
        frames,
        tracks,
    }
}

fn sanitize_pose(pose: LocalPose, fallback: LocalPose) -> LocalPose {
    if pose.translation.is_finite()
        && pose.rotation.is_finite()
        && pose.rotation.length_squared() > 1e-8
        && pose.scale.is_finite()
    {
        LocalPose {
            rotation: pose.rotation.normalize(),
            ..pose
        }
    } else {
        fallback
    }
}

fn samples_due(accumulator: f32) -> u32 {
    (accumulator.max(0.0) / SAMPLE_DT).floor() as u32
}

fn hemisphere_slerp(first: Quat, mut second: Quat, alpha: f32) -> Quat {
    if first.dot(second) < 0.0 {
        second = -second;
    }
    first.slerp(second, alpha.clamp(0.0, 1.0)).normalize()
}

fn shortest_rotation(rotation: Quat) -> Quat {
    if rotation.w < 0.0 {
        -rotation
    } else {
        rotation
    }
}

fn quaternion_exp(value: Vec3) -> Quat {
    let half_angle = value.length();
    if half_angle < 1e-8 {
        Quat::from_xyzw(value.x, value.y, value.z, 1.0).normalize()
    } else {
        let scale = half_angle.sin() / half_angle;
        Quat::from_xyzw(
            scale * value.x,
            scale * value.y,
            scale * value.z,
            half_angle.cos(),
        )
    }
}

fn quaternion_log(rotation: Quat) -> Vec3 {
    let vector = Vec3::new(rotation.x, rotation.y, rotation.z);
    let length = vector.length();
    if length < 1e-8 {
        vector
    } else {
        rotation.w.clamp(-1.0, 1.0).acos() * vector / length
    }
}

fn scaled_angle_axis(rotation: Quat) -> Vec3 {
    2.0 * quaternion_log(rotation)
}

fn quaternion_angular_velocity(next: Quat, current: Quat, delta_seconds: f32) -> Vec3 {
    scaled_angle_axis(shortest_rotation(next * current.inverse())) / delta_seconds.max(1e-5)
}

fn halflife_to_damping(halflife: f32) -> f32 {
    (4.0 * core::f32::consts::LN_2) / (halflife + 1e-5)
}

fn decay_spring_vec3(value: &mut Vec3, velocity: &mut Vec3, halflife: f32, delta: f32) {
    let damping = halflife_to_damping(halflife) / 2.0;
    let intermediate = *velocity + *value * damping;
    let decay = (-damping * delta.max(0.0)).exp();
    *value = decay * (*value + intermediate * delta);
    *velocity = decay * (*velocity - intermediate * damping * delta);
}

fn decay_spring_quaternion(
    value: &mut Quat,
    angular_velocity: &mut Vec3,
    halflife: f32,
    delta: f32,
) {
    let damping = halflife_to_damping(halflife) / 2.0;
    let angle = scaled_angle_axis(*value);
    let intermediate = *angular_velocity + angle * damping;
    let decay = (-damping * delta.max(0.0)).exp();
    *value = quaternion_exp(decay * (angle + intermediate * delta) / 2.0);
    *angular_velocity = decay * (*angular_velocity - intermediate * damping * delta);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(translation: Vec3, rotation: Quat) -> LocalPose {
        LocalPose {
            translation,
            rotation,
            scale: Vec3::ONE,
        }
    }

    #[test]
    fn quaternion_antipodes_interpolate_without_a_teleport() {
        let rotation = Quat::from_rotation_y(1.2);
        let halfway = hemisphere_slerp(rotation, -rotation, 0.5);
        assert!(rotation.angle_between(halfway) < 0.0001);
    }

    #[test]
    fn angular_velocity_uses_the_short_quaternion_hemisphere() {
        let rotation = Quat::from_rotation_x(0.4);
        let velocity = quaternion_angular_velocity(-rotation, rotation, SAMPLE_DT);
        assert!(velocity.length() < 0.0001);
    }

    #[test]
    fn large_frame_gaps_are_bounded_but_consume_sampler_debt() {
        assert_eq!(samples_due(0.0), 0);
        assert_eq!(samples_due(SAMPLE_DT * 3.2), 3);
        assert_eq!(samples_due(0.1), 3);
    }

    #[test]
    fn non_finite_samples_fall_back_to_the_bind_pose() {
        let bind = pose(Vec3::Y, Quat::IDENTITY);
        let invalid = pose(Vec3::splat(f32::NAN), Quat::IDENTITY);
        assert_eq!(sanitize_pose(invalid, bind), bind);
    }

    #[test]
    fn chained_interruptions_preserve_the_displayed_pose() {
        let first = pose(Vec3::new(1.0, 0.0, 0.0), Quat::from_rotation_y(0.4));
        let second = pose(Vec3::new(-1.0, 0.0, 0.0), Quat::from_rotation_y(-0.7));
        let third = pose(Vec3::new(0.0, 1.0, 0.0), Quat::from_rotation_x(0.8));
        let mut offset = JointInertialOffset::default();
        offset.capture(
            first,
            Vec3::ZERO,
            Vec3::ZERO,
            second,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        let displayed = offset.update(second, 0.025);
        offset.capture(
            displayed,
            Vec3::ZERO,
            Vec3::ZERO,
            third,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        let after_interrupt = offset.peek(third);
        assert!(displayed.translation.distance(after_interrupt.translation) < 0.0001);
        assert!(displayed.rotation.angle_between(after_interrupt.rotation) < 0.0001);
    }

    #[test]
    fn critically_damped_offsets_remain_finite_after_a_large_delta() {
        let mut offset = JointInertialOffset::default();
        offset.translation = Vec3::splat(100.0);
        offset.rotation = Quat::from_rotation_z(2.8);
        let result = offset.update(pose(Vec3::ZERO, Quat::IDENTITY), 2.0);
        assert!(result.translation.is_finite());
        assert!(result.rotation.is_finite());
        assert!(result.translation.length() < 0.01);
        assert!(result.rotation.angle_between(Quat::IDENTITY) < 0.01);
    }
}
