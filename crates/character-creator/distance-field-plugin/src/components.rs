use bevy::prelude::*;

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
