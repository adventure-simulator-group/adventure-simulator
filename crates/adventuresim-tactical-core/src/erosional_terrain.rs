//! Three-dimensional erosion fields. Mapped lithology constrains a procedural
//! landform; neither bedding nor an observed cliff is asserted by the recipe.

use bevy::math::{FloatExt, Vec2, Vec3, Vec3Swizzles};

use crate::{
    marching_tetrahedra::marching_tetrahedra,
    scene::SceneTerrain,
    volumetric_terrain::{SceneTerrainPatch, TerrainLandformKind, TerrainLandformRecipe},
};

mod basalt;
mod carbonate;
mod granite;
mod slump;

const SAMPLE_MARGIN_CELLS: f32 = 2.0;
const MAX_GRID_SIDE: usize = 195;
const MAX_GRID_HEIGHT: usize = 96;
const SANDSTONE_RECESS_METRES: f32 = 1.6;
const SANDSTONE_ROOF_FRACTION: f32 = 0.3;
const TALUS_RELIEF_FRACTION: f32 = 0.18;
const TALUS_CENTRE_METRES: f32 = 3.0;
const TALUS_HALF_WIDTH_METRES: f32 = 4.0;
const CARBONATE_RESIDUAL_RELIEF_FRACTION: f32 = 0.05;

pub(crate) fn patch(
    terrain: &SceneTerrain,
    recipe: TerrainLandformRecipe,
) -> Result<SceneTerrainPatch, &'static str> {
    let spacing = f32::from(recipe.lod.voxel_cm()) / 100.0;
    let origin = Vec2::new(recipe.origin_cm[0] as f32, recipe.origin_cm[1] as f32) / 100.0;
    let radius = f32::from(recipe.half_length_cm.max(recipe.half_width_cm)) / 100.0 + 2.0;
    let radius = (radius / spacing).ceil() * spacing + spacing;
    let side = (radius * 2.0 / spacing).round() as usize + 1;
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    for z in 0..side {
        for x in 0..side {
            let point = origin + Vec2::new(x as f32, z as f32) * spacing - Vec2::splat(radius);
            let height = terrain_height(terrain, point);
            minimum = minimum.min(height);
            maximum = maximum.max(height);
        }
    }
    let relief = f32::from(recipe.relief_cm) / 100.0;
    let bottom = ((minimum - relief - spacing * SAMPLE_MARGIN_CELLS) / spacing).floor() * spacing
        - spacing * 0.5;
    let top = maximum + spacing * SAMPLE_MARGIN_CELLS;
    let vertical = ((top - bottom) / spacing).ceil() as usize + 1;
    if side > MAX_GRID_SIDE || vertical > MAX_GRID_HEIGHT {
        return Err("erosional patch voxel grid exceeds its bound");
    }
    marching_tetrahedra(
        [side, vertical, side],
        |index| {
            Vec3::new(
                origin.x + index[0] as f32 * spacing - radius,
                bottom + index[1] as f32 * spacing,
                origin.y + index[2] as f32 * spacing - radius,
            )
        },
        |point| field(terrain, recipe, point),
        recipe.transition_collar(),
    )
}

fn terrain_height(terrain: &SceneTerrain, point: Vec2) -> f32 {
    let half = Vec2::new(terrain.width(), terrain.depth()) * 0.5;
    terrain
        .height_at(point.clamp(-half, half))
        .expect("bounded terrain sample")
}

