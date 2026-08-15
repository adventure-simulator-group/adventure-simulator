use adventuresim_tactical_core::prelude::{GroundCover, SceneGround, SceneTerrain};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    color::ColorToComponents,
    light::NotShadowCaster,
    math::FloatExt,
    mesh::{Indices, PrimitiveTopology},
    prelude::{Color, Commands, Handle, Mesh, Mesh3d, MeshMaterial3d, Name, Transform, Vec2, Vec3},
};
use std::collections::BTreeMap;

#[cfg(test)]
use crate::presentation::generate_procedural_environment_assets;
use crate::presentation::{ProceduralEnvironmentAssets, bps, leaf_material, splitmix64, unit_hash};

use super::{
    GroundScatterLayer, TacticalFoliageMaterial, TacticalTreeLeafCardMaterial, foliage_transform,
};

const DRY_LEAF_PASSES_PER_SAMPLE: u64 = 3;
const TWIG_PASSES_PER_SAMPLE: u64 = 2;
const WOODLAND_PLANT_PASSES_PER_SAMPLE: u64 = 1;
const WOODLAND_FLOOR_TRANSITION_METRES: f32 = 3.2;

pub(super) struct Assets {
    pub dry_leaf_meshes: Vec<Handle<Mesh>>,
    pub twig_meshes: Vec<Handle<Mesh>>,
    pub dry_leaf_material: Handle<TacticalTreeLeafCardMaterial>,
    pub twig_material: Handle<TacticalFoliageMaterial>,
    pub woodland_plant_meshes: Vec<Handle<Mesh>>,
    pub woodland_plant_material: Handle<TacticalFoliageMaterial>,
}

#[derive(Default)]
struct LitterBatch {
    leaves: Option<Mesh>,
    twigs: Option<Mesh>,
    plants: Option<Mesh>,
}

#[derive(Clone, Copy)]
enum BatchKind {
    Leaves,
    Twigs,
    Plants,
}

pub(super) fn spawn(
    commands: &mut Commands,
    meshes: &mut bevy::prelude::Assets<Mesh>,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    base_seed: u64,
    assets: &Assets,
) {
    const BATCH_CELL_METRES: f32 = 64.0;
    let mut batches = BTreeMap::<(i32, i32), LitterBatch>::new();
    for (index, sample) in ground.samples().iter().enumerate() {
        let grid_x = index % ground.grid_width();
        let grid_z = index / ground.grid_width();
        let transition = match sample.cover {
            GroundCover::LeafLitter => 1.0,
            GroundCover::TallGrass => leaf_litter_proximity(ground, grid_x, grid_z),
            _ => 0.0,
        };
        if transition <= 0.0 {
            continue;
        }
        let cell_origin = Vec2::new(
            grid_x as f32 * ground.grid_scale() - ground.width() * 0.5,
            grid_z as f32 * ground.grid_scale() - ground.depth() * 0.5,
        );
        let density = if sample.cover == GroundCover::LeafLitter {
            bps(sample.cover_density_bps)
        } else {
            transition * 0.34
        };
        for pass in 0..DRY_LEAF_PASSES_PER_SAMPLE {
            let hash =
                splitmix64(base_seed ^ index as u64 ^ pass.rotate_left(19) ^ 0x2b6f_5dd9_81aa_9135);
            if unit_hash(hash) >= density * 0.92 {
                continue;
            }
            let Some(transform) =
                forest_floor_patch_transform(terrain, ground, cell_origin, hash, 0.8, 0.001)
            else {
                continue;
            };
            append_litter_batch(
                meshes,
                &assets.dry_leaf_meshes[(hash % assets.dry_leaf_meshes.len() as u64) as usize],
                transform,
                BATCH_CELL_METRES,
                &mut batches,
                BatchKind::Leaves,
            );
        }
        for pass in 0..TWIG_PASSES_PER_SAMPLE {
            let hash =
                splitmix64(base_seed ^ index as u64 ^ pass.rotate_left(23) ^ 0xc41b_b83e_3a70_f965);
            if unit_hash(hash) >= density * 0.62 {
                continue;
            }
            let Some(transform) =
                forest_floor_patch_transform(terrain, ground, cell_origin, hash, 0.72, 0.006)
            else {
                continue;
            };
            append_litter_batch(
                meshes,
                &assets.twig_meshes[(hash % assets.twig_meshes.len() as u64) as usize],
                transform,
                BATCH_CELL_METRES,
                &mut batches,
                BatchKind::Twigs,
            );
        }
        let plant_chance = if sample.cover == GroundCover::LeafLitter {
            0.055
        } else {
            transition * 0.38
        };
        for pass in 0..WOODLAND_PLANT_PASSES_PER_SAMPLE {
            let hash =
                splitmix64(base_seed ^ index as u64 ^ pass.rotate_left(27) ^ 0x7b31_eaf4_2c5d_9081);
            if unit_hash(hash) >= plant_chance {
                continue;
            }
            let Some(transform) =
                forest_floor_patch_transform(terrain, ground, cell_origin, hash, 0.7, 0.004)
            else {
                continue;
            };
            append_litter_batch(
                meshes,
                &assets.woodland_plant_meshes
                    [(hash % assets.woodland_plant_meshes.len() as u64) as usize],
                transform,
                BATCH_CELL_METRES,
                &mut batches,
                BatchKind::Plants,
            );
        }
    }
    for ((cell_x, cell_z), batch) in batches {
        let transform = Transform::from_xyz(
            cell_x as f32 * BATCH_CELL_METRES,
            0.0,
            cell_z as f32 * BATCH_CELL_METRES,
        );
        if let Some(mesh) = batch.leaves {
            commands.spawn((
                Name::new("Batched tactical dry leaves"),
                GroundScatterLayer::DryLeaves,
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(assets.dry_leaf_material.clone()),
                VisibilityRange::abrupt(0.0, 35.0),
                transform,
            ));
        }
        if let Some(mesh) = batch.twigs {
            commands.spawn((
                Name::new("Batched tactical twigs"),
                GroundScatterLayer::Twigs,
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(assets.twig_material.clone()),
                VisibilityRange::abrupt(0.0, 24.0),
                transform,
            ));
        }
        if let Some(mesh) = batch.plants {
            commands.spawn((
                Name::new("Batched tactical woodland-floor plants"),
                GroundScatterLayer::Understory,
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(assets.woodland_plant_material.clone()),
                VisibilityRange::abrupt(0.0, 28.0),
                transform,
            ));
        }
    }
}

