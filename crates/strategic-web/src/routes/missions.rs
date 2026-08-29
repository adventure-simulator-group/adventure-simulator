//! Mission route handlers.

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::{AppState, PartyAction, PartyActionOutcome, execute_or_request_party_action};
use crate::session::Session;
use crate::spacetimedb::{
    BackendCaseSitePin, BackendContract, BattleResult, CharacterView, MissionServerRequestView,
    MissionServerView, PartyView, sql_string_literal,
};
use crate::templates::mission::{mission_status_fragment, mission_status_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/missions/enter", post(enter_mission))
        .route("/missions/{id}/status", get(mission_status))
        .route("/missions/{id}/cancel", post(cancel_mission))
}

#[derive(Deserialize)]
struct StatusQuery {
    #[serde(default)]
    fragment: bool,
}

async fn enter_mission(State(state): State<AppState>, session: Session) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let character = match super::data::character(&state, character_id).await {
        Ok(Some(character)) => character,
        Ok(None) => return Redirect::to("/characters"),
        Err(error) => {
            tracing::error!(%error, "failed to load mission character");
            return Redirect::to("/");
        }
    };

    let Some(party_id) = &character.party_id else {
        return Redirect::to("/");
    };

    let parties: Vec<PartyView> = state
        .db
        .query_sats_into::<adventuresim_stdb_client::Party, PartyView>(
            &crate::spacetimedb::party_by_id(party_id),
        )
        .await
        .unwrap_or_default();

    let Some(party) = parties.first() else {
        return Redirect::to("/");
    };

    let Some(quest_id) = &party.active_contract_id else {
        return Redirect::to("/");
    };
    let contract = state
        .db
        .query_one_sats::<BackendContract>(&crate::spacetimedb::contract_by_id(quest_id))
        .await
        .ok()
        .flatten();
    let Some(contract) = contract else {
        return Redirect::to("/");
    };
    let Some(case_site_id) = character.current_case_site_id.as_deref() else {
        return Redirect::to("/");
    };
    let site = state
        .db
        .query_one_sats::<BackendCaseSitePin>(&format!(
            "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {character_id} AND case_site_id = {}",
            sql_string_literal(case_site_id)
        ))
        .await
        .ok()
        .flatten();
    let Some(site) = site.filter(|site| site.case_id == contract.case_id) else {
        return Redirect::to("/");
    };
    let scene_key = site.scene_key;
    let mission_id = format!("mission:party-{}-{}", party_id, super::data::new_id());

    let outcome = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::RequestTacticalServer {
            mission_id: mission_id.clone(),
            scene_key: scene_key.clone(),
        },
    )
    .await;
    if let Err(ref error) = outcome {
        tracing::error!("Failed to request mission: {:?}", error);
        return Redirect::to("/");
    }

    match outcome.unwrap() {
        PartyActionOutcome::Executed => Redirect::to(&format!("/missions/{mission_id}/status")),
        PartyActionOutcome::Requested => Redirect::to("/?party-requested=initiate_combat"),
    }
}

async fn mission_status(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Query(query): Query<StatusQuery>,
    session: Session,
) -> impl IntoResponse {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };

    let viewer = match super::data::character(&state, character_id).await {
        Ok(Some(viewer)) => viewer,
        Ok(None) => return Redirect::to("/characters").into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to load mission viewer");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Strategic data is unavailable",
            )
                .into_response();
        }
    };

    let server = match get_mission_for_viewer(&state, &mission_id).await {
        Ok(server) => server,
        Err(error) => {
            tracing::error!(%error, "failed to load mission");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Strategic data is unavailable",
            )
                .into_response();
        }
    };
    let Some(mut server) = server else {
        let results: crate::spacetimedb::Result<Vec<BattleResult>> = state
            .db
            .query_sats(&crate::spacetimedb::battle_result_by_battle_id(&format!(
                "battle:{mission_id}"
            )))
            .await
            .map_err(|error| {
                tracing::error!(%error, "failed to load mission result");
                error
            });
        let results = match results {
            Ok(results) => results,
            Err(_) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Strategic data is unavailable",
                )
                    .into_response();
            }
        };
        if let Some(_result) = results
            .first()
            .filter(|result| viewer.party_id.as_deref() == Some(&result.party_id))
        {
            return viewer.current_case_site_id.as_deref().map_or_else(
                || Redirect::to("/").into_response(),
                |case_site_id| {
                    Redirect::to(&format!("/locations/case-site/{case_site_id}/enemy"))
                        .into_response()
                },
            );
        }
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    if !can_view_mission(&viewer, &server) {
        return (StatusCode::FORBIDDEN, "Not authorized for this mission").into_response();
    }
    present_mission_to_viewer(&mut server, &viewer);

    if query.fragment {
        Html(mission_status_fragment(&server).into_string()).into_response()
    } else {
        Html(mission_status_page(&server, Some(viewer.name.as_str())).into_string()).into_response()
    }
}

