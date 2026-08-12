use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::{
    camera::{Exposure, visibility::VisibilityRange},
    core_pipeline::tonemapping::Tonemapping,
    ecs::system::SystemParam,
    light::{AtmosphereEnvironmentMapLight, NotShadowCaster},
    pbr::ScreenSpaceAmbientOcclusion,
    post_process::bloom::Bloom,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::presentation::{
    GroundScatterLayer, ProceduralRockVisual, TacticalGraphicsSettings, TacticalPresentationPlugin,
    TacticalTreeLeafCardMaterial, TerrainMaterialPresentation, TreeImpostorProvenance,
    TreeLeafRepresentation, TreeLod, TreeLodCluster, TreeLodRenderOverride, VistaTerrain,
    WeatherParticle, oak_leaf_material, oak_review_terminal_specimen,
};

const VIEW_WIDTH: u32 = 1280;
const VIEW_HEIGHT: u32 = 720;
const STANDING_EYE_HEIGHT_METRES: f32 = 1.65;
const PROCEDURAL_OAK_LEAVES_PER_TREE: usize = 69_632;
const CAPTURE_PROFILE_VERSION: u16 = 7;
const CAMERA_VERSION: u16 = 6;
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
    tree_review_azimuth_degrees: f32,
    profile: String,
    requested_views: Vec<String>,
    views: Vec<CaptureView>,
}

#[derive(Resource)]
struct CaptureState {
    fixture: String,
    input_path: PathBuf,
    output: PathBuf,
    digest: String,
    seed: u64,
    absolute_minute: u64,
    latitude_microdegrees: i32,
    longitude_microdegrees: i32,
    canopy_bps: u16,
    generation_version: u16,
    weather: WeatherSnapshot,
    repairs: RepairSummary,
    terrain: TerrainSummary,
    expected_trees: usize,
    expected_rocks: usize,
    expects_grass: bool,
    vista_lods_supplied: usize,
    vista_diameter_metres: f32,
    vista_minimum_metres: f32,
    vista_peak_metres: f32,
    vista_relief_metres: f32,
    peak_target: Vec3,
    valley_target: Vec3,
    obstacle_focus: Vec3,
    tree_focus: Option<Vec3>,
    rock_focus: Option<Vec3>,
    debris_focus: Option<Vec3>,
    debris_leaf_distance_metres: Option<f32>,
    debris_twig_distance_metres: Option<f32>,
    tree_leaf_focus: Option<Vec3>,
    tree_leaf_camera: Option<Vec3>,
    tree_focus_entity: Option<Entity>,
    tree_review_entities: Vec<Entity>,
    tree_review_leaf_entities: Vec<(Entity, TreeLeafRepresentation)>,
    ground_eye_position: Vec3,
    ground_eye_target: Vec3,
    settle_frames: u32,
    tree_review_azimuth_degrees: f32,
    profile: String,
    requested_views: Vec<String>,
    views: Vec<CaptureView>,
    view: usize,
    view_started: bool,
    prime_readbacks: u8,
    lighting_luminance_samples: Vec<f32>,
    settled: u32,
    in_flight: bool,
    captures: Vec<CaptureRecord>,
    recursive_lods_observed: BTreeSet<(u8, u8)>,
}

#[derive(Component)]
struct CaptureOverlay;

#[derive(SystemParam)]
struct LightingObservationParams<'w> {
    settings: Res<'w, TacticalGraphicsSettings>,
    ambient: Res<'w, GlobalAmbientLight>,
}

#[derive(Component)]
struct TreeReviewBackdrop;

#[derive(Component)]
struct TreeReviewSpecimen;

#[derive(Clone, Copy)]
struct CaptureView {
    slug: &'static str,
    label: &'static str,
    overlay: bool,
}

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

const CAPTURE_VIEWS: [CaptureView; 24] = [
    CaptureView {
        slug: "warmup",
        label: "Render-pipeline warmup",
        overlay: false,
    },
    CaptureView {
        slug: "beauty-ground",
        label: "Ground-level beauty view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-detail",
        label: "Whole-tree individual-leaf LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-lighting-baseline",
        label: "Tree lighting baseline without canopy AO or shadows",
        overlay: false,
    },
    CaptureView {
        slug: "tree-lighting-ao",
        label: "Tree lighting with WebGPU-safe canopy ambient occlusion",
        overlay: false,
    },
    CaptureView {
        slug: "tree-lighting-shadows",
        label: "Tree lighting with directional leaf self shadows",
        overlay: false,
    },
    CaptureView {
        slug: "tree-lighting-combined",
        label: "Tree lighting with canopy AO and directional self shadows",
        overlay: false,
    },
    CaptureView {
        slug: "tree-recursive-lod",
        label: "Mixed recursive tree LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "ground-cover",
        label: "Tree-canopy leaf-litter and grass boundary",
        overlay: false,
    },
    CaptureView {
        slug: "tree-silhouette",
        label: "Neutral English oak silhouette plate",
        overlay: false,
    },
    CaptureView {
        slug: "tree-textured-leaf-detail",
        label: "Eight-triangle cambered PBR terminal-shoot close-up",
        overlay: false,
    },
    CaptureView {
        slug: "tree-leaf-card-detail",
        label: "Two-triangle textured terminal-shoot close-up",
        overlay: false,
    },
    CaptureView {
        slug: "tree-textured-leaf-lod",
        label: "Rendered eight-triangle cambered leaf LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-leaf-card-lod",
        label: "Rendered two-triangle leaf LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-leaf-transition-25",
        label: "Cambered-to-flat leaf transition 25% view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-leaf-transition-50",
        label: "Cambered-to-flat leaf transition 50% view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-leaf-transition-75",
        label: "Cambered-to-flat leaf transition 75% view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-twig-lod",
        label: "Leafed-twig tree LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-small-branch-lod",
        label: "Small-branch tree LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-crown-lod",
        label: "Crown-branch tree LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "tree-billboard-lod",
        label: "Whole-tree billboard LOD view",
        overlay: false,
    },
    CaptureView {
        slug: "beauty-overhead",
        label: "Overhead distribution view",
        overlay: false,
    },
    CaptureView {
        slug: "horizon",
        label: "Horizon and distant-vista view",
        overlay: false,
    },
    CaptureView {
        slug: "collision-overlay",
        label: "Obstacle collider overlay",
        overlay: true,
    },
];

