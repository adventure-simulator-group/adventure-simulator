use crate::prelude::*;

use crate::data::gpu::resource::GpuResource;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, Default)]
pub struct MatMulDefinition {
    pub cache: Arc<RwLock<Option<ComputePipeline>>>,
}

impl PartialEq for MatMulDefinition {
    fn eq(&self, _other: &Self) -> bool {
        true // Or based on configuration if we had any
    }
}

impl MatMulDefinition {
    pub fn new(_context: &WgpuContext, _code: String) -> Result<Self> {
        Ok(Self {
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

        let shader_code = "
            @group(0) @binding(0) var<storage, read> input_a: array<f32>;
            @group(0) @binding(1) var<storage, read> input_b: array<f32>;
            @group(0) @binding(2) var<storage, read_write> output: array<f32>;
            @compute @workgroup_size(64)
            fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
                output[global_id.x] = input_a[global_id.x];
            }
        ";
        let shader = ComputeShader::new(context, shader_code.to_string())?;
        let pipeline = crate::data::gpu::compute::build_compute_pipeline(context, &shader, "main")?;

        let mut cache = self.cache.write().unwrap();
        *cache = Some(pipeline.clone());
        Ok(pipeline)
    }
}

pub struct MatMul;

impl MatMul {
    pub fn execute(
        context: &WgpuContext,
        definition: &MatMulDefinition,
        input_a: &GpuResource,
        input_b: &GpuResource,
        output: &GpuResource,
    ) -> Result<()> {
        let pipeline = definition.get_or_create_pipeline(context)?;

        let mut parameters = crate::data::gpu::parameters::PassParameters::new();
        match input_a {
            GpuResource::Buffer(b) => parameters.insert("input_a", b.clone()),
            _ => return Err(anyhow::anyhow!("MatMul input_a must be a buffer")),
        }
        match input_b {
            GpuResource::Buffer(b) => parameters.insert("input_b", b.clone()),
            _ => return Err(anyhow::anyhow!("MatMul input_b must be a buffer")),
        }
        match output {
            GpuResource::Buffer(b) => parameters.insert("output", b.clone()),
            _ => return Err(anyhow::anyhow!("MatMul output must be a buffer")),
        }

        let wg_x = match output {
            GpuResource::Buffer(b) => ((b.size / 4) as u32).div_ceil(64),
            _ => 1,
        };

        crate::data::gpu::compute::ComputePass::new(context, pipeline, parameters, wg_x, 1, 1)?;

        Ok(())
    }
}
