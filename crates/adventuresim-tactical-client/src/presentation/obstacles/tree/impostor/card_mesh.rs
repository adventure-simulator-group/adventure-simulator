use super::super::geometry::{TreeBranchSegment, tree_crown_bounds};
use crate::presentation::{splitmix64, unit_hash};
use adventuresim_tactical_core::prelude::TREE_TRUNK_HEIGHT_METRES;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::{Mesh, Vec2, Vec3},
};

pub(super) fn append_tree_card_with_uv(
    center: Vec3,
    right: Vec3,
    up: Vec3,
    width: f32,
    height: f32,
    uv_min: Vec2,
    uv_max: Vec2,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let right = right.normalize() * width * 0.5;
    let up = up.normalize() * height * 0.5;
    let normal = right.cross(up).normalize_or_zero();
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        (center - right - up).to_array(),
        (center + right - up).to_array(),
        (center + right + up).to_array(),
        (center - right + up).to_array(),
    ]);
    normals.extend_from_slice(&[normal.to_array(); 4]);
    uvs.extend_from_slice(&[
        [uv_min.x, uv_max.y],
        [uv_max.x, uv_max.y],
        [uv_max.x, uv_min.y],
        [uv_min.x, uv_min.y],
    ]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[allow(dead_code)]
pub(super) fn procedural_tree_card_mesh(
    seed: u64,
    branches: &[TreeBranchSegment],
    lod: u8,
) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    match lod {
        0 => {
            for (index, branch) in branches
                .iter()
                .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
                .enumerate()
            {
                let direction = (branch.end - branch.start).normalize();
                let tangent = direction.cross(Vec3::Y).normalize_or_zero();
                let tangent = if tangent.length_squared() < 0.5 {
                    Vec3::X
                } else {
                    tangent
                };
                let binormal = direction.cross(tangent).normalize();
                for leaf in 0..13_u64 {
                    let leaf_seed =
                        splitmix64(seed ^ index as u64 ^ leaf.wrapping_mul(0x91e1_0da5));
                    let phase = unit_hash(leaf_seed) * 0.65 + leaf as f32 * 2.399_963_1;
                    let radial = (tangent * phase.cos() + binormal * phase.sin()).normalize();
                    let along = 0.1 + unit_hash(leaf_seed ^ 8) * 0.88;
                    let leaf_axis = (radial * 0.78
                        + direction * 0.22
                        + Vec3::Y * (0.22 - unit_hash(leaf_seed ^ 4) * 0.44))
                        .normalize();
                    let right = direction.cross(leaf_axis).normalize_or_zero();
                    let right = if right.length_squared() < 0.5 {
                        tangent
                    } else {
                        right
                    };
                    let center = branch.start.lerp(branch.end, along.min(0.98))
                        + radial * (0.055 + unit_hash(leaf_seed ^ 5) * 0.11)
                        + Vec3::Y * ((unit_hash(leaf_seed ^ 9) - 0.5) * 0.16);
                    append_tree_card(
                        center,
                        right,
                        leaf_axis,
                        0.15 + unit_hash(leaf_seed ^ 6) * 0.055,
                        0.24 + unit_hash(leaf_seed ^ 7) * 0.07,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        1 => {
            for group in 0..60_u16 {
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3 && branch.secondary_group == group
                });
                let up = branches
                    .iter()
                    .find(|branch| {
                        branch.depth == 2 && branch.secondary_group == group && branch.is_limb_tip
                    })
                    .map(|branch| (branch.end - branch.start).normalize())
                    .unwrap_or(Vec3::Y);
                let index = usize::from(group);
                let phase = unit_hash(splitmix64(seed ^ index as u64)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center() + Vec3::Y * 0.05,
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        up,
                        (bounds.horizontal_span() + 0.45) * 1.05,
                        (bounds.vertical_span() + 0.5) * 1.08,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        2 => {
            for group in 0..10_u8 {
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3 && branch.primary_group == group
                });
                let index = usize::from(group);
                let phase =
                    unit_hash(splitmix64(seed ^ index as u64 ^ 0x4a17)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center(),
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        Vec3::Y,
                        (bounds.horizontal_span() + 0.9) * 1.28,
                        (bounds.vertical_span() + 0.9) * 1.18,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        3 => {
            for index in 0..5 {
                let first = index as u8;
                let second = (index + 5) as u8;
                let bounds = tree_crown_bounds(branches, |branch| {
                    branch.depth == 3
                        && (branch.primary_group == first || branch.primary_group == second)
                });
                let phase =
                    unit_hash(splitmix64(seed ^ index as u64 ^ 0x7c31)) * core::f32::consts::TAU;
                for facing in 0..2 {
                    let facing_phase = phase + facing as f32 * core::f32::consts::FRAC_PI_2;
                    append_tree_card(
                        bounds.center() + Vec3::Y * 0.08,
                        Vec3::new(facing_phase.cos(), 0.0, facing_phase.sin()),
                        Vec3::Y,
                        (bounds.horizontal_span() + 1.0) * 1.2,
                        (bounds.vertical_span() + 1.0) * 1.15,
                        &mut positions,
                        &mut normals,
                        &mut uvs,
                        &mut indices,
                    );
                }
            }
        }
        4 => {
            let bounds = tree_crown_bounds(branches, |branch| branch.depth == 3);
            let bottom = -TREE_TRUNK_HEIGHT_METRES * 0.5;
            let top = bounds.maximum.y + 0.45;
            append_tree_card(
                Vec3::new(0.0, (bottom + top) * 0.5, 0.0),
                Vec3::X,
                Vec3::Y,
                (bounds.horizontal_span() + 1.25) * 1.15,
                top - bottom,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
            );
        }
        _ => unreachable!("tree LOD is bounded"),
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_tree_card(
    center: Vec3,
    right: Vec3,
    up: Vec3,
    width: f32,
    height: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let right = right.normalize() * width * 0.5;
    let up = up.normalize() * height * 0.5;
    let normal = right.cross(up).normalize_or_zero();
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        (center - right - up).to_array(),
        (center + right - up).to_array(),
        (center + right + up).to_array(),
        (center - right + up).to_array(),
    ]);
    normals.extend_from_slice(&[normal.to_array(); 4]);
    uvs.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}
