//! Converts compact geological recipes into the terrain extension's cliff uniforms.

use super::*;
use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;

const TERRAIN_SHADER: &str = "shaders/tactical_terrain.wgsl";

/// Flat Bevy binding shared by ordinary terrain and the implicit cliff patch.
/// Cliff uniforms and textures live here beside the pre-existing terrain
/// fields because one material extension owns binding group 100.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(in crate::presentation) struct TacticalTerrainExtension {
    #[uniform(100)]
    pub(super) base_color: Vec4,
    #[uniform(100)]
    pub(super) grass_color: Vec4,
    #[uniform(100)]
    pub(super) cover: Vec4,
    #[uniform(100)]
    pub(super) weather: Vec4,
    #[uniform(100)]
    pub(super) far_sward: Vec4,
    #[uniform(100)]
    pub(super) lod_sward: Vec4,
    #[uniform(100)]
    pub(super) playable_bounds: Vec4,
    #[uniform(100)]
    pub(super) detail_patch: Vec4,
    #[uniform(100)]
    pub(super) soil_detail: Vec4,
    #[uniform(100)]
    pub(super) litter_detail: Vec4,
    #[uniform(100)]
    pub(super) cliff_palette_a: Vec4,
    #[uniform(100)]
    pub(super) cliff_palette_b: Vec4,
    #[uniform(100)]
    pub(super) cliff_surface: Vec4,
    #[uniform(100)]
    pub(super) cliff_structure_a: Vec4,
    #[uniform(100)]
    pub(super) cliff_structure_b: Vec4,
    #[texture(101)]
    #[sampler(102)]
    pub(super) ground_map: Handle<Image>,
    #[texture(103)]
    #[sampler(104)]
    pub(super) soil_height_ao: Handle<Image>,
    #[texture(105)]
    #[sampler(106)]
    pub(super) litter_surface: Handle<Image>,
    #[texture(107)]
    #[sampler(108)]
    pub(super) litter_normal: Handle<Image>,
    #[texture(109)]
    #[sampler(110)]
    pub(super) blood_mask: Handle<Image>,
    #[texture(111)]
    #[sampler(112)]
    pub(super) cliff_height: Handle<Image>,
    #[texture(113)]
    #[sampler(114)]
    pub(super) cliff_arm: Handle<Image>,
}

impl MaterialExtension for TacticalTerrainExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

pub(in crate::presentation) type TacticalTerrainMaterial =
    ExtendedMaterial<StandardMaterial, TacticalTerrainExtension>;

pub(super) fn enable_cliff_surface(
    material: &mut TacticalTerrainMaterial,
    recipe: TerrainSurfaceRecipe,
) {
    let parameters = recipe.parameters();
    let palette = parameters.palette_srgb.map(|rgb| {
        Color::srgb_u8(rgb[0], rgb[1], rgb[2])
            .to_linear()
            .to_f32_array()
    });
    material.extension.cliff_palette_a =
        Vec4::new(palette[0][0], palette[0][1], palette[0][2], 1.0);
    material.extension.cliff_palette_b =
        Vec4::new(palette[1][0], palette[1][1], palette[1][2], 0.0);
    material.extension.cliff_surface = Vec4::new(
        1.0 / parameters.grain_tile_metres,
        parameters.microrelief_metres,
        parameters.roughness[0],
        parameters.roughness[1],
    );
    let basis_points_per_whole = f32::from(BASIS_POINTS_PER_WHOLE);
    let (normal, mode, structure) = match recipe.structure {
        TerrainGeologicStructure::Massive => ([0.0, 1.0, 0.0], 0.0, Vec4::ZERO),
        TerrainGeologicStructure::Bedded {
            normal_permyriad,
            bed_thickness_cm,
            thickness_variation_bps,
            warp_cm,
            cross_bedding_bps,
        } => (
            normal_permyriad.map(|value| f32::from(value) / basis_points_per_whole),
            1.0,
            Vec4::new(
                f32::from(bed_thickness_cm) / 100.0,
                f32::from(thickness_variation_bps) / basis_points_per_whole,
                f32::from(warp_cm) / 100.0,
                f32::from(cross_bedding_bps) / basis_points_per_whole,
            ),
        ),
        TerrainGeologicStructure::Foliated {
            normal_permyriad,
            band_spacing_cm,
            warp_cm,
        } => (
            normal_permyriad.map(|value| f32::from(value) / basis_points_per_whole),
            2.0,
            Vec4::new(
                f32::from(band_spacing_cm) / 100.0,
                0.22,
                f32::from(warp_cm) / 100.0,
                0.0,
            ),
        ),
    };
    material.extension.cliff_structure_a = Vec4::new(normal[0], normal[1], normal[2], mode);
    material.extension.cliff_structure_b = structure;
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_world_schema::{IgneousRock, SedimentaryRock, SurfaceLithology};

    #[test]
    fn ordinary_terrain_disables_cliff_and_patch_recipe_enables_it() {
        let mut images = Assets::<Image>::default();
        let procedural_assets = generate_procedural_textures(&mut images);
        let terrain = SceneTerrain::new(8, 8, 1.0, |_| 0.0);
        let environment = SceneEnvironmentFixture::TemperateHills.snapshot("cliff-material");
        let graphics = TacticalGraphicsSettings::default();
        let mut material = terrain_material(
            &terrain,
            &environment,
            None,
            &procedural_assets,
            &mut images,
            &graphics.config.grass,
        );
        assert_eq!(material.extension.cliff_palette_a.w, 0.0);

        let sandstone = TerrainSurfaceRecipe::new(
            SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone),
            TerrainSurfaceSource::AuthoredFixture,
            47_115,
            [10_000, 0],
        );
        enable_cliff_surface(&mut material, sandstone);
        assert_eq!(material.extension.cliff_palette_a.w, 1.0);
        assert_eq!(material.extension.cliff_structure_a.w, 1.0);
        assert_ne!(
            material.extension.cliff_palette_a.xyz(),
            material.extension.cliff_palette_b.xyz()
        );

        let granite = TerrainSurfaceRecipe::new(
            SurfaceLithology::Igneous(IgneousRock::Granite),
            TerrainSurfaceSource::AuthoredFixture,
            47_117,
            [10_000, 0],
        );
        enable_cliff_surface(&mut material, granite);
        assert_eq!(material.extension.cliff_structure_a.w, 0.0);
    }
}
