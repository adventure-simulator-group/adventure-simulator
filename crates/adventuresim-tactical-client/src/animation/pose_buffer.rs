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
use crate::presentation::TacticalGameplayCamera;

const SAMPLE_HZ: f32 = 30.0;
const SAMPLE_DT: f32 = 1.0 / SAMPLE_HZ;
const INERTIAL_HALFLIFE_SECONDS: f32 = 0.10;
const CULL_DISTANCE_METRES: f32 = 100.0;
const CULL_RADIUS_METRES: f32 = 2.0;
const AUTHORED_CONTACT_PLANT_LIMIT_METRES: f32 = 0.14;

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
    strips_root_translation: bool,
}

fn strips_gameplay_root_translation(name: &str) -> bool {
    // MetaHuman rigs name the skeleton-space root `body_world`; the joint
    // named `root` is the anatomical pelvis. Pelvis translation is authored
    // pose data (not gameplay root motion) and must survive clip baking.
    name.eq_ignore_ascii_case("body_world")
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

    fn extrapolate(self, next: Self, coordinate: f32) -> Self {
        let coordinate =
            coordinate.clamp(-AttackCurve::MAX_DRAWBACK, 1.0 + AttackCurve::MAX_OVERSHOOT);
        let relative = shortest_rotation(next.rotation * self.rotation.inverse());
        Self {
            translation: self.translation + (next.translation - self.translation) * coordinate,
            rotation: (quaternion_exp(quaternion_log(relative) * coordinate) * self.rotation)
                .normalize(),
            scale: self.scale + (next.scale - self.scale) * coordinate,
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
        clips.extend(playback.extrapolated_spans.iter().flat_map(|span| {
            [
                (span.start.handle.id(), span.start.layer),
                (span.end.handle.id(), span.end.layer),
            ]
        }));
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
                                strips_root_translation: name
                                    .as_deref()
                                    .is_some_and(strips_gameplay_root_translation),
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
        let Some(mut target) =
            sample_plan(playback, &rig.definition, &clips, &mut bank, &mut metrics)
        else {
            continue;
        };
        if terrain_ik_enabled.0
            && procedural::authored_locomotion_ik_owns(skeleton)
            && let Ok(terrain) = terrain.single()
        {
            let definition = Arc::clone(&rig.definition);
            conform_upcoming_pose_to_terrain(
                &definition,
                &mut target,
                owner_transform,
                playback.foot_ik_weights,
                terrain,
                &mut rig.terrain_plants,
                false,
            );
            // Translating authored cycles own their complete XZ trajectories.
            // Stationary guard turning uses the separate procedural pole-limit
            // plant path; never carry a world-space plant through this terrain
            // conformity pass.
        } else {
            rig.terrain_plants = [None; 2];
        }
        if transition {
            let capture_displayed = rig.active;
            for (joint, target_pose) in target.iter().copied().enumerate() {
                let buffered_displayed = rig.displayed_pose(joint);
                let displayed = if capture_displayed {
                    // Final procedural passes run after the pose buffer and can
                    // move a joint away from its cached input. A transition
                    // must begin from the transform that was actually rendered
                    // on the preceding frame, otherwise authored locomotion can
                    // snap back to the hidden pre-IK stance before inertializing.
                    rig.entities[joint]
                        .and_then(|entity| transforms.get(entity).ok())
                        .map(|transform| LocalPose::from_transform(*transform))
                        .unwrap_or(buffered_displayed)
                } else {
                    target_pose
                };
                let captured_final_modifier = capture_displayed
                    && (displayed
                        .translation
                        .distance(buffered_displayed.translation)
                        > 0.0001
                        || displayed
                            .rotation
                            .angle_between(buffered_displayed.rotation)
                            > 0.001);
                let (linear_velocity, angular_velocity) =
                    if capture_displayed && !captured_final_modifier {
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
            let due = samples_due(rig.sample_accumulator);
            if due > 0 {
                rig.previous = rig.next.clone();
                rig.next = target;
                rig.sample_accumulator = (rig.sample_accumulator - SAMPLE_DT * due as f32).max(0.0);
            }
            // Terrain IK has already modified `next`. Interpolate local joint
            // transforms from the preceding solved sample to that upcoming
            // solved sample; no post-interpolation contact toggle is needed.
            rig.interpolation_alpha = (rig.sample_accumulator / SAMPLE_DT).clamp(0.0, 1.0);
        }
    }
}

pub(super) fn apply_pose_buffers(
    mut rigs: Query<&mut PoseBufferRig>,
    mut transforms: Query<&mut Transform, Without<PoseBufferRig>>,
) {
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
    if playback.use_authored_bind_pose
        || (playback.clips.is_empty() && playback.extrapolated_spans.is_empty())
    {
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
        for (span, start_clip, end_clip) in &baked_spans {
            let included = match span.start.layer {
                ClipLayer::Whole => true,
                ClipLayer::Upper => !joint.lower_body,
                ClipLayer::Lower => joint.lower_body,
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
            let sample = sanitize_pose(start.extrapolate(end, span.coordinate), joint.bind);
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
        pose.push(if accumulated > f32::EPSILON {
            blended
        } else {
            joint.bind
        });
    }
    metrics.sampled_pose_count = metrics.sampled_pose_count.saturating_add(1);
    Some(pose)
}

/// Solve the terrain-adjusted upcoming sample in pose-buffer space. The
/// renderer subsequently interpolates `previous -> next`, so terrain
/// conformity follows exactly the same continuous local-pose interpolation as
/// authored FK instead of being switched onto the already displayed frame.
fn conform_upcoming_pose_to_terrain(
    definition: &RigDefinition,
    pose: &mut [LocalPose],
    owner: &GlobalTransform,
    weights: Vec2,
    terrain: &SceneTerrain,
    plants: &mut [Option<AuthoredContactPlant>; 2],
    retain_combat_plants: bool,
) {
    if !retain_combat_plants {
        *plants = [None; 2];
    }
    for (index, (left, weight, names)) in [
        (true, weights.x, ["l_upleg", "l_lowleg", "l_foot"]),
        (false, weights.y, ["r_upleg", "r_lowleg", "r_foot"]),
    ]
    .into_iter()
    .enumerate()
    {
        let weight = weight.clamp(0.0, 1.0);
        if weight <= f32::EPSILON {
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
        let foot_rotation_world = owner.rotation() * foot_global.rotation;
        let Some(height) = terrain.height_at(foot_world.xz()) else {
            continue;
        };
        let terrain_ankle_y = height + MEASURED_ANKLE_SOLE_OFFSET_METRES;
        let authored_target_world = foot_world.with_y(foot_world.y.max(terrain_ankle_y));
        let supported = retain_combat_plants && weight > 0.05;
        if !supported {
            plants[index] = None;
        } else if plants[index].is_none() {
            plants[index] = Some(AuthoredContactPlant {
                position_world: authored_target_world,
                rotation_world: foot_rotation_world,
                reference_owner_position: owner
                    .affine()
                    .inverse()
                    .transform_point3(authored_target_world),
                reference_owner_rotation: owner.rotation().inverse() * foot_rotation_world,
            });
        }
        if let Some(mut plant) = plants[index] {
            let reference_world = owner.transform_point(plant.reference_owner_position);
            let reference_rotation_world = owner.rotation() * plant.reference_owner_rotation;
            let displacement = reference_world.xz() - plant.position_world.xz();
            let distance = displacement.length();
            if distance > AUTHORED_CONTACT_PLANT_LIMIT_METRES {
                let excess = distance - AUTHORED_CONTACT_PLANT_LIMIT_METRES;
                plant.position_world +=
                    Vec3::new(displacement.x, 0.0, displacement.y).normalize_or_zero() * excess;
                plant.rotation_world = hemisphere_slerp(
                    plant.rotation_world,
                    reference_rotation_world,
                    (excess / distance).clamp(0.0, 1.0),
                );
            }
            if let Some(plant_height) = terrain.height_at(plant.position_world.xz()) {
                plant.position_world.y = plant
                    .position_world
                    .y
                    .max(plant_height + MEASURED_ANKLE_SOLE_OFFSET_METRES);
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
            (authored_target_world, foot_global.rotation)
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
            foot_world.lerp(terrain_world, weight)
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

pub(super) fn calibrate_authored_locomotion_strides(
    definitions: Res<RigDefinitions>,
    runtime: Res<AnimationRuntime>,
    clips: Res<Assets<AnimationClip>>,
    mut strides: ResMut<AuthoredLocomotionStrides>,
) {
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
        let Some(clip) = clips.get(&loaded.handle) else {
            continue;
        };
        let baked = bake_clip(clip, definition);
        let stride = match motion {
            "walk" => measure_authored_contact_step_distance(
                definition,
                &baked,
                axis,
                WALK_LOCOMOTION_PROFILE.support_phase_radius,
            ),
            "run" => measure_authored_contact_step_distance(
                definition,
                &baked,
                axis,
                RUN_LOCOMOTION_PROFILE.support_phase_radius,
            ),
            // Combat cycles currently expose alternating contact poses but no
            // typed support interval. Retain their geometric calibration until
            // that contact timing is part of the authored motion contract.
            "strafe" | "skip" => measure_authored_foot_range(definition, &baked, axis),
            _ => unreachable!("fixed authored locomotion calibration table"),
        };
        let Some(stride) = stride else {
            warn!(motion, "Could not infer authored locomotion stride");
            strides.measured_clips.insert(motion.to_owned(), id);
            continue;
        };
        match motion {
            "walk" => strides.walk = Some(stride),
            "run" => strides.run = Some(stride),
            "strafe" => strides.strafe = Some(stride),
            "skip" => strides.skip = Some(stride),
            _ => unreachable!("fixed authored locomotion calibration table"),
        }
        strides.measured_clips.insert(motion.to_owned(), id);
        info!(
            motion,
            stride_metres = stride,
            "Measured authored locomotion stride"
        );
    }
}

/// Infer travel by fitting the virtual root motion that makes each stance foot
/// stationary between initial contact and support release. Normalized phase
/// retains the authored flight time, so the fitted travel can exceed the
/// contact-pose separation without consulting unconstrained swing-foot motion.
fn measure_authored_contact_step_distance(
    definition: &RigDefinition,
    clip: &BakedClip,
    travel_axis: usize,
    support_phase: f32,
) -> Option<f32> {
    if clip.duration <= f32::EPSILON || support_phase <= f32::EPSILON {
        return None;
    }
    let feet = [("l_foot", 0.0_f32), ("r_foot", 0.5_f32)];
    let mut covariance = 0.0;
    let mut phase_variance = 0.0;
    for (name, contact_phase) in feet {
        let foot = definition
            .joints
            .iter()
            .position(|joint| joint.name.as_deref() == Some(name))?;
        let samples = (0..clip.frames)
            .filter_map(|frame| {
                let time = (frame as f32 * clip.frame_dt).min(clip.duration);
                let phase = time / clip.duration;
                let stance_phase = phase - contact_phase;
                if !(0.0..=support_phase).contains(&stance_phase) {
                    return None;
                }
                let mut globals = vec![None; definition.joints.len()];
                let position = sampled_global_transform(definition, clip, foot, time, &mut globals)
                    .translation[travel_axis];
                position.is_finite().then_some((stance_phase, position))
            })
            .collect::<Vec<_>>();
        if samples.len() < 3 {
            return None;
        }
        let count = samples.len() as f32;
        let mean_phase = samples.iter().map(|sample| sample.0).sum::<f32>() / count;
        let mean_position = samples.iter().map(|sample| sample.1).sum::<f32>() / count;
        for (phase, position) in samples {
            let centered_phase = phase - mean_phase;
            covariance += centered_phase * (position - mean_position);
            phase_variance += centered_phase * centered_phase;
        }
    }
    if phase_variance <= f32::EPSILON {
        return None;
    }
    let cycle_distance = (covariance / phase_variance).abs();
    let step_distance = cycle_distance * 0.5;
    (step_distance.is_finite() && (0.05..=3.0).contains(&step_distance)).then_some(step_distance)
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
    fn only_the_true_skeleton_root_strips_authored_translation() {
        assert!(strips_gameplay_root_translation("body_world"));
        assert!(strips_gameplay_root_translation("BODY_WORLD"));
        assert!(!strips_gameplay_root_translation("root"));
        assert!(!strips_gameplay_root_translation("c_spine0"));
    }

    #[test]
    fn authored_foot_range_is_the_average_foot_travel_along_the_motion_axis() {
        let joint = |name: &str, parent: Option<usize>| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(Vec3::ZERO, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
            strips_root_translation: false,
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
    fn authored_contact_fit_ignores_airborne_foot_excursions() {
        let joint = |name: &str, parent: Option<usize>| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(Vec3::ZERO, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
            strips_root_translation: false,
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
                    let position = if (0.0..=0.2).contains(&stance_phase) {
                        // A two-metre virtual root displacement per cycle
                        // exactly cancels this planted-foot trajectory.
                        -2.0 * stance_phase
                    } else {
                        airborne_offset + phase * phase * 10.0
                    };
                    Vec3::Z * position
                })
                .collect(),
            rotations: vec![Quat::IDENTITY; frames],
            scales: vec![Vec3::ONE; frames],
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
                },
                foot_track(0.0, 50.0),
                foot_track(0.5, -80.0),
            ],
        };

        let step = measure_authored_contact_step_distance(&definition, &clip, 2, 0.2)
            .expect("the two stance windows should determine travel");
        assert!((step - 1.0).abs() < 0.0001);
    }

    #[test]
    fn terrain_solve_modifies_next_pose_before_local_pose_interpolation() {
        let joint = |name: &str, parent: Option<usize>, translation: Vec3| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(translation, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
            strips_root_translation: false,
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
            &GlobalTransform::IDENTITY,
            Vec2::X,
            &terrain,
            &mut [None; 2],
            false,
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
        assert!((next_foot.y - (0.5 + MEASURED_ANKLE_SOLE_OFFSET_METRES)).abs() < 0.001);
    }

    #[test]
    fn terrain_solve_does_not_lower_an_authored_flat_ground_foot() {
        let joint = |name: &str, parent: Option<usize>, translation: Vec3| RigJoint {
            target: AnimationTargetId::from_name(&Name::new(name.to_owned())),
            bind: pose(translation, Quat::IDENTITY),
            parent,
            name: Some(name.to_owned()),
            lower_body: true,
            strips_root_translation: false,
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
            &GlobalTransform::IDENTITY,
            Vec2::X,
            &terrain,
            &mut [None; 2],
            false,
        );

        let after =
            local_pose_global(&definition, &next, 3, &mut vec![None; next.len()]).translation;
        assert!(before.y > MEASURED_ANKLE_SOLE_OFFSET_METRES);
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
            strips_root_translation: false,
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
                &GlobalTransform::IDENTITY,
                if sample_index % 2 == 0 {
                    Vec2::X
                } else {
                    Vec2::Y
                },
                &terrain,
                &mut [None; 2],
                false,
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
            strips_root_translation: false,
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
            &GlobalTransform::IDENTITY,
            Vec2::X,
            &terrain,
            &mut plants,
            true,
        );
        let acquired = plants[0].expect("supported foot should acquire a plant");

        let modest_owner =
            GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(0.35)));
        let mut modest = authored.clone();
        conform_upcoming_pose_to_terrain(
            &definition,
            &mut modest,
            &modest_owner,
            Vec2::new(0.25, 0.0),
            &terrain,
            &mut plants,
            true,
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
            &far_owner,
            Vec2::X,
            &terrain,
            &mut plants,
            true,
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
                <= AUTHORED_CONTACT_PLANT_LIMIT_METRES + 0.0001
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
