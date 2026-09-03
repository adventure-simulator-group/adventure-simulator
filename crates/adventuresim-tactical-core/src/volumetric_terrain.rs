//! Bounded implicit terrain patches and deterministic marching tetrahedra.
//!
//! The server selects a compact, immutable landform recipe. Server collision
//! and client presentation extract the same field after heightfield repair;
//! no terrain simulation or mesh persistence runs during tactical ticks.
//! The heightfield remains authoritative outside the transition collar.

use bevy::{
    math::{FloatExt, Vec2, Vec3, Vec3Swizzles},
    prelude::{Component, Reflect, ReflectComponent},
};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::marching_tetrahedra::marching_tetrahedra;
use crate::{scene::SceneTerrain, terrain_transition::TerrainTransitionCollar};

mod recipe;
pub use recipe::{TerrainLandformKind, TerrainLandformLod, TerrainLandformRecipe};

#[derive(Component, Clone, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct SceneTerrainPatch {
    pub transition_collar: TerrainTransitionCollar,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl SceneTerrainPatch {
    pub fn collider(&self) -> avian3d::prelude::Collider {
        avian3d::prelude::Collider::trimesh(
            self.positions
                .iter()
                .copied()
                .map(Vec3::from_array)
                .collect(),
            self.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|triangle| [triangle[0], triangle[1], triangle[2]])
                .collect(),
        )
    }

    pub fn collider_with_terrain(&self, terrain: &SceneTerrain) -> avian3d::prelude::Collider {
        let (mut positions, mut triangles) =
            terrain.collider_mesh_with_transition(self.transition_collar);
        let patch_offset = u32::try_from(positions.len())
            .expect("bounded tactical terrain collider fits in u32 indices");
        positions.extend(self.positions.iter().copied().map(Vec3::from_array));
        triangles.extend(self.indices.as_chunks::<3>().0.iter().map(|triangle| {
            [
                triangle[0] + patch_offset,
                triangle[1] + patch_offset,
                triangle[2] + patch_offset,
            ]
        }));
        avian3d::prelude::Collider::trimesh(positions, triangles)
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

pub fn terrain_landform_patch(
    terrain: &SceneTerrain,
    recipe: TerrainLandformRecipe,
) -> Result<SceneTerrainPatch, &'static str> {
    recipe.validate(terrain)?;
    if recipe.kind != TerrainLandformKind::FaultScarp {
        return crate::erosional_terrain::patch(terrain, recipe);
    }
    let spacing = f32::from(recipe.lod.voxel_cm()) / 100.0;
    let origin = Vec2::new(recipe.origin_cm[0] as f32, recipe.origin_cm[1] as f32) / 100.0;
    let radius = f32::from(recipe.half_length_cm.max(recipe.half_width_cm)) / 100.0
        + SCARP_RUPTURE_WANDER_METRES;
    // March one sample beyond the transition's outer edge. An isosurface that
    // lands exactly on the final voxel plane has no neighbouring cell from
    // which marching tetrahedra can emit its last triangles, which otherwise
    // leaves a thin rectangular opening at the fault tips.
    let radius = (radius / spacing).ceil() * spacing + spacing;
    let side = ((radius * 2.0 / spacing).round() as usize).saturating_add(1);
    let throw = f32::from(recipe.relief_cm) / 100.0;
    let surface =
        simulate_fault_scarp(terrain, recipe, side, spacing, origin - Vec2::splat(radius))?;
    let (minimum, maximum) = surface.height_range(terrain)?;
    // Offset the vertical lattice so a flat heightfield cannot coincide with
    // an entire voxel plane. Coincident zero-valued edges collapse otherwise
    // valid tetrahedra at the zero-displacement collar contour.
    let bottom = ((minimum - throw - spacing * 2.0) / spacing).floor() * spacing - spacing * 0.5;
    let required_top = maximum + throw + spacing * 2.0;
    let vertical = ((required_top - bottom) / spacing).ceil() as usize + 1;
    if side > 195 || vertical > 96 {
        return Err("fault patch voxel grid exceeds its bound");
    }
    let dimensions = [side, vertical, side];
    let sample_position = |index: [usize; 3]| {
        Vec3::new(
            origin.x + index[0] as f32 * spacing - radius,
            bottom + index[1] as f32 * spacing,
            origin.y + index[2] as f32 * spacing - radius,
        )
    };
    let field = |position: Vec3| {
        position.y
            - (terrain_height_extended_to_edge(terrain, position.xz())
                + surface.offset_at(position.xz()))
    };
    marching_tetrahedra(
        dimensions,
        sample_position,
        field,
        recipe.transition_collar(),
    )
}

