use crate::NodeAnimation;

/// One `AnimationStack` — what other tools call a take or a clip.
#[derive(Debug, Clone)]
pub struct Take {
    pub name: String,
    /// Seconds, from the stack's declared local time span when it has one and
    /// from the keys otherwise.
    pub duration: f64,
    pub nodes: Vec<NodeAnimation>,
}

impl Take {
    /// Every distinct key time in the take, sorted.
    pub fn key_times(&self) -> Vec<f64> {
        let mut times = Vec::new();
        for node in &self.nodes {
            node.key_times(&mut times);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        times.dedup_by(|a, b| (*a - *b).abs() <= 1.0e-9);
        if times.is_empty() {
            times.push(0.0);
        }
        times
    }
}
