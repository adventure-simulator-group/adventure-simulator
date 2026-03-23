use crate::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct GatherSignature {
    pub gather_args: Vec<String>, // Raw arguments
    pub input_element_type: ResourceBaseType,
    pub output_element_type: ResourceBaseType,
    pub index_type: Option<String>,
    pub param_names: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GatherDefinition {
    pub code: String,
}

impl GatherDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        let _ = Self::parse_signature(&code)?;
        Ok(Self { code })
    }

    pub fn parse_signature(code: &str) -> Result<GatherSignature> {
        let re = regex::Regex::new(r"(?s)fn\s+gather\s*\(([^)]*)\)\s*(?:->\s*([^\{]+))?\{")?;
        let caps = re
            .captures(code)
            .ok_or_else(|| anyhow::anyhow!("Compute shader must define a 'gather' function"))?;

        let args_str = caps.get(1).unwrap().as_str();
        let output_type_raw = caps.get(2).map(|m| m.as_str().trim().to_string());

        let mut args = Vec::new();
        let mut bracket_level = 0;
        let mut current_arg = String::new();
        for c in args_str.chars() {
            match c {
                '<' => { bracket_level += 1; current_arg.push(c); },
                '>' => { bracket_level -= 1; current_arg.push(c); },
                ',' if bracket_level == 0 => {
                    if !current_arg.trim().is_empty() {
                        args.push(current_arg.trim().to_string());
                    }
                    current_arg = String::new();
                },
                _ => current_arg.push(c),
            }
        }
        if !current_arg.trim().is_empty() {
            args.push(current_arg.trim().to_string());
        }

        let mut index_type = None;
        let mut param_names = vec![];
        let mut input_element_type = ResourceBaseType::F32;

        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() != 2 { return Err(anyhow::anyhow!("Invalid gather parameter format: {}", arg)); }
            let name = parts[0];
            let ty_str = parts[1];

            param_names.push(name.to_string());

            if name == "index" {
                if ty_str != "u32" && ty_str != "vec2<u32>" && ty_str != "vec3<u32>" {
                    return Err(anyhow::anyhow!("Index must be u32, vec2<u32>, or vec3<u32>"));
                }
                index_type = Some(ty_str.to_string());
            } else if ty_str.contains("Resource") || ty_str.contains("ptr<") || ty_str.contains("texture_") {
                input_element_type = crate::data::gpu::compute::signature::parse_base_type(ty_str);
            }
        }

        let output_element_type = if let Some(out_rt) = output_type_raw {
            crate::data::gpu::compute::signature::parse_base_type(&out_rt)
        } else {
            return Err(anyhow::anyhow!("Gather function must return a type."));
        };

        Ok(GatherSignature {
            gather_args: args,
            input_element_type,
            output_element_type,
            index_type,
            param_names,
        })
    }

    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceType,
        output_res: ResourceType,
    ) -> Result<(ComputePipeline, u64, u64)> {
        let sig = Self::parse_signature(&self.code)?;

        // 1. Strip the Resource parameter from the function signature in the code
        let mut transformed_code = self.code.clone();
        let mut new_args = Vec::new();
        for arg in &sig.gather_args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            let name = parts[0];
            let ty = parts[1].to_string();

            if ty.contains("Resource<") || ty.contains("ptr<") || ty.contains("texture_") {
                continue;
            }
            new_args.push(format!("{}: {}", name, ty));
        }

        let re_sig = regex::Regex::new(r"(?s)fn\s+gather\s*\(([^)]*)\)")?;
        transformed_code = re_sig.replace(&transformed_code, format!("fn gather({})", new_args.join(", "))).to_string();

        let mut full_code = String::new();

        match input_res {
            ResourceType::Buffer => full_code.push_str(&format!("@group(0) @binding(0) var<storage, read> input: array<{}>;\n", sig.input_element_type.as_str())),
            ResourceType::Texture2D => full_code.push_str(&format!("@group(0) @binding(0) var input: texture_2d<{}>;\n", sig.input_element_type.as_str())),
            ResourceType::Texture3D => full_code.push_str(&format!("@group(0) @binding(0) var input: texture_3d<{}>;\n", sig.input_element_type.as_str())),
        }

        match output_res {
            ResourceType::Buffer => full_code.push_str(&format!("@group(0) @binding(1) var<storage, read_write> output: array<{}>;\n", sig.output_element_type.as_str())),
            ResourceType::Texture2D => full_code.push_str("@group(0) @binding(1) var output: texture_storage_2d<rgba32float, write>;\n"),
            ResourceType::Texture3D => full_code.push_str("@group(0) @binding(1) var output: texture_storage_3d<rgba32float, write>;\n"),
        }

        full_code.push_str("\n");
        full_code.push_str(&transformed_code);
        full_code.push_str("\n");

        match output_res {
            ResourceType::Buffer => full_code.push_str("@compute @workgroup_size(64)\n"),
            ResourceType::Texture2D => full_code.push_str("@compute @workgroup_size(16, 16)\n"),
            ResourceType::Texture3D => full_code.push_str("@compute @workgroup_size(4, 4, 4)\n"),
        }
        full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");

        match output_res {
            ResourceType::Buffer => {
                full_code.push_str("    let _global_index = global_id.x;\n");
                full_code.push_str("    if (_global_index >= arrayLength(&output)) { return; }\n");
            },
            ResourceType::Texture2D => {
                full_code.push_str("    let _global_index = global_id.xy;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y) { return; }\n");
            },
            ResourceType::Texture3D => {
                full_code.push_str("    let _global_index = global_id;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y || _global_index.z >= tex_dim.z) { return; }\n");
            }
        }

        if let Some(ref idx_ty) = sig.index_type {
            if idx_ty == "u32" {
                match output_res {
                    ResourceType::Buffer => full_code.push_str("    let index = _global_index;\n"),
                    ResourceType::Texture2D => full_code.push_str("    let index = _global_index.y * tex_dim.x + _global_index.x;\n"),
                    ResourceType::Texture3D => full_code.push_str("    let index = (_global_index.z * tex_dim.y + _global_index.y) * tex_dim.x + _global_index.x;\n"),
                }
            } else if idx_ty == "vec2<u32>" {
                if output_res == ResourceType::Buffer { return Err(anyhow::anyhow!("vec2 index not supported for Buffer")); }
                full_code.push_str("    let index = _global_index.xy;\n");
            } else if idx_ty == "vec3<u32>" {
                if output_res != ResourceType::Texture3D { return Err(anyhow::anyhow!("vec3 index only supported for Texture3D")); }
                full_code.push_str("    let index = _global_index;\n");
            }
        }

        let mut call_args = vec![];
        for name in &sig.param_names {
            if name == "index" {
                call_args.push("index".to_string());
            }
            // other parameters (like the Resource one) are skipped because we stripped them from the WGSL gathering function signature.
            // The user's function must not expect them directly in the stripped code.
        }
        let gather_call = format!("gather({})", call_args.join(", "));

        match output_res {
            ResourceType::Buffer => full_code.push_str(&format!("    output[_global_index] = {};\n", gather_call)),
            ResourceType::Texture2D | ResourceType::Texture3D => {
                full_code.push_str(&format!("    let val = {};\n", gather_call));
                full_code.push_str("    textureStore(output, _global_index, vec4<f32>(val, 0.0, 0.0, 1.0));\n");
            }
        }
        full_code.push_str("}\n");

        let module = crate::data::gpu::shader::parse_naga(&full_code, wgpu::naga::ShaderStage::Compute)?;

        let mut input_size = 0;
        let mut output_size = 0;
        let calculate_size = |var_name: &str, module: &wgpu::naga::Module| -> u64 {
            if let Some((_, var)) = module.global_variables.iter().find(|(_, v)| v.name.as_deref() == Some(var_name)) {
                if let wgpu::naga::TypeInner::Array { base, .. } = module.types[var.ty].inner {
                    let mut layouter = wgpu::naga::proc::Layouter::default();
                    let _ = layouter.update(wgpu::naga::proc::GlobalCtx {
                        types: &module.types,
                        constants: &module.constants,
                        overrides: &module.overrides,
                        global_expressions: &module.global_expressions,
                    });
                    return layouter[base].size as u64;
                }
            }
            0
        };

        if let ResourceType::Buffer = input_res { input_size = calculate_size("input", &module); }
        if let ResourceType::Buffer = output_res { output_size = calculate_size("output", &module); }

        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = ComputePipeline::new(context, shader)?;

        Ok((pipeline, input_size, output_size))
    }
}
