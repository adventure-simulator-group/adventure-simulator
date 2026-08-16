//! Deterministic, renderer-independent melee weapon designs and triangle meshes.

mod catalog;
mod codec;
mod derive;
mod design;
mod hash;
mod mesh;
mod validation;

pub use catalog::{MELEE_CATALOG_IDS, PRESET_IDS, default_design, preset_design};
pub use codec::{CodecError, decode, encode};
pub use derive::derive_properties;
pub use design::*;
pub use hash::{DesignHash, design_hash};
pub use mesh::{GenerateError, generate};
pub use validation::{ValidationError, validate};

pub const SCHEMA_VERSION: u16 = 3;
pub const GENERATOR_VERSION: u16 = 3;
