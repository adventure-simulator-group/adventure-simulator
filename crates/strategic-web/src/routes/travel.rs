//! Strategic travel view models and road-network routing.

use std::collections::{BinaryHeap, HashMap, HashSet};

use adventuresim_core::prelude::{TravelFatigueInputs, party_travel_leg_minutes};
use serde::{Deserialize, Serialize};

use crate::spacetimedb::{
    CharacterAttributes, CharacterLimbs, CharacterStats, Quest, QuestStatus, Settlement, TravelEdge,
};

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
pub struct TravelCampForecast {
    pub fatigue_percent: u8,
    pub camp_stop_minutes: Vec<u64>,
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
    /// Cumulative minutes for server-derived camp forecasts.
    pub camp_stop_minutes: Vec<u64>,
    pub camp_forecasts: Vec<TravelCampForecast>,
    pub quest_in_progress: bool,
    /// This settlement is the next leg on the shortest road route to the
    /// posting settlement of the party's active quest.
    pub active_quest_route: bool,
    pub turn_in_ready: bool,
    /// At least one unaccepted quest is posted at this settlement.
    pub open_quest_available: bool,
}

/// Quest markers shared by settlement and off-road Map views. Available
/// settlements are indexed once so decorating destination lists remains
/// linear even as the quest history grows.
pub(crate) struct QuestMapMarkers<'a> {
    available_settlement_ids: HashSet<&'a str>,
    active_quest: Option<&'a Quest>,
}

impl<'a> QuestMapMarkers<'a> {
    pub(crate) fn new(quests: &'a [Quest], active_quest_id: Option<&str>) -> Self {
        Self {
            available_settlement_ids: quests
                .iter()
                .filter(|quest| quest.status == QuestStatus::Available)
                .map(|quest| quest.settlement_id.as_str())
                .collect(),
            active_quest: active_quest_id
                .and_then(|quest_id| quests.iter().find(|quest| quest.id == quest_id)),
        }
    }

    pub(crate) fn active_quest(&self) -> Option<&'a Quest> {
        self.active_quest
    }

    pub(crate) fn has_open_quest_at(&self, settlement_id: &str) -> bool {
        self.available_settlement_ids.contains(settlement_id)
    }

    pub(crate) fn completed_quest_turn_in_at(&self, settlement_id: &str) -> bool {
        self.active_quest.is_some_and(|quest| {
            quest.status == QuestStatus::Completed && quest.settlement_id == settlement_id
        })
    }

    pub(crate) fn decorate_settlement(&self, destination: &mut TravelDestination) {
        destination.open_quest_available = self.has_open_quest_at(&destination.id);
        destination.turn_in_ready = self.completed_quest_turn_in_at(&destination.id);
    }
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
        camp_stop_minutes: Vec::new(),
        camp_forecasts: Vec::new(),
        quest_in_progress: false,
        active_quest_route: false,
        turn_in_ready: false,
        open_quest_available: false,
    }
}

/// Calculate a camp forecast from the same pure fatigue function used by the
/// strategic reducer. The first leg uses current fatigue; later legs assume
/// the leader takes the recommended full-fatigue camp rest.
pub(crate) fn forecast_camp_stop_minutes(
    party_members: &[u64],
    attributes: &[CharacterAttributes],
    limbs: &[CharacterLimbs],
    stats: &[CharacterStats],
    journey_minutes: u64,
    fatigue_percent: u8,
) -> Option<Vec<u64>> {
    let fatigue_inputs: Vec<_> = party_members
        .iter()
        .map(|character_id| {
            let attributes = attributes
                .iter()
                .find(|row| row.character_id == *character_id)?;
            let limbs = limbs.iter().find(|row| row.character_id == *character_id)?;
            let stats = stats.iter().find(|row| row.character_id == *character_id)?;
            Some(TravelFatigueInputs {
                fatigue_capacity: (attributes.endurance * limbs.chest_health).max(0.01) * 1_000.0,
                calories_used: stats.calories_used,
            })
        })
        .collect::<Option<_>>()?;
    let mut stops = Vec::new();
    let mut elapsed = 0;
    let mut current_inputs = fatigue_inputs;
    while elapsed < journey_minutes {
        let leg = party_travel_leg_minutes(&current_inputs, fatigue_percent)?;
        elapsed = elapsed.saturating_add(leg).min(journey_minutes);
        if elapsed < journey_minutes {
            stops.push(elapsed);
            for member in &mut current_inputs {
                member.calories_used = 0.0;
            }
        }
    }
    Some(stops)
}

pub(crate) fn populate_camp_forecasts(
    destinations: &mut [TravelDestination],
    party_members: &[u64],
    attributes: &[CharacterAttributes],
    limbs: &[CharacterLimbs],
    stats: &[CharacterStats],
    selected_fatigue_percent: u8,
) {
    for destination in destinations {
        destination.camp_forecasts = (10..=100)
            .step_by(5)
            .filter_map(|fatigue_percent| {
                forecast_camp_stop_minutes(
                    party_members,
                    attributes,
                    limbs,
                    stats,
                    destination.journey_minutes,
                    fatigue_percent,
                )
                .map(|camp_stop_minutes| TravelCampForecast {
                    fatigue_percent,
                    camp_stop_minutes,
                })
            })
            .collect();
        destination.camp_stop_minutes = destination
            .camp_forecasts
            .iter()
            .find(|forecast| forecast.fatigue_percent == selected_fatigue_percent)
            .map(|forecast| forecast.camp_stop_minutes.clone())
            .unwrap_or_default();
    }
}

