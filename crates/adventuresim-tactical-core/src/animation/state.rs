use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Posture {
    #[default]
    Upright,
    Crouched,
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

/// A deliberate authored body-to-ground contact. Ragdoll is excluded: it is
/// physics-owned and cannot participate in prone/supine controls or rolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum DownedContact {
    Prone,
    Supine,
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
    Crouched,
}

/// Presentation-only anticipation for a release-triggered jump. This is kept
/// separate from `GroundedPosture::Crouched`: charging a jump must not select
/// crouched locomotion or change the authoritative movement speed.
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
            Self::Grounded(GroundedPosture::Crouched) => Posture::Crouched,
            Self::Airborne => Posture::Airborne,
            Self::Prone => Posture::Prone,
            Self::Supine => Posture::Supine,
            Self::Ragdolled => Posture::Ragdolled,
        }
    }

    pub fn is_downed(self) -> bool {
        matches!(self, Self::Prone | Self::Supine | Self::Ragdolled)
    }

    pub fn downed_contact(self) -> Option<DownedContact> {
        match self {
            Self::Prone => Some(DownedContact::Prone),
            Self::Supine => Some(DownedContact::Supine),
            Self::Grounded(_) | Self::Airborne | Self::Ragdolled => None,
        }
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
/// Speed follows the controller continuously so acceleration changes cadence
/// during the current step. Ordinary turns wait for the next foot handoff;
/// material opposite-direction reversals perform an immediate safe semantic
/// handoff so the support side agrees with the already-reversed gameplay root.
/// This invariant-bearing type intentionally does not implement Bevy reflection;
/// reflected field mutation would bypass its validated constructors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RaisedLocomotionIntent(RaisedLocomotionKind);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RaisedLocomotionKind {
    Planted {
        step_sequence: u32,
    },
    Moving {
        local_direction: Vec2,
        speed: f32,
        swing_foot: LeadFoot,
        step_sequence: u32,
    },
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
            RaisedLocomotionKind::Planted { step_sequence } => Self::planted(step_sequence),
            RaisedLocomotionKind::Moving {
                local_direction,
                speed,
                swing_foot,
                step_sequence,
            } => Self::moving(local_direction, speed, swing_foot, step_sequence),
        })
    }
}

impl Default for RaisedLocomotionIntent {
    fn default() -> Self {
        Self::planted(0)
    }
}

impl RaisedLocomotionIntent {
    pub fn planted(step_sequence: u32) -> Self {
        Self(RaisedLocomotionKind::Planted { step_sequence })
    }

