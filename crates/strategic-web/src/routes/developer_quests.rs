//! Developer-mode quest authoring HTTP adapter.
//!
//! Developer mode is browser-local UI hiding, not authorization. These routes
//! require an active character but intentionally add no developer credential.

use super::{AppState, BackendSettlementNpcRow as NpcRow};
use crate::{
    session::Session,
    spacetimedb::{BackendChallenge, Character, CharacterTime, Settlement, sql_string_literal},
};
use adventuresim_core::{
    developer_quest::{self as dq, DeveloperGenerationContext, DeveloperQuestDefinition},
    quest_generation::{
        GenerationContext, TemplateFamily, VisibleWitnessCandidateInput,
        retain_navigable_witnesses, visible_witness_candidate,
    },
    settlement_economy::player_visible_npc_tabs,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Deserialize)]
struct PresenceRow {
    npc_id: String,
    settlement_id: String,
    location_id: String,
    start_minute: u16,
    end_minute: u16,
    is_default: bool,
}

#[derive(Deserialize)]
struct SpawnRequest {
    definition: Value,
    #[serde(default)]
    allow_implausible: bool,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/developer/quests/schema", get(schema))
        .route("/api/developer/quests", post(spawn))
        .route("/api/developer/autopsy-demo", post(load_autopsy_demo))
        .route("/api/developer/outbreak-demo", post(load_outbreak_demo))
        .route("/api/developer/puzzle-demo", post(load_puzzle_demo))
}

async fn active_context(
    state: &AppState,
    session: &Session,
    seed: u64,
) -> Result<(u64, Settlement, GenerationContext), StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    let character = state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let settlement_id = character
        .current_settlement_id
        .ok_or(StatusCode::CONFLICT)?;
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&settlement_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let now_minute = state
        .db
        .query_one::<CharacterTime>(&format!(
            "SELECT * FROM character_time WHERE character_id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .map_or(0, |time| time.minutes);
    let literal = sql_string_literal(&settlement_id);
    let npc_sql =
        format!("SELECT * FROM backend_settlement_npcs WHERE home_settlement_id = {literal}");
    let presence_sql =
        format!("SELECT * FROM settlement_npc_presence WHERE settlement_id = {literal}");
    let (npcs, presences) = tokio::join!(
        state.db.query::<NpcRow>(&npc_sql),
        state.db.query::<PresenceRow>(&presence_sql)
    );
    let visible_tabs = player_visible_npc_tabs(
        &settlement.economy,
        matches!(
            settlement.category,
            crate::spacetimedb::SettlementCategory::Town
                | crate::spacetimedb::SettlementCategory::City
                | crate::spacetimedb::SettlementCategory::Capital
        ),
        &settlement_id,
    );
    let presences = presences.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut candidates = npcs
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .into_iter()
        .filter_map(|npc| {
            let presence = presences.iter().find(|row| row.npc_id == npc.id)?;
            visible_witness_candidate(VisibleWitnessCandidateInput {
                npc_id: &npc.id,
                display_name: &npc.name,
                age_band: &npc.age_band,
                presentation: &npc.presentation,
                height: &npc.height,
                build: &npc.build,
                hair: &npc.hair,
                clothing: &npc.clothing,
                profession: &npc.profession,
                local_role: &npc.local_role,
                settlement_id: &presence.settlement_id,
                location_id: &presence.location_id,
                start_minute: presence.start_minute,
                end_minute: presence.end_minute,
                is_default: presence.is_default,
            })
        })
        .collect::<Vec<_>>();
    candidates = retain_navigable_witnesses(candidates, &visible_tabs);
    candidates.sort_by(|left, right| left.npc_id.cmp(&right.npc_id));
    let context = GenerationContext {
        seed,
        observer_entropy_hi: seed.rotate_left(17),
        observer_entropy_lo: seed.rotate_right(13),
        settlement_id: settlement_id.clone(),
        settlement_name: settlement.name.clone(),
        scope: adventuresim_core::local_problem::Scope::Settlement { settlement_id },
        ordinal: 0,
        now_minute,
        incident_weather: adventuresim_core::weather::Precipitation::Clear,
        requested_family: Some(TemplateFamily::RecurringDepredation),
        witness_candidates: candidates,
    };
    Ok((character_id, settlement, context))
}

async fn schema(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<Value>, StatusCode> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_nanos() as u64;
    let (_, settlement, context) = active_context(&state, &session, seed).await?;
    let generated = adventuresim_core::quest_generation::generate(&context)
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let definition = DeveloperQuestDefinition::from_generated(generated);
    let mut schema = dq::schema_json(&context.witness_candidates);
    schema["settlement"] = json!({"id": settlement.id, "name": settlement.name});
    schema["definition"] =
        serde_json::to_value(definition).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(schema))
}

