use adventuresim_tactical_core::prelude::{SceneEnvironment, SceneGround, SceneId, SceneTerrain};
use bevy::{
    color::{ColorToComponents, LinearRgba},
    ecs::change_detection::DetectChanges,
    pbr::Material,
    prelude::{
        AlphaMode, Asset, Assets, Color, Commands, Component, Entity, GlobalTransform, Handle,
        Image, Mesh, Quat, Query, Reflect, Res, ResMut, Resource, StandardMaterial, Time,
        Transform, Vec2, Vec3, Vec4, With, Without, default,
    },
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

use super::obstacles::tree::{
    BLACKTHORN_BARK, BLACKTHORN_PARAMETERS, COMMON_HAWTHORN_BARK, COMMON_HAWTHORN_PARAMETERS,
    COMMON_HAZEL_BARK, COMMON_HAZEL_PARAMETERS, TacticalTreeBarkMaterial,
    TacticalTreeImpostorMaterial, TacticalTreeLeafCardMaterial, TreeLeafRepresentation,
    blackthorn_leaf_material, hawthorn_leaf_material, hazel_leaf_material,
    procedural_woody_branch_mesh, procedural_woody_cambered_leaf_mesh,
    procedural_woody_leaf_card_mesh, procedural_woody_plant_leaves,
    procedural_woody_plant_skeleton,
};
use super::{
    PresentedCelestialLighting, ProceduralEnvironmentAssets, bps, grass_cover_mask_image,
    splitmix64, stable_text_seed, unit_hash,
};

// Ground-scatter orchestration and shared presentation contracts.

mod grass;
mod litter;
mod loose_stone;
mod understory;

use grass::grass_material;
pub(in crate::presentation) use grass::{
    GRASS_PATCH_SPACING, GrassCommunity, GrassCommunityProfile, GrassMeshLod, GrassTopology,
    VISTA_GRASS_PATCH_SPACING, grass_community_at, grass_lod_visibility, grass_patch_mesh,
    vista_grass_material,
};
use litter::{
    DRY_LEAF_MESH_VARIANTS, TWIG_MESH_VARIANTS, dry_leaf_patch_mesh, forest_floor_leaf_material,
    twig_patch_mesh,
};
pub(crate) use loose_stone::{LooseStonePebblePatch, TacticalPebbleBillboardMaterial};

/// Real generated litter placements retained independently of batched mesh origins.
/// Capture diagnostics use this bounded pair to frame dry leaves and twigs without
/// mistaking a shared batch-cell transform for the rendered subjects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GroundLitterCapturePair {
    pub(crate) dry_leaf: Vec3,
    pub(crate) twig: Vec3,
}

#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct GroundLitterCaptureAnchors {
    pub(crate) pairs: Vec<GroundLitterCapturePair>,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GroundLitterDiagnostics {
    pub(crate) dry_leaf_patch_instances: usize,
    pub(crate) physical_dry_leaf_count: usize,
}

#[derive(Default)]
pub(in crate::presentation) struct WoodyUnderstoryPresentation {
    branches: Option<Handle<Mesh>>,
    cambered_leaves: Option<Handle<Mesh>>,
    leaf_cards: Option<Handle<Mesh>>,
    bark: Option<Handle<StandardMaterial>>,
    leaves: Option<Handle<TacticalTreeLeafCardMaterial>>,
}

#[derive(Resource, Default)]
pub(in crate::presentation) struct WoodyUnderstoryPresentationCache {
    hazel: WoodyUnderstoryPresentation,
    blackthorn: WoodyUnderstoryPresentation,
    hawthorn: WoodyUnderstoryPresentation,
}

impl WoodyUnderstoryPresentationCache {
    fn presentation(&self, species: understory::UnderstorySpecies) -> &WoodyUnderstoryPresentation {
        match species {
            understory::UnderstorySpecies::CommonHazel => &self.hazel,
            understory::UnderstorySpecies::Blackthorn => &self.blackthorn,
            understory::UnderstorySpecies::CommonHawthorn => &self.hawthorn,
        }
    }
}

