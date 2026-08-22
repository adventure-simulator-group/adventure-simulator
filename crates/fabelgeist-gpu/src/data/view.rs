use crate::data::matrix::Mat4;
use crate::data::transform::Transform;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub projection: Mat4,
    pub transform: Transform,
}

impl Default for View {
    fn default() -> Self {
        Self {
            projection: Mat4::identity(),
            transform: Transform::default(),
        }
    }
}

impl View {
    pub fn new(projection: Mat4, transform: Transform) -> Self {
        Self {
            projection,
            transform,
        }
    }
}
