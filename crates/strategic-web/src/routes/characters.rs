//! Character route handlers

use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::{clear_character_cookie, set_character_cookie, Session};
use crate::spacetimedb::{Character, InventoryItem};
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
    let characters: Vec<Character> = state
        .db
        .query("SELECT * FROM character")
        .await
        .unwrap_or_default();

    Html(
        characters_list_page(&characters, session.character_id_u64(), session.theme())
            .into_string(),
    )
}

async fn new_character_form(State(state): State<AppState>, session: Session) -> Html<String> {
    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(character_new_page(logged_in_as.as_deref(), session.theme()).into_string())
}

async fn create_character(
    State(state): State<AppState>,
    Form(form): Form<CreateCharacterForm>,
) -> Response {
    let id = chrono_id();

    if let Err(error) = state
        .db
        .call(
            "create_named_character_with_id",
            &[json!(id), json!(form.name)],
        )
        .await
    {
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
    let characters: Vec<Character> = state
        .db
        .query(&format!("SELECT * FROM character WHERE id = {}", id))
        .await
        .unwrap_or_default();

    let character = match characters.first() {
        Some(c) => c,
        None => return Html("<h1>Character not found</h1>".to_string()),
    };

    let inventory: Vec<InventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {}",
            id
        ))
        .await
        .unwrap_or_default();

    let is_current = session.character_id_u64() == id.parse::<u64>().ok();
    let logged_in_as = if is_current {
        Some(character.name.clone())
    } else {
        get_character_name(&state, session.character_id()).await
    };
    Html(
        character_detail_page(
            character,
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
    let _ = state
        .db
        .call(
            "update_character",
            &[
                json!(id.parse::<u64>().unwrap_or_default()),
                json!(form.name),
            ],
        )
        .await;

    Redirect::to(&format!("/characters/{}", id))
}

async fn show_inventory(State(state): State<AppState>, Path(id): Path<String>) -> Html<String> {
    let inventory: Vec<InventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {}",
            id
        ))
        .await
        .unwrap_or_default();

    // Return a simple inventory fragment
    let html = maud::html! {
        div # "inventory" {
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

/// Generate a simple timestamp-based ID
fn chrono_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Helper to get character name for session display
async fn get_character_name(state: &AppState, character_id: Option<&str>) -> Option<String> {
    let Some(id) = character_id else {
        return None;
    };
    let characters: Vec<Character> = state
        .db
        .query(&format!("SELECT * FROM character WHERE id = {}", id))
        .await
        .unwrap_or_default();
    characters.first().map(|c| c.name.clone())
}
