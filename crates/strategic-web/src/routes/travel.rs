//! Strategic travel view models and road-network routing.

use std::{
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use adventuresim_core::{
    bestiary::ThreatId,
    strategic_schedule::DailySchedule,
    strategic_time::{CampDurationPolicy, ItineraryMember, ItinerarySegment, forecast_itinerary},
};
use serde::Deserialize;

use crate::spacetimedb::{
    CampDurationMode, CharacterAttributes, CharacterLimbs, CharacterStats, CharacterTime,
    CharacterTrainingSchedule, Party, Quest, ScheduleAllocation, Settlement, TravelEdge,
};

const WALKING_SPEED_KM_PER_HOUR: u64 = 5;
const TERRAIN_PLAN_TIMEOUT: Duration = Duration::from_secs(10);
const TERRAIN_PLAN_CACHE_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TerrainPlanKey {
    coordinates: [i32; 4],
    profile: adventuresim_terrain::TerrainSkillProfile,
}

impl TerrainPlanKey {
    fn new(
        start: (f64, f64),
        goal: (f64, f64),
        profile: adventuresim_terrain::TerrainSkillProfile,
    ) -> Self {
        Self {
            coordinates: [
                (start.0 * 100_000.0).round() as i32,
                (start.1 * 100_000.0).round() as i32,
                (goal.0 * 100_000.0).round() as i32,
                (goal.1 * 100_000.0).round() as i32,
            ],
            profile,
        }
    }
}

#[derive(Default)]
struct TerrainPlanCache {
    plans: HashMap<TerrainPlanKey, adventuresim_terrain::RoutePlan>,
    order: VecDeque<TerrainPlanKey>,
}

/// Bounded async facade around the CPU-heavy, synchronous terrain planner.
/// At most two plans run concurrently and successful normalized routes are
/// cached for the life of the immutable terrain package.
pub struct TerrainPlanner {
    pack: Arc<adventuresim_terrain::TerrainPack>,
    permits: Arc<tokio::sync::Semaphore>,
    cache: Mutex<TerrainPlanCache>,
}

impl TerrainPlanner {
    pub fn new(pack: Arc<adventuresim_terrain::TerrainPack>) -> Self {
        Self {
            pack,
            permits: Arc::new(tokio::sync::Semaphore::new(2)),
            cache: Mutex::new(TerrainPlanCache::default()),
        }
    }

    pub fn digest(&self) -> &str {
        self.pack.digest()
    }

    pub async fn plan_with_profile(
        &self,
        start: (f64, f64),
        goal: (f64, f64),
        profile: adventuresim_terrain::TerrainSkillProfile,
    ) -> Result<adventuresim_terrain::RoutePlan, String> {
        let key = TerrainPlanKey::new(start, goal, profile);
        if let Some(plan) = self
            .cache
            .lock()
            .map_err(|_| "terrain route cache poisoned")?
            .plans
            .get(&key)
            .cloned()
        {
            return Ok(plan);
        }
        let permit =
            tokio::time::timeout(TERRAIN_PLAN_TIMEOUT, self.permits.clone().acquire_owned())
                .await
                .map_err(|_| "terrain route planning queue timed out")?
                .map_err(|_| "terrain planner is shutting down")?;
        let pack = Arc::clone(&self.pack);
        let deadline = Instant::now() + TERRAIN_PLAN_TIMEOUT;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            pack.plan_until_with_profile(start, goal, profile, deadline)
        });
        let plan = tokio::time::timeout(TERRAIN_PLAN_TIMEOUT + Duration::from_secs(1), task)
            .await
            .map_err(|_| "terrain route planning timed out")?
            .map_err(|error| format!("terrain route worker failed: {error}"))?
            .map_err(|error| error.to_string())?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "terrain route cache poisoned")?;
        if !cache.plans.contains_key(&key) {
            if cache.plans.len() == TERRAIN_PLAN_CACHE_ENTRIES
                && let Some(oldest) = cache.order.pop_front()
            {
                cache.plans.remove(&oldest);
            }
            cache.order.push_back(key);
            cache.plans.insert(key, plan.clone());
        }
        Ok(plan)
    }
}

pub(crate) fn active_quest_summary(quest: &Quest) -> String {
    let name = quest
        .enemy_type
        .parse::<ThreatId>()
        .map(|id| id.display_name(quest.enemy_count.max(0) as u32))
        .unwrap_or_else(|_| "Unknown threat".to_string());
    format!("Active quest · {} {name}", quest.enemy_count)
}

pub(crate) fn active_quest_tooltip(quest: &Quest) -> String {
    format!("{}\n{}", quest.description, active_quest_summary(quest))
}

#[derive(Debug, Default, Deserialize)]
pub struct TravelForm {}

