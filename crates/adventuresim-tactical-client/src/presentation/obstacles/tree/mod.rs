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
    pub(super) leaf_material: Handle<TacticalTreeLeafCardMaterial>,
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
    pub(super) cambered_leaf_mesh: Handle<Mesh>,
    pub(super) leaf_card_mesh: Handle<Mesh>,
    pub(super) bud_mesh: Handle<Mesh>,
    pub(super) card_meshes: [Handle<Mesh>; 3],
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(crate) struct TacticalTreeLeafCardMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub(crate) opacity: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub(crate) front_albedo: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    pub(crate) back_albedo: Handle<Image>,
    #[texture(6)]
    #[sampler(7)]
    pub(crate) front_normal: Handle<Image>,
    #[texture(8)]
    #[sampler(9)]
    pub(crate) back_normal: Handle<Image>,
    /// Wind direction XZ, strength, and CPU-synchronized phase time.
    #[uniform(10)]
    pub(crate) parameters: Vec4,
    /// Opacity cutoff, tangent-space normal strength, canopy AO strength, and
    /// diffuse transmission for the species' leaf thickness.
    #[uniform(10)]
    pub(crate) surface_parameters: Vec4,
    /// Perceptual roughness, physical thickness in metres, and reserved.
    #[uniform(10)]
    pub(crate) physical_parameters: Vec4,
}

pub(crate) fn oak_leaf_material(asset_server: &AssetServer) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        asset_server,
        "trees/oak_leaf_03",
        0.28,
        0.72,
        canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres),
        0.32,
    )
}

pub(in crate::presentation) fn hazel_leaf_material(
    asset_server: &AssetServer,
) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        asset_server,
        "shrubs/common_hazel_leaf",
        0.32,
        0.68,
        canopy_ao_strength(COMMON_HAZEL_PARAMETERS.crown_radius_metres),
        0.46,
    )
}

/// Approximates unresolved canopy occlusion as transmission through foliage.
///
/// The prior species constants gave a three-metre-wide hazel almost the same
/// occlusion as a twelve-metre-wide oak. Beer-Lambert transmission makes the
/// effect depend on the representative path length through each crown while
/// retaining the accepted mature-oak strength.
fn canopy_ao_strength(crown_radius_metres: f32) -> f32 {
    const FOLIAGE_EXTINCTION_PER_METRE: f32 = 0.16;
    1.0 - (-FOLIAGE_EXTINCTION_PER_METRE * crown_radius_metres.max(0.0)).exp()
}

fn leaf_material(
    asset_server: &AssetServer,
    stem: &str,
    alpha_cutoff: f32,
    normal_strength: f32,
    canopy_ao: f32,
    diffuse_transmission: f32,
) -> TacticalTreeLeafCardMaterial {
    let linear_image = |path| {
        asset_server
            .load_builder()
            .with_settings(|settings: &mut bevy::image::ImageLoaderSettings| {
                settings.is_srgb = false
            })
            .load(path)
    };
    TacticalTreeLeafCardMaterial {
        opacity: linear_image(format!("textures/{stem}_opacity.png")),
        front_albedo: asset_server.load(format!("textures/{stem}_front_albedo.png")),
        back_albedo: asset_server.load(format!("textures/{stem}_back_albedo.png")),
        front_normal: linear_image(format!("textures/{stem}_front_normal_dx.png")),
        back_normal: linear_image(format!("textures/{stem}_back_normal_dx.png")),
        parameters: Vec4::new(0.74, 0.67, 0.035, 0.0),
        surface_parameters: Vec4::new(
            alpha_cutoff,
            normal_strength,
            canopy_ao,
            diffuse_transmission,
        ),
        physical_parameters: Vec4::new(0.86, 0.001, 0.0, 0.0),
    }
}

pub(in crate::presentation) fn update_tree_leaf_wind(
    time: Res<Time>,
    mut materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
) {
    let phase_time = time.elapsed_secs() * 1.15;
    for (_, material) in materials.iter_mut() {
        material.parameters.w = phase_time;
    }
}

