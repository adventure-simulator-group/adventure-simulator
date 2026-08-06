//! A deterministic gameplay-presentation fixture for tactical animation review.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use adventuresim_tactical_core::physics::AdventureSimulatorPhysicsPlugin;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::client::WeaponGuardInputState;
use bevy::{
    app::AppExit,
    asset::io::AssetSourceBuilder,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::animation::{
    AnimationPlayback, ArmIkState, AttackFootworkState, BoneRole, HumanoidBone, LegIkState,
    LocomotionBodyResponseState, LocomotionHeightState, LocomotionPresentationEvent,
    LocomotionPresentationEventKind, MEASURED_ANKLE_SOLE_OFFSET_METRES, PresentedSkeleton,
    ProceduralAnimationClock, RaisedFootworkState, SOLE_CONTACT_TOLERANCE_METRES,
    TacticalAnimationPlugin, TerrainIkEnabled, locomotion_support_weights,
};
use crate::{
    camera::{CameraMode, TacticalCameraPlugin, TacticalCameraSet, third_person_offset},
    player::{LocalCharacterId, PlayerPlugin},
    presentation::TacticalPresentationPlugin,
};

const SAMPLE_HZ: f32 = LOCOMOTION_SAMPLE_HZ;
const CAPTURE_ROOT_GROUND_OFFSET_METRES: f32 = 0.95;
const FULL_PLANT_SUPPORT_WEIGHT: f32 = 0.99;
const RAISED_MINIMUM_INTER_FOOT_SEPARATION_METRES: f32 = 0.16;
const RAISED_MAXIMUM_PELVIS_VERTICAL_STEP_METRES: f32 = 0.05;
const ATTACK_MAXIMUM_CONSTRAINED_TARGET_STEP_METRES: f32 = 0.201;
// The parent guard-entry fixture already moves 33.13 mm in one sampled frame.
// Allow less than 4 mm of additional height-system overhead without weakening
// the broader raised-guard continuity bound.
const LOCOMOTION_STATE_MAXIMUM_PELVIS_VERTICAL_STEP_METRES: f32 = 0.04;
const ORDINARY_VERTICAL_RANGE_LIMIT_METRES: f32 = 0.20;
const RAISED_GUARD_VERTICAL_RANGE_LIMIT_METRES: f32 = 0.30;
// Ignore sub-3 mm sampling noise, but reject any additional visible beat in
// the phase-owned vertical curve.
const HEIGHT_PEAK_PROMINENCE_METRES: f32 = 0.003;
const PASSING_PEAK_PHASE_WINDOW: f32 = 0.10;
const VIEWS: [CaptureView; 3] = [CaptureView::Gameplay, CaptureView::Side, CaptureView::Front];
const TRACKED_BONE_NAMES: [&str; 15] = [
    "pelvis",
    "chest",
    "head",
    "left_shoulder",
    "right_shoulder",
    "left_elbow",
    "right_elbow",
    "left_hand",
    "right_hand",
    "left_hip",
    "right_hip",
    "left_knee",
    "right_knee",
    "left_foot",
    "right_foot",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenarioKind {
    Ordinary,
    RaisedGuard,
    Terrain,
    Transition,
    Landing,
    Attack,
}

#[derive(Debug, Clone, Copy)]
struct ScenarioMetadata {
    kind: ScenarioKind,
    repeatable: bool,
    procedural_solver: bool,
}

fn scenario_metadata(name: &str) -> ScenarioMetadata {
    if name == "terrain-toggle-mid-stride" {
        ScenarioMetadata {
            kind: ScenarioKind::Terrain,
            repeatable: false,
            // This scenario deliberately spends time with the solver off. Its
            // contract is bounded transition continuity, not steady plants.
            procedural_solver: false,
        }
    } else if name == "cross-slope-walk" || name.starts_with("terrain-") {
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
    } else if name.starts_with("attack-step-") {
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
                && name != "raised-guard-transition",
            procedural_solver: true,
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

pub(crate) fn run(
    output: PathBuf,
    asset_root: PathBuf,
    settle_frames: u32,
    scenario: Option<&str>,
) -> AppExit {
    fs::create_dir_all(&output).unwrap_or_else(|error| {
        panic!("failed to create animation capture directory {output:?}: {error}")
    });
    invalidate_previous_report(&output);
    let initial_terrain_ik = scenario.is_some_and(|name| {
        scenario_metadata(name).kind == ScenarioKind::Terrain || name.contains("terrain")
    });

    let workspace_asset_source =
        AssetSourceBuilder::platform_default(&asset_root.to_string_lossy(), None);
    App::new()
        .register_asset_source("workspace", workspace_asset_source)
        // The live debug client registers the same default through
        // `DebugPlugin`. The fixture does not install that input/network
        // plugin, so mirror its presentation default explicitly.
        .register_required_components_with::<Collider, _>(DebugRender::none)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Adventure Simulator Animation Review Capture".into(),
                        resolution: (960, 720).into(),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((
            AdventureSimulatorCorePlugins
                .build()
                .set(AdventureSimulatorPhysicsPlugin {
                    enable_simulation: false,
                }),
            EnhancedInputPlugin,
        ))
        .add_plugins((
            PlayerPlugin,
            TacticalAnimationPlugin,
            TacticalCameraPlugin,
            TacticalPresentationPlugin::default(),
        ))
        .insert_resource(LocalCharacterId(0))
        .insert_resource(CameraMode { third_person: true })
        .insert_resource(WeaponGuardInputState::default())
        .insert_resource(Time::<Fixed>::from_hz(SAMPLE_HZ as f64))
        // Individual scenarios select terrain conformity explicitly so the
        // viewer can retain FK-only controls after the live default changed.
        .insert_resource(TerrainIkEnabled(initial_terrain_ik))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.1, 0.13)))
        .insert_resource(CaptureSequence::new(output, settle_frames, scenario))
        .add_systems(Startup, setup_viewer)
        .add_systems(PreUpdate, drive_sequence)
        .add_systems(
            PostUpdate,
            position_capture_camera
                .after(TacticalCameraSet::Offset)
                .before(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            draw_skeleton_overlay.after(TransformSystems::Propagate),
        )
        .add_systems(
            Last,
            (collect_locomotion_presentation_events, capture_frame).chain(),
        )
        .run()
}

#[derive(Component)]
struct CaptureSubject;

#[derive(Component)]
struct CaptureLabel;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaptureView {
    Gameplay,
    Side,
    Front,
}

impl CaptureView {
    fn slug(self) -> &'static str {
        match self {
            Self::Gameplay => "gameplay",
            Self::Side => "side",
            Self::Front => "front",
        }
    }
}

#[derive(Debug, Clone)]
struct PlannedFrame {
    scenario: &'static str,
    scenario_frame: usize,
    speed: f32,
    time_seconds: f32,
    local_direction: Vec2,
    camera_yaw: f32,
    camera_pitch: f32,
    crouching: bool,
    action: SkeletonAction,
    weapon_guard: WeaponGuardState,
    lead_foot: LeadFoot,
}

#[derive(Resource)]
struct CaptureSequence {
    output: PathBuf,
    settle_frames: u32,
    plan: Vec<PlannedFrame>,
    index: usize,
    view_index: usize,
    applied: bool,
    settled: u32,
    waiting: u32,
    capture_in_flight: bool,
    view_fingerprints: Vec<u64>,
    duplicate_view_frames: Vec<String>,
    samples: Vec<FrameSample>,
    presentation_events: Vec<PresentationEventSample>,
    repeated_evaluation_baseline: Option<RepeatedEvaluationSnapshot>,
    repeated_evaluation_valid: bool,
    active_scenario: Option<&'static str>,
    simulation_tick: u64,
    scenario_distance: f32,
}

impl CaptureSequence {
    fn new(output: PathBuf, settle_frames: u32, scenario: Option<&str>) -> Self {
        let plan = capture_plan()
            .into_iter()
            .filter(|frame| scenario.is_none_or(|scenario| frame.scenario == scenario))
            .collect::<Vec<_>>();
        assert!(
            !plan.is_empty(),
            "requested animation capture scenario is unknown"
        );
        Self {
            output,
            settle_frames: settle_frames.max(1),
            plan,
            index: 0,
            view_index: 0,
            applied: false,
            settled: 0,
            waiting: 0,
            capture_in_flight: false,
            view_fingerprints: Vec::with_capacity(VIEWS.len()),
            duplicate_view_frames: Vec::new(),
            samples: Vec::new(),
            presentation_events: Vec::new(),
            repeated_evaluation_baseline: None,
            repeated_evaluation_valid: true,
            active_scenario: None,
            simulation_tick: 0,
            scenario_distance: 0.0,
        }
    }
}

struct RepeatedEvaluationSnapshot {
    scenario: &'static str,
    scenario_frame: usize,
    bones: BTreeMap<String, BoneSample>,
    contact_sequence: u64,
    landing_sequence: u64,
    event_count: usize,
}

#[derive(Debug, Serialize)]
struct CaptureManifest {
    sample_hz: f32,
    pipeline: &'static str,
    views: [CaptureView; 3],
    validation: CaptureValidation,
    scenarios: Vec<ScenarioMetrics>,
    frames: Vec<FrameSample>,
    presentation_events: Vec<PresentationEventSample>,
}

#[derive(Debug, Serialize)]
struct PresentationEventSample {
    scenario: String,
    scenario_frame: usize,
    owner: String,
    sequence: u64,
    sample_tick: u64,
    kind: String,
}

#[derive(Debug, Serialize)]
struct CaptureValidation {
    finite_transforms: bool,
    all_scenarios_complete: bool,
    all_artifacts_written: bool,
    continuity_within_review_bounds: bool,
    biomechanics_within_review_bounds: bool,
    no_ground_penetration: bool,
    raised_guard_fixed_lead: bool,
    flat_controller_height_stable: bool,
    phase_owned_height_valid: bool,
    run_flight_valid: bool,
    body_response_valid: bool,
    speed_ramp_phase_continuity_valid: bool,
    contact_sequences_valid: bool,
    cadence_contact_valid: bool,
    event_stream_valid: bool,
    landing_response_valid: bool,
    landing_foot_preservation_valid: bool,
    ordinary_swing_tracking_valid: bool,
    reported_support_contacts_valid: bool,
    stop_settle_capture_valid: bool,
    final_support_balance_valid: bool,
    hard_stop_maximum_pelvis_step_metres: Option<f32>,
    hard_stop_height_continuity_valid: bool,
    repeated_evaluation_valid: bool,
    attack_footwork_valid: bool,
    views_are_distinct: bool,
    duplicate_view_frames: Vec<String>,
    note: &'static str,
}

fn invalidate_previous_report(output: &std::path::Path) {
    for name in ["manifest.json", "index.html", "failure.txt"] {
        let path = output.join(name);
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            panic!("failed to invalidate previous animation report {path:?}: {error}");
        }
    }
}

fn capture_artifacts_written(output: &std::path::Path, frames: &[FrameSample]) -> bool {
    frames.iter().all(|frame| {
        VIEWS.iter().all(|view| {
            frame
                .screenshots
                .get(view.slug())
                .and_then(|name| fs::metadata(output.join(name)).ok())
                .is_some_and(|metadata| metadata.len() > 0)
        })
    })
}

#[derive(Debug, Serialize)]
struct ScenarioMetrics {
    scenario: String,
    frame_count: usize,
    maximum_root_relative_step_metres: f32,
    maximum_leg_root_relative_step_metres: f32,
    maximum_foot_root_relative_step_metres: f32,
    maximum_knee_root_relative_step_metres: f32,
    worst_displacement: Option<ContinuityLocation>,
    maximum_bone_rotation_step_degrees: f32,
    worst_rotation: Option<ContinuityLocation>,
    loop_seam_position_metres: Option<f32>,
    loop_seam_rotation_degrees: Option<f32>,
    pelvis_vertical_range_metres: f32,
    maximum_pelvis_vertical_step_metres: f32,
    controller_vertical_range_metres: f32,
    phase_height_range_metres: f32,
    contact_to_passing_height_gain_metres: f32,
    visual_height_peak_count: usize,
    visual_height_peaks_in_passing_windows: bool,
    maximum_no_support_seconds: f32,
    minimum_flight_sole_clearance_metres: f32,
    minimum_contact_sole_clearance_metres: f32,
    maximum_contact_sole_clearance_metres: f32,
    head_vertical_range_metres: f32,
    foot_terrain_relief_metres: f32,
    minimum_knee_forward_bend_metres: f32,
    minimum_signed_foot_track_metres: f32,
    minimum_inter_foot_separation_metres: f32,
    minimum_knee_flexion_degrees: f32,
    minimum_knee_hemisphere_dot: f32,
    maximum_facing_motion_error_degrees: f32,
    maximum_facing_tracking_excess_degrees: f32,
    maximum_guard_facing_error_degrees: f32,
    final_facing_motion_error_degrees: f32,
    maximum_supported_foot_slip_metres_per_frame: f32,
    maximum_planted_foot_drift_metres: f32,
    minimum_foot_clearance_metres: f32,
}

#[derive(Debug, Clone, Serialize)]
struct ContinuityLocation {
    bone: String,
    from_frame: usize,
    to_frame: usize,
    value: f32,
}

