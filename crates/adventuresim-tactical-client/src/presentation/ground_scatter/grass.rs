use adventuresim_tactical_core::prelude::{GroundCover, SceneGround, SceneTerrain};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    color::ColorToComponents,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::{
        Color, Commands, Handle, Image, Mesh, Mesh3d, MeshMaterial3d, Name, Quat, Transform, Vec2,
        Vec3, Vec4,
    },
};

use crate::presentation::{splitmix64, unit_hash};

use super::{GroundScatterLayer, TacticalFoliageMaterial, foliage_material};

pub(super) struct Assets {
    pub near_mesh: Handle<Mesh>,
    pub far_mesh: Handle<Mesh>,
    pub near_material: Handle<TacticalFoliageMaterial>,
    pub far_material: Handle<TacticalFoliageMaterial>,
}

pub(super) fn spawn(
    commands: &mut Commands,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    base_seed: u64,
    assets: &Assets,
) {
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    let count_x = (terrain.width() / GRASS_PATCH_SPACING).ceil() as i32;
    let count_z = (terrain.depth() / GRASS_PATCH_SPACING).ceil() as i32;
    for z in 0..count_z {
        for x in 0..count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell);
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let eligibility_world_x =
                -half_x + (x as f32 + 0.5 + jitter_x * 0.24) * GRASS_PATCH_SPACING;
            let eligibility_world_z =
                -half_z + (z as f32 + 0.5 + jitter_z * 0.24) * GRASS_PATCH_SPACING;
            let world_x = -half_x
                + (x as f32 + 0.5 + jitter_x * GRASS_PATCH_JITTER_FRACTION) * GRASS_PATCH_SPACING;
            let world_z = -half_z
                + (z as f32 + 0.5 + jitter_z * GRASS_PATCH_JITTER_FRACTION) * GRASS_PATCH_SPACING;
            let Some(transform) = grass_patch_placement(
                terrain,
                ground,
                Vec2::new(eligibility_world_x, eligibility_world_z),
                Vec2::new(world_x, world_z),
            ) else {
                continue;
            };
            commands.spawn((
                Name::new("Tactical grass near ribbons"),
                GroundScatterLayer::Grass,
                NotShadowCaster,
                Mesh3d(assets.near_mesh.clone()),
                MeshMaterial3d(assets.near_material.clone()),
                grass_lod_visibility(GrassMeshLod::Near),
                transform,
            ));
            commands.spawn((
                Name::new("Tactical grass far ribbons"),
                NotShadowCaster,
                Mesh3d(assets.far_mesh.clone()),
                MeshMaterial3d(assets.far_material.clone()),
                grass_lod_visibility(GrassMeshLod::Far),
                transform,
            ));
        }
    }
}

// A 54 x 54 grid preserves the established macro-patch footprint while
// placing four times as many authored blades per square metre as the original
// 27 x 27 grid.
const GRASS_PATCH_GRID_SIDE: usize = 54;
const GRASS_PATCH_SPACING: f32 = 3.2;
const GRASS_BLADE_SPACING: f32 = 3.51 / (GRASS_PATCH_GRID_SIDE - 1) as f32;
// Keep neighbouring near-flat macro patches inside the blade footprint even
// when their deterministic centre jitter diverges in opposite directions.
const GRASS_PATCH_JITTER_FRACTION: f32 = 0.04;
const GRASS_FAR_GRID_COORDINATES: [usize; 12] = [0, 5, 10, 14, 19, 24, 29, 34, 39, 43, 48, 53];
pub(super) fn grass_material(
    wind_scale: f32,
    lod: GrassMeshLod,
    grass_density: f32,
    grass_dryness: f32,
    ground_mask: Handle<Image>,
    ground: &SceneGround,
) -> TacticalFoliageMaterial {
    let mut material = foliage_material(wind_scale, true);
    // Grass uses this otherwise generic meadow-variation lane as a replicated
    // environmental dryness factor. Woodland shade and wet cover retain green
    // growth; exposed low-moisture swards develop coherent senescent cohorts.
    material.shading.y = grass_dryness;
    TacticalFoliageMaterial {
        // Only the near mesh is four times denser. The far mesh retains the
        // established 144-blade topology and projected coverage rather than
        // spending geometry on subpixel blades.
        shape: Vec4::new(1.0, 0.88, 0.09, lod.width_compensation(grass_density)),
        ground_mask_transform: Vec4::new(1.0 / ground.width(), 1.0 / ground.depth(), 0.5, 0.5),
        ground_mask: Some(ground_mask),
        ..material
    }
}
fn ground_allows_grass_patch(ground: &SceneGround, centre: Vec2) -> bool {
    let half_extent = GRASS_PATCH_SPACING * 0.58;
    [-1.0, 0.0, 1.0].into_iter().any(|z| {
        [-1.0, 0.0, 1.0].into_iter().any(|x| {
            ground
                .ground_at(centre + Vec2::new(x, z) * half_extent)
                .is_some_and(|sample| sample.cover == GroundCover::TallGrass)
        })
    })
}
fn grass_patch_transform(terrain: &SceneTerrain, world_x: f32, world_z: f32) -> Option<Transform> {
    let sample = Vec2::new(world_x, world_z);
    let height = terrain.height_at(sample)?;
    let normal = terrain.normal_at(sample)?;
    if normal.y < 0.72 {
        return None;
    }
    Some(
        Transform::from_xyz(world_x, height, world_z)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, normal)),
    )
}

