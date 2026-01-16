use crate::prelude::*;

#[derive(Component, Reflect)]
pub struct SdfConfig {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub voxel_size: f32,
    pub center: Vec3,
}

impl Default for SdfConfig {
    fn default() -> Self {
        Self {
            width: 36,
            height: 36,
            depth: 36,
            voxel_size: 0.12,
            center: Vec3::ZERO,
        }
    }
}