fn leaf_litter_proximity(ground: &SceneGround, grid_x: usize, grid_z: usize) -> f32 {
    let radius = (WOODLAND_FLOOR_TRANSITION_METRES / ground.grid_scale()).ceil() as isize;
    let mut nearest = f32::INFINITY;
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let x = grid_x as isize + dx;
            let z = grid_z as isize + dz;
            if x < 0
                || z < 0
                || x >= ground.grid_width() as isize
                || z >= ground.grid_depth() as isize
                || ground.samples()[z as usize * ground.grid_width() + x as usize].cover
                    != GroundCover::LeafLitter
            {
                continue;
            }
            nearest = nearest.min(Vec2::new(dx as f32, dz as f32).length() * ground.grid_scale());
        }
    }
    (1.0 - nearest / WOODLAND_FLOOR_TRANSITION_METRES).clamp(0.0, 1.0)
}

fn append_litter_batch(
    meshes: &bevy::prelude::Assets<Mesh>,
    source: &Handle<Mesh>,
    mut transform: Transform,
    cell_size: f32,
    batches: &mut BTreeMap<(i32, i32), LitterBatch>,
    kind: BatchKind,
) {
    let cell = (
        (transform.translation.x / cell_size).floor() as i32,
        (transform.translation.z / cell_size).floor() as i32,
    );
    transform.translation.x -= cell.0 as f32 * cell_size;
    transform.translation.z -= cell.1 as f32 * cell_size;
    let Some(source) = meshes.get(source) else {
        return;
    };
    let transformed = source.clone().transformed_by(transform);
    let batch = batches.entry(cell).or_default();
    let slot = match kind {
        BatchKind::Leaves => &mut batch.leaves,
        BatchKind::Twigs => &mut batch.twigs,
        BatchKind::Plants => &mut batch.plants,
    };
    if let Some(batch) = slot {
        batch
            .merge(&transformed)
            .expect("litter variants share one vertex contract");
    } else {
        *slot = Some(transformed);
    }
}

