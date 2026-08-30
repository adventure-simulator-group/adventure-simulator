use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Posture {
    #[default]
    Upright,
    Airborne,
    Prone,
    Supine,
    Ragdolled,
}

/// Mutually exclusive physical body modes. Ground contact and posture cannot
/// disagree because grounded posture is carried only by `Grounded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum BodyState {
    Grounded(GroundedPosture),
    Airborne,
    Prone,
    Supine,
    Ragdolled,
}

impl Default for BodyState {
    fn default() -> Self {
        Self::Grounded(GroundedPosture::Upright)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum GroundedPosture {
    #[default]
    Upright,
}

/// Presentation-only anticipation for a release-triggered jump. Charging a
/// jump does not change authoritative posture or movement speed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum JumpAnticipation {
    #[default]
    Inactive,
    Charging,
}

impl BodyState {
    pub fn is_grounded(self) -> bool {
        matches!(self, Self::Grounded(_))
    }
    pub fn posture(self) -> Posture {
        match self {
            Self::Grounded(GroundedPosture::Upright) => Posture::Upright,
            Self::Airborne => Posture::Airborne,
            Self::Prone => Posture::Prone,
            Self::Supine => Posture::Supine,
            Self::Ragdolled => Posture::Ragdolled,
        }
    }

    pub fn is_downed(self) -> bool {
        matches!(self, Self::Prone | Self::Supine | Self::Ragdolled)
    }

    pub fn is_surface_supported(self) -> bool {
        matches!(self, Self::Grounded(_) | Self::Prone | Self::Supine)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum LeadFoot {
    #[default]
    Left,
    Right,
}

impl LeadFoot {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum WeaponGuardState {
    #[default]
    Lowered,
    Raised,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum StanceState {
    #[default]
    Lowered,
    Raised {
        locomotion: RaisedLocomotionIntent,
    },
}

/// Compact authoritative input for client-side raised-guard foot placement.
/// The authority replicates only observed motion. Exact visual plants, swing
/// ownership, and progress are client presentation state, as in Overgrowth's
/// velocity-driven foot stance.
/// This invariant-bearing type intentionally does not implement Bevy reflection;
/// reflected field mutation would bypass its validated constructors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RaisedLocomotionIntent(RaisedLocomotionKind);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RaisedLocomotionKind {
    Planted,
    Moving { local_direction: Vec2, speed: f32 },
}

#[derive(Deserialize)]
struct RaisedLocomotionWire(RaisedLocomotionKind);

impl<'de> Deserialize<'de> for RaisedLocomotionIntent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let RaisedLocomotionWire(value) = RaisedLocomotionWire::deserialize(deserializer)?;
        Ok(match value {
            RaisedLocomotionKind::Planted => Self::planted(),
            RaisedLocomotionKind::Moving {
                local_direction,
                speed,
            } => Self::moving(local_direction, speed),
        })
    }
}

impl Default for RaisedLocomotionIntent {
    fn default() -> Self {
        Self::planted()
    }
}

impl RaisedLocomotionIntent {
    pub fn planted() -> Self {
        Self(RaisedLocomotionKind::Planted)
    }

    /// Creates validated moving intent. Invalid or effectively stationary
    /// input becomes planted while retaining its handoff identity.
    pub fn moving(local_direction: Vec2, speed: f32) -> Self {
        let direction = local_direction.normalize_or_zero();
        if !local_direction.is_finite()
            || !speed.is_finite()
            || direction == Vec2::ZERO
            || speed <= 0.05
        {
            return Self::planted();
        }
        Self(RaisedLocomotionKind::Moving {
            local_direction: direction,
            speed,
        })
    }

    pub fn is_moving(self) -> bool {
        matches!(self.0, RaisedLocomotionKind::Moving { .. })
    }

    pub fn local_direction(self) -> Vec2 {
        match self.0 {
            RaisedLocomotionKind::Moving {
                local_direction, ..
            } => local_direction,
            RaisedLocomotionKind::Planted => Vec2::ZERO,
        }
    }

    pub fn speed(self) -> f32 {
        match self.0 {
            RaisedLocomotionKind::Moving { speed, .. } => speed,
            RaisedLocomotionKind::Planted => 0.0,
        }
    }

    pub fn local_velocity(self) -> Vec3 {
        let direction = self.local_direction();
        Vec3::new(direction.x, 0.0, direction.y) * self.speed()
    }
}

/// Server-authored planar contact positions for raised-guard footwork. Values
/// are stored in the controller's local X/Z frame and advected against body
/// motion, so a planted virtual foot remains fixed while the root travels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GuardContacts {
    left: Vec2,
    right: Vec2,
}

impl GuardContacts {
    pub fn left(self) -> Vec2 {
        self.left
    }

    pub fn right(self) -> Vec2 {
        self.right
    }

    fn advected(self, local_displacement: Vec2) -> Self {
        Self {
            left: self.left - local_displacement,
            right: self.right - local_displacement,
        }
    }

    fn with_contact(self, foot: LeadFoot, position: Vec2) -> Self {
        match foot {
            LeadFoot::Left => Self {
                left: position,
                ..self
            },
            LeadFoot::Right => Self {
                right: position,
                ..self
            },
        }
    }

    fn normalized(self) -> Self {
        let finite = |value: Vec2, fallback: Vec2| {
            if value.is_finite() { value } else { fallback }
        };
        Self {
            left: finite(
                self.left,
                Vec2::new(-guard_footwork_config().default_half_width_metres, 0.0),
            ),
            right: finite(
                self.right,
                Vec2::new(guard_footwork_config().default_half_width_metres, 0.0),
            ),
        }
    }
}

impl Default for GuardContacts {
    fn default() -> Self {
        Self {
            left: Vec2::new(-guard_footwork_config().default_half_width_metres, 0.0),
            right: Vec2::new(guard_footwork_config().default_half_width_metres, 0.0),
        }
    }
}

/// One complete, authoritative raised-guard swing. Contact timing and landing
/// placement are a single plan; presentation may interpolate within it but may
/// not postpone its contact tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GuardStepPlan {
    contacts: GuardContacts,
    swing_foot: LeadFoot,
    swing_start: Vec2,
    landing: Vec2,
    start_tick: u64,
    contact_tick: u64,
}

impl GuardStepPlan {
    pub fn contacts(self) -> GuardContacts {
        self.contacts
    }

    pub fn swing_foot(self) -> LeadFoot {
        self.swing_foot
    }

    pub fn swing_start(self) -> Vec2 {
        self.swing_start
    }

    pub fn landing(self) -> Vec2 {
        self.landing
    }

    pub fn start_tick(self) -> u64 {
        self.start_tick
    }

    pub fn contact_tick(self) -> u64 {
        self.contact_tick
    }

    pub fn progress(self, tick: u64) -> f32 {
        let duration = self.contact_tick.saturating_sub(self.start_tick).max(1);
        tick.saturating_sub(self.start_tick) as f32 / duration as f32
    }

    pub fn direction(self) -> Vec2 {
        (self.landing - self.swing_start).normalize_or_zero()
    }

    fn advected(mut self, local_displacement: Vec2) -> Self {
        self.contacts = self.contacts.advected(local_displacement);
        self.swing_start -= local_displacement;
        self.landing -= local_displacement;
        self
    }

    fn normalized(mut self) -> Self {
        self.contacts = self.contacts.normalized();
        if !self.swing_start.is_finite() {
            self.swing_start = match self.swing_foot {
                LeadFoot::Left => self.contacts.left,
                LeadFoot::Right => self.contacts.right,
            };
        }
        if !self.landing.is_finite() {
            self.landing = self.swing_start;
        }
        self.contact_tick = self.contact_tick.max(self.start_tick.saturating_add(1));
        self
    }
}

/// The complete authoritative topology for raised-guard contacts. A step can
/// never exist without a support contact, landing, and mandatory contact tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GuardFootworkPlan {
    #[default]
    Uninitialized,
    Planted {
        contacts: GuardContacts,
        next_swing: LeadFoot,
    },
    Stepping(GuardStepPlan),
}

impl GuardFootworkPlan {
    pub fn contacts(self) -> Option<GuardContacts> {
        match self {
            Self::Uninitialized => None,
            Self::Planted { contacts, .. } => Some(contacts),
            Self::Stepping(step) => Some(step.contacts),
        }
    }

    pub fn step(self) -> Option<GuardStepPlan> {
        match self {
            Self::Stepping(step) => Some(step),
            Self::Uninitialized | Self::Planted { .. } => None,
        }
    }

    pub fn next_swing(self) -> Option<LeadFoot> {
        match self {
            Self::Planted { next_swing, .. } => Some(next_swing),
            Self::Stepping(step) => Some(opposite_foot(step.swing_foot)),
            Self::Uninitialized => None,
        }
    }

    fn normalized(self) -> Self {
        match self {
            Self::Uninitialized => Self::Uninitialized,
            Self::Planted {
                contacts,
                next_swing,
            } => Self::Planted {
                contacts: contacts.normalized(),
                next_swing,
            },
            Self::Stepping(step) => Self::Stepping(step.normalized()),
        }
    }
}

fn guard_footwork_config() -> crate::combat_config::GuardFootworkConfig {
    crate::combat_config::runtime_animation_config().guard_footwork
}

pub fn guard_contact_margin_metres() -> f32 {
    guard_footwork_config().contact_margin_metres
}

