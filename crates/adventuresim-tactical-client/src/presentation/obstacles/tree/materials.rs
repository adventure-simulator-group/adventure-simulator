use bevy::{
    pbr::Material,
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

use super::geometry::{
    BLACKTHORN_PARAMETERS, COMMON_BEECH_PARAMETERS, COMMON_HAWTHORN_PARAMETERS,
    COMMON_HAZEL_PARAMETERS, ENGLISH_OAK_PARAMETERS,
};
#[cfg(test)]
use crate::presentation::generate_procedural_environment_assets;
use crate::presentation::{LeafTextureSet, ProceduralEnvironmentAssets};

const TREE_IMPOSTOR_SHADER: &str = "shaders/tactical_tree_impostor.wgsl";
const TREE_LEAF_CARD_SHADER: &str = "shaders/tactical_tree_leaf_card.wgsl";

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
    #[texture(10)]
    #[sampler(11)]
    pub(crate) arm: Handle<Image>,
    /// Wind direction XZ, strength, and CPU-synchronized phase time.
    #[uniform(12)]
    pub(crate) parameters: Vec4,
    /// Opacity cutoff, tangent-space normal strength, canopy AO strength, and
    /// diffuse transmission for the species' leaf thickness.
    #[uniform(12)]
    pub(crate) surface_parameters: Vec4,
    /// Perceptual roughness, physical thickness in metres, and reserved.
    #[uniform(12)]
    pub(crate) physical_parameters: Vec4,
}

const OAK_LEAF_DIFFUSE_TRANSMISSION: f32 = 0.46;
/// Representative alpha-weighted oak pigment for software-baked impostors.
///
/// The live material samples distinct front/back procedural palettes. The
/// single-color impostor bake uses this bounded midpoint so the far crown
/// remains continuous with the generated material.
pub(super) const OAK_LEAF_IMPOSTOR_BASE_SRGB: [f32; 3] = [96.0, 113.0, 76.0];

pub(crate) fn oak_leaf_material(
    assets: &ProceduralEnvironmentAssets,
) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        &assets.oak_leaf,
        0.28,
        0.72,
        canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres),
        OAK_LEAF_DIFFUSE_TRANSMISSION,
    )
}

pub(crate) fn oak_bark_material(_assets: &ProceduralEnvironmentAssets) -> StandardMaterial {
    StandardMaterial {
        // The molded bark colour is uniform. Structural relief comes from the
        // unified trunk/root mesh, so no UV-dependent channel can reveal the
        // branch-influence handoff across the implicit flare.
        base_color: Color::srgb_u8(70, 50, 30),
        perceptual_roughness: 241.0 / 255.0,
        metallic: 0.0,
        ..default()
    }
}

pub(in crate::presentation) fn hazel_leaf_material(
    assets: &ProceduralEnvironmentAssets,
) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        &assets.hazel_leaf,
        0.32,
        0.68,
        canopy_ao_strength(COMMON_HAZEL_PARAMETERS.crown_radius_metres),
        0.46,
    )
}

pub(in crate::presentation) fn blackthorn_leaf_material(
    assets: &ProceduralEnvironmentAssets,
) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        &assets.blackthorn_leaf,
        0.34,
        0.7,
        canopy_ao_strength(BLACKTHORN_PARAMETERS.crown_radius_metres),
        0.42,
    )
}

pub(in crate::presentation) fn hawthorn_leaf_material(
    assets: &ProceduralEnvironmentAssets,
) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        &assets.hawthorn_leaf,
        0.31,
        0.7,
        canopy_ao_strength(COMMON_HAWTHORN_PARAMETERS.crown_radius_metres),
        0.44,
    )
}

