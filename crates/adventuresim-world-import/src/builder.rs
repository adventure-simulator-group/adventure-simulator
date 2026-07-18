use std::path::Path;

use adventuresim_world_schema::CompiledWorld;

use crate::{
    Result,
    sources::{elevation, viabundus},
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
    ) -> Result<CompiledWorld> {
        let draft = viabundus::compile(viabundus_directory, self.year)?;
        let world = elevation::enrich(draft, elevation_directory)?;
        validation::validate(&world)?;
        Ok(world)
    }
}
