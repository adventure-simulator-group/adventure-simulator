//! A network-free viewer that captures the real tactical animation pipeline.

use std::{fs, path::PathBuf};

use adventuresim_tactical_core::prelude::*;
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};

use crate::animation::{AnimationPlayback, BoneRole, HumanoidBone, TacticalAnimationPlugin};

const WALK_PHASES: [f32; 8] = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875];

pub(crate) fn run(output: PathBuf, asset_root: PathBuf, frames_per_sample: u32) {
    fs::create_dir_all(&output).unwrap_or_else(|error| {
        panic!("failed to create animation capture directory {output:?}: {error}")
    });
    let _ = fs::remove_file(output.join("failure.txt"));

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Adventure Simulator Animation Viewer".into(),
                        resolution: (960, 720).into(),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(TacticalAnimationPlugin)
        .insert_resource(ClearColor(Color::srgb(0.08, 0.1, 0.13)))
        .insert_resource(CaptureSequence::new(output, frames_per_sample))
        .add_systems(Startup, setup_viewer)
        .add_systems(PreUpdate, drive_capture_phase)
        .add_systems(Last, capture_settled_phase)
        .run();
}

#[derive(Component)]
struct CaptureSubject;

#[derive(Component)]
struct CaptureLabel;

#[derive(Resource)]
struct CaptureSequence {
    output: PathBuf,
    frames_per_sample: u32,
    index: usize,
    settled_frames: u32,
    phase_applied: bool,
    capture_in_flight: bool,
    waiting_frames: u32,
    samples: Vec<CaptureSample>,
}

impl CaptureSequence {
    fn new(output: PathBuf, frames_per_sample: u32) -> Self {
        Self {
            output,
            frames_per_sample,
            index: 0,
            settled_frames: 0,
            phase_applied: false,
            capture_in_flight: false,
            waiting_frames: 0,
            samples: Vec::with_capacity(WALK_PHASES.len()),
        }
    }
}

#[derive(Debug)]
struct CaptureManifest {
    animation: &'static str,
    frames_per_sample: u32,
    validation: CaptureValidation,
    samples: Vec<CaptureSample>,
}

#[derive(Debug)]
struct CaptureValidation {
    complete_cycle: bool,
    foot_separation_range: f32,
    foot_lead_changes: usize,
}

#[derive(Debug)]
struct CaptureSample {
    index: usize,
    gait_phase: f32,
    lower_body_mirror: f32,
    screenshot: String,
    left_knee: Option<[f32; 3]>,
    right_knee: Option<[f32; 3]>,
    left_foot: Option<[f32; 3]>,
    right_foot: Option<[f32; 3]>,
}

