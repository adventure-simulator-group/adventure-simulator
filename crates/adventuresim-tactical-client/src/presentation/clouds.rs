//! A single analytic cloud shell for the grounded tactical camera.

use super::*;

const CLOUD_SHADER: &str = "shaders/tactical_clouds.wgsl";
const CLOUD_DOME_DISTANCE_METRES: f32 = 20_000.0;
/// Deliberately smaller than Earth's radius so the global cloud surface bends
/// into the tactical horizon while staying visually flat around the player.
const CLOUD_CURVATURE_RADIUS_METRES: f32 = 180_000.0;
/// A fixed 2D texture supplies broad coverage, edge variation, underside
/// shade, and a domain offset in one filtered sample.
const CLOUD_NOISE_TEXTURE_EDGE: u32 = 128;
const CLOUD_NOISE_TEXTURE_CHANNELS: usize = 4;
const CLOUD_NOISE_TEXTURE_SEED: u64 = 0x4f7a_95c3_1bd2_e608;

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
    /// Shared deterministic coverage/variation texture.
    #[texture(6, dimension = "2d")]
    #[sampler(7)]
    noise_texture: Handle<Image>,
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
    surface_metres: f32,
    horizontal_scale: f32,
}

impl CloudLayerParameters {
    /// The global representation has one shell. Select the weather deck with
    /// the greatest optical presence, then fold every diagnosed deck into its
    /// coverage. This preserves storm/rain identity without transparent
    /// overdraw from three independent shells.
    fn from_environment(
        environment: &SceneEnvironment,
        capture: Option<TacticalCloudCaptureProfile>,
    ) -> Option<Self> {
        if let Some(profile) = capture {
            return Self::capture(profile);
        }

        let atmosphere = environment.weather.atmosphere;
        let layers = [
            atmosphere.low_cloud,
            atmosphere.middle_cloud,
            atmosphere.high_cloud,
        ]
        .into_iter()
        .flatten()
        .map(Self::from_layer)
        .collect::<Vec<_>>();
        let mut primary = *layers
            .iter()
            .max_by(|left, right| left.optical_presence().total_cmp(&right.optical_presence()))?;
        primary.coverage = 1.0
            - layers
                .iter()
                .fold(1.0, |clear, layer| clear * (1.0 - layer.coverage));
        primary.density = layers
            .iter()
            .fold(primary.density, |density, layer| density.max(layer.density));
        Some(primary)
    }

    fn optical_presence(self) -> f32 {
        let storm_bias = if matches!(self.profile as u8, 3 | 7) {
            0.25
        } else {
            0.0
        };
        self.coverage * self.density + storm_bias
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
            surface_metres: f32::from(layer.base_metres) + thickness_metres * 0.42,
            horizontal_scale,
        }
    }
}

pub(in crate::presentation) fn setup_tactical_clouds(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TacticalCloudMaterial>>,
) {
    let mesh = meshes.add(cloud_hemisphere_mesh());
    let noise_texture = images.add(cloud_noise_texture_image());
    commands.spawn((
        Name::new("Analytic tactical cloud shell"),
        TacticalCloudLayer { active: false },
        NoFrustumCulling,
        NotShadowCaster,
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(TacticalCloudMaterial {
            lighting: Vec4::new(0.0, 1.0, 0.0, 1.43),
            shape: Vec4::new(0.45, 0.9, 0.0, 0.0),
            layer: cloud_surface_uniform(2_000.0, 0.000_34),
            motion: Vec4::new(0.0, 0.0, 1.0, 1.0),
            spectral: Vec4::ONE,
            geometry: cloud_shell_geometry(),
            noise_texture,
        })),
        Transform::default(),
    ));
}