pub fn guard_maximum_unsupported_contact_seconds() -> f32 {
    guard_footwork_config().maximum_unsupported_contact_seconds
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum SkeletonAction {
    #[default]
    None,
    Dodge,
    Attack,
    Block,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum RollDirection {
    #[default]
    Left,
    Right,
}

impl RollDirection {
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum DiveDirection {
    #[default]
    Forward,
    Backward,
    Left,
    Right,
}

impl DiveDirection {
    pub fn opposite(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiveTrajectory {
    #[default]
    Airborne,
    GroundedSlide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostureTransitionKind {
    UprightToProne,
    ProneToUpright,
    ProneToSupine {
        direction: RollDirection,
    },
    SupineToProne {
        direction: RollDirection,
    },
    SupineToUpright,
    DiveToDowned {
        direction: DiveDirection,
        trajectory: DiveTrajectory,
    },
}

impl PostureTransitionKind {
    fn accepts(self, body: BodyState) -> bool {
        match self {
            Self::UprightToProne | Self::DiveToDowned { .. } => {
                matches!(body, BodyState::Grounded(_))
            }
            Self::ProneToUpright | Self::ProneToSupine { .. } => body == BodyState::Prone,
            Self::SupineToProne { .. } | Self::SupineToUpright => body == BodyState::Supine,
        }
    }

    fn target(self) -> BodyState {
        match self {
            Self::UprightToProne | Self::SupineToProne { .. } => BodyState::Prone,
            Self::DiveToDowned {
                direction: DiveDirection::Backward,
                ..
            } => BodyState::Supine,
            Self::DiveToDowned { .. } => BodyState::Prone,
            Self::ProneToSupine { .. } => BodyState::Supine,
            Self::ProneToUpright | Self::SupineToUpright => {
                BodyState::Grounded(GroundedPosture::Upright)
            }
        }
    }
}

/// Discrete camera-facing pose selected while a character is downed.
/// Interpolation occurs only while moving between these four sector centers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownedFacingPose {
    #[default]
    Prone,
    RollRight,
    Supine,
    RollLeft,
}

impl DownedFacingPose {
    fn from_half_turns(value: f32) -> Self {
        match (value * 2.0).round() as i64 % 4 {
            0 => Self::Prone,
            1 | -3 => Self::RollRight,
            2 | -2 => Self::Supine,
            3 | -1 => Self::RollLeft,
            _ => unreachable!("remainder is bounded to four downed poses"),
        }
    }

    fn canonical_half_turns(self) -> f32 {
        match self {
            Self::Prone => 0.0,
            Self::RollRight => 0.5,
            Self::Supine => 1.0,
            Self::RollLeft => -0.5,
        }
    }

    fn half_turns_near(self, reference: f32) -> f32 {
        let canonical = self.canonical_half_turns();
        canonical + ((reference - canonical) / 2.0).round() * 2.0
    }
}

/// Camera-driven roll around a downed character's head-to-feet axis. The
/// target is one of four sticky sectors; `half_turns` is the transient
/// interpolation coordinate. Values remain unwrapped so crossing the rear
/// camera seam does not reverse an in-progress roll.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DownedFacingState {
    half_turns: f32,
    target: DownedFacingPose,
    lateral_motion: f32,
}

impl DownedFacingState {
    fn normalized(mut self) -> Self {
        self.half_turns = if self.half_turns.is_finite() {
            self.half_turns
        } else {
            0.0
        };
        self.lateral_motion = if self.lateral_motion.is_finite() {
            self.lateral_motion.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.target =
            DownedFacingPose::from_half_turns(self.target.half_turns_near(self.half_turns));
        self
    }

    pub fn half_turns(self) -> f32 {
        self.half_turns
    }

    pub fn lateral_motion(self) -> f32 {
        self.lateral_motion
    }

    pub fn target(self) -> DownedFacingPose {
        self.target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PostureTransitionState {
    kind: PostureTransitionKind,
    start_tick: u64,
    duration_ticks: u64,
    phase: f32,
    dive_was_airborne: bool,
    dive_landing_tick: Option<u64>,
}

impl PostureTransitionState {
    fn new(kind: PostureTransitionKind, start_tick: u64, duration_ticks: u64) -> Self {
        Self {
            kind,
            start_tick,
            duration_ticks: duration_ticks.max(1),
            phase: 0.0,
            dive_was_airborne: false,
            dive_landing_tick: None,
        }
    }

    fn normalized(mut self) -> Self {
        self.duration_ticks = self.duration_ticks.max(1);
        self.phase = if self.phase.is_finite() {
            self.phase.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    pub fn kind(self) -> PostureTransitionKind {
        self.kind
    }

    pub fn phase(self) -> f32 {
        self.phase
    }

    /// Returns the dive recovery progress after terrain contact.
    /// The first half of a dive is guard-to-airborne and remains fixed at its
    /// airborne endpoint until impact; only the second half transfers the
    /// directional pose into its canonical downed contact pose.
    pub fn dive_recovery(self) -> Option<(DiveDirection, f32)> {
        let PostureTransitionKind::DiveToDowned {
            direction,
            trajectory,
        } = self.kind
        else {
            return None;
        };
        // The authored body first settles out of its tilted dive pose, then
        // expresses the planar yaw of the downed contact. Starting root yaw at
        // contact gets ahead of that anatomical turn and visibly twists the
        // whole character before the supine pose catches up. Reserve the first
        // part of recovery for settling, then transfer root yaw with zero
        // endpoint velocity. The endpoint remains the complete canonical turn.
        let root_handoff_start_fraction = crate::combat_config::runtime_animation_config()
            .state_transitions
            .dive_root_handoff_start_fraction;
        let root_handoff_end_fraction = match trajectory {
            // The shorter slide devotes a larger normalized share to the same
            // fixed presentation settling time, so its handoff finishes with
            // the transition. The longer airborne recovery can finish just
            // before the terminal pose without outrunning the authored turn.
            DiveTrajectory::GroundedSlide => 1.0,
            DiveTrajectory::Airborne => 0.92,
        };
        let recovery = ((self.phase - 0.5) * 2.0).clamp(0.0, 1.0);
        let handoff = ((recovery - root_handoff_start_fraction)
            / (root_handoff_end_fraction - root_handoff_start_fraction))
            .clamp(0.0, 1.0);
        Some((direction, smoothstep(handoff)))
    }
}

/// Incremental root-yaw handoff for the directional downed contact pose.
/// Applying this after each posture-transition advance keeps the character's
/// world-space head-to-feet direction fixed from contact through the final
/// downed pose.
pub fn dive_landing_facing_delta(
    previous: Option<PostureTransitionState>,
    current: Option<PostureTransitionState>,
) -> Quat {
    let Some((direction, previous_progress)) =
        previous.and_then(PostureTransitionState::dive_recovery)
    else {
        return Quat::IDENTITY;
    };
    let current_progress = current
        .and_then(PostureTransitionState::dive_recovery)
        .filter(|(current_direction, _)| *current_direction == direction)
        .map_or(1.0, |(_, progress)| progress);
    let total_yaw = match direction {
        DiveDirection::Forward => 0.0,
        // The authored backward-dive-to-supine blend contains the body's
        // half-turn. Transfer the inverse half-turn to the gameplay root over
        // the same contact-recovery interval so visible facing stays fixed
        // while the root reaches the canonical supine orientation. This must
        // not be omitted or applied as a single completion-tick correction.
        DiveDirection::Backward => std::f32::consts::PI,
        DiveDirection::Left => std::f32::consts::FRAC_PI_2,
        DiveDirection::Right => -std::f32::consts::FRAC_PI_2,
    };
    Quat::from_rotation_y(total_yaw * (current_progress - previous_progress).clamp(0.0, 1.0))
}

/// Returns the incremental root counter-yaw for a supine get-up.
///
/// Supine contact poses use the head-facing orientation required by the
/// continuous prone/supine roll coordinate. Interpolating that convention
/// into canonical upright poses therefore contains an implicit positive-pi
/// turn during the midpoint-to-upright half of the transition. Applying the
/// equivalent negative root turn only during that same half cancels the turn
/// in world space while leaving the root in the correct upright orientation at
/// the endpoint. No other posture transition receives this correction.
pub fn supine_get_up_counter_yaw_delta(
    previous: Option<PostureTransitionState>,
    current: Option<PostureTransitionState>,
) -> Quat {
    let Some(previous) =
        previous.filter(|transition| transition.kind() == PostureTransitionKind::SupineToUpright)
    else {
        return Quat::IDENTITY;
    };
    let previous_progress = ((previous.phase() - 0.5) * 2.0).clamp(0.0, 1.0);
    let current_progress = current
        .filter(|transition| transition.kind() == PostureTransitionKind::SupineToUpright)
        .map_or(1.0, |transition| {
            ((transition.phase() - 0.5) * 2.0).clamp(0.0, 1.0)
        });
    Quat::from_rotation_y(
        -std::f32::consts::PI * (current_progress - previous_progress).clamp(0.0, 1.0),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct ActionTimeline {
    start_tick: u64,
    preparation_ticks: u64,
    recovery_ticks: u64,
    phase: f32,
}

impl ActionTimeline {
    fn new(start_tick: u64, contact_tick: u64) -> Self {
        let preparation_ticks = contact_tick.saturating_sub(start_tick).max(1);
        Self {
            start_tick,
            preparation_ticks,
            recovery_ticks: preparation_ticks,
            phase: 0.0,
        }
    }

    fn with_recovery(start_tick: u64, contact_tick: u64, end_tick: u64) -> Self {
        Self {
            start_tick,
            preparation_ticks: contact_tick.saturating_sub(start_tick).max(1),
            recovery_ticks: end_tick.saturating_sub(contact_tick).max(1),
            phase: 0.0,
        }
    }

    fn normalized(mut self) -> Self {
        self.preparation_ticks = self.preparation_ticks.max(1);
        self.recovery_ticks = self.recovery_ticks.max(1);
        self.phase = if self.phase.is_finite() {
            self.phase.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    fn contact_tick(self) -> u64 {
        self.start_tick.saturating_add(self.preparation_ticks)
    }

    fn end_tick(self) -> u64 {
        self.contact_tick().saturating_add(self.recovery_ticks)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct QueuedAttack {
    target_height: f32,
    animation: AttackAnimation,
    strike_family: StrikeFamily,
    hand: AttackHand,
    curve: AttackCurve,
    timeline: ActionTimeline,
    start_coordinate: f32,
    incoming_tangent: f32,
}

impl QueuedAttack {
    fn normalized(mut self) -> Self {
        self.target_height = if self.target_height.is_finite() {
            self.target_height.clamp(0.0, 1.0)
        } else {
            AttackSpec::default().target_height
        };
        self.curve = self.curve.normalized();
        self.timeline = self.timeline.normalized();
        self.start_coordinate = finite_clamp(
            self.start_coordinate,
            -AttackCurve::maximum_drawback(),
            1.0 + AttackCurve::maximum_overshoot(),
            1.0,
        );
        self.incoming_tangent = if self.incoming_tangent.is_finite() {
            self.incoming_tangent.max(0.0)
        } else {
            0.0
        };
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum AttackSequence {
    Initial,
    Continuation {
        start_coordinate: f32,
        incoming_tangent: f32,
    },
}

impl AttackSequence {
    fn normalized(self) -> Self {
        match self {
            Self::Initial => Self::Initial,
            Self::Continuation {
                start_coordinate,
                incoming_tangent,
            } => Self::Continuation {
                start_coordinate: finite_clamp(
                    start_coordinate,
                    -AttackCurve::maximum_drawback(),
                    1.0 + AttackCurve::maximum_overshoot(),
                    1.0,
                ),
                incoming_tangent: if incoming_tangent.is_finite() {
                    incoming_tangent.max(0.0)
                } else {
                    0.0
                },
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum ActionKind {
    Idle,
    Dodge {
        dodge: DodgeKind,
        timeline: ActionTimeline,
    },
    Attack {
        target_height: f32,
        animation: AttackAnimation,
        strike_family: StrikeFamily,
        hand: AttackHand,
        sequence: AttackSequence,
        curve: AttackCurve,
        timeline: ActionTimeline,
        queued: Option<QueuedAttack>,
    },
    Block {
        incoming_line: AttackLine,
        timeline: ActionTimeline,
    },
}

/// Opaque action state. Action-specific payload and timeline construction are
/// available only through the typed transition methods on `SkeletonState`.
/// It intentionally omits reflection for the same reason as raised locomotion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ActionState(ActionKind);

impl<'de> Deserialize<'de> for ActionState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let kind = ActionKind::deserialize(deserializer)?;
        Ok(Self(match kind {
            ActionKind::Idle => ActionKind::Idle,
            ActionKind::Dodge { dodge, timeline } => ActionKind::Dodge {
                dodge,
                timeline: timeline.normalized(),
            },
            ActionKind::Attack {
                target_height,
                animation,
                strike_family,
                hand,
                sequence,
                curve,
                timeline,
                queued,
            } => ActionKind::Attack {
                target_height: if target_height.is_finite() {
                    target_height.clamp(0.0, 1.0)
                } else {
                    AttackSpec::default().target_height
                },
                animation,
                strike_family,
                hand,
                sequence: sequence.normalized(),
                curve: curve.normalized(),
                timeline: timeline.normalized(),
                queued: queued.map(QueuedAttack::normalized),
            },
            ActionKind::Block {
                incoming_line,
                timeline,
            } => ActionKind::Block {
                incoming_line,
                timeline: timeline.normalized(),
            },
        }))
    }
}

impl Default for ActionState {
    fn default() -> Self {
        Self(ActionKind::Idle)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum DodgeKind {
    Defensive,
    Quickstep { direction: QuickstepDirection },
}

/// A finite, non-zero, normalized quickstep direction. Constructing this type
/// at the input boundary makes a stationary defensive dodge structurally
/// distinct from a locomotion quickstep.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct QuickstepDirection(Vec2);

impl QuickstepDirection {
    pub fn new(direction: Vec2) -> Option<Self> {
        direction
            .is_finite()
            .then(|| direction.try_normalize())
            .flatten()
            .map(Self)
    }

    pub fn get(self) -> Vec2 {
        self.0
    }
}

impl<'de> Deserialize<'de> for QuickstepDirection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let direction = Vec2::deserialize(deserializer)?;
        Self::new(direction).ok_or_else(|| {
            serde::de::Error::custom("quickstep direction must be finite and non-zero")
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum DodgeSpec {
    #[default]
    Defensive,
    Quickstep(QuickstepDirection),
}

impl DodgeSpec {
    pub fn quickstep(direction: Vec2) -> Option<Self> {
        QuickstepDirection::new(direction).map(Self::Quickstep)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTransitionError {
    Downed,
    PostureTransitionActive,
    ActionBusy,
}

/// Bounded blend-coordinate curve for an attack's authored guard/contact
/// poses. Coordinates below zero draw back through the guard pose; coordinates
/// above one continue through contact. This mirrors Overgrowth's synced-pose
/// overshoot without extrapolating clip time beyond authored keys.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AttackCurve {
    /// Fraction of preparation spent drawing back before commitment begins.
    pub tell_fraction: f32,
    /// Furthest negative guard-to-contact blend coordinate reached by the tell.
    pub drawback: f32,
    /// Fraction of recovery spent continuing beyond contact before braking.
    pub follow_through_fraction: f32,
    /// Furthest distance beyond the contact pose reached during follow-through.
    pub overshoot: f32,
}

impl Default for AttackCurve {
    fn default() -> Self {
        Self::from_handling(0.3, 3.0)
    }
}

impl AttackCurve {
    pub fn maximum_drawback() -> f32 {
        crate::combat_config::runtime_combat_presentation_config()
            .attack_curve
            .maximum_drawback
    }

    pub fn maximum_overshoot() -> f32 {
        crate::combat_config::runtime_combat_presentation_config()
            .attack_curve
            .maximum_overshoot
    }

    /// Produces a readable but controlled curve from physical weapon inertia
    /// and the attacker's effective weapon skill check. High-inertia weapons
    /// and low-skill attacks telegraph and follow through more.
    pub fn from_handling(moment_of_inertia_kg_m2: f32, skill: f32) -> Self {
        Self::from_handling_with_config(
            moment_of_inertia_kg_m2,
            skill,
            &crate::combat_config::TacticalCombatConfig::default()
                .presentation
                .attack_curve,
        )
    }

    pub fn from_handling_with_config(
        moment_of_inertia_kg_m2: f32,
        skill: f32,
        config: &crate::combat_config::AttackCurveConfig,
    ) -> Self {
        let inertia = if moment_of_inertia_kg_m2.is_finite() {
            moment_of_inertia_kg_m2.max(0.0)
        } else {
            0.3
        };
        let inertia_difficulty = (inertia / (inertia + config.inertia_characteristic)).sqrt();
        let skill = finite_clamp(skill / 5.0, 0.0, 1.0, 0.0);
        let lack_of_control =
            inertia_difficulty * config.inertia_weight + (1.0 - skill) * config.skill_weight;
        Self {
            tell_fraction: config.tell_base + config.tell_span * lack_of_control,
            drawback: config.drawback_base + config.drawback_span * lack_of_control,
            follow_through_fraction: config.follow_through_base
                + config.follow_through_span * lack_of_control,
            overshoot: config.overshoot_base + config.overshoot_span * lack_of_control,
        }
        .normalized_with_limits(config.maximum_drawback, config.maximum_overshoot)
    }

    fn normalized(self) -> Self {
        self.normalized_with_limits(Self::maximum_drawback(), Self::maximum_overshoot())
    }

    fn normalized_with_limits(mut self, maximum_drawback: f32, maximum_overshoot: f32) -> Self {
        self.tell_fraction = finite_clamp(self.tell_fraction, 0.15, 0.75, 0.45);
        self.drawback = finite_clamp(self.drawback, 0.0, maximum_drawback, 0.3);
        self.follow_through_fraction = finite_clamp(self.follow_through_fraction, 0.1, 0.65, 0.3);
        self.overshoot = finite_clamp(self.overshoot, 0.0, maximum_overshoot, 0.2);
        self
    }

    /// Unclamped semantic pose coordinate at a normalized action phase where
    /// contact remains exactly 0.5.
    pub fn coordinate(self, phase: f32) -> f32 {
        let phase = finite_clamp(phase, 0.0, 1.0, 0.0);
        let tell_end = self.tell_fraction * 0.5;
        let follow_through_end = 0.5 + self.follow_through_fraction * 0.5;
        if phase <= tell_end {
            -self.drawback * smootherstep(phase / tell_end)
        } else if phase <= 0.5 {
            let duration = 0.5 - tell_end;
            quintic_hermite(
                -self.drawback,
                1.0,
                0.0,
                self.contact_velocity() * duration,
                (phase - tell_end) / duration,
            )
        } else if phase <= follow_through_end {
            let duration = follow_through_end - 0.5;
            quintic_hermite(
                1.0,
                1.0 + self.overshoot,
                self.contact_velocity() * duration,
                self.overshoot,
                (phase - 0.5) / duration,
            )
        } else {
            let duration = 1.0 - follow_through_end;
            let incoming_velocity = self.overshoot / (follow_through_end - 0.5);
            quintic_hermite(
                1.0 + self.overshoot,
                0.0,
                incoming_velocity * duration,
                0.0,
                (phase - follow_through_end) / duration,
            )
        }
    }

    /// A queued continuation consumes the complete post-contact backswing.
    /// It reaches the furthest frame-0-to-frame-4 extrapolation with a live
    /// tangent so the follow-up preparation can inherit its momentum.
    pub fn queued_recovery_coordinate(self, phase: f32) -> f32 {
        let phase = finite_clamp(phase, 0.0, 1.0, 0.0);
        let follow_through_end = 0.5 + self.follow_through_fraction * 0.5;
        if phase <= 0.5 {
            self.coordinate(phase)
        } else if phase <= follow_through_end {
            let duration = follow_through_end - 0.5;
            quintic_hermite(
                1.0,
                1.0 + self.overshoot,
                self.contact_velocity() * duration,
                self.overshoot,
                (phase - 0.5) / duration,
            )
        } else {
            let duration = follow_through_end - 0.5;
            let coordinate =
                1.0 + self.overshoot + (phase - follow_through_end) * self.overshoot / duration;
            coordinate.min(1.0 + Self::maximum_overshoot())
        }
    }

    fn queued_transition_phase(self) -> f32 {
        0.5 + self.follow_through_fraction * 0.5
    }

    /// Phase-coordinate speed shared by the strike and follow-through at
    /// contact. Bounding it by both neighboring secants keeps each quintic
    /// segment monotone while preventing the old stop-and-restart seam.
    fn contact_velocity(self) -> f32 {
        if self.overshoot <= f32::EPSILON {
            return 0.0;
        }
        let strike_duration = 0.5 * (1.0 - self.tell_fraction);
        let follow_through_duration = 0.5 * self.follow_through_fraction;
        let strike_secant = (1.0 + self.drawback) / strike_duration;
        let follow_through_secant = self.overshoot / follow_through_duration;
        2.0 * strike_secant.min(follow_through_secant)
    }
}

/// The follow-ready pose lies roughly halfway along the authored preparation
/// path. Global weapon-rotation measurements put the constant-acceleration
/// crossover at about 58% of the fixed preparation interval, independently of
/// the source file's arbitrary equal four-frame spacing.
const FOLLOW_READY_PREPARATION_FRACTION: f32 = 7.0 / 12.0;

pub(super) fn continuation_ready_phase() -> f32 {
    0.5 * FOLLOW_READY_PREPARATION_FRACTION
}

pub(super) fn continuation_outgoing_tangent_scale() -> f32 {
    FOLLOW_READY_PREPARATION_FRACTION / (1.0 - FOLLOW_READY_PREPARATION_FRACTION)
}

fn finite_clamp(value: f32, minimum: f32, maximum: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

/// Quintic Hermite interpolation with zero acceleration at both endpoints.
/// Velocities are expressed in normalized-segment coordinates.
fn quintic_hermite(
    start: f32,
    end: f32,
    start_velocity: f32,
    end_velocity: f32,
    progress: f32,
) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;
    let start_position_basis = 1.0 - 10.0 * t3 + 15.0 * t4 - 6.0 * t5;
    let start_velocity_basis = t - 6.0 * t3 + 8.0 * t4 - 3.0 * t5;
    let end_position_basis = 10.0 * t3 - 15.0 * t4 + 6.0 * t5;
    let end_velocity_basis = -4.0 * t3 + 7.0 * t4 - 3.0 * t5;
    start * start_position_basis
        + start_velocity * start_velocity_basis
        + end * end_position_basis
        + end_velocity * end_velocity_basis
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AttackSpec {
    pub target_height: f32,
    pub animation: AttackAnimation,
    pub strike_family: StrikeFamily,
    pub hand: AttackHand,
    pub continuation: bool,
    pub curve: AttackCurve,
}

impl Default for AttackSpec {
    fn default() -> Self {
        Self {
            target_height: 0.5,
            animation: AttackAnimation::Thrust,
            strike_family: StrikeFamily::Thrust,
            hand: AttackHand::Main,
            continuation: false,
            curve: AttackCurve::default(),
        }
    }
}

impl AttackSpec {
    pub fn new(animation: AttackAnimation) -> Self {
        Self {
            animation,
            strike_family: animation.strike_family(),
            ..Self::default()
        }
    }

    pub fn main(family: StrikeFamily, continuation: bool) -> Self {
        Self {
            animation: AttackAnimation::initial(family),
            strike_family: family,
            hand: AttackHand::Main,
            continuation,
            ..Self::default()
        }
    }

    pub fn offhand(family: StrikeFamily) -> Self {
        Self {
            animation: AttackAnimation::Offhand,
            strike_family: family,
            hand: AttackHand::Offhand,
            continuation: false,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockSpec {
    pub incoming_line: AttackLine,
}

impl ActionState {
    pub fn kind(self) -> SkeletonAction {
        match self {
            Self(ActionKind::Idle) => SkeletonAction::None,
            Self(ActionKind::Dodge { .. }) => SkeletonAction::Dodge,
            Self(ActionKind::Attack { .. }) => SkeletonAction::Attack,
            Self(ActionKind::Block { .. }) => SkeletonAction::Block,
        }
    }
    pub fn phase(self) -> f32 {
        match self {
            Self(ActionKind::Idle) => 0.0,
            Self(ActionKind::Dodge { timeline, .. })
            | Self(ActionKind::Attack { timeline, .. })
            | Self(ActionKind::Block { timeline, .. }) => timeline.phase,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum StrikeFamily {
    #[default]
    Thrust,
    Swing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum AttackHand {
    #[default]
    Main,
    Offhand,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum MeleePreparationInput {
    #[default]
    Preferred,
    Alternate,
    Offhand,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AttackPreparation {
    pub from: AttackAnimation,
    pub to: AttackAnimation,
    pub progress: f32,
}

impl Default for AttackPreparation {
    fn default() -> Self {
        Self::main(StrikeFamily::Thrust)
    }
}

impl AttackPreparation {
    pub const fn main(family: StrikeFamily) -> Self {
        let animation = AttackAnimation::initial(family);
        Self {
            from: animation,
            to: animation,
            progress: 1.0,
        }
    }

    pub const fn offhand() -> Self {
        Self {
            from: AttackAnimation::Offhand,
            to: AttackAnimation::Offhand,
            progress: 1.0,
        }
    }

    fn normalized(mut self) -> Self {
        self.progress = if self.progress.is_finite() {
            self.progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }
}

impl StrikeFamily {
    pub fn from_melee_style(style: MeleeAttackStyle) -> Self {
        match style {
            MeleeAttackStyle::Swing => Self::Swing,
            MeleeAttackStyle::Stab => Self::Thrust,
        }
    }

    pub fn melee_style(self) -> MeleeAttackStyle {
        match self {
            Self::Swing => MeleeAttackStyle::Swing,
            Self::Thrust => MeleeAttackStyle::Stab,
        }
    }

    pub const fn alternate(self) -> Self {
        match self {
            Self::Swing => Self::Thrust,
            Self::Thrust => Self::Swing,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum AttackAnimation {
    Swing,
    #[default]
    Thrust,
    Offhand,
}

impl AttackAnimation {
    pub fn strike_family(self) -> StrikeFamily {
        match self {
            Self::Swing => StrikeFamily::Swing,
            Self::Thrust => StrikeFamily::Thrust,
            Self::Offhand => StrikeFamily::Thrust,
        }
    }

    pub const fn initial(family: StrikeFamily) -> Self {
        match family {
            StrikeFamily::Swing => Self::Swing,
            StrikeFamily::Thrust => Self::Thrust,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct AttackAnimations {
    pub swing: bool,
    pub swing_continuation: bool,
    pub thrust: bool,
    pub thrust_continuation: bool,
    pub offhand: bool,
    pub offhand_preparation: bool,
}

impl AttackAnimations {
    pub const NONE: Self = Self {
        swing: false,
        swing_continuation: false,
        thrust: false,
        thrust_continuation: false,
        offhand: false,
        offhand_preparation: false,
    };
    pub const fn supports(self, animation: AttackAnimation) -> bool {
        match animation {
            AttackAnimation::Swing => self.swing,
            AttackAnimation::Thrust => self.thrust,
            AttackAnimation::Offhand => self.offhand,
        }
    }

    pub const fn supports_family(self, family: StrikeFamily) -> bool {
        self.supports(AttackAnimation::initial(family))
    }

    pub const fn any(self) -> bool {
        self.swing || self.thrust || self.offhand
    }

    pub const fn supports_continuation(self, animation: AttackAnimation) -> bool {
        match animation {
            AttackAnimation::Swing => self.swing_continuation,
            AttackAnimation::Thrust => self.thrust_continuation,
            AttackAnimation::Offhand => false,
        }
    }
}

impl Default for AttackAnimations {
    fn default() -> Self {
        Self {
            swing: true,
            swing_continuation: true,
            thrust: true,
            thrust_continuation: false,
            offhand: true,
            // The server accepts replicated held preparation for every
            // offhand attack. Each client resolves whether its loaded clip
            // actually has the optional frame-4 contact anchor.
            offhand_preparation: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum AttackLine {
    #[default]
    Thrust,
    CutFromLeft,
    CutFromRight,
}

/// Minimum server-authored presentation state. Bone transforms and IK targets
/// intentionally do not cross the network boundary.
/// Reflection is intentionally omitted because body/stance/action transitions
/// must pass through the atomic APIs below.
#[derive(Component, Debug, Clone, PartialEq, Serialize)]
pub struct SkeletonState {
    body: BodyState,
    jump_anticipation: JumpAnticipation,
    pub local_velocity: Vec3,
    pub world_velocity: Vec3,
    pub gait_phase: f32,
    pub locomotion_sample_tick: u64,
    pub world_acceleration: Vec3,
    pub contact_sequence: u64,
    pub contact_foot: LeadFoot,
    raised_footwork: GuardFootworkPlan,
    pub landing_sequence: u64,
    pub landing_impact_speed: f32,
    pub lead_foot: LeadFoot,
    guarded_sprint_locomotion: bool,
    stance: StanceState,
    attack_preparation: AttackPreparation,
    action: ActionState,
    posture_transition: Option<PostureTransitionState>,
    downed_facing: Option<DownedFacingState>,
    downed_turning: bool,
    pub animation_pack: String,
    pub attack_animations: AttackAnimations,
}

#[derive(Deserialize)]
struct SkeletonStateWire {
    body: BodyState,
    jump_anticipation: JumpAnticipation,
    local_velocity: Vec3,
    world_velocity: Vec3,
    gait_phase: f32,
    locomotion_sample_tick: u64,
    world_acceleration: Vec3,
    contact_sequence: u64,
    contact_foot: LeadFoot,
    #[serde(default)]
    raised_footwork: GuardFootworkPlan,
    landing_sequence: u64,
    landing_impact_speed: f32,
    lead_foot: LeadFoot,
    guarded_sprint_locomotion: bool,
    stance: StanceState,
    attack_preparation: AttackPreparation,
    action: ActionState,
    posture_transition: Option<PostureTransitionState>,
    downed_facing: Option<DownedFacingState>,
    downed_turning: bool,
    animation_pack: String,
    attack_animations: AttackAnimations,
}

impl<'de> Deserialize<'de> for SkeletonState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SkeletonStateWire::deserialize(deserializer)?;
        let finite = |value: Vec3| if value.is_finite() { value } else { Vec3::ZERO };
        let mut state = Self {
            body: BodyState::default(),
            jump_anticipation: JumpAnticipation::Inactive,
            local_velocity: finite(wire.local_velocity),
            world_velocity: finite(wire.world_velocity),
            gait_phase: if wire.gait_phase.is_finite() {
                wire.gait_phase.rem_euclid(1.0)
            } else {
                0.0
            },
            locomotion_sample_tick: wire.locomotion_sample_tick,
            world_acceleration: finite(wire.world_acceleration),
            contact_sequence: wire.contact_sequence,
            contact_foot: wire.contact_foot,
            raised_footwork: wire.raised_footwork.normalized(),
            landing_sequence: wire.landing_sequence,
            landing_impact_speed: if wire.landing_impact_speed.is_finite() {
                wire.landing_impact_speed.max(0.0)
            } else {
                0.0
            },
            lead_foot: wire.lead_foot,
            guarded_sprint_locomotion: wire.guarded_sprint_locomotion,
            stance: wire.stance,
            attack_preparation: wire.attack_preparation.normalized(),
            action: wire.action,
            posture_transition: wire
                .posture_transition
                .map(PostureTransitionState::normalized),
            downed_facing: wire.downed_facing.map(DownedFacingState::normalized),
            downed_turning: wire.downed_turning,
            animation_pack: wire.animation_pack,
            attack_animations: wire.attack_animations,
        };
        state.transition_body(wire.body);
        state.set_jump_anticipation(wire.jump_anticipation == JumpAnticipation::Charging);
        state.set_guarded_sprint_locomotion(wire.guarded_sprint_locomotion);
        Ok(state)
    }
}

impl Default for SkeletonState {
    fn default() -> Self {
        Self {
            body: BodyState::default(),
            jump_anticipation: JumpAnticipation::Inactive,
            local_velocity: Vec3::ZERO,
            world_velocity: Vec3::ZERO,
            gait_phase: 0.0,
            locomotion_sample_tick: 0,
            world_acceleration: Vec3::ZERO,
            contact_sequence: 0,
            contact_foot: LeadFoot::Left,
            raised_footwork: GuardFootworkPlan::default(),
            landing_sequence: 0,
            landing_impact_speed: 0.0,
            lead_foot: LeadFoot::Left,
            guarded_sprint_locomotion: false,
            stance: StanceState::Lowered,
            attack_preparation: AttackPreparation::default(),
            action: ActionState::default(),
            posture_transition: None,
            downed_facing: None,
            downed_turning: false,
            animation_pack: "humanoid_unarmed".to_owned(),
            attack_animations: AttackAnimations::default(),
        }
    }
}

/// Applies authoritative guard state and aligns a newly raised stance with
/// the static-guard endpoint shared by every directional shuttle.
pub fn set_weapon_guard(skeleton: &mut SkeletonState, weapon_guard: WeaponGuardState) {
    match (skeleton.stance, weapon_guard) {
        (StanceState::Lowered, WeaponGuardState::Lowered)
        | (StanceState::Raised { .. }, WeaponGuardState::Raised) => {}
        (StanceState::Lowered, WeaponGuardState::Raised)
            if !skeleton.body.is_downed() && skeleton.posture_transition.is_none() =>
        {
            skeleton.gait_phase = 0.0;
            skeleton.stance = StanceState::Raised {
                locomotion: RaisedLocomotionIntent::default(),
            };
            skeleton.raised_footwork = GuardFootworkPlan::default();
        }
        (StanceState::Lowered, WeaponGuardState::Raised) => {}
        (StanceState::Raised { .. }, WeaponGuardState::Lowered) => {
            skeleton.stance = StanceState::Lowered;
            skeleton.guarded_sprint_locomotion = false;
            skeleton.raised_footwork = GuardFootworkPlan::default();
        }
    }
}

impl SkeletonState {
    pub fn body(&self) -> BodyState {
        self.body
    }
    pub fn stance(&self) -> StanceState {
        self.stance
    }
    pub fn with_body_state(mut self, body: BodyState) -> Self {
        self.transition_body(body);
        self
    }
    pub fn with_local_velocity(mut self, velocity: Vec3) -> Self {
        self.local_velocity = if velocity.is_finite() {
            velocity
        } else {
            Vec3::ZERO
        };
        self
    }
    pub fn with_world_velocity(mut self, velocity: Vec3) -> Self {
        self.world_velocity = if velocity.is_finite() {
            velocity
        } else {
            Vec3::ZERO
        };
        self
    }
    pub fn with_gait_phase(mut self, phase: f32) -> Self {
        self.gait_phase = if phase.is_finite() {
            phase.rem_euclid(1.0)
        } else {
            0.0
        };
        self
    }
    pub fn with_locomotion_sample_tick(mut self, tick: u64) -> Self {
        self.locomotion_sample_tick = tick;
        self
    }
    pub fn with_lead_foot(mut self, lead: LeadFoot) -> Self {
        self.lead_foot = lead;
        self
    }
    /// Atomically changes physical mode. Raised movement is valid only while
    /// grounded upright; other non-downed modes retain the guard but plant it.
    /// Entering a downed mode also cancels presentation actions.
    pub fn transition_body(&mut self, body: BodyState) {
        self.body = body;
        if body != BodyState::Grounded(GroundedPosture::Upright) {
            self.jump_anticipation = JumpAnticipation::Inactive;
        }
        if !body.is_downed() {
            self.downed_facing = None;
            self.downed_turning = false;
        }
        if body != BodyState::Grounded(GroundedPosture::Upright) {
            self.guarded_sprint_locomotion = false;
        }
        if body.is_downed() {
            self.stance = StanceState::Lowered;
            self.raised_footwork = GuardFootworkPlan::default();
            self.action = ActionState::default();
        } else if body != BodyState::Grounded(GroundedPosture::Upright)
            && let StanceState::Raised { locomotion } = self.stance
            && locomotion.is_moving()
        {
            self.stance = StanceState::Raised {
                locomotion: RaisedLocomotionIntent::planted(),
            };
            self.raised_footwork = GuardFootworkPlan::default();
        }
    }
    pub fn with_weapon_guard(mut self, guard: WeaponGuardState) -> Self {
        set_weapon_guard(&mut self, guard);
        self
    }
    pub fn with_raised_locomotion(mut self, locomotion: RaisedLocomotionIntent) -> Self {
        self.set_raised_locomotion(locomotion);
        self
    }
    pub fn set_guarded_sprint_locomotion(&mut self, enabled: bool) {
        self.guarded_sprint_locomotion = enabled
            && self.body == BodyState::Grounded(GroundedPosture::Upright)
            && self.weapon_guard() == WeaponGuardState::Raised;
    }
    pub fn with_guarded_sprint_locomotion(mut self, enabled: bool) -> Self {
        self.set_guarded_sprint_locomotion(enabled);
        self
    }
    pub fn guarded_sprint_locomotion(&self) -> bool {
        self.guarded_sprint_locomotion
            && self.body == BodyState::Grounded(GroundedPosture::Upright)
            && self.weapon_guard() == WeaponGuardState::Raised
    }
    pub fn weapon_guard(&self) -> WeaponGuardState {
        match self.stance {
            StanceState::Lowered => WeaponGuardState::Lowered,
            StanceState::Raised { .. } => WeaponGuardState::Raised,
        }
    }
    pub fn raised_locomotion(&self) -> RaisedLocomotionIntent {
        match self.stance {
            StanceState::Lowered => RaisedLocomotionIntent::default(),
            StanceState::Raised { locomotion } => locomotion,
        }
    }
    pub fn raised_footwork(&self) -> GuardFootworkPlan {
        self.raised_footwork
    }
    fn set_raised_locomotion(&mut self, locomotion: RaisedLocomotionIntent) {
        if matches!(self.stance, StanceState::Raised { .. }) {
            let locomotion = if self.body == BodyState::Grounded(GroundedPosture::Upright) {
                locomotion
            } else {
                RaisedLocomotionIntent::planted()
            };
            self.stance = StanceState::Raised { locomotion };
        }
    }
    pub fn posture(&self) -> Posture {
        self.body.posture()
    }
    pub fn jump_anticipation(&self) -> JumpAnticipation {
        self.jump_anticipation
    }
    pub fn set_jump_anticipation(&mut self, charging: bool) {
        self.jump_anticipation = if charging
            && self.body == BodyState::Grounded(GroundedPosture::Upright)
            && self.posture_transition.is_none()
        {
            JumpAnticipation::Charging
        } else {
            JumpAnticipation::Inactive
        };
    }
    pub fn posture_transition(&self) -> Option<PostureTransitionState> {
        self.posture_transition
    }
    pub fn downed_facing(&self) -> Option<DownedFacingState> {
        self.downed_facing
    }
    pub fn downed_turning(&self) -> bool {
        self.downed_turning && self.body.is_downed() && self.posture_transition.is_none()
    }
    pub fn set_downed_turning(&mut self, turning: bool) {
        self.downed_turning = turning && self.body.is_downed() && self.posture_transition.is_none();
    }
    pub fn downed_lateral_motion(&self) -> f32 {
        if let Some(transition) = self.posture_transition {
            return match transition.kind {
                PostureTransitionKind::ProneToSupine { direction }
                | PostureTransitionKind::SupineToProne { direction } => match direction {
                    RollDirection::Left => -1.0,
                    RollDirection::Right => 1.0,
                },
                _ => 0.0,
            };
        }
        self.downed_facing
            .map(DownedFacingState::lateral_motion)
            .unwrap_or(0.0)
    }
    pub fn begin_posture_transition(
        &mut self,
        kind: PostureTransitionKind,
        start_tick: u64,
        duration_ticks: u64,
    ) -> bool {
        if self.posture_transition.is_some() || !kind.accepts(self.body) {
            return false;
        }
        self.stance = StanceState::Lowered;
        self.raised_footwork = GuardFootworkPlan::default();
        self.jump_anticipation = JumpAnticipation::Inactive;
        self.guarded_sprint_locomotion = false;
        self.action = ActionState::default();
        self.downed_facing = None;
        self.downed_turning = false;
        self.posture_transition = Some(PostureTransitionState::new(
            kind,
            start_tick,
            duration_ticks,
        ));
        true
    }
    pub fn advance_posture_transition(&mut self, current_tick: u64) {
        let Some(mut transition) = self.posture_transition else {
            return;
        };
        let elapsed = current_tick.saturating_sub(transition.start_tick);
        if matches!(
            transition.kind,
            PostureTransitionKind::DiveToDowned {
                trajectory: DiveTrajectory::GroundedSlide,
                ..
            }
        ) {
            if elapsed >= transition.duration_ticks {
                self.finish_posture_transition(transition.kind);
                return;
            }
            transition.phase = elapsed as f32 / transition.duration_ticks as f32;
            self.posture_transition = Some(transition);
            return;
        }
        if matches!(transition.kind, PostureTransitionKind::DiveToDowned { .. }) {
            if !self.body.is_surface_supported() {
                transition.dive_was_airborne = true;
                transition.phase = 0.5;
                self.posture_transition = Some(transition);
                return;
            }
            if transition.dive_was_airborne {
                let landing_tick = *transition.dive_landing_tick.get_or_insert(current_tick);
                let recovery_elapsed = current_tick.saturating_sub(landing_tick);
                if recovery_elapsed >= transition.duration_ticks {
                    self.finish_posture_transition(transition.kind);
                    return;
                }
                transition.phase =
                    0.5 + 0.5 * recovery_elapsed as f32 / transition.duration_ticks as f32;
                self.posture_transition = Some(transition);
                return;
            }
            if elapsed >= transition.duration_ticks {
                // A delayed or missed unsupported-controller sample must not
                // skip directly from the loading half to the final contact
                // pose. Enter the same bounded recovery used after a detected
                // landing so pose and facing remain continuous.
                transition.dive_was_airborne = true;
                transition.dive_landing_tick = Some(current_tick);
                transition.phase = 0.5;
                self.posture_transition = Some(transition);
                return;
            }
            transition.phase = 0.5 * elapsed as f32 / transition.duration_ticks as f32;
            self.posture_transition = Some(transition);
            return;
        }
        if elapsed >= transition.duration_ticks {
            self.posture_transition = None;
            self.transition_body(transition.kind.target());
            return;
        }
        transition.phase = elapsed as f32 / transition.duration_ticks as f32;
        self.posture_transition = Some(transition);
    }
    fn finish_posture_transition(&mut self, kind: PostureTransitionKind) {
        self.posture_transition = None;
        self.transition_body(kind.target());
        // A lateral dive's recovery already ends at the matching authored
        // side-supported roll pose. Seed the continuous roll coordinate at
        // that exact midpoint so camera-following can continue from it rather
        // than briefly returning through prone idle. Without held aim, the
        // ordinary downed-facing update settles this midpoint back to prone.
        self.downed_facing = match kind {
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Left,
                ..
            } => Some(DownedFacingState {
                half_turns: -0.5,
                target: DownedFacingPose::RollLeft,
                lateral_motion: 0.0,
            }),
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Right,
                ..
            } => Some(DownedFacingState {
                half_turns: 0.5,
                target: DownedFacingPose::RollRight,
                lateral_motion: 0.0,
            }),
            _ => None,
        };
    }
    pub fn is_posture_transitioning(&self) -> bool {
        self.posture_transition.is_some()
    }
    /// Follows or settles the downed roll. Held aim selects one of four sticky
    /// camera sectors; only a sector change interpolates the pose. After
    /// release, the raw camera angle selects the nearer whole contact pose.
    /// Returns true while the camera-driven state remains active.
    pub fn advance_downed_facing(
        &mut self,
        camera_target_half_turns: f32,
        aim_held: bool,
        maximum_step: f32,
    ) -> bool {
        if !matches!(self.body, BodyState::Prone | BodyState::Supine)
            || self.posture_transition.is_some()
        {
            self.downed_facing = None;
            return false;
        }
        let camera_target = if camera_target_half_turns.is_finite() {
            camera_target_half_turns
        } else {
            0.0
        };
        let initial_target = match self.body {
            BodyState::Prone => DownedFacingPose::Prone,
            BodyState::Supine => DownedFacingPose::Supine,
            _ => unreachable!("downed body checked above"),
        };
        let initial = initial_target.half_turns_near(camera_target);
        let previous = self.downed_facing;
        let current = previous
            .map(DownedFacingState::half_turns)
            .unwrap_or(initial);
        let target = if aim_held {
            let tuning = crate::combat_config::runtime_animation_config().state_transitions;
            let committed = previous.map(|state| state.target).unwrap_or(initial_target);
            let committed_half_turns = committed.half_turns_near(current);
            let camera_unwrapped =
                camera_target + ((committed_half_turns - camera_target) / 2.0).round() * 2.0;
            let target_pose = if (camera_unwrapped - committed_half_turns).abs()
                > tuning.downed_facing_sector_half_width + tuning.downed_facing_edge_stickiness
            {
                DownedFacingPose::from_half_turns(camera_unwrapped)
            } else {
                committed
            };
            target_pose.half_turns_near(camera_unwrapped)
        } else {
            let lower = current.floor();
            if (current - lower - 0.5).abs() <= 1.0e-4 {
                match self.body {
                    BodyState::Prone => (current / 2.0).round() * 2.0,
                    BodyState::Supine => ((current - 1.0) / 2.0).round() * 2.0 + 1.0,
                    _ => unreachable!("downed body checked above"),
                }
            } else {
                current.round()
            }
        };
        let target_pose = if aim_held {
            DownedFacingPose::from_half_turns(target)
        } else if (target as i64).rem_euclid(2) == 0 {
            DownedFacingPose::Prone
        } else {
            DownedFacingPose::Supine
        };
        let step = if maximum_step.is_finite() {
            maximum_step.max(0.0)
        } else {
            0.0
        };
        let next = current + (target - current).clamp(-step, step);
        self.downed_facing = Some(DownedFacingState {
            half_turns: next,
            target: target_pose,
            lateral_motion: if (next - current).abs() > 1.0e-5 {
                (next - current).signum()
            } else {
                0.0
            },
        });

        if (next - next.round()).abs() <= 1.0e-4 {
            let contact = next.round() as i64;
            self.transition_body(if contact.rem_euclid(2) == 0 {
                BodyState::Prone
            } else {
                BodyState::Supine
            });
            if !aim_held {
                self.downed_facing = None;
                return false;
            }
        }
        true
    }
    pub fn is_grounded(&self) -> bool {
        self.body.is_grounded()
    }
    pub fn is_surface_supported(&self) -> bool {
        self.body.is_surface_supported()
    }
    pub fn action(&self) -> ActionState {
        self.action
    }
    pub fn action_kind(&self) -> SkeletonAction {
        self.action.kind()
    }
    pub fn action_phase(&self) -> f32 {
        self.action.phase()
    }
    pub fn action_direction(&self) -> Vec2 {
        match self.action {
            ActionState(ActionKind::Dodge {
                dodge: DodgeKind::Quickstep { direction },
                ..
            }) => direction.get(),
            _ => Vec2::ZERO,
        }
    }
    pub fn is_quickstep(&self) -> bool {
        matches!(
            self.action,
            ActionState(ActionKind::Dodge {
                dodge: DodgeKind::Quickstep { .. },
                ..
            })
        )
    }
    pub fn attack_target_height(&self) -> f32 {
        match self.action {
            ActionState(ActionKind::Attack { target_height, .. }) => target_height,
            _ => 0.5,
        }
    }
    pub fn strike_family(&self) -> StrikeFamily {
        match self.action {
            ActionState(ActionKind::Attack { strike_family, .. }) => strike_family,
            _ => StrikeFamily::Thrust,
        }
    }
    pub fn attack_hand(&self) -> AttackHand {
        match self.action {
            ActionState(ActionKind::Attack { hand, .. }) => hand,
            _ => AttackHand::Main,
        }
    }
    pub fn attack_is_continuation(&self) -> bool {
        matches!(
            self.action,
            ActionState(ActionKind::Attack {
                sequence: AttackSequence::Continuation { .. },
                ..
            })
        )
    }
    pub fn attack_continuation_incoming_tangent(&self) -> Option<f32> {
        match self.action {
            ActionState(ActionKind::Attack {
                sequence:
                    AttackSequence::Continuation {
                        incoming_tangent, ..
                    },
                ..
            }) => Some(incoming_tangent),
            _ => None,
        }
    }
    pub fn attack_continuation_start_coordinate(&self) -> Option<f32> {
        match self.action {
            ActionState(ActionKind::Attack {
                sequence:
                    AttackSequence::Continuation {
                        start_coordinate, ..
                    },
                ..
            }) => Some(start_coordinate),
            _ => None,
        }
    }
    pub fn attack_curve(&self) -> AttackCurve {
        match self.action {
            ActionState(ActionKind::Attack { curve, .. }) => curve,
            _ => AttackCurve::default(),
        }
    }
    pub fn attack_animation(&self) -> Option<AttackAnimation> {
        match self.action {
            ActionState(ActionKind::Attack { animation, .. }) => Some(animation),
            _ => None,
        }
    }

    pub fn attack_preparation(&self) -> AttackPreparation {
        self.attack_preparation
    }

    pub fn set_attack_preparation(&mut self, preparation: AttackPreparation) {
        self.attack_preparation = preparation.normalized();
    }
    pub fn available_strike_family(&self, preferred: StrikeFamily) -> Option<StrikeFamily> {
        if self.attack_animations.supports_family(preferred) {
            Some(preferred)
        } else {
            let alternate = preferred.alternate();
            self.attack_animations
                .supports_family(alternate)
                .then_some(alternate)
        }
    }

    /// Selects a legal authored attack at the current action seam. An ordinary
    /// attack starts only from recovery-complete idle. A second swing may be
    /// queued while the initial swing is active when the pack owns a follow
    /// pose; it still begins only as the first attack reaches its complete
    /// post-contact backswing.
    pub fn select_main_attack(&self, family: StrikeFamily) -> Option<AttackSpec> {
        let animation = AttackAnimation::initial(family);
        match self.attack_animation() {
            None => self
                .attack_animations
                .supports(animation)
                .then_some(AttackSpec::main(family, false)),
            Some(active)
                if active == animation
                    && self.attack_hand() == AttackHand::Main
                    && !self.attack_is_continuation()
                    && !matches!(
                        self.action,
                        ActionState(ActionKind::Attack {
                            queued: Some(_),
                            ..
                        })
                    )
                    && self.attack_animations.supports_continuation(animation) =>
            {
                Some(AttackSpec::main(family, true))
            }
            _ => None,
        }
    }

    pub fn select_offhand_attack(&self, family: StrikeFamily) -> Option<AttackSpec> {
        (self.action_kind() == SkeletonAction::None && self.attack_animations.offhand)
            .then_some(AttackSpec::offhand(family))
    }
    pub fn action_start_tick(&self) -> Option<u64> {
        match self.action {
            ActionState(ActionKind::Idle) => None,
            ActionState(ActionKind::Dodge { timeline, .. })
            | ActionState(ActionKind::Attack { timeline, .. })
            | ActionState(ActionKind::Block { timeline, .. }) => Some(timeline.start_tick),
        }
    }
    pub fn action_preparation_ticks(&self) -> Option<u64> {
        match self.action {
            ActionState(ActionKind::Idle) => None,
            ActionState(ActionKind::Dodge { timeline, .. })
            | ActionState(ActionKind::Attack { timeline, .. })
            | ActionState(ActionKind::Block { timeline, .. }) => Some(timeline.preparation_ticks),
        }
    }
    pub fn attack_has_queued_continuation(&self) -> bool {
        matches!(
            self.action,
            ActionState(ActionKind::Attack {
                queued: Some(_),
                ..
            })
        )
    }
    pub fn action_recovery_ticks(&self) -> Option<u64> {
        match self.action {
            ActionState(ActionKind::Idle) => None,
            ActionState(ActionKind::Dodge { timeline, .. })
            | ActionState(ActionKind::Attack { timeline, .. })
            | ActionState(ActionKind::Block { timeline, .. }) => Some(timeline.recovery_ticks),
        }
    }

    pub fn action_end_tick(&self) -> Option<u64> {
        match self.action {
            ActionState(ActionKind::Idle) => None,
            ActionState(ActionKind::Dodge { timeline, .. })
            | ActionState(ActionKind::Attack { timeline, .. })
            | ActionState(ActionKind::Block { timeline, .. }) => Some(timeline.end_tick()),
        }
    }

    /// Tick where a queued attack may enter its authored follow-up
    /// preparation, after the current attack reaches full follow-through but
    /// before its ordinary return-to-guard recovery would begin.
    pub fn attack_continuation_tick(&self) -> Option<u64> {
        match self.action {
            ActionState(ActionKind::Attack {
                curve,
                timeline,
                sequence: AttackSequence::Initial,
                ..
            }) => {
                let recovery_progress =
                    ((curve.queued_transition_phase() - 0.5) * 2.0).clamp(0.0, 1.0);
                let follow_through_ticks =
                    (timeline.recovery_ticks as f64 * recovery_progress as f64).ceil() as u64;
                Some(timeline.contact_tick().saturating_add(follow_through_ticks))
            }
            _ => None,
        }
    }

    /// Overrides only the visual progress of an admitted action. Tactical
    /// presentation uses this on its private `PresentedSkeleton`; gameplay
    /// admission, timing, contact, and outcomes remain authoritative.
    pub fn set_presentation_action_phase(&mut self, phase: f32) {
        let timeline = match &mut self.action {
            ActionState(ActionKind::Idle) => return,
            ActionState(ActionKind::Dodge { timeline, .. })
            | ActionState(ActionKind::Attack { timeline, .. })
            | ActionState(ActionKind::Block { timeline, .. }) => timeline,
        };
        timeline.phase = if phase.is_finite() {
            phase.clamp(0.0, 1.0)
        } else {
            timeline.phase
        };
    }
    pub fn incoming_attack_line(&self) -> AttackLine {
        match self.action {
            ActionState(ActionKind::Block { incoming_line, .. }) => incoming_line,
            _ => AttackLine::Thrust,
        }
    }

    /// Presentation motion finishes an in-flight raised-guard step after
    /// gameplay velocity stops. Speed otherwise follows authoritative motion.
    pub fn animation_local_velocity(&self) -> Vec3 {
        let raised = self.raised_locomotion();
        if self.weapon_guard() == WeaponGuardState::Raised && raised.is_moving() {
            raised.local_velocity()
        } else {
            self.local_velocity
        }
    }

    pub fn animation_speed(&self) -> f32 {
        let physical = self.animation_local_velocity().xz().length();
        if self.downed_turning() {
            physical.max(0.8)
        } else {
            physical
        }
    }

    pub fn quickstep_is_launched(&self) -> bool {
        self.is_quickstep() && self.action_phase() >= 0.50
    }

    fn action_admission(&self) -> Result<(), ActionTransitionError> {
        if self.body.is_downed() {
            Err(ActionTransitionError::Downed)
        } else if self.posture_transition.is_some() {
            Err(ActionTransitionError::PostureTransitionActive)
        } else {
            Ok(())
        }
    }

    /// Evasion explicitly preempts an attack or block. Repeated evasion input
    /// is rejected so a held button cannot restart its timeline every tick.
    pub fn begin_dodge(
        &mut self,
        spec: DodgeSpec,
        start_tick: u64,
        contact_tick: u64,
    ) -> Result<(), ActionTransitionError> {
        self.action_admission()?;
        if self.action_kind() == SkeletonAction::Dodge {
            return Err(ActionTransitionError::ActionBusy);
        }
        let timeline = ActionTimeline::new(start_tick, contact_tick);
        let dodge = match spec {
            DodgeSpec::Defensive => DodgeKind::Defensive,
            DodgeSpec::Quickstep(direction) => DodgeKind::Quickstep { direction },
        };
        self.action = ActionState(ActionKind::Dodge { dodge, timeline });
        Ok(())
    }

    pub fn begin_attack(
        &mut self,
        spec: AttackSpec,
        start_tick: u64,
        contact_tick: u64,
    ) -> Result<(), ActionTransitionError> {
        let preparation_ticks = contact_tick.saturating_sub(start_tick).max(1);
        self.begin_attack_timed(
            spec,
            start_tick,
            contact_tick,
            contact_tick.saturating_add(preparation_ticks),
        )
    }

    /// Starts an attack with independently authored preparation and recovery.
    /// Contact remains semantic phase 0.5 for presentation and gameplay.
    pub fn begin_attack_timed(
        &mut self,
        spec: AttackSpec,
        start_tick: u64,
        contact_tick: u64,
        end_tick: u64,
    ) -> Result<(), ActionTransitionError> {
        self.action_admission()?;
        let target_height = if spec.target_height.is_finite() {
            spec.target_height.clamp(0.0, 1.0)
        } else {
            AttackSpec::default().target_height
        };
        if let ActionState(ActionKind::Attack {
            animation,
            hand: AttackHand::Main,
            sequence: AttackSequence::Initial,
            curve,
            timeline,
            queued,
            ..
        }) = &mut self.action
        {
            let recovery_progress = curve.follow_through_fraction.clamp(0.0, 1.0);
            let transition_tick = timeline.contact_tick().saturating_add(
                (timeline.recovery_ticks as f64 * recovery_progress as f64).ceil() as u64,
            );
            let may_follow = queued.is_none()
                && spec.hand == AttackHand::Main
                && spec.continuation
                && spec.animation == *animation
                && start_tick >= transition_tick;
            if may_follow {
                let queued_timeline =
                    ActionTimeline::with_recovery(start_tick, contact_tick, end_tick);
                let incoming_tangent = curve.overshoot
                    * queued_timeline.preparation_ticks as f32
                    * FOLLOW_READY_PREPARATION_FRACTION
                    / (curve.follow_through_fraction.max(f32::EPSILON)
                        * timeline.recovery_ticks as f32);
                let transition_phase = 0.5
                    + 0.5 * transition_tick.saturating_sub(timeline.contact_tick()) as f32
                        / timeline.recovery_ticks.max(1) as f32;
                *queued = Some(QueuedAttack {
                    target_height,
                    animation: spec.animation,
                    strike_family: spec.strike_family,
                    hand: spec.hand,
                    curve: spec.curve.normalized(),
                    timeline: queued_timeline,
                    start_coordinate: curve.queued_recovery_coordinate(transition_phase),
                    incoming_tangent,
                });
                return Ok(());
            }
        }
        if self.action_kind() != SkeletonAction::None {
            return Err(ActionTransitionError::ActionBusy);
        }
        self.action = ActionState(ActionKind::Attack {
            target_height,
            animation: spec.animation,
            strike_family: spec.strike_family,
            hand: spec.hand,
            sequence: if spec.continuation {
                AttackSequence::Continuation {
                    start_coordinate: 1.0 + spec.curve.overshoot,
                    incoming_tangent: 0.0,
                }
            } else {
                AttackSequence::Initial
            },
            curve: spec.curve.normalized(),
            timeline: ActionTimeline::with_recovery(start_tick, contact_tick, end_tick),
            queued: None,
        });
        Ok(())
    }

    pub fn begin_block(
        &mut self,
        spec: BlockSpec,
        start_tick: u64,
        contact_tick: u64,
    ) -> Result<(), ActionTransitionError> {
        self.action_admission()?;
        if self.action_kind() != SkeletonAction::None {
            return Err(ActionTransitionError::ActionBusy);
        }
        self.action = ActionState(ActionKind::Block {
            incoming_line: spec.incoming_line,
            timeline: ActionTimeline::new(start_tick, contact_tick),
        });
        Ok(())
    }

    /// Advances an action whose semantic contact is phase 0.5. Preparation
    /// and recovery may have different real-time durations.
    pub fn advance_action(&mut self, current_tick: u64) {
        let queued = match self.action {
            ActionState(ActionKind::Attack {
                curve,
                timeline,
                queued: Some(queued),
                ..
            }) if current_tick
                >= timeline.contact_tick().saturating_add(
                    (timeline.recovery_ticks as f64 * curve.follow_through_fraction as f64).ceil()
                        as u64,
                ) =>
            {
                Some(queued)
            }
            _ => None,
        };
        if let Some(queued) = queued {
            self.action = ActionState(ActionKind::Attack {
                target_height: queued.target_height,
                animation: queued.animation,
                strike_family: queued.strike_family,
                hand: queued.hand,
                sequence: AttackSequence::Continuation {
                    start_coordinate: queued.start_coordinate,
                    incoming_tangent: queued.incoming_tangent,
                },
                curve: queued.curve,
                timeline: queued.timeline,
                queued: None,
            });
        }
        let timeline = match &mut self.action {
            ActionState(ActionKind::Idle) => return,
            ActionState(ActionKind::Dodge { timeline, .. })
            | ActionState(ActionKind::Attack { timeline, .. })
            | ActionState(ActionKind::Block { timeline, .. }) => timeline,
        };
        let preparation = timeline.preparation_ticks.max(1);
        let recovery = timeline.recovery_ticks.max(1);
        let contact_tick = timeline.start_tick.saturating_add(preparation);
        let end_tick = contact_tick.saturating_add(recovery);
        if current_tick >= end_tick {
            if current_tick > end_tick || end_tick == u64::MAX {
                self.action = ActionState::default();
                return;
            }
            timeline.phase = 1.0;
            return;
        }
        timeline.phase = if current_tick <= contact_tick {
            0.5 * current_tick.saturating_sub(timeline.start_tick) as f32 / preparation as f32
        } else {
            0.5 + 0.5 * current_tick.saturating_sub(contact_tick) as f32 / recovery as f32
        };
    }
}

/// One authoritative fixed-tick locomotion observation. The tactical server
/// supplies this from its character controller; deterministic presentation
/// fixtures replay the same boundary without inventing gait phase directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonLocomotionInput {
    pub orientation: Quat,
    pub linear_velocity: Vec3,
    pub grounded: bool,
    pub delta_seconds: f32,
    pub tick: u64,
}

pub fn body_turn_speed_radians() -> f32 {
    std::f32::consts::PI
        / crate::combat_config::runtime_combat_presentation_config().body_turn_seconds_per_half_turn
}

pub fn downed_turn_speed_radians() -> f32 {
    crate::combat_config::runtime_combat_presentation_config().downed_turn_radians_per_second
}

/// Returns the controller's yaw without allowing camera pitch or roll to tilt
/// planar locomotion into or out of the ground plane.
pub fn controller_yaw(orientation: Quat) -> Quat {
    let forward = orientation * Vec3::NEG_Z;
    let Some(flat_forward) = forward.xz().try_normalize() else {
        return Quat::IDENTITY;
    };
    Quat::from_rotation_y((-flat_forward.x).atan2(-flat_forward.y))
}

/// Root orientation committed when a procedural directional dive launches.
/// Dive travel and pelvis tilt are both camera-relative, so they must capture
/// the same controller frame before posture-transition facing locks.
pub fn dive_launch_root_rotation(controller_orientation: Quat) -> Quat {
    let forward = controller_yaw(controller_orientation) * Vec3::NEG_Z;
    Quat::from_rotation_y(forward.x.atan2(forward.z))
}

/// Advances the authored body's +Z forward axis toward its single desired
/// world direction. Attack and block are the current guard boundary and keep
/// look-facing; all other moving states face authoritative planar velocity.
/// At exactly 180 degrees the positive turn direction is chosen consistently,
/// avoiding a normalize-through-zero snap.
pub fn advance_body_facing(
    current: Quat,
    controller_orientation: Quat,
    linear_velocity: Vec3,
    action: SkeletonAction,
    weapon_guard: WeaponGuardState,
    delta_seconds: f32,
) -> Quat {
    advance_body_facing_with_speed(
        current,
        controller_orientation,
        linear_velocity,
        action,
        weapon_guard,
        delta_seconds,
        body_turn_speed_radians(),
    )
}

pub fn advance_body_facing_with_speed(
    current: Quat,
    controller_orientation: Quat,
    linear_velocity: Vec3,
    action: SkeletonAction,
    weapon_guard: WeaponGuardState,
    delta_seconds: f32,
    turn_speed_radians: f32,
) -> Quat {
    let current_yaw = body_yaw(current);
    let desired_forward = if weapon_guard == WeaponGuardState::Raised
        || matches!(action, SkeletonAction::Attack | SkeletonAction::Block)
    {
        controller_yaw(controller_orientation) * Vec3::NEG_Z
    } else {
        if linear_velocity.xz().length() <= 0.05 {
            return Quat::from_rotation_y(current_yaw);
        }
        let Some(direction) = linear_velocity.xz().try_normalize() else {
            return Quat::from_rotation_y(current_yaw);
        };
        Vec3::new(direction.x, 0.0, direction.y)
    };
    advance_body_facing_toward(current, desired_forward, delta_seconds, turn_speed_radians)
}

/// Advances the authored body's +Z axis toward an explicit planar target.
/// Attack target acquisition uses this lower-level seam to share the ordinary
/// bounded facing actuator while choosing a target independently of camera yaw.
pub fn advance_body_facing_toward(
    current: Quat,
    desired_forward: Vec3,
    delta_seconds: f32,
    turn_speed_radians: f32,
) -> Quat {
    let current_yaw = body_yaw(current);
    let desired_yaw = desired_forward.x.atan2(desired_forward.z);
    let mut delta = (desired_yaw - current_yaw + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    if (delta + std::f32::consts::PI).abs() <= 1.0e-5 {
        delta = std::f32::consts::PI;
    }
    let maximum = (turn_speed_radians * delta_seconds.max(0.0)).min(std::f32::consts::PI);
    Quat::from_rotation_y(current_yaw + delta.clamp(-maximum, maximum))
}

/// Angular speed required to reach a planar facing by a deadline without an
/// instantaneous turn. Re-evaluating this as the target moves still lands on
/// the live heading at canonical contact.
pub fn body_turn_speed_for_deadline(
    current: Quat,
    desired_forward: Vec3,
    remaining_seconds: f32,
    delta_seconds: f32,
) -> f32 {
    let Some(desired) = desired_forward.xz().try_normalize() else {
        return 0.0;
    };
    let current = (current * Vec3::Z).xz().normalize_or_zero();
    current.angle_to(desired).abs() / remaining_seconds.max(delta_seconds).max(f32::EPSILON)
}

/// Rotates a downed body's fixed head direction toward camera yaw only while
/// the caller keeps the alignment modifier held.
pub fn advance_downed_body_facing(
    current: Quat,
    controller_orientation: Quat,
    delta_seconds: f32,
) -> Quat {
    advance_downed_body_facing_with_speed(
        current,
        controller_orientation,
        delta_seconds,
        downed_turn_speed_radians(),
    )
}

pub fn advance_downed_body_facing_with_speed(
    current: Quat,
    controller_orientation: Quat,
    delta_seconds: f32,
    turn_speed_radians: f32,
) -> Quat {
    let current_yaw = body_yaw(current);
    let desired_forward = controller_yaw(controller_orientation) * Vec3::NEG_Z;
    let desired_yaw = desired_forward.x.atan2(desired_forward.z);
    let mut delta = (desired_yaw - current_yaw + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    if (delta + std::f32::consts::PI).abs() <= 1.0e-5 {
        delta = std::f32::consts::PI;
    }
    let maximum = (turn_speed_radians * delta_seconds.max(0.0)).min(std::f32::consts::PI);
    Quat::from_rotation_y(current_yaw + delta.clamp(-maximum, maximum))
}

fn body_yaw(rotation: Quat) -> f32 {
    let forward = rotation * Vec3::Z;
    forward.x.atan2(forward.z)
}

/// Converts camera yaw into the continuous downed-roll coordinate relative to
/// the body's fixed head direction. A quarter turn is the half-roll pose and a
/// half turn is the opposite contact pose.
pub fn downed_camera_roll_target(body_rotation: Quat, controller_orientation: Quat) -> f32 {
    let body = body_yaw(body_rotation);
    let camera_forward = controller_yaw(controller_orientation) * Vec3::NEG_Z;
    let camera = camera_forward.x.atan2(camera_forward.z);
    let mut delta = (camera - body + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    if (delta + std::f32::consts::PI).abs() <= 1.0e-5 {
        delta = std::f32::consts::PI;
    }
    delta / std::f32::consts::PI
}

/// Projects controller motion into the compact replicated animation state.
/// Bone evaluation remains client-only; this is the shared server seam that
/// keeps deterministic captures on the same stride and posture rules.
pub fn project_skeleton_locomotion(skeleton: &mut SkeletonState, input: SkeletonLocomotionInput) {
    project_skeleton_locomotion_with_intent(skeleton, input, None);
}

/// Server projection variant that preserves held movement intent while the
/// physical motor temporarily brakes to zero between guard contacts.
pub fn project_skeleton_locomotion_with_intent(
    skeleton: &mut SkeletonState,
    input: SkeletonLocomotionInput,
    requested_local_direction: Option<Vec2>,
) {
    let body_rotation = controller_yaw(input.orientation);
    project_skeleton_locomotion_with_body_rotation(
        skeleton,
        input,
        body_rotation,
        requested_local_direction,
    );
}

/// Projection variant for the authoritative server, which owns both the
/// camera/controller frame and the independently rotating body root.
pub fn project_skeleton_locomotion_with_body_rotation(
    skeleton: &mut SkeletonState,
    input: SkeletonLocomotionInput,
    body_rotation: Quat,
    requested_local_direction: Option<Vec2>,
) {
    let linear_velocity = if input.linear_velocity.is_finite() {
        input.linear_velocity
    } else {
        Vec3::ZERO
    };
    let delta_seconds = if input.delta_seconds.is_finite() {
        // Preserve coalesced fixed-step handoffs across a bounded hitch. A
        // quarter-second clamp can erase a complete raised-guard half-step
        // and make sequence identity disagree with phase parity.
        input.delta_seconds.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let previous_world_velocity = skeleton.world_velocity;
    let was_supported = skeleton.body.is_surface_supported();
    let controller_local_velocity = controller_yaw(input.orientation).inverse() * linear_velocity;
    let body_rotation = if body_rotation.is_finite() {
        body_rotation
    } else {
        Quat::IDENTITY
    };
    let body_local_velocity = body_rotation.inverse() * linear_velocity;
    let local_velocity = if skeleton.weapon_guard() == WeaponGuardState::Raised {
        controller_local_velocity
    } else {
        body_local_velocity
    };
    let physical_speed = linear_velocity.xz().length();
    let contiguous_sample = input.tick == skeleton.locomotion_sample_tick.wrapping_add(1);
    skeleton.world_acceleration = if contiguous_sample {
        ((linear_velocity - previous_world_velocity) * locomotion_sample_hz())
            .clamp_length_max(80.0)
    } else {
        Vec3::ZERO
    };
    skeleton.local_velocity = local_velocity;
    skeleton.world_velocity = linear_velocity;
    skeleton.locomotion_sample_tick = input.tick;
    if skeleton.body == BodyState::Ragdolled {
        skeleton.set_raised_locomotion(RaisedLocomotionIntent::planted());
        skeleton.action = ActionState::default();
        return;
    }
    let landed = !was_supported && input.grounded;
    if landed {
        skeleton.landing_sequence = skeleton.landing_sequence.wrapping_add(1);
        skeleton.landing_impact_speed = (-previous_world_velocity.y).max(0.0);
    }
    skeleton.transition_body(if input.grounded {
        match skeleton.body {
            BodyState::Prone | BodyState::Supine => skeleton.body,
            _ => BodyState::Grounded(GroundedPosture::Upright),
        }
    } else {
        BodyState::Airborne
    });
    if landed && skeleton.action_kind() == SkeletonAction::Dodge {
        // Contact ends presentation ownership immediately. Residual horizontal
        // velocity remains authoritative and can continue through ordinary
        // raised locomotion while the server applies its landing drag.
        skeleton.action = ActionState::default();
    }

    let ground_speed = if skeleton.downed_turning() {
        // Turning in place has no physical velocity, but its crawl/scamper
        // cycle should run at twice the former synthetic cadence.
        physical_speed.max(0.8) * 2.0
    } else if matches!(skeleton.body, BodyState::Prone | BodyState::Supine) {
        // Residual impulses and collision correction may translate a downed
        // body, but prone/supine presentation deliberately slides in its idle
        // pose. Velocity-driven locomotion belongs only to upright bodies.
        0.0
    } else {
        physical_speed
    };
    if skeleton.weapon_guard() == WeaponGuardState::Raised && skeleton.posture() == Posture::Upright
    {
        advance_raised_locomotion_intent(skeleton, local_velocity, requested_local_direction);
        advance_guard_footwork(skeleton, delta_seconds, input.tick);
    } else {
        skeleton.set_raised_locomotion(RaisedLocomotionIntent::planted());
        if input.grounded && ground_speed > 0.05 {
            let profile = locomotion_profile(skeleton);
            let phase = skeleton.gait_phase.rem_euclid(1.0);
            let next_phase = phase + gait_cycle_phase_delta(profile, ground_speed, delta_seconds);
            let handoffs = ((next_phase * 2.0).floor() - (phase * 2.0).floor()).max(0.0) as u32;
            skeleton.gait_phase = next_phase.rem_euclid(1.0);
            advance_contact_identity(skeleton, handoffs, None);
            if skeleton.weapon_guard() == WeaponGuardState::Lowered {
                skeleton.lead_foot = if skeleton.gait_phase < 0.5 {
                    LeadFoot::Left
                } else {
                    LeadFoot::Right
                };
            }
        }
    }
    skeleton.advance_action(input.tick);
}

fn advance_contact_identity(
    skeleton: &mut SkeletonState,
    handoffs: u32,
    first_contact: Option<LeadFoot>,
) {
    for handoff in 0..handoffs {
        skeleton.contact_sequence = skeleton.contact_sequence.wrapping_add(1);
        skeleton.contact_foot = if handoff == 0 {
            first_contact.unwrap_or_else(|| opposite_foot(skeleton.contact_foot))
        } else {
            opposite_foot(skeleton.contact_foot)
        };
    }
}

pub(super) fn opposite_foot(foot: LeadFoot) -> LeadFoot {
    match foot {
        LeadFoot::Left => LeadFoot::Right,
        LeadFoot::Right => LeadFoot::Left,
    }
}

fn advance_raised_locomotion_intent(
    skeleton: &mut SkeletonState,
    observed_local_velocity: Vec3,
    requested_local_direction: Option<Vec2>,
) {
    let intent = skeleton.raised_locomotion();
    let observed_speed = observed_local_velocity.xz().length();
    let observed = (observed_speed > 0.05).then(|| {
        RaisedLocomotionIntent::moving(
            Vec2::new(observed_local_velocity.x, observed_local_velocity.z),
            observed_speed,
        )
    });
    let requested = requested_local_direction
        .filter(|direction| direction.is_finite() && direction.length_squared() > f32::EPSILON)
        .map(|direction| direction.normalize());
    let moving = observed.or_else(|| {
        requested
            .map(|direction| RaisedLocomotionIntent::moving(direction, intent.speed().max(0.051)))
    });
    skeleton.set_raised_locomotion(match moving {
        Some(moving) => moving,
        None if skeleton.raised_footwork.step().is_some() => intent,
        None => RaisedLocomotionIntent::planted(),
    });
}

fn advance_guard_footwork(skeleton: &mut SkeletonState, delta_seconds: f32, tick: u64) {
    let displacement = skeleton.local_velocity.xz() * delta_seconds.max(0.0);
    let physical_velocity = skeleton.local_velocity.xz();
    let physical_speed = physical_velocity.length();
    let physical_direction = physical_velocity.normalize_or_zero();
    let mut plan = match skeleton.raised_footwork {
        GuardFootworkPlan::Uninitialized => GuardFootworkPlan::Planted {
            contacts: GuardContacts::default(),
            next_swing: guard_movement_front_foot(
                skeleton.lead_foot,
                skeleton.raised_locomotion().local_direction(),
            ),
        },
        GuardFootworkPlan::Planted {
            contacts,
            next_swing,
        } => GuardFootworkPlan::Planted {
            contacts: contacts.advected(displacement),
            next_swing,
        },
        GuardFootworkPlan::Stepping(step) => {
            GuardFootworkPlan::Stepping(step.advected(displacement))
        }
    };

    if let GuardFootworkPlan::Stepping(mut step) = plan {
        let direction = if physical_direction == Vec2::ZERO {
            step.direction()
        } else {
            physical_direction
        };
        let leading_contact = step
            .contacts
            .left
            .dot(direction)
            .max(step.contacts.right.dot(direction));
        let contact_due = tick >= step.contact_tick || leading_contact <= 0.0;
        if contact_due {
            let contact_margin = guard_footwork_config().contact_margin_metres;
            if step.landing.dot(direction) < contact_margin {
                step.landing += direction * (contact_margin - step.landing.dot(direction));
            }
            let contacts = step.contacts.with_contact(step.swing_foot, step.landing);
            skeleton.contact_foot = step.swing_foot;
            skeleton.contact_sequence = skeleton.contact_sequence.wrapping_add(1);
            plan = GuardFootworkPlan::Planted {
                contacts,
                next_swing: opposite_foot(step.swing_foot),
            };
        }
    }

    if let GuardFootworkPlan::Planted {
        contacts,
        next_swing,
    } = plan
        && physical_speed > 0.05
    {
        plan = GuardFootworkPlan::Stepping(plan_guard_step(
            contacts,
            next_swing,
            physical_velocity,
            tick,
        ));
        skeleton.contact_foot = opposite_foot(next_swing);
    }

    skeleton.gait_phase = match plan {
        GuardFootworkPlan::Stepping(step) => {
            let progress = step.progress(tick).clamp(0.0, 1.0);
            match step.swing_foot {
                LeadFoot::Left => (0.5 + 0.5 * progress).rem_euclid(1.0),
                LeadFoot::Right => 0.5 * progress,
            }
        }
        GuardFootworkPlan::Uninitialized | GuardFootworkPlan::Planted { .. } => {
            match skeleton.contact_foot {
                LeadFoot::Left => 0.0,
                LeadFoot::Right => 0.5,
            }
        }
    };
    skeleton.raised_footwork = plan;
}

fn plan_guard_step(
    contacts: GuardContacts,
    swing_foot: LeadFoot,
    local_velocity: Vec2,
    start_tick: u64,
) -> GuardStepPlan {
    let speed = local_velocity.length().max(0.05);
    let direction = local_velocity.normalize_or_zero();
    let tuning = guard_footwork_config();
    let available_reach = tuning.planning_reach_metres - tuning.contact_margin_metres;
    let reach_seconds = available_reach / speed;
    let leading_contact = contacts
        .left
        .dot(direction)
        .max(contacts.right.dot(direction));
    let support_seconds = if leading_contact > 0.0 {
        leading_contact / speed
    } else {
        1.0 / locomotion_sample_hz()
    };
    let duration_seconds = reach_seconds
        .min(support_seconds)
        .clamp(tuning.minimum_step_seconds, tuning.maximum_step_seconds);
    let duration_ticks = (duration_seconds * locomotion_sample_hz()).ceil().max(1.0) as u64;
    let forward = (speed * duration_ticks as f32 / locomotion_sample_hz()
        + tuning.contact_margin_metres)
        .min(tuning.planning_reach_metres);
    let side = match swing_foot {
        LeadFoot::Left => -tuning.default_half_width_metres,
        LeadFoot::Right => tuning.default_half_width_metres,
    };
    let mut landing = direction * forward + Vec2::X * side;
    let along_shortfall = forward - landing.dot(direction);
    if along_shortfall > 0.0 {
        landing += direction * along_shortfall;
    }
    landing = landing.clamp_length_max(tuning.planning_reach_metres);
    let swing_start = match swing_foot {
        LeadFoot::Left => contacts.left,
        LeadFoot::Right => contacts.right,
    };
    GuardStepPlan {
        contacts,
        swing_foot,
        swing_start,
        landing,
        start_tick,
        contact_tick: start_tick.saturating_add(duration_ticks),
    }
}

// Measured hip-knee-ankle chain of the current humanoid rig. The animation
// client uses its live rig measurement; this is the server-side fallback until
// anatomical dimensions become part of character state.
/// Ground distance covered by one procedural combat-stance contact interval.
pub fn guard_step_length(_speed: f32) -> f32 {
    guard_contact_travel_distance(
        guard_footwork_config().reference_leg_length_metres,
        Vec2::NEG_Y,
    )
}

/// Anatomical foot leading a close-guard shuffle in local controller space.
/// Forward local velocity is -Y; backward therefore swaps the authored lead.
/// A predominantly lateral shuffle uses the foot on the movement side.
pub fn guard_movement_front_foot(lead: LeadFoot, local_direction: Vec2) -> LeadFoot {
    let direction = local_direction.normalize_or_zero();
    if direction == Vec2::ZERO || direction.y.abs() >= direction.x.abs() {
        if direction.y > 0.0 {
            opposite_foot(lead)
        } else {
            lead
        }
    } else if direction.x < 0.0 {
        LeadFoot::Left
    } else {
        LeadFoot::Right
    }
}

/// Maximum lateral open stance. Like the longitudinal opening, this is the
/// leg-length-scaled one-yard reference stance: the moving-side foot reaches
/// outward while the support foot remains behind the projected COM.
pub fn guard_maximum_lateral_foot_separation(leg_length_metres: f32) -> f32 {
    guard_maximum_foot_separation(leg_length_metres)
}

/// Direction-specific open stance, blended continuously for diagonals.
pub fn guard_open_foot_separation(leg_length_metres: f32, local_direction: Vec2) -> f32 {
    let direction = local_direction.normalize_or_zero();
    let longitudinal = direction.y.abs();
    guard_maximum_lateral_foot_separation(leg_length_metres).lerp(
        guard_maximum_foot_separation(leg_length_metres),
        longitudinal,
    )
}

/// Direction-specific closed stance. Lateral shuffles retain normal
/// anatomical width; the three-inch contract applies along the guard's
/// longitudinal axis.
pub fn guard_closed_foot_separation(leg_length_metres: f32, local_direction: Vec2) -> f32 {
    let direction = local_direction.normalize_or_zero();
    let longitudinal = direction.y.abs();
    (leg_length_metres.max(0.0) * 0.25).lerp(
        guard_rear_contact_separation(leg_length_metres),
        longitudinal,
    )
}

/// COM travel between centered open and closed contacts.
pub fn guard_contact_travel_distance(leg_length_metres: f32, local_direction: Vec2) -> f32 {
    (guard_open_foot_separation(leg_length_metres, local_direction)
        - guard_closed_foot_separation(leg_length_metres, local_direction))
    .max(0.0)
        * 0.5
}

/// Maximum fore-aft guard stance immediately before the following foot lifts.
/// The ratio maps a 0.851688 m average-male thigh-plus-shank chain at 5'11" to
/// the requested one-yard stance, then scales directly with the actual rig.
pub fn guard_maximum_foot_separation(leg_length_metres: f32) -> f32 {
    leg_length_metres.max(0.0) * 1.073_632_5
}

/// Maximum fore-aft separation when the rear foot returns beside the front
/// foot. This maps the same 5'11" reference leg to the requested three inches.
pub fn guard_rear_contact_separation(leg_length_metres: f32) -> f32 {
    leg_length_metres.max(0.0) * 0.089_469_37
}