pub(super) const DRY_LEAF_MESH_VARIANTS: u64 = 4;
pub(super) const TWIG_MESH_VARIANTS: u64 = 3;
pub(super) const WOODLAND_PLANT_MESH_VARIANTS: u64 = 3;
pub(super) fn forest_floor_leaf_material(
    assets: &ProceduralEnvironmentAssets,
) -> TacticalTreeLeafCardMaterial {
    let mut material = leaf_material(&assets.dry_oak_leaf, 0.28, 0.72, 0.0, 0.035);
    // Fallen leaves reuse the oak surface maps/PBR response but do not inherit
    // canopy wind displacement. NotShadowCaster on every litter entity keeps
    // their dense alpha geometry out of the shadow pass.
    material.parameters.z = 0.0;
    material.surface_parameters.z = 0.0;
    material.surface_parameters.w = 0.035;
    material.physical_parameters.x = 0.96;
    material.physical_parameters.y = 0.00035;
    material
}

fn forest_floor_patch_transform(
    terrain: &SceneTerrain,
    ground: &SceneGround,
    cell_origin: Vec2,
    hash: u64,
    scale: f32,
    height_offset: f32,
) -> Option<Transform> {
    let jitter = ground.grid_scale() * 0.78;
    let position = cell_origin
        + Vec2::new(
            unit_hash(splitmix64(hash ^ 0x672a_1f04)) - 0.5,
            unit_hash(splitmix64(hash ^ 0xeeb0_31cd)) - 0.5,
        ) * jitter;
    if ground.ground_at(position).is_none_or(|sample| {
        !matches!(
            sample.cover,
            GroundCover::LeafLitter | GroundCover::TallGrass
        )
    }) {
        return None;
    }
    let mut transform = foliage_transform(terrain, position.x, position.y, hash)?;
    transform.translation.y += height_offset;
    transform.scale *= scale;
    Some(transform)
}
pub(super) fn dry_leaf_patch_mesh(variant: u64) -> Mesh {
    let mut data = GroundLitterMeshData::default();
    let leaf_colors = [
        Color::srgb_u8(190, 132, 61),
        Color::srgb_u8(139, 82, 43),
        Color::srgb_u8(202, 164, 81),
        Color::srgb_u8(104, 72, 48),
        Color::srgb_u8(157, 126, 74),
    ];
    let cluster_count = 4;
    let clusters = (0..cluster_count)
        .map(|cluster| {
            let hash = splitmix64(variant.rotate_left(17) ^ cluster as u64 ^ 0x5a9d_31c4);
            Vec2::new(unit_hash(hash) - 0.5, unit_hash(splitmix64(hash ^ 1)) - 0.5) * 0.68
        })
        .collect::<Vec<_>>();
    for leaf in 0..24_u64 {
        let hash = splitmix64(leaf ^ variant.rotate_left(29) ^ 0x5ec4_57d2_bf90_1c37);
        let cluster_angle = unit_hash(splitmix64(hash ^ 0x43)) * core::f32::consts::TAU;
        let centre = if leaf < 20 {
            let cluster = clusters[leaf as usize % clusters.len()];
            let radial = 0.04 + unit_hash(splitmix64(hash ^ 0x42)) * 0.14;
            cluster + Vec2::new(cluster_angle.cos(), cluster_angle.sin()) * radial
        } else {
            Vec2::new(cluster_angle.cos(), cluster_angle.sin())
                * (0.36 + unit_hash(splitmix64(hash ^ 0x42)) * 0.16)
        };
        let angle = unit_hash(splitmix64(hash ^ 2)) * core::f32::consts::TAU;
        let long =
            Vec2::new(angle.cos(), angle.sin()) * (0.065 + unit_hash(splitmix64(hash ^ 3)) * 0.045);
        let side = Vec2::new(-long.y, long.x) * (0.34 + unit_hash(splitmix64(hash ^ 4)) * 0.16);
        data.append_cambered_leaf(
            centre,
            long,
            side,
            if leaf < 20 {
                [0.0, 0.0025, 0.005][(leaf as usize / 4) % 3]
            } else {
                0.0
            },
            hash,
            leaf_colors[leaf as usize % leaf_colors.len()],
        );
    }
    data.into_mesh()
}

