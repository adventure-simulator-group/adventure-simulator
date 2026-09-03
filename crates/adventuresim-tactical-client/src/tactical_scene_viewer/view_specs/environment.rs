use super::*;

pub(in crate::tactical_scene_viewer) const ENVIRONMENT_REVIEW_VIEWS: [CaptureViewSpec; 15] = [
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
        "Ground-level environment context",
        CapturePose::Ground,
        65.0,
        1000
    )
    .vista(),
    v!(
        "beauty-overhead",
        "Overhead playable-area and terrain composition",
        CapturePose::Overhead,
        65.0,
        1
    )
    .vista(),
    v!(
        "tree-root-detail",
        "Tree root flare and forest-floor detail",
        CapturePose::Root,
        38.0,
        350
    )
    .suppress_grass()
    .suppress_understory()
    .detail(DetailRequirement::TreeFocus),
    v!(
        "tree-branch-junction",
        "Trunk and primary-branch junction detail",
        CapturePose::BranchJunction,
        38.0,
        350
    )
    .suppress_leaves()
    .detail(DetailRequirement::BranchFocusWithLeafSuppression),
    v!(
        "rock-detail",
        "Procedural rock surface and ground contact detail",
        CapturePose::Rock,
        38.0,
        350
    )
    .detail(DetailRequirement::RockFocus),
    v!(
        "terrain-grazing-detail",
        "Ground material under grazing light",
        CapturePose::TerrainGrazing,
        42.0,
        1000
    )
    .suppress_grass()
    .detail(DetailRequirement::GrassSuppressed),
    v!(
        "fault-scarp",
        "Fault-scarp cliff face",
        CapturePose::FaultScarp,
        45.0,
        1000
    )
    .suppress_grass(),
    v!(
        "fault-scarp-seam",
        "Fault-scarp patch and heightfield seam",
        CapturePose::FaultScarpSeam,
        44.0,
        1000
    )
    .suppress_grass()
    .detail(DetailRequirement::GrassSuppressed),
    v!(
        "landform-underside",
        "Landform underside and contact",
        CapturePose::LandformUnderside,
        55.0,
        1000
    )
    .suppress_grass(),
    v!(
        "grass-seam-detail",
        "Grass macro-patch seam and density detail",
        CapturePose::GrassSeam,
        42.0,
        1000
    )
    .detail(DetailRequirement::GrassPresent),
    v!(
        "forest-floor-debris-detail",
        "Unobstructed forest-floor leaf-bed, twig, and pebble review",
        CapturePose::Debris,
        44.0,
        500
    )
    .debris()
    .suppress_grass()
    .detail(DetailRequirement::DebrisPair),
    v!(
        "horizon",
        "Horizon, Sun, Moon, and atmosphere context",
        CapturePose::Horizon,
        15.0,
        50
    )
    .vista(),
    v!(
        "vista-lod-oblique",
        "Playable edge and distant terrain LOD composition",
        CapturePose::VistaPeak,
        65.0,
        1000
    )
    .vista()
    .hide_obstacles(),
    v!(
        "vista-valley-oblique",
        "Playable edge and lowest regional terrain composition",
        CapturePose::VistaValley,
        65.0,
        1000
    )
    .vista(),
];
