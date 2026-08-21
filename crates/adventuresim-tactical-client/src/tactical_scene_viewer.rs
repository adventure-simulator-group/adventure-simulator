use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

use adventuresim_tactical_core::physics::AdventureSimulatorPhysicsPlugin;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::{
    app::ScheduleRunnerPlugin,
    camera::RenderTarget,
    camera::{Exposure, visibility::VisibilityRange},
    core_pipeline::tonemapping::Tonemapping,
    ecs::system::SystemParam,
    light::{AtmosphereEnvironmentMapLight, EnvironmentMapLight, NotShadowCaster},
    post_process::bloom::Bloom,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{ExitCondition, PresentMode},
    winit::WinitPlugin,
};
use serde::Serialize;

mod capture_state;
mod manifest;
mod view_specs;
use capture_state::{
    CapturePhase, CaptureReadback, CaptureState, foliage_detail_pixel_bps, foreground_pixel_bps,
    lighting_samples_stable, luminance_delta, mean_luminance, tree_canopy_pixel_bps,
};
use manifest::{
    CaptureManifest, CaptureRecord, CelestialProvenance, FoliageSummary,
    ObservedPresentationFeatures, ObstacleSummary, PendingCaptureManifest,
    PresentationFeatureState, PresentationFeatures, RecursiveTreeLodSummary, RepairSummary,
    TerrainSummary, TreeBakeCardSummary, TreeBakeSummary, ValidationSummary, VistaSummary,
    validation_passes,
};
#[cfg(test)]
use view_specs::TREE_BILLBOARD_TRANSITION_SCALES;
use view_specs::{
    ANIMATION_PLAY_VIEWS, CAPTURE_VIEWS, CapturePose, CaptureViewSpec, DetailRequirement,
    ENVIRONMENT_REVIEW_VIEWS, TREE_COLD_TRAVERSAL_VIEWS, TreeLightingModeId,
};

use crate::camera::CameraRigConfig;
use crate::presentation::{
    AtmosphereIblAmbientHandoff, GroundLitterCaptureAnchors, GroundLitterCapturePair,
    GroundLitterDiagnostics, GroundScatterLayer, LooseStonePebblePatch, PlayableTreeAggregateWood,
    PlayableTreeBuds, PlayableTreeCanopyCard, PlayableTreeDetailedLeaves, PlayableTreeDetailedWood,
    PlayableTreeTrunk, PresentedTree, ProceduralEnvironmentAssets, ProceduralRockVisual,
    TacticalCloudBenchmarkIsolation, TacticalCloudLayer, TacticalGraphicsSettings,
    TacticalPresentationPlugin, TacticalTreeBarkMaterial, TacticalTreeBenchmarkIsolation,
    TacticalTreeLeafCardMaterial, TerrainDetailPatch, TerrainMaterialPresentation,
    TreeAssetResidencyDiagnostics, TreeImpostorProvenance, TreeLeafRepresentation,
    TreeLeafTriangleCount, TreeLod, TreeLodCluster, TreeLodRenderOverride, TreeTrunkLod,
    VistaTerrain, VistaTreePresentation, WeatherParticle, oak_bark_material, oak_leaf_material,
    oak_review_terminal_specimen, terrain_heightmap_image,
};

const VIEW_WIDTH: u32 = 1280;
const VIEW_HEIGHT: u32 = 720;
const PERFORMANCE_VIEW_WIDTH: u32 = 2560;
const PERFORMANCE_VIEW_HEIGHT: u32 = 1440;
const PERFORMANCE_TARGET_FPS: f64 = 60.0;
const PERFORMANCE_FRAME_BUDGET_MS: f64 = 1_000.0 / PERFORMANCE_TARGET_FPS;
const STANDING_EYE_HEIGHT_METRES: f32 = 1.65;
const CAPTURE_PROFILE_VERSION: u16 = 19;
const CAMERA_VERSION: u16 = 13;
const CAPTURE_CLOCK_PHASE_SECONDS: f32 = 2.0;

#[derive(Resource)]
struct SceneSetup(Option<SceneSetupData>);

struct SceneSetupData {
    input: TacticalSceneInput,
    input_path: PathBuf,
    fixture: String,
    output: PathBuf,
    generated: GeneratedTacticalScene,
    environment: SceneEnvironment,
    settle_frames: u32,
    leaf_benchmark_frames: Option<u32>,
    tree_lighting_benchmark_frames: Option<u32>,
    scene_performance_benchmark_frames: Option<u32>,
    tree_review_azimuth_degrees: f32,
    profile: String,
    requested_views: Vec<String>,
    views: Vec<CaptureViewSpec>,
}

#[derive(Component)]
struct CaptureOverlay;

#[derive(SystemParam)]
struct LightingObservationParams<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    settings: Res<'w, TacticalGraphicsSettings>,
    ambient: Res<'w, GlobalAmbientLight>,
    ambient_handoff: Res<'w, AtmosphereIblAmbientHandoff>,
    tree_trunks: Query<'w, 's, (), With<TreeTrunkLod>>,
    presented_tree_roots: Query<'w, 's, (), With<PresentedTree>>,
    presented_tree_names: Query<'w, 's, &'static Name, With<PresentedTree>>,
    terrain: Query<'w, 's, &'static SceneTerrain>,
    litter_anchors: Query<'w, 's, &'static GroundLitterCaptureAnchors>,
    obstacle_transforms:
        Query<'w, 's, (&'static SceneObstacle, &'static GlobalTransform), Without<Camera3d>>,
    vista_trees: Query<
        'w,
        's,
        (
            &'static GlobalTransform,
            &'static TreeImpostorProvenance,
            &'static VisibilityRange,
            Has<Collider>,
        ),
        (
            With<VistaTreePresentation>,
            Without<SceneObstacle>,
            Without<Camera3d>,
        ),
    >,
}

#[derive(Component)]
struct TreeReviewBackdrop;

#[derive(Component)]
struct TreeReviewSpecimen;

const LEAF_BENCHMARK_WARMUP_FRAMES: u32 = 60;
const TREE_LIGHTING_BENCHMARK_WARMUP_FRAMES: u32 = 90;

#[derive(Resource)]
struct LeafBenchmarkState {
    sample_frames: u32,
    mode: usize,
    warmup_remaining: u32,
    samples_ms: Vec<f64>,
    results: Vec<LeafBenchmarkResult>,
}

impl LeafBenchmarkState {
    fn new(sample_frames: u32) -> Self {
        Self {
            sample_frames,
            mode: 0,
            warmup_remaining: LEAF_BENCHMARK_WARMUP_FRAMES * 2,
            samples_ms: Vec::with_capacity(sample_frames as usize),
            results: Vec::with_capacity(2),
        }
    }
}

#[derive(Serialize)]
struct LeafBenchmarkReport {
    pipeline: &'static str,
    fixture: String,
    resolution: [u32; 2],
    tree_count: usize,
    warmup_frames_per_mode: u32,
    sample_frames_per_mode: u32,
    note: &'static str,
    results: Vec<LeafBenchmarkResult>,
}

#[derive(Serialize)]
struct LeafBenchmarkResult {
    representation: &'static str,
    triangles_per_leaf: u8,
    scene_leaf_triangles: usize,
    mean_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    mean_fps: f64,
}

const LEAF_BENCHMARK_MODES: [(TreeLeafRepresentation, &str, u8); 2] = [
    (TreeLeafRepresentation::TexturedMesh, "Cambered PBR card", 8),
    (TreeLeafRepresentation::AlphaCard, "Flat alpha card", 2),
];

#[derive(Clone, Copy)]
struct TreeLightingMode {
    name: &'static str,
    ambient_occlusion_strength: f32,
    shadows_enabled: bool,
}

const TREE_LIGHTING_MODES: [TreeLightingMode; 4] = [
    TreeLightingMode {
        name: "Baseline",
        ambient_occlusion_strength: 0.0,
        shadows_enabled: false,
    },
    TreeLightingMode {
        name: "Canopy AO",
        ambient_occlusion_strength: 0.62,
        shadows_enabled: false,
    },
    TreeLightingMode {
        name: "Self shadows",
        ambient_occlusion_strength: 0.0,
        shadows_enabled: true,
    },
    TreeLightingMode {
        name: "Canopy AO + self shadows",
        ambient_occlusion_strength: 0.62,
        shadows_enabled: true,
    },
];

#[derive(Resource)]
struct TreeLightingBenchmarkState {
    sample_frames: u32,
    mode: usize,
    configured_mode: Option<usize>,
    warmup_remaining: u32,
    samples_ms: Vec<f64>,
    results: Vec<TreeLightingBenchmarkResult>,
}

impl TreeLightingBenchmarkState {
    fn new(sample_frames: u32) -> Self {
        Self {
            sample_frames,
            mode: 0,
            configured_mode: None,
            warmup_remaining: TREE_LIGHTING_BENCHMARK_WARMUP_FRAMES * 2,
            samples_ms: Vec::with_capacity(sample_frames as usize),
            results: Vec::with_capacity(TREE_LIGHTING_MODES.len()),
        }
    }
}

#[derive(Serialize)]
struct TreeLightingBenchmarkResult {
    mode: &'static str,
    ambient_occlusion: bool,
    self_shadows: bool,
    mean_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    mean_fps: f64,
}

#[derive(Serialize)]
struct TreeLightingBenchmarkReport {
    pipeline: &'static str,
    fixture: String,
    resolution: [u32; 2],
    tree_count: usize,
    warmup_frames_per_mode: u32,
    sample_frames_per_mode: u32,
    note: &'static str,
    results: Vec<TreeLightingBenchmarkResult>,
}

const SCENE_PERFORMANCE_WARMUP_FRAMES: u32 = 180;

#[derive(Clone, Copy)]
struct ScenePerformanceMode {
    name: &'static str,
    forced_lod: Option<u8>,
    forced_leaf: Option<TreeLeafRepresentation>,
    hide_playable_leaves: bool,
    hide_playable_trees: bool,
    hide_vista_trees: bool,
    hidden_scene_layers: u16,
}

const HIDE_LITTER: u16 = 1 << 0;
const HIDE_GRASS: u16 = 1 << 1;
const HIDE_UNDERSTORY: u16 = 1 << 2;
const HIDE_LOOSE_STONE: u16 = 1 << 3;
const HIDE_ROCKS: u16 = 1 << 4;
const HIDE_PLAYABLE_TERRAIN: u16 = 1 << 5;
const HIDE_VISTA_TERRAIN: u16 = 1 << 6;
const HIDE_CLOUDS: u16 = 1 << 7;
const HIDE_WEATHER: u16 = 1 << 8;
const HIDE_TREE_LEAVES: u16 = 1 << 9;
const HIDE_TREE_TRUNKS: u16 = 1 << 10;
const HIDE_TREE_BRANCHES: u16 = 1 << 11;
const HIDE_TREE_CANOPY_CARDS: u16 = 1 << 12;
const HIDE_TREE_BUDS: u16 = 1 << 13;
const HIDE_ALL_SCATTER: u16 = HIDE_LITTER | HIDE_GRASS | HIDE_UNDERSTORY | HIDE_LOOSE_STONE;

