//! Fixed-rate local-pose sampling and interruption-safe inertialization.
//!
//! This is deliberately downstream of semantic animation-pack resolution:
//! the semantic router produces an `AnimationPlayback` plan from the shared
//! `PresentedSkeleton`. This backend samples sparse locomotion anchors with
//! Bevy-side interpolation; other motions use clip curves baked onto a 30 Hz
//! grid. Both paths write per-character local-pose buffers before the shared
//! procedural passes.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bevy::{
    animation::{AnimationTargetId, animated_field},
    asset::{AssetEvent, AssetId},
    camera::primitives::{Frustum, Sphere},
};

use super::*;
use crate::presentation::TacticalGameplayCamera;

fn pose_tuning() -> PoseBufferConfig {
    runtime_animation_config().pose_buffer
}

fn pose_sample_seconds() -> f32 {
    pose_tuning().sample_hz.recip()
}

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
    parent: Option<usize>,
    name: Option<String>,
    lower_body: bool,
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
    animated: bool,
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

#[derive(Debug, Clone, Copy)]
struct ContinuationTransition {
    contact: LocalPose,
    ready: LocalPose,
    outgoing: LocalPose,
    start_coordinate: f32,
    incoming_tangent: f32,
    outgoing_tangent_scale: f32,
}

#[derive(Debug)]
struct SampledPlan {
    pose: Vec<LocalPose>,
    /// Pelvis rotation sampled from combat locomotion before the attack/guard
    /// rotation is applied. This is the FK reference for compensating leg IK.
    locomotion_pelvis_rotation: Option<Quat>,
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

    fn extrapolate(self, next: Self, coordinate: f32) -> Self {
        let coordinate = coordinate.clamp(
            -AttackCurve::maximum_drawback(),
            1.0 + AttackCurve::maximum_overshoot(),
        );
        self.extrapolate_unbounded(next, coordinate)
    }

    fn extrapolate_unbounded(self, next: Self, coordinate: f32) -> Self {
        let relative = shortest_rotation(next.rotation * self.rotation.inverse());
        Self {
            translation: self.translation + (next.translation - self.translation) * coordinate,
            rotation: (quaternion_exp(quaternion_log(relative) * coordinate) * self.rotation)
                .normalize(),
            scale: self.scale + (next.scale - self.scale) * coordinate,
        }
    }

    fn continuation_transition(self, transition: ContinuationTransition, progress: f32) -> Self {
        let start = self.extrapolate(transition.contact, transition.start_coordinate);
        let progress = progress.clamp(0.0, 1.0);
        let incoming_rotation = quaternion_log(shortest_rotation(
            transition.contact.rotation * self.rotation.inverse(),
        )) * transition.incoming_tangent;
        let end_rotation = quaternion_log(shortest_rotation(
            transition.ready.rotation * start.rotation.inverse(),
        ));
        let outgoing_rotation = quaternion_log(shortest_rotation(
            transition.outgoing.rotation * transition.ready.rotation.inverse(),
        )) * transition.outgoing_tangent_scale;
        Self {
            translation: cubic_hermite_vec3(
                start.translation,
                transition.ready.translation,
                (transition.contact.translation - self.translation) * transition.incoming_tangent,
                (transition.outgoing.translation - transition.ready.translation)
                    * transition.outgoing_tangent_scale,
                progress,
            ),
            rotation: (quaternion_exp(cubic_hermite_vec3(
                Vec3::ZERO,
                end_rotation,
                incoming_rotation,
                outgoing_rotation,
                progress,
            )) * start.rotation)
                .normalize(),
            scale: cubic_hermite_vec3(
                start.scale,
                transition.ready.scale,
                (transition.contact.scale - self.scale) * transition.incoming_tangent,
                (transition.outgoing.scale - transition.ready.scale)
                    * transition.outgoing_tangent_scale,
                progress,
            ),
        }
    }
}

fn cubic_hermite_vec3(
    start: Vec3,
    end: Vec3,
    start_velocity: Vec3,
    end_velocity: Vec3,
    progress: f32,
) -> Vec3 {
    let t = progress.clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;
    start * (2.0 * t3 - 3.0 * t2 + 1.0)
        + start_velocity * (t3 - 2.0 * t2 + t)
        + end * (-2.0 * t3 + 3.0 * t2)
        + end_velocity * (t3 - t2)
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
        clips.extend(playback.extrapolated_spans.iter().flat_map(|span| {
            [
                (span.start.handle.id(), span.start.layer),
                (span.end.handle.id(), span.end.layer),
            ]
        }));
        clips.extend(playback.continuation_spans.iter().flat_map(|span| {
            [
                (span.start.handle.id(), span.start.layer),
                (span.contact.handle.id(), span.contact.layer),
                (span.end.handle.id(), span.end.layer),
                (span.outgoing.handle.id(), span.outgoing.layer),
            ]
        }));
        clips.sort_by_key(|(id, layer)| (*id, *layer as u8));
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
    terrain_plants: [Option<AuthoredContactPlant>; 2],
    plan: Option<PosePlanKey>,
    active: bool,
    frozen: bool,
}

#[derive(Clone, Copy, Debug)]
struct AuthoredContactPlant {
    position_world: Vec3,
    rotation_world: Quat,
    /// The stance anchor at acquisition, expressed in owner space. Advancing
    /// authored clip phase must not consume the plant's reach allowance; only
    /// gameplay translation/turning of the owner moves this reference.
    reference_owner_position: Vec3,
    reference_owner_rotation: Quat,
}

impl PoseBufferRig {
    fn displayed_pose(&self, joint: usize) -> LocalPose {
        let pose = self.previous[joint].interpolate(self.next[joint], self.interpolation_alpha);
        self.offsets[joint].peek(pose)
    }

