use crate::data::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct Mat2 {
    pub columns: [[f32; 2]; 2],
}

impl Default for Mat2 {
    fn default() -> Self {
        Self::identity()
    }
}


impl Mat2 {
    pub fn new(a: Vec2, b: Vec2) -> Self {
        Self {
            columns: [[a.x, a.y], [b.x, b.y]],
        }
    }

    pub fn identity() -> Self {
        Self {
            columns: [[1.0, 0.0], [0.0, 1.0]],
        }
    }

    pub fn get(&self, column: usize, row: usize) -> f32 {
        if column < 2 && row < 2 {
            self.columns[column][row]
        } else {
            0.0
        }
    }
}
