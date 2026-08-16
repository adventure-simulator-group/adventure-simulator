use super::geometry::BarkRecipe;
use super::impostor::{
    BEECH_TREE_BAKE_STYLE, OAK_TREE_BAKE_STYLE, TreeBakeStyle, TreeImpostorProvenance, TreeLodBake,
    bake_tree_lod, bake_tree_lod_with_style, tree_impostor_material, tree_leaf_visibility,
    tree_lod_name, tree_lod_visibility, tree_trunk_visibility, validate_tree_bake_provenance,
};
use super::{
    COMMON_BEECH_BARK, COMMON_BEECH_PARAMETERS, ENGLISH_OAK_BARK, OAK_GNARLING_SHOWCASE,
    OakGnarlingParameters, TacticalTreeImpostorMaterial, TacticalTreeLeafCardMaterial,
    TreeLeafRepresentation, TreeLod, TreeLodCluster, TreeLodRenderOverride, TreeTrunkLod,
    beech_leaf_material, oak_bark_material, oak_leaf_material, procedural_oak_bud_group_mesh,
    procedural_oak_leaf_card_group_mesh, procedural_oak_leaves,
    procedural_oak_skeleton_with_gnarling, procedural_oak_textured_leaf_group_mesh,
    procedural_tree_branch_group_mesh, procedural_tree_branch_mesh, procedural_tree_skeleton,
    procedural_woody_branch_mesh, procedural_woody_crown_mesh, procedural_woody_plant_leaves,
    procedural_woody_plant_skeleton,
};
use crate::presentation::{
    ActiveTacticalScene, ProceduralEnvironmentAssets, SceneEnvironment, obstacle_seed, splitmix64,
    unit_hash,
};
use bevy::{
    camera::{primitives::Aabb, visibility::NoFrustumCulling},
    light::NotShadowCaster,
    prelude::*,
};

#[cfg(test)]
use super::TREE_PRIMARY_GROUP_COUNT;

#[derive(Component)]
pub(in crate::presentation) struct PendingTreePresentation;

/// Exact submitted triangle count for one streamed leaf-cluster entity.
/// Stored when the CPU mesh is built because production meshes intentionally
/// relinquish their main-world vertex data after render extraction.
#[derive(Component, Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) struct TreeLeafTriangleCount(pub(crate) usize);

#[derive(Resource, Default)]
pub(in crate::presentation) struct TreePresentationCache {
    variants: std::collections::HashMap<u64, CachedTreePresentation>,
    oak_bark_material: Option<Handle<StandardMaterial>>,
    beech_bark_material: Option<Handle<StandardMaterial>>,
}

/// Live accounting for procedurally generated playable-tree render assets.
///
/// Unlike `Assets` totals, these counters separate the representations that
/// the tree streamer requested. The scene benchmark records this resource so
/// a faster frame cannot hide an accidental increase in resident geometry.
#[derive(Resource, Debug, Clone, Default, serde::Serialize)]
pub(crate) struct TreeAssetResidencyDiagnostics {
    pub(crate) variants: usize,
    pub(crate) source_branches: usize,
    pub(crate) source_leaves: usize,
    pub(crate) trunk_vertices: usize,
    pub(crate) detailed_branch_vertices: usize,
    pub(crate) cambered_leaf_vertices: usize,
    pub(crate) leaf_card_vertices: usize,
    pub(crate) bud_vertices: usize,
    pub(crate) aggregate_branch_vertices: usize,
    pub(crate) impostor_vertices: usize,
    pub(crate) impostor_texture_bytes: usize,
    pub(crate) generated_lod_mask: u8,
    pub(crate) generation_milliseconds: u128,
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

struct CachedTreePresentation {
    variant_seed: u64,
    species: TreePresentationSpecies,
    species_name: &'static str,
    branches: Vec<super::TreeBranchSegment>,
    leaves: Vec<super::TreeLeaf>,
    trunk_mesh: Option<Handle<Mesh>>,
    cluster_layout: Option<Vec<(u8, Vec3, f32)>>,
    clusters: Option<Vec<CachedTreeClusterPresentation>>,
    aggregate_branch_meshes: [Option<Handle<Mesh>>; 2],
    lod_cards: [Option<CachedTreeCardPresentation>; 4],
    bark_material: Handle<StandardMaterial>,
    leaf_material: Handle<TacticalTreeLeafCardMaterial>,
    bud_material: Handle<StandardMaterial>,
}

struct CachedTreeClusterPresentation {
    primary_group: u8,
    center: Vec3,
    radius: f32,
    detailed_branch_mesh: Option<Handle<Mesh>>,
    cambered_leaf_mesh: Option<Handle<Mesh>>,
    leaf_card_mesh: Option<Handle<Mesh>>,
    bud_mesh: Option<Handle<Mesh>>,
}

struct CachedTreeCardPresentation {
    mesh: Handle<Mesh>,
    material: Handle<TacticalTreeImpostorMaterial>,
    provenance: TreeImpostorProvenance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum TreePresentationSpecies {
    EnglishOak,
    CommonBeech,
}

impl TreePresentationSpecies {
    pub(in crate::presentation) fn name(self) -> &'static str {
        match self {
            Self::EnglishOak => "English oak",
            Self::CommonBeech => "common beech",
        }
    }

    fn cache_salt(self) -> u64 {
        match self {
            Self::EnglishOak => 0,
            Self::CommonBeech => 0xbeec_5eed_0000_0001,
        }
    }

