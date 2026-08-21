use super::procedural_assets::{
    FOREST_LITTER_HEIGHT_RANGE_METRES, FOREST_LITTER_TILE_METRES, FOREST_SOIL_HEIGHT_RANGE_METRES,
    FOREST_SOIL_TILE_METRES,
};
use super::*;

const TERMINAL_SWARD_FADE_START_METRES: f32 = 100.0;
const TERMINAL_SWARD_FADE_END_METRES: f32 = 110.0;
const DETAIL_PATCH_RADIUS_METRES: f32 = 20.0;
const DETAIL_PATCH_MORPH_START_METRES: f32 = 15.5;
const DETAIL_PATCH_SPACING_METRES: f32 = 0.25;
const DETAIL_PATCH_SNAP_METRES: f32 = 1.0;
const DETAIL_PATCH_DEPTH_BIAS: f32 = 2.0;
const DETAIL_PATCH_BASE_CUTOUT_RADIUS_METRES: f32 = 18.5;
const DETAIL_RELIEF_MINIMUM_METRES: f32 = -0.075;
const DETAIL_RELIEF_MAXIMUM_METRES: f32 = 0.105;
pub(in crate::presentation) const TACTICAL_DIRT_SRGB: [u8; 3] = [101, 82, 49];

pub(super) fn scene_ground_color(environment: &SceneEnvironment) -> Color {
    let mut rgb = if environment.water_bps >= 5_000 {
        [52.0, 83.0, 98.0]
    } else if environment.wetland_bps >= 4_000 {
        [70.0, 62.0, 43.0]
    } else if environment.cultivation_bps >= 4_000 {
        [116.0, 91.0, 49.0]
    } else {
        TACTICAL_DIRT_SRGB.map(f32::from)
    };
    let snow = f32::from(environment.weather.snow_cover_bps) / 10_000.0;
    let wet = f32::from(environment.weather.ground_moisture_bps) / 10_000.0;
    for channel in &mut rgb {
        *channel *= 1.0 - wet * 0.22;
        *channel = *channel * (1.0 - snow) + 220.0 * snow;
    }
    Color::srgb(rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0)
}

pub(crate) fn terrain_heightmap_image(terrain: &SceneTerrain) -> Image {
    let width = terrain.grid_width() as u32;
    let height = terrain.grid_depth() as u32;
    let minimum = terrain.minimum_height();
    let maximum = terrain.maximum_height();
    encoded_terrain_heightmap_image(width, height, minimum, maximum, |x, z| {
        let world = Vec2::new(
            x as f32 * terrain.grid_scale() - terrain.width() * 0.5,
            z as f32 * terrain.grid_scale() - terrain.depth() * 0.5,
        );
        terrain.height_at(world).unwrap_or(minimum)
    })
}

/// Builds the heightfield used to seat tree materials against the close-range
/// render mesh. Unlike the authoritative heightmap, this includes the same
/// presentation-only clods, drainage, root mounds, and obstacle contact relief
/// evaluated by [`terrain_detail_patch_mesh`]. Sampling it at the detail patch
/// spacing keeps the shader's triangle reconstruction aligned with the visible
/// ground rather than a coarser surface hidden underneath it.
pub(crate) fn terrain_contact_heightmap_image(
    terrain: &SceneTerrain,
    ground: Option<&SceneGround>,
    environment: &SceneEnvironment,
    vista: &ActiveVistaSurface,
    obstacles: &Query<(&SceneObstacle, &Transform)>,
) -> (Image, Vec2) {
    let width = (terrain.width() / DETAIL_PATCH_SPACING_METRES).round() as u32 + 1;
    let height = (terrain.depth() / DETAIL_PATCH_SPACING_METRES).round() as u32 + 1;
    let minimum = terrain.minimum_height() + DETAIL_RELIEF_MINIMUM_METRES;
    let maximum = terrain.maximum_height() + DETAIL_RELIEF_MAXIMUM_METRES;
    let tree_positions = detail_tree_positions(obstacles);
    let rock_influences = detail_rock_influences(obstacles);
    let image = encoded_terrain_heightmap_image(width, height, minimum, maximum, |x, z| {
        let point = Vec2::new(
            x as f32 / (width - 1) as f32 * terrain.width() - terrain.width() * 0.5,
            z as f32 / (height - 1) as f32 * terrain.depth() - terrain.depth() * 0.5,
        );
        let base_height = terrain.height_at(point).unwrap_or(terrain.minimum_height());
        base_height
            + terrain_surface_relief(
                stable_text_seed(&environment.scene_digest) ^ 0x7465_7272_6169_6e64,
                point,
                terrain,
                ground,
                environment,
                vista,
                &tree_positions,
                &rock_influences,
            )
    });
    (image, Vec2::new(minimum, maximum))
}

fn encoded_terrain_heightmap_image(
    width: u32,
    height: u32,
    minimum: f32,
    maximum: f32,
    mut height_at: impl FnMut(u32, u32) -> f32,
) -> Image {
    let range = (maximum - minimum).max(0.001);
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for z in 0..height {
        for x in 0..width {
            let normalized = ((height_at(x, z) - minimum) / range).clamp(0.0, 1.0);
            let encoded = (normalized * 65_535.0).round() as u16;
            pixels.extend_from_slice(&[(encoded & 0xff) as u8, (encoded >> 8) as u8, 0, 255]);
        }
    }
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[derive(Component)]
pub(in crate::presentation) struct ScenePresentationOf(pub(in crate::presentation) Entity);

#[derive(Component)]
pub(crate) struct TerrainMaterialPresentation;

/// Camera-local render refinement. This mesh never participates in collision
/// or ground queries; its bounded displacement is presentation-only.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TerrainDetailPatch {
    centre: Vec2,
    vista_revision: u64,
    obstacle_signature: u64,
}

#[derive(Debug, Clone, Copy)]
struct DetailRockInfluence {
    centre: Vec2,
    radius: f32,
}

#[derive(Component)]
pub(in crate::presentation) struct PendingTerrainPresentation;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalTerrainExtension {
    #[uniform(100)]
    base_color: Vec4,
    #[uniform(100)]
    grass_color: Vec4,
    #[uniform(100)]
    cover: Vec4,
    #[uniform(100)]
    weather: Vec4,
    #[uniform(100)]
    far_sward: Vec4,
    #[uniform(100)]
    playable_bounds: Vec4,
    #[uniform(100)]
    detail_patch: Vec4,
    #[uniform(100)]
    soil_detail: Vec4,
    #[uniform(100)]
    litter_detail: Vec4,
    #[texture(101)]
    #[sampler(102)]
    ground_map: Handle<Image>,
    #[texture(103)]
    #[sampler(104)]
    soil_height_ao: Handle<Image>,
    #[texture(105)]
    #[sampler(106)]
    litter_surface: Handle<Image>,
    #[texture(107)]
    #[sampler(108)]
    litter_normal: Handle<Image>,
}

impl MaterialExtension for TacticalTerrainExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

pub(in crate::presentation) type TacticalTerrainMaterial =
    ExtendedMaterial<StandardMaterial, TacticalTerrainExtension>;

pub(in crate::presentation) fn on_game_scene_added(
    event: On<Add, SceneId>,
    mut commands: Commands,
) {
    commands
        .entity(event.entity)
        .insert(PendingTerrainPresentation);
}