const SCENE_PERFORMANCE_MODES: [ScenePerformanceMode; 27] = [
    ScenePerformanceMode {
        name: "Natural production LODs",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "No vista trees",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: true,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "No playable trees",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: true,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "No trees",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: true,
        hide_vista_trees: true,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "No leaves",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: true,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_TREE_LEAVES | HIDE_TREE_CANOPY_CARDS,
    },
    ScenePerformanceMode {
        name: "No tree trunks",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_TREE_TRUNKS,
    },
    ScenePerformanceMode {
        name: "No tree branches",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_TREE_BRANCHES,
    },
    ScenePerformanceMode {
        name: "No tree canopy cards",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_TREE_CANOPY_CARDS,
    },
    ScenePerformanceMode {
        name: "No tree buds",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_TREE_BUDS,
    },
    ScenePerformanceMode {
        name: "No forest-floor litter",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_LITTER,
    },
    ScenePerformanceMode {
        name: "No trees or forest-floor litter",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: true,
        hide_vista_trees: true,
        hidden_scene_layers: HIDE_LITTER,
    },
    ScenePerformanceMode {
        name: "No grass",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_GRASS,
    },
    ScenePerformanceMode {
        name: "No understory",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_UNDERSTORY,
    },
    ScenePerformanceMode {
        name: "No loose stones",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_LOOSE_STONE,
    },
    ScenePerformanceMode {
        name: "No clouds",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_CLOUDS,
    },
    ScenePerformanceMode {
        name: "No weather",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_WEATHER,
    },
    ScenePerformanceMode {
        name: "No ground scatter",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_ALL_SCATTER,
    },
    ScenePerformanceMode {
        name: "No procedural rocks",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_ROCKS,
    },
    ScenePerformanceMode {
        name: "No vista terrain",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_VISTA_TERRAIN,
    },
    ScenePerformanceMode {
        name: "No playable terrain",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_PLAYABLE_TERRAIN,
    },
    ScenePerformanceMode {
        name: "Forced LOD0 cambered leaves",
        forced_lod: Some(0),
        forced_leaf: Some(TreeLeafRepresentation::TexturedMesh),
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "Forced LOD0 flat leaves",
        forced_lod: Some(0),
        forced_leaf: Some(TreeLeafRepresentation::AlphaCard),
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "Forced LOD1 cards",
        forced_lod: Some(1),
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "Forced LOD2 cards",
        forced_lod: Some(2),
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "Forced LOD3 crown cards",
        forced_lod: Some(3),
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    ScenePerformanceMode {
        name: "Forced LOD4 billboards",
        forced_lod: Some(4),
        forced_leaf: None,
        hide_playable_leaves: false,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: 0,
    },
    // The detailed-leaf mode is separate from aggregate No leaves so every
    // foliage representation can be attributed without destructive mutation.
    ScenePerformanceMode {
        name: "No detailed tree leaves",
        forced_lod: None,
        forced_leaf: None,
        hide_playable_leaves: true,
        hide_playable_trees: false,
        hide_vista_trees: false,
        hidden_scene_layers: HIDE_TREE_LEAVES,
    },
];

#[derive(Resource)]
struct ScenePerformanceBenchmarkState {
    sample_frames: u32,
    mode: usize,
    configured_mode: Option<usize>,
    warmup_remaining: u32,
    samples_ms: Vec<f64>,
    render_diagnostic_samples: BTreeMap<String, Vec<f64>>,
    playable_tree_count: Option<usize>,
    playable_leaf_entities: Option<usize>,
    vista_tree_entities: Option<usize>,
    scene_entity_counts: Option<BTreeMap<String, usize>>,
    results: Vec<ScenePerformanceBenchmarkResult>,
    stop_after_mode: usize,
}

impl ScenePerformanceBenchmarkState {
    fn new(sample_frames: u32) -> Self {
        let selected_mode = std::env::var("TACTICAL_BENCH_ONLY_MODE")
            .ok()
            .map(|requested| {
                SCENE_PERFORMANCE_MODES
                    .iter()
                    .position(|mode| mode.name == requested)
                    .unwrap_or_else(|| panic!("unknown benchmark mode {requested:?}"))
            });
        let mode = selected_mode.unwrap_or(0);
        Self {
            sample_frames,
            mode,
            configured_mode: None,
            warmup_remaining: SCENE_PERFORMANCE_WARMUP_FRAMES * 2,
            samples_ms: Vec::with_capacity(sample_frames as usize),
            render_diagnostic_samples: BTreeMap::new(),
            playable_tree_count: None,
            playable_leaf_entities: None,
            vista_tree_entities: None,
            scene_entity_counts: None,
            results: Vec::with_capacity(SCENE_PERFORMANCE_MODES.len()),
            stop_after_mode: selected_mode.unwrap_or(SCENE_PERFORMANCE_MODES.len() - 1),
        }
    }
}

#[derive(Serialize)]
struct ScenePerformanceBenchmarkResult {
    mode: &'static str,
    mean_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    worst_ms: f64,
    mean_fps: f64,
    frames_over_budget: usize,
    frames_over_budget_percent: f64,
    wall_60_fps_budget_passes: bool,
    gpu_elapsed_median_ms: Option<f64>,
    gpu_elapsed_p95_ms: Option<f64>,
    gpu_60_fps_budget_passes: Option<bool>,
    limiting_p95_ms: f64,
    headroom_ms: f64,
    budget_utilization_percent: f64,
    measured_60_fps_passes: Option<bool>,
    active_cloud_layers: usize,
    visible_cloud_layers: usize,
    visible_tree_entities: BTreeMap<String, usize>,
    tree_asset_residency: TreeAssetResidencyDiagnostics,
    render_diagnostics: BTreeMap<String, BenchmarkMetricSummary>,
    /// Positive values estimate the isolated entity family's GPU cost versus
    /// natural mode, keyed by the exact render diagnostic path.
    gpu_cost_attribution_vs_natural_ms: BTreeMap<String, BenchmarkMetricDelta>,
}

#[derive(Clone, Serialize)]
struct BenchmarkMetricSummary {
    mean: f64,
    median: f64,
    p95: f64,
    p99: f64,
}

#[derive(Serialize)]
struct BenchmarkMetricDelta {
    median_ms: f64,
    p95_ms: f64,
}

#[derive(Serialize)]
struct BenchmarkTargetHardware {
    model: &'static str,
    chip: &'static str,
    cpu: &'static str,
    gpu: &'static str,
    unified_memory_gib: u8,
    memory_bandwidth_gib_per_second: u16,
    cooling: &'static str,
}

#[derive(Serialize)]
struct BenchmarkHost {
    os: &'static str,
    architecture: &'static str,
    chip_or_cpu: Option<String>,
    gpu: Option<String>,
    memory: Option<String>,
    release_build: bool,
}

#[derive(Serialize)]
struct ScenePerformanceBenchmarkReport {
    pipeline: &'static str,
    target_fps: f64,
    frame_budget_ms: f64,
    target_hardware: BenchmarkTargetHardware,
    host: BenchmarkHost,
    host_matches_target: bool,
    target_acceptance_passes: Option<bool>,
    fixture: String,
    source_input: String,
    resolution: [u32; 2],
    playable_tree_count: usize,
    playable_leaf_entities: usize,
    vista_tree_entities: usize,
    playable_area_square_km: f64,
    scene_entity_counts: BTreeMap<String, usize>,
    warmup_frames_per_mode: u32,
    sample_frames_per_mode: u32,
    render_diagnostics_enabled: bool,
    note: &'static str,
    results: Vec<ScenePerformanceBenchmarkResult>,
}

pub(crate) fn run(
    fixture: Option<String>,
    scene_input: Option<PathBuf>,
    output: Option<PathBuf>,
    settle_frames: u32,
    canopy_bps: Option<u16>,
    absolute_minute: Option<u64>,
    leaf_benchmark_frames: Option<u32>,
    tree_lighting_benchmark_frames: Option<u32>,
    scene_performance_benchmark_frames: Option<u32>,
    scene_performance_render_diagnostics: bool,
    tree_review_azimuth_degrees: f32,
    profile: &'static str,
    requested_views: Vec<String>,
) {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (fixture, input_path) = match (fixture, scene_input) {
        (Some(fixture), None) => {
            let path = repository_root
                .join("assets/tactical-scenes")
                .join(format!("{fixture}.json"));
            (fixture, path)
        }
        (None, Some(path)) => {
            let path = absolute_from_current(path);
            let fixture = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("custom-scene")
                .to_owned();
            (fixture, path)
        }
        _ => unreachable!("argument parser enforces one scene input"),
    };
    let input = TacticalSceneInput::load(&input_path)
        .unwrap_or_else(|error| panic!("failed to load {}: {error}", input_path.display()));
    let generated = input
        .generate()
        .unwrap_or_else(|error| panic!("failed to generate tactical scene: {error}"));
    let mut environment = input.environment_snapshot(generated.digest.clone());
    if let Some(canopy_bps) = canopy_bps {
        environment.canopy_bps = canopy_bps;
    }
    if let Some(absolute_minute) = absolute_minute {
        environment.absolute_minute = absolute_minute;
    }
    let output = output.map_or_else(
        || default_output(&repository_root, &fixture, &generated.digest),
        absolute_from_current,
    );
    prepare_fresh_output(&output);
    fs::copy(&input_path, output.join("input.json"))
        .unwrap_or_else(|error| panic!("failed to copy capture input: {error}"));
    println!("CAPTURE_OUTPUT={}", output.display());

    let views = selected_capture_views(profile, &requested_views)
        .unwrap_or_else(|error| panic!("invalid capture selection: {error}"));

    let asset_root = repository_root.join("assets");
    let setup = SceneSetupData {
        input,
        input_path,
        fixture,
        output,
        generated,
        environment,
        settle_frames,
        leaf_benchmark_frames,
        tree_lighting_benchmark_frames,
        scene_performance_benchmark_frames,
        tree_review_azimuth_degrees,
        profile: profile.to_owned(),
        requested_views: views
            .iter()
            .filter(|view| view.slug != "warmup")
            .map(|view| view.slug.to_owned())
            .collect(),
        views,
    };
    let leaf_benchmarking = leaf_benchmark_frames.is_some();
    let tree_lighting_benchmarking = tree_lighting_benchmark_frames.is_some();
    let scene_performance_benchmarking = scene_performance_benchmark_frames.is_some();
    let scene_performance_render_diagnostics = scene_performance_render_diagnostics
        || std::env::var_os("TACTICAL_BENCH_RENDER_DIAGNOSTICS").is_some();
    let mut app = App::new();
    let default_plugins = DefaultPlugins
        .set(AssetPlugin {
            file_path: asset_root.to_string_lossy().into_owned(),
            ..default()
        })
        .set(WindowPlugin {
            primary_window: (!scene_performance_benchmarking).then(|| Window {
                title: "Fabelgeist tactical scene capture".into(),
                resolution: (VIEW_WIDTH, VIEW_HEIGHT).into(),
                present_mode: PresentMode::AutoNoVsync,
                resizable: false,
                decorations: false,
                ..default()
            }),
            exit_condition: if scene_performance_benchmarking {
                ExitCondition::DontExit
            } else {
                ExitCondition::OnAllClosed
            },
            ..default()
        });
    if scene_performance_benchmarking {
        // Eliminate both swapchain presentation and the desktop event-loop
        // floor. The normal RenderPlugin still renders the production camera
        // to the offscreen texture; only frame scheduling becomes unpaced.
        app.add_plugins(default_plugins.disable::<WinitPlugin>())
            .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    } else {
        app.add_plugins(default_plugins);
    }
    app.add_plugins(AdventureSimulatorPhysicsPlugin {
        enable_simulation: false,
    })
    // Visual-review plates use the exact production presentation defaults.
    // Diagnostics may hide named occluder layers, but never substitute a
    // cheaper lighting or post-processing pipeline.
    .add_plugins(capture_presentation_plugin())
    .insert_resource(ClearColor(Color::srgb_u8(158, 181, 195)))
    .insert_resource(SceneSetup(Some(setup)));
    app.insert_gizmo_config(
        PhysicsGizmos::default(),
        GizmoConfig {
            enabled: false,
            ..default()
        },
    );
    if scene_performance_benchmarking {
        app.add_systems(
            PostStartup,
            (
                setup_scene,
                redirect_performance_camera_offscreen,
                freeze_capture_clock,
            )
                .chain(),
        );
    } else {
        app.add_systems(PostStartup, (setup_scene, freeze_capture_clock).chain());
    }
    if scene_performance_benchmarking && scene_performance_render_diagnostics {
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    }
    if leaf_benchmarking {
        app.add_systems(Last, benchmark_leaf_representations);
    } else if tree_lighting_benchmarking {
        app.add_systems(Last, benchmark_tree_lighting);
    } else if scene_performance_benchmarking {
        app.add_systems(Last, benchmark_scene_performance);
    } else {
        app.add_systems(Last, capture_views);
    }
    let exit = app.run();
    if exit != AppExit::Success {
        std::process::exit(1);
    }
}

/// Render performance captures into a texture so Windows compositor pacing
/// cannot masquerade as renderer work on sub-refresh-rate frame budgets.
fn redirect_performance_camera_offscreen(
    mut commands: Commands,
    camera: Single<Entity, With<Camera3d>>,
    mut images: ResMut<Assets<Image>>,
) {
    let image = Image::new_target_texture(
        PERFORMANCE_VIEW_WIDTH,
        PERFORMANCE_VIEW_HEIGHT,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        None,
    );
    commands
        .entity(*camera)
        .insert(RenderTarget::Image(images.add(image).into()));
}

fn capture_presentation_plugin() -> TacticalPresentationPlugin {
    let mut plugin = TacticalPresentationPlugin::default();
    if std::env::var_os("TACTICAL_BENCH_DISABLE_SHADOWS").is_some() {
        plugin.shadows_enabled = false;
    }
    if std::env::var_os("TACTICAL_BENCH_DISABLE_POST_PROCESSING").is_some() {
        plugin.bloom_enabled = false;
    }
    plugin
}

fn feature_state(settings: &TacticalGraphicsSettings) -> PresentationFeatureState {
    PresentationFeatureState {
        shadows: settings.shadows_enabled,
        atmosphere: settings.atmosphere_enabled,
        celestial: settings.celestial_enabled,
        environment_light: settings.environment_light_enabled,
        environment_map_size: settings.environment_map_size,
        bloom: settings.bloom_enabled,
        max_vista_lods: settings.max_vista_lods,
    }
}

fn requested_feature_state() -> PresentationFeatureState {
    let requested = TacticalPresentationPlugin::default();
    PresentationFeatureState {
        shadows: requested.shadows_enabled,
        atmosphere: requested.atmosphere_enabled,
        celestial: requested.celestial_enabled,
        environment_light: requested.environment_light_enabled,
        environment_map_size: requested.environment_map_size,
        bloom: requested.bloom_enabled,
        max_vista_lods: requested.max_vista_lods,
    }
}

fn observed_presentation_features(
    settings: &TacticalGraphicsSettings,
    environment_map: Option<&AtmosphereEnvironmentMapLight>,
    filtered_environment_map: Option<&EnvironmentMapLight>,
    bloom: Option<&Bloom>,
    exposure: &Exposure,
    tonemapping: &Tonemapping,
    ambient: &GlobalAmbientLight,
    ambient_handoff: &AtmosphereIblAmbientHandoff,
    celestial: &CelestialProvenance,
) -> PresentationFeatures {
    let requested = requested_feature_state();
    let observed_settings = feature_state(settings);
    let environment_map_size = environment_map.map(|light| light.size.to_array());
    let observed = ObservedPresentationFeatures {
        settings: observed_settings,
        camera_environment_map: environment_map.is_some(),
        camera_environment_map_size: environment_map_size,
        camera_environment_map_allocated: filtered_environment_map.is_some(),
        camera_environment_map_intensity: filtered_environment_map.map(|light| light.intensity),
        camera_bloom: bloom.is_some(),
        camera_exposure_ev100: exposure.ev100,
        camera_tonemapping: format!("{tonemapping:?}"),
        ambient_color: ambient.color.to_linear().to_f32_array(),
        ambient_brightness: ambient.brightness,
        ambient_policy: if ambient_handoff.active {
            "atmosphere_ibl_plus_bounded_multibounce"
        } else {
            "global_ambient_fallback"
        },
        expected_ambient_brightness: if ambient_handoff.active {
            crate::presentation::scene_ibl_visibility_floor(
                celestial.sun_altitude_degrees,
                celestial.moon_altitude_degrees,
                celestial.lunar_illumination,
            )
            .1
        } else {
            crate::presentation::scene_ambient_light(
                celestial.sun_altitude_degrees,
                celestial.moon_altitude_degrees,
                celestial.lunar_illumination,
            )
            .1
        },
    };
    let requested_matches_observed = observed_settings == requested
        && observed.camera_environment_map == requested.environment_light
        && observed.camera_environment_map_size
            == requested
                .environment_light
                .then_some([requested.environment_map_size; 2])
        && observed.camera_environment_map_allocated == requested.environment_light
        && observed.camera_environment_map_intensity
            == requested.environment_light.then_some(1.0)
        && observed.camera_bloom == requested.bloom
        // Production exposure is driven by the scene's solar/lunar state and
        // may be between authored targets while the ECS observer settles.
        && observed.camera_exposure_ev100.is_finite()
        && (-1.35..=15.0).contains(&observed.camera_exposure_ev100)
        && observed.camera_tonemapping.contains("AcesFitted")
        && observed.ambient_brightness.is_finite()
        && (observed.ambient_brightness - observed.expected_ambient_brightness).abs() <= 0.01;
    PresentationFeatures {
        requested,
        observed,
        requested_matches_observed,
        weather_iteration_in_scope: false,
        water_iteration_in_scope: false,
        cloud_iteration_in_scope: false,
        cave_iteration_in_scope: false,
        characters_present: false,
    }
}

fn selected_capture_views(
    profile: &str,
    requested: &[String],
) -> Result<Vec<CaptureViewSpec>, String> {
    let profile_views = match profile {
        "semantic" => CAPTURE_VIEWS.as_slice(),
        "environment-review" => ENVIRONMENT_REVIEW_VIEWS.as_slice(),
        "animation-play" => ANIMATION_PLAY_VIEWS.as_slice(),
        "tree-cold-traversal" => TREE_COLD_TRAVERSAL_VIEWS.as_slice(),
        _ => return Err(format!("unknown profile {profile}")),
    };
    if requested.is_empty() {
        return Ok(profile_views.to_vec());
    }
    let mut selected = vec![profile_views[0]];
    let mut seen = BTreeSet::new();
    for slug in requested {
        if slug == "warmup" {
            return Err("warmup is implicit and cannot be requested".into());
        }
        if !seen.insert(slug.as_str()) {
            return Err(format!("duplicate requested view {slug}"));
        }
        let view = profile_views
            .iter()
            .find(|view| view.slug == slug)
            .copied()
            .ok_or_else(|| format!("unknown requested view {slug}"))?;
        selected.push(view);
    }
    Ok(selected)
}

fn freeze_capture_clock(mut time: ResMut<Time<Virtual>>) {
    time.advance_by(Duration::from_secs_f32(CAPTURE_CLOCK_PHASE_SECONDS));
    time.pause();
}

#[cfg(test)]
mod capture_lighting_tests {
    use super::*;

    #[test]
    fn forced_tree_lod_views_map_exactly_and_fail_closed() {
        let expected = [
            ("tree-twig-lod", 1),
            ("tree-small-branch-lod", 2),
            ("tree-crown-lod", 3),
            ("tree-billboard-lod", 4),
        ];
        for (slug, lod) in expected {
            let spec = CAPTURE_VIEWS.iter().find(|spec| spec.slug == slug).unwrap();
            assert_eq!(spec.validated_forced_lod, Some(lod));
        }
        assert_eq!(CAPTURE_VIEWS[1].validated_forced_lod, None);
    }

    #[test]
    fn forced_tree_lod_comparison_views_share_the_lod0_projection() {
        let canonical = CAPTURE_VIEWS
            .iter()
            .find(|spec| spec.slug == "tree-textured-leaf-lod")
            .unwrap();
        assert_eq!(canonical.pose, CapturePose::TreeReview);
        for slug in [
            "tree-leaf-card-lod",
            "tree-twig-lod",
            "tree-small-branch-lod",
            "tree-crown-lod",
            "tree-billboard-lod",
        ] {
            let spec = CAPTURE_VIEWS.iter().find(|spec| spec.slug == slug).unwrap();
            // `camera_for_view` is a pure function of pose and capture state,
            // so equal poses guarantee identical translation, target, and up.
            assert_eq!(spec.pose, canonical.pose, "{slug} camera pose drifted");
            assert_eq!(
                spec.fov_degrees, canonical.fov_degrees,
                "{slug} projection FOV drifted"
            );
        }
    }

    #[test]
    fn semantic_profile_records_lod_transition_controls() {
        let views = selected_capture_views("semantic", &[]).unwrap();
        assert_eq!(views.len(), 32);
        assert_eq!(
            views.iter().filter(|view| view.slug != "warmup").count(),
            31
        );
        assert!(
            views
                .iter()
                .any(|view| view.slug == "tree-crown-transition-fixed")
        );
        assert!(
            views
                .iter()
                .any(|view| view.slug == "tree-billboard-transition-fixed")
        );
        for phase in ["25", "50", "75"] {
            assert!(
                views
                    .iter()
                    .any(|view| { view.slug == format!("tree-billboard-transition-{phase}") })
            );
        }
        let transition_scales = ["25", "50", "75"].map(|phase| {
            views
                .iter()
                .find(|view| view.slug == format!("tree-billboard-transition-{phase}"))
                .and_then(|view| view.projected_scale)
                .unwrap()
        });
        assert!(transition_scales.windows(2).all(|pair| pair[0] > pair[1]));
        assert_eq!(transition_scales, TREE_BILLBOARD_TRANSITION_SCALES);
        let focal_ratio = (80.0_f32.to_radians() * 0.5).tan() / (20.0_f32.to_radians() * 0.5).tan();
        let effective_scales = transition_scales.map(|scale| scale * focal_ratio);
        // At the fixed ~143 m camera, these span the 84..96 m equivalent
        // projected transition instead of collapsing onto one endpoint.
        assert!((1.69..1.71).contains(&effective_scales[0]));
        assert!((1.63..1.65).contains(&effective_scales[1]));
        assert!((1.57..1.59).contains(&effective_scales[2]));
    }

    #[test]
    fn semantic_profile_isolates_each_supported_understory_species() {
        let views = selected_capture_views("semantic", &[]).unwrap();
        let expected = [
            ("understory-common-hazel", "common hazel"),
            ("understory-blackthorn", "blackthorn"),
            ("understory-common-hawthorn", "common hawthorn"),
        ];
        for (slug, common_name) in expected {
            let view = views.iter().find(|view| view.slug == slug).unwrap();
            assert_eq!(view.pose, CapturePose::UnderstoryReview);
            assert_eq!(view.understory_species, Some(common_name));
            assert_eq!(view.detail_requirement, DetailRequirement::UnderstoryFocus);
            assert!(view.hide_obstacles);
            assert!(view.show_tree_backdrop);
        }
    }

    #[test]
    fn requested_views_are_ordered_and_fail_closed() {
        let requested = vec!["rock-detail".into(), "grass-seam-detail".into()];
        let views = selected_capture_views("environment-review", &requested).unwrap();
        assert_eq!(
            views.iter().map(|view| view.slug).collect::<Vec<_>>(),
            vec!["warmup", "rock-detail", "grass-seam-detail"]
        );
        assert!(selected_capture_views("environment-review", &["not-a-view".into()]).is_err());
        assert!(
            selected_capture_views("environment-review", &["tree-lighting-ao".into()]).is_err()
        );
        assert!(selected_capture_views("semantic", &["rock-detail".into()]).is_err());
    }

    #[test]
    fn capture_profiles_have_one_implicit_leading_warmup() {
        for profile in [
            "semantic",
            "environment-review",
            "animation-play",
            "tree-cold-traversal",
        ] {
            let views = selected_capture_views(profile, &[]).unwrap();
            assert_eq!(views.first().map(|view| view.slug), Some("warmup"));
            assert_eq!(views.iter().filter(|view| view.slug == "warmup").count(), 1);
            assert!(selected_capture_views(profile, &["warmup".into()]).is_err());
        }
    }

    #[test]
    fn animation_play_profile_covers_a_natural_unforced_camera_orbit() {
        let views = selected_capture_views("animation-play", &[]).unwrap();
        let plates = &views[1..];

        assert_eq!(plates.len(), 22);
        assert_eq!(
            plates.iter().map(|view| view.slug).collect::<Vec<_>>(),
            vec![
                "animation-play-000",
                "animation-play-045",
                "animation-play-090",
                "animation-play-135",
                "animation-play-180",
                "animation-play-225",
                "animation-play-270",
                "animation-play-315",
                "animation-play-boundary-n",
                "animation-play-boundary-ne",
                "animation-play-boundary-e",
                "animation-play-boundary-se",
                "animation-play-boundary-s",
                "animation-play-boundary-sw",
                "animation-play-boundary-w",
                "animation-play-boundary-nw",
                "tree-family-se-playable-only",
                "tree-family-se-vista-only",
                "animation-play-obstruction-000",
                "animation-play-obstruction-090",
                "animation-play-obstruction-180",
                "animation-play-obstruction-270",
            ]
        );
        for view in plates {
            assert_eq!(view.fov_degrees, 80.0);
            if view.slug == "tree-family-se-playable-only" {
                assert!(!view.vista_visible);
            } else {
                assert!(view.vista_visible);
            }
            assert_eq!(view.render_lod_override, None);
            assert_eq!(view.validated_forced_lod, None);
            assert_eq!(view.leaf_lod_override, None);
            assert!(!view.suppress_leaves);
            assert_eq!(
                view.hide_obstacles,
                view.slug == "tree-family-se-vista-only"
            );
        }
    }

    #[test]
    fn cold_tree_profile_repeats_the_same_inward_distances_after_retreat() {
        let views = selected_capture_views("tree-cold-traversal", &[]).unwrap();
        let first = views
            .iter()
            .filter_map(|view| {
                view.slug
                    .strip_prefix("tree-cold-first-")
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        let second = views
            .iter()
            .filter_map(|view| {
                view.slug
                    .strip_prefix("tree-warm-second-")
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        assert_eq!(first, second);
        assert_eq!(first.len(), 16);
        assert!(views.iter().all(|view| view.render_lod_override.is_none()));
    }

    #[test]
    fn root_detail_suppresses_only_occluding_floor_vegetation() {
        let views = selected_capture_views("environment-review", &[]).unwrap();
        let root = views
            .iter()
            .find(|view| view.slug == "tree-root-detail")
            .unwrap();
        assert!(root.suppress_grass);
        assert!(root.suppress_understory);
        assert!(!root.suppress_leaves);
        assert!(!root.hide_obstacles);
    }

    #[test]
    fn environment_profile_has_deterministic_grazing_debris_target() {
        let views = selected_capture_views("environment-review", &[]).unwrap();
        assert_eq!(views.len(), 12);
        assert!(
            views
                .iter()
                .any(|view| view.slug == "forest-floor-debris-detail")
        );
        let target = Vec3::new(4.0, 1.25, -2.0);
        let (camera, observed_target, up) = debris_detail_camera(target, None, 37.0);
        assert_eq!(observed_target, target);
        assert_eq!(up, Vec3::Y);
        assert!((camera.y - target.y - 1.35).abs() < 0.00001);
        assert!((camera.xz().distance(target.xz()) - 1.15).abs() < 0.00001);
        assert_eq!(
            views
                .iter()
                .find(|view| view.slug == "forest-floor-debris-detail")
                .unwrap()
                .fov_degrees,
            44.0
        );

        let leaves = [Vec3::new(4.0, 0.0, 3.0), Vec3::new(0.1, 0.0, 0.0)];
        let twigs = [Vec3::new(4.2, 0.0, 3.0), Vec3::new(0.4, 0.0, 0.0)];
        let pair = debris_capture_target(&leaves, &twigs, 0.55).unwrap();
        assert_eq!(pair.focus, Vec3::new(4.1, 0.0, 3.0));
        assert!((pair.leaf_distance_metres - 0.1).abs() < 0.00001);
        assert!((pair.twig_distance_metres - 0.1).abs() < 0.00001);
        assert_eq!(debris_capture_target(&[leaves[1]], &[twigs[0]], 0.55), None);

        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![1.0, 1.0, 1.0, 1.0]).unwrap();
        let snapped = terrain_snapped_debris_capture_target(
            &[Vec3::new(-0.2, 0.0, -0.1)],
            &[Vec3::new(0.1, 0.0, -0.1)],
            0.55,
            &terrain,
        )
        .unwrap();
        let ground = terrain.height_at(snapped.focus.xz()).unwrap();
        assert!(snapped.focus.y >= ground);
    }

    #[test]
    fn named_moonlit_minute_has_risen_illuminated_moon_and_dark_sky() {
        let sky = capture_celestial(359_940, 53_500_000, 10_000_000);
        assert!(sky.sun_altitude_degrees < -12.0);
        assert!(sky.moon_altitude_degrees > 20.0);
        assert!(sky.lunar_illumination > 0.9);
    }

    #[test]
    fn diagnostic_views_suppress_only_the_occluding_layer() {
        let find = |slug| {
            ENVIRONMENT_REVIEW_VIEWS
                .iter()
                .find(|view| view.slug == slug)
                .unwrap()
        };
        assert_eq!(
            (
                find("tree-branch-junction").suppress_leaves,
                find("tree-branch-junction").suppress_grass
            ),
            (true, false)
        );
        assert_eq!(
            (
                find("terrain-grazing-detail").suppress_leaves,
                find("terrain-grazing-detail").suppress_grass
            ),
            (false, true)
        );
        assert_eq!(
            (
                find("grass-seam-detail").suppress_leaves,
                find("grass-seam-detail").suppress_grass
            ),
            (false, false)
        );
    }

    #[test]
    fn requested_lighting_documents_every_production_default() {
        let requested = requested_feature_state();
        assert!(requested.shadows);
        assert!(requested.atmosphere);
        assert!(requested.celestial);
        assert!(requested.environment_light);
        assert_eq!(requested.environment_map_size, 64);
        assert!(requested.bloom);
        assert_eq!(requested.max_vista_lods, 3);
    }

    #[test]
    fn lighting_readiness_requires_two_stable_finite_readbacks() {
        assert!(!lighting_samples_stable(&[]));
        assert!(!lighting_samples_stable(&[42.0]));
        assert!(!lighting_samples_stable(&[42.0, f32::NAN]));
        assert!(lighting_samples_stable(&[100.0, 101.0]));
        assert!(!lighting_samples_stable(&[100.0, 110.0]));
    }

    #[test]
    fn overhead_plate_uses_its_detail_sentinel_for_uniform_terrain() {
        assert_eq!(
            CAPTURE_VIEWS
                .iter()
                .find(|view| view.slug == "beauty-overhead")
                .unwrap()
                .minimum_foreground_bps,
            1
        );
        assert_eq!(
            CAPTURE_VIEWS
                .iter()
                .find(|view| view.slug == "beauty-ground")
                .unwrap()
                .minimum_foreground_bps,
            1_000
        );
    }

    #[test]
    fn vista_metrics_report_both_extremes_and_total_relief() {
        let input = TacticalSceneInput::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/tactical-scenes/valley-distant-ridge.json"),
        )
        .unwrap();
        let (_, minimum, maximum, valley, peak) = vista_metrics(&input);
        assert!(minimum < maximum);
        assert_eq!(valley.y, minimum);
        assert_eq!(peak.y, maximum);
        assert!(maximum - minimum > 100.0);
    }

    #[test]
    fn qhd60_benchmark_contract_matches_the_base_target() {
        let target = benchmark_target_hardware();
        assert_eq!(
            [PERFORMANCE_VIEW_WIDTH, PERFORMANCE_VIEW_HEIGHT],
            [2560, 1440]
        );
        assert_eq!(PERFORMANCE_TARGET_FPS, 60.0);
        assert!((PERFORMANCE_FRAME_BUDGET_MS - 16.666_666).abs() < 0.000_01);
        assert_eq!(target.chip, "Apple M5");
        assert_eq!(target.gpu, "8-core GPU");
        assert_eq!(target.unified_memory_gib, 16);
        assert_eq!(target.memory_bandwidth_gib_per_second, 153);
        assert_eq!(target.cooling, "fanless");
    }

    #[test]
    fn mac_system_profiler_fields_tolerate_indentation() {
        let profile = "Hardware:\n    Chip: Apple M5\n    Memory: 16 GB\nGraphics:\n        Chipset Model: Apple M5\n";
        assert_eq!(profiler_value(profile, "Chip").as_deref(), Some("Apple M5"));
        assert_eq!(profiler_value(profile, "Memory").as_deref(), Some("16 GB"));
        assert_eq!(
            profiler_value(profile, "Chipset Model").as_deref(),
            Some("Apple M5")
        );
        assert_eq!(profiler_value(profile, "Missing"), None);
    }

    #[test]
    fn target_acceptance_rejects_non_target_hosts() {
        let target = BenchmarkHost {
            os: "macos",
            architecture: "aarch64",
            chip_or_cpu: Some("Apple M5".into()),
            gpu: Some("Apple M5".into()),
            memory: Some("16 GB".into()),
            release_build: true,
        };
        assert!(benchmark_host_matches_target(&target));
        assert!(!benchmark_host_matches_target(&BenchmarkHost {
            os: "windows",
            ..target
        }));
    }
}

fn absolute_from_current(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .expect("capture needs a working directory")
            .join(path)
    }
}

fn default_output(root: &Path, fixture: &str, digest: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock precedes Unix epoch")
        .as_millis();
    root.join("target/tactical-scene-captures")
        .join(fixture)
        .join(format!("{timestamp}-{}", &digest[..12]))
}

fn prepare_fresh_output(output: &Path) {
    if output.exists() {
        panic!(
            "capture output already exists; choose a fresh directory: {}",
            output.display()
        );
    }
    fs::create_dir_all(output)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output.display()));
}

