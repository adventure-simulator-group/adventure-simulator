use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{AppState, character_case_site_id};
use crate::{
    session::Session,
    spacetimedb::{Character, CharacterTime, LocalChatMessage, sql_string_literal},
};

const MAX_CHAT_HISTORY: usize = 200;
const MAX_INCOMING_PLAYERS: usize = 50;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/local-chat/{kind}/{subject_id}",
            get(messages).post(send_message),
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
}

#[derive(Deserialize)]
struct LocalNpcRow {
    id: String,
    home_settlement_id: String,
}

#[derive(Deserialize)]
struct LocalNpcPresenceRow {
    npc_id: String,
    settlement_id: String,
    location_id: String,
    start_minute: u16,
    end_minute: u16,
}

fn npc_authority_matches(
    settlement_id: &str,
    npc: &LocalNpcRow,
    presence: &LocalNpcPresenceRow,
    minute: u64,
) -> bool {
    let minute = (minute % 1_440) as u16;
    npc.id == presence.npc_id
        && npc.home_settlement_id == settlement_id
        && presence.settlement_id == settlement_id
        && !presence.location_id.is_empty()
        && presence.start_minute <= minute
        && minute < presence.end_minute
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
            if subject_id.chars().count() > 160
                || subject_id.chars().any(char::is_control)
                || subject_id.is_empty()
            {
                return Err("NPC is not local".into());
            }
            let npc = state
                .db
                .query_one::<LocalNpcRow>(&format!(
                    "SELECT * FROM settlement_npc WHERE id = {}",
                    sql_string_literal(subject_id)
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("NPC is not local")?;
            let presence = state
                .db
                .query_one::<LocalNpcPresenceRow>(&format!(
                    "SELECT * FROM settlement_npc_presence WHERE npc_id = {}",
                    sql_string_literal(subject_id)
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("NPC is not local")?;
            let minute = state
                .db
                .query_one::<CharacterTime>(&format!(
                    "SELECT * FROM character_time WHERE character_id = {}",
                    actor.id
                ))
                .await
                .map_err(|error| error.to_string())?
                .map_or(720, |time| time.minutes);
            if !npc_authority_matches(settlement, &npc, &presence, minute) {
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
            let actor_site = character_case_site_id(state, actor.id).await?;
            let subject_site = character_case_site_id(state, subject.id).await?;
            if actor.current_settlement_id != subject.current_settlement_id
                || actor_site != subject_site
                || (actor.current_settlement_id.is_none() && actor_site.is_none())
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
            "SELECT * FROM local_chat_message WHERE conversation_key = {}",
            sql_string_literal(&key)
        ))
        .await
        .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    messages.sort_by_key(|message| (message.created_micros, message.id));
    if messages.len() > MAX_CHAT_HISTORY {
        messages.drain(..messages.len() - MAX_CHAT_HISTORY);
    }
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
            "SELECT * FROM party_member WHERE party_id = {}",
            sql_string_literal(party_id)
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
    let actor_site = character_case_site_id(&state, actor.id)
        .await
        .ok()
        .flatten();
    let mut candidate_sites = std::collections::HashMap::new();
    for id in ids.iter().copied().take(MAX_INCOMING_PLAYERS) {
        candidate_sites.insert(id, character_case_site_id(&state, id).await.ok().flatten());
    }
    Json(
        characters
            .into_iter()
            .filter(|c| {
                ids.contains(&c.id)
                    && c.current_settlement_id == actor.current_settlement_id
                    && candidate_sites.get(&c.id) == Some(&actor_site)
            })
            .take(MAX_INCOMING_PLAYERS)
            .map(|c| IncomingPlayer {
                id: c.id.to_string(),
                name: c.name,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{LocalNpcPresenceRow, LocalNpcRow, npc_authority_matches};

    #[test]
    fn riverdale_inn_npc_chain_uses_authority_not_encoded_id_shape() {
        let npc = LocalNpcRow {
            id: "npc:riverdale:inn:0".into(),
            home_settlement_id: "riverdale".into(),
        };
        let mut presence = LocalNpcPresenceRow {
            npc_id: npc.id.clone(),
            settlement_id: "riverdale".into(),
            location_id: "inn".into(),
            start_minute: 0,
            end_minute: 1_440,
        };
        assert!(npc_authority_matches("riverdale", &npc, &presence, 720));

        presence.settlement_id = "ironforge".into();
        assert!(!npc_authority_matches("riverdale", &npc, &presence, 720));
        presence.settlement_id = "riverdale".into();
        presence.end_minute = 600;
        assert!(!npc_authority_matches("riverdale", &npc, &presence, 720));
    }

    #[test]
    fn npc_selection_waits_for_a_subject_and_preserves_reducer_diagnostics() {
        let javascript = include_str!("../../static/local-chat.js");
        assert!(javascript.contains("if (!kind || !subject) return null"));
        assert!(javascript.matches("if (!endpoint) return;").count() >= 2);
        assert!(!javascript.contains("/api/local-chat/${encodeURIComponent(node.dataset"));

        let local_route = include_str!("local_chat.rs")
            .split("async fn actor_and_key")
            .nth(1)
            .and_then(|tail| tail.split("async fn messages").next())
            .expect("local chat authority handler");
        assert!(local_route.contains("SELECT * FROM settlement_npc WHERE id = {}"));
        assert!(local_route.contains("SELECT * FROM settlement_npc_presence WHERE npc_id = {}"));
        assert!(!local_route.contains("subject_id.starts_with"));

        let dialogue_route = include_str!("dialogue.rs");
        assert!(dialogue_route.contains("start_dialogue reducer rejected an NPC encounter"));
        assert!(dialogue_route.contains("StatusCode::CONFLICT"));
    }
}
