use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{AppState, character_case_site_id};
use crate::{
    session::Session,
    spacetimedb::{
        BackendLocalChatMessage, Character, CharacterTime, Settlement, SettlementCategory,
        sql_string_literal,
    },
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

#[derive(Serialize)]
struct LocalChatMessage {
    id: u64,
    sender_id: u64,
    sender_name: String,
    body: String,
    created_micros: i64,
}

impl From<BackendLocalChatMessage> for LocalChatMessage {
    fn from(message: BackendLocalChatMessage) -> Self {
        Self {
            id: message.id,
            sender_id: message.sender_id,
            sender_name: message.sender_name,
            body: message.body,
            created_micros: message.created_micros,
        }
    }
}

#[derive(Deserialize)]
struct MessageForm {
    body: String,
    #[serde(default)]
    location_id: String,
}

#[derive(Default, Deserialize)]
struct LocationQuery {
    #[serde(default)]
    location_id: String,
}

#[derive(Deserialize)]
struct LocalNpcRow {
    character_id: u64,
    home_settlement_id: String,
}

#[derive(Deserialize)]
struct LocalNpcPresenceRow {
    character_id: u64,
    settlement_id: String,
    location_id: String,
    start_minute: u16,
    end_minute: u16,
}

fn npc_authority_matches(
    settlement_id: &str,
    npc: &LocalNpcRow,
    presence: &LocalNpcPresenceRow,
    requested_location_id: &str,
    minute: u64,
) -> bool {
    let minute = (minute % 1_440) as u16;
    npc.character_id == presence.character_id
        && npc.home_settlement_id == settlement_id
        && presence.settlement_id == settlement_id
        && presence.location_id == requested_location_id
        && !requested_location_id.is_empty()
        && presence.start_minute <= minute
        && minute < presence.end_minute
}

fn npc_history_location_is_navigable(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    category: &SettlementCategory,
    settlement_id: &str,
    location_id: &str,
) -> bool {
    let has_keep = matches!(
        category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    );
    adventuresim_core::settlement_economy::npc_location_is_navigable(
        profile,
        has_keep,
        settlement_id,
        location_id,
    )
}

enum ConversationSelector {
    Npc(String),
    PlayerParty(String),
}

async fn actor_and_selector(
    state: &AppState,
    actor_id: u64,
    kind: &str,
    subject_id: &str,
    location_id: &str,
) -> Result<(Character, ConversationSelector), String> {
    let actor = state
        .db
        .query::<Character>(&format!(
            "SELECT * FROM backend_characters WHERE id = {actor_id}"
        ))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("Character not found")?;
    actor.party_id.as_deref().ok_or("Character has no party")?;
    let selector = match kind {
        "npc" => {
            let settlement = actor
                .current_settlement_id
                .as_deref()
                .ok_or("NPC is not local")?;
            let settlement_authority = state
                .db
                .query_one::<Settlement>(&format!(
                    "SELECT * FROM settlement WHERE id = {}",
                    sql_string_literal(settlement)
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("NPC is not local")?;
            if !npc_history_location_is_navigable(
                &settlement_authority.economy,
                &settlement_authority.category,
                settlement,
                location_id,
            ) {
                return Err("NPC is not local".into());
            }
            let resident_character_id =
                subject_id.parse::<u64>().map_err(|_| "NPC is not local")?;
            let npc = state
                .db
                .query_one::<LocalNpcRow>(&format!(
                    "SELECT * FROM backend_settlement_residents WHERE character_id = {resident_character_id}"
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("NPC is not local")?;
            let presence = state
                .db
                .query_one::<LocalNpcPresenceRow>(&format!(
                    "SELECT * FROM settlement_resident_presence WHERE character_id = {resident_character_id}"
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("NPC is not local")?;
            let minute = state
                .db
                .query_one::<CharacterTime>(&format!(
                    "SELECT * FROM backend_character_times WHERE character_id = {}",
                    actor.id
                ))
                .await
                .map_err(|error| error.to_string())?
                .map_or(720, |time| time.minutes);
            if !npc_authority_matches(settlement, &npc, &presence, location_id, minute) {
                return Err("NPC is not local".into());
            }
            ConversationSelector::Npc(subject_id.to_string())
        }
        "player" => {
            if !location_id.is_empty() {
                return Err("Player conversations do not accept an NPC location".into());
            }
            let id: u64 = subject_id.parse().map_err(|_| "Invalid player")?;
            let subject = super::data::character_as_observed(state, id, actor.id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("Player is not available at your personal date")?;
            if !super::data::characters_share_frontier(state, actor.id, subject.id)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err("Player is not at this location".into());
            }
            let actor_site = character_case_site_id(state, actor.id).await?;
            let subject_site = character_case_site_id(state, subject.id).await?;
            if actor.current_settlement_id != subject.current_settlement_id
                || actor_site != subject_site
                || (actor.current_settlement_id.is_none() && actor_site.is_none())
            {
                return Err("Player is not at this location".into());
            }
            let other = subject.party_id.as_deref().ok_or("Player has no party")?;
            ConversationSelector::PlayerParty(other.to_string())
        }
        _ => return Err("Unknown Local subject".into()),
    };
    Ok((actor, selector))
}

async fn messages(
    State(state): State<AppState>,
    Path((kind, subject_id)): Path<(String, String)>,
    Query(query): Query<LocationQuery>,
    session: Session,
) -> Result<Json<LocalChatResponse>, (StatusCode, String)> {
    let actor_id = session
        .character_id_u64()
        .ok_or((StatusCode::UNAUTHORIZED, "Choose a character".into()))?;
    let (_, selector) =
        actor_and_selector(&state, actor_id, &kind, &subject_id, &query.location_id)
            .await
            .map_err(|e| (StatusCode::FORBIDDEN, e))?;
    let selector_filter = match &selector {
        ConversationSelector::Npc(resident_character_id) => format!(
            "conversation_kind = 'npc' AND subject_resident_character_id = {}",
            sql_string_literal(resident_character_id)
        ),
        ConversationSelector::PlayerParty(party_id) => format!(
            "conversation_kind = 'player' AND subject_party_id = {}",
            sql_string_literal(party_id)
        ),
    };
    let mut messages = state
        .db
        .query::<BackendLocalChatMessage>(&format!(
            "SELECT * FROM backend_local_chat_messages WHERE owner_character_id = {actor_id} AND {selector_filter}"
        ))
        .await
        .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    // Close the gap between the authority read and returning private message bodies.
    actor_and_selector(&state, actor_id, &kind, &subject_id, &query.location_id)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;
    messages.sort_by_key(|message| (message.created_micros, message.id));
    if messages.len() > MAX_CHAT_HISTORY {
        messages.drain(..messages.len() - MAX_CHAT_HISTORY);
    }
    Ok(Json(LocalChatResponse {
        messages: messages.into_iter().map(Into::into).collect(),
    }))
}

async fn send_message(
    State(state): State<AppState>,
    Path((kind, subject_id)): Path<(String, String)>,
    Query(query): Query<LocationQuery>,
    session: Session,
    Form(form): Form<MessageForm>,
) -> StatusCode {
    let Some(actor_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED;
    };
    if query.location_id != form.location_id {
        return StatusCode::BAD_REQUEST;
    }
    match state
        .db
        .call(
            "send_local_chat_message",
            &[
                json!(actor_id),
                json!(kind),
                json!(subject_id),
                json!(form.location_id),
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
        .query::<Character>(&format!(
            "SELECT * FROM backend_characters WHERE id = {actor_id}"
        ))
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
    let all_messages = state
        .db
        .query::<BackendLocalChatMessage>(&format!(
            "SELECT * FROM backend_local_chat_messages WHERE owner_character_id = {actor_id} AND conversation_kind = 'player'"
        ))
        .await
        .unwrap_or_default();
    let mut ids = std::collections::BTreeSet::new();
    for message in &all_messages {
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
    let mut visible = Vec::new();
    for id in ids.into_iter().take(MAX_INCOMING_PLAYERS) {
        let synchronized = super::data::characters_share_frontier(&state, actor.id, id)
            .await
            .unwrap_or(false);
        let candidate = if synchronized {
            super::data::character_as_observed(&state, id, actor.id)
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        if let Some(candidate) = candidate
            && candidate.current_settlement_id == actor.current_settlement_id
            && candidate_sites.get(&candidate.id) == Some(&actor_site)
        {
            visible.push(IncomingPlayer {
                id: candidate.id.to_string(),
                name: candidate.name,
            });
        }
    }
    Json(visible)
}

#[cfg(test)]
mod tests {
    use super::{
        LocalNpcPresenceRow, LocalNpcRow, npc_authority_matches, npc_history_location_is_navigable,
    };

    use crate::spacetimedb::SettlementCategory;

    #[test]
    fn player_chat_co_location_requires_equal_personal_frontiers() {
        let source = include_str!("local_chat.rs");
        let selector = source
            .split("async fn actor_and_selector")
            .nth(1)
            .unwrap()
            .split("async fn messages")
            .next()
            .unwrap();
        assert!(selector.contains("character_as_observed(state, id, actor.id)"));
        assert!(selector.contains("characters_share_frontier(state, actor.id, subject.id)"));

        let incoming = source
            .split("async fn incoming")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(incoming.contains("characters_share_frontier(&state, actor.id, id)"));
        assert!(!incoming.contains(".query::<Character>(\"SELECT * FROM backend_characters\")"));
    }

    #[test]
    fn hidden_npc_locations_cannot_authorize_chat_history() {
        let mut profile = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        assert!(npc_history_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "inn"
        ));
        assert!(npc_history_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "residences"
        ));
        assert!(!npc_history_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "church"
        ));
        assert!(!npc_history_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "armoury"
        ));
        assert!(!npc_history_location_is_navigable(
            &profile,
            &SettlementCategory::Hamlet,
            "fixture-no-orgs",
            "keep"
        ));
        profile
            .services
            .push(adventuresim_world_schema::SettlementService::Temple);
        assert!(npc_history_location_is_navigable(
            &profile,
            &SettlementCategory::Town,
            "fixture-no-orgs",
            "church"
        ));
        assert!(npc_history_location_is_navigable(
            &profile,
            &SettlementCategory::Town,
            "fixture-no-orgs",
            "keep"
        ));
    }

    #[test]
    fn riverdale_inn_npc_chain_uses_authority_not_encoded_id_shape() {
        let npc = LocalNpcRow {
            character_id: 41,
            home_settlement_id: "riverdale".into(),
        };
        let mut presence = LocalNpcPresenceRow {
            character_id: npc.character_id,
            settlement_id: "riverdale".into(),
            location_id: "inn".into(),
            start_minute: 0,
            end_minute: 1_440,
        };
        assert!(npc_authority_matches(
            "riverdale",
            &npc,
            &presence,
            "inn",
            720
        ));

        presence.settlement_id = "ironforge".into();
        assert!(!npc_authority_matches(
            "riverdale",
            &npc,
            &presence,
            "inn",
            720
        ));
        presence.settlement_id = "riverdale".into();
        presence.end_minute = 600;
        assert!(!npc_authority_matches(
            "riverdale",
            &npc,
            &presence,
            "inn",
            720
        ));
        presence.end_minute = 1_440;
        assert!(!npc_authority_matches(
            "riverdale",
            &npc,
            &presence,
            "market",
            720
        ));
    }

    #[test]
    fn npc_selection_waits_for_a_subject_and_preserves_reducer_diagnostics() {
        let javascript = include_str!("../../static/local-chat.js");
        assert!(javascript.contains("if (!kind || !subject) return null"));
        assert!(javascript.matches("if (!endpoint) return;").count() >= 2);
        assert!(!javascript.contains("/api/local-chat/${encodeURIComponent(node.dataset"));

        let local_route = include_str!("local_chat.rs")
            .split("async fn actor_and_selector")
            .nth(1)
            .and_then(|tail| tail.split("async fn messages").next())
            .expect("local chat authority handler");
        assert!(local_route.contains(
            "SELECT * FROM backend_settlement_residents WHERE character_id = {resident_character_id}"
        ));
        assert!(local_route.contains(
            "SELECT * FROM settlement_resident_presence WHERE character_id = {resident_character_id}"
        ));
        assert!(
            include_str!("local_chat.rs").contains("presence.location_id == requested_location_id")
        );
        assert!(!local_route.contains("subject_id.starts_with"));

        let dialogue_route = include_str!("dialogue.rs");
        assert!(dialogue_route.contains("start_dialogue reducer rejected an NPC encounter"));
        assert!(dialogue_route.contains("StatusCode::CONFLICT"));
    }
}
