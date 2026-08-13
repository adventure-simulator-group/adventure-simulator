use crate::presentation::TreeLeafRepresentation;

pub(super) const TREE_BILLBOARD_TRANSITION_SCALES: [f32; 3] = [0.357, 0.345, 0.333];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TreeLightingModeId {
    Baseline,
    AmbientOcclusion,
    Shadows,
    Combined,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CapturePose {
    Ground,
    TreeReview,
    RecursiveTree,
    Root,
    BranchJunction,
    Rock,
    TerrainGrazing,
    GrassSeam,
    Debris,
    GroundCover,
    LeafSpecimen,
    TreeLod { distance: f32 },
    Overhead,
    Horizon,
    VistaPeak,
    VistaValley,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DetailRequirement {
    None,
    TreeFocus,
    BranchFocusWithLeafSuppression,
    RockFocus,
    GrassSuppressed,
    GrassPresent,
    DebrisPair,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CaptureViewSpec {
    pub slug: &'static str,
    pub label: &'static str,
    pub overlay: bool,
    pub fov_degrees: f32,
    pub minimum_foreground_bps: u16,
    pub lighting_mode: TreeLightingModeId,
    pub render_lod_override: Option<u8>,
    pub validated_forced_lod: Option<u8>,
    pub leaf_lod_override: Option<TreeLeafRepresentation>,
    pub projected_scale: Option<f32>,
    pub specimen_leaf: Option<TreeLeafRepresentation>,
    pub suppress_leaves: bool,
    pub suppress_grass: bool,
    pub vista_visible: bool,
    pub hide_obstacles: bool,
    pub show_tree_backdrop: bool,
    pub detail_requirement: DetailRequirement,
    pub pose: CapturePose,
    pub warmup: bool,
    pub observe_recursive_lod: bool,
    pub debris_target: bool,
}

impl CaptureViewSpec {
    pub const fn new(
        slug: &'static str,
        label: &'static str,
        pose: CapturePose,
        fov_degrees: f32,
        minimum_foreground_bps: u16,
    ) -> Self {
        Self {
            slug,
            label,
            overlay: false,
            fov_degrees,
            minimum_foreground_bps,
            lighting_mode: TreeLightingModeId::Combined,
            render_lod_override: None,
            validated_forced_lod: None,
            leaf_lod_override: None,
            projected_scale: None,
            specimen_leaf: None,
            suppress_leaves: false,
            suppress_grass: false,
            vista_visible: false,
            hide_obstacles: false,
            show_tree_backdrop: false,
            detail_requirement: DetailRequirement::None,
            pose,
            warmup: false,
            observe_recursive_lod: false,
            debris_target: false,
        }
    }
    pub const fn overlay(mut self) -> Self {
        self.overlay = true;
        self
    }
    pub const fn lighting(mut self, value: TreeLightingModeId) -> Self {
        self.lighting_mode = value;
        self
    }
    pub const fn render_lod(mut self, value: u8) -> Self {
        self.render_lod_override = Some(value);
        self
    }
    pub const fn validated_lod(mut self, value: u8) -> Self {
        self.validated_forced_lod = Some(value);
        self
    }
    pub const fn leaf_lod(mut self, value: TreeLeafRepresentation) -> Self {
        self.leaf_lod_override = Some(value);
        self
    }
    pub const fn scale(mut self, value: f32) -> Self {
        self.projected_scale = Some(value);
        self
    }
    pub const fn specimen(mut self, value: TreeLeafRepresentation) -> Self {
        self.specimen_leaf = Some(value);
        self
    }
    pub const fn suppress_leaves(mut self) -> Self {
        self.suppress_leaves = true;
        self
    }
    pub const fn suppress_grass(mut self) -> Self {
        self.suppress_grass = true;
        self
    }
    pub const fn vista(mut self) -> Self {
        self.vista_visible = true;
        self
    }
    pub const fn hide_obstacles(mut self) -> Self {
        self.hide_obstacles = true;
        self
    }
    pub const fn backdrop(mut self) -> Self {
        self.show_tree_backdrop = true;
        self
    }
    pub const fn detail(mut self, value: DetailRequirement) -> Self {
        self.detail_requirement = value;
        self
    }
    pub const fn warmup(mut self) -> Self {
        self.warmup = true;
        self
    }
    pub const fn recursive(mut self) -> Self {
        self.observe_recursive_lod = true;
        self
    }
    pub const fn debris(mut self) -> Self {
        self.debris_target = true;
        self
    }
}

macro_rules! v {
    ($slug:literal, $label:literal, $pose:expr, $fov:expr, $min:expr) => {
        CaptureViewSpec::new($slug, $label, $pose, $fov, $min)
    };
}

pub(super) const CAPTURE_VIEWS: [CaptureViewSpec; 29] = [
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
        "Ground-level beauty view",
        CapturePose::Ground,
        65.0,
        1000
    )
    .vista(),
    v!(
        "tree-detail",
        "Whole-tree individual-leaf LOD view",
        CapturePose::TreeReview,
        48.0,
        1000
    ),
    v!(
        "tree-lighting-baseline",
        "Tree lighting baseline without canopy AO or shadows",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .lighting(TreeLightingModeId::Baseline),
    v!(
        "tree-lighting-ao",
        "Tree lighting with WebGPU-safe canopy ambient occlusion",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .lighting(TreeLightingModeId::AmbientOcclusion),
    v!(
        "tree-lighting-shadows",
        "Tree lighting with directional leaf self shadows",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .lighting(TreeLightingModeId::Shadows),
    v!(
        "tree-lighting-combined",
        "Tree lighting with canopy AO and directional self shadows",
        CapturePose::TreeReview,
        48.0,
        1000
    ),
    v!(
        "tree-recursive-lod",
        "Mixed recursive tree LOD view",
        CapturePose::RecursiveTree,
        80.0,
        200
    )
    .recursive(),
    v!(
        "ground-cover",
        "Tree-canopy leaf-litter and grass boundary",
        CapturePose::GroundCover,
        65.0,
        1000
    ),
    v!(
        "tree-silhouette",
        "Neutral English oak silhouette plate",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .backdrop(),
    v!(
        "tree-textured-leaf-detail",
        "Eight-triangle cambered PBR terminal-shoot close-up",
        CapturePose::LeafSpecimen,
        30.0,
        1000
    )
    .specimen(TreeLeafRepresentation::TexturedMesh),
    v!(
        "tree-leaf-card-detail",
        "Two-triangle textured terminal-shoot close-up",
        CapturePose::LeafSpecimen,
        30.0,
        1000
    )
    .specimen(TreeLeafRepresentation::AlphaCard),
    v!(
        "tree-textured-leaf-lod",
        "Rendered eight-triangle cambered leaf LOD view",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .render_lod(0)
    .leaf_lod(TreeLeafRepresentation::TexturedMesh),
    v!(
        "tree-leaf-card-lod",
        "Rendered two-triangle leaf LOD view",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .render_lod(0)
    .leaf_lod(TreeLeafRepresentation::AlphaCard),
    v!(
        "tree-leaf-transition-25",
        "Cambered-to-flat leaf transition 25% view",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .scale(0.60),
    v!(
        "tree-leaf-transition-50",
        "Cambered-to-flat leaf transition 50% view",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .scale(0.50),
    v!(
        "tree-leaf-transition-75",
        "Cambered-to-flat leaf transition 75% view",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .scale(0.40),
    v!(
        "tree-twig-lod",
        "Leafed-twig tree LOD view",
        CapturePose::TreeLod { distance: 30.0 },
        30.0,
        200
    )
    .render_lod(1)
    .validated_lod(1),
    v!(
        "tree-small-branch-lod",
        "Small-branch tree LOD view",
        CapturePose::TreeLod { distance: 48.0 },
        19.0,
        200
    )
    .render_lod(2)
    .validated_lod(2),
    v!(
        "tree-crown-lod",
        "Crown-branch tree LOD view",
        CapturePose::TreeLod { distance: 72.0 },
        13.0,
        200
    )
    .render_lod(3)
    .validated_lod(3),
    v!(
        "tree-billboard-lod",
        "Whole-tree billboard LOD view",
        CapturePose::TreeLod { distance: 118.0 },
        8.0,
        200
    )
    .render_lod(4)
    .validated_lod(4),
    v!(
        "tree-crown-transition-fixed",
        "Fixed-camera crown LOD transition control",
        CapturePose::TreeLod { distance: 92.0 },
        20.0,
        200
    )
    .render_lod(3)
    .validated_lod(3),
    v!(
        "tree-billboard-transition-fixed",
        "Fixed-camera billboard LOD transition control",
        CapturePose::TreeLod { distance: 92.0 },
        20.0,
        200
    )
    .render_lod(4)
    .validated_lod(4),
    v!(
        "tree-billboard-transition-25",
        "Natural crown-to-billboard transition 25% view",
        CapturePose::TreeLod { distance: 92.0 },
        20.0,
        200
    )
    .scale(TREE_BILLBOARD_TRANSITION_SCALES[0]),
    v!(
        "tree-billboard-transition-50",
        "Natural crown-to-billboard transition 50% view",
        CapturePose::TreeLod { distance: 92.0 },
        20.0,
        200
    )
    .scale(TREE_BILLBOARD_TRANSITION_SCALES[1]),
    v!(
        "tree-billboard-transition-75",
        "Natural crown-to-billboard transition 75% view",
        CapturePose::TreeLod { distance: 92.0 },
        20.0,
        200
    )
    .scale(TREE_BILLBOARD_TRANSITION_SCALES[2]),
    v!(
        "beauty-overhead",
        "Overhead distribution view",
        CapturePose::Overhead,
        65.0,
        1
    )
    .vista(),
    v!(
        "horizon",
        "Horizon and distant-vista view",
        CapturePose::Horizon,
        15.0,
        50
    )
    .vista(),
    v!(
        "collision-overlay",
        "Obstacle collider overlay",
        CapturePose::Ground,
        65.0,
        1000
    )
    .overlay(),
];

pub(super) const ENVIRONMENT_REVIEW_VIEWS: [CaptureViewSpec; 12] = [
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
        "grass-seam-detail",
        "Grass macro-patch seam and density detail",
        CapturePose::GrassSeam,
        42.0,
        1000
    )
    .detail(DetailRequirement::GrassPresent),
    v!(
        "forest-floor-debris-detail",
        "Fallen oak leaves and twig geometry close-up",
        CapturePose::Debris,
        39.6,
        500
    )
    .debris()
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
