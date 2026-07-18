//! Mission route handlers.

use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header::ACCEPT},
    response::{Html, IntoResponse, Json, Redirect, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use super::{AppState, PartyAction, PartyActionOutcome, execute_or_request_party_action};
use crate::session::Session;
use crate::spacetimedb::{BattleResult, Character, Party, TacticalServer, TacticalServerRequest};
use crate::templates::mission::{mission_status_fragment, mission_status_page};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/missions/enter", post(enter_mission))
        .route("/missions/{id}/status", get(mission_status))
        .route("/api/missions/{id}/handoff", get(mission_handoff))
        .route("/missions/{id}/cancel", post(cancel_mission))
}

#[derive(Deserialize)]
struct StatusQuery {
    #[serde(default)]
    fragment: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum EnterMissionOutcome {
    Executed {
        mission_id: String,
        status_url: String,
        handoff_url: String,
    },
    ApprovalRequired {
        message: &'static str,
    },
    Failed {
        message: String,
    },
}

fn json_or_redirect(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().starts_with("application/json"))
        })
}

async fn enter_mission(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
) -> Response {
    let wants_json = json_or_redirect(&headers);
    macro_rules! fail {
        ($redirect:expr, $message:expr) => {{
            if wants_json {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(EnterMissionOutcome::Failed {
                        message: $message.into(),
                    }),
                )
                    .into_response();
            }
            return Redirect::to($redirect).into_response();
        }};
    }
    let Some(character_id) = session.character_id_u64() else {
        fail!("/characters", "No active character");
    };

    let character = match super::data::character(&state, character_id).await {
        Ok(Some(character)) => character,
        Ok(None) => fail!("/characters", "Active character not found"),
        Err(error) => {
            tracing::error!(%error, "failed to load mission character");
            fail!("/", "Strategic data is unavailable");
        }
    };

    let Some(party_id) = &character.party_id else {
        fail!("/", "Character has no party");
    };

    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();

    let Some(party) = parties.first() else {
        fail!("/", "Party not found");
    };

    let Some(quest_id) = &party.active_quest_id else {
        fail!("/", "Party has no active quest");
    };
    if character.current_quest_location_id.as_ref() != Some(quest_id) {
        fail!("/", "Party is not at the quest destination");
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
        fail!("/", error.clone());
    }

    match outcome.unwrap() {
        PartyActionOutcome::Executed if wants_json => Json(EnterMissionOutcome::Executed {
            status_url: format!("/missions/{mission_id}/status"),
            handoff_url: format!("/api/missions/{mission_id}/handoff"),
            mission_id,
        })
        .into_response(),
        PartyActionOutcome::Executed => {
            Redirect::to(&format!("/missions/{mission_id}/status")).into_response()
        }
        PartyActionOutcome::Requested if wants_json => (
            StatusCode::ACCEPTED,
            Json(EnterMissionOutcome::ApprovalRequired {
                message: "The party leader must approve combat before a server is allocated",
            }),
        )
            .into_response(),
        PartyActionOutcome::Requested => {
            Redirect::to("/?party-requested=initiate_combat").into_response()
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum HandoffStatus {
    Unauthorized,
    Pending {
        fallback_url: String,
    },
    Ready {
        server_url: String,
        player_id: u64,
        fallback_url: String,
    },
    Failed {
        fallback_url: String,
    },
    Ended {
        fallback_url: String,
    },
}

async fn mission_handoff(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    session: Session,
) -> Response {
    if !valid_mission_id(&mission_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(HandoffStatus::Failed {
                fallback_url: "/".into(),
            }),
        )
            .into_response();
    }
    let Some(character_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, Json(HandoffStatus::Unauthorized)).into_response();
    };
    let viewer = match super::data::character(&state, character_id).await {
        Ok(Some(viewer)) => viewer,
        Ok(None) => {
            return (StatusCode::UNAUTHORIZED, Json(HandoffStatus::Unauthorized)).into_response();
        }
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HandoffStatus::Failed {
                    fallback_url: "/".into(),
                }),
            )
                .into_response();
        }
    };
    if !can_view_mission_id(&viewer, &mission_id) {
        return (StatusCode::FORBIDDEN, Json(HandoffStatus::Unauthorized)).into_response();
    }
    let fallback_url = format!("/missions/{mission_id}/status");
    let status = if let Some(server) = get_ready_mission(&state, &mission_id).await {
        match server.status {
            crate::spacetimedb::MissionStatus::Ready
                if adventuresim_render_contracts::valid_tactical_server_address(&server.addr) =>
            {
                HandoffStatus::Ready {
                    server_url: server.addr,
                    player_id: viewer.id,
                    fallback_url,
                }
            }
            crate::spacetimedb::MissionStatus::Ready => HandoffStatus::Failed { fallback_url },
            crate::spacetimedb::MissionStatus::Failed => HandoffStatus::Failed { fallback_url },
            crate::spacetimedb::MissionStatus::Ended => HandoffStatus::Ended { fallback_url },
            crate::spacetimedb::MissionStatus::Pending => HandoffStatus::Pending { fallback_url },
        }
    } else {
        let requests: Vec<TacticalServerRequest> = state
            .db
            .query(&format!(
                "SELECT * FROM tactical_server_request WHERE mission_id = '{}'",
                mission_id
            ))
            .await
            .unwrap_or_default();
        if requests.is_empty() {
            let results: Vec<BattleResult> = state
                .db
                .query(&format!(
                    "SELECT * FROM battle_result WHERE mission_id = '{}'",
                    mission_id
                ))
                .await
                .unwrap_or_default();
            if results
                .iter()
                .any(|result| viewer.party_id.as_deref() == Some(&result.party_id))
            {
                return Json(HandoffStatus::Ended { fallback_url }).into_response();
            }
            return (
                StatusCode::NOT_FOUND,
                Json(HandoffStatus::Failed { fallback_url }),
            )
                .into_response();
        }
        HandoffStatus::Pending { fallback_url }
    };
    Json(status).into_response()
}

