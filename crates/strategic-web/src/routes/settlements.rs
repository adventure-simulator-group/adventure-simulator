//! Settlement route handlers

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BinaryHeap, HashMap, HashSet};

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterEquip, CharacterLimbs,
    CharacterSkills, CharacterTrainingSchedule, InventoryItem, InventoryQuantityTarget,
    ItemDefinition, Party, PartyInventoryItem, PartyMember, PartyRecruitmentRole, PartyStake,
    Quest, QuestIssuer, RecruitmentRequirements, Settlement, TravelEdge,
};
use crate::templates::settlement::{
    LocationView, MerchantShop, RestSummary, inn_page, live_merchant_shop_page, merchants_page,
    party_discard_page, party_inventory_page, party_personal_page, party_pool_page,
    party_stats_page, religion_page, rest_result_page, settlement_map_page,
    settlement_overview_page, settlements_list_page, smith_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settlements", get(list_settlements))
        .route("/settlements/{id}", get(show_settlement))
        .route("/locations/settlement/{id}", get(show_settlement_location))
        .route("/locations/settlement/{id}/map", get(settlement_map))
        .route(
            "/api/settlements/{id}/service-quests",
            get(service_quest_offers),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}",
            get(party_personal),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory",
            get(party_member),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory/transfer",
            post(transfer_party_item),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/remove",
            post(remove_party_member),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory/offer",
            post(finalize_party_offer),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory/discard",
            post(discard_inventory_items),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory",
            get(party_pool_inventory),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory/deposit",
            post(deposit_party_inventory),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory/withdraw",
            post(withdraw_party_inventory),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory/liquidate",
            post(liquidate_party_assets),
        )
        .route("/api/inventory-target", post(set_inventory_target))
        .route(
            "/locations/{kind}/{id}/party/{character_id}/stats",
            get(party_stats),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/schedule",
            post(update_training_schedule),
        )
        .route("/settlements/{id}/tavern", get(redirect_to_inn))
        .route("/settlements/{id}/merchants", get(merchants))
        .route(
            "/settlements/{id}/merchants/offer",
            post(finalize_merchant_offer),
        )
        .route("/settlements/{id}/weapons", get(weapons))
        .route("/settlements/{id}/armor", get(armor))
        .route("/settlements/{id}/clothing", get(clothing))
        .route("/settlements/{id}/consumables", get(redirect_to_inn))
        .route("/settlements/{id}/smith", get(smith))
        .route("/settlements/{id}/inn", get(inn))
        .route("/settlements/{id}/religion", get(religion))
        .route("/settlements/{id}/rest/{kind}", post(rest))
        .route("/settlements/{id}/travel", post(travel))
}

async fn list_settlements(State(state): State<AppState>, session: Session) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        settlements_list_page(&settlements, logged_in_as.as_deref(), session.theme()).into_string(),
    )
}

async fn show_settlement(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/locations/settlement/{id}"))
}

async fn show_settlement_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        settlement_overview_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Default, Deserialize)]
struct LocationMapQuery {
    destination: Option<String>,
}

