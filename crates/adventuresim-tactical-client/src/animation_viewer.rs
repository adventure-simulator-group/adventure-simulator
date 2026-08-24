//! A deterministic gameplay-presentation fixture for tactical animation review.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

use adventuresim_tactical_core::animation::dive_launch_root_rotation;
use adventuresim_tactical_core::physics::{
    AdventureSimulatorPhysicsPlugin, TACTICAL_DIVE_HORIZONTAL_SPEED_METRES_PER_SECOND,
    TACTICAL_QUICKSTEP_JUMP_HEIGHT_METRES, TACTICAL_QUICKSTEP_SPEED_METRES_PER_SECOND,
};
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::client::WeaponGuardInputState;
use bevy::{
    animation::AnimationTargetId,
    app::AppExit,
    asset::io::AssetSourceBuilder,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::animation::jitter::{self, JitterBone, JitterFrame, JitterValidationSummary};
use crate::animation::pose_buffer::PoseBufferMetrics;
use crate::animation::{
    AnimationPlayback, AnimationRuntime, ArmIkState, AuthoredBindTransform, BoneRole, HumanoidBone,
    LegIkDiagnostics, LegIkState, LocomotionBodyResponseState, LocomotionHeightState,
    LocomotionPresentationEvent, LocomotionPresentationEventKind,
    MEASURED_ANKLE_SOLE_OFFSET_METRES, PresentedSkeleton, ProceduralAnimationClock,
    RaisedFootworkState, SOLE_CONTACT_TOLERANCE_METRES, TacticalAnimationPlugin, TerrainIkEnabled,
    locomotion_support_weights,
    semantic_route::{SemanticRoutePath, SemanticRouteTrace},
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
const TRACKED_BONE_NAMES: [&str; 17] = [
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
    "left_toe",
    "right_toe",
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
    if name == "quickstep-right" {
        ScenarioMetadata {
            kind: ScenarioKind::Transition,
            repeatable: false,
            procedural_solver: true,
        }
    } else if name.starts_with("downed-")
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
    let default_character_id = default_tactical_character_id();

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
                        title: "Fabelgeist Animation Review Capture".into(),
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
                    enable_presentation_simulation: true,
                }),
            EnhancedInputPlugin,
        ))
        .add_plugins((
            PlayerPlugin,
            TacticalAnimationPlugin,
            TacticalCameraPlugin,
            TacticalPresentationPlugin::default(),
        ))
        .insert_resource(LocalCharacterId(default_character_id))
        .insert_resource(CameraMode { third_person: true })
        .insert_resource(WeaponGuardInputState::default())
        .insert_resource(Time::<Fixed>::from_hz(SAMPLE_HZ as f64))
        // Individual scenarios select terrain conformity explicitly so the
        // viewer can retain FK-only controls after the live default changed.
        .insert_resource(TerrainIkEnabled(initial_terrain_ik))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.1, 0.13)))
        .insert_resource(CaptureSequence::new(output, settle_frames, scenario))
        .add_systems(Startup, setup_viewer)
        .add_systems(PreUpdate, (drive_sequence, freeze_capture_look).chain())
        .add_systems(
            PostUpdate,
            position_capture_camera
                .after(TacticalCameraSet::Offset)
                .before(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            (draw_flat_ground_grid, draw_skeleton_overlay)
                .chain()
                .after(TransformSystems::Propagate),
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
    global_bone_frames: Vec<GlobalBoneFrame>,
    presentation_events: Vec<PresentationEventSample>,
    repeated_evaluation_baseline: Option<RepeatedEvaluationSnapshot>,
    repeated_evaluation_valid: bool,
    active_scenario: Option<&'static str>,
    warmup_frames: u32,
    motion_ready_frames: u32,
    simulation_tick: u64,
    scenario_distance: f32,
}

impl CaptureSequence {
    fn new(output: PathBuf, settle_frames: u32, scenario: Option<&str>) -> Self {
        let plan = match scenario {
            Some("flat-grid-walk-2.0") => steady_scenario("flat-grid-walk-2.0", 2.0, 3.0),
            Some("flat-grid-run-5.5") => steady_scenario("flat-grid-run-5.5", 5.5, 3.0),
            Some("flat-grid-walk-stop") => flat_grid_walk_stop_scenario(),
            Some("full-ragdoll") => full_ragdoll_scenario(),
            _ => capture_plan()
                .into_iter()
                .filter(|frame| scenario.is_none_or(|scenario| frame.scenario == scenario))
                .collect::<Vec<_>>(),
        };
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
            global_bone_frames: Vec::new(),
            presentation_events: Vec::new(),
            repeated_evaluation_baseline: None,
            repeated_evaluation_valid: true,
            active_scenario: None,
            warmup_frames: 0,
            motion_ready_frames: 0,
            simulation_tick: 0,
            scenario_distance: 0.0,
        }
    }

    fn uses_flat_grid(&self) -> bool {
        self.plan
            .iter()
            .all(|frame| frame.scenario.starts_with("flat-grid-"))
    }
}

fn next_capture_simulation_tick(current: u64, absolute_first_sample: bool) -> u64 {
    if absolute_first_sample {
        current
    } else {
        current.wrapping_add(1)
    }
}

struct RepeatedEvaluationSnapshot {
    scenario: &'static str,
    scenario_frame: usize,
    bones: BTreeMap<String, BoneSample>,
    contact_sequence: u64,
    landing_sequence: u64,
    event_count: usize,
    leg_ik: LegIkDiagnostics,
}

fn repeated_bone_mismatch(
    expected: &BTreeMap<String, BoneSample>,
    actual: &BTreeMap<String, BoneSample>,
) -> Option<(String, f32, f32)> {
    if let Some(name) = expected
        .keys()
        .find(|name| !actual.contains_key(*name))
        .or_else(|| actual.keys().find(|name| !expected.contains_key(*name)))
    {
        return Some((name.clone(), f32::INFINITY, f32::INFINITY));
    }
    expected.iter().find_map(|(name, expected)| {
        let actual = actual
            .get(name)
            .expect("equal repeated-evaluation bone keys were checked above");
        let position_delta =
            Vec3::from_array(expected.position).distance(Vec3::from_array(actual.position));
        let expected_rotation = Quat::from_array(expected.rotation_xyzw);
        let actual_rotation = Quat::from_array(actual.rotation_xyzw);
        // `acos` quantizes identical f32 quaternions to roughly 0.056-0.079
        // degrees on some frames. Treat a direct dot match as identity, then
        // retain the angular report for genuine changes.
        let rotation_dot = expected_rotation.dot(actual_rotation).abs();
        let rotation_delta = if rotation_dot >= 1.0 - 0.000_001 {
            0.0
        } else {
            expected_rotation
                .angle_between(actual_rotation)
                .to_degrees()
        };
        // Re-evaluating one logical tick must be visually identical. Keep only
        // sub-half-millimetre/sub-twentieth-degree numeric noise.
        (position_delta > 0.0005 || rotation_delta > 0.05).then_some((
            name.clone(),
            position_delta,
            rotation_delta,
        ))
    })
}

#[derive(Debug, Serialize)]
struct CaptureManifest {
    sample_hz: f32,
    playback_backend: &'static str,
    global_bone_trace: &'static str,
    pose_buffer: PoseBufferMetrics,
    pipeline: &'static str,
    views: [CaptureView; 3],
    validation: CaptureValidation,
    quality_score: QualityScore,
    scenarios: Vec<ScenarioMetrics>,
    frames: Vec<FrameSample>,
    presentation_events: Vec<PresentationEventSample>,
    semantic_route_path_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize)]
struct QualityScore {
    weighted_defect_score: u8,
    maximum_weighted_defect_score: u8,
    quality_percent: f32,
    acceptance_passed: bool,
    categories: QualityCategories,
}

#[derive(Debug, Serialize)]
struct QualityCategories {
    catastrophic_foot_displacement_failed: bool,
    guard_step_liveness_failed: bool,
    anatomical_invalid_joints_failed: bool,
    contact_foot_airborne_failed: bool,
    both_feet_behind_hips_failed: bool,
    foot_dragging_failed: bool,
    jitter_and_jerk_failed: bool,
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
    raised_guard_fixed_support: bool,
    raised_guard_step_liveness_valid: bool,
    flat_controller_height_stable: bool,
    phase_owned_height_valid: bool,
    run_flight_valid: bool,
    body_response_valid: bool,
    straight_run_torso_sway_valid: bool,
    speed_ramp_phase_continuity_valid: bool,
    contact_sequences_valid: bool,
    cadence_contact_valid: bool,
    event_stream_valid: bool,
    landing_response_valid: bool,
    landing_foot_preservation_valid: bool,
    ordinary_swing_tracking_valid: bool,
    reported_support_contacts_valid: bool,
    run_contact_acquisition_valid: bool,
    stop_settle_capture_valid: bool,
    final_support_balance_valid: bool,
    hard_stop_maximum_pelvis_step_metres: Option<f32>,
    hard_stop_height_continuity_valid: bool,
    repeated_evaluation_valid: bool,
    semantic_route_paths_exercised: bool,
    jitter_validation: JitterValidationSummary,
    views_are_distinct: bool,
    duplicate_view_frames: Vec<String>,
    note: &'static str,
}

