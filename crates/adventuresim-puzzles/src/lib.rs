//! Deterministic, presenter-independent puzzle generation and analysis.

pub const ORDERED_SIGIL_RULES_VERSION: u16 = 3;
pub const ORDERED_SIGIL_COUNT: usize = 5;
pub const MAX_MINIMIZATION_SUBSETS: usize = 100_000;

mod analysis;
mod logic_grid;
mod ordered_sigils;
mod puzzles;
mod resource_allocation;

pub use analysis::*;
pub use logic_grid::*;
pub use ordered_sigils::*;
pub use puzzles::*;
pub use resource_allocation::*;
