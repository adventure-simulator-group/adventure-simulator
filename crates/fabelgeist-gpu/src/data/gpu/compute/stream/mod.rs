use crate::data::gpu::compute::ResourceDescriptor;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq)]
pub struct StreamSignature {
    pub stream_args: Vec<String>,
    pub input_element_type: ResourceBaseType,
    pub output_element_type: ResourceBaseType,
    pub index_type: Option<String>,
    pub index_param_name: Option<String>,
    pub param_names: Vec<String>,
    pub has_offset: bool,
    pub has_input_val: bool,
}

#[derive(Clone, Debug)]
pub struct StreamDefinition {
    pub code: String,
    pub cache: Arc<RwLock<HashMap<ResourceDescriptor, Arc<ComputePipeline>>>>,
}

impl Default for StreamDefinition {
    fn default() -> Self {
        Self {
            code: String::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl PartialEq for StreamDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl StreamDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        let _ = Self::parse_signature(&code)?;
        Ok(Self {
            code,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn parse_signature(code: &str) -> Result<StreamSignature> {
        let re = regex::Regex::new(r"(?s)fn\s+stream\s*\(([^)]*)\)\s*\{")?;
        let caps = re.captures(code).ok_or_else(|| {
            anyhow::anyhow!("Compute shader must define a 'stream' function: fn stream(...)")
        })?;

        let args_str = caps.get(1).unwrap().as_str();

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
        let mut output_element_type = ResourceBaseType::F32;
        let mut has_offset = false;
        let mut has_input_val = false;

        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid stream parameter format: {}", arg));
            }
            let name = parts[0];
            let ty_str = parts[1];

            param_names.push(name.to_string());

            let is_index_type = ty_str == "u32" || ty_str == "vec2<u32>" || ty_str == "vec3<u32>";
            let is_resource = ty_str.contains("Resource")
                || ty_str.contains("ptr<")
                || ty_str.contains("texture_");

            if name == "offset" {
                if ty_str != "u32" {
                    return Err(anyhow::anyhow!("Offset must be u32"));
                }
                has_offset = true;
            } else if is_index_type && !is_resource {
                if name == "index" || index_param_name.is_none() {
                    index_type = Some(ty_str.to_string());
                    index_param_name = Some(name.to_string());
                } else {
                    has_input_val = true;
                    input_element_type =
                        crate::data::gpu::compute::signature::parse_base_type(ty_str);
                }
            } else if is_resource {
                output_element_type = crate::data::gpu::compute::signature::parse_base_type(ty_str);
            } else {
                has_input_val = true;
                input_element_type = crate::data::gpu::compute::signature::parse_base_type(ty_str);
            }
        }

        Ok(StreamSignature {
            stream_args: args,
            input_element_type,
            output_element_type,
            index_type,
            index_param_name,
            param_names,
            has_offset,
            has_input_val,
        })
    }

    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
    ) -> Result<ComputePipeline> {
        let sig = Self::parse_signature(&self.code)?;

        // 1. Strip the Resource parameter from the function signature in the code
        let mut transformed_code = self.code.clone();
        let mut new_args = Vec::new();
        for (i, name) in sig.param_names.iter().enumerate() {
            let ty = &sig.stream_args[i].split(':').collect::<Vec<&str>>()[1].trim();
            if ty.contains("Resource<") || ty.contains("ptr<") || ty.contains("texture_") {
                continue;
            }
            new_args.push(format!("{}: {}", name, ty));
        }

        let re_sig = regex::Regex::new(r"(?s)fn\s+stream\s*\(([^)]*)\)")?;
        transformed_code = re_sig
            .replace(
                &transformed_code,
                format!("fn stream({})", new_args.join(", ")),
            )
            .to_string();

        let mut full_code = String::new();

        // Globals
        match input_res {
            ResourceDescriptor::Buffer(_) => full_code.push_str(&format!(
                "@group(0) @binding(0) var<storage, read> input: array<{}>;\n",
                sig.input_element_type.as_str()
            )),
            ResourceDescriptor::Texture2d(_) => full_code.push_str(&format!(
                "@group(0) @binding(0) var input: texture_2d<{}>;\n",
                sig.input_element_type.base_type().as_str()
            )),
            ResourceDescriptor::Texture3d(_) => full_code.push_str(&format!(
                "@group(0) @binding(0) var input: texture_3d<{}>;\n",
                sig.input_element_type.base_type().as_str()
            )),
        }

        full_code.push_str("@group(0) @binding(1) var<storage, read> counts: array<u32>;\n");
        full_code
            .push_str("@group(0) @binding(2) var<storage, read> inclusive_offsets: array<u32>;\n");
        full_code.push_str(&format!(
            "@group(0) @binding(3) var<storage, read_write> output: array<{}>;\n",
            sig.output_element_type.as_str()
        ));

        full_code.push('\n');
        full_code.push_str(&transformed_code);
        full_code.push('\n');

        match input_res {
            ResourceDescriptor::Buffer(_) => full_code.push_str("@compute @workgroup_size(64)\n"),
            ResourceDescriptor::Texture2d(_) => {
                full_code.push_str("@compute @workgroup_size(16, 16)\n")
            }
            ResourceDescriptor::Texture3d(_) => {
                full_code.push_str("@compute @workgroup_size(4, 4, 4)\n")
            }
        }
        full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");

        match input_res {
            ResourceDescriptor::Buffer(_) => {
                full_code.push_str("    let _global_index = global_id.x;\n");
                full_code.push_str(
                    "    if (_global_index >= arrayLength(&inclusive_offsets)) { return; }\n",
                );
            }
            ResourceDescriptor::Texture2d(_) => {
                full_code.push_str("    let _global_index = global_id.xy;\n");
                full_code.push_str("    let tex_dim = textureDimensions(input);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y) { return; }\n");
            }
            ResourceDescriptor::Texture3d(_) => {
                full_code.push_str("    let _global_index = global_id;\n");
                full_code.push_str("    let tex_dim = textureDimensions(input);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y || _global_index.z >= tex_dim.z) { return; }\n");
            }
        }

        match input_res {
            ResourceDescriptor::Buffer(_) => full_code.push_str("    let _linear_index = _global_index;\n"),
            ResourceDescriptor::Texture2d(_) => full_code.push_str("    let _linear_index = _global_index.y * tex_dim.x + _global_index.x;\n"),
            ResourceDescriptor::Texture3d(_) => full_code.push_str("    let _linear_index = (_global_index.z * tex_dim.y + _global_index.y) * tex_dim.x + _global_index.x;\n"),
        }

        full_code.push_str("    let _count = counts[_linear_index];\n");
        full_code.push_str("    if (_count == 0u) { return; }\n");
        full_code.push_str("    let _offset = inclusive_offsets[_linear_index] - _count;\n");

        if sig.has_input_val {
            match input_res {
                ResourceDescriptor::Buffer(_) => {
                    full_code.push_str("    let in_val = input[_linear_index];\n")
                }
                _ => {
                    let swizzle = match sig.input_element_type.component_count() {
                        1 => ".x",
                        2 => ".xy",
                        _ => "",
                    };
                    full_code.push_str(&format!(
                        "    let in_val = textureLoad(input, _global_index, 0){};\n",
                        swizzle
                    ));
                }
            }
        }

        let mut call_args = vec![];
        for (i, name) in sig.param_names.iter().enumerate() {
            let ty = &sig.stream_args[i].split(':').collect::<Vec<&str>>()[1].trim();
            if ty.contains("Resource<") || ty.contains("ptr<") || ty.contains("texture_") {
                continue;
            }

            if Some(name) == sig.index_param_name.as_ref() {
                call_args.push("_global_index".to_string());
            } else if name == "offset" {
                call_args.push("_offset".to_string());
            } else {
                call_args.push("in_val".to_string());
            }
        }
        full_code.push_str(&format!("    stream({});\n", call_args.join(", ")));
        full_code.push_str("}\n");

        let _module =
            crate::data::gpu::shader::parse_naga(&full_code, wgpu::naga::ShaderStage::Compute)?;

        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = ComputePipeline::new(context, shader)?;

        Ok(pipeline)
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
    ) -> Result<Arc<ComputePipeline>> {
        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&input_res) {
                return Ok(p.clone());
            }
        }

