use super::*;

// Preserve a little margin below the viewer's 2 cm pelvis-step contract.
// Keep the normal knee reserve while a landing visibly carries weight, then
// release it before the pelvis reaches the authored height. The released reach
// remains capped at the authored leg extension, preventing a final
// recovery-frame foot lift or snap without introducing a straight-leg target.

pub(in crate::animation::procedural) fn plant_is_continuous(
    plant: Vec3,
    current_foot: Vec3,
) -> bool {
    plant.is_finite()
        && current_foot.is_finite()
        && plant.distance(current_foot) <= ik_tuning().maximum_plant_discontinuity_metres
}

pub(in crate::animation::procedural) fn advance_pelvis_shift(
    current: f32,
    desired: f32,
    delta_seconds: f32,
) -> f32 {
    let maximum_step = (ik_tuning().pelvis_correction_speed_metres_per_second
        * delta_seconds.max(0.0))
    .min(maximum_pelvis_correction_step_metres());
    current + (desired - current).clamp(-maximum_step, maximum_step)
}

pub(in crate::animation::procedural) fn maximum_reach(upper_length: f32, lower_length: f32) -> f32 {
    (upper_length * upper_length
        + lower_length * lower_length
        + 2.0
            * upper_length
            * lower_length
            * ik_tuning().minimum_knee_flexion_degrees.to_radians().cos())
    .sqrt()
}

pub(in crate::animation::procedural) fn landing_maximum_reach(
    upper_length: f32,
    lower_length: f32,
    authored_reach: f32,
    compression: f32,
) -> f32 {
    let reserved_reach = maximum_reach(upper_length, lower_length);
    let full_reach = upper_length + lower_length - 0.0001;
    let released_reach = authored_reach.clamp(reserved_reach, full_reach);
    let reserve_weight = smoothstep(
        ik_tuning().landing_knee_reserve_release_compression_metres,
        ik_tuning().landing_knee_reserve_full_compression_metres,
        compression,
    );
    released_reach.lerp(reserved_reach, reserve_weight)
}

pub(in crate::animation::procedural) fn constrain_target_to_reach(
    target: Vec3,
    root: Vec3,
    maximum_reach: f32,
) -> Vec3 {
    let vertical = target.y - root.y;
    let maximum_horizontal = (maximum_reach * maximum_reach - vertical * vertical)
        .max(0.0)
        .sqrt();
    let horizontal = (target - root).xz().clamp_length_max(maximum_horizontal);
    Vec3::new(root.x + horizontal.x, target.y, root.z + horizontal.y)
}

pub(in crate::animation::procedural) fn canonical_knee_pole(side: f32) -> Vec3 {
    (Vec3::Z + Vec3::X * side * 0.18).normalize()
}

#[derive(Debug, Clone, Copy)]
pub(in crate::animation::procedural) struct TwoBoneSolution {
    pub(in crate::animation::procedural) knee: Vec3,
    pub(in crate::animation::procedural) end: Vec3,
    pub(in crate::animation::procedural) end_direction: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::animation::procedural) struct TwoBoneChain {
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
}

impl TwoBoneChain {
    pub(in crate::animation::procedural) const fn new(
        root: Vec3,
        current_knee: Vec3,
        current_end: Vec3,
        upper_length: f32,
        lower_length: f32,
        pole_direction: Vec3,
    ) -> Self {
        Self {
            root,
            current_knee,
            current_end,
            upper_length,
            lower_length,
            pole_direction,
        }
    }
}

pub(in crate::animation::procedural) fn solve_two_bone(
    chain: TwoBoneChain,
    target: Vec3,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        chain,
        target,
        maximum_reach(chain.upper_length, chain.lower_length),
        true,
    )
}

pub(in crate::animation::procedural) fn solve_landing_two_bone(
    chain: TwoBoneChain,
    target: Vec3,
    compression: f32,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        chain,
        target,
        landing_maximum_reach(
            chain.upper_length,
            chain.lower_length,
            chain.root.distance(chain.current_end),
            compression,
        ),
        // Landing supplies a foot-facing-constrained leg pole. Reblending the
        // authored knee here would occur after that constraint and could
        // recreate an anatomically impossible sideways bend.
        false,
    )
}

pub(in crate::animation::procedural) fn solve_two_bone_with_reach(
    chain: TwoBoneChain,
    target: Vec3,
    maximum_target_reach: f32,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(chain, target, maximum_target_reach, false)
}

pub(in crate::animation::procedural) fn advance_foot_target_at_speed(
    previous: Option<Vec3>,
    desired: Vec3,
    delta_seconds: f32,
    maximum_speed: f32,
) -> Vec3 {
    let Some(previous) = previous.filter(|position| position.is_finite()) else {
        return desired;
    };
    if !desired.is_finite() {
        return previous;
    }
    let maximum_step = maximum_speed.max(0.0) * delta_seconds.max(0.0);
    previous + (desired - previous).clamp_length_max(maximum_step)
}

