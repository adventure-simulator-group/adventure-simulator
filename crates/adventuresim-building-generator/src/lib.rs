//! Standalone semantic and geometric prototype for procedural buildings.
//!
//! This crate intentionally has no dependency on either the strategic or the
//! tactical runtime. It turns a high-level [`BuildingProgram`] into a bounded,
//! deterministic [`BuildingPlan`] that a renderer or future authoritative
//! gameplay adapter can consume.

mod generator;
mod model;

pub use generator::{GenerationError, generate};
pub use model::*;
