//! Typed, deterministic generation for investigation-led quests.
//!
//! The public API remains at `quest_generation::*`; implementation source is
//! ordered as model, player-safe projection, solver, assembly, validation,
//! audit, and tests.

//! Implementation is partitioned by behavior domain below. The fragments
//! intentionally share this module scope so public paths, declaration order,
//! privacy, and macro-generated SpacetimeDB names remain unchanged.

include!("model.rs");
include!("projection.rs");
include!("solver.rs");

mod assembly {
    use super::*;
    include!("assembly.rs");
}

mod validation {
    use super::*;
    include!("validation.rs");
}

mod audit {
    use super::*;
    include!("audit.rs");
}

pub use assembly::generate;
pub use audit::{audit, test_witnesses};
pub use validation::validate;
include!("tests.rs");
