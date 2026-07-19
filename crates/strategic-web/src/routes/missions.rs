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
use crate::spacetimedb::{BattleResult, Character, Party, TacticalServer, TacticalServerRequest};
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

    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();

    let Some(party) = parties.first() else {
        return Redirect::to("/");
    };

    let Some(quest_id) = &party.active_quest_id else {
        return Redirect::to("/");
    };
    if character.current_quest_location_id.as_ref() != Some(quest_id) {
        return Redirect::to("/");
    }

    let scene_key = quest_scene_key(&state, quest_id)
        .await
        .unwrap_or_else(|| "hills".to_string());
    let mission_id = format!("party-{}-{}", party_id, super::data::new_id());

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

    let Some(server) = get_mission_for_viewer(&state, &mission_id, &viewer).await else {
        let results: Vec<BattleResult> = state
            .db
            .query(&format!(
                "SELECT * FROM battle_result WHERE mission_id = '{}'",
                mission_id
            ))
            .await
            .unwrap_or_default();
        if let Some(result) = results
            .first()
            .filter(|result| viewer.party_id.as_deref() == Some(&result.party_id))
        {
            return Redirect::to(&format!("/locations/quest/{}/loot", result.quest_id))
                .into_response();
        }
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    if !can_view_mission(&viewer, &server) {
        return (StatusCode::FORBIDDEN, "Not authorized for this mission").into_response();
    }

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

    let Some(_server) = get_mission_for_viewer(&state, &mission_id, &viewer).await else {
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    let _ = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::CancelMission { mission_id },
    )
    .await;

    Redirect::to("/").into_response()
}

async fn quest_scene_key(state: &AppState, quest_id: &str) -> Option<String> {
    let quests: Vec<crate::spacetimedb::Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", quest_id))
        .await
        .ok()?;
    let quest = quests.first()?;
    Some(quest.location_scene_key.clone())
}

async fn get_mission_for_viewer(
    state: &AppState,
    mission_id: &str,
    viewer: &Character,
) -> Option<TacticalServer> {
    if let Some(mut server) = get_ready_mission(state, mission_id).await {
        server.character_id = Some(viewer.id);
        server.party_id = viewer.party_id.clone();
        return Some(server);
    }

    let requests: Vec<TacticalServerRequest> = state
        .db
        .query(&format!(
            "SELECT * FROM tactical_server_request WHERE mission_id = '{}'",
            mission_id
        ))
        .await
        .ok()?;

    requests.first().map(|request| {
        TacticalServer::pending(
            request.mission_id.clone(),
            request.scene_key.clone(),
            viewer.id,
            viewer.party_id.clone(),
        )
    })
}

async fn get_ready_mission(state: &AppState, mission_id: &str) -> Option<TacticalServer> {
    let servers: Vec<TacticalServer> = state
        .db
        .query(&format!(
            "SELECT * FROM tactical_server WHERE mission_id = '{}'",
            mission_id
        ))
        .await
        .ok()?;
    servers.into_iter().next()
}

fn can_view_mission(viewer: &Character, server: &TacticalServer) -> bool {
    if server.character_id == Some(viewer.id) {
        return true;
    }

    match (&viewer.party_id, &server.party_id) {
        (Some(viewer_party), Some(server_party)) => viewer_party == server_party,
        _ => false,
    }
}
