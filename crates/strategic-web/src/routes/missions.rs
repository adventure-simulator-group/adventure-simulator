//! Mission route handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::models::ConnectedPlayer;
use crate::services;
use crate::session::Session;
use crate::templates::mission::{mission_status_fragment, mission_status_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/missions/enter", post(enter_mission))
        .route("/missions/{id}/status", get(mission_status))
        .route("/missions/{id}/cancel", post(cancel_mission))
        .route(
            "/internal/missions/{id}/ready",
            post(internal_mission_ready),
        )
        .route(
            "/internal/missions/{id}/players/{character_id}/loadout",
            get(internal_player_loadout),
        )
        .route(
            "/internal/missions/{id}/players/{character_id}/enter",
            post(internal_player_enter),
        )
        .route(
            "/internal/missions/{id}/players/{character_id}/leave",
            post(internal_player_leave),
        )
        .route(
            "/internal/missions/{id}/result",
            post(internal_mission_result),
        )
}

#[derive(Deserialize)]
struct StatusQuery {
    #[serde(default)]
    fragment: bool,
}

async fn enter_mission(State(state): State<AppState>, session: Session) -> Redirect {
    let Some(character_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    else {
        return Redirect::to("/characters");
    };

    let launch =
        match services::request_tactical_mission(&state.db, &state.config, character_id).await {
            Ok(launch) => launch,
            Err(error) => {
                tracing::warn!("Failed to request mission for character {character_id}: {error}");
                return Redirect::to("/parties");
            }
        };

    if let Err(error) = services::spawn_tactical_server(&state.db, &state.config, &launch).await {
        tracing::error!("Failed to spawn tactical server for {}: {error}", launch.id);
        let _ = services::mark_mission_failed(&state.db, &launch.id).await;
    }

    Redirect::to(&format!("/missions/{}/status", launch.id))
}

async fn mission_status(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Query(query): Query<StatusQuery>,
    session: Session,
) -> impl IntoResponse {
    let Some(character_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    else {
        return Redirect::to("/characters").into_response();
    };

    let Some(viewer) = services::get_character(&state.db, character_id)
        .await
        .unwrap_or_default()
    else {
        return Redirect::to("/characters").into_response();
    };

    let Some(server) = services::get_mission_for_viewer(&state.db, &mission_id)
        .await
        .unwrap_or_default()
    else {
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    if !services::can_view_mission(&viewer, &server) {
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
    let Some(character_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    else {
        return Redirect::to("/characters").into_response();
    };

    let Some(viewer) = services::get_character(&state.db, character_id)
        .await
        .unwrap_or_default()
    else {
        return Redirect::to("/characters").into_response();
    };

    let Some(server) = services::get_mission_for_viewer(&state.db, &mission_id)
        .await
        .unwrap_or_default()
    else {
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    if !services::can_cancel_mission(&state.db, &viewer, &server)
        .await
        .unwrap_or(false)
    {
        return (
            StatusCode::FORBIDDEN,
            "Not authorized to cancel this mission",
        )
            .into_response();
    }

    let _ = services::cancel_mission(&state.db, &mission_id).await;

    if let Some(party_id) = server.party_id {
        Redirect::to(&format!("/parties/{}", party_id)).into_response()
    } else {
        Redirect::to("/").into_response()
    }
}

#[derive(Deserialize)]
struct MissionReadyPayload {
    addr: String,
    #[serde(default)]
    cert_digest: String,
}

#[derive(Deserialize)]
struct MissionResultPayload {
    success: bool,
    #[serde(default)]
    xp_gained: i64,
}

async fn internal_mission_ready(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Json(payload): Json<MissionReadyPayload>,
) -> impl IntoResponse {
    match services::mark_mission_ready(&state.db, &mission_id, payload.addr, payload.cert_digest)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn internal_player_loadout(
    State(state): State<AppState>,
    Path((mission_id, character_id)): Path<(String, u64)>,
) -> impl IntoResponse {
    let character_id = match services::u64_to_i64(character_id) {
        Ok(id) => id,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    match services::load_tactical_player_data(&state.db, &mission_id, character_id).await {
        Ok(player) => Json::<ConnectedPlayer>(player).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn internal_player_enter(
    State(state): State<AppState>,
    Path((mission_id, character_id)): Path<(String, u64)>,
) -> impl IntoResponse {
    let character_id = match services::u64_to_i64(character_id) {
        Ok(id) => id,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    match services::enter_tactical_mission(&state.db, &mission_id, character_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn internal_player_leave(
    State(state): State<AppState>,
    Path((mission_id, character_id)): Path<(String, u64)>,
) -> impl IntoResponse {
    let character_id = match services::u64_to_i64(character_id) {
        Ok(id) => id,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    match services::leave_tactical_mission(&state.db, &mission_id, character_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn internal_mission_result(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Json(payload): Json<MissionResultPayload>,
) -> impl IntoResponse {
    match services::commit_mission_result(
        &state.db,
        &mission_id,
        payload.success,
        payload.xp_gained,
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}
