//! Bounded procedural cloud shells for the grounded tactical camera.

use super::*;
use bevy::{
    camera::{ClearColorConfig, RenderTarget, visibility::RenderLayers},
    render::render_resource::{TextureDescriptor, TextureUsages},
};

const CLOUD_SHADER: &str = "shaders/tactical_clouds.wgsl";
const CLOUD_COMPOSITE_SHADER: &str = "shaders/tactical_cloud_composite.wgsl";
/// Render layer reserved for the offscreen volumetric cloud pass.
const CLOUD_OFFSCREEN_LAYER: usize = 2;
const CLOUD_DOME_DISTANCE_METRES: f32 = 20_000.0;
/// Deliberately smaller than Earth's radius so cloud decks bend into the
/// tactical horizon within the renderer's bounded trace distance.
const CLOUD_CURVATURE_RADIUS_METRES: f32 = 180_000.0;
const CLOUD_AERIAL_EXTINCTION_PER_METRE: f32 = 0.000_025;

#[derive(Component)]
pub(crate) struct TacticalCloudLayer {
    slot: usize,
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
    /// Tiling 3D value-noise field (RGB = broad fbm, warp A, warp B), baked
    /// once at startup. At reduced march quality the shader samples this
    /// with hardware trilinear filtering instead of evaluating ~10 ALU
    /// noise octaves per density sample - the technique volumetric-cloud
    /// renderers like Horizon Zero Dawn (and bevy-volumetric-clouds) use.
    /// The full-quality reference march keeps the ALU path for goldens.
    #[texture(6, dimension = "3d")]
    #[sampler(7)]
    noise: Handle<Image>,
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

/// Marks the reduced-resolution camera that renders the volumetric shells
/// offscreen. Gameplay systems that assume one `Camera3d` must exclude it.
#[derive(Component)]
pub(crate) struct TacticalCloudOffscreenCamera;

/// Camera-following dome that samples the offscreen cloud target back into
/// the main view, so terrain and trees keep occluding clouds through the
/// ordinary depth test.
#[derive(Component)]
pub(in crate::presentation) struct TacticalCloudComposite;

#[derive(Resource)]
pub(in crate::presentation) struct TacticalCloudOffscreenTarget {
    image: Handle<Image>,
    resolution_scale: f32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalCloudCompositeMaterial {
    #[texture(0)]
    #[sampler(1)]
    source: Handle<Image>,
}

impl Material for TacticalCloudCompositeMaterial {
    fn fragment_shader() -> ShaderRef {
        CLOUD_COMPOSITE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        // The offscreen pass already blends the shells premultiplied over a
        // transparent clear, so one premultiplied composite is equivalent to
        // the legacy in-view shell blending.
        AlphaMode::Premultiplied
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The camera remains inside the composite dome.
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
    mut composite_materials: ResMut<Assets<TacticalCloudCompositeMaterial>>,
    mut images: ResMut<Assets<Image>>,
    settings: Res<TacticalGraphicsSettings>,
    cameras: Query<(Entity, &Projection, &Camera), With<Camera3d>>,
) {
    // The volumetric march can render at a reduced offscreen resolution and
    // composite through one dome; clouds are soft enough that the bilinear
    // upsample is close to invisible while the fragment cost drops with the
    // resolution squared. The legacy in-view path (1.0) stays authoritative
    // for capture tooling.
    let offscreen = if settings.cloud_resolution_scale < 0.999 {
        cameras
            .iter()
            .next()
            .map(|(camera, projection, main)| {
                (camera, projection.clone(), main.physical_target_size())
            })
    } else {
        None
    };
    let mesh = meshes.add(cloud_hemisphere_mesh());
    let noise = images.add(cloud_noise_image());
    for slot in 0..3 {
        let mut shell = commands.spawn((
            Name::new(format!("Procedural tactical cloud deck {slot}")),
            TacticalCloudLayer { slot },
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
                noise: noise.clone(),
            })),
            Transform::default(),
        ));
        if offscreen.is_some() {
            shell.insert(RenderLayers::layer(CLOUD_OFFSCREEN_LAYER));
        }
    }
    let Some((camera, projection, target_size)) = offscreen else {
        return;
    };
    let resolution_scale = settings.cloud_resolution_scale.clamp(0.25, 1.0);
    // The camera's computed target size is often not known yet during
    // startup; the per-frame update system re-sizes the image on the first
    // frame where it is.
    let size = cloud_offscreen_size(target_size.unwrap_or(UVec2::new(960, 540)), resolution_scale);
    let image = images.add(cloud_offscreen_image(size));
    commands.insert_resource(TacticalCloudOffscreenTarget {
        image: image.clone(),
        resolution_scale,
    });
    commands.spawn((
        Name::new("Tactical cloud composite dome"),
        TacticalCloudComposite,
        NoFrustumCulling,
        NotShadowCaster,
        Mesh3d(mesh),
        MeshMaterial3d(composite_materials.add(TacticalCloudCompositeMaterial {
            source: image.clone(),
        })),
        Transform::default(),
    ));
    commands.entity(camera).with_children(|children| {
        children.spawn((
            Name::new("Tactical cloud offscreen camera"),
            TacticalCloudOffscreenCamera,
            Camera3d::default(),
            Camera {
                // Render before the main camera consumes the target.
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image.into()),
            projection,
            // The main pass tonemaps the composited result exactly once,
            // like the legacy in-view shells.
            Tonemapping::None,
            Msaa::Off,
            RenderLayers::layer(CLOUD_OFFSCREEN_LAYER),
            Transform::IDENTITY,
        ));
    });
}

