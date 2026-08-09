//! Client-presentational humanoid ragdolls for the focused native viewer.
//!
//! This module intentionally does not attach physics bodies to the replicated
//! player root or gameplay collider. The solver-owned bodies are transient
//! presentation state and only write back to rendered rig bones.

use std::collections::{BTreeMap, HashMap};

use avian3d::prelude::{
    AngularVelocity, JointFrame, LinearVelocity, PhysicsSystems, Position,
    RevoluteJoint as AvianRevoluteJoint, RigidBody, Rotation,
    SphericalJoint as AvianSphericalJoint,
};
use bevy::{app::AnimationSystems, math::Isometry3d, prelude::*};
use bevy_animation_graph::core::ragdoll::relative_kinematic_body::{
    RelativeKinematicBody, RelativeKinematicBodyPositionBased,
};
use bevy_animation_graph::core::ragdoll::{
    active_motor_avian::apply_active_motor,
    definition::{
        ActiveRevoluteMotor, Body, BodyId, BodyMode, Collider, ColliderMassMode, ColliderShape,
        Joint, JointVariant, Ragdoll, RevoluteJoint, SphericalJoint,
    },
    spawning::spawn_ragdoll_avian,
};

use super::procedural::{self, BoneRole, HumanoidRig};

pub(crate) const RAGDOLL_LAYER: u32 = 1 << 1;
pub(crate) const TERRAIN_LAYER: u32 = 1;

const BODY_SPECS: [(BoneRole, f32, f32); 15] = [
    (BoneRole::Pelvis, 0.18, 0.24),
    (BoneRole::Chest, 0.18, 0.28),
    (BoneRole::Head, 0.15, 0.16),
    (BoneRole::ThighLeft, 0.10, 0.36),
    (BoneRole::ShinLeft, 0.085, 0.34),
    (BoneRole::FootLeft, 0.09, 0.20),
    (BoneRole::ThighRight, 0.10, 0.36),
    (BoneRole::ShinRight, 0.085, 0.34),
    (BoneRole::FootRight, 0.09, 0.20),
    (BoneRole::UpperArmLeft, 0.075, 0.27),
    (BoneRole::ForearmLeft, 0.065, 0.25),
    (BoneRole::HandLeft, 0.07, 0.14),
    (BoneRole::UpperArmRight, 0.075, 0.27),
    (BoneRole::ForearmRight, 0.065, 0.25),
    (BoneRole::HandRight, 0.07, 0.14),
];