/// Finds the first settlement reached when following the shortest road route
/// from `origin` to `destination_id`.
///
/// The regular destination list intentionally stops at the next settlement,
/// so this traversal must continue through settlement nodes to provide an
/// actionable quest-direction marker for a more distant quest giver.
pub(crate) fn next_settlement_toward(
    origin: &Settlement,
    destination_id: &str,
    settlements: &[Settlement],
    edges: &[TravelEdge],
) -> Option<String> {
    let destination = settlements
        .iter()
        .find(|settlement| settlement.id == destination_id)?;
    let (Some(origin_node), Some(destination_node)) =
        (origin.source_node_id, destination.source_node_id)
    else {
        // Imported worlds without road-node data expose every settlement as a
        // direct destination, so the quest giver is the actionable choice.
        return (origin.id != destination.id).then(|| destination.id.clone());
    };
    if origin_node == destination_node {
        return None;
    }

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
    let mut previous = HashMap::new();
    let mut pending = BinaryHeap::from([std::cmp::Reverse((0_u64, origin_node))]);
    while let Some(std::cmp::Reverse((distance_m, node))) = pending.pop() {
        if distances
            .get(&node)
            .is_some_and(|known| *known != distance_m)
        {
            continue;
        }
        if node == destination_node {
            break;
        }
        for (neighbor, edge_length_m) in adjacency.get(&node).into_iter().flatten() {
            let next_distance = distance_m.saturating_add(u64::from(*edge_length_m));
            if distances
                .get(neighbor)
                .is_none_or(|known| next_distance < *known)
            {
                distances.insert(*neighbor, next_distance);
                previous.insert(*neighbor, node);
                pending.push(std::cmp::Reverse((next_distance, *neighbor)));
            }
        }
    }
    if !distances.contains_key(&destination_node) {
        return None;
    }

    let mut route = vec![destination_node];
    while route.last().copied()? != origin_node {
        route.push(*previous.get(route.last()?)?);
    }
    route.reverse();
    route.into_iter().skip(1).find_map(|node| {
        settlements_by_node
            .get(&node)
            .map(|settlement| settlement.id.clone())
    })
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
    use adventuresim_world_schema::{
        FallbackIndustry, IndustryEvidence, InferredIndustryProfile, LandRoute, RouteTerrain,
        TravelRoute,
    };

    fn settlement(id: &str, node: u64) -> Settlement {
        Settlement {
            id: id.to_string(),
            name: id.to_string(),
            coord_x: 0.0,
            coord_y: 0.0,
            population_level: 0,
            population_estimate: 0,
            category: crate::spacetimedb::SettlementCategory::Unknown,
            industries: InferredIndustryProfile::new(vec![IndustryEvidence::Fallback(
                FallbackIndustry::WoodlandFuelwood,
            )])
            .unwrap(),
            scene_key: String::new(),
            religion_id: String::new(),
            source_node_id: Some(node),
        }
    }

    fn edge(id: u64, from_node_id: u64, to_node_id: u64) -> TravelEdge {
        TravelEdge {
            id,
            from_node_id,
            to_node_id,
            route: TravelRoute::Land(LandRoute {
                bridge: None,
                water_crossings: vec![],
            }),
            length_m: 1_000,
            slope_multiplier: 1.0,
            terrain: RouteTerrain::stage_placeholder(),
            certainty: 100,
            section: String::new(),
        }
    }

    fn quest(id: &str, settlement_id: &str, status: QuestStatus) -> Quest {
        Quest {
            id: id.to_string(),
            title: id.to_string(),
            description: String::new(),
            difficulty: 1,
            gold_reward: 1,
            xp_reward: 1,
            settlement_id: settlement_id.to_string(),
            status,
            accepted_by: None,
            enemy_type: String::new(),
            enemy_count: 1,
            location_description: String::new(),
            location_scene_key: String::new(),
            location_coord_x: 0.0,
            location_coord_y: 0.0,
            coordinates_are_geographic: false,
            distance_m: 1_000,
        }
    }

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

    #[test]
    fn quest_direction_uses_the_first_settlement_on_the_shortest_route() {
        let settlements = vec![
            settlement("origin", 1),
            settlement("next", 2),
            settlement("quest-giver", 3),
            settlement("long-way", 4),
        ];
        let edges = vec![edge(1, 1, 2), edge(2, 2, 3), edge(3, 1, 4), edge(4, 4, 3)];

        assert_eq!(
            next_settlement_toward(&settlements[0], "quest-giver", &settlements, &edges),
            Some("next".to_string())
        );
    }

    #[test]
    fn quest_markers_index_open_settlements_and_completed_active_issuer() {
        let quests = vec![
            quest("open", "market", QuestStatus::Available),
            quest("active", "chapel", QuestStatus::Completed),
            quest("old", "market", QuestStatus::Completed),
        ];
        let markers = QuestMapMarkers::new(&quests, Some("active"));

        assert!(markers.has_open_quest_at("market"));
        assert!(!markers.has_open_quest_at("chapel"));
        assert!(markers.completed_quest_turn_in_at("chapel"));
        assert!(!markers.completed_quest_turn_in_at("market"));
    }
}
