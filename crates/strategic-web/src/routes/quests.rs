//! Quest route handlers

use axum::{
    Json, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Serialize;
use serde_json::json;

use super::{AppState, settlements::get_active_party_members};
use crate::session::Session;
use crate::spacetimedb::{
    BattleLootItem, BattleResult, Character, ItemDefinition, Party, PartyInventoryItem, PartyStake,
    Quest, Settlement,
};
use crate::templates::quest::{
    post_battle_page, quest_detail_page, quest_location_page, quests_list_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/quests", get(list_quests))
        .route("/api/current-quest", get(current_quest))
        .route("/quests/{id}", get(show_quest))
        .route("/quests/{id}/accept", post(accept_quest))
        .route("/quests/{id}/abandon", post(abandon_quest))
        .route("/quests/{id}/travel", post(travel_to_quest))
        .route("/quests/{id}/location", get(quest_location))
        .route("/quests/{id}/autoresolve", post(autoresolve_quest))
        .route("/quests/{id}/loot", get(post_battle_loot))
        .route("/quests/{id}/loot/store", post(store_battle_loot))
}

#[derive(Serialize)]
struct CurrentQuestSummary {
    id: String,
    title: String,
    can_abandon: bool,
}

async fn current_quest(
    State(state): State<AppState>,
    session: Session,
) -> Json<Option<CurrentQuestSummary>> {
    let Some(character_id) = session.character_id_u64() else {
        return Json(None);
    };
    let character = state
        .db
        .query::<Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(character) = character else {
        return Json(None);
    };
    let Some(party_id) = character.party_id.as_ref() else {
        return Json(None);
    };
    let party = state
        .db
        .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(party) = party else {
        return Json(None);
    };
    let Some(active_quest_id) = party.active_quest_id.as_ref() else {
        return Json(None);
    };
    let quest = state
        .db
        .query::<Quest>(&format!(
            "SELECT * FROM quest WHERE id = '{}'",
            active_quest_id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    Json(quest.map(|quest| CurrentQuestSummary {
        id: quest.id,
        title: quest.title,
        can_abandon: party.leader_id == character.id
            && character.current_quest_location_id.is_none(),
    }))
}

async fn list_quests(State(state): State<AppState>, session: Session) -> Response {
    if let Some(character_id) = session.character_id_u64() {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {character_id}"
            ))
            .await
            .unwrap_or_default();
        if let Some(character) = characters.first() {
            if let Some(quest_id) = &character.current_quest_location_id {
                return Redirect::to(&format!("/quests/{quest_id}/location")).into_response();
            }
            if let Some(settlement_id) = &character.current_settlement_id {
                return Redirect::to(&format!("/settlements/{settlement_id}/noticeboard"))
                    .into_response();
            }
        }
    }
    let quests: Vec<Quest> = state
        .db
        .query("SELECT * FROM quest")
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(quests_list_page(&quests, logged_in_as.as_deref(), session.theme()).into_string())
        .into_response()
}

async fn show_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let quest = match quests.first() {
        Some(q) => q,
        None => return Html("<h1>Quest not found</h1>".to_string()),
    };

    // Check if user can accept: must be party leader at quest's settlement
    let mut can_accept = false;
    let mut is_party_quest = false;

    if let Some(character_id) = session.character_id_u64() {
        // Get character
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {}",
                character_id
            ))
            .await
            .unwrap_or_default();

        if let Some(character) = characters.first() {
            // Check if at quest's settlement
            let at_settlement =
                character.current_settlement_id.as_ref() == Some(&quest.settlement_id);

            // Check if party leader
            if let Some(party_id) = &character.party_id {
                let parties: Vec<Party> = state
                    .db
                    .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
                    .await
                    .unwrap_or_default();

                if let Some(party) = parties.first() {
                    let is_leader = party.leader_id == character_id;
                    can_accept = is_available_status(&quest.status) && at_settlement && is_leader;
                    is_party_quest = quest.accepted_by.as_ref() == Some(party_id);
                }
            }
        }
    }

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        quest_detail_page(
            quest,
            can_accept,
            is_party_quest,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
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

fn is_available_status(status: &str) -> bool {
    status.to_ascii_lowercase().contains("available")
}

async fn accept_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let settlement_id = quests.first().map(|quest| quest.settlement_id.clone());
    let _ = state
        .db
        .call("accept_quest", &[json!(character_id), json!(id.clone())])
        .await;

    settlement_id.map_or_else(
        || Redirect::to("/quests"),
        |settlement_id| Redirect::to(&format!("/settlements/{settlement_id}/noticeboard")),
    )
}

async fn abandon_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let settlement_id = quests.first().map(|quest| quest.settlement_id.clone());
    let _ = state
        .db
        .call("abandon_quest", &[json!(character_id), json!(id.clone())])
        .await;

    settlement_id.map_or_else(
        || Redirect::to("/quests"),
        |settlement_id| Redirect::to(&format!("/settlements/{settlement_id}/noticeboard")),
    )
}

async fn travel_to_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call("travel_to_quest", &[json!(character_id), json!(id.clone())])
        .await
    {
        tracing::error!("Failed to travel to quest: {error:?}");
        return Redirect::to(&format!("/quests/{id}"));
    }
    Redirect::to(&format!("/quests/{id}/location"))
}

