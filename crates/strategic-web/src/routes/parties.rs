//! Party route handlers

use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, Json, Redirect},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{Character, Party, PartyJoinRequest, PartyMember, Quest};
use crate::templates::party::{parties_list_page, party_detail_page, party_new_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/parties", get(list_parties))
        .route("/parties/new", get(new_party_form))
        .route("/parties", post(create_party))
        .route("/parties/{id}", get(show_party))
        .route("/parties/{id}/join", post(join_party))
        .route(
            "/parties/{id}/requests/{request_id}/accept",
            post(accept_join_request),
        )
        .route(
            "/parties/{id}/requests/{request_id}/reject",
            post(reject_join_request),
        )
        .route("/party-notifications", get(party_notifications))
        .route("/parties/{id}/leave", post(leave_party))
        .route("/parties/{id}/disband", post(disband_party))
}

#[derive(Deserialize)]
struct CreatePartyForm {
    #[serde(default)]
    name: String,
    desired_additional_members: u32,
    #[serde(default)]
    recruiting_quest_id: Option<String>,
    #[serde(default)]
    return_to: Option<String>,
}

async fn list_parties(State(state): State<AppState>, session: Session) -> Html<String> {
    let parties: Vec<Party> = state
        .db
        .query("SELECT * FROM party")
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(parties_list_page(&parties, None, logged_in_as.as_deref(), session.theme()).into_string())
}

async fn new_party_form(State(state): State<AppState>, session: Session) -> Html<String> {
    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(party_new_page(logged_in_as.as_deref(), session.theme()).into_string())
}

async fn create_party(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<CreatePartyForm>,
) -> Redirect {
    let Some(leader_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let id = format!("party-{}", chrono_id());
    let leader = get_character(&state, leader_id).await;
    let party_name = if form.name.trim().is_empty() {
        leader
            .as_ref()
            .map(|character| format!("{}'s party", character.name))
            .unwrap_or_else(|| "Adventuring party".to_string())
    } else {
        form.name
    };
    let desired = form.desired_additional_members.min(8);
    let recruiting_quest_id = match form.recruiting_quest_id {
        Some(quest_id) => json!({ "some": quest_id }),
        None => json!({ "none": [] }),
    };

    let result = state
        .db
        .call(
            "create_party",
            &[
                json!(id.clone()),
                json!(party_name),
                json!(leader_id),
                recruiting_quest_id,
                json!(desired),
            ],
        )
        .await;

    if let Err(error) = result {
        tracing::warn!(
            "Failed to create party for character {}: {:?}",
            leader_id,
            error
        );
        return Redirect::to("/parties/new");
    }

    if state.db.is_local() && desired > 0 {
        if let Err(error) = state
            .db
            .call("seed_bot_join_requests", &[json!(id), json!(desired)])
            .await
        {
            tracing::warn!("Failed to seed local bot join requests: {error:?}");
        }
    }

    let destination = form
        .return_to
        .filter(|path| path.starts_with("/settlements/") && !path.starts_with("//"))
        .unwrap_or_else(|| format!("/parties/{id}"));
    Redirect::to(&destination)
}

async fn show_party(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let party = match parties.first() {
        Some(p) => p,
        None => return Html("<h1>Party not found</h1>".to_string()),
    };

    // Get party members
    let members: Vec<PartyMember> = state
        .db
        .query(&format!(
            "SELECT * FROM party_member WHERE party_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();

    // Get character info for each member
    let mut members_with_chars: Vec<(PartyMember, Option<Character>)> = Vec::new();
    for member in members {
        let chars: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {}",
                member.character_id
            ))
            .await
            .unwrap_or_default();
        members_with_chars.push((member, chars.into_iter().next()));
    }

    // Get active quest if any
    let active_quest: Option<Quest> = if let Some(quest_id) = &party.active_quest_id {
        let quests: Vec<Quest> = state
            .db
            .query(&format!("SELECT * FROM quest WHERE id = '{}'", quest_id))
            .await
            .unwrap_or_default();
        quests.into_iter().next()
    } else {
        None
    };

    // Check if current user is the leader
    let is_leader = session.character_id_u64() == Some(party.leader_id);

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        party_detail_page(
            party,
            &members_with_chars,
            active_quest.as_ref(),
            is_leader,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn join_party(
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
            "request_to_join_party",
            &[json!(character_id), json!(id.clone())],
        )
        .await;

    Redirect::to(&format!("/parties/{}", id))
}

async fn accept_join_request(
    State(state): State<AppState>,
    Path((id, request_id)): Path<(String, u64)>,
    session: Session,
) -> Redirect {
    let Some(leader_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = state
        .db
        .call(
            "accept_party_join_request",
            &[json!(leader_id), json!(request_id)],
        )
        .await;
    Redirect::to(&party_noticeboard_url(&state, &id).await)
}

async fn reject_join_request(
    State(state): State<AppState>,
    Path((id, request_id)): Path<(String, u64)>,
    session: Session,
) -> Redirect {
    let Some(leader_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = state
        .db
        .call(
            "reject_party_join_request",
            &[json!(leader_id), json!(request_id)],
        )
        .await;
    Redirect::to(&party_noticeboard_url(&state, &id).await)
}

async fn party_noticeboard_url(state: &AppState, party_id: &str) -> String {
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();
    let Some(party) = parties.first() else {
        return "/parties".to_string();
    };
    match (&party.current_settlement_id, &party.recruiting_quest_id) {
        (Some(settlement), Some(quest)) => {
            format!("/settlements/{settlement}/noticeboard?quest={quest}")
        }
        (Some(settlement), None) => format!("/settlements/{settlement}/noticeboard"),
        _ => format!("/parties/{party_id}"),
    }
}

#[derive(Serialize)]
struct PartyNotifications {
    pending_join_requests: usize,
}

async fn party_notifications(
    State(state): State<AppState>,
    session: Session,
) -> Json<PartyNotifications> {
    let Some(character_id) = session.character_id_u64() else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    };
    let Some(character) = get_character(&state, character_id).await else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    };
    let Some(party_id) = character.party_id else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    };
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();
    if parties
        .first()
        .is_none_or(|party| party.leader_id != character_id)
    {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    }
    let requests: Vec<PartyJoinRequest> = state
        .db
        .query(&format!(
            "SELECT * FROM party_join_request WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    Json(PartyNotifications {
        pending_join_requests: requests.len(),
    })
}

async fn leave_party(
    State(state): State<AppState>,
    Path(_id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let _ = state.db.call("leave_party", &[json!(character_id)]).await;

    Redirect::to("/parties")
}

async fn disband_party(State(state): State<AppState>, Path(id): Path<String>) -> Redirect {
    let _ = state.db.call("disband_party", &[json!(id)]).await;

    Redirect::to("/parties")
}

/// Generate a simple timestamp-based ID
fn chrono_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
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

async fn get_character(state: &AppState, character_id: u64) -> Option<Character> {
    let characters: Vec<Character> = state
        .db
        .query(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    characters.into_iter().next()
}
