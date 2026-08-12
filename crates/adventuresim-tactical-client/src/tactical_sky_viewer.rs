use std::{fs, path::PathBuf, process::Command};

use adventuresim_tactical_core::prelude::*;
use bevy::{
    app::AppExit,
    asset::AssetPlugin,
    camera::Exposure,
    light::AtmosphereEnvironmentMapLight,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{PresentMode, WindowResolution},
};
use serde::Serialize;

use crate::{SkyView, presentation::TacticalPresentationPlugin};

const VIEW_WIDTH: u32 = 1600;
const VIEW_HEIGHT: u32 = 900;
const LATITUDE_MICRODEGREES: i32 = 53_500_000;
const LONGITUDE_MICRODEGREES: i32 = 10_000_000;

#[derive(Resource)]
struct CaptureState {
    output: PathBuf,
    settle_frames: u32,
    settled: u32,
    prime_complete: bool,
    in_flight: bool,
    view: SkyView,
    absolute_minute: u64,
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
    camera_translation: [f32; 3],
    camera_direction: [f32; 3],
    vertical_fov_degrees: f32,
}

#[derive(Serialize)]
struct SkyValidation {
    expected_dimensions: bool,
    non_black_content: bool,
    subject_content: bool,
    passed: bool,
}

#[derive(Serialize)]
struct SkyManifest {
    pipeline: &'static str,
    view: &'static str,
    absolute_minute: u64,
    resolution: [u32; 2],
    settle_frames: u32,
    camera_version: u16,
    camera_translation: [f32; 3],
    camera_direction: [f32; 3],
    vertical_fov_degrees: f32,
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
    bright_pixel_bps: u16,
    non_black_pixel_bps: u16,
    bright_pixel_count: usize,
    non_black_pixel_count: usize,
    horizon_luma_delta: f32,
    revision: String,
    source_identity: String,
    validation: SkyValidation,
}

pub(super) fn run(view: SkyView, output: PathBuf, settle_frames: u32) {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create sky capture output directory");
    }

    let asset_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let exit = App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("Fabelgeist tactical sky: {view:?}"),
                        resolution: WindowResolution::new(VIEW_WIDTH, VIEW_HEIGHT)
                            .with_scale_factor_override(1.0),
                        present_mode: PresentMode::AutoNoVsync,
                        resizable: false,
                        decorations: false,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(TacticalPresentationPlugin {
            shadows_enabled: true,
            atmosphere_enabled: true,
            celestial_enabled: true,
            environment_light_enabled: true,
            environment_map_size: 64,
            bloom_enabled: true,
            ssao_enabled: false,
            max_vista_lods: 0,
        })
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(CaptureState {
            output,
            settle_frames,
            settled: 0,
            prime_complete: false,
            in_flight: false,
            view,
            absolute_minute: 0,
            sun_altitude_degrees: 0.0,
            moon_altitude_degrees: 0.0,
            lunar_illumination: 0.0,
            camera_translation: [0.0; 3],
            camera_direction: [0.0; 3],
            vertical_fov_degrees: 65.0,
        })
        .add_systems(PostStartup, move |world: &mut World| {
            setup_view(world, view)
        })
        .add_systems(Last, capture_view)
        .run();
    if exit != AppExit::Success {
        std::process::exit(1);
    }
}