/// Independent twig mesh. Longer, thinner pieces and a lower spawn density
/// let twigs form irregular accents over the denser dry-leaf carpet.
pub(super) fn twig_patch_mesh(variant: u64) -> Mesh {
    let mut data = GroundLitterMeshData::default();
    let twig_colors = [
        Color::srgb_u8(79, 47, 24),
        Color::srgb_u8(102, 62, 29),
        Color::srgb_u8(62, 40, 25),
    ];
    for twig in 0..9_u64 {
        let hash = splitmix64(twig ^ variant.rotate_left(31) ^ 0xa773_9fe2_410c_862d);
        let centre = Vec2::new(unit_hash(hash) - 0.5, unit_hash(splitmix64(hash ^ 1)) - 0.5) * 1.02;
        let angle = unit_hash(splitmix64(hash ^ 2)) * core::f32::consts::TAU;
        let long =
            Vec2::new(angle.cos(), angle.sin()) * (0.075 + unit_hash(splitmix64(hash ^ 3)) * 0.07);
        let lateral = Vec2::new(-long.y, long.x).normalize_or_zero();
        let start = Vec3::new(centre.x - long.x, -0.004, centre.y - long.y);
        let bend = (unit_hash(splitmix64(hash ^ 4)) - 0.5) * 0.055;
        let middle = Vec3::new(
            centre.x + lateral.x * bend,
            0.002 + unit_hash(splitmix64(hash ^ 0x44)) * 0.004,
            centre.y + lateral.y * bend,
        );
        let end = Vec3::new(centre.x + long.x, -0.006, centre.y + long.y);
        let sides = 5 + (hash % 2) as u32;
        let radius = 0.006 + unit_hash(splitmix64(hash ^ 5)) * 0.005;
        let color = twig_colors[twig as usize % twig_colors.len()];
        data.append_bent_twig(
            start,
            middle,
            end,
            radius,
            radius * 0.58,
            radius * 0.08,
            sides,
            true,
            centre,
            color,
        );
        if twig < 2 && unit_hash(splitmix64(hash ^ 6)) > 0.46 {
            let attach = middle.lerp(end, 0.22);
            let direction = (end - middle).normalize();
            let lateral = Vec3::new(-direction.z, 0.12, direction.x).normalize();
            let fork_end = attach
                + (direction * 0.38 + lateral * if hash & 1 == 0 { 0.62 } else { -0.62 })
                    .normalize()
                    * long.length()
                    * 0.72;
            let fork_middle = attach.lerp(fork_end, 0.52) + Vec3::Y * 0.002;
            data.append_bent_twig(
                attach,
                fork_middle,
                fork_end,
                radius * 0.55,
                radius * 0.3,
                radius * 0.05,
                sides,
                false,
                centre,
                color,
            );
        }
    }
    data.into_mesh()
}

/// A sparse shade-floor rosette. Seven-vertex cambered leaves provide a
/// readable close silhouette without introducing a textured albedo or the
/// single-triangle markers formerly used for tiny meadow accents.
pub(super) fn woodland_plant_patch_mesh(variant: u64) -> Mesh {
    let mut data = GroundLitterMeshData::default();
    let palette = [
        Color::srgb_u8(48, 79, 35),
        Color::srgb_u8(57, 91, 40),
        Color::srgb_u8(40, 68, 31),
    ];
    let plant_count = 2 + (variant % 2) as u64;
    for plant in 0..plant_count {
        let plant_hash = splitmix64(variant.rotate_left(23) ^ plant ^ 0x91e4_3bc7);
        let centre = Vec2::new(
            unit_hash(plant_hash) - 0.5,
            unit_hash(splitmix64(plant_hash ^ 1)) - 0.5,
        ) * 0.62;
        let leaf_count = 5 + (plant_hash % 3) as u64;
        let phase = unit_hash(splitmix64(plant_hash ^ 2)) * core::f32::consts::TAU;
        for leaf in 0..leaf_count {
            let hash = splitmix64(plant_hash ^ leaf.rotate_left(17));
            let angle = phase
                + leaf as f32 * core::f32::consts::TAU / leaf_count as f32
                + (unit_hash(hash) - 0.5) * 0.28;
            let length = 0.11 + unit_hash(splitmix64(hash ^ 3)) * 0.075;
            let width = length * (0.19 + unit_hash(splitmix64(hash ^ 4)) * 0.08);
            data.append_rosette_leaf(
                centre,
                Vec2::new(angle.cos(), angle.sin()),
                length,
                width,
                0.055 + unit_hash(splitmix64(hash ^ 5)) * 0.055,
                palette[(plant as usize + leaf as usize) % palette.len()],
            );
        }
    }
    data.into_mesh()
}

