//! Deterministic animation-review fixtures and scenario classification.

use super::*;

pub(super) const SAMPLE_HZ: f32 = LOCOMOTION_SAMPLE_HZ;
pub(super) const QUICKSTEP_FIXTURE_BIOLOGICAL_MASS_KG: f32 = 70.0;
pub(super) const QUICKSTEP_FIXTURE_TOTAL_MASS_KG: f32 = 93.9;
pub(super) const QUICKSTEP_FIXTURE_LEG_STRENGTH: f32 = 4.0;
pub(super) const QUICKSTEP_FIXTURE_LEG_AGILITY: f32 = 4.0;
// The canonical default inventory currently projects 24.9 kg into tactical
// play: one catalog unit weight for each durable inventory row.
pub(super) const JOHN_CURRENT_TOTAL_BURDEN_KG: f32 = 94.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScenarioKind {
    Ordinary,
    RaisedGuard,
    Terrain,
    Transition,
    Landing,
    Attack,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScenarioMetadata {
    pub(super) kind: ScenarioKind,
    pub(super) repeatable: bool,
    pub(super) procedural_solver: bool,
}

pub(super) fn uses_authored_combat_locomotion(name: &str) -> bool {
    name.starts_with("raised-guard")
        && !is_guard_stop_transition(name)
        && !matches!(
            name,
            "raised-guard-stationary-turn" | "raised-guard-transition"
        )
}

pub(super) fn is_guard_stop_transition(name: &str) -> bool {
    name.starts_with("raised-guard") && (name.contains("release") || name.contains("tap-stop"))
}

pub(super) fn is_quickstep_scenario(name: &str) -> bool {
    name.starts_with("quickstep-")
}

pub(super) fn scenario_metadata(name: &str) -> ScenarioMetadata {
    if is_quickstep_scenario(name)
        || name.starts_with("downed-")
        || name.starts_with("dive-")
        || name.ends_with("-get-up")
        || name.starts_with("prone-roll-")
        || name == "jump-charge-anticipation"
    {
        ScenarioMetadata {
            kind: ScenarioKind::Transition,
            repeatable: false,
            procedural_solver: false,
        }
    } else if name == "terrain-toggle-mid-stride" {
        ScenarioMetadata {
            kind: ScenarioKind::Terrain,
            repeatable: false,
            // This scenario deliberately spends time with the solver off. Its
            // contract is bounded transition continuity, not steady plants.
            procedural_solver: false,
        }
    } else if matches!(name, "flat-grid-walk-no-ik" | "flat-grid-sprint-no-ik") {
        ScenarioMetadata {
            kind: ScenarioKind::Ordinary,
            repeatable: true,
            procedural_solver: false,
        }
    } else if name == "cross-slope-walk"
        || name.starts_with("terrain-")
        || name.starts_with("flat-grid-")
    {
        ScenarioMetadata {
            kind: ScenarioKind::Terrain,
            repeatable: !name.contains("stop")
                && !name.contains("turn")
                && !name.contains("toggle")
                && !name.contains("restart")
                && !name.contains("chatter")
                && !name.starts_with("terrain-steady-run"),
            procedural_solver: true,
        }
    } else if name.starts_with("attack-live-") {
        ScenarioMetadata {
            kind: ScenarioKind::Attack,
            repeatable: false,
            procedural_solver: true,
        }
    } else if name.starts_with("raised-guard") {
        ScenarioMetadata {
            kind: ScenarioKind::RaisedGuard,
            repeatable: !name.contains("release")
                && !name.contains("reversal")
                && !name.contains("accelerate")
                && name != "raised-guard-stationary-turn"
                && name != "raised-guard-transition",
            // Translating combat locomotion is authored skip/strafe FK. The
            // stationary turn and guard-entry fixtures still exercise the
            // procedural plant/step solver.
            procedural_solver: !uses_authored_combat_locomotion(name),
        }
    } else if name == "airborne-landing" {
        ScenarioMetadata {
            kind: ScenarioKind::Landing,
            repeatable: false,
            procedural_solver: false,
        }
    } else if name.contains("transition")
        || name.contains("enter-exit")
        || name.contains("hard-stop")
        || name.contains("ramp")
        || name.contains("turn")
    {
        ScenarioMetadata {
            kind: ScenarioKind::Transition,
            repeatable: false,
            procedural_solver: false,
        }
    } else {
        ScenarioMetadata {
            kind: ScenarioKind::Ordinary,
            repeatable: true,
            procedural_solver: false,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PlannedFrame {
    pub(super) scenario: &'static str,
    pub(super) scenario_frame: usize,
    pub(super) speed: f32,
    pub(super) time_seconds: f32,
    pub(super) local_direction: Vec2,
    pub(super) camera_yaw: f32,
    pub(super) camera_pitch: f32,
    pub(super) action: SkeletonAction,
    pub(super) weapon_guard: WeaponGuardState,
    pub(super) lead_foot: LeadFoot,
}

pub(super) fn canonical_john_sprint_speed() -> f32 {
    tactical_sprint_speed(
        QUICKSTEP_FIXTURE_LEG_STRENGTH,
        QUICKSTEP_FIXTURE_LEG_STRENGTH,
        1.0,
        1.0,
        JOHN_CURRENT_TOTAL_BURDEN_KG,
    )
}

pub(super) fn steady_scenario(name: &'static str, speed: f32, cycles: f32) -> Vec<PlannedFrame> {
    steady_scenario_in_direction(name, speed, cycles, Vec2::NEG_Y)
}

pub(super) fn full_ragdoll_scenario() -> Vec<PlannedFrame> {
    // The first logical sample receives the viewer's sixty-frame load/settle
    // window, so nine captured samples already cover a stable physics result.
    (0..=8)
        .map(|scenario_frame| PlannedFrame {
            scenario: "full-ragdoll",
            scenario_frame,
            speed: 0.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::ZERO,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn steady_scenario_in_direction(
    name: &'static str,
    speed: f32,
    cycles: f32,
    local_direction: Vec2,
) -> Vec<PlannedFrame> {
    let cycle_duration = ordinary_step_distance(speed) * 2.0 / speed;
    let duration = cycles * cycle_duration;
    // Include the first authoritative tick after the requested final cycle.
    // Fixed-rate sampling rarely lands on the mathematical wrap exactly; the
    // post-wrap sample makes every steady scenario exercise its real loop
    // transition instead of silently reporting no seam.
    let last_frame = (duration * SAMPLE_HZ).ceil() as usize + 1;
    (0..=last_frame)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn transition_scenario() -> Vec<PlannedFrame> {
    let duration = 4.0;
    let last_frame = (duration * SAMPLE_HZ) as usize;
    (0..=last_frame)
        .map(|frame| {
            let t = frame as f32 / SAMPLE_HZ;
            let speed = if t < 0.5 {
                2.0 * smoothstep01(t / 0.5)
            } else if t < 1.0 {
                2.0
            } else if t < 1.75 {
                2.0 + 3.5 * smoothstep01((t - 1.0) / 0.75)
            } else if t < 2.5 {
                5.5
            } else if t < 3.5 {
                5.5 * (1.0 - smoothstep01((t - 2.5) / 1.0))
            } else {
                0.0
            };
            PlannedFrame {
                scenario: "start-stop-transition",
                scenario_frame: frame,
                speed,
                time_seconds: t,
                local_direction: Vec2::NEG_Y,
                camera_yaw: 0.0,
                camera_pitch: 0.0,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn terrain_toggle_scenario() -> Vec<PlannedFrame> {
    (0..=112)
        .map(|scenario_frame| PlannedFrame {
            scenario: "terrain-toggle-mid-stride",
            scenario_frame,
            speed: 2.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::X,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn terrain_half_turn_reversal_scenario() -> Vec<PlannedFrame> {
    (0..=192)
        .map(|scenario_frame| PlannedFrame {
            scenario: "terrain-half-turn-reversal",
            scenario_frame,
            speed: 2.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::NEG_Y,
            camera_yaw: if scenario_frame >= 128 {
                std::f32::consts::PI
            } else {
                0.0
            },
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn scenario_uses_terrain_ik(scenario: &str) -> bool {
    scenario_metadata(scenario).kind == ScenarioKind::Terrain
        || scenario.starts_with("raised-guard-tap-stop-")
}

pub(super) fn terrain_ik_enabled_for_frame(frame: &PlannedFrame) -> bool {
    (scenario_uses_terrain_ik(frame.scenario) || frame.scenario.contains("terrain"))
        && (frame.scenario != "terrain-toggle-mid-stride"
            || (16..80).contains(&frame.scenario_frame))
}

pub(super) fn raised_scenario_requires_zero_flight(scenario: &str) -> bool {
    scenario_metadata(scenario).kind == ScenarioKind::RaisedGuard
        && !scenario.starts_with("raised-guard-tap-stop-")
}

pub(super) fn dynamics_speed_scenario(name: &'static str, hard_stop: bool) -> Vec<PlannedFrame> {
    (0..=256)
        .map(|scenario_frame| {
            let speed = if hard_stop {
                if scenario_frame < 96 { 5.5 } else { 0.0 }
            } else if scenario_frame < 32 {
                5.5 * scenario_frame as f32 / 32.0
            } else if scenario_frame < 128 {
                5.5
            } else if scenario_frame < 160 {
                5.5 * (160 - scenario_frame) as f32 / 32.0
            } else {
                0.0
            };
            PlannedFrame {
                scenario: name,
                scenario_frame,
                speed,
                time_seconds: scenario_frame as f32 / SAMPLE_HZ,
                local_direction: Vec2::NEG_Y,
                camera_yaw: 0.0,
                camera_pitch: 0.0,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn flat_grid_walk_stop_scenario() -> Vec<PlannedFrame> {
    (0..=96)
        .map(|scenario_frame| {
            let speed = if scenario_frame < 48 {
                2.0
            } else if scenario_frame < 56 {
                2.0 * (56 - scenario_frame) as f32 / 8.0
            } else {
                0.0
            };
            PlannedFrame {
                scenario: "flat-grid-walk-stop",
                scenario_frame,
                speed,
                time_seconds: scenario_frame as f32 / SAMPLE_HZ,
                local_direction: Vec2::NEG_Y,
                camera_yaw: 0.0,
                camera_pitch: 0.0,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn terrain_tap_stop_scenario(
    name: &'static str,
    speed: f32,
    moving_frames: std::ops::Range<usize>,
    local_direction: Vec2,
) -> Vec<PlannedFrame> {
    (0..=96)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed: if moving_frames.contains(&scenario_frame) {
                speed
            } else {
                0.0
            },
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn terrain_tap_restart_scenario() -> Vec<PlannedFrame> {
    (0..=96)
        .map(|scenario_frame| {
            let moving = matches!(scenario_frame, 8..=17 | 24..=33 | 40..=49);
            PlannedFrame {
                scenario: "terrain-tap-restart-crossfade",
                scenario_frame,
                speed: if moving { 5.5 } else { 0.0 },
                time_seconds: scenario_frame as f32 / SAMPLE_HZ,
                local_direction: Vec2::NEG_Y,
                camera_yaw: 0.0,
                camera_pitch: 0.0,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn terrain_threshold_chatter_scenario() -> Vec<PlannedFrame> {
    (0..=128)
        .map(|scenario_frame| PlannedFrame {
            scenario: "terrain-speed-threshold-chatter",
            scenario_frame,
            speed: match scenario_frame {
                8..=39 => {
                    if scenario_frame % 2 == 0 {
                        0.079
                    } else {
                        0.081
                    }
                }
                40 => 0.09,
                41..=71 => {
                    if scenario_frame % 2 == 0 {
                        0.029
                    } else {
                        0.031
                    }
                }
                72 => 0.02,
                73..=103 => {
                    if scenario_frame % 2 == 0 {
                        0.079
                    } else {
                        0.081
                    }
                }
                _ => 0.0,
            },
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::NEG_Y,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn raised_guard_lateral_tap_stop_scenario(
    name: &'static str,
    direction: Vec2,
) -> Vec<PlannedFrame> {
    (0..=96)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed: if (32..38).contains(&scenario_frame) {
                1.0
            } else {
                0.0
            },
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: direction,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn airborne_landing_scenario() -> Vec<PlannedFrame> {
    (0..=96)
        .map(|scenario_frame| PlannedFrame {
            scenario: "airborne-landing",
            scenario_frame,
            speed: 0.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::ZERO,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

pub(super) fn attack_live_scenario(
    name: &'static str,
    speed: f32,
    initial_direction: Vec2,
    lead_foot: LeadFoot,
    reverse_velocity: bool,
) -> Vec<PlannedFrame> {
    const START: usize = 8;
    // Keep sampling after the authored action ends so procedural recovery can
    // take as many bounded guard-convergence steps as it needs.
    (0..=127)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            // Deliberately reverse velocity and yaw after attack start in the
            // stress fixture. The live movement input must remain the one
            // selected on frame zero.
            local_direction: if (reverse_velocity || name.contains("high-speed"))
                && scenario_frame > START
            {
                -initial_direction
            } else {
                initial_direction
            },
            camera_yaw: if name.contains("yaw-only") && scenario_frame > START {
                std::f32::consts::FRAC_PI_2
            } else {
                0.0
            },
            camera_pitch: 0.0,
            action: if scenario_frame < START {
                SkeletonAction::None
            } else {
                SkeletonAction::Attack
            },
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

pub(super) fn capture_plan() -> Vec<PlannedFrame> {
    [
        downed_contact_scenario("downed-prone-crawl", BodyState::Prone),
        downed_contact_scenario("downed-supine-scamper", BodyState::Supine),
        downed_look_scenario(),
        ordinary_camera_pitch_scenario(),
        posture_transition_scenario(
            "dive-forward",
            BodyState::Grounded(GroundedPosture::Upright),
        ),
        posture_transition_scenario(
            "dive-backward",
            BodyState::Grounded(GroundedPosture::Upright),
        ),
        posture_transition_scenario("dive-left", BodyState::Grounded(GroundedPosture::Upright)),
        posture_transition_scenario("dive-right", BodyState::Grounded(GroundedPosture::Upright)),
        dive_impact_scenario("dive-left-impact"),
        dive_impact_scenario("dive-right-impact"),
        dive_impact_scenario("dive-backward-impact"),
        aimed_dive_impact_scenario("dive-forward-aimed-impact"),
        aimed_dive_impact_scenario("dive-left-aimed-impact"),
        aimed_dive_impact_scenario("dive-right-aimed-impact"),
        aimed_dive_impact_scenario("dive-backward-aimed-impact"),
        posture_transition_scenario("prone-get-up", BodyState::Prone),
        posture_transition_scenario("supine-get-up", BodyState::Supine),
        posture_transition_scenario("prone-roll-left", BodyState::Prone),
        posture_transition_scenario("prone-roll-right", BodyState::Prone),
        jump_charge_scenario(),
        quickstep_scenario("quickstep-right", Vec2::X),
        quickstep_scenario("quickstep-left", Vec2::NEG_X),
        steady_scenario("steady-walk-2.0", 2.0, 2.0),
        steady_scenario("walk-run-blend-3.75", 3.75, 2.0),
        steady_scenario("steady-run-5.5", 5.5, 2.0),
        steady_scenario_in_direction("lateral-walk-2.0", 2.0, 1.0, Vec2::X),
        steady_scenario_in_direction("reverse-walk-2.0", 2.0, 1.0, Vec2::Y),
        turning_scenario("gradual-camera-turn", false),
        turning_scenario("half-turn-reversal", true),
        guard_plant_turn_scenario(),
        raised_guard_stationary_turn_scenario(),
        raised_guard_steady_scenario("raised-guard-forward", 2.0, 2.0, Vec2::NEG_Y),
        raised_guard_scenario("raised-guard-backward", Vec2::Y),
        raised_guard_scenario("raised-guard-left", Vec2::NEG_X),
        raised_guard_scenario("raised-guard-right", Vec2::X),
        raised_guard_scenario("raised-guard-forward-left", Vec2::new(-1.0, -1.0)),
        raised_guard_scenario("raised-guard-forward-right", Vec2::new(1.0, -1.0)),
        raised_guard_scenario("raised-guard-backward-left", Vec2::new(-1.0, 1.0)),
        raised_guard_scenario("raised-guard-backward-right", Vec2::ONE),
        raised_guard_steady_scenario("raised-guard-half-speed", 1.0, 2.0, Vec2::X),
        raised_guard_acceleration_scenario(),
        raised_guard_release_scenario(),
        raised_guard_lateral_tap_stop_scenario("raised-guard-tap-stop-left", Vec2::NEG_X),
        raised_guard_lateral_tap_stop_scenario("raised-guard-tap-stop-right", Vec2::X),
        raised_guard_reversal_scenario(),
        raised_guard_steady_scenario_with_lead(
            "raised-guard-right-support-left",
            2.0,
            1.0,
            Vec2::NEG_X,
            LeadFoot::Right,
        ),
        raised_guard_steady_scenario_with_lead(
            "raised-guard-right-support-right",
            2.0,
            1.0,
            Vec2::X,
            LeadFoot::Right,
        ),
        raised_guard_steady_scenario_with_lead(
            "raised-guard-right-support-forward-right",
            2.0,
            1.0,
            Vec2::new(1.0, -1.0),
            LeadFoot::Right,
        ),
        raised_guard_acceleration_scenario_with_lead(
            "raised-guard-right-support-accelerate",
            LeadFoot::Right,
        ),
        raised_guard_release_scenario_with_lead(
            "raised-guard-right-support-release",
            LeadFoot::Right,
        ),
        raised_guard_reversal_scenario_with_lead(
            "raised-guard-right-support-reversal",
            LeadFoot::Right,
        ),
        raised_guard_transition_scenario(),
        attack_live_scenario(
            "attack-live-forward-left-support",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Left,
            false,
        ),
        attack_live_scenario(
            "attack-live-forward-right-support",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Right,
            false,
        ),
        attack_live_scenario(
            "attack-live-backward-left-support",
            2.0,
            Vec2::Y,
            LeadFoot::Left,
            false,
        ),
        attack_live_scenario(
            "attack-live-backward-right-support",
            2.0,
            Vec2::Y,
            LeadFoot::Right,
            false,
        ),
        attack_live_scenario(
            "attack-live-stationary",
            0.0,
            Vec2::ZERO,
            LeadFoot::Left,
            false,
        ),
        attack_live_scenario(
            "attack-live-moving-thrust",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Left,
            false,
        ),
        attack_live_scenario(
            "attack-live-stationary-swing",
            0.0,
            Vec2::ZERO,
            LeadFoot::Left,
            false,
        ),
        attack_live_scenario(
            "attack-live-high-speed-reversal",
            5.5,
            Vec2::NEG_Y,
            LeadFoot::Left,
            false,
        ),
        attack_live_scenario(
            "attack-live-reversal",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Left,
            true,
        ),
        attack_live_scenario(
            "attack-live-yaw-only",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Left,
            false,
        ),
        attack_live_scenario(
            "attack-live-terrain-cross-slope",
            2.0,
            Vec2::new(0.5, -1.0).normalize(),
            LeadFoot::Left,
            false,
        ),
        dynamics_speed_scenario("speed-ramp-up-down", false),
        dynamics_speed_scenario("hard-stop", true),
        dynamics_turn_scenario("dynamics-turn-90", std::f32::consts::FRAC_PI_2),
        dynamics_turn_scenario("dynamics-turn-180", std::f32::consts::PI),
        airborne_landing_scenario(),
        steady_scenario("cadence-contact", 2.0, 2.0),
        // Terrain IK captures include a complete calibration cycle plus two
        // review cycles. The old one-cycle probe discarded its only full cycle
        // as warmup and judged a tiny post-wrap tail.
        steady_scenario_in_direction("cross-slope-walk", 2.0, 3.0, Vec2::X),
        steady_scenario("terrain-uphill-walk", 2.0, 3.0),
        steady_scenario_in_direction("terrain-downhill-walk", 2.0, 3.0, Vec2::Y),
        steady_scenario_in_direction(
            "terrain-diagonal-walk",
            2.0,
            3.0,
            Vec2::new(1.0, -1.0).normalize(),
        ),
        terrain_toggle_scenario(),
        dynamics_speed_scenario("terrain-hard-stop", true),
        terrain_tap_stop_scenario("terrain-tap-stop-forward", 0.8, 8..20, Vec2::NEG_Y),
        terrain_tap_stop_scenario("terrain-stop-mid-swing", 2.0, 8..28, Vec2::NEG_Y),
        terrain_tap_stop_scenario("terrain-run-flight-stop", 5.5, 8..18, Vec2::NEG_Y),
        terrain_tap_restart_scenario(),
        terrain_threshold_chatter_scenario(),
        steady_scenario("terrain-steady-run-5.5", 5.5, 3.0),
        dynamics_turn_scenario("terrain-turn-90", std::f32::consts::FRAC_PI_2),
        terrain_half_turn_reversal_scenario(),
        transition_scenario(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(super) fn quickstep_scenario(name: &'static str, local_direction: Vec2) -> Vec<PlannedFrame> {
    let planar_speeds = quickstep_fixture_planar_speeds(72);
    (0..72)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed: planar_speeds[scenario_frame],
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::Dodge,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn quickstep_action_ticks() -> usize {
    let config = TacticalCombatConfig::default();
    (quickstep_action_contact_ticks(config.movement.maneuvers.quickstep_duration_seconds) * 2)
        as usize
}

pub(super) fn quickstep_push_ticks() -> usize {
    let config = TacticalCombatConfig::default();
    (quickstep_push_seconds(QUICKSTEP_FIXTURE_LEG_AGILITY, &config.movement.motor) * SAMPLE_HZ)
        .ceil() as usize
}

pub(super) fn quickstep_fixture_planar_speeds(frame_count: usize) -> Vec<f32> {
    let config = TacticalCombatConfig::default();
    let action_ticks = quickstep_action_ticks();
    let peak_force = quickstep_peak_horizontal_force_newtons(
        QUICKSTEP_FIXTURE_BIOLOGICAL_MASS_KG,
        QUICKSTEP_FIXTURE_LEG_STRENGTH,
        &config.movement.motor,
    );
    let target_displacement = quickstep_target_displacement_metres(
        CharacterDimensions::default().leg_length_metres,
        &config.movement.motor,
    );
    let duration = action_ticks as f32 / SAMPLE_HZ;
    let mut velocity: f32 = 0.0;
    let mut displacement: f32 = 0.0;
    let mut speeds = Vec::with_capacity(frame_count);
    for frame in 0..frame_count {
        speeds.push(velocity.abs());
        if frame < action_ticks {
            let target = quickstep_motion_target(
                (frame + 1) as f32 / action_ticks as f32,
                target_displacement,
                duration,
                config
                    .movement
                    .motor
                    .quickstep_authored_displacement_profile,
            );
            let force = quickstep_tracking_force_newtons(
                displacement,
                velocity,
                target,
                QUICKSTEP_FIXTURE_TOTAL_MASS_KG,
                peak_force,
                1.0 / SAMPLE_HZ,
            );
            velocity += force / QUICKSTEP_FIXTURE_TOTAL_MASS_KG / SAMPLE_HZ;
        } else {
            velocity = 0.0;
        }
        displacement += velocity / SAMPLE_HZ;
    }
    speeds
}

pub(super) fn quickstep_release_frame() -> usize {
    let config = TacticalCombatConfig::default();
    let motor = &config.movement.motor;
    let ticks = quickstep_push_ticks();
    let acceleration = quickstep_peak_horizontal_force_newtons(
        QUICKSTEP_FIXTURE_BIOLOGICAL_MASS_KG,
        QUICKSTEP_FIXTURE_LEG_STRENGTH,
        motor,
    ) / QUICKSTEP_FIXTURE_TOTAL_MASS_KG;
    let mut speed = 0.0;
    let mut displacement = 0.0;
    for tick in 0..ticks {
        speed +=
            acceleration * quickstep_force_curve((tick as f32 + 0.5) / ticks as f32) / SAMPLE_HZ;
        displacement += speed / SAMPLE_HZ;
        if displacement >= motor.quickstep_maximum_supported_root_displacement_metres {
            return tick + 1;
        }
    }
    ticks
}

pub(super) fn quickstep_landing_frame() -> usize {
    ((quickstep_release_frame() + 1)..(quickstep_release_frame() + SAMPLE_HZ as usize))
        .find(|&frame| quickstep_fixture_vertical_state(frame).0 <= 0.0)
        .expect("the quickstep fixture must return to ground within one second")
}

pub(super) fn quickstep_fixture_vertical_state(scenario_frame: usize) -> (f32, f32) {
    let config = TacticalCombatConfig::default();
    let motor = &config.movement.motor;
    let push_ticks = quickstep_push_ticks();
    let release_frame = quickstep_release_frame();
    let peak_acceleration = quickstep_peak_horizontal_force_newtons(
        QUICKSTEP_FIXTURE_BIOLOGICAL_MASS_KG,
        QUICKSTEP_FIXTURE_LEG_STRENGTH,
        motor,
    ) / QUICKSTEP_FIXTURE_TOTAL_MASS_KG
        * motor.quickstep_takeoff_angle_degrees.to_radians().tan();
    let mut height = 0.0;
    let mut velocity = 0.0;
    for tick in 0..scenario_frame {
        if tick < release_frame {
            velocity += peak_acceleration
                * quickstep_force_curve((tick as f32 + 0.5) / push_ticks as f32)
                / SAMPLE_HZ;
        } else {
            velocity -= motor.gravity_metres_per_second_squared / SAMPLE_HZ;
        }
        height += velocity / SAMPLE_HZ;
    }
    (height, velocity)
}

pub(super) fn quickstep_fixture_action_distance_metres() -> f32 {
    quickstep_scenario("quickstep-right", Vec2::X)
        .iter()
        .take(quickstep_action_ticks() + 1)
        .map(|frame| frame.speed / SAMPLE_HZ)
        .sum()
}

pub(super) fn downed_contact_scenario(name: &'static str, body: BodyState) -> Vec<PlannedFrame> {
    let speed = match body {
        BodyState::Prone => 2.0,
        BodyState::Supine => 0.8,
        _ => 0.0,
    };
    // Include a full review cycle after the pose-buffer startup settles. The
    // shorter probe ended just before the first loop seam, hiding precisely
    // the kind of crawl discontinuity this scenario is meant to diagnose.
    (0..=148)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::NEG_Y,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn downed_look_scenario() -> Vec<PlannedFrame> {
    (0..=64)
        .map(|scenario_frame| PlannedFrame {
            scenario: "downed-prone-look-at",
            scenario_frame,
            speed: 0.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::ZERO,
            camera_yaw: std::f32::consts::FRAC_PI_2,
            camera_pitch: 0.6,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn ordinary_camera_pitch_scenario() -> Vec<PlannedFrame> {
    (0..=64)
        .map(|scenario_frame| PlannedFrame {
            scenario: "ordinary-camera-pitch",
            scenario_frame,
            speed: 0.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::ZERO,
            camera_yaw: 0.0,
            camera_pitch: if scenario_frame < 16 {
                0.0
            } else if scenario_frame < 40 {
                0.6
            } else {
                -0.6
            },
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn posture_transition_scenario(
    name: &'static str,
    _start: BodyState,
) -> Vec<PlannedFrame> {
    // Stop just before the runtime-owned endpoint handoff. The viewer is
    // validating the authored transition arc; ordinary base-pose captures
    // validate the contact endpoint independently.
    (0..=80)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed: 0.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::ZERO,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn dive_impact_scenario(name: &'static str) -> Vec<PlannedFrame> {
    dive_impact_scenario_with_aim(name, false)
}

pub(super) fn aimed_dive_impact_scenario(name: &'static str) -> Vec<PlannedFrame> {
    dive_impact_scenario_with_aim(name, true)
}

pub(super) fn dive_impact_scenario_with_aim(name: &'static str, aimed: bool) -> Vec<PlannedFrame> {
    let local_direction = if name.starts_with("dive-forward") {
        Vec2::NEG_Y
    } else if name.starts_with("dive-backward") {
        Vec2::Y
    } else if name.starts_with("dive-left") {
        Vec2::NEG_X
    } else if name.starts_with("dive-right") {
        Vec2::X
    } else {
        Vec2::ZERO
    };
    let final_frame = if name.starts_with("dive-backward") {
        56
    } else {
        48
    };
    (0..=final_frame)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            // Match the server's camera-relative launch rather than judging a
            // directional pose on a stationary root. Retain velocity through
            // the first terrain-contact sample so travel and body orientation
            // can be compared across the complete airborne arc.
            speed: if scenario_frame <= 17 {
                TACTICAL_DIVE_HORIZONTAL_SPEED_METRES_PER_SECOND
            } else {
                0.0
            },
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction,
            camera_yaw: if aimed { 0.85 } else { 0.0 },
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: if aimed {
                WeaponGuardState::Raised
            } else {
                WeaponGuardState::Lowered
            },
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn jump_charge_scenario() -> Vec<PlannedFrame> {
    (0..=64)
        .map(|scenario_frame| PlannedFrame {
            scenario: "jump-charge-anticipation",
            scenario_frame,
            speed: 0.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::ZERO,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn downed_body_for_scenario(scenario: &str) -> Option<BodyState> {
    match scenario {
        "downed-prone-crawl" | "downed-prone-look-at" => Some(BodyState::Prone),
        "downed-supine-scamper" => Some(BodyState::Supine),
        "full-ragdoll" => Some(BodyState::Ragdolled),
        _ => None,
    }
}

pub(super) fn required_motion_for_scenario(scenario: &str) -> Option<&'static str> {
    if scenario.starts_with("dive-") {
        return Some("dive");
    }
    match scenario {
        "downed-prone-crawl" => Some("prone_crawl"),
        "downed-supine-scamper" => Some("supine_scamper"),
        "downed-prone-look-at" => Some("prone_idle"),
        "prone-get-up" => Some("prone_transition"),
        "supine-get-up" => Some("supine_transition"),
        "prone-roll-left" => Some("prone_supine_roll_left"),
        "prone-roll-right" => Some("prone_supine_roll_right"),
        _ => None,
    }
}

pub(super) fn transition_for_scenario(
    scenario: &str,
) -> Option<(BodyState, PostureTransitionKind)> {
    let upright = BodyState::Grounded(GroundedPosture::Upright);
    let dive_direction = if scenario.starts_with("dive-forward") {
        Some(DiveDirection::Forward)
    } else if scenario.starts_with("dive-backward") {
        Some(DiveDirection::Backward)
    } else if scenario.starts_with("dive-left") {
        Some(DiveDirection::Left)
    } else if scenario.starts_with("dive-right") {
        Some(DiveDirection::Right)
    } else {
        None
    };
    if let Some(direction) = dive_direction {
        return Some((
            upright,
            PostureTransitionKind::DiveToDowned {
                direction,
                trajectory: DiveTrajectory::Airborne,
            },
        ));
    }
    match scenario {
        "prone-get-up" => Some((BodyState::Prone, PostureTransitionKind::ProneToUpright)),
        "supine-get-up" => Some((BodyState::Supine, PostureTransitionKind::SupineToUpright)),
        "prone-roll-left" => Some((
            BodyState::Prone,
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Left,
            },
        )),
        "prone-roll-right" => Some((
            BodyState::Prone,
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Right,
            },
        )),
        _ => None,
    }
}

pub(super) fn turning_scenario(name: &'static str, reversal: bool) -> Vec<PlannedFrame> {
    (0..=64)
        .map(|frame| {
            let progress = frame as f32 / 64.0;
            PlannedFrame {
                scenario: name,
                scenario_frame: frame,
                speed: 2.0,
                time_seconds: progress,
                local_direction: Vec2::NEG_Y,
                camera_yaw: if reversal && frame > 0 {
                    std::f32::consts::PI
                } else if reversal {
                    0.0
                } else {
                    std::f32::consts::FRAC_PI_2 * progress
                },
                camera_pitch: 0.55 * progress,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn dynamics_turn_scenario(name: &'static str, angle_radians: f32) -> Vec<PlannedFrame> {
    (0..=64)
        .map(|frame| {
            let progress = frame as f32 / 64.0;
            PlannedFrame {
                scenario: name,
                scenario_frame: frame,
                speed: 5.5,
                time_seconds: progress,
                local_direction: Vec2::NEG_Y,
                camera_yaw: angle_radians * progress,
                camera_pitch: 0.0,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn guard_plant_turn_scenario() -> Vec<PlannedFrame> {
    (0..=64)
        .map(|frame| {
            let progress = frame as f32 / 64.0;
            PlannedFrame {
                scenario: "planted-guard-turn",
                scenario_frame: frame,
                speed: 0.35,
                time_seconds: progress,
                local_direction: Vec2::X,
                camera_yaw: std::f32::consts::FRAC_PI_2 * progress,
                camera_pitch: 0.0,
                action: SkeletonAction::Block,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn raised_guard_stationary_turn_scenario() -> Vec<PlannedFrame> {
    (0..=127)
        .map(|scenario_frame| {
            let turn_progress = (scenario_frame as f32 / 64.0).clamp(0.0, 1.0);
            PlannedFrame {
                scenario: "raised-guard-stationary-turn",
                scenario_frame,
                speed: 0.0,
                time_seconds: scenario_frame as f32 / SAMPLE_HZ,
                local_direction: Vec2::ZERO,
                camera_yaw: std::f32::consts::FRAC_PI_2 * smoothstep01(turn_progress),
                camera_pitch: 0.0,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Raised,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

pub(super) fn raised_guard_scenario(name: &'static str, direction: Vec2) -> Vec<PlannedFrame> {
    let direction = direction.normalize_or_zero();
    // Leave enough post-startup time for both local support identities to
    // complete. One semantic cycle could end before the client-owned first
    // landing after asset/pose readiness and therefore never test alternation.
    raised_guard_steady_scenario(name, 2.0, 2.0, direction)
}

pub(super) fn raised_guard_steady_scenario(
    name: &'static str,
    speed: f32,
    cycles: f32,
    direction: Vec2,
) -> Vec<PlannedFrame> {
    raised_guard_steady_scenario_with_lead(name, speed, cycles, direction, LeadFoot::Left)
}

pub(super) fn raised_guard_steady_scenario_with_lead(
    name: &'static str,
    speed: f32,
    cycles: f32,
    direction: Vec2,
    lead_foot: LeadFoot,
) -> Vec<PlannedFrame> {
    let duration = cycles * guard_step_length(speed) * 2.0 / speed;
    let last_frame = (duration * SAMPLE_HZ).ceil() as usize + 1;
    (0..=last_frame)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: direction.normalize_or_zero(),
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

pub(super) fn raised_guard_acceleration_scenario() -> Vec<PlannedFrame> {
    raised_guard_acceleration_scenario_with_lead(
        "raised-guard-accelerate-from-rest",
        LeadFoot::Left,
    )
}

pub(super) fn raised_guard_acceleration_scenario_with_lead(
    name: &'static str,
    lead_foot: LeadFoot,
) -> Vec<PlannedFrame> {
    (0..=96)
        .map(|scenario_frame| {
            let time_seconds = scenario_frame as f32 / SAMPLE_HZ;
            PlannedFrame {
                scenario: name,
                scenario_frame,
                speed: (time_seconds / 0.5).clamp(0.0, 1.0) * 2.0,
                time_seconds,
                local_direction: Vec2::X,
                camera_yaw: 0.0,
                camera_pitch: 0.0,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Raised,
                lead_foot,
            }
        })
        .collect()
}

pub(super) fn raised_guard_transition_scenario() -> Vec<PlannedFrame> {
    (0..=64)
        .map(|scenario_frame| PlannedFrame {
            scenario: "raised-guard-transition",
            scenario_frame,
            speed: 2.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::NEG_Y,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: if scenario_frame < 16 {
                WeaponGuardState::Lowered
            } else {
                WeaponGuardState::Raised
            },
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

pub(super) fn raised_guard_release_scenario() -> Vec<PlannedFrame> {
    raised_guard_release_scenario_with_lead("raised-guard-release-at-peak", LeadFoot::Left)
}

pub(super) fn raised_guard_release_scenario_with_lead(
    name: &'static str,
    lead_foot: LeadFoot,
) -> Vec<PlannedFrame> {
    let release_frame = (guard_step_length(2.0) * 0.75 * SAMPLE_HZ).round() as usize;
    (0..=64)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed: if scenario_frame <= release_frame {
                2.0
            } else {
                0.0
            },
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::NEG_Y,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

pub(super) fn raised_guard_reversal_scenario() -> Vec<PlannedFrame> {
    raised_guard_reversal_scenario_with_lead("raised-guard-left-right-reversal", LeadFoot::Left)
}

pub(super) fn raised_guard_reversal_scenario_with_lead(
    name: &'static str,
    lead_foot: LeadFoot,
) -> Vec<PlannedFrame> {
    (0..=64)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed: 2.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: if scenario_frame < 16 {
                Vec2::NEG_X
            } else {
                Vec2::X
            },
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quickstep_fixture_keeps_feet_airborne_until_action_end() {
        assert_eq!(quickstep_push_ticks(), 25);
        assert_eq!(quickstep_release_frame(), 11);
        assert_eq!(quickstep_landing_frame(), 22);
        assert!(quickstep_landing_frame() < quickstep_action_ticks());
    }

    #[test]
    fn johns_quickstep_fixture_covers_about_one_metre_in_half_a_second() {
        let duration = quickstep_action_ticks() as f32 / SAMPLE_HZ;
        let distance = quickstep_fixture_action_distance_metres();
        assert!((0.49..=0.51).contains(&duration), "duration={duration}");
        assert!((0.90..=1.10).contains(&distance), "distance={distance}");
    }

    #[test]
    fn every_directional_dive_scenario_requires_the_shared_motion() {
        for scenario in [
            "dive-forward",
            "dive-backward-impact",
            "dive-left-aimed-impact",
            "dive-right",
        ] {
            assert_eq!(required_motion_for_scenario(scenario), Some("dive"));
        }
    }

    #[test]
    fn steady_scenarios_use_authoritative_fixed_tick_samples() {
        for (name, speed) in [("walk", 2.0), ("blend", 3.75), ("run", 5.5)] {
            let frames = steady_scenario(name, speed, 2.0);
            assert_eq!(frames.first().unwrap().time_seconds, 0.0);
            assert!(frames.len() > 30);
            for pair in frames.windows(2) {
                assert!(pair[1].time_seconds > pair[0].time_seconds);
                assert!(
                    (pair[1].time_seconds - pair[0].time_seconds - 1.0 / SAMPLE_HZ).abs() < 0.0001
                );
            }
        }
    }

    #[test]
    fn raised_tap_stop_adversaries_enable_terrain_ik_without_changing_gate_kind() {
        for (name, direction) in [
            ("raised-guard-tap-stop-left", Vec2::NEG_X),
            ("raised-guard-tap-stop-right", Vec2::X),
        ] {
            let frames = raised_guard_lateral_tap_stop_scenario(name, direction);
            assert_eq!(scenario_metadata(name).kind, ScenarioKind::RaisedGuard);
            assert!(scenario_uses_terrain_ik(name));
            assert!(!raised_scenario_requires_zero_flight(name));
            assert!(frames.iter().all(terrain_ik_enabled_for_frame));
        }
    }

    #[test]
    fn terrain_and_steady_raised_gate_classification_remain_distinct() {
        assert!(scenario_uses_terrain_ik("cross-slope-walk"));
        assert!(!scenario_uses_terrain_ik("raised-guard-forward"));
        assert!(!scenario_uses_terrain_ik("raised-guard-stationary-turn"));
        assert!(!scenario_uses_terrain_ik("steady-walk-2.0"));
        assert!(raised_scenario_requires_zero_flight("raised-guard-forward"));
        assert!(raised_scenario_requires_zero_flight(
            "raised-guard-stationary-turn"
        ));
        assert!(!raised_scenario_requires_zero_flight(
            "raised-guard-tap-stop-right"
        ));
        assert!(!raised_scenario_requires_zero_flight("cross-slope-walk"));

        let stationary_turn = scenario_metadata("raised-guard-stationary-turn");
        assert_eq!(stationary_turn.kind, ScenarioKind::RaisedGuard);
        assert!(!stationary_turn.repeatable);
        assert!(stationary_turn.procedural_solver);
    }

    #[test]
    fn transition_uses_server_stride_formula_without_non_finite_state() {
        let frames = transition_scenario();
        assert_eq!(frames.len(), 257);
        assert!(frames.iter().all(|frame| frame.speed.is_finite()
            && frame.time_seconds.is_finite()
            && frame.local_direction.is_finite()));
        assert_eq!(frames.first().unwrap().speed, 0.0);
        assert_eq!(frames.last().unwrap().speed, 0.0);
    }

    #[test]
    fn threshold_stress_hits_both_hysteresis_edges_and_crossings() {
        let speeds = terrain_threshold_chatter_scenario()
            .into_iter()
            .map(|frame| frame.speed)
            .collect::<Vec<_>>();
        for expected in [0.079, 0.081, 0.029, 0.031, 0.02, 0.09] {
            assert!(
                speeds.contains(&expected),
                "missing threshold sample {expected}"
            );
        }
    }

    #[test]
    fn capture_plan_covers_raised_guard_directions_and_gameplay_transition() {
        let plan = capture_plan();
        for scenario in [
            "raised-guard-forward",
            "raised-guard-backward",
            "raised-guard-left",
            "raised-guard-right",
            "raised-guard-forward-left",
            "raised-guard-forward-right",
            "raised-guard-backward-left",
            "raised-guard-backward-right",
        ] {
            assert!(plan.iter().any(|frame| {
                frame.scenario == scenario && frame.weapon_guard == WeaponGuardState::Raised
            }));
        }
        let stationary_turn = plan
            .iter()
            .filter(|frame| frame.scenario == "raised-guard-stationary-turn")
            .collect::<Vec<_>>();
        assert_eq!(stationary_turn.len(), 128);
        assert!(stationary_turn.iter().all(|frame| {
            frame.speed == 0.0
                && frame.local_direction == Vec2::ZERO
                && frame.weapon_guard == WeaponGuardState::Raised
        }));
        assert!(stationary_turn.first().unwrap().camera_yaw.abs() < 0.0001);
        assert!(
            (stationary_turn.last().unwrap().camera_yaw - std::f32::consts::FRAC_PI_2).abs()
                < 0.0001
        );
        let transition = plan
            .iter()
            .filter(|frame| frame.scenario == "raised-guard-transition")
            .collect::<Vec<_>>();
        assert!(
            transition
                .iter()
                .any(|frame| { frame.weapon_guard == WeaponGuardState::Lowered })
        );
        assert!(
            transition
                .iter()
                .any(|frame| { frame.weapon_guard == WeaponGuardState::Raised })
        );
        assert_eq!(
            transition.first().unwrap().weapon_guard,
            WeaponGuardState::Lowered
        );
        assert_eq!(
            transition.last().unwrap().weapon_guard,
            WeaponGuardState::Raised
        );
        for scenario in [
            "raised-guard-right-support-left",
            "raised-guard-right-support-right",
            "raised-guard-right-support-forward-right",
            "raised-guard-right-support-accelerate",
            "raised-guard-right-support-release",
            "raised-guard-right-support-reversal",
        ] {
            assert!(plan.iter().any(|frame| {
                frame.scenario == scenario
                    && frame.weapon_guard == WeaponGuardState::Raised
                    && frame.lead_foot == LeadFoot::Right
            }));
        }
    }

    #[test]
    fn raised_guard_viewer_scenarios_cross_complete_step_cycle_with_fixed_lead() {
        for scenario in [
            "raised-guard-forward",
            "raised-guard-backward",
            "raised-guard-left",
            "raised-guard-right",
            "raised-guard-forward-left",
            "raised-guard-forward-right",
            "raised-guard-backward-left",
            "raised-guard-backward-right",
        ] {
            let mut skeleton = SkeletonState::default();
            set_weapon_guard(&mut skeleton, WeaponGuardState::Raised);
            let mut phases = Vec::new();
            for frame in capture_plan()
                .into_iter()
                .filter(|frame| frame.scenario == scenario)
            {
                project_skeleton_locomotion(
                    &mut skeleton,
                    SkeletonLocomotionInput {
                        orientation: Quat::IDENTITY,
                        linear_velocity: Vec3::new(
                            frame.local_direction.x,
                            0.0,
                            frame.local_direction.y,
                        ) * frame.speed,
                        grounded: true,
                        delta_seconds: if frame.scenario_frame == 0 {
                            0.0
                        } else {
                            1.0 / SAMPLE_HZ
                        },
                        tick: frame.scenario_frame as u64,
                    },
                );
                assert_eq!(skeleton.lead_foot, frame.lead_foot);
                let evaluation = AnimationEvaluation::from_skeleton(&skeleton);
                assert_eq!(evaluation.base.len(), 1);
                assert_eq!(evaluation.base[0].pose, SemanticPose::GuardThrust);
                assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
                phases.push(skeleton.gait_phase);
            }
            assert!(phases.windows(2).any(|pair| pair[1] < pair[0]));
            assert!(phases.iter().any(|&phase| phase >= 0.5));
        }
    }

    #[test]
    fn raised_guard_viewer_finishes_release_and_updates_reversal_without_phase_reset() {
        let replay = |scenario: &str| {
            let mut skeleton = SkeletonState::default();
            set_weapon_guard(&mut skeleton, WeaponGuardState::Raised);
            let mut samples = Vec::new();
            for frame in capture_plan()
                .into_iter()
                .filter(|frame| frame.scenario == scenario)
            {
                project_skeleton_locomotion(
                    &mut skeleton,
                    SkeletonLocomotionInput {
                        orientation: Quat::IDENTITY,
                        linear_velocity: Vec3::new(
                            frame.local_direction.x,
                            0.0,
                            frame.local_direction.y,
                        ) * frame.speed,
                        grounded: true,
                        delta_seconds: if frame.scenario_frame == 0 {
                            0.0
                        } else {
                            1.0 / SAMPLE_HZ
                        },
                        tick: frame.scenario_frame as u64,
                    },
                );
                samples.push((
                    frame.scenario_frame,
                    skeleton.gait_phase,
                    skeleton.raised_locomotion(),
                ));
            }
            samples
        };

        let release = replay("raised-guard-release-at-peak");
        let release_frame = (guard_step_length(2.0) * 0.75 * SAMPLE_HZ).round() as usize;
        assert!(
            release
                .iter()
                .any(|(frame, phase, intent)| *frame > release_frame
                    && *phase > 0.5
                    && intent.is_moving())
        );
        assert!(!release.last().unwrap().2.is_moving());
        assert_eq!(release.last().unwrap().1, 0.0);

        let reversal = replay("raised-guard-left-right-reversal");
        let changed = reversal
            .iter()
            .find(|(_, _, intent)| intent.local_direction() == Vec2::X)
            .expect("reversal observation is accepted immediately");
        let previous_phase = reversal
            .iter()
            .find(|(frame, _, _)| *frame == 15)
            .expect("pre-reversal sample")
            .1;
        assert_eq!(changed.0, 16);
        let phase_delta = (changed.1 - previous_phase).rem_euclid(1.0);
        // A reversal may make the currently committed contact immediately
        // due and begin the opposite half-step. The authoritative guard plan
        // therefore does not promise constant phase velocity across this
        // tick; it promises forward continuity without skipping more than one
        // contact interval (one half of the normalized cycle).
        const MAXIMUM_REVERSAL_PHASE_ADVANCE: f32 = 0.5;
        assert!(
            phase_delta > 0.0 && phase_delta <= MAXIMUM_REVERSAL_PHASE_ADVANCE + 0.001,
            "reversal phase advanced by {phase_delta}, expected (0, {MAXIMUM_REVERSAL_PHASE_ADVANCE}]"
        );
    }

    #[test]
    fn guard_and_attack_captures_use_prepared_runtime_pose_assets() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in ["swing.glb", "thrust.glb", "offhand.glb"] {
            let source = root
                .join("assets_src/biped/unarmed")
                .join(name.replace(".glb", ".casc"));
            let runtime = root.join("assets/animations/biped/unarmed").join(name);
            assert!(source.is_file(), "missing authored source {source:?}");
            let runtime_bytes = fs::read(&runtime).unwrap_or_else(|error| {
                panic!("missing prepared runtime asset {runtime:?}: {error}")
            });
            assert!(
                runtime_bytes.starts_with(b"glTF") && runtime_bytes.len() > 20,
                "prepared runtime asset is not a non-empty binary glTF: {runtime:?}"
            );
        }
    }
}