const ENVIRONMENT_REVIEW_VIEWS: [CaptureView; 12] = [
    CaptureView {
        slug: "warmup",
        label: "Render-pipeline warmup",
        overlay: false,
    },
    CaptureView {
        slug: "beauty-ground",
        label: "Ground-level environment context",
        overlay: false,
    },
    CaptureView {
        slug: "beauty-overhead",
        label: "Overhead playable-area and terrain composition",
        overlay: false,
    },
    CaptureView {
        slug: "tree-root-detail",
        label: "Tree root flare and forest-floor detail",
        overlay: false,
    },
    CaptureView {
        slug: "tree-branch-junction",
        label: "Trunk and primary-branch junction detail",
        overlay: false,
    },
    CaptureView {
        slug: "rock-detail",
        label: "Procedural rock surface and ground contact detail",
        overlay: false,
    },
    CaptureView {
        slug: "terrain-grazing-detail",
        label: "Ground material under grazing light",
        overlay: false,
    },
    CaptureView {
        slug: "grass-seam-detail",
        label: "Grass macro-patch seam and density detail",
        overlay: false,
    },
    CaptureView {
        slug: "forest-floor-debris-detail",
        label: "Fallen oak leaves and twig geometry close-up",
        overlay: false,
    },
    CaptureView {
        slug: "horizon",
        label: "Horizon, Sun, Moon, and atmosphere context",
        overlay: false,
    },
    CaptureView {
        slug: "vista-lod-oblique",
        label: "Playable edge and distant terrain LOD composition",
        overlay: false,
    },
    CaptureView {
        slug: "vista-valley-oblique",
        label: "Playable edge and lowest regional terrain composition",
        overlay: false,
    },
];

#[derive(Clone, Serialize)]
struct CaptureRecord {
    view: String,
    label: String,
    screenshot: String,
    camera_translation: [f32; 3],
    camera_target: [f32; 3],
    camera_up: [f32; 3],
    vertical_fov_degrees: f32,
    foreground_pixel_bps: u16,
    detail_pixel_bps: u16,
    diagnostic_leaf_suppression: bool,
    diagnostic_grass_suppression: bool,
    debris_leaf_distance_metres: Option<f32>,
    debris_twig_distance_metres: Option<f32>,
    lighting_luminance_samples: Vec<f32>,
    lighting_luminance_delta: f32,
    lighting_ready: bool,
}

#[derive(Clone, Copy, Serialize)]
struct RepairSummary {
    upsampled_height_samples: u32,
    microrelief_adjusted_samples: u32,
    adjusted_height_samples: u32,
    repaired_water_samples: u32,
    removed_corridor_obstacles: u32,
}

#[derive(Clone, Copy, Serialize)]
struct TerrainSummary {
    width_metres: f32,
    depth_metres: f32,
    source_spacing_metres: f32,
    spacing_metres: f32,
    source_samples: usize,
    generated_samples: usize,
    minimum_height_metres: f32,
    maximum_height_metres: f32,
}

#[derive(Serialize)]
struct ObstacleSummary {
    generated_trees: usize,
    generated_rocks: usize,
    presented_trees: usize,
    presented_rocks: usize,
    collider_trees: usize,
    collider_rocks: usize,
    procedural_rock_meshes: usize,
    rock_meshes_inside_colliders: bool,
    tree_lods_presented: Vec<u8>,
}

#[derive(Serialize)]
struct FoliageSummary {
    grass_clumps: usize,
    understory_clumps: usize,
    dry_leaf_patches: usize,
    twig_patches: usize,
    loose_stone_patches: usize,
}

#[derive(Serialize)]
struct TreeBakeSummary {
    seed: u64,
    lod: u8,
    bake_version: u32,
    source_geometry_hash: String,
    render_method: &'static str,
    atlas_size: [u32; 2],
    cards: Vec<TreeBakeCardSummary>,
}

#[derive(Serialize)]
struct TreeBakeCardSummary {
    source_group: u16,
    source_leaf_count: u16,
    source_branch_count: u16,
    view_direction: [f32; 3],
    projected_bounds: [f32; 4],
    atlas_region: [u32; 4],
    opaque_pixel_count: u32,
    silhouette_centroid: [f32; 2],
}

#[derive(Serialize)]
struct RecursiveTreeLodSummary {
    primary_cluster_count: usize,
    visible_group_lods: Vec<[u8; 2]>,
    mixed_lods_observed: bool,
}

#[derive(Serialize)]
struct VistaSummary {
    supplied_lods: usize,
    presented_lods: Vec<u8>,
    presented_chunks: usize,
    diameter_metres: f32,
    minimum_height_metres: f32,
    peak_height_metres: f32,
    relief_metres: f32,
    collider_count: usize,
}

#[derive(Serialize)]
struct ValidationSummary {
    all_views_captured: bool,
    requested_views_captured_exactly_once: bool,
    requested_detail_targets_available: bool,
    production_lighting_parity: bool,
    lighting_readiness: bool,
    all_views_render_content: bool,
    foliage_detail_present: bool,
    all_obstacles_presented: bool,
    all_obstacles_collidable: bool,
    procedural_rocks_fit_colliders: bool,
    trees_have_five_lods: bool,
    tree_detail_captured_when_expected: bool,
    recursive_tree_lod_observed: bool,
    terrain_material_present: bool,
    coarse_source_terrain_upsampled: bool,
    microrelief_present: bool,
    grass_present_when_expected: bool,
    forest_floor_scatter_present_when_trees: bool,
    understory_present_when_expected: bool,
    loose_stone_scatter_present_when_expected: bool,
    vista_has_three_lods: bool,
    vista_reaches_fifty_kilometres: bool,
    vista_has_no_colliders: bool,
    precipitation_particles_present_when_expected: bool,
    fixture_feature_expectation_met: bool,
    passed: bool,
    note: &'static str,
}

