//! Bounded procedural cloud shells for the grounded tactical camera.

use super::*;
use fabelgeist_determinism::splitmix64;

const CLOUD_SHADER: &str = "shaders/tactical_clouds.wgsl";
const CLOUD_DOME_DISTANCE_METRES: f32 = 20_000.0;
/// The tactical sky is already strongly aerial-perspective limited at this
/// range. Keeping this bounded avoids spending cloud march work on a horizon
/// that is visually absorbed into haze.
const CLOUD_MAX_TRACE_DISTANCE_METRES: f32 = 16_000.0;
#[cfg(test)]
const CLOUD_AERIAL_FADE_START_FRACTION: f32 = 0.68;
/// Deliberately smaller than Earth's radius so cloud decks bend into the
/// tactical horizon within the renderer's bounded trace distance.
const CLOUD_CURVATURE_RADIUS_METRES: f32 = 180_000.0;
const CLOUD_AERIAL_EXTINCTION_PER_METRE: f32 = 0.000_025;
/// A small, filterable volume is enough because the cloud shader combines
/// separate broad and detail coordinates. Keeping this below WebGPU's lowest
/// practical 3D texture limit also makes the resource safe for browsers.
const CLOUD_NOISE_VOLUME_EDGE: u32 = 32;
const CLOUD_NOISE_VOLUME_CHANNELS: usize = 4;
const CLOUD_NOISE_VOLUME_SEED: u64 = 0x4f7a_95c3_1bd2_e608;
const CLOUD_NOISE_CHANNEL_STRIDE: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Component)]
pub(crate) struct TacticalCloudLayer {
    slot: usize,
    pub(crate) active: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
    sort_bias: f32,
    /// Direction toward the Sun and scene-referred cloud luminance.
    #[uniform(0)]
    lighting: Vec4,
    /// Coverage, density, profile family, deterministic noise seed.
    #[uniform(1)]
    shape: Vec4,
    /// Bottom altitude, thickness, horizontal noise scale, trace distance.
    #[uniform(2)]
    layer: Vec4,
    /// Wind offset in metres, direct-light fraction, weather transmission.
    #[uniform(3)]
    motion: Vec4,
    /// Solar RGB chroma derived from altitude; alpha is reserved.
    #[uniform(4)]
    spectral: Vec4,
    /// Fixed scene anchor X/Z, curvature radius, aerial extinction.
    #[uniform(5)]
    geometry: Vec4,
    /// Shared deterministic broad/detail/warp noise volume.
    #[texture(6, dimension = "3d")]
    #[sampler(7)]
    noise_volume: Handle<Image>,
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

