use crate::data::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct UiContext {
    pub container_position: Vec2,
}

impl UiContext {
    pub fn new(container_position: Vec2) -> Self {
        Self { container_position }
    }
}
