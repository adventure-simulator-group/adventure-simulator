use bevy::prelude::*;

use super::{GrassMeshLod, TacticalGrassInstancedMaterial};

#[derive(Component)]
pub(crate) struct GrassTriangleCount(pub(crate) usize);

pub(super) fn add_mesh(
    meshes: &mut Assets<Mesh>,
    mesh: Mesh,
) -> (Handle<Mesh>, GrassTriangleCount) {
    let triangles = crate::presentation::mesh_triangle_count(&mesh);
    (meshes.add(mesh), GrassTriangleCount(triangles))
}

pub(super) fn casts_shadows(
    lod: GrassMeshLod,
    grass: &crate::presentation::config::GrassConfig,
) -> bool {
    match lod {
        GrassMeshLod::Near => grass.lighting.casts_shadows.near,
        GrassMeshLod::NearEdge => grass.lighting.casts_shadows.near_edge,
        GrassMeshLod::Far => grass.lighting.casts_shadows.far,
        GrassMeshLod::Vista => grass.lighting.casts_shadows.vista,
    }
}

pub(super) const fn tier_index(lod: GrassMeshLod) -> usize {
    match lod {
        GrassMeshLod::Near => 0,
        GrassMeshLod::NearEdge => 1,
        GrassMeshLod::Far => 2,
        GrassMeshLod::Vista => 3,
    }
}

pub(super) fn material(
    lod: GrassMeshLod,
    grass: &crate::presentation::config::GrassConfig,
    grass_density: f32,
    grass_dryness: f32,
    wind_scale: f32,
) -> TacticalGrassInstancedMaterial {
    TacticalGrassInstancedMaterial {
        wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
        interaction: Vec4::ZERO,
        interaction_motion: Vec4::ZERO,
        params: Vec4::new(
            grass.lighting.root_occlusion,
            grass_dryness,
            0.09,
            lod.width_compensation(grass_density),
        ),
        shading: Vec4::new(1.0, grass.lighting.ambient_scale, 0.0, 0.0),
    }
}
