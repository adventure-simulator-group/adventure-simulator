use bevy::{
    pbr::{ExtendedMaterial, Material, MaterialExtension},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

use super::geometry::{COMMON_HAZEL_PARAMETERS, ENGLISH_OAK_PARAMETERS};

const TREE_IMPOSTOR_SHADER: &str = "shaders/tactical_tree_impostor.wgsl";
const TREE_LEAF_CARD_SHADER: &str = "shaders/tactical_tree_leaf_card.wgsl";
const TREE_BARK_SHADER: &str = "shaders/tactical_tree_bark.wgsl";

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalTreeBarkExtension {
    #[texture(100)]
    #[sampler(101)]
    diffuse: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    normal_gl: Handle<Image>,
    #[texture(104)]
    #[sampler(105)]
    arm: Handle<Image>,
    /// Horizontal tiles/metre, vertical tiles/metre, normal strength, blend sharpness.
    #[uniform(106)]
    projection: Vec4,
}

impl MaterialExtension for TacticalTreeBarkExtension {
    fn fragment_shader() -> ShaderRef {
        TREE_BARK_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TREE_BARK_SHADER.into()
    }
}

pub(in crate::presentation) type TacticalTreeBarkMaterial =
    ExtendedMaterial<StandardMaterial, TacticalTreeBarkExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(crate) struct TacticalTreeLeafCardMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub(crate) opacity: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub(crate) front_albedo: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    pub(crate) back_albedo: Handle<Image>,
    #[texture(6)]
    #[sampler(7)]
    pub(crate) front_normal: Handle<Image>,
    #[texture(8)]
    #[sampler(9)]
    pub(crate) back_normal: Handle<Image>,
    /// Wind direction XZ, strength, and CPU-synchronized phase time.
    #[uniform(10)]
    pub(crate) parameters: Vec4,
    /// Opacity cutoff, tangent-space normal strength, canopy AO strength, and
    /// diffuse transmission for the species' leaf thickness.
    #[uniform(10)]
    pub(crate) surface_parameters: Vec4,
    /// Perceptual roughness, physical thickness in metres, and reserved.
    #[uniform(10)]
    pub(crate) physical_parameters: Vec4,
}

const OAK_LEAF_DIFFUSE_TRANSMISSION: f32 = 0.40;
/// Representative alpha-weighted oak pigment for software-baked impostors.
///
/// The live material samples distinct front/back scans. The single-color
/// impostor bake cannot retain those textures, so this bounded midpoint keeps
/// its hue near the scan instead of using the former saturated lime surrogate.
pub(super) const OAK_LEAF_IMPOSTOR_BASE_SRGB: [f32; 3] = [96.0, 113.0, 76.0];

pub(crate) fn oak_leaf_material(asset_server: &AssetServer) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        asset_server,
        "trees/oak_leaf_03",
        0.28,
        0.72,
        canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres),
        OAK_LEAF_DIFFUSE_TRANSMISSION,
    )
}

pub(crate) fn oak_bark_material(asset_server: &AssetServer) -> StandardMaterial {
    let arm = bark_image(
        asset_server,
        "textures/trees/oak_bark/jolcham_oak_bark_01_arm_1k.jpg",
        false,
    );
    StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(bark_image(
            asset_server,
            "textures/trees/oak_bark/jolcham_oak_bark_01_diff_1k.jpg",
            true,
        )),
        normal_map_texture: Some(bark_image(
            asset_server,
            "textures/trees/oak_bark/jolcham_oak_bark_01_nor_gl_1k.jpg",
            false,
        )),
        metallic_roughness_texture: Some(arm.clone()),
        occlusion_texture: Some(arm),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        // Poly Haven distributes this channel in OpenGL/right-handed form.
        flip_normal_map_y: false,
        ..default()
    }
}

pub(in crate::presentation) fn oak_hero_bark_material(
    asset_server: &AssetServer,
) -> TacticalTreeBarkMaterial {
    TacticalTreeBarkMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalTreeBarkExtension {
            diffuse: bark_image(
                asset_server,
                "textures/trees/oak_bark/jolcham_oak_bark_01_diff_1k.jpg",
                true,
            ),
            normal_gl: bark_image(
                asset_server,
                "textures/trees/oak_bark/jolcham_oak_bark_01_nor_gl_1k.jpg",
                false,
            ),
            arm: bark_image(
                asset_server,
                "textures/trees/oak_bark/jolcham_oak_bark_01_arm_1k.jpg",
                false,
            ),
            projection: Vec4::new(1.0, 0.5, 0.58, 4.0),
        },
    }
}

