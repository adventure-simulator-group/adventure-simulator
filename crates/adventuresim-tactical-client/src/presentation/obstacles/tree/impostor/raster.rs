#[cfg(test)]
use super::OAK_TREE_BAKE_STYLE;
use super::{TreeBakeCard, TreeBakeStyle};
use crate::presentation::obstacles::tree::geometry::{
    TreeBranchSegment, TreeLeaf, procedural_woody_branch_bake_mesh,
    procedural_woody_cambered_leaf_mesh,
};
use crate::presentation::obstacles::tree::materials::canopy_ao_strength;
use bevy::{
    math::Vec3Swizzles,
    mesh::{Indices, VertexAttributeValues},
    prelude::{Mesh, Vec2, Vec3, Vec4},
};

pub(super) fn render_tree_card(
    card: TreeBakeCard,
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    lod: u8,
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
    style: TreeBakeStyle,
) {
    let mut depth = vec![f32::NEG_INFINITY; (tile_size * tile_size) as usize];
    let source_branches = branches
        .iter()
        .filter(|branch| card.includes_branch(branch))
        .copied()
        .collect::<Vec<_>>();
    let branch_mesh = procedural_woody_branch_bake_mesh(&source_branches);
    raster_source_mesh(
        card,
        &branch_mesh,
        TreeSourceMaterial::Bark,
        tile_size,
        atlas_width,
        atlas_height,
        tile_x,
        tile_y,
        pixels,
        &mut depth,
        style,
    );
    let (leaf_stride, leaf_scale) = style.aggregate_leaf_recipe(lod);
    let source_leaves = stratified_tree_bake_leaves(card, leaves, leaf_stride)
        .into_iter()
        .map(|mut leaf| {
            // Aggregate cards need optical crown coverage rather than every
            // sub-pixel production leaf. The species recipe calibrates proxy
            // area while bounding triangle setup and overdraw.
            leaf.length *= leaf_scale;
            leaf.width *= leaf_scale;
            leaf
        })
        .collect::<Vec<_>>();
    let leaf_mesh = procedural_woody_cambered_leaf_mesh(&source_leaves);
    raster_source_mesh(
        card,
        &leaf_mesh,
        TreeSourceMaterial::Leaf,
        tile_size,
        atlas_width,
        atlas_height,
        tile_x,
        tile_y,
        pixels,
        &mut depth,
        style,
    );
}

/// Reduces leaf raster work without repeatedly selecting the same ordinal
/// from every shoot. A global `step_by` aliases with both the oak's 16-leaf
/// terminal flush and the beech's 8-leaf spray: coarse LODs then retain only
/// basal leaves and collapse a broad crown into inner columns or branch
/// shelves. Each shoot instead receives a stable phase, so retained leaves
/// continue to sample its complete biological span. When the stride exceeds
/// the shoot size, a stable subset of shoots contributes one representative.
fn stratified_tree_bake_leaves(
    card: TreeBakeCard,
    leaves: &[TreeLeaf],
    stride: usize,
) -> Vec<TreeLeaf> {
    let included = leaves
        .iter()
        .filter(|leaf| card.includes_leaf(leaf))
        .copied()
        .collect::<Vec<_>>();
    if stride <= 1 {
        return included;
    }

    let mut sampled = Vec::with_capacity(included.len().div_ceil(stride));
    let mut start = 0;
    while start < included.len() {
        let shoot_id = included[start].shoot_id;
        let mut end = start + 1;
        while end < included.len() && included[end].shoot_id == shoot_id {
            end += 1;
        }
        let shoot = &included[start..end];
        let phase_hash = crate::presentation::splitmix64(
            u64::from(shoot_id) ^ u64::from(card.source_group).rotate_left(23) ^ 0x1eaf_5a6d,
        );
        if shoot.len() >= stride {
            let phase = phase_hash as usize % stride;
            let before = sampled.len();
            for ordinal in (phase..shoot.len()).step_by(stride) {
                sampled.push(shoot[ordinal]);
            }
            // A late phase may produce one fewer sample than the intended
            // ceiling. Wrap once to keep coverage and work bounded.
            let target = shoot.len().div_ceil(stride);
            if sampled.len() - before < target {
                let wrapped = (phase + (sampled.len() - before) * stride) % shoot.len();
                sampled.push(shoot[wrapped]);
            }
        } else if phase_hash as usize % stride < shoot.len() {
            sampled.push(shoot[(phase_hash.rotate_left(17) as usize) % shoot.len()]);
        }
        start = end;
    }
    sampled
}

#[derive(Clone, Copy)]
enum TreeSourceMaterial {
    Bark,
    Leaf,
}

