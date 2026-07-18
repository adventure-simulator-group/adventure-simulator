//! Stable random primitives. Algorithm changes require a format-version bump.

/// SplitMix64, chosen because its complete algorithm is tiny and stable across platforms.
#[derive(Clone, Copy, Debug)]
pub struct StableRng(u64);

impl StableRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    pub fn unit(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1_u32 << 24) as f32)
    }
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.unit() * (max - min)
    }
}

/// Derive independent streams without relying on collection or call order.
pub fn sub_seed(root: u64, domain: u64, index: u64) -> u64 {
    let mut rng =
        StableRng::new(root ^ domain.rotate_left(17) ^ index.wrapping_mul(0xd6e8feb86659fd93));
    rng.next_u64()
}