    fn displayed_velocity(&self, joint: usize) -> (Vec3, Vec3) {
        (
            (self.next[joint].translation - self.previous[joint].translation)
                / pose_sample_seconds(),
            quaternion_angular_velocity(
                self.next[joint].rotation,
                self.previous[joint].rotation,
                pose_sample_seconds(),
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
            pose_tuning().inertial_halflife_seconds,
            delta_seconds,
        );
        decay_spring_quaternion(
            &mut self.rotation,
            &mut self.angular_velocity,
            pose_tuning().inertial_halflife_seconds,
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

#[expect(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "Bevy injects each independently borrowed animation resource and query as a system parameter"
)]
pub(super) fn update_pose_buffers(
    mut commands: Commands,
    time: Res<Time>,
    procedural_clock: Res<ProceduralAnimationClock>,
    catalog: Res<AnimationPackCatalog>,
    clips: Res<Assets<AnimationClip>>,
    cameras: Query<(&GlobalTransform, &Frustum), With<TacticalGameplayCamera>>,
    terrain: Query<&SceneTerrain>,
    terrain_ik_enabled: Res<TerrainIkEnabled>,
    owners: Query<(
        Entity,
        &PresentedSkeleton,
        &AnimationPlayback,
        &GlobalTransform,
        Option<&mut PoseBufferRig>,
    )>,
    rig_scenes: Query<(&AnimationRigScene, &Transform)>,
    targets: Query<(
        Entity,
        &AnimationTargetId,
        &AuthoredBindTransform,
        Option<&Name>,
        Option<&ChildOf>,
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
                .filter(|(_, _, bind, _, _)| bind.owner == owner)
                .map(|(entity, target, bind, name, parent)| {
                    (
                        entity,
                        *target,
                        LocalPose::from_transform(bind.local),
                        name.map(|name| name.as_str().to_owned()),
                        parent.map(ChildOf::parent),
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
                        .map(|(entity, target, bind, name, parent)| {
                            (*entity, target, bind, name, *parent)
                        })
                        .collect::<Vec<_>>();
                    ordered.sort_by(|left, right| left.3.cmp(right.3));
                    let indices = ordered
                        .iter()
                        .enumerate()
                        .map(|(index, (entity, ..))| (*entity, index))
                        .collect::<HashMap<_, _>>();
                    Arc::new(RigDefinition {
                        family: family.clone(),
                        joints: ordered
                            .into_iter()
                            .map(|(_entity, target, bind, name, parent)| RigJoint {
                                target: *target,
                                bind: *bind,
                                parent: parent.and_then(|parent| indices.get(&parent).copied()),
                                name: name.clone(),
                                lower_body: name
                                    .as_deref()
                                    .is_some_and(is_lower_body_animation_target),
                            })
                            .collect(),
                    })
                })
                .clone();
            let by_target = found
                .into_iter()
                .map(|(entity, target, _, _, _)| (target, entity))
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
                terrain_plants: [None; 2],
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
                > pose_tuning().cull_distance_metres * pose_tuning().cull_distance_metres
                || !frustum.intersects_sphere(
                    &Sphere {
                        center: position.into(),
                        radius: pose_tuning().cull_radius_metres,
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
        let Some(mut sampled) =
            sample_plan(playback, &rig.definition, &clips, &mut bank, &mut metrics)
        else {
            continue;
        };
        let locomotion_ik_owns = procedural::authored_locomotion_ik_owns(skeleton);
        let terrain = (terrain_ik_enabled.0 && locomotion_ik_owns)
            .then(|| terrain.single().ok())
            .flatten();
        if terrain.is_some() || sampled.locomotion_pelvis_rotation.is_some() {
            let definition = Arc::clone(&rig.definition);
            // Buffered joint poses are relative to the authored scene, not to
            // the controller entity. The scene is translated from the
            // capsule's centre to the character's visual ground origin.
            // Omitting that transform makes every sampled ankle appear about
            // one capsule half-height too high and disables terrain contact.
            let presentation_transform = rig_scenes
                .iter()
                .find_map(|(scene, local)| {
                    (scene.0 == owner).then(|| presentation_world_transform(owner_transform, local))
                })
                .unwrap_or(*owner_transform);
            conform_upcoming_pose_to_terrain(
                &definition,
                &mut sampled.pose,
                &mut rig.terrain_plants,
                PoseConformity {
                    owner: &presentation_transform,
                    weights: playback.foot_ik_weights,
                    terrain,
                    contact_plants: ContactPlantPolicy::Reset,
                    locomotion_pelvis_rotation: sampled.locomotion_pelvis_rotation,
                },
            );
            // Translating authored cycles own their complete XZ trajectories.
            // Stationary guard turning uses the separate procedural pole-limit
            // plant path; never carry a world-space plant through this terrain
            // conformity pass.
        } else {
            rig.terrain_plants = [None; 2];
        }
        let target = sampled.pose;
        if transition {
            let capture_displayed = rig.active;
            for (joint, target_pose) in target.iter().copied().enumerate() {
                let buffered_displayed = rig.displayed_pose(joint);
                let displayed = if capture_displayed {
                    // Animation evaluation may already have written the new
                    // plan into live transforms this frame. The pose buffer is
                    // the authoritative previous authored output; procedural
                    // lean, secondary motion, and IK retain their own state and
                    // are reapplied after this pass.
                    buffered_displayed
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
            if playback.sampling_cadence() == PoseSamplingCadence::Direct {
                // The semantic curve supplies the continuous target. Preserve
                // transition inertialization, but do not quantize its normal
                // updates through the authored-clip sampling accumulator.
                rig.previous.clone_from(&target);
                rig.next = target;
                rig.sample_accumulator = 0.0;
                rig.interpolation_alpha = 1.0;
            } else {
                let due = samples_due(rig.sample_accumulator);
                if due > 0 {
                    rig.previous = rig.next.clone();
                    rig.next = target;
                    rig.sample_accumulator =
                        (rig.sample_accumulator - pose_sample_seconds() * due as f32).max(0.0);
                }
                // Terrain IK has already modified `next`. Interpolate local
                // joint transforms from the preceding solved sample to that
                // upcoming solved sample; no post-interpolation contact toggle
                // is needed.
                rig.interpolation_alpha =
                    (rig.sample_accumulator / pose_sample_seconds()).clamp(0.0, 1.0);
            }
        }
    }
}

fn presentation_world_transform(
    owner: &GlobalTransform,
    rig_scene_local: &Transform,
) -> GlobalTransform {
    owner.mul_transform(*rig_scene_local)
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
) -> Option<SampledPlan> {
    if playback.use_authored_bind_pose
        || (playback.clips.is_empty() && playback.extrapolated_spans.is_empty())
    {
        return Some(SampledPlan {
            pose: definition.joints.iter().map(|joint| joint.bind).collect(),
            locomotion_pelvis_rotation: None,
        });
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
        baked.push((weighted, clip, clips.get(&weighted.clip.handle)?));
    }
    let mut baked_spans = Vec::with_capacity(playback.extrapolated_spans.len());
    for span in &playback.extrapolated_spans {
        let start = get_or_bake(
            span.start.handle.id(),
            &span.start.handle,
            definition,
            clips,
            bank,
            metrics,
        )?;
        let end = get_or_bake(
            span.end.handle.id(),
            &span.end.handle,
            definition,
            clips,
            bank,
            metrics,
        )?;
        baked_spans.push((span, start, end));
    }
    let mut baked_continuations = Vec::with_capacity(playback.continuation_spans.len());
    for span in &playback.continuation_spans {
        let start = get_or_bake(
            span.start.handle.id(),
            &span.start.handle,
            definition,
            clips,
            bank,
            metrics,
        )?;
        let contact = get_or_bake(
            span.contact.handle.id(),
            &span.contact.handle,
            definition,
            clips,
            bank,
            metrics,
        )?;
        let end = get_or_bake(
            span.end.handle.id(),
            &span.end.handle,
            definition,
            clips,
            bank,
            metrics,
        )?;
        let outgoing = get_or_bake(
            span.outgoing.handle.id(),
            &span.outgoing.handle,
            definition,
            clips,
            bank,
            metrics,
        )?;
        baked_continuations.push((span, start, contact, end, outgoing));
    }
    let mut pose = Vec::with_capacity(definition.joints.len());
    let mut locomotion_pelvis_rotation = None;
    for (joint_index, joint) in definition.joints.iter().enumerate() {
        let mut blended = joint.bind;
        let mut accumulated = 0.0_f32;
        let pelvis = joint
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("root"));
        let mut combat_upper = joint.bind;
        let mut combat_upper_weight = 0.0_f32;
        let mut combat_lower = joint.bind;
        let mut combat_lower_weight = 0.0_f32;
        for (weighted, clip, source) in &baked {
            let included = match weighted.clip.layer {
                ClipLayer::Whole => true,
                ClipLayer::Upper => !joint.lower_body,
                ClipLayer::Lower => joint.lower_body,
                ClipLayer::CombatUpper => !joint.lower_body || pelvis,
                ClipLayer::CombatLower => joint.lower_body,
                ClipLayer::MainHand | ClipLayer::Offhand => false,
            };
            if !included || weighted.weight <= f32::EPSILON || !weighted.weight.is_finite() {
                continue;
            }
            let sample = if weighted.locomotion_phase.is_some() {
                let authored_phase = weighted.time_seconds / source.duration().max(f32::EPSILON);
                sample_two_pose_locomotion_clip(source, joint, authored_phase)
            } else {
                clip.sample(joint_index, weighted.time_seconds)
            };
            let sample = sanitize_pose(sample, joint.bind);
            if pelvis && weighted.clip.layer == ClipLayer::CombatUpper {
                accumulate_local_pose(
                    &mut combat_upper,
                    &mut combat_upper_weight,
                    sample,
                    weighted.weight,
                );
                continue;
            }
            if pelvis && weighted.clip.layer == ClipLayer::CombatLower {
                accumulate_local_pose(
                    &mut combat_lower,
                    &mut combat_lower_weight,
                    sample,
                    weighted.weight,
                );
                continue;
            }
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
        for (span, start_clip, end_clip) in &baked_spans {
            let included = match span.start.layer {
                ClipLayer::Whole => true,
                ClipLayer::Upper => !joint.lower_body,
                ClipLayer::Lower => joint.lower_body,
                ClipLayer::CombatUpper => !joint.lower_body || pelvis,
                ClipLayer::CombatLower => joint.lower_body,
                ClipLayer::MainHand | ClipLayer::Offhand => false,
            };
            if !included || span.weight <= f32::EPSILON || !span.weight.is_finite() {
                continue;
            }
            let start = sanitize_pose(
                start_clip.sample(joint_index, span.start_time_seconds),
                joint.bind,
            );
            let end = sanitize_pose(
                end_clip.sample(joint_index, span.end_time_seconds),
                joint.bind,
            );
            // CurveSpan is defined by its two semantic anchors, not the keys
            // authored between or after them. Applying their transform delta
            // directly gives one constant spatial path through coordinate 1,
            // so contact cannot introduce a hidden ease-out/ease-in stop.
            let sample = start.extrapolate(end, span.coordinate);
            let sample = sanitize_pose(sample, joint.bind);
            if pelvis && span.start.layer == ClipLayer::CombatUpper {
                accumulate_local_pose(
                    &mut combat_upper,
                    &mut combat_upper_weight,
                    sample,
                    span.weight,
                );
                continue;
            }
            if pelvis && span.start.layer == ClipLayer::CombatLower {
                accumulate_local_pose(
                    &mut combat_lower,
                    &mut combat_lower_weight,
                    sample,
                    span.weight,
                );
                continue;
            }
            let next_total = accumulated + span.weight;
            let alpha = if next_total > f32::EPSILON {
                span.weight / next_total
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
        for (span, start_clip, contact_clip, end_clip, outgoing_clip) in &baked_continuations {
            let included = match span.start.layer {
                ClipLayer::Whole => true,
                ClipLayer::Upper => !joint.lower_body,
                ClipLayer::Lower => joint.lower_body,
                ClipLayer::CombatUpper => !joint.lower_body || pelvis,
                ClipLayer::CombatLower => joint.lower_body,
                ClipLayer::MainHand | ClipLayer::Offhand => false,
            };
            if !included || span.weight <= f32::EPSILON || !span.weight.is_finite() {
                continue;
            }
            let start = sanitize_pose(
                start_clip.sample(joint_index, span.start_time_seconds),
                joint.bind,
            );
            let contact = sanitize_pose(
                contact_clip.sample(joint_index, span.contact_time_seconds),
                joint.bind,
            );
            let end = sanitize_pose(
                end_clip.sample(joint_index, span.end_time_seconds),
                joint.bind,
            );
            let outgoing = sanitize_pose(
                outgoing_clip.sample(joint_index, span.outgoing_time_seconds),
                joint.bind,
            );
            let sample = sanitize_pose(
                start.continuation_transition(
                    ContinuationTransition {
                        contact,
                        ready: end,
                        outgoing,
                        start_coordinate: span.start_coordinate,
                        incoming_tangent: span.incoming_tangent,
                        outgoing_tangent_scale: span.outgoing_tangent_scale,
                    },
                    span.progress,
                ),
                joint.bind,
            );
            if pelvis && span.start.layer == ClipLayer::CombatUpper {
                accumulate_local_pose(
                    &mut combat_upper,
                    &mut combat_upper_weight,
                    sample,
                    span.weight,
                );
                continue;
            }
            if pelvis && span.start.layer == ClipLayer::CombatLower {
                accumulate_local_pose(
                    &mut combat_lower,
                    &mut combat_lower_weight,
                    sample,
                    span.weight,
                );
                continue;
            }
            let next_total = accumulated + span.weight;
            let alpha = if next_total > f32::EPSILON {
                span.weight / next_total
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
        let mut result =
            if pelvis && combat_upper_weight > f32::EPSILON && combat_lower_weight > f32::EPSILON {
                locomotion_pelvis_rotation = Some(combat_lower.rotation);
                combine_combat_pelvis(combat_upper, combat_lower)
            } else if accumulated > f32::EPSILON {
                blended
            } else {
                joint.bind
            };
        if let Some(hand) = hand_animation_joint(definition, joint_index) {
            let mut hand_pose = joint.bind;
            let mut hand_weight = 0.0_f32;
            for (weighted, clip, _) in &baked {
                if weighted.clip.layer != hand
                    || !clip.tracks[joint_index].animated
                    || weighted.weight <= f32::EPSILON
                    || !weighted.weight.is_finite()
                {
                    continue;
                }
                let sample =
                    sanitize_pose(clip.sample(joint_index, weighted.time_seconds), joint.bind);
                let total = hand_weight + weighted.weight;
                hand_pose = if hand_weight <= f32::EPSILON {
                    sample
                } else {
                    hand_pose.interpolate(sample, weighted.weight / total)
                };
                hand_weight = total;
            }
            if hand_weight > f32::EPSILON {
                result = hand_pose;
            }
        }
        pose.push(result);
    }
    metrics.sampled_pose_count = metrics.sampled_pose_count.saturating_add(1);
    Some(SampledPlan {
        pose,
        locomotion_pelvis_rotation,
    })
}

fn accumulate_local_pose(
    accumulated_pose: &mut LocalPose,
    accumulated_weight: &mut f32,
    sample: LocalPose,
    weight: f32,
) {
    let next_total = *accumulated_weight + weight;
    *accumulated_pose = if *accumulated_weight <= f32::EPSILON {
        sample
    } else {
        accumulated_pose.interpolate(sample, weight / next_total)
    };
    *accumulated_weight = next_total;
}

fn combine_combat_pelvis(combat: LocalPose, locomotion: LocalPose) -> LocalPose {
    LocalPose {
        translation: locomotion.translation,
        rotation: combat.rotation,
        scale: locomotion.scale,
    }
}

fn sample_source_clip(clip: &AnimationClip, joint: &RigJoint, time_seconds: f32) -> LocalPose {
    let translation_field = animated_field!(Transform::translation);
    let rotation_field = animated_field!(Transform::rotation);
    let scale_field = animated_field!(Transform::scale);
    LocalPose {
        translation: clip
            .sample_clamped(translation_field, joint.target, time_seconds)
            .unwrap_or(joint.bind.translation),
        rotation: clip
            .sample_clamped(rotation_field, joint.target, time_seconds)
            .unwrap_or(joint.bind.rotation),
        scale: clip
            .sample_clamped(scale_field, joint.target, time_seconds)
            .unwrap_or(joint.bind.scale),
    }
}

fn sample_two_pose_locomotion_clip(
    clip: &AnimationClip,
    joint: &RigJoint,
    authored_phase: f32,
) -> LocalPose {
    // The source owns two unique poses: contact and passing/flight. Their
    // mirrored counterparts occupy the other half-cycle. Sample only those
    // four semantic anchors, then own every in-between here instead of asking
    // the glTF sampler (or the 30 Hz baked cache) for intermediate frames.
    let (start_index, end_index, amount) = sparse_locomotion_segment(authored_phase);
    const ANCHOR_COUNT: f32 = 4.0;
    let sample_anchor =
        |index: u32| sample_source_clip(clip, joint, clip.duration() * index as f32 / ANCHOR_COUNT);
    sample_anchor(start_index).interpolate(sample_anchor(end_index), amount)
}

fn sparse_locomotion_segment(authored_phase: f32) -> (u32, u32, f32) {
    let coordinate = authored_phase.rem_euclid(1.0) * 4.0;
    let start = coordinate.floor() as u32;
    let t = coordinate - start as f32;
    (start, (start + 1) % 4, t * t * (3.0 - 2.0 * t))
}

fn hand_animation_joint(definition: &RigDefinition, mut joint: usize) -> Option<ClipLayer> {
    loop {
        if let Some(name) = definition.joints[joint].name.as_deref() {
            if name.eq_ignore_ascii_case("r_wrist") {
                return Some(ClipLayer::MainHand);
            }
            if name.eq_ignore_ascii_case("l_wrist") {
                return Some(ClipLayer::Offhand);
            }
        }
        let parent = definition.joints[joint].parent?;
        joint = parent;
    }
}

/// Solve the terrain-adjusted upcoming sample in pose-buffer space. The
/// renderer subsequently interpolates `previous -> next`, so terrain
/// conformity follows exactly the same continuous local-pose interpolation as
/// authored FK instead of being switched onto the already displayed frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContactPlantPolicy {
    Reset,
    Retain,
}

#[derive(Debug, Clone, Copy)]
struct PoseConformity<'a> {
    owner: &'a GlobalTransform,
    weights: Vec2,
    terrain: Option<&'a SceneTerrain>,
    contact_plants: ContactPlantPolicy,
    locomotion_pelvis_rotation: Option<Quat>,
}

fn conform_upcoming_pose_to_terrain(
    definition: &RigDefinition,
    pose: &mut [LocalPose],
    plants: &mut [Option<AuthoredContactPlant>; 2],
    conformity: PoseConformity<'_>,
) {
    let PoseConformity {
        owner,
        weights,
        terrain,
        contact_plants,
        locomotion_pelvis_rotation,
    } = conformity;
    if contact_plants == ContactPlantPolicy::Reset {
        *plants = [None; 2];
    }
    let locomotion_reference = locomotion_pelvis_rotation.and_then(|rotation| {
        definition
            .joints
            .iter()
            .position(|joint| {
                joint
                    .name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case("root"))
            })
            .map(|pelvis| {
                let mut reference = pose.to_vec();
                reference[pelvis].rotation = rotation;
                reference
            })
    });
    let mut locomotion_reference_cache = locomotion_reference
        .as_ref()
        .map(|reference| vec![None; reference.len()]);
    for (index, (left, weight, names)) in [
        (true, weights.x, ["l_upleg", "l_lowleg", "l_foot"]),
        (false, weights.y, ["r_upleg", "r_lowleg", "r_foot"]),
    ]
    .into_iter()
    .enumerate()
    {
        let weight = weight.clamp(0.0, 1.0);
        if weight <= f32::EPSILON && locomotion_pelvis_rotation.is_none() {
            plants[index] = None;
            continue;
        }
        let [Some(upper), Some(lower), Some(foot)] = names.map(|name| {
            definition.joints.iter().position(|joint| {
                joint
                    .name
                    .as_deref()
                    .is_some_and(|joint_name| joint_name.eq_ignore_ascii_case(name))
            })
        }) else {
            continue;
        };
        let mut cache = vec![None; pose.len()];
        let upper_global = local_pose_global(definition, pose, upper, &mut cache);
        let lower_global = local_pose_global(definition, pose, lower, &mut cache);
        let foot_global = local_pose_global(definition, pose, foot, &mut cache);
        let foot_world = owner.transform_point(foot_global.translation);
        let reference_foot = match (&locomotion_reference, locomotion_reference_cache.as_mut()) {
            (Some(reference), Some(cache)) => local_pose_global(definition, reference, foot, cache),
            _ => foot_global,
        };
        let reference_foot_world = owner.transform_point(reference_foot.translation);
        let reference_foot_rotation_world = owner.rotation() * reference_foot.rotation;
        let terrain_target_world = terrain
            .and_then(|terrain| terrain.height_at(reference_foot_world.xz()))
            .map(|height| {
                reference_foot_world.with_y(
                    reference_foot_world
                        .y
                        .max(height + measured_ankle_sole_offset_metres()),
                )
            })
            .unwrap_or(reference_foot_world);
        let supported =
            terrain.is_some() && contact_plants == ContactPlantPolicy::Retain && weight > 0.05;
        if !supported {
            plants[index] = None;
        } else if plants[index].is_none() {
            plants[index] = Some(AuthoredContactPlant {
                position_world: terrain_target_world,
                rotation_world: reference_foot_rotation_world,
                reference_owner_position: owner
                    .affine()
                    .inverse()
                    .transform_point3(terrain_target_world),
                reference_owner_rotation: owner.rotation().inverse()
                    * reference_foot_rotation_world,
            });
        }
        if let Some(mut plant) = plants[index] {
            let reference_world = owner.transform_point(plant.reference_owner_position);
            let reference_rotation_world = owner.rotation() * plant.reference_owner_rotation;
            let displacement = reference_world.xz() - plant.position_world.xz();
            let distance = displacement.length();
            if distance > pose_tuning().authored_contact_plant_limit_metres {
                let excess = distance - pose_tuning().authored_contact_plant_limit_metres;
                plant.position_world +=
                    Vec3::new(displacement.x, 0.0, displacement.y).normalize_or_zero() * excess;
                plant.rotation_world = hemisphere_slerp(
                    plant.rotation_world,
                    reference_rotation_world,
                    (excess / distance).clamp(0.0, 1.0),
                );
            }
            if let Some(plant_height) =
                terrain.and_then(|terrain| terrain.height_at(plant.position_world.xz()))
            {
                plant.position_world.y = plant
                    .position_world
                    .y
                    .max(plant_height + measured_ankle_sole_offset_metres());
            }
            plants[index] = Some(plant);
        }
        let (terrain_world, preserved_foot_rotation) = if let Some(plant) = plants[index] {
            (
                // Contact ownership is discrete. Smoothness comes from the
                // pose buffer interpolating the previous displayed pose into
                // this conformed upcoming target; blending the target itself
                // by the contact scalar lets the planted ankle follow the
                // authored clip and the rotating owner.
                plant.position_world,
                owner.rotation().inverse() * plant.rotation_world,
            )
        } else {
            (terrain_target_world, reference_foot.rotation)
        };
        if terrain_world.distance(foot_world) <= 0.0001
            && preserved_foot_rotation.angle_between(foot_global.rotation) <= 0.0001
        {
            // An identity target is not an identity two-bone solve: pole
            // selection can still perturb an authored chain. Treat an already
            // clear upcoming sample as a strict no-op.
            continue;
        }
        // Authored locomotion is calibrated against flat ground. A fixed
        // ankle-to-sole target is only a lower bound here: lowering a pitched
        // or non-flat authored combat foot changes its intended sole clearance
        // and can bury the visible mesh even on level terrain. Preserve the
        // upcoming authored height unless terrain actually rises into it.
        let target_world = if plants[index].is_some() {
            terrain_world
        } else {
            reference_foot_world.lerp(terrain_world, weight)
        };
        let target = owner.affine().inverse().transform_point3(target_world);
        let hip = upper_global.translation;
        let knee = lower_global.translation;
        let ankle = foot_global.translation;
        let upper_length = hip.distance(knee);
        let lower_length = knee.distance(ankle);
        let Some(solution) = solve_pose_two_bone(
            hip,
            knee,
            target,
            upper_length,
            lower_length,
            if left { Vec3::NEG_Z } else { Vec3::Z },
        ) else {
            continue;
        };
        aim_pose_joint(definition, pose, upper, knee - hip, solution.0 - hip);
        let mut cache = vec![None; pose.len()];
        let lower_after = local_pose_global(definition, pose, lower, &mut cache);
        let foot_after = local_pose_global(definition, pose, foot, &mut cache);
        aim_pose_joint(
            definition,
            pose,
            lower,
            foot_after.translation - lower_after.translation,
            solution.1 - solution.0,
        );
        let mut cache = vec![None; pose.len()];
        let parent_rotation = definition.joints[foot]
            .parent
            .map(|parent| local_pose_global(definition, pose, parent, &mut cache).rotation)
            .unwrap_or(Quat::IDENTITY);
        let local = parent_rotation.inverse() * preserved_foot_rotation;
        if local.is_finite() {
            pose[foot].rotation = local.normalize();
        }
    }
}

fn local_pose_global(
    definition: &RigDefinition,
    pose: &[LocalPose],
    joint: usize,
    cache: &mut [Option<Transform>],
) -> Transform {
    if let Some(global) = cache[joint] {
        return global;
    }
    let local = Transform {
        translation: pose[joint].translation,
        rotation: pose[joint].rotation,
        scale: pose[joint].scale,
    };
    let global = definition.joints[joint].parent.map_or(local, |parent| {
        local_pose_global(definition, pose, parent, cache).mul_transform(local)
    });
    cache[joint] = Some(global);
    global
}

fn aim_pose_joint(
    definition: &RigDefinition,
    pose: &mut [LocalPose],
    joint: usize,
    from: Vec3,
    to: Vec3,
) {
    let (Some(from), Some(to)) = (from.try_normalize(), to.try_normalize()) else {
        return;
    };
    let mut cache = vec![None; pose.len()];
    let current = local_pose_global(definition, pose, joint, &mut cache);
    let parent_rotation = definition.joints[joint]
        .parent
        .map(|parent| local_pose_global(definition, pose, parent, &mut cache).rotation)
        .unwrap_or(Quat::IDENTITY);
    let desired_world = Quat::from_rotation_arc(from, to) * current.rotation;
    let local = parent_rotation.inverse() * desired_world;
    if local.is_finite() {
        pose[joint].rotation = local.normalize();
    }
}

fn solve_pose_two_bone(
    hip: Vec3,
    current_knee: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    fallback_pole: Vec3,
) -> Option<(Vec3, Vec3)> {
    let offset = target - hip;
    let direction = offset.try_normalize()?;
    let distance = offset.length().clamp(
        (upper_length - lower_length).abs() + 0.0001,
        upper_length + lower_length - 0.0001,
    );
    let end = hip + direction * distance;
    let along = (upper_length * upper_length - lower_length * lower_length + distance * distance)
        / (2.0 * distance);
    let height = (upper_length * upper_length - along * along)
        .max(0.0)
        .sqrt();
    let bend = (current_knee - hip)
        .reject_from_normalized(direction)
        .try_normalize()
        .or_else(|| {
            fallback_pole
                .reject_from_normalized(direction)
                .try_normalize()
        })?;
    let knee = hip + direction * along + bend * height;
    (knee.is_finite() && end.is_finite()).then_some((knee, end))
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
    let frames = (duration / pose_sample_seconds()).ceil() as usize + 1;
    let translation_field = animated_field!(Transform::translation);
    let rotation_field = animated_field!(Transform::rotation);
    let scale_field = animated_field!(Transform::scale);
    let tracks = definition
        .joints
        .iter()
        .map(|joint| {
            let sample_time = |frame: usize| (frame as f32 * pose_sample_seconds()).min(duration);
            let translations = (0..frames)
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
            BoneTrack {
                translations,
                rotations,
                scales,
                animated: clip.curves().contains_key(&joint.target),
            }
        })
        .collect();
    BakedClip {
        duration,
        frame_dt: pose_sample_seconds(),
        frames,
        tracks,
    }
}

pub(super) fn calibrate_authored_locomotion_strides(
    definitions: Res<RigDefinitions>,
    runtime: Res<AnimationRuntime>,
    clips: Res<Assets<AnimationClip>>,
    mut clip_events: MessageReader<AssetEvent<AnimationClip>>,
    mut bank: ResMut<BakedClipBank>,
    mut strides: ResMut<AuthoredLocomotionStrides>,
) {
    let changed = clip_events
        .read()
        .map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::Unused { id }
            | AssetEvent::LoadedWithDependencies { id } => *id,
        })
        .collect::<HashSet<_>>();
    if !changed.is_empty() {
        let invalidated_motions = strides
            .measured_clips
            .iter()
            .filter(|(_, measured)| changed.contains(measured))
            .map(|(motion, _)| motion.clone())
            .collect::<Vec<_>>();
        for motion in invalidated_motions {
            strides.clear_motion(&motion);
        }
        strides
            .measured_clips
            .retain(|_, measured| !changed.contains(measured));
        bank.0
            .retain(|(_, measured), _| !changed.contains(measured));
    }
    let Some(definition) = definitions.0.get("humanoid") else {
        return;
    };
    for (motion, axis) in [("walk", 2), ("run", 2), ("strafe", 0), ("skip", 2)] {
        let Some(loaded) = runtime
            .clips
            .get(&(HUMANOID_UNARMED_PACK.to_owned(), motion.to_owned()))
        else {
            continue;
        };
        let id = loaded.handle.id();
        if strides.measured_clips.get(motion) == Some(&id) {
            continue;
        }
        strides.clear_motion(motion);
        let Some(clip) = clips.get(&loaded.handle) else {
            continue;
        };
        let baked = bake_clip(clip, definition);
        let calibration = match motion {
            // Walk and run use a measured distance-domain phase curve while
            // the live sampler interpolates only their sparse semantic poses.
            "walk" | "run" => {
                measure_authored_contact_step_distance(definition, &baked, axis, -1.0)
            }
            // Combat cycles currently expose alternating contact poses but no
            // typed support interval. Retain their geometric calibration until
            // that contact timing is part of the authored motion contract.
            "strafe" | "skip" => {
                measure_authored_foot_range(definition, &baked, axis).map(|step_distance| {
                    AuthoredLocomotionCalibration {
                        stride: AuthoredStrideMeasurement {
                            step_distance,
                            maximum_stance_slip: 0.0,
                        },
                        phase_curve: None,
                    }
                })
            }
            _ => unreachable!("fixed authored locomotion calibration table"),
        };
        let Some(calibration) = calibration else {
            warn!(motion, "Could not infer authored locomotion stride");
            strides.measured_clips.insert(motion.to_owned(), id);
            continue;
        };
        let AuthoredLocomotionCalibration {
            stride,
            phase_curve,
        } = calibration;
        if let Some(phase_curve) = phase_curve {
            strides.phase_curves.insert(motion.to_owned(), phase_curve);
        }
        info!(
            motion,
            stride_metres = stride.step_distance,
            maximum_stance_slip_metres = stride.maximum_stance_slip,
            "Measured authored locomotion stride"
        );
        if stride.maximum_stance_slip > presentation::maximum_authored_stance_slip_metres() {
            warn!(
                motion,
                stride_metres = stride.step_distance,
                maximum_stance_slip_metres = stride.maximum_stance_slip,
                "Authored locomotion contact fit exceeds the stance-slip budget"
            );
        }
        match motion {
            "walk" => strides.walk = Some(stride),
            "run" => strides.run = Some(stride),
            "strafe" => strides.strafe = Some(stride),
            "skip" => strides.skip = Some(stride),
            _ => unreachable!("fixed authored locomotion calibration table"),
        }
        strides.measured_clips.insert(motion.to_owned(), id);
    }
}

/// Infer travel from the low portion of each authored foot trajectory. This
/// discovers contact timing from clip geometry, then fits the single virtual
/// root velocity that best holds every stance segment still. The residual is
/// retained because some clips cannot represent a requested speed with a
/// constant playback rate alone.
#[derive(Debug, Clone)]
struct AuthoredLocomotionCalibration {
    stride: AuthoredStrideMeasurement,
    phase_curve: Option<AuthoredPhaseCurve>,
}

fn measure_authored_contact_step_distance(
    definition: &RigDefinition,
    clip: &BakedClip,
    travel_axis: usize,
    expected_stance_direction: f32,
) -> Option<AuthoredLocomotionCalibration> {
    if clip.duration <= f32::EPSILON
        || clip.frames < 4
        || expected_stance_direction.abs() <= f32::EPSILON
    {
        return None;
    }
    let feet = ["l_foot", "r_foot"];
    let sample_count = clip.frames.saturating_sub(1);
    let mut segments = Vec::new();
    for name in feet {
        let mut foot_segments = Vec::new();
        let foot = definition
            .joints
            .iter()
            .position(|joint| joint.name.as_deref() == Some(name))?;
        let samples = (0..sample_count)
            .map(|frame| {
                let time = frame as f32 * clip.frame_dt;
                let mut globals = vec![None; definition.joints.len()];
                let position = sampled_global_transform(definition, clip, foot, time, &mut globals)
                    .translation;
                (time / clip.duration, position)
            })
            .collect::<Vec<_>>();
        let minimum_height = samples
            .iter()
            .map(|(_, position)| position.y)
            .reduce(f32::min)?;
        let maximum_height = samples
            .iter()
            .map(|(_, position)| position.y)
            .reduce(f32::max)?;
        let height_window = ((maximum_height - minimum_height)
            * pose_tuning().authored_contact_height_fraction)
            .clamp(
                pose_tuning().authored_contact_minimum_height_window_metres,
                pose_tuning().authored_contact_maximum_height_window_metres,
            );
        let supported = samples
            .iter()
            .map(|(_, position)| position.y <= minimum_height + height_window)
            .collect::<Vec<_>>();
        if supported.iter().all(|supported| *supported) {
            return None;
        }

        // Start immediately after a non-contact sample so a stance crossing
        // the loop seam becomes one monotonically unwrapped segment.
        let start = supported.iter().position(|supported| !*supported)?;
        let mut current = Vec::new();
        for offset in 1..=sample_count {
            let index = (start + offset) % sample_count;
            if supported[index] {
                let wraps = (start + offset) / sample_count;
                current.push((
                    samples[index].0 + wraps as f32,
                    samples[index].1[travel_axis],
                ));
            } else if !current.is_empty() {
                if current.len() >= 3 {
                    foot_segments.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
        }
        if current.len() >= 3 {
            foot_segments.push(current);
        }
        // A low swing-through can briefly enter the height band. The actual
        // stance is the longest circular low-height interval for each foot.
        segments.push(foot_segments.into_iter().max_by_key(Vec::len)?);
    }
    if segments.len() < 2 {
        return None;
    }

    // Some source tools export cyclic locomotion with the phase direction
    // opposite to the runtime's forward gait convention. Choose one playback
    // orientation for the entire clip from the signed stance displacement;
    // reversing clip time is safe for a cycle and does not reverse physical
    // gait phase or contact ownership.
    let signed_stance_displacement = segments
        .iter()
        .map(|samples| {
            (samples.last().unwrap().1 - samples.first().unwrap().1)
                * expected_stance_direction.signum()
        })
        .sum::<f32>();
    if signed_stance_displacement.abs() <= 0.02 {
        return None;
    }
    let reverse_playback = signed_stance_displacement < 0.0;
    let semantic_contact_centers = [0.0_f32, 0.5_f32];
    let mut offsets =
        segments
            .iter()
            .zip(semantic_contact_centers)
            .map(|(samples, semantic_center)| {
                let authored_center =
                    (samples.first().unwrap().0 + samples.last().unwrap().0) * 0.5;
                if reverse_playback {
                    authored_center + semantic_center
                } else {
                    authored_center - semantic_center
                }
            });
    let first_offset = offsets.next()?.rem_euclid(1.0);
    let (offset_sum, offset_count) = offsets.fold((first_offset, 1_u32), |(sum, count), offset| {
        let aligned = first_offset + (offset - first_offset + 0.5).rem_euclid(1.0) - 0.5;
        (sum + aligned, count + 1)
    });
    let phase_offset = (offset_sum / offset_count as f32).rem_euclid(1.0);
    let oriented_segments = segments
        .iter()
        .map(|samples| {
            if reverse_playback {
                samples
                    .iter()
                    .rev()
                    .map(|&(authored_phase, position)| {
                        (phase_offset - authored_phase, authored_phase, position)
                    })
                    .collect::<Vec<_>>()
            } else {
                samples
                    .iter()
                    .map(|&(authored_phase, position)| {
                        (authored_phase - phase_offset, authored_phase, position)
                    })
                    .collect::<Vec<_>>()
            }
        })
        .collect::<Vec<_>>();

    // Reparameterize each low-foot interval by its positive travel-axis
    // displacement. This is the inverse of the articulated foot projection:
    // physical phase advances linearly in distance while authored sample phase
    // accelerates and decelerates through the corresponding joint rotation.
    // A tiny derivative floor keeps the result strictly monotone if a sampled
    // curve contains a short plateau or wrong-way interval.
    let mut phase_spans = Vec::<Vec<(f32, f32)>>::new();
    let mut warped_segments = Vec::<Vec<(f32, f32)>>::new();
    for samples in &oriented_segments {
        let phase_span = samples.last()?.0 - samples.first()?.0;
        if phase_span <= f32::EPSILON {
            return None;
        }
        let positive_deltas = samples
            .windows(2)
            .map(|pair| ((pair[1].2 - pair[0].2) * expected_stance_direction.signum()).max(0.0))
            .collect::<Vec<_>>();
        let positive_total = positive_deltas.iter().sum::<f32>();
        if positive_total <= 0.01 {
            return None;
        }
        // Blend the exact distance inverse with a uniform phase derivative.
        // Pure inversion approaches infinite playback speed where the foot's
        // projected velocity approaches zero; this floor bounds that speed
        // without baking a hand-authored timing curve into the clip.
        let derivative_floor = positive_total * 0.5 / positive_deltas.len() as f32;
        let weighted_total = positive_total + derivative_floor * positive_deltas.len() as f32;
        let mut physical_phase = samples.first()?.0;
        let mut span = vec![(physical_phase, samples.first()?.1)];
        let mut warped = vec![(physical_phase, samples.first()?.2)];
        for (index, delta) in positive_deltas.into_iter().enumerate() {
            physical_phase += phase_span * (delta + derivative_floor) / weighted_total;
            span.push((physical_phase, samples[index + 1].1));
            warped.push((physical_phase, samples[index + 1].2));
        }
        // Preserve exact interval boundaries so adjacent unwarped swing
        // sampling remains continuous.
        if let Some(last) = span.last_mut() {
            last.0 = samples.last()?.0;
        }
        if let Some(last) = warped.last_mut() {
            last.0 = samples.last()?.0;
        }
        phase_spans.push(span);
        warped_segments.push(warped);
    }

    let mut authored_phases = (0..=256)
        .map(|index| {
            let physical_phase = index as f32 / 256.0;
            sample_authored_phase_spans(
                &phase_spans,
                physical_phase,
                reverse_playback,
                phase_offset,
            )
        })
        .collect::<Vec<_>>();
    smooth_periodic_phase_curve(&mut authored_phases, reverse_playback, phase_offset);
    let phase_curve = AuthoredPhaseCurve { authored_phases };

    // Each stance gets its own intercept; only the common virtual-root slope
    // is shared between feet and contact lobes. Measure the residual after the
    // phase warp, because that is the curve actually sampled at runtime.
    let mut covariance = 0.0;
    let mut phase_variance = 0.0;
    for samples in &warped_segments {
        let count = samples.len() as f32;
        let mean_phase = samples.iter().map(|sample| sample.0).sum::<f32>() / count;
        let mean_position = samples.iter().map(|sample| sample.1).sum::<f32>() / count;
        for &(phase, position) in samples {
            let centered_phase = phase - mean_phase;
            covariance += centered_phase * (position - mean_position);
            phase_variance += centered_phase * centered_phase;
        }
    }
    if phase_variance <= f32::EPSILON {
        return None;
    }
    let fitted_slope = covariance / phase_variance;
    let cycle_distance = fitted_slope * expected_stance_direction.signum();
    let step_distance = cycle_distance * 0.5;
    let maximum_stance_slip = warped_segments
        .iter()
        .map(|samples| {
            let residuals = samples
                .iter()
                .map(|(phase, position)| position - fitted_slope * phase);
            let (minimum, maximum) = residuals.fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), residual| (minimum.min(residual), maximum.max(residual)),
            );
            maximum - minimum
        })
        .fold(0.0_f32, f32::max);
    (step_distance.is_finite()
        && maximum_stance_slip.is_finite()
        && cycle_distance > 0.0
        && (0.05..=3.0).contains(&step_distance))
    .then_some(AuthoredLocomotionCalibration {
        stride: AuthoredStrideMeasurement {
            step_distance,
            maximum_stance_slip,
        },
        phase_curve: Some(phase_curve),
    })
}

fn sample_authored_phase_spans(
    spans: &[Vec<(f32, f32)>],
    physical_phase: f32,
    reverse_playback: bool,
    phase_offset: f32,
) -> f32 {
    for span in spans {
        for cycle_offset in [-1.0, 0.0, 1.0] {
            let authored_offset = if reverse_playback {
                -cycle_offset
            } else {
                cycle_offset
            };
            let start = span[0].0 + cycle_offset;
            let end = span[span.len() - 1].0 + cycle_offset;
            if physical_phase + 0.000_001 < start || physical_phase - 0.000_001 > end {
                continue;
            }
            let upper = span.partition_point(|(phase, _)| *phase + cycle_offset < physical_phase);
            if upper == 0 {
                return span[0].1 + authored_offset;
            }
            if upper >= span.len() {
                return span[span.len() - 1].1 + authored_offset;
            }
            let (lower_phase, lower_authored) = span[upper - 1];
            let (upper_phase, upper_authored) = span[upper];
            let amount = ((physical_phase - (lower_phase + cycle_offset))
                / (upper_phase - lower_phase))
                .clamp(0.0, 1.0);
            return (lower_authored + authored_offset)
                .lerp(upper_authored + authored_offset, amount);
        }
    }
    if reverse_playback {
        phase_offset - physical_phase
    } else {
        physical_phase + phase_offset
    }
}

fn smooth_periodic_phase_curve(values: &mut [f32], reverse: bool, phase_offset: f32) {
    let sample_count = values.len().saturating_sub(1);
    if sample_count < 3 {
        return;
    }
    let deviations = (0..sample_count)
        .map(|index| {
            let phase = index as f32 / sample_count as f32;
            let base = if reverse {
                phase_offset - phase
            } else {
                phase_offset + phase
            };
            values[index] - base
        })
        .collect::<Vec<_>>();
    const RADIUS: isize = 6;
    for (index, value) in values.iter_mut().take(sample_count).enumerate() {
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        for offset in -RADIUS..=RADIUS {
            let wrapped = (index as isize + offset).rem_euclid(sample_count as isize) as usize;
            let weight = (RADIUS + 1 - offset.abs()) as f32;
            weighted_sum += deviations[wrapped] * weight;
            total_weight += weight;
        }
        let phase = index as f32 / sample_count as f32;
        let base = if reverse {
            phase_offset - phase
        } else {
            phase_offset + phase
        };
        *value = base + weighted_sum / total_weight;
    }
    values[sample_count] = if reverse {
        values[0] - 1.0
    } else {
        values[0] + 1.0
    };
}

fn measure_authored_foot_range(
    definition: &RigDefinition,
    clip: &BakedClip,
    travel_axis: usize,
) -> Option<f32> {
    let feet = ["l_foot", "r_foot"].map(|name| {
        definition
            .joints
            .iter()
            .position(|joint| joint.name.as_deref() == Some(name))
    });
    let [Some(left), Some(right)] = feet else {
        return None;
    };
    let ranges = [left, right].map(|foot| {
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        for frame in 0..clip.frames {
            let mut globals = vec![None; definition.joints.len()];
            let position = sampled_global_transform(
                definition,
                clip,
                foot,
                frame as f32 * clip.frame_dt,
                &mut globals,
            )
            .translation[travel_axis];
            minimum = minimum.min(position);
            maximum = maximum.max(position);
        }
        maximum - minimum
    });
    let stride = (ranges[0] + ranges[1]) * 0.5;
    (stride.is_finite() && (0.05..=3.0).contains(&stride)).then_some(stride)
}

fn sampled_global_transform(
    definition: &RigDefinition,
    clip: &BakedClip,
    joint: usize,
    time_seconds: f32,
    cache: &mut [Option<Transform>],
) -> Transform {
    if let Some(transform) = cache[joint] {
        return transform;
    }
    let pose = clip.sample(joint, time_seconds);
    let local = Transform {
        translation: pose.translation,
        rotation: pose.rotation,
        scale: pose.scale,
    };
    let global = definition.joints[joint].parent.map_or(local, |parent| {
        sampled_global_transform(definition, clip, parent, time_seconds, cache).mul_transform(local)
    });
    cache[joint] = Some(global);
    global
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
    (accumulator.max(0.0) / pose_sample_seconds()).floor() as u32
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

    #[test]
    fn hand_layer_contains_only_hand_bone_subtrees() {
        let joint = |name: &str, parent: Option<usize>| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: LocalPose::from_transform(Transform::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: false,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None),
                joint("r_lowarm", Some(0)),
                joint("r_wrist", Some(1)),
                joint("r_index1", Some(2)),
                joint("l_wrist", Some(0)),
                joint("l_thumb1", Some(4)),
            ],
        };

        assert_eq!(hand_animation_joint(&definition, 0), None);
        assert_eq!(hand_animation_joint(&definition, 1), None);
        assert_eq!(
            hand_animation_joint(&definition, 2),
            Some(ClipLayer::MainHand)
        );
        assert_eq!(
            hand_animation_joint(&definition, 3),
            Some(ClipLayer::MainHand)
        );
        assert_eq!(
            hand_animation_joint(&definition, 4),
            Some(ClipLayer::Offhand)
        );
        assert_eq!(
            hand_animation_joint(&definition, 5),
            Some(ClipLayer::Offhand)
        );
    }

    fn pose(translation: Vec3, rotation: Quat) -> LocalPose {
        LocalPose {
            translation,
            rotation,
            scale: Vec3::ONE,
        }
    }

    #[test]
    fn combat_pelvis_uses_attack_rotation_and_locomotion_translation() {
        let combat = LocalPose {
            translation: Vec3::splat(9.0),
            rotation: Quat::from_rotation_y(0.7),
            scale: Vec3::splat(2.0),
        };
        let locomotion = LocalPose {
            translation: Vec3::new(0.1, 0.2, 0.3),
            rotation: Quat::from_rotation_x(0.4),
            scale: Vec3::splat(1.1),
        };

        let combined = combine_combat_pelvis(combat, locomotion);

        assert_eq!(combined.translation, locomotion.translation);
        assert_eq!(combined.rotation, combat.rotation);
        assert_eq!(combined.scale, locomotion.scale);
    }

    #[test]
    fn pose_extrapolation_extends_translation_and_shortest_rotation() {
        let start = pose(Vec3::ZERO, Quat::IDENTITY);
        let end = pose(Vec3::X, Quat::from_rotation_y(0.5));
        let drawback = start.extrapolate(end, -0.5);
        let follow_through = start.extrapolate(end, 1.5);
        assert!((drawback.translation.x + 0.5).abs() < 1.0e-5);
        assert!((follow_through.translation.x - 1.5).abs() < 1.0e-5);
        assert!(
            drawback
                .rotation
                .angle_between(Quat::from_rotation_y(-0.25))
                < 1.0e-4
        );
        assert!(
            follow_through
                .rotation
                .angle_between(Quat::from_rotation_y(0.75))
                < 1.0e-4
        );
    }

    #[test]
    fn anchor_curve_span_conserves_transform_velocity_through_contact() {
        let start = LocalPose {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        let end = LocalPose {
            translation: Vec3::X,
            rotation: Quat::from_rotation_y(0.2),
            scale: Vec3::ONE,
        };
        let step = 0.001;
        let before = start.extrapolate(end, 1.0 - step);
        let contact = start.extrapolate(end, 1.0);
        let after = start.extrapolate(end, 1.0 + step);
        let incoming_velocity = (contact.translation.x - before.translation.x) / step;
        let outgoing_velocity = (after.translation.x - contact.translation.x) / step;
        assert!((incoming_velocity - outgoing_velocity).abs() < 0.01);
        let incoming_angular_velocity = before.rotation.angle_between(contact.rotation) / step;
        let outgoing_angular_velocity = contact.rotation.angle_between(after.rotation) / step;
        assert!((incoming_angular_velocity - outgoing_angular_velocity).abs() < 0.01);
    }

    #[test]
    fn continuation_span_matches_incoming_and_outgoing_endpoint_velocity() {
        let guard = LocalPose {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        let contact = LocalPose {
            translation: Vec3::X,
            rotation: Quat::from_rotation_y(0.4),
            scale: Vec3::ONE,
        };
        let preparation = LocalPose {
            translation: Vec3::new(1.5, 0.2, 0.0),
            rotation: Quat::from_rotation_y(0.7),
            scale: Vec3::ONE,
        };
        let follow = LocalPose {
            translation: Vec3::new(1.7, 0.5, 0.1),
            rotation: Quat::from_rotation_y(1.0),
            scale: Vec3::ONE,
        };
        let coordinate = 1.2;
        let incoming_tangent = 0.35;
        let outgoing_tangent_scale = 2.0;
        let transition = ContinuationTransition {
            contact,
            ready: preparation,
            outgoing: follow,
            start_coordinate: coordinate,
            incoming_tangent,
            outgoing_tangent_scale,
        };
        let step = 0.0001;
        let before = guard.extrapolate_unbounded(contact, coordinate - incoming_tangent * step);
        let start = guard.continuation_transition(transition, 0.0);
        let after = guard.continuation_transition(transition, step);
        let incoming_translation_velocity = (start.translation - before.translation) / step;
        let outgoing_translation_velocity = (after.translation - start.translation) / step;
        assert!(incoming_translation_velocity.distance(outgoing_translation_velocity) < 0.01);
        let incoming_angular_velocity = before.rotation.angle_between(start.rotation) / step;
        let outgoing_angular_velocity = start.rotation.angle_between(after.rotation) / step;
        assert!((incoming_angular_velocity - outgoing_angular_velocity).abs() < 0.01);

        let before_end = guard.continuation_transition(transition, 1.0 - step);
        let end = guard.continuation_transition(transition, 1.0);
        let after_end = preparation.interpolate(follow, step * outgoing_tangent_scale);
        let incoming_end_velocity = (end.translation - before_end.translation) / step;
        let outgoing_end_velocity = (after_end.translation - end.translation) / step;
        assert!(incoming_end_velocity.distance(outgoing_end_velocity) < 0.01);
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
        let velocity = quaternion_angular_velocity(-rotation, rotation, pose_sample_seconds());
        assert!(velocity.length() < 0.0001);
    }

    #[test]
    fn large_frame_gaps_are_bounded_but_consume_sampler_debt() {
        assert_eq!(samples_due(0.0), 0);
        assert_eq!(samples_due(pose_sample_seconds() * 3.2), 3);
        assert_eq!(samples_due(0.1), 3);
    }

    #[test]
    fn non_finite_samples_fall_back_to_the_bind_pose() {
        let bind = pose(Vec3::Y, Quat::IDENTITY);
        let invalid = pose(Vec3::splat(f32::NAN), Quat::IDENTITY);
        assert_eq!(sanitize_pose(invalid, bind), bind);
    }

    #[test]
    fn authored_foot_range_is_the_average_foot_travel_along_the_motion_axis() {
        let joint = |name: &str, parent: Option<usize>| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(Vec3::ZERO, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None),
                joint("l_foot", Some(0)),
                joint("r_foot", Some(0)),
            ],
        };
        let track = |translations| BoneTrack {
            translations,
            rotations: vec![Quat::IDENTITY; 3],
            scales: vec![Vec3::ONE; 3],
            animated: true,
        };
        let clip = BakedClip {
            duration: 2.0 / 30.0,
            frame_dt: 1.0 / 30.0,
            frames: 3,
            tracks: vec![
                track(vec![Vec3::ZERO; 3]),
                track(vec![Vec3::ZERO, Vec3::Z, Vec3::Z * 2.0]),
                track(vec![Vec3::ZERO, Vec3::Z * -1.5, Vec3::Z * -3.0]),
            ],
        };

        assert_eq!(
            measure_authored_foot_range(&definition, &clip, 2),
            Some(2.5)
        );
    }

    #[test]
    fn sparse_locomotion_sampling_uses_only_semantic_quarter_cycle_anchors() {
        assert_eq!(sparse_locomotion_segment(0.0), (0, 1, 0.0));
        assert_eq!(sparse_locomotion_segment(0.25), (1, 2, 0.0));
        assert_eq!(sparse_locomotion_segment(0.5), (2, 3, 0.0));
        assert_eq!(sparse_locomotion_segment(0.75), (3, 0, 0.0));
        assert_eq!(sparse_locomotion_segment(1.0), (0, 1, 0.0));
        let (_, _, midpoint) = sparse_locomotion_segment(0.125);
        assert!((midpoint - 0.5).abs() < 0.0001);
    }

    #[test]
    fn authored_contact_fit_ignores_airborne_foot_excursions() {
        let joint = |name: &str, parent: Option<usize>| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(Vec3::ZERO, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None),
                joint("l_foot", Some(0)),
                joint("r_foot", Some(0)),
            ],
        };
        let frames = 21;
        let duration = 1.0;
        let frame_dt = duration / (frames - 1) as f32;
        let foot_track = |contact_phase: f32, airborne_offset: f32| BoneTrack {
            translations: (0..frames)
                .map(|frame| {
                    let phase = frame as f32 / (frames - 1) as f32;
                    let stance_phase = phase - contact_phase;
                    let (position, height) = if (0.0..=0.2).contains(&stance_phase) {
                        // A two-metre virtual root displacement toward -Z per
                        // cycle exactly cancels this +Z foot retraction.
                        (2.0 * stance_phase, 0.0)
                    } else {
                        (airborne_offset + phase * phase * 10.0, 0.5)
                    };
                    Vec3::new(0.0, height, position)
                })
                .collect(),
            rotations: vec![Quat::IDENTITY; frames],
            scales: vec![Vec3::ONE; frames],
            animated: true,
        };
        let clip = BakedClip {
            duration,
            frame_dt,
            frames,
            tracks: vec![
                BoneTrack {
                    translations: vec![Vec3::ZERO; frames],
                    rotations: vec![Quat::IDENTITY; frames],
                    scales: vec![Vec3::ONE; frames],
                    animated: true,
                },
                foot_track(0.1, 50.0),
                foot_track(0.6, -80.0),
            ],
        };

        let measurement = measure_authored_contact_step_distance(&definition, &clip, 2, 1.0)
            .expect("the two stance windows should determine travel");
        assert!((measurement.stride.step_distance - 1.0).abs() < 0.0001);
        assert!(measurement.stride.maximum_stance_slip < 0.0001);
        assert!(measurement.phase_curve.is_some());

        let mut projected_rotation = clip.clone();
        for (track_index, contact_phase) in [(1, 0.1_f32), (2, 0.6_f32)] {
            let track = &mut projected_rotation.tracks[track_index];
            for (frame, translation) in track.translations.iter_mut().enumerate() {
                let phase = frame as f32 / (frames - 1) as f32;
                let stance_phase = phase - contact_phase;
                if (-0.0001..=0.2001).contains(&stance_phase) {
                    // A quadratic projection stands in for the sine-like
                    // horizontal motion produced by a rotating leg.
                    translation.y = 0.0;
                    translation.z = 10.0 * stance_phase * stance_phase;
                }
            }
        }
        let projected =
            measure_authored_contact_step_distance(&definition, &projected_rotation, 2, 1.0)
                .expect("a monotone articulated projection should be phase-warpable");
        assert!((projected.stride.step_distance - 1.0).abs() < 0.02);
        assert!(projected.stride.maximum_stance_slip < 0.08);
        let curve = projected
            .phase_curve
            .expect("ordinary gait derives a curve");
        assert!(
            curve.sample(0.2) > 0.22,
            "distance inversion must be nonlinear"
        );

        let mut low_wrong_way_approach = clip.clone();
        for (track_index, indices) in [(1, [0, 1]), (2, [10, 11])] {
            let track = &mut low_wrong_way_approach.tracks[track_index];
            for (offset, index) in indices.into_iter().enumerate() {
                track.translations[index] = Vec3::new(0.0, 0.005, 0.2 - offset as f32 * 0.1);
            }
        }
        let misleading =
            measure_authored_contact_step_distance(&definition, &low_wrong_way_approach, 2, 1.0);
        assert!(
            misleading.is_none_or(|measurement| {
                measurement.stride.maximum_stance_slip
                    > presentation::maximum_authored_stance_slip_metres()
            }),
            "a visibly low wrong-way approach must make the fit unrepresentable"
        );

        let mut wrong_way = clip.clone();
        for track in &mut wrong_way.tracks[1..] {
            for translation in &mut track.translations {
                translation.z = -translation.z;
            }
        }
        let reversed = measure_authored_contact_step_distance(&definition, &wrong_way, 2, 1.0)
            .expect("a cyclic clip authored backward should be sampled in reverse");
        assert!((reversed.stride.step_distance - 1.0).abs() < 0.0001);
        let curve = reversed.phase_curve.unwrap();
        let authored_delta = (curve.sample(0.25) - curve.sample(0.2) + 0.5).rem_euclid(1.0) - 0.5;
        assert!(
            authored_delta < 0.0,
            "reverse playback must decrease authored phase while physical phase advances"
        );
    }

    #[test]
    fn terrain_solve_modifies_next_pose_before_local_pose_interpolation() {
        let joint = |name: &str, parent: Option<usize>, translation: Vec3| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(translation, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None, Vec3::ZERO),
                joint("l_upleg", Some(0), Vec3::Y * 2.0),
                joint("l_lowleg", Some(1), Vec3::NEG_Y),
                joint("l_foot", Some(2), Vec3::NEG_Y),
            ],
        };
        let previous = definition
            .joints
            .iter()
            .map(|joint| joint.bind)
            .collect::<Vec<_>>();
        let mut next = previous.clone();
        let terrain = SceneTerrain::new(2, 2, 1.0, |_| 0.5);
        conform_upcoming_pose_to_terrain(
            &definition,
            &mut next,
            &mut [None; 2],
            PoseConformity {
                owner: &GlobalTransform::IDENTITY,
                weights: Vec2::X,
                terrain: Some(&terrain),
                contact_plants: ContactPlantPolicy::Reset,
                locomotion_pelvis_rotation: None,
            },
        );

        assert_ne!(next[1].rotation, previous[1].rotation);
        let displayed = previous
            .iter()
            .zip(&next)
            .map(|(previous, next)| previous.interpolate(*next, 0.5))
            .collect::<Vec<_>>();
        let next_foot = local_pose_global(
            &definition,
            &next,
            3,
            &mut vec![None; definition.joints.len()],
        )
        .translation;
        let displayed_foot = local_pose_global(
            &definition,
            &displayed,
            3,
            &mut vec![None; definition.joints.len()],
        )
        .translation;
        assert!(displayed_foot.y > 0.0);
        assert!(displayed_foot.y < next_foot.y);
        assert!((next_foot.y - (0.5 + measured_ankle_sole_offset_metres())).abs() < 0.001);
    }

