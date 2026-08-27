//! Stable deterministic primitives shared across dependency layers.
//!
//! These functions participate in persisted and replayed behavior. Changing
//! their output requires explicit versioning at every affected boundary.

const SPLITMIX64_INCREMENT: u64 = 0x9e37_79b9_7f4a_7c15;
const SPLITMIX64_FIRST_MULTIPLIER: u64 = 0xbf58_476d_1ce4_e5b9;
const SPLITMIX64_SECOND_MULTIPLIER: u64 = 0x94d0_49bb_1331_11eb;
const UNIT_F32_SCALE: f32 = (1_u32 << 24) as f32;
const INCLUSIVE_UNIT_F32_SCALE: f32 = ((1_u32 << 24) - 1) as f32;
const UNIT_F64_SCALE: f64 = (1_u64 << 53) as f64;
const INCLUSIVE_UNIT_F64_SCALE: f64 = ((1_u64 << 53) - 1) as f64;

/// Apply the avalanche stage used by SplitMix64 to an already-combined word.
pub const fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(SPLITMIX64_FIRST_MULTIPLIER);
    value = (value ^ (value >> 27)).wrapping_mul(SPLITMIX64_SECOND_MULTIPLIER);
    value ^ (value >> 31)
}

/// Derive one stable word from an input seed using a complete SplitMix64 step.
pub const fn splitmix64(seed: u64) -> u64 {
    mix64(seed.wrapping_add(SPLITMIX64_INCREMENT))
}

/// Convert the high 24 bits to the half-open unit interval `[0, 1)`.
pub fn unit_f32(value: u64) -> f32 {
    (value >> 40) as f32 / UNIT_F32_SCALE
}

/// Convert the high 24 bits to the inclusive unit interval `[0, 1]`.
pub fn inclusive_unit_f32(value: u64) -> f32 {
    (value >> 40) as f32 / INCLUSIVE_UNIT_F32_SCALE
}

/// Convert the high 53 bits to the half-open unit interval `[0, 1)`.
pub fn unit_f64(value: u64) -> f64 {
    (value >> 11) as f64 / UNIT_F64_SCALE
}

/// Convert the high 53 bits to the inclusive unit interval `[0, 1]`.
pub fn inclusive_unit_f64(value: u64) -> f64 {
    (value >> 11) as f64 / INCLUSIVE_UNIT_F64_SCALE
}

/// A deterministic SplitMix64 stream with no ambient or platform RNG state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(SPLITMIX64_INCREMENT);
        mix64(self.state)
    }

    pub fn unit_f32(&mut self) -> f32 {
        unit_f32(self.next_u64())
    }

    pub fn range_f32(&mut self, minimum: f32, maximum: f32) -> f32 {
        minimum + self.unit_f32() * (maximum - minimum)
    }

    pub fn index(&mut self, length: usize) -> usize {
        debug_assert!(length > 0);
        self.next_u64() as usize % length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_seed_zero_sequence_is_stable() {
        let mut random = SplitMix64::new(0);
        assert_eq!(random.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(random.next_u64(), 0x6e78_9e6a_a1b9_65f4);
        assert_eq!(random.next_u64(), 0x06c4_5d18_8009_454f);
    }

    #[test]
    fn unit_conversions_keep_their_interval_contracts() {
        assert_eq!(unit_f32(0), 0.0);
        assert!(unit_f32(u64::MAX) < 1.0);
        assert_eq!(inclusive_unit_f32(u64::MAX), 1.0);
        assert_eq!(unit_f64(0), 0.0);
        assert!(unit_f64(u64::MAX) < 1.0);
        assert_eq!(inclusive_unit_f64(u64::MAX), 1.0);
    }
}