fn field(terrain: &SceneTerrain, recipe: TerrainLandformRecipe, point: Vec3) -> f32 {
    let collar = recipe.transition_collar();
    let weight = collar.blend_weight(point.xz());
    let height = terrain_height(terrain, point.xz());
    let base = point.y - height;
    if weight <= 0.0 {
        return base;
    }
    let local = collar.local_coordinates(point.xz());
    let tangent = Vec2::new(
        f32::from(recipe.tangent_permyriad[0]),
        f32::from(recipe.tangent_permyriad[1]),
    )
    .normalize();
    let downhill = Vec2::new(-tangent.y, tangent.x);
    let foot_distance = f32::from(recipe.half_width_cm - recipe.collar_cm) / 100.0;
    // Grade to the existing downhill foot. Subtracting relief from every local
    // height would create a closed trench when the collar returned uphill.
    let foot = terrain_height(terrain, point.xz() + downhill * (foot_distance - local.y));
    let relief = (height - foot)
        .max(0.001)
        .min(f32::from(recipe.relief_cm) / 100.0);
    let depth_fraction = ((height - point.y) / relief).clamp(0.0, 1.0);
    // A thick upper ledge remains above a shallow weathering alcove. This is
    // variation within sandstone, not an invented sandstone/shale contact.
    let recess = ((depth_fraction - SANDSTONE_ROOF_FRACTION) / (1.0 - SANDSTONE_ROOF_FRACTION)
        * std::f32::consts::PI)
        .sin()
        .max(0.0);
    let (front, debris_fraction) = match recipe.kind {
        TerrainLandformKind::SandstoneAlcove => (
            -SANDSTONE_RECESS_METRES.min(relief * 0.35) * recess,
            TALUS_RELIEF_FRACTION,
        ),
        TerrainLandformKind::CarbonateDissolution => (
            carbonate::front(local.x, depth_fraction, relief, recipe.seed),
            CARBONATE_RESIDUAL_RELIEF_FRACTION,
        ),
        TerrainLandformKind::GraniteJointRockfall => (
            granite::front(local.x, depth_fraction, relief, recipe.seed),
            TALUS_RELIEF_FRACTION,
        ),
        TerrainLandformKind::BasaltCoolingColumns => {
            (basalt::front(local.x, recipe.seed), TALUS_RELIEF_FRACTION)
        }
        TerrainLandformKind::CohesiveSlumpHeadscarp => {
            (slump::front(local.x, depth_fraction, relief), 0.0)
        }
        TerrainLandformKind::FaultScarp => unreachable!("faults use the displacement field"),
    };
    let talus = (1.0 - ((local.y - TALUS_CENTRE_METRES) / TALUS_HALF_WIDTH_METRES).abs()).max(0.0)
        * relief
        * debris_fraction;
    let retained_material = match recipe.kind {
        TerrainLandformKind::GraniteJointRockfall => granite::fragments(local.x, local.y, relief),
        TerrainLandformKind::CohesiveSlumpHeadscarp => slump::bench(local.x, local.y),
        _ => 0.0,
    };
    let floor = foot + talus + retained_material;
    let cut = (local.y - front).min(point.y - floor);
    base.lerp(base.max(cut), weight)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumetric_terrain::{TerrainLandformKind, TerrainLandformLod};
    use std::collections::HashMap;

    fn recipe() -> TerrainLandformRecipe {
        TerrainLandformRecipe {
            kind: TerrainLandformKind::SandstoneAlcove,
            seed: 47115,
            origin_cm: [0, 0],
            tangent_permyriad: [10000, 0],
            relief_cm: 600,
            half_length_cm: 1200,
            half_width_cm: 1000,
            collar_cm: 250,
            lod: TerrainLandformLod::Detail,
        }
    }

    #[test]
    fn sandstone_has_a_supported_roof_and_multiple_vertical_crossings() {
        let terrain = SceneTerrain::new(60, 60, 1.0, |p| -(p.y - 30.0) * 0.4);
        let recipe = recipe();
        let collar = recipe.transition_collar();
        let centre = (0..100)
            .map(|step| -2.0 + step as f32 * 0.04)
            .find(|&z| {
                let local = collar.local_coordinates(Vec2::new(0.0, z));
                (-0.7..-0.5).contains(&local.y)
            })
            .unwrap();
        let signs: Vec<_> = (0..100)
            .map(|step| {
                field(
                    &terrain,
                    recipe,
                    Vec3::new(0.0, -7.0 + step as f32 * 0.08, centre),
                ) > 0.0
            })
            .collect();
        assert!(signs.windows(2).filter(|pair| pair[0] != pair[1]).count() >= 3);
        assert!(field(&terrain, recipe, Vec3::new(0.0, -0.5, centre)) < 0.0);
        assert_eq!(field(&terrain, recipe, Vec3::new(20.0, 1.0, 0.0)), 1.0);
    }

    #[test]
    fn erosion_foot_rejoins_downhill_without_an_exit_lip() {
        let terrain = SceneTerrain::new(60, 60, 1.0, |p| -(p.y - 30.0) * 0.4);
        let recipe = recipe();
        // Beyond the talus apron the surface drains to the uncut hillside.
        let surface = |z| {
            (0..2000)
                .rev()
                .map(|step| -8.0 + step as f32 * 0.005)
                .find(|&y| field(&terrain, recipe, Vec3::new(0.0, y, z)) <= 0.0)
                .unwrap()
        };
        let heights: Vec<_> = (0..61)
            .map(|step| surface(5.0 + step as f32 * 0.1))
            .collect();
        assert!(heights.windows(2).all(|pair| pair[1] <= pair[0] + 0.005));
    }

    #[test]
    fn collar_cannot_be_longer_than_the_landform() {
        let terrain = SceneTerrain::new(60, 60, 1.0, |_| 0.0);
        let invalid = TerrainLandformRecipe {
            half_length_cm: 400,
            collar_cm: 500,
            half_width_cm: 2000,
            ..recipe()
        };
        assert!(invalid.validate(&terrain).is_err());
    }

    #[test]
    fn sandstone_mesh_is_deterministic_and_has_no_interior_boundary_edges() {
        assert_sound_mesh(TerrainLandformKind::SandstoneAlcove);
    }

    #[test]
    fn carbonate_mesh_is_deterministic_and_has_no_interior_boundary_edges() {
        assert_sound_mesh(TerrainLandformKind::CarbonateDissolution);
    }

    #[test]
    fn carbonate_has_separate_open_hollows_with_solid_bridges() {
        let terrain = SceneTerrain::new(60, 60, 1.0, |p| -(p.y - 30.0) * 0.4);
        let recipe = TerrainLandformRecipe {
            kind: TerrainLandformKind::CarbonateDissolution,
            ..recipe()
        };
        let max_crossings = |x| {
            (0..80)
                .map(|step| {
                    let z = -3.0 + step as f32 * 0.075;
                    let signs: Vec<_> = (0..160)
                        .map(|layer| {
                            field(
                                &terrain,
                                recipe,
                                Vec3::new(x, -6.0 + layer as f32 * 0.05, z),
                            ) > 0.0
                        })
                        .collect();
                    signs.windows(2).filter(|pair| pair[0] != pair[1]).count()
                })
                .max()
                .unwrap()
        };
        for x in [-6.0, -0.5, 5.5] {
            assert!(max_crossings(x) >= 3, "missing overhung hollow at {x}");
        }
        assert_eq!(max_crossings(2.15), 1, "rock bridge must remain solid");
    }

    #[test]
    fn fault_mesh_retains_consistent_winding_at_both_resolutions() {
        assert_sound_mesh(TerrainLandformKind::FaultScarp);
    }

    #[test]
    fn granite_has_consistent_winding_and_overhangs_at_both_resolutions() {
        assert_sound_mesh(TerrainLandformKind::GraniteJointRockfall);
    }

    #[test]
    fn basalt_has_consistent_winding_and_vertical_faces_at_both_resolutions() {
        assert_sound_mesh(TerrainLandformKind::BasaltCoolingColumns);
    }

    #[test]
    fn slump_has_consistent_winding_and_steep_head_at_both_resolutions() {
        assert_sound_mesh(TerrainLandformKind::CohesiveSlumpHeadscarp);
    }

    fn assert_sound_mesh(kind: TerrainLandformKind) {
        for lod in [TerrainLandformLod::Detail, TerrainLandformLod::Fringe] {
            assert_sound_mesh_lod(kind, lod);
        }
    }

    fn assert_sound_mesh_lod(kind: TerrainLandformKind, lod: TerrainLandformLod) {
        let terrain = SceneTerrain::new(60, 60, 1.0, |p| -(p.y - 30.0) * 0.4 + (p.x - 30.0) * 0.03);
        let recipe = TerrainLandformRecipe {
            kind,
            lod,
            ..recipe()
        };
        let generate = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| {
                    crate::volumetric_terrain::terrain_landform_patch(&terrain, recipe).unwrap()
                })
        };
        let mesh = generate(1);
        assert_eq!(mesh, generate(4));
        let mut edges = HashMap::<(u32, u32), (usize, i32)>::new();
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| Vec3::from_array(mesh.positions[i as usize]));
            assert!(a.is_finite() && b.is_finite() && c.is_finite());
            assert!((b - a).cross(c - a).length_squared() > 0.0);
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let entry = edges.entry((a.min(b), a.max(b))).or_default();
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        for ((a, b), (count, direction_sum)) in edges {
            let midpoint = (Vec3::from_array(mesh.positions[a as usize])
                + Vec3::from_array(mesh.positions[b as usize]))
                * 0.5;
            assert!(count <= 2);
            if count == 2 {
                assert_eq!(
                    direction_sum, 0,
                    "adjacent faces must wind opposite ways on their shared edge"
                );
            }
            if recipe.transition_collar().contains(midpoint.xz()) {
                assert_eq!(
                    count, 2,
                    "interior edge {:?} -> {:?}",
                    mesh.positions[a as usize], mesh.positions[b as usize]
                );
            }
        }
        if matches!(
            kind,
            TerrainLandformKind::BasaltCoolingColumns | TerrainLandformKind::CohesiveSlumpHeadscarp
        ) {
            let vertical_area: f32 = mesh
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|triangle| {
                    let [a, b, c] = triangle.map(|i| Vec3::from_array(mesh.positions[i as usize]));
                    let cross = (b - a).cross(c - a);
                    if cross.normalize().y.abs() < 0.025 {
                        cross.length() * 0.5
                    } else {
                        0.0
                    }
                })
                .sum();
            assert!(vertical_area > 5.0, "vertical area {vertical_area}");
        } else if kind != TerrainLandformKind::FaultScarp {
            assert!(mesh.normals.iter().any(|normal| normal[1] < -0.2));
        }
        assert!(mesh.triangle_count() < 150000);
    }
}
