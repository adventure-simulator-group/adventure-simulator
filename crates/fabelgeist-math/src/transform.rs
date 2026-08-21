use crate::matrix::Mat4;
use crate::vector::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Vec3, // Euler angles in degrees
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 0.0),
            rotation: Vec3::new(0.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }
}

impl Transform {
    pub fn identity() -> Self {
        Self::default()
    }

    pub fn new(position: Vec3, rotation: Vec3, scale: Vec3) -> Self {
        Self {
            position,
            rotation,
            scale,
        }
    }

    pub fn from_position(position: Vec3) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }

    pub fn from_rotation(rotation: Vec3) -> Self {
        Self {
            rotation,
            ..Default::default()
        }
    }

    pub fn from_scale(scale: Vec3) -> Self {
        Self {
            scale,
            ..Default::default()
        }
    }

    pub fn to_mat4(&self) -> Mat4 {
        let rad_x = self.rotation.x.to_radians();
        let rad_y = self.rotation.y.to_radians();
        let rad_z = self.rotation.z.to_radians();

        let cx = rad_x.cos();
        let sx = rad_x.sin();
        let cy = rad_y.cos();
        let sy = rad_y.sin();
        let cz = rad_z.cos();
        let sz = rad_z.sin();

        // Rotation matrix R = Rz * Ry * Rx
        let r00 = cy * cz;
        let r01 = sx * sy * cz - cx * sz;
        let r02 = cx * sy * cz + sx * sz;

        let r10 = cy * sz;
        let r11 = sx * sy * sz + cx * cz;
        let r12 = cx * sy * sz - sx * cz;

        let r20 = -sy;
        let r21 = sx * cy;
        let r22 = cx * cy;

        Mat4 {
            columns: [
                [
                    r00 * self.scale.x,
                    r10 * self.scale.x,
                    r20 * self.scale.x,
                    0.0,
                ],
                [
                    r01 * self.scale.y,
                    r11 * self.scale.y,
                    r21 * self.scale.y,
                    0.0,
                ],
                [
                    r02 * self.scale.z,
                    r12 * self.scale.z,
                    r22 * self.scale.z,
                    0.0,
                ],
                [self.position.x, self.position.y, self.position.z, 1.0],
            ],
        }
    }

    pub fn from_mat4(mat: Mat4) -> Self {
        // Extract translation
        let position = Vec3::new(mat.columns[3][0], mat.columns[3][1], mat.columns[3][2]);

        // Extract scale
        let sx = Vec3::new(mat.columns[0][0], mat.columns[0][1], mat.columns[0][2]).length();
        let sy = Vec3::new(mat.columns[1][0], mat.columns[1][1], mat.columns[1][2]).length();
        let sz = Vec3::new(mat.columns[2][0], mat.columns[2][1], mat.columns[2][2]).length();

        let scale = Vec3::new(sx, sy, sz);

        let m = mat.columns;

        let r20 = m[0][2] / scale.x; // -sy
        let r21 = m[1][2] / scale.y; // sx*cy
        let r22 = m[2][2] / scale.z; // cx*cy

        let ay = (-r20).clamp(-1.0, 1.0).asin();
        let (ax, az) = if ay.cos().abs() > 1e-6 {
            (r21.atan2(r22), m[0][1].atan2(m[0][0]))
        } else {
            // Gimbal lock: the Y rotation is +/-90 degrees, X and Z are no
            // longer separable, so the whole rotation is folded into X.
            // Column 1 is then (sx*sy, cx, .), and sy is +/-1, so the sign of
            // sy has to come back out or a -90 degree pitch reads inverted.
            let sin_y = -r20;
            ((sin_y * m[1][0]).atan2(m[1][1]), 0.0)
        };

        let rotation = Vec3::new(ax.to_degrees(), ay.to_degrees(), az.to_degrees());

        Self {
            position,
            rotation,
            scale,
        }
    }

    pub fn compose(&self, other: &Self) -> Self {
        Self::from_mat4(self.to_mat4() * other.to_mat4())
    }

    pub fn to_trs(&self) -> (Vec3, crate::vector::Vec4, Vec3) {
        let rad_x = self.rotation.x.to_radians();
        let rad_y = self.rotation.y.to_radians();
        let rad_z = self.rotation.z.to_radians();

        // Quaternion from Euler Rz * Ry * Rx
        let cx = (rad_x * 0.5).cos();
        let sx = (rad_x * 0.5).sin();
        let cy = (rad_y * 0.5).cos();
        let sy = (rad_y * 0.5).sin();
        let cz = (rad_z * 0.5).cos();
        let sz = (rad_z * 0.5).sin();

        let qx = sx * cy * cz - cx * sy * sz;
        let qy = cx * sy * cz + sx * cy * sz;
        let qz = cx * cy * sz - sx * sy * cz;
        let qw = cx * cy * cz + sx * sy * sz;

        (
            self.position,
            crate::vector::Vec4::new(qx, qy, qz, qw),
            self.scale,
        )
    }

    pub fn inverse(&self) -> Self {
        let mat = self.to_mat4();
        let inv_mat = mat.inverse().unwrap_or(Mat4::identity());
        Self::from_mat4(inv_mat)
    }

    pub fn add_translation(&self, translation: Vec3) -> Self {
        Self {
            position: self.position + translation,
            ..*self
        }
    }

    pub fn add_rotation(&self, rotation: Vec3) -> Self {
        Self {
            rotation: self.rotation + rotation,
            ..*self
        }
    }

    pub fn add_scale(&self, scale: Vec3) -> Self {
        Self {
            scale: self.scale * scale,
            ..*self
        }
    }
}

