use super::*;

/// One weighted authored pose contributing to the FK result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PoseSampling {
    /// Sample the pose's authoritative catalog frame.
    Anchor,
    /// Sample the complete authored motion at a normalized timeline phase.
    /// The graph still owns this sample's blend weight; the runtime only
    /// converts phase to the motion's authored timeline.
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
        let base = match state.action_kind() {
            SkeletonAction::Dodge => raised_guard_locomotion_samples(),
            _ => match state.posture() {
                Posture::Prone => gait_or_idle(
                    speed,
                    gait_phase,
                    SemanticPose::ProneIdle,
                    SemanticPose::ProneCrawlContact,
                ),
                Posture::Supine => gait_or_idle(
                    speed,
                    gait_phase,
                    SemanticPose::SupineIdle,
                    SemanticPose::SupineScamperContact,
                ),
                Posture::Airborne => vec![airborne_sample(state.local_velocity.xz().length())],
                Posture::Ragdolled => Vec::new(),
                Posture::Upright if state.weapon_guard() == WeaponGuardState::Raised => {
                    raised_guard_locomotion_samples()
                }
                Posture::Upright | Posture::Crouched => {
                    locomotion_samples(speed, gait_phase, crouch_amount)
                }
            },
        };
        let lower_body = if let Some(transition) = state.posture_transition()
            && let PostureTransitionKind::DiveToDowned { direction } = transition.kind()
        {
            dive_lower_body_samples(state.lead_foot, direction, transition.phase())
        } else if state.guarded_sprint_locomotion()
            && state.raised_locomotion().is_moving()
            && speed > 0.05
        {
            locomotion_samples(speed, gait_phase, 0.0)
        } else {
            Vec::new()
        };
        let action = posture_transition_samples(state)
            .or_else(|| downed_facing_samples(state))
            .unwrap_or_else(|| action_samples(state));
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

fn posture_transition_samples(state: &SkeletonState) -> Option<Vec<PoseSample>> {
    let transition = state.posture_transition()?;
    use PostureTransitionKind::*;
    if let DiveToDowned { direction } = transition.kind() {
        return Some(dive_transition_samples(
            state.lead_foot,
            direction,
            transition.phase(),
        ));
    }
    let (start, middle, end) = match transition.kind() {
        UprightToProne => (
            SemanticPose::CrouchIdle,
            SemanticPose::ProneTransition,
            SemanticPose::ProneIdle,
        ),
        ProneToUpright => (
            SemanticPose::ProneIdle,
            SemanticPose::ProneTransition,
            SemanticPose::CrouchIdle,
        ),
        ProneToSupine { direction } => (
            SemanticPose::ProneIdle,
            roll_pose(direction),
            SemanticPose::SupineIdle,
        ),
        SupineToProne { direction } => (
            SemanticPose::SupineIdle,
            // Reversing a prone-to-supine roll reverses its travel too. Pick
            // the opposite authored side so input direction remains spatial.
            roll_pose(direction.opposite()),
            SemanticPose::ProneIdle,
        ),
        SupineToUpright => (
            SemanticPose::SupineIdle,
            SemanticPose::SupineTransition,
            SemanticPose::CrouchIdle,
        ),
        DiveToDowned { .. } => unreachable!("dive transitions return above"),
    };
    let phase = transition.phase().clamp(0.0, 1.0);
    let (pose, next, progress) = if phase < 0.5 {
        (start, middle, phase * 2.0)
    } else {
        (middle, end, (phase - 0.5) * 2.0)
    };
    Some(vec![PoseSample {
        pose,
        sampling: PoseSampling::Span {
            end: next,
            progress,
        },
        weight: 1.0,
        mirror_lower_body: false,
    }])
}

fn downed_facing_samples(state: &SkeletonState) -> Option<Vec<PoseSample>> {
    let roll = state.downed_facing()?.half_turns();
    // Whole values are already represented by the body's prone/supine base
    // pose. Leaving a zero-progress roll action active here would mask the
    // crawl/scamper base layer for as long as aim remains held.
    if (roll - roll.round()).abs() <= 1.0e-4 {
        return None;
    }
    let start_index = roll.floor() as i64;
    let progress = roll - roll.floor();
    let start_prone = start_index.rem_euclid(2) == 0;
    let (start, end) = if start_prone {
        (SemanticPose::ProneIdle, SemanticPose::SupineIdle)
    } else {
        (SemanticPose::SupineIdle, SemanticPose::ProneIdle)
    };
    let middle = if start_index.rem_euclid(2) == 0 {
        SemanticPose::ProneSupineRollRight
    } else {
        SemanticPose::ProneSupineRollLeft
    };
    let (pose, next, span) = if progress < 0.5 {
        (start, middle, progress * 2.0)
    } else {
        (middle, end, (progress - 0.5) * 2.0)
    };
    Some(vec![PoseSample {
        pose,
        sampling: PoseSampling::Span {
            end: next,
            progress: span,
        },
        weight: 1.0,
        mirror_lower_body: false,
    }])
}

