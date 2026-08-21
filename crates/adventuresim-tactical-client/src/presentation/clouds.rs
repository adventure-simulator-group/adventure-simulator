//! Bounded procedural cloud shells for the grounded tactical camera.

use super::*;

const CLOUD_SHADER: &str = "shaders/tactical_clouds.wgsl";
const CLOUD_DOME_DISTANCE_METRES: f32 = 20_000.0;
/// Deliberately smaller than Earth's radius so cloud decks bend into the
/// tactical horizon within the renderer's bounded trace distance.
const CLOUD_CURVATURE_RADIUS_METRES: f32 = 180_000.0;
const CLOUD_AERIAL_EXTINCTION_PER_METRE: f32 = 0.000_025;

#[derive(Component)]
pub(crate) struct TacticalCloudLayer {
    slot: usize,
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
    mut materials: ResMut<Assets<TacticalCloudMaterial>>,
) {
    let mesh = meshes.add(cloud_hemisphere_mesh());
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
                layer: Vec4::new(1_250.0, 1_850.0, 0.000_34, 24_000.0),
                motion: Vec4::new(0.0, 0.0, 1.0, 1.0),
                spectral: Vec4::ONE,
                geometry: cloud_shell_geometry(),
            })),
            Transform::default(),
        ));
    }
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
            *visibility = cloud_visibility(false, *isolation);
        }
        return;
    };
    let Some(celestial) = celestial.snapshot.as_ref() else {
        for (mut cloud, _, _, mut visibility) in &mut clouds {
            cloud.active = false;
            *visibility = cloud_visibility(false, *isolation);
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
        material.layer = Vec4::new(
            parameters.bottom_metres,
            parameters.thickness_metres,
            parameters.horizontal_scale,
            24_000.0,
        );
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
    // below that origin keeps its curvature stable as the camera crosses the
    // playable area; the camera-following mesh remains only a raster proxy.
    Vec4::new(
        0.0,
        0.0,
        CLOUD_CURVATURE_RADIUS_METRES,
        CLOUD_AERIAL_EXTINCTION_PER_METRE,
    )
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
    fn cloud_shader_intersects_shells_and_adapts_its_sampling() {
        let shader = include_str!("../../../../assets/shaders/tactical_clouds.wgsl");

        assert!(shader.contains("ray_sphere_roots"));
        assert!(shader.contains("altitude_in_shell"));
        assert!(shader.contains("let ray_jitter"));
        assert!(shader.contains("var fine_marching = false"));
        assert!(!shader.contains("(cloud_layer.x - ray_origin.y) / ray_direction.y"));
    }
}
