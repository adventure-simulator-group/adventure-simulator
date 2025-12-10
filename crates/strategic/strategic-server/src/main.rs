use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Router,
};
use futures::Stream;
use async_stream::stream;
use serde::Serialize;
use strategic_db::StrategicDb;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/strategic".to_string());

    let db = StrategicDb::connect(&database_url, 5).await?;
    db.ensure_schema().await?;

    let (tx, _rx) = broadcast::channel::<QuestEvent>(32);

    let state = AppState {
        db: Arc::new(db),
        tx,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/quests/:id", get(get_quest))
        .route("/quests/:id/state", get(quest_state))
        .route("/quests/:id/events", get(quest_events))
        .route("/quests/:id/start", post(start_quest))
        .route("/quests/:id/complete", post(complete_quest))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_methods(Any)
                .allow_origin(Any)
                .allow_headers(Any),
        );

    let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    println!("strategic server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Clone)]
struct AppState {
    db: Arc<StrategicDb>,
    tx: broadcast::Sender<QuestEvent>,
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn get_quest(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    match state.db.get_quest(&id).await {
        Ok(Some(quest)) => Json(quest).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            eprintln!("failed to fetch quest: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn quest_state(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    match state.db.get_quest(&id).await {
        Ok(Some(quest)) => {
            let html = render_overlay_html(&quest.id, &format!("{}", quest.status_string()), "Strategic overlay");
            (
                [(axum::http::header::CONTENT_TYPE, "text/html")],
                html,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            eprintln!("failed to fetch quest: {err:?}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn quest_events(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    let mut rx = state.tx.subscribe();
    let stream = stream! {
        while let Ok(evt) = rx.recv().await {
            if evt.quest_id != id {
                continue;
            }
            let data = render_overlay_html(&evt.quest_id, &evt.status, &evt.message);
            yield Ok::<Event, std::convert::Infallible>(Event::default().data(data));
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn complete_quest(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    if let Err(err) = state.db.complete_quest(&id).await {
        eprintln!("failed to complete quest: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let _ = state.tx.send(QuestEvent {
        quest_id: id.clone(),
        status: "completed".to_string(),
        message: "Quest marked complete.".to_string(),
    });

    #[derive(Serialize)]
    struct Payload {
        status: &'static str,
        message: &'static str,
    }

    Json(Payload {
        status: "completed",
        message: "Quest updated",
    })
    .into_response()
}

async fn start_quest(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<String>) -> impl IntoResponse {
    if let Err(err) = state.db.start_quest(&id).await {
        eprintln!("failed to start quest: {err:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let _ = state.tx.send(QuestEvent {
        quest_id: id.clone(),
        status: "active".to_string(),
        message: "Quest started.",
    });

    #[derive(Serialize)]
    struct Payload {
        status: &'static str,
        message: &'static str,
    }

    Json(Payload {
        status: "active",
        message: "Quest started",
    })
    .into_response()
}

#[derive(Clone, Debug)]
struct QuestEvent {
    quest_id: String,
    status: String,
    message: String,
}

fn render_overlay_html(quest_id: &str, status: &str, message: &str) -> String {
    format!(
        r#"<div id="quest-overlay-inner" data-star-swap-oob="true">
  <div class="overlay-title">Quest: {}</div>
  <div class="overlay-status">Status: {}</div>
  <div class="overlay-message">{}</div>
</div>"#,
        quest_id, status, message
    )
}

trait QuestStatusString {
    fn status_string(&self) -> &'static str;
}

impl QuestStatusString for strategic_core::QuestStatus {
    fn status_string(&self) -> &'static str {
        match self {
            strategic_core::QuestStatus::Available => "available",
            strategic_core::QuestStatus::Active => "active",
            strategic_core::QuestStatus::Completed => "completed",
        }
    }
}
