//! A single, baked optical cloud shell for the grounded tactical camera.

use super::*;

#[cfg(not(target_family = "wasm"))]
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on};

const CLOUD_SHADER: &str = "shaders/tactical_clouds.wgsl";
const CLOUD_DOME_DISTANCE_METRES: f32 = 20_000.0;
/// Deliberately smaller than Earth's radius so the global cloud surface bends
/// into the tactical horizon while staying visually flat around the player.
const CLOUD_CURVATURE_RADIUS_METRES: f32 = 180_000.0;
/// The sky map is camera-locked around the playable-area reference point.
/// Its native dome parameterization gives low elevations their own rows rather
/// than compressing them into an orthographic texture's outer ring. The
/// duplicated azimuth seam is 1025 x 257 RGBA8, or about 1 MiB.
const CLOUD_BAKE_AZIMUTH_SEGMENTS: u32 = 1_024;
const CLOUD_BAKE_ELEVATION_SEGMENTS: u32 = 256;
const CLOUD_BAKE_TEXTURE_WIDTH: u32 = CLOUD_BAKE_AZIMUTH_SEGMENTS + 1;
const CLOUD_BAKE_TEXTURE_HEIGHT: u32 = CLOUD_BAKE_ELEVATION_SEGMENTS + 1;
const CLOUD_BAKE_CHANNELS: usize = 4;
const CLOUD_BAKE_VERTICAL_SAMPLES: u32 = 48;
const CLOUD_BAKE_REFERENCE_EYE_METRES: f32 = 1.7;
/// Long endpoint spacing keeps the expensive CPU bake comfortably ahead of
/// playback while wind compensation supplies continuous motion every frame.
const CLOUD_ANIMATION_INTERVAL_SECONDS: f32 = 45.0;
const CLOUD_MAX_WIND_METRES_PER_SECOND: f32 = 18.0;
const CLOUD_EVOLUTION_PER_SECOND: f32 = 0.002_5;

#[derive(Component)]
pub(crate) struct TacticalCloudLayer {
    active: bool,
}

