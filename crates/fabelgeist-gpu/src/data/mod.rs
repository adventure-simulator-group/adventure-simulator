pub mod camera;
pub mod gpu;
pub mod view;

pub mod layout;
pub mod skeleton;
pub mod ui;

// The maths and the animation model now live in their own crates, which know
// nothing about wgpu. They are re-exported under their original paths so that
// `fabelgeist_gpu::data::{vector, matrix, transform, math, animation}` keeps
// resolving for everything downstream.
pub use fabelgeist_animation::animation;
pub use fabelgeist_math::{math, matrix, transform, vector};

pub use camera::*;
pub use gpu::*;
pub use layout::*;
pub use math::*;
pub use matrix::*;
pub use skeleton::*;
pub use transform::*;
pub use ui::*;
pub use vector::*;
pub use view::*;
