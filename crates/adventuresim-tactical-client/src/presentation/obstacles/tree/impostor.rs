mod card_mesh;
mod raster;

use card_mesh::*;
use raster::*;

use super::super::super::*;
use super::geometry::*;

pub(in crate::presentation) const TREE_IMPOSTOR_BAKE_VERSION: u32 = 15;
pub(in crate::presentation) const TREE_IMPOSTOR_RENDER_METHOD: &str =
    "deterministic software triangle render with species-calibrated crown coverage";
const WHOLE_TREE_RUNTIME_WIDTH_SCALE: f32 = 1.0;
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
}

#[derive(Clone, Copy)]
pub(in crate::presentation) struct TreeBakeStyle {
    pub(in crate::presentation) bark_srgb: [f32; 3],
    pub(in crate::presentation) leaf_srgb: [f32; 3],
    pub(in crate::presentation) crown_radius_metres: f32,
    /// Sampling strides for aggregate LODs 1 through 4. Keeping this in the
    /// species style prevents one leaf-cluster topology from defining every
    /// crown's bake density.
    pub(in crate::presentation) aggregate_leaf_strides: [u8; 4],
    /// Linear proxy scales for aggregate LODs 1 through 4. These are
    /// calibrated together with the strides so projected crown occupancy,
    /// rather than raw retained-leaf count, stays close to the source.
    pub(in crate::presentation) aggregate_leaf_scales: [f32; 4],
    /// Oak's low lateral scaffolds need world-vertical near cards so their
    /// projected crown does not rotate downward around each shoot axis.
    pub(in crate::presentation) lod1_world_vertical: bool,
    /// A small horizontal-only runtime correction preserves the outer crown
    /// envelope after source depth is flattened into the near card.
    pub(in crate::presentation) lod1_runtime_width_scale: f32,
}

impl TreeBakeStyle {
    fn aggregate_leaf_recipe(self, lod: u8) -> (usize, f32) {
        let index = usize::from(lod.saturating_sub(1)).min(3);
        (
            usize::from(self.aggregate_leaf_strides[index]).max(1),
            self.aggregate_leaf_scales[index].max(0.01),
        )
    }
}

pub(in crate::presentation) const OAK_TREE_BAKE_STYLE: TreeBakeStyle = TreeBakeStyle {
    bark_srgb: [116.0, 103.0, 82.0],
    leaf_srgb: super::materials::OAK_LEAF_IMPOSTOR_BASE_SRGB,
    crown_radius_metres: ENGLISH_OAK_PARAMETERS.crown_radius_metres,
    aggregate_leaf_strides: [3, 4, 8, 16],
    // Oak's terminal flushes contain many small leaves. Square-root area
    // replacement made their overlapping coarse proxies nearly solid, so the
    // scale falls progressively below pure area preservation.
    aggregate_leaf_scales: [1.0, 1.72, 2.16, 2.65],
    lod1_world_vertical: true,
    lod1_runtime_width_scale: 1.10,
};

pub(in crate::presentation) const BEECH_TREE_BAKE_STYLE: TreeBakeStyle = TreeBakeStyle {
    bark_srgb: [145.0, 145.0, 135.0],
    leaf_srgb: [91.0, 119.0, 70.0],
    crown_radius_metres: COMMON_BEECH_PARAMETERS.crown_radius_metres,
    // Beech uses sparse eight-leaf spray proxies. Retaining multiple smaller
    // representatives per spray preserves its tall, continuous crown instead
    // of enlarging a few horizontal leaves into disconnected shelves.
    aggregate_leaf_strides: [2, 3, 4, 4],
    aggregate_leaf_scales: [1.05, 1.90, 2.25, 2.45],
    lod1_world_vertical: false,
    lod1_runtime_width_scale: 1.0,
};

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
    /// LOD1 cards group a contiguous run of secondary limbs from one primary
    /// crown sector. The production group IDs deliberately leave room between
    /// sectors, so the range never spills into a neighbouring sector.
    secondary_group_range: Option<(u16, u16)>,
    source_group: u16,
    minimum_branch_depth: u8,
}

