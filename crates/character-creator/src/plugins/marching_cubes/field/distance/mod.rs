use super::Field;
use crate::plugins::MarchingCubes;
use bevy::math::Vec3;

pub type Distance = f32;
pub type DistanceField = Field<Distance>;

impl DistanceField {
    pub fn new(width: usize, height: usize, depth: usize) -> Self {
        Self {
            data: vec![Distance::INFINITY; width * height * depth],
            width,
            height,
            depth,
        }
    }

    pub fn add_sphere(&mut self, center: Vec3, radius: f32, voxel_size: f32) {
        let (width, height, depth) = self.dimensions();
        let origin = MarchingCubes::grid_origin(width, height, depth, voxel_size);

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let world_position =
                        MarchingCubes::sample_to_world(origin, x, y, z, voxel_size);
                    let distance = world_position.distance(center) - radius;
                    let current = self.get(x, y, z);
                    self.set(x, y, z, current.min(distance));
                }
            }
        }
    }
}
