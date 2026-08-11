use super::super::super::*;

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct TreeBranchSegment {
    pub(in crate::presentation) start: Vec3,
    pub(in crate::presentation) end: Vec3,
    pub(in crate::presentation) start_radius: f32,
    pub(in crate::presentation) end_radius: f32,
    pub(in crate::presentation) depth: u8,
    pub(in crate::presentation) primary_group: u8,
    pub(in crate::presentation) secondary_group: u16,
    pub(in crate::presentation) is_limb_tip: bool,
}

pub(in crate::presentation) fn procedural_tree_skeleton(seed: u64) -> Vec<TreeBranchSegment> {
    let mut branches = Vec::new();
    let crown_phase = unit_hash(seed ^ 0x9182_64ac) * core::f32::consts::TAU;
    let trunk_bend = Vec3::new(crown_phase.cos(), 0.0, crown_phase.sin())
        * (0.22 + unit_hash(seed ^ 0x51c7_329d) * 0.18);
    let trunk_points = (0..=7)
        .map(|index| {
            let t = index as f32 / 7.0;
            Vec3::new(0.0, -TREE_TRUNK_HEIGHT_METRES * 0.5, 0.0)
                + Vec3::Y * (6.7 * t)
                + trunk_bend * t.powf(1.3)
        })
        .collect::<Vec<_>>();
    for index in 0..7 {
        let t0 = index as f32 / 7.0;
        let t1 = (index + 1) as f32 / 7.0;
        branches.push(TreeBranchSegment {
            start: trunk_points[index],
            end: trunk_points[index + 1],
            start_radius: 0.62 * (1.0 - t0 * 0.76),
            end_radius: (0.62 * (1.0 - t1 * 0.9)).max(0.025),
            depth: 0,
            primary_group: u8::MAX,
            secondary_group: u16::MAX,
            is_limb_tip: index == 6,
        });
    }

    for root_index in 0..8_u64 {
        let phase = crown_phase + root_index as f32 * core::f32::consts::TAU / 8.0;
        let direction = Vec3::new(phase.cos(), -0.1, phase.sin());
        branches.push(TreeBranchSegment {
            start: trunk_points[0] + Vec3::Y * 0.08,
            end: trunk_points[0] + direction * (0.72 + unit_hash(seed ^ root_index) * 0.35),
            start_radius: 0.3,
            end_radius: 0.055,
            depth: 0,
            primary_group: u8::MAX,
            secondary_group: u16::MAX,
            is_limb_tip: false,
        });
    }

    // A mature open-grown English oak has a few unequal, load-bearing scaffold
    // limbs rather than a radial whorl of equivalent ascending rods.
    for primary_index in 0..7_u64 {
        let primary_seed = splitmix64(seed ^ primary_index.wrapping_mul(0x9e37_79b9));
        let phase = crown_phase
            + primary_index as f32 * 2.399_963_1
            + (unit_hash(primary_seed) - 0.5) * 0.52;
        let outward = Vec3::new(phase.cos(), 0.0, phase.sin());
        let tangent = Vec3::new(-phase.sin(), 0.0, phase.cos());
        let trunk_t = 0.29 + primary_index as f32 * 0.062;
        let trunk_scaled = trunk_t * 7.0;
        let trunk_segment = trunk_scaled.floor().min(6.0) as usize;
        let start =
            trunk_points[trunk_segment].lerp(trunk_points[trunk_segment + 1], trunk_scaled.fract());
        let lower_crown = 1.0 - primary_index as f32 / 7.0;
        let reach = 3.8 + lower_crown * 2.05 + unit_hash(primary_seed ^ 1) * 0.65;
        let rise = 1.0 + (1.0 - lower_crown) * 2.0 + unit_hash(primary_seed ^ 2) * 0.45;
        let sag = 0.85 + lower_crown * 0.7;
        let curve = (unit_hash(primary_seed ^ 3) - 0.5) * 1.15;
        let mut primary_points = [Vec3::ZERO; 6];
        for (point_index, point) in primary_points.iter_mut().enumerate() {
            let t = point_index as f32 / 5.0;
            let gravity = -sag * (core::f32::consts::PI * t).sin();
            let recovery = rise * t.powf(2.15);
            *point = start
                + outward * reach * t
                + Vec3::Y * (gravity + recovery)
                + tangent * curve * (core::f32::consts::PI * t).sin()
                + outward * 0.22 * (t * core::f32::consts::TAU).sin();
        }
        let primary_base_radius = 0.3 + lower_crown * 0.16 + unit_hash(primary_seed ^ 9) * 0.05;
        for segment_index in 0..5 {
            let t0 = segment_index as f32 / 5.0;
            let t1 = (segment_index + 1) as f32 / 5.0;
            branches.push(TreeBranchSegment {
                start: primary_points[segment_index],
                end: primary_points[segment_index + 1],
                start_radius: primary_base_radius * (1.0 - t0 * 0.78),
                end_radius: (primary_base_radius * (1.0 - t1 * 0.9)).max(0.018),
                depth: 1,
                primary_group: primary_index as u8,
                secondary_group: u16::MAX,
                is_limb_tip: segment_index == 4,
            });
        }

        for secondary_index in 0..8_u64 {
            let secondary_seed = splitmix64(primary_seed ^ (secondary_index + 11));
            let attach_t = 0.25 + secondary_index as f32 * 0.09;
            let primary_scaled = attach_t * 5.0;
            let segment = primary_scaled.floor().min(4.0) as usize;
            let secondary_start =
                primary_points[segment].lerp(primary_points[segment + 1], primary_scaled.fract());
            let side = if secondary_index & 1 == 0 { 1.0 } else { -1.0 };
            let yaw = phase
                + (secondary_index as f32 - 3.5) * 0.19
                + side * (0.5 + unit_hash(secondary_seed) * 0.38);
            let secondary_outward = Vec3::new(yaw.cos(), 0.0, yaw.sin());
            let inherited = (primary_points[segment + 1] - primary_points[segment]).normalize();
            let secondary_direction = (inherited * 0.34
                + secondary_outward * 0.68
                + Vec3::Y * (0.28 + unit_hash(secondary_seed ^ 1) * 0.3))
                .normalize();
            let secondary_length = 1.5 + unit_hash(secondary_seed ^ 2) * 0.85;
            let bend = tangent * side * (0.22 + unit_hash(secondary_seed ^ 3) * 0.3);
            let mut secondary_points = [Vec3::ZERO; 4];
            for (point_index, point) in secondary_points.iter_mut().enumerate() {
                let t = point_index as f32 / 3.0;
                *point = secondary_start
                    + secondary_direction * secondary_length * t
                    + bend * (core::f32::consts::PI * t).sin()
                    + Vec3::Y * (0.55 * t.powf(2.0) - 0.2 * (core::f32::consts::PI * t).sin());
            }
            let secondary_group = (primary_index * 8 + secondary_index) as u16;
            for segment_index in 0..3 {
                let t0 = segment_index as f32 / 3.0;
                let t1 = (segment_index + 1) as f32 / 3.0;
                branches.push(TreeBranchSegment {
                    start: secondary_points[segment_index],
                    end: secondary_points[segment_index + 1],
                    start_radius: 0.095 * (1.0 - t0 * 0.75),
                    end_radius: (0.095 * (1.0 - t1 * 0.75)).max(0.018),
                    depth: 2,
                    primary_group: primary_index as u8,
                    secondary_group,
                    is_limb_tip: segment_index == 2,
                });
            }
            for twig_index in 0..32_u64 {
                let twig_seed = splitmix64(secondary_seed ^ (twig_index + 23));
                let twig_start = secondary_points[2]
                    .lerp(secondary_points[3], 0.08 + twig_index as f32 / 31.0 * 0.9);
                let twig_yaw = yaw + (twig_index as f32 - 15.5) * 0.14;
                let twig_direction = (secondary_direction * 0.5
                    + Vec3::new(
                        twig_yaw.cos(),
                        0.22 + unit_hash(twig_seed ^ 8) * 0.62,
                        twig_yaw.sin(),
                    ) * 0.68)
                    .normalize();
                let twig_length = 0.62 + unit_hash(twig_seed) * 0.28;
                let twig_mid = twig_start + twig_direction * twig_length * 0.54;
                let twig_end = twig_start
                    + twig_direction * twig_length
                    + Vec3::Y * (0.1 + unit_hash(twig_seed ^ 4) * 0.1);
                for (segment_index, (start, end)) in [(twig_start, twig_mid), (twig_mid, twig_end)]
                    .into_iter()
                    .enumerate()
                {
                    branches.push(TreeBranchSegment {
                        start,
                        end,
                        start_radius: if segment_index == 0 { 0.018 } else { 0.011 },
                        end_radius: if segment_index == 0 { 0.011 } else { 0.0045 },
                        depth: 3,
                        primary_group: primary_index as u8,
                        secondary_group,
                        is_limb_tip: segment_index == 1,
                    });
                }
            }
        }
    }
    branches
}

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct TreeLeaf {
    pub(in crate::presentation) center: Vec3,
    pub(in crate::presentation) right: Vec3,
    pub(in crate::presentation) up: Vec3,
    pub(in crate::presentation) length: f32,
    pub(in crate::presentation) width: f32,
    pub(in crate::presentation) primary_group: u8,
    pub(in crate::presentation) secondary_group: u16,
    pub(in crate::presentation) shoot_id: u16,
    pub(in crate::presentation) shade: f32,
}