impl std::fmt::Display for Transform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "T: {}, R: {}, S: {}",
            self.position, self.rotation, self.scale
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_compose() {
        let t1 = Transform {
            position: Vec3::new(1.0, 0.0, 0.0),
            rotation: Vec3::new(0.0, 90.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        let t2 = Transform {
            position: Vec3::new(1.0, 0.0, 0.0),
            rotation: Vec3::new(0.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        // t1 * t2:
        // 1. apply t2 (move 1.0 in X)
        // 2. apply t1 (rotate 90 in Y, move 1.0 in X)
        // Resulting position should be (2, 0, 0) if we do M1 * M2
        // Wait, M1 * M2 means apply M2 then M1.
        // M2 translation is (1,0,0).
        // M1 translation is (1,0,0) and rotation is 90 around Y.
        // M1 * M2 * v = M1 * (v + (1,0,0)) = R1 * (v + (1,0,0)) + T1 = R1*v + R1*(1,0,0) + T1
        // R1 is 90 deg around Y. R1*(1,0,0) = (0, 0, -1)
        // So position is (1, 0, 0) + (0, 0, -1) = (1, 0, -1)
        let result = t1.compose(&t2);

        assert!((result.position.x - 1.0).abs() < 1e-5);
        assert!((result.position.y - 0.0).abs() < 1e-5);
        assert!((result.position.z - (-1.0)).abs() < 1e-5);

        assert!((result.rotation.y - 90.0).abs() < 1e-5);
    }

    #[test]
    fn test_transform_inverse() {
        let t = Transform {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Vec3::new(10.0, 20.0, 30.0),
            scale: Vec3::new(2.0, 2.0, 2.0),
        };
        let inv = t.inverse();
        let identity = t.compose(&inv);

        assert!(identity.position.length() < 1e-4);
        assert!(identity.rotation.length() < 1e-4);
        assert!((identity.scale.x - 1.0).abs() < 1e-4);
    }

    #[test]
    fn euler_extraction_survives_gimbal_lock() {
        use crate::vector::Vec4;

        // A quarter turn about Y in either direction leaves the X and Z
        // rotations degenerate, and the two signs are not symmetric.
        for pitch in [90.0, -90.0] {
            for roll in [0.0, 30.0, -120.0] {
                let original = Transform::from_rotation(Vec3::new(roll, pitch, 0.0));
                let (_, quaternion, _) = original.to_trs();
                let (_, round_tripped, _) = Transform::from_mat4(original.to_mat4()).to_trs();
                let alignment: f32 = quaternion.dot(round_tripped).abs();
                assert!(
                    (alignment - 1.0).abs() < 1e-5,
                    "pitch {pitch}, roll {roll}: {:?} became {:?}",
                    quaternion,
                    round_tripped
                );
                let _: Vec4 = quaternion;
            }
        }
    }

    #[test]
    fn test_transform_add() {
        let t = Transform {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Vec3::new(10.0, 20.0, 30.0),
            scale: Vec3::new(2.0, 2.0, 2.0),
        };

        let t_added = t
            .add_translation(Vec3::new(0.5, -0.5, 1.0))
            .add_rotation(Vec3::new(5.0, -10.0, 15.0))
            .add_scale(Vec3::new(1.5, 0.5, 2.0));

        assert_eq!(t_added.position, Vec3::new(1.5, 1.5, 4.0));
        assert_eq!(t_added.rotation, Vec3::new(15.0, 10.0, 45.0));
        assert_eq!(t_added.scale, Vec3::new(3.0, 1.0, 4.0));
    }
}