    /// Creates validated moving intent. Invalid or effectively stationary
    /// input becomes planted while retaining its handoff identity.
    pub fn moving(
        local_direction: Vec2,
        speed: f32,
        swing_foot: LeadFoot,
        step_sequence: u32,
    ) -> Self {
        let direction = local_direction.normalize_or_zero();
        if !local_direction.is_finite()
            || !speed.is_finite()
            || direction == Vec2::ZERO
            || speed <= 0.05
        {
            return Self::planted(step_sequence);
        }
        Self(RaisedLocomotionKind::Moving {
            local_direction: direction,
            speed,
            swing_foot,
            step_sequence,
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
            RaisedLocomotionKind::Planted { .. } => Vec2::ZERO,
        }
    }

    pub fn speed(self) -> f32 {
        match self.0 {
            RaisedLocomotionKind::Moving { speed, .. } => speed,
            RaisedLocomotionKind::Planted { .. } => 0.0,
        }
    }

    pub fn swing_foot(self) -> Option<LeadFoot> {
        match self.0 {
            RaisedLocomotionKind::Moving { swing_foot, .. } => Some(swing_foot),
            RaisedLocomotionKind::Planted { .. } => None,
        }
    }

    pub fn step_sequence(self) -> u32 {
        match self.0 {
            RaisedLocomotionKind::Planted { step_sequence }
            | RaisedLocomotionKind::Moving { step_sequence, .. } => step_sequence,
        }
    }

    pub fn local_velocity(self) -> Vec3 {
        let direction = self.local_direction();
        Vec3::new(direction.x, 0.0, direction.y) * self.speed()
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostureTransitionKind {
    UprightToProne,
    ProneToUpright,
    ProneToSupine { direction: RollDirection },
    SupineToProne { direction: RollDirection },
    SupineToUpright,
    DiveToDowned { direction: DiveDirection },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum TimedPostureTransitionKind {
    UprightToProne,
    ProneToUpright,
    ProneToSupine { direction: RollDirection },
    SupineToProne { direction: RollDirection },
    SupineToUpright,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum PostureTransitionProgress {
    Timed {
        kind: TimedPostureTransitionKind,
        start_tick: u64,
        duration_ticks: u64,
        phase: f32,
    },
    Dive {
        direction: DiveDirection,
        start_tick: u64,
        duration_ticks: u64,
        phase: f32,
        was_airborne: bool,
        landing_tick: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PostureTransitionState(PostureTransitionProgress);

impl<'de> Deserialize<'de> for PostureTransitionState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(PostureTransitionProgress::deserialize(deserializer)?).normalized())
    }
}

impl PostureTransitionState {
    fn new(kind: PostureTransitionKind, start_tick: u64, duration_ticks: u64) -> Self {
        let duration_ticks = duration_ticks.max(1);
        match kind {
            PostureTransitionKind::DiveToDowned { direction } => {
                Self(PostureTransitionProgress::Dive {
                    direction,
                    start_tick,
                    duration_ticks,
                    phase: 0.0,
                    was_airborne: false,
                    landing_tick: None,
                })
            }
            kind => Self(PostureTransitionProgress::Timed {
                kind: match kind {
                    PostureTransitionKind::UprightToProne => {
                        TimedPostureTransitionKind::UprightToProne
                    }
                    PostureTransitionKind::ProneToUpright => {
                        TimedPostureTransitionKind::ProneToUpright
                    }
                    PostureTransitionKind::ProneToSupine { direction } => {
                        TimedPostureTransitionKind::ProneToSupine { direction }
                    }
                    PostureTransitionKind::SupineToProne { direction } => {
                        TimedPostureTransitionKind::SupineToProne { direction }
                    }
                    PostureTransitionKind::SupineToUpright => {
                        TimedPostureTransitionKind::SupineToUpright
                    }
                    PostureTransitionKind::DiveToDowned { .. } => unreachable!(),
                },
                start_tick,
                duration_ticks,
                phase: 0.0,
            }),
        }
    }

    fn normalized(self) -> Self {
        let normalized_phase = |phase: f32| {
            if phase.is_finite() {
                phase.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        Self(match self.0 {
            PostureTransitionProgress::Timed {
                kind,
                start_tick,
                duration_ticks,
                phase,
            } => PostureTransitionProgress::Timed {
                kind,
                start_tick,
                duration_ticks: duration_ticks.max(1),
                phase: normalized_phase(phase),
            },
            PostureTransitionProgress::Dive {
                direction,
                start_tick,
                duration_ticks,
                phase,
                was_airborne,
                landing_tick,
            } => PostureTransitionProgress::Dive {
                direction,
                start_tick,
                duration_ticks: duration_ticks.max(1),
                phase: normalized_phase(phase),
                was_airborne,
                landing_tick: landing_tick.map(|tick| tick.max(start_tick)),
            },
        })
    }

    pub fn kind(self) -> PostureTransitionKind {
        match self.0 {
            PostureTransitionProgress::Timed { kind, .. } => match kind {
                TimedPostureTransitionKind::UprightToProne => PostureTransitionKind::UprightToProne,
                TimedPostureTransitionKind::ProneToUpright => PostureTransitionKind::ProneToUpright,
                TimedPostureTransitionKind::ProneToSupine { direction } => {
                    PostureTransitionKind::ProneToSupine { direction }
                }
                TimedPostureTransitionKind::SupineToProne { direction } => {
                    PostureTransitionKind::SupineToProne { direction }
                }
                TimedPostureTransitionKind::SupineToUpright => {
                    PostureTransitionKind::SupineToUpright
                }
            },
            PostureTransitionProgress::Dive { direction, .. } => {
                PostureTransitionKind::DiveToDowned { direction }
            }
        }
    }

    pub fn phase(self) -> f32 {
        match self.0 {
            PostureTransitionProgress::Timed { phase, .. }
            | PostureTransitionProgress::Dive { phase, .. } => phase,
        }
    }

    fn normalized_for_body(mut self, body: BodyState) -> Option<Self> {
        let kind = self.kind();
        if matches!(self.0, PostureTransitionProgress::Timed { .. }) {
            return kind.accepts(body).then_some(self);
        }
        let PostureTransitionProgress::Dive {
            phase,
            was_airborne,
            landing_tick,
            ..
        } = &mut self.0
        else {
            unreachable!("timed transition returned above")
        };
        match body {
            BodyState::Airborne => {
                *was_airborne = true;
                *landing_tick = None;
                *phase = 0.5;
                Some(self)
            }
            BodyState::Grounded(_) => {
                if landing_tick.is_some() {
                    *was_airborne = true;
                    *phase = (*phase).max(0.5);
                } else if *was_airborne {
                    *phase = 0.5;
                } else {
                    *phase = (*phase).min(0.5);
                }
                Some(self)
            }
            BodyState::Prone | BodyState::Supine | BodyState::Ragdolled => None,
        }
    }

    /// Returns the authored dive recovery progress after terrain contact.
    /// The first half of a dive is duck-to-airborne and remains fixed at its
    /// airborne endpoint until impact; only the second half transfers the
    /// directional pose into its canonical downed contact pose.
    pub fn dive_recovery(self) -> Option<(DiveDirection, f32)> {
        match self.0 {
            PostureTransitionProgress::Dive {
                direction, phase, ..
            } => Some((direction, ((phase - 0.5) * 2.0).clamp(0.0, 1.0))),
            PostureTransitionProgress::Timed { .. } => None,
        }
    }
}

/// Incremental root-yaw handoff that cancels the authored dive pose's return
/// to canonical forward during landing recovery. Applying this after each
/// posture-transition advance keeps the character's world-space head-to-feet
/// direction fixed from contact through the final downed pose.
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
        // The authored backward-dive-to-supine span resolves its ambiguous
        // half turn through positive yaw. Transfer the root through the
        // equivalent negative branch so the two rotations cancel instead of
        // composing into a visible full flip.
        DiveDirection::Backward => -std::f32::consts::PI,
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

const MINIMUM_ATTACK_VISUAL_TICKS: u64 = 40;

impl ActionTimeline {
    fn new(start_tick: u64, contact_tick: u64) -> Self {
        Self::with_recovery(
            start_tick,
            contact_tick,
            contact_tick.saturating_sub(start_tick),
        )
    }

    fn with_recovery(start_tick: u64, contact_tick: u64, recovery_ticks: u64) -> Self {
        Self {
            start_tick,
            preparation_ticks: contact_tick.saturating_sub(start_tick).max(1),
            recovery_ticks: recovery_ticks.max(1),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum ActionKind {
    Idle,
    Dodge {
        direction: Vec2,
        timeline: ActionTimeline,
    },
    Attack {
        target_height: f32,
        animation: AttackAnimation,
        timeline: ActionTimeline,
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
pub(crate) struct ActionState(ActionKind);

impl<'de> Deserialize<'de> for ActionState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let kind = ActionKind::deserialize(deserializer)?;
        Ok(Self(match kind {
            ActionKind::Idle => ActionKind::Idle,
            ActionKind::Dodge {
                direction,
                timeline,
            } => ActionKind::Dodge {
                direction: if direction.is_finite() {
                    direction.normalize_or_zero()
                } else {
                    Vec2::ZERO
                },
                timeline: timeline.normalized(),
            },
            ActionKind::Attack {
                target_height,
                animation,
                timeline,
            } => ActionKind::Attack {
                target_height: if target_height.is_finite() {
                    target_height.clamp(0.0, 1.0)
                } else {
                    AttackSpec::default().target_height
                },
                animation,
                timeline: timeline.normalized(),
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct DodgeSpec {
    pub direction: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AttackSpec {
    pub target_height: f32,
    pub animation: AttackAnimation,
}

impl Default for AttackSpec {
    fn default() -> Self {
        Self {
            target_height: 0.5,
            animation: AttackAnimation::Thrust,
        }
    }
}

impl AttackSpec {
    pub fn new(animation: AttackAnimation) -> Self {
        Self {
            animation,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionTimelineView {
    pub start_tick: u64,
    pub preparation_ticks: u64,
    pub recovery_ticks: u64,
    pub phase: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionView {
    Dodge {
        direction: Vec2,
        timeline: ActionTimelineView,
    },
    Attack {
        target_height: f32,
        animation: AttackAnimation,
        timeline: ActionTimelineView,
    },
    Block {
        incoming_line: AttackLine,
        timeline: ActionTimelineView,
    },
}

impl ActionTimeline {
    fn view(self) -> ActionTimelineView {
        ActionTimelineView {
            start_tick: self.start_tick,
            preparation_ticks: self.preparation_ticks,
            recovery_ticks: self.recovery_ticks,
            phase: self.phase,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedAction {
    kind: SkeletonAction,
    start_tick: u64,
}

impl AcceptedAction {
    pub fn kind(self) -> SkeletonAction {
        self.kind
    }

    pub fn start_tick(self) -> u64 {
        self.start_tick
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionAdmissionError {
    BodyCannotAct(BodyState),
    PostureTransitionInProgress,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum StrikeFamily {
    #[default]
    Thrust,
    Swing,
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
    SwingFollow,
    #[default]
    Thrust,
}

impl AttackAnimation {
    pub fn strike_family(self) -> StrikeFamily {
        match self {
            Self::Swing | Self::SwingFollow => StrikeFamily::Swing,
            Self::Thrust => StrikeFamily::Thrust,
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
    pub swing_follow: bool,
    pub thrust: bool,
}

impl AttackAnimations {
    pub const NONE: Self = Self {
        swing: false,
        swing_follow: false,
        thrust: false,
    };
    pub const fn supports(self, animation: AttackAnimation) -> bool {
        match animation {
            AttackAnimation::Swing => self.swing,
            AttackAnimation::SwingFollow => self.swing_follow,
            AttackAnimation::Thrust => self.thrust,
        }
    }

    pub const fn supports_family(self, family: StrikeFamily) -> bool {
        self.supports(AttackAnimation::initial(family))
    }

    pub const fn any(self) -> bool {
        self.swing || self.swing_follow || self.thrust
    }
}

impl Default for AttackAnimations {
    fn default() -> Self {
        Self {
            swing: true,
            swing_follow: true,
            thrust: true,
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
    pub landing_sequence: u64,
    pub landing_impact_speed: f32,
    lead_foot: LeadFoot,
    guarded_sprint_locomotion: bool,
    stance: StanceState,
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
    landing_sequence: u64,
    landing_impact_speed: f32,
    lead_foot: LeadFoot,
    guarded_sprint_locomotion: bool,
    stance: StanceState,
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
            landing_sequence: wire.landing_sequence,
            landing_impact_speed: if wire.landing_impact_speed.is_finite() {
                wire.landing_impact_speed.max(0.0)
            } else {
                0.0
            },
            lead_foot: wire.lead_foot,
            guarded_sprint_locomotion: wire.guarded_sprint_locomotion,
            stance: wire.stance,
            action: wire.action,
            posture_transition: wire
                .posture_transition
                .and_then(|transition| transition.normalized_for_body(wire.body)),
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
            landing_sequence: 0,
            landing_impact_speed: 0.0,
            lead_foot: LeadFoot::Left,
            guarded_sprint_locomotion: false,
            stance: StanceState::Lowered,
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
        }
        (StanceState::Lowered, WeaponGuardState::Raised) => {}
        (StanceState::Raised { .. }, WeaponGuardState::Lowered) => {
            skeleton.stance = StanceState::Lowered;
            skeleton.guarded_sprint_locomotion = false;
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
    pub fn lead_foot(&self) -> LeadFoot {
        self.lead_foot
    }
    /// Overrides replicated semantic support only in a client presentation
    /// fixture. Gameplay authority must use locomotion observations instead.
    pub fn set_presentation_shadow_lead_foot(&mut self, lead: LeadFoot) {
        self.lead_foot = lead;
    }
    /// Atomically changes physical mode. Incompatible authored transitions are
    /// cancelled; a dive alone retains its explicit grounded/airborne stages.
    /// Raised movement is valid only while grounded upright. Entering authored
    /// downed contact or ragdoll also cancels presentation actions.
    pub fn transition_body(&mut self, body: BodyState) {
        self.posture_transition = self
            .posture_transition
            .and_then(|transition| transition.normalized_for_body(body));
        self.body = body;
        if body != BodyState::Grounded(GroundedPosture::Upright) {
            self.jump_anticipation = JumpAnticipation::Inactive;
        }
        if body.downed_contact().is_none() {
            self.downed_facing = None;
            self.downed_turning = false;
        }
        if body != BodyState::Grounded(GroundedPosture::Upright) {
            self.guarded_sprint_locomotion = false;
        }
        if body.is_downed() || self.posture_transition.is_some() {
            self.stance = StanceState::Lowered;
            self.action = ActionState::default();
            self.jump_anticipation = JumpAnticipation::Inactive;
            self.guarded_sprint_locomotion = false;
        }
        if body == BodyState::Ragdolled {
            self.downed_turning = false;
        } else if body != BodyState::Grounded(GroundedPosture::Upright)
            && let StanceState::Raised { locomotion } = self.stance
            && locomotion.is_moving()
        {
            self.stance = StanceState::Raised {
                locomotion: RaisedLocomotionIntent::planted(locomotion.step_sequence()),
            };
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
    fn set_raised_locomotion(&mut self, locomotion: RaisedLocomotionIntent) {
        if matches!(self.stance, StanceState::Raised { .. }) {
            let locomotion = if self.body == BodyState::Grounded(GroundedPosture::Upright) {
                locomotion
            } else {
                RaisedLocomotionIntent::planted(locomotion.step_sequence())
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
        self.downed_turning
            && self.body.downed_contact().is_some()
            && self.posture_transition.is_none()
    }
    pub fn set_downed_turning(&mut self, turning: bool) {
        self.downed_turning =
            turning && self.body.downed_contact().is_some() && self.posture_transition.is_none();
    }
    pub fn downed_lateral_motion(&self) -> f32 {
        if let Some(transition) = self.posture_transition {
            return match transition.kind() {
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
        let transition_kind = transition.kind();
        match &mut transition.0 {
            PostureTransitionProgress::Dive {
                direction,
                start_tick,
                duration_ticks,
                phase,
                was_airborne,
                landing_tick,
            } => {
                let kind = PostureTransitionKind::DiveToDowned {
                    direction: *direction,
                };
                let elapsed = current_tick.saturating_sub(*start_tick);
                if !self.body.is_surface_supported() {
                    *was_airborne = true;
                    *phase = 0.5;
                } else if *was_airborne {
                    let landed_at = *landing_tick.get_or_insert(current_tick);
                    let recovery_elapsed = current_tick.saturating_sub(landed_at);
                    if recovery_elapsed >= *duration_ticks {
                        self.finish_posture_transition(kind);
                        return;
                    }
                    *phase = 0.5 + 0.5 * recovery_elapsed as f32 / *duration_ticks as f32;
                } else if elapsed >= *duration_ticks {
                    self.finish_posture_transition(kind);
                    return;
                } else {
                    *phase = 0.5 * elapsed as f32 / *duration_ticks as f32;
                }
            }
            PostureTransitionProgress::Timed {
                kind: _,
                start_tick,
                duration_ticks,
                phase,
            } => {
                let elapsed = current_tick.saturating_sub(*start_tick);
                if elapsed >= *duration_ticks {
                    self.posture_transition = None;
                    self.transition_body(transition_kind.target());
                    return;
                }
                *phase = elapsed as f32 / *duration_ticks as f32;
            }
        }
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
            } => Some(DownedFacingState {
                half_turns: -0.5,
                target: DownedFacingPose::RollLeft,
                lateral_motion: 0.0,
            }),
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Right,
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
        if self.body.downed_contact().is_none() || self.posture_transition.is_some() {
            self.downed_facing = None;
            return false;
        }
        let camera_target = if camera_target_half_turns.is_finite() {
            camera_target_half_turns
        } else {
            0.0
        };
        let Some(contact) = self.body.downed_contact() else {
            self.downed_facing = None;
            return false;
        };
        let initial_target = match contact {
            DownedContact::Prone => DownedFacingPose::Prone,
            DownedContact::Supine => DownedFacingPose::Supine,
        };
        let initial = initial_target.half_turns_near(camera_target);
        let previous = self.downed_facing;
        let current = previous
            .map(DownedFacingState::half_turns)
            .unwrap_or(initial);
        let target = if aim_held {
            const SECTOR_HALF_WIDTH: f32 = 0.25;
            const EDGE_STICKINESS: f32 = 1.0 / 18.0; // ten degrees
            let committed = previous.map(|state| state.target).unwrap_or(initial_target);
            let committed_half_turns = committed.half_turns_near(current);
            let camera_unwrapped =
                camera_target + ((committed_half_turns - camera_target) / 2.0).round() * 2.0;
            let target_pose = if (camera_unwrapped - committed_half_turns).abs()
                > SECTOR_HALF_WIDTH + EDGE_STICKINESS
            {
                DownedFacingPose::from_half_turns(camera_unwrapped)
            } else {
                committed
            };
            target_pose.half_turns_near(camera_unwrapped)
        } else {
            let lower = current.floor();
            if (current - lower - 0.5).abs() <= 1.0e-4 {
                match contact {
                    DownedContact::Prone => (current / 2.0).round() * 2.0,
                    DownedContact::Supine => ((current - 1.0) / 2.0).round() * 2.0 + 1.0,
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
    pub fn action_kind(&self) -> SkeletonAction {
        self.action.kind()
    }
    pub fn action_phase(&self) -> f32 {
        self.action.phase()
    }
    pub fn action_view(&self) -> Option<ActionView> {
        match self.action {
            ActionState(ActionKind::Idle) => None,
            ActionState(ActionKind::Dodge {
                direction,
                timeline,
            }) => Some(ActionView::Dodge {
                direction,
                timeline: timeline.view(),
            }),
            ActionState(ActionKind::Attack {
                target_height,
                animation,
                timeline,
            }) => Some(ActionView::Attack {
                target_height,
                animation,
                timeline: timeline.view(),
            }),
            ActionState(ActionKind::Block {
                incoming_line,
                timeline,
            }) => Some(ActionView::Block {
                incoming_line,
                timeline: timeline.view(),
            }),
        }
    }
    pub fn dodge_view(&self) -> Option<(Vec2, ActionTimelineView)> {
        match self.action_view()? {
            ActionView::Dodge {
                direction,
                timeline,
            } => Some((direction, timeline)),
            ActionView::Attack { .. } | ActionView::Block { .. } => None,
        }
    }
    pub fn attack_view(&self) -> Option<(f32, AttackAnimation, ActionTimelineView)> {
        match self.action_view()? {
            ActionView::Attack {
                target_height,
                animation,
                timeline,
            } => Some((target_height, animation, timeline)),
            ActionView::Dodge { .. } | ActionView::Block { .. } => None,
        }
    }
    pub fn block_view(&self) -> Option<(AttackLine, ActionTimelineView)> {
        match self.action_view()? {
            ActionView::Block {
                incoming_line,
                timeline,
            } => Some((incoming_line, timeline)),
            ActionView::Dodge { .. } | ActionView::Attack { .. } => None,
        }
    }
    pub fn attack_animation(&self) -> Option<AttackAnimation> {
        self.attack_view().map(|(_, animation, _)| animation)
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
    /// attack starts only from recovery-complete idle. A second swing may
    /// replace the first after contact when the pack owns a follow pose.
    pub fn select_attack_animation(&self, family: StrikeFamily) -> Option<AttackAnimation> {
        let initial = AttackAnimation::initial(family);
        match self.attack_animation() {
            None => self.attack_animations.supports(initial).then_some(initial),
            Some(AttackAnimation::Swing)
                if family == StrikeFamily::Swing
                    && self.action_phase() >= 0.5
                    && self.attack_animations.swing_follow =>
            {
                Some(AttackAnimation::SwingFollow)
            }
            _ => None,
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
        self.dodge_view()
            .is_some_and(|(_, timeline)| timeline.phase >= 0.125)
    }

    /// Replaces the current action. This deliberately preserves the existing
    /// last-writer-wins compatibility policy until gameplay defines rejection
    /// or cancellation rules between actions.
    fn replace_action(
        &mut self,
        action: ActionState,
        start_tick: u64,
    ) -> Result<AcceptedAction, ActionAdmissionError> {
        if self.posture_transition.is_some() {
            return Err(ActionAdmissionError::PostureTransitionInProgress);
        }
        if self.body.is_downed() {
            return Err(ActionAdmissionError::BodyCannotAct(self.body));
        }
        let kind = action.kind();
        self.action = action;
        Ok(AcceptedAction { kind, start_tick })
    }

    pub fn begin_dodge(
        &mut self,
        spec: DodgeSpec,
        start_tick: u64,
        contact_tick: u64,
    ) -> Result<AcceptedAction, ActionAdmissionError> {
        let timeline = ActionTimeline::new(start_tick, contact_tick);
        let direction = if spec.direction.is_finite() {
            spec.direction.normalize_or_zero()
        } else {
            Vec2::ZERO
        };
        self.replace_action(
            ActionState(ActionKind::Dodge {
                direction,
                timeline,
            }),
            start_tick,
        )
    }

    pub fn begin_attack(
        &mut self,
        spec: AttackSpec,
        start_tick: u64,
        contact_tick: u64,
    ) -> Result<AcceptedAction, ActionAdmissionError> {
        let target_height = if spec.target_height.is_finite() {
            spec.target_height.clamp(0.0, 1.0)
        } else {
            AttackSpec::default().target_height
        };
        let preparation_ticks = contact_tick.saturating_sub(start_tick).max(1);
        let recovery_ticks =
            preparation_ticks.max(MINIMUM_ATTACK_VISUAL_TICKS.saturating_sub(preparation_ticks));
        self.replace_action(
            ActionState(ActionKind::Attack {
                target_height,
                animation: spec.animation,
                timeline: ActionTimeline::with_recovery(start_tick, contact_tick, recovery_ticks),
            }),
            start_tick,
        )
    }

    /// Clears only the transient action timeline so a client presentation
    /// replica can rebuild that action on its own monotonic visual clock.
    /// Gameplay authority must continue to use the typed begin/advance APIs.
    pub fn clear_action_for_presentation(&mut self) {
        self.action = ActionState::default();
    }

    pub fn begin_block(
        &mut self,
        spec: BlockSpec,
        start_tick: u64,
        contact_tick: u64,
    ) -> Result<AcceptedAction, ActionAdmissionError> {
        self.replace_action(
            ActionState(ActionKind::Block {
                incoming_line: spec.incoming_line,
                timeline: ActionTimeline::new(start_tick, contact_tick),
            }),
            start_tick,
        )
    }

    /// Advances an action with independently authored preparation and recovery
    /// spans. Gameplay contact can be early without truncating visual recovery.
    pub fn advance_action(&mut self, current_tick: u64) {
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
    pub crouching: bool,
    pub delta_seconds: f32,
    pub tick: u64,
}

/// Maximum server-authoritative body turn speed during ordinary locomotion.
pub const BODY_TURN_SPEED_RADIANS: f32 = std::f32::consts::PI / 0.25;
/// Deliberate head-direction alignment speed while prone or supine.
pub const DOWNED_TURN_SPEED_RADIANS: f32 = std::f32::consts::FRAC_PI_2;

/// Returns the controller's yaw without allowing camera pitch or roll to tilt
/// planar locomotion into or out of the ground plane.
pub fn controller_yaw(orientation: Quat) -> Quat {
    let forward = orientation * Vec3::NEG_Z;
    let Some(flat_forward) = forward.xz().try_normalize() else {
        return Quat::IDENTITY;
    };
    Quat::from_rotation_y((-flat_forward.x).atan2(-flat_forward.y))
}

/// Root orientation committed when an authored directional dive launches.
/// Dive travel and pose selection are both camera-relative, so they must
/// capture the same controller frame before posture-transition facing locks.
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
    let desired_yaw = desired_forward.x.atan2(desired_forward.z);
    let mut delta = (desired_yaw - current_yaw + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    if (delta + std::f32::consts::PI).abs() <= 1.0e-5 {
        delta = std::f32::consts::PI;
    }
    let maximum = (BODY_TURN_SPEED_RADIANS * delta_seconds.max(0.0)).min(std::f32::consts::PI);
    Quat::from_rotation_y(current_yaw + delta.clamp(-maximum, maximum))
}

/// Rotates a downed body's fixed head direction toward camera yaw only while
/// the caller keeps the alignment modifier held.
pub fn advance_downed_body_facing(
    current: Quat,
    controller_orientation: Quat,
    delta_seconds: f32,
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
    let maximum = (DOWNED_TURN_SPEED_RADIANS * delta_seconds.max(0.0)).min(std::f32::consts::PI);
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
    if skeleton.body == BodyState::Ragdolled {
        return;
    }
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
    let previous_guard_sequence = skeleton.raised_locomotion().step_sequence();
    let previous_guard_swing = skeleton.raised_locomotion().swing_foot();
    let local_velocity = controller_yaw(input.orientation).inverse() * linear_velocity;
    let physical_speed = local_velocity.xz().length();
    let contiguous_sample = input.tick == skeleton.locomotion_sample_tick.wrapping_add(1);
    skeleton.world_acceleration = if contiguous_sample {
        ((linear_velocity - previous_world_velocity) * LOCOMOTION_SAMPLE_HZ).clamp_length_max(80.0)
    } else {
        Vec3::ZERO
    };
    skeleton.local_velocity = local_velocity;
    skeleton.world_velocity = linear_velocity;
    skeleton.locomotion_sample_tick = input.tick;
    let landed = !was_supported && input.grounded;
    if landed {
        skeleton.landing_sequence = skeleton.landing_sequence.wrapping_add(1);
        skeleton.landing_impact_speed = (-previous_world_velocity.y).max(0.0);
    }
    skeleton.transition_body(if input.grounded {
        match skeleton.body {
            BodyState::Prone | BodyState::Supine => skeleton.body,
            _ if input.crouching => BodyState::Grounded(GroundedPosture::Crouched),
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
    } else if skeleton.body == BodyState::Prone {
        // Crawl pace is selected authoritatively; contacts follow the resulting
        // physical travel directly without the former two-thirds lag.
        physical_speed
    } else if skeleton.body == BodyState::Supine {
        // Supine retains its deliberately less literal scamper cadence.
        physical_speed * (2.0 / 3.0)
    } else {
        physical_speed
    };
    if skeleton.weapon_guard() == WeaponGuardState::Raised && skeleton.posture() == Posture::Upright
    {
        advance_raised_locomotion_intent(skeleton, local_velocity, delta_seconds);
        let handoffs = skeleton
            .raised_locomotion()
            .step_sequence()
            .wrapping_sub(previous_guard_sequence);
        advance_contact_identity(skeleton, handoffs, previous_guard_swing);
    } else {
        skeleton.set_raised_locomotion(RaisedLocomotionIntent::planted(previous_guard_sequence));
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
    delta_seconds: f32,
) {
    let mut intent = skeleton.raised_locomotion();
    let observed_speed = observed_local_velocity.xz().length();
    let observed = (observed_speed > 0.05).then(|| {
        RaisedLocomotionIntent::moving(
            Vec2::new(observed_local_velocity.x, observed_local_velocity.z),
            observed_speed,
            skeleton.lead_foot,
            intent.step_sequence(),
        )
    });
    if !intent.is_moving() {
        let Some(observed) = observed else {
            skeleton.gait_phase = 0.0;
            skeleton.set_raised_locomotion(intent);
            return;
        };
        intent = RaisedLocomotionIntent::moving(
            observed.local_direction(),
            observed.speed(),
            initial_guard_swing_foot(observed.local_direction(), skeleton.lead_foot),
            observed.step_sequence(),
        );
        skeleton.gait_phase = 0.0;
    }

    if let Some(observed) = observed {
        // Do not latch the tiny velocity from the first acceleration tick for
        // a complete pulse. Cadence and reach adapt immediately; only a hard
        // direction change waits until the current swing foot lands.
        let mut direction = intent.local_direction();
        let mut swing_foot = intent.swing_foot().unwrap_or(skeleton.lead_foot);
        let mut step_sequence = intent.step_sequence();
        if direction.dot(observed.local_direction()) < -0.5 {
            // Gameplay root velocity reverses immediately. Hand support off
            // immediately too, rather than dragging the old world plant
            // across its anatomical corridor until the scheduled seam.
            direction = observed.local_direction();
            swing_foot = match swing_foot {
                LeadFoot::Left => LeadFoot::Right,
                LeadFoot::Right => LeadFoot::Left,
            };
            step_sequence = step_sequence.wrapping_add(1);
            intent = RaisedLocomotionIntent::moving(
                direction,
                observed.speed(),
                swing_foot,
                step_sequence,
            );
            skeleton.gait_phase = if skeleton.gait_phase < 0.5 { 0.5 } else { 0.0 };
            skeleton.set_raised_locomotion(intent);
            return;
        }
        intent =
            RaisedLocomotionIntent::moving(direction, observed.speed(), swing_foot, step_sequence);
    }
    let speed = intent.speed();
    let phase = skeleton.gait_phase.rem_euclid(1.0);
    let profile = LocomotionProfile {
        step_distance: guard_step_length(speed),
        ..RAISED_GUARD_LOCOMOTION_PROFILE
    };
    let next_phase = phase + gait_cycle_phase_delta(profile, speed, delta_seconds.max(0.0));
    let handoffs = ((next_phase * 2.0).floor() - (phase * 2.0).floor()).max(0.0) as u32;
    let crossed_handoff = handoffs > 0;

    if observed.is_none() && crossed_handoff {
        let step_sequence = intent.step_sequence().wrapping_add(1);
        intent = RaisedLocomotionIntent::planted(step_sequence);
        skeleton.gait_phase = if phase < 0.5 { 0.5 } else { 0.0 };
        skeleton.set_raised_locomotion(intent);
        return;
    }

    skeleton.gait_phase = next_phase.rem_euclid(1.0);
    if crossed_handoff && let Some(observed) = observed {
        if handoffs % 2 == 1 {
            let swing_foot = match intent.swing_foot().unwrap_or(skeleton.lead_foot) {
                LeadFoot::Left => LeadFoot::Right,
                LeadFoot::Right => LeadFoot::Left,
            };
            intent = RaisedLocomotionIntent::moving(
                observed.local_direction(),
                observed.speed(),
                swing_foot,
                intent.step_sequence(),
            );
        }
        intent = RaisedLocomotionIntent::moving(
            observed.local_direction(),
            observed.speed(),
            intent.swing_foot().unwrap_or(skeleton.lead_foot),
            intent.step_sequence().wrapping_add(handoffs),
        );
    }
    skeleton.set_raised_locomotion(intent);
}

pub(super) fn initial_guard_swing_foot(direction: Vec2, lead: LeadFoot) -> LeadFoot {
    if direction.x.abs() >= direction.y.abs() {
        if direction.x.is_sign_negative() {
            LeadFoot::Left
        } else {
            LeadFoot::Right
        }
    } else if direction.y.is_sign_positive() {
        // Retreat begins with the forward foot; advance begins with the rear.
        lead
    } else {
        match lead {
            LeadFoot::Left => LeadFoot::Right,
            LeadFoot::Right => LeadFoot::Left,
        }
    }
}

/// Ground distance covered by one procedural combat-stance step. Raised
/// movement uses compact shuffles rather than ordinary walking strides.
pub fn guard_step_length(speed: f32) -> f32 {
    (0.26 + speed.max(0.0) * 0.06).clamp(0.28, 0.42)
}
