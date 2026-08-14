use super::geometry::BarkRecipe;
use super::impostor::{
    OAK_TREE_BAKE_STYLE, TreeBakeStyle, TreeImpostorProvenance, bake_tree_lod,
    bake_tree_lod_with_style, tree_impostor_material, tree_leaf_visibility, tree_lod_name,
    tree_lod_visibility, tree_trunk_visibility, validate_tree_bake_provenance,
};
use super::{
    COMMON_BEECH_BARK, COMMON_BEECH_PARAMETERS, OAK_GNARLING_SHOWCASE, OakGnarlingParameters,
    TREE_PRIMARY_GROUP_COUNT, TacticalTreeImpostorMaterial, TacticalTreeLeafCardMaterial,
    TreeLeafRepresentation, TreeLod, TreeLodCluster, TreeLodRenderOverride, TreeTrunkLod,
    beech_leaf_material, oak_bark_material, oak_leaf_material, procedural_oak_bud_group_mesh,
    procedural_oak_leaf_card_group_mesh, procedural_oak_leaves,
    procedural_oak_skeleton_with_gnarling, procedural_oak_textured_leaf_group_mesh,
    procedural_tree_branch_group_mesh, procedural_tree_branch_mesh, procedural_tree_skeleton,
    procedural_woody_branch_mesh, procedural_woody_plant_leaves, procedural_woody_plant_skeleton,
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

#[derive(Component)]
pub(in crate::presentation) struct PendingTreePresentation;

#[derive(Resource, Default)]
pub(in crate::presentation) struct TreePresentationCache {
    variants: std::collections::HashMap<u64, CachedTreePresentation>,
    oak_bark_material: Option<Handle<StandardMaterial>>,
    beech_bark_material: Option<Handle<StandardMaterial>>,
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
    species_name: &'static str,
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
            Self::CommonBeech => TreeBakeStyle {
                bark: COMMON_BEECH_BARK,
                bark_srgb: [145.0, 145.0, 135.0],
                leaf_srgb: [91.0, 119.0, 70.0],
                crown_radius_metres: COMMON_BEECH_PARAMETERS.crown_radius_metres,
            },
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
    cached: CachedTreePresentation,
    active_mask: u8,
    active_leaf: Option<TreeLeafRepresentation>,
}

#[derive(Component)]
pub(in crate::presentation) struct StreamedTreeChild;

fn spawn_cached_tree(commands: &mut Commands, entity: Entity, cached: &CachedTreePresentation) {
    commands.entity(entity).insert((
        Name::new(format!("Presented mature {}", cached.species_name)),
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
                Name::new(format!("{} trunk", cached.species_name)),
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
                            "{} scaffold {} cambered PBR leaves",
                            cached.species_name, cluster.primary_group
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
                            "{} scaffold {} PBR leaf cards",
                            cached.species_name, cluster.primary_group
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
                        "{} scaffold {} terminal buds",
                        cached.species_name, cluster.primary_group
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
                        "{} scaffold {} detailed wood",
                        cached.species_name, cluster.primary_group
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut leaf_card_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut tree_cache: ResMut<TreePresentationCache>,
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
        let seed = obstacle_seed(transform.translation);
        let species = tree_species_for_site(transform.translation, environment);
        let variant_index = (seed & 3) as usize;
        let variant_seed = splitmix64(0x6f61_6b00 ^ variant_index as u64);
        let competition_key = (competition * 4095.0).round() as u64;
        let cache_key = variant_seed
            ^ competition_key.rotate_left(32)
            ^ site_key.rotate_left(17)
            ^ species.cache_salt();
        let cached = if let Some(cached) = tree_cache.variants.get(&cache_key) {
            cached.clone()
        } else {
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
            let bake_style = species.bake_style();
            let baked_lods = (1..5)
                .map(|lod| {
                    if species == TreePresentationSpecies::EnglishOak {
                        bake_tree_lod(variant_seed, &branches, &leaves, lod)
                    } else {
                        bake_tree_lod_with_style(variant_seed, &branches, &leaves, lod, bake_style)
                    }
                })
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
                        detailed_branch_mesh: meshes.add(match species {
                            TreePresentationSpecies::EnglishOak => {
                                procedural_tree_branch_group_mesh(&branches, 3, primary_group)
                            }
                            TreePresentationSpecies::CommonBeech => {
                                procedural_species_branch_group_mesh(
                                    &branches,
                                    3,
                                    primary_group,
                                    COMMON_BEECH_BARK,
                                )
                            }
                        }),
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
                species_name: species.name(),
                trunk_mesh: meshes.add(match species {
                    TreePresentationSpecies::EnglishOak => {
                        procedural_tree_branch_mesh(&branches, 0)
                    }
                    TreePresentationSpecies::CommonBeech => {
                        procedural_woody_branch_mesh(&branches, 0, COMMON_BEECH_BARK)
                    }
                }),
                clusters,
                aggregate_branch_meshes: [2, 1].map(|depth| {
                    meshes.add(match species {
                        TreePresentationSpecies::EnglishOak => {
                            procedural_tree_branch_mesh(&branches, depth)
                        }
                        TreePresentationSpecies::CommonBeech => {
                            procedural_woody_branch_mesh(&branches, depth, COMMON_BEECH_BARK)
                        }
                    })
                }),
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
