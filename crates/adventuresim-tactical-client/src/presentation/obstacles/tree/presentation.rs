use super::impostor::{
    TreeImpostorProvenance, bake_tree_lod, tree_impostor_material, tree_leaf_visibility,
    tree_lod_name, tree_lod_visibility, tree_trunk_visibility, validate_tree_bake_provenance,
};
use super::{
    TREE_PRIMARY_GROUP_COUNT, TacticalTreeImpostorMaterial, TacticalTreeLeafCardMaterial,
    TreeLeafRepresentation, TreeLod, TreeLodCluster, TreeLodRenderOverride, TreeTrunkLod,
    oak_bark_material, oak_leaf_material, procedural_oak_bud_group_mesh,
    procedural_oak_leaf_card_group_mesh, procedural_oak_leaves,
    procedural_oak_textured_leaf_group_mesh, procedural_tree_branch_group_mesh,
    procedural_tree_branch_mesh, procedural_tree_skeleton,
};
use crate::presentation::{ActiveTacticalScene, SceneEnvironment, obstacle_seed, splitmix64};
use bevy::{
    camera::{primitives::Aabb, visibility::NoFrustumCulling},
    light::NotShadowCaster,
    prelude::*,
};

#[derive(Component)]
pub(in crate::presentation) struct PendingTreePresentation;

