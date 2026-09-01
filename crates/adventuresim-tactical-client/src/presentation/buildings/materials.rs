use adventuresim_building_generator::{BuildingLodMaterial, RoofMaterial, WallMaterialClass};
use adventuresim_procedural_textures::building::{
    BuildingSurfacePalette, FacadeFinish, brick_texture, checker_texture, facade_atlas,
    fachwerk_baked_texture,
};
use adventuresim_procedural_textures::{
    CRENELLATION_ALPHA_CUTOFF, ProceduralTextureAssets, WINDOW_GLASS_MATERIAL_CONTRACT,
};
use bevy::render::render_resource::Face;
use fabelgeist_determinism::splitmix64;

use super::super::*;

const APPEARANCE_DOMAIN: u64 = 0x6275_696c_645f_636f;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum BuildingAppearance {
    NaturalOak,
    WeatheredOak,
    OxideRed,
    RedBrownBrick,
    WeatheredGray,
    Ochre,
    RenderedCream,
    RenderedOchre,
}

impl BuildingAppearance {
    const ALL: [Self; 8] = [
        Self::NaturalOak,
        Self::WeatheredOak,
        Self::OxideRed,
        Self::RedBrownBrick,
        Self::WeatheredGray,
        Self::Ochre,
        Self::RenderedCream,
        Self::RenderedOchre,
    ];

    fn for_building(building_id: u64) -> Self {
        match splitmix64(building_id ^ APPEARANCE_DOMAIN) % 100 {
            0..=22 => Self::NaturalOak,
            23..=34 => Self::WeatheredOak,
            35..=49 => Self::OxideRed,
            50..=64 => Self::RedBrownBrick,
            65..=79 => Self::WeatheredGray,
            80..=89 => Self::Ochre,
            90..=94 => Self::RenderedCream,
            _ => Self::RenderedOchre,
        }
    }

    const fn spec(self) -> BuildingSurfacePalette {
        match self {
            Self::NaturalOak => BuildingSurfacePalette::new(
                FacadeFinish::PlasterInfill,
                [[214, 204, 177, 255], [192, 181, 153, 255]],
                [[84, 48, 27, 255], [57, 31, 19, 255]],
                [[112, 49, 34, 255], [77, 32, 25, 255]],
            ),
            Self::WeatheredOak => BuildingSurfacePalette::new(
                FacadeFinish::PlasterInfill,
                [[187, 184, 169, 255], [166, 163, 149, 255]],
                [[91, 72, 55, 255], [64, 50, 40, 255]],
                [[121, 63, 43, 255], [83, 43, 32, 255]],
            ),
            Self::OxideRed => BuildingSurfacePalette::new(
                FacadeFinish::PlasterInfill,
                [[207, 196, 164, 255], [186, 174, 143, 255]],
                [[111, 43, 29, 255], [75, 29, 22, 255]],
                [[102, 39, 29, 255], [66, 27, 24, 255]],
            ),
            Self::RedBrownBrick => BuildingSurfacePalette::new(
                FacadeFinish::BrickInfill,
                [[151, 75, 52, 255], [120, 55, 42, 255]],
                [[88, 35, 25, 255], [57, 24, 19, 255]],
                [[124, 55, 37, 255], [81, 33, 27, 255]],
            ),
            Self::WeatheredGray => BuildingSurfacePalette::new(
                FacadeFinish::PlasterInfill,
                [[196, 192, 174, 255], [174, 171, 155, 255]],
                [[79, 76, 70, 255], [53, 51, 48, 255]],
                [[103, 50, 38, 255], [68, 33, 29, 255]],
            ),
            Self::Ochre => BuildingSurfacePalette::new(
                FacadeFinish::PlasterInfill,
                [[202, 183, 132, 255], [180, 159, 111, 255]],
                [[130, 85, 32, 255], [91, 58, 25, 255]],
                [[114, 52, 34, 255], [76, 34, 27, 255]],
            ),
            Self::RenderedCream => BuildingSurfacePalette::new(
                FacadeFinish::FullyRendered,
                [[210, 201, 179, 255], [190, 180, 159, 255]],
                [[195, 183, 158, 255], [173, 162, 141, 255]],
                [[116, 59, 42, 255], [78, 39, 31, 255]],
            ),
            Self::RenderedOchre => BuildingSurfacePalette::new(
                FacadeFinish::FullyRendered,
                [[197, 173, 118, 255], [174, 150, 99, 255]],
                [[183, 155, 102, 255], [158, 132, 84, 255]],
                [[126, 64, 41, 255], [85, 41, 30, 255]],
            ),
        }
    }
}

