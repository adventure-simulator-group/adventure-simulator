use bevy::math::Vec3;

use crate::{RoofEnclosureFace, RoofFace};

const ROOF_VERTEX_TOLERANCE_SQUARED: f32 = 0.000_004;
const ENCLOSURE_THICKNESS_METRES: f32 = 0.16;

#[derive(Clone, Copy, Debug)]
pub struct RoofSurfaceTriangle {
    pub positions: [Vec3; 3],
    pub normal: Vec3,
    pub surface: RoofSurface,
}

/// Architectural side of a tessellated roof solid.
///
/// Exact-detail rendering uses this semantic boundary to give the weather skin
/// and its closed perimeter the authored roof covering while routing the
/// room-facing slope to interior boarding. Shell LODs deliberately retain a
/// single material batch and may ignore this classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoofSurface {
    Weather,
    Interior,
    Boundary,
    Enclosure,
}

pub fn tessellate_roof_face(face: &RoofFace) -> Vec<RoofSurfaceTriangle> {
    let normal = face.plane.normal.normalize_or_zero();
    let offset = -normal * face.thickness_metres;
    let mut outer = face.polygon.clone();
    let mut remaining_cutouts = Vec::new();
    for cutout in &face.cutouts {
        if let Some(notched) = boundary_notched_polygon(&outer, cutout) {
            outer = notched;
        } else {
            remaining_cutouts.push(cutout.clone());
        }
    }
    remove_collinear_vertices(&mut outer);

    let mut vertices = outer;
    let mut hole_indices = Vec::new();
    for cutout in &remaining_cutouts {
        hole_indices.push(vertices.len() as u32);
        vertices.extend(cutout.iter().copied());
    }
    let mut indices = Vec::new();
    earcut::Earcut::<f32>::new().earcut(
        vertices.iter().map(|point| [point.x, point.z]),
        &hole_indices,
        &mut indices,
    );

    let mut triangles = Vec::new();
    let mut top_edges = Vec::new();
    for triangle in indices.as_chunks::<3>().0 {
        let mut top = triangle.map(|index| vertices[index as usize]);
        orient_triangle(&mut top, normal);
        for index in 0..3 {
            top_edges.push((top[index], top[(index + 1) % 3]));
        }
        triangles.push(RoofSurfaceTriangle {
            positions: top,
            normal,
            surface: RoofSurface::Weather,
        });
        triangles.push(RoofSurfaceTriangle {
            positions: [top[2] + offset, top[1] + offset, top[0] + offset],
            normal: -normal,
            surface: RoofSurface::Interior,
        });
    }
    append_boundary_sides(&mut triangles, &top_edges, offset);
    triangles
}

pub fn tessellate_roof_enclosure(face: &RoofEnclosureFace) -> Vec<RoofSurfaceTriangle> {
    if face.polygon.len() < 3 {
        return Vec::new();
    }
    let normal = (face.polygon[1] - face.polygon[0])
        .cross(face.polygon[2] - face.polygon[0])
        .normalize_or_zero();
    let offset = -normal * ENCLOSURE_THICKNESS_METRES;
    let mut triangles = triangulate_fan(&face.polygon, normal, RoofSurface::Enclosure);
    triangles.extend(triangulate_fan(
        &face
            .polygon
            .iter()
            .rev()
            .map(|point| *point + offset)
            .collect::<Vec<_>>(),
        -normal,
        RoofSurface::Enclosure,
    ));
    for index in 0..face.polygon.len() {
        let next = (index + 1) % face.polygon.len();
        append_quad(
            &mut triangles,
            [
                face.polygon[index],
                face.polygon[index] + offset,
                face.polygon[next] + offset,
                face.polygon[next],
            ],
            RoofSurface::Enclosure,
        );
    }
    triangles
}

