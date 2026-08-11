//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::{
    asset::RenderAssetUsages,
    camera::Exposure,
    core_pipeline::tonemapping::Tonemapping,
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, NotShadowCaster, atmosphere::ScatteringMedium,
        light_consts::lux,
    },
    mesh::{Indices, PrimitiveTopology},
    pbr::{AtmosphereSettings, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
};

#[derive(Debug, Clone, Copy)]
pub struct TacticalPresentationPlugin {
    pub shadows_enabled: bool,
    pub atmosphere_enabled: bool,
    pub environment_light_enabled: bool,
    pub environment_map_size: u32,
    pub bloom_enabled: bool,
    pub ssao_enabled: bool,
    pub max_vista_lods: usize,
}

impl Default for TacticalPresentationPlugin {
    fn default() -> Self {
        Self {
            shadows_enabled: true,
            atmosphere_enabled: true,
            environment_light_enabled: true,
            environment_map_size: 64,
            bloom_enabled: true,
            ssao_enabled: true,
            max_vista_lods: 3,
        }
    }
}

impl Plugin for TacticalPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TacticalGraphicsSettings {
            shadows_enabled: self.shadows_enabled,
            atmosphere_enabled: self.atmosphere_enabled,
            environment_light_enabled: self.environment_light_enabled,
            environment_map_size: self.environment_map_size,
            bloom_enabled: self.bloom_enabled,
            ssao_enabled: self.ssao_enabled,
            max_vista_lods: self.max_vista_lods,
        })
        .add_systems(Startup, setup_tactical_presentation)
        .add_systems(Update, advance_weather_particles)
        .add_observer(on_game_scene_added)
        .add_observer(on_scene_environment_added)
        .add_observer(on_scene_obstacle_added)
        .add_observer(on_scene_vista_bundle);
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct TacticalGraphicsSettings {
    shadows_enabled: bool,
    atmosphere_enabled: bool,
    environment_light_enabled: bool,
    environment_map_size: u32,
    bloom_enabled: bool,
    ssao_enabled: bool,
    max_vista_lods: usize,
}

#[derive(Component)]
struct ScenePresentationOf(Entity);

#[derive(Component)]
pub(crate) struct TacticalSunlight;

#[derive(Component)]
pub(crate) struct WeatherParticle {
    velocity: Vec3,
    ceiling: f32,
}

#[derive(Component)]
#[allow(dead_code)]
pub(crate) struct VistaTerrain(pub(crate) u8);

const VISTA_CHUNK_CELLS: usize = 8;

