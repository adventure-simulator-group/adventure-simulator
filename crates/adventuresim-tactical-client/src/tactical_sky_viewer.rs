use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};

use adventuresim_tactical_core::prelude::*;
use adventuresim_world_schema::coordinates::{LatitudeMicrodegrees, LongitudeMicrodegrees};
use bevy::{
    app::AppExit,
    asset::AssetPlugin,
    camera::Exposure,
    light::SunDisk,
    pbr::AtmosphereSettings,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        view::{
            ExtractedView,
            screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
        },
    },
    window::{PresentMode, WindowResolution},
};
use serde::Serialize;

use crate::{
    SkyView,
    presentation::{
        TacticalCameraSetup, TacticalCloudCaptureOverride, TacticalCloudCaptureProfile,
        TacticalGameplayCamera, TacticalPresentationPlugin,
    },
};

const VIEW_WIDTH: u32 = 1600;
const VIEW_HEIGHT: u32 = 900;
const LATITUDE: LatitudeMicrodegrees = LatitudeMicrodegrees::new(53_500_000).unwrap();
const LONGITUDE: LongitudeMicrodegrees = LongitudeMicrodegrees::new(10_000_000).unwrap();

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
    observed_camera: Option<ObservedSkyCamera>,
    camera_observation_ready: bool,
}

