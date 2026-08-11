pub(in crate::presentation) mod geometry;
pub(in crate::presentation) mod impostor;

pub(in crate::presentation) use geometry::*;
pub(in crate::presentation) use impostor::*;

use super::super::*;

#[derive(Resource, Default)]
pub(in crate::presentation) struct TreePresentationCache {
    pub(super) variants: std::collections::HashMap<u64, CachedTreePresentation>,
}

#[derive(Clone)]
pub(in crate::presentation) struct CachedTreePresentation {
    pub(super) branch_meshes: [Handle<Mesh>; 4],
    pub(super) leaf_meshes: [Handle<Mesh>; OAK_LEAF_SECTOR_COUNT],
    pub(super) bud_mesh: Handle<Mesh>,
    pub(super) card_meshes: [Handle<Mesh>; 4],
    pub(super) bark_material: Handle<StandardMaterial>,
    pub(super) leaf_material: Handle<TacticalTreeLeafMaterial>,
    pub(super) bud_material: Handle<StandardMaterial>,
    pub(super) card_materials: [Handle<TacticalTreeImpostorMaterial>; 4],
    pub(super) provenance: [TreeImpostorProvenance; 4],
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalTreeLeafMaterial {
    /// Wind direction XZ, strength, and speed.
    #[uniform(0)]
    pub(super) parameters: Vec4,
}

impl Material for TacticalTreeLeafMaterial {
    fn vertex_shader() -> ShaderRef {
        TREE_LEAF_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        TREE_LEAF_SHADER.into()
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
pub(in crate::presentation) struct TacticalTreeImpostorMaterial {
    #[texture(0)]
    #[sampler(1)]
    baked_color: Handle<Image>,
    /// Representation level, deterministic seed, wind strength, wind speed.
    #[uniform(2)]
    parameters: Vec4,
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
pub(crate) struct TreeLod(pub(crate) u8);

pub(in crate::presentation) fn spawn_cached_tree(
    commands: &mut Commands,
    entity: Entity,
    cached: &CachedTreePresentation,
) {
    commands.entity(entity).insert((
        Name::new("Presented mature English oak"),
        TreeLod(0),
        Mesh3d(cached.branch_meshes[0].clone()),
        MeshMaterial3d(cached.bark_material.clone()),
        tree_lod_visibility(0),
    ));
    commands.entity(entity).with_children(|parent| {
        for (sector, mesh) in cached.leaf_meshes.iter().enumerate() {
            parent.spawn((
                Name::new(format!("English oak individual leaves sector {sector}")),
                TreeLod(0),
                NotShadowCaster,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(cached.leaf_material.clone()),
                tree_leaf_sector_visibility(sector),
            ));
        }
        parent.spawn((
            Name::new("English oak scaled terminal buds"),
            TreeLod(0),
            Mesh3d(cached.bud_mesh.clone()),
            MeshMaterial3d(cached.bud_material.clone()),
            tree_lod_visibility(0),
        ));
        for lod in 1..5 {
            parent.spawn((
                Name::new(tree_lod_name(lod, true)),
                TreeLod(lod),
                NotShadowCaster,
                Mesh3d(cached.card_meshes[lod as usize - 1].clone()),
                MeshMaterial3d(cached.card_materials[lod as usize - 1].clone()),
                cached.provenance[lod as usize - 1].clone(),
                tree_lod_visibility(lod),
            ));
            if (1..=3).contains(&lod) {
                parent.spawn((
                    Name::new(tree_lod_name(lod, false)),
                    TreeLod(lod),
                    Mesh3d(cached.branch_meshes[lod as usize].clone()),
                    MeshMaterial3d(cached.bark_material.clone()),
                    tree_lod_visibility(lod),
                ));
            }
        }
    });
}

const TREE_IMPOSTOR_SHADER: &str = "shaders/tactical_tree_impostor.wgsl";
const TREE_LEAF_SHADER: &str = "shaders/tactical_tree_leaf.wgsl";