fn raster_source_mesh(
    card: TreeBakeCard,
    mesh: &Mesh,
    material: TreeSourceMaterial,
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
    depth: &mut [f32],
    style: TreeBakeStyle,
) {
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(VertexAttributeValues::as_float3)
        .expect("procedural tree mesh has float positions");
    let normals = mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(VertexAttributeValues::as_float3)
        .expect("procedural tree mesh has float normals");
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(colors)) => Some(colors.as_slice()),
        _ => None,
    };
    let Indices::U32(indices) = mesh.indices().expect("procedural tree mesh is indexed") else {
        unreachable!("procedural tree mesh uses u32 indices")
    };
    for triangle in indices.chunks_exact(3) {
        let vertex_indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let projected = vertex_indices
            .map(|index| project_to_tile(card, Vec3::from_array(positions[index]), tile_size));
        let a = projected[0].xy();
        let b = projected[1].xy();
        let c = projected[2].xy();
        let denominator = (b - a).perp_dot(c - a);
        if denominator.abs() < 0.0001 {
            continue;
        }
        let minimum = a.min(b).min(c).floor().max(Vec2::ZERO);
        let maximum = a
            .max(b)
            .max(c)
            .ceil()
            .min(Vec2::splat(tile_size as f32 - 1.0));
        for y in minimum.y as u32..=maximum.y as u32 {
            for x in minimum.x as u32..=maximum.x as u32 {
                let sample = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let weight_b = (sample - a).perp_dot(c - a) / denominator;
                let weight_c = (b - a).perp_dot(sample - a) / denominator;
                let weight_a = 1.0 - weight_b - weight_c;
                if weight_a < -0.001 || weight_b < -0.001 || weight_c < -0.001 {
                    continue;
                }
                let weights = [weight_a, weight_b, weight_c];
                let z = projected
                    .iter()
                    .zip(weights)
                    .map(|(point, weight)| point.z * weight)
                    .sum();
                let normal = vertex_indices
                    .iter()
                    .zip(weights)
                    .map(|(index, weight)| Vec3::from_array(normals[*index]) * weight)
                    .sum::<Vec3>()
                    .normalize_or_zero();
                let light = 0.62 + normal.dot(Vec3::new(0.35, 0.86, 0.25)).abs() * 0.34;
                let color = match material {
                    TreeSourceMaterial::Bark => [
                        (style.bark_srgb[0] * light) as u8,
                        (style.bark_srgb[1] * light) as u8,
                        (style.bark_srgb[2] * light) as u8,
                        255,
                    ],
                    TreeSourceMaterial::Leaf => {
                        let tint = colors.map_or(Vec4::ONE, |colors| {
                            vertex_indices
                                .iter()
                                .zip(weights)
                                .map(|(index, weight)| Vec4::from_array(colors[*index]) * weight)
                                .sum()
                        });
                        baked_leaf_color(tint, light, style)
                    }
                };
                write_tree_pixel(
                    x,
                    y,
                    z,
                    color,
                    tile_size,
                    atlas_width,
                    atlas_height,
                    tile_x,
                    tile_y,
                    pixels,
                    depth,
                );
            }
        }
    }
}

fn baked_leaf_color(tint: Vec4, light: f32, style: TreeBakeStyle) -> [u8; 4] {
    // Vertex color RGB is semantic data, not an albedo tint: X/Y carry the
    // authored shade, Z selects a live directional self-shadow, and W carries
    // ambient visibility. The old bake accidentally treated XYZ as pigment,
    // producing lime cards and using the binary shadow selector as blue.
    let shade = ((tint.x + tint.y) * 0.5).clamp(0.0, 1.5);
    let canopy_visibility =
        1.0 + canopy_ao_strength(style.crown_radius_metres) * (tint.w.clamp(0.32, 1.0) - 1.0);
    let response = (shade * canopy_visibility * light).max(0.0);
    [
        (style.leaf_srgb[0] * response).min(255.0) as u8,
        (style.leaf_srgb[1] * response).min(255.0) as u8,
        (style.leaf_srgb[2] * response).min(255.0) as u8,
        255,
    ]
}

