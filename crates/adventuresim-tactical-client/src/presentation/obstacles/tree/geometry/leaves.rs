use bevy::{
    asset::RenderAssetUsages,
    math::{Quat, Vec3, Vec3Swizzles},
    mesh::{Indices, PrimitiveTopology},
    prelude::Mesh,
};

use crate::presentation::{splitmix64, unit_hash};

use super::{TreeBranchSegment, WoodyPlantForm, WoodyPlantParameters, branch_frame};

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct TreeLeaf {
    pub(in crate::presentation) petiole_start: Vec3,
    pub(in crate::presentation) center: Vec3,
    pub(in crate::presentation) right: Vec3,
    pub(in crate::presentation) up: Vec3,
    pub(in crate::presentation) length: f32,
    pub(in crate::presentation) width: f32,
    pub(in crate::presentation) primary_group: u8,
    pub(in crate::presentation) secondary_group: u16,
    pub(in crate::presentation) shoot_id: u16,
    /// Stable ordinal within the source shoot. This is deliberately stored on
    /// the leaf rather than inferred from a mesh-building iteration order so
    /// representation LODs can make deterministic per-leaf choices.
    pub(in crate::presentation) leaf_ordinal: u8,
    pub(in crate::presentation) shade: f32,
    /// Rotation accumulated from the base to the tip of the blade. Real oak
    /// leaves rarely present as perfectly planar cards, even in still air.
    pub(in crate::presentation) torsion: f32,
}

pub(in crate::presentation) fn procedural_oak_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
    canopy_competition: f32,
) -> Vec<TreeLeaf> {
    let mut leaves = Vec::new();
    let _ = canopy_competition;
    // A compact terminal flush gives each shoot one readable foliage mass.
    // The pulsed shoot layout then cuts redundant alpha overlap while
    // retaining the oak's irregular shell.
    let leaves_per_shoot = 16_u64;
    for (shoot_index, shoot) in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
        .enumerate()
    {
        let direction = (shoot.end - shoot.start).normalize();
        let tangent = direction.cross(Vec3::Y).normalize_or_zero();
        let tangent = if tangent.length_squared() < 0.25 {
            Vec3::X
        } else {
            tangent
        };
        let binormal = direction.cross(tangent).normalize();
        for leaf_index in 0..leaves_per_shoot {
            let leaf_seed =
                splitmix64(seed ^ shoot_index as u64 ^ leaf_index.wrapping_mul(0x91e1_0da5));
            // Alternate leaves along each current-year shoot, then finish in
            // the tighter terminal flush characteristic of pedunculate oak.
            let along = if leaf_index < 2 {
                0.12 + leaf_index as f32 * 0.28
            } else {
                (0.70
                    + (leaf_index - 2) as f32 / (leaves_per_shoot - 3) as f32 * 0.285
                    + (unit_hash(leaf_seed ^ 12) - 0.5) * 0.008)
                    .clamp(0.69, 0.99)
            };
            let alternate = if leaf_index & 1 == 0 { 1.0 } else { -1.0 };
            let spiral = leaf_index as f32 * 2.399_963_1 + (unit_hash(leaf_seed ^ 2) - 0.5) * 0.65;
            let radial = (tangent * spiral.cos() + binormal * spiral.sin()).normalize();
            let leaf_up = (radial * (0.46 + unit_hash(leaf_seed ^ 3) * 0.24)
                + direction * (0.42 + unit_hash(leaf_seed ^ 8) * 0.18)
                + Vec3::Y * (0.08 + unit_hash(leaf_seed ^ 9) * 0.18))
                .normalize();
            let leaf_normal_candidate = direction.cross(radial)
                + radial * (unit_hash(leaf_seed ^ 10) - 0.5) * 0.7
                + Vec3::Y * (unit_hash(leaf_seed ^ 11) - 0.5) * 0.35;
            let leaf_normal = if leaf_normal_candidate.length_squared() > 0.001 {
                leaf_normal_candidate.normalize()
            } else {
                branch_frame(leaf_up).1
            };
            let right_candidate = leaf_up.cross(leaf_normal);
            let leaf_right = if right_candidate.length_squared() > 0.001 {
                right_candidate.normalize()
            } else {
                branch_frame(leaf_up).0 * alternate
            };
            let petiole_start = shoot.start.lerp(shoot.end, along.min(0.98));
            // Pedunculate-oak leaf stalks are only a few millimetres long.
            // Keeping this independent of blade size avoids the long-stalked,
            // bilateral-comb silhouette of other broadleaf genera.
            let petiole_length = 0.003 + unit_hash(leaf_seed ^ 4) * 0.004;
            let blade_base =
                petiole_start + (radial * 0.82 + leaf_up * 0.18).normalize() * petiole_length;
            let leaf_length = 0.1 + unit_hash(leaf_seed ^ 5) * 0.06;
            let leaf_width = 0.065 + unit_hash(leaf_seed ^ 6) * 0.04;
            let shell_exposure = ((shoot.end.xz().length() - 1.25) / 4.75).clamp(0.0, 1.0);
            let shade = if leaf_index < 2 {
                0.58 + shell_exposure * 0.22 + unit_hash(leaf_seed ^ 7) * 0.12
            } else {
                0.68 + shell_exposure * 0.25 + unit_hash(leaf_seed ^ 7) * 0.14
            };
            leaves.push(TreeLeaf {
                petiole_start,
                center: blade_base + leaf_up * leaf_length * 0.5,
                right: leaf_right.normalize(),
                up: leaf_up,
                length: leaf_length,
                width: leaf_width,
                primary_group: shoot.primary_group,
                secondary_group: shoot.secondary_group,
                shoot_id: shoot_index as u16,
                leaf_ordinal: leaf_index as u8,
                shade,
                torsion: (unit_hash(leaf_seed ^ 13) - 0.5) * 0.42,
            });
        }
    }
    leaves
}

