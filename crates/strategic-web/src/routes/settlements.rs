//! Settlement route handlers

use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    routing::{get, post},
    Router,
};
use serde_json::json;

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{Character, InventoryItem, Party, PartyMember, Quest, Settlement};
use crate::templates::settlement::{
    armor_page, clothing_page, consumables_page, inn_page, merchants_page, noticeboard_page,
    religion_page, settlements_list_page, smith_page, tavern_page, weapons_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settlements", get(list_settlements))
        .route("/settlements/{id}", get(show_settlement))
        .route("/settlements/{id}/noticeboard", get(noticeboard))
        .route("/settlements/{id}/tavern", get(tavern))
        .route("/settlements/{id}/merchants", get(merchants))
        .route("/settlements/{id}/weapons", get(weapons))
        .route("/settlements/{id}/armor", get(armor))
        .route("/settlements/{id}/clothing", get(clothing))
        .route("/settlements/{id}/consumables", get(consumables))
        .route("/settlements/{id}/smith", get(smith))
        .route("/settlements/{id}/inn", get(inn))
        .route("/settlements/{id}/religion", get(religion))
        .route("/settlements/{id}/travel", post(travel))
}

async fn list_settlements(State(state): State<AppState>, session: Session) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        settlements_list_page(&settlements, logged_in_as.as_deref(), session.theme()).into_string(),
    )
}

async fn show_settlement(
    Path(id): Path<String>,
 ) -> Redirect {
    Redirect::to(&format!("/settlements/{id}/noticeboard"))
}

async fn noticeboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let quests: Vec<Quest> = state
        .db
        .query(&format!(
            "SELECT * FROM quest WHERE settlement_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        noticeboard_page(
            settlement,
            &quests,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
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
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let parties: Vec<Party> = state
        .db
        .query(&format!(
            "SELECT * FROM party WHERE current_settlement_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        tavern_page(
            settlement,
            &parties,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
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
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        merchants_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn smith(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        smith_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn inn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let settlement = match settlements.first() {
        Some(s) => s,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    Html(
        inn_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn travel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let _ = state
        .db
        .call(
            "travel_to_settlement",
            &[json!(character_id), json!(id.clone())],
        )
        .await;

    Redirect::to(&format!("/settlements/{}", id))
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

async fn weapons(State(state): State<AppState>, Path(id): Path<String>, session: Session) -> Html<String> {
    render_service_page(state, id, session, weapons_page).await
}

async fn armor(State(state): State<AppState>, Path(id): Path<String>, session: Session) -> Html<String> {
    render_service_page(state, id, session, armor_page).await
}

async fn clothing(State(state): State<AppState>, Path(id): Path<String>, session: Session) -> Html<String> {
    render_service_page(state, id, session, clothing_page).await
}

async fn consumables(State(state): State<AppState>, Path(id): Path<String>, session: Session) -> Html<String> {
    render_service_page(state, id, session, consumables_page).await
}

async fn religion(State(state): State<AppState>, Path(id): Path<String>, session: Session) -> Html<String> {
    render_service_page(state, id, session, religion_page).await
}

type ServiceRenderer = fn(
    &Settlement,
    Option<&Character>,
    &[InventoryItem],
    &[Character],
    Option<&str>,
    &str,
) -> maud::Markup;

async fn render_service_page(
    state: AppState,
    id: String,
    session: Session,
    render: ServiceRenderer,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let settlement = match settlements.first() {
        Some(settlement) => settlement,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());

    Html(
        render(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn get_active_character(
    state: &AppState,
    character_id: Option<u64>,
) -> Option<(Character, Vec<InventoryItem>)> {
    let character_id = character_id?;
    let characters: Vec<Character> = state
        .db
        .query(&format!("SELECT * FROM character WHERE id = {character_id}"))
        .await
        .unwrap_or_default();
    let character = characters.into_iter().next()?;
    let inventory: Vec<InventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    Some((character, inventory))
}

async fn get_active_party_members(state: &AppState, active_character: Option<&Character>) -> Vec<Character> {
    let Some(party_id) = active_character.and_then(|character| character.party_id.as_ref()) else {
        return Vec::new();
    };
    let memberships: Vec<PartyMember> = state
        .db
        .query(&format!("SELECT * FROM party_member WHERE party_id = '{}'", party_id))
        .await
        .unwrap_or_default();

    let mut members = Vec::new();
    for membership in memberships {
        let characters: Vec<Character> = state
            .db
            .query(&format!("SELECT * FROM character WHERE id = {}", membership.character_id))
            .await
            .unwrap_or_default();
        if let Some(character) = characters.into_iter().next() {
            members.push(character);
        }
    }
    members
}

