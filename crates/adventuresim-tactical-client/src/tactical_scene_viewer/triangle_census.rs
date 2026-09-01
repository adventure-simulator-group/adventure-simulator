use std::{collections::BTreeMap, fs, path::PathBuf};

use bevy::{
    camera::{visibility::RenderLayers, visibility::ViewVisibility},
    ecs::system::SystemParam,
    light::NotShadowCaster,
    pbr::wireframe::{Wireframe, WireframeColor, WireframeLineWidth},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};
use bevy_eidolon::prelude::InstanceMaterialData;
use serde::Serialize;

use super::{VIEW_HEIGHT, VIEW_WIDTH, capture_state::SceneCaptureState};
use crate::presentation::ground_scatter::instanced_grass::GrassTriangleCount;
use crate::presentation::{
    GroundScatterLayer, PresentedBuildingMesh, TacticalGameplayCamera, TerrainDetailPatch,
    TerrainMaterialPresentation, TerrainTriangleCount, VistaTerrainMesh,
};

#[derive(Resource)]
pub(super) struct TerrainWireframeCaptureState {
    output: PathBuf,
    configured: bool,
    settled_frames: u32,
    prime_readbacks: u8,
    readback_in_flight: bool,
    capture_requested: bool,
}

impl TerrainWireframeCaptureState {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            configured: false,
            settled_frames: 0,
            prime_readbacks: 0,
            readback_in_flight: false,
            capture_requested: false,
        }
    }
}

#[derive(Default, Serialize)]
struct TerrainWireframeTierReport {
    spacing_metres: f32,
    color: &'static str,
    resident_meshes: usize,
    visible_meshes: usize,
    resident_triangles: usize,
    visible_triangles: usize,
}

#[derive(Serialize)]
struct TerrainWireframeReport {
    pipeline: &'static str,
    fixture: String,
    screenshot: &'static str,
    resolution: [u32; 2],
    camera_translation: [f32; 3],
    camera_target: [f32; 3],
    vertical_fov_degrees: f32,
    count_semantics: &'static str,
    tiers: BTreeMap<String, TerrainWireframeTierReport>,
    total_visible_triangles: usize,
}

#[derive(SystemParam)]
#[allow(clippy::type_complexity)]
pub(super) struct WireframeMeshes<'w, 's> {
    meshes: Query<
        'w,
        's,
        (
            Entity,
            &'static mut Visibility,
            &'static ViewVisibility,
            Has<TerrainDetailPatch>,
            Has<TerrainMaterialPresentation>,
            Option<&'static VistaTerrainMesh>,
            Option<&'static TerrainTriangleCount>,
            Has<Wireframe>,
        ),
        (With<Mesh3d>, Without<Camera3d>),
    >,
}

pub(super) fn capture_terrain_wireframe(
    mut commands: Commands,
    mut state: ResMut<TerrainWireframeCaptureState>,
    capture: Option<Res<SceneCaptureState>>,
    mut camera: Single<
        (
            Entity,
            &mut Transform,
            &mut GlobalTransform,
            &mut Projection,
        ),
        With<TacticalGameplayCamera>,
    >,
    mut wireframe_meshes: WireframeMeshes,
) {
    let Some(capture) = capture.as_deref() else {
        return;
    };
    let terrain_meshes = wireframe_meshes
        .meshes
        .iter()
        .filter(|(_, _, _, detail, playable, vista, _, _)| *detail || *playable || vista.is_some())
        .count();
    if terrain_meshes == 0 {
        return;
    }

    configure_wireframe_entities(&mut commands, &mut wireframe_meshes);
    if configure_wireframe_camera(&mut commands, &mut state, capture, &mut camera) {
        return;
    }
    if state.readback_in_flight || state.capture_requested {
        return;
    }
    if state.prime_readbacks < 2 {
        state.readback_in_flight = true;
        commands.spawn(Screenshot::primary_window()).observe(
            |_: On<ScreenshotCaptured>, mut state: ResMut<TerrainWireframeCaptureState>| {
                state.prime_readbacks += 1;
                state.readback_in_flight = false;
                state.settled_frames = 0;
            },
        );
        return;
    }

    let report = terrain_wireframe_report(capture, &camera, &mut wireframe_meshes);
    fs::write(
        state.output.join("terrain-wireframe.json"),
        serde_json::to_vec_pretty(&report).expect("terrain wireframe report serializes"),
    )
    .expect("terrain wireframe report writes");
    let path = state.output.join(report.screenshot);
    state.capture_requested = true;
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save_to_disk(&path)(captured);
            exit.write(AppExit::Success);
        },
    );
}