fn dive_transition_samples(
    lead: LeadFoot,
    direction: DiveDirection,
    phase: f32,
) -> Vec<PoseSample> {
    let duck = duck_direction_pose(lead, direction);
    let dive = dive_pose(direction);
    let phase = phase.clamp(0.0, 1.0);
    if phase <= 0.5 {
        return vec![PoseSample {
            pose: duck,
            sampling: PoseSampling::Span {
                end: dive,
                progress: phase * 2.0,
            },
            weight: 1.0,
            mirror_lower_body: false,
        }];
    }
    let contact = match direction {
        DiveDirection::Forward => SemanticPose::ProneIdle,
        DiveDirection::Backward => SemanticPose::SupineIdle,
        DiveDirection::Left => SemanticPose::ProneSupineRollLeft,
        DiveDirection::Right => SemanticPose::ProneSupineRollRight,
    };
    let recovery = (phase - 0.5) * 2.0;
    vec![PoseSample {
        pose: dive,
        sampling: PoseSampling::Span {
            end: contact,
            progress: recovery,
        },
        weight: 1.0,
        mirror_lower_body: false,
    }]
}

/// The dive clip contributes only above the pelvis. The lower body holds the
/// directional load through takeoff, then procedurally blends that existing
/// stance into the appropriate authored ground contact after impact.
fn dive_lower_body_samples(
    lead: LeadFoot,
    direction: DiveDirection,
    phase: f32,
) -> Vec<PoseSample> {
    let duck = duck_direction_pose(lead, direction);
    let phase = phase.clamp(0.0, 1.0);
    let sampling = if phase <= 0.5 {
        PoseSampling::Anchor
    } else {
        let contact = match direction {
            DiveDirection::Forward => SemanticPose::ProneIdle,
            DiveDirection::Backward => SemanticPose::SupineIdle,
            DiveDirection::Left => SemanticPose::ProneSupineRollLeft,
            DiveDirection::Right => SemanticPose::ProneSupineRollRight,
        };
        PoseSampling::Span {
            end: contact,
            progress: (phase - 0.5) * 2.0,
        }
    };
    vec![PoseSample {
        pose: duck,
        sampling,
        weight: 1.0,
        mirror_lower_body: false,
    }]
}

fn dive_pose(direction: DiveDirection) -> SemanticPose {
    match direction {
        DiveDirection::Forward => SemanticPose::DiveForward,
        DiveDirection::Backward => SemanticPose::DiveBackward,
        DiveDirection::Left => SemanticPose::DiveLeft,
        DiveDirection::Right => SemanticPose::DiveRight,
    }
}

fn roll_pose(direction: RollDirection) -> SemanticPose {
    match direction {
        RollDirection::Left => SemanticPose::ProneSupineRollLeft,
        RollDirection::Right => SemanticPose::ProneSupineRollRight,
    }
}

