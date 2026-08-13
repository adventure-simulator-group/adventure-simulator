use super::TreeBakeCard;
use crate::presentation::obstacles::tree::geometry::{
    ENGLISH_OAK_PARAMETERS, TreeBranchSegment, TreeLeaf, procedural_oak_textured_leaf_mesh,
    procedural_tree_branch_mesh,
};
use crate::presentation::obstacles::tree::materials::{
    OAK_LEAF_IMPOSTOR_BASE_SRGB, canopy_ao_strength,
};
use bevy::{
    math::Vec3Swizzles,
    mesh::{Indices, VertexAttributeValues},
    prelude::{Mesh, Vec2, Vec3, Vec4},
};

pub(super) fn render_tree_card(
    card: TreeBakeCard,
    branches: &[TreeBranchSegment],
    leaves: &[TreeLeaf],
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
) {
    let mut depth = vec![f32::NEG_INFINITY; (tile_size * tile_size) as usize];
    let source_branches = branches
        .iter()
        .filter(|branch| card.includes_branch(branch))
        .copied()
        .collect::<Vec<_>>();
    let branch_mesh = procedural_tree_branch_mesh(&source_branches, 3);
    raster_source_mesh(
        card,
        &branch_mesh,
        TreeSourceMaterial::Bark,
        tile_size,
        atlas_width,
        atlas_height,
        tile_x,
        tile_y,
        pixels,
        &mut depth,
    );
    let source_leaves = leaves
        .iter()
        .filter(|leaf| card.includes_leaf(leaf))
        .copied()
        .collect::<Vec<_>>();
    let leaf_mesh = procedural_oak_textured_leaf_mesh(&source_leaves);
    raster_source_mesh(
        card,
        &leaf_mesh,
        TreeSourceMaterial::Leaf,
        tile_size,
        atlas_width,
        atlas_height,
        tile_x,
        tile_y,
        pixels,
        &mut depth,
    );
}

#[derive(Clone, Copy)]
enum TreeSourceMaterial {
    Bark,
    Leaf,
}

fn raster_source_mesh(
    card: TreeBakeCard,
    mesh: &Mesh,
    material: TreeSourceMaterial,
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
    depth: &mut [f32],
) {
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(VertexAttributeValues::as_float3)
        .expect("procedural tree mesh has float positions");
    let normals = mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(VertexAttributeValues::as_float3)
        .expect("procedural tree mesh has float normals");
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(colors)) => Some(colors.as_slice()),
        _ => None,
    };
    let Indices::U32(indices) = mesh.indices().expect("procedural tree mesh is indexed") else {
        unreachable!("procedural tree mesh uses u32 indices")
    };
    for triangle in indices.chunks_exact(3) {
        let vertex_indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let projected = vertex_indices
            .map(|index| project_to_tile(card, Vec3::from_array(positions[index]), tile_size));
        let a = projected[0].xy();
        let b = projected[1].xy();
        let c = projected[2].xy();
        let denominator = (b - a).perp_dot(c - a);
        if denominator.abs() < 0.0001 {
            continue;
        }
        let minimum = a.min(b).min(c).floor().max(Vec2::ZERO);
        let maximum = a
            .max(b)
            .max(c)
            .ceil()
            .min(Vec2::splat(tile_size as f32 - 1.0));
        for y in minimum.y as u32..=maximum.y as u32 {
            for x in minimum.x as u32..=maximum.x as u32 {
                let sample = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let weight_b = (sample - a).perp_dot(c - a) / denominator;
                let weight_c = (b - a).perp_dot(sample - a) / denominator;
                let weight_a = 1.0 - weight_b - weight_c;
                if weight_a < -0.001 || weight_b < -0.001 || weight_c < -0.001 {
                    continue;
                }
                let weights = [weight_a, weight_b, weight_c];
                let z = projected
                    .iter()
                    .zip(weights)
                    .map(|(point, weight)| point.z * weight)
                    .sum();
                let normal = vertex_indices
                    .iter()
                    .zip(weights)
                    .map(|(index, weight)| Vec3::from_array(normals[*index]) * weight)
                    .sum::<Vec3>()
                    .normalize_or_zero();
                let light = 0.62 + normal.dot(Vec3::new(0.35, 0.86, 0.25)).abs() * 0.34;
                let color = match material {
                    TreeSourceMaterial::Bark => [
                        (116.0 * light) as u8,
                        (103.0 * light) as u8,
                        (82.0 * light) as u8,
                        255,
                    ],
                    TreeSourceMaterial::Leaf => {
                        let tint = colors.map_or(Vec4::ONE, |colors| {
                            vertex_indices
                                .iter()
                                .zip(weights)
                                .map(|(index, weight)| Vec4::from_array(colors[*index]) * weight)
                                .sum()
                        });
                        baked_oak_leaf_color(tint, light)
                    }
                };
                write_tree_pixel(
                    x,
                    y,
                    z,
                    color,
                    tile_size,
                    atlas_width,
                    atlas_height,
                    tile_x,
                    tile_y,
                    pixels,
                    depth,
                );
            }
        }
    }
}

