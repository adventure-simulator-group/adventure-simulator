use adventuresim_core::{
    equipment::{EncumbranceSummary, INPUT_ADDRESS_MAPPINGS, InputAddressMapping},
    herbalism::{CraftOutcome, PreparationMethod},
    item_catalog_schema::EquipmentLocation as CoreEquipmentLocation,
    skill::Skill,
};
use maud::{Markup, html};

use super::{
    character_details::religion_name,
    character_health::stat_icon,
    character_skills::{SkillRankBarOptions, skill_rank_bar},
    chrome::{party_portrait_overlay, visual_stage},
    context::LocationView,
    rest::{RestSummary, SoapRestPreview, rest_default_minutes, rest_service_menu},
    social::{
        inventory_rail, merchant_offers_rail, npc_description_stage, npc_location_id,
        npc_portrait_strip, player_chat_area, settlement_chat_area, settlement_resident_chat_area,
    },
};
use crate::spacetimedb::{
    BackendFireplaceDish, BackendFireplaceStation, Character, CharacterAttributes,
    CharacterCondition, CharacterEquipmentGraph, CharacterLimbs, CharacterSkills, CharacterStats,
    EquipmentBodyPart, EquipmentChannel, EquipmentLocation, FoodLot, InventoryItem,
    InventoryItemAmount, InventoryQuantityTarget, ItemDefinition, PartyInventoryItem,
    PartyItemAmount, Settlement,
};
use crate::templates::inventory_browser::{InventoryBrowser, InventoryColumnSet};
use crate::templates::{
    decorative_game_icon, empty_state, game_icon, item_display_name, item_icon_name,
    item_source_edit_url, item_type_header, item_type_icon, settlement_layout_with_session,
    sidebar_section,
};

/// The currently available merchant storefronts. They share trade mechanics,
/// but each storefront limits the stock shown on its left-hand side.
#[derive(Clone, Copy)]
pub enum MerchantShop {
    General,
    Weapons,
    Armor,
    Clothing,
    Herbalist,
    Inn,
    Books,
}
impl MerchantShop {
    pub fn storefront(self) -> adventuresim_core::settlement_economy::Storefront {
        use adventuresim_core::settlement_economy::Storefront as S;
        match self {
            Self::General => S::General,
            Self::Weapons => S::Weapons,
            Self::Armor => S::Armor,
            Self::Clothing => S::Clothing,
            Self::Herbalist => S::Herbalist,
            Self::Inn => S::Inn,
            Self::Books => S::Books,
        }
    }

    pub fn available_at(self, settlement: &Settlement) -> bool {
        adventuresim_core::settlement_economy::storefront_available(
            &settlement.economy,
            self.storefront(),
        )
    }

    fn stocks_at(self, settlement: &Settlement, item: &crate::spacetimedb::ItemDefinition) -> bool {
        use adventuresim_core::settlement_economy::CatalogKind as C;
        let kind = match item.kind {
            crate::spacetimedb::ItemKind::Simple => C::Simple,
            crate::spacetimedb::ItemKind::Weapon => C::Weapon,
            crate::spacetimedb::ItemKind::Armor => C::Armor,
            crate::spacetimedb::ItemKind::Shield => C::Shield,
            crate::spacetimedb::ItemKind::Clothing => C::Clothing,
            crate::spacetimedb::ItemKind::Container => C::Simple,
            crate::spacetimedb::ItemKind::Currency => C::Currency,
            crate::spacetimedb::ItemKind::Ingredient => C::Ingredient,
            crate::spacetimedb::ItemKind::Medication => C::Medication,
            crate::spacetimedb::ItemKind::Food => C::Food,
        };
        let stocked = adventuresim_core::settlement_economy::storefront_stocks(
            &settlement.economy,
            self.storefront(),
            &item.id,
            kind,
        );
        stocked
            && (!matches!(self, Self::Books)
                || adventuresim_core::item_catalog::definition(&item.id).is_some_and(
                    |definition| {
                        definition.capabilities.book.as_ref().is_some_and(|book| {
                            book.settlement_allowlist.is_empty()
                                || book.settlement_allowlist.contains(&settlement.id)
                        })
                    },
                ))
    }
    pub fn service_id(self) -> &'static str {
        match self {
            Self::General => "merchants",
            Self::Weapons => "weapons",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
            Self::Herbalist => "herbalist",
            Self::Inn => "inn",
            Self::Books => "books",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "General Market",
            Self::Weapons => "Weaponsmith",
            Self::Armor => "Armourer",
            Self::Clothing => "Tailor",
            Self::Herbalist => "Herbalist",
            Self::Inn => "The Inn",
            Self::Books => "Bookstore",
        }
    }

    fn stocks(self, item: &crate::spacetimedb::ItemDefinition) -> bool {
        let kind = item.kind;
        match self {
            Self::General => !matches!(
                kind,
                crate::spacetimedb::ItemKind::Currency
                    | crate::spacetimedb::ItemKind::Ingredient
                    | crate::spacetimedb::ItemKind::Medication
            ),
            Self::Weapons => matches!(
                kind,
                crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield
            ),
            Self::Armor => kind == crate::spacetimedb::ItemKind::Armor,
            Self::Clothing => kind == crate::spacetimedb::ItemKind::Clothing,
            Self::Herbalist => matches!(
                kind,
                crate::spacetimedb::ItemKind::Ingredient | crate::spacetimedb::ItemKind::Medication
            ),
            Self::Inn => {
                adventuresim_core::food::definition(&item.id).is_some()
                    || matches!(
                        item.id.as_str(),
                        "cooking_pan" | "cooking_pot" | "portable_oven"
                    )
            }
            Self::Books => adventuresim_core::item_catalog::definition(&item.id)
                .is_some_and(|definition| definition.capabilities.book.is_some()),
        }
    }

    fn shows_inventory(self, item: &crate::spacetimedb::ItemDefinition) -> bool {
        item.kind == crate::spacetimedb::ItemKind::Currency || self.stocks(item)
    }
}

pub fn merchants_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    food_lots: &[FoodLot],
    party_members: &[Character],
    logged_in_as: Option<&str>,
) -> Markup {
    service_page(
        settlement,
        "merchants",
        "Market Square",
        "Market Steward",
        "The market steward has no listed stock at present.",
        active_character,
        inventory,
        &[],
        food_lots,
        party_members,
        logged_in_as,
        None,
        None,
        SoapRestPreview::default(),
    )
}

/// Church interface.
pub fn religion_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    limbs: Option<&CharacterLimbs>,
    stats: Option<&CharacterStats>,
    condition: Option<&CharacterCondition>,
    field_repair_minutes: u64,
    smith_wait_minutes: u64,
    soap_preview: SoapRestPreview,
    logged_in_as: Option<&str>,
) -> Markup {
    service_page(
        settlement,
        "religion",
        "Church",
        "Priest",
        "Faith, donations, and divine services require the religion and reputation systems.",
        active_character,
        inventory,
        items,
        food_lots,
        party_members,
        logged_in_as,
        rest_default_minutes(
            limbs,
            stats,
            condition,
            field_repair_minutes,
            smith_wait_minutes,
        ),
        None,
        soap_preview,
    )
}

/// Party inventory comparison.
pub fn party_inventory_page(
    location: &LocationView,
    selected: &Character,
    selected_inventory: &[InventoryItem],
    active_character: &Character,
    active_inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    selected_equip: Option<&CharacterEquipmentGraph>,
    active_equip: Option<&CharacterEquipmentGraph>,
    selected_targets: &[InventoryQuantityTarget],
    active_targets: &[InventoryQuantityTarget],
    selected_encumbrance: EncumbranceSummary,
    active_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_trade_inventory_rail(selected, selected_inventory, items, food_lots, active_character.id, "right", selected_equip, active_targets, selected_encumbrance, false))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id), false))
            (visual_stage("character", &selected.name, "Party member and trading companion"))
            (player_chat_area(location, selected, active_character))
            form id="party-offer" class="party-offer" action=(format!("{}/party/{}/inventory/offer", location.base_path(), selected.id)) method="post" hidden
                role="dialog" aria-modal="true" aria-label="Confirm party item offer" tabindex="-1" {
                span class="party-offer-summary" { "Review and send the staged item offer." }
                button type="button" class="party-offer-cancel" data-cancel-trade="party" { "Cancel" }
                button type="submit" disabled { "Offer" }
            }
        }
        aside class="right-sidebar" {
            (party_trade_inventory_rail(active_character, active_inventory, items, food_lots, selected.id, "left", active_equip, selected_targets, active_encumbrance, true))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name))
}

/// The active character's inventory with a staged discard list.
pub fn party_discard_page(
    location: &LocationView,
    active_character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    equip: Option<&CharacterEquipmentGraph>,
    encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Discard", html! {
                p class="text-muted small-copy" data-discard-empty { "Stage carried items here before discarding them." }
                div data-discard-table hidden {
                    (trade_inventory_table("discard-left", InventoryColumnSet::All, true, false, false, html! {}))
                }
            }))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id), false))
            (visual_stage("character", &active_character.name, "Your carried equipment and supplies"))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            form id="inventory-discard" class="party-offer"
                action=(format!("{}/party/{}/inventory/discard", location.base_path(), active_character.id))
                method="post" hidden role="dialog" aria-modal="true" aria-label="Confirm discarded items" tabindex="-1" {
                span class="party-offer-summary" data-discard-confirmation { "Discard the staged items?" }
                button type="button" class="party-offer-cancel" data-cancel-trade="discard" { "Cancel" }
                button type="submit" disabled { "Discard" }
            }
        }
        aside class="right-sidebar" {
            (discard_inventory_rail(active_character, inventory, items, food_lots, equip, encumbrance))
        }
    };
    location.render_layout("Inventory", content, Some(&active_character.name))
}

