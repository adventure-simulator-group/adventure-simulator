//! Deterministic weighted relations for persistent settlement residents.
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgeBand {
    Child,
    Adolescent,
    Adult,
    Elder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresenceBridge {
    NearbyHome,
    Errand,
    Concealment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeightedCandidate<T> {
    pub value: T,
    /// World plausibility. Zero is impossible.
    pub plausibility: u32,
    /// Editorial representation pressure, kept separate from plausibility.
    pub curation: u32,
    pub bridge: Option<PresenceBridge>,
}

pub fn choose<T: Copy>(
    seed: &str,
    candidates: &[WeightedCandidate<T>],
) -> Option<WeightedCandidate<T>> {
    let valid: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|c| c.plausibility > 0 && c.curation > 0)
        .collect();
    let total: u64 = valid
        .iter()
        .map(|c| u64::from(c.plausibility) * u64::from(c.curation))
        .sum();
    if total == 0 {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let mut draw = hasher.finish() % total;
    for candidate in valid {
        let weight = u64::from(candidate.plausibility) * u64::from(candidate.curation);
        if draw < weight {
            return Some(candidate);
        }
        draw -= weight;
    }
    None
}

/// Generic fixture for a venue whose primary activity is adult-only. A young
/// witness may be nearby only when the world supplies a causal bridge.
pub fn adult_venue_witness_candidates(has_nearby_homes: bool) -> Vec<WeightedCandidate<AgeBand>> {
    vec![
        WeightedCandidate {
            value: AgeBand::Adult,
            plausibility: 80,
            curation: 10,
            bridge: None,
        },
        WeightedCandidate {
            value: AgeBand::Elder,
            plausibility: 15,
            curation: 10,
            bridge: None,
        },
        WeightedCandidate {
            value: AgeBand::Adolescent,
            plausibility: if has_nearby_homes { 3 } else { 0 },
            curation: 10,
            bridge: has_nearby_homes.then_some(PresenceBridge::Errand),
        },
        WeightedCandidate {
            value: AgeBand::Child,
            plausibility: if has_nearby_homes { 1 } else { 0 },
            curation: 10,
            bridge: has_nearby_homes.then_some(PresenceBridge::NearbyHome),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_weight_is_impossible_and_sampling_is_deterministic() {
        let candidates = adult_venue_witness_candidates(false);
        for index in 0..500 {
            let selected = choose(&format!("case-{index}"), &candidates).unwrap();
            assert!(matches!(selected.value, AgeBand::Adult | AgeBand::Elder));
        }
        assert_eq!(choose("same", &candidates), choose("same", &candidates));
    }

    #[test]
    fn rare_child_presence_requires_a_bridge() {
        let candidates = adult_venue_witness_candidates(true);
        let child = candidates
            .iter()
            .find(|c| c.value == AgeBand::Child)
            .unwrap();
        let adult = candidates
            .iter()
            .find(|c| c.value == AgeBand::Adult)
            .unwrap();
        assert!(child.plausibility < adult.plausibility);
        assert!(child.bridge.is_some());
        assert!(
            candidates
                .iter()
                .filter(|c| matches!(c.value, AgeBand::Child | AgeBand::Adolescent))
                .all(|c| c.plausibility == 0 || c.bridge.is_some())
        );
    }

    #[test]
    fn curation_does_not_mutate_plausibility() {
        let mut candidates = adult_venue_witness_candidates(true);
        let before = candidates[0].plausibility;
        candidates[0].curation = 1;
        assert_eq!(candidates[0].plausibility, before);
    }
}
