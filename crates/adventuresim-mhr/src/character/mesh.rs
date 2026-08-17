/// Triangulated mesh with the original polygon topology dropped.
#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub vertices: Vec<[f32; 3]>,
    pub faces: Vec<[u32; 3]>,
    pub texcoords: Vec<[f32; 2]>,
    pub texcoord_faces: Vec<[u32; 3]>,
}
