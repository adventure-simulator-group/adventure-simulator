use super::*;

/// Production-parity plates for implicit terrain and geological landforms.
///
/// Every recorded view takes a second settled readback so material reviews
/// fail closed when streaming or presentation state changes between samples.
/// The suite deliberately avoids tree, understory, debris, rock-scatter, and
/// grass-presence focus requirements: geological fixtures need only provide
/// the terrain and optional landform under review.
pub(in crate::tactical_scene_viewer) const LANDFORM_REVIEW_VIEWS: [CaptureViewSpec; 8] = [
    v!(
        "warmup",
        "Render-pipeline warmup",
        CapturePose::Ground,
        65.0,
        1000
    )
    .warmup()
    .vista(),
    v!(
        "beauty-ground",
        "Ground-level landform context",
        CapturePose::Ground,
        65.0,
        1000
    )
    .vista()
    .settled_readback_pair(),
    v!(
        "beauty-overhead",
        "Overhead landform and playable-terrain composition",
        CapturePose::Overhead,
        65.0,
        1
    )
    .vista()
    .settled_readback_pair(),
    v!(
        "terrain-grazing-detail",
        "Terrain material under grazing light",
        CapturePose::TerrainGrazing,
        42.0,
        1000
    )
    .suppress_grass()
    .settled_readback_pair(),
    v!(
        "fault-scarp",
        "Fault-scarp cliff face",
        CapturePose::FaultScarp,
        45.0,
        1000
    )
    .suppress_grass()
    .settled_readback_pair(),
    v!(
        "fault-scarp-seam",
        "Fault-scarp patch and heightfield seam",
        CapturePose::FaultScarpSeam,
        44.0,
        1000
    )
    .suppress_grass()
    .settled_readback_pair(),
    v!(
        "landform-underside",
        "Landform underside and terrain contact",
        CapturePose::LandformUnderside,
        55.0,
        1000
    )
    .suppress_grass()
    .settled_readback_pair(),
    v!(
        "vista-lod-oblique",
        "Landform in regional terrain context",
        CapturePose::VistaPeak,
        65.0,
        1000
    )
    .vista()
    .hide_obstacles()
    .settled_readback_pair(),
];
