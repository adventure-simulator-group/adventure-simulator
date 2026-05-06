//! Mission route handlers.

use std::collections::HashSet;
use std::net::IpAddr;

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::edgegap::{DeploymentStatus, EnvVar};
use crate::session::Session;
use crate::spacetimedb::{
    Character, DeploymentSession, Party, PartyMember, TacticalServer, TacticalServerRequest,
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

async fn enter_mission(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    session: Session,
) -> Redirect {
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

    if let Some(edgegap) = &state.edgegap {
        let members: Vec<PartyMember> = state
            .db
            .query(&format!(
                "SELECT * FROM party_member WHERE party_id = '{}'",
                party_id
            ))
            .await
            .unwrap_or_default();

        if members.is_empty() {
            let _ = state
                .db
                .call(
                    "mission_failed",
                    &[
                        json!(mission_id.clone()),
                        json!("party has no members; cannot deploy mission"),
                    ],
                )
                .await;
            return Redirect::to(&format!("/missions/{}/status", mission_id));
        }

        let user_ips = extract_client_ips(&headers, Some(remote_addr.ip()));
        let env_vars = vec![
            EnvVar {
                key: "MISSION_ID".to_string(),
                value: mission_id.clone(),
                is_hidden: false,
            },
            EnvVar {
                key: "SCENE_KEY".to_string(),
                value: scene_key,
                is_hidden: false,
            },
            EnvVar {
                key: "SPACETIMEDB_URL".to_string(),
                value: state.spacetimedb_host.clone(),
                is_hidden: false,
            },
            EnvVar {
                key: "SPACETIMEDB_MODULE".to_string(),
                value: state.spacetimedb_database.clone(),
                is_hidden: false,
            },
        ];
        let tags = vec![
            format!("mission:{}", mission_id),
            format!("party:{}", party_id),
            "mode:pve".to_string(),
        ];

        match edgegap.create_deployment(user_ips, env_vars, tags).await {
            Ok(request_id) => {
                if let Err(error) = state
                    .db
                    .call(
                        "register_deployment_session",
                        &[
                            json!(mission_id.clone()),
                            json!(request_id.clone()),
                            json!(party_id.clone()),
                        ],
                    )
                    .await
                {
                    tracing::error!(
                        "Deployment {} created for mission {}, but failed to persist session: {:?}",
                        request_id,
                        mission_id,
                        error
                    );
                    let _ = edgegap.stop_deployment(&request_id).await;
                    let _ = state
                        .db
                        .call(
                            "mission_failed",
                            &[
                                json!(mission_id.clone()),
                                json!("failed to persist deployment session"),
                            ],
                        )
                        .await;
                }
            }
            Err(error) => {
                tracing::error!("Failed to create Edgegap deployment: {}", error);
                let _ = state
                    .db
                    .call(
                        "mission_failed",
                        &[json!(mission_id.clone()), json!(format!("{error}"))],
                    )
                    .await;
            }
        }
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

    let Some(mut server) = get_mission_for_viewer(&state, &mission_id, &viewer).await else {
        return (StatusCode::NOT_FOUND, "Mission not found").into_response();
    };

    if !can_view_mission(&viewer, &server) {
        return (StatusCode::FORBIDDEN, "Not authorized for this mission").into_response();
    }

    let mut deployment_status: Option<String> = None;
    let mut deployment_error: Option<String> = None;

    let sessions: Vec<DeploymentSession> = state
        .db
        .query(&format!(
            "SELECT * FROM deployment_session WHERE mission_id = '{}'",
            mission_id
        ))
        .await
        .unwrap_or_default();

    if let Some(session_row) = sessions.first() {
        deployment_status = Some(session_row.status.clone());
        deployment_error = session_row.last_error.clone();

        if server.status.eq_ignore_ascii_case("pending") {
            if let Some(edgegap) = &state.edgegap {
                match edgegap.get_status(&session_row.request_id).await {
                    Ok(status) => {
                        let observed_status = status.status_label();
                        let observed_error = status.last_error();
                        deployment_status = Some(observed_status.clone());
                        if observed_error.is_some() {
                            deployment_error = observed_error.clone();
                        }

                        persist_deployment_status(
                            &state,
                            &mission_id,
                            &observed_status,
                            observed_error.as_deref(),
                        )
                        .await;

                        if status.is_failed() {
                            let reason = observed_error.unwrap_or_else(|| {
                                format!("Edgegap deployment failed ({observed_status})")
                            });
                            let _ = state
                                .db
                                .call(
                                    "mission_failed",
                                    &[json!(mission_id.clone()), json!(reason.clone())],
                                )
                                .await;
                            deployment_error = Some(reason);
                            server.status = "Failed".to_string();
                        } else {
                            enrich_ready_hint_from_status(&status, &mut deployment_status);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Failed to poll Edgegap deployment status for mission {}: {}",
                            mission_id,
                            error
                        );
                    }
                }
            }
        }
    }

    if query.fragment {
        Html(
            mission_status_fragment(
                &server,
                deployment_status.as_deref(),
                deployment_error.as_deref(),
            )
            .into_string(),
        )
        .into_response()
    } else {
        Html(
            mission_status_page(
                &server,
                deployment_status.as_deref(),
                deployment_error.as_deref(),
                Some(viewer.name.as_str()),
                session.theme(),
            )
            .into_string(),
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

    if let Some(edgegap) = &state.edgegap {
        let sessions: Vec<DeploymentSession> = state
            .db
            .query(&format!(
                "SELECT * FROM deployment_session WHERE mission_id = '{}'",
                mission_id
            ))
            .await
            .unwrap_or_default();

        if let Some(session_row) = sessions.first() {
            if let Err(error) = edgegap.stop_deployment(&session_row.request_id).await {
                tracing::warn!(
                    "Failed to stop Edgegap deployment {}: {}",
                    session_row.request_id,
                    error
                );
            }
        }
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

async fn persist_deployment_status(
    state: &AppState,
    mission_id: &str,
    status: &str,
    last_error: Option<&str>,
) {
    let _ = state
        .db
        .call(
            "update_deployment_session_status",
            &[
                json!(mission_id),
                json!(status),
                json!(last_error.map(ToString::to_string)),
            ],
        )
        .await;
}

fn enrich_ready_hint_from_status(
    status: &DeploymentStatus,
    deployment_status: &mut Option<String>,
) {
    let status_label = status.status_label();
    let normalized = status_label.to_ascii_uppercase();

    if normalized.contains("READY")
        && (status.fqdn.is_some() || status.public_ip.is_some() || status.ports.is_some())
    {
        *deployment_status = Some("READY".to_string());
    }
}

fn extract_client_ips(headers: &HeaderMap, fallback_ip: Option<IpAddr>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for header_name in ["x-forwarded-for", "x-real-ip"] {
        let Some(raw) = headers.get(header_name) else {
            continue;
        };

        let Ok(value) = raw.to_str() else {
            continue;
        };

        for item in value.split(',') {
            let ip = item.trim();
            if ip.is_empty() {
                continue;
            }

            let Ok(parsed) = ip.parse::<IpAddr>() else {
                continue;
            };

            let normalized = parsed.to_string();
            if seen.insert(normalized.clone()) {
                out.push(normalized);
            }
        }
    }

    if out.is_empty() {
        if let Some(ip) = fallback_ip {
            out.push(ip.to_string());
        }
    }

    out
}

fn chrono_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
