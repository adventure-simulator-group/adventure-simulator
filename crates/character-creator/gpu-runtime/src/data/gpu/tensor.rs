#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shape(pub Vec<u32>);

impl From<Vec<u32>> for Shape {
    fn from(v: Vec<u32>) -> Self {
        Shape(v)
    }
}
