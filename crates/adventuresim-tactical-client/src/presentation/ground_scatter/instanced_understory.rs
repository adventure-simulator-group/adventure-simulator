//! GPU-instanced woody understory on `bevy_eidolon`.
//!
//! Nine batch entities (three species x branches/cambered leaves/alpha
//! cards) replace roughly three ECS entities per shrub. Placement reuses the
//! exact legacy walk (`understory::placements`), so thickets land where the
//! per-entity renderer put them; only the terrain-normal tilt is dropped -
//! instances carry yaw and uniform scale, and shrubs grow plumb like real
//! ones. The instanced path renders in the main opaque pass only, so shrubs
//! stop casting shadows; at their band distances the cast shadows were
//! already faint.

use std::sync::Arc;

use adventuresim_tactical_core::prelude::{SceneGround, SceneTerrain};
use bevy::{
    color::LinearRgba,
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use bevy_eidolon::prelude::*;

use crate::presentation::{splitmix64, unit_hash};

use super::{
    WoodyUnderstoryPresentationCache,
    instanced_grass::fitted_batch_aabb,
    understory::{self, ShrubPlacement, UnderstoryHabitat, UnderstorySpecies},
};

const SHRUB_BARK_SHADER: &str = "shaders/tactical_shrub_bark_instanced.wgsl";
const SHRUB_LEAF_SHADER: &str = "shaders/tactical_shrub_leaf_instanced.wgsl";

/// Widest shrub footprint in metres, for batch AABB headroom.
const SHRUB_FOOTPRINT_METRES: f32 = 4.5;

/// Simple lit bark for instanced shrub wood; the legacy path used a plain
/// `StandardMaterial`, so a colour + roughness uniform loses nothing.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(in crate::presentation) struct TacticalShrubBarkInstancedMaterial {
    /// Linear bark pigment; w is perceptual roughness (shading-only).
    #[uniform(0)]
    pub color: Vec4,
}

impl InstancedMaterial for TacticalShrubBarkInstancedMaterial {
    fn vertex_shader() -> ShaderRef {
        SHRUB_BARK_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHRUB_BARK_SHADER.into()
    }

    fn disable_prepass(&self) -> bool {
        true
    }
}

#[derive(Clone, Default, ShaderType)]
pub(in crate::presentation) struct TacticalShrubLeafInstancedUniform {
    parameters: Vec4,
    surface_parameters: Vec4,
    physical_parameters: Vec4,
    shading: Vec4,
}

/// Instanced twin of `TacticalTreeLeafCardMaterial` for shrub foliage. The
/// texture set and calibration uniforms are copied from the species' leaf
/// material; `shading.x` selects the fast lighting path used by the distant
/// alpha-card representation.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[uniform(12, TacticalShrubLeafInstancedUniform)]
pub(in crate::presentation) struct TacticalShrubLeafInstancedMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub opacity: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub front_albedo: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    pub back_albedo: Handle<Image>,
    #[texture(6)]
    #[sampler(7)]
    pub front_normal: Handle<Image>,
    #[texture(8)]
    #[sampler(9)]
    pub back_normal: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub arm: Handle<Image>,
    pub parameters: Vec4,
    pub surface_parameters: Vec4,
    pub physical_parameters: Vec4,
    /// x > 0.5 selects the fast shading path; y scales its ambient term.
    pub shading: Vec4,
}

impl From<&TacticalShrubLeafInstancedMaterial> for TacticalShrubLeafInstancedUniform {
    fn from(material: &TacticalShrubLeafInstancedMaterial) -> Self {
        Self {
            parameters: material.parameters,
            surface_parameters: material.surface_parameters,
            physical_parameters: material.physical_parameters,
            shading: material.shading,
        }
    }
}

impl InstancedMaterial for TacticalShrubLeafInstancedMaterial {
    fn vertex_shader() -> ShaderRef {
        SHRUB_LEAF_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHRUB_LEAF_SHADER.into()
    }

    fn disable_prepass(&self) -> bool {
        true
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: Self::Data,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Leaf cards are shaded double-sided and cut out via A2C.
        descriptor.primitive.cull_mode = None;
        if descriptor.multisample.count > 1 {
            descriptor.multisample.alpha_to_coverage_enabled = true;
        }
        Ok(())
    }
}

