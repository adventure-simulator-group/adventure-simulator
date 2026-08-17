use burn::tensor::Tensor;

/// One forward pass output.
pub struct MhrOutput {
    /// Posed vertices in centimetres, `[batch, vertices, 3]`.
    pub vertices: Tensor<3>,
    /// Unit vertex normals of the posed mesh, `[batch, vertices, 3]`.
    ///
    /// Recomputed from the deformed vertices rather than skinned from the rest
    /// pose, because blend shapes and the pose correctives change the surface
    /// itself, not just its rigid frame.
    pub normals: Tensor<3>,
    /// Global joint transforms `[tx, ty, tz, qx, qy, qz, qw, s]`, `[batch, joints, 8]`.
    pub skeleton_state: Tensor<3>,
}