async fn cancel_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    session: Session,
) -> impl IntoResponse {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };

    let viewer = match super::data::character(&state, character_id).await {
        Ok(Some(viewer)) => viewer,
        Ok(None) => return Redirect::to("/characters").into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to load mission viewer");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Strategic data is unavailable",
            )
                .into_response();
        }
    };

    let server = match get_mission_for_viewer(&state, &mission_id).await {
        Ok(server) => server,
        Err(error) => {
            tracing::error!(%error, "failed to load mission");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Strategic data is unavailable",
            )
                .into_response();
        }
    };
    let Some(server) = server else {
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };
    if !can_view_mission(&viewer, &server) {
        return (StatusCode::FORBIDDEN, "Not authorized for this mission").into_response();
    }

    let _ = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::CancelMission { mission_id },
    )
    .await;

    Redirect::to("/").into_response()
}

async fn get_mission_for_viewer(
    state: &AppState,
    mission_id: &str,
) -> crate::spacetimedb::Result<Option<MissionServerView>> {
    if let Some(server) = get_ready_mission(state, mission_id).await? {
        return Ok(Some(server));
    }

    let requests: Vec<MissionServerRequestView> = state
        .db
        .query_sats_into::<adventuresim_stdb_client::TacticalServerRequest, MissionServerRequestView>(
            &crate::spacetimedb::tactical_server_request_by_mission_id(mission_id),
        )
        .await?;

    Ok(requests.first().map(|request| {
        MissionServerView::pending(
            request.mission_id.clone(),
            request.gateway_bucket,
            request.scene_key.clone(),
            request.party_id.clone(),
        )
    }))
}

async fn get_ready_mission(
    state: &AppState,
    mission_id: &str,
) -> crate::spacetimedb::Result<Option<MissionServerView>> {
    state
        .db
        .query_one_sats_into::<adventuresim_stdb_client::TacticalServer, MissionServerView>(
            &crate::spacetimedb::tactical_server_by_mission_id(mission_id),
        )
        .await
}

fn can_view_mission(viewer: &CharacterView, server: &MissionServerView) -> bool {
    viewer.party_id.as_deref() == Some(server.party_id.as_str())
}

fn present_mission_to_viewer(server: &mut MissionServerView, viewer: &CharacterView) {
    debug_assert!(can_view_mission(viewer, server));
    server.character_id = Some(viewer.id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(id: u64, party_id: &str) -> CharacterView {
        CharacterView {
            id,
            name: "viewer".into(),
            xp: 0,
            level: 1,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some(party_id.into()),
            age_years: 18,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    fn mission(party_id: &str) -> MissionServerView {
        MissionServerView::pending("mission".into(), 0, "hills".into(), party_id.into())
    }

    #[test]
    fn mission_visibility_denies_cross_party_viewers_without_relabeling() {
        let server = mission("party-a");
        assert!(can_view_mission(&character(2, "party-a"), &server));
        assert!(!can_view_mission(&character(3, "party-b"), &server));
        assert_eq!(server.party_id, "party-a");
    }

    #[test]
    fn pending_and_ready_links_use_the_current_authorized_viewer() {
        let viewer = character(42, "party-a");
        for mut server in [
            mission("party-a"),
            MissionServerView {
                status: crate::spacetimedb::MissionStatus::Ready,
                ..mission("party-a")
            },
        ] {
            assert_eq!(server.character_id, None);
            present_mission_to_viewer(&mut server, &viewer);
            assert_eq!(server.character_id, Some(42));
            assert_eq!(server.party_id, "party-a");
        }
    }
}
