use std::path::Path;

use adventuresim_world_schema::{
    CompiledWorld, SettlementAliasImport, SettlementDescriptionImport, WorldBuildReport,
};

use crate::{
    Result,
    sources::{
        drought, elevation, forest_cover, geology, hydrology, land_use, potential_vegetation,
        religion, soil, tree_species, viabundus,
    },
    validation,
};

#[derive(Clone, Copy, Debug)]
pub struct WorldBuilder {
    year: i32,
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
    pub const fn new(year: i32) -> Self {
        Self { year }
    }

    pub fn build_from_viabundus(self, directory: &Path) -> Result<ViabundusEnrichment> {
        let draft = viabundus::compile(directory, self.year)?;
        Ok(ViabundusEnrichment {
            settlement_aliases: draft.settlement_aliases,
            settlement_descriptions: draft.settlement_descriptions,
            report: draft.report,
        })
    }

    pub fn build_from_sources(
        self,
        viabundus_directory: &Path,
        elevation_directory: &Path,
        land_use_directory: &Path,
        forest_cover_directory: &Path,
        potential_vegetation_directory: &Path,
        tree_species_archive: &Path,
        soil_directory: &Path,
        geology_geopackage: &Path,
        religion_regions: &Path,
        drought_netcdf: &Path,
        hydrology_directory: &Path,
    ) -> Result<CompiledWorld> {
        let draft = viabundus::compile(viabundus_directory, self.year)?;
        let draft = elevation::enrich(draft, elevation_directory)?;
        let draft = land_use::enrich(draft, land_use_directory)?;
        let draft = forest_cover::enrich(draft, forest_cover_directory)?;
        let draft = potential_vegetation::enrich(draft, potential_vegetation_directory)?;
        let draft = tree_species::enrich(draft, tree_species_archive)?;
        let draft = soil::enrich(draft, soil_directory)?;
        let draft = geology::enrich(draft, geology_geopackage)?;
        let draft = religion::enrich(draft, religion_regions)?;
        let draft = drought::enrich(draft, drought_netcdf)?;
        let world = hydrology::enrich(draft, hydrology_directory)?;
        validation::validate(&world)?;
        Ok(world)
    }
}
