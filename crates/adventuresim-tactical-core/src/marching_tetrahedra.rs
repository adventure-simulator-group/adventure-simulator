use std::collections::HashMap;

use bevy::prelude::Vec3;
use rayon::prelude::*;

use crate::{terrain_transition::TerrainTransitionCollar, volumetric_terrain::SceneTerrainPatch};

// Fields are in metres. Snap sub-millimetre roundoff to the shared lattice
// endpoint instead of producing several almost-coincident edge vertices.
const FIELD_ZERO_TOLERANCE_METRES: f32 = 0.00001;

const CUBE: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];
const TETS: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 3, 2, 7],
    [0, 2, 6, 7],
    [0, 6, 4, 7],
    [0, 4, 5, 7],
    [0, 5, 1, 7],
];

pub(crate) fn marching_tetrahedra(
    dimensions: [usize; 3],
    position_at: impl Fn([usize; 3]) -> Vec3 + Sync,
    field: impl Fn(Vec3) -> f32 + Sync,
    transition_collar: TerrainTransitionCollar,
) -> Result<SceneTerrainPatch, &'static str> {
    let (positions, values) = sample_field(dimensions, &position_at, &field)?;
    let (mesh_positions, indices) = extract_surface(dimensions, &positions, &values)?;
    build_surface(mesh_positions, indices, transition_collar)
}

fn sample_field(
    dimensions: [usize; 3],
    position_at: &(impl Fn([usize; 3]) -> Vec3 + Sync),
    field: &(impl Fn(Vec3) -> f32 + Sync),
) -> Result<(Vec<Vec3>, Vec<f32>), &'static str> {
    let count = dimensions[0]
        .checked_mul(dimensions[1])
        .and_then(|n| n.checked_mul(dimensions[2]))
        .ok_or("fault patch sample count overflow")?;
    let samples = (0..count)
        .into_par_iter()
        .map(|index| {
            let yz = index / dimensions[0];
            let position = position_at([
                index % dimensions[0],
                yz % dimensions[1],
                yz / dimensions[1],
            ]);
            let value = field(position);
            let value = if value.abs() <= FIELD_ZERO_TOLERANCE_METRES {
                0.0
            } else {
                value
            };
            (position.is_finite() && value.is_finite())
                .then_some((position, value))
                .ok_or("fault patch field is not finite")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(samples.into_iter().unzip())
}

fn extract_surface(
    dimensions: [usize; 3],
    positions: &[Vec3],
    values: &[f32],
) -> Result<(Vec<Vec3>, Vec<u32>), &'static str> {
    let sample_id = |x: usize, y: usize, z: usize| (z * dimensions[1] + y) * dimensions[0] + x;
    let mut mesh_positions = Vec::new();
    let mut indices = Vec::new();
    let mut edge_vertices = HashMap::new();
    for z in 0..dimensions[2] - 1 {
        for y in 0..dimensions[1] - 1 {
            for x in 0..dimensions[0] - 1 {
                let cube =
                    CUBE.map(|offset| sample_id(x + offset[0], y + offset[1], z + offset[2]));
                for tet in TETS {
                    emit_tetrahedron(
                        tet.map(|corner| cube[corner]),
                        positions,
                        values,
                        &mut edge_vertices,
                        &mut mesh_positions,
                        &mut indices,
                    )?;
                }
            }
        }
    }
    Ok((mesh_positions, indices))
}

