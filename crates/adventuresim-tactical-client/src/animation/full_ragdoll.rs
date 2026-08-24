//! Detailed client-side ragdoll used while the replicated body mode is full
//! ragdoll. The server owns the coarse pelvis/root dynamic body; these bodies
//! supply articulated visual motion and terrain contact only.

use std::collections::{BTreeMap, HashMap};

use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

use super::{
    PresentedSkeleton,
    procedural::{BoneRole, HumanoidRig},
};

const RAGDOLL_LAYER: u32 = 1 << 7;

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

const HINGE_LINKS: [(BoneRole, BoneRole); 6] = [
    (BoneRole::ThighLeft, BoneRole::ShinLeft),
    (BoneRole::ShinLeft, BoneRole::FootLeft),
    (BoneRole::ThighRight, BoneRole::ShinRight),
    (BoneRole::ShinRight, BoneRole::FootRight),
    (BoneRole::UpperArmLeft, BoneRole::ForearmLeft),
    (BoneRole::UpperArmRight, BoneRole::ForearmRight),
];

#[derive(Debug, Clone, Copy)]
struct BodyBinding {
    bone: Entity,
    body: Entity,
    world_scale: Vec3,
}

#[derive(Component)]
pub(super) struct FullBodyRagdoll {
    parts: Vec<Entity>,
    bodies: Vec<BodyBinding>,
    pelvis_body: Entity,
    pelvis_owner_local: Transform,
}

#[derive(Component)]
pub(super) struct RagdollPart {
    owner: Entity,
}

#[derive(Component)]
pub(super) struct RagdollBodyPart {
    radius: f32,
    half_length: f32,
}