/// Side length of the tiling cloud-noise volume.
const CLOUD_NOISE_TEXELS: usize = 96;
/// Noise-domain units spanned by one texture period. Kept in sync with
/// `CLOUD_NOISE_PERIOD` in `tactical_clouds.wgsl`.
const CLOUD_NOISE_PERIOD: f32 = 8.0;

/// Deterministically bakes the tiling value-noise volume the reduced-quality
/// cloud march samples instead of evaluating noise octaves in ALU.
fn cloud_noise_image() -> Image {
    let broad8 = cloud_noise_lattice(8, 0x636c_6f75);
    let broad16 = cloud_noise_lattice(16, 0x6e6f_6973);
    let broad32 = cloud_noise_lattice(32, 0x6265_7673);
    let warp_a = cloud_noise_lattice(8, 0x7761_7270);
    let warp_b = cloud_noise_lattice(8, 0x6472_6966);

    let n = CLOUD_NOISE_TEXELS;
    let texel_units = CLOUD_NOISE_PERIOD / n as f32;
    let mut data = vec![0u8; n * n * n * 4];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let p = Vec3::new(x as f32, y as f32, z as f32) * texel_units;
                // Three wrapped octaves stand in for the shader's four
                // unwrapped ones; the dropped highest octave is below the
                // texel Nyquist limit anyway, and the weights are
                // renormalised to preserve overall amplitude.
                let broad = 0.559 * cloud_lattice_noise(&broad8, 8, p)
                    + 0.290 * cloud_lattice_noise(&broad16, 16, p * 2.0)
                    + 0.151 * cloud_lattice_noise(&broad32, 32, p * 4.0);
                let index = ((z * n + y) * n + x) * 4;
                data[index] = (broad.clamp(0.0, 1.0) * 255.0) as u8;
                data[index + 1] =
                    (cloud_lattice_noise(&warp_a, 8, p).clamp(0.0, 1.0) * 255.0) as u8;
                data[index + 2] =
                    (cloud_lattice_noise(&warp_b, 8, p).clamp(0.0, 1.0) * 255.0) as u8;
                data[index + 3] = 255;
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: n as u32,
            height: n as u32,
            depth_or_array_layers: n as u32,
        },
        TextureDimension::D3,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        label: Some("tactical_cloud_noise_sampler".to_owned()),
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        address_mode_w: bevy::image::ImageAddressMode::Repeat,
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A wrapped cubic lattice of deterministic unit hashes.
fn cloud_noise_lattice(period: usize, salt: u64) -> Vec<f32> {
    let mut values = vec![0.0f32; period * period * period];
    for z in 0..period {
        for y in 0..period {
            for x in 0..period {
                let key = (x as u64)
                    | ((y as u64) << 16)
                    | ((z as u64) << 32)
                    | (salt << 48 ^ salt);
                values[(z * period + y) * period + x] =
                    (splitmix64(key) >> 40) as f32 / 16_777_216.0;
            }
        }
    }
    values
}

