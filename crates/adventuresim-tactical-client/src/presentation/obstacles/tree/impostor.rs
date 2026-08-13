mod card_mesh;
mod raster;

use card_mesh::*;
use raster::*;

use super::super::super::*;
use super::geometry::*;

pub(in crate::presentation) const TREE_IMPOSTOR_BAKE_VERSION: u32 = 6;
pub(in crate::presentation) const TREE_IMPOSTOR_RENDER_METHOD: &str =
    "deterministic software triangle render of exact production branch and leaf meshes";
const WHOLE_TREE_RUNTIME_WIDTH_SCALE: f32 = 0.95;
const WHOLE_TREE_BAKE_EXPOSURE: f32 = 0.91;

#[derive(Component, Clone, Debug)]
pub(crate) struct TreeImpostorProvenance {
    pub seed: u64,
    pub lod: u8,
    pub bake_version: u32,
    pub source_geometry_hash: u64,
    pub render_method: &'static str,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub records: Vec<TreeImpostorBakeRecord>,
}

#[derive(Clone, Debug)]
pub(crate) struct TreeImpostorBakeRecord {
    pub source_group: u16,
    pub source_leaf_count: u16,
    pub source_branch_count: u16,
    pub view_direction: Vec3,
    pub projected_bounds: Vec4,
    pub atlas_region: UVec4,
    pub opaque_pixel_count: u32,
    pub silhouette_centroid: Vec2,
}

pub(in crate::presentation) struct TreeLodBake {
    pub(in crate::presentation) lod: u8,
    pub(in crate::presentation) mesh: Mesh,
    pub(in crate::presentation) clusters: Vec<TreeLodClusterBake>,
    pub(in crate::presentation) image: Image,
    pub(in crate::presentation) provenance: TreeImpostorProvenance,
}

pub(in crate::presentation) struct TreeLodClusterBake {
    pub(in crate::presentation) primary_group: u8,
    pub(in crate::presentation) center: Vec3,
    pub(in crate::presentation) radius: f32,
    pub(in crate::presentation) mesh: Mesh,
}

pub(in crate::presentation) fn validate_tree_bake_provenance(provenance: &TreeImpostorProvenance) {
    debug_assert!(provenance.seed.count_ones() > 0);
    debug_assert!((1..=4).contains(&provenance.lod));
    debug_assert_eq!(provenance.bake_version, TREE_IMPOSTOR_BAKE_VERSION);
    debug_assert_ne!(provenance.source_geometry_hash, 0);
    debug_assert_eq!(provenance.render_method, TREE_IMPOSTOR_RENDER_METHOD);
    debug_assert!(provenance.atlas_width > 0 && provenance.atlas_height > 0);
    debug_assert!(!provenance.records.is_empty());
    for record in &provenance.records {
        debug_assert!(record.source_leaf_count > 0);
        debug_assert!(record.source_branch_count > 0);
        debug_assert!(record.view_direction.is_finite());
        debug_assert!(record.projected_bounds.is_finite());
        debug_assert!(record.atlas_region.z > 0 && record.atlas_region.w > 0);
        debug_assert!(record.opaque_pixel_count > 0);
        debug_assert!(record.silhouette_centroid.is_finite());
        let _ = record.source_group;
    }
}

#[derive(Clone, Copy)]
pub(in crate::presentation) struct TreeBakeCard {
    center: Vec3,
    right: Vec3,
    up: Vec3,
    width: f32,
    height: f32,
    primary_mask: u8,
    secondary_group: Option<u16>,
    source_group: u16,
    primary_group: u8,
    minimum_branch_depth: u8,
}

impl TreeBakeCard {
    fn includes_branch(self, branch: &TreeBranchSegment) -> bool {
        match (self.secondary_group, self.primary_mask) {
            (Some(group), _) => {
                branch.secondary_group == group && branch.depth >= self.minimum_branch_depth
            }
            (None, mask) if mask != 0 => {
                branch.primary_group < 8
                    && mask & (1 << branch.primary_group) != 0
                    && branch.depth >= self.minimum_branch_depth
            }
            (None, _) => true,
        }
    }

    fn includes_leaf(self, leaf: &TreeLeaf) -> bool {
        match (self.secondary_group, self.primary_mask) {
            (Some(group), _) => leaf.secondary_group == group,
            (None, mask) if mask != 0 => mask & (1 << leaf.primary_group) != 0,
            (None, _) => true,
        }
    }

