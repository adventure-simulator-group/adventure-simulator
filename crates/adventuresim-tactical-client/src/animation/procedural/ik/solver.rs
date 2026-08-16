use super::*;

pub(in crate::animation::procedural) const FOOT_FOLLOWER_MAXIMUM_ACCELERATION: f32 = 72.0;
pub(in crate::animation::procedural) const FOOT_FOLLOWER_MAXIMUM_JERK: f32 = 1152.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct WorldFootTargetSample {
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
}

impl WorldFootTargetSample {
    pub fn new(position: Vec3, velocity: Vec3, acceleration: Vec3) -> Option<Self> {
        (position.is_finite() && velocity.is_finite() && acceleration.is_finite()).then_some(Self {
            position,
            velocity,
            acceleration,
        })
    }

    pub const fn position(self) -> Vec3 {
        self.position
    }

    pub const fn velocity(self) -> Vec3 {
        self.velocity
    }

    pub const fn acceleration(self) -> Vec3 {
        self.acceleration
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) enum IdealFootTarget {
    WorldPlant { position: Vec3 },
    WorldSwing(WorldFootTargetSample),
}

impl IdealFootTarget {
    pub fn world_plant(position: Vec3) -> Option<Self> {
        position
            .is_finite()
            .then_some(Self::WorldPlant { position })
    }