#[derive(Clone, Copy)]
struct SkyViewConfiguration {
    absolute_minute: u64,
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
    camera_translation: Vec3,
    camera_direction: Vec3,
    vertical_fov_degrees: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ObservedSkyCamera {
    translation: Vec3,
    direction: Vec3,
    vertical_fov_degrees: f32,
    render_observations: u64,
}

#[derive(Resource, Clone, ExtractResource)]
struct SkyCameraProbe(Arc<Mutex<Option<ObservedSkyCamera>>>);

#[derive(Serialize)]
struct SkyValidation {
    expected_dimensions: bool,
    non_black_content: bool,
    subject_content: bool,
    camera_observation_ready: bool,
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
    requested_camera_translation: [f32; 3],
    requested_camera_direction: [f32; 3],
    requested_vertical_fov_degrees: f32,
    camera_translation: [f32; 3],
    camera_direction: [f32; 3],
    vertical_fov_degrees: f32,
    render_camera_observations: u64,
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
    bright_pixel_bps: u16,
    non_black_pixel_bps: u16,
    bright_pixel_count: usize,
    non_black_pixel_count: usize,
    horizon_luma_delta: f32,
    upper_sky_mean_luma: f32,
    upper_sky_luma_variance: f32,
    horizon_sky_mean_luma: f32,
    horizon_sky_red_blue_delta: f32,
    sun_disk_angular_diameter_degrees: f32,
    diagnostic_magnification: bool,
    cloud_profile: Option<&'static str>,
    exposure_ev100: f32,
    solar_source_illuminance_lux: f32,
    atmosphere_enabled: bool,
    environment_map_size: u32,
    revision: String,
    source_identity: String,
    validation: SkyValidation,
}

pub(super) fn run(view: SkyView, output: PathBuf, settle_frames: u32) {
    let settle_frames = if is_cloud_view(view) {
        settle_frames.max(60)
    } else {
        settle_frames
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create sky capture output directory");
    }
    let _ = fs::remove_file(output.with_extension("failure.txt"));

    let asset_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let view_configuration = sky_view_configuration(view);
    let camera_probe = SkyCameraProbe(Arc::new(Mutex::new(None)));
    let mut app = App::new();
    app.add_plugins(
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
    .add_plugins(ExtractResourcePlugin::<SkyCameraProbe>::default())
    .insert_resource(camera_probe)
    .insert_resource(TacticalCameraSetup {
        translation: view_configuration.camera_translation,
        direction: view_configuration.camera_direction,
        vertical_fov_degrees: view_configuration.vertical_fov_degrees,
    })
    .add_plugins({
        let mut plugin = TacticalPresentationPlugin::default();
        plugin.config.rendering.vista.maximum_lods = 0;
        plugin.config.rendering.clouds.quality_scale = 1.0;
        plugin.config.rendering.clouds.resolution_scale = 1.0;
        plugin.config.rendering.anti_aliasing =
            crate::presentation::AntiAliasingConfig::Msaa { samples: 4 };
        plugin
    })
    .insert_resource(TacticalCloudCaptureOverride(Some(cloud_capture_profile(
        view,
    ))))
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(CaptureState {
        output,
        settle_frames,
        settled: 0,
        prime_complete: false,
        in_flight: false,
        view,
        absolute_minute: view_configuration.absolute_minute,
        sun_altitude_degrees: view_configuration.sun_altitude_degrees,
        moon_altitude_degrees: view_configuration.moon_altitude_degrees,
        lunar_illumination: view_configuration.lunar_illumination,
        camera_translation: view_configuration.camera_translation.to_array(),
        camera_direction: view_configuration.camera_direction.to_array(),
        vertical_fov_degrees: view_configuration.vertical_fov_degrees,
        observed_camera: None,
        camera_observation_ready: false,
    })
    .add_systems(PostStartup, move |world: &mut World| {
        setup_view(world, view)
    })
    .add_systems(Last, capture_view);
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        observe_extracted_sky_camera.in_set(RenderSystems::PrepareViews),
    );
    let exit = app.run();
    if exit != AppExit::Success {
        std::process::exit(1);
    }
}

fn sky_view_configuration(view: SkyView) -> SkyViewConfiguration {
    let absolute_minute = match view {
        SkyView::Sun => 172 * MINUTES_PER_DAY + 12 * 60,
        // Clear midsummer low Sun: demonstrates atmospheric extinction and
        // aureole structure without changing the production solar model.
        SkyView::SunDetail => 172 * MINUTES_PER_DAY + 19 * 60,
        SkyView::Twilight => 80 * MINUTES_PER_DAY + 18 * 60,
        // Day 249 23:00: dark sky, Moon +24.6 degrees and 99% illuminated.
        // Keep this identical to the compact matrix's verified moonlit slot.
        SkyView::Moon => 359_940,
        // Canonical new moon at 23:00 on day 77.
        SkyView::Stars => 637_860,
        SkyView::CloudCumulus
        | SkyView::CloudStratocumulus
        | SkyView::CloudCirrus
        | SkyView::CloudOvercast
        | SkyView::CloudStorm => 172 * MINUTES_PER_DAY + 15 * 60,
    };
    let celestial = celestial_directions(absolute_minute, LATITUDE, LONGITUDE);
    let sun = to_bevy_direction(celestial.sun);
    let moon = to_bevy_direction(celestial.moon);
    let view_direction = match view {
        SkyView::Sun => horizon_view(sun, 0.5),
        SkyView::SunDetail => sun,
        SkyView::Twilight => horizon_view(sun, 0.03),
        SkyView::Moon => moon,
        SkyView::Stars => Vec3::new(0.15, 0.55, -0.82).normalize(),
        SkyView::CloudCumulus
        | SkyView::CloudStratocumulus
        | SkyView::CloudCirrus
        | SkyView::CloudOvercast
        | SkyView::CloudStorm => horizon_view(sun, 0.34),
    };
    SkyViewConfiguration {
        absolute_minute,
        sun_altitude_degrees: celestial.sun[1].asin().to_degrees(),
        moon_altitude_degrees: celestial.moon[1].asin().to_degrees(),
        lunar_illumination: celestial.lunar_illumination,
        camera_translation: Vec3::new(0.0, 2.0, 8.0),
        camera_direction: view_direction,
        vertical_fov_degrees: if matches!(view, SkyView::Moon) {
            12.0
        } else if matches!(view, SkyView::SunDetail) {
            20.0
        } else if is_cloud_view(view) {
            72.0
        } else {
            80.0
        },
    }
}

fn setup_view(world: &mut World, view: SkyView) {
    let configuration = sky_view_configuration(view);
    let absolute_minute = configuration.absolute_minute;
    let mut camera_query =
        world.query_filtered::<(&Transform, &Projection), With<TacticalGameplayCamera>>();
    let (camera_transform, camera_projection) =
        camera_query.single(world).expect("one tactical camera");
    let live_fov_degrees = match camera_projection {
        Projection::Perspective(perspective) => perspective.fov.to_degrees(),
        _ => panic!("sky diagnostics require a perspective camera"),
    };
    assert!(
        camera_transform
            .translation
            .abs_diff_eq(configuration.camera_translation, 1e-5)
    );
    assert!(
        camera_transform
            .forward()
            .as_vec3()
            .abs_diff_eq(configuration.camera_direction, 1e-5)
    );
    assert!((live_fov_degrees - configuration.vertical_fov_degrees).abs() <= 1e-5);

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
    let (capture_precipitation, capture_intensity_bps) = if matches!(view, SkyView::CloudStorm) {
        (Precipitation::Rain, 9_500)
    } else {
        (Precipitation::Clear, 0)
    };
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
            latitude_microdegrees: LATITUDE.get(),
            longitude_microdegrees: LONGITUDE.get(),
            absolute_minute,
            lunar_phase_minute: absolute_minute,
            absolute_elevation_metres: 20,
            weather: WeatherSnapshot {
                rules_version: WEATHER_RULES_VERSION,
                interval_start_minute: absolute_minute,
                cell_latitude: 0,
                cell_longitude: 0,
                temperature_deci_c: 150,
                wind_speed_bps: 0,
                precipitation: capture_precipitation,
                intensity_bps: capture_intensity_bps,
                ground_moisture_bps: 0,
                snow_cover_bps: 0,
                atmosphere: Default::default(),
            },
            canopy_bps: 0,
            wetland_bps: 0,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        },
    ));

    // The baked path begins with Bevy's physical atmosphere on the camera and
    // creates a one-shot LightProbe after this PostStartup scene becomes active.
    assert!(
        world
            .query_filtered::<Entity, (With<TacticalGameplayCamera>, With<AtmosphereSettings>)>()
            .single(world)
            .is_ok()
    );
}