#[derive(Resource, Default)]
pub(in crate::presentation) struct GroundFoliagePresentationCache {
    forest_floor_leaves: Option<Handle<TacticalTreeLeafCardMaterial>>,
    dry_leaf_meshes: Option<Vec<Handle<Mesh>>>,
    twig_meshes: Option<Vec<Handle<Mesh>>>,
    woodland_plant_meshes: Option<Vec<Handle<Mesh>>>,
    twig_material: Option<Handle<StandardMaterial>>,
    woodland_plant_material: Option<Handle<TacticalFoliageMaterial>>,
}

pub(super) fn foliage_material(wind_scale: f32, ground_foliage: bool) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
        interaction: Vec4::ZERO,
        interaction_motion: Vec4::ZERO,
        // Root occlusion, reserved palette variation, normal up-bias, and
        // whether nearby player movement affects this material.
        shading: if ground_foliage {
            Vec4::new(0.52, 0.13, 0.76, 1.0)
        } else {
            Vec4::new(0.55, 0.08, 0.28, 0.0)
        },
        // Curved ribbon geometry, edge-on view thickening, authored lean, and
        // reserved future shaping control. Understory cards retain the older
        // crossed-plane deformation path.
        shape: Vec4::ZERO,
        ground_mask_transform: Vec4::ZERO,
        ground_mask: None,
    }
}

