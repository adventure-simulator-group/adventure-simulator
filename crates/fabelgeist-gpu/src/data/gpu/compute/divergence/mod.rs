use crate::data::gpu::compute::ResourceDescriptor;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct DivergenceDefinition {
    pub boundary_mode: u32,
    pub cache:
        Arc<RwLock<HashMap<(ResourceDescriptor, ResourceDescriptor, u32), Arc<ComputePipeline>>>>,
}

impl DivergenceDefinition {
    pub fn new(boundary_mode: u32) -> Self {
        Self {
            boundary_mode,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for DivergenceDefinition {
    fn default() -> Self {
        Self::new(0)
    }
}

impl DivergenceDefinition {
    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<ComputePipeline> {
        let mut full_code = String::new();

        let dim = match input_res {
            ResourceDescriptor::Texture2d(_) => 2,
            ResourceDescriptor::Texture3d(_) => 3,
            _ => return Err(anyhow!("Divergence only supports Texture2d or Texture3d")),
        };

        full_code.push_str(&input_res.to_wgsl_input_binding(0, 0, "velocity"));
        full_code.push_str(&output_res.to_wgsl_output_binding(0, 1, "output"));

        full_code.push_str("\nstruct Parameters {\n");
        full_code.push_str("    half_inverse_cell_size: f32,\n");
        full_code.push_str("    _pad: vec3<f32>,\n");
        full_code.push_str("};\n");
        full_code.push_str("@group(0) @binding(2) var<uniform> _params: Parameters;\n\n");

        if dim == 2 {
            full_code.push_str("@compute @workgroup_size(16, 16, 1)\n");
            full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
            full_code.push_str("    let tex_dim = textureDimensions(velocity);\n");
            full_code.push_str(
                "    if (global_id.x >= tex_dim.x || global_id.y >= tex_dim.y) { return; }\n",
            );
            full_code.push_str("    let x = i32(global_id.x);\n");
            full_code.push_str("    let y = i32(global_id.y);\n");
            full_code.push_str("    let w = i32(tex_dim.x);\n");
            full_code.push_str("    let h = i32(tex_dim.y);\n");
            if self.boundary_mode == 1 {
                full_code.push_str(
                    "    let L = textureLoad(velocity, vec2<i32>((x - 1 + w) % w, y), 0).x;\n",
                );
                full_code.push_str(
                    "    let R = textureLoad(velocity, vec2<i32>((x + 1) % w, y), 0).x;\n",
                );
                full_code.push_str(
                    "    let B = textureLoad(velocity, vec2<i32>(x, (y - 1 + h) % h), 0).y;\n",
                );
                full_code.push_str(
                    "    let T = textureLoad(velocity, vec2<i32>(x, (y + 1) % h), 0).y;\n",
                );
            } else {
                full_code.push_str(
                    "    let L = textureLoad(velocity, vec2<i32>(max(x - 1, 0), y), 0).x;\n",
                );
                full_code.push_str(
                    "    let R = textureLoad(velocity, vec2<i32>(min(x + 1, w - 1), y), 0).x;\n",
                );
                full_code.push_str(
                    "    let B = textureLoad(velocity, vec2<i32>(x, max(y - 1, 0)), 0).y;\n",
                );
                full_code.push_str(
                    "    let T = textureLoad(velocity, vec2<i32>(x, min(y + 1, h - 1)), 0).y;\n",
                );
            }
            full_code.push_str("    let div = (R - L + T - B) * _params.half_inverse_cell_size;\n");
            full_code.push_str(
                "    textureStore(output, global_id.xy, vec4<f32>(div, 0.0, 0.0, 1.0));\n",
            );
            full_code.push_str("}\n");
        } else {
            full_code.push_str("@compute @workgroup_size(8, 8, 4)\n");
            full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
            full_code.push_str("    let tex_dim = textureDimensions(velocity);\n");
            full_code.push_str("    if (global_id.x >= tex_dim.x || global_id.y >= tex_dim.y || global_id.z >= tex_dim.z) { return; }\n");
            full_code.push_str("    let coords = vec3<i32>(global_id.xyz);\n");
            full_code.push_str("    let dim = vec3<i32>(tex_dim);\n");
            if self.boundary_mode == 1 {
                full_code.push_str("    let L = textureLoad(velocity, (coords - vec3<i32>(1, 0, 0) + dim) % dim, 0).x;\n");
                full_code.push_str("    let R = textureLoad(velocity, (coords + vec3<i32>(1, 0, 0)) % dim, 0).x;\n");
                full_code.push_str("    let B = textureLoad(velocity, (coords - vec3<i32>(0, 1, 0) + dim) % dim, 0).y;\n");
                full_code.push_str("    let T = textureLoad(velocity, (coords + vec3<i32>(0, 1, 0)) % dim, 0).y;\n");
                full_code.push_str("    let F = textureLoad(velocity, (coords - vec3<i32>(0, 0, 1) + dim) % dim, 0).z;\n");
                full_code.push_str("    let BK = textureLoad(velocity, (coords + vec3<i32>(0, 0, 1)) % dim, 0).z;\n");
            } else {
                full_code.push_str("    let L = textureLoad(velocity, clamp(coords - vec3<i32>(1, 0, 0), vec3<i32>(0), dim - 1), 0).x;\n");
                full_code.push_str("    let R = textureLoad(velocity, clamp(coords + vec3<i32>(1, 0, 0), vec3<i32>(0), dim - 1), 0).x;\n");
                full_code.push_str("    let B = textureLoad(velocity, clamp(coords - vec3<i32>(0, 1, 0), vec3<i32>(0), dim - 1), 0).y;\n");
                full_code.push_str("    let T = textureLoad(velocity, clamp(coords + vec3<i32>(0, 1, 0), vec3<i32>(0), dim - 1), 0).y;\n");
                full_code.push_str("    let F = textureLoad(velocity, clamp(coords - vec3<i32>(0, 0, 1), vec3<i32>(0), dim - 1), 0).z;\n");
                full_code.push_str("    let BK = textureLoad(velocity, clamp(coords + vec3<i32>(0, 0, 1), vec3<i32>(0), dim - 1), 0).z;\n");
            }
            full_code.push_str(
                "    let div = (R - L + T - B + BK - F) * _params.half_inverse_cell_size;\n",
            );
            full_code
                .push_str("    textureStore(output, global_id, vec4<f32>(div, 0.0, 0.0, 1.0));\n");
            full_code.push_str("}\n");
        }

        let shader = ComputeShader::new(context, full_code)?;
        ComputePipeline::new(context, shader)
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<Arc<ComputePipeline>> {
        let key = (input_res.clone(), output_res.clone(), self.boundary_mode);
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&key) {
                return Ok(p.clone());
            }
        }

        let pipeline = self.build_pipeline(context, input_res, output_res)?;
        let arc_p = Arc::new(pipeline);

        let mut cache = self.cache.write().unwrap();
        cache.insert(key, arc_p.clone());
        Ok(arc_p)
    }
}

pub struct Divergence;

impl Divergence {
    pub fn execute(
        context: &WgpuContext,
        definition: &DivergenceDefinition,
        velocity: &GpuResource,
        half_inverse_cell_size: f32,
        output: &GpuResource,
    ) -> Result<()> {
        let input_descriptor = ResourceDescriptor::from_resource(
            velocity,
            ResourceBaseType::Vec2(Box::new(ResourceBaseType::F32)),
        );
        let output_descriptor = ResourceDescriptor::from_resource(output, ResourceBaseType::F32);

        let pipeline =
            definition.get_or_create_pipeline(context, input_descriptor, output_descriptor)?;

        let mut parameters = crate::data::gpu::parameters::PassParameters::new();
        parameters.insert(
            "velocity",
            match velocity {
                GpuResource::Texture2d(t) => PassParameter::Texture2d(t.clone()),
                GpuResource::Texture3d(t) => PassParameter::Texture3d(t.clone()),
                _ => return Err(anyhow!("Velocity must be a texture")),
            },
        );
        parameters.insert(
            "output",
            match output {
                GpuResource::Texture2d(t) => PassParameter::Texture2d(t.clone()),
                GpuResource::Texture3d(t) => PassParameter::Texture3d(t.clone()),
                _ => return Err(anyhow!("Output must be a texture")),
            },
        );

        parameters.insert("half_inverse_cell_size", half_inverse_cell_size);

        let (wg_x, wg_y, wg_z) = match output {
            GpuResource::Texture2d(t) => ((t.size.0 + 15) / 16, (t.size.1 + 15) / 16, 1),
            GpuResource::Texture3d(t) => {
                ((t.size.0 + 7) / 8, (t.size.1 + 7) / 8, (t.size.2 + 3) / 4)
            }
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
