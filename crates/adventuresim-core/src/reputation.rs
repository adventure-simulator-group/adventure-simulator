//! Bounded, settlement-scoped public reputation math.
//!
//! Reputation uses integer centipoints. An action is the only propagation
//! source: imported contributions are terminal and must never become events.

use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
};

pub const REPUTATION_SCALE: i32 = 100;
pub const REPUTATION_CAP: i32 = 100_000;
pub const MAX_SPILL_DESTINATIONS: usize = 24;
pub const MAX_SPILL_GRAPH_NODES: usize = 4_096;
pub const SPILL_BUDGET_BPS: i64 = 3_000;
const MIN_REACH_METERS: u64 = 20_000;
const MAX_REACH_METERS: u64 = 250_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationSettlement {
    pub id: String,
    pub node_id: Option<u64>,
    pub population_level: i32,
    pub population_estimate: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReputationEdge {
    pub from: u64,
    pub to: u64,
    pub length_m: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationContribution {
    pub settlement_id: String,
    pub fame: i32,
    pub infamy: i32,
    pub distance_m: u64,
}

pub fn effective_population(population_level: i32, population_estimate: u32) -> u32 {
    if population_estimate > 0 {
        population_estimate
    } else {
        match population_level {
            i32::MIN..=1 => 1_000,
            2 => 3_000,
            3 => 6_000,
            4 => 10_000,
            _ => 16_000,
        }
    }
}

pub fn local_multiplier_bps(population_level: i32, population_estimate: u32) -> i64 {
    let population = f64::from(effective_population(population_level, population_estimate));
    (f64::from(BASIS_POINTS_PER_WHOLE) / (1.0 + (population / 1_000.0).ln() * 0.6))
        .round()
        .clamp(2_000.0, f64::from(BASIS_POINTS_PER_WHOLE)) as i64
}

pub fn reach_meters(population_level: i32, population_estimate: u32) -> u64 {
    let population = f64::from(effective_population(population_level, population_estimate));
    (MIN_REACH_METERS as f64 + population.ln_1p() * 18_000.0)
        .round()
        .clamp(MIN_REACH_METERS as f64, MAX_REACH_METERS as f64) as u64
}

pub fn clamp_reputation(value: i64) -> i32 {
    value.clamp(0, i64::from(REPUTATION_CAP)) as i32
}

pub fn apply_delta(current: i32, delta: i32) -> i32 {
    clamp_reputation(i64::from(current) + i64::from(delta))
}

pub fn local_delta(raw: i32, population_level: i32, population_estimate: u32) -> i32 {
    clamp_reputation(
        i64::from(raw.max(0))
            .saturating_mul(local_multiplier_bps(population_level, population_estimate))
            / i64::from(BASIS_POINTS_PER_WHOLE),
    )
}

/// Compute origin and terminal spill contributions for one action event. The
/// normalized spill budget cannot grow when more destinations are reachable.
pub fn contributions(
    origin_id: &str,
    fame: i32,
    infamy: i32,
    settlements: &[ReputationSettlement],
    edges: &[ReputationEdge],
) -> Vec<ReputationContribution> {
    let Some(origin) = settlements.iter().find(|value| value.id == origin_id) else {
        return Vec::new();
    };
    let local_fame = local_delta(fame, origin.population_level, origin.population_estimate);
    let local_infamy = local_delta(infamy, origin.population_level, origin.population_estimate);
    let mut result = vec![ReputationContribution {
        settlement_id: origin.id.clone(),
        fame: local_fame,
        infamy: local_infamy,
        distance_m: 0,
    }];
    let Some(origin_node) = origin.node_id else {
        return result;
    };
    let reach = reach_meters(origin.population_level, origin.population_estimate);
    let mut adjacency: BTreeMap<u64, Vec<(u64, u32)>> = BTreeMap::new();
    for edge in edges {
        adjacency
            .entry(edge.from)
            .or_default()
            .push((edge.to, edge.length_m.max(1)));
        adjacency
            .entry(edge.to)
            .or_default()
            .push((edge.from, edge.length_m.max(1)));
    }
    for values in adjacency.values_mut() {
        values.sort_unstable();
    }
    let mut distances = BTreeMap::from([(origin_node, 0_u64)]);
    let mut queue = BinaryHeap::from([Reverse((0_u64, origin_node))]);
    while let Some(Reverse((distance, node))) = queue.pop() {
        if distance > reach || distances.get(&node).copied() != Some(distance) {
            continue;
        }
        for &(next, length) in adjacency.get(&node).into_iter().flatten() {
            let candidate = distance.saturating_add(u64::from(length));
            if candidate > reach {
                continue;
            }
            match distances.get(&next).copied() {
                Some(existing) if candidate < existing => {
                    distances.insert(next, candidate);
                    queue.push(Reverse((candidate, next)));
                }
                None if distances.len() < MAX_SPILL_GRAPH_NODES => {
                    distances.insert(next, candidate);
                    queue.push(Reverse((candidate, next)));
                }
                _ => {}
            }
        }
    }
    let mut destinations: Vec<_> = settlements
        .iter()
        .filter(|settlement| settlement.id != origin_id)
        .filter_map(|settlement| {
            let distance = distances.get(&settlement.node_id?)?;
            (*distance <= reach).then_some((settlement.id.clone(), *distance))
        })
        .collect();
    destinations.sort_by(|left, right| (left.1, &left.0).cmp(&(right.1, &right.0)));
    destinations.truncate(MAX_SPILL_DESTINATIONS);
    if destinations.is_empty() {
        return result;
    }
    let weights: Vec<i64> = destinations
        .iter()
        .map(|(_, distance)| ((reach + 1).saturating_sub(*distance)).max(1) as i64)
        .collect();
    let total_weight = weights.iter().sum::<i64>().max(1);
    let fame_budget = i64::from(local_fame) * SPILL_BUDGET_BPS / i64::from(BASIS_POINTS_PER_WHOLE);
    let infamy_budget =
        i64::from(local_infamy) * SPILL_BUDGET_BPS / i64::from(BASIS_POINTS_PER_WHOLE);
    result.extend(destinations.into_iter().zip(weights).filter_map(
        |((settlement_id, distance_m), weight)| {
            let fame = clamp_reputation(fame_budget.saturating_mul(weight) / total_weight);
            let infamy = clamp_reputation(infamy_budget.saturating_mul(weight) / total_weight);
            (fame > 0 || infamy > 0).then_some(ReputationContribution {
                settlement_id,
                fame,
                infamy,
                distance_m,
            })
        },
    ));
    result
}

pub fn npc_reaction_modifier(fame: i32, infamy: i32, familiarity_bps: u16) -> i16 {
    let net = i64::from(fame) - i64::from(infamy);
    let unfamiliarity = i64::from(BASIS_POINTS_PER_WHOLE)
        .saturating_sub(i64::from(familiarity_bps.min(BASIS_POINTS_PER_WHOLE)));
    (net.saturating_mul(unfamiliarity)
        / i64::from(REPUTATION_SCALE)
        / i64::from(BASIS_POINTS_PER_WHOLE))
    .clamp(-20, 20) as i16
}

/// Fine only snapshotted, still-unsettled offenses. Empty charge sets cannot
/// produce a punitive payment.
pub fn authority_fine(severities: &[u8]) -> Option<u64> {
    (!severities.is_empty()).then(|| {
        severities
            .iter()
            .map(|severity| u64::from((*severity).clamp(1, 5)) * 5)
            .sum::<u64>()
            .min(100)
    })
}

pub fn authority_fine_for_charges(charges: &[(u8, bool)]) -> Option<u64> {
    authority_fine(
        &charges
            .iter()
            .filter_map(|(severity, settled)| (!settled).then_some(*severity))
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settlement(id: &str, node: u64, population: u32) -> ReputationSettlement {
        ReputationSettlement {
            id: id.into(),
            node_id: Some(node),
            population_level: 1,
            population_estimate: population,
        }
    }

    #[test]
    fn larger_origins_dilute_locally_but_reach_farther() {
        assert!(local_delta(1_000, 1, 1_000) > local_delta(1_000, 5, 50_000));
        assert!(reach_meters(5, 50_000) > reach_meters(1, 1_000));
    }

    #[test]
    fn missing_estimate_has_explicit_level_fallback() {
        assert_eq!(effective_population(1, 0), 1_000);
        assert_eq!(effective_population(5, 0), 16_000);
        assert!(local_multiplier_bps(1, 0) > local_multiplier_bps(5, 0));
    }

    #[test]
    fn spill_budget_is_conserved_and_cycles_do_not_amplify() {
        let settlements = [
            settlement("a", 1, 20_000),
            settlement("b", 2, 1_000),
            settlement("c", 3, 1_000),
        ];
        let edges = [
            ReputationEdge {
                from: 1,
                to: 2,
                length_m: 1_000,
            },
            ReputationEdge {
                from: 2,
                to: 3,
                length_m: 1_000,
            },
            ReputationEdge {
                from: 3,
                to: 1,
                length_m: 1_000,
            },
        ];
        let values = contributions("a", 10_000, 0, &settlements, &edges);
        let local = values[0].fame;
        let spill: i32 = values.iter().skip(1).map(|value| value.fame).sum();
        assert!(spill <= (i64::from(local) * SPILL_BUDGET_BPS / 10_000) as i32);
        assert_eq!(values.len(), 3);
        assert_eq!(values, contributions("a", 10_000, 0, &settlements, &edges));
    }

    #[test]
    fn caps_and_inverse_familiarity_are_bounded() {
        assert_eq!(apply_delta(REPUTATION_CAP - 1, i32::MAX), REPUTATION_CAP);
        assert_eq!(apply_delta(1, -100), 0);
        assert_eq!(npc_reaction_modifier(100_000, 0, 0), 20);
        assert_eq!(npc_reaction_modifier(100_000, 0, 10_000), 0);
    }

    #[test]
    fn high_degree_graph_refuses_unseen_nodes_at_the_hard_bound() {
        let mut edges = Vec::new();
        for node in 2..=(MAX_SPILL_GRAPH_NODES as u64 + 500) {
            edges.push(ReputationEdge {
                from: 1,
                to: node,
                length_m: 1_000,
            });
        }
        let mut settlements = vec![settlement("origin", 1, 50_000)];
        for node in 2..=30 {
            settlements.push(settlement(&format!("near-{node}"), node, 1_000));
        }
        settlements.push(settlement(
            "beyond-bound",
            MAX_SPILL_GRAPH_NODES as u64 + 400,
            1_000,
        ));
        let values = contributions("origin", 10_000, 0, &settlements, &edges);
        assert!(values.len() <= 1 + MAX_SPILL_DESTINATIONS);
        assert!(
            !values
                .iter()
                .any(|value| value.settlement_id == "beyond-bound")
        );
        let local = values[0].fame;
        let spill: i32 = values.iter().skip(1).map(|value| value.fame).sum();
        assert!(spill <= (i64::from(local) * SPILL_BUDGET_BPS / 10_000) as i32);
    }

    #[test]
    fn settled_charges_cannot_be_fined_twice_but_new_charges_can() {
        let mut charges = vec![(1, false), (3, false)];
        assert_eq!(authority_fine_for_charges(&charges), Some(20));
        for (_, settled) in &mut charges {
            *settled = true;
        }
        assert_eq!(authority_fine_for_charges(&charges), None);
        charges.push((2, false));
        assert_eq!(authority_fine_for_charges(&charges), Some(10));
        assert_eq!(authority_fine(&[5; 30]), Some(100));
    }
}
