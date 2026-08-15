use super::*;

/// Presentation-owned selector for the one playable tactical scene.
///
/// Scene data remains authoritative on its entity. This resource only makes
/// the presentation-wide choice explicit: the most recently activated live
/// scene entity wins, and removal deterministically falls back to the prior
/// live scene.
#[derive(Resource, Debug, Clone, Default)]
pub(in crate::presentation) struct ActiveTacticalScene {
    pub(in crate::presentation) entity: Option<Entity>,
    activation_order: Vec<Entity>,
}

pub(in crate::presentation) fn activate_tactical_scene(
    event: On<Add, SceneEnvironment>,
    mut active: ResMut<ActiveTacticalScene>,
) {
    active
        .activation_order
        .retain(|entity| *entity != event.entity);
    active.activation_order.push(event.entity);
    active.entity = Some(event.entity);
}

pub(in crate::presentation) fn refresh_active_tactical_scene(
    environments: Query<(), With<SceneEnvironment>>,
    mut active: ResMut<ActiveTacticalScene>,
) {
    let selected = active
        .activation_order
        .iter()
        .rev()
        .copied()
        .find(|entity| environments.contains(*entity));
    if selected == active.entity
        && active
            .activation_order
            .iter()
            .all(|entity| environments.contains(*entity))
    {
        return;
    }
    active
        .activation_order
        .retain(|entity| environments.contains(*entity));
    active.entity = selected;
}

pub(super) fn scene_sunlight_illuminance(
    environment: &SceneEnvironment,
    sun_altitude_degrees: f32,
) -> f32 {
    let intensity = f32::from(environment.weather.intensity_bps) / 10_000.0;
    let transmission = match environment.weather.precipitation {
        Precipitation::Clear => 1.0,
        Precipitation::Rain => 0.62 - intensity * 0.27,
        Precipitation::Snow => 0.72 - intensity * 0.22,
    };
    let altitude = (sun_altitude_degrees / 8.0).clamp(0.0, 1.0);
    let altitude_transmission = altitude * altitude * (3.0 - 2.0 * altitude);
    lux::RAW_SUNLIGHT * transmission.clamp(0.25, 1.0) * altitude_transmission
}

/// Solar source energy presented to Bevy's atmosphere. Unlike direct fallback
/// lighting, this must remain available while the Sun is below the horizon so
/// the atmosphere can scatter civil and nautical twilight. Bevy's atmosphere
/// transmittance and visible-disc calculation prevent this source from lighting
/// ground surfaces from below the planet horizon.
pub(super) fn scene_atmosphere_solar_illuminance(environment: &SceneEnvironment) -> f32 {
    let intensity = f32::from(environment.weather.intensity_bps) / 10_000.0;
    let transmission = match environment.weather.precipitation {
        Precipitation::Clear => 1.0,
        Precipitation::Rain => 0.62 - intensity * 0.27,
        Precipitation::Snow => 0.72 - intensity * 0.22,
    };
    lux::RAW_SUNLIGHT * transmission.clamp(0.25, 1.0)
}

pub(super) fn scene_distance_fog(environment: &SceneEnvironment) -> DistanceFog {
    let intensity = f32::from(environment.weather.intensity_bps) / 10_000.0;
    let (start, end, color) = match environment.weather.precipitation {
        // Even clear continental air loses contrast over kilometres. Starting
        // the blend beyond tactical ranges gives regional ridges and tree
        // lines depth without washing out nearby gameplay silhouettes.
        Precipitation::Clear => (2_500.0, 42_000.0, Color::srgb_u8(188, 201, 207)),
        Precipitation::Rain => (
            800.0 + 3_500.0 * (1.0 - intensity),
            3_000.0 + 14_000.0 * (1.0 - intensity),
            Color::srgb_u8(112, 126, 135),
        ),
        Precipitation::Snow => (
            600.0 + 2_500.0 * (1.0 - intensity),
            2_500.0 + 10_000.0 * (1.0 - intensity),
            Color::srgb_u8(207, 216, 220),
        ),
    };
    DistanceFog {
        color,
        falloff: FogFalloff::Linear { start, end },
        ..default()
    }
}

pub(super) fn scene_night_factor(
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
) -> f32 {
    let daylight = smoothstep(-8.0, 2.0, sun_altitude_degrees);
    let moon = lunar_illumination * smoothstep(-2.0, 8.0, moon_altitude_degrees);
    (daylight + moon * 0.12).clamp(0.0, 1.0)
}

