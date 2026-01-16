use bevy_math::*;

pub struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn add_triangle(&mut self, v0: Vec3, v1: Vec3, v2: Vec3) {
        let normal = (v2 - v0).cross(v1 - v0);
        if normal.length_squared() <= f32::EPSILON {
            return;
        }
        let normal = normal.normalize();
        let normal_array = normal.to_array();

        let base_index = self.positions.len() as u32;

        self.positions.push(v0.to_array());
        self.positions.push(v2.to_array());
        self.positions.push(v1.to_array());

        self.normals.push(normal_array);
        self.normals.push(normal_array);
        self.normals.push(normal_array);

        const UV_SCALE: f32 = 0.25;
        self.uvs
            .push([v0.x * UV_SCALE + 0.5, v0.z * UV_SCALE + 0.5]);
        self.uvs
            .push([v2.x * UV_SCALE + 0.5, v2.z * UV_SCALE + 0.5]);
        self.uvs
            .push([v1.x * UV_SCALE + 0.5, v1.z * UV_SCALE + 0.5]);

        self.indices
            .extend([base_index, base_index + 1, base_index + 2]);
    }
}

#[cfg(feature = "bevy")]
mod bevy {
    use super::MeshBuilder;
    use bevy_mesh::*;
    use bevy_asset::RenderAssetUsages;

    impl MeshBuilder {
        pub fn build(self) -> Mesh {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
            mesh.insert_indices(Indices::U32(self.indices));
            mesh
        }
    }
}