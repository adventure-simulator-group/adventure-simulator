use bevy::prelude::Vec3;

#[cfg(feature = "meshgen")]
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::Mesh,
};

use crate::{scene::SceneTerrain, terrain_transition::TerrainTransitionCollar};

impl SceneTerrain {
    #[cfg(feature = "meshgen")]
    pub(crate) fn mesh_from_components(
        &self,
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        uvs: Vec<[f32; 2]>,
    ) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh.with_computed_area_weighted_normals()
    }

    pub(crate) fn collider_mesh_with_transition(
        &self,
        collar: TerrainTransitionCollar,
    ) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let (positions, indices, _) = self.mesh_components_with_stride_and_transition(1, collar);
        (
            positions.into_iter().map(Vec3::from_array).collect(),
            indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|triangle| [triangle[0], triangle[1], triangle[2]])
                .collect(),
        )
    }

    #[cfg(feature = "meshgen")]
    pub fn coarse_mesh_with_transition(&self, collar: TerrainTransitionCollar) -> Mesh {
        self.mesh_with_stride_and_transition(self.coarse_stride(), collar)
    }

    #[cfg(feature = "meshgen")]
    fn mesh_with_stride_and_transition(
        &self,
        stride: usize,
        collar: TerrainTransitionCollar,
    ) -> Mesh {
        let (positions, indices, uvs) =
            self.mesh_components_with_stride_and_transition(stride, collar);
        self.mesh_from_components(positions, indices, uvs)
    }

    pub(crate) fn mesh_components_with_stride_and_transition(
        &self,
        stride: usize,
        collar: TerrainTransitionCollar,
    ) -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 2]>) {
        self.mesh_components_with_stride_filtered(stride, |point| !collar.cuts_out(point))
    }

    #[cfg(feature = "meshgen")]
    fn coarse_stride(&self) -> usize {
        (self.coarse_grid_scale() / self.grid_scale()).round() as usize
    }
}
