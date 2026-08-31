//! Standalone semantic and geometric prototype for procedural buildings.
//!
//! This crate intentionally has no dependency on either the strategic or the
//! tactical runtime. It turns a high-level [`BuildingProgram`] into a bounded,
//! deterministic [`BuildingPlan`] that a renderer or future authoritative
//! gameplay adapter can consume.

mod audit;
mod collision;
mod detail;
mod doors;
mod generator;
mod lod;
mod model;
mod roof_tessellation;

pub use audit::{AuditIssue, MeshAuditReport, audit_plan, audit_triangle_mesh};
pub use collision::{
    BuildingCollision, CollisionBounds, CollisionCuboid, compile_building_collision,
};
pub use detail::{BuildingDetail, compile_building_detail, compile_static_building_detail};
pub use doors::{DoorSpec, compile_operable_doors};
pub use generator::{GenerationError, edit_document, generate, generate_document, set_roof_pitch};
pub use lod::{
    BuildingLod, BuildingLodLevel, BuildingLodMaterial, FacadeRun, FacadeRunPath, LodMesh,
    LodVertex, compile_building_lod,
};
pub use model::*;
pub use roof_tessellation::{RoofSurfaceTriangle, tessellate_roof_enclosure, tessellate_roof_face};