pub(in crate::presentation) fn procedural_oak_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
) -> Vec<TreeLeaf> {
    let mut leaves = Vec::new();
    for (shoot_index, shoot) in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
        .enumerate()
    {
        let direction = (shoot.end - shoot.start).normalize();
        let tangent = direction.cross(Vec3::Y).normalize_or_zero();
        let tangent = if tangent.length_squared() < 0.25 {
            Vec3::X
        } else {
            tangent
        };
        let binormal = direction.cross(tangent).normalize();
        for leaf_index in 0..28_u64 {
            let leaf_seed =
                splitmix64(seed ^ shoot_index as u64 ^ leaf_index.wrapping_mul(0x91e1_0da5));
            // Alternate leaves along the current shoot, then compress the last
            // flush into the terminal cluster characteristic of pedunculate oak.
            let along = if leaf_index < 12 {
                0.08 + leaf_index as f32 * 0.06
            } else {
                0.78 + (leaf_index - 12) as f32 / 15.0 * 0.2
            };
            let alternate = if leaf_index & 1 == 0 { 1.0 } else { -1.0 };
            let spiral = leaf_index as f32 * 2.399_963_1 + (unit_hash(leaf_seed ^ 2) - 0.5) * 0.32;
            let radial = (tangent * spiral.cos() + binormal * spiral.sin()).normalize();
            let leaf_up = (radial * 0.72
                + direction * 0.28
                + Vec3::Y * (0.12 + unit_hash(leaf_seed ^ 3) * 0.22))
                .normalize();
            let leaf_right = leaf_up.cross(direction).normalize_or_zero();
            let leaf_right = if leaf_right.length_squared() < 0.25 {
                tangent * alternate
            } else {
                leaf_right * alternate
            };
            leaves.push(TreeLeaf {
                center: shoot.start.lerp(shoot.end, along.min(0.98))
                    + radial * (0.018 + unit_hash(leaf_seed ^ 4) * 0.025),
                right: leaf_right.normalize(),
                up: leaf_up,
                length: 0.085 + unit_hash(leaf_seed ^ 5) * 0.065,
                width: 0.06 + unit_hash(leaf_seed ^ 6) * 0.04,
                primary_group: shoot.primary_group,
                secondary_group: shoot.secondary_group,
                shoot_id: shoot_index as u16,
                shade: 0.78 + unit_hash(leaf_seed ^ 7) * 0.32,
            });
        }
    }
    leaves
}

