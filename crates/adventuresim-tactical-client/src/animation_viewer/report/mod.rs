//! Capture report assembly and artifact writing.

use super::*;

mod html;
mod metrics;
mod model;
mod validation;

use html::*;
use metrics::*;
pub(super) use model::*;
use validation::*;

pub(super) fn invalidate_previous_report(output: &std::path::Path) {
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

pub(super) fn write_completed_capture(completed: CompletedCapture) -> AppExit {
    let CompletedReport {
        output,
        manifest,
        global_bone_frames,
        acceptance_passed,
    } = build_completed_report(completed);
    let global_bone_trace_path = output.join("global-bone-transforms.jsonl");
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
    let manifest_path = output.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("capture manifest must serialize"),
    )
    .unwrap_or_else(|error| panic!("failed to write {manifest_path:?}: {error}"));
    let index_path = output.join("index.html");
    fs::write(&index_path, review_html(&manifest))
        .unwrap_or_else(|error| panic!("failed to write {index_path:?}: {error}"));
    info!(path = ?index_path, "Animation review capture completed");
    if acceptance_passed {
        AppExit::Success
    } else {
        let failure_path = output.join("failure.txt");
        fs::write(
            &failure_path,
            "capture failed artifact/completeness/continuity/biomechanics/penetration/distinct-view validation; inspect manifest.json\n",
        )
        .unwrap_or_else(|error| panic!("failed to write {failure_path:?}: {error}"));
        AppExit::Error(1.try_into().expect("one is non-zero"))
    }
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
            secondary_upper_body_bone_count: 0,
            secondary_upper_body_mean_blend_weight: 0.0,
            secondary_upper_body_maximum_pose_lag_degrees: 0.0,
            secondary_upper_body_maximum_inertial_acceleration_radians_per_second_squared: 0.0,
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
            semantic_route_requested_path: SemanticRoutePath::GeneralPose,
            semantic_route_selected_path: SemanticRoutePath::GeneralPose,
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
}