pub(in crate::presentation) fn present_pending_terrain(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &SceneId,
            &SceneTerrain,
            Option<&SceneEnvironment>,
            Option<&SceneGround>,
        ),
        With<PendingTerrainPresentation>,
    >,
    presentations: Query<
        (
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        ),
        With<TerrainMaterialPresentation>,
    >,
    mut detail_presentations: Query<
        (
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
            &mut TerrainDetailPatch,
            &Mesh3d,
        ),
        With<TerrainDetailPatch>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalTerrainMaterial>>,
    mut images: ResMut<Assets<Image>>,
    procedural_assets: Res<ProceduralEnvironmentAssets>,
    vista: Res<ActiveVistaSurface>,
    obstacles: Query<(&SceneObstacle, &Transform)>,
) {
    let tree_positions = detail_tree_positions(&obstacles);
    let rock_influences = detail_rock_influences(&obstacles);
    let obstacle_signature = detail_obstacle_signature(&tree_positions, &rock_influences);
    for (entity, id, terrain, environment, ground) in &query {
        let legacy_environment;
        let environment = if let Some(environment) = environment {
            environment
        } else {
            legacy_environment = legacy_scene_environment(id);
            &legacy_environment
        };
        let presented =
            if let (Some((_, handle)), Some((_, detail_handle, mut patch, mesh_handle))) = (
                presentations.iter().find(|(source, _)| source.0 == entity),
                detail_presentations
                    .iter_mut()
                    .find(|(source, _, _, _)| source.0 == entity),
            ) {
                if materials.get(&handle.0).is_some() && materials.get(&detail_handle.0).is_some() {
                    let material = terrain_material(
                        terrain,
                        environment,
                        ground,
                        &procedural_assets,
                        &mut images,
                    );
                    *materials
                        .get_mut(&handle.0)
                        .expect("checked terrain material") = material.clone();
                    let mut detail_material = material;
                    detail_material.base.depth_bias = DETAIL_PATCH_DEPTH_BIAS;
                    detail_material.extension.detail_patch.x = 0.0;
                    *materials
                        .get_mut(&detail_handle.0)
                        .expect("checked detail terrain material") = detail_material;
                    if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
                        *mesh = terrain_detail_patch_mesh(
                            terrain,
                            ground,
                            environment,
                            &vista,
                            patch.centre,
                            &tree_positions,
                            &rock_influences,
                        );
                        patch.vista_revision = vista.revision();
                        patch.obstacle_signature = obstacle_signature;
                    }
                    true
                } else {
                    false
                }
            } else {
                info!(?entity, "Spawning a scene {id:?}");
                let material = terrain_material(
                    terrain,
                    environment,
                    ground,
                    &procedural_assets,
                    &mut images,
                );
                let mut detail_material = material.clone();
                detail_material.base.depth_bias = DETAIL_PATCH_DEPTH_BIAS;
                detail_material.extension.detail_patch.x = 0.0;
                let material = materials.add(material);
                let detail_material = materials.add(detail_material);
                commands.spawn((
                    Name::new(format!("{} terrain mesh", id.0)),
                    ScenePresentationOf(entity),
                    TerrainMaterialPresentation,
                    Mesh3d(meshes.add(terrain.mesh())),
                    MeshMaterial3d(material.clone()),
                ));
                let centre = Vec2::ZERO;
                commands.spawn((
                    Name::new(format!("{} camera-local terrain detail patch", id.0)),
                    ScenePresentationOf(entity),
                    TerrainDetailPatch {
                        centre,
                        vista_revision: vista.revision(),
                        obstacle_signature,
                    },
                    NotShadowCaster,
                    Mesh3d(meshes.add(terrain_detail_patch_mesh(
                        terrain,
                        ground,
                        environment,
                        &vista,
                        centre,
                        &tree_positions,
                        &rock_influences,
                    ))),
                    MeshMaterial3d(detail_material),
                ));
                true
            };
        if presented {
            commands
                .entity(entity)
                .remove::<PendingTerrainPresentation>();
        }
    }
}

/// Moves the bounded high-resolution patch in one-metre increments. All
/// relief is evaluated in world space, so snapping changes only the fully
/// morphed-out perimeter rather than making the surface swim under the player.
pub(in crate::presentation) fn update_terrain_detail_patch(
    camera: Single<&GlobalTransform, With<Camera3d>>,
    active: Res<ActiveTacticalScene>,
    scenes: Query<(&SceneTerrain, Option<&SceneGround>, &SceneEnvironment)>,
    mut patches: Query<(&ScenePresentationOf, &mut TerrainDetailPatch, &Mesh3d)>,
    obstacles: Query<(&SceneObstacle, &Transform)>,
    vista: Res<ActiveVistaSurface>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let camera_position = camera.translation().xz();
    let desired_centre =
        (camera_position / DETAIL_PATCH_SNAP_METRES).round() * DETAIL_PATCH_SNAP_METRES;
    let tree_positions = detail_tree_positions(&obstacles);
    let rock_influences = detail_rock_influences(&obstacles);
    let obstacle_signature = detail_obstacle_signature(&tree_positions, &rock_influences);
    for (source, mut patch, mesh_handle) in &mut patches {
        if active.entity.is_some_and(|entity| entity != source.0)
            || (patch.vista_revision == vista.revision()
                && patch.obstacle_signature == obstacle_signature
                && patch.centre.distance_squared(desired_centre)
                    < DETAIL_PATCH_SNAP_METRES * DETAIL_PATCH_SNAP_METRES * 0.25)
        {
            continue;
        }
        let Ok((terrain, ground, environment)) = scenes.get(source.0) else {
            continue;
        };
        let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) else {
            continue;
        };
        *mesh = terrain_detail_patch_mesh(
            terrain,
            ground,
            environment,
            &vista,
            desired_centre,
            &tree_positions,
            &rock_influences,
        );
        patch.centre = desired_centre;
        patch.vista_revision = vista.revision();
        patch.obstacle_signature = obstacle_signature;
    }
}

fn detail_tree_positions(obstacles: &Query<(&SceneObstacle, &Transform)>) -> Vec<Vec2> {
    let mut positions = obstacles
        .iter()
        .filter_map(|(obstacle, transform)| {
            matches!(obstacle, SceneObstacle::Tree).then_some(transform.translation.xz())
        })
        .collect::<Vec<_>>();
    positions.sort_unstable_by(|left, right| {
        left.x
            .total_cmp(&right.x)
            .then_with(|| left.y.total_cmp(&right.y))
    });
    positions
}

fn detail_rock_influences(
    obstacles: &Query<(&SceneObstacle, &Transform)>,
) -> Vec<DetailRockInfluence> {
    let mut rocks = obstacles
        .iter()
        .filter_map(|(obstacle, transform)| match obstacle {
            SceneObstacle::Rock(recipe) => Some(DetailRockInfluence {
                centre: transform.translation.xz(),
                radius: recipe.collision_radius_metres(),
            }),
            SceneObstacle::Tree => None,
        })
        .collect::<Vec<_>>();
    rocks.sort_unstable_by(|left, right| {
        left.centre
            .x
            .total_cmp(&right.centre.x)
            .then_with(|| left.centre.y.total_cmp(&right.centre.y))
            .then_with(|| left.radius.total_cmp(&right.radius))
    });
    rocks
}

fn detail_obstacle_signature(
    tree_positions: &[Vec2],
    rock_influences: &[DetailRockInfluence],
) -> u64 {
    let trees = tree_positions.iter().fold(
        0x7472_6565_5f67_7264 ^ tree_positions.len() as u64,
        |signature, position| {
            splitmix64(
                signature
                    ^ u64::from(position.x.to_bits()).rotate_left(17)
                    ^ u64::from(position.y.to_bits()),
            )
        },
    );
    rock_influences.iter().fold(
        trees ^ (rock_influences.len() as u64).rotate_left(31),
        |signature, rock| {
            splitmix64(
                signature
                    ^ u64::from(rock.centre.x.to_bits()).rotate_left(11)
                    ^ u64::from(rock.centre.y.to_bits()).rotate_left(37)
                    ^ u64::from(rock.radius.to_bits()),
            )
        },
    )
}

