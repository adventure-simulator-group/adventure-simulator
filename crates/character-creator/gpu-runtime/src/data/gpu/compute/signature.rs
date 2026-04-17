use crate::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResourceBaseType {
    F32,
    U32,
    I32,
    Vec2(Box<ResourceBaseType>),
    Vec4(Box<ResourceBaseType>),
    Custom(String), // Custom struct name
}

impl ResourceBaseType {
    pub fn as_str(&self) -> String {
        match self {
            Self::F32 => "f32".to_string(),
            Self::U32 => "u32".to_string(),
            Self::I32 => "i32".to_string(),
            Self::Vec2(inner) => format!("vec2<{}>", inner.as_str()),
            Self::Vec4(inner) => format!("vec4<{}>", inner.as_str()),
            Self::Custom(s) => s.clone(),
        }
    }

    pub fn is_scalar(&self) -> bool {
        match self {
            Self::F32 | Self::U32 | Self::I32 => true,
            _ => false,
        }
    }

    pub fn component_count(&self) -> usize {
        match self {
            Self::F32 | Self::U32 | Self::I32 => 1,
            Self::Vec2(_) => 2,
            Self::Vec4(_) => 4,
            Self::Custom(_) => 1, // Assume custom types are treated as single elements for now
        }
    }

    pub fn base_type(&self) -> Self {
        match self {
            Self::Vec2(inner) | Self::Vec4(inner) => inner.base_type(),
            _ => self.clone(),
        }
    }
}

pub fn clean_type_name(ty: &str) -> String {
    let ty = ty.trim();
    if ty.starts_with("ptr<") && ty.ends_with('>') {
        let inner = &ty[4..ty.len() - 1];
        let parts = split_wgsl_template_args(inner);
        if parts.len() >= 2 {
            return clean_type_name(&parts[1]);
        }
    }
    if (ty.starts_with("array<") || ty.starts_with("texture_") || ty.starts_with("Resource<")) && ty.ends_with('>') {
        if let Some(pos) = ty.find('<') {
            let inner = &ty[pos + 1..ty.len() - 1];
            let parts = split_wgsl_template_args(inner);
            if !parts.is_empty() {
                return clean_type_name(&parts[0]);
            }
        }
    }
    if ty.contains('<') && ty.ends_with('>') {
        if let Some(pos) = ty.find('<') {
            let prefix = ty[..pos].trim();
            if prefix == "vec2" || prefix == "vec4" {
                return ty.to_string();
            }

            let inner = &ty[pos + 1..ty.len() - 1];
            let parts = split_wgsl_template_args(inner);
            if !parts.is_empty() {
                return clean_type_name(&parts[0]);
            }
        }
    }
    ty.to_string()
}

fn split_wgsl_template_args(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for c in inner.chars() {
        if c == ',' && depth == 0 {
            parts.push(current.trim().to_string());
            current = String::new();
        } else {
            if c == '<' {
                depth += 1;
            }
            if c == '>' {
                depth -= 1;
            }
            current.push(c);
        }
    }
    parts.push(current.trim().to_string());
    parts
}

pub fn parse_base_type(ty: &str) -> ResourceBaseType {
    let clean = clean_type_name(ty);
    match clean.as_str() {
        "f32" => ResourceBaseType::F32,
        "u32" => ResourceBaseType::U32,
        "i32" => ResourceBaseType::I32,
        _ if clean.starts_with("vec2<") => {
            let inner = &clean[5..clean.len() - 1];
            ResourceBaseType::Vec2(Box::new(parse_base_type(inner)))
        }
        _ if clean.starts_with("vec4<") => {
            let inner = &clean[5..clean.len() - 1];
            ResourceBaseType::Vec4(Box::new(parse_base_type(inner)))
        }
        other => ResourceBaseType::Custom(other.to_string()),
    }
}

/// Parses a binary operator signature like `fn name(a: T, b: T) -> T` and returns `T`
pub fn parse_binary_op_signature(code: &str, func_name: &str) -> Result<ResourceBaseType> {
    let pattern = format!(r"(?s)fn\s+{}\s*\(\s*[a-zA-Z_][a-zA-Z0-9_]*\s*:\s*([^,]+)\s*,\s*[a-zA-Z_][a-zA-Z0-9_]*\s*:\s*([^)]+)\)\s*->\s*([^\{{]+)\{{", func_name);
    let re = regex::Regex::new(&pattern)?;
    let caps = re
        .captures(code)
        .ok_or_else(|| anyhow::anyhow!("Compute shader must define a '{}' function: fn {}(a: T, b: T) -> T", func_name, func_name))?;
    
    // Extract type T
    let ty_str = caps.get(1).unwrap().as_str().trim();
    Ok(parse_base_type(ty_str))
}
