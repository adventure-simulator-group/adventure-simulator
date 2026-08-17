/// Momentum allows at most eight joint influences per vertex.
pub const MAX_SKIN_JOINTS: usize = 8;

/// Fixed-width sparse skinning, eight influences per vertex.
#[derive(Debug, Default, Clone)]
pub struct SkinWeights {
    pub index: Vec<[u32; MAX_SKIN_JOINTS]>,
    pub weight: Vec<[f32; MAX_SKIN_JOINTS]>,
}
