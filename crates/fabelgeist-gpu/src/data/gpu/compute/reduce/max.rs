use super::{Reduce, ReduceDefinition};
use crate::data::gpu::compute::signature::ResourceBaseType;
use crate::prelude::*;

pub struct Max;

impl Max {
    pub fn execute(
        context: &WgpuContext,
        input: &crate::data::gpu::resource::GpuResource,
        custom_code: Option<&str>,
        scratchpad: &mut super::ReduceScratchpad,
    ) -> Result<crate::data::gpu::Buffer> {
        let code = if let Some(custom) = custom_code {
            let ty = crate::data::gpu::compute::signature::parse_compare_signature(custom, "max")?;
            let ty_str = ty.as_str();
            format!(
                r#"
{}
fn reduce(a: {}, b: {}) -> {} {{
    if (max_cmp(a, b)) {{ return a; }} else {{ return b; }}
}}
"#,
                custom.replace("fn max(", "fn max_cmp("),
                ty_str,
                ty_str,
                ty_str
            )
        } else {
            let ty_str = input.base_type().as_str();
            format!(
                "fn reduce(a: {}, b: {}) -> {} {{ return max(a, b); }}",
                ty_str, ty_str, ty_str
            )
        };

        let definition = ReduceDefinition::new(context, code)?;
        Reduce::execute(context, &definition, input, scratchpad)
    }

    pub async fn execute_to_number(
        context: &WgpuContext,
        input: &crate::data::gpu::resource::GpuResource,
        scratchpad: &mut super::ReduceScratchpad,
    ) -> Result<f64> {
        let buffer = Self::execute(context, input, None, scratchpad)?;
        let base_type = input.base_type();

        match base_type.base_type() {
            ResourceBaseType::F32 => {
                let data: Vec<f32> = buffer.read(context).await?;
                Ok(data[0] as f64)
            }
            ResourceBaseType::U32 => {
                let data: Vec<u32> = buffer.read(context).await?;
                Ok(data[0] as f64)
            }
            ResourceBaseType::I32 => {
                let data: Vec<i32> = buffer.read(context).await?;
                Ok(data[0] as f64)
            }
            _ => Err(anyhow!(
                "Reduction to a single Number is only supported for scalar types (f32, u32, i32) or vectors of them. Got: {}",
                base_type.as_str()
            )),
        }
    }
}