async fn post_battle_loot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let Some(character_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    let characters: Vec<Character> = state
        .db
        .query(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let Some(character) = characters.first() else {
        return Html("<h1>Character not found</h1>".to_string());
    };
    let results: Vec<BattleResult> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_result WHERE quest_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let Some(result) = results.first() else {
        return Html("<h1>Battle result not found</h1>".to_string());
    };
    if character.party_id.as_deref() != Some(&result.party_id) {
        return Html("<h1>This battle belongs to another party</h1>".to_string());
    }
    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(quest) = quests.first() else {
        return Html("<h1>Quest not found</h1>".to_string());
    };
    let loot: Vec<BattleLootItem> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_loot_item WHERE quest_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let pooled: Vec<PartyInventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
            result.party_id
        ))
        .await
        .unwrap_or_default();
    let stakes: Vec<PartyStake> = state
        .db
        .query(&format!(
            "SELECT * FROM party_stake WHERE party_id = '{}'",
            result.party_id
        ))
        .await
        .unwrap_or_default();
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let stake = stakes
        .iter()
        .find(|stake| stake.character_id == character.id)
        .map_or(0, |stake| stake.value);
    Html(
        post_battle_page(
            quest,
            character,
            &loot,
            &pooled,
            stake,
            &items,
            session.theme(),
        )
        .into_string(),
    )
}

async fn store_battle_loot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call(
            "store_battle_loot",
            &[json!(character_id), json!(id.clone())],
        )
        .await
    {
        tracing::error!("Failed to store battle loot: {error:?}");
    }
    Redirect::to(&format!("/quests/{id}/location"))
}

#[derive(Debug, Clone)]
pub struct NearbySettlement {
    pub settlement: Settlement,
    pub distance_m: u64,
}

async fn quest_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(quest) = quests.first() else {
        return Html("<h1>Quest location not found</h1>".to_string());
    };
    let character = match session.character_id_u64() {
        Some(character_id) => {
            let characters: Vec<Character> = state
                .db
                .query(&format!(
                    "SELECT * FROM character WHERE id = {character_id}"
                ))
                .await
                .unwrap_or_default();
            characters.into_iter().next()
        }
        None => None,
    };
    let party = if let Some(party_id) = character.as_ref().and_then(|c| c.party_id.as_ref()) {
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
    let is_at_location = character
        .as_ref()
        .is_some_and(|c| c.current_quest_location_id.as_deref() == Some(&quest.id));
    if !is_at_location {
        return Html("<h1>Your party is not at this quest location</h1>".to_string());
    }
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let mut nearby: Vec<NearbySettlement> = settlements
        .into_iter()
        .map(|settlement| {
            let distance_m = straight_line_distance_m(quest, &settlement);
            NearbySettlement {
                settlement,
                distance_m,
            }
        })
        .collect();
    nearby.sort_by_key(|destination| destination.distance_m);
    nearby.truncate(5);
    let can_control = character
        .as_ref()
        .zip(party.as_ref())
        .is_some_and(|(character, party)| party.leader_id == character.id);
    let can_fight = can_control
        && party
            .as_ref()
            .is_some_and(|party| party.active_quest_id.as_deref() == Some(&quest.id));
    let results: Vec<BattleResult> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_result WHERE quest_id = '{}'",
            quest.id
        ))
        .await
        .unwrap_or_default();
    let resolved = party
        .as_ref()
        .is_some_and(|party| results.iter().any(|result| result.party_id == party.id));
    let loot: Vec<BattleLootItem> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_loot_item WHERE quest_id = '{}'",
            quest.id
        ))
        .await
        .unwrap_or_default();
    let pooled: Vec<PartyInventoryItem> = if let Some(party) = party.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
                party.id
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let stakes: Vec<PartyStake> = if let Some(party) = party.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM party_stake WHERE party_id = '{}'",
                party.id
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let stake = character.as_ref().map_or(0, |character| {
        stakes
            .iter()
            .find(|stake| stake.character_id == character.id)
            .map_or(0, |stake| stake.value)
    });
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let party_members = get_active_party_members(&state, character.as_ref()).await;
    Html(
        quest_location_page(
            quest,
            &nearby,
            character.as_ref(),
            &party_members,
            can_control,
            can_fight,
            resolved,
            &loot,
            &pooled,
            stake,
            &items,
            character.as_ref().map(|c| c.name.as_str()),
            session.theme(),
        )
        .into_string(),
    )
}

async fn autoresolve_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call(
            "autoresolve_quest",
            &[json!(character_id), json!(id.clone())],
        )
        .await
    {
        tracing::error!("Failed to autoresolve quest: {error:?}");
    }
    Redirect::to(&format!("/quests/{id}/location"))
}

fn straight_line_distance_m(quest: &Quest, settlement: &Settlement) -> u64 {
    if quest.coordinates_are_geographic && settlement.source_node_id.is_some() {
        let lat1 = quest.location_coord_y.to_radians();
        let lat2 = settlement.coord_y.to_radians();
        let delta_lat = (settlement.coord_y - quest.location_coord_y).to_radians();
        let delta_lon = (settlement.coord_x - quest.location_coord_x).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        (6_371_000.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())).round() as u64
    } else {
        (((quest.location_coord_x - settlement.coord_x).powi(2)
            + (quest.location_coord_y - settlement.coord_y).powi(2))
        .sqrt()
            * 1_000.0)
            .round() as u64
    }
}
