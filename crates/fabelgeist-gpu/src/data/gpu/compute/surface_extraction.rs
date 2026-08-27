use fabelgeist_math::Vec3;

/// Dimensions of the source lattice used for implicit-surface extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceGrid {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl SurfaceGrid {
    pub const fn new(width: u32, height: u32, depth: u32) -> Self {
        Self {
            width,
            height,
            depth,
        }
    }

    pub const fn cell_count(self) -> u64 {
        self.width as u64 * self.height as u64 * self.depth as u64
    }
}

/// Isosurface value sampled from the distance field.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceThreshold(f32);

impl SurfaceThreshold {
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

/// Maximum number of vertices allocated for a surface mesh.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexCapacity(u32);

impl VertexCapacity {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Maximum number of indices allocated for an indexed surface mesh.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexCapacity(u32);

impl IndexCapacity {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Geometry and sampling controls shared by GPU surface extractors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceExtractionSettings {
    pub grid: SurfaceGrid,
    pub threshold: SurfaceThreshold,
    pub scale: Vec3,
    pub offset: Vec3,
}

impl SurfaceExtractionSettings {
    pub const fn new(
        grid: SurfaceGrid,
        threshold: SurfaceThreshold,
        scale: Vec3,
        offset: Vec3,
    ) -> Self {
        Self {
            grid,
            threshold,
            scale,
            offset,
        }
    }
}

/// Output buffer capacities for an indexed surface mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexedMeshCapacity {
    pub vertices: VertexCapacity,
    pub indices: IndexCapacity,
}

impl IndexedMeshCapacity {
    pub const fn new(vertices: VertexCapacity, indices: IndexCapacity) -> Self {
        Self { vertices, indices }
    }

    pub const fn equal(vertices_and_indices: u32) -> Self {
        Self::new(
            VertexCapacity::new(vertices_and_indices),
            IndexCapacity::new(vertices_and_indices),
        )
    }
}
