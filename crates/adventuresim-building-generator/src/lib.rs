//! Standalone semantic and geometric prototype for procedural buildings.
//!
//! This crate intentionally has no dependency on either the strategic or the
//! tactical runtime. It turns a high-level [`BuildingProgram`] into a bounded,
//! deterministic [`BuildingPlan`] that a renderer or future authoritative
//! gameplay adapter can consume.

mod audit;
mod generator;
mod model;

pub use audit::{AuditIssue, MeshAuditReport, audit_plan, audit_triangle_mesh};
pub use generator::{GenerationError, edit_document, generate, generate_document, set_roof_pitch};
pub use model::*;