const SPHERICAL_LINKS: [(BoneRole, BoneRole); 6] = [
    (BoneRole::Pelvis, BoneRole::Chest),
    (BoneRole::Chest, BoneRole::Head),
    (BoneRole::Pelvis, BoneRole::ThighLeft),
    (BoneRole::Pelvis, BoneRole::ThighRight),
    (BoneRole::Chest, BoneRole::UpperArmLeft),
    (BoneRole::Chest, BoneRole::UpperArmRight),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HingeKind {
    Knee,
    Elbow,
    PassiveAnkle,
}

const HINGE_LINKS: [(BoneRole, BoneRole, HingeKind); 6] = [
    (BoneRole::ThighLeft, BoneRole::ShinLeft, HingeKind::Knee),
    (
        BoneRole::ShinLeft,
        BoneRole::FootLeft,
        HingeKind::PassiveAnkle,
    ),
    (BoneRole::ThighRight, BoneRole::ShinRight, HingeKind::Knee),
    (
        BoneRole::ShinRight,
        BoneRole::FootRight,
        HingeKind::PassiveAnkle,
    ),
    (
        BoneRole::UpperArmLeft,
        BoneRole::ForearmLeft,
        HingeKind::Elbow,
    ),
    (
        BoneRole::UpperArmRight,
        BoneRole::ForearmRight,
        HingeKind::Elbow,
    ),
];

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RagdollMode {
    #[default]
    Animated,
    Active,
    Passive,
}

impl RagdollMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Animated => "animated",
            Self::Active => "active motors",
            Self::Passive => "passive ragdoll",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Animated => Self::Active,
            Self::Active => Self::Passive,
            Self::Passive => Self::Animated,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct RagdollReset(pub(crate) bool);

#[derive(Resource, Debug, Clone)]
pub(crate) struct ActiveMotorProfile {
    pub(crate) strength: f32,
    pub(crate) target_knee_radians: f32,
    pub(crate) target_elbow_radians: f32,
    pub(crate) max_torque: f32,
}

impl Default for ActiveMotorProfile {
    fn default() -> Self {
        Self {
            strength: 1.0,
            target_knee_radians: 0.35,
            target_elbow_radians: 0.25,
            max_torque: 140.0,
        }
    }
}

#[derive(Resource, Debug, Clone, Default, serde::Serialize)]
pub(crate) struct RagdollMotorTelemetry {
    pub(crate) samples: Vec<MotorTelemetrySample>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct MotorTelemetrySample {
    pub(crate) tick: u64,
    pub(crate) strength: f32,
    pub(crate) driven_hinges: usize,
    pub(crate) mean_error_radians: f32,
    pub(crate) maximum_error_radians: f32,
    pub(crate) pelvis_speed: f32,
    pub(crate) finite: bool,
}

#[derive(Resource, Default)]
struct ActiveMotorBlend {
    strength: f32,
    tick: u64,
}

#[derive(Component)]
struct RagdollOwnedBone;

/// Client-only camera/presentation focus following the solved pelvis. It does
/// not affect the replicated player root, controller, or gameplay hitboxes.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RagdollPresentationFocus {
    pub(crate) position: Vec3,
    pub(crate) linear_velocity: Vec3,
}

/// Marks bodies owned by this bridge rather than BAG's AnimatedScene target
/// writer. Such bodies must not retain either relative-kinematic component.
#[derive(Component)]
struct ManuallyOwnedRagdollBody;

#[derive(Component)]
pub(crate) struct RagdollPresentationReady;

#[derive(Debug, Clone, Copy)]
struct BodyBinding {
    bone: Entity,
    body: Entity,
}

#[derive(Debug, Clone, Copy)]
struct JointFrameBinding {
    joint: Entity,
    body1: Entity,
    body2: Entity,
    pivot_world: Vec3,
    basis_world: Quat,
}

#[derive(Debug, Clone, Copy)]
struct HingeBinding {
    joint: Entity,
    body1: Entity,
    body2: Entity,
    kind: HingeKind,
    axis_sign: f32,
}

#[derive(Component)]
struct JointFramesConfigured;

#[derive(Component)]
struct RagdollRootOwner(Entity);

#[derive(Component, Default)]
struct CapturedRagdollPose(HashMap<Entity, Transform>);

#[derive(Component)]
struct HumanoidRagdoll {
    root: Entity,
    bindings: Vec<BodyBinding>,
    joint_frames: Vec<JointFrameBinding>,
    hinges: Vec<HingeBinding>,
    focus_body: Option<Entity>,
}

pub(crate) struct HumanoidRagdollPlugin;

impl Plugin for HumanoidRagdollPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RagdollMode>()
            .init_resource::<RagdollReset>()
            .init_resource::<ActiveMotorProfile>()
            .init_resource::<ActiveMotorBlend>()
            .init_resource::<RagdollMotorTelemetry>()
            .add_systems(
                Update,
                (
                    spawn_missing_humanoid_ragdolls.after(procedural::cache_humanoid_rigs),
                    cleanup_orphaned_ragdolls,
                ),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    configure_joint_frames,
                    drive_or_release_bodies,
                    apply_active_motors,
                )
                    .chain()
                    .before(PhysicsSystems::First),
            )
            .add_systems(
                FixedPostUpdate,
                (sample_motor_telemetry, capture_solved_body_poses)
                    .chain()
                    .after(PhysicsSystems::Last),
            )
            .add_systems(
                PostUpdate,
                read_passive_pose_to_rendered_bones
                    .after(AnimationSystems)
                    .after(bevy_animation_graph::core::plugin::AnimationGraphSet::Final)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn complete_topology(rig: &HumanoidRig) -> bool {
    BODY_SPECS
        .iter()
        .all(|(role, _, _)| rig.get(role).is_some())
}

fn spawn_missing_humanoid_ragdolls(
    mut commands: Commands,
    rigs: Query<(Entity, &HumanoidRig), Without<HumanoidRagdoll>>,
    globals: Query<&GlobalTransform>,
) {
    for (owner, rig) in &rigs {
        if !complete_topology(rig) {
            continue;
        }
        let Some((definition, body_roles, joint_roles)) = build_definition(rig, &globals) else {
            continue;
        };
        let spawned = spawn_ragdoll_avian(
            owner,
            &definition,
            Isometry3d::IDENTITY,
            None,
            &mut commands,
        );
        let mut bindings = Vec::with_capacity(body_roles.len());
        let mut focus_body = None;
        let mut focus_position = None;
        for (body_id, role, bone, global) in body_roles {
            let Some(body) = spawned.bodies.get(&body_id).copied() else {
                continue;
            };
            commands
                .entity(body)
                .insert((
                    ManuallyOwnedRagdollBody,
                    Transform::from_translation(global.translation())
                        .with_rotation(global.rotation()),
                    Position(global.translation()),
                    Rotation(global.rotation()),
                    LinearVelocity::ZERO,
                    AngularVelocity::ZERO,
                ))
                // `spawn_ragdoll_avian` inserts the position-based target and
                // its required velocity target. These queued removals follow
                // that spawn command, preventing BAG's update systems from
                // driving manually simulated dynamic bodies toward identity.
                .remove::<RelativeKinematicBodyPositionBased>()
                .remove::<RelativeKinematicBody>();
            bindings.push(BodyBinding { bone, body });
            if role == BoneRole::Pelvis {
                focus_body = Some(body);
                focus_position = Some(global.translation());
            }
        }
        let joint_frames = joint_roles
            .iter()
            .filter_map(|binding| {
                Some(JointFrameBinding {
                    joint: *spawned.joints.get(&binding.joint)?,
                    body1: *spawned.bodies.get(&binding.body1)?,
                    body2: *spawned.bodies.get(&binding.body2)?,
                    pivot_world: binding.pivot_world,
                    basis_world: binding.basis_world,
                })
            })
            .collect();
        let hinges = joint_roles
            .iter()
            .filter_map(|binding| {
                Some(HingeBinding {
                    joint: *spawned.joints.get(&binding.joint)?,
                    body1: *spawned.bodies.get(&binding.body1)?,
                    body2: *spawned.bodies.get(&binding.body2)?,
                    kind: binding.hinge_kind?,
                    axis_sign: 1.0,
                })
            })
            .collect();
        commands
            .entity(spawned.root)
            .insert(RagdollRootOwner(owner));
        commands.entity(owner).insert((
            HumanoidRagdoll {
                root: spawned.root,
                bindings,
                joint_frames,
                hinges,
                focus_body,
            },
            CapturedRagdollPose::default(),
            RagdollPresentationFocus {
                position: focus_position.unwrap_or_default(),
                linear_velocity: Vec3::ZERO,
            },
            RagdollPresentationReady,
        ));
    }
}

type DefinitionBinding = (BodyId, BoneRole, Entity, GlobalTransform);

struct JointDefinitionBinding {
    joint: bevy_animation_graph::core::ragdoll::definition::JointId,
    body1: BodyId,
    body2: BodyId,
    pivot_world: Vec3,
    basis_world: Quat,
    hinge_kind: Option<HingeKind>,
}

fn build_definition(
    rig: &HumanoidRig,
    globals: &Query<&GlobalTransform>,
) -> Option<(Ragdoll, Vec<DefinitionBinding>, Vec<JointDefinitionBinding>)> {
    let mut ragdoll = Ragdoll {
        total_mass: 72.0,
        ..default()
    };
    let mut ids = BTreeMap::new();
    let mut bindings = Vec::with_capacity(BODY_SPECS.len());
    for (role, radius, length) in BODY_SPECS {
        let bone = *rig.get(&role)?;
        let global = *globals.get(bone).ok()?;
        let mut collider = Collider::new();
        collider.label = format!("{role:?} collider");
        collider.shape = ColliderShape::Capsule(Capsule3d::new(radius, length));
        collider.layer_membership = RAGDOLL_LAYER;
        collider.layer_filter = TERRAIN_LAYER;
        collider.override_layers = true;
        collider.mass_mode = ColliderMassMode::ByVolume;
        let mut body = Body::new();
        body.label = format!("{role:?}");
        body.offset = global.translation();
        body.colliders.push(collider.id);
        body.default_mode = BodyMode::Kinematic;
        ids.insert(role, body.id);
        bindings.push((body.id, role, bone, global));
        ragdoll.add_collider(collider);
        ragdoll.add_body(body);
    }
    let mut joint_bindings = Vec::new();
    for (parent, child) in SPHERICAL_LINKS {
        let child_global = globals.get(*rig.get(&child)?).ok()?;
        let position = child_global.translation();
        let mut joint = Joint::new();
        joint.label = format!("{parent:?} -> {child:?}");
        joint.variant = JointVariant::Spherical(SphericalJoint {
            body1: ids[&parent],
            body2: ids[&child],
            position,
            twist_axis: Vec3::Y,
            swing_limit: Some(
                bevy_animation_graph::core::ragdoll::definition::AngleLimit {
                    min: -0.8,
                    max: 0.8,
                },
            ),
            twist_limit: Some(
                bevy_animation_graph::core::ragdoll::definition::AngleLimit {
                    min: -0.6,
                    max: 0.6,
                },
            ),
            ..default()
        });
        joint_bindings.push(JointDefinitionBinding {
            joint: joint.id,
            body1: ids[&parent],
            body2: ids[&child],
            pivot_world: position,
            basis_world: child_global.rotation(),
            hinge_kind: None,
        });
        ragdoll.add_joint(joint);
    }
    for (parent, child, kind) in HINGE_LINKS {
        let child_global = globals.get(*rig.get(&child)?).ok()?;
        let position = child_global.translation();
        let mut joint = Joint::new();
        joint.label = format!("{parent:?} -> {child:?}");
        joint.variant = JointVariant::Revolute(RevoluteJoint {
            body1: ids[&parent],
            body2: ids[&child],
            position,
            hinge_axis: Vec3::X,
            angle_limit: Some(
                bevy_animation_graph::core::ragdoll::definition::AngleLimit {
                    min: -0.15,
                    max: 2.5,
                },
            ),
            ..default()
        });
        joint_bindings.push(JointDefinitionBinding {
            joint: joint.id,
            body1: ids[&parent],
            body2: ids[&child],
            pivot_world: position,
            basis_world: child_global.rotation(),
            hinge_kind: Some(kind),
        });
        ragdoll.add_joint(joint);
    }
    Some((ragdoll, bindings, joint_bindings))
}

fn local_joint_frame(body: Transform, pivot_world: Vec3, basis_world: Quat) -> JointFrame {
    let local_anchor = body.rotation.inverse() * (pivot_world - body.translation);
    let local_basis = body.rotation.inverse() * basis_world;
    JointFrame::local(Isometry3d::new(local_anchor, local_basis))
}

fn configure_joint_frames(
    mut commands: Commands,
    ragdolls: Query<&HumanoidRagdoll>,
    configured: Query<(), With<JointFramesConfigured>>,
    body_poses: Query<(&Position, &Rotation)>,
    mut joints: ParamSet<(
        Query<&mut AvianSphericalJoint>,
        Query<&mut AvianRevoluteJoint>,
    )>,
) {
    for ragdoll in &ragdolls {
        for binding in &ragdoll.joint_frames {
            if configured.contains(binding.joint) {
                continue;
            }
            let (Ok((position1, rotation1)), Ok((position2, rotation2))) =
                (body_poses.get(binding.body1), body_poses.get(binding.body2))
            else {
                continue;
            };
            let frame1 = local_joint_frame(
                Transform::from_translation(position1.0).with_rotation(rotation1.0),
                binding.pivot_world,
                binding.basis_world,
            );
            let frame2 = local_joint_frame(
                Transform::from_translation(position2.0).with_rotation(rotation2.0),
                binding.pivot_world,
                binding.basis_world,
            );
            let mut applied = false;
            if let Ok(mut joint) = joints.p0().get_mut(binding.joint) {
                joint.frame1 = frame1;
                joint.frame2 = frame2;
                applied = true;
            } else if let Ok(mut joint) = joints.p1().get_mut(binding.joint) {
                joint.frame1 = frame1;
                joint.frame2 = frame2;
                applied = true;
            }
            if applied {
                commands.entity(binding.joint).insert(JointFramesConfigured);
            }
        }
    }
}

fn cleanup_orphaned_ragdolls(
    mut commands: Commands,
    roots: Query<(Entity, &RagdollRootOwner)>,
    owners: Query<(), With<HumanoidRagdoll>>,
) {
    for (root, owner) in &roots {
        if owners.get(owner.0).is_err() {
            commands.entity(root).despawn();
        }
    }
}

fn drive_or_release_bodies(
    mut commands: Commands,
    mode: Res<RagdollMode>,
    mut reset: ResMut<RagdollReset>,
    ragdolls: Query<&HumanoidRagdoll>,
    globals: Query<&GlobalTransform>,
    mut bodies: Query<(
        &RigidBody,
        &mut Position,
        &mut Rotation,
        &mut LinearVelocity,
        &mut AngularVelocity,
    )>,
) {
    let reset_now = std::mem::take(&mut reset.0);
    for ragdoll in &ragdolls {
        for binding in &ragdoll.bindings {
            let Ok((rigid_body, mut position, mut rotation, mut linear, mut angular)) =
                bodies.get_mut(binding.body)
            else {
                continue;
            };
            let animate = *mode == RagdollMode::Animated || reset_now;
            let target_body = if animate {
                RigidBody::Kinematic
            } else {
                RigidBody::Dynamic
            };
            if *rigid_body != target_body {
                commands.entity(binding.body).insert(target_body);
            }
            if animate && let Ok(global) = globals.get(binding.bone) {
                position.0 = global.translation();
                rotation.0 = global.rotation();
                linear.0 = Vec3::ZERO;
                angular.0 = Vec3::ZERO;
            }
        }
    }
}

fn motor_target(kind: HingeKind, profile: &ActiveMotorProfile) -> Option<f32> {
    match kind {
        HingeKind::Knee => Some(profile.target_knee_radians),
        HingeKind::Elbow => Some(profile.target_elbow_radians),
        HingeKind::PassiveAnkle => None,
    }
}

fn apply_active_motors(
    mode: Res<RagdollMode>,
    profile: Res<ActiveMotorProfile>,
    time: Res<Time<Fixed>>,
    mut blend: ResMut<ActiveMotorBlend>,
    ragdolls: Query<&HumanoidRagdoll>,
    mut joints: Query<&mut AvianRevoluteJoint>,
) {
    let requested = if *mode == RagdollMode::Active {
        profile.strength.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let max_step = 4.0 * time.delta_secs();
    blend.strength += (requested - blend.strength).clamp(-max_step, max_step);
    blend.tick = blend.tick.saturating_add(1);

    for ragdoll in &ragdolls {
        for hinge in &ragdoll.hinges {
            let Some(target) = motor_target(hinge.kind, &profile) else {
                if let Ok(mut joint) = joints.get_mut(hinge.joint) {
                    let _ = apply_active_motor(
                        &mut joint,
                        ActiveRevoluteMotor::default(),
                        hinge.axis_sign,
                    );
                }
                continue;
            };
            let settings = ActiveRevoluteMotor {
                enabled: blend.strength > f32::EPSILON,
                target_position: target,
                target_velocity: 0.0,
                max_torque: profile.max_torque * blend.strength,
                frequency_hz: 6.0 * blend.strength,
                damping_ratio: 1.0,
            };
            if let Ok(mut joint) = joints.get_mut(hinge.joint) {
                let _ = apply_active_motor(&mut joint, settings, hinge.axis_sign);
            }
        }
    }
}

fn wrapped_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn revolute_angle(joint: &AvianRevoluteJoint, rotation1: Quat, rotation2: Quat) -> Option<f32> {
    let basis1 = joint.local_basis1()?;
    let basis2 = joint.local_basis2()?;
    let axis = joint.hinge_axis.try_normalize()?;
    let orthogonal = axis.any_orthonormal_vector();
    let a1 = rotation1 * basis1 * axis;
    let b1 = rotation1 * basis1 * orthogonal;
    let b2 = rotation2 * basis2 * orthogonal;
    Some(b1.cross(b2).dot(a1).atan2(b1.dot(b2)))
}

fn sample_motor_telemetry(
    profile: Res<ActiveMotorProfile>,
    blend: Res<ActiveMotorBlend>,
    ragdolls: Query<&HumanoidRagdoll>,
    joints: Query<&AvianRevoluteJoint>,
    rotations: Query<&Rotation>,
    linear_velocities: Query<&LinearVelocity>,
    mut telemetry: ResMut<RagdollMotorTelemetry>,
) {
    for ragdoll in &ragdolls {
        let mut driven = 0;
        let mut error_sum = 0.0;
        let mut maximum_error = 0.0_f32;
        let mut finite = true;
        for hinge in &ragdoll.hinges {
            let Some(target) = motor_target(hinge.kind, &profile) else {
                continue;
            };
            let (Ok(joint), Ok(rotation1), Ok(rotation2)) = (
                joints.get(hinge.joint),
                rotations.get(hinge.body1),
                rotations.get(hinge.body2),
            ) else {
                finite = false;
                continue;
            };
            if joint.motor.enabled {
                driven += 1;
            }
            let Some(angle) = revolute_angle(joint, rotation1.0, rotation2.0) else {
                finite = false;
                continue;
            };
            let error = wrapped_angle(target * hinge.axis_sign - angle).abs();
            finite &= error.is_finite();
            if error.is_finite() {
                error_sum += error;
                maximum_error = maximum_error.max(error);
            }
        }
        let driven_contract_count = ragdoll
            .hinges
            .iter()
            .filter(|hinge| motor_target(hinge.kind, &profile).is_some())
            .count();
        let pelvis_speed = ragdoll
            .focus_body
            .and_then(|body| linear_velocities.get(body).ok())
            .map_or(f32::NAN, |velocity| velocity.0.length());
        finite &= pelvis_speed.is_finite();
        telemetry.samples.push(MotorTelemetrySample {
            tick: blend.tick,
            strength: blend.strength,
            driven_hinges: driven,
            mean_error_radians: if driven_contract_count == 0 {
                0.0
            } else {
                error_sum / driven_contract_count as f32
            },
            maximum_error_radians: maximum_error,
            pelvis_speed,
            finite,
        });
        const MAX_TELEMETRY_SAMPLES: usize = 256;
        if telemetry.samples.len() > MAX_TELEMETRY_SAMPLES {
            let excess = telemetry.samples.len() - MAX_TELEMETRY_SAMPLES;
            telemetry.samples.drain(..excess);
        }
    }
}

fn capture_solved_body_poses(
    mut ragdolls: Query<(
        &HumanoidRagdoll,
        &mut CapturedRagdollPose,
        &mut RagdollPresentationFocus,
    )>,
    body_poses: Query<(&Position, &Rotation, &LinearVelocity)>,
) {
    for (ragdoll, mut captured, mut focus) in &mut ragdolls {
        captured.0.clear();
        if let Some(focus_body) = ragdoll.focus_body
            && let Ok((position, _, linear_velocity)) = body_poses.get(focus_body)
        {
            focus.position = position.0;
            focus.linear_velocity = linear_velocity.0;
        }
        for binding in &ragdoll.bindings {
            if let Ok((position, rotation, _)) = body_poses.get(binding.body) {
                captured.0.insert(
                    binding.bone,
                    Transform::from_translation(position.0).with_rotation(rotation.0),
                );
            }
        }
    }
}

fn read_passive_pose_to_rendered_bones(
    mode: Res<RagdollMode>,
    mut commands: Commands,
    ragdolls: Query<(&HumanoidRagdoll, &CapturedRagdollPose)>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut bone_transforms: ParamSet<(Query<(Entity, &Transform)>, Query<&mut Transform>)>,
) {
    let local_snapshot = bone_transforms
        .p0()
        .iter()
        .map(|(entity, transform)| (entity, *transform))
        .collect::<HashMap<_, _>>();
    let mut writable_bones = bone_transforms.p1();
    for (ragdoll, captured) in &ragdolls {
        let _root_is_kept_for_lifecycle = ragdoll.root;
        for binding in &ragdoll.bindings {
            if *mode == RagdollMode::Animated {
                commands.entity(binding.bone).remove::<RagdollOwnedBone>();
                continue;
            }
            let Ok(parent) = parents.get(binding.bone) else {
                continue;
            };
            let Some(parent_world) = resolved_world_transform(
                parent.parent(),
                &captured.0,
                &local_snapshot,
                &parents,
                &globals,
                0,
            ) else {
                continue;
            };
            let Some(desired_world) = captured.0.get(&binding.bone).copied() else {
                continue;
            };
            let Ok(mut local) = writable_bones.get_mut(binding.bone) else {
                continue;
            };
            *local = local_from_world(parent_world, desired_world);
            commands.entity(binding.bone).insert(RagdollOwnedBone);
        }
    }
}

fn resolved_world_transform(
    entity: Entity,
    desired: &HashMap<Entity, Transform>,
    locals: &HashMap<Entity, Transform>,
    parents: &Query<&ChildOf>,
    globals: &Query<&GlobalTransform>,
    depth: usize,
) -> Option<Transform> {
    if let Some(transform) = desired.get(&entity) {
        return Some(*transform);
    }
    if depth >= 64 {
        return None;
    }
    if let (Some(local), Ok(parent)) = (locals.get(&entity), parents.get(entity)) {
        let parent_world = resolved_world_transform(
            parent.parent(),
            desired,
            locals,
            parents,
            globals,
            depth + 1,
        )?;
        return Some(parent_world.mul_transform(*local));
    }
    globals
        .get(entity)
        .ok()
        .map(GlobalTransform::compute_transform)
}

fn local_from_world(parent_world: Transform, desired_world: Transform) -> Transform {
    Transform::from_matrix(parent_world.to_matrix().inverse() * desired_world.to_matrix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conservative_topology_excludes_twist_toe_and_weapon_bones() {
        assert_eq!(BODY_SPECS.len(), 15);
        assert!(!BODY_SPECS.iter().any(|(role, _, _)| matches!(
            role,
            BoneRole::ThighTwistLeft
                | BoneRole::ShinTwistLeft
                | BoneRole::ToeLeft
                | BoneRole::WeaponLeft
                | BoneRole::ThighTwistRight
                | BoneRole::ShinTwistRight
                | BoneRole::ToeRight
                | BoneRole::WeaponRight
        )));
    }

    #[test]
    fn topology_has_one_body_per_role_and_expected_hinges() {
        let unique = BODY_SPECS
            .iter()
            .map(|(role, _, _)| *role)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), BODY_SPECS.len());
        assert!(HINGE_LINKS.contains(&(BoneRole::ThighLeft, BoneRole::ShinLeft, HingeKind::Knee)));
        assert!(HINGE_LINKS.contains(&(
            BoneRole::UpperArmRight,
            BoneRole::ForearmRight,
            HingeKind::Elbow
        )));
    }

    #[test]
    fn rotated_body_joint_frame_reconstructs_world_anchor_and_basis() {
        let body = Transform::from_xyz(2.0, 1.0, -3.0).with_rotation(Quat::from_rotation_y(0.7));
        let pivot = Vec3::new(2.4, 1.3, -2.8);
        let basis = Quat::from_rotation_x(0.4);
        let frame = local_joint_frame(body, pivot, basis)
            .get_local_isometry()
            .expect("helper always creates a local frame");
        let reconstructed = body.mul_transform(
            Transform::from_translation(frame.translation.into()).with_rotation(frame.rotation),
        );
        assert!(reconstructed.translation.distance(pivot) < 1.0e-5);
        assert!(reconstructed.rotation.angle_between(basis) < 1.0e-5);
    }

    #[test]
    fn parent_and_child_readback_preserve_both_world_poses() {
        let desired_parent =
            Transform::from_xyz(1.0, 2.0, 3.0).with_rotation(Quat::from_rotation_z(0.4));
        let desired_child =
            Transform::from_xyz(1.2, 2.7, 3.1).with_rotation(Quat::from_rotation_y(-0.3));
        let child_local = local_from_world(desired_parent, desired_child);
        let reconstructed = desired_parent.mul_transform(child_local);
        assert!(
            reconstructed
                .translation
                .distance(desired_child.translation)
                < 1.0e-5
        );
        assert!(reconstructed.rotation.angle_between(desired_child.rotation) < 1.0e-5);
    }

    #[test]
    fn manual_body_contract_removes_both_graph_kinematic_targets() {
        let mut world = World::new();
        let body = world
            .spawn(RelativeKinematicBodyPositionBased::default())
            .id();
        assert!(
            world
                .entity(body)
                .contains::<RelativeKinematicBodyPositionBased>()
        );
        assert!(world.entity(body).contains::<RelativeKinematicBody>());
        world
            .entity_mut(body)
            .remove::<RelativeKinematicBodyPositionBased>()
            .remove::<RelativeKinematicBody>()
            .insert(ManuallyOwnedRagdollBody);
        assert!(
            !world
                .entity(body)
                .contains::<RelativeKinematicBodyPositionBased>()
        );
        assert!(!world.entity(body).contains::<RelativeKinematicBody>());
        assert!(world.entity(body).contains::<ManuallyOwnedRagdollBody>());
    }

    #[test]
    fn mode_cycle_is_animated_active_passive() {
        assert_eq!(RagdollMode::Animated.next(), RagdollMode::Active);
        assert_eq!(RagdollMode::Active.next(), RagdollMode::Passive);
        assert_eq!(RagdollMode::Passive.next(), RagdollMode::Animated);
    }

    #[test]
    fn ankle_hinges_are_explicitly_not_driven() {
        let profile = ActiveMotorProfile::default();
        assert_eq!(motor_target(HingeKind::PassiveAnkle, &profile), None);
        assert!(motor_target(HingeKind::Knee, &profile).is_some());
        assert!(motor_target(HingeKind::Elbow, &profile).is_some());
    }

    #[test]
    fn revolute_metric_matches_joint_frame_signed_angle() {
        let joint = AvianRevoluteJoint::new(Entity::PLACEHOLDER, Entity::PLACEHOLDER)
            .with_hinge_axis(Vec3::X);
        let expected = 0.6;
        let measured = revolute_angle(&joint, Quat::IDENTITY, Quat::from_rotation_x(expected))
            .expect("local joint frames yield an angle");
        assert!((wrapped_angle(expected - measured)).abs() < 1.0e-5);
    }
}
