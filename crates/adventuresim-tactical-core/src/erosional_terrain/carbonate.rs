//! Localized dissolution hollows in exposed soluble bedrock. Pocket positions
//! are procedural fracture/weathering variation, not mapped conduits or beds.

use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

const MAX_POCKET_DEPTH_METRES: f32 = 1.8;
const MAX_POCKET_RELIEF_FRACTION: f32 = 0.5;
const POCKET_POSITION_JITTER_METRES: f32 = 0.6;

pub(super) fn front(along: f32, depth_fraction: f32, relief: f32, seed: u64) -> f32 {
    // Authored dimensional catalog: centre, half-width, depth centre, depth radius.
    // The hollows remain separated by load-bearing rock; none reaches the roof.
    let pockets = [
        (-6.0, 2.8, 0.59, 0.25),
        (-0.5, 2.3, 0.67, 0.23),
        (5.5, 3.0, 0.56, 0.24),
    ];
    let mut recess: f32 = 0.0;
    for (index, (centre, width, depth, radius)) in pockets.into_iter().enumerate() {
        let jitter = (inclusive_unit_f32(splitmix64(seed ^ index as u64)) - 0.5)
            * POCKET_POSITION_JITTER_METRES;
        let across = (along - centre - jitter) / width;
        let down = (depth_fraction - depth) / radius;
        recess = recess.max((1.0 - across * across - down * down).max(0.0).sqrt());
    }
    -MAX_POCKET_DEPTH_METRES.min(relief * MAX_POCKET_RELIEF_FRACTION) * recess
}
