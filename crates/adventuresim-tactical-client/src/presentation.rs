//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::{
    asset::RenderAssetUsages,
    camera::{Exposure, visibility::VisibilityRange},
    core_pipeline::tonemapping::Tonemapping,
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, NotShadowCaster, atmosphere::ScatteringMedium,
        light_consts::lux,
    },
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::{AtmosphereSettings, ExtendedMaterial, MaterialExtension, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

const TERRAIN_SHADER: &str = "shaders/tactical_terrain.wgsl";
const FOLIAGE_SHADER: &str = "shaders/tactical_foliage.wgsl";

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
        app.add_plugins((
            MaterialPlugin::<TacticalTerrainMaterial>::default(),
            MaterialPlugin::<TacticalFoliageMaterial>::default(),
        ))
        .insert_resource(TacticalGraphicsSettings {
            shadows_enabled: self.shadows_enabled,
            atmosphere_enabled: self.atmosphere_enabled,
            environment_light_enabled: self.environment_light_enabled,
            environment_map_size: self.environment_map_size,
            bloom_enabled: self.bloom_enabled,
            ssao_enabled: self.ssao_enabled,
            max_vista_lods: self.max_vista_lods,
        })
        .add_systems(Startup, setup_tactical_presentation)
        .init_resource::<GrassInteractionState>()
        .add_systems(
            Update,
            (advance_weather_particles, update_grass_interaction),
        )
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
pub(crate) struct TerrainMaterialPresentation;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
struct TacticalTerrainExtension {
    #[uniform(100)]
    base_color: Vec4,
    #[uniform(100)]
    cover: Vec4,
    #[uniform(100)]
    weather: Vec4,
    #[uniform(100)]
    variation: Vec4,
}

impl MaterialExtension for TacticalTerrainExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

type TacticalTerrainMaterial = ExtendedMaterial<StandardMaterial, TacticalTerrainExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
struct TacticalFoliageMaterial {
    #[uniform(0)]
    wind: Vec4,
    #[uniform(0)]
    interaction: Vec4,
    #[uniform(0)]
    interaction_motion: Vec4,
    #[uniform(0)]
    lod: Vec4,
    #[uniform(0)]
    shading: Vec4,
}

impl Material for TacticalFoliageMaterial {
    fn vertex_shader() -> ShaderRef {
        FOLIAGE_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        FOLIAGE_SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FoliageLayer {
    Grass,
    Understory,
}

/// Marks the locally controlled character whose movement bends nearby grass.
#[derive(Component)]
pub(crate) struct GrassInteractor;

#[derive(Resource, Default)]
struct GrassInteractionState {
    previous_position: Option<Vec3>,
    smoothed_velocity: Vec3,
}

#[derive(Component)]
struct FoliageOf(Entity);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TreeLod(pub(crate) u8);

#[derive(Component)]
pub(crate) struct ProceduralRockVisual;

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
    mut materials: ResMut<Assets<TacticalTerrainMaterial>>,
) -> Result {
    let (id, terrain, environment) = query.get(event.entity)?;
    info!(entity = ?event.entity, "Spawning a scene {id:?}");

    let legacy_environment;
    let environment = if let Some(environment) = environment {
        environment
    } else {
        legacy_environment = legacy_scene_environment(id);
        &legacy_environment
    };

    commands.spawn((
        Name::new(format!("{} terrain mesh", id.0)),
        ScenePresentationOf(event.entity),
        TerrainMaterialPresentation,
        Mesh3d(meshes.add(terrain.mesh())),
        MeshMaterial3d(materials.add(terrain_material(environment))),
    ));
    Ok(())
}

fn on_scene_environment_added(
    event: On<Add, SceneEnvironment>,
    environments: Query<&SceneEnvironment>,
    presentations: Query<(
        &ScenePresentationOf,
        &MeshMaterial3d<TacticalTerrainMaterial>,
    )>,
    scenes: Query<(&SceneId, &SceneTerrain)>,
    foliage: Query<&FoliageOf>,
    mut sunlight: Single<&mut DirectionalLight, With<TacticalSunlight>>,
    mut fog: Single<&mut DistanceFog, With<Camera3d>>,
    particles: Query<Entity, With<WeatherParticle>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut terrain_materials: ResMut<Assets<TacticalTerrainMaterial>>,
    mut foliage_materials: ResMut<Assets<TacticalFoliageMaterial>>,
) -> Result {
    let environment = environments.get(event.entity)?;
    for (source, material) in &presentations {
        if source.0 == event.entity
            && let Some(mut material) = terrain_materials.get_mut(&material.0)
        {
            *material = terrain_material(environment);
        }
    }
    if !foliage.iter().any(|source| source.0 == event.entity) {
        let (scene_id, terrain) = scenes.get(event.entity)?;
        spawn_ground_foliage(
            &mut commands,
            &mut meshes,
            &mut foliage_materials,
            event.entity,
            scene_id,
            terrain,
            environment,
        );
    }
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
    obstacles: Query<(&SceneObstacle, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foliage_materials: ResMut<Assets<TacticalFoliageMaterial>>,
) -> Result {
    let (obstacle, transform) = obstacles.get(event.entity)?;
    let seed = obstacle_seed(transform.translation);
    match *obstacle {
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
            let crown_material = foliage_materials.add(foliage_material(0.32, false));
            let high_crown = meshes.add(tree_crown_mesh(seed, 2, 1.42));
            let side_crown_a = meshes.add(tree_crown_mesh(seed ^ 0x41ac_921d, 2, 1.08));
            let side_crown_b = meshes.add(tree_crown_mesh(seed ^ 0xc337_8ba9, 2, 1.14));
            let low_crown = meshes.add(tree_crown_mesh(seed, 1, 1.58));
            let billboard = meshes.add(foliage_clump_mesh(2.7, 3.4, Color::srgb_u8(40, 78, 34), 2));
            let branch_mesh = meshes.add(Cylinder::new(0.12, 1.8));
            commands.entity(event.entity).insert((
                Name::new("Presented tactical tree"),
                Mesh3d(trunk_mesh),
                MeshMaterial3d(trunk_material.clone()),
                VisibilityRange::abrupt(0.0, 82.0),
                children![
                    (
                        Name::new("Tree crown LOD 0"),
                        TreeLod(0),
                        NotShadowCaster,
                        Mesh3d(high_crown),
                        MeshMaterial3d(crown_material.clone()),
                        VisibilityRange::abrupt(0.0, 34.0),
                        Transform::from_xyz(0.0, TREE_TRUNK_HEIGHT_METRES * 0.42, 0.0),
                    ),
                    (
                        Name::new("Tree crown LOD 1"),
                        TreeLod(1),
                        NotShadowCaster,
                        Mesh3d(low_crown),
                        MeshMaterial3d(crown_material.clone()),
                        VisibilityRange::abrupt(30.0, 76.0),
                        Transform::from_xyz(0.0, TREE_TRUNK_HEIGHT_METRES * 0.42, 0.0),
                    ),
                    tree_branch_bundle(0, seed, branch_mesh.clone(), trunk_material.clone(),),
                    tree_branch_bundle(1, seed, branch_mesh.clone(), trunk_material.clone(),),
                    tree_branch_bundle(2, seed, branch_mesh, trunk_material.clone()),
                    (
                        Name::new("Tree crown lobe A"),
                        TreeLod(0),
                        NotShadowCaster,
                        Mesh3d(side_crown_a),
                        MeshMaterial3d(crown_material.clone()),
                        VisibilityRange::abrupt(0.0, 34.0),
                        Transform::from_xyz(-0.62, TREE_TRUNK_HEIGHT_METRES * 0.34, 0.28),
                    ),
                    (
                        Name::new("Tree crown lobe B"),
                        TreeLod(0),
                        NotShadowCaster,
                        Mesh3d(side_crown_b),
                        MeshMaterial3d(crown_material.clone()),
                        VisibilityRange::abrupt(0.0, 34.0),
                        Transform::from_xyz(0.58, TREE_TRUNK_HEIGHT_METRES * 0.37, -0.22),
                    ),
                    (
                        Name::new("Tree billboard LOD 2"),
                        TreeLod(2),
                        NotShadowCaster,
                        Mesh3d(billboard),
                        MeshMaterial3d(crown_material),
                        VisibilityRange::abrupt(70.0, 190.0),
                        Transform::from_xyz(0.0, TREE_TRUNK_HEIGHT_METRES * 0.42, 0.0),
                    ),
                ],
            ));
        }
        SceneObstacle::Rock => {
            commands.entity(event.entity).insert((
                Name::new("Presented tactical rock"),
                ProceduralRockVisual,
                Mesh3d(meshes.add(procedural_rock_mesh(seed))),
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

fn terrain_material(environment: &SceneEnvironment) -> TacticalTerrainMaterial {
    TacticalTerrainMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.92,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalTerrainExtension {
            base_color: color_vec4(scene_ground_color(environment)),
            cover: Vec4::new(
                bps(environment.canopy_bps),
                bps(environment.wetland_bps),
                bps(environment.cultivation_bps),
                bps(environment.water_bps),
            ),
            weather: Vec4::new(
                bps(environment.weather.ground_moisture_bps),
                bps(environment.weather.snow_cover_bps),
                bps(environment.hilly_bps),
                bps(environment.weather.wind_speed_bps),
            ),
            variation: Vec4::new(
                digest_unit(&environment.scene_digest),
                0.055,
                0.032,
                environment.generation_version as f32,
            ),
        },
    }
}

fn legacy_scene_environment(id: &SceneId) -> SceneEnvironment {
    let (canopy_bps, hilly_bps, cultivation_bps) = match id.0.as_str() {
        "hills" => (1_200, 7_000, 0),
        "desert" => (0, 1_500, 0),
        value => {
            warn!("Unknown legacy scene: {value}");
            (0, 0, 0)
        }
    };
    SceneEnvironment {
        scene_digest: id.0.clone(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        weather: WeatherSnapshot {
            rules_version: WEATHER_RULES_VERSION,
            interval_start_minute: 0,
            cell_latitude: 0,
            cell_longitude: 0,
            temperature_deci_c: 100,
            wind_speed_bps: 1_500,
            precipitation: Precipitation::Clear,
            intensity_bps: 0,
            ground_moisture_bps: 0,
            snow_cover_bps: 0,
        },
        canopy_bps,
        wetland_bps: 0,
        cultivation_bps,
        water_bps: 0,
        hilly_bps,
    }
}

fn foliage_material(wind_scale: f32, ground_foliage: bool) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
        interaction: Vec4::ZERO,
        interaction_motion: Vec4::ZERO,
        lod: if ground_foliage {
            Vec4::new(24.0, 120.0, 0.18, 1.0)
        } else {
            Vec4::ZERO
        },
        // Root brightness, meadow colour variation, normal up-bias, and
        // whether nearby player movement affects this material.
        shading: if ground_foliage {
            Vec4::new(0.42, 0.13, 0.76, 1.0)
        } else {
            Vec4::new(0.55, 0.08, 0.28, 0.0)
        },
    }
}

fn update_grass_interaction(
    time: Res<Time>,
    interactors: Query<&GlobalTransform, With<GrassInteractor>>,
    mut state: ResMut<GrassInteractionState>,
    mut materials: ResMut<Assets<TacticalFoliageMaterial>>,
) {
    let Some(position) = interactors.iter().next().map(GlobalTransform::translation) else {
        state.previous_position = None;
        state.smoothed_velocity = Vec3::ZERO;
        for (_, material) in materials.iter_mut() {
            material.interaction = Vec4::ZERO;
            material.interaction_motion = Vec4::ZERO;
        }
        return;
    };
    let delta_seconds = time.delta_secs().max(1.0 / 240.0);
    let velocity = state
        .previous_position
        .map(|previous| ((position - previous) / delta_seconds).clamp_length_max(8.0))
        .unwrap_or_default();
    let response = 1.0 - (-delta_seconds * 10.0).exp();
    state.smoothed_velocity = state.smoothed_velocity.lerp(velocity, response);
    state.previous_position = Some(position);
    let speed = state.smoothed_velocity.length();
    for (_, material) in materials.iter_mut() {
        if material.shading.w <= 0.5 {
            continue;
        }
        material.interaction = position.extend(1.35);
        material.interaction_motion = Vec4::new(
            state.smoothed_velocity.x,
            state.smoothed_velocity.y,
            state.smoothed_velocity.z,
            (0.7 + speed * 0.11).clamp(0.7, 1.35),
        );
    }
}

fn spawn_ground_foliage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TacticalFoliageMaterial>,
    source: Entity,
    scene_id: &SceneId,
    terrain: &SceneTerrain,
    environment: &SceneEnvironment,
) {
    let grass_color = if environment.weather.snow_cover_bps >= 5_000 {
        Color::srgb_u8(155, 164, 137)
    } else if environment.cultivation_bps >= 4_000 {
        Color::srgb_u8(142, 133, 61)
    } else {
        Color::srgb_u8(82, 119, 45)
    };
    let grass_mesh = meshes.add(grass_patch_mesh(grass_color));
    let understory_mesh = meshes.add(if environment.weather.snow_cover_bps >= 5_000 {
        foliage_clump_mesh(0.72, 0.92, Color::srgb_u8(130, 144, 119), 3)
    } else if environment.wetland_bps >= 3_000 {
        foliage_clump_mesh(0.42, 1.35, Color::srgb_u8(75, 112, 58), 4)
    } else {
        foliage_clump_mesh(0.9, 1.05, Color::srgb_u8(52, 91, 43), 3)
    });
    let grass_material = materials.add(foliage_material(
        0.16 + bps(environment.weather.wind_speed_bps) * 0.36,
        true,
    ));
    let understory_material = materials.add(foliage_material(
        0.1 + bps(environment.weather.wind_speed_bps) * 0.24,
        true,
    ));
    let base_seed = stable_text_seed(&environment.scene_digest) ^ stable_text_seed(&scene_id.0);
    let canopy = bps(environment.canopy_bps);
    let water = bps(environment.water_bps);
    let wetland = bps(environment.wetland_bps);
    let cultivation = bps(environment.cultivation_bps);
    let snow = bps(environment.weather.snow_cover_bps);
    let grass_chance = (0.96 - canopy * 0.16 - water * 0.88 + cultivation * 0.04).clamp(0.06, 0.98)
        * (1.0 - snow * 0.36);
    let understory_chance = (canopy * 0.16 + wetland * 0.22).clamp(0.0, 0.24);
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    // Each instance is a forty-nine-blade patch whose footprint overlaps its
    // neighbours. This keeps the entity count bounded while producing the
    // near-continuous oblique coverage expected from grassland.
    let spacing = 1.0;
    let count_x = (terrain.width() / spacing).floor() as i32;
    let count_z = (terrain.depth() / spacing).floor() as i32;
    for z in 0..count_z {
        for x in 0..count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell);
            let choose = unit_hash(hash);
            let layer = if choose < understory_chance {
                Some(FoliageLayer::Understory)
            } else if choose < understory_chance + grass_chance {
                Some(FoliageLayer::Grass)
            } else {
                None
            };
            let Some(layer) = layer else { continue };
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let world_x = -half_x + (x as f32 + 0.5 + jitter_x * 0.72) * spacing;
            let world_z = -half_z + (z as f32 + 0.5 + jitter_z * 0.72) * spacing;
            let Some(height) = terrain.height_at(Vec2::new(world_x, world_z)) else {
                continue;
            };
            if terrain
                .normal_at(Vec2::new(world_x, world_z))
                .is_none_or(|normal| normal.y < 0.72)
            {
                continue;
            }
            let scale = 0.72 + unit_hash(splitmix64(hash ^ 0x8c0a_3c95)) * 0.58;
            let (mesh, material) = match layer {
                FoliageLayer::Grass => (grass_mesh.clone(), grass_material.clone()),
                FoliageLayer::Understory => (understory_mesh.clone(), understory_material.clone()),
            };
            commands.spawn((
                Name::new(match layer {
                    FoliageLayer::Grass => "Tactical grass clump",
                    FoliageLayer::Understory => "Tactical understory clump",
                }),
                FoliageOf(source),
                layer,
                NotShadowCaster,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                VisibilityRange::abrupt(
                    0.0,
                    if layer == FoliageLayer::Grass {
                        130.0
                    } else {
                        92.0
                    },
                ),
                Transform::from_xyz(world_x, height, world_z)
                    .with_rotation(Quat::from_rotation_y(
                        unit_hash(hash) * core::f32::consts::TAU,
                    ))
                    .with_scale(Vec3::splat(scale)),
            ));
        }
    }
}

fn grass_patch_mesh(color: Color) -> Mesh {
    let blades = (0..49)
        .map(|index| {
            let row = index / 7;
            let column = index % 7;
            let hash = splitmix64(index as u64 ^ 0x8d12_6f4a_0bc3_7791);
            let jitter_x = (unit_hash(hash) - 0.5) * 0.07;
            let jitter_z = (unit_hash(splitmix64(hash)) - 0.5) * 0.07;
            let scale = 0.68 + unit_hash(splitmix64(hash ^ 0x52a9_f131)) * 0.36;
            (
                (column as f32 - 3.0) * 0.18 + jitter_x,
                (row as f32 - 3.0) * 0.18 + jitter_z,
                scale,
            )
        })
        .collect::<Vec<_>>();
    foliage_patch_mesh(0.045, 0.82, color, 2, &blades)
}

fn foliage_clump_mesh(width: f32, height: f32, color: Color, planes: usize) -> Mesh {
    foliage_patch_mesh(width, height, color, planes, &[(0.0, 0.0, 1.0)])
}

fn foliage_patch_mesh(
    width: f32,
    height: f32,
    color: Color,
    planes: usize,
    tufts: &[(f32, f32, f32)],
) -> Mesh {
    let mut positions = Vec::with_capacity(tufts.len() * planes * 5);
    let mut normals = Vec::with_capacity(tufts.len() * planes * 5);
    let mut uvs = Vec::with_capacity(tufts.len() * planes * 5);
    let mut blade_roots = Vec::with_capacity(tufts.len() * planes * 5);
    let mut colors = Vec::with_capacity(tufts.len() * planes * 5);
    let mut indices = Vec::with_capacity(tufts.len() * planes * 9);
    let linear = color.to_linear().to_f32_array();
    for (tuft_index, &(offset_x, offset_z, tuft_scale)) in tufts.iter().enumerate() {
        let centre = Vec3::new(offset_x, 0.0, offset_z);
        let blade_threshold = unit_hash(splitmix64(tuft_index as u64 ^ 0x3d91_02ea_61b8_7c45));
        let blade_color = [linear[0], linear[1], linear[2], blade_threshold];
        for plane in 0..planes {
            let angle = plane as f32 * core::f32::consts::PI / planes as f32;
            let direction = Vec3::new(angle.cos(), 0.0, angle.sin()) * width * tuft_scale * 0.5;
            let shoulder = direction * 0.48;
            let tip = Vec3::Y * height * tuft_scale;
            let base = positions.len() as u32;
            positions.extend_from_slice(&[
                (centre - direction).to_array(),
                (centre + direction).to_array(),
                (centre - shoulder + tip * 0.72).to_array(),
                (centre + shoulder + tip * 0.72).to_array(),
                (centre + tip).to_array(),
            ]);
            let normal = Vec3::Y.cross(direction).normalize_or_zero().to_array();
            normals.extend_from_slice(&[normal; 5]);
            uvs.extend_from_slice(&[
                [0.0, 0.0],
                [1.0, 0.0],
                [0.25, 0.72],
                [0.75, 0.72],
                [0.5, 1.0],
            ]);
            blade_roots.extend_from_slice(&[[offset_x, offset_z]; 5]);
            colors.extend_from_slice(&[blade_color; 5]);
            indices.extend_from_slice(&[
                base,
                base + 1,
                base + 3,
                base,
                base + 3,
                base + 2,
                base + 2,
                base + 3,
                base + 4,
            ]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, blade_roots);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn tree_crown_mesh(seed: u64, subdivisions: u32, radius: f32) -> Mesh {
    let mut mesh = Sphere::new(radius)
        .mesh()
        .ico(subdivisions)
        .expect("valid tree crown");
    let color = if seed & 1 == 0 {
        Color::srgb_u8(45, 86, 38)
    } else {
        Color::srgb_u8(54, 92, 40)
    };
    let linear = color.to_linear().to_f32_array();
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for position in positions.iter_mut() {
            let point = Vec3::from_array(*position);
            let phase = point.x * 1.7 + point.y * 1.1 + point.z * 2.3 + unit_hash(seed) * 4.0;
            let scale = 0.9 + phase.sin() * 0.08;
            *position = Vec3::new(
                point.x * scale * 0.92,
                point.y * (1.08 + scale * 0.16),
                point.z * scale * 0.92,
            )
            .to_array();
        }
        let count = positions.len();
        let uvs = positions
            .iter()
            .map(|position| [0.5, (position[1] / radius * 0.5 + 0.5).clamp(0.0, 1.0)])
            .collect::<Vec<_>>();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![linear; count]);
    }
    mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL);
    mesh.with_computed_area_weighted_normals()
}

fn tree_branch_bundle(
    index: u64,
    seed: u64,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
) -> impl Bundle {
    let phase =
        unit_hash(splitmix64(seed ^ index.wrapping_mul(0x9e37_79b9))) * core::f32::consts::TAU;
    let direction = Vec3::new(phase.cos() * 0.7, 0.72, phase.sin() * 0.7).normalize();
    let center = Vec3::Y * 0.9 + direction * 0.62;
    (
        Name::new(format!("Tree primary branch {}", index + 1)),
        TreeLod(0),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        VisibilityRange::abrupt(0.0, 34.0),
        Transform::from_translation(center)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, direction)),
    )
}