impl TreeBakeCard {
    fn includes_branch(self, branch: &TreeBranchSegment) -> bool {
        match (self.secondary_group_range, self.primary_mask) {
            (Some((first, last)), _) => {
                (first..=last).contains(&branch.secondary_group)
                    && branch.depth >= self.minimum_branch_depth
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
        match (self.secondary_group_range, self.primary_mask) {
            (Some((first, last)), _) => (first..=last).contains(&leaf.secondary_group),
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
    bake_tree_lod_with_style(seed, branches, leaves, lod, OAK_TREE_BAKE_STYLE)
}

pub(in crate::presentation) fn bake_tree_lod_with_style(
    seed: u64,
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    lod: u8,
    style: TreeBakeStyle,
) -> TreeLodBake {
    let started = web_time::Instant::now();
    let cards = tree_bake_cards_with_style(seed, branches, leaves, lod, style);
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

    for (index, card) in cards.iter().copied().enumerate() {
        let tile_x = index as u32 % columns;
        let tile_y = index as u32 / columns;
        render_tree_card(
            card,
            branches,
            leaves,
            lod,
            tile_size,
            atlas_width,
            atlas_height,
            tile_x,
            tile_y,
            &mut pixels,
            style,
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
                    * match lod {
                        1 => style.lod1_runtime_width_scale,
                        4 => WHOLE_TREE_RUNTIME_WIDTH_SCALE,
                        _ => 1.0,
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
    let bake = TreeLodBake {
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
    };
    tracing::info!(
        lod,
        cards = bake.provenance.records.len(),
        atlas_width,
        atlas_height,
        elapsed_ms = started.elapsed().as_millis(),
        "Generated tactical tree impostor atlas"
    );
    bake
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
    tree_bake_cards_with_style(seed, branches, leaves, lod, OAK_TREE_BAKE_STYLE)
}

fn tree_bake_cards_with_style(
    seed: u64,
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    lod: u8,
    style: TreeBakeStyle,
) -> Vec<TreeBakeCard> {
    let mut cards = Vec::new();
    match lod {
        1 => {
            // LOD1 used to create two cards for every secondary limb. An oak
            // therefore baked roughly 210 cards, repeating a great deal of
            // overlapping foliage just as the player leaves detailed range.
            // Keep the same primary crown sectors but partition each into two
            // contiguous limb runs. Secondary limbs are generated in scaffold
            // order, making each run a stable local crown mass rather than a
            // random sampling of opposing sides of the tree. Two perpendicular
            // cards for each run preserve broad crown occupancy from an
            // oblique approach, producing four cards per primary sector.
            for primary_group in 0..TREE_PRIMARY_GROUP_COUNT {
                for (subcluster, secondary_group_range) in
                    lod1_macro_cluster_ranges(branches, primary_group)
                        .into_iter()
                        .enumerate()
                {
                    let axis =
                        lod1_macro_cluster_axis(branches, primary_group, secondary_group_range);
                    // All aggregate tiers inherit the source primary-sector
                    // orientation. This prevents each LOD from rotating the same
                    // crown mass into a visibly different silhouette at handoff.
                    let (card_up, frame_right, rotation_axis) = if style.lod1_world_vertical {
                        let horizontal_axis = Vec3::new(axis.x, 0.0, axis.z).normalize_or_zero();
                        let frame_right = if horizontal_axis.length_squared() > 0.25 {
                            horizontal_axis
                        } else {
                            Vec3::X
                        };
                        (Vec3::Y, frame_right, Vec3::Y)
                    } else {
                        let frame_reference = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
                        (axis, axis.cross(frame_reference).normalize(), axis)
                    };
                    let subcluster_key = u16::from(primary_group)
                        * LOD1_MACRO_SUBCLUSTERS_PER_PRIMARY as u16
                        + subcluster as u16;
                    let phase = unit_hash(splitmix64(seed ^ u64::from(subcluster_key)))
                        * core::f32::consts::FRAC_PI_2;
                    for facing in 0..LOD1_CARD_FACINGS_PER_SUBCLUSTER {
                        let source_group = u16::from(primary_group)
                            * LOD1_MACRO_CARDS_PER_PRIMARY as u16
                            + subcluster as u16 * LOD1_CARD_FACINGS_PER_SUBCLUSTER as u16
                            + facing as u16;
                        let right = bevy::math::Quat::from_axis_angle(
                            rotation_axis,
                            phase + facing as f32 * core::f32::consts::FRAC_PI_2,
                        ) * frame_right;
                        cards.push(fit_tree_bake_card(
                            branches,
                            leaves,
                            right,
                            card_up,
                            1 << primary_group,
                            Some(secondary_group_range),
                            source_group,
                            3,
                        ));
                    }
                }
            }
        }
        2 => {
            for group in 0..TREE_PRIMARY_GROUP_COUNT {
                let right = crown_group_right(branches, group);
                for facing in 0..2 {
                    let right = if facing == 0 {
                        right
                    } else {
                        Vec3::Y.cross(right).normalize()
                    };
                    cards.push(fit_tree_bake_card(
                        branches,
                        leaves,
                        right,
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
                let right = crown_group_right(branches, group);
                for facing in 0..2 {
                    let right = if facing == 0 {
                        right
                    } else {
                        Vec3::Y.cross(right).normalize()
                    };
                    let mut card = fit_tree_bake_card(
                        branches,
                        leaves,
                        right,
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
                    secondary_group_range: None,
                    source_group: view,
                    minimum_branch_depth: 0,
                });
            }
        }
        _ => unreachable!("tree LOD is bounded"),
    }
    cards
}

const LOD1_MACRO_SUBCLUSTERS_PER_PRIMARY: usize = 2;
const LOD1_CARD_FACINGS_PER_SUBCLUSTER: usize = 2;
const LOD1_MACRO_CARDS_PER_PRIMARY: usize =
    LOD1_MACRO_SUBCLUSTERS_PER_PRIMARY * LOD1_CARD_FACINGS_PER_SUBCLUSTER;

/// Divides one primary crown sector into deterministic, similarly sized
/// secondary-limb runs. A malformed or intentionally sparse morphology can
/// use fewer than two runs, but the mature playable-tree recipes supply both
/// in each of their seven primary sectors.
fn lod1_macro_cluster_ranges(branches: &[TreeBranchSegment], primary_group: u8) -> Vec<(u16, u16)> {
    let mut groups = branches
        .iter()
        .filter(|branch| branch.depth == 2 && branch.primary_group == primary_group)
        .map(|branch| branch.secondary_group)
        .collect::<Vec<_>>();
    groups.sort_unstable();
    groups.dedup();

    let cluster_count = groups.len().min(LOD1_MACRO_SUBCLUSTERS_PER_PRIMARY);
    (0..cluster_count)
        .map(|subcluster| {
            let first = subcluster * groups.len() / cluster_count;
            let last = (subcluster + 1) * groups.len() / cluster_count - 1;
            (groups[first], groups[last])
        })
        .collect()
}

fn lod1_macro_cluster_axis(
    branches: &[TreeBranchSegment],
    primary_group: u8,
    secondary_group_range: (u16, u16),
) -> Vec3 {
    let (first, last) = secondary_group_range;
    let axis = branches
        .iter()
        .filter(|branch| {
            branch.depth == 2
                && branch.primary_group == primary_group
                && (first..=last).contains(&branch.secondary_group)
        })
        .map(|branch| {
            let direction = branch.end - branch.start;
            direction.normalize_or_zero() * direction.length()
        })
        .sum::<Vec3>();
    if axis.length_squared() > 0.01 {
        axis.normalize()
    } else {
        crown_group_right(branches, primary_group)
    }
}

fn crown_group_right(branches: &[TreeBranchSegment], group: u8) -> Vec3 {
    branches
        .iter()
        .filter(|branch| branch.depth == 1 && branch.primary_group == group)
        .max_by(|left, right| {
            left.end
                .xz()
                .length_squared()
                .total_cmp(&right.end.xz().length_squared())
        })
        .map(|branch| {
            let horizontal = branch.end.xz().normalize_or_zero();
            if horizontal.length_squared() > 0.25 {
                Vec3::new(horizontal.x, 0.0, horizontal.y)
            } else {
                Vec3::X
            }
        })
        .unwrap_or(Vec3::X)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn fit_tree_bake_card(
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    right: Vec3,
    up: Vec3,
    primary_mask: u8,
    secondary_group_range: Option<(u16, u16)>,
    source_group: u16,
    minimum_branch_depth: u8,
) -> TreeBakeCard {
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
        secondary_group_range,
        source_group,
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
        secondary_group_range,
        source_group,
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
        // Dense real-world forest profiling showed that the prior ranges kept
        // tens of millions of live leaf vertices active after their detail
        // was no longer readable. Preserve overlap, but hand off one botanical
        // order earlier at every stage.
        let base = [6.0..8.0, 12.0..16.0, 24.0..32.0, 50.0..60.0][index].clone();
        let spatial_scale = if index < 3 { cluster_scale } else { 1.0 };
        (base.start * focal_scale * spatial_scale)..(base.end * focal_scale * spatial_scale)
    };
    let delayed_transition_out = |index: usize| {
        let handoff = transition(index);
        let width = handoff.end - handoff.start;
        handoff.end..(handoff.end + width)
    };
    let (start, end) = match lod {
        0 => (0.0..0.0, transition(0)),
        // Keep the outgoing aggregate fully visible while the next one
        // dithers in, then fade it out over an equally wide trailing band.
        // The baked silhouettes are not pixel-identical, so exact
        // complementary dithering can otherwise expose crown holes.
        1 => (transition(0), delayed_transition_out(1)),
        2 => (transition(1), delayed_transition_out(2)),
        3 => (transition(2), delayed_transition_out(3)),
        4 => (transition(3), (190.0 * focal_scale)..(200.0 * focal_scale)),
        _ => unreachable!("tree LOD is bounded"),
    };
    VisibilityRange {
        start_margin: start,
        end_margin: end,
        // Every representation must measure from the same anchor. Using each
        // mesh AABB made a camera beside the trunk appear close to an
        // aggregate whole-tree mesh but far from every crown cluster. The
        // aggregate then hid before the detailed leaves became eligible,
        // producing a leafless tree. All tree LODs share the obstacle origin.
        use_aabb: false,
    }
}

pub(in crate::presentation) fn tree_leaf_visibility(
    representation: TreeLeafRepresentation,
    focal_scale: f32,
    cluster_radius: f32,
) -> VisibilityRange {
    let cluster_scale = (cluster_radius / 3.5).sqrt().clamp(0.65, 1.35);
    let leaf_transition = (3.5 * focal_scale * cluster_scale)..(5.0 * focal_scale * cluster_scale);
    let aggregate_transition =
        tree_projected_lod_visibility(0, focal_scale, cluster_radius).end_margin;
    let (start_margin, end_margin) = match representation {
        TreeLeafRepresentation::TexturedMesh => (0.0..0.0, leaf_transition.clone()),
        TreeLeafRepresentation::AlphaCard => (leaf_transition, aggregate_transition),
    };
    VisibilityRange {
        start_margin,
        end_margin,
        use_aabb: false,
    }
}

pub(in crate::presentation) fn tree_trunk_visibility() -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: 50.0..60.0,
        use_aabb: false,
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
        assert_eq!(first.provenance.bake_version, TREE_IMPOSTOR_BAKE_VERSION);
        assert_eq!(first.image.data, second.image.data);
        assert!((WHOLE_TREE_RUNTIME_WIDTH_SCALE - 1.0).abs() < f32::EPSILON);
        assert!((WHOLE_TREE_BAKE_EXPOSURE - 0.91).abs() < f32::EPSILON);
        assert_eq!(
            tree_impostor_material(42, 4, Handle::default()).alpha_mode(),
            AlphaMode::AlphaToCoverage
        );
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
        assert!(shader.contains("let horizontal_scale = length(world_from_local[0].xyz)"));
        assert!(shader.contains("vertex.position.x * horizontal_scale"));
        assert!(shader.contains("vertex.position.y * vertical_scale"));
        assert!(
            shader.contains("in.visibility_range_dither <= -8 || in.visibility_range_dither > 8")
        );
        assert!(!shader.contains("visibility_alpha"));
        assert!(!shader.contains("pbr_functions::visibility_range_dither("));
        assert!(shader.contains("abs(dot(normalize(in.world_normal), light_direction))"));
        assert!(shader.contains("let normal_light = 0.25 + card_light * 0.75"));
    }

    #[test]
    fn complete_runtime_tree_bake_suite_preserves_every_lod() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let bakes = (1..=4)
            .map(|lod| bake_tree_lod(42, &branches, &leaves, lod))
            .collect::<Vec<_>>();
        assert_eq!(bakes.len(), 4);
        assert!(bakes.iter().all(|bake| !bake.provenance.records.is_empty()));
        assert!(bakes.windows(2).all(|pair| pair[0].lod < pair[1].lod));
    }

    #[test]
    fn tree_lods_collapse_one_botanical_order_at_a_time() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let expected_cards = [28, 14, 14, 8];
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
    fn lod1_macro_cluster_cards_are_bounded_vertex_light_and_deterministic() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let first = bake_tree_lod(42, &branches, &leaves, 1);
        let second = bake_tree_lod(42, &branches, &leaves, 1);

        assert_eq!(first.provenance.records.len(), 28);
        assert_eq!(first.mesh.count_vertices(), 112);
        assert_eq!(
            first
                .provenance
                .records
                .iter()
                .map(|record| record.source_group)
                .collect::<Vec<_>>(),
            (0..28).collect::<Vec<_>>()
        );
        assert!(first.provenance.records.iter().all(|record| {
            record.source_leaf_count > 0
                && record.source_branch_count > 0
                && record.opaque_pixel_count > 0
        }));
        assert_eq!(first.image.data, second.image.data);
    }

    #[test]
    fn adjacent_aggregate_lods_keep_primary_crown_planes_aligned() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let small_branches = tree_bake_cards(42, &branches, &leaves, 2);
        let crown_branches = tree_bake_cards(42, &branches, &leaves, 3);

        for group in 0..TREE_PRIMARY_GROUP_COUNT {
            let lower = small_branches
                .iter()
                .filter(|card| card.primary_mask == 1 << group)
                .collect::<Vec<_>>();
            let upper = crown_branches
                .iter()
                .filter(|card| card.primary_mask == 1 << group)
                .collect::<Vec<_>>();
            assert_eq!(lower.len(), 2);
            assert_eq!(upper.len(), 2);
            assert!(
                lower
                    .iter()
                    .zip(upper)
                    .all(|(a, b)| a.right.dot(b.right) > 0.999)
            );
        }
    }

    #[test]
    fn leaf_crossfade_is_exact_and_aggregate_handoffs_overlap() {
        let cambered_leaf = tree_leaf_visibility(TreeLeafRepresentation::TexturedMesh, 1.0, 3.5);
        let alpha_leaf = tree_leaf_visibility(TreeLeafRepresentation::AlphaCard, 1.0, 3.5);
        assert_eq!(cambered_leaf.end_margin, alpha_leaf.start_margin);
        assert_eq!(alpha_leaf.end_margin, tree_lod_visibility(1).start_margin);
        for lod in 0..4 {
            let current = tree_lod_visibility(lod);
            let next = tree_lod_visibility(lod + 1);
            if lod == 0 {
                assert_eq!(current.end_margin, next.start_margin);
            } else {
                assert_eq!(current.end_margin.start, next.start_margin.end);
                assert!(current.end_margin.end > next.start_margin.end);
            }
            assert!(!current.is_abrupt());
        }
    }

    #[test]
    fn production_tree_lod_ranges_handoff_before_detail_is_subpixel() {
        let expected = [
            (0.0..0.0, 6.0..8.0),
            (6.0..8.0, 16.0..20.0),
            (12.0..16.0, 32.0..40.0),
            (24.0..32.0, 60.0..70.0),
            (50.0..60.0, 190.0..200.0),
        ];
        for (lod, (start, end)) in expected.into_iter().enumerate() {
            let range = tree_lod_visibility(lod as u8);
            assert_eq!(range.start_margin, start);
            assert_eq!(range.end_margin, end);
        }
        assert_eq!(
            tree_leaf_visibility(TreeLeafRepresentation::TexturedMesh, 1.0, 3.5).end_margin,
            3.5..5.0
        );
        assert_eq!(tree_trunk_visibility().end_margin, 50.0..60.0);
    }

    #[test]
    fn recursive_lod_preserves_safe_overlap_at_every_projected_scale() {
        for focal_scale in [0.55, 1.0, 2.4] {
            for radius in [1.8, 3.5, 6.0] {
                for lod in 0..4 {
                    let current = tree_projected_lod_visibility(lod, focal_scale, radius);
                    let next = tree_projected_lod_visibility(lod + 1, focal_scale, radius);
                    if lod == 0 {
                        assert_eq!(current.end_margin, next.start_margin);
                    } else {
                        assert_eq!(current.end_margin.start, next.start_margin.end);
                    }
                    assert!(!current.use_aabb && !next.use_aabb);
                }
            }
        }
    }
}
