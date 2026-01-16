use crate::prelude::*;

pub use distance_field::SdfOperation;

#[derive(Component, Default, Clone, Copy, Debug, PartialEq)]
pub struct SdfOperationComponent(pub SdfOperation);