const SCARP_EROSION_STEPS: usize = 224;
const SCARP_COLLUVIAL_GRADE: f32 = 0.46;
const SCARP_RESISTANT_FACE_GRADE_RANGE: f32 = 2.6;
const SCARP_MAXIMUM_TRANSFER_FRACTION: f32 = 0.20;
const SCARP_RUPTURE_WANDER_METRES: f32 = 1.25;
const SCARP_WIDTH_VARIATION_BPS: u16 = 1_200;
const SCARP_FREE_FACE_HALF_WIDTH_METRES: f32 = 0.55;
const SCARP_GULLY_COUNT: usize = 4;

struct SimulatedScarpSurface {
    minimum: Vec2,
    spacing: f32,
    side: usize,
    offsets: Vec<f32>,
}

impl SimulatedScarpSurface {
    fn offset_at(&self, point: Vec2) -> f32 {
        let grid = (point - self.minimum) / self.spacing;
        let x0 = (grid.x.floor() as usize).min(self.side - 2);
        let z0 = (grid.y.floor() as usize).min(self.side - 2);
        let fraction = (grid - Vec2::new(x0 as f32, z0 as f32)).clamp(Vec2::ZERO, Vec2::ONE);
        let a = self.offsets[z0 * self.side + x0];
        let b = self.offsets[z0 * self.side + x0 + 1];
        let c = self.offsets[(z0 + 1) * self.side + x0];
        let d = self.offsets[(z0 + 1) * self.side + x0 + 1];
        a.lerp(b, fraction.x)
            .lerp(c.lerp(d, fraction.x), fraction.y)
    }

    fn height_range(&self, terrain: &SceneTerrain) -> Result<(f32, f32), &'static str> {
        let range = self
            .offsets
            .par_iter()
            .enumerate()
            .map(|(index, offset)| {
                let x = index % self.side;
                let z = index / self.side;
                let point = self.minimum + Vec2::new(x as f32, z as f32) * self.spacing;
                let height = terrain_height_extended_to_edge(terrain, point) + offset;
                (height, height)
            })
            .reduce(
                || (f32::INFINITY, f32::NEG_INFINITY),
                |left, right| (left.0.min(right.0), left.1.max(right.1)),
            );
        Ok(range)
    }
}