impl TacticalCloudLayer {
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(dead_code)] // Named variants are consumed by the native capture binary.
pub(crate) enum TacticalCloudCaptureProfile {
    #[default]
    Cumulus,
    Clear,
    Stratocumulus,
    Cirrus,
    Overcast,
    Storm,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct TacticalCloudCaptureOverride(pub(crate) Option<TacticalCloudCaptureProfile>);

/// Benchmark-only rendering isolation that remains effective while cloud
/// parameters continue updating every frame.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct TacticalCloudBenchmarkIsolation {
    pub(crate) hide_clouds: bool,
    pub(crate) freeze_animation: bool,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct TacticalCloudAnimationStatus {
    ready: bool,
}

impl TacticalCloudAnimationStatus {
    pub(crate) fn is_ready(&self) -> bool {
        self.ready
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalCloudMaterial {
    /// Direction toward the Sun and scene-referred cloud luminance.
    #[uniform(0)]
    lighting: Vec4,
    /// Coverage, density, profile family, deterministic texture seed.
    #[uniform(1)]
    shape: Vec4,
    /// Cloud-surface altitude and horizontal texture scale.
    #[uniform(2)]
    layer: Vec4,
    /// Wind offset in metres, direct-light fraction, weather transmission.
    #[uniform(3)]
    motion: Vec4,
    /// Solar RGB chroma derived from altitude; alpha is reserved.
    #[uniform(4)]
    spectral: Vec4,
    /// Fixed scene anchor X/Z and curvature radius.
    #[uniform(5)]
    geometry: Vec4,
    /// Consecutive finite, camera-locked directional optical-property bakes.
    #[texture(6, dimension = "2d")]
    #[sampler(7)]
    baked_texture_a: Handle<Image>,
    #[texture(8, dimension = "2d")]
    #[sampler(9)]
    baked_texture_b: Handle<Image>,
}

impl Material for TacticalCloudMaterial {
    fn vertex_shader() -> ShaderRef {
        CLOUD_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        CLOUD_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The camera remains inside the raster hemisphere. Disabling culling
        // avoids requiring a second, inside-out mesh in the browser bundle.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CloudLayerParameters {
    coverage: f32,
    density: f32,
    profile: f32,
    seed: f32,
    bottom_metres: f32,
    thickness_metres: f32,
    horizontal_scale: f32,
}

impl CloudLayerParameters {
    fn layers_from_environment(
        environment: &SceneEnvironment,
        capture: Option<TacticalCloudCaptureProfile>,
    ) -> [Option<Self>; 3] {
        if let Some(profile) = capture {
            return [Self::capture(profile), None, None];
        }
        let atmosphere = environment.weather.atmosphere;
        [
            atmosphere.low_cloud,
            atmosphere.middle_cloud,
            atmosphere.high_cloud,
        ]
        .map(|layer| layer.map(Self::from_layer))
    }

    fn capture(profile: TacticalCloudCaptureProfile) -> Option<Self> {
        let seed = match profile {
            TacticalCloudCaptureProfile::Clear => 0,
            TacticalCloudCaptureProfile::Cumulus => 117,
            TacticalCloudCaptureProfile::Stratocumulus => 283,
            TacticalCloudCaptureProfile::Cirrus => 419,
            TacticalCloudCaptureProfile::Overcast => 631,
            TacticalCloudCaptureProfile::Storm => 887,
        };
        let layer = match profile {
            TacticalCloudCaptureProfile::Clear => return None,
            TacticalCloudCaptureProfile::Cumulus => CloudLayerSnapshot {
                form: CloudForm::Cumulus,
                coverage_bps: 4_800,
                optical_density_bps: 5_000,
                base_metres: 1_250,
                top_metres: 3_100,
            },
            TacticalCloudCaptureProfile::Stratocumulus => CloudLayerSnapshot {
                form: CloudForm::Stratocumulus,
                coverage_bps: 5_700,
                optical_density_bps: 5_500,
                base_metres: 1_050,
                top_metres: 1_950,
            },
            TacticalCloudCaptureProfile::Cirrus => CloudLayerSnapshot {
                form: CloudForm::Cirrus,
                coverage_bps: 4_200,
                optical_density_bps: 2_000,
                base_metres: 5_500,
                top_metres: 8_500,
            },
            TacticalCloudCaptureProfile::Overcast => CloudLayerSnapshot {
                form: CloudForm::Stratus,
                coverage_bps: 9_400,
                optical_density_bps: 7_000,
                base_metres: 700,
                top_metres: 1_350,
            },
            TacticalCloudCaptureProfile::Storm => CloudLayerSnapshot {
                form: CloudForm::Cumulonimbus,
                coverage_bps: 7_200,
                optical_density_bps: 9_000,
                base_metres: 720,
                top_metres: 10_500,
            },
        };
        let mut parameters = Self::from_layer(layer);
        parameters.seed = (seed % 4_096) as f32;
        Some(parameters)
    }

    fn from_layer(layer: CloudLayerSnapshot) -> Self {
        let (profile, horizontal_scale) = match layer.form {
            CloudForm::Cumulus => (0.0, 0.000_52),
            CloudForm::Stratocumulus => (1.0, 0.000_62),
            CloudForm::Cirrus => (2.0, 0.000_27),
            CloudForm::Cumulonimbus => (3.0, 0.000_34),
            CloudForm::Stratus => (4.0, 0.000_25),
            CloudForm::Altocumulus => (5.0, 0.000_58),
            CloudForm::Altostratus => (6.0, 0.000_22),
            CloudForm::Nimbostratus => (7.0, 0.000_21),
            CloudForm::Cirrocumulus => (8.0, 0.000_68),
            CloudForm::Cirrostratus => (9.0, 0.000_16),
            CloudForm::CumulusCongestus => (10.0, 0.000_48),
        };
        let thickness_metres =
            f32::from(layer.top_metres.saturating_sub(layer.base_metres).max(100));
        Self {
            coverage: f32::from(layer.coverage_bps) / 10_000.0,
            density: 0.4 + f32::from(layer.optical_density_bps) / 10_000.0,
            profile,
            seed: 0.0,
            bottom_metres: f32::from(layer.base_metres),
            thickness_metres,
            horizontal_scale,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CloudBakeKey {
    layers: [Option<CloudLayerParameters>; 3],
    seed: u64,
}

#[derive(Clone, Debug)]
struct CloudBakeRequest {
    key: CloudBakeKey,
    endpoint: u64,
    wind_velocity: Vec2,
}

impl CloudBakeRequest {
    fn elapsed_seconds(&self) -> f32 {
        self.endpoint as f32 * CLOUD_ANIMATION_INTERVAL_SECONDS
    }

    fn advection_metres(&self) -> Vec2 {
        self.wind_velocity * self.elapsed_seconds()
    }

    fn evolution(&self) -> f32 {
        self.elapsed_seconds() * CLOUD_EVOLUTION_PER_SECOND
    }
}

struct CompletedCloudBake {
    request: CloudBakeRequest,
    image: Image,
}

#[derive(Resource, Default)]
pub(in crate::presentation) struct CloudBakeState {
    key: Option<CloudBakeKey>,
    elapsed_seconds: f32,
    end_ready: bool,
    queued: Option<CompletedCloudBake>,
    #[cfg(not(target_family = "wasm"))]
    pending: Option<Task<CompletedCloudBake>>,
}

pub(in crate::presentation) fn setup_tactical_clouds(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TacticalCloudMaterial>>,
) {
    let mesh = meshes.add(cloud_hemisphere_mesh());
    let empty_request = CloudBakeRequest {
        key: CloudBakeKey {
            layers: [None, None, None],
            seed: 0,
        },
        endpoint: 0,
        wind_velocity: Vec2::ZERO,
    };
    let baked_texture_a = images.add(cloud_bake_image(&empty_request));
    let baked_texture_b = images.add(cloud_bake_image(&empty_request));
    commands.insert_resource(CloudBakeState::default());
    commands.insert_resource(TacticalCloudAnimationStatus::default());
    commands.spawn((
        Name::new("Baked tactical cloud shell"),
        TacticalCloudLayer { active: false },
        NoFrustumCulling,
        NotShadowCaster,
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(TacticalCloudMaterial {
            lighting: Vec4::new(0.0, 1.0, 0.0, 1.43),
            shape: Vec4::ZERO,
            layer: Vec4::new(2_000.0, 1.0, 0.0, 0.0),
            motion: Vec4::new(0.0, 0.0, 1.0, 1.0),
            spectral: Vec4::ONE,
            geometry: cloud_shell_geometry(),
            baked_texture_a,
            baked_texture_b,
        })),
        Transform::default(),
    ));
}

/// Bakes one endpoint from a stable tactical eye point into the upper dome's
/// angular coordinates. Runtime sees the same coherent clouds as a ray march
/// without repeating that integration every frame.
fn cloud_bake_image(request: &CloudBakeRequest) -> Image {
    let mut pixels = Vec::with_capacity(
        CLOUD_BAKE_TEXTURE_WIDTH as usize
            * CLOUD_BAKE_TEXTURE_HEIGHT as usize
            * CLOUD_BAKE_CHANNELS,
    );
    let reference_eye = Vec3::new(0.0, CLOUD_BAKE_REFERENCE_EYE_METRES, 0.0);
    let advection_metres = request.advection_metres();
    let evolution = request.evolution();
    for y in 0..CLOUD_BAKE_TEXTURE_HEIGHT {
        for x in 0..CLOUD_BAKE_TEXTURE_WIDTH {
            let direction = cloud_bake_direction(x, y);
            let mut optical_depth = 0.0;
            let mut lighting = 0.0;
            let mut variation = 0.0;
            for (slot, layer) in request.key.layers.iter().flatten().enumerate() {
                let Some((start, end)) = cloud_ray_layer_interval(reference_eye, direction, *layer)
                else {
                    continue;
                };
                let step = (end - start) / CLOUD_BAKE_VERTICAL_SAMPLES as f32;
                for sample in 0..CLOUD_BAKE_VERTICAL_SAMPLES {
                    let distance = start + (sample as f32 + 0.5) * step;
                    let position = reference_eye + direction * distance;
                    let height = ((cloud_shell_altitude(position) - layer.bottom_metres)
                        / layer.thickness_metres)
                        .clamp(0.0, 1.0);
                    let density = baked_cloud_density(
                        position.xz() - advection_metres,
                        height,
                        *layer,
                        request.key.seed,
                        slot as u64,
                        evolution,
                    );
                    let contribution = density * step * 0.001_45;
                    optical_depth += contribution;
                    let detail = cloud_bake_lighting_variation(
                        position.xz() - advection_metres,
                        height,
                        *layer,
                        request.key.seed,
                        slot as u64,
                        evolution,
                    );
                    lighting += contribution * (0.18 + height * 0.76) * (0.18 + detail * 0.82);
                    variation += contribution * detail;
                }
            }
            let alpha = 1.0 - (-optical_depth).exp();
            let lighting = if optical_depth > 0.0001 {
                lighting / optical_depth
            } else {
                0.0
            };
            let variation = if optical_depth > 0.0001 {
                variation / optical_depth
            } else {
                0.0
            };
            let sun_transmission = (-optical_depth * (0.45 + variation * 0.55)).exp();
            pixels.extend([
                (alpha * 255.0).round() as u8,
                (lighting.clamp(0.0, 1.0) * 255.0).round() as u8,
                (sun_transmission * 255.0).round() as u8,
                (variation.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: CLOUD_BAKE_TEXTURE_WIDTH,
            height: CLOUD_BAKE_TEXTURE_HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

/// Maps the same equirectangular UV as the dome mesh: U is azimuth and V is
/// elevation. The first and final columns intentionally evaluate the exact
/// same ray, allowing clamp sampling to cross the visible seam continuously.
fn cloud_bake_direction(x: u32, y: u32) -> Vec3 {
    let azimuth_index = if x == CLOUD_BAKE_AZIMUTH_SEGMENTS {
        0
    } else {
        x
    };
    let azimuth =
        (azimuth_index as f32 / CLOUD_BAKE_AZIMUTH_SEGMENTS as f32 - 0.5) * core::f32::consts::TAU;
    let elevation = y as f32 / CLOUD_BAKE_ELEVATION_SEGMENTS as f32 * core::f32::consts::FRAC_PI_2;
    let horizontal = elevation.cos();
    Vec3::new(
        horizontal * azimuth.cos(),
        elevation.sin(),
        horizontal * azimuth.sin(),
    )
}

fn cloud_shell_altitude(position: Vec3) -> f32 {
    (position - Vec3::new(0.0, -CLOUD_CURVATURE_RADIUS_METRES, 0.0)).length()
        - CLOUD_CURVATURE_RADIUS_METRES
}

fn cloud_ray_layer_interval(
    origin: Vec3,
    direction: Vec3,
    layer: CloudLayerParameters,
) -> Option<(f32, f32)> {
    let center = Vec3::new(0.0, -CLOUD_CURVATURE_RADIUS_METRES, 0.0);
    let roots = |radius: f32| {
        let relative = origin - center;
        let projected = relative.dot(direction);
        let discriminant = projected * projected - (relative.length_squared() - radius * radius);
        (discriminant >= 0.0).then(|| {
            Vec2::new(
                -projected - discriminant.sqrt(),
                -projected + discriminant.sqrt(),
            )
        })
    };
    let inner = roots(CLOUD_CURVATURE_RADIUS_METRES + layer.bottom_metres)?;
    let outer =
        roots(CLOUD_CURVATURE_RADIUS_METRES + layer.bottom_metres + layer.thickness_metres)?;
    let start = inner.y.max(0.0);
    // Match the former ray marcher: distant grazing intersections beyond the
    // tactical cloud horizon are not part of the rendered volume.
    let end = outer.y.min(CLOUD_DOME_DISTANCE_METRES);
    (end > start).then_some((start, end))
}

fn baked_cloud_density(
    world: Vec2,
    height: f32,
    layer: CloudLayerParameters,
    seed: u64,
    slot: u64,
    evolution: f32,
) -> f32 {
    let kind = layer.profile as u32;
    let coordinate = cloud_density_coordinate(world, height, layer, seed, evolution);
    // Broad coverage, domain warp, and fine erosion all sample a genuine
    // finite 3-D lattice. Height is an independent coordinate, not a planar
    // translation, so integrating a ray cannot turn a single 2-D field into
    // radial wedges.
    let warp = Vec3::new(
        non_periodic_value_noise_3d(coordinate * 0.36, seed ^ slot.rotate_left(7)),
        non_periodic_value_noise_3d(
            coordinate * 0.36 + Vec3::splat(13.7),
            seed ^ slot.rotate_left(13),
        ),
        non_periodic_value_noise_3d(
            coordinate * 0.36 + Vec3::new(4.1, 9.7, 17.3),
            seed ^ slot.rotate_left(19),
        ),
    ) - Vec3::splat(0.5);
    let warped = coordinate + warp * Vec3::new(0.85, 0.42, 0.85);
    // Three incommensurate, non-periodic frequencies form clustered lobes;
    // no individual octave can reveal a repeated cell over the dome.
    let broad = non_periodic_value_noise_3d(warped * 0.58, seed ^ slot.rotate_left(11)) * 0.29
        + non_periodic_value_noise_3d(warped * 1.23, seed ^ slot.rotate_left(17)) * 0.44
        + non_periodic_value_noise_3d(warped * 2.61, seed ^ slot.rotate_left(23)) * 0.27;
    let detail = non_periodic_value_noise_3d(
        warped * 5.9 + Vec3::new(9.7, 1.3, 4.1),
        seed ^ slot.rotate_left(29),
    );
    let profile = cloud_vertical_profile(height, kind, broad);
    let mut threshold = 0.78 - layer.coverage * 0.34;
    if matches!(kind, 4 | 6 | 7 | 9) {
        threshold -= 0.08;
    }
    if kind == 0 {
        // Representative fair-weather coverage needs cores above the stable
        // tactical reference point, not only at long grazing paths.
        threshold += height * 0.07 - 0.24;
    }
    if kind == 3 {
        threshold += height * 0.24 - smoothstep(0.68, 0.82, height) * 0.11;
        threshold -= 0.10;
    }
    if kind == 10 {
        threshold += height * 0.14;
    }
    let body = if matches!(kind, 4 | 6 | 7 | 9) {
        smoothstep(threshold, threshold + 0.17, broad) * (0.58 + detail * 0.50)
    } else {
        smoothstep(threshold, threshold + 0.16, broad - (1.0 - detail) * 0.20)
    };
    (body * profile * layer.density).clamp(0.0, 1.35)
}

fn cloud_density_coordinate(
    world: Vec2,
    height: f32,
    layer: CloudLayerParameters,
    seed: u64,
    evolution: f32,
) -> Vec3 {
    let family_scale = match layer.profile as u32 {
        2 => Vec2::new(0.32, 1.8),
        5 | 8 => Vec2::splat(1.75),
        4 | 6 | 7 | 9 => Vec2::splat(0.58),
        _ => Vec2::ONE,
    };
    let seed_offset = layer.seed + (seed & 0x0fff) as f32;
    let mut coordinate = Vec3::new(
        world.x * layer.horizontal_scale + seed_offset * 0.013,
        height * 1.8 + seed_offset * 0.007,
        world.y * layer.horizontal_scale - seed_offset * 0.011,
    );
    // Slow non-rigid evolution changes cell edges between endpoints. Known
    // horizontal wind translation is handled separately and removed by the
    // runtime warp before the two optical solutions are blended.
    coordinate += Vec3::new(evolution * 0.37, evolution, -evolution * 0.23);
    coordinate.x *= family_scale.x;
    coordinate.z *= family_scale.y;
    coordinate
}

fn cloud_bake_lighting_variation(
    world: Vec2,
    height: f32,
    layer: CloudLayerParameters,
    seed: u64,
    slot: u64,
    evolution: f32,
) -> f32 {
    non_periodic_value_noise_3d(
        cloud_density_coordinate(world, height, layer, seed, evolution) * 3.17
            + Vec3::new(2.1, 7.3, 11.9),
        seed ^ slot.rotate_left(21),
    )
}

fn cloud_vertical_profile(height: f32, kind: u32, noise: f32) -> f32 {
    match kind {
        0 => smoothstep(0.0, 0.08, height) * (1.0 - smoothstep(0.58 + noise * 0.2, 1.0, height)),
        1 => smoothstep(0.0, 0.13, height) * (1.0 - smoothstep(0.72, 1.0, height)),
        2 => (1.0 - smoothstep(0.08, 0.34, (height - 0.52).abs())) * (0.55 + noise * 0.45),
        3 => (smoothstep(0.0, 0.035, height)
            * (1.0 - smoothstep(0.78 + noise * 0.12, 1.0, height)))
        .max(smoothstep(0.68, 0.78, height) * (1.0 - smoothstep(0.9, 1.0, height)) * 0.85),
        4 | 6 | 7 => {
            smoothstep(0.0, if kind == 4 { 0.06 } else { 0.12 }, height)
                * (1.0 - smoothstep(if kind == 4 { 0.82 } else { 0.9 }, 1.0, height))
        }
        5 => (1.0 - smoothstep(0.18, 0.46, (height - 0.5).abs())) * (0.7 + noise * 0.3),
        8 => (1.0 - smoothstep(0.12, 0.34, (height - 0.52).abs())) * (0.65 + noise * 0.35),
        9 => 1.0 - smoothstep(0.24, 0.48, (height - 0.5).abs()),
        _ => smoothstep(0.0, 0.05, height) * (1.0 - smoothstep(0.72 + noise * 0.18, 1.0, height)),
    }
}

fn non_periodic_value_noise_3d(position: Vec3, seed: u64) -> f32 {
    let cell = position.floor();
    let fraction = position - cell;
    let smooth = fraction * fraction * (Vec3::splat(3.0) - fraction * 2.0);
    let value = |offset: Vec3| {
        let lattice = cell + offset;
        cloud_noise_hash(
            seed ^ (lattice.x as i64 as u64).wrapping_mul(0x9e37_79b9)
                ^ (lattice.y as i64 as u64).rotate_left(23)
                ^ (lattice.z as i64 as u64).wrapping_mul(0xd1b5_4a32_d192_ed03),
        ) as f32
            / u64::MAX as f32
    };
    let x0 = value(Vec3::ZERO).lerp(value(Vec3::X), smooth.x);
    let x1 = value(Vec3::Y).lerp(value(Vec3::ONE), smooth.x);
    let y0 = x0.lerp(x1, smooth.y);
    let x2 = value(Vec3::Z).lerp(value(Vec3::Z + Vec3::X), smooth.x);
    let x3 = value(Vec3::Z + Vec3::Y).lerp(value(Vec3::ONE), smooth.x);
    y0.lerp(x2.lerp(x3, smooth.y), smooth.z)
}

fn cloud_noise_hash(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn cloud_hemisphere_mesh() -> Mesh {
    // The mesh supplies only view directions. This moderate tessellation keeps
    // a smooth horizon while eliminating the old ray-march proxy density.
    const AZIMUTH_SEGMENTS: u32 = 128;
    const ELEVATION_SEGMENTS: u32 = 64;
    let mut positions =
        Vec::with_capacity(((AZIMUTH_SEGMENTS + 1) * (ELEVATION_SEGMENTS + 1)) as usize);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    for elevation_index in 0..=ELEVATION_SEGMENTS {
        let elevation =
            elevation_index as f32 / ELEVATION_SEGMENTS as f32 * core::f32::consts::FRAC_PI_2;
        let horizontal = elevation.cos();
        let y = elevation.sin();
        for azimuth_index in 0..=AZIMUTH_SEGMENTS {
            let azimuth = azimuth_index as f32 / AZIMUTH_SEGMENTS as f32 * core::f32::consts::TAU;
            let direction = Vec3::new(horizontal * azimuth.cos(), y, horizontal * azimuth.sin());
            positions.push((direction * CLOUD_DOME_DISTANCE_METRES).to_array());
            normals.push(direction.to_array());
            uvs.push([
                azimuth_index as f32 / AZIMUTH_SEGMENTS as f32,
                elevation_index as f32 / ELEVATION_SEGMENTS as f32,
            ]);
        }
    }
    let mut indices = Vec::with_capacity((AZIMUTH_SEGMENTS * ELEVATION_SEGMENTS * 6) as usize);
    let stride = AZIMUTH_SEGMENTS + 1;
    for elevation_index in 0..ELEVATION_SEGMENTS {
        for azimuth_index in 0..AZIMUTH_SEGMENTS {
            let lower_left = elevation_index * stride + azimuth_index;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + stride;
            let upper_right = upper_left + 1;
            indices.extend_from_slice(&[
                lower_left,
                upper_left,
                lower_right,
                lower_right,
                upper_left,
                upper_right,
            ]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(in crate::presentation) fn update_tactical_clouds(
    time: Res<Time>,
    active: Res<ActiveTacticalScene>,
    environments: Query<&SceneEnvironment>,
    celestial: Res<PresentedCelestialLighting>,
    capture: Res<TacticalCloudCaptureOverride>,
    isolation: Res<TacticalCloudBenchmarkIsolation>,
    camera: Single<&GlobalTransform, With<Camera3d>>,
    mut clouds: Query<(
        &mut TacticalCloudLayer,
        &MeshMaterial3d<TacticalCloudMaterial>,
        &mut Transform,
        &mut Visibility,
    )>,
    mut materials: ResMut<Assets<TacticalCloudMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bake_state: ResMut<CloudBakeState>,
    mut animation_status: ResMut<TacticalCloudAnimationStatus>,
) {
    animation_status.ready = false;
    let environment = active
        .entity
        .and_then(|entity| environments.get(entity).ok());
    let celestial = celestial.snapshot.as_ref();
    let layers = environment
        .map(|environment| CloudLayerParameters::layers_from_environment(environment, capture.0));

    for (mut cloud, handle, mut transform, mut visibility) in &mut clouds {
        transform.translation = camera.translation();
        let Some(environment) = environment else {
            cloud.active = false;
            *visibility = cloud_visibility(false, *isolation);
            continue;
        };
        let Some(celestial) = celestial else {
            cloud.active = false;
            *visibility = cloud_visibility(false, *isolation);
            continue;
        };
        let Some(layers) = layers else {
            cloud.active = false;
            *visibility = cloud_visibility(false, *isolation);
            continue;
        };
        if layers
            .iter()
            .flatten()
            .all(|layer| layer.coverage <= 0.001 || layer.density <= 0.001)
        {
            cloud.active = false;
            *visibility = cloud_visibility(false, *isolation);
            continue;
        }
        let seed = cloud_seed(environment);
        let bake_key = CloudBakeKey { layers, seed };
        let Some(mut material) = materials.get_mut(&handle.0) else {
            continue;
        };
        if bake_state.key.as_ref() != Some(&bake_key) {
            #[cfg(not(target_family = "wasm"))]
            {
                bake_state.pending = None;
            }
            bake_state.queued = None;
            bake_state.elapsed_seconds = 0.0;
            bake_state.end_ready = false;

            let wind_velocity = cloud_wind_velocity(environment);
            let initial_request = CloudBakeRequest {
                key: bake_key.clone(),
                endpoint: 0,
                wind_velocity,
            };
            let initial = cloud_bake_image(&initial_request);
            if let Some(mut image) = images.get_mut(&material.baked_texture_a) {
                *image = initial.clone();
            }
            if let Some(mut image) = images.get_mut(&material.baked_texture_b) {
                *image = initial;
            }
            bake_state.key = Some(bake_key.clone());

            #[cfg(not(target_family = "wasm"))]
            {
                bake_state.pending = Some(spawn_cloud_bake(CloudBakeRequest {
                    key: bake_key.clone(),
                    endpoint: 1,
                    wind_velocity,
                }));
            }
        }

        #[cfg(not(target_family = "wasm"))]
        advance_cloud_bake_pipeline(
            &mut bake_state,
            &mut material,
            &mut images,
            if isolation.freeze_animation {
                0.0
            } else {
                time.delta_secs()
            },
        );
        let representative_altitude = cloud_representative_altitude(layers);
        let storminess = layers
            .iter()
            .flatten()
            .any(|layer| matches!(layer.profile as u32, 3 | 7)) as u8
            as f32;
        cloud.active = true;
        let daylight = smoothstep(-8.0, 8.0, celestial.sun_altitude_degrees);
        let scene_luminance = 0.08 + daylight * 1.35;
        let solar_color = cloud_solar_color(celestial.sun_altitude_degrees);
        material.lighting = celestial.sun_direction.extend(scene_luminance);
        let blend = if bake_state.end_ready {
            (bake_state.elapsed_seconds / CLOUD_ANIMATION_INTERVAL_SECONDS).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let wind_velocity = cloud_wind_velocity(environment);
        material.shape = Vec4::new(
            storminess,
            blend,
            CLOUD_ANIMATION_INTERVAL_SECONDS,
            CLOUD_BAKE_REFERENCE_EYE_METRES,
        );
        material.layer = Vec4::new(representative_altitude, 1.0, 0.0, 0.0);
        material.motion = Vec4::new(
            wind_velocity.x,
            wind_velocity.y,
            daylight,
            celestial.weather_transmission,
        );
        material.spectral = solar_color.extend(1.0);
        material.geometry = cloud_shell_geometry();
        #[cfg(not(target_family = "wasm"))]
        {
            animation_status.ready = bake_state.end_ready && bake_state.queued.is_some();
        }
        #[cfg(target_family = "wasm")]
        {
            animation_status.ready = true;
        }
        *visibility = cloud_visibility(true, *isolation);
    }
}

fn cloud_wind_velocity(environment: &SceneEnvironment) -> Vec2 {
    let bearing = f32::from(environment.weather.atmosphere.wind_direction_degrees).to_radians();
    let speed =
        f32::from(environment.weather.wind_speed_bps) / 10_000.0 * CLOUD_MAX_WIND_METRES_PER_SECOND;
    Vec2::new(bearing.sin(), -bearing.cos()) * speed
}

fn cloud_representative_altitude(layers: [Option<CloudLayerParameters>; 3]) -> f32 {
    let mut weighted_altitude = 0.0;
    let mut total_weight = 0.0;
    for layer in layers.into_iter().flatten() {
        let weight = (layer.coverage * layer.density).max(0.001);
        weighted_altitude += (layer.bottom_metres + layer.thickness_metres * 0.5) * weight;
        total_weight += weight;
    }
    if total_weight > 0.0 {
        weighted_altitude / total_weight
    } else {
        2_000.0
    }
}

#[cfg(not(target_family = "wasm"))]
fn spawn_cloud_bake(request: CloudBakeRequest) -> Task<CompletedCloudBake> {
    AsyncComputeTaskPool::get().spawn(async move {
        let image = cloud_bake_image(&request);
        CompletedCloudBake { request, image }
    })
}

#[cfg(not(target_family = "wasm"))]
fn advance_cloud_bake_pipeline(
    state: &mut CloudBakeState,
    material: &mut TacticalCloudMaterial,
    images: &mut Assets<Image>,
    delta_seconds: f32,
) {
    if state.pending.as_ref().is_some_and(Task::is_finished) {
        let completed = block_on(
            state
                .pending
                .take()
                .expect("finished cloud bake task remains present"),
        );
        if state.key.as_ref() == Some(&completed.request.key) {
            if state.end_ready {
                state.queued = Some(completed);
            } else {
                if let Some(mut image) = images.get_mut(&material.baked_texture_b) {
                    *image = completed.image;
                }
                state.end_ready = true;
                state.elapsed_seconds = 0.0;
                state.pending = Some(spawn_cloud_bake(CloudBakeRequest {
                    key: completed.request.key,
                    endpoint: completed.request.endpoint + 1,
                    wind_velocity: completed.request.wind_velocity,
                }));
            }
        }
    }

    if !state.end_ready {
        return;
    }
    state.elapsed_seconds =
        (state.elapsed_seconds + delta_seconds).min(CLOUD_ANIMATION_INTERVAL_SECONDS);
    if state.elapsed_seconds < CLOUD_ANIMATION_INTERVAL_SECONDS {
        return;
    }
    let Some(completed) = state.queued.take() else {
        return;
    };

    let recycled = material.baked_texture_a.clone();
    material.baked_texture_a = material.baked_texture_b.clone();
    material.baked_texture_b = recycled;
    if let Some(mut image) = images.get_mut(&material.baked_texture_b) {
        *image = completed.image;
    }
    state.elapsed_seconds = 0.0;
    state.pending = Some(spawn_cloud_bake(CloudBakeRequest {
        key: completed.request.key,
        endpoint: completed.request.endpoint + 1,
        wind_velocity: completed.request.wind_velocity,
    }));
}

fn cloud_visibility(active: bool, isolation: TacticalCloudBenchmarkIsolation) -> Visibility {
    if active && !isolation.hide_clouds {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn cloud_shell_geometry() -> Vec4 {
    // Tactical world coordinates are local to the scene. Anchoring the shell
    // below that origin keeps curvature stable as the camera crosses the
    // playable area; the camera-following mesh remains only a raster proxy.
    Vec4::new(0.0, 0.0, CLOUD_CURVATURE_RADIUS_METRES, 0.0)
}

#[cfg(test)]
fn cloud_shell_altitude_at_distance(surface_metres: f32, horizontal_metres: f32) -> f32 {
    let radius = CLOUD_CURVATURE_RADIUS_METRES + surface_metres;
    (radius * radius - horizontal_metres * horizontal_metres)
        .max(0.0)
        .sqrt()
        - CLOUD_CURVATURE_RADIUS_METRES
}

fn cloud_seed(environment: &SceneEnvironment) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in environment.scene_digest.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^= environment.absolute_minute / 360;
    hash ^= (environment.latitude_microdegrees as u32 as u64) << 32;
    hash ^= environment.longitude_microdegrees as u32 as u64;
    hash
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cloud_solar_color(sun_altitude_degrees: f32) -> Vec3 {
    let warmth = 1.0 - smoothstep(4.0, 22.0, sun_altitude_degrees);
    Vec3::ONE.lerp(Vec3::new(1.0, 0.78, 0.60), warmth)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment() -> SceneEnvironment {
        SceneEnvironment {
            scene_digest: "cloud-parameter-test".into(),
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 100_000,
            absolute_elevation_metres: 20,
            weather: WeatherSnapshot {
                rules_version: WEATHER_RULES_VERSION,
                interval_start_minute: 100_000,
                cell_latitude: 0,
                cell_longitude: 0,
                temperature_deci_c: 150,
                wind_speed_bps: 2_000,
                precipitation: Precipitation::Rain,
                intensity_bps: 8_000,
                ground_moisture_bps: 0,
                snow_cover_bps: 0,
                atmosphere: AtmosphericSnapshot {
                    wind_direction_degrees: 240,
                    wind_shear_bps: 4_000,
                    low_cloud: Some(CloudLayerSnapshot {
                        form: CloudForm::Cumulonimbus,
                        coverage_bps: 8_500,
                        optical_density_bps: 9_000,
                        base_metres: 700,
                        top_metres: 10_500,
                    }),
                    middle_cloud: Some(CloudLayerSnapshot {
                        form: CloudForm::Altostratus,
                        coverage_bps: 6_000,
                        optical_density_bps: 4_000,
                        base_metres: 2_800,
                        top_metres: 5_200,
                    }),
                    high_cloud: Some(CloudLayerSnapshot {
                        form: CloudForm::Cirrus,
                        coverage_bps: 4_000,
                        optical_density_bps: 2_000,
                        base_metres: 6_200,
                        top_metres: 10_500,
                    }),
                    ..Default::default()
                },
            },
            canopy_bps: 0,
            wetland_bps: 0,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        }
    }

    #[test]
    fn authoritative_weather_keeps_all_three_decks_for_the_bake() {
        let layers = CloudLayerParameters::layers_from_environment(&environment(), None);
        assert_eq!(layers.iter().flatten().count(), 3);
        assert!(layers[0].unwrap().bottom_metres < layers[2].unwrap().bottom_metres);
    }

    #[test]
    fn capture_profiles_cover_distinct_altitude_and_density_families() {
        let cumulus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cumulus).unwrap();
        let cirrus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cirrus).unwrap();
        let storm = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Storm).unwrap();
        assert!(cirrus.bottom_metres > cumulus.bottom_metres);
        assert!(cirrus.density < cumulus.density);
        assert!(storm.density > cumulus.density);
        assert!(storm.coverage > cumulus.coverage);
        assert!(CloudLayerParameters::capture(TacticalCloudCaptureProfile::Clear).is_none());
    }

    #[test]
    fn cumulus_density_uses_seeded_offsets_and_populates_the_reference_overhead() {
        let cumulus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cumulus).unwrap();
        let mut unseeded = cumulus;
        unseeded.seed = 0.0;
        let sample_world = Vec2::new(640.0, -420.0);
        assert!(
            (baked_cloud_density(sample_world, 0.34, cumulus, 42, 0, 0.0)
                - baked_cloud_density(sample_world, 0.34, unseeded, 42, 0, 0.0))
            .abs()
                > 0.001
        );
        let occupied = (-3..=3)
            .flat_map(|z| (-3..=3).map(move |x| Vec2::new(x as f32 * 500.0, z as f32 * 500.0)))
            .filter(|world| baked_cloud_density(*world, 0.34, cumulus, 42, 0, 0.0) > 0.08)
            .count();
        assert!(
            occupied >= 4,
            "cumulus must occupy several nearby overhead samples; observed {occupied}"
        );
    }

    #[test]
    fn cloud_volume_noise_decorrelates_height_without_translating_the_world_field() {
        let cumulus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cumulus).unwrap();
        let world = Vec2::new(860.0, -510.0);
        let low = cloud_density_coordinate(world, 0.22, cumulus, 42, 0.0);
        let high = cloud_density_coordinate(world, 0.66, cumulus, 42, 0.0);
        assert_eq!(low.xz(), high.xz());
        let low_noise = non_periodic_value_noise_3d(low * 1.23, 42);
        let high_noise = non_periodic_value_noise_3d(high * 1.23, 42);
        assert!(
            (low_noise - high_noise).abs() > 0.001,
            "height must select an independent 3-D lattice slice"
        );
    }

    #[test]
    fn benchmark_isolation_hides_an_active_cloud_layer() {
        assert_eq!(
            cloud_visibility(true, TacticalCloudBenchmarkIsolation::default()),
            Visibility::Inherited
        );
        assert_eq!(
            cloud_visibility(
                true,
                TacticalCloudBenchmarkIsolation {
                    hide_clouds: true,
                    freeze_animation: false,
                }
            ),
            Visibility::Hidden
        );
    }

    #[test]
    fn every_diagnosed_cloud_form_has_a_distinct_analytic_profile() {
        let forms = [
            CloudForm::Cumulus,
            CloudForm::Stratocumulus,
            CloudForm::Cirrus,
            CloudForm::Cumulonimbus,
            CloudForm::Stratus,
            CloudForm::Altocumulus,
            CloudForm::Altostratus,
            CloudForm::Nimbostratus,
            CloudForm::Cirrocumulus,
            CloudForm::Cirrostratus,
            CloudForm::CumulusCongestus,
        ];
        let mut profiles = forms
            .map(|form| {
                CloudLayerParameters::from_layer(CloudLayerSnapshot {
                    form,
                    coverage_bps: 5_000,
                    optical_density_bps: 5_000,
                    base_metres: 1_000,
                    top_metres: 3_000,
                })
                .profile as u8
            })
            .to_vec();
        profiles.sort_unstable();
        profiles.dedup();
        assert_eq!(profiles.len(), forms.len());
    }

    #[test]
    fn raster_shell_contains_only_the_upper_hemisphere() {
        let mesh = cloud_hemisphere_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("cloud shell must contain float positions");
        };
        assert_eq!(positions.len(), 129 * 65);
        assert!(positions.iter().all(|position| position[1] >= -0.001));
        assert!(
            positions
                .iter()
                .any(|position| position[1] >= CLOUD_DOME_DISTANCE_METRES * 0.99)
        );
    }

    #[test]
    fn solar_chroma_is_neutral_by_day_and_warm_only_near_the_horizon() {
        let midday = cloud_solar_color(38.0);
        let low_sun = cloud_solar_color(6.0);
        assert!(midday.abs_diff_eq(Vec3::ONE, 1e-6));
        assert!(low_sun.x > low_sun.y);
        assert!(low_sun.y > low_sun.z);
        assert!(low_sun.z < midday.z);
    }

    #[test]
    fn cloud_bake_is_deterministic_finite_and_clamp_filtered() {
        use bevy::{image::ImageAddressMode, render::render_resource::FilterMode};

        let request = CloudBakeRequest {
            key: CloudBakeKey {
                layers: CloudLayerParameters::layers_from_environment(&environment(), None),
                seed: 42,
            },
            endpoint: 0,
            wind_velocity: Vec2::ZERO,
        };
        let first = cloud_bake_image(&request);
        let second = cloud_bake_image(&request);
        let data = first.data.as_ref().expect("generated bake has pixels");
        let extent = first.texture_descriptor.size;
        assert_eq!(extent.width, CLOUD_BAKE_TEXTURE_WIDTH);
        assert_eq!(extent.height, CLOUD_BAKE_TEXTURE_HEIGHT);
        assert_eq!(extent.depth_or_array_layers, 1);
        assert_eq!(first.texture_descriptor.dimension, TextureDimension::D2);
        assert_eq!(first.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(
            data.len(),
            CLOUD_BAKE_TEXTURE_WIDTH as usize
                * CLOUD_BAKE_TEXTURE_HEIGHT as usize
                * CLOUD_BAKE_CHANNELS
        );
        assert_eq!(first.data, second.data);
        let ImageSampler::Descriptor(sampler) = &first.sampler else {
            panic!("cloud bake must use an explicit clamp sampler");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.mag_filter, FilterMode::Linear.into());
        assert_eq!(sampler.min_filter, FilterMode::Linear.into());
        assert_eq!(sampler.mipmap_filter, FilterMode::Linear.into());
    }

    #[test]
    fn directional_bake_maps_native_dome_rays_and_duplicates_the_azimuth_seam() {
        assert!(cloud_bake_direction(0, 0).y < 0.001);
        assert!(
            cloud_bake_direction(
                CLOUD_BAKE_AZIMUTH_SEGMENTS / 2,
                CLOUD_BAKE_ELEVATION_SEGMENTS,
            )
            .y > 0.999
        );
        assert!(
            cloud_bake_direction(0, CLOUD_BAKE_ELEVATION_SEGMENTS / 2).abs_diff_eq(
                cloud_bake_direction(
                    CLOUD_BAKE_AZIMUTH_SEGMENTS,
                    CLOUD_BAKE_ELEVATION_SEGMENTS / 2,
                ),
                0.0,
            )
        );
        let image = cloud_bake_image(&CloudBakeRequest {
            key: CloudBakeKey {
                layers: CloudLayerParameters::layers_from_environment(&environment(), None),
                seed: 42,
            },
            endpoint: 0,
            wind_velocity: Vec2::ZERO,
        });
        let data = image.data.expect("generated bake has pixels");
        for row in 0..CLOUD_BAKE_TEXTURE_HEIGHT as usize {
            let first = row * CLOUD_BAKE_TEXTURE_WIDTH as usize * CLOUD_BAKE_CHANNELS;
            let final_column =
                first + (CLOUD_BAKE_TEXTURE_WIDTH as usize - 1) * CLOUD_BAKE_CHANNELS;
            assert_eq!(
                &data[first..first + CLOUD_BAKE_CHANNELS],
                &data[final_column..final_column + CLOUD_BAKE_CHANNELS],
            );
        }
    }

    #[test]
    fn cloud_shell_is_locally_flat_but_bends_into_the_horizon() {
        let surface_metres = 2_000.0;
        let nearby = cloud_shell_altitude_at_distance(surface_metres, 1_000.0);
        let distant = cloud_shell_altitude_at_distance(surface_metres, 20_000.0);
        assert!((nearby - surface_metres).abs() < 4.0);
        assert!(distant < surface_metres - 1_000.0);
        assert!(distant > 0.0);
        assert_eq!(cloud_shell_geometry().xy(), Vec2::ZERO);
    }

    #[test]
    fn cloud_shader_warps_two_endpoints_and_has_no_marching_or_shadow_loop() {
        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");
        assert!(shader.contains("var cloud_baked_texture_a: texture_2d<f32>;"));
        assert!(shader.contains("var cloud_baked_texture_b: texture_2d<f32>;"));
        assert!(shader.contains("fn sample_cloud_surface"));
        assert!(shader.contains("fn wind_compensated_direction"));
        assert!(shader.contains("fn ray_sphere_roots"));
        assert!(shader.contains("atan2(ray_direction.z, ray_direction.x)"));
        assert!(shader.contains("asin(clamp(ray_direction.y, 0.0, 1.0))"));
        assert!(shader.contains("let horizon_fade"));
        assert!(shader.contains("let storminess"));
        assert!(shader.contains("let ray_opacity = baked.r"));
        assert!(shader.contains("-log(max(1.0 - baked_a.r"));
        assert_eq!(shader.matches("textureSample(").count(), 2);
        assert!(!shader.contains("vertical_depth"));
        assert!(!shader.contains("max(ray_direction.y, 0.14)"));
        assert!(!shader.contains("fract("));
        assert!(!shader.contains("Repeat"));
        assert!(!shader.contains("texture_3d"));
        assert!(!shader.contains("sample_density"));
        assert!(!shader.contains("sunlight_transmittance"));
        assert!(!shader.contains("cloud_render_budget"));
        assert!(!shader.contains("for ("));
    }

    #[test]
    fn temporal_endpoints_advect_and_evolve_a_stable_cloud_field() {
        let request = CloudBakeRequest {
            key: CloudBakeKey {
                layers: CloudLayerParameters::layers_from_environment(&environment(), None),
                seed: 42,
            },
            endpoint: 2,
            wind_velocity: Vec2::new(3.0, -4.0),
        };
        assert_eq!(request.elapsed_seconds(), 90.0);
        assert_eq!(request.advection_metres(), Vec2::new(270.0, -360.0));
        assert!(request.evolution() > 0.0);

        let cumulus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cumulus).unwrap();
        let world = Vec2::new(640.0, -420.0);
        let initial = cloud_density_coordinate(world, 0.34, cumulus, 42, 0.0);
        let evolved = cloud_density_coordinate(world, 0.34, cumulus, 42, request.evolution());
        assert!(!initial.abs_diff_eq(evolved, 0.001));
    }

    #[test]
    fn cloud_wind_uses_authoritative_bearing_and_bounded_speed() {
        let mut environment = environment();
        environment.weather.wind_speed_bps = 10_000;
        environment.weather.atmosphere.wind_direction_degrees = 90;
        let velocity = cloud_wind_velocity(&environment);
        assert!((velocity.x - CLOUD_MAX_WIND_METRES_PER_SECOND).abs() < 0.001);
        assert!(velocity.y.abs() < 0.001);
    }
}