#[allow(clippy::too_many_arguments)]
pub fn fireplace_page(
    title: &str,
    back_href: &str,
    action_base: &str,
    rest_action: &str,
    _active_character: &Character,
    inventory_scope: &str,
    personal_inventory: &[InventoryItem],
    party_inventory: &[PartyInventoryItem],
    personal_amounts: &[InventoryItemAmount],
    party_amounts: &[PartyItemAmount],
    food_lots: &[FoodLot],
    definitions: &[ItemDefinition],
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
            "Overcooking"
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
                p { strong { "Instrument: " } (instrument.map(item_display_name).unwrap_or_else(|| "Roasting spit".into())) }
                @if let Some(row) = dish {
                    p data-fireplace-status { strong { (status.unwrap_or("Cooking")) } " · " (elapsed) "/" (row.target_minutes) " min" }
                    p class="small-copy text-muted" { "Added by " (&row.contributor_name) " at character minute " (row.started_at_minute) ". Hidden food-safety state is intentionally not shown." }
                    form action=(format!("{action_base}/retrieve")) method="post" {
                        label { "Retrieve to "
                            select name="inventory_scope" {
                                option value="personal" selected[inventory_scope == "personal"] { "Personal inventory" }
                                option value="party" selected[inventory_scope == "party"] { "Party inventory" }
                            }
                        }
                        button class="btn btn-primary btn-block" type="submit" { "Retrieve dish" }
                    }
                } @else {
                    p class="small-copy text-muted" { "One dish may occupy this fireplace. With no instrument installed, ingredients roast." }
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
                            p { strong { (item_name_with_display(vessel_item_id, &item_display_name(vessel_item_id), vessel_definition)) } }
                            button type="button" class="btn btn-secondary btn-small"
                                data-container-open=(object_id) aria-label=(format!("Open {}", item_display_name(vessel_item_id))) { "Open" }
                            @if let Some(cooking) = vessel_dish {
                                p class="small-copy" { (cooking.display_name.as_str()) " is cooking." }
                                form action=(format!("{action_base}/retrieve")) method="post" {
                                    input type="hidden" name="inventory_scope" value=(format!("container:{object_id}"));
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
                    p class="small-copy" { "Target: " (row.target_minutes) " minutes · Remaining: " (u64::from(row.target_minutes).saturating_sub(elapsed)) " minutes" }
                    @if elapsed >= u64::from(row.target_minutes) {
                        p class="strategic-warning" role="status" { "The dish is ready. More rest will overcook it." }
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
            (visual_stage("campfire", title, "A working fireplace and its cooking station"))
            @if dish.is_none() {
                section class="cooking-activity" {
                    input type="radio" name="method-preview" value=(method) checked hidden data-cooking-method;
                    form id="cooking-submit-form" method="post" action=(format!("{action_base}/ingredients")) {
                        input type="hidden" name="inventory_scope" value=(inventory_scope);
                        input type="hidden" name="inventory_item_ids" value="" data-cooking-ids;
                        input type="hidden" name="amounts_milliunits" value="" data-cooking-amounts;
                        p class="strategic-warning" { "Adding ingredients is irreversible: selected food is immediately diced and consolidated into one generic cooked meal." }
                        p class="small-copy text-muted cooking-preview" data-cooking-preview { "Stage at least one measured food portion." }
                        button type="submit" class="btn btn-primary" disabled data-cook-submit { "Add Ingredients" }
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
                                @for item in personal_inventory.iter().filter(|row| row.qty > 0) {
                                    (fireplace_inventory_row(action_base, inventory_scope, item.id, &item.item_id, item.qty, personal_amounts.iter().find(|a| a.inventory_item_id == item.id).map(|a| a.remaining_milliunits), food_lots.iter().find(|l| l.inventory_item_id == Some(item.id)), definitions, instrument))
                                }
                            } @else {
                                @for item in party_inventory.iter().filter(|row| row.quantity > 0) {
                                    (fireplace_inventory_row(action_base, inventory_scope, item.id, &item.item_id, item.quantity, party_amounts.iter().find(|a| a.party_inventory_item_id == item.id).map(|a| a.remaining_milliunits), food_lots.iter().find(|l| l.party_inventory_item_id == Some(item.id)), definitions, instrument))
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

fn fireplace_inventory_row(
    action_base: &str,
    scope: &str,
    id: u64,
    item_id: &str,
    quantity: u32,
    measured: Option<u32>,
    lot: Option<&FoodLot>,
    definitions: &[ItemDefinition],
    _installed: Option<&str>,
) -> Markup {
    let definition = definitions.iter().find(|d| d.id == item_id);
    let is_tool = matches!(item_id, "cooking_pan" | "cooking_pot" | "portable_oven");
    let display = lot.map_or_else(|| item_display_name(item_id), |l| l.display_name.clone());
    let amount = measured.unwrap_or(quantity.saturating_mul(1_000_000));
    html! { tr class="trade-inventory-row trade-row-player" data-cooking-source=[lot.map(|_| id)] data-personal-inventory-id=[(scope == "personal").then_some(id)] data-party-inventory-id=[(scope == "party").then_some(id)] {
        td class="inventory-item-type" { (item_type_icon(item_id)) }
        td class="inventory-item-name" { (item_name_with_display(item_id, &display, definition))
            span class="inventory-row-actions" {
                @if lot.is_some() && adventuresim_core::food::is_cookable_ingredient(item_id) {
                    button type="button" class="trade-transfer trade-transfer-left"
                        data-cooking-stage=(id) data-cooking-name=(&display) data-count=(amount)
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
        td class="inventory-count" { (format!("{:.2}", amount as f32 / 1_000_000.0)) }
        td class="inventory-weight" { (definition.map_or(0.0, |d| d.weight)) }
    } }
}

pub(super) fn herbalism_activity_dialog(
    location: &LocationView,
    active_character: &Character,
    skills: Option<&CharacterSkills>,
    attributes: Option<&CharacterAttributes>,
    inventory: &[InventoryItem],
    item_definitions: &[ItemDefinition],
) -> Markup {
    let close_href = location.preserve_building(format!(
        "{}/party/{}",
        location.base_path(),
        active_character.id
    ));
    let action = location.preserve_building(format!(
        "{}/party/{}/herbalism",
        location.base_path(),
        active_character.id
    ));
    let capability = Skill::Herbalism.capped_rank_for_aptitude(
        skills.map_or(0.0, |row| row.herbalism_hours),
        attributes.map_or(0.0, |row| row.intelligence),
    );
    let ingredients = inventory
        .iter()
        .filter(|row| adventuresim_core::herbalism::normalize_ingredient(&row.item_id).is_some())
        .collect::<Vec<_>>();
    let encoded_preview = |item_id: &str, method: PreparationMethod| {
        adventuresim_core::herbalism::preview(item_id, method, capability).map(|preview| {
            let (output, degraded) = match preview.outcome {
                CraftOutcome::Medication(id) => (item_display_name(id), false),
                CraftOutcome::DegradedWaste(id) => (item_display_name(id), true),
            };
            let requirement = preview.required_consumable.map_or_else(
                || "No additional consumable".to_string(),
                |required| {
                    format!(
                        "{} × {}",
                        item_display_name(required.item_id),
                        required.units
                    )
                },
            );
            format!(
                "{}|{}|{}|{}|{}|{}|{}",
                output,
                preview.duration_minutes,
                preview.input_units,
                requirement,
                preview.expected_effect,
                preview.risk,
                degraded
            )
        })
    };
    html! {
        div class="character-action-overlay" data-character-action-dialog data-initial-focus="[name=inventory_item_id]" {
            a class="character-action-backdrop" href=(&close_href) aria-label="Close herbalism dialog" {}
            section class="character-action-dialog herbalism-dialog" role="dialog" aria-modal="true"
                aria-labelledby="herbalism-dialog-title" tabindex="-1" data-herbalism-activity {
                header class="character-action-dialog-header" {
                    h2 id="herbalism-dialog-title" { "Herbalism" }
                    a class="character-action-dialog-close" href=(&close_href)
                        aria-label="Close herbalism dialog" { "×" }
                }
                form method="post" action=(&action) data-herbalism-form {
                    section aria-labelledby="herbal-ingredient-title" {
                        h3 id="herbal-ingredient-title" { "Medicinal ingredient" }
                        @if ingredients.is_empty() {
                            (empty_state("No medicinal ingredients carried.", None, None))
                        } @else {
                            table class="trade-inventory-table herbalism-exchange-list" {
                                tbody {
                                    @for row in &ingredients {
                                        @let definition = item_definitions.iter().find(|item| item.id == row.item_id);
                                        @let grade = adventuresim_core::herbalism::normalize_ingredient(&row.item_id)
                                            .map_or("Ordinary", |(_, grade)| grade.label());
                                        tr class="trade-inventory-row trade-row-player" {
                                            td class="inventory-item-type" { (item_type_icon(&row.item_id)) }
                                            td class="inventory-item-name" {
                                                label {
                                                    input type="radio" name="inventory_item_id" value=(row.id)
                                                        data-herbal-ingredient
                                                        data-item-id=(&row.item_id)
                                                        data-dry-grind=[encoded_preview(&row.item_id, PreparationMethod::DryGrind)]
                                                        data-infuse-decoct=[encoded_preview(&row.item_id, PreparationMethod::InfuseDecoct)]
                                                        data-tincture=[encoded_preview(&row.item_id, PreparationMethod::Tincture)];
                                                    (item_display_name(&row.item_id))
                                                    span class="inventory-item-quality" {
                                                        " · " (grade)
                                                    }
                                                }
                                            }
                                            td class="inventory-count" { (row.qty) }
                                            td class="inventory-weight" { (definition.map_or(0.0, |item| item.weight)) " kg" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    fieldset class="herbalism-methods" {
                        legend { "Preparation method" }
                        @for method in PreparationMethod::ALL {
                            @let description_id = format!("herbal-method-{}-description", method.slug());
                            label class="btn btn-secondary"
                                tabindex="0"
                                aria-describedby=(&description_id)
                                aria-disabled="true"
                                data-method-label=(method.label())
                                data-method-description=(method.description())
                                data-strategic-tooltip=(method.description()) {
                                input type="radio" name="method" value=(method.slug())
                                    data-herbal-method;
                                (method.label())
                                span id=(&description_id) class="sr-only"
                                    data-herbal-method-status {
                                    (method.description())
                                }
                            }
                        }
                    }
                    p class="small-copy herbalism-preview" data-herbal-preview role="status" aria-live="polite" {
                        "Select one ingredient and one method."
                    }
                    div class="party-offer herbalism-actions" {
                        a class="btn btn-secondary party-offer-cancel" href=(&close_href) { "Cancel" }
                        button type="submit" class="btn btn-primary" disabled data-herbal-submit {
                            "Prepare"
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn filth_status_bar(
    deposits: &[crate::spacetimedb::CharacterFilth],
    wetness_bps: u16,
) -> Markup {
    use crate::spacetimedb::{FilthOrigin, FilthSubstance};
    let dirt: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Dirt)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let blood: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let total = dirt
        .saturating_add(blood)
        .min(adventuresim_core::filth::MAX_FILTH);
    let dirt_width = f32::from(dirt.min(total));
    let blood_width = f32::from(blood.min(total.saturating_sub(dirt.min(total))));
    let (own_blood, foreign_blood, unknown_blood) = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .fold((0_u16, 0_u16, 0_u16), |mut amounts, deposit| {
            match deposit.origin {
                FilthOrigin::Own => amounts.0 = amounts.0.saturating_add(deposit.amount),
                FilthOrigin::Foreign => amounts.1 = amounts.1.saturating_add(deposit.amount),
                FilthOrigin::Unknown => amounts.2 = amounts.2.saturating_add(deposit.amount),
            }
            amounts
        });
    let summary = format!(
        "Current: {total}/100 — {dirt} dirt, {blood} blood ({own_blood} own, {foreign_blood} foreign, {unknown_blood} unknown)."
    );
    let details = format!(
        "Filth accumulates from travel, combat, and medical treatment. Dirt and blood fill the inner bar. Foreign blood can transmit bloodborne disease, with greater risk through open cuts and lesser risk through bandaged cuts. Soap is used automatically before rest to wash filth away.\n\nWetness is the blue outer bar behind filth. Rain and immersion add water; warmth and wind dry it. Wetness increases cold exposure. Current wetness: {}%.\n\n{summary}",
        wetness_bps / 100
    );
    html! {
        div class="coating-status" role="group" aria-label="Coatings" {
          strong class="metric-label filth-status-label" { "Filth" }
          div class="coating-track-stack" {
            div class="wetness-status" role="meter"
                aria-valuemin="0" aria-valuemax="100"
                aria-valuenow=(wetness_bps / 100)
                aria-label=(format!("Wetness {} out of 100", wetness_bps / 100)) {
              span class="wetness-track" aria-hidden="true" {
                  span style=(format!("width:{}%", wetness_bps / 100)) {}
              }
            }
            div class="filth-status" tabindex="0" role="meter" aria-valuemin="0" aria-valuemax="100"
                aria-valuenow=(total) aria-label=(format!("Filth {total} out of 100"))
                data-strategic-tooltip=(&details) {
              span class="filth-track" aria-hidden="true" {
                  @if dirt > 0 {
                      span class="filth-segment filth-dirt" style=(format!("width:{dirt_width}%"))
                          data-strategic-tooltip=(format!("Dirt\n{dirt}")) {}
                  }
                  @if blood > 0 {
                      span class="filth-segment filth-blood" style=(format!("width:{blood_width}%"))
                          data-strategic-tooltip=(format!("Blood\n{blood}")) {}
                  }
              }
            }
          }
        }
    }
}

pub(super) fn religious_demand_rail(
    demand: &crate::spacetimedb::ReligiousDemand,
    location_path: &str,
    character_id: u64,
) -> Markup {
    let action = format!(
        "{location_path}/party/{character_id}/religious-demand/{}",
        demand.id
    );
    html! {
        (sidebar_section("Conviction demands", html! {
            article class="religious-demand" {
                h3 { (&demand.title) }
                p { (&demand.description) }
                p class="text-muted small-copy" {
                    "Observe and bear the practical cost, or decline. Party Command automatically reduces the morale cost of neglect and can remove it entirely."
                }
                form method="post" action=(action) class="religious-demand-actions" {
                    button type="submit" name="choice" value="observe" class="btn btn-primary" { "Observe" }
                    button type="submit" name="choice" value="refuse" class="btn btn-danger" { "Do not observe" }
                }
            }
        }))
    }
}

/// Selected party member stats and biography.
pub(super) fn service_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    npc_name: &str,
    service_summary: &str,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
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
                    (rest_service_menu("Inn", &settlement.id, "inn", rest_default_minutes, rest_summary, soap_preview))
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
                    (rest_service_menu("Temple", &settlement.id, "temple", rest_default_minutes, rest_summary, soap_preview))
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
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None, false))
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

fn party_trade_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    recipient_id: u64,
    direction: &str,
    equip: Option<&CharacterEquipmentGraph>,
    recipient_targets: &[InventoryQuantityTarget],
    encumbrance: EncumbranceSummary,
    medication_is_self: bool,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            (encumbrance_inventory_rail(html! {
                @if inventory.is_empty() {
                    p class="text-muted small-copy" { "No items carried." }
                } @else {
                    (trade_inventory_table(if direction == "left" { "party-transfer-right" } else { "party-transfer-left" }, InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let is_equipped = equip.is_some_and(|equip| equip.contains(item.id));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                            @let display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let target = target_quantity(recipient_targets, &item.item_id);
                            @let item_name = item_display_name(&item.item_id);
                                tr class=(if direction == "left" { "trade-inventory-row trade-row-player" } else { "trade-inventory-row trade-row-merchant" }) data-item-key=(&item.item_id) data-personal-inventory-id=(item.id) {
                                    td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                    td class="inventory-item-name" {
                                        (item_name_with_food_lot(&item.item_id, &display_name, definition, food_lot))
                                        span class="inventory-row-actions" {
                                            @if is_equipped {
                                                (disabled_transfer_button(direction, "Equipped items cannot be transferred"))
                                            } @else {
                                                button type="button" class=(format!("trade-transfer trade-transfer-{direction} party-draft-transfer")) data-dynamic-transfer data-default-transfer-mode="one" data-from=(character.id) data-to=(recipient_id) data-item=(item.id) data-key=(&item.item_id) data-count=(item.qty) data-target=(target) data-transfer-mode="one" data-label-one=(format!("Transfer one {item_name}")) data-label-target=(format!("Transfer {item_name} to target")) data-label-all=(format!("Transfer all {item_name}")) aria-label=(format!("Transfer one {item_name}")) title=(format!("Transfer one {item_name}")) { (transfer_glyph(1)) }
                                            }
                                        }
                                    }
                                    td class="inventory-count" { (item.qty) }
                                    td class="inventory-equipped" { (equipment_control(item, definition, is_equipped, medication_is_self, equip)) }
                                    td class="inventory-weight" { (item_weight(definition)) }
                                    td class="inventory-gold" { (item_value(definition)) }
                                }
                            }
                    }))
                }
            }, inventory_footer_controls(if direction == "left" { "party-left" } else { "party-right" }, "Transfer to targets", "Transfer everything"), encumbrance))
        }))
    }
}

fn discard_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    equip: Option<&CharacterEquipmentGraph>,
    encumbrance: EncumbranceSummary,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            (encumbrance_inventory_rail(html! {
                @if inventory.is_empty() {
                    p class="text-muted small-copy" { "No items carried." }
                } @else {
                    (trade_inventory_table("discard-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let is_equipped = equip.is_some_and(|equip| equip.contains(item.id));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                            @let display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row trade-row-player" data-discard-source=(item.id) data-personal-inventory-id=(item.id) data-item-key=(&item.item_id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_food_lot(&item.item_id, &display_name, definition, food_lot))
                                    span class="inventory-row-actions" {
                                        @if is_equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be discarded"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left"
                                            data-discard-item=(item.id) data-count=(item.qty)
                                            data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                                            data-label-one=(format!("Discard one {item_name}"))
                                            data-label-target=(format!("Discard {item_name} down to target"))
                                            data-label-all=(format!("Discard all {item_name}"))
                                            aria-label=(format!("Discard {item_name}"))
                                            title=(format!("Discard one {item_name}")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (item.qty) }
                                td class="inventory-equipped" { (equipment_control(item, definition, is_equipped, true, equip)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }
            }, html! {}, encumbrance))
        }))
    }
}

pub fn live_merchant_shop_page(
    settlement: &Settlement,
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    equip: Option<&CharacterEquipmentGraph>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    pooled: &[PartyInventoryItem],
    shop: MerchantShop,
    shared_language: f32,
    problem_buy_bps: i32,
    problem_sell_penalty_bps: i32,
    conditions: &[crate::spacetimedb::ItemCondition],
    smith: Option<&crate::spacetimedb::SettlementSmith>,
    repair_orders: &[crate::spacetimedb::RepairOrder],
    now_minutes: u64,
    personal_encumbrance: EncumbranceSummary,
    party_encumbrance: EncumbranceSummary,
    rest_default_minutes: Option<u64>,
    soap_preview: SoapRestPreview,
) -> Markup {
    let title = shop.title();
    let service_id = shop.service_id();
    // Herbalist purchases use a separate reducer and retain their specialized quote.
    let trade_language = if matches!(shop, MerchantShop::Herbalist) {
        1.0
    } else {
        shared_language
    };
    let smith_skill = smith
        .map(|smith| {
            if matches!(shop, MerchantShop::Armor) {
                smith.armourer_skill
            } else if matches!(shop, MerchantShop::Clothing) {
                smith.tailor_skill
            } else {
                smith.weaponsmith_skill
            }
        })
        .unwrap_or(0);
    let player_footer = if matches!(shop, MerchantShop::Herbalist) {
        html! {}
    } else {
        inventory_footer_controls_with_leading(
            matches!(
                shop,
                MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing
            )
            .then(|| repair_all_control(settlement, service_id)),
            "sell",
            "Sell surplus",
            "Sell everything",
        )
    };
    let stocked_items = items
        .iter()
        .filter(|item| shop.stocks_at(settlement, item))
        .collect::<Vec<_>>();
    let content = html! {
        aside class=(if matches!(shop, MerchantShop::Inn) { "left-sidebar smith-wares-column service-left-sidebar" } else { "left-sidebar smith-wares-column" }) {
        div class=(if matches!(shop, MerchantShop::Inn) { "service-left-stack" } else { "merchant-stock-stack" }) {
        div class=(if matches!(shop, MerchantShop::Inn) { "service-inventory-area" } else { "merchant-stock-area" }) {
        (sidebar_section(if matches!(shop, MerchantShop::Herbalist) { "Existing preparations and ingredients" } else if matches!(shop, MerchantShop::Inn) { "Cooking supplies" } else { "Merchant stock" }, html! {
            div class="smith-wares-scroll" {
            @if stocked_items.is_empty() {
                (empty_state("No stock is available here.", None, None))
            } @else {
            (trade_inventory_table("merchant-left", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, false, false, false, html! {
                @for item in stocked_items.iter().copied() {
                    @let is_currency = item.kind == crate::spacetimedb::ItemKind::Currency;
                    @let intervention = adventuresim_core::physiology::intervention_profile(&item.id, 1);
                    @let buy_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_buy_price(
                        adventuresim_core::strategic_economy::merchant_buy_price(item.base_value.unwrap_or(1)),
                        trade_language
                    ), problem_buy_bps);
                    @let sell_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_sell_price((item.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32, trade_language), -problem_sell_penalty_bps);
                    @let target = target_quantity(personal_targets, &item.id);
                    @let display_name = item_display_name(&item.id);
                    tr class="trade-inventory-row trade-row-merchant" data-merchant-item=(&item.id) data-merchant-sell-price=(sell_price) data-group-summary="catalog" data-intervention-profile-version=[intervention.map(|profile| profile.version)] { td class="inventory-item-type" { (item_type_icon(&item.id)) } td class="inventory-item-name" { (item_name_with_display(&item.id, &display_name, Some(item))) @if !is_currency { (merchant_buy_controls(&item.id, buy_price, target, 999)) } } td class="inventory-count" hidden { "999" } td class="inventory-weight" { (weight_display(item.weight)) } td class="inventory-gold" { (buy_price) } }
                }
            }))
            (inventory_footer_controls("buy", "Buy to targets", "Buy everything"))
            @if matches!(shop, MerchantShop::Herbalist) {
                p class="small-copy text-muted" { "Pre-existing preparations are sold into personal inventory for versioned administration. Physiology does not craft them; #214 owns preparation." }
            }
            }
            }
        }))
        }
        @if matches!(shop, MerchantShop::Inn) {
            section class="inn-rest-panel" aria-label="Inn lodging and rest" {
                (rest_service_menu("Inn", &settlement.id, "inn", rest_default_minutes, None, soap_preview))
            }
        }
        }
        @if matches!(shop, MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing) {
            (repair_custody_panel(settlement, shop, repair_orders, conditions, items, now_minutes, smith_skill))
        }
        }
        main class="center-content settlement-main" { (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), None, false)) (npc_portrait_strip(&settlement.id, npc_location_id(service_id))) (npc_description_stage(title, "Merchant counter and attending craftsperson")) (settlement_resident_chat_area(title, Some(character), &settlement.id, npc_location_id(service_id), Some(service_id))) form # "merchant-offer" class="party-offer" action=(if matches!(shop, MerchantShop::Herbalist) { format!("/settlements/{}/herbalist/purchase", settlement.id) } else { format!("/settlements/{}/storefront/{service_id}/offer", settlement.id) }) method="post" data-hard-navigation hidden role="dialog" aria-modal="true" aria-label="Confirm merchant offer" tabindex="-1" { span class="party-offer-summary" { "Review and submit the staged trade." } input type="hidden" name="return_to" value=(format!("/settlements/{}/{}", settlement.id, service_id)); input type="hidden" name="inventory_scope" value="player"; button type="button" class="party-offer-cancel" data-cancel-trade="merchant" { "Cancel" } button type="submit" disabled { "Offer" } } }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Trading inventory" {
                button type="button" class="inventory-owner-tab active" data-inventory-tab="player" { "Player" }
                @if !matches!(shop, MerchantShop::Herbalist) {
                    button type="button" class="inventory-owner-tab" data-inventory-tab="party" { "Party" }
                }
            }
            div data-inventory-pane="player" {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-player-right", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, true, matches!(shop, MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing), html! {
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                        @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let is_equipped = equip.is_some_and(|equip| equip.contains(item.id));
                        @let sell_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_sell_price(merchant_inventory_sell_price(definition, food_lot), trade_language), -problem_sell_penalty_bps);
                        @let target = target_quantity(personal_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-personal-inventory-id=(item.id) data-merchant-equipped=(is_equipped) data-inventory-quantity=(item.qty) data-target=(target) {
                        @let condition = conditions.iter().find(|condition| condition.inventory_item_id == item.id);
                        @let repair_skill = smith_skill;
                        @let durable_item = definition.is_some_and(|definition| definition.repairable);
                        @let service_matches = definition.is_some_and(|definition| if matches!(shop, MerchantShop::Armor) { definition.kind == crate::spacetimedb::ItemKind::Armor } else if matches!(shop, MerchantShop::Clothing) { definition.kind == crate::spacetimedb::ItemKind::Clothing } else { matches!(definition.kind, crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield) });
                        @let can_sell = !is_currency && !is_equipped;
                        td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                        td class="inventory-item-name" { (item_name_with_food_lot(&item.item_id, &food_display_name, definition, food_lot)) @if !matches!(shop, MerchantShop::Herbalist) && (can_sell || service_matches) { (merchant_sell_repair_controls(item.id, &item.item_id, sell_price, item.qty, target, can_sell, service_matches.then(|| repair_submit_control(settlement, service_id, item.id, condition, repair_skill)))) } }
                        td class="inventory-count" { (quantity_target_control(item.qty, target, &item.item_id, false)) } td class="inventory-equipped" { (equipment_control(item, definition, is_equipped, true, equip)) } td class="inventory-durability" { @if durable_item { (condition_bar(condition, service_matches.then_some(repair_skill))) } @else { "—" } } td class="inventory-weight" { (merchant_inventory_weight(definition, food_lot)) } td class="inventory-gold" { (sell_price) }
                    }}
                    @for target in personal_targets.iter().filter(|target| target.quantity > 0 && !inventory.iter().any(|item| item.item_id == target.item_id) && items.iter().find(|definition| definition.id == target.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, false)) }
                            td class="inventory-equipped" {
                                span class="equipment-unavailable" role="img" tabindex="0"
                                    aria-label="No equipment in this row"
                                    data-strategic-tooltip="No equipment is available in this row" {}
                            }
                            td class="inventory-durability" { "—" }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                }, player_footer, personal_encumbrance))
            }
            }
            @if !matches!(shop, MerchantShop::Herbalist) { div data-inventory-pane="party" hidden {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-party-right", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, false, false, html! {
                    @for item in pooled.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.party_inventory_item_id == Some(item.id));
                        @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let sell_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_sell_price(merchant_inventory_sell_price(definition, food_lot), trade_language), -problem_sell_penalty_bps);
                        @let target = target_quantity(party_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-party-inventory-id=(item.id) data-inventory-quantity=(item.quantity) data-target=(target) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_food_lot(&item.item_id, &food_display_name, definition, food_lot)) @if !is_currency { (merchant_sell_controls(item.id, &item.item_id, sell_price, item.quantity, target)) } }
                            td class="inventory-count" { (quantity_target_control(item.quantity, target, &item.item_id, true)) }
                            td class="inventory-weight" { (merchant_inventory_weight(definition, food_lot)) }
                            td class="inventory-gold" { (sell_price) }
                        }
                    }
                    // Party purchases may spend pooled coin first and the active
                    // character's coin second. Show both funding sources as the
                    // same collapsed Coin row in this scope.
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency)) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        tr class="trade-inventory-row trade-row-player party-personal-currency" data-merchant-item=(&item.item_id) data-inventory-quantity=(item.qty) data-target="0" title="Personal coin available for party purchases" {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                    @for target in party_targets.iter().filter(|target| target.quantity > 0 && !pooled.iter().any(|item| item.item_id == target.item_id) && items.iter().find(|definition| definition.id == target.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                }, inventory_footer_controls("sell", "Sell surplus", "Sell everything"), party_encumbrance))
            }
            }
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
        Some(&character.name),
    )
}

