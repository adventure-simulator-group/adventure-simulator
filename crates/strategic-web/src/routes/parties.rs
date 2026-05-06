//! Party route handlers

use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{Character, Party, PartyMember, Quest};
use crate::templates::party::{parties_list_page, party_detail_page, party_new_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/parties", get(list_parties))
        .route("/parties/new", get(new_party_form))
        .route("/parties", post(create_party))
        .route("/parties/{id}", get(show_party))
        .route("/parties/{id}/join", post(join_party))
        .route("/parties/{id}/leave", post(leave_party))
        .route("/parties/{id}/disband", post(disband_party))
}

#[derive(Deserialize)]
struct CreatePartyForm {
    name: String,
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

    let result = state
        .db
        .call(
            "create_party",
            &[json!(id.clone()), json!(form.name), json!(leader_id)],
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

    Redirect::to(&format!("/parties/{}", id))
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
        .call("join_party", &[json!(character_id), json!(id.clone())])
        .await;

    Redirect::to(&format!("/parties/{}", id))
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
