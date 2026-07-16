use adventuresim_world_schema::{
    ElevationMeters, SourceProvenance, TravelEdgeImport, TravelEdgeKind, WorldBuildReport,
    WorldNodeImport,
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
