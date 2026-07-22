use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AppState;
use crate::session::Session;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/dialogue/catalog", get(catalog_view))
        .route("/api/dialogue/start", post(start))
        .route("/api/dialogue/topic", post(topic))
        .route("/api/dialogue/answer", post(answer))
}

#[derive(Serialize)]
struct CatalogView {
    revision: &'static str,
    conversations: &'static [adventuresim_dialogue::CatalogDocument],
    known_topics: Vec<String>,
}

#[derive(Deserialize)]
struct TopicKnowledgeRow {
    topic_id: String,
}

async fn catalog_view(State(state): State<AppState>, session: Session) -> Json<CatalogView> {
    let known_topics = if let Some(character_id) = session.character_id_u64() {
        state
            .db
            .query::<TopicKnowledgeRow>(&format!(
                "SELECT topic_id FROM character_topic_knowledge WHERE character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.topic_id)
            .collect()
    } else {
        Vec::new()
    };
    Json(CatalogView {
        revision: adventuresim_dialogue::CATALOG_DIGEST,
        conversations: adventuresim_dialogue::catalog(),
        known_topics,
    })
}

#[derive(Deserialize)]
struct StartRequest {
    conversation_id: String,
    npc_actor_id: String,
    revision: String,
}
async fn start(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<StartRequest>,
) -> Result<Json<StartResponse>, StatusCode> {
    let Some(character_id) = session.character_id_u64() else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_micros());
    let session_id = format!("dialogue:{character_id}:{nonce}");
    state
        .db
        .call(
            "start_dialogue",
            &[
                json!(character_id),
                json!(&session_id),
                json!(request.conversation_id),
                json!(request.npc_actor_id),
                json!(request.revision),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(StartResponse {
        session_id,
        revision: adventuresim_dialogue::CATALOG_DIGEST,
    }))
}

#[derive(Serialize)]
struct StartResponse {
    session_id: String,
    revision: &'static str,
}

#[derive(Deserialize)]
struct TopicRequest {
    session_id: String,
    conversation_id: String,
    topic_id: String,
    revision: String,
}
async fn topic(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<TopicRequest>,
) -> Result<Json<adventuresim_dialogue::Response>, StatusCode> {
    let Some(character_id) = session.character_id_u64() else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    state
        .db
        .call(
            "choose_dialogue_topic",
            &[
                json!(character_id),
                json!(request.session_id),
                json!(request.topic_id),
                json!(request.revision),
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?;
    let response = adventuresim_dialogue::find_conversation(&request.conversation_id)
        .and_then(|conversation| {
            conversation
                .topics
                .iter()
                .find(|topic| topic.id == request.topic_id)
        })
        .and_then(|topic| {
            adventuresim_dialogue::select_response(
                topic,
                &adventuresim_dialogue::FactContext::default(),
            )
            .ok()
        })
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(response))
}

#[derive(Deserialize)]
struct AnswerRequest {
    prompt_row_id: String,
    choice_ids: Vec<String>,
    revision: String,
}
async fn answer(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<AnswerRequest>,
) -> StatusCode {
    let Some(character_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED;
    };
    reducer_status(
        state
            .db
            .call(
                "answer_dialogue_prompt",
                &[
                    json!(character_id),
                    json!(request.prompt_row_id),
                    json!(serde_json::to_string(&request.choice_ids).unwrap()),
                    json!(request.revision),
                ],
            )
            .await,
    )
}

fn reducer_status(result: crate::spacetimedb::Result<()>) -> StatusCode {
    if result.is_ok() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CONFLICT
    }
}