pub(super) fn project_to_tile(card: TreeBakeCard, point: Vec3, tile_size: u32) -> Vec3 {
    let relative = point - card.center;
    Vec3::new(
        (relative.dot(card.right) / card.width + 0.5) * (tile_size - 1) as f32,
        (0.5 - relative.dot(card.up) / card.height) * (tile_size - 1) as f32,
        relative.dot(card.normal()),
    )
}
pub(super) fn write_tree_pixel(
    x: u32,
    y: u32,
    z: f32,
    color: [u8; 4],
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
    depth: &mut [f32],
) {
    let local_index = (y * tile_size + x) as usize;
    if z <= depth[local_index] {
        return;
    }
    depth[local_index] = z;
    let atlas_x = tile_x * tile_size + x;
    let atlas_y = tile_y * tile_size + y;
    debug_assert!(atlas_x < atlas_width && atlas_y < atlas_height);
    let index = ((atlas_y * atlas_width + atlas_x) * 4) as usize;
    pixels[index..index + 4].copy_from_slice(&color);
}

#[cfg(test)]
mod tests {
    use super::super::{
        BEECH_TREE_BAKE_STYLE, fit_tree_bake_card, tree_bake_cards, tree_bake_cards_with_style,
    };
    use super::*;
    use crate::presentation::obstacles::tree::presentation::oak_gnarling_for_test_site;
    use crate::presentation::obstacles::tree::{
        COMMON_BEECH_PARAMETERS, OAK_GNARLING_SHOWCASE, procedural_oak_leaves,
        procedural_oak_skeleton_with_gnarling, procedural_tree_skeleton,
        procedural_woody_plant_leaves, procedural_woody_plant_skeleton,
    };
    use crate::presentation::{obstacle_seed, splitmix64};
    use adventuresim_tactical_core::prelude::{Precipitation, SceneEnvironment, WeatherSnapshot};

    #[derive(Clone, Copy, Debug)]
    struct ProjectedCrownCoverage {
        occupied_fraction: f32,
        bounds: Vec4,
        silhouette_centroid: Vec2,
    }

    fn bounds_minimum(bounds: Vec4) -> Vec2 {
        Vec2::new(bounds.x, bounds.y)
    }

    fn bounds_maximum(bounds: Vec4) -> Vec2 {
        Vec2::new(bounds.z, bounds.w)
    }

    /// Measures the crown-space effect of a bake recipe without coupling the
    /// regression to a species' raw leaf count. A small projected grid is
    /// sufficient to catch both holes and loss of the source silhouette.
    fn projected_crown_coverage(
        card: TreeBakeCard,
        leaves: &[TreeLeaf],
        lod: u8,
        style: TreeBakeStyle,
    ) -> ProjectedCrownCoverage {
        const GRID: usize = 96;
        let (stride, scale) = if lod == 0 {
            (1, 1.0)
        } else {
            style.aggregate_leaf_recipe(lod)
        };
        let sampled = stratified_tree_bake_leaves(card, leaves, stride);
        let mut occupied = [false; GRID * GRID];
        let mut minimum = Vec2::splat(f32::INFINITY);
        let mut maximum = Vec2::splat(f32::NEG_INFINITY);

        for leaf in sampled {
            let half_right = leaf.right * leaf.width * scale * 0.5;
            let half_up = leaf.up * leaf.length * scale * 0.5;
            let projected = [
                leaf.center - half_right - half_up,
                leaf.center + half_right - half_up,
                leaf.center - half_right + half_up,
                leaf.center + half_right + half_up,
            ]
            .map(|point| {
                let relative = point - card.center;
                Vec2::new(
                    relative.dot(card.right) / card.width + 0.5,
                    0.5 - relative.dot(card.up) / card.height,
                )
            });
            let leaf_minimum = projected
                .into_iter()
                .fold(Vec2::splat(f32::INFINITY), Vec2::min);
            let leaf_maximum = projected
                .into_iter()
                .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
            minimum = minimum.min(leaf_minimum);
            maximum = maximum.max(leaf_maximum);

            let grid_minimum = (leaf_minimum * GRID as f32)
                .floor()
                .clamp(Vec2::ZERO, Vec2::splat((GRID - 1) as f32));
            let grid_maximum = (leaf_maximum * GRID as f32)
                .ceil()
                .clamp(Vec2::ZERO, Vec2::splat((GRID - 1) as f32));
            for y in grid_minimum.y as usize..=grid_maximum.y as usize {
                for x in grid_minimum.x as usize..=grid_maximum.x as usize {
                    occupied[y * GRID + x] = true;
                }
            }
        }

        let (occupied_count, centroid_sum) = occupied.iter().enumerate().fold(
            (0_usize, Vec2::ZERO),
            |(count, sum), (index, cell)| {
                if *cell {
                    let x = index % GRID;
                    let y = index / GRID;
                    (
                        count + 1,
                        sum + Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / GRID as f32,
                    )
                } else {
                    (count, sum)
                }
            },
        );
        ProjectedCrownCoverage {
            occupied_fraction: occupied_count as f32 / (GRID * GRID) as f32,
            bounds: Vec4::new(minimum.x, minimum.y, maximum.x, maximum.y),
            silhouette_centroid: centroid_sum / occupied_count.max(1) as f32,
        }
    }

