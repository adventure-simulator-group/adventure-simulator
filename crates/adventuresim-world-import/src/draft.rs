use adventuresim_world_schema::{
    ElevationMeters, ForestCover, LandUseProfile, PotentialVegetation, SourceProvenance,
    TravelEdgeImport, TravelEdgeKind, TreeSpeciesProfile, WorldBuildReport, WorldNodeImport,
};

#[derive(Debug)]
pub(crate) struct WorldDraft<S> {
    pub(crate) year: i32,
    pub(crate) sources: Vec<SourceProvenance>,
    pub(crate) road_types: Vec<TravelEdgeKind>,
    pub(crate) nodes: Vec<WorldNodeImport>,
    pub(crate) edges: Vec<TravelEdgeImport>,
    pub(crate) settlements: Vec<S>,
    pub(crate) report: WorldBuildReport,
}

#[derive(Debug)]
pub(crate) struct SettlementDraft {
    pub(crate) id: String,
    pub(crate) source_node_id: u64,
    pub(crate) name: String,
    pub(crate) longitude: f64,
    pub(crate) latitude: f64,
    pub(crate) population_level: i32,
    pub(crate) population_estimate: u32,
    pub(crate) scene_key: String,
    pub(crate) religion_id: String,
}

#[derive(Debug)]
pub(crate) struct ElevatedSettlementDraft {
    pub(crate) settlement: SettlementDraft,
    pub(crate) elevation: ElevationMeters,
}

#[derive(Debug)]
pub(crate) struct LandUseSettlementDraft {
    pub(crate) elevated: ElevatedSettlementDraft,
    pub(crate) land_use: LandUseProfile,
}

#[derive(Debug)]
pub(crate) struct ForestSettlementDraft {
    pub(crate) land: LandUseSettlementDraft,
    pub(crate) forest_cover: ForestCover,
}

#[derive(Debug)]
pub(crate) struct PotentialVegetationSettlementDraft {
    pub(crate) forest: ForestSettlementDraft,
    pub(crate) potential_vegetation: PotentialVegetation,
}

#[derive(Debug)]
pub(crate) struct TreeSpeciesSettlementDraft {
    pub(crate) vegetated: PotentialVegetationSettlementDraft,
    pub(crate) tree_species: TreeSpeciesProfile,
}
