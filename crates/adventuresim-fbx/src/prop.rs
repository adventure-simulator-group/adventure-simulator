/// A typed FBX property value.
#[derive(Debug, Clone)]
pub enum Prop {
    I16(i16),
    Bool(bool),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    ArrF32(Vec<f32>),
    ArrF64(Vec<f64>),
    ArrI32(Vec<i32>),
    ArrI64(Vec<i64>),
    ArrBool(Vec<u8>),
    /// FBX strings are not UTF-8 in general; object names embed a `\0\x01` separator.
    Str(Vec<u8>),
    Raw(Vec<u8>),
}

impl Prop {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Prop::I16(v) => Some(*v as i64),
            Prop::I32(v) => Some(*v as i64),
            Prop::I64(v) => Some(*v),
            Prop::Bool(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Prop::F32(v) => Some(*v as f64),
            Prop::F64(v) => Some(*v),
            Prop::I16(v) => Some(*v as f64),
            Prop::I32(v) => Some(*v as f64),
            Prop::I64(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&[u8]> {
        match self {
            Prop::Str(v) | Prop::Raw(v) => Some(v),
            _ => None,
        }
    }

    /// Numeric array widened to `f64`, whatever the on-disk element type.
    pub fn as_f64_array(&self) -> Option<Vec<f64>> {
        match self {
            Prop::ArrF32(v) => Some(v.iter().map(|x| *x as f64).collect()),
            Prop::ArrF64(v) => Some(v.clone()),
            Prop::ArrI32(v) => Some(v.iter().map(|x| *x as f64).collect()),
            Prop::ArrI64(v) => Some(v.iter().map(|x| *x as f64).collect()),
            _ => None,
        }
    }

    /// Integer array widened to `i64`, whatever the on-disk element type.
    pub fn as_i64_array(&self) -> Option<Vec<i64>> {
        match self {
            Prop::ArrI32(v) => Some(v.iter().map(|x| *x as i64).collect()),
            Prop::ArrI64(v) => Some(v.clone()),
            Prop::ArrBool(v) => Some(v.iter().map(|x| *x as i64).collect()),
            _ => None,
        }
    }
}
