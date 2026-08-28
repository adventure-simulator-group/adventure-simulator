use crate::prelude::*;

use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct ScanDefinition {
    pub code: String,
    pub cache: Arc<RwLock<Option<(ComputePipeline, ComputePipeline, String)>>>,
}

impl Default for ScanDefinition {
    fn default() -> Self {
        Self {
            code: String::new(),
            cache: Arc::new(RwLock::new(None)),
        }
    }
}

impl PartialEq for ScanDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl ScanDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        let _ = crate::data::gpu::compute::signature::parse_binary_op_signature(&code, "scan")?;
        Ok(Self {
            code,
            cache: Arc::new(RwLock::new(None)),
        })
    }

    pub fn get_or_create_pipelines(
        &self,
        context: &WgpuContext,
    ) -> Result<(ComputePipeline, ComputePipeline, String)> {
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.as_ref() {
                return Ok(p.clone());
            }
        }

        let ty_base =
            crate::data::gpu::compute::signature::parse_binary_op_signature(&self.code, "scan")?;
        let ty = ty_base.as_str().to_string();

        let block_size = 256;

        // Pass 1: Local Scan & Block Sums
        let scan_blocks_code = format!(
            r#"@group(0) @binding(0) var<storage, read> input: array<{ty}>;
@group(0) @binding(1) var<storage, read_write> output: array<{ty}>;
@group(0) @binding(2) var<storage, read_write> aux: array<{ty}>;

var<workgroup> shared_data: array<{ty}, {block_size}>;

{code}

@compute @workgroup_size({block_size})
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group_id: vec3<u32>
) {{
    let input_len = arrayLength(&input);
    let index = global_id.x;

    // Load into shared memory
    if (index < input_len) {{
        shared_data[local_id.x] = input[index];
    }}
    workgroupBarrier();

    // Hillis-Steele inclusive scan within workgroup
    for (var offset = 1u; offset < {block_size}u; offset *= 2u) {{
        var t: {ty};
        if (local_id.x >= offset) {{
            t = scan(shared_data[local_id.x - offset], shared_data[local_id.x]);
        }}
        workgroupBarrier();
        if (local_id.x >= offset) {{
            shared_data[local_id.x] = t;
        }}
        workgroupBarrier();
    }}

    // Write to output and aux
    if (index < input_len) {{
        output[index] = shared_data[local_id.x];
    }}
    if (local_id.x == {block_size}u - 1u && group_id.x < arrayLength(&aux)) {{
        aux[group_id.x] = shared_data[local_id.x];
    }}

    // Handle the case where the last block is partially full
    if (index == input_len - 1u && local_id.x != {block_size}u - 1u && group_id.x < arrayLength(&aux)) {{
        aux[group_id.x] = shared_data[local_id.x];
    }}
}}
"#,
            ty = ty,
            code = self.code,
            block_size = block_size
        );

        // Pass 3: Add Block Sums to Output
        let add_aux_code = format!(
            r#"@group(0) @binding(0) var<storage, read_write> output: array<{ty}>;
@group(0) @binding(1) var<storage, read> aux: array<{ty}>;

{code}

@compute @workgroup_size({block_size})
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group_id: vec3<u32>
) {{
    if (group_id.x > 0u && global_id.x < arrayLength(&output)) {{
        let block_sum = aux[group_id.x - 1u];
        output[global_id.x] = scan(block_sum, output[global_id.x]);
    }}
}}
"#,
            ty = ty,
            code = self.code,
            block_size = block_size
        );

        let scan_blocks_shader = ComputeShader::new(context, scan_blocks_code)?;
        let add_aux_shader = ComputeShader::new(context, add_aux_code)?;

        let p1 = ComputePipeline::new(context, scan_blocks_shader)?;
        let p2 = ComputePipeline::new(context, add_aux_shader)?;

        let res = (p1, p2, ty);
        let mut cache = self.cache.write().unwrap();
        *cache = Some(res.clone());
        Ok(res)
    }
}

pub struct Scan;

