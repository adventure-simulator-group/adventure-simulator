use adventuresim_world_schema::{
    DroughtProfile, EdgeEndpoint, ElevationMeters, ForestCover, LandUseProfile,
    PotentialVegetation, SoilProfile, SourceProvenance, TravelEdgeKind, TreeSpeciesProfile,
    WorldBuildReport, WorldNodeImport,
};

#[derive(Debug)]
pub(crate) struct WorldDraft<S> {
    pub(crate) year: i32,
    pub(crate) sources: Vec<SourceProvenance>,
    pub(crate) road_types: Vec<TravelEdgeKind>,
    pub(crate) nodes: Vec<WorldNodeImport>,
    pub(crate) edges: Vec<TravelEdgeDraft>,
    pub(crate) settlements: Vec<S>,
    pub(crate) report: WorldBuildReport,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TravelRouteDraft {
    Land { bridge: Option<EdgeEndpoint> },
    Ferry,
}

impl TravelRouteDraft {
    pub(crate) const fn has_crossing(self) -> bool {
        matches!(self, Self::Land { bridge: Some(_) } | Self::Ferry)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TravelEdgeDraft {
    pub(crate) id: u64,
    pub(crate) from_node_id: u64,
    pub(crate) to_node_id: u64,
    pub(crate) route: TravelRouteDraft,
    pub(crate) toll: Option<EdgeEndpoint>,
    pub(crate) length_m: u32,
    pub(crate) slope_multiplier: f32,
    pub(crate) certainty: u8,
    pub(crate) section: String,
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

#[derive(Debug)]
pub(crate) struct SoilSettlementDraft {
    pub(crate) trees: TreeSpeciesSettlementDraft,
    pub(crate) soil: SoilProfile,
}

#[derive(Debug)]
pub(crate) struct GeologySettlementDraft {
    pub(crate) soil: SoilSettlementDraft,
    pub(crate) geology: adventuresim_world_schema::SurfaceGeology,
}

#[derive(Debug)]
pub(crate) struct ReligionSettlementDraft {
    pub(crate) geologic: GeologySettlementDraft,
    pub(crate) religious_status: adventuresim_world_schema::SettlementReligiousStatus,
}

#[derive(Debug)]
pub(crate) struct DroughtSettlementDraft {
    pub(crate) religious: ReligionSettlementDraft,
    pub(crate) drought: DroughtProfile,
}
