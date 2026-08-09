use super::*;

/// One weighted authored pose contributing to the FK result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PoseSampling {
    /// Sample the pose's authoritative catalog frame.
    Anchor,
    /// Sample the complete authored motion at the shared normalized gait
    /// phase. The graph still owns this sample's blend weight; the runtime
    /// only converts phase to the motion's authored timeline.
    Cycle { phase: f32 },
    /// Blend two semantic anchor poses. The client samples both catalog frames
    /// exactly and never evaluates exported in-between keys.
    Span { end: SemanticPose, progress: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PoseSample {
    pub pose: SemanticPose,
    pub sampling: PoseSampling,
    pub weight: f32,
    /// Selects the pre-mirrored gait clip for this complete anchor pose.
    /// Mirroring is binary because fractional reflection after FK blending
    /// collapses the bilateral limbs' forward/back separation.
    pub mirror_lower_body: bool,
}

/// Client-side blend coordinates derived from authoritative state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationEvaluation {
    pub base: Vec<PoseSample>,
    /// Optional locomotion layer evaluated only on the pelvis and legs. This
    /// lets a raised upper-body guard retain an ordinary walk/run blend.
    pub lower_body: Vec<PoseSample>,
    pub action: Vec<PoseSample>,
    pub movement_speed: f32,
    pub gait_phase: f32,
    pub crouch_amount: f32,
    pub airborne_phase: f32,
    pub action_phase: f32,
    pub attack_target_height: f32,
}

impl AnimationEvaluation {
    /// Evaluates semantic FK inputs. Clip resolution, bone masks, IK, and the
    /// final procedural pass happen after this deterministic selection stage.
    pub fn from_skeleton(state: &SkeletonState) -> Self {
        let speed = state.animation_speed();
        let gait_phase = state.gait_phase.rem_euclid(1.0);
        let crouch_amount = matches!(state.posture(), Posture::Crouched) as u8 as f32;
        let base = match state.posture() {
            Posture::Prone => gait_or_idle(
                speed,
                gait_phase,
                SemanticPose::ProneIdle,
                SemanticPose::ProneCrawlContact,
                SemanticPose::ProneCrawlPassing,
            ),
            Posture::Supine => gait_or_idle(
                speed,
                gait_phase,
                SemanticPose::SupineIdle,
                SemanticPose::SupineScamperContact,
                SemanticPose::SupineScamperPassing,
            ),
            Posture::Airborne => vec![airborne_sample(state.local_velocity.xz().length())],
            Posture::Ragdolled => Vec::new(),
            Posture::Upright if state.weapon_guard() == WeaponGuardState::Raised => {
                raised_guard_locomotion_samples(
                    state.animation_local_velocity(),
                    gait_phase,
                    state.lead_foot,
                )
            }
            Posture::Upright | Posture::Crouched => {
                locomotion_samples(speed, gait_phase, crouch_amount)
            }
        };
        let lower_body = if state.posture() == Posture::Upright
            && state.weapon_guard() == WeaponGuardState::Raised
            && speed > RAISED_GUARD_LOCOMOTION_PROFILE.reference_speed
        {
            locomotion_samples(speed, gait_phase, 0.0)
        } else {
            Vec::new()
        };
        let action = action_samples(state);
        Self {
            base,
            lower_body,
            action,
            movement_speed: speed,
            gait_phase,
            crouch_amount,
            airborne_phase: (0.5 - state.local_velocity.y * 0.2).clamp(0.0, 1.0),
            action_phase: state.action_phase().clamp(0.0, 1.0),
            attack_target_height: state.attack_target_height().clamp(0.0, 1.0),
        }
    }
}

fn raised_guard_locomotion_samples(
    _local_velocity: Vec3,
    _phase: f32,
    lead: LeadFoot,
) -> Vec<PoseSample> {
    vec![PoseSample {
        pose: guard_pose(lead),
        sampling: PoseSampling::Anchor,
        weight: 1.0,
        mirror_lower_body: false,
    }]
}

fn gait_or_idle(
    speed: f32,
    phase: f32,
    idle: SemanticPose,
    contact: SemanticPose,
    passing: SemanticPose,
) -> Vec<PoseSample> {
    if speed < 0.05 {
        vec![PoseSample {
            pose: idle,
            sampling: PoseSampling::Anchor,
            weight: 1.0,
            mirror_lower_body: false,
        }]
    } else {
        gait_pair(phase, contact, passing)
    }
}

