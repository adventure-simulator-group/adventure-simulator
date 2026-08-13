use bevy::{
    asset::RenderAssetUsages,
    math::{FloatExt, Vec2, Vec3, Vec3Swizzles},
    mesh::{Indices, PrimitiveTopology},
    prelude::Mesh,
};

use super::{TreeBranchSegment, branch_frame, transport_branch_frame};

pub(in crate::presentation) fn procedural_tree_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let flare = RootFlareField::from_branches(branches, maximum_depth);
    if let Some(flare) = &flare {
        append_root_flare_mesh(flare, &mut positions, &mut normals, &mut uvs, &mut indices);
    }
    let visible = branches
        .iter()
        .filter(|branch| {
            branch.depth <= maximum_depth
                && flare
                    .as_ref()
                    .is_none_or(|flare| !flare.contains_segment(branch))
        })
        .copied()
        .collect::<Vec<_>>();
    let mut curve_start = 0;
    while curve_start < visible.len() {
        let curve_end = visible[curve_start..]
            .iter()
            .position(|branch| branch.is_limb_tip)
            .map(|offset| curve_start + offset + 1)
            .unwrap_or(visible.len());
        let curve = &visible[curve_start..curve_end];
        append_branch_curve_tube(
            curve,
            (8_u32.saturating_sub(u32::from(curve[0].depth))).max(4),
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
        );
        curve_start = curve_end;
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.generate_tangents()
        .expect("procedural woody axes have valid metric UVs and normals");
    mesh
}

#[derive(Clone)]
struct RootFlareField {
    segments: Vec<TreeBranchSegment>,
    minimum: Vec3,
    maximum: Vec3,
    cell: f32,
    blend: f32,
}

impl RootFlareField {
    fn from_branches(branches: &[TreeBranchSegment], _maximum_depth: u8) -> Option<Self> {
        // Hybrid skeleton/sweep/implicit architecture adapted from:
        // https://gist.github.com/halbe/5613d15ecfa84e80c04a56f34a656456
        // Only the non-tubular root flare is contoured; ordinary woody runs
        // retain the cheaper generalized-cylinder mesh below.
        let base = branches
            .iter()
            .filter(|branch| branch.depth == 0)
            .map(|branch| branch.start.y.min(branch.end.y))
            .fold(f32::INFINITY, f32::min);
        let cutoff = base + 1.65;
        let segments = branches
            .iter()
            .filter(|branch| branch.depth == 0 && branch.start.y.min(branch.end.y) < cutoff)
            .copied()
            .collect::<Vec<_>>();
        let has_roots = segments.iter().any(|segment| {
            let axis = segment.end - segment.start;
            axis.xz().length() > axis.y.abs() * 0.7
        });
        if !has_roots {
            return None;
        }
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for segment in &segments {
            let extent = Vec3::splat(segment.start_radius.max(segment.end_radius) + 0.22);
            minimum = minimum
                .min(segment.start - extent)
                .min(segment.end - extent);
            maximum = maximum
                .max(segment.start + extent)
                .max(segment.end + extent);
        }
        minimum.y = minimum.y.max(base - 0.42);
        maximum.y = maximum.y.min(cutoff + 0.16);
        Some(Self {
            segments,
            minimum,
            maximum,
            cell: 0.105,
            blend: 0.18,
        })
    }

    fn contains_segment(&self, segment: &TreeBranchSegment) -> bool {
        segment.depth == 0
            && segment.start.y <= self.maximum.y - self.cell
            && segment.end.y <= self.maximum.y - self.cell
    }

    fn distance(&self, point: Vec3) -> f32 {
        self.segments.iter().fold(f32::INFINITY, |field, segment| {
            smooth_min(field, capsule_distance(point, segment), self.blend)
        })
    }

    fn normal(&self, point: Vec3) -> Vec3 {
        let epsilon = self.cell * 0.45;
        Vec3::new(
            self.distance(point + Vec3::X * epsilon) - self.distance(point - Vec3::X * epsilon),
            self.distance(point + Vec3::Y * epsilon) - self.distance(point - Vec3::Y * epsilon),
            self.distance(point + Vec3::Z * epsilon) - self.distance(point - Vec3::Z * epsilon),
        )
        .normalize_or_zero()
    }

