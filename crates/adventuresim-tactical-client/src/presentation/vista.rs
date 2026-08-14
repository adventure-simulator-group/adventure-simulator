use super::*;

/// Marker for a distant tree billboard spawned as part of a vista ring.
#[derive(Component)]
pub(crate) struct VistaTreePresentation;

pub(super) fn on_scene_vista_bundle(
    bundle: On<SceneVistaBundle>,
    mut commands: Commands,
    existing: Query<Entity, With<VistaTerrain>>,
    playable_scenes: Query<(&SceneTerrain, &SceneEnvironment)>,
    settings: Res<TacticalGraphicsSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalVistaMaterial>>,
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut vista_tree_cache: ResMut<VistaTreePresentationCache>,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let visible_lods = bundle
        .lods
        .iter()
        .take(settings.max_vista_lods)
        .collect::<Vec<_>>();
    let playable_scene = playable_scenes
        .iter()
        .find(|(_, environment)| environment.scene_digest == bundle.scene_digest);
    let playable_terrain = playable_scene.map(|(terrain, _)| terrain);
    let weather = playable_scene
        .map(|(_, environment)| environment.weather)
        .unwrap_or_else(clear_vista_weather);
    let material = materials.add(vista_material(weather));
    if playable_scene.is_none() {
        warn!(
            scene_digest = %bundle.scene_digest,
            "Tactical vista arrived before its authoritative playable terrain; edge stitching is unavailable"
        );
    }
    let mut inner_half_extent = bundle.playable_half_extent_metres;
    for (index, lod) in visible_lods.iter().copied().enumerate() {
        let meshes_for_lod = vista_lod_meshes_with_morph(
            lod,
            inner_half_extent,
            visible_lods.get(index + 1).copied(),
            (index == 0).then_some(playable_terrain).flatten(),
            weather,
        );
        if meshes_for_lod.is_empty() {
            warn!(level = lod.level, "Rejected malformed tactical vista LOD");
            continue;
        }
        let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
        for (chunk, mesh) in meshes_for_lod.into_iter().enumerate() {
            commands.spawn((
                Name::new(format!("Tactical vista LOD {} chunk {chunk}", lod.level)),
                VistaTerrain(lod.level),
                NotShadowCaster,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(
                    lod.origin_east_metres as f32,
                    0.0,
                    lod.origin_north_metres as f32,
                ),
            ));
        }
        if index <= 1 {
            spawn_vista_trees(
                &mut commands,
                lod,
                visible_lods.get(index + 1).copied(),
                inner_half_extent,
                &bundle.scene_digest,
                &mut meshes,
                &mut tree_materials,
                &mut images,
                &mut vista_tree_cache,
            );
        }
        inner_half_extent = Vec2::new(
            half_extent,
            f32::from(lod.depth.saturating_sub(1)) * lod.spacing_metres * 0.5,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_vista_trees(
    commands: &mut Commands,
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_half_extent: Vec2,
    scene_digest: &str,
    meshes: &mut Assets<Mesh>,
    tree_materials: &mut Assets<TacticalTreeImpostorMaterial>,
    images: &mut Assets<Image>,
    cache: &mut VistaTreePresentationCache,
) {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    let center = Vec2::new((width - 1) as f32, (depth - 1) as f32) * 0.5;
    let scene_seed = stable_text_seed(scene_digest);
    for z in 0..depth - 1 {
        for x in 0..width - 1 {
            let sample = lod.environment[z * width + x];
            let canopy = bps(sample.canopy_bps)
                * (1.0 - bps(sample.water_bps))
                * (1.0 - bps(sample.cultivation_bps) * 0.85);
            // A regional source cell represents a stand, not individual
            // stems. Keep a physical-area-scaled silhouette sample; the
            // terrain material carries the remaining aggregate canopy.
            let cell_key = ((x as u64) << 32) | z as u64;
            let candidate_count = vista_tree_candidate_count(
                canopy,
                lod.spacing_metres,
                splitmix64(scene_seed ^ cell_key ^ 0x74c3_019d),
            )
            .min(if lod.spacing_metres <= 50.0 {
                usize::MAX
            } else {
                3
            });
            if candidate_count == 0 {
                continue;
            }
            let cell_min = (Vec2::new(x as f32, z as f32) - center) * lod.spacing_metres;
            for candidate in 0..candidate_count {
                let hash = splitmix64(
                    scene_seed ^ cell_key ^ (candidate as u64).wrapping_mul(0x9e37_79b9),
                );
                let local = cell_min
                    + Vec2::new(unit_hash(hash), unit_hash(splitmix64(hash ^ 0x51b7_2d8a)))
                        * lod.spacing_metres;
                if local.x.abs() <= playable_half_extent.x + 7.0
                    && local.y.abs() <= playable_half_extent.y + 7.0
                {
                    continue;
                }
                let world = local
                    + Vec2::new(
                        lod.origin_east_metres as f32,
                        lod.origin_north_metres as f32,
                    );
                let Some(height) = presented_height_at(lod, world, coarser_lod) else {
                    continue;
                };
                // Vista stands share one calibrated whole-tree atlas. Scale,
                // rotation-independent view selection, and placement still
                // break repetition without baking during every source cell.
                let variant_seed = splitmix64(0x6f61_6b00);
                let cached = ensure_vista_tree_variant(
                    variant_seed,
                    0.5,
                    meshes,
                    tree_materials,
                    images,
                    cache,
                );
                // Each atlas represents the visible crown mass of a small
                // stand at regional distance, not a survey-accurate stem.
                let stand_scale = if lod.spacing_metres <= 50.0 {
                    1.0
                } else {
                    1.65
                };
                let scale = (1.05 + unit_hash(splitmix64(hash ^ 0xa29c_413d)) * 0.75) * stand_scale;
                commands.spawn((
                    Name::new("Distant vista oak billboard"),
                    VistaTerrain(lod.level),
                    VistaTreePresentation,
                    NoFrustumCulling,
                    NotShadowCaster,
                    Mesh3d(cached.mesh.clone()),
                    MeshMaterial3d(cached.material.clone()),
                    cached.provenance.clone(),
                    Transform::from_xyz(local.x, height, local.y).with_scale(Vec3::splat(scale)),
                ));
            }
        }
    }
}

fn vista_tree_candidate_count(canopy: f32, spacing_metres: f32, seed: u64) -> usize {
    let expected = canopy.clamp(0.0, 1.0) * spacing_metres * spacing_metres / 3_200.0;
    expected.floor() as usize + usize::from(unit_hash(seed) < expected.fract())
}

#[cfg(test)]
pub(super) fn vista_lod_meshes(lod: &VistaLod, inner_half_extent: Vec2) -> Vec<Mesh> {
    vista_lod_meshes_with_morph(lod, inner_half_extent, None, None, clear_vista_weather())
}

fn vista_lod_meshes_with_morph(
    lod: &VistaLod,
    inner_half_extent: Vec2,
    coarser_lod: Option<&VistaLod>,
    playable_terrain: Option<&SceneTerrain>,
    weather: WeatherSnapshot,
) -> Vec<Mesh> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    if width < 2
        || depth < 2
        || width.checked_mul(depth).is_none_or(|samples| {
            lod.heights_metres.len() != samples || lod.environment.len() != samples
        })
        || !lod.spacing_metres.is_finite()
        || lod.spacing_metres <= 0.0
    {
        return Vec::new();
    }
    let center_x = (width - 1) as f32 * 0.5;
    let center_z = (depth - 1) as f32 * 0.5;
    let mut meshes = Vec::new();
    for chunk_z in (0..depth - 1).step_by(VISTA_CHUNK_CELLS) {
        for chunk_x in (0..width - 1).step_by(VISTA_CHUNK_CELLS) {
            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut colors = Vec::new();
            let mut indices = Vec::new();
            for z in chunk_z..(chunk_z + VISTA_CHUNK_CELLS).min(depth - 1) {
                for x in chunk_x..(chunk_x + VISTA_CHUNK_CELLS).min(width - 1) {
                    let cell_min = Vec2::new(
                        (x as f32 - center_x) * lod.spacing_metres,
                        (z as f32 - center_z) * lod.spacing_metres,
                    );
                    let cell_max = cell_min + Vec2::splat(lod.spacing_metres);
                    for rectangle in cell_rectangles_outside_inner_rectangle(
                        cell_min,
                        cell_max,
                        inner_half_extent,
                    ) {
                        for [minimum_x, maximum_x, minimum_z, maximum_z] in
                            subdivide_playable_boundary_rectangle(
                                rectangle,
                                inner_half_extent,
                                playable_terrain,
                            )
                        {
                            let vertex = |local: Vec2| {
                                let world = local
                                    + Vec2::new(
                                        lod.origin_east_metres as f32,
                                        lod.origin_north_metres as f32,
                                    );
                                let height = presented_vista_vertex_height(
                                    lod,
                                    coarser_lod,
                                    playable_terrain,
                                    local,
                                    inner_half_extent,
                                )
                                .expect("clipped vista vertex remains inside its source LOD");
                                let delta = lod.spacing_metres.min(100.0);
                                let height_offset = |offset: Vec2| {
                                    presented_vista_vertex_height(
                                        lod,
                                        coarser_lod,
                                        playable_terrain,
                                        local + offset,
                                        inner_half_extent,
                                    )
                                    .unwrap_or(height)
                                };
                                let tangent_x = Vec3::new(
                                    delta * 2.0,
                                    height_offset(Vec2::X * delta)
                                        - height_offset(-Vec2::X * delta),
                                    0.0,
                                );
                                let tangent_z = Vec3::new(
                                    0.0,
                                    height_offset(Vec2::Y * delta)
                                        - height_offset(-Vec2::Y * delta),
                                    delta * 2.0,
                                );
                                (
                                    [local.x, height, local.y],
                                    tangent_z.cross(tangent_x).normalize().to_array(),
                                    presented_color_at(lod, world, coarser_lod, weather).expect(
                                        "clipped vista color remains inside its source LOD",
                                    ),
                                )
                            };
                            let base = positions.len() as u32;
                            let vertices = [
                                vertex(Vec2::new(minimum_x, minimum_z)),
                                vertex(Vec2::new(maximum_x, minimum_z)),
                                vertex(Vec2::new(maximum_x, maximum_z)),
                                vertex(Vec2::new(minimum_x, maximum_z)),
                            ];
                            positions.extend(vertices.map(|vertex| vertex.0));
                            normals.extend(vertices.map(|vertex| vertex.1));
                            colors.extend(vertices.map(|vertex| vertex.2));
                            indices.extend_from_slice(&[
                                base,
                                base + 2,
                                base + 1,
                                base,
                                base + 3,
                                base + 2,
                            ]);
                        }
                    }
                }
            }
            if positions.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_indices(Indices::U32(indices));
            meshes.push(mesh);
        }
    }
    meshes
}

fn presented_vista_vertex_height(
    lod: &VistaLod,
    coarser_lod: Option<&VistaLod>,
    playable_terrain: Option<&SceneTerrain>,
    local: Vec2,
    playable_half_extent: Vec2,
) -> Option<f32> {
    let world = local
        + Vec2::new(
            lod.origin_east_metres as f32,
            lod.origin_north_metres as f32,
        );
    let vista_height = presented_height_at(lod, world, coarser_lod)?;
    Some(playable_terrain.map_or(vista_height, |terrain| {
        stitch_vista_height_to_playable_edge(
            terrain,
            local,
            playable_half_extent,
            lod.spacing_metres,
            vista_height,
        )
    }))
}

fn subdivide_playable_boundary_rectangle(
    rectangle: [f32; 4],
    playable_half_extent: Vec2,
    terrain: Option<&SceneTerrain>,
) -> Vec<[f32; 4]> {
    let Some(terrain) = terrain else {
        return vec![rectangle];
    };
    let [minimum_x, maximum_x, minimum_z, maximum_z] = rectangle;
    let epsilon = terrain.grid_scale() * 0.01;
    if (minimum_x - playable_half_extent.x).abs() <= epsilon
        || (maximum_x + playable_half_extent.x).abs() <= epsilon
    {
        return split_rectangle_axis(
            rectangle,
            1,
            terrain.depth() * -0.5,
            terrain.grid_scale(),
            terrain.grid_depth(),
        );
    }
    if (minimum_z - playable_half_extent.y).abs() <= epsilon
        || (maximum_z + playable_half_extent.y).abs() <= epsilon
    {
        return split_rectangle_axis(
            rectangle,
            0,
            terrain.width() * -0.5,
            terrain.grid_scale(),
            terrain.grid_width(),
        );
    }
    vec![rectangle]
}

fn split_rectangle_axis(
    rectangle: [f32; 4],
    axis: usize,
    terrain_minimum: f32,
    spacing: f32,
    sample_count: usize,
) -> Vec<[f32; 4]> {
    let (minimum, maximum) = if axis == 0 {
        (rectangle[0], rectangle[1])
    } else {
        (rectangle[2], rectangle[3])
    };
    let mut boundaries = vec![minimum, maximum];
    boundaries.extend((0..sample_count).filter_map(|index| {
        let coordinate = terrain_minimum + index as f32 * spacing;
        (coordinate > minimum && coordinate < maximum).then_some(coordinate)
    }));
    boundaries.sort_by(f32::total_cmp);
    boundaries.dedup_by(|left, right| (*left - *right).abs() < spacing * 0.001);
    boundaries
        .windows(2)
        .map(|interval| {
            let mut split = rectangle;
            if axis == 0 {
                split[0] = interval[0];
                split[1] = interval[1];
            } else {
                split[2] = interval[0];
                split[3] = interval[1];
            }
            split
        })
        .collect()
}

fn stitch_vista_height_to_playable_edge(
    terrain: &SceneTerrain,
    local: Vec2,
    playable_half_extent: Vec2,
    transition_width: f32,
    vista_height: f32,
) -> f32 {
    let boundary = local.clamp(-playable_half_extent, playable_half_extent);
    let Some(playable_height) = terrain.height_at(boundary) else {
        return vista_height;
    };
    let outside_distance = (local.abs() - playable_half_extent)
        .max(Vec2::ZERO)
        .max_element();
    let vista_weight = (outside_distance / transition_width.max(f32::EPSILON)).clamp(0.0, 1.0);
    playable_height.lerp(vista_height, vista_weight)
}

fn cell_rectangles_outside_inner_rectangle(
    minimum: Vec2,
    maximum: Vec2,
    inner_half_extent: Vec2,
) -> Vec<[f32; 4]> {
    if inner_half_extent.x <= 0.0
        || inner_half_extent.y <= 0.0
        || maximum.x <= -inner_half_extent.x
        || minimum.x >= inner_half_extent.x
        || maximum.y <= -inner_half_extent.y
        || minimum.y >= inner_half_extent.y
    {
        return vec![[minimum.x, maximum.x, minimum.y, maximum.y]];
    }
    if minimum.x >= -inner_half_extent.x
        && maximum.x <= inner_half_extent.x
        && minimum.y >= -inner_half_extent.y
        && maximum.y <= inner_half_extent.y
    {
        return Vec::new();
    }

    let mut rectangles = Vec::with_capacity(4);
    if minimum.x < -inner_half_extent.x {
        rectangles.push([
            minimum.x,
            maximum.x.min(-inner_half_extent.x),
            minimum.y,
            maximum.y,
        ]);
    }
    if maximum.x > inner_half_extent.x {
        rectangles.push([
            minimum.x.max(inner_half_extent.x),
            maximum.x,
            minimum.y,
            maximum.y,
        ]);
    }
    let middle_minimum_x = minimum.x.max(-inner_half_extent.x);
    let middle_maximum_x = maximum.x.min(inner_half_extent.x);
    if middle_minimum_x < middle_maximum_x {
        if minimum.y < -inner_half_extent.y {
            rectangles.push([
                middle_minimum_x,
                middle_maximum_x,
                minimum.y,
                maximum.y.min(-inner_half_extent.y),
            ]);
        }
        if maximum.y > inner_half_extent.y {
            rectangles.push([
                middle_minimum_x,
                middle_maximum_x,
                minimum.y.max(inner_half_extent.y),
                maximum.y,
            ]);
        }
    }
    rectangles
}

#[cfg(test)]
fn presented_height(
    lod: &VistaLod,
    x: usize,
    z: usize,
    world: Vec2,
    coarser_lod: Option<&VistaLod>,
) -> f32 {
    let own = lod.heights_metres[z * usize::from(lod.width) + x];
    let Some(coarser) = coarser_lod else {
        return own;
    };
    let weight = lod_transition_weight(lod, coarser, world);
    sample_vista_height(coarser, world)
        .map(|height| own.lerp(height, weight))
        .unwrap_or(own)
}

fn presented_height_at(lod: &VistaLod, world: Vec2, coarser_lod: Option<&VistaLod>) -> Option<f32> {
    let own = sample_vista_height(lod, world)?;
    let Some(coarser) = coarser_lod else {
        return Some(own);
    };
    let weight = lod_transition_weight(lod, coarser, world);
    if weight <= 0.0 {
        return Some(own);
    }
    Some(
        sample_vista_height(coarser, world)
            .map(|height| own.lerp(height, weight))
            .unwrap_or(own),
    )
}

#[cfg(test)]
fn presented_color(
    lod: &VistaLod,
    x: usize,
    z: usize,
    world: Vec2,
    coarser_lod: Option<&VistaLod>,
    weather: WeatherSnapshot,
) -> [f32; 4] {
    let own = vista_sample_color(lod.environment[z * usize::from(lod.width) + x], weather);
    let Some(coarser) = coarser_lod else {
        return own.to_array();
    };
    let weight = lod_transition_weight(lod, coarser, world);
    sample_vista_color(coarser, world, weather)
        .map(|color| own.lerp(color, weight))
        .unwrap_or(own)
        .to_array()
}

fn presented_color_at(
    lod: &VistaLod,
    world: Vec2,
    coarser_lod: Option<&VistaLod>,
    weather: WeatherSnapshot,
) -> Option<[f32; 4]> {
    let own = sample_vista_color(lod, world, weather)?;
    let Some(coarser) = coarser_lod else {
        return Some(own.to_array());
    };
    let weight = lod_transition_weight(lod, coarser, world);
    Some(
        sample_vista_color(coarser, world, weather)
            .map(|color| own.lerp(color, weight))
            .unwrap_or(own)
            .to_array(),
    )
}

fn lod_transition_weight(lod: &VistaLod, coarser: &VistaLod, world: Vec2) -> f32 {
    let center = Vec2::new(
        lod.origin_east_metres as f32,
        lod.origin_north_metres as f32,
    );
    let half_extent = f32::from(lod.width.saturating_sub(1)) * lod.spacing_metres * 0.5;
    // Begin morphing one coarse sample before the boundary. A one-fine-cell
    // band still exposes the square footprint whenever adjacent LOD spacing
    // grows rapidly (50 m -> 250 m -> 1 km).
    let transition_width = coarser
        .spacing_metres
        .min(half_extent)
        .max(lod.spacing_metres);
    let radius = (world - center).abs().max_element();
    ((radius - (half_extent - transition_width)) / transition_width).clamp(0.0, 1.0)
}

fn sample_vista_height(lod: &VistaLod, world: Vec2) -> Option<f32> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    let local = world
        - Vec2::new(
            lod.origin_east_metres as f32,
            lod.origin_north_metres as f32,
        );
    let coordinate =
        local / lod.spacing_metres + Vec2::new((width - 1) as f32 * 0.5, (depth - 1) as f32 * 0.5);
    if coordinate.x < 0.0
        || coordinate.y < 0.0
        || coordinate.x > (width - 1) as f32
        || coordinate.y > (depth - 1) as f32
    {
        return None;
    }
    let lower = coordinate.floor().as_uvec2();
    let upper = (lower + UVec2::ONE).min(UVec2::new(width as u32 - 1, depth as u32 - 1));
    let fraction = coordinate.fract();
    let at = |x: u32, z: u32| lod.heights_metres[z as usize * width + x as usize];
    let near = at(lower.x, lower.y).lerp(at(upper.x, lower.y), fraction.x);
    let far = at(lower.x, upper.y).lerp(at(upper.x, upper.y), fraction.x);
    Some(near.lerp(far, fraction.y))
}

fn vista_sample_color(sample: EnvironmentalSample, weather: WeatherSnapshot) -> Vec4 {
    let environment = SceneEnvironment {
        scene_digest: String::new(),
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        latitude_microdegrees: 53_500_000,
        longitude_microdegrees: 10_000_000,
        absolute_minute: 12 * 60,
        absolute_elevation_metres: 20,
        weather,
        canopy_bps: sample.canopy_bps,
        wetland_bps: sample.wetland_bps,
        cultivation_bps: sample.cultivation_bps,
        water_bps: sample.water_bps,
        hilly_bps: sample.hilly_bps,
    };
    let mut color = Vec4::from_array(scene_ground_color(&environment).to_linear().to_f32_array());
    let hills = bps(sample.hilly_bps);
    let snow = bps(weather.snow_cover_bps);
    let exposed_rock = hills
        * (1.0 - bps(sample.water_bps))
        * (1.0 - bps(sample.wetland_bps) * 0.8)
        * (1.0 - bps(sample.canopy_bps) * 0.45)
        * (1.0 - snow);
    let rock = Color::srgb_u8(104, 101, 91).to_linear().to_f32_array();
    color = color.lerp(Vec4::from_array(rock), exposed_rock * 0.62);
    color.w = vista_sward_coverage(sample) * (1.0 - snow * 0.92);
    color
}

fn vista_sward_coverage(sample: EnvironmentalSample) -> f32 {
    let surface = match sample.surface {
        TacticalSurface::Open | TacticalSurface::SparseWoods => 1.0,
        TacticalSurface::DeepWoods => 0.28,
        TacticalSurface::Wetland => 0.42,
        TacticalSurface::Road | TacticalSurface::Water => 0.0,
    };
    (surface
        * (1.0 - bps(sample.water_bps))
        * (1.0 - bps(sample.cultivation_bps) * 0.72)
        * (1.0 - bps(sample.hilly_bps) * 0.82))
        .clamp(0.0, 1.0)
}

fn sample_vista_color(lod: &VistaLod, world: Vec2, weather: WeatherSnapshot) -> Option<Vec4> {
    let width = usize::from(lod.width);
    let depth = usize::from(lod.depth);
    let local = world
        - Vec2::new(
            lod.origin_east_metres as f32,
            lod.origin_north_metres as f32,
        );
    let coordinate =
        local / lod.spacing_metres + Vec2::new((width - 1) as f32 * 0.5, (depth - 1) as f32 * 0.5);
    if coordinate.x < 0.0
        || coordinate.y < 0.0
        || coordinate.x > (width - 1) as f32
        || coordinate.y > (depth - 1) as f32
    {
        return None;
    }
    let lower = coordinate.floor().as_uvec2();
    let upper = (lower + UVec2::ONE).min(UVec2::new(width as u32 - 1, depth as u32 - 1));
    let fraction = coordinate.fract();
    let at = |x: u32, z: u32| {
        vista_sample_color(lod.environment[z as usize * width + x as usize], weather)
    };
    let near = at(lower.x, lower.y).lerp(at(upper.x, lower.y), fraction.x);
    let far = at(lower.x, upper.y).lerp(at(upper.x, upper.y), fraction.x);
    Some(near.lerp(far, fraction.y))
}

fn clear_vista_weather() -> WeatherSnapshot {
    WeatherSnapshot {
        rules_version: WEATHER_RULES_VERSION,
        interval_start_minute: 0,
        cell_latitude: 0,
        cell_longitude: 0,
        temperature_deci_c: 100,
        wind_speed_bps: 0,
        precipitation: Precipitation::Clear,
        intensity_bps: 0,
        ground_moisture_bps: 0,
        snow_cover_bps: 0,
    }
}

#[derive(Component)]
#[allow(dead_code)]
pub(crate) struct VistaTerrain(pub(crate) u8);

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalVistaExtension {
    #[uniform(100)]
    weather: Vec4,
}

impl MaterialExtension for TacticalVistaExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/tactical_vista.wgsl".into()
    }
}