fn solve_two_bone_internal(
    chain: TwoBoneChain,
    target: Vec3,
    maximum_target_reach: f32,
    preserve_authored_bend: bool,
) -> Option<TwoBoneSolution> {
    let TwoBoneChain {
        root,
        current_knee,
        current_end,
        upper_length,
        lower_length,
        pole_direction,
    } = chain;
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

pub(in crate::animation::procedural) fn snapshot(
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

pub(in crate::animation::procedural) fn snapshot_chain(
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

pub(in crate::animation::procedural) fn apply_two_bone_solution(
    upper: Entity,
    lower: Entity,
    end: Entity,
    solution: TwoBoneSolution,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let Some((upper_before, lower_before, end_before)) =
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

    // The analytic solve owns joint positions, not an airborne foot's authored
    // facing. Recompute through the newly rotated parent hierarchy and restore
    // the end bone's pre-solve world orientation. Contact slope alignment runs
    // after this seam and intentionally overrides it when the sole is loaded.
    let Some(end_after) = snapshot(end, parents, &transforms.p0()) else {
        return;
    };
    let local = end_after.parent_rotation.inverse() * end_before.global.rotation();
    if local.is_finite()
        && let Ok(mut transform) = transforms.p1().get_mut(end)
    {
        transform.rotation = local.normalize();
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
        let solved = solve_two_bone(
            TwoBoneChain::new(root, knee, end, 1.0, 1.0, Vec3::NEG_Z),
            target,
        )
        .unwrap();
        assert!((root.distance(solved.knee) - 1.0).abs() < 0.0001);
        assert!((solved.knee.distance(solved.end) - 1.0).abs() < 0.0001);
        assert!(solved.end.abs_diff_eq(target, 0.0001));
    }

    #[test]
    fn two_bone_solver_clamps_unreachable_target_without_nan() {
        let solved = solve_two_bone(
            TwoBoneChain::new(
                Vec3::ZERO,
                Vec3::new(0.0, -1.0, 0.1),
                Vec3::new(0.0, -2.0, 0.0),
                1.0,
                1.0,
                Vec3::NEG_Z,
            ),
            Vec3::new(0.0, -20.0, 0.0),
        )
        .unwrap();
        assert!(solved.knee.is_finite() && solved.end.is_finite());
        assert!(solved.end.length() < 2.0);
    }

    #[test]
    fn straight_chain_uses_rig_bind_space_knee_pole() {
        let solved = solve_two_bone(
            TwoBoneChain::new(
                Vec3::ZERO,
                Vec3::NEG_Y,
                Vec3::NEG_Y * 2.0,
                1.0,
                1.0,
                Vec3::Z,
            ),
            Vec3::new(0.0, -1.8, 0.0),
        )
        .unwrap();
        assert!(solved.knee.z > 0.0);
        assert!(solved.knee.is_finite());
    }

    #[test]
    fn stable_pole_overrides_an_authored_knee_in_the_opposite_hemisphere() {
        let solved = solve_two_bone(
            TwoBoneChain::new(
                Vec3::ZERO,
                Vec3::new(0.0, -1.0, 0.1),
                Vec3::NEG_Y * 2.0,
                1.0,
                1.0,
                Vec3::NEG_Z,
            ),
            Vec3::new(0.0, -1.8, 0.0),
        )
        .unwrap();
        assert!(solved.knee.z < 0.0);
    }

    #[test]
    fn authored_knee_bend_is_preserved_within_the_stable_pole_hemisphere() {
        let solved = solve_two_bone(
            TwoBoneChain::new(
                Vec3::ZERO,
                Vec3::new(0.1, -1.0, 0.1),
                Vec3::NEG_Y * 2.0,
                1.0,
                1.0,
                Vec3::Z,
            ),
            Vec3::new(0.0, -1.8, 0.0),
        )
        .unwrap();
        assert!(solved.knee.x > 0.0);
        assert!(solved.knee.z > 0.0);
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
            TwoBoneChain::new(
                Vec3::ZERO,
                Vec3::NEG_Y,
                Vec3::NEG_Y * 2.0,
                1.0,
                1.0,
                Vec3::NEG_Z,
            ),
            Vec3::new(0.45, -1.75, 0.0),
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
    fn leg_solver_keeps_minimum_flexion_and_anatomical_hemisphere() {
        let pole = canonical_knee_pole(-1.0);
        let solved = solve_two_bone(
            TwoBoneChain::new(
                Vec3::ZERO,
                Vec3::new(0.0, -1.0, -0.1),
                Vec3::NEG_Y * 2.0,
                1.0,
                1.0,
                pole,
            ),
            Vec3::NEG_Y * 20.0,
        )
        .unwrap();
        assert!(solved.end.length() <= maximum_reach(1.0, 1.0) + 0.0001);
        let bend = (solved.knee)
            .reject_from_normalized(solved.end_direction)
            .normalize();
        assert!(bend.dot(pole) > 0.0);
    }
}