fn setup_viewer(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn(SceneTerrain::new(16, 16, 1.0, |_| 0.0));
    commands.spawn((
        Name::new("Animation viewer floor"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(16.0, 16.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.28, 0.2),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    commands.spawn((
        Name::new("Animation capture subject"),
        CaptureSubject,
        Player {
            name: "Animation viewer".into(),
        },
        CharacterLook::default(),
        SkeletonState {
            local_velocity: Vec3::Z * 1.5,
            grounded: true,
            ..default()
        },
        Transform::from_xyz(0.0, 0.95, 0.0),
        Visibility::Inherited,
    ));

    commands.spawn((
        Name::new("Animation viewer sun"),
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 12_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 7.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn(AmbientLight {
        color: Color::WHITE,
        brightness: 350.0,
        affects_lightmapped_meshes: true,
    });
    commands.spawn((
        Name::new("Animation viewer camera"),
        Camera3d::default(),
        Transform::from_xyz(3.2, 2.1, 4.5).looking_at(Vec3::new(0.0, 0.95, 0.0), Vec3::Y),
    ));
    commands.spawn((
        CaptureLabel,
        Text::new("Loading authored animation rig..."),
        TextFont::from_font_size(24.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
}

fn drive_capture_phase(
    mut sequence: ResMut<CaptureSequence>,
    mut subjects: Query<&mut SkeletonState, With<CaptureSubject>>,
    mut labels: Query<&mut Text, With<CaptureLabel>>,
) {
    if sequence.phase_applied || sequence.capture_in_flight || sequence.index >= WALK_PHASES.len() {
        return;
    }
    let phase = WALK_PHASES[sequence.index];
    for mut skeleton in &mut subjects {
        skeleton.gait_phase = phase;
    }
    for mut label in &mut labels {
        **label = format!(
            "walk | phase {phase:.3} | sample {} / {}",
            sequence.index + 1,
            WALK_PHASES.len()
        );
    }
    sequence.phase_applied = true;
    sequence.settled_frames = 0;
}

fn capture_settled_phase(
    mut commands: Commands,
    mut sequence: ResMut<CaptureSequence>,
    subjects: Query<(Entity, &SkeletonState, Option<&AnimationPlayback>), With<CaptureSubject>>,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
    mut exit: MessageWriter<AppExit>,
) {
    if !sequence.phase_applied || sequence.capture_in_flight {
        return;
    }
    let Ok((subject, skeleton, playback)) = subjects.single() else {
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
            "authored walk clip has not resolved",
            &mut exit,
        );
        return;
    }
    let mut left_knee = None;
    let mut right_knee = None;
    let mut left_foot = None;
    let mut right_foot = None;
    for (bone, transform) in &bones {
        if bone.owner != subject {
            continue;
        }
        let position = transform.translation().to_array();
        match bone.role {
            BoneRole::ShinLeft => left_knee = Some(position),
            BoneRole::ShinRight => right_knee = Some(position),
            BoneRole::FootLeft => left_foot = Some(position),
            BoneRole::FootRight => right_foot = Some(position),
            _ => {}
        }
    }
    if left_foot.is_none() || right_foot.is_none() {
        wait_or_fail(
            &mut sequence,
            "authored rig is missing bound foot bones",
            &mut exit,
        );
        return;
    }

    sequence.waiting_frames = 0;
    sequence.settled_frames += 1;
    let required_frames = if sequence.index == 0 {
        sequence.frames_per_sample.max(60)
    } else {
        sequence.frames_per_sample
    };
    if sequence.settled_frames < required_frames {
        return;
    }

    let index = sequence.index;
    let phase = skeleton.gait_phase;
    let file_name = format!("walk-{index:02}-phase-{phase:.3}.png");
    let path = sequence.output.join(&file_name);
    sequence.samples.push(CaptureSample {
        index,
        gait_phase: phase,
        lower_body_mirror: playback.lower_body_mirror,
        screenshot: file_name,
        left_knee,
        right_knee,
        left_foot,
        right_foot,
    });
    sequence.capture_in_flight = true;

    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              mut sequence: ResMut<CaptureSequence>,
              mut exit: MessageWriter<AppExit>| {
            save_to_disk(&path)(captured);
            sequence.index += 1;
            sequence.phase_applied = false;
            sequence.capture_in_flight = false;
            if sequence.index == WALK_PHASES.len() {
                let validation = CaptureValidation::from_samples(&sequence.samples);
                let manifest = CaptureManifest {
                    animation: "walk",
                    frames_per_sample: sequence.frames_per_sample,
                    validation,
                    samples: std::mem::take(&mut sequence.samples),
                };
                let manifest_path = sequence.output.join("manifest.json");
                fs::write(&manifest_path, manifest.to_json())
                    .unwrap_or_else(|error| panic!("failed to write {manifest_path:?}: {error}"));
                info!(path = ?manifest_path, "Animation capture completed");
                if manifest.validation.complete_cycle {
                    exit.write(AppExit::Success);
                } else {
                    let failure_path = sequence.output.join("failure.txt");
                    fs::write(
                        &failure_path,
                        "captured foot coordinates do not traverse a complete gait cycle\n",
                    )
                    .unwrap_or_else(|error| panic!("failed to write {failure_path:?}: {error}"));
                    exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
                }
            }
        },
    );
}

fn wait_or_fail(sequence: &mut CaptureSequence, reason: &str, exit: &mut MessageWriter<AppExit>) {
    sequence.waiting_frames += 1;
    if sequence.waiting_frames < 600 {
        return;
    }
    let message = format!(
        "animation viewer timed out after {} rendered frames: {reason}\n",
        sequence.waiting_frames
    );
    let path = sequence.output.join("failure.txt");
    fs::write(&path, &message).unwrap_or_else(|error| panic!("failed to write {path:?}: {error}"));
    error!(%reason, path = ?path, "Animation capture failed");
    exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
}

impl CaptureManifest {
    fn to_json(&self) -> String {
        let samples = self
            .samples
            .iter()
            .map(CaptureSample::to_json)
            .collect::<Vec<_>>()
            .join(",\n");
        format!(
            concat!(
                "{{\n",
                "  \"animation\": \"{}\",\n",
                "  \"frames_per_sample\": {},\n",
                "  \"validation\": {{\n",
                "    \"complete_cycle\": {},\n",
                "    \"foot_separation_range\": {:.6},\n",
                "    \"foot_lead_changes\": {}\n",
                "  }},\n",
                "  \"samples\": [\n{}\n  ]\n",
                "}}\n"
            ),
            self.animation,
            self.frames_per_sample,
            self.validation.complete_cycle,
            self.validation.foot_separation_range,
            self.validation.foot_lead_changes,
            samples
        )
    }
}

impl CaptureValidation {
    fn from_samples(samples: &[CaptureSample]) -> Self {
        let separations = samples
            .iter()
            .filter_map(CaptureSample::foot_separation)
            .collect::<Vec<_>>();
        let (minimum, maximum) = separations.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), &value| (minimum.min(value), maximum.max(value)),
        );
        let foot_separation_range = if minimum.is_finite() && maximum.is_finite() {
            maximum - minimum
        } else {
            0.0
        };
        let foot_lead_changes = separations
            .iter()
            .zip(separations.iter().cycle().skip(1))
            .take(separations.len())
            .filter(|(before, after)| before.signum() != after.signum())
            .count();
        Self {
            complete_cycle: separations.len() == WALK_PHASES.len()
                && foot_separation_range > 0.5
                && foot_lead_changes >= 2,
            foot_separation_range,
            foot_lead_changes,
        }
    }
}

impl CaptureSample {
    fn foot_separation(&self) -> Option<f32> {
        Some(self.left_foot?[2] - self.right_foot?[2])
    }

    fn to_json(&self) -> String {
        format!(
            concat!(
                "    {{\n",
                "      \"index\": {},\n",
                "      \"gait_phase\": {:.6},\n",
                "      \"lower_body_mirror\": {:.6},\n",
                "      \"screenshot\": \"{}\",\n",
                "      \"left_knee\": {},\n",
                "      \"right_knee\": {},\n",
                "      \"left_foot\": {},\n",
                "      \"right_foot\": {}\n",
                "    }}"
            ),
            self.index,
            self.gait_phase,
            self.lower_body_mirror,
            self.screenshot,
            json_vec3(self.left_knee),
            json_vec3(self.right_knee),
            json_vec3(self.left_foot),
            json_vec3(self.right_foot),
        )
    }
}

fn json_vec3(value: Option<[f32; 3]>) -> String {
    value.map_or_else(
        || "null".to_owned(),
        |[x, y, z]| format!("[{x:.6}, {y:.6}, {z:.6}]"),
    )
}
