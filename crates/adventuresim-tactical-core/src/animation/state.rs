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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StanceState {
    Lowered,
    Raised { locomotion: RaisedLocomotionIntent },
}

impl Default for StanceState {
    fn default() -> Self {
        Self::Lowered
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct ActionTimeline {
    start_tick: u64,
    preparation_ticks: u64,
    phase: f32,
}

impl ActionTimeline {
    fn new(start_tick: u64, contact_tick: u64) -> Self {
        Self {
            start_tick,
            preparation_ticks: contact_tick.saturating_sub(start_tick).max(1),
            phase: 0.0,
        }
    }

    fn normalized(mut self) -> Self {
        self.preparation_ticks = self.preparation_ticks.max(1);
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
        strike_family: StrikeFamily,
        step: AttackStep,
        step_speed: f32,
        movement_direction: Vec2,
        movement_speed: f32,
        start_lead: LeadFoot,
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
pub struct ActionState(ActionKind);

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
                strike_family,
                step,
                step_speed,
                movement_direction,
                movement_speed,
                start_lead,
                timeline,
            } => ActionKind::Attack {
                target_height: if target_height.is_finite() {
                    target_height.clamp(0.0, 1.0)
                } else {
                    AttackSpec::default().target_height
                },
                strike_family,
                step,
                step_speed: if step_speed.is_finite() {
                    step_speed.clamp(0.0, 8.0)
                } else {
                    0.0
                },
                movement_direction: if movement_direction.is_finite() {
                    movement_direction.normalize_or_zero()
                } else {
                    Vec2::ZERO
                },
                movement_speed: if movement_speed.is_finite() {
                    movement_speed.clamp(0.0, 8.0)
                } else {
                    0.0
                },
                start_lead,
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
    pub strike_family: StrikeFamily,
    /// Longitudinal step snapshotted from authoritative movement velocity at
    /// attack start. It is semantic rather than a free vector so later input
    /// or facing changes cannot repick the attacking foot.
    pub step: AttackStep,
    pub step_speed: f32,
    /// Controller-local physical travel captured with the attack. Gameplay
    /// holds this direction until recovery ends; later input is retained for
    /// the next ordinary locomotion tick instead of steering the attack.
    pub movement_direction: Vec2,
    pub movement_speed: f32,
}

impl Default for AttackSpec {
    fn default() -> Self {
        Self {
            target_height: 0.5,
            strike_family: StrikeFamily::Thrust,
            step: AttackStep::Stay,
            step_speed: 0.0,
            movement_direction: Vec2::ZERO,
            movement_speed: 0.0,
        }
    }
}

impl AttackSpec {
    /// Builds melee footwork from the controller-local velocity already
    /// observed by the authority. Forward is Bevy's conventional -Z axis;
    /// lateral-only and tiny longitudinal motion deliberately remain planted.
    pub fn melee_from_local_velocity(local_velocity: Vec3) -> Self {
        let step = AttackStep::from_local_velocity(local_velocity);
        Self {
            step,
            step_speed: if local_velocity.is_finite() {
                local_velocity.z.abs().clamp(0.0, 8.0)
            } else {
                0.0
            },
            movement_direction: if local_velocity.is_finite() {
                // Ahoy's controller input uses +Y for forward while Bevy
                // local velocity uses -Z. Store the captured physical input
                // direction, not an unconverted X/Z projection, because the
                // server feeds this value back to the controller during the
                // attack preparation.
                Vec2::new(local_velocity.x, -local_velocity.z).normalize_or_zero()
            } else {
                Vec2::ZERO
            },
            movement_speed: if local_velocity.is_finite() {
                local_velocity.xz().length().clamp(0.0, 8.0)
            } else {
                0.0
            },
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
    Slash,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Footwork {
    #[default]
    Stay,
    Switch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum AttackStep {
    #[default]
    Stay,
    Forward,
    Backward,
}

impl AttackStep {
    pub const MIN_LONGITUDINAL_SPEED: f32 = 0.15;

    pub fn from_local_velocity(local_velocity: Vec3) -> Self {
        if !local_velocity.is_finite() {
            return Self::Stay;
        }
        if local_velocity.z <= -Self::MIN_LONGITUDINAL_SPEED {
            Self::Forward
        } else if local_velocity.z >= Self::MIN_LONGITUDINAL_SPEED {
            Self::Backward
        } else {
            Self::Stay
        }
    }

    pub fn footwork(self) -> Footwork {
        match self {
            Self::Stay => Footwork::Stay,
            Self::Forward | Self::Backward => Footwork::Switch,
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
    pub local_velocity: Vec3,
    pub world_velocity: Vec3,
    pub gait_phase: f32,
    pub locomotion_sample_tick: u64,
    pub world_acceleration: Vec3,
    pub contact_sequence: u64,
    pub contact_foot: LeadFoot,
    pub landing_sequence: u64,
    pub landing_impact_speed: f32,
    pub lead_foot: LeadFoot,
    stance: StanceState,
    action: ActionState,
    pub animation_pack: String,
}

#[derive(Deserialize)]
struct SkeletonStateWire {
    body: BodyState,
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
    stance: StanceState,
    action: ActionState,
    animation_pack: String,
}

impl<'de> Deserialize<'de> for SkeletonState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SkeletonStateWire::deserialize(deserializer)?;
        let finite = |value: Vec3| value.is_finite().then_some(value).unwrap_or(Vec3::ZERO);
        let mut state = Self {
            body: BodyState::default(),
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
            stance: wire.stance,
            action: wire.action,
            animation_pack: wire.animation_pack,
        };
        state.transition_body(wire.body);
        Ok(state)
    }
}

impl Default for SkeletonState {
    fn default() -> Self {
        Self {
            body: BodyState::default(),
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
            stance: StanceState::Lowered,
            action: ActionState::default(),
            animation_pack: "humanoid_unarmed".to_owned(),
        }
    }
}

/// Applies authoritative guard state and aligns a newly raised stance with
/// the static-guard endpoint shared by every directional shuttle.
pub fn set_weapon_guard(skeleton: &mut SkeletonState, weapon_guard: WeaponGuardState) {
    match (skeleton.stance, weapon_guard) {
        (StanceState::Lowered, WeaponGuardState::Lowered)
        | (StanceState::Raised { .. }, WeaponGuardState::Raised) => {}
        (StanceState::Lowered, WeaponGuardState::Raised) if !skeleton.body.is_downed() => {
            skeleton.gait_phase = 0.0;
            skeleton.stance = StanceState::Raised {
                locomotion: RaisedLocomotionIntent::default(),
            };
        }
        (StanceState::Lowered, WeaponGuardState::Raised) => {}
        (StanceState::Raised { .. }, WeaponGuardState::Lowered) => {
            skeleton.stance = StanceState::Lowered;
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
        if body.is_downed() {
            self.stance = StanceState::Lowered;
            self.action = ActionState::default();
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
    pub fn is_grounded(&self) -> bool {
        self.body.is_grounded()
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
            ActionState(ActionKind::Dodge { direction, .. }) => direction,
            _ => Vec2::ZERO,
        }
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
    pub fn footwork(&self) -> Footwork {
        self.attack_step().footwork()
    }
    pub fn attack_step(&self) -> AttackStep {
        match self.action {
            ActionState(ActionKind::Attack { step, .. }) => step,
            _ => AttackStep::Stay,
        }
    }
    pub fn attack_step_speed(&self) -> f32 {
        match self.action {
            ActionState(ActionKind::Attack { step_speed, .. }) => step_speed,
            _ => 0.0,
        }
    }
    pub fn attack_movement(&self) -> Option<(Vec2, f32)> {
        match self.action {
            ActionState(ActionKind::Attack {
                movement_direction,
                movement_speed,
                ..
            }) => Some((movement_direction, movement_speed)),
            _ => None,
        }
    }
    pub fn attack_start_lead(&self) -> LeadFoot {
        match self.action {
            ActionState(ActionKind::Attack { start_lead, .. }) => start_lead,
            _ => self.lead_foot,
        }
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
        self.animation_local_velocity().xz().length()
    }

    /// Replaces the current action. This deliberately preserves the existing
    /// last-writer-wins compatibility policy until gameplay defines rejection
    /// or cancellation rules between actions.
    fn replace_action(&mut self, action: ActionState) {
        self.action = if self.body.is_downed() {
            ActionState::default()
        } else {
            action
        };
    }

    pub fn begin_dodge(&mut self, spec: DodgeSpec, start_tick: u64, contact_tick: u64) {
        let timeline = ActionTimeline::new(start_tick, contact_tick);
        let direction = if spec.direction.is_finite() {
            spec.direction.normalize_or_zero()
        } else {
            Vec2::ZERO
        };
        self.replace_action(ActionState(ActionKind::Dodge {
            direction,
            timeline,
        }));
    }

    pub fn begin_attack(&mut self, spec: AttackSpec, start_tick: u64, contact_tick: u64) {
        let target_height = if spec.target_height.is_finite() {
            spec.target_height.clamp(0.0, 1.0)
        } else {
            AttackSpec::default().target_height
        };
        self.replace_action(ActionState(ActionKind::Attack {
            target_height,
            strike_family: spec.strike_family,
            step: spec.step,
            step_speed: if spec.step_speed.is_finite() {
                spec.step_speed.clamp(0.0, 8.0)
            } else {
                0.0
            },
            movement_direction: if spec.movement_direction.is_finite() {
                spec.movement_direction.normalize_or_zero()
            } else {
                Vec2::ZERO
            },
            movement_speed: if spec.movement_speed.is_finite() {
                spec.movement_speed.clamp(0.0, 8.0)
            } else {
                0.0
            },
            start_lead: self.lead_foot,
            timeline: ActionTimeline::new(start_tick, contact_tick),
        }));
    }

    pub fn begin_block(&mut self, spec: BlockSpec, start_tick: u64, contact_tick: u64) {
        self.replace_action(ActionState(ActionKind::Block {
            incoming_line: spec.incoming_line,
            timeline: ActionTimeline::new(start_tick, contact_tick),
        }));
    }

    /// Advances an action whose contact is the midpoint of its visual
    /// timeline. Recovery gets the same bounded duration as preparation.
    pub fn advance_action(&mut self, current_tick: u64) {
        let switching_attack_start_lead = match self.action {
            ActionState(ActionKind::Attack {
                step: AttackStep::Forward | AttackStep::Backward,
                start_lead,
                ..
            }) => Some(start_lead),
            _ => None,
        };
        let timeline = match &mut self.action {
            ActionState(ActionKind::Idle) => return,
            ActionState(ActionKind::Dodge { timeline, .. })
            | ActionState(ActionKind::Attack { timeline, .. })
            | ActionState(ActionKind::Block { timeline, .. }) => timeline,
        };
        let preparation = timeline.preparation_ticks.max(1);
        let contact_tick = timeline.start_tick.saturating_add(preparation);
        let end_tick = contact_tick.saturating_add(preparation);
        if current_tick >= end_tick {
            if let Some(start_lead) = switching_attack_start_lead
                && self.lead_foot == start_lead
            {
                self.lead_foot = match start_lead {
                    LeadFoot::Left => LeadFoot::Right,
                    LeadFoot::Right => LeadFoot::Left,
                };
                // Keep lowered-stance locomotion from immediately deriving the
                // old lead again on the next tick. Raised guard ignores this
                // parity except as a stable planted restart seam.
                self.gait_phase = match self.lead_foot {
                    LeadFoot::Left => 0.0,
                    LeadFoot::Right => 0.5,
                };
            }
            // A saturated timeline cannot receive the following tick that
            // normally clears its retained endpoint. More generally, action
            // advancement may arrive after the exact endpoint; commit the
            // stance above before clearing so a missed tick cannot snap a
            // switching attack back to its starting guard.
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
            0.5 + 0.5 * current_tick.saturating_sub(contact_tick) as f32 / preparation as f32
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

/// Returns the controller's yaw without allowing camera pitch or roll to tilt
/// planar locomotion into or out of the ground plane.
pub fn controller_yaw(orientation: Quat) -> Quat {
    let forward = orientation * Vec3::NEG_Z;
    let Some(flat_forward) = forward.xz().try_normalize() else {
        return Quat::IDENTITY;
    };
    Quat::from_rotation_y((-flat_forward.x).atan2(-flat_forward.y))
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

fn body_yaw(rotation: Quat) -> f32 {
    let forward = rotation * Vec3::Z;
    forward.x.atan2(forward.z)
}

/// Projects controller motion into the compact replicated animation state.
/// Bone evaluation remains client-only; this is the shared server seam that
/// keeps deterministic captures on the same stride and posture rules.
pub fn project_skeleton_locomotion(skeleton: &mut SkeletonState, input: SkeletonLocomotionInput) {
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
    let was_grounded = skeleton.is_grounded();
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
    if !was_grounded && input.grounded {
        skeleton.landing_sequence = skeleton.landing_sequence.wrapping_add(1);
        skeleton.landing_impact_speed = (-previous_world_velocity.y).max(0.0);
    }
    skeleton.transition_body(if input.grounded {
        if input.crouching {
            BodyState::Grounded(GroundedPosture::Crouched)
        } else {
            BodyState::Grounded(GroundedPosture::Upright)
        }
    } else {
        BodyState::Airborne
    });

    let ground_speed = physical_speed;
    let attack_active = skeleton.action_kind() == SkeletonAction::Attack;
    if skeleton.weapon_guard() == WeaponGuardState::Raised
        && skeleton.posture() == Posture::Upright
        && !attack_active
    {
        advance_raised_locomotion_intent(skeleton, local_velocity, delta_seconds);
        let handoffs = skeleton
            .raised_locomotion()
            .step_sequence()
            .wrapping_sub(previous_guard_sequence);
        advance_contact_identity(skeleton, handoffs, previous_guard_swing);
    } else {
        skeleton.set_raised_locomotion(RaisedLocomotionIntent::planted(previous_guard_sequence));
        if input.grounded && ground_speed > 0.05 && !attack_active {
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
