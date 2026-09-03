use crate::presentation::TreeLeafRepresentation;
use bevy::prelude::Vec3;

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
    AnimationPlay {
        yaw_degrees: f32,
    },
    AnimationPlayBoundary {
        player_x: f32,
        player_z: f32,
        yaw_degrees: f32,
    },
    AnimationPlayObstruction {
        yaw_degrees: f32,
    },
    BuildingInterior {
        camera: u8,
    },
    CityExterior {
        camera: u8,
    },
    TreeColdTraversal {
        distance: f32,
    },
    TreeReview,
    RecursiveTree,
    Root,
    BranchJunction,
    Rock,
    TerrainGrazing,
    FaultScarp,
    FaultScarpSeam,
    LandformUnderside,
    GrassSeam,
    Debris,
    GroundCover,
    LeafSpecimen,
    UnderstoryReview {
        distance: f32,
        elevation_degrees: f32,
        orbit_degrees: f32,
    },
    TreeLod {
        distance: f32,
    },
    Overhead,
    CityOblique {
        azimuth_degrees: f32,
    },
    Horizon,
    VistaPeak,
    VistaValley,
}

impl CapturePose {
    pub(super) fn fault_camera(self) -> Option<(Vec3, Vec3, Vec3)> {
        match self {
            Self::FaultScarp => {
                Some((Vec3::new(0.0, 8.0, 24.0), Vec3::new(0.0, 2.0, 0.0), Vec3::Y))
            }
            Self::FaultScarpSeam => Some((
                Vec3::new(24.0, 5.0, 18.0),
                Vec3::new(16.5, 1.8, 7.0),
                Vec3::Y,
            )),
            Self::LandformUnderside => Some((
                Vec3::new(1.0, -3.0, 7.0),
                Vec3::new(0.0, -2.0, -1.0),
                Vec3::Y,
            )),
            _ => None,
        }
    }
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
    UnderstoryFocus,
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
    pub understory_species: Option<&'static str>,
    pub suppress_leaves: bool,
    pub suppress_grass: bool,
    pub suppress_understory: bool,
    pub vista_visible: bool,
    pub hide_obstacles: bool,
    pub show_tree_backdrop: bool,
    pub detail_requirement: DetailRequirement,
    pub pose: CapturePose,
    pub warmup: bool,
    pub observe_recursive_lod: bool,
    pub debris_target: bool,
    pub temporal_motion: bool,
    pub verify_settled_readbacks: bool,
    pub plaster_grazing_light: bool,
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
            understory_species: None,
            suppress_leaves: false,
            suppress_grass: false,
            suppress_understory: false,
            vista_visible: false,
            hide_obstacles: false,
            show_tree_backdrop: false,
            detail_requirement: DetailRequirement::None,
            pose,
            warmup: false,
            observe_recursive_lod: false,
            debris_target: false,
            temporal_motion: false,
            verify_settled_readbacks: false,
            plaster_grazing_light: false,
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
    pub const fn understory(mut self, common_name: &'static str) -> Self {
        self.understory_species = Some(common_name);
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
    pub const fn suppress_understory(mut self) -> Self {
        self.suppress_understory = true;
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
    pub const fn motion(mut self) -> Self {
        self.temporal_motion = true;
        self
    }
    pub const fn settled_readback_pair(mut self) -> Self {
        self.verify_settled_readbacks = true;
        self
    }
    pub const fn plaster_grazing_light(mut self) -> Self {
        self.plaster_grazing_light = true;
        self
    }
}

macro_rules! v {
    ($slug:literal, $label:literal, $pose:expr, $fov:expr, $min:expr) => {
        CaptureViewSpec::new($slug, $label, $pose, $fov, $min)
    };
}

pub(super) const CAPTURE_VIEWS: [CaptureViewSpec; 40] = [
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
        600
    )
    .specimen(TreeLeafRepresentation::TexturedMesh),
    v!(
        "tree-leaf-card-detail",
        "Two-triangle textured terminal-shoot close-up",
        CapturePose::LeafSpecimen,
        30.0,
        600
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
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .render_lod(1)
    .validated_lod(1),
    v!(
        "tree-small-branch-lod",
        "Small-branch tree LOD view",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .render_lod(2)
    .validated_lod(2),
    v!(
        "tree-crown-lod",
        "Crown-branch tree LOD view",
        CapturePose::TreeReview,
        48.0,
        1000
    )
    .render_lod(3)
    .validated_lod(3),
    v!(
        "tree-billboard-lod",
        "Whole-tree billboard LOD view",
        CapturePose::TreeReview,
        48.0,
        1000
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
        "understory-common-hazel",
        "Isolated common hazel review",
        CapturePose::UnderstoryReview {
            distance: 5.5,
            elevation_degrees: 5.7,
            orbit_degrees: 0.0,
        },
        38.0,
        100
    )
    .understory("common hazel")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus),
    v!(
        "understory-blackthorn",
        "Isolated blackthorn review",
        CapturePose::UnderstoryReview {
            distance: 5.5,
            elevation_degrees: 5.7,
            orbit_degrees: 0.0,
        },
        38.0,
        100
    )
    .understory("blackthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus),
    v!(
        "understory-blackthorn-gameplay-distance",
        "Blackthorn leaf cards at gameplay distance",
        CapturePose::UnderstoryReview {
            distance: 12.0,
            elevation_degrees: 2.0,
            orbit_degrees: 0.0,
        },
        38.0,
        80
    )
    .understory("blackthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus),
    v!(
        "understory-blackthorn-grazing",
        "Blackthorn leaf cards at a grazing review angle",
        CapturePose::UnderstoryReview {
            distance: 5.5,
            elevation_degrees: 1.0,
            orbit_degrees: 72.0,
        },
        32.0,
        80
    )
    .understory("blackthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus),
    v!(
        "understory-blackthorn-motion-01",
        "Blackthorn temporal traversal frame 1",
        CapturePose::UnderstoryReview {
            distance: 9.0,
            elevation_degrees: 3.0,
            orbit_degrees: -4.0,
        },
        38.0,
        80
    )
    .understory("blackthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus)
    .motion(),
    v!(
        "understory-blackthorn-motion-02",
        "Blackthorn temporal traversal frame 2",
        CapturePose::UnderstoryReview {
            distance: 9.0,
            elevation_degrees: 3.0,
            orbit_degrees: 0.0,
        },
        38.0,
        80
    )
    .understory("blackthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus)
    .motion(),
    v!(
        "understory-blackthorn-motion-03",
        "Blackthorn temporal traversal frame 3",
        CapturePose::UnderstoryReview {
            distance: 9.0,
            elevation_degrees: 3.0,
            orbit_degrees: 4.0,
        },
        38.0,
        80
    )
    .understory("blackthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus)
    .motion(),
    v!(
        "understory-blackthorn-settled",
        "Blackthorn settled readback identity control",
        CapturePose::UnderstoryReview {
            distance: 9.0,
            elevation_degrees: 3.0,
            orbit_degrees: 0.0,
        },
        38.0,
        80
    )
    .understory("blackthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus)
    .settled_readback_pair(),
    v!(
        "understory-common-hawthorn",
        "Isolated common hawthorn review",
        CapturePose::UnderstoryReview {
            distance: 5.5,
            elevation_degrees: 5.7,
            orbit_degrees: 0.0,
        },
        38.0,
        100
    )
    .understory("common hawthorn")
    .hide_obstacles()
    .backdrop()
    .detail(DetailRequirement::UnderstoryFocus),
    v!(
        "beauty-overhead",
        "Overhead distribution view",
        CapturePose::Overhead,
        65.0,
        1
    )
    .vista(),
    v!(
        "city-oblique-northwest",
        "City-scale oblique view from the northwest",
        CapturePose::CityOblique {
            azimuth_degrees: 315.0,
        },
        65.0,
        1
    )
    .vista(),
    v!(
        "city-oblique-southeast",
        "City-scale oblique view from the southeast",
        CapturePose::CityOblique {
            azimuth_degrees: 135.0,
        },
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

mod environment;
pub(super) use environment::ENVIRONMENT_REVIEW_VIEWS;
mod city;
mod interiors;
pub(super) use city::CITY_REVIEW_VIEWS;
pub(super) use interiors::INTERIOR_REVIEW_VIEWS;

pub(super) const ANIMATION_PLAY_VIEWS: [CaptureViewSpec; 23] = [
    v!(
        "warmup",
        "Animation-play production-camera warmup",
        CapturePose::AnimationPlay { yaw_degrees: 0.0 },
        80.0,
        1000
    )
    .warmup()
    .vista(),
    v!(
        "animation-play-000",
        "Animation-play camera facing north",
        CapturePose::AnimationPlay { yaw_degrees: 0.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-045",
        "Animation-play camera facing north-east",
        CapturePose::AnimationPlay { yaw_degrees: 45.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-090",
        "Animation-play camera facing east",
        CapturePose::AnimationPlay { yaw_degrees: 90.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-135",
        "Animation-play camera facing south-east",
        CapturePose::AnimationPlay { yaw_degrees: 135.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-180",
        "Animation-play camera facing south",
        CapturePose::AnimationPlay { yaw_degrees: 180.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-225",
        "Animation-play camera facing south-west",
        CapturePose::AnimationPlay { yaw_degrees: 225.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-270",
        "Animation-play camera facing west",
        CapturePose::AnimationPlay { yaw_degrees: 270.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-315",
        "Animation-play camera facing north-west",
        CapturePose::AnimationPlay { yaw_degrees: 315.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-n",
        "Production camera at the north playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: 0.0,
            player_z: 44.0,
            yaw_degrees: 180.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-ne",
        "Production camera at the north-east playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: 31.0,
            player_z: 31.0,
            yaw_degrees: 225.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-e",
        "Production camera at the east playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: 44.0,
            player_z: 0.0,
            yaw_degrees: 270.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-se",
        "Production camera at the south-east playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: 31.0,
            player_z: -31.0,
            yaw_degrees: 315.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-s",
        "Production camera at the south playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: 0.0,
            player_z: -44.0,
            yaw_degrees: 0.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-sw",
        "Production camera at the south-west playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: -31.0,
            player_z: -31.0,
            yaw_degrees: 45.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-w",
        "Production camera at the west playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: -44.0,
            player_z: 0.0,
            yaw_degrees: 90.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-boundary-nw",
        "Production camera at the north-west playable boundary facing the vista",
        CapturePose::AnimationPlayBoundary {
            player_x: -31.0,
            player_z: 31.0,
            yaw_degrees: 135.0
        },
        80.0,
        1000
    )
    .vista(),
    v!(
        "tree-family-se-playable-only",
        "South-east boundary camera with playable trees only",
        CapturePose::AnimationPlayBoundary {
            player_x: 31.0,
            player_z: -31.0,
            yaw_degrees: 315.0
        },
        80.0,
        1000
    ),
    v!(
        "tree-family-se-vista-only",
        "South-east boundary camera with vista trees only",
        CapturePose::AnimationPlayBoundary {
            player_x: 31.0,
            player_z: -31.0,
            yaw_degrees: 315.0
        },
        80.0,
        1000
    )
    .vista()
    .hide_obstacles(),
    v!(
        "animation-play-obstruction-000",
        "Production camera boom blocked by a north-side tree trunk",
        CapturePose::AnimationPlayObstruction { yaw_degrees: 0.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-obstruction-090",
        "Production camera boom blocked by an east-side tree trunk",
        CapturePose::AnimationPlayObstruction { yaw_degrees: 90.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-obstruction-180",
        "Production camera boom blocked by a south-side tree trunk",
        CapturePose::AnimationPlayObstruction { yaw_degrees: 180.0 },
        80.0,
        1000
    )
    .vista(),
    v!(
        "animation-play-obstruction-270",
        "Production camera boom blocked by a west-side tree trunk",
        CapturePose::AnimationPlayObstruction { yaw_degrees: 270.0 },
        80.0,
        1000
    )
    .vista(),
];

const fn traversal_view(slug: &'static str, label: &'static str, distance: f32) -> CaptureViewSpec {
    CaptureViewSpec::new(
        slug,
        label,
        CapturePose::TreeColdTraversal { distance },
        80.0,
        200,
    )
    .vista()
}

// Ordered temporal evidence for demand-driven tree residency. The first pass
// starts with only the distant whole-tree card warm, crosses every production
// LOD and leaf-representation handoff, retreats, then repeats the exact inward
// path with the generated assets resident. Non-warmup views intentionally use
// no settle frames or disposable readbacks in the capture driver.
pub(super) const TREE_COLD_TRAVERSAL_VIEWS: [CaptureViewSpec; 41] = [
    CaptureViewSpec::new(
        "warmup",
        "Distant-card pipeline warmup",
        CapturePose::TreeColdTraversal { distance: 120.0 },
        80.0,
        200,
    )
    .warmup()
    .vista(),
    traversal_view("tree-cold-first-090", "Cold approach at 90 metres", 90.0),
    traversal_view("tree-cold-first-072", "Cold approach at 72 metres", 72.0),
    traversal_view("tree-cold-first-062", "Cold approach at 62 metres", 62.0),
    traversal_view("tree-cold-first-058", "Cold approach at 58 metres", 58.0),
    traversal_view("tree-cold-first-050", "Cold approach at 50 metres", 50.0),
    traversal_view("tree-cold-first-042", "Cold approach at 42 metres", 42.0),
    traversal_view("tree-cold-first-034", "Cold approach at 34 metres", 34.0),
    traversal_view("tree-cold-first-030", "Cold approach at 30 metres", 30.0),
    traversal_view("tree-cold-first-026", "Cold approach at 26 metres", 26.0),
    traversal_view("tree-cold-first-022", "Cold approach at 22 metres", 22.0),
    traversal_view("tree-cold-first-018", "Cold approach at 18 metres", 18.0),
    traversal_view("tree-cold-first-014", "Cold approach at 14 metres", 14.0),
    traversal_view("tree-cold-first-010", "Cold approach at 10 metres", 10.0),
    traversal_view("tree-cold-first-007", "Cold approach at 7 metres", 7.0),
    traversal_view("tree-cold-first-005", "Cold approach at 5 metres", 5.0),
    traversal_view("tree-cold-first-003", "Cold approach at 3 metres", 3.0),
    traversal_view("tree-retreat-010", "Retreat at 10 metres", 10.0),
    traversal_view("tree-retreat-018", "Retreat at 18 metres", 18.0),
    traversal_view("tree-retreat-026", "Retreat at 26 metres", 26.0),
    traversal_view("tree-retreat-034", "Retreat at 34 metres", 34.0),
    traversal_view("tree-retreat-042", "Retreat at 42 metres", 42.0),
    traversal_view("tree-retreat-058", "Retreat at 58 metres", 58.0),
    traversal_view("tree-retreat-072", "Retreat at 72 metres", 72.0),
    traversal_view("tree-retreat-090", "Retreat at 90 metres", 90.0),
    traversal_view("tree-warm-second-090", "Warm approach at 90 metres", 90.0),
    traversal_view("tree-warm-second-072", "Warm approach at 72 metres", 72.0),
    traversal_view("tree-warm-second-062", "Warm approach at 62 metres", 62.0),
    traversal_view("tree-warm-second-058", "Warm approach at 58 metres", 58.0),
    traversal_view("tree-warm-second-050", "Warm approach at 50 metres", 50.0),
    traversal_view("tree-warm-second-042", "Warm approach at 42 metres", 42.0),
    traversal_view("tree-warm-second-034", "Warm approach at 34 metres", 34.0),
    traversal_view("tree-warm-second-030", "Warm approach at 30 metres", 30.0),
    traversal_view("tree-warm-second-026", "Warm approach at 26 metres", 26.0),
    traversal_view("tree-warm-second-022", "Warm approach at 22 metres", 22.0),
    traversal_view("tree-warm-second-018", "Warm approach at 18 metres", 18.0),
    traversal_view("tree-warm-second-014", "Warm approach at 14 metres", 14.0),
    traversal_view("tree-warm-second-010", "Warm approach at 10 metres", 10.0),
    traversal_view("tree-warm-second-007", "Warm approach at 7 metres", 7.0),
    traversal_view("tree-warm-second-005", "Warm approach at 5 metres", 5.0),
    traversal_view("tree-warm-second-003", "Warm approach at 3 metres", 3.0),
];

const fn beech_motion_view(
    slug: &'static str,
    label: &'static str,
    distance: f32,
) -> CaptureViewSpec {
    CaptureViewSpec::new(
        slug,
        label,
        CapturePose::TreeColdTraversal { distance },
        48.0,
        200,
    )
    .vista()
    .detail(DetailRequirement::TreeFocus)
    .motion()
}

// Consecutive zero-settle production frames for BeechLeaf alpha/mip review.
// The 25 cm camera steps create a slow continuous zoom and fractional-pixel
// crown motion while remaining within the live individual-leaf/card range.
pub(super) const BEECH_LEAF_MOTION_VIEWS: [CaptureViewSpec; 17] = [
    CaptureViewSpec::new(
        "warmup",
        "Common-beech leaf-card pipeline warmup",
        CapturePose::TreeColdTraversal { distance: 22.25 },
        48.0,
        200,
    )
    .warmup()
    .vista(),
    beech_motion_view("beech-leaf-motion-01", "Beech slow zoom frame 1", 22.00),
    beech_motion_view("beech-leaf-motion-02", "Beech slow zoom frame 2", 21.75),
    beech_motion_view("beech-leaf-motion-03", "Beech slow zoom frame 3", 21.50),
    beech_motion_view("beech-leaf-motion-04", "Beech slow zoom frame 4", 21.25),
    beech_motion_view("beech-leaf-motion-05", "Beech slow zoom frame 5", 21.00),
    beech_motion_view("beech-leaf-motion-06", "Beech slow zoom frame 6", 20.75),
    beech_motion_view("beech-leaf-motion-07", "Beech slow zoom frame 7", 20.50),
    beech_motion_view("beech-leaf-motion-08", "Beech slow zoom frame 8", 20.25),
    beech_motion_view("beech-leaf-motion-09", "Beech slow zoom frame 9", 20.00),
    beech_motion_view("beech-leaf-motion-10", "Beech slow zoom frame 10", 19.75),
    beech_motion_view("beech-leaf-motion-11", "Beech slow zoom frame 11", 19.50),
    beech_motion_view("beech-leaf-motion-12", "Beech slow zoom frame 12", 19.25),
    beech_motion_view("beech-leaf-motion-13", "Beech slow zoom frame 13", 19.00),
    beech_motion_view("beech-leaf-motion-14", "Beech slow zoom frame 14", 18.75),
    beech_motion_view("beech-leaf-motion-15", "Beech slow zoom frame 15", 18.50),
    beech_motion_view("beech-leaf-motion-16", "Beech slow zoom frame 16", 18.25),
];
