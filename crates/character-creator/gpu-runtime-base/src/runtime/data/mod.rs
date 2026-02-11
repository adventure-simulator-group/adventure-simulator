mod type_;
mod value;

pub use type_::*;
pub use value::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Data {
    #[serde(rename = "type")]
    pub type_: DataType,
    pub value: Option<Value>,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            type_: DataType::String,
            value: None,
        }
    }
}

impl From<Value> for Data {
    fn from(value: Value) -> Self {
        let type_ = value.data_type();
        Self {
            type_,
            value: Some(value),
        }
    }
}
