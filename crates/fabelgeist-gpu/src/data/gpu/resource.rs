use crate::data::gpu::buffer::Buffer;
use crate::data::gpu::compute::signature::ResourceBaseType;
use crate::data::gpu::texture::{Texture2d, Texture3d};

#[derive(Clone, Debug, PartialEq)]
pub enum GpuResource {
    Buffer(Buffer),
    Texture2d(Texture2d),
    Texture3d(Texture3d),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Buffer,
    Texture2d,
    Texture3d,
}

impl GpuResource {
    pub fn base_type(&self) -> ResourceBaseType {
        match self {
            GpuResource::Buffer(_) => ResourceBaseType::F32, // Default to f32 for buffers
            GpuResource::Texture2d(t) => ResourceBaseType::from_texture_format(t.format),
            GpuResource::Texture3d(t) => ResourceBaseType::from_texture_format(t.format),
        }
    }

    pub fn resource_type(&self) -> ResourceType {
        match self {
            GpuResource::Buffer(_) => ResourceType::Buffer,
            GpuResource::Texture2d(_) => ResourceType::Texture2d,
            GpuResource::Texture3d(_) => ResourceType::Texture3d,
        }
    }

    pub fn as_buffer(&self) -> Option<&Buffer> {
        if let GpuResource::Buffer(b) = self {
            Some(b)
        } else {
            None
        }
    }

    pub fn as_texture_2d(&self) -> Option<&Texture2d> {
        if let GpuResource::Texture2d(t) = self {
            Some(t)
        } else {
            None
        }
    }

    pub fn as_texture_3d(&self) -> Option<&Texture3d> {
        if let GpuResource::Texture3d(t) = self {
            Some(t)
        } else {
            None
        }
    }

    pub async fn read<T: bytemuck::AnyBitPattern>(
        &self,
        context: &crate::globals::WgpuContext,
    ) -> anyhow::Result<Vec<T>> {
        match self {
            GpuResource::Buffer(b) => b.read(context).await,
            GpuResource::Texture2d(t) => t.read(context).await,
            GpuResource::Texture3d(t) => t.read(context).await,
        }
    }

    pub fn write<T: bytemuck::NoUninit>(
        &self,
        context: &crate::globals::WgpuContext,
        data: &[T],
    ) -> anyhow::Result<()> {
        match self {
            GpuResource::Buffer(b) => b.write(context, data),
            GpuResource::Texture2d(t) => t.write(context, data),
            GpuResource::Texture3d(t) => t.write(context, data),
        }
    }
}

impl From<Buffer> for GpuResource {
    fn from(b: Buffer) -> Self {
        GpuResource::Buffer(b)
    }
}

impl From<Texture2d> for GpuResource {
    fn from(t: Texture2d) -> Self {
        GpuResource::Texture2d(t)
    }
}

impl From<Texture3d> for GpuResource {
    fn from(t: Texture3d) -> Self {
        GpuResource::Texture3d(t)
    }
}
