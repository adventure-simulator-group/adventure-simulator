//! Deterministic, renderer-independent melee weapon designs and triangle meshes.

mod catalog;
mod codec;
mod derive;
mod design;
mod hash;
mod icon;
mod mesh;
mod validation;

pub use catalog::{
    MELEE_CATALOG_IDS, PRESET_IDS, default_design, default_holder_design, preset_design,
    recommended_holder,
};
pub use codec::{CodecError, decode, decode_holder, encode, encode_holder};
pub use derive::{derive_holder_properties, derive_material_masses, derive_properties};
pub use design::*;
pub use hash::{DesignHash, design_hash, holder_design_hash};
pub use icon::{
    ICON_RENDERER_VERSION, IconBounds, IconError, WeaponIcon, WeaponIconLayout, WeaponIconSpec,
    generate_holder_icon, generate_icon, icon_layout,
};
pub use mesh::{GenerateError, generate, generate_holder};
pub use validation::{ValidationError, validate, validate_holder};

pub const SCHEMA_VERSION: u16 = 3;
pub const GENERATOR_VERSION: u16 = 3;
pub const HOLDER_SCHEMA_VERSION: u16 = 1;
pub const HOLDER_GENERATOR_VERSION: u16 = 1;