fn append_boundary_sides(
    triangles: &mut Vec<RoofSurfaceTriangle>,
    edges: &[(Vec3, Vec3)],
    offset: Vec3,
) {
    for (index, (start, end)) in edges.iter().copied().enumerate() {
        let uses = edges
            .iter()
            .enumerate()
            .filter(|(candidate_index, (candidate_start, candidate_end))| {
                *candidate_index != index
                    && ((same_point(*candidate_start, start) && same_point(*candidate_end, end))
                        || (same_point(*candidate_start, end) && same_point(*candidate_end, start)))
            })
            .count();
        if uses == 0 {
            append_quad(
                triangles,
                [start, start + offset, end + offset, end],
                RoofSurface::Boundary,
            );
        }
    }
}

fn append_quad(
    triangles: &mut Vec<RoofSurfaceTriangle>,
    positions: [Vec3; 4],
    surface: RoofSurface,
) {
    let normal = (positions[1] - positions[0])
        .cross(positions[2] - positions[0])
        .normalize_or_zero();
    triangles.extend([
        RoofSurfaceTriangle {
            positions: [positions[0], positions[1], positions[2]],
            normal,
            surface,
        },
        RoofSurfaceTriangle {
            positions: [positions[0], positions[2], positions[3]],
            normal,
            surface,
        },
    ]);
}

fn triangulate_fan(
    polygon: &[Vec3],
    normal: Vec3,
    surface: RoofSurface,
) -> Vec<RoofSurfaceTriangle> {
    (1..polygon.len().saturating_sub(1))
        .map(|index| {
            let mut positions = [polygon[0], polygon[index], polygon[index + 1]];
            orient_triangle(&mut positions, normal);
            RoofSurfaceTriangle {
                positions,
                normal,
                surface,
            }
        })
        .collect()
}

fn orient_triangle(positions: &mut [Vec3; 3], normal: Vec3) {
    if (positions[1] - positions[0])
        .cross(positions[2] - positions[0])
        .dot(normal)
        < 0.0
    {
        positions.swap(1, 2);
    }
}

fn remove_collinear_vertices(polygon: &mut Vec<Vec3>) {
    loop {
        let removable = (0..polygon.len()).find(|index| {
            let previous = polygon[(*index + polygon.len() - 1) % polygon.len()];
            let current = polygon[*index];
            let next = polygon[(*index + 1) % polygon.len()];
            (current - previous).cross(next - current).length_squared()
                <= ROOF_VERTEX_TOLERANCE_SQUARED
        });
        if polygon.len() <= 3 || removable.is_none() {
            break;
        }
        polygon.remove(removable.expect("removable index was checked"));
    }
}

fn boundary_notched_polygon(outer: &[Vec3], cutout: &[Vec3]) -> Option<Vec<Vec3>> {
    let on_segment = |point: Vec3, start: Vec3, end: Vec3| {
        let delta = end - start;
        let t = ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
        point.distance_squared(start + delta * t) <= ROOF_VERTEX_TOLERANCE_SQUARED
    };
    for edge_index in 0..outer.len() {
        let start = outer[edge_index];
        let end = outer[(edge_index + 1) % outer.len()];
        let delta = end - start;
        let mut touches = cutout
            .iter()
            .enumerate()
            .filter(|(_, point)| on_segment(**point, start, end))
            .map(|(index, point)| {
                (
                    index,
                    ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0),
                )
            })
            .collect::<Vec<_>>();
        if touches.len() != 2 {
            continue;
        }
        touches.sort_by(|left, right| left.1.total_cmp(&right.1));
        let (first, second) = (touches[0].0, touches[1].0);
        let forward_steps = (second + cutout.len() - first) % cutout.len();
        let step: isize = if forward_steps > 1 { 1 } else { -1 };
        let mut path = Vec::new();
        let mut current = first;
        loop {
            path.push(cutout[current]);
            if current == second {
                break;
            }
            current = (current as isize + step).rem_euclid(cutout.len() as isize) as usize;
        }
        let mut polygon = Vec::with_capacity(outer.len() + path.len());
        for (index, point) in outer.iter().copied().enumerate() {
            polygon.push(point);
            if index == edge_index {
                polygon.extend(path.iter().copied().filter(|candidate| {
                    !same_point(*candidate, start) && !same_point(*candidate, end)
                }));
            }
        }
        return Some(polygon);
    }

    let removed = outer
        .iter()
        .position(|outer_point| cutout.iter().any(|cut| same_point(*cut, *outer_point)))?;
    let previous = outer[(removed + outer.len() - 1) % outer.len()];
    let removed_point = outer[removed];
    let next = outer[(removed + 1) % outer.len()];
    let previous_touch = cutout.iter().copied().find(|point| {
        !same_point(*point, removed_point) && on_segment(*point, previous, removed_point)
    })?;
    let next_touch = cutout.iter().copied().find(|point| {
        !same_point(*point, removed_point) && on_segment(*point, removed_point, next)
    })?;
    let interior = cutout.iter().copied().find(|point| {
        !same_point(*point, removed_point)
            && !same_point(*point, previous_touch)
            && !same_point(*point, next_touch)
    })?;
    let mut polygon = Vec::with_capacity(outer.len() + 2);
    for (index, point) in outer.iter().copied().enumerate() {
        if index == removed {
            polygon.extend([previous_touch, interior, next_touch]);
        } else {
            polygon.push(point);
        }
    }
    Some(polygon)
}