fn simulate_fault_scarp(
    terrain: &SceneTerrain,
    recipe: TerrainLandformRecipe,
    side: usize,
    spacing: f32,
    minimum: Vec2,
) -> Result<SimulatedScarpSurface, &'static str> {
    let collar = recipe.transition_collar();
    let throw = f32::from(recipe.relief_cm) / 100.0;
    // Keep both displaced blocks clear of the original heightfield.  A zero
    // offset on either side makes the replacement isosurface coincide with
    // the removed heightfield and can leave an unmeshed ownership hole.  The
    // unequal split leaves most relief on the footwall while retaining a
    // lower source surface from which the erosion pass can build its apron.
    let footwall_uplift = throw * 0.875;
    let hanging_wall_drop = -throw * 0.125;
    let mut offsets = vec![0.0; side * side];
    let mut masks = vec![0.0; side * side];
    let mut base_heights = vec![0.0; side * side];
    offsets
        .par_iter_mut()
        .zip(masks.par_iter_mut())
        .zip(base_heights.par_iter_mut())
        .enumerate()
        .for_each(|(index, ((offset, mask), base_height))| {
            let x = index % side;
            let z = index / side;
            let point = minimum + Vec2::new(x as f32, z as f32) * spacing;
            let local = collar.local_coordinates(point);
            let along = local.x;
            let across = local.y;
            *mask = collar.blend_weight(point);
            let throw_variation = 0.84
                + (smooth_value_noise(recipe.seed ^ 0x7468_726f_7700_0001, along / 3.6) * 0.5
                    + 0.5)
                    * 0.16;
            *offset = if across >= 0.0 {
                hanging_wall_drop
            } else {
                footwall_uplift
            } * *mask
                * throw_variation;
            *base_height = terrain_height_extended_to_edge(terrain, point);
        });

    erode_surface(
        &mut offsets,
        &masks,
        &base_heights,
        recipe,
        collar,
        side,
        spacing,
        minimum,
    );

    Ok(SimulatedScarpSurface {
        minimum,
        spacing,
        side,
        offsets,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the erosion grid inputs are independent"
)]
fn erode_surface(
    offsets: &mut [f32],
    masks: &[f32],
    base_heights: &[f32],
    recipe: TerrainLandformRecipe,
    collar: TerrainTransitionCollar,
    side: usize,
    spacing: f32,
    minimum: Vec2,
) {
    let mut horizontal_transfers = vec![0.0; offsets.len()];
    let mut vertical_transfers = vec![0.0; offsets.len()];
    for _ in 0..SCARP_EROSION_STEPS {
        horizontal_transfers
            .par_iter_mut()
            .zip(vertical_transfers.par_iter_mut())
            .enumerate()
            .for_each(|(source, (horizontal, vertical))| {
                let x = source % side;
                let z = source / side;
                let source_point = minimum + Vec2::new(x as f32, z as f32) * spacing;
                let transfer_to = |nx: usize, nz: usize| {
                    if nx >= side || nz >= side {
                        return 0.0;
                    }
                    let target = nz * side + nx;
                    if masks[source] <= 0.0 || masks[target] <= 0.0 {
                        return 0.0;
                    }
                    let difference = (base_heights[source] + offsets[source])
                        - (base_heights[target] + offsets[target]);
                    let target_point = minimum + Vec2::new(nx as f32, nz as f32) * spacing;
                    let resistance = scarp_resistance(
                        recipe.seed,
                        collar,
                        (source_point + target_point) * 0.5,
                        f32::from(recipe.half_length_cm) / 100.0,
                    );
                    let threshold = spacing
                        * (SCARP_COLLUVIAL_GRADE + resistance * SCARP_RESISTANT_FACE_GRADE_RANGE);
                    let excess = difference.abs() - threshold;
                    if excess <= 0.0 {
                        return 0.0;
                    }
                    let transfer =
                        excess * SCARP_MAXIMUM_TRANSFER_FRACTION * (1.0 - resistance * 0.55);
                    transfer * difference.signum()
                };
                *horizontal = transfer_to(x + 1, z);
                *vertical = transfer_to(x, z + 1);
            });
        offsets
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, offset)| {
                if masks[index] <= 0.0 {
                    *offset = 0.0;
                    return;
                }
                let x = index % side;
                let z = index / side;
                let mut transfer = 0.0;
                if z > 0 {
                    transfer += vertical_transfers[index - side];
                }
                if x > 0 {
                    transfer += horizontal_transfers[index - 1];
                }
                transfer -= horizontal_transfers[index];
                transfer -= vertical_transfers[index];
                *offset += transfer;
            });
    }
}

fn terrain_height_extended_to_edge(terrain: &SceneTerrain, point: Vec2) -> f32 {
    let half = Vec2::new(terrain.width(), terrain.depth()) * 0.5;
    terrain
        .height_at(point.clamp(-half, half))
        .expect("clamped point lies on validated terrain")
}

fn smooth_value_noise(seed: u64, coordinate: f32) -> f32 {
    let cell = coordinate.floor() as i64;
    let fraction = smoothstep01(coordinate - coordinate.floor());
    let sample = |offset: i64| {
        inclusive_unit_f32(splitmix64(seed ^ cell.wrapping_add(offset) as u64)) * 2.0 - 1.0
    };
    sample(0).lerp(sample(1), fraction)
}

fn scarp_resistance(
    seed: u64,
    collar: TerrainTransitionCollar,
    point: Vec2,
    half_length: f32,
) -> f32 {
    let local = collar.local_coordinates(point);
    // Resistant material is biased slightly into the uplifted block.  The
    // hanging-wall side therefore relaxes to a broader colluvial toe while a
    // narrower bedrock free face survives below the rounded crest.
    let free_face_distance = (local.y + 0.35).abs();
    let free_face = smoothstep01(
        (SCARP_FREE_FACE_HALF_WIDTH_METRES - free_face_distance)
            / SCARP_FREE_FACE_HALF_WIDTH_METRES,
    );
    let coherent_material =
        (smooth_value_noise(seed ^ 0x6d61_7465_7269_616c, local.x / 6.5 + local.y / 8.0) * 0.5
            + 0.5)
            * 0.18;
    let drainage = scarp_gully_strength(seed, local.x, half_length)
        * smoothstep01((SCARP_FREE_FACE_HALF_WIDTH_METRES * 2.4 - local.y.abs()) / 3.0);
    (0.08 + free_face * 0.78 + coherent_material - drainage * 0.58).clamp(0.0, 1.0)
}

