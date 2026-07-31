use adventuresim_core::errantry::{OrderedSigilProjection, OrderedSigilSubmission};
use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::{
    session::Session,
    spacetimedb::{BackendChallenge, Character, sql_string_literal},
    templates::challenge::{ordered_sigil_page, parse_form_sigils},
};

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/quests/{case_id}/challenges/{challenge_id}",
        get(show).post(submit),
    )
}

async fn projection(
    state: &AppState,
    character_id: u64,
    case_id: &str,
    challenge_id: &str,
) -> Result<BackendChallenge, StatusCode> {
    let sql = format!(
        "SELECT * FROM backend_challenges WHERE owner_character_id = {character_id} AND case_id = {} AND id = {} AND active = true",
        sql_string_literal(case_id),
        sql_string_literal(challenge_id)
    );
    state
        .db
        .query_one::<BackendChallenge>(&sql)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn show(
    State(state): State<AppState>,
    session: Session,
    Path((case_id, challenge_id)): Path<(String, String)>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let character_sql = format!("SELECT * FROM character WHERE id = {character_id}");
    let (challenge, character) = tokio::join!(
        projection(&state, character_id, &case_id, &challenge_id),
        state.db.query_one::<Character>(&character_sql)
    );
    let challenge = match challenge {
        Ok(value) => value,
        Err(status) => return status.into_response(),
    };
    let character = match character {
        Ok(Some(value)) => value,
        _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let puzzle: OrderedSigilProjection =
        match serde_json::from_str(&challenge.puzzle_projection_json) {
            Ok(value) => value,
            Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
    Html(
        ordered_sigil_page(
            &challenge.id,
            &challenge.case_id,
            challenge.revision,
            &puzzle,
            challenge.solved,
            challenge.last_attempt_correct,
            challenge.boon_item_id.as_deref(),
            challenge.boon_combat_scale_reduction_bps,
            &character.name,
        )
        .into_string(),
    )
    .into_response()
}

#[derive(Deserialize)]
struct ChallengeForm {
    expected_revision: u32,
    sigil_0: String,
    sigil_1: String,
    sigil_2: String,
    sigil_3: String,
    sigil_4: String,
}

async fn submit(
    State(state): State<AppState>,
    session: Session,
    Path((case_id, challenge_id)): Path<(String, String)>,
    Form(form): Form<ChallengeForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Err(status) = projection(&state, character_id, &case_id, &challenge_id).await {
        return status.into_response();
    }
    let ordering = match parse_form_sigils([
        &form.sigil_0,
        &form.sigil_1,
        &form.sigil_2,
        &form.sigil_3,
        &form.sigil_4,
    ]) {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let submission = OrderedSigilSubmission {
        expected_revision: form.expected_revision,
        ordering,
    };
    let ordering_json = match serde_json::to_string(&submission.ordering) {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    match state
        .db
        .call(
            "submit_ordered_sigil_challenge",
            &[
                json!(character_id),
                json!(case_id),
                json!(challenge_id),
                json!(submission.expected_revision),
                json!(ordering_json),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to(&format!("/quests/{}/challenges/{}", case_id, challenge_id))
            .into_response(),
        Err(error) if error.to_string().contains("stale") => StatusCode::CONFLICT.into_response(),
        Err(error) => {
            tracing::warn!(%error, character_id, "challenge submission rejected");
            StatusCode::UNPROCESSABLE_ENTITY.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn route_is_server_rendered_post_redirect_get() {
        let source = include_str!("challenges.rs");
        assert!(source.contains("get(show).post(submit)"));
        assert!(source.contains("Redirect::to"));
        assert!(source.contains("owner_character_id = {character_id}"));
        assert!(source.contains("AND active = true"));
        assert!(
            source
                .matches("projection(&state, character_id, &case_id, &challenge_id)")
                .count()
                >= 2,
            "both display and submission must reject stale camp URLs before reducer dispatch"
        );
        assert!(source.contains("submit_ordered_sigil_challenge"));
    }
}