impl Scan {
    pub fn execute(
        context: &WgpuContext,
        definition: &ScanDefinition,
        input: &crate::data::gpu::Buffer,
    ) -> Result<crate::data::gpu::Buffer> {
        let (scan_blocks_pipeline, add_aux_pipeline, _element_type) =
            definition.get_or_create_pipelines(context)?;

        let num_elements = (input.size / 4) as u32;
        if num_elements == 0 {
            return Ok(input.clone());
        }

        let block_size = 256;
        let num_blocks = num_elements.div_ceil(block_size);

        // Output buffer
        let output = crate::data::gpu::Buffer::new(
            context,
            input.size,
            crate::data::BufferDefinition::storage()
                .with_label("output")
                .with_copy_src()
                .with_copy_dst(),
        )?;

        if num_blocks <= 1 {
            let mut parameters = crate::data::gpu::parameters::PassParameters::new();
            parameters.insert("input", input.clone());
            parameters.insert("output", output.clone());

            let aux = crate::data::gpu::Buffer::new(
                context,
                4,
                crate::data::BufferDefinition::storage()
                    .with_label("aux")
                    .with_copy_src()
                    .with_copy_dst(),
            )?;
            parameters.insert("aux", aux);

            crate::data::gpu::compute::ComputePass::execute(
                context,
                scan_blocks_pipeline,
                parameters,
                1,
                1,
                1,
            )?;
        } else {
            let aux = crate::data::gpu::Buffer::new(
                context,
                (num_blocks as u64) * 4,
                crate::data::BufferDefinition::storage()
                    .with_label("aux")
                    .with_copy_src()
                    .with_copy_dst(),
            )?;

            // Pass 1
            let mut parameters_p1 = crate::data::gpu::parameters::PassParameters::new();
            parameters_p1.insert("input", input.clone());
            parameters_p1.insert("output", output.clone());
            parameters_p1.insert("aux", aux.clone());

            crate::data::gpu::compute::ComputePass::execute(
                context,
                scan_blocks_pipeline.clone(),
                parameters_p1,
                num_blocks,
                1,
                1,
            )?;

            // Pass 2
            let mut aux_buffers = vec![aux.clone()];
            let mut current_num_blocks = num_blocks;

            while current_num_blocks > 1 {
                let next_num_blocks = current_num_blocks.div_ceil(block_size);
                let next_aux = crate::data::gpu::Buffer::new(
                    context,
                    (next_num_blocks as u64) * 4,
                    crate::data::BufferDefinition::storage()
                        .with_label("next_aux")
                        .with_copy_src()
                        .with_copy_dst(),
                )?;

                let scanned_aux = crate::data::gpu::Buffer::new(
                    context,
                    (current_num_blocks as u64) * 4,
                    crate::data::BufferDefinition::storage()
                        .with_label("scanned_aux")
                        .with_copy_src()
                        .with_copy_dst(),
                )?;

                let mut parameters_p2 = crate::data::gpu::parameters::PassParameters::new();
                let last_aux = aux_buffers.last().unwrap();
                parameters_p2.insert("input", last_aux.clone());
                parameters_p2.insert("output", scanned_aux.clone());
                parameters_p2.insert("aux", next_aux.clone());

                crate::data::gpu::compute::ComputePass::execute(
                    context,
                    scan_blocks_pipeline.clone(),
                    parameters_p2,
                    next_num_blocks,
                    1,
                    1,
                )?;

                let last_idx = aux_buffers.len() - 1;
                aux_buffers[last_idx] = scanned_aux;

                aux_buffers.push(next_aux);
                current_num_blocks = next_num_blocks;
            }

            // Pass 3
            aux_buffers.pop();

            while let Some(current_aux) = aux_buffers.pop() {
                let mut parameters_p3 = crate::data::gpu::parameters::PassParameters::new();

                if aux_buffers.is_empty() {
                    parameters_p3.insert("output", output.clone());
                    parameters_p3.insert("aux", current_aux.clone());

                    crate::data::gpu::compute::ComputePass::execute(
                        context,
                        add_aux_pipeline.clone(),
                        parameters_p3,
                        num_blocks,
                        1,
                        1,
                    )?;
                } else {
                    let target = aux_buffers.last().unwrap();
                    parameters_p3.insert("output", target.clone());
                    parameters_p3.insert("aux", current_aux.clone());

                    let target_blocks = (target.size / 4) as u32;
                    crate::data::gpu::compute::ComputePass::execute(
                        context,
                        add_aux_pipeline.clone(),
                        parameters_p3,
                        target_blocks,
                        1,
                        1,
                    )?;
                }
            }
        }

        Ok(output)
    }
}
