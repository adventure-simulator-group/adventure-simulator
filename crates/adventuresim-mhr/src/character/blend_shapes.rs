/// Dense blend-shape basis, `shape-major`: `vectors[shape * n_verts * 3 + ..]`.
#[derive(Debug, Default, Clone)]
pub struct BlendShapes {
    pub names: Vec<String>,
    pub vectors: Vec<f32>,
    pub num_vertices: usize,
}

impl BlendShapes {
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}
