use adventuresim_core::errantry::{PuzzleProjection, PuzzleSubmission};
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
    templates::challenge::{parse_form_sigils, parse_sigil, parse_witness_path, puzzle_page},
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
    let puzzle: PuzzleProjection = match serde_json::from_str(&challenge.puzzle_projection_json) {
        Ok(value) => value,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let last_submission = match challenge.last_submission_json.as_deref() {
        Some(value) => match serde_json::from_str::<PuzzleSubmission>(value) {
            Ok(submission) if submission.kind() == puzzle.kind() => Some(submission),
            _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        },
        None => None,
    };
    let catalog = match challenge.presenter_catalog_id {
        crate::spacetimedb::ChallengePresenterCatalogId::LadyBeneathThornV1 => {
            adventuresim_core::errantry::FeyPresenterCatalogId::LadyBeneathThornV1
        }
    };
    Html(
        puzzle_page(
            &challenge.id,
            &challenge.case_id,
            catalog,
            challenge.revision,
            &puzzle,
            challenge.solved,
            challenge.last_attempt_correct,
            last_submission.as_ref(),
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
    sigil_0: Option<String>,
    sigil_1: Option<String>,
    sigil_2: Option<String>,
    sigil_3: Option<String>,
    sigil_4: Option<String>,
    safe_path: Option<String>,
    rune_result: Option<String>,
}

fn submission_for(
    projection: &PuzzleProjection,
    form: &ChallengeForm,
) -> Result<PuzzleSubmission, &'static str> {
    fn required(value: &Option<String>) -> Result<&str, &'static str> {
        value.as_deref().ok_or("Puzzle answer is incomplete")
    }
    match projection {
        PuzzleProjection::OrderedSigils(_) => Ok(PuzzleSubmission::OrderedSigils {
            ordering: parse_form_sigils([
                required(&form.sigil_0)?,
                required(&form.sigil_1)?,
                required(&form.sigil_2)?,
                required(&form.sigil_3)?,
                required(&form.sigil_4)?,
            ])?,
        }),
        PuzzleProjection::TruthfulWitnesses(_) => Ok(PuzzleSubmission::TruthfulWitnesses {
            safe_path: parse_witness_path(required(&form.safe_path)?)?,
        }),
        PuzzleProjection::RuneTransformation(_) => Ok(PuzzleSubmission::RuneTransformation {
            result: parse_sigil(required(&form.rune_result)?)?,
        }),
    }
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
    let challenge = match projection(&state, character_id, &case_id, &challenge_id).await {
        Ok(value) => value,
        Err(status) => return status.into_response(),
    };
    let puzzle: PuzzleProjection = match serde_json::from_str(&challenge.puzzle_projection_json) {
        Ok(value) => value,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let submission = match submission_for(&puzzle, &form) {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let submission_json = match serde_json::to_string(&submission) {
        Ok(value) => value,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    match state
        .db
        .call(
            "submit_puzzle_challenge",
            &[
                json!(character_id),
                json!(case_id),
                json!(challenge_id),
                json!(form.expected_revision),
                json!(submission_json),
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
    use super::*;
    use adventuresim_core::errantry::{PuzzleAuthority, PuzzleKind};

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
        assert!(source.contains("submit_puzzle_challenge"));
    }

    #[test]
    fn form_answers_are_parsed_against_the_authoritative_projection_kind() {
        let ordered = ChallengeForm {
            expected_revision: 2,
            sigil_0: Some("Crown".into()),
            sigil_1: Some("Hart".into()),
            sigil_2: Some("Moon".into()),
            sigil_3: Some("Rose".into()),
            sigil_4: Some("Sword".into()),
            safe_path: None,
            rune_result: None,
        };
        let ordered_projection =
            PuzzleAuthority::generate(PuzzleKind::OrderedSigils, 1).projection();
        assert!(matches!(
            submission_for(&ordered_projection, &ordered),
            Ok(PuzzleSubmission::OrderedSigils { .. })
        ));

        let witness_projection =
            PuzzleAuthority::generate(PuzzleKind::TruthfulWitnesses, 2).projection();
        assert!(submission_for(&witness_projection, &ordered).is_err());
        let witness = ChallengeForm {
            safe_path: Some("Moon path".into()),
            sigil_0: None,
            sigil_1: None,
            sigil_2: None,
            sigil_3: None,
            sigil_4: None,
            rune_result: None,
            expected_revision: 0,
        };
        assert!(matches!(
            submission_for(&witness_projection, &witness),
            Ok(PuzzleSubmission::TruthfulWitnesses { .. })
        ));

        let rune_projection =
            PuzzleAuthority::generate(PuzzleKind::RuneTransformation, 3).projection();
        let rune = ChallengeForm {
            rune_result: Some("Rose".into()),
            safe_path: None,
            sigil_0: None,
            sigil_1: None,
            sigil_2: None,
            sigil_3: None,
            sigil_4: None,
            expected_revision: 0,
        };
        assert!(matches!(
            submission_for(&rune_projection, &rune),
            Ok(PuzzleSubmission::RuneTransformation { .. })
        ));
    }
}
