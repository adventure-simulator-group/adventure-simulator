//! Deterministic, presenter-independent puzzle generation and analysis.

pub const ORDERED_SIGIL_RULES_VERSION: u16 = 3;
pub const ORDERED_SIGIL_COUNT: usize = 5;
pub const MAX_MINIMIZATION_SUBSETS: usize = 100_000;

mod analysis;
mod ordered_sigils;
mod puzzles;

pub use analysis::*;
pub use ordered_sigils::*;
pub use puzzles::*;