impl Material for TacticalTreeLeafCardMaterial {
    fn vertex_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(self.surface_parameters.x)
    }

    fn enable_prepass() -> bool {
        true
    }

    fn enable_shadows() -> bool {
        true
    }

    fn prepass_vertex_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
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
    /// Direction toward the dominant celestial light and day/night strength.
    #[uniform(2)]
    pub(in crate::presentation) lighting: Vec4,
    /// Ambient irradiance colour and normalized strength.
    #[uniform(2)]
    pub(in crate::presentation) ambient: Vec4,
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

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TreeLeafRepresentation {
    TexturedMesh,
    AlphaCard,
}

#[derive(Component)]
pub(in crate::presentation) struct TreeTrunkLod;

#[derive(Resource, Clone, Copy, Default)]
pub(crate) struct TreeLodRenderOverride {
    pub(crate) lod: Option<u8>,
    pub(crate) leaf: Option<TreeLeafRepresentation>,
    pub(crate) projected_scale: Option<f32>,
}

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
                    "English oak scaffold {} cambered PBR leaves",
                    cluster.primary_group
                )),
                TreeLod(0),
                cluster_marker,
                cluster_aabb,
                TreeLeafRepresentation::TexturedMesh,
                Mesh3d(cluster.cambered_leaf_mesh.clone()),
                MeshMaterial3d(cached.leaf_material.clone()),
                tree_leaf_visibility(TreeLeafRepresentation::TexturedMesh, 1.0, cluster.radius),
            ));
            parent.spawn((
                Name::new(format!(
                    "English oak scaffold {} PBR leaf cards",
                    cluster.primary_group
                )),
                TreeLod(0),
                cluster_marker,
                cluster_aabb,
                TreeLeafRepresentation::AlphaCard,
                Mesh3d(cluster.leaf_card_mesh.clone()),
                MeshMaterial3d(cached.leaf_material.clone()),
                tree_leaf_visibility(TreeLeafRepresentation::AlphaCard, 1.0, cluster.radius),
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
        Option<&TreeLeafRepresentation>,
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
    let focal_scale =
        (focal / reference_focal * lod_override.projected_scale.unwrap_or(1.0)).clamp(0.25, 4.0);
    for (lod, cluster, leaf_representation, mut range, mut visibility) in &mut lods {
        if let Some(forced_lod) = lod_override.lod {
            let selected_leaf = match (leaf_representation, lod_override.leaf) {
                (
                    Some(TreeLeafRepresentation::TexturedMesh),
                    None | Some(TreeLeafRepresentation::TexturedMesh),
                ) => true,
                (
                    Some(TreeLeafRepresentation::AlphaCard),
                    Some(TreeLeafRepresentation::AlphaCard),
                ) => true,
                (Some(_), _) => false,
                (None, _) => true,
            };
            *visibility = if lod.0 == forced_lod && selected_leaf {
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
            *range = match leaf_representation {
                Some(TreeLeafRepresentation::TexturedMesh) => {
                    tree_leaf_visibility(TreeLeafRepresentation::TexturedMesh, focal_scale, radius)
                }
                Some(TreeLeafRepresentation::AlphaCard) => {
                    tree_leaf_visibility(TreeLeafRepresentation::AlphaCard, focal_scale, radius)
                }
                None => tree_projected_lod_visibility(lod.0, focal_scale, radius),
            };
        }
    }
    for (mut range, mut visibility) in &mut trunks {
        if let Some(forced_lod) = lod_override.lod {
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
const TREE_LEAF_CARD_SHADER: &str = "shaders/tactical_tree_leaf_card.wgsl";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canopy_ao_tracks_crown_scale_without_changing_the_accepted_oak() {
        let oak = canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres);
        let hazel = canopy_ao_strength(COMMON_HAZEL_PARAMETERS.crown_radius_metres);

        assert!((oak - 0.62).abs() < 0.01);
        assert!((hazel - 0.22).abs() < 0.01);
        assert!(hazel < oak * 0.4);
    }
}
