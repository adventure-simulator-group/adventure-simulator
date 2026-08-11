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
const TREE_IMPOSTOR_SHADER: &str = "shaders/tactical_tree_impostor.wgsl";

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
            MaterialPlugin::<TacticalTreeImpostorMaterial>::default(),
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

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
struct TacticalTreeImpostorMaterial {
    /// Representation level, deterministic seed, wind strength, wind speed.
    #[uniform(0)]
    parameters: Vec4,
    /// Leaf highlight, leaf shadow, and bark colour packed as linear RGBA.
    #[uniform(0)]
    leaf_light: Vec4,
    #[uniform(0)]
    leaf_shadow: Vec4,
    #[uniform(0)]
    bark: Vec4,
}

impl Material for TacticalTreeImpostorMaterial {
    fn vertex_shader() -> ShaderRef {
        TREE_IMPOSTOR_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        TREE_IMPOSTOR_SHADER.into()
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
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
) -> Result {
    let (obstacle, transform) = obstacles.get(event.entity)?;
    let seed = obstacle_seed(transform.translation);
    match *obstacle {
        SceneObstacle::Tree => {
            let branches = procedural_tree_skeleton(seed);
            let bark_material = materials.add(StandardMaterial {
                base_color: tree_bark_color(seed),
                perceptual_roughness: 0.95,
                ..default()
            });
            let branch_meshes =
                [3, 2, 1, 0].map(|depth| meshes.add(procedural_tree_branch_mesh(&branches, depth)));
            let card_meshes = (0..5)
                .map(|lod| meshes.add(procedural_tree_card_mesh(seed, &branches, lod)))
                .collect::<Vec<_>>();
            let card_materials = (0..5)
                .map(|lod| tree_materials.add(tree_impostor_material(seed, lod)))
                .collect::<Vec<_>>();
            commands.entity(event.entity).insert((
                Name::new("Presented tactical tree"),
                TreeLod(0),
                Mesh3d(branch_meshes[0].clone()),
                MeshMaterial3d(bark_material.clone()),
                tree_lod_visibility(0),
            ));
            commands.entity(event.entity).with_children(|parent| {
                for lod in 0..5 {
                    parent.spawn((
                        Name::new(tree_lod_name(lod, true)),
                        TreeLod(lod),
                        NotShadowCaster,
                        Mesh3d(card_meshes[lod as usize].clone()),
                        MeshMaterial3d(card_materials[lod as usize].clone()),
                        tree_lod_visibility(lod),
                    ));
                    if (1..=3).contains(&lod) {
                        parent.spawn((
                            Name::new(tree_lod_name(lod, false)),
                            TreeLod(lod),
                            Mesh3d(branch_meshes[lod as usize].clone()),
                            MeshMaterial3d(bark_material.clone()),
                            tree_lod_visibility(lod),
                        ));
                    }
                }
            });
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

#[derive(Clone, Copy, Debug)]
struct TreeBranchSegment {
    start: Vec3,
    end: Vec3,
    start_radius: f32,
    end_radius: f32,
    depth: u8,
    primary_group: u8,
    secondary_group: u16,
    is_limb_tip: bool,
}

fn procedural_tree_skeleton(seed: u64) -> Vec<TreeBranchSegment> {
    let mut branches = Vec::new();
    let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
    let trunk_bend = Vec3::new(crown_phase.cos(), 0.0, crown_phase.sin())
        * (0.12 + unit_hash(seed ^ 0x51c7_329d) * 0.1);
    let trunk_points = (0..=6)
        .map(|index| {
            let t = index as f32 / 6.0;
            Vec3::new(0.0, -TREE_TRUNK_HEIGHT_METRES * 0.5, 0.0)
                + Vec3::Y * (5.45 * t)
                + trunk_bend * t.powf(1.45)
        })
        .collect::<Vec<_>>();
    for index in 0..6 {
        let t0 = index as f32 / 6.0;
        let t1 = (index + 1) as f32 / 6.0;
        branches.push(TreeBranchSegment {
            start: trunk_points[index],
            end: trunk_points[index + 1],
            start_radius: TREE_TRUNK_RADIUS_METRES * (1.0 - t0 * 0.78),
            end_radius: (TREE_TRUNK_RADIUS_METRES * (1.0 - t1 * 0.78)).max(0.045),
            depth: 0,
            primary_group: u8::MAX,
            secondary_group: u16::MAX,
            is_limb_tip: index == 5,
        });
    }

    for primary_index in 0..10_u64 {
        let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
        let phase = crown_phase
            + primary_index as f32 * 2.399_963_1
            + (unit_hash(primary_seed) - 0.5) * 0.34;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        let trunk_t = 0.43 + primary_index as f32 * 0.045;
        let trunk_scaled = trunk_t * 6.0;
        let trunk_segment = trunk_scaled.floor().min(5.0) as usize;
        let start =
            trunk_points[trunk_segment].lerp(trunk_points[trunk_segment + 1], trunk_scaled.fract());
        let lower_crown = 1.0 - primary_index as f32 / 10.0;
        let reach = 1.45 + lower_crown * 0.95 + unit_hash(primary_seed ^ 1) * 0.22;
        let rise = 1.05 + (1.0 - lower_crown) * 0.8 + unit_hash(primary_seed ^ 2) * 0.25;
        let curve = (unit_hash(primary_seed ^ 3) - 0.5) * 0.52;
        let mut primary_points = [Vec3::ZERO; 4];
        for (point_index, point) in primary_points.iter_mut().enumerate() {
            let t = point_index as f32 / 3.0;
            *point = start
                + outward * reach * t
                + Vec3::Y * (rise * t + 0.16 * (core::f32::consts::PI * t).sin())
                + tangent * curve * (core::f32::consts::PI * t).sin();
        }
        for segment_index in 0..3 {
            let t0 = segment_index as f32 / 3.0;
            let t1 = (segment_index + 1) as f32 / 3.0;
            branches.push(TreeBranchSegment {
                start: primary_points[segment_index],
                end: primary_points[segment_index + 1],
                start_radius: 0.17 * (1.0 - t0 * 0.68),
                end_radius: 0.17 * (1.0 - t1 * 0.68),
                depth: 1,
                primary_group: primary_index as u8,
                secondary_group: u16::MAX,
                is_limb_tip: segment_index == 2,
            });
        }

        for secondary_index in 0..6_u64 {
            let secondary_seed = splitmix64(primary_seed ^ (secondary_index + 11));
            let attach_t = 0.24 + secondary_index as f32 * 0.145;
            let primary_scaled = attach_t * 3.0;
            let segment = primary_scaled.floor().min(2.0) as usize;
            let secondary_start =
                primary_points[segment].lerp(primary_points[segment + 1], primary_scaled.fract());
            let side = if secondary_index & 1 == 0 { 1.0 } else { -1.0 };
            let yaw = phase
                + (secondary_index as f32 - 2.5) * 0.36
                + side * (0.24 + unit_hash(secondary_seed) * 0.26);
            let secondary_outward = Vec3::new(yaw.cos(), 0.0, yaw.sin());
            let inherited = (primary_points[3] - primary_points[2]).normalize();
            let secondary_direction = (inherited * 0.42
                + secondary_outward * 0.72
                + Vec3::Y * (0.44 + unit_hash(secondary_seed ^ 1) * 0.2))
                .normalize();
            let secondary_length = 0.78 + unit_hash(secondary_seed ^ 2) * 0.3;
            let bend = tangent * side * (0.08 + unit_hash(secondary_seed ^ 3) * 0.12);
            let secondary_mid = secondary_start
                + secondary_direction * secondary_length * 0.52
                + bend
                + Vec3::Y * 0.06;
            let secondary_end = secondary_start
                + secondary_direction * secondary_length
                + bend * 0.35
                + Vec3::Y * 0.12;
            let secondary_points = [secondary_start, secondary_mid, secondary_end];
            let secondary_group = (primary_index * 6 + secondary_index) as u16;
            for segment_index in 0..2 {
                branches.push(TreeBranchSegment {
                    start: secondary_points[segment_index],
                    end: secondary_points[segment_index + 1],
                    start_radius: if segment_index == 0 { 0.06 } else { 0.038 },
                    end_radius: if segment_index == 0 { 0.038 } else { 0.018 },
                    depth: 2,
                    primary_group: primary_index as u8,
                    secondary_group,
                    is_limb_tip: segment_index == 1,
                });
            }
            for twig_index in 0..5_u64 {
                let twig_seed = splitmix64(secondary_seed ^ (twig_index + 23));
                let twig_start =
                    secondary_points[1].lerp(secondary_points[2], 0.12 + twig_index as f32 * 0.21);
                let twig_yaw = yaw + (twig_index as f32 - 2.0) * 0.46;
                let twig_direction = (secondary_direction * 0.5
                    + Vec3::new(twig_yaw.cos(), 0.52, twig_yaw.sin()) * 0.62)
                    .normalize();
                branches.push(TreeBranchSegment {
                    start: twig_start,
                    end: twig_start + twig_direction * (0.48 + unit_hash(twig_seed) * 0.2),
                    start_radius: 0.021,
                    end_radius: 0.007,
                    depth: 3,
                    primary_group: primary_index as u8,
                    secondary_group,
                    is_limb_tip: true,
                });
            }
        }
    }
    branches
}

fn procedural_tree_branch_mesh(branches: &[TreeBranchSegment], maximum_depth: u8) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for branch in branches
        .iter()
        .filter(|branch| branch.depth <= maximum_depth)
    {
        append_branch_tube(
            *branch,
            (8_u32.saturating_sub(u32::from(branch.depth))).max(4),
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
        );
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn append_branch_tube(
    branch: TreeBranchSegment,
    sides: u32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let direction = (branch.end - branch.start).normalize();
    let reference = if direction.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = direction.cross(reference).normalize();
    let forward = right.cross(direction).normalize();
    let start_center = branch.start - direction * branch.start_radius * 0.28;
    let end_center_position = branch.end + direction * branch.end_radius * 0.7;
    let base = positions.len() as u32;
    for ring in 0..2 {
        let (center, radius) = if ring == 0 {
            (start_center, branch.start_radius)
        } else {
            (end_center_position, branch.end_radius)
        };
        for side in 0..sides {
            let phase = side as f32 * core::f32::consts::TAU / sides as f32;
            let normal = right * phase.cos() + forward * phase.sin();
            positions.push((center + normal * radius).to_array());
            normals.push(normal.to_array());
            uvs.push([side as f32 / sides as f32, ring as f32]);
        }
    }
    for side in 0..sides {
        let next = (side + 1) % sides;
        indices.extend_from_slice(&[
            base + side,
            base + sides + side,
            base + sides + next,
            base + side,
            base + sides + next,
            base + next,
        ]);
    }
    let end_center = positions.len() as u32;
    positions.push(end_center_position.to_array());
    normals.push(direction.to_array());
    uvs.push([0.5, 1.0]);
    for side in 0..sides {
        let next = (side + 1) % sides;
        indices.extend_from_slice(&[end_center, base + sides + side, base + sides + next]);
    }
}

#[derive(Clone, Copy)]
struct TreeCrownBounds {
    minimum: Vec3,
    maximum: Vec3,
}

impl TreeCrownBounds {
    fn center(self) -> Vec3 {
        (self.minimum + self.maximum) * 0.5
    }

    fn horizontal_span(self) -> f32 {
        (self.maximum.x - self.minimum.x).max(self.maximum.z - self.minimum.z)
    }

    fn vertical_span(self) -> f32 {
        self.maximum.y - self.minimum.y
    }
}

fn tree_crown_bounds(
    branches: &[TreeBranchSegment],
    mut includes: impl FnMut(&TreeBranchSegment) -> bool,
) -> TreeCrownBounds {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for branch in branches.iter().filter(|branch| includes(branch)) {
        minimum = minimum.min(branch.start).min(branch.end);
        maximum = maximum.max(branch.start).max(branch.end);
    }
    debug_assert!(minimum.is_finite() && maximum.is_finite());
    TreeCrownBounds { minimum, maximum }
}

fn procedural_tree_card_mesh(seed: u64, branches: &[TreeBranchSegment], lod: u8) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    match lod {
        0 => {
            for (index, branch) in branches
                .iter()
                .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
                .enumerate()
            {
                let direction = (branch.end - branch.start).normalize();
                let tangent = direction.cross(Vec3::Y).normalize_or_zero();
                let tangent = if tangent.length_squared() < 0.5 {
                    Vec3::X
                } else {
                    tangent
                };
                let binormal = direction.cross(tangent).normalize();
                for leaf in 0..13_u64 {
                    let leaf_seed =
                        splitmix64(seed ^ index as u64 ^ leaf.wrapping_mul(0x91e1_0da5));
                    let phase = unit_hash(leaf_seed) * 0.65 + leaf as f32 * 2.399_963_1;
                    let radial = (tangent * phase.cos() + binormal * phase.sin()).normalize();
                    let along = 0.1 + unit_hash(leaf_seed ^ 8) * 0.88;
                    let leaf_axis = (radial * 0.78
                        + direction * 0.22
                        + Vec3::Y * (0.22 - unit_hash(leaf_seed ^ 4) * 0.44))
                        .normalize();
                    let right = direction.cross(leaf_axis).normalize_or_zero();
                    let right = if right.length_squared() < 0.5 {
                        tangent
                    } else {
                        right
                    };
                    let center = branch.start.lerp(branch.end, along.min(0.98))
                        + radial * (0.055 + unit_hash(leaf_seed ^ 5) * 0.11)
                        + Vec3::Y * ((unit_hash(leaf_seed ^ 9) - 0.5) * 0.16);
                    append_tree_card(
                        center,
                        right,
                        leaf_axis,
                        0.15 + unit_hash(leaf_seed ^ 6) * 0.055,
                        0.24 + unit_hash(leaf_seed ^ 7) * 0.07,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        1 => {
            for group in 0..60_u16 {
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3 && branch.secondary_group == group
                });
                let up = branches
                    .iter()
                    .find(|branch| {
                        branch.depth == 2 && branch.secondary_group == group && branch.is_limb_tip
                    })
                    .map(|branch| (branch.end - branch.start).normalize())
                    .unwrap_or(Vec3::Y);
                let index = usize::from(group);
                let phase = unit_hash(splitmix64(seed ^ index as u64)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center() + Vec3::Y * 0.05,
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        up,
                        (bounds.horizontal_span() + 0.45) * 1.05,
                        (bounds.vertical_span() + 0.5) * 1.08,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        2 => {
            for group in 0..10_u8 {
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3 && branch.primary_group == group
                });
                let index = usize::from(group);
                let phase =
                    unit_hash(splitmix64(seed ^ index as u64 ^ 0x4a17)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center(),
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        Vec3::Y,
                        (bounds.horizontal_span() + 0.9) * 1.28,
                        (bounds.vertical_span() + 0.9) * 1.18,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        3 => {
            for index in 0..5 {
                let first = index as u8;
                let second = (index + 5) as u8;
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3
                        && (branch.primary_group == first || branch.primary_group == second)
                });
                let phase =
                    unit_hash(splitmix64(seed ^ index as u64 ^ 0x7c31)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center() + Vec3::Y * 0.08,
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        Vec3::Y,
                        (bounds.horizontal_span() + 1.0) * 1.2,
                        (bounds.vertical_span() + 1.0) * 1.15,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        4 => {
            let bounds = tree_crown_bounds(branches, |branch| branch.depth == 3);
            let bottom = -TREE_TRUNK_HEIGHT_METRES * 0.5;
            let top = bounds.maximum.y + 0.45;
            append_tree_card(
                Vec3::new(0.0, (bottom + top) * 0.5, 0.0),
                Vec3::X,
                Vec3::Y,
                (bounds.horizontal_span() + 1.25) * 1.15,
                top - bottom,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
            );
        }
        _ => unreachable!("tree LOD is bounded"),
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[allow(clippy::too_many_arguments)]
fn append_tree_card(
    center: Vec3,
    right: Vec3,
    up: Vec3,
    width: f32,
    height: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let right = right.normalize() * width * 0.5;
    let up = up.normalize() * height * 0.5;
    let normal = right.cross(up).normalize_or_zero();
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        (center - right - up).to_array(),
        (center + right - up).to_array(),
        (center + right + up).to_array(),
        (center - right + up).to_array(),
    ]);
    normals.extend_from_slice(&[normal.to_array(); 4]);
    uvs.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn tree_impostor_material(seed: u64, lod: u8) -> TacticalTreeImpostorMaterial {
    let hue = unit_hash(seed ^ 0x2f37_84c1) - 0.5;
    TacticalTreeImpostorMaterial {
        parameters: Vec4::new(lod as f32, unit_hash(seed), 0.08 + lod as f32 * 0.018, 1.0),
        leaf_light: Color::srgb(0.24 + hue * 0.05, 0.5 + hue * 0.06, 0.13)
            .to_linear()
            .to_f32_array()
            .into(),
        leaf_shadow: Color::srgb(0.05, 0.2 + hue * 0.03, 0.04)
            .to_linear()
            .to_f32_array()
            .into(),
        bark: tree_bark_color(seed).to_linear().to_f32_array().into(),
    }
}

fn tree_bark_color(seed: u64) -> Color {
    let variation = unit_hash(seed ^ 0x6b11_2e09) - 0.5;
    Color::srgb(0.38 + variation * 0.05, 0.23 + variation * 0.03, 0.12)
}

fn tree_lod_visibility(lod: u8) -> VisibilityRange {
    let (start, end) = match lod {
        0 => (0.0..0.0, 18.0..22.0),
        1 => (18.0..22.0, 32.0..38.0),
        2 => (32.0..38.0, 52.0..60.0),
        3 => (52.0..60.0, 78.0..90.0),
        4 => (78.0..90.0, 190.0..200.0),
        _ => unreachable!("tree LOD is bounded"),
    };
    VisibilityRange {
        start_margin: start,
        end_margin: end,
        use_aabb: false,
    }
}

fn tree_lod_name(lod: u8, cards: bool) -> String {
    let representation = match lod {
        0 => "individual leaves",
        1 => "leafed twigs",
        2 => "small branches",
        3 => "crown branches",
        4 => "whole-tree billboard",
        _ => unreachable!("tree LOD is bounded"),
    };
    format!(
        "Tree LOD {lod} {representation} {}",
        if cards { "cards" } else { "wood" }
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
    fn procedural_tree_has_a_deterministic_four_order_branch_hierarchy() {
        let branches = procedural_tree_skeleton(42);
        let counts = (0..=3)
            .map(|depth| {
                branches
                    .iter()
                    .filter(|branch| branch.depth == depth)
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(counts, vec![6, 30, 120, 300]);
        assert!(branches.iter().all(|branch| branch.end.y > branch.start.y));
        assert!(
            branches.iter().all(|branch| {
                branch.start_radius > branch.end_radius && branch.end_radius > 0.0
            })
        );
    }

    #[test]
    fn tree_lods_collapse_one_botanical_order_at_a_time() {
        let branches = procedural_tree_skeleton(42);
        let expected_cards = [3_900, 120, 20, 10, 1];
        for (lod, expected) in expected_cards.into_iter().enumerate() {
            let mesh = procedural_tree_card_mesh(42, &branches, lod as u8);
            assert_eq!(mesh.count_vertices(), expected * 4);
        }
        let branch_vertices = (0..=3)
            .rev()
            .map(|depth| procedural_tree_branch_mesh(&branches, depth).count_vertices())
            .collect::<Vec<_>>();
        assert!(branch_vertices.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn tree_lod_crossfades_share_exact_transition_margins() {
        for lod in 0..4 {
            let current = tree_lod_visibility(lod);
            let next = tree_lod_visibility(lod + 1);
            assert_eq!(current.end_margin, next.start_margin);
            assert!(!current.is_abrupt());
        }
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
