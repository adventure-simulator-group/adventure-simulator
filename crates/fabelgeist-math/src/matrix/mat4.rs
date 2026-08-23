use crate::{Mat3, Vec3, Vec4};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable,
)]
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
    pub fn from_cols_array_2d(columns: &[[f32; 4]; 4]) -> Self {
        Self { columns: *columns }
    }
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

    pub fn from_translation(translation: Vec3) -> Self {
        Self::from_trs(
            translation,
            Vec4::new(0.0, 0.0, 0.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
        )
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

    pub fn from_trs(translation: Vec3, rotation: Vec4, scale: Vec3) -> Self {
        let q = rotation;
        let x2 = q.x + q.x;
        let y2 = q.y + q.y;
        let z2 = q.z + q.z;
        let xx = q.x * x2;
        let xy = q.x * y2;
        let xz = q.x * z2;
        let yy = q.y * y2;
        let yz = q.y * z2;
        let zz = q.z * z2;
        let wx = q.w * x2;
        let wy = q.w * y2;
        let wz = q.w * z2;

        let mut res = Self::identity();
        res.columns[0][0] = (1.0 - (yy + zz)) * scale.x;
        res.columns[0][1] = (xy + wz) * scale.x;
        res.columns[0][2] = (xz - wy) * scale.x;

        res.columns[1][0] = (xy - wz) * scale.y;
        res.columns[1][1] = (1.0 - (xx + zz)) * scale.y;
        res.columns[1][2] = (yz + wx) * scale.y;

        res.columns[2][0] = (xz + wy) * scale.z;
        res.columns[2][1] = (yz - wx) * scale.z;
        res.columns[2][2] = (1.0 - (xx + yy)) * scale.z;

        res.columns[3][0] = translation.x;
        res.columns[3][1] = translation.y;
        res.columns[3][2] = translation.z;
        res.columns[3][3] = 1.0;

        res
    }

    pub fn perspective(fovy_rad: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fovy_rad / 2.0).tan();
        Self {
            columns: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, far / (near - far), -1.0],
                [0.0, 0.0, (far * near) / (near - far), 0.0],
            ],
        }
    }

    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        Self {
            columns: [
                [2.0 / (right - left), 0.0, 0.0, 0.0],
                [0.0, 2.0 / (top - bottom), 0.0, 0.0],
                [0.0, 0.0, 1.0 / (near - far), 0.0],
                [
                    -(right + left) / (right - left),
                    -(top + bottom) / (top - bottom),
                    near / (near - far),
                    1.0,
                ],
            ],
        }
    }

    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);

        Self {
            columns: [
                [s.x, u.x, -f.x, 0.0],
                [s.y, u.y, -f.y, 0.0],
                [s.z, u.z, -f.z, 0.0],
                [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
            ],
        }
    }

    pub fn inverse(&self) -> Option<Self> {
        let m = self.columns;
        let s0 = m[0][0] * m[1][1] - m[1][0] * m[0][1];
        let s1 = m[0][0] * m[1][2] - m[1][0] * m[0][2];
        let s2 = m[0][0] * m[1][3] - m[1][0] * m[0][3];
        let s3 = m[0][1] * m[1][2] - m[1][1] * m[0][2];
        let s4 = m[0][1] * m[1][3] - m[1][1] * m[0][3];
        let s5 = m[0][2] * m[1][3] - m[1][2] * m[0][3];

        let c5 = m[2][2] * m[3][3] - m[3][2] * m[2][3];
        let c4 = m[2][1] * m[3][3] - m[3][1] * m[2][3];
        let c3 = m[2][1] * m[3][2] - m[3][1] * m[2][2];
        let c2 = m[2][0] * m[3][3] - m[3][0] * m[2][3];
        let c1 = m[2][0] * m[3][2] - m[3][0] * m[2][2];
        let c0 = m[2][0] * m[3][1] - m[3][0] * m[2][1];

        let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;

        if det.abs() < 1e-9 {
            return None;
        }

        let inv_det = 1.0 / det;

        let mut res = Self::identity();
        res.columns[0][0] = (m[1][1] * c5 - m[1][2] * c4 + m[1][3] * c3) * inv_det;
        res.columns[0][1] = (-m[0][1] * c5 + m[0][2] * c4 - m[0][3] * c3) * inv_det;
        res.columns[0][2] = (m[3][1] * s5 - m[3][2] * s4 + m[3][3] * s3) * inv_det;
        res.columns[0][3] = (-m[2][1] * s5 + m[2][2] * s4 - m[2][3] * s3) * inv_det;

        res.columns[1][0] = (-m[1][0] * c5 + m[1][2] * c2 - m[1][3] * c1) * inv_det;
        res.columns[1][1] = (m[0][0] * c5 - m[0][2] * c2 + m[0][3] * c1) * inv_det;
        res.columns[1][2] = (-m[3][0] * s5 + m[3][2] * s2 - m[3][3] * s1) * inv_det;
        res.columns[1][3] = (m[2][0] * s5 - m[2][2] * s2 + m[2][3] * s1) * inv_det;

        res.columns[2][0] = (m[1][0] * c4 - m[1][1] * c2 + m[1][3] * c0) * inv_det;
        res.columns[2][1] = (-m[0][0] * c4 + m[0][1] * c2 - m[0][3] * c0) * inv_det;
        res.columns[2][2] = (m[3][0] * s4 - m[3][1] * s2 + m[3][3] * s0) * inv_det;
        res.columns[2][3] = (-m[2][0] * s4 + m[2][1] * s2 - m[2][3] * s0) * inv_det;

        res.columns[3][0] = (-m[1][0] * c3 + m[1][1] * c1 - m[1][2] * c0) * inv_det;
        res.columns[3][1] = (m[0][0] * c3 - m[0][1] * c1 + m[0][2] * c0) * inv_det;
        res.columns[3][2] = (-m[3][0] * s3 + m[3][1] * s1 - m[3][2] * s0) * inv_det;
        res.columns[3][3] = (m[2][0] * s3 - m[2][1] * s1 + m[2][2] * s0) * inv_det;

        Some(res)
    }
}

impl std::ops::Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let mut res = Self::identity();
        for i in 0..4 {
            for j in 0..4 {
                res.columns[i][j] = self.columns[0][j] * rhs.columns[i][0]
                    + self.columns[1][j] * rhs.columns[i][1]
                    + self.columns[2][j] * rhs.columns[i][2]
                    + self.columns[3][j] * rhs.columns[i][3];
            }
        }
        res
    }
}

impl std::fmt::Display for Mat4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}))",
            self.columns[0][0],
            self.columns[0][1],
            self.columns[0][2],
            self.columns[0][3],
            self.columns[1][0],
            self.columns[1][1],
            self.columns[1][2],
            self.columns[1][3],
            self.columns[2][0],
            self.columns[2][1],
            self.columns[2][2],
            self.columns[2][3],
            self.columns[3][0],
            self.columns[3][1],
            self.columns[3][2],
            self.columns[3][3]
        )
    }
}
