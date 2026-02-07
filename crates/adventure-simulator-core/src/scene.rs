use core::f32;

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use serde::{Deserialize, Serialize};

/// Id of the scene in which the game takes place.
#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct SceneId(pub String);

#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq)]
pub struct SceneTerrain {
    heightmap: Vec<f32>,
    width: usize,
    scale: f32,
}

impl SceneTerrain {
    pub fn new(width: usize, depth: usize, scale: f32, op: impl Fn(Vec2) -> f32) -> Self {
        // number of points = segments + 1
        let width = width + 1;
        let depth = depth + 1;

        let heightmap = (0..=(width * depth))
            .map(move |i| {
                let coords = Vec2::new((i % width) as f32, (i / depth) as f32);
                op(coords)
            })
            .collect();

        Self {
            width,
            heightmap,
            scale,
        }
    }

    pub fn grid_width(&self) -> usize {
        self.width
    }

    pub fn grid_depth(&self) -> usize {
        self.heightmap.len() / self.width
    }

    pub fn width(&self) -> f32 {
        self.grid_width() as f32 * self.scale
    }

    pub fn depth(&self) -> f32 {
        self.grid_depth() as f32 * self.scale
    }

    pub fn grid_scale(&self) -> f32 {
        self.scale
    }

    pub fn height_at(&self, pos: Vec2) -> Option<f32> {
        // offset pos because floor's origin is at the center
        let pos = pos + Vec2::new(self.width(), self.depth()) * 0.5;

        let min_x = (pos.x / self.scale) as usize;
        let min_y = (pos.y / self.scale) as usize;

        let x0y0 = *self.heightmap.get(min_x + min_y * self.width)?;
        let x1y0 = *self.heightmap.get(min_x + 1 + min_y * self.width)?;
        let x0y1 = *self.heightmap.get(min_x + (min_y + 1) * self.width)?;
        let x1y1 = *self.heightmap.get(min_x + 1 + (min_y + 1) * self.width)?;

        let y0 = x0y0.lerp(x1y0, pos.x % self.scale);
        let y1 = x0y1.lerp(x1y1, pos.x % self.scale);

        let height = y0.lerp(y1, pos.y % self.scale);
        Some(height)
    }

    pub fn collider(&self) -> Collider {
        let mesh = self.mesh();

        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            unreachable!("mesh should always have position")
        };

        let indices = mesh.indices().unwrap();
        let indices = indices
            .iter()
            .map(|index| index as u32)
            .array_chunks()
            .collect();

        let vertices = positions.iter().copied().map(Vec3::from_array).collect();

        Collider::trimesh(vertices, indices)
    }

    pub fn mesh(&self) -> Mesh {
        let mut positions = Vec::with_capacity(self.heightmap.len());
        let mut normals = Vec::with_capacity(self.heightmap.len());
        let mut uvs = Vec::with_capacity(self.heightmap.len());
        for x in 0..self.grid_width() {
            for z in 0..self.grid_depth() {
                let i = z * self.grid_width() + x;
                let y = self.heightmap[i];

                positions.push([x as f32, y, z as f32]);

                normals.push([0.0, 1.0, 0.0]); // placeholder
                uvs.push([
                    x as f32 / (self.grid_width() - 1) as f32,
                    z as f32 / (self.grid_depth() - 1) as f32,
                ]);
            }
        }

        let mut indices = Vec::with_capacity((self.grid_width() - 1) * (self.grid_depth() - 1) * 6);
        for x in 0..self.grid_width() - 1 {
            for z in 0..self.grid_depth() - 1 {
                let i = (z * self.grid_width() + x) as u32;

                indices.extend_from_slice(&[
                    i,
                    i + 1,
                    i + self.grid_width() as u32 + 1,
                    i,
                    i + self.grid_width() as u32 + 1,
                    i + self.grid_width() as u32,
                ]);
            }
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));

        mesh.translated_by(
            Vec3::new(self.grid_width() as f32, 0.0, self.grid_depth() as f32) * -0.5,
        )
        .scaled_by(Vec3::new(self.scale, 1.0, self.scale))
        .with_computed_area_weighted_normals()
    }
}
