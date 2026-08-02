use adventuresim_core::{
    durability::RepairService,
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
use maud::Markup;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, Deserialize)]
struct BuildingQuery {
    building: Option<String>,
    cook: Option<bool>,
    herbalism: Option<bool>,
    corpse: Option<String>,
    medical: Option<String>,
    forage: Option<bool>,
    forage_receipt: Option<String>,
    forage_error: Option<String>,
    social_feedback: Option<String>,
}

impl BuildingQuery {
    fn herbalism(&self) -> bool {
        self.herbalism.unwrap_or(false)
    }
    fn valid_for<'a>(&'a self, location: &LocationView) -> Option<&'a str> {
        self.building
            .as_deref()
            .and_then(|building| location.valid_building(building))
    }

    fn append_to_location(&self, location: &LocationView, path: String) -> String {
        self.valid_for(location).map_or_else(
            || path.clone(),
            |building| {
                format!(
                    "{path}{}building={building}",
                    if path.contains('?') { "&" } else { "?" }
                )
            },
        )
    }

    async fn append_to(
        &self,
        state: &AppState,
        kind: &str,
        id: &str,
        path: String,
    ) -> String {
        match resolve_location(state, kind, id).await {
            LocationLookup::Found(location) => self.append_to_location(&location, path),
            LocationLookup::NotFound | LocationLookup::Unavailable => path,
        }
    }

    fn cooking(&self) -> bool {
        self.cook == Some(true)
    }
}

#[cfg(test)]
mod building_query_tests {
    use super::{BuildingQuery, SETTLEMENTS_SOURCE, merchant_service_location};

