use super::*;

pub(in crate::animation::procedural) const FOOT_FOLLOWER_MAXIMUM_ACCELERATION: f32 = 72.0;
pub(in crate::animation::procedural) const FOOT_FOLLOWER_MAXIMUM_JERK: f32 = 1152.0;
pub(in crate::animation::procedural) const PELVIS_FOLLOWER_MAXIMUM_ACCELERATION: f32 = 12.0;
pub(in crate::animation::procedural) const PELVIS_FOLLOWER_MAXIMUM_JERK: f32 = 192.0;

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

    pub fn contains_warning_at(self, point: Vec3, seconds: f32, delta_seconds: f32) -> bool {
        if !point.is_finite()
            || !seconds.is_finite()
            || seconds < 0.0
            || !delta_seconds.is_finite()
            || delta_seconds <= f32::EPSILON
        {
            return false;
        }
        let root_velocity = (self.next_root - self.current_root) / delta_seconds;
        let predicted_root = self.current_root + root_velocity * seconds;
        point.distance(predicted_root) <= self.warning_reach + 0.0001
    }

    pub const fn current_root(self) -> Vec3 {
        self.current_root
    }

    pub const fn next_root(self) -> Vec3 {
        self.next_root
    }

    pub const fn warning_reach(self) -> f32 {
        self.warning_reach
    }

    pub const fn hard_reach(self) -> f32 {
        self.hard_reach
    }
}

/// A fixed world-space contact selected by a semantic footstep planner. The
/// constructor admits only a previously geometry-conformed endpoint that is
/// already inside the predicted warning-reach tube; it never mutates X/Z or
/// terrain height after the semantic planner has validated them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct FeasibleFootEndpoint(Vec3);

/// A world-space endpoint used only to retire IK ownership safely. Unlike a
/// contact endpoint it makes no terrain-contact claim; it is admitted solely
/// against the predicted hard-reach tube.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct FeasibleReleaseEndpoint(Vec3);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct PredictedHipTrajectory {
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    warning_reach: f32,
    hard_reach: f32,
    uncertainty_radius: f32,
    /// Configured growth of the presentation-space prediction reserve. This
    /// bounds the planner's admitted tube; it is not a claim that arbitrary
    /// future controller or authored hip motion can be forecast exactly.
    uncertainty_speed: f32,
}

impl PredictedHipTrajectory {
    pub fn from_retained_motion(
        reach: FootReachEnvelope,
        previous_position: Option<Vec3>,
        previous_velocity: Vec3,
        delta_seconds: f32,
        uncertainty_radius: f32,
        uncertainty_speed: f32,
    ) -> Option<Self> {
        if !delta_seconds.is_finite()
            || delta_seconds <= f32::EPSILON
            || !previous_velocity.is_finite()
            || !uncertainty_radius.is_finite()
            || uncertainty_radius < 0.0
            || !uncertainty_speed.is_finite()
            || uncertainty_speed < 0.0
        {
            return None;
        }
        let retained_position = previous_position.filter(|position| position.is_finite());
        let velocity = retained_position
            .map(|position| (reach.current_root - position) / delta_seconds)
            .unwrap_or((reach.next_root - reach.current_root) / delta_seconds);
        let acceleration = retained_position
            .map(|_| (velocity - previous_velocity) / delta_seconds)
            .unwrap_or(Vec3::ZERO);
        (velocity.is_finite() && acceleration.is_finite()).then_some(Self {
            position: reach.current_root,
            velocity,
            acceleration,
            warning_reach: reach.warning_reach,
            hard_reach: reach.hard_reach,
            uncertainty_radius,
            uncertainty_speed,
        })
    }

    fn center_at(self, seconds: f32) -> Vec3 {
        self.position + self.velocity * seconds + self.acceleration * (0.5 * seconds.powi(2))
    }

    fn uncertainty_at(self, seconds: f32) -> f32 {
        self.uncertainty_radius + self.uncertainty_speed * seconds.max(0.0)
    }

