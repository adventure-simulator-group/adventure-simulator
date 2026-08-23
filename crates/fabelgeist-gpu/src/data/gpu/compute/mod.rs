pub mod advancing_front;
pub mod advect;
pub mod broadcast;
pub mod distance_field;
pub mod distance_field_jfa;
pub mod divergence;
pub mod dual_contouring;
pub mod gather;
pub mod gradient;
pub mod map;
pub mod marching_cubes;
pub mod matmul;
mod pass;
pub mod perlin_noise;
mod pipeline;
pub mod reduce;
pub mod reshape;
pub mod scan;
pub mod scatter;
pub mod signature;
pub mod simplex_noise;
pub mod stencil;
pub mod stream;
pub mod texture_ops;
pub mod transpose;

#[cfg(test)]
pub mod test_utils;

pub use advect::{Advect, AdvectDefinition};
pub use distance_field::DistanceField;
pub use distance_field_jfa::DistanceFieldJfa;
pub use divergence::{Divergence, DivergenceDefinition};
pub use gather::*;
pub use gradient::{Gradient, GradientDefinition};
pub use map::{MapDefinition, MapSignature};
pub use perlin_noise::RenderPerlin;
pub use reduce::{Max, Min, ReduceDefinition};
pub use scatter::*;
pub use simplex_noise::RenderSimplex;
pub use texture_ops::{TextureBinaryOp, TextureBinaryOpDefinition};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResourceDescriptor {
    Buffer(crate::data::gpu::compute::signature::ResourceBaseType),
    Texture2d(crate::data::gpu::texture::TextureFormat),
    Texture3d(crate::data::gpu::texture::TextureFormat),
}

impl ResourceDescriptor {
    pub fn from_resource(
        res: &crate::data::gpu::resource::GpuResource,
        sig_type: crate::data::gpu::compute::signature::ResourceBaseType,
    ) -> Self {
        match res {
            crate::data::gpu::resource::GpuResource::Buffer(_) => {
                ResourceDescriptor::Buffer(sig_type)
            }
            crate::data::gpu::resource::GpuResource::Texture2d(t) => {
                ResourceDescriptor::Texture2d(t.format)
            }
            crate::data::gpu::resource::GpuResource::Texture3d(t) => {
                ResourceDescriptor::Texture3d(t.format)
            }
        }
    }

    pub fn to_wgsl_storage_format(&self) -> String {
        match self {
            ResourceDescriptor::Buffer(ty) => ty.as_str(),
            ResourceDescriptor::Texture2d(f) | ResourceDescriptor::Texture3d(f) => {
                f.to_wgsl_storage_format().to_string()
            }
        }
    }

    pub fn to_wgsl_input_binding(&self, group: u32, binding: u32, name: &str) -> String {
        match self {
            ResourceDescriptor::Buffer(ty) => format!(
                "@group({}) @binding({}) var<storage, read> {}: array<{}>;\n",
                group,
                binding,
                name,
                ty.as_str()
            ),
            ResourceDescriptor::Texture2d(_) => format!(
                "@group({}) @binding({}) var {}: texture_2d<{}>;\n",
                group,
                binding,
                name,
                self.base_type_str()
            ),
            ResourceDescriptor::Texture3d(_) => format!(
                "@group({}) @binding({}) var {}: texture_3d<{}>;\n",
                group,
                binding,
                name,
                self.base_type_str()
            ),
        }
    }

    pub fn to_wgsl_output_binding(&self, group: u32, binding: u32, name: &str) -> String {
        match self {
            ResourceDescriptor::Buffer(ty) => format!(
                "@group({}) @binding({}) var<storage, read_write> {}: array<{}>;\n",
                group,
                binding,
                name,
                ty.as_str()
            ),
            ResourceDescriptor::Texture2d(_) => format!(
                "@group({}) @binding({}) var {}: texture_storage_2d<{}, write>;\n",
                group,
                binding,
                name,
                self.to_wgsl_storage_format()
            ),
            ResourceDescriptor::Texture3d(_) => format!(
                "@group({}) @binding({}) var {}: texture_storage_3d<{}, write>;\n",
                group,
                binding,
                name,
                self.to_wgsl_storage_format()
            ),
        }
    }

