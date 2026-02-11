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
pub enum SamplerAddressMode {
    #[default]
    Repeat,
    ClampToEdge,
    MirrorRepeat,
    ClampToBorder,
}

impl From<SamplerAddressMode> for wgpu::AddressMode {
    fn from(mode: SamplerAddressMode) -> Self {
        match mode {
            SamplerAddressMode::Repeat => wgpu::AddressMode::Repeat,
            SamplerAddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
            SamplerAddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
            SamplerAddressMode::ClampToBorder => wgpu::AddressMode::ClampToBorder,
        }
    }
}

impl From<f64> for SamplerAddressMode {
    fn from(v: f64) -> Self {
        match v as u32 {
            0 => SamplerAddressMode::Repeat,
            1 => SamplerAddressMode::ClampToEdge,
            2 => SamplerAddressMode::MirrorRepeat,
            3 => SamplerAddressMode::ClampToBorder,
            _ => SamplerAddressMode::Repeat,
        }
    }
}
