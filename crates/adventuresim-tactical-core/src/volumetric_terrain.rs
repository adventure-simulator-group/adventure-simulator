//! Bounded implicit terrain patches and deterministic marching tetrahedra.

use std::collections::HashMap;

use bevy::{
    math::{FloatExt, Vec2, Vec3, Vec3Swizzles},
    prelude::{Component, Reflect, ReflectComponent},
};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::scene::{SceneTerrain, TerrainTransitionCollar};

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(deny_unknown_fields)]
pub struct FaultScarpRecipe {
    pub seed: u64,
    pub origin_cm: [i32; 2],
    /// Unit tangent encoded in ten-thousandths.
    pub tangent_permyriad: [i16; 2],
    pub throw_cm: u16,
    pub half_length_cm: u16,
    pub half_width_cm: u16,
    pub collar_cm: u16,
    pub lod: FaultScarpLod,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultScarpLod {
    Detail,
    Fringe,
}

impl FaultScarpLod {
    const fn voxel_cm(self) -> u16 {
        match self {
            Self::Detail => 50,
            Self::Fringe => 100,
        }
    }
}

impl FaultScarpRecipe {
    pub fn validate(self, terrain: &SceneTerrain) -> Result<(), &'static str> {
        let tangent = Vec2::new(
            f32::from(self.tangent_permyriad[0]),
            f32::from(self.tangent_permyriad[1]),
        ) / 10_000.0;
        if !(0.98..=1.02).contains(&tangent.length()) {
            return Err("fault scarp tangent is not normalized");
        }
        if !(100..=2_000).contains(&self.throw_cm)
            || !(400..=5_000).contains(&self.half_length_cm)
            || !(300..=2_000).contains(&self.half_width_cm)
            || self.collar_cm < 100
            || self.collar_cm * 2 >= self.half_width_cm
        {
            return Err("fault scarp dimensions are outside their bounds");
        }
        let origin = Vec2::new(self.origin_cm[0] as f32, self.origin_cm[1] as f32) / 100.0;
        let half = Vec2::new(terrain.width(), terrain.depth()) * 0.5;
        let half_length = f32::from(self.half_length_cm) / 100.0;
        let half_width = f32::from(self.half_width_cm) / 100.0 + SCARP_RUPTURE_WANDER_METRES;
        let normal = Vec2::new(-tangent.y, tangent.x);
        let extent = tangent.abs() * half_length + normal.abs() * half_width;
        if origin.x.abs() > half.x + extent.x || origin.y.abs() > half.y + extent.y {
            return Err("fault scarp does not overlap the playable terrain");
        }
        Ok(())
    }

    pub fn transition_collar(self) -> TerrainTransitionCollar {
        let origin = Vec2::new(self.origin_cm[0] as f32, self.origin_cm[1] as f32) / 100.0;
        let tangent = Vec2::new(
            f32::from(self.tangent_permyriad[0]),
            f32::from(self.tangent_permyriad[1]),
        ) / 10_000.0;
        let half_length = f32::from(self.half_length_cm) / 100.0;
        let half_width = f32::from(self.half_width_cm) / 100.0;
        TerrainTransitionCollar::irregular_ellipse(
            origin,
            tangent,
            half_length,
            half_width,
            f32::from(self.collar_cm) / 100.0,
            self.seed,
            SCARP_RUPTURE_WANDER_METRES,
            SCARP_WIDTH_VARIATION_BPS,
        )
        .expect("validated fault-scarp dimensions produce a transition collar")
    }
}

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

pub fn fault_scarp_patch(
    terrain: &SceneTerrain,
    recipe: FaultScarpRecipe,
) -> Result<SceneTerrainPatch, &'static str> {
    recipe.validate(terrain)?;
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
    let throw = f32::from(recipe.throw_cm) / 100.0;
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
    recipe: FaultScarpRecipe,
    side: usize,
    spacing: f32,
    minimum: Vec2,
) -> Result<SimulatedScarpSurface, &'static str> {
    let collar = recipe.transition_collar();
    let throw = f32::from(recipe.throw_cm) / 100.0;
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

    Ok(SimulatedScarpSurface {
        minimum,
        spacing,
        side,
        offsets,
    })
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