fn setup_scene(
    mut commands: Commands,
    mut setup: ResMut<SceneSetup>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bark_materials: ResMut<Assets<TacticalTreeBarkMaterial>>,
    mut leaf_card_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    procedural_assets: Res<ProceduralEnvironmentAssets>,
) {
    let setup = setup.0.take().expect("scene setup runs exactly once");
    let SceneSetupData {
        input,
        input_path,
        fixture,
        output,
        generated,
        environment,
        settle_frames,
        leaf_benchmark_frames,
        tree_lighting_benchmark_frames,
        scene_performance_benchmark_frames,
        tree_review_azimuth_degrees,
        profile,
        requested_views,
        views,
    } = setup;
    let GeneratedTacticalScene {
        digest,
        terrain,
        ground,
        obstacles,
        repairs,
    } = generated;
    let terrain_summary = TerrainSummary {
        width_metres: terrain.width(),
        depth_metres: terrain.depth(),
        source_spacing_metres: input.playable.spacing_metres,
        spacing_metres: terrain.grid_scale(),
        source_samples: input.playable.heights_metres.len(),
        generated_samples: terrain.grid_width() * terrain.grid_depth(),
        minimum_height_metres: terrain.minimum_height(),
        maximum_height_metres: terrain.maximum_height(),
    };
    let (
        vista_diameter_metres,
        vista_minimum_metres,
        vista_peak_metres,
        valley_target,
        peak_target,
    ) = vista_metrics(&input);
    let vista_relief_metres = vista_peak_metres - vista_minimum_metres;
    let mut expected_trees = 0;
    let mut expected_rocks = 0;
    let mut obstacle_position_sum = Vec3::ZERO;
    let mut obstacle_count = 0usize;
    let mut tree_focus = None;
    let mut rock_focus = None;
    let mut tree_focus_entity = None;

    for obstacle in obstacles {
        let (grid_x, grid_z, kind, collider, y_offset, overlay_shape, overlay_color) =
            match obstacle {
                GeneratedObstacle::Tree { x, z } => {
                    expected_trees += 1;
                    (
                        x,
                        z,
                        SceneObstacle::Tree,
                        Collider::cylinder(TREE_TRUNK_RADIUS_METRES, TREE_TRUNK_HEIGHT_METRES),
                        TREE_TRUNK_HEIGHT_METRES * 0.5,
                        meshes.add(Cylinder::new(
                            TREE_TRUNK_RADIUS_METRES * 1.08,
                            TREE_TRUNK_HEIGHT_METRES * 1.02,
                        )),
                        Color::srgb(0.1, 1.0, 0.85),
                    )
                }
                GeneratedObstacle::Rock { x, z, recipe } => {
                    expected_rocks += 1;
                    (
                        x,
                        z,
                        SceneObstacle::Rock(recipe),
                        Collider::sphere(recipe.collision_radius_metres()),
                        recipe.collision_radius_metres(),
                        meshes.add(Sphere::new(recipe.collision_radius_metres() * 1.08)),
                        Color::srgb(1.0, 0.08, 0.72),
                    )
                }
            };
        let x = f32::from(grid_x) * input.playable.spacing_metres - terrain.width() * 0.5;
        let z = f32::from(grid_z) * input.playable.spacing_metres - terrain.depth() * 0.5;
        let y = terrain.height_at(Vec2::new(x, z)).unwrap_or_default() + y_offset;
        let focuses_tree = matches!(kind, SceneObstacle::Tree)
            && tree_focus.is_none_or(|focus: Vec3| {
                Vec2::new(x, z).length_squared() < focus.xz().length_squared()
            });
        if focuses_tree {
            tree_focus = Some(Vec3::new(x, y, z));
        }
        let focuses_rock = matches!(kind, SceneObstacle::Rock(_))
            && rock_focus.is_none_or(|focus: Vec3| {
                Vec2::new(x, z).length_squared() < focus.xz().length_squared()
            });
        if focuses_rock {
            rock_focus = Some(Vec3::new(x, y, z));
        }
        obstacle_position_sum += Vec3::new(x, y, z);
        obstacle_count += 1;
        let yaw = match kind {
            SceneObstacle::Rock(recipe) => {
                (recipe.seed >> 40) as f32 / ((1_u32 << 24) - 1) as f32 * core::f32::consts::TAU
            }
            SceneObstacle::Tree => 0.0,
        };
        let transform = Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(yaw));
        let obstacle_entity = commands
            .spawn((
                Name::new("Captured tactical obstacle"),
                kind,
                RigidBody::Static,
                CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
                collider,
                transform,
            ))
            .id();
        if focuses_tree {
            tree_focus_entity = Some(obstacle_entity);
        }
        commands.spawn((
            Name::new("Capture collider overlay"),
            CaptureOverlay,
            Mesh3d(overlay_shape),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: overlay_color,
                emissive: LinearRgba::new(6.0, 6.0, 6.0, 1.0),
                unlit: true,
                ..default()
            })),
            Visibility::Hidden,
            transform,
        ));
    }

    let terrain_collider = terrain.collider();
    let mut obstacle_focus = if obstacle_count == 0 {
        Vec3::ZERO
    } else {
        obstacle_position_sum / obstacle_count as f32
    };
    let focus_limit = terrain.width().min(terrain.depth()) * 0.25;
    obstacle_focus.x = obstacle_focus.x.clamp(-focus_limit, focus_limit);
    obstacle_focus.z = obstacle_focus.z.clamp(-focus_limit, focus_limit);
    obstacle_focus.y = terrain
        .height_at(Vec2::new(obstacle_focus.x, obstacle_focus.z))
        .unwrap_or_default()
        + 1.5;
    let half = terrain.width().max(terrain.depth()) * 0.5;
    let camera_margin = terrain.grid_scale().max(0.5);
    let camera_x = (obstacle_focus.x - half * 0.62).clamp(
        -terrain.width() * 0.5 + camera_margin,
        terrain.width() * 0.5 - camera_margin,
    );
    let camera_z = (obstacle_focus.z + half * 0.62).clamp(
        -terrain.depth() * 0.5 + camera_margin,
        terrain.depth() * 0.5 - camera_margin,
    );
    let camera_ground = terrain
        .height_at(Vec2::new(camera_x, camera_z))
        .unwrap_or_default();
    let focus_ground = terrain
        .height_at(Vec2::new(obstacle_focus.x, obstacle_focus.z))
        .unwrap_or_default();
    let ground_eye_position = Vec3::new(
        camera_x,
        camera_ground + STANDING_EYE_HEIGHT_METRES,
        camera_z,
    );
    let ground_eye_target = Vec3::new(
        obstacle_focus.x,
        focus_ground + STANDING_EYE_HEIGHT_METRES,
        obstacle_focus.z,
    );
    // The animation profile spawns the player at the deterministic scene
    // origin. Controller translation is capsule-centred, then the lowered
    // production camera adds its 0.48 m focus height.
    let animation_play_focus = Vec3::new(
        0.0,
        terrain.height_at(Vec2::ZERO).unwrap_or_default() + 1.48,
        0.0,
    );
    let canopy_bps = environment.canopy_bps;
    let absolute_minute = environment.absolute_minute;
    let latitude_microdegrees = environment.latitude_microdegrees;
    let longitude_microdegrees = environment.longitude_microdegrees;
    let expects_grass = ground.cover_count(GroundCover::TallGrass) > 0;
    let debris_focus = ground
        .samples()
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.cover == GroundCover::LeafLitter)
        .map(|(index, _)| {
            let x = index % ground.grid_width();
            let z = index / ground.grid_width();
            Vec2::new(
                x as f32 * ground.grid_scale() - ground.width() * 0.5,
                z as f32 * ground.grid_scale() - ground.depth() * 0.5,
            )
        })
        .min_by(|left, right| left.length_squared().total_cmp(&right.length_squared()))
        .map(|point| {
            Vec3::new(
                point.x,
                terrain.height_at(point).unwrap_or_default() + 0.018,
                point.y,
            )
        });
    let mut tree_leaf_focus = None;
    let mut tree_leaf_camera = None;
    let mut tree_review_entities = Vec::new();
    let mut tree_review_leaf_entities = Vec::new();
    if let Some(tree) = tree_focus {
        commands.spawn((
            Name::new("Neutral tree review backdrop"),
            TreeReviewBackdrop,
            Mesh3d(meshes.add(Cuboid::new(40.0, 26.0, 0.1))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.78, 0.82, 0.84),
                unlit: true,
                ..default()
            })),
            Visibility::Hidden,
            Transform {
                translation: tree + Vec3::new(-8.0, 3.0, -8.0),
                rotation: Quat::from_rotation_y(core::f32::consts::FRAC_PI_4),
                ..default()
            },
        ));
        let specimen_origin = tree + Vec3::Y * 15.0;
        let (
            branch_mesh,
            textured_leaf_mesh,
            leaf_card_mesh,
            bud_mesh,
            local_focus,
            camera_direction,
        ) = oak_review_terminal_specimen(tree, canopy_bps);
        let bark_material = bark_materials.add(oak_bark_material(
            &procedural_assets,
            images.add(terrain_heightmap_image(&terrain)),
            Vec2::new(terrain.minimum_height(), terrain.maximum_height()),
            &terrain,
        ));
        let leaf_material = leaf_card_materials.add(oak_leaf_material(&procedural_assets));
        let bud_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.27, 0.1),
            perceptual_roughness: 0.92,
            ..default()
        });
        tree_review_entities.push(
            commands
                .spawn((
                    Name::new("Isolated production oak terminal shoot"),
                    TreeReviewSpecimen,
                    Mesh3d(meshes.add(branch_mesh)),
                    MeshMaterial3d(bark_material),
                    Visibility::Hidden,
                    Transform::from_translation(specimen_origin),
                ))
                .id(),
        );
        tree_review_entities.push(
            commands
                .spawn((
                    Name::new("Isolated production oak terminal bud"),
                    TreeReviewSpecimen,
                    Mesh3d(meshes.add(bud_mesh)),
                    MeshMaterial3d(bud_material),
                    Visibility::Hidden,
                    Transform::from_translation(specimen_origin),
                ))
                .id(),
        );
        let textured_entity = commands
            .spawn((
                Name::new("Isolated cambered textured oak terminal leaves"),
                TreeReviewSpecimen,
                TreeLeafRepresentation::TexturedMesh,
                Mesh3d(meshes.add(textured_leaf_mesh)),
                MeshMaterial3d(leaf_material.clone()),
                Visibility::Hidden,
                Transform::from_translation(specimen_origin),
            ))
            .id();
        tree_review_leaf_entities.push((textured_entity, TreeLeafRepresentation::TexturedMesh));
        let card_entity = commands
            .spawn((
                Name::new("Isolated flat-card oak terminal leaves"),
                TreeReviewSpecimen,
                TreeLeafRepresentation::AlphaCard,
                Mesh3d(meshes.add(leaf_card_mesh)),
                MeshMaterial3d(leaf_material),
                Visibility::Hidden,
                Transform::from_translation(specimen_origin),
            ))
            .id();
        tree_review_leaf_entities.push((card_entity, TreeLeafRepresentation::AlphaCard));
        tree_leaf_focus = Some(specimen_origin + local_focus);
        let review_rotation = Quat::from_rotation_y(tree_review_azimuth_degrees.to_radians());
        tree_leaf_camera =
            Some(specimen_origin + local_focus + review_rotation * camera_direction * 0.68);
        tree_review_entities.push(
            commands
                .spawn((
                    Name::new("Ten centimetre tree review scale bar"),
                    TreeReviewSpecimen,
                    Mesh3d(meshes.add(Cuboid::new(0.1, 0.012, 0.012))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: Color::srgb(0.12, 0.12, 0.12),
                        unlit: true,
                        ..default()
                    })),
                    Visibility::Hidden,
                    Transform::from_translation(
                        specimen_origin + local_focus + Vec3::new(-0.12, -0.13, 0.0),
                    ),
                ))
                .id(),
        );
    }
    commands.spawn((
        Name::new("Captured tactical terrain"),
        SceneId(input.scene_key.clone()),
        environment,
        ground,
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        terrain_collider,
        terrain,
        Transform::default(),
    ));
    commands.trigger(SceneVistaBundle {
        scene_digest: digest.clone(),
        playable_half_extent_metres: Vec2::new(
            terrain_summary.width_metres * 0.5,
            terrain_summary.depth_metres * 0.5,
        ),
        lods: input.vista.lods.clone(),
    });
    commands.insert_resource(CaptureState {
        fixture,
        input_path,
        output,
        digest,
        seed: input.seed,
        absolute_minute,
        latitude_microdegrees,
        longitude_microdegrees,
        canopy_bps,
        generation_version: input.generation_version,
        scene_source: input.source,
        weather: input.weather,
        repairs: RepairSummary {
            upsampled_height_samples: repairs.upsampled_height_samples,
            microrelief_adjusted_samples: repairs.microrelief_adjusted_samples,
            adjusted_height_samples: repairs.adjusted_height_samples,
            repaired_water_samples: repairs.repaired_water_samples,
            removed_corridor_obstacles: repairs.removed_corridor_obstacles,
        },
        terrain: terrain_summary,
        expected_trees,
        expected_rocks,
        expects_grass,
        vista_lods_supplied: input.vista.lods.len(),
        vista_diameter_metres,
        vista_minimum_metres,
        vista_peak_metres,
        vista_relief_metres,
        peak_target,
        valley_target,
        obstacle_focus,
        tree_focus,
        rock_focus,
        debris_focus,
        debris_camera: None,
        debris_leaf_distance_metres: None,
        debris_twig_distance_metres: None,
        tree_leaf_focus,
        tree_leaf_camera,
        tree_focus_entity,
        tree_review_entities,
        tree_review_leaf_entities,
        ground_eye_position,
        ground_eye_target,
        animation_play_focus,
        settle_frames,
        tree_review_azimuth_degrees,
        profile,
        requested_views,
        views,
        view: 0,
        phase: CapturePhase::Configure,
        lighting_luminance_samples: Vec::new(),
        captures: Vec::new(),
        recursive_lods_observed: BTreeSet::new(),
        recursive_aggregate_lods_observed: BTreeSet::new(),
    });
    if let Some(sample_frames) = leaf_benchmark_frames {
        commands.insert_resource(LeafBenchmarkState::new(sample_frames));
    }
    if let Some(sample_frames) = tree_lighting_benchmark_frames {
        commands.insert_resource(TreeLightingBenchmarkState::new(sample_frames));
    }
    if let Some(sample_frames) = scene_performance_benchmark_frames {
        commands.insert_resource(ScenePerformanceBenchmarkState::new(sample_frames));
    }
}

