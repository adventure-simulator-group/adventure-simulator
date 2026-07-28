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
    extract::{Path, Query, State, rejection::FormRejection},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use futures_util::{
    future::join_all,
    stream::{self, StreamExt},
};
use maud::{Markup, html};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

const BUILDINGS: &[&str] = &[
    "public-square",
    "residences",
    "keep",
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
    cook: Option<bool>,
    forage: Option<bool>,
    forage_receipt: Option<String>,
    forage_error: Option<String>,
    social_feedback: Option<String>,
}

#[derive(Deserialize)]
struct MerchantProviderRow {
    id: String,
    home_settlement_id: String,
    service_id: String,
}

#[derive(Deserialize)]
struct MerchantProviderPresenceRow {
    npc_id: String,
    settlement_id: String,
    location_id: String,
    is_default: bool,
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
            |building| {
                format!(
                    "{path}{}building={building}",
                    if path.contains('?') { "&" } else { "?" }
                )
            },
        )
    }

    fn cooking(&self) -> bool {
        self.cook == Some(true)
    }
}

#[cfg(test)]
mod building_query_tests {
    use super::{BuildingQuery, merchant_service_location};

    #[test]
    fn building_query_is_closed_and_preserved_on_redirects() {
        let valid = BuildingQuery {
            building: Some("inn".into()),
            ..Default::default()
        };
        assert_eq!(valid.valid(), Some("inn"));
        assert_eq!(
            valid.append_to("/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1?building=inn"
        );
        assert_eq!(
            valid.append_to("/locations/settlement/x/party/1?cook=true".into()),
            "/locations/settlement/x/party/1?cook=true&building=inn"
        );
        let non_service = BuildingQuery {
            building: Some("public-square".into()),
            ..Default::default()
        };
        assert_eq!(
            non_service.append_to("/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1?building=public-square"
        );
        let invalid = BuildingQuery {
            building: Some("../religion".into()),
            ..Default::default()
        };
        assert_eq!(invalid.valid(), None);
        assert_eq!(
            invalid.append_to("/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1"
        );
    }

    #[test]
    fn merchant_offer_routes_accept_only_bound_storefront_services() {
        let source = include_str!("settlements.rs");
        assert!(source.contains("\"/settlements/{id}/storefront/{service_id}/offer\""));
        assert!(!source.contains("\"/settlements/{id}/{service_id}/offer\""));
        assert_eq!(merchant_service_location("merchants"), Some("market"));
        assert_eq!(merchant_service_location("weapons"), Some("forge"));
        assert_eq!(merchant_service_location("armor"), Some("armoury"));
        assert_eq!(merchant_service_location("clothing"), Some("tailor"));
        assert_eq!(merchant_service_location("inn"), Some("inn"));
        assert_eq!(merchant_service_location("herbalist"), None);
        assert_eq!(merchant_service_location("../inn"), None);
    }

    #[test]
    fn settlement_entry_activates_activity_without_a_local_server_bypass() {
        let source = include_str!("settlements.rs").replace('\r', "");
        let entry = source
            .rsplit("async fn show_settlement_location")
            .next()
            .and_then(|tail| tail.split("async fn settlement_map").next())
            .expect("settlement entry route");
        assert!(entry.contains(".call("));
        assert!(entry.contains("\"ensure_settlement_activity\""));
        assert!(!entry.contains("is_local()"));

        let offers = source
            .split("async fn service_quest_offers")
            .nth(1)
            .and_then(|tail| tail.split("fn service_quest_greeting").next())
            .expect("service quest offers route");
        assert!(!offers.contains("ensure_settlement_activity"));
    }
}

use super::AppState;
use super::inventory_forms::{
    DiscardInventoryForm, MerchantOfferForm, PartyOfferForm, PartyPoolTransferForm,
};
use super::redirect_to_local;
use super::travel::{
    CaseSiteKnowledgePresentation, TravelDestination, TravelForm, TravelProvisionForecast,
    active_contract_tooltip, connected_destinations, populate_itinerary_forecasts,
};
use crate::session::Session;
use crate::spacetimedb::sql_string_literal;
use crate::spacetimedb::{
    AlcoholConsumption, AutomaticSocialChat, BackendCaseSitePin, BackendLocalProblemTradeEffect,
    BackendPhysiologyAdministration, BackendPhysiologyChart, Character, CharacterAffinity,
    CharacterAttributes, CharacterCapability, CharacterCondition, CharacterEquip,
    CharacterFamiliarity, CharacterFilth, CharacterLimbs, CharacterMoraleSource, CharacterNeeds,
    CharacterNotoriety, CharacterPersonality, CharacterSkills, CharacterStats,
    CharacterStrategicCondition, CharacterTime, CharacterTrainingSchedule, CharacterVirtue,
    ContractPresentation, ContractPresentationStatus, FoodLot, InventoryItem, InventoryItemAmount,
    InventoryQuantityTarget, ItemCondition, ItemDefinition, ItemKind, ItemSlot, LimbInjury,
    LimbRegion, Party, PartyInventoryItem, PartyItemAmount, PartyJourney, PartyJourneyItinerary,
    PartyJourneyRoute, PartyMember, PartyRecruitmentRole, PartyStake, RecruitmentOffer,
    RecruitmentOfferStatus, RecruitmentRequirements, ReligiousDemand, RepairOrder,
    RetainedProjectile, ScheduleAllocation, Settlement, SettlementAlias, SettlementDescription,
    SettlementSmith, SocialAddress, SocialBelief, StrategicEncounter, TravelEdge,
};
use crate::templates::settlement::{
    ActivityPreviewRates, CampTravelDestination, LocationKind, LocationView, MerchantShop,
    RestSummary, SoapRestPreview, SocialPresentation, camp_page, live_merchant_shop_page,
    merchants_page, party_discard_page, party_inventory_page, party_personal_page, party_pool_page,
    party_social_dialog, party_stats_page, religion_page, rest_default_minutes, rest_result_page,
    settlement_map_page, settlement_npc_location_page, settlement_overview_page, surgery_dialog,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settlements/{id}", get(show_settlement))
        .route(
            "/settlements/{id}/places/{place}",
            get(settlement_npc_place),
        )
        .route("/locations/settlement/{id}", get(show_settlement_location))
        .route("/locations/settlement/{id}/map", get(settlement_map))
        .route("/locations/settlement/{id}/alchemy", get(alchemy))
        .route(
            "/locations/settlement/{id}/map/travel-configuration",
            post(update_travel_configuration),
        )
        .route(
            "/locations/settlement/{id}/map/rest",
            post(rest_at_settlement_map),
        )
        .route(
            "/locations/case-site/{id}/map/travel-configuration",
            post(update_travel_configuration),
        )
        .route("/camp", get(camp))
        .route("/camp/rest", post(rest_at_camp))
        .route(
            "/camp/travel-configuration",
            post(update_camp_travel_configuration),
        )
        .route("/camp/continue", post(continue_camp_travel))
        .route("/camp/encounter", post(resolve_camp_encounter))
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
            "/locations/{kind}/{id}/party/{character_id}",
            get(party_personal),
        )
        .route(
            "/locations/settlement/{id}/party/{character_id}/organizations",
            get(character_organizations),
        )
        .route(
            "/locations/settlement/{id}/party/{character_id}/organizations/{organization_id}/{action}",
            post(update_character_organization),
        )
        .route(
            "/locations/settlement/{id}/party/{character_id}/organizations-none",
            post(clear_presented_organization),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/cook",
            post(cook_food),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/physiology/administer",
            post(administer_preparation),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/physiology/{administration_id}/stop",
            post(stop_preparation),
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
            "/locations/{kind}/{id}/party/{character_id}/social",
            get(party_social).post(perform_social_action),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/social/chat",
            post(chat_with_party_member),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/social/automatic",
            post(set_automatic_social_chat),
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
            "/locations/{kind}/{id}/players/{character_id}",
            get(party_stats),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/schedule",
            post(update_training_schedule),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/activity",
            post(perform_immediate_activity),
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
            "/settlements/{id}/storefront/{service_id}/offer",
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
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let Some(actor_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    let Some(selected_limb) = parse_surgery_limb(&limb) else {
        return Html("<h1>Limb not found</h1>".into());
    };
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".into()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".into());
        }
    };
    location.active_building = building.valid().map(str::to_owned);
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
    let procedure_checks = get_character_capability(&state, actor_id)
        .await
        .map_or([0.0; 3], |capability| {
            [capability.anatomy, capability.knife, capability.tailoring]
        });
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
    let dialog = surgery_dialog(
        &location,
        &active,
        &patient,
        &injuries,
        &projectiles,
        selected_limb,
        quantity("bandage"),
        quantity("surgery_kit"),
        available_splints,
        quantity("soft_soap"),
        alcohol_count,
        selected_alcohol,
        procedure_checks,
    );
    if patient_id == active.id {
        render_party_personal(
            &state,
            &kind,
            &id,
            patient_id,
            building,
            &session,
            Some(dialog),
            Some(&limb),
            false,
        )
        .await
    } else {
        render_party_stats(
            &state,
            &kind,
            &id,
            patient_id,
            building,
            &session,
            Some(dialog),
            Some(&limb),
            false,
        )
        .await
    }
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
    value["apprenticeship_organization_id"] =
        spacetime_option_string(schedule.apprenticeship_organization_id.as_deref());
    value["practice_organization_id"] =
        spacetime_option_string(schedule.practice_organization_id.as_deref());
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
            apprenticeship_organization_id: Some("armourers_guild".into()),
            practice_organization_id: None,
            ..Default::default()
        });
        assert_eq!(
            encoded["apprenticeship_organization_id"],
            json!({ "some": "armourers_guild" })
        );
        assert_eq!(encoded["practice_organization_id"], json!({ "none": [] }));
    }
}