#[derive(Debug, Clone, Serialize)]
struct FrameSample {
    scenario: String,
    scenario_frame: usize,
    time_seconds: f32,
    speed_metres_per_second: f32,
    gait_phase: f32,
    locomotion_sample_tick: u64,
    body_acceleration: [f32; 3],
    world_acceleration: [f32; 3],
    contact_sequence: u64,
    contact_foot: LeadFoot,
    landing_sequence: u64,
    landing_impact_speed: f32,
    body_lean_pitch_degrees: f32,
    body_lean_roll_degrees: f32,
    landing_compression_metres: f32,
    root_distance_metres: f32,
    root_position_metres: [f32; 3],
    world_travel_direction: [f32; 3],
    desired_body_forward_direction: [f32; 3],
    body_forward_direction: [f32; 3],
    body_rotation_xyzw: [f32; 4],
    weapon_guard: WeaponGuardState,
    lead_foot: LeadFoot,
    action: SkeletonAction,
    action_phase: f32,
    attack_step: AttackStep,
    guard_action: bool,
    left_support_weight: f32,
    right_support_weight: f32,
    desired_left_foot_target: Option<[f32; 3]>,
    desired_right_foot_target: Option<[f32; 3]>,
    ik_left_authored_target: Option<[f32; 3]>,
    ik_right_authored_target: Option<[f32; 3]>,
    ik_left_planned_contact: Option<[f32; 3]>,
    ik_right_planned_contact: Option<[f32; 3]>,
    ik_settle_capture_point: Option<[f32; 3]>,
    ik_left_solve_target: Option<[f32; 3]>,
    ik_right_solve_target: Option<[f32; 3]>,
    ik_left_support_weight: f32,
    ik_right_support_weight: f32,
    ik_left_release_active: bool,
    ik_right_release_active: bool,
    ik_left_release_target: Option<[f32; 3]>,
    ik_right_release_target: Option<[f32; 3]>,
    ik_settle_progress: Option<f32>,
    attack_requested_left_foot_target: Option<[f32; 3]>,
    attack_requested_right_foot_target: Option<[f32; 3]>,
    attack_constrained_left_foot_target: Option<[f32; 3]>,
    attack_constrained_right_foot_target: Option<[f32; 3]>,
    attack_support_handoffs: u8,
    attack_maximum_reach_yield_metres: f32,
    screenshots: BTreeMap<String, String>,
    bones: BTreeMap<String, BoneSample>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct BoneSample {
    position: [f32; 3],
    rotation_xyzw: [f32; 4],
    terrain_clearance_metres: Option<f32>,
}

fn steady_scenario(name: &'static str, speed: f32, cycles: f32) -> Vec<PlannedFrame> {
    steady_scenario_in_direction(name, speed, cycles, Vec2::NEG_Y)
}

fn steady_scenario_in_direction(
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
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn transition_scenario() -> Vec<PlannedFrame> {
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
                crouching: false,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn crouch_steady_scenario() -> Vec<PlannedFrame> {
    let mut frames = steady_scenario("steady-crouch-1.5", 1.5, 2.0);
    for frame in &mut frames {
        frame.crouching = true;
    }
    frames
}

fn terrain_crouch_scenario() -> Vec<PlannedFrame> {
    let mut frames = steady_scenario_in_direction("terrain-crouch-cross-slope", 1.5, 3.0, Vec2::X);
    for frame in &mut frames {
        frame.crouching = true;
    }
    frames
}

fn terrain_toggle_scenario() -> Vec<PlannedFrame> {
    (0..=112)
        .map(|scenario_frame| PlannedFrame {
            scenario: "terrain-toggle-mid-stride",
            scenario_frame,
            speed: 2.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::X,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn terrain_half_turn_reversal_scenario() -> Vec<PlannedFrame> {
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
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn scenario_uses_terrain_ik(scenario: &str) -> bool {
    scenario_metadata(scenario).kind == ScenarioKind::Terrain
        || scenario.starts_with("raised-guard-tap-stop-")
}

fn terrain_ik_enabled_for_frame(frame: &PlannedFrame) -> bool {
    (scenario_uses_terrain_ik(frame.scenario) || frame.scenario.contains("terrain"))
        && (frame.scenario != "terrain-toggle-mid-stride"
            || (16..80).contains(&frame.scenario_frame))
}

fn raised_scenario_requires_zero_flight(scenario: &str) -> bool {
    scenario_metadata(scenario).kind == ScenarioKind::RaisedGuard
        && !scenario.starts_with("raised-guard-tap-stop-")
}

fn crouch_transition_scenario() -> Vec<PlannedFrame> {
    (0_usize..=96)
        .map(|scenario_frame| PlannedFrame {
            scenario: "crouch-enter-exit",
            scenario_frame,
            speed: 1.5,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::NEG_Y,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            crouching: (24..72).contains(&scenario_frame),
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn dynamics_speed_scenario(name: &'static str, hard_stop: bool) -> Vec<PlannedFrame> {
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
                crouching: false,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn terrain_tap_stop_scenario(
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
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn terrain_tap_restart_scenario() -> Vec<PlannedFrame> {
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
                crouching: false,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn terrain_threshold_chatter_scenario() -> Vec<PlannedFrame> {
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
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn raised_guard_lateral_tap_stop_scenario(
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
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn airborne_landing_scenario() -> Vec<PlannedFrame> {
    (0..=96)
        .map(|scenario_frame| PlannedFrame {
            scenario: "airborne-landing",
            scenario_frame,
            speed: 0.0,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::ZERO,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn attack_step_scenario(
    name: &'static str,
    speed: f32,
    initial_direction: Vec2,
    lead_foot: LeadFoot,
    reverse_velocity: bool,
) -> Vec<PlannedFrame> {
    const START: usize = 8;
    (0..=47)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            // Deliberately reverse velocity and yaw after attack start in the
            // stress fixture. The replicated AttackStep must remain the one
            // selected on frame zero.
            local_direction: if (reverse_velocity || name.contains("high-speed"))
                && scenario_frame >= START + 1
            {
                -initial_direction
            } else {
                initial_direction
            },
            camera_yaw: if name.contains("yaw-only") && scenario_frame >= START + 1 {
                std::f32::consts::FRAC_PI_2
            } else {
                0.0
            },
            camera_pitch: 0.0,
            crouching: false,
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

fn capture_plan() -> Vec<PlannedFrame> {
    [
        steady_scenario("steady-walk-2.0", 2.0, 2.0),
        steady_scenario("walk-run-blend-3.75", 3.75, 2.0),
        steady_scenario("steady-run-5.5", 5.5, 2.0),
        crouch_steady_scenario(),
        steady_scenario_in_direction("lateral-walk-2.0", 2.0, 1.0, Vec2::X),
        steady_scenario_in_direction("reverse-walk-2.0", 2.0, 1.0, Vec2::Y),
        turning_scenario("gradual-camera-turn", false),
        turning_scenario("half-turn-reversal", true),
        guard_plant_turn_scenario(),
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
            "raised-guard-right-lead-left",
            2.0,
            1.0,
            Vec2::NEG_X,
            LeadFoot::Right,
        ),
        raised_guard_steady_scenario_with_lead(
            "raised-guard-right-lead-right",
            2.0,
            1.0,
            Vec2::X,
            LeadFoot::Right,
        ),
        raised_guard_steady_scenario_with_lead(
            "raised-guard-right-lead-forward-right",
            2.0,
            1.0,
            Vec2::new(1.0, -1.0),
            LeadFoot::Right,
        ),
        raised_guard_acceleration_scenario_with_lead(
            "raised-guard-right-lead-accelerate",
            LeadFoot::Right,
        ),
        raised_guard_release_scenario_with_lead("raised-guard-right-lead-release", LeadFoot::Right),
        raised_guard_reversal_scenario_with_lead(
            "raised-guard-right-lead-reversal",
            LeadFoot::Right,
        ),
        raised_guard_transition_scenario(),
        attack_step_scenario(
            "attack-step-forward-left-lead",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Left,
            false,
        ),
        attack_step_scenario(
            "attack-step-forward-right-lead",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Right,
            false,
        ),
        attack_step_scenario(
            "attack-step-backward-left-lead",
            2.0,
            Vec2::Y,
            LeadFoot::Left,
            false,
        ),
        attack_step_scenario(
            "attack-step-backward-right-lead",
            2.0,
            Vec2::Y,
            LeadFoot::Right,
            false,
        ),
        attack_step_scenario(
            "attack-step-stationary",
            0.0,
            Vec2::ZERO,
            LeadFoot::Left,
            false,
        ),
        attack_step_scenario(
            "attack-step-high-speed-reversal",
            5.5,
            Vec2::NEG_Y,
            LeadFoot::Left,
            false,
        ),
        attack_step_scenario(
            "attack-step-reversal",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Left,
            true,
        ),
        attack_step_scenario(
            "attack-step-yaw-only",
            2.0,
            Vec2::NEG_Y,
            LeadFoot::Left,
            false,
        ),
        attack_step_scenario(
            "attack-step-terrain-cross-slope",
            2.0,
            Vec2::new(0.5, -1.0).normalize(),
            LeadFoot::Left,
            false,
        ),
        crouch_transition_scenario(),
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
        terrain_crouch_scenario(),
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

fn turning_scenario(name: &'static str, reversal: bool) -> Vec<PlannedFrame> {
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
                crouching: false,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn dynamics_turn_scenario(name: &'static str, angle_radians: f32) -> Vec<PlannedFrame> {
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
                crouching: false,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn guard_plant_turn_scenario() -> Vec<PlannedFrame> {
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
                crouching: false,
                action: SkeletonAction::Block,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn raised_guard_scenario(name: &'static str, direction: Vec2) -> Vec<PlannedFrame> {
    let direction = direction.normalize_or_zero();
    raised_guard_steady_scenario(name, 2.0, 1.0, direction)
}

fn raised_guard_steady_scenario(
    name: &'static str,
    speed: f32,
    cycles: f32,
    direction: Vec2,
) -> Vec<PlannedFrame> {
    raised_guard_steady_scenario_with_lead(name, speed, cycles, direction, LeadFoot::Left)
}

fn raised_guard_steady_scenario_with_lead(
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
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

fn raised_guard_acceleration_scenario() -> Vec<PlannedFrame> {
    raised_guard_acceleration_scenario_with_lead(
        "raised-guard-accelerate-from-rest",
        LeadFoot::Left,
    )
}

fn raised_guard_acceleration_scenario_with_lead(
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
                crouching: false,
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Raised,
                lead_foot,
            }
        })
        .collect()
}

fn raised_guard_transition_scenario() -> Vec<PlannedFrame> {
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
            crouching: false,
            weapon_guard: if scenario_frame < 16 {
                WeaponGuardState::Lowered
            } else {
                WeaponGuardState::Raised
            },
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn raised_guard_release_scenario() -> Vec<PlannedFrame> {
    raised_guard_release_scenario_with_lead("raised-guard-release-at-peak", LeadFoot::Left)
}

fn raised_guard_release_scenario_with_lead(
    name: &'static str,
    lead_foot: LeadFoot,
) -> Vec<PlannedFrame> {
    (0..=64)
        .map(|scenario_frame| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed: if scenario_frame <= 20 { 2.0 } else { 0.0 },
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::NEG_Y,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

fn raised_guard_reversal_scenario() -> Vec<PlannedFrame> {
    raised_guard_reversal_scenario_with_lead("raised-guard-left-right-reversal", LeadFoot::Left)
}

fn raised_guard_reversal_scenario_with_lead(
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
            crouching: false,
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

fn setup_viewer(mut commands: Commands) {
    let mut generator = TerrainGenerator::new(0xA11C_E5E1);
    generator.period = 200.0;
    let terrain = generator.generate(100, 30, 100);
    let spawn_height =
        terrain.height_at(Vec2::ZERO).unwrap_or_default() + CAPTURE_ROOT_GROUND_OFFSET_METRES;
    commands.spawn((
        Name::new("Animation review hills scene"),
        SceneId("hills".to_owned()),
        terrain,
        Transform::default(),
    ));

    commands.spawn((
        Name::new("Animation review subject"),
        CaptureSubject,
        Player {
            name: "Animation review".into(),
        },
        CharacterId(0),
        CharacterLook::default(),
        SkeletonState::default(),
        Transform::from_xyz(0.0, spawn_height, 0.0),
        Collider::cylinder(0.4, 1.9),
        CollisionMargin(0.01),
        tactical_character_controller(),
    ));
    commands.spawn((
        CaptureLabel,
        Text::new("Loading authored animation rig..."),
        TextFont::from_font_size(22.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
}

fn drive_sequence(
    mut sequence: ResMut<CaptureSequence>,
    mut procedural_clock: ResMut<ProceduralAnimationClock>,
    mut terrain_ik: ResMut<TerrainIkEnabled>,
    mut guard_input: ResMut<WeaponGuardInputState>,
    terrain: Single<&SceneTerrain>,
    mut subjects: Query<
        (
            &mut SkeletonState,
            &mut Transform,
            &mut CharacterLook,
            Option<&AnimationPlayback>,
            Option<&mut LegIkState>,
            Option<&mut ArmIkState>,
            Option<&mut RaisedFootworkState>,
            Option<&mut AttackFootworkState>,
            Option<&mut LocomotionHeightState>,
            Option<&mut LocomotionBodyResponseState>,
        ),
        With<CaptureSubject>,
    >,
    mut labels: Query<&mut Text, With<CaptureLabel>>,
) {
    if sequence.applied || sequence.capture_in_flight || sequence.index >= sequence.plan.len() {
        return;
    }
    let frame = sequence.plan[sequence.index].clone();
    let metadata = scenario_metadata(frame.scenario);
    let mut gait_phase = 0.0;
    for (
        mut skeleton,
        mut transform,
        mut look,
        playback,
        ik_state,
        arm_ik_state,
        raised_footwork,
        attack_footwork,
        height_state,
        body_response,
    ) in &mut subjects
    {
        let Some(playback) = playback else {
            return;
        };
        if !playback.authored_pose_is_ready() {
            return;
        }

        if sequence.active_scenario != Some(frame.scenario) {
            sequence.active_scenario = Some(frame.scenario);
            sequence.simulation_tick = 0;
            sequence.scenario_distance = 0.0;
            *skeleton = SkeletonState::default();
            *guard_input = WeaponGuardInputState::default();
            if let Some(mut ik_state) = ik_state {
                *ik_state = LegIkState::default();
            }
            if let Some(mut arm_ik_state) = arm_ik_state {
                *arm_ik_state = ArmIkState::default();
            }
            if let Some(mut raised_footwork) = raised_footwork {
                *raised_footwork = RaisedFootworkState::default();
            }
            if let Some(mut attack_footwork) = attack_footwork {
                *attack_footwork = AttackFootworkState::default();
            }
            if let Some(mut height_state) = height_state {
                *height_state = LocomotionHeightState::default();
            }
            if let Some(mut body_response) = body_response {
                *body_response = LocomotionBodyResponseState::default();
            }
            let ground = terrain.height_at(Vec2::ZERO).unwrap_or_default();
            transform.translation = Vec3::new(0.0, ground + CAPTURE_ROOT_GROUND_OFFSET_METRES, 0.0);
            transform.rotation = Quat::from_rotation_y(std::f32::consts::PI);
        }

        let orientation =
            Quat::from_euler(EulerRot::YXZ, frame.camera_yaw, frame.camera_pitch, 0.0);
        look.yaw = frame.camera_yaw;
        look.pitch = frame.camera_pitch;
        let wheel = match frame.weapon_guard {
            WeaponGuardState::Raised => 1.0,
            WeaponGuardState::Lowered => -1.0,
        };
        let attack_start_frame = frame.scenario.starts_with("attack-step-").then_some(8);
        if frame.action != SkeletonAction::Attack
            || attack_start_frame == Some(frame.scenario_frame)
        {
            skeleton.lead_foot = frame.lead_foot;
        }
        terrain_ik.0 = terrain_ik_enabled_for_frame(&frame);
        guard_input.apply_controls(wheel, false);
        set_weapon_guard(&mut skeleton, guard_input.desired);
        let grounded = metadata.kind != ScenarioKind::Landing || frame.scenario_frame >= 32;
        let vertical_velocity = if metadata.kind == ScenarioKind::Landing && !grounded {
            -4.5
        } else {
            0.0
        };
        let requested_local_velocity = Vec3::new(
            frame.local_direction.x * frame.speed,
            vertical_velocity,
            frame.local_direction.y * frame.speed,
        );
        let local_velocity = skeleton
            .attack_movement()
            .map(|(direction, speed)| {
                if skeleton.action_phase() < 0.5 {
                    Vec3::new(direction.x * speed, vertical_velocity, direction.y * speed)
                } else {
                    Vec3::new(0.0, vertical_velocity, 0.0)
                }
            })
            .unwrap_or(requested_local_velocity);
        let world_velocity = controller_yaw(orientation) * local_velocity;
        let delta_seconds = if frame.scenario_frame == 0 {
            0.0
        } else {
            sequence.simulation_tick += 1;
            1.0 / SAMPLE_HZ
        };
        procedural_clock.set_fixed_tick(sequence.simulation_tick, delta_seconds);
        let horizontal = transform.translation.xz() + world_velocity.xz() * delta_seconds;
        let vertical = if terrain_ik.0 {
            terrain.height_at(horizontal).unwrap_or_default() + CAPTURE_ROOT_GROUND_OFFSET_METRES
        } else {
            transform.translation.y
        };
        transform.translation = Vec3::new(horizontal.x, vertical, horizontal.y);
        if frame.action != SkeletonAction::None
            && (frame.action != SkeletonAction::Attack
                || attack_start_frame == Some(frame.scenario_frame))
        {
            let start = sequence.simulation_tick;
            let contact = start
                + if frame.action == SkeletonAction::Attack {
                    19
                } else {
                    64
                };
            match frame.action {
                SkeletonAction::Attack => skeleton.begin_attack(
                    AttackSpec::melee_from_local_velocity(local_velocity),
                    start,
                    contact,
                ),
                SkeletonAction::Dodge => skeleton.begin_dodge(DodgeSpec::default(), start, contact),
                SkeletonAction::Block => skeleton.begin_block(BlockSpec::default(), start, contact),
                SkeletonAction::None => {}
            }
        }
        transform.rotation = advance_body_facing(
            transform.rotation,
            orientation,
            world_velocity,
            frame.action,
            skeleton.weapon_guard(),
            delta_seconds,
        );
        sequence.scenario_distance += frame.speed * delta_seconds;
        project_skeleton_locomotion(
            &mut skeleton,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: world_velocity,
                grounded,
                crouching: frame.crouching,
                delta_seconds,
                tick: sequence.simulation_tick,
            },
        );
        gait_phase = skeleton.gait_phase;
    }
    for mut label in &mut labels {
        **label = format!(
            "{} | {:>4.2} m/s | phase {:>5.3} | {} view | 64 Hz frame {}",
            frame.scenario,
            frame.speed,
            gait_phase,
            VIEWS[sequence.view_index].slug(),
            frame.scenario_frame,
        );
    }
    sequence.applied = true;
    sequence.settled = 0;
}

fn position_capture_camera(
    sequence: Res<CaptureSequence>,
    subjects: Query<(&Transform, &PresentedSkeleton), With<CaptureSubject>>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<CaptureSubject>)>,
    mut labels: Query<(&mut Text, &mut Visibility), With<CaptureLabel>>,
) {
    let (Ok((subject, skeleton)), Ok(mut camera)) = (subjects.single(), cameras.single_mut())
    else {
        return;
    };
    let focus = subject.translation + Vec3::Y * 0.95;
    let view = VIEWS[sequence.view_index.min(VIEWS.len() - 1)];
    match view {
        CaptureView::Gameplay => {
            // Physics simulation is disabled in this fixture, so ahoy does
            // not refresh its controller-follow base transform. Reconstruct
            // that default base and apply the exact gameplay camera offset;
            // otherwise the offset accumulates and the first raw frame is a
            // pelvis-level/empty view.
            camera.translation = subject.translation + third_person_offset(Quat::IDENTITY);
            camera.rotation = Quat::IDENTITY;
        }
        CaptureView::Side => {
            camera.translation = focus + Vec3::new(5.0, 0.45, 0.0);
            camera.look_at(focus, Vec3::Y);
        }
        CaptureView::Front => {
            camera.translation = focus + Vec3::new(0.0, 0.45, -5.0);
            camera.look_at(focus, Vec3::Y);
        }
    }
    if sequence.applied
        && let Some(frame) = sequence.plan.get(sequence.index)
    {
        for (mut label, mut visibility) in &mut labels {
            *visibility = if matches!(view, CaptureView::Gameplay) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            **label = format!(
                "{} | {:>4.2} m/s | phase {:>5.3} | {} view | 64 Hz frame {}",
                frame.scenario,
                frame.speed,
                skeleton.gait_phase,
                view.slug(),
                frame.scenario_frame,
            );
        }
    }
}

fn draw_skeleton_overlay(
    sequence: Res<CaptureSequence>,
    mut gizmos: Gizmos,
    subjects: Query<(Entity, &PresentedSkeleton), With<CaptureSubject>>,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
) {
    if matches!(
        VIEWS[sequence.view_index.min(VIEWS.len() - 1)],
        CaptureView::Gameplay
    ) {
        return;
    }
    let Ok((subject, skeleton)) = subjects.single() else {
        return;
    };
    let positions = bones
        .iter()
        .filter(|(bone, _)| bone.owner == subject)
        .map(|(bone, transform)| (bone.role, transform.translation()))
        .collect::<BTreeMap<_, _>>();
    let connections = [
        (BoneRole::Pelvis, BoneRole::Chest),
        (BoneRole::Chest, BoneRole::Head),
        (BoneRole::Chest, BoneRole::UpperArmLeft),
        (BoneRole::UpperArmLeft, BoneRole::ForearmLeft),
        (BoneRole::ForearmLeft, BoneRole::HandLeft),
        (BoneRole::Chest, BoneRole::UpperArmRight),
        (BoneRole::UpperArmRight, BoneRole::ForearmRight),
        (BoneRole::ForearmRight, BoneRole::HandRight),
        (BoneRole::Pelvis, BoneRole::ThighLeft),
        (BoneRole::ThighLeft, BoneRole::ShinLeft),
        (BoneRole::ShinLeft, BoneRole::FootLeft),
        (BoneRole::Pelvis, BoneRole::ThighRight),
        (BoneRole::ThighRight, BoneRole::ShinRight),
        (BoneRole::ShinRight, BoneRole::FootRight),
    ];
    for (start, end) in connections {
        if let (Some(&start), Some(&end)) = (positions.get(&start), positions.get(&end)) {
            gizmos.line(start, end, Color::srgba(0.1, 0.9, 1.0, 0.8));
        }
    }
    let (left_support, right_support) = locomotion_support_weights(skeleton);
    for (role, support) in [
        (BoneRole::FootLeft, left_support),
        (BoneRole::FootRight, right_support),
    ] {
        let Some(&position) = positions.get(&role) else {
            continue;
        };
        let color = if support >= 0.55 {
            Color::srgb(1.0, 0.8, 0.05)
        } else {
            Color::srgb(1.0, 0.1, 0.7)
        };
        gizmos.line(position - Vec3::X * 0.09, position + Vec3::X * 0.09, color);
        gizmos.line(position - Vec3::Z * 0.09, position + Vec3::Z * 0.09, color);
        gizmos.line(
            position,
            position + Vec3::Y * (0.05 + support * 0.15),
            color,
        );
    }
}

fn collect_locomotion_presentation_events(
    mut events: MessageReader<LocomotionPresentationEvent>,
    mut sequence: ResMut<CaptureSequence>,
) {
    if sequence.index >= sequence.plan.len() {
        return;
    }
    let scenario = sequence.plan[sequence.index].scenario.to_owned();
    let scenario_frame = sequence.plan[sequence.index].scenario_frame;
    sequence
        .presentation_events
        .extend(events.read().map(move |event| PresentationEventSample {
            scenario: scenario.clone(),
            scenario_frame,
            owner: format!("{:?}", event.owner),
            sequence: event.sequence,
            sample_tick: event.sample_tick,
            kind: match event.kind {
                LocomotionPresentationEventKind::Contact(foot) => {
                    format!("contact_{foot:?}").to_lowercase()
                }
                LocomotionPresentationEventKind::Landing => "landing".to_owned(),
            },
        }));
}

fn capture_frame(
    mut commands: Commands,
    mut sequence: ResMut<CaptureSequence>,
    terrain_ik: Res<TerrainIkEnabled>,
    subjects: Query<
        (
            Entity,
            &PresentedSkeleton,
            &GlobalTransform,
            Option<&AnimationPlayback>,
            Option<&RaisedFootworkState>,
            Option<&AttackFootworkState>,
            Option<&LocomotionBodyResponseState>,
            Option<&LocomotionHeightState>,
            Option<&LegIkState>,
        ),
        With<CaptureSubject>,
    >,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
    terrain: Single<&SceneTerrain>,
    mut exit: MessageWriter<AppExit>,
) {
    if !sequence.applied || sequence.capture_in_flight {
        return;
    }
    let Ok((
        subject,
        skeleton,
        subject_global,
        playback,
        raised_footwork,
        attack_footwork,
        body_response,
        height_state,
        leg_ik,
    )) = subjects.single()
    else {
        wait_or_fail(&mut sequence, "capture subject is missing", &mut exit);
        return;
    };
    let Some(playback) = playback else {
        wait_or_fail(
            &mut sequence,
            "capture subject has no AnimationPlayback",
            &mut exit,
        );
        return;
    };
    if !playback.authored_pose_is_ready() {
        wait_or_fail(
            &mut sequence,
            "authored locomotion clip has not resolved",
            &mut exit,
        );
        return;
    }
    sequence.waiting = 0;
    sequence.settled += 1;
    let required = if sequence.index == 0 {
        sequence.settle_frames.max(60)
    } else {
        // A view change needs one complete render before requesting the next
        // asynchronous screenshot; otherwise two paths may receive the same
        // previously rendered camera image.
        sequence.settle_frames.max(2)
    };
    if sequence.settled < required {
        return;
    }

    let frame = sequence.plan[sequence.index].clone();
    let view = VIEWS[sequence.view_index];
    let file_name = format!(
        "{}-{:04}-{}.png",
        frame.scenario,
        frame.scenario_frame,
        view.slug()
    );
    let path = sequence.output.join(&file_name);
    let evaluation_bones = collect_bones(
        subject,
        &bones,
        &terrain,
        (!terrain_ik.0)
            .then_some(subject_global.translation().y - CAPTURE_ROOT_GROUND_OFFSET_METRES),
    );
    if sequence.view_index == 0 {
        sequence.repeated_evaluation_baseline = Some(RepeatedEvaluationSnapshot {
            scenario: frame.scenario,
            scenario_frame: frame.scenario_frame,
            bones: evaluation_bones.clone(),
            contact_sequence: skeleton.contact_sequence,
            landing_sequence: skeleton.landing_sequence,
            event_count: sequence.presentation_events.len(),
        });
    } else if let Some(baseline) = &sequence.repeated_evaluation_baseline {
        let bone_mismatch = baseline.bones.iter().find_map(|(name, expected)| {
            let actual = evaluation_bones.get(name)?;
            let position_delta =
                Vec3::from_array(expected.position).distance(Vec3::from_array(actual.position));
            let rotation_delta = Quat::from_array(expected.rotation_xyzw)
                .angle_between(Quat::from_array(actual.rotation_xyzw))
                .to_degrees();
            // Global transforms settle once between render views. Millimetre-
            // scale propagation convergence is visually inert; larger motion
            // still catches a temporal solver advancing twice for one sample.
            (position_delta > 0.005 || rotation_delta > 0.25).then_some((
                name,
                position_delta,
                rotation_delta,
            ))
        });
        let bones_match = bone_mismatch.is_none();
        let repeated_evaluation_matches = baseline.scenario == frame.scenario
            && baseline.scenario_frame == frame.scenario_frame
            && bones_match
            && baseline.contact_sequence == skeleton.contact_sequence
            && baseline.landing_sequence == skeleton.landing_sequence
            && baseline.event_count == sequence.presentation_events.len();
        if !repeated_evaluation_matches
            && let Some((bone, position_delta, rotation_delta)) = bone_mismatch
        {
            warn!(
                scenario = frame.scenario,
                scenario_frame = frame.scenario_frame,
                bone,
                position_delta,
                rotation_delta,
                "repeated animation evaluation changed a captured bone"
            );
        }
        if !repeated_evaluation_matches && bone_mismatch.is_none() {
            warn!(
                scenario = frame.scenario,
                scenario_frame = frame.scenario_frame,
                baseline_contact = baseline.contact_sequence,
                contact = skeleton.contact_sequence,
                baseline_landing = baseline.landing_sequence,
                landing = skeleton.landing_sequence,
                baseline_events = baseline.event_count,
                events = sequence.presentation_events.len(),
                "repeated animation evaluation changed non-bone state"
            );
        }
        sequence.repeated_evaluation_valid &= repeated_evaluation_matches;
    }
    if sequence.view_index == 0 {
        let cadence_support = locomotion_support_weights(skeleton);
        let root_distance_metres = sequence.scenario_distance;
        let (desired_left_foot_target, desired_right_foot_target) = attack_footwork
            .filter(|state| state.initialized)
            .map(|state| (state.left_solve_target, state.right_solve_target))
            .or_else(|| {
                raised_footwork.map(|state| (state.left_solve_target, state.right_solve_target))
            })
            .unwrap_or_default();
        let leg_ik = leg_ik.map(LegIkState::diagnostics).unwrap_or_default();
        let ik_support =
            if leg_ik.left_solve_target.is_some() || leg_ik.right_solve_target.is_some() {
                (leg_ik.left_support_weight, leg_ik.right_support_weight)
            } else {
                cadence_support
            };
        let (left_support_weight, right_support_weight) = attack_footwork
            .filter(|state| state.initialized)
            .map(|state| (state.left_support_weight, state.right_support_weight))
            .unwrap_or(ik_support);
        let (
            attack_requested_left,
            attack_requested_right,
            attack_constrained_left,
            attack_constrained_right,
            attack_support_handoffs,
            attack_maximum_reach_yield,
        ) = attack_footwork.filter(|state| state.initialized).map_or(
            (None, None, None, None, 0, 0.0),
            |state| {
                (
                    state.left_requested_target,
                    state.right_requested_target,
                    state.left_solve_target,
                    state.right_solve_target,
                    state.support_handoffs,
                    state.maximum_reach_yield,
                )
            },
        );
        sequence.samples.push(FrameSample {
            scenario: frame.scenario.to_owned(),
            scenario_frame: frame.scenario_frame,
            time_seconds: frame.time_seconds,
            speed_metres_per_second: frame.speed,
            gait_phase: skeleton.gait_phase,
            locomotion_sample_tick: skeleton.locomotion_sample_tick,
            body_acceleration: (subject_global.rotation().inverse() * skeleton.world_acceleration)
                .to_array(),
            world_acceleration: skeleton.world_acceleration.to_array(),
            contact_sequence: skeleton.contact_sequence,
            contact_foot: skeleton.contact_foot,
            landing_sequence: skeleton.landing_sequence,
            landing_impact_speed: skeleton.landing_impact_speed,
            body_lean_pitch_degrees: body_response
                .map_or(0.0, |state| state.pitch_radians.to_degrees()),
            body_lean_roll_degrees: body_response
                .map_or(0.0, |state| state.roll_radians.to_degrees()),
            landing_compression_metres: height_state.map_or(0.0, |state| state.landing_compression),
            root_distance_metres,
            root_position_metres: subject_global.translation().to_array(),
            world_travel_direction: (controller_yaw(Quat::from_rotation_y(frame.camera_yaw))
                * Vec3::new(frame.local_direction.x, 0.0, frame.local_direction.y))
            .normalize_or_zero()
            .to_array(),
            desired_body_forward_direction: if frame.weapon_guard == WeaponGuardState::Raised
                || matches!(frame.action, SkeletonAction::Attack | SkeletonAction::Block)
            {
                (controller_yaw(Quat::from_rotation_y(frame.camera_yaw)) * Vec3::NEG_Z).to_array()
            } else {
                (controller_yaw(Quat::from_rotation_y(frame.camera_yaw))
                    * Vec3::new(frame.local_direction.x, 0.0, frame.local_direction.y))
                .normalize_or_zero()
                .to_array()
            },
            body_forward_direction: (subject_global.rotation() * Vec3::Z).to_array(),
            body_rotation_xyzw: subject_global.rotation().to_array(),
            weapon_guard: frame.weapon_guard,
            lead_foot: skeleton.lead_foot,
            action: skeleton.action_kind(),
            action_phase: skeleton.action_phase(),
            attack_step: skeleton.attack_step(),
            guard_action: frame.weapon_guard == WeaponGuardState::Raised
                || matches!(frame.action, SkeletonAction::Attack | SkeletonAction::Block),
            left_support_weight,
            right_support_weight,
            desired_left_foot_target: desired_left_foot_target.map(|value| value.to_array()),
            desired_right_foot_target: desired_right_foot_target.map(|value| value.to_array()),
            ik_left_authored_target: leg_ik.left_authored_target.map(|value| value.to_array()),
            ik_right_authored_target: leg_ik.right_authored_target.map(|value| value.to_array()),
            ik_left_planned_contact: leg_ik.left_planned_contact.map(|value| value.to_array()),
            ik_right_planned_contact: leg_ik.right_planned_contact.map(|value| value.to_array()),
            ik_settle_capture_point: leg_ik.settle_capture_point.map(|value| value.to_array()),
            ik_left_solve_target: leg_ik.left_solve_target.map(|value| value.to_array()),
            ik_right_solve_target: leg_ik.right_solve_target.map(|value| value.to_array()),
            ik_left_support_weight: leg_ik.left_support_weight,
            ik_right_support_weight: leg_ik.right_support_weight,
            ik_left_release_active: leg_ik.left_release_active,
            ik_right_release_active: leg_ik.right_release_active,
            ik_left_release_target: leg_ik.left_release_target.map(|value| value.to_array()),
            ik_right_release_target: leg_ik.right_release_target.map(|value| value.to_array()),
            ik_settle_progress: leg_ik.settle_progress,
            attack_requested_left_foot_target: attack_requested_left.map(|value| value.to_array()),
            attack_requested_right_foot_target: attack_requested_right
                .map(|value| value.to_array()),
            attack_constrained_left_foot_target: attack_constrained_left
                .map(|value| value.to_array()),
            attack_constrained_right_foot_target: attack_constrained_right
                .map(|value| value.to_array()),
            attack_support_handoffs,
            attack_maximum_reach_yield_metres: attack_maximum_reach_yield,
            screenshots: VIEWS
                .into_iter()
                .map(|view| {
                    (
                        view.slug().to_owned(),
                        format!(
                            "{}-{:04}-{}.png",
                            frame.scenario,
                            frame.scenario_frame,
                            view.slug()
                        ),
                    )
                })
                .collect(),
            bones: evaluation_bones,
        });
    }
    sequence.capture_in_flight = true;
    let frame_key = format!("{}:{}", frame.scenario, frame.scenario_frame);
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              mut sequence: ResMut<CaptureSequence>,
              mut exit: MessageWriter<AppExit>| {
            sequence
                .view_fingerprints
                .push(visual_fingerprint(&captured.image));
            save_to_disk(&path)(captured);
            sequence.capture_in_flight = false;
            sequence.view_index += 1;
            sequence.settled = 0;
            if sequence.view_index < VIEWS.len() {
                return;
            }
            if sequence
                .view_fingerprints
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != VIEWS.len()
            {
                sequence.duplicate_view_frames.push(frame_key.clone());
            }
            sequence.view_fingerprints.clear();
            sequence.view_index = 0;
            sequence.index += 1;
            sequence.applied = false;
            if sequence.index == sequence.plan.len() {
                finish_capture(&mut sequence, &mut exit);
            }
        },
    );
}

fn visual_fingerprint(image: &Image) -> u64 {
    let Some(data) = image.data.as_deref() else {
        return 0;
    };
    let width = image.texture_descriptor.size.width as usize;
    let height = image.texture_descriptor.size.height as usize;
    let stride = width.saturating_mul(4);
    if stride == 0 || data.len() < stride.saturating_mul(height) {
        return 0;
    }
    // Ignore the top UI label and hash a regular sample of the rendered 3D
    // view. This catches accidentally identical camera outputs cheaply.
    let mut hash = 0xcbf29ce484222325_u64;
    for y in (96.min(height)..height).step_by(8) {
        for x in (0..width).step_by(8) {
            for channel in 0..3 {
                hash ^= data[y * stride + x * 4 + channel] as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

fn collect_bones(
    subject: Entity,
    bones: &Query<(&HumanoidBone, &GlobalTransform)>,
    terrain: &SceneTerrain,
    flat_ground_height: Option<f32>,
) -> BTreeMap<String, BoneSample> {
    bones
        .iter()
        .filter(|(bone, _)| bone.owner == subject && tracked_bone(bone.role).is_some())
        .map(|(bone, transform)| {
            let rotation = transform.rotation();
            (
                tracked_bone(bone.role).unwrap().to_owned(),
                BoneSample {
                    position: transform.translation().to_array(),
                    rotation_xyzw: [rotation.x, rotation.y, rotation.z, rotation.w],
                    terrain_clearance_metres: flat_ground_height
                        .or_else(|| terrain.height_at(transform.translation().xz()))
                        .map(|height| transform.translation().y - height),
                },
            )
        })
        .collect()
}

fn tracked_bone(role: BoneRole) -> Option<&'static str> {
    Some(match role {
        BoneRole::Pelvis => "pelvis",
        BoneRole::Chest => "chest",
        BoneRole::Head => "head",
        BoneRole::UpperArmLeft => "left_shoulder",
        BoneRole::UpperArmRight => "right_shoulder",
        BoneRole::ForearmLeft => "left_elbow",
        BoneRole::ForearmRight => "right_elbow",
        BoneRole::HandLeft => "left_hand",
        BoneRole::HandRight => "right_hand",
        BoneRole::ThighLeft => "left_hip",
        BoneRole::ThighRight => "right_hip",
        BoneRole::ShinLeft => "left_knee",
        BoneRole::ShinRight => "right_knee",
        BoneRole::FootLeft => "left_foot",
        BoneRole::FootRight => "right_foot",
        _ => return None,
    })
}

fn wait_or_fail(sequence: &mut CaptureSequence, reason: &str, exit: &mut MessageWriter<AppExit>) {
    sequence.waiting += 1;
    if sequence.waiting < 1200 {
        return;
    }
    let message = format!(
        "animation viewer timed out after {} rendered frames: {reason}\n",
        sequence.waiting
    );
    let path = sequence.output.join("failure.txt");
    fs::write(&path, &message).unwrap_or_else(|error| panic!("failed to write {path:?}: {error}"));
    error!(%reason, path = ?path, "Animation capture failed");
    exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
}

fn finish_capture(sequence: &mut CaptureSequence, exit: &mut MessageWriter<AppExit>) {
    let frames = std::mem::take(&mut sequence.samples);
    let presentation_events = std::mem::take(&mut sequence.presentation_events);
    let scenarios = scenario_metrics(&frames);
    let finite_transforms = frames.iter().all(|frame| {
        frame.bones.values().all(|bone| {
            bone.position.into_iter().all(f32::is_finite)
                && bone.rotation_xyzw.into_iter().all(f32::is_finite)
        })
    });
    let all_scenarios_complete = frames.len() == sequence.plan.len()
        && frames.iter().zip(&sequence.plan).all(|(frame, planned)| {
            frame.scenario == planned.scenario
                && frame.scenario_frame == planned.scenario_frame
                && TRACKED_BONE_NAMES
                    .iter()
                    .all(|name| frame.bones.contains_key(*name))
        });
    let all_artifacts_written = capture_artifacts_written(&sequence.output, &frames);
    let continuity_within_review_bounds = scenarios.iter().all(|metrics| {
        metrics.maximum_root_relative_step_metres
            <= if metrics.scenario.starts_with("attack-step-") {
                0.30
            } else {
                0.20
            }
            && metrics.maximum_foot_root_relative_step_metres
                <= if metrics.scenario.starts_with("attack-step-") {
                    0.21
                } else {
                    0.055
                }
            && metrics.maximum_knee_root_relative_step_metres
                <= if metrics.scenario.starts_with("attack-step-") {
                    0.15
                } else {
                    0.10
                }
            && metrics.maximum_bone_rotation_step_degrees <= 60.0
            && (!metrics.scenario.starts_with("raised-guard")
                || metrics.maximum_pelvis_vertical_step_metres
                    <= RAISED_MAXIMUM_PELVIS_VERTICAL_STEP_METRES)
    });
    let no_ground_penetration = scenarios.iter().all(|metrics| {
        if scenario_metadata(&metrics.scenario).kind == ScenarioKind::Attack {
            // Gait-phase contact selection does not describe a one-shot attack
            // handoff. Its dedicated validator checks the actual requested
            // plant; retain only the raw ankle penetration guard here.
            metrics.minimum_foot_clearance_metres >= -0.04
        } else if scenario_metadata(&metrics.scenario).kind == ScenarioKind::Terrain
            && metrics.scenario != "terrain-toggle-mid-stride"
        {
            // Only a contact foot is ground-constrained. Gate those contacts;
            // an airborne authored foot is intentionally not projected onto
            // every terrain feature underneath its swing path.
            metrics.minimum_contact_sole_clearance_metres >= -0.01
        } else if metrics.scenario.starts_with("raised-guard-tap-stop") {
            metrics.minimum_contact_sole_clearance_metres >= -0.04
        } else {
            metrics.minimum_contact_sole_clearance_metres >= -0.02
        }
    });
    let raised_guard_fixed_lead = frames.windows(2).all(|pair| {
        pair[0].scenario != pair[1].scenario
            || pair[0].weapon_guard != WeaponGuardState::Raised
            || pair[1].weapon_guard != WeaponGuardState::Raised
            || pair[0].action == SkeletonAction::Attack
            || pair[1].action == SkeletonAction::Attack
            || pair[0].lead_foot == pair[1].lead_foot
    });
    let flat_controller_height_stable = scenarios.iter().all(|metrics| {
        scenario_uses_terrain_ik(&metrics.scenario)
            || metrics.scenario.contains("terrain")
            || metrics.controller_vertical_range_metres <= 0.0001
    });
    let phase_owned_height_valid = scenarios.iter().all(|metrics| {
        if matches!(
            metrics.scenario.as_str(),
            "start-stop-transition" | "raised-guard-transition" | "crouch-enter-exit"
        ) && metrics.maximum_pelvis_vertical_step_metres
            > LOCOMOTION_STATE_MAXIMUM_PELVIS_VERTICAL_STEP_METRES
        {
            return false;
        }
        let Some((minimum, maximum, expected_peaks)) = expected_visual_height(&metrics.scenario)
        else {
            return true;
        };
        metrics.phase_height_range_metres >= minimum
            && metrics.phase_height_range_metres <= maximum
            && metrics.contact_to_passing_height_gain_metres >= minimum * 0.75
            && metrics.visual_height_peak_count == expected_peaks
            && metrics.visual_height_peaks_in_passing_windows
    });
    let run_flight_valid = scenarios.iter().all(|metrics| {
        if metrics.scenario == "steady-run-5.5" {
            (0.085..=0.12).contains(&metrics.maximum_no_support_seconds)
                && (0.10..=0.30).contains(&metrics.minimum_flight_sole_clearance_metres)
        } else if metrics.scenario == "steady-walk-2.0"
            || raised_scenario_requires_zero_flight(&metrics.scenario)
        {
            metrics.maximum_no_support_seconds <= f32::EPSILON
        } else {
            true
        }
    });
    let contact_sequences_valid =
        frames.windows(2).all(|pair| {
            if pair[0].scenario != pair[1].scenario {
                return true;
            }
            let delta = pair[1]
                .contact_sequence
                .wrapping_sub(pair[0].contact_sequence);
            delta <= 1
                && (delta == 0
                    || pair[1].contact_foot != pair[0].contact_foot
                    || pair[0].scenario.starts_with("raised-guard-tap-stop"))
                && !(pair[0].speed_metres_per_second <= 0.05
                    && pair[1].speed_metres_per_second <= 0.05
                    && !pair[0].scenario.starts_with("raised-guard-tap-stop")
                    && delta != 0)
        }) && ["raised-guard-tap-stop-left", "raised-guard-tap-stop-right"]
            .iter()
            .all(|scenario| {
                frames
                    .windows(2)
                    .filter(|pair| pair[0].scenario == *scenario && pair[1].scenario == *scenario)
                    .filter(|pair| pair[1].contact_sequence != pair[0].contact_sequence)
                    .count()
                    <= 1
            });
    let cadence_frames = frames
        .iter()
        .filter(|frame| frame.scenario == "cadence-contact")
        .collect::<Vec<_>>();
    let cadence_contacts = cadence_frames
        .windows(2)
        .filter_map(|pair| {
            (pair[1].contact_sequence == pair[0].contact_sequence + 1).then_some(pair[1])
        })
        .collect::<Vec<_>>();
    let cadence_step_distance = ordinary_step_distance(2.0);
    let adjusted_contact_distances = cadence_contacts
        .iter()
        .map(|frame| {
            let contact_phase = match frame.contact_foot {
                LeadFoot::Left => 0.0,
                LeadFoot::Right => 0.5,
            };
            let phase_since_contact = (frame.gait_phase - contact_phase).rem_euclid(1.0);
            frame.root_distance_metres - phase_since_contact * cadence_step_distance * 2.0
        })
        .collect::<Vec<_>>();
    let cadence_tolerance = (cadence_step_distance * 0.01).max(0.005);
    let cadence_contact_valid = cadence_frames.is_empty()
        || (cadence_contacts.len() == 4
            && cadence_contacts.windows(2).all(|pair| {
                pair[1].contact_sequence == pair[0].contact_sequence + 1
                    && pair[1].contact_foot != pair[0].contact_foot
            })
            && adjusted_contact_distances.windows(2).all(|pair| {
                ((pair[1] - pair[0]) - cadence_step_distance).abs() <= cadence_tolerance
            })
            && adjusted_contact_distances.windows(3).all(|window| {
                ((window[2] - window[0]) - cadence_step_distance * 2.0).abs() <= cadence_tolerance
            }));
    let event_stream_valid = presentation_events.windows(2).all(|pair| {
        let same_stream = (pair[0].kind.starts_with("contact")
            && pair[1].kind.starts_with("contact"))
            || (pair[0].kind == "landing" && pair[1].kind == "landing");
        pair[0].scenario != pair[1].scenario
            || !same_stream
            || (pair[1].sequence > pair[0].sequence && pair[1].sample_tick >= pair[0].sample_tick)
    }) && presentation_events
        .iter()
        .enumerate()
        .all(|(index, event)| {
            !presentation_events[..index].iter().any(|previous| {
                previous.owner == event.owner
                    && previous.scenario == event.scenario
                    && previous.kind == event.kind
                    && previous.sequence == event.sequence
            })
        })
        && (cadence_frames.is_empty()
            || presentation_events
                .iter()
                .filter(|event| {
                    event.scenario == "cadence-contact" && event.kind.starts_with("contact")
                })
                .count()
                == 4)
        && (!frames
            .iter()
            .any(|frame| frame.scenario == "airborne-landing")
            || presentation_events
                .iter()
                .filter(|event| event.scenario == "airborne-landing" && event.kind == "landing")
                .count()
                == 1);
    let speed_ramp_phase_continuity_valid = frames.windows(2).all(|pair| {
        if pair[0].scenario != "speed-ramp-up-down" || pair[1].scenario != "speed-ramp-up-down" {
            return true;
        }
        let phase_delta = pair[1].gait_phase - pair[0].gait_phase;
        phase_delta >= -0.001
            || (phase_delta < -0.5
                && pair[1]
                    .contact_sequence
                    .wrapping_sub(pair[0].contact_sequence)
                    == 1)
    });
    let lean_range = |scenario: &str, select: fn(&FrameSample) -> f32| {
        frames
            .iter()
            .filter(|frame| frame.scenario == scenario)
            .map(select)
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            )
    };
    let (ramp_pitch_minimum, ramp_pitch_maximum) =
        lean_range("speed-ramp-up-down", |frame| frame.body_lean_pitch_degrees);
    let (_, hard_stop_pitch_maximum) =
        lean_range("hard-stop", |frame| frame.body_lean_pitch_degrees);
    let turn_90_roll = lean_range("dynamics-turn-90", |frame| {
        frame.body_lean_roll_degrees.abs()
    });
    let turn_180_roll = lean_range("dynamics-turn-180", |frame| {
        frame.body_lean_roll_degrees.abs()
    });
    let lean_step_valid = frames.windows(2).all(|pair| {
        pair[0].scenario != pair[1].scenario
            || Vec2::new(
                pair[1].body_lean_pitch_degrees - pair[0].body_lean_pitch_degrees,
                pair[1].body_lean_roll_degrees - pair[0].body_lean_roll_degrees,
            )
            .length()
                <= 2.01
    });
    let has_scenario = |name: &str| frames.iter().any(|frame| frame.scenario == name);
    let body_response_valid = (!has_scenario("speed-ramp-up-down")
        || ((-12.0..=-8.0).contains(&ramp_pitch_minimum)
            && (6.0..=10.0).contains(&ramp_pitch_maximum)))
        && (!has_scenario("hard-stop") || (6.0..=10.0).contains(&hard_stop_pitch_maximum))
        && (!has_scenario("dynamics-turn-90") || (6.0..=10.0).contains(&turn_90_roll.1))
        && (!has_scenario("dynamics-turn-180") || (6.0..=10.0).contains(&turn_180_roll.1))
        && lean_step_valid
        && ["speed-ramp-up-down", "hard-stop"]
            .iter()
            .filter(|scenario| has_scenario(scenario))
            .all(|scenario| {
                frames
                    .iter()
                    .rev()
                    .find(|frame| frame.scenario == *scenario)
                    .is_some_and(|frame| {
                        Vec2::new(frame.body_lean_pitch_degrees, frame.body_lean_roll_degrees)
                            .length()
                            <= 0.5
                    })
            });
    let landing_frames = frames
        .iter()
        .filter(|frame| scenario_metadata(&frame.scenario).kind == ScenarioKind::Landing)
        .collect::<Vec<_>>();
    let landing_response_valid = landing_frames.is_empty()
        || (landing_frames
            .last()
            .zip(landing_frames.first())
            .is_some_and(|(last, first)| {
                last.landing_sequence.wrapping_sub(first.landing_sequence) == 1
            })
            && (0.04..=0.08).contains(
                &landing_frames
                    .iter()
                    .map(|frame| frame.landing_compression_metres)
                    .fold(0.0, f32::max),
            )
            && landing_frames
                .last()
                .is_some_and(|frame| frame.landing_compression_metres <= 0.005)
            && landing_frames
                .iter()
                .max_by(|left, right| {
                    left.landing_compression_metres
                        .total_cmp(&right.landing_compression_metres)
                })
                .is_some_and(|frame| frame_minimum_knee_flexion(frame) >= 10.0));
    let landing_grounded_frames = landing_frames
        .iter()
        .copied()
        .filter(|frame| frame.scenario_frame >= 32)
        .collect::<Vec<_>>();
    let landing_foot_preservation_valid = landing_frames.is_empty()
        || landing_grounded_frames.last().is_some_and(|reference| {
            ["left_foot", "right_foot"].iter().all(|name| {
                let Some(reference_position) = reference
                    .bones
                    .get(*name)
                    .map(|bone| Vec3::from_array(bone.position))
                else {
                    return false;
                };
                landing_grounded_frames.iter().all(|frame| {
                    frame.bones.get(*name).is_some_and(|bone| {
                        Vec3::from_array(bone.position).distance(reference_position) <= 0.01
                            && bone
                                .terrain_clearance_metres
                                .is_some_and(|height| height >= -0.01)
                    })
                })
            })
        });
    let ordinary_swing_tracking_valid = frames.iter().all(|frame| {
        if scenario_metadata(&frame.scenario).kind != ScenarioKind::Terrain
            || frame.speed_metres_per_second <= 0.05
            || frame.ik_settle_progress.is_some()
        {
            return true;
        }
        [
            (
                "left_foot",
                frame.ik_left_support_weight,
                frame.ik_left_authored_target,
                frame.ik_left_solve_target,
                frame.ik_left_planned_contact,
                frame.ik_left_release_active,
            ),
            (
                "right_foot",
                frame.ik_right_support_weight,
                frame.ik_right_authored_target,
                frame.ik_right_solve_target,
                frame.ik_right_planned_contact,
                frame.ik_right_release_active,
            ),
        ]
        .into_iter()
        .all(
            |(_foot, support, authored, solved, planned, release_active)| {
                support > 0.05
                    || authored.zip(solved).is_some_and(|(authored, solved)| {
                        let authored = Vec3::from_array(authored);
                        let solved = Vec3::from_array(solved);
                        planned.is_some() || release_active || solved.distance(authored) <= 0.03
                    })
            },
        )
    }) && frames.windows(2).all(|pair| {
        if pair[0].scenario != pair[1].scenario {
            return true;
        }
        if scenario_metadata(&pair[1].scenario).kind != ScenarioKind::Terrain
            || pair[1].speed_metres_per_second <= 0.05
            || pair[1].ik_settle_progress.is_some()
        {
            return true;
        }
        [
            (
                pair[0].ik_left_planned_contact,
                pair[1].ik_left_planned_contact,
                pair[0].ik_left_solve_target,
                pair[1].ik_left_solve_target,
                pair[1].ik_left_support_weight,
                pair[1].ik_left_release_active,
                pair[0].ik_left_release_target,
                pair[1].ik_left_release_target,
            ),
            (
                pair[0].ik_right_planned_contact,
                pair[1].ik_right_planned_contact,
                pair[0].ik_right_solve_target,
                pair[1].ik_right_solve_target,
                pair[1].ik_right_support_weight,
                pair[1].ik_right_release_active,
                pair[0].ik_right_release_target,
                pair[1].ik_right_release_target,
            ),
        ]
        .into_iter()
        .all(
            |(
                before_plan,
                after_plan,
                before_solve,
                after_solve,
                support,
                release_active,
                before_release_target,
                after_release_target,
            )| {
                if support > 0.5 {
                    return true;
                }
                if before_plan.is_none() && after_plan.is_none() && release_active {
                    return before_solve.zip(after_solve).is_some_and(
                        |(before_solve, after_solve)| {
                            let before_solve_world = Vec3::from_array(before_solve);
                            let after_solve_world = Vec3::from_array(after_solve);
                            let before_solve = target_body_local(&pair[0], before_solve_world);
                            let after_solve = target_body_local(&pair[1], after_solve_world);
                            let frozen_goal_converges = before_release_target
                                .zip(after_release_target)
                                .is_none_or(|(before_goal, after_goal)| {
                                    let before_goal = Vec3::from_array(before_goal);
                                    let after_goal = Vec3::from_array(after_goal);
                                    target_body_local(&pair[0], before_goal)
                                        .distance(target_body_local(&pair[1], after_goal))
                                        > 0.002
                                        || after_solve_world.distance(after_goal)
                                            <= before_solve_world.distance(before_goal) + 0.001
                                });
                            after_solve.distance(before_solve) <= 0.055 && frozen_goal_converges
                        },
                    );
                }
                before_plan
                    .zip(after_plan)
                    .zip(before_solve.zip(after_solve))
                    .is_none_or(|((before_plan, after_plan), (before_solve, after_solve))| {
                        let before_plan = Vec3::from_array(before_plan);
                        let after_plan = Vec3::from_array(after_plan);
                        before_plan.distance(after_plan) <= 0.002
                            && Vec3::from_array(after_solve).distance(after_plan)
                                <= Vec3::from_array(before_solve).distance(before_plan) + 0.005
                    })
            },
        )
    });
    let reported_support_contacts_valid = reported_support_contacts_are_valid(&frames);
    let stop_settle_scenarios = [
        "terrain-tap-stop-forward",
        "terrain-stop-mid-swing",
        "terrain-run-flight-stop",
        "terrain-tap-restart-crossfade",
    ];
    let stop_settle_capture_valid = stop_settle_scenarios.iter().all(|scenario| {
        let scenario_frames = frames
            .iter()
            .filter(|frame| frame.scenario == *scenario)
            .collect::<Vec<_>>();
        scenario_frames.is_empty()
            || (scenario_frames.iter().any(|frame| {
                let Some(capture) = frame.ik_settle_capture_point.map(Vec3::from_array) else {
                    return false;
                };
                let direction = Vec3::from_array(frame.world_travel_direction).normalize_or_zero();
                frame
                    .ik_left_planned_contact
                    .or(frame.ik_right_planned_contact)
                    .is_some_and(|target| {
                        (Vec3::from_array(target) - capture).dot(direction) >= 0.02
                    })
            }) && scenario_frames
                .iter()
                .rev()
                .take(12)
                .all(|frame| frame.ik_settle_progress.is_none()))
    });
    let final_support_balance_valid = stop_settle_scenarios.iter().all(|scenario| {
        !frames.iter().any(|frame| frame.scenario == *scenario)
            || frames
                .iter()
                .rev()
                .find(|frame| frame.scenario == *scenario)
                .is_some_and(|frame| support_capsule_margin(frame) >= -0.02)
    });
    let hard_stop_maximum_pelvis_step_metres = hard_stop_pelvis_vertical_step(&frames);
    let hard_stop_height_continuity_valid =
        hard_stop_maximum_pelvis_step_metres.is_none_or(|maximum_step| maximum_step <= 0.02);
    let biomechanics_within_review_bounds = scenarios.iter().all(|metrics| {
        // Raised guard deliberately adds a little vertical readiness through
        // the pelvis and torso. Keep the stricter ordinary-locomotion gate,
        // while allowing the documented guard silhouette (including the
        // transition scenario) rather than reporting it as a regression.
        let vertical_range_limit =
            vertical_range_limit(&metrics.scenario, metrics.foot_terrain_relief_metres);
        // Knee reserve/hemisphere are analytic-solver contracts, not authored
        // FK pose requirements. Apply them only where that solver is active.
        let procedural_solver_gates_apply = procedural_leg_solver_gates_apply(&metrics.scenario);
        if metrics.scenario.starts_with("raised-guard-tap-stop") {
            return metrics.minimum_inter_foot_separation_metres
                >= RAISED_MINIMUM_INTER_FOOT_SEPARATION_METRES
                && metrics.final_facing_motion_error_degrees <= 3.0
                && metrics.pelvis_vertical_range_metres <= vertical_range_limit
                && metrics.head_vertical_range_metres <= vertical_range_limit;
        }
        let attack = scenario_metadata(&metrics.scenario).kind == ScenarioKind::Attack;
        (!procedural_solver_gates_apply
            || attack
            || (metrics.maximum_supported_foot_slip_metres_per_frame
                <= supported_foot_slip_limit(&metrics.scenario)
                && metrics.maximum_planted_foot_drift_metres
                    <= planted_drift_limit(&metrics.scenario)))
            && metrics.minimum_signed_foot_track_metres >= -0.01
            && metrics.minimum_inter_foot_separation_metres
                >= inter_foot_separation_limit(&metrics.scenario)
            && (!procedural_solver_gates_apply
                || metrics.scenario == "attack-step-stationary"
                || (metrics.minimum_knee_flexion_degrees >= 3.9
                    && metrics.minimum_knee_hemisphere_dot >= 0.0))
            && metrics.maximum_facing_tracking_excess_degrees <= 0.2
            && metrics.final_facing_motion_error_degrees <= 3.0
            && (attack || metrics.maximum_contact_sole_clearance_metres <= 0.04)
            && metrics.pelvis_vertical_range_metres <= vertical_range_limit
            && metrics.head_vertical_range_metres <= vertical_range_limit
            && if !expects_loop_seam(&metrics.scenario) {
                metrics.loop_seam_position_metres.is_none()
                    && metrics.loop_seam_rotation_degrees.is_none()
            } else {
                metrics
                    .loop_seam_position_metres
                    .is_some_and(|value| value <= 0.015)
                    && metrics
                        .loop_seam_rotation_degrees
                        .is_some_and(|value| value <= 5.0)
            }
    });
    let views_are_distinct = sequence.duplicate_view_frames.is_empty();
    let repeated_evaluation_valid = sequence.repeated_evaluation_valid;
    let attack_footwork_valid = validate_attack_footwork(&frames);
    let manifest = CaptureManifest {
        sample_hz: SAMPLE_HZ,
        pipeline: "shared tactical player, scene, camera, authoritative locomotion projection, authored FK, and final procedural passes",
        views: VIEWS,
        validation: CaptureValidation {
            finite_transforms,
            all_scenarios_complete,
            all_artifacts_written,
            continuity_within_review_bounds,
            biomechanics_within_review_bounds,
            no_ground_penetration,
            raised_guard_fixed_lead,
            flat_controller_height_stable,
            phase_owned_height_valid,
            run_flight_valid,
            body_response_valid,
            speed_ramp_phase_continuity_valid,
            contact_sequences_valid,
            cadence_contact_valid,
            event_stream_valid,
            landing_response_valid,
            landing_foot_preservation_valid,
            ordinary_swing_tracking_valid,
            reported_support_contacts_valid,
            stop_settle_capture_valid,
            final_support_balance_valid,
            hard_stop_maximum_pelvis_step_metres,
            hard_stop_height_continuity_valid,
            repeated_evaluation_valid,
            attack_footwork_valid,
            views_are_distinct,
            duplicate_view_frames: sequence.duplicate_view_frames.clone(),
            note: "Continuity metrics are regression signals, not biomechanical proof; review index.html at normal and slow speed.",
        },
        scenarios,
        frames,
        presentation_events,
    };
    let manifest_path = sequence.output.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("capture manifest must serialize"),
    )
    .unwrap_or_else(|error| panic!("failed to write {manifest_path:?}: {error}"));
    let index_path = sequence.output.join("index.html");
    fs::write(&index_path, review_html(&manifest))
        .unwrap_or_else(|error| panic!("failed to write {index_path:?}: {error}"));
    info!(path = ?index_path, "Animation review capture completed");
    if finite_transforms
        && all_scenarios_complete
        && all_artifacts_written
        && continuity_within_review_bounds
        && biomechanics_within_review_bounds
        && no_ground_penetration
        && raised_guard_fixed_lead
        && flat_controller_height_stable
        && phase_owned_height_valid
        && run_flight_valid
        && body_response_valid
        && speed_ramp_phase_continuity_valid
        && contact_sequences_valid
        && cadence_contact_valid
        && event_stream_valid
        && landing_response_valid
        && landing_foot_preservation_valid
        && ordinary_swing_tracking_valid
        && reported_support_contacts_valid
        && stop_settle_capture_valid
        && final_support_balance_valid
        && hard_stop_height_continuity_valid
        && repeated_evaluation_valid
        && attack_footwork_valid
        && views_are_distinct
    {
        exit.write(AppExit::Success);
    } else {
        let failure_path = sequence.output.join("failure.txt");
        fs::write(
            &failure_path,
            "capture failed artifact/completeness/continuity/biomechanics/penetration/distinct-view validation; inspect manifest.json\n",
        )
        .unwrap_or_else(|error| panic!("failed to write {failure_path:?}: {error}"));
        exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
    }
}

fn validate_attack_footwork(frames: &[FrameSample]) -> bool {
    let mut grouped = BTreeMap::<&str, Vec<&FrameSample>>::new();
    for frame in frames
        .iter()
        .filter(|frame| frame.scenario.starts_with("attack-step-"))
    {
        grouped.entry(&frame.scenario).or_default().push(frame);
    }
    if grouped.is_empty() {
        return true;
    }
    let expected = [
        "attack-step-forward-left-lead",
        "attack-step-forward-right-lead",
        "attack-step-backward-left-lead",
        "attack-step-backward-right-lead",
        "attack-step-stationary",
        "attack-step-high-speed-reversal",
        "attack-step-reversal",
        "attack-step-yaw-only",
        "attack-step-terrain-cross-slope",
    ];
    let selected = if grouped.len() == 1 {
        grouped.keys().copied().collect::<Vec<_>>()
    } else {
        expected.to_vec()
    };
    selected.into_iter().all(|name| {
        let Some(samples) = grouped.get(name) else {
            return false;
        };
        let start_lead = if name.contains("right-lead") {
            LeadFoot::Right
        } else {
            LeadFoot::Left
        };
        let expected_step = if name == "attack-step-stationary" {
            AttackStep::Stay
        } else if name.contains("backward") {
            AttackStep::Backward
        } else {
            AttackStep::Forward
        };
        let active = samples
            .iter()
            .copied()
            .filter(|frame| frame.action == SkeletonAction::Attack)
            .collect::<Vec<_>>();
        if active.is_empty()
            || active.iter().any(|frame| {
                frame.attack_step != expected_step
                    || (frame.action_phase < 0.999 && frame.lead_foot != start_lead)
            })
            || active
                .windows(2)
                .any(|pair| pair[1].action_phase + 0.0001 < pair[0].action_phase)
        {
            return false;
        }
        let contact = active.iter().min_by(|left, right| {
            (left.action_phase - 0.5)
                .abs()
                .total_cmp(&(right.action_phase - 0.5).abs())
        });
        if contact.is_none_or(|frame| {
            (frame.action_phase - 0.5).abs() > 0.001 || frame.scenario_frame != 27
        }) {
            return false;
        }
        let final_lead = samples.last().map(|frame| frame.lead_foot);
        if expected_step == AttackStep::Stay {
            let stable = [true, false].into_iter().all(|left| {
                let positions = active
                    .iter()
                    .filter_map(|frame| {
                        if left {
                            frame.attack_requested_left_foot_target
                        } else {
                            frame.attack_requested_right_foot_target
                        }
                    })
                    .map(Vec3::from_array)
                    .collect::<Vec<_>>();
                positions.first().is_some_and(|first| {
                    positions
                        .iter()
                        .all(|position| position.distance(*first) <= 0.03)
                })
            });
            return final_lead == Some(start_lead)
                && stable
                && active.iter().all(|frame| {
                    frame.left_support_weight >= 0.99
                        && frame.right_support_weight >= 0.99
                        && frame.attack_support_handoffs == 0
                });
        }
        let opposite = match start_lead {
            LeadFoot::Left => LeadFoot::Right,
            LeadFoot::Right => LeadFoot::Left,
        };
        if final_lead != Some(opposite) {
            return false;
        }
        // The initially planted foot must remain fixed through contact. At
        // high speed the analytic reach limiter may compress the visible
        // lunge, so judge the requested world target and separately retain the
        // global continuity/reach gates for rendered bones.
        let planted_left = match expected_step {
            AttackStep::Forward => start_lead == LeadFoot::Left,
            AttackStep::Backward => start_lead == LeadFoot::Right,
            AttackStep::Stay => true,
        };
        let requested = active
            .iter()
            .take_while(|frame| frame.action_phase <= 0.5 + 0.001)
            .filter_map(|frame| {
                if planted_left {
                    frame.attack_requested_left_foot_target
                } else {
                    frame.attack_requested_right_foot_target
                }
            })
            .map(Vec3::from_array)
            .collect::<Vec<_>>();
        let requested_plant_stable = requested.first().is_some_and(|first| {
            requested
                .iter()
                .all(|target| target.is_finite() && target.distance(*first) <= 0.01)
        });
        let planted_foot = if planted_left {
            "left_foot"
        } else {
            "right_foot"
        };
        let maximum_slip = active
            .windows(2)
            .filter(|pair| pair[1].action_phase <= 0.5 + 0.001)
            .filter_map(|pair| {
                Some(
                    (Vec3::from_array(pair[1].bones.get(planted_foot)?.position)
                        - Vec3::from_array(pair[0].bones.get(planted_foot)?.position))
                    .xz()
                    .length(),
                )
            })
            .fold(0.0, f32::max);
        let slip_limit = if name.contains("high-speed") {
            0.11
        } else if name.contains("terrain") {
            // The analytic cross-slope solve contributes a small horizontal
            // component while conforming the ankle to the surface normal.
            0.05
        } else {
            0.04
        };
        let moving_foot = if planted_left {
            "right_foot"
        } else {
            "left_foot"
        };
        // Measure the step in the facing frame captured when the attack
        // began. A player may continue turning during the action; judging
        // each frame in its new body frame would turn a valid lunge into a
        // sideways or backwards step in telemetry.
        let attack_forward = active
            .first()
            .map(|frame| Vec3::from_array(frame.body_forward_direction).normalize_or_zero())
            .unwrap_or(Vec3::NEG_Z);
        let moving_start = active.first().and_then(|frame| {
            frame
                .bones
                .get(moving_foot)
                .map(|bone| Vec3::from_array(bone.position))
        });
        let signed_extensions = active
            .iter()
            .filter_map(|frame| {
                let moving = Vec3::from_array(frame.bones.get(moving_foot)?.position);
                let signed = (moving - moving_start?).dot(attack_forward);
                Some((frame.scenario_frame, signed))
            })
            .collect::<Vec<_>>();
        let direction_valid = contact.is_some_and(|frame| {
            let moving = frame
                .bones
                .get(moving_foot)
                .map(|bone| Vec3::from_array(bone.position));
            moving.zip(moving_start).is_some_and(|(moving, start)| {
                let margin = (moving - start).dot(attack_forward);
                if expected_step == AttackStep::Forward {
                    margin >= 0.05
                } else {
                    margin <= -0.05
                }
            })
        });
        // Recovery may include an in-place guard pivot after a large facing
        // change. Judge strike extent only over the lunge half; recovery is
        // separately bounded by continuity and the one-handoff invariant.
        let maximum_extension = signed_extensions
            .iter()
            .filter(|(frame, _)| *frame <= 27)
            .map(|(_, extension)| extension.abs())
            .fold(0.0, f32::max);
        let maximum_at_contact = signed_extensions
            .iter()
            .find(|(frame, _)| *frame == 27)
            // Two centimetres covers the two fixed samples around exact
            // authored contact while still rejecting a visibly early step.
            .is_some_and(|(_, extension)| extension.abs() + 0.02 >= maximum_extension);
        let constrained_continuous = [true, false].into_iter().all(|left| {
            active
                .windows(2)
                .filter_map(|pair| {
                    let target = |frame: &FrameSample| {
                        if left {
                            frame.attack_constrained_left_foot_target
                        } else {
                            frame.attack_constrained_right_foot_target
                        }
                        .map(Vec3::from_array)
                    };
                    Some(target(pair[0])?.distance(target(pair[1])?))
                })
                .all(|step| step <= ATTACK_MAXIMUM_CONSTRAINED_TARGET_STEP_METRES)
        });
        let final_attack = active.last().copied();
        let airborne_lunge = name.contains("high-speed");
        let yield_valid = if airborne_lunge {
            final_attack.is_some_and(|frame| frame.attack_maximum_reach_yield_metres <= 0.15)
        } else {
            active
                .iter()
                .all(|frame| frame.attack_maximum_reach_yield_metres <= 4.0)
        };
        let support_valid = if airborne_lunge {
            contact.is_some_and(|frame| {
                frame.left_support_weight.max(frame.right_support_weight) >= 0.9
                    && frame.left_support_weight.min(frame.right_support_weight) <= 0.1
            }) && active.first().is_some_and(|frame| {
                frame.left_support_weight.max(frame.right_support_weight) >= 0.9
            })
        } else {
            active
                .iter()
                .all(|frame| frame.left_support_weight.max(frame.right_support_weight) >= 0.5)
        };
        let valid = (requested_plant_stable || airborne_lunge)
            && maximum_slip <= slip_limit
            && direction_valid
            && maximum_at_contact
            && constrained_continuous
            && yield_valid
            && final_attack.is_some_and(|frame| frame.attack_support_handoffs == 1)
            && support_valid
            && active
                .iter()
                .all(|frame| frame.attack_maximum_reach_yield_metres.is_finite());
        if !valid {
            warn!(
                scenario = name,
                requested_plant_stable,
                maximum_slip,
                slip_limit,
                direction_valid,
                maximum_at_contact,
                constrained_continuous,
                yield_valid,
                support_valid,
                handoffs = final_attack.map_or(0, |frame| frame.attack_support_handoffs),
                "attack footwork validation failed"
            );
        }
        valid
    })
}

fn planted_drift_limit(scenario: &str) -> f32 {
    if scenario.starts_with("raised-guard") {
        0.01
    } else if scenario == "terrain-crouch-cross-slope" {
        // The crouched stride deliberately operates near its compact reach limit.
        // Permit the small sole travel needed to keep both feet above uneven ground.
        0.051
    } else if scenario == "terrain-steady-run-5.5" {
        // At full run speed the leg spends the last few contact samples near
        // its reach boundary. Keep instantaneous slip strict while permitting
        // the measured 5.5 cm of cumulative reach accommodation.
        0.06
    } else {
        0.035
    }
}

fn estimated_center_of_mass(frame: &FrameSample) -> Vec3 {
    let mut weighted = Vec3::ZERO;
    let mut total = 0.0;
    for (bone, weight) in [("pelvis", 0.45), ("chest", 0.35), ("head", 0.20)] {
        if let Some(sample) = frame.bones.get(bone) {
            weighted += Vec3::from_array(sample.position) * weight;
            total += weight;
        }
    }
    if total > 0.0 {
        weighted / total
    } else {
        Vec3::from_array(frame.root_position_metres)
    }
}

/// Signed horizontal margin around the convex support approximation formed by
/// both soles. The 18 cm radius includes half a boot and the small amount of
/// ankle strategy a human can use without taking another step.
fn support_capsule_margin(frame: &FrameSample) -> f32 {
    let Some(left) = frame
        .bones
        .get("left_foot")
        .map(|bone| Vec3::from_array(bone.position).xz())
    else {
        return f32::NEG_INFINITY;
    };
    let Some(right) = frame
        .bones
        .get("right_foot")
        .map(|bone| Vec3::from_array(bone.position).xz())
    else {
        return f32::NEG_INFINITY;
    };
    let com = estimated_center_of_mass(frame).xz();
    let segment = right - left;
    let progress = if segment.length_squared() > 0.000001 {
        ((com - left).dot(segment) / segment.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    0.18 - com.distance(left + segment * progress)
}

fn supported_foot_slip_limit(scenario: &str) -> f32 {
    let _ = scenario;
    0.035
}

fn procedural_leg_solver_gates_apply(scenario: &str) -> bool {
    scenario_metadata(scenario).procedural_solver
}

fn inter_foot_separation_limit(scenario: &str) -> f32 {
    if scenario.starts_with("raised-guard") {
        RAISED_MINIMUM_INTER_FOOT_SEPARATION_METRES
    } else {
        0.08
    }
}

fn scenario_metrics(frames: &[FrameSample]) -> Vec<ScenarioMetrics> {
    let mut grouped = BTreeMap::<&str, Vec<&FrameSample>>::new();
    for frame in frames {
        grouped.entry(&frame.scenario).or_default().push(frame);
    }
    grouped
        .into_iter()
        .map(|(scenario, frames)| {
            // Terrain conformity calibrates retained pelvis/plant state during
            // its first cycle. Judge its steady gait after that deterministic
            // warmup, matching the phase-height validation window.
            let metadata = scenario_metadata(scenario);
            let metric_frames = if metadata.kind == ScenarioKind::Terrain {
                phase_validation_frames(&frames)
            } else {
                frames.clone()
            };
            let procedural_frames = metric_frames
                .iter()
                .copied()
                .filter(|frame| metadata.kind == ScenarioKind::Terrain || frame.guard_action)
                .collect::<Vec<_>>();
            let mut maximum_step = 0.0_f32;
            let mut maximum_leg_step = 0.0_f32;
            let mut maximum_foot_step = 0.0_f32;
            let mut maximum_knee_step = 0.0_f32;
            let mut maximum_rotation = 0.0_f32;
            let mut maximum_slip = 0.0_f32;
            let mut worst_displacement = None;
            let mut worst_rotation = None;
            // Frame zero establishes retained procedural state from a fresh
            // component. The scenario pre-roll begins at frame one; continuity
            // gates therefore start with the first real pre-roll transition
            // (1→2) without exempting any movement frame.
            for pair in metric_frames.windows(2).skip(1) {
                let (before, after) = (pair[0], pair[1]);
                let body_turn = Quat::from_array(before.body_rotation_xyzw)
                    .angle_between(Quat::from_array(after.body_rotation_xyzw))
                    .to_degrees();
                for (name, before_bone) in &before.bones {
                    let Some(after_bone) = after.bones.get(name) else {
                        continue;
                    };
                    let displacement = body_local(after, name)
                        .zip(body_local(before, name))
                        .map_or(f32::INFINITY, |(after, before)| after.distance(before));
                    if displacement > maximum_step {
                        maximum_step = displacement;
                        worst_displacement = Some(ContinuityLocation {
                            bone: name.clone(),
                            from_frame: before.scenario_frame,
                            to_frame: after.scenario_frame,
                            value: displacement,
                        });
                    }
                    if is_leg_bone(name) {
                        maximum_leg_step = maximum_leg_step.max(displacement);
                    }
                    if is_foot_bone(name) {
                        maximum_foot_step = maximum_foot_step.max(displacement);
                    }
                    if is_knee_bone(name) {
                        maximum_knee_step = maximum_knee_step.max(displacement);
                    }
                    let rotation = body_local_rotation(after, after_bone)
                        .angle_between(body_local_rotation(before, before_bone))
                        .to_degrees();
                    if rotation > maximum_rotation {
                        maximum_rotation = rotation;
                        worst_rotation = Some(ContinuityLocation {
                            bone: name.clone(),
                            from_frame: before.scenario_frame,
                            to_frame: after.scenario_frame,
                            value: rotation,
                        });
                    }
                }
                for (foot, support) in [
                    (
                        "left_foot",
                        before.left_support_weight.min(after.left_support_weight),
                    ),
                    (
                        "right_foot",
                        before.right_support_weight.min(after.right_support_weight),
                    ),
                ] {
                    // A body turn deliberately slides an old world plant onto
                    // its new anatomical side corridor. Treat that bounded
                    // adaptation separately from skating under a stable body.
                    let procedural_plant_active = metadata.kind == ScenarioKind::Terrain
                        || (before.guard_action && after.guard_action);
                    if procedural_plant_active
                        && support >= 0.9
                        && body_turn <= 0.5
                        && let (Some(a), Some(b)) = (before.bones.get(foot), after.bones.get(foot))
                    {
                        maximum_slip = maximum_slip.max(
                            (Vec3::from_array(b.position) - Vec3::from_array(a.position))
                                .xz()
                                .length(),
                        );
                    }
                }
            }
            let maximum_plant_drift = maximum_planted_foot_drift(scenario, &metric_frames);
            let looped = expects_loop_seam(scenario);
            let (loop_position, loop_rotation) = if looped {
                let seams = frames
                    .windows(2)
                    .filter(|pair| (pair[1].gait_phase - pair[0].gait_phase).abs() > 0.5)
                    .map(|pair| loop_seam(pair[0], pair[1]))
                    .collect::<Vec<_>>();
                if seams.is_empty() {
                    (None, None)
                } else {
                    (
                        Some(seams.iter().map(|seam| seam.0).fold(0.0, f32::max)),
                        Some(seams.iter().map(|seam| seam.1).fold(0.0, f32::max)),
                    )
                }
            } else {
                (None, None)
            };
            let (visual_height_peak_count, visual_height_peaks_in_passing_windows) =
                visual_height_peaks(&metric_frames);
            ScenarioMetrics {
                scenario: scenario.to_owned(),
                frame_count: frames.len(),
                maximum_root_relative_step_metres: maximum_step,
                maximum_leg_root_relative_step_metres: maximum_leg_step,
                maximum_foot_root_relative_step_metres: maximum_foot_step,
                maximum_knee_root_relative_step_metres: maximum_knee_step,
                worst_displacement,
                maximum_bone_rotation_step_degrees: maximum_rotation,
                worst_rotation,
                loop_seam_position_metres: loop_position,
                loop_seam_rotation_degrees: loop_rotation,
                pelvis_vertical_range_metres: root_relative_vertical_range(
                    &metric_frames,
                    "pelvis",
                ),
                maximum_pelvis_vertical_step_metres: root_relative_vertical_step(
                    &metric_frames,
                    "pelvis",
                ),
                controller_vertical_range_metres: controller_vertical_range(&metric_frames),
                phase_height_range_metres: phase_height_range(&metric_frames),
                contact_to_passing_height_gain_metres: contact_to_passing_height_gain(
                    &metric_frames,
                ),
                visual_height_peak_count,
                visual_height_peaks_in_passing_windows,
                maximum_no_support_seconds: maximum_no_support_seconds(&metric_frames),
                minimum_flight_sole_clearance_metres: minimum_flight_sole_clearance(&metric_frames),
                minimum_contact_sole_clearance_metres: contact_sole_clearance_range(&metric_frames)
                    .0,
                maximum_contact_sole_clearance_metres: contact_sole_clearance_range(&metric_frames)
                    .1,
                head_vertical_range_metres: root_relative_vertical_range(&metric_frames, "head"),
                foot_terrain_relief_metres: foot_terrain_relief(&metric_frames),
                minimum_knee_forward_bend_metres: minimum_knee_bend(&metric_frames),
                minimum_signed_foot_track_metres: minimum_signed_foot_track(&metric_frames),
                minimum_inter_foot_separation_metres: minimum_inter_foot_separation(&metric_frames),
                minimum_knee_flexion_degrees: minimum_knee_flexion(&procedural_frames),
                minimum_knee_hemisphere_dot: minimum_knee_hemisphere(&procedural_frames),
                maximum_facing_motion_error_degrees: maximum_facing_error(&metric_frames),
                maximum_facing_tracking_excess_degrees: maximum_facing_tracking_excess(
                    &metric_frames,
                ),
                maximum_guard_facing_error_degrees: maximum_guard_facing_error(&metric_frames),
                final_facing_motion_error_degrees: final_facing_error(&metric_frames),
                maximum_supported_foot_slip_metres_per_frame: maximum_slip,
                maximum_planted_foot_drift_metres: maximum_plant_drift,
                minimum_foot_clearance_metres: minimum_foot_clearance(&metric_frames),
            }
        })
        .collect()
}

fn expects_loop_seam(scenario: &str) -> bool {
    scenario_metadata(scenario).repeatable
}

fn vertical_range_limit(scenario: &str, foot_terrain_relief_metres: f32) -> f32 {
    if scenario.starts_with("attack-step-") {
        0.35
    } else if scenario.starts_with("raised-guard-") {
        RAISED_GUARD_VERTICAL_RANGE_LIMIT_METRES
    } else if scenario == "hard-stop" {
        // The scenario intentionally spans the run apex and exact final idle.
        // Per-frame release continuity is enforced separately at 2 cm.
        ORDINARY_VERTICAL_RANGE_LIMIT_METRES + 0.01
    } else if scenario_metadata(scenario).kind == ScenarioKind::Terrain {
        ORDINARY_VERTICAL_RANGE_LIMIT_METRES + foot_terrain_relief_metres.max(0.0)
    } else {
        ORDINARY_VERTICAL_RANGE_LIMIT_METRES
    }
}

fn expected_visual_height(scenario: &str) -> Option<(f32, f32, usize)> {
    Some(match scenario {
        "steady-walk-2.0" => (0.025, 0.06, 2),
        "steady-run-5.5" => (0.03, 0.12, 2),
        "steady-crouch-1.5" => (0.035, 0.065, 2),
        "raised-guard-forward" | "raised-guard-half-speed" => (0.018, 0.05, 2),
        _ => return None,
    })
}

fn quat(bone: &BoneSample) -> Quat {
    Quat::from_array(bone.rotation_xyzw).normalize()
}

fn body_local(frame: &FrameSample, bone: &str) -> Option<Vec3> {
    let world = Vec3::from_array(frame.bones.get(bone)?.position)
        - Vec3::from_array(frame.root_position_metres);
    Some(Quat::from_array(frame.body_rotation_xyzw).inverse() * world)
}

fn is_leg_bone(name: &str) -> bool {
    matches!(
        name,
        "left_hip" | "right_hip" | "left_knee" | "right_knee" | "left_foot" | "right_foot"
    )
}

fn is_foot_bone(name: &str) -> bool {
    matches!(name, "left_foot" | "right_foot")
}

fn is_knee_bone(name: &str) -> bool {
    matches!(name, "left_knee" | "right_knee")
}

fn target_body_local(frame: &FrameSample, world: Vec3) -> Vec3 {
    Quat::from_array(frame.body_rotation_xyzw).inverse()
        * (world - Vec3::from_array(frame.root_position_metres))
}

fn body_local_rotation(frame: &FrameSample, bone: &BoneSample) -> Quat {
    Quat::from_array(frame.body_rotation_xyzw).inverse() * quat(bone)
}

fn minimum_signed_foot_track(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .flat_map(|frame| {
            [("left_hip", "left_foot"), ("right_hip", "right_foot")].map(|(hip, foot)| {
                let side = body_local(frame, hip).map_or(1.0, |value| value.x.signum());
                body_local(frame, foot).map_or(f32::INFINITY, |value| value.x * side)
            })
        })
        .fold(f32::INFINITY, f32::min)
}

fn minimum_inter_foot_separation(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .filter_map(|frame| {
            Some((body_local(frame, "left_foot")?.x - body_local(frame, "right_foot")?.x).abs())
        })
        .fold(f32::INFINITY, f32::min)
}

fn minimum_knee_flexion(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .map(|frame| frame_minimum_knee_flexion(frame))
        .fold(f32::INFINITY, f32::min)
}

fn frame_minimum_knee_flexion(frame: &FrameSample) -> f32 {
    [
        ("left_hip", "left_knee", "left_foot"),
        ("right_hip", "right_knee", "right_foot"),
    ]
    .into_iter()
    .map(|(hip, knee, foot)| {
        let (Some(hip), Some(knee), Some(foot)) = (
            body_local(frame, hip),
            body_local(frame, knee),
            body_local(frame, foot),
        ) else {
            return f32::INFINITY;
        };
        180.0 - (hip - knee).angle_between(foot - knee).to_degrees()
    })
    .fold(f32::INFINITY, f32::min)
}

fn minimum_knee_hemisphere(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .flat_map(|frame| {
            [
                ("left_hip", "left_knee", "left_foot"),
                ("right_hip", "right_knee", "right_foot"),
            ]
            .map(|(hip, knee, foot)| {
                let (Some(hip), Some(knee), Some(foot)) = (
                    body_local(frame, hip),
                    body_local(frame, knee),
                    body_local(frame, foot),
                ) else {
                    return f32::INFINITY;
                };
                let Some(axis) = (foot - hip).try_normalize() else {
                    return f32::INFINITY;
                };
                let Some(bend) = (knee - hip).reject_from_normalized(axis).try_normalize() else {
                    return -1.0;
                };
                let side = hip.x.signum();
                bend.dot((Vec3::Z + Vec3::X * side * 0.18).normalize())
            })
        })
        .fold(f32::INFINITY, f32::min)
}

fn maximum_facing_error(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .filter(|frame| frame.speed_metres_per_second > 0.05)
        .map(|frame| {
            Vec3::from_array(frame.body_forward_direction)
                .angle_between(Vec3::from_array(frame.desired_body_forward_direction))
                .to_degrees()
        })
        .fold(0.0, f32::max)
}

fn maximum_facing_tracking_excess(frames: &[&FrameSample]) -> f32 {
    frames
        .windows(2)
        .filter(|pair| pair[1].speed_metres_per_second > 0.05)
        .map(|pair| {
            let before = Vec3::from_array(pair[0].body_forward_direction);
            let actual = Vec3::from_array(pair[1].body_forward_direction);
            let desired = Vec3::from_array(pair[1].desired_body_forward_direction);
            let elapsed = (pair[1].time_seconds - pair[0].time_seconds).max(0.0);
            let permitted_residual =
                (before.angle_between(desired) - BODY_TURN_SPEED_RADIANS * elapsed).max(0.0);
            (actual.angle_between(desired) - permitted_residual)
                .abs()
                .to_degrees()
        })
        .fold(0.0, f32::max)
}

fn maximum_guard_facing_error(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .filter(|frame| frame.guard_action && frame.speed_metres_per_second > 0.05)
        .map(|frame| {
            Vec3::from_array(frame.body_forward_direction)
                .angle_between(Vec3::from_array(frame.desired_body_forward_direction))
                .to_degrees()
        })
        .fold(0.0, f32::max)
}

fn final_facing_error(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .rev()
        .find(|frame| frame.speed_metres_per_second > 0.05)
        .map_or(0.0, |frame| {
            Vec3::from_array(frame.body_forward_direction)
                .angle_between(Vec3::from_array(frame.desired_body_forward_direction))
                .to_degrees()
        })
}

fn loop_seam(first: &FrameSample, last: &FrameSample) -> (f32, f32) {
    first
        .bones
        .iter()
        .fold((0.0_f32, 0.0_f32), |metrics, (name, a)| {
            // Stateful foot locking intentionally changes the lower-body
            // contact solution with world position and fixed-step aliasing.
            // Its continuity is gated by per-frame plant slip/drift instead;
            // this seam metric checks the repeatable authored upper body.
            if name.ends_with("_hip") || name.ends_with("_knee") || name.ends_with("_foot") {
                return metrics;
            }
            let Some(b) = last.bones.get(name) else {
                return metrics;
            };
            (
                metrics.0.max(
                    body_local(last, name)
                        .zip(body_local(first, name))
                        .map_or(f32::INFINITY, |(last, first)| last.distance(first)),
                ),
                metrics.1.max(
                    body_local_rotation(first, a)
                        .angle_between(body_local_rotation(last, b))
                        .to_degrees(),
                ),
            )
        })
}

fn root_relative_vertical_range(frames: &[&FrameSample], bone: &str) -> f32 {
    let (minimum, maximum) = frames
        .iter()
        .filter_map(|frame| {
            frame
                .bones
                .get(bone)
                .map(|bone| bone.position[1] - frame.root_position_metres[1])
        })
        .fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
        );
    if minimum.is_finite() && maximum.is_finite() {
        maximum - minimum
    } else {
        0.0
    }
}

fn root_relative_vertical_step(frames: &[&FrameSample], bone: &str) -> f32 {
    frames
        .windows(2)
        .filter_map(|pair| {
            let previous = pair[0].bones.get(bone)?.position[1] - pair[0].root_position_metres[1];
            let current = pair[1].bones.get(bone)?.position[1] - pair[1].root_position_metres[1];
            Some((current - previous).abs())
        })
        .fold(0.0, f32::max)
}

fn hard_stop_pelvis_vertical_step(frames: &[FrameSample]) -> Option<f32> {
    let hard_stop = frames
        .iter()
        .filter(|frame| frame.scenario == "hard-stop")
        .collect::<Vec<_>>();
    if hard_stop.is_empty() {
        return None;
    }
    let Some(first_stopped) = hard_stop
        .iter()
        .position(|frame| frame.speed_metres_per_second <= 0.05)
    else {
        return Some(f32::INFINITY);
    };
    let transition_start = first_stopped.saturating_sub(1);
    Some(root_relative_vertical_step(
        &hard_stop[transition_start..],
        "pelvis",
    ))
}

fn controller_vertical_range(frames: &[&FrameSample]) -> f32 {
    let (minimum, maximum) = frames.iter().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(minimum, maximum), frame| {
            let value = frame.root_position_metres[1];
            (minimum.min(value), maximum.max(value))
        },
    );
    if minimum.is_finite() && maximum.is_finite() {
        maximum - minimum
    } else {
        0.0
    }
}

fn contact_to_passing_height_gain(frames: &[&FrameSample]) -> f32 {
    let frames = phase_validation_frames(frames);
    let phase_distance = |phase: f32, target: f32| {
        let distance = (phase - target).abs();
        distance.min(1.0 - distance)
    };
    let contact = frames
        .iter()
        .filter(|frame| {
            phase_distance(frame.gait_phase, 0.0) <= 0.04
                || phase_distance(frame.gait_phase, 0.5) <= 0.04
        })
        .filter_map(|frame| body_local(frame, "pelvis").map(|pelvis| pelvis.y))
        .fold(f32::NEG_INFINITY, f32::max);
    let passing = frames
        .iter()
        .filter(|frame| {
            phase_distance(frame.gait_phase, 0.25) <= 0.04
                || phase_distance(frame.gait_phase, 0.75) <= 0.04
        })
        .filter_map(|frame| body_local(frame, "pelvis").map(|pelvis| pelvis.y))
        .fold(f32::INFINITY, f32::min);
    if contact.is_finite() && passing.is_finite() {
        passing - contact
    } else {
        0.0
    }
}

fn visual_height_peaks(frames: &[&FrameSample]) -> (usize, bool) {
    let warmed = phase_validation_frames(frames);
    let mut cycle_start = 0;
    let mut measurements = Vec::new();
    for cycle_end in warmed
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| (pair[1].gait_phase < pair[0].gait_phase).then_some(index + 1))
        .chain(std::iter::once(warmed.len()))
    {
        let cycle = &warmed[cycle_start..cycle_end];
        let phase_span = cycle
            .first()
            .zip(cycle.last())
            .map_or(0.0, |(first, last)| last.gait_phase - first.gait_phase);
        // Ignore a trailing partial cycle created when capture ends just after
        // a wrap; every complete post-warmup cycle remains independently gated.
        if phase_span >= 0.8 {
            let samples = cycle
                .iter()
                .filter_map(|frame| {
                    body_local(frame, "pelvis")
                        .map(|pelvis| (frame.gait_phase.rem_euclid(1.0), pelvis.y))
                })
                .collect::<Vec<_>>();
            measurements.push(prominent_height_peaks(&samples));
        }
        cycle_start = cycle_end;
    }
    if measurements.is_empty() {
        return (0, false);
    }
    (
        measurements
            .iter()
            .map(|(count, _)| *count)
            .max()
            .unwrap_or(0),
        measurements
            .iter()
            .all(|(count, in_windows)| *count == 2 && *in_windows),
    )
}

fn prominent_height_peaks(samples: &[(f32, f32)]) -> (usize, bool) {
    if samples.len() < 3 {
        return (0, false);
    }
    let mut peaks = Vec::new();
    for (index, &(phase, height)) in samples.iter().enumerate() {
        let previous = samples[(index + samples.len() - 1) % samples.len()].1;
        let next = samples[(index + 1) % samples.len()].1;
        if height <= previous || height < next {
            continue;
        }
        let left_minimum = samples
            .iter()
            .filter_map(|&(candidate_phase, candidate_height)| {
                let distance = (phase - candidate_phase).rem_euclid(1.0);
                (distance > f32::EPSILON && distance <= 0.125).then_some(candidate_height)
            })
            .fold(f32::INFINITY, f32::min);
        let right_minimum = samples
            .iter()
            .filter_map(|&(candidate_phase, candidate_height)| {
                let distance = (candidate_phase - phase).rem_euclid(1.0);
                (distance > f32::EPSILON && distance <= 0.125).then_some(candidate_height)
            })
            .fold(f32::INFINITY, f32::min);
        let prominence = height - left_minimum.max(right_minimum);
        if prominence >= HEIGHT_PEAK_PROMINENCE_METRES {
            peaks.push(phase);
        }
    }
    let phase_distance = |phase: f32, target: f32| {
        let distance = (phase - target).abs();
        distance.min(1.0 - distance)
    };
    let in_passing_windows = [0.25, 0.75].into_iter().all(|target| {
        peaks
            .iter()
            .filter(|&&phase| phase_distance(phase, target) <= PASSING_PEAK_PHASE_WINDOW)
            .count()
            == 1
    });
    (peaks.len(), in_passing_windows)
}

fn phase_height_range(frames: &[&FrameSample]) -> f32 {
    let frames = phase_validation_frames(frames);
    root_relative_vertical_range(&frames, "pelvis")
}

fn phase_validation_frames<'a>(frames: &'a [&'a FrameSample]) -> Vec<&'a FrameSample> {
    let after_first_wrap = frames
        .windows(2)
        .position(|pair| pair[1].gait_phase < pair[0].gait_phase)
        .map_or(0, |index| index + 1);
    frames[after_first_wrap..].to_vec()
}

fn maximum_no_support_seconds(frames: &[&FrameSample]) -> f32 {
    let mut maximum = 0.0_f32;
    let mut began = None;
    for frame in frames {
        if frame.left_support_weight <= 0.001 && frame.right_support_weight <= 0.001 {
            began.get_or_insert(frame.time_seconds);
            maximum = maximum.max(frame.time_seconds - began.unwrap());
        } else {
            began = None;
        }
    }
    maximum
}

fn minimum_flight_sole_clearance(frames: &[&FrameSample]) -> f32 {
    let minimum = frames
        .iter()
        .filter(|frame| frame.left_support_weight <= 0.001 && frame.right_support_weight <= 0.001)
        .flat_map(|frame| ["left_foot", "right_foot"].map(|foot| (frame, foot)))
        .filter_map(|(frame, foot)| {
            frame
                .bones
                .get(foot)?
                .terrain_clearance_metres
                .map(|ankle| ankle - 0.085)
        })
        .fold(f32::INFINITY, f32::min);
    if minimum.is_finite() { minimum } else { 0.0 }
}

fn contact_sole_clearance_range(frames: &[&FrameSample]) -> (f32, f32) {
    let (minimum, maximum) = frames
        .iter()
        .flat_map(|frame| {
            let procedural_solve_active =
                frame.ik_left_solve_target.is_some() || frame.ik_right_solve_target.is_some();
            [
                ("left_foot", frame.ik_left_support_weight, true),
                ("right_foot", frame.ik_right_support_weight, false),
            ]
            .into_iter()
            .filter_map(move |(foot, support, left)| {
                let is_contact = if procedural_solve_active {
                    support > 0.5
                } else {
                    let phase = frame.gait_phase.rem_euclid(1.0);
                    let distance = if left {
                        phase.min(1.0 - phase)
                    } else {
                        (phase - 0.5).abs()
                    };
                    distance <= 0.035
                };
                if !is_contact {
                    return None;
                }
                frame
                    .bones
                    .get(foot)?
                    .terrain_clearance_metres
                    .map(|ankle| ankle - MEASURED_ANKLE_SOLE_OFFSET_METRES)
            })
        })
        .fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), clearance| (minimum.min(clearance), maximum.max(clearance)),
        );
    if minimum.is_finite() && maximum.is_finite() {
        (minimum, maximum)
    } else {
        (0.0, 0.0)
    }
}

fn reported_support_contacts_are_valid(frames: &[FrameSample]) -> bool {
    frames.iter().all(|frame| {
        [
            ("left_foot", frame.ik_left_support_weight),
            ("right_foot", frame.ik_right_support_weight),
        ]
        .into_iter()
        .all(|(foot, support)| {
            support.is_finite()
                && (support <= 0.0
                    || frame
                        .bones
                        .get(foot)
                        .and_then(|bone| bone.terrain_clearance_metres)
                        .is_some_and(|ankle_clearance| {
                            (ankle_clearance - MEASURED_ANKLE_SOLE_OFFSET_METRES).abs()
                                <= SOLE_CONTACT_TOLERANCE_METRES
                        }))
        })
    })
}

/// Terrain height variation under the two sampled feet relative to the
/// controller root's ground point. Subtracting this measured relief from the
/// torso range preserves the flat-ground gait envelope while allowing the
/// bounded pelvis correction required to keep both legs reachable on a slope.
fn foot_terrain_relief(frames: &[&FrameSample]) -> f32 {
    let (minimum, maximum) = frames
        .iter()
        .flat_map(|frame| {
            ["left_foot", "right_foot"].into_iter().filter_map(|foot| {
                let bone = frame.bones.get(foot)?;
                let clearance = bone.terrain_clearance_metres?;
                let terrain_height = bone.position[1] - clearance;
                let root_ground = frame.root_position_metres[1] - CAPTURE_ROOT_GROUND_OFFSET_METRES;
                Some(terrain_height - root_ground)
            })
        })
        .fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
        );
    if minimum.is_finite() && maximum.is_finite() {
        maximum - minimum
    } else {
        0.0
    }
}

fn maximum_planted_foot_drift(scenario: &str, frames: &[&FrameSample]) -> f32 {
    let mut maximum = 0.0_f32;
    for (foot, left) in [("left_foot", true), ("right_foot", false)] {
        let mut anchor = None;
        for frame in frames {
            if scenario_metadata(scenario).kind != ScenarioKind::Terrain && !frame.guard_action {
                anchor = None;
                continue;
            }
            let support = if left {
                frame.left_support_weight
            } else {
                frame.right_support_weight
            };
            // The IK target deliberately releases continuously below full
            // support. Measure cumulative plant drift only while the solver is
            // effectively pinned; the separate >=0.9 per-frame slip metric
            // continues to gate the blended release interval.
            if support >= FULL_PLANT_SUPPORT_WEIGHT {
                let Some(position) = frame
                    .bones
                    .get(foot)
                    .map(|bone| Vec3::from_array(bone.position).xz())
                else {
                    continue;
                };
                let origin = anchor.get_or_insert(position);
                maximum = maximum.max(position.distance(*origin));
            } else {
                anchor = None;
            }
        }
    }
    maximum
}

fn minimum_knee_bend(frames: &[&FrameSample]) -> f32 {
    let mut minimum = f32::INFINITY;
    for frame in frames {
        for side in ["left", "right"] {
            let (Some(hip), Some(knee), Some(foot)) = (
                frame.bones.get(&format!("{side}_hip")),
                frame.bones.get(&format!("{side}_knee")),
                frame.bones.get(&format!("{side}_foot")),
            ) else {
                continue;
            };
            let (hip, knee, foot) = (
                Vec3::from_array(hip.position),
                Vec3::from_array(knee.position),
                Vec3::from_array(foot.position),
            );
            let axis = foot - hip;
            if axis.length_squared() > 0.0001 {
                let closest = hip + axis * (knee - hip).dot(axis) / axis.length_squared();
                let forward = Vec3::from_array(frame.world_travel_direction)
                    .try_normalize()
                    .unwrap_or(Vec3::Z);
                minimum = minimum.min((knee - closest).dot(forward));
            }
        }
    }
    if minimum.is_finite() { minimum } else { 0.0 }
}

fn minimum_foot_clearance(frames: &[&FrameSample]) -> f32 {
    let minimum = frames
        .iter()
        .flat_map(|frame| [frame.bones.get("left_foot"), frame.bones.get("right_foot")])
        .flatten()
        .filter_map(|foot| foot.terrain_clearance_metres)
        .fold(f32::INFINITY, f32::min);
    if minimum.is_finite() { minimum } else { -1.0 }
}

fn review_html(manifest: &CaptureManifest) -> String {
    let frame_json = serde_json::to_string(&manifest.frames).expect("review frames must serialize");
    let scenario_names_json = serde_json::to_string(
        &manifest
            .scenarios
            .iter()
            .map(|scenario| scenario.scenario.as_str())
            .collect::<Vec<_>>(),
    )
    .expect("review scenario names must serialize");
    let scenario_buttons = manifest
        .scenarios
        .iter()
        .map(|scenario| {
            format!(
                "<button data-scenario=\"{}\">{}</button>",
                scenario.scenario, scenario.scenario
            )
        })
        .collect::<String>();
    let metrics = manifest
        .scenarios
        .iter()
        .map(|scenario| {
            let describe = |worst: &Option<ContinuityLocation>, unit: &str| {
                worst.as_ref().map_or("&mdash;".to_owned(), |worst| {
                    format!(
                        "{} {}&rarr;{} ({:.4}{unit})",
                        worst.bone, worst.from_frame, worst.to_frame, worst.value
                    )
                })
            };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td><td>{:.2}</td><td>{:.3}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.4}</td></tr>",
                scenario.scenario,
                scenario.frame_count,
                describe(&scenario.worst_displacement, "m"),
                describe(&scenario.worst_rotation, "deg"),
                scenario.loop_seam_position_metres.map_or("&mdash;".into(), |value| format!("{value:.4}")),
                scenario.loop_seam_rotation_degrees.map_or("&mdash;".into(), |value| format!("{value:.2}")),
                scenario.maximum_supported_foot_slip_metres_per_frame,
                scenario.maximum_planted_foot_drift_metres,
                scenario.minimum_signed_foot_track_metres,
                scenario.minimum_inter_foot_separation_metres,
                scenario.minimum_knee_flexion_degrees,
                scenario.minimum_knee_hemisphere_dot,
                scenario.maximum_facing_motion_error_degrees,
                scenario.maximum_facing_tracking_excess_degrees,
                scenario.maximum_guard_facing_error_degrees,
                scenario.final_facing_motion_error_degrees,
                scenario.minimum_foot_clearance_metres,
            )
        })
        .collect::<String>();
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Animation review</title><style>
body{{font:15px system-ui;background:#111820;color:#e8eef5;margin:24px}}button,select{{margin:4px;padding:8px}}img{{max-width:min(960px,100%);background:#222}}table{{border-collapse:collapse;margin-top:20px}}td,th{{border:1px solid #526171;padding:6px}}.note{{max-width:960px;color:#b9c7d5}}#contact{{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:8px;margin-top:20px}}#contact img{{width:100%}}
</style></head><body><h1>Tactical locomotion review</h1>
<p class="note">This runs the shared tactical player, hills scene, gameplay camera, 64 Hz authoritative locomotion projection, authored FK, and final procedural passes. Gameplay images are raw; side/front diagnostics add the cyan skeleton and support markers. Use normal speed first, then slow motion.</p>
<div>{scenario_buttons}</div><label>View <select id="view"><option value="gameplay">gameplay (raw)</option><option value="side">side diagnostic</option><option value="front">front diagnostic</option></select></label>
<label>Playback <select id="rate"><option value="1">normal</option><option value="2">half speed</option><option value="4">quarter speed</option></select></label>
<p id="telemetry"></p><img id="player"><div id="contact"></div>
<table><thead><tr><th>scenario</th><th>frames</th><th>worst root-relative displacement</th><th>worst rotation</th><th>loop seam m</th><th>loop seam deg</th><th>supported slip m/frame</th><th>planted interval drift m</th><th>signed foot track m</th><th>inter-foot separation m</th><th>knee flexion deg</th><th>knee hemisphere dot</th><th>maximum facing error deg</th><th>tracking excess deg</th><th>guard facing error deg</th><th>final facing error deg</th><th>minimum terrain-relative foot clearance m</th></tr></thead><tbody>{metrics}</tbody></table>
<script>const all={frame_json},scenarioNames={scenario_names_json};let scenario=scenarioNames[0]||"",i=0,timer;const player=document.querySelector('#player'),view=document.querySelector('#view'),rate=document.querySelector('#rate'),telemetry=document.querySelector('#telemetry');
function frames(){{return all.filter(x=>x.scenario===scenario)}}function show(){{const list=frames(),f=list.length?list[i%list.length]:null;if(!f){{player.removeAttribute('src');telemetry.textContent='No completed capture frames';return}}player.src=f.screenshots[view.value];telemetry.textContent=`${{f.scenario}} frame ${{f.scenario_frame}} | guard ${{f.weapon_guard}} lead ${{f.lead_foot}} | ${{f.speed_metres_per_second.toFixed(2)}} m/s | phase ${{f.gait_phase.toFixed(3)}} | world plants L ${{f.left_support_weight.toFixed(2)}} R ${{f.right_support_weight.toFixed(2)}}`;}}
function play(){{clearInterval(timer);timer=setInterval(()=>{{i=(i+1)%frames().length;show()}},1000/64*Number(rate.value))}}function contacts(){{const f=frames(),step=Math.max(1,Math.floor(f.length/12)),box=document.querySelector('#contact');box.innerHTML='';for(let n=0;n<f.length;n+=step){{let x=document.createElement('img');x.src=f[n].screenshots[view.value];x.title=`frame ${{f[n].scenario_frame}} phase ${{f[n].gait_phase.toFixed(3)}}`;box.appendChild(x)}}}}
document.querySelectorAll('button').forEach(b=>b.onclick=()=>{{scenario=b.dataset.scenario;i=0;show();contacts();play()}});view.onchange=()=>{{show();contacts()}};rate.onchange=play;show();contacts();play();</script></body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_output(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "adventuresim-animation-viewer-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn report_invalidation_preserves_unrelated_files() {
        let output = unique_test_output("invalidate");
        fs::create_dir_all(&output).unwrap();
        for name in ["manifest.json", "index.html", "failure.txt", "notes.txt"] {
            fs::write(output.join(name), b"old").unwrap();
        }
        invalidate_previous_report(&output);
        assert!(!output.join("manifest.json").exists());
        assert!(!output.join("index.html").exists());
        assert!(!output.join("failure.txt").exists());
        assert!(output.join("notes.txt").exists());
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn capture_requires_every_nonempty_view_artifact() {
        let output = unique_test_output("artifacts");
        fs::create_dir_all(&output).unwrap();
        let screenshots = VIEWS
            .into_iter()
            .map(|view| (view.slug().to_owned(), format!("{}.png", view.slug())))
            .collect::<BTreeMap<_, _>>();
        let frame = FrameSample {
            scenario: "test".into(),
            scenario_frame: 0,
            time_seconds: 0.0,
            speed_metres_per_second: 0.0,
            gait_phase: 0.0,
            locomotion_sample_tick: 0,
            body_acceleration: Vec3::ZERO.to_array(),
            world_acceleration: Vec3::ZERO.to_array(),
            contact_sequence: 0,
            contact_foot: LeadFoot::Left,
            landing_sequence: 0,
            landing_impact_speed: 0.0,
            body_lean_pitch_degrees: 0.0,
            body_lean_roll_degrees: 0.0,
            landing_compression_metres: 0.0,
            root_distance_metres: 0.0,
            root_position_metres: Vec3::ZERO.to_array(),
            world_travel_direction: Vec3::Z.to_array(),
            desired_body_forward_direction: Vec3::Z.to_array(),
            body_forward_direction: Vec3::Z.to_array(),
            body_rotation_xyzw: Quat::IDENTITY.to_array(),
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
            action: SkeletonAction::None,
            action_phase: 0.0,
            attack_step: AttackStep::Stay,
            guard_action: false,
            left_support_weight: 1.0,
            right_support_weight: 1.0,
            desired_left_foot_target: None,
            desired_right_foot_target: None,
            ik_left_authored_target: None,
            ik_right_authored_target: None,
            ik_left_planned_contact: None,
            ik_right_planned_contact: None,
            ik_settle_capture_point: None,
            ik_left_solve_target: None,
            ik_right_solve_target: None,
            ik_left_support_weight: 0.0,
            ik_right_support_weight: 0.0,
            ik_left_release_active: false,
            ik_right_release_active: false,
            ik_left_release_target: None,
            ik_right_release_target: None,
            ik_settle_progress: None,
            attack_requested_left_foot_target: None,
            attack_requested_right_foot_target: None,
            attack_constrained_left_foot_target: None,
            attack_constrained_right_foot_target: None,
            attack_support_handoffs: 0,
            attack_maximum_reach_yield_metres: 0.0,
            screenshots,
            bones: BTreeMap::new(),
        };
        for name in frame.screenshots.values() {
            fs::write(output.join(name), b"png").unwrap();
        }
        assert!(capture_artifacts_written(
            &output,
            std::slice::from_ref(&frame)
        ));
        fs::write(output.join("front.png"), b"").unwrap();
        assert!(!capture_artifacts_written(&output, &[frame]));
        fs::remove_dir_all(output).unwrap();
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
        assert!(!scenario_uses_terrain_ik("steady-walk-2.0"));
        assert!(raised_scenario_requires_zero_flight("raised-guard-forward"));
        assert!(!raised_scenario_requires_zero_flight(
            "raised-guard-tap-stop-right"
        ));
        assert!(!raised_scenario_requires_zero_flight("cross-slope-walk"));
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
        let crouch = plan
            .iter()
            .filter(|frame| frame.scenario == "crouch-enter-exit")
            .collect::<Vec<_>>();
        assert!(crouch.iter().any(|frame| frame.crouching));
        assert!(!crouch.first().unwrap().crouching);
        assert!(!crouch.last().unwrap().crouching);
        assert!(
            plan.iter()
                .any(|frame| { frame.scenario == "steady-crouch-1.5" && frame.crouching })
        );
        for scenario in [
            "raised-guard-right-lead-left",
            "raised-guard-right-lead-right",
            "raised-guard-right-lead-forward-right",
            "raised-guard-right-lead-accelerate",
            "raised-guard-right-lead-release",
            "raised-guard-right-lead-reversal",
        ] {
            assert!(plan.iter().any(|frame| {
                frame.scenario == scenario
                    && frame.weapon_guard == WeaponGuardState::Raised
                    && frame.lead_foot == LeadFoot::Right
            }));
        }
    }

    #[test]
    fn raised_guard_uses_strict_plant_and_separation_gates() {
        assert_eq!(planted_drift_limit("raised-guard-right"), 0.01);
        assert_eq!(inter_foot_separation_limit("raised-guard-right"), 0.16);
        assert_eq!(planted_drift_limit("steady-walk-2.0"), 0.035);
        assert_eq!(inter_foot_separation_limit("steady-walk-2.0"), 0.08);
        assert!(!procedural_leg_solver_gates_apply("steady-walk-2.0"));
        assert!(!procedural_leg_solver_gates_apply("start-stop-transition"));
        assert!(procedural_leg_solver_gates_apply("cross-slope-walk"));
        assert!(procedural_leg_solver_gates_apply("raised-guard-forward"));
        for transition in [
            "raised-guard-release-at-peak",
            "raised-guard-right-lead-release",
            "raised-guard-left-right-reversal",
            "raised-guard-right-lead-reversal",
            "raised-guard-accelerate-from-rest",
            "terrain-tap-restart-crossfade",
            "terrain-speed-threshold-chatter",
        ] {
            assert!(!expects_loop_seam(transition));
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
                        crouching: false,
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
                assert_eq!(evaluation.base[0].pose, SemanticPose::GuardLeadLeft);
                assert_eq!(evaluation.base[0].sampling, PoseSampling::Anchor);
                phases.push(skeleton.gait_phase);
            }
            assert!(phases.windows(2).any(|pair| pair[1] < pair[0]));
            assert!(phases.iter().any(|&phase| phase >= 0.5));
        }
    }

    #[test]
    fn raised_guard_viewer_finishes_release_and_reverses_at_foot_handoff() {
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
                        crouching: false,
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
        assert!(
            release
                .iter()
                .any(|(frame, phase, intent)| *frame > 20 && *phase > 0.5 && intent.is_moving())
        );
        assert!(!release.last().unwrap().2.is_moving());
        assert_eq!(release.last().unwrap().1, 0.0);

        let reversal = replay("raised-guard-left-right-reversal");
        let changed = reversal
            .iter()
            .find(|(_, _, intent)| intent.local_direction() == Vec2::X)
            .expect("reversal is accepted at a foot handoff");
        assert!(changed.0 >= 16);
        assert!(changed.1 < 0.08 || (0.5..0.58).contains(&changed.1));
    }

    #[test]
    fn raised_guard_capture_uses_prepared_runtime_pose_assets() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in [
            "guard_walk_lead_left.glb",
            "guard_strafe_lead_left_left.glb",
            "guard_strafe_lead_left_right.glb",
        ] {
            let source = root.join("assets_src/biped/unarmed").join(name);
            let runtime = root.join("assets/animations/biped/unarmed").join(name);
            assert_eq!(fs::read(source).unwrap(), fs::read(runtime).unwrap());
        }
    }

    #[test]
    fn manifest_metrics_detect_crossed_feet_and_facing_mismatch() {
        let bone = |position: Vec3| BoneSample {
            position: position.to_array(),
            rotation_xyzw: Quat::IDENTITY.to_array(),
            terrain_clearance_metres: Some(0.0),
        };
        let bones = BTreeMap::from([
            ("left_hip".into(), bone(Vec3::new(-0.15, 1.0, 0.0))),
            ("left_knee".into(), bone(Vec3::new(-0.1, 0.5, 0.2))),
            ("left_foot".into(), bone(Vec3::new(0.12, 0.0, 0.0))),
            ("right_hip".into(), bone(Vec3::new(0.15, 1.0, 0.0))),
            ("right_knee".into(), bone(Vec3::new(0.1, 0.5, 0.2))),
            ("right_foot".into(), bone(Vec3::new(-0.12, 0.0, 0.0))),
        ]);
        let mut frame = FrameSample {
            scenario: "metric-test".into(),
            scenario_frame: 0,
            time_seconds: 0.0,
            speed_metres_per_second: 2.0,
            gait_phase: 0.0,
            locomotion_sample_tick: 0,
            body_acceleration: Vec3::ZERO.to_array(),
            world_acceleration: Vec3::ZERO.to_array(),
            contact_sequence: 0,
            contact_foot: LeadFoot::Left,
            landing_sequence: 0,
            landing_impact_speed: 0.0,
            body_lean_pitch_degrees: 0.0,
            body_lean_roll_degrees: 0.0,
            landing_compression_metres: 0.0,
            root_distance_metres: 0.0,
            root_position_metres: Vec3::ZERO.to_array(),
            world_travel_direction: Vec3::NEG_Z.to_array(),
            desired_body_forward_direction: Vec3::NEG_Z.to_array(),
            body_forward_direction: Vec3::Z.to_array(),
            body_rotation_xyzw: Quat::IDENTITY.to_array(),
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
            action: SkeletonAction::None,
            action_phase: 0.0,
            attack_step: AttackStep::Stay,
            guard_action: false,
            left_support_weight: 1.0,
            right_support_weight: 1.0,
            desired_left_foot_target: None,
            desired_right_foot_target: None,
            ik_left_authored_target: None,
            ik_right_authored_target: None,
            ik_left_planned_contact: None,
            ik_right_planned_contact: None,
            ik_settle_capture_point: None,
            ik_left_solve_target: None,
            ik_right_solve_target: None,
            ik_left_support_weight: 0.0,
            ik_right_support_weight: 0.0,
            ik_left_release_active: false,
            ik_right_release_active: false,
            ik_left_release_target: None,
            ik_right_release_target: None,
            ik_settle_progress: None,
            attack_requested_left_foot_target: None,
            attack_requested_right_foot_target: None,
            attack_constrained_left_foot_target: None,
            attack_constrained_right_foot_target: None,
            attack_support_handoffs: 0,
            attack_maximum_reach_yield_metres: 0.0,
            screenshots: BTreeMap::new(),
            bones,
        };
        let frames = [&frame];
        assert!(minimum_signed_foot_track(&frames) < 0.0);
        assert!(maximum_facing_error(&frames) > 179.0);

        let mut previous = frame.clone();
        previous.desired_body_forward_direction = Vec3::Z.to_array();
        frame.scenario_frame = 1;
        frame.time_seconds = 1.0 / SAMPLE_HZ;
        assert!(maximum_facing_tracking_excess(&[&previous, &frame]) > 8.0);

        frame.guard_action = true;
        assert!(maximum_guard_facing_error(&[&frame]) > 179.0);
    }

    #[test]
    fn hard_stop_height_gate_measures_only_the_transition_and_settle_window() {
        let frame = |scenario_frame: usize, speed: f32, pelvis_y: f32| {
            let mut frame = foot_metric_frame(scenario_frame, -0.1, 1.0, 0.0, 0.0);
            frame.scenario = "hard-stop".into();
            frame.speed_metres_per_second = speed;
            frame.bones.insert(
                "pelvis".into(),
                BoneSample {
                    position: (Vec3::Y * pelvis_y).to_array(),
                    rotation_xyzw: Quat::IDENTITY.to_array(),
                    terrain_clearance_metres: None,
                },
            );
            frame
        };
        let frames = vec![
            frame(0, 5.5, 0.0),
            frame(1, 5.5, 0.03),
            frame(2, 5.5, 0.04),
            frame(3, 0.0, 0.055),
            frame(4, 0.0, 0.065),
        ];
        assert!(
            (root_relative_vertical_step(&frames.iter().collect::<Vec<_>>(), "pelvis") - 0.03)
                .abs()
                < 0.0001
        );
        assert!((hard_stop_pelvis_vertical_step(&frames).unwrap() - 0.015).abs() < 0.0001);
        assert_eq!(hard_stop_pelvis_vertical_step(&[]), None);
    }

    fn foot_metric_frame(
        scenario_frame: usize,
        left_x: f32,
        left_support_weight: f32,
        left_terrain_height: f32,
        right_terrain_height: f32,
    ) -> FrameSample {
        let foot = |x: f32, terrain_height: f32| BoneSample {
            position: Vec3::new(x, terrain_height, 0.0).to_array(),
            rotation_xyzw: Quat::IDENTITY.to_array(),
            terrain_clearance_metres: Some(0.0),
        };
        FrameSample {
            scenario: "metric-test".into(),
            scenario_frame,
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            speed_metres_per_second: 2.0,
            gait_phase: 0.0,
            locomotion_sample_tick: scenario_frame as u64,
            body_acceleration: Vec3::ZERO.to_array(),
            world_acceleration: Vec3::ZERO.to_array(),
            contact_sequence: 0,
            contact_foot: LeadFoot::Left,
            landing_sequence: 0,
            landing_impact_speed: 0.0,
            body_lean_pitch_degrees: 0.0,
            body_lean_roll_degrees: 0.0,
            landing_compression_metres: 0.0,
            root_distance_metres: 0.0,
            root_position_metres: Vec3::new(0.0, CAPTURE_ROOT_GROUND_OFFSET_METRES, 0.0).to_array(),
            world_travel_direction: Vec3::NEG_Z.to_array(),
            desired_body_forward_direction: Vec3::NEG_Z.to_array(),
            body_forward_direction: Vec3::NEG_Z.to_array(),
            body_rotation_xyzw: Quat::IDENTITY.to_array(),
            weapon_guard: WeaponGuardState::Lowered,
            lead_foot: LeadFoot::Left,
            action: SkeletonAction::None,
            action_phase: 0.0,
            attack_step: AttackStep::Stay,
            guard_action: false,
            left_support_weight,
            right_support_weight: 0.0,
            desired_left_foot_target: None,
            desired_right_foot_target: None,
            ik_left_authored_target: None,
            ik_right_authored_target: None,
            ik_left_planned_contact: None,
            ik_right_planned_contact: None,
            ik_settle_capture_point: None,
            ik_left_solve_target: None,
            ik_right_solve_target: None,
            ik_left_support_weight: 0.0,
            ik_right_support_weight: 0.0,
            ik_left_release_active: false,
            ik_right_release_active: false,
            ik_left_release_target: None,
            ik_right_release_target: None,
            ik_settle_progress: None,
            attack_requested_left_foot_target: None,
            attack_requested_right_foot_target: None,
            attack_constrained_left_foot_target: None,
            attack_constrained_right_foot_target: None,
            attack_support_handoffs: 0,
            attack_maximum_reach_yield_metres: 0.0,
            screenshots: BTreeMap::new(),
            bones: BTreeMap::from([
                ("left_foot".into(), foot(left_x, left_terrain_height)),
                ("right_foot".into(), foot(0.2, right_terrain_height)),
            ]),
        }
    }

    #[test]
    fn planted_drift_excludes_the_deliberate_support_release_interval() {
        let frames = [
            foot_metric_frame(0, 0.0, 1.0, 0.0, 0.0),
            foot_metric_frame(1, 0.02, 1.0, 0.0, 0.0),
            foot_metric_frame(2, 0.20, 0.95, 0.0, 0.0),
        ];
        let references = frames.iter().collect::<Vec<_>>();

        assert!(
            (maximum_planted_foot_drift("cross-slope-walk", &references) - 0.02).abs() < 0.0001
        );
    }

    #[test]
    fn contact_clearance_uses_effective_ik_support_instead_of_gait_phase() {
        let mut frame = foot_metric_frame(0, 0.0, 1.0, 0.0, 0.0);
        frame.ik_left_solve_target = Some(Vec3::ZERO.to_array());
        frame.ik_right_solve_target = Some(Vec3::ZERO.to_array());
        frame.ik_left_support_weight = 0.0;
        frame.ik_right_support_weight = 1.0;
        frame
            .bones
            .get_mut("left_foot")
            .unwrap()
            .terrain_clearance_metres = Some(0.161);
        frame
            .bones
            .get_mut("right_foot")
            .unwrap()
            .terrain_clearance_metres = Some(0.085);

        // Gait phase zero identifies the left foot, but the solver reports it
        // airborne and the right foot supported. Only the supported sole is a
        // contact-clearance sample.
        assert_eq!(contact_sole_clearance_range(&[&frame]), (0.0, 0.0));
    }

    #[test]
    fn contact_clearance_falls_back_to_phase_without_procedural_targets() {
        let mut frame = foot_metric_frame(0, 0.0, 0.0, 0.0, 0.0);
        frame
            .bones
            .get_mut("left_foot")
            .unwrap()
            .terrain_clearance_metres = Some(0.095);
        frame
            .bones
            .get_mut("right_foot")
            .unwrap()
            .terrain_clearance_metres = Some(0.145);

        let (minimum, maximum) = contact_sole_clearance_range(&[&frame]);
        assert!((minimum - 0.01).abs() < 0.0001);
        assert!((maximum - 0.01).abs() < 0.0001);
    }

    #[test]
    fn viewer_rejects_reported_support_beyond_the_shared_contact_tolerance() {
        let mut frame = foot_metric_frame(0, 0.0, 0.0, 0.0, 0.0);
        frame.ik_left_support_weight = 1.0;
        frame
            .bones
            .get_mut("left_foot")
            .unwrap()
            .terrain_clearance_metres =
            Some(MEASURED_ANKLE_SOLE_OFFSET_METRES + SOLE_CONTACT_TOLERANCE_METRES - 0.00001);
        assert!(reported_support_contacts_are_valid(&[frame.clone()]));

        frame
            .bones
            .get_mut("left_foot")
            .unwrap()
            .terrain_clearance_metres =
            Some(MEASURED_ANKLE_SOLE_OFFSET_METRES + SOLE_CONTACT_TOLERANCE_METRES + 0.0001);
        assert!(!reported_support_contacts_are_valid(&[frame]));
    }

    #[test]
    fn continuity_bone_classes_separate_feet_from_knees() {
        assert!(is_foot_bone("left_foot"));
        assert!(is_foot_bone("right_foot"));
        assert!(!is_foot_bone("left_knee"));
        assert!(is_knee_bone("left_knee"));
        assert!(is_knee_bone("right_knee"));
        assert!(!is_knee_bone("right_hip"));
    }

    #[test]
    fn terrain_vertical_allowance_is_derived_from_sampled_foot_relief() {
        let frame = foot_metric_frame(0, 0.0, 1.0, -0.04, 0.04);

        assert!((foot_terrain_relief(&[&frame]) - 0.08).abs() < 0.0001);
        assert_eq!(
            vertical_range_limit("steady-walk", 0.08),
            ORDINARY_VERTICAL_RANGE_LIMIT_METRES
        );
        assert!((vertical_range_limit("cross-slope-walk", 0.08) - 0.28).abs() < 0.0001);
    }

    #[test]
    fn pelvis_vertical_step_detects_a_one_frame_guard_height_snap() {
        let mut first = foot_metric_frame(0, 0.0, 1.0, 0.0, 0.0);
        let mut second = foot_metric_frame(1, 0.0, 1.0, 0.0, 0.0);
        let pelvis = |height: f32| BoneSample {
            position: Vec3::new(0.0, height, 0.0).to_array(),
            rotation_xyzw: Quat::IDENTITY.to_array(),
            terrain_clearance_metres: None,
        };
        first.bones.insert("pelvis".into(), pelvis(1.0));
        second.bones.insert("pelvis".into(), pelvis(0.86));

        assert!((root_relative_vertical_step(&[&first, &second], "pelvis") - 0.14).abs() < 0.0001);
    }

    #[test]
    fn prominent_height_gate_requires_only_the_two_passing_peaks() {
        let mut clean = (0..64)
            .map(|sample| {
                let phase = sample as f32 / 64.0;
                let sine = (phase * std::f32::consts::TAU).sin();
                (phase, 0.04 * sine * sine)
            })
            .collect::<Vec<_>>();
        assert_eq!(prominent_height_peaks(&clean), (2, true));

        // A visible contact beat is a third gait-height peak even though the
        // intended passing peaks remain correctly positioned.
        clean[0].1 = 0.01;
        let (count, passing_windows) = prominent_height_peaks(&clean);
        assert!(passing_windows);
        assert_ne!(count, 2);
    }

    #[test]
    fn release_transition_is_not_misclassified_as_a_repeatable_loop() {
        assert!(!expects_loop_seam("raised-guard-release-at-peak"));
        assert!(expects_loop_seam("raised-guard-forward"));
        assert!(expects_loop_seam("steady-walk"));
    }
}
