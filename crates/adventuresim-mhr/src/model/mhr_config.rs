/// How to load an MHR model.
#[derive(Debug, Clone, Copy)]
pub struct MhrConfig {
    pub lod: u8,
    /// Load the pose-corrective network. It dominates both load time and
    /// memory (2.5 GiB of coefficients at LOD 0), so it can be turned off.
    pub pose_correctives: bool,
}

impl Default for MhrConfig {
    fn default() -> Self {
        Self {
            lod: 1,
            pose_correctives: true,
        }
    }
}
