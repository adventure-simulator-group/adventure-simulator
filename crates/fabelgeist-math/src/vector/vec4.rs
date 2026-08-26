use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    bytemuck::Pod,
    bytemuck::Zeroable,
)]
#[repr(C)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
        Self { x, y, z, w }
    }

    pub fn ones() -> Self {
        Self::new(1.0, 1.0, 1.0, 1.0)
    }

    pub fn from_scalar(s: f32) -> Self {
        Self::new(s, s, s, s)
    }

    pub fn break_(self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.z, self.w)
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w
    }

    pub fn slerp(self, mut other: Self, t: f32) -> Self {
        let mut dot = self.dot(other);

        if dot < 0.0 {
            other = Self::new(-other.x, -other.y, -other.z, -other.w);
            dot = -dot;
        }

        let dot = dot.clamp(-1.0, 1.0);

        if dot > 0.9995 {
            return Self::new(
                self.x + t * (other.x - self.x),
                self.y + t * (other.y - self.y),
                self.z + t * (other.z - self.z),
                self.w + t * (other.w - self.w),
            )
            .normalize();
        }

        let theta_0 = dot.acos();
        let theta = theta_0 * t;
        let sin_theta = theta.sin();
        let sin_theta_0 = theta_0.sin();

        let s0 = (theta_0 - theta).sin() / sin_theta_0;
        let s1 = sin_theta / sin_theta_0;

        Self::new(
            s0 * self.x + s1 * other.x,
            s0 * self.y + s1 * other.y,
            s0 * self.z + s1 * other.z,
            s0 * self.w + s1 * other.w,
        )
    }

    pub fn normalize(self) -> Self {
        let len_sq = self.dot(self);
        if len_sq > 0.0 {
            let inv_len = 1.0 / len_sq.sqrt();
            Self::new(
                self.x * inv_len,
                self.y * inv_len,
                self.z * inv_len,
                self.w * inv_len,
            )
        } else {
            self
        }
    }

    /// Returns the conjugate of a unit quaternion (equivalent to inverse for normalized quaternions).
    pub fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    pub fn mul_quat(self, other: Self) -> Self {
        Self::new(
            self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
            self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
        )
    }

    /// The identity quaternion, `(0, 0, 0, 1)`.
    pub fn quat_identity() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    /// A quaternion rotating `radians` around `axis`, which need not be normalized.
    pub fn from_axis_angle(axis: crate::vector::Vec3, radians: f32) -> Self {
        let axis = axis.normalize();
        let (sin, cos) = (radians * 0.5).sin_cos();
        Self::new(axis.x * sin, axis.y * sin, axis.z * sin, cos)
    }

    /// Rotates a vector by this unit quaternion, `v + 2 * (w * (a x v) + a x (a x v))`.
    pub fn rotate_vec3(self, point: crate::vector::Vec3) -> crate::vector::Vec3 {
        let axis = crate::vector::Vec3::new(self.x, self.y, self.z);
        let cross = axis.cross(point);
        point + (cross * self.w + axis.cross(cross)) * 2.0
    }

    /// Swing-twist decomposition: the part of this rotation that turns around
    /// `axis`. `twist * swing` reproduces the original rotation.
    pub fn twist_about(self, axis: crate::vector::Vec3) -> Self {
        let axis = axis.normalize();
        let rotation = crate::vector::Vec3::new(self.x, self.y, self.z);
        let projection = axis * rotation.dot(axis);
        let twist = Self::new(projection.x, projection.y, projection.z, self.w);
        if twist.dot(twist) < 1.0e-12 {
            Self::quat_identity()
        } else {
            twist.normalize()
        }
    }
}

impl From<(f32, f32, f32, f32)> for Vec4 {
    fn from((x, y, z, w): (f32, f32, f32, f32)) -> Self {
        Self::new(x, y, z, w)
    }
}

#[cfg(feature = "wgpu")]
impl From<Vec4> for wgpu::Color {
    fn from(val: Vec4) -> Self {
        wgpu::Color {
            r: val.x as f64,
            g: val.y as f64,
            b: val.z as f64,
            a: val.w as f64,
        }
    }
}

impl std::fmt::Display for Vec4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {}, {})", self.x, self.y, self.z, self.w)
    }
}