/// Generates one fixed, filterable RGBA texture for all weather states.
///
/// Each channel is periodic value noise assembled from a few low-frequency
/// octaves. Generating the coherence here leaves the shader with one ordinary
/// filtered lookup instead of trying to reconstruct broad cloud masses from
/// independent texels.
fn cloud_noise_texture_image() -> Image {
    let edge = CLOUD_NOISE_TEXTURE_EDGE as usize;
    let mut pixels = Vec::with_capacity(edge * edge * CLOUD_NOISE_TEXTURE_CHANNELS);
    for y in 0..CLOUD_NOISE_TEXTURE_EDGE {
        for x in 0..CLOUD_NOISE_TEXTURE_EDGE {
            let channels = [
                coherent_cloud_noise(x, y, 0, &[(3, 0.58), (6, 0.30), (12, 0.12)]),
                coherent_cloud_noise(x, y, 1, &[(5, 0.56), (10, 0.30), (20, 0.14)]),
                coherent_cloud_noise(x, y, 2, &[(2, 0.60), (4, 0.28), (8, 0.12)]),
                coherent_cloud_noise(x, y, 3, &[(4, 0.68), (8, 0.32)]),
            ];
            pixels.extend(channels.map(|value| (value * 255.0).round() as u8));
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: CLOUD_NOISE_TEXTURE_EDGE,
            height: CLOUD_NOISE_TEXTURE_EDGE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

fn coherent_cloud_noise(x: u32, y: u32, channel: u64, octaves: &[(u32, f32)]) -> f32 {
    octaves
        .iter()
        .map(|&(frequency, weight)| periodic_value_noise(x, y, frequency, channel) * weight)
        .sum()
}

/// Bilinearly interpolated lattice noise with a wrapped lattice. Sampling the
/// generated texture with a repeat sampler is therefore continuous at both
/// texture seams as well as spatially coherent within every broad cloud body.
fn periodic_value_noise(x: u32, y: u32, frequency: u32, channel: u64) -> f32 {
    debug_assert!(frequency > 0);
    let scaled_x = x * frequency;
    let scaled_y = y * frequency;
    let x0 = scaled_x / CLOUD_NOISE_TEXTURE_EDGE;
    let y0 = scaled_y / CLOUD_NOISE_TEXTURE_EDGE;
    let x1 = (x0 + 1) % frequency;
    let y1 = (y0 + 1) % frequency;
    let tx = smooth_noise_fraction(scaled_x % CLOUD_NOISE_TEXTURE_EDGE);
    let ty = smooth_noise_fraction(scaled_y % CLOUD_NOISE_TEXTURE_EDGE);
    let lower = cloud_lattice_value(x0, y0, frequency, channel)
        .lerp(cloud_lattice_value(x1, y0, frequency, channel), tx);
    let upper = cloud_lattice_value(x0, y1, frequency, channel)
        .lerp(cloud_lattice_value(x1, y1, frequency, channel), tx);
    lower.lerp(upper, ty)
}

fn smooth_noise_fraction(remainder: u32) -> f32 {
    let fraction = remainder as f32 / CLOUD_NOISE_TEXTURE_EDGE as f32;
    fraction * fraction * (3.0 - 2.0 * fraction)
}

fn cloud_lattice_value(x: u32, y: u32, frequency: u32, channel: u64) -> f32 {
    let cell = u64::from(x) | (u64::from(y) << 16) | (u64::from(frequency) << 32);
    let value = cloud_noise_hash(
        CLOUD_NOISE_TEXTURE_SEED
            ^ cell.rotate_left((channel as u32 + 1) * 13)
            ^ channel.wrapping_mul(0x9e37_79b9_7f4a_7c15),
    );
    (value >> 40) as f32 / 16_777_215.0
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
    const AZIMUTH_SEGMENTS: u32 = 64;
    const ELEVATION_SEGMENTS: u32 = 24;
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
) {
    let environment = active
        .entity
        .and_then(|entity| environments.get(entity).ok());
    let celestial = celestial.snapshot.as_ref();
    let parameters = environment
        .and_then(|environment| CloudLayerParameters::from_environment(environment, capture.0));

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
        let Some(mut parameters) = parameters else {
            cloud.active = false;
            *visibility = cloud_visibility(false, *isolation);
            continue;
        };
        if parameters.coverage <= 0.001 || parameters.density <= 0.001 {
            cloud.active = false;
            *visibility = cloud_visibility(false, *isolation);
            continue;
        }
        cloud.active = true;
        let Some(mut material) = materials.get_mut(&handle.0) else {
            continue;
        };
        let seed = cloud_seed(environment);
        parameters.seed = (seed % 4_096) as f32;
        let bearing = f32::from(environment.weather.atmosphere.wind_direction_degrees).to_radians();
        let base_wind_direction = Vec2::new(bearing.sin(), -bearing.cos());
        let wind_speed = 2.0 + f32::from(environment.weather.wind_speed_bps) / 10_000.0 * 16.0;
        let elapsed = time.elapsed_secs()
            + (environment.absolute_minute % (7 * MINUTES_PER_DAY)) as f32 * 60.0;
        let daylight = smoothstep(-8.0, 8.0, celestial.sun_altitude_degrees);
        let scene_luminance = 0.08 + daylight * 1.35;
        let solar_color = cloud_solar_color(celestial.sun_altitude_degrees);
        let shear = f32::from(environment.weather.atmosphere.wind_shear_bps) / 10_000.0;
        let wind_direction = Mat2::from_angle(shear * 0.35) * base_wind_direction;
        let wind_offset = wind_direction * wind_speed * elapsed;
        material.lighting = celestial.sun_direction.extend(scene_luminance);
        material.shape = Vec4::new(
            parameters.coverage,
            parameters.density,
            parameters.profile,
            parameters.seed,
        );
        material.layer =
            cloud_surface_uniform(parameters.surface_metres, parameters.horizontal_scale);
        material.motion = Vec4::new(
            wind_offset.x,
            wind_offset.y,
            daylight,
            celestial.weather_transmission,
        );
        material.spectral = solar_color.extend(1.0);
        material.geometry = cloud_shell_geometry();
        *visibility = cloud_visibility(true, *isolation);
    }
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

fn cloud_surface_uniform(surface_metres: f32, horizontal_scale: f32) -> Vec4 {
    Vec4::new(surface_metres, horizontal_scale, 0.0, 0.0)
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
    fn authoritative_weather_collapses_to_one_storm_shell_with_combined_coverage() {
        let cloud = CloudLayerParameters::from_environment(&environment(), None).unwrap();
        assert_eq!(cloud.profile, 3.0);
        assert!((cloud.coverage - 0.964).abs() < 1e-6);
        assert_eq!(cloud.density, 1.3);
        assert_eq!(cloud.surface_metres, 4_816.0);
    }

    #[test]
    fn capture_profiles_cover_distinct_altitude_and_density_families() {
        let cumulus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cumulus).unwrap();
        let cirrus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cirrus).unwrap();
        let storm = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Storm).unwrap();
        assert!(cirrus.surface_metres > cumulus.surface_metres);
        assert!(cirrus.density < cumulus.density);
        assert!(storm.density > cumulus.density);
        assert!(storm.coverage > cumulus.coverage);
        assert!(CloudLayerParameters::capture(TacticalCloudCaptureProfile::Clear).is_none());
    }

    #[test]
    fn benchmark_isolation_hides_an_active_cloud_layer() {
        assert_eq!(
            cloud_visibility(true, TacticalCloudBenchmarkIsolation::default()),
            Visibility::Inherited
        );
        assert_eq!(
            cloud_visibility(true, TacticalCloudBenchmarkIsolation { hide_clouds: true }),
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
        assert_eq!(positions.len(), 65 * 25);
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
    fn cloud_noise_texture_is_repeatable_and_filterable() {
        use bevy::{image::ImageAddressMode, render::render_resource::FilterMode};

        let first = cloud_noise_texture_image();
        let second = cloud_noise_texture_image();
        let data = first.data.as_ref().expect("generated noise has pixels");
        let extent = first.texture_descriptor.size;
        assert_eq!(extent.width, CLOUD_NOISE_TEXTURE_EDGE);
        assert_eq!(extent.height, CLOUD_NOISE_TEXTURE_EDGE);
        assert_eq!(extent.depth_or_array_layers, 1);
        assert_eq!(first.texture_descriptor.dimension, TextureDimension::D2);
        assert_eq!(first.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(data.len(), 128 * 128 * 4);
        assert_eq!(first.data, second.data);
        assert_eq!(cloud_noise_checksum(data), 0x1e7f_7871_c87e_3dca);
        let ImageSampler::Descriptor(sampler) = &first.sampler else {
            panic!("cloud noise must use an explicit repeat sampler");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        assert_eq!(sampler.mag_filter, FilterMode::Linear.into());
        assert_eq!(sampler.min_filter, FilterMode::Linear.into());
        assert_eq!(sampler.mipmap_filter, FilterMode::Linear.into());
    }

    #[test]
    fn cloud_noise_texture_pins_low_frequency_spatial_coherence() {
        let image = cloud_noise_texture_image();
        let data = image.data.as_ref().expect("generated noise has pixels");
        let adjacent_difference = cloud_channel_difference(data, 1, 0);
        let separated_difference = cloud_channel_difference(data, 17, 11);

        assert!(
            adjacent_difference < 0.035,
            "adjacent cloud samples lost their low-frequency correlation: {adjacent_difference}"
        );
        assert!(
            separated_difference > adjacent_difference * 4.0,
            "cloud texture no longer forms broad bodies: adjacent={adjacent_difference}, separated={separated_difference}"
        );
    }

    fn cloud_noise_checksum(data: &[u8]) -> u64 {
        data.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    fn cloud_channel_difference(data: &[u8], offset_x: usize, offset_y: usize) -> f32 {
        let edge = CLOUD_NOISE_TEXTURE_EDGE as usize;
        let mut total = 0.0;
        for y in 0..edge {
            for x in 0..edge {
                let sample = data[(y * edge + x) * CLOUD_NOISE_TEXTURE_CHANNELS] as f32;
                let other_x = (x + offset_x) % edge;
                let other_y = (y + offset_y) % edge;
                let other = data[(other_y * edge + other_x) * CLOUD_NOISE_TEXTURE_CHANNELS] as f32;
                total += (sample - other).abs() / 255.0;
            }
        }
        total / (edge * edge) as f32
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
        assert_eq!(
            cloud_surface_uniform(2_000.0, 0.000_34),
            Vec4::new(2_000.0, 0.000_34, 0.0, 0.0)
        );
    }

    #[test]
    fn cloud_shader_uses_one_2d_lookup_and_no_marching_or_shadow_loop() {
        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");
        assert!(shader.contains("var cloud_noise_texture: texture_2d<f32>;"));
        assert!(shader.contains("fn sample_cloud_surface"));
        assert!(shader.contains("fn ray_sphere_roots"));
        assert!(shader.contains("let surface_roots"));
        assert!(shader.contains("let horizon_fade"));
        assert!(shader.contains("let storminess"));
        assert!(!shader.contains("texture_3d"));
        assert!(!shader.contains("sample_density"));
        assert!(!shader.contains("sunlight_transmittance"));
        assert!(!shader.contains("cloud_render_budget"));
        assert!(!shader.contains("for ("));
    }
}