fn bark_image(asset_server: &AssetServer, path: &'static str, is_srgb: bool) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(move |settings: &mut bevy::image::ImageLoaderSettings| {
            use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
            settings.is_srgb = is_srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                address_mode_w: ImageAddressMode::Repeat,
                anisotropy_clamp: 8,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(path)
}

pub(in crate::presentation) fn hazel_leaf_material(
    asset_server: &AssetServer,
) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        asset_server,
        "shrubs/common_hazel_leaf",
        0.32,
        0.68,
        canopy_ao_strength(COMMON_HAZEL_PARAMETERS.crown_radius_metres),
        0.46,
    )
}

/// Approximates unresolved canopy occlusion as transmission through foliage.
///
/// The prior species constants gave a three-metre-wide hazel almost the same
/// occlusion as a twelve-metre-wide oak. Beer-Lambert transmission makes the
/// effect depend on the representative path length through each crown. The
/// extinction is deliberately bounded below dense-forest values because the
/// explicit leaf cards and screen-space AO already resolve part of the crown's
/// self-occlusion.
pub(super) fn canopy_ao_strength(crown_radius_metres: f32) -> f32 {
    // This is an empirical unresolved-path coefficient calibrated under the
    // production atmosphere IBL, not a measured whole-leaf absorption value.
    const UNRESOLVED_FOLIAGE_EXTINCTION_PER_METRE: f32 = 0.11;
    1.0 - (-UNRESOLVED_FOLIAGE_EXTINCTION_PER_METRE * crown_radius_metres.max(0.0)).exp()
}

fn leaf_material(
    asset_server: &AssetServer,
    stem: &str,
    alpha_cutoff: f32,
    normal_strength: f32,
    canopy_ao: f32,
    diffuse_transmission: f32,
) -> TacticalTreeLeafCardMaterial {
    let linear_image = |path| {
        asset_server
            .load_builder()
            .with_settings(|settings: &mut bevy::image::ImageLoaderSettings| {
                settings.is_srgb = false
            })
            .load(path)
    };
    TacticalTreeLeafCardMaterial {
        opacity: linear_image(format!("textures/{stem}_opacity.png")),
        front_albedo: asset_server.load(format!("textures/{stem}_front_albedo.png")),
        back_albedo: asset_server.load(format!("textures/{stem}_back_albedo.png")),
        front_normal: linear_image(format!("textures/{stem}_front_normal_dx.png")),
        back_normal: linear_image(format!("textures/{stem}_back_normal_dx.png")),
        parameters: Vec4::new(0.74, 0.67, 0.035, 0.0),
        surface_parameters: Vec4::new(
            alpha_cutoff,
            normal_strength,
            canopy_ao,
            diffuse_transmission,
        ),
        physical_parameters: Vec4::new(0.86, 0.001, 0.0, 0.0),
    }
}

pub(in crate::presentation) fn update_tree_leaf_wind(
    time: Res<Time>,
    mut materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
) {
    let phase_time = time.elapsed_secs() * 1.15;
    for (_, material) in materials.iter_mut() {
        material.parameters.w = phase_time;
    }
}

impl Material for TacticalTreeLeafCardMaterial {
    fn vertex_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(self.surface_parameters.x)
    }

    fn enable_prepass() -> bool {
        true
    }

    fn enable_shadows() -> bool {
        true
    }

    fn prepass_vertex_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        TREE_LEAF_CARD_SHADER.into()
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(fragment) = descriptor.fragment.as_mut() {
            enable_leaf_transmission_shader_defs(&mut fragment.shader_defs);
        }
        Ok(())
    }
}

