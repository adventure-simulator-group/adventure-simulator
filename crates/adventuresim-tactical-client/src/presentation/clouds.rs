//! Bounded procedural cloud slab for the grounded tactical camera.

use super::*;

const CLOUD_SHADER: &str = "shaders/tactical_clouds.wgsl";
const CLOUD_DOME_DISTANCE_METRES: f32 = 20_000.0;

#[derive(Component)]
pub(crate) struct TacticalCloudLayer;

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

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalCloudMaterial {
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
    fn from_environment(
        environment: &SceneEnvironment,
        capture: Option<TacticalCloudCaptureProfile>,
    ) -> Self {
        if let Some(profile) = capture {
            return Self::capture(profile);
        }

        let seed = cloud_seed(environment);
        let random = ((seed >> 16) & 0xffff) as f32 / 65_535.0;
        let clear_profile = (seed % 3) as f32;
        match environment.weather.precipitation {
            Precipitation::Clear => {
                let coverage = 0.28 + random * 0.34;
                Self::for_profile(clear_profile, coverage, 0.82, seed)
            }
            Precipitation::Rain => {
                let intensity = f32::from(environment.weather.intensity_bps) / 10_000.0;
                Self::for_profile(3.0, 0.78 + intensity * 0.18, 1.0 + intensity * 0.35, seed)
            }
            Precipitation::Snow => {
                let intensity = f32::from(environment.weather.intensity_bps) / 10_000.0;
                Self::for_profile(1.0, 0.76 + intensity * 0.18, 0.92 + intensity * 0.2, seed)
            }
        }
    }

    fn capture(profile: TacticalCloudCaptureProfile) -> Self {
        let seed = match profile {
            TacticalCloudCaptureProfile::Clear => 0,
            TacticalCloudCaptureProfile::Cumulus => 117,
            TacticalCloudCaptureProfile::Stratocumulus => 283,
            TacticalCloudCaptureProfile::Cirrus => 419,
            TacticalCloudCaptureProfile::Overcast => 631,
            TacticalCloudCaptureProfile::Storm => 887,
        };
        match profile {
            TacticalCloudCaptureProfile::Clear => Self::for_profile(0.0, 0.0, 0.0, seed),
            TacticalCloudCaptureProfile::Cumulus => Self::for_profile(0.0, 0.48, 0.9, seed),
            TacticalCloudCaptureProfile::Stratocumulus => Self::for_profile(1.0, 0.68, 0.9, seed),
            TacticalCloudCaptureProfile::Cirrus => Self::for_profile(2.0, 0.42, 0.64, seed),
            TacticalCloudCaptureProfile::Overcast => Self::for_profile(1.0, 0.94, 1.1, seed),
            TacticalCloudCaptureProfile::Storm => Self::for_profile(3.0, 0.91, 1.32, seed),
        }
    }

    fn for_profile(profile: f32, coverage: f32, density: f32, seed: u64) -> Self {
        let (bottom_metres, thickness_metres, horizontal_scale) = match profile as u32 {
            0 => (1_250.0, 1_850.0, 0.000_34),
            1 => (1_050.0, 900.0, 0.000_25),
            2 => (5_500.0, 520.0, 0.000_18),
            _ => (720.0, 2_600.0, 0.000_28),
        };
        Self {
            coverage: coverage.clamp(0.0, 1.0),
            density,
            profile,
            seed: (seed % 4_096) as f32,
            bottom_metres,
            thickness_metres,
            horizontal_scale,
        }
    }
}

pub(in crate::presentation) fn setup_tactical_clouds(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalCloudMaterial>>,
) {
    commands.spawn((
        Name::new("Procedural tactical cloud slab"),
        TacticalCloudLayer,
        NoFrustumCulling,
        NotShadowCaster,
        Mesh3d(meshes.add(cloud_hemisphere_mesh())),
        MeshMaterial3d(materials.add(TacticalCloudMaterial {
            lighting: Vec4::new(0.0, 1.0, 0.0, 1.43),
            shape: Vec4::new(0.45, 0.9, 0.0, 0.0),
            layer: Vec4::new(1_250.0, 1_850.0, 0.000_34, 48_000.0),
            motion: Vec4::new(0.0, 0.0, 1.0, 1.0),
        })),
        Transform::default(),
    ));
}