impl GroundLitterMeshData {
    fn append_rosette_leaf(
        &mut self,
        root: Vec2,
        direction: Vec2,
        length: f32,
        width: f32,
        rise: f32,
        color: Color,
    ) {
        let base = self.positions.len() as u32;
        let side = Vec2::new(-direction.y, direction.x);
        let centre = |along: f32, lateral: f32, height: f32| {
            let point = root + direction * (length * along) + side * (width * lateral);
            Vec3::new(point.x, height, point.y)
        };
        let positions = [
            centre(0.0, -0.18, 0.002),
            centre(0.0, 0.18, 0.002),
            centre(0.38, -1.0, rise * 0.72),
            centre(0.38, 1.0, rise * 0.72),
            centre(0.76, -0.62, rise),
            centre(0.76, 0.62, rise),
            centre(1.0, 0.0, rise * 0.82),
        ];
        let normal = Vec3::new(-direction.x * 0.24, 0.94, -direction.y * 0.24).normalize();
        let linear_color = color.to_linear().to_f32_array();
        for (index, position) in positions.into_iter().enumerate() {
            self.positions.push(position.to_array());
            self.normals.push(normal.to_array());
            self.uvs.push([
                if index % 2 == 0 { 0.0 } else { 1.0 },
                [0.0, 0.0, 0.38, 0.38, 0.76, 0.76, 1.0][index],
            ]);
            self.roots.push(root.to_array());
            self.colors.push(linear_color);
        }
        self.indices.extend_from_slice(&[
            base,
            base + 2,
            base + 1,
            base + 1,
            base + 2,
            base + 3,
            base + 2,
            base + 4,
            base + 3,
            base + 3,
            base + 4,
            base + 5,
            base + 4,
            base + 6,
            base + 5,
        ]);
    }

