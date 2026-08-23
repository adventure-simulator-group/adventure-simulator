use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Font {
    pub family: String,
    pub data: Option<Vec<u8>>,
}

impl Font {
    pub fn new(family: String, data: Option<Vec<u8>>) -> Self {
        Self { family, data }
    }
}
