use core::f32;

use avian3d::prelude::*;
use bevy::platform::hash::RandomState;
use bevy::prelude::*;
#[cfg(feature = "meshgen")]
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
};
use noiz::prelude::*;
use serde::{Deserialize, Serialize};
use std::hash::BuildHasher;

/// Terrain generator shared by the authoritative server and deterministic
/// presentation fixtures. Keeping the implementation here prevents animation
/// captures from approximating a different surface than gameplay uses.
#[derive(Debug, Clone)]
pub struct TerrainGenerator {
    pub seed: u32,
    pub period: f32,
    pub grid_scale: f32,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            period: 30.0,
            grid_scale: 1.0,
        }
    }

    pub fn from_hash(hash: impl std::hash::Hash) -> Self {
        Self::new(RandomState::default().hash_one(&hash) as u32)
    }

    pub fn generate(self, width: usize, height: usize, depth: usize) -> SceneTerrain {
        let mut noise = Noise::from(LayeredNoise::new(
            Normed::<f32>::default(),
            Persistence(0.5),
            FractalLayers {
                layer: Octave::<MixCellGradients<OrthoGrid, Smoothstep, QuickGradients>>::default(),
                lacunarity: 2.0,
                amount: 8,
            },
        ));
        noise.set_seed(self.seed);
        noise.set_period(self.period);

        SceneTerrain::new(width, depth, self.grid_scale, move |location| {
            let normal: f32 = noise.sample(location);
            normal * height as f32
        })
    }
}

/// Id of the scene in which the game takes place.
#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq, Eq)]
#[reflect(Component)]
#[component(immutable)]
pub struct SceneId(pub String);