    fn uv(&self, point: Vec3) -> Vec2 {
        let segment = self
            .segments
            .iter()
            .min_by(|left, right| {
                capsule_distance(point, left).total_cmp(&capsule_distance(point, right))
            })
            .expect("root flare has source segments");
        let axis = segment.end - segment.start;
        let length = axis.length();
        let tangent = axis / length;
        let along = ((point - segment.start).dot(tangent) / length).clamp(0.0, 1.0);
        let center = segment.start.lerp(segment.end, along);
        let (right, forward) = branch_frame(tangent);
        let radial = point - center;
        let theta = radial.dot(forward).atan2(radial.dot(right));
        Vec2::new(theta / core::f32::consts::TAU, (length * along) / 2.0)
    }
}

fn capsule_distance(point: Vec3, segment: &TreeBranchSegment) -> f32 {
    let axis = segment.end - segment.start;
    let length_squared = axis.length_squared();
    let along = if length_squared > 1.0e-6 {
        ((point - segment.start).dot(axis) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let radius = segment
        .start_radius
        .lerp(segment.end_radius, along.powf(0.64));
    point.distance(segment.start + axis * along) - radius
}

fn smooth_min(left: f32, right: f32, blend: f32) -> f32 {
    if !left.is_finite() {
        return right;
    }
    let h = (0.5 + 0.5 * (right - left) / blend).clamp(0.0, 1.0);
    right.lerp(left, h) - blend * h * (1.0 - h)
}

const CUBE_CORNERS: [Vec3; 8] = [
    Vec3::new(0.0, 0.0, 0.0),
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(1.0, 1.0, 0.0),
    Vec3::new(0.0, 1.0, 0.0),
    Vec3::new(0.0, 0.0, 1.0),
    Vec3::new(1.0, 0.0, 1.0),
    Vec3::new(1.0, 1.0, 1.0),
    Vec3::new(0.0, 1.0, 1.0),
];
const CUBE_TETRAHEDRA: [[usize; 4]; 6] = [
    [0, 5, 1, 6],
    [0, 1, 2, 6],
    [0, 2, 3, 6],
    [0, 3, 7, 6],
    [0, 7, 4, 6],
    [0, 4, 5, 6],
];

fn append_root_flare_mesh(
    field: &RootFlareField,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let size = field.maximum - field.minimum;
    let dimensions = (size / field.cell).ceil().as_uvec3();
    for z in 0..dimensions.z {
        for y in 0..dimensions.y {
            for x in 0..dimensions.x {
                let origin = field.minimum + Vec3::new(x as f32, y as f32, z as f32) * field.cell;
                let points = CUBE_CORNERS.map(|corner| origin + corner * field.cell);
                let values = points.map(|point| field.distance(point));
                for tetrahedron in CUBE_TETRAHEDRA {
                    polygonize_tetrahedron(
                        field,
                        tetrahedron.map(|i| points[i]),
                        tetrahedron.map(|i| values[i]),
                        positions,
                        normals,
                        uvs,
                        indices,
                    );
                }
            }
        }
    }
}

fn polygonize_tetrahedron(
    field: &RootFlareField,
    points: [Vec3; 4],
    values: [f32; 4],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let inside = (0..4)
        .filter(|&index| values[index] < 0.0)
        .collect::<Vec<_>>();
    let outside = (0..4)
        .filter(|&index| values[index] >= 0.0)
        .collect::<Vec<_>>();
    let edge = |a: usize, b: usize| {
        let t = (values[a] / (values[a] - values[b])).clamp(0.0, 1.0);
        points[a].lerp(points[b], t)
    };
    let triangles = match inside.len() {
        0 | 4 => return,
        1 => vec![[
            edge(inside[0], outside[0]),
            edge(inside[0], outside[1]),
            edge(inside[0], outside[2]),
        ]],
        3 => vec![[
            edge(outside[0], inside[0]),
            edge(outside[0], inside[2]),
            edge(outside[0], inside[1]),
        ]],
        2 => {
            let ac = edge(inside[0], outside[0]);
            let ad = edge(inside[0], outside[1]);
            let bc = edge(inside[1], outside[0]);
            let bd = edge(inside[1], outside[1]);
            vec![[ac, bc, bd], [ac, bd, ad]]
        }
        _ => unreachable!(),
    };
    if triangles.is_empty() {
        return;
    }
    for mut vertices in triangles {
        let face = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
        let mean_normal = vertices
            .iter()
            .map(|point| field.normal(*point))
            .sum::<Vec3>();
        if face.dot(mean_normal) < 0.0 {
            vertices.swap(1, 2);
        }
        let base = positions.len() as u32;
        let mut triangle_uvs = vertices.map(|vertex| field.uv(vertex));
        let minimum_u = triangle_uvs
            .iter()
            .map(|uv| uv.x)
            .fold(f32::INFINITY, f32::min);
        let maximum_u = triangle_uvs
            .iter()
            .map(|uv| uv.x)
            .fold(f32::NEG_INFINITY, f32::max);
        if maximum_u - minimum_u > 0.5 {
            for uv in &mut triangle_uvs {
                if uv.x < 0.0 {
                    uv.x += 1.0;
                }
            }
        }
        for (vertex, uv) in vertices.into_iter().zip(triangle_uvs) {
            positions.push(vertex.to_array());
            normals.push(field.normal(vertex).to_array());
            uvs.push(uv.to_array());
        }
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
}

pub(in crate::presentation) fn procedural_tree_branch_group_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
    primary_group: u8,
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
    procedural_tree_branch_mesh(&group, maximum_depth)
}

fn append_branch_curve_tube(
    curve: &[TreeBranchSegment],
    sides: u32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    const BARK_TEXTURE_WIDTH_METRES: f32 = 1.0;
    const BARK_TEXTURE_HEIGHT_METRES: f32 = 2.0;

    let first = curve[0];
    let last = curve[curve.len() - 1];
    let first_direction = (first.end - first.start).normalize();
    let last_direction = (last.end - last.start).normalize();
    let mut rings = Vec::with_capacity(curve.len() + 2);
    if first.depth > 0 {
        // One basal collar belongs at the biological attachment. Repeating a
        // collar at every polygon joint makes a smooth axis look assembled.
        rings.push((
            first.start - first_direction * first.start_radius * 0.22,
            first.start_radius * 1.18,
            first_direction,
        ));
        rings.push((
            first.start + first_direction * first.start_radius * 0.5,
            first.start_radius * 0.94,
            first_direction,
        ));
    } else {
        rings.push((
            first.start - first_direction * first.start_radius * 0.18,
            first.start_radius,
            first_direction,
        ));
    }
    for (index, branch) in curve.iter().enumerate() {
        let direction = (branch.end - branch.start).normalize();
        let tangent = if index + 1 < curve.len() {
            (direction + (curve[index + 1].end - curve[index + 1].start).normalize()).normalize()
        } else {
            direction
        };
        rings.push((branch.end, branch.end_radius, tangent));
    }
    let ring_stride = sides + 1;
    // A whole-number wrap keeps the duplicated cylindrical seam texel-exact.
    // Choose it from the biological base circumference so scale is physical
    // and stable along a tapering axis rather than resetting per segment.
    let circumference_tiles = (core::f32::consts::TAU * first.start_radius
        / BARK_TEXTURE_WIDTH_METRES)
        .round()
        .max(1.0);
    let base = positions.len() as u32;
    let mut accumulated_distance = 0.0;
    let (mut right, mut forward) = branch_frame(rings[0].2);
    let mut previous_center = rings[0].0;
    let mut previous_tangent = rings[0].2;
    for (ring, (center, radius, tangent)) in rings.iter().copied().enumerate() {
        if ring > 0 {
            accumulated_distance += center.distance(previous_center);
            (right, forward) = transport_branch_frame(previous_tangent, right, tangent);
        }
        for side in 0..=sides {
            let phase = side as f32 * core::f32::consts::TAU / sides as f32;
            let normal = right * phase.cos() + forward * phase.sin();
            positions.push((center + normal * radius).to_array());
            normals.push(normal.to_array());
            uvs.push([
                side as f32 / sides as f32 * circumference_tiles,
                accumulated_distance / BARK_TEXTURE_HEIGHT_METRES,
            ]);
        }
        previous_center = center;
        previous_tangent = tangent;
    }
    for ring in 0..rings.len() as u32 - 1 {
        let from = base + ring * ring_stride;
        let to = from + ring_stride;
        for side in 0..sides {
            let next = side + 1;
            indices.extend_from_slice(&[
                from + side,
                to + side,
                to + next,
                from + side,
                to + next,
                from + next,
            ]);
        }
    }
    let end_ring = base + (rings.len() as u32 - 1) * ring_stride;
    if last.is_limb_tip {
        // A pair of shrinking rings gives every terminal axis a rounded,
        // natural taper. Flat caps read as sawn-off limbs and become black
        // rectangular artifacts in the descendant renders.
        let shoulder = positions.len() as u32;
        let bud_length = last.end_radius;
        let (right, forward) = transport_branch_frame(previous_tangent, right, last_direction);
        let mut terminal_distance = accumulated_distance;
        let mut terminal_center = last.end;
        for (distance, radius_scale) in [(0.55, 0.58), (0.92, 0.12)] {
            let center = last.end + last_direction * bud_length * distance;
            terminal_distance += center.distance(terminal_center);
            let radius = last.end_radius * radius_scale;
            for side in 0..=sides {
                let phase = side as f32 * core::f32::consts::TAU / sides as f32;
                let radial = right * phase.cos() + forward * phase.sin();
                let normal = (radial * 0.75 + last_direction * 0.66).normalize();
                positions.push((center + radial * radius).to_array());
                normals.push(normal.to_array());
                uvs.push([
                    side as f32 / sides as f32 * circumference_tiles,
                    terminal_distance / BARK_TEXTURE_HEIGHT_METRES,
                ]);
            }
            terminal_center = center;
        }
        for ring in 0..2_u32 {
            let from = if ring == 0 { end_ring } else { shoulder };
            let to = shoulder + ring * ring_stride;
            for side in 0..sides {
                let next = side + 1;
                indices.extend_from_slice(&[
                    from + side,
                    to + side,
                    to + next,
                    from + side,
                    to + next,
                    from + next,
                ]);
            }
        }
        let tip = positions.len() as u32;
        positions.push((last.end + last_direction * bud_length).to_array());
        normals.push(last_direction.to_array());
        uvs.push([
            0.0,
            (accumulated_distance + bud_length) / BARK_TEXTURE_HEIGHT_METRES,
        ]);
        for side in 0..sides {
            let next = side + 1;
            indices.extend_from_slice(&[
                tip,
                shoulder + ring_stride + side,
                shoulder + ring_stride + next,
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::obstacles::tree::geometry::procedural_tree_skeleton;
    use bevy::{math::Vec3, mesh::VertexAttributeValues};

    #[test]
    fn branch_mesh_has_metric_seam_safe_uvs_and_valid_tangents() {
        let branches = [TreeBranchSegment {
            start: Vec3::ZERO,
            end: Vec3::Y * 2.0,
            start_radius: 0.4,
            end_radius: 0.3,
            depth: 1,
            primary_group: 0,
            secondary_group: 0,
            is_limb_tip: true,
        }];
        let mesh = procedural_tree_branch_mesh(&branches, 1);
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("branch mesh has float UVs");
        };
        let Some(VertexAttributeValues::Float32x4(tangents)) =
            mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!("normal-mapped branch mesh has float tangents");
        };
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("branch mesh has float positions");

        assert_eq!(uvs.len(), tangents.len());
        assert!(uvs.iter().flatten().all(|component| component.is_finite()));
        assert!(
            tangents
                .iter()
                .flatten()
                .all(|component| component.is_finite())
        );
        assert!(tangents.iter().all(|tangent| {
            let length = Vec3::from_array([tangent[0], tangent[1], tangent[2]]).length();
            (length - 1.0).abs() < 1.0e-3 && tangent[3].abs() == 1.0
        }));
        assert!(Vec3::from_array(positions[0]).distance(Vec3::from_array(positions[7])) < 1.0e-5);
        assert_eq!(uvs[0][0], 0.0);
        assert!(uvs[7][0] >= 1.0 && uvs[7][0].fract().abs() < f32::EPSILON);
        assert!(
            uvs.iter().map(|uv| uv[1]).fold(0.0_f32, f32::max) > 0.5,
            "a two-metre-long axis must advance a full physical bark tile"
        );
    }

    #[test]
    fn root_flare_is_a_bounded_smooth_union_with_finite_surface_data() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let field = RootFlareField::from_branches(&branches, 0).expect("oak has a root flare");
        assert!(field.maximum.x - field.minimum.x < 4.0);
        assert!(field.maximum.z - field.minimum.z < 4.0);
        assert!(field.maximum.y - field.minimum.y < 2.6);

        assert!(smooth_min(-0.1, -0.1, field.blend) < -0.1);
        for point in [Vec3::ZERO, field.minimum, field.maximum] {
            let hard_union = field
                .segments
                .iter()
                .map(|segment| capsule_distance(point, segment))
                .fold(f32::INFINITY, f32::min);
            assert!(field.distance(point) <= hard_union + 1.0e-6);
        }

        let mesh = procedural_tree_branch_mesh(&branches, 0);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .unwrap();
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attribute| attribute.as_float3())
            .unwrap();
        assert!(positions.len() > 1_000);
        assert_eq!(positions.len(), normals.len());
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        assert!(normals.iter().flatten().all(|value| value.is_finite()));
        assert!(
            normals
                .iter()
                .all(|normal| { (Vec3::from_array(*normal).length() - 1.0).abs() < 1.0e-3 })
        );
    }
}
