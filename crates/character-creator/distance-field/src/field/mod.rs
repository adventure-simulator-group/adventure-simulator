#[derive(Clone)]
pub struct Field<T> where T: Send + Sync + 'static {
    pub data: Vec<T>,
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub voxel_size: f32,
}

impl<T> Field<T> where T: Send + Sync + 'static {
    pub fn new(width: usize, height: usize, depth: usize, voxel_size: f32, initial_value: T) -> Self 
    where T: Clone {
        Self {
            data: vec![initial_value; width * height * depth],
            width,
            height,
            depth,
            voxel_size,
        }
    }

    pub fn set_all(&mut self, value: T)
    where T: Clone
    {
        self.data.fill(value);
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
