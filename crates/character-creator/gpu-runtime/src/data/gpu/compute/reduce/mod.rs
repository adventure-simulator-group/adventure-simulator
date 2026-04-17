use crate::prelude::*;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct ReduceDefinition {
    pub code: String,
    pub cache: Arc<RwLock<Option<ComputePipeline>>>,
}

impl Default for ReduceDefinition {
    fn default() -> Self {
        Self {
            code: String::new(),
            cache: Arc::new(RwLock::new(None)),
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
            cache: Arc::new(RwLock::new(None)),
        })
    }

    pub fn get_or_create_pipeline(&self, context: &WgpuContext) -> Result<ComputePipeline> {
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.as_ref() {
                return Ok(p.clone());
            }
        }

        let ty =
            crate::data::gpu::compute::signature::parse_binary_op_signature(&self.code, "reduce")?;
        let ty_str = ty.as_str();

        let full_code = format!(
            r#"@group(0) @binding(0) var<storage, read> input: array<{}>;
@group(0) @binding(1) var<storage, read_write> output: array<{}>;

var<workgroup> shared_data: array<{}, 64>;

{}

@compute @workgroup_size(64)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group_id: vec3<u32>
) {{
    let input_len = arrayLength(&input);
    let index = global_id.x;
    
    if (index < input_len) {{
        shared_data[local_id.x] = input[index];
    }}
    
    // We need to sync even OOB threads if they participate in the barrier
    workgroupBarrier();

    for (var s = 32u; s > 0u; s >>= 1u) {{
        if (local_id.x < s) {{
            let idx_a = local_id.x;
            let idx_b = local_id.x + s;
            
            // Check if the second element we want to reduce with is within the original array bounds
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
            ty_str, ty_str, ty_str, self.code
        );
        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = ComputePipeline::new(context, shader)?;

        let mut cache = self.cache.write().unwrap();
        *cache = Some(pipeline.clone());
        Ok(pipeline)
    }
}

pub struct Reduce;

impl Reduce {
    pub fn execute(
        context: &WgpuContext,
        definition: &ReduceDefinition,
        input: &crate::data::gpu::Buffer,
        scratchpad_a: &mut Option<crate::data::gpu::Buffer>,
        scratchpad_b: &mut Option<crate::data::gpu::Buffer>,
    ) -> Result<crate::data::gpu::Buffer> {
        let pipeline = definition.get_or_create_pipeline(context)?;

        let mut current_buffer = input.clone();
        let mut current_element_count = (current_buffer.size / 4).max(1) as u32;

        if current_element_count <= 1 {
            return Ok(current_buffer);
        }

        // Prepare auxiliary buffers
        let first_pass_output_count = (current_element_count + 63) / 64;
        let first_pass_output_size = (first_pass_output_count * 4) as u64;

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

        ensure_buffer(context, scratchpad_a, first_pass_output_size)?;

        if first_pass_output_count > 1 {
            let second_pass_output_count = (first_pass_output_count + 63) / 64;
            ensure_buffer(context, scratchpad_b, (second_pass_output_count * 4) as u64)?;
        }

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Reduce Batch Encoder"),
            });

        let mut pass_idx = 0;
        while current_element_count > 1 {
            let workgroup_count = (current_element_count + 63) / 64;

            let output_buffer = if pass_idx % 2 == 0 {
                scratchpad_a.as_ref().unwrap()
            } else {
                scratchpad_b.as_ref().unwrap()
            };

            let mut parameters = crate::data::gpu::parameters::PassParameters::new();
            parameters.insert("input", current_buffer.clone());
            parameters.insert("output", output_buffer.clone());

            crate::data::gpu::compute::ComputePass::record(
                context,
                &pipeline,
                &parameters,
                &mut encoder,
                workgroup_count,
                1,
                1,
            )?;

            current_buffer = output_buffer.clone();
            current_element_count = workgroup_count;
            pass_idx += 1;
        }

        context.queue.submit(std::iter::once(encoder.finish()));

        Ok(current_buffer)
    }
}

#[cfg(test)]
mod tests;
