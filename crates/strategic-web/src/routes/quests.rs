//! Quest route handlers

use axum::{
    Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde_json::json;

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{Character, Party, Quest, Settlement};
use crate::templates::quest::{quest_detail_page, quest_location_page, quests_list_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/quests", get(list_quests))
        .route("/quests/{id}", get(show_quest))
        .route("/quests/{id}/accept", post(accept_quest))
        .route("/quests/{id}/abandon", post(abandon_quest))
        .route("/quests/{id}/travel", post(travel_to_quest))
        .route("/quests/{id}/location", get(quest_location))
        .route("/quests/{id}/autoresolve", post(autoresolve_quest))
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

#[derive(Debug, Clone)]
pub struct NearbySettlement {
    pub settlement: Settlement,
    pub distance_m: u64,
    pub journey_minutes: u64,
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
                journey_minutes: quest_journey_minutes(distance_m),
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
    Html(
        quest_location_page(
            quest,
            &nearby,
            character.as_ref(),
            can_control,
            can_fight,
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

fn quest_journey_minutes(distance_m: u64) -> u64 {
    distance_m
        .saturating_mul(60)
        .div_ceil(5_000)
        .saturating_mul(4)
        .max(1)
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
