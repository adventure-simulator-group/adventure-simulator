//! Home route handlers

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{Party, sql_string_literal};

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(home))
}

async fn home(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let character = match super::data::character(&state, character_id).await {
        Ok(Some(character)) => character,
        Ok(None) => return Redirect::to("/characters").into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to load home character");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Strategic data is unavailable",
            )
                .into_response();
        }
    };
    if let Some(party_id) = character.party_id.as_deref()
        && let Ok(Some(party)) = state
            .db
            .query_one::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
        && party.camp_destination_id.is_some()
    {
        return Redirect::to("/camp").into_response();
    }
    match (
        &character.current_settlement_id,
        &character.current_case_site_id,
    ) {
        (Some(settlement_id), _) => {
            Redirect::to(&format!("/locations/settlement/{settlement_id}")).into_response()
        }
        (_, Some(case_site_id)) => {
            Redirect::to(&format!("/locations/case-site/{case_site_id}")).into_response()
        }
        _ => Redirect::to("/characters").into_response(),
    }
}