pub(in crate::presentation) fn procedural_woody_plant_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
    canopy_competition: f32,
    parameters: WoodyPlantParameters,
) -> Vec<TreeLeaf> {
    match parameters.form {
        WoodyPlantForm::MatureOak => procedural_oak_leaves(seed, branches, canopy_competition),
        WoodyPlantForm::MatureBeech => procedural_beech_leaves(seed, branches, parameters),
        WoodyPlantForm::MultiStemShrub => {
            procedural_multistem_shrub_leaves(seed, branches, parameters)
        }
    }
}

/// Arranges beech leaves in overlapping, approximately two-ranked sprays.
/// The broad, mostly horizontal leaf planes build a closed shade-casting crown
/// from the existing twig and leaf budget instead of filling it by count.
fn procedural_beech_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
    parameters: WoodyPlantParameters,
) -> Vec<TreeLeaf> {
    let mut leaves = Vec::new();
    let leaves_per_shoot = u64::from(parameters.leaves_per_shoot.max(4));
    for (shoot_index, shoot) in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
        .enumerate()
    {
        let direction = (shoot.end - shoot.start).normalize();
        let horizontal = Vec3::new(direction.x, 0.0, direction.z).normalize_or_zero();
        let spray_forward = if horizontal.length_squared() > 0.25 {
            horizontal
        } else {
            branch_frame(direction).0
        };
        let spray_side = Vec3::Y.cross(spray_forward).normalize();
        for leaf_index in 0..leaves_per_shoot {
            let leaf_seed =
                splitmix64(seed ^ shoot_index as u64 ^ leaf_index.wrapping_mul(0x91e1_0da5));
            let along = (0.1
                + leaf_index as f32 / (leaves_per_shoot - 1) as f32 * 0.84
                + (unit_hash(leaf_seed ^ 1) - 0.5) * 0.018)
                .clamp(0.06, 0.97);
            let side = if leaf_index & 1 == 0 { 1.0 } else { -1.0 };
            let blade_outward = (spray_forward * (0.56 + unit_hash(leaf_seed ^ 2) * 0.18)
                + spray_side * side * (0.7 + unit_hash(leaf_seed ^ 3) * 0.16)
                + Vec3::Y * (0.06 + unit_hash(leaf_seed ^ 4) * 0.1))
                .normalize();
            // Keep the lamina close to horizontal while allowing a small,
            // deterministic ripple through each spray.
            let horizontal_weight = if leaf_index & 3 < 2 { 1.0 } else { 0.62 };
            let posture_normal = (Vec3::Y * horizontal_weight
                + spray_forward * (1.0 - horizontal_weight) * 1.35
                + spray_side * (unit_hash(leaf_seed ^ 5) - 0.5) * 0.28
                + spray_forward * (unit_hash(leaf_seed ^ 6) - 0.5) * 0.18)
                .normalize();
            let right = blade_outward.cross(posture_normal).normalize_or_zero();
            let right = if right.length_squared() > 0.25 {
                right
            } else {
                branch_frame(blade_outward).0
            };
            let petiole_start = shoot.start.lerp(shoot.end, along);
            let petiole_length = parameters.petiole_length_metres[0]
                + unit_hash(leaf_seed ^ 7)
                    * (parameters.petiole_length_metres[1] - parameters.petiole_length_metres[0]);
            // At mature-tree scale this card represents a small overlapping
            // beech spray, not a single isolated lamina. Keeping the card
            // count bounded while expanding the spray footprint produces the
            // species' closed, shade-casting crown without multiplying draw
            // and streaming work across a dense stand.
            let length = (parameters.leaf_length_metres[0]
                + unit_hash(leaf_seed ^ 8)
                    * (parameters.leaf_length_metres[1] - parameters.leaf_length_metres[0]))
                * 3.0;
            let width = length
                * (parameters.leaf_width_ratio[0]
                    + unit_hash(leaf_seed ^ 9)
                        * (parameters.leaf_width_ratio[1] - parameters.leaf_width_ratio[0]));
            let blade_base = petiole_start + blade_outward * petiole_length;
            leaves.push(TreeLeaf {
                petiole_start,
                center: blade_base + blade_outward * length * 0.5,
                right,
                up: blade_outward,
                length,
                width,
                primary_group: shoot.primary_group,
                secondary_group: shoot.secondary_group,
                shoot_id: shoot_index as u16,
                leaf_ordinal: leaf_index as u8,
                shade: 0.7 + unit_hash(leaf_seed ^ 10) * 0.2,
                torsion: (unit_hash(leaf_seed ^ 11) - 0.5) * 0.18,
            });
        }
    }
    leaves
}