fn locomotion_samples(speed: f32, phase: f32, crouch: f32) -> Vec<PoseSample> {
    const LOCOMOTION_BLEND_SPEED: f32 = 0.75;
    let locomotion = smoothstep01(speed / LOCOMOTION_BLEND_SPEED);
    let run = ((speed - WALK_LOCOMOTION_PROFILE.reference_speed)
        / (RUN_LOCOMOTION_PROFILE.reference_speed - WALK_LOCOMOTION_PROFILE.reference_speed))
        .clamp(0.0, 1.0);
    let mut samples = Vec::with_capacity(8);
    let idle = weighted_pair(SemanticPose::IdleRelaxed, SemanticPose::CrouchIdle, crouch);
    append_scaled(&mut samples, idle, 1.0 - locomotion);
    append_scaled(
        &mut samples,
        vec![cycle_sample(SemanticPose::WalkContact, phase)],
        locomotion * (1.0 - run) * (1.0 - crouch),
    );
    append_scaled(
        &mut samples,
        vec![cycle_sample(SemanticPose::RunContact, phase)],
        locomotion * run * (1.0 - crouch),
    );
    append_scaled(
        &mut samples,
        vec![cycle_sample(SemanticPose::WalkContact, phase)],
        locomotion * crouch,
    );
    samples.retain(|sample| sample.weight > f32::EPSILON);
    samples
}

fn cycle_sample(pose: SemanticPose, phase: f32) -> PoseSample {
    PoseSample {
        pose,
        sampling: PoseSampling::Cycle {
            phase: phase.rem_euclid(1.0),
        },
        weight: 1.0,
        mirror_lower_body: false,
    }
}

fn append_scaled(into: &mut Vec<PoseSample>, samples: Vec<PoseSample>, scale: f32) {
    into.extend(samples.into_iter().map(|mut sample| {
        sample.weight *= scale;
        sample
    }));
}

fn weighted_pair(a: SemanticPose, b: SemanticPose, b_weight: f32) -> Vec<PoseSample> {
    let b_weight = b_weight.clamp(0.0, 1.0);
    let mut samples = Vec::with_capacity(2);
    if b_weight < 1.0 {
        samples.push(PoseSample {
            pose: a,
            sampling: PoseSampling::Anchor,
            weight: 1.0 - b_weight,
            mirror_lower_body: false,
        });
    }
    if b_weight > 0.0 {
        samples.push(PoseSample {
            pose: b,
            sampling: PoseSampling::Anchor,
            weight: b_weight,
            mirror_lower_body: false,
        });
    }
    samples
}

