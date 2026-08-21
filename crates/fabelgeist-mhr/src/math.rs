//! Small `f64` rotation/transform helpers used while building a character.
//!
//! Quaternions are stored `[x, y, z, w]`, matching momentum's skeleton state
//! layout `[tx, ty, tz, qx, qy, qz, qw, s]`.

pub type Quat = [f64; 4];
pub type Vec3 = [f64; 3];
/// Row-major 4x4 matrix.
pub type Mat4 = [[f64; 4]; 4];

pub const IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

/// Hamilton product `a * b`.
pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [x1, y1, z1, w1] = a;
    let [x2, y2, z2, w2] = b;
    [
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2,
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
    ]
}

pub fn quat_normalize(q: Quat) -> Quat {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < 1e-12 {
        IDENTITY
    } else {
        q.map(|v| v / n)
    }
}

/// Rotation about a coordinate axis (0 = x, 1 = y, 2 = z).
fn quat_axis(axis: usize, angle: f64) -> Quat {
    let (s, c) = (angle * 0.5).sin_cos();
    let mut q = [0.0, 0.0, 0.0, c];
    q[axis] = s;
    q
}

/// Euler angles (radians) composed in the given order, momentum-style: the
/// axis listed first is applied first, so XYZ yields `Rz * Ry * Rx`.
pub fn quat_from_euler(angles: Vec3, order: [usize; 3]) -> Quat {
    let mut result = IDENTITY;
    for axis in order {
        result = quat_mul(quat_axis(axis, angles[axis]), result);
    }
    result
}

/// FBX stores Euler angles in degrees.
pub fn quat_from_euler_degrees(angles: Vec3, order: [usize; 3]) -> Quat {
    let radians = angles.map(f64::to_radians);
    quat_from_euler(radians, order)
}

/// FBX `RotationOrder` enum -> the axis order used by [`quat_from_euler`].
pub fn rotation_order(order: i64) -> [usize; 3] {
    match order {
        1 => [0, 2, 1], // XZY
        2 => [1, 2, 0], // YZX
        3 => [1, 0, 2], // YXZ
        4 => [2, 0, 1], // ZXY
        5 => [2, 1, 0], // ZYX
        _ => [0, 1, 2], // XYZ (and spherical, which MHR does not use)
    }
}

/// Shepperd's method, matching `Eigen::Quaternion(Matrix3)`.
pub fn quat_from_matrix(m: [[f64; 3]; 3]) -> Quat {
    let trace = m[0][0] + m[1][1] + m[2][2];
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            0.25 * s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[2][1] - m[1][2]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
            (m[0][2] - m[2][0]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
            (m[1][0] - m[0][1]) / s,
        ]
    }
}