fn procedural_multistem_shrub_leaves(
    seed: u64,
    branches: &[TreeBranchSegment],
    parameters: WoodyPlantParameters,
) -> Vec<TreeLeaf> {
    let mut leaves = Vec::new();
    let leaves_per_shoot = u64::from(parameters.leaves_per_shoot.max(4));
    for (shoot_index, shoot) in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
        .enumerate()
    {
        let direction = (shoot.end - shoot.start).normalize();
        let (frame_right, frame_up) = branch_frame(direction);
        for leaf_index in 0..leaves_per_shoot {
            let leaf_seed =
                splitmix64(seed ^ shoot_index as u64 ^ leaf_index.wrapping_mul(0x91e1_0da5));
            // Common hazel leaves are alternate and loosely distichous. The
            // golden-angle perturbation prevents a flat bilateral comb while
            // retaining opposite-side succession along each current shoot.
            let along = 0.08
                + leaf_index as f32 / (leaves_per_shoot - 1) as f32 * 0.84
                + (unit_hash(leaf_seed ^ 1) - 0.5) * 0.025;
            let side = if leaf_index & 1 == 0 { 1.0 } else { -1.0 };
            let phase = side * (0.82 + unit_hash(leaf_seed ^ 2) * 0.28) + leaf_index as f32 * 0.32;
            let radial = (frame_right * phase.cos() + frame_up * phase.sin()).normalize();
            let leaf_up = (radial * 0.72 + direction * 0.48 + Vec3::Y * 0.16).normalize();
            let azimuth_normal = direction.cross(radial).normalize_or_zero();
            let azimuth_normal = if azimuth_normal.length_squared() > 0.25 {
                azimuth_normal
            } else {
                frame_up
            };
            // Hazel blades are generally held obliquely upward rather than as
            // vertical fins around an upright shoot. Bias the generated plane
            // normal toward the sky while retaining azimuthal variation, then
            // project it perpendicular to the blade's midrib. This lets the
            // ordinary PBR response to the sun light the shrub naturally.
            let posture_normal = (Vec3::Y * 0.82 + azimuth_normal * 0.38).normalize();
            let right = leaf_up.cross(posture_normal).normalize_or_zero();
            let right = if right.length_squared() > 0.25 {
                right
            } else {
                leaf_up.cross(frame_right).normalize()
            };
            let petiole_start = shoot.start.lerp(shoot.end, along.clamp(0.04, 0.96));
            let petiole_length = parameters.petiole_length_metres[0]
                + unit_hash(leaf_seed ^ 3)
                    * (parameters.petiole_length_metres[1] - parameters.petiole_length_metres[0]);
            let length = parameters.leaf_length_metres[0]
                + unit_hash(leaf_seed ^ 4)
                    * (parameters.leaf_length_metres[1] - parameters.leaf_length_metres[0]);
            let width = length
                * (parameters.leaf_width_ratio[0]
                    + unit_hash(leaf_seed ^ 5)
                        * (parameters.leaf_width_ratio[1] - parameters.leaf_width_ratio[0]));
            let blade_base = petiole_start + radial * petiole_length;
            leaves.push(TreeLeaf {
                petiole_start,
                center: blade_base + leaf_up * length * 0.5,
                right,
                up: leaf_up,
                length,
                width,
                primary_group: shoot.primary_group,
                secondary_group: shoot.secondary_group,
                shoot_id: shoot_index as u16,
                leaf_ordinal: leaf_index as u8,
                shade: 0.68 + unit_hash(leaf_seed ^ 6) * 0.24,
                torsion: (unit_hash(leaf_seed ^ 7) - 0.5) * 0.28,
            });
        }
    }
    leaves
}

