use crate::data::gpu::buffer::Buffer;
use crate::data::gpu::texture::{Texture2D, Texture3D};

#[derive(Clone, Debug, PartialEq)]
pub enum GpuResource {
    Buffer(Buffer),
    Texture2D(Texture2D),
    Texture3D(Texture3D),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Buffer,
    Texture2D,
    Texture3D,
}

impl GpuResource {
    pub fn resource_type(&self) -> ResourceType {
        match self {
            GpuResource::Buffer(_) => ResourceType::Buffer,
            GpuResource::Texture2D(_) => ResourceType::Texture2D,
            GpuResource::Texture3D(_) => ResourceType::Texture3D,
        }
    }

    pub fn as_buffer(&self) -> Option<&Buffer> {
        if let GpuResource::Buffer(b) = self {
            Some(b)
        } else {
            None
        }
    }

    pub fn as_texture_2d(&self) -> Option<&Texture2D> {
        if let GpuResource::Texture2D(t) = self {
            Some(t)
        } else {
            None
        }
    }

    pub fn as_texture_3d(&self) -> Option<&Texture3D> {
        if let GpuResource::Texture3D(t) = self {
            Some(t)
        } else {
            None
        }
    }

    pub async fn read<T: bytemuck::AnyBitPattern>(&self, context: &crate::globals::WgpuContext) -> anyhow::Result<Vec<T>> {
        match self {
            GpuResource::Buffer(b) => b.read(context).await,
            GpuResource::Texture2D(t) => t.read(context).await,
            GpuResource::Texture3D(t) => t.read(context).await,
        }
    }

    pub fn write<T: bytemuck::NoUninit>(&self, context: &crate::globals::WgpuContext, data: &[T]) -> anyhow::Result<()> {
        match self {
            GpuResource::Buffer(b) => b.write(context, data),
            GpuResource::Texture2D(t) => t.write(context, data),
            GpuResource::Texture3D(t) => t.write(context, data),
        }
    }
}

impl From<Buffer> for GpuResource {
    fn from(b: Buffer) -> Self {
        GpuResource::Buffer(b)
    }
}

impl From<Texture2D> for GpuResource {
    fn from(t: Texture2D) -> Self {
        GpuResource::Texture2D(t)
    }
}

impl From<Texture3D> for GpuResource {
    fn from(t: Texture3D) -> Self {
        GpuResource::Texture3D(t)
    }
}
