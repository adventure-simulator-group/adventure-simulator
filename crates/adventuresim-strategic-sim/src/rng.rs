//! Stable random primitives. Algorithm changes require a format-version bump.

use fabelgeist_determinism::SplitMix64;

const SUBSEED_INDEX_STRIDE: u64 = 0xd6e8_feb8_6659_fd93;

/// Derive independent streams without relying on collection or call order.
pub fn sub_seed(root: u64, domain: u64, index: u64) -> u64 {
    let mut rng =
        SplitMix64::new(root ^ domain.rotate_left(17) ^ index.wrapping_mul(SUBSEED_INDEX_STRIDE));
    rng.next_u64()
}