fn cloud_hemisphere_mesh() -> Mesh {
    const AZIMUTH_SEGMENTS: u32 = 64;
    const ELEVATION_SEGMENTS: u32 = 20;
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
    camera: Single<&GlobalTransform, With<Camera3d>>,
    mut clouds: Single<
        (
            &MeshMaterial3d<TacticalCloudMaterial>,
            &mut Transform,
            &mut Visibility,
        ),
        With<TacticalCloudLayer>,
    >,
    mut materials: ResMut<Assets<TacticalCloudMaterial>>,
) {
    clouds.1.translation = camera.translation();
    let Some(environment) = active
        .entity
        .and_then(|entity| environments.get(entity).ok())
    else {
        *clouds.2 = Visibility::Hidden;
        return;
    };
    let Some(celestial) = celestial.snapshot.as_ref() else {
        *clouds.2 = Visibility::Hidden;
        return;
    };
    let Some(mut material) = materials.get_mut(&clouds.0.0) else {
        return;
    };

    let parameters = CloudLayerParameters::from_environment(environment, capture.0);
    if parameters.coverage <= 0.001 || parameters.density <= 0.001 {
        *clouds.2 = Visibility::Hidden;
        return;
    }
    let seed = cloud_seed(environment);
    let wind_angle = ((seed >> 32) as f32 / u32::MAX as f32) * core::f32::consts::TAU;
    let wind_speed = 2.0 + f32::from(environment.weather.wind_speed_bps) / 10_000.0 * 16.0;
    let elapsed =
        time.elapsed_secs() + (environment.absolute_minute % (7 * MINUTES_PER_DAY)) as f32 * 60.0;
    let wind_offset = Vec2::from_angle(wind_angle) * wind_speed * elapsed;
    let daylight = smoothstep(-8.0, 8.0, celestial.sun_altitude_degrees);
    let scene_luminance = 0.08 + daylight * 1.35;

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
        48_000.0,
    );
    material.motion = Vec4::new(
        wind_offset.x,
        wind_offset.y,
        daylight,
        celestial.weather_transmission,
    );
    *clouds.2 = Visibility::Inherited;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(precipitation: Precipitation, intensity_bps: u16) -> SceneEnvironment {
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
                precipitation,
                intensity_bps,
                ground_moisture_bps: 0,
                snow_cover_bps: 0,
            },
            canopy_bps: 0,
            wetland_bps: 0,
            cultivation_bps: 0,
            water_bps: 0,
            hilly_bps: 0,
        }
    }

    #[test]
    fn precipitation_produces_dense_low_clouds() {
        let clear =
            CloudLayerParameters::from_environment(&environment(Precipitation::Clear, 0), None);
        let rain =
            CloudLayerParameters::from_environment(&environment(Precipitation::Rain, 8_000), None);
        assert!(rain.coverage > clear.coverage);
        assert!(rain.density > clear.density);
        assert_eq!(rain.profile, 3.0);
        assert!(rain.bottom_metres < clear.bottom_metres);
    }

    #[test]
    fn capture_profiles_cover_distinct_altitude_and_density_families() {
        let cumulus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cumulus);
        let cirrus = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Cirrus);
        let storm = CloudLayerParameters::capture(TacticalCloudCaptureProfile::Storm);
        assert!(cirrus.bottom_metres > cumulus.bottom_metres);
        assert!(cirrus.thickness_metres < cumulus.thickness_metres);
        assert!(storm.density > cumulus.density);
        assert!(storm.coverage > cumulus.coverage);
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
}