fn vista_metrics(input: &TacticalSceneInput) -> (f32, f32, f32, Vec3, Vec3) {
    let mut diameter = 0.0_f32;
    let mut minimum = f32::INFINITY;
    let mut peak = f32::NEG_INFINITY;
    let mut valley_target = Vec3::ZERO;
    let mut target = Vec3::ZERO;
    for lod in &input.vista.lods {
        diameter = diameter.max(f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres);
        let center_x = f32::from(lod.width.saturating_sub(1)) * 0.5;
        let center_z = f32::from(lod.depth.saturating_sub(1)) * 0.5;
        for (index, height) in lod.heights_metres.iter().copied().enumerate() {
            let x = (index % usize::from(lod.width)) as f32;
            let z = (index / usize::from(lod.width)) as f32;
            let position = Vec3::new(
                (x - center_x) * lod.spacing_metres + lod.origin_east_metres as f32,
                height,
                (z - center_z) * lod.spacing_metres + lod.origin_north_metres as f32,
            );
            if height < minimum {
                minimum = height;
                valley_target = position;
            }
            if height > peak {
                peak = height;
                target = position;
            }
        }
    }
    if !minimum.is_finite() {
        minimum = 0.0;
    }
    if !peak.is_finite() {
        peak = 0.0;
    }
    (diameter, minimum, peak, valley_target, target)
}

fn benchmark_leaf_representations(
    mut state: Option<ResMut<LeafBenchmarkState>>,
    capture: Option<Res<CaptureState>>,
    time: Res<Time<Real>>,
    mut tree_lod_override: ResMut<TreeLodRenderOverride>,
    mut camera: Single<(&mut Transform, &mut GlobalTransform, &mut Projection), With<Camera3d>>,
    leaf_meshes: Query<(&TreeLeafTriangleCount, &TreeLeafRepresentation)>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(state), Some(capture)) = (state.as_deref_mut(), capture.as_deref()) else {
        return;
    };
    let Some(&(representation, name, triangles_per_leaf)) = LEAF_BENCHMARK_MODES.get(state.mode)
    else {
        return;
    };

    tree_lod_override.lod = Some(0);
    tree_lod_override.leaf = Some(representation);
    tree_lod_override.projected_scale = None;
    let transform = Transform::from_translation(capture.ground_eye_position)
        .looking_at(capture.ground_eye_target, Vec3::Y);
    *camera.0 = transform;
    *camera.1 = GlobalTransform::from(transform);
    if let Projection::Perspective(projection) = &mut *camera.2 {
        projection.fov = 80.0_f32.to_radians();
    }

    if state.warmup_remaining > 0 {
        state.warmup_remaining -= 1;
        return;
    }
    state.samples_ms.push(time.delta_secs_f64() * 1_000.0);
    if state.samples_ms.len() < state.sample_frames as usize {
        return;
    }

    state.samples_ms.sort_by(f64::total_cmp);
    let mean_ms = state.samples_ms.iter().sum::<f64>() / state.samples_ms.len() as f64;
    let median_ms = percentile(&state.samples_ms, 0.50);
    let p95_ms = percentile(&state.samples_ms, 0.95);
    let scene_leaf_triangles = leaf_meshes
        .iter()
        .filter(|(_, active)| **active == representation)
        .map(|(triangles, _)| triangles.0)
        .sum();
    state.results.push(LeafBenchmarkResult {
        representation: name,
        triangles_per_leaf,
        scene_leaf_triangles,
        mean_ms,
        median_ms,
        p95_ms,
        mean_fps: 1_000.0 / mean_ms,
    });
    println!(
        "LEAF_BENCHMARK representation={name:?} mean_ms={mean_ms:.3} median_ms={median_ms:.3} p95_ms={p95_ms:.3}"
    );
    state.mode += 1;
    state.samples_ms.clear();
    state.warmup_remaining = LEAF_BENCHMARK_WARMUP_FRAMES;
    if state.mode < LEAF_BENCHMARK_MODES.len() {
        return;
    }

    let report = LeafBenchmarkReport {
        pipeline: "tactical_scene_leaf_benchmark_v1",
        fixture: capture.fixture.clone(),
        resolution: [VIEW_WIDTH, VIEW_HEIGHT],
        tree_count: capture.expected_trees,
        warmup_frames_per_mode: LEAF_BENCHMARK_WARMUP_FRAMES,
        sample_frames_per_mode: state.sample_frames,
        note: "End-to-end uncapped frame duration; not an isolated GPU timestamp. All leaf modes use identical scene, camera, lighting, wind, and LOD0 wood.",
        results: core::mem::take(&mut state.results),
    };
    write_leaf_benchmark(&capture.output, &report);
    exit.write(AppExit::Success);
}

fn apply_tree_lighting_mode(
    mode: TreeLightingMode,
    materials: &mut Assets<TacticalTreeLeafCardMaterial>,
) {
    for (_, material) in materials.iter_mut() {
        material.surface_parameters.z = mode.ambient_occlusion_strength;
    }
}

fn benchmark_tree_lighting(
    mut state: Option<ResMut<TreeLightingBenchmarkState>>,
    capture: Option<Res<CaptureState>>,
    time: Res<Time<Real>>,
    mut commands: Commands,
    mut leaf_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    leaf_entities: Query<Entity, With<MeshMaterial3d<TacticalTreeLeafCardMaterial>>>,
    mut tree_lod_override: ResMut<TreeLodRenderOverride>,
    mut camera: Single<(&mut Transform, &mut GlobalTransform, &mut Projection), With<Camera3d>>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(state), Some(capture)) = (state.as_deref_mut(), capture.as_deref()) else {
        return;
    };
    let Some(&mode) = TREE_LIGHTING_MODES.get(state.mode) else {
        return;
    };

    if state.configured_mode != Some(state.mode) {
        apply_tree_lighting_mode(mode, &mut leaf_materials);
        for entity in &leaf_entities {
            if mode.shadows_enabled {
                commands.entity(entity).remove::<NotShadowCaster>();
            } else {
                commands.entity(entity).insert(NotShadowCaster);
            }
        }
        state.configured_mode = Some(state.mode);
    }
    tree_lod_override.lod = Some(0);
    tree_lod_override.leaf = Some(TreeLeafRepresentation::TexturedMesh);
    tree_lod_override.projected_scale = None;
    let transform = Transform::from_translation(capture.ground_eye_position)
        .looking_at(capture.ground_eye_target, Vec3::Y);
    *camera.0 = transform;
    *camera.1 = GlobalTransform::from(transform);
    if let Projection::Perspective(projection) = &mut *camera.2 {
        projection.fov = 80.0_f32.to_radians();
    }

    if state.warmup_remaining > 0 {
        state.warmup_remaining -= 1;
        return;
    }
    state.samples_ms.push(time.delta_secs_f64() * 1_000.0);
    if state.samples_ms.len() < state.sample_frames as usize {
        return;
    }

    state.samples_ms.sort_by(f64::total_cmp);
    let mean_ms = state.samples_ms.iter().sum::<f64>() / state.samples_ms.len() as f64;
    let median_ms = percentile(&state.samples_ms, 0.50);
    let p95_ms = percentile(&state.samples_ms, 0.95);
    state.results.push(TreeLightingBenchmarkResult {
        mode: mode.name,
        ambient_occlusion: mode.ambient_occlusion_strength > 0.0,
        self_shadows: mode.shadows_enabled,
        mean_ms,
        median_ms,
        p95_ms,
        mean_fps: 1_000.0 / mean_ms,
    });
    println!(
        "TREE_LIGHTING_BENCHMARK mode={:?} mean_ms={mean_ms:.3} median_ms={median_ms:.3} p95_ms={p95_ms:.3}",
        mode.name
    );
    state.mode += 1;
    state.samples_ms.clear();
    state.warmup_remaining = TREE_LIGHTING_BENCHMARK_WARMUP_FRAMES;
    if state.mode < TREE_LIGHTING_MODES.len() {
        return;
    }

    let report = TreeLightingBenchmarkReport {
        pipeline: "tactical_scene_tree_lighting_benchmark_v1",
        fixture: capture.fixture.clone(),
        resolution: [VIEW_WIDTH, VIEW_HEIGHT],
        tree_count: capture.expected_trees,
        warmup_frames_per_mode: TREE_LIGHTING_BENCHMARK_WARMUP_FRAMES,
        sample_frames_per_mode: state.sample_frames,
        note: "End-to-end uncapped frame duration; not an isolated GPU timestamp. All modes use identical dense-forest geometry, camera, weather, wind, and forced LOD0 cambered leaves.",
        results: core::mem::take(&mut state.results),
    };
    write_tree_lighting_benchmark(&capture.output, &report);
    exit.write(AppExit::Success);
}

