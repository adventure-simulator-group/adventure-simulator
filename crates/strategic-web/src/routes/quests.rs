//! Quest route handlers

use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    routing::{get, post},
    Router,
};

use super::AppState;
use crate::models::Quest;
use crate::services;
use crate::session::Session;
use crate::templates::quest::{quest_detail_page, quests_list_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/quests", get(list_quests))
        .route("/quests/{id}", get(show_quest))
        .route("/quests/{id}/accept", post(accept_quest))
        .route("/quests/{id}/abandon", post(abandon_quest))
}

async fn list_quests(State(state): State<AppState>, session: Session) -> Html<String> {
    let quests: Vec<Quest> = services::list_quests(&state.db).await.unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(quests_list_page(&quests, logged_in_as.as_deref(), session.theme()).into_string())
}

async fn show_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let quest = match services::get_quest(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(q) => q,
        None => return Html("<h1>Quest not found</h1>".to_string()),
    };

    // Check if user can accept: must be party leader at quest's settlement
    let mut can_accept = false;
    let mut is_party_quest = false;

    if let Some(character_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    {
        // Get character
        let character = services::get_character(&state.db, character_id)
            .await
            .unwrap_or_default();

        if let Some(character) = character.as_ref() {
            // Check if at quest's settlement
            let at_settlement =
                character.current_settlement_id.as_ref() == Some(&quest.settlement_id);

            // Check if party leader
            if let Some(party_id) = &character.party_id {
                let party = services::get_party(&state.db, party_id)
                    .await
                    .unwrap_or_default();

                if let Some(party) = party.as_ref() {
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
            &quest,
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
    services::get_character_name(&state.db, character_id).await
}

fn is_available_status(status: &str) -> bool {
    status.to_ascii_lowercase().contains("available")
}

async fn accept_quest(
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

    let _ = services::accept_quest(&state.db, character_id, id.clone()).await;

    Redirect::to(&format!("/quests/{}", id))
}

async fn abandon_quest(
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

    let _ = services::abandon_quest(&state.db, character_id, id.clone()).await;

    Redirect::to(&format!("/quests/{}", id))
}
