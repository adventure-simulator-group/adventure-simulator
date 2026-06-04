//! Settlement route handlers

use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    routing::{get, post},
    Router,
};

use super::AppState;
use crate::models::{Party, Quest, Settlement};
use crate::services;
use crate::session::Session;
use crate::templates::settlement::{
    inn_page, merchants_page, noticeboard_page, settlement_detail_page, settlements_list_page,
    smith_page, tavern_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settlements", get(list_settlements))
        .route("/settlements/{id}", get(show_settlement))
        .route("/settlements/{id}/noticeboard", get(noticeboard))
        .route("/settlements/{id}/tavern", get(tavern))
        .route("/settlements/{id}/merchants", get(merchants))
        .route("/settlements/{id}/smith", get(smith))
        .route("/settlements/{id}/inn", get(inn))
        .route("/settlements/{id}/travel", post(travel))
}

async fn list_settlements(State(state): State<AppState>, session: Session) -> Html<String> {
    let settlements: Vec<Settlement> = services::list_settlements(&state.db)
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        settlements_list_page(&settlements, logged_in_as.as_deref(), session.theme()).into_string(),
    )
}

async fn show_settlement(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement = match services::get_settlement(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let quests: Vec<Quest> = services::quests_at_settlement(&state.db, &id)
        .await
        .unwrap_or_default();
    let available_quests: Vec<Quest> = quests
        .into_iter()
        .filter(|q| is_available_status(&q.status))
        .collect();

    // Get parties at this settlement
    let parties: Vec<Party> = services::parties_at_settlement(&state.db, &id)
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        settlement_detail_page(
            &settlement,
            &available_quests,
            &parties,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn noticeboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement = match services::get_settlement(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let quests: Vec<Quest> = services::quests_at_settlement(&state.db, &id)
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        noticeboard_page(
            &settlement,
            &quests,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn tavern(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement = match services::get_settlement(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let parties: Vec<Party> = services::parties_at_settlement(&state.db, &id)
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        tavern_page(
            &settlement,
            &parties,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn merchants(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement = match services::get_settlement(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(merchants_page(&settlement, logged_in_as.as_deref(), session.theme()).into_string())
}

async fn smith(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement = match services::get_settlement(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(smith_page(&settlement, logged_in_as.as_deref(), session.theme()).into_string())
}

async fn inn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement = match services::get_settlement(&state.db, &id)
        .await
        .unwrap_or_default()
    {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(inn_page(&settlement, logged_in_as.as_deref(), session.theme()).into_string())
}

async fn travel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok())
    else {
        return Redirect::to("/characters");
    };

    let _ = services::travel_to_settlement(&state.db, character_id, id.clone()).await;

    Redirect::to(&format!("/settlements/{}", id))
}

/// Helper to get character name for session display
async fn get_character_name(state: &AppState, character_id: Option<&str>) -> Option<String> {
    services::get_character_name(&state.db, character_id).await
}

fn is_available_status(status: &str) -> bool {
    status.to_ascii_lowercase().contains("available")
}