struct AppearanceMaterials {
    finish: FacadeFinish,
    infill: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
    tile: Handle<StandardMaterial>,
    fachwerk_baked: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub(crate) struct TacticalBuildingMaterials {
    appearances: Vec<AppearanceMaterials>,
    brick: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    slate: Handle<StandardMaterial>,
    timber_roof: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    details: Handle<StandardMaterial>,
    crown_mask: Handle<StandardMaterial>,
}

impl TacticalBuildingMaterials {
    pub(crate) fn get_for_building(
        &self,
        building_id: u64,
        material: BuildingLodMaterial,
    ) -> Handle<StandardMaterial> {
        let appearance = BuildingAppearance::for_building(building_id);
        let palette = &self.appearances[appearance as usize];
        match material {
            BuildingLodMaterial::Wall(WallMaterialClass::TimberInfill) => palette.infill.clone(),
            BuildingLodMaterial::Wall(WallMaterialClass::CivilianMasonry)
                if palette.finish == FacadeFinish::FullyRendered =>
            {
                palette.infill.clone()
            }
            BuildingLodMaterial::Wall(WallMaterialClass::CivilianMasonry) => self.brick.clone(),
            BuildingLodMaterial::Wall(WallMaterialClass::InternalTimber) => palette.timber.clone(),
            BuildingLodMaterial::Wall(_) | BuildingLodMaterial::CrownMasonry => self.stone.clone(),
            BuildingLodMaterial::Roof(RoofMaterial::ClayTile) => palette.tile.clone(),
            BuildingLodMaterial::Roof(RoofMaterial::Slate | RoofMaterial::Lead) => {
                self.slate.clone()
            }
            BuildingLodMaterial::Roof(_) => self.timber_roof.clone(),
            BuildingLodMaterial::FachwerkBaked => palette.fachwerk_baked.clone(),
            BuildingLodMaterial::Timber => palette.timber.clone(),
            BuildingLodMaterial::Iron => self.iron.clone(),
            BuildingLodMaterial::Floor => self.floor.clone(),
            BuildingLodMaterial::Glass => self.glass.clone(),
            BuildingLodMaterial::FacadeDetails => self.details.clone(),
            BuildingLodMaterial::CrownMask => self.crown_mask.clone(),
        }
    }
}

pub(in crate::presentation) fn setup_tactical_building_materials(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    procedural_textures: Res<ProceduralTextureAssets>,
) {
    let appearances = BuildingAppearance::ALL
        .into_iter()
        .map(|appearance| {
            let spec = appearance.spec();
            let infill = if spec.finish == FacadeFinish::BrickInfill {
                images.add(brick_texture(spec.infill))
            } else {
                images.add(checker_texture(spec.infill[0], spec.infill[1]))
            };
            let timber = images.add(checker_texture(spec.timber[0], spec.timber[1]));
            let tile = images.add(checker_texture(spec.tile[0], spec.tile[1]));
            let fachwerk = images.add(fachwerk_baked_texture(spec));
            AppearanceMaterials {
                finish: spec.finish,
                infill: materials.add(opaque_material(infill)),
                timber: materials.add(opaque_material(timber)),
                tile: materials.add(opaque_material(tile)),
                fachwerk_baked: materials.add(opaque_material(fachwerk)),
            }
        })
        .collect();
    let brick = images.add(brick_texture([[137, 63, 43, 255], [102, 43, 31, 255]]));
    let stone = images.add(checker_texture([121, 122, 111, 255], [88, 91, 84, 255]));
    let slate = images.add(checker_texture([61, 67, 73, 255], [40, 45, 51, 255]));
    let timber_roof = images.add(checker_texture([91, 57, 31, 255], [61, 38, 24, 255]));
    let floor = images.add(checker_texture([109, 94, 72, 255], [83, 72, 57, 255]));
    let details = images.add(facade_atlas());
    commands.insert_resource(TacticalBuildingMaterials {
        appearances,
        brick: materials.add(opaque_material(brick)),
        stone: materials.add(opaque_material(stone.clone())),
        slate: materials.add(opaque_material(slate)),
        timber_roof: materials.add(opaque_material(timber_roof)),
        iron: materials.add(StandardMaterial {
            base_color: Color::srgb(0.055, 0.06, 0.065),
            perceptual_roughness: 0.42,
            metallic: 0.78,
            ..default()
        }),
        floor: materials.add(opaque_material(floor)),
        glass: materials.add(standard_window_glass_material(&procedural_textures)),
        details: materials.add(StandardMaterial {
            base_color_texture: Some(details),
            perceptual_roughness: 0.9,
            cull_mode: None,
            ..default()
        }),
        crown_mask: materials.add(crenellation_mask_material(&procedural_textures)),
    });
    commands.insert_resource(super::TacticalBuildingMeshCache::default());
}

fn crenellation_mask_material(textures: &ProceduralTextureAssets) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(textures.crenellation_mask.clone()),
        perceptual_roughness: 0.95,
        alpha_mode: AlphaMode::Mask(CRENELLATION_ALPHA_CUTOFF),
        cull_mode: None,
        ..default()
    }
}

