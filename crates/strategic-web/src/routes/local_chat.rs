use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AppState;
use crate::{
    session::Session,
    spacetimedb::{Character, LocalChatMessage},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/local-chat/{kind}/{subject_id}",
            get(messages).post(send_message),
        )
        .route(
            "/api/local-chat/{kind}/{subject_id}/npc",
            post(record_npc_message),
        )
        .route("/api/local-chat/incoming", get(incoming))
}

#[derive(Serialize)]
struct LocalChatResponse {
    messages: Vec<LocalChatMessage>,
}

#[derive(Deserialize)]
struct MessageForm {
    body: String,
    #[serde(default)]
    speaker: String,
}

async fn actor_and_key(
    state: &AppState,
    actor_id: u64,
    kind: &str,
    subject_id: &str,
) -> Result<(Character, String), String> {
    let actor = state
        .db
        .query::<Character>(&format!("SELECT * FROM character WHERE id = {actor_id}"))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("Character not found")?;
    let party_id = actor.party_id.as_deref().ok_or("Character has no party")?;
    let key = match kind {
        "npc" => {
            let settlement = actor
                .current_settlement_id
                .as_deref()
                .ok_or("NPC is not local")?;
            if !subject_id.starts_with(&format!("{settlement}:")) {
                return Err("NPC is not local".into());
            }
            format!("npc:{party_id}:{subject_id}")
        }
        "player" => {
            let id: u64 = subject_id.parse().map_err(|_| "Invalid player")?;
            let subject = state
                .db
                .query::<Character>(&format!("SELECT * FROM character WHERE id = {id}"))
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .next()
                .ok_or("Player not found")?;
            if actor.current_settlement_id != subject.current_settlement_id
                || actor.current_quest_location_id != subject.current_quest_location_id
                || (actor.current_settlement_id.is_none()
                    && actor.current_quest_location_id.is_none())
            {
                return Err("Player is not at this location".into());
            }
            let other = subject.party_id.as_deref().ok_or("Player has no party")?;
            let (a, b) = if party_id <= other {
                (party_id, other)
            } else {
                (other, party_id)
            };
            format!("players:{a}:{b}")
        }
        _ => return Err("Unknown Local subject".into()),
    };
    Ok((actor, key))
}

async fn messages(
    State(state): State<AppState>,
    Path((kind, subject_id)): Path<(String, String)>,
    session: Session,
) -> Result<Json<LocalChatResponse>, (StatusCode, String)> {
    let actor_id = session
        .character_id_u64()
        .ok_or((StatusCode::UNAUTHORIZED, "Choose a character".into()))?;
    let (_, key) = actor_and_key(&state, actor_id, &kind, &subject_id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;
    let mut messages = state
        .db
        .query::<LocalChatMessage>(&format!(
            "SELECT * FROM local_chat_message WHERE conversation_key = '{}'",
            key
        ))
        .await
        .unwrap_or_default();
    messages.sort_by_key(|message| (message.created_micros, message.id));
    Ok(Json(LocalChatResponse { messages }))
}

async fn send_message(
    State(state): State<AppState>,
    Path((kind, subject_id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<MessageForm>,
) -> StatusCode {
    let Some(actor_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED;
    };
    match state
        .db
        .call(
            "send_local_chat_message",
            &[
                json!(actor_id),
                json!(kind),
                json!(subject_id),
                json!(form.body),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

async fn record_npc_message(
    State(state): State<AppState>,
    Path((kind, subject_id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<MessageForm>,
) -> StatusCode {
    if kind != "npc" {
        return StatusCode::BAD_REQUEST;
    }
    let Some(actor_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED;
    };
    match state
        .db
        .call(
            "record_local_npc_message",
            &[
                json!(actor_id),
                json!(subject_id),
                json!(form.speaker),
                json!(form.body),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

#[derive(Serialize)]
struct IncomingPlayer {
    id: String,
    name: String,
}

async fn incoming(State(state): State<AppState>, session: Session) -> Json<Vec<IncomingPlayer>> {
    let Some(actor_id) = session.character_id_u64() else {
        return Json(Vec::new());
    };
    let Some(actor) = state
        .db
        .query::<Character>(&format!("SELECT * FROM character WHERE id = {actor_id}"))
        .await
        .ok()
        .and_then(|characters| characters.into_iter().next())
    else {
        return Json(Vec::new());
    };
    let Some(party_id) = actor.party_id.as_deref() else {
        return Json(Vec::new());
    };
    let memberships = state
        .db
        .query::<crate::spacetimedb::PartyMember>(&format!(
            "SELECT * FROM party_member WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let own: std::collections::HashSet<u64> =
        memberships.into_iter().map(|m| m.character_id).collect();
    let characters = state
        .db
        .query::<Character>("SELECT * FROM character")
        .await
        .unwrap_or_default();
    let all_messages = state
        .db
        .query::<LocalChatMessage>("SELECT * FROM local_chat_message")
        .await
        .unwrap_or_default();
    let mut ids = std::collections::BTreeSet::new();
    for message in all_messages.iter().filter(|m| {
        m.conversation_key.starts_with("players:")
            && m.conversation_key
                .split(':')
                .skip(1)
                .any(|id| id == party_id)
    }) {
        if message.sender_id != 0 && !own.contains(&message.sender_id) {
            ids.insert(message.sender_id);
        }
    }
    Json(
        characters
            .into_iter()
            .filter(|c| {
                ids.contains(&c.id)
                    && c.current_settlement_id == actor.current_settlement_id
                    && c.current_quest_location_id == actor.current_quest_location_id
            })
            .map(|c| IncomingPlayer {
                id: c.id.to_string(),
                name: c.name,
            })
            .collect(),
    )
}
