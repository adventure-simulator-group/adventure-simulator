use crate::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct MapSignature {
    pub map_args: Vec<String>, // Raw arguments: e.g. "index: vec2<u32>", "in_val: f32"
    pub input_element_type: ResourceBaseType,
    pub output_element_type: ResourceBaseType,
    pub index_type: Option<String>, // e.g., "u32", "vec2<u32>", "vec3<u32>"
    pub has_input_param: bool,
    pub param_names: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapDefinition {
    pub code: String,
}

impl MapDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        // Validate signature early
        let _ = Self::parse_signature(&code)?;
        Ok(Self { code })
    }

    pub fn parse_signature(code: &str) -> Result<MapSignature> {
        let re = regex::Regex::new(r"(?s)fn\s+map\s*\(([^)]*)\)\s*(?:->\s*([^\{]+))?\{")?;
        let caps = re
            .captures(code)
            .ok_or_else(|| anyhow::anyhow!("Compute shader must define a 'map' function"))?;

        let args_str = caps.get(1).unwrap().as_str();
        let output_type_raw = caps.get(2).map(|m| m.as_str().trim().to_string());

        let mut args = Vec::new();
        let mut bracket_level = 0;
        let mut current_arg = String::new();
        for c in args_str.chars() {
            match c {
                '<' => {
                    bracket_level += 1;
                    current_arg.push(c);
                }
                '>' => {
                    bracket_level -= 1;
                    current_arg.push(c);
                }
                ',' if bracket_level == 0 => {
                    if !current_arg.trim().is_empty() {
                        args.push(current_arg.trim().to_string());
                    }
                    current_arg = String::new();
                }
                _ => current_arg.push(c),
            }
        }
        if !current_arg.trim().is_empty() {
            args.push(current_arg.trim().to_string());
        }

        let mut index_type = None;
        let mut has_input_param = false;
        let mut param_names = vec![];
        let mut input_element_type = ResourceBaseType::F32;

        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid map parameter format: {}", arg));
            }
            let name = parts[0];
            let ty_str = parts[1];
            
            if ty_str.contains("Resource") || ty_str.contains("ptr<") || ty_str.contains("texture_") {
                return Err(anyhow::anyhow!("Map function cannot take Resource types as parameters. Use Compute.Gather or Compute.Scatter instead."));
            }

            param_names.push(name.to_string());

            if name == "index" {
                if ty_str != "u32" && ty_str != "vec2<u32>" && ty_str != "vec3<u32>" {
                    return Err(anyhow::anyhow!("Index must be u32, vec2<u32>, or vec3<u32>"));
                }
                index_type = Some(ty_str.to_string());
            } else {
                has_input_param = true;
                input_element_type = crate::data::gpu::compute::signature::parse_base_type(ty_str);
            }
        }

        let output_element_type = if let Some(out_rt) = output_type_raw {
            crate::data::gpu::compute::signature::parse_base_type(&out_rt)
        } else {
            return Err(anyhow::anyhow!("Map function must return a type."));
        };

        Ok(MapSignature {
            map_args: args,
            input_element_type,
            output_element_type,
            index_type,
            has_input_param,
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

        // 1. Build the full WGSL code
        let mut full_code = String::new();

        // Globals
        match input_res {
            ResourceType::Buffer => full_code.push_str(&format!("@group(0) @binding(0) var<storage, read> input_buffer: array<{}>;\n", sig.input_element_type.as_str())),
            ResourceType::Texture2D => full_code.push_str(&format!("@group(0) @binding(0) var input_texture: texture_2d<{}>;\n", sig.input_element_type.as_str())),
            ResourceType::Texture3D => full_code.push_str(&format!("@group(0) @binding(0) var input_texture: texture_3d<{}>;\n", sig.input_element_type.as_str())),
        }

        match output_res {
            ResourceType::Buffer => full_code.push_str(&format!("@group(0) @binding(1) var<storage, read_write> output_buffer: array<{}>;\n", sig.output_element_type.as_str())),
            ResourceType::Texture2D => full_code.push_str("@group(0) @binding(1) var output_texture: texture_storage_2d<rgba32float, write>;\n"),
            ResourceType::Texture3D => full_code.push_str("@group(0) @binding(1) var output_texture: texture_storage_3d<rgba32float, write>;\n"),
        }

        full_code.push_str("\n");
        // No modification to user's code needed; paste it directly because it contains no banned types
        full_code.push_str(&self.code);
        full_code.push_str("\n");

        // Main compute function
        match output_res {
            ResourceType::Buffer => full_code.push_str("@compute @workgroup_size(64)\n"),
            ResourceType::Texture2D => full_code.push_str("@compute @workgroup_size(16, 16)\n"),
            ResourceType::Texture3D => full_code.push_str("@compute @workgroup_size(4, 4, 4)\n"),
        }
        full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");

        // General bounds checking and natural index
        match output_res {
            ResourceType::Buffer => {
                full_code.push_str("    let _global_index = global_id.x;\n");
                full_code.push_str("    if (_global_index >= arrayLength(&output_buffer)) { return; }\n");
            },
            ResourceType::Texture2D => {
                full_code.push_str("    let _global_index = global_id.xy;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output_texture);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y) { return; }\n");
            },
            ResourceType::Texture3D => {
                full_code.push_str("    let _global_index = global_id;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output_texture);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y || _global_index.z >= tex_dim.z) { return; }\n");
            }
        }

        // Generate the user's requested index type
        if let Some(ref idx_ty) = sig.index_type {
            if idx_ty == "u32" {
                match output_res {
                    ResourceType::Buffer => full_code.push_str("    let index = _global_index;\n"),
                    ResourceType::Texture2D => full_code.push_str("    let index = _global_index.y * tex_dim.x + _global_index.x;\n"),
                    ResourceType::Texture3D => full_code.push_str("    let index = (_global_index.z * tex_dim.y + _global_index.y) * tex_dim.x + _global_index.x;\n"),
                }
            } else if idx_ty == "vec2<u32>" {
                if output_res != ResourceType::Texture2D && output_res != ResourceType::Texture3D { 
                    return Err(anyhow::anyhow!("vec2 index only supported for Textures")); 
                }
                full_code.push_str("    let index = _global_index.xy;\n");
            } else if idx_ty == "vec3<u32>" {
                if output_res != ResourceType::Texture3D { 
                    return Err(anyhow::anyhow!("vec3 index only supported for Texture3D")); 
                }
                full_code.push_str("    let index = _global_index;\n");
            }
        }

        // Fetch input value if the map function uses it
        if sig.has_input_param {
            match input_res { 
                ResourceType::Buffer => {
                    full_code.push_str("    let _in_global_index = global_id.x;\n");
                    full_code.push_str("    if (_in_global_index >= arrayLength(&input_buffer)) { return; }\n");
                    full_code.push_str("    let in_val = input_buffer[_in_global_index];\n");
                },
                ResourceType::Texture2D => {
                    full_code.push_str("    let _in_global_index = global_id.xy;\n");
                    full_code.push_str("    let in_tex_dim = textureDimensions(input_texture);\n");
                    full_code.push_str("    if (_in_global_index.x >= in_tex_dim.x || _in_global_index.y >= in_tex_dim.y) { return; }\n");
                    full_code.push_str("    let in_val = textureLoad(input_texture, _in_global_index, 0);\n"); // Default vector read
                },
                ResourceType::Texture3D => {
                    full_code.push_str("    let _in_global_index = global_id;\n");
                    full_code.push_str("    let in_tex_dim = textureDimensions(input_texture);\n");
                    full_code.push_str("    if (_in_global_index.x >= in_tex_dim.x || _in_global_index.y >= in_tex_dim.y || _in_global_index.z >= in_tex_dim.z) { return; }\n");
                    full_code.push_str("    let in_val = textureLoad(input_texture, _in_global_index, 0);\n");
                }
            }
        }

        // Prepare map call arguments
        let mut map_call_args = vec![];
        for name in &sig.param_names {
            if name == "index" {
                map_call_args.push("index".to_string());
            } else {
                map_call_args.push("in_val".to_string());
            }
        }
        let map_call = format!("map({})", map_call_args.join(", "));

        // Output logic 
        match output_res {
            ResourceType::Buffer => {
                full_code.push_str(&format!("    output_buffer[_global_index] = {};\n", map_call));
            }
            ResourceType::Texture2D | ResourceType::Texture3D => {
                full_code.push_str(&format!("    let val = {};\n", map_call));
                // Assuming writing vector type to Storage Texture
                // Needs to match rgba32float type
                full_code.push_str("    textureStore(output_texture, _global_index, vec4<f32>(val, 0.0, 0.0, 1.0));\n");
            }
        }
        full_code.push_str("}\n");

        // 3. Parse with naga to get struct sizes for validation
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

        if let ResourceType::Buffer = input_res { input_size = calculate_size("input_buffer", &module); }
        if let ResourceType::Buffer = output_res { output_size = calculate_size("output_buffer", &module); }

        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = ComputePipeline::new(context, shader)?;

        Ok((pipeline, input_size, output_size))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResourceType {
    Buffer,
    Texture2D,
    Texture3D,
}