fn observe_extracted_sky_camera(
    views: Query<&ExtractedView, With<Camera3d>>,
    probe: Res<SkyCameraProbe>,
) {
    let Ok(view) = views.single() else {
        return;
    };
    let projection_scale_y = view.clip_from_view.y_axis.y;
    if !projection_scale_y.is_finite() || projection_scale_y <= 0.0 {
        return;
    }
    let mut observation = probe.0.lock().expect("sky camera probe lock");
    let render_observations = observation.map_or(1, |previous| previous.render_observations + 1);
    *observation = Some(ObservedSkyCamera {
        translation: view.world_from_view.translation(),
        direction: view.world_from_view.forward().as_vec3(),
        vertical_fov_degrees: (2.0 * projection_scale_y.recip().atan()).to_degrees(),
        render_observations,
    });
}

fn camera_observation_matches(
    observed: ObservedSkyCamera,
    requested_translation: Vec3,
    requested_direction: Vec3,
    requested_fov_degrees: f32,
) -> bool {
    observed.render_observations >= 2
        && observed
            .translation
            .abs_diff_eq(requested_translation, 1e-4)
        && observed.direction.abs_diff_eq(requested_direction, 1e-4)
        && (observed.vertical_fov_degrees - requested_fov_degrees).abs() <= 1e-3
}

