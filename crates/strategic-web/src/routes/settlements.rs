//! Settlement route handlers

use adventuresim_core::prelude::{
    ProvisioningInputs, STANDARD_TRAVEL_RATION_ID, STANDARD_WATERSKIN_ID,
    STRATEGIC_PROVISION_BUFFER_PERCENT, STRATEGIC_TRAVEL_KCAL_PER_DAY,
    STRATEGIC_TRAVEL_WATER_ML_PER_DAY, Skill,
};
use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AppState;
use super::inventory_forms::{
    DiscardInventoryForm, MerchantOfferForm, PartyOfferForm, PartyPoolTransferForm,
};
use super::travel::{
    TravelDestination, TravelForm, TravelProvisionForecast, TravelerProvisionForecast,
    connected_destinations, next_settlement_toward, populate_camp_forecasts,
};
use crate::session::Session;
use crate::spacetimedb::sql_string_literal;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterCondition, CharacterEquip,
    CharacterLimbs, CharacterMoraleSource, CharacterNeeds, CharacterNotoriety,
    CharacterPersonality, CharacterSkills, CharacterStats, CharacterStrategicCondition,
    CharacterTime, CharacterTrainingSchedule, CommittedCutRow, EquippedMedication,
    HerbalistExaminationRow, InfectionEpisodeRow, InventoryItem, InventoryQuantityTarget,
    ItemCondition, ItemDefinition, ItemSlot, MedicalExaminationRow, Party, PartyInventoryItem,
    PartyJourney, PartyMember, PartyRecruitmentRole, PartyStake, Quest, QuestIssuer, QuestStatus,
    RecruitmentRequirements, ReligiousDemand, RepairOrder, ScheduleAllocation, Settlement,
    SettlementAlias, SettlementDescription, SettlementSmith, TravelEdge,
};
use crate::templates::settlement::{
    ActivityPreviewRates, LocationKind, LocationView, MerchantShop, RestSummary, alchemy_page,
    camp_page, inn_page, live_merchant_shop_page, merchants_page, party_discard_page,
    party_inventory_page, party_personal_page, party_pool_page, party_stats_page, religion_page,
    rest_result_page, settlement_map_page, settlement_overview_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settlements/{id}", get(show_settlement))
        .route("/locations/settlement/{id}", get(show_settlement_location))
        .route("/locations/settlement/{id}/map", get(settlement_map))
        .route("/locations/settlement/{id}/alchemy", get(alchemy))
        .route(
            "/locations/settlement/{id}/alchemy/craft",
            post(craft_medication),
        )
        .route(
            "/locations/settlement/{id}/map/travel-configuration",
            post(update_travel_configuration),
        )
        .route("/camp", get(camp))
        .route("/camp/rest", post(rest_at_camp))
        .route("/camp/continue", post(continue_camp_travel))
        .route(
            "/api/settlements/{id}/service-quests",
            get(service_quest_offers),
        )
        .route(
            "/api/settlements/{id}/religion",
            get(religion_dialogue).post(set_religion),
        )
        .route(
            "/api/settlements/{id}/herbalist/examination",
            post(herbalist_examination),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}",
            get(party_personal),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory",
            get(party_member),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory/transfer",
            post(transfer_party_item),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/remove",
            post(remove_party_member),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory/offer",
            post(finalize_party_offer),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/inventory/discard",
            post(discard_inventory_items),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory",
            get(party_pool_inventory),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory/deposit",
            post(deposit_party_inventory),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory/withdraw",
            post(withdraw_party_inventory),
        )
        .route(
            "/locations/{kind}/{id}/party-inventory/liquidate",
            post(liquidate_party_assets),
        )
        .route("/api/inventory-target", post(set_inventory_target))
        .route("/api/equipment", post(set_equipment))
        .route(
            "/locations/{kind}/{id}/party/{character_id}/stats",
            get(party_stats),
        )
        .route(
            "/locations/{kind}/{id}/party/{target_id}/examine",
            post(examine_patient),
        )
        .route(
            "/locations/{kind}/{id}/party/{target_id}/medication/{equipment_id}/unequip",
            post(unequip_medication),
        )
        .route(
            "/locations/{kind}/{id}/party/{target_id}/examination/{examination_id}/dismiss",
            post(dismiss_medical_examination),
        )
        .route(
            "/locations/{kind}/{id}/players/{character_id}",
            get(party_stats),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/schedule",
            post(update_training_schedule),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/religion/renounce",
            post(renounce_religion),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/religious-demand/{demand_id}",
            post(resolve_religious_demand),
        )
        .route("/settlements/{id}/merchants", get(merchants))
        .route(
            "/settlements/{id}/merchants/offer",
            post(finalize_merchant_offer),
        )
        .route("/settlements/{id}/weapons", get(weapons))
        .route("/settlements/{id}/armor", get(armor))
        .route("/settlements/{id}/{shop}/repair", post(submit_repair))
        .route(
            "/settlements/{id}/{shop}/repair-all",
            post(submit_all_repairs),
        )
        .route(
            "/settlements/{id}/{shop}/repairs/{order_id}/retrieve",
            post(retrieve_repair),
        )
        .route(
            "/settlements/{id}/{shop}/repairs/retrieve",
            post(retrieve_repairs),
        )
        .route("/settlements/{id}/clothing", get(clothing))
        .route("/settlements/{id}/herbalist", get(herbalist))
        .route(
            "/settlements/{id}/herbalist/purchase",
            post(purchase_from_herbalist),
        )
        .route("/settlements/{id}/inn", get(inn))
        .route("/settlements/{id}/religion", get(religion))
        .route("/settlements/{id}/rest/{kind}", post(rest))
        .route("/settlements/{id}/travel", post(travel))
}

#[derive(Default, Deserialize)]
struct AlchemyQuery {
    recipe: Option<String>,
    scope: Option<String>,
}

#[derive(Deserialize)]
struct CraftMedicationForm {
    disease_id: String,
    #[serde(default)]
    party_scope: bool,
}

