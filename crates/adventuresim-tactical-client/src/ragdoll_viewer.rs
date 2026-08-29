use std::{fs, path::PathBuf};

use adventuresim_tactical_core::{physics::AdventureSimulatorPhysicsPlugin, prelude::*};
use adventuresim_tactical_netcode::client::WeaponGuardInputState;
use bevy::{
    asset::io::AssetSourceBuilder,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::{
    animation::{
        TacticalAnimationPlugin,
        ragdoll::{
            HumanoidRagdollPlugin, MotorTelemetrySample, RAGDOLL_LAYER, RagdollMode,
            RagdollMotorTelemetry, RagdollPresentationFocus, RagdollPresentationReady,
            RagdollReset, TERRAIN_LAYER,
        },
    },
    camera::{CameraMode, TacticalCameraPlugin, TacticalCameraSet},
    player::{LocalCharacterId, PlayerPlugin},
    presentation::TacticalPresentationPlugin,
};

#[derive(Component)]
struct RagdollViewerSubject;

#[derive(Component)]
struct RagdollViewerLabel;

pub(crate) fn run(asset_root: PathBuf, output: Option<PathBuf>) {
    if let Some(output) = &output {
        fs::create_dir_all(output)
            .unwrap_or_else(|error| panic!("failed to create {output:?}: {error}"));
        for name in ["manifest.json", "failure.txt"] {
            let path = output.join(name);
            if let Err(error) = fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                panic!("failed to invalidate {path:?}: {error}");
            }
        }
    }
    let source = AssetSourceBuilder::platform_default(&asset_root.to_string_lossy(), None);
    App::new()
        .register_asset_source("workspace", source)
        .register_required_components_with::<Collider, _>(DebugRender::none)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Fabelgeist Ragdoll Viewer".into(),
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
                    enable_simulation: true,
                }),
            EnhancedInputPlugin,
        ))
        .add_plugins((
            PlayerPlugin,
            TacticalAnimationPlugin,
            HumanoidRagdollPlugin,
            TacticalCameraPlugin,
            TacticalPresentationPlugin::default(),
        ))
        .insert_resource(LocalCharacterId(0))
        .insert_resource(CameraMode { third_person: true })
        .insert_resource(WeaponGuardInputState::default())
        .insert_resource(ClearColor(Color::srgb(0.08, 0.1, 0.13)))
        .insert_resource(RagdollCapture::new(output))
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_controls, update_label))
        .add_systems(
            PostUpdate,
            position_viewer_camera
                .after(TacticalCameraSet::Offset)
                .before(TransformSystems::Propagate),
        )
        .add_systems(Last, capture_modes)
        .run();
}

fn viewer_camera_transform(focus: Vec3) -> Transform {
    let target = focus + Vec3::new(0.0, -0.15, 0.0);
    Transform::from_translation(focus + Vec3::new(2.0, 1.0, 3.2)).looking_at(target, Vec3::Y)
}

fn position_viewer_camera(
    subject: Single<(&Transform, Option<&RagdollPresentationFocus>), With<RagdollViewerSubject>>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<RagdollViewerSubject>)>,
) {
    let focus = subject
        .1
        .map_or(subject.0.translation, |focus| focus.position);
    let framed = viewer_camera_transform(focus);
    for mut camera in &mut cameras {
        *camera = framed;
    }
}

const CAPTURE_MODES: [(RagdollMode, &str, u32); 3] = [
    (RagdollMode::Animated, "animated", 30),
    (RagdollMode::Active, "active", 180),
    (RagdollMode::Passive, "passive", 90),
];
const SETTLE_SPEED_WINDOW_TICKS: u64 = 30;

fn fixed_steps_settled(start_tick: u64, current_tick: u64, required_steps: u32) -> bool {
    current_tick.saturating_sub(start_tick) >= u64::from(required_steps)
}

#[derive(Resource)]
struct RagdollCapture {
    output: Option<PathBuf>,
    stage: usize,
    stage_started_tick: Option<u64>,
    in_flight: bool,
    captures: Vec<CaptureSample>,
    active_initial_error: Option<f32>,
}

