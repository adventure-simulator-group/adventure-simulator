use std::path::Path;

use adventuresim_world_schema::{
    CompiledWorld, PLAYABLE_BOUNDS, SettlementAliasImport, SettlementDescriptionImport,
    SpatialGridSpec, WorldBuildReport,
};

use crate::{
    Result,
    sources::{
        drought, economies, elevation, environment_synthesis, faults, forest_cover, geology,
        hydrology, industries, land_use, potential_vegetation, religion, road_inference,
        route_terrain, soil, tree_species, viabundus,
    },
    validation,
};

#[derive(Clone, Copy, Debug)]
pub struct WorldBuilder {
    year: i32,
    spatial_grid: SpatialGridSpec,
    bounds: Option<[f64; 4]>,
}

#[derive(Clone, Copy, Debug)]
pub struct WorldSourcePaths<'a> {
    pub viabundus: &'a Path,
    pub elevation: &'a Path,
    pub land_use: &'a Path,
    pub forest_cover: &'a Path,
    pub potential_vegetation: &'a Path,
    pub tree_species: &'a Path,
    pub soilgrids: &'a Path,
    pub geology: &'a Path,
    pub faults: &'a Path,
    pub religion_regions: &'a Path,
    pub drought: &'a Path,
    pub hydrology: &'a Path,
}

/// Viabundus-only enrichment output used to inspect source-boundary behavior
/// without fabricating values for the required environmental stages.
#[derive(Debug)]
pub struct ViabundusEnrichment {
    pub settlement_aliases: Vec<SettlementAliasImport>,
    pub settlement_descriptions: Vec<SettlementDescriptionImport>,
    pub report: WorldBuildReport,
}

impl WorldBuilder {
    pub fn new(year: i32) -> Self {
        Self {
            year,
            spatial_grid: SpatialGridSpec::default(),
            bounds: None,
        }
    }

    pub const fn with_spatial_grid(mut self, spatial_grid: SpatialGridSpec) -> Self {
        self.spatial_grid = spatial_grid;
        self
    }

    /// Restrict source topology and settlements before enrichment begins.
    pub const fn with_bounds(mut self, bounds: [f64; 4]) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Apply the repository's authoritative MVP playable boundary.
    pub const fn with_playable_bounds(self) -> Self {
        self.with_bounds(PLAYABLE_BOUNDS)
    }

    pub fn build_from_viabundus(self, directory: &Path) -> Result<ViabundusEnrichment> {
        let draft = viabundus::compile(directory, self.year, self.spatial_grid, self.bounds)?;
        Ok(ViabundusEnrichment {
            settlement_aliases: draft.settlement_aliases,
            settlement_descriptions: draft.settlement_descriptions,
            report: draft.report,
        })
    }

    pub fn build_from_sources(self, sources: WorldSourcePaths<'_>) -> Result<CompiledWorld> {
        self.build_from_sources_inner(sources, None)
    }

    /// Compile with terrain-aware road gap filling against an immutable base
    /// pack containing documented Viabundus roads only.
    pub fn build_from_sources_with_base_terrain(
        self,
        sources: WorldSourcePaths<'_>,
        base_terrain: &adventuresim_terrain::TerrainPack,
    ) -> Result<CompiledWorld> {
        self.build_from_sources_inner(sources, Some(base_terrain))
    }

    fn build_from_sources_inner(
        self,
        sources: WorldSourcePaths<'_>,
        base_terrain: Option<&adventuresim_terrain::TerrainPack>,
    ) -> Result<CompiledWorld> {
        let WorldSourcePaths {
            viabundus,
            elevation,
            land_use,
            forest_cover,
            potential_vegetation,
            tree_species,
            soilgrids,
            geology,
            faults: fault_geopackage,
            religion_regions,
            drought,
            hydrology,
        } = sources;
        let draft = viabundus::compile(viabundus, self.year, self.spatial_grid, self.bounds)?;
        let draft = elevation::enrich(draft, elevation)?;
        let draft = land_use::enrich(draft, land_use)?;
        let draft = forest_cover::enrich(draft, forest_cover)?;
        let draft = potential_vegetation::enrich(draft, potential_vegetation)?;
        let draft = tree_species::enrich(draft, tree_species)?;
        let draft = soil::predict(draft, soilgrids)?;
        let draft = geology::enrich(draft, geology)?;
        let draft = religion::enrich(draft, religion_regions)?;
        let draft = drought::enrich(draft, drought)?;
        let draft = hydrology::enrich(draft, hydrology)?;
        let draft = soil::finalize(draft)?;
        let world = environment_synthesis::finalize(draft)?;
        let world = faults::enrich(
            world,
            fault_geopackage,
            self.bounds.unwrap_or(PLAYABLE_BOUNDS),
        )?;
        let world = geology::windows::enrich(world, geology)?;
        let world = if let Some(terrain) = base_terrain {
            road_inference::enrich(world, terrain)?
        } else {
            world
        };
        let world = route_terrain::enrich(world, elevation)?;
        let world = industries::enrich(world)?;
        let world = economies::enrich(world)?;
        validation::validate(&world)?;
        Ok(world)
    }
}
