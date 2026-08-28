mod geometry;
mod impostor;
mod lod;
mod materials;
mod presentation;

pub(in crate::presentation) use geometry::{
    BLACKTHORN_PARAMETERS, COMMON_BEECH_PARAMETERS, COMMON_HAWTHORN_PARAMETERS,
    COMMON_HAZEL_PARAMETERS, OAK_GNARLING_SHOWCASE, OakGnarlingParameters,
    TREE_PRIMARY_GROUP_COUNT, TreeBranchSegment, TreeLeaf, WoodyBranchMeshQuality,
    procedural_oak_bud_group_mesh, procedural_oak_bud_mesh, procedural_oak_leaf_card_group_mesh,
    procedural_oak_leaf_card_mesh, procedural_oak_leaves, procedural_oak_skeleton_with_gnarling,
    procedural_oak_textured_leaf_group_mesh, procedural_oak_textured_leaf_mesh,
    procedural_tree_branch_group_mesh, procedural_tree_branch_mesh, procedural_tree_skeleton,
    procedural_woody_branch_mesh, procedural_woody_cambered_leaf_mesh, procedural_woody_crown_mesh,
    procedural_woody_leaf_card_mesh, procedural_woody_mid_trunk_mesh,
    procedural_woody_plant_leaves, procedural_woody_plant_skeleton,
    procedural_woody_sparse_leaf_card_mesh,
};
pub(crate) use impostor::TreeImpostorProvenance;
pub(in crate::presentation) use lod::update_tree_projected_lod_ranges;
pub(crate) use lod::{
    PlayableTreeAggregateWood, PlayableTreeBuds, PlayableTreeCanopyCard,
    PlayableTreeDetailedLeaves, PlayableTreeDetailedTrunk, PlayableTreeDetailedWood,
    PlayableTreeMidTrunk, PlayableTreeTrunk, TacticalTreeBenchmarkIsolation,
    TreeLeafRepresentation, TreeLod, TreeLodCluster, TreeLodRenderOverride, TreeTrunkLod,
};
pub(crate) use materials::{
    TacticalTreeAggregateBarkMaterial, TacticalTreeBarkMaterial, TacticalTreeLeafCardMaterial,
    oak_aggregate_bark_material, oak_bark_material, oak_leaf_material,
};
pub(in crate::presentation) use materials::{
    TacticalTreeImpostorMaterial, beech_aggregate_bark_material, beech_bark_material,
    beech_leaf_material, blackthorn_leaf_material, hawthorn_leaf_material, hazel_leaf_material,
    leaf_material,
};
pub(in crate::presentation) use presentation::canopy_competition;
pub(in crate::presentation) use presentation::{
    PendingTreePresentation, StreamedTreePresentation, TreePresentationCache,
    TreePresentationSpecies, VistaTreePresentationCache, ensure_vista_tree_variant,
    present_pending_trees, stream_tree_lod_children, tree_species_for_site,
};
pub(crate) use presentation::{
    PresentedTree, TreeAssetResidencyDiagnostics, TreeLeafTriangleCount,
};