fn procedural_rock_mesh(seed: u64) -> Mesh {
    let mut mesh = Sphere::new(ROCK_RADIUS_METRES)
        .mesh()
        .ico(2)
        .expect("valid procedural rock seed mesh");
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for position in positions {
            let point = Vec3::from_array(*position);
            let direction = point.normalize_or_zero();
            let phase = direction.x * 4.7
                + direction.y * 6.1
                + direction.z * 5.3
                + unit_hash(seed) * core::f32::consts::TAU;
            let radius = ROCK_RADIUS_METRES * (0.82 + 0.12 * phase.sin());
            *position = Vec3::new(
                direction.x * radius,
                direction.y * radius * 0.78,
                direction.z * radius,
            )
            .to_array();
        }
    }
    mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL);
    mesh.with_computed_area_weighted_normals()
}

fn obstacle_seed(position: Vec3) -> u64 {
    splitmix64(u64::from(position.x.to_bits()) << 32 ^ u64::from(position.z.to_bits()))
}

fn stable_text_seed(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn digest_unit(value: &str) -> f32 {
    unit_hash(stable_text_seed(value))
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn unit_hash(value: u64) -> f32 {
    (value >> 40) as f32 / ((1_u32 << 24) - 1) as f32
}

fn bps(value: u16) -> f32 {
    f32::from(value) / 10_000.0
}

fn color_vec4(color: Color) -> Vec4 {
    Vec4::from_array(color.to_linear().to_f32_array())
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
    fn procedural_rocks_remain_inside_the_authoritative_sphere() {
        for seed in [0, 1, 42, u64::MAX] {
            let mesh = procedural_rock_mesh(seed);
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(VertexAttributeValues::as_float3)
                .unwrap();
            assert!(positions.iter().all(|position| {
                Vec3::from_array(*position).length() <= ROCK_RADIUS_METRES + 0.001
            }));
        }
    }

    #[test]
    fn foliage_clumps_carry_root_to_tip_wind_weights() {
        let mesh = foliage_clump_mesh(0.5, 0.8, Color::WHITE, 3);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("foliage mesh must carry float2 UV wind weights");
        };
        assert!(uvs.iter().any(|uv| uv[1] == 0.0));
        assert!(uvs.iter().any(|uv| uv[1] == 1.0));
        assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
    }

    #[test]
    fn grass_patches_pack_forty_nine_thin_blades_into_each_instance() {
        let mesh = grass_patch_mesh(Color::WHITE);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(positions.len(), 49 * 2 * 5);
        let Some(VertexAttributeValues::Float32x2(roots)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry stable blade roots");
        };
        assert_eq!(roots.len(), positions.len());
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("grass mesh must carry stable blade thresholds");
        };
        assert!(colors.iter().all(|color| (0.0..1.0).contains(&color[3])));
        assert!(colors.iter().any(|color| color[3] < 0.25));
        assert!(colors.iter().any(|color| color[3] > 0.75));
    }

    #[test]
    fn ground_foliage_enables_continuous_lod_and_interaction() {
        let grass = foliage_material(0.3, true);
        let crown = foliage_material(0.3, false);
        assert_eq!(grass.lod, Vec4::new(24.0, 120.0, 0.18, 1.0));
        assert_eq!(grass.shading.w, 1.0);
        assert_eq!(crown.lod, Vec4::ZERO);
        assert_eq!(crown.shading.w, 0.0);
    }

    #[test]
    fn local_interactor_position_reaches_only_ground_foliage_materials() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.init_resource::<Assets<TacticalFoliageMaterial>>();
        app.init_resource::<GrassInteractionState>();
        app.add_systems(Update, update_grass_interaction);
        let (grass, crown) = {
            let mut materials = app
                .world_mut()
                .resource_mut::<Assets<TacticalFoliageMaterial>>();
            (
                materials.add(foliage_material(0.3, true)),
                materials.add(foliage_material(0.3, false)),
            )
        };
        app.world_mut().spawn((
            GrassInteractor,
            GlobalTransform::from_translation(Vec3::new(3.0, 1.0, -2.0)),
        ));

        app.update();

        let materials = app.world().resource::<Assets<TacticalFoliageMaterial>>();
        assert_eq!(
            materials.get(&grass).unwrap().interaction,
            Vec4::new(3.0, 1.0, -2.0, 1.35)
        );
        assert_eq!(materials.get(&crown).unwrap().interaction, Vec4::ZERO);
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
