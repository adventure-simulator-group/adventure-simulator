//! Character route handlers

use axum::{
    Form, Json, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::{Session, clear_character_cookie, set_character_cookie};
use crate::spacetimedb::{Character, CharacterStrategicCondition};
use crate::templates::character::{character_new_page, characters_list_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/characters", get(list_characters))
        .route("/characters/new", get(new_character_form))
        .route("/characters", post(create_character))
        .route("/characters/{id}/select", post(select_character))
        .route("/api/characters/{id}/condition", get(character_condition))
        .route("/characters/switch", post(switch_character))
}

async fn character_condition(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Json<Option<CharacterStrategicCondition>> {
    if let Err(error) = state
        .db
        .call("refresh_strategic_condition", &[json!(id)])
        .await
    {
        tracing::warn!(%error, character_id = id, "failed to refresh strategic condition");
        return Json(None);
    }
    Json(
        state
            .db
            .query_one(&format!(
                "SELECT * FROM character_strategic_condition WHERE character_id = {id}"
            ))
            .await
            .ok()
            .flatten(),
    )
}

#[derive(Deserialize)]
struct CreateCharacterForm {
    name: String,
}

async fn list_characters(State(state): State<AppState>, session: Session) -> Response {
    let characters: Vec<Character> = match state.db.query("SELECT * FROM character").await {
        Ok(characters) => characters,
        Err(error) => {
            tracing::error!(%error, "failed to list characters");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Strategic data is unavailable",
            )
                .into_response();
        }
    };

    Html(
        characters_list_page(&characters, session.character_id_u64(), session.theme())
            .into_string(),
    )
    .into_response()
}

async fn new_character_form(State(state): State<AppState>, session: Session) -> Html<String> {
    let logged_in_as = get_character_name(&state, session.character_id_u64()).await;
    Html(character_new_page(logged_in_as.as_deref(), session.theme()).into_string())
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
