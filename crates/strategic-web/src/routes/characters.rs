//! Character route handlers

use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::{Session, clear_character_cookie, set_character_cookie};
use crate::spacetimedb::Character;
use crate::templates::character::{
    character_new_page, character_new_page_with_error, characters_list_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/characters", get(list_characters))
        .route("/characters/new", get(new_character_form))
        .route("/characters", post(create_character))
        .route("/characters/{id}/select", post(select_character))
        .route("/characters/switch", post(switch_character))
}

#[derive(Deserialize)]
struct CreateCharacterForm {
    name: String,
}

async fn list_characters(State(state): State<AppState>, session: Session) -> Response {
    let characters: Vec<Character> = if let Some(characters) = state.live.cached_characters() {
        characters
    } else {
        match state.db.query("SELECT * FROM character").await {
            Ok(characters) => characters,
            Err(error) => {
                tracing::error!(%error, "failed to list characters");
                return (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "Strategic data is unavailable",
                )
                    .into_response();
            }
        }
    };

    Html(characters_list_page(&characters, session.character_id_u64()).into_string())
        .into_response()
}

async fn new_character_form(State(state): State<AppState>, session: Session) -> Html<String> {
    let logged_in_as = get_character_name(&state, session.character_id_u64()).await;
    Html(character_new_page(logged_in_as.as_deref()).into_string())
}

async fn create_character(
    State(state): State<AppState>,
    Form(form): Form<CreateCharacterForm>,
) -> Response {
    let id = super::data::new_id();

    if let Err(error) = state
        .db
        .call(
            "create_named_character_with_id",
            &[json!(id), json!(form.name)],
        )
        .await
    {
        tracing::error!("Failed to create character {id}: {error}");
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Html(
                character_new_page_with_error(
                    Some(form.name.trim()),
                    Some("That adventurer could not be created. Check the name and try again."),
                )
                .into_string(),
            ),
        )
            .into_response();
    }

    // Auto-select the newly created character
    set_character_cookie(&id.to_string(), "/")
}

async fn select_character(Path(id): Path<String>) -> Response {
    set_character_cookie(&id, "/")
}

async fn switch_character() -> Response {
    clear_character_cookie("/characters")
}

/// Helper to get character name for session display
async fn get_character_name(state: &AppState, character_id: Option<u64>) -> Option<String> {
    let Some(id) = character_id else {
        return None;
    };
    match super::data::character(state, id).await {
        Ok(character) => character.map(|character| character.name),
        Err(error) => {
            tracing::error!(%error, "failed to load selected character");
            None
        }
    }
}