pub(in crate::presentation) fn oak_leaf_outline() -> Vec<Vec2> {
    let samples = 18;
    let mut outline = Vec::with_capacity(samples * 2 + 2);
    for side in [-1.0_f32, 1.0] {
        let range: Box<dyn Iterator<Item = usize>> = if side < 0.0 {
            Box::new(0..=samples)
        } else {
            Box::new((0..=samples).rev())
        };
        for index in range {
            let t = index as f32 / samples as f32;
            let blade = (core::f32::consts::PI * t).sin().powf(0.72);
            let lobes = (t * core::f32::consts::PI * 5.0).sin().abs().powf(0.48);
            let auricle = (-((t - 0.08) / 0.055).powi(2)).exp() * 0.34;
            let taper = if t > 0.86 { (1.0 - t) / 0.14 } else { 1.0 };
            let half_width = (0.18 + 0.82 * lobes + auricle) * blade * taper.max(0.0);
            outline.push(Vec2::new(side * half_width * 0.5, t - 0.5));
        }
    }
    outline
}

pub(in crate::presentation) fn procedural_oak_leaf_mesh(leaves: &[TreeLeaf]) -> Mesh {
    let outline = oak_leaf_outline();
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for leaf in leaves {
        let normal = leaf.right.cross(leaf.up).normalize();
        let base = positions.len() as u32;
        positions.push(leaf.center.to_array());
        normals.push(normal.to_array());
        uvs.push([0.5, 0.5]);
        colors.push([leaf.shade, leaf.shade, leaf.shade, 1.0]);
        for point in &outline {
            let cup = normal * (point.x.abs() * 0.018 + (point.y * 3.0).sin() * 0.002);
            let position = leaf.center
                + leaf.right * point.x * leaf.width
                + leaf.up * point.y * leaf.length
                + cup;
            positions.push(position.to_array());
            normals.push((normal + leaf.up * point.y * 0.14).normalize().to_array());
            uvs.push([point.x + 0.5, point.y + 0.5]);
            colors.push([leaf.shade, leaf.shade, leaf.shade, 1.0]);
        }
        for index in 0..outline.len() as u32 {
            let next = (index + 1) % outline.len() as u32;
            indices.extend_from_slice(&[base, base + 1 + index, base + 1 + next]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(in crate::presentation) fn procedural_tree_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for branch in branches
        .iter()
        .filter(|branch| branch.depth <= maximum_depth)
    {
        append_branch_tube(
            *branch,
            (8_u32.saturating_sub(u32::from(branch.depth))).max(4),
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

pub(in crate::presentation) fn append_branch_tube(
    branch: TreeBranchSegment,
    sides: u32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let direction = (branch.end - branch.start).normalize();
    let reference = if direction.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = direction.cross(reference).normalize();
    let forward = right.cross(direction).normalize();
    let start_center = branch.start - direction * branch.start_radius * 0.28;
    let end_center_position = branch.end + direction * branch.end_radius * 0.7;
    let base = positions.len() as u32;
    for ring in 0..2 {
        let (center, radius) = if ring == 0 {
            (start_center, branch.start_radius)
        } else {
            (end_center_position, branch.end_radius)
        };
        for side in 0..sides {
            let phase = side as f32 * core::f32::consts::TAU / sides as f32;
            let normal = right * phase.cos() + forward * phase.sin();
            positions.push((center + normal * radius).to_array());
            normals.push(normal.to_array());
            uvs.push([side as f32 / sides as f32, ring as f32]);
        }
    }
    for side in 0..sides {
        let next = (side + 1) % sides;
        indices.extend_from_slice(&[
            base + side,
            base + sides + side,
            base + sides + next,
            base + side,
            base + sides + next,
            base + next,
        ]);
    }
    let end_center = positions.len() as u32;
    positions.push(end_center_position.to_array());
    normals.push(direction.to_array());
    uvs.push([0.5, 1.0]);
    for side in 0..sides {
        let next = (side + 1) % sides;
        indices.extend_from_slice(&[end_center, base + sides + side, base + sides + next]);
    }
}

#[derive(Clone, Copy)]
pub(in crate::presentation) struct TreeCrownBounds {
    pub(in crate::presentation) minimum: Vec3,
    pub(in crate::presentation) maximum: Vec3,
}

impl TreeCrownBounds {
    pub(in crate::presentation) fn center(self) -> Vec3 {
        (self.minimum + self.maximum) * 0.5
    }

    pub(in crate::presentation) fn horizontal_span(self) -> f32 {
        (self.maximum.x - self.minimum.x).max(self.maximum.z - self.minimum.z)
    }

    pub(in crate::presentation) fn vertical_span(self) -> f32 {
        self.maximum.y - self.minimum.y
    }
}

pub(in crate::presentation) fn tree_crown_bounds(
    branches: &[TreeBranchSegment],
    mut includes: impl FnMut(&TreeBranchSegment) -> bool,
) -> TreeCrownBounds {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for branch in branches.iter().filter(|branch| includes(branch)) {
        minimum = minimum.min(branch.start).min(branch.end);
        maximum = maximum.max(branch.start).max(branch.end);
    }
    debug_assert!(minimum.is_finite() && maximum.is_finite());
    TreeCrownBounds { minimum, maximum }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_tree_has_a_deterministic_four_order_branch_hierarchy() {
        let branches = procedural_tree_skeleton(42);
        let counts = (0..=3)
            .map(|depth| {
                branches
                    .iter()
                    .filter(|branch| branch.depth == depth)
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(counts, vec![15, 35, 168, 3_584]);
        assert!(branches.iter().all(|branch| {
            branch.start.is_finite()
                && branch.end.is_finite()
                && branch.start.distance(branch.end) > 0.0
        }));
        assert!(
            branches.iter().all(|branch| {
                branch.start_radius > branch.end_radius && branch.end_radius > 0.0
            })
        );
    }
}
