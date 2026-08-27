use super::*;

/// Applies optional client-only hand targets and held-item constraints. Missing
/// targets, sockets, or arm bones are intentionally inert.
pub(in crate::animation) fn apply_arm_and_weapon_constraints(
    rigs: Query<(Entity, &HumanoidRig)>,
    parents: Query<&ChildOf>,
    targets: Query<&HumanoidIkTargets>,
    mut ik_states: Query<&mut ArmIkState>,
    weapon_constraints: Query<(Entity, &HeldWeaponConstraint)>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
    mut commands: Commands,
) {
    // Move an explicitly targeted primary hand first. The socket is a child of
    // that hand, so the weapon placement below observes the same-frame result.
    for (_, constraint) in &weapon_constraints {
        let Ok((_, rig)) = rigs.get(constraint.owner) else {
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
        let Ok((_, rig)) = rigs.get(constraint.owner) else {
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
            weapon_socket_world_transform(socket_global),
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

    for (owner, rig) in &rigs {
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
                rig,
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
            commands.entity(owner).insert(ArmIkState(memory));
        }
    }
}

fn weapon_socket_world_transform(socket: GlobalTransform) -> Transform {
    Transform {
        translation: socket.translation(),
        rotation: socket.rotation(),
        scale: Vec3::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_weapon_uses_the_authored_socket_orientation_without_inheriting_scale() {
        let socket = GlobalTransform::from(Transform {
            translation: Vec3::new(0.4, 1.2, -0.3),
            rotation: Quat::from_euler(EulerRot::XYZ, 0.7, -0.4, 1.1),
            scale: Vec3::splat(1.25),
        });

        let weapon = weapon_socket_world_transform(socket);

        assert!(weapon.translation.abs_diff_eq(socket.translation(), 1e-5));
        assert!(weapon.rotation.dot(socket.rotation()).abs() > 1.0 - 1e-5);
        assert_eq!(weapon.scale, Vec3::ONE);
    }
}

fn apply_hand_target(
    owner: Entity,
    rig: &HumanoidRig,
    (upper_role, lower_role, hand_role): (BoneRole, BoneRole, BoneRole),
    target: HandIkTarget,
    left: bool,
    memory: &mut ArmIkMemory,
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

pub(in crate::animation::procedural) fn secondary_grip_world(
    weapon: GlobalTransform,
    local_grip: Vec3,
) -> Vec3 {
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
mod memory_tests {
    use super::*;

    #[test]
    fn resetting_leg_memory_cannot_reset_arm_continuity() {
        let mut leg = LegIkState(LegIkMemory {
            left_leg: Some(Vec3::X),
            ..default()
        });
        let arm = ArmIkState(ArmIkMemory {
            left_arm: Some(Vec3::Y),
            ..default()
        });
        assert_eq!(leg.0.left_leg, Some(Vec3::X));
        leg = LegIkState::default();
        assert_eq!(leg.0.left_leg, None);
        assert_eq!(arm.0.left_arm, Some(Vec3::Y));
    }
}
