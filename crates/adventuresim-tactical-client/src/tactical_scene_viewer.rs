use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::presentation::{
    FoliageLayer, ProceduralRockVisual, TacticalPresentationPlugin, TerrainMaterialPresentation,
    TreeLod, VistaTerrain, WeatherParticle,
};

const VIEW_WIDTH: u32 = 1280;
const VIEW_HEIGHT: u32 = 720;
const STANDING_EYE_HEIGHT_METRES: f32 = 1.65;

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
}

#[derive(Resource)]
struct CaptureState {
    fixture: String,
    input_path: PathBuf,
    output: PathBuf,
    digest: String,
    seed: u64,
    generation_version: u16,
    weather: WeatherSnapshot,
    repairs: RepairSummary,
    terrain: TerrainSummary,
    expected_trees: usize,
    expected_rocks: usize,
    vista_lods_supplied: usize,
    vista_diameter_metres: f32,
    vista_peak_metres: f32,
    peak_target: Vec3,
    obstacle_focus: Vec3,
    ground_eye_position: Vec3,
    ground_eye_target: Vec3,
    settle_frames: u32,
    view: usize,
    view_started: bool,
    prime_readbacks: u8,
    settled: u32,
    in_flight: bool,
    captures: Vec<CaptureRecord>,
}

#[derive(Component)]
struct CaptureOverlay;

#[derive(Clone, Copy)]
struct CaptureView {
    slug: &'static str,
    label: &'static str,
    overlay: bool,
}

const CAPTURE_VIEWS: [CaptureView; 5] = [
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
    trees_have_three_lods: bool,
    terrain_material_present: bool,
    coarse_source_terrain_upsampled: bool,
    microrelief_present: bool,
    grass_present: bool,
    understory_present_when_expected: bool,
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
    generation_version: u16,
    weather: WeatherSnapshot,
    repairs: RepairSummary,
    terrain: TerrainSummary,
    obstacles: ObstacleSummary,
    foliage: FoliageSummary,
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
    let environment = input.environment_snapshot(generated.digest.clone());
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
    };
    let exit = App::new()
        .add_plugins(
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
            atmosphere_enabled: false,
            environment_light_enabled: false,
            bloom_enabled: false,
            ssao_enabled: false,
            ..default()
        })
        .insert_resource(ClearColor(Color::srgb_u8(158, 181, 195)))
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 30_000.0,
            ..default()
        })
        .insert_resource(SceneSetup(Some(setup)))
        .add_systems(PostStartup, setup_scene)
        .add_systems(Last, capture_views)
        .run();
    if exit != AppExit::Success {
        std::process::exit(1);
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
    } = setup;
    let GeneratedTacticalScene {
        digest,
        terrain,
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
                GeneratedObstacle::Rock { x, z } => {
                    expected_rocks += 1;
                    (
                        x,
                        z,
                        SceneObstacle::Rock,
                        Collider::sphere(ROCK_RADIUS_METRES),
                        ROCK_RADIUS_METRES,
                        meshes.add(Sphere::new(ROCK_RADIUS_METRES * 1.08)),
                        Color::srgb(1.0, 0.08, 0.72),
                    )
                }
            };
        let x = f32::from(grid_x) * input.playable.spacing_metres - terrain.width() * 0.5;
        let z = f32::from(grid_z) * input.playable.spacing_metres - terrain.depth() * 0.5;
        let y = terrain.height_at(Vec2::new(x, z)).unwrap_or_default() + y_offset;
        obstacle_position_sum += Vec3::new(x, y, z);
        obstacle_count += 1;
        commands.spawn((
            Name::new("Captured tactical obstacle"),
            kind,
            RigidBody::Static,
            CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
            collider,
            Transform::from_xyz(x, y, z),
        ));
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
            Transform::from_xyz(x, y, z),
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
    commands.spawn((
        Name::new("Captured tactical terrain"),
        SceneId(input.scene_key.clone()),
        environment,
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
        vista_lods_supplied: input.vista.lods.len(),
        vista_diameter_metres,
        vista_peak_metres,
        peak_target,
        obstacle_focus,
        ground_eye_position,
        ground_eye_target,
        settle_frames,
        view: 0,
        view_started: false,
        prime_readbacks: 0,
        settled: 0,
        in_flight: false,
        captures: Vec::new(),
    });
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

