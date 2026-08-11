//! Resolution-independent celestial presentation layered over Bevy's
//! physically based atmosphere.

use super::*;
use bevy::{camera::visibility::NoFrustumCulling, light::SunDisk};

const MOON_DISTANCE_METRES: f32 = 30_000.0;
const MOON_ANGULAR_RADIUS_RADIANS: f32 = 0.25_f32.to_radians();
const STAR_DISTANCE_METRES: f32 = 55_000.0;
const MOON_SHADER: &str = "shaders/tactical_moon.wgsl";
const STAR_SHADER: &str = "shaders/tactical_stars.wgsl";

const ATTRIBUTE_STAR_DIRECTION: MeshVertexAttribute =
    MeshVertexAttribute::new("StarDirection", 2_180_001, VertexFormat::Float32x3);
const ATTRIBUTE_STAR_COLOR_MAGNITUDE: MeshVertexAttribute =
    MeshVertexAttribute::new("StarColorMagnitude", 2_180_002, VertexFormat::Float32x4);
const ATTRIBUTE_STAR_CORNER: MeshVertexAttribute =
    MeshVertexAttribute::new("StarCorner", 2_180_003, VertexFormat::Float32x2);

#[derive(Component)]
pub(crate) struct TacticalSunlight;

#[derive(Component)]
pub(crate) struct TacticalMoonlight;

#[derive(Component)]
pub(crate) struct TacticalMoon;

#[derive(Component)]
pub(crate) struct TacticalStars;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalMoonMaterial {
    /// Direction toward the Sun and overall weather transmission.
    #[uniform(0)]
    light: Vec4,
    /// Earthshine floor, disc radiance, phase, reserved.
    #[uniform(1)]
    appearance: Vec4,
}

impl Material for TacticalMoonMaterial {
    fn vertex_shader() -> ShaderRef {
        MOON_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        MOON_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalStarMaterial {
    #[uniform(0)]
    equatorial_to_world: Mat4,
    /// Visibility, physical-pixel scale, HDR radiance scale, distance.
    #[uniform(1)]
    settings: Vec4,
}

impl Material for TacticalStarMaterial {
    fn vertex_shader() -> ShaderRef {
        STAR_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        STAR_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_STAR_DIRECTION.at_shader_location(1),
            ATTRIBUTE_STAR_COLOR_MAGNITUDE.at_shader_location(2),
            ATTRIBUTE_STAR_CORNER.at_shader_location(3),
        ])?];
        Ok(())
    }
}

