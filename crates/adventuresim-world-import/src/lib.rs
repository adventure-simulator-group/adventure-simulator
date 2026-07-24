//! Offline world compiler.
//!
//! Source modules parse their own formats. The outer builder combines those
//! source models into the canonical, source-independent import schema.

pub mod builder;
mod draft;
pub mod error;
mod manifest;
mod sources;
pub mod spatial;
mod validation;

pub use builder::WorldBuilder;
pub use error::{Error, Result};
pub use sources::drought::derive_profiles as derive_owda_profiles;
#[cfg(feature = "strategic-map-renderer")]
pub use sources::forest_cover::{
    PREPARED_FOREST_FORMAT, PreparedForestRaster, read_prepared_forest_raster,
    validate_prepared_forest_manifest,
};
pub use sources::potential_vegetation::{WetlandSpatialData, wetland_spatial_data};
pub use validation::validate as validate_world;
