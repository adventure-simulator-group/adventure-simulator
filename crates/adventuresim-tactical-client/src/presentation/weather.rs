use super::terrain::terrain_heightmap_image;
use super::*;
use fabelgeist_determinism::splitmix64;

const WEATHER_SHADER: &str = "shaders/tactical_weather.wgsl";
const FALLING_PARTICLE_CAPACITY: usize = 3_072;
const IMPACT_PARTICLE_CAPACITY: usize = 384;
const DISTANT_SHEET_SEGMENTS: usize = 16;
const DISTANT_SHEET_LAYERS: usize = 6;
const WEATHER_OCCLUSION_RESOLUTION: u32 = 512;

const ATTRIBUTE_WEATHER_PARTICLE_DATA: MeshVertexAttribute =
    MeshVertexAttribute::new("WeatherParticleData", 2_180_101, VertexFormat::Float32x4);
const ATTRIBUTE_WEATHER_PARTICLE_CORNER: MeshVertexAttribute =
    MeshVertexAttribute::new("WeatherParticleCorner", 2_180_102, VertexFormat::Float32x2);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WeatherParticle {
    Falling,
    Impact,
    DistantSheet,
}

#[derive(Resource, Default)]
pub(in crate::presentation) struct WeatherOcclusionState {
    scene: Option<Entity>,
    tree_signature: Option<u64>,
    image: Option<Handle<Image>>,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalWeatherMaterial {
    /// Precipitation kind, intensity, wind strength, deterministic seed.
    #[uniform(0)]
    weather: Vec4,
    /// Horizontal wind direction, camera-local radius, fall volume height.
    #[uniform(1)]
    motion: Vec4,
    /// Playable half-width/depth and encoded minimum/maximum terrain height.
    #[uniform(2)]
    terrain: Vec4,
    /// Row-major playable heightfield encoded into two eight-bit channels.
    #[texture(3)]
    heightmap: Handle<Image>,
    /// Top-down woody shelter height, with alpha marking covered texels.
    #[texture(4)]
    occlusion_map: Handle<Image>,
}

impl Material for TacticalWeatherMaterial {
    fn vertex_shader() -> ShaderRef {
        WEATHER_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        WEATHER_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_WEATHER_PARTICLE_DATA.at_shader_location(1),
            ATTRIBUTE_WEATHER_PARTICLE_CORNER.at_shader_location(2),
        ])?];
        Ok(())
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects active weather scene data, presentation assets, and occlusion state independently"
)]
pub(super) fn apply_active_scene_weather(
    active: Res<ActiveTacticalScene>,
    scenes: Query<(&SceneEnvironment, &SceneTerrain)>,
    particles: Query<Entity, With<WeatherParticle>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalWeatherMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut occlusion: ResMut<WeatherOcclusionState>,
) {
    if !active.is_changed() {
        return;
    }
    for entity in &particles {
        commands.entity(entity).despawn();
    }
    occlusion.scene = active.entity;
    occlusion.tree_signature = None;
    occlusion.image = None;
    let Some((environment, terrain)) = active.entity.and_then(|entity| scenes.get(entity).ok())
    else {
        return;
    };
    if environment.weather.precipitation == Precipitation::Clear
        || environment.weather.intensity_bps == 0
    {
        return;
    }

    let heightmap = images.add(terrain_heightmap_image(terrain));
    let occlusion_map = images.add(empty_weather_occlusion_image());
    occlusion.image = Some(occlusion_map.clone());
    let falling_material = materials.add(weather_material(
        environment,
        terrain,
        heightmap.clone(),
        occlusion_map.clone(),
        WeatherParticle::Falling,
    ));
    commands.spawn((
        Name::new("GPU tactical falling precipitation"),
        WeatherParticle::Falling,
        NoFrustumCulling,
        NotShadowCaster,
        Mesh3d(meshes.add(weather_particle_mesh(FALLING_PARTICLE_CAPACITY))),
        MeshMaterial3d(falling_material),
        Transform::default(),
    ));

    if environment.weather.precipitation == Precipitation::Rain {
        let impact_material = materials.add(weather_material(
            environment,
            terrain,
            heightmap.clone(),
            occlusion_map.clone(),
            WeatherParticle::Impact,
        ));
        commands.spawn((
            Name::new("GPU tactical rain impacts"),
            WeatherParticle::Impact,
            NoFrustumCulling,
            NotShadowCaster,
            Mesh3d(meshes.add(weather_particle_mesh(IMPACT_PARTICLE_CAPACITY))),
            MeshMaterial3d(impact_material),
            Transform::default(),
        ));

        if environment.weather.intensity_bps >= 8_000 {
            let sheet_material = materials.add(weather_material(
                environment,
                terrain,
                heightmap,
                occlusion_map,
                WeatherParticle::DistantSheet,
            ));
            commands.spawn((
                Name::new("GPU tactical distant rain sheets"),
                WeatherParticle::DistantSheet,
                NoFrustumCulling,
                NotShadowCaster,
                Mesh3d(meshes.add(weather_sheet_mesh(
                    DISTANT_SHEET_SEGMENTS,
                    DISTANT_SHEET_LAYERS,
                ))),
                MeshMaterial3d(sheet_material),
                Transform::default(),
            ));
        }
    }
}