async fn perform_surgery(
    State(state): State<AppState>,
    Path((kind, id, patient_id, limb)): Path<(String, String, u64, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<SurgeryProcedureForm>,
) -> Redirect {
    let destination = format!("/locations/{kind}/{id}/party/{patient_id}/surgery/{limb}");
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to(&building.append_to(destination));
    };
    if parse_surgery_limb(&limb).is_none() {
        return Redirect::to(&building.append_to(destination));
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
    Redirect::to(&building.append_to(destination))
}

async fn alchemy(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Response {
    if session.character_id_u64().is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Html(crate::templates::strategic_notice_page(
        "Alchemy is not yet modelled",
        "Physiology observes patients and administers existing preparations; it does not craft them. Herbalism issue #214 owns preparations, and chemistry issue #215 owns Alchemy.",
        &format!("/locations/settlement/{id}"),
        "Return to the settlement",
        None,
    ).into_string())
    .into_response()
}
#[derive(Deserialize)]
struct RepairItemForm {
    inventory_item_id: u64,
}

fn repair_service(shop: &str) -> Option<&'static str> {
    match shop {
        "weapons" => Some("weapons"),
        "armor" => Some("armor"),
        "clothing" => Some("clothing"),
        _ => None,
    }
}

#[cfg(test)]
mod repair_route_tests {
    use super::repair_service;

    #[test]
    fn repair_routes_dispatch_all_and_only_the_three_authoritative_services() {
        assert_eq!(repair_service("weapons"), Some("weapons"));
        assert_eq!(repair_service("armor"), Some("armor"));
        assert_eq!(repair_service("clothing"), Some("clothing"));
        assert_eq!(repair_service("merchants"), None);
        assert_eq!(repair_service("smith"), None);
    }
}

async fn submit_repair(
    State(state): State<AppState>,
    Path((id, shop)): Path<(String, String)>,
    session: Session,
    Form(form): Form<RepairItemForm>,
) -> Redirect {
    if let Some(service) = repair_service(&shop) {
        if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
        {
            let _ = state
                .db
                .call(
                    "submit_item_for_repair",
                    &[
                        json!(character.id),
                        json!(id),
                        json!(service),
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
    if let Some(service) = repair_service(&shop) {
        if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
        {
            let _ = state
                .db
                .call(
                    "submit_all_repairable_items",
                    &[json!(character.id), json!(id), json!(service)],
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
    if repair_service(&shop).is_some()
        && let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    {
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
    if let Some(service) = repair_service(&shop)
        && let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    {
        let _ = state
            .db
            .call(
                "retrieve_repaired_items",
                &[
                    json!(character.id),
                    json!(id),
                    json!(service),
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

async fn settlement_npc_place(
    State(state): State<AppState>,
    Path((id, place)): Path<(String, String)>,
    session: Session,
) -> Html<String> {
    if !matches!(place.as_str(), "overview" | "residences" | "keep") {
        return Html("<h1>Settlement place not found</h1>".into());
    }
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .ok()
        .flatten();
    let Some(settlement) = settlement else {
        return Html("<h1>Settlement not found</h1>".into());
    };
    let active = get_active_character(&state, session.character_id_u64()).await;
    let Some((character, _)) = active.as_ref() else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return Html("<h1>You are not in this settlement</h1>".into());
    }
    if place == "keep"
        && !matches!(
            settlement.category,
            crate::spacetimedb::SettlementCategory::Town
                | crate::spacetimedb::SettlementCategory::City
                | crate::spacetimedb::SettlementCategory::Capital
        )
    {
        return Html("<h1>This settlement has no keep</h1>".into());
    }
    let party_members = get_active_party_members(&state, Some(character)).await;
    Html(
        settlement_npc_location_page(
            &settlement,
            character,
            &party_members,
            &place,
            Some(&character.name),
        )
        .into_string(),
    )
}

async fn show_settlement_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement_literal = sql_string_literal(&id);
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {settlement_literal}"
        ))
        .await;
    let settlement = match settlement {
        Ok(Some(settlement)) => settlement,
        Ok(None) => return Html("<h1>Settlement not found</h1>".to_string()),
        Err(error) => {
            tracing::error!(%error, settlement_id = %id, "failed to load settlement");
            return Html("<h1>Settlement data unavailable</h1>".to_string());
        }
    };
    if let Err(error) = state
        .db
        .call("ensure_settlement_activity", &[json!(id.clone())])
        .await
    {
        tracing::warn!(%error, settlement_id = %id, "failed to activate settlement activity");
    }
    let alias_sql =
        format!("SELECT * FROM settlement_alias WHERE settlement_id = {settlement_literal}");
    let description_sql =
        format!("SELECT * FROM settlement_description WHERE settlement_id = {settlement_literal}");
    let (aliases, descriptions, active_character) = tokio::join!(
        state.db.query::<SettlementAlias>(&alias_sql),
        state.db.query::<SettlementDescription>(&description_sql),
        get_active_character(&state, session.character_id_u64()),
    );
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
            &settlement,
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
    let quests: Vec<ContractPresentation> = state
        .db
        .query("SELECT * FROM backend_contracts")
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
    let active_contract_id = active_party
        .as_ref()
        .and_then(|party| party.active_contract_id.as_deref());
    let active_contract =
        active_contract_id.and_then(|id| quests.iter().find(|contract| contract.id == id));
    let case_sites = if let Some(character_id) = session.character_id_u64() {
        state
            .db
            .query::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let is_current_settlement = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(&settlement.id)
    });
    // The interactive map bundle is optional presentation. Exact observer-owned
    // case sites must remain selectable and travelable through the HTML fallback.
    let can_travel =
        settlement_html_travel_available(is_current_settlement, active_party.is_some());
    if can_travel {
        for site in &case_sites {
            let distance_m = crate::routes::quests::straight_line_distance_m(site, settlement);
            destinations.push(TravelDestination {
                id: site.case_site_id.clone(),
                name: site.name.clone(),
                description: site.description.clone(),
                summary: CaseSiteKnowledgePresentation::from_stage(&site.knowledge_stage)
                    .map(|knowledge| knowledge.label().to_string()),
                travel_action: format!("/case-sites/{}/travel", site.case_site_id),
                track_action: Some(format!("/case-sites/{}/track", site.case_site_id)),
                tracked: site.tracked,
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
                round_trip_destination: true,
                case_site_knowledge: CaseSiteKnowledgePresentation::from_stage(
                    &site.knowledge_stage,
                ),
                active_contract_destination: case_site_has_active_contract(
                    &site.case_id,
                    active_contract,
                ),
                provision_forecast: None,
                terrain_route: None,
                return_terrain_route: None,
                route_fallback: true,
            });
        }
    }
    if let Some(selected_id) = query.destination.as_deref()
        && let Some(destination) = destinations
            .iter_mut()
            .find(|destination| destination.id == selected_id)
    {
        let goal = if let Some(site) = case_sites
            .iter()
            .find(|site| site.case_site_id == destination.id)
        {
            Some((
                f64::from(site.latitude_e7) / 10_000_000.0,
                f64::from(site.longitude_e7) / 10_000_000.0,
            ))
        } else {
            settlements
                .iter()
                .find(|candidate| candidate.id == destination.id)
                .map(|candidate| (candidate.coord_y, candidate.coord_x))
        };
        if let Some(goal) = goal {
            let terrain_profile = if let Some((character, _)) = active_character.as_ref() {
                crate::routes::party_terrain_profile(&state, character)
                    .await
                    .unwrap_or_default()
            } else {
                adventuresim_terrain::TerrainSkillProfile::default()
            };
            crate::routes::travel::apply_terrain_route(
                destination,
                state.terrain.as_deref(),
                (settlement.coord_y, settlement.coord_x),
                goal,
                terrain_profile,
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
    let provisioning_path = if can_travel {
        provisioning_storefront_path(&state, settlement).await
    } else {
        None
    };
    Html(
        settlement_map_page(
            settlement,
            &settlements,
            &case_sites,
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
            provisioning_path.as_deref(),
            is_current_settlement,
            active_contract.filter(|contract| {
                can_abandon_active_contract(
                    contract,
                    active_character
                        .as_ref()
                        .and_then(|(character, _)| character.current_case_site_id.as_deref()),
                )
            }),
            active_character
                .as_ref()
                .map(|(character, _)| character.name.as_str()),
        )
        .into_string(),
    )
}

fn settlement_html_travel_available(is_current_settlement: bool, has_party: bool) -> bool {
    is_current_settlement && has_party
}

fn case_site_has_active_contract(
    case_id: &str,
    active_contract: Option<&ContractPresentation>,
) -> bool {
    active_contract.is_some_and(|contract| contract.case_id == case_id)
}

fn can_abandon_active_contract(
    contract: &ContractPresentation,
    current_case_site_id: Option<&str>,
) -> bool {
    contract.status == ContractPresentationStatus::Accepted && current_case_site_id.is_none()
}

#[cfg(test)]
mod map_quest_tests {
    use super::*;

    #[test]
    fn exact_owned_case_sites_use_the_current_settlement_as_the_map_origin() {
        let source = include_str!("settlements.rs");
        let map = source
            .split("async fn settlement_map(")
            .nth(1)
            .and_then(|tail| tail.split("fn settlement_html_travel_available").next())
            .expect("settlement map route");

        assert!(map.contains("for site in &case_sites"));
        assert!(map.contains("straight_line_distance_m(site, settlement)"));
        assert!(!map.contains("site.origin_settlement_id == settlement.id"));
    }

    #[test]
    fn html_case_site_travel_does_not_depend_on_optional_map_data() {
        assert!(settlement_html_travel_available(true, true));
        assert!(!settlement_html_travel_available(false, true));
        assert!(!settlement_html_travel_available(true, false));
    }

    fn quest(status: ContractPresentationStatus) -> ContractPresentation {
        ContractPresentation {
            id: "active".into(),
            case_id: "case:active".into(),
            title: "Active quest".into(),
            description: String::new(),
            difficulty: 1,
            gold_reward: 1,
            xp_reward: 1,
            settlement_id: "issuer".into(),
            service_id: "inn".into(),
            issuer_npc_id: String::new(),
            status,
            accepted_by: Some("party".into()),
            opposition_wording: "unknown opposition".into(),
            opposition_count_wording: "an unknown number of".into(),
        }
    }

    #[test]
    fn accepted_active_quest_can_only_be_abandoned_before_reaching_its_location() {
        assert!(can_abandon_active_contract(
            &quest(ContractPresentationStatus::Accepted),
            None
        ));
        assert!(!can_abandon_active_contract(
            &quest(ContractPresentationStatus::Accepted),
            Some("active")
        ));
        assert!(!can_abandon_active_contract(
            &quest(ContractPresentationStatus::ReadyToReport),
            None
        ));
    }

    #[test]
    fn case_site_badge_requires_an_explicit_active_contract_case_match() {
        let active = quest(ContractPresentationStatus::Accepted);

        assert!(case_site_has_active_contract("case:active", Some(&active)));
        assert!(!case_site_has_active_contract(
            "case:reported-decoy",
            Some(&active)
        ));
        assert!(!case_site_has_active_contract("case:active", None));
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

#[derive(Default, Deserialize)]
struct CampQuery {
    forage: Option<bool>,
    forage_receipt: Option<String>,
    forage_error: Option<String>,
}

async fn camp(
    State(state): State<AppState>,
    Query(query): Query<CampQuery>,
    session: Session,
) -> Response {
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
            .is_some_and(|party| party.camp_destination.is_some())
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
    if camp_entry_redirect(true, party.camp_destination.is_some()).is_some() {
        return Redirect::to("/").into_response();
    }
    let Some(destination) = party.camp_destination.as_ref() else {
        return Redirect::to("/").into_response();
    };
    let destination_name = destination.name().to_string();
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
        legacy.total_elapsed_minutes = if legacy.destination.case_site_id().is_some() {
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
    let encounter = match state
        .db
        .query_one::<StrategicEncounter>(&format!(
            "SELECT * FROM strategic_encounter WHERE party_id = {}",
            sql_string_literal(&party.id)
        ))
        .await
    {
        Ok(encounter) => encounter,
        Err(error) => {
            tracing::warn!(
                %error,
                party_id = %party.id,
                "camp encounter state unavailable; refusing to render travel controls"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Encounter details are temporarily unavailable. Reload camp before continuing travel.",
            )
                .into_response();
        }
    };
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
    let continue_block_reason = camp_continue_block_reason(
        encounter
            .as_ref()
            .map(|encounter| encounter.status.as_str()),
        is_walking_time(
            current_party_minute,
            party.walking_minutes_per_day,
            party.travel_at_night,
        ),
    );
    let remaining_journey_minutes = journey
        .as_ref()
        .map_or(party.camp_remaining_minutes, |row| {
            row.total_elapsed_minutes
                .saturating_sub(row.completed_elapsed_minutes)
        });
    let remaining_rest_intervals: Vec<_> = journey
        .as_ref()
        .zip(itinerary.as_ref())
        .into_iter()
        .flat_map(|(journey, itinerary)| {
            let remaining_start = journey.completed_elapsed_minutes;
            let remaining_end = journey.total_elapsed_minutes;
            itinerary
                .forecast_camp_intervals
                .iter()
                .filter_map(move |camp| {
                    let camp_start = camp.elapsed_start_minute.max(remaining_start);
                    let camp_end = camp
                        .elapsed_start_minute
                        .saturating_add(camp.elapsed_minutes)
                        .min(remaining_end);
                    (camp_end > camp_start).then(|| {
                        (
                            journey.departure_minute.saturating_add(camp_start),
                            camp_end - camp_start,
                        )
                    })
                })
        })
        .collect();
    let provision_forecast = travel_provision_forecast_for_minutes(
        &state,
        Some(&party),
        &party_members,
        remaining_journey_minutes,
        &remaining_rest_intervals,
        false,
    )
    .await
    .ok()
    .flatten();
    let camp_destinations = camp_settlement_destinations(&state, &party, journey.as_ref()).await;
    let soap_preview = soap_rest_preview(&state, &party_members, Some(&party.id)).await;
    let foraging_dialog = if query.forage.unwrap_or(false) {
        Some(
            crate::routes::foraging::activity_dialog(
                &state,
                &character,
                "/camp",
                query.forage_receipt.as_deref(),
                query.forage_error.as_deref(),
            )
            .await,
        )
    } else {
        None
    };
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
            continue_block_reason,
            encounter.as_ref(),
            foraging_dialog,
            Some(&character.name),
        )
        .into_string(),
    )
    .into_response()
}

fn camp_continue_block_reason(
    encounter_status: Option<&str>,
    is_walking_time: bool,
) -> Option<&'static str> {
    if encounter_status == Some("awaiting_choice") {
        Some("Resolve the encounter above before continuing travel.")
    } else if !is_walking_time {
        Some("Rest until the planned walking window begins.")
    } else {
        None
    }
}

#[derive(Debug, Deserialize)]
struct EncounterChoiceForm {
    choice: String,
}

async fn resolve_camp_encounter(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<EncounterChoiceForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    match state
        .db
        .call(
            "resolve_strategic_encounter",
            &[json!(character_id), json!(form.choice)],
        )
        .await
    {
        Ok(()) => Redirect::to("/camp").into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
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
    if let Some(origin_id) = journey.origin.settlement_id()
        && journey.completed_minutes > 0
    {
        endpoints.push((origin_id, journey.completed_minutes));
    }
    if let Some(destination_id) = journey.destination.settlement_id() {
        endpoints.push((
            destination_id,
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
                current: party
                    .camp_destination
                    .as_ref()
                    .and_then(|destination| destination.settlement_id())
                    == Some(id),
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
    let rest_intervals: Vec<_> = destination
        .itinerary_segments
        .iter()
        .filter(|segment| {
            segment.kind == adventuresim_core::strategic_time::ItinerarySegmentKind::Camp
        })
        .map(|segment| {
            (
                destination
                    .departure_minute
                    .saturating_add(segment.elapsed_start),
                segment.elapsed_minutes,
            )
        })
        .collect();
    travel_provision_forecast_for_minutes(
        state,
        party,
        travelers,
        destination.itinerary_total_elapsed_minutes,
        &rest_intervals,
        departing_settlement,
    )
    .await
}

async fn travel_provision_forecast_for_minutes(
    state: &AppState,
    party: Option<&Party>,
    travelers: &[Character],
    planning_minutes: u64,
    rest_intervals: &[(u64, u64)],
    departing_settlement: bool,
) -> Result<Option<TravelProvisionForecast>, String> {
    let mut travelers: Vec<_> = travelers.iter().filter(|traveler| traveler.alive).collect();
    travelers.sort_by_key(|traveler| traveler.id);
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
    let food_lots: Vec<FoodLot> = state
        .db
        .query("SELECT * FROM food_lot")
        .await
        .map_err(|error| error.to_string())?;
    let mut food_reserve_kcal = 0.0;
    let mut food_lot_kcal = 0.0;
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
                    item_id: def.id.clone(),
                    stable_id: entry.id,
                    owner: Some(traveler.id),
                });
            }
        }
        let time = query_single::<CharacterTime>(state, "character_time", traveler.id).await;
        let personality = query_single::<CharacterPersonality>(
            state,
            "backend_character_personalities",
            traveler.id,
        )
        .await;
        if time.is_some() {
            let history = state
                .db
                .query::<AlcoholConsumption>(&format!(
                    "SELECT * FROM alcohol_consumption WHERE character_id = {}",
                    traveler.id
                ))
                .await
                .map_err(|error| error.to_string())?;
            let mut evenings: Vec<_> = rest_intervals
                .iter()
                .map(|(start, minutes)| {
                    adventuresim_core::alcohol::rest_evenings(
                        *start,
                        start.saturating_add(*minutes),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .filter(|evening| {
                    !history
                        .iter()
                        .any(|row| row.evening_id == *evening && row.morale_evaluated)
                })
                .collect();
            evenings.sort_unstable();
            evenings.dedup();
            match personality.map(|p| p.temperance) {
                Some(crate::spacetimedb::Temperance::Temperate) => {}
                Some(crate::spacetimedb::Temperance::Drunkard) => {
                    expected_morale_demands.extend(evenings.into_iter().map(|evening| {
                        (
                            evening,
                            traveler.id,
                            adventuresim_core::alcohol::HEAVY_ETHANOL_ML,
                        )
                    }));
                }
                _ => {
                    let mut heavy_evenings: Vec<u64> = history
                        .iter()
                        .filter(|row| adventuresim_core::alcohol::qualifying_heavy(row.ethanol_ml))
                        .map(|row| row.evening_id)
                        .collect();
                    for evening in evenings {
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
                        expected_morale_demands.push((evening, traveler.id, target));
                    }
                }
            }
        }
        let owned = |item_id: &str| {
            inventory
                .iter()
                .filter(|entry| entry.item_id == item_id)
                .map(|entry| entry.qty)
                .sum::<u32>()
        };
        food_reserve_kcal += needs.food_balance_kcal;
        food_lot_kcal += food_lots
            .iter()
            .filter(|lot| {
                lot.inventory_item_id
                    .is_some_and(|id| inventory.iter().any(|entry| entry.id == id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum::<f32>();
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
                    item_id: def.id.clone(),
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
        food_lot_kcal += food_lots
            .iter()
            .filter(|lot| {
                lot.party_inventory_item_id
                    .is_some_and(|id| pooled.iter().any(|entry| entry.id == id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum::<f32>();
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
    expected_morale_demands.sort_by_key(|(evening, character_id, _)| (*evening, *character_id));
    let ordered_morale_demands: Vec<_> = expected_morale_demands
        .into_iter()
        .map(|(_, character_id, target)| (character_id, target))
        .collect();
    let emergency_alcohol_hydration_ml =
        adventuresim_core::alcohol::hydration_after_expected_drinking(
            alcohol_supplies,
            &ordered_morale_demands,
        );
    let inputs = PartyProvisioningInputs {
        planning_minutes,
        living_members: travelers.len() as u32,
        food_reserve_kcal,
        food_lot_kcal,
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
}

#[derive(Serialize)]
struct ServiceActivityResponse {
    quests: Vec<ServiceQuestOffer>,
    recruitment: Vec<ServiceQuestRecruitment>,
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
    let Some(organization) = adventuresim_core::organization::organizations_for_chapter(&id)
        .find(|organization| organization.service_id.as_deref() == Some(service_id.as_str()))
    else {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "No local organization offers that professional activity.",
        });
    };
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
            "join_organization",
            &[json!(character.id), json!(organization.id)],
        )
        .await
    {
        Ok(()) => Json(ApprenticeshipResult {
            enrolled: true,
            message: "Your membership begins today.",
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

fn organization_requirement_label(
    requirement: &adventuresim_core::organization::Requirement,
) -> String {
    use adventuresim_core::organization::Requirement;
    match requirement {
        Requirement::SkillRating {
            skill,
            minimum,
            leaf,
        } => format!(
            "{}{} {:.1}",
            skill.replace('_', " "),
            leaf.as_ref()
                .map_or(String::new(), |leaf| format!(" ({leaf})")),
            minimum
        ),
        Requirement::ProfessedReligion { religion } => {
            format!("Professes {}", religion.replace('_', " "))
        }
    }
}

async fn character_organizations(
    State(state): State<AppState>,
    Path((id, character_id)): Path<(String, u64)>,
    session: Session,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (StatusCode::FORBIDDEN, "Select this character first").into_response();
    }
    let Some(character) = state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .ok()
        .flatten()
    else {
        return (StatusCode::NOT_FOUND, "Character not found").into_response();
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            "Organizations can only be managed at the current settlement",
        )
            .into_response();
    }
    let memberships: Vec<crate::spacetimedb::OrganizationMembership> = state
        .db
        .query(&format!(
            "SELECT * FROM organization_membership WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let presentation = state
        .db
        .query_one::<crate::spacetimedb::OrganizationPresentation>(&format!(
            "SELECT * FROM organization_presentation WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten();
    let minute = state
        .db
        .query_one::<CharacterTime>(&format!(
            "SELECT * FROM character_time WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten()
        .map_or(0, |row| row.minutes);
    let base = format!("/locations/settlement/{id}/party/{character_id}");
    let markup = html! {
        (maud::DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Organizations — " (character.name) }
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                main class="center-content settlement-main" data-live-region="organizations" {
                    nav { a href=(base) { "← Back to character" } }
                    h1 { "Organizations" }
                    p { "Memberships are independent. Exactly one recognized, dues-current organization may be presented, or none." }
                    section aria-labelledby="membership-heading" {
                        h2 id="membership-heading" { "Memberships" }
                        @if memberships.is_empty() {
                            p { "No memberships." }
                        }
                        @for membership in &memberships {
                            @let definition = adventuresim_core::organization::organization(&membership.organization_id);
                            @let rank = definition.and_then(|definition| definition.rank(&membership.rank_id));
                            article class="card organization-membership" {
                                h3 { (definition.map_or(membership.organization_id.as_str(), |entry| entry.name.as_str())) }
                                p { strong { (rank.map_or(membership.rank_id.as_str(), |rank| rank.name.as_str())) } }
                                @if let Some(rank) = rank { p { (rank.description) } }
                                p {
                                    "Status: " (membership.status)
                                    @if membership.dues_paid_through_minute != u64::MAX {
                                        " · paid through minute " (membership.dues_paid_through_minute)
                                        @if minute > membership.dues_paid_through_minute { " (payment required)" }
                                    } @else { " · no dues" }
                                }
                                @if let Some(definition) = definition {
                                    p { "Privileges: "
                                        @if definition.privileges.is_empty() { "none" }
                                        @for privilege in &definition.privileges { code { (format!("{privilege:?}")) } " " }
                                    }
                                    p { "Recognition: " (match &definition.recognition {
                                        adventuresim_core::organization::Recognition::Universal => "universal".into(),
                                        adventuresim_core::organization::Recognition::Settlements { settlement_ids } => settlement_ids.join(", "),
                                    }) }
                                    @if let Some(next) = definition.next_rank(&membership.rank_id) {
                                        p { "Next rank: " strong { (next.name) } " — " (next.description) }
                                        form method="post" action=(format!("{base}/organizations/{}/promote", definition.id)) {
                                            button type="submit" { "Request promotion" }
                                        }
                                    }
                                    @if definition.dues.is_some() {
                                        form method="post" action=(format!("{base}/organizations/{}/pay", definition.id)) {
                                            button type="submit" { "Pay one dues interval" }
                                        }
                                    }
                                    @if definition.recognition.includes(&id) && membership.status == "active" && minute <= membership.dues_paid_through_minute {
                                        form method="post" action=(format!("{base}/organizations/{}/present", definition.id)) {
                                            button type="submit" aria-pressed=(presentation.as_ref().is_some_and(|row| row.organization_id == definition.id)) {
                                                "Present as " (definition.name)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        form method="post" action=(format!("{base}/organizations-none")) {
                            button type="submit" aria-pressed=(presentation.is_none()) { "Present as none" }
                        }
                    }
                    section aria-labelledby="available-heading" {
                        h2 id="available-heading" { "Available here" }
                        @for definition in adventuresim_core::organization::organizations_for_chapter(&id) {
                            @if !memberships.iter().any(|row| row.organization_id == definition.id) {
                                article class="card organization-available" {
                                    h3 { (definition.name) }
                                    p { (definition.description) }
                                    @if let Some(note) = &definition.historical_fantasy_note {
                                        p class="text-muted" { (note) }
                                    }
                                    p { "Joining fee: " (definition.admission.joining_fee) " coin(s)" }
                                    @if !definition.admission.requirements.is_empty() {
                                        ul {
                                            @for requirement in &definition.admission.requirements {
                                                li { (organization_requirement_label(requirement)) }
                                            }
                                        }
                                    }
                                    form method="post" action=(format!("{base}/organizations/{}/join", definition.id)) {
                                        button type="submit" { "Join" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    Html(markup.into_string()).into_response()
}

async fn update_character_organization(
    State(state): State<AppState>,
    Path((id, character_id, organization_id, action)): Path<(String, u64, String, String)>,
    Query(query): Query<OrganizationActionQuery>,
    session: Session,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (StatusCode::FORBIDDEN, "Select this character first").into_response();
    }
    let reducer = match action.as_str() {
        "join" => "join_organization",
        "promote" => "promote_organization_membership",
        "pay" => "pay_organization_dues",
        "present" => "present_organization",
        _ => return (StatusCode::NOT_FOUND, "Unknown organization action").into_response(),
    };
    match state
        .db
        .call(reducer, &[json!(character_id), json!(organization_id)])
        .await
    {
        Ok(()) => {
            Redirect::to(&organization_action_redirect(&id, character_id, &query)).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn clear_presented_organization(
    State(state): State<AppState>,
    Path((id, character_id)): Path<(String, u64)>,
    Query(query): Query<OrganizationActionQuery>,
    session: Session,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (StatusCode::FORBIDDEN, "Select this character first").into_response();
    }
    match state
        .db
        .call("clear_organization_presentation", &[json!(character_id)])
        .await
    {
        Ok(()) => {
            Redirect::to(&organization_action_redirect(&id, character_id, &query)).into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Default, Deserialize)]
struct OrganizationActionQuery {
    return_to: Option<String>,
}

fn organization_action_redirect(
    settlement_id: &str,
    character_id: u64,
    query: &OrganizationActionQuery,
) -> String {
    let base = format!("/locations/settlement/{settlement_id}/party/{character_id}");
    if query.return_to.as_deref() == Some("character") {
        base
    } else {
        format!("{base}/organizations")
    }
}

#[derive(Serialize)]
struct ServiceQuestRecruitment {
    offer_id: String,
    service_id: &'static str,
    location_id: String,
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
) -> Json<ServiceActivityResponse> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Json(ServiceActivityResponse {
            quests: Vec::new(),
            recruitment: Vec::new(),
        });
    };
    let quests: Vec<ContractPresentation> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_contracts WHERE settlement_id = {}",
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
                .is_some_and(|party| party.active_contract_id.is_none())
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
    let recruitment_offers: Vec<RecruitmentOffer> = state
        .db
        .query(&format!(
            "SELECT * FROM recruitment_offer WHERE settlement_id = {}",
            sql_string_literal(&id)
        ))
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

    let recruiting_companies = recruitment_offers
        .iter()
        .filter(|offer| {
            offer.status == RecruitmentOfferStatus::Open
                && viewer_party_id != Some(offer.recruiting_party_id.as_str())
        })
        .filter_map(|offer| {
            let party = parties
                .iter()
                .find(|party| party.id == offer.recruiting_party_id)?;
            let leader = characters
                .iter()
                .find(|character| character.id == offer.leader_id)?;
            if party.current_settlement_id.as_deref() != Some(id.as_str())
                || party.leader_id != leader.id
            {
                return None;
            }
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
                    let (match_level, match_summary) = party_role_match(&viewer_capabilities, role);
                    let (left_html, right_html) =
                        crate::templates::recruitment::service_role_inspection(
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
            (!roles.is_empty()).then(|| ServiceQuestRecruitment {
                offer_id: offer.id_key.clone(),
                service_id: "inn",
                location_id: offer.location_id.clone(),
                party_name: party.name.clone(),
                leader_id: leader.id.to_string(),
                leader_name: leader.name.clone(),
                roles,
            })
        })
        .collect();
    let quest_offers = quests
            .iter()
            .filter_map(|quest| {
                let is_current = active_party.as_ref().is_some_and(|party| {
                    party.active_contract_id.as_deref() == Some(quest.id.as_str())
                        && quest.accepted_by.as_deref() == Some(party.id.as_str())
                });
                let state = if quest.status == ContractPresentationStatus::Offered {
                    "available"
                } else if is_current
                    && quest.status == ContractPresentationStatus::ReadyToReport
                {
                    "ready"
                } else if is_current {
                    "underway"
                } else {
                    return None;
                };
                let problem = quest.description.trim_end_matches('.').to_lowercase();
                let (npc_name, greeting) = service_quest_greeting(&quest.service_id);
                Some(ServiceQuestOffer {
                    id: quest.id.clone(),
                    title: quest.title.clone(),
                    description: active_contract_tooltip(quest),
                    service_id: quest.service_id.clone(),
                    npc_name,
                    greeting: greeting.to_string(),
                    follow_up: format!("{problem}?"),
                    problem,
                    details: service_quest_details(
                        &quest.service_id,
                        quest,
                        &settlement.name,
                        &neighboring_name,
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
                })
            })
            .collect();
    Json(ServiceActivityResponse {
        quests: quest_offers,
        recruitment: recruiting_companies,
    })
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
    _service_id: &str,
    quest: &ContractPresentation,
    _settlement_name: &str,
    _neighboring_name: &str,
) -> String {
    // The generated quest is authoritative. Service identifies the speaker,
    // never the threat or location; several templates intentionally share it.
    let situation = &quest.description;
    format!(
        "Yes, {situation}. I believe it involves {} {}, but that account may be wrong. I'd offer {} coin for a verified resolution. Learn more before committing to a fight. Are you",
        quest.opposition_count_wording, quest.opposition_wording, quest.gold_reward,
    )
}

#[cfg(test)]
mod bestiary_quest_presentation_tests {
    use super::*;

    fn quest(opposition_wording: &str, description: &str) -> ContractPresentation {
        ContractPresentation {
            id: "q".into(),
            case_id: "case:q".into(),
            title: "Problem".into(),
            description: description.into(),
            difficulty: 2,
            gold_reward: 40,
            xp_reward: 20,
            settlement_id: "s".into(),
            service_id: "inn".into(),
            issuer_npc_id: String::new(),
            status: ContractPresentationStatus::Offered,
            accepted_by: None,
            opposition_wording: opposition_wording.into(),
            opposition_count_wording: "perhaps several".into(),
        }
    }

    #[test]
    fn shared_service_never_substitutes_its_old_fixed_threat_or_location() {
        let alp = quest("alp", "Sleepers report an unseen visitor.");
        let hound = quest("spectral_hound", "A black hound haunts the road.");
        let alp_details = service_quest_details("inn", &alp, "A", "B");
        let hound_details = service_quest_details("inn", &hound, "A", "B");
        assert!(alp_details.contains("unseen visitor"));
        assert!(hound_details.contains("black hound"));
        assert!(!alp_details.contains("goblin") && !hound_details.contains("goblin"));
    }
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
        LocationKind::CaseSite => state
            .db
            .query_one::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE case_site_id = {}",
                sql_string_literal(id)
            ))
            .await
            .map(|row| row.map(|site| (site.display_title, None, None))),
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
        LocationKind::CaseSite => {
            character.current_case_site_id.as_deref() == Some(location.id.as_str())
        }
    }
}

async fn party_personal(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    render_party_personal(
        &state,
        &kind,
        &id,
        character_id,
        building,
        &session,
        None,
        None,
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn render_party_personal(
    state: &AppState,
    kind: &str,
    id: &str,
    character_id: u64,
    building: BuildingQuery,
    session: &Session,
    dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
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
    let apprenticeships: Vec<crate::spacetimedb::OrganizationMembership> = state
        .db
        .query(&format!(
            "SELECT * FROM organization_membership WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let organization_presentation = state
        .db
        .query_one::<crate::spacetimedb::OrganizationPresentation>(&format!(
            "SELECT * FROM organization_presentation WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten();
    let character_minute = query_single::<CharacterTime>(&state, "character_time", character_id)
        .await
        .map_or(0, |time| time.minutes);
    let capability = get_character_capability(&state, character_id).await;
    let combat_profile = get_combat_training_profile(&state, character_id).await;
    let can_examine = false;
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
    )
    .with_professions(
        attributes.first(),
        skills.first(),
        &apprenticeships,
        &location.id,
        character_minute,
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
    let virtue = query_single::<CharacterVirtue>(&state, "character_virtue", character_id)
        .await
        .map_or(0.0, |virtue| virtue.value);
    // Authoritative personality is private. Ordinary pages render only
    // observer-specific beliefs through the dedicated social route.
    let personality: Option<CharacterPersonality> = None;
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
    let food_lots = state
        .db
        .query::<FoodLot>("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let inventory_amounts = state
        .db
        .query::<InventoryItemAmount>("SELECT * FROM inventory_item_amount")
        .await
        .unwrap_or_default();
    let item_definitions = state
        .db
        .query::<ItemDefinition>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let foraging_dialog = if building.forage.unwrap_or(false) {
        Some(
            crate::routes::foraging::activity_dialog(
                state,
                &active_character,
                &location
                    .preserve_building(format!("{}/party/{character_id}", location.base_path(),)),
                building.forage_receipt.as_deref(),
                building.forage_error.as_deref(),
            )
            .await,
        )
    } else {
        None
    };
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
            &apprenticeships,
            organization_presentation.as_ref(),
            character_minute,
            prayer_religion_check,
            schedule.first(),
            combat_profile,
            activity_preview,
            religious_demand.as_ref(),
            virtue,
            personality.as_ref(),
            &medical,
            can_examine,
            &injuries,
            &projectiles,
            &filth,
            building.cooking(),
            &active_inventory,
            &inventory_amounts,
            &food_lots,
            &item_definitions,
            dialog,
            surgery_open,
            social_open,
            foraging_dialog,
        )
        .into_string(),
    )
}

#[derive(Deserialize)]
struct CookFoodForm {
    method: String,
    inventory_item_ids: String,
    amounts_milliunits: String,
}

async fn cook_food(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<CookFoodForm>,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (
            StatusCode::FORBIDDEN,
            "Only the selected character can cook",
        )
            .into_response();
    }
    let parse = |value: &str| -> Result<Vec<u64>, _> {
        value
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::parse)
            .collect()
    };
    let ids = match parse(&form.inventory_item_ids) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ingredient selection").into_response(),
    };
    let amounts_milliunits = match form
        .amounts_milliunits
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(value) => value,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid ingredient amounts").into_response();
        }
    };
    let method = match form.method.as_str() {
        "pan-fry" => json!({ "panFry": {} }),
        "stew" => json!({ "stew": {} }),
        "roast" => json!({ "roast": {} }),
        "bake" => json!({ "bake": {} }),
        _ => return (StatusCode::BAD_REQUEST, "Invalid cooking method").into_response(),
    };
    if let Err(error) = state
        .db
        .call(
            "cook_food",
            &[
                json!(character_id),
                method,
                json!(ids),
                json!(amounts_milliunits),
            ],
        )
        .await
    {
        tracing::warn!(%error, character_id, "cooking failed");
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{character_id}?cook=true"
    )))
    .into_response()
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
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
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
    apprenticeship_organization_id: Option<String>,
    profession_practice_minutes: u16,
    practice_organization_id: Option<String>,
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
    fn schedule_form_contains_only_activity_allocations() {
        let form: TrainingScheduleForm = serde_json::from_value(json!({
            "combat_training_minutes": 90, "labor_minutes": 15,
            "prayer_minutes": 30, "thievery_minutes": 0, "raiding_minutes": 0
        }))
        .unwrap();
        assert_eq!(form.combat_training_minutes, 90);
        assert_eq!(form.labor_minutes, 15);
        assert_eq!(form.prayer_minutes, 30);
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
        apprenticeship_organization_id: form.apprenticeship_organization_id,
        profession_practice_minutes: form.profession_practice_minutes,
        practice_organization_id: form.practice_organization_id,
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

#[derive(Deserialize)]
struct ImmediateActivityForm {
    activity: String,
    requested_minutes: u64,
    #[serde(default)]
    service_id: Option<String>,
}

fn immediate_activity_arg(activity: &str) -> Option<serde_json::Value> {
    let tag = match activity {
        "prayer" => "prayer",
        "combat_training" => "combatTraining",
        "carousing" => "carousing",
        "apprenticeship" => "apprenticeship",
        "profession_practice" => "professionPractice",
        "labor" => "labor",
        "thievery" => "thievery",
        "raiding" => "raiding",
        _ => return None,
    };
    Some(json!({ (tag): {} }))
}

async fn perform_immediate_activity(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<ImmediateActivityForm>,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (
            StatusCode::FORBIDDEN,
            "Select this character before performing an activity",
        )
            .into_response();
    }
    let Some(activity) = immediate_activity_arg(&form.activity) else {
        return (StatusCode::BAD_REQUEST, "Unknown activity").into_response();
    };
    if form.requested_minutes < 60
        || form.requested_minutes > 1_440
        || form.requested_minutes % 60 != 0
    {
        return (StatusCode::BAD_REQUEST, "Choose one to 24 whole hours").into_response();
    }
    let service_id = form.service_id.filter(|value| !value.is_empty());
    match state
        .db
        .call(
            "perform_immediate_activity",
            &[
                json!(character_id),
                activity,
                json!(form.requested_minutes),
                json!(service_id),
            ],
        )
        .await
    {
        Ok(()) => {
            if let Some((character, _)) = get_active_character(&state, Some(character_id)).await
                && let Some(case_site_id) = character.current_case_site_id
            {
                return Redirect::to(&format!("/locations/case-site/{case_site_id}"))
                    .into_response();
            }
            Redirect::to(
                &building.append_to(format!("/locations/{kind}/{id}/party/{character_id}")),
            )
            .into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
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
        || selected.current_case_site_id != active_character.current_case_site_id
    {
        return Html("<h1>Character is not at this location</h1>".to_string());
    }
    if selected.id != active_character.id
        && (active_character.party_id.is_none() || selected.party_id != active_character.party_id)
    {
        return Html("<h1>Party member not found</h1>".to_string());
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
    let food_lots: Vec<FoodLot> = state
        .db
        .query("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let selected_targets = personal_inventory_targets(&state, selected.id).await;
    let active_targets = personal_inventory_targets(&state, active_character.id).await;
    let encumbrance_rows =
        EncumbranceRows::query(&state, &[selected.id, active_character.id]).await;
    let selected_encumbrance = personal_encumbrance(
        selected.id,
        &selected_inventory,
        &items,
        &food_lots,
        &encumbrance_rows,
    );
    let active_encumbrance = personal_encumbrance(
        active_character.id,
        &active_inventory,
        &items,
        &food_lots,
        &encumbrance_rows,
    );

    if character_id == active_character.id {
        return Html(
            party_discard_page(
                &location,
                &active_character,
                &active_inventory,
                &items,
                &food_lots,
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
            &food_lots,
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
    let food_lots: Vec<FoodLot> = state
        .db
        .query("SELECT * FROM food_lot")
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
            &food_lots,
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
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character").into_response();
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
            return (StatusCode::SERVICE_UNAVAILABLE, "Inventory is unavailable").into_response();
        }
    };
    let Some(inventory) = inventory else {
        return (StatusCode::NOT_FOUND, "Item is not in this inventory").into_response();
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
            )
                .into_response();
        }
    };
    let Some(definition) = definition else {
        return (StatusCode::NOT_FOUND, "Item definition is missing").into_response();
    };
    if definition.kind == crate::spacetimedb::ItemKind::Medication {
        return (
            StatusCode::BAD_REQUEST,
            "Preparations are administered through the Physiology interface.",
        )
            .into_response();
    }
    let destination = if form.equipped {
        definition.slot
    } else {
        ItemSlot::None
    };
    if form.equipped && destination == ItemSlot::None {
        return (StatusCode::BAD_REQUEST, "This item cannot be equipped").into_response();
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
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    for reducer in ["refresh_capabilities", "refresh_strategic_condition"] {
        if let Err(error) = state.db.call(reducer, &[json!(character_id)]).await {
            tracing::warn!(%error, character_id, reducer, "failed to refresh equipment projection");
        }
    }
    (StatusCode::NO_CONTENT, "").into_response()
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
    render_party_stats(
        &state,
        &kind,
        &id,
        character_id,
        building,
        &session,
        None,
        None,
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn render_party_stats(
    state: &AppState,
    kind: &str,
    id: &str,
    character_id: u64,
    building: BuildingQuery,
    session: &Session,
    dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
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
        || selected.current_case_site_id != active_character.current_case_site_id
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
    let can_examine = false;
    let condition = get_strategic_condition(&state, character_id).await;
    let morale_sources = get_morale_sources(&state, character_id).await;
    let religion = query_single::<CharacterCondition>(&state, "character_condition", character_id)
        .await
        .and_then(|condition| condition.religion_id);
    let virtue = query_single::<CharacterVirtue>(&state, "character_virtue", character_id)
        .await
        .map_or(0.0, |virtue| virtue.value);
    let personality: Option<CharacterPersonality> = None;
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
            virtue,
            personality.as_ref(),
            &medical,
            can_examine,
            &injuries,
            &projectiles,
            &filth,
            dialog,
            surgery_open,
            social_open,
        )
        .into_string(),
    )
}

pub(crate) async fn medical_presentation(
    state: &AppState,
    viewer_id: u64,
    target_id: u64,
) -> crate::medical::MedicalPresentation {
    let rows = match state
        .db
        .query::<BackendPhysiologyChart>(&format!(
            "SELECT * FROM backend_physiology_charts WHERE observer_id = {viewer_id} AND patient_id = {target_id}"
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
    let administrations = if administration_history_visible(viewer_id, target_id, !rows.is_empty())
    {
        state
            .db
            .query::<BackendPhysiologyAdministration>(&format!(
                "SELECT * FROM backend_physiology_administrations WHERE patient_id = {target_id}"
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    crate::medical::sanitize(&rows, &administrations)
}

fn administration_history_visible(
    viewer_id: u64,
    target_id: u64,
    has_authorized_observations: bool,
) -> bool {
    viewer_id == target_id || has_authorized_observations
}

#[cfg(test)]
mod physiology_privacy_tests {
    use super::administration_history_visible;

    #[test]
    fn administration_history_requires_self_or_an_authorized_observation() {
        assert!(administration_history_visible(7, 7, false));
        assert!(administration_history_visible(7, 8, true));
        assert!(!administration_history_visible(7, 8, false));
    }
}

#[derive(Deserialize)]
struct AdministrationForm {
    inventory_item_id: u64,
    route: String,
    amount_milliunits: u32,
    region: Option<String>,
}

async fn administer_preparation(
    State(state): State<AppState>,
    Path((kind, id, patient_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<AdministrationForm>,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call(
            "administer_preparation",
            &[
                json!(actor_id),
                json!(patient_id),
                json!(form.inventory_item_id),
                json!(1u16),
                json!(form.route),
                json!(form.amount_milliunits),
                json!(form.region.filter(|value| !value.is_empty())),
            ],
        )
        .await
    {
        tracing::warn!(%error, actor_id, patient_id, "preparation administration rejected");
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{patient_id}")))
}

async fn stop_preparation(
    State(state): State<AppState>,
    Path((kind, id, patient_id, administration_id)): Path<(String, String, u64, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call(
            "stop_preparation",
            &[json!(actor_id), json!(administration_id)],
        )
        .await
    {
        tracing::warn!(%error, actor_id, patient_id, administration_id, "preparation stop rejected");
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{patient_id}")))
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

async fn party_social(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
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
    let Some((active, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    let selected = if target_id == active.id {
        active.clone()
    } else {
        match state
            .db
            .query_one::<Character>(&format!("SELECT * FROM character WHERE id = {target_id}"))
            .await
            .ok()
            .flatten()
        {
            Some(value) => value,
            None => return Html("<h1>Party member not found</h1>".into()),
        }
    };
    let same_party = target_id == active.id
        || (active.party_id.is_some() && active.party_id == selected.party_id);
    let colocated = active.current_settlement_id == selected.current_settlement_id
        && active.current_case_site_id == selected.current_case_site_id;
    if !same_party
        || !colocated
        || !active.alive
        || !selected.alive
        || !character_is_at_location(&active, &location)
    {
        return Html("<h1>Social actions require a living, co-located party member</h1>".into());
    }
    let party_members = get_active_party_members(&state, Some(&active)).await;
    let sources = get_morale_sources(&state, target_id).await;
    let actor_sources = get_morale_sources(&state, active.id).await;
    let mut shared_concerns = actor_sources
        .iter()
        .filter(|source| {
            adventuresim_core::social::social_source_eligible(&source.kind, source.magnitude)
        })
        .filter_map(|source| adventuresim_core::social::topic_for_source_kind(&source.kind))
        .collect::<Vec<_>>();
    shared_concerns.sort_by_key(|topic| format!("{topic:?}"));
    shared_concerns.dedup();
    let target_condition_result = state
        .db
        .query_one::<CharacterCondition>(&format!(
            "SELECT * FROM character_condition WHERE character_id = {target_id}"
        ))
        .await;
    let religion_id = target_condition_result
        .as_ref()
        .ok()
        .and_then(|value| value.as_ref())
        .and_then(|value| value.religion_id.clone());
    let virtue = query_single::<CharacterVirtue>(&state, "character_virtue", target_id)
        .await
        .map_or(0.0, |value| value.value);
    let target_minute = query_single::<CharacterTime>(&state, "character_time", target_id)
        .await
        .map_or(0, |v| v.minutes);
    let affinity_id = format!("{target_id}:{}", active.id);
    let affinity_result = state
        .db
        .query_one::<CharacterAffinity>(&format!(
            "SELECT * FROM backend_character_affinities WHERE id = {}",
            sql_string_literal(&affinity_id)
        ))
        .await;
    let affinity_available = affinity_result.is_ok();
    let affinity = affinity_result.ok().flatten().map_or(0.0, |v| {
        adventuresim_core::social::settle_affinity(
            v.anchor,
            target_minute.saturating_sub(v.anchor_minute),
        )
    });
    let (low, high) = (active.id.min(target_id), active.id.max(target_id));
    let familiarity_id = format!("{low}:{high}");
    let familiarity_result = state
        .db
        .query_one::<CharacterFamiliarity>(&format!(
            "SELECT * FROM backend_character_familiarities WHERE id = {}",
            sql_string_literal(&familiarity_id)
        ))
        .await;
    let familiarity_available = familiarity_result.is_ok();
    let shared_minutes = familiarity_result
        .ok()
        .flatten()
        .map_or(0, |v| v.shared_minutes);
    let beliefs_result = state
        .db
        .query::<SocialBelief>(&format!(
            "SELECT * FROM backend_social_beliefs WHERE observer_id = {}",
            active.id
        ))
        .await;
    let beliefs_available = beliefs_result.is_ok();
    let beliefs = match beliefs_result {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| row.subject_id == target_id)
            .collect(),
        Err(error) => {
            tracing::error!(%error, observer_id=active.id, target_id, "private social belief query failed closed");
            Vec::new()
        }
    };
    let addressed_source_ids = state
        .db
        .query::<SocialAddress>(&format!(
            "SELECT * FROM backend_social_addresses WHERE actor_id = {}",
            active.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.target_id == target_id)
        .map(|row| row.source_id)
        .collect();
    let automatic_chat_enabled = if target_id == active.id {
        false
    } else {
        state
            .db
            .query_one::<AutomaticSocialChat>(&format!(
                "SELECT * FROM backend_automatic_social_chats WHERE id = {}",
                sql_string_literal(&format!("{}:{target_id}", active.id))
            ))
            .await
            .ok()
            .flatten()
            .is_some_and(|row| row.enabled)
    };
    let actor_personality_result = state
        .db
        .query::<CharacterPersonality>(&format!(
            "SELECT * FROM backend_character_personalities WHERE character_id = {}",
            active.id
        ))
        .await;
    let actor_personality_available = actor_personality_result.is_ok();
    let actor_personality = match actor_personality_result {
        Ok(rows) => rows.into_iter().next(),
        Err(error) => {
            tracing::error!(
                %error,
                actor_id = active.id,
                "private actor personality query failed closed"
            );
            None
        }
    };
    let actor_skills_result = state
        .db
        .query_one::<CharacterSkills>(&format!(
            "SELECT * FROM character_skills WHERE character_id = {}",
            active.id
        ))
        .await;
    let prayer_disabled_reason = if target_id == active.id {
        None
    } else if !actor_personality_available || actor_personality.is_none() {
        Some("Prayer eligibility is unavailable right now.".to_owned())
    } else if actor_personality.as_ref().is_some_and(|personality| {
        personality.conviction == crate::spacetimedb::Conviction::Zealous
    }) {
        Some("Your Zealous conviction prevents you from leading a companion's prayer.".to_owned())
    } else {
        match &target_condition_result {
            Err(error) => {
                tracing::error!(%error, target_id, "target religion query failed closed");
                Some("Their religion is unavailable right now.".to_owned())
            }
            Ok(None) => Some("Their religion is unavailable right now.".to_owned()),
            Ok(Some(condition)) => match condition.religion_id.as_deref() {
                None => Some("They profess no religion.".to_owned()),
                Some(religion_id) => {
                    match adventuresim_world_schema::OfficialReligion::from_id(religion_id) {
                        None => Some("Their religion is unknown.".to_owned()),
                        Some(religion) => match &actor_skills_result {
                            Err(error) => {
                                tracing::error!(%error, actor_id=active.id, "private Religion knowledge query failed closed");
                                Some("Your Religion knowledge is unavailable right now.".to_owned())
                            }
                            Ok(None) => {
                                Some("Your Religion knowledge is unavailable right now.".to_owned())
                            }
                            Ok(Some(skills))
                                if !skills.religion_hours.direct(religion).is_finite()
                                    || skills.religion_hours.direct(religion) <= 0.0 =>
                            {
                                Some(format!(
                                    "You have not directly studied {}.",
                                    religion.label()
                                ))
                            }
                            Ok(Some(_)) => None,
                        },
                    }
                }
            },
        }
    };
    let social = SocialPresentation {
        affinity,
        familiarity_hours: adventuresim_core::social::effective_familiarity_hours(
            shared_minutes,
            party_members.iter().filter(|v| v.alive).count(),
            true,
        ),
        religion_id,
        virtue,
        beliefs,
        shared_concerns,
        addressed_source_ids,
        automatic_chat_enabled,
        joke_blocked: social_action_blocked_by_actor(
            actor_personality_available,
            actor_personality.as_ref(),
            adventuresim_core::social::SocialActionKind::LightenMood,
        ),
        flirt_blocked: social_action_blocked_by_actor(
            actor_personality_available,
            actor_personality.as_ref(),
            adventuresim_core::social::SocialActionKind::Flirt,
        ),
        prayer_disabled_reason,
        feedback: social_feedback(building.social_feedback.as_deref()),
        unavailable: !beliefs_available || !affinity_available || !familiarity_available,
    };
    let dialog = party_social_dialog(&location, &selected, &active, &sources, &social);
    if target_id == active.id {
        render_party_personal(
            &state,
            &kind,
            &id,
            target_id,
            building,
            &session,
            Some(dialog),
            None,
            true,
        )
        .await
    } else {
        render_party_stats(
            &state,
            &kind,
            &id,
            target_id,
            building,
            &session,
            Some(dialog),
            None,
            true,
        )
        .await
    }
}

#[derive(Deserialize)]
struct SocialActionForm {
    source_id: String,
    action_kind: String,
}

#[derive(Deserialize)]
struct CasualChatForm {
    requested_minutes: u64,
    action_id: String,
}

#[derive(Deserialize)]
struct BackendSocialChatReceiptRow {
    outcome: String,
}

#[derive(Deserialize)]
struct AutomaticSocialChatForm {
    enabled: Option<String>,
}

async fn set_automatic_social_chat(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<AutomaticSocialChatForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character first").into_response();
    };
    if let Err(error) = state
        .db
        .call(
            "set_automatic_social_chat",
            &[
                json!(actor_id),
                json!(target_id),
                json!(form.enabled.is_some()),
            ],
        )
        .await
    {
        tracing::warn!(%error, actor_id, target_id, "automatic social chat preference rejected");
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{target_id}/social")))
        .into_response()
}

async fn perform_social_action(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<SocialActionForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character first").into_response();
    };
    // The actor is derived exclusively from the signed session, never form input.
    let result = state
        .db
        .call(
            "perform_social_action",
            &[
                json!(actor_id),
                json!(target_id),
                json!(form.source_id),
                json!(form.action_kind),
            ],
        )
        .await;
    let feedback = match result {
        Ok(()) => {
            let address_id = format!("{actor_id}:{target_id}:{}", form.source_id);
            match state
                .db
                .query_one::<SocialAddress>(&format!(
                    "SELECT * FROM backend_social_addresses WHERE id = {}",
                    sql_string_literal(&address_id)
                ))
                .await
            {
                Ok(Some(_)) => "addressed",
                Ok(None) => "not_addressed",
                Err(error) => {
                    tracing::warn!(%error, actor_id, target_id, "social action result unavailable");
                    "unavailable"
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, actor_id, target_id, "social action rejected");
            social_action_error_feedback(&error.to_string())
        }
    };
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{target_id}/social?social_feedback={feedback}"
    )))
    .into_response()
}

fn valid_casual_chat_minutes(minutes: u64) -> bool {
    (15..=8 * 60).contains(&minutes) && minutes % 15 == 0
}

fn valid_casual_chat_action_id(action_id: &str) -> bool {
    !action_id.is_empty()
        && action_id.len() <= 96
        && action_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

async fn chat_with_party_member(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<CasualChatForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character first").into_response();
    };
    if !valid_casual_chat_minutes(form.requested_minutes) {
        return (
            StatusCode::BAD_REQUEST,
            "Choose 15 minutes to 8 hours in 15-minute increments",
        )
            .into_response();
    }
    if !valid_casual_chat_action_id(&form.action_id) {
        return (StatusCode::BAD_REQUEST, "Invalid conversation action ID").into_response();
    }
    let result = state
        .db
        .call(
            "chat_with_party_member",
            &[
                json!(actor_id),
                json!(target_id),
                json!(form.requested_minutes),
                json!(&form.action_id),
            ],
        )
        .await;
    let feedback = match result {
        Ok(()) => state
            .db
            .query_one::<BackendSocialChatReceiptRow>(&format!(
                "SELECT * FROM backend_social_chat_receipts WHERE id = {} AND actor_id = {actor_id}",
                sql_string_literal(&format!("{actor_id}:{}", form.action_id))
            ))
            .await
            .ok()
            .flatten()
            .map_or("chat_unavailable", |row| match row.outcome.as_str() {
                "positive" => "chat_positive",
                "mixed" => "chat_mixed",
                "negative" => "chat_negative",
                _ => "chat_unavailable",
            }),
        Err(error) => {
            tracing::warn!(%error, actor_id, target_id, "casual party chat rejected");
            "chat_unavailable"
        }
    };
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{target_id}/social?social_feedback={feedback}"
    )))
    .into_response()
}

fn social_action_error_feedback(error: &str) -> &'static str {
    if error.contains("needs time before it can be tried again") {
        "cooldown"
    } else if error.contains("Morale source is stale")
        || error.contains("Only current, negative, recognized morale sources")
        || error.contains("Morale source is not actionable")
    {
        "stale"
    } else {
        "unavailable"
    }
}

fn social_action_blocked_by_actor(
    personality_available: bool,
    personality: Option<&CharacterPersonality>,
    action: adventuresim_core::social::SocialActionKind,
) -> bool {
    use adventuresim_core::social::{
        Courtship as CoreCourtship, Mirth as CoreMirth, actor_allows_social_action,
    };

    if !personality_available {
        return true;
    }
    let Some(personality) = personality else {
        return true;
    };
    let mirth = match personality.mirth {
        crate::spacetimedb::Mirth::Neutral => CoreMirth::Neutral,
        crate::spacetimedb::Mirth::Merry => CoreMirth::Merry,
        crate::spacetimedb::Mirth::Grave => CoreMirth::Grave,
    };
    let courtship = match personality.courtship {
        crate::spacetimedb::Courtship::Neutral => CoreCourtship::Neutral,
        crate::spacetimedb::Courtship::Amorous => CoreCourtship::Amorous,
        crate::spacetimedb::Courtship::Proper => CoreCourtship::Proper,
    };
    !actor_allows_social_action(action, mirth, courtship)
}

fn social_feedback(value: Option<&str>) -> Option<crate::templates::settlement::SocialFeedback> {
    use crate::templates::settlement::SocialFeedback;
    match value {
        Some("addressed") => Some(SocialFeedback {
            message: "This concern is addressed.",
            is_error: false,
        }),
        Some("not_addressed") => Some(SocialFeedback {
            message: "This concern remains unresolved.",
            is_error: false,
        }),
        Some("cooldown") => Some(SocialFeedback {
            message: "That approach needs time before it can be tried again.",
            is_error: true,
        }),
        Some("stale") => Some(SocialFeedback {
            message: "That morale concern has changed. Choose a current concern.",
            is_error: true,
        }),
        Some("unavailable") => Some(SocialFeedback {
            message: "The social action could not be completed right now.",
            is_error: true,
        }),
        Some("chat_positive") => Some(SocialFeedback {
            message: "The conversation brings you closer.",
            is_error: false,
        }),
        Some("chat_mixed") => Some(SocialFeedback {
            message: "The conversation has warm moments and awkward ones.",
            is_error: false,
        }),
        Some("chat_negative") => Some(SocialFeedback {
            message: "The conversation leaves some friction between you.",
            is_error: false,
        }),
        Some("chat_unavailable") => Some(SocialFeedback {
            message: "The conversation could not be completed right now.",
            is_error: true,
        }),
        _ => None,
    }
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
    Path((id, service_id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<MerchantOfferForm>,
) -> Redirect {
    let Some(location_id) = merchant_service_location(&service_id) else {
        return Redirect::to(&format!("/settlements/{id}/merchants"));
    };
    let fallback = format!("/settlements/{id}/{service_id}");
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
            let provider_npc_id = merchant_provider_id(&state, &id, &service_id, location_id).await;
            if items.is_empty() && sell_ids.is_empty() {
                trade_completed = true;
            } else if let Some(provider_npc_id) = provider_npc_id {
                match state
                    .db
                    .call(
                        "finalize_storefront_trade",
                        &[
                            json!(character.id),
                            json!(&id),
                            json!(&service_id),
                            json!(provider_npc_id),
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
            }
        }
    }
    if trade_completed {
        redirect_to_local(&form.return_to, &fallback)
    } else {
        Redirect::to(&fallback)
    }
}

fn merchant_service_location(service_id: &str) -> Option<&'static str> {
    match service_id {
        "merchants" => Some("market"),
        "weapons" => Some("forge"),
        "armor" => Some("armoury"),
        "clothing" => Some("tailor"),
        "inn" => Some("inn"),
        _ => None,
    }
}

async fn merchant_provider_id(
    state: &AppState,
    settlement_id: &str,
    service_id: &str,
    location_id: &str,
) -> Option<String> {
    let settlement_literal = sql_string_literal(settlement_id);
    let providers_sql = format!(
        "SELECT * FROM backend_settlement_npcs WHERE home_settlement_id = {settlement_literal}"
    );
    let presences_sql =
        format!("SELECT * FROM settlement_npc_presence WHERE settlement_id = {settlement_literal}");
    let (providers, presences) = tokio::join!(
        state.db.query::<MerchantProviderRow>(&providers_sql),
        state
            .db
            .query::<MerchantProviderPresenceRow>(&presences_sql),
    );
    let providers = providers.ok()?;
    let presences = presences.ok()?;
    let mut matches = providers.into_iter().filter_map(|provider| {
        (provider.home_settlement_id == settlement_id && provider.service_id == service_id)
            .then_some(provider)
            .and_then(|provider| {
                presences
                    .iter()
                    .any(|presence| {
                        presence.npc_id == provider.id
                            && presence.settlement_id == settlement_id
                            && presence.location_id == location_id
                            && presence.is_default
                    })
                    .then_some(provider.id)
            })
    });
    let provider = matches.next()?;
    matches.next().is_none().then_some(provider)
}

async fn provisioning_storefront_path(state: &AppState, settlement: &Settlement) -> Option<String> {
    use adventuresim_core::settlement_economy::{Storefront, storefront_available};

    for (storefront, service_id, location_id) in [
        (Storefront::General, "merchants", "market"),
        (Storefront::Inn, "inn", "inn"),
    ] {
        if storefront_available(&settlement.economy, storefront)
            && merchant_provider_id(state, &settlement.id, service_id, location_id)
                .await
                .is_some()
        {
            return Some(format!("/settlements/{}/{service_id}", settlement.id));
        }
    }
    None
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
    merchant_shop(state, id, session, MerchantShop::Inn).await
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

fn safe_rest_error(error: &str) -> &'static str {
    if error.contains("Not enough coin") {
        "You do not have enough coin for that inn stay."
    } else {
        "The rest could not be completed. Review the duration and try again."
    }
}

async fn rest(
    State(state): State<AppState>,
    Path((id, kind)): Path<(String, String)>,
    session: Session,
    form: Result<Form<RestForm>, FormRejection>,
) -> Response {
    let at_inn = match kind.as_str() {
        "inn" => true,
        "temple" => false,
        _ => return Html("<h1>Rest service not found</h1>".to_string()).into_response(),
    };
    let Some(character_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".to_string()).into_response();
    };
    let form = match form {
        Ok(Form(form)) => form,
        Err(error) => {
            tracing::warn!(
                character_id,
                requested_settlement_id_length = id.len(),
                service = kind.as_str(),
                rejection_status = %error.status(),
                error = %error,
                "settlement rest form extraction rejected request"
            );
            return error.into_response();
        }
    };
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
    let service = if at_inn {
        adventuresim_core::settlement_economy::SettlementActionService::Inn
    } else {
        adventuresim_core::settlement_economy::SettlementActionService::Temple
    };
    if !settlement_action_service_available(&settlement.economy, service) {
        return Html("<h1>Rest service unavailable</h1>".to_string()).into_response();
    }
    let requested_minutes = match settlement_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(message) => {
            let unit = match form.unit.as_str() {
                "hours" => "hours",
                "days" => "days",
                _ => "unknown",
            };
            tracing::warn!(
                character_id,
                requested_settlement_id = %id,
                requested_minutes = ?form.requested_minutes,
                at_inn,
                service = kind.as_str(),
                unit,
                duration_length = form.duration.len(),
                reason = message,
                "settlement rest duration validation rejected request"
            );
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Html(
                    crate::templates::strategic_notice_page(
                        "Unable to rest",
                        message,
                        &format!(
                            "/settlements/{id}/{}",
                            if at_inn { "inn" } else { "religion" }
                        ),
                        "Return to rest service",
                        None,
                    )
                    .into_string(),
                ),
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
    let character_settlement_id = before_character
        .as_ref()
        .and_then(|(character, _)| character.current_settlement_id.as_deref())
        .unwrap_or("<none>");
    if let Err(error) = state
        .db
        .call(
            "rest_at_settlement_hours",
            &[json!(character_id), json!(requested_minutes), json!(at_inn)],
        )
        .await
    {
        tracing::warn!(
            character_id,
            requested_settlement_id = %id,
            character_settlement_id,
            requested_minutes,
            at_inn,
            service = kind.as_str(),
            error = %error,
            "settlement rest reducer rejected request"
        );
        return (
            StatusCode::BAD_REQUEST,
            Html(
                crate::templates::strategic_notice_page(
                    "Unable to rest",
                    safe_rest_error(&error.to_string()),
                    &format!(
                        "/settlements/{id}/{}",
                        if at_inn { "inn" } else { "religion" }
                    ),
                    "Return to rest service",
                    None,
                )
                .into_string(),
            ),
        )
            .into_response();
    }

    let active_character = get_active_character(&state, Some(character_id)).await;
    if let Some(case_site_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.current_case_site_id.as_deref())
    {
        return Redirect::to(&format!("/locations/case-site/{case_site_id}")).into_response();
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
        at_inn,
        requested_minutes,
    );
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let items = state
        .db
        .query::<ItemDefinition>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let food_lots = state
        .db
        .query::<FoodLot>("SELECT * FROM food_lot")
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
            &food_lots,
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
    at_inn: bool,
    requested_minutes: u64,
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
    let (full_board_gold_spent, additional_gold_spent) =
        rest_spending_breakdown(gold_spent, at_inn, requested_minutes);
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
        full_board_gold_spent,
        additional_gold_spent,
        gold_earned,
        notoriety_gained,
        healed,
        trained,
    }
}

fn rest_spending_breakdown(
    total_gold_spent: u32,
    at_inn: bool,
    requested_minutes: u64,
) -> (u32, u32) {
    let full_board = if at_inn {
        adventuresim_core::strategic_economy::inn_full_board_cost(requested_minutes)
            .and_then(|cost| u32::try_from(cost).ok())
            .unwrap_or(u32::MAX)
    } else {
        0
    };
    (full_board, total_gold_spent.saturating_sub(full_board))
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
        ("Polearm", before.polearm_hours, after.polearm_hours),
        ("Axe", before.axe_hours, after.axe_hours),
        ("Bludgeon", before.bludgeon_hours, after.bludgeon_hours),
        ("Sword", before.sword_hours, after.sword_hours),
        ("Knife", before.knife_hours, after.knife_hours),
        ("Dodge", before.dodge_hours, after.dodge_hours),
        ("Block", before.block_hours, after.block_hours),
        ("Bow", before.bow_hours, after.bow_hours),
        ("Crossbow", before.crossbow_hours, after.crossbow_hours),
        ("Firearm", before.firearm_hours, after.firearm_hours),
        ("Throw", before.throw_hours, after.throw_hours),
        ("Will", before.will_hours, after.will_hours),
        ("Insight", before.insight_hours, after.insight_hours),
        ("Charm", before.charm_hours, after.charm_hours),
        ("Command", before.command_hours, after.command_hours),
        ("Deception", before.deception_hours, after.deception_hours),
        (
            "Physiology",
            before.physiology_hours,
            after.physiology_hours,
        ),
        ("Cooking", before.cooking_hours, after.cooking_hours),
        (
            "Religion",
            before.religion_hours.total_direct(),
            after.religion_hours.total_direct(),
        ),
        (
            "Bestiary",
            before.bestiary_hours.total_direct(),
            after.bestiary_hours.total_direct(),
        ),
        ("Stealth", before.stealth_hours, after.stealth_hours),
        ("Balance", before.balance_hours, after.balance_hours),
        ("Anatomy", before.anatomy_hours, after.anatomy_hours),
        ("Tailoring", before.tailoring_hours, after.tailoring_hours),
        ("Smithing", before.smithing_hours, after.smithing_hours),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| {
        let delta = after - before;
        (delta > 0.001).then(|| (name.to_string(), delta))
    })
    .collect()
}

#[cfg(test)]
mod anatomy_skill_delta_tests {
    use super::skill_deltas;
    use crate::spacetimedb::CharacterSkills;

    #[test]
    fn anatomy_training_is_reported_as_a_leaf_skill_delta() {
        let before = CharacterSkills {
            anatomy_hours: 12.0,
            ..Default::default()
        };
        let after = CharacterSkills {
            anatomy_hours: 13.5,
            ..Default::default()
        };
        assert_eq!(skill_deltas(&before, &after), vec![("Anatomy".into(), 1.5)]);
    }
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

async fn religion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    render_service_page(
        state,
        id,
        session,
        adventuresim_core::settlement_economy::SettlementActionService::Temple,
        religion_page,
    )
    .await
}

fn settlement_action_service_available(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    service: adventuresim_core::settlement_economy::SettlementActionService,
) -> bool {
    adventuresim_core::settlement_economy::action_service_available(profile, service)
}

#[cfg(test)]
mod service_availability_tests {
    use super::settlement_action_service_available;
    use adventuresim_core::settlement_economy::{
        SettlementActionService, player_visible_npc_tabs, visible_npc_tab,
    };
    use adventuresim_world_schema::{SettlementEconomyProfile, SettlementService};

    #[test]
    fn direct_routes_reject_unadvertised_church_inn_and_armoury() {
        let mut profile = SettlementEconomyProfile::stage_placeholder();
        profile.services.clear();
        assert!(!settlement_action_service_available(
            &profile,
            SettlementActionService::Temple
        ));
        assert!(!settlement_action_service_available(
            &profile,
            SettlementActionService::Inn
        ));
        let tabs = player_visible_npc_tabs(&profile, false);
        assert!(visible_npc_tab(&tabs, "church").is_none());
        assert!(visible_npc_tab(&tabs, "inn").is_none());
        assert!(visible_npc_tab(&tabs, "armoury").is_none());

        profile.services = vec![SettlementService::Inn, SettlementService::Temple];
        profile.services.sort();
        assert!(settlement_action_service_available(
            &profile,
            SettlementActionService::Temple
        ));
        assert!(settlement_action_service_available(
            &profile,
            SettlementActionService::Inn
        ));
    }
}

#[derive(Deserialize)]
struct ReligionForm {
    religion_id: String,
}

#[derive(Serialize)]
struct ReligionDialogue {
    religion_id: Option<String>,
    priest_religion_id: String,
    represented_religion_ids: Vec<String>,
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
        .filter(|settlement| {
            settlement_action_service_available(
                &settlement.economy,
                adventuresim_core::settlement_economy::SettlementActionService::Temple,
            )
        })
        .map(|settlement| settlement.religion_id.clone())
        .unwrap_or_default();
    let represented_religion_ids = settlement
        .as_ref()
        .filter(|settlement| {
            settlement_action_service_available(
                &settlement.economy,
                adventuresim_core::settlement_economy::SettlementActionService::Temple,
            )
        })
        .map(|s| {
            s.religious_status
                .represented_religions()
                .into_iter()
                .map(|r| r.religion_id().to_string())
                .collect()
        })
        .unwrap_or_default();
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Json(ReligionDialogue {
            religion_id: None,
            priest_religion_id,
            represented_religion_ids,
            can_choose: false,
        });
    };
    let can_choose = settlement.as_ref().is_some_and(|settlement| {
        settlement_action_service_available(
            &settlement.economy,
            adventuresim_core::settlement_economy::SettlementActionService::Temple,
        )
    }) && character.current_settlement_id.as_deref() == Some(id.as_str());
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
        represented_religion_ids,
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
    if !settlement_action_service_available(
        &settlement.economy,
        adventuresim_core::settlement_economy::SettlementActionService::Temple,
    ) {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "There is no church here to receive your profession.",
        });
    }
    if !settlement
        .religious_status
        .represented_religions()
        .iter()
        .any(|religion| religion.religion_id() == religion_id)
    {
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
    &[FoodLot],
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
    if !shop.available_at(settlement) {
        return Html(
            crate::templates::strategic_notice_page(
                "Service unavailable",
                "This settlement does not offer that service.",
                &format!("/locations/settlement/{}", settlement.id),
                "Return to settlement",
                None,
            )
            .into_string(),
        );
    }
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
    let consequence_sql = format!(
        "SELECT * FROM backend_local_problem_trade_effects WHERE character_id = {}",
        character.id
    );
    let (
        party_members,
        items,
        food_lots,
        equip,
        trade_context,
        conditions,
        smiths,
        orders,
        times,
        consequences,
    ) = tokio::join!(
        get_active_party_members(&state, Some(character)),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
        state.db.query::<FoodLot>("SELECT * FROM food_lot"),
        state.db.query::<CharacterEquip>(&equip_sql),
        inventory_trade_context(&state, character),
        state.db.query::<ItemCondition>(&condition_sql),
        state.db.query::<SettlementSmith>(&smith_sql),
        state.db.query::<RepairOrder>(&order_sql),
        state.db.query::<CharacterTime>(&time_sql),
        state
            .db
            .query::<BackendLocalProblemTradeEffect>(&consequence_sql),
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
    let (inn_rest_default, inn_soap_preview) = if matches!(shop, MerchantShop::Inn) {
        let (limbs, stats, condition) = tokio::join!(
            query_single::<CharacterLimbs>(&state, "character_limbs", character.id),
            query_single::<CharacterStats>(&state, "character_stats", character.id),
            query_single::<CharacterCondition>(&state, "character_condition", character.id),
        );
        let (field_repair_minutes, smith_wait_minutes) =
            equipment_rest_recommendation(&state, character.id, &id, inventory).await;
        let soap = soap_rest_preview(
            &state,
            std::slice::from_ref(character),
            character.party_id.as_deref(),
        )
        .await;
        (
            rest_default_minutes(
                limbs.as_ref(),
                stats.as_ref(),
                condition.as_ref(),
                field_repair_minutes,
                smith_wait_minutes,
            ),
            soap,
        )
    } else {
        (None, SoapRestPreview::default())
    };
    let speaker = query_single::<CharacterSkills>(&state, "character_skills", character.id)
        .await
        .map_or_default(|skills| skills.oral_languages);
    let speaker_cap =
        query_single::<CharacterAttributes>(&state, "character_attributes", character.id)
            .await
            .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
    let mut merchant_languages = adventuresim_world_schema::OralLanguageHours::default();
    *merchant_languages.direct_mut(settlement.languages.dominant_german()) =
        adventuresim_world_schema::ORAL_FLUENCY_HOURS;
    let (_, shared_language) = adventuresim_world_schema::best_common_oral_language_capped(
        speaker,
        speaker_cap,
        merchant_languages,
        adventuresim_world_schema::ORAL_FLUENCY_HOURS,
    );
    let now_minutes = times
        .as_ref()
        .ok()
        .and_then(|rows| rows.first())
        .map_or(0, |time| time.minutes);
    let problem_effects = consequences
        .unwrap_or_default()
        .into_iter()
        .find(|row| row.character_id == character.id && row.settlement_id == id)
        .unwrap_or(BackendLocalProblemTradeEffect {
            character_id: character.id,
            settlement_id: id.clone(),
            buy_bps: 0,
            sell_penalty_bps: 0,
        });
    Html(
        live_merchant_shop_page(
            settlement,
            character,
            inventory,
            &items,
            &food_lots.unwrap_or_default(),
            &party_members,
            equip.first(),
            &personal_targets,
            &party_targets,
            &pooled,
            shop,
            shared_language,
            problem_effects.buy_bps,
            problem_effects.sell_penalty_bps,
            &conditions.unwrap_or_default(),
            smiths.unwrap_or_default().first(),
            &orders.unwrap_or_default(),
            now_minutes,
            encumbrance.personal,
            encumbrance.party,
            inn_rest_default,
            inn_soap_preview,
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
    required_service: adventuresim_core::settlement_economy::SettlementActionService,
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
    if !settlement_action_service_available(&settlement.economy, required_service) {
        return Html(
            crate::templates::strategic_notice_page(
                "Service unavailable",
                "This settlement does not offer that service.",
                &format!("/locations/settlement/{}", settlement.id),
                "Return to settlement",
                None,
            )
            .into_string(),
        );
    }

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
    let (party_members, items, food_lots, limbs, stats, condition, equipment_recovery) = tokio::join!(
        get_active_party_members(&state, active_character_ref),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
        state.db.query::<FoodLot>("SELECT * FROM food_lot"),
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
            &food_lots.unwrap_or_default(),
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
    let attributes_sql =
        format!("SELECT * FROM character_attributes WHERE character_id = {character_id}");
    let settlement_literal = sql_string_literal(settlement_id);
    let orders_sql = format!(
        "SELECT * FROM repair_order WHERE owner_character_id = {character_id} AND settlement_id = {settlement_literal}"
    );
    let time_sql = format!("SELECT * FROM character_time WHERE character_id = {character_id}");
    let (conditions, skills, attributes, orders, times) = tokio::join!(
        state
            .db
            .query::<ItemCondition>("SELECT * FROM item_condition"),
        state.db.query::<CharacterSkills>(&skills_sql),
        state.db.query::<CharacterAttributes>(&attributes_sql),
        state.db.query::<RepairOrder>(&orders_sql),
        state.db.query::<CharacterTime>(&time_sql),
    );
    let skills = skills.unwrap_or_default();
    let attributes = attributes.unwrap_or_default();
    let skill = skills
        .first()
        .zip(attributes.first())
        .map(|(skills, attributes)| {
            let arm_agility = (attributes.left_arm_agility + attributes.right_arm_agility) * 0.5;
            Skill::Smithing
                .capped_rank_for_aptitude(skills.smithing_hours, arm_agility)
                .floor() as u8
        })
        .unwrap_or_default()
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
    let food_lots = state
        .db
        .query::<FoodLot>("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    InventoryEncumbranceSummaries {
        personal: personal_encumbrance(
            active_character.id,
            active_inventory,
            items,
            &food_lots,
            &rows,
        ),
        party: include_party
            .then(|| party_encumbrance(members, &all_inventories, pooled, items, &food_lots, &rows))
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
    food_lots: &[FoodLot],
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
        .map(|row| {
            food_lots
                .iter()
                .find(|lot| lot.inventory_item_id == Some(row.id))
                .map_or_else(
                    || item_stack_weight_kg(&row.item_id, row.qty, items),
                    |lot| lot.mass_kg.max(0.0),
                )
        })
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
    food_lots: &[FoodLot],
    rows: &EncumbranceRows,
) -> EncumbranceSummary {
    let member_summary = members.iter().filter(|member| member.alive).fold(
        EncumbranceSummary::default(),
        |summary, member| {
            summary.combined(personal_encumbrance(
                member.id,
                inventories,
                items,
                food_lots,
                rows,
            ))
        },
    );
    let pooled_weight = pooled
        .iter()
        .map(|row| {
            food_lots
                .iter()
                .find(|lot| lot.party_inventory_item_id == Some(row.id))
                .map_or_else(
                    || item_stack_weight_kg(&row.item_id, row.quantity, items),
                    |lot| lot.mass_kg.max(0.0),
                )
        })
        .sum::<f32>();
    member_summary.combined(EncumbranceSummary::new(pooled_weight, 0.0))
}

async fn get_active_character(
    state: &AppState,
    character_id: Option<u64>,
) -> Option<(Character, Vec<InventoryItem>)> {
    let character_id = character_id?;
    let inventory_sql = format!("SELECT * FROM inventory_item WHERE character_id = {character_id}");
    let (character, inventory) = tokio::join!(
        super::data::character(state, character_id),
        state.db.query::<InventoryItem>(&inventory_sql),
    );
    let character = character.ok().flatten()?;
    let inventory = inventory.unwrap_or_default();
    Some((character, inventory))
}

fn camp_entry_redirect(has_party: bool, has_camp: bool) -> Option<&'static str> {
    (!has_party || !has_camp).then_some("/")
}

#[cfg(test)]
mod camp_page_model_tests {
    use super::camp_entry_redirect;

    #[test]
    fn camp_page_model_requires_selected_party_and_camp_projection() {
        assert_eq!(camp_entry_redirect(false, false), Some("/"));
        assert_eq!(camp_entry_redirect(true, false), Some("/"));
        assert_eq!(camp_entry_redirect(true, true), None);
    }
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

pub(crate) async fn get_combat_training_profile(
    state: &AppState,
    character_id: u64,
) -> CombatTrainingProfile {
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
                weapons: item.weapon_skills.core(),
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
    if let Some(actor) = active_character {
        let addresses_sql = format!(
            "SELECT * FROM backend_social_addresses WHERE actor_id = {}",
            actor.id
        );
        let automatic_sql = format!(
            "SELECT * FROM backend_automatic_social_chats WHERE actor_id = {}",
            actor.id
        );
        let source_lookups = members.iter().map(|member| async move {
            state
                .db
                .query::<CharacterMoraleSource>(&format!(
                    "SELECT * FROM character_morale_source WHERE character_id = {}",
                    member.id
                ))
                .await
                .unwrap_or_default()
        });
        let (source_groups, addresses, automatic_chats) = tokio::join!(
            join_all(source_lookups),
            state.db.query::<SocialAddress>(&addresses_sql),
            state.db.query::<AutomaticSocialChat>(&automatic_sql),
        );
        let sources: Vec<_> = source_groups.into_iter().flatten().collect();
        let successful = addresses.unwrap_or_default();
        let automatic_targets: HashSet<u64> = automatic_chats
            .unwrap_or_default()
            .into_iter()
            .filter(|preference| preference.enabled && preference.actor_id == actor.id)
            .map(|preference| preference.target_id)
            .collect();
        for member in &mut members {
            let colocated = member.id == actor.id
                || (member.current_settlement_id == actor.current_settlement_id
                    && member.current_case_site_id == actor.current_case_site_id);
            if !member.alive || !actor.alive || !colocated {
                continue;
            }
            member.social_notification_count =
                adventuresim_core::social::unaddressed_social_source_count(
                    actor.id,
                    member.id,
                    sources
                        .iter()
                        .filter(|source| source.character_id == member.id)
                        .map(|source| (source.id.as_str(), source.kind.as_str(), source.magnitude)),
                    successful.iter().map(|address| {
                        (
                            address.actor_id,
                            address.target_id,
                            address.source_id.as_str(),
                            true,
                        )
                    }),
                );
            member.automatic_social_chat_enabled = automatic_targets.contains(&member.id);
        }
    }
    members.sort_by_key(|member| (Some(member.id) != leader_id, member.id));
    members
}

#[cfg(test)]
mod social_notification_query_tests {
    use super::{
        social_action_blocked_by_actor, social_action_error_feedback, social_feedback,
        valid_casual_chat_action_id, valid_casual_chat_minutes,
    };
    use adventuresim_core::social::SocialActionKind;

    #[test]
    fn social_action_feedback_is_allowlisted_and_describes_cooldowns_and_results() {
        assert_eq!(
            social_action_error_feedback(
                "SpacetimeDB error: That approach needs time before it can be tried again"
            ),
            "cooldown"
        );
        assert_eq!(
            social_action_error_feedback("transport details that must not reach the browser"),
            "unavailable"
        );
        assert_eq!(
            social_feedback(Some("addressed")).unwrap().message,
            "This concern is addressed."
        );
        assert!(social_feedback(Some("made-up")).is_none());
    }

    #[test]
    fn casual_chat_forms_validate_stable_opaque_action_ids() {
        assert!(valid_casual_chat_minutes(15));
        assert!(valid_casual_chat_minutes(480));
        assert!(!valid_casual_chat_minutes(14));
        assert!(!valid_casual_chat_minutes(481));
        assert!(valid_casual_chat_action_id("chat-19af-2"));
        assert!(!valid_casual_chat_action_id(""));
        assert!(!valid_casual_chat_action_id("chat:19af"));

        let source = include_str!("settlements.rs");
        let handler = source
            .split("async fn chat_with_party_member")
            .nth(1)
            .and_then(|tail| tail.split("fn social_action_error_feedback").next())
            .expect("party chat handler");
        assert!(handler.contains("json!(&form.action_id)"));
        assert!(!handler.contains("SystemTime::now"));
    }

    #[test]
    fn party_rail_queries_current_party_sources_and_compact_addresses_only() {
        let source = include_str!("settlements.rs");
        let loader = source
            .split("pub(crate) async fn get_active_party_members")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) async fn soap_rest_preview").next())
            .expect("party member loader");
        assert!(loader.contains("SELECT * FROM character_morale_source WHERE character_id = {}"));
        assert!(
            !loader.contains(
                "query::<CharacterMoraleSource>(\"SELECT * FROM character_morale_source\")"
            )
        );
        assert!(loader.contains("SELECT * FROM backend_social_addresses WHERE actor_id = {}"));
        assert!(
            loader.contains("SELECT * FROM backend_automatic_social_chats WHERE actor_id = {}")
        );
        assert!(!loader.contains("backend_social_interactions"));
    }

    #[test]
    fn social_actor_action_visibility_uses_shared_policy_and_fails_closed() {
        assert!(social_action_blocked_by_actor(
            false,
            None,
            SocialActionKind::LightenMood
        ));
        let mut personality = crate::spacetimedb::CharacterPersonality::neutral(1);
        assert!(!social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::LightenMood
        ));
        personality.mirth = crate::spacetimedb::Mirth::Grave;
        assert!(social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::LightenMood
        ));
        assert!(!social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::Flirt
        ));
        personality.courtship = crate::spacetimedb::Courtship::Proper;
        assert!(social_action_blocked_by_actor(
            true,
            Some(&personality),
            SocialActionKind::Flirt
        ));
        assert!(social_action_blocked_by_actor(
            true,
            None,
            SocialActionKind::Flirt
        ));
    }

    #[test]
    fn prayer_preview_uses_private_actor_study_and_fails_closed() {
        let source = include_str!("settlements.rs");
        let handler = source
            .split("async fn party_social")
            .nth(1)
            .and_then(|tail| tail.split("struct SocialActionForm").next())
            .expect("social dialog handler");
        assert!(handler.contains("private actor personality query failed closed"));
        assert!(handler.contains("private Religion knowledge query failed closed"));
        assert!(handler.contains("skills.religion_hours.direct(religion) <= 0.0"));
        assert!(!handler.contains("maximum_effective"));
        assert!(handler.contains("Their religion is unknown."));
        assert!(handler.contains("They profess no religion."));
    }
}

pub(crate) async fn soap_rest_preview(
    state: &AppState,
    members: &[Character],
    party_id: Option<&str>,
) -> SoapRestPreview {
    let (filth, personal, shared, personal_amounts, party_amounts, definitions, personalities) = tokio::join!(
        state
            .db
            .query::<CharacterFilth>("SELECT * FROM character_filth"),
        state
            .db
            .query::<InventoryItem>("SELECT * FROM inventory_item"),
        state
            .db
            .query::<PartyInventoryItem>("SELECT * FROM party_inventory_item"),
        state
            .db
            .query::<InventoryItemAmount>("SELECT * FROM inventory_item_amount"),
        state
            .db
            .query::<PartyItemAmount>("SELECT * FROM party_item_amount"),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
        state
            .db
            .query::<CharacterPersonality>("SELECT * FROM backend_character_personalities"),
    );
    let personal = personal.unwrap_or_default();
    let shared = shared.unwrap_or_default();
    let personal_amounts = personal_amounts.unwrap_or_default();
    let party_amounts = party_amounts.unwrap_or_default();
    let mut preview = calculate_soap_rest_preview(
        members,
        &filth.unwrap_or_default(),
        &personal,
        &shared,
        &personal_amounts,
        &party_amounts,
        party_id,
    );
    calculate_rest_supply_availability(
        &mut preview,
        members,
        &personal,
        &shared,
        &personal_amounts,
        &party_amounts,
        &definitions.unwrap_or_default(),
        &personalities.unwrap_or_default(),
        party_id,
    );
    preview
}

fn calculate_rest_supply_availability(
    preview: &mut SoapRestPreview,
    members: &[Character],
    personal: &[InventoryItem],
    shared: &[PartyInventoryItem],
    personal_amounts: &[InventoryItemAmount],
    party_amounts: &[PartyItemAmount],
    definitions: &[ItemDefinition],
    personalities: &[CharacterPersonality],
    party_id: Option<&str>,
) {
    const SOAP_ITEM_ID: &str = "soft_soap";
    let living_ids = members
        .iter()
        .filter(|member| member.alive)
        .map(|member| member.id)
        .collect::<std::collections::BTreeSet<_>>();
    let is_temperate = |character_id| {
        personalities
            .iter()
            .find(|personality| personality.character_id == character_id)
            .is_some_and(|personality| {
                personality.temperance == crate::spacetimedb::Temperance::Temperate
            })
    };
    let alcoholic_ids = definitions
        .iter()
        .filter(|item| {
            item.alcohol_potable
                && item.alcohol_serving_ml > 0
                && item.alcohol_abv_basis_points > 0
                && !item.alcohol_disinfectant_focused
        })
        .map(|item| item.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    let personal_soap = personal
        .iter()
        .filter(|stack| living_ids.contains(&stack.character_id) && stack.item_id == SOAP_ITEM_ID)
        .map(|stack| {
            personal_amounts
                .iter()
                .find(|state| state.inventory_item_id == stack.id)
                .map_or(0, |state| {
                    state.remaining_milliunits
                        / (1_000_000 / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                })
        })
        .sum::<u32>();
    let shared_soap = party_id.map_or(0, |party_id| {
        shared
            .iter()
            .filter(|stack| stack.party_id == party_id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| {
                party_amounts
                    .iter()
                    .find(|state| state.party_inventory_item_id == stack.id)
                    .map_or(0, |state| {
                        state.remaining_milliunits
                            / (1_000_000
                                / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                    })
            })
            .sum::<u32>()
    });
    preview.available_units = personal_soap.saturating_add(shared_soap);

    let personal_alcohol = personal.iter().any(|stack| {
        living_ids.contains(&stack.character_id)
            && personal_amounts
                .iter()
                .any(|state| state.inventory_item_id == stack.id && state.remaining_milliunits > 0)
            && alcoholic_ids.contains(stack.item_id.as_str())
    });
    let personal_drink = personal.iter().any(|stack| {
        living_ids.contains(&stack.character_id)
            && !is_temperate(stack.character_id)
            && personal_amounts
                .iter()
                .any(|state| state.inventory_item_id == stack.id && state.remaining_milliunits > 0)
            && alcoholic_ids.contains(stack.item_id.as_str())
    });
    let shared_alcohol = party_id.is_some_and(|party_id| {
        shared.iter().any(|stack| {
            stack.party_id == party_id
                && party_amounts.iter().any(|state| {
                    state.party_inventory_item_id == stack.id && state.remaining_milliunits > 0
                })
                && alcoholic_ids.contains(stack.item_id.as_str())
        })
    });
    let has_non_temperate_member = living_ids
        .iter()
        .any(|character_id| !is_temperate(*character_id));
    preview.alcohol_available = personal_alcohol || shared_alcohol;
    preview.alcohol_will_be_consumed =
        personal_drink || (shared_alcohol && has_non_temperate_member);
}

fn calculate_soap_rest_preview(
    members: &[Character],
    filth: &[CharacterFilth],
    personal: &[InventoryItem],
    shared: &[PartyInventoryItem],
    personal_amounts: &[InventoryItemAmount],
    party_amounts: &[PartyItemAmount],
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
        let needed = amount;
        let available = personal
            .iter()
            .filter(|stack| stack.character_id == member.id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| {
                personal_amounts
                    .iter()
                    .find(|state| state.inventory_item_id == stack.id)
                    .map_or(0, |state| {
                        state.remaining_milliunits
                            / (1_000_000
                                / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                    })
            })
            .sum::<u32>();
        let used = needed.min(available);
        personal_units = personal_units.saturating_add(used);
        need_after_personal = need_after_personal.saturating_add(needed.saturating_sub(used));
    }
    let shared_available = party_id.map_or(0, |party_id| {
        shared
            .iter()
            .filter(|stack| stack.party_id == party_id && stack.item_id == SOAP_ITEM_ID)
            .map(|stack| {
                party_amounts
                    .iter()
                    .find(|state| state.party_inventory_item_id == stack.id)
                    .map_or(0, |state| {
                        state.remaining_milliunits
                            / (1_000_000
                                / u32::from(adventuresim_core::filth::SOAP_CLEANSING_CAPACITY))
                    })
            })
            .sum()
    });
    let shared_units = need_after_personal.min(shared_available);
    SoapRestPreview {
        total_units: personal_units.saturating_add(shared_units),
        personal_units,
        shared_units,
        ..SoapRestPreview::default()
    }
}

#[cfg(test)]
mod rest_form_tests {
    use adventuresim_core::strategic_time::{is_walking_time, minutes_until_next_walking_start};

    use super::{
        RestForm, calculate_rest_supply_availability, calculate_soap_rest_preview,
        camp_continue_block_reason, rest_spending_breakdown, safe_rest_error,
        settlement_rest_minutes, travel_rest_minutes,
    };
    use crate::spacetimedb::{
        Character, CharacterFilth, CharacterPersonality, Conscience, Conviction, Drive,
        FilthOrigin, FilthSubstance, Hygiene, InventoryItem, InventoryItemAmount, ItemDefinition,
        Nerve, Outlook, PartyInventoryItem, PartyItemAmount, SelfRegard, Sociability, Temperance,
    };
    use crate::templates::settlement::SoapRestPreview;

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
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 30,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    fn personality(character_id: u64, temperance: Temperance) -> CharacterPersonality {
        CharacterPersonality {
            character_id,
            nerve: Nerve::Neutral,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Neutral,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
            hygiene: Hygiene::Neutral,
            temperance,
            ..CharacterPersonality::neutral(character_id)
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
        let shared = [
            PartyInventoryItem {
                id: 2,
                party_id: "party".into(),
                item_id: "soft_soap".into(),
                quantity: 1,
            },
            PartyInventoryItem {
                id: 3,
                party_id: "party".into(),
                item_id: "soft_soap".into(),
                quantity: 1,
            },
        ];
        let personal_amounts = [InventoryItemAmount {
            inventory_item_id: 1,
            remaining_milliunits: 1_000_000,
        }];
        let party_amounts = [
            PartyItemAmount {
                party_inventory_item_id: 2,
                remaining_milliunits: 1_000_000,
            },
            PartyItemAmount {
                party_inventory_item_id: 3,
                remaining_milliunits: 1_000_000,
            },
        ];
        let preview = calculate_soap_rest_preview(
            &[member(1), member(2)],
            &filth,
            &personal,
            &shared,
            &personal_amounts,
            &party_amounts,
            Some("party"),
        );
        assert_eq!(preview.personal_units, 25);
        assert_eq!(preview.shared_units, 31);
        assert_eq!(preview.total_units, 56);
    }

    #[test]
    fn rest_supply_availability_greys_alcohol_for_temperate_characters() {
        let supplies = [
            InventoryItem {
                id: 1,
                character_id: 1,
                item_id: "soft_soap".into(),
                qty: 1,
            },
            InventoryItem {
                id: 2,
                character_id: 1,
                item_id: "table_wine".into(),
                qty: 1,
            },
        ];
        let alcohol = ItemDefinition {
            id: "table_wine".into(),
            alcohol_serving_ml: 250,
            alcohol_abv_basis_points: 1_200,
            alcohol_potable: true,
            ..ItemDefinition::default()
        };
        let amounts = [
            InventoryItemAmount {
                inventory_item_id: 1,
                remaining_milliunits: 1_000_000,
            },
            InventoryItemAmount {
                inventory_item_id: 2,
                remaining_milliunits: 1_000_000,
            },
        ];
        let mut preview = SoapRestPreview::default();
        calculate_rest_supply_availability(
            &mut preview,
            &[member(1)],
            &supplies,
            &[],
            &amounts,
            &[],
            &[alcohol.clone()],
            &[personality(1, Temperance::Temperate)],
            Some("party"),
        );
        assert_eq!(preview.available_units, 25);
        assert!(preview.alcohol_available);
        assert!(!preview.alcohol_will_be_consumed);

        calculate_rest_supply_availability(
            &mut preview,
            &[member(1)],
            &supplies,
            &[],
            &amounts,
            &[],
            &[alcohol],
            &[personality(1, Temperance::Neutral)],
            Some("party"),
        );
        assert!(preview.alcohol_will_be_consumed);
    }

    #[test]
    fn exact_hours_preserve_minutes_and_enforce_one_day() {
        assert_eq!(
            settlement_rest_minutes(&form("24:01", "hours", Some(1_441))),
            Ok(1_441)
        );
        assert_eq!(
            settlement_rest_minutes(&form("36:32", "hours", Some(2_192))),
            Ok(2_192)
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
        assert_eq!(settlement_rest_minutes(&form("1", "days", None)), Ok(1_440));
        assert_eq!(
            settlement_rest_minutes(&form("2", "days", Some(1_441))),
            Ok(2_880)
        );
        assert!(settlement_rest_minutes(&form("0", "days", None)).is_err());
        assert!(settlement_rest_minutes(&form("1.5", "days", None)).is_err());
        assert_eq!(
            settlement_rest_minutes(&form("365", "days", None)),
            Ok(365 * 1_440)
        );
        assert!(settlement_rest_minutes(&form("366", "days", None)).is_err());
    }

    #[test]
    fn rest_spending_itemizes_full_board_and_other_downtime_costs() {
        assert_eq!(rest_spending_breakdown(4, true, 1_440), (2, 2));
        assert_eq!(rest_spending_breakdown(10, true, 2_880), (4, 6));
        assert_eq!(rest_spending_breakdown(2, false, 1_440), (0, 2));
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
    fn rest_failures_have_safe_visible_prose() {
        assert_eq!(
            safe_rest_error("Not enough coin to pay for the inn stay"),
            "You do not have enough coin for that inn stay."
        );
        assert!(!safe_rest_error("private injury authority 123").contains("123"));
    }

    #[test]
    fn rest_form_extraction_failures_are_logged_without_request_contents() {
        let source = include_str!("settlements.rs");
        let handler = source
            .split("async fn rest(")
            .nth(1)
            .and_then(|tail| tail.split("async fn query_single").next())
            .expect("settlement rest handler");
        let extraction = handler
            .split("let form = match form")
            .nth(1)
            .and_then(|tail| tail.split("let settlements").next())
            .expect("form extraction branch");
        assert!(extraction.contains("tracing::warn!("));
        for field in [
            "character_id",
            "requested_settlement_id_length = id.len()",
            "service = kind.as_str()",
            "rejection_status = %error.status()",
            "error = %error",
        ] {
            assert!(extraction.contains(field), "{field}");
        }
        assert!(extraction.contains("return error.into_response()"));
        assert!(!extraction.contains("requested_settlement_id = %id"));
        assert!(!extraction.contains("form.duration"));
        assert!(!extraction.contains("request body"));
        assert!(
            handler.find("let Some(character_id)") < handler.find("let form = match form"),
            "authentication precedes malformed-form warning"
        );
    }

    #[test]
    fn rest_duration_validation_logs_bounded_metadata_before_safe_notice() {
        let source = include_str!("settlements.rs");
        let handler = source
            .split("async fn rest(")
            .nth(1)
            .and_then(|tail| tail.split("async fn query_single").next())
            .expect("settlement rest handler");
        let validation = handler
            .split("let requested_minutes = match settlement_rest_minutes(&form)")
            .nth(1)
            .and_then(|tail| tail.split("let before_character").next())
            .expect("rest duration validation branch");
        let warning = validation.find("tracing::warn!(").expect("warning");
        let safe_notice = validation
            .find("strategic_notice_page(")
            .expect("safe notice");
        assert!(warning < safe_notice);
        for field in [
            "character_id",
            "requested_settlement_id = %id",
            "requested_minutes = ?form.requested_minutes",
            "at_inn",
            "service = kind.as_str()",
            "duration_length = form.duration.len()",
            "reason = message",
        ] {
            assert!(validation[..safe_notice].contains(field), "{field}");
        }
        for category in [
            "\"hours\" => \"hours\"",
            "\"days\" => \"days\"",
            "_ => \"unknown\"",
        ] {
            assert!(validation[..safe_notice].contains(category), "{category}");
        }
        assert!(!validation.contains("duration = %form.duration"));
        assert!(!validation.contains("unit = %form.unit"));
    }

    #[test]
    fn rest_reducer_rejections_are_logged_before_the_sanitized_notice() {
        let source = include_str!("settlements.rs");
        let handler = source
            .split("async fn rest(")
            .nth(1)
            .and_then(|tail| tail.split("async fn query_single").next())
            .expect("settlement rest handler");
        let reducer_error = handler
            .split("if let Err(error)")
            .nth(1)
            .expect("rest reducer error branch");
        let warning = reducer_error.find("tracing::warn!(").expect("warning");
        let sanitization = reducer_error
            .find("safe_rest_error(&error.to_string())")
            .expect("safe response");
        assert!(warning < sanitization);
        for field in [
            "character_id",
            "requested_settlement_id = %id",
            "character_settlement_id",
            "requested_minutes",
            "at_inn",
            "service = kind.as_str()",
            "error = %error",
        ] {
            assert!(reducer_error[..sanitization].contains(field), "{field}");
        }
        assert!(handler.contains("character.current_settlement_id.as_deref()"));
        assert!(handler.contains(".unwrap_or(\"<none>\")"));
        assert!(reducer_error.contains("settlement rest reducer rejected request"));
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

    #[test]
    fn unresolved_encounters_override_walking_time_for_camp_continuation() {
        assert_eq!(
            camp_continue_block_reason(Some("awaiting_choice"), true),
            Some("Resolve the encounter above before continuing travel.")
        );
        assert_eq!(camp_continue_block_reason(Some("resolved"), true), None);
        assert_eq!(camp_continue_block_reason(None, true), None);
        assert_eq!(
            camp_continue_block_reason(None, false),
            Some("Rest until the planned walking window begins.")
        );
    }
}

#[cfg(test)]
mod herbalist_tests {
    use super::living_party_members;
    use crate::spacetimedb::Character;

    fn member(id: u64, alive: bool) -> Character {
        Character {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
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
}

#[cfg(test)]
mod encumbrance_tests {
    use super::{
        ENCUMBRANCE_QUERY_CONCURRENCY, EncumbranceRows, encumbrance_query_ids, party_encumbrance,
        personal_encumbrance,
    };
    use crate::spacetimedb::{
        Character, CharacterAttributes, CharacterCondition, CharacterLimbs, CharacterNeeds,
        FoodLot, FoodPreparation, InventoryItem, ItemDefinition, PartyInventoryItem,
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
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    fn rows() -> EncumbranceRows {
        EncumbranceRows {
            attributes: vec![CharacterAttributes {
                character_id: 1,
                endurance: 0.0,
                immunity: 0.0,
                gut: 0.0,
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
        let summary = personal_encumbrance(1, &inventory, &[item("sword", 4.0)], &[], &rows());
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
            &[],
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
            &[],
            &EncumbranceRows::default(),
        );
        assert_eq!(summary.burden_kg, 0.0);
        assert_eq!(summary.capacity_kg, 0.0);
        assert_eq!(summary.penalty_fraction(), 1.0);
    }

    #[test]
    fn linked_food_lot_mass_replaces_static_item_weight() {
        let inventory = vec![InventoryItem {
            id: 40,
            character_id: 1,
            item_id: "cooked_meal".into(),
            qty: 1,
        }];
        let lots = vec![FoodLot {
            id: 5,
            inventory_item_id: Some(40),
            party_inventory_item_id: None,
            display_name: "Large stew".into(),
            preparation: FoodPreparation::Stewed,
            ingredient_item_ids: vec!["raw_venison".into()],
            ingredient_quantities: vec![25.0],
            salty_kg: 0.0,
            spicy_kg: 0.0,
            sweet_kg: 0.0,
            sour_kg: 0.0,
            savory_kg: 10.0,
            quality: 3,
            mass_kg: 25.0,
            nutrition_kcal: 10_000.0,
            total_value: 25.0,
            created_at_minute: 1,
        }];
        let summary =
            personal_encumbrance(1, &inventory, &[item("cooked_meal", 0.0)], &lots, &rows());
        assert_eq!(summary.burden_kg, 97.5);

        let mut partial = lots[0].clone();
        partial.mass_kg = 6.25;
        let summary = personal_encumbrance(
            1,
            &inventory,
            &[item("cooked_meal", 0.0)],
            &[partial],
            &rows(),
        );
        assert_eq!(summary.burden_kg, 78.75);
    }
}
