use bevy::{
    asset::RenderAssetUsages,
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
    let visible = branches
        .iter()
        .filter(|branch| branch.depth <= maximum_depth)
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
        let branches = procedural_tree_skeleton(42, 0.0);
        let mesh = procedural_tree_branch_mesh(&branches, 0);
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
        assert!(Vec3::from_array(positions[0]).distance(Vec3::from_array(positions[8])) < 1.0e-5);
        assert_eq!(uvs[0][0], 0.0);
        assert!(uvs[8][0] >= 1.0 && uvs[8][0].fract().abs() < f32::EPSILON);
        assert!(
            uvs.iter().map(|uv| uv[1]).fold(0.0_f32, f32::max) > 0.5,
            "a two-metre-long axis must advance a full physical bark tile"
        );
    }
}
