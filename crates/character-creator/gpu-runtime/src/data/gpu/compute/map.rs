use crate::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum ResourceBaseType {
    F32,
    U32,
    I32,
    Custom(String), // Custom struct name
}

impl ResourceBaseType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::F32 => "f32",
            Self::U32 => "u32",
            Self::I32 => "i32",
            Self::Custom(s) => s.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapSignature {
    pub map_args: Vec<String>, // Raw arguments: e.g. "index: vec2<u32>", "input: Resource<f32>"
    pub input_element_type: ResourceBaseType,
    pub output_element_type: ResourceBaseType,
    pub index_type: Option<String>, // e.g., "u32", "vec2<u32>", "vec3<u32>"
    pub has_input_param: bool,
    pub has_output_param: bool,
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

        let clean_type_name = |ty: &str| -> String {
            let mut current = ty.trim();
            loop {
                if let Some(pos) = current.rfind(',') {
                    current = current[pos + 1..].trim();
                    continue;
                }
                if let Some(pos) = current.rfind('<') {
                    // Extract T from Box<T> or Resource<T>
                    let inner = &current[pos + 1..current.len() - 1];
                    current = inner.trim();
                    continue;
                }
                break;
            }
            current.trim_end_matches('>').trim().to_string()
        };

        let parse_base_type = |ty: &str| -> ResourceBaseType {
            let clean = clean_type_name(ty);
            match clean.as_str() {
                "f32" => ResourceBaseType::F32,
                "u32" => ResourceBaseType::U32,
                "i32" => ResourceBaseType::I32,
                other => ResourceBaseType::Custom(other.to_string()),
            }
        };

        let mut index_type = None;
        let mut has_input_param = false;
        let mut has_output_param = false;
        let mut param_names = vec![];
        let mut input_element_type = ResourceBaseType::F32;

        for arg in &args {
            let parts: Vec<&str> = arg.split(':').map(|s| s.trim()).collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid map parameter format: {}", arg));
            }
            let name = parts[0];
            let ty_str = parts[1];
            param_names.push(name.to_string());

            if name == "index" {
                index_type = Some(ty_str.to_string());
            } else if name == "input" {
                has_input_param = true;
                input_element_type = parse_base_type(ty_str);
            } else if name == "output" {
                has_output_param = true;
            } else if args.len() == 1 {
                // Example 1: `fn map(input: T) -> U`
                has_input_param = true;
                input_element_type = parse_base_type(ty_str);
            }
        }

        let output_element_type = if let Some(out_rt) = output_type_raw {
            parse_base_type(&out_rt)
        } else {
            // Assume Example 5 where output is a parameter `output: Resource<T>`
            input_element_type.clone()
        };

        Ok(MapSignature {
            map_args: args,
            input_element_type,
            output_element_type,
            index_type,
            has_input_param,
            has_output_param,
            param_names,
        })
    }
}
