//! Mission route handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{Character, Party, TacticalServer, TacticalServerRequest};
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

    let Some(character) = get_character(&state, character_id).await else {
        return Redirect::to("/characters");
    };

    let Some(party_id) = &character.party_id else {
        return Redirect::to("/parties");
    };

    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();

    let Some(party) = parties.first() else {
        return Redirect::to("/parties");
    };

    if party.leader_id != character_id {
        return Redirect::to(&format!("/parties/{}", party_id));
    }

    let Some(quest_id) = &party.active_quest_id else {
        return Redirect::to(&format!("/parties/{}", party_id));
    };

    let scene_key = quest_scene_key(&state, quest_id)
        .await
        .unwrap_or_else(|| "hills".to_string());
    let mission_id = format!("party-{}-{}", party_id, chrono_id());

    if let Err(error) = state
        .db
        .call(
            "request_tactical_server",
            &[json!(mission_id.clone()), json!(scene_key.clone())],
        )
        .await
    {
        tracing::error!("Failed to request mission: {:?}", error);
        return Redirect::to(&format!("/parties/{}", party_id));
    }

    Redirect::to(&format!("/missions/{}/status", mission_id))
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

    let Some(viewer) = get_character(&state, character_id).await else {
        return Redirect::to("/characters").into_response();
    };

    let Some(server) = get_mission_for_viewer(&state, &mission_id, &viewer).await else {
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    if !can_view_mission(&viewer, &server) {
        return (StatusCode::FORBIDDEN, "Not authorized for this mission").into_response();
    }

    if query.fragment {
        Html(mission_status_fragment(&server).into_string()).into_response()
    } else {
        Html(
            mission_status_page(&server, Some(viewer.name.as_str()), session.theme()).into_string(),
        )
        .into_response()
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

    let Some(viewer) = get_character(&state, character_id).await else {
        return Redirect::to("/characters").into_response();
    };

    let Some(server) = get_mission_for_viewer(&state, &mission_id, &viewer).await else {
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    if !can_cancel_mission(&state, &viewer, &server).await {
        return (
            StatusCode::FORBIDDEN,
            "Not authorized to cancel this mission",
        )
            .into_response();
    }

    let _ = state
        .db
        .call("cancel_mission_request", &[json!(mission_id)])
        .await;

    if let Some(party_id) = server.party_id {
        Redirect::to(&format!("/parties/{}", party_id)).into_response()
    } else {
        Redirect::to("/").into_response()
    }
}

async fn quest_scene_key(state: &AppState, quest_id: &str) -> Option<String> {
    let quests: Vec<crate::spacetimedb::Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", quest_id))
        .await
        .ok()?;
    let quest = quests.first()?;
    let settlements: Vec<crate::spacetimedb::Settlement> = state
        .db
        .query(&format!(
            "SELECT * FROM settlement WHERE id = '{}'",
            quest.settlement_id
        ))
        .await
        .ok()?;
    settlements.first().map(|s| s.scene_key.clone())
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

async fn can_cancel_mission(state: &AppState, viewer: &Character, server: &TacticalServer) -> bool {
    if let Some(party_id) = &server.party_id {
        let parties: Vec<Party> = state
            .db
            .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default();

        if let Some(party) = parties.first() {
            return party.leader_id == viewer.id;
        }

        return server.character_id == Some(viewer.id);
    }

    server.character_id == Some(viewer.id)
}

async fn get_character(state: &AppState, character_id: u64) -> Option<Character> {
    let characters: Vec<Character> = state
        .db
        .query(&format!(
            "SELECT * FROM character WHERE id = {}",
            character_id
        ))
        .await
        .ok()?;
    characters.into_iter().next()
}

fn chrono_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
