//! A network-free, reviewer-oriented capture of the real tactical animation pipeline.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use adventuresim_tactical_core::prelude::*;
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::animation::{
    AnimationPlayback, BoneRole, HumanoidBone, TacticalAnimationPlugin, gait_support_weights,
};

const SAMPLE_HZ: f32 = 60.0;
const VIEWS: [CaptureView; 3] = [
    CaptureView::ThirdPerson,
    CaptureView::Side,
    CaptureView::Front,
];
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

pub(crate) fn run(output: PathBuf, asset_root: PathBuf, settle_frames: u32) -> AppExit {
    fs::create_dir_all(&output).unwrap_or_else(|error| {
        panic!("failed to create animation capture directory {output:?}: {error}")
    });
    invalidate_previous_report(&output);

    App::new()
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
        .add_plugins(TacticalAnimationPlugin)
        .insert_resource(ClearColor(Color::srgb(0.08, 0.1, 0.13)))
        .insert_resource(CaptureSequence::new(output, settle_frames))
        .add_systems(Startup, setup_viewer)
        .add_systems(PreUpdate, (drive_sequence, position_capture_camera).chain())
        .add_systems(
            PostUpdate,
            draw_skeleton_overlay.after(TransformSystems::Propagate),
        )
        .add_systems(Last, capture_frame)
        .run()
}

#[derive(Component)]
struct CaptureSubject;

#[derive(Component)]
struct CaptureCamera;

#[derive(Component)]
struct CaptureLabel;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaptureView {
    ThirdPerson,
    Side,
    Front,
}

