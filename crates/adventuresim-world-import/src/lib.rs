//! Offline world compiler.
//!
//! Source modules parse their own formats. The outer builder combines those
//! source models into the canonical, source-independent import schema.

pub mod builder;
mod draft;
pub mod error;
mod sources;
pub mod spatial;
mod validation;

pub use builder::WorldBuilder;
pub use error::{Error, Result};