/// Two-sided transfer view for the equally owned party chest.
pub fn party_pool_page(
    location: &LocationView,
    character: &Character,
    inventory: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    equip: Option<&CharacterEquipmentGraph>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    party_encumbrance: EncumbranceSummary,
    personal_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Party inventory", html! {
                (encumbrance_inventory_rail(html! {
                    div class="party-stake-summary" tabindex="0"
                        data-strategic-tooltip="Withdrawals use your stake; personal coin covers an indivisible item's shortfall."
                        aria-label=(format!("Your available stake: {stake} coin. Withdrawals use your stake; personal coin covers an indivisible item's shortfall.")) {
                        span { "Your available stake" }
                        strong { (stake) " coin" }
                    }
                    (trade_inventory_table("party-pool-left", InventoryColumnSet::All, true, false, false, html! {
                        @for item in pooled {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.party_inventory_item_id == Some(item.id));
                            @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let value = definition.and_then(|definition| definition.base_value).unwrap_or(0) as u64;
                            @let target = target_quantity(personal_targets, &item.item_id);
                            @let current = inventory.iter().find(|personal| personal.item_id == item.item_id).map_or(0, |personal| personal.qty);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row" data-party-inventory-id=(item.id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_food_lot(&item.item_id, &food_display_name, definition, food_lot))
                                span class="inventory-row-actions" { button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="withdraw" data-transfer-mode="one" data-count=(item.quantity) data-current=(current) data-target=(target) data-label-one=(format!("Withdraw one {item_name}")) data-label-target=(format!("Withdraw {item_name} to target")) data-label-all=(format!("Withdraw all {item_name}")) title=(if value > stake { format!("Withdraw one {item_name}; {} personal coin required", value - stake) } else { format!("Withdraw one {item_name} using your stake") }) aria-label=(format!("Withdraw one {item_name}")) { (transfer_glyph(1)) } }
                                }
                                td class="inventory-count" { (quantity_target_control(item.quantity, target_quantity(party_targets, &item.item_id), &item.item_id, true)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }, inventory_footer_controls("withdraw", "Withdraw to personal targets", "Withdraw everything"), party_encumbrance))
            }))
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, Some(character), &location.base_path(), None, false))
            (visual_stage("chest", "Party chest", "Shared supplies and each member's stake"))
            (settlement_chat_area("Party inventory", Some(character)))
        }
        aside class="right-sidebar" {
            (sidebar_section(&format!("{}'s inventory", character.name), html! {
                (encumbrance_inventory_rail(html! {
                    (trade_inventory_table("party-pool-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                            @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let equipped = equip.is_some_and(|equip| equip.contains(item.id));
                            @let target = target_quantity(party_targets, &item.item_id);
                            @let current = pooled.iter().find(|pooled| pooled.item_id == item.item_id).map_or(0, |pooled| pooled.quantity);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row" data-personal-inventory-id=(item.id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_food_lot(&item.item_id, &food_display_name, definition, food_lot))
                                    span class="inventory-row-actions" {
                                        @if equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be deposited"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="deposit" data-transfer-mode="one" data-count=(item.qty) data-current=(current) data-target=(target) data-label-one=(format!("Deposit one {item_name}")) data-label-target=(format!("Deposit {item_name} to target")) data-label-all=(format!("Deposit all {item_name}")) aria-label=(format!("Deposit one {item_name} at its objective coin value")) data-strategic-tooltip=(format!("Deposit one {item_name} at its objective coin value")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (quantity_target_control(item.qty, target_quantity(personal_targets, &item.item_id), &item.item_id, false)) }
                                td class="inventory-equipped" { (equipment_control(item, definition, equipped, true, equip)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }, inventory_footer_controls("deposit", "Deposit to party targets", "Deposit everything"), personal_encumbrance))
            }))
        }
        form method="post" action=(format!("{}/party-inventory/deposit", location.base_path())) id="pool-transfer-offer" class="party-offer" hidden role="dialog" aria-modal="true" aria-label="Confirm party inventory transfer" tabindex="-1" { span class="party-offer-summary" { "Apply the staged party inventory transfer?" } button type="button" data-cancel-pool class="party-offer-cancel" { "Cancel" } button type="submit" disabled { "Offer" } }
    };
    location.render_layout("Party inventory", content, Some(&character.name))
}

fn item_weight(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.map_or_else(|| "—".to_owned(), |item| weight_display(item.weight))
}

fn merchant_inventory_weight(
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    food_lot: Option<&FoodLot>,
) -> String {
    food_lot.map_or_else(
        || item_weight(definition),
        |lot| weight_display(lot.mass_kg),
    )
}

fn merchant_inventory_sell_price(
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    food_lot: Option<&FoodLot>,
) -> u32 {
    food_lot.map_or_else(
        || {
            definition.map_or(0, |definition| {
                adventuresim_core::strategic_economy::merchant_sell_price(
                    definition.base_value.unwrap_or(1),
                )
            })
        },
        |lot| {
            adventuresim_core::strategic_economy::merchant_sell_food_lot_value(lot.total_value)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0)
        },
    )
}

fn encumbrance_inventory_rail(
    content: Markup,
    footer_controls: Markup,
    summary: EncumbranceSummary,
) -> Markup {
    html! {
        div class="encumbrance-inventory-rail" {
            div class="encumbrance-inventory-scroll" { (content) }
            (footer_controls)
            (encumbrance_meter(summary))
        }
    }
}

fn encumbrance_meter(summary: EncumbranceSummary) -> Markup {
    let penalty_percent = summary.penalty_fraction() * 100.0;
    let accessible_text = format!(
        "Weight {:.1} / {:.1} kilograms; Penalty -{penalty_percent:.1}%",
        summary.burden_kg, summary.capacity_kg
    );
    html! {
        div class="encumbrance" {
            div class="encumbrance-visual" {
                div class="encumbrance-meter"
                    tabindex="0"
                    data-strategic-tooltip=(&accessible_text)
                    role="meter"
                    aria-label="Encumbrance"
                    aria-valuemin="0"
                    aria-valuemax="100"
                    aria-valuenow=(format!("{penalty_percent:.1}"))
                    aria-valuetext=(accessible_text) {
                    span class="encumbrance-marker"
                        style=(format!("--encumbrance-position: {penalty_percent:.4}%")) {}
                }
            }
        }
    }
}

fn equipment_target_is_self_or_descendant(
    equip: &CharacterEquipmentGraph,
    moving_inventory_item_id: u64,
    candidate_parent_id: u64,
) -> bool {
    let mut ancestors = vec![candidate_parent_id];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(item_id) = ancestors.pop() {
        if item_id == moving_inventory_item_id {
            return true;
        }
        if visited.insert(item_id) {
            ancestors.extend(
                equip
                    .equipment_occupancies
                    .iter()
                    .filter(|row| row.inventory_item_id == item_id)
                    .filter_map(|row| row.parent_inventory_item_id),
            );
        }
    }
    false
}

fn web_equipment_location(location: CoreEquipmentLocation) -> EquipmentLocation {
    match location {
        CoreEquipmentLocation::Head => EquipmentLocation::Head,
        CoreEquipmentLocation::Face => EquipmentLocation::Face,
        CoreEquipmentLocation::Neck => EquipmentLocation::Neck,
        CoreEquipmentLocation::Chest => EquipmentLocation::Chest,
        CoreEquipmentLocation::Stomach => EquipmentLocation::Stomach,
        CoreEquipmentLocation::Back => EquipmentLocation::Back,
        CoreEquipmentLocation::LeftShoulder => EquipmentLocation::LeftShoulder,
        CoreEquipmentLocation::RightShoulder => EquipmentLocation::RightShoulder,
        CoreEquipmentLocation::LeftArm => EquipmentLocation::LeftArm,
        CoreEquipmentLocation::RightArm => EquipmentLocation::RightArm,
        CoreEquipmentLocation::LeftHand => EquipmentLocation::LeftHand,
        CoreEquipmentLocation::RightHand => EquipmentLocation::RightHand,
        CoreEquipmentLocation::LeftLeg => EquipmentLocation::LeftLeg,
        CoreEquipmentLocation::RightLeg => EquipmentLocation::RightLeg,
        CoreEquipmentLocation::LeftFoot => EquipmentLocation::LeftFoot,
        CoreEquipmentLocation::RightFoot => EquipmentLocation::RightFoot,
        CoreEquipmentLocation::LeftBelt => EquipmentLocation::LeftBelt,
        CoreEquipmentLocation::RightBelt => EquipmentLocation::RightBelt,
        CoreEquipmentLocation::FrontBelt => EquipmentLocation::FrontBelt,
        CoreEquipmentLocation::BackBelt => EquipmentLocation::BackBelt,
        CoreEquipmentLocation::LeftPocket => EquipmentLocation::LeftPocket,
        CoreEquipmentLocation::RightPocket => EquipmentLocation::RightPocket,
        CoreEquipmentLocation::BackLeftPocket => EquipmentLocation::BackLeftPocket,
        CoreEquipmentLocation::BackRightPocket => EquipmentLocation::BackRightPocket,
    }
}

fn equipment_input_display(input: &str) -> String {
    if input == "tab" {
        "Tab".to_owned()
    } else {
        input.to_ascii_uppercase()
    }
}

fn equipment_location_display(location: CoreEquipmentLocation) -> &'static str {
    match location {
        CoreEquipmentLocation::Head => "Head",
        CoreEquipmentLocation::Face => "Face",
        CoreEquipmentLocation::Neck => "Neck",
        CoreEquipmentLocation::Chest => "Chest",
        CoreEquipmentLocation::Stomach => "Stomach",
        CoreEquipmentLocation::Back => "Back",
        CoreEquipmentLocation::LeftShoulder => "Left shoulder",
        CoreEquipmentLocation::RightShoulder => "Right shoulder",
        CoreEquipmentLocation::LeftArm => "Left arm",
        CoreEquipmentLocation::RightArm => "Right arm",
        CoreEquipmentLocation::LeftHand => "Left hand",
        CoreEquipmentLocation::RightHand => "Right hand",
        CoreEquipmentLocation::LeftLeg => "Left leg",
        CoreEquipmentLocation::RightLeg => "Right leg",
        CoreEquipmentLocation::LeftFoot => "Left foot",
        CoreEquipmentLocation::RightFoot => "Right foot",
        CoreEquipmentLocation::LeftBelt => "Left belt",
        CoreEquipmentLocation::RightBelt => "Right belt",
        CoreEquipmentLocation::FrontBelt => "Front belt",
        CoreEquipmentLocation::BackBelt => "Back belt",
        CoreEquipmentLocation::LeftPocket => "Left pocket",
        CoreEquipmentLocation::RightPocket => "Right pocket",
        CoreEquipmentLocation::BackLeftPocket => "Back-left pocket",
        CoreEquipmentLocation::BackRightPocket => "Back-right pocket",
    }
}

fn equipped_location_display(location: EquipmentLocation) -> &'static str {
    match location {
        EquipmentLocation::Head => "Head",
        EquipmentLocation::Face => "Face",
        EquipmentLocation::Neck => "Neck",
        EquipmentLocation::Chest => "Chest",
        EquipmentLocation::Stomach => "Stomach",
        EquipmentLocation::Back => "Back",
        EquipmentLocation::LeftShoulder => "Left shoulder",
        EquipmentLocation::RightShoulder => "Right shoulder",
        EquipmentLocation::LeftArm => "Left arm",
        EquipmentLocation::RightArm => "Right arm",
        EquipmentLocation::LeftHand => "Left hand",
        EquipmentLocation::RightHand => "Right hand",
        EquipmentLocation::LeftLeg => "Left leg",
        EquipmentLocation::RightLeg => "Right leg",
        EquipmentLocation::LeftFoot => "Left foot",
        EquipmentLocation::RightFoot => "Right foot",
        EquipmentLocation::LeftBelt => "Left belt",
        EquipmentLocation::RightBelt => "Right belt",
        EquipmentLocation::FrontBelt => "Front belt",
        EquipmentLocation::BackBelt => "Back belt",
        EquipmentLocation::LeftPocket => "Left pocket",
        EquipmentLocation::RightPocket => "Right pocket",
        EquipmentLocation::BackLeftPocket => "Back-left pocket",
        EquipmentLocation::BackRightPocket => "Back-right pocket",
    }
}

