//! Merchant storefront policy, quoted stock, and trade presentation.

use super::super::service::service_page;
use super::*;
use super::{equipment::*, inventory::*, repairs::*};

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

    pub fn available_at(self, settlement: &SettlementView) -> bool {
        adventuresim_core::settlement_economy::storefront_available(
            &settlement.economy,
            self.storefront(),
        ) || (matches!(self, Self::Weapons)
            && adventuresim_core::organization::organization_service_chapter(
                &settlement.id,
                self.service_id(),
            )
            .is_some())
    }

    fn stocks_at(
        self,
        settlement: &SettlementView,
        item: &crate::spacetimedb::CatalogItemView,
    ) -> bool {
        use adventuresim_core::settlement_economy::CatalogKind as C;
        let kind = match item.kind {
            crate::spacetimedb::CatalogItemKind::Simple => C::Simple,
            crate::spacetimedb::CatalogItemKind::Weapon => C::Weapon,
            crate::spacetimedb::CatalogItemKind::Armor => C::Armor,
            crate::spacetimedb::CatalogItemKind::Shield => C::Shield,
            crate::spacetimedb::CatalogItemKind::Clothing => C::Clothing,
            crate::spacetimedb::CatalogItemKind::Container => C::Simple,
            crate::spacetimedb::CatalogItemKind::Currency => C::Currency,
            crate::spacetimedb::CatalogItemKind::Ingredient => C::Ingredient,
            crate::spacetimedb::CatalogItemKind::Medication => C::Medication,
            crate::spacetimedb::CatalogItemKind::Food => C::Food,
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

    pub(super) fn stocks(self, item: &crate::spacetimedb::CatalogItemView) -> bool {
        let kind = item.kind;
        match self {
            Self::General => !matches!(
                kind,
                crate::spacetimedb::CatalogItemKind::Currency
                    | crate::spacetimedb::CatalogItemKind::Ingredient
                    | crate::spacetimedb::CatalogItemKind::Medication
            ),
            Self::Weapons => matches!(
                kind,
                crate::spacetimedb::CatalogItemKind::Weapon
                    | crate::spacetimedb::CatalogItemKind::Shield
            ),
            Self::Armor => kind == crate::spacetimedb::CatalogItemKind::Armor,
            Self::Clothing => kind == crate::spacetimedb::CatalogItemKind::Clothing,
            Self::Herbalist => matches!(
                kind,
                crate::spacetimedb::CatalogItemKind::Ingredient
                    | crate::spacetimedb::CatalogItemKind::Medication
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

    fn shows_inventory(self, item: &crate::spacetimedb::CatalogItemView) -> bool {
        item.kind == crate::spacetimedb::CatalogItemKind::Currency || self.stocks(item)
    }
}

pub fn merchants_page(
    settlement: &SettlementView,
    active_character: Option<&CharacterView>,
    inventory: &[InventoryItem],
    food_lots: &[FoodLot],
    party_members: &[CharacterView],
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

#[expect(
    clippy::too_many_arguments,
    reason = "the merchant page boundary composes independently loaded stock, custody, and service projections"
)]
pub fn live_merchant_shop_page(
    settlement: &SettlementView,
    character: &CharacterView,
    inventory: &[InventoryItem],
    personal_amounts: &[InventoryItemAmount],
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    party_members: &[CharacterView],
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
    let player_footer = if matches!(shop, MerchantShop::Herbalist | MerchantShop::Weapons) {
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
        @if matches!(shop, MerchantShop::Weapons) {
            section class="sidebar-section forge-customization" data-forge-customization data-live-preserve="forge-customization" {
                h2 { "Forge a weapon" }
                form method="post" action=(format!("/settlements/{}/weapons/forge", settlement.id)) {
                    label { "Chassis" select data-forge-catalog aria-label="Weapon chassis" {} }
                    div class="forge-recipe-editor" data-forge-editor aria-live="polite" { "Loading complete weapon recipe…" }
                    input type="hidden" name="recipe" data-forge-recipe;
                    dl class="forge-material-staging" data-forge-materials aria-live="polite" {}
                    div class="forge-submit-row" {
                        button type="submit" class="btn btn-primary" disabled data-forge-submit { "Forge" }
                        span data-forge-eta { "Calculating…" }
                    }
                }
            }
        }
        @if !matches!(shop, MerchantShop::Weapons) {
        (sidebar_section(if matches!(shop, MerchantShop::Herbalist) { "Existing preparations and ingredients" } else if matches!(shop, MerchantShop::Inn) { "Cooking supplies" } else { "Merchant stock" }, html! {
            div class="smith-wares-scroll" {
            @if stocked_items.is_empty() {
                (empty_state("No stock is available here.", None, None))
            } @else {
            (trade_inventory_table("merchant-left", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, false, false, false, html! {
                @for item in stocked_items.iter().copied() {
                    @let is_currency = item.kind == crate::spacetimedb::CatalogItemKind::Currency;
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
        }
        @if matches!(shop, MerchantShop::Inn) {
            section class="inn-rest-panel" aria-label="Inn lodging and rest" {
                (rest_service_menu("Inn", &settlement.id, RestServiceKind::Inn, rest_default_minutes, None, soap_preview))
            }
        }
        }
        @if matches!(shop, MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing) {
            (repair_custody_panel(settlement, shop, repair_orders, conditions, items, now_minutes, smith_skill))
        }
        }
        main class="center-content settlement-main" { (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), None)) (npc_portrait_strip(&settlement.id, npc_location_id(service_id))) @if matches!(shop, MerchantShop::Weapons) { (forge_description_stage(title, "Forge preview loading")) } @else { (npc_description_stage(title, "Merchant counter and attending craftsperson")) } (settlement_resident_chat_area(title, Some(character), &settlement.id, npc_location_id(service_id), Some(service_id))) form # "merchant-offer" class="party-offer" action=(if matches!(shop, MerchantShop::Herbalist) { format!("/settlements/{}/herbalist/purchase", settlement.id) } else { format!("/settlements/{}/storefront/{service_id}/offer", settlement.id) }) method="post" hidden role="dialog" aria-modal="true" aria-label="Confirm merchant offer" tabindex="-1" { span class="party-offer-summary" { "Review and submit the staged trade." } input type="hidden" name="return_to" value=(format!("/settlements/{}/{}", settlement.id, service_id)); input type="hidden" name="inventory_scope" value="player"; button type="button" class="party-offer-cancel" data-cancel-trade="merchant" { "Cancel" } button type="submit" disabled { "Offer" } } }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Trading inventory" {
                button type="button" class="inventory-owner-tab active" data-inventory-tab="player" { "Player" }
                @if !matches!(shop, MerchantShop::Herbalist | MerchantShop::Weapons) {
                    button type="button" class="inventory-owner-tab" data-inventory-tab="party" { "Party" }
                }
            }
            div data-inventory-pane="player" {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-player-right", if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, !matches!(shop, MerchantShop::Weapons), matches!(shop, MerchantShop::Armor | MerchantShop::Clothing), html! {
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| if matches!(shop, MerchantShop::Weapons) { matches!(definition.id.as_str(), "steel_stock" | "leather_stock" | "brass_stock" | "wood_stock") } else { shop.shows_inventory(definition) })) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                        @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::CatalogItemKind::Currency);
                        @let is_equipped = equip.is_some_and(|equip| equip.contains(item.id));
                        @let sell_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_sell_price(merchant_inventory_sell_price(definition, food_lot), trade_language), -problem_sell_penalty_bps);
                        @let target = target_quantity(personal_targets, &item.item_id);
                        @let measured_fraction = personal_amounts.iter().find(|amount| amount.inventory_item_id == item.id).map(|amount| adventuresim_core::inventory_measurement::ConsumableFractionMicros::try_new(amount.remaining_fraction_micros).expect("public consumable fraction must not exceed one whole"));
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-personal-inventory-id=(item.id) data-merchant-equipped=(is_equipped) data-inventory-quantity=(item.quantity) data-target=(target) {
                        @let condition = conditions.iter().find(|condition| condition.inventory_item_id == item.id);
                        @let repair_skill = smith_skill;
                        @let durable_item = definition.is_some_and(|definition| definition.repairable);
                        @let service_matches = definition.is_some_and(|definition| if matches!(shop, MerchantShop::Armor) { definition.kind == crate::spacetimedb::CatalogItemKind::Armor } else if matches!(shop, MerchantShop::Clothing) { definition.kind == crate::spacetimedb::CatalogItemKind::Clothing } else { matches!(definition.kind, crate::spacetimedb::CatalogItemKind::Weapon | crate::spacetimedb::CatalogItemKind::Shield) });
                        @let can_sell = !is_currency && !is_equipped;
                        td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                        td class="inventory-item-name" { (item_name_with_food_lot(&item.item_id, &food_display_name, definition, food_lot)) @if !matches!(shop, MerchantShop::Herbalist | MerchantShop::Weapons) && (can_sell || service_matches) { (merchant_sell_repair_controls(item.id, &item.item_id, sell_price, item.quantity, target, can_sell, service_matches.then(|| repair_submit_control(settlement, service_id, item.id, condition, repair_skill)))) } }
                        td class="inventory-count" { @if matches!(shop, MerchantShop::Weapons) { (format!("{:.3} kg", measured_fraction.map_or(0.0, adventuresim_core::inventory_measurement::ConsumableFractionMicros::as_unit_f32) * definition.map_or(0.0, |definition| definition.weight))) } @else { (quantity_target_control(item.quantity, target, &item.item_id, false)) } } td class="inventory-equipped" { (equipment_control(item, definition, is_equipped, true, equip)) } td class="inventory-durability" { @if durable_item { (condition_bar(condition, service_matches.then_some(repair_skill))) } @else { "—" } } td class="inventory-weight" { (merchant_inventory_weight(definition, food_lot)) } td class="inventory-gold" { (sell_price) }
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
            @if !matches!(shop, MerchantShop::Herbalist | MerchantShop::Weapons) { div data-inventory-pane="party" hidden {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-party-right", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, false, false, html! {
                    @for item in pooled.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.party_inventory_item_id == Some(item.id));
                        @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::CatalogItemKind::Currency);
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
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| definition.kind == crate::spacetimedb::CatalogItemKind::Currency)) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        tr class="trade-inventory-row trade-row-player party-personal-currency" data-merchant-item=(&item.item_id) data-inventory-quantity=(item.quantity) data-target="0" title="Personal coin available for party purchases" {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) }
                            td class="inventory-count" { (item.quantity) }
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

pub(super) fn merchant_inventory_weight(
    definition: Option<&crate::spacetimedb::CatalogItemView>,
    food_lot: Option<&FoodLot>,
) -> String {
    food_lot.map_or_else(
        || item_weight(definition),
        |lot| weight_display(lot.mass_kg),
    )
}

pub(super) fn merchant_inventory_sell_price(
    definition: Option<&crate::spacetimedb::CatalogItemView>,
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
pub(super) fn merchant_buy_controls(
    item_id: &str,
    price: u32,
    target: u32,
    available: u32,
) -> Markup {
    let item_name = item_display_name(item_id);
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="one" data-target=(target) data-count=(available) data-label-one=(format!("Buy one {item_name}")) data-label-target=(format!("Buy {item_name} to target")) data-label-all=(format!("Buy all {item_name}")) aria-label=(format!("Buy one {item_name}")) title=(format!("Buy one {item_name}")) { (transfer_glyph(1)) }
    } }
}

pub(super) fn merchant_sell_controls(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use crate::templates::settlement::test_support::*;
    use adventuresim_core::equipment::EncumbranceSummary;

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
    fn authored_weapons_service_chapter_exposes_forge_without_economy_storefront() {
        let mut guildhall = settlement();
        guildhall.id = "viabundus-0".into();
        guildhall.economy =
            adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        assert!(MerchantShop::Weapons.available_at(&guildhall));

        guildhall.id = "settlement-without-weapons-chapter".into();
        assert!(!MerchantShop::Weapons.available_at(&guildhall));
    }

    #[test]
    fn forge_client_restores_live_recipe_and_exposes_manual_preview_controls() {
        let renderer = include_str!("../../../../static/strategic-renderer.js");
        assert!(renderer.contains("strategic-live-regions-refreshed"));
        assert!(renderer.contains("currentForgeDesign"));
        assert!(renderer.contains("wasm_weapon_editor_fields"));
        assert!(!renderer.contains("numericBounds"));
        assert!(renderer.contains(r#"type: "orbit-forge""#));
        assert!(renderer.contains(r#"type: "zoom-forge""#));

        let dialogue = include_str!("../../../../static/dialogue-client.js");
        assert!(dialogue.contains("data-repair-custody-service"));
        assert!(dialogue.contains("npc.service_id !== panel.dataset.repairCustodyService"));

        let css = include_str!("../../../../static/css/strategic.css");
        assert!(css.contains(".settlement-main:has(.forge-description-stage)"));
        assert!(css.contains("background: transparent"));
        assert!(css.contains(".repair-custody-panel[hidden] { display: none; }"));
    }

    #[test]
    fn merchant_food_quote_and_weight_follow_remaining_lot() {
        let mut lot = FoodLot {
            id: 1,
            inventory_item_id: Some(9),
            party_inventory_item_id: None,
            material_revision: 1,
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
        let ingredient = crate::spacetimedb::CatalogItemView {
            kind: CatalogItemKind::Ingredient,
            ..Default::default()
        };
        let medication = crate::spacetimedb::CatalogItemView {
            kind: CatalogItemKind::Medication,
            ..Default::default()
        };
        assert!(MerchantShop::Herbalist.stocks(&ingredient));
        assert!(MerchantShop::Herbalist.stocks(&medication));
        let apple = crate::spacetimedb::CatalogItemView {
            id: "apple".into(),
            kind: CatalogItemKind::Food,
            ..Default::default()
        };
        let pan = crate::spacetimedb::CatalogItemView {
            id: "cooking_pan".into(),
            ..Default::default()
        };
        let honey = crate::spacetimedb::CatalogItemView {
            id: "honey".into(),
            kind: CatalogItemKind::Ingredient,
            ..Default::default()
        };
        assert!(MerchantShop::Inn.stocks(&apple));
        assert!(MerchantShop::Inn.stocks(&honey));
        assert!(MerchantShop::Inn.stocks(&pan));
        assert!(!MerchantShop::Inn.stocks(&medication));
        assert!(!adventuresim_core::physiology::INTERVENTION_PROFILES.is_empty());
        let definition = crate::spacetimedb::CatalogItemView {
            id: "black_death_tonic".into(),
            kind: CatalogItemKind::Medication,
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
    fn merchant_tabs_render_personal_and_party_encumbrance_as_applicable() {
        let character = CharacterView {
            id: 1,
            name: "Trader".into(),
            xp: 0,
            level: 1,
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
        assert!(merchant.contains("data-bevy-scene=\"forge\""));
        assert!(merchant.contains("data-forge-customization"));
        assert!(merchant.contains("action=\"/settlements/viabundus-1/weapons/forge\""));
        assert!(merchant.contains("name=\"recipe\""));
        assert!(merchant.contains("data-forge-editor"));
        assert!(merchant.contains("data-forge-eta"));
        assert!(merchant.contains("data-inventory-pane=\"player\""));
        assert!(!merchant.contains("data-inventory-pane=\"party\""));
        assert!(!merchant.contains("Merchant stock"));
        assert!(!merchant.contains("Sell surplus"));
        assert!(
            merchant.contains(
                "data-strategic-tooltip=\"Weight 10.0 / 100.0 kilograms; Penalty -10.0%\""
            )
        );
        assert!(!merchant.contains(">10.0 / 100.0 kg<"));

        let armourer = render(MerchantShop::Armor);
        assert!(armourer.contains("data-inventory-pane=\"party\""));
        assert!(
            armourer.contains(
                "data-strategic-tooltip=\"Weight 30.0 / 200.0 kilograms; Penalty -15.0%\""
            )
        );
        assert!(!armourer.contains(">30.0 / 200.0 kg<"));

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
        let character = CharacterView {
            id: 1,
            name: "Traveller".into(),
            xp: 0,
            level: 1,
            current_settlement_id: Some(town.id.clone()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let ration = CatalogItemView {
            id: "travel_ration".into(),
            weight: 0.65,
            base_value: Some(3),
            nutrition_kcal: 2_500.0,
            kind: CatalogItemKind::Food,
            ..Default::default()
        };

        let markup = live_merchant_shop_page(
            &town,
            &character,
            &[],
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
    fn non_smith_sell_controls_do_not_reserve_a_repair_slot() {
        let rendered = merchant_sell_repair_controls(4, "shirt", 3, 1, 0, true, None).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions\">"));
        assert!(!rendered.contains("smith-player-actions"));
        assert!(rendered.contains("data-merchant-sell"));
    }
}
