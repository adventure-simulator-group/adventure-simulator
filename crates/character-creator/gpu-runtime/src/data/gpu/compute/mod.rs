mod pass;
mod pipeline;
pub mod map;
pub mod gather;
pub mod scatter;
pub mod reduce;
pub mod scan;
pub mod signature;
pub mod stencil;
pub mod reshape;
pub mod transpose;
pub mod broadcast;
pub mod matmul;
pub mod marching_cubes;
pub mod stream;

#[cfg(test)]
pub mod test_utils;


pub use map::{MapDefinition, MapSignature};
pub use gather::*;
pub use scatter::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResourceDescriptor {
    Buffer(crate::data::gpu::compute::signature::ResourceBaseType),
    Texture2D(crate::data::gpu::texture::TextureFormat),
    Texture3D(crate::data::gpu::texture::TextureFormat),
}

impl ResourceDescriptor {
    pub fn from_resource(res: &crate::data::gpu::resource::GpuResource, sig_type: crate::data::gpu::compute::signature::ResourceBaseType) -> Self {
        match res {
            crate::data::gpu::resource::GpuResource::Buffer(_) => ResourceDescriptor::Buffer(sig_type),
            crate::data::gpu::resource::GpuResource::Texture2D(t) => ResourceDescriptor::Texture2D(t.format),
            crate::data::gpu::resource::GpuResource::Texture3D(t) => ResourceDescriptor::Texture3D(t.format),
        }
    }

    pub fn to_wgsl_storage_format(&self) -> String {
        match self {
            ResourceDescriptor::Buffer(ty) => ty.as_str(),
            ResourceDescriptor::Texture2D(f) | ResourceDescriptor::Texture3D(f) => {
                f.to_wgsl_storage_format().to_string()
            }
        }
    }
}

pub use pass::*;
pub use pipeline::*;
pub use map::*;
pub use reduce::*;
pub use scan::*;
pub use signature::*;
pub use stencil::*;
pub use reshape::*;
pub use transpose::*;
pub use broadcast::*;
pub use stream::*;
pub use matmul::*;

pub fn build_compute_pipeline(
    context: &crate::globals::WgpuContext,
    shader: &pipeline::ComputeShader,
    _entry_point: &str,
) -> anyhow::Result<pipeline::ComputePipeline> {
    pipeline::ComputePipeline::new(context, shader.clone())
}