pub(in crate::presentation) fn beech_leaf_material(
    assets: &ProceduralEnvironmentAssets,
) -> TacticalTreeLeafCardMaterial {
    leaf_material(
        &assets.beech_leaf,
        0.3,
        0.68,
        canopy_ao_strength(COMMON_BEECH_PARAMETERS.crown_radius_metres),
        0.43,
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
    const UNRESOLVED_FOLIAGE_EXTINCTION_PER_METRE: f32 = 0.078;
    1.0 - (-UNRESOLVED_FOLIAGE_EXTINCTION_PER_METRE * crown_radius_metres.max(0.0)).exp()
}

pub(in crate::presentation) fn leaf_material(
    textures: &LeafTextureSet,
    alpha_cutoff: f32,
    normal_strength: f32,
    canopy_ao: f32,
    diffuse_transmission: f32,
) -> TacticalTreeLeafCardMaterial {
    TacticalTreeLeafCardMaterial {
        opacity: textures.opacity.clone(),
        front_albedo: textures.front_albedo.clone(),
        back_albedo: textures.back_albedo.clone(),
        front_normal: textures.front_normal.clone(),
        back_normal: textures.back_normal.clone(),
        arm: textures.arm.clone(),
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
        // Preserve the procedural cutout, then let 4x MSAA turn its remaining
        // fractional opacity into sample coverage instead of a jagged binary
        // silhouette. This is hardware multisampling and works on WebGPU.
        AlphaMode::AlphaToCoverage
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
        let assets = generate_procedural_environment_assets(
            &mut app.world_mut().resource_mut::<Assets<Image>>(),
        );
        let bark = oak_bark_material(&assets);

        assert_eq!(bark.base_color, Color::srgb_u8(70, 50, 30));
        assert!(bark.base_color_texture.is_none());
        assert!(bark.normal_map_texture.is_none());
        assert!(bark.metallic_roughness_texture.is_none());
        assert!(bark.occlusion_texture.is_none());
        assert_eq!(bark.metallic, 0.0);
        assert_eq!(bark.perceptual_roughness, 241.0 / 255.0);
    }

    #[test]
    fn canopy_ao_tracks_crown_scale_without_double_counting_resolved_leaves() {
        let clear = canopy_ao_strength(0.0);
        let oak = canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres);
        let hazel = canopy_ao_strength(COMMON_HAZEL_PARAMETERS.crown_radius_metres);
        let deep_crown = canopy_ao_strength(12.0);

        assert_eq!(clear, 0.0);
        assert!((oak - 0.37).abs() < 0.01);
        assert!((hazel - 0.12).abs() < 0.01);
        assert!(hazel < oak * 0.36);
        assert!(clear < hazel && hazel < oak && oak < deep_crown);
        assert!((0.0..1.0).contains(&deep_crown));
    }

    #[test]
    fn oak_leaf_optics_preserve_bounded_transmission_and_occlusion() {
        let oak_occlusion = canopy_ao_strength(ENGLISH_OAK_PARAMETERS.crown_radius_metres);

        assert!((oak_occlusion - 0.37).abs() < 0.01);
        assert!((OAK_LEAF_DIFFUSE_TRANSMISSION - 0.46).abs() < f32::EPSILON);
        assert!(oak_occlusion < OAK_LEAF_DIFFUSE_TRANSMISSION);
        assert!(OAK_LEAF_DIFFUSE_TRANSMISSION < 0.5);

        let darkest_authored_visibility = 1.0 + oak_occlusion * (0.32 - 1.0);
        assert!((0.73..=0.75).contains(&darkest_authored_visibility));
    }

    #[test]
    fn leaf_cutouts_use_hardware_multisample_coverage_and_lod_dither() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Image>();
        let assets = generate_procedural_environment_assets(
            &mut app.world_mut().resource_mut::<Assets<Image>>(),
        );
        assert_eq!(
            oak_leaf_material(&assets).alpha_mode(),
            AlphaMode::AlphaToCoverage
        );
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_tree_leaf_card.wgsl"
        ));
        assert!(shader.contains("visibility_range_dither(in.position"));
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
