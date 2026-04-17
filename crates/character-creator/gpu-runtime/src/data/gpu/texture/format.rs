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
    R32Float,
    Rg32Float,
    Rgba32Float,
    Depth32Float,
    R8Uint,
    R8Sint,
    R8Snorm,
    R8Unorm,
    Rg8Uint,
    Rg8Sint,
    Rg8Snorm,
    Rg8Unorm,
    Rgba8Uint,
    Rgba8Sint,
    Rgba8Snorm,
    Rgba8Unorm,
    R32Sint,
    Rg32Sint,
    Rgba32Sint,
    R32Uint,
    Rg32Uint,
    Rgba32Uint,
}

impl From<TextureFormat> for wgpu::TextureFormat {
    fn from(f: TextureFormat) -> Self {
        match f {
            TextureFormat::R32Float => wgpu::TextureFormat::R32Float,
            TextureFormat::Rg32Float => wgpu::TextureFormat::Rg32Float,
            TextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
            TextureFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
            TextureFormat::R8Uint => wgpu::TextureFormat::R8Uint,
            TextureFormat::R8Sint => wgpu::TextureFormat::R8Sint,
            TextureFormat::R8Snorm => wgpu::TextureFormat::R8Snorm,
            TextureFormat::R8Unorm => wgpu::TextureFormat::R8Unorm,
            TextureFormat::Rg8Uint => wgpu::TextureFormat::Rg8Uint,
            TextureFormat::Rg8Sint => wgpu::TextureFormat::Rg8Sint,
            TextureFormat::Rg8Snorm => wgpu::TextureFormat::Rg8Snorm,
            TextureFormat::Rg8Unorm => wgpu::TextureFormat::Rg8Unorm,
            TextureFormat::Rgba8Uint => wgpu::TextureFormat::Rgba8Uint,
            TextureFormat::Rgba8Sint => wgpu::TextureFormat::Rgba8Sint,
            TextureFormat::Rgba8Snorm => wgpu::TextureFormat::Rgba8Snorm,
            TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            TextureFormat::R32Sint => wgpu::TextureFormat::R32Sint,
            TextureFormat::Rg32Sint => wgpu::TextureFormat::Rg32Sint,
            TextureFormat::Rgba32Sint => wgpu::TextureFormat::Rgba32Sint,
            TextureFormat::R32Uint => wgpu::TextureFormat::R32Uint,
            TextureFormat::Rg32Uint => wgpu::TextureFormat::Rg32Uint,
            TextureFormat::Rgba32Uint => wgpu::TextureFormat::Rgba32Uint,
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

    pub fn pixel_size(&self) -> u32 {
        match self {
            TextureFormat::R32Float => 4,
            TextureFormat::Rg32Float => 8,
            TextureFormat::Rgba32Float => 16,
            TextureFormat::Depth32Float => 4,
            TextureFormat::R8Uint => 1,
            TextureFormat::R8Sint => 1,
            TextureFormat::R8Snorm => 1,
            TextureFormat::R8Unorm => 1,
            TextureFormat::Rg8Uint => 2,
            TextureFormat::Rg8Sint => 2,
            TextureFormat::Rg8Snorm => 2,
            TextureFormat::Rg8Unorm => 2,
            TextureFormat::Rgba8Uint => 4,
            TextureFormat::Rgba8Sint => 4,
            TextureFormat::Rgba8Snorm => 4,
            TextureFormat::Rgba8Unorm => 4,
            TextureFormat::R32Sint => 4,
            TextureFormat::Rg32Sint => 8,
            TextureFormat::Rgba32Sint => 16,
            TextureFormat::R32Uint => 4,
            TextureFormat::Rg32Uint => 8,
            TextureFormat::Rgba32Uint => 16,
        }
    }

    pub fn components(&self) -> usize {
        match self {
            TextureFormat::R32Float
            | TextureFormat::Depth32Float
            | TextureFormat::R8Uint
            | TextureFormat::R8Sint
            | TextureFormat::R8Snorm
            | TextureFormat::R8Unorm
            | TextureFormat::R32Sint
            | TextureFormat::R32Uint => 1,
            TextureFormat::Rg32Float
            | TextureFormat::Rg8Uint
            | TextureFormat::Rg8Sint
            | TextureFormat::Rg8Snorm
            | TextureFormat::Rg8Unorm
            | TextureFormat::Rg32Sint
            | TextureFormat::Rg32Uint => 2,
            TextureFormat::Rgba32Float
            | TextureFormat::Rgba8Uint
            | TextureFormat::Rgba8Sint
            | TextureFormat::Rgba8Snorm
            | TextureFormat::Rgba8Unorm
            | TextureFormat::Rgba32Sint
            | TextureFormat::Rgba32Uint => 4,
        }
    }

    pub fn to_wgsl_storage_format(&self) -> &str {
        match self {
            TextureFormat::Rgba8Unorm => "rgba8unorm",
            TextureFormat::R32Float => "r32float",
            TextureFormat::Rg32Float => "rg32float",
            TextureFormat::Rgba32Float => "rgba32float",
            TextureFormat::Rgba8Uint => "rgba8uint",
            TextureFormat::R8Uint => "r8uint",
            TextureFormat::R8Sint => "r8sint",
            TextureFormat::R8Snorm => "r8snorm",
            TextureFormat::R8Unorm => "r8unorm",
            TextureFormat::Rg8Uint => "rg8uint",
            TextureFormat::Rg8Sint => "rg8sint",
            TextureFormat::Rg8Snorm => "rg8snorm",
            TextureFormat::Rg8Unorm => "rg8unorm",
            TextureFormat::Rgba8Sint => "rgba8sint",
            TextureFormat::Rgba8Snorm => "rgba8snorm",
            TextureFormat::R32Sint => "r32sint",
            TextureFormat::Rg32Sint => "rg32sint",
            TextureFormat::Rgba32Sint => "rgba32sint",
            TextureFormat::R32Uint => "r32uint",
            TextureFormat::Rg32Uint => "rg32uint",
            TextureFormat::Rgba32Uint => "rgba32uint",
            _ => "rgba32float", // Fallback for safety, though some formats might not be storage-compatible
        }
    }
}
