use crate::data::gpu::compute::ResourceDescriptor;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GradientDefinition {
    pub boundary_mode: u32,
    pub cache: ComputePipelineCache<(
        ResourceDescriptor,
        ResourceDescriptor,
        ResourceDescriptor,
        u32,
    )>,
}

impl GradientDefinition {
    pub fn new(boundary_mode: u32) -> Self {
        Self {
            boundary_mode,
            cache: ComputePipelineCache::default(),
        }
    }
}

impl Default for GradientDefinition {
    fn default() -> Self {
        Self::new(0)
    }
}

impl GradientDefinition {
    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        velocity_res: ResourceDescriptor,
        pressure_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<ComputePipeline> {
        let mut full_code = String::new();

        let dim = match velocity_res {
            ResourceDescriptor::Texture2d(_) => 2,
            ResourceDescriptor::Texture3d(_) => 3,
            _ => return Err(anyhow!("Gradient only supports Texture2d or Texture3d")),
        };

        full_code.push_str(&velocity_res.to_wgsl_input_binding(0, 0, "velocity"));
        full_code.push_str(&pressure_res.to_wgsl_input_binding(0, 1, "pressure"));
        full_code.push_str(&output_res.to_wgsl_output_binding(0, 2, "output"));

        full_code.push_str("\nstruct Parameters {\n");
        full_code.push_str("    half_inverse_cell_size: f32,\n");
        full_code.push_str("    _pad: vec3<f32>,\n");
        full_code.push_str("};\n");
        full_code.push_str("@group(0) @binding(3) var<uniform> _params: Parameters;\n\n");

        if dim == 2 {
            full_code.push_str("@compute @workgroup_size(16, 16, 1)\n");
            full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
            full_code.push_str("    let tex_dim = textureDimensions(pressure);\n");
            full_code.push_str(
                "    if (global_id.x >= tex_dim.x || global_id.y >= tex_dim.y) { return; }\n",
            );
            full_code.push_str("    let x = i32(global_id.x);\n");
            full_code.push_str("    let y = i32(global_id.y);\n");
            full_code.push_str("    let w = i32(tex_dim.x);\n");
            full_code.push_str("    let h = i32(tex_dim.y);\n");

            if self.boundary_mode == 1 {
                full_code.push_str(
                    "    let pL = textureLoad(pressure, vec2<i32>((x - 1 + w) % w, y), 0).x;\n",
                );
                full_code.push_str(
                    "    let pR = textureLoad(pressure, vec2<i32>((x + 1) % w, y), 0).x;\n",
                );
                full_code.push_str(
                    "    let pB = textureLoad(pressure, vec2<i32>(x, (y - 1 + h) % h), 0).x;\n",
                );
                full_code.push_str(
                    "    let pT = textureLoad(pressure, vec2<i32>(x, (y + 1) % h), 0).x;\n",
                );
            } else {
                full_code.push_str(
                    "    let pL = textureLoad(pressure, vec2<i32>(max(x - 1, 0), y), 0).x;\n",
                );
                full_code.push_str(
                    "    let pR = textureLoad(pressure, vec2<i32>(min(x + 1, w - 1), y), 0).x;\n",
                );
                full_code.push_str(
                    "    let pB = textureLoad(pressure, vec2<i32>(x, max(y - 1, 0)), 0).x;\n",
                );
                full_code.push_str(
                    "    let pT = textureLoad(pressure, vec2<i32>(x, min(y + 1, h - 1)), 0).x;\n",
                );
            }

            full_code.push_str("    let vel = textureLoad(velocity, global_id.xy, 0).xy;\n");
            full_code.push_str(
                "    let grad = vec2<f32>(pR - pL, pT - pB) * _params.half_inverse_cell_size;\n",
            );
            full_code.push_str(
                "    textureStore(output, global_id.xy, vec4<f32>(vel - grad, 0.0, 1.0));\n",
            );
            full_code.push_str("}\n");
        } else {
            full_code.push_str("@compute @workgroup_size(8, 8, 4)\n");
            full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
            full_code.push_str("    let tex_dim = textureDimensions(pressure);\n");
            full_code.push_str("    if (global_id.x >= tex_dim.x || global_id.y >= tex_dim.y || global_id.z >= tex_dim.z) { return; }\n");
            full_code.push_str("    let coords = vec3<i32>(global_id.xyz);\n");
            full_code.push_str("    let dim = vec3<i32>(tex_dim);\n");

            if self.boundary_mode == 1 {
                full_code.push_str("    let pL = textureLoad(pressure, (coords - vec3<i32>(1, 0, 0) + dim) % dim, 0).x;\n");
                full_code.push_str("    let pR = textureLoad(pressure, (coords + vec3<i32>(1, 0, 0)) % dim, 0).x;\n");
                full_code.push_str("    let pB = textureLoad(pressure, (coords - vec3<i32>(0, 1, 0) + dim) % dim, 0).x;\n");
                full_code.push_str("    let pT = textureLoad(pressure, (coords + vec3<i32>(0, 1, 0)) % dim, 0).x;\n");
                full_code.push_str("    let pF = textureLoad(pressure, (coords - vec3<i32>(0, 0, 1) + dim) % dim, 0).x;\n");
                full_code.push_str("    let pBK = textureLoad(pressure, (coords + vec3<i32>(0, 0, 1)) % dim, 0).x;\n");
            } else {
                full_code.push_str("    let pL = textureLoad(pressure, clamp(coords - vec3<i32>(1, 0, 0), vec3<i32>(0), dim - 1), 0).x;\n");
                full_code.push_str("    let pR = textureLoad(pressure, clamp(coords + vec3<i32>(1, 0, 0), vec3<i32>(0), dim - 1), 0).x;\n");
                full_code.push_str("    let pB = textureLoad(pressure, clamp(coords - vec3<i32>(0, 1, 0), vec3<i32>(0), dim - 1), 0).x;\n");
                full_code.push_str("    let pT = textureLoad(pressure, clamp(coords + vec3<i32>(0, 1, 0), vec3<i32>(0), dim - 1), 0).x;\n");
                full_code.push_str("    let pF = textureLoad(pressure, clamp(coords - vec3<i32>(0, 0, 1), vec3<i32>(0), dim - 1), 0).x;\n");
                full_code.push_str("    let pBK = textureLoad(pressure, clamp(coords + vec3<i32>(0, 0, 1), vec3<i32>(0), dim - 1), 0).x;\n");
            }

            full_code.push_str("    let vel = textureLoad(velocity, global_id, 0).xyz;\n");
            full_code.push_str("    let grad = vec3<f32>(pR - pL, pT - pB, pBK - pF) * _params.half_inverse_cell_size;\n");
            full_code
                .push_str("    textureStore(output, global_id, vec4<f32>(vel - grad, 1.0));\n");
            full_code.push_str("}\n");
        }

        let shader = ComputeShader::new(context, full_code)?;
        ComputePipeline::new(context, shader)
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        velocity_res: ResourceDescriptor,
        pressure_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<Arc<ComputePipeline>> {
        let key = (
            velocity_res.clone(),
            pressure_res.clone(),
            output_res.clone(),
            self.boundary_mode,
        );
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&key) {
                return Ok(p.clone());
            }
        }

