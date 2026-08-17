use anyhow::{Result, bail};

/// The NumPy element types this crate decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dtype {
    F32,
    F64,
    I32,
    I64,
    U8,
    Bool,
}

impl Dtype {
    pub fn parse(descr: &str) -> Result<Self> {
        Ok(match descr.trim_matches(['\'', '"']) {
            "<f4" | "=f4" | "f4" => Dtype::F32,
            "<f8" | "=f8" | "f8" => Dtype::F64,
            "<i4" | "=i4" | "i4" => Dtype::I32,
            "<i8" | "=i8" | "i8" => Dtype::I64,
            "|u1" | "u1" => Dtype::U8,
            "|b1" | "b1" => Dtype::Bool,
            other => bail!("unsupported NumPy dtype {other:?}"),
        })
    }

    pub fn size(self) -> usize {
        match self {
            Dtype::U8 | Dtype::Bool => 1,
            Dtype::F32 | Dtype::I32 => 4,
            Dtype::F64 | Dtype::I64 => 8,
        }
    }
}