fn capture_view(
    mut commands: Commands,
    mut state: ResMut<CaptureState>,
    camera: Single<(&Exposure, &GlobalTransform, &Projection), With<TacticalGameplayCamera>>,
    camera_probe: Res<SkyCameraProbe>,
    sunlight: Single<&DirectionalLight, With<crate::presentation::TacticalSunlight>>,
) {
    if state.in_flight {
        return;
    }
    let Some(render_observed) = *camera_probe.0.lock().expect("sky camera probe lock") else {
        return;
    };
    let main_fov_degrees = match camera.2 {
        Projection::Perspective(perspective) => perspective.fov.to_degrees(),
        _ => f32::NAN,
    };
    let requested_translation = Vec3::from_array(state.camera_translation);
    let requested_direction = Vec3::from_array(state.camera_direction);
    let main_observed = ObservedSkyCamera {
        translation: camera.1.translation(),
        direction: camera.1.forward().as_vec3(),
        vertical_fov_degrees: main_fov_degrees,
        render_observations: render_observed.render_observations,
    };
    state.observed_camera = Some(render_observed);
    state.camera_observation_ready = camera_observation_matches(
        render_observed,
        requested_translation,
        requested_direction,
        state.vertical_fov_degrees,
    ) && camera_observation_matches(
        main_observed,
        requested_translation,
        requested_direction,
        state.vertical_fov_degrees,
    );
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
    let exposure_ev100 = camera.0.ev100;
    let solar_source_illuminance_lux = sunlight.illuminance;
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              state: Res<CaptureState>,
              mut exit: MessageWriter<AppExit>| {
            let horizon_row = projected_horizon_row(
                captured.image.height(),
                state.camera_direction[1].asin(),
                state.vertical_fov_degrees.to_radians(),
            );
            let metrics = sky_metrics(
                captured.image.data.as_deref(),
                captured.image.width(),
                captured.image.height(),
                horizon_row,
            );
            let expected_dimensions =
                captured.image.width() == VIEW_WIDTH && captured.image.height() == VIEW_HEIGHT;
            let subject_content = match state.view {
                SkyView::Sun | SkyView::SunDetail | SkyView::Moon => {
                    metrics.bright_pixel_count >= 16
                }
                SkyView::Twilight => twilight_subject_content(&metrics),
                SkyView::Stars => metrics.bright_pixel_count >= 8,
                SkyView::CloudCumulus
                | SkyView::CloudStratocumulus
                | SkyView::CloudCirrus
                | SkyView::CloudOvercast
                | SkyView::CloudStorm => metrics.upper_sky_luma_variance >= 12.0,
            };
            let validation = SkyValidation {
                expected_dimensions,
                non_black_content: metrics.non_black_pixel_count >= 256,
                subject_content,
                camera_observation_ready: state.camera_observation_ready,
                passed: expected_dimensions
                    && metrics.non_black_pixel_count >= 256
                    && subject_content
                    && state.camera_observation_ready,
            };
            let observed_camera = state.observed_camera.unwrap_or(ObservedSkyCamera {
                translation: Vec3::splat(f32::NAN),
                direction: Vec3::splat(f32::NAN),
                vertical_fov_degrees: f32::NAN,
                render_observations: 0,
            });
            let manifest = SkyManifest {
                pipeline: "tactical_sky_native_capture_v4",
                view: sky_view_slug(state.view),
                absolute_minute: state.absolute_minute,
                resolution: [VIEW_WIDTH, VIEW_HEIGHT],
                settle_frames: state.settle_frames,
                camera_version: 1,
                requested_camera_translation: state.camera_translation,
                requested_camera_direction: state.camera_direction,
                requested_vertical_fov_degrees: state.vertical_fov_degrees,
                camera_translation: observed_camera.translation.to_array(),
                camera_direction: observed_camera.direction.to_array(),
                vertical_fov_degrees: observed_camera.vertical_fov_degrees,
                render_camera_observations: observed_camera.render_observations,
                sun_altitude_degrees: state.sun_altitude_degrees,
                moon_altitude_degrees: state.moon_altitude_degrees,
                lunar_illumination: state.lunar_illumination,
                bright_pixel_bps: metrics.bright_pixel_bps,
                non_black_pixel_bps: metrics.non_black_pixel_bps,
                bright_pixel_count: metrics.bright_pixel_count,
                non_black_pixel_count: metrics.non_black_pixel_count,
                horizon_luma_delta: metrics.horizon_luma_delta,
                upper_sky_mean_luma: metrics.upper_sky_mean_luma,
                upper_sky_luma_variance: metrics.upper_sky_luma_variance,
                horizon_sky_mean_luma: metrics.horizon_sky_mean_luma,
                horizon_sky_red_blue_delta: metrics.horizon_sky_red_blue_delta,
                sun_disk_angular_diameter_degrees: SunDisk::EARTH.angular_size.to_degrees(),
                diagnostic_magnification: matches!(state.view, SkyView::SunDetail),
                cloud_profile: cloud_profile_slug(state.view),
                exposure_ev100,
                solar_source_illuminance_lux,
                atmosphere_enabled: true,
                environment_map_size: 64,
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
    upper_sky_mean_luma: f32,
    upper_sky_luma_variance: f32,
    horizon_sky_mean_luma: f32,
    horizon_sky_red_blue_delta: f32,
}

fn sky_metrics(data: Option<&[u8]>, width: u32, height: u32, horizon_row: usize) -> SkyMetrics {
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
    let mut upper_sky_sum = 0.0_f64;
    let mut upper_sky_squared_sum = 0.0_f64;
    let mut upper_sky_count = 0usize;
    let mut horizon_sky_luma_sum = 0.0_f64;
    let mut horizon_sky_red_blue_sum = 0.0_f64;
    let mut horizon_sky_count = 0usize;
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
        let y = index / width.max(1) as usize;
        let sky_horizon = horizon_row.min(height as usize);
        if y < sky_horizon.saturating_sub(height as usize / 8) {
            upper_sky_sum += f64::from(luma);
            upper_sky_squared_sum += f64::from(luma).powi(2);
            upper_sky_count += 1;
        } else if y < sky_horizon {
            horizon_sky_luma_sum += f64::from(luma);
            horizon_sky_red_blue_sum += f64::from(pixel[0]) - f64::from(pixel[2]);
            horizon_sky_count += 1;
        }
    }
    let bps = |count: usize| {
        adventuresim_world_schema::UnitBasisPoints::from_ratio_floor(
            count as u64,
            pixels.len() as u64,
        )
        .expect("sky capture has pixels")
        .get()
    };
    let upper_sky_mean = upper_sky_sum / upper_sky_count.max(1) as f64;
    SkyMetrics {
        bright_pixel_bps: bps(bright),
        non_black_pixel_bps: bps(non_black),
        bright_pixel_count: bright,
        non_black_pixel_count: non_black,
        horizon_luma_delta: ((upper as f64 / split as f64)
            - (lower as f64 / (pixels.len() - split) as f64))
            .abs() as f32,
        upper_sky_mean_luma: upper_sky_mean as f32,
        upper_sky_luma_variance: (upper_sky_squared_sum / upper_sky_count.max(1) as f64
            - upper_sky_mean * upper_sky_mean)
            .max(0.0) as f32,
        horizon_sky_mean_luma: (horizon_sky_luma_sum / horizon_sky_count.max(1) as f64) as f32,
        horizon_sky_red_blue_delta: (horizon_sky_red_blue_sum / horizon_sky_count.max(1) as f64)
            as f32,
    }
}

fn projected_horizon_row(height: u32, camera_elevation: f32, vertical_fov: f32) -> usize {
    let focal_y = height as f32 / (2.0 * (vertical_fov * 0.5).tan());
    (height as f32 * 0.5 + camera_elevation.tan() * focal_y)
        .round()
        .clamp(0.0, height as f32) as usize
}

fn twilight_subject_content(metrics: &SkyMetrics) -> bool {
    metrics.upper_sky_mean_luma >= 6.0
        && metrics.upper_sky_luma_variance >= 4.0
        && metrics.horizon_sky_mean_luma >= metrics.upper_sky_mean_luma + 3.0
        && metrics.horizon_sky_red_blue_delta >= 3.0
}

fn sky_view_slug(view: SkyView) -> &'static str {
    match view {
        SkyView::Sun => "sun",
        SkyView::SunDetail => "sun-detail",
        SkyView::Twilight => "twilight",
        SkyView::Moon => "moon",
        SkyView::Stars => "stars",
        SkyView::CloudCumulus => "cloud-cumulus",
        SkyView::CloudStratocumulus => "cloud-stratocumulus",
        SkyView::CloudCirrus => "cloud-cirrus",
        SkyView::CloudOvercast => "cloud-overcast",
        SkyView::CloudStorm => "cloud-storm",
    }
}