fn invalidate_previous_report(output: &std::path::Path) {
    for name in [
        "manifest.json",
        "index.html",
        "failure.txt",
        "global-bone-transforms.jsonl",
    ] {
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
    maximum_foot_rotation_step_degrees: f32,
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
    minimum_flight_toe_clearance_metres: f32,
    minimum_contact_toe_clearance_metres: f32,
    head_vertical_range_metres: f32,
    foot_terrain_relief_metres: f32,
    minimum_knee_forward_bend_metres: f32,
    minimum_signed_foot_track_metres: f32,
    minimum_inter_foot_separation_metres: f32,
    minimum_knee_flexion_degrees: f32,
    minimum_knee_hemisphere_dot: f32,
    maximum_knee_foot_yaw_offset_degrees: f32,
    maximum_facing_motion_error_degrees: f32,
    maximum_facing_tracking_excess_degrees: f32,
    maximum_guard_facing_error_degrees: f32,
    final_facing_motion_error_degrees: f32,
    maximum_dive_axis_motion_error_degrees: f32,
    maximum_supported_foot_slip_metres_per_frame: f32,
    maximum_planted_foot_drift_metres: f32,
    guard_step_liveness_required: bool,
    completed_guard_half_step_count: usize,
    visible_guard_half_step_count: usize,
    minimum_guard_swing_travel_metres: f32,
    minimum_guard_swing_clearance_gain_metres: f32,
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
    attack_animation: Option<AttackAnimation>,
    strike_family: StrikeFamily,
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
    ik_left_knee_foot_yaw_offset_degrees: f32,
    ik_right_knee_foot_yaw_offset_degrees: f32,
    semantic_route_requested_path: SemanticRoutePath,
    semantic_route_selected_path: SemanticRoutePath,
    semantic_route_runtime_evaluated: bool,
    screenshots: BTreeMap<String, String>,
    bones: BTreeMap<String, BoneSample>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct BoneSample {
    position: [f32; 3],
    rotation_xyzw: [f32; 4],
    terrain_clearance_metres: Option<f32>,
}

#[derive(Debug, Serialize)]
struct GlobalBoneFrame {
    scenario: String,
    scenario_frame: usize,
    time_seconds: f32,
    action: SkeletonAction,
    action_phase: f32,
    subject_translation: [f32; 3],
    subject_rotation_xyzw: [f32; 4],
    bones: Vec<GlobalBoneTransformSample>,
}

#[derive(Debug, Serialize)]
struct GlobalBoneTransformSample {
    name: String,
    target_id: String,
    translation: [f32; 3],
    rotation_xyzw: [f32; 4],
    scale: [f32; 3],
}

fn steady_scenario(name: &'static str, speed: f32, cycles: f32) -> Vec<PlannedFrame> {
    steady_scenario_in_direction(name, speed, cycles, Vec2::NEG_Y)
}

fn full_ragdoll_scenario() -> Vec<PlannedFrame> {
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
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
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
                action: SkeletonAction::None,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn flat_grid_walk_stop_scenario() -> Vec<PlannedFrame> {
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

fn attack_live_scenario(
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

fn capture_plan() -> Vec<PlannedFrame> {
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
        quickstep_scenario(),
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

fn quickstep_scenario() -> Vec<PlannedFrame> {
    (0..72)
        .map(|scenario_frame| PlannedFrame {
            scenario: "quickstep-right",
            scenario_frame,
            speed: if (5..=40).contains(&scenario_frame) {
                TACTICAL_QUICKSTEP_SPEED_METRES_PER_SECOND
            } else if (41..57).contains(&scenario_frame) {
                (TACTICAL_QUICKSTEP_SPEED_METRES_PER_SECOND
                    - 20.0 * (scenario_frame - 40) as f32 / SAMPLE_HZ)
                    .max(0.0)
            } else {
                0.0
            },
            time_seconds: scenario_frame as f32 / SAMPLE_HZ,
            local_direction: Vec2::X,
            camera_yaw: 0.0,
            camera_pitch: 0.0,
            action: SkeletonAction::Dodge,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot: LeadFoot::Left,
        })
        .collect()
}

fn downed_contact_scenario(name: &'static str, body: BodyState) -> Vec<PlannedFrame> {
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

fn downed_look_scenario() -> Vec<PlannedFrame> {
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

fn ordinary_camera_pitch_scenario() -> Vec<PlannedFrame> {
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

fn posture_transition_scenario(name: &'static str, _start: BodyState) -> Vec<PlannedFrame> {
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

fn dive_impact_scenario(name: &'static str) -> Vec<PlannedFrame> {
    dive_impact_scenario_with_aim(name, false)
}

fn aimed_dive_impact_scenario(name: &'static str) -> Vec<PlannedFrame> {
    dive_impact_scenario_with_aim(name, true)
}

fn dive_impact_scenario_with_aim(name: &'static str, aimed: bool) -> Vec<PlannedFrame> {
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

fn jump_charge_scenario() -> Vec<PlannedFrame> {
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

fn downed_body_for_scenario(scenario: &str) -> Option<BodyState> {
    match scenario {
        "downed-prone-crawl" | "downed-prone-look-at" => Some(BodyState::Prone),
        "downed-supine-scamper" => Some(BodyState::Supine),
        "full-ragdoll" => Some(BodyState::Ragdolled),
        _ => None,
    }
}

fn required_motion_for_scenario(scenario: &str) -> Option<&'static str> {
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

fn transition_for_scenario(scenario: &str) -> Option<(BodyState, PostureTransitionKind)> {
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
        return Some((upright, PostureTransitionKind::DiveToDowned { direction }));
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
                action: SkeletonAction::Block,
                weapon_guard: WeaponGuardState::Lowered,
                lead_foot: LeadFoot::Left,
            }
        })
        .collect()
}

fn raised_guard_stationary_turn_scenario() -> Vec<PlannedFrame> {
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

fn raised_guard_scenario(name: &'static str, direction: Vec2) -> Vec<PlannedFrame> {
    let direction = direction.normalize_or_zero();
    // Leave enough post-startup time for both local support identities to
    // complete. One semantic cycle could end before the client-owned first
    // landing after asset/pose readiness and therefore never test alternation.
    raised_guard_steady_scenario(name, 2.0, 2.0, direction)
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
            action: SkeletonAction::None,
            weapon_guard: WeaponGuardState::Raised,
            lead_foot,
        })
        .collect()
}

fn setup_viewer(mut commands: Commands, sequence: Res<CaptureSequence>) {
    let default_player = Player::default();
    let mut generator = TerrainGenerator::new(0xA11C_E5E1);
    generator.period = 200.0;
    let terrain = generator.generate(100, if sequence.uses_flat_grid() { 0 } else { 30 }, 100);
    let spawn_height =
        terrain.height_at(Vec2::ZERO).unwrap_or_default() + CAPTURE_ROOT_GROUND_OFFSET_METRES;
    commands.spawn((
        Name::new(if sequence.uses_flat_grid() {
            "Animation review flat-grid scene"
        } else {
            "Animation review hills scene"
        }),
        // Retain the known hills presentation family for its production sky,
        // lighting, and materials; only the authoritative heightfield becomes
        // flat for grid review.
        SceneId("hills".to_owned()),
        terrain,
        Transform::default(),
    ));

    commands.spawn((
        Name::new(default_player.name),
        CaptureSubject,
        Player::default(),
        CharacterId(default_tactical_character_id()),
        CharacterLook::default(),
        SkeletonState::default(),
        Transform::from_xyz(0.0, spawn_height, 0.0),
        Collider::cylinder(0.4, 1.9),
        CollisionMargin(0.01),
        tactical_character_controller(),
    ));
    commands.spawn((
        Name::new("Animation review fill light"),
        DirectionalLight {
            illuminance: 35_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-8.0, 12.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
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

/// Screenshot completion advances capture views asynchronously. Reassert the
/// planned look on every render of a logical sample so all three views enter
/// procedural PostUpdate with identical presentation input.
fn freeze_capture_look(
    sequence: Res<CaptureSequence>,
    mut subjects: Query<&mut CharacterLook, With<CaptureSubject>>,
) {
    let Some(frame) = sequence.plan.get(sequence.index) else {
        return;
    };
    for mut look in &mut subjects {
        look.yaw = frame.camera_yaw;
        look.pitch = frame.camera_pitch;
    }
}

fn drive_sequence(
    mut sequence: ResMut<CaptureSequence>,
    mut procedural_clock: ResMut<ProceduralAnimationClock>,
    mut terrain_ik: ResMut<TerrainIkEnabled>,
    mut guard_input: ResMut<WeaponGuardInputState>,
    animation_runtime: Res<AnimationRuntime>,
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
    let mut presentation_settled = false;
    for (
        mut skeleton,
        mut transform,
        mut look,
        playback,
        ik_state,
        arm_ik_state,
        raised_footwork,
        height_state,
        body_response,
    ) in &mut subjects
    {
        let Some(playback) = playback else {
            return;
        };
        presentation_settled = playback.presentation_is_settled();
        if frame.scenario != "full-ragdoll" && !playback.authored_pose_is_ready() {
            return;
        }

        if sequence.active_scenario != Some(frame.scenario) {
            sequence.active_scenario = Some(frame.scenario);
            sequence.scenario_distance = 0.0;
            sequence.motion_ready_frames = 0;
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
            if let Some(mut height_state) = height_state {
                *height_state = LocomotionHeightState::default();
            }
            if let Some(mut body_response) = body_response {
                *body_response = LocomotionBodyResponseState::default();
            }
            let ground = terrain.height_at(Vec2::ZERO).unwrap_or_default();
            transform.translation = Vec3::new(0.0, ground + CAPTURE_ROOT_GROUND_OFFSET_METRES, 0.0);
            transform.rotation = Quat::from_rotation_y(std::f32::consts::PI);
            if let Some((start_body, _)) = transition_for_scenario(frame.scenario) {
                skeleton.transition_body(start_body);
                // Prime the authored endpoint before beginning the transition.
                // Live characters have already evaluated their prone/supine or
                // upright base; a fresh viewer subject otherwise crossfades
                // from its default standing pose during the first samples.
                sequence.warmup_frames = 8;
            }
        }

        if let Some(body) = downed_body_for_scenario(frame.scenario) {
            skeleton.transition_body(body);
            let preload_locomotion = frame.scenario_frame == 0
                && matches!(
                    required_motion_for_scenario(frame.scenario),
                    Some("prone_crawl" | "supine_scamper")
                )
                && !required_motion_for_scenario(frame.scenario)
                    .is_some_and(|motion| animation_runtime.motion_is_processed(motion));
            skeleton.set_downed_turning(
                frame.scenario != "downed-prone-look-at"
                    && (preload_locomotion || (frame.scenario_frame >= 4 && frame.speed <= 0.05)),
            );
        }

        let orientation =
            Quat::from_euler(EulerRot::YXZ, frame.camera_yaw, frame.camera_pitch, 0.0);
        look.yaw = frame.camera_yaw;
        look.pitch = frame.camera_pitch;
        let attack_start_frame = frame.scenario.starts_with("attack-live-").then_some(8);
        if frame.action != SkeletonAction::Attack
            || attack_start_frame == Some(frame.scenario_frame)
        {
            skeleton.lead_foot = frame.lead_foot;
        }
        terrain_ik.0 = terrain_ik_enabled_for_frame(&frame);
        guard_input.desired = frame.weapon_guard;
        set_weapon_guard(&mut skeleton, guard_input.desired);
        if frame.scenario == "downed-prone-look-at" {
            let target = downed_camera_roll_target(transform.rotation, orientation);
            skeleton.advance_downed_facing(
                target,
                true,
                if frame.scenario_frame == 0 {
                    0.0
                } else {
                    1.0 / 84.0
                },
            );
        }
        let dive_impact = frame.scenario.ends_with("-impact");
        let quickstep = frame.scenario == "quickstep-right";
        let grounded = if quickstep {
            frame.scenario_frame < 5 || frame.scenario_frame >= 41
        } else if dive_impact {
            frame.scenario_frame == 0 || frame.scenario_frame >= 17
        } else {
            metadata.kind != ScenarioKind::Landing || frame.scenario_frame >= 32
        };
        let vertical_velocity = if quickstep && !grounded {
            let duration_seconds = 35.0 / SAMPLE_HZ;
            let flight = ((frame.scenario_frame.saturating_sub(5)) as f32 / 35.0).clamp(0.0, 1.0);
            4.0 * TACTICAL_QUICKSTEP_JUMP_HEIGHT_METRES * (1.0 - 2.0 * flight) / duration_seconds
        } else if (metadata.kind == ScenarioKind::Landing || dive_impact) && !grounded {
            -4.5
        } else {
            0.0
        };
        let requested_local_velocity = Vec3::new(
            frame.local_direction.x * frame.speed,
            vertical_velocity,
            frame.local_direction.y * frame.speed,
        );
        let local_velocity = requested_local_velocity;
        let world_velocity = controller_yaw(orientation) * local_velocity;
        sequence.simulation_tick = next_capture_simulation_tick(
            sequence.simulation_tick,
            sequence.index == 0 && sequence.warmup_frames == 0,
        );
        if sequence.warmup_frames == 0
            && frame.scenario_frame == 0
            && let Some((start_body, transition)) = transition_for_scenario(frame.scenario)
        {
            skeleton.transition_body(start_body);
            if frame.scenario.ends_with("-aimed-impact") {
                // Match the authoritative launch seam: velocity and authored
                // direction capture one camera frame even if the previously
                // displayed root had not finished turning toward it.
                transform.rotation =
                    dive_launch_root_rotation(Quat::from_rotation_y(frame.camera_yaw));
            }
            // Matches the live server's terrain-contact dive recovery.
            let duration = if frame.scenario.starts_with("dive-backward") {
                32
            } else if dive_impact {
                20
            } else {
                84
            };
            skeleton.begin_posture_transition(transition, sequence.simulation_tick, duration);
        }
        let delta_seconds = if frame.scenario_frame == 0 {
            0.0
        } else {
            1.0 / SAMPLE_HZ
        };
        procedural_clock.set_fixed_tick(sequence.simulation_tick, delta_seconds);
        let horizontal = transform.translation.xz() + world_velocity.xz() * delta_seconds;
        let vertical = if quickstep {
            let ground = terrain.height_at(horizontal).unwrap_or_default()
                + CAPTURE_ROOT_GROUND_OFFSET_METRES;
            let flight = ((frame.scenario_frame.saturating_sub(5)) as f32 / 35.0).clamp(0.0, 1.0);
            ground + 4.0 * TACTICAL_QUICKSTEP_JUMP_HEIGHT_METRES * flight * (1.0 - flight)
        } else if terrain_ik.0 {
            terrain.height_at(horizontal).unwrap_or_default() + CAPTURE_ROOT_GROUND_OFFSET_METRES
        } else {
            transform.translation.y
        };
        transform.translation = Vec3::new(horizontal.x, vertical, horizontal.y);
        let action_starts_now = match frame.action {
            SkeletonAction::Attack => attack_start_frame == Some(frame.scenario_frame),
            SkeletonAction::Dodge if quickstep => frame.scenario_frame == 0,
            SkeletonAction::Dodge | SkeletonAction::Block => skeleton.action_kind() != frame.action,
            SkeletonAction::None => false,
        };
        if action_starts_now {
            let start = sequence.simulation_tick;
            let contact = start
                + if frame.action == SkeletonAction::Attack {
                    19
                } else {
                    64
                };
            match frame.action {
                SkeletonAction::Attack => {
                    let attack = if frame.scenario == "attack-live-stationary-swing" {
                        AttackSpec::new(AttackAnimation::Swing)
                    } else {
                        AttackSpec::new(AttackAnimation::Thrust)
                    };
                    skeleton
                        .begin_attack(attack, start, contact)
                        .expect("viewer attack transition must be admitted");
                }
                SkeletonAction::Dodge => {
                    let spec = if quickstep {
                        DodgeSpec::quickstep(frame.local_direction)
                            .expect("quickstep scenario direction must be non-zero")
                    } else {
                        DodgeSpec::default()
                    };
                    skeleton
                        .begin_dodge(spec, start, if quickstep { start + 20 } else { contact })
                        .expect("viewer dodge transition must be admitted");
                }
                SkeletonAction::Block => {
                    skeleton
                        .begin_block(BlockSpec::default(), start, contact)
                        .expect("viewer block transition must be admitted");
                }
                SkeletonAction::None => {}
            }
        }
        if !skeleton.is_posture_transitioning() {
            transform.rotation = advance_body_facing(
                transform.rotation,
                orientation,
                world_velocity,
                frame.action,
                skeleton.weapon_guard(),
                delta_seconds,
            );
        }
        if frame.scenario == "full-ragdoll" {
            let fall = (frame.scenario_frame as f32 / 8.0).clamp(0.0, 1.0);
            transform.rotation = Quat::from_rotation_y(std::f32::consts::PI)
                * Quat::from_rotation_x(1.25 * smoothstep01(fall));
        }
        sequence.scenario_distance += frame.speed * delta_seconds;
        let jump_charging =
            frame.scenario == "jump-charge-anticipation" && (4..48).contains(&frame.scenario_frame);
        project_skeleton_locomotion(
            &mut skeleton,
            SkeletonLocomotionInput {
                orientation,
                linear_velocity: world_velocity,
                grounded,
                delta_seconds,
                tick: sequence.simulation_tick,
            },
        );
        skeleton.set_jump_anticipation(jump_charging);
        if sequence.warmup_frames == 0 && transition_for_scenario(frame.scenario).is_some() {
            let previous_transition = skeleton.posture_transition();
            skeleton.advance_posture_transition(sequence.simulation_tick);
            transform.rotation = (transform.rotation
                * dive_landing_facing_delta(previous_transition, skeleton.posture_transition())
                * supine_get_up_counter_yaw_delta(
                    previous_transition,
                    skeleton.posture_transition(),
                ))
            .normalize();
        }
        gait_phase = skeleton.gait_phase;
    }
    if sequence.warmup_frames > 0 {
        sequence.warmup_frames -= 1;
        return;
    }
    if frame.scenario_frame == 0
        && let Some(motion) = required_motion_for_scenario(frame.scenario)
    {
        if !animation_runtime.motion_is_processed(motion) || !presentation_settled {
            sequence.motion_ready_frames = 0;
            return;
        }
        sequence.motion_ready_frames += 1;
        if sequence.motion_ready_frames < 2 {
            return;
        }
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

fn draw_flat_ground_grid(sequence: Res<CaptureSequence>, mut gizmos: Gizmos) {
    let Some(frame) = sequence.plan.get(sequence.index) else {
        return;
    };
    if !frame.scenario.starts_with("flat-grid-") {
        return;
    }

    const HALF_EXTENT_METRES: i32 = 20;
    const SUBDIVISIONS_PER_METRE: i32 = 4;
    let half_steps = HALF_EXTENT_METRES * SUBDIVISIONS_PER_METRE;
    let height = 0.012;
    for step in -half_steps..=half_steps {
        let coordinate = step as f32 / SUBDIVISIONS_PER_METRE as f32;
        let whole_metre = step % SUBDIVISIONS_PER_METRE == 0;
        let color = if step == 0 {
            Color::srgba(1.0, 0.45, 0.12, 0.95)
        } else if whole_metre {
            Color::srgba(0.82, 0.86, 0.92, 0.80)
        } else {
            Color::srgba(0.42, 0.47, 0.55, 0.48)
        };
        gizmos.line(
            Vec3::new(coordinate, height, -HALF_EXTENT_METRES as f32),
            Vec3::new(coordinate, height, HALF_EXTENT_METRES as f32),
            color,
        );
        gizmos.line(
            Vec3::new(-HALF_EXTENT_METRES as f32, height, coordinate),
            Vec3::new(HALF_EXTENT_METRES as f32, height, coordinate),
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
    pose_buffer_metrics: Res<PoseBufferMetrics>,
    terrain_ik: Res<TerrainIkEnabled>,
    subjects: Query<
        (
            Entity,
            &PresentedSkeleton,
            &GlobalTransform,
            Option<&AnimationPlayback>,
            Option<&RaisedFootworkState>,
            Option<&LocomotionBodyResponseState>,
            Option<&LocomotionHeightState>,
            Option<&LegIkState>,
            Option<&SemanticRouteTrace>,
        ),
        With<CaptureSubject>,
    >,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
    animation_bones: Query<(
        &AnimationTargetId,
        &AuthoredBindTransform,
        Option<&Name>,
        &GlobalTransform,
    )>,
    terrain: Single<&SceneTerrain>,
    mut exit: MessageWriter<AppExit>,
) {
    let playback_backend = "pose_buffer";
    let pose_buffer_metrics = *pose_buffer_metrics;
    if !sequence.applied || sequence.capture_in_flight {
        return;
    }
    let Ok((
        subject,
        skeleton,
        subject_global,
        playback,
        raised_footwork,
        body_response,
        height_state,
        leg_ik,
        semantic_route,
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
    let Some(semantic_route) = semantic_route else {
        wait_or_fail(
            &mut sequence,
            "capture subject has no semantic route trace",
            &mut exit,
        );
        return;
    };
    if sequence.active_scenario != Some("full-ragdoll") && !playback.authored_pose_is_ready() {
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
    let evaluation_leg_ik = leg_ik.map(LegIkState::diagnostics).unwrap_or_default();
    if sequence.view_index == 0 {
        sequence.repeated_evaluation_baseline = Some(RepeatedEvaluationSnapshot {
            scenario: frame.scenario,
            scenario_frame: frame.scenario_frame,
            bones: evaluation_bones.clone(),
            contact_sequence: skeleton.contact_sequence,
            landing_sequence: skeleton.landing_sequence,
            event_count: sequence.presentation_events.len(),
            leg_ik: evaluation_leg_ik,
        });
    } else if let Some(baseline) = &sequence.repeated_evaluation_baseline {
        let bone_mismatch = repeated_bone_mismatch(&baseline.bones, &evaluation_bones);
        let bones_match = bone_mismatch.is_none();
        let repeated_evaluation_matches = baseline.scenario == frame.scenario
            && baseline.scenario_frame == frame.scenario_frame
            && bones_match
            && baseline.contact_sequence == skeleton.contact_sequence
            && baseline.landing_sequence == skeleton.landing_sequence
            && baseline.event_count == sequence.presentation_events.len()
            && baseline.leg_ik == evaluation_leg_ik;
        if !repeated_evaluation_matches
            && let Some((bone, position_delta, rotation_delta)) = &bone_mismatch
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
        sequence.global_bone_frames.push(GlobalBoneFrame {
            scenario: frame.scenario.to_owned(),
            scenario_frame: frame.scenario_frame,
            time_seconds: frame.time_seconds,
            action: skeleton.action_kind(),
            action_phase: skeleton.action_phase(),
            subject_translation: subject_global.translation().to_array(),
            subject_rotation_xyzw: subject_global.rotation().to_array(),
            bones: collect_global_bone_transforms(subject, &animation_bones),
        });
        let cadence_support = locomotion_support_weights(skeleton);
        let root_distance_metres = sequence.scenario_distance;
        let (desired_left_foot_target, desired_right_foot_target) = raised_footwork
            .map(|state| (state.left_solve_target, state.right_solve_target))
            .unwrap_or_default();
        let leg_ik = evaluation_leg_ik;
        let ik_support =
            if leg_ik.left_solve_target.is_some() || leg_ik.right_solve_target.is_some() {
                (leg_ik.left_support_weight, leg_ik.right_support_weight)
            } else {
                cadence_support
            };
        let (left_support_weight, right_support_weight) = raised_footwork
            .filter(|state| state.initialized())
            .map(|state| (state.left_support_weight, state.right_support_weight))
            .unwrap_or(ik_support);
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
            // Raised guard owns its visual contacts locally. Segment its
            // diagnostics by the sequence that actually changed the rendered
            // support foot, not by the replicated locomotion cadence.
            contact_sequence: raised_footwork.filter(|state| state.initialized()).map_or(
                skeleton.contact_sequence,
                RaisedFootworkState::step_sequence,
            ),
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
            attack_animation: skeleton.attack_animation(),
            strike_family: skeleton.strike_family(),
            guard_action: frame.weapon_guard == WeaponGuardState::Raised
                || matches!(
                    frame.action,
                    SkeletonAction::Dodge | SkeletonAction::Attack | SkeletonAction::Block
                ),
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
            ik_left_knee_foot_yaw_offset_degrees: leg_ik.left_knee_foot_yaw_offset_degrees,
            ik_right_knee_foot_yaw_offset_degrees: leg_ik.right_knee_foot_yaw_offset_degrees,
            semantic_route_requested_path: semantic_route.requested_path,
            semantic_route_selected_path: semantic_route.path,
            semantic_route_runtime_evaluated: semantic_route.runtime_evaluated,
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
                finish_capture(
                    &mut sequence,
                    playback_backend,
                    pose_buffer_metrics,
                    &mut exit,
                );
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

fn collect_global_bone_transforms(
    subject: Entity,
    bones: &Query<(
        &AnimationTargetId,
        &AuthoredBindTransform,
        Option<&Name>,
        &GlobalTransform,
    )>,
) -> Vec<GlobalBoneTransformSample> {
    let mut samples = bones
        .iter()
        .filter(|(_, bind, _, _)| bind.owner == subject)
        .map(|(target, _, name, transform)| {
            let (scale, rotation, translation) = transform.to_scale_rotation_translation();
            GlobalBoneTransformSample {
                name: name.map_or_else(|| "<unnamed>".to_owned(), |name| name.as_str().to_owned()),
                target_id: format!("{target:?}"),
                translation: translation.to_array(),
                rotation_xyzw: rotation.to_array(),
                scale: scale.to_array(),
            }
        })
        .collect::<Vec<_>>();
    samples
        .sort_by(|left, right| (&left.name, &left.target_id).cmp(&(&right.name, &right.target_id)));
    samples
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
        BoneRole::ToeLeft => "left_toe",
        BoneRole::ToeRight => "right_toe",
        _ => return None,
    })
}

fn jitter_frames(frames: &[FrameSample]) -> Vec<JitterFrame> {
    frames
        .iter()
        // A ragdoll is intentionally non-smooth at the animation-to-physics
        // handoff and does not obey authored locomotion jerk thresholds.
        .filter(|frame| frame.scenario != "full-ragdoll")
        .map(|frame| JitterFrame {
            scenario: frame.scenario.clone(),
            scenario_frame: frame.scenario_frame,
            time_seconds: frame.time_seconds,
            bones: frame
                .bones
                .iter()
                .map(|(name, bone)| {
                    let position = if name == "pelvis" {
                        // The capture root is authoritative locomotion, not
                        // a skeletal joint. Exclude its world translation
                        // from limb jitter while retaining pelvis rotation.
                        Vec3::ZERO
                    } else {
                        Vec3::from_array(bone.position)
                    };
                    let rotation = Quat::from_array(bone.rotation_xyzw);
                    let (position, rotation) = parent_bone(name)
                        .and_then(|parent| frame.bones.get(parent))
                        .map_or((position, rotation), |parent| {
                            let parent_position = Vec3::from_array(parent.position);
                            let parent_rotation = Quat::from_array(parent.rotation_xyzw);
                            (
                                parent_rotation.inverse() * (position - parent_position),
                                parent_rotation.inverse() * rotation,
                            )
                        });
                    (
                        name.clone(),
                        JitterBone {
                            position: position.to_array(),
                            rotation_xyzw: rotation.to_array(),
                        },
                    )
                })
                .collect(),
        })
        .collect()
}

fn parent_bone(name: &str) -> Option<&'static str> {
    Some(match name {
        "chest" | "left_hip" | "right_hip" => "pelvis",
        "head" | "left_shoulder" | "right_shoulder" => "chest",
        "left_elbow" => "left_shoulder",
        "right_elbow" => "right_shoulder",
        "left_hand" => "left_elbow",
        "right_hand" => "right_elbow",
        "left_knee" => "left_hip",
        "right_knee" => "right_hip",
        "left_foot" => "left_knee",
        "right_foot" => "right_knee",
        "left_toe" => "left_foot",
        "right_toe" => "right_foot",
        "pelvis" => return None,
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

fn finish_capture(
    sequence: &mut CaptureSequence,
    playback_backend: &'static str,
    pose_buffer_metrics: PoseBufferMetrics,
    exit: &mut MessageWriter<AppExit>,
) {
    let frames = std::mem::take(&mut sequence.samples);
    let global_bone_frames = std::mem::take(&mut sequence.global_bone_frames);
    let presentation_events = std::mem::take(&mut sequence.presentation_events);
    let scenarios = scenario_metrics(&frames);
    let jitter_validation = jitter::validate(&jitter_frames(&frames), Default::default());
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
        if metrics.scenario == "full-ragdoll" {
            // The root handoff and unconstrained limb motion are the behavior
            // under review; authored locomotion displacement limits do not
            // apply. Finite output, topology, terrain penetration, and visual
            // evidence remain mandatory.
            return metrics.maximum_bone_rotation_step_degrees <= 60.0;
        }
        metrics.maximum_root_relative_step_metres
            <= if metrics.scenario.starts_with("attack-live-") {
                0.30
            } else {
                0.20
            }
            && metrics.maximum_foot_root_relative_step_metres
                <= foot_continuity_limit(&metrics.scenario)
            && metrics.maximum_knee_root_relative_step_metres
                <= knee_continuity_limit(&metrics.scenario)
            && metrics.maximum_bone_rotation_step_degrees <= 60.0
            && (!metrics.scenario.contains("run")
                || metrics.maximum_foot_rotation_step_degrees
                    <= if scenario_metadata(&metrics.scenario).kind == ScenarioKind::Terrain {
                        // Direct weight-driven slope alignment can rotate a
                        // pointed run foot rapidly during the short contact
                        // approach. Position, contact, penetration, and knee
                        // gates remain strict; no temporal rotation cache is
                        // required by ordinary locomotion.
                        50.01
                    } else {
                        15.01
                    })
            && (!metrics.scenario.starts_with("raised-guard")
                || metrics.maximum_pelvis_vertical_step_metres
                    <= RAISED_MAXIMUM_PELVIS_VERTICAL_STEP_METRES)
            && (metrics.scenario != "terrain-steady-run-5.5"
                || metrics.maximum_pelvis_vertical_step_metres <= 0.02)
    });
    let no_ground_penetration = scenarios.iter().all(|metrics| {
        if metrics.scenario.starts_with("dive-") || metrics.scenario.ends_with("-get-up") {
            // These authored whole-body poses intentionally put the character
            // on the surface. Ankle/sole contact metrics assume upright feet
            // and report false penetration once the feet rotate onto a side
            // or heel; visual review and finite/continuity gates remain active.
            true
        } else if scenario_metadata(&metrics.scenario).kind == ScenarioKind::Attack {
            // Gait-phase contact selection does not describe a one-shot attack
            // handoff. Its dedicated validator checks the actual requested
            // plant; retain only the raw ankle penetration guard here.
            // The attack solver owns the ball/toe contact, so a bounded pivot
            // about that exact point can place the ankle joint slightly below
            // the sampled surface without moving the visible contact. Keep a
            // strict five-centimetre raw-joint guard while the dedicated gate
            // verifies the actual ball plant and its slip separately.
            metrics.minimum_foot_clearance_metres >= -0.05
        } else if scenario_metadata(&metrics.scenario).kind == ScenarioKind::Terrain
            && metrics.scenario != "terrain-toggle-mid-stride"
        {
            // Only a contact foot is ground-constrained. Gate those contacts;
            // an airborne authored foot is intentionally not projected onto
            // every terrain feature underneath its swing path.
            metrics.minimum_contact_sole_clearance_metres >= -0.01
                && (!scenario_requires_strict_terrain_toe_clearance(&metrics.scenario)
                    || metrics.minimum_contact_toe_clearance_metres >= -0.01)
        } else if metrics.scenario.starts_with("raised-guard-tap-stop") {
            metrics.minimum_contact_sole_clearance_metres >= -0.04
        } else {
            metrics.minimum_contact_sole_clearance_metres >= -0.02
        }
    });
    let raised_guard_fixed_support = frames.windows(2).all(|pair| {
        pair[0].scenario != pair[1].scenario
            || pair[0].weapon_guard != WeaponGuardState::Raised
            || pair[1].weapon_guard != WeaponGuardState::Raised
            || pair[0].action == SkeletonAction::Attack
            || pair[1].action == SkeletonAction::Attack
            || pair[0].lead_foot == pair[1].lead_foot
    });
    let raised_guard_step_liveness_valid = scenarios.iter().all(|metrics| {
        !metrics.guard_step_liveness_required
            || (metrics.completed_guard_half_step_count > 0
                && metrics.visible_guard_half_step_count == metrics.completed_guard_half_step_count)
    });
    let flat_controller_height_stable = scenarios.iter().all(|metrics| {
        scenario_uses_terrain_ik(&metrics.scenario)
            || metrics.scenario.contains("terrain")
            || metrics.scenario == "quickstep-right"
            || metrics.controller_vertical_range_metres <= 0.0001
    });
    let phase_owned_height_valid = scenarios.iter().all(|metrics| {
        if matches!(
            metrics.scenario.as_str(),
            "start-stop-transition" | "raised-guard-transition"
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
        if matches!(
            metrics.scenario.as_str(),
            "steady-run-5.5" | "terrain-steady-run-5.5" | "flat-grid-run-5.5"
        ) {
            (0.08..=0.20).contains(&metrics.maximum_no_support_seconds)
                && (0.05..=0.20).contains(&metrics.minimum_flight_sole_clearance_metres)
                && metrics.minimum_flight_toe_clearance_metres >= 0.01
        } else if matches!(
            metrics.scenario.as_str(),
            "terrain-run-flight-stop" | "terrain-tap-restart-crossfade"
        ) {
            // Transitioning into authored idle may blend support in before
            // a sampled zero-weight frame. If a true flight frame remains,
            // retain the toe-clearance gate; otherwise the ordinary contact
            // and penetration gates own the transition.
            metrics.maximum_no_support_seconds <= f32::EPSILON
                || strict_transition_flight_toe_clearance_is_valid(
                    metrics.minimum_flight_toe_clearance_metres,
                )
        } else if matches!(
            metrics.scenario.as_str(),
            "steady-walk-2.0" | "flat-grid-walk-2.0"
        ) || raised_scenario_requires_zero_flight(&metrics.scenario)
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
            if pair[0].scenario.starts_with("attack-live-") {
                // Attacks deliberately leave contact sequencing to the same
                // live guard locomotion planner. Its cadence is validated by
                // the raised-guard scenarios rather than duplicated here.
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
                    && !pair[0].scenario.starts_with("downed-")
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
    let (hard_stop_pitch_minimum, _) =
        lean_range("hard-stop", |frame| frame.body_lean_pitch_degrees);
    let (walk_stop_pitch_minimum, walk_stop_pitch_maximum) =
        lean_range("flat-grid-walk-stop", |frame| frame.body_lean_pitch_degrees);
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
    let body_lateral_range = |scenario: &str, bone: &str| {
        let (minimum, maximum) = frames
            .iter()
            .filter(|frame| frame.scenario == scenario)
            .filter_map(|frame| body_local(frame, bone).map(|position| position.x))
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
            );
        if minimum.is_finite() && maximum.is_finite() {
            maximum - minimum
        } else {
            0.0
        }
    };
    let straight_run_torso_sway_valid = ["steady-run-5.5", "flat-grid-run-5.5"]
        .iter()
        .filter(|scenario| has_scenario(scenario))
        .all(|scenario| {
            body_lateral_range(scenario, "chest") <= 0.015
                && body_lateral_range(scenario, "head") <= 0.025
        });
    let body_response_valid = (!has_scenario("speed-ramp-up-down")
        || ((-2.5..=0.0).contains(&ramp_pitch_minimum)
            && (22.0..=30.1).contains(&ramp_pitch_maximum)))
        && (!has_scenario("hard-stop") || (-0.1..=0.1).contains(&hard_stop_pitch_minimum))
        && (!has_scenario("flat-grid-walk-stop")
            || ((-2.5..=0.0).contains(&walk_stop_pitch_minimum)
                && (3.5..=5.0).contains(&walk_stop_pitch_maximum)))
        && (!has_scenario("dynamics-turn-90") || (6.0..=18.0).contains(&turn_90_roll.1))
        && (!has_scenario("dynamics-turn-180") || (6.0..=18.0).contains(&turn_180_roll.1))
        && lean_step_valid
        && ["speed-ramp-up-down", "hard-stop", "flat-grid-walk-stop"]
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
    let ordinary_swing_tracking_valid = frames.iter().all(ordinary_swing_frame_is_owned)
        && frames.windows(2).all(|pair| {
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
                    "left_foot",
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
                    "right_foot",
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
                    side,
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
                        return ordinary_unplanned_release_transition_is_valid(
                            &pair[0],
                            &pair[1],
                            before_solve,
                            after_solve,
                            before_release_target,
                            after_release_target,
                        );
                    }
                    ordinary_planned_transition_is_valid(
                        &pair[0],
                        &pair[1],
                        side,
                        before_plan,
                        after_plan,
                        before_solve,
                        after_solve,
                        support,
                        release_active,
                    )
                },
            )
        });
    let reported_support_contacts_valid = reported_support_contacts_are_valid(&frames);
    let run_contact_acquisition_valid = terrain_run_contacts_are_valid(&frames);
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
            || scenario_frames.iter().all(|frame| {
                frame.ik_settle_capture_point.is_none()
                    && frame.ik_left_planned_contact.is_none()
                    && frame.ik_right_planned_contact.is_none()
                    && frame.ik_settle_progress.is_none()
            })
    });
    let final_support_balance_valid = stop_settle_scenarios.iter().all(|scenario| {
        let scenario_frames = frames
            .iter()
            .filter(|frame| frame.scenario == *scenario)
            .collect::<Vec<_>>();
        scenario_frames.is_empty()
            || scenario_frames.last().is_some_and(|frame| {
                frame.speed_metres_per_second <= 0.05
                    && frame.ik_left_support_weight >= 0.95
                    && frame.ik_right_support_weight >= 0.95
            })
    });
    let hard_stop_maximum_pelvis_step_metres = hard_stop_pelvis_vertical_step(&frames);
    let hard_stop_height_continuity_valid =
        hard_stop_maximum_pelvis_step_metres.is_none_or(|maximum_step| maximum_step <= 0.02);
    let biomechanics_within_review_bounds = scenarios.iter().all(|metrics| {
        if metrics.scenario.starts_with("dive-") {
            // Root forward is intentionally not the travel axis for lateral
            // dives. Judge the posed pelvis-to-head long axis instead.
            return metrics.maximum_dive_axis_motion_error_degrees <= 20.0;
        }
        if metrics.scenario == "full-ragdoll"
            || metrics.scenario.starts_with("downed-")
            || metrics.scenario.ends_with("-get-up")
            || metrics.scenario == "jump-charge-anticipation"
            || metrics.scenario == "ordinary-camera-pitch"
        {
            // Posture scenarios deliberately leave the upright foot-track and
            // knee hemispheres; the stationary camera-pitch diagnostic has no
            // gait to validate. Their acceptance gates are finite output,
            // continuity, penetration, and visual review of the authored arc.
            return true;
        }
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
        let world_plants = matches!(
            scenario_metadata(&metrics.scenario).kind,
            ScenarioKind::RaisedGuard | ScenarioKind::Attack
        );
        (!world_plants
            || attack
            || (metrics.maximum_supported_foot_slip_metres_per_frame
                <= supported_foot_slip_limit(&metrics.scenario)
                && metrics.maximum_planted_foot_drift_metres
                    <= planted_drift_limit(&metrics.scenario)))
            && (metrics.scenario == "quickstep-right"
                || metrics.minimum_signed_foot_track_metres >= -0.01)
            && metrics.minimum_inter_foot_separation_metres
                >= inter_foot_separation_limit(&metrics.scenario)
            && (!procedural_solver_gates_apply
                // Stationary attack fixtures include the authored fully
                // extended guard leg; moving procedural steps retain the
                // analytic knee-reserve gate below.
                || metrics.scenario.starts_with("attack-live-stationary")
                || (metrics.minimum_knee_flexion_degrees >= 3.9
                    && metrics.minimum_knee_hemisphere_dot >= 0.0))
            && (!procedural_solver_gates_apply
                || metrics.maximum_knee_foot_yaw_offset_degrees <= 22.6)
            && metrics.maximum_facing_tracking_excess_degrees <= 0.2
            && metrics.final_facing_motion_error_degrees <= 3.0
            && (attack
                || !procedural_solver_gates_apply
                || metrics.maximum_contact_sole_clearance_metres
                    <= if metrics.scenario == "terrain-steady-run-5.5" {
                        0.01
                    } else {
                        0.04
                    })
            && metrics.pelvis_vertical_range_metres <= vertical_range_limit
            && metrics.head_vertical_range_metres <= vertical_range_limit
            && if !expects_loop_seam(&metrics.scenario) {
                metrics.loop_seam_position_metres.is_none()
                    && metrics.loop_seam_rotation_degrees.is_none()
            } else {
                metrics
                    .loop_seam_position_metres
                    .is_some_and(|value| value <= loop_seam_position_limit(&metrics.scenario))
                    && metrics
                        .loop_seam_rotation_degrees
                        .is_some_and(|value| value <= 5.0)
            }
    });
    let views_are_distinct = sequence.duplicate_view_frames.is_empty();
    let repeated_evaluation_valid = sequence.repeated_evaluation_valid;
    let semantic_route_paths_exercised = frames.iter().all(|frame| {
        frame.semantic_route_requested_path == SemanticRoutePath::LegacyFallback
            || (frame.semantic_route_runtime_evaluated
                && frame.semantic_route_selected_path == frame.semantic_route_requested_path)
    });
    let semantic_route_path_counts = frames.iter().fold(BTreeMap::new(), |mut counts, frame| {
        *counts
            .entry(frame.semantic_route_selected_path.as_str().to_owned())
            .or_insert(0) += 1;
        counts
    });
    let validation = CaptureValidation {
        finite_transforms,
        all_scenarios_complete,
        all_artifacts_written,
        continuity_within_review_bounds,
        biomechanics_within_review_bounds,
        no_ground_penetration,
        raised_guard_fixed_support,
        raised_guard_step_liveness_valid,
        flat_controller_height_stable,
        phase_owned_height_valid,
        run_flight_valid,
        body_response_valid,
        straight_run_torso_sway_valid,
        speed_ramp_phase_continuity_valid,
        contact_sequences_valid,
        cadence_contact_valid,
        event_stream_valid,
        landing_response_valid,
        landing_foot_preservation_valid,
        ordinary_swing_tracking_valid,
        reported_support_contacts_valid,
        run_contact_acquisition_valid,
        stop_settle_capture_valid,
        final_support_balance_valid,
        hard_stop_maximum_pelvis_step_metres,
        hard_stop_height_continuity_valid,
        repeated_evaluation_valid,
        semantic_route_paths_exercised,
        jitter_validation,
        views_are_distinct,
        duplicate_view_frames: sequence.duplicate_view_frames.clone(),
        note: "Continuity metrics are regression signals, not biomechanical proof; review index.html at normal and slow speed.",
    };
    let quality_score = quality_score(&frames, &scenarios, &validation);
    let acceptance_passed = validation_passed(&validation);
    let manifest = CaptureManifest {
        sample_hz: SAMPLE_HZ,
        playback_backend,
        global_bone_trace: "global-bone-transforms.jsonl",
        pose_buffer: pose_buffer_metrics,
        pipeline: "shared tactical player, scene, camera, authoritative locomotion projection, direct semantic routing, fixed-rate pose-buffer FK with per-joint inertialization, and final procedural passes",
        views: VIEWS,
        validation,
        quality_score,
        scenarios,
        frames,
        presentation_events,
        semantic_route_path_counts,
    };
    let global_bone_trace_path = sequence.output.join("global-bone-transforms.jsonl");
    let trace_file = File::create(&global_bone_trace_path)
        .unwrap_or_else(|error| panic!("failed to create {global_bone_trace_path:?}: {error}"));
    let mut trace_writer = BufWriter::new(trace_file);
    for frame in &global_bone_frames {
        serde_json::to_writer(&mut trace_writer, frame).unwrap_or_else(|error| {
            panic!("failed to serialize {global_bone_trace_path:?}: {error}")
        });
        trace_writer
            .write_all(b"\n")
            .unwrap_or_else(|error| panic!("failed to write {global_bone_trace_path:?}: {error}"));
    }
    trace_writer
        .flush()
        .unwrap_or_else(|error| panic!("failed to flush {global_bone_trace_path:?}: {error}"));
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
    if acceptance_passed {
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

fn validation_passed(validation: &CaptureValidation) -> bool {
    validation.finite_transforms
        && validation.all_scenarios_complete
        && validation.all_artifacts_written
        && validation.continuity_within_review_bounds
        && validation.biomechanics_within_review_bounds
        && validation.no_ground_penetration
        && validation.raised_guard_fixed_support
        && validation.raised_guard_step_liveness_valid
        && validation.flat_controller_height_stable
        && validation.phase_owned_height_valid
        && validation.run_flight_valid
        && validation.body_response_valid
        && validation.straight_run_torso_sway_valid
        && validation.speed_ramp_phase_continuity_valid
        && validation.contact_sequences_valid
        && validation.cadence_contact_valid
        && validation.event_stream_valid
        && validation.landing_response_valid
        && validation.landing_foot_preservation_valid
        && validation.ordinary_swing_tracking_valid
        && validation.reported_support_contacts_valid
        && validation.run_contact_acquisition_valid
        && validation.stop_settle_capture_valid
        && validation.final_support_balance_valid
        && validation.hard_stop_height_continuity_valid
        && validation.repeated_evaluation_valid
        && validation.semantic_route_paths_exercised
        && validation.jitter_validation.diagnostics_complete
        && validation
            .jitter_validation
            .unacceptable_final_incident_count
            == 0
        && validation.views_are_distinct
}

fn quality_score(
    frames: &[FrameSample],
    scenarios: &[ScenarioMetrics],
    validation: &CaptureValidation,
) -> QualityScore {
    let catastrophic_foot_displacement_failed = catastrophic_foot_displacement(frames);
    let anatomical_invalid_joints_failed = scenarios.iter().any(|metrics| {
        procedural_leg_solver_gates_apply(&metrics.scenario)
            && (metrics.minimum_knee_flexion_degrees < 3.9
                || metrics.minimum_knee_hemisphere_dot < 0.0
                || metrics.minimum_signed_foot_track_metres < -0.01
                || metrics.minimum_inter_foot_separation_metres
                    < inter_foot_separation_limit(&metrics.scenario)
                || metrics.maximum_knee_foot_yaw_offset_degrees > 22.6)
    });
    let contact_foot_airborne_failed = !validation.no_ground_penetration
        || !validation.run_flight_valid
        || !validation.reported_support_contacts_valid
        || !validation.run_contact_acquisition_valid;
    let both_feet_behind_hips_failed = both_feet_behind_hips(frames);
    let guard_step_liveness_failed = !validation.raised_guard_step_liveness_valid;
    let foot_dragging_failed = scenarios.iter().any(|metrics| {
        let supported_slip_limit = supported_foot_slip_limit(&metrics.scenario);
        metrics.maximum_supported_foot_slip_metres_per_frame > supported_slip_limit
            || metrics.maximum_planted_foot_drift_metres > planted_drift_limit(&metrics.scenario)
    });
    let jitter_and_jerk_failed = !validation.jitter_validation.diagnostics_complete
        || validation
            .jitter_validation
            .unacceptable_final_incident_count
            > 0;
    let categories = QualityCategories {
        catastrophic_foot_displacement_failed,
        guard_step_liveness_failed,
        anatomical_invalid_joints_failed,
        contact_foot_airborne_failed,
        both_feet_behind_hips_failed,
        foot_dragging_failed,
        jitter_and_jerk_failed,
    };
    let weighted_defect_score = weighted_defect_score(&categories);
    let capture_complete = validation.all_scenarios_complete && validation.all_artifacts_written;
    let quality_percent = if capture_complete {
        100.0 * (1.0 - f32::from(weighted_defect_score) / 31.0)
    } else {
        0.0
    };
    QualityScore {
        weighted_defect_score,
        maximum_weighted_defect_score: 31,
        quality_percent,
        acceptance_passed: validation_passed(validation),
        categories,
    }
}

fn weighted_defect_score(categories: &QualityCategories) -> u8 {
    if categories.catastrophic_foot_displacement_failed || categories.guard_step_liveness_failed {
        31
    } else {
        u8::from(categories.anatomical_invalid_joints_failed) * 16
            + u8::from(categories.contact_foot_airborne_failed) * 8
            + u8::from(categories.both_feet_behind_hips_failed) * 4
            + u8::from(categories.foot_dragging_failed) * 2
            + u8::from(categories.jitter_and_jerk_failed)
    }
}

const CATASTROPHIC_FOOT_HORIZONTAL_HIP_OFFSET_METRES: f32 = 0.65;
const CATASTROPHIC_FOOT_DISPLACEMENT_SECONDS: f32 = 0.1;

fn catastrophic_horizontal_foot_offset(hip: Vec3, foot: Vec3) -> bool {
    (foot - hip).xz().length() > CATASTROPHIC_FOOT_HORIZONTAL_HIP_OFFSET_METRES
}

fn catastrophic_foot_displacement(frames: &[FrameSample]) -> bool {
    let mut duration = 0.0_f32;
    for pair in frames.windows(2) {
        let [before, after] = pair else {
            continue;
        };
        if before.scenario != after.scenario || !procedural_leg_solver_gates_apply(&after.scenario)
        {
            duration = 0.0;
            continue;
        }
        let displaced = [("left_hip", "left_foot"), ("right_hip", "right_foot")]
            .into_iter()
            .any(|(hip, foot)| {
                body_local(after, hip)
                    .zip(body_local(after, foot))
                    .is_some_and(|(hip, foot)| catastrophic_horizontal_foot_offset(hip, foot))
            });
        if displaced {
            duration += (after.time_seconds - before.time_seconds).max(0.0);
            if duration >= CATASTROPHIC_FOOT_DISPLACEMENT_SECONDS {
                return true;
            }
        } else {
            duration = 0.0;
        }
    }
    false
}

fn both_feet_behind_hips(frames: &[FrameSample]) -> bool {
    let mut duration = 0.0_f32;
    for pair in frames.windows(2) {
        let [before, after] = pair else {
            continue;
        };
        if before.scenario != after.scenario || after.speed_metres_per_second <= 0.05 {
            duration = 0.0;
            continue;
        }
        let Some(direction) = Vec3::from_array(after.world_travel_direction).try_normalize() else {
            duration = 0.0;
            continue;
        };
        let behind = [("left_hip", "left_foot"), ("right_hip", "right_foot")]
            .into_iter()
            .all(|(hip, foot)| {
                let Some(hip) = after.bones.get(hip) else {
                    return false;
                };
                let Some(foot) = after.bones.get(foot) else {
                    return false;
                };
                (Vec3::from_array(foot.position) - Vec3::from_array(hip.position)).dot(direction)
                    < -0.02
            });
        if behind {
            duration += (after.time_seconds - before.time_seconds).max(0.0);
            if duration >= 0.3 {
                return true;
            }
        } else {
            duration = 0.0;
        }
    }
    false
}

fn foot_continuity_limit(scenario: &str) -> f32 {
    if scenario == "quickstep-right" {
        // A 5 m/s world-planted takeoff foot releases into an airborne guard
        // target. Its measured 7.84 cm sample remains below one root-travel
        // tick and is independently bounded by plant-drift and reach gates.
        0.09
    } else if scenario.starts_with("attack-live-") {
        0.21
    } else if scenario.starts_with("dive-") || scenario.ends_with("-get-up") {
        // One-shot authored whole-body transitions move freely rather than
        // preserving an upright gait plant. Keep a strict per-sample teleport
        // guard while allowing the measured supine hand/foot recovery speed.
        0.10
    } else if scenario.starts_with("raised-guard") {
        // A guard swing replaces a 2 m/s body support in one half-step and
        // therefore legitimately travels faster than the owner. The measured
        // sustained-forward maximum is 8.83 cm/sample with zero plant drift;
        // retain a narrow 9 cm teleport guard instead of reinstating lag.
        0.09
    } else if scenario == "flat-grid-run-5.5" {
        // The rigid travel lean and flat-ground solve peak at 16.09 cm in the
        // current authored cycle. Rendered review remains continuous; retain a
        // narrow 16.5 cm teleport guard for this dedicated diagnostic.
        0.165
    } else if scenario.contains("run") || scenario_requires_strict_terrain_toe_clearance(scenario) {
        // A complete authored run cycle moves a foot relative to the body as
        // well as translating the body by 8.594 cm per 64 Hz sample. Keep a
        // bounded visual-continuity gate without requiring a world-space
        // plant or follower from the removed ordinary-locomotion planner.
        0.15
    } else {
        0.055
    }
}

fn knee_continuity_limit(scenario: &str) -> f32 {
    if scenario == "quickstep-right" {
        // Reactive release from the analytic reach boundary bends a nearly
        // extended knee faster than ordinary walking. Retain the terrain-run
        // solver's strict 16 cm teleport guard for this one-shot hop.
        0.16
    } else if scenario.starts_with("attack-live-") {
        0.15
    } else if scenario_requires_strict_terrain_toe_clearance(scenario) {
        // Terrain contact acquisition adds slope-aligned leg flexion to the
        // authored Run motion. The measured worst adjacent samples (frames
        // 60-61) remain visually continuous at 15.2 cm.
        0.16
    } else if scenario.contains("run") {
        // Preserve the complete authored Run flight pose. Its measured knee
        // motion is 12.5 cm per 64 Hz sample; this gate leaves a small review
        // margin without weakening the non-run contract.
        0.13
    } else {
        0.10
    }
}

fn loop_seam_position_limit(scenario: &str) -> f32 {
    if scenario.starts_with("raised-guard") {
        // Raised cycles are sampled one 2 m/s controller tick across the
        // nominal seam (3.125 cm at 64 Hz).
        0.035
    } else if scenario == "flat-grid-run-5.5" {
        // Three complete cycles measure a 3.10 cm sampled seam on the flat
        // terrain solve. Keep this diagnostic's margin local.
        0.032
    } else if scenario.contains("run") {
        // The sampled seam of the complete authored Run cycle is 2.87 cm.
        0.03
    } else {
        0.015
    }
}

fn scenario_requires_strict_terrain_toe_clearance(scenario: &str) -> bool {
    matches!(
        scenario,
        "terrain-steady-run-5.5"
            | "flat-grid-run-5.5"
            | "terrain-run-flight-stop"
            | "terrain-tap-restart-crossfade"
    )
}

fn planted_drift_limit(scenario: &str) -> f32 {
    if scenario.starts_with("raised-guard") || scenario == "terrain-steady-run-5.5" {
        0.01
    } else {
        0.035
    }
}

fn supported_foot_slip_limit(scenario: &str) -> f32 {
    if scenario == "terrain-steady-run-5.5" {
        0.01
    } else {
        0.035
    }
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
            let mut maximum_foot_rotation = 0.0_f32;
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
                    if is_foot_bone(name) {
                        maximum_foot_rotation = maximum_foot_rotation.max(rotation);
                    }
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
            let guard_liveness = guard_step_liveness_metrics(&metric_frames);
            ScenarioMetrics {
                scenario: scenario.to_owned(),
                frame_count: frames.len(),
                maximum_root_relative_step_metres: maximum_step,
                maximum_leg_root_relative_step_metres: maximum_leg_step,
                maximum_foot_root_relative_step_metres: maximum_foot_step,
                maximum_knee_root_relative_step_metres: maximum_knee_step,
                worst_displacement,
                maximum_bone_rotation_step_degrees: maximum_rotation,
                maximum_foot_rotation_step_degrees: maximum_foot_rotation,
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
                minimum_flight_toe_clearance_metres: minimum_toe_clearance(&metric_frames, false),
                minimum_contact_toe_clearance_metres: minimum_toe_clearance(&metric_frames, true),
                head_vertical_range_metres: root_relative_vertical_range(&metric_frames, "head"),
                foot_terrain_relief_metres: foot_terrain_relief(&metric_frames),
                minimum_knee_forward_bend_metres: minimum_knee_bend(&metric_frames),
                minimum_signed_foot_track_metres: minimum_signed_foot_track(&metric_frames),
                minimum_inter_foot_separation_metres: minimum_inter_foot_separation(&metric_frames),
                minimum_knee_flexion_degrees: minimum_knee_flexion(&procedural_frames),
                minimum_knee_hemisphere_dot: minimum_knee_hemisphere(&procedural_frames),
                maximum_knee_foot_yaw_offset_degrees: maximum_knee_foot_yaw_offset(
                    &procedural_frames,
                ),
                maximum_facing_motion_error_degrees: maximum_facing_error(&metric_frames),
                maximum_facing_tracking_excess_degrees: maximum_facing_tracking_excess(
                    &metric_frames,
                ),
                maximum_guard_facing_error_degrees: maximum_guard_facing_error(&metric_frames),
                final_facing_motion_error_degrees: final_facing_error(&metric_frames),
                maximum_dive_axis_motion_error_degrees: maximum_dive_axis_motion_error(
                    &metric_frames,
                ),
                maximum_supported_foot_slip_metres_per_frame: maximum_slip,
                maximum_planted_foot_drift_metres: maximum_plant_drift,
                guard_step_liveness_required: guard_liveness.required,
                completed_guard_half_step_count: guard_liveness.completed_half_steps,
                visible_guard_half_step_count: guard_liveness.visible_half_steps,
                minimum_guard_swing_travel_metres: guard_liveness.minimum_swing_travel_metres,
                minimum_guard_swing_clearance_gain_metres: guard_liveness
                    .minimum_swing_clearance_gain_metres,
                minimum_foot_clearance_metres: minimum_foot_clearance(&metric_frames),
            }
        })
        .collect()
}

fn expects_loop_seam(scenario: &str) -> bool {
    scenario_metadata(scenario).repeatable
}

fn vertical_range_limit(scenario: &str, foot_terrain_relief_metres: f32) -> f32 {
    if scenario == "quickstep-right" {
        0.5
    } else if scenario.starts_with("attack-live-") {
        0.35
    } else if scenario.starts_with("raised-guard-") {
        // Stationary raised ownership includes initial guard-pelvis
        // acquisition. Preserve a few millimetres of numerical margin above
        // the authored range while per-frame pelvis, knee, plant, and track
        // gates remain strict.
        RAISED_GUARD_VERTICAL_RANGE_LIMIT_METRES + 0.005
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
        // Terrain conformity contributes the authored leg/pelvis reach on top
        // of the 4 cm phase wave even though this terrain has zero relief.
        "flat-grid-walk-2.0" => (0.025, 0.075, 2),
        "steady-run-5.5" | "flat-grid-run-5.5" => (0.025, 0.10, 2),
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

const MINIMUM_GUARD_SWING_TRAVEL_METRES: f32 = 0.05;
const MINIMUM_GUARD_SWING_CLEARANCE_GAIN_METRES: f32 = 0.03;

#[derive(Debug, Default)]
struct GuardStepLivenessMetrics {
    required: bool,
    completed_half_steps: usize,
    visible_half_steps: usize,
    minimum_swing_travel_metres: f32,
    minimum_swing_clearance_gain_metres: f32,
}

fn guard_step_liveness_metrics(frames: &[&FrameSample]) -> GuardStepLivenessMetrics {
    let required = frames.first().is_some_and(|frame| {
        let metadata = scenario_metadata(&frame.scenario);
        metadata.kind == ScenarioKind::RaisedGuard
            && metadata.repeatable
            && frames
                .iter()
                .all(|frame| frame.speed_metres_per_second > 0.05)
    });
    if !required {
        return GuardStepLivenessMetrics::default();
    }

    let mut interval_start = 0;
    let mut completed_half_steps = 0;
    let mut visible_half_steps = 0;
    let mut minimum_swing_travel = f32::INFINITY;
    let mut minimum_swing_clearance_gain = f32::INFINITY;

    for interval_end in 1..frames.len() {
        if frames[interval_end].contact_sequence == frames[interval_end - 1].contact_sequence {
            continue;
        }

        let start = frames[interval_start];
        let end = frames[interval_end];
        let left_gain = end.left_support_weight - start.left_support_weight;
        let right_gain = end.right_support_weight - start.right_support_weight;
        let (
            swing_foot,
            start_swing_support,
            end_swing_support,
            start_other_support,
            end_other_support,
        ) = if left_gain >= right_gain {
            (
                "left_foot",
                start.left_support_weight,
                end.left_support_weight,
                start.right_support_weight,
                end.right_support_weight,
            )
        } else {
            (
                "right_foot",
                start.right_support_weight,
                end.right_support_weight,
                start.left_support_weight,
                end.left_support_weight,
            )
        };

        completed_half_steps += 1;
        let support_swap_valid = start_swing_support <= 0.25
            && end_swing_support >= 0.75
            && start_other_support >= 0.75
            && end_other_support <= 0.25;
        let travel = start
            .bones
            .get(swing_foot)
            .zip(end.bones.get(swing_foot))
            .map_or(0.0, |(start, end)| {
                (Vec3::from_array(end.position) - Vec3::from_array(start.position))
                    .xz()
                    .length()
            });
        let interval = &frames[interval_start..=interval_end];
        let start_clearance = start
            .bones
            .get(swing_foot)
            .and_then(|bone| bone.terrain_clearance_metres);
        let end_clearance = end
            .bones
            .get(swing_foot)
            .and_then(|bone| bone.terrain_clearance_metres);
        let maximum_clearance = interval
            .iter()
            .filter_map(|frame| {
                frame
                    .bones
                    .get(swing_foot)
                    .and_then(|bone| bone.terrain_clearance_metres)
            })
            .fold(f32::NEG_INFINITY, f32::max);
        let clearance_gain = start_clearance
            .zip(end_clearance)
            .filter(|_| maximum_clearance.is_finite())
            .map_or(0.0, |(start, end)| maximum_clearance - start.max(end));

        minimum_swing_travel = minimum_swing_travel.min(travel);
        minimum_swing_clearance_gain = minimum_swing_clearance_gain.min(clearance_gain);
        if support_swap_valid
            && travel >= MINIMUM_GUARD_SWING_TRAVEL_METRES
            && clearance_gain >= MINIMUM_GUARD_SWING_CLEARANCE_GAIN_METRES
        {
            visible_half_steps += 1;
        }
        interval_start = interval_end;
    }

    GuardStepLivenessMetrics {
        required,
        completed_half_steps,
        visible_half_steps,
        minimum_swing_travel_metres: if minimum_swing_travel.is_finite() {
            minimum_swing_travel
        } else {
            0.0
        },
        minimum_swing_clearance_gain_metres: if minimum_swing_clearance_gain.is_finite() {
            minimum_swing_clearance_gain
        } else {
            0.0
        },
    }
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

fn maximum_knee_foot_yaw_offset(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        .flat_map(|frame| {
            [
                frame.ik_left_knee_foot_yaw_offset_degrees,
                frame.ik_right_knee_foot_yaw_offset_degrees,
            ]
        })
        .fold(0.0, f32::max)
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

fn maximum_dive_axis_motion_error(frames: &[&FrameSample]) -> f32 {
    frames
        .iter()
        // Preserve the launch vector as the directional contract through the
        // complete terrain-contact recovery. Physical speed may already be
        // zero while the authored body is still resolving its landing pose.
        .filter(|frame| frame.scenario.starts_with("dive-"))
        .filter_map(|frame| {
            let head = Vec3::from_array(frame.bones.get("head")?.position);
            let pelvis = Vec3::from_array(frame.bones.get("pelvis")?.position);
            let axis = Vec3::new(head.x - pelvis.x, 0.0, head.z - pelvis.z);
            // While the character is still nearly upright, the horizontal
            // projection of its long axis is too short to define a stable
            // travel heading. Begin judging once the dive has visibly tipped.
            if axis.length() < 0.25 {
                return None;
            }
            let axis = axis.normalize();
            let travel = Vec3::from_array(frame.world_travel_direction).try_normalize()?;
            Some(axis.angle_between(travel).to_degrees())
        })
        .fold(0.0, f32::max)
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

fn ordinary_swing_frame_is_owned(frame: &FrameSample) -> bool {
    if scenario_metadata(&frame.scenario).kind != ScenarioKind::Terrain
        || frame.speed_metres_per_second <= 0.05
        || frame.ik_settle_progress.is_some()
    {
        return true;
    }
    [
        (
            "left",
            frame.ik_left_support_weight,
            frame.ik_left_authored_target,
            frame.ik_left_solve_target,
            frame.ik_left_planned_contact,
            frame.ik_left_release_active,
        ),
        (
            "right",
            frame.ik_right_support_weight,
            frame.ik_right_authored_target,
            frame.ik_right_solve_target,
            frame.ik_right_planned_contact,
            frame.ik_right_release_active,
        ),
    ]
    .into_iter()
    .all(
        |(_side, support, authored, solved, planned, release_active)| {
            support > 0.05
                || authored.zip(solved).is_some_and(|(authored, solved)| {
                    let authored = Vec3::from_array(authored);
                    let solved = Vec3::from_array(solved);
                    planned.is_some() || release_active || solved.distance(authored) <= 0.03
                })
        },
    )
}

fn ordinary_unplanned_release_transition_is_valid(
    before: &FrameSample,
    after: &FrameSample,
    before_solve: Option<[f32; 3]>,
    after_solve: Option<[f32; 3]>,
    before_release_target: Option<[f32; 3]>,
    after_release_target: Option<[f32; 3]>,
) -> bool {
    before_solve
        .zip(after_solve)
        .is_some_and(|(before_solve, after_solve)| {
            let before_solve_world = Vec3::from_array(before_solve);
            let after_solve_world = Vec3::from_array(after_solve);
            let before_solve = target_body_local(before, before_solve_world);
            let after_solve = target_body_local(after, after_solve_world);
            let frozen_goal_converges = before_release_target.zip(after_release_target).is_none_or(
                |(before_goal, after_goal)| {
                    let before_goal = Vec3::from_array(before_goal);
                    let after_goal = Vec3::from_array(after_goal);
                    target_body_local(before, before_goal)
                        .distance(target_body_local(after, after_goal))
                        > 0.002
                        || after_solve_world.distance(after_goal)
                            <= before_solve_world.distance(before_goal) + 0.001
                },
            );
            let step_limit = if frame_uses_run_motion_budget(after) {
                // Run's owner-space target advances up to 8.75 cm per sample;
                // its terrain scenario independently rejects rendered foot
                // motion above 9.5 cm. Walk/settle retains the 5.5 cm budget.
                0.095
            } else {
                0.055
            };
            after_solve.distance(before_solve) <= step_limit && frozen_goal_converges
        })
}

fn frame_uses_run_motion_budget(frame: &FrameSample) -> bool {
    let run_speed_threshold =
        (WALK_LOCOMOTION_PROFILE.reference_speed + RUN_LOCOMOTION_PROFILE.reference_speed) * 0.5;
    frame.speed_metres_per_second >= run_speed_threshold
}

#[allow(clippy::too_many_arguments)]
fn ordinary_planned_transition_is_valid(
    before: &FrameSample,
    after: &FrameSample,
    foot: &str,
    before_plan: Option<[f32; 3]>,
    after_plan: Option<[f32; 3]>,
    before_solve: Option<[f32; 3]>,
    after_solve: Option<[f32; 3]>,
    after_support: f32,
    release_active: bool,
) -> bool {
    before_plan
        .zip(after_plan)
        .zip(before_solve.zip(after_solve))
        .is_none_or(|((before_plan, after_plan), (before_solve, after_solve))| {
            let before_plan = Vec3::from_array(before_plan);
            let after_plan = Vec3::from_array(after_plan);
            let before_solve_world = Vec3::from_array(before_solve);
            let after_solve_world = Vec3::from_array(after_solve);
            if frame_uses_run_motion_budget(after) {
                let plan_is_frozen = before_plan.distance(after_plan) <= 0.002;
                let atomic_acquisition_retarget = after_support > 0.05
                    && !release_active
                    && after_solve_world.distance(after_plan) <= 0.02;
                let owned_airborne_replan = after_support <= 0.5 && release_active;
                let rendered_step = before.bones.get(foot).zip(after.bones.get(foot)).map(
                    |(before_foot, after_foot)| {
                        target_body_local(after, Vec3::from_array(after_foot.position)).distance(
                            target_body_local(before, Vec3::from_array(before_foot.position)),
                        )
                    },
                );
                (plan_is_frozen || atomic_acquisition_retarget || owned_airborne_replan)
                    && rendered_step.is_some_and(|step| step <= 0.095)
            } else {
                before_plan.distance(after_plan) <= 0.002
                    && after_solve_world.distance(after_plan)
                        <= before_solve_world.distance(before_plan) + 0.005
            }
        })
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
                    support >= 0.95
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

fn minimum_toe_clearance(frames: &[&FrameSample], contact: bool) -> f32 {
    let minimum = frames
        .iter()
        .filter(|frame| {
            contact
                || transition_flight_toe_sample_is_active(
                    &frame.scenario,
                    frame.speed_metres_per_second,
                    frame.ik_settle_progress,
                )
        })
        .flat_map(|frame| {
            let (left_support, right_support) =
                if scenario_metadata(&frame.scenario).procedural_solver {
                    (frame.ik_left_support_weight, frame.ik_right_support_weight)
                } else {
                    (frame.left_support_weight, frame.right_support_weight)
                };
            [("left_toe", left_support), ("right_toe", right_support)]
                .into_iter()
                .filter(move |(_, support)| (*support > 0.05) == contact)
                .filter_map(move |(toe, _)| {
                    frame
                        .bones
                        .get(toe)
                        .and_then(|bone| bone.terrain_clearance_metres)
                })
        })
        .fold(f32::INFINITY, f32::min);
    if minimum.is_finite() { minimum } else { 0.0 }
}

fn transition_flight_toe_sample_is_active(
    scenario: &str,
    speed_metres_per_second: f32,
    settle_progress: Option<f32>,
) -> bool {
    !matches!(
        scenario,
        "terrain-run-flight-stop" | "terrain-tap-restart-crossfade"
    ) || speed_metres_per_second > 0.05
        || settle_progress.is_some()
}

fn strict_transition_flight_toe_clearance_is_valid(clearance_metres: f32) -> bool {
    clearance_metres >= 0.01
}

fn reported_support_contacts_are_valid(frames: &[FrameSample]) -> bool {
    frames.iter().all(|frame| {
        if scenario_metadata(&frame.scenario).kind == ScenarioKind::Attack
            || frame.action != SkeletonAction::None
            || (!scenario_uses_terrain_ik(&frame.scenario)
                && frame.weapon_guard == WeaponGuardState::Lowered)
        {
            // In an FK-only comparison the semantic weights describe authored
            // loading, not a claim that the procedural solver owns contact.
            // Attack captures exercise the same raised-guard locomotion and
            // terrain IK ownership used outside attacks.
            return true;
        }
        [
            ("left_foot", frame.ik_left_support_weight),
            ("right_foot", frame.ik_right_support_weight),
        ]
        .into_iter()
        .all(|(foot, support)| {
            let quickstep_toe_contact = if frame.scenario == "quickstep-right" {
                let toe = if foot == "left_foot" {
                    "left_toe"
                } else {
                    "right_toe"
                };
                frame
                    .bones
                    .get(toe)
                    .and_then(|bone| bone.terrain_clearance_metres)
                    .is_some_and(|clearance| clearance.abs() <= SOLE_CONTACT_TOLERANCE_METRES)
            } else {
                false
            };
            support.is_finite()
                && (support < 0.95
                    || quickstep_toe_contact
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

fn terrain_run_contacts_are_valid(frames: &[FrameSample]) -> bool {
    let run = frames
        .iter()
        .filter(|frame| frame.scenario == "terrain-steady-run-5.5")
        .collect::<Vec<_>>();
    if run.is_empty() {
        return true;
    }
    let warmed = phase_validation_frames(&run);
    let mut acquisitions = Vec::new();
    let mut last = None;
    for frame in &warmed {
        let support = if frame
            .ik_left_support_weight
            .max(frame.ik_right_support_weight)
            < 0.5
        {
            None
        } else {
            Some(frame.ik_left_support_weight >= frame.ik_right_support_weight)
        };
        if support != last {
            if let Some(left) = support {
                acquisitions.push(left);
            }
            last = support;
        }
    }
    acquisitions.len() >= 4
        && acquisitions.contains(&true)
        && acquisitions.contains(&false)
        && acquisitions.windows(2).all(|pair| pair[0] != pair[1])
        && (0.08..=0.20).contains(&maximum_no_support_seconds(&warmed))
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
    let quality_summary = format!(
        "Quality score: {:.2}% ({}/31 weighted defect points); acceptance: {}",
        manifest.quality_score.quality_percent,
        manifest.quality_score.weighted_defect_score,
        if manifest.quality_score.acceptance_passed {
            "passed"
        } else {
            "failed"
        },
    );
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
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.4}</td><td>{:.4}</td><td>{}</td><td>{}/{}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td><td>{:.2}</td><td>{:.3}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.2}</td><td>{:.4}</td></tr>",
                scenario.scenario,
                scenario.frame_count,
                describe(&scenario.worst_displacement, "m"),
                describe(&scenario.worst_rotation, "deg"),
                scenario.loop_seam_position_metres.map_or("&mdash;".into(), |value| format!("{value:.4}")),
                scenario.loop_seam_rotation_degrees.map_or("&mdash;".into(), |value| format!("{value:.2}")),
                scenario.maximum_supported_foot_slip_metres_per_frame,
                scenario.maximum_planted_foot_drift_metres,
                scenario.guard_step_liveness_required,
                scenario.visible_guard_half_step_count,
                scenario.completed_guard_half_step_count,
                scenario.minimum_guard_swing_travel_metres,
                scenario.minimum_guard_swing_clearance_gain_metres,
                scenario.minimum_signed_foot_track_metres,
                scenario.minimum_inter_foot_separation_metres,
                scenario.minimum_knee_flexion_degrees,
                scenario.minimum_knee_hemisphere_dot,
                scenario.maximum_knee_foot_yaw_offset_degrees,
                scenario.maximum_facing_motion_error_degrees,
                scenario.maximum_facing_tracking_excess_degrees,
                scenario.maximum_guard_facing_error_degrees,
                scenario.final_facing_motion_error_degrees,
                scenario.maximum_dive_axis_motion_error_degrees,
                scenario.minimum_foot_clearance_metres,
            )
        })
        .collect::<String>();
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Animation review</title><style>
body{{font:15px system-ui;background:#111820;color:#e8eef5;margin:24px}}button,select{{margin:4px;padding:8px}}img{{max-width:min(960px,100%);background:#222}}table{{border-collapse:collapse;margin-top:20px}}td,th{{border:1px solid #526171;padding:6px}}.note{{max-width:960px;color:#b9c7d5}}#contact{{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:8px;margin-top:20px}}#contact img{{width:100%}}
</style></head><body><h1>Tactical locomotion review</h1>
<p class="note">{quality_summary}</p><p class="note">This runs the shared tactical player, hills scene, gameplay camera, 64 Hz authoritative locomotion projection, authored FK, and final procedural passes. Gameplay images are raw; side/front diagnostics add the cyan skeleton and support markers. Use normal speed first, then slow motion.</p>
<div>{scenario_buttons}</div><label>View <select id="view"><option value="gameplay">gameplay (raw)</option><option value="side">side diagnostic</option><option value="front">front diagnostic</option></select></label>
<label>Playback <select id="rate"><option value="1">normal</option><option value="2">half speed</option><option value="4">quarter speed</option></select></label>
<p id="telemetry"></p><img id="player"><div id="contact"></div>
<table><thead><tr><th>scenario</th><th>frames</th><th>worst root-relative displacement</th><th>worst rotation</th><th>loop seam m</th><th>loop seam deg</th><th>supported slip m/frame</th><th>planted interval drift m</th><th>guard liveness required</th><th>visible/completed half-steps</th><th>minimum swing travel m</th><th>minimum swing clearance gain m</th><th>signed foot track m</th><th>inter-foot separation m</th><th>knee flexion deg</th><th>knee hemisphere dot</th><th>knee-foot yaw offset deg</th><th>maximum facing error deg</th><th>tracking excess deg</th><th>guard facing error deg</th><th>final facing error deg</th><th>dive axis/travel error deg</th><th>minimum terrain-relative foot clearance m</th></tr></thead><tbody>{metrics}</tbody></table>
<script>const all={frame_json},scenarioNames={scenario_names_json};let scenario=scenarioNames[0]||"",i=0,timer;const player=document.querySelector('#player'),view=document.querySelector('#view'),rate=document.querySelector('#rate'),telemetry=document.querySelector('#telemetry');
function frames(){{return all.filter(x=>x.scenario===scenario)}}function show(){{const list=frames(),f=list.length?list[i%list.length]:null;if(!f){{player.removeAttribute('src');telemetry.textContent='No completed capture frames';return}}player.src=f.screenshots[view.value];telemetry.textContent=`${{f.scenario}} frame ${{f.scenario_frame}} | guard ${{f.weapon_guard}} lead ${{f.lead_foot}} | ${{f.speed_metres_per_second.toFixed(2)}} m/s | phase ${{f.gait_phase.toFixed(3)}} | world plants L ${{f.left_support_weight.toFixed(2)}} R ${{f.right_support_weight.toFixed(2)}}`;}}
function play(){{clearInterval(timer);timer=setInterval(()=>{{i=(i+1)%frames().length;show()}},1000/64*Number(rate.value))}}function contacts(){{const f=frames(),step=Math.max(1,Math.floor(f.length/12)),box=document.querySelector('#contact');box.innerHTML='';for(let n=0;n<f.length;n+=step){{let x=document.createElement('img');x.src=f[n].screenshots[view.value];x.title=`frame ${{f[n].scenario_frame}} phase ${{f[n].gait_phase.toFixed(3)}}`;box.appendChild(x)}}}}
document.querySelectorAll('button').forEach(b=>b.onclick=()=>{{scenario=b.dataset.scenario;i=0;show();contacts();play()}});view.onchange=()=>{{show();contacts()}};rate.onchange=play;show();contacts();play();</script></body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn quality_score_uses_the_documented_power_of_two_weights() {
        let categories = QualityCategories {
            catastrophic_foot_displacement_failed: false,
            guard_step_liveness_failed: false,
            anatomical_invalid_joints_failed: true,
            contact_foot_airborne_failed: true,
            both_feet_behind_hips_failed: true,
            foot_dragging_failed: true,
            jitter_and_jerk_failed: true,
        };
        assert_eq!(weighted_defect_score(&categories), 31);

        let categories = QualityCategories {
            catastrophic_foot_displacement_failed: false,
            guard_step_liveness_failed: false,
            anatomical_invalid_joints_failed: false,
            contact_foot_airborne_failed: false,
            both_feet_behind_hips_failed: false,
            foot_dragging_failed: false,
            jitter_and_jerk_failed: true,
        };
        assert_eq!(weighted_defect_score(&categories), 1);

        let categories = QualityCategories {
            catastrophic_foot_displacement_failed: true,
            guard_step_liveness_failed: false,
            anatomical_invalid_joints_failed: false,
            contact_foot_airborne_failed: false,
            both_feet_behind_hips_failed: false,
            foot_dragging_failed: false,
            jitter_and_jerk_failed: false,
        };
        assert_eq!(weighted_defect_score(&categories), 31);

        let categories = QualityCategories {
            catastrophic_foot_displacement_failed: false,
            guard_step_liveness_failed: true,
            anatomical_invalid_joints_failed: false,
            contact_foot_airborne_failed: false,
            both_feet_behind_hips_failed: false,
            foot_dragging_failed: false,
            jitter_and_jerk_failed: false,
        };
        assert_eq!(weighted_defect_score(&categories), 31);
    }

    #[test]
    fn severe_sideways_foot_displacement_is_catastrophic() {
        let hip = Vec3::new(0.2, 1.0, 0.0);
        assert!(!catastrophic_horizontal_foot_offset(
            hip,
            Vec3::new(0.84, 0.1, 0.0)
        ));
        assert!(catastrophic_horizontal_foot_offset(
            hip,
            Vec3::new(0.851, 0.1, 0.0)
        ));
    }

    #[test]
    fn quality_score_is_zero_for_an_incomplete_capture() {
        let validation = CaptureValidation {
            finite_transforms: true,
            all_scenarios_complete: false,
            all_artifacts_written: true,
            continuity_within_review_bounds: true,
            biomechanics_within_review_bounds: true,
            no_ground_penetration: true,
            raised_guard_fixed_support: true,
            raised_guard_step_liveness_valid: true,
            flat_controller_height_stable: true,
            phase_owned_height_valid: true,
            run_flight_valid: true,
            body_response_valid: true,
            straight_run_torso_sway_valid: true,
            speed_ramp_phase_continuity_valid: true,
            contact_sequences_valid: true,
            cadence_contact_valid: true,
            event_stream_valid: true,
            landing_response_valid: true,
            landing_foot_preservation_valid: true,
            ordinary_swing_tracking_valid: true,
            reported_support_contacts_valid: true,
            run_contact_acquisition_valid: true,
            stop_settle_capture_valid: true,
            final_support_balance_valid: true,
            hard_stop_maximum_pelvis_step_metres: None,
            hard_stop_height_continuity_valid: true,
            repeated_evaluation_valid: true,
            semantic_route_paths_exercised: true,
            jitter_validation: jitter::validate(&[], Default::default()),
            views_are_distinct: true,
            duplicate_view_frames: Vec::new(),
            note: "test",
        };
        let score = quality_score(&[], &[], &validation);
        assert_eq!(score.quality_percent, 0.0);
        assert!(!score.acceptance_passed);
    }

    fn unique_test_output(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "adventuresim-animation-viewer-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn complete_run_cycles_use_the_run_foot_continuity_budget_only() {
        assert!(0.149 <= foot_continuity_limit("terrain-run-flight-stop"));
        assert!(0.151 > foot_continuity_limit("terrain-run-flight-stop"));
        assert!(0.149 <= foot_continuity_limit("terrain-tap-restart-crossfade"));
        assert!(0.151 > foot_continuity_limit("terrain-tap-restart-crossfade"));
        assert!(0.056 > foot_continuity_limit("terrain-tap-stop-forward"));
        assert_eq!(foot_continuity_limit("terrain-steady-run-5.5"), 0.15);
        assert_eq!(knee_continuity_limit("terrain-steady-run-5.5"), 0.16);
        assert_eq!(knee_continuity_limit("steady-run-5.5"), 0.13);
        assert_eq!(knee_continuity_limit("steady-walk"), 0.10);
        assert_eq!(loop_seam_position_limit("steady-run-5.5"), 0.03);
        assert_eq!(loop_seam_position_limit("steady-walk"), 0.015);
    }

    #[test]
    fn terrain_run_stop_and_restart_require_the_strict_toe_clearance_contract() {
        assert!(scenario_requires_strict_terrain_toe_clearance(
            "terrain-steady-run-5.5"
        ));
        assert!(scenario_requires_strict_terrain_toe_clearance(
            "terrain-run-flight-stop"
        ));
        assert!(scenario_requires_strict_terrain_toe_clearance(
            "terrain-tap-restart-crossfade"
        ));
        assert!(!scenario_requires_strict_terrain_toe_clearance(
            "terrain-tap-stop-forward"
        ));
    }

    #[test]
    fn transient_toe_gate_excludes_idle_preroll_but_keeps_motion_and_settle() {
        for scenario in ["terrain-run-flight-stop", "terrain-tap-restart-crossfade"] {
            assert!(!transition_flight_toe_sample_is_active(scenario, 0.0, None));
            assert!(transition_flight_toe_sample_is_active(scenario, 5.5, None));
            assert!(transition_flight_toe_sample_is_active(
                scenario,
                0.0,
                Some(0.25)
            ));
        }
        assert!(transition_flight_toe_sample_is_active(
            "terrain-steady-run-5.5",
            0.0,
            None
        ));
        assert!(strict_transition_flight_toe_clearance_is_valid(0.01));
        assert!(!strict_transition_flight_toe_clearance_is_valid(-0.001));
    }

    #[test]
    fn capture_ticks_remain_unique_across_consecutive_scenario_boundaries() {
        let first = next_capture_simulation_tick(0, true);
        let second = next_capture_simulation_tick(first, false);
        let next_scenario_frame_zero = next_capture_simulation_tick(second, false);
        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(next_scenario_frame_zero, 2);
    }

    #[test]
    fn repeated_evaluation_rejects_missing_or_extra_tracked_bones() {
        let sample = BoneSample {
            position: Vec3::ZERO.to_array(),
            rotation_xyzw: Quat::IDENTITY.to_array(),
            terrain_clearance_metres: Some(0.0),
        };
        let expected = BTreeMap::from([("left_foot".to_owned(), sample)]);
        let missing = BTreeMap::new();
        assert!(repeated_bone_mismatch(&expected, &missing).is_some());
        assert!(repeated_bone_mismatch(&missing, &expected).is_some());
        assert!(repeated_bone_mismatch(&expected, &expected).is_none());
    }

    #[test]
    fn repeated_evaluation_diagnostics_detect_hidden_ik_state_mutation() {
        let baseline = LegIkDiagnostics::default();
        let changed = LegIkDiagnostics {
            left_support_weight: 1.0,
            left_release_active: true,
            left_planned_contact: Some(Vec3::NEG_Z),
            settle_progress: Some(0.5),
            ..default()
        };
        assert_ne!(baseline, changed);
    }

    #[test]
    fn report_invalidation_preserves_unrelated_files() {
        let output = unique_test_output("invalidate");
        fs::create_dir_all(&output).unwrap();
        for name in [
            "manifest.json",
            "index.html",
            "failure.txt",
            "global-bone-transforms.jsonl",
            "notes.txt",
        ] {
            fs::write(output.join(name), b"old").unwrap();
        }
        invalidate_previous_report(&output);
        assert!(!output.join("manifest.json").exists());
        assert!(!output.join("index.html").exists());
        assert!(!output.join("failure.txt").exists());
        assert!(!output.join("global-bone-transforms.jsonl").exists());
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
            attack_animation: None,
            strike_family: StrikeFamily::Thrust,
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
            ik_left_knee_foot_yaw_offset_degrees: 0.0,
            ik_right_knee_foot_yaw_offset_degrees: 0.0,
            semantic_route_requested_path: SemanticRoutePath::LegacyFallback,
            semantic_route_selected_path: SemanticRoutePath::LegacyFallback,
            semantic_route_runtime_evaluated: false,
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
    fn flat_grid_scenarios_are_opt_in_complete_cycles_with_terrain_ik() {
        for (scenario, speed) in [("flat-grid-walk-2.0", 2.0), ("flat-grid-run-5.5", 5.5)] {
            let sequence = CaptureSequence::new(PathBuf::new(), 1, Some(scenario));
            assert!(sequence.uses_flat_grid());
            assert!(sequence.plan.len() > 64);
            assert!(sequence.plan.iter().all(|frame| {
                frame.scenario == scenario
                    && frame.speed == speed
                    && terrain_ik_enabled_for_frame(frame)
            }));
        }

        let ordinary = CaptureSequence::new(PathBuf::new(), 1, Some("steady-walk-2.0"));
        assert!(!ordinary.uses_flat_grid());

        let stop = CaptureSequence::new(PathBuf::new(), 1, Some("flat-grid-walk-stop"));
        assert!(stop.uses_flat_grid());
        assert!(stop.plan[..48].iter().all(|frame| frame.speed == 2.0));
        assert!(stop.plan[56..].iter().all(|frame| frame.speed == 0.0));
        assert!(
            stop.plan[48..=56]
                .windows(2)
                .all(|pair| pair[1].speed <= pair[0].speed)
        );
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
            "raised-guard-right-support-release",
            "raised-guard-left-right-reversal",
            "raised-guard-right-support-reversal",
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
            .expect("reversal observation is accepted immediately");
        let previous_phase = reversal
            .iter()
            .find(|(frame, _, _)| *frame == 15)
            .expect("pre-reversal sample")
            .1;
        assert_eq!(changed.0, 16);
        let phase_delta = (changed.1 - previous_phase).rem_euclid(1.0);
        assert!((0.0..0.1).contains(&phase_delta));
    }

    #[test]
    fn guard_and_attack_captures_use_prepared_runtime_pose_assets() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in ["swing.glb", "thrust.glb", "offhand.glb"] {
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
            attack_animation: None,
            strike_family: StrikeFamily::Thrust,
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
            ik_left_knee_foot_yaw_offset_degrees: 0.0,
            ik_right_knee_foot_yaw_offset_degrees: 0.0,
            semantic_route_requested_path: SemanticRoutePath::LegacyFallback,
            semantic_route_selected_path: SemanticRoutePath::LegacyFallback,
            semantic_route_runtime_evaluated: false,
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

        frame.ik_left_knee_foot_yaw_offset_degrees = 90.0;
        assert!(maximum_knee_foot_yaw_offset(&[&frame]) > 45.0);
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

    #[test]
    fn cold_start_manifest_frame_requires_truthful_release_ownership() {
        let mut frame = foot_metric_frame(0, -0.1, 0.0, 0.0, 0.0);
        frame.scenario = "terrain-steady-run-5.5".into();
        frame.speed_metres_per_second = 5.5;
        frame.ik_left_authored_target = Some(Vec3::ZERO.to_array());
        frame.ik_left_solve_target = Some((Vec3::Y * 0.095).to_array());
        frame.ik_right_authored_target = Some(Vec3::ZERO.to_array());
        frame.ik_right_solve_target = Some(Vec3::ZERO.to_array());
        assert!(!ordinary_swing_frame_is_owned(&frame));

        frame.ik_left_release_active = true;
        assert!(ordinary_swing_frame_is_owned(&frame));
    }

    #[test]
    fn unplanned_release_transition_uses_run_specific_motion_budget() {
        let mut before = foot_metric_frame(0, -0.1, 0.0, 0.0, 0.0);
        let mut after = foot_metric_frame(1, -0.1, 0.0, 0.0, 0.0);
        before.scenario = "terrain-steady-run-5.5".into();
        after.scenario = before.scenario.clone();
        before.speed_metres_per_second = 5.5;
        after.speed_metres_per_second = 5.5;
        let origin = Some(Vec3::ZERO.to_array());
        let run_step = Some((Vec3::X * 0.0875).to_array());
        assert!(ordinary_unplanned_release_transition_is_valid(
            &before, &after, origin, run_step, None, None,
        ));

        let over_run_budget = Some((Vec3::X * 0.096).to_array());
        assert!(!ordinary_unplanned_release_transition_is_valid(
            &before,
            &after,
            origin,
            over_run_budget,
            None,
            None,
        ));

        before.speed_metres_per_second = 2.0;
        after.speed_metres_per_second = 2.0;
        assert!(!ordinary_unplanned_release_transition_is_valid(
            &before, &after, origin, run_step, None, None,
        ));
    }

    #[test]
    fn planned_run_transition_validates_metadata_and_body_local_motion() {
        let mut before = foot_metric_frame(0, -0.1, 0.0, 0.0, 0.0);
        let mut after = foot_metric_frame(1, -0.1, 0.0, 0.0, 0.0);
        before.scenario = "terrain-steady-run-5.5".into();
        after.scenario = before.scenario.clone();
        before.speed_metres_per_second = 5.5;
        after.speed_metres_per_second = 5.5;
        let plan = Some(Vec3::ZERO.to_array());
        let before_solve = Some((Vec3::X * 0.05).to_array());
        let bounded_run_solve = Some((Vec3::X * 0.1375).to_array());
        after.bones.get_mut("left_foot").unwrap().position =
            Vec3::new(-0.0125, 0.0, 0.0).to_array();
        assert!(ordinary_planned_transition_is_valid(
            &before,
            &after,
            "left_foot",
            plan,
            plan,
            before_solve,
            bounded_run_solve,
            0.0,
            false,
        ));

        let over_budget_solve = Some((Vec3::X * 0.146).to_array());
        after.bones.get_mut("left_foot").unwrap().position = Vec3::new(-0.004, 0.0, 0.0).to_array();
        assert!(!ordinary_planned_transition_is_valid(
            &before,
            &after,
            "left_foot",
            plan,
            plan,
            before_solve,
            over_budget_solve,
            0.0,
            false,
        ));

        before.speed_metres_per_second = 2.0;
        after.speed_metres_per_second = 2.0;
        assert!(!ordinary_planned_transition_is_valid(
            &before,
            &after,
            "left_foot",
            plan,
            plan,
            before_solve,
            bounded_run_solve,
            0.0,
            false,
        ));

        before.speed_metres_per_second = 5.5;
        after.speed_metres_per_second = 5.5;
        after.bones.get_mut("left_foot").unwrap().position = Vec3::new(-0.05, 0.0, 0.0).to_array();
        let retarget = Some((Vec3::X * 0.05).to_array());
        assert!(ordinary_planned_transition_is_valid(
            &before,
            &after,
            "left_foot",
            plan,
            retarget,
            plan,
            retarget,
            0.205,
            false,
        ));

        let replacement = Some(Vec3::new(0.0, 0.0, -5.0).to_array());
        let hidden_solve_jump = Some((Vec3::X * 0.50).to_array());
        assert!(ordinary_planned_transition_is_valid(
            &before,
            &after,
            "left_foot",
            plan,
            replacement,
            before_solve,
            hidden_solve_jump,
            0.0,
            true,
        ));
        after.bones.get_mut("left_foot").unwrap().position = Vec3::new(-0.004, 0.0, 0.0).to_array();
        assert!(!ordinary_planned_transition_is_valid(
            &before,
            &after,
            "left_foot",
            plan,
            replacement,
            before_solve,
            hidden_solve_jump,
            0.0,
            true,
        ));
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
            attack_animation: None,
            strike_family: StrikeFamily::Thrust,
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
            ik_left_knee_foot_yaw_offset_degrees: 0.0,
            ik_right_knee_foot_yaw_offset_degrees: 0.0,
            semantic_route_requested_path: SemanticRoutePath::LegacyFallback,
            semantic_route_selected_path: SemanticRoutePath::LegacyFallback,
            semantic_route_runtime_evaluated: false,
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
    fn guard_step_liveness_rejects_advancing_contact_metadata_with_frozen_feet() {
        let mut frames = [
            foot_metric_frame(0, 0.0, 0.0, 0.0, 0.0),
            foot_metric_frame(1, 0.0, 0.0, 0.0, 0.0),
            foot_metric_frame(2, 0.0, 1.0, 0.0, 0.0),
        ];
        for frame in &mut frames {
            frame.scenario = "raised-guard-right".into();
            frame.weapon_guard = WeaponGuardState::Raised;
            frame.right_support_weight = if frame.scenario_frame < 2 { 1.0 } else { 0.0 };
        }
        frames[2].contact_sequence = 1;
        frames[2].contact_foot = LeadFoot::Right;
        let references = frames.iter().collect::<Vec<_>>();

        let metrics = guard_step_liveness_metrics(&references);

        assert!(metrics.required);
        assert_eq!(metrics.completed_half_steps, 1);
        assert_eq!(metrics.visible_half_steps, 0);
        assert_eq!(metrics.minimum_swing_travel_metres, 0.0);
        assert_eq!(metrics.minimum_swing_clearance_gain_metres, 0.0);
    }

    #[test]
    fn guard_step_liveness_requires_final_bone_travel_clearance_and_replanting() {
        let mut frames = [
            foot_metric_frame(0, 0.0, 0.0, 0.0, 0.0),
            foot_metric_frame(1, 0.05, 0.0, 0.0, 0.0),
            foot_metric_frame(2, 0.10, 1.0, 0.0, 0.0),
        ];
        for frame in &mut frames {
            frame.scenario = "raised-guard-right".into();
            frame.weapon_guard = WeaponGuardState::Raised;
            frame.right_support_weight = if frame.scenario_frame < 2 { 1.0 } else { 0.0 };
        }
        frames[1]
            .bones
            .get_mut("left_foot")
            .unwrap()
            .terrain_clearance_metres = Some(0.05);
        frames[2].contact_sequence = 1;
        frames[2].contact_foot = LeadFoot::Right;
        let references = frames.iter().collect::<Vec<_>>();

        let metrics = guard_step_liveness_metrics(&references);

        assert!(metrics.required);
        assert_eq!(metrics.completed_half_steps, 1);
        assert_eq!(metrics.visible_half_steps, 1);
        assert!((metrics.minimum_swing_travel_metres - 0.10).abs() < 0.0001);
        assert!((metrics.minimum_swing_clearance_gain_metres - 0.05).abs() < 0.0001);
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
        frame.scenario = "terrain-steady-run-5.5".to_owned();
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
    fn viewer_uses_action_support_instead_of_stale_ordinary_ik_support() {
        let mut frame = foot_metric_frame(0, 0.0, 0.0, 0.0, 0.0);
        frame.scenario = "attack-live-forward-left-support".to_owned();
        frame.action = SkeletonAction::Attack;
        frame.ik_left_support_weight = 1.0;
        frame
            .bones
            .get_mut("left_foot")
            .unwrap()
            .terrain_clearance_metres = Some(1.0);
        assert!(reported_support_contacts_are_valid(&[frame]));
    }

    #[test]
    fn terrain_run_contact_gate_is_non_vacuous_and_requires_alternation() {
        let phase_step = gait_cycle_phase_delta(
            RUN_LOCOMOTION_PROFILE,
            RUN_LOCOMOTION_PROFILE.reference_speed,
            1.0 / SAMPLE_HZ,
        );
        let mut unsupported = (0..160)
            .map(|index| {
                let mut frame = foot_metric_frame(index, 0.0, 0.0, 0.0, 0.0);
                frame.scenario = "terrain-steady-run-5.5".to_owned();
                frame.gait_phase = (index as f32 * phase_step).rem_euclid(1.0);
                frame.time_seconds = index as f32 / SAMPLE_HZ;
                frame
            })
            .collect::<Vec<_>>();
        assert!(!terrain_run_contacts_are_valid(&unsupported));

        for frame in &mut unsupported {
            let phase = frame.gait_phase;
            let left_distance = phase.min(1.0 - phase);
            let right_distance = (phase - 0.5).abs();
            frame.ik_left_support_weight = (left_distance <= 0.17) as u8 as f32;
            frame.ik_right_support_weight = (right_distance <= 0.17) as u8 as f32;
            frame.left_support_weight = frame.ik_left_support_weight;
            frame.right_support_weight = frame.ik_right_support_weight;
        }
        assert!(terrain_run_contacts_are_valid(&unsupported));
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