pub(in crate::presentation) fn oak_leaf_card_bounds(leaf: TreeLeaf) -> (Vec3, f32, f32) {
    let blade_tip = leaf.center + leaf.up * leaf.length * 0.5;
    let bottom = leaf.petiole_start.dot(leaf.up);
    let top = blade_tip.dot(leaf.up);
    let height = (top - bottom).max(leaf.length) * 1.04;
    let center = leaf.center + leaf.up * (((top + bottom) * 0.5) - leaf.center.dot(leaf.up));
    (center, leaf.width * 1.08, height)
}

fn leaf_shadow_selector(leaf: TreeLeaf) -> f32 {
    let shoot_key = u64::from(leaf.primary_group)
        | (u64::from(leaf.secondary_group) << 8)
        | (u64::from(leaf.shoot_id) << 24);
    unit_hash(splitmix64(shoot_key ^ 0x5a17_8c3d_2149_b6e0))
}

/// Replaces every cambered production leaf with one alpha-masked quad while
/// retaining its biological attachment, orientation, scale, and wind UV.
pub(in crate::presentation) fn procedural_oak_leaf_card_mesh(leaves: &[TreeLeaf]) -> Mesh {
    procedural_woody_leaf_card_mesh(leaves)
}

pub(in crate::presentation) fn procedural_woody_leaf_card_mesh(leaves: &[TreeLeaf]) -> Mesh {
    procedural_woody_leaf_card_mesh_scaled(leaves, 1.0)
}

/// Builds the terminal shrub representation from one deterministic ordinal
/// lane per shoot. The cards grow into overlapping leaf clusters, so this is
/// substantially cheaper than retaining a card for every source leaf while
/// keeping the outline of each shrub species recognisable at distance.
pub(in crate::presentation) fn procedural_woody_sparse_leaf_card_mesh(leaves: &[TreeLeaf]) -> Mesh {
    let sparse_leaves = sparse_woody_far_card_leaves(leaves);
    procedural_woody_leaf_card_mesh_scaled(&sparse_leaves, 1.9)
}