    fn coverage_from_mask(occupied: &[bool], grid: usize) -> ProjectedCrownCoverage {
        let mut count = 0_usize;
        let mut centroid_sum = Vec2::ZERO;
        let mut minimum = Vec2::splat(f32::INFINITY);
        let mut maximum = Vec2::splat(f32::NEG_INFINITY);
        for (index, cell) in occupied.iter().copied().enumerate() {
            if !cell {
                continue;
            }
            let x = index % grid;
            let y = index / grid;
            let center = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / grid as f32;
            count += 1;
            centroid_sum += center;
            minimum = minimum.min(Vec2::new(x as f32, y as f32) / grid as f32);
            maximum = maximum.max(Vec2::new((x + 1) as f32, (y + 1) as f32) / grid as f32);
        }
        ProjectedCrownCoverage {
            occupied_fraction: count as f32 / (grid * grid) as f32,
            bounds: Vec4::new(minimum.x, minimum.y, maximum.x, maximum.y),
            silhouette_centroid: centroid_sum / count.max(1) as f32,
        }
    }

    fn projected_lod0_leaf_mesh_coverage(
        reference: TreeBakeCard,
        leaves: &[TreeLeaf],
    ) -> ProjectedCrownCoverage {
        const GRID: usize = 192;
        let mesh = procedural_woody_cambered_leaf_mesh(leaves);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("procedural leaf mesh has float positions");
        let Indices::U32(indices) = mesh.indices().expect("procedural leaf mesh is indexed") else {
            unreachable!("procedural leaf mesh uses u32 indices")
        };
        let mut occupied = vec![false; GRID * GRID];
        for triangle in indices.chunks_exact(3) {
            let projected = [triangle[0], triangle[1], triangle[2]].map(|index| {
                let relative = Vec3::from_array(positions[index as usize]) - reference.center;
                Vec2::new(
                    relative.dot(reference.right) / reference.width + 0.5,
                    0.5 - relative.dot(reference.up) / reference.height,
                ) * GRID as f32
            });
            let [a, b, c] = projected;
            let denominator = (b - a).perp_dot(c - a);
            if denominator.abs() < 0.0001 {
                continue;
            }
            let minimum = a
                .min(b)
                .min(c)
                .floor()
                .clamp(Vec2::ZERO, Vec2::splat((GRID - 1) as f32));
            let maximum = a
                .max(b)
                .max(c)
                .ceil()
                .clamp(Vec2::ZERO, Vec2::splat((GRID - 1) as f32));
            for y in minimum.y as usize..=maximum.y as usize {
                for x in minimum.x as usize..=maximum.x as usize {
                    let sample = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let weight_b = (sample - a).perp_dot(c - a) / denominator;
                    let weight_c = (b - a).perp_dot(sample - a) / denominator;
                    let weight_a = 1.0 - weight_b - weight_c;
                    if weight_a >= -0.001 && weight_b >= -0.001 && weight_c >= -0.001 {
                        occupied[y * GRID + x] = true;
                    }
                }
            }
        }
        coverage_from_mask(&occupied, GRID)
    }

    fn projected_lod1_raster_alpha_coverage(
        reference: TreeBakeCard,
        cards: &[TreeBakeCard],
        branches: &[TreeBranchSegment],
        leaves: &[TreeLeaf],
        style: TreeBakeStyle,
    ) -> ProjectedCrownCoverage {
        const GRID: usize = 192;
        const TILE: u32 = 96;
        const ALPHA_DISCARD: u8 = 51; // shader cutoff 0.2
        let mut tile_alpha = Vec::with_capacity(cards.len());
        for card in cards.iter().copied() {
            let mut pixels = vec![0_u8; (TILE * TILE * 4) as usize];
            render_tree_card(
                card,
                branches,
                leaves,
                1,
                TILE,
                TILE,
                TILE,
                0,
                0,
                &mut pixels,
                style,
            );
            tile_alpha.push(
                pixels
                    .chunks_exact(4)
                    .map(|pixel| pixel[3])
                    .collect::<Vec<_>>(),
            );
        }

        let view_normal = reference.normal();
        let mut occupied = vec![false; GRID * GRID];
        for y in 0..GRID {
            for x in 0..GRID {
                let screen = reference.center
                    + reference.right * (((x as f32 + 0.5) / GRID as f32 - 0.5) * reference.width)
                    + reference.up * ((0.5 - (y as f32 + 0.5) / GRID as f32) * reference.height);
                for (card, alpha) in cards.iter().zip(&tile_alpha) {
                    let normal = card.normal();
                    let denominator = view_normal.dot(normal);
                    if denominator.abs() < 0.0001 {
                        continue;
                    }
                    let point =
                        screen + view_normal * ((card.center - screen).dot(normal) / denominator);
                    let relative = point - card.center;
                    let uv = Vec2::new(
                        relative.dot(card.right) / (card.width * style.lod1_runtime_width_scale)
                            + 0.5,
                        0.5 - relative.dot(card.up) / card.height,
                    );
                    if !(0.0..=1.0).contains(&uv.x) || !(0.0..=1.0).contains(&uv.y) {
                        continue;
                    }
                    let tx = (uv.x * TILE as f32).floor().min((TILE - 1) as f32) as usize;
                    let ty = (uv.y * TILE as f32).floor().min((TILE - 1) as f32) as usize;
                    if alpha[ty * TILE as usize + tx] >= ALPHA_DISCARD {
                        occupied[y * GRID + x] = true;
                        break;
                    }
                }
            }
        }
        coverage_from_mask(&occupied, GRID)
    }

