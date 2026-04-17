use crate::data::gpu::compute::ResourceDescriptor;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq)]
pub struct GatherSignature {
    pub gather_args: Vec<String>, // Raw arguments
    pub input_element_type: ResourceBaseType,
    pub output_element_type: ResourceBaseType,
    pub index_type: Option<String>,
    pub index_param_name: Option<String>,
    pub param_names: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct GatherDefinition {
    pub code: String,
    pub cache: Arc<
        RwLock<HashMap<(ResourceDescriptor, ResourceDescriptor), Arc<(ComputePipeline, u64, u64)>>>,
    >,
}

impl Default for GatherDefinition {
    fn default() -> Self {
        Self {
            code: String::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl PartialEq for GatherDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl GatherDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        let _ = Self::parse_signature(&code)?;
        Ok(Self {
            code,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
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
        let mut index_param_name = None;
        let mut param_names = vec![];
        let mut input_element_type = ResourceBaseType::F32;

        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid gather parameter format: {}", arg));
            }
            let name = parts[0];
            let ty_str = parts[1];

            param_names.push(name.to_string());

            let is_index_type = ty_str == "u32" || ty_str == "vec2<u32>" || ty_str == "vec3<u32>";
            let is_resource = ty_str.contains("Resource")
                || ty_str.contains("ptr<")
                || ty_str.contains("texture_");

            if is_index_type && !is_resource {
                if name == "index" || index_param_name.is_none() {
                    index_type = Some(ty_str.to_string());
                    index_param_name = Some(name.to_string());
                }
            } else if is_resource {
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
            index_param_name,
            param_names,
        })
    }

    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<(ComputePipeline, u64, u64)> {
        let sig = Self::parse_signature(&self.code)?;

        // 1. Build the full WGSL code
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
        transformed_code = re_sig
            .replace(
                &transformed_code,
                format!("fn gather({})", new_args.join(", ")),
            )
            .to_string();

        let mut full_code = String::new();

        match input_res {
            ResourceDescriptor::Buffer(_) => full_code.push_str(&format!(
                "@group(0) @binding(0) var<storage, read> input: array<{}>;\n",
                sig.input_element_type.as_str()
            )),
            ResourceDescriptor::Texture2D(_) => full_code.push_str(&format!(
                "@group(0) @binding(0) var input: texture_2d<{}>;\n",
                sig.input_element_type.base_type().as_str()
            )),
            ResourceDescriptor::Texture3D(_) => full_code.push_str(&format!(
                "@group(0) @binding(0) var input: texture_3d<{}>;\n",
                sig.input_element_type.base_type().as_str()
            )),
        }

        match output_res {
            ResourceDescriptor::Buffer(_) => full_code.push_str(&format!(
                "@group(0) @binding(1) var<storage, read_write> output: array<{}>;\n",
                sig.output_element_type.as_str()
            )),
            ResourceDescriptor::Texture2D(_) => full_code.push_str(&format!(
                "@group(0) @binding(1) var output: texture_storage_2d<{}, write>;\n",
                output_res.to_wgsl_storage_format()
            )),
            ResourceDescriptor::Texture3D(_) => full_code.push_str(&format!(
                "@group(0) @binding(1) var output: texture_storage_3d<{}, write>;\n",
                output_res.to_wgsl_storage_format()
            )),
        }

        full_code.push_str("\n");
        full_code.push_str(&transformed_code);
        full_code.push_str("\n");

        match output_res {
            ResourceDescriptor::Buffer(_) => full_code.push_str("@compute @workgroup_size(64)\n"),
            ResourceDescriptor::Texture2D(_) => {
                full_code.push_str("@compute @workgroup_size(16, 16)\n")
            }
            ResourceDescriptor::Texture3D(_) => {
                full_code.push_str("@compute @workgroup_size(4, 4, 4)\n")
            }
        }
        full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");

        match output_res {
            ResourceDescriptor::Buffer(_) => {
                full_code.push_str("    let _global_index = global_id.x;\n");
                full_code.push_str("    if (_global_index >= arrayLength(&output)) { return; }\n");
            }
            ResourceDescriptor::Texture2D(_) => {
                full_code.push_str("    let _global_index = global_id.xy;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y) { return; }\n");
            }
            ResourceDescriptor::Texture3D(_) => {
                full_code.push_str("    let _global_index = global_id;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y || _global_index.z >= tex_dim.z) { return; }\n");
            }
        }

        if let Some(ref idx_ty) = sig.index_type {
            if idx_ty == "u32" {
                match output_res {
                    ResourceDescriptor::Buffer(_) => full_code.push_str("    let index = _global_index;\n"),
                    ResourceDescriptor::Texture2D(_) => full_code.push_str("    let index = _global_index.y * tex_dim.x + _global_index.x;\n"),
                    ResourceDescriptor::Texture3D(_) => full_code.push_str("    let index = (_global_index.z * tex_dim.y + _global_index.y) * tex_dim.x + _global_index.x;\n"),
                }
            } else if idx_ty == "vec2<u32>" {
                if !matches!(
                    output_res,
                    ResourceDescriptor::Texture2D(_) | ResourceDescriptor::Texture3D(_)
                ) {
                    return Err(anyhow::anyhow!("vec2 index not supported for Buffer"));
                }
                full_code.push_str("    let index = _global_index.xy;\n");
            } else if idx_ty == "vec3<u32>" {
                if !matches!(output_res, ResourceDescriptor::Texture3D(_)) {
                    return Err(anyhow::anyhow!("vec3 index only supported for Texture3D"));
                }
                full_code.push_str("    let index = _global_index;\n");
            }
        }

        let mut call_args = vec![];
        for name in &sig.param_names {
            if Some(name) == sig.index_param_name.as_ref() {
                call_args.push("index".to_string());
            }
        }
        let gather_call = format!("gather({})", call_args.join(", "));

        match output_res {
            ResourceDescriptor::Buffer(_) => {
                full_code.push_str(&format!("    output[_global_index] = {};\n", gather_call))
            }
            ResourceDescriptor::Texture2D(_) | ResourceDescriptor::Texture3D(_) => {
                full_code.push_str(&format!("    let _map_result = {};\n", gather_call));
                let base_ty_str = sig.output_element_type.base_type().as_str();
                let pad_val = if base_ty_str == "f32" { "0.0" } else { "0" };
                let sig_comps = sig.output_element_type.component_count();

                let store_val = match sig_comps {
                    1 => format!(
                        "vec4<{}>(_map_result, {}, {}, {})",
                        base_ty_str, pad_val, pad_val, pad_val
                    ),
                    2 => format!(
                        "vec4<{}>(_map_result, {}, {})",
                        base_ty_str, pad_val, pad_val
                    ),
                    4 => "_map_result".to_string(),
                    _ => "_map_result".to_string(),
                };

                full_code.push_str(&format!(
                    "    textureStore(output, _global_index, {});\n",
                    store_val
                ));
            }
        }
        full_code.push_str("}\n");

        let module =
            crate::data::gpu::shader::parse_naga(&full_code, wgpu::naga::ShaderStage::Compute)?;

        let mut input_size = 0;
        let mut output_size = 0;
        let calculate_size = |var_name: &str, module: &wgpu::naga::Module| -> u64 {
            if let Some((_, var)) = module
                .global_variables
                .iter()
                .find(|(_, v)| v.name.as_deref() == Some(var_name))
            {
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

        if let ResourceDescriptor::Buffer(_) = input_res {
            input_size = calculate_size("input", &module);
        }
        if let ResourceDescriptor::Buffer(_) = output_res {
            output_size = calculate_size("output", &module);
        }

        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = ComputePipeline::new(context, shader)?;

        Ok((pipeline, input_size, output_size))
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
    ) -> Result<Arc<(ComputePipeline, u64, u64)>> {
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&(input_res.clone(), output_res.clone())) {
                return Ok(p.clone());
            }
        }

        let pipeline_info = self.build_pipeline(context, input_res.clone(), output_res.clone())?;
        let arc_info = Arc::new(pipeline_info);

        let mut cache = self.cache.write().unwrap();
        cache.insert((input_res, output_res), arc_info.clone());
        Ok(arc_info)
    }
}

pub struct Gather;

impl Gather {
    pub fn execute(
        context: &WgpuContext,
        definition: &GatherDefinition,
        input: &GpuResource,
        output: &GpuResource,
    ) -> Result<()> {
        Self::execute_with_parameters(context, definition, input, output, None)
    }

