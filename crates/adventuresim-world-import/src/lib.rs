//! Offline world compiler.
//!
//! Source modules parse their own formats. The outer builder combines those
//! source models into the canonical, source-independent import schema.

pub mod builder;
pub mod error;
pub mod sources;
pub mod validation;

pub use builder::WorldBuilder;
pub use error::{Error, Result};