fn same_point(left: Vec3, right: Vec3) -> bool {
    left.distance_squared(right) <= ROOF_VERTEX_TOLERANCE_SQUARED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResolvedItemId, RoofMaterial, RoofPlaneEquation, audit_triangle_mesh};

    #[test]
    fn roof_prism_honours_cutout_area_and_is_closed_and_consistently_wound() {
        let face = RoofFace {
            id: ResolvedItemId(1),
            polygon: vec![
                Vec3::new(-2.0, 0.0, -2.0),
                Vec3::new(2.0, 0.0, -2.0),
                Vec3::new(2.0, 0.0, 2.0),
                Vec3::new(-2.0, 0.0, 2.0),
            ],
            cutouts: vec![vec![
                Vec3::new(-1.0, 0.0, -1.0),
                Vec3::new(-1.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, -1.0),
            ]],
            plane: RoofPlaneEquation {
                normal: Vec3::Y,
                constant: 0.0,
            },
            pitch_degrees: 0.0,
            thickness_metres: 0.2,
            material: RoofMaterial::ClayTile,
            support_nodes: Vec::new(),
            drainage_catchment: ResolvedItemId(2),
        };

        let triangles = tessellate_roof_face(&face);
        assert!(
            triangles
                .iter()
                .any(|triangle| triangle.surface == RoofSurface::Weather)
        );
        assert!(
            triangles
                .iter()
                .any(|triangle| triangle.surface == RoofSurface::Interior)
        );
        assert!(
            triangles
                .iter()
                .any(|triangle| triangle.surface == RoofSurface::Boundary)
        );
        assert!(triangles.iter().all(|triangle| {
            match triangle.surface {
                RoofSurface::Weather => triangle.normal.dot(Vec3::Y) > 0.99,
                RoofSurface::Interior => triangle.normal.dot(-Vec3::Y) > 0.99,
                RoofSurface::Boundary => triangle.normal.y.abs() < 0.01,
                RoofSurface::Enclosure => false,
            }
        }));
        let top_area = triangles
            .iter()
            .filter(|triangle| triangle.normal.dot(Vec3::Y) > 0.99)
            .map(|triangle| {
                (triangle.positions[1] - triangle.positions[0])
                    .cross(triangle.positions[2] - triangle.positions[0])
                    .length()
                    * 0.5
            })
            .sum::<f32>();
        assert!((top_area - 12.0).abs() < 0.0001);
        assert!(triangles.iter().all(|triangle| {
            (triangle.positions[1] - triangle.positions[0])
                .cross(triangle.positions[2] - triangle.positions[0])
                .dot(triangle.normal)
                > 0.0
        }));

        let positions = triangles
            .iter()
            .flat_map(|triangle| triangle.positions)
            .map(|position| position.to_array())
            .collect::<Vec<_>>();
        let indices = (0..positions.len() as u32).collect::<Vec<_>>();
        let report = audit_triangle_mesh(&positions, &indices);
        assert!(report.passes_closed_solid(), "{report:?}");
    }
}
