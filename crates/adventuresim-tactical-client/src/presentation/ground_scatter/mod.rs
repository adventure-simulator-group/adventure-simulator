use adventuresim_tactical_core::prelude::{SceneEnvironment, SceneGround, SceneId, SceneTerrain};
use bevy::{
    ecs::change_detection::DetectChanges,
    pbr::Material,
    prelude::{
        Asset, AssetServer, Assets, Color, Commands, Component, Entity, GlobalTransform, Handle,
        Image, Mesh, Quat, Query, Reflect, Res, ResMut, Resource, StandardMaterial, Time,
        Transform, Vec2, Vec3, Vec4, With, Without, default,
    },
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

use super::obstacles::tree::{
    COMMON_HAZEL_PARAMETERS, TacticalTreeImpostorMaterial, TacticalTreeLeafCardMaterial,
    TreeLeafRepresentation, hazel_leaf_material, procedural_tree_branch_mesh,
    procedural_woody_cambered_leaf_mesh, procedural_woody_leaf_card_mesh,
    procedural_woody_plant_leaves, procedural_woody_plant_skeleton,
};
use super::{
    PresentedCelestialLighting, bps, grass_cover_mask_image, splitmix64, stable_text_seed,
    unit_hash,
};

// Ground-scatter orchestration and shared presentation contracts.

mod grass;
mod litter;
mod loose_stone;
mod understory;

use grass::{GrassMeshLod, grass_material, grass_patch_mesh};
use litter::{
    DRY_LEAF_MESH_VARIANTS, TWIG_MESH_VARIANTS, dry_leaf_patch_mesh, forest_floor_leaf_material,
    twig_patch_mesh,
};

#[derive(Resource, Default)]
pub(in crate::presentation) struct HazelPresentationCache {
    branches: Option<Handle<Mesh>>,
    cambered_leaves: Option<Handle<Mesh>>,
    leaf_cards: Option<Handle<Mesh>>,
    bark: Option<Handle<StandardMaterial>>,
    leaves: Option<Handle<TacticalTreeLeafCardMaterial>>,
}

#[derive(Resource, Default)]
pub(in crate::presentation) struct GroundFoliagePresentationCache {
    forest_floor_leaves: Option<Handle<TacticalTreeLeafCardMaterial>>,
    dry_leaf_meshes: Option<Vec<Handle<Mesh>>>,
    twig_meshes: Option<Vec<Handle<Mesh>>>,
    twig_material: Option<Handle<TacticalFoliageMaterial>>,
}

pub(super) fn foliage_material(wind_scale: f32, ground_foliage: bool) -> TacticalFoliageMaterial {
    TacticalFoliageMaterial {
        wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
        interaction: Vec4::ZERO,
        interaction_motion: Vec4::ZERO,
        // Root brightness, meadow colour variation, normal up-bias, and
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
    mut impostor_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
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
    for (_, material) in impostor_materials.iter_mut() {
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
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    hazel_cache: &mut HazelPresentationCache,
    ground_foliage_cache: &mut GroundFoliagePresentationCache,
    asset_server: &AssetServer,
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
    let grass_density = (0.96 - canopy * 0.16 - water * 0.88 + cultivation * 0.04)
        .clamp(0.06, 0.98)
        * (1.0 - snow * 0.36);
    // A mature shared hazel occupies far more space than the former crossed
    // card placeholder. Scatter on a wider lattice so woodland contains
    // legible individual shrubs and traversable openings instead of a wall of
    // overlapping coppice stems.
    let understory_chance = (canopy * 0.52 + wetland * 0.3).clamp(0.0, 0.52);
    let grass_dryness = (1.0
        - bps(environment.weather.ground_moisture_bps) * 0.7
        - canopy * 1.2
        - wetland * 0.8
        - water * 0.8)
        .clamp(0.0, 1.0);
    let grass_color = if environment.weather.snow_cover_bps >= 5_000 {
        Color::srgb_u8(155, 164, 137)
    } else if environment.cultivation_bps >= 4_000 {
        Color::srgb_u8(142, 133, 61)
    } else {
        // Blend pigment in sRGB authoring space from hydrated chlorophyll to
        // a dry olive sward. Fine-grained cohort variation remains in the
        // shader, but the biome-wide baseline now honors replicated moisture
        // and shade instead of rendering dry grassland as woodland green.
        let hydrated = Vec3::new(82.0, 119.0, 45.0);
        let senescent = Vec3::new(150.0, 126.0, 52.0);
        let pigment = hydrated.lerp(senescent, grass_dryness * 0.78);
        Color::srgb_u8(pigment.x as u8, pigment.y as u8, pigment.z as u8)
    };
    let grass_near_mesh = meshes.add(grass_patch_mesh(
        grass_color,
        GrassMeshLod::Near,
        grass_density,
    ));
    let grass_far_mesh = meshes.add(grass_patch_mesh(
        grass_color,
        GrassMeshLod::Far,
        grass_density,
    ));
    ensure_hazel_presentation(
        meshes,
        standard_materials,
        leaf_materials,
        hazel_cache,
        asset_server,
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
    let dry_leaf_material = ground_foliage_cache
        .forest_floor_leaves
        .get_or_insert_with(|| leaf_materials.add(forest_floor_leaf_material(asset_server)))
        .clone();
    let twig_material = ground_foliage_cache
        .twig_material
        .get_or_insert_with(|| materials.add(foliage_material(0.0, false)))
        .clone();
    let base_seed = stable_text_seed(&environment.scene_digest) ^ stable_text_seed(&scene_id.0);
    // Grass uses a macro patch whose internal blade spacing matches the old
    // one-metre patch. A roughly ten-times larger footprint therefore retains
    // density while cutting extraction, visibility, and instance entities by
    // an order of magnitude. Macro patches stay unit-scale and nearly gridded:
    // randomly shrinking/rotating the square footprint opened visible seams.
    // Aligning each patch to the sampled terrain normal keeps the shared plane
    // seated on slopes while its blades retain deterministic local variation.
    grass::spawn(
        commands,
        terrain,
        ground,
        base_seed,
        &grass::Assets {
            near_mesh: grass_near_mesh,
            far_mesh: grass_far_mesh,
            near_material: grass_near_material,
            far_material: grass_far_material,
        },
    );

    understory::spawn(
        commands,
        terrain,
        ground,
        hazel_cache,
        base_seed,
        understory_chance,
    );

    litter::spawn(
        commands,
        terrain,
        ground,
        base_seed,
        &litter::Assets {
            dry_leaf_meshes,
            twig_meshes,
            dry_leaf_material,
            twig_material,
        },
    );

    loose_stone::spawn(
        commands,
        meshes,
        standard_materials,
        terrain,
        ground,
        base_seed,
    );
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
    mut leaf_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut hazel_cache: ResMut<HazelPresentationCache>,
    mut ground_foliage_cache: ResMut<GroundFoliagePresentationCache>,
    asset_server: Res<AssetServer>,
) {
    for (entity, scene_id, terrain, ground, environment) in &scenes {
        spawn_ground_foliage(
            &mut commands,
            &mut meshes,
            &mut foliage_materials,
            &mut standard_materials,
            &mut leaf_materials,
            &mut hazel_cache,
            &mut ground_foliage_cache,
            &asset_server,
            &mut images,
            scene_id,
            terrain,
            ground,
            environment,
        );
        commands.entity(entity).insert(GroundScatterPresented);
    }
}

fn ensure_hazel_presentation(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    cache: &mut HazelPresentationCache,
    asset_server: &AssetServer,
) {
    if cache.branches.is_some() {
        return;
    }
    // One deterministic specimen is shared by every scattered shrub. Instance
    // transforms still vary placement, rotation, and scale without generating
    // unique botanical geometry per occurrence.
    let seed = 0xc0a1_5a2e_11_u64;
    let branches = procedural_woody_plant_skeleton(seed, 0.0, COMMON_HAZEL_PARAMETERS);
    let leaves = procedural_woody_plant_leaves(seed, &branches, 0.0, COMMON_HAZEL_PARAMETERS);
    cache.branches = Some(meshes.add(procedural_tree_branch_mesh(&branches, 3)));
    cache.cambered_leaves = Some(meshes.add(procedural_woody_cambered_leaf_mesh(&leaves)));
    cache.leaf_cards = Some(meshes.add(procedural_woody_leaf_card_mesh(&leaves)));
    cache.bark = Some(materials.add(StandardMaterial {
        base_color: Color::srgb_u8(118, 104, 78),
        perceptual_roughness: 0.96,
        ..default()
    }));
    cache.leaves = Some(leaf_materials.add(hazel_leaf_material(asset_server)));
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