fn procedural_woody_leaf_card_mesh_scaled(leaves: &[TreeLeaf], coverage_scale: f32) -> Mesh {
    let mut positions = Vec::with_capacity(leaves.len() * 4);
    let mut normals = Vec::with_capacity(leaves.len() * 4);
    let mut uvs = Vec::with_capacity(leaves.len() * 4);
    let mut colors = Vec::with_capacity(leaves.len() * 4);
    let mut indices = Vec::with_capacity(leaves.len() * 6);
    for leaf in leaves {
        let (mut center, width, height) = oak_leaf_card_bounds(*leaf);
        // A single plane loses the accepted leaf's projected camber when seen
        // obliquely. Enlarge about the fixed petiole (not the card centre) so
        // the intermediate LOD preserves crown coverage without swimming at
        // its biological attachment.
        const CARD_COVERAGE_SCALE: f32 = 1.24;
        let scaled_width = width * CARD_COVERAGE_SCALE * coverage_scale;
        let scaled_height = height * CARD_COVERAGE_SCALE * coverage_scale;
        center += leaf.up * (scaled_height - height) * 0.5;
        let right = leaf.right * scaled_width * 0.5;
        let up = leaf.up * scaled_height * 0.5;
        let normal = leaf.right.cross(leaf.up).normalize();
        let base = positions.len() as u32;
        positions.extend_from_slice(&[
            (center - right - up).to_array(),
            (center + right - up).to_array(),
            (center + right + up).to_array(),
            (center - right + up).to_array(),
        ]);
        normals.extend_from_slice(&[normal.to_array(); 4]);
        // Image-space V grows downward. Keep the generated petiole at the
        // biological attachment and the blade tip at the distal end.
        uvs.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        let shade = (leaf.shade / 0.82).clamp(0.72, 1.18);
        let ambient_visibility = ((leaf.shade - 0.52) / 0.44).clamp(0.32, 1.0);
        let shadow_selector = leaf_shadow_selector(*leaf);
        colors.extend_from_slice(&[[shade, shade, shadow_selector, ambient_visibility]; 4]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Selects one stable ordinal lane from every source shoot. Sorting the
/// retained leaves makes the resulting mesh byte-for-byte deterministic even
/// if the source generator changes its iteration order.
fn sparse_woody_far_card_leaves(leaves: &[TreeLeaf]) -> Vec<TreeLeaf> {
    let mut retained = leaves
        .iter()
        .filter(|leaf| sparse_woody_far_card_retained(**leaf))
        .copied()
        .collect::<Vec<_>>();
    retained.sort_unstable_by_key(|leaf| {
        (
            leaf.primary_group,
            leaf.secondary_group,
            leaf.shoot_id,
            leaf.leaf_ordinal,
        )
    });
    retained
}

fn sparse_woody_far_card_retained(leaf: TreeLeaf) -> bool {
    let shoot_key = u64::from(leaf.primary_group)
        | (u64::from(leaf.secondary_group) << 8)
        | (u64::from(leaf.shoot_id) << 24);
    let retained_ordinal_lane = splitmix64(shoot_key ^ 0x74a3_2f9b_d817_c56e) as u8 & 3;
    leaf.leaf_ordinal & 3 == retained_ordinal_lane
}

/// The near leaf is a small cambered grid: the geometry retains fold, cupping,
/// torsion, and grazing-angle area while the generated opacity texture owns its
/// fine lobed silhouette. It transitions directly to the flat card.
pub(in crate::presentation) fn procedural_oak_textured_leaf_mesh(leaves: &[TreeLeaf]) -> Mesh {
    procedural_woody_cambered_leaf_mesh(leaves)
}

pub(in crate::presentation) fn procedural_woody_cambered_leaf_mesh(leaves: &[TreeLeaf]) -> Mesh {
    const GRID: u32 = 3;
    const VERTICES_PER_LEAF: usize = (GRID * GRID) as usize;
    const INDICES_PER_LEAF: usize = ((GRID - 1) * (GRID - 1) * 6) as usize;
    let mut positions = Vec::with_capacity(leaves.len() * VERTICES_PER_LEAF);
    let mut normals = Vec::with_capacity(leaves.len() * VERTICES_PER_LEAF);
    let mut uvs = Vec::with_capacity(leaves.len() * VERTICES_PER_LEAF);
    let mut colors = Vec::with_capacity(leaves.len() * VERTICES_PER_LEAF);
    let mut indices = Vec::with_capacity(leaves.len() * INDICES_PER_LEAF);
    for leaf in leaves {
        let (mut center, width, height) = oak_leaf_card_bounds(*leaf);
        const COVERAGE_SCALE: f32 = 1.10;
        let scaled_width = width * COVERAGE_SCALE;
        let scaled_height = height * COVERAGE_SCALE;
        center += leaf.up * (scaled_height - height) * 0.5;
        let shade = (leaf.shade / 0.82).clamp(0.72, 1.18);
        let ambient_visibility = ((leaf.shade - 0.52) / 0.44).clamp(0.32, 1.0);
        let shadow_selector = leaf_shadow_selector(*leaf);
        let base = positions.len() as u32;
        let curl_sign = if leaf.torsion.is_sign_negative() {
            -1.0
        } else {
            1.0
        };
        for row in 0..GRID {
            let v = row as f32 / (GRID - 1) as f32;
            // Even an almost-untwisted source leaf needs enough geometric
            // change to justify this near representation over the flat card.
            // Accumulate a small asymmetric twist toward the tip while the
            // source torsion keeps every leaf from curling identically.
            let twist_angle = (leaf.torsion * 1.15 + curl_sign * 0.08) * (v - 0.20);
            let twist = Quat::from_axis_angle(leaf.up, twist_angle);
            let cross_right = (twist * leaf.right).normalize();
            let cross_normal = cross_right.cross(leaf.up).normalize();
            for column in 0..GRID {
                let u = column as f32 / (GRID - 1) as f32;
                let side = (u - 0.5) * 2.0;
                let lateral = side * scaled_width * 0.5;
                let length_profile = (core::f32::consts::PI * v).sin();
                let midrib_ridge = length_profile * (1.0 - side.abs()) * leaf.width * 0.08;
                let margin_cup = length_profile * side.abs() * leaf.width * 0.11;
                let tip_curl = v * v * v * scaled_height * 0.035 * curl_sign;
                positions.push(
                    (center
                        + leaf.up * (v - 0.5) * scaled_height
                        + cross_right * lateral
                        + cross_normal * (midrib_ridge - margin_cup + tip_curl))
                        .to_array(),
                );
                normals.push(
                    (cross_normal + cross_right * -side * 0.52 - leaf.up * v * curl_sign * 0.08)
                        .normalize()
                        .to_array(),
                );
                uvs.push([u, 1.0 - v]);
                colors.push([shade, shade, shadow_selector, ambient_visibility]);
            }
        }
        for row in 0..(GRID - 1) {
            for column in 0..(GRID - 1) {
                let lower_left = base + row * GRID + column;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + GRID;
                let upper_right = upper_left + 1;
                indices.extend_from_slice(&[
                    lower_left,
                    lower_right,
                    upper_right,
                    lower_left,
                    upper_right,
                    upper_left,
                ]);
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(in crate::presentation) fn procedural_oak_leaf_card_group_mesh(
    leaves: &[TreeLeaf],
    primary_group: u8,
) -> Mesh {
    let group_leaves = detailed_flat_card_group_leaves(leaves, primary_group);
    procedural_woody_leaf_card_mesh(&group_leaves)
}

/// Returns the deterministic 75% subset used exclusively by streamed
/// playable-tree flat cards. Each shoot omits one of its four ordinal lanes;
/// the omitted lane is salted by stable source identity, keeping the crown
/// distributed when source vectors are reordered. Cambered leaves and baked
/// aggregate canopy cards deliberately retain the full source set.
pub(in crate::presentation) fn detailed_flat_card_group_leaves(
    leaves: &[TreeLeaf],
    primary_group: u8,
) -> Vec<TreeLeaf> {
    let mut retained = leaves
        .iter()
        .filter(|leaf| {
            leaf.primary_group == primary_group && detailed_flat_card_leaf_retained(**leaf)
        })
        .copied()
        .collect::<Vec<_>>();
    retained.sort_unstable_by_key(|leaf| {
        (
            leaf.primary_group,
            leaf.secondary_group,
            leaf.shoot_id,
            leaf.leaf_ordinal,
        )
    });
    retained
}

fn detailed_flat_card_leaf_retained(leaf: TreeLeaf) -> bool {
    let shoot_key = u64::from(leaf.primary_group)
        | (u64::from(leaf.secondary_group) << 8)
        | (u64::from(leaf.shoot_id) << 24);
    let omitted_ordinal_lane = splitmix64(shoot_key ^ 0x8f4d_6b29_13ce_57a1) as u8 & 3;
    leaf.leaf_ordinal & 3 != omitted_ordinal_lane
}

pub(in crate::presentation) fn procedural_oak_textured_leaf_group_mesh(
    leaves: &[TreeLeaf],
    primary_group: u8,
) -> Mesh {
    let group_leaves = leaves
        .iter()
        .filter(|leaf| leaf.primary_group == primary_group)
        .copied()
        .collect::<Vec<_>>();
    procedural_woody_cambered_leaf_mesh(&group_leaves)
}

/// Models the compact, scaled winter bud at every current-year shoot tip.
/// It is a separate production mesh so its warm color and overlapping scale
/// silhouette remain legible instead of disappearing into the bark tube cap.
pub(in crate::presentation) fn procedural_oak_bud_mesh(branches: &[TreeBranchSegment]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    const SIDES: u32 = 6;
    const BUD_LENGTH: f32 = 0.008;
    for branch in branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
    {
        let direction = (branch.end - branch.start).normalize();
        let (frame_right, frame_forward) = branch_frame(direction);
        let base = positions.len() as u32;
        let rings = [(0.0_f32, 0.0018_f32), (0.38, 0.0038), (0.78, 0.0025)];
        for (ring_index, (along, radius)) in rings.into_iter().enumerate() {
            let center = branch.end + direction * (along * BUD_LENGTH);
            let phase_offset = if ring_index & 1 == 0 { 0.0 } else { 0.22 };
            for side in 0..SIDES {
                let phase = side as f32 * core::f32::consts::TAU / SIDES as f32 + phase_offset;
                // Alternating ridges suggest overlapping protective scales at
                // the close-review distance without inflating the tiny bud.
                let scale_ridge = if (side + ring_index as u32) & 1 == 0 {
                    1.12
                } else {
                    0.94
                };
                let radial = frame_right * phase.cos() + frame_forward * phase.sin();
                positions.push((center + radial * radius * scale_ridge).to_array());
                normals.push((radial * 0.86 + direction * 0.18).normalize().to_array());
                uvs.push([side as f32 / SIDES as f32, along]);
            }
        }
        for ring in 0..rings.len() as u32 - 1 {
            let from = base + ring * SIDES;
            let to = from + SIDES;
            for side in 0..SIDES {
                let next = (side + 1) % SIDES;
                indices.extend_from_slice(&[
                    from + side,
                    to + side,
                    to + next,
                    from + side,
                    to + next,
                    from + next,
                ]);
            }
        }
        let tip = positions.len() as u32;
        positions.push((branch.end + direction * BUD_LENGTH).to_array());
        normals.push(direction.to_array());
        uvs.push([0.5, 1.0]);
        let last_ring = base + (rings.len() as u32 - 1) * SIDES;
        for side in 0..SIDES {
            let next = (side + 1) % SIDES;
            indices.extend_from_slice(&[tip, last_ring + side, last_ring + next]);
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
    mesh
}

pub(in crate::presentation) fn procedural_oak_bud_group_mesh(
    branches: &[TreeBranchSegment],
    primary_group: u8,
) -> Mesh {
    let group = branches
        .iter()
        .filter(|branch| branch.primary_group == primary_group)
        .copied()
        .collect::<Vec<_>>();
    procedural_oak_bud_mesh(&group)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::obstacles::tree::geometry::{
        TREE_PRIMARY_GROUP_COUNT, procedural_tree_branch_mesh, procedural_tree_skeleton,
    };

    #[test]
    fn production_oak_has_finite_cambered_leaf_geometry() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        assert!((45_000..60_000).contains(&leaves.len()));
        assert!(leaves.iter().all(|leaf| leaf.petiole_start.is_finite()
            && leaf.center.is_finite()
            && leaf.right.is_finite()
            && leaf.up.is_finite()
            && leaf.right.length_squared() > 0.9
            && leaf.up.length_squared() > 0.9
            && leaf.right.cross(leaf.up).length_squared() > 0.5
            && leaf.torsion.is_finite()));
        let mesh = procedural_oak_textured_leaf_mesh(&leaves);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("leaf mesh has float positions");
        assert_eq!(positions.len(), leaves.len() * 9);
        assert_eq!(
            mesh.indices().expect("leaf mesh has indices").len() / 3,
            leaves.len() * 8
        );
        assert!(
            positions
                .iter()
                .flatten()
                .all(|component| component.is_finite())
        );
        let buds = procedural_oak_bud_mesh(&branches);
        let bud_positions = buds
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3())
            .expect("bud mesh has float positions");
        let terminal_shoots = branches
            .iter()
            .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
            .count();
        assert_eq!(terminal_shoots, leaves.len() / 16);
        assert_eq!(bud_positions.len(), terminal_shoots * 19);
        assert_eq!(
            buds.indices().expect("bud mesh has indices").len() / 3,
            terminal_shoots * 30
        );
        assert!(
            bud_positions
                .iter()
                .flatten()
                .all(|component| component.is_finite())
        );
        let leaf_triangles = mesh.indices().expect("leaf mesh has indices").len() / 3;
        let bud_triangles = buds.indices().expect("bud mesh has indices").len() / 3;
        let branch_triangles = procedural_tree_branch_mesh(&branches, 3)
            .indices()
            .expect("branch mesh has indices")
            .len()
            / 3;
        assert!(leaf_triangles + bud_triangles + branch_triangles <= 3_600_000);
        let group_triangles = (0..TREE_PRIMARY_GROUP_COUNT)
            .map(|group| {
                procedural_oak_textured_leaf_group_mesh(&leaves, group)
                    .indices()
                    .expect("sector has indices")
                    .len()
                    / 3
            })
            .sum::<usize>();
        assert_eq!(group_triangles, leaf_triangles);
    }

    #[test]
    fn alpha_leaf_lod_uses_exactly_two_triangles_per_leaf() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let mesh = procedural_oak_leaf_card_mesh(&leaves);
        assert_eq!(mesh.count_vertices(), leaves.len() * 4);
        assert_eq!(
            mesh.indices().expect("leaf cards are indexed").len(),
            leaves.len() * 6
        );
    }

    #[test]
    fn detailed_flat_cards_keep_a_stable_balanced_three_quarters_per_cluster() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let source_count = leaves.len();
        let mut retained_count = 0;

        for primary_group in 0..TREE_PRIMARY_GROUP_COUNT {
            let source = leaves
                .iter()
                .filter(|leaf| leaf.primary_group == primary_group)
                .count();
            let retained = detailed_flat_card_group_leaves(&leaves, primary_group);
            let repeated = detailed_flat_card_group_leaves(&leaves, primary_group);
            let reordered = leaves.iter().rev().copied().collect::<Vec<_>>();
            let reordered_retained = detailed_flat_card_group_leaves(&reordered, primary_group);
            assert_eq!(retained.len(), source * 3 / 4);
            assert_eq!(retained.len(), repeated.len());
            assert!(
                retained
                    .iter()
                    .zip(repeated.iter())
                    .all(|(left, right)| left.shoot_id == right.shoot_id
                        && left.leaf_ordinal == right.leaf_ordinal)
            );
            assert!(
                retained
                    .iter()
                    .zip(reordered_retained.iter())
                    .all(|(left, right)| left.shoot_id == right.shoot_id
                        && left.leaf_ordinal == right.leaf_ordinal)
            );
            let mesh = procedural_oak_leaf_card_group_mesh(&leaves, primary_group);
            assert_eq!(mesh.count_vertices(), retained.len() * 4);
            assert_eq!(
                mesh.indices().expect("flat cards are indexed").len(),
                retained.len() * 6
            );
            assert!(
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    .and_then(|attribute| attribute.as_float3())
                    .expect("flat cards have positions")
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite())
            );
            assert!(
                mesh.indices()
                    .expect("flat cards are indexed")
                    .iter()
                    .all(|index| (index as usize) < mesh.count_vertices())
            );
            retained_count += retained.len();
        }

        assert_eq!(source_count, 50_784);
        assert_eq!(retained_count, 38_088);
    }

    #[test]
    fn textured_leaf_lod_uses_exactly_eight_triangles_per_leaf() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let mesh = procedural_oak_textured_leaf_mesh(&leaves);
        assert_eq!(mesh.count_vertices(), leaves.len() * 9);
        assert_eq!(
            mesh.indices().expect("textured leaves are indexed").len(),
            leaves.len() * 24
        );
    }

    #[test]
    fn leaf_shadow_transmission_is_stable_per_shoot_and_well_distributed() {
        let branches = procedural_tree_skeleton(42, 0.0);
        let leaves = procedural_oak_leaves(42, &branches, 0.0);
        let mut shoots = std::collections::BTreeMap::new();
        for leaf in leaves {
            let key = (leaf.primary_group, leaf.secondary_group, leaf.shoot_id);
            let selector = leaf_shadow_selector(leaf);
            let previous = shoots.entry(key).or_insert(selector);
            assert_eq!(*previous, selector);
        }
        let transmitting = shoots.values().filter(|selector| **selector < 0.42).count();
        let fraction = transmitting as f32 / shoots.len() as f32;
        assert!((0.37..=0.47).contains(&fraction));
    }
}
