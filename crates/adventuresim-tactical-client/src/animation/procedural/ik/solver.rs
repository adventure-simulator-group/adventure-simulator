use super::*;

pub(in crate::animation::procedural) fn plant_is_continuous(
    plant: Vec3,
    current_foot: Vec3,
) -> bool {
    plant.is_finite()
        && current_foot.is_finite()
        && plant.distance(current_foot) <= MAX_PLANT_DISCONTINUITY
}

pub(in crate::animation::procedural) fn advance_foot_target(
    previous: Option<Vec3>,
    desired: Vec3,
    delta_seconds: f32,
) -> Vec3 {
    let Some(previous) = previous.filter(|position| position.is_finite()) else {
        return desired;
    };
    if !desired.is_finite() {
        return previous;
    }
    if previous.distance(desired) > MAX_PLANT_DISCONTINUITY {
        return desired;
    }
    let maximum_step = (MAX_FOOT_TARGET_SPEED * delta_seconds.max(0.0)).min(MAX_FOOT_TARGET_STEP);
    previous + (desired - previous).clamp_length_max(maximum_step)
}

pub(in crate::animation::procedural) fn advance_pelvis_shift(
    current: f32,
    desired: f32,
    delta_seconds: f32,
) -> f32 {
    let maximum_step =
        (PELVIS_CORRECTION_SPEED * delta_seconds.max(0.0)).min(MAX_PELVIS_CORRECTION_STEP);
    current + (desired - current).clamp(-maximum_step, maximum_step)
}

pub(in crate::animation::procedural) fn maximum_reach(upper_length: f32, lower_length: f32) -> f32 {
    (upper_length * upper_length
        + lower_length * lower_length
        + 2.0 * upper_length * lower_length * MIN_KNEE_FLEXION.cos())
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
        LANDING_KNEE_RESERVE_RELEASE_COMPRESSION,
        LANDING_KNEE_RESERVE_FULL_COMPRESSION,
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

pub(in crate::animation::procedural) fn solve_two_bone(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        root,
        current_knee,
        current_end,
        target,
        upper_length,
        lower_length,
        pole_direction,
        maximum_reach(upper_length, lower_length),
        true,
    )
}

pub(in crate::animation::procedural) fn solve_landing_two_bone(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
    compression: f32,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        root,
        current_knee,
        current_end,
        target,
        upper_length,
        lower_length,
        pole_direction,
        landing_maximum_reach(
            upper_length,
            lower_length,
            root.distance(current_end),
            compression,
        ),
        true,
    )
}

pub(in crate::animation::procedural) fn solve_two_bone_with_reach(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
    maximum_target_reach: f32,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        root,
        current_knee,
        current_end,
        target,
        upper_length,
        lower_length,
        pole_direction,
        maximum_target_reach,
        false,
    )
}

pub(in crate::animation::procedural) fn solve_two_bone_preserving_with_reach(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
    maximum_target_reach: f32,
) -> Option<TwoBoneSolution> {
    solve_two_bone_internal(
        root,
        current_knee,
        current_end,
        target,
        upper_length,
        lower_length,
        pole_direction,
        maximum_target_reach,
        true,
    )
}

fn solve_two_bone_internal(
    root: Vec3,
    current_knee: Vec3,
    current_end: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole_direction: Vec3,
    maximum_target_reach: f32,
    preserve_authored_bend: bool,
) -> Option<TwoBoneSolution> {
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
    let Some((upper_before, lower_before, _)) =
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
}
