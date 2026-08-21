//! Prism's linear algebra: vectors, matrices, and the transform built on them.
//!
//! Nothing here knows about the GPU, animation, or the graph — it is the layer
//! every other one is written in terms of, and it is kept free of dependencies
//! so that it can be.

pub mod math;
pub mod matrix;
pub mod transform;
pub mod vector;

pub use math::*;
pub use matrix::*;
pub use transform::*;
pub use vector::*;