fn weather_material(
    environment: &SceneEnvironment,
    terrain: &SceneTerrain,
    heightmap: Handle<Image>,
    occlusion_map: Handle<Image>,
    layer: WeatherParticle,
) -> TacticalWeatherMaterial {
    let kind = match (environment.weather.precipitation, layer) {
        (Precipitation::Rain, WeatherParticle::Falling) => 1.0,
        (Precipitation::Snow, WeatherParticle::Falling) => 2.0,
        (Precipitation::Rain, WeatherParticle::Impact) => 3.0,
        (Precipitation::Rain, WeatherParticle::DistantSheet) => 4.0,
        _ => 0.0,
    };
    let bearing = f32::from(environment.weather.atmosphere.wind_direction_degrees).to_radians();
    let seed = stable_text_seed(&environment.scene_digest)
        ^ environment.weather.interval_start_minute.rotate_left(17);
    let seed = (seed % 65_521) as f32;
    let radius = match layer {
        WeatherParticle::Impact => 18.0,
        WeatherParticle::Falling => 30.0,
        WeatherParticle::DistantSheet => 36.0,
    };
    TacticalWeatherMaterial {
        weather: Vec4::new(
            kind,
            bps(environment.weather.intensity_bps),
            bps(environment.weather.wind_speed_bps),
            seed,
        ),
        motion: Vec4::new(bearing.sin(), -bearing.cos(), radius, 24.0),
        terrain: Vec4::new(
            terrain.width() * 0.5,
            terrain.depth() * 0.5,
            terrain.minimum_height(),
            terrain.maximum_height(),
        ),
        heightmap,
        occlusion_map,
    }
}

