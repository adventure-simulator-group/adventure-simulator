use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::{
    camera::visibility::VisibilityRange,
    light::NotShadowCaster,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::presentation::{
    GroundScatterLayer, ProceduralRockVisual, TacticalPresentationPlugin,
    TacticalTreeLeafCardMaterial, TerrainMaterialPresentation, TreeImpostorProvenance,
    TreeLeafRepresentation, TreeLod, TreeLodCluster, TreeLodRenderOverride, VistaTerrain,
    WeatherParticle, oak_leaf_material, oak_review_terminal_specimen, scene_ambient_light,
};

const VIEW_WIDTH: u32 = 1280;
const VIEW_HEIGHT: u32 = 720;
const STANDING_EYE_HEIGHT_METRES: f32 = 1.65;
const PROCEDURAL_OAK_LEAVES_PER_TREE: usize = 69_632;

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
}

#[derive(Resource)]
struct CaptureState {
    fixture: String,
    input_path: PathBuf,
    output: PathBuf,
    digest: String,
    seed: u64,
    absolute_minute: u64,
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
    vista_peak_metres: f32,
    peak_target: Vec3,
    obstacle_focus: Vec3,
    tree_focus: Option<Vec3>,
    tree_leaf_focus: Option<Vec3>,
    tree_leaf_camera: Option<Vec3>,
    tree_focus_entity: Option<Entity>,
    tree_review_entities: Vec<Entity>,
    tree_review_leaf_entities: Vec<(Entity, TreeLeafRepresentation)>,
    ground_eye_position: Vec3,
    ground_eye_target: Vec3,
    settle_frames: u32,
    tree_review_azimuth_degrees: f32,
    view: usize,
    view_started: bool,
    prime_readbacks: u8,
    settled: u32,
    in_flight: bool,
    captures: Vec<CaptureRecord>,
    recursive_lods_observed: BTreeSet<(u8, u8)>,
}

#[derive(Component)]
struct CaptureOverlay;

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

#[derive(Clone, Serialize)]
struct CaptureRecord {
    view: String,
    label: String,
    screenshot: String,
    camera_translation: [f32; 3],
    camera_target: [f32; 3],
    foreground_pixel_bps: u16,
    detail_pixel_bps: u16,
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
    peak_height_metres: f32,
    collider_count: usize,
}

#[derive(Serialize)]
struct ValidationSummary {
    all_views_captured: bool,
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
    let (ambient_color, ambient_brightness) = capture_ambient_light(&environment);
    let output = output.map_or_else(
        || default_output(&repository_root, &fixture, &generated.digest),
        absolute_from_current,
    );
    prepare_fresh_output(&output);
    fs::copy(&input_path, output.join("input.json"))
        .unwrap_or_else(|error| panic!("failed to copy capture input: {error}"));
    println!("CAPTURE_OUTPUT={}", output.display());

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
    .add_plugins(TacticalPresentationPlugin {
        // Use the same sun-driven atmosphere as gameplay. A fixed clear color
        // made night captures display stars against a pale daytime-blue sky.
        atmosphere_enabled: true,
        environment_light_enabled: false,
        bloom_enabled: false,
        ssao_enabled: false,
        ..default()
    })
    .insert_resource(ClearColor(Color::srgb_u8(158, 181, 195)))
    .insert_resource(GlobalAmbientLight {
        color: Color::srgb(ambient_color.x, ambient_color.y, ambient_color.z),
        brightness: ambient_brightness,
        ..default()
    })
    .insert_resource(SceneSetup(Some(setup)))
    .add_systems(PostStartup, setup_scene);
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

fn capture_ambient_light(environment: &SceneEnvironment) -> (Vec3, f32) {
    let celestial = celestial_directions(
        environment.absolute_minute,
        environment.latitude_microdegrees,
        environment.longitude_microdegrees,
    );
    let sun_altitude = celestial.sun[1].asin().to_degrees();
    let moon_altitude = celestial.moon[1].asin().to_degrees();
    scene_ambient_light(sun_altitude, moon_altitude, celestial.lunar_illumination)
}

#[cfg(test)]
mod capture_lighting_tests {
    use super::*;

