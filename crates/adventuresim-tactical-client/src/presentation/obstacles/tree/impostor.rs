use super::super::super::*;
use super::geometry::*;

pub(in crate::presentation) const TREE_IMPOSTOR_BAKE_VERSION: u32 = 4;
pub(in crate::presentation) const TREE_IMPOSTOR_RENDER_METHOD: &str =
    "deterministic software triangle render of exact production branch and leaf meshes";

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
    pub(in crate::presentation) image: Image,
    pub(in crate::presentation) provenance: TreeImpostorProvenance,
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
                card.width,
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

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
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
            let sector_count = TREE_PRIMARY_GROUP_COUNT.div_ceil(2);
            for group in 0..sector_count {
                let phase = crown_group_phase(seed, group);
                let paired_group = group + sector_count;
                let primary_mask = (1 << group)
                    | if paired_group < TREE_PRIMARY_GROUP_COUNT {
                        1 << paired_group
                    } else {
                        0
                    };
                for facing in 0..2 {
                    let angle = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    let mut card = fit_tree_bake_card(
                        branches,
                        leaves,
                        Vec3::new(angle.cos(), 0.0, angle.sin()),
                        Vec3::Y,
                        primary_mask,
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
pub(in crate::presentation) fn render_tree_card(
    card: TreeBakeCard,
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
) {
    let mut depth = vec![f32::NEG_INFINITY; (tile_size * tile_size) as usize];
    let source_branches = branches
        .iter()
        .filter(|branch| card.includes_branch(branch))
        .copied()
        .collect::<Vec<_>>();
    let branch_mesh = procedural_tree_branch_mesh(&source_branches, 3);
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
    );
    let source_leaves = leaves
        .iter()
        .filter(|leaf| card.includes_leaf(leaf))
        .copied()
        .collect::<Vec<_>>();
    let leaf_mesh = procedural_oak_leaf_mesh(&source_leaves);
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
    );
}

#[derive(Clone, Copy)]
enum TreeSourceMaterial {
    Bark,
    Leaf,
}

#[allow(clippy::too_many_arguments)]
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
                        (116.0 * light) as u8,
                        (103.0 * light) as u8,
                        (82.0 * light) as u8,
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
                        [
                            (105.0 * tint.x * light).min(255.0) as u8,
                            (158.0 * tint.y * light).min(255.0) as u8,
                            (52.0 * tint.z * light).min(255.0) as u8,
                            255,
                        ]
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

pub(in crate::presentation) fn project_to_tile(
    card: TreeBakeCard,
    point: Vec3,
    tile_size: u32,
) -> Vec3 {
    let relative = point - card.center;
    Vec3::new(
        (relative.dot(card.right) / card.width + 0.5) * (tile_size - 1) as f32,
        (0.5 - relative.dot(card.up) / card.height) * (tile_size - 1) as f32,
        relative.dot(card.normal()),
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn write_tree_pixel(
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

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn append_tree_card_with_uv(
    center: Vec3,
    right: Vec3,
    up: Vec3,
    width: f32,
    height: f32,
    uv_min: Vec2,
    uv_max: Vec2,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let right = right.normalize() * width * 0.5;
    let up = up.normalize() * height * 0.5;
    let normal = right.cross(up).normalize_or_zero();
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        (center - right - up).to_array(),
        (center + right - up).to_array(),
        (center + right + up).to_array(),
        (center - right + up).to_array(),
    ]);
    normals.extend_from_slice(&[normal.to_array(); 4]);
    uvs.extend_from_slice(&[
        [uv_min.x, uv_max.y],
        [uv_max.x, uv_max.y],
        [uv_max.x, uv_min.y],
        [uv_min.x, uv_min.y],
    ]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[allow(dead_code)]
pub(in crate::presentation) fn procedural_tree_card_mesh(
    seed: u64,
    branches: &[TreeBranchSegment],
    lod: u8,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    match lod {
        0 => {
            for (index, branch) in branches
                .iter()
                .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
                .enumerate()
            {
                let direction = (branch.end - branch.start).normalize();
                let tangent = direction.cross(Vec3::Y).normalize_or_zero();
                let tangent = if tangent.length_squared() < 0.5 {
                    Vec3::X
                } else {
                    tangent
                };
                let binormal = direction.cross(tangent).normalize();
                for leaf in 0..13_u64 {
                    let leaf_seed =
                        splitmix64(seed ^ index as u64 ^ leaf.wrapping_mul(0x91e1_0da5));
                    let phase = unit_hash(leaf_seed) * 0.65 + leaf as f32 * 2.399_963_1;
                    let radial = (tangent * phase.cos() + binormal * phase.sin()).normalize();
                    let along = 0.1 + unit_hash(leaf_seed ^ 8) * 0.88;
                    let leaf_axis = (radial * 0.78
                        + direction * 0.22
                        + Vec3::Y * (0.22 - unit_hash(leaf_seed ^ 4) * 0.44))
                        .normalize();
                    let right = direction.cross(leaf_axis).normalize_or_zero();
                    let right = if right.length_squared() < 0.5 {
                        tangent
                    } else {
                        right
                    };
                    let center = branch.start.lerp(branch.end, along.min(0.98))
                        + radial * (0.055 + unit_hash(leaf_seed ^ 5) * 0.11)
                        + Vec3::Y * ((unit_hash(leaf_seed ^ 9) - 0.5) * 0.16);
                    append_tree_card(
                        center,
                        right,
                        leaf_axis,
                        0.15 + unit_hash(leaf_seed ^ 6) * 0.055,
                        0.24 + unit_hash(leaf_seed ^ 7) * 0.07,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        1 => {
            for group in 0..60_u16 {
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3 && branch.secondary_group == group
                });
                let up = branches
                    .iter()
                    .find(|branch| {
                        branch.depth == 2 && branch.secondary_group == group && branch.is_limb_tip
                    })
                    .map(|branch| (branch.end - branch.start).normalize())
                    .unwrap_or(Vec3::Y);
                let index = usize::from(group);
                let phase = unit_hash(splitmix64(seed ^ index as u64)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center() + Vec3::Y * 0.05,
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        up,
                        (bounds.horizontal_span() + 0.45) * 1.05,
                        (bounds.vertical_span() + 0.5) * 1.08,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        2 => {
            for group in 0..10_u8 {
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3 && branch.primary_group == group
                });
                let index = usize::from(group);
                let phase =
                    unit_hash(splitmix64(seed ^ index as u64 ^ 0x4a17)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center(),
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        Vec3::Y,
                        (bounds.horizontal_span() + 0.9) * 1.28,
                        (bounds.vertical_span() + 0.9) * 1.18,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        3 => {
            for index in 0..5 {
                let first = index as u8;
                let second = (index + 5) as u8;
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3
                        && (branch.primary_group == first || branch.primary_group == second)
                });
                let phase =
                    unit_hash(splitmix64(seed ^ index as u64 ^ 0x7c31)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center() + Vec3::Y * 0.08,
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        Vec3::Y,
                        (bounds.horizontal_span() + 1.0) * 1.2,
                        (bounds.vertical_span() + 1.0) * 1.15,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        4 => {
            let bounds = tree_crown_bounds(branches, |branch| branch.depth == 3);
            let bottom = -TREE_TRUNK_HEIGHT_METRES * 0.5;
            let top = bounds.maximum.y + 0.45;
            append_tree_card(
                Vec3::new(0.0, (bottom + top) * 0.5, 0.0),
                Vec3::X,
                Vec3::Y,
                (bounds.horizontal_span() + 1.25) * 1.15,
                top - bottom,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
            );
        }
        _ => unreachable!("tree LOD is bounded"),
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

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn append_tree_card(
    center: Vec3,
    right: Vec3,
    up: Vec3,
    width: f32,
    height: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let right = right.normalize() * width * 0.5;
    let up = up.normalize() * height * 0.5;
    let normal = right.cross(up).normalize_or_zero();
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        (center - right - up).to_array(),
        (center + right - up).to_array(),
        (center + right + up).to_array(),
        (center - right + up).to_array(),
    ]);
    normals.extend_from_slice(&[normal.to_array(); 4]);
    uvs.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

pub(in crate::presentation) fn tree_impostor_material(
    seed: u64,
    lod: u8,
    baked_color: Handle<Image>,
) -> TacticalTreeImpostorMaterial {
    TacticalTreeImpostorMaterial {
        baked_color,
        parameters: Vec4::new(lod as f32, unit_hash(seed), 0.08 + lod as f32 * 0.018, 1.0),
    }
}

pub(in crate::presentation) fn procedural_oak_bark_image(seed: u64) -> Image {
    const WIDTH: u32 = 128;
    const HEIGHT: u32 = 128;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let u = x as f32 / WIDTH as f32;
            let v = y as f32 / HEIGHT as f32;
            let phase = unit_hash(seed ^ u64::from(x / 11)) * core::f32::consts::TAU;
            let warp = (v * 5.0 + phase).sin() * 0.012 + (v * 13.0 - phase).sin() * 0.006;
            let fissure = ((u + warp) * 13.0 * core::f32::consts::PI)
                .sin()
                .abs()
                .powf(15.0);
            let plate = ((u * 17.0 + v * 7.0) * core::f32::consts::PI).sin()
                * ((u * 5.0 - v * 11.0) * core::f32::consts::PI).sin();
            let value = (0.82 - fissure * 0.26 + plate * 0.025).clamp(0.42, 0.9);
            pixels.extend_from_slice(&[
                (145.0 * value) as u8,
                (132.0 * value) as u8,
                (110.0 * value) as u8,
                255,
            ]);
        }
    }
    Image::new(
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

pub(in crate::presentation) fn tree_lod_visibility(lod: u8) -> VisibilityRange {
    let (start, end) = match lod {
        0 => (0.0..0.0, 22.0..26.0),
        1 => (22.0..26.0, 36.0..42.0),
        2 => (36.0..42.0, 56.0..64.0),
        3 => (56.0..64.0, 84.0..96.0),
        4 => (84.0..96.0, 190.0..200.0),
        _ => unreachable!("tree LOD is bounded"),
    };
    VisibilityRange {
        start_margin: start,
        end_margin: end,
        use_aabb: false,
    }
}

pub(in crate::presentation) fn tree_leaf_sector_visibility(sector: usize) -> VisibilityRange {
    debug_assert!(sector < OAK_LEAF_SECTOR_COUNT);
    let offset = sector as f32 * 0.7;
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: (21.0 + offset)..(25.0 + offset),
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

    #[test]
    fn tree_lods_collapse_one_botanical_order_at_a_time() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let expected_cards = [348, 14, 8, 8];
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
        for lod in 0..4 {
            let current = tree_lod_visibility(lod);
            let next = tree_lod_visibility(lod + 1);
            assert_eq!(current.end_margin, next.start_margin);
            assert!(!current.is_abrupt());
        }
    }

    #[test]
    fn leaf_sectors_thin_continuously_inside_the_twig_crossfade() {
        let twig = tree_lod_visibility(1);
        let sectors = (0..OAK_LEAF_SECTOR_COUNT)
            .map(tree_leaf_sector_visibility)
            .collect::<Vec<_>>();
        assert!(sectors.windows(2).all(|pair| {
            pair[0].end_margin.start < pair[1].end_margin.start
                && pair[0].end_margin.end < pair[1].end_margin.end
        }));
        assert!(sectors.iter().all(|sector| {
            sector.end_margin.start < twig.start_margin.end
                && sector.end_margin.end > twig.start_margin.start
                && sector.use_aabb
        }));
    }
}