fn marching_tetrahedra(
    dimensions: [usize; 3],
    position_at: impl Fn([usize; 3]) -> Vec3 + Sync,
    field: impl Fn(Vec3) -> f32 + Sync,
    transition_collar: TerrainTransitionCollar,
) -> Result<SceneTerrainPatch, &'static str> {
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
    let sample_count = dimensions[0]
        .checked_mul(dimensions[1])
        .and_then(|n| n.checked_mul(dimensions[2]))
        .ok_or("fault patch sample count overflow")?;
    let samples = (0..sample_count)
        .into_par_iter()
        .map(|index| {
            let x = index % dimensions[0];
            let yz = index / dimensions[0];
            let y = yz % dimensions[1];
            let z = yz / dimensions[1];
            let position = position_at([x, y, z]);
            let value = field(position);
            if !position.is_finite() || !value.is_finite() {
                return Err("fault patch field is not finite");
            }
            Ok((position, value))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (positions, values): (Vec<_>, Vec<_>) = samples.into_iter().unzip();
    let sample_id = |x: usize, y: usize, z: usize| (z * dimensions[1] + y) * dimensions[0] + x;
    let mut mesh_positions = Vec::<Vec3>::new();
    let mut indices = Vec::<u32>::new();
    let mut edge_vertices = HashMap::<(usize, usize), u32>::new();
    {
        let mut vertex_on_edge = |a: usize, b: usize| -> Result<u32, &'static str> {
            let key = if a < b { (a, b) } else { (b, a) };
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
            let point = positions[a].lerp(positions[b], fraction);
            let index = u32::try_from(mesh_positions.len())
                .map_err(|_| "fault patch has too many vertices")?;
            mesh_positions.push(point);
            edge_vertices.insert(key, index);
            Ok(index)
        };
        for z in 0..dimensions[2] - 1 {
            for y in 0..dimensions[1] - 1 {
                for x in 0..dimensions[0] - 1 {
                    let cube =
                        CUBE.map(|offset| sample_id(x + offset[0], y + offset[1], z + offset[2]));
                    for tet in TETS {
                        let vertices = tet.map(|corner| cube[corner]);
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
                        match (inside.len(), outside.len()) {
                            (1, 3) => {
                                let triangle = outside
                                    .iter()
                                    .map(|&b| vertex_on_edge(inside[0], b))
                                    .collect::<Result<Vec<_>, _>>()?;
                                indices.extend(triangle);
                            }
                            (3, 1) => {
                                let triangle = inside
                                    .iter()
                                    .map(|&a| vertex_on_edge(a, outside[0]))
                                    .collect::<Result<Vec<_>, _>>()?;
                                indices.extend([triangle[0], triangle[2], triangle[1]]);
                            }
                            (2, 2) => {
                                let ac = vertex_on_edge(inside[0], outside[0])?;
                                let ad = vertex_on_edge(inside[0], outside[1])?;
                                let bc = vertex_on_edge(inside[1], outside[0])?;
                                let bd = vertex_on_edge(inside[1], outside[1])?;
                                indices.extend([ac, bc, ad, ad, bc, bd]);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    let oriented_triangles = indices
        .par_chunks_exact(3)
        .map(|source_triangle| {
            let mut triangle = [source_triangle[0], source_triangle[1], source_triangle[2]];
            let a = mesh_positions[triangle[0] as usize];
            let b = mesh_positions[triangle[1] as usize];
            let c = mesh_positions[triangle[2] as usize];
            let mut normal = (b - a).cross(c - a);
            if normal.length_squared() < 1e-10 {
                return None;
            }
            let centre = (a + b + c) / 3.0;
            let epsilon = 0.02;
            let gradient = Vec3::new(
                field(centre + Vec3::X * epsilon) - field(centre - Vec3::X * epsilon),
                field(centre + Vec3::Y * epsilon) - field(centre - Vec3::Y * epsilon),
                field(centre + Vec3::Z * epsilon) - field(centre - Vec3::Z * epsilon),
            );
            if normal.dot(gradient) < 0.0 {
                triangle.swap(1, 2);
                normal = -normal;
            }
            Some((triangle, normal))
        })
        .collect::<Vec<_>>();
    let mut normals = vec![Vec3::ZERO; mesh_positions.len()];
    let mut oriented_indices = Vec::with_capacity(indices.len());
    for (triangle, normal) in oriented_triangles.into_iter().flatten() {
        for &index in triangle.iter() {
            normals[index as usize] += normal;
        }
        oriented_indices.extend(triangle);
    }
    let normals = normals
        .into_iter()
        .map(|normal| normal.normalize_or_zero().to_array())
        .collect();
    if oriented_indices.is_empty() {
        return Err("fault patch extraction produced no surface");
    }
    Ok(SceneTerrainPatch {
        transition_collar,
        positions: mesh_positions
            .into_iter()
            .map(|position| position.to_array())
            .collect(),
        normals,
        indices: oriented_indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn fault_scarp_is_deterministic_and_has_a_vertical_face() {
        let terrain = SceneTerrain::new(40, 40, 1.0, |_| 0.0);
        let recipe = FaultScarpRecipe {
            seed: 17,
            origin_cm: [0, 0],
            tangent_permyriad: [10_000, 0],
            throw_cm: 600,
            half_length_cm: 1_000,
            half_width_cm: 800,
            collar_cm: 200,
            lod: FaultScarpLod::Detail,
        };
        let generate_with_threads = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| fault_scarp_patch(&terrain, recipe).unwrap())
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
        let recipe = FaultScarpRecipe {
            seed: 29,
            origin_cm: [0, 6_000],
            tangent_permyriad: [10_000, 0],
            throw_cm: 800,
            half_length_cm: 4_500,
            half_width_cm: 1_800,
            collar_cm: 400,
            lod: FaultScarpLod::Fringe,
        };

        let patch = fault_scarp_patch(&terrain, recipe).unwrap();

        assert!(patch.positions.iter().any(|position| position[2] <= 50.0));
        assert!(patch.positions.iter().any(|position| position[2] > 50.0));
        assert_eq!(patch.transition_collar, recipe.transition_collar());
    }

    #[test]
    fn collar_matches_the_heightfield_exactly() {
        let terrain = SceneTerrain::new(40, 40, 1.0, |point| point.x * 0.02);
        let recipe = FaultScarpRecipe {
            seed: 17,
            origin_cm: [0, 0],
            tangent_permyriad: [10_000, 0],
            throw_cm: 600,
            half_length_cm: 1_000,
            half_width_cm: 800,
            collar_cm: 200,
            lod: FaultScarpLod::Detail,
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
