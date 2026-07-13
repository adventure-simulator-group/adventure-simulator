//! Settlement route handlers

use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, Redirect},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterEquip, CharacterLimbs, CharacterSkills,
    CharacterTrainingSchedule, InventoryItem, ItemDefinition, Party, PartyMember, Quest, Settlement,
};
use crate::templates::settlement::{
    MerchantShop, inn_page, live_merchant_shop_page, merchants_page, noticeboard_page,
    party_inventory_page, party_personal_page, party_stats_page, religion_page,
    settlements_list_page, smith_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settlements", get(list_settlements))
        .route("/settlements/{id}", get(show_settlement))
        .route("/settlements/{id}/noticeboard", get(noticeboard))
        .route(
            "/settlements/{id}/party/{character_id}",
            get(party_personal),
        )
        .route(
            "/settlements/{id}/party/{character_id}/inventory",
            get(party_member),
        )
        .route(
            "/settlements/{id}/party/{character_id}/inventory/transfer",
            post(transfer_party_item),
        )
        .route(
            "/settlements/{id}/party/{character_id}/inventory/offer",
            post(finalize_party_offer),
        )
        .route(
            "/settlements/{id}/party/{character_id}/stats",
            get(party_stats),
        )
        .route(
            "/settlements/{id}/party/{character_id}/schedule",
            post(update_training_schedule),
        )
        .route("/settlements/{id}/tavern", get(redirect_to_inn))
        .route("/settlements/{id}/merchants", get(merchants))
        .route(
            "/settlements/{id}/merchants/offer",
            post(finalize_merchant_offer),
        )
        .route("/settlements/{id}/weapons", get(weapons))
        .route("/settlements/{id}/armor", get(armor))
        .route("/settlements/{id}/clothing", get(clothing))
        .route("/settlements/{id}/consumables", get(redirect_to_inn))
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

async fn show_settlement(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/settlements/{id}/noticeboard"))
}

async fn redirect_to_inn(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/settlements/{id}/inn"))
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
        noticeboard_page(
            settlement,
            &quests,
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

async fn party_personal(
    State(state): State<AppState>,
    Path((id, character_id)): Path<(String, u64)>,
    session: Session,
) -> Html<String> {
    if let Some(character_id) = session.character_id_u64() {
        let _ = state
            .db
            .call("synchronize_character_time", &[json!(character_id)])
            .await;
    }
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let Some((active_character, active_inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if character_id != active_character.id {
        return Html("<h1>Party member not found</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
    let attributes: Vec<CharacterAttributes> = state
        .db
        .query(&format!(
            "SELECT * FROM character_attributes WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let skills: Vec<CharacterSkills> = state
        .db
        .query(&format!(
            "SELECT * FROM character_skills WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let limbs: Vec<CharacterLimbs> = state
        .db
        .query(&format!(
            "SELECT * FROM character_limbs WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let schedule: Vec<CharacterTrainingSchedule> = state
        .db
        .query(&format!(
            "SELECT * FROM character_training_schedule WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    Html(
        party_personal_page(
            settlement,
            &active_character,
            &active_inventory,
            &party_members,
            attributes.first(),
            skills.first(),
            limbs.first(),
            schedule.first(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct TrainingScheduleForm {
    melee_minutes: u16,
    dodge_minutes: u16,
    block_minutes: u16,
    ranged_minutes: u16,
    will_minutes: u16,
    charisma_minutes: u16,
    medicine_minutes: u16,
    faith_minutes: u16,
    stealth_minutes: u16,
    balance_minutes: u16,
    surgeon_minutes: u16,
    labor_minutes: u16,
}

async fn update_training_schedule(
    State(state): State<AppState>,
    Path((settlement_id, character_id)): Path<(String, u64)>,
    session: Session,
    Form(form): Form<TrainingScheduleForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id) {
        let _ = state
            .db
            .call(
                "update_training_schedule",
                &[
                    json!(character_id), json!(form.melee_minutes), json!(form.dodge_minutes),
                    json!(form.block_minutes), json!(form.ranged_minutes), json!(form.will_minutes),
                    json!(form.charisma_minutes), json!(form.medicine_minutes), json!(form.faith_minutes),
                    json!(form.stealth_minutes), json!(form.balance_minutes), json!(form.surgeon_minutes),
                    json!(form.labor_minutes),
                ],
            )
            .await;
    }
    Redirect::to(&format!("/settlements/{settlement_id}/party/{character_id}"))
}

async fn party_member(
    State(state): State<AppState>,
    Path((id, character_id)): Path<(String, u64)>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };

    let Some((active_character, active_inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
    if character_id != active_character.id
        && !party_members.iter().any(|member| member.id == character_id)
    {
        return Html("<h1>Party member not found</h1>".to_string());
    }

    let selected = if character_id == active_character.id {
        active_character.clone()
    } else {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {character_id}"
            ))
            .await
            .unwrap_or_default();
        match characters.into_iter().next() {
            Some(character) => character,
            None => return Html("<h1>Party member not found</h1>".to_string()),
        }
    };
    let selected_inventory: Vec<InventoryItem> = if character_id == active_character.id {
        active_inventory.clone()
    } else {
        state
            .db
            .query(&format!(
                "SELECT * FROM inventory_item WHERE character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
    };

    let selected_equip: Vec<CharacterEquip> = state
        .db
        .query(&format!(
            "SELECT * FROM character_equip WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let active_equip: Vec<CharacterEquip> = if character_id == active_character.id {
        selected_equip.clone()
    } else {
        state
            .db
            .query(&format!(
                "SELECT * FROM character_equip WHERE character_id = {}",
                active_character.id
            ))
            .await
            .unwrap_or_default()
    };
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();

    Html(
        party_inventory_page(
            settlement,
            &selected,
            &selected_inventory,
            &active_character,
            &active_inventory,
            &items,
            &party_members,
            selected_equip.first(),
            active_equip.first(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn party_stats(
    State(state): State<AppState>,
    Path((id, character_id)): Path<(String, u64)>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
    if character_id != active_character.id
        && !party_members.iter().any(|member| member.id == character_id)
    {
        return Html("<h1>Party member not found</h1>".to_string());
    }
    let selected = if character_id == active_character.id {
        active_character.clone()
    } else {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {character_id}"
            ))
            .await
            .unwrap_or_default();
        match characters.into_iter().next() {
            Some(character) => character,
            None => return Html("<h1>Party member not found</h1>".to_string()),
        }
    };
    let selected_attributes: Vec<CharacterAttributes> = state
        .db
        .query(&format!(
            "SELECT * FROM character_attributes WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let selected_skills: Vec<CharacterSkills> = state
        .db
        .query(&format!(
            "SELECT * FROM character_skills WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let selected_limbs: Vec<CharacterLimbs> = state
        .db
        .query(&format!(
            "SELECT * FROM character_limbs WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let active_id = active_character.id;
    let active_attributes: Vec<CharacterAttributes> = state
        .db
        .query(&format!(
            "SELECT * FROM character_attributes WHERE character_id = {active_id}"
        ))
        .await
        .unwrap_or_default();
    let active_skills: Vec<CharacterSkills> = state
        .db
        .query(&format!(
            "SELECT * FROM character_skills WHERE character_id = {active_id}"
        ))
        .await
        .unwrap_or_default();
    let active_limbs: Vec<CharacterLimbs> = state
        .db
        .query(&format!(
            "SELECT * FROM character_limbs WHERE character_id = {active_id}"
        ))
        .await
        .unwrap_or_default();
    Html(
        party_stats_page(
            settlement,
            &selected,
            &active_character,
            &party_members,
            selected_attributes.first(),
            selected_skills.first(),
            selected_limbs.first(),
            active_attributes.first(),
            active_skills.first(),
            active_limbs.first(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct PartyTransferForm {
    from_character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
}

#[derive(Deserialize)]
struct PartyOfferForm {
    from_character_ids: String,
    to_character_ids: String,
    inventory_item_ids: String,
    quantities: String,
}

async fn finalize_party_offer(
    State(state): State<AppState>,
    Path((settlement_id, character_id)): Path<(String, u64)>,
    session: Session,
    Form(form): Form<PartyOfferForm>,
) -> Redirect {
    if let Some((active, _)) = get_active_character(&state, session.character_id_u64()).await {
        let parse = |value: &str| {
            value
                .split(',')
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
        };
        let quantities = form
            .quantities
            .split(',')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>();
        if let (Ok(from_ids), Ok(to_ids), Ok(item_ids), Ok(quantities)) = (
            parse(&form.from_character_ids),
            parse(&form.to_character_ids),
            parse(&form.inventory_item_ids),
            quantities,
        ) {
            if from_ids
                .iter()
                .all(|id| *id == active.id || *id == character_id)
                && to_ids
                    .iter()
                    .all(|id| *id == active.id || *id == character_id)
            {
                let _ = state
                    .db
                    .call(
                        "finalize_party_offer",
                        &[
                            json!(from_ids),
                            json!(to_ids),
                            json!(item_ids),
                            json!(quantities),
                        ],
                    )
                    .await;
            }
        }
    }
    Redirect::to(&format!(
        "/settlements/{settlement_id}/party/{character_id}/inventory"
    ))
}

async fn transfer_party_item(
    State(state): State<AppState>,
    Path((settlement_id, recipient_id)): Path<(String, u64)>,
    session: Session,
    Form(form): Form<PartyTransferForm>,
) -> Redirect {
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    if form.from_character_id != active_character.id && recipient_id != active_character.id {
        return Redirect::to(&format!("/settlements/{settlement_id}"));
    }
    let to_character_id = if form.from_character_id == active_character.id {
        recipient_id
    } else {
        active_character.id
    };
    if let Err(error) = state
        .db
        .call(
            "transfer_party_item",
            &[
                json!(form.from_character_id),
                json!(to_character_id),
                json!(form.inventory_item_id),
                json!(form.quantity),
            ],
        )
        .await
    {
        tracing::warn!("Party item transfer failed: {error}");
    }
    let comparison_character_id = if form.from_character_id == active_character.id {
        recipient_id
    } else {
        form.from_character_id
    };
    Redirect::to(&format!(
        "/settlements/{settlement_id}/party/{comparison_character_id}/inventory"
    ))
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
    let Some((character, inventory)) = active_character.as_ref() else {
        return Html(
            merchants_page(
                settlement,
                None,
                &[],
                &party_members,
                logged_in_as.as_deref(),
                session.theme(),
            )
            .into_string(),
        );
    };
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let equip: Vec<CharacterEquip> = state
        .db
        .query(&format!(
            "SELECT * FROM character_equip WHERE character_id = {}",
            character.id
        ))
        .await
        .unwrap_or_default();
    Html(
        live_merchant_shop_page(
            settlement,
            character,
            inventory,
            &items,
            &party_members,
            equip.first(),
            session.theme(),
            MerchantShop::General,
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct MerchantOfferForm {
    buy_item_ids: String,
    buy_quantities: String,
    #[serde(default)]
    sell_inventory_ids: String,
    #[serde(default)]
    sell_quantities: String,
    #[serde(default)]
    return_to: String,
}

async fn finalize_merchant_offer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<MerchantOfferForm>,
) -> Redirect {
    if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await {
        let items = form
            .buy_item_ids
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let quantities = form
            .buy_quantities
            .split(',')
            .filter_map(|value| value.parse::<u32>().ok())
            .collect::<Vec<_>>();
        if items.len() == quantities.len() {
            let sell_ids = form
                .sell_inventory_ids
                .split(',')
                .filter_map(|value| value.parse::<u64>().ok())
                .collect::<Vec<_>>();
            let sell_quantities = form
                .sell_quantities
                .split(',')
                .filter_map(|value| value.parse::<u32>().ok())
                .collect::<Vec<_>>();
            if !items.is_empty() || !sell_ids.is_empty() {
                let _ = state
                    .db
                    .call(
                        "finalize_merchant_trade",
                        &[
                            json!(character.id),
                            json!(items),
                            json!(quantities),
                            json!(sell_ids),
                            json!(sell_quantities),
                        ],
                    )
                    .await;
            }
        }
    }
    let return_to = match form.return_to.as_str() {
        "weapons" | "armor" | "clothing" | "merchants" => form.return_to,
        _ => "merchants".to_owned(),
    };
    Redirect::to(&format!("/settlements/{id}/{return_to}"))
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

async fn weapons(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Weapons).await
}

async fn armor(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Armor).await
}

async fn clothing(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Clothing).await
}

async fn religion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
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

async fn merchant_shop(
    state: AppState,
    id: String,
    session: Session,
    shop: MerchantShop,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
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
    let Some((character, inventory)) = active_character.as_ref() else {
        return Html(
            merchants_page(
                settlement,
                None,
                &[],
                &party_members,
                logged_in_as.as_deref(),
                session.theme(),
            )
            .into_string(),
        );
    };
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let equip: Vec<CharacterEquip> = state
        .db
        .query(&format!(
            "SELECT * FROM character_equip WHERE character_id = {}",
            character.id
        ))
        .await
        .unwrap_or_default();
    Html(
        live_merchant_shop_page(
            settlement,
            character,
            inventory,
            &items,
            &party_members,
            equip.first(),
            session.theme(),
            shop,
        )
        .into_string(),
    )
}

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

    let inventory = active_character
        .as_ref()
        .map_or_else(Vec::new, |(_, inventory)| inventory.clone());

    Html(
        render(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            &inventory,
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
        .query(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
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

async fn get_active_party_members(
    state: &AppState,
    active_character: Option<&Character>,
) -> Vec<Character> {
    let Some(party_id) = active_character.and_then(|character| character.party_id.as_ref()) else {
        return Vec::new();
    };
    let memberships: Vec<PartyMember> = state
        .db
        .query(&format!(
            "SELECT * FROM party_member WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();

    let mut members = Vec::new();
    for membership in memberships {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {}",
                membership.character_id
            ))
            .await
            .unwrap_or_default();
        if let Some(character) = characters.into_iter().next() {
            members.push(character);
        }
    }
    members
}
