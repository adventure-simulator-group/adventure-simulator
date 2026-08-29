//! Fireplace cooking and vessel-custody presentation.

use super::{
    chrome::{VisualStageKind, visual_stage},
    trade::*,
};
use crate::spacetimedb::{
    BackendFireplaceDish, BackendFireplaceStation, CatalogItemView, CharacterView, FoodLot,
    InventoryItem, InventoryItemAmount, PartyInventoryItem, PartyItemAmount,
};
use crate::templates::{decorative_game_icon, item_display_name, item_type_icon, sidebar_section};
use adventuresim_stdb_client::CookingMethod;
use maud::{Markup, html};

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub fn fireplace_page(
    title: &str,
    back_href: &str,
    action_base: &str,
    rest_action: &str,
    _active_character: &CharacterView,
    inventory_scope: &str,
    personal_inventory: &[InventoryItem],
    party_inventory: &[PartyInventoryItem],
    personal_amounts: &[InventoryItemAmount],
    party_amounts: &[PartyItemAmount],
    food_lots: &[FoodLot],
    definitions: &[CatalogItemView],
    station: Option<&BackendFireplaceStation>,
    dish: Option<&BackendFireplaceDish>,
    vessel_stations: &[BackendFireplaceStation],
    vessel_dishes: &[BackendFireplaceDish],
    character_minute: u64,
    layout: impl FnOnce(Markup) -> Markup,
) -> Markup {
    let instrument = station.and_then(|row| row.instrument_item_id.as_deref());
    let method = "roast";
    let elapsed = dish.map_or(0, |row| {
        character_minute.saturating_sub(row.started_at_minute)
    });
    let remaining = dish.map_or(1, |row| {
        u64::from(row.target_minutes).saturating_sub(elapsed).max(1)
    });
    let status = dish.map(|row| {
        if elapsed < u64::from(row.target_minutes) {
            "Undercooked"
        } else if elapsed == u64::from(row.target_minutes) {
            "Ready"
        } else {
            match row.method {
                CookingMethod::Stew => "Ready (holding safely)",
                CookingMethod::Roast => "Drying / smoking",
                CookingMethod::PanFry | CookingMethod::Bake => "Burning",
            }
        }
    });
    let cooking_progress = dish.map(|row| {
        let target = u64::from(row.target_minutes).max(1);
        (elapsed.min(target) * 100 / target) as u8
    });
    let cooking_state_icon = dish.map(|row| {
        if elapsed < u64::from(row.target_minutes) {
            "flame"
        } else if elapsed == u64::from(row.target_minutes) {
            "check-mark"
        } else {
            "flame"
        }
    });
    let cooking_phase = dish.map(|row| {
        if elapsed < u64::from(row.target_minutes) {
            "cooking"
        } else if elapsed == u64::from(row.target_minutes) {
            "ready"
        } else {
            "overdue"
        }
    });
    let scope_href = |scope: &str| {
        format!(
            "{action_base}{}inventory_scope={scope}",
            if action_base.contains('?') { "&" } else { "?" }
        )
    };
    let content = html! {
        div class="fireplace-layout" data-cooking-activity[dish.is_none()] data-pan-fat-ratio=(adventuresim_core::food::PAN_FRY_MIN_FAT_MASS_RATIO) {
        aside class="left-sidebar fireplace-station-sidebar" {
            (sidebar_section("Fireplace", html! {
                p class="fireplace-instrument" title="Loose food is always spit-roasted; pots, pans, and ovens cook their own contents." {
                    (decorative_game_icon("campfire"))
                    span { (instrument.map(item_display_name).unwrap_or_else(|| "Roasting spit".into())) }
                }
                @if let Some(row) = dish {
                    div class="fireplace-cook-status" data-fireplace-status data-cooking-phase=(cooking_phase.unwrap_or("cooking")) role="group"
                        aria-label=(format!("{}: {} of {} minutes", status.unwrap_or("Cooking"), elapsed, row.target_minutes)) {
                        span class="fireplace-cook-status-icon" aria-hidden="true" { (decorative_game_icon(cooking_state_icon.unwrap_or("flame"))) }
                        div class="fireplace-cook-progress" role="meter" aria-label="Cooking progress"
                            aria-valuemin="0" aria-valuemax="100" aria-valuenow=(cooking_progress.unwrap_or(0)) {
                            span style=(format!("--cooking-progress:{}%", cooking_progress.unwrap_or(0))) {}
                        }
                        strong { (status.unwrap_or("Cooking")) }
                        time { (elapsed) "/" (row.target_minutes) "m" }
                    }
                    form action=(format!("{action_base}/retrieve")) method="post" {
                        button class="btn btn-primary btn-block" type="submit" { "Retrieve dish" }
                    }
                } @else {
                    p class="small-copy text-muted" { "One loose spit-roasted dish may cook alongside any number of placed vessels. Each vessel uses only its own contents." }
                }
            }))
            @if !vessel_stations.is_empty() {
                (sidebar_section("Vessels over the fire", html! {
                    div data-inventory-browser="fireplace-vessels-left" data-optional-columns="" {
                    @for vessel in vessel_stations {
                        @let object_id = vessel.instrument_object_id.unwrap_or_default();
                        @let vessel_dish = vessel_dishes.iter().find(|dish| dish.station_key == vessel.key);
                        @let vessel_item_id = vessel.instrument_item_id.as_deref().unwrap_or("container");
                        @let vessel_definition = definitions.iter().find(|item| item.id == vessel_item_id);
                        section class="fireplace-vessel" data-fireplace-container=(object_id) {
                            p class="fireplace-vessel-name" { (decorative_game_icon("meal")) strong { (item_name_with_display(vessel_item_id, &item_display_name(vessel_item_id), vessel_definition)) } }
                            button type="button" class="btn btn-secondary btn-small"
                                data-container-open=(object_id) aria-label=(format!("Open {}", item_display_name(vessel_item_id))) { "Open" }
                            @if let Some(cooking) = vessel_dish {
                                p class="fireplace-vessel-cooking" title=(format!("{} is cooking", cooking.display_name)) {
                                    (decorative_game_icon("flame")) span { (cooking.display_name.as_str()) }
                                }
                                form action=(format!("{action_base}/retrieve")) method="post" {
                                    input type="hidden" name="container_object_id" value=(object_id);
                                    button class="btn btn-primary btn-small" type="submit" { "Retrieve into vessel" }
                                }
                            } @else {
                                form action=(format!("{action_base}/container/start")) method="post" {
                                    input type="hidden" name="container_object_id" value=(object_id);
                                    button class="btn btn-primary btn-small" type="submit" { "Start cooking contents" }
                                }
                                form action=(format!("{action_base}/container/remove")) method="post" {
                                    input type="hidden" name="container_object_id" value=(object_id);
                                    button class="btn btn-secondary btn-small" type="submit" { "Remove from fire" }
                                }
                            }
                        }
                    }
                    }
                }))
            }
            @if let Some(row) = dish {
                section class="rest-service-menu fireplace-rest-menu" aria-label="Rest until food is ready" {
                    div class="rest-service-heading" { strong { "Rest while cooking" } }
                    p class="fireplace-rest-countdown" aria-label=(format!("{} minutes remaining", u64::from(row.target_minutes).saturating_sub(elapsed))) {
                        (decorative_game_icon("flame"))
                        span { (u64::from(row.target_minutes).saturating_sub(elapsed)) "m" }
                    }
                    @if elapsed >= u64::from(row.target_minutes) {
                        p class="strategic-warning fireplace-ready-warning" role="status" {
                            (decorative_game_icon("flame"))
                            @match row.method {
                                CookingMethod::Stew => { "The dish is ready and will hold safely in its wet pot." }
                                CookingMethod::Roast => { "The dish is ready. More time will dry and smoke it, reducing nutrition." }
                                CookingMethod::PanFry | CookingMethod::Bake => { "The dish is ready. More time will burn it." }
                            }
                        }
                    }
                    form action=(rest_action) method="post" {
                        input type="hidden" name="unit" value="minutes";
                        input type="hidden" name="shelter" value="bivouac";
                        label for="fireplace-rest-minutes" { "Minutes" }
                        input id="fireplace-rest-minutes" type="range" name="duration" min="1" max=(row.target_minutes.saturating_mul(2).max(1)) value=(remaining);
                        output for="fireplace-rest-minutes" { (remaining) }
                        button type="submit" class="btn btn-primary btn-block" { "Rest" }
                    }
                }
            }
        }
        main class="center-content settlement-main fireplace-stage" {
            a class="btn btn-secondary btn-small" href=(back_href) { "Back" }
            (visual_stage(VisualStageKind::Campfire, title, "A working fireplace and its cooking station"))
            @if dish.is_none() {
                section class="cooking-activity" {
                    input type="radio" name="method-preview" value=(method) checked hidden data-cooking-method;
                    form id="cooking-submit-form" method="post" action=(format!("{action_base}/ingredients")) {
                        input type="hidden" name="inventory_scope" value=(inventory_scope);
                        input type="hidden" name="inventory_item_ids" value="" data-cooking-ids;
                        input type="hidden" name="fractions_micros" value="" data-cooking-amounts;
                        p class="strategic-warning" { "Loose food selected here is immediately consolidated into one spit-roasted meal. Each placed vessel cooks its contained food into a separate meal." }
                        p class="small-copy text-muted cooking-preview" data-cooking-preview { "Stage at least one measured food portion." }
                        button type="submit" class="btn btn-primary" disabled data-cook-submit { "Start spit roast" }
                    }
                    div data-cooking-pot-empty hidden {}
                    div data-inventory-browser="cooking-pot-left" hidden { table { tbody {} } }
                }
            }
        }
        aside class="right-sidebar fireplace-inventory-sidebar" {
            (sidebar_section("Inventory", html! {
                nav class="tab-list" aria-label="Ingredient inventory source" {
                    a class=(if inventory_scope == "personal" { "active" } else { "" }) href=(scope_href("personal")) aria-current=(if inventory_scope == "personal" { "page" } else { "false" }) { "Personal" }
                    a class=(if inventory_scope == "party" { "active" } else { "" }) href=(scope_href("party")) aria-current=(if inventory_scope == "party" { "page" } else { "false" }) { "Party" }
                }
                @if dish.is_none() {
                    div data-inventory-browser="cooking-inventory-right" {
                        table class="trade-inventory-table" { tbody {
                            @if inventory_scope == "personal" {
                                @for item in personal_inventory.iter().filter(|row| row.quantity > 0) {
                                    (fireplace_inventory_row(action_base, inventory_scope, item.id, &item.item_id, item.quantity, personal_amounts.iter().find(|a| a.inventory_item_id == item.id).map(|a| a.remaining_fraction_micros), food_lots.iter().find(|l| l.inventory_item_id == Some(item.id)), definitions, instrument))
                                }
                            } @else {
                                @for item in party_inventory.iter().filter(|row| row.quantity > 0) {
                                    (fireplace_inventory_row(action_base, inventory_scope, item.id, &item.item_id, item.quantity, party_amounts.iter().find(|a| a.party_inventory_item_id == item.id).map(|a| a.remaining_fraction_micros), food_lots.iter().find(|l| l.party_inventory_item_id == Some(item.id)), definitions, instrument))
                                }
                            }
                        } }
                    }
                }
            }))
        }
        }
    };
    layout(content)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the fireplace row mirrors independent custody and installation form fields"
)]
pub(super) fn fireplace_inventory_row(
    action_base: &str,
    scope: &str,
    id: u64,
    item_id: &str,
    quantity: u32,
    measured_fraction_micros: Option<u32>,
    lot: Option<&FoodLot>,
    definitions: &[CatalogItemView],
    _installed: Option<&str>,
) -> Markup {
    let definition = definitions.iter().find(|d| d.id == item_id);
    let is_tool = matches!(item_id, "cooking_pan" | "cooking_pot" | "portable_oven");
    let display = lot.map_or_else(|| item_display_name(item_id), |l| l.display_name.clone());
    let measured_fraction = measured_fraction_micros.map(|value| {
        adventuresim_core::inventory_measurement::ConsumableFractionMicros::try_new(value)
            .expect("public consumable fraction must not exceed one whole")
    });
    let amount_micros = measured_fraction.map_or_else(
        || {
            quantity.saturating_mul(
                adventuresim_core::inventory_measurement::ConsumableFractionMicros::MICROS_PER_WHOLE,
            )
        },
        adventuresim_core::inventory_measurement::ConsumableFractionMicros::get,
    );
    let display_units =
        measured_fraction.map_or(quantity as f32, |fraction| fraction.as_unit_f32());
    html! { tr class="trade-inventory-row trade-row-player" data-cooking-source=[lot.map(|_| id)] data-personal-inventory-id=[(scope == "personal").then_some(id)] data-party-inventory-id=[(scope == "party").then_some(id)] {
        td class="inventory-item-type" { (item_type_icon(item_id)) }
        td class="inventory-item-name" { (item_name_with_display(item_id, &display, definition))
            span class="inventory-row-actions" {
                @if lot.is_some() && adventuresim_core::food::is_cookable_ingredient(item_id) {
                    button type="button" class="trade-transfer trade-transfer-left"
                        data-cooking-stage=(id) data-cooking-name=(&display) data-count=(amount_micros)
                        data-mass=(format!("{:.4}", lot.map_or(0.0, |l| l.mass_kg))) data-safety=(adventuresim_core::food::definition(item_id).map_or(5, |f| f.cooking_minutes))
                        data-culinary-fat=(adventuresim_core::food::definition(item_id).is_some_and(|f| f.culinary_fat))
                        data-salty=(lot.map_or(0.0, |l| l.salty_kg)) data-spicy=(lot.map_or(0.0, |l| l.spicy_kg))
                        data-sweet=(lot.map_or(0.0, |l| l.sweet_kg)) data-sour=(lot.map_or(0.0, |l| l.sour_kg)) data-savory=(lot.map_or(0.0, |l| l.savory_kg))
                        data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                        data-label-one=(format!("Add 0.25 {display}")) data-label-target=(format!("Add {display}")) data-label-all=(format!("Add all {display}"))
                        aria-label=(format!("Add 0.25 {display}")) title=(format!("Add 0.25 {display}")) { (transfer_glyph(1)) }
                } @else if is_tool {
                    form action=(format!("{action_base}/container/place")) method="post" {
                        input type="hidden" name="inventory_scope" value=(scope);
                        input type="hidden" name="inventory_item_id" value=(id);
                        button type="submit" class="btn btn-primary btn-small" { "Place over fire" }
                    }
                }
            }
        }
        td class="inventory-count" { (format!("{display_units:.2}")) }
        td class="inventory-weight" { (definition.map_or(0.0, |d| d.weight)) }
    } }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fireplace_container_rows_expose_shared_browser_metadata_and_open_controls() {
        let source = include_str!("cooking.rs");
        let fireplace = source
            .split("pub fn fireplace_page")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(fireplace.contains("data-inventory-browser=\"fireplace-vessels-left\""));
        assert!(fireplace.contains("data-container-open=(object_id)"));
        assert!(fireplace.contains("item_name_with_display(item_id, &display, definition)"));
    }
}
