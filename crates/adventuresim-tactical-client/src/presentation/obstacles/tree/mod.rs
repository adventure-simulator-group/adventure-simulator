pub(in crate::presentation) mod geometry;
pub(in crate::presentation) mod impostor;

pub(in crate::presentation) use geometry::*;
pub(in crate::presentation) use impostor::*;

use super::super::*;
use bevy::camera::primitives::Aabb;

#[derive(Resource, Default)]
pub(in crate::presentation) struct TreePresentationCache {
    pub(super) variants: std::collections::HashMap<u64, CachedTreePresentation>,
}

#[derive(Clone)]
pub(in crate::presentation) struct CachedTreePresentation {
    pub(super) trunk_mesh: Handle<Mesh>,
    pub(super) clusters: Vec<CachedTreeClusterPresentation>,
    pub(super) whole_tree_card_mesh: Handle<Mesh>,
    pub(super) bark_material: Handle<StandardMaterial>,
    pub(super) leaf_material: Handle<TacticalTreeLeafMaterial>,
    pub(super) bud_material: Handle<StandardMaterial>,
    pub(super) card_materials: [Handle<TacticalTreeImpostorMaterial>; 4],
    pub(super) provenance: [TreeImpostorProvenance; 4],
}

#[derive(Clone)]
pub(in crate::presentation) struct CachedTreeClusterPresentation {
    pub(super) primary_group: u8,
    pub(super) center: Vec3,
    pub(super) radius: f32,
    pub(super) branch_meshes: [Handle<Mesh>; 3],
    pub(super) leaf_mesh: Handle<Mesh>,
    pub(super) bud_mesh: Handle<Mesh>,
    pub(super) card_meshes: [Handle<Mesh>; 3],
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

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct TreeLodCluster {
    pub(crate) primary_group: u8,
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
}

#[derive(Component)]
pub(in crate::presentation) struct TreeTrunkLod;

#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct TreeLodRenderOverride(pub(crate) Option<u8>);

pub(in crate::presentation) fn spawn_cached_tree(
    commands: &mut Commands,
    entity: Entity,
    cached: &CachedTreePresentation,
) {
    commands.entity(entity).insert((
        Name::new("Presented mature English oak"),
        TreeTrunkLod,
        Mesh3d(cached.trunk_mesh.clone()),
        MeshMaterial3d(cached.bark_material.clone()),
        tree_trunk_visibility(),
    ));
    commands.entity(entity).with_children(|parent| {
        for cluster in &cached.clusters {
            let cluster_marker = TreeLodCluster {
                primary_group: cluster.primary_group,
                center: cluster.center,
                radius: cluster.radius,
            };
            let cluster_aabb = tree_cluster_aabb(cluster.center, cluster.radius);
            parent.spawn((
                Name::new(format!(
                    "English oak scaffold {} individual leaves",
                    cluster.primary_group
                )),
                TreeLod(0),
                cluster_marker,
                cluster_aabb,
                NotShadowCaster,
                Mesh3d(cluster.leaf_mesh.clone()),
                MeshMaterial3d(cached.leaf_material.clone()),
                tree_lod_visibility(0),
            ));
            parent.spawn((
                Name::new(format!(
                    "English oak scaffold {} terminal buds",
                    cluster.primary_group
                )),
                TreeLod(0),
                cluster_marker,
                cluster_aabb,
                Mesh3d(cluster.bud_mesh.clone()),
                MeshMaterial3d(cached.bud_material.clone()),
                tree_lod_visibility(0),
            ));
            parent.spawn((
                Name::new(format!(
                    "English oak scaffold {} detailed wood",
                    cluster.primary_group
                )),
                TreeLod(0),
                cluster_marker,
                cluster_aabb,
                Mesh3d(cluster.branch_meshes[0].clone()),
                MeshMaterial3d(cached.bark_material.clone()),
                tree_lod_visibility(0),
            ));
            for lod in 1..=3 {
                parent.spawn((
                    Name::new(format!(
                        "{} scaffold {}",
                        tree_lod_name(lod, true),
                        cluster.primary_group
                    )),
                    TreeLod(lod),
                    cluster_marker,
                    cluster_aabb,
                    NotShadowCaster,
                    Mesh3d(cluster.card_meshes[lod as usize - 1].clone()),
                    MeshMaterial3d(cached.card_materials[lod as usize - 1].clone()),
                    tree_lod_visibility(lod),
                ));
                if lod <= 2 {
                    parent.spawn((
                        Name::new(format!(
                            "{} scaffold {}",
                            tree_lod_name(lod, false),
                            cluster.primary_group
                        )),
                        TreeLod(lod),
                        cluster_marker,
                        cluster_aabb,
                        Mesh3d(cluster.branch_meshes[lod as usize].clone()),
                        MeshMaterial3d(cached.bark_material.clone()),
                        tree_lod_visibility(lod),
                    ));
                }
            }
        }
        parent.spawn((
            Name::new(tree_lod_name(4, true)),
            TreeLod(4),
            NotShadowCaster,
            Mesh3d(cached.whole_tree_card_mesh.clone()),
            MeshMaterial3d(cached.card_materials[3].clone()),
            cached.provenance[3].clone(),
            tree_lod_visibility(4),
        ));
        for lod in 1..=3 {
            parent.spawn((
                Name::new(format!("Tree LOD {lod} bake provenance")),
                cached.provenance[lod as usize - 1].clone(),
            ));
        }
    });
}

fn tree_cluster_aabb(center: Vec3, radius: f32) -> Aabb {
    let extent = Vec3::splat(radius.max(0.01));
    Aabb::from_min_max(center - extent, center + extent)
}

pub(in crate::presentation) fn update_tree_projected_lod_ranges(
    cameras: Query<(&Camera, &Projection), With<Camera3d>>,
    lod_override: Res<TreeLodRenderOverride>,
    mut lods: Query<(
        &TreeLod,
        Option<&TreeLodCluster>,
        &mut VisibilityRange,
        &mut Visibility,
    )>,
    mut trunks: Query<
        (&mut VisibilityRange, &mut Visibility),
        (With<TreeTrunkLod>, Without<TreeLod>),
    >,
) {
    let Ok((camera, projection)) = cameras.single() else {
        return;
    };
    let viewport_height = camera
        .physical_viewport_size()
        .map_or(720.0, |size| size.y as f32);
    let reference_focal = 720.0 / (2.0 * (80.0_f32.to_radians() * 0.5).tan());
    let focal = match projection {
        Projection::Perspective(perspective) => {
            viewport_height / (2.0 * (perspective.fov * 0.5).tan())
        }
        _ => reference_focal,
    };
    let focal_scale = (focal / reference_focal).clamp(0.25, 4.0);
    for (lod, cluster, mut range, mut visibility) in &mut lods {
        if let Some(forced_lod) = lod_override.0 {
            *visibility = if lod.0 == forced_lod {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            *range = VisibilityRange::abrupt(0.0, f32::MAX);
        } else {
            *visibility = Visibility::Inherited;
            let radius = cluster.map_or(3.5, |cluster| {
                debug_assert!(cluster.center.is_finite());
                debug_assert!(cluster.primary_group < TREE_PRIMARY_GROUP_COUNT);
                cluster.radius
            });
            *range = tree_projected_lod_visibility(lod.0, focal_scale, radius);
        }
    }
    for (mut range, mut visibility) in &mut trunks {
        if let Some(forced_lod) = lod_override.0 {
            *visibility = if forced_lod < 4 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            *range = VisibilityRange::abrupt(0.0, f32::MAX);
        } else {
            *visibility = Visibility::Inherited;
            let end = tree_projected_lod_visibility(3, focal_scale, 3.5).end_margin;
            *range = VisibilityRange {
                start_margin: 0.0..0.0,
                end_margin: end,
                use_aabb: true,
            };
        }
    }
}

const TREE_IMPOSTOR_SHADER: &str = "shaders/tactical_tree_impostor.wgsl";
const TREE_LEAF_SHADER: &str = "shaders/tactical_tree_leaf.wgsl";