async fn spawn(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<SpawnRequest>,
) -> Response {
    let (character_id, _, context) = match active_context(&state, &session, 0x0ddc_0ffe).await {
        Ok(context) => context,
        Err(status) => {
            let (code, message) = match status {
                StatusCode::UNAUTHORIZED => (
                    "character_not_selected",
                    "Select a character before spawning a developer quest",
                ),
                StatusCode::SERVICE_UNAVAILABLE => (
                    "strategic_data_unavailable",
                    "Strategic data is unavailable",
                ),
                StatusCode::NOT_FOUND => (
                    "current_settlement_not_found",
                    "The character's current settlement no longer exists",
                ),
                StatusCode::CONFLICT => (
                    "not_in_settlement",
                    "Developer quests can only be spawned in a settlement",
                ),
                _ => (
                    "context_unavailable",
                    "Developer quest context is unavailable",
                ),
            };
            return (
                status,
                Json(json!({"diagnostics":[{
                    "path":"$","code":code,
                    "message":message,
                    "tier":"structural"
                }]})),
            )
                .into_response();
        }
    };
    let definition: DeveloperQuestDefinition = match serde_json::from_value(request.definition) {
        Ok(definition) => definition,
        Err(error) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"diagnostics":[{
                    "path":"$","code":"invalid_definition",
                    "message":error.to_string(),"tier":"structural"
                }]})),
            )
                .into_response();
        }
    };
    let preview = DeveloperGenerationContext {
        base: context,
        definition: definition.clone(),
        allow_implausible: request.allow_implausible,
    };
    if let Err(diagnostics) = dq::compile(&preview) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"diagnostics": diagnostics})),
        )
            .into_response();
    }
    let definition_json = match serde_json::to_string(&definition) {
        Ok(value) if value.len() <= dq::MAX_DEVELOPER_QUEST_JSON_BYTES => value,
        _ => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"diagnostics":[{
                    "path":"$","code":"payload_too_large",
                    "message":"Developer quest definition exceeds the server limit",
                    "tier":"structural"
                }]})),
            )
                .into_response();
        }
    };
    match state
        .db
        .call(
            "spawn_developer_quest",
            &[
                json!(character_id),
                json!(definition_json),
                json!(request.allow_implausible),
            ],
        )
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(json!({"status":"created","discovery":"normal_rumor"})),
        )
            .into_response(),
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"diagnostics":[{
                "path":"$","code":"authority_rejected",
                "message":error.to_string(),"tier":"structural"
            }]})),
        )
            .into_response(),
    }
}

async fn load_autopsy_demo(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message":"Select a character before loading the autopsy demo"})),
        )
            .into_response();
    };
    let character = match state
        .db
        .query_one::<crate::spacetimedb::Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
    {
        Ok(Some(character)) => character,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"message":"Selected character was not found"})),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, character_id, "failed to load autopsy demo character");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"message":"Strategic data is unavailable"})),
            )
                .into_response();
        }
    };
    let Some(_settlement_id) = character.current_settlement_id else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"message":"Load the autopsy demo while in a settlement"})),
        )
            .into_response();
    };
    match state
        .db
        .call("load_autopsy_demo", &[json!(character_id)])
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(json!({
                "status":"loaded",
                "redirect_to":format!("/locations/settlement/{settlement_id}")
            })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"message":error.to_string()})),
        )
            .into_response(),
    }
}

async fn load_outbreak_demo(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message":"Select a character before loading the outbreak demo"})),
        )
            .into_response();
    };
    let character = match state
        .db
        .query_one::<crate::spacetimedb::Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
    {
        Ok(Some(character)) => character,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"message":"Selected character was not found"})),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, character_id, "failed to load outbreak demo character");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"message":"Strategic data is unavailable"})),
            )
                .into_response();
        }
    };
    let Some(settlement_id) = character.current_settlement_id else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"message":"Load the outbreak demo while in a settlement"})),
        )
            .into_response();
    };
    match state
        .db
        .call("load_outbreak_demo", &[json!(character_id)])
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(json!({
                "status":"loaded",
                "discovery":"normal_rumor",
                "redirect_to":format!("/locations/settlement/{settlement_id}")
            })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"message":error.to_string()})),
        )
            .into_response(),
    }
}

