use super::*;

/// Typed locomotion families shared by authoritative cadence projection and
/// client-only presentation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum LocomotionGait {
    #[default]
    Walk,
    Run,
    Downed,
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

fn landing_profile() -> LandingProfile {
    let config = crate::combat_config::runtime_animation_config()
        .locomotion
        .landing;
    LandingProfile {
        compression_per_metre_per_second: config.compression_per_metre_per_second,
        minimum_compression_metres: config.minimum_compression_metres,
        maximum_compression_metres: config.maximum_compression_metres,
        recovery_seconds: config.recovery_seconds,
    }
}

fn gait_profile(
    gait: LocomotionGait,
    config: crate::combat_config::AnimationGaitConfig,
) -> LocomotionProfile {
    LocomotionProfile {
        gait,
        reference_speed: config.reference_speed_metres_per_second,
        step_distance: config.step_distance_metres,
        support_phase_radius: config.support_phase_radius,
        bounce_metres: config.bounce_metres,
        flight_apex_metres: config.flight_apex_metres,
        landing: landing_profile(),
    }
}

pub fn locomotion_sample_hz() -> f32 {
    crate::combat_config::runtime_animation_config()
        .locomotion
        .sample_hz
}

pub fn walk_locomotion_profile() -> LocomotionProfile {
    let config = crate::combat_config::runtime_animation_config()
        .locomotion
        .walk;
    gait_profile(LocomotionGait::Walk, config)
}

pub fn run_locomotion_profile() -> LocomotionProfile {
    let config = crate::combat_config::runtime_animation_config()
        .locomotion
        .run;
    gait_profile(LocomotionGait::Run, config)
}

pub fn raised_guard_locomotion_profile() -> LocomotionProfile {
    let config = crate::combat_config::runtime_animation_config()
        .locomotion
        .raised_guard;
    gait_profile(LocomotionGait::RaisedGuard, config)
}

pub fn prone_locomotion_profile() -> LocomotionProfile {
    let config = crate::combat_config::runtime_animation_config()
        .locomotion
        .prone;
    gait_profile(LocomotionGait::Downed, config)
}

pub fn supine_locomotion_profile() -> LocomotionProfile {
    let config = crate::combat_config::runtime_animation_config()
        .locomotion
        .supine;
    gait_profile(LocomotionGait::Downed, config)
}

pub fn locomotion_profile(state: &SkeletonState) -> LocomotionProfile {
    let speed = state.animation_speed();
    match state.body() {
        BodyState::Prone => return prone_locomotion_profile(),
        BodyState::Supine => return supine_locomotion_profile(),
        _ => {}
    }
    if state.weapon_guard() == WeaponGuardState::Raised && !state.guarded_sprint_locomotion() {
        return LocomotionProfile {
            step_distance: guard_step_length(speed),
            ..raised_guard_locomotion_profile()
        };
    }
    let walk = walk_locomotion_profile();
    let run_profile = run_locomotion_profile();
    let run = ((speed - walk.reference_speed)
        / (run_profile.reference_speed - walk.reference_speed))
        .clamp(0.0, 1.0);
    LocomotionProfile {
        gait: if run >= 0.5 {
            LocomotionGait::Run
        } else {
            LocomotionGait::Walk
        },
        reference_speed: walk.reference_speed.lerp(run_profile.reference_speed, run),
        step_distance: ordinary_step_distance(speed),
        support_phase_radius: walk
            .support_phase_radius
            .lerp(run_profile.support_phase_radius, run),
        bounce_metres: walk.bounce_metres * (1.0 - run),
        flight_apex_metres: run_profile.flight_apex_metres * run,
        landing: landing_profile(),
    }
}

/// Shared distance model through authored walk/run reference points. This
/// replaces duplicated cadence arithmetic without changing current timing.
pub fn ordinary_step_distance(speed: f32) -> f32 {
    let speed = speed.max(0.0);
    let walk = walk_locomotion_profile();
    let run = run_locomotion_profile();
    if speed <= walk.reference_speed {
        0.9_f32.lerp(walk.step_distance, speed / walk.reference_speed)
    } else {
        let blend = ((speed - walk.reference_speed) / (run.reference_speed - walk.reference_speed))
            .clamp(0.0, 1.0);
        walk.step_distance.lerp(run.step_distance, blend)
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