impl RagdollCapture {
    fn new(output: Option<PathBuf>) -> Self {
        Self {
            output,
            stage: 0,
            stage_started_tick: None,
            in_flight: false,
            captures: Vec::new(),
            active_initial_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct CaptureSample {
    mode: String,
    screenshot: String,
    telemetry: Option<MotorTelemetrySample>,
    pelvis_position: [f32; 3],
    terrain_height: Option<f32>,
    pelvis_height_above_terrain: Option<f32>,
    pelvis_speed: f32,
    maximum_recent_pelvis_speed: f32,
}

#[derive(Serialize)]
struct RagdollCaptureManifest {
    pipeline: &'static str,
    controls: &'static str,
    captures: Vec<CaptureSample>,
    validation: RagdollCaptureValidation,
}

#[derive(Serialize)]
struct RagdollCaptureValidation {
    all_modes_captured: bool,
    finite_telemetry: bool,
    active_hinges_driven: bool,
    active_convergence_observed: bool,
    passive_zero_strength: bool,
    terrain_contact_bounded: bool,
    pelvis_settled: bool,
    note: &'static str,
}

fn pelvis_has_bounded_support(sample: &CaptureSample) -> bool {
    sample
        .pelvis_height_above_terrain
        .is_some_and(|height| (-0.1..=0.75).contains(&height))
}

fn supported_and_settled(sample: &CaptureSample) -> bool {
    const MAX_SETTLED_PELVIS_SPEED: f32 = 0.5;

    pelvis_has_bounded_support(sample)
        && sample.maximum_recent_pelvis_speed.is_finite()
        && sample.maximum_recent_pelvis_speed <= MAX_SETTLED_PELVIS_SPEED
}

fn setup(mut commands: Commands) {
    // Keep the focused physics fixture level so capture measures ragdoll
    // settling rather than downhill travel on the gameplay hills profile.
    let terrain = TerrainGenerator::new(0xA11C_E5E1).generate(80, 0, 80);
    let height = terrain.height_at(Vec2::ZERO).unwrap_or_default() + 0.95;
    let terrain_collider = terrain.collider();
    commands.spawn((
        Name::new("Ragdoll viewer terrain"),
        SceneId("hills".to_owned()),
        SceneEnvironmentFixture::TemperateHills.snapshot("hills"),
        terrain,
        RigidBody::Static,
        terrain_collider,
        terrain_collision_layers(),
        Transform::default(),
    ));
    commands.spawn((
        Name::new("Ragdoll viewer subject"),
        RagdollViewerSubject,
        Player {
            name: "Ragdoll review".into(),
        },
        CharacterId(0),
        CharacterLook::default(),
        SkeletonState::default(),
        Transform::from_xyz(0.0, height, 0.0),
    ));
    commands.spawn((
        RagdollViewerLabel,
        Text::new("Loading Cascadeur humanoid..."),
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

fn terrain_collision_layers() -> CollisionLayers {
    CollisionLayers::new(TERRAIN_LAYER, RAGDOLL_LAYER)
}

fn keyboard_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<RagdollMode>,
    mut reset: ResMut<RagdollReset>,
) {
    if keyboard.just_pressed(KeyCode::KeyT) {
        *mode = mode.next();
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        reset.0 = true;
        *mode = RagdollMode::Animated;
    }
}

fn update_label(mode: Res<RagdollMode>, mut labels: Query<&mut Text, With<RagdollViewerLabel>>) {
    if !mode.is_changed() {
        return;
    }
    for mut label in &mut labels {
        label.0 = format!(
            "Mode: {}\nT: animated/active/passive | R: reset",
            mode.label()
        );
    }
}

fn capture_modes(
    mut commands: Commands,
    mut capture: ResMut<RagdollCapture>,
    mut mode: ResMut<RagdollMode>,
    ready: Query<&RagdollPresentationFocus, With<RagdollPresentationReady>>,
    terrains: Query<&SceneTerrain>,
    telemetry: Res<RagdollMotorTelemetry>,
) {
    let Some(output) = capture.output.clone() else {
        return;
    };
    if capture.in_flight || capture.stage >= CAPTURE_MODES.len() {
        return;
    }
    let Ok(focus) = ready.single() else {
        return;
    };
    let Ok(terrain) = terrains.single() else {
        return;
    };
    let (target_mode, slug, settle_fixed_steps) = CAPTURE_MODES[capture.stage];
    if *mode != target_mode {
        *mode = target_mode;
        capture.stage_started_tick = None;
        return;
    }
    let Some(current_tick) = telemetry.samples.last().map(|sample| sample.tick) else {
        return;
    };
    let stage_started_tick = *capture.stage_started_tick.get_or_insert(current_tick);
    if target_mode == RagdollMode::Active
        && capture.active_initial_error.is_none()
        && let Some(sample) = telemetry
            .samples
            .iter()
            .find(|sample| sample.tick >= stage_started_tick && sample.strength >= 0.5)
    {
        capture.active_initial_error = Some(sample.mean_error_radians);
    }
    if !fixed_steps_settled(stage_started_tick, current_tick, settle_fixed_steps) {
        return;
    }

    let screenshot = format!("{slug}.png");
    let path = output.join(&screenshot);
    let terrain_height = terrain.height_at(Vec2::new(focus.position.x, focus.position.z));
    let recent_start_tick = current_tick
        .saturating_sub(SETTLE_SPEED_WINDOW_TICKS)
        .max(stage_started_tick);
    let maximum_recent_pelvis_speed = telemetry
        .samples
        .iter()
        .filter(|sample| sample.tick >= recent_start_tick)
        .map(|sample| sample.pelvis_speed)
        .try_fold(0.0_f32, |maximum, speed| {
            speed.is_finite().then(|| maximum.max(speed))
        })
        .unwrap_or(f32::NAN);
    let sample = CaptureSample {
        mode: slug.to_owned(),
        screenshot,
        telemetry: telemetry.samples.last().cloned(),
        pelvis_position: focus.position.to_array(),
        terrain_height,
        pelvis_height_above_terrain: terrain_height.map(|height| focus.position.y - height),
        pelvis_speed: focus.linear_velocity.length(),
        maximum_recent_pelvis_speed,
    };
    capture.in_flight = true;
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              mut capture: ResMut<RagdollCapture>,
              mut exit: MessageWriter<AppExit>| {
            save_to_disk(&path)(captured);
            capture.captures.push(sample.clone());
            capture.stage += 1;
            capture.stage_started_tick = None;
            capture.in_flight = false;
            if capture.stage == CAPTURE_MODES.len() {
                finish_capture(&capture, &mut exit);
            }
        },
    );
}

fn finish_capture(capture: &RagdollCapture, exit: &mut MessageWriter<AppExit>) {
    let Some(output) = &capture.output else {
        return;
    };
    let active = capture
        .captures
        .iter()
        .find(|sample| sample.mode == "active");
    let passive = capture
        .captures
        .iter()
        .find(|sample| sample.mode == "passive");
    let active_final = active
        .and_then(|sample| sample.telemetry.as_ref())
        .map(|sample| sample.mean_error_radians);
    let validation = RagdollCaptureValidation {
        all_modes_captured: capture.captures.len() == CAPTURE_MODES.len(),
        finite_telemetry: capture
            .captures
            .iter()
            .filter_map(|sample| sample.telemetry.as_ref())
            .all(|sample| sample.finite),
        active_hinges_driven: active
            .and_then(|sample| sample.telemetry.as_ref())
            .is_some_and(|sample| sample.driven_hinges == 4 && sample.strength >= 0.95),
        active_convergence_observed: capture.active_initial_error.zip(active_final).is_some_and(
            |(initial, final_error)| {
                if initial <= 0.05 {
                    final_error <= 0.05
                } else {
                    final_error <= initial * 0.8
                }
            },
        ),
        passive_zero_strength: passive
            .and_then(|sample| sample.telemetry.as_ref())
            .is_some_and(|sample| sample.strength <= 0.001 && sample.driven_hinges == 0),
        terrain_contact_bounded: [active, passive]
            .into_iter()
            .flatten()
            .all(pelvis_has_bounded_support),
        pelvis_settled: [active, passive]
            .into_iter()
            .flatten()
            .all(supported_and_settled),
        note: "Terrain contact and settling are machine-gated; screenshots require visual review.",
    };
    let valid = validation.all_modes_captured
        && validation.finite_telemetry
        && validation.active_hinges_driven
        && validation.active_convergence_observed
        && validation.passive_zero_strength
        && validation.terrain_contact_bounded
        && validation.pelvis_settled;
    let manifest = RagdollCaptureManifest {
        pipeline: "cascadeur_humanoid_avian_ragdoll",
        controls: "T cycles animated -> active -> passive; R resets",
        captures: capture.captures.clone(),
        validation,
    };
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("ragdoll manifest serializes"),
    )
    .unwrap_or_else(|error| panic!("failed to write ragdoll manifest: {error}"));
    if !valid {
        fs::write(
            output.join("failure.txt"),
            "Ragdoll capture validation failed; inspect manifest.json and screenshots.\n",
        )
        .unwrap_or_else(|error| panic!("failed to write ragdoll failure marker: {error}"));
        exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
    } else {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaptureSample, fixed_steps_settled, pelvis_has_bounded_support, supported_and_settled,
        terrain_collision_layers, viewer_camera_transform,
    };
    use crate::animation::ragdoll::{RAGDOLL_LAYER, TERRAIN_LAYER};
    use avian3d::prelude::CollisionLayers;
    use bevy::prelude::*;

    #[test]
    fn fixture_terrain_and_ragdoll_layers_interact_bidirectionally() {
        let terrain = terrain_collision_layers();
        let ragdoll = CollisionLayers::new(RAGDOLL_LAYER, TERRAIN_LAYER);

        assert!(terrain.interacts_with(ragdoll));
        assert!(!terrain.interacts_with(terrain));
        assert!(!ragdoll.interacts_with(ragdoll));
    }

    #[test]
    fn capture_settle_uses_fixed_tick_delta_not_render_calls() {
        assert!(!fixed_steps_settled(100, 129, 30));
        assert!(fixed_steps_settled(100, 130, 30));
        assert!(fixed_steps_settled(100, 131, 30));
        assert!(!fixed_steps_settled(100, 99, 1));
    }

    #[test]
    fn capture_rejects_freefall_and_accepts_supported_rest() {
        let sample = |height, speed| CaptureSample {
            mode: "passive".to_owned(),
            screenshot: "passive.png".to_owned(),
            telemetry: None,
            pelvis_position: [0.0, height, 0.0],
            terrain_height: Some(0.0),
            pelvis_height_above_terrain: Some(height),
            pelvis_speed: speed,
            maximum_recent_pelvis_speed: speed,
        };

        assert!(supported_and_settled(&sample(0.4, 0.1)));
        assert!(!supported_and_settled(&sample(-30.0, 24.0)));
        assert!(!supported_and_settled(&sample(0.4, 1.0)));
        assert!(!pelvis_has_bounded_support(&sample(1.0, 0.1)));
        assert!(!pelvis_has_bounded_support(&sample(-0.2, 0.1)));
    }

    #[test]
    fn deterministic_camera_faces_subject_from_stable_three_quarter_offset() {
        let subject = Vec3::new(1.0, 2.0, -3.0);
        let camera = viewer_camera_transform(subject);
        let target = subject + Vec3::new(0.0, -0.15, 0.0);
        let expected = (target - camera.translation).normalize();
        assert!(camera.forward().as_vec3().dot(expected) > 0.9999);
        let distance = (camera.translation - subject).length();
        assert!(distance > 3.5 && distance < 4.0);
    }
}
