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

pub fn clean_type_name(ty: &str) -> String {
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
}

pub fn parse_base_type(ty: &str) -> ResourceBaseType {
    let clean = clean_type_name(ty);
    match clean.as_str() {
        "f32" => ResourceBaseType::F32,
        "u32" => ResourceBaseType::U32,
        "i32" => ResourceBaseType::I32,
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