    pub fn execute_with_parameters(
        context: &WgpuContext,
        definition: &GatherDefinition,
        input: &GpuResource,
        output: &GpuResource,
        extra_parameters: Option<crate::data::gpu::parameters::PassParameters>,
    ) -> Result<()> {
        let sig = GatherDefinition::parse_signature(&definition.code)?;
        let pipeline_info = definition.get_or_create_pipeline(
            context,
            ResourceDescriptor::from_resource(input, sig.input_element_type),
            ResourceDescriptor::from_resource(output, sig.output_element_type),
        )?;
        let (pipeline, _input_size, output_size) = pipeline_info.as_ref();

        let mut parameters =
            extra_parameters.unwrap_or_else(crate::data::gpu::parameters::PassParameters::new);

        // Output resource determines grid size
        let output_num_elements = match output {
            GpuResource::Buffer(b) => {
                parameters.insert("output", b.clone());
                b.size / output_size.max(&1)
            }
            GpuResource::Texture2D(t) => {
                parameters.insert("output", t.clone());
                (t.size.0 * t.size.1) as u64
            }
            GpuResource::Texture3D(t) => {
                parameters.insert("output", t.clone());
                (t.size.0 * t.size.1 * t.size.2) as u64
            }
        };

        // Gather input info
        match input {
            GpuResource::Buffer(b) => {
                parameters.insert("input", b.clone());
            }
            GpuResource::Texture2D(t) => {
                parameters.insert("input", t.clone());
            }
            GpuResource::Texture3D(t) => {
                parameters.insert("input", t.clone());
            }
        }

        let (wg_x, wg_y, wg_z) = match output {
            GpuResource::Buffer(_) => ((output_num_elements as u32 + 63) / 64, 1, 1),
            GpuResource::Texture2D(t) => ((t.size.0 + 15) / 16, (t.size.1 + 15) / 16, 1),
            GpuResource::Texture3D(t) => {
                ((t.size.0 + 3) / 4, (t.size.1 + 3) / 4, (t.size.2 + 3) / 4)
            }
        };

        crate::data::gpu::compute::ComputePass::new(
            context,
            pipeline.clone(),
            parameters,
            wg_x,
            wg_y,
            wg_z,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