pub(in crate::presentation) fn setup_tactical_sky(
    mut commands: Commands,
    settings: Res<TacticalGraphicsSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut moon_materials: ResMut<Assets<TacticalMoonMaterial>>,
    mut star_materials: ResMut<Assets<TacticalStarMaterial>>,
) {
    commands.spawn((
        Name::new("Tactical sunlight"),
        TacticalSunlight,
        SunDisk::EARTH,
        Transform::from_xyz(200.0, 1000.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
        DirectionalLight {
            shadow_maps_enabled: false,
            illuminance: 0.0,
            ..default()
        },
    ));

    commands.spawn((
        Name::new("Tactical moonlight"),
        TacticalMoonlight,
        SunDisk::OFF,
        Transform::from_xyz(-200.0, 1000.0, -100.0).looking_at(Vec3::ZERO, Vec3::Y),
        DirectionalLight {
            color: Color::srgb(0.72, 0.80, 1.0),
            illuminance: 0.0,
            shadow_maps_enabled: false,
            ..default()
        },
    ));

    if !settings.celestial_enabled {
        return;
    }

    commands.spawn((
        Name::new("Tactical Moon disc"),
        TacticalMoon,
        NotShadowCaster,
        Mesh3d(meshes.add(Sphere::new(1.0))),
        MeshMaterial3d(moon_materials.add(TacticalMoonMaterial {
            light: Vec4::Y,
            appearance: Vec4::new(0.025, 8.0, 0.0, 0.0),
        })),
        Transform::from_scale(Vec3::splat(
            MOON_DISTANCE_METRES * MOON_ANGULAR_RADIUS_RADIANS.tan(),
        )),
    ));

    commands.spawn((
        Name::new("Hipparcos naked-eye star field"),
        TacticalStars,
        NoFrustumCulling,
        NotShadowCaster,
        Mesh3d(meshes.add(star_mesh())),
        MeshMaterial3d(star_materials.add(TacticalStarMaterial {
            equatorial_to_world: Mat4::IDENTITY,
            settings: Vec4::new(0.0, 1.0, 24.0, STAR_DISTANCE_METRES),
        })),
    ));
}

pub(super) fn on_environment_added(
    event: On<Add, SceneEnvironment>,
    environments: Query<&SceneEnvironment>,
    settings: Res<TacticalGraphicsSettings>,
    sunlight: Single<(&mut DirectionalLight, &mut Transform), With<TacticalSunlight>>,
    moonlight: Single<
        (&mut DirectionalLight, &mut Transform),
        (With<TacticalMoonlight>, Without<TacticalSunlight>),
    >,
    camera: Single<&mut Exposure, With<Camera3d>>,
    moon: Query<&MeshMaterial3d<TacticalMoonMaterial>, With<TacticalMoon>>,
    stars: Query<&MeshMaterial3d<TacticalStarMaterial>, With<TacticalStars>>,
    mut moon_materials: ResMut<Assets<TacticalMoonMaterial>>,
    mut star_materials: ResMut<Assets<TacticalStarMaterial>>,
) -> Result {
    let environment = environments.get(event.entity)?;
    let celestial = celestial_directions(
        environment.absolute_minute,
        environment.latitude_microdegrees,
        environment.longitude_microdegrees,
    );
    let sun_direction = to_bevy_direction(celestial.sun);
    let moon_direction = to_bevy_direction(celestial.moon);
    let sun_altitude = sun_direction.y.asin().to_degrees();
    let moon_altitude = moon_direction.y.asin().to_degrees();
    let weather_transmission = sky_weather_transmission(environment);

    let (mut sun, mut sun_transform) = sunlight.into_inner();
    sun.illuminance = scene_sunlight_illuminance(environment, sun_altitude);
    sun.shadow_maps_enabled = settings.shadows_enabled && sun_altitude > 0.0;
    *sun_transform = light_transform(sun_direction);

    let (mut moon_light, mut moon_transform) = moonlight.into_inner();
    moon_light.illuminance = 0.25
        * celestial.lunar_illumination
        * smoothstep(-2.0, 4.0, moon_altitude)
        * weather_transmission;
    moon_light.shadow_maps_enabled = settings.shadows_enabled
        && sun_altitude <= -2.0
        && moon_altitude > 0.0
        && celestial.lunar_illumination > 0.15;
    *moon_transform = light_transform(moon_direction);

    camera.into_inner().ev100 = scene_exposure_ev100(sun_altitude, celestial.lunar_illumination);

    if let Ok(handle) = moon.single()
        && let Some(mut material) = moon_materials.get_mut(&handle.0)
    {
        material.light = sun_direction.extend(weather_transmission);
        material.appearance.z = celestial.lunar_phase;
    }
    if let Ok(handle) = stars.single()
        && let Some(mut material) = star_materials.get_mut(&handle.0)
    {
        material.equatorial_to_world = equatorial_to_world(
            environment.absolute_minute,
            environment.latitude_microdegrees,
            environment.longitude_microdegrees,
        );
        material.settings.x = star_visibility(sun_altitude) * weather_transmission;
    }
    Ok(())
}

pub(in crate::presentation) fn keep_celestial_visuals_centered(
    camera: Single<&GlobalTransform, With<Camera3d>>,
    environments: Query<&SceneEnvironment>,
    mut moon: Query<&mut Transform, With<TacticalMoon>>,
) {
    let Some(environment) = environments.iter().next() else {
        return;
    };
    let Ok(mut moon_transform) = moon.single_mut() else {
        return;
    };
    let celestial = celestial_directions(
        environment.absolute_minute,
        environment.latitude_microdegrees,
        environment.longitude_microdegrees,
    );
    moon_transform.translation =
        camera.translation() + to_bevy_direction(celestial.moon) * MOON_DISTANCE_METRES;
}

fn light_transform(direction_to_light: Vec3) -> Transform {
    Transform::from_translation(direction_to_light * 1_000.0).looking_at(Vec3::ZERO, Vec3::Y)
}

fn to_bevy_direction(east_up_north: [f32; 3]) -> Vec3 {
    Vec3::new(east_up_north[0], east_up_north[1], -east_up_north[2]).normalize()
}

fn sky_weather_transmission(environment: &SceneEnvironment) -> f32 {
    let intensity = f32::from(environment.weather.intensity_bps) / 10_000.0;
    match environment.weather.precipitation {
        Precipitation::Clear => 1.0,
        Precipitation::Rain => 0.12 * (1.0 - intensity * 0.7),
        Precipitation::Snow => 0.2 * (1.0 - intensity * 0.6),
    }
}

pub(super) fn scene_exposure_ev100(sun_altitude_degrees: f32, lunar_illumination: f32) -> f32 {
    if sun_altitude_degrees >= 12.0 {
        15.0
    } else if sun_altitude_degrees >= 0.0 {
        11.0 + sun_altitude_degrees / 3.0
    } else if sun_altitude_degrees >= -6.0 {
        8.0 + (sun_altitude_degrees + 6.0) * 0.5
    } else if sun_altitude_degrees >= -12.0 {
        4.0 + (sun_altitude_degrees + 12.0) * (4.0 / 6.0)
    } else {
        2.5 + lunar_illumination * 1.5
    }
}

fn star_visibility(sun_altitude_degrees: f32) -> f32 {
    1.0 - smoothstep(-15.0, -7.0, sun_altitude_degrees)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn equatorial_to_world(
    absolute_minute: u64,
    latitude_microdegrees: i32,
    longitude_microdegrees: i32,
) -> Mat4 {
    let latitude = (latitude_microdegrees as f32 / 1_000_000.0).to_radians();
    let longitude = (longitude_microdegrees as f32 / 1_000_000.0).to_radians();
    let day = absolute_minute as f32 / MINUTES_PER_DAY as f32;
    let sidereal = (4.383_4 + day * core::f32::consts::TAU * 1.002_737_9 + longitude)
        .rem_euclid(core::f32::consts::TAU);
    let (sin_latitude, cos_latitude) = latitude.sin_cos();
    let (sin_sidereal, cos_sidereal) = sidereal.sin_cos();
    Mat4::from_mat3(Mat3::from_cols(
        Vec3::new(
            -sin_sidereal,
            cos_latitude * cos_sidereal,
            sin_latitude * cos_sidereal,
        ),
        Vec3::new(0.0, sin_latitude, -cos_latitude),
        Vec3::new(
            cos_sidereal,
            cos_latitude * sin_sidereal,
            sin_latitude * sin_sidereal,
        ),
    ))
}

fn star_mesh() -> Mesh {
    let stars = parse_star_catalog(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/data/hipparcos-bright-stars.csv"
    )));
    let mut positions = Vec::with_capacity(stars.len() * 6);
    let mut directions = Vec::with_capacity(stars.len() * 6);
    let mut colors = Vec::with_capacity(stars.len() * 6);
    let mut corners = Vec::with_capacity(stars.len() * 6);
    let quad = [
        [-1.0, -1.0],
        [1.0, -1.0],
        [1.0, 1.0],
        [-1.0, -1.0],
        [1.0, 1.0],
        [-1.0, 1.0],
    ];
    for star in stars {
        for corner in quad {
            positions.push([0.0, 0.0, 0.0]);
            directions.push(star.direction);
            colors.push([star.color[0], star.color[1], star.color[2], star.magnitude]);
            corners.push(corner);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(ATTRIBUTE_STAR_DIRECTION, directions)
    .with_inserted_attribute(ATTRIBUTE_STAR_COLOR_MAGNITUDE, colors)
    .with_inserted_attribute(ATTRIBUTE_STAR_CORNER, corners)
}

#[derive(Clone, Copy)]
struct CatalogStar {
    direction: [f32; 3],
    color: [f32; 3],
    magnitude: f32,
}

fn parse_star_catalog(csv: &str) -> Vec<CatalogStar> {
    csv.lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split(',');
            let _hip = fields.next()?;
            let right_ascension = fields.next()?.parse::<f32>().ok()?.to_radians();
            let declination = fields.next()?.parse::<f32>().ok()?.to_radians();
            let magnitude = fields.next()?.parse::<f32>().ok()?;
            let color_index = fields.next()?.parse::<f32>().ok().unwrap_or(0.65);
            let (sin_ra, cos_ra) = right_ascension.sin_cos();
            let (sin_dec, cos_dec) = declination.sin_cos();
            Some(CatalogStar {
                direction: [cos_dec * cos_ra, sin_dec, cos_dec * sin_ra],
                color: bv_to_linear_rgb(color_index),
                magnitude,
            })
        })
        .collect()
}

