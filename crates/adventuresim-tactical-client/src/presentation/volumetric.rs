//! Client-only extraction of render surfaces from bounded scalar fields.
//!
//! The network and tactical server replicate compact recipes. They do not
//! depend on this module, sample these fields, or transmit extracted meshes.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct SurfaceNetsGrid {
    pub sample_counts: [usize; 3],
    pub minimum: Vec3,
    pub maximum: Vec3,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct ExtractedSurface {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl ExtractedSurface {
    pub fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

pub(super) fn extract_surface_nets(
    grid: SurfaceNetsGrid,
    field: impl Fn(Vec3) -> f32,
) -> Option<ExtractedSurface> {
    let [nx, ny, nz] = grid.sample_counts;
    if nx < 2
        || ny < 2
        || nz < 2
        || !grid.minimum.is_finite()
        || !grid.maximum.is_finite()
        || !grid.minimum.cmplt(grid.maximum).all()
    {
        return None;
    }
    let sample_count = nx.checked_mul(ny)?.checked_mul(nz)?;
    let cell_counts = [nx - 1, ny - 1, nz - 1];
    let cell_count = cell_counts[0]
        .checked_mul(cell_counts[1])?
        .checked_mul(cell_counts[2])?;
    let spacing = (grid.maximum - grid.minimum)
        / Vec3::new((nx - 1) as f32, (ny - 1) as f32, (nz - 1) as f32);
    let sample_position = |x: usize, y: usize, z: usize| {
        grid.minimum + Vec3::new(x as f32, y as f32, z as f32) * spacing
    };
    let sample_index = |x: usize, y: usize, z: usize| (z * ny + y) * nx + x;
    let cell_index = |x: usize, y: usize, z: usize| (z * cell_counts[1] + y) * cell_counts[0] + x;
    let mut samples = Vec::with_capacity(sample_count);
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let value = field(sample_position(x, y, z));
                if !value.is_finite() {
                    return None;
                }
                samples.push(value);
            }
        }
    }

    const CORNERS: [[usize; 3]; 8] = [
        [0, 0, 0],
        [1, 0, 0],
        [0, 1, 0],
        [1, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [0, 1, 1],
        [1, 1, 1],
    ];
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut vertices = vec![None; cell_count];
    let mut surface = ExtractedSurface::default();
    let normal_step = spacing.min_element() * 0.35;
    for z in 0..cell_counts[2] {
        for y in 0..cell_counts[1] {
            for x in 0..cell_counts[0] {
                let corner_values =
                    CORNERS.map(|[dx, dy, dz]| samples[sample_index(x + dx, y + dy, z + dz)]);
                let inside = corner_values.map(|value| value <= 0.0);
                if inside.iter().all(|value| *value) || inside.iter().all(|value| !*value) {
                    continue;
                }
                let mut point = Vec3::ZERO;
                let mut intersections = 0_u32;
                for (start, end) in EDGES {
                    if inside[start] == inside[end] {
                        continue;
                    }
                    let [sx, sy, sz] = CORNERS[start];
                    let [ex, ey, ez] = CORNERS[end];
                    let start_position = sample_position(x + sx, y + sy, z + sz);
                    let end_position = sample_position(x + ex, y + ey, z + ez);
                    let start_value = corner_values[start];
                    let end_value = corner_values[end];
                    let amount = (start_value / (start_value - end_value)).clamp(0.0, 1.0);
                    point += start_position.lerp(end_position, amount);
                    intersections += 1;
                }
                if intersections == 0 {
                    continue;
                }
                point /= intersections as f32;
                let step = Vec3::splat(normal_step);
                let gradient = Vec3::new(
                    field(point + Vec3::X * step.x) - field(point - Vec3::X * step.x),
                    field(point + Vec3::Y * step.y) - field(point - Vec3::Y * step.y),
                    field(point + Vec3::Z * step.z) - field(point - Vec3::Z * step.z),
                );
                let normal = gradient.try_normalize().unwrap_or(Vec3::Y);
                let index = surface.positions.len() as u32;
                surface.positions.push(point.to_array());
                surface.normals.push(normal.to_array());
                vertices[cell_index(x, y, z)] = Some(index);
            }
        }
    }

    let mut add_quad = |cells: [[usize; 3]; 4]| {
        let [Some(a), Some(b), Some(c), Some(d)] =
            cells.map(|[x, y, z]| vertices[cell_index(x, y, z)])
        else {
            return;
        };
        let indices = [a, b, c, d];
        let positions = indices.map(|index| Vec3::from_array(surface.positions[index as usize]));
        let face_normal = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
        let expected_normal = indices
            .iter()
            .map(|index| Vec3::from_array(surface.normals[*index as usize]))
            .sum::<Vec3>();
        if face_normal.dot(expected_normal) >= 0.0 {
            surface.indices.extend_from_slice(&[
                indices[0], indices[1], indices[2], indices[0], indices[2], indices[3],
            ]);
        } else {
            surface.indices.extend_from_slice(&[
                indices[0], indices[3], indices[2], indices[0], indices[2], indices[1],
            ]);
        }
    };
    let changes_sign = |left: f32, right: f32| (left <= 0.0) != (right <= 0.0);
    for z in 1..nz - 1 {
        for y in 1..ny - 1 {
            for x in 0..nx - 1 {
                if changes_sign(
                    samples[sample_index(x, y, z)],
                    samples[sample_index(x + 1, y, z)],
                ) {
                    add_quad([[x, y - 1, z - 1], [x, y, z - 1], [x, y, z], [x, y - 1, z]]);
                }
            }
        }
    }
    for z in 1..nz - 1 {
        for y in 0..ny - 1 {
            for x in 1..nx - 1 {
                if changes_sign(
                    samples[sample_index(x, y, z)],
                    samples[sample_index(x, y + 1, z)],
                ) {
                    add_quad([[x - 1, y, z - 1], [x - 1, y, z], [x, y, z], [x, y, z - 1]]);
                }
            }
        }
    }
    for z in 0..nz - 1 {
        for y in 1..ny - 1 {
            for x in 1..nx - 1 {
                if changes_sign(
                    samples[sample_index(x, y, z)],
                    samples[sample_index(x, y, z + 1)],
                ) {
                    add_quad([[x - 1, y - 1, z], [x, y - 1, z], [x, y, z], [x - 1, y, z]]);
                }
            }
        }
    }
    Some(surface)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn sphere() -> ExtractedSurface {
        extract_surface_nets(
            SurfaceNetsGrid {
                sample_counts: [18; 3],
                minimum: Vec3::splat(-1.0),
                maximum: Vec3::splat(1.0),
            },
            |point| point.length() - 0.75,
        )
        .unwrap()
    }

    #[test]
    fn extracts_a_deterministic_outward_closed_surface() {
        let first = sphere();
        let second = sphere();
        assert_eq!(first, second);
        assert!(first.positions.len() > 500);
        assert!(first.indices.len() > 3_000);
        for (position, normal) in first.positions.iter().zip(&first.normals) {
            let position = Vec3::from_array(*position);
            let normal = Vec3::from_array(*normal);
            assert!(position.length() <= 0.76);
            assert!(normal.is_normalized());
            assert!(position.dot(normal) > 0.0);
        }
        let mut edge_uses = BTreeMap::<(u32, u32), usize>::new();
        for triangle in first.indices.as_chunks::<3>().0 {
            for [left, right] in [
                [triangle[0], triangle[1]],
                [triangle[1], triangle[2]],
                [triangle[2], triangle[0]],
            ] {
                *edge_uses
                    .entry((left.min(right), left.max(right)))
                    .or_default() += 1;
            }
        }
        assert!(edge_uses.values().all(|uses| *uses == 2));
    }

    #[test]
    fn rejects_invalid_grids_and_non_finite_fields() {
        assert!(
            extract_surface_nets(
                SurfaceNetsGrid {
                    sample_counts: [1, 2, 2],
                    minimum: Vec3::ZERO,
                    maximum: Vec3::ONE,
                },
                |_| 1.0,
            )
            .is_none()
        );
        assert!(
            extract_surface_nets(
                SurfaceNetsGrid {
                    sample_counts: [2; 3],
                    minimum: Vec3::ZERO,
                    maximum: Vec3::ONE,
                },
                |_| f32::NAN,
            )
            .is_none()
        );
    }
}
