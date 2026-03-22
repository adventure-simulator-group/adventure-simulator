use crate::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReduceDefinition {
    pub shader: ComputeShader,
}

impl ReduceDefinition {
    pub fn new(context: &WgpuContext, code: String) -> Result<Self> {
        let full_code = format!(
            r#"@group(0) @binding(0) var<storage, read> input: array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;

var<workgroup> shared_data: array<f32, 64>;

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
            code
        );
        let shader = ComputeShader::new(context, full_code)?;
        Ok(Self { shader })
    }
}
