//! Home route handlers

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};

use super::AppState;
use crate::session::{Session, clear_character_cookie};
use crate::spacetimedb::Character;

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

fn home_path(character: &crate::spacetimedb::Character, party_is_camping: bool) -> String {
    if party_is_camping {
        "/camp".into()
    } else if let Some(path) = character_location_path(
        character.current_settlement_id.as_deref(),
        character.current_case_site_id.as_deref(),
    ) {
        path
    } else {
        "/characters".into()
    }
}

async fn home(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters/candidates").into_response();
    };
    let character = match super::data::character(&state, character_id).await {
        Ok(Some(character)) => character,
        Ok(None) => {
            let destination = match state
                .db
                .query::<Character>("SELECT * FROM character")
                .await
            {
                Ok(characters) if characters.is_empty() => "/characters/candidates",
                _ => "/characters",
            };
            return clear_character_cookie(destination);
        }
        Err(error) => {
            tracing::error!(%error, "failed to load home character");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Strategic data is unavailable",
            )
                .into_response();
        }
    };
    let party_is_camping = match character.party_id.as_deref() {
        Some(party_id) => match state.live.cached_party_has_camp(party_id) {
            Some(value) => value,
            None => state
                .db
                .query_one::<crate::spacetimedb::Party>(&format!(
                    "SELECT * FROM party WHERE id = '{}'",
                    party_id.replace('\'', "''")
                ))
                .await
                .ok()
                .flatten()
                .is_some_and(|party| party.camp_destination.is_some()),
        },
        None => false,
    };
    Redirect::to(&home_path(&character, party_is_camping)).into_response()
}

#[cfg(test)]
mod tests {
    use super::{character_location_path, home_path};

    fn character() -> crate::spacetimedb::Character {
        crate::spacetimedb::Character {
            id: 1,
            name: "Ada".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party-1".into()),
            age_years: 20,
            alive: true,
            temporary: false,
        }
    }

    #[test]
    fn page_model_prefers_camp_over_location_and_falls_back_to_picker() {
        let mut character = character();
        character.current_settlement_id = Some("lubeck".into());
        assert_eq!(home_path(&character, true), "/camp");
        assert_eq!(home_path(&character, false), "/locations/settlement/lubeck");
        character.current_settlement_id = None;
        assert_eq!(home_path(&character, false), "/characters");
    }

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
