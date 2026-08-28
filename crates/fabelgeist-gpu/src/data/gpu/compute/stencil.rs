use crate::data::gpu::compute::ResourceDescriptor;
use crate::data::gpu::compute::signature::ResourceBaseType;
use crate::data::gpu::resource::GpuResource;
use crate::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

fn sanitize_type_name(t: &str) -> String {
    t.replace("<", "_")
        .replace(">", "")
        .replace(" ", "")
        .replace(",", "_")
}

fn get_swizzle(components: usize) -> &'static str {
    match components {
        1 => ".x",
        2 => ".xy",
        3 => ".xyz",
        _ => "",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StencilSignature {
    pub is_neighborhood: bool,
    pub dim: u32,
    pub input_element_type: ResourceBaseType,
    pub output_element_type: ResourceBaseType,
    pub index_type: Option<String>,
    pub index_param_name: Option<String>,
    pub size_type: Option<String>,
    pub size_param_name: Option<String>,
    pub center_param_name: Option<String>,
    pub neighbors_param_name: Option<String>,
    pub user_params: Vec<(String, ResourceBaseType)>,
    pub param_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct StencilPipelineKey {
    input: ResourceDescriptor,
    output: ResourceDescriptor,
    secondary: OrderedResourceDescriptors,
    boundary_mode: u32,
}

#[derive(Clone, Debug, Default)]
pub struct StencilDefinition {
    pub code: String,
    pub boundary_mode: u32,
    cache: ComputePipelineCache<StencilPipelineKey, (ComputePipeline, u64, u64)>,
}

impl PartialEq for StencilDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code && self.boundary_mode == other.boundary_mode
    }
}

impl StencilDefinition {
    pub fn new(_context: &WgpuContext, code: String) -> Result<Self> {
        let _ = Self::parse_signature(&code)?;
        Ok(Self {
            code,
            boundary_mode: 0,
            cache: ComputePipelineCache::default(),
        })
    }

    pub fn new_with_boundary(code: String, boundary_mode: u32) -> Result<Self> {
        let _ = Self::parse_signature(&code)?;
        Ok(Self {
            code,
            boundary_mode,
            cache: ComputePipelineCache::default(),
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

        let mut index_type = None;
        let mut index_param_name = None;
        let mut size_type = None;
        let mut size_param_name = None;
        let mut center_param_name = None;
        let mut neighbors_param_name = None;
        let mut user_params = vec![];
        let mut param_names = vec![];
        let mut input_element_type = ResourceBaseType::F32;
        let mut dim = 2;

        let mut is_neighborhood = false;
        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() == 2 && parts[1].contains("Neighbors") {
                is_neighborhood = true;
                break;
            }
        }

        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid stencil parameter format: {}", arg));
            }
            let name = parts[0];
            let ty_str = parts[1];

            param_names.push(name.to_string());

            if name == "index" {
                index_type = Some(ty_str.to_string());
                index_param_name = Some(name.to_string());
            } else if name == "size" {
                size_type = Some(ty_str.to_string());
                size_param_name = Some(name.to_string());
            } else if is_neighborhood {
                if ty_str.contains("Neighbors") {
                    neighbors_param_name = Some(name.to_string());
                    if ty_str.contains("Neighbors1D") {
                        dim = 1;
                    } else if ty_str.contains("Neighbors2D") {
                        dim = 2;
                    } else if ty_str.contains("Neighbors3D") {
                        dim = 3;
                    } else if ty_str.contains("Neighbors4D") {
                        dim = 4;
                    }

                    if let (Some(start), Some(end)) = (ty_str.find('<'), ty_str.rfind('>')) {
                        let inner_ty = &ty_str[start + 1..end];
                        input_element_type =
                            crate::data::gpu::compute::signature::parse_base_type(inner_ty);
                    }
                } else if name == "center" || name == "val" {
                    center_param_name = Some(name.to_string());
                } else {
                    user_params.push((
                        name.to_string(),
                        crate::data::gpu::compute::signature::parse_base_type(ty_str),
                    ));
                }
            } else {
                if name == "input" {
                    input_element_type =
                        crate::data::gpu::compute::signature::parse_base_type(ty_str);
                } else {
                    user_params.push((
                        name.to_string(),
                        crate::data::gpu::compute::signature::parse_base_type(ty_str),
                    ));
                }
            }
        }

        if is_neighborhood && center_param_name.is_none() {
            for arg in &args {
                let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
                let name = parts[0];
                let ty_str = parts[1];
                if name != "index" && name != "size" && !ty_str.contains("Neighbors") {
                    center_param_name = Some(name.to_string());
                    user_params.retain(|(n, _)| n != name);
                    break;
                }
            }
        }

        let output_element_type = if let Some(out_rt) = output_type_raw {
            crate::data::gpu::compute::signature::parse_base_type(&out_rt)
        } else {
            return Err(anyhow::anyhow!("Stencil function must return a type."));
        };