async fn alchemy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<AlchemyQuery>,
    session: Session,
) -> Html<String> {
    let Some((character, inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    if character.current_settlement_id.as_deref() != Some(&id) {
        return Html("<h1>Your character is not at this settlement</h1>".into());
    }
    let medicine = get_character_capability(&state, character.id)
        .await
        .map_or(0.0, |capability| capability.medicine);
    if medicine < adventuresim_core::disease::MEDICINE_VITALS_THRESHOLD {
        return Html("<h1>Medicine 2 is required to prepare medication</h1>".into());
    }
    let settlement: Option<Settlement> = state
        .db
        .query_one(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlement else {
        return Html("<h1>Settlement not found</h1>".into());
    };
    let selected = query
        .recipe
        .as_deref()
        .and_then(adventuresim_core::disease::medication_recipe_for_item)
        .filter(|recipe| adventuresim_core::disease::can_prepare_medication(medicine, recipe))
        .or_else(|| {
            adventuresim_core::disease::MEDICATION_RECIPES
                .iter()
                .find(|recipe| adventuresim_core::disease::can_prepare_medication(medicine, recipe))
        })
        .expect("Medicine 2 always unlocks at least one recipe");
    let party_scope = query.scope.as_deref() == Some("party");
    let (members, items, trade_context) = tokio::join!(
        get_active_party_members(&state, Some(&character)),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
        inventory_trade_context(&state, &character),
    );
    let (personal_targets, party_targets, pooled) = trade_context;
    Html(
        alchemy_page(
            &settlement,
            &character,
            &members,
            medicine,
            selected,
            &inventory,
            &pooled,
            &items.unwrap_or_default(),
            &personal_targets,
            &party_targets,
            party_scope,
            session.theme(),
        )
        .into_string(),
    )
}

async fn craft_medication(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<CraftMedicationForm>,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64()
        && let Err(error) = state
            .db
            .call(
                "craft_medication",
                &[
                    json!(character_id),
                    json!(form.disease_id),
                    json!(form.party_scope),
                ],
            )
            .await
    {
        tracing::warn!(%error, character_id, "medication crafting rejected");
    }
    let recipe = adventuresim_core::disease::MEDICATION_RECIPES
        .iter()
        .find(|recipe| format!("{:?}", recipe.disease_id).eq_ignore_ascii_case(&form.disease_id))
        .map_or("", |recipe| recipe.item_id);
    Redirect::to(&format!(
        "/locations/settlement/{id}/alchemy?recipe={recipe}&scope={}",
        if form.party_scope {
            "party"
        } else {
            "personal"
        }
    ))
}

#[derive(Deserialize)]
struct RepairItemForm {
    inventory_item_id: u64,
}

async fn submit_repair(
    State(state): State<AppState>,
    Path((id, shop)): Path<(String, String)>,
    session: Session,
    Form(form): Form<RepairItemForm>,
) -> Redirect {
    if matches!(shop.as_str(), "weapons" | "armor") {
        if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
        {
            let _ = state
                .db
                .call(
                    "submit_item_for_repair",
                    &[
                        json!(character.id),
                        json!(id),
                        json!(form.inventory_item_id),
                    ],
                )
                .await;
        }
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

async fn submit_all_repairs(
    State(state): State<AppState>,
    Path((id, shop)): Path<(String, String)>,
    session: Session,
) -> Redirect {
    if matches!(shop.as_str(), "weapons" | "armor") {
        if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
        {
            let _ = state
                .db
                .call(
                    "submit_all_repairable_items",
                    &[json!(character.id), json!(id), json!(shop == "armor")],
                )
                .await;
        }
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

async fn retrieve_repair(
    State(state): State<AppState>,
    Path((id, shop, order_id)): Path<(String, String, u64)>,
    session: Session,
) -> Redirect {
    if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await {
        let _ = state
            .db
            .call(
                "retrieve_repaired_item",
                &[json!(character.id), json!(order_id)],
            )
            .await;
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

#[derive(Deserialize)]
struct RetrieveRepairsForm {
    item_id: Option<String>,
    limit: u32,
}

async fn retrieve_repairs(
    State(state): State<AppState>,
    Path((id, shop)): Path<(String, String)>,
    session: Session,
    Form(form): Form<RetrieveRepairsForm>,
) -> Redirect {
    if matches!(shop.as_str(), "weapons" | "armor")
        && let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    {
        let _ = state
            .db
            .call(
                "retrieve_repaired_items",
                &[
                    json!(character.id),
                    json!(id),
                    json!(shop == "armor"),
                    json!(form.item_id),
                    json!(form.limit),
                ],
            )
            .await;
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

async fn show_settlement(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/locations/settlement/{id}"))
}

async fn show_settlement_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement_literal = sql_string_literal(&id);
    let alias_sql =
        format!("SELECT * FROM settlement_alias WHERE settlement_id = {settlement_literal}");
    let description_sql =
        format!("SELECT * FROM settlement_description WHERE settlement_id = {settlement_literal}");
    let (settlements, aliases, descriptions, active_character) = tokio::join!(
        state.db.query::<Settlement>("SELECT * FROM settlement"),
        state.db.query::<SettlementAlias>(&alias_sql),
        state.db.query::<SettlementDescription>(&description_sql),
        get_active_character(&state, session.character_id_u64()),
    );
    let settlements = match settlements {
        Ok(settlements) => settlements,
        Err(error) => {
            tracing::error!(%error, settlement_id = %id, "failed to load settlements");
            return Html("<h1>Settlement data unavailable</h1>".to_string());
        }
    };
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let aliases = aliases.unwrap_or_else(|error| {
        tracing::warn!(%error, settlement_id = %id, "failed to load settlement aliases");
        Vec::new()
    });
    let descriptions = descriptions.unwrap_or_else(|error| {
        tracing::warn!(%error, settlement_id = %id, "failed to load settlement descriptions");
        Vec::new()
    });
    let mut aliases: Vec<_> = aliases
        .into_iter()
        .filter(|alias| alias.settlement_id == id)
        .collect();
    aliases.sort_by(|left, right| left.id.cmp(&right.id));
    let mut descriptions: Vec<_> = descriptions
        .into_iter()
        .filter(|description| description.settlement_id == id)
        .collect();
    descriptions.sort_by(|left, right| left.id.cmp(&right.id));
    Html(
        settlement_overview_page(
            settlement,
            &aliases,
            &descriptions,
            active_character.as_ref().map(|(character, _)| character),
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Default, Deserialize)]
struct LocationMapQuery {
    destination: Option<String>,
}

async fn settlement_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LocationMapQuery>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let edges: Vec<TravelEdge> = state
        .db
        .query("SELECT * FROM travel_edge")
        .await
        .unwrap_or_default();
    let mut destinations = connected_destinations(settlement, &settlements, &edges);
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let active_party = if let Some(party_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.party_id.as_ref())
    {
        state
            .db
            .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let active_quest = if let Some(quest_id) = active_party
        .as_ref()
        .and_then(|party| party.active_quest_id.as_ref())
    {
        state
            .db
            .query::<Quest>(&format!("SELECT * FROM quest WHERE id = '{}'", quest_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let can_travel = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(&settlement.id) && active_party.is_some()
    });
    if let Some(quest) = active_quest
        .as_ref()
        .filter(|quest| quest.status == QuestStatus::Accepted)
    {
        if can_travel && settlement.id == quest.settlement_id {
            let distance_m = crate::routes::quests::straight_line_distance_m(quest, settlement);
            destinations.push(TravelDestination {
                id: quest.id.clone(),
                name: quest.title.clone(),
                description: quest.description.clone(),
                summary: Some(format!(
                    "Active quest · {} {}",
                    quest.enemy_count, quest.enemy_type
                )),
                travel_action: format!("/quests/{}/travel", quest.id),
                distance_m,
                journey_minutes: crate::routes::quests::offroad_journey_minutes(distance_m),
                camp_stop_minutes: Vec::new(),
                camp_forecasts: Vec::new(),
                quest_in_progress: true,
                active_quest_route: false,
                turn_in_ready: false,
            });
        } else if can_travel {
            if let Some(next_settlement_id) =
                next_settlement_toward(settlement, &quest.settlement_id, &settlements, &edges)
            {
                if let Some(destination) = destinations
                    .iter_mut()
                    .find(|destination| destination.id == next_settlement_id)
                {
                    destination.active_quest_route = true;
                }
            }
        }
    }
    if let Some(quest) = active_quest
        .as_ref()
        .filter(|quest| quest.status == QuestStatus::Completed)
    {
        for destination in &mut destinations {
            destination.turn_in_ready = destination.id == quest.settlement_id;
        }
    }
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    if can_travel {
        if let Some(party) = active_party.as_ref() {
            let attributes: Vec<CharacterAttributes> = state
                .db
                .query("SELECT * FROM character_attributes")
                .await
                .unwrap_or_default();
            let limbs: Vec<CharacterLimbs> = state
                .db
                .query("SELECT * FROM character_limbs")
                .await
                .unwrap_or_default();
            let stats: Vec<CharacterStats> = state
                .db
                .query("SELECT * FROM character_stats")
                .await
                .unwrap_or_default();
            let member_ids: Vec<_> = party_members.iter().map(|member| member.id).collect();
            populate_camp_forecasts(
                &mut destinations,
                &member_ids,
                &attributes,
                &limbs,
                &stats,
                party.camp_fatigue_percent,
            );
        }
    }
    let provision_forecast = if can_travel {
        if let Some(destination) = query
            .destination
            .as_deref()
            .and_then(|id| destinations.iter().find(|destination| destination.id == id))
        {
            travel_provision_forecast(&state, &party_members, destination)
                .await
                .ok()
                .flatten()
        } else {
            None
        }
    } else {
        None
    };
    Html(
        settlement_map_page(
            settlement,
            &destinations,
            query.destination.as_deref(),
            active_character.as_ref().map(|(character, _)| character),
            active_party.as_ref(),
            &party_members,
            can_travel,
            provision_forecast.as_ref(),
            active_character
                .as_ref()
                .map(|(character, _)| character.name.as_str()),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct TravelConfigurationForm {
    fatigue_percent: u8,
}

async fn update_travel_configuration(
    State(state): State<AppState>,
    Path(_id): Path<String>,
    session: Session,
    Form(form): Form<TravelConfigurationForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "set_party_camp_fatigue_percent",
            &[
                json!(character_id),
                json!(form.fatigue_percent.clamp(10, 100)),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn camp(State(state): State<AppState>, session: Session) -> Response {
    let Some((character, _inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters").into_response();
    };
    let Some(party_id) = character.party_id.as_deref() else {
        return Redirect::to("/").into_response();
    };
    // A reducer response can arrive a fraction before its row is visible to
    // the SQL endpoint. Retry briefly so a completed travel POST resolves to
    // camp rather than falling through to the character picker.
    let mut party = None;
    for attempt in 0..4 {
        party = state
            .db
            .query_one::<Party>(&format!("SELECT * FROM party WHERE id = '{party_id}'"))
            .await
            .ok()
            .flatten();
        if party
            .as_ref()
            .is_some_and(|party| party.camp_destination_id.is_some())
        {
            break;
        }
        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }
    let Some(party) = party else {
        return Redirect::to("/").into_response();
    };
    let Some(destination_id) = party.camp_destination_id.as_deref() else {
        return Redirect::to("/").into_response();
    };
    let destination_name = match party.camp_destination_kind.as_deref() {
        Some("settlement") => state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = '{destination_id}'"
            ))
            .await
            .ok()
            .flatten()
            .map(|item| item.name),
        Some("quest") => state
            .db
            .query_one::<Quest>(&format!(
                "SELECT * FROM quest WHERE id = '{destination_id}'"
            ))
            .await
            .ok()
            .flatten()
            .map(|item| item.title),
        _ => None,
    }
    .unwrap_or_else(|| "Unknown destination".into());
    // The party and journey rows are committed atomically, but the SQL view
    // can observe the camp row a fraction before the journey projection.
    // Retry briefly so the first camp render retains the original start.
    let mut journey = None;
    for attempt in 0..4 {
        journey = state
            .db
            .query_one::<PartyJourney>(&format!(
                "SELECT * FROM party_journey WHERE party_id = '{}'",
                party.id
            ))
            .await
            .ok()
            .flatten();
        if journey.is_some() || attempt == 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    let party_members = get_active_party_members(&state, Some(&character)).await;
    let stats: Vec<CharacterStats> = state
        .db
        .query("SELECT * FROM character_stats")
        .await
        .unwrap_or_default();
    let default_rest_minutes = party_members
        .iter()
        .filter_map(|member| stats.iter().find(|stat| stat.character_id == member.id))
        .map(|stat| ((stat.calories_used / STRATEGIC_TRAVEL_KCAL_PER_DAY) * 1_440.0).ceil() as u64)
        .max()
        .unwrap_or(0);
    Html(
        camp_page(
            &party,
            journey.as_ref(),
            &destination_name,
            Some(&character),
            &party_members,
            default_rest_minutes,
            Some(&character.name),
            session.theme(),
        )
        .into_string(),
    )
    .into_response()
}

#[derive(Deserialize)]
struct CampRestForm {
    duration: u64,
    unit: String,
}

fn rest_duration_minutes(duration: u64, unit: &str) -> u64 {
    match unit {
        "days" => duration.saturating_mul(1_440),
        _ => duration.saturating_mul(60),
    }
}

async fn rest_at_camp(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<CampRestForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "rest_at_camp",
            &[
                json!(character_id),
                json!(rest_duration_minutes(form.duration, &form.unit)),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn continue_camp_travel(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call("continue_camp_travel", &[json!(character_id)])
        .await
    {
        // Navigation follows the authoritative SSE revision once the party
        // state is visible to every connected member.
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn travel_provision_forecast(
    state: &AppState,
    travelers: &[Character],
    destination: &TravelDestination,
) -> Result<Option<TravelProvisionForecast>, String> {
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .map_err(|error| error.to_string())?;
    let Some(ration) = items
        .iter()
        .find(|item| item.id == STANDARD_TRAVEL_RATION_ID)
    else {
        return Ok(None);
    };
    let Some(waterskin) = items.iter().find(|item| item.id == STANDARD_WATERSKIN_ID) else {
        return Ok(None);
    };
    let planning_minutes = if destination.quest_in_progress {
        destination.journey_minutes.saturating_mul(2)
    } else {
        destination.journey_minutes
    };
    let mut forecast = TravelProvisionForecast::default();
    for traveler in travelers {
        let Some(needs) = state
            .db
            .query_one::<CharacterNeeds>(&format!(
                "SELECT * FROM character_needs WHERE character_id = {}",
                traveler.id
            ))
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let inventory: Vec<InventoryItem> = state
            .db
            .query(&format!(
                "SELECT * FROM inventory_item WHERE character_id = {}",
                traveler.id
            ))
            .await
            .map_err(|error| error.to_string())?;
        let required = ProvisioningInputs {
            planning_minutes,
            buffer_percent: STRATEGIC_PROVISION_BUFFER_PERCENT,
            food_balance_kcal: needs.food_balance_kcal,
            water_balance_ml: needs.water_balance_ml,
            travel_kcal_per_day: STRATEGIC_TRAVEL_KCAL_PER_DAY,
            travel_water_ml_per_day: STRATEGIC_TRAVEL_WATER_ML_PER_DAY,
            ration_kcal: ration.nutrition_kcal,
            waterskin_capacity_ml: waterskin.water_capacity_ml,
        }
        .required_units();
        let owned = |item_id: &str| {
            inventory
                .iter()
                .filter(|entry| entry.item_id == item_id)
                .map(|entry| entry.qty)
                .sum::<u32>()
        };
        let rations_to_buy = required
            .rations
            .saturating_sub(owned(STANDARD_TRAVEL_RATION_ID));
        let waterskins_to_buy = required
            .waterskins
            .saturating_sub(owned(STANDARD_WATERSKIN_ID));
        let cost = rations_to_buy
            .saturating_mul(ration.base_value.unwrap_or(0))
            .saturating_add(waterskins_to_buy.saturating_mul(waterskin.base_value.unwrap_or(0)));
        forecast.total_cost = forecast.total_cost.saturating_add(cost);
        forecast.travelers.push(TravelerProvisionForecast {
            name: traveler.name.clone(),
            rations_to_buy,
            waterskins_to_buy,
            cost,
        });
    }
    Ok(Some(forecast))
}

#[derive(Serialize)]
struct ServiceQuestOffer {
    id: String,
    title: String,
    service_id: String,
    npc_name: &'static str,
    greeting: String,
    problem: String,
    follow_up: String,
    details: String,
    acceptance: &'static str,
    state: &'static str,
    waiting: &'static str,
    turn_in_response: String,
    can_accept: bool,
    can_turn_in: bool,
    recruitment: Option<ServiceQuestRecruitment>,
}

#[derive(Serialize)]
struct ServiceQuestRecruitment {
    party_name: String,
    leader_id: String,
    leader_name: String,
    roles: Vec<ServiceQuestRole>,
}

#[derive(Serialize)]
struct ServiceQuestRole {
    id: u64,
    name: String,
    remaining: u32,
    requirements: Vec<String>,
    requirements_summary: String,
    match_level: &'static str,
    match_summary: String,
    left_html: String,
    right_html: String,
}

async fn service_quest_offers(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<Vec<ServiceQuestOffer>> {
    if state.db.is_local() {
        let _ = state
            .db
            .call("ensure_settlement_activity", &[json!(id.clone())])
            .await;
    }
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Json(Vec::new());
    };
    let issuers: Vec<QuestIssuer> = state
        .db
        .query(&format!(
            "SELECT * FROM quest_issuer WHERE settlement_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let quests: Vec<Quest> = state
        .db
        .query(&format!(
            "SELECT * FROM quest WHERE settlement_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let edges: Vec<TravelEdge> = state
        .db
        .query("SELECT * FROM travel_edge")
        .await
        .unwrap_or_default();
    let neighboring_name = connected_destinations(settlement, &settlements, &edges)
        .first()
        .map(|destination| destination.name.clone())
        .unwrap_or_else(|| "the next settlement".to_string());
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let active_party = if let Some(party_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.party_id.as_ref())
    {
        state
            .db
            .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let can_accept = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(id.as_str())
            && active_party
                .as_ref()
                .is_some_and(|party| party.active_quest_id.is_none())
    });
    let can_turn_in = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(id.as_str()) && active_party.is_some()
    });
    let parties: Vec<Party> = state
        .db
        .query("SELECT * FROM party")
        .await
        .unwrap_or_default();
    let party_memberships: Vec<PartyMember> = state
        .db
        .query("SELECT * FROM party_member")
        .await
        .unwrap_or_default();
    let recruitment_roles: Vec<PartyRecruitmentRole> = state
        .db
        .query("SELECT * FROM party_recruitment_role")
        .await
        .unwrap_or_default();
    let characters: Vec<Character> = state
        .db
        .query("SELECT * FROM character")
        .await
        .unwrap_or_default();
    let viewer_party_id = active_party.as_ref().map(|party| party.id.as_str());
    let viewer_member_ids: Vec<u64> = viewer_party_id
        .map(|party_id| {
            party_memberships
                .iter()
                .filter(|member| member.party_id == party_id)
                .map(|member| member.character_id)
                .collect()
        })
        .unwrap_or_default();
    let mut viewer_capabilities = Vec::new();
    for character_id in viewer_member_ids {
        let _ = state
            .db
            .call("refresh_capabilities", &[json!(character_id)])
            .await;
        if let Some(capability) = state
            .db
            .query::<CharacterCapability>(&format!(
                "SELECT * FROM character_capability WHERE character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
        {
            viewer_capabilities.push(capability);
        }
    }

    Json(
        issuers
            .into_iter()
            .filter_map(|issuer| {
                let quest = quests.iter().find(|quest| quest.id == issuer.quest_id)?;
                let is_current = active_party.as_ref().is_some_and(|party| {
                    party.active_quest_id.as_deref() == Some(quest.id.as_str())
                        && quest.accepted_by.as_deref() == Some(party.id.as_str())
                });
                let recruitment = quest.accepted_by.as_deref().and_then(|party_id| {
                    if viewer_party_id == Some(party_id) {
                        return None;
                    }
                    let party = parties.iter().find(|party| party.id == party_id)?;
                    if party.current_settlement_id.as_deref() != Some(id.as_str()) {
                        return None;
                    }
                    let leader = characters.iter().find(|character| character.id == party.leader_id)?;
                    let roles = recruitment_roles
                        .iter()
                        .filter(|role| role.party_id == party.id)
                        .filter_map(|role| {
                            let filled = party_memberships
                                .iter()
                                .filter(|member| member.recruitment_role_id == Some(role.id))
                                .count() as u32;
                            let remaining = role.quantity.saturating_sub(filled);
                            if remaining == 0 {
                                return None;
                            }
                            let requirements = role_requirement_labels(role);
                            let (match_level, match_summary) =
                                party_role_match(&viewer_capabilities, role);
                            let (left_html, right_html) = crate::templates::recruitment::service_role_inspection(
                                &role.name,
                                &requirements,
                                &party.name,
                                &leader.name,
                                remaining,
                                match_level,
                                &match_summary,
                                &format!("/party-roles/{}/join", role.id),
                                can_accept,
                            );
                            Some(ServiceQuestRole {
                                id: role.id,
                                name: role.name.clone(),
                                remaining,
                                requirements_summary: if requirements.is_empty() {
                                    "No minimum recommendations".to_string()
                                } else {
                                    requirements.join(" · ")
                                },
                                requirements,
                                match_level,
                                match_summary,
                                left_html,
                                right_html,
                            })
                        })
                        .collect::<Vec<_>>();
                    Some(ServiceQuestRecruitment {
                        party_name: party.name.clone(),
                        leader_id: leader.id.to_string(),
                        leader_name: leader.name.clone(),
                        roles,
                    })
                });
                let state = if quest.status == QuestStatus::Available {
                    "available"
                } else if is_current && quest.status == QuestStatus::Completed {
                    "ready"
                } else if is_current {
                    "underway"
                } else if recruitment.is_some() {
                    "recruiting"
                } else {
                    return None;
                };
                let problem = quest.description.trim_end_matches('.').to_lowercase();
                let low = (quest.enemy_count - 2).max(1);
                let high = quest.enemy_count + 2;
                let (npc_name, greeting) = service_quest_greeting(&issuer.service_id);
                Some(ServiceQuestOffer {
                    id: quest.id.clone(),
                    title: quest.title.clone(),
                    service_id: issuer.service_id.clone(),
                    npc_name,
                    greeting: greeting.to_string(),
                    follow_up: format!("{problem}?"),
                    problem,
                    details: service_quest_details(
                        &issuer.service_id,
                        quest,
                        &settlement.name,
                        &neighboring_name,
                        low,
                        high,
                    ),
                    acceptance: "Splendid! And please, do be careful! You wouldn't be the first men they've slain.",
                    state,
                    waiting: "Hello again, I eagerly await the results of your efforts.",
                    turn_in_response: format!(
                        "Excellent work. Here is the promised {} gold. You've earned it.",
                        quest.gold_reward
                    ),
                    can_accept,
                    can_turn_in: can_turn_in && state == "ready",
                    recruitment,
                })
            })
            .collect(),
    )
}

fn service_quest_greeting(service_id: &str) -> (&'static str, &'static str) {
    match service_id {
        "weapons" => (
            "Weaponsmith",
            "Welcome. Business would be better, were it not for how",
        ),
        "armor" => ("Armourer", "Welcome. Production has nearly stopped because"),
        "clothing" => (
            "Clothier",
            "Welcome, traveler. Cloth is scarce of late because",
        ),
        "inn" => (
            "Innkeeper",
            "Welcome. Travelers have been avoiding this road because",
        ),
        "religion" => (
            "Priest",
            "God give you peace. I must ask your aid concerning",
        ),
        _ => (
            "Merchant",
            "Welcome, traveler. You'll have to excuse the sorry state of my inventory;",
        ),
    }
}

fn service_quest_details(
    service_id: &str,
    quest: &Quest,
    settlement_name: &str,
    neighboring_name: &str,
    low: i32,
    high: i32,
) -> String {
    let situation = match service_id {
        "weapons" => format!(
            "the thieves are hiding with the stolen arms near the road between {settlement_name} and {neighboring_name}"
        ),
        "armor" => format!(
            "the old mine between {settlement_name} and {neighboring_name} is choked with giant spiders, and no miner will go near it"
        ),
        "clothing" => format!(
            "the wolves are ranging through the grazing land between {settlement_name} and {neighboring_name}, where our shepherds cannot avoid them"
        ),
        "inn" => format!(
            "the goblins are lairing in a cave near the road between {settlement_name} and {neighboring_name} and attacking travelers after dark"
        ),
        "religion" => format!(
            "a necromancer has occupied an old crypt outside {settlement_name} and raised its dead"
        ),
        _ => format!(
            "a handful of bandits are camped in the forest near the road between {settlement_name} and {neighboring_name} and have been laying ambushes for my caravans"
        ),
    };
    format!(
        "Yes, {situation}. I believe there are about {low} or {high} {}, give or take. I'd offer {} gold to anyone who clears them out. Are you",
        quest.enemy_type, quest.gold_reward,
    )
}

fn role_requirement_labels(role: &PartyRecruitmentRole) -> Vec<String> {
    let requirements = role.requirements;
    let mut labels = Vec::new();
    for (required, label) in [
        (requirements.melee, "Melee"),
        (requirements.ranged, "Ranged"),
        (requirements.heavy, "Heavy"),
        (requirements.quarter_armor, "1/4 armor"),
        (requirements.half_armor, "1/2 armor"),
        (requirements.three_quarter_armor, "3/4 armor"),
        (requirements.full_armor, "Full armor"),
    ] {
        if required {
            labels.push(label.to_string());
        }
    }
    let precision = role.effective_weapon_precision();
    if precision > 0.0 {
        labels.push(format!("Weapon precision {precision:.1}+"));
    }
    for (minimum, label) in [
        (requirements.athletics, "Athletics"),
        (requirements.endurance, "Endurance"),
    ] {
        if minimum > 0 {
            labels.push(format!("{label} {minimum}+"));
        }
    }
    labels
}

fn party_role_match(
    capabilities: &[CharacterCapability],
    role: &PartyRecruitmentRole,
) -> (&'static str, String) {
    let total = role_requirement_labels(role).len();
    if total == 0 {
        return (
            "none",
            "This role has no minimum recommendations.".to_string(),
        );
    }
    let best = capabilities
        .iter()
        .map(|capability| matched_role_requirements(capability, role))
        .max()
        .unwrap_or(0);
    if best == total {
        (
            "all",
            "Someone in your party meets every recommendation.".to_string(),
        )
    } else if best > 0 {
        (
            "some",
            format!("Your best candidate meets {best} of {total} recommendations."),
        )
    } else {
        (
            "none-met",
            "No one in your party meets any recommendation.".to_string(),
        )
    }
}

fn matched_role_requirements(
    capability: &CharacterCapability,
    role: &PartyRecruitmentRole,
) -> usize {
    let requirements: RecruitmentRequirements = role.requirements;
    let mut matched = 0;
    for (required, present) in [
        (requirements.melee, capability.melee),
        (requirements.ranged, capability.ranged),
        (requirements.heavy, capability.heavy),
        (requirements.quarter_armor, capability.quarter_armor),
        (requirements.half_armor, capability.half_armor),
        (
            requirements.three_quarter_armor,
            capability.three_quarter_armor,
        ),
        (requirements.full_armor, capability.full_armor),
    ] {
        if required && present {
            matched += 1;
        }
    }
    if role.effective_weapon_precision() > 0.0
        && capability.weapon_precision >= role.effective_weapon_precision()
    {
        matched += 1;
    }
    for (minimum, value) in [
        (requirements.athletics, capability.athletics),
        (requirements.endurance, capability.endurance),
    ] {
        if minimum > 0 && adventuresim_core::capability::rating(value) >= minimum {
            matched += 1;
        }
    }
    matched
}

enum LocationLookup {
    Found(LocationView),
    NotFound,
    Unavailable,
}

async fn resolve_location(state: &AppState, kind: &str, id: &str) -> LocationLookup {
    let Ok(kind) = kind.parse::<LocationKind>() else {
        return LocationLookup::NotFound;
    };
    let location = match kind {
        LocationKind::Settlement => state
            .db
            .query_one::<Settlement>(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
            .await
            .map(|row| {
                row.map(|settlement| {
                    (
                        settlement.name,
                        Some(settlement.category),
                        Some(settlement.religion_id),
                    )
                })
            }),
        LocationKind::Quest => state
            .db
            .query_one::<Quest>(&format!("SELECT * FROM quest WHERE id = '{}'", id))
            .await
            .map(|row| row.map(|quest| (quest.title, None, None))),
    };
    let (name, category, religion_id) = match location {
        Ok(Some(location)) => location,
        Ok(None) => return LocationLookup::NotFound,
        Err(error) => {
            tracing::error!(%error, "failed to resolve location");
            return LocationLookup::Unavailable;
        }
    };
    LocationLookup::Found(LocationView {
        kind,
        id: id.to_string(),
        name,
        religion_id,
        category,
    })
}

fn character_is_at_location(character: &Character, location: &LocationView) -> bool {
    match location.kind {
        LocationKind::Settlement => {
            character.current_settlement_id.as_deref() == Some(location.id.as_str())
        }
        LocationKind::Quest => {
            character.current_quest_location_id.as_deref() == Some(location.id.as_str())
        }
    }
}

async fn party_personal(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Html<String> {
    if let Some(character_id) = session.character_id_u64() {
        if let Err(error) = state
            .db
            .call("synchronize_character_time", &[json!(character_id)])
            .await
        {
            tracing::error!("Failed to liquidate party inventory: {error:?}");
        }
    }
    let location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".to_string()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".to_string());
        }
    };
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if !character_is_at_location(&active_character, &location) {
        return Html("<h1>Your party is not at this location</h1>".to_string());
    }
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
    let capability = get_character_capability(&state, character_id).await;
    let can_examine = get_character_capability(&state, active_character.id)
        .await
        .is_some_and(|capability| {
            capability.medicine >= adventuresim_core::disease::MEDICINE_VITALS_THRESHOLD
        });
    let stats = query_single::<CharacterStats>(&state, "character_stats", character_id).await;
    let settlement = if location.kind == LocationKind::Settlement {
        state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(&location.id)
            ))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let activity_preview = ActivityPreviewRates::from_character(
        attributes.first(),
        skills.first(),
        limbs.first(),
        capability.as_ref(),
        settlement.as_ref(),
        stats.as_ref(),
    );
    let condition = get_strategic_condition(&state, character_id).await;
    let morale_sources = get_morale_sources(&state, character_id).await;
    let religion = query_single::<CharacterCondition>(&state, "character_condition", character_id)
        .await
        .and_then(|condition| condition.religion_id);
    let notoriety = query_single::<CharacterNotoriety>(&state, "character_notoriety", character_id)
        .await
        .map_or(0.0, |notoriety| notoriety.value);
    let personality =
        query_single::<CharacterPersonality>(&state, "character_personality", character_id).await;
    let medical = medical_presentation(&state, character_id, character_id).await;
    let religious_demand = state
        .db
        .query::<ReligiousDemand>(&format!(
            "SELECT * FROM religious_demand WHERE character_id = {character_id} AND status = 'pending'"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    Html(
        party_personal_page(
            &location,
            &active_character,
            &party_members,
            capability.as_ref(),
            attributes.first(),
            skills.first(),
            limbs.first(),
            condition.as_ref(),
            &morale_sources,
            religion.as_deref(),
            schedule.first(),
            activity_preview,
            religious_demand.as_ref(),
            notoriety,
            personality.as_ref(),
            &medical,
            can_examine,
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct ReligiousDemandForm {
    choice: String,
}

async fn resolve_religious_demand(
    State(state): State<AppState>,
    Path((kind, id, character_id, demand_id)): Path<(String, String, u64, u64)>,
    session: Session,
    Form(form): Form<ReligiousDemandForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id)
        && let Err(error) = state
            .db
            .call(
                "resolve_religious_demand",
                &[json!(demand_id), json!(form.choice)],
            )
            .await
    {
        tracing::warn!(%error, character_id, demand_id, "failed to resolve religious demand");
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party/{character_id}"))
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
    smithing_minutes: u16,
    labor_minutes: u16,
    prayer_minutes: u16,
    thievery_minutes: u16,
    raiding_minutes: u16,
}

async fn update_training_schedule(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<TrainingScheduleForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id) {
        let _ = state
            .db
            .call(
                "update_training_schedule",
                &[
                    json!(character_id),
                    json!(ScheduleAllocation {
                        melee_minutes: form.melee_minutes,
                        dodge_minutes: form.dodge_minutes,
                        block_minutes: form.block_minutes,
                        ranged_minutes: form.ranged_minutes,
                        will_minutes: form.will_minutes,
                        charisma_minutes: form.charisma_minutes,
                        medicine_minutes: form.medicine_minutes,
                        faith_minutes: form.faith_minutes,
                        stealth_minutes: form.stealth_minutes,
                        balance_minutes: form.balance_minutes,
                        surgeon_minutes: form.surgeon_minutes,
                        smithing_minutes: form.smithing_minutes,
                        labor_minutes: form.labor_minutes,
                        prayer_minutes: form.prayer_minutes,
                        thievery_minutes: form.thievery_minutes,
                        raiding_minutes: form.raiding_minutes,
                    }),
                    json!(ScheduleAllocation::default()),
                ],
            )
            .await;
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party/{character_id}"))
}

async fn party_member(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Html<String> {
    let location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".to_string()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".to_string());
        }
    };

    let Some((active_character, active_inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if !character_is_at_location(&active_character, &location) {
        return Html("<h1>Your party is not at this location</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;

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
    if selected.current_settlement_id != active_character.current_settlement_id
        || selected.current_quest_location_id != active_character.current_quest_location_id
    {
        return Html("<h1>Character is not at this location</h1>".to_string());
    }
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
    let selected_targets = personal_inventory_targets(&state, selected.id).await;
    let active_targets = personal_inventory_targets(&state, active_character.id).await;

    if character_id == active_character.id {
        return Html(
            party_discard_page(
                &location,
                &active_character,
                &active_inventory,
                &items,
                &party_members,
                active_equip.first(),
                session.theme(),
            )
            .into_string(),
        );
    }

    Html(
        party_inventory_page(
            &location,
            &selected,
            &selected_inventory,
            &active_character,
            &active_inventory,
            &items,
            &party_members,
            selected_equip.first(),
            active_equip.first(),
            &selected_targets,
            &active_targets,
            session.theme(),
        )
        .into_string(),
    )
}

async fn party_pool_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
) -> Html<String> {
    let location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".into()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".into());
        }
    };
    let Some((character, inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    if !character_is_at_location(&character, &location) {
        return Html("<h1>Your party is not at this location</h1>".into());
    }
    let Some(party_id) = character.party_id.as_ref() else {
        return Html("<h1>Character has no party</h1>".into());
    };
    let pooled: Vec<PartyInventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let stakes: Vec<PartyStake> = state
        .db
        .query(&format!(
            "SELECT * FROM party_stake WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
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
    let members = get_active_party_members(&state, Some(&character)).await;
    let stake = stakes
        .iter()
        .find(|stake| stake.character_id == character.id)
        .map_or(0, |stake| stake.value);
    let (personal_targets, party_targets, _) = inventory_trade_context(&state, &character).await;
    Html(
        party_pool_page(
            &location,
            &character,
            &inventory,
            &pooled,
            stake,
            &items,
            &members,
            equip.first(),
            &personal_targets,
            &party_targets,
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct InventoryTargetForm {
    item_id: String,
    quantity: u32,
    #[serde(default)]
    party_scope: bool,
}

async fn set_inventory_target(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<InventoryTargetForm>,
) -> impl IntoResponse {
    let Some(character_id) = session.character_id_u64() else {
        return (axum::http::StatusCode::UNAUTHORIZED, "Choose a character");
    };
    let args = vec![
        json!(character_id),
        json!(form.party_scope),
        json!(form.item_id),
        json!(form.quantity),
    ];
    let result = if form.party_scope {
        super::execute_or_request_party_action(
            &state,
            character_id,
            super::PartyAction::SetInventoryQuantityTarget {
                item_id: form.item_id,
                quantity: form.quantity,
            },
        )
        .await
        .map(|_| ())
    } else {
        state
            .db
            .call("set_inventory_quantity_target", &args)
            .await
            .map_err(|error| error.to_string())
    };
    match result {
        Ok(()) => (axum::http::StatusCode::NO_CONTENT, ""),
        Err(error) => {
            tracing::warn!("Failed to save inventory target: {error}");
            (axum::http::StatusCode::BAD_REQUEST, "Could not save target")
        }
    }
}

#[derive(Deserialize)]
struct EquipmentForm {
    inventory_item_id: u64,
    equipped: bool,
}

async fn set_equipment(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<EquipmentForm>,
) -> impl IntoResponse {
    let Some(character_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character");
    };
    let inventory: Option<InventoryItem> = match state
        .db
        .query_one(&format!(
            "SELECT * FROM inventory_item WHERE id = {} AND character_id = {character_id}",
            form.inventory_item_id
        ))
        .await
    {
        Ok(inventory) => inventory,
        Err(error) => {
            tracing::warn!(%error, character_id, "failed to load equipment inventory row");
            return (StatusCode::SERVICE_UNAVAILABLE, "Inventory is unavailable");
        }
    };
    let Some(inventory) = inventory else {
        return (StatusCode::NOT_FOUND, "Item is not in this inventory");
    };
    let definition: Option<ItemDefinition> = match state
        .db
        .query_one(&format!(
            "SELECT * FROM item WHERE id = {}",
            sql_string_literal(&inventory.item_id)
        ))
        .await
    {
        Ok(definition) => definition,
        Err(error) => {
            tracing::warn!(%error, character_id, "failed to load equipment definition");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Equipment catalog is unavailable",
            );
        }
    };
    let Some(definition) = definition else {
        return (StatusCode::NOT_FOUND, "Item definition is missing");
    };
    if definition.kind == crate::spacetimedb::ItemKind::Medication {
        let reducer = if form.equipped {
            "equip_medication"
        } else {
            "unequip_medication"
        };
        if let Err(error) = state
            .db
            .call(
                reducer,
                &[json!(character_id), json!(form.inventory_item_id)],
            )
            .await
        {
            tracing::warn!(%error, character_id, "failed to update medication");
            return (StatusCode::BAD_REQUEST, "Could not update medication");
        }
        return (StatusCode::NO_CONTENT, "");
    }
    let destination = if form.equipped {
        definition.slot
    } else {
        ItemSlot::None
    };
    if form.equipped && destination == ItemSlot::None {
        return (StatusCode::BAD_REQUEST, "This item cannot be equipped");
    }
    if let Err(error) = state
        .db
        .call(
            "equip_item",
            &[
                json!(character_id),
                json!(form.inventory_item_id),
                destination.sats_json(),
            ],
        )
        .await
    {
        tracing::warn!(%error, character_id, "failed to update equipment");
        return (StatusCode::BAD_REQUEST, "Could not update equipment");
    }
    for reducer in ["refresh_capabilities", "refresh_strategic_condition"] {
        if let Err(error) = state.db.call(reducer, &[json!(character_id)]).await {
            tracing::warn!(%error, character_id, reducer, "failed to refresh equipment projection");
        }
    }
    (StatusCode::NO_CONTENT, "")
}

async fn deposit_party_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64() {
        for (item_id, quantity) in transfer_entries(&form) {
            let _ = state
                .db
                .call(
                    "deposit_party_inventory_item",
                    &[json!(character_id), json!(item_id), json!(quantity)],
                )
                .await;
        }
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party-inventory"))
}

async fn withdraw_party_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64() {
        for (item_id, quantity) in transfer_entries(&form) {
            let _ = state
                .db
                .call(
                    "withdraw_party_inventory_item",
                    &[json!(character_id), json!(item_id), json!(quantity)],
                )
                .await;
        }
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party-inventory"))
}

fn transfer_entries(form: &PartyPoolTransferForm) -> Vec<(u64, u32)> {
    match form.entries() {
        Ok(entries) => entries
            .into_iter()
            .map(|entry| (entry.id, entry.quantity))
            .collect(),
        Err(error) => {
            tracing::warn!(error, "invalid party inventory transfer form");
            Vec::new()
        }
    }
}

async fn liquidate_party_assets(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if kind == "settlement"
        && let Some(character_id) = session.character_id_u64()
    {
        let entries = transfer_entries(&form);
        let _ = state
            .db
            .call(
                "liquidate_party_inventory",
                &[
                    json!(character_id),
                    json!(id.clone()),
                    json!(entries.iter().map(|entry| entry.0).collect::<Vec<_>>()),
                    json!(entries.iter().map(|entry| entry.1).collect::<Vec<_>>()),
                ],
            )
            .await;
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party-inventory"))
}

async fn remove_party_member(
    State(state): State<AppState>,
    Path((kind, id, member_character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Redirect {
    if let Some(actor_character_id) = session.character_id_u64() {
        let result = if actor_character_id == member_character_id {
            state
                .db
                .call("leave_party", &[json!(actor_character_id)])
                .await
                .map_err(|error| error.to_string())
        } else {
            super::execute_or_request_party_action(
                &state,
                actor_character_id,
                super::PartyAction::RemovePartyMember {
                    character_id: member_character_id,
                },
            )
            .await
            .map(|_| ())
        };
        if let Err(error) = result {
            tracing::error!("Failed to remove party member: {error:?}");
        }
    }
    Redirect::to(&format!("/locations/{kind}/{id}"))
}

async fn party_stats(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Html<String> {
    let location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".to_string()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".to_string());
        }
    };
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if !character_is_at_location(&active_character, &location) {
        return Html("<h1>Your party is not at this location</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
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
    if selected.current_settlement_id != active_character.current_settlement_id
        || selected.current_quest_location_id != active_character.current_quest_location_id
    {
        return Html("<h1>Character is not at this location</h1>".to_string());
    }
    let active_party = match active_character.party_id.as_deref() {
        Some(party_id) => state
            .db
            .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next(),
        None => None,
    };
    let selected_party = match selected.party_id.as_deref() {
        Some(party_id) => state
            .db
            .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next(),
        None => None,
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
    let capability = get_character_capability(&state, character_id).await;
    let can_examine = get_character_capability(&state, active_character.id)
        .await
        .is_some_and(|capability| {
            capability.medicine >= adventuresim_core::disease::MEDICINE_VITALS_THRESHOLD
        });
    let condition = get_strategic_condition(&state, character_id).await;
    let morale_sources = get_morale_sources(&state, character_id).await;
    let religion = query_single::<CharacterCondition>(&state, "character_condition", character_id)
        .await
        .and_then(|condition| condition.religion_id);
    let notoriety = query_single::<CharacterNotoriety>(&state, "character_notoriety", character_id)
        .await
        .map_or(0.0, |notoriety| notoriety.value);
    let personality =
        query_single::<CharacterPersonality>(&state, "character_personality", character_id).await;
    let medical = medical_presentation(&state, active_character.id, character_id).await;
    Html(
        party_stats_page(
            &location,
            &selected,
            &active_character,
            &party_members,
            capability.as_ref(),
            selected_attributes.first(),
            selected_skills.first(),
            selected_limbs.first(),
            condition.as_ref(),
            &morale_sources,
            religion.as_deref(),
            active_party.as_ref(),
            selected_party.as_ref(),
            notoriety,
            personality.as_ref(),
            &medical,
            can_examine,
            session.theme(),
        )
        .into_string(),
    )
}

pub(crate) async fn medical_presentation(
    state: &AppState,
    viewer_id: u64,
    target_id: u64,
) -> crate::medical::MedicalPresentation {
    let viewer = get_character_capability(state, viewer_id).await;
    let rows = match state
        .db
        .query::<InfectionEpisodeRow>(&format!(
            "SELECT * FROM backend_infection_episodes WHERE character_id = {target_id}"
        ))
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error,target_id,"private medical query failed closed");
            return crate::medical::MedicalPresentation {
                unavailable: true,
                ..Default::default()
            };
        }
    };
    let time = query_single::<CharacterTime>(state, "character_time", target_id)
        .await
        .map_or(0, |t| t.minutes);
    let attributes =
        query_single::<CharacterAttributes>(state, "character_attributes", target_id).await;
    let medications = state
        .db
        .query::<EquippedMedication>(&format!(
            "SELECT * FROM equipped_medication WHERE character_id = {target_id}"
        ))
        .await
        .unwrap_or_default();
    let examination = match state
        .db
        .query::<MedicalExaminationRow>(&format!(
            "SELECT * FROM backend_medical_examinations WHERE doctor_id = {viewer_id}"
        ))
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| row.target_id == target_id)
            .max_by_key(|row| row.id),
        Err(error) => {
            tracing::error!(%error,viewer_id,target_id,"private examination query failed closed");
            return crate::medical::MedicalPresentation {
                unavailable: true,
                ..Default::default()
            };
        }
    };
    let cuts = match state
        .db
        .query::<CommittedCutRow>(&format!(
            "SELECT * FROM backend_committed_cuts WHERE character_id = {target_id}"
        ))
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error,target_id,"private damage query failed closed");
            return crate::medical::MedicalPresentation {
                unavailable: true,
                ..Default::default()
            };
        }
    };
    let mut presentation = crate::medical::sanitize(
        &rows,
        &medications,
        examination.as_ref(),
        time,
        attributes.map_or(3.0, |a| a.immunity),
        viewer.map_or(0.0, |capability| capability.medicine),
    );
    presentation.obvious_cut = cuts
        .iter()
        .map(|cut| cut.severity)
        .sum::<f32>()
        .clamp(0.0, 1.0);
    presentation
}

async fn examine_patient(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    session: Session,
) -> Redirect {
    if let Some(doctor_id) = session.character_id_u64()
        && let Err(error) = state
            .db
            .call("examine_patient", &[json!(doctor_id), json!(target_id)])
            .await
    {
        tracing::warn!(%error, doctor_id, target_id, "patient examination rejected");
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party/{target_id}/stats"))
}

async fn dismiss_medical_examination(
    State(state): State<AppState>,
    Path((kind, id, target_id, examination_id)): Path<(String, String, u64, u64)>,
    session: Session,
) -> Redirect {
    if let Some(doctor_id) = session.character_id_u64()
        && let Err(error) = state
            .db
            .call(
                "dismiss_medical_examination",
                &[json!(doctor_id), json!(target_id), json!(examination_id)],
            )
            .await
    {
        tracing::warn!(%error, doctor_id, target_id, examination_id, "examination dismissal rejected");
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party/{target_id}/stats"))
}

async fn get_strategic_condition(
    state: &AppState,
    character_id: u64,
) -> Option<CharacterStrategicCondition> {
    if let Err(error) = state
        .db
        .call("refresh_strategic_condition", &[json!(character_id)])
        .await
    {
        tracing::warn!(%error, character_id, "failed to refresh strategic condition");
        return None;
    }
    query_single(state, "character_strategic_condition", character_id).await
}

async fn get_morale_sources(state: &AppState, character_id: u64) -> Vec<CharacterMoraleSource> {
    let mut sources: Vec<CharacterMoraleSource> = state
        .db
        .query(&format!(
            "SELECT * FROM character_morale_source WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    sources.sort_by(|left, right| right.magnitude.abs().total_cmp(&left.magnitude.abs()));
    sources
}

#[derive(Deserialize)]
struct PartyTransferForm {
    from_character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
}

async fn discard_inventory_items(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<DiscardInventoryForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id) {
        if let Ok(entries) = form.entries() {
            let (item_ids, quantities): (Vec<_>, Vec<_>) = entries
                .into_iter()
                .map(|entry| (entry.id, entry.quantity))
                .unzip();
            if let Err(error) = state
                .db
                .call(
                    "discard_inventory_items",
                    &[json!(character_id), json!(item_ids), json!(quantities)],
                )
                .await
            {
                tracing::warn!("Inventory discard failed: {error}");
            }
        }
    }
    Redirect::to(&format!(
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    ))
}

async fn finalize_party_offer(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<PartyOfferForm>,
) -> Redirect {
    if let Some((active, _)) = get_active_character(&state, session.character_id_u64()).await {
        if let Ok(entries) = form.entries() {
            let from_ids = entries.iter().map(|entry| entry.from).collect::<Vec<_>>();
            let to_ids = entries.iter().map(|entry| entry.to).collect::<Vec<_>>();
            let item_ids = entries
                .iter()
                .map(|entry| entry.inventory_id)
                .collect::<Vec<_>>();
            let quantities = entries
                .iter()
                .map(|entry| entry.quantity)
                .collect::<Vec<_>>();
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
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    ))
}

async fn transfer_party_item(
    State(state): State<AppState>,
    Path((kind, id, recipient_id)): Path<(String, String, u64)>,
    session: Session,
    Form(form): Form<PartyTransferForm>,
) -> Redirect {
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    if form.from_character_id != active_character.id && recipient_id != active_character.id {
        return Redirect::to(&format!("/locations/{kind}/{id}"));
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
        "/locations/{kind}/{id}/party/{comparison_character_id}/inventory"
    ))
}

async fn merchants(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::General).await
}

async fn finalize_merchant_offer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<MerchantOfferForm>,
) -> Redirect {
    if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await {
        if let (Ok(buys), Ok(sells)) = (form.buys(), form.sells()) {
            let (items, quantities): (Vec<_>, Vec<_>) = buys
                .into_iter()
                .map(|entry| (entry.id, entry.quantity))
                .unzip();
            let (sell_ids, sell_quantities): (Vec<_>, Vec<_>) = sells
                .into_iter()
                .map(|entry| (entry.id, entry.quantity))
                .unzip();
            if !items.is_empty() || !sell_ids.is_empty() {
                let _ = state
                    .db
                    .call(
                        "finalize_merchant_trade",
                        &[
                            json!(character.id),
                            json!(id),
                            json!(items),
                            json!(quantities),
                            json!(sell_ids),
                            json!(sell_quantities),
                            json!(form.inventory_scope == "party"),
                        ],
                    )
                    .await;
            }
        }
    }
    let return_to = match form.return_to.as_str() {
        "weapons" | "armor" | "clothing" | "merchants" | "herbalist" => form.return_to,
        _ => "merchants".to_owned(),
    };
    Redirect::to(&format!("/settlements/{id}/{return_to}"))
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
    let limbs = match active_character.as_ref() {
        Some((character, _)) => {
            query_single::<CharacterLimbs>(&state, "character_limbs", character.id).await
        }
        None => None,
    };
    let stats = match active_character.as_ref() {
        Some((character, _)) => {
            query_single::<CharacterStats>(&state, "character_stats", character.id).await
        }
        None => None,
    };
    let condition = match active_character.as_ref() {
        Some((character, _)) => {
            query_single::<CharacterCondition>(&state, "character_condition", character.id).await
        }
        None => None,
    };
    let (field_repair_minutes, smith_wait_minutes) = match active_character.as_ref() {
        Some((character, inventory)) => {
            equipment_rest_recommendation(&state, character.id, &id, inventory).await
        }
        None => (0, 0),
    };
    let items = state
        .db
        .query::<ItemDefinition>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    Html(
        inn_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &items,
            &party_members,
            limbs.as_ref(),
            stats.as_ref(),
            condition.as_ref(),
            field_repair_minutes,
            smith_wait_minutes,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct RestForm {
    duration: u64,
    unit: String,
}

async fn rest(
    State(state): State<AppState>,
    Path((id, kind)): Path<(String, String)>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    let at_inn = match kind.as_str() {
        "inn" => true,
        "temple" => false,
        _ => return Html("<h1>Rest service not found</h1>".to_string()).into_response(),
    };
    let Some(character_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".to_string()).into_response();
    };
    let before_character = get_active_character(&state, Some(character_id)).await;
    let before_limbs =
        query_single::<CharacterLimbs>(&state, "character_limbs", character_id).await;
    let before_skills =
        query_single::<CharacterSkills>(&state, "character_skills", character_id).await;
    let before_time =
        query_single::<crate::spacetimedb::CharacterTime>(&state, "character_time", character_id)
            .await;
    let before_notoriety =
        query_single::<CharacterNotoriety>(&state, "character_notoriety", character_id).await;
    if let Err(error) = state
        .db
        .call(
            "rest_at_settlement_hours",
            &[
                json!(character_id),
                json!(rest_duration_minutes(form.duration, &form.unit)),
                json!(at_inn),
            ],
        )
        .await
    {
        return Html(format!("<h1>Unable to rest</h1><p>{error}</p>")).into_response();
    }

    let settlements: Vec<Settlement> = state
        .db
        .query(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string()).into_response();
    };
    let active_character = get_active_character(&state, Some(character_id)).await;
    if let Some(quest_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.current_quest_location_id.as_deref())
    {
        return Redirect::to(&format!("/locations/quest/{quest_id}")).into_response();
    }
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let after_limbs = query_single::<CharacterLimbs>(&state, "character_limbs", character_id).await;
    let after_skills =
        query_single::<CharacterSkills>(&state, "character_skills", character_id).await;
    let after_time =
        query_single::<crate::spacetimedb::CharacterTime>(&state, "character_time", character_id)
            .await;
    let after_notoriety =
        query_single::<CharacterNotoriety>(&state, "character_notoriety", character_id).await;
    let summary = rest_summary(
        before_character.as_ref().map(|(character, _)| character),
        active_character.as_ref().map(|(character, _)| character),
        before_limbs.as_ref(),
        after_limbs.as_ref(),
        before_skills.as_ref(),
        after_skills.as_ref(),
        before_time.as_ref(),
        after_time.as_ref(),
        before_notoriety.as_ref(),
        after_notoriety.as_ref(),
    );
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let items = state
        .db
        .query::<ItemDefinition>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    Html(
        rest_result_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &items,
            &party_members,
            logged_in_as.as_deref(),
            session.theme(),
            at_inn,
            &summary,
        )
        .into_string(),
    )
    .into_response()
}

async fn query_single<T: serde::de::DeserializeOwned>(
    state: &AppState,
    table: &str,
    character_id: u64,
) -> Option<T> {
    state
        .db
        .query(&format!(
            "SELECT * FROM {table} WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
}

fn rest_summary(
    before_character: Option<&Character>,
    after_character: Option<&Character>,
    before_limbs: Option<&CharacterLimbs>,
    after_limbs: Option<&CharacterLimbs>,
    before_skills: Option<&CharacterSkills>,
    after_skills: Option<&CharacterSkills>,
    before_time: Option<&crate::spacetimedb::CharacterTime>,
    after_time: Option<&crate::spacetimedb::CharacterTime>,
    before_notoriety: Option<&CharacterNotoriety>,
    after_notoriety: Option<&CharacterNotoriety>,
) -> RestSummary {
    let minutes = before_time.zip(after_time).map_or(0, |(before, after)| {
        after.minutes.saturating_sub(before.minutes)
    });
    let gold_spent = before_character
        .zip(after_character)
        .map_or(0, |(before, after)| before.gold.saturating_sub(after.gold));
    let gold_earned = before_character
        .zip(after_character)
        .map_or(0, |(before, after)| after.gold.saturating_sub(before.gold));
    let notoriety_gained = after_notoriety.map_or(0.0, |after| {
        after.value - before_notoriety.map_or(0.0, |before| before.value)
    });
    let healed = match (before_limbs, after_limbs) {
        (Some(before), Some(after)) => limb_deltas(before, after),
        _ => vec![],
    };
    let trained = match (before_skills, after_skills) {
        (Some(before), Some(after)) => skill_deltas(before, after),
        _ => vec![],
    };
    RestSummary {
        minutes,
        gold_spent,
        gold_earned,
        notoriety_gained,
        healed,
        trained,
    }
}

fn limb_deltas(before: &CharacterLimbs, after: &CharacterLimbs) -> Vec<(String, f32)> {
    [
        ("Left arm", before.left_arm_health, after.left_arm_health),
        ("Right arm", before.right_arm_health, after.right_arm_health),
        ("Left leg", before.left_leg_health, after.left_leg_health),
        ("Right leg", before.right_leg_health, after.right_leg_health),
        ("Head", before.head_health, after.head_health),
        ("Chest", before.chest_health, after.chest_health),
        ("Stomach", before.stomach_health, after.stomach_health),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| {
        let delta = (after - before) * 100.0;
        (delta > 0.01).then(|| (name.to_string(), delta))
    })
    .collect()
}

fn skill_deltas(before: &CharacterSkills, after: &CharacterSkills) -> Vec<(String, f32)> {
    [
        ("Melee", before.melee_hours, after.melee_hours),
        ("Dodge", before.dodge_hours, after.dodge_hours),
        ("Block", before.block_hours, after.block_hours),
        ("Ranged", before.ranged_hours, after.ranged_hours),
        ("Will", before.will_hours, after.will_hours),
        ("Charisma", before.charisma_hours, after.charisma_hours),
        ("Medicine", before.medicine_hours, after.medicine_hours),
        ("Faith", before.faith_hours, after.faith_hours),
        ("Stealth", before.stealth_hours, after.stealth_hours),
        ("Balance", before.balance_hours, after.balance_hours),
        ("Surgeon", before.surgeon_hours, after.surgeon_hours),
        ("Smithing", before.smithing_hours, after.smithing_hours),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| {
        let delta = after - before;
        (delta > 0.001).then(|| (name.to_string(), delta))
    })
    .collect()
}

async fn travel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<TravelForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };

    let outcome = super::execute_or_request_party_action(
        &state,
        character_id,
        super::PartyAction::TravelToSettlement {
            settlement_id: id.clone(),
            provisioning: form.provisioning,
        },
    )
    .await;
    match outcome {
        // The live navigation stream routes every party member after the
        // reducer's committed state is visible.
        Ok(super::PartyActionOutcome::Executed) => StatusCode::NO_CONTENT.into_response(),
        Ok(super::PartyActionOutcome::Requested) => StatusCode::ACCEPTED.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    }
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

async fn unequip_medication(
    State(state): State<AppState>,
    Path((kind, id, target_id, equipment_id)): Path<(String, String, u64, u64)>,
    session: Session,
) -> Redirect {
    if session.character_id_u64() == Some(target_id)
        && let Err(error) = state
            .db
            .call(
                "unequip_medication",
                &[json!(target_id), json!(equipment_id)],
            )
            .await
    {
        tracing::warn!(%error, target_id, equipment_id, "medication removal rejected");
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party/{target_id}"))
}

async fn herbalist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Herbalist).await
}

async fn purchase_from_herbalist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<MerchantOfferForm>,
) -> Redirect {
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    // Prepared courses are individual equipment-like items, so this storefront
    // intentionally has no party-scope purchase path.
    if form.inventory_scope == "player"
        && let Ok(buys) = form.buys()
        && !buys.is_empty()
    {
        let (items, quantities): (Vec<_>, Vec<_>) = buys
            .into_iter()
            .map(|entry| (entry.id, entry.quantity))
            .unzip();
        if let Err(error) = state
            .db
            .call(
                "purchase_from_herbalist",
                &[
                    json!(character.id),
                    json!(id),
                    json!(items),
                    json!(quantities),
                ],
            )
            .await
        {
            tracing::warn!(%error, character_id = character.id, "herbalist purchase rejected");
        }
    }
    Redirect::to(&format!("/settlements/{id}/herbalist"))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HerbalistDiagnosisDto {
    disease_name: String,
    medication_name: String,
}

#[derive(Serialize)]
struct HerbalistExaminationResponse {
    diagnoses: Vec<HerbalistDiagnosisDto>,
    message: &'static str,
}

fn herbalist_diagnosis_dtos(row: &HerbalistExaminationRow) -> Vec<HerbalistDiagnosisDto> {
    row.disease_names
        .iter()
        .zip(&row.medication_names)
        .map(|(disease_name, medication_name)| HerbalistDiagnosisDto {
            disease_name: disease_name.clone(),
            medication_name: medication_name.clone(),
        })
        .collect()
}

async fn herbalist_examination(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<HerbalistExaminationResponse> {
    const UNABLE: &str = "I am sorry, but I cannot name your illness with confidence. Seek a more skilled physician.";
    let Some((patient, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Json(HerbalistExaminationResponse {
            diagnoses: Vec::new(),
            message: "Choose a character before asking for an examination.",
        });
    };
    if let Err(error) = state
        .db
        .call("examine_by_herbalist", &[json!(patient.id), json!(id)])
        .await
    {
        tracing::warn!(%error, patient_id = patient.id, "herbalist examination rejected");
        return Json(HerbalistExaminationResponse {
            diagnoses: Vec::new(),
            message: "I cannot examine you until my fee can be paid and you stand before me.",
        });
    }

    let result = state
        .db
        .query::<HerbalistExaminationRow>(&format!(
            "SELECT * FROM backend_herbalist_examinations WHERE patient_id = {}",
            patient.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.settlement_id == id)
        .max_by_key(|row| row.id);
    let Some(result) = result else {
        return Json(HerbalistExaminationResponse {
            diagnoses: Vec::new(),
            message: UNABLE,
        });
    };
    let diagnoses = herbalist_diagnosis_dtos(&result);
    if let Err(error) = state
        .db
        .call(
            "dismiss_herbalist_examination",
            &[json!(patient.id), json!(result.id)],
        )
        .await
    {
        tracing::warn!(%error, patient_id = patient.id, "name-only herbalist result was not dismissed");
    }
    Json(HerbalistExaminationResponse {
        message: if diagnoses.is_empty() { UNABLE } else { "" },
        diagnoses,
    })
}

async fn religion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    render_service_page(state, id, session, religion_page).await
}

#[derive(Deserialize)]
struct ReligionForm {
    religion_id: String,
}

#[derive(Serialize)]
struct ReligionDialogue {
    religion_id: Option<String>,
    priest_religion_id: String,
    can_choose: bool,
}

async fn religion_dialogue(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<ReligionDialogue> {
    let settlement = state
        .db
        .query::<Settlement>(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let priest_religion_id = settlement
        .as_ref()
        .map(|settlement| settlement.religion_id.clone())
        .unwrap_or_default();
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Json(ReligionDialogue {
            religion_id: None,
            priest_religion_id,
            can_choose: false,
        });
    };
    let can_choose =
        settlement.is_some() && character.current_settlement_id.as_deref() == Some(id.as_str());
    let condition = state
        .db
        .query::<CharacterCondition>(&format!(
            "SELECT * FROM character_condition WHERE character_id = {}",
            character.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    Json(ReligionDialogue {
        religion_id: condition.and_then(|condition| condition.religion_id),
        priest_religion_id,
        can_choose,
    })
}

#[derive(Serialize)]
struct ReligionChange {
    changed: bool,
    religion_id: Option<String>,
    message: &'static str,
}

async fn set_religion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<ReligionForm>,
) -> Json<ReligionChange> {
    let religion_id = form.religion_id.trim();
    let settlement = state
        .db
        .query::<Settlement>(&format!("SELECT * FROM settlement WHERE id = '{}'", id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(settlement) = settlement else {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "There is no church here to receive your profession.",
        });
    };
    if religion_id != settlement.religion_id {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "This priest can receive you only into his own faith.",
        });
    }
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "Choose a character before speaking with the priest.",
        });
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "You must be at this church to make a profession of faith.",
        });
    }
    match state
        .db
        .call(
            "set_character_religion",
            &[json!(character.id), json!(religion_id)],
        )
        .await
    {
        Ok(()) => Json(ReligionChange {
            changed: true,
            religion_id: (!religion_id.is_empty()).then(|| religion_id.to_string()),
            message: "Your profession has been recorded.",
        }),
        Err(error) => {
            tracing::warn!(%error, character_id = character.id, "failed to set character religion");
            Json(ReligionChange {
                changed: false,
                religion_id: None,
                message: "The priest cannot receive your profession just now.",
            })
        }
    }
}

async fn renounce_religion(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    session: Session,
) -> Redirect {
    if session.character_id_u64() == Some(character_id) {
        if let Err(error) = state
            .db
            .call("set_character_religion", &[json!(character_id), json!("")])
            .await
        {
            tracing::warn!(%error, character_id, "failed to renounce character religion");
        }
    }
    Redirect::to(&format!("/locations/{kind}/{id}/party/{character_id}"))
}

type ServiceRenderer = fn(
    &Settlement,
    Option<&Character>,
    &[InventoryItem],
    &[ItemDefinition],
    &[Character],
    Option<&CharacterLimbs>,
    Option<&CharacterStats>,
    Option<&CharacterCondition>,
    u64,
    u64,
    Option<&str>,
    &str,
) -> maud::Markup;

async fn merchant_shop(
    state: AppState,
    id: String,
    session: Session,
    shop: MerchantShop,
) -> Html<String> {
    let settlement_literal = sql_string_literal(&id);
    let settlement_sql = format!("SELECT * FROM settlement WHERE id = {settlement_literal}");
    let (settlements, active_character) = tokio::join!(
        state.db.query::<Settlement>(&settlement_sql),
        get_active_character(&state, session.character_id_u64()),
    );
    let settlements = settlements.unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let Some((character, inventory)) = active_character.as_ref() else {
        let party_members = get_active_party_members(&state, None).await;
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
    let equip_sql = format!(
        "SELECT * FROM character_equip WHERE character_id = {}",
        character.id
    );
    let condition_sql = format!("SELECT * FROM item_condition");
    let smith_sql =
        format!("SELECT * FROM settlement_smith WHERE settlement_id = {settlement_literal}");
    let order_sql = format!(
        "SELECT * FROM repair_order WHERE owner_character_id = {} AND settlement_id = {settlement_literal}",
        character.id
    );
    let time_sql = format!(
        "SELECT * FROM character_time WHERE character_id = {}",
        character.id
    );
    let (party_members, items, equip, trade_context, conditions, smiths, orders, times) = tokio::join!(
        get_active_party_members(&state, Some(character)),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
        state.db.query::<CharacterEquip>(&equip_sql),
        inventory_trade_context(&state, character),
        state.db.query::<ItemCondition>(&condition_sql),
        state.db.query::<SettlementSmith>(&smith_sql),
        state.db.query::<RepairOrder>(&order_sql),
        state.db.query::<CharacterTime>(&time_sql),
    );
    let items = items.unwrap_or_default();
    let equip = equip.unwrap_or_default();
    let (personal_targets, party_targets, pooled) = trade_context;
    Html(
        live_merchant_shop_page(
            settlement,
            character,
            inventory,
            &items,
            &party_members,
            equip.first(),
            &personal_targets,
            &party_targets,
            &pooled,
            session.theme(),
            shop,
            &conditions.unwrap_or_default(),
            smiths.unwrap_or_default().first(),
            &orders.unwrap_or_default(),
            times
                .unwrap_or_default()
                .first()
                .map_or(0, |time| time.minutes),
        )
        .into_string(),
    )
}

async fn inventory_trade_context(
    state: &AppState,
    character: &Character,
) -> (
    Vec<InventoryQuantityTarget>,
    Vec<InventoryQuantityTarget>,
    Vec<PartyInventoryItem>,
) {
    let personal_sql = format!(
        "SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = false",
        character.id
    );
    let Some(party_id) = character.party_id.as_ref() else {
        let personal = state.db.query(&personal_sql).await.unwrap_or_default();
        return (personal, Vec::new(), Vec::new());
    };
    let party_sql = format!("SELECT * FROM party WHERE id = '{}'", party_id);
    let (personal, party) = tokio::join!(
        state.db.query(&personal_sql),
        state.db.query::<Party>(&party_sql),
    );
    let personal = personal.unwrap_or_default();
    let party = party.unwrap_or_default().into_iter().next();
    let Some(party) = party else {
        return (personal, Vec::new(), Vec::new());
    };
    let party_targets_sql = format!(
        "SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = true",
        party.leader_id
    );
    let pooled_sql = format!(
        "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
        party_id
    );
    let (party_targets, pooled) = tokio::join!(
        state.db.query(&party_targets_sql),
        state.db.query(&pooled_sql),
    );
    (
        personal,
        party_targets.unwrap_or_default(),
        pooled.unwrap_or_default(),
    )
}

async fn personal_inventory_targets(
    state: &AppState,
    character_id: u64,
) -> Vec<InventoryQuantityTarget> {
    state.db.query(&format!("SELECT * FROM inventory_quantity_target WHERE owner_character_id = {character_id} AND party_scope = false")).await.unwrap_or_default()
}

async fn render_service_page(
    state: AppState,
    id: String,
    session: Session,
    render: ServiceRenderer,
) -> Html<String> {
    let settlement_sql = format!("SELECT * FROM settlement WHERE id = '{}'", id);
    let (settlements, active_character) = tokio::join!(
        state.db.query::<Settlement>(&settlement_sql),
        get_active_character(&state, session.character_id_u64()),
    );
    let settlements = settlements.unwrap_or_default();
    let settlement = match settlements.first() {
        Some(settlement) => settlement,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };

    let active_character_ref = active_character.as_ref().map(|(character, _)| character);
    let limbs_lookup = async {
        match active_character_ref {
            Some(character) => {
                query_single::<CharacterLimbs>(&state, "character_limbs", character.id).await
            }
            None => None,
        }
    };
    let stats_lookup = async {
        match active_character_ref {
            Some(character) => {
                query_single::<CharacterStats>(&state, "character_stats", character.id).await
            }
            None => None,
        }
    };
    let condition_lookup = async {
        match active_character_ref {
            Some(character) => {
                query_single::<CharacterCondition>(&state, "character_condition", character.id)
                    .await
            }
            None => None,
        }
    };
    let equipment_lookup = async {
        match active_character.as_ref() {
            Some((character, inventory)) => {
                equipment_rest_recommendation(&state, character.id, &id, inventory).await
            }
            None => (0, 0),
        }
    };
    let (party_members, items, limbs, stats, condition, equipment_recovery) = tokio::join!(
        get_active_party_members(&state, active_character_ref),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
        limbs_lookup,
        stats_lookup,
        condition_lookup,
        equipment_lookup,
    );
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
            &items.unwrap_or_default(),
            &party_members,
            limbs.as_ref(),
            stats.as_ref(),
            condition.as_ref(),
            equipment_recovery.0,
            equipment_recovery.1,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
}

async fn equipment_rest_recommendation(
    state: &AppState,
    character_id: u64,
    settlement_id: &str,
    inventory: &[InventoryItem],
) -> (u64, u64) {
    let skills_sql = format!("SELECT * FROM character_skills WHERE character_id = {character_id}");
    let settlement_literal = sql_string_literal(settlement_id);
    let orders_sql = format!(
        "SELECT * FROM repair_order WHERE owner_character_id = {character_id} AND settlement_id = {settlement_literal}"
    );
    let time_sql = format!("SELECT * FROM character_time WHERE character_id = {character_id}");
    let (conditions, skills, orders, times) = tokio::join!(
        state
            .db
            .query::<ItemCondition>("SELECT * FROM item_condition"),
        state.db.query::<CharacterSkills>(&skills_sql),
        state.db.query::<RepairOrder>(&orders_sql),
        state.db.query::<CharacterTime>(&time_sql),
    );
    let skill = skills
        .unwrap_or_default()
        .first()
        .map(|skills| Skill::Smithing.training_rank(skills.smithing_hours).floor() as u8)
        .unwrap_or(0)
        .min(2);
    let owned: std::collections::HashSet<u64> = inventory.iter().map(|item| item.id).collect();
    let yellow: f32 = conditions
        .unwrap_or_default()
        .iter()
        .filter(|condition| owned.contains(&condition.inventory_item_id))
        .map(|condition| condition.bins().iter().take(skill as usize).sum::<f32>())
        .sum();
    let field_minutes = (yellow * 2_880.0).ceil() as u64;
    let now = times
        .unwrap_or_default()
        .first()
        .map_or(0, |time| time.minutes);
    let smith_wait = orders
        .unwrap_or_default()
        .iter()
        .map(|order| order.ready_at_minutes.saturating_sub(now))
        .max()
        .unwrap_or(0);
    (field_minutes, smith_wait)
}

async fn get_active_character(
    state: &AppState,
    character_id: Option<u64>,
) -> Option<(Character, Vec<InventoryItem>)> {
    let character_id = character_id?;
    let character_sql = format!("SELECT * FROM character WHERE id = {character_id}");
    let inventory_sql = format!("SELECT * FROM inventory_item WHERE character_id = {character_id}");
    let (characters, inventory) = tokio::join!(
        state.db.query::<Character>(&character_sql),
        state.db.query::<InventoryItem>(&inventory_sql),
    );
    let characters = characters.unwrap_or_default();
    let character = characters.into_iter().next()?;
    let inventory = inventory.unwrap_or_default();
    Some((character, inventory))
}

async fn get_character_capability(
    state: &AppState,
    character_id: u64,
) -> Option<CharacterCapability> {
    let _ = state
        .db
        .call("refresh_capabilities", &[json!(character_id)])
        .await;
    state
        .db
        .query(&format!(
            "SELECT * FROM character_capability WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
}

pub(crate) async fn get_active_party_members(
    state: &AppState,
    active_character: Option<&Character>,
) -> Vec<Character> {
    let Some(party_id) = active_character.and_then(|character| character.party_id.as_ref()) else {
        return Vec::new();
    };
    let memberships_sql = format!("SELECT * FROM party_member WHERE party_id = '{}'", party_id);
    let party_sql = format!("SELECT * FROM party WHERE id = '{}'", party_id);
    let (memberships, party) = tokio::join!(
        state.db.query::<PartyMember>(&memberships_sql),
        state.db.query::<Party>(&party_sql),
    );
    let memberships = memberships.unwrap_or_default();
    let leader_id = party
        .unwrap_or_default()
        .first()
        .map(|party| party.leader_id);
    let lookups = memberships.into_iter().map(|membership| async move {
        state
            .db
            .query::<Character>(&format!(
                "SELECT * FROM character WHERE id = {}",
                membership.character_id
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    });
    let mut members: Vec<Character> = join_all(lookups).await.into_iter().flatten().collect();
    members.sort_by_key(|member| (Some(member.id) != leader_id, member.id));
    members
}

#[cfg(test)]
mod herbalist_tests {
    use super::{HerbalistDiagnosisDto, herbalist_diagnosis_dtos};
    use crate::spacetimedb::HerbalistExaminationRow;

    #[test]
    fn npc_examination_dto_contains_only_canonical_name_pairs() {
        let row = HerbalistExaminationRow {
            id: 1,
            patient_id: 7,
            settlement_id: "riverdale".into(),
            disease_names: vec!["Catarrhal fever".into(), "Bloody flux".into()],
            medication_names: vec![
                "Catarrhal fever cordial".into(),
                "Bloody flux electuary".into(),
            ],
        };
        assert_eq!(
            herbalist_diagnosis_dtos(&row),
            vec![
                HerbalistDiagnosisDto {
                    disease_name: "Catarrhal fever".into(),
                    medication_name: "Catarrhal fever cordial".into(),
                },
                HerbalistDiagnosisDto {
                    disease_name: "Bloody flux".into(),
                    medication_name: "Bloody flux electuary".into(),
                },
            ]
        );
    }

    #[test]
    fn malformed_parallel_result_fails_closed_without_inventing_details() {
        let row = HerbalistExaminationRow {
            id: 2,
            patient_id: 7,
            settlement_id: "riverdale".into(),
            disease_names: vec!["Catarrhal fever".into(), "unpaired".into()],
            medication_names: vec!["Catarrhal fever cordial".into()],
        };
        assert_eq!(herbalist_diagnosis_dtos(&row).len(), 1);
    }
}
