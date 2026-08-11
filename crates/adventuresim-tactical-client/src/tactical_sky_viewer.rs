use std::{fs, path::PathBuf};

use adventuresim_tactical_core::prelude::*;
use bevy::{
    app::AppExit,
    asset::AssetPlugin,
    camera::Exposure,
    light::AtmosphereEnvironmentMapLight,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};

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
                        resolution: (VIEW_WIDTH, VIEW_HEIGHT).into(),
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
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save_to_disk(&path)(captured);
            exit.write(AppExit::Success);
        },
    );
}

fn to_bevy_direction(east_up_north: [f32; 3]) -> Vec3 {
    Vec3::new(east_up_north[0], east_up_north[1], -east_up_north[2]).normalize()
}

fn horizon_view(direction: Vec3, altitude: f32) -> Vec3 {
    let horizontal = Vec3::new(direction.x, 0.0, direction.z).normalize_or_zero();
    (horizontal + Vec3::Y * altitude).normalize()
}
