use super::super::super::*;
use super::geometry::*;

pub(in crate::presentation) const TREE_IMPOSTOR_BAKE_VERSION: u32 = 2;

#[derive(Component, Clone, Debug)]
pub(crate) struct TreeImpostorProvenance {
    pub seed: u64,
    pub lod: u8,
    pub bake_version: u32,
    pub source_geometry_hash: u64,
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
    debug_assert!(provenance.atlas_width > 0 && provenance.atlas_height > 0);
    debug_assert!(!provenance.records.is_empty());
    for record in &provenance.records {
        debug_assert!(record.source_leaf_count > 0);
        debug_assert!(record.source_branch_count > 0);
        debug_assert!(record.view_direction.is_finite());
        debug_assert!(record.projected_bounds.is_finite());
        debug_assert!(record.atlas_region.z > 0 && record.atlas_region.w > 0);
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
}

impl TreeBakeCard {
    fn includes_branch(self, branch: &TreeBranchSegment) -> bool {
        match (self.secondary_group, self.primary_mask) {
            (Some(group), _) => branch.secondary_group == group && branch.depth >= 2,
            (None, mask) if mask != 0 => {
                branch.primary_group < 8
                    && mask & (1 << branch.primary_group) != 0
                    && branch.depth >= 1
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
            atlas_width,
            atlas_height,
            records,
        },
    }
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
            for group in 0..56_u16 {
                let axis = branches
                    .iter()
                    .find(|branch| {
                        branch.depth == 2 && branch.secondary_group == group && branch.is_limb_tip
                    })
                    .map(|branch| (branch.end - branch.start).normalize())
                    .unwrap_or(Vec3::Y);
                let phase = unit_hash(splitmix64(seed ^ u64::from(group))) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let angle = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    cards.push(fit_tree_bake_card(
                        branches,
                        leaves,
                        Vec3::new(angle.cos(), 0.0, angle.sin()),
                        axis,
                        0,
                        Some(group),
                        group * 2 + facing,
                    ));
                }
            }
        }
        2 => {
            for group in 0..7_u8 {
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
                    ));
                }
            }
        }
        3 => {
            for group in 0..4_u8 {
                let phase = crown_group_phase(seed, group);
                let primary_mask = match group {
                    0 => (1 << 0) | (1 << 4),
                    1 => (1 << 1) | (1 << 5),
                    2 => (1 << 2) | (1 << 6),
                    _ => 1 << 3,
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
                    );
                    // Each crown card is a genuine complete-tree render from a
                    // different view; overlapping cards retain parallax volume.
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
    };
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for branch in branches
        .iter()
        .filter(|branch| probe.includes_branch(branch))
    {
        for point in [branch.start, branch.end] {
            let projected = Vec2::new(point.dot(right), point.dot(up));
            let radius = branch.start_radius.max(branch.end_radius);
            min = min.min(projected - Vec2::splat(radius));
            max = max.max(projected + Vec2::splat(radius));
        }
    }
    for leaf in leaves.iter().filter(|leaf| probe.includes_leaf(leaf)) {
        let radius = leaf.length.max(leaf.width) * 0.65;
        let projected = Vec2::new(leaf.center.dot(right), leaf.center.dot(up));
        min = min.min(projected - Vec2::splat(radius));
        max = max.max(projected + Vec2::splat(radius));
    }
    let margin = Vec2::splat(0.08) + (max - min) * 0.035;
    min -= margin;
    max += margin;
    let projected_center = (min + max) * 0.5;
    TreeBakeCard {
        center: right * projected_center.x + up * projected_center.y,
        right,
        up,
        width: max.x - min.x,
        height: max.y - min.y,
        primary_mask,
        secondary_group,
        source_group,
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
    for branch in branches
        .iter()
        .filter(|branch| card.includes_branch(branch))
    {
        raster_branch(
            card,
            *branch,
            tile_size,
            atlas_width,
            atlas_height,
            tile_x,
            tile_y,
            pixels,
            &mut depth,
        );
    }
    let outline = oak_leaf_outline();
    for leaf in leaves.iter().filter(|leaf| card.includes_leaf(leaf)) {
        raster_leaf(
            card,
            *leaf,
            &outline,
            tile_size,
            atlas_width,
            atlas_height,
            tile_x,
            tile_y,
            pixels,
            &mut depth,
        );
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
pub(in crate::presentation) fn raster_branch(
    card: TreeBakeCard,
    branch: TreeBranchSegment,
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
    depth: &mut [f32],
) {
    let start = project_to_tile(card, branch.start, tile_size);
    let end = project_to_tile(card, branch.end, tile_size);
    let radius = branch.start_radius.max(branch.end_radius) / card.width * tile_size as f32;
    let minimum = start.xy().min(end.xy()) - Vec2::splat(radius + 1.0);
    let maximum = start.xy().max(end.xy()) + Vec2::splat(radius + 1.0);
    let line = end.xy() - start.xy();
    let line_length = line.length_squared().max(0.001);
    for y in minimum.y.floor().max(0.0) as u32..=maximum.y.ceil().min(tile_size as f32 - 1.0) as u32
    {
        for x in
            minimum.x.floor().max(0.0) as u32..=maximum.x.ceil().min(tile_size as f32 - 1.0) as u32
        {
            let sample = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let along = ((sample - start.xy()).dot(line) / line_length).clamp(0.0, 1.0);
            let local_radius =
                branch.start_radius.lerp(branch.end_radius, along) / card.width * tile_size as f32;
            let distance = sample.distance(start.xy() + line * along);
            if distance <= local_radius.max(0.65) {
                let z = start.z.lerp(end.z, along) + (local_radius - distance) * 0.002;
                let bark_light = 0.58 + (1.0 - distance / local_radius.max(0.65)) * 0.24;
                write_tree_pixel(
                    x,
                    y,
                    z,
                    [
                        (91.0 * bark_light) as u8,
                        (79.0 * bark_light) as u8,
                        (62.0 * bark_light) as u8,
                        255,
                    ],
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

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn raster_leaf(
    card: TreeBakeCard,
    leaf: TreeLeaf,
    outline: &[Vec2],
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
    depth: &mut [f32],
) {
    let center = project_to_tile(card, leaf.center, tile_size);
    let projected = outline
        .iter()
        .map(|point| {
            project_to_tile(
                card,
                leaf.center + leaf.right * point.x * leaf.width + leaf.up * point.y * leaf.length,
                tile_size,
            )
        })
        .collect::<Vec<_>>();
    let leaf_normal = leaf.right.cross(leaf.up).normalize();
    let facing = leaf_normal.dot(card.normal()).abs();
    let light = (0.68 + leaf_normal.dot(Vec3::new(0.35, 0.86, 0.25)).abs() * 0.28) * leaf.shade;
    let color = [
        (48.0 * light) as u8,
        (118.0 * light) as u8,
        (32.0 * light) as u8,
        255,
    ];
    let minimum = projected
        .iter()
        .fold(Vec2::splat(f32::INFINITY), |bounds, point| {
            bounds.min(point.xy())
        });
    let maximum = projected
        .iter()
        .fold(Vec2::splat(f32::NEG_INFINITY), |bounds, point| {
            bounds.max(point.xy())
        });
    let polygon = projected.iter().map(|point| point.xy()).collect::<Vec<_>>();
    let transmission = 0.78 + facing * 0.22;
    let shaded = [
        (f32::from(color[0]) * transmission) as u8,
        (f32::from(color[1]) * transmission) as u8,
        (f32::from(color[2]) * transmission) as u8,
        255,
    ];
    for y in minimum.y.floor().max(0.0) as u32..=maximum.y.ceil().min(tile_size as f32 - 1.0) as u32
    {
        for x in
            minimum.x.floor().max(0.0) as u32..=maximum.x.ceil().min(tile_size as f32 - 1.0) as u32
        {
            let point = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            if point_in_polygon(point, &polygon) {
                write_tree_pixel(
                    x,
                    y,
                    center.z,
                    shaded,
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

pub(in crate::presentation) fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let a = polygon[current];
        let b = polygon[previous];
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
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
            let value = (0.78 - fissure * 0.5 + plate * 0.035).clamp(0.2, 0.88);
            pixels.extend_from_slice(&[
                (112.0 * value) as u8,
                (102.0 * value) as u8,
                (84.0 * value) as u8,
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
        let branches = procedural_tree_skeleton(42);
        let leaves = procedural_oak_leaves(42, &branches);
        let expected_cards = [112, 14, 8, 8];
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
}