    pub fn contains_warning_at(self, point: Vec3, seconds: f32) -> bool {
        point.is_finite()
            && seconds.is_finite()
            && seconds >= 0.0
            && point.distance(self.center_at(seconds)) + self.uncertainty_at(seconds)
                <= self.warning_reach + 0.0001
    }

    pub fn contains_hard_at(self, point: Vec3, seconds: f32) -> bool {
        point.is_finite()
            && seconds.is_finite()
            && seconds >= 0.0
            && point.distance(self.center_at(seconds)) + self.uncertainty_at(seconds)
                <= self.hard_reach + 0.0001
    }

    pub fn recovery_target_at(self, presented: Vec3, seconds: f32) -> Option<Vec3> {
        if !presented.is_finite() || !seconds.is_finite() || seconds < 0.0 {
            return None;
        }
        let center = self.center_at(seconds);
        let vertical_budget = (self.hard_reach - self.uncertainty_at(seconds)).max(0.0);
        Some(Vec3::new(
            center.x,
            presented
                .y
                .clamp(center.y - vertical_budget, center.y + vertical_budget),
            center.z,
        ))
    }

    /// Proves a fixed quintic path stays inside the configured swept reach
    /// tube. Runtime hard-reach ownership remains the final guard against a
    /// controller trajectory that departs from this retained-motion estimate.
    /// The hip quadratic is degree-elevated to quintic; the relative curve is
    /// then bounded by the convex hull of its Bernstein control points.
    pub fn contains_quintic_path(
        self,
        foot_control: [Vec3; 6],
        duration_seconds: f32,
        hard_limit: bool,
    ) -> bool {
        if !duration_seconds.is_finite()
            || duration_seconds < 0.0
            || foot_control.iter().any(|point| !point.is_finite())
        {
            return false;
        }
        let duration = duration_seconds;
        let hip_quadratic = [
            self.position,
            self.position + self.velocity * (duration * 0.5),
            self.center_at(duration),
        ];
        let hip_control = elevate_quadratic_to_quintic(hip_quadratic);
        let reach = if hard_limit {
            self.hard_reach
        } else {
            self.warning_reach
        };
        let available = reach - self.uncertainty_at(duration);
        available >= 0.0
            && foot_control
                .into_iter()
                .zip(hip_control)
                .all(|(foot, hip)| foot.distance(hip) <= available + 0.0001)
    }
}

fn elevate_quadratic_to_quintic(control: [Vec3; 3]) -> [Vec3; 6] {
    // Degree elevation preserves the curve exactly. These coefficients are
    // C(2,i) C(3,k-i) / C(5,k) for k=0..5.
    [
        control[0],
        control[0] * 0.6 + control[1] * 0.4,
        control[0] * 0.3 + control[1] * 0.6 + control[2] * 0.1,
        control[0] * 0.1 + control[1] * 0.6 + control[2] * 0.3,
        control[1] * 0.4 + control[2] * 0.6,
        control[2],
    ]
}

impl FeasibleReleaseEndpoint {
    pub(super) const fn from_proven_guard_release(requested: Vec3) -> Self {
        Self(requested)
    }

    pub fn for_predicted_release(
        requested: Vec3,
        trajectory: PredictedHipTrajectory,
        release_seconds: f32,
    ) -> Option<Self> {
        trajectory
            .contains_hard_at(requested, release_seconds)
            .then_some(Self(requested))
    }

    pub const fn position(self) -> Vec3 {
        self.0
    }
}

impl FeasibleFootEndpoint {
    /// Constructs a contact whose terrain/corridor geometry and complete
    /// guard-specific hip path were proven by the owning planner. Keeping this
    /// constructor private to the procedural IK module prevents callers from
    /// bypassing either proof with an arbitrary world point.
    pub(super) const fn from_proven_guard_contact(requested: Vec3) -> Self {
        Self(requested)
    }

    pub fn for_predicted_terrain_contact(
        requested: Vec3,
        trajectory: PredictedHipTrajectory,
        contact_seconds: f32,
    ) -> Option<Self> {
        if !requested.is_finite() || !contact_seconds.is_finite() || contact_seconds < 0.0 {
            return None;
        }
        trajectory
            .contains_warning_at(requested, contact_seconds)
            .then_some(Self(requested))
    }