fn grass_patch_placement(
    terrain: &SceneTerrain,
    ground: &SceneGround,
    legacy_predicate_centre: Vec2,
    render_centre: Vec2,
) -> Option<Transform> {
    // The legacy centre remains a one-way count-invariance guard: a formerly
    // rejected patch stays rejected. The actual rendered centre must also be
    // legal, so reducing jitter cannot move grass into leaf litter or outside
    // a usable terrain anchor.
    if !ground_allows_grass_patch(ground, legacy_predicate_centre)
        || !ground_allows_grass_patch(ground, render_centre)
    {
        return None;
    }
    grass_patch_transform(terrain, render_centre.x, render_centre.y)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GrassMeshLod {
    Near,
    Far,
}

impl GrassMeshLod {
    fn row_heights(self) -> &'static [f32] {
        match self {
            // Seven paired rows plus a shared tip: the same fifteen-vertex
            // near ribbon used by Ghost of Tsushima's published grass design.
            Self::Near => &[0.0, 0.14, 0.29, 0.45, 0.61, 0.76, 0.9],
            // Three paired rows plus a shared tip: seven vertices at distance.
            Self::Far => &[0.0, 0.45, 0.82],
        }
    }

    fn blade_grid_indices(self, grass_density: f32) -> impl Iterator<Item = usize> {
        let coordinates: &[usize] = match self {
            Self::Near => &[],
            Self::Far => &GRASS_FAR_GRID_COORDINATES,
        };
        (0..GRASS_PATCH_GRID_SIDE * GRASS_PATCH_GRID_SIDE).filter(move |index| {
            let selected_for_lod = if coordinates.is_empty() {
                true
            } else {
                let row = index / GRASS_PATCH_GRID_SIDE;
                let column = index % GRASS_PATCH_GRID_SIDE;
                coordinates.contains(&row) && coordinates.contains(&column)
            };
            selected_for_lod
                && (grass_density >= 1.0
                    || unit_hash(splitmix64(*index as u64 ^ 0x24e8_51c6_9a37_b40d)) < grass_density)
        })
    }

    fn blade_count(self, grass_density: f32) -> usize {
        self.blade_grid_indices(grass_density).count()
    }

    fn width_compensation(self, grass_density: f32) -> f32 {
        if self == Self::Near {
            return 1.0;
        }
        // Keep the far representation calibrated to the original 27 x 27
        // near field. The additional 54 x 54 density is intentionally local.
        let near_count = (27 * 27) as f32 * grass_density.clamp(0.0, 1.0);
        let lod_count = self.blade_count(grass_density).max(1) as f32;
        (near_count.max(1.0) / lod_count).sqrt()
    }
}

fn grass_lod_visibility(lod: GrassMeshLod) -> VisibilityRange {
    match lod {
        GrassMeshLod::Near => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: 18.0..26.0,
            use_aabb: false,
        },
        GrassMeshLod::Far => VisibilityRange {
            start_margin: 18.0..26.0,
            end_margin: 124.0..132.0,
            use_aabb: false,
        },
    }
}

