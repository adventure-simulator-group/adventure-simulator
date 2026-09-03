//! Exposed polygonal columns from cooling contraction. Vertical axes assume
//! approximately horizontal cooling surfaces; the map does not measure them.

use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

const COLUMN_SPACING_METRES: f32 = 2.4;

pub(super) fn front(along: f32, seed: u64) -> f32 {
    let phase = inclusive_unit_f32(splitmix64(seed)) * COLUMN_SPACING_METRES;
    let local = (along + phase).rem_euclid(COLUMN_SPACING_METRES) - COLUMN_SPACING_METRES * 0.5;
    // Three exposed faces of connected hexagonal prisms. Bedrock remains
    // behind them: joints are not open air gaps or unsupported separate poles.
    let side = (local.abs() - COLUMN_SPACING_METRES * 0.25).max(0.0);
    -3.0_f32.sqrt() * side
}