    #[test]
    fn building_query_is_closed_and_preserved_on_redirects() {
        let economy = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        let (_organization, chapter) = adventuresim_core::organization::catalog()
            .organizations
            .iter()
            .find_map(|organization| {
                organization.chapters.iter().find(|chapter| {
                    adventuresim_core::organization::chapter_has_standalone_building(
                        organization,
                        chapter,
                        &economy,
                    )
                }).map(|chapter| (organization, chapter))
            })
            .expect("standalone catalog chapter");
        let location = crate::templates::settlement::LocationView {
            kind: crate::templates::settlement::LocationKind::Settlement,
            id: chapter.settlement_id.clone(),
            name: "Place".into(),
            religion_id: None,
            category: Some(crate::spacetimedb::SettlementCategory::Village),
            economy: Some(economy),
            active_building: None,
        };
        let valid = BuildingQuery {
            building: Some("inn".into()),
            ..Default::default()
        };
        assert_eq!(valid.valid_for(&location), Some("inn"));
        let unavailable = BuildingQuery { building: Some("books".into()), ..Default::default() };
        assert_eq!(unavailable.valid_for(&location), None);
        let organization_query = BuildingQuery {
            building: Some(chapter.location_id.clone()),
            ..Default::default()
        };
        assert_eq!(organization_query.valid_for(&location), Some(chapter.location_id.as_str()));
        if let Some(foreign) = adventuresim_core::organization::catalog()
            .organizations
            .iter()
            .flat_map(|organization| &organization.chapters)
            .find(|foreign| {
                foreign.settlement_id != location.id
                    && adventuresim_core::organization::organization_chapter_at(
                        &location.id,
                        &foreign.location_id,
                    )
                    .is_none()
            })
        {
            let foreign_query = BuildingQuery {
                building: Some(foreign.location_id.clone()),
                ..Default::default()
            };
            assert_eq!(foreign_query.valid_for(&location), None);
        }
        assert_eq!(
            valid.append_to_location(&location, "/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1?building=inn"
        );
        assert_eq!(
            valid.append_to_location(&location, "/locations/settlement/x/party/1?cook=true".into()),
            "/locations/settlement/x/party/1?cook=true&building=inn"
        );
        let non_service = BuildingQuery {
            building: Some("public-square".into()),
            ..Default::default()
        };
        assert_eq!(
            non_service.append_to_location(&location, "/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1?building=public-square"
        );
        let invalid = BuildingQuery {
            building: Some("../religion".into()),
            ..Default::default()
        };
        assert_eq!(invalid.valid_for(&location), None);
        assert_eq!(
            invalid.append_to_location(&location, "/locations/settlement/x/party/1".into()),
            "/locations/settlement/x/party/1"
        );
    }

    #[test]
    fn merchant_offer_routes_accept_only_bound_storefront_services() {
        let source = SETTLEMENTS_SOURCE;
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
        let source = SETTLEMENTS_SOURCE.replace('\r', "");
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

use super::inventory_forms::{
    DiscardInventoryForm, MerchantOfferForm, PartyOfferForm, PartyPoolTransferForm,
};
use super::redirect_to_local;
use super::travel::{
    CaseSiteKnowledgePresentation, TravelDestination, TravelForm, TravelProvisionForecast,
    active_contract_tooltip, connected_destinations, populate_itinerary_forecasts,
};
use super::{
    AppState, PartyAction, PartyActionOutcome, SocialActionId, SocialDuration,
    execute_or_request_party_action,
};
use crate::session::Session;
use crate::spacetimedb::sql_string_literal;
use crate::spacetimedb::{
    AlcoholConsumption, AutomaticSocialChat, BackendCaseSitePin, BackendChallenge,
    BackendCharacterRelationshipStatus, BackendCharacterResidenceStatus, BackendCorpse,
    BackendFamilyChild,
    BackendLocalProblemTradeEffect, BackendPhysiologyAdministration, BackendPhysiologyChart,
    BackendRoadChallenge, BackendContextCharacter, BackendContextDisposition, Character, CharacterAffinity, CharacterAttributes, CharacterCapability,
    CharacterCondition, CharacterEquipmentGraph, CharacterEquippedItem, CharacterFamiliarity,
    CharacterFilth, CharacterLimbs, CharacterMoraleSource, CharacterNeeds, CharacterPersonality,
    CharacterSettlementReputation, CharacterSkills, CharacterStats, CharacterStrategicCondition,
    CharacterTime, CharacterTrainingSchedule, ContractPresentation, ContractPresentationStatus,
    EquipmentAnchorKind, EquipmentAttachmentTarget, EquipmentOccupancy, FoodLot, InventoryItem,
    InventoryItemAmount, InventoryQuantityTarget, ItemCondition, ItemDefinition, ItemKind,
    ItemSlot, LimbInjury, LimbRegion, Party, PartyInventoryItem, PartyJourney,
    PartyJourneyItinerary, PartyJourneyRoute, PartyMember, PartyRecruitmentRole, PartyStake,
    RecruitmentOffer, RecruitmentOfferStatus, RecruitmentRequirements, ReligiousDemand,
    RepairOrder, ResidenceTier, RetainedProjectile, ScheduleAllocation, Settlement,
    SettlementAlias, SettlementDescription, SettlementResidenceOffer, SettlementSmith,
    SocialAddress, SocialBelief, SocialChatOutcome, StrategicEncounter, TravelEdge,
};
use crate::templates::settlement::{
    ActivityPreviewRates, CampTravelDestination, ChildPresentation, LocationKind, LocationView,
    MerchantShop, RelationshipPresentation, RestSummary, SoapRestPreview, SocialPresentation,
    WeddingPresentation, camp_page, live_merchant_shop_page, merchants_page, party_discard_page,
    party_inventory_page, party_personal_page, party_pool_page, party_social_dialog,
    party_stats_page, religion_page, rest_default_minutes, rest_result_page, settlement_map_page,
    settlement_overview_page, settlement_residence_page, settlement_resident_location_page,
    surgery_dialog,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settlements/{id}", get(show_settlement))
        .route(
            "/settlements/{id}/places/{place}",
            get(settlement_resident_place),
        )
        .route(
            "/settlements/{id}/residences/{action}/{tier}",
            post(change_residence),
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
            "/camp/errantry-road-challenge",
            post(resolve_errantry_road_challenge),
        )
        .route(
            "/camp/travel-configuration",
            post(update_camp_travel_configuration),
        )
        .route("/camp/continue", post(continue_camp_travel))
        .route("/camp/encounter", post(resolve_camp_encounter))
        .route("/camp/counterparty/contact", post(contact_camp_counterparty))
        .route("/camp/counterparty/surrender",post(surrender_camp_counterparty))
        .route("/camp/counterparty/bandage", post(bandage_camp_counterparty))
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
            "/locations/settlement/{id}/party/{character_id}/organization-presentation/{organization_id}",
            post(update_organization_presentation),
        )
        .route(
            "/locations/settlement/{id}/party/{character_id}/organization-presentation-none",
            post(clear_presented_organization),
        )
        .route(
            "/locations/{kind}/{id}/party/{character_id}/cook",
            post(cook_food),
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
            "/locations/{kind}/{id}/party/{character_id}/herbalism",
            post(prepare_herbal_remedy),
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
        .route("/settlements/{id}/books", get(bookstore))
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