fn setup_view(world: &mut World, view: SkyView) {
    let absolute_minute = match view {
        SkyView::Sun => 172 * MINUTES_PER_DAY + 12 * 60,
        SkyView::Twilight => 80 * MINUTES_PER_DAY + 18 * 60,
        // Canonical first-quarter Moon at 21:55 on day 36.
        SkyView::Moon => 53_155,
        // Canonical new moon at 23:00 on day 77.
        SkyView::Stars => 637_860,
    };
    let celestial = celestial_directions(
        absolute_minute,
        LATITUDE_MICRODEGREES,
        LONGITUDE_MICRODEGREES,
    );
    let sun = to_bevy_direction(celestial.sun);
    let moon = to_bevy_direction(celestial.moon);
    let view_direction = match view {
        SkyView::Sun => horizon_view(sun, 0.5),
        SkyView::Twilight => horizon_view(sun, 0.03),
        SkyView::Moon => moon,
        SkyView::Stars => Vec3::new(0.15, 0.55, -0.82).normalize(),
    };

    let mut camera = world
        .query_filtered::<(&mut Transform, &mut Exposure, &mut Projection), With<Camera3d>>()
        .single_mut(world)
        .expect("one tactical camera");
    camera.0.translation = Vec3::new(0.0, 2.0, 8.0);
    camera.0.look_to(view_direction, Vec3::Y);
    if matches!(view, SkyView::Moon)
        && let Projection::Perspective(perspective) = &mut *camera.2
    {
        perspective.fov = 12.0_f32.to_radians();
    }
    let fov = match &*camera.2 {
        Projection::Perspective(perspective) => perspective.fov.to_degrees(),
        _ => 65.0,
    };
    let camera_translation = camera.0.translation.to_array();
    let camera_direction = camera.0.forward().as_vec3().to_array();
    drop(camera);
    let mut state = world.resource_mut::<CaptureState>();
    state.absolute_minute = absolute_minute;
    state.sun_altitude_degrees = celestial.sun[1].asin().to_degrees();
    state.moon_altitude_degrees = celestial.moon[1].asin().to_degrees();
    state.lunar_illumination = celestial.lunar_illumination;
    state.camera_translation = camera_translation;
    state.camera_direction = camera_direction;
    state.vertical_fov_degrees = fov;
    drop(state);

    // A two-by-two authoritative terrain is the smallest complete scene
    // contract accepted by the production presentation observers. Keep its
    // gameplay extent tiny so foliage generation remains bounded.
    let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4])
        .expect("valid sky verification terrain");
    // This unreplicated plane exists only to give the capture a distant visual
    // horizon; it does not participate in tactical terrain or foliage logic.
    let ground_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Plane3d::default().mesh().size(40_000.0, 40_000.0));
    let ground_material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.11, 0.07),
            perceptual_roughness: 0.95,
            ..default()
        });
    world.spawn((
        Name::new("Sky verification horizon plane"),
        Mesh3d(ground_mesh),
        MeshMaterial3d(ground_material),
    ));
    world.spawn((
        Name::new("Sky verification environment"),
        SceneId(format!("sky-{view:?}")),
        terrain,
        SceneEnvironment {
            scene_digest: format!("sky-{view:?}"),
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            latitude_microdegrees: LATITUDE_MICRODEGREES,
            longitude_microdegrees: LONGITUDE_MICRODEGREES,
            absolute_minute,
            absolute_elevation_metres: 20,
            weather: WeatherSnapshot {
                rules_version: WEATHER_RULES_VERSION,
                interval_start_minute: absolute_minute,
                cell_latitude: 0,
                cell_longitude: 0,
                temperature_deci_c: 150,
                wind_speed_bps: 0,
                precipitation: Precipitation::Clear,
                intensity_bps: 0,
                ground_moisture_bps: 0,
                snow_cover_bps: 0,
            },
            canopy_bps: 0,
            wetland_bps: 0,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        },
    ));

    // The capture deliberately exercises the atmosphere-generated light too.
    assert!(
        world
            .query_filtered::<Entity, (With<Camera3d>, With<AtmosphereEnvironmentMapLight>)>()
            .single(world)
            .is_ok()
    );
}

fn capture_view(mut commands: Commands, mut state: ResMut<CaptureState>) {
    if state.in_flight {
        return;
    }
    if state.settled < state.settle_frames {
        state.settled += 1;
        return;
    }
    state.in_flight = true;
    if !state.prime_complete {
        commands.spawn(Screenshot::primary_window()).observe(
            |_: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                state.prime_complete = true;
                state.settled = 0;
                state.in_flight = false;
            },
        );
        return;
    }

    let path = state.output.clone();
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              state: Res<CaptureState>,
              mut exit: MessageWriter<AppExit>| {
            let metrics = sky_metrics(captured.image.data.as_deref());
            let expected_dimensions =
                captured.image.width() == VIEW_WIDTH && captured.image.height() == VIEW_HEIGHT;
            let subject_content = match state.view {
                SkyView::Sun | SkyView::Moon => metrics.bright_pixel_count >= 16,
                SkyView::Twilight => metrics.horizon_luma_delta >= 3.0,
                SkyView::Stars => metrics.bright_pixel_count >= 8,
            };
            let validation = SkyValidation {
                expected_dimensions,
                non_black_content: metrics.non_black_pixel_count >= 256,
                subject_content,
                passed: expected_dimensions
                    && metrics.non_black_pixel_count >= 256
                    && subject_content,
            };
            let manifest = SkyManifest {
                pipeline: "tactical_sky_native_capture_v2",
                view: sky_view_slug(state.view),
                absolute_minute: state.absolute_minute,
                resolution: [VIEW_WIDTH, VIEW_HEIGHT],
                settle_frames: state.settle_frames,
                camera_version: 1,
                camera_translation: state.camera_translation,
                camera_direction: state.camera_direction,
                vertical_fov_degrees: state.vertical_fov_degrees,
                sun_altitude_degrees: state.sun_altitude_degrees,
                moon_altitude_degrees: state.moon_altitude_degrees,
                lunar_illumination: state.lunar_illumination,
                bright_pixel_bps: metrics.bright_pixel_bps,
                non_black_pixel_bps: metrics.non_black_pixel_bps,
                bright_pixel_count: metrics.bright_pixel_count,
                non_black_pixel_count: metrics.non_black_pixel_count,
                horizon_luma_delta: metrics.horizon_luma_delta,
                revision: capture_revision(),
                source_identity: std::env::var("CAPTURE_SOURCE_IDENTITY")
                    .unwrap_or_else(|_| "standalone-unlabelled".into()),
                validation,
            };
            save_to_disk(&path)(captured);
            let manifest_path = path.with_extension("manifest.json");
            fs::write(
                &manifest_path,
                serde_json::to_vec_pretty(&manifest).unwrap(),
            )
            .expect("write sky manifest");
            if manifest.validation.passed {
                exit.write(AppExit::Success);
            } else {
                fs::write(
                    path.with_extension("failure.txt"),
                    "Sky semantic validation failed.\n",
                )
                .expect("write sky failure marker");
                exit.write(AppExit::error());
            }
        },
    );
}

