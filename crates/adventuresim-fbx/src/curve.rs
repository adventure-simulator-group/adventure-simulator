/// One animated scalar channel.
#[derive(Debug, Clone, Default)]
pub struct Curve {
    /// Key times in seconds, ascending.
    pub times: Vec<f64>,
    pub values: Vec<f64>,
}

impl Curve {
    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// Samples the curve, holding the end values outside the keyed range.
    ///
    /// FBX keys carry cubic tangents that this ignores: sampling linearly at
    /// the file's own key times reproduces every key exactly, which is what an
    /// importer that re-keys on import needs.
    pub fn sample(&self, time: f64) -> Option<f64> {
        if self.times.is_empty() {
            return None;
        }
        let last = self.times.len() - 1;
        if time <= self.times[0] {
            return Some(self.values[0]);
        }
        if time >= self.times[last] {
            return Some(self.values[last]);
        }
        let upper = self.times.partition_point(|t| *t < time).min(last);
        let lower = upper.saturating_sub(1);
        let span = self.times[upper] - self.times[lower];
        if span <= f64::EPSILON {
            return Some(self.values[lower]);
        }
        let factor = (time - self.times[lower]) / span;
        Some(self.values[lower] + (self.values[upper] - self.values[lower]) * factor)
    }
}
