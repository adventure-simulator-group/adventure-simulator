use bevy_math::Vec3;

use crate::Field;

pub type Distance = f32;
pub type DistanceField = Field<Distance>;

impl DistanceField {
    pub fn new_distance_field(width: usize, height: usize, depth: usize, voxel_size: f32) -> Self {
        Self::new(width, height, depth, voxel_size, Distance::INFINITY)
    }

    pub fn clear(&mut self) {
        self.set_all(Distance::INFINITY);
    }

    pub fn add_sphere(&mut self, center: Vec3, radius: f32, voxel_size: f32) {
        // Simple SDF addition (min)
        let (width, height, depth) = self.dimensions();
        let origin = Vec3::new(
            -(width as f32 - 1.0) * 0.5 * voxel_size,
            -(height as f32 - 1.0) * 0.5 * voxel_size,
            -(depth as f32 - 1.0) * 0.5 * voxel_size,
        );

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let world_position = origin + Vec3::new(
                        x as f32 * voxel_size,
                        y as f32 * voxel_size,
                        z as f32 * voxel_size,
                    );
                    let distance = world_position.distance(center) - radius;
                    let current = self.get(x, y, z);
                    self.set(x, y, z, current.min(distance));
                }
            }
        }
    }
}
