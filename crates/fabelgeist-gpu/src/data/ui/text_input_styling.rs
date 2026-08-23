use super::TextStyle;
use crate::data::vector::Vec4;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextInputStyling {
    pub normal_fill_color: Vec4,
    pub hover_fill_color: Vec4,
    pub focused_fill_color: Vec4,
    pub normal_stroke_color: Vec4,
    pub hover_stroke_color: Vec4,
    pub focused_stroke_color: Vec4,
    pub stroke_thickness: f32,
    pub corner_radius: f32,
    pub text_style: TextStyle,
    pub normal_text_color: Vec4,
    pub hover_text_color: Vec4,
    pub focused_text_color: Vec4,
    pub placeholder_text_color: Vec4,
}

impl Default for TextInputStyling {
    fn default() -> Self {
        Self {
            normal_fill_color: Vec4::new(0.15, 0.15, 0.17, 1.0),
            hover_fill_color: Vec4::new(0.18, 0.18, 0.20, 1.0),
            focused_fill_color: Vec4::new(0.10, 0.10, 0.12, 1.0),
            normal_stroke_color: Vec4::new(0.30, 0.30, 0.33, 1.0),
            hover_stroke_color: Vec4::new(0.40, 0.40, 0.44, 1.0),
            focused_stroke_color: Vec4::new(0.50, 0.30, 0.90, 1.0), // Violet accent
            stroke_thickness: 1.5,
            corner_radius: 6.0,
            text_style: TextStyle {
                font: None,
                font_size: 16.0,
                line_height: 20.0,
            },
            normal_text_color: Vec4::new(0.95, 0.95, 0.95, 1.0),
            hover_text_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            focused_text_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            placeholder_text_color: Vec4::new(0.70, 0.70, 0.75, 1.0),
        }
    }
}

impl TextInputStyling {
    pub fn new(
        normal_fill_color: Vec4,
        hover_fill_color: Vec4,
        focused_fill_color: Vec4,
        normal_stroke_color: Vec4,
        hover_stroke_color: Vec4,
        focused_stroke_color: Vec4,
        stroke_thickness: f32,
        corner_radius: f32,
        text_style: TextStyle,
        normal_text_color: Vec4,
        hover_text_color: Vec4,
        focused_text_color: Vec4,
        placeholder_text_color: Vec4,
    ) -> Self {
        Self {
            normal_fill_color,
            hover_fill_color,
            focused_fill_color,
            normal_stroke_color,
            hover_stroke_color,
            focused_stroke_color,
            stroke_thickness,
            corner_radius,
            text_style,
            normal_text_color,
            hover_text_color,
            focused_text_color,
            placeholder_text_color,
        }
    }
}
