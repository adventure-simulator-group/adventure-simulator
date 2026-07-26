//! Developer-mode quest authoring HTTP adapter.
//!
//! Developer mode is browser-local UI hiding, not authorization. These routes
//! require an active character but intentionally add no developer credential.

use super::AppState;
use crate::{
    session::Session,
    spacetimedb::{Character, CharacterTime, Settlement, sql_string_literal},
};
use adventuresim_core::{
    developer_quest::{self as dq, DeveloperGenerationContext, DeveloperQuestDefinition},
    quest_generation::{
        Circumstance, GenerationContext, TemplateFamily, WitnessCandidate, WitnessDemographic,
        retain_navigable_witnesses,
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
use std::collections::BTreeSet;

#[derive(Clone, Deserialize)]
struct NpcRow {
    id: String,
    name: String,
    age_band: String,
    sex: String,
    height: String,
    build: String,
    hair: String,
    clothing: String,
    profession: String,
    local_role: String,
}

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
    );
    let presences = presences.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut candidates = npcs
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .into_iter()
        .filter_map(|npc| {
            let presence = presences.iter().find(|row| row.npc_id == npc.id)?;
            let authored = adventuresim_core::quest_catalog::catalog().witness_demographic_for(
                &npc.age_band.to_ascii_lowercase(),
                &npc.sex.to_ascii_lowercase(),
                &npc.profession,
                &npc.local_role,
            )?;
            let demographic = WitnessDemographic::try_new(&authored.id).ok()?;
            let mut allowed_circumstances = BTreeSet::from([
                Circumstance::NightWindow,
                Circumstance::RoadJourney,
                Circumstance::LivestockWatch,
            ]);
            if presence.location_id == "church" {
                allowed_circumstances.insert(Circumstance::GraveDuty);
            }
            if presence.location_id == "adult_venue" || demographic != WitnessDemographic::Child {
                allowed_circumstances.insert(Circumstance::AdultVenue);
            }
            if demographic != WitnessDemographic::Child {
                allowed_circumstances.insert(Circumstance::SecretRiversideMeeting);
            }
            Some(WitnessCandidate {
                npc_id: npc.id.clone(),
                display_name: npc.name,
                demographic,
                age_band: npc.age_band.to_ascii_lowercase(),
                sex: npc.sex.to_ascii_lowercase(),
                profession: npc.profession.clone(),
                visible_description: format!(
                    "{}, {}, with {}, wearing {}",
                    npc.height, npc.build, npc.hair, npc.clothing
                ),
                expected_location: presence.location_id.clone(),
                expected_location_label: String::new(),
                presence_version: adventuresim_core::settlement_population::stable_hash(&format!(
                    "victim-presence-v1:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    npc.id,
                    npc.age_band,
                    npc.sex,
                    npc.profession,
                    presence.settlement_id,
                    presence.location_id,
                    presence.start_minute,
                    presence.end_minute,
                    presence.is_default
                )),
                allowed_circumstances,
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
}
