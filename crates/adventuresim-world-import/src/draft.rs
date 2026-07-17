use adventuresim_world_schema::{
    DroughtProfile, EdgeEndpoint, ElevationMeters, ForestCover, LandUseProfile,
    PotentialVegetation, SettlementAliasImport, SettlementDescriptionImport, SoilProfile,
    SourceProvenance, SpatialGridSpec, TravelEdgeKind, TreeSpeciesProfile, WorldBuildReport,
    WorldNodeImport,
};

#[derive(Debug)]
pub(crate) struct WorldDraft<S> {
    pub(crate) year: i32,
    pub(crate) spatial_grid: SpatialGridSpec,
    pub(crate) sources: Vec<SourceProvenance>,
    pub(crate) road_types: Vec<TravelEdgeKind>,
    pub(crate) nodes: Vec<WorldNodeImport>,
    pub(crate) edges: Vec<TravelEdgeDraft>,
    pub(crate) settlement_aliases: Vec<SettlementAliasImport>,
    pub(crate) settlement_descriptions: Vec<SettlementDescriptionImport>,
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
    pub(crate) sources: String,
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
    pub(crate) sources: String,
}

pub(crate) trait SettlementDraftAccess {
    fn base_settlement(&self) -> &SettlementDraft;
    fn base_settlement_mut(&mut self) -> &mut SettlementDraft;
}

impl SettlementDraftAccess for SettlementDraft {
    fn base_settlement(&self) -> &SettlementDraft {
        self
    }

    fn base_settlement_mut(&mut self) -> &mut SettlementDraft {
        self
    }
}

macro_rules! delegate_settlement_access {
    ($type:ty, $field:ident) => {
        impl SettlementDraftAccess for $type {
            fn base_settlement(&self) -> &SettlementDraft {
                self.$field.base_settlement()
            }

            fn base_settlement_mut(&mut self) -> &mut SettlementDraft {
                self.$field.base_settlement_mut()
            }
        }
    };
}

pub(crate) fn push_source_note(target: &mut impl SettlementDraftAccess, note: &str) {
    let sources = &mut target.base_settlement_mut().sources;
    if !sources.is_empty() {
        sources.push('\n');
    }
    sources.push_str("- ");
    sources.push_str(note);
}

#[derive(Debug)]
pub(crate) struct ElevatedSettlementDraft {
    pub(crate) settlement: SettlementDraft,
    pub(crate) elevation: ElevationMeters,
}
delegate_settlement_access!(ElevatedSettlementDraft, settlement);

#[derive(Debug)]
pub(crate) struct LandUseSettlementDraft {
    pub(crate) elevated: ElevatedSettlementDraft,
    pub(crate) land_use: LandUseProfile,
}
delegate_settlement_access!(LandUseSettlementDraft, elevated);

#[derive(Debug)]
pub(crate) struct ForestSettlementDraft {
    pub(crate) land: LandUseSettlementDraft,
    pub(crate) forest_cover: ForestCover,
}
delegate_settlement_access!(ForestSettlementDraft, land);

#[derive(Debug)]
pub(crate) struct PotentialVegetationSettlementDraft {
    pub(crate) forest: ForestSettlementDraft,
    pub(crate) potential_vegetation: PotentialVegetation,
}
delegate_settlement_access!(PotentialVegetationSettlementDraft, forest);

#[derive(Debug)]
pub(crate) struct TreeSpeciesSettlementDraft {
    pub(crate) vegetated: PotentialVegetationSettlementDraft,
    pub(crate) tree_species: TreeSpeciesProfile,
}
delegate_settlement_access!(TreeSpeciesSettlementDraft, vegetated);

#[derive(Debug)]
pub(crate) struct SoilSettlementDraft {
    pub(crate) trees: TreeSpeciesSettlementDraft,
    pub(crate) soil: SoilProfile,
}
delegate_settlement_access!(SoilSettlementDraft, trees);

#[derive(Debug)]
pub(crate) struct GeologySettlementDraft {
    pub(crate) soil: SoilSettlementDraft,
    pub(crate) geology: adventuresim_world_schema::SurfaceGeology,
}
delegate_settlement_access!(GeologySettlementDraft, soil);

#[derive(Debug)]
pub(crate) struct ReligionSettlementDraft {
    pub(crate) geologic: GeologySettlementDraft,
    pub(crate) religious_status: adventuresim_world_schema::SettlementReligiousStatus,
}
delegate_settlement_access!(ReligionSettlementDraft, geologic);

#[derive(Debug)]
pub(crate) struct DroughtSettlementDraft {
    pub(crate) religious: ReligionSettlementDraft,
    pub(crate) drought: DroughtProfile,
}
delegate_settlement_access!(DroughtSettlementDraft, religious);

#[cfg(test)]
mod tests {
    use super::{SettlementDraft, push_source_note};

    #[test]
    fn source_notes_append_as_deterministic_markdown_bullets() {
        let mut settlement = SettlementDraft {
            id: "test".into(),
            source_node_id: 1,
            name: "Test".into(),
            longitude: 0.0,
            latitude: 0.0,
            population_level: 1,
            population_estimate: 0,
            scene_key: "test".into(),
            sources: "- First source.".into(),
        };

        push_source_note(&mut settlement, "**Fallback:** Deterministic guess.");

        assert_eq!(
            settlement.sources,
            "- First source.\n- **Fallback:** Deterministic guess."
        );
    }
}