#[derive(Default)]
struct SkyMetrics {
    bright_pixel_bps: u16,
    non_black_pixel_bps: u16,
    bright_pixel_count: usize,
    non_black_pixel_count: usize,
    horizon_luma_delta: f32,
}

fn sky_metrics(data: Option<&[u8]>) -> SkyMetrics {
    let Some(data) = data else {
        return SkyMetrics::default();
    };
    let pixels = data.as_chunks::<4>().0;
    if pixels.is_empty() {
        return SkyMetrics::default();
    }
    let mut bright = 0usize;
    let mut non_black = 0usize;
    let mut upper = 0u64;
    let mut lower = 0u64;
    let split = pixels.len() / 2;
    for (index, pixel) in pixels.iter().enumerate() {
        let luma =
            (u16::from(pixel[0]) * 54 + u16::from(pixel[1]) * 183 + u16::from(pixel[2]) * 19) / 256;
        bright += usize::from(luma >= 180);
        non_black += usize::from(luma >= 6);
        if index < split {
            upper += u64::from(luma)
        } else {
            lower += u64::from(luma)
        }
    }
    let bps = |count: usize| (count * 10_000 / pixels.len()).min(10_000) as u16;
    SkyMetrics {
        bright_pixel_bps: bps(bright),
        non_black_pixel_bps: bps(non_black),
        bright_pixel_count: bright,
        non_black_pixel_count: non_black,
        horizon_luma_delta: ((upper as f64 / split as f64)
            - (lower as f64 / (pixels.len() - split) as f64))
            .abs() as f32,
    }
}

fn sky_view_slug(view: SkyView) -> &'static str {
    match view {
        SkyView::Sun => "sun",
        SkyView::Twilight => "twilight",
        SkyView::Moon => "moon",
        SkyView::Stars => "stars",
    }
}

fn capture_revision() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| "unavailable".into())
}

fn to_bevy_direction(east_up_north: [f32; 3]) -> Vec3 {
    Vec3::new(east_up_north[0], east_up_north[1], -east_up_north[2]).normalize()
}

fn horizon_view(direction: Vec3, altitude: f32) -> Vec3 {
    let horizontal = Vec3::new(direction.x, 0.0, direction.z).normalize_or_zero();
    (horizontal + Vec3::Y * altitude).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_sky_cannot_satisfy_content_metrics() {
        let data = vec![0; VIEW_WIDTH as usize * VIEW_HEIGHT as usize * 4];
        let metrics = sky_metrics(Some(&data));
        assert_eq!(metrics.non_black_pixel_bps, 0);
        assert_eq!(metrics.bright_pixel_bps, 0);
        assert_eq!(metrics.horizon_luma_delta, 0.0);
    }

    #[test]
    fn horizon_and_bright_subject_are_measurable() {
        let mut data = vec![0; 16 * 4];
        for pixel in data.as_chunks_mut::<4>().0.iter_mut().take(8) {
            *pixel = [80, 60, 40, 255];
        }
        data[..4].copy_from_slice(&[255, 255, 255, 255]);
        let metrics = sky_metrics(Some(&data));
        assert!(metrics.non_black_pixel_bps > 0);
        assert!(metrics.bright_pixel_bps > 0);
        assert!(metrics.horizon_luma_delta > 3.0);
    }
}