    fn bake_style(self) -> TreeBakeStyle {
        match self {
            Self::EnglishOak => OAK_TREE_BAKE_STYLE,
            Self::CommonBeech => BEECH_TREE_BAKE_STYLE,
        }
    }
}

pub(in crate::presentation) fn tree_species_for_site(
    position: Vec3,
    environment: &SceneEnvironment,
) -> TreePresentationSpecies {
    let canopy = f32::from(environment.canopy_bps) / 10_000.0;
    let moisture = f32::from(environment.weather.ground_moisture_bps) / 10_000.0;
    let wetland = f32::from(environment.wetland_bps) / 10_000.0;
    let cultivation = f32::from(environment.cultivation_bps) / 10_000.0;
    // Beech is concentrated in mesic, closed-canopy communities. Using a
    // 30-metre community key produces stands instead of tree-by-tree confetti
    // and places it where the existing canopy mask already strongly suppresses
    // grass. Species remains deterministic presentation data until the compact
    // server tree recipe grows an explicit species field.
    let probability =
        (canopy * 0.62 + moisture * 0.26 - wetland * 0.38 - cultivation * 0.18 - 0.12)
            .clamp(0.0, 0.68);
    let community_x = (position.x / 30.0).floor() as i32;
    let community_z = (position.z / 30.0).floor() as i32;
    let community = ((community_x as u32 as u64) << 32) | community_z as u32 as u64;
    let hash = splitmix64(oak_site_key(environment) ^ community ^ 0xbeec_7a1d);
    if unit_hash(hash) < probability {
        TreePresentationSpecies::CommonBeech
    } else {
        TreePresentationSpecies::EnglishOak
    }
}

fn procedural_species_branch_group_mesh(
    branches: &[super::TreeBranchSegment],
    maximum_depth: u8,
    primary_group: u8,
    bark: BarkRecipe,
) -> Mesh {
    let group = branches
        .iter()
        .filter(|branch| {
            branch.depth > 0
                && branch.depth <= maximum_depth
                && branch.primary_group == primary_group
        })
        .copied()
        .collect::<Vec<_>>();
    procedural_woody_branch_mesh(&group, maximum_depth, bark)
}

/// Root marker for a fully presented playable tree.
///
/// Capture benchmarks use this boundary to attribute tree cost without
/// depending on display names or reaching into the tree's child layout.
#[derive(Component)]
pub(crate) struct PresentedTree;

#[derive(Component, Clone)]
pub(in crate::presentation) struct StreamedTreePresentation {
    cache_key: u64,
    resident_mask: u8,
    resident_leaf_mask: u8,
    active_mask: u8,
    active_leaf: Option<TreeLeafRepresentation>,
}

#[derive(Component)]
pub(in crate::presentation) struct StreamedTreeChild;

fn spawn_cached_tree(
    commands: &mut Commands,
    entity: Entity,
    cache_key: u64,
    species_name: &'static str,
) {
    commands.entity(entity).insert((
        Name::new(format!("Presented mature {species_name}")),
        PresentedTree,
        Visibility::Inherited,
        StreamedTreePresentation {
            cache_key,
            resident_mask: 0,
            resident_leaf_mask: 0,
            active_mask: u8::MAX,
            active_leaf: None,
        },
    ));
}

fn spawn_streamed_tree_children(
    commands: &mut Commands,
    entity: Entity,
    cached: &CachedTreePresentation,
    new_mask: u8,
    new_leaf_mask: u8,
) {
    commands.entity(entity).with_children(|parent| {
        // Keep renderable trunk state below the obstacle root. Hiding the trunk
        // at whole-tree LOD must not hide the camera-facing billboard sibling
        // through inherited parent visibility.
        if new_mask & 1 != 0 {
            parent.spawn((
                Name::new(format!("{} trunk", cached.species_name)),
                StreamedTreeChild,
                TreeTrunkLod,
                Mesh3d(
                    cached
                        .trunk_mesh
                        .clone()
                        .expect("requested trunk mesh is resident"),
                ),
                MeshMaterial3d(cached.bark_material.clone()),
                tree_trunk_visibility(),
            ));
        }
        if new_mask & (1 << 1) != 0 || new_leaf_mask != 0 {
            for cluster in cached
                .clusters
                .as_ref()
                .expect("requested detailed crown assets are resident")
            {
                let cluster_marker = TreeLodCluster {
                    primary_group: cluster.primary_group,
                    center: cluster.center,
                    radius: cluster.radius,
                };
                let cluster_aabb = tree_cluster_aabb(cluster.center, cluster.radius);
                let cluster_leaf_count = cached
                    .leaves
                    .iter()
                    .filter(|leaf| leaf.primary_group == cluster.primary_group)
                    .count();
                if new_leaf_mask & 1 != 0 {
                    parent.spawn((
                        Name::new(format!(
                            "{} scaffold {} cambered PBR leaves",
                            cached.species_name, cluster.primary_group
                        )),
                        StreamedTreeChild,
                        TreeLod(0),
                        cluster_marker,
                        cluster_aabb,
                        TreeLeafRepresentation::TexturedMesh,
                        TreeLeafTriangleCount(cluster_leaf_count * 8),
                        Mesh3d(
                            cluster
                                .cambered_leaf_mesh
                                .clone()
                                .expect("requested cambered leaf mesh is resident"),
                        ),
                        MeshMaterial3d(cached.leaf_material.clone()),
                        tree_leaf_visibility(
                            TreeLeafRepresentation::TexturedMesh,
                            1.0,
                            cluster.radius,
                        ),
                    ));
                }
                if new_leaf_mask & 2 != 0 {
                    parent.spawn((
                        Name::new(format!(
                            "{} scaffold {} PBR leaf cards",
                            cached.species_name, cluster.primary_group
                        )),
                        StreamedTreeChild,
                        TreeLod(0),
                        cluster_marker,
                        cluster_aabb,
                        TreeLeafRepresentation::AlphaCard,
                        TreeLeafTriangleCount(cluster_leaf_count * 2),
                        Mesh3d(
                            cluster
                                .leaf_card_mesh
                                .clone()
                                .expect("requested leaf card mesh is resident"),
                        ),
                        MeshMaterial3d(cached.leaf_material.clone()),
                        tree_leaf_visibility(
                            TreeLeafRepresentation::AlphaCard,
                            1.0,
                            cluster.radius,
                        ),
                    ));
                }
                if new_mask & (1 << 1) != 0 {
                    parent.spawn((
                        Name::new(format!(
                            "{} scaffold {} terminal buds",
                            cached.species_name, cluster.primary_group
                        )),
                        StreamedTreeChild,
                        TreeLod(0),
                        cluster_marker,
                        cluster_aabb,
                        Mesh3d(
                            cluster
                                .bud_mesh
                                .clone()
                                .expect("requested bud mesh is resident"),
                        ),
                        MeshMaterial3d(cached.bud_material.clone()),
                        tree_lod_visibility(0),
                    ));
                    parent.spawn((
                        Name::new(format!(
                            "{} scaffold {} detailed wood",
                            cached.species_name, cluster.primary_group
                        )),
                        StreamedTreeChild,
                        TreeLod(0),
                        cluster_marker,
                        cluster_aabb,
                        Mesh3d(
                            cluster
                                .detailed_branch_mesh
                                .clone()
                                .expect("requested detailed branch mesh is resident"),
                        ),
                        MeshMaterial3d(cached.bark_material.clone()),
                        tree_lod_visibility(0),
                    ));
                }
            }
        }
        for lod in 1..=3 {
            if new_mask & (1 << (lod + 1)) == 0 {
                continue;
            }
            let card = cached.lod_cards[lod as usize - 1]
                .as_ref()
                .expect("requested aggregate tree card is resident");
            parent.spawn((
                Name::new(tree_lod_name(lod, true)),
                StreamedTreeChild,
                TreeLod(lod),
                NotShadowCaster,
                Mesh3d(card.mesh.clone()),
                MeshMaterial3d(card.material.clone()),
                tree_lod_visibility(lod),
            ));
            if lod <= 2 {
                parent.spawn((
                    Name::new(tree_lod_name(lod, false)),
                    StreamedTreeChild,
                    TreeLod(lod),
                    Mesh3d(
                        cached.aggregate_branch_meshes[lod as usize - 1]
                            .clone()
                            .expect("requested aggregate branch mesh is resident"),
                    ),
                    MeshMaterial3d(cached.bark_material.clone()),
                    tree_lod_visibility(lod),
                ));
            }
        }
        if new_mask & (1 << 5) != 0 {
            let card = cached.lod_cards[3]
                .as_ref()
                .expect("requested whole-tree card is resident");
            parent.spawn((
                Name::new(tree_lod_name(4, true)),
                StreamedTreeChild,
                TreeLod(4),
                NoFrustumCulling,
                NotShadowCaster,
                Mesh3d(card.mesh.clone()),
                MeshMaterial3d(card.material.clone()),
                card.provenance.clone(),
                tree_lod_visibility(4),
            ));
        }
    });
}

fn bake_tree_card_for_cached(cached: &CachedTreePresentation, lod: u8) -> TreeLodBake {
    if cached.species == TreePresentationSpecies::EnglishOak {
        bake_tree_lod(cached.variant_seed, &cached.branches, &cached.leaves, lod)
    } else {
        bake_tree_lod_with_style(
            cached.variant_seed,
            &cached.branches,
            &cached.leaves,
            lod,
            cached.species.bake_style(),
        )
    }
}

fn ensure_tree_card_resident(
    cached: &mut CachedTreePresentation,
    lod: u8,
    meshes: &mut Assets<Mesh>,
    tree_materials: &mut Assets<TacticalTreeImpostorMaterial>,
    images: &mut Assets<Image>,
    diagnostics: &mut TreeAssetResidencyDiagnostics,
) {
    let index = lod as usize - 1;
    if cached.lod_cards[index].is_some() {
        return;
    }
    let bake = bake_tree_card_for_cached(cached, lod);
    validate_tree_bake_provenance(&bake.provenance);
    if lod == 1 {
        cached.cluster_layout = Some(
            bake.clusters
                .iter()
                .map(|cluster| (cluster.primary_group, cluster.center, cluster.radius))
                .collect(),
        );
    }
    diagnostics.impostor_vertices += bake.mesh.count_vertices();
    diagnostics.impostor_texture_bytes += bake.image.data.as_ref().map_or(0, |pixels| pixels.len());
    diagnostics.generated_lod_mask |= 1 << (lod + 1);
    let texture = images.add(bake.image);
    cached.lod_cards[index] = Some(CachedTreeCardPresentation {
        mesh: meshes.add(bake.mesh),
        material: tree_materials.add(tree_impostor_material(
            cached.variant_seed,
            bake.lod,
            texture,
        )),
        provenance: bake.provenance,
    });
}

fn ensure_detailed_tree_assets_resident(
    cached: &mut CachedTreePresentation,
    selected_leaf: Option<TreeLeafRepresentation>,
    meshes: &mut Assets<Mesh>,
    diagnostics: &mut TreeAssetResidencyDiagnostics,
) {
    if cached.clusters.is_none() {
        cached.clusters = Some(
            cached
                .cluster_layout
                .as_ref()
                .expect("LOD1 bake supplies detailed crown cluster bounds")
                .iter()
                .map(
                    |&(primary_group, center, radius)| CachedTreeClusterPresentation {
                        primary_group,
                        center,
                        radius,
                        detailed_branch_mesh: None,
                        cambered_leaf_mesh: None,
                        leaf_card_mesh: None,
                        bud_mesh: None,
                    },
                )
                .collect(),
        );
    }
    for cluster in cached
        .clusters
        .as_mut()
        .expect("detailed crown cache was initialized")
    {
        if cluster.detailed_branch_mesh.is_none() {
            let mesh = match cached.species {
                TreePresentationSpecies::EnglishOak => {
                    procedural_tree_branch_group_mesh(&cached.branches, 3, cluster.primary_group)
                }
                TreePresentationSpecies::CommonBeech => procedural_species_branch_group_mesh(
                    &cached.branches,
                    3,
                    cluster.primary_group,
                    COMMON_BEECH_BARK,
                ),
            };
            diagnostics.detailed_branch_vertices += mesh.count_vertices();
            cluster.detailed_branch_mesh = Some(meshes.add(mesh));
        }
        if cluster.bud_mesh.is_none() {
            let mesh = procedural_oak_bud_group_mesh(&cached.branches, cluster.primary_group);
            diagnostics.bud_vertices += mesh.count_vertices();
            cluster.bud_mesh = Some(meshes.add(mesh));
        }
        if selected_leaf != Some(TreeLeafRepresentation::AlphaCard)
            && cluster.cambered_leaf_mesh.is_none()
        {
            let mesh =
                procedural_oak_textured_leaf_group_mesh(&cached.leaves, cluster.primary_group);
            diagnostics.cambered_leaf_vertices += mesh.count_vertices();
            cluster.cambered_leaf_mesh = Some(meshes.add(mesh));
        }
        if selected_leaf != Some(TreeLeafRepresentation::TexturedMesh)
            && cluster.leaf_card_mesh.is_none()
        {
            let mesh = procedural_oak_leaf_card_group_mesh(&cached.leaves, cluster.primary_group);
            diagnostics.leaf_card_vertices += mesh.count_vertices();
            cluster.leaf_card_mesh = Some(meshes.add(mesh));
        }
    }
    diagnostics.generated_lod_mask |= 1 << 1;
}

#[allow(clippy::too_many_arguments)]
fn ensure_tree_assets_resident(
    cached: &mut CachedTreePresentation,
    mask: u8,
    selected_leaf: Option<TreeLeafRepresentation>,
    meshes: &mut Assets<Mesh>,
    tree_materials: &mut Assets<TacticalTreeImpostorMaterial>,
    images: &mut Assets<Image>,
    diagnostics: &mut TreeAssetResidencyDiagnostics,
) {
    let started = std::time::Instant::now();
    if mask & 1 != 0 && cached.trunk_mesh.is_none() {
        let mesh = match cached.species {
            TreePresentationSpecies::EnglishOak => procedural_tree_branch_mesh(&cached.branches, 0),
            TreePresentationSpecies::CommonBeech => {
                procedural_woody_branch_mesh(&cached.branches, 0, COMMON_BEECH_BARK)
            }
        };
        diagnostics.trunk_vertices += mesh.count_vertices();
        cached.trunk_mesh = Some(meshes.add(mesh));
        diagnostics.generated_lod_mask |= 1;
    }
    if mask & (1 << 1) != 0 {
        ensure_tree_card_resident(cached, 1, meshes, tree_materials, images, diagnostics);
        ensure_detailed_tree_assets_resident(cached, selected_leaf, meshes, diagnostics);
    }
    for lod in 1..=4 {
        if mask & (1 << (lod + 1)) == 0 {
            continue;
        }
        ensure_tree_card_resident(cached, lod, meshes, tree_materials, images, diagnostics);
        if lod <= 2 && cached.aggregate_branch_meshes[lod as usize - 1].is_none() {
            let depth = if lod == 1 { 2 } else { 1 };
            let mesh = match cached.species {
                TreePresentationSpecies::EnglishOak => {
                    procedural_woody_crown_mesh(&cached.branches, depth, ENGLISH_OAK_BARK)
                }
                TreePresentationSpecies::CommonBeech => {
                    procedural_woody_crown_mesh(&cached.branches, depth, COMMON_BEECH_BARK)
                }
            };
            diagnostics.aggregate_branch_vertices += mesh.count_vertices();
            cached.aggregate_branch_meshes[lod as usize - 1] = Some(meshes.add(mesh));
        }
    }
    let elapsed = started.elapsed().as_millis();
    diagnostics.generation_milliseconds += elapsed;
    if elapsed > 0 {
        info!(
            species = cached.species_name,
            requested_mask = mask,
            elapsed_ms = elapsed,
            resident_mask = diagnostics.generated_lod_mask,
            "Generated demand-driven tactical tree assets"
        );
    }
}

pub(in crate::presentation) fn stream_tree_lod_children(
    mut commands: Commands,
    camera: Single<(&GlobalTransform, &Projection), With<Camera3d>>,
    lod_override: Res<TreeLodRenderOverride>,
    mut trees: Query<(Entity, &GlobalTransform, &mut StreamedTreePresentation)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut tree_cache: ResMut<TreePresentationCache>,
    mut residency: ResMut<TreeAssetResidencyDiagnostics>,
) {
    let focal_scale = match camera.1 {
        Projection::Perspective(projection) => {
            (80.0_f32.to_radians() * 0.5).tan() / (projection.fov * 0.5).tan()
        }
        _ => 1.0,
    } * lod_override.projected_scale.unwrap_or(1.0);
    for (entity, transform, mut presentation) in &mut trees {
        let distance = camera.0.translation().distance(transform.translation());
        let scaled = distance / focal_scale.max(0.25);
        let mask = if let Some(lod) = lod_override.lod {
            (1 << (lod + 1)) | u8::from(lod < 4)
        } else {
            tree_streaming_mask(scaled)
        };
        let selected_leaf = lod_override
            .leaf
            .or_else(|| tree_streamed_leaf_representation(scaled));
        if presentation.active_mask == mask && presentation.active_leaf == selected_leaf {
            continue;
        }
        debug!(
            distance,
            scaled_distance = scaled,
            requested_mask = mask,
            ?selected_leaf,
            "Changed streamed tactical tree residency"
        );
        let cached = tree_cache
            .variants
            .get_mut(&presentation.cache_key)
            .expect("streamed tree cache entry remains resident");
        ensure_tree_assets_resident(
            cached,
            mask,
            selected_leaf,
            &mut meshes,
            &mut tree_materials,
            &mut images,
            &mut residency,
        );
        let desired_leaf_mask = if mask & (1 << 1) == 0 {
            0
        } else {
            match selected_leaf {
                Some(TreeLeafRepresentation::TexturedMesh) => 1,
                Some(TreeLeafRepresentation::AlphaCard) => 2,
                None => 3,
            }
        };
        let new_mask = mask & !presentation.resident_mask;
        let new_leaf_mask = desired_leaf_mask & !presentation.resident_leaf_mask;
        spawn_streamed_tree_children(&mut commands, entity, cached, new_mask, new_leaf_mask);
        presentation.resident_mask |= new_mask;
        presentation.resident_leaf_mask |= new_leaf_mask;
        presentation.active_mask = mask;
        presentation.active_leaf = selected_leaf;
    }
}

fn tree_streamed_leaf_representation(scaled_distance: f32) -> Option<TreeLeafRepresentation> {
    // The material visibility ranges overlap from roughly 3.5-5 m. Widen the
    // residency overlap slightly so projection and cluster-size adjustments
    // cannot expose an ungenerated representation at the handoff.
    if scaled_distance < 3.0 {
        Some(TreeLeafRepresentation::TexturedMesh)
    } else if scaled_distance > 6.0 {
        Some(TreeLeafRepresentation::AlphaCard)
    } else {
        None
    }
}

fn tree_streaming_mask(scaled_distance: f32) -> u8 {
    // Generate each incoming crown tier one full outgoing LOD band before it
    // becomes visible. Mesh/material insertion is asynchronous in the render
    // world: the old four-metre lead could leave only the already-resident
    // trunk on a first approach, while a second approach was correct because
    // the crown assets remained cached. This changes when eventual residency
    // is paid, not which tiers are drawn or retained.
    u8::from(scaled_distance < 75.0)
        | (u8::from(scaled_distance < 24.0) << 1)
        | (u8::from((3.0..44.0).contains(&scaled_distance)) << 2)
        | (u8::from((8.0..75.0).contains(&scaled_distance)) << 3)
        | (u8::from((18.0..100.0).contains(&scaled_distance)) << 4)
        | (u8::from((40.0..220.0).contains(&scaled_distance)) << 5)
}

fn tree_cluster_aabb(center: Vec3, radius: f32) -> Aabb {
    let extent = Vec3::splat(radius.max(0.01));
    Aabb::from_min_max(center - extent, center + extent)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn ensure_vista_tree_variant(
    variant_seed: u64,
    competition: f32,
    species: TreePresentationSpecies,
    meshes: &mut Assets<Mesh>,
    tree_materials: &mut Assets<TacticalTreeImpostorMaterial>,
    images: &mut Assets<Image>,
    cache: &mut VistaTreePresentationCache,
) -> CachedVistaTreePresentation {
    let competition_key = (competition * 4095.0).round() as u64;
    let cache_key = variant_seed ^ competition_key.rotate_left(32) ^ species.cache_salt();
    if let Some(cached) = cache.variants.get(&cache_key) {
        return cached.clone();
    }
    let (branches, leaves) = match species {
        TreePresentationSpecies::EnglishOak => {
            let branches = procedural_tree_skeleton(variant_seed, competition);
            let leaves = procedural_oak_leaves(variant_seed, &branches, competition);
            (branches, leaves)
        }
        TreePresentationSpecies::CommonBeech => {
            let branches =
                procedural_woody_plant_skeleton(variant_seed, competition, COMMON_BEECH_PARAMETERS);
            let leaves = procedural_woody_plant_leaves(
                variant_seed,
                &branches,
                competition,
                COMMON_BEECH_PARAMETERS,
            );
            (branches, leaves)
        }
    };
    let bake = if species == TreePresentationSpecies::EnglishOak {
        bake_tree_lod(variant_seed, &branches, &leaves, 4)
    } else {
        bake_tree_lod_with_style(variant_seed, &branches, &leaves, 4, species.bake_style())
    };
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
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut leaf_card_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    mut tree_cache: ResMut<TreePresentationCache>,
    mut residency: ResMut<TreeAssetResidencyDiagnostics>,
    procedural_assets: Res<ProceduralEnvironmentAssets>,
) {
    let Some(environment) = active
        .entity
        .and_then(|entity| environments.get(entity).ok())
    else {
        return;
    };
    let competition = canopy_competition(environment.canopy_bps);
    let site_key = oak_site_key(environment);
    for (entity, transform) in &pending {
        let started = std::time::Instant::now();
        info!("Generating playable tactical tree presentation");
        let seed = obstacle_seed(transform.translation);
        let species = tree_species_for_site(transform.translation, environment);
        let variant_index = (seed & 3) as usize;
        let variant_seed = splitmix64(0x6f61_6b00 ^ variant_index as u64);
        let competition_key = (competition * 4095.0).round() as u64;
        let cache_key = variant_seed
            ^ competition_key.rotate_left(32)
            ^ site_key.rotate_left(17)
            ^ species.cache_salt();
        if !tree_cache.variants.contains_key(&cache_key) {
            let (branches, leaves) = match species {
                TreePresentationSpecies::EnglishOak => {
                    let gnarling = oak_gnarling_for_site(
                        OAK_GNARLING_SHOWCASE[variant_index],
                        environment,
                        variant_seed,
                    );
                    let branches =
                        procedural_oak_skeleton_with_gnarling(variant_seed, competition, gnarling);
                    let leaves = procedural_oak_leaves(variant_seed, &branches, competition);
                    (branches, leaves)
                }
                TreePresentationSpecies::CommonBeech => {
                    let branches = procedural_woody_plant_skeleton(
                        variant_seed,
                        competition,
                        COMMON_BEECH_PARAMETERS,
                    );
                    let leaves = procedural_woody_plant_leaves(
                        variant_seed,
                        &branches,
                        competition,
                        COMMON_BEECH_PARAMETERS,
                    );
                    (branches, leaves)
                }
            };
            info!(
                elapsed_ms = started.elapsed().as_millis(),
                branches = branches.len(),
                leaves = leaves.len(),
                "Generated playable tactical tree source geometry"
            );
            let bark_material = match species {
                TreePresentationSpecies::EnglishOak => {
                    tree_cache.oak_bark_material.clone().unwrap_or_else(|| {
                        let material = materials.add(oak_bark_material(&procedural_assets));
                        tree_cache.oak_bark_material = Some(material.clone());
                        material
                    })
                }
                TreePresentationSpecies::CommonBeech => {
                    tree_cache.beech_bark_material.clone().unwrap_or_else(|| {
                        let material = materials.add(StandardMaterial {
                            base_color: Color::srgb_u8(145, 145, 135),
                            perceptual_roughness: 0.9,
                            ..default()
                        });
                        tree_cache.beech_bark_material = Some(material.clone());
                        material
                    })
                }
            };
            let leaf_material = leaf_card_materials.add(match species {
                TreePresentationSpecies::EnglishOak => oak_leaf_material(&procedural_assets),
                TreePresentationSpecies::CommonBeech => beech_leaf_material(&procedural_assets),
            });
            let bud_material = materials.add(StandardMaterial {
                base_color: match species {
                    TreePresentationSpecies::EnglishOak => Color::srgb(0.36, 0.27, 0.1),
                    TreePresentationSpecies::CommonBeech => Color::srgb_u8(112, 68, 43),
                },
                perceptual_roughness: 0.92,
                ..default()
            });
            let cached = CachedTreePresentation {
                variant_seed,
                species,
                species_name: species.name(),
                branches,
                leaves,
                trunk_mesh: None,
                cluster_layout: None,
                clusters: None,
                aggregate_branch_meshes: [None, None],
                lod_cards: [None, None, None, None],
                bark_material,
                leaf_material,
                bud_material,
            };
            residency.variants += 1;
            residency.source_branches += cached.branches.len();
            residency.source_leaves += cached.leaves.len();
            tree_cache.variants.insert(cache_key, cached);
        }
        spawn_cached_tree(&mut commands, entity, cache_key, species.name());
        info!(
            elapsed_ms = started.elapsed().as_millis(),
            "Generated playable tactical tree presentation"
        );
        commands.entity(entity).remove::<PendingTreePresentation>();
    }
}

pub(in crate::presentation) fn canopy_competition(canopy_bps: u16) -> f32 {
    let normalized = f32::from(canopy_bps) / 10_000.0;
    normalized * normalized * (3.0 - 2.0 * normalized)
}

fn oak_site_key(environment: &SceneEnvironment) -> u64 {
    let location = u64::from(environment.latitude_microdegrees as u32) << 32
        | u64::from(environment.longitude_microdegrees as u32);
    let terrain = u64::from(environment.hilly_bps)
        | u64::from(environment.wetland_bps) << 14
        | u64::from(environment.cultivation_bps) << 28
        | u64::from(environment.canopy_bps) << 42;
    splitmix64(
        location ^ terrain ^ (environment.absolute_elevation_metres as i64 as u64).rotate_left(9),
    )
}

fn oak_gnarling_for_site(
    mut recipe: OakGnarlingParameters,
    environment: &SceneEnvironment,
    tree_seed: u64,
) -> OakGnarlingParameters {
    let canopy = f32::from(environment.canopy_bps) / 10_000.0;
    let open_exposure = 1.0 - canopy;
    let slope = f32::from(environment.hilly_bps) / 10_000.0;
    let wetland = f32::from(environment.wetland_bps) / 10_000.0;
    let cultivation = f32::from(environment.cultivation_bps) / 10_000.0;
    let elevation =
        ((f32::from(environment.absolute_elevation_metres) - 40.0) / 900.0).clamp(0.0, 1.0);
    let susceptibility = 0.72 + unit_hash(splitmix64(tree_seed ^ 0x5355_5343)) * 0.28;
    let wind_exposure =
        (open_exposure * 0.46 + slope * 0.34 + elevation * 0.2).clamp(0.0, 1.0) * susceptibility;
    let age_and_wounds = unit_hash(splitmix64(tree_seed ^ 0x4147_4557));
    let location = u64::from(environment.latitude_microdegrees as u32) << 32
        | u64::from(environment.longitude_microdegrees as u32);
    recipe.stress_azimuth_radians =
        unit_hash(splitmix64(location ^ 0x5749_4e44)) * core::f32::consts::TAU;
    let add = |value: f32, stress: f32| (value + stress).clamp(0.0, 1.0);
    recipe.root_spread = add(
        recipe.root_spread,
        slope * 0.34 + wetland * 0.24 + wind_exposure * 0.2,
    );
    recipe.root_meander = add(recipe.root_meander, slope * 0.28 + wetland * 0.18);
    recipe.root_exposure = add(recipe.root_exposure, slope * 0.5 + open_exposure * 0.12);
    recipe.root_forking = add(recipe.root_forking, slope * 0.2 + age_and_wounds * 0.12);
    recipe.trunk_lean = add(recipe.trunk_lean, wind_exposure * 0.62 + wetland * 0.18);
    recipe.trunk_sweep = add(recipe.trunk_sweep, wind_exposure * 0.7);
    recipe.trunk_twist = add(
        recipe.trunk_twist,
        wind_exposure * 0.24 + age_and_wounds * 0.16,
    );
    recipe.trunk_crooks = add(
        recipe.trunk_crooks,
        cultivation * 0.3 + age_and_wounds * 0.16,
    );
    recipe.taper_irregularity = add(
        recipe.taper_irregularity,
        wetland * 0.18 + cultivation * 0.22 + age_and_wounds * 0.14,
    );
    recipe.knot_frequency = add(
        recipe.knot_frequency,
        cultivation * 0.38 + age_and_wounds * 0.24,
    );
    recipe.knot_scale = add(recipe.knot_scale, cultivation * 0.24 + age_and_wounds * 0.2);
    recipe.burl_scale = add(recipe.burl_scale, wetland * 0.3 + age_and_wounds * 0.16);
    recipe.scaffold_droop = add(
        recipe.scaffold_droop,
        age_and_wounds * 0.18 + wetland * 0.12,
    );
    recipe.scaffold_sweep = add(recipe.scaffold_sweep, wind_exposure * 0.76);
    recipe.scaffold_contortion = add(
        recipe.scaffold_contortion,
        wind_exposure * 0.32 + age_and_wounds * 0.18,
    );
    recipe.crown_asymmetry = add(recipe.crown_asymmetry, wind_exposure * 0.82);
    recipe
}

#[cfg(test)]
pub(super) fn oak_gnarling_for_test_site(
    recipe: OakGnarlingParameters,
    environment: &SceneEnvironment,
    tree_seed: u64,
) -> OakGnarlingParameters {
    oak_gnarling_for_site(recipe, environment, tree_seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_tactical_core::prelude::{Precipitation, WeatherSnapshot};

    fn environment(canopy_bps: u16, hilly_bps: u16, wetland_bps: u16) -> SceneEnvironment {
        SceneEnvironment {
            scene_digest: "oak-site-test".into(),
            generation_version: 7,
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 340_440,
            absolute_elevation_metres: 420,
            weather: WeatherSnapshot {
                rules_version: 2,
                interval_start_minute: 340_440,
                cell_latitude: 214,
                cell_longitude: 40,
                temperature_deci_c: 120,
                wind_speed_bps: 1_200,
                precipitation: Precipitation::Clear,
                intensity_bps: 0,
                ground_moisture_bps: 100,
                snow_cover_bps: 0,
            },
            canopy_bps,
            wetland_bps,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps,
        }
    }

    #[test]
    fn stable_site_not_current_weather_drives_oak_growth_history() {
        let site = environment(2_000, 8_000, 0);
        let first = oak_gnarling_for_site(OAK_GNARLING_SHOWCASE[0], &site, 42);
        let mut storm = site.clone();
        storm.weather.wind_speed_bps = 10_000;
        storm.weather.intensity_bps = 10_000;
        assert_eq!(
            first,
            oak_gnarling_for_site(OAK_GNARLING_SHOWCASE[0], &storm, 42)
        );
        assert_eq!(oak_site_key(&site), oak_site_key(&storm));
    }

    #[test]
    fn streaming_keeps_adjacent_tree_lods_resident_through_every_handoff() {
        for (distance, required_bits) in [
            (7.0, (1 << 1) | (1 << 2)),
            (14.0, (1 << 2) | (1 << 3)),
            (28.0, (1 << 3) | (1 << 4)),
            (55.0, (1 << 4) | (1 << 5)),
        ] {
            assert_eq!(tree_streaming_mask(distance) & required_bits, required_bits);
        }
    }

    #[test]
    fn streaming_prefetches_each_incoming_crown_one_outgoing_band_early() {
        for (distance, incoming_lod_bit) in [
            (90.0, 1 << 4),
            (72.0, 1 << 3),
            (42.0, 1 << 2),
            (22.0, 1 << 1),
        ] {
            assert_ne!(tree_streaming_mask(distance) & incoming_lod_bit, 0);
        }
        assert_eq!(tree_streaming_mask(120.0), 1 << 5);
    }

    #[test]
    fn leaf_residency_only_overlaps_near_the_representation_handoff() {
        assert_eq!(
            tree_streamed_leaf_representation(2.0),
            Some(TreeLeafRepresentation::TexturedMesh)
        );
        assert_eq!(tree_streamed_leaf_representation(4.0), None);
        assert_eq!(
            tree_streamed_leaf_representation(7.0),
            Some(TreeLeafRepresentation::AlphaCard)
        );
    }

    #[test]
    fn distant_request_does_not_generate_near_tree_geometry() {
        let variant_seed = 42;
        let branches = procedural_tree_skeleton(variant_seed, 0.0);
        let leaves = procedural_oak_leaves(variant_seed, &branches, 0.0);
        let mut cached = CachedTreePresentation {
            variant_seed,
            species: TreePresentationSpecies::EnglishOak,
            species_name: TreePresentationSpecies::EnglishOak.name(),
            branches,
            leaves,
            trunk_mesh: None,
            cluster_layout: None,
            clusters: None,
            aggregate_branch_meshes: [None, None],
            lod_cards: [None, None, None, None],
            bark_material: Handle::default(),
            leaf_material: Handle::default(),
            bud_material: Handle::default(),
        };
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<TacticalTreeImpostorMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut diagnostics = TreeAssetResidencyDiagnostics::default();
        ensure_tree_assets_resident(
            &mut cached,
            1 << 5,
            None,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut diagnostics,
        );
        assert!(cached.lod_cards[3].is_some());
        assert!(cached.lod_cards[..3].iter().all(Option::is_none));
        assert!(cached.trunk_mesh.is_none());
        assert!(cached.clusters.is_none());
        assert_eq!(diagnostics.trunk_vertices, 0);
        assert_eq!(diagnostics.cambered_leaf_vertices, 0);
        assert_eq!(diagnostics.leaf_card_vertices, 0);
        assert!(diagnostics.impostor_vertices > 0);
        assert!(diagnostics.impostor_texture_bytes > 0);
    }

    #[test]
    fn complete_live_oak_mesh_suite_remains_constructible() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let mut vertex_count = procedural_tree_branch_mesh(&branches, 0).count_vertices();
        for primary_group in 0..TREE_PRIMARY_GROUP_COUNT {
            vertex_count +=
                procedural_tree_branch_group_mesh(&branches, 3, primary_group).count_vertices();
            vertex_count +=
                procedural_oak_textured_leaf_group_mesh(&leaves, primary_group).count_vertices();
            vertex_count +=
                procedural_oak_leaf_card_group_mesh(&leaves, primary_group).count_vertices();
            vertex_count +=
                procedural_oak_bud_group_mesh(&branches, primary_group).count_vertices();
        }
        vertex_count += [2, 1]
            .into_iter()
            .map(|depth| {
                procedural_woody_crown_mesh(&branches, depth, ENGLISH_OAK_BARK).count_vertices()
            })
            .sum::<usize>();
        assert!(vertex_count > 0);
    }

    #[test]
    fn exposed_hilly_sites_share_wind_direction_and_gnarl_more_than_shelter() {
        let exposed = environment(1_000, 9_000, 0);
        let sheltered = environment(9_000, 0, 0);
        let exposed_a = oak_gnarling_for_site(OAK_GNARLING_SHOWCASE[0], &exposed, 7);
        let exposed_b = oak_gnarling_for_site(OAK_GNARLING_SHOWCASE[0], &exposed, 91);
        let sheltered = oak_gnarling_for_site(OAK_GNARLING_SHOWCASE[0], &sheltered, 7);
        assert_eq!(
            exposed_a.stress_azimuth_radians,
            exposed_b.stress_azimuth_radians
        );
        assert!(exposed_a.trunk_lean > sheltered.trunk_lean);
        assert!(exposed_a.scaffold_sweep > sheltered.scaffold_sweep);
        assert!(exposed_a.crown_asymmetry > sheltered.crown_asymmetry);
        assert!(exposed_a.root_spread > sheltered.root_spread);
        assert!(exposed_a.root_exposure > sheltered.root_exposure);
    }

    #[test]
    fn beech_selection_is_clustered_and_favors_mesic_closed_canopy() {
        let mut mesic = environment(9_000, 1_000, 0);
        mesic.weather.ground_moisture_bps = 7_000;
        let mut open = environment(1_000, 1_000, 0);
        open.weather.ground_moisture_bps = 800;
        let mesic_count = (-30..30)
            .flat_map(|x| (-30..30).map(move |z| Vec3::new(x as f32 * 30.0, 0.0, z as f32 * 30.0)))
            .filter(|position| {
                tree_species_for_site(*position, &mesic) == TreePresentationSpecies::CommonBeech
            })
            .count();
        let open_count = (-30..30)
            .flat_map(|x| (-30..30).map(move |z| Vec3::new(x as f32 * 30.0, 0.0, z as f32 * 30.0)))
            .filter(|position| {
                tree_species_for_site(*position, &open) == TreePresentationSpecies::CommonBeech
            })
            .count();
        assert!(mesic_count > open_count * 3);
        let origin = tree_species_for_site(Vec3::new(1.0, 0.0, 1.0), &mesic);
        assert_eq!(
            origin,
            tree_species_for_site(Vec3::new(29.0, 0.0, 29.0), &mesic)
        );
    }

    #[test]
    fn beech_whole_tree_billboard_uses_beech_geometry_and_palette() {
        let branches = procedural_woody_plant_skeleton(42, 0.7, COMMON_BEECH_PARAMETERS);
        let leaves = procedural_woody_plant_leaves(42, &branches, 0.7, COMMON_BEECH_PARAMETERS);
        let bake = bake_tree_lod_with_style(
            42,
            &branches,
            &leaves,
            4,
            TreePresentationSpecies::CommonBeech.bake_style(),
        );
        validate_tree_bake_provenance(&bake.provenance);
        assert!(
            bake.image
                .data
                .as_ref()
                .is_some_and(|pixels| pixels.iter().any(|channel| *channel > 0))
        );
        assert_eq!(
            bake.provenance.source_geometry_hash,
            super::super::impostor::tree_source_geometry_hash(&branches, &leaves)
                ^ u64::from(super::super::impostor::TREE_IMPOSTOR_BAKE_VERSION)
        );
    }

    #[test]
    fn forced_beech_vista_handoff_uses_beech_source_geometry_and_cache_identity() {
        let variant_seed = splitmix64(0x6f61_6b00);
        let competition = 0.5;
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<TacticalTreeImpostorMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut cache = VistaTreePresentationCache::default();
        let beech = ensure_vista_tree_variant(
            variant_seed,
            competition,
            TreePresentationSpecies::CommonBeech,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut cache,
        );
        let oak = ensure_vista_tree_variant(
            variant_seed,
            competition,
            TreePresentationSpecies::EnglishOak,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut cache,
        );
        let branches =
            procedural_woody_plant_skeleton(variant_seed, competition, COMMON_BEECH_PARAMETERS);
        let leaves = procedural_woody_plant_leaves(
            variant_seed,
            &branches,
            competition,
            COMMON_BEECH_PARAMETERS,
        );
        assert_eq!(
            beech.provenance.source_geometry_hash,
            super::super::impostor::tree_source_geometry_hash(&branches, &leaves)
                ^ u64::from(super::super::impostor::TREE_IMPOSTOR_BAKE_VERSION)
        );
        assert_ne!(
            beech.provenance.source_geometry_hash,
            oak.provenance.source_geometry_hash
        );
        assert_eq!(
            cache.variants.len(),
            2,
            "oak and beech vista atlases must not alias"
        );
    }
}
