use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize,
    Deserialize,
    strum::EnumIter,
    strum::AsRefStr,
    strum::EnumString,
)]
pub enum SamplerFilterMode {
    #[default]
    Linear,
    Nearest,
}

impl From<SamplerFilterMode> for wgpu::FilterMode {
    fn from(mode: SamplerFilterMode) -> Self {
        match mode {
            SamplerFilterMode::Nearest => wgpu::FilterMode::Nearest,
            SamplerFilterMode::Linear => wgpu::FilterMode::Linear,
        }
    }
}

impl From<f64> for SamplerFilterMode {
    fn from(v: f64) -> Self {
        match v as u32 {
            0 => SamplerFilterMode::Linear,
            1 => SamplerFilterMode::Nearest,
            _ => SamplerFilterMode::Linear,
        }
    }
}