    #[test]
    fn capture_fill_light_respects_sun_and_moon_horizons() {
        assert_eq!(scene_ambient_light(-25.0, -24.0, 1.0).1, 0.6);
        assert_eq!(scene_ambient_light(30.0, -25.0, 0.0).1, 30_000.0);
        assert_eq!(scene_ambient_light(-25.0, 30.0, 1.0).1, 0.85);
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
    let (vista_diameter_metres, vista_peak_metres, peak_target) = vista_metrics(&input);
    let mut expected_trees = 0;
    let mut expected_rocks = 0;
    let mut obstacle_position_sum = Vec3::ZERO;
    let mut obstacle_count = 0usize;
    let mut tree_focus = None;
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
    let expects_grass = ground.cover_count(GroundCover::TallGrass) > 0;
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
        vista_peak_metres,
        peak_target,
        obstacle_focus,
        tree_focus,
        tree_leaf_focus,
        tree_leaf_camera,
        tree_focus_entity,
        tree_review_entities,
        tree_review_leaf_entities,
        ground_eye_position,
        ground_eye_target,
        settle_frames,
        tree_review_azimuth_degrees,
        view: 0,
        view_started: false,
        prime_readbacks: 0,
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

fn vista_metrics(input: &TacticalSceneInput) -> (f32, f32, Vec3) {
    let mut diameter = 0.0_f32;
    let mut peak = f32::NEG_INFINITY;
    let mut target = Vec3::ZERO;
    for lod in &input.vista.lods {
        diameter = diameter.max(f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres);
        let center_x = f32::from(lod.width.saturating_sub(1)) * 0.5;
        let center_z = f32::from(lod.depth.saturating_sub(1)) * 0.5;
        for (index, height) in lod.heights_metres.iter().copied().enumerate() {
            if height > peak {
                let x = (index % usize::from(lod.width)) as f32;
                let z = (index / usize::from(lod.width)) as f32;
                peak = height;
                target = Vec3::new(
                    (x - center_x) * lod.spacing_metres + lod.origin_east_metres as f32,
                    height,
                    (z - center_z) * lod.spacing_metres + lod.origin_north_metres as f32,
                );
            }
        }
    }
    if !peak.is_finite() {
        peak = 0.0;
    }
    (diameter, peak, target)
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
        ),
        With<Camera3d>,
    >,
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
        Query<Entity, With<MeshMaterial3d<TacticalTreeLeafCardMaterial>>>,
    )>,
    particles: Query<(), With<WeatherParticle>>,
) {
    let Some(state) = state.as_deref_mut() else {
        return;
    };
    if state.in_flight || state.view >= CAPTURE_VIEWS.len() {
        return;
    }
    let view = CAPTURE_VIEWS[state.view];
    if !state.view_started {
        let lighting_mode = match view.slug {
            "tree-lighting-baseline" => TREE_LIGHTING_MODES[0],
            "tree-lighting-ao" => TREE_LIGHTING_MODES[1],
            "tree-lighting-shadows" => TREE_LIGHTING_MODES[2],
            _ => TREE_LIGHTING_MODES[3],
        };
        for (_, material) in scene_visibility.p2().iter_mut() {
            material.surface_parameters.z = lighting_mode.ambient_occlusion_strength;
        }
        let leaf_entities = scene_visibility.p3().iter().collect::<Vec<_>>();
        for entity in leaf_entities {
            if lighting_mode.shadows_enabled {
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
            *visibility = if view.slug == "horizon" {
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
                foreground_pixel_bps: 0,
                detail_pixel_bps: 0,
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
    let required_prime_readbacks = if state.view == 0 { 2 } else { 1 };
    if state.prime_readbacks < required_prime_readbacks {
        state.in_flight = true;
        commands.spawn(Screenshot::primary_window()).observe(
            |_: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                state.prime_readbacks += 1;
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
                state.in_flight = false;
            },
        );
        return;
    }

    let path = state.output.join(format!("{}.png", view.slug));
    let final_view = state.view + 1 == CAPTURE_VIEWS.len();
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
            particles.iter().count(),
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
            state.in_flight = false;
            if let Some((mut manifest, _)) = final_data.take() {
                manifest.captures.clone_from(&state.captures);
                manifest.validation.all_views_render_content =
                    manifest.captures.iter().all(|capture| {
                        capture.foreground_pixel_bps >= minimum_foreground_bps(&capture.view)
                    });
                // The flat fixture provides a stable image-space sentinel for
                // the foliage material. Slopes, dark wetlands, and tree cover
                // can legitimately hide the same fine overhead contrast.
                manifest.validation.foliage_detail_present = manifest.fixture
                    != "flat-dry-grassland"
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
            let horizontal = Vec2::new(state.peak_target.x, state.peak_target.z);
            let lateral = horizontal
                .try_normalize()
                .map(|direction| Vec2::new(-direction.y, direction.x))
                .unwrap_or(Vec2::Y)
                * (horizontal.length() * 0.75).clamp(500.0, 5_000.0);
            let position = state.obstacle_focus + Vec3::new(lateral.x, 6.5, lateral.y);
            (position, state.peak_target, Vec3::Y)
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
        peak_height_metres: state.vista_peak_metres,
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
        all_views_captured: state.captures.len() == CAPTURE_VIEWS.len() - 1,
        all_views_render_content: false,
        foliage_detail_present: false,
        all_obstacles_presented: presented_trees == state.expected_trees
            && presented_rocks == state.expected_rocks,
        all_obstacles_collidable: collider_trees == state.expected_trees
            && collider_rocks == state.expected_rocks,
        procedural_rocks_fit_colliders: rock_meshes_inside_colliders,
        trees_have_five_lods: state.expected_trees == 0
            || tree_lods_presented == vec![0, 1, 2, 3, 4],
        tree_detail_captured_when_expected: state.expected_trees == 0
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
        recursive_tree_lod_observed: state.expected_trees == 0
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
            pipeline: "tactical_scene_native_capture_v1",
            fixture: state.fixture.clone(),
            source_input: state.input_path.display().to_string(),
            scene_digest: state.digest.clone(),
            seed: state.seed,
            absolute_minute: state.absolute_minute,
            canopy_bps: state.canopy_bps,
            generation_version: state.generation_version,
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
        fs::write(
            output.join("failure.txt"),
            "Tactical scene capture validation failed; inspect manifest.json and screenshots.\n",
        )
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