pub(super) fn update_grass_interaction(
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

pub(super) fn update_celestial_material_lighting(
    celestial: Res<PresentedCelestialLighting>,
    mut bark_materials: ResMut<Assets<TacticalTreeBarkMaterial>>,
    mut impostor_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut pebble_materials: ResMut<Assets<TacticalPebbleBillboardMaterial>>,
) {
    if !celestial.is_changed() {
        return;
    }
    let Some(celestial) = celestial.snapshot.as_ref() else {
        return;
    };
    let direction = if celestial.sun_altitude_degrees > -6.0 {
        celestial.sun_direction
    } else if celestial.moon_altitude_degrees > -2.0 {
        celestial.moon_direction
    } else {
        Vec3::new(0.25, 0.92, 0.3).normalize()
    };
    for (_, material) in bark_materials.iter_mut() {
        material.extension.lighting = direction.extend(celestial.material_light_factor);
    }
    for (_, material) in impostor_materials.iter_mut() {
        material.lighting = direction.extend(celestial.material_light_factor);
        material.ambient = celestial
            .ambient_color
            .extend(celestial.material_ambient_response);
    }
    for (_, material) in pebble_materials.iter_mut() {
        material.lighting = direction.extend(celestial.material_light_factor);
        material.ambient = celestial
            .ambient_color
            .extend(celestial.material_ambient_response);
    }
}

pub(super) fn spawn_ground_foliage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TacticalFoliageMaterial>,
    standard_materials: &mut Assets<StandardMaterial>,
    pebble_billboard_materials: &mut Assets<TacticalPebbleBillboardMaterial>,
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    understory_cache: &mut WoodyUnderstoryPresentationCache,
    ground_foliage_cache: &mut GroundFoliagePresentationCache,
    procedural_assets: &ProceduralEnvironmentAssets,
    images: &mut Assets<Image>,
    scene_id: &SceneId,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    environment: &SceneEnvironment,
) {
    let canopy = bps(environment.canopy_bps);
    let water = bps(environment.water_bps);
    let wetland = bps(environment.wetland_bps);
    let cultivation = bps(environment.cultivation_bps);
    let snow = bps(environment.weather.snow_cover_bps);
    // Mature open swards can exceed a thousand shoots per square metre, while
    // closed oak canopy suppresses grass well before it suppresses woody
    // understory. Keep the expensive new density in open terrain instead of
    // charging every woodland for meadow-level geometry beneath deep shade.
    let grass_density = grass_scatter_density(canopy, water, cultivation, snow);
    // Equal-area QHD benchmarks show that the full woody hazel/reed-like
    // specimen, rather than the trees themselves, dominates dense woodland
    // and wetland cost. Keep sparse woodland's established occupancy while
    // capping denser biomes near one plant in four lattice cells. This leaves
    // traversable openings and gives every terrain family a comparable GPU
    // budget without reducing the much cheaper canopy-tree population.
    let understory_chance = understory_scatter_chance(canopy, wetland, cultivation);
    let (grass_color, grass_dryness) = grass_pigment(environment);
    let grass_profile = GrassCommunityProfile::from_environment(environment);
    let grass_community_meshes = GrassCommunity::ALL.map(|community| {
        grass::CommunityMeshes::new(|lod, topology| {
            meshes.add(grass_patch_mesh(
                grass_color,
                lod,
                grass_density * topology.density(),
                community,
            ))
        })
    });
    ensure_understory_presentations(
        meshes,
        standard_materials,
        leaf_materials,
        understory_cache,
        procedural_assets,
    );
    let grass_wind_scale = 0.16 + bps(environment.weather.wind_speed_bps) * 0.36;
    let grass_mask = images.add(grass_cover_mask_image(
        ground,
        stable_text_seed(&environment.scene_digest),
    ));
    let grass_near_material = materials.add(grass_material(
        grass_wind_scale,
        GrassMeshLod::Near,
        grass_density,
        grass_dryness,
        grass_mask.clone(),
        ground,
    ));
    let grass_far_material = materials.add(grass_material(
        grass_wind_scale,
        GrassMeshLod::Far,
        grass_density,
        grass_dryness,
        grass_mask.clone(),
        ground,
    ));
    let grass_vista_material = materials.add(grass_material(
        grass_wind_scale,
        GrassMeshLod::Vista,
        grass_density,
        grass_dryness,
        grass_mask,
        ground,
    ));
    let dry_leaf_meshes = ground_foliage_cache
        .dry_leaf_meshes
        .get_or_insert_with(|| {
            (0..DRY_LEAF_MESH_VARIANTS)
                .map(|variant| meshes.add(dry_leaf_patch_mesh(variant)))
                .collect::<Vec<_>>()
        })
        .clone();
    let twig_meshes = ground_foliage_cache
        .twig_meshes
        .get_or_insert_with(|| {
            (0..TWIG_MESH_VARIANTS)
                .map(|variant| meshes.add(twig_patch_mesh(variant)))
                .collect::<Vec<_>>()
        })
        .clone();
    let woodland_plant_meshes = ground_foliage_cache
        .woodland_plant_meshes
        .get_or_insert_with(|| {
            (0..litter::WOODLAND_PLANT_MESH_VARIANTS)
                .map(|variant| meshes.add(litter::woodland_plant_patch_mesh(variant)))
                .collect::<Vec<_>>()
        })
        .clone();
    let dry_leaf_material = ground_foliage_cache
        .forest_floor_leaves
        .get_or_insert_with(|| leaf_materials.add(forest_floor_leaf_material(procedural_assets)))
        .clone();
    let twig_material = ground_foliage_cache
        .twig_material
        .get_or_insert_with(|| standard_materials.add(litter::static_twig_material()))
        .clone();
    let woodland_plant_material = ground_foliage_cache
        .woodland_plant_material
        .get_or_insert_with(|| materials.add(foliage_material(0.035, false)))
        .clone();
    let base_seed = stable_text_seed(&environment.scene_digest) ^ stable_text_seed(&scene_id.0);
    // Grass uses a macro patch whose internal blade spacing matches the old
    // one-metre patch. A roughly ten-times larger footprint therefore retains
    // density while cutting extraction, visibility, and instance entities by
    // an order of magnitude. Macro patches stay unit-scale and nearly gridded:
    // randomly shrinking/rotating the square footprint opened visible seams.
    // Aligning each patch to the sampled terrain normal keeps the shared plane
    // seated on slopes while its blades retain deterministic local variation.
    let grass_seed = stable_text_seed(&environment.scene_digest) ^ 0x6772_6173_735f_6c6f;
    grass::spawn(
        commands,
        terrain,
        ground,
        grass_seed,
        grass_profile,
        &grass::Assets {
            community_meshes: grass_community_meshes,
            near_material: grass_near_material,
            far_material: grass_far_material,
            vista_material: grass_vista_material,
        },
    );

    understory::spawn(
        commands,
        terrain,
        ground,
        understory_cache,
        base_seed,
        understory_chance,
        understory::UnderstoryHabitat {
            canopy,
            wetland,
            cultivation,
            moisture: bps(environment.weather.ground_moisture_bps),
        },
    );

    litter::spawn(
        commands,
        meshes,
        terrain,
        ground,
        base_seed,
        &litter::Assets {
            dry_leaf_meshes,
            twig_meshes,
            dry_leaf_material,
            twig_material,
            woodland_plant_meshes,
            woodland_plant_material,
        },
    );

    loose_stone::spawn(
        commands,
        meshes,
        standard_materials,
        pebble_billboard_materials,
        terrain,
        ground,
        base_seed,
    );
}

