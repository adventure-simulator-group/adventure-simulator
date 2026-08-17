pub mod blend_shapes;
pub mod character;
pub mod mesh;
pub mod skeleton;
pub mod skin_weights;

pub use blend_shapes::BlendShapes;
pub use character::Character;
pub use mesh::Mesh;
pub use skeleton::Skeleton;
pub use skin_weights::{MAX_SKIN_JOINTS, SkinWeights};

/// `tx, ty, tz, rx, ry, rz, sc`.
pub const PARAMETERS_PER_JOINT: usize = 7;