fn setup_tactical_presentation(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    settings: Res<TacticalGraphicsSettings>,
) {
    commands.spawn((
        Name::new("Tactical sunlight"),
        TacticalSunlight,
        Transform::from_xyz(200.0, 1000.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
        DirectionalLight {
            shadow_maps_enabled: settings.shadows_enabled,
            illuminance: lux::DIRECT_SUNLIGHT,
            ..default()
        },
    ));

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

fn on_game_scene_added(
    event: On<Add, SceneId>,
    mut commands: Commands,
    query: Query<(&SceneId, &SceneTerrain, Option<&SceneEnvironment>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let (id, terrain, environment) = query.get(event.entity)?;
    info!(entity = ?event.entity, "Spawning a scene {id:?}");

    let floor_color = environment.map_or_else(
        || match id.0.as_str() {
            "hills" => Color::srgb_u8(96, 108, 56),
            "desert" => Color::srgb_u8(221, 161, 94),
            id => {
                warn!("Unknown legacy scene: {id}");
                Color::BLACK
            }
        },
        scene_ground_color,
    );

    commands.spawn((
        Name::new(format!("{} terrain mesh", id.0)),
        ScenePresentationOf(event.entity),
        Mesh3d(meshes.add(terrain.mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: floor_color,
            perceptual_roughness: 0.8,
            metallic: 0.0,
            ..default()
        })),
    ));
    Ok(())
}

fn on_scene_environment_added(
    event: On<Add, SceneEnvironment>,
    environments: Query<&SceneEnvironment>,
    presentations: Query<(&ScenePresentationOf, &MeshMaterial3d<StandardMaterial>)>,
    mut sunlight: Single<&mut DirectionalLight, With<TacticalSunlight>>,
    mut fog: Single<&mut DistanceFog, With<Camera3d>>,
    particles: Query<Entity, With<WeatherParticle>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let color = scene_ground_color(environments.get(event.entity)?);
    for (source, material) in &presentations {
        if source.0 == event.entity
            && let Some(mut material) = materials.get_mut(&material.0)
        {
            material.base_color = color;
        }
    }
    let environment = environments.get(event.entity)?;
    sunlight.illuminance = scene_sunlight_illuminance(environment);
    **fog = scene_distance_fog(environment);
    for entity in &particles {
        commands.entity(entity).despawn();
    }
    spawn_weather_particles(&mut commands, &mut meshes, &mut materials, environment);
    Ok(())
}

fn on_scene_obstacle_added(
    event: On<Add, SceneObstacle>,
    mut commands: Commands,
    obstacles: Query<&SceneObstacle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    match *obstacles.get(event.entity)? {
        SceneObstacle::Tree => {
            let trunk_mesh = meshes.add(Cylinder::new(
                TREE_TRUNK_RADIUS_METRES,
                TREE_TRUNK_HEIGHT_METRES,
            ));
            let trunk_material = materials.add(StandardMaterial {
                base_color: Color::srgb_u8(91, 58, 34),
                perceptual_roughness: 0.95,
                ..default()
            });
            let crown_mesh = meshes.add(Sphere::new(1.55));
            let crown_material = materials.add(StandardMaterial {
                base_color: Color::srgb_u8(45, 82, 38),
                perceptual_roughness: 0.9,
                ..default()
            });
            commands.entity(event.entity).insert((
                Name::new("Presented tactical tree"),
                Mesh3d(trunk_mesh),
                MeshMaterial3d(trunk_material),
                children![(
                    Name::new("Tree crown"),
                    Mesh3d(crown_mesh),
                    MeshMaterial3d(crown_material),
                    Transform::from_xyz(0.0, TREE_TRUNK_HEIGHT_METRES * 0.42, 0.0),
                )],
            ));
        }
        SceneObstacle::Rock => {
            commands.entity(event.entity).insert((
                Name::new("Presented tactical rock"),
                Mesh3d(meshes.add(Sphere::new(ROCK_RADIUS_METRES))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb_u8(104, 101, 94),
                    perceptual_roughness: 1.0,
                    ..default()
                })),
            ));
        }
    }
    Ok(())
}

fn on_scene_vista_bundle(
    bundle: On<SceneVistaBundle>,
    mut commands: Commands,
    existing: Query<Entity, With<VistaTerrain>>,
    settings: Res<TacticalGraphicsSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let mut inner_half_extent = 55.0;
    for lod in bundle.lods.iter().take(settings.max_vista_lods) {
        let meshes_for_lod = vista_lod_meshes(lod, inner_half_extent);
        if meshes_for_lod.is_empty() {
            warn!(level = lod.level, "Rejected malformed tactical vista LOD");
            continue;
        }
        let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
        let color = vista_lod_color(lod);
        let material = materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 1.0,
            ..default()
        });
        for (chunk, mesh) in meshes_for_lod.into_iter().enumerate() {
            commands.spawn((
                Name::new(format!("Tactical vista LOD {} chunk {chunk}", lod.level)),
                VistaTerrain(lod.level),
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(
                    lod.origin_east_metres as f32,
                    -0.06 * (f32::from(lod.level) + 1.0),
                    lod.origin_north_metres as f32,
                ),
            ));
        }
        inner_half_extent = half_extent;
    }
}