/// Resolves presentation-only body contacts against the authoritative terrain
/// heightfield. Avian still integrates and constrains the articulated bodies;
/// keeping terrain response out of its contact graph avoids mixing this
/// client-only island with the disabled replicated collision world.
pub(super) fn resolve_ragdoll_terrain_contacts(
    terrains: Query<&SceneTerrain>,
    mut bodies: Query<(
        &RagdollBodyPart,
        &mut Position,
        &Rotation,
        &mut LinearVelocity,
    )>,
) {
    for (part, mut position, rotation, mut velocity) in &mut bodies {
        let vertical_extent =
            part.radius + part.half_length * (rotation.0 * Vec3::Y).dot(Vec3::Y).abs();
        let ground = terrains
            .iter()
            .filter_map(|terrain| terrain.height_at(position.0.xz()))
            .reduce(f32::max);
        let Some(minimum_y) = ground.map(|height| height + vertical_extent) else {
            continue;
        };
        if position.0.y < minimum_y {
            position.0.y = minimum_y;
            if velocity.y < 0.0 {
                velocity.y *= -0.12;
            }
            velocity.x *= 0.78;
            velocity.z *= 0.78;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_full_ragdolls(
    mut commands: Commands,
    owners: Query<
        (
            Entity,
            &PresentedSkeleton,
            &HumanoidRig,
            &GlobalTransform,
            Option<&FullBodyRagdoll>,
        ),
        With<Player>,
    >,
    globals: Query<&GlobalTransform>,
    parts: Query<(Entity, &RagdollPart)>,
) {
    for (owner, skeleton, rig, owner_global, ragdoll) in &owners {
        if skeleton.body() != BodyState::Ragdolled {
            if let Some(ragdoll) = ragdoll {
                for part in &ragdoll.parts {
                    commands.entity(*part).despawn();
                }
                commands.entity(owner).remove::<FullBodyRagdoll>();
            }
            continue;
        }
        let Some(ragdoll) = ragdoll else {
            spawn_full_ragdoll(&mut commands, owner, skeleton, rig, owner_global, &globals);
            continue;
        };
        let target = owner_global
            .compute_transform()
            .mul_transform(ragdoll.pelvis_owner_local);
        commands
            .entity(ragdoll.pelvis_body)
            .insert((Position(target.translation), Rotation(target.rotation)));
    }

    for (part, ragdoll_part) in &parts {
        if owners.get(ragdoll_part.owner).is_err() {
            commands.entity(part).despawn();
        }
    }
}

fn spawn_full_ragdoll(
    commands: &mut Commands,
    owner: Entity,
    skeleton: &PresentedSkeleton,
    rig: &HumanoidRig,
    owner_global: &GlobalTransform,
    globals: &Query<&GlobalTransform>,
) {
    let mut role_bodies = BTreeMap::new();
    let mut bodies = Vec::new();
    let mut parts = Vec::new();
    for (role, radius, length) in BODY_SPECS {
        let Some(&bone) = rig.get(&role) else {
            return;
        };
        let Ok(global) = globals.get(bone) else {
            return;
        };
        let rigid_body = if role == BoneRole::Pelvis {
            RigidBody::Kinematic
        } else {
            RigidBody::Dynamic
        };
        let body = commands
            .spawn((
                Name::new(format!("Presentation ragdoll {role:?}")),
                rigid_body,
                Collider::capsule(radius, length),
                CollisionLayers::new(RAGDOLL_LAYER, 0u32),
                Transform::from_translation(global.translation()).with_rotation(global.rotation()),
                LinearVelocity(skeleton.world_velocity),
                AngularVelocity(initial_angular_velocity(role, skeleton.world_velocity)),
                LinearDamping(0.35),
                AngularDamping(1.2),
                SleepingDisabled,
                RagdollPart { owner },
                RagdollBodyPart {
                    radius,
                    half_length: length * 0.5,
                },
            ))
            .remove::<RigidBodyDisabled>()
            .id();
        role_bodies.insert(role, body);
        bodies.push(BodyBinding {
            bone,
            body,
            world_scale: global.compute_transform().scale,
        });
        parts.push(body);
    }

    for (parent, child) in SPHERICAL_LINKS {
        let Some(joint) = spherical_joint(parent, child, rig, globals, &role_bodies) else {
            continue;
        };
        let entity = commands.spawn((joint, RagdollPart { owner })).id();
        parts.push(entity);
    }
    for (parent, child) in HINGE_LINKS {
        let Some(joint) = hinge_joint(parent, child, rig, globals, &role_bodies) else {
            continue;
        };
        let entity = commands.spawn((joint, RagdollPart { owner })).id();
        parts.push(entity);
    }

    let pelvis_body = role_bodies[&BoneRole::Pelvis];
    let pelvis_global = globals
        .get(*rig.get(&BoneRole::Pelvis).expect("pelvis checked above"))
        .expect("pelvis global checked above")
        .compute_transform();
    let pelvis_owner_local = local_from_world(owner_global.compute_transform(), pelvis_global);
    commands.entity(owner).insert(FullBodyRagdoll {
        parts,
        bodies,
        pelvis_body,
        pelvis_owner_local,
    });
}

fn initial_angular_velocity(role: BoneRole, world_velocity: Vec3) -> Vec3 {
    let speed_seed = world_velocity.xz().length().clamp(0.0, 8.0) * 0.12;
    let (pitch, roll) = match role {
        BoneRole::Chest => (0.22, -0.12),
        BoneRole::Head => (-0.18, 0.2),
        BoneRole::UpperArmLeft | BoneRole::ForearmRight | BoneRole::ThighRight => (0.1, 0.45),
        BoneRole::UpperArmRight | BoneRole::ForearmLeft | BoneRole::ThighLeft => (-0.1, -0.45),
        BoneRole::ShinLeft | BoneRole::FootRight | BoneRole::HandLeft => (0.3, -0.18),
        BoneRole::ShinRight | BoneRole::FootLeft | BoneRole::HandRight => (-0.3, 0.18),
        _ => (0.0, 0.0),
    };
    Vec3::new(pitch + speed_seed, 0.0, roll)
}

fn body_transform(
    role: BoneRole,
    rig: &HumanoidRig,
    globals: &Query<&GlobalTransform>,
) -> Option<Transform> {
    Some(globals.get(*rig.get(&role)?).ok()?.compute_transform())
}

fn local_anchor(body: Transform, pivot_world: Vec3) -> Vec3 {
    body.rotation.inverse() * (pivot_world - body.translation)
}

fn spherical_joint(
    parent: BoneRole,
    child: BoneRole,
    rig: &HumanoidRig,
    globals: &Query<&GlobalTransform>,
    bodies: &BTreeMap<BoneRole, Entity>,
) -> Option<SphericalJoint> {
    let parent_transform = body_transform(parent, rig, globals)?;
    let child_transform = body_transform(child, rig, globals)?;
    let pivot = child_transform.translation;
    let (swing, twist) = match (parent, child) {
        (BoneRole::Pelvis, BoneRole::ThighLeft | BoneRole::ThighRight) => (0.58, 0.38),
        (BoneRole::Pelvis, BoneRole::Chest) => (0.5, 0.38),
        (BoneRole::Chest, BoneRole::Head) => (0.52, 0.48),
        _ => (0.9, 0.7),
    };
    Some(
        SphericalJoint::new(*bodies.get(&parent)?, *bodies.get(&child)?)
            .with_local_anchor1(local_anchor(parent_transform, pivot))
            .with_local_anchor2(local_anchor(child_transform, pivot))
            .with_swing_limits(-swing, swing)
            .with_twist_limits(-twist, twist),
    )
}

fn hinge_joint(
    parent: BoneRole,
    child: BoneRole,
    rig: &HumanoidRig,
    globals: &Query<&GlobalTransform>,
    bodies: &BTreeMap<BoneRole, Entity>,
) -> Option<RevoluteJoint> {
    let parent_transform = body_transform(parent, rig, globals)?;
    let child_transform = body_transform(child, rig, globals)?;
    let pivot = child_transform.translation;
    Some(
        RevoluteJoint::new(*bodies.get(&parent)?, *bodies.get(&child)?)
            .with_local_anchor1(local_anchor(parent_transform, pivot))
            .with_local_anchor2(local_anchor(child_transform, pivot))
            .with_hinge_axis(Vec3::X)
            .with_angle_limits(-0.15, 2.5),
    )
}

pub(super) fn apply_full_ragdoll_pose(
    ragdolls: Query<&FullBodyRagdoll>,
    body_poses: Query<(&Position, &Rotation)>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut transforms: ParamSet<(Query<(Entity, &Transform)>, Query<&mut Transform>)>,
) {
    let locals = transforms
        .p0()
        .iter()
        .map(|(entity, transform)| (entity, *transform))
        .collect::<HashMap<_, _>>();
    let mut writable = transforms.p1();
    for ragdoll in &ragdolls {
        let desired = ragdoll
            .bodies
            .iter()
            .filter_map(|binding| {
                let (position, rotation) = body_poses.get(binding.body).ok()?;
                Some((
                    binding.bone,
                    Transform::from_translation(position.0)
                        .with_rotation(rotation.0)
                        .with_scale(binding.world_scale),
                ))
            })
            .collect::<HashMap<_, _>>();
        for binding in &ragdoll.bodies {
            let Ok(parent) = parents.get(binding.bone) else {
                continue;
            };
            let Some(parent_world) =
                resolved_world_transform(parent.parent(), &desired, &locals, &parents, &globals, 0)
            else {
                continue;
            };
            let Some(desired_world) = desired.get(&binding.bone).copied() else {
                continue;
            };
            if let Ok(mut local) = writable.get_mut(binding.bone) {
                // Rig translations and scales encode the authored skeleton's
                // segment lengths. Physics owns joint orientation, but copying
                // body positions into those locals would stretch or collapse
                // the skinned hierarchy whenever an armature has helper bones
                // or non-unit import scale.
                local.rotation =
                    (parent_world.rotation.inverse() * desired_world.rotation).normalize();
            }
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
        return Some(
            resolved_world_transform(
                parent.parent(),
                desired,
                locals,
                parents,
                globals,
                depth + 1,
            )?
            .mul_transform(*local),
        );
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
    fn conservative_ragdoll_has_expected_topology() {
        let unique = BODY_SPECS
            .iter()
            .map(|(role, _, _)| *role)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), 15);
        assert_eq!(SPHERICAL_LINKS.len() + HINGE_LINKS.len(), 12);
        assert!(!unique.contains(&BoneRole::WeaponLeft));
        assert!(!unique.contains(&BoneRole::ToeRight));
    }

    #[test]
    fn local_world_round_trip_preserves_pose() {
        let parent = Transform::from_xyz(1.0, 2.0, 3.0).with_rotation(Quat::from_rotation_z(0.4));
        let desired = Transform::from_xyz(1.2, 2.7, 3.1).with_rotation(Quat::from_rotation_y(-0.3));
        let reconstructed = parent.mul_transform(local_from_world(parent, desired));
        assert!(reconstructed.translation.distance(desired.translation) < 1.0e-5);
        assert!(reconstructed.rotation.angle_between(desired.rotation) < 1.0e-5);
    }
}