async fn settlement_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LocationMapQuery>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let edges: Vec<TravelEdge> = state
        .db
        .query("SELECT * FROM travel_edge")
        .await
        .unwrap_or_default();
    let mut destinations = connected_destinations(settlement, &settlements, &edges);
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let active_party = if let Some(party_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.party_id.as_ref())
    {
        state
            .db
            .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    if let Some(quest_id) = active_party
        .as_ref()
        .and_then(|party| party.active_quest_id.as_ref())
        && let Some(quest) = state
            .db
            .query::<Quest>(&format!("SELECT * FROM quest WHERE id = '{}'", quest_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
        && quest.status.eq_ignore_ascii_case("accepted")
    {
        let distance_m = crate::routes::quests::straight_line_distance_m(&quest, settlement);
        destinations.push(TravelDestination {
            id: quest.id.clone(),
            name: quest.title.clone(),
            description: quest.description.clone(),
            summary: Some(format!(
                "Active quest · {} {}",
                quest.enemy_count, quest.enemy_type
            )),
            travel_action: format!("/quests/{}/travel", quest.id),
            distance_m,
            journey_minutes: crate::routes::quests::offroad_journey_minutes(distance_m),
        });
    }
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let can_travel = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(&settlement.id)
            && active_party
                .as_ref()
                .is_some_and(|party| party.leader_id == character.id)
    });
    Html(
        settlement_map_page(
            settlement,
            &destinations,
            query.destination.as_deref(),
            active_character.as_ref().map(|(character, _)| character),
            &party_members,
            can_travel,
            active_character
                .as_ref()
                .map(|(character, _)| character.name.as_str()),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Serialize)]
struct ServiceQuestOffer {
    id: String,
    title: String,
    service_id: String,
    npc_name: &'static str,
    greeting: String,
    problem: String,
    follow_up: String,
    details: String,
    acceptance: &'static str,
    state: &'static str,
    waiting: &'static str,
    turn_in_response: String,
    can_accept: bool,
    can_turn_in: bool,
    recruitment: Option<ServiceQuestRecruitment>,
}

#[derive(Serialize)]
struct ServiceQuestRecruitment {
    party_name: String,
    leader_name: String,
    roles: Vec<ServiceQuestRole>,
}

#[derive(Serialize)]
struct ServiceQuestRole {
    id: u64,
    name: String,
    remaining: u32,
    requirements: Vec<String>,
    requirements_summary: String,
    match_level: &'static str,
    match_summary: String,
}

async fn service_quest_offers(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<Vec<ServiceQuestOffer>> {
    if state.db.is_local() {
        let _ = state
            .db
            .call("ensure_settlement_activity", &[json!(id.clone())])
            .await;
    }
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Json(Vec::new());
    };
    let issuers: Vec<QuestIssuer> = state
        .db
        .query(&format!(
            "SELECT * FROM quest_issuer WHERE settlement_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let quests: Vec<Quest> = state
        .db
        .query(&format!(
            "SELECT * FROM quest WHERE settlement_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let edges: Vec<TravelEdge> = state
        .db
        .query("SELECT * FROM travel_edge")
        .await
        .unwrap_or_default();
    let neighboring_name = connected_destinations(settlement, &settlements, &edges)
        .first()
        .map(|destination| destination.name.clone())
        .unwrap_or_else(|| "the next settlement".to_string());
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let active_party = if let Some(party_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.party_id.as_ref())
    {
        state
            .db
            .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let can_accept = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(id.as_str())
            && active_party.as_ref().is_some_and(|party| {
                party.leader_id == character.id && party.active_quest_id.is_none()
            })
    });
    let can_turn_in = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(id.as_str())
            && active_party
                .as_ref()
                .is_some_and(|party| party.leader_id == character.id)
    });
    let parties: Vec<Party> = state
        .db
        .query("SELECT * FROM party")
        .await
        .unwrap_or_default();
    let party_memberships: Vec<PartyMember> = state
        .db
        .query("SELECT * FROM party_member")
        .await
        .unwrap_or_default();
    let recruitment_roles: Vec<PartyRecruitmentRole> = state
        .db
        .query("SELECT * FROM party_recruitment_role")
        .await
        .unwrap_or_default();
    let characters: Vec<Character> = state
        .db
        .query("SELECT * FROM character")
        .await
        .unwrap_or_default();
    let viewer_party_id = active_party.as_ref().map(|party| party.id.as_str());
    let viewer_member_ids: Vec<u64> = viewer_party_id
        .map(|party_id| {
            party_memberships
                .iter()
                .filter(|member| member.party_id == party_id)
                .map(|member| member.character_id)
                .collect()
        })
        .unwrap_or_default();
    let mut viewer_capabilities = Vec::new();
    for character_id in viewer_member_ids {
        let _ = state
            .db
            .call("refresh_capabilities", &[json!(character_id)])
            .await;
        if let Some(capability) = state
            .db
            .query::<CharacterCapability>(&format!(
                "SELECT * FROM character_capability WHERE character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
        {
            viewer_capabilities.push(capability);
        }
    }

    Json(
        issuers
            .into_iter()
            .filter_map(|issuer| {
                let quest = quests.iter().find(|quest| quest.id == issuer.quest_id)?;
                let is_current = active_party.as_ref().is_some_and(|party| {
                    party.active_quest_id.as_deref() == Some(quest.id.as_str())
                        && quest.accepted_by.as_deref() == Some(party.id.as_str())
                });
                let recruitment = quest.accepted_by.as_deref().and_then(|party_id| {
                    if viewer_party_id == Some(party_id) {
                        return None;
                    }
                    let party = parties.iter().find(|party| party.id == party_id)?;
                    let leader = characters.iter().find(|character| {
                        character.id == party.leader_id && character.temporary
                    })?;
                    let roles = recruitment_roles
                        .iter()
                        .filter(|role| role.party_id == party.id)
                        .filter_map(|role| {
                            let filled = party_memberships
                                .iter()
                                .filter(|member| member.recruitment_role_id == Some(role.id))
                                .count() as u32;
                            let remaining = role.quantity.saturating_sub(filled);
                            if remaining == 0 {
                                return None;
                            }
                            let requirements = role_requirement_labels(role);
                            let (match_level, match_summary) =
                                party_role_match(&viewer_capabilities, role);
                            Some(ServiceQuestRole {
                                id: role.id,
                                name: role.name.clone(),
                                remaining,
                                requirements_summary: if requirements.is_empty() {
                                    "No minimum recommendations".to_string()
                                } else {
                                    requirements.join(" · ")
                                },
                                requirements,
                                match_level,
                                match_summary,
                            })
                        })
                        .collect::<Vec<_>>();
                    (!roles.is_empty()).then(|| ServiceQuestRecruitment {
                        party_name: party.name.clone(),
                        leader_name: leader.name.clone(),
                        roles,
                    })
                });
                let state = if quest.status.eq_ignore_ascii_case("available") {
                    "available"
                } else if is_current && quest.status.eq_ignore_ascii_case("completed") {
                    "ready"
                } else if is_current {
                    "underway"
                } else if recruitment.is_some() {
                    "recruiting"
                } else {
                    return None;
                };
                let problem = quest.description.trim_end_matches('.').to_lowercase();
                let low = (quest.enemy_count - 2).max(1);
                let high = quest.enemy_count + 2;
                let (npc_name, greeting) = service_quest_greeting(&issuer.service_id);
                Some(ServiceQuestOffer {
                    id: quest.id.clone(),
                    title: quest.title.clone(),
                    service_id: issuer.service_id.clone(),
                    npc_name,
                    greeting: greeting.to_string(),
                    follow_up: format!("{problem}?"),
                    problem,
                    details: service_quest_details(
                        &issuer.service_id,
                        quest,
                        &settlement.name,
                        &neighboring_name,
                        low,
                        high,
                    ),
                    acceptance: "Splendid! And please, do be careful! You wouldn't be the first men they've slain.",
                    state,
                    waiting: "Hello again, I eagerly await the results of your efforts.",
                    turn_in_response: format!(
                        "Excellent work. Here is the promised {} gold. You've earned it.",
                        quest.gold_reward
                    ),
                    can_accept,
                    can_turn_in: can_turn_in && state == "ready",
                    recruitment,
                })
            })
            .collect(),
    )
}

fn service_quest_greeting(service_id: &str) -> (&'static str, &'static str) {
    match service_id {
        "weapons" => (
            "Weaponsmith",
            "Welcome. Business would be better, were it not for how",
        ),
        "armor" => ("Armourer", "Welcome. Production has nearly stopped because"),
        "clothing" => (
            "Clothier",
            "Welcome, traveler. Cloth is scarce of late because",
        ),
        "inn" => (
            "Innkeeper",
            "Welcome. Travelers have been avoiding this road because",
        ),
        "religion" => (
            "Priest",
            "Peace be with you. I fear our prayers alone cannot mend this:",
        ),
        _ => (
            "Merchant",
            "Welcome, traveler. You'll have to excuse the sorry state of my inventory;",
        ),
    }
}

fn service_quest_details(
    service_id: &str,
    quest: &Quest,
    settlement_name: &str,
    neighboring_name: &str,
    low: i32,
    high: i32,
) -> String {
    let situation = match service_id {
        "weapons" => format!(
            "the thieves are hiding with the stolen arms near the road between {settlement_name} and {neighboring_name}"
        ),
        "armor" => format!(
            "the old mine between {settlement_name} and {neighboring_name} is choked with giant spiders, and no miner will go near it"
        ),
        "clothing" => format!(
            "the wolves are ranging through the grazing land between {settlement_name} and {neighboring_name}, where our shepherds cannot avoid them"
        ),
        "inn" => format!(
            "the goblins are lairing in a cave near the road between {settlement_name} and {neighboring_name} and attacking travelers after dark"
        ),
        "religion" => format!(
            "a necromancer has occupied an old crypt outside {settlement_name} and raised its dead"
        ),
        _ => format!(
            "a handful of bandits are camped in the forest near the road between {settlement_name} and {neighboring_name} and have been laying ambushes for my caravans"
        ),
    };
    format!(
        "Yes, {situation}. I believe there are about {low} or {high} {}, give or take. I'd offer {} gold to anyone who clears them out. Are you",
        quest.enemy_type, quest.gold_reward,
    )
}

fn role_requirement_labels(role: &PartyRecruitmentRole) -> Vec<String> {
    let requirements = role.requirements;
    let mut labels = Vec::new();
    for (required, label) in [
        (requirements.melee, "Melee"),
        (requirements.ranged, "Ranged"),
        (requirements.heavy, "Heavy"),
        (requirements.quarter_armor, "1/4 armor"),
        (requirements.half_armor, "1/2 armor"),
        (requirements.three_quarter_armor, "3/4 armor"),
        (requirements.full_armor, "Full armor"),
    ] {
        if required {
            labels.push(label.to_string());
        }
    }
    let precision = role.effective_weapon_precision();
    if precision > 0.0 {
        labels.push(format!("Weapon precision {precision:.1}+"));
    }
    for (minimum, label) in [
        (requirements.athletics, "Athletics"),
        (requirements.endurance, "Endurance"),
    ] {
        if minimum > 0 {
            labels.push(format!("{label} {minimum}+"));
        }
    }
    labels
}

fn party_role_match(
    capabilities: &[CharacterCapability],
    role: &PartyRecruitmentRole,
) -> (&'static str, String) {
    let total = role_requirement_labels(role).len();
    if total == 0 {
        return (
            "none",
            "This role has no minimum recommendations.".to_string(),
        );
    }
    let best = capabilities
        .iter()
        .map(|capability| matched_role_requirements(capability, role))
        .max()
        .unwrap_or(0);
    if best == total {
        (
            "all",
            "Someone in your party meets every recommendation.".to_string(),
        )
    } else if best > 0 {
        (
            "some",
            format!("Your best candidate meets {best} of {total} recommendations."),
        )
    } else {
        (
            "none-met",
            "No one in your party meets any recommendation.".to_string(),
        )
    }
}

fn matched_role_requirements(
    capability: &CharacterCapability,
    role: &PartyRecruitmentRole,
) -> usize {
    let requirements: RecruitmentRequirements = role.requirements;
    let mut matched = 0;
    for (required, present) in [
        (requirements.melee, capability.melee),
        (requirements.ranged, capability.ranged),
        (requirements.heavy, capability.heavy),
        (requirements.quarter_armor, capability.quarter_armor),
        (requirements.half_armor, capability.half_armor),
        (
            requirements.three_quarter_armor,
            capability.three_quarter_armor,
        ),
        (requirements.full_armor, capability.full_armor),
    ] {
        if required && present {
            matched += 1;
        }
    }
    if role.effective_weapon_precision() > 0.0
        && capability.weapon_precision >= role.effective_weapon_precision()
    {
        matched += 1;
    }
    for (minimum, value) in [
        (requirements.athletics, capability.athletics),
        (requirements.endurance, capability.endurance),
    ] {
        if minimum > 0 && adventuresim_core::capability::rating(value) >= minimum {
            matched += 1;
        }
    }
    matched
}

const WALKING_SPEED_KM_PER_HOUR: u64 = 5;

#[derive(Clone)]
pub struct TravelDestination {
    pub id: String,
    pub name: String,
    pub description: String,
    pub summary: Option<String>,
    pub travel_action: String,
    pub distance_m: u64,
    pub journey_minutes: u64,
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
    }
}

fn connected_destinations(
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
    for edge in edges
        .iter()
        .filter(|edge| matches!(edge.kind.as_str(), "land" | "ferry"))
    {
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

async fn redirect_to_inn(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/settlements/{id}/inn"))
}

async fn resolve_location(state: &AppState, kind: &str, id: &str) -> Option<LocationView> {
    let name = match kind {
        "settlement" => state
            .db
            .query::<Settlement>(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
            .map(|settlement| settlement.name),
        "quest" => state
            .db
            .query::<Quest>(&format!("SELECT * FROM quest WHERE id = '{}'", id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
            .map(|quest| quest.title),
        _ => None,
    }?;
    Some(LocationView {
        kind: kind.to_string(),
        id: id.to_string(),
        name,
    })
}

fn character_is_at_location(character: &Character, location: &LocationView) -> bool {
    match location.kind.as_str() {
        "settlement" => character.current_settlement_id.as_deref() == Some(location.id.as_str()),
        "quest" => character.current_quest_location_id.as_deref() == Some(location.id.as_str()),
        _ => false,
    }
}

async fn party_personal(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Html<String> {
    if let Some(character_id) = session.character_id_u64() {
        if let Err(error) = state
            .db
            .call("synchronize_character_time", &[json!(character_id)])
            .await
        {
            tracing::error!("Failed to liquidate party inventory: {error:?}");
        }
    }
    let Some(location) = resolve_location(&state, &kind, &id).await else {
        return Html("<h1>Location not found</h1>".to_string());
    };
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if !character_is_at_location(&active_character, &location) {
        return Html("<h1>Your party is not at this location</h1>".to_string());
    }
    if character_id != active_character.id {
        return Html("<h1>Party member not found</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
    let attributes: Vec<CharacterAttributes> = state
        .db
        .query(&format!(
            "SELECT * FROM character_attributes WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let skills: Vec<CharacterSkills> = state
        .db
        .query(&format!(
            "SELECT * FROM character_skills WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let limbs: Vec<CharacterLimbs> = state
        .db
        .query(&format!(
            "SELECT * FROM character_limbs WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let schedule: Vec<CharacterTrainingSchedule> = state
        .db
        .query(&format!(
            "SELECT * FROM character_training_schedule WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let capability = get_character_capability(&state, character_id).await;
    Html(
        party_personal_page(
            &location,
            &active_character,
            &party_members,
            capability.as_ref(),
            attributes.first(),
            skills.first(),
            limbs.first(),
            schedule.first(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct TrainingScheduleForm {
    melee_minutes: u16,
    dodge_minutes: u16,
    block_minutes: u16,
    ranged_minutes: u16,
    will_minutes: u16,
    charisma_minutes: u16,
    medicine_minutes: u16,
    faith_minutes: u16,
    stealth_minutes: u16,
    balance_minutes: u16,
    surgeon_minutes: u16,
    labor_minutes: u16,
}

async fn update_training_schedule(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<TrainingScheduleForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id) {
        let _ = state
            .db
            .call(
                "update_training_schedule",
                &[
                    json!(character_id),
                    json!(form.melee_minutes),
                    json!(form.dodge_minutes),
                    json!(form.block_minutes),
                    json!(form.ranged_minutes),
                    json!(form.will_minutes),
                    json!(form.charisma_minutes),
                    json!(form.medicine_minutes),
                    json!(form.faith_minutes),
                    json!(form.stealth_minutes),
                    json!(form.balance_minutes),
                    json!(form.surgeon_minutes),
                    json!(form.labor_minutes),
                ],
            )
            .await;
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party/{character_id}"))
}

async fn party_member(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Html<String> {
    let Some(location) = resolve_location(&state, &kind, &id).await else {
        return Html("<h1>Location not found</h1>".to_string());
    };

    let Some((active_character, active_inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if !character_is_at_location(&active_character, &location) {
        return Html("<h1>Your party is not at this location</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
    if character_id != active_character.id
        && !party_members.iter().any(|member| member.id == character_id)
    {
        return Html("<h1>Party member not found</h1>".to_string());
    }

    let selected = if character_id == active_character.id {
        active_character.clone()
    } else {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {character_id}"
            ))
            .await
            .unwrap_or_default();
        match characters.into_iter().next() {
            Some(character) => character,
            None => return Html("<h1>Party member not found</h1>".to_string()),
        }
    };
    let selected_inventory: Vec<InventoryItem> = if character_id == active_character.id {
        active_inventory.clone()
    } else {
        state
            .db
            .query(&format!(
                "SELECT * FROM inventory_item WHERE character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
    };

    let selected_equip: Vec<CharacterEquip> = state
        .db
        .query(&format!(
            "SELECT * FROM character_equip WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let active_equip: Vec<CharacterEquip> = if character_id == active_character.id {
        selected_equip.clone()
    } else {
        state
            .db
            .query(&format!(
                "SELECT * FROM character_equip WHERE character_id = {}",
                active_character.id
            ))
            .await
            .unwrap_or_default()
    };
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let selected_targets = personal_inventory_targets(&state, selected.id).await;
    let active_targets = personal_inventory_targets(&state, active_character.id).await;

    if character_id == active_character.id {
        return Html(
            party_discard_page(
                &location,
                &active_character,
                &active_inventory,
                &items,
                &party_members,
                active_equip.first(),
                session.theme(),
            )
            .into_string(),
        );
    }

    Html(
        party_inventory_page(
            &location,
            &selected,
            &selected_inventory,
            &active_character,
            &active_inventory,
            &items,
            &party_members,
            selected_equip.first(),
            active_equip.first(),
            &selected_targets,
            &active_targets,
            session.theme(),
        )
        .into_string(),
    )
}

async fn party_pool_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
) -> Html<String> {
    let Some(location) = resolve_location(&state, &kind, &id).await else {
        return Html("<h1>Location not found</h1>".into());
    };
    let Some((character, inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    if !character_is_at_location(&character, &location) {
        return Html("<h1>Your party is not at this location</h1>".into());
    }
    let Some(party_id) = character.party_id.as_ref() else {
        return Html("<h1>Character has no party</h1>".into());
    };
    let pooled: Vec<PartyInventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let stakes: Vec<PartyStake> = state
        .db
        .query(&format!(
            "SELECT * FROM party_stake WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let equip: Vec<CharacterEquip> = state
        .db
        .query(&format!(
            "SELECT * FROM character_equip WHERE character_id = {}",
            character.id
        ))
        .await
        .unwrap_or_default();
    let members = get_active_party_members(&state, Some(&character)).await;
    let stake = stakes
        .iter()
        .find(|stake| stake.character_id == character.id)
        .map_or(0, |stake| stake.value);
    let (personal_targets, party_targets, _) = inventory_trade_context(&state, &character).await;
    Html(
        party_pool_page(
            &location,
            &character,
            &inventory,
            &pooled,
            stake,
            &items,
            &members,
            equip.first(),
            &personal_targets,
            &party_targets,
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct PartyPoolTransferForm {
    item_id: String,
    #[serde(default)]
    quantity: String,
}

#[derive(Deserialize)]
struct InventoryTargetForm {
    item_id: String,
    quantity: u32,
    #[serde(default)]
    party_scope: bool,
}

async fn set_inventory_target(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<InventoryTargetForm>,
) -> impl IntoResponse {
    let Some(character_id) = session.character_id_u64() else {
        return (axum::http::StatusCode::UNAUTHORIZED, "Choose a character");
    };
    match state
        .db
        .call(
            "set_inventory_quantity_target",
            &[
                json!(character_id),
                json!(form.party_scope),
                json!(form.item_id),
                json!(form.quantity),
            ],
        )
        .await
    {
        Ok(()) => (axum::http::StatusCode::NO_CONTENT, ""),
        Err(error) => {
            tracing::warn!("Failed to save inventory target: {error}");
            (axum::http::StatusCode::BAD_REQUEST, "Could not save target")
        }
    }
}

async fn deposit_party_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64() {
        for (item_id, quantity) in transfer_entries(&form) {
            let _ = state
                .db
                .call(
                    "deposit_party_inventory_item",
                    &[json!(character_id), json!(item_id), json!(quantity)],
                )
                .await;
        }
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party-inventory"))
}

async fn withdraw_party_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64() {
        for (item_id, quantity) in transfer_entries(&form) {
            let _ = state
                .db
                .call(
                    "withdraw_party_inventory_item",
                    &[json!(character_id), json!(item_id), json!(quantity)],
                )
                .await;
        }
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party-inventory"))
}

fn transfer_entries(form: &PartyPoolTransferForm) -> Vec<(u64, u32)> {
    form.item_id
        .split(',')
        .zip(form.quantity.split(','))
        .filter_map(|(id, quantity)| Some((id.parse().ok()?, quantity.parse().ok()?)))
        .collect()
}

async fn liquidate_party_assets(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if kind == "settlement"
        && let Some(character_id) = session.character_id_u64()
    {
        let entries = transfer_entries(&form);
        let _ = state
            .db
            .call(
                "liquidate_party_inventory",
                &[
                    json!(character_id),
                    json!(id.clone()),
                    json!(entries.iter().map(|entry| entry.0).collect::<Vec<_>>()),
                    json!(entries.iter().map(|entry| entry.1).collect::<Vec<_>>()),
                ],
            )
            .await;
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party-inventory"))
}

async fn remove_party_member(
    State(state): State<AppState>,
    Path((kind, id, member_character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Redirect {
    if let Some(actor_character_id) = session.character_id_u64() {
        if let Err(error) = state
            .db
            .call(
                "remove_party_member",
                &[json!(actor_character_id), json!(member_character_id)],
            )
            .await
        {
            tracing::error!("Failed to remove party member: {error:?}");
        }
    }
    Redirect::to(&format!("/locations/{kind}/{id}"))
}

async fn party_stats(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Html<String> {
    let Some(location) = resolve_location(&state, &kind, &id).await else {
        return Html("<h1>Location not found</h1>".to_string());
    };
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if !character_is_at_location(&active_character, &location) {
        return Html("<h1>Your party is not at this location</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
    if character_id != active_character.id
        && !party_members.iter().any(|member| member.id == character_id)
    {
        return Html("<h1>Party member not found</h1>".to_string());
    }
    let selected = if character_id == active_character.id {
        active_character.clone()
    } else {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {character_id}"
            ))
            .await
            .unwrap_or_default();
        match characters.into_iter().next() {
            Some(character) => character,
            None => return Html("<h1>Party member not found</h1>".to_string()),
        }
    };
    let selected_attributes: Vec<CharacterAttributes> = state
        .db
        .query(&format!(
            "SELECT * FROM character_attributes WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let selected_skills: Vec<CharacterSkills> = state
        .db
        .query(&format!(
            "SELECT * FROM character_skills WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let selected_limbs: Vec<CharacterLimbs> = state
        .db
        .query(&format!(
            "SELECT * FROM character_limbs WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let capability = get_character_capability(&state, character_id).await;
    Html(
        party_stats_page(
            &location,
            &selected,
            &active_character,
            &party_members,
            capability.as_ref(),
            selected_attributes.first(),
            selected_skills.first(),
            selected_limbs.first(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct PartyTransferForm {
    from_character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
}

#[derive(Deserialize)]
struct PartyOfferForm {
    from_character_ids: String,
    to_character_ids: String,
    inventory_item_ids: String,
    quantities: String,
}

#[derive(Deserialize)]
struct DiscardInventoryForm {
    inventory_item_ids: String,
    quantities: String,
}

async fn discard_inventory_items(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<DiscardInventoryForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id) {
        let item_ids = form
            .inventory_item_ids
            .split(',')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>();
        let quantities = form
            .quantities
            .split(',')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>();
        if let (Ok(item_ids), Ok(quantities)) = (item_ids, quantities) {
            if let Err(error) = state
                .db
                .call(
                    "discard_inventory_items",
                    &[json!(character_id), json!(item_ids), json!(quantities)],
                )
                .await
            {
                tracing::warn!("Inventory discard failed: {error}");
            }
        }
    }
    Redirect::to(&format!(
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    ))
}

async fn finalize_party_offer(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<PartyOfferForm>,
) -> Redirect {
    if let Some((active, _)) = get_active_character(&state, session.character_id_u64()).await {
        let parse = |value: &str| {
            value
                .split(',')
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
        };
        let quantities = form
            .quantities
            .split(',')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>();
        if let (Ok(from_ids), Ok(to_ids), Ok(item_ids), Ok(quantities)) = (
            parse(&form.from_character_ids),
            parse(&form.to_character_ids),
            parse(&form.inventory_item_ids),
            quantities,
        ) {
            if from_ids
                .iter()
                .all(|id| *id == active.id || *id == character_id)
                && to_ids
                    .iter()
                    .all(|id| *id == active.id || *id == character_id)
            {
                let _ = state
                    .db
                    .call(
                        "finalize_party_offer",
                        &[
                            json!(from_ids),
                            json!(to_ids),
                            json!(item_ids),
                            json!(quantities),
                        ],
                    )
                    .await;
            }
        }
    }
    Redirect::to(&format!(
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    ))
}

async fn transfer_party_item(
    State(state): State<AppState>,
    Path((kind, id, recipient_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<PartyTransferForm>,
) -> Redirect {
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    if form.from_character_id != active_character.id && recipient_id != active_character.id {
        return Redirect::to(&format!("/locations/{kind}/{id}"));
    }
    let to_character_id = if form.from_character_id == active_character.id {
        recipient_id
    } else {
        active_character.id
    };
    if let Err(error) = state
        .db
        .call(
            "transfer_party_item",
            &[
                json!(form.from_character_id),
                json!(to_character_id),
                json!(form.inventory_item_id),
                json!(form.quantity),
            ],
        )
        .await
    {
        tracing::warn!("Party item transfer failed: {error}");
    }
    let comparison_character_id = if form.from_character_id == active_character.id {
        recipient_id
    } else {
        form.from_character_id
    };
    Redirect::to(&format!(
        "/locations/{kind}/{id}/party/{comparison_character_id}/inventory"
    ))
}

async fn merchants(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let Some((character, inventory)) = active_character.as_ref() else {
        return Html(
            merchants_page(
                settlement,
                None,
                &[],
                &party_members,
                logged_in_as.as_deref(),
                session.theme(),
            )
            .into_string(),
        );
    };
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let equip: Vec<CharacterEquip> = state
        .db
        .query(&format!(
            "SELECT * FROM character_equip WHERE character_id = {}",
            character.id
        ))
        .await
        .unwrap_or_default();
    let (personal_targets, party_targets, pooled) =
        inventory_trade_context(&state, character).await;
    Html(
        live_merchant_shop_page(
            settlement,
            character,
            inventory,
            &items,
            &party_members,
            equip.first(),
            &personal_targets,
            &party_targets,
            &pooled,
            session.theme(),
            MerchantShop::General,
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct MerchantOfferForm {
    buy_item_ids: String,
    buy_quantities: String,
    #[serde(default)]
    sell_inventory_ids: String,
    #[serde(default)]
    sell_quantities: String,
    #[serde(default)]
    return_to: String,
    #[serde(default)]
    inventory_scope: String,
}

async fn finalize_merchant_offer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<MerchantOfferForm>,
) -> Redirect {
    if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await {
        let items = form
            .buy_item_ids
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let quantities = form
            .buy_quantities
            .split(',')
            .filter_map(|value| value.parse::<u32>().ok())
            .collect::<Vec<_>>();
        if items.len() == quantities.len() {
            let sell_ids = form
                .sell_inventory_ids
                .split(',')
                .filter_map(|value| value.parse::<u64>().ok())
                .collect::<Vec<_>>();
            let sell_quantities = form
                .sell_quantities
                .split(',')
                .filter_map(|value| value.parse::<u32>().ok())
                .collect::<Vec<_>>();
            if !items.is_empty() || !sell_ids.is_empty() {
                let _ = state
                    .db
                    .call(
                        "finalize_merchant_trade",
                        &[
                            json!(character.id),
                            json!(id),
                            json!(items),
                            json!(quantities),
                            json!(sell_ids),
                            json!(sell_quantities),
                            json!(form.inventory_scope == "party"),
                        ],
                    )
                    .await;
            }
        }
    }
    let return_to = match form.return_to.as_str() {
        "weapons" | "armor" | "clothing" | "merchants" => form.return_to,
        _ => "merchants".to_owned(),
    };
    Redirect::to(&format!("/settlements/{id}/{return_to}"))
}

async fn smith(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        smith_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn inn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let limbs = match active_character.as_ref() {
        Some((character, _)) => {
            query_single::<CharacterLimbs>(&state, "character_limbs", character.id).await
        }
        None => None,
    };
    Html(
        inn_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
            limbs.as_ref(),
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct RestForm {
    days: u16,
}

async fn rest(
    State(state): State<AppState>,
    Path((id, kind)): Path<(String, String)>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Html<String> {
    let at_inn = match kind.as_str() {
        "inn" => true,
        "temple" => false,
        _ => return Html("<h1>Rest service not found</h1>".to_string()),
    };
    let Some(character_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    let before_character = get_active_character(&state, Some(character_id)).await;
    let before_limbs =
        query_single::<CharacterLimbs>(&state, "character_limbs", character_id).await;
    let before_skills =
        query_single::<CharacterSkills>(&state, "character_skills", character_id).await;
    let before_time =
        query_single::<crate::spacetimedb::CharacterTime>(&state, "character_time", character_id)
            .await;
    if let Err(error) = state
        .db
        .call(
            "rest_at_settlement",
            &[json!(character_id), json!(form.days.max(1)), json!(at_inn)],
        )
        .await
    {
        return Html(format!("<h1>Unable to rest</h1><p>{error}</p>"));
    }

    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let active_character = get_active_character(&state, Some(character_id)).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let after_limbs = query_single::<CharacterLimbs>(&state, "character_limbs", character_id).await;
    let after_skills =
        query_single::<CharacterSkills>(&state, "character_skills", character_id).await;
    let after_time =
        query_single::<crate::spacetimedb::CharacterTime>(&state, "character_time", character_id)
            .await;
    let summary = rest_summary(
        before_character.as_ref().map(|(character, _)| character),
        active_character.as_ref().map(|(character, _)| character),
        before_limbs.as_ref(),
        after_limbs.as_ref(),
        before_skills.as_ref(),
        after_skills.as_ref(),
        before_time.as_ref(),
        after_time.as_ref(),
    );
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        rest_result_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
            at_inn,
            &summary,
        )
        .into_string(),
    )
}

async fn query_single<T: serde::de::DeserializeOwned>(
    state: &AppState,
    table: &str,
    character_id: u64,
) -> Option<T> {
    state
        .db
        .query(&format!(
            "SELECT * FROM {table} WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
}

fn rest_summary(
    before_character: Option<&Character>,
    after_character: Option<&Character>,
    before_limbs: Option<&CharacterLimbs>,
    after_limbs: Option<&CharacterLimbs>,
    before_skills: Option<&CharacterSkills>,
    after_skills: Option<&CharacterSkills>,
    before_time: Option<&crate::spacetimedb::CharacterTime>,
    after_time: Option<&crate::spacetimedb::CharacterTime>,
) -> RestSummary {
    let days = before_time.zip(after_time).map_or(0, |(before, after)| {
        after.minutes.saturating_sub(before.minutes) / 1_440
    });
    let gold_spent = before_character
        .zip(after_character)
        .map_or(0, |(before, after)| before.gold.saturating_sub(after.gold));
    let healed = match (before_limbs, after_limbs) {
        (Some(before), Some(after)) => limb_deltas(before, after),
        _ => vec![],
    };
    let trained = match (before_skills, after_skills) {
        (Some(before), Some(after)) => skill_deltas(before, after),
        _ => vec![],
    };
    RestSummary {
        days,
        gold_spent,
        healed,
        trained,
    }
}

fn limb_deltas(before: &CharacterLimbs, after: &CharacterLimbs) -> Vec<(String, f32)> {
    [
        ("Left arm", before.left_arm_health, after.left_arm_health),
        ("Right arm", before.right_arm_health, after.right_arm_health),
        ("Left leg", before.left_leg_health, after.left_leg_health),
        ("Right leg", before.right_leg_health, after.right_leg_health),
        ("Head", before.head_health, after.head_health),
        ("Chest", before.chest_health, after.chest_health),
        ("Stomach", before.stomach_health, after.stomach_health),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| {
        let delta = (after - before) * 100.0;
        (delta > 0.01).then(|| (name.to_string(), delta))
    })
    .collect()
}

fn skill_deltas(before: &CharacterSkills, after: &CharacterSkills) -> Vec<(String, f32)> {
    [
        ("Melee", before.melee_hours, after.melee_hours),
        ("Dodge", before.dodge_hours, after.dodge_hours),
        ("Block", before.block_hours, after.block_hours),
        ("Ranged", before.ranged_hours, after.ranged_hours),
        ("Will", before.will_hours, after.will_hours),
        ("Charisma", before.charisma_hours, after.charisma_hours),
        ("Medicine", before.medicine_hours, after.medicine_hours),
        ("Faith", before.faith_hours, after.faith_hours),
        ("Stealth", before.stealth_hours, after.stealth_hours),
        ("Balance", before.balance_hours, after.balance_hours),
        ("Surgeon", before.surgeon_hours, after.surgeon_hours),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| {
        let delta = after - before;
        (delta > 0.001).then(|| (name.to_string(), delta))
    })
    .collect()
}

async fn travel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let _ = state
        .db
        .call(
            "travel_to_settlement",
            &[json!(character_id), json!(id.clone())],
        )
        .await;

    Redirect::to(&format!("/locations/settlement/{}", id))
}

/// Helper to get character name for session display
async fn get_character_name(state: &AppState, character_id: Option<&str>) -> Option<String> {
    let Some(id) = character_id else {
        return None;
    };
    let characters: Vec<Character> = state
        .db
        .query(&format!("SELECT * FROM character WHERE id = {}", id))
        .await
        .unwrap_or_default();
    characters.first().map(|c| c.name.clone())
}

async fn weapons(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Weapons).await
}

async fn armor(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Armor).await
}

async fn clothing(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Clothing).await
}

async fn religion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    render_service_page(state, id, session, religion_page).await
}

type ServiceRenderer = fn(
    &Settlement,
    Option<&Character>,
    &[InventoryItem],
    &[Character],
    Option<&CharacterLimbs>,
    Option<&str>,
    &str,
) -> maud::Markup;

async fn merchant_shop(
    state: AppState,
    id: String,
    session: Session,
    shop: MerchantShop,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let Some((character, inventory)) = active_character.as_ref() else {
        return Html(
            merchants_page(
                settlement,
                None,
                &[],
                &party_members,
                logged_in_as.as_deref(),
                session.theme(),
            )
            .into_string(),
        );
    };
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let equip: Vec<CharacterEquip> = state
        .db
        .query(&format!(
            "SELECT * FROM character_equip WHERE character_id = {}",
            character.id
        ))
        .await
        .unwrap_or_default();
    let (personal_targets, party_targets, pooled) =
        inventory_trade_context(&state, character).await;
    Html(
        live_merchant_shop_page(
            settlement,
            character,
            inventory,
            &items,
            &party_members,
            equip.first(),
            &personal_targets,
            &party_targets,
            &pooled,
            session.theme(),
            shop,
        )
        .into_string(),
    )
}

async fn inventory_trade_context(
    state: &AppState,
    character: &Character,
) -> (
    Vec<InventoryQuantityTarget>,
    Vec<InventoryQuantityTarget>,
    Vec<PartyInventoryItem>,
) {
    let personal = state.db.query(&format!(
        "SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = false",
        character.id
    )).await.unwrap_or_default();
    let Some(party_id) = character.party_id.as_ref() else {
        return (personal, Vec::new(), Vec::new());
    };
    let party = state
        .db
        .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(party) = party else {
        return (personal, Vec::new(), Vec::new());
    };
    let party_targets = state.db.query(&format!(
        "SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = true",
        party.leader_id
    )).await.unwrap_or_default();
    let pooled = state
        .db
        .query(&format!(
            "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    (personal, party_targets, pooled)
}

async fn personal_inventory_targets(
    state: &AppState,
    character_id: u64,
) -> Vec<InventoryQuantityTarget> {
    state.db.query(&format!("SELECT * FROM inventory_quantity_target WHERE owner_character_id = {character_id} AND party_scope = false")).await.unwrap_or_default()
}

async fn render_service_page(
    state: AppState,
    id: String,
    session: Session,
    render: ServiceRenderer,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let settlement = match settlements.first() {
        Some(settlement) => settlement,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());

    let inventory = active_character
        .as_ref()
        .map_or_else(Vec::new, |(_, inventory)| inventory.clone());
    let limbs = match active_character.as_ref() {
        Some((character, _)) => {
            query_single::<CharacterLimbs>(&state, "character_limbs", character.id).await
        }
        None => None,
    };

    Html(
        render(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            &inventory,
            &party_members,
            limbs.as_ref(),
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn get_active_character(
    state: &AppState,
    character_id: Option<u64>,
) -> Option<(Character, Vec<InventoryItem>)> {
    let character_id = character_id?;
    let characters: Vec<Character> = state
        .db
        .query(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let character = characters.into_iter().next()?;
    let inventory: Vec<InventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    Some((character, inventory))
}

async fn get_character_capability(
    state: &AppState,
    character_id: u64,
) -> Option<CharacterCapability> {
    let _ = state
        .db
        .call("refresh_capabilities", &[json!(character_id)])
        .await;
    state
        .db
        .query(&format!(
            "SELECT * FROM character_capability WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
}

pub(crate) async fn get_active_party_members(
    state: &AppState,
    active_character: Option<&Character>,
) -> Vec<Character> {
    let Some(party_id) = active_character.and_then(|character| character.party_id.as_ref()) else {
        return Vec::new();
    };
    let memberships: Vec<PartyMember> = state
        .db
        .query(&format!(
            "SELECT * FROM party_member WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();

    let leader_id = state
        .db
        .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default()
        .first()
        .map(|party| party.leader_id);
    let mut members = Vec::new();
    for membership in memberships {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {}",
                membership.character_id
            ))
            .await
            .unwrap_or_default();
        if let Some(character) = characters.into_iter().next() {
            members.push(character);
        }
    }
    members.sort_by_key(|member| (Some(member.id) != leader_id, member.id));
    members
}
