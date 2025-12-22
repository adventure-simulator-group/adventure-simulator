mod distance;
mod voxel;

pub use distance::*;
pub use voxel::*;

#[derive(Clone)]
pub struct Field<T> {
    data: Vec<T>,
    width: usize,
    height: usize,
    depth: usize,
}

impl<T> Field<T> {
    pub fn get(&self, x: usize, y: usize, z: usize) -> &T {
        &self.data[x + y * self.width + z * self.width * self.height]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, value: T) {
        self.data[x + y * self.width + z * self.width * self.height] = value;
    }

    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.width, self.height, self.depth)
    }
}