    #[allow(clippy::too_many_arguments)]
    fn append_bent_twig(
        &mut self,
        start: Vec3,
        middle: Vec3,
        end: Vec3,
        start_radius: f32,
        middle_radius: f32,
        end_radius: f32,
        sides: u32,
        cap_start: bool,
        root: Vec2,
        color: Color,
    ) {
        let base = self.positions.len() as u32;
        let direction = (end - start).normalize();
        let reference = if direction.y.abs() < 0.9 {
            Vec3::Y
        } else {
            Vec3::X
        };
        let right = direction.cross(reference).normalize();
        let forward = right.cross(direction).normalize();
        let linear_color = color.to_linear().to_f32_array();
        let near_tip = middle.lerp(end, 0.86);
        let late_tip = middle.lerp(end, 0.97);
        for (ring, (centre, radius)) in [
            (start, start_radius),
            (middle, middle_radius),
            (near_tip, middle_radius.lerp(end_radius, 0.72)),
            (late_tip, end_radius),
        ]
        .into_iter()
        .enumerate()
        {
            for side_index in 0..sides {
                let phase = side_index as f32 * core::f32::consts::TAU / sides as f32;
                let normal = right * phase.cos() + forward * phase.sin();
                self.positions.push((centre + normal * radius).to_array());
                self.normals.push(normal.to_array());
                self.uvs
                    .push([side_index as f32 / sides as f32, ring as f32 / 3.0]);
                self.roots.push(root.to_array());
                self.colors.push(linear_color);
            }
        }
        for ring in 0..3_u32 {
            let from = base + ring * sides;
            let to = from + sides;
            for side_index in 0..sides {
                let next = (side_index + 1) % sides;
                self.indices.extend_from_slice(&[
                    from + side_index,
                    to + side_index,
                    to + next,
                    from + side_index,
                    to + next,
                    from + next,
                ]);
            }
        }
        if cap_start {
            let cap = self.positions.len() as u32;
            self.positions.push(start.to_array());
            self.normals.push((-direction).to_array());
            self.uvs.push([0.5, 0.0]);
            self.roots.push(root.to_array());
            self.colors.push(linear_color);
            for side_index in 0..sides {
                let next = (side_index + 1) % sides;
                self.indices
                    .extend_from_slice(&[cap, base + side_index, base + next]);
            }
        }
        let apex = self.positions.len() as u32;
        self.positions.push(end.to_array());
        self.normals.push(direction.to_array());
        self.uvs.push([0.5, 1.0]);
        self.roots.push(root.to_array());
        self.colors.push(linear_color);
        let tip_ring = base + sides * 3;
        for side_index in 0..sides {
            let next = (side_index + 1) % sides;
            self.indices
                .extend_from_slice(&[tip_ring + side_index, apex, tip_ring + next]);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.roots);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

#[derive(Default)]
struct GroundLitterMeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    roots: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl GroundLitterMeshData {
    fn append_cambered_leaf(
        &mut self,
        centre: Vec2,
        long: Vec2,
        side: Vec2,
        height: f32,
        seed: u64,
        color: Color,
    ) {
        let base = self.positions.len() as u32;
        // Fallen leaves should curl without becoming little tents. Build the
        // varied plate first, then seat its lowest vertex just below the local
        // patch ground plane so every instance visibly makes contact.
        let long_slope = (unit_hash(splitmix64(seed ^ 0x11)) - 0.5) * 0.12;
        let side_slope = (unit_hash(splitmix64(seed ^ 0x12)) - 0.5) * 0.08;
        let camber = 0.003 + unit_hash(splitmix64(seed ^ 0x13)) * 0.008;
        let curl = (unit_hash(splitmix64(seed ^ 0x14)) - 0.5) * 0.007;
        let burial = 0.0007 + height.min(0.006) * 0.15 + unit_hash(splitmix64(seed ^ 0x15)) * 0.001;
        let long3 = Vec3::new(long.x, long_slope * long.length(), long.y);
        let side3 = Vec3::new(side.x, side_slope * side.length(), side.y);
        let centre3 = Vec3::new(centre.x, 0.0, centre.y);
        let outline = [
            (0.0, -1.0),
            (0.82, -0.55),
            (1.0, 0.0),
            (0.74, 0.58),
            (0.0, 1.0),
            (-0.74, 0.58),
            (-1.0, 0.0),
            (-0.82, -0.55),
        ];
        let mut leaf_positions = Vec::with_capacity(9);
        leaf_positions.push(centre3 + Vec3::Y * camber);
        for (u, v) in outline {
            let lift = camber * (1.0 - u * u) * (1.0 - v * v) + curl * v * v;
            leaf_positions.push(centre3 + long3 * v + side3 * u + Vec3::Y * lift);
        }
        let minimum_y = leaf_positions
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        for point in &mut leaf_positions {
            point.y += height - minimum_y - burial;
        }
        let mut leaf_normals = vec![Vec3::ZERO; 9];
        for outline_index in 0..8_usize {
            let left = 1 + outline_index;
            let right = 1 + (outline_index + 1) % 8;
            let face = (leaf_positions[right] - leaf_positions[0])
                .cross(leaf_positions[left] - leaf_positions[0]);
            leaf_normals[0] += face;
            leaf_normals[left] += face;
            leaf_normals[right] += face;
            self.indices
                .extend_from_slice(&[base, base + right as u32, base + left as u32]);
        }
        for (index, point) in leaf_positions.into_iter().enumerate() {
            self.positions.push(point.to_array());
            self.normals
                .push(leaf_normals[index].normalize().to_array());
            self.roots.push(centre.to_array());
        }
        self.uvs.push([0.5, 0.5]);
        self.uvs
            .extend(outline.map(|(u, v)| [0.5 + u * 0.5, 0.5 + v * 0.5]));
        let color = color.to_linear().to_f32_array();
        self.colors.extend_from_slice(&[color; 9]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::obstacles::tree::oak_leaf_material;
    use adventuresim_tactical_core::prelude::GroundSurface;
    use bevy::{
        asset::{AssetApp, AssetPlugin},
        mesh::VertexAttributeValues,
        prelude::{App, Image, TaskPoolPlugin, default},
    };

    #[test]
    fn woodland_floor_transition_decays_outward_from_litter() {
        let mut samples = vec![
            GroundSurface {
                cover: GroundCover::TallGrass,
                cover_density_bps: 10_000,
                cover_height_cm: 82,
                ..default()
            };
            9 * 9
        ];
        samples[4 * 9 + 4].cover = GroundCover::LeafLitter;
        let ground = SceneGround::from_samples(9, 9, 1.0, samples).unwrap();
        let edge = leaf_litter_proximity(&ground, 5, 4);
        let middle = leaf_litter_proximity(&ground, 6, 4);
        let outside = leaf_litter_proximity(&ground, 8, 4);
        assert!(1.0 > edge && edge > middle && middle > outside);
        assert_eq!(outside, 0.0);
    }

    #[test]
    fn forest_floor_meshes_are_deterministic_bounded_and_volumetric() {
        let leaves = dry_leaf_patch_mesh(0);
        let repeated_leaves = dry_leaf_patch_mesh(0);
        let alternate_leaves = dry_leaf_patch_mesh(1);
        let twigs = twig_patch_mesh(0);
        let repeated_twigs = twig_patch_mesh(0);
        let plants = woodland_plant_patch_mesh(0);
        let repeated_plants = woodland_plant_patch_mesh(0);
        let leaf_positions = leaves
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let twig_positions = twigs
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let plant_positions = plants
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let leaf_normals = leaves
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let twig_normals = twigs
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(leaf_positions.len(), 24 * 9);
        assert_eq!(leaves.indices().unwrap().len() / 3, 24 * 8);
        assert!((190..=300).contains(&twig_positions.len()));
        assert!((360..=520).contains(&(twigs.indices().unwrap().len() / 3)));
        assert_eq!(
            leaves.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_leaves.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            twigs.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_twigs.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            plants.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_plants.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_ne!(
            leaves.attribute(Mesh::ATTRIBUTE_POSITION),
            alternate_leaves.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        for mesh in [&leaves, &twigs, &plants] {
            assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
            assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_1).is_some());
        }
        assert!(
            leaf_normals
                .iter()
                .all(|normal| Vec3::from_array(*normal).is_normalized())
        );
        assert!(
            twig_normals
                .iter()
                .all(|normal| Vec3::from_array(*normal).is_normalized())
        );
        assert!(
            leaf_normals
                .iter()
                .any(|normal| Vec3::from_array(*normal).distance(Vec3::Y) > 0.03)
        );
        let mut contacting_leaves = 0;
        let leaf_spans = leaf_positions
            .chunks_exact(9)
            .map(|leaf| {
                let minimum = leaf
                    .iter()
                    .map(|point| point[1])
                    .fold(f32::INFINITY, f32::min);
                let maximum = leaf
                    .iter()
                    .map(|point| point[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                contacting_leaves += usize::from(minimum <= -0.0006);
                assert!(maximum <= 0.025, "leaf lift must stay bounded: {maximum}");
                maximum - minimum
            })
            .collect::<Vec<_>>();
        assert!(
            contacting_leaves >= 8,
            "each loose pile needs seated base leaves"
        );
        assert!(leaf_spans.iter().all(|span| *span > 0.003));
        assert!(
            leaf_spans
                .windows(2)
                .any(|pair| (pair[0] - pair[1]).abs() > 0.001)
        );
        let Some(VertexAttributeValues::Float32x2(leaf_uvs)) =
            leaves.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("fallen-leaf UVs must use Float32x2 storage");
        };
        assert!(leaf_uvs.contains(&[0.5, 0.0]));
        assert!(leaf_uvs.contains(&[1.0, 0.5]));
        assert!(leaf_uvs.contains(&[0.5, 1.0]));
        assert!(leaf_uvs.contains(&[0.0, 0.5]));
        assert!(!leaf_uvs.contains(&[0.0, 0.0]));
        assert!(!leaf_uvs.contains(&[1.0, 1.0]));
        let leaf_indices = leaves.indices().unwrap().iter().collect::<Vec<_>>();
        for triangle in leaf_indices.chunks_exact(3) {
            let a = Vec3::from_array(leaf_positions[triangle[0] as usize]);
            let b = Vec3::from_array(leaf_positions[triangle[1] as usize]);
            let c = Vec3::from_array(leaf_positions[triangle[2] as usize]);
            let average_normal = (Vec3::from_array(leaf_normals[triangle[0] as usize])
                + Vec3::from_array(leaf_normals[triangle[1] as usize])
                + Vec3::from_array(leaf_normals[triangle[2] as usize]))
            .normalize();
            assert!((b - a).cross(c - a).dot(average_normal) > 0.0);
        }
        for positions in [leaf_positions, twig_positions, plant_positions] {
            assert!(positions.iter().flatten().all(|value| value.is_finite()));
        }
        assert!(
            leaf_positions
                .iter()
                .all(|point| point[0].abs() < 0.7 && point[2].abs() < 0.7)
        );
        assert!(
            twig_positions
                .iter()
                .all(|point| point[0].abs() < 0.9 && point[2].abs() < 0.9)
        );
        assert!((70..=150).contains(&plant_positions.len()));
        assert!(plant_positions.iter().any(|point| point[1] > 0.08));
        let leaf_height_bounds = leaf_positions.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), point| (minimum.min(point[1]), maximum.max(point[1])),
        );
        assert!(
            leaf_height_bounds.0 >= -0.05 && leaf_height_bounds.1 <= 0.06,
            "fallen leaf height bounds: {leaf_height_bounds:?}"
        );
        assert!(
            twig_positions
                .iter()
                .all(|point| (-0.02..0.20).contains(&point[1]))
        );
    }

    #[test]
    fn forest_floor_leaves_use_dry_oak_palette_and_surface_contract() {
        let mut app = App::new();
        app.add_plugins(TaskPoolPlugin::default());
        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Image>();
        let assets = generate_procedural_environment_assets(
            &mut app
                .world_mut()
                .resource_mut::<bevy::prelude::Assets<Image>>(),
        );
        let floor = forest_floor_leaf_material(&assets);
        let oak = oak_leaf_material(&assets);
        assert_eq!(floor.opacity, assets.dry_oak_leaf.opacity);
        assert_ne!(floor.front_albedo, oak.front_albedo);
        assert_ne!(floor.back_albedo, oak.back_albedo);
        assert_eq!(floor.arm, assets.dry_oak_leaf.arm);
        assert_eq!(floor.parameters.z, 0.0);
        assert_eq!(floor.surface_parameters.z, 0.0);
        assert!(floor.surface_parameters.w < oak.surface_parameters.w * 0.2);
        assert!(floor.physical_parameters.y < oak.physical_parameters.y);
        let shader = include_str!("../../../../../assets/shaders/tactical_tree_leaf_card.wgsl");
        assert!(shader.contains("pbr_input.material.base_color = vec4<f32>("));
        assert!(shader.contains("albedo,"));
        assert!(!shader.contains("spatial_hue"));
    }

    #[test]
    fn fallen_leaf_vertex_pigments_are_dry_warm_and_varied() {
        let leaves = dry_leaf_patch_mesh(0);
        let Some(VertexAttributeValues::Float32x4(colors)) =
            leaves.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("fallen-leaf pigments must use Float32x4 storage");
        };
        assert!(colors.iter().all(|color| {
            color[0] > color[1] && color[1] > color[2] && color[0] - color[2] < 0.58
        }));
        let pigments = colors
            .chunks_exact(9)
            .map(|leaf| leaf[0])
            .collect::<Vec<_>>();
        assert!(pigments.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn twig_variants_have_exact_bounded_topology_and_only_fork_base_boundaries() {
        for variant in 0..TWIG_MESH_VARIANTS {
            let mesh = twig_patch_mesh(variant);
            let mut expected_vertices = 0;
            let mut expected_triangles = 0;
            let mut expected_boundaries = 0;
            for twig in 0..9_u64 {
                let hash = splitmix64(twig ^ variant.rotate_left(31) ^ 0xa773_9fe2_410c_862d);
                let sides = 5 + (hash % 2) as usize;
                expected_vertices += sides * 4 + 2;
                expected_triangles += sides * 8;
                if twig < 2 && unit_hash(splitmix64(hash ^ 6)) > 0.46 {
                    expected_vertices += sides * 4 + 1;
                    expected_triangles += sides * 7;
                    expected_boundaries += sides;
                }
            }
            assert_eq!(mesh.count_vertices(), expected_vertices);
            assert_eq!(mesh.indices().unwrap().len() / 3, expected_triangles);
            let mut edges = std::collections::BTreeMap::new();
            let indices = mesh.indices().unwrap().iter().collect::<Vec<_>>();
            for triangle in indices.chunks_exact(3) {
                for edge in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    *edges
                        .entry(if edge.0 < edge.1 {
                            edge
                        } else {
                            (edge.1, edge.0)
                        })
                        .or_insert(0) += 1;
                }
            }
            assert!(edges.values().all(|count| *count <= 2));
            assert_eq!(
                edges.values().filter(|count| **count == 1).count(),
                expected_boundaries
            );
        }
    }
}
