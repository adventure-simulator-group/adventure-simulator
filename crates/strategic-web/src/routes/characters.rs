//! Character route handlers

use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::{Session, clear_character_cookie, set_character_cookie};
use crate::spacetimedb::Character;
use crate::templates::character::{character_new_page, characters_list_page};

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

async fn list_characters(State(state): State<AppState>, session: Session) -> Html<String> {
    let characters: Vec<Character> = state
        .db
        .query("SELECT * FROM character")
        .await
        .unwrap_or_default();

    Html(
        characters_list_page(&characters, session.character_id_u64(), session.theme())
            .into_string(),
    )
}

async fn new_character_form(State(state): State<AppState>, session: Session) -> Html<String> {
    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(character_new_page(logged_in_as.as_deref(), session.theme()).into_string())
}

async fn create_character(
    State(state): State<AppState>,
    Form(form): Form<CreateCharacterForm>,
) -> Response {
    let id = chrono_id();

    if let Err(error) = state
        .db
        .call(
            "create_named_character_with_id",
            &[json!(id), json!(form.name)],
        )
        .await
    {
        tracing::error!("Failed to create character {id}: {error}");
        return Redirect::to("/characters/new").into_response();
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
