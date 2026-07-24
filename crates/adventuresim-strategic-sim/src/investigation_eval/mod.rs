//! Offline, observer-safe evaluation of generated investigation quests.
//!
//! The evaluator deliberately has two halves. A policy receives only
//! [`PlayerFrame`] and produces an opaque choice. Canonical generator output is
//! retained by [`DeveloperCaseAnalysis`] and joined after the run for scoring.

mod environment;
mod policy;
mod provider;
mod report;
mod types;

pub use environment::*;
pub use policy::*;
pub use provider::*;
pub use report::*;
pub use types::*;