#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq)]
#[reflect(Component)]
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

        let heightmap = (0..(width * depth))
            .map(move |i| {
                let coords = Vec2::new((i % width) as f32, (i / width) as f32);
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
        self.grid_width().saturating_sub(1) as f32 * self.scale
    }

    pub fn depth(&self) -> f32 {
        self.grid_depth().saturating_sub(1) as f32 * self.scale
    }

    pub fn grid_scale(&self) -> f32 {
        self.scale
    }

    pub fn height_at(&self, pos: Vec2) -> Option<f32> {
        self.surface_at(pos).map(|sample| sample.0)
    }

    /// Samples the same triangle surface used by the rendered mesh and
    /// authoritative collider. Returning the triangle normal alongside the
    /// height keeps terrain IK from fitting a foot to a different, bilinear
    /// surface than the one visible beneath it.
    fn surface_at(&self, pos: Vec2) -> Option<(f32, Vec3)> {
        if self.scale <= 0.0 || self.grid_width() < 2 || self.grid_depth() < 2 {
            return None;
        }
        // offset pos because floor's origin is at the center
        let pos = pos + Vec2::new(self.width(), self.depth()) * 0.5;
        let grid = pos / self.scale;
        if grid.x < 0.0
            || grid.y < 0.0
            || grid.x > (self.grid_width() - 1) as f32
            || grid.y > (self.grid_depth() - 1) as f32
        {
            return None;
        }
        // The final vertex has no cell to its right/below. Sample its adjacent
        // cell with a coordinate of one so edges remain well-defined.
        let min_x = (grid.x.floor() as usize).min(self.grid_width() - 2);
        let min_y = (grid.y.floor() as usize).min(self.grid_depth() - 2);
        let fraction = grid - Vec2::new(min_x as f32, min_y as f32);

        let x0y0 = *self.heightmap.get(min_x + min_y * self.width)?;
        let x1y0 = *self.heightmap.get(min_x + 1 + min_y * self.width)?;
        let x0y1 = *self.heightmap.get(min_x + (min_y + 1) * self.width)?;
        let x1y1 = *self.heightmap.get(min_x + 1 + (min_y + 1) * self.width)?;

        let (height, tangent_x, tangent_z) = if fraction.x >= fraction.y {
            // Matches [x0y0, x1y1, x1y0] in `mesh_components`.
            (
                x0y0 + (x1y0 - x0y0) * fraction.x + (x1y1 - x1y0) * fraction.y,
                Vec3::new(self.scale, x1y0 - x0y0, 0.0),
                Vec3::new(0.0, x1y1 - x1y0, self.scale),
            )
        } else {
            // Matches [x0y0, x0y1, x1y1] in `mesh_components`.
            (
                x0y0 + (x1y1 - x0y1) * fraction.x + (x0y1 - x0y0) * fraction.y,
                Vec3::new(self.scale, x1y1 - x0y1, 0.0),
                Vec3::new(0.0, x0y1 - x0y0, self.scale),
            )
        };
        let normal = tangent_z.cross(tangent_x).try_normalize()?;
        Some((height, normal))
    }

    /// Returns the finite, normalized normal of the rendered/collided triangle.
    pub fn normal_at(&self, pos: Vec2) -> Option<Vec3> {
        self.surface_at(pos).map(|sample| sample.1)
    }

    pub fn collider(&self) -> Collider {
        let (positions, indices, _) = self.mesh_components();
        let indices = indices.into_iter().array_chunks().collect();
        let vertices = positions.iter().copied().map(Vec3::from_array).collect();
        Collider::trimesh(vertices, indices)
    }

    #[cfg(feature = "meshgen")]
    pub fn mesh(&self) -> Mesh {
        let (positions, indices, uvs) = self.mesh_components();

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));

        mesh.with_computed_area_weighted_normals()
    }

    fn mesh_components(&self) -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 2]>) {
        let mesh_offset = Vec2::new(
            self.grid_width().saturating_sub(1) as f32,
            self.grid_depth().saturating_sub(1) as f32,
        ) * -0.5;

        let mut positions = Vec::with_capacity(self.heightmap.len());
        let mut uvs = Vec::with_capacity(self.heightmap.len());
        // Keep vertices in the same row-major (z * width + x) order used by
        // height sampling and the triangle indices below. Iterating x first
        // transposed the storage without transposing the indices, which made
        // otherwise smooth terrain render as long shredded triangles.
        for z in 0..self.grid_depth() {
            for x in 0..self.grid_width() {
                let i = z * self.grid_width() + x;
                let y = self.heightmap[i];

                uvs.push([
                    x as f32 / (self.grid_width() - 1) as f32,
                    z as f32 / (self.grid_depth() - 1) as f32,
                ]);

                let x = (x as f32 + mesh_offset.x) * self.scale;
                let z = (z as f32 + mesh_offset.y) * self.scale;
                positions.push([x, y, z]);
            }
        }

        let mut indices = Vec::with_capacity((self.grid_width() - 1) * (self.grid_depth() - 1) * 6);
        for x in 0..self.grid_width() - 1 {
            for z in 0..self.grid_depth() - 1 {
                let i = (z * self.grid_width() + x) as u32;

                indices.extend_from_slice(&[
                    i,
                    i + self.grid_width() as u32 + 1,
                    i + 1,
                    i,
                    i + self.grid_width() as u32,
                    i + self.grid_width() as u32 + 1,
                ]);
            }
        }

        (positions, indices, uvs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_interpolation_respects_non_unit_grid_scale() {
        let terrain = SceneTerrain::new(2, 2, 2.0, |point| point.x + point.y * 2.0);
        assert!((terrain.height_at(Vec2::new(-1.0, -1.0)).unwrap() - 1.5).abs() < 0.0001);
        assert_eq!(terrain.height_at(Vec2::new(-3.0, 0.0)), None);
    }

    #[test]
    fn height_sampling_matches_each_mesh_triangle() {
        let terrain = SceneTerrain::new(
            1,
            1,
            1.0,
            |point| {
                if point == Vec2::ONE { 1.0 } else { 0.0 }
            },
        );

        // The diagonal high vertex affects both triangles linearly, not as the
        // bilinear saddle that used to diverge from mesh and collider height.
        assert!((terrain.height_at(Vec2::new(0.25, -0.25)).unwrap() - 0.25).abs() < 0.0001);
        assert!((terrain.height_at(Vec2::new(-0.25, 0.25)).unwrap() - 0.25).abs() < 0.0001);
        assert_eq!(terrain.height_at(Vec2::ZERO).unwrap(), 0.5);
    }

    #[test]
    fn mesh_vertices_follow_heightmap_row_major_order() {
        let terrain = SceneTerrain::new(2, 2, 1.0, |point| point.x + point.y * 10.0);
        let (positions, indices, _) = terrain.mesh_components();

        assert_eq!(positions[0][1], 0.0);
        assert_eq!(positions[1][1], 1.0);
        assert_eq!(positions[2][1], 2.0);
        assert_eq!(positions[3][1], 10.0);

        let a = Vec3::from_array(positions[indices[0] as usize]);
        let b = Vec3::from_array(positions[indices[1] as usize]);
        let c = Vec3::from_array(positions[indices[2] as usize]);
        assert!((b - a).cross(c - a).y > 0.0);
    }

    #[test]
    fn normals_are_finite_at_center_and_boundary() {
        let terrain = SceneTerrain::new(2, 2, 2.0, |point| point.x * 0.25);
        for position in [Vec2::ZERO, Vec2::new(2.0, 2.0)] {
            let normal = terrain.normal_at(position).unwrap();
            assert!(normal.is_finite());
            assert!((normal.length() - 1.0).abs() < 0.0001);
            assert!(normal.y > 0.0);
        }
    }
}
