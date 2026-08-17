/// Inclusive bounds on one model parameter.
#[derive(Debug, Clone, Copy)]
pub struct ParameterLimit {
    pub parameter: usize,
    pub min: f32,
    pub max: f32,
    /// How strongly a solver should enforce the bound; 1.0 unless the file says
    /// otherwise. Clamping ignores it.
    pub weight: f32,
}
