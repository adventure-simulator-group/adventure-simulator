use super::*;

pub(super) fn attack_samples(state: &SkeletonState) -> Vec<PoseSample> {
    let phase = state.action_phase().clamp(0.0, 1.0);
    let animation = state.attack_animation().unwrap_or(AttackAnimation::Thrust);
    let start_guard =
        if animation == AttackAnimation::Offhand && !state.attack_animations.offhand_preparation {
            guard_pose(state.attack_preparation().to)
        } else {
            guard_pose(animation)
        };
    let end_guard = guard_pose(state.attack_preparation().to);
    let contact =
        if animation == AttackAnimation::Offhand && state.attack_animations.offhand_preparation {
            SemanticPose::AttackOffhandPrepared
        } else {
            attack_pose(animation)
        };
    if state.attack_is_continuation() {
        let curve = state.attack_curve();
        return vec![PoseSample {
            pose: start_guard,
            sampling: PoseSampling::ContinuationSpan {
                contact,
                end: recovery_pose(animation),
                outgoing: continuation_pose(animation),
                finish: end_guard,
                start_coordinate: state
                    .attack_continuation_start_coordinate()
                    .unwrap_or(1.0 + curve.overshoot),
                incoming_tangent: state.attack_continuation_incoming_tangent().unwrap_or(0.0),
                ready_phase: continuation_ready_phase(),
                progress: phase,
            },
            weight: 1.0,
            mirror_lower_body: false,
        }];
    }
    let coordinate = if phase < 0.5 || !state.attack_has_queued_continuation() {
        state.attack_curve().coordinate(phase)
    } else {
        state.attack_curve().queued_recovery_coordinate(phase)
    };
    vec![PoseSample {
        pose: start_guard,
        sampling: PoseSampling::CurveSpan {
            end: contact,
            coordinate,
        },
        weight: 1.0,
        mirror_lower_body: false,
    }]
}

pub(super) fn guard_pose(animation: AttackAnimation) -> SemanticPose {
    match animation {
        AttackAnimation::Swing => SemanticPose::GuardSwing,
        AttackAnimation::Thrust => SemanticPose::GuardThrust,
        AttackAnimation::Offhand => SemanticPose::GuardOffhand,
    }
}

fn attack_pose(animation: AttackAnimation) -> SemanticPose {
    match animation {
        AttackAnimation::Swing => SemanticPose::AttackSwing,
        AttackAnimation::Thrust => SemanticPose::AttackThrust,
        AttackAnimation::Offhand => SemanticPose::AttackOffhand,
    }
}

fn recovery_pose(animation: AttackAnimation) -> SemanticPose {
    match animation {
        AttackAnimation::Swing => SemanticPose::RecoverSwing,
        AttackAnimation::Thrust => SemanticPose::RecoverThrust,
        AttackAnimation::Offhand => SemanticPose::GuardOffhand,
    }
}

fn continuation_pose(animation: AttackAnimation) -> SemanticPose {
    match animation {
        AttackAnimation::Swing => SemanticPose::ContinueSwing,
        AttackAnimation::Thrust => SemanticPose::ContinueThrust,
        AttackAnimation::Offhand => SemanticPose::AttackOffhand,
    }
}
