//! Parsers for upstream datasets. Source-specific fields must not leak into
//! the database boundary unless the game actually needs them.

pub mod drought;
pub mod elevation;
pub mod environment_synthesis;
pub mod forest_cover;
pub mod geology;
pub mod hydrology;
pub mod land_use;
pub mod potential_vegetation;
pub mod religion;
pub mod route_terrain;
pub mod soil;
pub mod tree_species;
pub mod viabundus;