fn weather_particle_mesh(capacity: usize) -> Mesh {
    let mut positions = Vec::with_capacity(capacity * 4);
    let mut particle_data = Vec::with_capacity(capacity * 4);
    let mut corners = Vec::with_capacity(capacity * 4);
    let mut indices = Vec::with_capacity(capacity * 6);
    const QUAD_CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    for index in 0..capacity {
        let seed_a = unit_hash(splitmix64(index as u64 ^ 0xa1b2_c3d4_e5f6_0718));
        let seed_b = unit_hash(splitmix64(index as u64 ^ 0x1827_3645_5a69_7887));
        let rank = (index as f32 + 0.5) / capacity as f32;
        let base = positions.len() as u32;
        for corner in QUAD_CORNERS {
            positions.push([0.0, 0.0, 0.0]);
            particle_data.push([seed_a, seed_b, rank, index as f32]);
            corners.push(corner);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(ATTRIBUTE_WEATHER_PARTICLE_DATA, particle_data);
    mesh.insert_attribute(ATTRIBUTE_WEATHER_PARTICLE_CORNER, corners);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn weather_sheet_mesh(segments: usize, layers: usize) -> Mesh {
    let capacity = segments * layers;
    let mut positions = Vec::with_capacity(capacity * 4);
    let mut particle_data = Vec::with_capacity(capacity * 4);
    let mut corners = Vec::with_capacity(capacity * 4);
    let mut indices = Vec::with_capacity(capacity * 6);
    const QUAD_CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    // Transparent primitives in a single draw are not depth-sorted for us.
    // Emit the distant shells first so their soft veils blend back-to-front.
    for layer in (0..layers).rev() {
        for segment in 0..segments {
            let angle = (segment as f32 + 0.5) / segments as f32;
            let layer_fraction = (layer as f32 + 0.5) / layers as f32;
            let rank = (segment as f32 + 0.5) / segments as f32;
            let seed = (layer * segments + segment) as f32;
            let base = positions.len() as u32;
            for corner in QUAD_CORNERS {
                positions.push([0.0, 0.0, 0.0]);
                particle_data.push([angle, layer_fraction, rank, seed]);
                corners.push(corner);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(ATTRIBUTE_WEATHER_PARTICLE_DATA, particle_data);
    mesh.insert_attribute(ATTRIBUTE_WEATHER_PARTICLE_CORNER, corners);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(super) fn update_weather_occlusion_map(
    active: Res<ActiveTacticalScene>,
    scenes: Query<(&SceneEnvironment, &SceneTerrain)>,
    trees: Query<(&GlobalTransform, &StreamedTreePresentation)>,
    tree_cache: Res<TreePresentationCache>,
    mut state: ResMut<WeatherOcclusionState>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(scene) = active.entity else {
        return;
    };
    let Some(image_handle) = state
        .scene
        .filter(|selected| *selected == scene)
        .and(state.image.as_ref())
    else {
        return;
    };
    let Ok((environment, terrain)) = scenes.get(scene) else {
        return;
    };
    if environment.weather.precipitation == Precipitation::Clear {
        return;
    }

    let signature = weather_occlusion_tree_signature(scene, &trees);
    if state.tree_signature == Some(signature) {
        return;
    }
    let Some(mut image) = images.get_mut(image_handle) else {
        return;
    };
    *image = weather_occlusion_image(terrain, &trees, &tree_cache);
    state.tree_signature = Some(signature);
}

fn weather_occlusion_tree_signature(
    scene: Entity,
    trees: &Query<(&GlobalTransform, &StreamedTreePresentation)>,
) -> u64 {
    let mut signature = splitmix64(scene.to_bits() ^ 0x7261_696e_5f6f_6363);
    let mut count = 0_u64;
    for (transform, presentation) in trees {
        let translation = transform.translation();
        let tree = presentation.weather_occlusion_cache_key()
            ^ u64::from(translation.x.to_bits()).rotate_left(11)
            ^ u64::from(translation.y.to_bits()).rotate_left(29)
            ^ u64::from(translation.z.to_bits()).rotate_left(47);
        signature ^= splitmix64(tree);
        count += 1;
    }
    splitmix64(signature ^ count.rotate_left(17))
}

fn empty_weather_occlusion_image() -> Image {
    Image::new(
        Extent3d {
            width: WEATHER_OCCLUSION_RESOLUTION,
            height: WEATHER_OCCLUSION_RESOLUTION,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; WEATHER_OCCLUSION_RESOLUTION as usize * WEATHER_OCCLUSION_RESOLUTION as usize * 4],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn weather_occlusion_image(
    terrain: &SceneTerrain,
    trees: &Query<(&GlobalTransform, &StreamedTreePresentation)>,
    tree_cache: &TreePresentationCache,
) -> Image {
    let pixel_count = WEATHER_OCCLUSION_RESOLUTION as usize * WEATHER_OCCLUSION_RESOLUTION as usize;
    let mut shelter_heights = vec![f32::NEG_INFINITY; pixel_count];
    for (transform, presentation) in trees {
        let Some(branches) =
            tree_cache.weather_occlusion_branches(presentation.weather_occlusion_cache_key())
        else {
            continue;
        };
        for branch in branches.iter().filter(|branch| branch.depth <= 2) {
            rasterize_weather_branch(&mut shelter_heights, terrain, transform, branch);
        }
    }

    let minimum = terrain.minimum_height() - 2.0;
    let range = terrain.maximum_height() + 24.0 - minimum;
    let mut pixels = vec![0; pixel_count * 4];
    for (index, height) in shelter_heights.into_iter().enumerate() {
        if !height.is_finite() {
            continue;
        }
        let encoded = (((height - minimum) / range).clamp(0.0, 1.0) * 65_535.0).round() as u16;
        pixels[index * 4] = (encoded & 0xff) as u8;
        pixels[index * 4 + 1] = (encoded >> 8) as u8;
        pixels[index * 4 + 3] = 255;
    }
    Image::new(
        Extent3d {
            width: WEATHER_OCCLUSION_RESOLUTION,
            height: WEATHER_OCCLUSION_RESOLUTION,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn rasterize_weather_branch(
    shelter_heights: &mut [f32],
    terrain: &SceneTerrain,
    transform: &GlobalTransform,
    branch: &TreeBranchSegment,
) {
    let start = transform.transform_point(branch.start);
    let end = transform.transform_point(branch.end);
    let scale = transform
        .to_scale_rotation_translation()
        .0
        .abs()
        .max_element();
    let start_radius = branch.start_radius * scale;
    let end_radius = branch.end_radius * scale;
    let cell_width = terrain.width() / WEATHER_OCCLUSION_RESOLUTION as f32;
    let cell_depth = terrain.depth() / WEATHER_OCCLUSION_RESOLUTION as f32;
    let footprint_margin = start_radius.max(end_radius) + cell_width.hypot(cell_depth) * 0.5;
    let minimum = start.xz().min(end.xz()) - Vec2::splat(footprint_margin);
    let maximum = start.xz().max(end.xz()) + Vec2::splat(footprint_margin);
    let to_texel = |value: f32, half_extent: f32, span: f32| {
        (((value + half_extent) / span) * WEATHER_OCCLUSION_RESOLUTION as f32).floor() as i32
    };
    let min_x = to_texel(minimum.x, terrain.width() * 0.5, terrain.width())
        .clamp(0, WEATHER_OCCLUSION_RESOLUTION as i32 - 1);
    let max_x = to_texel(maximum.x, terrain.width() * 0.5, terrain.width())
        .clamp(0, WEATHER_OCCLUSION_RESOLUTION as i32 - 1);
    let min_z = to_texel(minimum.y, terrain.depth() * 0.5, terrain.depth())
        .clamp(0, WEATHER_OCCLUSION_RESOLUTION as i32 - 1);
    let max_z = to_texel(maximum.y, terrain.depth() * 0.5, terrain.depth())
        .clamp(0, WEATHER_OCCLUSION_RESOLUTION as i32 - 1);
    if min_x > max_x || min_z > max_z {
        return;
    }

    let axis = end.xz() - start.xz();
    let axis_length_squared = axis.length_squared();
    let half_diagonal = cell_width.hypot(cell_depth) * 0.5;
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let point = Vec2::new(
                (x as f32 + 0.5) * cell_width - terrain.width() * 0.5,
                (z as f32 + 0.5) * cell_depth - terrain.depth() * 0.5,
            );
            let along = if axis_length_squared > 0.000_001 {
                ((point - start.xz()).dot(axis) / axis_length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let centre = start.xz() + axis * along;
            let radius = start_radius + (end_radius - start_radius) * along;
            if point.distance_squared(centre) > (radius + half_diagonal).powi(2) {
                continue;
            }
            let shelter_height = start.y + (end.y - start.y) * along + radius;
            let index = z as usize * WEATHER_OCCLUSION_RESOLUTION as usize + x as usize;
            shelter_heights[index] = shelter_heights[index].max(shelter_height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_mesh_is_one_indexed_quad_batch() {
        let mesh = weather_particle_mesh(17);
        assert_eq!(mesh.count_vertices(), 17 * 4);
        assert_eq!(mesh.indices().unwrap().len(), 17 * 6);
        assert!(mesh.attribute(ATTRIBUTE_WEATHER_PARTICLE_DATA).is_some());
        assert!(mesh.attribute(ATTRIBUTE_WEATHER_PARTICLE_CORNER).is_some());
    }

    #[test]
    fn heightmap_encoding_preserves_terrain_range() {
        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![-2.0, 0.0, 1.0, 4.0]).unwrap();
        let image = terrain_heightmap_image(&terrain);
        assert_eq!(image.texture_descriptor.size.width, 2);
        assert_eq!(image.texture_descriptor.size.height, 2);
        let pixels = image.data.as_deref().unwrap();
        let first = u16::from_le_bytes([pixels[0], pixels[1]]);
        let last = u16::from_le_bytes([pixels[12], pixels[13]]);
        assert_eq!(first, 0);
        assert_eq!(last, u16::MAX);
    }

    #[test]
    fn distant_sheet_mesh_batches_every_shell_panel() {
        let mesh = weather_sheet_mesh(8, 6);
        assert_eq!(mesh.count_vertices(), 8 * 6 * 4);
        assert_eq!(mesh.indices().unwrap().len(), 8 * 6 * 6);
    }

    #[test]
    fn branch_occlusion_raster_preserves_a_large_limb_footprint() {
        let terrain = SceneTerrain::from_heightmap(11, 11, 1.0, vec![0.0; 121]).unwrap();
        let branch = TreeBranchSegment {
            start: Vec3::new(-3.0, 5.0, 0.0),
            end: Vec3::new(3.0, 5.0, 0.0),
            start_radius: 0.32,
            end_radius: 0.18,
            depth: 1,
            primary_group: 0,
            secondary_group: 0,
            is_limb_tip: false,
        };
        let mut heights = vec![
            f32::NEG_INFINITY;
            WEATHER_OCCLUSION_RESOLUTION as usize
                * WEATHER_OCCLUSION_RESOLUTION as usize
        ];
        rasterize_weather_branch(&mut heights, &terrain, &GlobalTransform::IDENTITY, &branch);
        let covered = heights
            .iter()
            .enumerate()
            .filter_map(|(index, height)| {
                height.is_finite().then_some((
                    index % WEATHER_OCCLUSION_RESOLUTION as usize,
                    index / WEATHER_OCCLUSION_RESOLUTION as usize,
                ))
            })
            .collect::<Vec<_>>();
        let width = covered.iter().map(|(x, _)| *x).max().unwrap()
            - covered.iter().map(|(x, _)| *x).min().unwrap()
            + 1;
        let depth = covered.iter().map(|(_, z)| *z).max().unwrap()
            - covered.iter().map(|(_, z)| *z).min().unwrap()
            + 1;
        assert!(covered.len() > 1_000);
        assert!(width > depth * 5);
        assert!(heights[256 * WEATHER_OCCLUSION_RESOLUTION as usize + 256] >= 5.18);
    }
}
