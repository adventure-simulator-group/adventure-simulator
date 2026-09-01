use std::{fs, path::PathBuf, process::Command};

use adventuresim_procedural_textures::{CRENELLATION_ALPHA_CUTOFF, CRENELLATION_MASK_TEXTURE_SIZE};
use bevy::{
    app::AppExit,
    camera::{ClearColorConfig, RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};
use serde::Serialize;

const CAPTURE_PIPELINE: &str = "building_shell_lod_traversal_v1";
const PRIME_READBACKS_PER_POSE: u8 = 2;
const MAX_CAPTURE_ATTEMPTS_PER_POSE: u8 = 3;
const CAMERA_SAMPLE_COUNT: usize = 32;
const CROWN_DIAGNOSTIC_LAYER: usize = 31;
const VIEW_WIDTH: u32 = 1440;
const VIEW_HEIGHT: u32 = 900;
const START_DISTANCE_FACTOR: f32 = 0.9;
const END_DISTANCE_FACTOR: f32 = 7.8;
const START_AZIMUTH_DEGREES: f32 = -18.0;
const END_AZIMUTH_DEGREES: f32 = 18.0;

#[derive(Component)]
pub(super) struct ShellCaptureCamera;

#[derive(Component)]
pub(super) struct ShellCaptureDiagnosticCamera;

#[derive(Resource)]
pub(super) struct ShellCaptureDiagnosticTarget(Handle<Image>);

#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraPose {
    position: Vec3,
    target: Vec3,
    distance_metres: f32,
    azimuth_degrees: f32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CapturePhase {
    #[default]
    Configure,
    Settle,
    Prime,
    Save,
    Diagnostic,
    ContinuousStart,
    Continuous,
    ContinuousDrain,
    Complete,
}

#[derive(Resource)]
pub(super) struct ShellCaptureConfig {
    output: PathBuf,
    settle_frames: u32,
    fixture: String,
    seed: u64,
    git_head: String,
    dirty_state: String,
    poses: Vec<CameraPose>,
    sample_index: usize,
    phase: CapturePhase,
    settled: u32,
    prime_readbacks: u8,
    capture_attempt: u8,
    readback_in_flight: bool,
    frame_index: u64,
    records: Vec<CaptureRecord>,
    temporal_readbacks: Vec<TemporalReadback>,
    continuous_index: usize,
    continuous_readbacks: Vec<ContinuousReadback>,
    continuous_crown_readbacks: Vec<ContinuousCrownReadback>,
    discarded_readbacks: Vec<DiscardedReadback>,
}

impl ShellCaptureConfig {
    pub(super) fn new(
        output: PathBuf,
        settle_frames: u32,
        fixture: String,
        seed: u64,
        dimensions: Vec2,
        maximum_height: f32,
    ) -> Result<Self, String> {
        if output.exists() {
            return Err(format!(
                "capture output {} already exists; use a fresh directory",
                output.display()
            ));
        }
        if settle_frames == 0 {
            return Err("capture settle frames must be non-zero".to_owned());
        }
        fs::create_dir_all(&output)
            .map_err(|error| format!("create {}: {error}", output.display()))?;
        let git_head = command_output(["rev-parse", "HEAD"]);
        let dirty_state = command_output(["status", "--short"]);
        Ok(Self {
            output,
            settle_frames,
            fixture,
            seed,
            git_head,
            dirty_state,
            poses: camera_path(dimensions, maximum_height),
            sample_index: 0,
            phase: CapturePhase::Configure,
            settled: 0,
            prime_readbacks: 0,
            capture_attempt: 1,
            readback_in_flight: false,
            frame_index: 0,
            records: Vec::new(),
            temporal_readbacks: Vec::new(),
            continuous_index: 0,
            continuous_readbacks: Vec::new(),
            continuous_crown_readbacks: Vec::new(),
            discarded_readbacks: Vec::new(),
        })
    }
}

fn command_output<const N: usize>(args: [&str; N]) -> String {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository_root = crate_dir
        .ancestors()
        .nth(2)
        .expect("building-generator crate lives below the repository root");
    let safe_directory = format!(
        "safe.directory={}",
        repository_root.display().to_string().replace('\\', "/")
    );
    Command::new("git")
        .current_dir(repository_root)
        .args(["-c", &safe_directory])
        .args(args)
        .output()
        .map(|output| {
            if output.status.success() {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            } else {
                format!(
                    "UNAVAILABLE: git exited {:?}: {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                )
            }
        })
        .unwrap_or_else(|error| format!("UNAVAILABLE: {error}"))
}

fn camera_path(dimensions: Vec2, maximum_height: f32) -> Vec<CameraPose> {
    let target = Vec3::new(0.0, maximum_height * 0.72, 0.0);
    let base_distance = dimensions.length().max(maximum_height) * 1.05;
    (0..CAMERA_SAMPLE_COUNT)
        .map(|index| {
            let t = index as f32 / (CAMERA_SAMPLE_COUNT - 1) as f32;
            let factor = START_DISTANCE_FACTOR.lerp(END_DISTANCE_FACTOR, t);
            let azimuth_degrees = START_AZIMUTH_DEGREES.lerp(END_AZIMUTH_DEGREES, t);
            let distance = base_distance * factor;
            let azimuth = azimuth_degrees.to_radians();
            let elevation = 0.18_f32;
            let horizontal = distance * elevation.cos();
            let position = target
                + Vec3::new(
                    horizontal * azimuth.cos(),
                    distance * elevation.sin(),
                    horizontal * azimuth.sin(),
                );
            CameraPose {
                position,
                target,
                distance_metres: position.distance(target),
                azimuth_degrees,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
struct CaptureRecord {
    sample_index: usize,
    screenshot: String,
    frame_index: u64,
    camera_position: [f32; 3],
    camera_target: [f32; 3],
    distance_metres: f32,
    azimuth_degrees: f32,
    settled_frames: u32,
    prime_readbacks: u8,
    capture_attempt: u8,
    pixel_hash: String,
    non_clear_pixel_bps: u32,
    crown_diagnostic_screenshot: String,
    crown_pixel_count: u32,
    crown_pixel_bps: u32,
}

#[derive(Clone, Debug, Serialize)]
struct TemporalReadback {
    sample_index: usize,
    repeat_index: u8,
    screenshot: String,
    frame_index: u64,
    pixel_hash: String,
    non_clear_pixel_bps: u32,
}

#[derive(Clone, Debug, Serialize)]
struct ContinuousReadback {
    sample_index: usize,
    screenshot: String,
    requested_frame_index: u64,
    completed_frame_index: u64,
    camera_position: [f32; 3],
    camera_target: [f32; 3],
    distance_metres: f32,
    azimuth_degrees: f32,
    pixel_hash: String,
    non_clear_pixel_bps: u32,
}

#[derive(Clone, Debug, Serialize)]
struct ContinuousCrownReadback {
    sample_index: usize,
    screenshot: String,
    requested_frame_index: u64,
    completed_frame_index: u64,
    distance_metres: f32,
    crown_pixel_count: u32,
    crown_pixel_bps: u32,
}

#[derive(Clone, Debug, Serialize)]
struct DiscardedReadback {
    sample_index: usize,
    screenshot: String,
    frame_index: u64,
    capture_attempt: u8,
    reason: &'static str,
    pixel_hash: String,
    non_clear_pixel_bps: u32,
}

#[derive(Serialize)]
struct CaptureManifest<'a> {
    schema: u32,
    pipeline: &'static str,
    status: &'static str,
    fixture: &'a str,
    seed: u64,
    lod: &'static str,
    revision: &'a str,
    dirty_state: &'a str,
    width: u32,
    height: u32,
    alpha_mode: &'static str,
    alpha_cutoff: f32,
    mask_texture_size: u32,
    mask_mip_levels: u32,
    camera_path_semantics: &'static str,
    temporal_readback_semantics: &'static str,
    crown_diagnostic_semantics: &'static str,
    validation_passed: bool,
    validation_failures: Vec<String>,
    discarded_readbacks: &'a [DiscardedReadback],
    temporal_readbacks: &'a [TemporalReadback],
    continuous_readbacks: &'a [ContinuousReadback],
    continuous_crown_readbacks: &'a [ContinuousCrownReadback],
    captures: &'a [CaptureRecord],
}

pub(super) fn spawn_crown_diagnostic(
    world: &mut World,
    mesh: Handle<Mesh>,
    mask: Handle<Image>,
    transform: Transform,
) {
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color_texture: Some(mask),
            alpha_mode: AlphaMode::Mask(CRENELLATION_ALPHA_CUTOFF),
            unlit: true,
            cull_mode: None,
            ..default()
        });
    world.spawn((
        Name::new("Shell CrownMask occupancy diagnostic"),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        transform,
        RenderLayers::layer(CROWN_DIAGNOSTIC_LAYER),
    ));
    let target = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            VIEW_WIDTH,
            VIEW_HEIGHT,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    world.insert_resource(ShellCaptureDiagnosticTarget(target.clone()));
    world.spawn((
        Name::new("Shell CrownMask occupancy camera"),
        ShellCaptureDiagnosticCamera,
        Camera3d::default(),
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        RenderTarget::Image(target.into()),
        RenderLayers::layer(CROWN_DIAGNOSTIC_LAYER),
        Transform::default(),
    ));
}

#[allow(clippy::type_complexity)]
pub(super) fn drive_shell_capture(
    mut commands: Commands,
    state: Option<ResMut<ShellCaptureConfig>>,
    camera: Option<
        Single<
            &mut Transform,
            (
                With<ShellCaptureCamera>,
                Without<ShellCaptureDiagnosticCamera>,
            ),
        >,
    >,
    diagnostic_camera: Option<
        Single<
            &mut Transform,
            (
                With<ShellCaptureDiagnosticCamera>,
                Without<ShellCaptureCamera>,
            ),
        >,
    >,
    diagnostic_target: Option<Res<ShellCaptureDiagnosticTarget>>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(mut state), Some(mut camera), Some(mut diagnostic_camera), Some(diagnostic_target)) =
        (state, camera, diagnostic_camera, diagnostic_target)
    else {
        return;
    };
    state.frame_index += 1;
    match state.phase {
        CapturePhase::Configure => {
            let pose = state.poses[state.sample_index];
            **camera = Transform::from_translation(pose.position).looking_at(pose.target, Vec3::Y);
            **diagnostic_camera =
                Transform::from_translation(pose.position).looking_at(pose.target, Vec3::Y);
            state.settled = 0;
            state.prime_readbacks = 0;
            state.capture_attempt = 1;
            state.phase = CapturePhase::Settle;
        }
        CapturePhase::Settle => {
            state.settled += 1;
            let required = if state.sample_index == 0 {
                state.settle_frames
            } else {
                1
            };
            if state.settled >= required {
                state.phase = CapturePhase::Prime;
            }
        }
        CapturePhase::Prime => {
            if state.readback_in_flight {
                return;
            }
            if state.prime_readbacks >= PRIME_READBACKS_PER_POSE {
                state.phase = CapturePhase::Save;
                return;
            }
            state.readback_in_flight = true;
            let sample_index = state.sample_index;
            let repeat_index = state.prime_readbacks + 1;
            let screenshot =
                format!("shell-traversal-{sample_index:02}-temporal-{repeat_index:02}.png");
            let path = state.output.join(&screenshot);
            commands.spawn(Screenshot::primary_window()).observe(
                move |captured: On<ScreenshotCaptured>, mut state: ResMut<ShellCaptureConfig>| {
                    let bytes = captured.image.data.as_deref().unwrap_or(&[]);
                    let pixel_hash = format!("{:016x}", stable_hash(bytes));
                    let non_clear_pixel_bps = non_clear_pixel_bps(bytes);
                    if non_clear_pixel_bps < 25
                        && state.capture_attempt < MAX_CAPTURE_ATTEMPTS_PER_POSE
                    {
                        let discarded_screenshot = format!(
                            "shell-traversal-{sample_index:02}-discarded-temporal-attempt-{:02}.png",
                            state.capture_attempt
                        );
                        save_to_disk(state.output.join(&discarded_screenshot))(captured);
                        let frame_index = state.frame_index;
                        let capture_attempt = state.capture_attempt;
                        state.discarded_readbacks.push(DiscardedReadback {
                            sample_index,
                            screenshot: discarded_screenshot,
                            frame_index,
                            capture_attempt,
                            reason: "startup_or_clear_temporal_readback_below_25_bps",
                            pixel_hash,
                            non_clear_pixel_bps,
                        });
                        state.capture_attempt += 1;
                        state.settled = 0;
                        state.readback_in_flight = false;
                        state.phase = CapturePhase::Settle;
                        return;
                    }
                    save_to_disk(&path)(captured);
                    let frame_index = state.frame_index;
                    state.temporal_readbacks.push(TemporalReadback {
                        sample_index,
                        repeat_index,
                        screenshot: screenshot.clone(),
                        frame_index,
                        pixel_hash,
                        non_clear_pixel_bps,
                    });
                    state.prime_readbacks += 1;
                    state.readback_in_flight = false;
                },
            );
        }
        CapturePhase::Save => request_final_readback(&mut commands, &mut state),
        CapturePhase::Diagnostic => {
            request_diagnostic_readback(&mut commands, &mut state, &diagnostic_target.0)
        }
        CapturePhase::ContinuousStart => {
            state.continuous_index = 0;
            state.phase = CapturePhase::Continuous;
        }
        CapturePhase::Continuous => request_continuous_readbacks(
            &mut commands,
            &mut state,
            &mut camera,
            &mut diagnostic_camera,
            &diagnostic_target.0,
        ),
        CapturePhase::ContinuousDrain => {
            if state.continuous_readbacks.len() == state.poses.len()
                && state.continuous_crown_readbacks.len() == state.poses.len()
            {
                state.phase = CapturePhase::Complete;
                finish_capture(&state, &mut exit);
            }
        }
        CapturePhase::Complete => {}
    }
}

fn request_final_readback(commands: &mut Commands, state: &mut ShellCaptureConfig) {
    if state.readback_in_flight {
        return;
    }
    state.readback_in_flight = true;
    let sample_index = state.sample_index;
    let pose = state.poses[sample_index];
    let screenshot = format!("shell-traversal-{sample_index:02}.png");
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut state: ResMut<ShellCaptureConfig>| {
            let bytes = captured.image.data.as_deref().unwrap_or(&[]);
            let non_clear_pixel_bps = non_clear_pixel_bps(bytes);
            let pixel_hash = format!("{:016x}", stable_hash(bytes));
            if non_clear_pixel_bps < 25 && state.capture_attempt < MAX_CAPTURE_ATTEMPTS_PER_POSE {
                let discarded_screenshot = format!(
                    "shell-traversal-{sample_index:02}-discarded-attempt-{:02}.png",
                    state.capture_attempt
                );
                save_to_disk(state.output.join(&discarded_screenshot))(captured);
                let frame_index = state.frame_index;
                let capture_attempt = state.capture_attempt;
                state.discarded_readbacks.push(DiscardedReadback {
                    sample_index,
                    screenshot: discarded_screenshot,
                    frame_index,
                    capture_attempt,
                    reason: "startup_or_clear_readback_below_25_bps",
                    pixel_hash,
                    non_clear_pixel_bps,
                });
                state.capture_attempt += 1;
                state.settled = 0;
                state.prime_readbacks = 0;
                state.readback_in_flight = false;
                state.phase = CapturePhase::Settle;
                return;
            }
            let record = CaptureRecord {
                sample_index,
                screenshot: screenshot.clone(),
                frame_index: state.frame_index,
                camera_position: pose.position.to_array(),
                camera_target: pose.target.to_array(),
                distance_metres: pose.distance_metres,
                azimuth_degrees: pose.azimuth_degrees,
                settled_frames: state.settle_frames,
                prime_readbacks: state.prime_readbacks,
                capture_attempt: state.capture_attempt,
                pixel_hash,
                non_clear_pixel_bps,
                crown_diagnostic_screenshot: String::new(),
                crown_pixel_count: 0,
                crown_pixel_bps: 0,
            };
            save_to_disk(state.output.join(&screenshot))(captured);
            state.records.push(record);
            state.readback_in_flight = false;
            state.phase = CapturePhase::Diagnostic;
        },
    );
}

fn request_diagnostic_readback(
    commands: &mut Commands,
    state: &mut ShellCaptureConfig,
    target: &Handle<Image>,
) {
    if state.readback_in_flight {
        return;
    }
    state.readback_in_flight = true;
    let sample_index = state.sample_index;
    let screenshot = format!("shell-traversal-{sample_index:02}-crown-diagnostic.png");
    let path = state.output.join(&screenshot);
    commands.spawn(Screenshot::image(target.clone())).observe(
        move |captured: On<ScreenshotCaptured>, mut state: ResMut<ShellCaptureConfig>| {
            let bytes = captured.image.data.as_deref().unwrap_or(&[]);
            let crown_pixel_count = diagnostic_pixel_count(bytes);
            let total_pixels = (bytes.len() / 4).max(1) as u64;
            let crown_pixel_bps = (u64::from(crown_pixel_count) * 10_000 / total_pixels) as u32;
            save_to_disk(&path)(captured);
            let record = state
                .records
                .last_mut()
                .expect("production capture precedes CrownMask diagnostic");
            record.crown_diagnostic_screenshot = screenshot.clone();
            record.crown_pixel_count = crown_pixel_count;
            record.crown_pixel_bps = crown_pixel_bps;
            state.readback_in_flight = false;
            state.sample_index += 1;
            if state.sample_index < state.poses.len() {
                state.phase = CapturePhase::Configure;
                return;
            }
            state.phase = CapturePhase::ContinuousStart;
        },
    );
}

fn request_continuous_readbacks(
    commands: &mut Commands,
    state: &mut ShellCaptureConfig,
    camera: &mut Transform,
    diagnostic_camera: &mut Transform,
    diagnostic_target: &Handle<Image>,
) {
    if state.continuous_index >= state.poses.len() {
        state.phase = CapturePhase::ContinuousDrain;
        return;
    }
    let sample_index = state.continuous_index;
    let pose = state.poses[sample_index];
    *camera = Transform::from_translation(pose.position).looking_at(pose.target, Vec3::Y);
    *diagnostic_camera =
        Transform::from_translation(pose.position).looking_at(pose.target, Vec3::Y);
    let requested_frame_index = state.frame_index;

    let screenshot = format!("shell-motion-{sample_index:02}.png");
    let path = state.output.join(&screenshot);
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut state: ResMut<ShellCaptureConfig>| {
            let bytes = captured.image.data.as_deref().unwrap_or(&[]);
            let record = ContinuousReadback {
                sample_index,
                screenshot: screenshot.clone(),
                requested_frame_index,
                completed_frame_index: state.frame_index,
                camera_position: pose.position.to_array(),
                camera_target: pose.target.to_array(),
                distance_metres: pose.distance_metres,
                azimuth_degrees: pose.azimuth_degrees,
                pixel_hash: format!("{:016x}", stable_hash(bytes)),
                non_clear_pixel_bps: non_clear_pixel_bps(bytes),
            };
            save_to_disk(&path)(captured);
            state.continuous_readbacks.push(record);
        },
    );

    let crown_screenshot = format!("shell-motion-{sample_index:02}-crown-diagnostic.png");
    let crown_path = state.output.join(&crown_screenshot);
    commands
        .spawn(Screenshot::image(diagnostic_target.clone()))
        .observe(
            move |captured: On<ScreenshotCaptured>, mut state: ResMut<ShellCaptureConfig>| {
                let bytes = captured.image.data.as_deref().unwrap_or(&[]);
                let crown_pixel_count = diagnostic_pixel_count(bytes);
                let total_pixels = (bytes.len() / 4).max(1) as u64;
                let record = ContinuousCrownReadback {
                    sample_index,
                    screenshot: crown_screenshot.clone(),
                    requested_frame_index,
                    completed_frame_index: state.frame_index,
                    distance_metres: pose.distance_metres,
                    crown_pixel_count,
                    crown_pixel_bps: (u64::from(crown_pixel_count) * 10_000 / total_pixels) as u32,
                };
                save_to_disk(&crown_path)(captured);
                state.continuous_crown_readbacks.push(record);
            },
        );
    state.continuous_index += 1;
}

