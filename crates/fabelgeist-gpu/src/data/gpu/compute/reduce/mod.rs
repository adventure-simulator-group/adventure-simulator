pub mod max;
pub mod min;

pub use max::Max;
pub use min::Min;

use crate::data::gpu::compute::ResourceDescriptor;
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct ReduceDefinition {
    pub code: String,
    pub cache: Arc<RwLock<HashMap<ResourceDescriptor, (ComputePipeline, u64)>>>,
}

#[derive(Default)]
pub struct ReduceScratchpad {
    pub a: Option<crate::data::gpu::Buffer>,
    pub b: Option<crate::data::gpu::Buffer>,
}

impl Default for ReduceDefinition {
    fn default() -> Self {
        Self {
            code: String::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl PartialEq for ReduceDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl ReduceDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        let _ = crate::data::gpu::compute::signature::parse_binary_op_signature(&code, "reduce")?;
        Ok(Self {
            code,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
    ) -> Result<(ComputePipeline, u64)> {
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&input_res) {
                return Ok(p.clone());
            }
        }

        let ty =
            crate::data::gpu::compute::signature::parse_binary_op_signature(&self.code, "reduce")?;
        let ty_str = ty.as_str();

        let input_binding = input_res.to_wgsl_input_binding(0, 0, "input");
        let output_binding = format!(
            "@group(0) @binding(1) var<storage, read_write> output: array<{}>;\n",
            ty_str
        );

        let prologue = match input_res {
            ResourceDescriptor::Buffer(_) => "".to_string(),
            ResourceDescriptor::Texture2d(_) => {
                "let _dim = textureDimensions(input);\n".to_string()
            }
            ResourceDescriptor::Texture3d(_) => {
                "let _dim = textureDimensions(input);\n".to_string()
            }
        };

        let input_len_calc = match input_res {
            ResourceDescriptor::Buffer(_) => "arrayLength(&input)".to_string(),
            ResourceDescriptor::Texture2d(_) => "(_dim.x * _dim.y)".to_string(),
            ResourceDescriptor::Texture3d(_) => "(_dim.x * _dim.y * _dim.z)".to_string(),
        };

        // Helper to fetch value from any resource type given a flat index
        let fetch_logic = match input_res {
            ResourceDescriptor::Buffer(_) => "input[index]".to_string(),
            ResourceDescriptor::Texture2d(_) => {
                let swizzle = match ty.component_count() {
                    1 => ".x",
                    2 => ".xy",
                    _ => "",
                };
                format!(
                    "textureLoad(input, vec2<u32>(index % _dim.x, index / _dim.x), 0){}",
                    swizzle
                )
            }
            ResourceDescriptor::Texture3d(_) => {
                let swizzle = match ty.component_count() {
                    1 => ".x",
                    2 => ".xy",
                    _ => "",
                };
                format!(
                    "textureLoad(input, vec3<u32>(index % _dim.x, (index / _dim.x) % _dim.y, index / (_dim.x * _dim.y)), 0){}",
                    swizzle
                )
            }
        };

        let full_code = format!(
            r#"{}
{}

var<workgroup> shared_data: array<{}, 64>;

{}

@compute @workgroup_size(64)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group_id: vec3<u32>
) {{
    {}
    let input_len = {};
    let index = global_id.x;

    if (index < input_len) {{
        shared_data[local_id.x] = {};
    }}

    workgroupBarrier();

    for (var s = 32u; s > 0u; s >>= 1u) {{
        if (local_id.x < s) {{
            let idx_a = local_id.x;
            let idx_b = local_id.x + s;

            let global_idx_b = group_id.x * 64u + idx_b;

            if (global_idx_b < input_len) {{
                shared_data[idx_a] = reduce(shared_data[idx_a], shared_data[idx_b]);
            }}
        }}
        workgroupBarrier();
    }}

    if (local_id.x == 0u) {{
        output[group_id.x] = shared_data[0];
    }}
}}
"#,
            input_binding, output_binding, ty_str, self.code, prologue, input_len_calc, fetch_logic
        );

        let module =
            crate::data::gpu::shader::parse_naga(&full_code, wgpu::naga::ShaderStage::Compute)?;

        let mut element_size = ty.element_size();

        if let Some((_, var)) = module
            .global_variables
            .iter()
            .find(|(_, v)| v.name.as_deref() == Some("output"))
            && let wgpu::naga::TypeInner::Array { base, .. } = module.types[var.ty].inner
        {
            let mut layouter = wgpu::naga::proc::Layouter::default();
            let _ = layouter.update(wgpu::naga::proc::GlobalCtx {
                types: &module.types,
                constants: &module.constants,
                overrides: &module.overrides,
                global_expressions: &module.global_expressions,
            });
            element_size = layouter[base].size as u64;
        }

        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = ComputePipeline::new(context, shader)?;

        let mut cache = self.cache.write().unwrap();
        cache.insert(input_res, (pipeline.clone(), element_size));
        Ok((pipeline, element_size))
    }
}

