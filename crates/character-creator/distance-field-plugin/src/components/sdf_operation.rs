use crate::prelude::*;

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