fn finish_capture(state: &ShellCaptureConfig, exit: &mut MessageWriter<AppExit>) {
    let failures = validation_failures(state);
    let passed = failures.is_empty();
    write_manifest(state, passed, failures.clone());
    if passed {
        exit.write(AppExit::Success);
    } else {
        fs::write(state.output.join("failure.txt"), failures.join("\n"))
            .expect("write Shell capture failure");
        exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn non_clear_pixel_bps(bytes: &[u8]) -> u32 {
    if bytes.len() < 4 {
        return 0;
    }
    let clear = [184_i16, 204, 219];
    let non_clear = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| {
            (i16::from(pixel[0]) - clear[0]).abs() > 12
                || (i16::from(pixel[1]) - clear[1]).abs() > 12
                || (i16::from(pixel[2]) - clear[2]).abs() > 12
        })
        .count();
    (non_clear as u64 * 10_000 / (bytes.len() / 4) as u64) as u32
}

fn diagnostic_pixel_count(bytes: &[u8]) -> u32 {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] > 16 || pixel[1] > 16 || pixel[2] > 16)
        .count() as u32
}

fn validation_failures(state: &ShellCaptureConfig) -> Vec<String> {
    let mut failures = Vec::new();
    if state.records.len() != state.poses.len() {
        failures.push(format!(
            "capture count {} != pose count {}",
            state.records.len(),
            state.poses.len()
        ));
    }
    if state.git_head.len() != 40 || !state.git_head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        failures.push(format!(
            "revision provenance is invalid: {:?}",
            state.git_head
        ));
    }
    if state
        .records
        .windows(2)
        .any(|pair| pair[1].distance_metres <= pair[0].distance_metres)
    {
        failures.push("camera distances are not strictly increasing".to_owned());
    }
    let expected_temporal_readbacks = state.poses.len() * usize::from(PRIME_READBACKS_PER_POSE);
    if state.temporal_readbacks.len() != expected_temporal_readbacks {
        failures.push(format!(
            "temporal readback count {} != expected {}",
            state.temporal_readbacks.len(),
            expected_temporal_readbacks
        ));
    }
    for record in &state.records {
        if record.pixel_hash == "cbf29ce484222325" {
            failures.push(format!(
                "capture {} has an empty pixel hash",
                record.sample_index
            ));
        }
        if record.non_clear_pixel_bps < 25 {
            failures.push(format!(
                "capture {} has only {} bps non-clear pixels",
                record.sample_index, record.non_clear_pixel_bps
            ));
        }
        if record.prime_readbacks != PRIME_READBACKS_PER_POSE {
            failures.push(format!(
                "capture {} had {} prime readbacks",
                record.sample_index, record.prime_readbacks
            ));
        }
        if record.crown_pixel_count == 0 {
            failures.push(format!(
                "capture {} has no CrownMask diagnostic pixels",
                record.sample_index
            ));
        }
        let repeats = state
            .temporal_readbacks
            .iter()
            .filter(|readback| readback.sample_index == record.sample_index)
            .collect::<Vec<_>>();
        if repeats.len() == usize::from(PRIME_READBACKS_PER_POSE) {
            if repeats
                .iter()
                .any(|readback| readback.pixel_hash != record.pixel_hash)
            {
                failures.push(format!(
                    "capture {} changed across repeated same-pose GPU readbacks",
                    record.sample_index
                ));
            }
            if repeats
                .iter()
                .any(|readback| readback.non_clear_pixel_bps < 25)
            {
                failures.push(format!(
                    "capture {} has a blank repeated GPU readback",
                    record.sample_index
                ));
            }
        }
    }
    for pair in state.records.windows(2) {
        let first =
            f64::from(pair[0].crown_pixel_count) * f64::from(pair[0].distance_metres).powi(2);
        let second =
            f64::from(pair[1].crown_pixel_count) * f64::from(pair[1].distance_metres).powi(2);
        if first > 0.0 {
            let ratio = second / first;
            if !(0.55..=1.80).contains(&ratio) {
                failures.push(format!(
                    "CrownMask normalized occupancy jumped by ratio {ratio:.3} between samples {} and {}",
                    pair[0].sample_index, pair[1].sample_index
                ));
            }
        }
    }
    if state.continuous_readbacks.len() != state.poses.len() {
        failures.push(format!(
            "continuous production readback count {} != pose count {}",
            state.continuous_readbacks.len(),
            state.poses.len()
        ));
    }
    if state.continuous_crown_readbacks.len() != state.poses.len() {
        failures.push(format!(
            "continuous CrownMask readback count {} != pose count {}",
            state.continuous_crown_readbacks.len(),
            state.poses.len()
        ));
    }
    let mut continuous = state.continuous_readbacks.iter().collect::<Vec<_>>();
    continuous.sort_by_key(|record| record.sample_index);
    if continuous
        .windows(2)
        .any(|pair| pair[1].requested_frame_index != pair[0].requested_frame_index + 1)
    {
        failures.push(
            "continuous production readbacks were not requested on consecutive frames".to_owned(),
        );
    }
    for record in &continuous {
        if record.non_clear_pixel_bps < 25 {
            failures.push(format!(
                "continuous production sample {} is blank",
                record.sample_index
            ));
        }
    }
    let mut continuous_crowns = state.continuous_crown_readbacks.iter().collect::<Vec<_>>();
    continuous_crowns.sort_by_key(|record| record.sample_index);
    if continuous_crowns
        .windows(2)
        .any(|pair| pair[1].requested_frame_index != pair[0].requested_frame_index + 1)
    {
        failures.push(
            "continuous CrownMask readbacks were not requested on consecutive frames".to_owned(),
        );
    }
    for record in &continuous_crowns {
        if record.crown_pixel_count == 0 {
            failures.push(format!(
                "continuous CrownMask sample {} disappeared",
                record.sample_index
            ));
        }
    }
    for pair in continuous_crowns.windows(2) {
        let first =
            f64::from(pair[0].crown_pixel_count) * f64::from(pair[0].distance_metres).powi(2);
        let second =
            f64::from(pair[1].crown_pixel_count) * f64::from(pair[1].distance_metres).powi(2);
        if first > 0.0 {
            let ratio = second / first;
            if !(0.55..=1.80).contains(&ratio) {
                failures.push(format!(
                    "continuous CrownMask normalized occupancy jumped by ratio {ratio:.3} between samples {} and {}",
                    pair[0].sample_index, pair[1].sample_index
                ));
            }
        }
    }
    failures
}

