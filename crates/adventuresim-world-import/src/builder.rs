use std::path::Path;

use adventuresim_world_schema::CompiledWorld;

use crate::{Result, sources::viabundus, validation};

#[derive(Clone, Copy, Debug)]
pub struct WorldBuilder {
    year: i32,
}

impl WorldBuilder {
    pub const fn new(year: i32) -> Self {
        Self { year }
    }

    pub fn build_from_viabundus(self, directory: &Path) -> Result<CompiledWorld> {
        let world = viabundus::compile(directory, self.year)?;
        validation::validate(&world)?;
        Ok(world)
    }
}
