use bevy::prelude::*;
use shrinkwraprs::Shrinkwrap;

#[derive(Component, Clone, Copy, Debug, Reflect)]
pub enum SdfShape {
    Sphere { radius: f32 },
    Box { size: Vec3 },
    // We can add torus, etc. later
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
pub enum SdfOperation {
    Union,
    Intersection,
    Subtraction,
}

impl Default for SdfOperation {
    fn default() -> Self {
        Self::Union
    }
}

#[derive(Component, Shrinkwrap)]
#[shrinkwrap(mutable)]
pub struct DistanceField(pub distance_field::Field<f32>);

impl DistanceField {
    pub fn new(x: usize, y: usize, z: usize) -> Self {
        Self(distance_field::Field::new_distance_field(x, y, z))
    }
}