fn vista_lod_meshes(lod: &VistaLod, inner_half_extent: f32) -> Vec<Mesh> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    if width < 2
        || depth < 2
        || width
            .checked_mul(depth)
            .is_none_or(|samples| lod.heights_metres.len() != samples)
        || !lod.spacing_metres.is_finite()
        || lod.spacing_metres <= 0.0
    {
        return Vec::new();
    }
    let center_x = (width - 1) as f32 * 0.5;
    let center_z = (depth - 1) as f32 * 0.5;
    let mut meshes = Vec::new();
    for chunk_z in (0..depth - 1).step_by(VISTA_CHUNK_CELLS) {
        for chunk_x in (0..width - 1).step_by(VISTA_CHUNK_CELLS) {
            let mut positions = Vec::new();
            let mut indices = Vec::new();
            for z in chunk_z..(chunk_z + VISTA_CHUNK_CELLS).min(depth - 1) {
                for x in chunk_x..(chunk_x + VISTA_CHUNK_CELLS).min(width - 1) {
                    let cell_x = (x as f32 + 0.5 - center_x) * lod.spacing_metres;
                    let cell_z = (z as f32 + 0.5 - center_z) * lod.spacing_metres;
                    // Exclude cells fully covered by the playable mesh or the
                    // preceding finer ring. Testing the outer cell edge keeps
                    // one boundary cell without filling the inner hole.
                    if cell_x.abs().max(cell_z.abs()) + lod.spacing_metres * 0.5
                        <= inner_half_extent
                    {
                        continue;
                    }
                    let vertex = |vx: usize, vz: usize| {
                        [
                            (vx as f32 - center_x) * lod.spacing_metres,
                            lod.heights_metres[vz * width + vx],
                            (vz as f32 - center_z) * lod.spacing_metres,
                        ]
                    };
                    let base = positions.len() as u32;
                    positions.extend_from_slice(&[
                        vertex(x, z),
                        vertex(x + 1, z),
                        vertex(x + 1, z + 1),
                        vertex(x, z + 1),
                    ]);
                    indices.extend_from_slice(&[
                        base,
                        base + 2,
                        base + 1,
                        base,
                        base + 3,
                        base + 2,
                    ]);
                }
            }
            if positions.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_indices(Indices::U32(indices));
            meshes.push(mesh.with_computed_area_weighted_normals());
        }
    }
    meshes
}

fn vista_lod_color(lod: &VistaLod) -> Color {
    let count = lod.environment.len().max(1) as f32;
    let (canopy, wetland, cultivation, water) =
        lod.environment
            .iter()
            .fold((0.0, 0.0, 0.0, 0.0), |sum, sample| {
                (
                    sum.0 + f32::from(sample.canopy_bps),
                    sum.1 + f32::from(sample.wetland_bps),
                    sum.2 + f32::from(sample.cultivation_bps),
                    sum.3 + f32::from(sample.water_bps),
                )
            });
    let environment = SceneEnvironment {
        scene_digest: String::new(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
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
        canopy_bps: (canopy / count) as u16,
        wetland_bps: (wetland / count) as u16,
        cultivation_bps: (cultivation / count) as u16,
        water_bps: (water / count) as u16,
        hilly_bps: 0,
    };
    scene_ground_color(&environment)
}

fn scene_ground_color(environment: &SceneEnvironment) -> Color {
    let mut rgb = if environment.water_bps >= 5_000 {
        [52.0, 83.0, 98.0]
    } else if environment.wetland_bps >= 4_000 {
        [73.0, 86.0, 58.0]
    } else if environment.canopy_bps >= 5_000 {
        [55.0, 82.0, 43.0]
    } else if environment.cultivation_bps >= 4_000 {
        [126.0, 116.0, 66.0]
    } else {
        [96.0, 108.0, 56.0]
    };
    let snow = f32::from(environment.weather.snow_cover_bps) / 10_000.0;
    let wet = f32::from(environment.weather.ground_moisture_bps) / 10_000.0;
    for channel in &mut rgb {
        *channel *= 1.0 - wet * 0.22;
        *channel = *channel * (1.0 - snow) + 220.0 * snow;
    }
    Color::srgb(rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0)
}

fn scene_sunlight_illuminance(environment: &SceneEnvironment) -> f32 {
    let intensity = f32::from(environment.weather.intensity_bps) / 10_000.0;
    let transmission = match environment.weather.precipitation {
        Precipitation::Clear => 1.0,
        Precipitation::Rain => 0.62 - intensity * 0.27,
        Precipitation::Snow => 0.72 - intensity * 0.22,
    };
    lux::DIRECT_SUNLIGHT * transmission.clamp(0.25, 1.0)
}

fn scene_distance_fog(environment: &SceneEnvironment) -> DistanceFog {
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

fn spawn_weather_particles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    environment: &SceneEnvironment,
) {
    let (mesh, material, fall_speed) = match environment.weather.precipitation {
        Precipitation::Clear => return,
        Precipitation::Rain => (
            meshes.add(Cuboid::new(0.06, 1.4, 0.06)),
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.72, 0.84, 0.94, 0.8),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            }),
            24.0,
        ),
        Precipitation::Snow => (
            meshes.add(Sphere::new(0.065)),
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.92, 0.96, 1.0, 0.9),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            }),
            3.2,
        ),
    };
    let count = 24 + usize::from(environment.weather.intensity_bps) * 104 / 10_000;
    let wind = f32::from(environment.weather.wind_speed_bps) / 10_000.0 * 8.0;
    let velocity = Vec3::new(wind, -fall_speed, wind * 0.27);
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Y, velocity.normalize_or_zero());
    for index in 0..count {
        let x = fixture_coordinate(index as u64, 0) * 110.0;
        let z = fixture_coordinate(index as u64, 1) * 110.0;
        let y = 3.0 + (fixture_coordinate(index as u64, 2) + 0.5) * 32.0;
        commands.spawn((
            Name::new("Tactical weather particle"),
            WeatherParticle {
                velocity,
                ceiling: 35.0,
            },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, y, z).with_rotation(rotation),
        ));
    }
}