    pub fn base_type_str(&self) -> &str {
        match self {
            ResourceDescriptor::Buffer(ty) => ty.base_type().as_str().leak(), // leak for static str for now
            ResourceDescriptor::Texture2d(f) | ResourceDescriptor::Texture3d(f) => {
                if f.is_float() {
                    "f32"
                } else if f.is_uint() {
                    "u32"
                } else {
                    "i32"
                }
            }
        }
    }

    pub fn generate_prologue(&self) -> String {
        let mut code = String::new();
        match self {
            ResourceDescriptor::Buffer(_) => {
                code.push_str("    let _global_index = global_id.x;\n");
                code.push_str("    if (_global_index >= arrayLength(&output)) { return; }\n");
            }
            ResourceDescriptor::Texture2d(_) => {
                code.push_str("    let _global_index = global_id.xy;\n");
                code.push_str("    let tex_dim = textureDimensions(output);\n");
                code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y) { return; }\n");
            }
            ResourceDescriptor::Texture3d(_) => {
                code.push_str("    let _global_index = global_id;\n");
                code.push_str("    let tex_dim = textureDimensions(output);\n");
                code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y || _global_index.z >= tex_dim.z) { return; }\n");
            }
        }
        code
    }

    pub fn generate_fetch(
        &self,
        input_name: &str,
        output_res: &ResourceDescriptor,
        element_type: &ResourceBaseType,
    ) -> String {
        let mut code = String::new();
        match self {
            ResourceDescriptor::Buffer(_) => {
                match output_res {
                    ResourceDescriptor::Buffer(_) => {
                        code.push_str("    let _in_global_index = global_id.x;\n");
                    }
                    ResourceDescriptor::Texture2d(_) => {
                        code.push_str("    let _fetch_out_tex_dim = textureDimensions(output);\n");
                        code.push_str("    let _in_global_index = global_id.y * _fetch_out_tex_dim.x + global_id.x;\n");
                    }
                    ResourceDescriptor::Texture3d(_) => {
                        code.push_str("    let _fetch_out_tex_dim = textureDimensions(output);\n");
                        code.push_str("    let _in_global_index = (global_id.z * _fetch_out_tex_dim.y + global_id.y) * _fetch_out_tex_dim.x + global_id.x;\n");
                    }
                }
                code.push_str(&format!(
                    "    let _in_global_index_safe = _in_global_index % arrayLength(&{});\n",
                    input_name
                ));
                code.push_str(&format!(
                    "    let in_val = {}[_in_global_index_safe];\n",
                    input_name
                ));
            }
            ResourceDescriptor::Texture2d(_) => {
                code.push_str("    let _in_global_index = global_id.xy;\n");
                code.push_str(&format!(
                    "    let in_tex_dim = textureDimensions({});\n",
                    input_name
                ));
                code.push_str("    if (_in_global_index.x >= in_tex_dim.x || _in_global_index.y >= in_tex_dim.y) { return; }\n");
                let swizzle = match element_type.component_count() {
                    1 => ".x",
                    2 => ".xy",
                    _ => "",
                };
                code.push_str(&format!(
                    "    let in_val = textureLoad({}, _in_global_index, 0){};\n",
                    input_name, swizzle
                ));
            }
            ResourceDescriptor::Texture3d(_) => {
                code.push_str("    let _in_global_index = global_id;\n");
                code.push_str(&format!(
                    "    let in_tex_dim = textureDimensions({});\n",
                    input_name
                ));
                code.push_str("    if (_in_global_index.x >= in_tex_dim.x || _in_global_index.y >= in_tex_dim.y || _in_global_index.z >= in_tex_dim.z) { return; }\n");
                let swizzle = match element_type.component_count() {
                    1 => ".x",
                    2 => ".xy",
                    _ => "",
                };
                code.push_str(&format!(
                    "    let in_val = textureLoad({}, _in_global_index, 0){};\n",
                    input_name, swizzle
                ));
            }
        }
        code
    }
}

pub use broadcast::*;
pub use map::*;
pub use matmul::*;
pub use pass::*;
pub use pipeline::*;
pub use reduce::*;
pub use reshape::*;
pub use scan::*;
pub use signature::*;
pub use stencil::*;
pub use stream::*;
pub use transpose::*;

pub fn build_compute_pipeline(
    context: &crate::globals::WgpuContext,
    shader: &pipeline::ComputeShader,
    _entry_point: &str,
) -> anyhow::Result<pipeline::ComputePipeline> {
    pipeline::ComputePipeline::new(context, shader.clone())
}