fn benchmark_scene_performance(
    mut state: Option<ResMut<ScenePerformanceBenchmarkState>>,
    capture: Option<Res<CaptureState>>,
    time: Res<Time<Real>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    tree_asset_residency: Res<TreeAssetResidencyDiagnostics>,
    mut tree_lod_override: ResMut<TreeLodRenderOverride>,
    mut tree_isolation: ResMut<TacticalTreeBenchmarkIsolation>,
    mut cloud_isolation: ResMut<TacticalCloudBenchmarkIsolation>,
    mut camera: Single<(&mut Transform, &mut GlobalTransform, &mut Projection), With<Camera3d>>,
    mut visibility_layers: ParamSet<(
        Query<
            (Entity, &mut Visibility),
            (
                With<PresentedTree>,
                Without<VistaTreePresentation>,
                Without<MeshMaterial3d<TacticalTreeLeafCardMaterial>>,
                Without<GroundScatterLayer>,
            ),
        >,
        Query<
            &mut Visibility,
            (
                With<VistaTreePresentation>,
                Without<PresentedTree>,
                Without<MeshMaterial3d<TacticalTreeLeafCardMaterial>>,
                Without<GroundScatterLayer>,
            ),
        >,
        Query<(&GroundScatterLayer, &mut Visibility)>,
        Query<&mut Visibility, With<ProceduralRockVisual>>,
        Query<&mut Visibility, Or<(With<TerrainMaterialPresentation>, With<TerrainDetailPatch>)>>,
        Query<&mut Visibility, With<VistaTerrain>>,
        Query<
            (
                &mut Visibility,
                Option<&TacticalCloudLayer>,
                Option<&WeatherParticle>,
            ),
            Or<(With<TacticalCloudLayer>, With<WeatherParticle>)>,
        >,
        Query<
            (
                Has<PlayableTreeDetailedLeaves>,
                Has<PlayableTreeTrunk>,
                Has<PlayableTreeDetailedWood>,
                Has<PlayableTreeAggregateWood>,
                Has<PlayableTreeBuds>,
                Has<PlayableTreeCanopyCard>,
            ),
            (
                Without<TreeReviewSpecimen>,
                Or<(
                    With<PlayableTreeDetailedLeaves>,
                    With<PlayableTreeTrunk>,
                    With<PlayableTreeDetailedWood>,
                    With<PlayableTreeAggregateWood>,
                    With<PlayableTreeBuds>,
                    With<PlayableTreeCanopyCard>,
                )>,
            ),
        >,
    )>,
    playable_leaves: Query<
        Entity,
        (
            With<MeshMaterial3d<TacticalTreeLeafCardMaterial>>,
            Without<TreeReviewSpecimen>,
            Without<PresentedTree>,
            Without<VistaTreePresentation>,
            Without<GroundScatterLayer>,
        ),
    >,
    litter_diagnostics: Query<&GroundLitterDiagnostics>,
    loose_stone_pebble_patches: Query<&LooseStonePebblePatch>,
    visible_tree_lods: Query<(&TreeLod, &ViewVisibility, Option<&TreeLeafRepresentation>)>,
    aggregate_wood_shadow_casters: Query<
        (&TreeLod, Has<NotShadowCaster>),
        With<PlayableTreeAggregateWood>,
    >,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(state), Some(capture)) = (state.as_deref_mut(), capture.as_deref()) else {
        return;
    };
    let Some(&mode) = SCENE_PERFORMANCE_MODES.get(state.mode) else {
        return;
    };

    if state.configured_mode != Some(state.mode) {
        tree_lod_override.lod = mode.forced_lod;
        tree_lod_override.leaf = mode.forced_leaf;
        tree_lod_override.projected_scale = None;
        tree_isolation.hide_detailed_leaves =
            mode.hide_playable_leaves || mode.hidden_scene_layers & HIDE_TREE_LEAVES != 0;
        tree_isolation.hide_canopy_cards = mode.hidden_scene_layers & HIDE_TREE_CANOPY_CARDS != 0;
        tree_isolation.hide_buds = mode.hidden_scene_layers & HIDE_TREE_BUDS != 0;
        tree_isolation.hide_trunks = mode.hidden_scene_layers & HIDE_TREE_TRUNKS != 0;
        tree_isolation.hide_branches = mode.hidden_scene_layers & HIDE_TREE_BRANCHES != 0;
        cloud_isolation.hide_clouds = mode.hidden_scene_layers & HIDE_CLOUDS != 0;
        let transform = Transform::from_translation(capture.ground_eye_position)
            .looking_at(capture.ground_eye_target, Vec3::Y);
        *camera.0 = transform;
        *camera.1 = GlobalTransform::from(transform);
        if let Projection::Perspective(projection) = &mut *camera.2 {
            projection.fov = 80.0_f32.to_radians();
        }
        for (_, mut visibility) in &mut visibility_layers.p0() {
            *visibility = if mode.hide_playable_trees {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        for mut visibility in &mut visibility_layers.p1() {
            *visibility = if mode.hide_vista_trees {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        for (layer, mut visibility) in &mut visibility_layers.p2() {
            let mask = match layer {
                GroundScatterLayer::DryLeaves | GroundScatterLayer::Twigs => HIDE_LITTER,
                GroundScatterLayer::Grass => HIDE_GRASS,
                GroundScatterLayer::Understory => HIDE_UNDERSTORY,
                GroundScatterLayer::LooseStone => HIDE_LOOSE_STONE,
            };
            *visibility = if mode.hidden_scene_layers & mask != 0 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        for mut visibility in &mut visibility_layers.p3() {
            *visibility = if mode.hidden_scene_layers & HIDE_ROCKS != 0 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        for mut visibility in &mut visibility_layers.p4() {
            *visibility = if mode.hidden_scene_layers & HIDE_PLAYABLE_TERRAIN != 0 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        for mut visibility in &mut visibility_layers.p5() {
            *visibility = if mode.hidden_scene_layers & HIDE_VISTA_TERRAIN != 0 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        for (mut visibility, cloud, weather) in &mut visibility_layers.p6() {
            if weather.is_some() {
                *visibility = if mode.hidden_scene_layers & HIDE_WEATHER != 0 {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                };
            }
            debug_assert!(cloud.is_none() || weather.is_none());
        }
        state.configured_mode = Some(state.mode);
    }
    state
        .playable_tree_count
        .get_or_insert_with(|| visibility_layers.p0().iter().count());
    state
        .playable_leaf_entities
        .get_or_insert_with(|| playable_leaves.iter().count());
    state
        .vista_tree_entities
        .get_or_insert_with(|| visibility_layers.p1().iter().count());
    if state.scene_entity_counts.is_none() {
        let mut counts = BTreeMap::new();
        for (layer, _) in visibility_layers.p2().iter() {
            let name = match layer {
                GroundScatterLayer::Grass => "grass_patches",
                GroundScatterLayer::Understory => "understory_patches",
                GroundScatterLayer::DryLeaves => "dry_leaf_patches",
                GroundScatterLayer::Twigs => "twig_patches",
                GroundScatterLayer::LooseStone => "loose_stone_patches",
            };
            *counts.entry(name.to_owned()).or_default() += 1;
        }
        let litter_diagnostics = litter_diagnostics
            .iter()
            .next()
            .copied()
            .unwrap_or_default();
        counts.insert(
            "dry_leaf_patch_instances".to_owned(),
            litter_diagnostics.dry_leaf_patch_instances,
        );
        counts.insert(
            "physical_dry_leaves".to_owned(),
            litter_diagnostics.physical_dry_leaf_count,
        );
        counts.insert(
            "procedural_rocks".to_owned(),
            visibility_layers.p3().iter().count(),
        );
        counts.insert(
            "loose_stone_pebbles".to_owned(),
            loose_stone_pebble_patches
                .iter()
                .map(|patch| patch.physical_pebbles)
                .sum(),
        );
        counts.insert(
            "active_cloud_layers".to_owned(),
            visibility_layers
                .p6()
                .iter()
                .filter(|(_, cloud, _)| cloud.is_some_and(TacticalCloudLayer::is_active))
                .count(),
        );
        counts.insert(
            "weather_particles".to_owned(),
            visibility_layers
                .p6()
                .iter()
                .filter(|(_, _, weather)| weather.is_some())
                .count(),
        );
        counts.insert(
            "tree_detailed_leaf_entities".to_owned(),
            visibility_layers
                .p7()
                .iter()
                .filter(|(leaves, _, _, _, _, _)| *leaves)
                .count(),
        );
        counts.insert(
            "tree_trunk_entities".to_owned(),
            visibility_layers
                .p7()
                .iter()
                .filter(|(_, trunks, _, _, _, _)| *trunks)
                .count(),
        );
        counts.insert(
            "tree_detailed_wood_entities".to_owned(),
            visibility_layers
                .p7()
                .iter()
                .filter(|(_, _, wood, _, _, _)| *wood)
                .count(),
        );
        counts.insert(
            "tree_aggregate_wood_entities".to_owned(),
            visibility_layers
                .p7()
                .iter()
                .filter(|(_, _, _, wood, _, _)| *wood)
                .count(),
        );
        for lod in [1_u8, 2] {
            counts.insert(
                format!("tree_lod{lod}_aggregate_wood_shadow_caster_entities"),
                aggregate_wood_shadow_casters
                    .iter()
                    .filter(|(tree_lod, not_shadow_caster)| {
                        tree_lod.0 == lod && !*not_shadow_caster
                    })
                    .count(),
            );
        }
        counts.insert(
            "tree_bud_entities".to_owned(),
            visibility_layers
                .p7()
                .iter()
                .filter(|(_, _, _, _, buds, _)| *buds)
                .count(),
        );
        counts.insert(
            "tree_canopy_card_entities".to_owned(),
            visibility_layers
                .p7()
                .iter()
                .filter(|(_, _, _, _, _, cards)| *cards)
                .count(),
        );
        state.scene_entity_counts = Some(counts);
    }

    if state.warmup_remaining > 0 {
        state.warmup_remaining -= 1;
        return;
    }

    state.samples_ms.push(time.delta_secs_f64() * 1_000.0);
    for diagnostic in diagnostics.iter() {
        let path = diagnostic.path().as_str();
        if !path.starts_with("render/") {
            continue;
        }
        let Some(value) = diagnostic.value().filter(|value| value.is_finite()) else {
            continue;
        };
        state
            .render_diagnostic_samples
            .entry(path.to_owned())
            .or_default()
            .push(value);
    }
    if state.samples_ms.len() < state.sample_frames as usize {
        return;
    }

    state.samples_ms.sort_by(f64::total_cmp);
    let mean_ms = state.samples_ms.iter().sum::<f64>() / state.samples_ms.len() as f64;
    let median_ms = percentile(&state.samples_ms, 0.50);
    let p95_ms = percentile(&state.samples_ms, 0.95);
    let render_diagnostics = core::mem::take(&mut state.render_diagnostic_samples)
        .into_iter()
        .filter_map(|(path, mut samples)| {
            if samples.len() < state.sample_frames as usize / 2 {
                return None;
            }
            samples.sort_by(f64::total_cmp);
            let mean = samples.iter().sum::<f64>() / samples.len() as f64;
            Some((
                path,
                BenchmarkMetricSummary {
                    mean,
                    median: percentile(&samples, 0.50),
                    p95: percentile(&samples, 0.95),
                    p99: percentile(&samples, 0.99),
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let gpu_elapsed_median_ms =
        summed_render_metric(&render_diagnostics, "elapsed_gpu", |metric| metric.median);
    let gpu_elapsed_p95_ms =
        summed_render_metric(&render_diagnostics, "elapsed_gpu", |metric| metric.p95);
    let p99_ms = percentile(&state.samples_ms, 0.99);
    let worst_ms = *state.samples_ms.last().expect("benchmark has samples");
    let frames_over_budget = state
        .samples_ms
        .iter()
        .filter(|sample| **sample > PERFORMANCE_FRAME_BUDGET_MS)
        .count();
    let frames_over_budget_percent =
        frames_over_budget as f64 * 100.0 / state.samples_ms.len() as f64;
    let limiting_p95_ms = gpu_elapsed_p95_ms.map_or(p95_ms, |gpu_ms| p95_ms.max(gpu_ms));
    let mut visible_tree_entities = BTreeMap::new();
    let active_cloud_layers = visibility_layers
        .p6()
        .iter()
        .filter(|(_, cloud, _)| cloud.is_some_and(TacticalCloudLayer::is_active))
        .count();
    let visible_cloud_layers = visibility_layers
        .p6()
        .iter()
        .filter(|(visibility, cloud, _)| {
            cloud.is_some_and(TacticalCloudLayer::is_active)
                && !matches!(**visibility, Visibility::Hidden)
        })
        .count();
    for (lod, view_visibility, leaf_representation) in &visible_tree_lods {
        if !view_visibility.get() {
            continue;
        }
        let name = match leaf_representation {
            Some(TreeLeafRepresentation::TexturedMesh) => "lod0_textured_leaves".to_owned(),
            Some(TreeLeafRepresentation::AlphaCard) => "lod0_leaf_cards".to_owned(),
            None => format!("lod{}", lod.0),
        };
        *visible_tree_entities.entry(name).or_default() += 1;
    }
    state.results.push(ScenePerformanceBenchmarkResult {
        mode: mode.name,
        mean_ms,
        median_ms,
        p95_ms,
        p99_ms,
        worst_ms,
        mean_fps: 1_000.0 / mean_ms,
        frames_over_budget,
        frames_over_budget_percent,
        wall_60_fps_budget_passes: p95_ms <= PERFORMANCE_FRAME_BUDGET_MS,
        gpu_elapsed_median_ms,
        gpu_elapsed_p95_ms,
        gpu_60_fps_budget_passes: gpu_elapsed_p95_ms
            .map(|elapsed| elapsed <= PERFORMANCE_FRAME_BUDGET_MS),
        limiting_p95_ms,
        headroom_ms: PERFORMANCE_FRAME_BUDGET_MS - limiting_p95_ms,
        budget_utilization_percent: limiting_p95_ms / PERFORMANCE_FRAME_BUDGET_MS * 100.0,
        measured_60_fps_passes: gpu_elapsed_p95_ms
            .map(|gpu_ms| p95_ms.max(gpu_ms) <= PERFORMANCE_FRAME_BUDGET_MS),
        active_cloud_layers,
        visible_cloud_layers,
        visible_tree_entities,
        tree_asset_residency: tree_asset_residency.clone(),
        render_diagnostics,
        gpu_cost_attribution_vs_natural_ms: BTreeMap::new(),
    });
    println!(
        "SCENE_PERFORMANCE_BENCHMARK mode={:?} mean_ms={mean_ms:.3} median_ms={median_ms:.3} p95_ms={p95_ms:.3}",
        mode.name
    );
    state.mode += 1;
    state.configured_mode = None;
    state.samples_ms.clear();
    state.warmup_remaining = SCENE_PERFORMANCE_WARMUP_FRAMES;
    if state.mode <= state.stop_after_mode {
        return;
    }

    let natural_diagnostics = state
        .results
        .first()
        .map(|result| result.render_diagnostics.clone());
    if let Some(natural_diagnostics) = natural_diagnostics.as_ref() {
        for result in &mut state.results {
            if result.mode == "Natural production LODs" {
                continue;
            }
            result.gpu_cost_attribution_vs_natural_ms = result
                .render_diagnostics
                .iter()
                .filter_map(|(path, metric)| {
                    if !path.ends_with("elapsed_gpu") {
                        return None;
                    }
                    let natural = natural_diagnostics.get(path)?;
                    Some((
                        path.clone(),
                        BenchmarkMetricDelta {
                            median_ms: natural.median - metric.median,
                            p95_ms: natural.p95 - metric.p95,
                        },
                    ))
                })
                .collect();
        }
    }

    let render_diagnostics_enabled = state
        .results
        .iter()
        .any(|result| !result.render_diagnostics.is_empty());
    let host = benchmark_host();
    let host_matches_target = benchmark_host_matches_target(&host);
    let target_acceptance_passes =
        (host_matches_target && host.release_build && render_diagnostics_enabled).then(|| {
            state
                .results
                .first()
                .and_then(|result| result.measured_60_fps_passes)
                .unwrap_or(false)
        });
    let report = ScenePerformanceBenchmarkReport {
        pipeline: "tactical_scene_qhd60_performance_benchmark_v2",
        target_fps: PERFORMANCE_TARGET_FPS,
        frame_budget_ms: PERFORMANCE_FRAME_BUDGET_MS,
        target_hardware: benchmark_target_hardware(),
        host,
        host_matches_target,
        target_acceptance_passes,
        fixture: capture.fixture.clone(),
        source_input: capture.input_path.display().to_string(),
        resolution: [PERFORMANCE_VIEW_WIDTH, PERFORMANCE_VIEW_HEIGHT],
        playable_tree_count: state.playable_tree_count.unwrap_or_default(),
        playable_leaf_entities: state.playable_leaf_entities.unwrap_or_default(),
        vista_tree_entities: state.vista_tree_entities.unwrap_or_default(),
        playable_area_square_km: f64::from(
            capture.terrain.width_metres * capture.terrain.depth_metres,
        ) / 1_000_000.0,
        scene_entity_counts: state.scene_entity_counts.take().unwrap_or_default(),
        warmup_frames_per_mode: SCENE_PERFORMANCE_WARMUP_FRAMES,
        sample_frames_per_mode: state.sample_frames,
        render_diagnostics_enabled,
        note: "Paired, same-camera, release-mode offscreen measurements at 2560x1440. Wall time covers Bevy scheduling, extraction, and render-world synchronization; GPU timestamp diagnostics are required for a conclusive 60 FPS result. Run on a base M5 MacBook Air after thermal warm-up for target-hardware acceptance; results from other hosts are comparative only.",
        results: core::mem::take(&mut state.results),
    };
    write_scene_performance_benchmark(&capture.output, &report);
    exit.write(AppExit::Success);
}

fn summed_render_metric(
    diagnostics: &BTreeMap<String, BenchmarkMetricSummary>,
    suffix: &str,
    value: impl Fn(&BenchmarkMetricSummary) -> f64,
) -> Option<f64> {
    let matching = diagnostics
        .iter()
        .filter(|(path, _)| path.ends_with(suffix))
        .map(|(_, metric)| value(metric))
        .collect::<Vec<_>>();
    (!matching.is_empty()).then(|| matching.into_iter().sum())
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}

fn benchmark_target_hardware() -> BenchmarkTargetHardware {
    BenchmarkTargetHardware {
        model: "13-inch MacBook Air (2026 base configuration)",
        chip: "Apple M5",
        cpu: "10-core CPU (4 super, 6 efficiency)",
        gpu: "8-core GPU",
        unified_memory_gib: 16,
        memory_bandwidth_gib_per_second: 153,
        cooling: "fanless",
    }
}

fn benchmark_host() -> BenchmarkHost {
    let profiler = (std::env::consts::OS == "macos")
        .then(|| {
            Command::new("system_profiler")
                .args([
                    "SPHardwareDataType",
                    "SPDisplaysDataType",
                    "-detailLevel",
                    "mini",
                ])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
        })
        .flatten();
    BenchmarkHost {
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        chip_or_cpu: profiler
            .as_deref()
            .and_then(|profile| profiler_value(profile, "Chip"))
            .or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok()),
        gpu: profiler
            .as_deref()
            .and_then(|profile| profiler_value(profile, "Chipset Model")),
        memory: profiler
            .as_deref()
            .and_then(|profile| profiler_value(profile, "Memory")),
        release_build: !cfg!(debug_assertions),
    }
}

fn benchmark_host_matches_target(host: &BenchmarkHost) -> bool {
    host.os == "macos"
        && host.architecture == "aarch64"
        && host
            .chip_or_cpu
            .as_deref()
            .is_some_and(|chip| chip.contains("Apple M5"))
        && host
            .gpu
            .as_deref()
            .is_some_and(|gpu| gpu.contains("Apple M5"))
        && host
            .memory
            .as_deref()
            .and_then(|memory| memory.split_whitespace().next())
            .and_then(|amount| amount.parse::<u16>().ok())
            .is_some_and(|gib| gib >= 16)
}

fn profiler_value(profile: &str, key: &str) -> Option<String> {
    profile.lines().find_map(|line| {
        let (candidate, value) = line.trim().split_once(':')?;
        (candidate == key).then(|| value.trim().to_owned())
    })
}

fn write_leaf_benchmark(output: &Path, report: &LeafBenchmarkReport) {
    let json = serde_json::to_string_pretty(report).expect("benchmark report serializes");
    fs::write(output.join("leaf-benchmark.json"), format!("{json}\n"))
        .expect("benchmark JSON writes");
    let fastest = report
        .results
        .iter()
        .map(|result| result.mean_ms)
        .fold(f64::INFINITY, f64::min);
    let mut markdown = format!(
        "# Dense-forest leaf benchmark\n\nFixture: `{}`; {} trees; {}x{}; {} measured frames per mode after {} warm-up frames.\n\n| Representation | Triangles / leaf | Scene leaf triangles | Mean ms | Median ms | P95 ms | Mean FPS | Cost vs fastest |\n|---|---:|---:|---:|---:|---:|---:|---:|\n",
        report.fixture,
        report.tree_count,
        report.resolution[0],
        report.resolution[1],
        report.sample_frames_per_mode,
        report.warmup_frames_per_mode,
    );
    for result in &report.results {
        markdown.push_str(&format!(
            "| {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.1} | {:.2}x |\n",
            result.representation,
            result.triangles_per_leaf,
            result.scene_leaf_triangles,
            result.mean_ms,
            result.median_ms,
            result.p95_ms,
            result.mean_fps,
            result.mean_ms / fastest,
        ));
    }
    markdown.push_str("\n_Frame time is end-to-end with vsync disabled, not an isolated GPU timestamp. Geometry, camera, weather, wind state, and all non-leaf work are held constant._\n");
    fs::write(output.join("comparison.md"), markdown).expect("benchmark table writes");
}

fn write_tree_lighting_benchmark(output: &Path, report: &TreeLightingBenchmarkReport) {
    let json = serde_json::to_string_pretty(report).expect("benchmark report serializes");
    fs::write(
        output.join("tree-lighting-benchmark.json"),
        format!("{json}\n"),
    )
    .expect("tree-lighting benchmark JSON writes");
    let baseline = report.results.first().map_or(1.0, |result| result.mean_ms);
    let mut markdown = format!(
        "# Dense-forest tree-lighting benchmark\n\nFixture: `{}`; {} trees; {}x{}; {} measured frames per mode after {} warm-up frames. LOD0 cambered leaves are forced to expose the maximum leaf-shadow cost.\n\n| Mode | Canopy AO | Leaf self shadows | Mean ms | Median ms | P95 ms | Mean FPS | Cost vs baseline |\n|---|:---:|:---:|---:|---:|---:|---:|---:|\n",
        report.fixture,
        report.tree_count,
        report.resolution[0],
        report.resolution[1],
        report.sample_frames_per_mode,
        report.warmup_frames_per_mode,
    );
    for result in &report.results {
        markdown.push_str(&format!(
            "| {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.1} | {:.2}x |\n",
            result.mode,
            if result.ambient_occlusion {
                "yes"
            } else {
                "no"
            },
            if result.self_shadows { "yes" } else { "no" },
            result.mean_ms,
            result.median_ms,
            result.p95_ms,
            result.mean_fps,
            result.mean_ms / baseline,
        ));
    }
    markdown.push_str("\n_Frame time is end-to-end with vsync disabled, not an isolated GPU timestamp. Canopy AO is a WebGPU-safe per-vertex visibility term; self shadows use the directional shadow map._\n");
    fs::write(output.join("tree-lighting-comparison.md"), markdown)
        .expect("tree-lighting benchmark table writes");
}

fn write_scene_performance_benchmark(output: &Path, report: &ScenePerformanceBenchmarkReport) {
    let json = serde_json::to_string_pretty(report).expect("benchmark report serializes");
    fs::write(
        output.join("scene-performance-benchmark.json"),
        format!("{json}\n"),
    )
    .expect("scene performance benchmark JSON writes");
    let baseline = report.results.first().map_or(1.0, |result| result.mean_ms);
    let mut markdown = format!(
        "# Tactical scene QHD/60 performance benchmark\n\nTarget: {} with {}, {}, {} GiB unified memory, and {} cooling. Scene: `{}`; {} playable trees; {} leaf entities; {} vista tree billboards; {}x{}; {} measured frames per mode after {} warm-up frames. The {:.3} ms 60 FPS budget is evaluated at P95; P99, worst-frame, and over-budget counts remain in the JSON report.\n\nHost: `{}` / `{}`; release build: **{}**; target-class match: **{}**; target acceptance: **{}**. A conclusive target result requires GPU timestamps and a run on target-class hardware.\n\n| Mode | Wall P95 ms | GPU P95 ms | Limiting P95 ms | Budget used | Headroom ms | Measured 60 FPS gate | Frames over budget | Cost vs natural |\n|---|---:|---:|---:|---:|---:|:---:|---:|---:|\n",
        report.target_hardware.model,
        report.target_hardware.chip,
        report.target_hardware.gpu,
        report.target_hardware.unified_memory_gib,
        report.target_hardware.cooling,
        report.fixture,
        report.playable_tree_count,
        report.playable_leaf_entities,
        report.vista_tree_entities,
        report.resolution[0],
        report.resolution[1],
        report.sample_frames_per_mode,
        report.warmup_frames_per_mode,
        report.frame_budget_ms,
        report.host.os,
        report.host.architecture,
        report.host.release_build,
        report.host_matches_target,
        report
            .target_acceptance_passes
            .map_or("n/a", |passed| if passed { "pass" } else { "fail" }),
    );
    for result in &report.results {
        markdown.push_str(&format!(
            "| {} | {:.3} | {} | {:.3} | {:.1}% | {:+.3} | {} | {} ({:.1}%) | {:.2}x |\n",
            result.mode,
            result.p95_ms,
            result
                .gpu_elapsed_p95_ms
                .map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3}")),
            result.limiting_p95_ms,
            result.budget_utilization_percent,
            result.headroom_ms,
            result
                .measured_60_fps_passes
                .map_or("n/a", |passed| { if passed { "pass" } else { "fail" } }),
            result.frames_over_budget,
            result.frames_over_budget_percent,
            result.mean_ms / baseline,
        ));
    }
    if let Some(natural) = report.results.first() {
        let assets = &natural.tree_asset_residency;
        markdown.push_str(&format!(
            "\nNatural-view tree residency: {} variants; {} source branches; {} source leaves; {} trunk vertices; {} detailed-branch vertices; {} cambered-leaf vertices; {} leaf-card vertices; {} bud vertices; {} aggregate-branch vertices (LOD1/2: {} / {} vertices, {} / {} triangles); {} LOD1 impostor cards / {} vertices; {} impostor vertices; {:.2} / {:.2} / {:.2} / {:.2} MiB for LOD1/2/3/4 impostor pixels ({:.2} MiB total); {} ms cumulative demand-generation time. Resident LOD mask: `{:#08b}`.\n",
            assets.variants,
            assets.source_branches,
            assets.source_leaves,
            assets.trunk_vertices,
            assets.detailed_branch_vertices,
            assets.cambered_leaf_vertices,
            assets.leaf_card_vertices,
            assets.bud_vertices,
            assets.aggregate_branch_vertices,
            assets.aggregate_branch_vertices_by_lod[0],
            assets.aggregate_branch_vertices_by_lod[1],
            assets.aggregate_branch_triangles_by_lod[0],
            assets.aggregate_branch_triangles_by_lod[1],
            assets.lod1_impostor_cards,
            assets.lod1_impostor_vertices,
            assets.impostor_vertices,
            assets.impostor_texture_bytes_by_lod[0] as f64 / (1024.0 * 1024.0),
            assets.impostor_texture_bytes_by_lod[1] as f64 / (1024.0 * 1024.0),
            assets.impostor_texture_bytes_by_lod[2] as f64 / (1024.0 * 1024.0),
            assets.impostor_texture_bytes_by_lod[3] as f64 / (1024.0 * 1024.0),
            assets.impostor_texture_bytes as f64 / (1024.0 * 1024.0),
            assets.generation_milliseconds,
            assets.generated_lod_mask,
        ));
    }
    markdown.push_str("\n## GPU isolation attribution\n\nPositive values estimate the hidden family\'s contribution to each GPU pass versus natural mode. Negative values indicate measurement noise or cross-pass effects.\n\n| Isolated family | Opaque 3D median ms | Opaque 3D P95 ms | Transparent 3D median ms | Transparent 3D P95 ms |\n|---|---:|---:|---:|---:|\n");
    for result in &report.results {
        if result.gpu_cost_attribution_vs_natural_ms.is_empty() {
            continue;
        }
        let opaque = result
            .gpu_cost_attribution_vs_natural_ms
            .get("render/main_opaque_pass_3d/elapsed_gpu");
        let transparent = result
            .gpu_cost_attribution_vs_natural_ms
            .get("render/main_transparent_pass_3d/elapsed_gpu");
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            result.mode,
            opaque.map_or_else(
                || "n/a".to_owned(),
                |value| format!("{:+.3}", value.median_ms)
            ),
            opaque.map_or_else(|| "n/a".to_owned(), |value| format!("{:+.3}", value.p95_ms)),
            transparent.map_or_else(
                || "n/a".to_owned(),
                |value| format!("{:+.3}", value.median_ms)
            ),
            transparent.map_or_else(|| "n/a".to_owned(), |value| format!("{:+.3}", value.p95_ms)),
        ));
    }
    markdown.push_str("\n_An `n/a` gate means GPU timestamps were unavailable, so wall timing alone cannot certify the target. Raw render diagnostics and per-pass isolation deltas are retained in the JSON report. Isolation modes hide one named production entity family while holding camera, terrain, lighting, and other scene work constant._\n");
    fs::write(output.join("scene-performance-comparison.md"), markdown)
        .expect("scene performance benchmark table writes");
}

fn capture_views(
    mut commands: Commands,
    mut state: Option<ResMut<CaptureState>>,
    mut tree_lod_override: ResMut<TreeLodRenderOverride>,
    mut camera: Single<
        (
            Entity,
            &mut Transform,
            &mut GlobalTransform,
            &mut Projection,
            Option<&AtmosphereEnvironmentMapLight>,
            Option<&EnvironmentMapLight>,
            Option<&Bloom>,
            &Exposure,
            &Tonemapping,
        ),
        With<Camera3d>,
    >,
    lighting: LightingObservationParams,
    mut overlays: Query<&mut Visibility, (With<CaptureOverlay>, Without<VistaTerrain>)>,
    mut tree_backdrops: Query<
        (&mut Visibility, &mut Transform),
        (
            With<TreeReviewBackdrop>,
            Without<Camera3d>,
            Without<CaptureOverlay>,
            Without<VistaTerrain>,
        ),
    >,
    obstacles: Query<(&SceneObstacle, Has<Collider>)>,
    rock_visuals: Query<&Mesh3d, With<ProceduralRockVisual>>,
    tree_lods: Query<(&TreeLod, &ViewVisibility, Option<&ChildOf>)>,
    tree_lod_clusters: Query<
        (
            &TreeLod,
            &TreeLodCluster,
            &GlobalTransform,
            &VisibilityRange,
            &ViewVisibility,
        ),
        Without<Camera3d>,
    >,
    tree_bakes: Query<&TreeImpostorProvenance>,
    foliage: Query<&GroundScatterLayer>,
    terrain_materials: Query<(), With<TerrainMaterialPresentation>>,
    meshes: Res<Assets<Mesh>>,
    mut scene_visibility: ParamSet<(
        Query<(&VistaTerrain, Has<Collider>)>,
        Query<&mut Visibility, (With<VistaTerrain>, Without<CaptureOverlay>)>,
        ResMut<Assets<TacticalTreeLeafCardMaterial>>,
        Query<
            (Entity, Option<&GroundScatterLayer>),
            With<MeshMaterial3d<TacticalTreeLeafCardMaterial>>,
        >,
        Query<
            Entity,
            Or<(
                With<TreeLeafRepresentation>,
                With<MeshMaterial3d<TacticalTreeLeafCardMaterial>>,
            )>,
        >,
        Query<(Entity, &GroundScatterLayer, &GlobalTransform, &Name), Without<Camera3d>>,
        Query<(), With<WeatherParticle>>,
        Query<
            &mut Visibility,
            (
                With<SceneObstacle>,
                Without<CaptureOverlay>,
                Without<VistaTerrain>,
                Without<TreeReviewBackdrop>,
            ),
        >,
    )>,
) {
    let Some(state) = state.as_deref_mut() else {
        return;
    };
    if state.phase.readback_in_flight() || state.view >= state.views.len() {
        return;
    }
    let view = state.views[state.view];
    if state.phase == CapturePhase::Configure {
        let lighting_mode = match view.lighting_mode {
            TreeLightingModeId::Baseline => TREE_LIGHTING_MODES[0],
            TreeLightingModeId::AmbientOcclusion => TREE_LIGHTING_MODES[1],
            TreeLightingModeId::Shadows => TREE_LIGHTING_MODES[2],
            TreeLightingModeId::Combined => TREE_LIGHTING_MODES[3],
        };
        for (_, material) in scene_visibility.p2().iter_mut() {
            material.surface_parameters.z = if material.physical_parameters.z > 0.5 {
                0.0
            } else {
                lighting_mode.ambient_occlusion_strength
            };
        }
        let leaf_entities = scene_visibility
            .p3()
            .iter()
            .map(|(entity, layer)| (entity, layer.copied()))
            .collect::<Vec<_>>();
        for (entity, layer) in leaf_entities {
            if lighting_mode.shadows_enabled && layer != Some(GroundScatterLayer::DryLeaves) {
                commands.entity(entity).remove::<NotShadowCaster>();
            } else {
                commands.entity(entity).insert(NotShadowCaster);
            }
        }
        tree_lod_override.lod = view.render_lod_override;
        tree_lod_override.leaf = view.leaf_lod_override;
        tree_lod_override.projected_scale = view.projected_scale;
        let specimen_representation = view.specimen_leaf;
        let specimen_view = specimen_representation.is_some();
        let specimen_pipeline_warmup = view.warmup;
        let (suppress_leaves, suppress_grass, suppress_understory) = (
            view.suppress_leaves,
            view.suppress_grass,
            view.suppress_understory,
        );
        let production_leaves = scene_visibility.p4().iter().collect::<Vec<_>>();
        for entity in production_leaves {
            commands.entity(entity).insert(if suppress_leaves {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
        let ground_scatter_entities = scene_visibility
            .p5()
            .iter()
            .map(|(entity, layer, transform, name)| {
                (
                    entity,
                    *layer,
                    transform.translation(),
                    name.as_str().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let understory_focus = view.understory_species.and_then(|common_name| {
            ground_scatter_entities
                .iter()
                .filter(|(_, layer, _, name)| {
                    *layer == GroundScatterLayer::Understory
                        && name == &format!("Shared {common_name} shrub wood")
                })
                .min_by(|left, right| {
                    left.2
                        .xz()
                        .length_squared()
                        .total_cmp(&right.2.xz().length_squared())
                })
                .map(|(_, _, position, _)| *position)
        });
        if view.debris_target {
            let pairs = lighting
                .litter_anchors
                .iter()
                .flat_map(|anchors| anchors.pairs.iter().copied())
                .collect::<Vec<_>>();
            let obstacles = lighting
                .obstacle_transforms
                .iter()
                .map(|(obstacle, transform)| {
                    let radius = match obstacle {
                        SceneObstacle::Tree => 1.8,
                        SceneObstacle::Rock(recipe) => recipe.collision_radius_metres(),
                    };
                    (transform.translation(), radius)
                })
                .collect::<Vec<_>>();
            let terrain = lighting.terrain.single().expect("one tactical terrain");
            if let Some(target) = reviewable_debris_capture_target(
                &pairs,
                terrain,
                &obstacles,
                state.tree_review_azimuth_degrees,
            ) {
                state.debris_focus = Some(target.focus);
                state.debris_camera = Some(target.camera);
                state.debris_leaf_distance_metres = Some(target.leaf_distance_metres);
                state.debris_twig_distance_metres = Some(target.twig_distance_metres);
            } else {
                state.debris_focus = None;
                state.debris_camera = None;
                state.debris_leaf_distance_metres = None;
                state.debris_twig_distance_metres = None;
            }
        }
        for (entity, layer, position, name) in ground_scatter_entities {
            let isolated_understory_visible = view
                .understory_species
                .zip(understory_focus)
                .is_some_and(|(common_name, focus)| {
                    layer == GroundScatterLayer::Understory
                        && name.starts_with(&format!("Shared {common_name} "))
                        && position.distance_squared(focus) <= 0.0001
                });
            let hide_for_view = if view.understory_species.is_some() {
                !isolated_understory_visible
            } else {
                (layer == GroundScatterLayer::Grass && suppress_grass)
                    || (layer == GroundScatterLayer::Understory && suppress_understory)
                    || view.hide_obstacles
            };
            if matches!(
                layer,
                GroundScatterLayer::Grass | GroundScatterLayer::Understory
            ) || view.hide_obstacles
                || view.understory_species.is_some()
            {
                commands.entity(entity).insert(if hide_for_view {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                });
            }
        }
        for mut visibility in scene_visibility.p7().iter_mut() {
            *visibility = if view.hide_obstacles {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        if let Some(entity) = state.tree_focus_entity {
            commands
                .entity(entity)
                .insert(if specimen_view || view.hide_obstacles {
                    Visibility::Hidden
                } else {
                    Visibility::Visible
                });
        }
        for &entity in &state.tree_review_entities {
            commands
                .entity(entity)
                .insert(if specimen_view || specimen_pipeline_warmup {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                });
        }
        for &(entity, representation) in &state.tree_review_leaf_entities {
            commands.entity(entity).insert(
                if specimen_representation == Some(representation) || specimen_pipeline_warmup {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
            );
        }
        let (transform, target, obstruction) = match view.pose {
            CapturePose::AnimationPlayObstruction { yaw_degrees } => {
                animation_play_obstruction_camera(state, &lighting.spatial, yaw_degrees)
            }
            CapturePose::AnimationPlayBoundary {
                player_x,
                player_z,
                yaw_degrees,
            } => animation_play_boundary_camera(
                state,
                lighting.terrain.single().expect("one tactical terrain"),
                &lighting.spatial,
                player_x,
                player_z,
                yaw_degrees,
            ),
            CapturePose::UnderstoryReview => {
                let focus = understory_focus.unwrap_or(state.obstacle_focus);
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                let target = focus + Vec3::Y * 1.35;
                let position = focus + Vec3::new(azimuth.sin() * 5.5, 1.9, azimuth.cos() * 5.5);
                (
                    Transform::from_translation(position).looking_at(target, Vec3::Y),
                    target,
                    None,
                )
            }
            CapturePose::TreeReview => {
                let tree = state.tree_focus.unwrap_or(state.obstacle_focus);
                let species = state
                    .tree_focus_entity
                    .and_then(|entity| lighting.presented_tree_names.get(entity).ok())
                    .map(Name::as_str)
                    .unwrap_or_default();
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                let (radius, camera_height, target_height) = if species.ends_with("common beech") {
                    (30.0, 10.0, 8.0)
                } else {
                    (18.0, 6.0, 4.5)
                };
                let target = tree + Vec3::Y * target_height;
                let position = tree
                    + Vec3::new(
                        azimuth.sin() * radius,
                        camera_height,
                        azimuth.cos() * radius,
                    );
                (
                    Transform::from_translation(position).looking_at(target, Vec3::Y),
                    target,
                    None,
                )
            }
            CapturePose::BranchJunction => {
                let tree = state.tree_focus.unwrap_or(state.obstacle_focus);
                let species = state
                    .tree_focus_entity
                    .and_then(|entity| lighting.presented_tree_names.get(entity).ok())
                    .map(Name::as_str)
                    .unwrap_or_default();
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                let (height, radius, camera_lift) = if species.ends_with("common beech") {
                    // Closed-canopy beech carries its first live scaffold limbs
                    // well above the oak junction sampled by the old camera.
                    (4.5, 8.0, 2.0)
                } else {
                    (0.8, 6.0, 1.4)
                };
                let target = tree + Vec3::Y * height;
                let position =
                    target + Vec3::new(azimuth.sin() * radius, camera_lift, azimuth.cos() * radius);
                (
                    Transform::from_translation(position).looking_at(target, Vec3::Y),
                    target,
                    None,
                )
            }
            _ => {
                let (transform, target) = camera_for_view(view.pose, state);
                (transform, target, None)
            }
        };
        *camera.1 = transform;
        *camera.2 = GlobalTransform::from(transform);
        if let Projection::Perspective(projection) = &mut *camera.3 {
            projection.fov = view.fov_degrees.to_radians();
        }
        for mut visibility in &mut overlays {
            *visibility = if view.overlay {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        for mut visibility in &mut scene_visibility.p1() {
            *visibility = if view.vista_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        for (mut visibility, mut backdrop) in &mut tree_backdrops {
            *visibility = if view.show_tree_backdrop || specimen_view {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            if *visibility == Visibility::Visible {
                let away_from_camera = (target - camera.1.translation).normalize_or_zero();
                backdrop.translation = target + away_from_camera * 5.0;
                backdrop.rotation = Transform::from_translation(backdrop.translation)
                    .looking_at(camera.1.translation, Vec3::Y)
                    .rotation;
            }
        }
        state.phase = CapturePhase::Settling {
            frames: 0,
            prime_readbacks: 0,
        };
        if !view.warmup {
            let vista_observation = matches!(view.pose, CapturePose::AnimationPlayBoundary { .. })
                .then(|| largest_visible_vista_tree(&lighting.vista_trees, camera.1.translation))
                .flatten();
            let nearest_playable_tree_distance_metres =
                matches!(view.pose, CapturePose::AnimationPlayBoundary { .. })
                    .then(|| {
                        lighting
                            .obstacle_transforms
                            .iter()
                            .filter(|(obstacle, _)| matches!(obstacle, SceneObstacle::Tree))
                            .map(|(_, transform)| {
                                camera.1.translation.distance(transform.translation())
                            })
                            .min_by(f32::total_cmp)
                    })
                    .flatten();
            state.captures.push(CaptureRecord {
                view: view.slug.to_owned(),
                label: view.label.to_owned(),
                screenshot: format!("{}.png", view.slug),
                rendered_resolution: [0, 0],
                camera_translation: camera.1.translation.to_array(),
                camera_target: target.to_array(),
                camera_up: camera.1.up().as_vec3().to_array(),
                vertical_fov_degrees: view.fov_degrees,
                camera_boom_desired_metres: obstruction.map(|value| value.desired_metres),
                camera_boom_resolved_metres: obstruction.map(|value| value.resolved_metres),
                camera_obstruction_hit: obstruction.map(|value| value.hit),
                nearest_playable_tree_distance_metres,
                largest_visible_vista_tree_distance_metres: vista_observation
                    .map(|value| value.distance_metres),
                largest_visible_vista_tree_angular_height_degrees: vista_observation
                    .map(|value| value.angular_height_degrees),
                largest_visible_vista_tree_has_collider: vista_observation
                    .map(|value| value.has_collider),
                foreground_pixel_bps: 0,
                detail_pixel_bps: 0,
                canopy_pixel_bps: 0,
                forced_tree_lod: tree_lod_override.lod,
                focused_tree_lod_queued: None,
                focused_tree_species: state
                    .tree_focus_entity
                    .and_then(|entity| lighting.presented_tree_names.get(entity).ok())
                    .and_then(|name| name.as_str().strip_prefix("Presented mature "))
                    .map(str::to_owned),
                focused_understory_species: understory_focus
                    .and(view.understory_species)
                    .map(str::to_owned),
                diagnostic_leaf_suppression: suppress_leaves,
                diagnostic_grass_suppression: suppress_grass,
                debris_leaf_distance_metres: view
                    .debris_target
                    .then_some(state.debris_leaf_distance_metres)
                    .flatten(),
                debris_twig_distance_metres: view
                    .debris_target
                    .then_some(state.debris_twig_distance_metres)
                    .flatten(),
                lighting_luminance_samples: Vec::new(),
                lighting_luminance_delta: f32::INFINITY,
                lighting_ready: false,
            });
        }
        return;
    }
    // Custom terrain and foliage pipelines compile asynchronously. Give the
    // warmup view a wider budget so the first review frame cannot race a new
    // shader permutation while ordinary camera transitions stay quick.
    let temporal_tree_traversal = state.profile == "tree-cold-traversal" && !view.warmup;
    let settle_target = if temporal_tree_traversal {
        0
    } else if state.view == 0 {
        state.settle_frames.saturating_mul(4)
    } else {
        state.settle_frames
    };
    let Some((settled, prime_readbacks)) = state.phase.settling() else {
        return;
    };
    if settled < settle_target {
        state.phase = CapturePhase::Settling {
            frames: settled + 1,
            prime_readbacks,
        };
        return;
    }

    if view.observe_recursive_lod {
        let camera_position = camera.1.translation;
        state.recursive_lods_observed.extend(
            tree_lod_clusters
                .iter()
                .filter(|(_, cluster, transform, range, visibility)| {
                    let world_center = transform.transform_point(cluster.center);
                    visibility.get()
                        && range.is_visible_at_all(camera_position.distance(world_center))
                })
                .map(|(lod, cluster, _, _, _)| (cluster.primary_group, lod.0)),
        );
        // LOD1 and coarser collapse many biological groups into one mesh to
        // reduce draw calls. Record those visible aggregate tiers separately
        // instead of pretending they still have per-cluster entity identity.
        state.recursive_aggregate_lods_observed.extend(
            tree_lods
                .iter()
                .filter(|(lod, visibility, _)| lod.0 > 0 && visibility.get())
                .map(|(lod, _, _)| lod.0),
        );
    }

    // Bevy's asynchronous window readback can still contain the render world
    // from before a camera transition. Prime one disposable readback per view,
    // then capture again without changing any scene or camera state.
    let required_prime_readbacks = if temporal_tree_traversal { 0 } else { 2 };
    if prime_readbacks < required_prime_readbacks {
        state.phase = CapturePhase::Readback {
            view: state.view,
            prime_readbacks,
            kind: CaptureReadback::Prime,
        };
        commands.spawn(Screenshot::primary_window()).observe(
            |captured: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                let CapturePhase::Readback {
                    view,
                    prime_readbacks,
                    kind: CaptureReadback::Prime,
                } = state.phase
                else {
                    return;
                };
                if state.view != view {
                    return;
                }
                state
                    .lighting_luminance_samples
                    .push(mean_luminance(captured.image.data.as_deref()));
                state.phase = CapturePhase::Settling {
                    frames: 0,
                    prime_readbacks: prime_readbacks + 1,
                };
            },
        );
        return;
    }
    if view.warmup {
        state.phase = CapturePhase::Readback {
            view: state.view,
            prime_readbacks,
            kind: CaptureReadback::Warmup,
        };
        commands.spawn(Screenshot::primary_window()).observe(
            |_: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                if !matches!(
                    state.phase,
                    CapturePhase::Readback {
                        view,
                        kind: CaptureReadback::Warmup,
                        ..
                    } if view == state.view
                ) {
                    return;
                }
                state.view += 1;
                state.lighting_luminance_samples.clear();
                state.phase = CapturePhase::Configure;
            },
        );
        return;
    }

    if let Some(record) = state.captures.last_mut() {
        record.focused_tree_lod_queued = record.forced_tree_lod.map(|forced_lod| {
            focused_tree_lod_queued(state.tree_focus_entity, forced_lod, &tree_lods)
        });
        record
            .lighting_luminance_samples
            .clone_from(&state.lighting_luminance_samples);
        record.lighting_luminance_delta = luminance_delta(&state.lighting_luminance_samples);
        record.lighting_ready =
            temporal_tree_traversal || lighting_samples_stable(&state.lighting_luminance_samples);
    }
    let celestial = capture_celestial(
        state.absolute_minute,
        state.latitude_microdegrees,
        state.longitude_microdegrees,
    );
    let observed_presentation = observed_presentation_features(
        &lighting.settings,
        camera.4,
        camera.5,
        camera.6,
        camera.7,
        camera.8,
        &lighting.ambient,
        &lighting.ambient_handoff,
        &celestial,
    );
    let path = state.output.join(format!("{}.png", view.slug));
    let final_view = state.view + 1 == state.views.len();
    let weather_particle_count = scene_visibility.p6().iter().count();
    let mut final_data = final_view.then(|| {
        build_manifest(
            state,
            &obstacles,
            &lighting.presented_tree_roots,
            &rock_visuals,
            &tree_lods,
            &tree_bakes,
            &foliage,
            &terrain_materials,
            &meshes,
            &scene_visibility.p0(),
            weather_particle_count,
            observed_presentation,
        )
    });
    state.phase = CapturePhase::Readback {
        view: state.view,
        prime_readbacks,
        kind: CaptureReadback::Screenshot,
    };
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              mut state: ResMut<CaptureState>,
              mut exit: MessageWriter<AppExit>| {
            let foreground_pixel_bps = foreground_pixel_bps(captured.image.data.as_deref());
            if !matches!(
                state.phase,
                CapturePhase::Readback {
                    view,
                    kind: CaptureReadback::Screenshot,
                    ..
                } if view == state.view
            ) {
                return;
            }
            let rendered_width = captured.image.width();
            let rendered_height = captured.image.height();
            let detail_pixel_bps = foliage_detail_pixel_bps(
                captured.image.data.as_deref(),
                rendered_width,
                rendered_height,
            );
            let canopy_pixel_bps = tree_canopy_pixel_bps(
                captured.image.data.as_deref(),
                rendered_width,
                rendered_height,
            );
            save_to_disk(&path)(captured);
            if let Some(record) = state.captures.last_mut() {
                record.foreground_pixel_bps = foreground_pixel_bps;
                record.detail_pixel_bps = detail_pixel_bps;
                record.canopy_pixel_bps = canopy_pixel_bps;
                record.rendered_resolution = [rendered_width, rendered_height];
            }
            state.view += 1;
            state.lighting_luminance_samples.clear();
            state.phase = CapturePhase::Configure;
            if let Some(pending) = final_data.take() {
                let (manifest, valid) =
                    pending.finalize_after_screenshot(&state.captures, &state.views);
                finish_capture(&state.output, &manifest, valid, &mut exit);
            }
        },
    );
}

fn focused_tree_lod_queued(
    focused_tree: Option<Entity>,
    forced_lod: u8,
    tree_lods: &Query<(&TreeLod, &ViewVisibility, Option<&ChildOf>)>,
) -> bool {
    let Some(focused_tree) = focused_tree else {
        return false;
    };
    tree_lods.iter().any(|(lod, view_visibility, parent)| {
        lod.0 == forced_lod
            && view_visibility.get()
            && parent.is_some_and(|parent| parent.parent() == focused_tree)
    })
}

fn camera_for_view(pose: CapturePose, state: &CaptureState) -> (Transform, Vec3) {
    let half = state.terrain.width_metres.max(state.terrain.depth_metres) * 0.5;
    let (position, target, up) = match pose {
        CapturePose::Ground => (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
        CapturePose::AnimationPlay { yaw_degrees } => {
            let yaw = Quat::from_rotation_y(yaw_degrees.to_radians());
            let focus = state.animation_play_focus;
            let position = focus + yaw * Vec3::Z * 3.75;
            (position, focus, Vec3::Y)
        }
        CapturePose::AnimationPlayBoundary { .. } => unreachable!(
            "boundary views require terrain height and the live production spatial query"
        ),
        CapturePose::AnimationPlayObstruction { .. } => unreachable!(
            "obstruction views require the live spatial query used by the production camera"
        ),
        CapturePose::TreeColdTraversal { distance } => tree_cold_traversal_camera(state, distance),
        CapturePose::TreeReview => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                // Frame both current mature-tree presentations, including the
                // taller closed-canopy beech. The previous 21.2 m radius and
                // 4.5 m target cropped its crown at every review azimuth.
                let radius = 30.0;
                (
                    tree + Vec3::new(azimuth.sin() * radius, 10.0, azimuth.cos() * radius),
                    tree + Vec3::new(0.0, 8.0, 0.0),
                    Vec3::Y,
                )
            },
        ),
        CapturePose::RecursiveTree => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                (
                    // Exercise the current detailed-cluster to leafed-twig
                    // handoff. The older 47 m plate predated the tightened
                    // production LOD ranges and could only see whole-crown
                    // aggregates, so it could never prove recursive mixing.
                    tree + Vec3::new(8.0, 4.5, 8.0),
                    tree + Vec3::new(0.0, 4.5, 0.0),
                    Vec3::Y,
                )
            },
        ),
        CapturePose::Root => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                let root = tree - Vec3::Y * (TREE_TRUNK_HEIGHT_METRES * 0.5 - 0.22);
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                (
                    // Include the surrounding soil in the root-junction plate:
                    // the camera-local terrain recipe now continues the root
                    // ridges beyond the tree mesh, which cannot be judged in
                    // an extreme trunk-only close-up.
                    root + Vec3::new(azimuth.sin() * 3.8, 1.3, azimuth.cos() * 3.8),
                    root + Vec3::Y * 0.08,
                    Vec3::Y,
                )
            },
        ),
        CapturePose::BranchJunction => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                let junction = tree + Vec3::Y * 0.15;
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                (
                    junction + Vec3::new(azimuth.sin() * 3.2, 0.65, azimuth.cos() * 3.2),
                    junction,
                    Vec3::Y,
                )
            },
        ),
        CapturePose::Rock => state.rock_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |rock| {
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                (
                    rock + Vec3::new(azimuth.sin() * 3.0, 1.1, azimuth.cos() * 3.0),
                    rock - Vec3::Y * 0.15,
                    Vec3::Y,
                )
            },
        ),
        CapturePose::TerrainGrazing => {
            let target = state.obstacle_focus - Vec3::Y * 1.42;
            // Keep this plate close enough to resolve centimetre-scale ground
            // geometry. The former seven-metre view was useful as landscape
            // context but could not distinguish a 25 cm detail grid from the
            // authoritative two-metre surface it refines.
            (
                target + Vec3::new(-1.65, 0.28, 1.45),
                target + Vec3::new(0.35, 0.01, -0.25),
                Vec3::Y,
            )
        }
        CapturePose::GrassSeam => {
            let target = state.obstacle_focus - Vec3::Y * 1.30;
            (target + Vec3::new(-3.4, 1.25, 3.4), target, Vec3::Y)
        }
        CapturePose::Debris => state.debris_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |target| {
                debris_detail_camera(
                    target,
                    state.debris_camera,
                    state.tree_review_azimuth_degrees,
                )
            },
        ),
        CapturePose::GroundCover => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                (
                    tree + Vec3::new(5.5, -0.4, 5.5),
                    tree + Vec3::new(0.0, -2.25, 0.0),
                    Vec3::Y,
                )
            },
        ),
        CapturePose::LeafSpecimen => state.tree_leaf_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |focus| {
                (
                    state.tree_leaf_camera.unwrap_or(focus + Vec3::Z),
                    focus,
                    Vec3::Y,
                )
            },
        ),
        CapturePose::UnderstoryReview => {
            unreachable!("understory review requires the live production ground-scatter query")
        }
        CapturePose::TreeLod { distance } => tree_lod_camera(state, distance),
        CapturePose::Overhead => (
            state.obstacle_focus + Vec3::new(0.0, half * 2.15, half * 0.16),
            state.obstacle_focus,
            Vec3::Z,
        ),
        CapturePose::Horizon => {
            let mut position = state.ground_eye_position + Vec3::Y * 12.0;
            position.y = position.y.max(
                state
                    .terrain
                    .maximum_height_metres
                    .max(state.vista_peak_metres)
                    + 12.0,
            );
            let direction = (state.peak_target.xz() - position.xz())
                .try_normalize()
                .unwrap_or(Vec2::X);
            let target =
                position + Vec3::new(direction.x, -0.08, direction.y) * (half * 10.0).max(200.0);
            (position, target, Vec3::Y)
        }
        CapturePose::VistaPeak => {
            let direction = (state.peak_target.xz() - state.obstacle_focus.xz())
                .try_normalize()
                .unwrap_or(Vec2::X);
            let safe_height = state
                .terrain
                .maximum_height_metres
                .max(state.vista_peak_metres)
                + 18.0;
            let mut position = state.obstacle_focus
                - Vec3::new(direction.x, 0.0, direction.y) * (half * 0.62)
                + Vec3::Y * 8.0;
            position.y = position.y.max(safe_height);
            let mut target = state.obstacle_focus
                + Vec3::new(direction.x, 0.0, direction.y) * (half * 5.0)
                + Vec3::Y * 2.0;
            target.y = target.y.clamp(position.y - 80.0, position.y - 8.0);
            (position, target, Vec3::Y)
        }
        CapturePose::VistaValley => {
            let direction = (state.valley_target.xz() - state.obstacle_focus.xz())
                .try_normalize()
                .unwrap_or(-Vec2::X);
            let safe_height = state
                .terrain
                .maximum_height_metres
                .max(state.vista_peak_metres)
                + 18.0;
            let mut position = state.obstacle_focus
                - Vec3::new(direction.x, 0.0, direction.y) * (half * 0.62)
                + Vec3::Y * 8.0;
            position.y = position.y.max(safe_height);
            let mut target = state.obstacle_focus
                + Vec3::new(direction.x, 0.0, direction.y) * (half * 5.0)
                + Vec3::Y * 2.0;
            target.y = target.y.clamp(position.y - 80.0, position.y - 8.0);
            (position, target, Vec3::Y)
        }
    };
    (
        Transform::from_translation(position).looking_at(target, up),
        target,
    )
}

fn animation_play_obstruction_camera(
    state: &CaptureState,
    spatial: &SpatialQuery,
    yaw_degrees: f32,
) -> (Transform, Vec3, Option<CameraObstructionObservation>) {
    let config = CameraRigConfig::default();
    let Some(tree) = state.tree_focus else {
        let (transform, target) =
            camera_for_view(CapturePose::AnimationPlay { yaw_degrees }, state);
        return (
            transform,
            target,
            Some(CameraObstructionObservation {
                desired_metres: config.lowered.distance,
                resolved_metres: config.lowered.distance,
                hit: false,
            }),
        );
    };
    let yaw = Quat::from_rotation_y(yaw_degrees.to_radians());
    let outward = yaw * Vec3::Z;
    let tree_root_y = tree.y - TREE_TRUNK_HEIGHT_METRES * 0.5;
    let target = Vec3::new(tree.x, tree_root_y + 1.35, tree.z) + outward * 0.95;
    let backward = -outward;
    let cast_direction = Dir3::new(backward).unwrap_or(Dir3::Z);
    let cast = spatial.cast_shape(
        &Collider::sphere(config.collision_radius),
        target,
        Quat::IDENTITY,
        cast_direction,
        &ShapeCastConfig::from_max_distance(config.lowered.distance)
            .with_target_distance(config.collision_margin),
        &SpatialQueryFilter::default(),
    );
    let distance = cast
        .map_or(config.lowered.distance, |hit| hit.distance)
        .clamp(0.0, config.lowered.distance);
    let position = target + backward * distance;
    let observation = CameraObstructionObservation {
        desired_metres: config.lowered.distance,
        resolved_metres: distance,
        hit: cast.is_some(),
    };
    (
        Transform::from_translation(position).looking_at(target, Vec3::Y),
        target,
        Some(observation),
    )
}

fn animation_play_boundary_camera(
    _state: &CaptureState,
    terrain: &SceneTerrain,
    spatial: &SpatialQuery,
    player_x: f32,
    player_z: f32,
    yaw_degrees: f32,
) -> (Transform, Vec3, Option<CameraObstructionObservation>) {
    let config = CameraRigConfig::default();
    let yaw = Quat::from_rotation_y(yaw_degrees.to_radians());
    let backward = yaw * Vec3::Z;
    let focus = Vec3::new(
        player_x,
        terrain
            .height_at(Vec2::new(player_x, player_z))
            .unwrap_or_default()
            + 1.48,
        player_z,
    );
    let cast_direction = Dir3::new(backward).unwrap_or(Dir3::Z);
    let cast = spatial.cast_shape(
        &Collider::sphere(config.collision_radius),
        focus,
        yaw,
        cast_direction,
        &ShapeCastConfig::from_max_distance(config.lowered.distance)
            .with_target_distance(config.collision_margin),
        &SpatialQueryFilter::default(),
    );
    let distance = cast
        .map_or(config.lowered.distance, |hit| hit.distance)
        .clamp(0.0, config.lowered.distance);
    let position = focus + backward * distance;
    (
        Transform::from_translation(position).looking_at(focus, Vec3::Y),
        focus,
        Some(CameraObstructionObservation {
            desired_metres: config.lowered.distance,
            resolved_metres: distance,
            hit: cast.is_some(),
        }),
    )
}

#[derive(Clone, Copy)]
struct VistaTreeObservation {
    distance_metres: f32,
    angular_height_degrees: f32,
    has_collider: bool,
}

fn largest_visible_vista_tree(
    trees: &Query<
        (
            &GlobalTransform,
            &TreeImpostorProvenance,
            &VisibilityRange,
            Has<Collider>,
        ),
        (
            With<VistaTreePresentation>,
            Without<SceneObstacle>,
            Without<Camera3d>,
        ),
    >,
    camera: Vec3,
) -> Option<VistaTreeObservation> {
    trees
        .iter()
        .filter_map(|(transform, provenance, range, has_collider)| {
            let distance = camera.distance(transform.translation());
            if !range.is_visible_at_all(distance) {
                return None;
            }
            let scale = transform.to_scale_rotation_translation().0.y.abs();
            let height = provenance
                .records
                .first()
                .map_or(0.0, |record| record.projected_bounds.w.abs())
                * scale;
            let angular_height_degrees =
                (2.0 * (height * 0.5).atan2(distance.max(0.001))).to_degrees();
            Some(VistaTreeObservation {
                distance_metres: distance,
                angular_height_degrees,
                has_collider,
            })
        })
        .max_by(|left, right| {
            left.angular_height_degrees
                .total_cmp(&right.angular_height_degrees)
        })
}

#[derive(Clone, Copy)]
struct CameraObstructionObservation {
    desired_metres: f32,
    resolved_metres: f32,
    hit: bool,
}

fn build_manifest(
    state: &CaptureState,
    obstacles: &Query<(&SceneObstacle, Has<Collider>)>,
    presented_tree_roots: &Query<(), With<PresentedTree>>,
    rock_visuals: &Query<&Mesh3d, With<ProceduralRockVisual>>,
    tree_lods: &Query<(&TreeLod, &ViewVisibility, Option<&ChildOf>)>,
    tree_bakes: &Query<&TreeImpostorProvenance>,
    foliage: &Query<&GroundScatterLayer>,
    terrain_materials: &Query<(), With<TerrainMaterialPresentation>>,
    meshes: &Assets<Mesh>,
    vistas: &Query<(&VistaTerrain, Has<Collider>)>,
    weather_particle_count: usize,
    presentation_features: PresentationFeatures,
) -> PendingCaptureManifest {
    // PresentedTree lives on the non-rendering root and means the complete
    // five-level presentation is cached and streamable. Counting transient
    // trunk/LOD children would incorrectly fail whenever the active camera is
    // legitimately using a far representation.
    let presented_trees = presented_tree_roots.iter().count();
    let presented_rocks = rock_visuals.iter().count();
    let mut collider_trees = 0;
    let mut collider_rocks = 0;
    for (kind, collidable) in obstacles {
        match kind {
            SceneObstacle::Tree => {
                collider_trees += usize::from(collidable);
            }
            SceneObstacle::Rock(_) => {
                collider_rocks += usize::from(collidable);
            }
        }
    }
    let procedural_rock_meshes = rock_visuals.iter().count();
    let rock_meshes_inside_colliders = procedural_rock_meshes == state.expected_rocks
        && rock_visuals.iter().all(|mesh_handle| {
            meshes.get(&mesh_handle.0).is_some_and(|mesh| {
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    .is_some_and(|positions| {
                        positions.as_float3().is_some_and(|positions| {
                            positions.iter().all(|position| {
                                Vec3::from_array(*position).length() <= ROCK_RADIUS_METRES + 0.001
                            })
                        })
                    })
            })
        });
    let tree_lods_presented = tree_lods
        .iter()
        .map(|(lod, _, _)| lod.0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let visible_group_lods = state
        .recursive_lods_observed
        .iter()
        .map(|&(group, lod)| [group, lod])
        .collect::<Vec<_>>();
    let recursive_tree_lod = RecursiveTreeLodSummary {
        primary_cluster_count: state
            .recursive_lods_observed
            .iter()
            .map(|(group, _)| group)
            .collect::<BTreeSet<_>>()
            .len(),
        visible_aggregate_lods: state
            .recursive_aggregate_lods_observed
            .iter()
            .copied()
            .collect(),
        mixed_lods_observed: !state.recursive_lods_observed.is_empty()
            && !state.recursive_aggregate_lods_observed.is_empty(),
        visible_group_lods,
    };
    let mut grass_clumps = 0;
    let mut understory_clumps = 0;
    let mut dry_leaf_patches = 0;
    let mut twig_patches = 0;
    let mut loose_stone_patches = 0;
    for layer in foliage {
        match layer {
            GroundScatterLayer::Grass => grass_clumps += 1,
            GroundScatterLayer::Understory => understory_clumps += 1,
            GroundScatterLayer::DryLeaves => dry_leaf_patches += 1,
            GroundScatterLayer::Twigs => twig_patches += 1,
            GroundScatterLayer::LooseStone => loose_stone_patches += 1,
        }
    }
    let foliage_summary = FoliageSummary {
        grass_clumps,
        understory_clumps,
        dry_leaf_patches,
        twig_patches,
        loose_stone_patches,
    };
    let tree_impostor_bakes = tree_bakes
        .iter()
        .map(|bake| TreeBakeSummary {
            seed: bake.seed,
            lod: bake.lod,
            bake_version: bake.bake_version,
            source_geometry_hash: format!("{:016x}", bake.source_geometry_hash),
            render_method: bake.render_method,
            atlas_size: [bake.atlas_width, bake.atlas_height],
            cards: bake
                .records
                .iter()
                .map(|record| TreeBakeCardSummary {
                    source_group: record.source_group,
                    source_leaf_count: record.source_leaf_count,
                    source_branch_count: record.source_branch_count,
                    view_direction: record.view_direction.to_array(),
                    projected_bounds: record.projected_bounds.to_array(),
                    atlas_region: record.atlas_region.to_array(),
                    opaque_pixel_count: record.opaque_pixel_count,
                    silhouette_centroid: record.silhouette_centroid.to_array(),
                })
                .collect(),
        })
        .collect();
    let mut presented_lods = BTreeSet::new();
    let mut vista_colliders = 0;
    let mut presented_chunks = 0;
    for (vista, collidable) in vistas {
        presented_lods.insert(vista.0);
        presented_chunks += 1;
        vista_colliders += usize::from(collidable);
    }
    let obstacle_summary = ObstacleSummary {
        generated_trees: state.expected_trees,
        generated_rocks: state.expected_rocks,
        presented_trees,
        presented_rocks,
        collider_trees,
        collider_rocks,
        procedural_rock_meshes,
        rock_meshes_inside_colliders,
        tree_lods_presented: tree_lods_presented.clone(),
    };
    let vista_summary = VistaSummary {
        supplied_lods: state.vista_lods_supplied,
        presented_lods: presented_lods.into_iter().collect(),
        presented_chunks,
        diameter_metres: state.vista_diameter_metres,
        minimum_height_metres: state.vista_minimum_metres,
        peak_height_metres: state.vista_peak_metres,
        relief_metres: state.vista_relief_metres,
        collider_count: vista_colliders,
    };
    let expects_precipitation =
        state.weather.precipitation != Precipitation::Clear && state.weather.intensity_bps > 0;
    let fixture_feature_expectation_met = match state.fixture.as_str() {
        "dense-woodland" => state.expected_trees >= 2,
        "sparse-woodland" => state.expected_trees >= 1,
        "flat-dry-grassland" => state.expected_trees == 0,
        "steep-open-hillside" => state.expected_rocks >= 1,
        "narrow-peak-lod-boundary" => state.vista_peak_metres >= 850.0,
        _ => true,
    };
    let expects_understory = matches!(
        state.fixture.as_str(),
        "dense-woodland" | "sparse-woodland" | "saturated-wetland"
    );
    let mut validation = ValidationSummary {
        all_views_captured: state.captures.len() == state.views.len() - 1,
        requested_views_captured_exactly_once: state.requested_views.iter().all(|view| {
            state
                .captures
                .iter()
                .filter(|capture| &capture.view == view)
                .count()
                == 1
        }) && state.captures.len()
            == state.requested_views.len(),
        requested_detail_targets_available: state.requested_views.iter().all(|view| {
            let record = state.captures.iter().find(|capture| &capture.view == view);
            let requirement = state
                .views
                .iter()
                .find(|spec| spec.slug == view)
                .map_or(DetailRequirement::None, |spec| spec.detail_requirement);
            match requirement {
                DetailRequirement::TreeFocus => state.tree_focus.is_some(),
                DetailRequirement::BranchFocusWithLeafSuppression => {
                    state.tree_focus.is_some()
                        && record.is_some_and(|capture| capture.diagnostic_leaf_suppression)
                }
                DetailRequirement::RockFocus => state.rock_focus.is_some(),
                DetailRequirement::GrassSuppressed => {
                    record.is_some_and(|capture| capture.diagnostic_grass_suppression)
                }
                DetailRequirement::GrassPresent => state.expects_grass && grass_clumps > 0,
                DetailRequirement::DebrisPair => {
                    state.debris_focus.is_some()
                        && state
                            .debris_leaf_distance_metres
                            .is_some_and(|distance| distance <= 0.275)
                        && state
                            .debris_twig_distance_metres
                            .is_some_and(|distance| distance <= 0.275)
                        && dry_leaf_patches > 0
                        && twig_patches > 0
                }
                DetailRequirement::UnderstoryFocus => record
                    .and_then(|capture| capture.focused_understory_species.as_deref())
                    .is_some(),
                DetailRequirement::None => true,
            }
        }),
        camera_obstruction_resolved: state.requested_views.iter().all(|view| {
            let Some(spec) = state.views.iter().find(|spec| spec.slug == view) else {
                return false;
            };
            if !matches!(spec.pose, CapturePose::AnimationPlayObstruction { .. }) {
                return true;
            }
            state
                .captures
                .iter()
                .find(|capture| &capture.view == view)
                .is_some_and(|capture| {
                    capture.camera_obstruction_hit == Some(true)
                        && capture
                            .camera_boom_resolved_metres
                            .zip(capture.camera_boom_desired_metres)
                            .is_some_and(|(resolved, desired)| resolved + 0.05 < desired)
                })
        }),
        vista_tree_near_field_bounded: true,
        tree_cold_traversal_canopy_continuous: true,
        production_lighting_parity: presentation_features.requested_matches_observed,
        lighting_readiness: state.captures.iter().all(|capture| capture.lighting_ready),
        all_views_render_content: false,
        foliage_detail_present: false,
        all_obstacles_presented: presented_trees == state.expected_trees
            && presented_rocks == state.expected_rocks,
        all_obstacles_collidable: collider_trees == state.expected_trees
            && collider_rocks == state.expected_rocks,
        procedural_rocks_fit_colliders: rock_meshes_inside_colliders,
        trees_have_five_lods: presented_trees == state.expected_trees,
        tree_detail_captured_when_expected: state.expected_trees == 0
            || state.captures.iter().all(|capture| {
                state
                    .views
                    .iter()
                    .find(|spec| spec.slug == capture.view)
                    .is_some_and(|spec| {
                        spec.validated_forced_lod.is_none_or(|expected| {
                            capture.forced_tree_lod == Some(expected)
                                && capture.focused_tree_lod_queued == Some(true)
                        })
                    })
            }),
        recursive_tree_lod_observed: state.profile != "semantic"
            || state.expected_trees == 0
            || !state
                .requested_views
                .iter()
                .any(|view| view == "tree-recursive-lod")
            || (recursive_tree_lod.primary_cluster_count >= 2
                && recursive_tree_lod.mixed_lods_observed),
        terrain_material_present: terrain_materials.iter().count() == 1,
        coarse_source_terrain_upsampled: state.terrain.source_spacing_metres <= 2.0
            || (state.terrain.spacing_metres <= 2.0
                && state.terrain.generated_samples > state.terrain.source_samples),
        microrelief_present: state.repairs.microrelief_adjusted_samples > 0,
        grass_present_when_expected: !state.expects_grass || grass_clumps > 0,
        forest_floor_scatter_present_when_trees: state.expected_trees == 0
            || (dry_leaf_patches > 0 && twig_patches > 0),
        understory_present_when_expected: !expects_understory || understory_clumps > 0,
        loose_stone_scatter_present_when_expected: state.fixture != "steep-open-hillside"
            || loose_stone_patches > 0,
        vista_has_three_lods: vista_summary.presented_lods.len() >= 3,
        vista_reaches_fifty_kilometres: vista_summary.diameter_metres >= 50_000.0,
        vista_has_no_colliders: vista_colliders == 0,
        precipitation_particles_present_when_expected: !expects_precipitation
            || weather_particle_count > 0,
        fixture_feature_expectation_met,
        passed: false,
        note: "Semantic gates are automatic; beauty, scale, composition, and visual artifacts require image inspection.",
    };
    validation.passed = validation_passes(&validation);
    PendingCaptureManifest::new(CaptureManifest {
        pipeline: "tactical_scene_native_capture_v6",
        fixture: state.fixture.clone(),
        source_input: state.input_path.display().to_string(),
        scene_digest: state.digest.clone(),
        seed: state.seed,
        absolute_minute: state.absolute_minute,
        canopy_bps: state.canopy_bps,
        generation_version: state.generation_version,
        scene_source: state.scene_source.clone(),
        capture_profile: state.profile.clone(),
        capture_profile_version: CAPTURE_PROFILE_VERSION,
        camera_version: CAMERA_VERSION,
        requested_views: state.requested_views.clone(),
        settle_frames: state.settle_frames,
        resolution: [VIEW_WIDTH, VIEW_HEIGHT],
        review_azimuth_degrees: state.tree_review_azimuth_degrees,
        capture_clock_strategy: "Bevy virtual clock advanced to a fixed phase after startup, then paused through settling and GPU readback",
        capture_clock_phase_seconds: CAPTURE_CLOCK_PHASE_SECONDS,
        renderer: "Bevy/wgpu production tactical presentation",
        executable_version: env!("CARGO_PKG_VERSION"),
        revision: capture_revision(),
        source_identity: std::env::var("CAPTURE_SOURCE_IDENTITY")
            .unwrap_or_else(|_| "standalone-unlabelled".into()),
        celestial: capture_celestial(
            state.absolute_minute,
            state.latitude_microdegrees,
            state.longitude_microdegrees,
        ),
        presentation_features,
        weather: state.weather,
        repairs: state.repairs,
        terrain: state.terrain,
        obstacles: obstacle_summary,
        foliage: foliage_summary,
        tree_impostor_bakes,
        recursive_tree_lod,
        vista: vista_summary,
        weather_particle_count,
        captures: state.captures.clone(),
        validation,
    })
}

fn capture_revision() -> String {
    ["GITHUB_SHA", "RENDER_GIT_COMMIT", "SOURCE_REVISION"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|revision| revision.trim().to_owned())
                .filter(|revision| !revision.is_empty())
        })
        .unwrap_or_else(|| "unavailable (set SOURCE_REVISION for capture provenance)".into())
}

fn capture_celestial(
    absolute_minute: u64,
    latitude_microdegrees: i32,
    longitude_microdegrees: i32,
) -> CelestialProvenance {
    let celestial = celestial_directions(
        absolute_minute,
        latitude_microdegrees,
        longitude_microdegrees,
    );
    CelestialProvenance {
        sun_altitude_degrees: celestial.sun[1].asin().to_degrees(),
        moon_altitude_degrees: celestial.moon[1].asin().to_degrees(),
        lunar_illumination: celestial.lunar_illumination,
    }
}

fn tree_lod_camera(state: &CaptureState, distance: f32) -> (Vec3, Vec3, Vec3) {
    state.tree_focus.map_or(
        (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
        |tree| {
            let diagonal = distance * 1.55 * core::f32::consts::FRAC_1_SQRT_2;
            (
                tree + Vec3::new(diagonal, 7.55, diagonal),
                tree + Vec3::new(0.0, 7.0, 0.0),
                Vec3::Y,
            )
        },
    )
}

fn tree_cold_traversal_camera(state: &CaptureState, distance: f32) -> (Vec3, Vec3, Vec3) {
    state.tree_focus.map_or(
        (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
        |tree| {
            let horizontal = distance.max(0.75) * core::f32::consts::FRAC_1_SQRT_2;
            (
                tree + Vec3::new(horizontal, 7.55, horizontal),
                tree + Vec3::new(0.0, 7.0, 0.0),
                Vec3::Y,
            )
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DebrisCaptureTarget {
    focus: Vec3,
    camera: Vec3,
    leaf_distance_metres: f32,
    twig_distance_metres: f32,
}

fn debris_capture_target(
    leaf_positions: &[Vec3],
    twig_positions: &[Vec3],
    maximum_pair_distance_metres: f32,
) -> Option<DebrisCaptureTarget> {
    let mut candidates = leaf_positions
        .iter()
        .flat_map(|leaf| twig_positions.iter().map(move |twig| (*leaf, *twig)))
        .filter_map(|(leaf, twig)| {
            let distance = leaf.xz().distance(twig.xz());
            (distance <= maximum_pair_distance_metres).then_some((distance, leaf, twig))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.x.total_cmp(&right.1.x))
            .then_with(|| left.1.z.total_cmp(&right.1.z))
            .then_with(|| left.2.x.total_cmp(&right.2.x))
            .then_with(|| left.2.z.total_cmp(&right.2.z))
    });
    let (_, leaf, twig) = candidates.first().copied()?;
    let focus = leaf.lerp(twig, 0.5);
    Some(DebrisCaptureTarget {
        focus,
        camera: Vec3::ZERO,
        leaf_distance_metres: focus.xz().distance(leaf.xz()),
        twig_distance_metres: focus.xz().distance(twig.xz()),
    })
}

fn terrain_snapped_debris_capture_target(
    leaf_positions: &[Vec3],
    twig_positions: &[Vec3],
    maximum_pair_distance_metres: f32,
    terrain: &SceneTerrain,
) -> Option<DebrisCaptureTarget> {
    let mut target =
        debris_capture_target(leaf_positions, twig_positions, maximum_pair_distance_metres)?;
    let terrain_height = terrain.height_at(target.focus.xz())?;
    target.focus.y = target.focus.y.max(terrain_height + 0.001);
    Some(target)
}

fn reviewable_debris_capture_target(
    pairs: &[GroundLitterCapturePair],
    terrain: &SceneTerrain,
    obstacles: &[(Vec3, f32)],
    fallback_azimuth_degrees: f32,
) -> Option<DebrisCaptureTarget> {
    let half_width = terrain.width() * 0.5;
    let half_depth = terrain.depth() * 0.5;
    let mut candidates = pairs
        .iter()
        .filter_map(|pair| {
            let mut target = terrain_snapped_debris_capture_target(
                &[pair.dry_leaf],
                &[pair.twig],
                0.55,
                terrain,
            )?;
            let normal = terrain.normal_at(target.focus.xz())?;
            let edge_clearance =
                (half_width - target.focus.x.abs()).min(half_depth - target.focus.z.abs());
            if normal.y < 0.82 || edge_clearance < 1.8 {
                return None;
            }
            let nearest = obstacles.iter().min_by(|left, right| {
                target
                    .focus
                    .xz()
                    .distance(left.0.xz())
                    .total_cmp(&target.focus.xz().distance(right.0.xz()))
            });
            let obstacle_clearance = nearest
                .map_or(half_width.min(half_depth), |(position, radius)| {
                    target.focus.xz().distance(position.xz()) - radius
                });
            if obstacle_clearance < 1.25 {
                return None;
            }
            let fallback = fallback_azimuth_degrees.to_radians();
            let camera_horizontal = nearest
                .map(|(position, _)| (target.focus.xz() - position.xz()).normalize_or_zero())
                .filter(|direction| direction.length_squared() > 0.5)
                .unwrap_or(Vec2::new(fallback.sin(), fallback.cos()));
            let camera_xz = target.focus.xz() + camera_horizontal * 1.15;
            let camera_ground = terrain.height_at(camera_xz)?;
            target.camera = Vec3::new(
                camera_xz.x,
                (target.focus.y + 1.35).max(camera_ground + 1.0),
                camera_xz.y,
            );
            let score = obstacle_clearance.min(8.0) * 4.0 + edge_clearance.min(8.0) + normal.y;
            Some((score, target))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.focus.x.total_cmp(&right.1.focus.x))
            .then_with(|| left.1.focus.z.total_cmp(&right.1.focus.z))
    });
    candidates.first().map(|(_, target)| *target)
}

fn debris_detail_camera(
    target: Vec3,
    camera: Option<Vec3>,
    azimuth_degrees: f32,
) -> (Vec3, Vec3, Vec3) {
    let azimuth = azimuth_degrees.to_radians();
    (
        camera.unwrap_or(target + Vec3::new(azimuth.sin() * 1.15, 1.35, azimuth.cos() * 1.15)),
        target,
        Vec3::Y,
    )
}

fn finish_capture(
    output: &Path,
    manifest: &CaptureManifest,
    valid: bool,
    exit: &mut MessageWriter<AppExit>,
) {
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(manifest).expect("capture manifest serializes"),
    )
    .unwrap_or_else(|error| panic!("failed to write capture manifest: {error}"));
    fs::write(output.join("index.html"), capture_index(manifest))
        .unwrap_or_else(|error| panic!("failed to write capture index: {error}"));
    if valid {
        println!("TACTICAL_SCENE_CAPTURE_VALID={}", output.display());
        exit.write(AppExit::Success);
    } else {
        let reason = if !manifest.validation.production_lighting_parity {
            "Production lighting readiness failed: requested presentation components/settings were unavailable or mismatched.\n"
        } else if !manifest.validation.lighting_readiness {
            "Production lighting readiness failed: consecutive settled readbacks did not reach bounded luminance stability.\n"
        } else {
            "Tactical scene capture validation failed; inspect manifest.json and screenshots.\n"
        };
        fs::write(output.join("failure.txt"), reason)
            .unwrap_or_else(|error| panic!("failed to write capture failure marker: {error}"));
        exit.write(AppExit::error());
    }
}

fn capture_index(manifest: &CaptureManifest) -> String {
    let cards = manifest
        .captures
        .iter()
        .map(|capture| {
            format!(
                "<figure><img src=\"{}\" alt=\"{}\"><figcaption><strong>{}</strong><br>{}</figcaption></figure>",
                capture.screenshot, capture.label, capture.view, capture.label
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>{fixture} tactical capture</title>\
         <style>body{{margin:2rem;background:#111820;color:#edf2f4;font:16px system-ui}}\
         main{{display:grid;grid-template-columns:repeat(auto-fit,minmax(480px,1fr));gap:1.5rem}}\
         figure{{margin:0;background:#202a34;padding:1rem;border-radius:.5rem}}\
         img{{width:100%;height:auto}}a{{color:#8dd6ff}}</style>\
         <h1>{fixture}</h1><p>Digest <code>{digest}</code> Â· <a href=\"manifest.json\">manifest</a> Â· \
         validation: <strong>{passed}</strong></p><main>{cards}</main>",
        fixture = manifest.fixture,
        digest = manifest.scene_digest,
        passed = if manifest.validation.passed {
            "passed"
        } else {
            "FAILED"
        },
    )
}