/// Fade bands per representation; adjacent cambered/card bands share their
/// endpoints so the complementary crossfade partition hands off exactly.
fn representation_band(representation: ShrubRepresentation) -> Vec4 {
    match representation {
        ShrubRepresentation::Branches => Vec4::new(0.0, 0.001, 44.0, 52.0),
        ShrubRepresentation::CamberedLeaves => Vec4::new(0.0, 0.001, 26.0, 34.0),
        ShrubRepresentation::LeafCards => Vec4::new(26.0, 34.0, 84.0, 96.0),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShrubRepresentation {
    Branches,
    CamberedLeaves,
    LeafCards,
}

const REPRESENTATIONS: [ShrubRepresentation; 3] = [
    ShrubRepresentation::Branches,
    ShrubRepresentation::CamberedLeaves,
    ShrubRepresentation::LeafCards,
];

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn(
    commands: &mut Commands,
    bark_materials: &mut Assets<TacticalShrubBarkInstancedMaterial>,
    leaf_materials: &mut Assets<TacticalShrubLeafInstancedMaterial>,
    standard_materials: &Assets<StandardMaterial>,
    leaf_sources: &Assets<super::TacticalTreeLeafCardMaterial>,
    cache: &WoodyUnderstoryPresentationCache,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    base_seed: u64,
    chance: f32,
    habitat: UnderstoryHabitat,
) {
    let mut per_species: [Vec<InstanceData>; 3] = Default::default();
    for ShrubPlacement {
        species,
        world_x,
        world_z,
        hash,
    } in understory::placements(terrain, ground, base_seed, chance, habitat)
    {
        // Mirrors `foliage_transform`'s yaw/scale hashes; the slope check and
        // terrain height also match it, so both renderers agree on sites.
        let sample = Vec2::new(world_x, world_z);
        let (Some(height), Some(normal)) = (terrain.height_at(sample), terrain.normal_at(sample))
        else {
            continue;
        };
        if normal.y < 0.72 {
            continue;
        }
        let yaw = unit_hash(hash) * core::f32::consts::TAU;
        let scale = 0.72 + unit_hash(splitmix64(hash ^ 0x8c0a_3c95)) * 0.58;
        per_species[species_index(species)].push(InstanceData {
            position: Vec3::new(world_x, height, world_z),
            scale,
            rotation: yaw,
            seed: (splitmix64(hash ^ 0x7368_7275_62) & 0xffff_ffff) as u32,
            ..Default::default()
        });
    }

    for species in [
        UnderstorySpecies::CommonHazel,
        UnderstorySpecies::Blackthorn,
        UnderstorySpecies::CommonHawthorn,
    ] {
        let instances = Arc::new(std::mem::take(&mut per_species[species_index(species)]));
        if instances.is_empty() {
            continue;
        }
        let presentation = cache.presentation(species);
        let (Some(branches), Some(cambered), Some(cards), Some(bark_handle), Some(leaf_handle)) = (
            presentation.branches.clone(),
            presentation.cambered_leaves.clone(),
            presentation.leaf_cards.clone(),
            presentation.bark.as_ref(),
            presentation.leaves.as_ref(),
        ) else {
            continue;
        };
        let bark_color = standard_materials
            .get(bark_handle)
            .map(|material| {
                let linear = material.base_color.to_linear();
                Vec4::new(
                    linear.red,
                    linear.green,
                    linear.blue,
                    material.perceptual_roughness,
                )
            })
            .unwrap_or(Vec4::new(0.16, 0.13, 0.10, 0.96));
        let Some(leaf) = leaf_sources.get(leaf_handle) else {
            continue;
        };
        let aabb = fitted_batch_aabb(&instances, SHRUB_FOOTPRINT_METRES);
        let bark = bark_materials.add(TacticalShrubBarkInstancedMaterial { color: bark_color });
        for representation in REPRESENTATIONS {
            let (mesh, name) = match representation {
                ShrubRepresentation::Branches => (branches.clone(), "wood"),
                ShrubRepresentation::CamberedLeaves => (cambered.clone(), "cambered leaves"),
                ShrubRepresentation::LeafCards => (cards.clone(), "leaf cards"),
            };
            let mut entity = commands.spawn((
                Name::new(format!(
                    "Instanced {species:?} shrub {name} ({})",
                    instances.len()
                )),
                super::GroundScatterLayer::Understory,
                GpuCullCompute,
                // Whole-scene batches: culling belongs to the GPU compute
                // pass, and CPU frustum culling would free the retained
                // instance buffers (see the grass batches).
                bevy::camera::visibility::NoFrustumCulling,
                Mesh3d(mesh),
                aabb,
                InstanceMaterialData {
                    instances: instances.clone(),
                    color: LinearRgba::WHITE,
                    visibility_range: representation_band(representation),
                },
                Transform::default(),
                Visibility::Inherited,
            ));
            match representation {
                ShrubRepresentation::Branches => {
                    entity.insert(InstancedMeshMaterial(bark.clone()));
                }
                ShrubRepresentation::CamberedLeaves | ShrubRepresentation::LeafCards => {
                    // Both the cambered near leaves and the distant cards shade
                    // fast now; the cambered path's former full PBR was pure
                    // per-fragment cost. y=0.72 scales the flat ambient.
                    entity.insert(InstancedMeshMaterial(leaf_materials.add(
                        TacticalShrubLeafInstancedMaterial {
                            opacity: leaf.opacity.clone(),
                            front_albedo: leaf.front_albedo.clone(),
                            back_albedo: leaf.back_albedo.clone(),
                            front_normal: leaf.front_normal.clone(),
                            back_normal: leaf.back_normal.clone(),
                            arm: leaf.arm.clone(),
                            parameters: leaf.parameters,
                            surface_parameters: leaf.surface_parameters,
                            physical_parameters: leaf.physical_parameters,
                            shading: Vec4::new(1.0, 0.72, 0.0, 0.0),
                        },
                    )));
                }
            }
        }
    }
}

fn species_index(species: UnderstorySpecies) -> usize {
    match species {
        UnderstorySpecies::CommonHazel => 0,
        UnderstorySpecies::Blackthorn => 1,
        UnderstorySpecies::CommonHawthorn => 2,
    }
}
