use std::any::TypeId;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, Eq)]
pub enum DataType {
    Number,
    String,
    Boolean,
    Vector,
    Object,
    Any(std::string::String),
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::Number => write!(f, "Number"),
            DataType::String => write!(f, "String"),
            DataType::Boolean => write!(f, "Boolean"),
            DataType::Vector => write!(f, "Vector"),
            DataType::Object => write!(f, "Object"),
            DataType::Any(dtype) => write!(f, "{}", dtype),
        }
    }
}

impl DataType {
    pub fn of<T: ?Sized + 'static>() -> Self {
        let id = TypeId::of::<T>();
        if id == TypeId::of::<f64>() {
            DataType::Number
        } else if id == TypeId::of::<String>() {
            DataType::String
        } else if id == TypeId::of::<bool>() {
            DataType::Boolean
        } else if id == TypeId::of::<Vec<Value>>() {
            DataType::Vector
        } else if id == TypeId::of::<IndexMap<String, Value>>() {
            DataType::Object
        } else {
            DataType::Any(std::any::type_name::<T>().to_string())
        }
    }
}
