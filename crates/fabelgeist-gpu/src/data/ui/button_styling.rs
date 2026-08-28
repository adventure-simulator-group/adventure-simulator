use super::TextStyle;
use fabelgeist_math::Vec4;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ButtonStyling {
    pub normal_fill_color: Vec4,
    pub hover_fill_color: Vec4,
    pub pressed_fill_color: Vec4,
    pub normal_stroke_color: Vec4,
    pub hover_stroke_color: Vec4,
    pub pressed_stroke_color: Vec4,
    pub stroke_thickness: f32,
    pub corner_radius: f32,
    pub text_style: TextStyle,
    pub normal_text_color: Vec4,
    pub hover_text_color: Vec4,
    pub pressed_text_color: Vec4,
}

impl Default for ButtonStyling {
    fn default() -> Self {
        Self {
            normal_fill_color: Vec4::new(0.25, 0.25, 0.28, 1.0),
            hover_fill_color: Vec4::new(0.35, 0.35, 0.38, 1.0),
            pressed_fill_color: Vec4::new(0.18, 0.18, 0.20, 1.0),
            normal_stroke_color: Vec4::new(0.40, 0.40, 0.44, 1.0),
            hover_stroke_color: Vec4::new(0.50, 0.50, 0.55, 1.0),
            pressed_stroke_color: Vec4::new(0.60, 0.60, 0.65, 1.0),
            stroke_thickness: 1.5,
            corner_radius: 6.0,
            text_style: TextStyle {
                font: None,
                font_size: 16.0,
                line_height: 20.0,
            },
            normal_text_color: Vec4::new(0.95, 0.95, 0.95, 1.0),
            hover_text_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            pressed_text_color: Vec4::new(0.80, 0.80, 0.80, 1.0),
        }
    }
}
