use bevy::prelude::{Vec2, Vec3};

#[derive(Clone, Copy, Debug)]
pub(super) struct TreeCardGeometry {
    pub(super) center: Vec3,
    pub(super) right: Vec3,
    pub(super) up: Vec3,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) uv_min: Vec2,
    pub(super) uv_max: Vec2,
}

pub(super) struct TreeCardMeshBuffers<'a> {
    pub(super) positions: &'a mut Vec<[f32; 3]>,
    pub(super) normals: &'a mut Vec<[f32; 3]>,
    pub(super) uvs: &'a mut Vec<[f32; 2]>,
    pub(super) indices: &'a mut Vec<u32>,
}

pub(super) fn append_tree_card_with_uv(
    geometry: TreeCardGeometry,
    buffers: TreeCardMeshBuffers<'_>,
) {
    let TreeCardGeometry {
        center,
        right,
        up,
        width,
        height,
        uv_min,
        uv_max,
    } = geometry;
    let right = right.normalize() * width * 0.5;
    let up = up.normalize() * height * 0.5;
    let normal = right.cross(up).normalize_or_zero();
    let base = buffers.positions.len() as u32;
    buffers.positions.extend_from_slice(&[
        (center - right - up).to_array(),
        (center + right - up).to_array(),
        (center + right + up).to_array(),
        (center - right + up).to_array(),
    ]);
    buffers.normals.extend_from_slice(&[normal.to_array(); 4]);
    buffers.uvs.extend_from_slice(&[
        [uv_min.x, uv_max.y],
        [uv_max.x, uv_max.y],
        [uv_max.x, uv_min.y],
        [uv_min.x, uv_min.y],
    ]);
    buffers
        .indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}
