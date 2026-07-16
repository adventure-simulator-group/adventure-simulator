//! Strategic travel view models and road-network routing.

use std::collections::{BinaryHeap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::spacetimedb::{Settlement, TravelEdge};

const WALKING_SPEED_KM_PER_HOUR: u64 = 5;

/// Explicit travel-provision choice parsed at the HTTP boundary and persisted
/// in party-action requests.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvisioningChoice {
    Provision,
    Underprovisioned,
}

impl ProvisioningChoice {
    pub fn should_provision(self) -> bool {
        matches!(self, Self::Provision)
    }
}

#[derive(Debug, Deserialize)]
pub struct TravelForm {
    pub provisioning: ProvisioningChoice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TravelerProvisionForecast {
    pub name: String,
    pub rations_to_buy: u32,
    pub waterskins_to_buy: u32,
    pub cost: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TravelProvisionForecast {
    pub travelers: Vec<TravelerProvisionForecast>,
    pub total_cost: u32,
}

#[derive(Clone)]
pub struct TravelDestination {
    pub id: String,
    pub name: String,
    pub description: String,
    pub summary: Option<String>,
    pub travel_action: String,
    pub distance_m: u64,
    pub journey_minutes: u64,
    pub quest_in_progress: bool,
    pub turn_in_ready: bool,
}

pub(crate) fn settlement_destination(
    settlement: Settlement,
    distance_m: u64,
    journey_minutes: u64,
) -> TravelDestination {
    let summary = (settlement.population_estimate > 0).then(|| {
        format!(
            "Population approximately {}",
            settlement.population_estimate
        )
    });
    TravelDestination {
        id: settlement.id.clone(),
        name: settlement.name.clone(),
        description: crate::templates::settlement::settlement_description(
            settlement.population_level,
        )
        .to_string(),
        summary,
        travel_action: format!("/settlements/{}/travel", settlement.id),
        distance_m,
        journey_minutes,
        quest_in_progress: false,
        turn_in_ready: false,
    }
}

pub(crate) fn connected_destinations(
    origin: &Settlement,
    settlements: &[Settlement],
    edges: &[TravelEdge],
) -> Vec<TravelDestination> {
    let Some(origin_node) = origin.source_node_id else {
        return settlements
            .iter()
            .filter(|settlement| settlement.id != origin.id)
            .cloned()
            .map(|settlement| {
                let distance_km = ((origin.coord_x - settlement.coord_x).powi(2)
                    + (origin.coord_y - settlement.coord_y).powi(2))
                .sqrt()
                .ceil() as u64;
                let distance_m = distance_km.saturating_mul(1_000);
                settlement_destination(settlement, distance_m, journey_minutes(distance_m))
            })
            .collect();
    };
    let settlement_nodes: HashSet<u64> = settlements
        .iter()
        .filter_map(|settlement| settlement.source_node_id)
        .collect();
    let settlements_by_node: HashMap<u64, &Settlement> = settlements
        .iter()
        .filter_map(|settlement| settlement.source_node_id.map(|node| (node, settlement)))
        .collect();
    let mut adjacency: HashMap<u64, Vec<(u64, u32)>> = HashMap::new();
    for edge in edges {
        adjacency
            .entry(edge.from_node_id)
            .or_default()
            .push((edge.to_node_id, edge.length_m));
        adjacency
            .entry(edge.to_node_id)
            .or_default()
            .push((edge.from_node_id, edge.length_m));
    }
    let mut distances = HashMap::from([(origin_node, 0_u64)]);
    let mut pending = BinaryHeap::from([std::cmp::Reverse((0_u64, origin_node))]);
    let mut destinations = Vec::new();
    while let Some(std::cmp::Reverse((distance_m, node))) = pending.pop() {
        if distances
            .get(&node)
            .is_some_and(|known| *known != distance_m)
        {
            continue;
        }
        if node != origin_node && settlement_nodes.contains(&node) {
            if let Some(settlement) = settlements_by_node.get(&node) {
                destinations.push(settlement_destination(
                    (*settlement).clone(),
                    distance_m,
                    journey_minutes(distance_m),
                ));
            }
            continue;
        }
        for (neighbor, edge_length_m) in adjacency.get(&node).into_iter().flatten() {
            let next_distance = distance_m.saturating_add(u64::from(*edge_length_m));
            if distances
                .get(neighbor)
                .is_none_or(|known| next_distance < *known)
            {
                distances.insert(*neighbor, next_distance);
                pending.push(std::cmp::Reverse((next_distance, *neighbor)));
            }
        }
    }
    destinations.sort_by_key(|destination| destination.distance_m);
    destinations
}

fn journey_minutes(distance_m: u64) -> u64 {
    distance_m
        .saturating_mul(60)
        .div_ceil(WALKING_SPEED_KM_PER_HOUR * 1_000)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walking_time_rounds_up_to_a_minute() {
        assert_eq!(journey_minutes(1), 1);
        assert_eq!(journey_minutes(5_000), 60);
    }

    #[test]
    fn travel_form_requires_an_explicit_provisioning_choice() {
        assert!(serde_json::from_str::<TravelForm>(r#"{}"#).is_err());
        let form: TravelForm =
            serde_json::from_str(r#"{"provisioning":"underprovisioned"}"#).unwrap();
        assert_eq!(form.provisioning, ProvisioningChoice::Underprovisioned);
    }
}