    fn depth_bias(&self) -> f32 {
        self.sort_bias
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The camera remains inside this shell. Disabling culling also avoids
        // depending on a second inside-out mesh asset in the browser bundle.
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
        Self {
            coverage: f32::from(layer.coverage_bps) / 10_000.0,
            density: 0.4 + f32::from(layer.optical_density_bps) / 10_000.0,
            profile,
            seed: 0.0,
            bottom_metres: f32::from(layer.base_metres),
            thickness_metres: f32::from(
                layer.top_metres.saturating_sub(layer.base_metres).max(100),
            ),
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
    // All decks share this fixed volume. Per-deck deterministic seeds remain
    // offsets in world-space sampling coordinates, not texture allocations.
    let noise_volume = images.add(cloud_noise_volume_image());
    for slot in 0..3 {
        commands.spawn((
            Name::new(format!("Procedural tactical cloud deck {slot}")),
            TacticalCloudLayer {
                slot,
                active: false,
            },
            NoFrustumCulling,
            NotShadowCaster,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(materials.add(TacticalCloudMaterial {
                sort_bias: cloud_sort_bias(slot),
                lighting: Vec4::new(0.0, 1.0, 0.0, 1.43),
                shape: Vec4::new(0.45, 0.9, 0.0, 0.0),
                layer: cloud_layer_uniform(1_250.0, 1_850.0, 0.000_34),
                motion: Vec4::new(0.0, 0.0, 1.0, 1.0),
                spectral: Vec4::ONE,
                geometry: cloud_shell_geometry(),
                noise_volume: noise_volume.clone(),
            })),
            Transform::default(),
        ));
    }
}

/// Generates one fixed RGBA volume for every cloud layer.
///
/// R is broad coverage, G is fine erosion, and B/A are independent X/Z
/// domain-warp offsets.  The shader samples this volume twice for detailed
/// density (rather than evaluating ten eight-corner value-noise calls) and
/// once for coarse occupancy (rather than two such calls).
fn cloud_noise_volume_image() -> Image {
    let edge = CLOUD_NOISE_VOLUME_EDGE as usize;
    let mut pixels = Vec::with_capacity(edge * edge * edge * CLOUD_NOISE_VOLUME_CHANNELS);
    for z in 0..CLOUD_NOISE_VOLUME_EDGE {
        for y in 0..CLOUD_NOISE_VOLUME_EDGE {
            for x in 0..CLOUD_NOISE_VOLUME_EDGE {
                let cell = u64::from(x) | (u64::from(y) << 8) | (u64::from(z) << 16);
                for channel in 0..CLOUD_NOISE_VOLUME_CHANNELS {
                    let value = splitmix64(
                        CLOUD_NOISE_VOLUME_SEED
                            ^ cell.rotate_left((channel as u32 + 1) * 11)
                            ^ (channel as u64).wrapping_mul(CLOUD_NOISE_CHANNEL_STRIDE),
                    );
                    pixels.push((value >> 56) as u8);
                }
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: CLOUD_NOISE_VOLUME_EDGE,
            height: CLOUD_NOISE_VOLUME_EDGE,
            depth_or_array_layers: CLOUD_NOISE_VOLUME_EDGE,
        },
        TextureDimension::D3,
        pixels,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

fn cloud_hemisphere_mesh() -> Mesh {
    // Ray directions are interpolated across this shell in the fragment
    // shader. Dense elevation tessellation is especially important near the
    // horizon: coarse rings become visible as horizontal density bands long
    // before their polygon silhouettes are otherwise noticeable.
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

fn cloud_sort_bias(slot: usize) -> f32 {
    // Bevy's transparent sort values increase toward the camera. The shells
    // share one camera-centred origin, so an explicit bias makes high cloud
    // render first and low cloud composite over it last.
    (2_usize.saturating_sub(slot) as f32) * 1_000.0
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects cloud scene state, lighting, capture controls, and material storage independently"
)]
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
    let Some(environment) = active
        .entity
        .and_then(|entity| environments.get(entity).ok())
    else {
        for (mut cloud, _, _, mut visibility) in &mut clouds {
            cloud.active = false;
            *visibility = cloud_visibility(cloud.active, *isolation);
        }
        return;
    };
    let Some(celestial) = celestial.snapshot.as_ref() else {
        for (mut cloud, _, _, mut visibility) in &mut clouds {
            cloud.active = false;
            *visibility = cloud_visibility(cloud.active, *isolation);
        }
        return;
    };
    let seed = cloud_seed(environment);
    let layers = CloudLayerParameters::layers_from_environment(environment, capture.0);
    let bearing = f32::from(environment.weather.atmosphere.wind_direction_degrees).to_radians();
    let base_wind_direction = Vec2::new(bearing.sin(), -bearing.cos());
    let wind_speed = 2.0 + f32::from(environment.weather.wind_speed_bps) / 10_000.0 * 16.0;
    let elapsed =
        time.elapsed_secs() + (environment.absolute_minute % (7 * MINUTES_PER_DAY)) as f32 * 60.0;
    let daylight = smoothstep(-8.0, 8.0, celestial.sun_altitude_degrees);
    let scene_luminance = 0.08 + daylight * 1.35;
    let solar_color = cloud_solar_color(celestial.sun_altitude_degrees);
    let shear = f32::from(environment.weather.atmosphere.wind_shear_bps) / 10_000.0;

    for (mut cloud, handle, mut transform, mut visibility) in &mut clouds {
        transform.translation = camera.translation();
        let Some(mut parameters) = layers[cloud.slot] else {
            cloud.active = false;
            *visibility = cloud_visibility(cloud.active, *isolation);
            continue;
        };
        if parameters.coverage <= 0.001 || parameters.density <= 0.001 {
            cloud.active = false;
            *visibility = cloud_visibility(cloud.active, *isolation);
            continue;
        }
        cloud.active = true;
        let Some(mut material) = materials.get_mut(&handle.0) else {
            continue;
        };
        parameters.seed = ((seed.wrapping_add(cloud.slot as u64 * 1_013)) % 4_096) as f32;
        let altitude_fraction = cloud.slot as f32 * 0.5;
        let wind_direction =
            Mat2::from_angle(shear * altitude_fraction * 0.7) * base_wind_direction;
        let layer_wind_speed = wind_speed * (1.0 + altitude_fraction * (0.35 + shear * 0.65));
        let wind_offset = wind_direction * layer_wind_speed * elapsed;
        material.lighting = celestial.sun_direction.extend(scene_luminance);
        material.shape = Vec4::new(
            parameters.coverage,
            parameters.density,
            parameters.profile,
            parameters.seed,
        );
        material.layer = cloud_layer_uniform(
            parameters.bottom_metres,
            parameters.thickness_metres,
            parameters.horizontal_scale,
        );
        material.motion = Vec4::new(
            wind_offset.x,
            wind_offset.y,
            daylight,
            celestial.weather_transmission,
        );
        material.spectral = solar_color.extend(1.0);
        material.geometry = cloud_shell_geometry();
        *visibility = cloud_visibility(cloud.active, *isolation);
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
    // below that origin keeps its curvature stable as the camera crosses the
    // playable area; the camera-following mesh remains only a raster proxy.
    Vec4::new(
        0.0,
        0.0,
        CLOUD_CURVATURE_RADIUS_METRES,
        CLOUD_AERIAL_EXTINCTION_PER_METRE,
    )
}

/// Every cloud material receives its trace limit through this one path. The
/// default shell material and its per-frame weather/capture refresh therefore
/// cannot accidentally use different visibility horizons.
fn cloud_layer_uniform(bottom_metres: f32, thickness_metres: f32, horizontal_scale: f32) -> Vec4 {
    Vec4::new(
        bottom_metres,
        thickness_metres,
        horizontal_scale,
        CLOUD_MAX_TRACE_DISTANCE_METRES,
    )
}

#[cfg(test)]
fn cloud_aerial_fade_start_metres() -> f32 {
    CLOUD_MAX_TRACE_DISTANCE_METRES * CLOUD_AERIAL_FADE_START_FRACTION
}

#[cfg(test)]
fn cloud_shell_vertical_trace_interval(layer: CloudLayerParameters) -> Option<(f32, f32)> {
    // At the tactical camera anchor, the upward vertical ray reaches the
    // shell at the layer's configured bottom and top altitudes. This is the
    // least ambiguous guaranteed intersection for every supported deck;
    // oblique rays have a longer shell path until curvature turns them below
    // the layer.
    let start = layer.bottom_metres;
    let end = layer.bottom_metres + layer.thickness_metres;
    (end > start && start < CLOUD_MAX_TRACE_DISTANCE_METRES)
        .then_some((start, end.min(CLOUD_MAX_TRACE_DISTANCE_METRES)))
}

#[cfg(test)]
fn cloud_shell_altitude_at_distance(base_metres: f32, horizontal_metres: f32) -> f32 {
    let radius = CLOUD_CURVATURE_RADIUS_METRES + base_metres;
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
    // Atmospheric extinction warms direct sunlight only near the horizon.
    // Above 22 degrees the tactical cloud light is effectively neutral;
    // retaining a permanent golden tint made daytime storm clouds olive when
    // composited over the blue atmosphere.
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
    fn authoritative_weather_produces_three_distinct_cloud_decks() {
        let layers = CloudLayerParameters::layers_from_environment(&environment(), None);
        let low = layers[0].unwrap();
        let middle = layers[1].unwrap();
        let high = layers[2].unwrap();
        assert_eq!(low.profile, 3.0);
        assert_eq!(middle.profile, 6.0);
        assert_eq!(high.profile, 2.0);
        assert!(low.bottom_metres < middle.bottom_metres);
        assert!(middle.bottom_metres < high.bottom_metres);
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
    }

    #[test]
    fn benchmark_isolation_hides_an_active_cloud_layer() {
        assert_eq!(
            cloud_visibility(true, TacticalCloudBenchmarkIsolation::default()),
            Visibility::Inherited
        );
        assert_eq!(
            cloud_visibility(true, TacticalCloudBenchmarkIsolation { hide_clouds: true },),
            Visibility::Hidden
        );
    }

    #[test]
    fn every_diagnosed_cloud_form_has_a_distinct_shader_profile() {
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
        assert!(CloudLayerParameters::capture(TacticalCloudCaptureProfile::Clear).is_none());
    }

    #[test]
    fn raster_shell_contains_only_the_upper_hemisphere() {
        let mesh = cloud_hemisphere_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("cloud shell must contain float positions");
        };
        assert!(!positions.is_empty());
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
    fn cloud_decks_composite_high_to_low() {
        assert!(cloud_sort_bias(2) < cloud_sort_bias(1));
        assert!(cloud_sort_bias(1) < cloud_sort_bias(0));
    }

    #[test]
    fn cloud_noise_volume_is_small_repeatable_and_filterable() {
        use bevy::{image::ImageAddressMode, render::render_resource::FilterMode};

        let first = cloud_noise_volume_image();
        let second = cloud_noise_volume_image();
        let data = first.data.as_ref().expect("generated noise has pixels");
        let extent = first.texture_descriptor.size;
        assert_eq!(extent.width, 32);
        assert_eq!(extent.height, 32);
        assert_eq!(extent.depth_or_array_layers, 32);
        assert_eq!(first.texture_descriptor.dimension, TextureDimension::D3);
        assert_eq!(first.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(data.len(), 32 * 32 * 32 * 4);
        assert_eq!(first.data, second.data);
        assert_eq!(cloud_noise_checksum(data), 0x2400_0361_e7a5_5835);
        let ImageSampler::Descriptor(sampler) = &first.sampler else {
            panic!("cloud noise must use an explicit repeat sampler");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_w, ImageAddressMode::Repeat);
        assert_eq!(sampler.mag_filter, FilterMode::Linear.into());
        assert_eq!(sampler.min_filter, FilterMode::Linear.into());
        assert_eq!(sampler.mipmap_filter, FilterMode::Linear.into());
    }

    fn cloud_noise_checksum(data: &[u8]) -> u64 {
        data.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[test]
    fn cloud_shell_is_locally_flat_but_bends_into_the_horizon() {
        let base_metres = 1_250.0;
        let nearby = cloud_shell_altitude_at_distance(base_metres, 1_000.0);
        let distant = cloud_shell_altitude_at_distance(base_metres, 20_000.0);

        assert!((nearby - base_metres).abs() < 4.0);
        assert!(distant < base_metres - 1_000.0);
        assert!(distant > 0.0);
        assert_eq!(cloud_shell_geometry().xy(), Vec2::ZERO);
    }

    #[test]
    fn tactical_trace_limit_covers_every_supported_cloud_deck() {
        // These are the tallest bounds emitted by the authoritative weather
        // diagnosis: low convective, middle, and high decks. Capture
        // overrides are tested below as independently constructed layers.
        let diagnosed_layers = [
            CloudLayerSnapshot {
                form: CloudForm::Cumulonimbus,
                coverage_bps: 8_500,
                optical_density_bps: 8_500,
                base_metres: 3_000,
                top_metres: 12_000,
            },
            CloudLayerSnapshot {
                form: CloudForm::Nimbostratus,
                coverage_bps: 8_000,
                optical_density_bps: 8_000,
                base_metres: 2_600,
                top_metres: 5_400,
            },
            CloudLayerSnapshot {
                form: CloudForm::Cirrostratus,
                coverage_bps: 7_000,
                optical_density_bps: 3_750,
                base_metres: 6_200,
                top_metres: 10_500,
            },
        ];
        let captures = [
            TacticalCloudCaptureProfile::Cumulus,
            TacticalCloudCaptureProfile::Stratocumulus,
            TacticalCloudCaptureProfile::Cirrus,
            TacticalCloudCaptureProfile::Overcast,
            TacticalCloudCaptureProfile::Storm,
        ];
        let layers = diagnosed_layers
            .into_iter()
            .map(CloudLayerParameters::from_layer)
            .chain(
                captures
                    .into_iter()
                    .map(|profile| CloudLayerParameters::capture(profile).unwrap()),
            );

        for layer in layers {
            let (start, end) = cloud_shell_vertical_trace_interval(layer)
                .expect("supported cloud deck must intersect the tactical upward ray");
            assert!(start < end);
            assert!(end <= CLOUD_MAX_TRACE_DISTANCE_METRES);
        }
    }

    #[test]
    fn cloud_trace_distance_and_aerial_fade_are_pinned() {
        assert_eq!(CLOUD_MAX_TRACE_DISTANCE_METRES, 16_000.0);
        assert_eq!(cloud_aerial_fade_start_metres(), 10_880.0);
        assert_eq!(
            cloud_layer_uniform(1_250.0, 1_850.0, 0.000_34),
            Vec4::new(1_250.0, 1_850.0, 0.000_34, 16_000.0),
        );

        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");
        assert!(shader.contains(
            "let distance_fade = 1.0 - smoothstep(cloud_layer.w * 0.68, cloud_layer.w, distance);"
        ));
    }

    #[test]
    fn cloud_shader_intersects_shells_and_adapts_its_sampling() {
        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");

        assert!(shader.contains("ray_sphere_roots"));
        assert!(shader.contains("altitude_in_shell"));
        assert!(shader.contains("let ray_jitter"));
        assert!(shader.contains("var fine_marching = false"));
        assert!(!shader.contains("(cloud_layer.x - ray_origin.y) / ray_direction.y"));
    }

    #[test]
    fn cloud_shader_uses_coarse_occupancy_before_detailed_integration() {
        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");
        let coarse_density = shader
            .split_once("fn sample_density_coarse")
            .expect("cloud shader must define coarse occupancy sampling")
            .1
            .split_once("fn sunlight_transmittance")
            .expect("coarse sampling must precede lighting")
            .0;
        let fragment = shader
            .split_once("@fragment")
            .expect("cloud shader must have a fragment entry point")
            .1;

        assert!(shader.contains("var cloud_noise_volume: texture_3d<f32>;"));
        assert!(shader.contains("var cloud_noise_sampler: sampler;"));
        assert!(shader.contains("fn sample_cloud_noise"));
        assert!(!shader.contains("fn value_noise_3d"));
        assert!(!shader.contains("fn fbm("));
        assert!(!shader.contains("fn fbm_coarse"));
        let detailed_density = shader
            .split_once("fn sample_density(")
            .expect("cloud shader must define detailed density")
            .1
            .split_once("fn sample_density_coarse")
            .expect("detailed density must precede coarse sampling")
            .0;
        assert_eq!(detailed_density.matches("sample_cloud_noise").count(), 2);
        assert_eq!(coarse_density.matches("sample_cloud_noise").count(), 1);
        assert!(!detailed_density.contains("hash13("));
        assert!(!coarse_density.contains("hash13("));
        assert!(!coarse_density.contains("let warp"));
        assert!(!coarse_density.contains("let detail"));
        assert!(coarse_density.contains("threshold - 0.22"));
        assert!(fragment.contains("density = sample_density_coarse(position)"));
        assert!(
            fragment.contains("if fine_marching {\n            density = sample_density(position)")
        );
        assert!(
            fragment.contains("} else {\n            density = sample_density_coarse(position)")
        );
        assert!(shader.contains("optical_depth += sample_density(sample_position)"));
    }

    #[test]
    fn cloud_shader_amortizes_detailed_self_shadow_sampling() {
        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");
        let shadow = shader
            .split_once("fn sunlight_transmittance")
            .expect("cloud shader must define direct-light self-shadowing")
            .1
            .split_once("fn henyey_greenstein")
            .expect("self-shadowing must precede phase lighting")
            .0;
        let fragment = shader
            .split_once("@fragment")
            .expect("cloud shader must have a fragment entry point")
            .1;

        assert!(shadow.contains("optical_depth += sample_density(sample_position)"));
        assert!(shadow.contains("step < CLOUD_MAX_SUNLIGHT_PROBES"));
        assert!(shader.contains("const CLOUD_CONVECTIVE_COARSE_INTERVALS = 24.0;"));
        assert!(shader.contains("const CLOUD_FINE_STEP_SCALE = 0.5;"));
        assert!(shader.contains("const CLOUD_CONVECTIVE_MAX_MARCH_STEPS = 48u;"));
        assert!(shader.contains("const CLOUD_MAX_MARCH_STEPS = CLOUD_CONVECTIVE_MAX_MARCH_STEPS;"));
        assert!(fragment.contains("/ budget.coarse_intervals"));
        assert!(fragment.contains("coarse_step * CLOUD_FINE_STEP_SCALE"));
        assert!(fragment.contains("step < CLOUD_MAX_MARCH_STEPS"));
        assert!(fragment.contains("if step >= budget.max_march_steps"));
        assert!(fragment.contains("var occupied_fine_steps = 0u;"));
        assert!(fragment.contains("var sun_visibility = 1.0;"));
        assert!(fragment.contains(
            "if occupied_fine_steps % budget.sunlight_refresh_interval == 0u {\n                sun_visibility = sunlight_transmittance(\n                    position,\n                    sun_direction,\n                    budget.sunlight_probe_count,\n                );\n            }\n            occupied_fine_steps += 1u;"
        ));
        assert!(fragment.contains("occupied_fine_steps = 0u;"));
        assert!(!fragment.contains("let sun_visibility = sunlight_transmittance"));
    }

    #[test]
    fn cloud_shader_pins_profile_specific_march_and_lighting_budgets() {
        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");

        assert!(shader.contains("fn cloud_render_budget(family: f32) -> CloudRenderBudget"));
        assert!(
            shader
                .contains("if kind == 4u {\n        return CloudRenderBudget(10.0, 16u, 1u, 8u);")
        );
        assert!(
            shader
                .contains("if kind == 6u {\n        return CloudRenderBudget(12.0, 20u, 1u, 10u);")
        );
        assert!(
            shader
                .contains("if kind == 7u {\n        return CloudRenderBudget(14.0, 24u, 1u, 8u);")
        );
        assert!(
            shader
                .contains("if kind == 9u {\n        return CloudRenderBudget(8.0, 12u, 1u, 12u);")
        );
        assert!(
            shader
                .contains("if kind == 2u {\n        return CloudRenderBudget(10.0, 16u, 1u, 12u);")
        );
        assert!(shader.contains(
            "if kind == 5u || kind == 8u {\n        return CloudRenderBudget(16.0, 32u, 1u, 8u);"
        ));
        assert!(
            shader
                .contains("if kind == 1u {\n        return CloudRenderBudget(18.0, 36u, 2u, 6u);")
        );
        assert!(shader.contains(
            "CLOUD_CONVECTIVE_COARSE_INTERVALS,\n        CLOUD_CONVECTIVE_MAX_MARCH_STEPS,\n        2u,\n        4u,"
        ));
    }
}