fn equipment_body_part_display(part: EquipmentBodyPart) -> &'static str {
    match part {
        EquipmentBodyPart::LeftArm => "Left Arm",
        EquipmentBodyPart::RightArm => "Right Arm",
        EquipmentBodyPart::LeftLeg => "Left Leg",
        EquipmentBodyPart::RightLeg => "Right Leg",
        EquipmentBodyPart::Chest => "Chest",
        EquipmentBodyPart::Stomach => "Stomach",
        EquipmentBodyPart::Head => "Head",
    }
}

fn equipment_channel_rank(channel: EquipmentChannel) -> u64 {
    match channel {
        EquipmentChannel::Held => 0,
        EquipmentChannel::BaseClothing => 10,
        EquipmentChannel::Padding => 20,
        EquipmentChannel::FlexibleArmor => 30,
        EquipmentChannel::RigidArmor => 40,
        EquipmentChannel::Outerwear => 50,
        EquipmentChannel::Accessory => 60,
        EquipmentChannel::Mount => 70,
        EquipmentChannel::Containment => 80,
    }
}

fn equipment_binding_contains(binding: &InputAddressMapping, location: EquipmentLocation) -> bool {
    binding
        .locations
        .iter()
        .copied()
        .map(web_equipment_location)
        .any(|candidate| candidate == location)
}

fn equipment_item_roots(
    equip: &CharacterEquipmentGraph,
    inventory_item_id: u64,
) -> Vec<(EquipmentLocation, EquipmentChannel, u16, usize)> {
    fn visit(
        equip: &CharacterEquipmentGraph,
        inventory_item_id: u64,
        attachment_depth: usize,
        path: &mut std::collections::BTreeSet<u64>,
        roots: &mut Vec<(EquipmentLocation, EquipmentChannel, u16, usize)>,
    ) {
        if !path.insert(inventory_item_id) {
            return;
        }
        for row in equip
            .equipment_occupancies
            .iter()
            .filter(|row| row.inventory_item_id == inventory_item_id)
        {
            if let Some(location) = row.location {
                roots.push((location, row.channel, row.order, attachment_depth));
            } else if let Some(parent_id) = row.parent_inventory_item_id {
                visit(equip, parent_id, attachment_depth + 1, path, roots);
            }
        }
        path.remove(&inventory_item_id);
    }

    let mut roots = Vec::new();
    visit(
        equip,
        inventory_item_id,
        0,
        &mut std::collections::BTreeSet::new(),
        &mut roots,
    );
    roots
}

fn equipment_binding_rank(
    binding: &InputAddressMapping,
    roots: &[(EquipmentLocation, EquipmentChannel, u16, usize)],
) -> Option<u64> {
    roots
        .iter()
        .filter(|(location, _, _, _)| equipment_binding_contains(binding, *location))
        .map(|(_, channel, order, attachment_depth)| {
            (*attachment_depth as u64 * 100_000)
                + (equipment_channel_rank(*channel) * 1_000)
                + u64::from(*order)
        })
        .max()
}

fn equipment_input_ranks_for_item(
    equip: &CharacterEquipmentGraph,
    inventory_item_id: u64,
) -> serde_json::Value {
    let roots = equipment_item_roots(equip, inventory_item_id);
    serde_json::Value::Object(
        INPUT_ADDRESS_MAPPINGS
            .iter()
            .filter_map(|binding| {
                equipment_binding_rank(binding, &roots)
                    .map(|rank| (binding.input.to_owned(), serde_json::json!(rank)))
            })
            .collect(),
    )
}