pub(super) fn scene_ambient_response(
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
) -> f32 {
    let daylight = smoothstep(-8.0, 4.0, sun_altitude_degrees);
    let moon = lunar_illumination * smoothstep(-2.0, 8.0, moon_altitude_degrees);
    (0.05 + daylight * 0.23 + moon * 0.04).clamp(0.05, 0.28)
}

pub(crate) fn scene_ambient_light(
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
) -> (Vec3, f32) {
    let daylight = smoothstep(-8.0, 4.0, sun_altitude_degrees);
    let moon = lunar_illumination * smoothstep(-2.0, 8.0, moon_altitude_degrees);
    let night_color = Vec3::new(0.36, 0.48, 0.72);
    let color = night_color.lerp(Vec3::ONE, daylight);
    // GlobalAmbientLight is our inexpensive approximation of hemispherical
    // sky irradiance and unresolved multi-bounce light. Outdoor daylight has
    // tens of thousands of lux of diffuse illumination even where direct sun
    // is occluded; the former value of 80 was effectively black at EV100 15.
    // Preserve the deliberately dim moonless-night floor independently.
    let brightness = 0.6 + daylight * 29_999.4 + moon * 0.25;
    (color, brightness)
}

/// Bounded unresolved multi-bounce term retained alongside generated atmosphere
/// IBL. The directional map owns first-bounce sky diffuse/specular response;
/// this isotropic term preserves outdoor material readability without restoring
/// the former full-strength duplicate sky approximation.
pub(crate) fn scene_ibl_visibility_floor(
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
) -> (Vec3, f32) {
    let daylight = smoothstep(-8.0, 4.0, sun_altitude_degrees);
    let moon = lunar_illumination * smoothstep(-2.0, 8.0, moon_altitude_degrees);
    let color = Vec3::new(0.36, 0.48, 0.72).lerp(Vec3::ONE, daylight);
    let night_floor = 0.6 + moon * 0.25;
    let daylight_multibounce = 10_500.0 * daylight;
    (color, night_floor * (1.0 - daylight) + daylight_multibounce)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn tactical_msaa() -> Msaa {
    // Fixed-function 4x hardware resolve is supported by the WebGPU path and
    // is particularly valuable for thin grass and branch silhouettes.
    Msaa::Sample4
}

#[derive(Resource, Clone, Copy)]
pub(crate) struct TacticalCameraSetup {
    pub(crate) translation: Vec3,
    pub(crate) direction: Vec3,
    pub(crate) vertical_fov_degrees: f32,
}

impl Default for TacticalCameraSetup {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            direction: Vec3::NEG_Z,
            vertical_fov_degrees: 80.0,
        }
    }
}

pub(in crate::presentation) fn setup_tactical_presentation(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    settings: Res<TacticalGraphicsSettings>,
    camera_setup: Res<TacticalCameraSetup>,
) {
    if settings.atmosphere_enabled {
        commands.spawn(Atmosphere::earth(
            scattering_mediums.add(ScatteringMedium::default()),
        ));
    }

    let mut camera = commands.spawn((
        Name::new("Tactical gameplay camera"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: camera_setup.vertical_fov_degrees.to_radians(),
            far: 60_000.0,
            ..default()
        }),
        Transform::from_translation(camera_setup.translation)
            .looking_to(camera_setup.direction, Vec3::Y),
        Exposure::SUNLIGHT,
        Tonemapping::AcesFitted,
        DistanceFog {
            color: Color::srgb_u8(188, 201, 207),
            falloff: FogFalloff::Linear {
                start: 30_000.0,
                end: 50_000.0,
            },
            ..default()
        },
        tactical_msaa(),
    ));
    if settings.atmosphere_enabled {
        camera.insert(AtmosphereSettings::default());
        if settings.environment_light_enabled {
            camera.insert(AtmosphereEnvironmentMapLight {
                size: UVec2::splat(settings.environment_map_size),
                ..default()
            });
        }
    }
    if settings.bloom_enabled {
        camera.insert(Bloom::NATURAL);
    }
}