fn is_cloud_view(view: SkyView) -> bool {
    matches!(
        view,
        SkyView::CloudCumulus
            | SkyView::CloudStratocumulus
            | SkyView::CloudCirrus
            | SkyView::CloudOvercast
            | SkyView::CloudStorm
    )
}

fn cloud_capture_profile(view: SkyView) -> TacticalCloudCaptureProfile {
    match view {
        SkyView::CloudCumulus => TacticalCloudCaptureProfile::Cumulus,
        SkyView::CloudStratocumulus => TacticalCloudCaptureProfile::Stratocumulus,
        SkyView::CloudCirrus => TacticalCloudCaptureProfile::Cirrus,
        SkyView::CloudOvercast => TacticalCloudCaptureProfile::Overcast,
        SkyView::CloudStorm => TacticalCloudCaptureProfile::Storm,
        _ => TacticalCloudCaptureProfile::Clear,
    }
}

fn cloud_profile_slug(view: SkyView) -> Option<&'static str> {
    match view {
        SkyView::CloudCumulus => Some("cumulus"),
        SkyView::CloudStratocumulus => Some("stratocumulus"),
        SkyView::CloudCirrus => Some("cirrus"),
        SkyView::CloudOvercast => Some("overcast"),
        SkyView::CloudStorm => Some("storm"),
        _ => None,
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
    fn narrow_diagnostic_projections_are_frozen_before_startup() {
        let sun = sky_view_configuration(SkyView::SunDetail);
        let moon = sky_view_configuration(SkyView::Moon);
        assert_eq!(sun.vertical_fov_degrees, 20.0);
        assert_eq!(moon.vertical_fov_degrees, 12.0);
        assert!(sun.camera_direction.is_normalized());
        assert!(moon.camera_direction.is_normalized());
        assert_ne!(sun.camera_direction, moon.camera_direction);
        assert!(moon.sun_altitude_degrees < -12.0);
        assert!(moon.moon_altitude_degrees > 20.0);
        assert!(moon.lunar_illumination > 0.9);
        let observed = ObservedSkyCamera {
            translation: moon.camera_translation,
            direction: moon.camera_direction,
            vertical_fov_degrees: moon.vertical_fov_degrees,
            render_observations: 2,
        };
        assert!(camera_observation_matches(
            observed,
            moon.camera_translation,
            moon.camera_direction,
            moon.vertical_fov_degrees,
        ));
        assert!(!camera_observation_matches(
            ObservedSkyCamera {
                vertical_fov_degrees: 20.0,
                ..observed
            },
            moon.camera_translation,
            moon.camera_direction,
            moon.vertical_fov_degrees,
        ));
    }

    #[test]
    fn black_sky_cannot_satisfy_content_metrics() {
        let data = vec![0; VIEW_WIDTH as usize * VIEW_HEIGHT as usize * 4];
        let metrics = sky_metrics(
            Some(&data),
            VIEW_WIDTH,
            VIEW_HEIGHT,
            VIEW_HEIGHT as usize / 2,
        );
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
        let metrics = sky_metrics(Some(&data), 4, 4, 2);
        assert!(metrics.non_black_pixel_bps > 0);
        assert!(metrics.bright_pixel_bps > 0);
        assert!(metrics.horizon_luma_delta > 3.0);
    }

    #[test]
    fn twilight_gate_rejects_black_sky_over_blue_ground_and_accepts_warm_gradient() {
        let width = 160_u32;
        let height = 90_u32;
        let horizon = 47_usize;
        let mut black_sky = vec![0_u8; width as usize * height as usize * 4];
        for y in horizon..height as usize {
            for x in 0..width as usize {
                black_sky[(y * width as usize + x) * 4..][..4].copy_from_slice(&[12, 25, 45, 255]);
            }
        }
        assert!(!twilight_subject_content(&sky_metrics(
            Some(&black_sky),
            width,
            height,
            horizon
        )));

        let mut twilight = vec![0_u8; width as usize * height as usize * 4];
        for y in 0..height as usize {
            let t = (y as f32 / horizon as f32).clamp(0.0, 1.0);
            let pixel = if y < horizon {
                [
                    (45.0 + 155.0 * t) as u8,
                    (40.0 + 65.0 * t) as u8,
                    (65.0 - 35.0 * t) as u8,
                    255,
                ]
            } else {
                [12, 25, 45, 255]
            };
            for x in 0..width as usize {
                twilight[(y * width as usize + x) * 4..][..4].copy_from_slice(&pixel);
            }
        }
        assert!(twilight_subject_content(&sky_metrics(
            Some(&twilight),
            width,
            height,
            horizon
        )));
    }

    #[test]
    fn projected_horizon_tracks_camera_pitch() {
        assert_eq!(projected_horizon_row(900, 0.0, 80.0_f32.to_radians()), 450);
        assert!(projected_horizon_row(900, 0.03, 80.0_f32.to_radians()) > 450);
    }
}