#[derive(Clone, Debug, PartialEq)]
pub struct TravelProvisionForecast {
    pub planning_minutes: u64,
    pub living_members: u32,
    pub food_days: f32,
    pub water_days: f32,
    pub ordinary_water_days: f32,
    pub emergency_alcohol_days: f32,
    pub emergency_alcohol_hydration_ml: u32,
    pub food_reserve_kcal: f32,
    pub water_reserve_ml: f32,
    pub ration_count: u32,
    pub waterskin_count: u32,
    pub ration_kcal: f32,
    pub waterskin_capacity_ml: u32,
    pub rations_to_buy: u32,
    pub waterskins_to_buy: u32,
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
    pub departure_minute: u64,
    pub itinerary_total_elapsed_minutes: u64,
    pub itinerary_segments: Vec<ItinerarySegment>,
    pub quest_in_progress: bool,
    pub provision_forecast: Option<TravelProvisionForecast>,
    pub terrain_route: Option<adventuresim_terrain::RoutePlan>,
    pub return_terrain_route: Option<adventuresim_terrain::RoutePlan>,
    pub route_fallback: bool,
}

/// Active accepted destination lookup. Conventional available, issuer-route,
/// and turn-in quest markers are intentionally absent.
pub(crate) struct QuestMapMarkers<'a> {
    active_quest: Option<&'a Quest>,
}

impl<'a> QuestMapMarkers<'a> {
    pub(crate) fn new(quests: &'a [Quest], active_quest_id: Option<&str>) -> Self {
        Self {
            active_quest: active_quest_id
                .and_then(|quest_id| quests.iter().find(|quest| quest.id == quest_id)),
        }
    }

    pub(crate) fn active_quest(&self) -> Option<&'a Quest> {
        self.active_quest
    }
}

impl TravelDestination {
    pub fn forecast_minutes(&self) -> u64 {
        if self.quest_in_progress {
            self.journey_minutes.saturating_add(
                self.return_terrain_route
                    .as_ref()
                    .map_or(self.journey_minutes, |route| route.minutes),
            )
        } else {
            self.journey_minutes
        }
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
        departure_minute: 0,
        itinerary_total_elapsed_minutes: journey_minutes,
        itinerary_segments: Vec::new(),
        quest_in_progress: false,
        provision_forecast: None,
        terrain_route: None,
        return_terrain_route: None,
        route_fallback: true,
    }
}

pub(crate) async fn apply_terrain_route(
    destination: &mut TravelDestination,
    terrain: Option<&TerrainPlanner>,
    start: (f64, f64),
    goal: (f64, f64),
    profile: adventuresim_terrain::TerrainSkillProfile,
) {
    let Some(terrain) = terrain else {
        destination.route_fallback = true;
        return;
    };
    match terrain.plan_with_profile(start, goal, profile).await {
        Ok(plan) => {
            let return_plan = if destination.quest_in_progress {
                match terrain.plan_with_profile(goal, start, profile).await {
                    Ok(plan) => Some(plan),
                    Err(error) => {
                        tracing::warn!(%error, destination=%destination.id, "return terrain route unavailable; using explicitly marked legacy estimate");
                        destination.route_fallback = true;
                        return;
                    }
                }
            } else {
                None
            };
            destination.distance_m = plan.distance_m;
            destination.journey_minutes = plan.minutes;
            destination.itinerary_total_elapsed_minutes = if destination.quest_in_progress {
                plan.minutes.saturating_add(
                    return_plan
                        .as_ref()
                        .map_or(plan.minutes, |route| route.minutes),
                )
            } else {
                plan.minutes
            };
            destination.terrain_route = Some(plan);
            destination.return_terrain_route = return_plan;
            destination.route_fallback = false;
        }
        Err(error) => {
            tracing::warn!(%error, destination=%destination.id, "bounded terrain route unavailable; using explicitly marked legacy estimate");
            destination.route_fallback = true;
        }
    }
}

/// Calculate a camp forecast from the same pure fatigue function used by the
/// strategic reducer. The first leg uses current fatigue; later legs assume
/// the leader takes the recommended full-fatigue camp rest.
fn camp_schedule(allocation: &ScheduleAllocation) -> DailySchedule {
    DailySchedule {
        combat_training_minutes: allocation.combat_training_minutes,
        carousing_minutes: allocation.carousing_minutes,
        apprenticeship_minutes: allocation.apprenticeship_minutes,
        apprenticeship_service_id: allocation
            .apprenticeship_service_id
            .as_deref()
            .and_then(adventuresim_core::profession::ProfessionId::from_service_id),
        profession_practice_minutes: allocation.profession_practice_minutes,
        profession_service_id: allocation
            .profession_service_id
            .as_deref()
            .and_then(adventuresim_core::profession::ProfessionId::from_service_id),
        labor: 0,
        prayer: allocation.prayer_minutes,
        thievery: 0,
        raiding: 0,
    }
}

