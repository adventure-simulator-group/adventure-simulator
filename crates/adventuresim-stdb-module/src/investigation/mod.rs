//! Private investigation authority and observer-safe gateway projections.

//! Implementation is partitioned by behavior domain below. The fragments
//! intentionally share this module scope because SpacetimeDB discovers table,
//! reducer, and view macros in this module and generates accessor names from
//! that scope. Moving those declarations into ordinary child modules changes
//! generated bindings. Macro-free calculations remain grouped with the
//! authority or observer-safe projection that owns them; tests are partitioned
//! by evidence, projection, site/action, and authority behavior.

#[cfg(test)]
pub(crate) const INVESTIGATION_SOURCE: &str = concat!(
    include_str!("model.rs"),
    include_str!("geometry.rs"),
    include_str!("projections.rs"),
    include_str!("capabilities.rs"),
    include_str!("actions.rs"),
    include_str!("sites.rs"),
    include_str!("claims.rs"),
);

mod geometry;
use geometry::coordinate_area_contains_e7;

include!("model.rs");
include!("projections.rs");
include!("capabilities.rs");
include!("actions.rs");
include!("sites.rs");
include!("claims.rs");
include!("tests.rs");