    fn assert_aggregate_crown_continuity(
        card: TreeBakeCard,
        leaves: &[TreeLeaf],
        style: TreeBakeStyle,
    ) {
        let source = projected_crown_coverage(card, leaves, 0, style);
        let source_extent = bounds_maximum(source.bounds) - bounds_minimum(source.bounds);
        let mut previous = source;
        for lod in 1..=4 {
            let aggregate = projected_crown_coverage(card, leaves, lod, style);
            let coverage_ratio = aggregate.occupied_fraction / source.occupied_fraction;
            assert!(
                (0.72..=1.20).contains(&coverage_ratio),
                "LOD {lod} projected occupancy ratio {coverage_ratio:.3} left the source crown envelope"
            );
            let adjacent_ratio = aggregate.occupied_fraction / previous.occupied_fraction;
            assert!(
                (0.78..=1.22).contains(&adjacent_ratio),
                "LOD {lod} projected occupancy jumped {adjacent_ratio:.3} across its handoff"
            );
            let extent = bounds_maximum(aggregate.bounds) - bounds_minimum(aggregate.bounds);
            let extent_ratio = extent / source_extent;
            assert!(
                extent_ratio.cmpge(Vec2::splat(0.88)).all()
                    && extent_ratio.cmple(Vec2::splat(1.15)).all(),
                "LOD {lod} projected crown extent {extent_ratio:?} diverged from its source"
            );
            previous = aggregate;
        }
    }

    fn projected_lod1_runtime_coverage(
        reference: TreeBakeCard,
        cards: &[TreeBakeCard],
        leaves: &[TreeLeaf],
        style: TreeBakeStyle,
    ) -> ProjectedCrownCoverage {
        let (stride, scale) = style.aggregate_leaf_recipe(1);
        let mut flattened = Vec::new();
        for card in cards {
            for mut leaf in stratified_tree_bake_leaves(*card, leaves, stride) {
                let relative = leaf.center - card.center;
                leaf.center = card.center
                    + card.right * relative.dot(card.right) * style.lod1_runtime_width_scale
                    + card.up * relative.dot(card.up);
                leaf.right =
                    card.right * leaf.right.dot(card.right) * style.lod1_runtime_width_scale
                        + card.up * leaf.right.dot(card.up);
                leaf.up = card.right * leaf.up.dot(card.right) + card.up * leaf.up.dot(card.up);
                leaf.length *= scale;
                leaf.width *= scale;
                flattened.push(leaf);
            }
        }
        projected_crown_coverage(reference, &flattened, 0, style)
    }

