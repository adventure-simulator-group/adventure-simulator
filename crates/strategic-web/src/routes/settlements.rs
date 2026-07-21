//! Settlement route handlers

use adventuresim_core::{
    equipment::{EncumbranceSummary, encumbrance_capacity_kg},
    prelude::{
        PartyProvisioningInputs, STANDARD_TRAVEL_RATION_ID, STANDARD_WATERSKIN_ID,
        STRATEGIC_TRAVEL_KCAL_PER_DAY, Skill,
    },
    strategic_schedule::{CombatTrainingProfile, EquippedCombatItem},
    strategic_time::{is_walking_time, minutes_until_next_walking_start},
};
use adventuresim_world_schema::OfficialReligion;
use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use futures_util::{
    future::join_all,
    stream::{self, StreamExt},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const BUILDINGS: &[&str] = &[
    "map",
    "merchants",
    "weapons",
    "armor",
    "clothing",
    "inn",
    "religion",
];

#[derive(Clone, Debug, Default, Deserialize)]
struct BuildingQuery {
    building: Option<String>,
}

impl BuildingQuery {
    fn valid(&self) -> Option<&str> {
        self.building
            .as_deref()
            .filter(|value| BUILDINGS.contains(value))
    }

    fn append_to(&self, path: String) -> String {
        self.valid().map_or_else(
            || path.clone(),
            |building| format!("{path}?building={building}"),
        )
    }
}

#[cfg(test)]
mod building_query_tests {
    use super::BuildingQuery;

    #[test]
    fn building_query_is_closed_and_preserved_on_redirects() {
        let valid = BuildingQuery {
            building: Some("inn".into()),
        };
        assert_eq!(valid.valid(), Some("inn"));
        assert_eq!(
            valid.append_to("/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1?building=inn"
        );
        let invalid = BuildingQuery {
            building: Some("../religion".into()),
        };
        assert_eq!(invalid.valid(), None);
        assert_eq!(
            invalid.append_to("/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1"
        );
    }
}

use super::AppState;
use super::inventory_forms::{
    DiscardInventoryForm, MerchantOfferForm, PartyOfferForm, PartyPoolTransferForm,
};
use super::redirect_to_local;
use super::travel::{
    QuestMapMarkers, TravelDestination, TravelForm, TravelProvisionForecast, active_quest_summary,
    active_quest_tooltip, connected_destinations, next_settlement_toward,
    populate_itinerary_forecasts,
};
use crate::session::Session;
use crate::spacetimedb::sql_string_literal;
use crate::spacetimedb::{
    AlcoholConsumption, Character, CharacterAttributes, CharacterCapability, CharacterCondition,
    CharacterEquip, CharacterFilth, CharacterLimbs, CharacterMoraleSource, CharacterNeeds,
    CharacterNotoriety, CharacterPersonality, CharacterSkills, CharacterStats,
    CharacterStrategicCondition, CharacterTime, CharacterTrainingSchedule, EquippedMedication,
    HerbalistExaminationRow, InfectionEpisodeRow, InventoryItem, InventoryQuantityTarget,
    ItemCondition, ItemDefinition, ItemKind, ItemSlot, LimbInjury, LimbRegion,
    MedicalExaminationRow, Party, PartyInventoryItem, PartyJourney, PartyJourneyItinerary,
    PartyJourneyRoute, PartyMember, PartyRecruitmentRole, PartyStake, Quest, QuestIssuer,
    QuestStatus, RecruitmentRequirements, ReligiousDemand, RepairOrder, RetainedProjectile,
    ScheduleAllocation, Settlement, SettlementAlias, SettlementDescription, SettlementSmith,
    TravelEdge,
};
use crate::templates::settlement::{
    ActivityPreviewRates, CampTravelDestination, LocationKind, LocationView, MerchantShop,
    RestSummary, SoapRestPreview, alchemy_page, camp_page, inn_page, live_merchant_shop_page,
    merchants_page, party_discard_page, party_inventory_page, party_personal_page, party_pool_page,
    party_stats_page, religion_page, rest_result_page, settlement_map_page,
    settlement_overview_page, surgery_page,
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
        .route(
            "/locations/settlement/{id}/map/rest",
            post(rest_at_settlement_map),
        )
        .route(
            "/locations/quest/{id}/map/travel-configuration",
            post(update_travel_configuration),
        )
        .route("/camp", get(camp))
        .route("/camp/rest", post(rest_at_camp))
        .route(
            "/camp/travel-configuration",
            post(update_camp_travel_configuration),
        )
        .route("/camp/continue", post(continue_camp_travel))
        .route("/camp/destination/{id}", post(change_camp_destination))
        .route(
            "/api/settlements/{id}/service-quests",
            get(service_quest_offers),
        )
        .route(
            "/api/settlements/{id}/professions/{service_id}/apprenticeship",
            post(begin_service_apprenticeship),
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
            "/locations/{kind}/{id}/party/{character_id}/surgery/{limb}",
            get(surgery),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/surgery/{limb}/procedure",
            post(perform_surgery),
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

fn parse_surgery_limb(slug: &str) -> Option<LimbRegion> {
    Some(match slug {
        "left-arm" => LimbRegion::LeftArm,
        "right-arm" => LimbRegion::RightArm,
        "left-leg" => LimbRegion::LeftLeg,
        "right-leg" => LimbRegion::RightLeg,
        "chest" => LimbRegion::Chest,
        "stomach" => LimbRegion::Stomach,
        "head" => LimbRegion::Head,
        _ => return None,
    })
}

async fn surgery(
    State(state): State<AppState>,
    Path((kind, id, patient_id, limb)): Path<(String, String, u64, String)>,
    session: Session,
) -> Html<String> {
    let Some(actor_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    let Some(selected_limb) = parse_surgery_limb(&limb) else {
        return Html("<h1>Limb not found</h1>".into());
    };
    let location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".into()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".into());
        }
    };
    let Some((active, _)) = get_active_character(&state, Some(actor_id)).await else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    let party_members = get_active_party_members(&state, Some(&active)).await;
    let Some(patient) = party_members
        .iter()
        .find(|member| member.id == patient_id)
        .cloned()
    else {
        return Html("<h1>Party member not found</h1>".into());
    };
    if !character_is_at_location(&active, &location)
        || !character_is_at_location(&patient, &location)
    {
        return Html("<h1>Surgeon and patient must be together</h1>".into());
    }
    let injuries = state
        .db
        .query::<LimbInjury>(&format!(
            "SELECT * FROM limb_injury WHERE character_id = {patient_id}"
        ))
        .await
        .unwrap_or_default();
    let projectiles = state
        .db
        .query::<RetainedProjectile>(&format!(
            "SELECT * FROM retained_projectile WHERE character_id = {patient_id}"
        ))
        .await
        .unwrap_or_default();
    let inventory = state
        .db
        .query::<InventoryItem>(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {actor_id}"
        ))
        .await
        .unwrap_or_default();
    let item_definitions = state
        .db
        .query::<ItemDefinition>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let alcohol_count = inventory
        .iter()
        .filter(|entry| {
            item_definitions
                .iter()
                .any(|def| def.id == entry.item_id && def.alcohol_disinfectant_effectiveness > 0)
        })
        .map(|entry| entry.qty)
        .sum();
    let disinfectants = inventory
        .iter()
        .filter_map(|entry| {
            item_definitions
                .iter()
                .find(|def| def.id == entry.item_id && def.alcohol_disinfectant_effectiveness > 0)
                .map(|def| {
                    (
                        def.alcohol_disinfectant_effectiveness,
                        entry.id,
                        def.id.as_str(),
                    )
                })
        })
        .collect::<Vec<_>>();
    let selected_alcohol = adventuresim_core::alcohol::best_disinfectant(
        &disinfectants
            .iter()
            .map(|(effectiveness, id, _)| (*effectiveness, *id))
            .collect::<Vec<_>>(),
    )
    .map(|index| disinfectants[index].2);
    let actor_injuries = if actor_id == patient_id {
        injuries.clone()
    } else {
        state
            .db
            .query::<LimbInjury>(&format!(
                "SELECT * FROM limb_injury WHERE character_id = {actor_id}"
            ))
            .await
            .unwrap_or_default()
    };
    let quantity = |item_id: &str| {
        inventory
            .iter()
            .filter(|item| item.item_id == item_id)
            .map(|item| item.qty)
            .sum()
    };
    let skill = get_character_capability(&state, actor_id)
        .await
        .map_or(0.0, |capability| capability.surgery);
    let available_splints = inventory
        .iter()
        .filter(|item| {
            item.item_id == "splint"
                && !actor_injuries
                    .iter()
                    .any(|injury| injury.splint_inventory_item_id == Some(item.id))
        })
        .map(|item| item.qty)
        .sum();
    let patient_capability = get_character_capability(&state, patient_id).await;
    let patient_attributes =
        query_single::<CharacterAttributes>(&state, "character_attributes", patient_id).await;
    let patient_skills =
        query_single::<CharacterSkills>(&state, "character_skills", patient_id).await;
    let patient_limbs = query_single::<CharacterLimbs>(&state, "character_limbs", patient_id).await;
    let medical = medical_presentation(&state, actor_id, patient_id).await;
    let combat_profile = get_combat_training_profile(&state, patient_id).await;
    Html(
        surgery_page(
            &location,
            &active,
            &patient,
            &party_members,
            patient_capability.as_ref(),
            patient_attributes.as_ref(),
            patient_skills.as_ref(),
            patient_limbs.as_ref(),
            &medical,
            combat_profile,
            &injuries,
            &projectiles,
            selected_limb,
            quantity("bandage"),
            quantity("surgery_kit"),
            available_splints,
            quantity("soft_soap"),
            alcohol_count,
            selected_alcohol,
            skill,
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct SurgeryProcedureForm {
    procedure: String,
    projectile_id: Option<u64>,
    #[serde(default)]
    use_soap: bool,
}

/// SpacetimeDB's raw HTTP reducer API expects algebraic `Option<T>` values,
/// not Serde's scalar-or-null representation: `Some(v) = {"some": v}` and
/// `None = {"none": []}`.
fn spacetime_option_u64(value: Option<u64>) -> serde_json::Value {
    match value {
        Some(value) => json!({ "some": value }),
        None => json!({ "none": [] }),
    }
}

fn spacetime_option_string(value: Option<&str>) -> serde_json::Value {
    match value {
        Some(value) => json!({ "some": value }),
        None => json!({ "none": [] }),
    }
}

fn schedule_allocation_reducer_arg(schedule: &ScheduleAllocation) -> serde_json::Value {
    let mut value = json!(schedule);
    value["apprenticeship_service_id"] =
        spacetime_option_string(schedule.apprenticeship_service_id.as_deref());
    value["profession_service_id"] =
        spacetime_option_string(schedule.profession_service_id.as_deref());
    value
}

#[cfg(test)]
mod surgery_reducer_argument_tests {
    use super::{schedule_allocation_reducer_arg, spacetime_option_u64};
    use crate::spacetimedb::ScheduleAllocation;
    use serde_json::json;

    #[test]
    fn projectile_id_uses_spacetime_option_encoding() {
        assert_eq!(spacetime_option_u64(Some(73)), json!({ "some": 73 }));
        assert_eq!(spacetime_option_u64(None), json!({ "none": [] }));
    }

    #[test]
    fn schedule_profession_ids_use_spacetime_option_encoding() {
        let encoded = schedule_allocation_reducer_arg(&ScheduleAllocation {
            apprenticeship_service_id: Some("armor".into()),
            profession_service_id: None,
            ..Default::default()
        });
        assert_eq!(
            encoded["apprenticeship_service_id"],
            json!({ "some": "armor" })
        );
        assert_eq!(encoded["profession_service_id"], json!({ "none": [] }));
    }
}

async fn perform_surgery(
    State(state): State<AppState>,
    Path((kind, id, patient_id, limb)): Path<(String, String, u64, String)>,
    session: Session,
    Form(form): Form<SurgeryProcedureForm>,
) -> Redirect {
    let destination = format!("/locations/{kind}/{id}/party/{patient_id}/surgery/{limb}");
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to(&destination);
    };
    if parse_surgery_limb(&limb).is_none() {
        return Redirect::to(&destination);
    }
    if let Err(error) = state
        .db
        .call(
            "treat_limb",
            &[
                json!(actor_id),
                json!(patient_id),
                json!(limb),
                json!(form.procedure),
                spacetime_option_u64(form.projectile_id),
                json!(form.use_soap),
            ],
        )
        .await
    {
        tracing::warn!(?error, "Manual surgery procedure failed");
    }
    Redirect::to(&destination)
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
) -> Response {
    let Some((character, inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return (
            StatusCode::UNAUTHORIZED,
            Html(
                crate::templates::strategic_notice_page(
                    "Choose an adventurer",
                    "Select an adventurer before opening the alchemy workbench.",
                    "/characters",
                    "Choose an adventurer",
                    None,
                )
                .into_string(),
            ),
        )
            .into_response();
    };
    if character.current_settlement_id.as_deref() != Some(&id) {
        let return_href = character
            .current_settlement_id
            .as_deref()
            .map(|settlement_id| format!("/locations/settlement/{settlement_id}"))
            .unwrap_or_else(|| "/characters".to_string());
        return (
            StatusCode::FORBIDDEN,
            Html(
                crate::templates::strategic_notice_page(
                    "Alchemy is out of reach",
                    "Your adventurer must be at this settlement to use its workbench.",
                    &return_href,
                    "Return to your location",
                    Some(&character.name),
                )
                .into_string(),
            ),
        )
            .into_response();
    }
    let medicine = get_character_capability(&state, character.id)
        .await
        .map_or(0.0, |capability| capability.medicine);
    if medicine < adventuresim_core::disease::MEDICINE_VITALS_THRESHOLD {
        return (
            StatusCode::FORBIDDEN,
            Html(crate::templates::strategic_notice_page(
                "More Medicine training required",
                "Alchemy requires Medicine 2. Visit the herbalist for prepared treatments and ingredients.",
                &format!("/settlements/{id}/herbalist"),
                "Return to the herbalist",
                Some(&character.name),
            ).into_string()),
        ).into_response();
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
        return (
            StatusCode::NOT_FOUND,
            Html(
                crate::templates::strategic_notice_page(
                    "Settlement not found",
                    "The requested settlement could not be found.",
                    "/characters",
                    "Return to character select",
                    Some(&character.name),
                )
                .into_string(),
            ),
        )
            .into_response();
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
        )
        .into_string(),
    )
    .into_response()
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
    let map_data_initialized = crate::strategic_map::has_geographic_source(settlement);
    let mut destinations = if map_data_initialized {
        connected_destinations(settlement, &settlements, &edges)
    } else {
        Vec::new()
    };
    let quests: Vec<Quest> = state
        .db
        .query("SELECT * FROM quest")
        .await
        .unwrap_or_default();
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let active_party = if let Some(party_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.party_id.as_ref())
    {
        state
            .db
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let active_quest_id = active_party
        .as_ref()
        .and_then(|party| party.active_quest_id.as_deref());
    let markers = QuestMapMarkers::new(&quests, active_quest_id);
    let map_quests = map_quests_for_settlement(&quests, &settlement.id, active_quest_id);
    for destination in &mut destinations {
        markers.decorate_settlement(destination);
    }
    let active_quest = markers.active_quest();
    let is_current_settlement = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(&settlement.id)
    });
    let can_travel = map_data_initialized && is_current_settlement && active_party.is_some();
    if let Some(quest) = active_quest.filter(|quest| quest.status == QuestStatus::Accepted) {
        if can_travel && settlement.id == quest.settlement_id {
            let distance_m = crate::routes::quests::straight_line_distance_m(quest, settlement);
            destinations.push(TravelDestination {
                id: quest.id.clone(),
                name: quest.title.clone(),
                description: quest.description.clone(),
                summary: Some(active_quest_summary(quest)),
                travel_action: format!("/quests/{}/travel", quest.id),
                distance_m,
                journey_minutes: crate::routes::quests::offroad_journey_minutes(distance_m),
                camp_stop_minutes: Vec::new(),
                camp_forecasts: Vec::new(),
                departure_minute: 0,
                itinerary_total_elapsed_minutes: crate::routes::quests::offroad_journey_minutes(
                    distance_m,
                )
                .saturating_mul(2),
                itinerary_segments: Vec::new(),
                quest_in_progress: true,
                active_quest_route: false,
                turn_in_ready: false,
                open_quest_available: false,
                provision_forecast: None,
                terrain_route: None,
                return_terrain_route: None,
                route_fallback: true,
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
    if let Some(quest) = active_quest.filter(|quest| quest.status == QuestStatus::Completed) {
        for destination in &mut destinations {
            destination.turn_in_ready = destination.id == quest.settlement_id;
        }
        if can_travel && settlement.id != quest.settlement_id {
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
    if let Some(selected_id) = query.destination.as_deref()
        && let Some(destination) = destinations
            .iter_mut()
            .find(|destination| destination.id == selected_id)
    {
        let goal = if destination.quest_in_progress {
            map_quests
                .iter()
                .find(|quest| quest.id == destination.id)
                .map(|quest| (quest.location_coord_y, quest.location_coord_x))
        } else {
            settlements
                .iter()
                .find(|candidate| candidate.id == destination.id)
                .map(|candidate| (candidate.coord_y, candidate.coord_x))
        };
        if let Some(goal) = goal {
            crate::routes::travel::apply_terrain_route(
                destination,
                state.terrain.as_deref(),
                (settlement.coord_y, settlement.coord_x),
                goal,
            )
            .await;
        }
    }
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let living_party_members = living_party_members(&party_members);
    let stats: Vec<CharacterStats> = state
        .db
        .query("SELECT * FROM character_stats")
        .await
        .unwrap_or_default();
    let default_rest_minutes = living_party_members
        .iter()
        .filter_map(|member| stats.iter().find(|row| row.character_id == member.id))
        .map(|row| {
            (row.calories_used.max(0.0) / STRATEGIC_TRAVEL_KCAL_PER_DAY * 1_440.0).ceil() as u64
        })
        .max()
        .unwrap_or(0)
        .max(1);
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
            let times: Vec<CharacterTime> = state
                .db
                .query("SELECT * FROM character_time")
                .await
                .unwrap_or_default();
            let schedules: Vec<CharacterTrainingSchedule> = state
                .db
                .query("SELECT * FROM character_training_schedule")
                .await
                .unwrap_or_default();
            let member_ids: Vec<_> = living_party_members
                .iter()
                .map(|member| member.id)
                .collect();
            populate_itinerary_forecasts(
                &mut destinations,
                &member_ids,
                &attributes,
                &limbs,
                &stats,
                &times,
                &schedules,
                party,
            );
        }
    }
    if can_travel {
        for destination in &mut destinations {
            destination.provision_forecast = travel_provision_forecast(
                &state,
                active_party.as_ref(),
                &living_party_members,
                destination,
                true,
            )
            .await
            .ok()
            .flatten();
        }
    }
    let provision_forecast = query
        .destination
        .as_deref()
        .and_then(|id| destinations.iter().find(|destination| destination.id == id))
        .and_then(|destination| destination.provision_forecast.as_ref());
    let soap_preview = soap_rest_preview(
        &state,
        &party_members,
        active_party.as_ref().map(|party| party.id.as_str()),
    )
    .await;
    Html(
        settlement_map_page(
            settlement,
            &settlements,
            &map_quests,
            state.strategic_map.as_deref(),
            &destinations,
            query.destination.as_deref(),
            active_character.as_ref().map(|(character, _)| character),
            active_party.as_ref(),
            &party_members,
            default_rest_minutes,
            soap_preview,
            can_travel,
            provision_forecast,
            is_current_settlement,
            markers.has_open_quest_at(&settlement.id),
            markers.completed_quest_turn_in_at(&settlement.id),
            active_quest.filter(|quest| {
                can_abandon_active_quest(
                    quest,
                    active_character
                        .as_ref()
                        .and_then(|(character, _)| character.current_quest_location_id.as_deref()),
                )
            }),
            active_character
                .as_ref()
                .map(|(character, _)| character.name.as_str()),
        )
        .into_string(),
    )
}

fn can_abandon_active_quest(quest: &Quest, current_quest_location_id: Option<&str>) -> bool {
    quest.status == QuestStatus::Accepted && current_quest_location_id.is_none()
}

fn map_quests_for_settlement(
    quests: &[Quest],
    settlement_id: &str,
    active_quest_id: Option<&str>,
) -> Vec<Quest> {
    quests
        .iter()
        .filter(|quest| {
            quest.settlement_id == settlement_id
                && (quest.status == QuestStatus::Available
                    || (quest.status == QuestStatus::Accepted
                        && active_quest_id == Some(quest.id.as_str())))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod map_quest_tests {
    use super::*;

    fn quest(status: QuestStatus) -> Quest {
        Quest {
            id: "active".into(),
            title: "Active quest".into(),
            description: String::new(),
            difficulty: 1,
            gold_reward: 1,
            xp_reward: 1,
            settlement_id: "issuer".into(),
            status,
            accepted_by: Some("party".into()),
            enemy_type: String::new(),
            enemy_count: 1,
            location_description: String::new(),
            location_scene_key: String::new(),
            location_coord_x: 0.0,
            location_coord_y: 0.0,
            coordinates_are_geographic: false,
            distance_m: 1_000,
        }
    }

    #[test]
    fn accepted_active_quest_can_only_be_abandoned_before_reaching_its_location() {
        assert!(can_abandon_active_quest(
            &quest(QuestStatus::Accepted),
            None
        ));
        assert!(!can_abandon_active_quest(
            &quest(QuestStatus::Accepted),
            Some("active")
        ));
        assert!(!can_abandon_active_quest(
            &quest(QuestStatus::Completed),
            None
        ));
    }

    #[test]
    fn map_quest_pins_are_bounded_to_the_local_issuer_and_active_destination() {
        let mut local_available = quest(QuestStatus::Available);
        local_available.id = "local-available".into();
        local_available.accepted_by = None;
        let mut remote_available = local_available.clone();
        remote_available.id = "remote-available".into();
        remote_available.settlement_id = "elsewhere".into();
        let mut local_active = quest(QuestStatus::Accepted);
        local_active.id = "local-active".into();
        let mut local_inactive = local_active.clone();
        local_inactive.id = "other-party-active".into();
        let mut completed = quest(QuestStatus::Completed);
        completed.id = "local-completed".into();

        let visible = map_quests_for_settlement(
            &[
                local_available,
                remote_available,
                local_active,
                local_inactive,
                completed,
            ],
            "issuer",
            Some("local-active"),
        );
        let ids = visible
            .iter()
            .map(|quest| quest.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, ["local-available", "local-active"]);
    }
}

#[derive(Deserialize)]
struct TravelConfigurationForm {
    walking_hours: f32,
    #[serde(default)]
    travel_at_night: bool,
}

async fn update_travel_configuration(
    State(state): State<AppState>,
    Path(_id): Path<String>,
    session: Session,
    Form(form): Form<TravelConfigurationForm>,
) -> Response {
    save_travel_configuration(&state, &session, form).await
}

async fn update_camp_travel_configuration(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<TravelConfigurationForm>,
) -> Response {
    save_travel_configuration(&state, &session, form).await
}

async fn save_travel_configuration(
    state: &AppState,
    session: &Session,
    form: TravelConfigurationForm,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let walking_minutes = (form.walking_hours.clamp(0.0, 24.0) * 60.0).round() as u16;
    match state
        .db
        .call(
            "set_party_travel_itinerary",
            &[
                json!(character_id),
                json!(walking_minutes),
                json!(form.travel_at_night),
                json!(false),
                json!((24 * 60_u16).saturating_sub(walking_minutes)),
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
            .query_one::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
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
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(destination_id)
            ))
            .await
            .ok()
            .flatten()
            .map(|item| item.name),
        Some("quest") => state
            .db
            .query_one::<Quest>(&format!(
                "SELECT * FROM quest WHERE id = {}",
                sql_string_literal(destination_id)
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
                "SELECT * FROM party_journey WHERE party_id = {}",
                sql_string_literal(&party.id)
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
    let member_times: Vec<CharacterTime> = state
        .db
        .query("SELECT * FROM character_time")
        .await
        .unwrap_or_default();
    let current_party_minute = party_members
        .iter()
        .filter_map(|member| {
            member_times
                .iter()
                .find(|time| time.character_id == member.id)
        })
        .map(|time| time.minutes)
        .max()
        .unwrap_or(0);
    if let Some(legacy) = journey.as_mut().filter(|journey| journey.plan_version == 0) {
        legacy.completed_elapsed_minutes = legacy.completed_minutes;
        legacy.departure_minute =
            current_party_minute.saturating_sub(legacy.completed_elapsed_minutes);
        legacy.total_elapsed_minutes = if legacy.destination_kind == "quest" {
            legacy.total_minutes.saturating_mul(2)
        } else {
            legacy.total_minutes
        };
    }
    let itinerary = state
        .db
        .query_one::<PartyJourneyItinerary>(&format!(
            "SELECT * FROM party_journey_itinerary WHERE party_id = {}",
            sql_string_literal(&party.id)
        ))
        .await
        .ok()
        .flatten();
    let terrain_route = state
        .db
        .query_one::<PartyJourneyRoute>(&format!(
            "SELECT * FROM party_journey_route WHERE party_id = {}",
            sql_string_literal(&party.id)
        ))
        .await
        .ok()
        .flatten();
    let stats: Vec<CharacterStats> = state
        .db
        .query("SELECT * FROM character_stats")
        .await
        .unwrap_or_default();
    let fatigue_rest_minutes = party_members
        .iter()
        .filter_map(|member| stats.iter().find(|stat| stat.character_id == member.id))
        .map(|stat| ((stat.calories_used / STRATEGIC_TRAVEL_KCAL_PER_DAY) * 1_440.0).ceil() as u64)
        .max()
        .unwrap_or(0);
    let default_rest_minutes = minutes_until_next_walking_start(
        current_party_minute,
        party.walking_minutes_per_day,
        party.travel_at_night,
    )
    .unwrap_or(fatigue_rest_minutes)
    .max(1);
    let planned_wake_minute =
        (current_party_minute.saturating_add(default_rest_minutes) % 1_440) as u16;
    let can_continue_travel = is_walking_time(
        current_party_minute,
        party.walking_minutes_per_day,
        party.travel_at_night,
    );
    let remaining_journey_minutes = journey
        .as_ref()
        .map_or(party.camp_remaining_minutes, |row| {
            row.total_elapsed_minutes
                .saturating_sub(row.completed_elapsed_minutes)
        });
    let provision_forecast = travel_provision_forecast_for_minutes(
        &state,
        Some(&party),
        &party_members,
        remaining_journey_minutes,
        false,
    )
    .await
    .ok()
    .flatten();
    let camp_destinations = camp_settlement_destinations(&state, &party, journey.as_ref()).await;
    let soap_preview = soap_rest_preview(&state, &party_members, Some(&party.id)).await;
    Html(
        camp_page(
            &party,
            journey.as_ref(),
            itinerary.as_ref(),
            terrain_route.as_ref(),
            &destination_name,
            Some(&character),
            &party_members,
            &camp_destinations,
            provision_forecast.as_ref(),
            default_rest_minutes,
            soap_preview,
            planned_wake_minute,
            can_continue_travel,
            Some(&character.name),
        )
        .into_string(),
    )
    .into_response()
}

async fn camp_settlement_destinations(
    state: &AppState,
    party: &Party,
    journey: Option<&PartyJourney>,
) -> Vec<CampTravelDestination> {
    let Some(journey) = journey else {
        return Vec::new();
    };
    let mut endpoints = Vec::new();
    if journey.origin_kind == "settlement" && journey.completed_minutes > 0 {
        endpoints.push((journey.origin_id.as_str(), journey.completed_minutes));
    }
    if journey.destination_kind == "settlement" {
        endpoints.push((
            journey.destination_id.as_str(),
            journey
                .total_minutes
                .saturating_sub(journey.completed_minutes),
        ));
    }

    let mut destinations = Vec::new();
    for (id, journey_minutes) in endpoints {
        if destinations
            .iter()
            .any(|destination: &CampTravelDestination| destination.id == id)
        {
            continue;
        }
        let settlement = state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(id)
            ))
            .await
            .ok()
            .flatten();
        if let Some(settlement) = settlement {
            destinations.push(CampTravelDestination {
                current: party.camp_destination_kind.as_deref() == Some("settlement")
                    && party.camp_destination_id.as_deref() == Some(id),
                id: settlement.id,
                name: settlement.name,
                journey_minutes,
            });
        }
    }
    destinations
}

async fn rest_at_camp(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let requested_minutes = match travel_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match state
        .db
        .call(
            "rest_at_camp",
            &[json!(character_id), json!(requested_minutes)],
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
        // A normal form redirect re-renders the authoritative camp or arrival
        // state. This remains reliable even when the live revision races the
        // reducer response.
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn change_camp_destination(
    State(state): State<AppState>,
    session: Session,
    Path(settlement_id): Path<String>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "travel_to_settlement",
            &[json!(character_id), json!(settlement_id)],
        )
        .await
    {
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(crate) async fn travel_provision_forecast(
    state: &AppState,
    party: Option<&Party>,
    travelers: &[Character],
    destination: &TravelDestination,
    departing_settlement: bool,
) -> Result<Option<TravelProvisionForecast>, String> {
    travel_provision_forecast_for_minutes(
        state,
        party,
        travelers,
        destination.itinerary_total_elapsed_minutes,
        departing_settlement,
    )
    .await
}

async fn travel_provision_forecast_for_minutes(
    state: &AppState,
    party: Option<&Party>,
    travelers: &[Character],
    planning_minutes: u64,
    departing_settlement: bool,
) -> Result<Option<TravelProvisionForecast>, String> {
    let travelers: Vec<_> = travelers.iter().filter(|traveler| traveler.alive).collect();
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
    let mut food_reserve_kcal = 0.0;
    let mut water_reserve_ml = 0.0;
    let mut ration_count = 0;
    let mut waterskin_count = 0;
    let mut alcohol_supplies = Vec::new();
    let mut expected_morale_demands = Vec::new();
    for traveler in &travelers {
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
        for entry in &inventory {
            if let Some(def) = items.iter().find(|def| def.id == entry.item_id) {
                alcohol_supplies.push(adventuresim_core::alcohol::ScopedAlcoholSupply {
                    properties: adventuresim_core::alcohol::AlcoholProperties {
                        serving_ml: def.alcohol_serving_ml,
                        abv_basis_points: def.alcohol_abv_basis_points,
                        net_hydration_ml: def.alcohol_net_hydration_ml,
                        disinfectant_effectiveness: def.alcohol_disinfectant_effectiveness,
                        disinfectant_focused: def.alcohol_disinfectant_focused,
                        potable: def.alcohol_potable,
                    },
                    quantity: entry.qty,
                    stable_id: entry.id,
                    owner: Some(traveler.id),
                });
            }
        }
        let time = query_single::<CharacterTime>(state, "character_time", traveler.id).await;
        let personality =
            query_single::<CharacterPersonality>(state, "character_personality", traveler.id).await;
        if let Some(time) = time {
            let evenings: Vec<_> = adventuresim_core::alcohol::crossed_evenings(
                time.minutes,
                time.minutes.saturating_add(planning_minutes),
            )
            .collect();
            let demand = match personality.map(|p| p.temperance) {
                Some(crate::spacetimedb::Temperance::Temperate) => 0,
                Some(crate::spacetimedb::Temperance::Drunkard) => (evenings.len() as u32)
                    .saturating_mul(adventuresim_core::alcohol::HEAVY_ETHANOL_ML),
                _ => {
                    let mut heavy_evenings: Vec<u64> = state
                        .db
                        .query::<AlcoholConsumption>(&format!(
                            "SELECT * FROM alcohol_consumption WHERE character_id = {}",
                            traveler.id
                        ))
                        .await
                        .map_err(|error| error.to_string())?
                        .into_iter()
                        .filter(|row| adventuresim_core::alcohol::qualifying_heavy(row.ethanol_ml))
                        .map(|row| row.evening_id)
                        .collect();
                    evenings.into_iter().fold(0_u32, |total, evening| {
                        let had_recent_heavy = heavy_evenings.iter().any(|prior| {
                            *prior < evening
                                && evening - *prior < adventuresim_core::alcohol::ROLLING_WEEK_DAYS
                        });
                        let target = if had_recent_heavy {
                            adventuresim_core::alcohol::MODEST_ETHANOL_ML
                        } else {
                            heavy_evenings.push(evening);
                            adventuresim_core::alcohol::HEAVY_ETHANOL_ML
                        };
                        total.saturating_add(target)
                    })
                }
            };
            expected_morale_demands.push((traveler.id, demand));
        }
        let owned = |item_id: &str| {
            inventory
                .iter()
                .filter(|entry| entry.item_id == item_id)
                .map(|entry| entry.qty)
                .sum::<u32>()
        };
        food_reserve_kcal += needs.food_balance_kcal;
        water_reserve_ml += needs.water_balance_ml;
        ration_count += owned(STANDARD_TRAVEL_RATION_ID);
        let skins = owned(STANDARD_WATERSKIN_ID);
        if departing_settlement {
            waterskin_count += skins;
        } else {
            water_reserve_ml += needs.carried_water_ml.max(0.0);
        }
    }
    if let Some(party) = party {
        let pooled: Vec<PartyInventoryItem> = state
            .db
            .query(&format!(
                "SELECT * FROM party_inventory_item WHERE party_id = {}",
                sql_string_literal(&party.id)
            ))
            .await
            .map_err(|error| error.to_string())?;
        for entry in &pooled {
            if let Some(def) = items.iter().find(|def| def.id == entry.item_id) {
                alcohol_supplies.push(adventuresim_core::alcohol::ScopedAlcoholSupply {
                    properties: adventuresim_core::alcohol::AlcoholProperties {
                        serving_ml: def.alcohol_serving_ml,
                        abv_basis_points: def.alcohol_abv_basis_points,
                        net_hydration_ml: def.alcohol_net_hydration_ml,
                        disinfectant_effectiveness: def.alcohol_disinfectant_effectiveness,
                        disinfectant_focused: def.alcohol_disinfectant_focused,
                        potable: def.alcohol_potable,
                    },
                    quantity: entry.quantity,
                    stable_id: entry.id,
                    owner: None,
                });
            }
        }
        ration_count += pooled
            .iter()
            .filter(|row| row.item_id == STANDARD_TRAVEL_RATION_ID)
            .map(|row| row.quantity)
            .sum::<u32>();
        let party_skins = pooled
            .iter()
            .filter(|row| row.item_id == STANDARD_WATERSKIN_ID)
            .map(|row| row.quantity)
            .sum::<u32>();
        if departing_settlement {
            waterskin_count += party_skins;
        } else {
            water_reserve_ml += party.pooled_water_ml.max(0.0);
        }
    }
    let emergency_alcohol_hydration_ml =
        adventuresim_core::alcohol::hydration_after_expected_drinking(
            alcohol_supplies,
            &expected_morale_demands,
        );
    let inputs = PartyProvisioningInputs {
        planning_minutes,
        living_members: travelers.len() as u32,
        food_reserve_kcal,
        water_reserve_ml,
        ration_count,
        waterskin_count,
        ration_kcal: ration.nutrition_kcal,
        waterskin_capacity_ml: waterskin.water_capacity_ml,
        emergency_alcohol_hydration_ml,
        ..Default::default()
    };
    let result = inputs.forecast();
    Ok(Some(TravelProvisionForecast {
        planning_minutes,
        living_members: travelers.len() as u32,
        food_days: result.food_days,
        water_days: result.water_days,
        ordinary_water_days: result.ordinary_water_days,
        emergency_alcohol_days: result.emergency_alcohol_days,
        emergency_alcohol_hydration_ml,
        food_reserve_kcal,
        water_reserve_ml,
        ration_count,
        waterskin_count,
        ration_kcal: ration.nutrition_kcal,
        waterskin_capacity_ml: waterskin.water_capacity_ml,
        rations_to_buy: result.rations_to_buy,
        waterskins_to_buy: result.waterskins_to_buy,
    }))
}

pub(crate) fn living_party_members(members: &[Character]) -> Vec<Character> {
    members
        .iter()
        .filter(|member| member.alive)
        .cloned()
        .collect()
}

#[derive(Serialize)]
struct ServiceQuestOffer {
    id: String,
    title: String,
    description: String,
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
struct ApprenticeshipResult {
    enrolled: bool,
    message: &'static str,
}

async fn begin_service_apprenticeship(
    State(state): State<AppState>,
    Path((id, service_id)): Path<(String, String)>,
    session: Session,
) -> Json<ApprenticeshipResult> {
    const PROFESSIONS: &[&str] = &[
        "merchants",
        "weapons",
        "armor",
        "clothing",
        "herbalist",
        "inn",
        "religion",
    ];
    if !PROFESSIONS.contains(&service_id.as_str()) {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "That profession is not taught here.",
        });
    }
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "Choose a character before asking to train.",
        });
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "You must speak to the guild member in person.",
        });
    }
    match state
        .db
        .call(
            "begin_apprenticeship",
            &[json!(character.id), json!(service_id)],
        )
        .await
    {
        Ok(()) => Json(ApprenticeshipResult {
            enrolled: true,
            message: if service_id == "religion" {
                "Then you shall begin as a novice. In time, a cleric may become a teacher."
            } else {
                "Then your apprenticeship begins today."
            },
        }),
        Err(error) => {
            tracing::warn!(%error, character_id = character.id, %service_id, "failed to begin apprenticeship");
            Json(ApprenticeshipResult {
                enrolled: false,
                message: "I cannot take you on just now.",
            })
        }
    }
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
            "SELECT * FROM quest_issuer WHERE settlement_id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default();
    let quests: Vec<Quest> = state
        .db
        .query(&format!(
            "SELECT * FROM quest WHERE settlement_id = {}",
            sql_string_literal(&id)
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
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
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
                    description: active_quest_tooltip(quest),
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
                        "Excellent work. Here is the promised {} coin. You've earned it.",
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
        "Yes, {situation}. I believe there are about {low} or {high} {}, give or take. I'd offer {} coin to anyone who clears them out. Are you",
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
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(id)
            ))
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
            .query_one::<Quest>(&format!(
                "SELECT * FROM quest WHERE id = {}",
                sql_string_literal(id)
            ))
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
        active_building: None,
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
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".to_string()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".to_string());
        }
    };
    location.active_building = building.valid().map(str::to_owned);
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
    let combat_profile = get_combat_training_profile(&state, character_id).await;
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
    let prayer_religion_check = match religion.as_deref() {
        Some(religion_id) => {
            party_religion_knowledge_check(&state, &party_members, religion_id).await
        }
        None => 0.0,
    };
    let notoriety = query_single::<CharacterNotoriety>(&state, "character_notoriety", character_id)
        .await
        .map_or(0.0, |notoriety| notoriety.value);
    let personality =
        query_single::<CharacterPersonality>(&state, "character_personality", character_id).await;
    let medical = medical_presentation(&state, character_id, character_id).await;
    let injuries = state
        .db
        .query::<LimbInjury>(&format!(
            "SELECT * FROM limb_injury WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let projectiles = state
        .db
        .query::<RetainedProjectile>(&format!(
            "SELECT * FROM retained_projectile WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let religious_demand = state
        .db
        .query::<ReligiousDemand>(&format!(
            "SELECT * FROM religious_demand WHERE character_id = {character_id} AND status = 'pending'"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let filth = state
        .db
        .query::<crate::spacetimedb::CharacterFilth>(&format!(
            "SELECT * FROM character_filth WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
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
            prayer_religion_check,
            schedule.first(),
            combat_profile,
            activity_preview,
            religious_demand.as_ref(),
            notoriety,
            personality.as_ref(),
            &medical,
            can_examine,
            &injuries,
            &projectiles,
            &filth,
        )
        .into_string(),
    )
}

async fn party_religion_knowledge_check(
    state: &AppState,
    party_members: &[Character],
    religion_id: &str,
) -> f32 {
    let Some(religion) = OfficialReligion::from_id(religion_id) else {
        return 0.0;
    };
    let mut checks = Vec::with_capacity(party_members.len());
    for member in living_party_member_refs(party_members) {
        let skills = query_single::<CharacterSkills>(state, "character_skills", member.id).await;
        let attributes =
            query_single::<CharacterAttributes>(state, "character_attributes", member.id).await;
        let limbs = query_single::<CharacterLimbs>(state, "character_limbs", member.id).await;
        let stats = query_single::<CharacterStats>(state, "character_stats", member.id).await;
        if let (Some(skills), Some(attributes), Some(limbs), Some(stats)) =
            (skills, attributes, limbs, stats)
        {
            checks.push(adventuresim_core::capability::religion_knowledge_check(
                skills.religion_hours.effective(religion),
                attributes.instinct,
                attributes.intelligence,
                stats.focus,
                limbs.head_health,
            ));
        }
    }
    adventuresim_core::capability::aggregate_party_check(checks).clamp(0.0, 5.0)
}

fn living_party_member_refs(party_members: &[Character]) -> impl Iterator<Item = &Character> {
    party_members.iter().filter(|member| member.alive)
}

#[cfg(test)]
mod party_religion_knowledge_tests {
    use super::living_party_member_refs;
    use crate::spacetimedb::Character;

    fn party_member(id: u64, alive: bool) -> Character {
        Character {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_quest_location_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
        }
    }

    #[test]
    fn prayer_preview_knowledge_excludes_dead_party_members() {
        let members = [party_member(1, true), party_member(2, false)];
        let ids = living_party_member_refs(&members)
            .map(|member| member.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![1]);
    }
}

#[derive(Deserialize)]
struct ReligiousDemandForm {
    choice: String,
}

async fn resolve_religious_demand(
    State(state): State<AppState>,
    Path((kind, id, character_id, demand_id)): Path<(String, String, u64, u64)>,
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{character_id}")))
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct TrainingScheduleForm {
    combat_training_minutes: u16,
    carousing_minutes: u16,
    apprenticeship_minutes: u16,
    apprenticeship_service_id: Option<String>,
    profession_practice_minutes: u16,
    profession_service_id: Option<String>,
    #[serde(default)]
    combat_minutes: u16,
    #[serde(default)]
    combat_auto_train: bool,
    melee_minutes: u16,
    dodge_minutes: u16,
    block_minutes: u16,
    ranged_minutes: u16,
    will_minutes: u16,
    charisma_minutes: u16,
    medicine_minutes: u16,
    #[serde(default)]
    religion_minutes: u16,
    #[serde(default)]
    religion_auto_train: bool,
    #[serde(default)]
    religion_roman_catholic_minutes: u16,
    #[serde(default)]
    religion_lutheran_minutes: u16,
    #[serde(default)]
    religion_reformed_minutes: u16,
    #[serde(default)]
    religion_anglican_minutes: u16,
    #[serde(default)]
    religion_eastern_orthodox_minutes: u16,
    #[serde(default)]
    religion_islamic_minutes: u16,
    #[serde(default)]
    religion_judaism_minutes: u16,
    stealth_minutes: u16,
    balance_minutes: u16,
    surgeon_minutes: u16,
    smithing_minutes: u16,
    labor_minutes: u16,
    prayer_minutes: u16,
    thievery_minutes: u16,
    raiding_minutes: u16,
}

#[cfg(test)]
mod training_schedule_form_tests {
    use super::TrainingScheduleForm;
    use serde_json::json;

    #[test]
    fn omitted_checkbox_and_inactive_religion_inputs_deserialize_as_false_and_zero() {
        let form: TrainingScheduleForm = serde_json::from_value(json!({
            "melee_minutes": 0, "dodge_minutes": 0, "block_minutes": 0,
            "ranged_minutes": 0, "will_minutes": 0, "charisma_minutes": 0,
            "medicine_minutes": 0, "stealth_minutes": 0, "balance_minutes": 0,
            "surgeon_minutes": 0, "smithing_minutes": 0, "labor_minutes": 0,
            "prayer_minutes": 0, "thievery_minutes": 0, "raiding_minutes": 0
        }))
        .unwrap();
        assert!(!form.religion_auto_train);
        assert!(!form.combat_auto_train);
        assert_eq!(form.combat_minutes, 0);
        assert_eq!(form.religion_minutes, 0);
        assert_eq!(form.religion_judaism_minutes, 0);
    }

    #[test]
    fn submitted_schedule_retains_both_religion_allocation_branches() {
        let form: TrainingScheduleForm = serde_json::from_value(json!({
            "melee_minutes": 0, "dodge_minutes": 0, "block_minutes": 0,
            "ranged_minutes": 0, "will_minutes": 0, "charisma_minutes": 0,
            "medicine_minutes": 0, "combat_minutes": 90, "combat_auto_train": true,
            "religion_minutes": 120,
            "religion_auto_train": false, "religion_judaism_minutes": 45,
            "stealth_minutes": 0, "balance_minutes": 0, "surgeon_minutes": 0,
            "smithing_minutes": 0, "labor_minutes": 0, "prayer_minutes": 0,
            "thievery_minutes": 0, "raiding_minutes": 0
        }))
        .unwrap();
        assert!(!form.religion_auto_train);
        assert!(form.combat_auto_train);
        assert_eq!(form.combat_minutes, 90);
        assert_eq!(form.religion_minutes, 120);
        assert_eq!(form.religion_judaism_minutes, 45);
    }
}

async fn update_training_schedule(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<TrainingScheduleForm>,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (
            StatusCode::FORBIDDEN,
            "Select this character before editing their schedule",
        )
            .into_response();
    }
    let downtime = ScheduleAllocation {
        combat_training_minutes: form.combat_training_minutes,
        carousing_minutes: form.carousing_minutes,
        apprenticeship_minutes: form.apprenticeship_minutes,
        apprenticeship_service_id: form.apprenticeship_service_id,
        profession_practice_minutes: form.profession_practice_minutes,
        profession_service_id: form.profession_service_id,
        combat_minutes: form.combat_minutes,
        combat_auto_train: form.combat_auto_train,
        melee_minutes: form.melee_minutes,
        dodge_minutes: form.dodge_minutes,
        block_minutes: form.block_minutes,
        ranged_minutes: form.ranged_minutes,
        will_minutes: form.will_minutes,
        charisma_minutes: form.charisma_minutes,
        medicine_minutes: form.medicine_minutes,
        religion_minutes: form.religion_minutes,
        religion_auto_train: form.religion_auto_train,
        religion_minutes_by_tradition: adventuresim_world_schema::ReligionMinutes {
            roman_catholic: form.religion_roman_catholic_minutes,
            lutheran: form.religion_lutheran_minutes,
            reformed: form.religion_reformed_minutes,
            anglican: form.religion_anglican_minutes,
            eastern_orthodox: form.religion_eastern_orthodox_minutes,
            islamic: form.religion_islamic_minutes,
            judaism: form.religion_judaism_minutes,
        },
        stealth_minutes: form.stealth_minutes,
        balance_minutes: form.balance_minutes,
        surgeon_minutes: form.surgeon_minutes,
        smithing_minutes: form.smithing_minutes,
        labor_minutes: form.labor_minutes,
        prayer_minutes: form.prayer_minutes,
        thievery_minutes: form.thievery_minutes,
        raiding_minutes: form.raiding_minutes,
    };
    match state
        .db
        .call(
            "update_training_schedule",
            &[
                json!(character_id),
                schedule_allocation_reducer_arg(&downtime),
                schedule_allocation_reducer_arg(&ScheduleAllocation::default()),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to(
            &building.append_to(format!("/locations/{kind}/{id}/party/{character_id}")),
        )
        .into_response(),
        Err(error) => {
            tracing::warn!(%error, character_id, "failed to update training schedule");
            (StatusCode::BAD_REQUEST, error.to_string()).into_response()
        }
    }
}

async fn party_member(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".to_string()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".to_string());
        }
    };
    location.active_building = building.valid().map(str::to_owned);

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
    let encumbrance_rows =
        EncumbranceRows::query(&state, &[selected.id, active_character.id]).await;
    let selected_encumbrance =
        personal_encumbrance(selected.id, &selected_inventory, &items, &encumbrance_rows);
    let active_encumbrance = personal_encumbrance(
        active_character.id,
        &active_inventory,
        &items,
        &encumbrance_rows,
    );

    if character_id == active_character.id {
        return Html(
            party_discard_page(
                &location,
                &active_character,
                &active_inventory,
                &items,
                &party_members,
                active_equip.first(),
                active_encumbrance,
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
            selected_encumbrance,
            active_encumbrance,
        )
        .into_string(),
    )
}

async fn party_pool_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".into()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".into());
        }
    };
    location.active_building = building.valid().map(str::to_owned);
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
            "SELECT * FROM party_inventory_item WHERE party_id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .unwrap_or_default();
    let stakes: Vec<PartyStake> = state
        .db
        .query(&format!(
            "SELECT * FROM party_stake WHERE party_id = {}",
            sql_string_literal(party_id)
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
    let encumbrance = inventory_encumbrance_summaries(
        &state, &character, &inventory, &members, &pooled, &items, true,
    )
    .await;
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
            encumbrance.party,
            encumbrance.personal,
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
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party-inventory")))
}

async fn withdraw_party_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party-inventory")))
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
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party-inventory")))
}

async fn remove_party_member(
    State(state): State<AppState>,
    Path((kind, id, member_character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}")))
}

async fn party_stats(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".to_string()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".to_string());
        }
    };
    location.active_building = building.valid().map(str::to_owned);
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
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next(),
        None => None,
    };
    let selected_party = match selected.party_id.as_deref() {
        Some(party_id) => state
            .db
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
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
    let combat_profile = get_combat_training_profile(&state, character_id).await;
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
    let injuries = state
        .db
        .query::<LimbInjury>(&format!(
            "SELECT * FROM limb_injury WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let projectiles = state
        .db
        .query::<RetainedProjectile>(&format!(
            "SELECT * FROM retained_projectile WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let filth = state
        .db
        .query::<CharacterFilth>(&format!(
            "SELECT * FROM character_filth WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
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
            combat_profile,
            condition.as_ref(),
            &morale_sources,
            religion.as_deref(),
            active_party.as_ref(),
            selected_party.as_ref(),
            notoriety,
            personality.as_ref(),
            &medical,
            can_examine,
            &injuries,
            &projectiles,
            &filth,
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
    crate::medical::sanitize(
        &rows,
        &medications,
        examination.as_ref(),
        time,
        attributes.map_or(3.0, |a| a.immunity),
        viewer.map_or(0.0, |capability| capability.medicine),
    )
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
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    )))
}

async fn finalize_party_offer(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    )))
}

async fn transfer_party_item(
    State(state): State<AppState>,
    Path((kind, id, recipient_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<PartyTransferForm>,
) -> Redirect {
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    if form.from_character_id != active_character.id && recipient_id != active_character.id {
        return Redirect::to(&building.append_to(format!("/locations/{kind}/{id}")));
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
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{comparison_character_id}/inventory"
    )))
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
    let fallback = format!("/settlements/{id}/merchants");
    let mut trade_completed = false;
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
                match state
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
                    .await
                {
                    Ok(()) => trade_completed = true,
                    Err(error) => {
                        tracing::warn!(%error, settlement_id = %id, "merchant offer was rejected");
                    }
                }
            } else {
                trade_completed = true;
            }
        }
    }
    if trade_completed {
        redirect_to_local(&form.return_to, &fallback)
    } else {
        Redirect::to(&fallback)
    }
}

async fn rest_at_settlement_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters").into_response();
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            "The party is not at this settlement",
        )
            .into_response();
    }
    let requested_minutes = match travel_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match state
        .db
        .call(
            "rest_at_camp",
            &[json!(character.id), json!(requested_minutes)],
        )
        .await
    {
        Ok(()) => Redirect::to(&format!("/locations/settlement/{id}/map")).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn inn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlements: Vec<Settlement> = state
        .db
        .query(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
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
    let soap_preview = soap_rest_preview(
        &state,
        active_character
            .as_ref()
            .map_or(&[][..], |(character, _)| std::slice::from_ref(character)),
        active_character
            .as_ref()
            .and_then(|(character, _)| character.party_id.as_deref()),
    )
    .await;
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
            soap_preview,
            logged_in_as.as_deref(),
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
pub(crate) struct RestForm {
    pub(crate) duration: String,
    pub(crate) unit: String,
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    pub(crate) requested_minutes: Option<u64>,
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.parse().map_err(serde::de::Error::custom))
        .transpose()
}

const MAX_SETTLEMENT_REST_MINUTES: u64 = 365 * 1_440;

fn settlement_rest_minutes(form: &RestForm) -> Result<u64, &'static str> {
    let minutes = parsed_rest_minutes(form)?;
    if minutes < 1_440 {
        return Err("Settlement rest must last at least one day");
    }
    if minutes > MAX_SETTLEMENT_REST_MINUTES {
        return Err("Settlement rest cannot exceed 365 days");
    }
    Ok(minutes)
}

