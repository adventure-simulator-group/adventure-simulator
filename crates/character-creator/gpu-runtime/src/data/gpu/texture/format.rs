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
pub enum TextureFormat {
    #[default]
    Rgba8Unorm,
    R32Float,
    Rgba32Float,
    Rgba8Uint,
    Depth32Float,
}

impl From<TextureFormat> for wgpu::TextureFormat {
    fn from(f: TextureFormat) -> Self {
        match f {
            TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            TextureFormat::R32Float => wgpu::TextureFormat::R32Float,
            TextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
            TextureFormat::Rgba8Uint => wgpu::TextureFormat::Rgba8Uint,
            TextureFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
        }
    }
}

impl TextureFormat {
    pub fn naga_to_wgpu_format(format: wgpu::naga::StorageFormat) -> wgpu::TextureFormat {
        match format {
            wgpu::naga::StorageFormat::R8Unorm => wgpu::TextureFormat::R8Unorm,
            wgpu::naga::StorageFormat::R8Snorm => wgpu::TextureFormat::R8Snorm,
            wgpu::naga::StorageFormat::R8Uint => wgpu::TextureFormat::R8Uint,
            wgpu::naga::StorageFormat::R8Sint => wgpu::TextureFormat::R8Sint,
            wgpu::naga::StorageFormat::R16Uint => wgpu::TextureFormat::R16Uint,
            wgpu::naga::StorageFormat::R16Sint => wgpu::TextureFormat::R16Sint,
            wgpu::naga::StorageFormat::R16Float => wgpu::TextureFormat::R16Float,
            wgpu::naga::StorageFormat::Rg8Unorm => wgpu::TextureFormat::Rg8Unorm,
            wgpu::naga::StorageFormat::Rg8Snorm => wgpu::TextureFormat::Rg8Snorm,
            wgpu::naga::StorageFormat::Rg8Uint => wgpu::TextureFormat::Rg8Uint,
            wgpu::naga::StorageFormat::Rg8Sint => wgpu::TextureFormat::Rg8Sint,
            wgpu::naga::StorageFormat::R32Uint => wgpu::TextureFormat::R32Uint,
            wgpu::naga::StorageFormat::R32Sint => wgpu::TextureFormat::R32Sint,
            wgpu::naga::StorageFormat::R32Float => wgpu::TextureFormat::R32Float,
            wgpu::naga::StorageFormat::Rg16Uint => wgpu::TextureFormat::Rg16Uint,
            wgpu::naga::StorageFormat::Rg16Sint => wgpu::TextureFormat::Rg16Sint,
            wgpu::naga::StorageFormat::Rg16Float => wgpu::TextureFormat::Rg16Float,
            wgpu::naga::StorageFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            wgpu::naga::StorageFormat::Rgba8Snorm => wgpu::TextureFormat::Rgba8Snorm,
            wgpu::naga::StorageFormat::Rgba8Uint => wgpu::TextureFormat::Rgba8Uint,
            wgpu::naga::StorageFormat::Rgba8Sint => wgpu::TextureFormat::Rgba8Sint,
            wgpu::naga::StorageFormat::Rg32Uint => wgpu::TextureFormat::Rg32Uint,
            wgpu::naga::StorageFormat::Rg32Sint => wgpu::TextureFormat::Rg32Sint,
            wgpu::naga::StorageFormat::Rg32Float => wgpu::TextureFormat::Rg32Float,
            wgpu::naga::StorageFormat::Rgba16Uint => wgpu::TextureFormat::Rgba16Uint,
            wgpu::naga::StorageFormat::Rgba16Sint => wgpu::TextureFormat::Rgba16Sint,
            wgpu::naga::StorageFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            wgpu::naga::StorageFormat::Rgba32Uint => wgpu::TextureFormat::Rgba32Uint,
            wgpu::naga::StorageFormat::Rgba32Sint => wgpu::TextureFormat::Rgba32Sint,
            wgpu::naga::StorageFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
            wgpu::naga::StorageFormat::R16Unorm => wgpu::TextureFormat::R16Unorm,
            wgpu::naga::StorageFormat::R16Snorm => wgpu::TextureFormat::R16Snorm,
            wgpu::naga::StorageFormat::Rg16Unorm => wgpu::TextureFormat::Rg16Unorm,
            wgpu::naga::StorageFormat::Rg16Snorm => wgpu::TextureFormat::Rg16Snorm,
            wgpu::naga::StorageFormat::Rgba16Unorm => wgpu::TextureFormat::Rgba16Unorm,
            wgpu::naga::StorageFormat::Rgba16Snorm => wgpu::TextureFormat::Rgba16Snorm,
            wgpu::naga::StorageFormat::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
            wgpu::naga::StorageFormat::Rgb10a2Uint => wgpu::TextureFormat::Rgb10a2Uint,
            wgpu::naga::StorageFormat::Rgb10a2Unorm => wgpu::TextureFormat::Rgb10a2Unorm,
            wgpu::naga::StorageFormat::Rg11b10Ufloat => wgpu::TextureFormat::Rg11b10Ufloat,
            wgpu::naga::StorageFormat::R64Uint => wgpu::TextureFormat::R64Uint,
        }
    }

    pub fn is_filterable(format: wgpu::TextureFormat, features: wgpu::Features) -> bool {
        match format {
            wgpu::TextureFormat::R32Float
            | wgpu::TextureFormat::Rg32Float
            | wgpu::TextureFormat::Rgba32Float => {
                features.contains(wgpu::Features::FLOAT32_FILTERABLE)
            }
            wgpu::TextureFormat::R16Float
            | wgpu::TextureFormat::Rg16Float
            | wgpu::TextureFormat::Rgba16Float
            | wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb => true,
            _ => false,
        }
    }
}