    fn normal(self) -> Vec3 {
        self.right.cross(self.up).normalize()
    }
}

pub(in crate::presentation) fn bake_tree_lod(
    seed: u64,
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    lod: u8,
) -> TreeLodBake {
    let cards = tree_bake_cards(seed, branches, leaves, lod);
    let tile_size = match lod {
        1 => 96,
        2 => 144,
        3 => 192,
        4 => 320,
        _ => unreachable!("only aggregate tree LODs are baked"),
    };
    let columns = (cards.len() as f32).sqrt().ceil() as u32;
    let rows = (cards.len() as u32).div_ceil(columns);
    let atlas_width = columns * tile_size;
    let atlas_height = rows * tile_size;
    let mut pixels = vec![0_u8; (atlas_width * atlas_height * 4) as usize];
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let source_geometry_hash = tree_source_geometry_hash(branches, leaves);
    let mut records = Vec::with_capacity(cards.len());
    let mut card_uvs = Vec::with_capacity(cards.len());

    for (index, card) in cards.iter().copied().enumerate() {
        let tile_x = index as u32 % columns;
        let tile_y = index as u32 / columns;
        render_tree_card(
            card,
            branches,
            leaves,
            tile_size,
            atlas_width,
            atlas_height,
            tile_x,
            tile_y,
            &mut pixels,
        );
        let (opaque_pixel_count, silhouette_centroid) = tree_tile_alpha_stats(
            &pixels,
            atlas_width,
            tile_x * tile_size,
            tile_y * tile_size,
            tile_size,
        );
        let uv_min = Vec2::new(
            tile_x as f32 * tile_size as f32 / atlas_width as f32,
            tile_y as f32 * tile_size as f32 / atlas_height as f32,
        );
        let uv_max = Vec2::new(
            (tile_x + 1) as f32 * tile_size as f32 / atlas_width as f32,
            (tile_y + 1) as f32 * tile_size as f32 / atlas_height as f32,
        );
        card_uvs.push((uv_min, uv_max));
        if lod != 4 || index == 0 {
            let (mesh_uv_min, mesh_uv_max) = if lod == 4 {
                (Vec2::ZERO, Vec2::ONE)
            } else {
                (uv_min, uv_max)
            };
            append_tree_card_with_uv(
                card.center,
                card.right,
                card.up,
                card.width
                    * if lod == 4 {
                        WHOLE_TREE_RUNTIME_WIDTH_SCALE
                    } else {
                        1.0
                    },
                card.height,
                mesh_uv_min,
                mesh_uv_max,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
            );
        }
        records.push(TreeImpostorBakeRecord {
            source_group: card.source_group,
            source_leaf_count: leaves
                .iter()
                .filter(|leaf| card.includes_leaf(leaf))
                .count() as u16,
            source_branch_count: branches
                .iter()
                .filter(|branch| card.includes_branch(branch))
                .count() as u16,
            view_direction: card.normal(),
            projected_bounds: Vec4::new(card.center.x, card.center.y, card.width, card.height),
            atlas_region: UVec4::new(tile_x * tile_size, tile_y * tile_size, tile_size, tile_size),
            opaque_pixel_count,
            silhouette_centroid,
        });
    }

    if lod == 4 {
        // The aggregate crown has fewer overlapping depth layers than LOD3.
        // A bounded bake-space exposure restores comparable interior depth
        // without a runtime material multiplier or changing alpha coverage.
        for pixel in pixels.chunks_exact_mut(4) {
            if pixel[3] != 0 {
                pixel[0] = (f32::from(pixel[0]) * WHOLE_TREE_BAKE_EXPOSURE) as u8;
                pixel[1] = (f32::from(pixel[1]) * WHOLE_TREE_BAKE_EXPOSURE) as u8;
                pixel[2] = (f32::from(pixel[2]) * WHOLE_TREE_BAKE_EXPOSURE) as u8;
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    let clusters = if lod < 4 {
        (0..TREE_PRIMARY_GROUP_COUNT)
            .filter_map(|primary_group| {
                let (center, radius) = tree_primary_group_sphere(branches, leaves, primary_group)?;
                Some(TreeLodClusterBake {
                    primary_group,
                    center,
                    radius,
                    mesh: tree_cluster_card_mesh(&cards, &card_uvs, primary_group),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let image = Image::new(
        Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    TreeLodBake {
        lod,
        mesh,
        clusters,
        image,
        provenance: TreeImpostorProvenance {
            seed,
            lod,
            bake_version: TREE_IMPOSTOR_BAKE_VERSION,
            source_geometry_hash: source_geometry_hash ^ u64::from(TREE_IMPOSTOR_BAKE_VERSION),
            render_method: TREE_IMPOSTOR_RENDER_METHOD,
            atlas_width,
            atlas_height,
            records,
        },
    }
}

fn tree_cluster_card_mesh(
    cards: &[TreeBakeCard],
    card_uvs: &[(Vec2, Vec2)],
    primary_group: u8,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for (card, (uv_min, uv_max)) in cards
        .iter()
        .zip(card_uvs)
        .filter(|(card, _)| card.primary_group == primary_group)
    {
        append_tree_card_with_uv(
            card.center,
            card.right,
            card.up,
            card.width,
            card.height,
            *uv_min,
            *uv_max,
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

fn tree_primary_group_sphere(
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    primary_group: u8,
) -> Option<(Vec3, f32)> {
    let group_branches = branches
        .iter()
        .filter(|branch| branch.depth > 0 && branch.primary_group == primary_group)
        .collect::<Vec<_>>();
    let group_leaves = leaves
        .iter()
        .filter(|leaf| leaf.primary_group == primary_group)
        .collect::<Vec<_>>();
    if group_branches.is_empty() && group_leaves.is_empty() {
        return None;
    }
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for branch in &group_branches {
        let extent = Vec3::splat(branch.start_radius.max(branch.end_radius));
        minimum = minimum.min(branch.start - extent).min(branch.end - extent);
        maximum = maximum.max(branch.start + extent).max(branch.end + extent);
    }
    for leaf in &group_leaves {
        let extent = Vec3::splat(leaf.length.max(leaf.width) * 0.65);
        minimum = minimum.min(leaf.center - extent);
        maximum = maximum.max(leaf.center + extent);
    }
    let center = (minimum + maximum) * 0.5;
    let radius = group_branches
        .iter()
        .flat_map(|branch| {
            [
                branch.start.distance(center) + branch.start_radius,
                branch.end.distance(center) + branch.end_radius,
            ]
        })
        .chain(
            group_leaves
                .iter()
                .map(|leaf| leaf.center.distance(center) + leaf.length.max(leaf.width) * 0.65),
        )
        .fold(0.0_f32, f32::max);
    Some((center, radius.max(0.01)))
}

fn tree_tile_alpha_stats(
    pixels: &[u8],
    atlas_width: u32,
    tile_x: u32,
    tile_y: u32,
    tile_size: u32,
) -> (u32, Vec2) {
    let mut count = 0_u32;
    let mut sum = Vec2::ZERO;
    for y in 0..tile_size {
        for x in 0..tile_size {
            let pixel = (((tile_y + y) * atlas_width + tile_x + x) * 4) as usize;
            if pixels[pixel + 3] != 0 {
                count += 1;
                sum += Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / tile_size as f32;
            }
        }
    }
    (count, sum / count.max(1) as f32)
}

pub(in crate::presentation) fn tree_bake_cards(
    seed: u64,
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    lod: u8,
) -> Vec<TreeBakeCard> {
    let mut cards = Vec::new();
    match lod {
        1 => {
            let mut secondary_groups = branches
                .iter()
                .filter(|branch| branch.depth == 2)
                .map(|branch| branch.secondary_group)
                .collect::<Vec<_>>();
            secondary_groups.sort_unstable();
            secondary_groups.dedup();
            for group in secondary_groups {
                let axis = branches
                    .iter()
                    .find(|branch| {
                        branch.depth == 2 && branch.secondary_group == group && branch.is_limb_tip
                    })
                    .map(|branch| (branch.end - branch.start).normalize())
                    .unwrap_or(Vec3::Y);
                let phase = unit_hash(splitmix64(seed ^ u64::from(group))) * core::f32::consts::TAU;
                for facing in 0..3 {
                    let angle = phase + facing as f32 * core::f32::consts::FRAC_PI_3;
                    cards.push(fit_tree_bake_card(
                        branches,
                        leaves,
                        Vec3::new(angle.cos(), 0.0, angle.sin()),
                        axis,
                        0,
                        Some(group),
                        group * 3 + facing,
                        3,
                    ));
                }
            }
        }
        2 => {
            for group in 0..TREE_PRIMARY_GROUP_COUNT {
                let phase = unit_hash(splitmix64(seed ^ u64::from(group) ^ 0x4a17))
                    * core::f32::consts::TAU;
                for facing in 0..2 {
                    let angle = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    cards.push(fit_tree_bake_card(
                        branches,
                        leaves,
                        Vec3::new(angle.cos(), 0.0, angle.sin()),
                        Vec3::Y,
                        1 << group,
                        None,
                        u16::from(group) * 2 + facing,
                        2,
                    ));
                }
            }
        }
        3 => {
            for group in 0..TREE_PRIMARY_GROUP_COUNT {
                let phase = crown_group_phase(seed, group);
                for facing in 0..2 {
                    let angle = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    let mut card = fit_tree_bake_card(
                        branches,
                        leaves,
                        Vec3::new(angle.cos(), 0.0, angle.sin()),
                        Vec3::Y,
                        1 << group,
                        None,
                        u16::from(group) * 2 + facing,
                        1,
                    );
                    // Each card renders its assigned primary crown sectors;
                    // overlapping sector cards retain parallax volume.
                    card.width *= 1.02;
                    card.height *= 1.02;
                    cards.push(card);
                }
            }
        }
        4 => {
            let bounds = tree_crown_bounds(branches, |_| true);
            let center = bounds.center();
            let width = (bounds.maximum.x - bounds.minimum.x)
                .max(bounds.maximum.z - bounds.minimum.z)
                + 0.6;
            let height = bounds.maximum.y - bounds.minimum.y + 0.5;
            for view in 0..8_u16 {
                let angle = view as f32 * core::f32::consts::TAU / 8.0;
                cards.push(TreeBakeCard {
                    center,
                    right: Vec3::new(angle.cos(), 0.0, angle.sin()),
                    up: Vec3::Y,
                    width,
                    height,
                    primary_mask: 0,
                    secondary_group: None,
                    source_group: view,
                    primary_group: u8::MAX,
                    minimum_branch_depth: 0,
                });
            }
        }
        _ => unreachable!("tree LOD is bounded"),
    }
    cards
}

pub(in crate::presentation) fn crown_group_phase(seed: u64, group: u8) -> f32 {
    unit_hash(splitmix64(seed ^ u64::from(group) ^ 0x7c31)) * core::f32::consts::TAU
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn fit_tree_bake_card(
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    right: Vec3,
    up: Vec3,
    primary_mask: u8,
    secondary_group: Option<u16>,
    source_group: u16,
    minimum_branch_depth: u8,
) -> TreeBakeCard {
    let primary_group = secondary_group.map_or_else(
        || {
            if primary_mask.count_ones() == 1 {
                primary_mask.trailing_zeros() as u8
            } else {
                u8::MAX
            }
        },
        |group| (group / TREE_SECONDARY_GROUP_STRIDE) as u8,
    );
    let up = up.normalize();
    let right = (right - up * right.dot(up)).normalize_or_zero();
    let right = if right.length_squared() < 0.25 {
        Vec3::X
    } else {
        right
    };
    let probe = TreeBakeCard {
        center: Vec3::ZERO,
        right,
        up,
        width: 1.0,
        height: 1.0,
        primary_mask,
        secondary_group,
        source_group,
        primary_group,
        minimum_branch_depth,
    };
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let normal = right.cross(up).normalize();
    let mut minimum_depth = f32::INFINITY;
    let mut maximum_depth = f32::NEG_INFINITY;
    for branch in branches
        .iter()
        .filter(|branch| probe.includes_branch(branch))
    {
        for point in [branch.start, branch.end] {
            let projected = Vec2::new(point.dot(right), point.dot(up));
            let radius = branch.start_radius.max(branch.end_radius);
            min = min.min(projected - Vec2::splat(radius));
            max = max.max(projected + Vec2::splat(radius));
            let depth = point.dot(normal);
            minimum_depth = minimum_depth.min(depth - radius);
            maximum_depth = maximum_depth.max(depth + radius);
        }
    }
    for leaf in leaves.iter().filter(|leaf| probe.includes_leaf(leaf)) {
        let radius = leaf.length.max(leaf.width) * 0.65;
        let projected = Vec2::new(leaf.center.dot(right), leaf.center.dot(up));
        min = min.min(projected - Vec2::splat(radius));
        max = max.max(projected + Vec2::splat(radius));
        let depth = leaf.center.dot(normal);
        minimum_depth = minimum_depth.min(depth - radius);
        maximum_depth = maximum_depth.max(depth + radius);
    }
    let margin = Vec2::splat(0.08) + (max - min) * 0.035;
    min -= margin;
    max += margin;
    let projected_center = (min + max) * 0.5;
    TreeBakeCard {
        center: right * projected_center.x
            + up * projected_center.y
            + normal * ((minimum_depth + maximum_depth) * 0.5),
        right,
        up,
        width: max.x - min.x,
        height: max.y - min.y,
        primary_mask,
        secondary_group,
        source_group,
        primary_group,
        minimum_branch_depth,
    }
}

pub(in crate::presentation) fn tree_source_geometry_hash(
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in branches.iter().flat_map(|branch| {
        [
            branch.start.x,
            branch.start.y,
            branch.start.z,
            branch.end.x,
            branch.end.y,
            branch.end.z,
            branch.start_radius,
            branch.end_radius,
        ]
    }) {
        hash ^= u64::from(value.to_bits());
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    for leaf in leaves {
        for value in [
            leaf.petiole_start.x,
            leaf.petiole_start.y,
            leaf.petiole_start.z,
            leaf.center.x,
            leaf.center.y,
            leaf.center.z,
            leaf.length,
            leaf.width,
            f32::from(leaf.shoot_id),
        ] {
            hash ^= u64::from(value.to_bits());
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn tree_impostor_material(
    seed: u64,
    lod: u8,
    baked_color: Handle<Image>,
) -> TacticalTreeImpostorMaterial {
    TacticalTreeImpostorMaterial {
        baked_color,
        parameters: Vec4::new(lod as f32, unit_hash(seed), 0.08 + lod as f32 * 0.018, 1.0),
        lighting: Vec3::new(0.35, 0.86, 0.25).normalize().extend(1.0),
        ambient: Vec4::new(1.0, 1.0, 1.0, 0.28),
    }
}

pub(in crate::presentation) fn tree_lod_visibility(lod: u8) -> VisibilityRange {
    tree_projected_lod_visibility(lod, 1.0, 3.5)
}

pub(in crate::presentation) fn tree_projected_lod_visibility(
    lod: u8,
    focal_scale: f32,
    cluster_radius: f32,
) -> VisibilityRange {
    let cluster_scale = (cluster_radius / 3.5).sqrt().clamp(0.65, 1.35);
    let transition = |index: usize| {
        let base = [22.0..26.0, 36.0..42.0, 56.0..64.0, 84.0..96.0][index].clone();
        let spatial_scale = if index < 3 { cluster_scale } else { 1.0 };
        (base.start * focal_scale * spatial_scale)..(base.end * focal_scale * spatial_scale)
    };
    let (start, end) = match lod {
        0 => (0.0..0.0, transition(0)),
        1 => (transition(0), transition(1)),
        2 => (transition(1), transition(2)),
        3 => (transition(2), transition(3)),
        4 => (transition(3), (190.0 * focal_scale)..(200.0 * focal_scale)),
        _ => unreachable!("tree LOD is bounded"),
    };
    VisibilityRange {
        start_margin: start,
        end_margin: end,
        use_aabb: true,
    }
}

pub(in crate::presentation) fn tree_leaf_visibility(
    representation: TreeLeafRepresentation,
    focal_scale: f32,
    cluster_radius: f32,
) -> VisibilityRange {
    let cluster_scale = (cluster_radius / 3.5).sqrt().clamp(0.65, 1.35);
    let leaf_transition =
        (10.0 * focal_scale * cluster_scale)..(13.0 * focal_scale * cluster_scale);
    let aggregate_transition =
        tree_projected_lod_visibility(0, focal_scale, cluster_radius).end_margin;
    let (start_margin, end_margin) = match representation {
        TreeLeafRepresentation::TexturedMesh => (0.0..0.0, leaf_transition.clone()),
        TreeLeafRepresentation::AlphaCard => (leaf_transition, aggregate_transition),
    };
    VisibilityRange {
        start_margin,
        end_margin,
        use_aabb: true,
    }
}

pub(in crate::presentation) fn tree_trunk_visibility() -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: 84.0..96.0,
        use_aabb: true,
    }
}

pub(in crate::presentation) fn tree_lod_name(lod: u8, cards: bool) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn whole_tree_view_index(camera_direction: Vec2) -> usize {
        let angle = camera_direction.y.atan2(camera_direction.x);
        ((angle / core::f32::consts::TAU - 0.25).rem_euclid(1.0) * 8.0).round() as usize % 8
    }

    #[test]
    fn whole_tree_atlas_selection_matches_baked_view_normals() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let cards = tree_bake_cards(42, &branches, &leaves, 4);

        for step in 0..8 {
            let angle = step as f32 * core::f32::consts::TAU / 8.0;
            let camera_direction = Vec2::new(angle.cos(), angle.sin());
            let index = whole_tree_view_index(camera_direction);
            assert!(cards[index].normal().xz().dot(camera_direction) > 0.999);
        }
        assert_eq!(
            whole_tree_view_index(Vec2::splat(core::f32::consts::FRAC_1_SQRT_2)),
            7
        );
    }

    #[test]
    fn whole_tree_runtime_quad_stays_bounded_and_bake_is_deterministic() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let first = bake_tree_lod(42, &branches, &leaves, 4);
        let second = bake_tree_lod(42, &branches, &leaves, 4);

        assert_eq!(first.mesh.count_vertices(), 4);
        assert_eq!(first.provenance.records.len(), 8);
        assert_eq!(first.provenance.bake_version, 6);
        assert_eq!(first.image.data, second.image.data);
        assert!((WHOLE_TREE_RUNTIME_WIDTH_SCALE - 0.95).abs() < f32::EPSILON);
        assert!((WHOLE_TREE_BAKE_EXPOSURE - 0.91).abs() < f32::EPSILON);
        assert!(
            first
                .provenance
                .records
                .iter()
                .all(|record| record.opaque_pixel_count > 0)
        );

        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_tree_impostor.wgsl"
        ));
        assert!(shader.contains("pbr_functions::visibility_range_dither("));
    }

    #[test]
    fn tree_lods_collapse_one_botanical_order_at_a_time() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let expected_cards = [348, 14, 14, 8];
        for (index, expected) in expected_cards.into_iter().enumerate() {
            assert_eq!(
                tree_bake_cards(42, &branches, &leaves, index as u8 + 1).len(),
                expected
            );
        }
        let branch_vertices = (0..=3)
            .rev()
            .map(|depth| procedural_tree_branch_mesh(&branches, depth).count_vertices())
            .collect::<Vec<_>>();
        assert!(branch_vertices.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn tree_lod_crossfades_share_exact_transition_margins() {
        let cambered_leaf = tree_leaf_visibility(TreeLeafRepresentation::TexturedMesh, 1.0, 3.5);
        let alpha_leaf = tree_leaf_visibility(TreeLeafRepresentation::AlphaCard, 1.0, 3.5);
        assert_eq!(cambered_leaf.end_margin, alpha_leaf.start_margin);
        assert_eq!(alpha_leaf.end_margin, tree_lod_visibility(1).start_margin);
        for lod in 0..4 {
            let current = tree_lod_visibility(lod);
            let next = tree_lod_visibility(lod + 1);
            assert_eq!(current.end_margin, next.start_margin);
            assert!(!current.is_abrupt());
        }
    }

    #[test]
    fn recursive_lod_preserves_shared_boundaries_at_every_projected_scale() {
        for focal_scale in [0.55, 1.0, 2.4] {
            for radius in [1.8, 3.5, 6.0] {
                for lod in 0..4 {
                    let current = tree_projected_lod_visibility(lod, focal_scale, radius);
                    let next = tree_projected_lod_visibility(lod + 1, focal_scale, radius);
                    assert_eq!(current.end_margin, next.start_margin);
                    assert!(current.use_aabb && next.use_aabb);
                }
            }
        }
    }
}
