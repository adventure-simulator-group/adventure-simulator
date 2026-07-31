//! Deterministic, observer-safe lifecycle acceptance scenario.
//!
//! This is deliberately an offline tier. It exercises the authoritative pure
//! rules shared by reducers, but does not claim that a SpacetimeDB reducer or
//! browser projection was invoked.

mod model;
mod privacy;
mod runner;

pub use model::*;
pub use runner::{run_lifecycle_acceptance, write_lifecycle_acceptance};