pub(in crate::presentation) type TacticalVistaMaterial =
    ExtendedMaterial<StandardMaterial, TacticalVistaExtension>;

fn vista_material(weather: WeatherSnapshot) -> TacticalVistaMaterial {
    TacticalVistaMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.94,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalVistaExtension {
            weather: Vec4::new(
                bps(weather.ground_moisture_bps),
                bps(weather.snow_cover_bps),
                bps(weather.wind_speed_bps),
                0.0,
            ),
        },
    }
}

const VISTA_CHUNK_CELLS: usize = 8;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn vista_ground_uses_solid_palette_colors_and_geometry_normals() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_vista.wgsl"
        ));
        assert!(!shader.contains("texture_2d"));
        assert!(!shader.contains("textureSample"));
        assert!(!shader.contains("composed_normal"));
        assert!(shader.contains("let molded_rock = vec3<f32>(0.31, 0.30, 0.275)"));
    }

    #[test]
    fn vista_lods_build_independent_overlapping_rings() {
        let input = TacticalSceneInput::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/tactical-scenes/valley-distant-ridge.json"),
        )
        .unwrap();
        let mut inner = Vec2::splat(55.0);
        for (index, lod) in input.vista.lods.iter().enumerate() {
            let meshes = vista_lod_meshes(lod, inner);
            assert!(!meshes.is_empty());
            assert!(meshes.iter().all(|mesh| mesh.count_vertices() > 0));
            assert!(meshes.iter().all(|mesh| {
                mesh.count_vertices() <= VISTA_CHUNK_CELLS * VISTA_CHUNK_CELLS * 4 * 4
            }));
            if index > 0 {
                assert!(
                    meshes.len() > 1,
                    "regional LODs must be independently culled"
                );
            }
            inner = Vec2::new(
                f32::from(lod.width - 1) * lod.spacing_metres * 0.5,
                f32::from(lod.depth - 1) * lod.spacing_metres * 0.5,
            );
        }
    }

    #[test]
    fn coarse_vista_cells_are_clipped_to_the_playable_hole() {
        let lod = VistaLod {
            level: 0,
            spacing_metres: 250.0,
            width: 9,
            depth: 9,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![8.0; 81],
            environment: vec![EnvironmentalSample::default(); 81],
        };
        let inner_half_extent = Vec2::new(55.0, 42.0);
        let meshes = vista_lod_meshes(&lod, inner_half_extent);
        let mut touches_boundary = false;
        for mesh in meshes {
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("vista mesh must expose Float32x3 positions");
            };
            for quad in positions.chunks_exact(4) {
                let outside = quad
                    .iter()
                    .all(|position| position[0] <= -inner_half_extent.x)
                    || quad
                        .iter()
                        .all(|position| position[0] >= inner_half_extent.x)
                    || quad
                        .iter()
                        .all(|position| position[2] <= -inner_half_extent.y)
                    || quad
                        .iter()
                        .all(|position| position[2] >= inner_half_extent.y);
                assert!(
                    outside,
                    "vista quad overlaps the playable terrain: {quad:?}"
                );
                touches_boundary |= quad.iter().any(|position| {
                    (position[0].abs() - inner_half_extent.x).abs() < 0.001
                        || (position[2].abs() - inner_half_extent.y).abs() < 0.001
                });
            }
        }
        assert!(
            touches_boundary,
            "coarse cells must be split at the exact playable boundary"
        );
    }

    #[test]
    fn first_vista_ring_stitches_to_playable_height_then_blends_outward() {
        let terrain = SceneTerrain::from_heightmap(
            3,
            3,
            50.0,
            vec![12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 12.0],
        )
        .unwrap();
        let half_extent = Vec2::splat(50.0);

        assert_eq!(
            stitch_vista_height_to_playable_edge(
                &terrain,
                Vec2::new(50.0, 10.0),
                half_extent,
                250.0,
                112.0,
            ),
            12.0
        );
        assert_eq!(
            stitch_vista_height_to_playable_edge(
                &terrain,
                Vec2::new(175.0, 10.0),
                half_extent,
                250.0,
                112.0,
            ),
            62.0
        );
        assert_eq!(
            stitch_vista_height_to_playable_edge(
                &terrain,
                Vec2::new(300.0, 10.0),
                half_extent,
                250.0,
                112.0,
            ),
            112.0
        );
    }

    #[test]
    fn first_vista_ring_reuses_every_playable_boundary_sample() {
        let heights = (0..5)
            .flat_map(|z| (0..5).map(move |_| z as f32 * 7.0))
            .collect::<Vec<_>>();
        let terrain = SceneTerrain::from_heightmap(5, 5, 25.0, heights).unwrap();
        let lod = VistaLod {
            level: 0,
            spacing_metres: 250.0,
            width: 3,
            depth: 3,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![100.0; 9],
            environment: vec![EnvironmentalSample::default(); 9],
        };
        let half_extent = Vec2::splat(50.0);
        let meshes = vista_lod_meshes_with_morph(
            &lod,
            half_extent,
            None,
            Some(&terrain),
            clear_vista_weather(),
        );
        let mut east_edge = Vec::new();
        for mesh in meshes {
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("vista mesh must expose Float32x3 positions");
            };
            east_edge.extend(
                positions
                    .iter()
                    .copied()
                    .filter(|position| (position[0] - half_extent.x).abs() < 0.001),
            );
        }

        for sample in 0..terrain.grid_depth() {
            let z = -half_extent.y + sample as f32 * terrain.grid_scale();
            let expected_height = terrain.height_at(Vec2::new(half_extent.x, z)).unwrap();
            assert!(
                east_edge.iter().any(|position| {
                    (position[2] - z).abs() < 0.001 && (position[1] - expected_height).abs() < 0.001
                }),
                "vista edge omitted playable boundary sample z={z}, height={expected_height}"
            );
        }
    }

    #[test]
    fn finer_ring_morphs_onto_the_coarse_surface_at_its_outer_boundary() {
        let sample = EnvironmentalSample::default();
        let finer = VistaLod {
            level: 0,
            spacing_metres: 10.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![12.0; 25],
            environment: vec![sample; 25],
        };
        let coarse = VistaLod {
            level: 1,
            spacing_metres: 20.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![38.0; 25],
            environment: vec![sample; 25],
        };
        assert_eq!(
            presented_height(&finer, 4, 2, Vec2::new(20.0, 0.0), Some(&coarse)),
            38.0
        );
        assert_eq!(
            presented_height(&finer, 2, 2, Vec2::ZERO, Some(&coarse)),
            12.0
        );
        assert_eq!(
            presented_height(&finer, 3, 2, Vec2::new(15.0, 0.0), Some(&coarse)),
            31.5
        );
    }

    #[test]
    fn vista_vertex_colors_reuse_ground_palette_in_linear_space() {
        let open = EnvironmentalSample::default();
        let expected = scene_ground_color(&SceneEnvironment {
            scene_digest: String::new(),
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 12 * 60,
            absolute_elevation_metres: 20,
            weather: clear_vista_weather(),
            canopy_bps: 0,
            wetland_bps: 0,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        })
        .to_linear()
        .to_f32_array();
        assert_eq!(
            vista_sample_color(open, clear_vista_weather()).to_array(),
            expected
        );

        let lod = VistaLod {
            level: 0,
            spacing_metres: 10.0,
            width: 3,
            depth: 3,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![0.0; 9],
            environment: vec![open; 9],
        };
        assert!(
            vista_lod_meshes(&lod, Vec2::ZERO)
                .iter()
                .all(|mesh| mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some())
        );
    }

    #[test]
    fn distant_sward_respects_surface_and_land_cover() {
        let open = EnvironmentalSample::default();
        let deep_woods = EnvironmentalSample {
            surface: TacticalSurface::DeepWoods,
            ..open
        };
        let mountain = EnvironmentalSample {
            hilly_bps: 10_000,
            ..open
        };
        let road = EnvironmentalSample {
            surface: TacticalSurface::Road,
            ..open
        };
        assert_eq!(vista_sward_coverage(open), 1.0);
        assert!(vista_sward_coverage(deep_woods) < 0.3);
        assert!(vista_sward_coverage(mountain) < 0.2);
        assert_eq!(vista_sward_coverage(road), 0.0);
    }

    #[test]
    fn snow_palette_carries_into_vista_and_suppresses_sward() {
        let open = EnvironmentalSample::default();
        let clear = vista_sample_color(open, clear_vista_weather());
        let snow = vista_sample_color(
            open,
            WeatherSnapshot {
                snow_cover_bps: 10_000,
                precipitation: Precipitation::Snow,
                ..clear_vista_weather()
            },
        );
        assert!(snow.x > clear.x && snow.y > clear.y && snow.z > clear.z);
        assert!(snow.w < 0.1);
    }

    #[test]
    fn vista_tree_density_scales_with_physical_cell_area() {
        let small = (0..64_u64)
            .map(|seed| vista_tree_candidate_count(1.0, 50.0, splitmix64(seed)))
            .sum::<usize>();
        let large = (0..64_u64)
            .map(|seed| vista_tree_candidate_count(1.0, 100.0, splitmix64(seed)))
            .sum::<usize>();
        assert!(small > 0);
        assert!(large >= small * 3);
        assert_eq!(vista_tree_candidate_count(0.0, 250.0, 0), 0);
    }

    #[test]
    fn finer_color_morph_matches_coarse_color_at_outer_boundary() {
        let forest = EnvironmentalSample {
            canopy_bps: 8_000,
            ..default()
        };
        let cultivated = EnvironmentalSample {
            cultivation_bps: 8_000,
            ..default()
        };
        let finer = VistaLod {
            level: 0,
            spacing_metres: 10.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![0.0; 25],
            environment: vec![forest; 25],
        };
        let coarse = VistaLod {
            level: 1,
            spacing_metres: 20.0,
            width: 5,
            depth: 5,
            origin_east_metres: 0.0,
            origin_north_metres: 0.0,
            heights_metres: vec![0.0; 25],
            environment: vec![cultivated; 25],
        };
        assert_eq!(
            presented_color(
                &finer,
                4,
                2,
                Vec2::new(20.0, 0.0),
                Some(&coarse),
                clear_vista_weather(),
            ),
            vista_sample_color(cultivated, clear_vista_weather()).to_array()
        );
        assert_eq!(
            presented_color(
                &finer,
                2,
                2,
                Vec2::ZERO,
                Some(&coarse),
                clear_vista_weather(),
            ),
            vista_sample_color(forest, clear_vista_weather()).to_array()
        );
    }
}