pub(super) fn apply_active_environment_fog(
    active: Res<ActiveTacticalScene>,
    environments: Query<Ref<SceneEnvironment>>,
    mut fog: Single<&mut DistanceFog, With<Camera3d>>,
) {
    let Some(entity) = active.entity else {
        if active.is_changed() {
            **fog = scene_distance_fog(&legacy_scene_environment(&SceneId("default".into())));
        }
        return;
    };
    let Ok(environment) = environments.get(entity) else {
        return;
    };
    if !active.is_changed() && !environment.is_changed() {
        return;
    }
    **fog = scene_distance_fog(&environment);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(precipitation: Precipitation, intensity_bps: u16) -> SceneEnvironment {
        SceneEnvironment {
            scene_digest: "fixture".into(),
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 12 * 60,
            absolute_elevation_metres: 20,
            weather: WeatherSnapshot {
                rules_version: WEATHER_RULES_VERSION,
                interval_start_minute: 0,
                cell_latitude: 0,
                cell_longitude: 0,
                temperature_deci_c: 100,
                wind_speed_bps: 0,
                precipitation,
                intensity_bps,
                ground_moisture_bps: 0,
                snow_cover_bps: 0,
            },
            canopy_bps: 0,
            wetland_bps: 0,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        }
    }

    #[test]
    fn daylight_precipitation_dims_but_never_extinguishes_sunlight() {
        let clear = scene_sunlight_illuminance(&environment(Precipitation::Clear, 0), 30.0);
        let rain = scene_sunlight_illuminance(&environment(Precipitation::Rain, 10_000), 30.0);
        let snow = scene_sunlight_illuminance(&environment(Precipitation::Snow, 10_000), 30.0);
        assert!(rain < snow && snow < clear);
        assert!(rain >= lux::RAW_SUNLIGHT * 0.25);
    }

    #[test]
    fn gameplay_uses_four_sample_webgpu_hardware_msaa() {
        assert_eq!(tactical_msaa(), Msaa::Sample4);
    }

    #[test]
    fn direct_sunlight_never_shines_upward_from_below_the_horizon() {
        let clear = environment(Precipitation::Clear, 0);
        assert_eq!(scene_sunlight_illuminance(&clear, -45.0), 0.0);
        assert_eq!(scene_sunlight_illuminance(&clear, -0.01), 0.0);
        assert_eq!(scene_sunlight_illuminance(&clear, 0.0), 0.0);

        let low = scene_sunlight_illuminance(&clear, 2.0);
        let risen = scene_sunlight_illuminance(&clear, 6.0);
        let daylight = scene_sunlight_illuminance(&clear, 8.0);
        assert!(0.0 < low && low < risen && risen < daylight);
        assert_eq!(daylight, lux::RAW_SUNLIGHT);
    }

    #[test]
    fn atmosphere_retains_a_bounded_solar_source_through_twilight() {
        let clear = environment(Precipitation::Clear, 0);
        let rain = environment(Precipitation::Rain, 10_000);
        assert_eq!(
            scene_atmosphere_solar_illuminance(&clear),
            lux::RAW_SUNLIGHT
        );
        assert!(scene_atmosphere_solar_illuminance(&rain) > 0.0);
        assert!(scene_atmosphere_solar_illuminance(&rain) < lux::RAW_SUNLIGHT);
    }

    #[test]
    fn heavy_weather_reduces_visibility_below_clear_horizon() {
        let clear = scene_distance_fog(&environment(Precipitation::Clear, 0));
        let rain = scene_distance_fog(&environment(Precipitation::Rain, 10_000));
        let snow = scene_distance_fog(&environment(Precipitation::Snow, 10_000));
        let fog_end = |fog: &DistanceFog| match &fog.falloff {
            FogFalloff::Linear { end, .. } => *end,
            _ => panic!("scene weather must use bounded linear fog"),
        };
        assert!(fog_end(&rain) < fog_end(&clear));
        assert!(fog_end(&snow) < fog_end(&clear));
    }

    #[test]
    fn clear_air_haze_starts_beyond_tactical_gameplay_range() {
        let clear = scene_distance_fog(&environment(Precipitation::Clear, 0));
        let FogFalloff::Linear { start, end } = clear.falloff else {
            panic!("clear weather must use bounded linear fog");
        };
        assert!(start >= 2_000.0);
        assert!(start < 5_000.0);
        assert!(end >= 40_000.0);
    }

    #[test]
    fn night_material_response_is_dim_and_moon_sensitive() {
        let moonless = scene_night_factor(-25.0, 30.0, 0.0);
        let moonlit = scene_night_factor(-25.0, 30.0, 1.0);
        let daylight = scene_night_factor(30.0, -20.0, 0.0);
        assert_eq!(moonless, 0.0);
        assert!(moonless < moonlit && moonlit < 0.2);
        assert_eq!(daylight, 1.0);
        let (night_color, moonless_ambient) = scene_ambient_light(-25.0, 30.0, 0.0);
        let (_, moonlit_ambient) = scene_ambient_light(-25.0, 30.0, 1.0);
        let (day_color, daylight_ambient) = scene_ambient_light(30.0, -20.0, 0.0);
        assert_eq!(night_color, Vec3::new(0.36, 0.48, 0.72));
        assert!((moonless_ambient - 0.6).abs() < f32::EPSILON);
        assert!(moonlit_ambient > moonless_ambient && moonlit_ambient <= 0.85);
        assert_eq!(day_color, Vec3::ONE);
        assert!((daylight_ambient - 30_000.0).abs() < f32::EPSILON);
        assert!((scene_ambient_response(-25.0, -20.0, 0.0) - 0.05).abs() < f32::EPSILON);
        assert!((scene_ambient_response(30.0, -20.0, 0.0) - 0.28).abs() < f32::EPSILON);
    }

    #[test]
    fn atmosphere_ibl_floor_preserves_night_and_bounds_daylight_multibounce() {
        let (_, day) = scene_ibl_visibility_floor(30.0, -20.0, 0.0);
        let (_, moonless) = scene_ibl_visibility_floor(-25.0, 30.0, 0.0);
        let (_, moonlit) = scene_ibl_visibility_floor(-25.0, 30.0, 1.0);
        assert_eq!(day, 10_500.0);
        assert_eq!(moonless, 0.6);
        assert!((0.84..=0.85).contains(&moonlit));
    }

    #[test]
    fn active_scene_is_latest_regardless_of_query_iteration_and_falls_back() {
        let mut app = App::new();
        app.init_resource::<ActiveTacticalScene>()
            .add_observer(activate_tactical_scene)
            .add_systems(Update, refresh_active_tactical_scene);
        let first_environment = environment(Precipitation::Clear, 0);
        let mut second_environment = environment(Precipitation::Rain, 7_000);
        second_environment.scene_digest = "second".into();
        second_environment.absolute_minute += 60;
        let first = app.world_mut().spawn(first_environment.clone()).id();
        app.update();
        assert_eq!(
            app.world().resource::<ActiveTacticalScene>().entity,
            Some(first)
        );

        let second = app.world_mut().spawn(second_environment.clone()).id();
        app.update();
        let active = app.world().resource::<ActiveTacticalScene>();
        assert_eq!(active.entity, Some(second));
        assert_eq!(
            app.world().get::<SceneEnvironment>(second),
            Some(&second_environment)
        );

        app.world_mut().despawn(second);
        app.update();
        let active = app.world().resource::<ActiveTacticalScene>();
        assert_eq!(active.entity, Some(first));
        assert_eq!(
            app.world().get::<SceneEnvironment>(first),
            Some(&first_environment)
        );

        app.world_mut().despawn(first);
        app.update();
        assert_eq!(app.world().resource::<ActiveTacticalScene>().entity, None);
    }

    #[test]
    fn replacing_active_environment_refreshes_snapshot_without_entity_change() {
        let mut app = App::new();
        app.init_resource::<ActiveTacticalScene>()
            .add_observer(activate_tactical_scene)
            .add_systems(Update, refresh_active_tactical_scene);
        let entity = app
            .world_mut()
            .spawn(environment(Precipitation::Clear, 0))
            .id();
        app.update();

        let mut replacement = environment(Precipitation::Snow, 8_000);
        replacement.scene_digest = "replacement".into();
        replacement.absolute_minute += 720;
        app.world_mut()
            .entity_mut(entity)
            .insert(replacement.clone());
        app.update();

        let active = app.world().resource::<ActiveTacticalScene>();
        assert_eq!(active.entity, Some(entity));
        assert_eq!(
            app.world().get::<SceneEnvironment>(entity),
            Some(&replacement)
        );
    }

    #[test]
    fn activation_order_survives_entity_recycling() {
        let mut app = App::new();
        app.init_resource::<ActiveTacticalScene>()
            .add_observer(activate_tactical_scene)
            .add_systems(Update, refresh_active_tactical_scene);
        let first = app
            .world_mut()
            .spawn(environment(Precipitation::Clear, 0))
            .id();
        let second = app
            .world_mut()
            .spawn(environment(Precipitation::Rain, 3_000))
            .id();
        app.update();
        assert_eq!(
            app.world().resource::<ActiveTacticalScene>().entity,
            Some(second)
        );

        app.world_mut().despawn(second);
        let recycled = app
            .world_mut()
            .spawn(environment(Precipitation::Snow, 3_000))
            .id();
        app.update();
        assert_eq!(
            app.world().resource::<ActiveTacticalScene>().entity,
            Some(recycled)
        );

        app.world_mut().despawn(recycled);
        app.update();
        assert_eq!(
            app.world().resource::<ActiveTacticalScene>().entity,
            Some(first)
        );
    }
}
