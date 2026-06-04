//! Character route handlers

use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::models::{Character, InventoryItem};
use crate::services;
use crate::session::{clear_character_cookie, set_character_cookie, Session};
use crate::templates::character::{
    character_detail_page, character_new_page, characters_list_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/characters", get(list_characters))
        .route("/characters/new", get(new_character_form))
        .route("/characters", post(create_character))
        .route("/characters/{id}", get(show_character))
        .route("/characters/{id}", post(update_character))
        .route("/characters/{id}/select", post(select_character))
        .route("/characters/{id}/inventory", get(show_inventory))
        .route("/characters/logout", post(logout))
}

#[derive(Deserialize)]
struct CreateCharacterForm {
    name: String,
}

#[derive(Deserialize)]
struct UpdateCharacterForm {
    name: String,
}

async fn list_characters(State(state): State<AppState>, session: Session) -> Html<String> {
    let characters: Vec<Character> = services::list_characters(&state.db)
        .await
        .unwrap_or_default();

    let current_character_id = session
        .character_id_u64()
        .and_then(|id| services::u64_to_i64(id).ok());
    Html(characters_list_page(&characters, current_character_id, session.theme()).into_string())
}

async fn new_character_form(State(state): State<AppState>, session: Session) -> Html<String> {
    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(character_new_page(logged_in_as.as_deref(), session.theme()).into_string())
}

async fn create_character(
    State(state): State<AppState>,
    Form(form): Form<CreateCharacterForm>,
) -> Response {
    let id = services::chrono_id();

    if let Err(error) = services::create_named_character_with_id(&state.db, id, form.name).await {
        tracing::error!("Failed to create character {id}: {error}");
        return Redirect::to("/characters/new").into_response();
    }

    // Auto-select the newly created character
    set_character_cookie(&id.to_string(), "/")
}

async fn select_character(Path(id): Path<String>) -> Response {
    set_character_cookie(&id, "/")
}

async fn logout() -> Response {
    clear_character_cookie("/characters")
}

async fn show_character(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let Ok(id) = id.parse::<i64>() else {
        return Html("<h1>Character not found</h1>".to_string());
    };

    let character = match services::get_character(&state.db, id)
        .await
        .unwrap_or_default()
    {
        Some(c) => c,
        None => return Html("<h1>Character not found</h1>".to_string()),
    };

    let inventory: Vec<InventoryItem> = services::inventory_for_character(&state.db, id)
        .await
        .unwrap_or_default();

    let is_current = session
        .character_id_u64()
        .and_then(|current| services::u64_to_i64(current).ok())
        == Some(id);
    let logged_in_as = if is_current {
        Some(character.name.clone())
    } else {
        get_character_name(&state, session.character_id()).await
    };
    Html(
        character_detail_page(
            &character,
            &inventory,
            is_current,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn update_character(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<UpdateCharacterForm>,
) -> Redirect {
    if let Ok(id) = id.parse::<i64>() {
        let _ = services::update_character(&state.db, id, form.name).await;
    }

    Redirect::to(&format!("/characters/{}", id))
}

async fn show_inventory(State(state): State<AppState>, Path(id): Path<String>) -> Html<String> {
    let inventory: Vec<InventoryItem> = match id.parse::<i64>() {
        Ok(id) => services::inventory_for_character(&state.db, id)
            .await
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    // Return a simple inventory fragment
    let html = maud::html! {
        div #"inventory" {
            h3 { "Inventory" }
            @if inventory.is_empty() {
                p { "No items" }
            } @else {
                ul {
                    @for item in &inventory {
                        li { (item.item_id) " x" (item.qty) }
                    }
                }
            }
        }
    };

    Html(html.into_string())
}

/// Helper to get character name for session display
async fn get_character_name(state: &AppState, character_id: Option<&str>) -> Option<String> {
    services::get_character_name(&state.db, character_id).await
}
