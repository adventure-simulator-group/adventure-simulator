use bevy::prelude::*;

#[derive(Resource, Clone)]
pub struct Field<T> where T: Send + Sync + 'static {
    data: Vec<T>,
    width: usize,
    height: usize,
    depth: usize,
}

impl<T> Field<T> where T: Send + Sync + 'static {
    pub fn new(width: usize, height: usize, depth: usize, initial_value: T) -> Self 
    where T: Clone {
        Self {
            data: vec![initial_value; width * height * depth],
            width,
            height,
            depth,
        }
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> &T {
        &self.data[x + y * self.width + z * self.width * self.height]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, value: T) {
        self.data[x + y * self.width + z * self.width * self.height] = value;
    }

    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.width, self.height, self.depth)
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.data.iter_mut()
    }
}

pub type Distance = f32;
pub type DistanceField = Field<Distance>;

impl DistanceField {
    pub fn new_distance_field(width: usize, height: usize, depth: usize) -> Self {
        Self::new(width, height, depth, Distance::INFINITY)
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