fn understory_scatter_chance(canopy: f32, wetland: f32, cultivation: f32) -> f32 {
    (canopy * 0.52 + wetland * 0.3 + cultivation * 0.08).clamp(0.0, 0.24)
}

fn grass_scatter_density(canopy: f32, water: f32, cultivation: f32, snow: f32) -> f32 {
    (0.98 - canopy * 0.95 - water * 0.88 + cultivation * 0.04).clamp(0.25, 0.98)
        * (1.0 - snow * 0.36)
}

pub(in crate::presentation) fn grass_pigment(environment: &SceneEnvironment) -> (Color, f32) {
    let grass_dryness = (1.0
        - bps(environment.weather.ground_moisture_bps) * 0.7
        - bps(environment.canopy_bps) * 1.2
        - bps(environment.wetland_bps) * 0.8
        - bps(environment.water_bps) * 0.8)
        .clamp(0.0, 1.0);
    let color = if environment.weather.snow_cover_bps >= 5_000 {
        Color::srgb_u8(155, 164, 137)
    } else if environment.cultivation_bps >= 4_000 {
        Color::srgb_u8(142, 133, 61)
    } else {
        // Blend pigment in sRGB authoring space from hydrated chlorophyll to
        // a dry olive sward. Fine-grained cohort variation remains in the
        // shader, but the biome-wide baseline honors replicated moisture and
        // shade instead of rendering dry grassland as woodland green.
        let hydrated = Vec3::new(82.0, 119.0, 45.0);
        let senescent = Vec3::new(150.0, 126.0, 52.0);
        let pigment = hydrated.lerp(senescent, grass_dryness * 0.78);
        Color::srgb_u8(pigment.x as u8, pigment.y as u8, pigment.z as u8)
    };
    (color, grass_dryness)
}

/// Solid-ground albedo that reproduces the *rendered* optical mass of the
/// procedural sward. Blade vertex pigments are subsequently darkened by the
/// species/cohort palette, root occlusion, rib occlusion, and thin-foliage
/// lighting, so copying their input pigment directly makes upward-facing
/// terrain much brighter than the grass it replaces.
pub(in crate::presentation) fn grass_terminal_pigment(environment: &SceneEnvironment) -> Color {
    let linear = grass_pigment(environment).0.to_linear().to_f32_array();
    Color::LinearRgba(LinearRgba::new(
        linear[0] * 0.22,
        linear[1] * 0.25,
        linear[2] * 0.05,
        1.0,
    ))
}

fn foliage_transform(
    terrain: &SceneTerrain,
    world_x: f32,
    world_z: f32,
    hash: u64,
) -> Option<Transform> {
    let sample = Vec2::new(world_x, world_z);
    let height = terrain.height_at(sample)?;
    let normal = terrain.normal_at(sample)?;
    if normal.y < 0.72 {
        return None;
    }
    let terrain_rotation = Quat::from_rotation_arc(Vec3::Y, normal);
    let yaw = Quat::from_rotation_y(unit_hash(hash) * core::f32::consts::TAU);
    let scale = 0.72 + unit_hash(splitmix64(hash ^ 0x8c0a_3c95)) * 0.58;
    Some(
        Transform::from_xyz(world_x, height, world_z)
            .with_rotation(terrain_rotation * yaw)
            .with_scale(Vec3::splat(scale)),
    )
}

