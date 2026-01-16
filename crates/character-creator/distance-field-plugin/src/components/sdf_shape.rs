use crate::prelude::*;

#[derive(Component, Clone, Copy, Debug, Reflect)]
pub enum SdfShape {
    Sphere { radius: f32 },
    Box { size: Vec3 },
    // We can add torus, etc. later
}
