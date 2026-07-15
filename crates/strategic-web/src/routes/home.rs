//! Home route handlers

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::Character;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(home))
}

async fn home(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let characters: Vec<Character> = state
        .db
        .query(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let Some(character) = characters.first() else {
        return Redirect::to("/characters").into_response();
    };
    match (
        &character.current_settlement_id,
        &character.current_quest_location_id,
    ) {
        (Some(settlement_id), _) => {
            Redirect::to(&format!("/locations/settlement/{settlement_id}")).into_response()
        }
        (_, Some(quest_id)) => {
            Redirect::to(&format!("/locations/quest/{quest_id}")).into_response()
        }
        _ => Redirect::to("/characters").into_response(),
    }
}
