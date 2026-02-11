use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}


impl Vec2 {
    pub fn new(x: f32, y: f32) -> Vec2 {
        Self { x, y }
    }

    pub fn break_(self) -> (f32, f32) {
        (self.x, self.y)
    }
}