pub(super) fn grass_patch_mesh(color: Color, lod: GrassMeshLod, grass_density: f32) -> Mesh {
    let grid_side = GRASS_PATCH_GRID_SIDE;
    let centre = (grid_side - 1) as f32 * 0.5;
    let blade_spacing = GRASS_BLADE_SPACING;
    let blades = lod
        .blade_grid_indices(grass_density)
        .map(|index| {
            let row = index / grid_side;
            let column = index % grid_side;
            let hash = splitmix64(index as u64 ^ 0x8d12_6f4a_0bc3_7791);
            let clump_x = ((row as f32 * 0.47 + column as f32 * 0.19).sin()) * blade_spacing * 0.24;
            let clump_z = ((column as f32 * 0.41 - row as f32 * 0.23).sin()) * blade_spacing * 0.24;
            let jitter_x = (unit_hash(hash) - 0.5) * blade_spacing * 0.46;
            let jitter_z = (unit_hash(splitmix64(hash)) - 0.5) * blade_spacing * 0.46;
            let clump_vigor = 0.5 + 0.5 * (row as f32 * 0.31 + column as f32 * 0.17 + 0.8).sin();
            let height_scale =
                (0.50 + unit_hash(splitmix64(hash ^ 0x52a9_f131)) * 0.62 + clump_vigor * 0.20)
                    .clamp(0.50, 1.30);
            let width_scale = 0.62 + unit_hash(splitmix64(hash ^ 0x91e2_57a4)) * 0.76;
            let base_x = (column as f32 - centre) * blade_spacing;
            let base_z = (row as f32 - centre) * blade_spacing;
            let mut offset_x = base_x + jitter_x + clump_x;
            let mut offset_z = base_z + jitter_z + clump_z;
            // Boundary rows may wander outward but never inward. This retains
            // organic clumping inside the patch while mitigating gaps along
            // near-flat and ordinary sloped shared edges.
            if column == 0 {
                offset_x = offset_x.min(base_x);
            } else if column + 1 == grid_side {
                offset_x = offset_x.max(base_x);
            }
            if row == 0 {
                offset_z = offset_z.min(base_z);
            } else if row + 1 == grid_side {
                offset_z = offset_z.max(base_z);
            }
            GrassBlade {
                offset_x,
                offset_z,
                height_scale,
                width_scale,
                seed: index as u64,
            }
        })
        .collect::<Vec<_>>();
    grass_ribbon_patch_mesh(0.026, 0.82, color, lod, &blades)
}

#[derive(Clone, Copy)]
struct GrassBlade {
    offset_x: f32,
    offset_z: f32,
    height_scale: f32,
    width_scale: f32,
    seed: u64,
}