        let pipeline = self.build_pipeline(context, velocity_res, pressure_res, output_res)?;
        let arc_p = Arc::new(pipeline);

        let mut cache = self.cache.write().unwrap();
        cache.insert(key, arc_p.clone());
        Ok(arc_p)
    }
}

pub struct Gradient;

impl Gradient {
    pub fn execute(
        context: &WgpuContext,
        definition: &GradientDefinition,
        velocity: &GpuResource,
        pressure: &GpuResource,
        half_inverse_cell_size: f32,
        output: &GpuResource,
    ) -> Result<()> {
        let velocity_descriptor = ResourceDescriptor::from_resource(
            velocity,
            ResourceBaseType::Vec2(Box::new(ResourceBaseType::F32)),
        );
        let pressure_descriptor =
            ResourceDescriptor::from_resource(pressure, ResourceBaseType::F32);
        let output_descriptor = ResourceDescriptor::from_resource(
            output,
            ResourceBaseType::Vec2(Box::new(ResourceBaseType::F32)),
        );

        let pipeline = definition.get_or_create_pipeline(
            context,
            velocity_descriptor,
            pressure_descriptor,
            output_descriptor,
        )?;

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
            "pressure",
            match pressure {
                GpuResource::Texture2d(t) => PassParameter::Texture2d(t.clone()),
                GpuResource::Texture3d(t) => PassParameter::Texture3d(t.clone()),
                _ => return Err(anyhow!("Pressure must be a texture")),
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
            GpuResource::Texture2d(t) => (t.size.0.div_ceil(16), t.size.1.div_ceil(16), 1),
            GpuResource::Texture3d(t) => (
                t.size.0.div_ceil(8),
                t.size.1.div_ceil(8),
                t.size.2.div_ceil(4),
            ),
            _ => unreachable!(),
        };

        crate::data::gpu::compute::ComputePass::execute(
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
