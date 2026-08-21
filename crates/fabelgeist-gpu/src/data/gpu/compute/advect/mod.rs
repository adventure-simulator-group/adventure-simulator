use crate::data::gpu::parameters::PassParameter;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AdvectSignature {
    pub velocity_type: ResourceBaseType,
    pub quantity_type: ResourceBaseType,
    pub output_type: ResourceBaseType,
    pub dimension: u32, // 2 or 3
}

#[derive(Clone, Debug)]
pub struct AdvectDefinition {
    pub mode: u32,
    pub cache: Arc<
        RwLock<
            HashMap<
                (
                    ResourceDescriptor,
                    ResourceDescriptor,
                    ResourceDescriptor,
                    u32,
                ),
                Arc<ComputePipeline>,
            >,
        >,
    >,
}

impl AdvectDefinition {
    pub fn new(mode: u32) -> Self {
        Self {
            mode,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for AdvectDefinition {
    fn default() -> Self {
        Self::new(0)
    }
}

impl AdvectDefinition {
    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        velocity_res: ResourceDescriptor,
        quantity_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<ComputePipeline> {
        let mut full_code = String::new();

        let dim = match velocity_res {
            ResourceDescriptor::Texture2d(_) => 2,
            ResourceDescriptor::Texture3d(_) => 3,
            _ => return Err(anyhow!("Advect only supports Texture2d or Texture3d")),
        };

        let is_quantity_srgb = if let ResourceDescriptor::Texture2d(f) = quantity_res {
            f.is_srgb()
        } else if let ResourceDescriptor::Texture3d(f) = quantity_res {
            f.is_srgb()
        } else {
            false
        };

        full_code.push_str(&format!(
            "const IS_QUANTITY_SRGB: bool = {};\n",
            is_quantity_srgb
        ));

        full_code.push_str(&velocity_res.to_wgsl_input_binding(0, 0, "velocity"));
        full_code.push_str(&quantity_res.to_wgsl_input_binding(0, 1, "quantity"));
        full_code.push_str(&output_res.to_wgsl_output_binding(0, 2, "output"));
        full_code.push_str("\nstruct Parameters {\n");
        full_code.push_str("    size: vec4<f32>,\n");
        full_code.push_str("    dt: f32,\n");
        full_code.push_str("    _pad: vec3<f32>,\n");
        full_code.push_str("};\n");
        full_code.push_str("@group(0) @binding(3) var<uniform> _params: Parameters;\n\n");

        let wrap_logic = if self.mode == 1 {
            "((pos % 1.0) + 1.0) % 1.0"
        } else {
            "clamp(pos, vec2<f32>(0.0), vec2<f32>(1.0))"
        };

        let bilinear_logic = format!(
            r#"
fn sample_bilinear(tex: texture_2d<f32>, pos: vec2<f32>) -> vec4<f32> {{
    let t_size = vec2<f32>(textureDimensions(tex));
    let wrapped_pos = {};
    let f_coords = wrapped_pos * t_size - 0.5;
    let i_coords = vec2<i32>(floor(f_coords));
    let frac = f_coords - vec2<f32>(i_coords);
    
    let t_size_i = vec2<i32>(t_size);
    "#,
            wrap_logic
        );

        full_code.push_str(&bilinear_logic);

        if self.mode == 1 {
            full_code.push_str(
                r#"
    let c00 = textureLoad(tex, (i_coords + vec2<i32>(0, 0) + t_size_i) % t_size_i, 0);
    let c10 = textureLoad(tex, (i_coords + vec2<i32>(1, 0) + t_size_i) % t_size_i, 0);
    let c01 = textureLoad(tex, (i_coords + vec2<i32>(0, 1) + t_size_i) % t_size_i, 0);
    let c11 = textureLoad(tex, (i_coords + vec2<i32>(1, 1) + t_size_i) % t_size_i, 0);
"#,
            );
        } else {
            full_code.push_str(
                r#"
    let c00 = textureLoad(tex, clamp(i_coords + vec2<i32>(0, 0), vec2<i32>(0), t_size_i - 1), 0);
    let c10 = textureLoad(tex, clamp(i_coords + vec2<i32>(1, 0), vec2<i32>(0), t_size_i - 1), 0);
    let c01 = textureLoad(tex, clamp(i_coords + vec2<i32>(0, 1), vec2<i32>(0), t_size_i - 1), 0);
    let c11 = textureLoad(tex, clamp(i_coords + vec2<i32>(1, 1), vec2<i32>(0), t_size_i - 1), 0);
"#,
            );
        }

        full_code.push_str(
            r#"
    let r0 = mix(c00, c10, frac.x);
    let r1 = mix(c01, c11, frac.x);
    let res = mix(r0, r1, frac.y);
    
    // Manual sRGB to Linear conversion if needed
    // (textureLoad does not perform automatic conversion for sRGB formats)
    if IS_QUANTITY_SRGB {
        let linear_rgb = select(
            pow((res.rgb + 0.055) / 1.055, vec3<f32>(2.4)),
            res.rgb / 12.92,
            res.rgb <= vec3<f32>(0.04045)
        );
        return vec4<f32>(linear_rgb, res.a);
    }

    return res;
}
"#,
        );

        if dim == 2 {
            full_code.push_str("@compute @workgroup_size(16, 16, 1)\n");
            full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
            full_code.push_str("    let tex_dim = textureDimensions(output);\n");
            full_code.push_str(
                "    if (global_id.x >= tex_dim.x || global_id.y >= tex_dim.y) { return; }\n",
            );
            full_code.push_str("    let coords = vec2<f32>(global_id.xy) + 0.5;\n");
            full_code.push_str("    let normalized_pos = coords / _params.size.xy;\n");
            full_code
                .push_str("    let vel_pixel = sample_bilinear(velocity, normalized_pos).xy;\n");
            full_code.push_str("    let vel_dim = vec2<f32>(textureDimensions(velocity));\n");
            full_code.push_str("    let vel_norm = vel_pixel / vel_dim;\n");
            full_code.push_str("    let pos = normalized_pos - vel_norm * _params.dt;\n");
            full_code.push_str("    let val = sample_bilinear(quantity, pos);\n");
            full_code.push_str("    textureStore(output, global_id.xy, val);\n");
            full_code.push_str("}\n");
        } else {
            full_code.push_str("@compute @workgroup_size(8, 8, 4)\n");
            full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
            full_code.push_str("    let tex_dim = textureDimensions(output);\n");
            full_code.push_str("    if (global_id.x >= tex_dim.x || global_id.y >= tex_dim.y || global_id.z >= tex_dim.z) { return; }\n");
            full_code.push_str("    let coords = vec3<f32>(global_id) + 0.5;\n");
            full_code.push_str("    let normalized_pos = coords / _params.size.xyz;\n");
            full_code.push_str("    let vel_pixel = textureLoad(velocity, global_id, 0).xyz;\n");
            full_code.push_str("    let vel_dim = vec3<f32>(textureDimensions(velocity));\n");
            full_code.push_str("    let vel_norm = vel_pixel / vel_dim;\n");
            full_code.push_str("    let pos = normalized_pos - vel_norm * _params.dt;\n");

            if self.mode == 1 {
                full_code.push_str("    let wrapped_pos = ((pos % 1.0) + 1.0) % 1.0;\n");
                full_code.push_str("    let val = textureLoad(quantity, vec3<u32>(wrapped_pos * vec3<f32>(textureDimensions(quantity))), 0);\n");
            } else {
                full_code.push_str("    let val = textureLoad(quantity, vec3<u32>(clamp(pos, vec3<f32>(0.0), vec3<f32>(1.0)) * vec3<f32>(textureDimensions(quantity))), 0);\n");
            }
            full_code.push_str("    textureStore(output, global_id, val);\n");
            full_code.push_str("}\n");
        }

        let shader = ComputeShader::new(context, full_code)?;
        ComputePipeline::new(context, shader)
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        velocity_res: ResourceDescriptor,
        quantity_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<Arc<ComputePipeline>> {
        let key = (
            velocity_res.clone(),
            quantity_res.clone(),
            output_res.clone(),
            self.mode,
        );
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&key) {
                return Ok(p.clone());
            }
        }

        let pipeline = self.build_pipeline(context, velocity_res, quantity_res, output_res)?;
        let arc_p = Arc::new(pipeline);

        let mut cache = self.cache.write().unwrap();
        cache.insert(key, arc_p.clone());
        Ok(arc_p)
    }
}

pub struct Advect;

impl Advect {
    pub fn execute(
        context: &WgpuContext,
        definition: &AdvectDefinition,
        velocity: &GpuResource,
        quantity: &GpuResource,
        _sampler: &crate::data::gpu::sampler::Sampler,
        dt: f32,
        output: &GpuResource,
    ) -> Result<()> {
        let velocity_descriptor = ResourceDescriptor::from_resource(
            velocity,
            ResourceBaseType::Vec2(Box::new(ResourceBaseType::F32)),
        );
        let quantity_descriptor = ResourceDescriptor::from_resource(
            quantity,
            match quantity {
                GpuResource::Texture2d(t) => ResourceBaseType::from_texture_format(t.format),
                GpuResource::Texture3d(t) => ResourceBaseType::from_texture_format(t.format),
                _ => return Err(anyhow!("Quantity must be a texture")),
            },
        );
        let output_descriptor = ResourceDescriptor::from_resource(
            output,
            match output {
                GpuResource::Texture2d(t) => ResourceBaseType::from_texture_format(t.format),
                GpuResource::Texture3d(t) => ResourceBaseType::from_texture_format(t.format),
                _ => return Err(anyhow!("Output must be a texture")),
            },
        );

        let pipeline = definition.get_or_create_pipeline(
            context,
            velocity_descriptor,
            quantity_descriptor,
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
            "quantity",
            match quantity {
                GpuResource::Texture2d(t) => PassParameter::Texture2d(t.clone()),
                GpuResource::Texture3d(t) => PassParameter::Texture3d(t.clone()),
                _ => return Err(anyhow!("Quantity must be a texture")),
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

        let size = match output {
            GpuResource::Texture2d(t) => {
                crate::data::vector::Vec4::new(t.size.0 as f32, t.size.1 as f32, 1.0, 1.0)
            }
            GpuResource::Texture3d(t) => crate::data::vector::Vec4::new(
                t.size.0 as f32,
                t.size.1 as f32,
                t.size.2 as f32,
                1.0,
            ),
            _ => return Err(anyhow!("Output must be a texture")),
        };

        parameters.insert("size", size);
        parameters.insert("dt", dt);

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