fn grass_ribbon_patch_mesh(
    width: f32,
    height: f32,
    color: Color,
    lod: GrassMeshLod,
    blades: &[GrassBlade],
) -> Mesh {
    let rows = lod.row_heights();
    let vertices_per_blade = rows.len() * 2 + 1;
    let triangles_per_blade = (rows.len() - 1) * 2 + 1;
    let mut positions = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut normals = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut uvs = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut blade_roots = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut colors = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut indices = Vec::with_capacity(blades.len() * triangles_per_blade * 3);
    let linear = color.to_linear().to_f32_array();

    for &GrassBlade {
        offset_x,
        offset_z,
        height_scale,
        width_scale,
        seed: blade_seed,
    } in blades
    {
        let root = Vec3::new(offset_x, 0.0, offset_z);
        let hash = splitmix64(blade_seed ^ 0x6c8e_9cf5_701a_d30b);
        let angle = unit_hash(hash) * core::f32::consts::TAU;
        let half_width = Vec3::new(angle.cos(), 0.0, angle.sin()) * width * width_scale * 0.5;
        let normal = Vec3::Y.cross(half_width).normalize_or_zero().to_array();
        let blade_threshold = unit_hash(splitmix64(hash ^ 0x3d91_02ea_61b8_7c45));
        let pigment = unit_hash(splitmix64(hash ^ 0x76b3_144d));
        let warmth = unit_hash(splitmix64(hash ^ 0xa52d_98c7));
        let age = unit_hash(splitmix64(hash ^ 0x1b47_c95a_622d_41e3));
        let mature_age = ((age - 0.68) / (0.94 - 0.68)).clamp(0.0, 1.0);
        let mature_age = mature_age * mature_age * (3.0 - 2.0 * mature_age);
        let brightness = 0.82 + pigment * 0.30;
        let blade_color = [
            (linear[0] * brightness * (0.94 + warmth * 0.12)).clamp(0.0, 1.0),
            (linear[1] * brightness * (1.04 - warmth * 0.08)).clamp(0.0, 1.0),
            (linear[2] * brightness * (0.88 + warmth * 0.10)).clamp(0.0, 1.0),
            blade_threshold,
        ];
        let luminance = blade_color[0] * 0.2126 + blade_color[1] * 0.7152 + blade_color[2] * 0.0722;
        let straw_color = [luminance * 1.12, luminance * 0.88, luminance * 0.42];
        let base = positions.len() as u32;

        for &height_fraction in rows {
            let taper = (1.0 - height_fraction).powf(0.72);
            let side = half_width * taper;
            let centre = root + Vec3::Y * height * height_scale * height_fraction;
            positions.extend_from_slice(&[(centre - side).to_array(), (centre + side).to_array()]);
            normals.extend_from_slice(&[normal; 2]);
            uvs.extend_from_slice(&[[0.0, height_fraction], [1.0, height_fraction]]);
            blade_roots.extend_from_slice(&[[offset_x, offset_z]; 2]);
            let tip = ((height_fraction - 0.48) / (0.96 - 0.48)).clamp(0.0, 1.0);
            let tip = tip * tip * (3.0 - 2.0 * tip) * mature_age * 0.72;
            let row_color = [
                blade_color[0] + (straw_color[0] - blade_color[0]) * tip,
                blade_color[1] + (straw_color[1] - blade_color[1]) * tip,
                blade_color[2] + (straw_color[2] - blade_color[2]) * tip,
                blade_threshold,
            ];
            colors.extend_from_slice(&[row_color; 2]);
        }
        positions.push((root + Vec3::Y * height * height_scale).to_array());
        normals.push(normal);
        uvs.push([0.5, 1.0]);
        blade_roots.push([offset_x, offset_z]);
        colors.push([
            blade_color[0] + (straw_color[0] - blade_color[0]) * mature_age * 0.72,
            blade_color[1] + (straw_color[1] - blade_color[1]) * mature_age * 0.72,
            blade_color[2] + (straw_color[2] - blade_color[2]) * mature_age * 0.72,
            blade_threshold,
        ]);

        for row in 0..rows.len() - 1 {
            let lower = base + (row * 2) as u32;
            let upper = lower + 2;
            indices.extend_from_slice(&[lower, lower + 1, upper + 1, lower, upper + 1, upper]);
        }
        let shoulder = base + ((rows.len() - 1) * 2) as u32;
        let tip = base + (vertices_per_blade - 1) as u32;
        indices.extend_from_slice(&[shoulder, shoulder + 1, tip]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, blade_roots);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_tactical_core::prelude::GroundSurface;
    use bevy::{mesh::VertexAttributeValues, prelude::default};
    use std::collections::BTreeSet;

    #[test]
    fn grass_patches_use_a_stable_reduced_far_subset() {
        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let far = grass_patch_mesh(Color::WHITE, GrassMeshLod::Far, 1.0);
        let sparse = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 0.25);
        let near_positions = near
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let far_positions = far
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(near_positions.len(), 2_916 * 15);
        assert_eq!(far_positions.len(), 144 * 7);
        assert!(near_positions.len() > far_positions.len());
        let sparse_positions = sparse
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert!(!sparse_positions.is_empty());
        assert!(sparse_positions.len() < near_positions.len());
        let Some(VertexAttributeValues::Float32x2(near_roots)) =
            near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry stable blade roots");
        };
        let Some(VertexAttributeValues::Float32x2(far_roots)) = far.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("far grass mesh must carry stable blade roots");
        };
        assert_eq!(near_roots.len(), near_positions.len());
        assert_eq!(far_roots.len(), far_positions.len());
        assert!(far_roots.iter().all(|root| near_roots.contains(root)));
        let Some(VertexAttributeValues::Float32x4(colors)) = near.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("grass mesh must carry stable blade thresholds");
        };
        let Some(VertexAttributeValues::Float32x4(far_colors)) =
            far.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("far grass mesh must carry stable blade thresholds");
        };
        assert!(colors.iter().all(|color| (0.0..1.0).contains(&color[3])));
        assert!(colors.iter().any(|color| color[3] < 0.25));
        assert!(colors.iter().any(|color| color[3] > 0.75));
        for (far_root, far_color) in far_roots.chunks_exact(7).zip(far_colors.chunks_exact(7)) {
            let matching_near_blade = near_roots
                .chunks_exact(15)
                .position(|near_root| near_root[0] == far_root[0])
                .expect("every far blade must retain its exact near-LOD root");
            assert_eq!(
                colors[matching_near_blade * 15][3],
                far_color[0][3],
                "near and far LODs must apply the same ground-mask threshold"
            );
            assert_eq!(
                colors[matching_near_blade * 15],
                far_color[0],
                "near and far LOD roots must retain the same base pigment and age"
            );
            assert_eq!(
                colors[matching_near_blade * 15 + 14],
                far_color[6],
                "near and far LOD tips must retain the same senescent pigment"
            );
        }

        let blade_heights = near_positions
            .chunks_exact(15)
            .map(|blade| {
                blade
                    .iter()
                    .map(|position| position[1])
                    .fold(0.0_f32, f32::max)
            })
            .collect::<Vec<_>>();
        let minimum_height = blade_heights.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum_height = blade_heights
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            minimum_height < 0.52,
            "short blades should break the curtain silhouette"
        );
        assert!(
            maximum_height > 0.95,
            "mature blades should remain visibly taller"
        );
        assert!(maximum_height - minimum_height > 0.45);

        let blade_widths = near_positions
            .chunks_exact(15)
            .map(|blade| Vec3::from_array(blade[0]).distance(Vec3::from_array(blade[1])))
            .collect::<Vec<_>>();
        let minimum_width = blade_widths.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum_width = blade_widths
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(maximum_width / minimum_width > 2.0);

        let distinct_pigments = colors
            .iter()
            .map(|color| [color[0].to_bits(), color[1].to_bits(), color[2].to_bits()])
            .collect::<BTreeSet<_>>();
        assert!(distinct_pigments.len() > 100);
    }

    #[test]
    fn unit_scale_macro_patch_footprints_overlap_at_worst_case_near_flat_jitter() {
        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let Some(VertexAttributeValues::Float32x2(roots)) = near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry roots");
        };
        let min_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_z = roots
            .iter()
            .map(|root| root[1])
            .fold(f32::INFINITY, f32::min);
        let max_z = roots
            .iter()
            .map(|root| root[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let worst_adjacent_centre_distance =
            GRASS_PATCH_SPACING * (1.0 + GRASS_PATCH_JITTER_FRACTION);
        assert!(max_x - min_x > worst_adjacent_centre_distance);
        assert!(max_z - min_z > worst_adjacent_centre_distance);

        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).unwrap();
        let transform = grass_patch_transform(&terrain, 0.0, 0.0).unwrap();
        assert_eq!(transform.scale, Vec3::ONE);
        assert_eq!(transform.rotation, Quat::IDENTITY);
    }

    #[test]
    fn boundary_patch_is_retained_for_per_blade_ground_masking() {
        let width = 81;
        let depth = 41;
        let mut samples = vec![GroundSurface::default(); width * depth];
        // x=1.9 m: outside the legacy footprint centred at -0.32 m, but
        // inside the actual footprint centred at 0.0 m.
        let leaf_x = 59;
        let leaf_z = 20;
        samples[leaf_z * width + leaf_x].cover = GroundCover::LeafLitter;
        let ground = SceneGround::from_samples(width, depth, 0.1, samples).unwrap();
        let terrain = SceneTerrain::from_heightmap(9, 9, 1.0, vec![0.0; 81]).unwrap();
        let legacy = Vec2::new(-0.32, 0.0);
        let rendered = Vec2::ZERO;
        assert!(ground_allows_grass_patch(&ground, legacy));
        assert!(ground_allows_grass_patch(&ground, rendered));
        assert!(grass_patch_placement(&terrain, &ground, legacy, rendered).is_some());
    }

    #[test]
    fn invalid_render_anchor_is_skipped_without_legacy_fallback() {
        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).unwrap();
        let ground =
            SceneGround::from_samples(81, 81, 0.1, vec![GroundSurface::default(); 81 * 81])
                .unwrap();
        assert!(grass_patch_transform(&terrain, 0.0, 0.0).is_some());
        assert!(
            grass_patch_placement(&terrain, &ground, Vec2::ZERO, Vec2::new(2.0, 0.0)).is_none()
        );
    }

    #[test]
    fn representative_slope_keeps_adjacent_boundary_rows_overlapping() {
        let heights = (0..3)
            .flat_map(|_| (0..9).map(|x| x as f32 * 0.25))
            .collect::<Vec<_>>();
        let terrain = SceneTerrain::from_heightmap(9, 3, 1.0, heights).unwrap();
        let left = grass_patch_transform(&terrain, -1.6, 0.0).unwrap();
        let right = grass_patch_transform(&terrain, 1.6, 0.0).unwrap();
        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let Some(VertexAttributeValues::Float32x2(roots)) = near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry roots");
        };
        let min_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let direction = (right.translation - left.translation).normalize();
        let left_edge = left.transform_point(Vec3::new(max_x, 0.0, 0.0));
        let right_edge = right.transform_point(Vec3::new(min_x, 0.0, 0.0));
        assert!((right_edge - left_edge).dot(direction) <= 0.0);
    }

    #[test]
    fn grass_lods_crossfade_across_the_same_distance_interval() {
        let near = grass_lod_visibility(GrassMeshLod::Near);
        let far = grass_lod_visibility(GrassMeshLod::Far);
        assert_eq!(near.end_margin, far.start_margin);
        assert!(!near.is_abrupt());
        assert!(!far.is_abrupt());
    }

    #[test]
    fn grass_composition_reuses_existing_mask_fetch_and_preserves_topology() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_foliage.wgsl"
        ));
        assert_eq!(shader.matches("textureSampleLevel(").count(), 1);
        assert!(shader.contains("let effective_coverage = ground_coverage * clump_coverage"));
        assert!(shader.contains("let edge_growth = mix(0.58, 1.0"));
        assert!(!shader.contains("let tip_age"));
        assert!(shader.contains("* mix(1.0, 0.94, mature_age)"));
        assert!(shader.contains("lean_amount + 0.012 * mature_age"));

        let near = grass_patch_mesh(Color::WHITE, GrassMeshLod::Near, 1.0);
        let far = grass_patch_mesh(Color::WHITE, GrassMeshLod::Far, 1.0);
        assert_eq!(near.count_vertices(), 2_916 * 15);
        assert_eq!(far.count_vertices(), 144 * 7);
    }

    #[test]
    fn only_deep_leaf_litter_omits_a_grass_patch() {
        let mut samples = vec![GroundSurface::default(); 81];
        samples[40].cover = GroundCover::LeafLitter;
        let boundary = SceneGround::from_samples(9, 9, 1.0, samples).unwrap();
        assert!(ground_allows_grass_patch(&boundary, Vec2::ZERO));
        let litter = SceneGround::from_samples(
            9,
            9,
            1.0,
            vec![
                GroundSurface {
                    cover: GroundCover::LeafLitter,
                    ..default()
                };
                81
            ],
        )
        .unwrap();
        assert!(!ground_allows_grass_patch(&litter, Vec2::ZERO));
    }

    #[test]
    fn ground_foliage_enables_continuous_lod_and_interaction() {
        let grass = foliage_material(0.3, true);
        let crown = foliage_material(0.3, false);
        assert_eq!(grass.shading.w, 1.0);
        assert_eq!(crown.shading.w, 0.0);
        assert_eq!(grass.shape, Vec4::ZERO);
        assert_eq!(GrassMeshLod::Near.width_compensation(1.0), 1.0);
        assert_eq!(
            Vec4::new(1.0, 0.88, 0.09, GrassMeshLod::Near.width_compensation(1.0)),
            Vec4::new(1.0, 0.88, 0.09, 1.0)
        );
        assert_eq!(
            Vec4::new(1.0, 0.88, 0.09, GrassMeshLod::Far.width_compensation(1.0)),
            Vec4::new(1.0, 0.88, 0.09, 2.25)
        );
    }
}
