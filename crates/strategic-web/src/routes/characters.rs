//! Character route handlers

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::{Session, clear_character_cookie, set_character_cookie};
use crate::spacetimedb::Character;
use crate::templates::character::{
    character_candidates_bootstrap_page, character_candidates_page, characters_list_page,
};
use adventuresim_core::starting_character::{GENERATOR_VERSION, generate, roster};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/characters", get(list_characters))
        .route(
            "/characters/candidates",
            get(candidate_roster).post(confirm_candidate),
        )
        .route("/characters/new", get(redirect_to_candidates))
        .route("/characters/{id}/select", post(select_character))
        .route("/characters/switch", post(switch_character))
}

#[derive(Deserialize, Default)]
struct CandidateQuery {
    version: Option<u16>,
    seed: Option<String>,
    selected: Option<u8>,
}

#[derive(Deserialize)]
struct ConfirmCandidateForm {
    version: u16,
    seed: String,
    slot: u8,
}

async fn redirect_to_candidates() -> Response {
    axum::response::Redirect::to("/characters/candidates").into_response()
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

async fn candidate_roster(Query(query): Query<CandidateQuery>) -> Response {
    let (Some(version), Some(seed)) = (query.version, query.seed.as_deref()) else {
        return Html(character_candidates_bootstrap_page(GENERATOR_VERSION).into_string())
            .into_response();
    };
    let candidates = match roster(version, seed) {
        Ok(candidates) => candidates,
        Err(_) => {
            return Html(character_candidates_bootstrap_page(GENERATOR_VERSION).into_string())
                .into_response();
        }
    };
    let selected = query
        .selected
        .filter(|slot| *slot < candidates.len() as u8)
        .unwrap_or(0);
    Html(character_candidates_page(version, seed, &candidates, selected).into_string())
        .into_response()
}

async fn confirm_candidate(
    State(state): State<AppState>,
    Form(form): Form<ConfirmCandidateForm>,
) -> Response {
    let spec = match generate(form.version, &form.seed, form.slot) {
        Ok(spec) => spec,
        Err(error) => return (axum::http::StatusCode::BAD_REQUEST, error).into_response(),
    };
    if let Err(error) = state
        .db
        .call(
            "create_starting_character",
            &[json!(form.version), json!(form.seed), json!(form.slot)],
        )
        .await
    {
        tracing::error!(character_id = spec.id, %error, "failed to confirm starting character");
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "That candidate could not be created. Please try again.",
        )
            .into_response();
    }
    set_character_cookie(&spec.id.to_string(), "/")
}

async fn select_character(State(state): State<AppState>, Path(id): Path<u64>) -> Response {
    match super::data::character(&state, id).await {
        Ok(Some(_)) => set_character_cookie(&id.to_string(), "/"),
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, "Character not found").into_response(),
        Err(_) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Strategic data is unavailable",
        )
            .into_response(),
    }
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