pub(super) fn present_ground_scatter(
    scenes: Query<
        (
            Entity,
            &SceneId,
            &SceneTerrain,
            &SceneGround,
            &SceneEnvironment,
        ),
        Without<GroundScatterPresented>,
    >,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut foliage_materials: ResMut<Assets<TacticalFoliageMaterial>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut pebble_billboard_materials: ResMut<Assets<TacticalPebbleBillboardMaterial>>,
    mut leaf_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut understory_cache: ResMut<WoodyUnderstoryPresentationCache>,
    mut ground_foliage_cache: ResMut<GroundFoliagePresentationCache>,
    procedural_assets: Res<ProceduralEnvironmentAssets>,
) {
    for (entity, scene_id, terrain, ground, environment) in &scenes {
        let started = web_time::Instant::now();
        tracing::info!("Generating tactical ground scatter");
        spawn_ground_foliage(
            &mut commands,
            &mut meshes,
            &mut foliage_materials,
            &mut standard_materials,
            &mut pebble_billboard_materials,
            &mut leaf_materials,
            &mut understory_cache,
            &mut ground_foliage_cache,
            &procedural_assets,
            &mut images,
            scene_id,
            terrain,
            ground,
            environment,
        );
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis(),
            "Generated tactical ground scatter"
        );
        commands.entity(entity).insert(GroundScatterPresented);
    }
}

fn ensure_understory_presentations(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    cache: &mut WoodyUnderstoryPresentationCache,
    procedural_assets: &ProceduralEnvironmentAssets,
) {
    if cache.hazel.branches.is_some() {
        return;
    }
    // One deterministic specimen is shared by every scattered shrub. Instance
    // transforms still vary placement, rotation, and scale without generating
    // unique botanical geometry per occurrence.
    let species = [
        (
            &mut cache.hazel,
            0x00c0_a15a_2e11_u64,
            COMMON_HAZEL_PARAMETERS,
            COMMON_HAZEL_BARK,
            Color::srgb_u8(118, 104, 78),
            hazel_leaf_material(procedural_assets),
        ),
        (
            &mut cache.blackthorn,
            0x00b1_ac7a_0e31_u64,
            BLACKTHORN_PARAMETERS,
            BLACKTHORN_BARK,
            Color::srgb_u8(61, 52, 44),
            blackthorn_leaf_material(procedural_assets),
        ),
        (
            &mut cache.hawthorn,
            0x00a7_a74a_0e51_u64,
            COMMON_HAWTHORN_PARAMETERS,
            COMMON_HAWTHORN_BARK,
            Color::srgb_u8(91, 76, 60),
            hawthorn_leaf_material(procedural_assets),
        ),
    ];
    for (cache, seed, parameters, bark, bark_color, leaf_material) in species {
        let branches = procedural_woody_plant_skeleton(seed, 0.0, parameters);
        let leaves = procedural_woody_plant_leaves(seed, &branches, 0.0, parameters);
        cache.branches = Some(meshes.add(procedural_woody_branch_mesh(&branches, 3, bark)));
        cache.cambered_leaves = Some(meshes.add(procedural_woody_cambered_leaf_mesh(&leaves)));
        cache.leaf_cards = Some(meshes.add(procedural_woody_leaf_card_mesh(&leaves)));
        cache.bark = Some(materials.add(StandardMaterial {
            base_color: bark_color,
            perceptual_roughness: 0.96,
            ..default()
        }));
        cache.leaves = Some(leaf_materials.add(leaf_material));
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalFoliageMaterial {
    #[uniform(0)]
    wind: Vec4,
    #[uniform(0)]
    interaction: Vec4,
    #[uniform(0)]
    interaction_motion: Vec4,
    #[uniform(0)]
    shading: Vec4,
    #[uniform(0)]
    shape: Vec4,
    #[uniform(0)]
    ground_mask_transform: Vec4,
    #[texture(1)]
    #[sampler(2)]
    ground_mask: Option<Handle<Image>>,
}

impl Material for TacticalFoliageMaterial {
    fn vertex_shader() -> ShaderRef {
        FOLIAGE_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        FOLIAGE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::AlphaToCoverage
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
pub(crate) enum GroundScatterLayer {
    Grass,
    Understory,
    DryLeaves,
    Twigs,
    LooseStone,
}

#[derive(Component)]
pub(in crate::presentation) struct GroundScatterPresented;

/// Marks the locally controlled character whose movement bends nearby grass.
#[derive(Component)]
pub(crate) struct GrassInteractor;

#[derive(Resource, Default)]
pub(in crate::presentation) struct GrassInteractionState {
    previous_position: Option<Vec3>,
    smoothed_velocity: Vec3,
}

const FOLIAGE_SHADER: &str = "shaders/tactical_foliage.wgsl";

#[cfg(test)]
mod tests;
