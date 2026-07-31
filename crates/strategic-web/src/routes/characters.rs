//! Character route handlers

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use crate::session::{Session, clear_character_cookie, redirect_with_session_cookie};
use crate::spacetimedb::{Character, CharacterStrategicCondition};
use crate::templates::character::{
    character_candidates_bootstrap_page, character_candidates_page, character_switcher_options,
    characters_list_page,
};
use adventuresim_core::starting_character::{GENERATOR_VERSION, StartingAgeTier, generate, roster};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/characters", get(list_characters))
        .route(
            "/characters/candidates",
            get(candidate_roster).post(confirm_candidate),
        )
        .route("/characters/new", get(redirect_to_candidates))
        .route("/characters/menu", get(character_menu))
        .route("/characters/{id}/select", post(select_character))
        .route("/api/characters/{id}/condition", get(character_condition))
        .route("/characters/switch", post(switch_character))
}

#[derive(Deserialize, Default)]
struct CandidateQuery {
    version: Option<u16>,
    seed: Option<String>,
    age: Option<StartingAgeTier>,
    selected: Option<u8>,
    view: Option<CandidateView>,
}

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CandidateView {
    Profile,
    Inventory,
}

async fn character_condition(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
) -> Response {
    let Some(viewer_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let (viewer, subject) = tokio::join!(
        super::data::character(&state, viewer_id),
        super::data::character_as_observed(&state, id, viewer_id),
    );
    let (Ok(Some(viewer)), Ok(Some(subject))) = (viewer, subject) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let synchronized = viewer.id == subject.id
        || super::data::characters_share_frontier(&state, viewer.id, subject.id)
            .await
            .unwrap_or(false);
    let same_party = viewer.id == subject.id
        || (viewer.party_id.is_some() && viewer.party_id == subject.party_id);
    let colocated = viewer.current_settlement_id == subject.current_settlement_id
        && viewer.current_case_site_id == subject.current_case_site_id;
    if !synchronized || !same_party || !colocated {
        return StatusCode::FORBIDDEN.into_response();
    }
    Json(
        state
            .db
            .query_one::<CharacterStrategicCondition>(&format!(
                "SELECT * FROM backend_character_strategic_conditions WHERE character_id = {id}"
            ))
            .await
            .ok()
            .flatten(),
    )
    .into_response()
}

#[derive(Deserialize)]
struct ConfirmCandidateForm {
    version: u16,
    seed: String,
    age: StartingAgeTier,
    slot: u8,
}

async fn redirect_to_candidates() -> Response {
    axum::response::Redirect::to("/characters/candidates").into_response()
}

async fn list_characters(State(state): State<AppState>, session: Session) -> Response {
    let characters = remembered_characters(&state, &session).await;
    Html(characters_list_page(&characters, session.character_id_u64()).into_string())
        .into_response()
}

async fn character_menu(State(state): State<AppState>, session: Session) -> Response {
    let characters = remembered_characters(&state, &session).await;
    Html(character_switcher_options(&characters, session.character_id_u64()).into_string())
        .into_response()
}

async fn remembered_characters(state: &AppState, session: &Session) -> Vec<Character> {
    let ids = session.character_ids();
    if ids.is_empty() {
        return Vec::new();
    }
    let characters: Vec<Character> = if let Some(characters) = state.live.cached_characters() {
        characters
    } else {
        match state
            .db
            .query::<Character>("SELECT * FROM backend_characters")
            .await
        {
            Ok(characters) => characters,
            Err(error) => {
                tracing::error!(%error, "failed to list characters");
                return Vec::new();
            }
        }
    };
    let mut remembered: Vec<_> = ids
        .into_iter()
        .filter_map(|id| {
            characters
                .iter()
                .find(|character| character.id == id && !character.temporary)
                .cloned()
        })
        .collect();
    for character in &mut remembered {
        if let Err(error) = super::data::project_alive_as_observed(
            state,
            character.id,
            std::slice::from_mut(character),
        )
        .await
        {
            tracing::warn!(%error, character_id = character.id, "could not project remembered character life state");
            // Do not disclose broad current death state when its chronology
            // could not be compared with this character's personal date.
            character.alive = true;
        }
    }
    remembered
}

async fn candidate_roster(Query(query): Query<CandidateQuery>) -> Response {
    let (Some(version), Some(seed), Some(age)) = (query.version, query.seed.as_deref(), query.age)
    else {
        return Html(character_candidates_bootstrap_page(GENERATOR_VERSION).into_string())
            .into_response();
    };
    let candidates = match roster(version, seed, age) {
        Ok(candidates) => candidates,
        Err(_) => {
            return Html(character_candidates_bootstrap_page(GENERATOR_VERSION).into_string())
                .into_response();
        }
    };
    let selected = query.selected.filter(|slot| *slot < candidates.len() as u8);
    Html(
        character_candidates_page(
            version,
            seed,
            age,
            &candidates,
            selected,
            query.view == Some(CandidateView::Inventory),
        )
        .into_string(),
    )
    .into_response()
}

async fn confirm_candidate(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ConfirmCandidateForm>,
) -> Response {
    let spec = match generate(form.version, &form.seed, form.age, form.slot) {
        Ok(spec) => spec,
        Err(error) => return (axum::http::StatusCode::BAD_REQUEST, error).into_response(),
    };
    let issued = if session.owner_key().is_none() {
        match state.session_codec.issue() {
            Ok(issued) => Some(issued),
            Err(error) => {
                tracing::error!(%error, "failed to issue browser session");
                return (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "A browser session could not be created. Please try again.",
                )
                    .into_response();
            }
        }
    } else {
        None
    };
    let owner_key = session
        .owner_key()
        .or_else(|| issued.as_ref().map(|issued| issued.owner_key.as_str()))
        .expect("existing or newly issued browser owner");
    let token = session
        .token()
        .or_else(|| issued.as_ref().map(|issued| issued.token.as_str()));
    if let Err(error) = state
        .db
        .call(
            "create_starting_character",
            &[
                json!(owner_key),
                json!(form.version),
                json!(form.seed),
                starting_age_tier_argument(form.age),
                json!(form.slot),
            ],
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
    if let Err(error) = state
        .db
        .call(
            "select_browser_character",
            &[json!(owner_key), json!(spec.id)],
        )
        .await
    {
        tracing::error!(character_id = spec.id, %error, "failed to select starting character");
        // Preserve a newly issued identity after the durable grant succeeds so
        // a transient selection failure cannot orphan the character.
        return redirect_with_session_cookie(&state.session_codec, token, "/characters");
    }
    redirect_with_session_cookie(&state.session_codec, token, "/")
}

async fn select_character(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
) -> Response {
    let Some(owner_key) = session.owner_key() else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            "Browser session required",
        )
            .into_response();
    };
    match super::data::character(&state, id).await {
        Ok(Some(character)) if !character.temporary && session.character_ids().contains(&id) => {
            match state
                .db
                .call("select_browser_character", &[json!(owner_key), json!(id)])
                .await
            {
                Ok(()) => redirect_with_session_cookie(&state.session_codec, None, "/"),
                Err(error) => {
                    tracing::error!(character_id = id, %error, "failed to select granted character");
                    (
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        "Character selection is unavailable",
                    )
                        .into_response()
                }
            }
        }
        Ok(Some(_)) => {
            (axum::http::StatusCode::FORBIDDEN, "Character not available").into_response()
        }
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, "Character not found").into_response(),
        Err(_) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Strategic data is unavailable",
        )
            .into_response(),
    }
}