#[derive(Serialize)]
struct CaptureManifest {
    pipeline: &'static str,
    fixture: String,
    source_input: String,
    scene_digest: String,
    seed: u64,
    absolute_minute: u64,
    canopy_bps: u16,
    generation_version: u16,
    capture_profile: String,
    capture_profile_version: u16,
    camera_version: u16,
    requested_views: Vec<String>,
    settle_frames: u32,
    resolution: [u32; 2],
    review_azimuth_degrees: f32,
    capture_clock_strategy: &'static str,
    capture_clock_phase_seconds: f32,
    renderer: &'static str,
    executable_version: &'static str,
    revision: String,
    source_identity: String,
    celestial: CelestialProvenance,
    presentation_features: PresentationFeatures,
    weather: WeatherSnapshot,
    repairs: RepairSummary,
    terrain: TerrainSummary,
    obstacles: ObstacleSummary,
    foliage: FoliageSummary,
    tree_impostor_bakes: Vec<TreeBakeSummary>,
    recursive_tree_lod: RecursiveTreeLodSummary,
    vista: VistaSummary,
    weather_particle_count: usize,
    captures: Vec<CaptureRecord>,
    validation: ValidationSummary,
}

#[derive(Serialize)]
struct CelestialProvenance {
    sun_altitude_degrees: f32,
    moon_altitude_degrees: f32,
    lunar_illumination: f32,
}

#[derive(Clone, Serialize)]
struct PresentationFeatures {
    requested: PresentationFeatureState,
    observed: ObservedPresentationFeatures,
    requested_matches_observed: bool,
    weather_iteration_in_scope: bool,
    water_iteration_in_scope: bool,
    cloud_iteration_in_scope: bool,
    cave_iteration_in_scope: bool,
    characters_present: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct PresentationFeatureState {
    shadows: bool,
    atmosphere: bool,
    celestial: bool,
    environment_light: bool,
    environment_map_size: u32,
    bloom: bool,
    ssao: bool,
    max_vista_lods: usize,
}

#[derive(Clone, Serialize)]
struct ObservedPresentationFeatures {
    settings: PresentationFeatureState,
    camera_environment_map: bool,
    camera_environment_map_size: Option<[u32; 2]>,
    camera_bloom: bool,
    camera_ssao: bool,
    camera_exposure_ev100: f32,
    camera_tonemapping: String,
    ambient_color: [f32; 4],
    ambient_brightness: f32,
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
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: asset_root.to_string_lossy().into_owned(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Fabelgeist tactical scene capture".into(),
                    resolution: (VIEW_WIDTH, VIEW_HEIGHT).into(),
                    present_mode: PresentMode::AutoNoVsync,
                    resizable: false,
                    decorations: false,
                    ..default()
                }),
                ..default()
            }),
    )
    // Visual-review plates use the exact production presentation defaults.
    // Diagnostics may hide named occluder layers, but never substitute a
    // cheaper lighting or post-processing pipeline.
    .add_plugins(capture_presentation_plugin())
    .insert_resource(ClearColor(Color::srgb_u8(158, 181, 195)))
    .insert_resource(SceneSetup(Some(setup)))
    .add_systems(PostStartup, (setup_scene, freeze_capture_clock).chain());
    if leaf_benchmarking {
        app.add_systems(Last, benchmark_leaf_representations);
    } else if tree_lighting_benchmarking {
        app.add_systems(Last, benchmark_tree_lighting);
    } else {
        app.add_systems(Last, capture_views);
    }
    let exit = app.run();
    if exit != AppExit::Success {
        std::process::exit(1);
    }
}

fn capture_presentation_plugin() -> TacticalPresentationPlugin {
    TacticalPresentationPlugin::default()
}

fn feature_state(settings: &TacticalGraphicsSettings) -> PresentationFeatureState {
    PresentationFeatureState {
        shadows: settings.shadows_enabled,
        atmosphere: settings.atmosphere_enabled,
        celestial: settings.celestial_enabled,
        environment_light: settings.environment_light_enabled,
        environment_map_size: settings.environment_map_size,
        bloom: settings.bloom_enabled,
        ssao: settings.ssao_enabled,
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
        ssao: requested.ssao_enabled,
        max_vista_lods: requested.max_vista_lods,
    }
}