#[derive(Resource, Default)]
pub(in crate::presentation) struct TreePresentationCache {
    variants: std::collections::HashMap<u64, CachedTreePresentation>,
    oak_bark_material: Option<Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
pub(in crate::presentation) struct VistaTreePresentationCache {
    variants: std::collections::HashMap<u64, CachedVistaTreePresentation>,
}

#[derive(Clone)]
pub(in crate::presentation) struct CachedVistaTreePresentation {
    pub(in crate::presentation) mesh: Handle<Mesh>,
    pub(in crate::presentation) material: Handle<TacticalTreeImpostorMaterial>,
    pub(in crate::presentation) provenance: TreeImpostorProvenance,
}

#[derive(Clone)]
struct CachedTreePresentation {
    trunk_mesh: Handle<Mesh>,
    clusters: Vec<CachedTreeClusterPresentation>,
    aggregate_branch_meshes: [Handle<Mesh>; 2],
    aggregate_card_meshes: [Handle<Mesh>; 3],
    whole_tree_card_mesh: Handle<Mesh>,
    bark_material: Handle<StandardMaterial>,
    leaf_material: Handle<TacticalTreeLeafCardMaterial>,
    bud_material: Handle<StandardMaterial>,
    card_materials: [Handle<TacticalTreeImpostorMaterial>; 4],
    provenance: [TreeImpostorProvenance; 4],
}

#[derive(Clone)]
struct CachedTreeClusterPresentation {
    primary_group: u8,
    center: Vec3,
    radius: f32,
    detailed_branch_mesh: Handle<Mesh>,
    cambered_leaf_mesh: Handle<Mesh>,
    leaf_card_mesh: Handle<Mesh>,
    bud_mesh: Handle<Mesh>,
}

/// Root marker for a fully presented playable tree.
///
/// Capture benchmarks use this boundary to attribute tree cost without
/// depending on display names or reaching into the tree's child layout.
#[derive(Component)]
pub(crate) struct PresentedTree;

#[derive(Component, Clone)]
pub(in crate::presentation) struct StreamedTreePresentation {
    cached: CachedTreePresentation,
    active_mask: u8,
    active_leaf: Option<TreeLeafRepresentation>,
}

#[derive(Component)]
pub(in crate::presentation) struct StreamedTreeChild;

fn spawn_cached_tree(commands: &mut Commands, entity: Entity, cached: &CachedTreePresentation) {
    commands.entity(entity).insert((
        Name::new("Presented mature English oak"),
        PresentedTree,
        Visibility::Inherited,
        StreamedTreePresentation {
            cached: cached.clone(),
            active_mask: u8::MAX,
            active_leaf: None,
        },
    ));
}

fn spawn_streamed_tree_children(
    commands: &mut Commands,
    entity: Entity,
    cached: &CachedTreePresentation,
    mask: u8,
    selected_leaf: Option<TreeLeafRepresentation>,
) {
    commands.entity(entity).with_children(|parent| {
        // Keep renderable trunk state below the obstacle root. Hiding the trunk
        // at whole-tree LOD must not hide the camera-facing billboard sibling
        // through inherited parent visibility.
        if mask & 1 != 0 {
            parent.spawn((
                Name::new("English oak trunk"),
                StreamedTreeChild,
                TreeTrunkLod,
                Mesh3d(cached.trunk_mesh.clone()),
                MeshMaterial3d(cached.bark_material.clone()),
                tree_trunk_visibility(),
            ));
        }
        if mask & (1 << 1) != 0 {
            for cluster in &cached.clusters {
                let cluster_marker = TreeLodCluster {
                    primary_group: cluster.primary_group,
                    center: cluster.center,
                    radius: cluster.radius,
                };
                let cluster_aabb = tree_cluster_aabb(cluster.center, cluster.radius);
                if selected_leaf != Some(TreeLeafRepresentation::AlphaCard) {
                    parent.spawn((
                        Name::new(format!(
                            "English oak scaffold {} cambered PBR leaves",
                            cluster.primary_group
                        )),
                        StreamedTreeChild,
                        TreeLod(0),
                        cluster_marker,
                        cluster_aabb,
                        TreeLeafRepresentation::TexturedMesh,
                        Mesh3d(cluster.cambered_leaf_mesh.clone()),
                        MeshMaterial3d(cached.leaf_material.clone()),
                        tree_leaf_visibility(
                            TreeLeafRepresentation::TexturedMesh,
                            1.0,
                            cluster.radius,
                        ),
                    ));
                }
                if selected_leaf != Some(TreeLeafRepresentation::TexturedMesh) {
                    parent.spawn((
                        Name::new(format!(
                            "English oak scaffold {} PBR leaf cards",
                            cluster.primary_group
                        )),
                        StreamedTreeChild,
                        TreeLod(0),
                        cluster_marker,
                        cluster_aabb,
                        TreeLeafRepresentation::AlphaCard,
                        Mesh3d(cluster.leaf_card_mesh.clone()),
                        MeshMaterial3d(cached.leaf_material.clone()),
                        tree_leaf_visibility(
                            TreeLeafRepresentation::AlphaCard,
                            1.0,
                            cluster.radius,
                        ),
                    ));
                }
                parent.spawn((
                    Name::new(format!(
                        "English oak scaffold {} terminal buds",
                        cluster.primary_group
                    )),
                    StreamedTreeChild,
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
                    StreamedTreeChild,
                    TreeLod(0),
                    cluster_marker,
                    cluster_aabb,
                    Mesh3d(cluster.detailed_branch_mesh.clone()),
                    MeshMaterial3d(cached.bark_material.clone()),
                    tree_lod_visibility(0),
                ));
            }
        }
        for lod in 1..=3 {
            if mask & (1 << (lod + 1)) == 0 {
                continue;
            }
            parent.spawn((
                Name::new(tree_lod_name(lod, true)),
                StreamedTreeChild,
                TreeLod(lod),
                NotShadowCaster,
                Mesh3d(cached.aggregate_card_meshes[lod as usize - 1].clone()),
                MeshMaterial3d(cached.card_materials[lod as usize - 1].clone()),
                tree_lod_visibility(lod),
            ));
            if lod <= 2 {
                parent.spawn((
                    Name::new(tree_lod_name(lod, false)),
                    StreamedTreeChild,
                    TreeLod(lod),
                    Mesh3d(cached.aggregate_branch_meshes[lod as usize - 1].clone()),
                    MeshMaterial3d(cached.bark_material.clone()),
                    tree_lod_visibility(lod),
                ));
            }
        }
        if mask & (1 << 5) != 0 {
            parent.spawn((
                Name::new(tree_lod_name(4, true)),
                StreamedTreeChild,
                TreeLod(4),
                NoFrustumCulling,
                NotShadowCaster,
                Mesh3d(cached.whole_tree_card_mesh.clone()),
                MeshMaterial3d(cached.card_materials[3].clone()),
                cached.provenance[3].clone(),
                tree_lod_visibility(4),
            ));
        }
    });
}

pub(in crate::presentation) fn stream_tree_lod_children(
    mut commands: Commands,
    camera: Single<(&GlobalTransform, &Projection), With<Camera3d>>,
    lod_override: Res<TreeLodRenderOverride>,
    mut trees: Query<(
        Entity,
        &GlobalTransform,
        Option<&Children>,
        &mut StreamedTreePresentation,
    )>,
    streamed_children: Query<(), With<StreamedTreeChild>>,
) {
    let focal_scale = match camera.1 {
        Projection::Perspective(projection) => {
            (80.0_f32.to_radians() * 0.5).tan() / (projection.fov * 0.5).tan()
        }
        _ => 1.0,
    } * lod_override.projected_scale.unwrap_or(1.0);
    for (entity, transform, children, mut presentation) in &mut trees {
        let distance = camera.0.translation().distance(transform.translation());
        let mask = if let Some(lod) = lod_override.lod {
            (1 << (lod + 1)) | u8::from(lod < 4)
        } else {
            let scaled = distance / focal_scale.max(0.25);
            u8::from(scaled < 60.0)
                | (u8::from(scaled < 1.5) << 1)
                | (u8::from((1.0..2.5).contains(&scaled)) << 2)
                | (u8::from((1.5..4.0).contains(&scaled)) << 3)
                | (u8::from((2.5..60.0).contains(&scaled)) << 4)
                | (u8::from((50.0..200.0).contains(&scaled)) << 5)
        };
        if presentation.active_mask == mask && presentation.active_leaf == lod_override.leaf {
            continue;
        }
        if let Some(children) = children {
            for child in children.iter() {
                if streamed_children.contains(child) {
                    commands.entity(child).despawn();
                }
            }
        }
        spawn_streamed_tree_children(
            &mut commands,
            entity,
            &presentation.cached,
            mask,
            lod_override.leaf,
        );
        presentation.active_mask = mask;
        presentation.active_leaf = lod_override.leaf;
    }
}

fn tree_cluster_aabb(center: Vec3, radius: f32) -> Aabb {
    let extent = Vec3::splat(radius.max(0.01));
    Aabb::from_min_max(center - extent, center + extent)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn ensure_vista_tree_variant(
    variant_seed: u64,
    competition: f32,
    meshes: &mut Assets<Mesh>,
    tree_materials: &mut Assets<TacticalTreeImpostorMaterial>,
    images: &mut Assets<Image>,
    cache: &mut VistaTreePresentationCache,
) -> CachedVistaTreePresentation {
    let competition_key = (competition * 4095.0).round() as u64;
    let cache_key = variant_seed ^ competition_key.rotate_left(32);
    if let Some(cached) = cache.variants.get(&cache_key) {
        return cached.clone();
    }
    let branches = procedural_tree_skeleton(variant_seed, competition);
    let leaves = procedural_oak_leaves(variant_seed, &branches, competition);
    let bake = bake_tree_lod(variant_seed, &branches, &leaves, 4);
    validate_tree_bake_provenance(&bake.provenance);
    let texture = images.add(bake.image.clone());
    let cached = CachedVistaTreePresentation {
        mesh: meshes.add(bake.mesh),
        material: tree_materials.add(tree_impostor_material(variant_seed, 4, texture)),
        provenance: bake.provenance,
    };
    cache.variants.insert(cache_key, cached.clone());
    cached
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn present_pending_trees(
    mut commands: Commands,
    pending: Query<(Entity, &Transform), With<PendingTreePresentation>>,
    active: Res<ActiveTacticalScene>,
    environments: Query<&SceneEnvironment>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut leaf_card_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut tree_cache: ResMut<TreePresentationCache>,
    asset_server: Res<AssetServer>,
) {
    let Some(environment) = active
        .entity
        .and_then(|entity| environments.get(entity).ok())
    else {
        return;
    };
    let competition = canopy_competition(environment.canopy_bps);
    for (entity, transform) in &pending {
        let seed = obstacle_seed(transform.translation);
        let variant_seed = splitmix64(0x6f61_6b00 ^ (seed & 3));
        let competition_key = (competition * 4095.0).round() as u64;
        let cache_key = variant_seed ^ competition_key.rotate_left(32);
        let cached = if let Some(cached) = tree_cache.variants.get(&cache_key) {
            cached.clone()
        } else {
            let branches = procedural_tree_skeleton(variant_seed, competition);
            let leaves = procedural_oak_leaves(variant_seed, &branches, competition);
            let bark_material = tree_cache.oak_bark_material.clone().unwrap_or_else(|| {
                let material = materials.add(oak_bark_material(&asset_server));
                tree_cache.oak_bark_material = Some(material.clone());
                material
            });
            let leaf_material = leaf_card_materials.add(oak_leaf_material(&asset_server));
            let bud_material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.36, 0.27, 0.1),
                perceptual_roughness: 0.92,
                ..default()
            });
            let baked_lods = (1..5)
                .map(|lod| bake_tree_lod(variant_seed, &branches, &leaves, lod))
                .collect::<Vec<_>>();
            for bake in &baked_lods {
                validate_tree_bake_provenance(&bake.provenance);
            }
            let clusters = (0..TREE_PRIMARY_GROUP_COUNT)
                .map(|primary_group| {
                    let source = baked_lods[0]
                        .clusters
                        .iter()
                        .find(|cluster| cluster.primary_group == primary_group)
                        .expect("every generated primary scaffold has a baked cluster");
                    CachedTreeClusterPresentation {
                        primary_group,
                        center: source.center,
                        radius: source.radius,
                        detailed_branch_mesh: meshes.add(procedural_tree_branch_group_mesh(
                            &branches,
                            3,
                            primary_group,
                        )),
                        cambered_leaf_mesh: meshes.add(procedural_oak_textured_leaf_group_mesh(
                            &leaves,
                            primary_group,
                        )),
                        leaf_card_mesh: meshes
                            .add(procedural_oak_leaf_card_group_mesh(&leaves, primary_group)),
                        bud_mesh: meshes
                            .add(procedural_oak_bud_group_mesh(&branches, primary_group)),
                    }
                })
                .collect();
            let cached = CachedTreePresentation {
                trunk_mesh: meshes.add(procedural_tree_branch_mesh(&branches, 0)),
                clusters,
                aggregate_branch_meshes: [2, 1]
                    .map(|depth| meshes.add(procedural_tree_branch_mesh(&branches, depth))),
                aggregate_card_meshes: core::array::from_fn(|index| {
                    meshes.add(baked_lods[index].mesh.clone())
                }),
                whole_tree_card_mesh: meshes.add(baked_lods[3].mesh.clone()),
                bark_material,
                leaf_material,
                bud_material,
                card_materials: core::array::from_fn(|index| {
                    let bake = &baked_lods[index];
                    let texture = images.add(bake.image.clone());
                    tree_materials.add(tree_impostor_material(variant_seed, bake.lod, texture))
                }),
                provenance: core::array::from_fn(|index| baked_lods[index].provenance.clone()),
            };
            tree_cache.variants.insert(cache_key, cached.clone());
            cached
        };
        spawn_cached_tree(&mut commands, entity, &cached);
        commands.entity(entity).remove::<PendingTreePresentation>();
    }
}

pub(in crate::presentation) fn canopy_competition(canopy_bps: u16) -> f32 {
    let normalized = f32::from(canopy_bps) / 10_000.0;
    normalized * normalized * (3.0 - 2.0 * normalized)
}