async fn switch_character(State(state): State<AppState>, session: Session) -> Response {
    if let Some(owner_key) = session.owner_key()
        && let Err(error) = state
            .db
            .call("clear_browser_character_selection", &[json!(owner_key)])
            .await
    {
        tracing::error!(%error, "failed to clear browser character selection");
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Character selection is unavailable",
        )
            .into_response();
    }
    clear_character_cookie("/characters/candidates")
}

fn starting_age_tier_argument(age: StartingAgeTier) -> Value {
    match age {
        StartingAgeTier::Young => json!({ "young": {} }),
        StartingAgeTier::Adult => json!({ "adult": {} }),
        StartingAgeTier::Old => json!({ "old": {} }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_age_tiers_use_spacetime_sum_encoding() {
        assert_eq!(
            starting_age_tier_argument(StartingAgeTier::Young),
            json!({ "young": {} })
        );
        assert_eq!(
            starting_age_tier_argument(StartingAgeTier::Adult),
            json!({ "adult": {} })
        );
        assert_eq!(
            starting_age_tier_argument(StartingAgeTier::Old),
            json!({ "old": {} })
        );
    }

    #[test]
    fn remembered_roster_projects_each_character_at_its_own_date() {
        let source = include_str!("characters.rs");
        let loader = source
            .split("async fn remembered_characters")
            .nth(1)
            .unwrap()
            .split("async fn candidate_roster")
            .next()
            .unwrap();
        assert!(loader.contains("project_alive_as_observed"));
        assert!(loader.contains("character.id"));
        assert!(loader.contains("character.alive = true"));
    }

    #[test]
    fn condition_authorization_rejects_unsynchronized_mutable_character_state() {
        let source = include_str!("characters.rs");
        let handler = source
            .split("async fn character_condition")
            .nth(1)
            .unwrap()
            .split("struct ConfirmCandidateForm")
            .next()
            .unwrap();
        assert!(handler.contains("character_as_observed(&state, id, viewer_id)"));
        assert!(handler.contains("characters_share_frontier"));
        assert!(handler.contains("!synchronized || !same_party || !colocated"));
    }
}