fn raised_guard_locomotion_samples() -> Vec<PoseSample> {
    vec![PoseSample {
        pose: SemanticPose::Guard,
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
) -> Vec<PoseSample> {
    if speed < 0.05 {
        vec![PoseSample {
            pose: idle,
            sampling: PoseSampling::Anchor,
            weight: 1.0,
            mirror_lower_body: false,
        }]
    } else {
        alternating_contact_pair(phase, contact)
    }
}

fn alternating_contact_pair(phase: f32, contact: SemanticPose) -> Vec<PoseSample> {
    let half = phase.rem_euclid(1.0) * 2.0;
    let progress = smoothstep01(half.fract());
    let (start_mirrored, end_mirrored) = if half < 1.0 {
        (false, true)
    } else {
        (true, false)
    };
    let mut samples = Vec::with_capacity(2);
    if progress < 1.0 {
        samples.push(PoseSample {
            pose: contact,
            sampling: PoseSampling::Anchor,
            weight: 1.0 - progress,
            mirror_lower_body: start_mirrored,
        });
    }
    if progress > 0.0 {
        samples.push(PoseSample {
            pose: contact,
            sampling: PoseSampling::Anchor,
            weight: progress,
            mirror_lower_body: end_mirrored,
        });
    }
    samples
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

#[cfg(test)]
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
        SkeletonAction::Dodge => Vec::new(),
        SkeletonAction::Attack => attack_samples(state),
        SkeletonAction::Block => vec![out_and_back(
            SemanticPose::Guard,
            block_pose(state.incoming_attack_line()),
            state.action_phase(),
        )],
    }
}

fn duck_direction_pose(_lead: LeadFoot, direction: DiveDirection) -> SemanticPose {
    match direction {
        DiveDirection::Forward => SemanticPose::DuckForward,
        DiveDirection::Backward => SemanticPose::DuckBackward,
        DiveDirection::Left => SemanticPose::DuckLeft,
        DiveDirection::Right => SemanticPose::DuckRight,
    }
}

fn attack_samples(state: &SkeletonState) -> Vec<PoseSample> {
    let phase = state.action_phase().clamp(0.0, 1.0);
    let start_guard = SemanticPose::Guard;
    let end_guard = SemanticPose::Guard;
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

fn block_pose(line: AttackLine) -> SemanticPose {
    match line {
        AttackLine::CutFromLeft => SemanticPose::BlockCutLeft,
        AttackLine::CutFromRight => SemanticPose::BlockCutRight,
        AttackLine::Thrust => SemanticPose::BlockThrust,
    }
}

fn attack_pose(state: &SkeletonState) -> SemanticPose {
    match state.attack_animation().unwrap_or(AttackAnimation::Thrust) {
        AttackAnimation::Swing => SemanticPose::AttackSwing,
        AttackAnimation::SwingFollow => SemanticPose::AttackSwingFollow,
        AttackAnimation::Thrust => SemanticPose::AttackThrust,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dive_blends_from_guard_specific_duck_to_direction_only_airborne_pose() {
        let loading = dive_transition_samples(LeadFoot::Left, DiveDirection::Right, 0.1);
        assert_eq!(loading[0].pose, SemanticPose::DuckRight);
        assert_eq!(
            loading[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::DiveRight,
                progress: 0.2,
            }
        );

        let airborne = dive_transition_samples(LeadFoot::Left, DiveDirection::Right, 0.5);
        assert_eq!(
            airborne[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::DiveRight,
                progress: 1.0,
            }
        );

        let recovery = dive_transition_samples(LeadFoot::Left, DiveDirection::Right, 0.75);
        assert_eq!(recovery.len(), 1);
        assert_eq!(recovery[0].pose, SemanticPose::DiveRight);
        assert_eq!(
            recovery[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::ProneSupineRollRight,
                progress: 0.5,
            }
        );
    }

    #[test]
    fn lateral_dives_recover_directly_into_the_matching_half_roll() {
        for (direction, contact) in [
            (DiveDirection::Left, SemanticPose::ProneSupineRollLeft),
            (DiveDirection::Right, SemanticPose::ProneSupineRollRight),
        ] {
            let sample = dive_transition_samples(LeadFoot::Left, direction, 1.0);
            assert_eq!(sample[0].pose, dive_pose(direction));
            assert_eq!(
                sample[0].sampling,
                PoseSampling::Span {
                    end: contact,
                    progress: 1.0,
                }
            );
        }
    }

    #[test]
    fn dive_lower_body_never_samples_the_dive_asset() {
        let airborne = dive_lower_body_samples(LeadFoot::Left, DiveDirection::Backward, 0.5);
        assert_eq!(airborne[0].pose, SemanticPose::DuckBackward);
        assert_eq!(airborne[0].sampling, PoseSampling::Anchor);

        let recovery = dive_lower_body_samples(LeadFoot::Left, DiveDirection::Backward, 0.75);
        assert_eq!(recovery[0].pose, SemanticPose::DuckBackward);
        assert_eq!(
            recovery[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::SupineIdle,
                progress: 0.5,
            }
        );
    }

    #[test]
    fn forward_dive_airborne_pose_is_independent_of_guard_lead() {
        let sample = dive_transition_samples(LeadFoot::Right, DiveDirection::Forward, 0.6);
        assert_eq!(sample[0].pose, SemanticPose::DiveForward);
    }
}