    fn assert_lod1_runtime_transition_preserves_source_crown(
        seed: u64,
        branches: &[TreeBranchSegment],
        leaves: &[TreeLeaf],
        style: TreeBakeStyle,
    ) {
        let reference = tree_bake_cards_with_style(seed, branches, leaves, 4, style)[0];
        let source = projected_crown_coverage(reference, leaves, 0, style);
        let lod1_cards = tree_bake_cards_with_style(seed, branches, leaves, 1, style);
        let aggregate = projected_lod1_runtime_coverage(reference, &lod1_cards, leaves, style);
        let occupancy_ratio = aggregate.occupied_fraction / source.occupied_fraction;
        assert!(
            (0.70..=1.18).contains(&occupancy_ratio),
            "LOD1 runtime occupancy ratio {occupancy_ratio:.3} diverged from high detail"
        );
        let source_extent = bounds_maximum(source.bounds) - bounds_minimum(source.bounds);
        let aggregate_extent = bounds_maximum(aggregate.bounds) - bounds_minimum(aggregate.bounds);
        let extent_ratio = aggregate_extent / source_extent;
        assert!(
            extent_ratio.cmpge(Vec2::splat(0.88)).all()
                && extent_ratio.cmple(Vec2::splat(1.15)).all(),
            "LOD1 runtime crown extent {extent_ratio:?} diverged from high detail"
        );
        let center_shift = (bounds_minimum(aggregate.bounds) + bounds_maximum(aggregate.bounds)
            - bounds_minimum(source.bounds)
            - bounds_maximum(source.bounds))
            * 0.5;
        assert!(
            center_shift.abs().cmple(Vec2::splat(0.06)).all(),
            "LOD1 runtime crown center shifted {center_shift:?} from high detail"
        );
    }

    fn assert_oak_lod1_matches_high_detail_from_multiple_azimuths(
        seed: u64,
        branches: &[TreeBranchSegment],
        leaves: &[TreeLeaf],
    ) {
        let references = tree_bake_cards_with_style(seed, branches, leaves, 4, OAK_TREE_BAKE_STYLE);
        let lod1_cards = tree_bake_cards_with_style(seed, branches, leaves, 1, OAK_TREE_BAKE_STYLE);
        for (view, reference) in references.into_iter().take(4).enumerate() {
            let source = projected_crown_coverage(reference, leaves, 0, OAK_TREE_BAKE_STYLE);
            let aggregate = projected_lod1_runtime_coverage(
                reference,
                &lod1_cards,
                leaves,
                OAK_TREE_BAKE_STYLE,
            );
            let occupancy_ratio = aggregate.occupied_fraction / source.occupied_fraction;
            assert!(
                (0.70..=1.08).contains(&occupancy_ratio),
                "oak LOD1 view {view} occupancy ratio {occupancy_ratio:.3} lost the airy source crown"
            );
            let source_extent = bounds_maximum(source.bounds) - bounds_minimum(source.bounds);
            let extent_ratio = (bounds_maximum(aggregate.bounds)
                - bounds_minimum(aggregate.bounds))
                / source_extent;
            assert!(
                (0.92..=1.12).contains(&extent_ratio.x) && (0.94..=1.10).contains(&extent_ratio.y),
                "oak LOD1 view {view} extent {extent_ratio:?} lost lateral or upper reach"
            );
            let centroid_shift = aggregate.silhouette_centroid - source.silhouette_centroid;
            assert!(
                centroid_shift.abs().cmple(Vec2::splat(0.045)).all(),
                "oak LOD1 view {view} silhouette centroid shifted {centroid_shift:?}"
            );
        }
    }

    #[test]
    fn aggregate_tree_bakes_reduce_leaf_work_monotonically() {
        assert_eq!(
            (1..=4)
                .map(|lod| OAK_TREE_BAKE_STYLE.aggregate_leaf_recipe(lod).0)
                .collect::<Vec<_>>(),
            [3, 4, 8, 16]
        );
    }

    #[test]
    fn coarse_leaf_sampling_preserves_positions_across_every_shoot() {
        let card = TreeBakeCard {
            center: Vec3::ZERO,
            right: Vec3::X,
            up: Vec3::Y,
            width: 20.0,
            height: 20.0,
            primary_mask: 0,
            secondary_group: None,
            source_group: 3,
            minimum_branch_depth: 0,
        };
        let leaves = (0..32_u16)
            .flat_map(|shoot_id| {
                (0..16).map(move |ordinal| TreeLeaf {
                    petiole_start: Vec3::ZERO,
                    center: Vec3::new(ordinal as f32, shoot_id as f32 * 0.1, 0.0),
                    right: Vec3::X,
                    up: Vec3::Y,
                    length: 0.1,
                    width: 0.07,
                    primary_group: 0,
                    secondary_group: 0,
                    shoot_id,
                    shade: 1.0,
                    torsion: 0.0,
                })
            })
            .collect::<Vec<_>>();

        let sampled = stratified_tree_bake_leaves(card, &leaves, 16);
        assert_eq!(sampled.len(), 32);
        let repeated = stratified_tree_bake_leaves(card, &leaves, 16);
        assert!(sampled.iter().zip(repeated).all(|(left, right)| {
            left.shoot_id == right.shoot_id && left.center == right.center
        }));
        let mut ordinals = sampled
            .iter()
            .map(|leaf| leaf.center.x as u8)
            .collect::<Vec<_>>();
        ordinals.sort_unstable();
        ordinals.dedup();
        assert!(ordinals.len() >= 10);
        assert!(ordinals[0] <= 2 && ordinals[ordinals.len() - 1] >= 13);
    }