fn configure_wireframe_entities(commands: &mut Commands, meshes: &mut WireframeMeshes) {
    for (entity, mut visibility, _, detail, playable, vista, _, has_wireframe) in &mut meshes.meshes
    {
        let color = if detail {
            Some((Color::srgb(1.0, 0.82, 0.12), 1.5))
        } else if playable {
            Some((Color::srgb(0.08, 0.95, 1.0), 1.25))
        } else {
            vista.map(|lod| {
                let color = match lod.0 {
                    0 => Color::srgb(0.18, 1.0, 0.35),
                    1 => Color::srgb(1.0, 0.36, 0.82),
                    _ => Color::srgb(1.0, 0.25, 0.12),
                };
                (color, 1.0)
            })
        };
        *visibility = if color.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some((color, width)) = color
            && !has_wireframe
        {
            commands.entity(entity).insert((
                Wireframe,
                WireframeColor { color },
                WireframeLineWidth { width },
                NotShadowCaster,
                RenderLayers::layer(31),
            ));
        }
    }
}

fn configure_wireframe_camera(
    commands: &mut Commands,
    state: &mut TerrainWireframeCaptureState,
    capture: &SceneCaptureState,
    camera: &mut (
        Entity,
        Mut<Transform>,
        Mut<GlobalTransform>,
        Mut<Projection>,
    ),
) -> bool {
    if !state.configured {
        let transform = Transform::from_translation(capture.ground_eye_position)
            .looking_at(capture.ground_eye_target, Vec3::Y);
        commands.entity(camera.0).insert(RenderLayers::layer(31));
        *camera.1 = transform;
        *camera.2 = GlobalTransform::from(transform);
        if let Projection::Perspective(projection) = &mut *camera.3 {
            projection.fov = 80.0_f32.to_radians();
        }
        state.configured = true;
        return true;
    }
    if state.settled_frames < capture.settle_frames.max(30) {
        state.settled_frames += 1;
        return true;
    }
    false
}

fn terrain_wireframe_report(
    capture: &SceneCaptureState,
    camera: &(
        Entity,
        Mut<Transform>,
        Mut<GlobalTransform>,
        Mut<Projection>,
    ),
    meshes: &mut WireframeMeshes,
) -> TerrainWireframeReport {
    let input = adventuresim_tactical_core::prelude::TacticalSceneInput::load(&capture.input_path)
        .unwrap_or_else(|error| panic!("failed to reload terrain wireframe input: {error}"));
    let vista_spacing = input
        .vista
        .lods
        .iter()
        .map(|lod| (lod.level, lod.spacing_metres))
        .collect::<BTreeMap<_, _>>();
    let mut tiers = BTreeMap::<String, TerrainWireframeTierReport>::new();
    for (_, _, view_visibility, detail, playable, vista, triangle_count, _) in &mut meshes.meshes {
        let Some(triangle_count) = triangle_count else {
            continue;
        };
        let triangles = triangle_count.0;
        let (name, spacing_metres, color) = if detail {
            (
                "detail patch".to_owned(),
                crate::presentation::DETAIL_PATCH_SPACING_METRES,
                "yellow",
            )
        } else if playable {
            (
                "playable terrain".to_owned(),
                capture.terrain.spacing_metres,
                "cyan",
            )
        } else if let Some(lod) = vista {
            (
                format!("vista LOD{}", lod.0),
                vista_spacing.get(&lod.0).copied().unwrap_or_default(),
                match lod.0 {
                    0 => "green",
                    1 => "magenta",
                    _ => "red",
                },
            )
        } else {
            continue;
        };
        let tier = tiers.entry(name).or_default();
        tier.spacing_metres = spacing_metres;
        tier.color = color;
        tier.resident_meshes += 1;
        tier.resident_triangles += triangles;
        if view_visibility.get() {
            tier.visible_meshes += 1;
            tier.visible_triangles += triangles;
        }
    }
    TerrainWireframeReport {
        pipeline: "tactical_terrain_wireframe_v1",
        fixture: capture.fixture.clone(),
        screenshot: "terrain-wireframe.png",
        resolution: [VIEW_WIDTH, VIEW_HEIGHT],
        camera_translation: camera.1.translation.to_array(),
        camera_target: capture.ground_eye_target.to_array(),
        vertical_fov_degrees: 80.0,
        count_semantics: "Triangles in terrain mesh entities whose Bevy ViewVisibility is true. Chunk-level vista culling is reflected; partial triangle clipping within a visible mesh is not.",
        total_visible_triangles: tiers.values().map(|tier| tier.visible_triangles).sum(),
        tiers,
    }
}

