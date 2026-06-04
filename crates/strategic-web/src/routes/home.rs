//! Home route handlers

use axum::{extract::State, response::Html, routing::get, Router};

use super::AppState;
use crate::models::{Character, Party, Quest, Settlement};
use crate::services;
use crate::session::Session;
use crate::templates::home::home_page;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(home))
}

async fn home(State(state): State<AppState>, session: Session) -> Html<String> {
    // Get all characters
    let characters: Vec<Character> = services::list_characters(&state.db)
        .await
        .unwrap_or_default();

    // Find current character from session
    let current_character = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
        .and_then(|id| characters.iter().find(|c| c.id == id));

    // Get related data if we have a current character
    let (current_settlement, active_party, active_quest) =
        if let Some(character) = current_character {
            // Get settlement
            let settlement: Option<Settlement> =
                if let Some(settlement_id) = &character.current_settlement_id {
                    services::get_settlement(&state.db, settlement_id)
                        .await
                        .unwrap_or_default()
                } else {
                    None
                };

            // Get party
            let party: Option<Party> = if let Some(party_id) = &character.party_id {
                services::get_party(&state.db, party_id)
                    .await
                    .unwrap_or_default()
            } else {
                None
            };

            // Get active quest if party has one
            let quest: Option<Quest> =
                if let Some(quest_id) = party.as_ref().and_then(|p| p.active_quest_id.as_ref()) {
                    services::get_quest(&state.db, quest_id)
                        .await
                        .unwrap_or_default()
                } else {
                    None
                };

            (settlement, party, quest)
        } else {
            (None, None, None)
        };

    Html(
        home_page(
            &characters,
            current_character,
            current_settlement.as_ref(),
            active_party.as_ref(),
            active_quest.as_ref(),
            session.theme(),
        )
        .into_string(),
    )
}
