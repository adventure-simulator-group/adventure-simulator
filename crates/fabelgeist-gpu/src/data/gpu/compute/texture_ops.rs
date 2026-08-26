use crate::data::gpu::parameters::PassParameter;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct TextureBinaryOpDefinition {
    pub op_code: String,
    pub cache: Arc<
        RwLock<
            HashMap<
                (ResourceDescriptor, ResourceDescriptor, ResourceDescriptor),
                Arc<ComputePipeline>,
            >,
        >,
    >,
}

impl PartialEq for TextureBinaryOpDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.op_code == other.op_code
    }
}

impl Eq for TextureBinaryOpDefinition {}

impl std::hash::Hash for TextureBinaryOpDefinition {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.op_code.hash(state);
    }
}

impl TextureBinaryOpDefinition {
    pub fn new(op_code: impl ToString) -> Self {
        Self {
            op_code: op_code.to_string(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        input_a_res: ResourceDescriptor,
        input_b_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<ComputePipeline> {
        let mut full_code = String::new();

        full_code.push_str(&input_a_res.to_wgsl_input_binding(0, 0, "input_a"));
        full_code.push_str(&input_b_res.to_wgsl_input_binding(0, 1, "input_b"));
        full_code.push_str(&output_res.to_wgsl_output_binding(0, 2, "output"));

        full_code.push_str("\nstruct Parameters {\n");
        full_code.push_str("    amount: f32,\n");
        full_code.push_str("    _pad: vec3<f32>,\n");
        full_code.push_str("};\n");
        full_code.push_str("@group(0) @binding(3) var<uniform> _params: Parameters;\n\n");

        full_code.push_str("@compute @workgroup_size(16, 16, 1)\n");
        full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
        full_code.push_str("    let tex_dim = textureDimensions(output);\n");
        full_code.push_str(
            "    if (global_id.x >= tex_dim.x || global_id.y >= tex_dim.y) { return; }\n",
        );

        // Load input_a
        let a_base = input_a_res.base_type_str();
        if a_base == "f32" {
            full_code.push_str("    let a = textureLoad(input_a, global_id.xy, 0);\n");
        } else {
            full_code.push_str("    let a = vec4<f32>(textureLoad(input_a, global_id.xy, 0));\n");
        }

        // Load input_b
        let b_base = input_b_res.base_type_str();
        if b_base == "f32" {
            full_code.push_str("    let b = textureLoad(input_b, global_id.xy, 0);\n");
        } else {
            full_code.push_str("    let b = vec4<f32>(textureLoad(input_b, global_id.xy, 0));\n");
        }

        full_code.push_str("    let amount = _params.amount;\n");

        full_code.push_str(&format!("    let res = {};\n", self.op_code));

        // Store output
        let out_base = output_res.base_type_str();
        if out_base == "u32" {
            full_code.push_str("    textureStore(output, global_id.xy, vec4<u32>(res));\n");
        } else if out_base == "i32" {
            full_code.push_str("    textureStore(output, global_id.xy, vec4<i32>(res));\n");
        } else {
            full_code.push_str("    textureStore(output, global_id.xy, res);\n");
        }
        full_code.push_str("}\n");

        let shader = ComputeShader::new(context, full_code)?;
        ComputePipeline::new(context, shader)
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        input_a_res: ResourceDescriptor,
        input_b_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<Arc<ComputePipeline>> {
        let key = (input_a_res.clone(), input_b_res.clone(), output_res.clone());
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&key) {
                return Ok(p.clone());
            }
        }

        let pipeline = self.build_pipeline(context, input_a_res, input_b_res, output_res)?;
        let arc_p = Arc::new(pipeline);

        let mut cache = self.cache.write().unwrap();
        cache.insert(key, arc_p.clone());
        Ok(arc_p)
    }
}

pub struct TextureBinaryOp;

impl TextureBinaryOp {
    pub fn execute(
        context: &WgpuContext,
        definition: &TextureBinaryOpDefinition,
        input_a: &GpuResource,
        input_b: &GpuResource,
        amount: f32,
        output: &GpuResource,
    ) -> Result<()> {
        let a_descriptor = ResourceDescriptor::from_resource(
            input_a,
            ResourceBaseType::Vec4(Box::new(ResourceBaseType::F32)),
        );
        let b_descriptor = ResourceDescriptor::from_resource(
            input_b,
            ResourceBaseType::Vec4(Box::new(ResourceBaseType::F32)),
        );
        let output_descriptor = ResourceDescriptor::from_resource(
            output,
            ResourceBaseType::Vec4(Box::new(ResourceBaseType::F32)),
        );

        let pipeline = definition.get_or_create_pipeline(
            context,
            a_descriptor,
            b_descriptor,
            output_descriptor,
        )?;

        let mut parameters = crate::data::gpu::parameters::PassParameters::new();
        parameters.insert(
            "input_a",
            match input_a {
                GpuResource::Texture2d(t) => PassParameter::Texture2d(t.clone()),
                _ => return Err(anyhow!("Input A must be a Texture2d")),
            },
        );
        parameters.insert(
            "input_b",
            match input_b {
                GpuResource::Texture2d(t) => PassParameter::Texture2d(t.clone()),
                _ => return Err(anyhow!("Input B must be a Texture2d")),
            },
        );
        parameters.insert(
            "output",
            match output {
                GpuResource::Texture2d(t) => PassParameter::Texture2d(t.clone()),
                _ => return Err(anyhow!("Output must be a Texture2d")),
            },
        );

        parameters.insert("amount", amount);

        let (wg_x, wg_y, wg_z) = match output {
            GpuResource::Texture2d(t) => (t.size.0.div_ceil(16), t.size.1.div_ceil(16), 1),
            _ => unreachable!(),
        };

        crate::data::gpu::compute::ComputePass::new(
            context,
            pipeline.as_ref().clone(),
            parameters,
            wg_x,
            wg_y,
            wg_z,
        )?;

        Ok(())
    }
}
