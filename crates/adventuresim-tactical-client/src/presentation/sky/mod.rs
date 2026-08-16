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
    /// NASA LRO equirectangular colour mosaic, sampled as sRGB base reflectance.
    #[texture(2)]
    #[sampler(3)]
    albedo: Handle<Image>,
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
        // The Moon is a closed sphere. Opaque depth prevents stars from
        // bleeding through its earthlit or unlit hemisphere.
        AlphaMode::Opaque
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
    asset_server: Res<AssetServer>,
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
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(96, 48))),
        MeshMaterial3d(moon_materials.add(TacticalMoonMaterial {
            albedo: asset_server.load("textures/moon/lroc_color_2k.jpg"),
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

#[derive(Debug, Clone)]
pub(in crate::presentation) struct CelestialLightingSnapshot {
    pub(in crate::presentation) scene: Entity,
    pub(in crate::presentation) sun_direction: Vec3,
    pub(in crate::presentation) moon_direction: Vec3,
    pub(in crate::presentation) sun_altitude_degrees: f32,
    pub(in crate::presentation) moon_altitude_degrees: f32,
    pub(in crate::presentation) lunar_illumination: f32,
    lunar_phase: f32,
    pub(in crate::presentation) weather_transmission: f32,
    equatorial_to_world: Mat4,
    exposure_ev100: f32,
    pub(in crate::presentation) ambient_color: Vec3,
    pub(in crate::presentation) ambient_brightness: f32,
    pub(in crate::presentation) ibl_ambient_color: Vec3,
    pub(in crate::presentation) ibl_ambient_brightness: f32,
    pub(in crate::presentation) material_light_factor: f32,
    pub(in crate::presentation) material_ambient_response: f32,
}

#[derive(Resource, Debug, Clone, Default)]
pub(in crate::presentation) struct PresentedCelestialLighting {
    pub(in crate::presentation) snapshot: Option<CelestialLightingSnapshot>,
}

impl CelestialLightingSnapshot {
    fn from_environment(scene: Entity, environment: &SceneEnvironment) -> Self {
        let celestial = celestial_directions(
            environment.absolute_minute,
            environment.latitude_microdegrees,
            environment.longitude_microdegrees,
        );
        let sun_direction = to_bevy_direction(celestial.sun);
        let moon_direction = to_bevy_direction(celestial.moon);
        let sun_altitude_degrees = sun_direction.y.asin().to_degrees();
        let moon_altitude_degrees = moon_direction.y.asin().to_degrees();
        let (ambient_color, ambient_brightness) = scene_ambient_light(
            sun_altitude_degrees,
            moon_altitude_degrees,
            celestial.lunar_illumination,
        );
        let (ibl_ambient_color, ibl_ambient_brightness) = scene_ibl_visibility_floor(
            sun_altitude_degrees,
            moon_altitude_degrees,
            celestial.lunar_illumination,
        );
        Self {
            scene,
            sun_direction,
            moon_direction,
            sun_altitude_degrees,
            moon_altitude_degrees,
            lunar_illumination: celestial.lunar_illumination,
            lunar_phase: celestial.lunar_phase,
            weather_transmission: sky_weather_transmission(environment),
            equatorial_to_world: equatorial_to_world(
                environment.absolute_minute,
                environment.latitude_microdegrees,
                environment.longitude_microdegrees,
            ),
            exposure_ev100: scene_exposure_ev100(
                sun_altitude_degrees,
                moon_altitude_degrees,
                celestial.lunar_illumination,
            ),
            ambient_color,
            ambient_brightness,
            ibl_ambient_color,
            ibl_ambient_brightness,
            material_light_factor: scene_night_factor(
                sun_altitude_degrees,
                moon_altitude_degrees,
                celestial.lunar_illumination,
            ),
            material_ambient_response: scene_ambient_response(
                sun_altitude_degrees,
                moon_altitude_degrees,
                celestial.lunar_illumination,
            ),
        }
    }
}

pub(in crate::presentation) fn update_presented_celestial_lighting(
    active: Res<ActiveTacticalScene>,
    environments: Query<Ref<SceneEnvironment>>,
    mut presented: ResMut<PresentedCelestialLighting>,
) {
    let environment = active.entity.and_then(|entity| {
        environments
            .get(entity)
            .ok()
            .map(|environment| (entity, environment))
    });
    if !active.is_changed()
        && environment
            .as_ref()
            .is_some_and(|(_, environment)| !environment.is_changed())
    {
        return;
    }
    presented.snapshot = environment.map(|(entity, environment)| {
        CelestialLightingSnapshot::from_environment(entity, &environment)
    });
}

pub(in crate::presentation) fn apply_presented_celestial_lighting(
    active: Res<ActiveTacticalScene>,
    celestial: Res<PresentedCelestialLighting>,
    settings: Res<TacticalGraphicsSettings>,
    sunlight: Single<(&mut DirectionalLight, &mut Transform), With<TacticalSunlight>>,
    moonlight: Single<
        (&mut DirectionalLight, &mut Transform),
        (With<TacticalMoonlight>, Without<TacticalSunlight>),
    >,
    camera: Single<&mut Exposure, With<Camera3d>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut moon: Query<
        (&MeshMaterial3d<TacticalMoonMaterial>, &mut Visibility),
        (With<TacticalMoon>, Without<TacticalStars>),
    >,
    mut stars: Query<
        (&MeshMaterial3d<TacticalStarMaterial>, &mut Visibility),
        (With<TacticalStars>, Without<TacticalMoon>),
    >,
    mut moon_materials: ResMut<Assets<TacticalMoonMaterial>>,
    mut star_materials: ResMut<Assets<TacticalStarMaterial>>,
    environments: Query<&SceneEnvironment>,
) {
    if !celestial.is_changed() && !settings.is_changed() {
        return;
    }
    let Some(celestial) = celestial.snapshot.as_ref() else {
        let (mut sun, _) = sunlight.into_inner();
        sun.illuminance = 0.0;
        sun.shadow_maps_enabled = false;
        let (mut moon_light, _) = moonlight.into_inner();
        moon_light.illuminance = 0.0;
        moon_light.shadow_maps_enabled = false;
        camera.into_inner().ev100 = Exposure::SUNLIGHT.ev100;
        ambient.brightness = 0.0;
        if let Ok((handle, mut visibility)) = moon.single_mut() {
            *visibility = Visibility::Hidden;
            if let Some(mut material) = moon_materials.get_mut(&handle.0) {
                material.light = Vec4::ZERO;
            }
        }
        if let Ok((handle, mut visibility)) = stars.single_mut() {
            *visibility = Visibility::Hidden;
            if let Some(mut material) = star_materials.get_mut(&handle.0) {
                material.settings.x = 0.0;
            }
        }
        return;
    };
    let Some(environment) = active
        .entity
        .filter(|entity| *entity == celestial.scene)
        .and_then(|entity| environments.get(entity).ok())
    else {
        return;
    };

    let (mut sun, mut sun_transform) = sunlight.into_inner();
    sun.illuminance = selected_solar_illuminance(
        settings.atmosphere_enabled,
        environment,
        celestial.sun_altitude_degrees,
    );
    sun.shadow_maps_enabled = settings.shadows_enabled && celestial.sun_altitude_degrees > 0.0;
    *sun_transform = light_transform(celestial.sun_direction);

    let (mut moon_light, mut moon_transform) = moonlight.into_inner();
    moon_light.illuminance = 0.25
        * celestial.lunar_illumination
        * smoothstep(-2.0, 4.0, celestial.moon_altitude_degrees)
        * celestial.weather_transmission;
    moon_light.shadow_maps_enabled = settings.shadows_enabled
        && celestial.sun_altitude_degrees <= -2.0
        && celestial.moon_altitude_degrees > 0.0
        && celestial.lunar_illumination > 0.15;
    *moon_transform = light_transform(celestial.moon_direction);

    camera.into_inner().ev100 = celestial.exposure_ev100;
    ambient.color = Color::srgb(
        celestial.ambient_color.x,
        celestial.ambient_color.y,
        celestial.ambient_color.z,
    );
    ambient.brightness = celestial.ambient_brightness;

    if let Ok((handle, mut visibility)) = moon.single_mut()
        && let Some(mut material) = moon_materials.get_mut(&handle.0)
    {
        *visibility = Visibility::Inherited;
        material.light = celestial
            .sun_direction
            .extend(celestial.weather_transmission);
        material.appearance.z = celestial.lunar_phase;
    }
    if let Ok((handle, mut visibility)) = stars.single_mut()
        && let Some(mut material) = star_materials.get_mut(&handle.0)
    {
        *visibility = Visibility::Inherited;
        material.equatorial_to_world = celestial.equatorial_to_world;
        material.settings.x =
            star_visibility(celestial.sun_altitude_degrees) * celestial.weather_transmission;
    }
}

const ENVIRONMENT_MAP_ALLOCATION_GRACE_FRAMES: u8 = 4;

#[derive(Resource, Debug, Default)]
pub(crate) struct AtmosphereIblAmbientHandoff {
    allocated_frames: u8,
    pub(crate) active: bool,
}

pub(super) fn update_global_ambient_policy(
    settings: Res<TacticalGraphicsSettings>,
    celestial: Res<PresentedCelestialLighting>,
    camera_environment: Single<Option<&EnvironmentMapLight>, With<Camera3d>>,
    mut handoff: ResMut<AtmosphereIblAmbientHandoff>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let celestial_changed = celestial.is_changed();
    let Some(celestial) = celestial.snapshot.as_ref() else {
        handoff.allocated_frames = 0;
        handoff.active = false;
        ambient.brightness = 0.0;
        return;
    };
    // EnvironmentMapLight is inserted with placeholder images before render-world
    // filtering is observable from the main world. Hold the full fallback for a
    // short bounded grace after allocation; deterministic captures additionally
    // require consecutive stable readbacks before accepting evidence.
    let allocated = settings.atmosphere_enabled
        && settings.environment_light_enabled
        && camera_environment.is_some();
    if allocated && handoff.active && !settings.is_changed() && !celestial_changed {
        return;
    }
    handoff.allocated_frames = if allocated {
        handoff.allocated_frames.saturating_add(1)
    } else {
        0
    };
    handoff.active = handoff.allocated_frames >= ENVIRONMENT_MAP_ALLOCATION_GRACE_FRAMES;
    let (color, brightness) = if handoff.active {
        (
            celestial.ibl_ambient_color,
            celestial.ibl_ambient_brightness,
        )
    } else {
        (celestial.ambient_color, celestial.ambient_brightness)
    };
    ambient.color = Color::srgb(color.x, color.y, color.z);
    ambient.brightness = brightness;
}

#[cfg(test)]
mod ambient_handoff_tests {
    use super::*;

    #[test]
    fn missing_active_scene_clears_all_celestial_outputs() {
        let mut app = App::new();
        app.init_resource::<Assets<TacticalMoonMaterial>>()
            .init_resource::<Assets<TacticalStarMaterial>>()
            .init_resource::<ActiveTacticalScene>()
            .init_resource::<PresentedCelestialLighting>()
            .insert_resource(TacticalGraphicsSettings {
                shadows_enabled: true,
                atmosphere_enabled: true,
                celestial_enabled: true,
                environment_light_enabled: true,
                environment_map_size: 64,
                bloom_enabled: true,
                max_vista_lods: 3,
            })
            .insert_resource(GlobalAmbientLight {
                brightness: 42.0,
                ..default()
            })
            .add_systems(Update, apply_presented_celestial_lighting);
        app.world_mut().spawn((
            TacticalSunlight,
            DirectionalLight {
                illuminance: 10.0,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::default(),
        ));
        app.world_mut().spawn((
            TacticalMoonlight,
            DirectionalLight {
                illuminance: 5.0,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::default(),
        ));
        app.world_mut()
            .spawn((Camera3d::default(), Exposure { ev100: 2.0 }));
        let moon = app
            .world_mut()
            .resource_mut::<Assets<TacticalMoonMaterial>>()
            .add(TacticalMoonMaterial {
                albedo: default(),
                light: Vec4::ONE,
                appearance: Vec4::new(0.025, 8.0, 0.5, 0.0),
            });
        app.world_mut().spawn((
            TacticalMoon,
            MeshMaterial3d(moon.clone()),
            Visibility::Visible,
        ));
        let stars = app
            .world_mut()
            .resource_mut::<Assets<TacticalStarMaterial>>()
            .add(TacticalStarMaterial {
                equatorial_to_world: Mat4::IDENTITY,
                settings: Vec4::ONE,
            });
        app.world_mut().spawn((
            TacticalStars,
            MeshMaterial3d(stars.clone()),
            Visibility::Visible,
        ));

        app.update();

        let world = app.world_mut();
        assert_eq!(world.resource::<GlobalAmbientLight>().brightness, 0.0);
        assert_eq!(
            world
                .resource::<Assets<TacticalMoonMaterial>>()
                .get(&moon)
                .expect("moon material")
                .light,
            Vec4::ZERO
        );
        assert_eq!(
            world
                .resource::<Assets<TacticalMoonMaterial>>()
                .get(&moon)
                .expect("moon material")
                .appearance
                .y,
            8.0
        );
        assert_eq!(
            world
                .resource::<Assets<TacticalStarMaterial>>()
                .get(&stars)
                .expect("star material")
                .settings
                .x,
            0.0
        );
        assert_eq!(
            world
                .query_filtered::<&Visibility, With<TacticalMoon>>()
                .single(world)
                .expect("one moon"),
            &Visibility::Hidden
        );
        assert_eq!(
            world
                .query_filtered::<&Visibility, With<TacticalStars>>()
                .single(world)
                .expect("one star field"),
            &Visibility::Hidden
        );
        assert_eq!(
            world
                .query_filtered::<&Exposure, With<Camera3d>>()
                .single(world)
                .expect("one camera")
                .ev100,
            Exposure::SUNLIGHT.ev100
        );
        for light in world.query::<&DirectionalLight>().iter(world) {
            assert_eq!(light.illuminance, 0.0);
            assert!(!light.shadow_maps_enabled);
        }
    }

    #[test]
    fn allocation_grace_is_bounded_and_resets() {
        let mut handoff = AtmosphereIblAmbientHandoff::default();
        for frame in 1..ENVIRONMENT_MAP_ALLOCATION_GRACE_FRAMES {
            handoff.allocated_frames = handoff.allocated_frames.saturating_add(1);
            handoff.active = handoff.allocated_frames >= ENVIRONMENT_MAP_ALLOCATION_GRACE_FRAMES;
            assert!(
                !handoff.active,
                "frame {frame} must retain fallback ambient"
            );
        }
        handoff.allocated_frames = handoff.allocated_frames.saturating_add(1);
        handoff.active = handoff.allocated_frames >= ENVIRONMENT_MAP_ALLOCATION_GRACE_FRAMES;
        assert!(handoff.active);
        handoff.allocated_frames = 0;
        handoff.active = false;
        assert!(!handoff.active);
    }
}

fn selected_solar_illuminance(
    atmosphere_enabled: bool,
    environment: &SceneEnvironment,
    sun_altitude_degrees: f32,
) -> f32 {
    if atmosphere_enabled {
        scene_atmosphere_solar_illuminance(environment)
    } else {
        // The non-atmospheric fallback has no planetary transmittance or
        // horizon occlusion, so it must retain the explicit altitude clamp.
        scene_sunlight_illuminance(environment, sun_altitude_degrees)
    }
}

pub(in crate::presentation) fn keep_celestial_visuals_centered(
    camera: Single<&GlobalTransform, With<Camera3d>>,
    celestial: Res<PresentedCelestialLighting>,
    mut moon: Query<&mut Transform, With<TacticalMoon>>,
) {
    let Some(celestial) = celestial.snapshot.as_ref() else {
        return;
    };
    let Ok(mut moon_transform) = moon.single_mut() else {
        return;
    };
    moon_transform.translation =
        camera.translation() + celestial.moon_direction * MOON_DISTANCE_METRES;
    moon_transform.rotation = moon_near_side_rotation(celestial.moon_direction);
}

/// Keeps the map's zero-longitude near side pointed at the observer while
/// retaining a stable celestial north. This is the bounded synchronous-rotation
/// model appropriate at the Moon's sub-pixel libration scale here.
fn moon_near_side_rotation(direction_to_moon: Vec3) -> Quat {
    let local_x_world = -direction_to_moon.normalize();
    let mut local_z_world = Vec3::Y - local_x_world * Vec3::Y.dot(local_x_world);
    if local_z_world.length_squared() < 1.0e-4 {
        local_z_world = Vec3::Z - local_x_world * Vec3::Z.dot(local_x_world);
    }
    local_z_world = local_z_world.normalize();
    let local_y_world = local_z_world.cross(local_x_world).normalize();
    Quat::from_mat3(&Mat3::from_cols(
        local_x_world,
        local_y_world,
        local_z_world,
    ))
}

fn light_transform(direction_to_light: Vec3) -> Transform {
    Transform::from_translation(direction_to_light * 1_000.0).looking_at(Vec3::ZERO, Vec3::Y)
}

pub(super) fn to_bevy_direction(east_up_north: [f32; 3]) -> Vec3 {
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

pub(super) fn scene_exposure_ev100(
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
) -> f32 {
    if sun_altitude_degrees >= 12.0 {
        15.0
    } else if sun_altitude_degrees >= 0.0 {
        11.0 + sun_altitude_degrees / 3.0
    } else if sun_altitude_degrees >= -6.0 {
        8.0 + (sun_altitude_degrees + 6.0) * 0.5
    } else if sun_altitude_degrees >= -12.0 {
        4.0 + (sun_altitude_degrees + 12.0) * (4.0 / 6.0)
    } else if sun_altitude_degrees >= -18.0 {
        let night_target = night_exposure_ev100(moon_altitude_degrees, lunar_illumination);
        let transition = smoothstep(12.0, 18.0, -sun_altitude_degrees);
        4.0 + (night_target - 4.0) * transition
    } else {
        // Preserve genuinely dark, obstacle-obscuring moonless nights. A risen
        // moon lowers EV100 as the eye adapts to reveal the 0.25-lux surface
        // response; a below-horizon moon does neither. The former positive
        // sign darkened the camera as lunar illumination increased.
        night_exposure_ev100(moon_altitude_degrees, lunar_illumination)
    }
}

fn night_exposure_ev100(moon_altitude_degrees: f32, lunar_illumination: f32) -> f32 {
    -0.5 - 0.75 * lunar_illumination * smoothstep(-2.0, 8.0, moon_altitude_degrees)
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
        assert_eq!(scene_exposure_ev100(30.0, -20.0, 0.0), 15.0);
        let moonless = scene_exposure_ev100(-20.0, 30.0, 0.0);
        let moonlit = scene_exposure_ev100(-20.0, 30.0, 1.0);
        let moon_below_horizon = scene_exposure_ev100(-20.0, -20.0, 1.0);
        assert!(moonlit < moonless);
        assert_eq!(moon_below_horizon, moonless);
        assert!((-1.3..=-1.2).contains(&moonlit));
        for knot in [-12.0, -18.0] {
            let below = scene_exposure_ev100(knot - 0.001, 30.0, 0.8);
            let above = scene_exposure_ev100(knot + 0.001, 30.0, 0.8);
            assert!(
                (below - above).abs() < 0.01,
                "exposure discontinuity at {knot}"
            );
        }
    }

    #[test]
    fn sun_disc_preserves_physical_angular_diameter() {
        assert!((SunDisk::EARTH.angular_size.to_degrees() - 0.533).abs() < 0.001);
        assert_eq!(SunDisk::EARTH.intensity, 1.0);
    }

    #[test]
    fn atmosphere_and_fallback_select_safe_solar_sources() {
        let environment = SceneEnvironment {
            scene_digest: "sky-test".into(),
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            latitude_microdegrees: 0,
            longitude_microdegrees: 0,
            absolute_minute: 0,
            absolute_elevation_metres: 0,
            weather: WeatherSnapshot {
                rules_version: WEATHER_RULES_VERSION,
                interval_start_minute: 0,
                cell_latitude: 0,
                cell_longitude: 0,
                temperature_deci_c: 100,
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
        };
        assert_eq!(selected_solar_illuminance(false, &environment, -6.0), 0.0);
        assert_eq!(
            selected_solar_illuminance(true, &environment, -6.0),
            lux::RAW_SUNLIGHT
        );
        assert_eq!(
            selected_solar_illuminance(false, &environment, 8.0),
            selected_solar_illuminance(true, &environment, 8.0)
        );
    }

    #[test]
    fn moon_rotation_keeps_zero_longitude_facing_the_observer() {
        for direction in [
            Vec3::new(0.7, 0.4, -0.2).normalize(),
            Vec3::new(-0.3, 0.9, 0.1).normalize(),
            Vec3::Y,
        ] {
            let rotation = moon_near_side_rotation(direction);
            assert!((rotation * Vec3::X + direction).length() < 1.0e-5);
            assert!((rotation * Vec3::Z).dot(direction).abs() < 1.0e-5);
        }
    }

    #[test]
    fn moon_disc_preserves_physical_angular_diameter() {
        let radius = MOON_DISTANCE_METRES * MOON_ANGULAR_RADIUS_RADIANS.tan();
        let reconstructed = (radius / MOON_DISTANCE_METRES).atan();
        assert!((reconstructed - MOON_ANGULAR_RADIUS_RADIANS).abs() < f32::EPSILON);
        assert!((MOON_ANGULAR_RADIUS_RADIANS.to_degrees() * 2.0 - 0.5).abs() < 1.0e-5);
    }
}
