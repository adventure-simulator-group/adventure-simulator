use crate::data::matrix::Mat4;
use crate::data::vector::Vec3;
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
    pub fn new(position: Vec3, rotation: Vec3, scale: Vec3) -> Self {
        Self {
            position,
            rotation,
            scale,
        }
    }

    pub fn to_mat4(&self) -> Mat4 {
        // Simple TRS matrix calculation
        // NOTE: This is a basic implementation for now.
        // Rotation is applied in ZYX order.

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
}
