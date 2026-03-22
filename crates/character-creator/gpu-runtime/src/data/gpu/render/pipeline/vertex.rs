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
pub enum VertexFormat {
    Uint8x2,
    Uint8x4,
    Sint8x2,
    Sint8x4,
    Unorm8x2,
    Unorm8x4,
    Snorm8x2,
    Snorm8x4,
    Uint16x2,
    Uint16x4,
    Sint16x2,
    Sint16x4,
    Unorm16x2,
    Unorm16x4,
    Snorm16x2,
    Snorm16x4,
    Float16x2,
    Float16x4,
    Float32,
    #[default]
    Float32x2,
    Float32x3,
    Float32x4,
    Uint32,
    Uint32x2,
    Uint32x3,
    Uint32x4,
    Sint32,
    Sint32x2,
    Sint32x3,
    Sint32x4,
    Float64,
    Float64x2,
    Float64x3,
    Float64x4,
}

impl From<VertexFormat> for wgpu::VertexFormat {
    fn from(f: VertexFormat) -> Self {
        match f {
            VertexFormat::Uint8x2 => wgpu::VertexFormat::Uint8x2,
            VertexFormat::Uint8x4 => wgpu::VertexFormat::Uint8x4,
            VertexFormat::Sint8x2 => wgpu::VertexFormat::Sint8x2,
            VertexFormat::Sint8x4 => wgpu::VertexFormat::Sint8x4,
            VertexFormat::Unorm8x2 => wgpu::VertexFormat::Unorm8x2,
            VertexFormat::Unorm8x4 => wgpu::VertexFormat::Unorm8x4,
            VertexFormat::Snorm8x2 => wgpu::VertexFormat::Snorm8x2,
            VertexFormat::Snorm8x4 => wgpu::VertexFormat::Snorm8x4,
            VertexFormat::Uint16x2 => wgpu::VertexFormat::Uint16x2,
            VertexFormat::Uint16x4 => wgpu::VertexFormat::Uint16x4,
            VertexFormat::Sint16x2 => wgpu::VertexFormat::Sint16x2,
            VertexFormat::Sint16x4 => wgpu::VertexFormat::Sint16x4,
            VertexFormat::Unorm16x2 => wgpu::VertexFormat::Unorm16x2,
            VertexFormat::Unorm16x4 => wgpu::VertexFormat::Unorm16x4,
            VertexFormat::Snorm16x2 => wgpu::VertexFormat::Snorm16x2,
            VertexFormat::Snorm16x4 => wgpu::VertexFormat::Snorm16x4,
            VertexFormat::Float16x2 => wgpu::VertexFormat::Float16x2,
            VertexFormat::Float16x4 => wgpu::VertexFormat::Float16x4,
            VertexFormat::Float32 => wgpu::VertexFormat::Float32,
            VertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
            VertexFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
            VertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
            VertexFormat::Uint32 => wgpu::VertexFormat::Uint32,
            VertexFormat::Uint32x2 => wgpu::VertexFormat::Uint32x2,
            VertexFormat::Uint32x3 => wgpu::VertexFormat::Uint32x3,
            VertexFormat::Uint32x4 => wgpu::VertexFormat::Uint32x4,
            VertexFormat::Sint32 => wgpu::VertexFormat::Sint32,
            VertexFormat::Sint32x2 => wgpu::VertexFormat::Sint32x2,
            VertexFormat::Sint32x3 => wgpu::VertexFormat::Sint32x3,
            VertexFormat::Sint32x4 => wgpu::VertexFormat::Sint32x4,
            VertexFormat::Float64 => wgpu::VertexFormat::Float64,
            VertexFormat::Float64x2 => wgpu::VertexFormat::Float64x2,
            VertexFormat::Float64x3 => wgpu::VertexFormat::Float64x3,
            VertexFormat::Float64x4 => wgpu::VertexFormat::Float64x4,
        }
    }
}

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
pub enum VertexStepMode {
    #[default]
    Vertex,
    Instance,
}

impl From<VertexStepMode> for wgpu::VertexStepMode {
    fn from(m: VertexStepMode) -> Self {
        match m {
            VertexStepMode::Vertex => wgpu::VertexStepMode::Vertex,
            VertexStepMode::Instance => wgpu::VertexStepMode::Instance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VertexAttribute {
    pub format: VertexFormat,
    pub offset: u64,
    pub shader_location: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VertexBufferLayout {
    pub array_stride: u64,
    pub step_mode: VertexStepMode,
    pub attributes: Vec<VertexAttribute>,
}