fn baked_oak_leaf_color(tint: Vec4, light: f32) -> [u8; 4] {
    // Vertex color RGB is semantic data, not an albedo tint: X/Y carry the
    // authored shade, Z selects a live directional self-shadow, and W carries
    // ambient visibility. The old bake accidentally treated XYZ as pigment,
    // producing lime cards and using the binary shadow selector as blue.
    let shade = ((tint.x + tint.y) * 0.5).clamp(0.0, 1.5);
    let canopy_visibility = 1.0
        + canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres)
            * (tint.w.clamp(0.32, 1.0) - 1.0);
    let response = (shade * canopy_visibility * light).max(0.0);
    [
        (OAK_LEAF_IMPOSTOR_BASE_SRGB[0] * response).min(255.0) as u8,
        (OAK_LEAF_IMPOSTOR_BASE_SRGB[1] * response).min(255.0) as u8,
        (OAK_LEAF_IMPOSTOR_BASE_SRGB[2] * response).min(255.0) as u8,
        255,
    ]
}

pub(super) fn project_to_tile(card: TreeBakeCard, point: Vec3, tile_size: u32) -> Vec3 {
    let relative = point - card.center;
    Vec3::new(
        (relative.dot(card.right) / card.width + 0.5) * (tile_size - 1) as f32,
        (0.5 - relative.dot(card.up) / card.height) * (tile_size - 1) as f32,
        relative.dot(card.normal()),
    )
}
pub(super) fn write_tree_pixel(
    x: u32,
    y: u32,
    z: f32,
    color: [u8; 4],
    tile_size: u32,
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    pixels: &mut [u8],
    depth: &mut [f32],
) {
    let local_index = (y * tile_size + x) as usize;
    if z <= depth[local_index] {
        return;
    }
    depth[local_index] = z;
    let atlas_x = tile_x * tile_size + x;
    let atlas_y = tile_y * tile_size + y;
    debug_assert!(atlas_x < atlas_width && atlas_y < atlas_height);
    let index = ((atlas_y * atlas_width + atlas_x) * 4) as usize;
    pixels[index..index + 4].copy_from_slice(&color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baked_leaf_color_uses_generated_palette_and_authored_canopy_visibility() {
        let exposed = baked_oak_leaf_color(Vec4::new(1.0, 1.0, 0.0, 1.0), 1.0);
        let alternate_shadow_selector = baked_oak_leaf_color(Vec4::new(1.0, 1.0, 1.0, 1.0), 1.0);
        let interior = baked_oak_leaf_color(Vec4::new(1.0, 1.0, 0.0, 0.32), 1.0);

        assert_eq!(exposed, [96, 113, 76, 255]);
        assert_eq!(alternate_shadow_selector, exposed);
        assert!(interior[0] < exposed[0]);
        assert!(interior[1] < exposed[1]);
        assert!(interior[2] < exposed[2]);
        assert!(
            exposed[1] - exposed[0] < 24,
            "oak pigment must not turn lime"
        );
        assert!(
            exposed[2] > exposed[0] / 2,
            "blue must come from pigment, not a selector"
        );
    }
}
