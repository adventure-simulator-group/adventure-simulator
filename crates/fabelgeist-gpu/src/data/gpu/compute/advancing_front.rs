//! GPU advancing-front extraction for implicit surfaces.
//!
//! On a regular distance-field lattice, every sign-changing lattice edge is an
//! active front.  The four incident cells supply projected vertex candidates;
//! joining those candidates closes the front with two triangles.  Ownership by
//! lattice edge makes the candidates non-overlapping, which is the structured
//! grid equivalent of SAFT's collision/selection phase.
//!
//! The underlying surface-front pipeline is shared with dual contouring: gather
//! finds active cells/fronts, scan allocates compact output ranges, and stream
//! projects candidates and emits triangles.  Keeping this specialization as a
//! thin facade avoids maintaining a second copy of the sizeable WGSL kernels.

use crate::data::gpu::buffer::Buffer;
use crate::data::gpu::compute::dual_contouring::{DualContouring, DualContouringDefinition};
use crate::data::gpu::compute::{IndexedMeshCapacity, SurfaceExtractionSettings};
use crate::data::gpu::resource::GpuResource;
use crate::globals::WgpuContext;
use anyhow::Result;

/// Compute resources used by the advancing-front extractor.
pub type AdvancingFrontDefinition = DualContouringDefinition;

pub struct AdvancingFront;

impl AdvancingFront {
    /// Extract an indexed triangle mesh from a 3D distance field.
    ///
    /// The capacity independently bounds projected front candidates and
    /// emitted triangle indices.
    pub fn execute(
        context: &WgpuContext,
        definition: &AdvancingFrontDefinition,
        sdf: &GpuResource,
        settings: SurfaceExtractionSettings,
        capacity: IndexedMeshCapacity,
    ) -> Result<(Buffer, Buffer, Buffer, Buffer)> {
        DualContouring::execute_advancing_front(context, definition, sdf, settings, capacity)
    }
}
