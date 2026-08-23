use super::Font;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font: Option<Font>,
    pub font_size: f32,
    pub line_height: f32,
}

impl TextStyle {
    pub fn new(font: Option<Font>, font_size: f32, line_height: f32) -> Self {
        Self {
            font,
            font_size,
            line_height,
        }
    }
}