fn bv_to_linear_rgb(bv: f32) -> [f32; 3] {
    let t = ((bv + 0.4) / 2.4).clamp(0.0, 1.0);
    // Stable, deliberately restrained approximation: hot stars are blue-white
    // and cool stars warm-white without turning the sky into confetti.
    let srgb = Vec3::new(
        0.72 + t * 0.28,
        0.82 + (1.0 - (t - 0.45).abs() * 1.1).clamp(0.0, 1.0) * 0.18,
        1.0 - t * 0.48,
    );
    [srgb.x.powf(2.2), srgb.y.powf(2.2), srgb.z.powf(2.2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_bounded_and_contains_thousands_of_visible_stars() {
        let stars = parse_star_catalog(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/data/hipparcos-bright-stars.csv"
        )));
        assert!((5_000..=12_000).contains(&stars.len()), "{}", stars.len());
        assert!(stars.iter().all(|star| star.magnitude <= 6.5));
    }

    #[test]
    fn star_visibility_follows_astronomical_twilight() {
        assert_eq!(star_visibility(0.0), 0.0);
        assert_eq!(star_visibility(-16.0), 1.0);
        assert!(star_visibility(-10.0) > 0.0 && star_visibility(-10.0) < 1.0);
    }

    #[test]
    fn exposure_adapts_between_day_and_night() {
        assert_eq!(scene_exposure_ev100(30.0, 0.0), 15.0);
        assert!(scene_exposure_ev100(-20.0, 1.0) > scene_exposure_ev100(-20.0, 0.0));
        assert!(scene_exposure_ev100(-20.0, 1.0) < 5.0);
    }
}