fn advance_weather_particles(
    time: Res<Time>,
    mut particles: Query<(&WeatherParticle, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (particle, mut transform) in &mut particles {
        transform.translation += particle.velocity * delta;
        if transform.translation.y < 0.0 {
            transform.translation.y = particle.ceiling;
            transform.translation.x = wrap_weather_coordinate(transform.translation.x);
            transform.translation.z = wrap_weather_coordinate(transform.translation.z);
        }
    }
}

fn wrap_weather_coordinate(value: f32) -> f32 {
    (value + 55.0).rem_euclid(110.0) - 55.0
}

fn fixture_coordinate(index: u64, axis: u64) -> f32 {
    let mut value = index ^ axis.wrapping_mul(0x9e37_79b9);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    (value % 10_001) as f32 / 10_000.0 - 0.5
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn environment(precipitation: Precipitation, intensity_bps: u16) -> SceneEnvironment {
        SceneEnvironment {
            scene_digest: "fixture".into(),
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
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
    fn precipitation_dims_but_never_extinguishes_sunlight() {
        let clear = scene_sunlight_illuminance(&environment(Precipitation::Clear, 0));
        let rain = scene_sunlight_illuminance(&environment(Precipitation::Rain, 10_000));
        let snow = scene_sunlight_illuminance(&environment(Precipitation::Snow, 10_000));
        assert!(rain < snow && snow < clear);
        assert!(rain >= lux::DIRECT_SUNLIGHT * 0.25);
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
    fn vista_lods_build_independent_overlapping_rings() {
        let input = TacticalSceneInput::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/tactical-scenes/valley-distant-ridge.json"),
        )
        .unwrap();
        let mut inner = 55.0;
        for (index, lod) in input.vista.lods.iter().enumerate() {
            let meshes = vista_lod_meshes(lod, inner);
            assert!(!meshes.is_empty());
            assert!(meshes.iter().all(|mesh| mesh.count_vertices() > 0));
            assert!(meshes.iter().all(|mesh| {
                mesh.count_vertices() <= VISTA_CHUNK_CELLS * VISTA_CHUNK_CELLS * 4
            }));
            if index > 0 {
                assert!(
                    meshes.len() > 1,
                    "regional LODs must be independently culled"
                );
            }
            inner = f32::from(lod.width - 1) * lod.spacing_metres * 0.5;
        }
    }
}
