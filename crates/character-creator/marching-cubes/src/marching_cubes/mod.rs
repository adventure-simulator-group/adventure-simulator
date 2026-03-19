mod tables;

use distance_field::{DistanceField, BoneIndexField, BoneWeightField};
use tables::{EDGE_TABLE, TRI_TABLE, VERTEX_OFFSETS, EDGE_CONNECTIONS};
use bevy_math::Vec3;

use crate::MeshBuilder;

pub struct MarchingCubes;

impl MarchingCubes {
    pub fn generate_mesh(
        distance_field: &DistanceField,
        bone_index_field: &BoneIndexField,
        bone_weight_field: &BoneWeightField,
        iso_level: f32,
        voxel_size: f32,
    ) -> MeshBuilder {
        let (width, height, depth) = distance_field.dimensions();

        let mut builder = MeshBuilder::new();

        if width < 2 || height < 2 || depth < 2 {
            return builder;
        }

        let origin = Self::grid_origin(width, height, depth, voxel_size);

        for z in 0..(depth - 1) {
            for y in 0..(height - 1) {
                for x in 0..(width - 1) {
                    let (cube_index, samples, corners, corner_indices, corner_weights) =
                        Self::get_cell_data(distance_field, bone_index_field, bone_weight_field, origin, x, y, z, voxel_size, iso_level);

                    let edge_mask = EDGE_TABLE[cube_index];
                    if edge_mask == 0 {
                        continue;
                    }

                    let (edge_vertices, edge_indices, edge_weights) =
                        Self::calculate_intersections(edge_mask, iso_level, samples, corners, corner_indices, corner_weights);

                    Self::process_triangles(cube_index, &edge_vertices, &edge_indices, &edge_weights, &mut builder);
                }
            }
        }

        builder
    }

    fn get_cell_data(
        distance_field: &DistanceField,
        bone_index_field: &BoneIndexField,
        bone_weight_field: &BoneWeightField,
        origin: Vec3,
        x: usize,
        y: usize,
        z: usize,
        voxel_size: f32,
        iso_level: f32,
    ) -> (usize, [f32; 8], [Vec3; 8], [[u16; 4]; 8], [[f32; 4]; 8]) {
        let mut cube_index = 0usize;
        let mut samples = [0.0f32; 8];
        let mut corners = [Vec3::ZERO; 8];
        let mut corner_indices = [[0u16; 4]; 8];
        let mut corner_weights = [[0.0f32; 4]; 8];

        for (i, &(dx, dy, dz)) in VERTEX_OFFSETS.iter().enumerate() {
            let sx = x + dx;
            let sy = y + dy;
            let sz = z + dz;

            let d = *distance_field.get(sx, sy, sz);
            samples[i] = d;
            corners[i] = Self::sample_to_world(origin, sx, sy, sz, voxel_size);
            
            let idices_u8 = bone_index_field.get(sx, sy, sz);
            corner_indices[i] = [idices_u8[0] as u16, idices_u8[1] as u16, idices_u8[2] as u16, idices_u8[3] as u16];
            corner_weights[i] = *bone_weight_field.get(sx, sy, sz);

            if d < iso_level {
                cube_index |= 1 << i;
            }
        }

        (cube_index, samples, corners, corner_indices, corner_weights)
    }

    fn calculate_intersections(
        edge_mask: i32,
        iso_level: f32,
        samples: [f32; 8],
        corners: [Vec3; 8],
        corner_indices: [[u16; 4]; 8],
        corner_weights: [[f32; 4]; 8],
    ) -> ([Vec3; 12], [[u16; 4]; 12], [[f32; 4]; 12]) {
        let mut edge_vertices = [Vec3::ZERO; 12];
        let mut edge_indices = [[0u16; 4]; 12];
        let mut edge_weights = [[0.0f32; 4]; 12];

        for edge in 0..12 {
            if (edge_mask & (1 << edge)) != 0 {
                let (v1, v2) = EDGE_CONNECTIONS[edge];
                let (pos, t) = Self::vertex_interpolate(
                    iso_level,
                    corners[v1],
                    corners[v2],
                    samples[v1],
                    samples[v2],
                );
                
                edge_vertices[edge] = pos;
                
                // For indices, pick the one closer (based on t) or simply max weight. Let's just linearly interpolate the weights 
                // and pick nearest neighbor for index (t < 0.5 ? v1 : v2) to avoid blending indices.
                if t < 0.5 {
                    edge_indices[edge] = corner_indices[v1];
                } else {
                    edge_indices[edge] = corner_indices[v2];
                }

                edge_weights[edge] = [
                    corner_weights[v1][0] * (1.0 - t) + corner_weights[v2][0] * t,
                    corner_weights[v1][1] * (1.0 - t) + corner_weights[v2][1] * t,
                    corner_weights[v1][2] * (1.0 - t) + corner_weights[v2][2] * t,
                    corner_weights[v1][3] * (1.0 - t) + corner_weights[v2][3] * t,
                ];
            }
        }
        (edge_vertices, edge_indices, edge_weights)
    }

    fn process_triangles(
        cube_index: usize, 
        edge_vertices: &[Vec3; 12], 
        edge_indices: &[[u16; 4]; 12],
        edge_weights: &[[f32; 4]; 12],
        builder: &mut MeshBuilder
    ) {
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
                edge_indices[idx0],
                edge_weights[idx0],
                edge_indices[idx1],
                edge_weights[idx1],
                edge_indices[idx2],
                edge_weights[idx2],
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

    fn vertex_interpolate(iso_level: f32, p1: Vec3, p2: Vec3, v1: f32, v2: f32) -> (Vec3, f32) {
        const EPSILON: f32 = 1.0e-6;

        if (iso_level - v1).abs() < EPSILON {
            return (p1, 0.0);
        }
        if (iso_level - v2).abs() < EPSILON {
            return (p2, 1.0);
        }
        if (v1 - v2).abs() < EPSILON {
            return (p1, 0.0);
        }

        let t = (iso_level - v1) / (v2 - v1);
        (p1 + (p2 - p1) * t, t)
    }
}