pub(crate) fn travel_rest_minutes(form: &RestForm) -> Result<u64, &'static str> {
    let minutes = parsed_rest_minutes(form)?;
    if minutes == 0 {
        return Err("Rest must last at least one minute");
    }
    if minutes > MAX_SETTLEMENT_REST_MINUTES {
        return Err("Rest cannot exceed 365 days");
    }
    Ok(minutes)
}

fn parsed_rest_minutes(form: &RestForm) -> Result<u64, &'static str> {
    Ok(match form.unit.as_str() {
        "hours" => {
            let (hours, minutes) = form
                .duration
                .split_once(':')
                .ok_or("Rest duration must use HH:MM")?;
            if minutes.len() != 2 || !minutes.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("Rest duration must use HH:MM");
            }
            let hours = hours
                .parse::<u64>()
                .map_err(|_| "Rest duration must use HH:MM")?;
            let minutes = minutes
                .parse::<u64>()
                .map_err(|_| "Rest duration must use HH:MM")?;
            if minutes >= 60 {
                return Err("Rest duration minutes must be between 00 and 59");
            }
            let duration_minutes = hours
                .checked_mul(60)
                .and_then(|value| value.checked_add(minutes))
                .ok_or("Rest duration is too large")?;
            if let Some(requested_minutes) = form.requested_minutes
                && requested_minutes != duration_minutes
            {
                return Err("Rest duration does not match the selected wake time");
            }
            form.requested_minutes.unwrap_or(duration_minutes)
        }
        "days" => {
            let days = form
                .duration
                .parse::<u64>()
                .map_err(|_| "Rest days must be a whole number")?;
            days.saturating_mul(1_440)
        }
        _ => return Err("Unknown rest duration unit"),
    })
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
    let requested_minutes = match settlement_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(message) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Html(format!("<h1>Unable to rest</h1><p>{message}</p>")),
            )
                .into_response();
        }
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
            &[json!(character_id), json!(requested_minutes), json!(at_inn)],
        )
        .await
    {
        return Html(format!("<h1>Unable to rest</h1><p>{error}</p>")).into_response();
    }

    let settlements: Vec<Settlement> = state
        .db
        .query(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
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
        before_character
            .as_ref()
            .map_or(&[], |(_, inventory)| inventory.as_slice()),
        active_character
            .as_ref()
            .map_or(&[], |(_, inventory)| inventory.as_slice()),
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
    let soap_preview = soap_rest_preview(
        &state,
        active_character
            .as_ref()
            .map_or(&[][..], |(character, _)| std::slice::from_ref(character)),
        active_character
            .as_ref()
            .and_then(|(character, _)| character.party_id.as_deref()),
    )
    .await;
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
            at_inn,
            &summary,
            soap_preview,
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
    before_inventory: &[InventoryItem],
    after_inventory: &[InventoryItem],
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
    let currency_total = |inventory: &[InventoryItem]| -> u32 {
        inventory
            .iter()
            .filter(|item| adventuresim_core::strategic_currency::is_currency_id(&item.item_id))
            .map(|item| item.qty)
            .sum()
    };
    let before_currency = currency_total(before_inventory);
    let after_currency = currency_total(after_inventory);
    let gold_spent = before_currency.saturating_sub(after_currency);
    let gold_earned = after_currency.saturating_sub(before_currency);
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
        (
            "Religion",
            before.religion_hours.total_direct(),
            after.religion_hours.total_direct(),
        ),
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
    Form(_form): Form<TravelForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };

    let outcome = super::execute_or_request_party_action(
        &state,
        character_id,
        super::PartyAction::TravelToSettlement {
            settlement_id: id.clone(),
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
    let fallback = format!("/settlements/{id}/herbalist");
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    let mut purchase_completed = false;
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
        match state
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
            Ok(()) => purchase_completed = true,
            Err(error) => {
                tracing::warn!(%error, character_id = character.id, "herbalist purchase rejected");
            }
        }
    }
    if purchase_completed {
        redirect_to_local(&form.return_to, &fallback)
    } else {
        Redirect::to(&fallback)
    }
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
        .query::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
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
        .query::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
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
    Query(building): Query<BuildingQuery>,
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
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{character_id}")))
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
    SoapRestPreview,
    Option<&str>,
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
    let encumbrance = inventory_encumbrance_summaries(
        &state,
        character,
        inventory,
        &party_members,
        &pooled,
        &items,
        !matches!(shop, MerchantShop::Herbalist),
    )
    .await;
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
            shop,
            &conditions.unwrap_or_default(),
            smiths.unwrap_or_default().first(),
            &orders.unwrap_or_default(),
            times
                .unwrap_or_default()
                .first()
                .map_or(0, |time| time.minutes),
            encumbrance.personal,
            encumbrance.party,
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
    let party_sql = format!(
        "SELECT * FROM party WHERE id = {}",
        sql_string_literal(party_id)
    );
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
        "SELECT * FROM party_inventory_item WHERE party_id = {}",
        sql_string_literal(party_id)
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
    let settlement_sql = format!(
        "SELECT * FROM settlement WHERE id = {}",
        sql_string_literal(&id)
    );
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
    let soap_preview = soap_rest_preview(
        &state,
        active_character_ref.map_or(&[][..], std::slice::from_ref),
        active_character_ref.and_then(|character| character.party_id.as_deref()),
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
            &items.unwrap_or_default(),
            &party_members,
            limbs.as_ref(),
            stats.as_ref(),
            condition.as_ref(),
            equipment_recovery.0,
            equipment_recovery.1,
            soap_preview,
            logged_in_as.as_deref(),
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

#[derive(Default)]
struct EncumbranceRows {
    attributes: Vec<CharacterAttributes>,
    limbs: Vec<CharacterLimbs>,
    conditions: Vec<CharacterCondition>,
    needs: Vec<CharacterNeeds>,
}

const ENCUMBRANCE_QUERY_CONCURRENCY: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
struct InventoryEncumbranceSummaries {
    personal: EncumbranceSummary,
    party: EncumbranceSummary,
}

fn encumbrance_query_ids(members: &[Character], active_character_id: u64) -> (Vec<u64>, Vec<u64>) {
    let living_ids: std::collections::BTreeSet<u64> = members
        .iter()
        .filter(|member| member.alive)
        .map(|member| member.id)
        .collect();
    let mut row_ids = living_ids.clone();
    row_ids.insert(active_character_id);
    (
        living_ids.into_iter().collect(),
        row_ids.into_iter().collect(),
    )
}

async fn inventory_encumbrance_summaries(
    state: &AppState,
    active_character: &Character,
    active_inventory: &[InventoryItem],
    members: &[Character],
    pooled: &[PartyInventoryItem],
    items: &[ItemDefinition],
    include_party: bool,
) -> InventoryEncumbranceSummaries {
    let aggregate_members = include_party.then_some(members).unwrap_or_default();
    let (member_ids, encumbrance_ids) =
        encumbrance_query_ids(aggregate_members, active_character.id);
    let all_inventories = stream::iter(member_ids)
        .map(|member_id| async move {
            if member_id == active_character.id {
                active_inventory.to_vec()
            } else {
                state
                    .db
                    .query::<InventoryItem>(&format!(
                        "SELECT * FROM inventory_item WHERE character_id = {member_id}"
                    ))
                    .await
                    .unwrap_or_default()
            }
        })
        .buffer_unordered(ENCUMBRANCE_QUERY_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let rows = EncumbranceRows::query(state, &encumbrance_ids).await;
    InventoryEncumbranceSummaries {
        personal: personal_encumbrance(active_character.id, active_inventory, items, &rows),
        party: include_party
            .then(|| party_encumbrance(members, &all_inventories, pooled, items, &rows))
            .unwrap_or_default(),
    }
}

impl EncumbranceRows {
    async fn query(state: &AppState, character_ids: &[u64]) -> Self {
        let unique_ids: std::collections::BTreeSet<u64> = character_ids.iter().copied().collect();
        let lookups = stream::iter(unique_ids)
            .map(|character_id| async move {
                // Keep each member's four lookups sequential so the outer
                // buffer is a bound on actual in-flight database calls.
                let attributes = query_single::<CharacterAttributes>(
                    state,
                    "character_attributes",
                    character_id,
                )
                .await;
                let limbs =
                    query_single::<CharacterLimbs>(state, "character_limbs", character_id).await;
                let condition =
                    query_single::<CharacterCondition>(state, "character_condition", character_id)
                        .await;
                let needs =
                    query_single::<CharacterNeeds>(state, "character_needs", character_id).await;
                (attributes, limbs, condition, needs)
            })
            .buffer_unordered(ENCUMBRANCE_QUERY_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        let mut rows = Self::default();
        for (attributes, limbs, condition, needs) in lookups {
            rows.attributes.extend(attributes);
            rows.limbs.extend(limbs);
            rows.conditions.extend(condition);
            rows.needs.extend(needs);
        }
        rows
    }
}

fn item_stack_weight_kg(item_id: &str, quantity: u32, items: &[ItemDefinition]) -> f32 {
    items
        .iter()
        .find(|definition| definition.id == item_id)
        .map_or(0.0, |definition| {
            definition.weight.max(0.0) * quantity as f32
        })
}

fn personal_encumbrance(
    character_id: u64,
    inventory: &[InventoryItem],
    items: &[ItemDefinition],
    rows: &EncumbranceRows,
) -> EncumbranceSummary {
    let body_weight = rows
        .conditions
        .iter()
        .find(|row| row.character_id == character_id)
        .map_or(0.0, |row| row.body_weight_kg.max(0.0));
    let water_weight = rows
        .needs
        .iter()
        .find(|row| row.character_id == character_id)
        .map_or(0.0, |row| row.carried_water_ml.max(0.0) / 1_000.0);
    let inventory_weight = inventory
        .iter()
        .filter(|row| row.character_id == character_id)
        .map(|row| item_stack_weight_kg(&row.item_id, row.qty, items))
        .sum::<f32>();
    let capacity = rows
        .attributes
        .iter()
        .find(|row| row.character_id == character_id)
        .zip(
            rows.limbs
                .iter()
                .find(|row| row.character_id == character_id),
        )
        .map_or(0.0, |(attributes, limbs)| {
            encumbrance_capacity_kg(
                (attributes.left_leg_strength * limbs.left_leg_health.clamp(0.0, 1.0)
                    + attributes.right_leg_strength * limbs.right_leg_health.clamp(0.0, 1.0))
                    / 2.0,
            )
        });

    EncumbranceSummary::new(body_weight + water_weight + inventory_weight, capacity)
}

fn party_encumbrance(
    members: &[Character],
    inventories: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    items: &[ItemDefinition],
    rows: &EncumbranceRows,
) -> EncumbranceSummary {
    let member_summary = members.iter().filter(|member| member.alive).fold(
        EncumbranceSummary::default(),
        |summary, member| {
            summary.combined(personal_encumbrance(member.id, inventories, items, rows))
        },
    );
    let pooled_weight = pooled
        .iter()
        .map(|row| item_stack_weight_kg(&row.item_id, row.quantity, items))
        .sum::<f32>();
    member_summary.combined(EncumbranceSummary::new(pooled_weight, 0.0))
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

async fn get_combat_training_profile(state: &AppState, character_id: u64) -> CombatTrainingProfile {
    let Some(equip) = query_single::<CharacterEquip>(state, "character_equip", character_id).await
    else {
        return CombatTrainingProfile::default();
    };
    let mut hands = Vec::new();
    for inventory_id in [equip.left_hand_item_id, equip.right_hand_item_id]
        .into_iter()
        .flatten()
    {
        let inventory = state
            .db
            .query_one::<InventoryItem>(&format!(
                "SELECT * FROM inventory_item WHERE id = {inventory_id}"
            ))
            .await
            .ok()
            .flatten();
        let Some(inventory) = inventory else { continue };
        let definition = state
            .db
            .query_one::<ItemDefinition>(&format!(
                "SELECT * FROM item WHERE id = {}",
                sql_string_literal(&inventory.item_id)
            ))
            .await
            .ok()
            .flatten();
        if let Some(item) = definition {
            hands.push(EquippedCombatItem {
                melee: item.kind == ItemKind::Weapon && item.melee,
                ranged: item.kind == ItemKind::Weapon && item.ranged,
                shield: item.kind == ItemKind::Shield,
                balance: item.balance,
            });
        }
    }
    CombatTrainingProfile::from_equipped_hands(hands)
}

pub(crate) async fn get_active_party_members(
    state: &AppState,
    active_character: Option<&Character>,
) -> Vec<Character> {
    let Some(party_id) = active_character.and_then(|character| character.party_id.as_ref()) else {
        return Vec::new();
    };
    let memberships_sql = format!(
        "SELECT * FROM party_member WHERE party_id = {}",
        sql_string_literal(party_id)
    );
    let party_sql = format!(
        "SELECT * FROM party WHERE id = {}",
        sql_string_literal(party_id)
    );
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

pub(crate) async fn soap_rest_preview(
    state: &AppState,
    members: &[Character],
    party_id: Option<&str>,
) -> SoapRestPreview {
    let (filth, personal, shared) = tokio::join!(
        state
            .db
            .query::<CharacterFilth>("SELECT * FROM character_filth"),
        state
            .db
            .query::<InventoryItem>("SELECT * FROM inventory_item"),
        state
            .db
            .query::<PartyInventoryItem>("SELECT * FROM party_inventory_item"),
    );
    calculate_soap_rest_preview(
        members,
        &filth.unwrap_or_default(),
        &personal.unwrap_or_default(),
        &shared.unwrap_or_default(),
        party_id,
    )
}

fn calculate_soap_rest_preview(
    members: &[Character],
    filth: &[CharacterFilth],
    personal: &[InventoryItem],
    shared: &[PartyInventoryItem],
    party_id: Option<&str>,
) -> SoapRestPreview {
    const SOAP_ITEM_ID: &str = "soft_soap";
    let mut personal_units = 0_u32;
    let mut need_after_personal = 0_u32;
    for member in members.iter().filter(|member| member.alive) {
        let amount = filth
            .iter()
            .filter(|deposit| deposit.character_id == member.id)
            .map(|deposit| u32::from(deposit.amount))
            .sum::<u32>();
        let needed = amount.div_ceil(u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY));
        let available = personal
            .iter()
            .filter(|stack| stack.character_id == member.id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| stack.qty)
            .sum::<u32>();
        let used = needed.min(available);
        personal_units = personal_units.saturating_add(used);
        need_after_personal = need_after_personal.saturating_add(needed.saturating_sub(used));
    }
    let shared_available = party_id.map_or(0, |party_id| {
        shared
            .iter()
            .filter(|stack| stack.party_id == party_id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| stack.quantity)
            .sum()
    });
    let shared_units = need_after_personal.min(shared_available);
    SoapRestPreview {
        total_units: personal_units.saturating_add(shared_units),
        personal_units,
        shared_units,
    }
}

#[cfg(test)]
mod rest_form_tests {
    use adventuresim_core::strategic_time::{is_walking_time, minutes_until_next_walking_start};

    use super::{
        RestForm, calculate_soap_rest_preview, settlement_rest_minutes, travel_rest_minutes,
    };
    use crate::spacetimedb::{
        Character, CharacterFilth, FilthOrigin, FilthSubstance, InventoryItem, PartyInventoryItem,
    };

    fn form(duration: &str, unit: &str, requested_minutes: Option<u64>) -> RestForm {
        RestForm {
            duration: duration.into(),
            unit: unit.into(),
            requested_minutes,
        }
    }

    fn member(id: u64) -> Character {
        Character {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_quest_location_id: None,
            party_id: Some("party".into()),
            age_years: 30,
            alive: true,
            temporary: false,
        }
    }

    #[test]
    fn soap_preview_exactly_splits_personal_and_shared_units() {
        let filth = [
            CharacterFilth {
                id: 1,
                character_id: 1,
                substance: FilthSubstance::Dirt,
                origin: FilthOrigin::Unknown,
                amount: 26,
                deposited_at: 0,
            },
            CharacterFilth {
                id: 2,
                character_id: 2,
                substance: FilthSubstance::Blood,
                origin: FilthOrigin::Foreign,
                amount: 30,
                deposited_at: 0,
            },
        ];
        let personal = [InventoryItem {
            id: 1,
            character_id: 1,
            item_id: "soft_soap".into(),
            qty: 1,
        }];
        let shared = [PartyInventoryItem {
            id: 1,
            party_id: "party".into(),
            item_id: "soft_soap".into(),
            quantity: 2,
        }];
        let preview = calculate_soap_rest_preview(
            &[member(1), member(2)],
            &filth,
            &personal,
            &shared,
            Some("party"),
        );
        assert_eq!(preview.personal_units, 1);
        assert_eq!(preview.shared_units, 2);
        assert_eq!(preview.total_units, 3);
    }

    #[test]
    fn exact_hours_preserve_minutes_and_enforce_one_day() {
        assert_eq!(
            settlement_rest_minutes(&form("24:01", "hours", Some(1_441))),
            Ok(1_441)
        );
        assert!(settlement_rest_minutes(&form("23:59", "hours", Some(1_439))).is_err());
    }

    #[test]
    fn field_rest_accepts_sub_day_wake_times() {
        assert_eq!(
            travel_rest_minutes(&form("01:30", "hours", Some(90))),
            Ok(90)
        );
        assert!(travel_rest_minutes(&form("00:00", "hours", Some(0))).is_err());
    }

    #[test]
    fn hours_fallback_parses_hh_mm() {
        assert_eq!(
            settlement_rest_minutes(&form("24:31", "hours", None)),
            Ok(1_471),
        );
        assert!(settlement_rest_minutes(&form("24:60", "hours", None)).is_err());
        assert!(settlement_rest_minutes(&form("24.5", "hours", None)).is_err());
    }

    #[test]
    fn days_are_independent_whole_days_with_a_minimum_of_one() {
        assert_eq!(
            settlement_rest_minutes(&form("2", "days", Some(1_441))),
            Ok(2_880)
        );
        assert!(settlement_rest_minutes(&form("0", "days", None)).is_err());
        assert!(settlement_rest_minutes(&form("1.5", "days", None)).is_err());
    }

    #[test]
    fn days_form_omits_disabled_exact_minutes_and_hours_reject_contradictions() {
        let parsed: RestForm =
            serde_urlencoded::from_str("duration=2&unit=days").expect("days form parses");
        assert_eq!(parsed.requested_minutes, None);
        assert_eq!(settlement_rest_minutes(&parsed), Ok(2_880));
        let blank: RestForm = serde_urlencoded::from_str("duration=2&unit=days&requested_minutes=")
            .expect("blank disabled-field fallback parses");
        assert_eq!(blank.requested_minutes, None);
        assert_eq!(settlement_rest_minutes(&blank), Ok(2_880));
        assert!(settlement_rest_minutes(&form("24:00", "hours", Some(1_441))).is_err());
    }

    #[test]
    fn camp_wake_defaults_follow_the_absolute_daily_schedule() {
        assert_eq!(
            minutes_until_next_walking_start(60, 8 * 60, true),
            Some(19 * 60)
        );
        assert_eq!(
            minutes_until_next_walking_start(7 * 60, 8 * 60, false),
            Some(60)
        );
        assert!(!is_walking_time(7 * 60, 8 * 60, false));
        assert!(is_walking_time(9 * 60, 8 * 60, false));
        assert_eq!(
            minutes_until_next_walking_start(9 * 60, 8 * 60, false),
            Some(23 * 60)
        );
        assert_eq!(
            minutes_until_next_walking_start(18 * 60, 8 * 60, true),
            Some(2 * 60)
        );
        assert!(is_walking_time(21 * 60, 8 * 60, true));
    }
}

#[cfg(test)]
mod herbalist_tests {
    use super::{HerbalistDiagnosisDto, herbalist_diagnosis_dtos, living_party_members};
    use crate::spacetimedb::{Character, HerbalistExaminationRow};

    fn member(id: u64, alive: bool) -> Character {
        Character {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_quest_location_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
        }
    }

    #[test]
    fn travel_forecasts_only_include_living_party_members() {
        let members = [member(1, true), member(2, false), member(3, true)];

        let living = living_party_members(&members);

        assert_eq!(
            living.iter().map(|member| member.id).collect::<Vec<_>>(),
            [1, 3]
        );
    }

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

#[cfg(test)]
mod encumbrance_tests {
    use super::{
        ENCUMBRANCE_QUERY_CONCURRENCY, EncumbranceRows, encumbrance_query_ids, party_encumbrance,
        personal_encumbrance,
    };
    use crate::spacetimedb::{
        Character, CharacterAttributes, CharacterCondition, CharacterLimbs, CharacterNeeds,
        InventoryItem, ItemDefinition, PartyInventoryItem,
    };
    use serde_json::json;

    fn item(id: &str, weight: f32) -> ItemDefinition {
        serde_json::from_value(json!({
            "id": id,
            "weight": weight,
            "kind": "Weapon"
        }))
        .unwrap()
    }

    fn character(id: u64, alive: bool) -> Character {
        Character {
            id,
            name: format!("Character {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_quest_location_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
        }
    }

    fn rows() -> EncumbranceRows {
        EncumbranceRows {
            attributes: vec![CharacterAttributes {
                character_id: 1,
                endurance: 0.0,
                immunity: 0.0,
                gut: 0.0,
                precision: 0.0,
                intelligence: 0.0,
                instinct: 0.0,
                eyesight: 0.0,
                hearing: 0.0,
                left_arm_strength: 0.0,
                right_arm_strength: 0.0,
                left_leg_strength: 4.0,
                right_leg_strength: 2.0,
                left_arm_agility: 0.0,
                right_arm_agility: 0.0,
                left_leg_agility: 0.0,
                right_leg_agility: 0.0,
            }],
            limbs: vec![CharacterLimbs {
                character_id: 1,
                left_arm_health: 1.0,
                right_arm_health: 1.0,
                left_leg_health: 0.5,
                right_leg_health: 1.0,
                head_health: 1.0,
                chest_health: 1.0,
                stomach_health: 1.0,
            }],
            conditions: vec![
                CharacterCondition {
                    character_id: 1,
                    body_weight_kg: 70.0,
                    current_blood_ml: 5_000.0,
                    maximum_blood_ml: 5_000.0,
                    religion_id: None,
                },
                CharacterCondition {
                    character_id: 2,
                    body_weight_kg: 90.0,
                    current_blood_ml: 5_000.0,
                    maximum_blood_ml: 5_000.0,
                    religion_id: None,
                },
            ],
            needs: vec![CharacterNeeds {
                character_id: 1,
                food_balance_kcal: 0.0,
                water_balance_ml: 0.0,
                carried_water_ml: 2_500.0,
            }],
        }
    }

    #[test]
    fn personal_summary_counts_body_water_quantity_and_injury_adjusted_capacity() {
        let inventory = vec![InventoryItem {
            id: 10,
            character_id: 1,
            item_id: "sword".into(),
            qty: 3,
        }];
        let summary = personal_encumbrance(1, &inventory, &[item("sword", 4.0)], &rows());
        assert_eq!(summary.burden_kg, 84.5);
        assert_eq!(summary.capacity_kg, 300.0);
    }

    #[test]
    fn party_summary_excludes_dead_members_and_adds_shared_pool_once() {
        let inventories = vec![
            InventoryItem {
                id: 10,
                character_id: 1,
                item_id: "sword".into(),
                qty: 3,
            },
            InventoryItem {
                id: 11,
                character_id: 2,
                item_id: "sword".into(),
                qty: 20,
            },
        ];
        let pooled = vec![PartyInventoryItem {
            id: 20,
            party_id: "party".into(),
            item_id: "sword".into(),
            quantity: 2,
        }];
        let summary = party_encumbrance(
            &[character(1, true), character(2, false)],
            &inventories,
            &pooled,
            &[item("sword", 4.0)],
            &rows(),
        );
        assert_eq!(summary.burden_kg, 92.5);
        assert_eq!(summary.capacity_kg, 300.0);
    }

    #[test]
    fn query_ids_are_living_only_deduplicated_and_keep_active_personal_rows() {
        let (inventory_ids, row_ids) = encumbrance_query_ids(
            &[character(1, true), character(1, true), character(2, false)],
            2,
        );
        assert_eq!(inventory_ids, vec![1]);
        assert_eq!(row_ids, vec![1, 2]);
        assert_eq!(ENCUMBRANCE_QUERY_CONCURRENCY, 4);
    }

    #[test]
    fn missing_rows_and_item_definitions_fail_closed_without_nan() {
        let summary = personal_encumbrance(
            99,
            &[InventoryItem {
                id: 30,
                character_id: 99,
                item_id: "unknown".into(),
                qty: 4,
            }],
            &[],
            &EncumbranceRows::default(),
        );
        assert_eq!(summary.burden_kg, 0.0);
        assert_eq!(summary.capacity_kg, 0.0);
        assert_eq!(summary.penalty_fraction(), 1.0);
    }
}
