//! Angular joint-bounded detachment scars in competent plutonic rock.
//! Joint spacing and exposed blocks are a procedural realization, not a
//! reconstruction of mapped fractures or a particular rockfall event.

use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

const JOINT_SPACING_METRES: f32 = 3.8;
const JOINT_SETBACK_METRES: f32 = 0.45;
const DETACHED_BLOCK_DEPTH_METRES: f32 = 1.1;
const DETACHED_BLOCK_RELIEF_FRACTION: f32 = 0.35;
const UPPER_JOINT_DEPTH_FRACTION: f32 = 0.3;
const LOWER_JOINT_DEPTH_FRACTION: f32 = 0.72;
const RETAINED_BLOCK_FRACTION: f32 = 0.28;
const JOINT_HEIGHT_VARIATION_FRACTION: f32 = 0.12;
const FRAGMENT_HEIGHT_METRES: f32 = 0.7;
const FRAGMENT_RELIEF_FRACTION: f32 = 0.2;

pub(super) fn front(along: f32, depth_fraction: f32, relief: f32, seed: u64) -> f32 {
    let phase = inclusive_unit_f32(splitmix64(seed)) * JOINT_SPACING_METRES;
    let block = ((along + phase) / JOINT_SPACING_METRES).floor() as i64;
    let variation = inclusive_unit_f32(splitmix64(seed ^ block as u64));
    let setback = JOINT_SETBACK_METRES * variation;
    let joint_shift = (variation - 0.5) * JOINT_HEIGHT_VARIATION_FRACTION;
    let detached = if variation > RETAINED_BLOCK_FRACTION
        && (UPPER_JOINT_DEPTH_FRACTION + joint_shift..LOWER_JOINT_DEPTH_FRACTION + joint_shift)
            .contains(&depth_fraction)
    {
        DETACHED_BLOCK_DEPTH_METRES.min(relief * DETACHED_BLOCK_RELIEF_FRACTION)
    } else {
        0.0
    };
    // The upper rock remains anchored behind short ledges; planar joint
    // intersections replace the rounded hollows used for soluble rock.
    -(setback + detached)
}

pub(super) fn fragments(along: f32, across: f32, relief: f32) -> f32 {
    // Embedded, bevelled fragments remain part of the static talus surface.
    // Authored local centres, half-widths and half-depths in metres.
    let fragments = [
        (-5.5, 3.5, 1.6, 1.4),
        (0.5, 4.2, 1.2, 1.0),
        (6.0, 3.0, 1.8, 1.3),
    ];
    fragments
        .into_iter()
        .map(|(x, z, width, depth)| {
            let distance = ((along - x) / width)
                .abs()
                .max(((across - z) / depth).abs());
            (1.0 - distance).clamp(0.0, 0.7)
        })
        .fold(0.0_f32, f32::max)
        * FRAGMENT_HEIGHT_METRES.min(relief * FRAGMENT_RELIEF_FRACTION)
}
