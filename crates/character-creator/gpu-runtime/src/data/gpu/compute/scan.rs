use crate::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ScanDefinition {
    pub scan_blocks_shader: ComputeShader,
    pub add_aux_shader: ComputeShader,
    pub element_type: String,
}

impl ScanDefinition {
    pub fn new(context: &WgpuContext, code: String) -> Result<Self> {
        let ty_base = crate::data::gpu::compute::signature::parse_binary_op_signature(&code, "scan")?;
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
            code = code,
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
            code = code,
            block_size = block_size
        );

        let scan_blocks_shader = ComputeShader::new(context, scan_blocks_code)?;
        let add_aux_shader = ComputeShader::new(context, add_aux_code)?;

        Ok(Self { scan_blocks_shader, add_aux_shader, element_type: ty })
    }
}
