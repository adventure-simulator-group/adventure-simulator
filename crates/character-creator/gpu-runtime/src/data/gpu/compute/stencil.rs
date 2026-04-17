use crate::data::gpu::compute::signature::ResourceBaseType;
use crate::data::gpu::resource::{GpuResource, ResourceType};
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, PartialEq)]
pub enum StencilParameter {
    Static(Vec<f32>, crate::data::gpu::tensor::Shape),
    Dynamic(Vec<f32>, crate::data::gpu::tensor::Shape),
}

impl Default for StencilParameter {
    fn default() -> Self {
        Self::Static(vec![0.0], crate::data::gpu::tensor::Shape::default())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StencilSignature {
    pub input_element_type: ResourceBaseType,
    pub index_type: String, // "u32" or "vec2<u32>"
    pub has_weights: bool,
    pub weights_element_type: Option<ResourceBaseType>,
    pub has_offsets: bool,
    pub offsets_element_type: Option<ResourceBaseType>,
    pub output_element_type: ResourceBaseType,
}

#[derive(Clone, Debug)]
pub struct StencilDefinition {
    pub code: String,
    pub cache: Arc<RwLock<HashMap<StencilCacheKey, Arc<(ComputePipeline, StencilSignature)>>>>,
}

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct StencilCacheKey {
    pub input_res: ResourceType,
    pub output_res: ResourceType,
    pub static_weights: Option<Vec<u32>>,
    pub static_offsets: Option<Vec<u32>>,
}

impl Default for StencilDefinition {
    fn default() -> Self {
        Self {
            code: String::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl PartialEq for StencilDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl StencilDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        let _ = Self::parse_signature(&code)?;
        Ok(Self {
            code,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn parse_signature(code: &str) -> Result<StencilSignature> {
        let re = regex::Regex::new(r"(?s)fn\s+stencil\s*\(([^)]*)\)\s*(?:->\s*([^\{]+))?\{")?;
        let caps = re
            .captures(code)
            .ok_or_else(|| anyhow::anyhow!("Stencil shader must define a 'stencil' function"))?;

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

        let mut input_element_type = ResourceBaseType::F32;
        let mut index_type = "u32".to_string();
        let mut has_weights = false;
        let mut weights_element_type = None;
        let mut has_offsets = false;
        let mut offsets_element_type = None;

        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid stencil parameter: {}", arg));
            }
            let name = parts[0];
            let ty_str = parts[1];

            match name {
                "index" => {
                    if ty_str != "u32" && ty_str != "vec2<u32>" {
                        return Err(anyhow::anyhow!("Stencil index must be u32 or vec2<u32>"));
                    }
                    index_type = ty_str.to_string();
                }
                "input" => {
                    input_element_type =
                        crate::data::gpu::compute::signature::parse_base_type(ty_str);
                }
                "weights" => {
                    has_weights = true;
                    weights_element_type = Some(
                        crate::data::gpu::compute::signature::parse_base_type(ty_str),
                    );
                }
                "offsets" => {
                    has_offsets = true;
                    offsets_element_type = Some(
                        crate::data::gpu::compute::signature::parse_base_type(ty_str),
                    );
                }
                _ => {}
            }
        }

        let output_element_type = if let Some(out_rt) = output_type_raw {
            crate::data::gpu::compute::signature::parse_base_type(&out_rt)
        } else {
            return Err(anyhow::anyhow!("Stencil function must return a type."));
        };

        Ok(StencilSignature {
            input_element_type,
            index_type,
            has_weights,
            weights_element_type,
            has_offsets,
            offsets_element_type,
            output_element_type,
        })
    }

    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceType,
        output_res: ResourceType,
        static_weights: Option<&[f32]>,
        static_offsets: Option<&[f32]>,
    ) -> Result<(ComputePipeline, StencilSignature)> {
        let sig = Self::parse_signature(&self.code)?;
        let mut full_code = String::new();

        // Bindings / Consts
        match input_res {
            ResourceType::Buffer => full_code.push_str(&format!(
                "@group(0) @binding(0) var<storage, read> input: array<{}>;\n",
                sig.input_element_type.as_str()
            )),
            ResourceType::Texture2D => full_code.push_str(&format!(
                "@group(0) @binding(0) var input: texture_2d<{}>;\n",
                sig.input_element_type.as_str()
            )),
            ResourceType::Texture3D => full_code.push_str(&format!(
                "@group(0) @binding(0) var input: texture_3d<{}>;\n",
                sig.input_element_type.as_str()
            )),
        }

        match output_res {
            ResourceType::Buffer => full_code.push_str(&format!(
                "@group(0) @binding(1) var<storage, read_write> output: array<{}>;\n",
                sig.output_element_type.as_str()
            )),
            ResourceType::Texture2D => full_code.push_str(
                "@group(0) @binding(1) var output: texture_storage_2d<rgba32float, write>;\n",
            ),
            ResourceType::Texture3D => full_code.push_str(
                "@group(0) @binding(1) var output: texture_storage_3d<rgba32float, write>;\n",
            ),
        }

        let mut binding_index = 2;
        if let Some(sw) = static_weights {
            let weights_str = sw
                .iter()
                .map(|f| format!("{:.6}", f))
                .collect::<Vec<_>>()
                .join(", ");
            full_code.push_str(&format!(
                "const weights = array<{0}, {1}>({2});\n",
                sig.weights_element_type
                    .as_ref()
                    .map(|t| t.as_str())
                    .unwrap_or_else(|| "f32".to_string()),
                sw.len(),
                weights_str
            ));
        } else if sig.has_weights {
            full_code.push_str(&format!(
                "@group(0) @binding({}) var<storage, read> weights: array<{}>;\n",
                binding_index,
                sig.weights_element_type.as_ref().unwrap().as_str()
            ));
            binding_index += 1;
        }

        if let Some(so) = static_offsets {
            let offsets_str = so
                .iter()
                .map(|f| format!("{:.6}", f))
                .collect::<Vec<_>>()
                .join(", ");
            full_code.push_str(&format!(
                "const offsets = array<{0}, {1}>({2});\n",
                sig.offsets_element_type
                    .as_ref()
                    .map(|t| t.as_str())
                    .unwrap_or_else(|| "f32".to_string()),
                so.len(),
                offsets_str
            ));
        } else if sig.has_offsets {
            full_code.push_str(&format!(
                "@group(0) @binding({}) var<storage, read> offsets: array<{}>;\n",
                binding_index,
                sig.offsets_element_type.as_ref().unwrap().as_str()
            ));
        }

        full_code.push_str("\n");
        full_code.push_str("alias Resource<T> = array<T>;\n\n");
        full_code.push_str(&self.code);
        full_code.push_str("\n");

        // Main
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
            }
            ResourceType::Texture2D => {
                full_code.push_str("    let _global_index = global_id.xy;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y) { return; }\n");
            }
            ResourceType::Texture3D => {
                full_code.push_str("    let _global_index = global_id;\n");
                full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                full_code.push_str("    if (_global_index.x >= tex_dim.x || _global_index.y >= tex_dim.y || _global_index.z >= tex_dim.z) { return; }\n");
            }
        }

        if sig.index_type == "u32" {
            match output_res {
                ResourceType::Buffer => full_code.push_str("    let index = _global_index;\n"),
                ResourceType::Texture2D => {
                    full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                    full_code.push_str(
                        "    let index = _global_index.y * tex_dim.x + _global_index.x;\n",
                    );
                }
                ResourceType::Texture3D => {
                    full_code.push_str("    let tex_dim = textureDimensions(output);\n");
                    full_code.push_str("    let index = (_global_index.z * tex_dim.y + _global_index.y) * tex_dim.x + _global_index.x;\n");
                }
            }
        } else if sig.index_type == "vec2<u32>" {
            full_code.push_str("    let index = _global_index.xy;\n");
        }

        let mut call_args = vec!["index".to_string(), "input".to_string()];
        if sig.has_weights {
            call_args.push("weights".to_string());
        }
        if sig.has_offsets {
            call_args.push("offsets".to_string());
        }

        let stencil_call = format!("stencil({})", call_args.join(", "));

        match output_res {
            ResourceType::Buffer => {
                full_code.push_str(&format!("    output[_global_index] = {};\n", stencil_call));
            }
            ResourceType::Texture2D | ResourceType::Texture3D => {
                full_code.push_str(&format!("    let val = {};\n", stencil_call));
                full_code.push_str(
                    "    textureStore(output, _global_index, vec4<f32>(val, 0.0, 0.0, 1.0));\n",
                );
            }
        }
        full_code.push_str("}\n");

        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = crate::data::gpu::compute::build_compute_pipeline(context, &shader, "main")?;
        Ok((pipeline, sig))
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceType,
        output_res: ResourceType,
        static_weights: Option<&[f32]>,
        static_offsets: Option<&[f32]>,
    ) -> Result<Arc<(ComputePipeline, StencilSignature)>> {
        let cache_key = StencilCacheKey {
            input_res,
            output_res,
            static_weights: static_weights.map(|w| w.iter().map(|f| f.to_bits()).collect()),
            static_offsets: static_offsets.map(|o| o.iter().map(|f| f.to_bits()).collect()),
        };

        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&cache_key) {
                return Ok(p.clone());
            }
        }

