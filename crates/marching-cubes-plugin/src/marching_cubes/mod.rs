mod tables;

use tables::{EDGE_TABLE, TRI_TABLE, VERTEX_OFFSETS, EDGE_CONNECTIONS};
use bevy::{
    asset::RenderAssetUsages,
    math::Vec3,
    mesh::{Mesh, PrimitiveTopology},
};

use super::{DistanceField, MeshBuilder};

pub struct MarchingCubes;

impl MarchingCubes {
    pub fn generate_mesh(distance_field: &DistanceField, iso_level: f32, voxel_size: f32) -> Mesh {
        let (width, height, depth) = distance_field.dimensions();

        if width < 2 || height < 2 || depth < 2 {
            return Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
        }

        let origin = Self::grid_origin(width, height, depth, voxel_size);
        let mut builder = MeshBuilder::new();

        for z in 0..(depth - 1) {
            for y in 0..(height - 1) {
                for x in 0..(width - 1) {
                    let (cube_index, samples, corners) =
                        Self::get_cell_data(distance_field, origin, x, y, z, voxel_size, iso_level);

                    let edge_mask = EDGE_TABLE[cube_index];
                    if edge_mask == 0 {
                        continue;
                    }

                    let edge_vertices =
                        Self::calculate_intersections(edge_mask, iso_level, samples, corners);

                    Self::process_triangles(cube_index, &edge_vertices, &mut builder);
                }
            }
        }

        builder.build()
    }

    fn get_cell_data(
        distance_field: &DistanceField,
        origin: Vec3,
        x: usize,
        y: usize,
        z: usize,
        voxel_size: f32,
        iso_level: f32,
    ) -> (usize, [f32; 8], [Vec3; 8]) {
        let mut cube_index = 0usize;
        let mut samples = [0.0f32; 8];
        let mut corners = [Vec3::ZERO; 8];

        for (i, &(dx, dy, dz)) in VERTEX_OFFSETS.iter().enumerate() {
            let sx = x + dx;
            let sy = y + dy;
            let sz = z + dz;

            let d = *distance_field.get(sx, sy, sz);
            samples[i] = d;
            corners[i] = Self::sample_to_world(origin, sx, sy, sz, voxel_size);

            if d < iso_level {
                cube_index |= 1 << i;
            }
        }

        (cube_index, samples, corners)
    }

    fn calculate_intersections(
        edge_mask: i32,
        iso_level: f32,
        samples: [f32; 8],
        corners: [Vec3; 8],
    ) -> [Vec3; 12] {
        let mut edge_vertices = [Vec3::ZERO; 12];
        for edge in 0..12 {
            if (edge_mask & (1 << edge)) != 0 {
                let (v1, v2) = EDGE_CONNECTIONS[edge];
                edge_vertices[edge] = Self::vertex_interpolate(
                    iso_level,
                    corners[v1],
                    corners[v2],
                    samples[v1],
                    samples[v2],
                );
            }
        }
        edge_vertices
    }

    fn process_triangles(cube_index: usize, edge_vertices: &[Vec3; 12], builder: &mut MeshBuilder) {
        let mut tri_index = 0usize;
        while tri_index + 2 < TRI_TABLE[cube_index].len() && TRI_TABLE[cube_index][tri_index] != -1
        {
            let idx0 = TRI_TABLE[cube_index][tri_index] as usize;
            let idx1 = TRI_TABLE[cube_index][tri_index + 1] as usize;
            let idx2 = TRI_TABLE[cube_index][tri_index + 2] as usize;

            builder.add_triangle(
                edge_vertices[idx0],
                edge_vertices[idx1],
                edge_vertices[idx2],
            );

            tri_index += 3;
        }
    }

    pub fn grid_origin(width: usize, height: usize, depth: usize, voxel_size: f32) -> Vec3 {
        Vec3::new(
            -(width as f32 - 1.0) * 0.5 * voxel_size,
            -(height as f32 - 1.0) * 0.5 * voxel_size,
            -(depth as f32 - 1.0) * 0.5 * voxel_size,
        )
    }

    pub fn sample_to_world(origin: Vec3, x: usize, y: usize, z: usize, voxel_size: f32) -> Vec3 {
        origin
            + Vec3::new(
                x as f32 * voxel_size,
                y as f32 * voxel_size,
                z as f32 * voxel_size,
            )
    }

    fn vertex_interpolate(iso_level: f32, p1: Vec3, p2: Vec3, v1: f32, v2: f32) -> Vec3 {
        const EPSILON: f32 = 1.0e-6;

        if (iso_level - v1).abs() < EPSILON {
            return p1;
        }
        if (iso_level - v2).abs() < EPSILON {
            return p2;
        }
        if (v1 - v2).abs() < EPSILON {
            return p1;
        }

        let t = (iso_level - v1) / (v2 - v1);
        p1 + (p2 - p1) * t
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::Indices;

    use super::*;

    #[test]
    fn test_marching_cubes_mesh_generation() {
        let size = 10;
        let voxel_size = 1.0;
        let radius = 2.5;

        let mut distance_field = DistanceField::new(size, size, size);
        distance_field.add_sphere(Vec3::ZERO, radius, voxel_size);

        let mesh = MarchingCubes::generate_mesh(&distance_field, 0.0, voxel_size);

        assert!(matches!(
            mesh.primitive_topology(),
            PrimitiveTopology::TriangleList
        ));

        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        // A sphere should generate some vertices
        assert!(positions.len() > 0);

        // Check if index count is divisible by 3 (triangles)
        if let Some(Indices::U32(indices)) = mesh.indices() {
            assert!(indices.len() > 0);
            assert_eq!(indices.len() % 3, 0);
        } else {
            panic!("Mesh should have U32 indices");
        }
    }
}