fn standard_window_glass_material(textures: &ProceduralTextureAssets) -> StandardMaterial {
    // Bevy's StandardMaterial reads G as roughness from this RG texture but
    // has no per-texel thickness input. Packed R remains generated evidence
    // and future custom-shader data; runtime thickness is the nominal scalar.
    StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(textures.window_glass.transmittance.clone()),
        normal_map_texture: Some(textures.window_glass.optical_normal_gl.clone()),
        metallic_roughness_texture: Some(textures.window_glass.thickness_roughness.clone()),
        perceptual_roughness: WINDOW_GLASS_MATERIAL_CONTRACT.base_perceptual_roughness,
        metallic: 0.0,
        reflectance: 0.5,
        diffuse_transmission: WINDOW_GLASS_MATERIAL_CONTRACT.diffuse_transmission,
        specular_transmission: WINDOW_GLASS_MATERIAL_CONTRACT.specular_transmission,
        thickness: WINDOW_GLASS_MATERIAL_CONTRACT.nominal_thickness_metres,
        ior: WINDOW_GLASS_MATERIAL_CONTRACT.index_of_refraction,
        attenuation_distance: WINDOW_GLASS_MATERIAL_CONTRACT.attenuation_distance_metres,
        attenuation_color: Color::linear_rgb(
            WINDOW_GLASS_MATERIAL_CONTRACT.attenuation_color_linear[0],
            WINDOW_GLASS_MATERIAL_CONTRACT.attenuation_color_linear[1],
            WINDOW_GLASS_MATERIAL_CONTRACT.attenuation_color_linear[2],
        ),
        cull_mode: None,
        ..default()
    }
}

fn opaque_material(texture: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(texture),
        perceptual_roughness: 0.9,
        cull_mode: Some(Face::Back),
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_glass_uses_nominal_scalar_thickness_not_packed_red() {
        let mut images = Assets::default();
        let textures = adventuresim_procedural_textures::generate_procedural_textures(&mut images);
        let packed_thickness_and_roughness = textures.window_glass.thickness_roughness.clone();
        let material = standard_window_glass_material(&textures);

        assert_eq!(
            material.thickness,
            WINDOW_GLASS_MATERIAL_CONTRACT.nominal_thickness_metres
        );
        assert_eq!(
            material.metallic_roughness_texture,
            Some(packed_thickness_and_roughness)
        );
        // StandardMaterial defines this binding's G channel as roughness. It
        // has no thickness-map field, so packed R cannot change the scalar.
        assert_eq!(material.thickness, 0.0032);
    }

    #[test]
    fn crown_material_uses_generated_mask_with_the_recipe_cutoff() {
        let mut images = Assets::default();
        let textures = adventuresim_procedural_textures::generate_procedural_textures(&mut images);
        let generated_mask = textures.crenellation_mask.clone();
        let material = crenellation_mask_material(&textures);

        assert_eq!(material.base_color_texture, Some(generated_mask));
        assert_eq!(
            material.alpha_mode,
            AlphaMode::Mask(CRENELLATION_ALPHA_CUTOFF)
        );
        assert_eq!(material.cull_mode, None);
    }

    #[test]
    fn appearance_is_stable_and_all_curated_variants_are_reachable() {
        let first = BuildingAppearance::for_building(1744);
        assert_eq!(first, BuildingAppearance::for_building(1744));

        let reached = (0..10_000)
            .map(BuildingAppearance::for_building)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(reached.len(), BuildingAppearance::ALL.len());
    }

    #[test]
    fn appearance_distribution_remains_restrained() {
        let appearances = (0..100_000)
            .map(BuildingAppearance::for_building)
            .collect::<Vec<_>>();
        let share = |appearance| {
            appearances
                .iter()
                .filter(|candidate| **candidate == appearance)
                .count() as f32
                / appearances.len() as f32
        };
        let natural =
            share(BuildingAppearance::NaturalOak) + share(BuildingAppearance::WeatheredOak);
        let red = share(BuildingAppearance::OxideRed) + share(BuildingAppearance::RedBrownBrick);
        let rendered =
            share(BuildingAppearance::RenderedCream) + share(BuildingAppearance::RenderedOchre);

        assert!((0.33..=0.37).contains(&natural));
        assert!((0.28..=0.32).contains(&red));
        assert!((0.08..=0.12).contains(&rendered));
    }
}