fn enable_leaf_transmission_shader_defs(shader_defs: &mut Vec<bevy::shader::ShaderDefVal>) {
    for name in [
        "STANDARD_MATERIAL_DIFFUSE_TRANSMISSION",
        "STANDARD_MATERIAL_DIFFUSE_OR_SPECULAR_TRANSMISSION",
    ] {
        let shader_def = bevy::shader::ShaderDefVal::from(name);
        if !shader_defs.contains(&shader_def) {
            shader_defs.push(shader_def);
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalTreeImpostorMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub(super) baked_color: Handle<Image>,
    /// Representation level, deterministic seed, wind strength, wind speed.
    #[uniform(2)]
    pub(super) parameters: Vec4,
    /// Direction toward the dominant celestial light and day/night strength.
    #[uniform(2)]
    pub(in crate::presentation) lighting: Vec4,
    /// Ambient irradiance colour and normalized strength.
    #[uniform(2)]
    pub(in crate::presentation) ambient: Vec4,
}

impl Material for TacticalTreeImpostorMaterial {
    fn vertex_shader() -> ShaderRef {
        TREE_IMPOSTOR_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        TREE_IMPOSTOR_SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oak_bark_material_uses_matched_dielectric_pbr_channels() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Image>();
        let asset_server = app.world().resource::<AssetServer>();
        let bark = oak_bark_material(asset_server);

        assert_eq!(bark.base_color, Color::WHITE);
        assert!(bark.base_color_texture.is_some());
        assert!(bark.normal_map_texture.is_some());
        assert_eq!(bark.metallic_roughness_texture, bark.occlusion_texture);
        assert_eq!(bark.metallic, 0.0);
        assert_eq!(bark.perceptual_roughness, 1.0);
        assert!(!bark.flip_normal_map_y);
    }

    #[test]
    fn hero_bark_uses_bounded_triplanar_pbr_only_for_close_wood() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_tree_bark.wgsl"
        ));
        assert_eq!(shader.matches("textureSample(").count(), 9);
        assert!(shader.contains("projection_weights(macro_normal)"));
        assert!(shader.contains("pbr_input.world_normal = composed_normal"));
        assert!(shader.contains("pbr_input.material.perceptual_roughness"));
        assert!(!shader.contains("discard;"));
    }

    #[test]
    fn canopy_ao_tracks_crown_scale_without_double_counting_resolved_leaves() {
        let clear = canopy_ao_strength(0.0);
        let oak = canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres);
        let hazel = canopy_ao_strength(COMMON_HAZEL_PARAMETERS.crown_radius_metres);
        let deep_crown = canopy_ao_strength(12.0);

        assert_eq!(clear, 0.0);
        assert!((oak - 0.48).abs() < 0.01);
        assert!((hazel - 0.16).abs() < 0.01);
        assert!(hazel < oak * 0.35);
        assert!(clear < hazel && hazel < oak && oak < deep_crown);
        assert!((0.0..1.0).contains(&deep_crown));
    }

    #[test]
    fn oak_leaf_optics_preserve_bounded_transmission_and_occlusion() {
        let oak_occlusion = canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres);

        assert!((oak_occlusion - 0.48).abs() < 0.01);
        assert!((OAK_LEAF_DIFFUSE_TRANSMISSION - 0.40).abs() < f32::EPSILON);
        assert!(OAK_LEAF_DIFFUSE_TRANSMISSION < 0.5);
        assert!(oak_occlusion > OAK_LEAF_DIFFUSE_TRANSMISSION);

        let darkest_authored_visibility = 1.0 + oak_occlusion * (0.32 - 1.0);
        assert!((0.66..=0.68).contains(&darkest_authored_visibility));
    }

    #[test]
    fn leaf_pipeline_enables_exact_diffuse_transmission_definitions() {
        let mut shader_defs = vec![bevy::shader::ShaderDefVal::from("EXISTING")];
        enable_leaf_transmission_shader_defs(&mut shader_defs);
        enable_leaf_transmission_shader_defs(&mut shader_defs);

        for expected in [
            "STANDARD_MATERIAL_DIFFUSE_TRANSMISSION",
            "STANDARD_MATERIAL_DIFFUSE_OR_SPECULAR_TRANSMISSION",
        ] {
            assert_eq!(
                shader_defs
                    .iter()
                    .filter(|shader_def| **shader_def == bevy::shader::ShaderDefVal::from(expected))
                    .count(),
                1
            );
        }
        assert_eq!(shader_defs.len(), 3);
    }
}
