use bevy::{
    asset::RenderAssetUsages,
    math::{FloatExt, Vec3},
    mesh::{Indices, PrimitiveTopology},
    prelude::Mesh,
};

use crate::presentation::volumetric::{SurfaceNetsGrid, extract_surface_nets};

use super::{TreeBranchSegment, branch_frame, transport_branch_frame};

const STANDARD_TRUNK_SIDES: u32 = 8;
const HERO_TRUNK_SIDES: u32 = 14;
const HERO_PRIMARY_SIDES: u32 = 10;

#[derive(Clone, Copy)]
struct WoodMeshStyle {
    trunk_sides: u32,
    primary_sides: u32,
    hero_cross_sections: bool,
}

impl WoodMeshStyle {
    const STANDARD: Self = Self {
        trunk_sides: STANDARD_TRUNK_SIDES,
        primary_sides: 7,
        hero_cross_sections: false,
    };

    const HERO: Self = Self {
        trunk_sides: HERO_TRUNK_SIDES,
        primary_sides: HERO_PRIMARY_SIDES,
        hero_cross_sections: true,
    };

    fn sides(self, depth: u8) -> u32 {
        match depth {
            0 => self.trunk_sides,
            1 => self.primary_sides,
            depth => (8_u32.saturating_sub(u32::from(depth))).max(4),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct WoodyAxisTopology {
    id: u16,
    parent: Option<u16>,
    parent_tangent: Option<bevy::math::Vec3>,
    is_main_trunk: bool,
}

pub(in crate::presentation) fn procedural_tree_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    procedural_tree_branch_mesh_with_style(
        branches,
        0,
        maximum_depth,
        WoodMeshStyle::STANDARD,
        None,
    )
}

/// Builds the close woody representation while retaining the skeleton as the
/// semantic source. Only this near mesh spends extra geometry on trunk form
/// and major biological attachments; farther woody LODs keep the standard
/// swept representation. A bounded implicit surface replaces the old root
/// tubes and lowest trunk span, then overlaps the resumed trunk sweep inside
/// the wood so no second exterior shell is visible.
pub(in crate::presentation) fn procedural_tree_hero_branch_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
) -> Mesh {
    procedural_tree_branch_mesh_with_style(branches, 0, maximum_depth, WoodMeshStyle::HERO, None)
}

fn procedural_tree_branch_mesh_with_style(
    branches: &[TreeBranchSegment],
    minimum_depth: u8,
    maximum_depth: u8,
    style: WoodMeshStyle,
    primary_group: Option<u8>,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let curve_ranges = woody_axis_ranges(branches);
    let topology = woody_axis_topology(branches, &curve_ranges);
    let mut curve_start = 0;
    let mut axis_index = 0;
    while curve_start < branches.len() {
        let curve_end = branches[curve_start..]
            .iter()
            .position(|branch| branch.is_limb_tip)
            .map(|offset| curve_start + offset + 1)
            .unwrap_or(branches.len());
        let curve = &branches[curve_start..curve_end];
        let implicit_root_axis =
            style.hero_cross_sections && minimum_depth == 0 && curve[0].depth == 0;
        let rendered_curve = if implicit_root_axis {
            if curve.len() > 1 {
                Some(&curve[1..])
            } else {
                None
            }
        } else {
            Some(curve)
        };
        if let Some(rendered_curve) = rendered_curve
            && (minimum_depth..=maximum_depth).contains(&curve[0].depth)
            && primary_group
                .is_none_or(|group| curve[0].depth > 0 && curve[0].primary_group == group)
        {
            let mut rendered_topology = topology[axis_index];
            if implicit_root_axis {
                rendered_topology.is_main_trunk = false;
            }
            append_branch_curve_tube(
                rendered_curve,
                style.sides(rendered_curve[0].depth),
                style,
                rendered_topology,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
            );
        }
        curve_start = curve_end;
        axis_index += 1;
    }
    if style.hero_cross_sections && minimum_depth == 0 {
        append_implicit_root_flare(
            branches,
            &curve_ranges,
            &topology,
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
    mesh.generate_tangents()
        .expect("procedural woody axes have valid metric UVs and normals");
    mesh
}

fn append_implicit_root_flare(
    branches: &[TreeBranchSegment],
    ranges: &[core::ops::Range<usize>],
    topology: &[WoodyAxisTopology],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let Some((trunk_index, trunk_range)) = ranges
        .iter()
        .enumerate()
        .find(|(index, _)| topology[*index].is_main_trunk)
    else {
        return;
    };
    let trunk = &branches[trunk_range.clone()];
    if trunk.len() < 2 {
        return;
    }
    let root_ranges = ranges
        .iter()
        .enumerate()
        .filter(|(index, range)| *index != trunk_index && branches[range.start].depth == 0)
        .map(|(_, range)| range.clone())
        .collect::<Vec<_>>();
    if root_ranges.is_empty() {
        return;
    }

    let trunk_base = trunk[0].start;
    let join = trunk[1].start;
    let join_direction = (trunk[1].end - trunk[1].start).normalize();
    let patch_top = join + join_direction * trunk[1].start_radius * 0.58;
    let mut minimum = trunk_base.min(patch_top);
    let mut maximum = trunk_base.max(patch_top);
    for range in &root_ranges {
        let segment = branches[range.start];
        let extent = Vec3::splat(segment.start_radius.max(segment.end_radius) * 1.4 + 0.1);
        minimum = minimum
            .min(segment.start - extent)
            .min(segment.end - extent);
        maximum = maximum
            .max(segment.start + extent)
            .max(segment.end + extent);
    }
    let trunk_padding = Vec3::new(
        trunk[0].start_radius * 1.55,
        trunk[0].start_radius * 0.5,
        trunk[0].start_radius * 1.55,
    );
    minimum = minimum.min(trunk_base - trunk_padding);
    maximum = maximum.max(patch_top + Vec3::splat(trunk[1].start_radius * 0.35));

    let field = |point: Vec3| {
        let trunk_field = tapered_capsule_field(
            point,
            trunk_base - (trunk[0].end - trunk[0].start).normalize() * trunk[0].start_radius * 0.22,
            patch_top,
            trunk[0].start_radius * 1.18,
            trunk[1].start_radius * 0.94,
        );
        root_ranges.iter().fold(trunk_field, |combined, range| {
            let segment = branches[range.start];
            let root = tapered_capsule_field(
                point,
                segment.start,
                segment.end,
                segment.start_radius * 1.1,
                segment.end_radius * 0.9,
            );
            smooth_min(combined, root, trunk[0].start_radius * 0.18)
        })
    };
    let surface = extract_surface_nets(
        SurfaceNetsGrid {
            sample_counts: [34, 26, 34],
            minimum,
            maximum,
        },
        field,
    )
    .expect("bounded oak root field produces a finite extracted surface");
    // The hero material samples bark in object space, so these UVs are only a
    // finite fallback for tangent generation and diagnostic mesh inspection.
    // Avoid interpolating between unrelated root-axis frames: the dominant
    // smooth normal selects the least-stretched projection without duplicating
    // Surface Nets' shared close-geometry vertices.
    let base_index = positions.len() as u32;
    for (position, normal) in surface.positions.iter().zip(&surface.normals) {
        positions.push(*position);
        normals.push(*normal);
        uvs.push(box_projected_bark_uv(
            Vec3::from_array(*position),
            Vec3::from_array(*normal),
        ));
    }
    indices.extend(surface.indices.into_iter().map(|index| base_index + index));

    debug_assert!(uvs.iter().flatten().all(|component| component.is_finite()));
}

fn box_projected_bark_uv(point: Vec3, normal: Vec3) -> [f32; 2] {
    const BARK_WIDTH_METRES: f32 = 1.0;
    const BARK_HEIGHT_METRES: f32 = 2.0;
    let dominant = normal.abs();
    if dominant.y >= dominant.x && dominant.y >= dominant.z {
        [point.x / BARK_WIDTH_METRES, point.z / BARK_WIDTH_METRES]
    } else if dominant.x >= dominant.z {
        [point.z / BARK_WIDTH_METRES, point.y / BARK_HEIGHT_METRES]
    } else {
        [point.x / BARK_WIDTH_METRES, point.y / BARK_HEIGHT_METRES]
    }
}

fn tapered_capsule_field(
    point: Vec3,
    start: Vec3,
    end: Vec3,
    start_radius: f32,
    end_radius: f32,
) -> f32 {
    let axis = end - start;
    let amount = ((point - start).dot(axis) / axis.length_squared()).clamp(0.0, 1.0);
    point.distance(start + axis * amount) - start_radius.lerp(end_radius, amount)
}

fn smooth_min(left: f32, right: f32, radius: f32) -> f32 {
    let amount = (0.5 + 0.5 * (right - left) / radius).clamp(0.0, 1.0);
    right.lerp(left, amount) - radius * amount * (1.0 - amount)
}

fn woody_axis_ranges(branches: &[TreeBranchSegment]) -> Vec<core::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < branches.len() {
        let end = branches[start..]
            .iter()
            .position(|branch| branch.is_limb_tip)
            .map(|offset| start + offset + 1)
            .unwrap_or(branches.len());
        ranges.push(start..end);
        start = end;
    }
    ranges
}

fn woody_axis_topology(
    branches: &[TreeBranchSegment],
    ranges: &[core::ops::Range<usize>],
) -> Vec<WoodyAxisTopology> {
    let main_trunk = ranges
        .iter()
        .enumerate()
        .filter(|(_, range)| branches[range.start].depth == 0)
        .max_by(|(_, left), (_, right)| {
            let height = |range: &core::ops::Range<usize>| {
                branches[range.clone()]
                    .iter()
                    .map(|segment| segment.end.y)
                    .fold(f32::NEG_INFINITY, f32::max)
            };
            height(left).total_cmp(&height(right))
        })
        .map(|(index, _)| index);
    ranges
        .iter()
        .enumerate()
        .map(|(axis_index, range)| {
            let first = branches[range.start];
            // Only the seven primary scaffolds need an attachment relationship
            // for the hero mesh. Descendant axes retain ordinary sweep collars,
            // avoiding an expensive all-pairs graph reconstruction across the
            // thousands of twigs in a mature oak.
            let parent = (first.depth == 1)
                .then(|| {
                    ranges
                        .iter()
                        .enumerate()
                        .filter(|(_, candidate)| branches[candidate.start].depth < first.depth)
                        .map(|(candidate_index, candidate)| {
                            let (distance, tangent) = branches[candidate.clone()]
                                .iter()
                                .map(|segment| {
                                    let direction = segment.end - segment.start;
                                    let amount = ((first.start - segment.start).dot(direction)
                                        / direction.length_squared())
                                    .clamp(0.0, 1.0);
                                    (
                                        first.start.distance(segment.start + direction * amount),
                                        direction.normalize(),
                                    )
                                })
                                .min_by(|left, right| left.0.total_cmp(&right.0))
                                .expect("a woody axis has at least one segment");
                            (candidate_index, distance, tangent)
                        })
                        .min_by(|left, right| left.1.total_cmp(&right.1))
                        .map(|(index, _, tangent)| (index as u16, tangent))
                })
                .flatten();
            WoodyAxisTopology {
                id: axis_index as u16,
                parent: parent.map(|(id, _)| id),
                parent_tangent: parent.map(|(_, tangent)| tangent),
                is_main_trunk: main_trunk == Some(axis_index),
            }
        })
        .collect()
}

pub(in crate::presentation) fn procedural_tree_descendant_group_mesh(
    branches: &[TreeBranchSegment],
    maximum_depth: u8,
    primary_group: u8,
) -> Mesh {
    procedural_tree_branch_mesh_with_style(
        branches,
        2,
        maximum_depth,
        WoodMeshStyle::STANDARD,
        Some(primary_group),
    )
}

fn append_branch_curve_tube(
    curve: &[TreeBranchSegment],
    sides: u32,
    style: WoodMeshStyle,
    topology: WoodyAxisTopology,
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
    let mut rings = Vec::with_capacity(curve.len() + 4);
    if first.depth > 0 {
        let parent_tangent = if style.hero_cross_sections {
            topology.parent_tangent.unwrap_or(first_direction)
        } else {
            first_direction
        };
        let attachment_recess = first.start_radius
            * if style.hero_cross_sections {
                0.42
            } else {
                0.22
            };
        // One parent-aware basal collar belongs at the biological attachment.
        // Its first tangent leans into the parent surface; subsequent rings
        // resolve to the child axis, avoiding a pasted-on cylinder profile.
        rings.push((
            first.start - first_direction * attachment_recess,
            first.start_radius
                * if style.hero_cross_sections {
                    1.34
                } else {
                    1.18
                },
            (first_direction + parent_tangent * 0.32).normalize(),
        ));
        if style.hero_cross_sections && first.depth == 1 {
            rings.push((
                first.start + first_direction * first.start_radius * 0.18,
                first.start_radius * 1.16,
                first_direction,
            ));
        }
        rings.push((
            first.start + first_direction * first.start_radius * 0.5,
            first.start_radius * 0.94,
            first_direction,
        ));
    } else {
        let base_scale = if style.hero_cross_sections && topology.is_main_trunk {
            1.48
        } else {
            1.0
        };
        rings.push((
            first.start - first_direction * first.start_radius * 0.18,
            first.start_radius * base_scale,
            first_direction,
        ));
        if style.hero_cross_sections && topology.is_main_trunk {
            rings.push((
                first.start + first_direction * first.start_radius * 0.45,
                first.start_radius * 1.17,
                first_direction,
            ));
        }
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
            let radial = right * phase.cos() + forward * phase.sin();
            let axis_twist = topology.id as f32 * 0.73
                + f32::from(topology.parent.unwrap_or(topology.id)) * 0.29
                + accumulated_distance * 0.19;
            let eccentricity = if style.hero_cross_sections {
                match first.depth {
                    0 => 0.075,
                    1 => 0.055,
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let buttress = if style.hero_cross_sections && topology.is_main_trunk && ring == 0 {
                0.12 * (5.0 * phase + topology.id as f32).cos().max(0.0)
            } else {
                0.0
            };
            let radial_scale = 1.0 + eccentricity * (2.0 * phase + axis_twist).cos() + buttress;
            positions.push((center + radial * radius * radial_scale).to_array());
            normals.push(radial.to_array());
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
    use std::time::Instant;

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

    #[test]
    fn hero_topology_recovers_the_main_trunk_and_primary_parents() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let ranges = woody_axis_ranges(&branches);
        let topology = woody_axis_topology(&branches, &ranges);
        assert_eq!(ranges.len(), topology.len());
        assert_eq!(topology.iter().filter(|axis| axis.is_main_trunk).count(), 1);

        let primary_axes = ranges
            .iter()
            .zip(&topology)
            .filter(|(range, _)| branches[range.start].depth == 1)
            .collect::<Vec<_>>();
        assert_eq!(primary_axes.len(), 7);
        assert!(primary_axes.iter().all(|(_, axis)| axis.parent.is_some()));
        assert!(
            primary_axes
                .iter()
                .all(|(_, axis)| axis.parent_tangent.is_some_and(Vec3::is_normalized))
        );
    }

    #[test]
    fn hero_wood_spends_bounded_geometry_only_on_the_near_representation() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let standard_trunk = procedural_tree_branch_mesh(&branches, 0);
        let hero_trunk = procedural_tree_hero_branch_mesh(&branches, 0);
        let standard_vertices = standard_trunk.count_vertices();
        let hero_vertices = hero_trunk.count_vertices();
        assert!(hero_vertices > standard_vertices);
        assert!(
            hero_vertices < 8_000,
            "the close implicit root replacement has a fixed hero-only geometry budget: {hero_vertices} vertices"
        );

        let standard_primary = procedural_tree_branch_mesh_with_style(
            &branches,
            2,
            3,
            WoodMeshStyle::STANDARD,
            Some(0),
        );
        let descendant = procedural_tree_descendant_group_mesh(&branches, 3, 0);
        assert!(descendant.count_vertices() > 0);
        assert_eq!(
            descendant.count_vertices(),
            standard_primary.count_vertices()
        );

        let second_descendant = procedural_tree_descendant_group_mesh(&branches, 3, 0);
        assert_eq!(
            descendant.attribute(Mesh::ATTRIBUTE_POSITION),
            second_descendant.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(descendant.indices(), second_descendant.indices());
    }

    #[test]
    fn reports_reference_hero_wood_generation_budget() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let started = Instant::now();
        let major_wood = procedural_tree_hero_branch_mesh(&branches, 1);
        let elapsed = started.elapsed();
        let vertices = major_wood.count_vertices();
        let triangles = major_wood.indices().map_or(0, |indices| indices.len() / 3);
        eprintln!("reference hero wood: {vertices} vertices, {triangles} triangles, {elapsed:?}");
        assert!(vertices < 50_000);
        assert!(triangles < 75_000);
        assert!(elapsed.as_millis() < 250);
    }
}
