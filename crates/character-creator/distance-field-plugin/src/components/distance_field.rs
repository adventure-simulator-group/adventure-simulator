use crate::prelude::*;

#[derive(Component, Shrinkwrap)]
#[shrinkwrap(mutable)]
pub struct DistanceField(pub distance_field::Field<f32>);

impl DistanceField {
    pub fn new(x: usize, y: usize, z: usize, voxel_size: f32) -> Self {
        Self(distance_field::Field::new_distance_field(x, y, z, voxel_size))
    }
}