async fn load_puzzle_demo(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"message":"Select a character before loading the puzzle demo"})),
        )
            .into_response();
    };
    let character = match state
        .db
        .query_one::<crate::spacetimedb::Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
    {
        Ok(Some(character)) => character,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let Some(settlement_id) = character.current_settlement_id else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"message":"Load the puzzle demo while in a settlement"})),
        )
            .into_response();
    };
    match state
        .db
        .call("load_puzzle_demo", &[json!(character_id)])
        .await
    {
        Ok(()) => {
            let demo_prefix = format!("challenge:ordered-sigils:demo:{character_id}:");
            let sql = format!(
                "SELECT * FROM backend_challenges WHERE owner_character_id = {character_id}"
            );
            let mut playable = match state.db.query::<BackendChallenge>(&sql).await {
                Ok(rows) => rows
                    .into_iter()
                    .filter(|row| {
                        row.active && row.open && !row.solved && row.id.starts_with(&demo_prefix)
                    })
                    .collect::<Vec<_>>(),
                Err(error) => {
                    tracing::error!(%error, character_id, "failed to project loaded puzzle demo");
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
            };
            if playable.len() != 1 {
                tracing::error!(
                    character_id,
                    count = playable.len(),
                    "puzzle demo did not expose exactly one playable challenge"
                );
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            let challenge = playable.pop().expect("length checked");
            (
                StatusCode::CREATED,
                Json(json!({
                    "status":"loaded",
                    "redirect_to":format!(
                        "/quests/{}/challenges/{}",
                        challenge.case_id, challenge.id
                    )
                })),
            )
                .into_response()
        }
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"message":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn endpoint_contract_does_not_accept_a_settlement_id() {
        let source = include_str!("developer_quests.rs");
        assert!(source.contains("current_settlement_id"));
        assert!(!source.contains("struct SpawnRequest {\n    settlement"));
        assert!(source.contains("StatusCode::UNPROCESSABLE_ENTITY"));
        assert!(source.contains("allow_implausible"));
    }

    #[test]
    fn autopsy_demo_loader_uses_selected_character_and_server_derived_settlement() {
        let source = include_str!("developer_quests.rs");
        assert!(source.contains("/api/developer/autopsy-demo"));
        assert!(source.contains("\"load_autopsy_demo\", &[json!(character_id)]"));
        assert!(source.contains("character.current_settlement_id"));
    }

    #[test]
    fn quest_preview_uses_only_the_gateway_npc_projection() {
        let source = include_str!("developer_quests.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("developer quest production source");
        let transport = include_str!("mod.rs");
        let row = transport
            .split("pub(crate) struct BackendSettlementNpcRow {")
            .nth(1)
            .and_then(|tail| tail.split_once('}').map(|(body, _)| body))
            .expect("NPC transport row");
        assert!(source.contains("BackendSettlementNpcRow as NpcRow"));
        assert!(row.contains("presentation: String"));
        assert!(!row.contains("sex: String"));
        assert!(!row.contains("projection_id:"));
        assert!(!production.contains("npc.sex"));
        assert!(production.contains("visible_witness_candidate"));
        assert!(production.contains("presentation: &npc.presentation"));
    }

    #[test]
    fn outbreak_demo_loader_preserves_normal_discovery() {
        let source = include_str!("developer_quests.rs");
        assert!(source.contains("/api/developer/outbreak-demo"));
        assert!(source.contains("\"load_outbreak_demo\", &[json!(character_id)]"));
        let loader = source.split("async fn load_outbreak_demo").nth(1).unwrap();
        assert!(loader.contains("\"discovery\":\"normal_rumor\""));
        assert!(!loader.contains("rumor_receipt"));
    }

    #[test]
    fn puzzle_demo_redirects_directly_to_the_playable_challenge() {
        let source = include_str!("developer_quests.rs");
        assert!(source.contains("/api/developer/puzzle-demo"));
        assert!(source.contains("\"load_puzzle_demo\", &[json!(character_id)]"));
        assert!(source.contains("query::<BackendChallenge>"));
        assert!(source.contains("row.id.starts_with(&demo_prefix)"));
        assert!(source.contains("challenge.case_id, challenge.id"));
        let loader = source
            .split("async fn load_puzzle_demo")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!loader.contains("rumor"));
    }
}
