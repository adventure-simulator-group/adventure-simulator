use super::*;

mod attack;

use attack::{attack_samples, guard_pose};

/// One weighted authored pose contributing to the FK result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PoseSampling {
    /// Sample the pose's authoritative catalog frame.
    Anchor,
    /// Sample the complete authored motion at a normalized timeline phase.
    /// The graph still owns this sample's blend weight; the runtime only
    /// converts phase to the motion's authored timeline.
    Cycle { phase: f32 },
    /// Sample a finite authored motion from its first through final frame.
    /// Progress one holds the final frame rather than wrapping to the start.
    Timeline { progress: f32 },
    /// Blend two semantic anchor poses. The client samples both catalog frames
    /// exactly and never evaluates exported in-between keys.
    Span { end: SemanticPose, progress: f32 },
    /// Extrapolate the transform delta between two semantic anchors. Unlike a
    /// normal span, the coordinate is intentionally allowed outside zero to
    /// one, within the bounds enforced by [`AttackCurve`].
    CurveSpan { end: SemanticPose, coordinate: f32 },
    /// Carry an extrapolated attack tangent through the full-backswing
    /// waypoint and every authored follow-up anchor with one C2-continuous
    /// path, ending at the next guard with zero velocity.
    ContinuationSpan {
        contact: SemanticPose,
        end: SemanticPose,
        outgoing: SemanticPose,
        finish: SemanticPose,
        start_coordinate: f32,
        incoming_tangent: f32,
        ready_phase: f32,
        progress: f32,
    },
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
        let base = match state.action_kind() {
            SkeletonAction::Dodge => raised_guard_locomotion_samples(state),
            _ => match state.posture() {
                Posture::Prone => gait_or_idle(
                    downed_animation_speed(state),
                    gait_phase,
                    SemanticPose::ProneIdle,
                    SemanticPose::ProneCrawlContact,
                ),
                Posture::Supine => gait_or_idle(
                    downed_animation_speed(state),
                    gait_phase,
                    SemanticPose::SupineIdle,
                    SemanticPose::SupineScamperContact,
                ),
                Posture::Airborne => vec![airborne_sample(state.local_velocity.xz().length())],
                Posture::Ragdolled => Vec::new(),
                Posture::Upright if state.weapon_guard() == WeaponGuardState::Raised => {
                    raised_guard_locomotion_samples(state)
                }
                Posture::Upright => upright_locomotion_samples(state),
            },
        };
        let lower_body = if let Some(transition) = state.posture_transition()
            && let PostureTransitionKind::DiveToDowned { direction, .. } = transition.kind()
        {
            dive_lower_body_samples(state.lead_foot, direction, transition.phase())
        } else if state.is_quickstep() {
            quickstep_lower_body_samples(state)
        } else if state.posture() == Posture::Upright
            && state.weapon_guard() == WeaponGuardState::Raised
        {
            combat_lower_body_samples(state)
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
            airborne_phase: (0.5 - state.local_velocity.y * 0.2).clamp(0.0, 1.0),
            action_phase: state.action_phase().clamp(0.0, 1.0),
            attack_target_height: state.attack_target_height().clamp(0.0, 1.0),
        }
    }
}