    #[test]
    fn pelvis_rotation_compensation_keeps_an_unweighted_foot_at_locomotion_fk() {
        let joint = |name: &str, parent: Option<usize>, translation: Vec3| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(translation, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None, Vec3::ZERO),
                joint("l_upleg", Some(0), Vec3::new(-0.2, 1.8, 0.0)),
                joint("l_lowleg", Some(1), Vec3::new(0.0, -0.9, 0.15)),
                joint("l_foot", Some(2), Vec3::new(0.0, -0.85, 0.15)),
            ],
        };
        let locomotion = definition
            .joints
            .iter()
            .map(|joint| joint.bind)
            .collect::<Vec<_>>();
        let expected = local_pose_global(
            &definition,
            &locomotion,
            3,
            &mut vec![None; locomotion.len()],
        )
        .translation;
        let mut combined = locomotion;
        combined[0].rotation = Quat::from_rotation_y(0.6);

        conform_upcoming_pose_to_terrain(
            &definition,
            &mut combined,
            &mut [None; 2],
            PoseConformity {
                owner: &GlobalTransform::IDENTITY,
                weights: Vec2::ZERO,
                terrain: None,
                contact_plants: ContactPlantPolicy::Reset,
                locomotion_pelvis_rotation: Some(Quat::IDENTITY),
            },
        );

        let compensated =
            local_pose_global(&definition, &combined, 3, &mut vec![None; combined.len()])
                .translation;
        assert!(compensated.distance(expected) < 0.0002);
    }

    #[test]
    fn presentation_space_removes_the_controller_centre_height() {
        let owner = GlobalTransform::from_translation(Vec3::Y * 0.95);
        let rig_scene = Transform::from_translation(Vec3::NEG_Y * 0.95);

        let presentation = presentation_world_transform(&owner, &rig_scene);

        assert!(presentation.translation().abs().max_element() < 0.0001);
    }

    #[test]
    fn terrain_solve_does_not_lower_an_authored_flat_ground_foot() {
        let joint = |name: &str, parent: Option<usize>, translation: Vec3| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(translation, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None, Vec3::ZERO),
                joint("l_upleg", Some(0), Vec3::Y * 2.1),
                joint("l_lowleg", Some(1), Vec3::NEG_Y),
                joint("l_foot", Some(2), Vec3::NEG_Y),
            ],
        };
        let mut next = definition
            .joints
            .iter()
            .map(|joint| joint.bind)
            .collect::<Vec<_>>();
        let before =
            local_pose_global(&definition, &next, 3, &mut vec![None; next.len()]).translation;
        let terrain = SceneTerrain::new(2, 2, 1.0, |_| 0.0);

        conform_upcoming_pose_to_terrain(
            &definition,
            &mut next,
            &mut [None; 2],
            PoseConformity {
                owner: &GlobalTransform::IDENTITY,
                weights: Vec2::X,
                terrain: Some(&terrain),
                contact_plants: ContactPlantPolicy::Reset,
                locomotion_pelvis_rotation: None,
            },
        );

        let after =
            local_pose_global(&definition, &next, 3, &mut vec![None; next.len()]).translation;
        assert!(before.y > measured_ankle_sole_offset_metres());
        assert!(after.distance(before) < 0.0001);
    }

    #[test]
    fn translating_combat_terrain_conformity_preserves_every_authored_xz_sample() {
        let joint = |name: &str, parent: Option<usize>, translation: Vec3| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(translation, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None, Vec3::ZERO),
                joint("l_upleg", Some(0), Vec3::new(-0.2, 2.085, 0.0)),
                joint("l_lowleg", Some(1), Vec3::NEG_Y),
                joint("l_foot", Some(2), Vec3::new(0.0, -1.003, 0.0)),
                joint("r_upleg", Some(0), Vec3::new(0.2, 2.085, 0.0)),
                joint("r_lowleg", Some(4), Vec3::NEG_Y),
                joint("r_foot", Some(5), Vec3::new(0.0, -1.003, 0.0)),
            ],
        };
        let terrain = SceneTerrain::new(4, 4, 1.0, |_| 0.0);
        let paths = [
            Vec2::new(-0.12, 0.0),
            Vec2::new(0.12, 0.0),
            Vec2::new(0.0, -0.12),
            Vec2::new(0.0, 0.12),
            Vec2::new(-0.09, -0.09),
            Vec2::new(0.09, -0.09),
            Vec2::new(-0.09, 0.09),
            Vec2::new(0.09, 0.09),
        ];

        for (sample_index, displacement) in paths.into_iter().enumerate() {
            let mut authored = definition
                .joints
                .iter()
                .map(|joint| joint.bind)
                .collect::<Vec<_>>();
            authored[3].translation += Vec3::new(displacement.x, 0.0, displacement.y);
            authored[6].translation -= Vec3::new(displacement.x, 0.0, displacement.y);
            let before = [3, 6].map(|foot| {
                local_pose_global(
                    &definition,
                    &authored,
                    foot,
                    &mut vec![None; authored.len()],
                )
                .translation
            });
            let mut conformed = authored;
            conform_upcoming_pose_to_terrain(
                &definition,
                &mut conformed,
                &mut [None; 2],
                PoseConformity {
                    owner: &GlobalTransform::IDENTITY,
                    weights: if sample_index % 2 == 0 {
                        Vec2::X
                    } else {
                        Vec2::Y
                    },
                    terrain: Some(&terrain),
                    contact_plants: ContactPlantPolicy::Reset,
                    locomotion_pelvis_rotation: None,
                },
            );
            let after = [3, 6].map(|foot| {
                local_pose_global(
                    &definition,
                    &conformed,
                    foot,
                    &mut vec![None; conformed.len()],
                )
                .translation
            });
            for foot in 0..2 {
                let xz_delta = after[foot].xz().distance(before[foot].xz());
                assert!(
                    xz_delta < 0.0005,
                    "sample {sample_index} foot {foot} changed XZ by {xz_delta:.6}m"
                );
            }
        }
    }

    #[test]
    fn combat_contact_retains_world_pose_until_the_plant_limit() {
        let joint = |name: &str, parent: Option<usize>, translation: Vec3| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(translation, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
        };
        let definition = RigDefinition {
            family: "test".to_owned(),
            joints: vec![
                joint("root", None, Vec3::ZERO),
                joint("l_upleg", Some(0), Vec3::new(0.2, 2.1, 0.0)),
                joint("l_lowleg", Some(1), Vec3::NEG_Y),
                joint("l_foot", Some(2), Vec3::NEG_Y),
            ],
        };
        let authored = definition
            .joints
            .iter()
            .map(|joint| joint.bind)
            .collect::<Vec<_>>();
        let terrain = SceneTerrain::new(2, 2, 1.0, |_| 0.0);
        let mut plants = [None; 2];
        let mut first = authored.clone();
        conform_upcoming_pose_to_terrain(
            &definition,
            &mut first,
            &mut plants,
            PoseConformity {
                owner: &GlobalTransform::IDENTITY,
                weights: Vec2::X,
                terrain: Some(&terrain),
                contact_plants: ContactPlantPolicy::Retain,
                locomotion_pelvis_rotation: None,
            },
        );
        let acquired = plants[0].expect("supported foot should acquire a plant");

        let modest_owner =
            GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(0.35)));
        let mut modest = authored.clone();
        conform_upcoming_pose_to_terrain(
            &definition,
            &mut modest,
            &mut plants,
            PoseConformity {
                owner: &modest_owner,
                weights: Vec2::new(0.25, 0.0),
                terrain: Some(&terrain),
                contact_plants: ContactPlantPolicy::Retain,
                locomotion_pelvis_rotation: None,
            },
        );
        let retained = plants[0].expect("plant should remain owned");
        assert!(retained.position_world.distance(acquired.position_world) < 0.0001);
        assert!(
            retained
                .rotation_world
                .angle_between(acquired.rotation_world)
                < 0.0001
        );
        let modest_foot = local_pose_global(
            &definition,
            &modest,
            3,
            &mut vec![None; definition.joints.len()],
        );
        let retained_distance = modest_owner
            .transform_point(modest_foot.translation)
            .distance(acquired.position_world);
        assert!(
            retained_distance < 0.002,
            "retained distance {retained_distance}"
        );
        assert!(
            (modest_owner.rotation() * modest_foot.rotation).angle_between(acquired.rotation_world)
                < 0.001
        );

        let far_owner = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(1.5)));
        let mut far = authored.clone();
        conform_upcoming_pose_to_terrain(
            &definition,
            &mut far,
            &mut plants,
            PoseConformity {
                owner: &far_owner,
                weights: Vec2::X,
                terrain: Some(&terrain),
                contact_plants: ContactPlantPolicy::Retain,
                locomotion_pelvis_rotation: None,
            },
        );
        let slid = plants[0].expect("plant should slide rather than release");
        let authored_foot = local_pose_global(
            &definition,
            &authored,
            3,
            &mut vec![None; definition.joints.len()],
        );
        let far_authored_world = far_owner.transform_point(authored_foot.translation);
        assert!(slid.position_world.distance(acquired.position_world) > 0.001);
        assert!(
            slid.position_world.xz().distance(far_authored_world.xz())
                <= pose_tuning().authored_contact_plant_limit_metres + 0.0001
        );
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
        let mut offset = JointInertialOffset {
            translation: Vec3::splat(100.0),
            rotation: Quat::from_rotation_z(2.8),
            ..default()
        };
        let result = offset.update(pose(Vec3::ZERO, Quat::IDENTITY), 2.0);
        assert!(result.translation.is_finite());
        assert!(result.rotation.is_finite());
        assert!(result.translation.length() < 0.01);
        assert!(result.rotation.angle_between(Quat::IDENTITY) < 0.01);
    }
}
