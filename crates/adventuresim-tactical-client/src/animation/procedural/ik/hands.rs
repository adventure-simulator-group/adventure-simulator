use super::*;

#[derive(Debug, Clone, Copy)]
struct HandChain {
    upper_role: BoneRole,
    lower_role: BoneRole,
    hand_role: BoneRole,
    side: HandSide,
}

impl HandChain {
    const LEFT: Self = Self {
        upper_role: BoneRole::UpperArmLeft,
        lower_role: BoneRole::ForearmLeft,
        hand_role: BoneRole::HandLeft,
        side: HandSide::Left,
    };
    const RIGHT: Self = Self {
        upper_role: BoneRole::UpperArmRight,
        lower_role: BoneRole::ForearmRight,
        hand_role: BoneRole::HandRight,
        side: HandSide::Right,
    };

    fn for_side(side: HandSide) -> Self {
        match side {
            HandSide::Left => Self::LEFT,
            HandSide::Right => Self::RIGHT,
        }
    }

    fn target(self, targets: HumanoidIkTargets) -> Option<HandIkTarget> {
        match self.side {
            HandSide::Left => targets.left,
            HandSide::Right => targets.right,
        }
    }

    fn is_left(self) -> bool {
        self.side == HandSide::Left
    }
}

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
        let chain = HandChain::for_side(constraint.primary_hand);
        let target = chain.target(explicit);
        let Some(target) = target else { continue };
        let mut memory = ik_states
            .get_mut(constraint.owner)
            .map(|state| state.0)
            .unwrap_or_default();
        apply_hand_target(
            constraint.owner,
            rig,
            chain,
            target,
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
            socket_global
                .mul_transform(constraint.socket_bind_correction)
                .compute_transform(),
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
        for (chain, target) in [
            (HandChain::LEFT, combined.left),
            (HandChain::RIGHT, combined.right),
        ] {
            let Some(target) = target else { continue };
            apply_hand_target(
                owner,
                rig,
                chain,
                target,
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

fn apply_hand_target(
    owner: Entity,
    rig: &HumanoidRig,
    chain: HandChain,
    target: HandIkTarget,
    memory: &mut ArmIkMemory,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let weight = target.weight.clamp(0.0, 1.0);
    if weight <= f32::EPSILON {
        return;
    }
    let (Some(&upper), Some(&lower), Some(&hand)) = (
        rig.get(&chain.upper_role),
        rig.get(&chain.lower_role),
        rig.get(&chain.hand_role),
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
    let remembered = if chain.is_left() {
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
        TwoBoneChain::new(
            upper_snapshot.global.translation(),
            lower_snapshot.global.translation(),
            hand_snapshot.global.translation(),
            upper_snapshot
                .global
                .translation()
                .distance(lower_snapshot.global.translation()),
            lower_snapshot
                .global
                .translation()
                .distance(hand_snapshot.global.translation()),
            pole_to_world(owner_rotation, remembered.unwrap_or(Vec3::NEG_Y)),
        ),
        blended_target,
    ) {
        apply_two_bone_solution(upper, lower, hand, solution, parents, transforms);
        let bend = (solution.knee - upper_snapshot.global.translation())
            .reject_from_normalized(solution.end_direction);
        if let Some(valid) = bend.try_normalize() {
            if chain.is_left() {
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
