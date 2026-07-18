//! Deterministic, native strategic-layer experiment harness.

mod analysis;
mod config;
mod profile;
mod rng;
mod runner;

pub use analysis::*;
pub use config::*;
pub use profile::*;
pub use runner::*;

pub const FORMAT_VERSION: u32 = 1;
