use crate::data::{Mat3, Vec3, Vec4};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct Mat4 {
    pub columns: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}


impl Mat4 {
    pub fn new(a: Vec4, b: Vec4, c: Vec4, d: Vec4) -> Self {
        Self {
            columns: [
                [a.x, a.y, a.z, a.w],
                [b.x, b.y, b.z, b.w],
                [c.x, c.y, c.z, c.w],
                [d.x, d.y, d.z, d.w],
            ],
        }
    }

    pub fn get(&self, column: usize, row: usize) -> f32 {
        if column < 4 && row < 4 {
            self.columns[column][row]
        } else {
            0.0
        }
    }

    pub fn identity() -> Self {
        Self {
            columns: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation(euler: Vec3) -> Self {
        let m3 = Mat3::rotation(euler);
        Self {
            columns: [
                [m3.columns[0][0], m3.columns[0][1], m3.columns[0][2], 0.0],
                [m3.columns[1][0], m3.columns[1][1], m3.columns[1][2], 0.0],
                [m3.columns[2][0], m3.columns[2][1], m3.columns[2][2], 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}