    #[test]
    fn oak_aggregate_recipe_preserves_source_crown_occupancy_and_bounds() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let card = tree_bake_cards(42, &branches, &leaves, 4)[0];
        assert_aggregate_crown_continuity(card, &leaves, OAK_TREE_BAKE_STYLE);
    }

    #[test]
    fn beech_aggregate_recipe_preserves_source_crown_occupancy_and_bounds() {
        let branches = procedural_woody_plant_skeleton(42, 0.7, COMMON_BEECH_PARAMETERS);
        let leaves = procedural_woody_plant_leaves(42, &branches, 0.7, COMMON_BEECH_PARAMETERS);
        let card = tree_bake_cards(42, &branches, &leaves, 4)[0];
        assert_aggregate_crown_continuity(card, &leaves, BEECH_TREE_BAKE_STYLE);
    }

    #[test]
    fn lod1_runtime_cards_preserve_oak_and_beech_source_crowns() {
        let oak_branches = procedural_tree_skeleton(42, 0.0);
        let oak_leaves = procedural_oak_leaves(42, &oak_branches, 0.0);
        assert_lod1_runtime_transition_preserves_source_crown(
            42,
            &oak_branches,
            &oak_leaves,
            OAK_TREE_BAKE_STYLE,
        );
        assert_oak_lod1_matches_high_detail_from_multiple_azimuths(42, &oak_branches, &oak_leaves);

        let beech_branches = procedural_woody_plant_skeleton(42, 0.7, COMMON_BEECH_PARAMETERS);
        let beech_leaves =
            procedural_woody_plant_leaves(42, &beech_branches, 0.7, COMMON_BEECH_PARAMETERS);
        assert_lod1_runtime_transition_preserves_source_crown(
            42,
            &beech_branches,
            &beech_leaves,
            BEECH_TREE_BAKE_STYLE,
        );
    }

    #[test]
    fn sparse_fixture_oak_lod1_preserves_45_degree_upper_right_silhouette() {
        const SCENE_SEED: u64 = 47_104;
        const FOCUSED_TREE_RECIPE_SEED: u64 = 15_240_619_980_244_641_867;
        const VISTA_CACHE_RECIPE_SEED: u64 = 16_311_644_104_379_926_507;
        const FIXTURE_COMPETITION: f32 = 0.281_75;
        // Seed 47104 deterministically places the reviewed playable tree at
        // (12.5, 37.5). Production hashes that placement, selects showcase
        // variant 1, and applies the sparse-woodland site history. The much
        // more frequent variant-0 seed in the manifest belongs to vista/cache
        // bakes and is not the tree framed by the forced-LOD views.
        let fixture_position = Vec3::new(12.5, 0.0, 37.5);
        let obstacle_seed = obstacle_seed(fixture_position);
        let variant_index = (obstacle_seed & 3) as usize;
        let variant_seed = splitmix64(0x6f61_6b00 ^ variant_index as u64);
        assert_eq!(SCENE_SEED, 47_104);
        assert_eq!(variant_index, 1);
        assert_eq!(variant_seed, FOCUSED_TREE_RECIPE_SEED);
        assert_ne!(variant_seed, VISTA_CACHE_RECIPE_SEED);
        let environment = SceneEnvironment {
            scene_digest: "4926dcc166599287c1966fd23ced047bd918de9f6b80ed1b8f9280159f094a6b".into(),
            generation_version: 8,
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 340_440,
            absolute_elevation_metres: 42,
            weather: WeatherSnapshot {
                rules_version: adventuresim_tactical_core::prelude::WEATHER_RULES_VERSION,
                interval_start_minute: 340_440,
                cell_latitude: 214,
                cell_longitude: 40,
                temperature_deci_c: 120,
                wind_speed_bps: 1_200,
                precipitation: Precipitation::Clear,
                intensity_bps: 0,
                ground_moisture_bps: 100,
                snow_cover_bps: 0,
                atmosphere: Default::default(),
            },
            canopy_bps: 3_500,
            wetland_bps: 300,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        };
        let gnarling = oak_gnarling_for_test_site(
            OAK_GNARLING_SHOWCASE[variant_index],
            &environment,
            variant_seed,
        );
        let branches =
            procedural_oak_skeleton_with_gnarling(variant_seed, FIXTURE_COMPETITION, gnarling);
        let leaves = procedural_oak_leaves(variant_seed, &branches, FIXTURE_COMPETITION);
        // The viewer places the camera in +X/+Z at review azimuth 45 degrees.
        // Use its manifested twig-LOD camera-up vector so this regression
        // evaluates the rejected fixture rather than a convenient atlas axis.
        let camera_right = Vec3::new(
            core::f32::consts::FRAC_1_SQRT_2,
            0.0,
            -core::f32::consts::FRAC_1_SQRT_2,
        );
        let camera_up = Vec3::new(-0.008_363_048, 0.999_93, -0.008_363_047).normalize();
        let reference =
            fit_tree_bake_card(&branches, &leaves, camera_right, camera_up, 0, None, 0, 0);
        let source = projected_lod0_leaf_mesh_coverage(reference, &leaves);
        let cards =
            tree_bake_cards_with_style(variant_seed, &branches, &leaves, 1, OAK_TREE_BAKE_STYLE);
        let aggregate = projected_lod1_raster_alpha_coverage(
            reference,
            &cards,
            &branches,
            &leaves,
            OAK_TREE_BAKE_STYLE,
        );
        let source_minimum = bounds_minimum(source.bounds);
        let source_maximum = bounds_maximum(source.bounds);
        let aggregate_minimum = bounds_minimum(aggregate.bounds);
        let aggregate_maximum = bounds_maximum(aggregate.bounds);
        let lateral_ratio =
            (aggregate_maximum.x - aggregate_minimum.x) / (source_maximum.x - source_minimum.x);
        let right_span_ratio = (aggregate_maximum.x - aggregate.silhouette_centroid.x)
            / (source_maximum.x - source.silhouette_centroid.x);
        let upper_span_ratio = (aggregate.silhouette_centroid.y - aggregate_minimum.y)
            / (source.silhouette_centroid.y - source_minimum.y);
        let occupancy_ratio = aggregate.occupied_fraction / source.occupied_fraction;
        let centroid_shift = aggregate.silhouette_centroid - source.silhouette_centroid;

        eprintln!(
            "seed={variant_seed} lateral={lateral_ratio:.3} right={right_span_ratio:.3} upper={upper_span_ratio:.3} occupancy={occupancy_ratio:.3} centroid_shift={centroid_shift:?}"
        );

        assert!(
            (0.94..=1.10).contains(&lateral_ratio)
                && (0.95..=1.10).contains(&right_span_ratio)
                && (0.95..=1.10).contains(&upper_span_ratio),
            "sparse fixture LOD1 span mismatch: lateral={lateral_ratio:.3}, right={right_span_ratio:.3}, upper={upper_span_ratio:.3}"
        );
        assert!(
            (0.70..=1.05).contains(&occupancy_ratio),
            "sparse fixture LOD1 occupancy {occupancy_ratio:.3} no longer preserves airy gaps"
        );
        assert!(
            centroid_shift.abs().cmple(Vec2::splat(0.035)).all(),
            "sparse fixture LOD1 centroid shifted {centroid_shift:?}"
        );
    }

    #[test]
    fn baked_leaf_color_uses_generated_palette_and_authored_canopy_visibility() {
        let exposed = baked_leaf_color(Vec4::new(1.0, 1.0, 0.0, 1.0), 1.0, OAK_TREE_BAKE_STYLE);
        let alternate_shadow_selector =
            baked_leaf_color(Vec4::new(1.0, 1.0, 1.0, 1.0), 1.0, OAK_TREE_BAKE_STYLE);
        let interior = baked_leaf_color(Vec4::new(1.0, 1.0, 0.0, 0.32), 1.0, OAK_TREE_BAKE_STYLE);

        assert_eq!(exposed, [96, 113, 76, 255]);
        assert_eq!(alternate_shadow_selector, exposed);
        assert!(interior[0] < exposed[0]);
        assert!(interior[1] < exposed[1]);
        assert!(interior[2] < exposed[2]);
        assert!(
            exposed[1] - exposed[0] < 24,
            "oak pigment must not turn lime"
        );
        assert!(
            exposed[2] > exposed[0] / 2,
            "blue must come from pigment, not a selector"
        );
    }

    #[test]
    fn baked_leaf_color_accepts_a_species_palette() {
        let beech = TreeBakeStyle {
            leaf_srgb: [91.0, 119.0, 70.0],
            crown_radius_metres: 4.6,
            ..OAK_TREE_BAKE_STYLE
        };
        assert_eq!(
            baked_leaf_color(Vec4::new(1.0, 1.0, 0.0, 1.0), 1.0, beech),
            [91, 119, 70, 255]
        );
    }
}
