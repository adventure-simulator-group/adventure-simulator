use crate::prelude::*;
pub use distance_field::shape::*;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SdfShapeComponent(pub SdfShape);

impl<T: Into<SdfShape>> From<T> for SdfShapeComponent {
    fn from(shape: T) -> Self {
        Self(shape.into())
    }
}
