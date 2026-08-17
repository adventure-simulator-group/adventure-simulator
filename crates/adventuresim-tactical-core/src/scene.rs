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
#[component(immutable)]
pub struct SceneId(pub String);

/// Physical material below a tactical ground-cover layer.
///
/// This is authoritative semantic data rather than a render-material index so
/// server gameplay can query it without depending on client asset catalogs.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum GroundSubstrate {
    #[default]
    Soil,
    Stone,
    Gravel,
    Mud,
    Road,
    Water,
}

/// Mutually exclusive ground-cover profile at one tactical surface sample.
///
/// A profile can render several compatible details (for example leaves and
/// twigs), but a location never simultaneously claims two cover profiles.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum GroundCover {
    Bare,
    #[default]
    TallGrass,
    LeafLitter,
    LooseStone,
    Reeds,
}

/// Compact server-queryable description of one ground sample.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GroundSurface {
    pub substrate: GroundSubstrate,
    pub cover: GroundCover,
    pub cover_density_bps: u16,
    pub cover_height_cm: u16,
}

/// Authoritative spatial ground semantics shared by the tactical server and
/// clients. Samples use the same centered row-major grid convention as
/// [`SceneTerrain`], allowing presentation and future gameplay systems to ask
/// the same world-position question.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[component(immutable)]
pub struct SceneGround {
    samples: Vec<GroundSurface>,
    width: usize,
    scale: f32,
}

impl SceneGround {
    pub fn uniform_for_terrain(terrain: &SceneTerrain, surface: GroundSurface) -> Self {
        Self {
            samples: vec![surface; terrain.grid_width() * terrain.grid_depth()],
            width: terrain.grid_width(),
            scale: terrain.grid_scale(),
        }
    }

    pub fn from_samples(
        grid_width: usize,
        grid_depth: usize,
        scale: f32,
        samples: Vec<GroundSurface>,
    ) -> Option<Self> {
        if grid_width < 2
            || grid_depth < 2
            || !scale.is_finite()
            || scale <= 0.0
            || samples.len() != grid_width.checked_mul(grid_depth)?
            || samples
                .iter()
                .any(|sample| sample.cover_density_bps > 10_000 || sample.cover_height_cm > 1_000)
        {
            return None;
        }
        Some(Self {
            samples,
            width: grid_width,
            scale,
        })
    }

    pub fn grid_width(&self) -> usize {
        self.width
    }

    pub fn grid_depth(&self) -> usize {
        self.samples.len() / self.width
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

    pub fn samples(&self) -> &[GroundSurface] {
        &self.samples
    }

    /// Returns the owning discrete ground sample for a centered world point.
    /// Enum values are intentionally not interpolated across boundaries.
    pub fn ground_at(&self, pos: Vec2) -> Option<GroundSurface> {
        let grid = (pos + Vec2::new(self.width(), self.depth()) * 0.5) / self.scale;
        if grid.x < 0.0
            || grid.y < 0.0
            || grid.x > (self.grid_width() - 1) as f32
            || grid.y > (self.grid_depth() - 1) as f32
        {
            return None;
        }
        let x = (grid.x.floor() as usize).min(self.grid_width() - 1);
        let z = (grid.y.floor() as usize).min(self.grid_depth() - 1);
        self.samples.get(z * self.width + x).copied()
    }

    /// Conservatively checks a square scatter footprint. Macro grass patches
    /// use this to avoid extending blades across a leaf-litter boundary.
    pub fn cover_intersects_square(
        &self,
        centre: Vec2,
        half_extent: f32,
        cover: GroundCover,
    ) -> bool {
        if !half_extent.is_finite() || half_extent < 0.0 {
            return false;
        }
        let half_size = Vec2::new(self.width(), self.depth()) * 0.5;
        let minimum = ((centre - Vec2::splat(half_extent) + half_size) / self.scale)
            .floor()
            .max(Vec2::ZERO);
        let maximum = ((centre + Vec2::splat(half_extent) + half_size) / self.scale)
            .ceil()
            .min(Vec2::new(
                (self.grid_width() - 1) as f32,
                (self.grid_depth() - 1) as f32,
            ));
        if minimum.x > maximum.x || minimum.y > maximum.y {
            return false;
        }
        for z in minimum.y as usize..=maximum.y as usize {
            for x in minimum.x as usize..=maximum.x as usize {
                if self.samples[z * self.width + x].cover == cover {
                    return true;
                }
            }
        }
        false
    }

    pub fn cover_count(&self, cover: GroundCover) -> usize {
        self.samples
            .iter()
            .filter(|sample| sample.cover == cover)
            .count()
    }
}

#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq)]
pub struct SceneTerrain {
    heightmap: Vec<f32>,
    width: usize,
    scale: f32,
}

impl SceneTerrain {
    /// Builds terrain from an already-authoritative row-major height sample.
    ///
    /// The dimensions count grid vertices, while `SceneTerrain::new` counts
    /// cells. This constructor is the scene-input boundary used by both the
    /// server collider and the client's replicated playable mesh.
    pub fn from_heightmap(
        grid_width: usize,
        grid_depth: usize,
        scale: f32,
        heightmap: Vec<f32>,
    ) -> Option<Self> {
        if grid_width < 2
            || grid_depth < 2
            || !scale.is_finite()
            || scale <= 0.0
            || heightmap.len() != grid_width.checked_mul(grid_depth)?
            || heightmap.iter().any(|height| !height.is_finite())
        {
            return None;
        }
        Some(Self {
            heightmap,
            width: grid_width,
            scale,
        })
    }

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

