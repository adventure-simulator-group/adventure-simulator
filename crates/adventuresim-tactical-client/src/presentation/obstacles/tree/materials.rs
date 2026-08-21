use adventuresim_tactical_core::prelude::SceneTerrain;
use bevy::{
    pbr::{ExtendedMaterial, Material, MaterialExtension},
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
use crate::presentation::procedural_assets::{
    FOREST_SOIL_HEIGHT_RANGE_METRES, FOREST_SOIL_TILE_METRES,
};
use crate::presentation::{
    LeafTextureSet, ProceduralEnvironmentAssets, color_vec4, terrain::TACTICAL_DIRT_SRGB,
};

const TREE_IMPOSTOR_SHADER: &str = "shaders/tactical_tree_impostor.wgsl";
const TREE_LEAF_CARD_SHADER: &str = "shaders/tactical_tree_leaf_card.wgsl";
const TREE_BARK_SHADER: &str = "shaders/tactical_tree_bark.wgsl";
const CANOPY_SHADED_SOIL_LINEAR_SCALE: Vec3 = Vec3::new(0.38, 0.40, 0.37);

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
    /// Perceptual roughness, physical thickness in metres, ground-litter
    /// vertex-pigment strength, and reserved.
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

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(crate) struct TacticalTreeBarkExtension {
    /// Canonical scalar relief: normalized height in R and horizon AO in G.
    #[texture(100)]
    #[sampler(101)]
    height_ao: Handle<Image>,
    /// Tiles/metre, physical height range, normal strength, and AO strength.
    #[uniform(102)]
    relief: Vec4,
    /// Triplanar exponent, branch alignment, parallax fraction, fade distance.
    #[uniform(102)]
    projection: Vec4,
    /// Direction toward dominant light and normalized directional strength.
    #[uniform(102)]
    pub(in crate::presentation) lighting: Vec4,
    /// Linear bark pigment and perceptual roughness.
    #[uniform(102)]
    surface: Vec4,
    /// Linear soil pigment and perceptual roughness. Soil remains a single
    /// molded albedo; only the binary coverage mask varies spatially.
    #[uniform(102)]
    soil_surface: Vec4,
    /// Solid soil height, maximum speck height, cell size, minimum radius.
    #[uniform(102)]
    deposition: Vec4,
    /// Playable half extents and encoded minimum/maximum terrain heights.
    #[uniform(102)]
    terrain_surface: Vec4,
    /// Soil tiles/metre, physical height range, normal strength, AO strength.
    #[uniform(102)]
    soil_response: Vec4,
    /// Soil dielectric reflectance; remaining components are reserved.
    #[uniform(102)]
    soil_optics: Vec4,
    /// Row-major playable terrain heightfield encoded into two channels.
    #[texture(103)]
    terrain_heightmap: Handle<Image>,
    /// The same packed height/AO surface sampled by tactical terrain.
    #[texture(104)]
    #[sampler(105)]
    soil_height_ao: Handle<Image>,
}

impl MaterialExtension for TacticalTreeBarkExtension {
    fn fragment_shader() -> ShaderRef {
        TREE_BARK_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TREE_BARK_SHADER.into()
    }
}

pub(crate) type TacticalTreeBarkMaterial =
    ExtendedMaterial<StandardMaterial, TacticalTreeBarkExtension>;

pub(crate) fn oak_bark_material(
    assets: &ProceduralEnvironmentAssets,
    terrain_heightmap: Handle<Image>,
    terrain_height_range: Vec2,
    terrain: &SceneTerrain,
) -> TacticalTreeBarkMaterial {
    bark_material(
        assets,
        terrain_heightmap,
        terrain_height_range,
        terrain,
        Color::srgb_u8(96, 68, 43),
        180.0 / 255.0,
        Vec4::new(2.0, 0.032, 1.30, 0.95),
    )
}

pub(in crate::presentation) fn beech_bark_material(
    assets: &ProceduralEnvironmentAssets,
    terrain_heightmap: Handle<Image>,
    terrain_height_range: Vec2,
    terrain: &SceneTerrain,
) -> TacticalTreeBarkMaterial {
    // Beech shares the pipeline so streamed wood remains one material type,
    // but its smooth bark deliberately bypasses oak relief and cavity AO.
    bark_material(
        assets,
        terrain_heightmap,
        terrain_height_range,
        terrain,
        Color::srgb_u8(145, 145, 135),
        0.9,
        Vec4::new(2.0, 0.0, 0.0, 0.0),
    )
}

fn bark_material(
    assets: &ProceduralEnvironmentAssets,
    terrain_heightmap: Handle<Image>,
    terrain_height_range: Vec2,
    terrain: &SceneTerrain,
    base_color: Color,
    perceptual_roughness: f32,
    relief: Vec4,
) -> TacticalTreeBarkMaterial {
    let dirt_color = Color::srgb_u8(
        TACTICAL_DIRT_SRGB[0],
        TACTICAL_DIRT_SRGB[1],
        TACTICAL_DIRT_SRGB[2],
    );
    // Match the soil deposited at the root contact to the shaded substrate
    // beneath canopy litter. The terrain shader applies this same linear
    // multiplier before layering individual litter pigments over the ground.
    let shaded_soil_color = color_vec4(dirt_color).xyz() * CANOPY_SHADED_SOIL_LINEAR_SCALE;
    TacticalTreeBarkMaterial {
        base: StandardMaterial {
            base_color,
            perceptual_roughness,
            metallic: 0.0,
            ..default()
        },
        extension: TacticalTreeBarkExtension {
            height_ao: assets.oak_bark.height_ao.clone(),
            relief,
            projection: Vec4::new(4.0, 0.92, 0.52, 12.0),
            lighting: Vec3::new(0.25, 0.92, 0.3).normalize().extend(1.0),
            surface: color_vec4(base_color).xyz().extend(perceptual_roughness),
            soil_surface: shaded_soil_color.extend(0.84),
            // The 7 mm minimum radius yields a 14 mm minimum full speck
            // diameter before edge antialiasing. A 45 mm cell leaves enough
            // negative space for the separate deposits to read clearly.
            deposition: Vec4::new(0.12, 0.46, 0.045, 0.007),
            terrain_surface: Vec4::new(
                terrain.width() * 0.5,
                terrain.depth() * 0.5,
                terrain_height_range.x,
                terrain_height_range.y,
            ),
            soil_response: Vec4::new(
                1.0 / FOREST_SOIL_TILE_METRES,
                FOREST_SOIL_HEIGHT_RANGE_METRES,
                1.0,
                0.82,
            ),
            soil_optics: Vec4::new(0.35, 0.0, 0.0, 0.0),
            terrain_heightmap,
            soil_height_ao: assets.forest_soil.height_ao.clone(),
        },
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

    fn alpha_mode(&self) -> AlphaMode {
        // Preserve smooth coverage only at the sparse atlas silhouette. The
        // shader handles LOD visibility as a crisp complementary handoff so
        // neither ordered stipple nor translucent duplicate crowns appear.
        AlphaMode::AlphaToCoverage
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
        let terrain = SceneTerrain::new(2, 2, 1.0, |point| point.x * 0.1 + point.y * 0.2);
        let heightmap = Handle::<Image>::default();
        let terrain_height_range = Vec2::new(-0.075, 0.705);
        let bark = oak_bark_material(&assets, heightmap.clone(), terrain_height_range, &terrain);

        assert_eq!(bark.base.base_color, Color::srgb_u8(96, 68, 43));
        assert!(bark.base.base_color_texture.is_none());
        assert!(bark.base.normal_map_texture.is_none());
        assert!(bark.base.metallic_roughness_texture.is_none());
        assert!(bark.base.occlusion_texture.is_none());
        assert_eq!(bark.base.metallic, 0.0);
        assert_eq!(bark.base.perceptual_roughness, 180.0 / 255.0);
        assert_eq!(bark.extension.relief, Vec4::new(2.0, 0.032, 1.30, 0.95));
        assert_eq!(bark.extension.projection, Vec4::new(4.0, 0.92, 0.52, 12.0));
        assert!(bark.extension.lighting.xyz().is_normalized());
        assert_eq!(
            bark.extension.surface,
            color_vec4(Color::srgb_u8(96, 68, 43))
                .xyz()
                .extend(180.0 / 255.0)
        );
        assert_eq!(
            bark.extension.soil_surface,
            (color_vec4(Color::srgb_u8(
                TACTICAL_DIRT_SRGB[0],
                TACTICAL_DIRT_SRGB[1],
                TACTICAL_DIRT_SRGB[2],
            ))
            .xyz()
                * CANOPY_SHADED_SOIL_LINEAR_SCALE)
                .extend(0.84)
        );
        assert_eq!(
            bark.extension.deposition,
            Vec4::new(0.12, 0.46, 0.045, 0.007)
        );
        assert!(bark.extension.deposition.w * 2.0 >= 0.01);
        assert_eq!(bark.extension.terrain_heightmap, heightmap);
        assert_eq!(
            bark.extension.terrain_surface,
            Vec4::new(1.0, 1.0, -0.075, 0.705)
        );
        assert_eq!(
            bark.extension.soil_response,
            Vec4::new(
                1.0 / FOREST_SOIL_TILE_METRES,
                FOREST_SOIL_HEIGHT_RANGE_METRES,
                1.0,
                0.82,
            )
        );
        assert_eq!(bark.extension.soil_height_ao, assets.forest_soil.height_ao);
        assert_eq!(bark.extension.soil_optics, Vec4::new(0.35, 0.0, 0.0, 0.0));
    }

    #[test]
    fn bark_shader_blends_shared_soil_response_at_the_sampled_terrain_contact() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_tree_bark.wgsl"
        ))
        .replace("\r\n", "\n");
        assert!(shader.contains("fn triplanar_height_ao"));
        assert!(shader.contains("let height_metres = (sample.r - 0.5)"));
        assert!(shader.contains("let height_dx = dpdx(height_metres)"));
        assert!(shader.contains("let height_dy = dpdy(height_metres)"));
        assert!(shader.contains("pbr_input.N = normalize(mix(composed_normal, soil_normal"));
        assert!(shader.contains("fn parallax_branch_coordinates"));
        assert!(shader.contains("branch_texture_coordinates(in.uv)"));
        assert!(shader.contains("view.lod_view_world_position"));
        assert!(shader.contains("if bark.relief.y <= 0.0001 || fade <= 0.001"));
        assert!(shader.contains("textureSampleGrad"));
        assert!(!shader.contains("textureSampleLevel"));
        assert!(shader.contains("fn directional_horizon_visibility"));
        assert!(shader.contains("layer < 6"));
        assert!(shader.contains("horizon_step <= 3"));
        assert!(shader.contains("let bark_roughness = clamp"));
        assert!(shader.contains("bark.soil_surface.w"));
        assert!(shader.contains("vec3<f32>(bark.soil_optics.x)"));
        assert!(shader.contains("fn root_soil_signed_distance"));
        assert!(shader.contains("fn soil_speck_distance"));
        assert!(shader.contains("let fine_specks = soil_speck_distance"));
        assert!(shader.contains("let coarse_specks = soil_speck_distance"));
        assert!(shader.contains("if root_height > bark.deposition.y + cell_size"));
        assert!(shader.contains("let root_plane_height = in.world_position.y - in.color.r"));
        assert!(shader.contains("let root_contact_ceiling = max("));
        assert!(shader.contains("bark.terrain_surface.w - root_plane_height"));
        assert!(shader.contains("if in.color.r <= root_contact_ceiling {"));
        assert!(shader.contains("let edge_width = max(fwidth(signed_distance)"));
        assert!(shader.contains("mix(bark.surface.rgb, bark.soil_surface.rgb, soil_coverage)"));
        assert!(shader.contains("let terrain_height = terrain_height_at(in.world_position.xz)"));
        assert!(shader.contains("let terrain_clearance = in.world_position.y - terrain_height"));
        assert!(shader.contains("var soil_response_coverage = 0.0"));
        assert!(shader.contains("smoothstep(0.0381, 0.0508"));
        assert!(shader.contains("soil_response_coverage = soil_coverage * contact_response"));
        assert!(shader.contains("soil_sample = soil_surface_sample(in.world_position.xyz)"));
        assert!(shader.contains("mix(composed_normal, soil_normal, soil_response_coverage)"));
        assert!(shader.contains("bark.soil_surface.w,\n        soil_coverage"));
        assert!(
            shader.contains("mix(ambient_visibility, soil_visibility, soil_response_coverage)")
        );
        assert!(!shader.contains("mix(ambient_visibility, soil_visibility, soil_coverage)"));
        assert!(shader.contains("#ifdef VERTEX_COLORS"));
        assert_eq!(shader.matches("textureSample(soil_height_ao,").count(), 1);
        assert!(!shader.contains("normal_map"));
    }

    #[test]
    fn bark_shader_limits_terrain_and_soil_sampling_to_the_conservative_root_band() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_tree_bark.wgsl"
        ))
        .replace("\r\n", "\n");
        let root_band_start = shader
            .find("if in.color.r <= root_contact_ceiling {")
            .expect("root-band guard");
        let root_band_end = shader[root_band_start..]
            .find("\n    }\n#endif")
            .map(|offset| root_band_start + offset)
            .expect("root-band guard closes before the vertex-colour block ends");
        let root_band = &shader[root_band_start..root_band_end];
        let upper_trunk = &shader[..root_band_start];

        assert!(root_band.contains("terrain_height_at(in.world_position.xz)"));
        assert!(root_band.contains("root_soil_signed_distance("));
        assert!(root_band.contains("soil_surface_sample(in.world_position.xyz)"));
        assert!(!upper_trunk.contains("terrain_height_at(in.world_position.xz)"));
        assert!(!upper_trunk.contains("root_soil_signed_distance(\n        in.world_position"));
        assert!(!upper_trunk.contains("soil_surface_sample(in.world_position.xyz)"));
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
    fn leaf_cutouts_use_hardware_multisample_coverage_without_screen_door_dither() {
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
        assert!(shader.contains("opacity * lod_coverage"));
        assert!(shader.contains("abs(f32(dither)) / 16.0"));
        assert!(shader.contains("dither <= -8 || dither > 8"));
        assert!(!shader.contains("visibility_range_dither(in.position"));
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
