use super::{DistanceField, Field};

pub type Voxel = bool;

pub type VoxelField = Field<Voxel>;

impl VoxelField {
    pub fn new(width: usize, height: usize, depth: usize) -> Self {
        Self {
            data: vec![false; width * height * depth],
            width,
            height,
            depth,
        }
    }
}

impl From<DistanceField> for VoxelField {
    fn from(field: DistanceField) -> Self {
        Self {
            data: field.data.iter().map(|&d| d <= 0.0).collect(),
            width: field.width,
            height: field.height,
            depth: field.depth,
        }
    }
}
