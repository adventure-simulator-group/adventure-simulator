//! Shared settlement service layout and rest/trade rail coordination.

use maud::{Markup, html};

use super::{
    character_details::religion_name,
    chrome::party_portrait_overlay,
    rest::{RestServiceKind, RestSummary, SoapRestPreview, rest_service_menu},
    social::{
        inventory_rail, merchant_offers_rail, npc_description_stage, npc_location_id,
        npc_portrait_strip, settlement_resident_chat_area,
    },
};
use crate::spacetimedb::{CharacterView, FoodLot, InventoryItem, SettlementView};
use crate::templates::{settlement_layout_with_session, sidebar_section};

/// Selected party member stats and biography.
#[expect(
    clippy::too_many_arguments,
    reason = "the service page boundary composes independent settlement, inventory, and recovery projections"
)]
pub(super) fn service_page(
    settlement: &SettlementView,
    service_id: &str,
    title: &str,
    npc_name: &str,
    service_summary: &str,
    active_character: Option<&CharacterView>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    party_members: &[CharacterView],
    logged_in_as: Option<&str>,
    rest_default_minutes: Option<u64>,
    rest_summary: Option<&RestSummary>,
    soap_preview: SoapRestPreview,
) -> Markup {
    let trade_offers: Option<(&str, &[&str])> = match service_id {
        "merchants" => Some((
            "Merchant stock",
            &["Weapon offer", "Armour offer", "Provision offer"],
        )),
        "weapons" => Some((
            "Weapons",
            &["Weapon offer", "Shield offer", "Ammunition offer"],
        )),
        "armor" => Some((
            "Armour",
            &["Head protection", "Torso protection", "Limb protection"],
        )),
        "clothing" => Some((
            "Clothing",
            &["Travel attire", "Cold-weather clothing", "Fine clothing"],
        )),
        "inn" => Some((
            "Inn supplies",
            &["Rations", "Water", "Supplies", "Bed for the night"],
        )),
        _ => None,
    };
    let content = html! {
        aside class=(if service_id == "inn" || service_id == "religion" { "left-sidebar service-left-sidebar" } else { "left-sidebar" }) {
            @if service_id == "inn" {
                div class="service-left-stack" {
                    div class="service-inventory-area" { (merchant_offers_rail("Inn supplies", &["Rations", "Water", "Supplies", "Bed for the night"])) }
                    (rest_service_menu("Inn", &settlement.id, RestServiceKind::Inn, rest_default_minutes, rest_summary, soap_preview))
                }
            } @else if service_id == "religion" {
                div class="service-left-stack" {
                    div class="service-inventory-area" {
                        (sidebar_section("Church services", html! {
                            p title=[active_character.is_some().then_some("Speak with the priest to profess this faith. Renunciation is available from your biography. Shared conviction strengthens allied Command; conflicting conviction penalizes morale.")] {
                                "Faith: " strong { (religion_name(Some(&settlement.religion_id))) }
                            }
                        }))
                    }
                    (rest_service_menu("Temple", &settlement.id, RestServiceKind::Temple, rest_default_minutes, rest_summary, soap_preview))
                }
            } @else if let Some((stock_title, offers)) = trade_offers {
                (merchant_offers_rail(stock_title, offers))
            } @else {
                (sidebar_section("Service", html! {
                    p class="small-copy" { (service_summary) }
                }))
            }
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None))
            (npc_portrait_strip(&settlement.id, npc_location_id(service_id)))
            (npc_description_stage(npc_name, &format!("{title} host and service counter")))
            (settlement_resident_chat_area(title, active_character, &settlement.id, npc_location_id(service_id), Some(service_id)))
        }
        aside class="right-sidebar" {
            @if trade_offers.is_some() {
                (inventory_rail(
                    active_character,
                    inventory,
                    items,
                    food_lots,
                    None,
                    matches!(service_id, "weapons" | "armor" | "clothing"),
                ))
            } @else if service_id == "smith" {
                (inventory_rail(
                    active_character,
                    inventory,
                    items,
                    food_lots,
                    None,
                    true,
                ))
            } @else if service_id == "religion" {
                (inventory_rail(active_character, inventory, items, food_lots, None, false))
            } @else {
                (sidebar_section("Service", html! {
                    p class="small-copy" { (service_summary) }
                }))
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        service_id,
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}
