use std::path::Path;

use adventuresim_world_schema::CompiledWorld;

use crate::{
    Result,
    sources::{elevation, forest_cover, land_use, potential_vegetation, viabundus},
    validation,
};

#[derive(Clone, Copy, Debug)]
pub struct WorldBuilder {
    year: i32,
}

impl WorldBuilder {
    pub const fn new(year: i32) -> Self {
        Self { year }
    }

    pub fn build_from_sources(
        self,
        viabundus_directory: &Path,
        elevation_directory: &Path,
        land_use_directory: &Path,
        forest_cover_directory: &Path,
        potential_vegetation_directory: &Path,
    ) -> Result<CompiledWorld> {
        let draft = viabundus::compile(viabundus_directory, self.year)?;
        let draft = elevation::enrich(draft, elevation_directory)?;
        let draft = land_use::enrich(draft, land_use_directory)?;
        let draft = forest_cover::enrich(draft, forest_cover_directory)?;
        let world = potential_vegetation::enrich(draft, potential_vegetation_directory)?;
        validation::validate(&world)?;
        Ok(world)
    }
}
