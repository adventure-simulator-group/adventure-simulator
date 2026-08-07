use super::*;

/// Typed locomotion families shared by authoritative cadence projection and
/// client-only presentation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum LocomotionGait {
    #[default]
    Walk,
    Run,
    Crouch,
    RaisedGuard,
}

/// Compact gait dynamics metadata. Phase 0..1 is one complete left/right
/// cycle; contact phases are 0 and 0.5.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LocomotionProfile {
    pub gait: LocomotionGait,
    pub reference_speed: f32,
    pub step_distance: f32,
    /// Radius around each contact phase that can carry support.
    pub support_phase_radius: f32,
    /// Visual grounded bounce, in metres.
    pub bounce_metres: f32,
    /// Visual unsupported apex, in metres. Zero means a grounded curve.
    pub flight_apex_metres: f32,
    pub landing: LandingProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LandingProfile {
    pub compression_per_metre_per_second: f32,
    pub minimum_compression_metres: f32,
    pub maximum_compression_metres: f32,
    pub recovery_seconds: f32,
}

pub const HUMANOID_LANDING_PROFILE: LandingProfile = LandingProfile {
    compression_per_metre_per_second: 0.012,
    minimum_compression_metres: 0.04,
    maximum_compression_metres: 0.08,
    recovery_seconds: 0.16,
};

pub const LOCOMOTION_SAMPLE_HZ: f32 = 64.0;

pub const WALK_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::Walk,
    reference_speed: 2.0,
    step_distance: 1.22,
    support_phase_radius: 0.28,
    bounce_metres: 0.04,
    flight_apex_metres: 0.0,
    landing: HUMANOID_LANDING_PROFILE,
};
pub const RUN_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::Run,
    reference_speed: 5.5,
    step_distance: 1.78,
    support_phase_radius: 0.175,
    bounce_metres: 0.0,
    flight_apex_metres: 0.09,
    landing: HUMANOID_LANDING_PROFILE,
};
pub const CROUCH_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::Crouch,
    reference_speed: 1.5,
    step_distance: 1.14,
    support_phase_radius: 0.30,
    bounce_metres: 0.025,
    flight_apex_metres: 0.0,
    landing: HUMANOID_LANDING_PROFILE,
};
pub const RAISED_GUARD_LOCOMOTION_PROFILE: LocomotionProfile = LocomotionProfile {
    gait: LocomotionGait::RaisedGuard,
    reference_speed: 2.0,
    step_distance: 0.38,
    support_phase_radius: 0.25,
    bounce_metres: 0.03,
    flight_apex_metres: 0.0,
    landing: HUMANOID_LANDING_PROFILE,
};

pub fn locomotion_profile(state: &SkeletonState) -> LocomotionProfile {
    let speed = state.animation_speed();
    if state.posture() == Posture::Crouched {
        return CROUCH_LOCOMOTION_PROFILE;
    }
    if state.weapon_guard() == WeaponGuardState::Raised {
        return LocomotionProfile {
            step_distance: guard_step_length(speed),
            ..RAISED_GUARD_LOCOMOTION_PROFILE
        };
    }
    let run = ((speed - WALK_LOCOMOTION_PROFILE.reference_speed)
        / (RUN_LOCOMOTION_PROFILE.reference_speed - WALK_LOCOMOTION_PROFILE.reference_speed))
        .clamp(0.0, 1.0);
    LocomotionProfile {
        gait: if run >= 0.5 {
            LocomotionGait::Run
        } else {
            LocomotionGait::Walk
        },
        reference_speed: WALK_LOCOMOTION_PROFILE
            .reference_speed
            .lerp(RUN_LOCOMOTION_PROFILE.reference_speed, run),
        step_distance: ordinary_step_distance(speed),
        support_phase_radius: WALK_LOCOMOTION_PROFILE
            .support_phase_radius
            .lerp(RUN_LOCOMOTION_PROFILE.support_phase_radius, run),
        bounce_metres: WALK_LOCOMOTION_PROFILE.bounce_metres * (1.0 - run),
        flight_apex_metres: RUN_LOCOMOTION_PROFILE.flight_apex_metres * run,
        landing: HUMANOID_LANDING_PROFILE,
    }
}

/// Shared distance model through authored walk/run reference points. This
/// replaces duplicated cadence arithmetic without changing current timing.
pub fn ordinary_step_distance(speed: f32) -> f32 {
    let speed = speed.max(0.0);
    if speed <= WALK_LOCOMOTION_PROFILE.reference_speed {
        0.9_f32.lerp(
            WALK_LOCOMOTION_PROFILE.step_distance,
            speed / WALK_LOCOMOTION_PROFILE.reference_speed,
        )
    } else {
        let blend = ((speed - WALK_LOCOMOTION_PROFILE.reference_speed)
            / (RUN_LOCOMOTION_PROFILE.reference_speed - WALK_LOCOMOTION_PROFILE.reference_speed))
            .clamp(0.0, 1.0);
        WALK_LOCOMOTION_PROFILE
            .step_distance
            .lerp(RUN_LOCOMOTION_PROFILE.step_distance, blend)
    }
}

pub fn gait_cycle_phase_delta(profile: LocomotionProfile, speed: f32, delta_seconds: f32) -> f32 {
    speed.max(0.0) * delta_seconds.max(0.0) / (profile.step_distance.max(0.01) * 2.0)
}

pub fn gait_support_weights(profile: LocomotionProfile, phase: f32) -> (f32, f32) {
    if profile.gait == LocomotionGait::RaisedGuard {
        return (1.0, 1.0);
    }
    let support = |contact: f32| {
        let distance = {
            let delta = (phase - contact).abs();
            delta.min(1.0 - delta)
        };
        (1.0 - smoothstep(
            profile.support_phase_radius * 0.45,
            profile.support_phase_radius,
            distance,
        ))
        .clamp(0.0, 1.0)
    };
    (support(0.0), support(0.5))
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