pub(crate) fn populate_itinerary_forecasts(
    destinations: &mut [TravelDestination],
    party_members: &[u64],
    attributes: &[CharacterAttributes],
    limbs: &[CharacterLimbs],
    stats: &[CharacterStats],
    times: &[CharacterTime],
    schedules: &[CharacterTrainingSchedule],
    party: &Party,
) {
    let members: Option<Vec<_>> = party_members
        .iter()
        .map(|id| {
            let attributes = attributes.iter().find(|row| row.character_id == *id)?;
            let limbs = limbs.iter().find(|row| row.character_id == *id)?;
            let stats = stats.iter().find(|row| row.character_id == *id)?;
            let schedule = schedules.iter().find(|row| row.character_id == *id)?;
            Some(ItineraryMember {
                fatigue_capacity: (attributes.endurance * limbs.chest_health).max(0.01) * 1_000.0,
                calories_used: stats.calories_used,
                camp_schedule: camp_schedule(&schedule.downtime),
            })
        })
        .collect();
    let Some(members) = members else {
        return;
    };
    let departure = party_members
        .iter()
        .filter_map(|id| times.iter().find(|row| row.character_id == *id))
        .map(|row| row.minutes)
        .max()
        .unwrap_or(0);
    let policy = match party.camp_duration_mode {
        CampDurationMode::Auto => CampDurationPolicy::Auto,
        CampDurationMode::Fixed => CampDurationPolicy::FixedMinutes(party.fixed_camp_minutes),
    };
    for destination in destinations {
        if let Some(forecast) = forecast_itinerary(
            departure,
            destination.forecast_minutes(),
            party.walking_minutes_per_day,
            party.travel_at_night,
            policy,
            &members,
        ) {
            destination.departure_minute = departure;
            destination.itinerary_total_elapsed_minutes = forecast.total_elapsed_minutes;
            destination.camp_stop_minutes = forecast
                .segments
                .iter()
                .filter(|segment| {
                    matches!(
                        segment.kind,
                        adventuresim_core::strategic_time::ItinerarySegmentKind::Camp
                    )
                })
                .map(|segment| segment.movement_start)
                .collect();
            destination.itinerary_segments = forecast.segments;
        }
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
    use crate::spacetimedb::QuestStatus;
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
            languages: adventuresim_world_schema::SettlementLanguageProfile {
                east_central_bp: 10_000,
                west_central_bp: 0,
                low_bp: 0,
                yiddish_incidence_bp: 75,
            },
            industries: InferredIndustryProfile::new(vec![IndustryEvidence::Fallback(
                FallbackIndustry::WoodlandFuelwood,
            )])
            .unwrap(),
            economy: adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder(),
            religious_status: adventuresim_world_schema::SettlementReligiousStatus::Established {
                religion: adventuresim_world_schema::OfficialReligion::RomanCatholic,
            },
            scene_key: String::new(),
            religion_id: String::new(),
            currency_id: "rhenish_gulden".into(),
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
    fn active_quest_tooltip_includes_encounter_summary() {
        let mut quest = quest("crypt", "riverdale", QuestStatus::Accepted);
        quest.description = "A necromancer has raised the dead.".into();
        quest.enemy_count = 11;
        quest.enemy_type = "skeleton".into();

        assert_eq!(
            active_quest_tooltip(&quest),
            "A necromancer has raised the dead.\nActive quest · 11 Skeletons"
        );
    }

    #[test]
    fn travel_form_has_no_provisioning_choice() {
        assert!(serde_json::from_str::<TravelForm>(r#"{}"#).is_ok());
    }

    #[test]
    fn quests_forecast_a_return_but_settlements_do_not() {
        let mut destination = settlement_destination(settlement("town", 1), 1_000, 120);
        assert_eq!(destination.forecast_minutes(), 120);
        destination.quest_in_progress = true;
        assert_eq!(destination.forecast_minutes(), 240);
    }

    #[test]
    fn terrain_cache_keys_normalize_sub_metre_coordinate_noise() {
        assert_eq!(
            TerrainPlanKey::new(
                (53.500_000_1, 10.000_000_1),
                (53.6, 10.1),
                Default::default()
            ),
            TerrainPlanKey::new(
                (53.500_000_2, 10.000_000_2),
                (53.6, 10.1),
                Default::default()
            )
        );
        assert_ne!(
            TerrainPlanKey::new((53.500_02, 10.0), (53.6, 10.1), Default::default()),
            TerrainPlanKey::new((53.500_04, 10.0), (53.6, 10.1), Default::default())
        );
        assert_ne!(
            TerrainPlanKey::new((53.5, 10.0), (53.6, 10.1), Default::default()),
            TerrainPlanKey::new(
                (53.5, 10.0),
                (53.6, 10.1),
                adventuresim_terrain::TerrainSkillProfile {
                    forest: 1_000,
                    ..Default::default()
                },
            )
        );
    }

    #[test]
    fn quest_marker_lookup_only_retains_the_selected_active_destination() {
        let quests = vec![
            quest("open", "market", QuestStatus::Available),
            quest("active", "chapel", QuestStatus::Completed),
            quest("old", "market", QuestStatus::Completed),
        ];
        let markers = QuestMapMarkers::new(&quests, Some("active"));

        assert_eq!(
            markers.active_quest().map(|quest| quest.id.as_str()),
            Some("active")
        );
    }
}