async fn mission_status(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Query(query): Query<StatusQuery>,
    session: Session,
) -> impl IntoResponse {
    if !valid_mission_id(&mission_id) {
        return (StatusCode::BAD_REQUEST, "Invalid mission ID").into_response();
    }
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

    if !can_view_mission_id(&viewer, &mission_id) {
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

fn can_view_mission_id(viewer: &Character, mission_id: &str) -> bool {
    let Some((mission_party, nonce)) = mission_id
        .strip_prefix("party-")
        .and_then(|value| value.rsplit_once('-'))
    else {
        return false;
    };
    nonce.bytes().all(|byte| byte.is_ascii_digit())
        && viewer.party_id.as_deref() == Some(mission_party)
}

fn valid_mission_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer(party_id: Option<&str>) -> Character {
        Character {
            id: 7,
            name: "Viewer".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_quest_location_id: None,
            party_id: party_id.map(str::to_owned),
            age_years: 20,
            alive: true,
            temporary: false,
        }
    }

    #[test]
    fn json_accept_is_explicit_and_html_remains_default() {
        let mut headers = HeaderMap::new();
        assert!(!json_or_redirect(&headers));
        headers.insert(ACCEPT, "text/html,application/xhtml+xml".parse().unwrap());
        assert!(!json_or_redirect(&headers));
        headers.insert(ACCEPT, "application/json".parse().unwrap());
        assert!(json_or_redirect(&headers));
    }

    #[test]
    fn handoff_authorization_is_party_scoped_and_ids_are_sql_safe() {
        let party_viewer = viewer(Some("party-42"));
        assert!(can_view_mission_id(&party_viewer, "party-party-42-123456"));
        assert!(!can_view_mission_id(
            &party_viewer,
            "party-party-420-123456"
        ));
        assert!(!can_view_mission_id(
            &viewer(Some("party")),
            "party-party-42-123456"
        ));
        assert!(!can_view_mission_id(&viewer(None), "party-party-42-123456"));
        assert!(valid_mission_id("party-party-42-123456"));
        assert!(!valid_mission_id("party-x-' OR 1=1"));
    }

    #[test]
    fn handoff_outcomes_are_closed_typed_json() {
        assert_eq!(
            serde_json::to_value(EnterMissionOutcome::Executed {
                mission_id: "mission-1".into(),
                status_url: "/missions/mission-1/status".into(),
                handoff_url: "/api/missions/mission-1/handoff".into(),
            })
            .unwrap()["outcome"],
            "executed"
        );
        let ready = HandoffStatus::Ready {
            server_url: "127.0.0.1:6000".into(),
            player_id: 7,
            fallback_url: "/missions/mission-1/status".into(),
        };
        let value = serde_json::to_value(ready).unwrap();
        assert_eq!(value["status"], "ready");
        assert_eq!(value["player_id"], 7);
        assert!(value.get("mission_id").is_none());
        assert_eq!(
            serde_json::to_value(HandoffStatus::Pending {
                fallback_url: "/missions/mission-1/status".into()
            })
            .unwrap()["status"],
            "pending"
        );
        assert_eq!(
            serde_json::to_value(HandoffStatus::Ended {
                fallback_url: "/missions/mission-1/status".into()
            })
            .unwrap()["status"],
            "ended"
        );
    }
}
