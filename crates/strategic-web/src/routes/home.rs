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

fn character_location_path(
    settlement_id: Option<&str>,
    case_site_id: Option<&str>,
) -> Option<String> {
    settlement_id
        .map(|id| format!("/locations/settlement/{id}"))
        .or_else(|| case_site_id.map(|id| format!("/locations/case-site/{id}")))
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
        && party.camp_destination.is_some()
    {
        return Redirect::to("/camp").into_response();
    }
    if let Some(path) = character_location_path(
        character.current_settlement_id.as_deref(),
        character.current_case_site_id.as_deref(),
    ) {
        Redirect::to(&path).into_response()
    } else {
        Redirect::to("/characters").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::character_location_path;

    #[test]
    fn selected_character_at_generated_case_site_routes_after_camp_arrival() {
        assert_eq!(
            character_location_path(None, Some("case-site:generated:old-graveyard")),
            Some("/locations/case-site/case-site:generated:old-graveyard".into())
        );
        assert_eq!(
            character_location_path(Some("lubeck"), Some("case-site:generated:site")),
            Some("/locations/settlement/lubeck".into())
        );
        assert_eq!(character_location_path(None, None), None);
    }
}