    pub const fn position(self) -> Vec3 {
        self.0
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
    let current_hard_invalid = limits
        .reach
        .is_some_and(|reach| current.position.distance(reach.current_root) > reach.hard_reach);
    let reach_braking = limits.reach.and_then(|reach| {
        let radial = (current.position - reach.next_root).normalize_or_zero();
        let root_velocity = (reach.next_root - reach.current_root) / dt;
        let outward_velocity = (current.velocity - root_velocity).dot(radial);
        let outward_acceleration = current.acceleration.dot(radial);
        let stopping_distance = jerk_limited_stopping_distance(
            outward_velocity,
            outward_acceleration,
            limits.maximum_acceleration,
            limits.maximum_jerk,
        );
        (current.position.distance(reach.next_root) + stopping_distance >= reach.warning_reach)
            .then_some(radial)
    });
    let tracking_acceleration = ideal.acceleration
        + control_position_error * frequency.powi(2)
        + control_velocity_error * damping;
    let requested_acceleration = if current_hard_invalid {
        let reach = limits.reach.expect("hard reach check requires an envelope");
        let outward = (current.position - reach.next_root).normalize_or_zero();
        let root_velocity = (reach.next_root - reach.current_root) / dt;
        let relative_outward_speed = (current.velocity - root_velocity).dot(outward);
        let recovery_distance =
            (current.position.distance(reach.next_root) - reach.warning_reach).max(0.0);
        let inward_speed = (2.0 * limits.maximum_acceleration.max(0.0) * recovery_distance).sqrt();
        let desired_outward_speed = -inward_speed;
        outward
            * ((desired_outward_speed - relative_outward_speed) / dt)
                .clamp(-limits.maximum_acceleration, limits.maximum_acceleration)
    } else if let Some(radial) = reach_braking {
        // Begin radial braking while the current p/v/a can still stop inside
        // warning reach. Preserve tangential tracking so a grounded shuffle
        // can keep moving around the hip without trading its flexion reserve
        // for an avoidable downstream hard clamp.
        tracking_acceleration.reject_from_normalized(radial) - radial * limits.maximum_acceleration
    } else {
        tracking_acceleration
    };
    let requested_jerk = (requested_acceleration - current.acceleration) / dt;
    let jerk = requested_jerk.clamp_length_max(limits.maximum_jerk.max(0.0));
    let acceleration =
        (current.acceleration + jerk * dt).clamp_length_max(limits.maximum_acceleration.max(0.0));
    let mut velocity = current.velocity + acceleration * dt;
    let mut acceleration = acceleration;
    let mut position = current.position + velocity * dt;
    let mut reach_constrained = false;
    if let Some(reach) = limits.reach {
        let offset = position - reach.next_root;
        let distance = offset.length();
        if distance > reach.warning_reach {
            reach_constrained = true;
            // Reach is a unilateral kinematic constraint, not merely a reason
            // to ask the semantic owner for a different target next frame.
            // Project the complete presented state onto its tangent space so
            // the downstream two-bone solver never has to snap position while
            // retaining impossible outward derivatives. This is deliberately
            // morphology-relative and remains valid when limb scale or gait
            // speed changes.
            let radial = offset / distance;
            position = reach.next_root + radial * reach.warning_reach;
            let root_velocity = (reach.next_root - reach.current_root) / dt;
            let relative_velocity = velocity - root_velocity;
            velocity -= radial * relative_velocity.dot(radial).max(0.0);
            acceleration -= radial * acceleration.dot(radial).max(0.0);
        }
    }
    let state = FootFollowerState {
        position,
        velocity,
        acceleration,
        previous_ideal: ideal.position,
        previous_ideal_velocity: ideal.velocity,
        previous_ideal_acceleration: ideal.acceleration,
    };
    let pose_error = position.distance(ideal.position);
    if current_hard_invalid {
        let reach = limits.reach.expect("hard reach check requires an envelope");
        return FootFollowOutcome::NeedsReleaseOrReplan {
            presented_state: state,
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
        } else if reach_constrained || candidate_reach > reach.warning_reach {
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
    FootFollowOutcome::NeedsReleaseOrReplan {
        // Never encode reach safety as a frozen position with retained
        // derivatives. The semantic owner consumes the replan reason while
        // this state continues the jerk-bounded trajectory; on the next tick
        // an already-hard state enters the explicit inward recovery law above.
        presented_state: state,
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

/// Distance travelled along a reach radius while applying the fastest lawful
/// braking profile: maximum inward jerk until maximum inward acceleration,
/// then constant inward acceleration. This is the admission test used before
/// transferring a moving presentation target to a new semantic owner.
pub(in crate::animation::procedural) fn jerk_limited_stopping_distance(
    outward_velocity: f32,
    outward_acceleration: f32,
    maximum_acceleration: f32,
    maximum_jerk: f32,
) -> f32 {
    if !outward_velocity.is_finite()
        || !outward_acceleration.is_finite()
        || !maximum_acceleration.is_finite()
        || !maximum_jerk.is_finite()
        || maximum_acceleration <= 0.0
        || maximum_jerk < 0.0
    {
        return f32::INFINITY;
    }
    let velocity = outward_velocity.max(0.0);
    if velocity <= f32::EPSILON {
        return 0.0;
    }
    let acceleration = outward_acceleration.clamp(-maximum_acceleration, maximum_acceleration);
    if maximum_jerk <= f32::EPSILON {
        return if acceleration < 0.0 {
            velocity * velocity / (-2.0 * acceleration)
        } else {
            f32::INFINITY
        };
    }

    // v(t) = v0 + a0*t - j*t^2/2. If it reaches zero before the
    // acceleration cap, integrate only to that root.
    let stop_during_ramp = (acceleration
        + (acceleration * acceleration + 2.0 * maximum_jerk * velocity).sqrt())
        / maximum_jerk;
    let ramp_to_cap = (acceleration + maximum_acceleration) / maximum_jerk;
    let ramp_seconds = stop_during_ramp.min(ramp_to_cap.max(0.0));
    let ramp_distance = velocity * ramp_seconds + 0.5 * acceleration * ramp_seconds.powi(2)
        - maximum_jerk * ramp_seconds.powi(3) / 6.0;
    if stop_during_ramp <= ramp_to_cap {
        return ramp_distance.max(0.0);
    }
    let velocity_after_ramp = (velocity + acceleration * ramp_seconds
        - 0.5 * maximum_jerk * ramp_seconds.powi(2))
    .max(0.0);
    (ramp_distance + velocity_after_ramp.powi(2) / (2.0 * maximum_acceleration)).max(0.0)
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(in crate::animation::procedural) struct PelvisFollowerState {
    pub position: f32,
    pub velocity: f32,
    pub acceleration: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::animation::procedural) struct PelvisRecoverySegment {
    start: PelvisFollowerState,
    end: f32,
    elapsed_ticks: u32,
    total_ticks: u32,
    fixed_delta_seconds: f32,
}

impl PelvisRecoverySegment {
    pub(super) fn progress(self) -> f32 {
        self.elapsed_ticks.min(self.total_ticks) as f32 / self.total_ticks.max(1) as f32
    }

    fn sample(self) -> PelvisFollowerState {
        let progress = self.progress();
        pelvis_boundary_quintic_sample(
            self.start,
            self.end,
            progress,
            self.total_ticks as f32 * self.fixed_delta_seconds,
        )
    }

    fn advance(&mut self) {
        self.elapsed_ticks = self.elapsed_ticks.saturating_add(1).min(self.total_ticks);
    }

    fn is_complete(self) -> bool {
        self.elapsed_ticks >= self.total_ticks
    }
}

fn pelvis_boundary_quintic_sample(
    start: PelvisFollowerState,
    end: f32,
    progress: f32,
    duration_seconds: f32,
) -> PelvisFollowerState {
    let progress = progress.clamp(0.0, 1.0);
    let duration = duration_seconds.max(f32::EPSILON);
    let velocity_term = start.velocity * duration;
    let acceleration_term = start.acceleration * duration.powi(2);
    let residual = end - start.position - velocity_term - acceleration_term * 0.5;
    let final_velocity_residual = -velocity_term - acceleration_term;
    let final_acceleration_residual = -acceleration_term;
    let c3 = residual * 10.0 - final_velocity_residual * 4.0 + final_acceleration_residual * 0.5;
    let c4 = residual * -15.0 + final_velocity_residual * 7.0 - final_acceleration_residual;
    let c5 = residual * 6.0 - final_velocity_residual * 3.0 + final_acceleration_residual * 0.5;
    PelvisFollowerState {
        position: start.position
            + velocity_term * progress
            + acceleration_term * (0.5 * progress.powi(2))
            + c3 * progress.powi(3)
            + c4 * progress.powi(4)
            + c5 * progress.powi(5),
        velocity: (velocity_term
            + acceleration_term * progress
            + c3 * (3.0 * progress.powi(2))
            + c4 * (4.0 * progress.powi(3))
            + c5 * (5.0 * progress.powi(4)))
            / duration,
        acceleration: (acceleration_term
            + c3 * (6.0 * progress)
            + c4 * (12.0 * progress.powi(2))
            + c5 * (20.0 * progress.powi(3)))
            / duration.powi(2),
    }
}

fn plan_pelvis_recovery(
    start: PelvisFollowerState,
    end: f32,
    fixed_delta_seconds: f32,
) -> Option<PelvisRecoverySegment> {
    if !fixed_delta_seconds.is_finite()
        || fixed_delta_seconds <= f32::EPSILON
        || !start.position.is_finite()
        || !start.velocity.is_finite()
        || !start.acceleration.is_finite()
        || !end.is_finite()
    {
        return None;
    }
    for total_ticks in 1..=1024 {
        let duration = total_ticks as f32 * fixed_delta_seconds;
        let velocity_term = start.velocity * duration;
        let acceleration_term = start.acceleration * duration.powi(2);
        let residual = end - start.position - velocity_term - acceleration_term * 0.5;
        let final_velocity_residual = -velocity_term - acceleration_term;
        let final_acceleration_residual = -acceleration_term;
        let c3 =
            residual * 10.0 - final_velocity_residual * 4.0 + final_acceleration_residual * 0.5;
        let c4 = residual * -15.0 + final_velocity_residual * 7.0 - final_acceleration_residual;
        let c5 = residual * 6.0 - final_velocity_residual * 3.0 + final_acceleration_residual * 0.5;
        let position_controls = [
            start.position,
            start.position + velocity_term / 5.0,
            start.position + velocity_term * 0.4 + acceleration_term / 20.0,
            end,
            end,
            end,
        ];
        let minimum_position = start.position.min(end) - 0.000001;
        let maximum_position = start.position.max(end) + 0.000001;
        if position_controls
            .iter()
            .any(|position| *position < minimum_position || *position > maximum_position)
        {
            continue;
        }
        let acceleration_power = [
            acceleration_term / duration.powi(2),
            6.0 * c3 / duration.powi(2),
            12.0 * c4 / duration.powi(2),
            20.0 * c5 / duration.powi(2),
        ];
        let acceleration_controls = [
            acceleration_power[0],
            acceleration_power[0] + acceleration_power[1] / 3.0,
            acceleration_power[0]
                + acceleration_power[1] * (2.0 / 3.0)
                + acceleration_power[2] / 3.0,
            acceleration_power.iter().sum(),
        ];
        let jerk_power = [
            acceleration_power[1] / duration,
            2.0 * acceleration_power[2] / duration,
            3.0 * acceleration_power[3] / duration,
        ];
        let jerk_controls = [
            jerk_power[0],
            jerk_power[0] + jerk_power[1] * 0.5,
            jerk_power.iter().sum(),
        ];
        if acceleration_controls
            .iter()
            .all(|value| value.abs() <= PELVIS_FOLLOWER_MAXIMUM_ACCELERATION + 0.0001)
            && jerk_controls
                .iter()
                .all(|value| value.abs() <= PELVIS_FOLLOWER_MAXIMUM_JERK + 0.001)
        {
            return Some(PelvisRecoverySegment {
                start,
                end,
                elapsed_ticks: 0,
                total_ticks,
                fixed_delta_seconds,
            });
        }
    }
    None
}

pub(in crate::animation::procedural) fn advance_pelvis_follower_with_recovery(
    current: PelvisFollowerState,
    recovery: &mut Option<PelvisRecoverySegment>,
    desired: f32,
    delta_seconds: f32,
) -> PelvisFollowerState {
    if let Some(segment) = *recovery {
        let admitted_direction = (segment.end - segment.start.position).signum();
        let requested_direction = (desired - segment.end).signum();
        let extends_admitted_motion = admitted_direction != 0.0
            && requested_direction == admitted_direction
            && (desired - segment.end).abs() > 0.000001;
        if extends_admitted_motion
            && let Some(replanned) = plan_pelvis_recovery(current, desired, delta_seconds)
        {
            *recovery = Some(replanned);
        }
    }
    if recovery.is_none() && (desired - current.position).abs() > 0.000001 {
        *recovery = plan_pelvis_recovery(current, desired, delta_seconds);
    }
    if let Some(segment) = recovery.as_mut() {
        segment.advance();
        let mut sample = segment.sample();
        if segment.is_complete() {
            sample = PelvisFollowerState {
                position: segment.end,
                velocity: 0.0,
                acceleration: 0.0,
            };
            *recovery = None;
        }
        return sample;
    }
    advance_pelvis_follower(current, desired, delta_seconds)
}

pub(in crate::animation::procedural) fn advance_pelvis_follower(
    current: PelvisFollowerState,
    desired: f32,
    delta_seconds: f32,
) -> PelvisFollowerState {
    if !desired.is_finite()
        || !delta_seconds.is_finite()
        || delta_seconds <= f32::EPSILON
        || !current.position.is_finite()
        || !current.velocity.is_finite()
        || !current.acceleration.is_finite()
    {
        return current;
    }
    let frequency = 10.0;
    let error = desired - current.position;
    let braking_distance = current.velocity.max(0.0).powi(2)
        / (2.0 * PELVIS_FOLLOWER_MAXIMUM_ACCELERATION)
        + current.velocity.max(0.0) * current.acceleration.abs() / PELVIS_FOLLOWER_MAXIMUM_JERK;
    let requested_acceleration = if error >= 0.0 && current.velocity < 0.0 {
        PELVIS_FOLLOWER_MAXIMUM_ACCELERATION
    } else if error >= 0.0 && current.velocity > 0.000001 && braking_distance >= error.max(0.0) {
        -PELVIS_FOLLOWER_MAXIMUM_ACCELERATION
    } else {
        error * frequency * frequency - current.velocity * (2.0 * frequency)
    };
    let requested_jerk = (requested_acceleration - current.acceleration) / delta_seconds;
    let jerk = requested_jerk.clamp(-PELVIS_FOLLOWER_MAXIMUM_JERK, PELVIS_FOLLOWER_MAXIMUM_JERK);
    let acceleration = (current.acceleration + jerk * delta_seconds).clamp(
        -PELVIS_FOLLOWER_MAXIMUM_ACCELERATION,
        PELVIS_FOLLOWER_MAXIMUM_ACCELERATION,
    );
    let velocity = current.velocity + acceleration * delta_seconds;
    let next = PelvisFollowerState {
        position: current.position + velocity * delta_seconds,
        velocity,
        acceleration,
    };
    if (desired - next.position).abs() <= 0.000001
        && next.velocity.abs() <= 0.00001
        && next.acceleration.abs() <= 0.0001
    {
        return PelvisFollowerState {
            position: desired,
            velocity: 0.0,
            acceleration: 0.0,
        };
    }
    next
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