fn equipment_input_map_json() -> String {
    let inputs = INPUT_ADDRESS_MAPPINGS
        .iter()
        .map(|binding| {
            serde_json::json!({
                "input": binding.input,
                "label": equipment_input_display(binding.input),
                "row": binding.keyboard_row,
                "column": binding.keyboard_column,
                "locations": binding.locations
                    .iter()
                    .copied()
                    .map(equipment_location_display)
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&inputs).unwrap_or_else(|_| "[]".to_owned())
}

fn equipment_occupant_json(
    equip: &CharacterEquipmentGraph,
    inventory_item_id: u64,
) -> serde_json::Value {
    let item_id = equip
        .equipment_nodes
        .iter()
        .find(|node| node.inventory_item_id == inventory_item_id)
        .map(|node| node.item_name.as_str())
        .unwrap_or("unknown_item");
    serde_json::json!({
        "inventoryItemId": inventory_item_id,
        "itemName": item_display_name(item_id),
        "icon": format!("/static/icons/game/{}.svg", item_icon_name(item_id))
    })
}

fn equipped_input_badges(
    equip: Option<&CharacterEquipmentGraph>,
    inventory_item_id: u64,
) -> Vec<(String, usize, String)> {
    let Some(equip) = equip else {
        return Vec::new();
    };
    INPUT_ADDRESS_MAPPINGS
        .iter()
        .filter_map(|binding| {
            let roots = equipment_item_roots(equip, inventory_item_id);
            equipment_binding_rank(binding, &roots)?;
            let mut stack = equip
                .equipment_nodes
                .iter()
                .filter_map(|node| {
                    let roots = equipment_item_roots(equip, node.inventory_item_id);
                    equipment_binding_rank(binding, &roots)
                        .map(|rank| (std::cmp::Reverse(rank), node.inventory_item_id))
                })
                .collect::<Vec<_>>();
            stack.sort_unstable();
            let depth = stack
                .iter()
                .position(|(_, item_id)| *item_id == inventory_item_id)
                .unwrap_or_default();
            let locations = binding
                .locations
                .iter()
                .copied()
                .map(equipment_location_display)
                .collect::<Vec<_>>()
                .join(" / ");
            Some((equipment_input_display(binding.input), depth, locations))
        })
        .collect()
}

fn equipment_control(
    inventory: &InventoryItem,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    equipped: bool,
    medication_is_self: bool,
    equip: Option<&CharacterEquipmentGraph>,
) -> Markup {
    let medication = definition
        .is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Medication);
    let equippable = definition.is_some_and(|definition| {
        !definition.equipment_placements.is_empty() || (medication && medication_is_self)
    });
    let placement_labels = definition
        .map(|definition| {
            definition
                .equipment_placements
                .iter()
                .map(|placement| {
                    let anchors = placement
                        .occupancy
                        .iter()
                        .map(|requirement| {
                            format!(
                                "{:?} · {} · depth {}",
                                requirement.location,
                                requirement.channel.label(),
                                requirement.order
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let parent = placement
                        .parents
                        .iter()
                        .map(|parent| {
                            format!(
                                "attached via {} · depth {}",
                                parent.channel.label(),
                                parent.order
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let protection = placement
                        .protection
                        .iter()
                        .copied()
                        .map(equipment_body_part_display)
                        .collect::<Vec<_>>()
                        .join(", ");
                    let conflicts = equip
                        .into_iter()
                        .flat_map(|equip| equip.equipment_occupancies.iter())
                        .filter(|occupied| occupied.inventory_item_id != inventory.id)
                        .filter(|occupied| {
                            placement.occupancy.iter().any(|requirement| {
                                occupied.location == Some(requirement.location)
                                    && occupied.channel == requirement.channel
                                    && occupied.order == requirement.order
                            })
                        })
                        .map(|occupied| format!("#{}", occupied.inventory_item_id))
                        .collect::<Vec<_>>();
                    let anchor_or_parent = if parent.is_empty() { anchors } else { parent };
                    format!(
                        "{}: {}{}{}",
                        placement.id,
                        anchor_or_parent,
                        (!protection.is_empty())
                            .then(|| {
                                format!(
                                    "; protects {protection} ({:.0}% coverage)",
                                    definition.coverage * 100.0
                                )
                            })
                            .unwrap_or_default(),
                        (!conflicts.is_empty())
                            .then(|| format!("; conflict with {}", conflicts.join(", ")))
                            .unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .unwrap_or_default();
    let wear_layer = definition
        .and_then(|definition| {
            definition
                .equipment_placements
                .iter()
                .flat_map(|placement| placement.occupancy.iter())
                .map(|requirement| requirement.channel)
                .max()
        })
        .map(|channel| channel.label())
        .unwrap_or("Attachment");
    let placement_options = definition
        .map(|definition| {
            definition
                .equipment_placements
                .iter()
                .enumerate()
                .map(|(placement_index, placement)| {
                    let requirements = placement
                        .parents
                        .iter()
                        .enumerate()
                        .map(|(requirement_index, requirement)| {
                            let targets = equip
                                .into_iter()
                                .flat_map(|equip| {
                                    equip.attachment_targets.iter().filter(move |target| {
                                        target.channel == requirement.channel
                                            && target.order == requirement.order
                                            && (target.accepts_tags.is_empty()
                                                || definition
                                                    .attachment_tags
                                                    .iter()
                                                    .any(|tag| target.accepts_tags.contains(tag)))
                                            && !equipment_target_is_self_or_descendant(
                                                equip,
                                                inventory.id,
                                                target.parent_inventory_item_id,
                                            )
                                    })
                                })
                                .map(|target| {
                                    let mut occupants = equip
                                        .into_iter()
                                        .flat_map(|equip| equip.equipment_occupancies.iter())
                                        .filter(|row| {
                                            row.parent_inventory_item_id
                                                == Some(target.parent_inventory_item_id)
                                                && row.attachment_point_id.as_deref()
                                                    == Some(target.attachment_point_id.as_str())
                                        })
                                        .collect::<Vec<_>>();
                                    occupants.sort_by_key(|row| row.capacity_index);
                                    serde_json::json!({
                                        "parentInventoryItemId": target.parent_inventory_item_id,
                                        "attachmentPointId": target.attachment_point_id,
                                        "freeCapacity": target.free_capacity,
                                        "occupants": occupants
                                            .into_iter()
                                            .filter_map(|row| equip.map(|equip| {
                                                equipment_occupant_json(equip, row.inventory_item_id)
                                            }))
                                            .collect::<Vec<_>>(),
                                        "inputRanks": equip
                                            .map(|equip| equipment_input_ranks_for_item(
                                                equip,
                                                target.parent_inventory_item_id,
                                            ))
                                            .unwrap_or_else(|| serde_json::json!({})),
                                        "label": format!(
                                            "{} / {} ({} free)",
                                            item_display_name(&target.parent_item_name),
                                            target.attachment_point_id,
                                            target.free_capacity
                                        )
                                    })
                                })
                                .collect::<Vec<_>>();
                            serde_json::json!({
                                "requirementIndex": requirement_index,
                                "channel": requirement.channel.label(),
                                "targets": targets
                            })
                        })
                        .collect::<Vec<_>>();
                    let input_ranks = serde_json::Value::Object(
                        INPUT_ADDRESS_MAPPINGS
                            .iter()
                            .filter_map(|binding| {
                                placement
                                    .occupancy
                                    .iter()
                                    .filter(|requirement| {
                                        equipment_binding_contains(binding, requirement.location)
                                    })
                                    .map(|requirement| {
                                        (equipment_channel_rank(requirement.channel) * 1_000)
                                            + u64::from(requirement.order)
                                    })
                                    .max()
                                    .map(|rank| (binding.input.to_owned(), serde_json::json!(rank)))
                            })
                            .collect(),
                    );
                    let input_occupants = serde_json::Value::Object(
                        INPUT_ADDRESS_MAPPINGS
                            .iter()
                            .filter_map(|binding| {
                                let occupant = placement
                                    .occupancy
                                    .iter()
                                    .filter(|requirement| {
                                        equipment_binding_contains(binding, requirement.location)
                                    })
                                    .filter_map(|requirement| {
                                        equip
                                            .into_iter()
                                            .flat_map(|equip| {
                                                equip.equipment_occupancies.iter().map(move |row| {
                                                    (equip, row)
                                                })
                                            })
                                            .find(|(_, occupied)| {
                                                occupied.location == Some(requirement.location)
                                                    && occupied.channel == requirement.channel
                                                    && occupied.order == requirement.order
                                            })
                                    })
                                    .max_by_key(|(_, occupied)| {
                                        (
                                            equipment_channel_rank(occupied.channel),
                                            occupied.order,
                                            occupied.inventory_item_id,
                                        )
                                    });
                                occupant.map(|(equip, occupied)| {
                                    (
                                        binding.input.to_owned(),
                                        equipment_occupant_json(
                                            equip,
                                            occupied.inventory_item_id,
                                        ),
                                    )
                                })
                            })
                            .collect(),
                    );
                    serde_json::json!({
                        "placementIndex": placement_index,
                        "label": placement_labels
                            .split('|')
                            .nth(placement_index)
                            .unwrap_or(&placement.id),
                        "hasBody": !placement.occupancy.is_empty(),
                        "inputRanks": input_ranks,
                        "inputOccupants": input_occupants,
                        "requirements": requirements
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let placement_options_json =
        serde_json::to_string(&placement_options).unwrap_or_else(|_| "[]".to_owned());
    let input_map_json = equipment_input_map_json();
    let input_badges = equipped_input_badges(equip, inventory.id);
    let equipped_node = equip.and_then(|equip| {
        equip
            .equipment_nodes
            .iter()
            .find(|node| node.inventory_item_id == inventory.id)
    });
    let attachment_summary = equip.and_then(|equip| {
        let parents = equip
            .equipment_occupancies
            .iter()
            .filter(|row| row.inventory_item_id == inventory.id)
            .filter_map(|row| {
                let parent_id = row.parent_inventory_item_id?;
                let parent_name = equip
                    .equipment_nodes
                    .iter()
                    .find(|parent| parent.inventory_item_id == parent_id)
                    .map(|parent| item_display_name(&parent.item_name))
                    .unwrap_or_else(|| format!("#{parent_id}"));
                Some(format!(
                    "{parent_name} / {}",
                    row.attachment_point_id.as_deref()?
                ))
            })
            .collect::<Vec<_>>();
        (!parents.is_empty()).then(|| format!("Attached: {}", parents.join(" + ")))
    });
    let mut equipped_context = Vec::new();
    if let Some(equip) = equip.filter(|_| equipped) {
        let mut locations = Vec::new();
        for (location, _, _, _) in equipment_item_roots(equip, inventory.id) {
            let location = equipped_location_display(location);
            if !locations.contains(&location) {
                locations.push(location);
            }
        }
        if !locations.is_empty() {
            equipped_context.push(format!("Slots: {}", locations.join(", ")));
        }
    }
    if let Some(protection) = definition
        .zip(equipped_node)
        .and_then(|(definition, node)| {
            definition
                .equipment_placements
                .iter()
                .find(|placement| placement.id == node.placement_id)
                .filter(|placement| !placement.protection.is_empty())
                .map(|placement| {
                    format!(
                        "Protects {} ({:.0}% coverage)",
                        placement
                            .protection
                            .iter()
                            .copied()
                            .map(equipment_body_part_display)
                            .collect::<Vec<_>>()
                            .join(", "),
                        definition.coverage * 100.0
                    )
                })
        })
    {
        equipped_context.push(protection);
    }
    if let Some(attachment) = &attachment_summary {
        equipped_context.push(attachment.clone());
    }
    if !input_badges.is_empty() {
        equipped_context.push(format!(
            "Keyboard: {}",
            input_badges
                .iter()
                .map(|(input, depth, locations)| {
                    format!("{input} = {locations}, layer {}", depth + 1)
                })
                .collect::<Vec<_>>()
                .join("; ")
        ));
    } else if equipped {
        equipped_context.push("Equipped; no keyboard binding".into());
    }
    let item_name = item_display_name(&inventory.item_id);
    let base_label = if medication && medication_is_self {
        format!("Administer {item_name}")
    } else if medication {
        format!("Only {item_name}'s owner can administer it")
    } else if equipped {
        format!("Unequip {item_name}")
    } else {
        format!("Equip {item_name}")
    };
    let base_title = if medication && medication_is_self {
        "Administer one standard course of this preparation"
    } else if medication {
        "Select this character to administer their preparation"
    } else if equipped {
        "Click to unassign this item from all equipped slots"
    } else if equippable {
        "Click to choose an available keyboard slot"
    } else {
        "This item cannot be equipped"
    };
    let label = if equipped_context.is_empty() {
        base_label
    } else {
        format!("{base_label}. {}", equipped_context.join(". "))
    };
    let title = if equipped_context.is_empty() {
        base_title.to_owned()
    } else {
        format!("{base_title}. {}", equipped_context.join(". "))
    };
    html! {
        @if medication {
            input type="checkbox"
                checked[equipped]
                disabled[!equippable]
                data-equipment-toggle
                data-equipment-medication
                data-inventory-item-id=(inventory.id)
                aria-describedby=(format!("equipment-status-{}", inventory.id))
                aria-label=(label)
                title=(title);
        } @else if equippable {
            button type="button"
                class="equipment-slot-control"
                data-equipment-toggle
                data-equipment-equipped=(equipped)
                data-inventory-item-id=(inventory.id)
                data-wear-layer=(wear_layer)
                data-wear-placements=(placement_labels)
                data-equipment-input-map=(input_map_json)
                data-equipment-placement-options=(placement_options_json)
                aria-haspopup="dialog"
                aria-describedby=(format!("equipment-status-{}", inventory.id))
                aria-label=(label)
                data-strategic-tooltip=(title) {
                @if equipped {
                    @if input_badges.is_empty() {
                        span class="equipment-slot-unbound"
                            aria-hidden="true" { (decorative_game_icon("check-mark")) }
                    } @else {
                        @for (input, depth, locations) in &input_badges {
                            @let lightness = 88usize.saturating_sub((*depth).min(5) * 9);
                            kbd class="equipment-slot-key"
                                data-equipment-layer-depth=(depth)
                                data-strategic-tooltip=(format!("{input}: {locations}, layer {}", depth + 1))
                                style=(format!("--equipment-layer-lightness: {lightness}%")) {
                                (input)
                            }
                        }
                    }
                } @else {
                    span class="equipment-slot-empty" aria-hidden="true" { "+" }
                }
            }
        } @else {
            span class="equipment-unavailable" role="img" tabindex="0"
                aria-label="Not equippable"
                data-strategic-tooltip="This item cannot be equipped" {}
        }
        span id=(format!("equipment-status-{}", inventory.id))
            class="equipment-toggle-status"
            data-equipment-status
            role="status"
            aria-live="polite"
            hidden {}
        @if let Some(attachment_summary) = attachment_summary {
            span class="equipment-graph-summary" { (attachment_summary) }
        }
    }
}

fn item_value(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.and_then(|item| item.base_value)
        .map_or_else(|| "—".to_owned(), |value| value.to_string())
}

pub(in crate::templates) fn item_name_with_quality(
    item_id: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
) -> Markup {
    let currency_name = adventuresim_core::strategic_currency::currency_name(item_id);
    let edit_url = item_source_edit_url(item_id);
    if let Some(currency_name) = currency_name {
        html! {
            span class="inventory-item-label" data-item-name="Coin"
                data-item-kind="currency" data-currency-name=(currency_name)
                data-item-edit-url=[edit_url] { "Coin" }
        }
    } else {
        let display_name = item_display_name(item_id);
        item_name_with_display(item_id, &display_name, definition)
    }
}

fn item_name_with_display(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
) -> Markup {
    item_name_with_display_quality(item_id, display_name, definition, None)
}

pub(in crate::templates) fn item_name_with_food_lot(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    food_lot: Option<&FoodLot>,
) -> Markup {
    item_name_with_display_quality(
        item_id,
        display_name,
        definition,
        food_lot.map(|lot| lot.quality.clamp(1, 5)),
    )
}

fn item_name_with_display_quality(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    quality_override: Option<u8>,
) -> Markup {
    let edit_url = item_source_edit_url(item_id);
    let alcohol_group = definition
        .filter(|item| item.alcohol_serving_ml > 0)
        .map(|_| "alcohol");
    let food_quality =
        quality_override.is_some() || adventuresim_core::food::definition(item_id).is_some();
    let book_quality = adventuresim_core::item_catalog::definition(item_id)
        .is_some_and(|item| item.capabilities.book.is_some());
    let quality = quality_override.or_else(|| {
        definition
            .filter(|item| {
                matches!(
                    item.kind,
                    crate::spacetimedb::ItemKind::Weapon
                        | crate::spacetimedb::ItemKind::Armor
                        | crate::spacetimedb::ItemKind::Shield
                        | crate::spacetimedb::ItemKind::Food
                ) || adventuresim_core::food::definition(item_id).is_some()
                    || book_quality
            })
            .map(|item| item.quality.clamp(1, 5))
    });
    let label = quality.map(|quality| {
        if food_quality || book_quality {
            format!("Quality {quality}")
        } else {
            match quality {
                1 => "Quality 1".to_string(),
                2 => "Quality 2".to_string(),
                3 => "Quality 3 — munition grade".to_string(),
                4 => "Quality 4 — knightly commission".to_string(),
                5 => "Quality 5 — royal or heroic commission".to_string(),
                _ => unreachable!(),
            }
        }
    });
    let damage_types = definition.map(|item| {
        [
            item.blunt.then_some("Blunt"),
            item.slash.then_some("Slash"),
            item.pierce.then_some("Pierce"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    });
    html! {
        span class=(quality.map_or_else(|| "inventory-item-label".to_string(), |quality| format!("inventory-item-label item-quality-{quality}"))) title=[label]
            data-item-name=(item_id)
            data-item-kind=[definition.map(|item| format!("{:?}", item.kind).to_ascii_lowercase())]
            data-item-group=[alcohol_group]
            data-group-name=[alcohol_group.map(|_| "Alcohol")]
            data-food-lot=[adventuresim_core::food::definition(item_id).map(|_| "true")]
            data-container-capacity-ml=[definition.and_then(|item| (item.container_capacity_ml > 0).then_some(item.container_capacity_ml))]
            data-exterior-volume-ml=[definition.map(|item| item.exterior_volume_ml)]
            data-stat-accuracy=[definition.map(|item| weight_display(item.accuracy))]
            data-stat-reach=[definition.map(|item| weight_display(item.reach))]
            data-stat-penetration=[definition.map(|item| weight_display(item.penetration))]
            data-stat-damage=[damage_types]
            data-stat-block=[definition.map(|item| weight_display(item.block))]
            data-stat-coverage=[definition.map(|item| weight_display(item.coverage))]
            data-stat-resistance=[definition.map(|item| weight_display(item.resistance))]
            data-stat-padding=[definition.map(|item| weight_display(item.padding))]
            data-stat-flexibility=[definition.map(|item| weight_display(item.flexibility))]
            data-stat-range-of-motion=[definition.map(|item| weight_display(item.range_of_motion))]
            data-detail-slot=[definition.map(|item| format!("{:?}", item.slot))]
            data-detail-balance=[definition.map(|item| weight_display(item.balance))]
            data-item-edit-url=[edit_url]
            data-detail-mode=[definition.map(|item| match (item.melee, item.ranged, item.precise) { (true, true, true) => "Melee, ranged, precise", (true, true, false) => "Melee and ranged", (true, false, true) => "Melee, precise", (false, true, true) => "Ranged, precise", (true, false, false) => "Melee", (false, true, false) => "Ranged", (false, false, true) => "Precise", _ => "—" }.to_string())] {
            (display_name)
        }
    }
}

fn weight_display(weight: f32) -> String {
    let display = format!("{weight:.2}");
    display
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn trade_inventory_table(
    namespace: &str,
    optional_columns: InventoryColumnSet,
    show_quantities: bool,
    show_equipped: bool,
    show_condition: bool,
    rows: Markup,
) -> Markup {
    InventoryBrowser {
        namespace,
        show_quantities,
        show_equipped,
        show_condition,
        optional_columns,
        rows,
    }
    .render()
}

fn target_quantity(targets: &[InventoryQuantityTarget], item_id: &str) -> u32 {
    targets
        .iter()
        .find(|target| target.item_id == item_id)
        .map_or(0, |target| target.quantity)
}

fn quantity_target_control(quantity: u32, target: u32, item_id: &str, party_scope: bool) -> Markup {
    let item_name = item_display_name(item_id);
    html! {
        span class="inventory-target-control" data-target-control data-quantity=(quantity) data-item-id=(item_id) data-party-scope=(party_scope) title=(format!("Carrying {quantity}; target {target}")) {
            span class="inventory-target-value" data-target-value role="button" tabindex="0"
                aria-label=(format!("Target quantity for {item_name}"))
                title=(format!("Click to edit the target quantity for {item_name}")) { (target) }
        }
    }
}

pub(in crate::templates) fn transfer_glyph(count: usize) -> Markup {
    html! { span class=(format!("inventory-transfer-glyph arrows-{count}")) aria-hidden="true" { @for _ in 0..count { i {} } } }
}

fn disabled_transfer_button(direction: &str, explanation: &str) -> Markup {
    html! {
        button type="button" class=(format!("trade-transfer trade-transfer-{direction}")) disabled title=(explanation) aria-label=(explanation) { (transfer_glyph(1)) }
    }
}

fn merchant_buy_controls(item_id: &str, price: u32, target: u32, available: u32) -> Markup {
    let item_name = item_display_name(item_id);
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="one" data-target=(target) data-count=(available) data-label-one=(format!("Buy one {item_name}")) data-label-target=(format!("Buy {item_name} to target")) data-label-all=(format!("Buy all {item_name}")) aria-label=(format!("Buy one {item_name}")) title=(format!("Buy one {item_name}")) { (transfer_glyph(1)) }
    } }
}

fn merchant_sell_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
) -> Markup {
    let item_name = item_display_name(item_id);
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_name}")) data-label-target=(format!("Sell surplus {item_name}")) data-label-all=(format!("Sell all {item_name}")) aria-label=(format!("Sell one {item_name}")) title=(format!("Sell one {item_name}")) { (transfer_glyph(1)) }
    } }
}

fn merchant_sell_repair_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
    can_sell: bool,
    repair: Option<Markup>,
) -> Markup {
    let has_repair = repair.is_some();
    let item_name = item_display_name(item_id);
    html! { div class=(if has_repair { "inventory-row-actions smith-player-actions" } else { "inventory-row-actions" }) {
        @if let Some(repair) = repair { (repair) }
        @if can_sell {
            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_name}")) data-label-target=(format!("Sell surplus {item_name}")) data-label-all=(format!("Sell all {item_name}")) aria-label=(format!("Sell one {item_name}")) title=(format!("Sell one {item_name}")) { (transfer_glyph(1)) }
        } @else if has_repair {
            (disabled_transfer_button("left", "Equipped items cannot be sold"))
        }
    } }
}

fn condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    repair_skill: Option<u8>,
) -> Markup {
    let bins = condition.map(|value| value.bins()).unwrap_or([0.0; 5]);
    let total = bins.iter().sum::<f32>().clamp(0.0, 1.0);
    let green = (1.0 - total).max(0.0);
    let label = if total <= f32::EPSILON {
        "Full durability".to_string()
    } else if repair_skill
        .is_some_and(|skill| bins.iter().take(skill.min(5) as usize).sum::<f32>() > f32::EPSILON)
    {
        "Damaged; the flashing portion can be repaired by this smith".to_string()
    } else {
        "Damaged beyond this smith's skill".to_string()
    };
    html! {
        span class="condition-bar" data-sort-value=(weight_display(green)) title=(&label) aria-label=(&label) {
            span class="condition-green" style=(format!("width:{}%", green * 100.0)) {}
            @for (index, amount) in bins.iter().enumerate() {
                @let repairable = repair_skill.is_some_and(|skill| index < skill.min(5) as usize);
                span class=(format!("condition-tier-{}{}", index + 1, if repairable { " condition-repairable" } else { "" })) style=(format!("width:{}%", amount.clamp(0.0, 1.0) * 100.0)) {}
            }
        }
    }
}

fn completed_repair_condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    smith_skill: u8,
) -> Markup {
    let Some(condition) = condition else {
        return condition_bar(None, None);
    };
    let mut repaired = condition.clone();
    let mut bins = [
        &mut repaired.tier_1,
        &mut repaired.tier_2,
        &mut repaired.tier_3,
        &mut repaired.tier_4,
        &mut repaired.tier_5,
    ];
    for amount in bins.iter_mut().take(smith_skill.min(5) as usize) {
        **amount = 0.0;
    }
    condition_bar(Some(&repaired), None)
}

fn repair_all_control(settlement: &Settlement, service_id: &str) -> Markup {
    html! {
        form class="repair-all-form inventory-footer-repair" action=(format!("/settlements/{}/{}/repair-all", settlement.id, service_id)) method="post" {
            button type="submit" class="repair-all-button" title="Entrust all eligible items for repair" aria-label="Repair all eligible items" {
                span class="repair-action-icon" aria-hidden="true" {}
            }
        }
    }
}

fn repair_submit_control(
    settlement: &Settlement,
    service_id: &str,
    inventory_item_id: u64,
    condition: Option<&crate::spacetimedb::ItemCondition>,
    skill: u8,
) -> Markup {
    let total = condition.map_or(0.0, |value| value.total());
    let repairable = condition.map_or(0.0, |value| value.repairable(skill));
    let residual = condition.map_or(0.0, |value| value.residual(skill));
    let disabled = total <= f32::EPSILON || repairable <= f32::EPSILON;
    let explanation = if total <= f32::EPSILON {
        "Item is already in full condition".to_string()
    } else if repairable <= f32::EPSILON {
        format!("All damage requires Smithing above this smith's level {skill}")
    } else if residual > f32::EPSILON {
        "Repair all damage within this smith's skill; harder damage will remain".to_string()
    } else {
        format!("Repair all damage (smith level {skill})")
    };
    html! {
        form class="row-repair-form" action=(format!("/settlements/{}/{}/repair", settlement.id, service_id)) method="post" {
            input type="hidden" name="inventory_item_id" value=(inventory_item_id);
            @if disabled {
                span class="disabled-repair-explanation" tabindex="0" title=(&explanation) aria-label=(&explanation) {
                    button type="submit" class="repair-item-button" disabled { span class="repair-action-icon" aria-hidden="true" {} }
                }
            } @else {
                button type="submit" class="repair-item-button" title=(&explanation) aria-label=(&explanation) { span class="repair-action-icon" aria-hidden="true" {} }
            }
        }
    }
}

fn repair_custody_panel(
    settlement: &Settlement,
    shop: MerchantShop,
    orders: &[crate::spacetimedb::RepairOrder],
    conditions: &[crate::spacetimedb::ItemCondition],
    items: &[crate::spacetimedb::ItemDefinition],
    now: u64,
    smith_skill: u8,
) -> Markup {
    let service_id = shop.service_id();
    let mut matching: Vec<_> = orders
        .iter()
        .filter(|order| {
            order.settlement_id == settlement.id
                && items
                    .iter()
                    .find(|item| item.id == order.item_id)
                    .is_some_and(|item| shop.stocks(item))
        })
        .collect();
    matching.sort_by_key(|order| (order.submitted_at_minutes, order.id));
    html! {
        section class="repair-custody-panel" aria-label="Items entrusted for repair" {
            header class="repair-custody-header" {
                h3 { @if matches!(shop, MerchantShop::Clothing) { "In the tailor's care" } @else { "In the smith's care" } }
                @let craft = if matches!(shop, MerchantShop::Clothing) { "Tailoring" } else { "Smithing" };
                span class="repair-custody-skill" title=(format!("{craft} {smith_skill}")) {
                    (stat_icon(craft, "skills", if craft == "Tailoring" { "sewing-needle" } else { "smithing" }, false))
                    (skill_rank_bar(f32::from(smith_skill), f32::from(smith_skill), &format!("{craft} {smith_skill}"), SkillRankBarOptions::default()))
                }
            }
            div class="repair-custody-scroll" {
                @if matching.is_empty() { p class="text-muted small-copy" { "No items entrusted." } }
                div class="repair-custody-list" {
                    table class="trade-inventory-table repair-custody-table" {
                        colgroup {
                            col class="inventory-column-type";
                            col class="inventory-column-item";
                            col class="inventory-column-durability";
                            col class="repair-column-eta";
                            col class="inventory-column-gold";
                            col class="inventory-column-actions";
                        }
                        thead { tr {
                            (item_type_header())
                            th scope="col" class="inventory-column-item" { "Item" }
                            th scope="col" class="inventory-column-durability" { "Durability" }
                            th scope="col" class="repair-column-eta" { "ETA" }
                            th scope="col" class="inventory-column-gold" title="Full repair cost (Currency)" { (currency_header("Full repair cost in Currency")) }
                            th class="inventory-actions-header" aria-label="Repair retrieval actions" {
                                div class="inventory-footer-actions repair-custody-header-actions" {
                                    form class="repair-retrieve-all-form" data-repair-retrieve-form data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) method="post" {
                                        input type="hidden" name="limit" value="2";
                                        button type="submit" class="trade-transfer trade-transfer-right inventory-footer-transfer repair-retrieve-all" data-dynamic-transfer data-default-transfer-mode="target" data-transfer-mode="target" data-label-target="Retrieve up to two completed repairs" data-label-all="Retrieve all completed repairs" title="Retrieve up to two completed repairs" aria-label="Retrieve up to two completed repairs" { (transfer_glyph(2)) }
                                    }
                                }
                            }
                        } }
                        tbody {
                        @for order in matching {
                            @let condition = conditions.iter().find(|condition| condition.inventory_item_id == order.inventory_item_id);
                            @let definition = items.iter().find(|item| item.id == order.item_id);
                            @let ready = now >= order.ready_at_minutes;
                            @let remaining = order.ready_at_minutes.saturating_sub(now);
                            tr class="trade-inventory-row trade-row-merchant repair-order-row" {
                                td class="inventory-item-type" { (item_type_icon(&order.item_id)) }
                                td class="inventory-item-name" { (item_name_with_quality(&order.item_id, definition)) }
                                td class="inventory-durability" {
                                    @if ready { (completed_repair_condition_bar(condition, order.smith_skill)) }
                                    @else { (condition_bar(condition, Some(order.smith_skill))) }
                                }
                                td class="repair-column-eta" { @if ready { "Ready" } @else { (format!("{}h {}m", remaining / 60, remaining % 60)) } }
                                td class="inventory-gold" title="Quoted full-job cost, paid on retrieval" { (order.quoted_cost) }
                                td class="inventory-actions-cell" aria-label="Item actions" {
                                    span class="inventory-row-actions repair-retrieve-actions" {
                                        form data-repair-retrieve-form data-single-action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) method="post" {
                                            input type="hidden" name="item_id" value=(&order.item_id);
                                            input type="hidden" name="limit" value="1" disabled;
                                            button type="submit" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one" data-label-one="Retrieve this completed item" data-label-target="Retrieve up to two completed matching items" data-label-all="Retrieve all completed matching items" disabled[!ready] title=(if ready { "Retrieve this completed item" } else { "Repair is still underway" }) aria-label="Retrieve this completed item" { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                            }
                        }
                        }
                    }
                }
            }
        }
    }
}

pub(in crate::templates) fn inventory_footer_controls(
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    inventory_footer_controls_with_leading(None, action, target_label, all_label)
}

fn inventory_footer_controls_with_leading(
    leading: Option<Markup>,
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    let grouped = leading.is_some();
    html! { div class=(if grouped { "inventory-footer-actions inventory-footer-actions-grouped" } else { "inventory-footer-actions" }) {
        @if let Some(leading) = leading { (leading) }
        button type="button" class="trade-transfer inventory-footer-transfer" data-dynamic-transfer data-default-transfer-mode="target" data-inventory-bulk=(action) data-transfer-mode="target" data-label-target=(target_label) data-label-all=(all_label) aria-label=(target_label) title=(target_label) { (transfer_glyph(2)) }
    } }
}

fn currency_header(label: &str) -> Markup {
    game_icon(label, "coins")
}

// Kept for one-sided placeholder/service tables that are intentionally not
// inventory browsers.
pub(super) fn trade_inventory_table_header(
    show_equipped: bool,
    condition_header: Option<Markup>,
) -> Markup {
    html! { thead { tr {
        (item_type_header())
        th scope="col" class="inventory-column-item" { "Item" }
        th scope="col" class="inventory-column-count" title="Quantity" { (game_icon("Quantity", "open-chest")) }
        @if show_equipped { th scope="col" class="inventory-column-equipped" title="Equipped" { (game_icon("Equipped", "check-mark")) } }
        @if let Some(condition_header) = condition_header { th scope="col" class="inventory-column-durability" { (condition_header) } }
        th scope="col" class="inventory-column-weight" title="Weight" { (game_icon("Weight", "weight")) }
        th scope="col" class="inventory-column-gold" title="Currency" { (currency_header("Currency")) }
    } } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use crate::spacetimedb::{EquipmentAnchorKind, EquipmentLocation};
    use crate::templates::settlement::test_support::*;
    use adventuresim_core::equipment::EncumbranceSummary;

    #[test]
    fn fireplace_container_rows_expose_shared_browser_metadata_and_open_controls() {
        let source = include_str!("trade.rs");
        let fireplace = source
            .split("pub fn fireplace_page")
            .nth(1)
            .unwrap()
            .split("pub(super) fn herbalism_activity_dialog")
            .next()
            .unwrap();
        assert!(fireplace.contains("data-inventory-browser=\"fireplace-vessels-left\""));
        assert!(fireplace.contains("data-container-open=(object_id)"));
        assert!(fireplace.contains("item_name_with_display(item_id, &display, definition)"));
    }

    #[test]
    fn inferred_general_blacksmith_exposes_limited_weapon_and_armor_stock() {
        let industries = adventuresim_world_schema::InferredIndustryProfile::new(vec![
            adventuresim_world_schema::IndustryEvidence::Fallback(
                adventuresim_world_schema::FallbackIndustry::CommonAggregate,
            ),
        ])
        .unwrap();
        let economy =
            adventuresim_world_schema::infer_settlement_economy(2, 500, 1, false, &industries)
                .unwrap();

        assert!(adventuresim_core::settlement_economy::storefront_stocks(
            &economy,
            adventuresim_core::settlement_economy::Storefront::Weapons,
            "club",
            adventuresim_core::settlement_economy::CatalogKind::Weapon,
        ));
        assert!(adventuresim_core::settlement_economy::storefront_stocks(
            &economy,
            adventuresim_core::settlement_economy::Storefront::Armor,
            "leather vest",
            adventuresim_core::settlement_economy::CatalogKind::Armor,
        ));
        for category in [
            adventuresim_world_schema::StockCategory::Weapons,
            adventuresim_world_schema::StockCategory::Armor,
        ] {
            assert_eq!(
                economy
                    .stock
                    .iter()
                    .find(|stock| stock.category == category)
                    .unwrap()
                    .abundance,
                1
            );
        }
    }

    #[test]
    fn merchant_food_quote_and_weight_follow_remaining_lot() {
        let mut lot = FoodLot {
            id: 1,
            inventory_item_id: Some(9),
            party_inventory_item_id: None,
            display_name: "Roasted venison".into(),
            preparation: FoodPreparation::Stewed,
            ingredient_item_ids: vec!["raw_venison".into()],
            ingredient_quantities: vec![1.0],
            salty_kg: 0.0,
            spicy_kg: 0.0,
            sweet_kg: 0.0,
            sour_kg: 0.0,
            savory_kg: 0.36,
            quality: 3,
            mass_kg: 25.0,
            nutrition_kcal: 5_000.0,
            total_value: 10.0,
            created_at_minute: 1,
        };
        assert_eq!(merchant_inventory_weight(None, Some(&lot)), "25");
        assert_eq!(merchant_inventory_sell_price(None, Some(&lot)), 8);
        let rendered = item_name_with_food_lot("cooked_meal", &lot.display_name, None, Some(&lot))
            .into_string();
        assert!(rendered.contains("Roasted venison"));
        assert!(rendered.contains("item-quality-3"));
        assert!(rendered.contains("title=\"Quality 3\""));
        assert!(!rendered.contains("munition grade"));
        lot.mass_kg = 6.25;
        lot.total_value = 2.5;
        assert_eq!(merchant_inventory_weight(None, Some(&lot)), "6.25");
        assert_eq!(merchant_inventory_sell_price(None, Some(&lot)), 2);
        lot.total_value = 0.5;
        let zero = merchant_inventory_sell_price(None, Some(&lot));
        assert_eq!(zero, 0);
        assert_eq!(
            adventuresim_core::strategic_economy::language_adjusted_sell_price(zero, 0.0),
            0
        );
    }

    #[test]
    fn herbalist_stock_template_includes_every_prepared_course_and_ingredients() {
        let ingredient = crate::spacetimedb::ItemDefinition {
            kind: ItemKind::Ingredient,
            ..Default::default()
        };
        let medication = crate::spacetimedb::ItemDefinition {
            kind: ItemKind::Medication,
            ..Default::default()
        };
        assert!(MerchantShop::Herbalist.stocks(&ingredient));
        assert!(MerchantShop::Herbalist.stocks(&medication));
        let apple = crate::spacetimedb::ItemDefinition {
            id: "apple".into(),
            kind: ItemKind::Food,
            ..Default::default()
        };
        let pan = crate::spacetimedb::ItemDefinition {
            id: "cooking_pan".into(),
            ..Default::default()
        };
        let honey = crate::spacetimedb::ItemDefinition {
            id: "honey".into(),
            kind: ItemKind::Ingredient,
            ..Default::default()
        };
        assert!(MerchantShop::Inn.stocks(&apple));
        assert!(MerchantShop::Inn.stocks(&honey));
        assert!(MerchantShop::Inn.stocks(&pan));
        assert!(!MerchantShop::Inn.stocks(&medication));
        assert!(!adventuresim_core::physiology::INTERVENTION_PROFILES.is_empty());
        let definition = crate::spacetimedb::ItemDefinition {
            id: "black_death_tonic".into(),
            kind: ItemKind::Medication,
            ..Default::default()
        };
        let rendered =
            item_name_with_display("black_death_tonic", "Black Death tonic", Some(&definition))
                .into_string();
        assert!(rendered.contains("data-item-name=\"black_death_tonic\""));
        assert!(rendered.contains("data-item-kind=\"medication\""));
        assert!(rendered.contains(">Black Death tonic</span>"));
    }

    #[test]
    fn encumbrance_meter_formats_exact_text_and_accessible_linear_position() {
        let markup = encumbrance_meter(EncumbranceSummary::new(85.36, 150.0)).into_string();
        assert!(markup.contains("Weight 85.4 / 150.0 kilograms; Penalty -56.9%"));
        assert!(!markup.contains("encumbrance-values"));
        assert!(!markup.contains(">85.4 / 150.0 kg<"));
        assert!(!markup.contains(">-56.9%<"));
        assert!(
            markup.contains(
                "data-strategic-tooltip=\"Weight 85.4 / 150.0 kilograms; Penalty -56.9%\""
            )
        );
        assert!(markup.contains("tabindex=\"0\""));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("aria-valuenow=\"56.9\""));
        assert!(markup.contains("--encumbrance-position: 56.9067%"));
    }

    #[test]
    fn overloaded_meter_keeps_burden_but_clamps_penalty_and_marker() {
        let markup = encumbrance_meter(EncumbranceSummary::new(185.4, 150.0)).into_string();
        assert!(markup.contains("Weight 185.4 / 150.0 kilograms; Penalty -100.0%"));
        assert!(!markup.contains(">185.4 / 150.0 kg<"));
        assert!(markup.contains("--encumbrance-position: 100.0000%"));
    }

    #[test]
    fn encumbrance_css_uses_a_linear_midpoint_gradient_and_contrast_marker() {
        let css = include_str!("../../../static/css/strategic.css");
        assert!(css.contains("linear-gradient(90deg, #238b45 0%, #f4d03f 50%, #c62828 100%)"));
        assert!(css.contains(".encumbrance-marker"));
        assert!(css.contains("background: #fff"));
    }

    #[test]
    fn equipment_slot_dialog_is_centered_in_the_viewport() {
        let css = include_str!("../../../static/css/strategic.css");
        let rule = css
            .split(".equipment-placement-modal {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("equipment placement dialog rule");
        assert!(rule.contains("position: fixed"));
        assert!(rule.contains("inset: 0"));
        assert!(rule.contains("margin: auto"));
        assert!(rule.contains("max-block-size: calc(100dvh - 2rem)"));
    }

    #[test]
    fn encumbrance_rail_scrolls_items_but_keeps_footer_and_meter_outside() {
        let markup = encumbrance_inventory_rail(
            maud::html! { table class="test-items" {} },
            maud::html! { button class="test-footer" {} },
            EncumbranceSummary::new(10.0, 100.0),
        )
        .into_string();
        assert!(markup.contains(
            "<div class=\"encumbrance-inventory-scroll\"><table class=\"test-items\"></table></div><button class=\"test-footer\"></button><div class=\"encumbrance\">"
        ));

        let css = include_str!("../../../static/css/strategic.css");
        assert!(css.contains(".sidebar-section:has(> .encumbrance-inventory-rail)"));
        assert!(css.contains(".encumbrance-inventory-scroll"));
        assert!(css.contains("overflow-y: auto"));
        assert!(css.contains("padding-left: 3.25rem"));
        assert!(css.contains("padding-right: 1.75rem"));
        assert!(css.contains("container-type: inline-size"));
        assert!(css.contains("flex: 1 1 100%"));
        assert!(css.contains("width: 100%"));
        assert!(!markup.contains("encumbrance-values"));
        assert!(css.contains(".encumbrance-meter"));
        assert!(css.contains("width: 100%"));
        assert!(css.contains("@container (max-width: 12rem)"));
        assert!(css.contains("padding-inline: 0.2rem"));
        assert!(css.contains("@container (max-width: 10rem)"));
        assert!(css.contains("padding-inline: 0.1rem"));
        assert!(!css.contains(".encumbrance-values"));
    }

    #[test]
    fn merchant_tabs_render_personal_and_party_encumbrance_as_applicable() {
        let character = Character {
            id: 1,
            name: "Trader".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("viabundus-1".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let render = |shop| {
            live_merchant_shop_page(
                &settlement(),
                &character,
                &[],
                &[],
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                shop,
                1.0,
                0,
                0,
                &[],
                None,
                &[],
                0,
                EncumbranceSummary::new(10.0, 100.0),
                EncumbranceSummary::new(30.0, 200.0),
                None,
                SoapRestPreview::default(),
            )
            .into_string()
        };
        let merchant = render(MerchantShop::Weapons);
        assert!(merchant.contains("action=\"/settlements/viabundus-1/storefront/weapons/offer\""));
        assert!(merchant.contains("data-hard-navigation"));
        assert!(merchant.contains("data-inventory-pane=\"player\""));
        assert!(merchant.contains("data-inventory-pane=\"party\""));
        assert!(
            merchant.contains(
                "data-strategic-tooltip=\"Weight 10.0 / 100.0 kilograms; Penalty -10.0%\""
            )
        );
        assert!(
            merchant.contains(
                "data-strategic-tooltip=\"Weight 30.0 / 200.0 kilograms; Penalty -15.0%\""
            )
        );
        assert!(!merchant.contains(">10.0 / 100.0 kg<"));
        assert!(!merchant.contains(">30.0 / 200.0 kg<"));

        let herbalist = render(MerchantShop::Herbalist);
        assert!(
            herbalist.contains("aria-valuetext=\"Weight 10.0 / 100.0 kilograms; Penalty -10.0%\"")
        );
        assert!(!herbalist.contains(">10.0 / 100.0 kg<"));
        assert!(!herbalist.contains("data-inventory-pane=\"party\""));
        assert!(!herbalist.contains("Weight 30.0 / 200.0 kilograms"));

        let inn = render(MerchantShop::Inn);
        assert!(inn.contains("Cooking supplies"));
        assert!(inn.contains("aria-label=\"Inn rest service\""));
        assert!(inn.contains("action=\"/settlements/viabundus-1/storefront/inn/offer\""));
        assert!(inn.contains("class=\"inn-rest-panel\""));
        assert!(inn.contains("aria-label=\"Inn lodging and rest\""));
    }

    #[test]
    fn inn_catalog_renders_an_authoritatively_quoted_travel_ration_purchase() {
        let mut town = settlement();
        town.economy.services = vec![adventuresim_world_schema::SettlementService::Inn];
        let character = Character {
            id: 1,
            name: "Traveller".into(),
            xp: 0,
            level: 1,
            gold: 20,
            current_settlement_id: Some(town.id.clone()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let ration = ItemDefinition {
            id: "travel_ration".into(),
            weight: 0.65,
            base_value: Some(3),
            nutrition_kcal: 2_500.0,
            kind: ItemKind::Food,
            ..Default::default()
        };

        let markup = live_merchant_shop_page(
            &town,
            &character,
            &[],
            std::slice::from_ref(&ration),
            &[],
            &[],
            None,
            &[],
            &[],
            &[],
            MerchantShop::Inn,
            1.0,
            0,
            0,
            &[],
            None,
            &[],
            0,
            EncumbranceSummary::default(),
            EncumbranceSummary::default(),
            None,
            SoapRestPreview::default(),
        )
        .into_string();

        assert!(markup.contains("data-merchant-item=\"travel_ration\""));
        assert!(markup.contains("data-merchant-buy=\"travel_ration\""));
        assert!(markup.contains("data-merchant-buy-price=\"5\""));
        assert!(markup.contains(">0.65<"));
    }

    #[test]
    fn disabled_repair_explanation_is_hoverable_and_focusable() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.0,
            tier_2: 0.0,
            tier_3: 0.0,
            tier_4: 0.2,
            tier_5: 0.0,
        };
        let rendered =
            repair_submit_control(&settlement(), "weapons", 4, Some(&condition), 3).into_string();
        assert!(rendered.contains("disabled-repair-explanation"));
        assert!(rendered.contains("tabindex=\"0\""));
        assert!(rendered.contains("All damage requires Smithing"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn tailor_repair_control_targets_the_clothing_service() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.25,
            tier_2: 0.0,
            tier_3: 0.0,
            tier_4: 0.0,
            tier_5: 0.0,
        };
        let rendered =
            repair_submit_control(&settlement(), "clothing", 4, Some(&condition), 2).into_string();
        assert!(rendered.contains("/clothing/repair"));
        assert!(rendered.contains("row-repair-form"));
        assert!(!rendered.contains("disabled"));
    }

    #[test]
    fn collapsed_currency_label_hides_the_historical_denomination() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "lubeck_mark".into(),
            kind: crate::spacetimedb::ItemKind::Currency,
            base_value: Some(1),
            weight: 0.01,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains(">Coin<"));
        assert!(rendered.contains("data-item-edit-url=\"https://github.com/"));
        assert!(rendered.contains("content/items/catalog.yaml#L"));
        assert!(rendered.contains("data-currency-name=\"Lübeck mark\""));
        assert!(!rendered.contains(">Lübeck mark<"));
    }

    #[test]
    fn alcohol_labels_expose_a_shared_inventory_group() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "small_beer".into(),
            kind: crate::spacetimedb::ItemKind::Simple,
            alcohol_serving_ml: 500,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("data-item-name=\"small_beer\""));
        assert!(rendered.contains("data-item-group=\"alcohol\""));
        assert!(rendered.contains("data-group-name=\"Alcohol\""));
        assert!(rendered.contains("data-item-edit-url=\"https://github.com/"));
    }

    #[test]
    fn smith_player_actions_keep_sell_and_repair_in_one_hover_area() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "torch", 2, 3, 1, true, Some(repair)).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions smith-player-actions\">"));
        assert_eq!(rendered.matches("data-merchant-sell=\"").count(), 1);
        assert!(rendered.contains("data-dynamic-transfer"));
        assert!(rendered.contains("data-default-transfer-mode=\"one\""));
        assert!(rendered.contains("data-label-target=\"Sell surplus Torch\""));
        assert!(rendered.contains("data-label-all=\"Sell all Torch\""));
        assert!(rendered.contains("row-repair-form"));
        assert_eq!(rendered.matches("smith-player-actions").count(), 1);
    }

    #[test]
    fn equipped_smith_items_retain_the_repair_action_without_sell_controls() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "sword", 10, 1, 0, false, Some(repair)).into_string();

        assert!(rendered.contains("smith-player-actions"));
        assert!(rendered.contains("row-repair-form"));
        assert!(!rendered.contains("data-merchant-sell"));
        assert!(rendered.contains("Equipped items cannot be sold"));
        assert!(rendered.contains("trade-transfer trade-transfer-left"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn non_smith_sell_controls_do_not_reserve_a_repair_slot() {
        let rendered = merchant_sell_repair_controls(4, "shirt", 3, 1, 0, true, None).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions\">"));
        assert!(!rendered.contains("smith-player-actions"));
        assert!(rendered.contains("data-merchant-sell"));
    }

    #[test]
    fn unavailable_transfer_button_keeps_a_disabled_action_slot() {
        let rendered =
            disabled_transfer_button("left", "Equipped items cannot be transferred").into_string();

        assert!(rendered.contains("trade-transfer trade-transfer-left"));
        assert!(rendered.contains("Equipped items cannot be transferred"));
        assert!(rendered.contains("disabled"));
        assert!(rendered.contains("inventory-transfer-glyph"));
    }

    #[test]
    fn durability_bar_uses_qualitative_copy_and_marks_smith_repairable_damage() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.0,
            tier_3: 0.2,
            tier_4: 0.1,
            tier_5: 0.0,
        };
        let rendered = condition_bar(Some(&condition), Some(3)).into_string();
        assert!(rendered.contains("condition-repairable"));
        for tier in 1..=5 {
            assert!(rendered.contains(&format!("condition-tier-{tier}")));
        }
        assert!(rendered.contains("flashing portion can be repaired"));
        assert!(!rendered.contains("condition-number"));
        assert!(!rendered.contains("% condition"));
    }

    #[test]
    fn durable_item_names_expose_quality_color_and_description() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "commissioned_sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 4,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            ..Default::default()
        };

        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("item-quality-4"));
        assert!(rendered.contains("knightly commission"));
    }

    #[test]
    fn book_names_use_shared_quality_color_without_equipment_copy() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "human_anatomy".into(),
            kind: crate::spacetimedb::ItemKind::Simple,
            quality: 4,
            ..Default::default()
        };

        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("item-quality-4"));
        assert!(rendered.contains("title=\"Quality 4\""));
        assert!(!rendered.contains("knightly commission"));
    }

    #[test]
    fn completed_repair_bar_projects_the_condition_before_retrieval() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.2,
            tier_3: 0.0,
            tier_4: 0.0,
            tier_5: 0.0,
        };

        let rendered = completed_repair_condition_bar(Some(&condition), 3).into_string();
        assert!(rendered.contains("Full durability"));
        assert!(rendered.contains("width:100%"));
    }

    #[test]
    fn smith_player_inventory_uses_the_compact_seven_column_table() {
        let rendered = trade_inventory_table(
            "test",
            InventoryColumnSet::Weapons,
            true,
            true,
            true,
            html! {},
        )
        .into_string();
        assert!(rendered.contains("smith-player-inventory-table"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("aria-label=\"Item type\""));
        assert!(rendered.contains("inventory-column-durability"));
        assert!(rendered.contains("hammer-nails.svg"));
        assert!(!rendered.contains("Repair all eligible items"));
        assert!(!rendered.contains("durability-header-label"));
    }

    #[test]
    fn repair_all_precedes_the_sell_bulk_control() {
        let rendered = inventory_footer_controls_with_leading(
            Some(repair_all_control(&settlement(), "weapons")),
            "sell",
            "Sell surplus",
            "Sell everything",
        )
        .into_string();
        let repair = rendered.find("inventory-footer-repair").unwrap();
        let sell = rendered.find("data-inventory-bulk=\"sell\"").unwrap();
        assert!(rendered.contains("inventory-footer-actions-grouped"));
        assert!(repair < sell);
    }

    #[test]
    fn equipment_slot_control_is_enabled_only_for_authored_placements() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "sword".into(),
            qty: 1,
        };
        let mut definition = crate::spacetimedb::ItemDefinition {
            id: "sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 3,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            equipment_placements: vec![EquipmentPlacement {
                id: "left_hand".into(),
                occupancy: vec![EquipmentOccupancyRequirement {
                    location: EquipmentLocation::LeftHand,
                    channel: EquipmentChannel::Held,
                    order: 0,
                }],
                parents: Vec::new(),
                protection: Vec::new(),
            }],
            ..Default::default()
        };
        let enabled =
            equipment_control(&inventory, Some(&definition), false, true, None).into_string();
        assert!(enabled.contains("data-equipment-toggle"));
        assert!(enabled.contains("equipment-slot-control"));
        assert!(enabled.contains("data-equipment-input-map"));
        assert!(!enabled.contains(" disabled"));
        definition.equipment_placements.clear();
        let disabled =
            equipment_control(&inventory, Some(&definition), false, true, None).into_string();
        assert!(disabled.contains("class=\"equipment-unavailable\""));
        assert!(disabled.contains("aria-label=\"Not equippable\""));
        assert!(!disabled.contains("equipment-slot-control"));

        definition.equipment_placements.push(EquipmentPlacement {
            id: "left_hand".into(),
            occupancy: vec![EquipmentOccupancyRequirement {
                location: EquipmentLocation::LeftHand,
                channel: EquipmentChannel::Held,
                order: 0,
            }],
            parents: Vec::new(),
            protection: Vec::new(),
        });
        let unbound =
            equipment_control(&inventory, Some(&definition), true, true, None).into_string();
        assert!(unbound.contains("check-mark.svg"));
        assert!(unbound.contains("Equipped; no keyboard binding"));
        assert!(!unbound.contains(">?</kbd>"));
    }

    #[test]
    fn equipped_slot_control_lists_all_keys_and_darkens_inner_layers() {
        let outer = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "cloak".into(),
            qty: 1,
        };
        let inner = InventoryItem {
            id: 8,
            character_id: 9,
            item_id: "tunic".into(),
            qty: 1,
        };
        let placement = |id: &str, channel| EquipmentPlacement {
            id: id.into(),
            occupancy: vec![
                EquipmentOccupancyRequirement {
                    location: EquipmentLocation::Chest,
                    channel,
                    order: 0,
                },
                EquipmentOccupancyRequirement {
                    location: EquipmentLocation::Stomach,
                    channel,
                    order: 0,
                },
            ],
            parents: Vec::new(),
            protection: vec![EquipmentBodyPart::Chest],
        };
        let definition = crate::spacetimedb::ItemDefinition {
            id: outer.item_id.clone(),
            kind: crate::spacetimedb::ItemKind::Clothing,
            coverage: 0.8,
            equipment_placements: vec![placement("worn", EquipmentChannel::Outerwear)],
            ..Default::default()
        };
        let occupancy = |inventory_item_id, location, channel| EquipmentOccupancy {
            id: format!("{inventory_item_id}:{location:?}"),
            character_id: 9,
            inventory_item_id,
            anchor_kind: EquipmentAnchorKind::CharacterLocation,
            location: Some(location),
            parent_inventory_item_id: None,
            attachment_point_id: None,
            channel,
            order: 0,
            requirement_index: 0,
            capacity_index: 0,
        };
        let graph = CharacterEquipmentGraph {
            _character_id: 9,
            worn_item_ids: vec![outer.id, inner.id],
            equipment_nodes: vec![
                CharacterEquippedItem {
                    inventory_item_id: outer.id,
                    character_id: 9,
                    placement_id: "worn".into(),
                    item_name: outer.item_id.clone(),
                },
                CharacterEquippedItem {
                    inventory_item_id: inner.id,
                    character_id: 9,
                    placement_id: "worn".into(),
                    item_name: inner.item_id.clone(),
                },
            ],
            equipment_occupancies: vec![
                occupancy(
                    outer.id,
                    EquipmentLocation::Chest,
                    EquipmentChannel::Outerwear,
                ),
                occupancy(
                    outer.id,
                    EquipmentLocation::Stomach,
                    EquipmentChannel::Outerwear,
                ),
                occupancy(
                    inner.id,
                    EquipmentLocation::Chest,
                    EquipmentChannel::BaseClothing,
                ),
            ],
            attachment_targets: Vec::new(),
        };

        let outer_control =
            equipment_control(&outer, Some(&definition), true, true, Some(&graph)).into_string();
        assert!(outer_control.contains(">G</kbd>"));
        assert!(outer_control.contains(">Y</kbd>"));
        assert!(outer_control.contains("--equipment-layer-lightness: 88%"));
        assert!(outer_control.contains("aria-label=\"Unequip Cloak. Slots: Chest, Stomach. Protects Chest (80% coverage). Keyboard: G = Chest, layer 1; Y = Stomach, layer 1\""));
        assert!(outer_control.contains("data-strategic-tooltip=\"G: Chest, layer 1\""));
        assert!(outer_control.contains("protects Chest (80% coverage)"));

        let inner_definition = crate::spacetimedb::ItemDefinition {
            id: inner.item_id.clone(),
            kind: crate::spacetimedb::ItemKind::Clothing,
            equipment_placements: vec![placement("worn", EquipmentChannel::BaseClothing)],
            ..Default::default()
        };
        let inner_control =
            equipment_control(&inner, Some(&inner_definition), true, true, Some(&graph))
                .into_string();
        assert!(inner_control.contains(">G</kbd>"));
        assert!(inner_control.contains("--equipment-layer-lightness: 79%"));
    }

    #[test]
    fn attached_item_names_its_parent_on_the_inventory_row() {
        let parent = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "belt".into(),
            qty: 1,
        };
        let child = InventoryItem {
            id: 8,
            character_id: 9,
            item_id: "pouch".into(),
            qty: 1,
        };
        let definition = crate::spacetimedb::ItemDefinition {
            id: child.item_id.clone(),
            kind: crate::spacetimedb::ItemKind::Container,
            equipment_placements: vec![EquipmentPlacement {
                id: "hung".into(),
                occupancy: Vec::new(),
                parents: Vec::new(),
                protection: Vec::new(),
            }],
            ..Default::default()
        };
        let graph = CharacterEquipmentGraph {
            _character_id: 9,
            worn_item_ids: vec![parent.id, child.id],
            equipment_nodes: vec![
                CharacterEquippedItem {
                    inventory_item_id: parent.id,
                    character_id: 9,
                    placement_id: "worn".into(),
                    item_name: parent.item_id.clone(),
                },
                CharacterEquippedItem {
                    inventory_item_id: child.id,
                    character_id: 9,
                    placement_id: "hung".into(),
                    item_name: child.item_id.clone(),
                },
            ],
            equipment_occupancies: vec![
                EquipmentOccupancy {
                    id: "belt:front".into(),
                    character_id: 9,
                    inventory_item_id: parent.id,
                    anchor_kind: EquipmentAnchorKind::CharacterLocation,
                    location: Some(EquipmentLocation::FrontBelt),
                    parent_inventory_item_id: None,
                    attachment_point_id: None,
                    channel: EquipmentChannel::Mount,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                },
                EquipmentOccupancy {
                    id: "pouch:belt".into(),
                    character_id: 9,
                    inventory_item_id: child.id,
                    anchor_kind: EquipmentAnchorKind::ItemAttachment,
                    location: None,
                    parent_inventory_item_id: Some(parent.id),
                    attachment_point_id: Some("front-loop".into()),
                    channel: EquipmentChannel::Containment,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                },
            ],
            attachment_targets: Vec::new(),
        };

        let rendered =
            equipment_control(&child, Some(&definition), true, true, Some(&graph)).into_string();
        assert!(rendered.contains(">F</kbd>"));
        assert!(rendered.contains(
            "aria-label=\"Unequip Pouch. Slots: Front belt. Attached: Belt / front-loop. Keyboard:"
        ));
        assert!(rendered.contains("F = Front belt"));
        assert!(rendered.contains(
            "<span class=\"equipment-graph-summary\">Attached: Belt / front-loop</span>"
        ));
    }

    #[test]
    fn medication_checkbox_describes_administration_instead_of_equipping() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "oral_rehydration_draught".into(),
            qty: 1,
        };
        let definition = crate::spacetimedb::ItemDefinition {
            id: inventory.item_id.clone(),
            slot: ItemSlot::None,
            kind: crate::spacetimedb::ItemKind::Medication,
            ..Default::default()
        };
        let rendered =
            equipment_control(&inventory, Some(&definition), false, true, None).into_string();
        assert!(!rendered.contains(" disabled"));
        assert!(rendered.contains("data-equipment-medication"));
        assert!(rendered.contains("type=\"checkbox\""));
        assert!(rendered.contains("aria-label=\"Administer Oral rehydration draught\""));
        assert!(rendered.contains("title=\"Administer one standard course of this preparation\""));
        assert!(!rendered.contains("Equip Oral rehydration draught"));
    }

    #[test]
    fn companion_medication_checkbox_is_disabled_with_honest_copy() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 10,
            item_id: "oral_rehydration_draught".into(),
            qty: 1,
        };
        let definition = crate::spacetimedb::ItemDefinition {
            id: inventory.item_id.clone(),
            slot: ItemSlot::None,
            kind: crate::spacetimedb::ItemKind::Medication,
            ..Default::default()
        };
        let rendered =
            equipment_control(&inventory, Some(&definition), false, false, None).into_string();
        assert!(rendered.contains(" disabled"));
        assert!(
            rendered
                .contains("aria-label=\"Only Oral rehydration draught's owner can administer it\"")
        );
        assert!(
            rendered.contains("title=\"Select this character to administer their preparation\"")
        );
        assert!(!rendered.contains("aria-label=\"Administer Oral rehydration draught\""));
    }

    #[test]
    fn merchant_stock_table_hides_quantity_and_target_columns() {
        let rendered = trade_inventory_table(
            "merchant-left",
            InventoryColumnSet::Basic,
            false,
            false,
            false,
            html! {},
        )
        .into_string();
        assert!(rendered.contains("<colgroup>"));
        assert!(!rendered.contains("inventory-column-count"));
        assert!(!rendered.contains("inventory-column-target"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("inventory-column-weight"));
        assert!(rendered.contains("inventory-column-gold"));
        assert!(rendered.contains("title=\"Currency\""));
        assert!(rendered.contains("aria-label=\"Currency\""));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
    }

    #[test]
    fn inventory_type_header_and_row_share_the_first_column() {
        let rendered = trade_inventory_table(
            "test",
            InventoryColumnSet::Basic,
            true,
            false,
            false,
            html! {
                tr class="trade-inventory-row" {
                    td class="inventory-item-type" { (item_type_icon("arming_sword")) }
                    td class="inventory-item-name" { "Arming sword" }
                    td { "1" } td { "1" } td { "12" }
                }
            },
        )
        .into_string();
        let header = rendered.find("inventory-column-type").unwrap();
        let item_header = rendered.find("inventory-column-item").unwrap();
        let type_cell = rendered.find("inventory-item-type").unwrap();
        let item_cell = rendered.find("inventory-item-name").unwrap();
        assert!(header < item_header && type_cell < item_cell);
        assert!(rendered.contains("/static/icons/game/broadsword.svg"));
    }

    #[test]
    fn smith_custody_panel_shows_only_matching_service_orders() {
        let orders = [
            crate::spacetimedb::RepairOrder {
                id: 1,
                owner_character_id: 9,
                inventory_item_id: 11,
                item_id: "sword".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                equipped_placement_id: None,
                attachment_targets: Vec::new(),
                quoted_cost: 12,
            },
            crate::spacetimedb::RepairOrder {
                id: 2,
                owner_character_id: 9,
                inventory_item_id: 12,
                item_id: "cuirass".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                equipped_placement_id: None,
                attachment_targets: Vec::new(),
                quoted_cost: 24,
            },
        ];
        let items = [
            crate::spacetimedb::ItemDefinition {
                id: "sword".into(),
                weight: 1.0,
                slot: ItemSlot::AnyHolding,
                kind: crate::spacetimedb::ItemKind::Weapon,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
                ..Default::default()
            },
            crate::spacetimedb::ItemDefinition {
                id: "cuirass".into(),
                weight: 1.0,
                slot: ItemSlot::Chest,
                kind: crate::spacetimedb::ItemKind::Armor,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
                ..Default::default()
            },
        ];
        let weapons = repair_custody_panel(
            &settlement(),
            MerchantShop::Weapons,
            &orders,
            &[],
            &items,
            0,
            4,
        )
        .into_string();
        let armor = repair_custody_panel(
            &settlement(),
            MerchantShop::Armor,
            &orders,
            &[],
            &items,
            0,
            3,
        )
        .into_string();
        assert!(weapons.contains("sword"));
        assert!(!weapons.contains("cuirass"));
        assert!(weapons.contains("repair-custody-table"));
        assert!(weapons.contains("Smithing 4"));
        assert!(weapons.contains("stat-icon-smithing"));
        for tier in 1..=5 {
            assert!(weapons.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert!(weapons.contains("repair-custody-header-actions"));
        assert!(weapons.contains("inventory-actions-header"));
        assert!(weapons.contains("inventory-actions-cell"));
        assert!(weapons.contains("Durability"));
        assert!(weapons.contains("ETA"));
        assert!(weapons.contains("Full repair cost"));
        assert!(weapons.contains("repair-retrieve-all"));
        assert!(weapons.contains("Retrieve up to two completed matching items"));
        assert!(!weapons.to_lowercase().contains("affordable prefix"));
        assert!(weapons.contains("/repairs/1/retrieve"));
        assert!(weapons.contains(">12<"));
        assert!(!weapons.contains("Target "));
        assert!(armor.contains("cuirass"));
        assert!(!armor.contains("sword"));
    }

    #[test]
    fn attachment_target_filter_rejects_self_and_descendants() {
        let edge = |child, parent| EquipmentOccupancy {
            id: format!("{parent}->{child}"),
            character_id: 1,
            inventory_item_id: child,
            anchor_kind: EquipmentAnchorKind::ItemAttachment,
            location: None,
            parent_inventory_item_id: Some(parent),
            attachment_point_id: Some("point".into()),
            channel: EquipmentChannel::Mount,
            order: 0,
            requirement_index: 0,
            capacity_index: 0,
        };
        let equip = CharacterEquipmentGraph {
            _character_id: 1,
            worn_item_ids: vec![10, 20, 30],
            equipment_nodes: Vec::new(),
            equipment_occupancies: vec![edge(20, 10), edge(30, 20)],
            attachment_targets: Vec::new(),
        };
        assert!(equipment_target_is_self_or_descendant(&equip, 10, 10));
        assert!(equipment_target_is_self_or_descendant(&equip, 10, 30));
        assert!(!equipment_target_is_self_or_descendant(&equip, 30, 10));
    }
}
