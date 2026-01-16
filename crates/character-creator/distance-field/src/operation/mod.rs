#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SdfOperation {
    #[default]
    Union,
    Intersection,
    Subtraction,
}