fn capture_views(
    mut commands: Commands,
    mut state: Option<ResMut<CaptureState>>,
    mut camera: Single<(&mut Transform, &mut GlobalTransform, &mut Projection), With<Camera3d>>,
    mut overlays: Query<&mut Visibility, (With<CaptureOverlay>, Without<VistaTerrain>)>,
    obstacles: Query<(&SceneObstacle, Has<Mesh3d>, Has<Collider>)>,
    rock_visuals: Query<&Mesh3d, With<ProceduralRockVisual>>,
    tree_lods: Query<&TreeLod>,
    foliage: Query<&FoliageLayer>,
    terrain_materials: Query<(), With<TerrainMaterialPresentation>>,
    meshes: Res<Assets<Mesh>>,
    mut vistas: ParamSet<(
        Query<(&VistaTerrain, Has<Collider>)>,
        Query<&mut Visibility, (With<VistaTerrain>, Without<CaptureOverlay>)>,
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
        let (transform, target) = camera_for_view(view.slug, state);
        *camera.0 = transform;
        *camera.1 = GlobalTransform::from(transform);
        if let Projection::Perspective(projection) = &mut *camera.2 {
            projection.fov = capture_view_fov(view.slug).to_radians();
        }
        for mut visibility in &mut overlays {
            *visibility = if view.overlay {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        for mut visibility in &mut vistas.p1() {
            *visibility = if view.slug == "horizon" {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        state.view_started = true;
        state.settled = 0;
        if view.slug != "warmup" {
            state.captures.push(CaptureRecord {
                view: view.slug.to_owned(),
                label: view.label.to_owned(),
                screenshot: format!("{}.png", view.slug),
                camera_translation: camera.0.translation.to_array(),
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
            &foliage,
            &terrain_materials,
            &meshes,
            &vistas.p0(),
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
    foliage: &Query<&FoliageLayer>,
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
            SceneObstacle::Rock => {
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
    let mut grass_clumps = 0;
    let mut understory_clumps = 0;
    for layer in foliage {
        match layer {
            FoliageLayer::Grass => grass_clumps += 1,
            FoliageLayer::Understory => understory_clumps += 1,
        }
    }
    let foliage_summary = FoliageSummary {
        grass_clumps,
        understory_clumps,
    };
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
        trees_have_three_lods: state.expected_trees == 0 || tree_lods_presented == vec![0, 1, 2],
        terrain_material_present: terrain_materials.iter().count() == 1,
        coarse_source_terrain_upsampled: state.terrain.source_spacing_metres <= 2.0
            || (state.terrain.spacing_metres <= 2.0
                && state.terrain.generated_samples > state.terrain.source_samples),
        microrelief_present: state.repairs.microrelief_adjusted_samples > 0,
        grass_present: grass_clumps > 0,
        understory_present_when_expected: !expects_understory || understory_clumps > 0,
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
            generation_version: state.generation_version,
            weather: state.weather,
            repairs: state.repairs,
            terrain: state.terrain,
            obstacles: obstacle_summary,
            foliage: foliage_summary,
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
        && validation.trees_have_three_lods
        && validation.terrain_material_present
        && validation.coarse_source_terrain_upsampled
        && validation.microrelief_present
        && validation.grass_present
        && validation.understory_present_when_expected
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
    for pixel in data.chunks_exact(4) {
        pixels += 1;
        let difference = pixel[..3]
            .iter()
            .zip(&background[..3])
            .map(|(left, right)| left.abs_diff(*right) as u16)
            .sum::<u16>();
        foreground += usize::from(difference >= 12);
    }
    if pixels == 0 {
        0
    } else {
        ((foreground * 10_000 / pixels).min(10_000)) as u16
    }
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
        for pair in row.chunks_exact(4).zip(row[4..].chunks_exact(4)) {
            compared += 1;
            let difference = pair.0[..3]
                .iter()
                .zip(&pair.1[..3])
                .map(|(left, right)| left.abs_diff(*right) as u16)
                .sum::<u16>();
            detailed += usize::from(difference >= 4);
        }
    }
    if compared == 0 {
        0
    } else {
        ((detailed * 10_000 / compared).min(10_000)) as u16
    }
}

fn minimum_foreground_bps(view: &str) -> u16 {
    if view == "horizon" { 50 } else { 1_000 }
}

fn capture_view_fov(view: &str) -> f32 {
    if view == "horizon" { 15.0 } else { 65.0 }
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
