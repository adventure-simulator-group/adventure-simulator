use super::*;

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
        Precipitation::Clear => (30_000.0, 50_000.0, Color::srgb_u8(188, 201, 207)),
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

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(in crate::presentation) fn setup_tactical_presentation(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    settings: Res<TacticalGraphicsSettings>,
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
            fov: 80.0_f32.to_radians(),
            far: 60_000.0,
            ..default()
        }),
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
        // Gameplay MSAA is deliberately off even in the full preset.
        Msaa::Off,
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
    if settings.ssao_enabled {
        camera.insert(ScreenSpaceAmbientOcclusion::default());
    }
}

pub(super) fn on_environment_added(
    event: On<Add, SceneEnvironment>,
    environments: Query<&SceneEnvironment>,
    mut fog: Single<&mut DistanceFog, With<Camera3d>>,
) -> Result {
    let environment = environments.get(event.entity)?;
    **fog = scene_distance_fog(environment);
    Ok(())
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
}
