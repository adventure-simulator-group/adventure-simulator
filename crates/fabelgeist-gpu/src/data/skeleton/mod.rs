//! The skeleton model, plus the GPU-resident pose built on it.
//!
//! [`Skeleton`], [`Joint`] and the skinning maths belong to `fabelgeist-animation`
//! and are re-exported here unchanged; only [`pose::Pose`], which owns a wgpu
//! buffer, is local to this crate.

pub use fabelgeist_animation::skeleton::*;

pub mod pose;
