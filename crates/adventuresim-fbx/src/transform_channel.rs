use crate::Curve;

/// One of a node's transform properties, as three channels plus the static
/// values to use where a channel has no curve.
#[derive(Debug, Clone, Default)]
pub struct TransformChannel {
    pub x: Curve,
    pub y: Curve,
    pub z: Curve,
    /// The curve node's own `d|X`, `d|Y`, `d|Z` values.
    pub default: [f64; 3],
}

impl TransformChannel {
    pub fn is_empty(&self) -> bool {
        self.x.is_empty() && self.y.is_empty() && self.z.is_empty()
    }

    /// The value at a time, falling back per axis to the static default.
    pub fn sample(&self, time: f64) -> [f64; 3] {
        [
            self.x.sample(time).unwrap_or(self.default[0]),
            self.y.sample(time).unwrap_or(self.default[1]),
            self.z.sample(time).unwrap_or(self.default[2]),
        ]
    }

    /// Every key time in the channel, sorted and deduplicated.
    pub fn key_times(&self, into: &mut Vec<f64>) {
        into.extend_from_slice(&self.x.times);
        into.extend_from_slice(&self.y.times);
        into.extend_from_slice(&self.z.times);
    }
}
