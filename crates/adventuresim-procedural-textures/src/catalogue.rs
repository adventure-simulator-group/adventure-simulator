/// Stable identifier used by review tools and future recipe-specific agents.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextureRecipeId {
    WhiteOakLeaf,
    DryWhiteOakLeaf,
    HazelLeaf,
    BlackthornLeaf,
    HawthornLeaf,
    BeechLeaf,
    OakBark,
    ForestSoil,
    ForestLitter,
    Rock,
    LimePlaster,
    HewnOak,
    WattleAndDaub,
    HandmadeBrick,
    RubbleMasonry,
    DressedStone,
    ClayRoofTile,
    SlateRoof,
    TimberShingle,
    PlankFloor,
    LeadSheet,
    Ironwork,
    WindowGlass,
    CrenellationMask,
}

impl TextureRecipeId {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::WhiteOakLeaf => "white-oak-leaf",
            Self::DryWhiteOakLeaf => "dry-white-oak-leaf",
            Self::HazelLeaf => "hazel-leaf",
            Self::BlackthornLeaf => "blackthorn-leaf",
            Self::HawthornLeaf => "hawthorn-leaf",
            Self::BeechLeaf => "beech-leaf",
            Self::OakBark => "oak-bark",
            Self::ForestSoil => "forest-soil",
            Self::ForestLitter => "forest-litter",
            Self::Rock => "rock",
            Self::LimePlaster => "lime-plaster",
            Self::HewnOak => "hewn-oak",
            Self::WattleAndDaub => "wattle-and-daub",
            Self::HandmadeBrick => "handmade-brick",
            Self::RubbleMasonry => "rubble-masonry",
            Self::DressedStone => "dressed-stone",
            Self::ClayRoofTile => "clay-roof-tile",
            Self::SlateRoof => "slate-roof",
            Self::TimberShingle => "timber-shingle",
            Self::PlankFloor => "plank-floor",
            Self::LeadSheet => "lead-sheet",
            Self::Ironwork => "ironwork",
            Self::WindowGlass => "window-glass",
            Self::CrenellationMask => "crenellation-mask",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureFamily {
    Foliage,
    Wood,
    Ground,
    Stone,
    Wall,
    Roof,
    Floor,
    Metal,
    Glass,
    Mask,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureOutput {
    Albedo,
    Opacity,
    Normal,
    Height,
    AmbientRoughnessMetallic,
    PackedHeightAmbientOcclusion,
    Transmittance,
    OpticalNormal,
    PackedThicknessRoughness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureRecipeStatus {
    Implemented,
    Baseline,
    Planned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureRecipeDescriptor {
    pub id: TextureRecipeId,
    pub family: TextureFamily,
    pub status: TextureRecipeStatus,
    pub outputs: &'static [TextureOutput],
}

const LEAF_OUTPUTS: &[TextureOutput] = &[
    TextureOutput::Albedo,
    TextureOutput::Opacity,
    TextureOutput::Normal,
    TextureOutput::Height,
    TextureOutput::AmbientRoughnessMetallic,
];
const SURFACE_OUTPUTS: &[TextureOutput] = &[
    TextureOutput::Albedo,
    TextureOutput::Normal,
    TextureOutput::Height,
    TextureOutput::AmbientRoughnessMetallic,
];
const HEIGHT_AO_OUTPUTS: &[TextureOutput] = &[TextureOutput::PackedHeightAmbientOcclusion];
const GLASS_OUTPUTS: &[TextureOutput] = &[
    TextureOutput::Transmittance,
    TextureOutput::OpticalNormal,
    TextureOutput::PackedThicknessRoughness,
];
const MASK_OUTPUTS: &[TextureOutput] = &[TextureOutput::Opacity];

macro_rules! recipe {
    ($id:ident, $family:ident, $status:ident, $outputs:ident) => {
        TextureRecipeDescriptor {
            id: TextureRecipeId::$id,
            family: TextureFamily::$family,
            status: TextureRecipeStatus::$status,
            outputs: $outputs,
        }
    };
}

/// Complete inventory. Planned entries deliberately have no placeholder
/// generator: recipe work becomes visible only when it produces real outputs.
pub const PROCEDURAL_TEXTURE_CATALOGUE: &[TextureRecipeDescriptor] = &[
    recipe!(WhiteOakLeaf, Foliage, Implemented, LEAF_OUTPUTS),
    recipe!(DryWhiteOakLeaf, Foliage, Implemented, LEAF_OUTPUTS),
    recipe!(HazelLeaf, Foliage, Implemented, LEAF_OUTPUTS),
    recipe!(BlackthornLeaf, Foliage, Implemented, LEAF_OUTPUTS),
    recipe!(HawthornLeaf, Foliage, Implemented, LEAF_OUTPUTS),
    recipe!(BeechLeaf, Foliage, Implemented, LEAF_OUTPUTS),
    recipe!(OakBark, Wood, Implemented, HEIGHT_AO_OUTPUTS),
    recipe!(ForestSoil, Ground, Implemented, HEIGHT_AO_OUTPUTS),
    recipe!(ForestLitter, Ground, Implemented, SURFACE_OUTPUTS),
    recipe!(Rock, Stone, Implemented, SURFACE_OUTPUTS),
    recipe!(LimePlaster, Wall, Implemented, SURFACE_OUTPUTS),
    recipe!(HewnOak, Wood, Implemented, SURFACE_OUTPUTS),
    recipe!(WattleAndDaub, Wall, Implemented, SURFACE_OUTPUTS),
    recipe!(HandmadeBrick, Wall, Implemented, SURFACE_OUTPUTS),
    recipe!(RubbleMasonry, Stone, Implemented, SURFACE_OUTPUTS),
    recipe!(DressedStone, Stone, Implemented, SURFACE_OUTPUTS),
    recipe!(ClayRoofTile, Roof, Implemented, SURFACE_OUTPUTS),
    recipe!(SlateRoof, Roof, Implemented, SURFACE_OUTPUTS),
    recipe!(TimberShingle, Roof, Implemented, SURFACE_OUTPUTS),
    recipe!(PlankFloor, Floor, Implemented, SURFACE_OUTPUTS),
    recipe!(LeadSheet, Metal, Implemented, SURFACE_OUTPUTS),
    recipe!(Ironwork, Metal, Implemented, SURFACE_OUTPUTS),
    recipe!(WindowGlass, Glass, Implemented, GLASS_OUTPUTS),
    recipe!(CrenellationMask, Mask, Implemented, MASK_OUTPUTS),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn recipe_ids_are_unique() {
        let ids = PROCEDURAL_TEXTURE_CATALOGUE
            .iter()
            .map(|recipe| recipe.id)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), PROCEDURAL_TEXTURE_CATALOGUE.len());
    }

    #[test]
    fn lime_plaster_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::LimePlaster
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn handmade_brick_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::HandmadeBrick
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn rubble_masonry_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::RubbleMasonry
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn dressed_stone_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::DressedStone
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn clay_roof_tile_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::ClayRoofTile
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn slate_roof_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::SlateRoof
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn timber_shingle_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::TimberShingle
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn wattle_and_daub_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::WattleAndDaub
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn hewn_oak_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::HewnOak
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn plank_floor_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::PlankFloor
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn ironwork_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::Ironwork
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn lead_sheet_recipe_claims_its_generated_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::LeadSheet
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == SURFACE_OUTPUTS
        }));
    }

    #[test]
    fn window_glass_recipe_claims_transmission_outputs() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::WindowGlass
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == GLASS_OUTPUTS
        }));
    }

    #[test]
    fn crenellation_mask_recipe_claims_generated_opacity() {
        assert!(PROCEDURAL_TEXTURE_CATALOGUE.iter().any(|recipe| {
            recipe.id == TextureRecipeId::CrenellationMask
                && recipe.status == TextureRecipeStatus::Implemented
                && recipe.outputs == MASK_OUTPUTS
        }));
    }
}