        Ok(StencilSignature {
            is_neighborhood,
            dim,
            input_element_type,
            output_element_type,
            index_type,
            index_param_name,
            size_type,
            size_param_name,
            center_param_name,
            neighbors_param_name,
            user_params,
            param_names,
        })
    }

    pub fn build_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
        secondary_resources: &HashMap<String, ResourceDescriptor>,
    ) -> Result<(ComputePipeline, u64, u64)> {
        let sig = Self::parse_signature(&self.code)?;
        let mut preprocessed_user_code = self.code.clone();

        if sig.is_neighborhood {
            let re_neighbors = regex::Regex::new(
                r"Neighbors([1-4])D\s*<\s*([a-zA-Z0-9_]+(?:\s*<\s*[a-zA-Z0-9_]+\s*>)?)\s*>",
            )?;
            preprocessed_user_code = re_neighbors
                .replace_all(&preprocessed_user_code, |caps: &regex::Captures| {
                    let d = &caps[1];
                    let ty = &caps[2];
                    let ty_sanitized = sanitize_type_name(ty);
                    format!("Neighbors{}D_{}", d, ty_sanitized)
                })
                .into_owned();
        } else {
            let mut new_args = Vec::new();
            for arg in &sig.param_names {
                let is_resource = arg == "input" || secondary_resources.contains_key(arg);

                if is_resource {
                    continue;
                }

                // Keep index, size, or uniform params in the signature
                if arg == "index" {
                    new_args.push(format!(
                        "index: {}",
                        sig.index_type.as_deref().unwrap_or("u32")
                    ));
                } else if arg == "size" {
                    new_args.push(format!(
                        "size: {}",
                        sig.size_type.as_deref().unwrap_or("u32")
                    ));
                } else if let Some((_, ty)) = sig.user_params.iter().find(|(n, _)| n == arg) {
                    new_args.push(format!("{}: {}", arg, ty.as_str()));
                }
            }

            let re_sig = regex::Regex::new(r"(?s)fn\s+stencil\s*\(([^)]*)\)")?;
            preprocessed_user_code = re_sig
                .replace(
                    &preprocessed_user_code,
                    format!("fn stencil({})", new_args.join(", ")),
                )
                .to_string();
        }

        let mut full_code = String::new();

        if sig.is_neighborhood {
            let t_str = sig.input_element_type.as_str();
            let t_sanitized = sanitize_type_name(&t_str);

            match sig.dim {
                1 => {
                    full_code.push_str(&format!(
                        "struct Neighbors1D_{} {{\n    left: {},\n    right: {},\n    prev: {},\n    next: {},\n}};\n\n",
                        t_sanitized, t_str, t_str, t_str, t_str
                    ));
                }
                2 => {
                    full_code.push_str(&format!(
                        "struct Neighbors2D_{} {{\n    left: {},\n    right: {},\n    top: {},\n    bottom: {},\n    up: {},\n    down: {},\n}};\n\n",
                        t_sanitized, t_str, t_str, t_str, t_str, t_str, t_str
                    ));
                }
                3 => {
                    full_code.push_str(&format!(
                        "struct Neighbors3D_{} {{\n    left: {},\n    right: {},\n    top: {},\n    bottom: {},\n    up: {},\n    down: {},\n    front: {},\n    back: {},\n}};\n\n",
                        t_sanitized, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str
                    ));
                }
                4 => {
                    full_code.push_str(&format!(
                        "struct Neighbors4D_{} {{\n    left: {},\n    right: {},\n    top: {},\n    bottom: {},\n    up: {},\n    down: {},\n    front: {},\n    back: {},\n    ana: {},\n    kata: {},\n    w_left: {},\n    w_right: {},\n    past: {},\n    future: {},\n}};\n\n",
                        t_sanitized, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str, t_str
                    ));
                }
                _ => return Err(anyhow::anyhow!("Unsupported dimension: {}", sig.dim)),
            }
        }

        full_code.push_str(&input_res.to_wgsl_input_binding(0, 0, "input"));
        full_code.push_str(&output_res.to_wgsl_output_binding(0, 1, "output"));

        let mut current_binding = 2;
        let mut uniform_params = Vec::new();
        let mut resource_params = Vec::new();

        for (name, ty) in &sig.user_params {
            if secondary_resources.contains_key(name) {
                resource_params.push((name.clone(), ty.clone()));
            } else {
                uniform_params.push((name.clone(), ty.clone()));
            }
        }

        for (name, _ty) in &resource_params {
            let res_desc = &secondary_resources[name];
            full_code.push_str(&res_desc.to_wgsl_input_binding(0, current_binding, name));
            current_binding += 1;
        }

        if !uniform_params.is_empty() {
            full_code.push_str("\nstruct Parameters {\n");
            for (name, ty) in &uniform_params {
                full_code.push_str(&format!("    {}: {},\n", name, ty.as_str()));
            }
            full_code.push_str("};\n");
            full_code.push_str(&format!(
                "@group(0) @binding({}) var<uniform> _params: Parameters;\n",
                current_binding
            ));
        }

        if sig.is_neighborhood {
            let get_coord_1d = if self.boundary_mode == 1 {
                "fn get_coord_1d(x: i32, w: i32) -> i32 { return (x % w + w) % w; }\n"
            } else {
                "fn get_coord_1d(x: i32, w: i32) -> i32 { return clamp(x, 0, w - 1); }\n"
            };

            let get_coord_2d = if self.boundary_mode == 1 {
                "fn get_coord_2d(c: vec2<i32>, dim: vec2<i32>) -> vec2<i32> { return vec2<i32>((c.x % dim.x + dim.x) % dim.x, (c.y % dim.y + dim.y) % dim.y); }\n"
            } else {
                "fn get_coord_2d(c: vec2<i32>, dim: vec2<i32>) -> vec2<i32> { return clamp(c, vec2<i32>(0), dim - 1); }\n"
            };

            let get_coord_3d = if self.boundary_mode == 1 {
                "fn get_coord_3d(c: vec3<i32>, dim: vec3<i32>) -> vec3<i32> { return vec3<i32>((c.x % dim.x + dim.x) % dim.x, (c.y % dim.y + dim.y) % dim.y, (c.z % dim.z + dim.z) % dim.z); }\n"
            } else {
                "fn get_coord_3d(c: vec3<i32>, dim: vec3<i32>) -> vec3<i32> { return clamp(c, vec3<i32>(0), dim - 1); }\n"
            };

            let get_coord_4d = if self.boundary_mode == 1 {
                "fn get_coord_4d(c: vec4<i32>, dim: vec4<i32>) -> vec4<i32> { return vec4<i32>((c.x % dim.x + dim.x) % dim.x, (c.y % dim.y + dim.y) % dim.y, (c.z % dim.z + dim.z) % dim.z, (c.w % dim.w + dim.w) % dim.w); }\n"
            } else {
                "fn get_coord_4d(c: vec4<i32>, dim: vec4<i32>) -> vec4<i32> { return clamp(c, vec4<i32>(0), dim - 1); }\n"
            };

            full_code.push_str(get_coord_1d);
            full_code.push_str(get_coord_2d);
            full_code.push_str(get_coord_3d);
            full_code.push_str(get_coord_4d);
            full_code.push('\n');
        }

        full_code.push_str(&preprocessed_user_code);
        full_code.push('\n');

        match output_res {
            ResourceDescriptor::Buffer(_) => {
                full_code.push_str("@compute @workgroup_size(64, 1, 1)\n")
            }
            ResourceDescriptor::Texture2d(_) => {
                full_code.push_str("@compute @workgroup_size(16, 16, 1)\n")
            }
            ResourceDescriptor::Texture3d(_) => {
                full_code.push_str("@compute @workgroup_size(8, 8, 4)\n")
            }
        }
        full_code.push_str("fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {\n");
        full_code.push_str(&output_res.generate_prologue());

        if let Some(ref idx_ty) = sig.index_type {
            if idx_ty == "u32" {
                match output_res {
                    ResourceDescriptor::Buffer(_) => full_code.push_str("    let index = _global_index;\n"),
                    ResourceDescriptor::Texture2d(_) => full_code.push_str("    let index = _global_index.y * tex_dim.x + _global_index.x;\n"),
                    ResourceDescriptor::Texture3d(_) => full_code.push_str("    let index = (_global_index.z * tex_dim.y + _global_index.y) * tex_dim.x + _global_index.x;\n"),
                }
            } else if idx_ty == "vec2<u32>" {
                full_code.push_str("    let index = _global_index.xy;\n");
            } else if idx_ty == "vec3<u32>" {
                full_code.push_str("    let index = _global_index;\n");
            }
        }

        if let Some(ref size_ty) = sig.size_type {
            match size_ty.as_str() {
                "u32" => match output_res {
                    ResourceDescriptor::Buffer(_) => {
                        full_code.push_str("    let size = arrayLength(&output);\n")
                    }
                    ResourceDescriptor::Texture2d(_) => {
                        full_code.push_str("    let size = tex_dim.x * tex_dim.y;\n")
                    }
                    ResourceDescriptor::Texture3d(_) => {
                        full_code.push_str("    let size = tex_dim.x * tex_dim.y * tex_dim.z;\n")
                    }
                },
                "vec2<u32>" => match output_res {
                    ResourceDescriptor::Buffer(_) => {
                        full_code.push_str("    let size = vec2<u32>(arrayLength(&output), 1u);\n")
                    }
                    ResourceDescriptor::Texture2d(_) => {
                        full_code.push_str("    let size = tex_dim;\n")
                    }
                    ResourceDescriptor::Texture3d(_) => {
                        full_code.push_str("    let size = tex_dim.xy;\n")
                    }
                },
                "vec3<u32>" => match output_res {
                    ResourceDescriptor::Buffer(_) => full_code
                        .push_str("    let size = vec3<u32>(arrayLength(&output), 1u, 1u);\n"),
                    ResourceDescriptor::Texture2d(_) => {
                        full_code.push_str("    let size = vec3<u32>(tex_dim, 1u);\n")
                    }
                    ResourceDescriptor::Texture3d(_) => {
                        full_code.push_str("    let size = tex_dim;\n")
                    }
                },
                _ => return Err(anyhow::anyhow!("Unsupported size type: {}", size_ty)),
            }
        }

        if sig.is_neighborhood {
            let t_str = sig.input_element_type.as_str();
            let t_sanitized = sanitize_type_name(&t_str);

            let grid_dim_decl = match sig.dim {
                1 => match &input_res {
                    ResourceDescriptor::Texture2d(_) => {
                        "    let _grid_w = i32(textureDimensions(input).x);\n".to_string()
                    }
                    ResourceDescriptor::Texture3d(_) => {
                        "    let _grid_w = i32(textureDimensions(input).x);\n".to_string()
                    }
                    ResourceDescriptor::Buffer(_) => {
                        if sig.size_type.is_some() {
                            "    let _grid_w = i32(size);\n".to_string()
                        } else {
                            "    let _grid_w = i32(arrayLength(&input));\n".to_string()
                        }
                    }
                },
                2 => {
                    match &input_res {
                        ResourceDescriptor::Texture2d(_) => {
                            "    let _grid_dim = vec2<i32>(textureDimensions(input));\n".to_string()
                        }
                        ResourceDescriptor::Texture3d(_) => {
                            "    let _grid_dim = vec2<i32>(textureDimensions(input).xy);\n"
                                .to_string()
                        }
                        ResourceDescriptor::Buffer(_) => {
                            if let Some(ref sz_ty) = sig.size_type {
                                if sz_ty == "vec2<u32>" {
                                    "    let _grid_dim = vec2<i32>(size);\n".to_string()
                                } else if sz_ty == "vec3<u32>" {
                                    "    let _grid_dim = vec2<i32>(size.xy);\n".to_string()
                                } else {
                                    "    let _grid_dim = vec2<i32>(i32(size), 1);\n".to_string()
                                }
                            } else {
                                match &output_res {
                                ResourceDescriptor::Texture2d(_) => "    let _grid_dim = vec2<i32>(textureDimensions(output));\n".to_string(),
                                ResourceDescriptor::Texture3d(_) => "    let _grid_dim = vec2<i32>(textureDimensions(output).xy);\n".to_string(),
                                _ => "    let _grid_dim = vec2<i32>(64, 64);\n".to_string(),
                            }
                            }
                        }
                    }
                }
                3 => {
                    match &input_res {
                        ResourceDescriptor::Texture2d(_) => {
                            "    let _grid_dim = vec3<i32>(textureDimensions(input).xy, 1);\n"
                                .to_string()
                        }
                        ResourceDescriptor::Texture3d(_) => {
                            "    let _grid_dim = vec3<i32>(textureDimensions(input));\n".to_string()
                        }
                        ResourceDescriptor::Buffer(_) => {
                            if let Some(ref sz_ty) = sig.size_type {
                                if sz_ty == "vec3<u32>" {
                                    "    let _grid_dim = vec3<i32>(size);\n".to_string()
                                } else if sz_ty == "vec2<u32>" {
                                    "    let _grid_dim = vec3<i32>(size, 1);\n".to_string()
                                } else {
                                    "    let _grid_dim = vec3<i32>(i32(size), 1, 1);\n".to_string()
                                }
                            } else {
                                match &output_res {
                                ResourceDescriptor::Texture3d(_) => "    let _grid_dim = vec3<i32>(textureDimensions(output));\n".to_string(),
                                _ => "    let _grid_dim = vec3<i32>(64, 64, 64);\n".to_string(),
                            }
                            }
                        }
                    }
                }
                4 => {
                    if let Some(ref sz_ty) = sig.size_type {
                        if sz_ty == "vec4<u32>" {
                            "    let _grid_dim = vec4<i32>(size);\n".to_string()
                        } else if sz_ty == "vec3<u32>" {
                            "    let _grid_dim = vec4<i32>(size, 1);\n".to_string()
                        } else {
                            "    let _grid_dim = vec4<i32>(i32(size), 1, 1, 1);\n".to_string()
                        }
                    } else {
                        "    let _grid_dim = vec4<i32>(64, 64, 64, 64);\n".to_string()
                    }
                }
                _ => unreachable!(),
            };

            let swizzle = get_swizzle(sig.input_element_type.component_count());
            let fetch_expr = match &input_res {
                ResourceDescriptor::Buffer(_) => match sig.dim {
                    1 => "input[{}]".to_string(),
                    2 => "input[{0}.y * _grid_dim.x + {0}.x]".to_string(),
                    3 => "input[({0}.z * _grid_dim.y + {0}.y) * _grid_dim.x + {0}.x]".to_string(),
                    4 => "input[((({0}.w * _grid_dim.z + {0}.z) * _grid_dim.y + {0}.y) * _grid_dim.x) + {0}.x]".to_string(),
                    _ => unreachable!(),
                },
                ResourceDescriptor::Texture2d(_) => match sig.dim {
                    1 => format!("textureLoad(input, vec2<i32>({{}}, 0), 0){}", swizzle),
                    2 => format!("textureLoad(input, {{}}, 0){}", swizzle),
                    3 => format!("textureLoad(input, {{}}.xy, 0){}", swizzle),
                    4 => format!("textureLoad(input, {{}}.xy, 0){}", swizzle),
                    _ => unreachable!(),
                },
                ResourceDescriptor::Texture3d(_) => match sig.dim {
                    1 => format!("textureLoad(input, vec3<i32>({{}}, 0, 0), 0){}", swizzle),
                    2 => format!("textureLoad(input, vec3<i32>({{}}, 0), 0){}", swizzle),
                    3 => format!("textureLoad(input, {{}}, 0){}", swizzle),
                    4 => format!("textureLoad(input, {{}}.xyz, 0){}", swizzle),
                    _ => unreachable!(),
                },

            };

            let get_fetch = |coord_str: &str| -> String {
                fetch_expr
                    .replace("{}", coord_str)
                    .replace("{0}", coord_str)
            };

            match sig.dim {
                1 => {
                    full_code.push_str(&grid_dim_decl);
                    full_code.push_str("    let coords = i32(global_id.x);\n");
                    full_code.push_str(&format!("    let center = {};\n", get_fetch("coords")));

                    full_code.push_str(&format!(
                        "    let _n_left = {};\n",
                        get_fetch("get_coord_1d(coords - 1, _grid_w)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_right = {};\n",
                        get_fetch("get_coord_1d(coords + 1, _grid_w)")
                    ));
                    full_code.push_str("    let _n_prev = _n_left;\n");
                    full_code.push_str("    let _n_next = _n_right;\n");
                    full_code.push_str(&format!(
                        "    let neighbors = Neighbors1D_{}(_n_left, _n_right, _n_prev, _n_next);\n",
                        t_sanitized
                    ));
                }
                2 => {
                    full_code.push_str(&grid_dim_decl);
                    full_code.push_str("    let coords = vec2<i32>(global_id.xy);\n");
                    full_code.push_str(&format!("    let center = {};\n", get_fetch("coords")));

                    full_code.push_str(&format!(
                        "    let _n_left = {};\n",
                        get_fetch("get_coord_2d(coords + vec2<i32>(-1, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_right = {};\n",
                        get_fetch("get_coord_2d(coords + vec2<i32>(1, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_top = {};\n",
                        get_fetch("get_coord_2d(coords + vec2<i32>(0, -1), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_bottom = {};\n",
                        get_fetch("get_coord_2d(coords + vec2<i32>(0, 1), _grid_dim)")
                    ));
                    full_code.push_str("    let _n_up = _n_top;\n");
                    full_code.push_str("    let _n_down = _n_bottom;\n");
                    full_code.push_str(&format!(
                        "    let neighbors = Neighbors2D_{}(_n_left, _n_right, _n_top, _n_bottom, _n_up, _n_down);\n",
                        t_sanitized
                    ));
                }
                3 => {
                    full_code.push_str(&grid_dim_decl);
                    full_code.push_str("    let coords = vec3<i32>(global_id.xyz);\n");
                    full_code.push_str(&format!("    let center = {};\n", get_fetch("coords")));

                    full_code.push_str(&format!(
                        "    let _n_left = {};\n",
                        get_fetch("get_coord_3d(coords + vec3<i32>(-1, 0, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_right = {};\n",
                        get_fetch("get_coord_3d(coords + vec3<i32>(1, 0, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_top = {};\n",
                        get_fetch("get_coord_3d(coords + vec3<i32>(0, -1, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_bottom = {};\n",
                        get_fetch("get_coord_3d(coords + vec3<i32>(0, 1, 0), _grid_dim)")
                    ));
                    full_code.push_str("    let _n_up = _n_top;\n");
                    full_code.push_str("    let _n_down = _n_bottom;\n");
                    full_code.push_str(&format!(
                        "    let _n_front = {};\n",
                        get_fetch("get_coord_3d(coords + vec3<i32>(0, 0, -1), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_back = {};\n",
                        get_fetch("get_coord_3d(coords + vec3<i32>(0, 0, 1), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let neighbors = Neighbors3D_{}(_n_left, _n_right, _n_top, _n_bottom, _n_up, _n_down, _n_front, _n_back);\n",
                        t_sanitized
                    ));
                }
                4 => {
                    full_code.push_str(&grid_dim_decl);
                    match output_res {
                        ResourceDescriptor::Buffer(_) => {
                            full_code.push_str("    let _flat_idx = _global_index;\n");
                        }
                        ResourceDescriptor::Texture2d(_) => {
                            full_code.push_str("    let _flat_idx = _global_index.y * tex_dim.x + _global_index.x;\n");
                        }
                        ResourceDescriptor::Texture3d(_) => {
                            full_code.push_str("    let _flat_idx = (_global_index.z * tex_dim.y + _global_index.y) * tex_dim.x + _global_index.x;\n");
                        }
                    }
                    full_code.push_str("    let _coord_x = i32(_flat_idx % u32(_grid_dim.x));\n");
                    full_code.push_str("    let _coord_y = i32((_flat_idx / u32(_grid_dim.x)) % u32(_grid_dim.y));\n");
                    full_code.push_str("    let _coord_z = i32((_flat_idx / (u32(_grid_dim.x) * u32(_grid_dim.y))) % u32(_grid_dim.z));\n");
                    full_code.push_str("    let _coord_w = i32(_flat_idx / (u32(_grid_dim.x) * u32(_grid_dim.y) * u32(_grid_dim.z)));\n");
                    full_code.push_str(
                        "    let coords = vec4<i32>(_coord_x, _coord_y, _coord_z, _coord_w);\n",
                    );
                    full_code.push_str(&format!("    let center = {};\n", get_fetch("coords")));

                    full_code.push_str(&format!(
                        "    let _n_left = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(-1, 0, 0, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_right = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(1, 0, 0, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_top = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(0, -1, 0, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_bottom = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(0, 1, 0, 0), _grid_dim)")
                    ));
                    full_code.push_str("    let _n_up = _n_top;\n");
                    full_code.push_str("    let _n_down = _n_bottom;\n");
                    full_code.push_str(&format!(
                        "    let _n_front = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(0, 0, -1, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_back = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(0, 0, 1, 0), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_ana = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(0, 0, 0, -1), _grid_dim)")
                    ));
                    full_code.push_str(&format!(
                        "    let _n_kata = {};\n",
                        get_fetch("get_coord_4d(coords + vec4<i32>(0, 0, 0, 1), _grid_dim)")
                    ));
                    full_code.push_str("    let _n_w_left = _n_ana;\n");
                    full_code.push_str("    let _n_w_right = _n_kata;\n");
                    full_code.push_str("    let _n_past = _n_ana;\n");
                    full_code.push_str("    let _n_future = _n_kata;\n");
                    full_code.push_str(&format!(
                        "    let neighbors = Neighbors4D_{}(_n_left, _n_right, _n_top, _n_bottom, _n_up, _n_down, _n_front, _n_back, _n_ana, _n_kata, _n_w_left, _n_w_right, _n_past, _n_future);\n",
                        t_sanitized
                    ));
                }
                _ => unreachable!(),
            }
        }

        if !uniform_params.is_empty() {
            for (name, _) in &uniform_params {
                full_code.push_str(&format!("    let {} = _params.{};\n", name, name));
            }
        }

        if sig.is_neighborhood {
            for (name, ty) in &sig.user_params {
                match ty {
                    ResourceBaseType::Texture2d(_) | ResourceBaseType::Texture3d(_) => {}
                    _ => {
                        full_code.push_str(&format!("    let _val_{} = _params.{};\n", name, name));
                    }
                }
            }
        }

        let mut stencil_call_args = vec![];
        for name in &sig.param_names {
            if Some(name) == sig.index_param_name.as_ref() {
                stencil_call_args.push("index".to_string());
            } else if Some(name) == sig.size_param_name.as_ref() {
                stencil_call_args.push("size".to_string());
            } else if sig.is_neighborhood {
                if Some(name) == sig.center_param_name.as_ref() {
                    stencil_call_args.push("center".to_string());
                } else if Some(name) == sig.neighbors_param_name.as_ref() {
                    stencil_call_args.push("neighbors".to_string());
                } else {
                    let is_texture = sig.user_params.iter().any(|(n, t)| {
                        n == name
                            && matches!(
                                t,
                                ResourceBaseType::Texture2d(_) | ResourceBaseType::Texture3d(_)
                            )
                    });
                    if is_texture {
                        stencil_call_args.push(name.clone());
                    } else {
                        stencil_call_args.push(format!("_val_{}", name));
                    }
                }
            } else {
                let is_stripped = name == "input" || secondary_resources.contains_key(name);

                if !is_stripped {
                    stencil_call_args.push(name.clone());
                }
            }
        }
        let stencil_call = format!("stencil({})", stencil_call_args.join(", "));
        match output_res {
            ResourceDescriptor::Buffer(_) => {
                full_code.push_str(&format!("    output[_global_index] = {};\n", stencil_call));
            }
            ResourceDescriptor::Texture2d(_) | ResourceDescriptor::Texture3d(_) => {
                full_code.push_str(&format!("    let _stencil_result = {};\n", stencil_call));
                let base_ty_str = sig.output_element_type.base_type().as_str();
                let pad_val = if base_ty_str == "f32" { "0.0" } else { "0" };
                let sig_comps = sig.output_element_type.component_count();

                let store_val = match sig_comps {
                    1 => format!(
                        "vec4<{}>(_stencil_result, {}, {}, {})",
                        base_ty_str, pad_val, pad_val, pad_val
                    ),
                    2 => format!(
                        "vec4<{}>(_stencil_result, {}, {})",
                        base_ty_str, pad_val, pad_val
                    ),
                    4 => "_stencil_result".to_string(),
                    _ => "_stencil_result".to_string(),
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
                && let wgpu::naga::TypeInner::Array { base, .. } = module.types[var.ty].inner
            {
                let mut layouter = wgpu::naga::proc::Layouter::default();
                let _ = layouter.update(wgpu::naga::proc::GlobalCtx {
                    types: &module.types,
                    constants: &module.constants,
                    overrides: &module.overrides,
                    global_expressions: &module.global_expressions,
                });
                return layouter[base].size as u64;
            }
            0
        };

        if let ResourceDescriptor::Buffer(_) = &input_res {
            input_size = calculate_size("input", &module);
        }
        if let ResourceDescriptor::Buffer(_) = output_res {
            output_size = calculate_size("output", &module);
        }

        let shader = ComputeShader::new(context, full_code)?;
        let pipeline = crate::data::gpu::compute::build_compute_pipeline(context, &shader, "main")?;
        Ok((pipeline, input_size, output_size))
    }

    pub fn get_or_create_pipeline(
        &self,
        context: &WgpuContext,
        input_res: ResourceDescriptor,
        output_res: ResourceDescriptor,
        secondary_resources: &HashMap<String, ResourceDescriptor>,
    ) -> Result<Arc<(ComputePipeline, u64, u64)>> {
        let key = StencilPipelineKey {
            input: input_res.clone(),
            output: output_res.clone(),
            secondary: secondary_resources.into(),
            boundary_mode: self.boundary_mode,
        };

        {
            let cache = self.cache.read().unwrap();
            if let Some(p) = cache.get(&key) {
                return Ok(p.clone());
            }
        }

        let pipeline_info = self.build_pipeline(
            context,
            input_res.clone(),
            output_res.clone(),
            secondary_resources,
        )?;
        let arc_info = Arc::new(pipeline_info);

        let mut cache = self.cache.write().unwrap();
        cache.insert(key, arc_info.clone());
        Ok(arc_info)
    }
}

pub struct Stencil;

impl Stencil {
    pub fn execute(
        context: &WgpuContext,
        definition: &StencilDefinition,
        input: &GpuResource,
        output: &GpuResource,
    ) -> Result<()> {
        Self::execute_with_parameters(context, definition, input, output, None)
    }

    pub fn execute_with_parameters(
        context: &WgpuContext,
        definition: &StencilDefinition,
        input: &GpuResource,
        output: &GpuResource,
        extra_parameters: Option<crate::data::gpu::parameters::PassParameters>,
    ) -> Result<()> {
        let sig = StencilDefinition::parse_signature(&definition.code)?;

        let input_descriptor =
            ResourceDescriptor::from_resource(input, sig.input_element_type.clone());
        let output_descriptor =
            ResourceDescriptor::from_resource(output, sig.output_element_type.clone());

        let mut parameters = extra_parameters.unwrap_or_default();

        let mut secondary_resources = HashMap::new();
        for (name, val) in &parameters.parameters {
            match val {
                crate::data::gpu::parameters::PassParameter::Buffer(_) => {
                    if let Some((_, param_ty)) = sig.user_params.iter().find(|(n, _)| n == name) {
                        secondary_resources
                            .insert(name.clone(), ResourceDescriptor::Buffer(param_ty.clone()));
                    }
                }
                crate::data::gpu::parameters::PassParameter::Texture2d(t) => {
                    secondary_resources
                        .insert(name.clone(), ResourceDescriptor::Texture2d(t.format));
                }
                crate::data::gpu::parameters::PassParameter::Texture3d(t) => {
                    secondary_resources
                        .insert(name.clone(), ResourceDescriptor::Texture3d(t.format));
                }
                _ => {}
            }
        }

        let pipeline_info = definition.get_or_create_pipeline(
            context,
            input_descriptor,
            output_descriptor,
            &secondary_resources,
        )?;
        let (pipeline, _input_size, output_size) = pipeline_info.as_ref();

        // Output resource determines grid size
        let output_num_elements: u64 = match output {
            GpuResource::Buffer(b) => {
                parameters.insert("output", b.clone());
                b.size / output_size.max(&1)
            }
            GpuResource::Texture2d(t) => {
                parameters.insert("output", t.clone());
                (t.size.0 * t.size.1) as u64
            }
            GpuResource::Texture3d(t) => {
                parameters.insert("output", t.clone());
                (t.size.0 * t.size.1 * t.size.2) as u64
            }
        };

        match input {
            GpuResource::Buffer(b) => {
                parameters.insert("input", b.clone());
            }
            GpuResource::Texture2d(t) => {
                parameters.insert("input", t.clone());
            }
            GpuResource::Texture3d(t) => {
                parameters.insert("input", t.clone());
            }
        }

        let (wg_x, wg_y, wg_z) = match output {
            GpuResource::Buffer(_) => ((output_num_elements as u32).div_ceil(64), 1, 1),
            GpuResource::Texture2d(t) => (t.size.0.div_ceil(16), t.size.1.div_ceil(16), 1),
            GpuResource::Texture3d(t) => (
                t.size.0.div_ceil(8),
                t.size.1.div_ceil(8),
                t.size.2.div_ceil(4),
            ),
        };

        crate::data::gpu::compute::ComputePass::execute(
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
mod tests {
    use super::*;
    use crate::data::gpu::compute::test_utils::*;
    use fabelgeist_math::Vec2;

    pub async fn test_stencil_generalized<IN, OUT, S>(
        definition_code: &str,
        input_data: &[IN],
        expected_output: &[OUT],
        boundary_mode: u32,
    ) -> Result<()>
    where
        IN: bytemuck::NoUninit
            + bytemuck::AnyBitPattern
            + PartialEq
            + std::fmt::Debug
            + Default
            + Copy,
        OUT: bytemuck::NoUninit
            + bytemuck::AnyBitPattern
            + PartialEq
            + std::fmt::Debug
            + Default
            + Copy,
        S: bytemuck::Pod + std::fmt::Debug + Default + Copy + PartialEq,
    {
        let definition =
            StencilDefinition::new_with_boundary(definition_code.to_string(), boundary_mode)?;
        let sig = StencilDefinition::parse_signature(definition_code)?;

        run_compute_test::<IN, OUT, S, _>(
            input_data,
            expected_output,
            sig.input_element_type,
            sig.output_element_type,
            |context, in_res, out_res| Stencil::execute(context, &definition, &in_res, &out_res),
        )
        .await
    }

    macro_rules! test_stencil {
        ($name:ident, $definition:expr, $input:expr, $output:expr, $s:ty) => {
            #[tokio::test]
            async fn $name() -> Result<()> {
                test_stencil_generalized::<_, _, $s>($definition, &$input, &$output, 0).await
            }
        };
        ($name:ident, $definition:expr, $input:expr, $output:expr, $s:ty, $boundary:expr) => {
            #[tokio::test]
            async fn $name() -> Result<()> {
                test_stencil_generalized::<_, _, $s>($definition, &$input, &$output, $boundary)
                    .await
            }
        };
    }

    test_stencil!(
        test_1d_stencil,
        "fn stencil(center: f32, neighbors: Neighbors1D<f32>) -> f32 { return neighbors.left + neighbors.right - 2.0 * center; }",
        [1.0f32, 2.0f32, 4.0f32, 7.0f32],
        [1.0f32, 1.0f32, 1.0f32, -3.0f32],
        f32
    );

    test_stencil!(
        test_heat_diffusion_step,
        "fn stencil(val: f32, neighbors: Neighbors1D<f32>) -> f32 { let lap = neighbors.left + neighbors.right - 2.0 * val; return val + 0.2 * lap; }",
        [10.0f32, 20.0f32, 40.0f32],
        [12.0f32, 22.0f32, 36.0f32],
        f32
    );

    #[tokio::test]
    async fn test_2d_stencil_texture() -> Result<()> {
        let context = WgpuContext::new().await.expect("Failed to init WGPU");
        let code = "fn stencil(center: f32, neighbors: Neighbors2D<f32>) -> f32 { return neighbors.left + neighbors.right + neighbors.top + neighbors.bottom - 4.0 * center; }";
        let definition = StencilDefinition::new(&context, code.to_string())?;

        let size = Vec2::new(4.0, 4.0);
        let mut input_data = vec![0.0f32; 16];
        input_data[5] = 1.0;

        let input_texture = crate::data::gpu::texture::Texture2d::create(
            &context,
            size,
            crate::data::gpu::texture::TextureFormat::R32Float,
        )?;
        input_texture.write(&context, &input_data)?;

        let output_texture = crate::data::gpu::texture::Texture2d::create(
            &context,
            size,
            crate::data::gpu::texture::TextureFormat::R32Float,
        )?;

        Stencil::execute(
            &context,
            &definition,
            &GpuResource::Texture2d(input_texture),
            &GpuResource::Texture2d(output_texture.clone()),
        )?;

        let result = output_texture.read::<f32>(&context).await?;

        assert_eq!(result[5], -4.0);
        assert_eq!(result[4], 1.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_resource_based_stencil() -> Result<()> {
        let context = WgpuContext::new().await.expect("Failed to init WGPU");
        let code = "fn stencil(index: u32, input: Resource<f32>, weights: Resource<f32>) -> f32 { return input[index] * weights[0]; }";
        let definition = StencilDefinition::new(&context, code.to_string())?;

        let input_data = vec![1.0f32, 2.0f32, 3.0f32, 4.0f32];
        let weights_data = vec![2.0f32];

        let input_buf = crate::data::gpu::buffer::Buffer::from_slice(
            &context,
            &input_data,
            crate::data::gpu::buffer::BufferDefinition::storage(),
        )?;
        let output_buf = crate::data::gpu::buffer::Buffer::new(
            &context,
            16,
            crate::data::gpu::buffer::BufferDefinition::storage(),
        )?;

        let weights_buf = crate::data::gpu::buffer::Buffer::from_slice(
            &context,
            &weights_data,
            crate::data::gpu::buffer::BufferDefinition::storage(),
        )?;

        let mut params = crate::data::gpu::parameters::PassParameters::new();
        params.insert("weights", weights_buf);

        Stencil::execute_with_parameters(
            &context,
            &definition,
            &GpuResource::Buffer(input_buf),
            &GpuResource::Buffer(output_buf.clone()),
            Some(params),
        )?;

        let result = output_buf.read::<f32>(&context).await?;
        assert_eq!(result, vec![2.0, 4.0, 6.0, 8.0]);

        Ok(())
    }
}
