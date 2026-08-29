//! Serialized capture, validation, metric, and trace contracts.

use super::*;

pub(in crate::animation_viewer) const VIEWS: [CaptureView; 3] =
    [CaptureView::Gameplay, CaptureView::Side, CaptureView::Front];

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::animation_viewer) enum CaptureView {
    Gameplay,
    Side,
    Front,
}

impl CaptureView {
    pub(in crate::animation_viewer) fn slug(self) -> &'static str {
        match self {
            Self::Gameplay => "gameplay",
            Self::Side => "side",
            Self::Front => "front",
        }
    }
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct AnimationCaptureManifest {
    pub(in crate::animation_viewer) sample_hz: f32,
    pub(in crate::animation_viewer) playback_backend: &'static str,
    pub(in crate::animation_viewer) global_bone_trace: &'static str,
    pub(in crate::animation_viewer) pose_buffer: PoseBufferMetrics,
    pub(in crate::animation_viewer) pipeline: &'static str,
    pub(in crate::animation_viewer) views: [CaptureView; 3],
    pub(in crate::animation_viewer) validation: AnimationCaptureValidation,
    pub(in crate::animation_viewer) quality_score: QualityScore,
    pub(in crate::animation_viewer) scenarios: Vec<ScenarioMetrics>,
    pub(in crate::animation_viewer) frames: Vec<FrameSample>,
    pub(in crate::animation_viewer) presentation_events: Vec<PresentationEventSample>,
    pub(in crate::animation_viewer) semantic_route_path_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct QualityScore {
    pub(in crate::animation_viewer) weighted_defect_score: u8,
    pub(in crate::animation_viewer) maximum_weighted_defect_score: u8,
    pub(in crate::animation_viewer) quality_percent: f32,
    pub(in crate::animation_viewer) acceptance_passed: bool,
    pub(in crate::animation_viewer) categories: QualityCategories,
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct QualityCategories {
    pub(in crate::animation_viewer) catastrophic_foot_displacement_failed: bool,
    pub(in crate::animation_viewer) guard_step_liveness_failed: bool,
    pub(in crate::animation_viewer) anatomical_invalid_joints_failed: bool,
    pub(in crate::animation_viewer) contact_foot_airborne_failed: bool,
    pub(in crate::animation_viewer) both_feet_behind_hips_failed: bool,
    pub(in crate::animation_viewer) foot_dragging_failed: bool,
    pub(in crate::animation_viewer) jitter_and_jerk_failed: bool,
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct PresentationEventSample {
    pub(in crate::animation_viewer) scenario: String,
    pub(in crate::animation_viewer) scenario_frame: usize,
    pub(in crate::animation_viewer) owner: String,
    pub(in crate::animation_viewer) sequence: u64,
    pub(in crate::animation_viewer) sample_tick: u64,
    pub(in crate::animation_viewer) kind: String,
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct AnimationCaptureValidation {
    pub(in crate::animation_viewer) finite_transforms: bool,
    pub(in crate::animation_viewer) all_scenarios_complete: bool,
    pub(in crate::animation_viewer) all_artifacts_written: bool,
    pub(in crate::animation_viewer) continuity_within_review_bounds: bool,
    pub(in crate::animation_viewer) biomechanics_within_review_bounds: bool,
    pub(in crate::animation_viewer) no_ground_penetration: bool,
    pub(in crate::animation_viewer) raised_guard_fixed_support: bool,
    pub(in crate::animation_viewer) raised_guard_step_liveness_valid: bool,
    pub(in crate::animation_viewer) flat_controller_height_stable: bool,
    pub(in crate::animation_viewer) phase_owned_height_valid: bool,
    pub(in crate::animation_viewer) run_flight_valid: bool,
    pub(in crate::animation_viewer) body_response_valid: bool,
    pub(in crate::animation_viewer) upper_body_secondary_physics_valid: bool,
    pub(in crate::animation_viewer) straight_run_torso_sway_valid: bool,
    pub(in crate::animation_viewer) speed_ramp_phase_continuity_valid: bool,
    pub(in crate::animation_viewer) contact_sequences_valid: bool,
    pub(in crate::animation_viewer) cadence_contact_valid: bool,
    pub(in crate::animation_viewer) event_stream_valid: bool,
    pub(in crate::animation_viewer) landing_response_valid: bool,
    pub(in crate::animation_viewer) landing_foot_preservation_valid: bool,
    pub(in crate::animation_viewer) ordinary_swing_tracking_valid: bool,
    pub(in crate::animation_viewer) reported_support_contacts_valid: bool,
    pub(in crate::animation_viewer) run_contact_acquisition_valid: bool,
    pub(in crate::animation_viewer) stop_settle_capture_valid: bool,
    pub(in crate::animation_viewer) final_support_balance_valid: bool,
    pub(in crate::animation_viewer) hard_stop_maximum_pelvis_step_metres: Option<f32>,
    pub(in crate::animation_viewer) hard_stop_height_continuity_valid: bool,
    pub(in crate::animation_viewer) repeated_evaluation_valid: bool,
    pub(in crate::animation_viewer) semantic_route_paths_exercised: bool,
    pub(in crate::animation_viewer) jitter_validation: JitterValidationSummary,
    pub(in crate::animation_viewer) views_are_distinct: bool,
    pub(in crate::animation_viewer) duplicate_view_frames: Vec<String>,
    pub(in crate::animation_viewer) note: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct ScenarioMetrics {
    pub(in crate::animation_viewer) scenario: String,
    pub(in crate::animation_viewer) frame_count: usize,
    pub(in crate::animation_viewer) maximum_root_relative_step_metres: f32,
    pub(in crate::animation_viewer) maximum_leg_root_relative_step_metres: f32,
    pub(in crate::animation_viewer) maximum_foot_root_relative_step_metres: f32,
    pub(in crate::animation_viewer) maximum_knee_root_relative_step_metres: f32,
    pub(in crate::animation_viewer) worst_displacement: Option<ContinuityLocation>,
    pub(in crate::animation_viewer) maximum_bone_rotation_step_degrees: f32,
    pub(in crate::animation_viewer) maximum_foot_rotation_step_degrees: f32,
    pub(in crate::animation_viewer) worst_rotation: Option<ContinuityLocation>,
    pub(in crate::animation_viewer) loop_seam_position_metres: Option<f32>,
    pub(in crate::animation_viewer) loop_seam_rotation_degrees: Option<f32>,
    pub(in crate::animation_viewer) pelvis_vertical_range_metres: f32,
    pub(in crate::animation_viewer) maximum_pelvis_vertical_step_metres: f32,
    pub(in crate::animation_viewer) controller_vertical_range_metres: f32,
    pub(in crate::animation_viewer) phase_height_range_metres: f32,
    pub(in crate::animation_viewer) contact_to_passing_height_gain_metres: f32,
    pub(in crate::animation_viewer) visual_height_peak_count: usize,
    pub(in crate::animation_viewer) visual_height_peaks_in_passing_windows: bool,
    pub(in crate::animation_viewer) maximum_no_support_seconds: f32,
    pub(in crate::animation_viewer) minimum_flight_sole_clearance_metres: f32,
    pub(in crate::animation_viewer) minimum_contact_sole_clearance_metres: f32,
    pub(in crate::animation_viewer) maximum_contact_sole_clearance_metres: f32,
    pub(in crate::animation_viewer) minimum_flight_toe_clearance_metres: f32,
    pub(in crate::animation_viewer) minimum_contact_toe_clearance_metres: f32,
    pub(in crate::animation_viewer) head_vertical_range_metres: f32,
    pub(in crate::animation_viewer) foot_terrain_relief_metres: f32,
    pub(in crate::animation_viewer) minimum_knee_forward_bend_metres: f32,
    pub(in crate::animation_viewer) minimum_signed_foot_track_metres: f32,
    pub(in crate::animation_viewer) minimum_inter_foot_separation_metres: f32,
    pub(in crate::animation_viewer) minimum_knee_flexion_degrees: f32,
    pub(in crate::animation_viewer) minimum_knee_hemisphere_dot: f32,
    pub(in crate::animation_viewer) maximum_knee_foot_yaw_offset_degrees: f32,
    pub(in crate::animation_viewer) maximum_facing_motion_error_degrees: f32,
    pub(in crate::animation_viewer) maximum_facing_tracking_excess_degrees: f32,
    pub(in crate::animation_viewer) maximum_guard_facing_error_degrees: f32,
    pub(in crate::animation_viewer) final_facing_motion_error_degrees: f32,
    pub(in crate::animation_viewer) maximum_dive_axis_motion_error_degrees: f32,
    pub(in crate::animation_viewer) maximum_supported_foot_slip_metres_per_frame: f32,
    pub(in crate::animation_viewer) maximum_planted_foot_drift_metres: f32,
    pub(in crate::animation_viewer) guard_step_liveness_required: bool,
    pub(in crate::animation_viewer) completed_guard_half_step_count: usize,
    pub(in crate::animation_viewer) visible_guard_half_step_count: usize,
    pub(in crate::animation_viewer) minimum_guard_swing_travel_metres: f32,
    pub(in crate::animation_viewer) minimum_guard_swing_clearance_gain_metres: f32,
    pub(in crate::animation_viewer) minimum_foot_clearance_metres: f32,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::animation_viewer) struct ContinuityLocation {
    pub(in crate::animation_viewer) bone: String,
    pub(in crate::animation_viewer) from_frame: usize,
    pub(in crate::animation_viewer) to_frame: usize,
    pub(in crate::animation_viewer) value: f32,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::animation_viewer) struct FrameSample {
    pub(in crate::animation_viewer) scenario: String,
    pub(in crate::animation_viewer) scenario_frame: usize,
    pub(in crate::animation_viewer) time_seconds: f32,
    pub(in crate::animation_viewer) speed_metres_per_second: f32,
    pub(in crate::animation_viewer) gait_phase: f32,
    pub(in crate::animation_viewer) locomotion_sample_tick: u64,
    pub(in crate::animation_viewer) body_acceleration: [f32; 3],
    pub(in crate::animation_viewer) world_acceleration: [f32; 3],
    pub(in crate::animation_viewer) secondary_upper_body_bone_count: u32,
    pub(in crate::animation_viewer) secondary_upper_body_mean_blend_weight: f32,
    pub(in crate::animation_viewer) secondary_upper_body_maximum_pose_lag_degrees: f32,
    pub(in crate::animation_viewer) secondary_upper_body_maximum_inertial_acceleration_radians_per_second_squared:
        f32,
    pub(in crate::animation_viewer) contact_sequence: u64,
    pub(in crate::animation_viewer) contact_foot: LeadFoot,
    pub(in crate::animation_viewer) landing_sequence: u64,
    pub(in crate::animation_viewer) landing_impact_speed: f32,
    pub(in crate::animation_viewer) body_lean_pitch_degrees: f32,
    pub(in crate::animation_viewer) body_lean_roll_degrees: f32,
    pub(in crate::animation_viewer) landing_compression_metres: f32,
    pub(in crate::animation_viewer) root_distance_metres: f32,
    pub(in crate::animation_viewer) root_position_metres: [f32; 3],
    pub(in crate::animation_viewer) world_travel_direction: [f32; 3],
    pub(in crate::animation_viewer) desired_body_forward_direction: [f32; 3],
    pub(in crate::animation_viewer) body_forward_direction: [f32; 3],
    pub(in crate::animation_viewer) body_rotation_xyzw: [f32; 4],
    pub(in crate::animation_viewer) weapon_guard: WeaponGuardState,
    pub(in crate::animation_viewer) lead_foot: LeadFoot,
    pub(in crate::animation_viewer) action: SkeletonAction,
    pub(in crate::animation_viewer) action_phase: f32,
    pub(in crate::animation_viewer) attack_animation: Option<AttackAnimation>,
    pub(in crate::animation_viewer) strike_family: StrikeFamily,
    pub(in crate::animation_viewer) guard_action: bool,
    pub(in crate::animation_viewer) left_support_weight: f32,
    pub(in crate::animation_viewer) right_support_weight: f32,
    pub(in crate::animation_viewer) desired_left_foot_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) desired_right_foot_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_left_authored_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_right_authored_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_left_planned_contact: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_right_planned_contact: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_settle_capture_point: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_left_solve_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_right_solve_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_left_support_weight: f32,
    pub(in crate::animation_viewer) ik_right_support_weight: f32,
    pub(in crate::animation_viewer) ik_left_release_active: bool,
    pub(in crate::animation_viewer) ik_right_release_active: bool,
    pub(in crate::animation_viewer) ik_left_release_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_right_release_target: Option<[f32; 3]>,
    pub(in crate::animation_viewer) ik_settle_progress: Option<f32>,
    pub(in crate::animation_viewer) ik_left_knee_foot_yaw_offset_degrees: f32,
    pub(in crate::animation_viewer) ik_right_knee_foot_yaw_offset_degrees: f32,
    pub(in crate::animation_viewer) semantic_route_requested_path: SemanticRoutePath,
    pub(in crate::animation_viewer) semantic_route_selected_path: SemanticRoutePath,
    pub(in crate::animation_viewer) semantic_route_runtime_evaluated: bool,
    pub(in crate::animation_viewer) screenshots: BTreeMap<String, String>,
    pub(in crate::animation_viewer) bones: BTreeMap<String, BoneSample>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(in crate::animation_viewer) struct BoneSample {
    pub(in crate::animation_viewer) position: [f32; 3],
    pub(in crate::animation_viewer) rotation_xyzw: [f32; 4],
    pub(in crate::animation_viewer) terrain_clearance_metres: Option<f32>,
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct GlobalBoneFrame {
    pub(in crate::animation_viewer) scenario: String,
    pub(in crate::animation_viewer) scenario_frame: usize,
    pub(in crate::animation_viewer) time_seconds: f32,
    pub(in crate::animation_viewer) action: SkeletonAction,
    pub(in crate::animation_viewer) action_phase: f32,
    pub(in crate::animation_viewer) subject_translation: [f32; 3],
    pub(in crate::animation_viewer) subject_rotation_xyzw: [f32; 4],
    pub(in crate::animation_viewer) bones: Vec<GlobalBoneTransformSample>,
}

#[derive(Debug, Serialize)]
pub(in crate::animation_viewer) struct GlobalBoneTransformSample {
    pub(in crate::animation_viewer) name: String,
    pub(in crate::animation_viewer) target_id: String,
    pub(in crate::animation_viewer) translation: [f32; 3],
    pub(in crate::animation_viewer) rotation_xyzw: [f32; 4],
    pub(in crate::animation_viewer) scale: [f32; 3],
}

pub(super) struct CompletedReport {
    pub(super) output: PathBuf,
    pub(super) manifest: AnimationCaptureManifest,
    pub(super) global_bone_frames: Vec<GlobalBoneFrame>,
    pub(super) acceptance_passed: bool,
}
