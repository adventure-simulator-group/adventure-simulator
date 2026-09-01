use super::*;

#[derive(Resource, Clone, Debug)]
pub(crate) struct ProceduralEnvironmentAssets {
    pub(in crate::presentation) oak_leaf: LeafTextureSet,
    pub(in crate::presentation) dry_oak_leaf: LeafTextureSet,
    pub(in crate::presentation) hazel_leaf: LeafTextureSet,
    pub(in crate::presentation) blackthorn_leaf: LeafTextureSet,
    pub(in crate::presentation) hawthorn_leaf: LeafTextureSet,
    pub(in crate::presentation) beech_leaf: LeafTextureSet,
    pub(in crate::presentation) oak_bark: BarkTextureSet,
    pub(in crate::presentation) forest_soil: GroundTextureSet,
    pub(in crate::presentation) rock: SurfaceTextureSet,
    pub(crate) terrain_blood_mask: Handle<Image>,
}