pub(super) fn gait_pair(
    phase: f32,
    contact: SemanticPose,
    passing: SemanticPose,
) -> Vec<PoseSample> {
    let quarter = phase.rem_euclid(1.0) * 4.0;
    // A smooth cubic Hermite coordinate gives every sparse contact/flight
    // anchor zero endpoint velocity. Linear quarter interpolation changed
    // velocity discontinuously at each anchor and produced a visible knee and
    // ankle snap even though the poses themselves were continuous.
    let progress = smoothstep01(quarter.fract());
    let (start, start_mirrored, end, end_mirrored) = match quarter.floor() as u8 {
        0 => (contact, false, passing, false),
        1 => (passing, false, contact, true),
        2 => (contact, true, passing, true),
        _ => (passing, true, contact, false),
    };
    let mut samples = Vec::with_capacity(2);
    if progress < 1.0 {
        samples.push(PoseSample {
            pose: start,
            sampling: PoseSampling::Anchor,
            weight: 1.0 - progress,
            mirror_lower_body: start_mirrored,
        });
    }
    if progress > 0.0 {
        samples.push(PoseSample {
            pose: end,
            sampling: PoseSampling::Anchor,
            weight: progress,
            mirror_lower_body: end_mirrored,
        });
    }
    samples
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn airborne_sample(horizontal_speed: f32) -> PoseSample {
    PoseSample {
        pose: SemanticPose::AirborneCenter,
        sampling: PoseSampling::Span {
            end: SemanticPose::AirborneTravel,
            progress: smoothstep01(horizontal_speed / WALK_LOCOMOTION_PROFILE.reference_speed),
        },
        weight: 1.0,
        mirror_lower_body: false,
    }
}

fn out_and_back(start: SemanticPose, middle: SemanticPose, phase: f32) -> PoseSample {
    let phase = phase.clamp(0.0, 1.0);
    let (pose, end, progress) = if phase < 0.5 {
        (start, middle, phase * 2.0)
    } else {
        (middle, start, (phase - 0.5) * 2.0)
    };
    PoseSample {
        pose,
        sampling: PoseSampling::Span { end, progress },
        weight: 1.0,
        mirror_lower_body: false,
    }
}

fn action_samples(state: &SkeletonState) -> Vec<PoseSample> {
    match state.action_kind() {
        SkeletonAction::None => Vec::new(),
        SkeletonAction::Dodge => {
            let direction = state.action_direction();
            let pose = if direction.x.abs() > direction.y.abs() {
                duck_side_pose(state.lead_foot, direction.x < 0.0)
            } else if direction.y < 0.0 {
                match state.lead_foot {
                    LeadFoot::Left => SemanticPose::DuckLeadLeftBackward,
                    LeadFoot::Right => SemanticPose::DuckLeadRightBackward,
                }
            } else {
                SemanticPose::CrouchIdle
            };
            vec![out_and_back(
                guard_pose(state.lead_foot),
                pose,
                state.action_phase(),
            )]
        }
        SkeletonAction::Attack => attack_samples(state),
        SkeletonAction::Block => vec![out_and_back(
            guard_pose(state.lead_foot),
            block_pose(state.incoming_attack_line(), state.lead_foot),
            state.action_phase(),
        )],
    }
}

fn duck_side_pose(lead: LeadFoot, duck_left: bool) -> SemanticPose {
    match (lead, duck_left) {
        (LeadFoot::Left, true) => SemanticPose::DuckLeadLeftLeft,
        (LeadFoot::Left, false) => SemanticPose::DuckLeadLeftRight,
        (LeadFoot::Right, true) => SemanticPose::DuckLeadRightLeft,
        (LeadFoot::Right, false) => SemanticPose::DuckLeadRightRight,
    }
}

fn attack_samples(state: &SkeletonState) -> Vec<PoseSample> {
    let phase = state.action_phase().clamp(0.0, 1.0);
    let start_lead = state.attack_start_lead();
    let start_guard = guard_pose(start_lead);
    let end_guard = guard_pose(match state.footwork() {
        Footwork::Stay => start_lead,
        Footwork::Switch => opposite_foot(start_lead),
    });
    let contact = attack_pose(state);
    let (pose, end, blend) = if phase < 0.5 {
        (start_guard, contact, phase * 2.0)
    } else {
        (contact, end_guard, (phase - 0.5) * 2.0)
    };
    vec![PoseSample {
        pose,
        sampling: PoseSampling::Span {
            end,
            progress: blend,
        },
        weight: 1.0,
        mirror_lower_body: false,
    }]
}

fn guard_pose(lead: LeadFoot) -> SemanticPose {
    match lead {
        LeadFoot::Left => SemanticPose::GuardLeadLeft,
        LeadFoot::Right => SemanticPose::GuardLeadRight,
    }
}

fn block_pose(line: AttackLine, lead: LeadFoot) -> SemanticPose {
    use {AttackLine::*, LeadFoot::*, SemanticPose::*};
    match (line, lead) {
        (CutFromLeft, Left) => BlockCutLeftLeadLeft,
        (CutFromLeft, Right) => BlockCutLeftLeadRight,
        (CutFromRight, Left) => BlockCutRightLeadLeft,
        (CutFromRight, Right) => BlockCutRightLeadRight,
        (Thrust, Left) => BlockThrustLeadLeft,
        (Thrust, Right) => BlockThrustLeadRight,
    }
}

fn attack_pose(state: &SkeletonState) -> SemanticPose {
    use {LeadFoot::*, SemanticPose::*, StrikeFamily::*};
    match (state.strike_family(), state.attack_start_lead()) {
        (Thrust, Left) => AttackThrustLeadLeftContact,
        (Thrust, Right) => AttackThrustLeadRightContact,
        (Slash, Left) => AttackSlashLeadLeftContact,
        (Slash, Right) => AttackSlashLeadRightContact,
    }
}