#[derive(Resource, Default)]
pub(super) struct TriangleCensusState {
    configured: bool,
    elapsed_frames: u32,
}

#[derive(Default, Serialize)]
struct MeshTierCensus {
    resident_meshes: usize,
    view_visible_meshes: usize,
    resident_triangles: usize,
    view_visible_triangles: usize,
}

#[derive(Default, Serialize)]
struct GrassTierCensus {
    resident_batches: usize,
    resident_tufts: usize,
    gpu_distance_eligible_tufts: usize,
    resident_triangles: usize,
    gpu_distance_eligible_triangles: usize,
}

#[derive(Serialize)]
struct TriangleCensusReport {
    pipeline: &'static str,
    fixture: String,
    camera_translation: [f32; 3],
    camera_target: [f32; 3],
    vertical_fov_degrees: f32,
    mesh_count_semantics: &'static str,
    grass_count_semantics: &'static str,
    terrain: BTreeMap<String, MeshTierCensus>,
    buildings: BTreeMap<String, MeshTierCensus>,
    grass: BTreeMap<String, GrassTierCensus>,
    total_view_visible_mesh_triangles: usize,
    total_gpu_distance_eligible_grass_triangles: usize,
}

#[expect(
    clippy::type_complexity,
    reason = "the census query identifies the mutually exclusive terrain representations without adding diagnostic-only marker entities"
)]
pub(super) fn capture_triangle_census(
    mut state: ResMut<TriangleCensusState>,
    capture: Res<SceneCaptureState>,
    mut camera: Single<
        (&mut Transform, &mut GlobalTransform, &mut Projection),
        With<TacticalGameplayCamera>,
    >,
    terrain_meshes: Query<
        (
            &ViewVisibility,
            &TerrainTriangleCount,
            Has<TerrainDetailPatch>,
            Has<TerrainMaterialPresentation>,
            Option<&VistaTerrainMesh>,
        ),
        With<Mesh3d>,
    >,
    building_meshes: Query<(&ViewVisibility, &PresentedBuildingMesh), With<Mesh3d>>,
    grass_batches: Query<
        (
            &GroundScatterLayer,
            &InstanceMaterialData,
            &GlobalTransform,
            &GrassTriangleCount,
        ),
        Without<TacticalGameplayCamera>,
    >,
    mut exit: MessageWriter<AppExit>,
) {
    if !state.configured {
        let transform = Transform::from_translation(capture.ground_eye_position)
            .looking_at(capture.ground_eye_target, Vec3::Y);
        *camera.0 = transform;
        *camera.1 = GlobalTransform::from(transform);
        if let Projection::Perspective(projection) = &mut *camera.2 {
            projection.fov = 80.0_f32.to_radians();
        }
        state.configured = true;
        return;
    }
    if state.elapsed_frames < capture.settle_frames.max(30) {
        state.elapsed_frames += 1;
        return;
    }

    let camera_position = camera.1.translation();
    let terrain = census_terrain(&terrain_meshes);
    let buildings = census_buildings(&building_meshes);
    let grass = census_grass(&grass_batches, camera_position);

    let total_view_visible_mesh_triangles = terrain
        .values()
        .chain(buildings.values())
        .map(|tier| tier.view_visible_triangles)
        .sum();
    let total_gpu_distance_eligible_grass_triangles = grass
        .values()
        .map(|tier| tier.gpu_distance_eligible_triangles)
        .sum();
    let report = TriangleCensusReport {
        pipeline: "tactical_triangle_census_v1",
        fixture: capture.fixture.clone(),
        camera_translation: camera_position.to_array(),
        camera_target: capture.ground_eye_target.to_array(),
        vertical_fov_degrees: 80.0,
        mesh_count_semantics: "Triangles in terrain and building mesh entities whose Bevy ViewVisibility is true. Partial clipping within a visible mesh is not counted.",
        grass_count_semantics: "Tufts whose world-space camera distance passes Eidolon's configured per-instance range. Eidolon frustum-culls only each scene-wide species/LOD batch, so this is the submitted population whenever that batch intersects the frustum; it intentionally includes off-screen tufts within the distance ring.",
        terrain,
        buildings,
        grass,
        total_view_visible_mesh_triangles,
        total_gpu_distance_eligible_grass_triangles,
    };
    fs::write(
        capture.output.join("triangle-census.json"),
        serde_json::to_vec_pretty(&report).expect("triangle census serializes"),
    )
    .expect("triangle census writes");
    exit.write(AppExit::Success);
}