/// Smooth trilinear value noise over a wrapped lattice, matching the blend
/// curve of `value_noise_3d` in the cloud shader.
fn cloud_lattice_noise(values: &[f32], period: usize, position: Vec3) -> f32 {
    let cell = position.floor();
    let local = position - cell;
    let blend = local * local * (Vec3::splat(3.0) - 2.0 * local);
    let corner = |dx: i32, dy: i32, dz: i32| -> f32 {
        let wrap = |v: f32, offset: i32| {
            ((v as i32 + offset).rem_euclid(period as i32)) as usize
        };
        values[(wrap(cell.z, dz) * period + wrap(cell.y, dy)) * period + wrap(cell.x, dx)]
    };
    let z0 = (corner(0, 0, 0) * (1.0 - blend.x) + corner(1, 0, 0) * blend.x)
        * (1.0 - blend.y)
        + (corner(0, 1, 0) * (1.0 - blend.x) + corner(1, 1, 0) * blend.x) * blend.y;
    let z1 = (corner(0, 0, 1) * (1.0 - blend.x) + corner(1, 0, 1) * blend.x)
        * (1.0 - blend.y)
        + (corner(0, 1, 1) * (1.0 - blend.x) + corner(1, 1, 1) * blend.x) * blend.y;
    z0 * (1.0 - blend.z) + z1 * blend.z
}

fn cloud_offscreen_size(target_size: UVec2, resolution_scale: f32) -> Extent3d {
    Extent3d {
        width: ((target_size.x as f32 * resolution_scale) as u32).max(1),
        height: ((target_size.y as f32 * resolution_scale) as u32).max(1),
        depth_or_array_layers: 1,
    }
}

fn cloud_offscreen_image(size: Extent3d) -> Image {
    let mut image = Image::default();
    image.texture_descriptor = TextureDescriptor {
        label: Some("tactical_cloud_offscreen_target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        // Preserves the scene-referred radiance range until the composite,
        // matching what the shells previously wrote into the main pass.
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    };
    image.resize(size);
    image
}

/// Keeps the offscreen target matched to the window and the offscreen
/// camera's projection matched to the gameplay camera.
pub(in crate::presentation) fn update_tactical_cloud_offscreen_target(
    target: Option<Res<TacticalCloudOffscreenTarget>>,
    mut images: ResMut<Assets<Image>>,
    main_camera: Query<&Camera, (With<Camera3d>, Without<TacticalCloudOffscreenCamera>)>,
    main_projection: Query<
        &Projection,
        (
            With<Camera3d>,
            Without<TacticalCloudOffscreenCamera>,
            Changed<Projection>,
        ),
    >,
    mut offscreen_projection: Query<&mut Projection, With<TacticalCloudOffscreenCamera>>,
) {
    let Some(target) = target else {
        return;
    };
    if let Some(target_size) = main_camera
        .iter()
        .next()
        .and_then(Camera::physical_target_size)
    {
        let desired = cloud_offscreen_size(target_size, target.resolution_scale);
        let stale = images
            .get(&target.image)
            .is_some_and(|image| image.texture_descriptor.size != desired);
        if stale && let Some(mut image) = images.get_mut(&target.image) {
            image.resize(desired);
        }
    }
    if let (Some(main), Some(mut offscreen)) = (
        main_projection.iter().next(),
        offscreen_projection.iter_mut().next(),
    ) {
        *offscreen = main.clone();
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
    settings: Res<super::TacticalGraphicsSettings>,
    camera: Single<&GlobalTransform, (With<Camera3d>, Without<TacticalCloudOffscreenCamera>)>,
    mut clouds: Query<(
        &TacticalCloudLayer,
        &MeshMaterial3d<TacticalCloudMaterial>,
        &mut Transform,
        &mut Visibility,
    )>,
    mut composites: Query<
        &mut Transform,
        (With<TacticalCloudComposite>, Without<TacticalCloudLayer>),
    >,
    mut materials: ResMut<Assets<TacticalCloudMaterial>>,
) {
    for mut transform in &mut composites {
        transform.translation = camera.translation();
    }
    let Some(environment) = active
        .entity
        .and_then(|entity| environments.get(entity).ok())
    else {
        for (_, _, _, mut visibility) in &mut clouds {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let Some(celestial) = celestial.snapshot.as_ref() else {
        for (_, _, _, mut visibility) in &mut clouds {
            *visibility = Visibility::Hidden;
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

    for (cloud, handle, mut transform, mut visibility) in &mut clouds {
        transform.translation = camera.translation();
        let Some(mut parameters) = layers[cloud.slot] else {
            *visibility = Visibility::Hidden;
            continue;
        };
        if parameters.coverage <= 0.001 || parameters.density <= 0.001 {
            *visibility = Visibility::Hidden;
            continue;
        }
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
        material.spectral = solar_color.extend(settings.cloud_quality_scale.clamp(0.35, 1.0));
        material.geometry = cloud_shell_geometry();
        *visibility = Visibility::Inherited;
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