    pub fn world(self) -> WorldFootTargetSample {
        match self {
            Self::WorldPlant { position } => WorldFootTargetSample {
                position,
                velocity: Vec3::ZERO,
                acceleration: Vec3::ZERO,
            },
            Self::WorldSwing(sample) => sample,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct FootFollowerState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub acceleration: Vec3,
    pub previous_ideal: Vec3,
    pub previous_ideal_velocity: Vec3,
    pub previous_ideal_acceleration: Vec3,
}

impl FootFollowerState {
    pub fn from_presented_pose(
        position: Vec3,
        velocity: Vec3,
        acceleration: Vec3,
        previous_ideal: Vec3,
        previous_ideal_velocity: Vec3,
        previous_ideal_acceleration: Vec3,
    ) -> Option<Self> {
        (position.is_finite()
            && velocity.is_finite()
            && acceleration.is_finite()
            && previous_ideal.is_finite()
            && previous_ideal_velocity.is_finite()
            && previous_ideal_acceleration.is_finite())
        .then_some(Self {
            position,
            velocity,
            acceleration,
            previous_ideal,
            previous_ideal_velocity,
            previous_ideal_acceleration,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct FootReachEnvelope {
    current_root: Vec3,
    next_root: Vec3,
    warning_reach: f32,
    hard_reach: f32,
}

impl FootReachEnvelope {
    pub fn new(
        current_root: Vec3,
        next_root: Vec3,
        warning_reach: f32,
        hard_reach: f32,
    ) -> Option<Self> {
        (current_root.is_finite()
            && next_root.is_finite()
            && warning_reach.is_finite()
            && hard_reach.is_finite()
            && warning_reach > 0.0
            && hard_reach >= warning_reach)
            .then_some(Self {
                current_root,
                next_root,
                warning_reach,
                hard_reach,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct MaximumPoseError(f32);

impl MaximumPoseError {
    pub fn new(metres: f32) -> Option<Self> {
        (metres.is_finite() && metres >= 0.0).then_some(Self(metres))
    }

    pub const fn metres(self) -> f32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct FootFollowerLimits {
    nominal_error: MaximumPoseError,
    replan_error: MaximumPoseError,
    maximum_acceleration: f32,
    maximum_jerk: f32,
    contact_deadline_seconds: Option<f32>,
    reach: Option<FootReachEnvelope>,
}

impl FootFollowerLimits {
    pub fn animation(
        reach: Option<FootReachEnvelope>,
        contact_deadline_seconds: Option<f32>,
    ) -> Self {
        Self {
            nominal_error: MaximumPoseError(0.025),
            replan_error: MaximumPoseError(0.05),
            maximum_acceleration: FOOT_FOLLOWER_MAXIMUM_ACCELERATION,
            maximum_jerk: FOOT_FOLLOWER_MAXIMUM_JERK,
            contact_deadline_seconds: contact_deadline_seconds
                .filter(|seconds| seconds.is_finite() && *seconds > 0.0),
            reach,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::animation::procedural) enum FootFollowReason {
    PoseError,
    ReachWarning,
    ReachHardLimit,
    ContactDeadline,
    DiscontinuousTarget,
    InvalidInput,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) enum FootFollowOutcome {
    Tracking(FootFollowerState),
    NeedsReleaseOrReplan {
        presented_state: FootFollowerState,
        reason: FootFollowReason,
        suggested_semantic_target: Vec3,
    },
    Invalid(FootFollowReason),
}

impl FootFollowOutcome {
    pub fn presented_state(self) -> Option<FootFollowerState> {
        match self {
            Self::Tracking(state) => Some(state),
            Self::NeedsReleaseOrReplan {
                presented_state, ..
            } => Some(presented_state),
            Self::Invalid(_) => None,
        }
    }
}

pub(in crate::animation::procedural) fn advance_foot_follower(
    current: FootFollowerState,
    ideal: IdealFootTarget,
    limits: FootFollowerLimits,
    delta_seconds: f32,
) -> FootFollowOutcome {
    if !delta_seconds.is_finite()
        || delta_seconds <= f32::EPSILON
        || !current.position.is_finite()
        || !current.velocity.is_finite()
        || !current.acceleration.is_finite()
    {
        return FootFollowOutcome::Invalid(FootFollowReason::InvalidInput);
    }
    let dt = delta_seconds;
    let ideal = ideal.world();
    let expected_ideal = current.previous_ideal
        + current.previous_ideal_velocity * dt
        + current.previous_ideal_acceleration * (0.5 * dt * dt);
    if ideal.position.distance(expected_ideal) > limits.replan_error.metres() {
        return FootFollowOutcome::NeedsReleaseOrReplan {
            presented_state: current,
            reason: FootFollowReason::DiscontinuousTarget,
            suggested_semantic_target: expected_ideal
                + (ideal.position - expected_ideal).clamp_length_max(limits.replan_error.metres()),
        };
    }
    let error = ideal.position - current.position;
    let nominal = limits.nominal_error.metres().max(0.001);
    let replan = limits.replan_error.metres().max(nominal);
    let pose_demand = smoothstep(nominal, replan, error.length());
    let predicted_position =
        current.position + current.velocity * dt + current.acceleration * (0.5 * dt * dt);
    let reach_demand = limits.reach.map_or(0.0, |reach| {
        smoothstep(
            reach.warning_reach,
            reach.hard_reach,
            predicted_position.distance(reach.next_root),
        )
    });
    let deadline_demand = limits.contact_deadline_seconds.map_or(0.0, |deadline| {
        let direction = error.normalize_or_zero();
        let closing_speed = (current.velocity - ideal.velocity).dot(direction).max(0.0);
        let along_acceleration = current.acceleration.dot(direction).max(0.0);
        let available_distance = (closing_speed * deadline
            + jerk_limited_distance(
                along_acceleration,
                limits.maximum_acceleration,
                limits.maximum_jerk,
                deadline,
            ))
        .max(0.0);
        let required_distance = (error.length() - nominal).max(0.0);
        smoothstep(0.75, 1.0, required_distance / available_distance.max(0.001))
    });
    let demand = pose_demand.max(reach_demand).max(deadline_demand);
    let frequency = 8.0_f32.lerp(20.0, demand);
    // Slightly overdamped at the discrete presentation rates so catch-up
    // velocity is shed before crossing a moving semantic target.
    let damping = 2.5 * frequency;
    // Compare the sample against where the retained state would land after
    // applying only the generator's feed-forward acceleration. This keeps an
    // exactly-followed ramp an invariant of the semi-implicit integrator;
    // using the pre-step position/velocity as PD errors double-counted the
    // target's lawful motion and made the follower cross slow ramps.
    let feed_forward_velocity = current.velocity + ideal.acceleration * dt;
    let feed_forward_position = current.position + feed_forward_velocity * dt;
    let control_position_error = ideal.position - feed_forward_position;
    let control_velocity_error = ideal.velocity - feed_forward_velocity;
    let requested_acceleration = ideal.acceleration
        + control_position_error * frequency.powi(2)
        + control_velocity_error * damping;
    let requested_jerk = (requested_acceleration - current.acceleration) / dt;
    let jerk = requested_jerk.clamp_length_max(limits.maximum_jerk.max(0.0));
    let acceleration =
        (current.acceleration + jerk * dt).clamp_length_max(limits.maximum_acceleration.max(0.0));
    let velocity = current.velocity + acceleration * dt;
    let position = current.position + velocity * dt;
    let state = FootFollowerState {
        position,
        velocity,
        acceleration,
        previous_ideal: ideal.position,
        previous_ideal_velocity: ideal.velocity,
        previous_ideal_acceleration: ideal.acceleration,
    };
    let pose_error = position.distance(ideal.position);
    let current_hard_invalid = limits
        .reach
        .is_some_and(|reach| current.position.distance(reach.current_root) > reach.hard_reach);
    if current_hard_invalid {
        let reach = limits.reach.expect("hard reach check requires an envelope");
        return FootFollowOutcome::NeedsReleaseOrReplan {
            presented_state: current,
            reason: FootFollowReason::ReachHardLimit,
            suggested_semantic_target: constrain_target_to_reach(
                current.position,
                reach.next_root,
                reach.warning_reach,
            ),
        };
    }
    let reach_reason = limits.reach.and_then(|reach| {
        let candidate_reach = position.distance(reach.next_root);
        if candidate_reach > reach.hard_reach {
            Some(FootFollowReason::ReachHardLimit)
        } else if candidate_reach > reach.warning_reach {
            Some(FootFollowReason::ReachWarning)
        } else {
            None
        }
    });
    let deadline_missed = limits.contact_deadline_seconds.is_some_and(|deadline| {
        let direction = error.normalize_or_zero();
        let closing_speed = (current.velocity - ideal.velocity).dot(direction).max(0.0);
        let along_acceleration = current.acceleration.dot(direction).max(0.0);
        let available_distance = (closing_speed * deadline
            + jerk_limited_distance(
                along_acceleration,
                limits.maximum_acceleration,
                limits.maximum_jerk,
                deadline,
            ))
        .max(0.0);
        (error.length() - nominal).max(0.0) > available_distance
    });
    let reason = reach_reason.or_else(|| {
        deadline_missed
            .then_some(FootFollowReason::ContactDeadline)
            .or((pose_error > replan).then_some(FootFollowReason::PoseError))
    });
    let Some(reason) = reason else {
        return FootFollowOutcome::Tracking(state);
    };
    let mut suggested_semantic_target = current.position
        + (ideal.position - current.position).clamp_length_max(limits.replan_error.metres());
    if let Some(reach) = limits.reach {
        suggested_semantic_target = constrain_target_to_reach(
            suggested_semantic_target,
            reach.next_root,
            reach.warning_reach,
        );
    }
    let presented_state = if reason == FootFollowReason::ReachHardLimit {
        current
    } else {
        state
    };
    FootFollowOutcome::NeedsReleaseOrReplan {
        presented_state,
        reason,
        suggested_semantic_target,
    }
}

fn jerk_limited_distance(
    initial_acceleration: f32,
    maximum_acceleration: f32,
    maximum_jerk: f32,
    duration: f32,
) -> f32 {
    let duration = duration.max(0.0);
    let maximum_acceleration = maximum_acceleration.max(0.0);
    let initial_acceleration =
        initial_acceleration.clamp(-maximum_acceleration, maximum_acceleration);
    let maximum_jerk = maximum_jerk.max(0.0);
    if maximum_jerk <= f32::EPSILON {
        return 0.5 * initial_acceleration * duration * duration;
    }
    let ramp_seconds =
        ((maximum_acceleration - initial_acceleration).max(0.0) / maximum_jerk).min(duration);
    let ramp_distance = 0.5 * initial_acceleration * ramp_seconds * ramp_seconds
        + maximum_jerk * ramp_seconds.powi(3) / 6.0;
    let ramp_velocity =
        initial_acceleration * ramp_seconds + 0.5 * maximum_jerk * ramp_seconds * ramp_seconds;
    let cruise_seconds = duration - ramp_seconds;
    ramp_distance
        + ramp_velocity * cruise_seconds
        + 0.5 * maximum_acceleration * cruise_seconds * cruise_seconds
}

pub(in crate::animation::procedural) fn plant_is_continuous(
    plant: Vec3,
    current_foot: Vec3,
) -> bool {
    plant.is_finite()
        && current_foot.is_finite()
        && plant.distance(current_foot) <= MAX_PLANT_DISCONTINUITY
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
        // Landing supplies a foot-facing-constrained leg pole. Reblending the
        // authored knee here would occur after that constraint and could
        // recreate an anatomically impossible sideways bend.
        false,
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
    apply_two_bone_solution_weighted(upper, lower, end, solution, 1.0, parents, transforms);
}

pub(in crate::animation::procedural) fn apply_two_bone_solution_weighted(
    upper: Entity,
    lower: Entity,
    end: Entity,
    solution: TwoBoneSolution,
    weight: f32,
    parents: &Query<&ChildOf>,
    transforms: &mut ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    let weight = if weight.is_finite() {
        weight.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if weight <= 0.0 {
        return;
    }
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
        transform.rotation = transform.rotation.slerp(rotation, weight).normalize();
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
        transform.rotation = transform.rotation.slerp(rotation, weight).normalize();
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
        transform.rotation = transform
            .rotation
            .slerp(local.normalize(), weight)
            .normalize();
    }
}
