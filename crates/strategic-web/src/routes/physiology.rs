use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};

use super::AppState;
use crate::{
    session::Session, spacetimedb::Character, templates::physiology::physiology_reference_page,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/physiology", get(reference))
}

async fn reference(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
    {
        Ok(Some(character)) => {
            Html(physiology_reference_page(&character.name).into_string()).into_response()
        }
        Ok(None) => StatusCode::UNAUTHORIZED.into_response(),
        Err(error) => {
            tracing::error!(%error, character_id, "physiology reference character lookup failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
