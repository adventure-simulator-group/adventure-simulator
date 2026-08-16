use super::*;

const WEATHER_SHADER: &str = "shaders/tactical_weather.wgsl";
const FALLING_PARTICLE_CAPACITY: usize = 3_072;
const IMPACT_PARTICLE_CAPACITY: usize = 384;
const DISTANT_SHEET_SEGMENTS: usize = 16;
const DISTANT_SHEET_LAYERS: usize = 6;

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

pub(super) fn apply_active_scene_weather(
    active: Res<ActiveTacticalScene>,
    scenes: Query<(&SceneEnvironment, &SceneTerrain)>,
    particles: Query<Entity, With<WeatherParticle>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalWeatherMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if !active.is_changed() {
        return;
    }
    for entity in &particles {
        commands.entity(entity).despawn();
    }
    let Some((environment, terrain)) = active.entity.and_then(|entity| scenes.get(entity).ok())
    else {
        return;
    };
    if environment.weather.precipitation == Precipitation::Clear
        || environment.weather.intensity_bps == 0
    {
        return;
    }

    let heightmap = images.add(weather_heightmap_image(terrain));
    let falling_material = materials.add(weather_material(
        environment,
        terrain,
        heightmap.clone(),
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
            f32::from(environment.weather.intensity_bps) / 10_000.0,
            f32::from(environment.weather.wind_speed_bps) / 10_000.0,
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

fn weather_heightmap_image(terrain: &SceneTerrain) -> Image {
    let width = terrain.grid_width() as u32;
    let height = terrain.grid_depth() as u32;
    let minimum = terrain.minimum_height();
    let range = (terrain.maximum_height() - minimum).max(0.001);
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for z in 0..height {
        for x in 0..width {
            let world = Vec2::new(
                x as f32 * terrain.grid_scale() - terrain.width() * 0.5,
                z as f32 * terrain.grid_scale() - terrain.depth() * 0.5,
            );
            let normalized =
                ((terrain.height_at(world).unwrap_or(minimum) - minimum) / range).clamp(0.0, 1.0);
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
        let image = weather_heightmap_image(&terrain);
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
}