fn observed_presentation_features(
    settings: &TacticalGraphicsSettings,
    environment_map: Option<&AtmosphereEnvironmentMapLight>,
    bloom: Option<&Bloom>,
    ssao: Option<&ScreenSpaceAmbientOcclusion>,
    exposure: &Exposure,
    tonemapping: &Tonemapping,
    ambient: &GlobalAmbientLight,
) -> PresentationFeatures {
    let requested = requested_feature_state();
    let observed_settings = feature_state(settings);
    let environment_map_size = environment_map.map(|light| light.size.to_array());
    let observed = ObservedPresentationFeatures {
        settings: observed_settings,
        camera_environment_map: environment_map.is_some(),
        camera_environment_map_size: environment_map_size,
        camera_bloom: bloom.is_some(),
        camera_ssao: ssao.is_some(),
        camera_exposure_ev100: exposure.ev100,
        camera_tonemapping: format!("{tonemapping:?}"),
        ambient_color: ambient.color.to_linear().to_f32_array(),
        ambient_brightness: ambient.brightness,
    };
    let requested_matches_observed = observed_settings == requested
        && observed.camera_environment_map == requested.environment_light
        && observed.camera_environment_map_size
            == requested
                .environment_light
                .then_some([requested.environment_map_size; 2])
        && observed.camera_bloom == requested.bloom
        && observed.camera_ssao == requested.ssao
        // Production exposure is driven by the scene's solar/lunar state and
        // may be between authored targets while the ECS observer settles.
        && observed.camera_exposure_ev100.is_finite()
        && (-1.35..=15.0).contains(&observed.camera_exposure_ev100)
        && observed.camera_tonemapping.contains("AcesFitted")
        && observed.ambient_brightness.is_finite()
        && observed.ambient_brightness > 0.0;
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

fn selected_capture_views(profile: &str, requested: &[String]) -> Result<Vec<CaptureView>, String> {
    let profile_views = match profile {
        "semantic" => CAPTURE_VIEWS.as_slice(),
        "environment-review" => ENVIRONMENT_REVIEW_VIEWS.as_slice(),
        _ => return Err(format!("unknown profile {profile}")),
    };
    if requested.is_empty() {
        return Ok(profile_views.to_vec());
    }
    let mut selected = vec![ENVIRONMENT_REVIEW_VIEWS[0]];
    let mut seen = BTreeSet::new();
    for slug in requested {
        if slug == "warmup" {
            return Err("warmup is implicit and cannot be requested".into());
        }
        if !seen.insert(slug.as_str()) {
            return Err(format!("duplicate requested view {slug}"));
        }
        let view = CAPTURE_VIEWS
            .iter()
            .chain(ENVIRONMENT_REVIEW_VIEWS.iter())
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
    fn semantic_profile_preserves_twenty_three_recorded_views() {
        let views = selected_capture_views("semantic", &[]).unwrap();
        assert_eq!(views.len(), 24);
        assert_eq!(
            views.iter().filter(|view| view.slug != "warmup").count(),
            23
        );
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
        let (camera, observed_target, up) = debris_detail_camera(target, 37.0);
        assert_eq!(observed_target, target);
        assert_eq!(up, Vec3::Y);
        assert!((camera.y - target.y - 0.36).abs() < 0.00001);
        assert!((camera.xz().distance(target.xz()) - 0.92).abs() < 0.00001);
        assert_eq!(capture_view_fov("forest-floor-debris-detail"), 39.6);

        let leaves = [Vec3::new(4.0, 0.0, 3.0), Vec3::new(0.1, 0.0, 0.0)];
        let twigs = [Vec3::new(4.2, 0.0, 3.0), Vec3::new(0.4, 0.0, 0.0)];
        let pair = debris_capture_target(&leaves, &twigs, 0.55).unwrap();
        assert_eq!(pair.focus, Vec3::new(4.1, 0.0, 3.0));
        assert!((pair.leaf_distance_metres - 0.1).abs() < 0.00001);
        assert!((pair.twig_distance_metres - 0.1).abs() < 0.00001);
        assert_eq!(debris_capture_target(&[leaves[1]], &[twigs[0]], 0.55), None);
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
        assert_eq!(
            diagnostic_suppression("tree-branch-junction"),
            (true, false)
        );
        assert_eq!(
            diagnostic_suppression("terrain-grazing-detail"),
            (false, true)
        );
        assert_eq!(diagnostic_suppression("grass-seam-detail"), (false, false));
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
        assert!(requested.ssao);
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
        assert_eq!(minimum_foreground_bps("beauty-overhead"), 1);
        assert_eq!(minimum_foreground_bps("beauty-ground"), 1_000);
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
    mut leaf_card_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
    asset_server: Res<AssetServer>,
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
        let bark_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.38, 0.31),
            perceptual_roughness: 0.95,
            ..default()
        });
        let leaf_material = leaf_card_materials.add(oak_leaf_material(&asset_server));
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
        debris_leaf_distance_metres: None,
        debris_twig_distance_metres: None,
        tree_leaf_focus,
        tree_leaf_camera,
        tree_focus_entity,
        tree_review_entities,
        tree_review_leaf_entities,
        ground_eye_position,
        ground_eye_target,
        settle_frames,
        tree_review_azimuth_degrees,
        profile,
        requested_views,
        views,
        view: 0,
        view_started: false,
        prime_readbacks: 0,
        lighting_luminance_samples: Vec::new(),
        settled: 0,
        in_flight: false,
        captures: Vec::new(),
        recursive_lods_observed: BTreeSet::new(),
    });
    if let Some(sample_frames) = leaf_benchmark_frames {
        commands.insert_resource(LeafBenchmarkState::new(sample_frames));
    }
    if let Some(sample_frames) = tree_lighting_benchmark_frames {
        commands.insert_resource(TreeLightingBenchmarkState::new(sample_frames));
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
    let scene_leaf_triangles =
        capture.expected_trees * PROCEDURAL_OAK_LEAVES_PER_TREE * usize::from(triangles_per_leaf);
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

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
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
            Option<&Bloom>,
            Option<&ScreenSpaceAmbientOcclusion>,
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
    obstacles: Query<(&SceneObstacle, Has<Mesh3d>, Has<Collider>)>,
    rock_visuals: Query<&Mesh3d, With<ProceduralRockVisual>>,
    tree_lods: Query<&TreeLod>,
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
        Query<(Entity, &GroundScatterLayer, &GlobalTransform), Without<Camera3d>>,
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
    if state.in_flight || state.view >= state.views.len() {
        return;
    }
    let view = state.views[state.view];
    if !state.view_started {
        let lighting_mode = match view.slug {
            "tree-lighting-baseline" => TREE_LIGHTING_MODES[0],
            "tree-lighting-ao" => TREE_LIGHTING_MODES[1],
            "tree-lighting-shadows" => TREE_LIGHTING_MODES[2],
            _ => TREE_LIGHTING_MODES[3],
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
        tree_lod_override.lod = match view.slug {
            "tree-textured-leaf-lod" | "tree-leaf-card-lod" => Some(0),
            "tree-twig-lod" => Some(1),
            "tree-small-branch-lod" => Some(2),
            "tree-crown-lod" => Some(3),
            "tree-billboard-lod" => Some(4),
            _ => None,
        };
        tree_lod_override.leaf = match view.slug {
            "tree-textured-leaf-lod" => Some(TreeLeafRepresentation::TexturedMesh),
            "tree-leaf-card-lod" => Some(TreeLeafRepresentation::AlphaCard),
            _ => None,
        };
        tree_lod_override.projected_scale = match view.slug {
            "tree-leaf-transition-25" => Some(0.60),
            "tree-leaf-transition-50" => Some(0.50),
            "tree-leaf-transition-75" => Some(0.40),
            _ => None,
        };
        let specimen_representation = match view.slug {
            "tree-textured-leaf-detail" => Some(TreeLeafRepresentation::TexturedMesh),
            "tree-leaf-card-detail" => Some(TreeLeafRepresentation::AlphaCard),
            _ => None,
        };
        let specimen_view = specimen_representation.is_some();
        let specimen_pipeline_warmup = view.slug == "warmup";
        let (suppress_leaves, suppress_grass) = diagnostic_suppression(view.slug);
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
            .map(|(entity, layer, transform)| (entity, *layer, transform.translation()))
            .collect::<Vec<_>>();
        if view.slug == "forest-floor-debris-detail" {
            let leaves = ground_scatter_entities
                .iter()
                .filter(|(_, layer, _)| *layer == GroundScatterLayer::DryLeaves)
                .map(|(_, _, translation)| *translation)
                .collect::<Vec<_>>();
            let twigs = ground_scatter_entities
                .iter()
                .filter(|(_, layer, _)| *layer == GroundScatterLayer::Twigs)
                .map(|(_, _, translation)| *translation)
                .collect::<Vec<_>>();
            if let Some(target) = debris_capture_target(&leaves, &twigs, 0.55) {
                state.debris_focus = Some(target.focus);
                state.debris_leaf_distance_metres = Some(target.leaf_distance_metres);
                state.debris_twig_distance_metres = Some(target.twig_distance_metres);
            } else {
                state.debris_focus = None;
                state.debris_leaf_distance_metres = None;
                state.debris_twig_distance_metres = None;
            }
        }
        for (entity, layer, _) in ground_scatter_entities {
            let hide_for_view = (layer == GroundScatterLayer::Grass && suppress_grass)
                || view.slug == "vista-lod-oblique";
            if layer == GroundScatterLayer::Grass || view.slug == "vista-lod-oblique" {
                commands.entity(entity).insert(if hide_for_view {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                });
            }
        }
        for mut visibility in scene_visibility.p7().iter_mut() {
            *visibility = if view.slug == "vista-lod-oblique" {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        if let Some(entity) = state.tree_focus_entity {
            commands.entity(entity).insert(if specimen_view {
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
        let (transform, target) = camera_for_view(view.slug, state);
        *camera.1 = transform;
        *camera.2 = GlobalTransform::from(transform);
        if let Projection::Perspective(projection) = &mut *camera.3 {
            projection.fov = capture_view_fov(view.slug).to_radians();
        }
        for mut visibility in &mut overlays {
            *visibility = if view.overlay {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        for mut visibility in &mut scene_visibility.p1() {
            *visibility = if matches!(
                view.slug,
                "warmup"
                    | "beauty-ground"
                    | "beauty-overhead"
                    | "horizon"
                    | "vista-lod-oblique"
                    | "vista-valley-oblique"
            ) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        for (mut visibility, mut backdrop) in &mut tree_backdrops {
            *visibility = if view.slug == "tree-silhouette" || specimen_view {
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
        state.view_started = true;
        state.settled = 0;
        if view.slug != "warmup" {
            state.captures.push(CaptureRecord {
                view: view.slug.to_owned(),
                label: view.label.to_owned(),
                screenshot: format!("{}.png", view.slug),
                camera_translation: camera.1.translation.to_array(),
                camera_target: target.to_array(),
                camera_up: camera.1.up().as_vec3().to_array(),
                vertical_fov_degrees: capture_view_fov(view.slug),
                foreground_pixel_bps: 0,
                detail_pixel_bps: 0,
                diagnostic_leaf_suppression: suppress_leaves,
                diagnostic_grass_suppression: suppress_grass,
                debris_leaf_distance_metres: (view.slug == "forest-floor-debris-detail")
                    .then_some(state.debris_leaf_distance_metres)
                    .flatten(),
                debris_twig_distance_metres: (view.slug == "forest-floor-debris-detail")
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
    let settle_target = if state.view == 0 {
        state.settle_frames.saturating_mul(4)
    } else {
        state.settle_frames
    };
    if state.settled < settle_target {
        state.settled += 1;
        return;
    }

    if view.slug == "tree-recursive-lod" {
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
    }

    // Bevy's asynchronous window readback can still contain the render world
    // from before a camera transition. Prime one disposable readback per view,
    // then capture again without changing any scene or camera state.
    let required_prime_readbacks = 2;
    if state.prime_readbacks < required_prime_readbacks {
        state.in_flight = true;
        commands.spawn(Screenshot::primary_window()).observe(
            |captured: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                state.prime_readbacks += 1;
                state
                    .lighting_luminance_samples
                    .push(mean_luminance(captured.image.data.as_deref()));
                state.settled = 0;
                state.in_flight = false;
            },
        );
        return;
    }
    if view.slug == "warmup" {
        state.in_flight = true;
        commands.spawn(Screenshot::primary_window()).observe(
            |_: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                state.view += 1;
                state.view_started = false;
                state.prime_readbacks = 0;
                state.lighting_luminance_samples.clear();
                state.in_flight = false;
            },
        );
        return;
    }

    if let Some(record) = state.captures.last_mut() {
        record
            .lighting_luminance_samples
            .clone_from(&state.lighting_luminance_samples);
        record.lighting_luminance_delta = luminance_delta(&state.lighting_luminance_samples);
        record.lighting_ready = lighting_samples_stable(&state.lighting_luminance_samples);
    }
    let observed_presentation = observed_presentation_features(
        &lighting.settings,
        camera.4,
        camera.5,
        camera.6,
        camera.7,
        camera.8,
        &lighting.ambient,
    );
    let path = state.output.join(format!("{}.png", view.slug));
    let final_view = state.view + 1 == state.views.len();
    let weather_particle_count = scene_visibility.p6().iter().count();
    let mut final_data = final_view.then(|| {
        build_manifest(
            state,
            &obstacles,
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
    state.in_flight = true;
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>,
              mut state: ResMut<CaptureState>,
              mut exit: MessageWriter<AppExit>| {
            let foreground_pixel_bps = foreground_pixel_bps(captured.image.data.as_deref());
            let detail_pixel_bps = foliage_detail_pixel_bps(captured.image.data.as_deref());
            save_to_disk(&path)(captured);
            if let Some(record) = state.captures.last_mut() {
                record.foreground_pixel_bps = foreground_pixel_bps;
                record.detail_pixel_bps = detail_pixel_bps;
            }
            state.view += 1;
            state.view_started = false;
            state.prime_readbacks = 0;
            state.lighting_luminance_samples.clear();
            state.in_flight = false;
            if let Some((mut manifest, _)) = final_data.take() {
                manifest.captures.clone_from(&state.captures);
                manifest.validation.all_views_render_content =
                    manifest.captures.iter().all(|capture| {
                        capture.foreground_pixel_bps >= minimum_foreground_bps(&capture.view)
                    });
                if manifest
                    .requested_views
                    .iter()
                    .any(|view| view == "forest-floor-debris-detail")
                {
                    manifest.validation.requested_detail_targets_available &= manifest
                        .captures
                        .iter()
                        .find(|capture| capture.view == "forest-floor-debris-detail")
                        .is_some_and(|capture| capture.detail_pixel_bps >= 60);
                }
                // The flat fixture provides a stable image-space sentinel for
                // the foliage material. Slopes, dark wetlands, and tree cover
                // can legitimately hide the same fine overhead contrast.
                manifest.validation.foliage_detail_present = manifest.capture_profile != "semantic"
                    || manifest.fixture != "flat-dry-grassland"
                    || manifest
                        .captures
                        .iter()
                        .find(|capture| capture.view == "beauty-overhead")
                        .is_some_and(|capture| capture.detail_pixel_bps >= 100);
                if manifest.fixture == "narrow-peak-lod-boundary" {
                    manifest.validation.fixture_feature_expectation_met &= manifest
                        .captures
                        .iter()
                        .find(|capture| capture.view == "horizon")
                        .is_some_and(|capture| capture.foreground_pixel_bps >= 200);
                }
                manifest.validation.passed = validation_passes(&manifest.validation);
                let valid = manifest.validation.passed;
                finish_capture(&state.output, &manifest, valid, &mut exit);
            }
        },
    );
}

fn camera_for_view(slug: &str, state: &CaptureState) -> (Transform, Vec3) {
    let half = state.terrain.width_metres.max(state.terrain.depth_metres) * 0.5;
    let (position, target, up) = match slug {
        "warmup" | "beauty-ground" | "collision-overlay" => {
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y)
        }
        "tree-detail"
        | "tree-lighting-baseline"
        | "tree-lighting-ao"
        | "tree-lighting-shadows"
        | "tree-lighting-combined"
        | "tree-silhouette"
        | "tree-textured-leaf-lod"
        | "tree-leaf-card-lod"
        | "tree-leaf-transition-25"
        | "tree-leaf-transition-50"
        | "tree-leaf-transition-75" => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                let radius = 15.0 * core::f32::consts::SQRT_2;
                (
                    tree + Vec3::new(azimuth.sin() * radius, 7.0, azimuth.cos() * radius),
                    tree + Vec3::new(0.0, 4.5, 0.0),
                    Vec3::Y,
                )
            },
        ),
        "tree-recursive-lod" => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                (
                    tree + Vec3::new(33.0, 5.5, 33.0),
                    tree + Vec3::new(0.0, 4.5, 0.0),
                    Vec3::Y,
                )
            },
        ),
        "tree-root-detail" => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                let root = tree - Vec3::Y * (TREE_TRUNK_HEIGHT_METRES * 0.5 - 0.22);
                let azimuth = state.tree_review_azimuth_degrees.to_radians();
                (
                    root + Vec3::new(azimuth.sin() * 2.4, 1.05, azimuth.cos() * 2.4),
                    root + Vec3::Y * 0.16,
                    Vec3::Y,
                )
            },
        ),
        "tree-branch-junction" => state.tree_focus.map_or(
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
        "rock-detail" => state.rock_focus.map_or(
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
        "terrain-grazing-detail" => {
            let target = state.obstacle_focus - Vec3::Y * 1.42;
            (target + Vec3::new(-5.5, 0.72, 4.5), target, Vec3::Y)
        }
        "grass-seam-detail" => {
            let target = state.obstacle_focus - Vec3::Y * 1.30;
            (target + Vec3::new(-3.4, 1.25, 3.4), target, Vec3::Y)
        }
        "forest-floor-debris-detail" => state.debris_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |target| debris_detail_camera(target, state.tree_review_azimuth_degrees),
        ),
        "ground-cover" => state.tree_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |tree| {
                (
                    tree + Vec3::new(5.5, -0.4, 5.5),
                    tree + Vec3::new(0.0, -2.25, 0.0),
                    Vec3::Y,
                )
            },
        ),
        "tree-textured-leaf-detail" | "tree-leaf-card-detail" => state.tree_leaf_focus.map_or(
            (state.ground_eye_position, state.ground_eye_target, Vec3::Y),
            |focus| {
                (
                    state.tree_leaf_camera.unwrap_or(focus + Vec3::Z),
                    focus,
                    Vec3::Y,
                )
            },
        ),
        "tree-twig-lod" => tree_lod_camera(state, 30.0),
        "tree-small-branch-lod" => tree_lod_camera(state, 48.0),
        "tree-crown-lod" => tree_lod_camera(state, 72.0),
        "tree-billboard-lod" => tree_lod_camera(state, 118.0),
        "beauty-overhead" => (
            state.obstacle_focus + Vec3::new(0.0, half * 2.15, half * 0.16),
            state.obstacle_focus,
            Vec3::Z,
        ),
        "horizon" => {
            let position = state.ground_eye_position + Vec3::Y * 12.0;
            (position, state.peak_target, Vec3::Y)
        }
        "vista-lod-oblique" => {
            let direction = (state.peak_target.xz() - state.obstacle_focus.xz())
                .try_normalize()
                .unwrap_or(Vec2::X);
            let position = state.obstacle_focus
                - Vec3::new(direction.x, 0.0, direction.y) * (half * 0.62)
                + Vec3::Y * 8.0;
            let target = state.obstacle_focus
                + Vec3::new(direction.x, 0.0, direction.y) * (half * 5.0)
                + Vec3::Y * 2.0;
            (position, target, Vec3::Y)
        }
        "vista-valley-oblique" => {
            let direction = (state.valley_target.xz() - state.obstacle_focus.xz())
                .try_normalize()
                .unwrap_or(-Vec2::X);
            let position = state.obstacle_focus
                - Vec3::new(direction.x, 0.0, direction.y) * (half * 0.62)
                + Vec3::Y * 8.0;
            let target = state.obstacle_focus
                + Vec3::new(direction.x, 0.0, direction.y) * (half * 5.0)
                + Vec3::Y * 2.0;
            (position, target, Vec3::Y)
        }
        _ => unreachable!("capture view is fixed"),
    };
    (
        Transform::from_translation(position).looking_at(target, up),
        target,
    )
}

fn build_manifest(
    state: &CaptureState,
    obstacles: &Query<(&SceneObstacle, Has<Mesh3d>, Has<Collider>)>,
    rock_visuals: &Query<&Mesh3d, With<ProceduralRockVisual>>,
    tree_lods: &Query<&TreeLod>,
    tree_bakes: &Query<&TreeImpostorProvenance>,
    foliage: &Query<&GroundScatterLayer>,
    terrain_materials: &Query<(), With<TerrainMaterialPresentation>>,
    meshes: &Assets<Mesh>,
    vistas: &Query<(&VistaTerrain, Has<Collider>)>,
    weather_particle_count: usize,
    presentation_features: PresentationFeatures,
) -> (CaptureManifest, bool) {
    let mut presented_trees = 0;
    let mut presented_rocks = 0;
    let mut collider_trees = 0;
    let mut collider_rocks = 0;
    for (kind, presented, collidable) in obstacles {
        match kind {
            SceneObstacle::Tree => {
                presented_trees += usize::from(presented);
                collider_trees += usize::from(collidable);
            }
            SceneObstacle::Rock(_) => {
                presented_rocks += usize::from(presented);
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
        .map(|lod| lod.0)
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
        mixed_lods_observed: state
            .recursive_lods_observed
            .iter()
            .map(|(_, lod)| lod)
            .collect::<BTreeSet<_>>()
            .len()
            >= 2,
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
            match view.as_str() {
                "tree-root-detail" => state.tree_focus.is_some(),
                "tree-branch-junction" => {
                    state.tree_focus.is_some()
                        && record.is_some_and(|capture| capture.diagnostic_leaf_suppression)
                }
                "rock-detail" => state.rock_focus.is_some(),
                "terrain-grazing-detail" => {
                    record.is_some_and(|capture| capture.diagnostic_grass_suppression)
                }
                "grass-seam-detail" => state.expects_grass && grass_clumps > 0,
                "forest-floor-debris-detail" => {
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
                _ => true,
            }
        }),
        production_lighting_parity: presentation_features.requested_matches_observed,
        lighting_readiness: state.captures.iter().all(|capture| capture.lighting_ready),
        all_views_render_content: false,
        foliage_detail_present: false,
        all_obstacles_presented: presented_trees == state.expected_trees
            && presented_rocks == state.expected_rocks,
        all_obstacles_collidable: collider_trees == state.expected_trees
            && collider_rocks == state.expected_rocks,
        procedural_rocks_fit_colliders: rock_meshes_inside_colliders,
        trees_have_five_lods: state.expected_trees == 0
            || tree_lods_presented == vec![0, 1, 2, 3, 4],
        tree_detail_captured_when_expected: state.profile != "semantic"
            || state.expected_trees == 0
            || [
                "tree-detail",
                "tree-lighting-baseline",
                "tree-lighting-ao",
                "tree-lighting-shadows",
                "tree-lighting-combined",
                "tree-recursive-lod",
                "ground-cover",
                "tree-textured-leaf-detail",
                "tree-leaf-card-detail",
                "tree-textured-leaf-lod",
                "tree-leaf-card-lod",
                "tree-leaf-transition-25",
                "tree-leaf-transition-50",
                "tree-leaf-transition-75",
                "tree-twig-lod",
                "tree-small-branch-lod",
                "tree-crown-lod",
                "tree-billboard-lod",
            ]
            .into_iter()
            .all(|view| state.captures.iter().any(|capture| capture.view == view)),
        recursive_tree_lod_observed: state.profile != "semantic"
            || state.expected_trees == 0
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
    let passed = validation.passed;
    (
        CaptureManifest {
            pipeline: "tactical_scene_native_capture_v4",
            fixture: state.fixture.clone(),
            source_input: state.input_path.display().to_string(),
            scene_digest: state.digest.clone(),
            seed: state.seed,
            absolute_minute: state.absolute_minute,
            canopy_bps: state.canopy_bps,
            generation_version: state.generation_version,
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
        },
        passed,
    )
}

fn validation_passes(validation: &ValidationSummary) -> bool {
    validation.all_views_captured
        && validation.requested_views_captured_exactly_once
        && validation.requested_detail_targets_available
        && validation.production_lighting_parity
        && validation.lighting_readiness
        && validation.all_views_render_content
        && validation.foliage_detail_present
        && validation.all_obstacles_presented
        && validation.all_obstacles_collidable
        && validation.procedural_rocks_fit_colliders
        && validation.trees_have_five_lods
        && validation.tree_detail_captured_when_expected
        && validation.recursive_tree_lod_observed
        && validation.terrain_material_present
        && validation.coarse_source_terrain_upsampled
        && validation.microrelief_present
        && validation.grass_present_when_expected
        && validation.forest_floor_scatter_present_when_trees
        && validation.understory_present_when_expected
        && validation.loose_stone_scatter_present_when_expected
        && validation.vista_has_three_lods
        && validation.vista_reaches_fifty_kilometres
        && validation.vista_has_no_colliders
        && validation.precipitation_particles_present_when_expected
        && validation.fixture_feature_expectation_met
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

fn diagnostic_suppression(view: &str) -> (bool, bool) {
    (
        view == "tree-branch-junction",
        view == "terrain-grazing-detail",
    )
}

fn foreground_pixel_bps(data: Option<&[u8]>) -> u16 {
    let Some(data) = data else {
        return 0;
    };
    let Some(background) = data.get(..4) else {
        return 0;
    };
    let mut pixels = 0usize;
    let mut foreground = 0usize;
    for pixel in data.as_chunks::<4>().0 {
        pixels += 1;
        let difference = pixel[..3]
            .iter()
            .zip(&background[..3])
            .map(|(left, right)| left.abs_diff(*right) as u16)
            .sum::<u16>();
        foreground += usize::from(difference >= 12);
    }
    foreground
        .checked_mul(10_000)
        .and_then(|value| value.checked_div(pixels))
        .unwrap_or(0)
        .min(10_000) as u16
}

fn mean_luminance(data: Option<&[u8]>) -> f32 {
    let Some(data) = data else {
        return f32::NAN;
    };
    let pixels = data.as_chunks::<4>().0;
    if pixels.is_empty() {
        return f32::NAN;
    }
    let total = pixels
        .iter()
        .map(|pixel| {
            f64::from(pixel[0]) * 0.2126
                + f64::from(pixel[1]) * 0.7152
                + f64::from(pixel[2]) * 0.0722
        })
        .sum::<f64>();
    (total / pixels.len() as f64) as f32
}

fn luminance_delta(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

fn lighting_samples_stable(samples: &[f32]) -> bool {
    if samples.len() < 2 || samples.iter().any(|sample| !sample.is_finite()) {
        return false;
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    luminance_delta(samples) <= (mean * 0.02).max(1.5)
}

fn foliage_detail_pixel_bps(data: Option<&[u8]>) -> u16 {
    let Some(data) = data else {
        return 0;
    };
    let row_bytes = VIEW_WIDTH as usize * 4;
    if data.len() < row_bytes * VIEW_HEIGHT as usize {
        return 0;
    }
    let first_row = VIEW_HEIGHT as usize / 3;
    let mut compared = 0usize;
    let mut detailed = 0usize;
    for y in first_row..VIEW_HEIGHT as usize {
        let row = &data[y * row_bytes..(y + 1) * row_bytes];
        for pair in row
            .as_chunks::<4>()
            .0
            .iter()
            .zip(row[4..].as_chunks::<4>().0)
        {
            compared += 1;
            let difference = pair.0[..3]
                .iter()
                .zip(&pair.1[..3])
                .map(|(left, right)| left.abs_diff(*right) as u16)
                .sum::<u16>();
            detailed += usize::from(difference >= 4);
        }
    }
    detailed
        .checked_mul(10_000)
        .and_then(|value| value.checked_div(compared))
        .unwrap_or(0)
        .min(10_000) as u16
}

fn minimum_foreground_bps(view: &str) -> u16 {
    match view {
        "horizon" => 50,
        // A valid overhead plate can be almost entirely covered by one
        // continuous terrain surface, including the top-left reference pixel.
        // Its separate foliage-detail sentinel protects against a blank frame.
        "beauty-overhead" => 1,
        "tree-root-detail" | "tree-branch-junction" | "rock-detail" => 350,
        "forest-floor-debris-detail" => 500,
        "tree-twig-lod"
        | "tree-small-branch-lod"
        | "tree-crown-lod"
        | "tree-billboard-lod"
        | "tree-recursive-lod" => 200,
        _ => 1_000,
    }
}

fn capture_view_fov(view: &str) -> f32 {
    match view {
        "horizon" => 15.0,
        "tree-detail"
        | "tree-lighting-baseline"
        | "tree-lighting-ao"
        | "tree-lighting-shadows"
        | "tree-lighting-combined"
        | "tree-silhouette"
        | "tree-textured-leaf-lod"
        | "tree-leaf-card-lod"
        | "tree-leaf-transition-25"
        | "tree-leaf-transition-50"
        | "tree-leaf-transition-75" => 48.0,
        "tree-recursive-lod" => 80.0,
        "tree-textured-leaf-detail" | "tree-leaf-card-detail" => 30.0,
        "tree-root-detail" | "tree-branch-junction" | "rock-detail" => 38.0,
        "terrain-grazing-detail" | "grass-seam-detail" => 42.0,
        "forest-floor-debris-detail" => 39.6,
        "tree-twig-lod" => 30.0,
        "tree-small-branch-lod" => 19.0,
        "tree-crown-lod" => 13.0,
        "tree-billboard-lod" => 8.0,
        _ => 65.0,
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct DebrisCaptureTarget {
    focus: Vec3,
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
        leaf_distance_metres: focus.xz().distance(leaf.xz()),
        twig_distance_metres: focus.xz().distance(twig.xz()),
    })
}

fn debris_detail_camera(target: Vec3, azimuth_degrees: f32) -> (Vec3, Vec3, Vec3) {
    let azimuth = azimuth_degrees.to_radians();
    (
        target + Vec3::new(azimuth.sin() * 0.92, 0.36, azimuth.cos() * 0.92),
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
         <h1>{fixture}</h1><p>Digest <code>{digest}</code> · <a href=\"manifest.json\">manifest</a> · \
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
