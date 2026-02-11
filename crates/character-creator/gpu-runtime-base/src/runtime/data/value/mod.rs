use crate::{ConvertInto, DataType, Error, Result};
#[allow(unused_imports)]
use anyhow::Context;
use enum_as_inner::EnumAsInner;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{any::Any, fmt::Debug, sync::Arc};

#[derive(Debug, Clone, EnumAsInner, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    String(String),
    Number(f64),
    Boolean(bool),
    Object(IndexMap<String, Value>),
    Vector(Vec<Value>),
    #[serde(skip)]
    Any((Arc<dyn Any + Send + Sync + 'static>, std::string::String)),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::String(s1), Value::String(s2)) => s1 == s2,
            (Value::Number(n1), Value::Number(n2)) => n1 == n2,
            (Value::Boolean(b1), Value::Boolean(b2)) => b1 == b2,
            (Value::Object(o1), Value::Object(o2)) => o1 == o2,
            (Value::Vector(a1), Value::Vector(a2)) => a1 == a2,
            (Value::Any((a1, _)), Value::Any((a2, _))) => Arc::ptr_eq(a1, a2),
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Value {
    pub fn new<T: Send + Sync + 'static>(value: T) -> Self {
        use std::any::TypeId;

        let tid = TypeId::of::<T>();

        if tid == TypeId::of::<Value>() {
            let val = unsafe { std::ptr::read(&value as *const T as *const Value) };
            std::mem::forget(value);
            return val;
        }

        if tid == TypeId::of::<IndexMap<String, Value>>() {
            let val =
                unsafe { std::ptr::read(&value as *const T as *const IndexMap<String, Value>) };
            std::mem::forget(value);
            return Value::Object(val);
        }

        if tid == TypeId::of::<Vec<Value>>() {
            let val = unsafe { std::ptr::read(&value as *const T as *const Vec<Value>) };
            std::mem::forget(value);
            return Value::Vector(val);
        }

        if tid == TypeId::of::<String>() {
            let s = unsafe { std::ptr::read(&value as *const T as *const String) };
            std::mem::forget(value);
            return Value::String(s);
        }

        if tid == TypeId::of::<&str>() {
            let s = unsafe { std::ptr::read(&value as *const T as *const &str) };
            std::mem::forget(value);
            return Value::String(s.to_string());
        }

        macro_rules! cast_num {
            ($($t:ty),*) => {
                $(
                    if tid == TypeId::of::<$t>() {
                        let v = unsafe { std::ptr::read(&value as *const T as *const $t) };
                        std::mem::forget(value);
                        return Value::Number(v as f64);
                    }
                )*
            };
        }

        cast_num!(f64, f32, i64, i32, i16, i8, u64, u32, u16, u8, isize, usize);

        if tid == TypeId::of::<bool>() {
            let v = unsafe { std::ptr::read(&value as *const T as *const bool) };
            std::mem::forget(value);
            return Value::Boolean(v);
        }

        Self::new_any(value)
    }

    pub fn data_type(&self) -> DataType {
        match self {
            Value::String(_) => DataType::String,
            Value::Number(_) => DataType::Number,
            Value::Boolean(_) => DataType::Boolean,
            Value::Object(_) => DataType::Object,
            Value::Vector(_) => DataType::Vector,
            Value::Any((_, type_name)) => DataType::Any(type_name.clone()),
        }
    }

    pub fn new_any<T: Send + Sync + 'static>(value: T) -> Self {
        let type_name = std::any::type_name::<T>().to_string();
        Value::Any((Arc::new(value), type_name))
    }
}

// --- From implementations using Value::new ---

macro_rules! impl_from {
    ($($t:ty),*) => {
        $(
            impl From<$t> for Value {
                fn from(value: $t) -> Self {
                    Value::new(value)
                }
            }
        )*
    };
}

impl_from!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, usize, isize, bool, String);

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_string())
    }
}

impl From<IndexMap<String, Value>> for Value {
    fn from(value: IndexMap<String, Value>) -> Self {
        Value::Object(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Value::Vector(value)
    }
}

// --- TryFrom implementations delegating to ConvertInto ---

impl TryFrom<Value> for f64 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value.convert_into()
    }
}

impl TryFrom<Value> for bool {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value.convert_into()
    }
}

impl TryFrom<Value> for String {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value.convert_into()
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Number(n) => write!(f, "{}", n),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Object(o) => write!(f, "{:?}", o),
            Value::Vector(a) => write!(f, "{:?}", a),
            Value::Any((_, type_name)) => write!(f, "{}", type_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_constructor() {
        // Primitive numbers
        assert_eq!(Value::new(1u8), Value::Number(1.0));
        assert_eq!(Value::new(123i32), Value::Number(123.0));
        assert_eq!(Value::new(45.6f64), Value::Number(45.6));

        // Booleans
        assert_eq!(Value::new(true), Value::Boolean(true));
        assert_eq!(Value::new(false), Value::Boolean(false));

        // Strings
        assert_eq!(Value::new("hello"), Value::String("hello".to_string()));
        assert_eq!(
            Value::new("world".to_string()),
            Value::String("world".to_string())
        );

        // Value passthrough
        let nested = Value::Number(10.0);
        assert_eq!(Value::new(nested), Value::Number(10.0));

        // Custom type (falls back to new_any)
        #[derive(Debug, PartialEq, Clone)]
        struct Custom(i32);
        let custom = Custom(42);
        let val = Value::new(custom.clone());
        match val {
            Value::Any((arc, _)) => {
                let downcasted = arc.downcast_ref::<Custom>().unwrap();
                assert_eq!(downcasted, &custom);
            }
            _ => panic!("Expected Value::Any"),
        }
    }
}