        let pipeline = self.build_pipeline(context, input_res.clone())?;
        let arc_info = Arc::new(pipeline);

        let mut cache = self.cache.write().unwrap();
        cache.insert(input_res, arc_info.clone());
        Ok(arc_info)
    }
}

pub struct Stream;

impl Stream {
    pub fn execute(
        context: &WgpuContext,
        definition: &StreamDefinition,
        input: &GpuResource,
        counts: &crate::data::gpu::Buffer,
        inclusive_offsets: &crate::data::gpu::Buffer,
        output: &crate::data::gpu::Buffer,
        extra_parameters: Option<crate::data::gpu::parameters::PassParameters>,
    ) -> Result<()> {
        let sig = StreamDefinition::parse_signature(&definition.code)?;
        let pipeline = definition.get_or_create_pipeline(
            context,
            ResourceDescriptor::from_resource(input, sig.input_element_type),
        )?;

        let mut parameters = extra_parameters.unwrap_or_default();

        match input {
            GpuResource::Buffer(b) => parameters.insert("input", b.clone()),
            GpuResource::Texture2d(t) => parameters.insert("input", t.clone()),
            GpuResource::Texture3d(t) => parameters.insert("input", t.clone()),
        }

        parameters.insert("counts", counts.clone());
        parameters.insert("inclusive_offsets", inclusive_offsets.clone());
        parameters.insert("output", output.clone());

        let (workgroups_x, workgroups_y, workgroups_z) = match input {
            GpuResource::Buffer(_b) => (((inclusive_offsets.size / 4) as u32).div_ceil(64), 1, 1),
            GpuResource::Texture2d(t) => (t.size.0.div_ceil(16), t.size.1.div_ceil(16), 1),
            GpuResource::Texture3d(t) => (
                t.size.0.div_ceil(4),
                t.size.1.div_ceil(4),
                t.size.2.div_ceil(4),
            ),
        };

        crate::data::gpu::compute::ComputePass::execute(
            context,
            pipeline.as_ref().clone(),
            parameters,
            workgroups_x,
            workgroups_y,
            workgroups_z,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