fn emit_tetrahedron(
    vertices: [usize; 4],
    positions: &[Vec3],
    values: &[f32],
    edge_vertices: &mut HashMap<(usize, usize), u32>,
    mesh_positions: &mut Vec<Vec3>,
    indices: &mut Vec<u32>,
) -> Result<(), &'static str> {
    let inside = vertices
        .iter()
        .copied()
        .filter(|&index| values[index] <= 0.0)
        .collect::<Vec<_>>();
    let outside = vertices
        .iter()
        .copied()
        .filter(|&index| values[index] > 0.0)
        .collect::<Vec<_>>();
    let first_triangle = indices.len();
    let mut vertex = |a, b| vertex_on_edge(a, b, positions, values, edge_vertices, mesh_positions);
    match (inside.len(), outside.len()) {
        (1, 3) => {
            for &b in &outside {
                indices.push(vertex(inside[0], b)?);
            }
        }
        (3, 1) => {
            let triangle = inside
                .iter()
                .map(|&a| vertex(a, outside[0]))
                .collect::<Result<Vec<_>, _>>()?;
            indices.extend([triangle[0], triangle[2], triangle[1]]);
        }
        (2, 2) => {
            let ac = vertex(inside[0], outside[0])?;
            let ad = vertex(inside[0], outside[1])?;
            let bc = vertex(inside[1], outside[0])?;
            let bd = vertex(inside[1], outside[1])?;
            indices.extend([ac, bc, ad, ad, bc, bd]);
        }
        _ => {}
    }
    if let (Some(&solid), Some(&empty)) = (inside.first(), outside.first()) {
        // The extracted surface is linear within this tetrahedron. Its normal
        // must face from the solid sample toward the empty sample. A gradient
        // of the original nonlinear field at a triangle centre can face the
        // other way at a cusp, breaking winding across shared edges.
        let outward = positions[empty].as_dvec3() - positions[solid].as_dvec3();
        for triangle in indices[first_triangle..].as_chunks_mut::<3>().0 {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                .map(|index| mesh_positions[index as usize].as_dvec3());
            if (b - a).cross(c - a).dot(outward) < 0.0 {
                triangle.swap(1, 2);
            }
        }
    }
    Ok(())
}

fn vertex_on_edge(
    a: usize,
    b: usize,
    positions: &[Vec3],
    values: &[f32],
    edge_vertices: &mut HashMap<(usize, usize), u32>,
    mesh_positions: &mut Vec<Vec3>,
) -> Result<u32, &'static str> {
    // Several tetrahedra can reach the same zero-valued lattice vertex along
    // different edges. Give that endpoint one identity to preserve topology.
    let key = if values[a] == 0.0 {
        (a, a)
    } else if values[b] == 0.0 {
        (b, b)
    } else if a < b {
        (a, b)
    } else {
        (b, a)
    };
    if let Some(&index) = edge_vertices.get(&key) {
        return Ok(index);
    }
    let denominator = values[a] - values[b];
    let fraction = if denominator.abs() < 1e-7 {
        0.5
    } else {
        values[a] / denominator
    }
    .clamp(0.0, 1.0);
    let index =
        u32::try_from(mesh_positions.len()).map_err(|_| "fault patch has too many vertices")?;
    mesh_positions.push(positions[a].lerp(positions[b], fraction));
    edge_vertices.insert(key, index);
    Ok(index)
}

fn build_surface(
    mesh_positions: Vec<Vec3>,
    indices: Vec<u32>,
    transition_collar: TerrainTransitionCollar,
) -> Result<SceneTerrainPatch, &'static str> {
    let oriented = indices
        .par_chunks_exact(3)
        .map(|source| triangle_normal(source, &mesh_positions))
        .collect::<Vec<_>>();
    let mut normals = vec![Vec3::ZERO; mesh_positions.len()];
    let mut oriented_indices = Vec::with_capacity(indices.len());
    for (triangle, normal) in oriented.into_iter().flatten() {
        for &index in &triangle {
            normals[index as usize] += normal;
        }
        oriented_indices.extend(triangle);
    }
    if oriented_indices.is_empty() {
        return Err("fault patch extraction produced no surface");
    }
    Ok(SceneTerrainPatch {
        transition_collar,
        positions: mesh_positions
            .into_iter()
            .map(|position| position.to_array())
            .collect(),
        normals: normals
            .into_iter()
            .map(|normal| normal.normalize_or_zero().to_array())
            .collect(),
        indices: oriented_indices,
    })
}

fn triangle_normal(source: &[u32], positions: &[Vec3]) -> Option<([u32; 3], Vec3)> {
    let triangle = [source[0], source[1], source[2]];
    let [a, b, c] = triangle.map(|index| positions[index as usize]);
    let normal = (b - a).cross(c - a);
    // Tiny triangles close a valid surface near lattice corners. Dropping
    // them on an arbitrary area threshold opens cracks around carved roofs.
    if normal.length_squared() == 0.0 {
        return None;
    }
    Some((triangle, normal))
}