impl CaptureView {
    fn slug(self) -> &'static str {
        match self {
            Self::ThirdPerson => "third-person",
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
    gait_phase: f32,
    distance: f32,
    time_seconds: f32,
    local_direction: Vec2,
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
}

impl CaptureSequence {
    fn new(output: PathBuf, settle_frames: u32) -> Self {
        Self {
            output,
            settle_frames: settle_frames.max(1),
            plan: capture_plan(),
            index: 0,
            view_index: 0,
            applied: false,
            settled: 0,
            waiting: 0,
            capture_in_flight: false,
            view_fingerprints: Vec::with_capacity(VIEWS.len()),
            duplicate_view_frames: Vec::new(),
            samples: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct CaptureManifest {
    sample_hz: f32,
    pipeline: &'static str,
    views: [CaptureView; 3],
    validation: CaptureValidation,
    scenarios: Vec<ScenarioMetrics>,
    frames: Vec<FrameSample>,
}

#[derive(Debug, Serialize)]
struct CaptureValidation {
    finite_transforms: bool,
    all_scenarios_complete: bool,
    all_artifacts_written: bool,
    continuity_within_review_bounds: bool,
    biomechanics_within_review_bounds: bool,
    no_ground_penetration: bool,
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
    worst_displacement: Option<ContinuityLocation>,
    maximum_bone_rotation_step_degrees: f32,
    worst_rotation: Option<ContinuityLocation>,
    loop_seam_position_metres: Option<f32>,
    loop_seam_rotation_degrees: Option<f32>,
    pelvis_vertical_range_metres: f32,
    head_vertical_range_metres: f32,
    minimum_knee_forward_bend_metres: f32,
    maximum_supported_foot_slip_metres_per_frame: f32,
    maximum_planted_foot_drift_metres: f32,
    minimum_foot_height_metres: f32,
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
    root_distance_metres: f32,
    root_position_metres: [f32; 3],
    world_travel_direction: [f32; 3],
    left_support_weight: f32,
    right_support_weight: f32,
    screenshots: BTreeMap<String, String>,
    bones: BTreeMap<String, BoneSample>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct BoneSample {
    position: [f32; 3],
    rotation_xyzw: [f32; 4],
}

fn stride_length(speed: f32) -> f32 {
    (0.9 + speed * 0.16).clamp(0.9, 1.8)
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
    let cycle_duration = stride_length(speed) / speed;
    let duration = cycles * cycle_duration;
    let last_regular_frame = (duration * SAMPLE_HZ).floor() as usize;
    let mut times = (0..=last_regular_frame)
        .map(|frame| (frame as f32 / SAMPLE_HZ, false))
        .collect::<Vec<_>>();
    for cycle in 1..=cycles.round() as usize {
        times.push((cycle as f32 * cycle_duration, true));
    }
    times.sort_by(|a, b| a.0.total_cmp(&b.0));
    times.dedup_by(|a, b| {
        if (a.0 - b.0).abs() < 0.00001 {
            b.1 |= a.1;
            true
        } else {
            false
        }
    });
    times
        .into_iter()
        .enumerate()
        .map(|(scenario_frame, (time_seconds, closure))| PlannedFrame {
            scenario: name,
            scenario_frame,
            speed,
            gait_phase: if closure {
                0.0
            } else {
                (time_seconds / cycle_duration).rem_euclid(1.0)
            },
            distance: speed * time_seconds,
            time_seconds,
            local_direction,
        })
        .collect()
}

fn transition_scenario() -> Vec<PlannedFrame> {
    let duration = 4.0;
    let last_frame = (duration * SAMPLE_HZ) as usize;
    let mut phase = 0.0;
    let mut distance = 0.0;
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
            if frame > 0 {
                phase = (phase + speed / stride_length(speed) / SAMPLE_HZ).rem_euclid(1.0);
                distance += speed / SAMPLE_HZ;
            }
            PlannedFrame {
                scenario: "start-stop-transition",
                scenario_frame: frame,
                speed,
                gait_phase: phase,
                distance,
                time_seconds: t,
                local_direction: Vec2::NEG_Y,
            }
        })
        .collect()
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn capture_plan() -> Vec<PlannedFrame> {
    [
        steady_scenario("steady-walk-2.0", 2.0, 2.0),
        steady_scenario("walk-run-blend-3.75", 3.75, 2.0),
        steady_scenario("steady-run-5.5", 5.5, 2.0),
        steady_scenario_in_direction("lateral-walk-2.0", 2.0, 1.0, Vec2::X),
        steady_scenario_in_direction("reverse-walk-2.0", 2.0, 1.0, Vec2::Y),
        transition_scenario(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn setup_viewer(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let terrain = SceneTerrain::new(64, 24, 1.0, |position| {
        let centered = position - Vec2::new(32.0, 12.0);
        // Deliberately visible, deterministic uneven ground. Its broad slope
        // and smaller cross-wave expose overreaching leg solves and incorrect
        // foot normals that the previous three-centimetre ripple concealed.
        (centered.y * 0.22).sin() * 0.22
            + (centered.x * 0.31).sin() * 0.10
            + ((centered.x + centered.y) * 0.55).sin() * 0.035
    });
    let terrain_mesh = terrain.mesh();
    commands.spawn(terrain);
    commands.spawn((
        Name::new("Animation review floor"),
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.23, 0.16),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    let grid_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.76, 0.58),
        emissive: LinearRgba::new(0.08, 0.09, 0.05, 1.0),
        perceptual_roughness: 1.0,
        ..default()
    });
    let cross_line = meshes.add(Cuboid::new(8.0, 0.008, 0.025));
    for metre in -2..=20 {
        commands.spawn((
            Name::new(format!("World grid metre {metre}")),
            Mesh3d(cross_line.clone()),
            MeshMaterial3d(grid_material.clone()),
            Transform::from_xyz(0.0, 0.006, metre as f32),
        ));
    }
    commands.spawn((
        Name::new("World grid center line"),
        Mesh3d(meshes.add(Cuboid::new(0.025, 0.009, 24.0))),
        MeshMaterial3d(grid_material),
        Transform::from_xyz(0.0, 0.008, 9.0),
    ));

    commands.spawn((
        Name::new("Animation review subject"),
        CaptureSubject,
        Player {
            name: "Animation review".into(),
        },
        CharacterLook::default(),
        SkeletonState {
            local_velocity: Vec3::NEG_Z * 2.0,
            grounded: true,
            ..default()
        },
        Transform::from_xyz(0.0, 0.95, 0.0),
        Visibility::Inherited,
    ));
    commands.spawn((
        Name::new("Animation review sun"),
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
        Name::new("Animation review camera"),
        CaptureCamera,
        Camera3d::default(),
        Transform::from_xyz(3.2, 2.1, -4.5).looking_at(Vec3::Y, Vec3::Y),
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
    terrain: Single<&SceneTerrain>,
    mut subjects: Query<(&mut SkeletonState, &mut Transform), With<CaptureSubject>>,
    mut labels: Query<&mut Text, With<CaptureLabel>>,
) {
    if sequence.applied || sequence.capture_in_flight || sequence.index >= sequence.plan.len() {
        return;
    }
    let frame = sequence.plan[sequence.index].clone();
    for (mut skeleton, mut transform) in &mut subjects {
        skeleton.local_velocity =
            Vec3::new(frame.local_direction.x, 0.0, frame.local_direction.y) * frame.speed;
        skeleton.gait_phase = frame.gait_phase;
        skeleton.grounded = true;
        skeleton.posture = Posture::Upright;
        // Gameplay's controller frame is half-turned relative to the authored
        // mesh, so world travel is the negative local velocity direction.
        let horizontal = -frame.local_direction * frame.distance;
        let ground = terrain.height_at(horizontal).unwrap_or(0.0);
        transform.translation = Vec3::new(horizontal.x, ground + 0.95, horizontal.y);
    }
    for mut label in &mut labels {
        **label = format!(
            "{} | {:>4.2} m/s | phase {:>5.3} | {} view | 60 Hz frame {}",
            frame.scenario,
            frame.speed,
            frame.gait_phase,
            VIEWS[sequence.view_index].slug(),
            frame.scenario_frame,
        );
    }
    sequence.applied = true;
    sequence.settled = 0;
}

fn position_capture_camera(
    sequence: Res<CaptureSequence>,
    subjects: Query<&Transform, With<CaptureSubject>>,
    mut cameras: Query<&mut Transform, (With<CaptureCamera>, Without<CaptureSubject>)>,
    mut labels: Query<&mut Text, With<CaptureLabel>>,
) {
    let (Ok(subject), Ok(mut camera)) = (subjects.single(), cameras.single_mut()) else {
        return;
    };
    let focus = subject.translation + Vec3::Y * 0.95;
    let view = VIEWS[sequence.view_index.min(VIEWS.len() - 1)];
    camera.translation = focus
        + match view {
            CaptureView::ThirdPerson => Vec3::new(3.2, 1.6, 4.5),
            CaptureView::Side => Vec3::new(5.0, 0.45, 0.0),
            CaptureView::Front => Vec3::new(0.0, 0.45, -5.0),
        };
    camera.look_at(focus, Vec3::Y);
    if sequence.applied
        && let Some(frame) = sequence.plan.get(sequence.index)
    {
        for mut label in &mut labels {
            **label = format!(
                "{} | {:>4.2} m/s | phase {:>5.3} | {} view | 60 Hz frame {}",
                frame.scenario,
                frame.speed,
                frame.gait_phase,
                view.slug(),
                frame.scenario_frame,
            );
        }
    }
}

fn draw_skeleton_overlay(
    mut gizmos: Gizmos,
    subjects: Query<(Entity, &SkeletonState), With<CaptureSubject>>,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
) {
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
    let (left_support, right_support) =
        gait_support_weights(skeleton.gait_phase, skeleton.local_velocity.xz().length());
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

fn capture_frame(
    mut commands: Commands,
    mut sequence: ResMut<CaptureSequence>,
    subjects: Query<
        (
            Entity,
            &SkeletonState,
            &GlobalTransform,
            Option<&AnimationPlayback>,
        ),
        With<CaptureSubject>,
    >,
    bones: Query<(&HumanoidBone, &GlobalTransform)>,
    mut exit: MessageWriter<AppExit>,
) {
    if !sequence.applied || sequence.capture_in_flight {
        return;
    }
    let Ok((subject, skeleton, subject_global, playback)) = subjects.single() else {
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
    if sequence.view_index == 0 {
        let (left_support_weight, right_support_weight) =
            gait_support_weights(skeleton.gait_phase, frame.speed);
        sequence.samples.push(FrameSample {
            scenario: frame.scenario.to_owned(),
            scenario_frame: frame.scenario_frame,
            time_seconds: frame.time_seconds,
            speed_metres_per_second: frame.speed,
            gait_phase: skeleton.gait_phase,
            root_distance_metres: frame.distance,
            root_position_metres: subject_global.translation().to_array(),
            world_travel_direction: Vec3::new(
                -frame.local_direction.x,
                0.0,
                -frame.local_direction.y,
            )
            .to_array(),
            left_support_weight,
            right_support_weight,
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
            bones: collect_bones(subject, &bones),
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
        metrics.maximum_root_relative_step_metres <= 0.20
            && metrics.maximum_bone_rotation_step_degrees <= 60.0
    });
    let no_ground_penetration = scenarios
        .iter()
        .all(|metrics| metrics.minimum_foot_height_metres >= -0.08);
    let biomechanics_within_review_bounds = scenarios.iter().all(|metrics| {
        metrics.maximum_supported_foot_slip_metres_per_frame <= 0.035
            && metrics.maximum_planted_foot_drift_metres <= 0.035
            && metrics.minimum_knee_forward_bend_metres >= -0.08
            && metrics.pelvis_vertical_range_metres <= 0.20
            && metrics.head_vertical_range_metres <= 0.20
            && metrics
                .loop_seam_position_metres
                .is_none_or(|value| value <= 0.015)
            && metrics
                .loop_seam_rotation_degrees
                .is_none_or(|value| value <= 5.0)
    });
    let views_are_distinct = sequence.duplicate_view_frames.is_empty();
    let manifest = CaptureManifest {
        sample_hz: SAMPLE_HZ,
        pipeline: "authored FK plus final tactical mirroring, look, and terrain leg IK",
        views: VIEWS,
        validation: CaptureValidation {
            finite_transforms,
            all_scenarios_complete,
            all_artifacts_written,
            continuity_within_review_bounds,
            biomechanics_within_review_bounds,
            no_ground_penetration,
            views_are_distinct,
            duplicate_view_frames: sequence.duplicate_view_frames.clone(),
            note: "Continuity metrics are regression signals, not biomechanical proof; review index.html at normal and slow speed.",
        },
        scenarios,
        frames,
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

fn scenario_metrics(frames: &[FrameSample]) -> Vec<ScenarioMetrics> {
    let mut grouped = BTreeMap::<&str, Vec<&FrameSample>>::new();
    for frame in frames {
        grouped.entry(&frame.scenario).or_default().push(frame);
    }
    grouped
        .into_iter()
        .map(|(scenario, frames)| {
            let mut maximum_step = 0.0_f32;
            let mut maximum_rotation = 0.0_f32;
            let mut maximum_slip = 0.0_f32;
            let mut maximum_plant_drift = 0.0_f32;
            let mut worst_displacement = None;
            let mut worst_rotation = None;
            for pair in frames.windows(2) {
                let (before, after) = (pair[0], pair[1]);
                let root_delta = Vec3::from_array(after.root_position_metres)
                    - Vec3::from_array(before.root_position_metres);
                for (name, before_bone) in &before.bones {
                    let Some(after_bone) = after.bones.get(name) else {
                        continue;
                    };
                    let displacement = (Vec3::from_array(after_bone.position)
                        - Vec3::from_array(before_bone.position)
                        - root_delta)
                        .length();
                    if displacement > maximum_step {
                        maximum_step = displacement;
                        worst_displacement = Some(ContinuityLocation {
                            bone: name.clone(),
                            from_frame: before.scenario_frame,
                            to_frame: after.scenario_frame,
                            value: displacement,
                        });
                    }
                    let rotation = quat(after_bone)
                        .angle_between(quat(before_bone))
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
                    if support >= 0.9
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
            for (foot, left) in [("left_foot", true), ("right_foot", false)] {
                let mut anchor = None;
                for frame in &frames {
                    let support = if left {
                        frame.left_support_weight
                    } else {
                        frame.right_support_weight
                    };
                    if support >= 0.9 {
                        let Some(position) = frame
                            .bones
                            .get(foot)
                            .map(|bone| Vec3::from_array(bone.position).xz())
                        else {
                            continue;
                        };
                        let origin = anchor.get_or_insert(position);
                        maximum_plant_drift = maximum_plant_drift.max(position.distance(*origin));
                    } else {
                        anchor = None;
                    }
                }
            }
            let looped = scenario != "start-stop-transition";
            let (loop_position, loop_rotation) = if looped {
                let closures = frames
                    .iter()
                    .copied()
                    .filter(|frame| frame.gait_phase.abs() < 0.0001)
                    .collect::<Vec<_>>();
                closures
                    .get(closures.len().saturating_sub(2))
                    .copied()
                    .zip(closures.last().copied())
                    .map(|(first, last)| loop_seam(first, last))
                    .map_or((None, None), |(position, rotation)| {
                        (Some(position), Some(rotation))
                    })
            } else {
                (None, None)
            };
            ScenarioMetrics {
                scenario: scenario.to_owned(),
                frame_count: frames.len(),
                maximum_root_relative_step_metres: maximum_step,
                worst_displacement,
                maximum_bone_rotation_step_degrees: maximum_rotation,
                worst_rotation,
                loop_seam_position_metres: loop_position,
                loop_seam_rotation_degrees: loop_rotation,
                pelvis_vertical_range_metres: root_relative_vertical_range(&frames, "pelvis"),
                head_vertical_range_metres: root_relative_vertical_range(&frames, "head"),
                minimum_knee_forward_bend_metres: minimum_knee_bend(&frames),
                maximum_supported_foot_slip_metres_per_frame: maximum_slip,
                maximum_planted_foot_drift_metres: maximum_plant_drift,
                minimum_foot_height_metres: minimum_foot_height(&frames),
            }
        })
        .collect()
}

fn quat(bone: &BoneSample) -> Quat {
    Quat::from_array(bone.rotation_xyzw).normalize()
}

fn loop_seam(first: &FrameSample, last: &FrameSample) -> (f32, f32) {
    let root_delta =
        Vec3::from_array(last.root_position_metres) - Vec3::from_array(first.root_position_metres);
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
                    (Vec3::from_array(b.position) - Vec3::from_array(a.position) - root_delta)
                        .length(),
                ),
                metrics.1.max(quat(a).angle_between(quat(b)).to_degrees()),
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

fn minimum_foot_height(frames: &[&FrameSample]) -> f32 {
    let minimum = frames
        .iter()
        .flat_map(|frame| [frame.bones.get("left_foot"), frame.bones.get("right_foot")])
        .flatten()
        .map(|foot| foot.position[1])
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
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td></tr>",
                scenario.scenario,
                scenario.frame_count,
                describe(&scenario.worst_displacement, "m"),
                describe(&scenario.worst_rotation, "deg"),
                scenario.loop_seam_position_metres.map_or("&mdash;".into(), |value| format!("{value:.4}")),
                scenario.loop_seam_rotation_degrees.map_or("&mdash;".into(), |value| format!("{value:.2}")),
                scenario.maximum_supported_foot_slip_metres_per_frame,
                scenario.maximum_planted_foot_drift_metres,
                scenario.minimum_foot_height_metres,
            )
        })
        .collect::<String>();
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Animation review</title><style>
body{{font:15px system-ui;background:#111820;color:#e8eef5;margin:24px}}button,select{{margin:4px;padding:8px}}img{{max-width:min(960px,100%);background:#222}}table{{border-collapse:collapse;margin-top:20px}}td,th{{border:1px solid #526171;padding:6px}}.note{{max-width:960px;color:#b9c7d5}}#contact{{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:8px;margin-top:20px}}#contact img{{width:100%}}
</style></head><body><h1>Tactical locomotion review</h1>
<p class="note">This is the real base mesh and TacticalAnimationPlugin, sampled at 60 Hz with live stride phase, root travel, final mirroring/look/terrain IK, and a cyan skeleton overlay. Use normal speed first, then slow motion. Metrics flag continuity for investigation; they are not biomechanical proof.</p>
<div>{scenario_buttons}</div><label>View <select id="view"><option value="third-person">third person</option><option value="side">side</option><option value="front">front</option></select></label>
<label>Playback <select id="rate"><option value="1">normal</option><option value="2">half speed</option><option value="4">quarter speed</option></select></label>
<p id="telemetry"></p><img id="player"><div id="contact"></div>
<table><thead><tr><th>scenario</th><th>frames</th><th>worst root-relative displacement</th><th>worst rotation</th><th>loop seam m</th><th>loop seam deg</th><th>supported slip m/frame</th><th>planted interval drift m</th><th>minimum foot height m</th></tr></thead><tbody>{metrics}</tbody></table>
<script>const all={frame_json},scenarioNames={scenario_names_json};let scenario=scenarioNames[0]||"",i=0,timer;const player=document.querySelector('#player'),view=document.querySelector('#view'),rate=document.querySelector('#rate'),telemetry=document.querySelector('#telemetry');
function frames(){{return all.filter(x=>x.scenario===scenario)}}function show(){{const list=frames(),f=list.length?list[i%list.length]:null;if(!f){{player.removeAttribute('src');telemetry.textContent='No completed capture frames';return}}player.src=f.screenshots[view.value];telemetry.textContent=`${{f.scenario}} frame ${{f.scenario_frame}} | ${{f.speed_metres_per_second.toFixed(2)}} m/s | phase ${{f.gait_phase.toFixed(3)}} | support L ${{f.left_support_weight.toFixed(2)}} R ${{f.right_support_weight.toFixed(2)}}`;}}
function play(){{clearInterval(timer);timer=setInterval(()=>{{i=(i+1)%frames().length;show()}},1000/60*Number(rate.value))}}function contacts(){{const f=frames(),step=Math.max(1,Math.floor(f.length/12)),box=document.querySelector('#contact');box.innerHTML='';for(let n=0;n<f.length;n+=step){{let x=document.createElement('img');x.src=f[n].screenshots[view.value];x.title=`frame ${{f[n].scenario_frame}} phase ${{f[n].gait_phase.toFixed(3)}}`;box.appendChild(x)}}}}
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
            root_distance_metres: 0.0,
            root_position_metres: Vec3::ZERO.to_array(),
            world_travel_direction: Vec3::Z.to_array(),
            left_support_weight: 1.0,
            right_support_weight: 1.0,
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
    fn steady_scenarios_close_exactly_after_two_cycles() {
        for (name, speed) in [("walk", 2.0), ("blend", 3.75), ("run", 5.5)] {
            let frames = steady_scenario(name, speed, 2.0);
            assert_eq!(frames.first().unwrap().gait_phase, 0.0);
            assert!(frames.last().unwrap().gait_phase.abs() < 0.0001);
            assert!(frames.len() > 30);
            for pair in frames.windows(2) {
                assert!(pair[1].time_seconds > pair[0].time_seconds);
                assert!(pair[1].time_seconds - pair[0].time_seconds <= 1.0 / SAMPLE_HZ + 0.0001);
            }
        }
    }

    #[test]
    fn transition_uses_server_stride_formula_without_non_finite_state() {
        let frames = transition_scenario();
        assert_eq!(frames.len(), 241);
        assert!(frames.iter().all(|frame| frame.speed.is_finite()
            && frame.gait_phase.is_finite()
            && frame.distance.is_finite()));
        assert_eq!(frames.first().unwrap().speed, 0.0);
        assert_eq!(frames.last().unwrap().speed, 0.0);
    }
}