fn posture_transition_samples(state: &SkeletonState) -> Option<Vec<PoseSample>> {
    let transition = state.posture_transition()?;
    use PostureTransitionKind::*;
    if let DiveToDowned { direction, .. } = transition.kind() {
        return Some(dive_transition_samples(
            state.lead_foot,
            direction,
            transition.phase(),
        ));
    }
    let (start, middle, end) = match transition.kind() {
        UprightToProne => (
            SemanticPose::IdleRelaxed,
            SemanticPose::ProneTransition,
            SemanticPose::ProneIdle,
        ),
        ProneToUpright => (
            SemanticPose::ProneIdle,
            SemanticPose::ProneTransition,
            SemanticPose::IdleRelaxed,
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
            SemanticPose::IdleRelaxed,
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
    _lead: LeadFoot,
    direction: DiveDirection,
    phase: f32,
) -> Vec<PoseSample> {
    let dive = dive_pose(direction);
    let phase = phase.clamp(0.0, 1.0);
    if phase < 0.5 {
        return vec![PoseSample {
            pose: SemanticPose::GuardThrust,
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
    // Contact begins and ends at zero blend velocity, matching the procedural
    // pelvis release and the authoritative directional root handoff. Starting
    // this span linearly made the complete upper-body chain visibly twist on
    // the first grounded frame, including on forward dives with no root yaw.
    let recovery = dive_contact_pose_progress(phase);
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
/// guard through takeoff, then blends into the appropriate ground contact.
fn dive_lower_body_samples(
    _lead: LeadFoot,
    direction: DiveDirection,
    phase: f32,
) -> Vec<PoseSample> {
    let phase = phase.clamp(0.0, 1.0);
    let sampling = if phase < 0.5 {
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
            progress: dive_contact_pose_progress(phase),
        }
    };
    vec![PoseSample {
        pose: SemanticPose::GuardThrust,
        sampling,
        weight: 1.0,
        mirror_lower_body: false,
    }]
}

fn dive_contact_pose_progress(phase: f32) -> f32 {
    let recovery = smoothstep01((phase - 0.5) * 2.0);
    // Weighted clips at or below epsilon are discarded before the pose-buffer
    // plan key is built. Prime the contact endpoint invisibly on the exact
    // landing pose so the first advancing recovery frame does not also change
    // clip topology and retrigger whole-chain inertialization.
    recovery.max(f32::EPSILON * 2.0)
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

fn raised_guard_locomotion_samples(state: &SkeletonState) -> Vec<PoseSample> {
    let preparation = state.attack_preparation();
    let sampling = if preparation.from == preparation.to {
        PoseSampling::Anchor
    } else {
        PoseSampling::Span {
            end: guard_pose(preparation.to),
            progress: preparation.progress,
        }
    };
    vec![PoseSample {
        pose: guard_pose(preparation.from),
        sampling,
        weight: 1.0,
        mirror_lower_body: false,
    }]
}

fn combat_lower_body_samples(state: &SkeletonState) -> Vec<PoseSample> {
    let speed = state.animation_speed();
    if speed <= 0.05 || !state.raised_locomotion().is_moving() {
        return vec![anchor_sample(SemanticPose::CombatStance)];
    }
    if state.guarded_sprint_locomotion() {
        return locomotion_samples(speed, state.gait_phase);
    }

    let direction = state.raised_locomotion().local_direction();
    let strafe = direction.x.abs();
    let skip = direction.y.abs();
    let total = strafe + skip;
    if total <= f32::EPSILON {
        return vec![anchor_sample(SemanticPose::CombatStance)];
    }
    let mut samples = Vec::with_capacity(2);
    if strafe > f32::EPSILON {
        let phase = combat_cycle_phase(state.gait_phase);
        let mut sample = cycle_sample(
            SemanticPose::StrafeCycle,
            if direction.x < 0.0 {
                reverse_cycle_phase(phase)
            } else {
                phase
            },
        );
        sample.weight = strafe / total;
        samples.push(sample);
    }
    if skip > f32::EPSILON {
        let phase = combat_cycle_phase(state.gait_phase);
        // `skip.glb` serves both travel directions by changing which foot is
        // forward. Backward is the authored foot order; forward therefore
        // pairs against the opposite half-cycle. Without this contact-identity
        // shift, both forward diagonals blend a strafe pose with the wrong
        // skip foot even though their scalar weights are correct.
        let phase = if direction.y < 0.0 {
            (phase + 0.5).rem_euclid(1.0)
        } else {
            phase
        };
        let mut sample = cycle_sample(SemanticPose::SkipCycle, phase);
        sample.weight = skip / total;
        samples.push(sample);
    }
    samples
}

fn combat_cycle_phase(gait_phase: f32) -> f32 {
    // Gait phase 0/0.5 are authoritative contact boundaries. The authored
    // combat cycles place those contacts at frames 6/18 of a 24-frame cycle.
    (gait_phase + 0.25).rem_euclid(1.0)
}

fn reverse_cycle_phase(phase: f32) -> f32 {
    (1.0 - phase).rem_euclid(1.0)
}

fn quickstep_lower_body_samples(state: &SkeletonState) -> Vec<PoseSample> {
    // Authored frames 0/12 are the combat-idle endpoints surrounding the
    // complete load, launch, flight, and landing sequence at 3/6/9. Sampling
    // the full action phase keeps those poses synchronized with the physical
    // force curve instead of starting the legs only after the root launches.
    let progress = state.action_phase().clamp(0.0, 1.0);
    let direction = state.action_direction().normalize_or_zero();
    let directional = [
        (direction.y.max(0.0), SemanticPose::QuickstepForwardTakeoff),
        (direction.x.max(0.0), SemanticPose::QuickstepRightTakeoff),
        ((-direction.x).max(0.0), SemanticPose::QuickstepLeftTakeoff),
        ((-direction.y).max(0.0), SemanticPose::QuickstepBackTakeoff),
    ];
    let total = directional.iter().map(|(weight, _)| weight).sum::<f32>();
    directional
        .into_iter()
        .filter(|(weight, _)| *weight > f32::EPSILON)
        .map(|(weight, pose)| PoseSample {
            pose,
            sampling: PoseSampling::Timeline { progress },
            weight: weight / total,
            mirror_lower_body: false,
        })
        .collect()
}

fn anchor_sample(pose: SemanticPose) -> PoseSample {
    PoseSample {
        pose,
        sampling: PoseSampling::Anchor,
        weight: 1.0,
        mirror_lower_body: false,
    }
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

fn locomotion_samples(speed: f32, phase: f32) -> Vec<PoseSample> {
    let locomotion = smoothstep01(
        speed
            / crate::combat_config::runtime_animation_config()
                .locomotion
                .blend_speed,
    );
    let run = ((speed - walk_locomotion_profile().reference_speed)
        / (run_locomotion_profile().reference_speed - walk_locomotion_profile().reference_speed))
        .clamp(0.0, 1.0);
    let mut samples = Vec::with_capacity(5);
    append_scaled(
        &mut samples,
        vec![PoseSample {
            pose: SemanticPose::IdleRelaxed,
            sampling: PoseSampling::Anchor,
            weight: 1.0,
            mirror_lower_body: false,
        }],
        1.0 - locomotion,
    );
    append_scaled(
        &mut samples,
        vec![cycle_sample(SemanticPose::WalkContact, phase)],
        locomotion * (1.0 - run),
    );
    append_scaled(
        &mut samples,
        vec![cycle_sample(SemanticPose::RunContact, phase)],
        locomotion * run,
    );
    samples.retain(|sample| sample.weight > f32::EPSILON);
    samples
}

/// Ordinary upright locomotion blends the forward gait against the authored
/// strafe cycle in the hips' frame. As the root turns toward world velocity,
/// the lateral contribution naturally falls to zero.
fn upright_locomotion_samples(state: &SkeletonState) -> Vec<PoseSample> {
    let speed = state.animation_speed();
    let phase = state.gait_phase.rem_euclid(1.0);
    let direction = state.animation_local_velocity().xz().normalize_or_zero();
    let lateral = direction.x.abs();
    let longitudinal = direction.y.abs();
    let total = lateral + longitudinal;
    if lateral <= f32::EPSILON || total <= f32::EPSILON {
        return locomotion_samples(speed, phase);
    }

    let locomotion = smoothstep01(
        speed
            / crate::combat_config::runtime_animation_config()
                .locomotion
                .blend_speed,
    );
    let longitudinal_weight = longitudinal / total;
    let lateral_weight = lateral / total;
    let mut samples = locomotion_samples(speed, phase);
    for sample in &mut samples {
        if sample.pose == SemanticPose::IdleRelaxed {
            continue;
        }
        sample.weight *= longitudinal_weight;
    }
    let strafe_phase = combat_cycle_phase(phase);
    let mut strafe = cycle_sample(
        SemanticPose::StrafeCycle,
        if direction.x < 0.0 {
            reverse_cycle_phase(strafe_phase)
        } else {
            strafe_phase
        },
    );
    strafe.weight = locomotion * lateral_weight;
    samples.push(strafe);
    samples.retain(|sample| sample.weight > f32::EPSILON);
    samples
}

fn downed_animation_speed(state: &SkeletonState) -> f32 {
    if state.downed_turning() {
        state.animation_speed()
    } else {
        0.0
    }
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
            progress: smoothstep01(horizontal_speed / walk_locomotion_profile().reference_speed),
        },
        weight: 1.0,
        mirror_lower_body: false,
    }
}

fn action_samples(state: &SkeletonState) -> Vec<PoseSample> {
    match state.action_kind() {
        SkeletonAction::None => Vec::new(),
        SkeletonAction::Dodge => Vec::new(),
        SkeletonAction::Attack => attack_samples(state),
        SkeletonAction::Block => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dive_blends_from_guard_to_directional_airborne_pose() {
        let loading = dive_transition_samples(LeadFoot::Left, DiveDirection::Right, 0.1);
        assert_eq!(loading[0].pose, SemanticPose::GuardThrust);
        assert_eq!(
            loading[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::DiveRight,
                progress: 0.2,
            }
        );

        let contact = dive_transition_samples(LeadFoot::Left, DiveDirection::Right, 0.5);
        assert_eq!(contact[0].pose, SemanticPose::DiveRight);
        assert_eq!(
            contact[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::ProneSupineRollRight,
                progress: f32::EPSILON * 2.0,
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
    fn dive_lower_body_holds_guard_until_ground_contact() {
        let contact = dive_lower_body_samples(LeadFoot::Left, DiveDirection::Backward, 0.5);
        assert_eq!(contact[0].pose, SemanticPose::GuardThrust);
        assert_eq!(
            contact[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::SupineIdle,
                progress: f32::EPSILON * 2.0,
            }
        );

        let recovery = dive_lower_body_samples(LeadFoot::Left, DiveDirection::Backward, 0.75);
        assert_eq!(recovery[0].pose, SemanticPose::GuardThrust);
        assert_eq!(
            recovery[0].sampling,
            PoseSampling::Span {
                end: SemanticPose::SupineIdle,
                progress: 0.5,
            }
        );
    }

    #[test]
    fn dive_contact_recovery_starts_with_zero_slope() {
        let sample = dive_transition_samples(LeadFoot::Left, DiveDirection::Forward, 0.525);
        let PoseSampling::Span { progress, .. } = sample[0].sampling else {
            panic!("dive recovery must remain an authored pose span");
        };
        assert!((progress - 0.00725).abs() < 0.000_01);

        let lower = dive_lower_body_samples(LeadFoot::Left, DiveDirection::Forward, 0.525);
        let PoseSampling::Span { progress, .. } = lower[0].sampling else {
            panic!("dive lower-body recovery must remain an authored pose span");
        };
        assert!((progress - 0.00725).abs() < 0.000_01);
    }

    #[test]
    fn forward_dive_airborne_pose_is_independent_of_guard_lead() {
        let sample = dive_transition_samples(LeadFoot::Right, DiveDirection::Forward, 0.6);
        assert_eq!(sample[0].pose, SemanticPose::DiveForward);
    }

    #[test]
    fn diagonal_quickstep_blends_directional_authored_timelines() {
        let mut state = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        state
            .begin_dodge(DodgeSpec::quickstep(Vec2::new(1.0, 1.0)).unwrap(), 0, 100)
            .unwrap();
        state.advance_action(150);
        let samples = quickstep_lower_body_samples(&state);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].pose, SemanticPose::QuickstepForwardTakeoff);
        assert_eq!(samples[0].weight, 0.5);
        assert_eq!(samples[1].pose, SemanticPose::QuickstepRightTakeoff);
        assert_eq!(samples[1].weight, 0.5);
        assert!(samples.iter().all(|sample| {
            matches!(sample.sampling, PoseSampling::Timeline { progress: 0.75 })
        }));
    }

    #[test]
    fn attacks_keep_authored_weapon_upper_body_and_combat_lower_body_separate() {
        let mut state = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        state.begin_attack(AttackSpec::default(), 0, 100).unwrap();
        state.advance_action(50);
        let evaluation = AnimationEvaluation::from_skeleton(&state);
        assert_eq!(evaluation.action[0].pose, SemanticPose::GuardThrust);
        assert_eq!(evaluation.lower_body[0].pose, SemanticPose::CombatStance);
    }
}