fn write_manifest(state: &ShellCaptureConfig, passed: bool, failures: Vec<String>) {
    let manifest = CaptureManifest {
        schema: 1,
        pipeline: CAPTURE_PIPELINE,
        status: if passed { "PASS" } else { "FAIL" },
        fixture: &state.fixture,
        seed: state.seed,
        lod: "shell",
        revision: &state.git_head,
        dirty_state: &state.dirty_state,
        width: VIEW_WIDTH,
        height: VIEW_HEIGHT,
        alpha_mode: "mask",
        alpha_cutoff: CRENELLATION_ALPHA_CUTOFF,
        mask_texture_size: CRENELLATION_MASK_TEXTURE_SIZE,
        mask_mip_levels: CRENELLATION_MASK_TEXTURE_SIZE.ilog2() + 1,
        camera_path_semantics: "32 strictly increasing fine-step distances with a bounded azimuth sweep; only the first pose uses the configured startup settle, then each pose advances after one rendered frame",
        temporal_readback_semantics: "two preserved same-pose production GPU readbacks plus one final production frame per pose require identical raw hashes; a second traversal requests all 32 production and CrownMask frames on consecutive Update frames without awaiting completion",
        crown_diagnostic_semantics: "offscreen render of the exact CrownMask mesh and shared alpha-tested texture on an isolated render layer; both settled and consecutive-motion validation require presence and bounded distance-normalized occupancy continuity",
        validation_passed: passed,
        validation_failures: failures,
        discarded_readbacks: &state.discarded_readbacks,
        temporal_readbacks: &state.temporal_readbacks,
        continuous_readbacks: &state.continuous_readbacks,
        continuous_crown_readbacks: &state.continuous_crown_readbacks,
        captures: &state.records,
    };
    fs::write(
        state.output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize Shell capture manifest"),
    )
    .expect("write Shell capture manifest");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_path_moves_strictly_away_while_sweeping_the_crown() {
        let poses = camera_path(Vec2::new(28.0, 24.0), 18.0);
        assert_eq!(poses.len(), CAMERA_SAMPLE_COUNT);
        assert!(
            poses
                .windows(2)
                .all(|pair| pair[1].distance_metres > pair[0].distance_metres)
        );
        assert!(poses.first().unwrap().azimuth_degrees < 0.0);
        assert!(poses.last().unwrap().azimuth_degrees > 0.0);
        assert!(poses.iter().all(|pose| pose.position.is_finite()));
    }

    #[test]
    fn manifest_contract_uses_the_shared_mask_and_gpu_readback_budget() {
        assert_eq!(CAPTURE_PIPELINE, "building_shell_lod_traversal_v1");
        assert_eq!(CRENELLATION_MASK_TEXTURE_SIZE, 256);
        assert_eq!(CRENELLATION_MASK_TEXTURE_SIZE.ilog2() + 1, 9);
        assert_eq!(CRENELLATION_ALPHA_CUTOFF, 0.5);
        assert_eq!(PRIME_READBACKS_PER_POSE, 2);
        assert_eq!(MAX_CAPTURE_ATTEMPTS_PER_POSE, 3);
        const { assert!(CAMERA_SAMPLE_COUNT >= 24) };
    }
}
