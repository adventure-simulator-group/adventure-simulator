//! Party route handlers

use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::models::{Character, Party, PartyMember, Quest};
use crate::services;
use crate::session::Session;
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
    let parties: Vec<Party> = services::list_parties(&state.db).await.unwrap_or_default();

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
    let Some(leader_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    else {
        return Redirect::to("/characters");
    };

    let id = format!("party-{}", services::chrono_id());

    let result = services::create_party(&state.db, id.clone(), form.name, leader_id).await;

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
    let party = match services::get_party(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(p) => p,
        None => return Html("<h1>Party not found</h1>".to_string()),
    };

    // Get party members
    let members: Vec<PartyMember> = services::list_party_members(&state.db, &id)
        .await
        .unwrap_or_default();

    // Get character info for each member
    let mut members_with_chars: Vec<(PartyMember, Option<Character>)> = Vec::new();
    for member in members {
        let character = services::get_character(&state.db, member.character_id)
            .await
            .unwrap_or_default();
        members_with_chars.push((member, character));
    }

    // Get active quest if any
    let active_quest: Option<Quest> = if let Some(quest_id) = &party.active_quest_id {
        services::get_quest(&state.db, quest_id)
            .await
            .unwrap_or_default()
    } else {
        None
    };

    // Check if current user is the leader
    let is_leader = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
        == Some(party.leader_id);

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        party_detail_page(
            &party,
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
    let Some(character_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    else {
        return Redirect::to("/characters");
    };

    let _ = services::join_party(&state.db, character_id, id.clone()).await;

    Redirect::to(&format!("/parties/{}", id))
}

async fn leave_party(
    State(state): State<AppState>,
    Path(_id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    else {
        return Redirect::to("/characters");
    };

    let _ = services::leave_party(&state.db, character_id).await;

    Redirect::to("/parties")
}

async fn disband_party(State(state): State<AppState>, Path(id): Path<String>) -> Redirect {
    let _ = services::disband_party(&state.db, id).await;

    Redirect::to("/parties")
}

/// Helper to get character name for session display
async fn get_character_name(state: &AppState, character_id: Option<&str>) -> Option<String> {
    services::get_character_name(&state.db, character_id).await
}