    pub fn minimum_height(&self) -> f32 {
        self.heightmap.iter().copied().fold(f32::INFINITY, f32::min)
    }

    pub fn maximum_height(&self) -> f32 {
        self.heightmap
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
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
        self.mesh_with_cell_filter(|_| true)
    }

    /// Builds the client render mesh while allowing a presentation-owned tile to replace whole
    /// heightfield cells. Collision and terrain queries continue to use the unfiltered surface.
    #[cfg(feature = "meshgen")]
    pub fn mesh_with_cell_filter(&self, include_cell: impl Fn(Vec2) -> bool) -> Mesh {
        let (positions, indices, uvs) = self.mesh_components_with_cell_filter(include_cell);

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
        self.mesh_components_with_cell_filter(|_| true)
    }

    fn mesh_components_with_cell_filter(
        &self,
        include_cell: impl Fn(Vec2) -> bool,
    ) -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 2]>) {
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
                let cell_center = Vec2::new(
                    (x as f32 + 0.5 + mesh_offset.x) * self.scale,
                    (z as f32 + 0.5 + mesh_offset.y) * self.scale,
                );
                if !include_cell(cell_center) {
                    continue;
                }
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
    fn ground_grid_queries_discrete_centered_samples_and_rejects_invalid_data() {
        let mut samples = vec![GroundSurface::default(); 9];
        samples[4] = GroundSurface {
            substrate: GroundSubstrate::Soil,
            cover: GroundCover::LeafLitter,
            cover_density_bps: 8_500,
            cover_height_cm: 6,
        };
        let ground = SceneGround::from_samples(3, 3, 2.0, samples).unwrap();
        assert_eq!(
            ground.ground_at(Vec2::ZERO).unwrap().cover,
            GroundCover::LeafLitter
        );
        assert!(ground.cover_intersects_square(Vec2::new(1.8, 0.0), 0.3, GroundCover::LeafLitter));
        assert!(ground.ground_at(Vec2::new(2.01, 0.0)).is_none());
        assert!(SceneGround::from_samples(1, 3, 1.0, vec![GroundSurface::default(); 3]).is_none());
    }

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

    #[test]
    fn render_cell_filter_removes_only_strict_tile_interior() {
        let terrain = SceneTerrain::new(4, 4, 2.0, |_| 0.0);
        let (_, all_indices, _) = terrain.mesh_components();
        let (_, filtered_indices, _) = terrain.mesh_components_with_cell_filter(|center| {
            !(center.x > -2.0 && center.x < 2.0 && center.y > -2.0 && center.y < 2.0)
        });
        assert_eq!(all_indices.len(), 4 * 4 * 6);
        assert_eq!(filtered_indices.len(), (4 * 4 - 4) * 6);
        assert_eq!(terrain.height_at(Vec2::ZERO), Some(0.0));
    }
}