        let pipeline_info = self.build_pipeline(
            context,
            input_res,
            output_res,
            static_weights,
            static_offsets,
        )?;
        let arc_info = Arc::new(pipeline_info);

        let mut cache = self.cache.write().unwrap();
        cache.insert(cache_key, arc_info.clone());
        Ok(arc_info)
    }
}

pub struct Stencil;

impl Stencil {
    pub fn execute(
        context: &WgpuContext,
        definition: &StencilDefinition,
        input: &GpuResource,
        weights: Option<StencilParameter>,
        offsets: Option<StencilParameter>,
        output: &GpuResource,
    ) -> Result<()> {
        let pipeline_info = definition.get_or_create_pipeline(
            context,
            input.resource_type(),
            output.resource_type(),
            match &weights {
                Some(StencilParameter::Static(w, _)) => Some(w.as_slice()),
                _ => None,
            },
            match &offsets {
                Some(StencilParameter::Static(o, _)) => Some(o.as_slice()),
                _ => None,
            },
        )?;
        let (pipeline, sig) = pipeline_info.as_ref();

        let mut parameters = crate::data::gpu::parameters::PassParameters::new();

        // Input
        match input {
            GpuResource::Buffer(b) => parameters.insert("input", b.clone()),
            GpuResource::Texture2D(t) => parameters.insert("input", t.clone()),
            GpuResource::Texture3D(t) => parameters.insert("input", t.clone()),
        }

        // Output
        match output {
            GpuResource::Buffer(b) => parameters.insert("output", b.clone()),
            GpuResource::Texture2D(t) => parameters.insert("output", t.clone()),
            GpuResource::Texture3D(t) => parameters.insert("output", t.clone()),
        }

        // Weights
        let dw_buffer;
        if let Some(p) = &weights {
            match p {
                StencilParameter::Static(_, _) => {}
                StencilParameter::Dynamic(data, _) => {
                    dw_buffer = crate::data::gpu::buffer::Buffer::from_slice(
                        context,
                        data,
                        crate::data::gpu::buffer::BufferDefinition::storage()
                            .with_label("StencilWeights"),
                    )?;
                    parameters.insert("weights", dw_buffer);
                }
            }
        } else if sig.has_weights {
            return Err(anyhow!("Stencil requires weights but none provided"));
        }

        // Offsets
        let do_buffer;
        if let Some(p) = &offsets {
            match p {
                StencilParameter::Static(_, _) => {}
                StencilParameter::Dynamic(data, _) => {
                    do_buffer = crate::data::gpu::buffer::Buffer::from_slice(
                        context,
                        data,
                        crate::data::gpu::buffer::BufferDefinition::storage()
                            .with_label("StencilOffsets"),
                    )?;
                    parameters.insert("offsets", do_buffer);
                }
            }
        } else if sig.has_offsets {
            return Err(anyhow!("Stencil requires offsets but none provided"));
        }

        // Dispatch
        let (wg_x, wg_y, wg_z) = match output {
            GpuResource::Buffer(b) => (((b.size / 4) as u32 + 63) / 64, 1, 1),
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