fn scarp_gully_strength(seed: u64, along: f32, half_length: f32) -> f32 {
    (0..SCARP_GULLY_COUNT)
        .map(|index| {
            let hash = splitmix64(seed ^ 0x6775_6c6c_7900_0000 ^ index as u64);
            let interval = (index as f32 + 0.5) / SCARP_GULLY_COUNT as f32;
            let jitter = (inclusive_unit_f32(hash) - 0.5) * 0.16;
            let centre = (interval + jitter) * half_length * 1.8 - half_length * 0.9;
            let width = 1.4 + inclusive_unit_f32(splitmix64(hash)) * 1.3;
            smoothstep01((width - (along - centre).abs()) / width)
        })
        .fold(0.0, f32::max)
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn fault_scarp_is_deterministic_and_has_a_vertical_face() {
        let terrain = SceneTerrain::new(40, 40, 1.0, |_| 0.0);
        let recipe = TerrainLandformRecipe {
            kind: TerrainLandformKind::FaultScarp,
            seed: 17,
            origin_cm: [0, 0],
            tangent_permyriad: [10_000, 0],
            relief_cm: 600,
            half_length_cm: 1_000,
            half_width_cm: 800,
            collar_cm: 200,
            lod: TerrainLandformLod::Detail,
        };
        let generate_with_threads = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| terrain_landform_patch(&terrain, recipe).unwrap())
        };
        let first = generate_with_threads(1);
        let second = generate_with_threads(4);
        assert_eq!(first, second);
        assert!(first.triangle_count() > 100);
        assert!(first.normals.iter().any(|normal| normal[1].abs() < 0.25));
        assert!(
            first
                .positions
                .iter()
                .any(|position| position[0].abs() > 10.0),
            "the extracted surface must extend beyond the transition edge"
        );
        let rupture_depths = first
            .positions
            .iter()
            .zip(&first.normals)
            .filter(|(position, normal)| {
                position[0].abs() < 7.0 && normal[1].abs() < 0.55 && normal[2].abs() > 0.5
            })
            .map(|(position, _)| position[2])
            .collect::<Vec<_>>();
        let rupture_minimum = rupture_depths.iter().copied().fold(f32::INFINITY, f32::min);
        let rupture_maximum = rupture_depths
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(rupture_maximum - rupture_minimum > 0.5);
        let mut edges = HashMap::<(u32, u32), usize>::new();
        for triangle in first.indices.as_chunks::<3>().0 {
            for edge in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let key = if edge.0 < edge.1 {
                    edge
                } else {
                    (edge.1, edge.0)
                };
                *edges.entry(key).or_default() += 1;
            }
        }
        assert!(edges.values().all(|&uses| uses <= 2));
    }

    #[test]
    fn fringe_lod_keeps_the_full_feature_across_the_playable_edge() {
        let terrain = SceneTerrain::new(100, 100, 1.0, |_| 0.0);
        let recipe = TerrainLandformRecipe {
            kind: TerrainLandformKind::FaultScarp,
            seed: 29,
            origin_cm: [0, 6_000],
            tangent_permyriad: [10_000, 0],
            relief_cm: 800,
            half_length_cm: 4_500,
            half_width_cm: 1_800,
            collar_cm: 400,
            lod: TerrainLandformLod::Fringe,
        };

        let patch = terrain_landform_patch(&terrain, recipe).unwrap();

        assert!(patch.positions.iter().any(|position| position[2] <= 50.0));
        assert!(patch.positions.iter().any(|position| position[2] > 50.0));
        assert_eq!(patch.transition_collar, recipe.transition_collar());
    }

    #[test]
    fn collar_matches_the_heightfield_exactly() {
        let terrain = SceneTerrain::new(40, 40, 1.0, |point| point.x * 0.02);
        let recipe = TerrainLandformRecipe {
            kind: TerrainLandformKind::FaultScarp,
            seed: 17,
            origin_cm: [0, 0],
            tangent_permyriad: [10_000, 0],
            relief_cm: 600,
            half_length_cm: 1_000,
            half_width_cm: 800,
            collar_cm: 200,
            lod: TerrainLandformLod::Detail,
        };
        let spacing = f32::from(recipe.lod.voxel_cm()) / 100.0;
        let radius = f32::from(recipe.half_length_cm.max(recipe.half_width_cm)) / 100.0;
        let side = (radius * 2.0 / spacing).round() as usize + 1;
        let surface =
            simulate_fault_scarp(&terrain, recipe, side, spacing, Vec2::splat(-radius)).unwrap();
        assert_eq!(surface.offset_at(Vec2::new(0.0, 8.0)), 0.0);
        assert_eq!(surface.offset_at(Vec2::new(10.0, 0.0)), 0.0);
    }
}
