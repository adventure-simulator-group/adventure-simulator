use crate::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct Mat3 {
    pub columns: [[f32; 3]; 3],
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat3 {
    pub fn new(a: Vec3, b: Vec3, c: Vec3) -> Self {
        Self {
            columns: [[a.x, a.y, a.z], [b.x, b.y, b.z], [c.x, c.y, c.z]],
        }
    }

    pub fn identity() -> Self {
        Self {
            columns: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn rotation(euler: Vec3) -> Self {
        let rad_x = euler.x.to_radians();
        let rad_y = euler.y.to_radians();
        let rad_z = euler.z.to_radians();

        let cx = rad_x.cos();
        let sx = rad_x.sin();
        let cy = rad_y.cos();
        let sy = rad_y.sin();
        let cz = rad_z.cos();
        let sz = rad_z.sin();

        // Rotation matrix R = Rz * Ry * Rx (standard for computer graphics)
        let r00 = cy * cz;
        let r10 = cy * sz;
        let r20 = -sy;

        let r01 = sx * sy * cz - cx * sz;
        let r11 = sx * sy * sz + cx * cz;
        let r21 = sx * cy;

        let r02 = cx * sy * cz + sx * sz;
        let r12 = cx * sy * sz - sx * cz;
        let r22 = cx * cy;

        Self {
            columns: [[r00, r10, r20], [r01, r11, r21], [r02, r12, r22]],
        }
    }

    pub fn get(&self, column: usize, row: usize) -> f32 {
        if column < 3 && row < 3 {
            self.columns[column][row]
        } else {
            0.0
        }
    }
}

impl std::fmt::Display for Mat3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(({}, {}, {}), ({}, {}, {}), ({}, {}, {}))",
            self.columns[0][0],
            self.columns[0][1],
            self.columns[0][2],
            self.columns[1][0],
            self.columns[1][1],
            self.columns[1][2],
            self.columns[2][0],
            self.columns[2][1],
            self.columns[2][2]
        )
    }
}
