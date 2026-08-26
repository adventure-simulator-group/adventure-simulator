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
#[reflect(Component)]
pub struct SceneTerrain {
    heightmap: Vec<f32>,
    width: usize,
    scale: f32,
    /// Number of authoritative sample intervals represented by one vertex in
    /// the coarse render LOD. Collision and spatial queries always use every
    /// sample; this stride only avoids submitting the full-resolution field
    /// outside the camera-local detail patch.
    coarse_stride: usize,
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
            coarse_stride: 1,
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
            coarse_stride: 1,
        }
    }

    /// Refines every coarse cell by an integer factor and evaluates one
    /// authoritative height function at the resulting vertices. The original
    /// grid remains implicit as the coarse render LOD through `coarse_stride`.
    pub fn refined(
        &self,
        target_spacing: f32,
        height_at: impl Fn(Vec2, f32) -> f32,
    ) -> Option<Self> {
        if !target_spacing.is_finite() || target_spacing <= 0.0 {
            return None;
        }
        let subdivisions = (self.scale / target_spacing).ceil().max(1.0) as usize;
        let cells_x = self
            .grid_width()
            .checked_sub(1)?
            .checked_mul(subdivisions)?;
        let cells_z = self
            .grid_depth()
            .checked_sub(1)?
            .checked_mul(subdivisions)?;
        let width = cells_x.checked_add(1)?;
        let depth = cells_z.checked_add(1)?;
        let scale = self.scale / subdivisions as f32;
        let half_extent = Vec2::new(self.width(), self.depth()) * 0.5;
        let mut heights = Vec::with_capacity(width.checked_mul(depth)?);
        for z in 0..depth {
            for x in 0..width {
                let point = Vec2::new(x as f32 * scale, z as f32 * scale) - half_extent;
                let base_height = self.height_at(point)?;
                let height = height_at(point, base_height);
                if !height.is_finite() {
                    return None;
                }
                heights.push(height);
            }
        }
        Some(Self {
            heightmap: heights,
            width,
            scale,
            coarse_stride: subdivisions,
        })
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

    pub fn coarse_grid_scale(&self) -> f32 {
        self.scale * self.coarse_stride.max(1) as f32
    }

    /// Constrains every cardinal edge to a maximum grade while retaining the
    /// same immutable dimensions and LOD relationship.
    pub fn constrain_max_grade(&mut self, maximum_grade: f32) -> bool {
        if !maximum_grade.is_finite() || maximum_grade < 0.0 {
            return false;
        }
        let maximum_step = self.scale * maximum_grade;
        for _ in 0..4 {
            for z in 0..self.grid_depth() {
                for x in 1..self.grid_width() {
                    self.clamp_height_pair(
                        z * self.width + x - 1,
                        z * self.width + x,
                        maximum_step,
                    );
                }
                for x in (0..self.grid_width() - 1).rev() {
                    self.clamp_height_pair(
                        z * self.width + x + 1,
                        z * self.width + x,
                        maximum_step,
                    );
                }
            }
            for x in 0..self.grid_width() {
                for z in 1..self.grid_depth() {
                    self.clamp_height_pair(
                        (z - 1) * self.width + x,
                        z * self.width + x,
                        maximum_step,
                    );
                }
                for z in (0..self.grid_depth() - 1).rev() {
                    self.clamp_height_pair(
                        (z + 1) * self.width + x,
                        z * self.width + x,
                        maximum_step,
                    );
                }
            }
        }
        true
    }

    fn clamp_height_pair(&mut self, source: usize, target: usize, maximum_step: f32) {
        let source_height = self.heightmap[source];
        self.heightmap[target] = self.heightmap[target]
            .clamp(source_height - maximum_step, source_height + maximum_step);
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
        self.surface_at_stride(pos, 1).map(|sample| sample.0)
    }

    /// Samples the triangle surface used by the coarse render LOD.
    pub fn coarse_height_at(&self, pos: Vec2) -> Option<f32> {
        self.surface_at_stride(pos, self.coarse_stride.max(1))
            .map(|sample| sample.0)
    }

    /// Samples the same triangle surface used by the rendered mesh and
    /// authoritative collider. Returning the triangle normal alongside the
    /// height keeps terrain IK from fitting a foot to a different, bilinear
    /// surface than the one visible beneath it.
    fn surface_at_stride(&self, pos: Vec2, stride: usize) -> Option<(f32, Vec3)> {
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
        let stride = stride.max(1);
        let last_cell_x = ((self.grid_width() - 2) / stride) * stride;
        let last_cell_y = ((self.grid_depth() - 2) / stride) * stride;
        let min_x = ((grid.x / stride as f32).floor() as usize * stride).min(last_cell_x);
        let min_y = ((grid.y / stride as f32).floor() as usize * stride).min(last_cell_y);
        let max_x = (min_x + stride).min(self.grid_width() - 1);
        let max_y = (min_y + stride).min(self.grid_depth() - 1);
        let cell_scale_x = (max_x - min_x) as f32 * self.scale;
        let cell_scale_z = (max_y - min_y) as f32 * self.scale;
        let fraction = Vec2::new(
            (grid.x - min_x as f32) / (max_x - min_x) as f32,
            (grid.y - min_y as f32) / (max_y - min_y) as f32,
        );

        let x0y0 = *self.heightmap.get(min_x + min_y * self.width)?;
        let x1y0 = *self.heightmap.get(max_x + min_y * self.width)?;
        let x0y1 = *self.heightmap.get(min_x + max_y * self.width)?;
        let x1y1 = *self.heightmap.get(max_x + max_y * self.width)?;

        let (height, tangent_x, tangent_z) = if fraction.x + fraction.y <= 1.0 {
            // Matches [x0y0, x0y1, x1y0] in `mesh_components_with_stride` and
            // Avian's default heightfield subdivision.
            (
                x0y0 + (x1y0 - x0y0) * fraction.x + (x0y1 - x0y0) * fraction.y,
                Vec3::new(cell_scale_x, x1y0 - x0y0, 0.0),
                Vec3::new(0.0, x0y1 - x0y0, cell_scale_z),
            )
        } else {
            // Matches [x1y0, x0y1, x1y1].
            (
                x1y1 + (x0y1 - x1y1) * (1.0 - fraction.x) + (x1y0 - x1y1) * (1.0 - fraction.y),
                Vec3::new(cell_scale_x, x1y1 - x0y1, 0.0),
                Vec3::new(0.0, x1y1 - x1y0, cell_scale_z),
            )
        };
        let normal = tangent_z.cross(tangent_x).try_normalize()?;
        Some((height, normal))
    }

    /// Returns the finite, normalized normal of the rendered/collided triangle.
    pub fn normal_at(&self, pos: Vec2) -> Option<Vec3> {
        self.surface_at_stride(pos, 1).map(|sample| sample.1)
    }

    pub fn collider(&self) -> Collider {
        let heights = self
            .heightmap
            .chunks_exact(self.grid_width())
            .map(|row| row.to_vec())
            .collect();
        Collider::heightfield(heights, Vec3::new(self.width(), 1.0, self.depth()))
    }

    #[cfg(feature = "meshgen")]
    pub fn mesh(&self) -> Mesh {
        self.mesh_with_stride(1)
    }

    #[cfg(feature = "meshgen")]
    pub fn coarse_mesh(&self) -> Mesh {
        self.mesh_with_stride(self.coarse_stride.max(1))
    }

    #[cfg(feature = "meshgen")]
    fn mesh_with_stride(&self, stride: usize) -> Mesh {
        let (positions, indices, uvs) = self.mesh_components_with_stride(stride);

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));

        mesh.with_computed_area_weighted_normals()
    }

    #[cfg(test)]
    fn mesh_components(&self) -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 2]>) {
        self.mesh_components_with_stride(1)
    }

    fn mesh_components_with_stride(
        &self,
        stride: usize,
    ) -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 2]>) {
        let stride = stride.max(1);
        let xs = (0..self.grid_width()).step_by(stride).collect::<Vec<_>>();
        let zs = (0..self.grid_depth()).step_by(stride).collect::<Vec<_>>();
        debug_assert_eq!(xs.last().copied(), Some(self.grid_width() - 1));
        debug_assert_eq!(zs.last().copied(), Some(self.grid_depth() - 1));
        let mesh_offset = Vec2::new(
            self.grid_width().saturating_sub(1) as f32,
            self.grid_depth().saturating_sub(1) as f32,
        ) * -0.5;

        let mut positions = Vec::with_capacity(xs.len() * zs.len());
        let mut uvs = Vec::with_capacity(xs.len() * zs.len());
        // Keep vertices in the same row-major (z * width + x) order used by
        // height sampling and the triangle indices below. Iterating x first
        // transposed the storage without transposing the indices, which made
        // otherwise smooth terrain render as long shredded triangles.
        for &z in &zs {
            for &x in &xs {
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

        let mut indices = Vec::with_capacity((xs.len() - 1) * (zs.len() - 1) * 6);
        for x in 0..xs.len() - 1 {
            for z in 0..zs.len() - 1 {
                let i = (z * xs.len() + x) as u32;

                indices.extend_from_slice(&[
                    i,
                    i + xs.len() as u32,
                    i + 1,
                    i + 1,
                    i + xs.len() as u32,
                    i + xs.len() as u32 + 1,
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

        // Avian's heightfield diagonal joins the two zero-height off-diagonal
        // vertices. The high corner therefore affects only its own triangle,
        // and CPU queries and rendered indices must use that same split.
        assert_eq!(terrain.height_at(Vec2::new(0.25, -0.25)).unwrap(), 0.0);
        assert_eq!(terrain.height_at(Vec2::new(-0.25, 0.25)).unwrap(), 0.0);
        assert_eq!(terrain.height_at(Vec2::ZERO).unwrap(), 0.0);
        assert!((terrain.height_at(Vec2::new(0.25, 0.25)).unwrap() - 0.5).abs() < 0.0001);
    }

    #[test]
    fn refined_surface_keeps_a_coarse_lod_without_splitting_query_authority() {
        let coarse = SceneTerrain::new(4, 4, 1.0, |_| 0.0);
        let refined = coarse
            .refined(0.5, |point, base| {
                base + (point.x * core::f32::consts::PI).sin()
                    * (point.y * core::f32::consts::PI).sin()
                    * 0.04
            })
            .unwrap();

        assert_eq!(refined.grid_scale(), 0.5);
        assert_eq!(refined.coarse_grid_scale(), 1.0);
        let point = Vec2::splat(0.5);
        assert!((refined.height_at(point).unwrap() - 0.04).abs() < 0.000_01);
        assert!(refined.coarse_height_at(point).unwrap().abs() < 0.000_01);
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