#[allow(clippy::type_complexity)]
fn census_terrain(
    meshes: &Query<
        (
            &ViewVisibility,
            &TerrainTriangleCount,
            Has<TerrainDetailPatch>,
            Has<TerrainMaterialPresentation>,
            Option<&VistaTerrainMesh>,
        ),
        With<Mesh3d>,
    >,
) -> BTreeMap<String, MeshTierCensus> {
    let mut result = BTreeMap::<String, MeshTierCensus>::new();
    for (visibility, triangles, detail, playable, vista) in meshes {
        let tier = if detail {
            "detail patch".to_owned()
        } else if playable {
            "playable terrain".to_owned()
        } else if let Some(vista) = vista {
            format!("vista LOD{}", vista.0)
        } else {
            continue;
        };
        record_mesh(result.entry(tier).or_default(), visibility, triangles.0);
    }
    result
}

fn census_buildings(
    meshes: &Query<(&ViewVisibility, &PresentedBuildingMesh), With<Mesh3d>>,
) -> BTreeMap<String, MeshTierCensus> {
    let mut result = BTreeMap::<String, MeshTierCensus>::new();
    for (visibility, building) in meshes {
        let tier = format!("{:?}/{:?}", building.level, building.material);
        record_mesh(
            result.entry(tier).or_default(),
            visibility,
            building.triangles,
        );
    }
    result
}

fn census_grass(
    batches: &Query<
        (
            &GroundScatterLayer,
            &InstanceMaterialData,
            &GlobalTransform,
            &GrassTriangleCount,
        ),
        Without<TacticalGameplayCamera>,
    >,
    camera_position: Vec3,
) -> BTreeMap<String, GrassTierCensus> {
    let mut result = BTreeMap::<String, GrassTierCensus>::new();
    for (layer, instances, transform, triangle_count) in batches {
        if *layer != GroundScatterLayer::Grass {
            continue;
        }
        let triangles_per_tuft = triangle_count.0;
        let tier_name = format!(
            "{:.3}-{:.3} metres",
            instances.visibility_range.x, instances.visibility_range.w
        );
        let tier = result.entry(tier_name).or_default();
        let resident_tufts = instances.instances.len();
        let distance_eligible_tufts = instances
            .instances
            .iter()
            .filter(|instance| {
                let distance = transform
                    .transform_point(instance.position)
                    .distance(camera_position);
                distance >= instances.visibility_range.x && distance <= instances.visibility_range.w
            })
            .count();
        tier.resident_batches += 1;
        tier.resident_tufts += resident_tufts;
        tier.gpu_distance_eligible_tufts += distance_eligible_tufts;
        tier.resident_triangles += resident_tufts * triangles_per_tuft;
        tier.gpu_distance_eligible_triangles += distance_eligible_tufts * triangles_per_tuft;
    }
    result
}

fn record_mesh(tier: &mut MeshTierCensus, visibility: &ViewVisibility, triangles: usize) {
    tier.resident_meshes += 1;
    tier.resident_triangles += triangles;
    if visibility.get() {
        tier.view_visible_meshes += 1;
        tier.view_visible_triangles += triangles;
    }
}