pub struct Reduce;

impl Reduce {
    pub fn execute(
        context: &WgpuContext,
        definition: &ReduceDefinition,
        input: &crate::data::gpu::resource::GpuResource,
        scratchpad: &mut ReduceScratchpad,
    ) -> Result<crate::data::gpu::Buffer> {
        let ty = crate::data::gpu::compute::signature::parse_binary_op_signature(
            &definition.code,
            "reduce",
        )?;

        let input_res_descriptor = ResourceDescriptor::from_resource(input, ty.clone());
        let (first_pass_pipeline, element_size) =
            definition.get_or_create_pipeline(context, input_res_descriptor)?;

        let buffer_res_descriptor = ResourceDescriptor::Buffer(ty.clone());
        let (standard_pipeline, _) =
            definition.get_or_create_pipeline(context, buffer_res_descriptor)?;

        let mut current_resource = input.clone();
        let mut current_element_count = match input {
            crate::data::gpu::resource::GpuResource::Buffer(b) => {
                (b.size / element_size.max(1)).max(1) as u32
            }
            crate::data::gpu::resource::GpuResource::Texture2d(t) => t.size.0 * t.size.1,
            crate::data::gpu::resource::GpuResource::Texture3d(t) => t.size.0 * t.size.1 * t.size.2,
        };

        if current_element_count <= 1 {
            return match current_resource {
                crate::data::gpu::resource::GpuResource::Buffer(b) => Ok(b),
                _ => Err(anyhow::anyhow!("Cannot reduce texture of size 1 to buffer")),
            };
        }

        // Prepare auxiliary buffers
        let first_pass_output_count = current_element_count.div_ceil(64);
        let first_pass_output_size = first_pass_output_count as u64 * element_size;

        fn ensure_buffer(
            ctx: &WgpuContext,
            buffer_opt: &mut Option<crate::data::gpu::Buffer>,
            required_size: u64,
        ) -> Result<()> {
            let resize = if let Some(buf) = buffer_opt {
                buf.size < required_size
            } else {
                true
            };

            if resize {
                *buffer_opt = Some(crate::data::gpu::Buffer::new(
                    ctx,
                    required_size,
                    crate::data::BufferDefinition::storage()
                        .with_label("scratchpad")
                        .with_copy_src(),
                )?);
            }
            Ok(())
        }

        ensure_buffer(context, &mut scratchpad.a, first_pass_output_size)?;

        if first_pass_output_count > 1 {
            let second_pass_output_count = first_pass_output_count.div_ceil(64);
            ensure_buffer(
                context,
                &mut scratchpad.b,
                second_pass_output_count as u64 * element_size,
            )?;
        }

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Reduce Batch Encoder"),
            });

        let mut pass_idx = 0;
        let mut current_buffer: Option<crate::data::gpu::Buffer> = None;

        while current_element_count > 1 {
            let workgroup_count = current_element_count.div_ceil(64);

            let output_buffer = if pass_idx % 2 == 0 {
                scratchpad.a.as_ref().unwrap()
            } else {
                scratchpad.b.as_ref().unwrap()
            };

            let mut parameters = crate::data::gpu::parameters::PassParameters::new();
            match &current_resource {
                crate::data::gpu::resource::GpuResource::Buffer(b) => {
                    parameters.insert("input", b.clone())
                }
                crate::data::gpu::resource::GpuResource::Texture2d(t) => {
                    parameters.insert("input", t.clone())
                }
                crate::data::gpu::resource::GpuResource::Texture3d(t) => {
                    parameters.insert("input", t.clone())
                }
            }
            parameters.insert("output", output_buffer.clone());

            let pipeline = if pass_idx == 0 {
                &first_pass_pipeline
            } else {
                &standard_pipeline
            };

            crate::data::gpu::compute::ComputePass::record(
                context,
                pipeline,
                &parameters,
                &mut encoder,
                workgroup_count,
                1,
                1,
            )?;

            current_buffer = Some(output_buffer.clone());
            current_resource =
                crate::data::gpu::resource::GpuResource::Buffer(output_buffer.clone());
            current_element_count = workgroup_count;
            pass_idx += 1;
        }

        let final_buffer = crate::data::gpu::Buffer::new(
            context,
            element_size,
            crate::data::BufferDefinition::storage()
                .with_label("reduction_result")
                .with_copy_src()
                .with_copy_dst(),
        )?;

        encoder.copy_buffer_to_buffer(
            &current_buffer
                .expect("Reduce failed to produce output")
                .buffer,
            0,
            &final_buffer.buffer,
            0,
            element_size,
        );

        context.queue.submit(std::iter::once(encoder.finish()));

        Ok(final_buffer)
    }
}

#[cfg(test)]
mod tests;