pub fn quat_to_matrix(q: Quat) -> [[f64; 3]; 3] {
    let [x, y, z, w] = quat_normalize(q);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

pub fn rotate_vector(q: Quat, v: Vec3) -> Vec3 {
    let m = quat_to_matrix(q);
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

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
        Self {
            translation: [m[0][3], m[1][3], m[2][3]],
            rotation: quat_normalize(quat_from_matrix(linear)),
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

/// Inverts a general affine 4x4 matrix (the bottom row must be `[0, 0, 0, 1]`).
pub fn affine_inverse(m: &Mat4) -> Mat4 {
    let a = [
        [m[0][0], m[0][1], m[0][2]],
        [m[1][0], m[1][1], m[1][2]],
        [m[2][0], m[2][1], m[2][2]],
    ];
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    let inv_det = if det.abs() > 1e-20 { 1.0 / det } else { 0.0 };

    let mut inv = [[0.0; 4]; 4];
    inv[0][0] = (a[1][1] * a[2][2] - a[1][2] * a[2][1]) * inv_det;
    inv[0][1] = (a[0][2] * a[2][1] - a[0][1] * a[2][2]) * inv_det;
    inv[0][2] = (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * inv_det;
    inv[1][0] = (a[1][2] * a[2][0] - a[1][0] * a[2][2]) * inv_det;
    inv[1][1] = (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * inv_det;
    inv[1][2] = (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * inv_det;
    inv[2][0] = (a[1][0] * a[2][1] - a[1][1] * a[2][0]) * inv_det;
    inv[2][1] = (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * inv_det;
    inv[2][2] = (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * inv_det;

    let translation = [m[0][3], m[1][3], m[2][3]];
    for row in inv.iter_mut().take(3) {
        row[3] = -(row[0] * translation[0] + row[1] * translation[1] + row[2] * translation[2]);
    }
    inv[3][3] = 1.0;
    inv
}

/// Reads a column-major FBX 4x4 matrix into a row-major [`Mat4`].
pub fn mat4_from_column_major(values: &[f64]) -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            m[row][col] = values[col * 4 + row];
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: &[f64], b: &[f64], eps: f64) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < eps, "{a:?} != {b:?}");
        }
    }

    #[test]
    fn euler_xyz_matches_the_closed_form() {
        // pymomentum's euler_xyz_to_quaternion, which the reference model uses.
        let (rx, ry, rz) = (0.3_f64, -0.7_f64, 1.1_f64);
        let (sr, cr) = (rx * 0.5).sin_cos();
        let (sp, cp) = (ry * 0.5).sin_cos();
        let (sy, cy) = (rz * 0.5).sin_cos();
        let expected = [
            sr * cp * cy - cr * sp * sy,
            cr * sp * cy + sr * cp * sy,
            cr * cp * sy - sr * sp * cy,
            cr * cp * cy + sr * sp * sy,
        ];
        approx(&quat_from_euler([rx, ry, rz], [0, 1, 2]), &expected, 1e-12);
    }

    #[test]
    fn matrix_round_trip() {
        let q = quat_normalize([0.2, -0.5, 0.31, 0.77]);
        let back = quat_from_matrix(quat_to_matrix(q));
        approx(&q, &back, 1e-12);
    }

    #[test]
    fn transform_inverse_cancels() {
        let t = Transform {
            translation: [1.5, -2.0, 3.25],
            rotation: quat_normalize([0.2, -0.5, 0.31, 0.77]),
            scale: 1.7,
        };
        let identity = t.compose(&t.inverse());
        approx(&identity.translation, &[0.0, 0.0, 0.0], 1e-12);
        assert!((identity.scale - 1.0).abs() < 1e-12);
    }

    #[test]
    fn from_matrix_recovers_a_transform() {
        let t = Transform {
            translation: [1.5, -2.0, 3.25],
            rotation: quat_normalize([0.2, -0.5, 0.31, 0.77]),
            scale: 1.7,
        };
        let r = quat_to_matrix(t.rotation);
        let mut m = [[0.0; 4]; 4];
        for row in 0..3 {
            for col in 0..3 {
                m[row][col] = r[row][col] * t.scale;
            }
            m[row][3] = t.translation[row];
        }
        m[3][3] = 1.0;

        let decoded = Transform::from_matrix(&m);
        approx(&decoded.translation, &t.translation, 1e-12);
        approx(&decoded.rotation, &t.rotation, 1e-12);
        assert!((decoded.scale - t.scale).abs() < 1e-12);
    }

    #[test]
    fn affine_inverse_matches_transform_inverse() {
        let t = Transform {
            translation: [0.5, 4.0, -1.25],
            rotation: quat_normalize([-0.1, 0.4, 0.2, 0.9]),
            scale: 0.8,
        };
        let r = quat_to_matrix(t.rotation);
        let mut m = [[0.0; 4]; 4];
        for row in 0..3 {
            for col in 0..3 {
                m[row][col] = r[row][col] * t.scale;
            }
            m[row][3] = t.translation[row];
        }
        m[3][3] = 1.0;

        let decoded = Transform::from_matrix(&affine_inverse(&m));
        let expected = t.inverse();
        approx(&decoded.translation, &expected.translation, 1e-10);
        assert!((decoded.scale - expected.scale).abs() < 1e-10);
    }
}
