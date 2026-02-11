use anyhow::Context;
use indexmap::IndexMap;

use crate::{Result, Value};

pub trait ConvertInto<T> {
    fn convert_into(self) -> Result<T>;
}

// --- Primitive Conversions ---

macro_rules! impl_primitive_conversion {
    ($($t:ty),*) => {
        $(
            impl ConvertInto<$t> for Option<Value> {
                fn convert_into(self) -> Result<$t> {
                    let v = self.context("Missing value")?;
                    let n = v.into_number().map_err(|_| anyhow::anyhow!("Value is not a number"))?;
                    Ok(n as $t)
                }
            }
            impl ConvertInto<$t> for Value {
                fn convert_into(self) -> Result<$t> { Some(self).convert_into() }
            }
            impl ConvertInto<Option<Value>> for $t {
                fn convert_into(self) -> Result<Option<Value>> { Ok(Some(Value::new(self))) }
            }
            impl ConvertInto<Result<Vec<Option<Value>>>> for $t {
                fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
                    Ok(Ok(vec![self.convert_into()?]))
                }
            }
        )*
    };
}

impl_primitive_conversion!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, usize, isize);

// --- Boolean ---

impl ConvertInto<bool> for Option<Value> {
    fn convert_into(self) -> Result<bool> {
        self.context("Missing value")?
            .into_boolean()
            .map_err(|_| anyhow::anyhow!("Not a bool"))
    }
}
impl ConvertInto<bool> for Value {
    fn convert_into(self) -> Result<bool> {
        Some(self).convert_into()
    }
}
impl ConvertInto<Option<Value>> for bool {
    fn convert_into(self) -> Result<Option<Value>> {
        Ok(Some(Value::new(self)))
    }
}
impl ConvertInto<Result<Vec<Option<Value>>>> for bool {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(Ok(vec![self.convert_into()?]))
    }
}

// --- String ---

impl ConvertInto<String> for Option<Value> {
    fn convert_into(self) -> Result<String> {
        self.context("Missing value")?
            .into_string()
            .map_err(|_| anyhow::anyhow!("Not a string"))
    }
}
impl ConvertInto<String> for Value {
    fn convert_into(self) -> Result<String> {
        Some(self).convert_into()
    }
}
impl ConvertInto<Option<Value>> for String {
    fn convert_into(self) -> Result<Option<Value>> {
        Ok(Some(Value::new(self)))
    }
}
impl ConvertInto<Result<Vec<Option<Value>>>> for String {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(Ok(vec![self.convert_into()?]))
    }
}

// --- IndexMap ---

impl ConvertInto<IndexMap<String, Value>> for Option<Value> {
    fn convert_into(self) -> Result<IndexMap<String, Value>> {
        self.context("Missing value")?
            .into_object()
            .map_err(|_| anyhow::anyhow!("Not an object"))
    }
}
impl ConvertInto<IndexMap<String, Value>> for Value {
    fn convert_into(self) -> Result<IndexMap<String, Value>> {
        Some(self).convert_into()
    }
}
impl ConvertInto<Option<Value>> for IndexMap<String, Value> {
    fn convert_into(self) -> Result<Option<Value>> {
        Ok(Some(Value::new(self)))
    }
}
impl ConvertInto<Result<Vec<Option<Value>>>> for IndexMap<String, Value> {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(Ok(vec![self.convert_into()?]))
    }
}

// --- Vec<Value> ---

impl ConvertInto<Vec<Value>> for Option<Value> {
    fn convert_into(self) -> Result<Vec<Value>> {
        self.context("Missing value")?
            .into_vector()
            .map_err(|_| anyhow::anyhow!("Not a vector"))
    }
}
impl ConvertInto<Vec<Value>> for Value {
    fn convert_into(self) -> Result<Vec<Value>> {
        Some(self).convert_into()
    }
}
impl ConvertInto<Option<Value>> for Vec<Value> {
    fn convert_into(self) -> Result<Option<Value>> {
        Ok(Some(Value::new(self)))
    }
}
impl ConvertInto<Result<Vec<Option<Value>>>> for Vec<Value> {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(Ok(vec![self.convert_into()?]))
    }
}

// --- Identity and Value Passthrough ---

impl ConvertInto<Option<Value>> for Option<Value> {
    fn convert_into(self) -> Result<Option<Value>> {
        Ok(self)
    }
}
impl ConvertInto<Result<Vec<Option<Value>>>> for Option<Value> {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(Ok(vec![self]))
    }
}

impl ConvertInto<Option<Value>> for Value {
    fn convert_into(self) -> Result<Option<Value>> {
        Ok(Some(self))
    }
}
impl ConvertInto<Result<Vec<Option<Value>>>> for Value {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(Ok(vec![Some(self)]))
    }
}

impl ConvertInto<Value> for Option<Value> {
    fn convert_into(self) -> Result<Value> {
        self.context("Missing value")
    }
}

impl ConvertInto<Value> for Value {
    fn convert_into(self) -> Result<Value> {
        Ok(self)
    }
}

// --- Tuples ---

macro_rules! impl_tuple_convert {
    ($($name:ident),*) => {
        impl<$($name: ConvertInto<Option<Value>>),*> ConvertInto<Result<Vec<Option<Value>>>> for ($($name,)*) {
            #[allow(non_snake_case)]
            fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
                let ($($name,)*) = self;
                Ok(Ok(vec![$($name.convert_into()?,)*]))
            }
        }
    };
}

impl_tuple_convert!(A);
impl_tuple_convert!(A, B);
impl_tuple_convert!(A, B, C);
impl_tuple_convert!(A, B, C, D);
impl_tuple_convert!(A, B, C, D, E);
impl_tuple_convert!(A, B, C, D, E, F);
impl_tuple_convert!(A, B, C, D, E, F, G);
impl_tuple_convert!(A, B, C, D, E, F, G, H);
impl_tuple_convert!(A, B, C, D, E, F, G, H, I);
impl_tuple_convert!(A, B, C, D, E, F, G, H, I, J);
impl_tuple_convert!(A, B, C, D, E, F, G, H, I, J, K);
impl_tuple_convert!(A, B, C, D, E, F, G, H, I, J, K, L);

// --- Result handling ---

impl<T: ConvertInto<Option<Value>>> ConvertInto<Result<Vec<Option<Value>>>> for Result<T> {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(Ok(vec![self?.convert_into()?]))
    }
}
impl ConvertInto<Result<Vec<Option<Value>>>> for Result<Vec<Option<Value>>> {
    fn convert_into(self) -> Result<Result<Vec<Option<Value>>>> {
        Ok(self)
    }
}
