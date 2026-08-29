//! Home route handlers

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};

use super::AppState;
use crate::session::{Session, clear_character_cookie, redirect_with_session_cookie};
use crate::spacetimedb::{CharacterView, party_by_id};
use serde_json::json;

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

fn home_path(character: &crate::spacetimedb::CharacterView, party_is_camping: bool) -> String {
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
        let issued = if session.owner_key().is_none() {
            match state.session_codec.issue() {
                Ok(issued) => Some(issued),
                Err(error) => {
                    tracing::error!(%error, "failed to issue default-character browser session");
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
        let default = adventuresim_core::starting_character::default_character(owner_key);
        if let Err(error) = state
            .db
            .call("create_default_character", &[json!(owner_key)])
            .await
        {
            tracing::error!(%error, "failed to create the default character");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "The default character could not be created. Please try again.",
            )
                .into_response();
        }
        if let Err(error) = state
            .db
            .call(
                "select_browser_character",
                &[json!(owner_key), json!(default.id)],
            )
            .await
        {
            tracing::error!(%error, character_id = default.id, "failed to select the default character");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "The default character could not be selected. Please try again.",
            )
                .into_response();
        }
        let token = session
            .token()
            .or_else(|| issued.as_ref().map(|issued| issued.token.as_str()));
        return redirect_with_session_cookie(&state.session_codec, token, "/");
    };
    let character = match super::data::character(&state, character_id).await {
        Ok(Some(character)) => character,
        Ok(None) => {
            let destination = match state
                .db
                .query_sats_into::<adventuresim_stdb_client::Character, CharacterView>(
                    "SELECT * FROM backend_characters",
                )
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
            None => {
                let query = party_by_id(party_id);
                state
                    .db
                    .query_one_sats_into::<
                        adventuresim_stdb_client::Party,
                        crate::spacetimedb::PartyView,
                    >(query.as_str())
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|party| party.camp_destination.is_some())
            }
        },
        None => false,
    };
    Redirect::to(&home_path(&character, party_is_camping)).into_response()
}

#[cfg(test)]
mod tests {
    use super::{character_location_path, home_path};

    fn character() -> crate::spacetimedb::CharacterView {
        crate::spacetimedb::CharacterView {
            id: 1,
            name: "Ada".into(),
            xp: 0,
            level: 1,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party-1".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
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