fn terrain_detail_patch_mesh(
    terrain: &SceneTerrain,
    ground: Option<&SceneGround>,
    environment: &SceneEnvironment,
    vista: &ActiveVistaSurface,
    centre: Vec2,
    tree_positions: &[Vec2],
    rock_influences: &[DetailRockInfluence],
) -> Mesh {
    let diameter_steps =
        (DETAIL_PATCH_RADIUS_METRES * 2.0 / DETAIL_PATCH_SPACING_METRES).round() as usize;
    let width = diameter_steps + 1;
    let minimum = centre - Vec2::splat(DETAIL_PATCH_RADIUS_METRES);
    let seed = stable_text_seed(&environment.scene_digest) ^ 0x7465_7272_6169_6e64;
    let mut positions = Vec::with_capacity(width * width);
    let mut uvs = Vec::with_capacity(width * width);
    let mut valid = Vec::with_capacity(width * width);

    for z in 0..width {
        for x in 0..width {
            let point = minimum + Vec2::new(x as f32, z as f32) * DETAIL_PATCH_SPACING_METRES;
            let base_height = vista.presented_height_at(&environment.scene_digest, terrain, point);
            let radius = point.distance(centre);
            let morph = 1.0
                - terrain_smoothstep(
                    DETAIL_PATCH_MORPH_START_METRES,
                    DETAIL_PATCH_RADIUS_METRES - DETAIL_PATCH_SPACING_METRES * 1.5,
                    radius,
                );
            let relief = base_height.map_or(0.0, |_| {
                terrain_surface_relief(
                    seed,
                    point,
                    terrain,
                    ground,
                    environment,
                    vista,
                    tree_positions,
                    rock_influences,
                ) * morph
            });
            positions.push([point.x, base_height.unwrap_or_default() + relief, point.y]);
            uvs.push([
                (point.x / terrain.width() + 0.5).clamp(0.0, 1.0),
                (point.y / terrain.depth() + 0.5).clamp(0.0, 1.0),
            ]);
            valid.push(base_height.is_some());
        }
    }

    let mut indices = Vec::with_capacity(diameter_steps * diameter_steps * 6);
    for z in 0..diameter_steps {
        for x in 0..diameter_steps {
            let cell_centre = minimum
                + (Vec2::new(x as f32, z as f32) + Vec2::splat(0.5)) * DETAIL_PATCH_SPACING_METRES;
            if cell_centre.distance(centre) > DETAIL_PATCH_RADIUS_METRES {
                continue;
            }
            let i = z * width + x;
            if !valid[i] || !valid[i + 1] || !valid[i + width] || !valid[i + width + 1] {
                continue;
            }
            let i = i as u32;
            indices.extend_from_slice(&[
                i,
                i + width as u32 + 1,
                i + 1,
                i,
                i + width as u32,
                i + width as u32 + 1,
            ]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.with_computed_area_weighted_normals()
}

#[derive(Debug, Clone, Copy)]
struct TerrainShapeSample {
    downhill: Vec2,
    slope: f32,
    concavity: f32,
}

fn terrain_surface_relief(
    seed: u64,
    point: Vec2,
    terrain: &SceneTerrain,
    ground: Option<&SceneGround>,
    environment: &SceneEnvironment,
    vista: &ActiveVistaSurface,
    tree_positions: &[Vec2],
    rock_influences: &[DetailRockInfluence],
) -> f32 {
    let surface = ground.and_then(|ground| ground.ground_at(point));
    if surface.is_some_and(|surface| surface.substrate == GroundSubstrate::Water) {
        return 0.0;
    }
    if surface.is_some_and(|surface| surface.substrate == GroundSubstrate::Road) {
        return road_surface_relief(seed, point, ground.expect("road came from SceneGround"));
    }

    let broad = signed_ground_noise(seed ^ 0x6272_6f61_645f_0001, point / 3.2) * 0.024;
    let fine = signed_ground_noise(seed ^ 0x6669_6e65_5f00_0002, point / 0.92) * 0.009;
    let clod_strength = match surface.map(|surface| surface.substrate) {
        Some(GroundSubstrate::Stone) => 0.0,
        Some(GroundSubstrate::Gravel) => 0.25,
        Some(GroundSubstrate::Mud | GroundSubstrate::Soil) => 1.0,
        Some(GroundSubstrate::Road | GroundSubstrate::Water) => unreachable!(),
        None => 1.0,
    };
    let clods = terrain_clod_relief(seed, point) * clod_strength;
    let shape = terrain_shape_sample(
        &environment.scene_digest,
        terrain,
        vista,
        point,
        DETAIL_PATCH_SPACING_METRES * 3.0,
    );
    let process_relief = shape.map_or(0.0, |shape| {
        drainage_relief(seed, point, shape, environment)
            + soil_creep_relief(seed, point, shape)
            + shape.concavity.clamp(-0.018, 0.024)
    });
    let roots = tree_root_relief(seed, point, tree_positions);
    let rock_contact = boulder_ground_relief(seed, point, shape, rock_influences);
    let rock_strata = match surface.map(|surface| surface.substrate) {
        Some(GroundSubstrate::Stone) => rocky_substrate_relief(seed, point, shape, 1.0),
        Some(GroundSubstrate::Gravel) => rocky_substrate_relief(seed, point, shape, 0.62),
        _ => 0.0,
    };
    let substrate_strength = match surface.map(|surface| surface.substrate) {
        Some(GroundSubstrate::Stone) => 0.54,
        Some(GroundSubstrate::Gravel) => 0.68,
        Some(GroundSubstrate::Mud) => 0.78,
        Some(GroundSubstrate::Soil) => 1.0,
        Some(GroundSubstrate::Road | GroundSubstrate::Water) => unreachable!(),
        None => 0.62,
    };
    let cover_strength = match surface.map(|surface| surface.cover) {
        Some(GroundCover::Reeds) => 0.35,
        Some(GroundCover::LooseStone) => 0.78,
        _ => 1.0,
    };
    ((broad + fine + clods + process_relief) * substrate_strength * cover_strength
        + roots
        + rock_contact
        + rock_strata)
        .clamp(DETAIL_RELIEF_MINIMUM_METRES, DETAIL_RELIEF_MAXIMUM_METRES)
}

fn signed_ground_noise(seed: u64, point: Vec2) -> f32 {
    ground_mask_noise(seed, point) * 2.0 - 1.0
}

fn terrain_clod_relief(seed: u64, point: Vec2) -> f32 {
    let field = ground_mask_noise(seed ^ 0x636c_6f64_5f66_6c64, point / 0.58);
    terrain_smoothstep(0.69, 0.91, field) * 0.022 - 0.003
}

fn terrain_shape_sample(
    scene_digest: &str,
    terrain: &SceneTerrain,
    vista: &ActiveVistaSurface,
    point: Vec2,
    radius: f32,
) -> Option<TerrainShapeSample> {
    let centre = vista.presented_height_at(scene_digest, terrain, point)?;
    let east = vista.presented_height_at(scene_digest, terrain, point + Vec2::X * radius)?;
    let west = vista.presented_height_at(scene_digest, terrain, point - Vec2::X * radius)?;
    let north = vista.presented_height_at(scene_digest, terrain, point + Vec2::Y * radius)?;
    let south = vista.presented_height_at(scene_digest, terrain, point - Vec2::Y * radius)?;
    let gradient = Vec2::new(east - west, north - south) / (radius * 2.0);
    let slope = gradient.length();
    let downhill = if slope > 0.000_1 {
        -gradient / slope
    } else {
        Vec2::X
    };
    // Positive values are bowls where transported material plausibly settles;
    // negative values are exposed convexities. Keep this a small residual,
    // rather than changing the authoritative broad landform.
    let concavity = ((east + west + north + south) * 0.25 - centre) * 0.18;
    Some(TerrainShapeSample {
        downhill,
        slope,
        concavity,
    })
}

fn drainage_relief(
    seed: u64,
    point: Vec2,
    shape: TerrainShapeSample,
    environment: &SceneEnvironment,
) -> f32 {
    let slope_weight = terrain_smoothstep(0.012, 0.16, shape.slope);
    if slope_weight <= 0.0 {
        return 0.0;
    }
    let normal = Vec2::new(-shape.downhill.y, shape.downhill.x);
    let warp = signed_ground_noise(seed ^ 0x7269_6c6c_5f77_6172, point / 5.5) * 0.85;
    let spacing = 2.6 + ground_mask_noise(seed ^ 0x7269_6c6c_5f73_7063, point / 11.0) * 1.4;
    let across = point.dot(normal) + warp;
    let distance = periodic_distance(across, spacing);
    let channel = 1.0 - terrain_smoothstep(0.08, 0.34, distance);
    let shoulder =
        terrain_smoothstep(0.18, 0.42, distance) * (1.0 - terrain_smoothstep(0.42, 0.72, distance));
    let moisture = f32::from(environment.weather.ground_moisture_bps) / 10_000.0;
    (-channel * (0.026 + moisture * 0.012) + shoulder * 0.009) * slope_weight
}

fn soil_creep_relief(seed: u64, point: Vec2, shape: TerrainShapeSample) -> f32 {
    let slope_weight = terrain_smoothstep(0.035, 0.22, shape.slope);
    let warp = signed_ground_noise(seed ^ 0x6372_6565_705f_7772, point / 7.0) * 0.55;
    let downhill_coordinate = point.dot(shape.downhill) + warp;
    let distance = periodic_distance(downhill_coordinate, 3.1);
    let ridge = 1.0 - terrain_smoothstep(0.12, 0.52, distance);
    ridge * 0.019 * slope_weight
}

/// Exposed rock is organized into broad ledges with sparse intersecting
/// fractures. The wavelengths stay above the detail grid spacing, so this
/// adds readable form rather than sub-pixel noise.
fn rocky_substrate_relief(
    seed: u64,
    point: Vec2,
    shape: Option<TerrainShapeSample>,
    strength: f32,
) -> f32 {
    let fallback_angle = unit_hash(seed ^ 0x7374_7261_7461_6469) * core::f32::consts::TAU;
    let downhill = shape
        .map(|shape| shape.downhill)
        .unwrap_or(Vec2::new(fallback_angle.cos(), fallback_angle.sin()));
    let across = Vec2::new(-downhill.y, downhill.x);
    let slope_weight = shape
        .map(|shape| terrain_smoothstep(0.018, 0.18, shape.slope))
        .unwrap_or(0.35);
    let warp = signed_ground_noise(seed ^ 0x7374_7261_7461_7772, point / 6.5) * 0.72;
    let contour = point.dot(downhill) + warp;
    let ledge_distance = periodic_distance(contour, 2.15);
    let shelf =
        (1.0 - terrain_smoothstep(0.08, 0.48, ledge_distance)) * (0.019 + slope_weight * 0.029);

    let fracture_a = periodic_distance(
        point.dot(across) + signed_ground_noise(seed ^ 0x6672_6163_7475_7261, point / 4.8) * 0.4,
        3.7,
    );
    let diagonal = (across * 0.72 + downhill * 0.69).normalize_or_zero();
    let fracture_b = periodic_distance(
        point.dot(diagonal) + signed_ground_noise(seed ^ 0x6672_6163_7475_7262, point / 5.6) * 0.34,
        5.3,
    );
    let crack = (1.0 - terrain_smoothstep(0.035, 0.17, fracture_a))
        .max((1.0 - terrain_smoothstep(0.035, 0.15, fracture_b)) * 0.72);
    (shelf - crack * 0.019) * strength
}

/// Deforms only the render-detail residual around boulders. The authoritative
/// terrain and collider remain unchanged: a shallow socket, contact apron,
/// and downhill debris tail visually seat each generated rock in the landform.
fn boulder_ground_relief(
    seed: u64,
    point: Vec2,
    shape: Option<TerrainShapeSample>,
    rocks: &[DetailRockInfluence],
) -> f32 {
    let mut relief = 0.0;
    for rock in rocks {
        let offset = point - rock.centre;
        let distance = offset.length();
        let radius = rock.radius.max(0.35);
        if distance > radius * 5.0 {
            continue;
        }
        let rock_seed = splitmix64(
            seed ^ u64::from(rock.centre.x.to_bits()).rotate_left(29)
                ^ u64::from(rock.centre.y.to_bits()),
        );
        let fallback_angle = unit_hash(rock_seed) * core::f32::consts::TAU;
        let downhill = shape
            .map(|shape| shape.downhill)
            .unwrap_or(Vec2::new(fallback_angle.cos(), fallback_angle.sin()));
        let across_axis = Vec2::new(-downhill.y, downhill.x);

        let socket = (1.0 - terrain_smoothstep(radius * 0.48, radius * 1.08, distance)) * -0.042;
        let apron = terrain_smoothstep(radius * 0.72, radius * 1.04, distance)
            * (1.0 - terrain_smoothstep(radius * 1.04, radius * 1.72, distance))
            * 0.033;

        let downstream = offset.dot(downhill);
        let across = offset.dot(across_axis).abs();
        let tail_length = radius * (3.2 + unit_hash(splitmix64(rock_seed)) * 1.1);
        let longitudinal = terrain_smoothstep(radius * 0.45, radius * 0.95, downstream)
            * (1.0 - terrain_smoothstep(tail_length * 0.62, tail_length, downstream));
        let tail_width = radius * 0.42 + downstream.max(0.0) * 0.24;
        let lateral = 1.0 - terrain_smoothstep(tail_width * 0.42, tail_width, across);
        let granular = 0.72
            + ground_mask_noise(
                rock_seed ^ 0x6465_6272_6973_746c,
                Vec2::new(downstream / 1.7, across / 0.8),
            ) * 0.28;
        let debris_tail = longitudinal * lateral * granular * 0.034;

        relief += socket + apron + debris_tail;
    }
    relief.clamp(-0.055, 0.07)
}

fn periodic_distance(value: f32, period: f32) -> f32 {
    let wrapped = value.rem_euclid(period);
    wrapped.min(period - wrapped)
}

fn tree_root_relief(seed: u64, point: Vec2, tree_positions: &[Vec2]) -> f32 {
    let mut relief = 0.0;
    for &tree in tree_positions {
        let offset = point - tree;
        let radius = offset.length();
        if radius > 8.0 {
            continue;
        }
        let tree_seed = splitmix64(
            seed ^ u64::from(tree.x.to_bits()).rotate_left(23) ^ u64::from(tree.y.to_bits()),
        );
        let mound = (-(radius / 1.35).powi(2)).exp() * 0.045;
        let basin = terrain_smoothstep(0.9, 1.8, radius)
            * (1.0 - terrain_smoothstep(5.2, 7.7, radius))
            * -0.012;
        let angle = offset.y.atan2(offset.x);
        let mut ridges = 0.0_f32;
        for root in 0..7_u64 {
            let root_seed = splitmix64(tree_seed ^ root.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let origin = unit_hash(root_seed) * core::f32::consts::TAU;
            let phase = unit_hash(splitmix64(root_seed)) * core::f32::consts::TAU;
            let length = 4.8 + unit_hash(splitmix64(root_seed ^ 0x6c65_6e67)) * 2.9;
            if radius > length || radius < 0.28 {
                continue;
            }
            let curved_angle = origin + (radius * 0.9 + phase).sin() * 0.11;
            let angular_distance = wrapped_angle_difference(angle, curved_angle).abs() * radius;
            let width = 0.16 + radius * 0.045;
            let ridge = 1.0 - terrain_smoothstep(width * 0.3, width, angular_distance);
            let taper = 1.0 - terrain_smoothstep(length * 0.58, length, radius);
            ridges = ridges.max(ridge * taper * (0.082 - radius * 0.007).max(0.025));
        }
        relief += mound + basin + ridges;
    }
    relief.clamp(-0.02, 0.115)
}

fn wrapped_angle_difference(left: f32, right: f32) -> f32 {
    (left - right + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU)
        - core::f32::consts::PI
}

fn road_surface_relief(seed: u64, point: Vec2, ground: &SceneGround) -> f32 {
    let is_road = |sample: Vec2| {
        ground
            .ground_at(sample)
            .is_some_and(|surface| surface.substrate == GroundSubstrate::Road)
    };
    let mut tangent = Vec2::X;
    let mut best_score = -1_i32;
    for direction_index in 0..8 {
        let angle = direction_index as f32 * core::f32::consts::PI / 8.0;
        let candidate = Vec2::new(angle.cos(), angle.sin());
        let score = [0.75_f32, 1.5, 2.5]
            .into_iter()
            .map(|distance| {
                i32::from(is_road(point + candidate * distance))
                    + i32::from(is_road(point - candidate * distance))
            })
            .sum();
        if score > best_score {
            best_score = score;
            tangent = candidate;
        }
    }
    let normal = Vec2::new(-tangent.y, tangent.x);
    let edge_distance = |direction: f32| {
        (1..=24)
            .map(|step| step as f32 * 0.25)
            .find(|distance| !is_road(point + normal * direction * *distance))
            .unwrap_or(6.0)
    };
    let positive_edge = edge_distance(1.0);
    let negative_edge = edge_distance(-1.0);
    let half_width = ((positive_edge + negative_edge) * 0.5).max(0.6);
    let across = (negative_edge - positive_edge) * 0.5;
    let rut_offset = (half_width * 0.46).clamp(0.38, 0.82);
    let rut_width = (half_width * 0.12).clamp(0.14, 0.27);
    let gaussian = |distance: f32| (-(distance / rut_width).powi(2) * 1.7).exp();
    let ruts = gaussian(across - rut_offset) + gaussian(across + rut_offset);
    let crown = (1.0 - (across / half_width).powi(2)).max(0.0) * 0.026;
    let travelled = point.dot(tangent);
    let irregularity = signed_ground_noise(
        seed ^ 0x726f_6164_5f72_7574,
        Vec2::new(travelled / 2.4, across / 1.1),
    ) * 0.004;
    (crown - ruts * 0.038 + irregularity).clamp(-0.048, 0.032)
}

fn terrain_smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(in crate::presentation) fn terrain_material(
    terrain: &SceneTerrain,
    environment: &SceneEnvironment,
    ground: Option<&SceneGround>,
    procedural_assets: &ProceduralEnvironmentAssets,
    images: &mut Assets<Image>,
) -> TacticalTerrainMaterial {
    TacticalTerrainMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.92,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalTerrainExtension {
            base_color: color_vec4(scene_ground_color(environment)),
            // Match the rendered optical average of the sward rather than its
            // brighter pre-lighting blade pigment.
            grass_color: color_vec4(grass_terminal_pigment(environment)),
            cover: Vec4::new(
                bps(environment.canopy_bps),
                bps(environment.wetland_bps),
                bps(environment.cultivation_bps),
                bps(environment.water_bps),
            ),
            weather: Vec4::new(
                bps(environment.weather.ground_moisture_bps),
                bps(environment.weather.snow_cover_bps),
                bps(environment.hilly_bps),
                bps(environment.weather.wind_speed_bps),
            ),
            // Beyond the geometric grass range, retain a band-limited sward
            // response in the terrain material instead of paying for blades
            // that project to less than a pixel. x/y are the fade interval;
            // z is environment-dependent coverage and w is reserved.
            far_sward: Vec4::new(
                TERMINAL_SWARD_FADE_START_METRES,
                TERMINAL_SWARD_FADE_END_METRES,
                (1.0 - bps(environment.water_bps) * 0.9
                    - bps(environment.weather.snow_cover_bps) * 0.8)
                    .clamp(0.0, 1.0),
                0.0,
            ),
            // x/y are the authoritative playable half extents. The detail
            // patch alone can extend beyond them; z controls its discrete
            // substrate-to-vista-sward handoff and w is reserved.
            playable_bounds: Vec4::new(terrain.width() * 0.5, terrain.depth() * 0.5, 4.0, 0.0),
            // x marks the coarse base material. Its shader removes only the
            // safely covered interior beneath the signed detail patch; the
            // outer overlap remains coplanar and morphs continuously.
            detail_patch: Vec4::new(1.0, DETAIL_PATCH_BASE_CUTOUT_RADIUS_METRES, 0.0, 0.0),
            // x is tile repetitions per metre, y is the decoded physical
            // height range, z scales the derivative normal, and w is the
            // distance where sub-centimetre detail is completely absent.
            soil_detail: Vec4::new(
                1.0 / FOREST_SOIL_TILE_METRES,
                FOREST_SOIL_HEIGHT_RANGE_METRES,
                1.0,
                24.0,
            ),
            // Dense leaf litter is an aggregate terrain material first. Its
            // packed height/AO/tone/coverage map supplies the continuous
            // forest-floor mass beneath sparse silhouette-breaking meshes.
            litter_detail: Vec4::new(
                1.0 / FOREST_LITTER_TILE_METRES,
                FOREST_LITTER_HEIGHT_RANGE_METRES,
                0.72,
                32.0,
            ),
            ground_map: images.add(ground_map_image(
                ground,
                stable_text_seed(&environment.scene_digest),
            )),
            soil_height_ao: procedural_assets.forest_soil.height_ao.clone(),
            litter_surface: procedural_assets.forest_soil.litter_surface.clone(),
            litter_normal: procedural_assets.forest_soil.litter_normal.clone(),
        },
    }
}

const GROUND_PRESENTATION_SAMPLES_PER_CELL: usize = 6;

fn ground_surface_pixel(sample: GroundSurface) -> [u8; 4] {
    let cover = match sample.cover {
        GroundCover::Bare => 0,
        GroundCover::TallGrass => 1,
        GroundCover::LeafLitter => 2,
        GroundCover::LooseStone => 3,
        GroundCover::Reeds => 4,
    };
    let substrate = match sample.substrate {
        GroundSubstrate::Soil => 0,
        GroundSubstrate::Stone => 1,
        GroundSubstrate::Gravel => 2,
        GroundSubstrate::Mud => 3,
        GroundSubstrate::Road => 4,
        GroundSubstrate::Water => 5,
    };
    [
        cover,
        substrate,
        (u32::from(sample.cover_density_bps) * 255 / 10_000) as u8,
        sample.cover_height_cm.min(255) as u8,
    ]
}

fn chamfer_distance_to(
    width: usize,
    height: usize,
    sources: impl Fn(usize) -> bool,
    maximum: usize,
) -> Vec<usize> {
    let mut distance = (0..width * height)
        .map(|index| if sources(index) { 0 } else { maximum + 1 })
        .collect::<Vec<_>>();
    for z in 0..height {
        for x in 0..width {
            let index = z * width + x;
            for (dx, dz) in [(-1_isize, 0_isize), (0, -1), (-1, -1), (1, -1)] {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx >= 0 && nz >= 0 && nx < width as isize && nz < height as isize {
                    distance[index] = distance[index]
                        .min(distance[nz as usize * width + nx as usize].saturating_add(1));
                }
            }
        }
    }
    for z in (0..height).rev() {
        for x in (0..width).rev() {
            let index = z * width + x;
            for (dx, dz) in [(1_isize, 0_isize), (0, 1), (1, 1), (-1, 1)] {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx >= 0 && nz >= 0 && nx < width as isize && nz < height as isize {
                    distance[index] = distance[index]
                        .min(distance[nz as usize * width + nx as usize].saturating_add(1));
                }
            }
        }
    }
    distance
}

fn encode_canopy_floor_distance(
    ground: &SceneGround,
    width: usize,
    height: usize,
    pixels: &mut [u8],
) {
    let metres_per_pixel = ground.grid_scale() / GROUND_PRESENTATION_SAMPLES_PER_CELL as f32;
    let inner_radius = (2.2 / metres_per_pixel).ceil().max(1.0) as usize;
    let outer_radius = (4.8 / metres_per_pixel).ceil().max(1.0) as usize;
    let litter = pixels
        .chunks_exact(4)
        .map(|pixel| pixel[0] == GroundCover::LeafLitter as u8)
        .collect::<Vec<_>>();
    let distance_to_litter =
        chamfer_distance_to(width, height, |index| litter[index], outer_radius);
    let distance_to_other =
        chamfer_distance_to(width, height, |index| !litter[index], inner_radius);
    for index in 0..width * height {
        let encoded = if litter[index] {
            let depth = (distance_to_other[index] as f32 / inner_radius as f32).clamp(0.0, 1.0);
            128.0 + depth * 127.0
        } else {
            let proximity =
                (1.0 - distance_to_litter[index] as f32 / outer_radius as f32).clamp(0.0, 1.0);
            proximity * 127.0
        };
        // Alpha is presentation-only. It carries signed distance from the
        // organic litter boundary instead of duplicating gameplay cover
        // height, which remains authoritative in SceneGround.
        pixels[index * 4 + 3] = encoded.round() as u8;
    }
}

fn ground_mask_noise(seed: u64, point: Vec2) -> f32 {
    let cell = point.floor();
    let local = point - cell;
    let curve = local * local * (Vec2::splat(3.0) - local * 2.0);
    let hash = |offset: Vec2| {
        let coordinate = cell + offset;
        let x = i64::from(coordinate.x as i32) as u64;
        let y = i64::from(coordinate.y as i32) as u64;
        unit_hash(splitmix64(
            seed ^ x.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ y.wrapping_mul(0xbf58_476d_1ce4_e5b9),
        ))
    };
    let bottom = hash(Vec2::ZERO).lerp(hash(Vec2::X), curve.x);
    let top = hash(Vec2::Y).lerp(hash(Vec2::ONE), curve.x);
    bottom.lerp(top, curve.y)
}

pub(super) fn organic_ground_pixels(ground: &SceneGround, seed: u64) -> (u32, u32, Vec<u8>) {
    let source_width = ground.grid_width();
    let source_depth = ground.grid_depth();
    let width = (source_width - 1) * GROUND_PRESENTATION_SAMPLES_PER_CELL + 1;
    let depth = (source_depth - 1) * GROUND_PRESENTATION_SAMPLES_PER_CELL + 1;
    let mut pixels = Vec::with_capacity(width * depth * 4);
    let scale = GROUND_PRESENTATION_SAMPLES_PER_CELL as f32;
    for z in 0..depth {
        for x in 0..width {
            let point = Vec2::new(x as f32 / scale, z as f32 / scale);
            let broad_point = point * 0.38;
            let fine_point = point * 0.93;
            let broad_warp = Vec2::new(
                ground_mask_noise(seed ^ 0x2f31_9a87, broad_point),
                ground_mask_noise(seed ^ 0x91b7_43cd, broad_point + Vec2::new(17.3, -9.1)),
            ) * 2.0
                - Vec2::ONE;
            let fine_warp = Vec2::new(
                ground_mask_noise(seed ^ 0x6d25_e9f1, fine_point + Vec2::new(31.7, 5.9)),
                ground_mask_noise(seed ^ 0xc4ab_1283, fine_point + Vec2::new(-7.7, 23.1)),
            ) * 2.0
                - Vec2::ONE;
            let warped = point + broad_warp * 0.78 + fine_warp * 0.22;
            let source_x = (warped.x.round() as isize).clamp(0, source_width as isize - 1) as usize;
            let source_z = (warped.y.round() as isize).clamp(0, source_depth as isize - 1) as usize;
            pixels.extend_from_slice(&ground_surface_pixel(
                ground.samples()[source_z * source_width + source_x],
            ));
        }
    }
    encode_canopy_floor_distance(ground, width, depth, &mut pixels);
    (width as u32, depth as u32, pixels)
}

pub(super) fn grass_cover_mask_pixels(ground: &SceneGround, seed: u64) -> (u32, u32, Vec<u8>) {
    let (width, height, ground_pixels) = organic_ground_pixels(ground, seed);
    let mut mask = vec![0_u8; width as usize * height as usize];
    let metres_per_pixel = ground.grid_scale() / GROUND_PRESENTATION_SAMPLES_PER_CELL as f32;
    let radius = (4.8 / metres_per_pixel).ceil().max(1.0) as usize;
    let width_usize = width as usize;
    let height_usize = height as usize;
    // The playable rectangle is a data-authority boundary, not a vegetation
    // boundary. Only authored non-grass pixels seed this distance field.
    let distance = chamfer_distance_to(
        width_usize,
        height_usize,
        |index| ground_pixels[index * 4] != GroundCover::TallGrass as u8,
        radius,
    );
    for z in 0..height as usize {
        for x in 0..width as usize {
            let pixel = (z * width as usize + x) * 4;
            if ground_pixels[pixel] != GroundCover::TallGrass as u8 {
                continue;
            }
            let density = ground_pixels[pixel + 2];
            let feather = (distance[z * width_usize + x] as f32 / radius as f32)
                .clamp(0.0, 1.0)
                .powf(1.28);
            mask[z * width as usize + x] = (f32::from(density) * feather) as u8;
        }
    }
    (width, height, mask)
}

pub(super) fn grass_cover_mask_image(ground: &SceneGround, seed: u64) -> Image {
    let (width, height, mask) = grass_cover_mask_pixels(ground, seed);
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        mask,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

fn ground_map_image(ground: Option<&SceneGround>, seed: u64) -> Image {
    let (width, height, pixels) = ground.map_or_else(
        || (1, 1, vec![0, 0, 0, 0]),
        |ground| organic_ground_pixels(ground, seed),
    );
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    // The shader rounds the interpolated enum channel back to one exact cover
    // kind before selecting a material. Linear filtering therefore smooths
    // the baked contour itself without producing a visible colour gradient.
    image.sampler = ImageSampler::linear();
    image
}

pub(in crate::presentation) fn legacy_scene_environment(id: &SceneId) -> SceneEnvironment {
    let (canopy_bps, hilly_bps, cultivation_bps) = match id.0.as_str() {
        "hills" => (1_200, 7_000, 0),
        "desert" => (0, 1_500, 0),
        value => {
            warn!("Unknown legacy scene: {value}");
            (0, 0, 0)
        }
    };
    SceneEnvironment {
        scene_digest: id.0.clone(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        latitude_microdegrees: 53_500_000,
        longitude_microdegrees: 10_000_000,
        absolute_minute: 12 * 60,
        absolute_elevation_metres: 20,
        weather: WeatherSnapshot {
            rules_version: WEATHER_RULES_VERSION,
            interval_start_minute: 0,
            cell_latitude: 0,
            cell_longitude: 0,
            temperature_deci_c: 100,
            wind_speed_bps: 1_500,
            precipitation: Precipitation::Clear,
            intensity_bps: 0,
            ground_moisture_bps: 0,
            snow_cover_bps: 0,
            atmosphere: Default::default(),
        },
        canopy_bps,
        wetland_bps: 0,
        cultivation_bps,
        water_bps: 0,
        hilly_bps,
    }
}

pub(super) fn on_environment_added(event: On<Add, SceneEnvironment>, mut commands: Commands) {
    commands
        .entity(event.entity)
        .insert(PendingTerrainPresentation);
}

pub(super) fn on_ground_added(event: On<Add, SceneGround>, mut commands: Commands) {
    commands
        .entity(event.entity)
        .insert(PendingTerrainPresentation);
}

const TERRAIN_SHADER: &str = "shaders/tactical_terrain.wgsl";

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use bevy::asset::{AssetApp, AssetPlugin};

    #[test]
    fn ordinary_dirt_uses_one_palette_color_regardless_of_canopy() {
        let mut environment = legacy_scene_environment(&SceneId("dirt-palette".into()));
        environment.weather.ground_moisture_bps = 0;
        environment.weather.snow_cover_bps = 0;
        environment.wetland_bps = 0;
        environment.cultivation_bps = 0;
        environment.water_bps = 0;
        let expected = Color::srgb_u8(
            TACTICAL_DIRT_SRGB[0],
            TACTICAL_DIRT_SRGB[1],
            TACTICAL_DIRT_SRGB[2],
        );

        environment.canopy_bps = 0;
        assert_eq!(scene_ground_color(&environment), expected);
        environment.canopy_bps = 10_000;
        assert_eq!(scene_ground_color(&environment), expected);
    }

    #[test]
    fn ground_shader_layers_parallax_litter_and_normal_over_one_planar_soil_sample() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_terrain.wgsl"
        ))
        .replace("\r\n", "\n");
        assert!(shader.contains("var ground_map: texture_2d<f32>"));
        assert!(shader.contains("var soil_height_ao: texture_2d<f32>"));
        assert!(shader.contains("var litter_surface: texture_2d<f32>"));
        assert!(shader.contains("var litter_normal_map: texture_2d<f32>"));
        assert_eq!(
            shader.matches("textureSample(soil_height_ao,").count(),
            1,
            "forest soil should add exactly one packed texture fetch"
        );
        assert_eq!(
            shader
                .matches("textureSample(\n        litter_surface,")
                .count(),
            2
        );
        assert_eq!(
            shader
                .matches("textureSample(\n        litter_normal_map,")
                .count(),
            1
        );
        assert!(shader.contains("pbr_input.material.base_color = vec4<f32>(color, 1.0)"));
        assert!(shader.contains("distance(position, view.lod_view_world_position.xyz)"));
        assert!(shader.contains("distance(position.xz, view.lod_view_world_position.xz)"));
        assert!(shader.contains("let soil_uv = position.xz * terrain.soil_detail.x"));
        assert!(shader.contains("let height_dx = dpdx(height_metres)"));
        assert!(shader.contains("pbr_input.diffuse_occlusion *= mix(1.0, soil_sample.g"));
        assert!(!shader.contains("select(color, terrain.grass_color.rgb, tall_grass > 0.5)"));
        assert!(!shader.contains("shaded_substrate"));
        assert!(!shader.contains("tall_grass > 0.5 && canopy_floor"));
        assert!(shader.contains("let canopy_floor = smoothstep(0.14, 0.72"));
        assert!(shader.contains("let litter_color = mix("));
        assert!(shader.contains("litter_region * litter_sample.a"));
        assert!(shader.contains("let parallax_offset = clamp("));
        assert!(shader.contains("let litter_mapped_normal = normalize("));
        assert!(shader.contains("let sward_color = terrain.grass_color.rgb"));
        assert!(!shader.contains("sward_color = color *"));
        assert!(shader.contains("sward_dither < sward_amount"));
        assert!(shader.contains("abs(position.x) - terrain.playable_bounds.x"));
        assert!(shader.contains("sward_dither < outside_sward"));
        assert!(shader.contains("terrain.detail_patch.x > 0.5"));
        assert!(shader.contains("discard"));
        assert!(!shader.contains("sward_amount >= 0.5"));
        for forbidden in [
            "dirt_diffuse",
            "dirt_normal_gl",
            "dirt_arm",
            "procedural_normal",
            "mapped_dirt_normal",
        ] {
            assert!(!shader.contains(forbidden), "found {forbidden}");
        }
    }

    #[test]
    fn terminal_terrain_sward_starts_when_the_final_grass_lod_fades() {
        let vista = grass_lod_visibility(GrassMeshLod::Vista);
        assert_eq!(vista.end_margin.start, TERMINAL_SWARD_FADE_START_METRES);
        assert_eq!(vista.end_margin.end, TERMINAL_SWARD_FADE_END_METRES);
    }

    use bevy::prelude::TaskPoolPlugin;

    #[test]
    fn terrain_presentation_remains_pending_until_terrain_arrives() {
        let mut app = App::new();
        app.add_observer(on_game_scene_added);
        let scene = app.world_mut().spawn(SceneId("add-order".into())).id();
        app.update();
        assert!(
            app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );

        app.world_mut()
            .entity_mut(scene)
            .insert(SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).expect("valid terrain"));
        app.update();
        assert!(
            app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );
    }

    #[test]
    fn terrain_reconcile_updates_once_and_retries_a_stale_material_handle() {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
        app.init_asset::<Image>();
        app.init_asset::<Mesh>();
        app.init_asset::<TacticalTerrainMaterial>();
        app.init_resource::<ActiveVistaSurface>();
        let procedural_assets = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            generate_procedural_environment_assets(&mut images)
        };
        app.insert_resource(procedural_assets);
        app.add_observer(on_game_scene_added);
        app.add_observer(on_environment_added);
        app.add_observer(on_ground_added);
        app.add_systems(Update, present_pending_terrain);
        let ground = SceneGround::from_samples(2, 2, 1.0, vec![GroundSurface::default(); 4])
            .expect("valid ground");
        let scene = app
            .world_mut()
            .spawn((
                SceneId("lifecycle".into()),
                SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).expect("valid terrain"),
                legacy_scene_environment(&SceneId("lifecycle".into())),
                ground,
            ))
            .id();
        app.update();

        assert!(
            !app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );
        let mut presentation_query = app.world_mut().query::<(
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        )>();
        let presentations = presentation_query
            .iter(app.world())
            .filter(|(source, _)| source.0 == scene)
            .map(|(_, handle)| handle.0.clone())
            .collect::<Vec<_>>();
        assert_eq!(presentations.len(), 2);
        assert_ne!(presentations[0], presentations[1]);
        let mut base_query = app.world_mut().query_filtered::<(
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        ), With<TerrainMaterialPresentation>>();
        let handle = base_query
            .iter(app.world())
            .find(|(source, _)| source.0 == scene)
            .unwrap()
            .1
            .0
            .clone();

        app.world_mut()
            .entity_mut(scene)
            .insert(PendingTerrainPresentation);
        app.update();
        let mut presentation_query = app.world_mut().query::<(
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        )>();
        let refreshed = presentation_query
            .iter(app.world())
            .filter(|(source, _)| source.0 == scene)
            .map(|(_, material)| material.0.clone())
            .collect::<Vec<_>>();
        assert_eq!(refreshed.len(), 2);
        assert!(refreshed.contains(&handle));

        app.world_mut()
            .resource_mut::<Assets<TacticalTerrainMaterial>>()
            .remove(&handle);
        app.update();
        let image_count = app.world().resource::<Assets<Image>>().len();
        app.world_mut()
            .entity_mut(scene)
            .insert(PendingTerrainPresentation);
        app.update();
        assert!(
            app.world()
                .entity(scene)
                .contains::<PendingTerrainPresentation>()
        );
        assert_eq!(app.world().resource::<Assets<Image>>().len(), image_count);
    }

    #[test]
    fn camera_local_detail_patch_is_bounded_deterministic_and_morphs_flat_at_its_edge() {
        let terrain = SceneTerrain::new(64, 64, 1.0, |point| point.x * 0.01 + point.y * 0.02);
        let environment = legacy_scene_environment(&SceneId("detail-patch".into()));
        let vista = ActiveVistaSurface::default();
        let first =
            terrain_detail_patch_mesh(&terrain, None, &environment, &vista, Vec2::ZERO, &[], &[]);
        let repeated =
            terrain_detail_patch_mesh(&terrain, None, &environment, &vista, Vec2::ZERO, &[], &[]);
        let positions = match first.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            VertexAttributeValues::Float32x3(values) => values,
            other => panic!("unexpected positions {other:?}"),
        };
        let repeated_positions = match repeated.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            VertexAttributeValues::Float32x3(values) => values,
            other => panic!("unexpected positions {other:?}"),
        };
        assert_eq!(positions, repeated_positions);
        assert_eq!(positions.len(), 161 * 161);
        let triangle_count = first.indices().unwrap().len() / 3;
        assert!(
            (39_000..=41_000).contains(&triangle_count),
            "{triangle_count}"
        );
        assert!(
            DETAIL_PATCH_RADIUS_METRES - DETAIL_PATCH_BASE_CUTOUT_RADIUS_METRES
                > DETAIL_PATCH_SNAP_METRES * core::f32::consts::FRAC_1_SQRT_2
                    + DETAIL_PATCH_SPACING_METRES * 2.0,
            "the coarse cutout must stay inside the snapped circular patch"
        );

        let mut minimum_relief = f32::INFINITY;
        let mut maximum_relief = f32::NEG_INFINITY;
        for position in positions {
            let point = Vec2::new(position[0], position[2]);
            let relief = position[1] - terrain.height_at(point).unwrap();
            assert!((-0.075_01..=0.105_01).contains(&relief), "relief {relief}");
            minimum_relief = minimum_relief.min(relief);
            maximum_relief = maximum_relief.max(relief);
        }
        assert!(minimum_relief < -0.005, "signed relief needs depressions");
        assert!(
            maximum_relief > 0.025,
            "relief was not visually meaningful: {maximum_relief}"
        );
        let normals = match first.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap() {
            VertexAttributeValues::Float32x3(values) => values,
            other => panic!("unexpected normals {other:?}"),
        };
        assert!(
            normals
                .iter()
                .any(|normal| Vec2::new(normal[0], normal[2]).length() > 0.03),
            "refined geometry must change grazing-angle lighting"
        );

        for position in positions {
            let point = Vec2::new(position[0], position[2]);
            if point.length() < DETAIL_PATCH_RADIUS_METRES - DETAIL_PATCH_SPACING_METRES * 1.5 {
                continue;
            }
            let expected = terrain.height_at(point).unwrap();
            assert!((position[1] - expected).abs() < 0.000_01);
        }
    }

    #[test]
    fn detail_relief_preserves_water_flatness_and_builds_a_crowned_rutted_road() {
        let terrain = SceneTerrain::new(16, 16, 1.0, |_| 0.0);
        let environment = legacy_scene_environment(&SceneId("surface-processes".into()));
        let vista = ActiveVistaSurface::default();
        let water = SceneGround::uniform_for_terrain(
            &terrain,
            GroundSurface {
                substrate: GroundSubstrate::Water,
                ..default()
            },
        );
        assert_eq!(
            terrain_surface_relief(
                42,
                Vec2::ZERO,
                &terrain,
                Some(&water),
                &environment,
                &vista,
                &[],
                &[],
            ),
            0.0
        );

        let width = 41;
        let scale = 0.25;
        let samples = (0..width * width)
            .map(|index| {
                let z = index / width;
                let world_z = z as f32 * scale - (width - 1) as f32 * scale * 0.5;
                GroundSurface {
                    substrate: if world_z.abs() <= 1.5 {
                        GroundSubstrate::Road
                    } else {
                        GroundSubstrate::Soil
                    },
                    ..default()
                }
            })
            .collect();
        let road = SceneGround::from_samples(width, width, scale, samples).unwrap();
        let crown = road_surface_relief(42, Vec2::ZERO, &road);
        let left_rut = (1..=12)
            .map(|step| road_surface_relief(42, Vec2::new(0.0, -(step as f32) * 0.1), &road))
            .fold(f32::INFINITY, f32::min);
        let right_rut = (1..=12)
            .map(|step| road_surface_relief(42, Vec2::new(0.0, step as f32 * 0.1), &road))
            .fold(f32::INFINITY, f32::min);
        assert!(crown > 0.015, "road needs a readable crown: {crown}");
        assert!(left_rut < -0.005, "left wheel rut missing: {left_rut}");
        assert!(right_rut < -0.005, "right wheel rut missing: {right_rut}");
    }

    #[test]
    fn rocky_relief_builds_ledges_and_seats_boulders_with_downhill_debris() {
        let shape = TerrainShapeSample {
            downhill: Vec2::X,
            slope: 0.14,
            concavity: 0.0,
        };
        let strata = (-32..=32)
            .map(|step| {
                rocky_substrate_relief(91, Vec2::new(step as f32 * 0.125, 0.37), Some(shape), 1.0)
            })
            .collect::<Vec<_>>();
        let strata_range = strata.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - strata.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(
            strata_range > 0.02,
            "bedrock ledges and fractures need readable relief: {strata_range}"
        );

        let rock = DetailRockInfluence {
            centre: Vec2::ZERO,
            radius: 1.5,
        };
        let socket = boulder_ground_relief(91, Vec2::ZERO, Some(shape), &[rock]);
        let contact_apron = boulder_ground_relief(91, Vec2::new(1.5, 0.0), Some(shape), &[rock]);
        let downstream = boulder_ground_relief(91, Vec2::new(2.2, 0.0), Some(shape), &[rock]);
        let upstream = boulder_ground_relief(91, Vec2::new(-2.2, 0.0), Some(shape), &[rock]);
        assert!(socket < -0.035, "boulder socket missing: {socket}");
        assert!(
            contact_apron > 0.015,
            "boulder contact apron missing: {contact_apron}"
        );
        assert!(
            downstream > upstream + 0.012,
            "debris must trail downhill: downstream={downstream}, upstream={upstream}"
        );
    }

    #[test]
    fn drainage_and_creep_are_directional_and_tree_roots_form_long_coherent_ridges() {
        let shape = TerrainShapeSample {
            downhill: Vec2::X,
            slope: 0.12,
            concavity: 0.0,
        };
        let environment = legacy_scene_environment(&SceneId("directional-relief".into()));
        let along_flow = (0..20)
            .map(|step| {
                drainage_relief(71, Vec2::new(step as f32 * 0.25, 0.0), shape, &environment)
            })
            .collect::<Vec<_>>();
        let across_flow = (0..20)
            .map(|step| {
                drainage_relief(71, Vec2::new(0.0, step as f32 * 0.25), shape, &environment)
            })
            .collect::<Vec<_>>();
        let range = |samples: &[f32]| {
            samples.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - samples.iter().copied().fold(f32::INFINITY, f32::min)
        };
        assert!(
            range(&across_flow) > range(&along_flow) * 1.35,
            "rills should vary more across flow than along it"
        );
        assert!(across_flow.iter().any(|relief| *relief < -0.012));
        assert!(soil_creep_relief(71, Vec2::new(0.0, 0.0), shape) >= 0.0);

        let tree = Vec2::new(2.0, -1.0);
        let radial_samples = (0..360)
            .map(|degree| {
                let angle = degree as f32 * core::f32::consts::PI / 180.0;
                tree_root_relief(
                    71,
                    tree + Vec2::new(angle.cos(), angle.sin()) * 2.4,
                    &[tree],
                )
            })
            .collect::<Vec<_>>();
        assert!(radial_samples.iter().copied().fold(0.0, f32::max) > 0.045);
        assert_eq!(tree_root_relief(71, tree + Vec2::splat(8.0), &[tree]), 0.0);
    }

    #[test]
    fn presentation_ground_mask_is_deterministic_discrete_and_not_grid_aligned() {
        let mut samples = vec![GroundSurface::default(); 7 * 7];
        for z in 0..7 {
            for x in 3..7 {
                samples[z * 7 + x] = GroundSurface {
                    cover: GroundCover::LeafLitter,
                    cover_density_bps: 9_200,
                    cover_height_cm: 6,
                    ..default()
                };
            }
        }
        let ground = SceneGround::from_samples(7, 7, 2.0, samples).expect("valid ground");
        let (width, depth, pixels) = organic_ground_pixels(&ground, 42);
        let repeated = organic_ground_pixels(&ground, 42);
        let changed = organic_ground_pixels(&ground, 43);
        assert_eq!((width, depth), (37, 37));
        assert_eq!((width, depth, pixels.clone()), repeated);
        assert_ne!(pixels, changed.2);

        let valid_surfaces = [
            ground_surface_pixel(GroundSurface::default()),
            ground_surface_pixel(GroundSurface {
                cover: GroundCover::LeafLitter,
                cover_density_bps: 9_200,
                cover_height_cm: 6,
                ..default()
            }),
        ];
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| valid_surfaces.iter().any(|valid| pixel[..3] == valid[..3]))
        );
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] == GroundCover::LeafLitter as u8 && pixel[3] > 180),
            "deep litter must encode an interior loam zone"
        );
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] != GroundCover::LeafLitter as u8 && pixel[3] > 0),
            "open cover must retain an exterior canopy transition"
        );

        let first_leaf_litter_x = (0..depth as usize)
            .filter_map(|z| {
                pixels
                    .chunks_exact(4)
                    .skip(z * width as usize)
                    .take(width as usize)
                    .position(|pixel| pixel[0] == 2)
            })
            .collect::<BTreeSet<_>>();
        assert!(
            first_leaf_litter_x.len() >= 5,
            "organic boundary should cross rows at varied positions: {first_leaf_litter_x:?}"
        );
    }

    #[test]
    fn grass_cover_mask_is_deterministic_and_feathers_authoritative_edges() {
        let grass = GroundSurface {
            cover: GroundCover::TallGrass,
            cover_density_bps: 10_000,
            cover_height_cm: 82,
            ..default()
        };
        let mut samples = vec![grass; 17 * 17];
        for z in 7..10 {
            for x in 7..10 {
                samples[z * 17 + x].cover = GroundCover::LeafLitter;
            }
        }
        let ground = SceneGround::from_samples(17, 17, 2.0, samples).unwrap();
        let image = grass_cover_mask_image(&ground, 91);
        let repeated = grass_cover_mask_image(&ground, 91);
        let values = image.data.as_deref().unwrap();
        assert_eq!(image.data, repeated.data);
        assert!(values.contains(&0), "non-grass must reject every blade");
        assert!(
            values.iter().copied().max().unwrap_or_default() > 200,
            "deep grass must retain most blades"
        );
        assert!(
            values.iter().any(|value| (1..255).contains(value)),
            "grass-side boundary must be progressively sparse"
        );
        assert!(values[0] > 200 && values[values.len() - 1] > 200);
    }
}
