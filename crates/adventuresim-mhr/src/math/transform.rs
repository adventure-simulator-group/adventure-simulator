use crate::math::{IDENTITY, Mat4, Quat, Vec3, quat_mul, quat_normalize, rotate_vector};

/// A rigid-plus-uniform-scale transform, i.e. one momentum skeleton state.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: IDENTITY,
        scale: 1.0,
    };

    /// `self * other`, the momentum skeleton-state product.
    pub fn compose(&self, other: &Self) -> Self {
        let rotated = rotate_vector(self.rotation, other.translation);
        Self {
            translation: [
                self.translation[0] + self.scale * rotated[0],
                self.translation[1] + self.scale * rotated[1],
                self.translation[2] + self.scale * rotated[2],
            ],
            rotation: quat_mul(self.rotation, other.rotation),
            scale: self.scale * other.scale,
        }
    }

    pub fn inverse(&self) -> Self {
        let inv_rotation = [
            -self.rotation[0],
            -self.rotation[1],
            -self.rotation[2],
            self.rotation[3],
        ];
        let inv_scale = 1.0 / self.scale;
        let rotated = rotate_vector(inv_rotation, self.translation);
        Self {
            translation: rotated.map(|v| -inv_scale * v),
            rotation: inv_rotation,
            scale: inv_scale,
        }
    }

    /// Decomposes an affine matrix, assuming uniform scale and no shear.
    ///
    /// This is momentum's `Transform::fromMatrix`: the scale is the norm of the
    /// first linear column, and the rotation is what remains after dividing it out.
    pub fn from_matrix(m: &Mat4) -> Self {
        let scale = (m[0][0] * m[0][0] + m[1][0] * m[1][0] + m[2][0] * m[2][0]).sqrt();
        let inv = if scale > 1e-12 { 1.0 / scale } else { 1.0 };
        let linear = [
            [m[0][0] * inv, m[0][1] * inv, m[0][2] * inv],
            [m[1][0] * inv, m[1][1] * inv, m[1][2] * inv],
            [m[2][0] * inv, m[2][1] * inv, m[2][2] * inv],
        ];
        let trace = linear[0][0] + linear[1][1] + linear[2][2];
        let quat = if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            [
                (linear[2][1] - linear[1][2]) / s,
                (linear[0][2] - linear[2][0]) / s,
                (linear[1][0] - linear[0][1]) / s,
                0.25 * s,
            ]
        } else if linear[0][0] > linear[1][1] && linear[0][0] > linear[2][2] {
            let s = (1.0 + linear[0][0] - linear[1][1] - linear[2][2]).sqrt() * 2.0;
            [
                0.25 * s,
                (linear[0][1] + linear[1][0]) / s,
                (linear[0][2] + linear[2][0]) / s,
                (linear[2][1] - linear[1][2]) / s,
            ]
        } else if linear[1][1] > linear[2][2] {
            let s = (1.0 + linear[1][1] - linear[0][0] - linear[2][2]).sqrt() * 2.0;
            [
                (linear[0][1] + linear[1][0]) / s,
                0.25 * s,
                (linear[1][2] + linear[2][1]) / s,
                (linear[0][2] - linear[2][0]) / s,
            ]
        } else {
            let s = (1.0 + linear[2][2] - linear[0][0] - linear[1][1]).sqrt() * 2.0;
            [
                (linear[0][2] + linear[2][0]) / s,
                (linear[1][2] + linear[2][1]) / s,
                0.25 * s,
                (linear[1][0] - linear[0][1]) / s,
            ]
        };

        Self {
            translation: [m[0][3], m[1][3], m[2][3]],
            rotation: quat_normalize(quat),
            scale,
        }
    }

    /// `[tx, ty, tz, qx, qy, qz, qw, s]`, the momentum skeleton-state layout.
    pub fn to_skel_state(self) -> [f32; 8] {
        let q = quat_normalize(self.rotation);
        [
            self.translation[0] as f32,
            self.translation[1] as f32,
            self.translation[2] as f32,
            q[0] as f32,
            q[1] as f32,
            q[2] as f32,
            q[3] as f32,
            self.scale as f32,
        ]
    }
}
