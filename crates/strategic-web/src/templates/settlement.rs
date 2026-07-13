//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.

use maud::{Markup, html};

use super::{
    difficulty_stars, empty_state, gold_display, list_item, population_description,
    settlement_layout_with_session, sidebar_section, status_badge,
};
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterEquip, CharacterLimbs, CharacterSkills, InventoryItem,
    Party, Quest, Settlement,
};

/// The currently available merchant storefronts. They share trade mechanics,
/// but each storefront limits the stock shown on its left-hand side.
#[derive(Clone, Copy)]
pub enum MerchantShop {
    General,
    Weapons,
    Armor,
    Clothing,
}

impl MerchantShop {
    pub fn service_id(self) -> &'static str {
        match self {
            Self::General => "merchants",
            Self::Weapons => "weapons",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "Market Square",
            Self::Weapons => "Weaponsmith",
            Self::Armor => "Armourer",
            Self::Clothing => "Tailor",
        }
    }

    fn stocks(self, kind: crate::spacetimedb::ItemKind) -> bool {
        match self {
            Self::General => kind != crate::spacetimedb::ItemKind::Currency,
            Self::Weapons => kind == crate::spacetimedb::ItemKind::Weapon,
            Self::Armor => matches!(
                kind,
                crate::spacetimedb::ItemKind::Armor | crate::spacetimedb::ItemKind::Shield
            ),
            Self::Clothing => kind == crate::spacetimedb::ItemKind::Clothing,
        }
    }
}

/// List all settlements.
pub fn settlements_list_page(
    settlements: &[Settlement],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlements", html! {
                @if settlements.is_empty() {
                    (empty_state("No settlements discovered.", None, None))
                } @else {
                    @for settlement in settlements {
                        (list_item(
                            &format!("/settlements/{}", settlement.id),
                            &settlement.name,
                            Some(population_description(settlement.population_level)),
                        ))
                    }
                }
            }))
        }
        main class="center-content" {
            h2 class="page-title" { "World Map" }
            div class="settlement-grid" {
                @for settlement in settlements {
                    a href=(format!("/settlements/{}", settlement.id)) class="settlement-card" {
                        h3 { (settlement.name) }
                        p class="population" { (population_description(settlement.population_level)) }
                        div class="coords" { "(" (settlement.coord_x as i32) ", " (settlement.coord_y as i32) ")" }
                    }
                }
            }
        }
        aside class="right-sidebar" {
            (sidebar_section("Travel", html! {
                p class="text-muted small-copy" { "Select a settlement to view its services." }
            }))
        }
    };

    super::base_layout_with_session("Settlements", content, logged_in_as, theme)
}

/// Notice board page.
pub fn noticeboard_page(
    settlement: &Settlement,
    quests: &[Quest],
    parties: &[Party],
    active_character: Option<&Character>,
    _inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Posted quests", html! {
                @if quests.is_empty() {
                    p class="text-muted small-copy" { "No notices have been posted." }
                } @else {
                    div class="notice-quest-list" {
                        @for quest in quests {
                            article class="notice-quest" {
                                strong { (quest.title) }
                                div class="notice-quest-meta" {
                                    (difficulty_stars(quest.difficulty))
                                    (gold_display(quest.gold_reward))
                                }
                                p class="small-copy" { (quest.enemy_count) " " (quest.enemy_type) }
                                @if quest.status.to_lowercase().contains("available") {
                                    form action=(format!("/quests/{}/accept", quest.id)) method="post" {
                                        button type="submit" class="btn btn-primary btn-small btn-block" { "Accept" }
                                    }
                                } @else {
                                    (status_badge(&quest.status))
                                }
                            }
                        }
                    }
                }
            }))
            (sidebar_section("Party formation", html! {
                @if parties.is_empty() {
                    p class="text-muted small-copy" { "No parties currently recruiting." }
                } @else {
                    @for party in parties {
                        div class="member-item" {
                            span class="member-name" { (party.name) }
                            form action=(format!("/parties/{}/join", party.id)) method="post" {
                                button type="submit" class="btn btn-secondary btn-small" { "Join" }
                            }
                        }
                    }
                }
                a href="/parties/new" class="btn btn-primary btn-small btn-block mt-1" { "Create party" }
            }))
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, active_character, &settlement.id, None))
            (visual_stage("map", "Settlement map", "TODO: settlement map image"))
            (settlement_chat_area("Notice Board", active_character))
        }
        aside class="right-sidebar" { (quest_detail_rail()) }
    };
    settlement_layout_with_session(
        "Notice Board",
        &settlement.name,
        &settlement.id,
        "noticeboard",
        content,
        logged_in_as,
        theme,
    )
}

/// Market interface. Inventory and prices are intentionally UI-only placeholders
/// until settlement-owned inventory and trade reducers exist.
pub fn merchants_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "merchants",
        "Market Square",
        "Market Steward",
        "Merchant stock and prices will appear here once the trade backend is available.",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
    )
}

/// Smith interface placeholder.
pub fn smith_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "smith",
        "The Smithy",
        "Master Smith",
        "Repair costs and crafting orders require inventory durability and smithing reducers.",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
    )
}

/// Inn interface placeholder.
pub fn inn_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "inn",
        "The Inn",
        "Innkeeper",
        "Rest duration, recovery, training, and strategic time advancement are not connected yet.",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
    )
}

/// Church placeholder.
pub fn religion_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "religion",
        "Church",
        "Priest",
        "Faith, donations, and divine services require the religion and reputation systems.",
        active_character,
        inventory,
        party_members,
        logged_in_as,
        theme,
    )
}

/// Party inventory comparison.
pub fn party_inventory_page(
    settlement: &Settlement,
    selected: &Character,
    selected_inventory: &[InventoryItem],
    active_character: &Character,
    active_inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    selected_equip: Option<&CharacterEquip>,
    active_equip: Option<&CharacterEquip>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_trade_inventory_rail(selected, selected_inventory, items, active_character.id, "right", selected_equip))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &settlement.id, Some(selected.id)))
            (visual_stage("npc", &selected.name, &format!("TODO: {} portrait", selected.name.to_lowercase())))
            (settlement_chat_area(&selected.name, Some(active_character)))
            form id="party-offer" class="party-offer" action=(format!("/settlements/{}/party/{}/inventory/offer", settlement.id, selected.id)) method="post" hidden {
                button type="button" class="party-offer-cancel" data-cancel-trade="party" { "Cancel" }
                button type="submit" disabled { "Offer" }
            }
        }
        aside class="right-sidebar" {
            (party_trade_inventory_rail(active_character, active_inventory, items, selected.id, "left", active_equip))
        }
    };
    settlement_layout_with_session(
        "Party",
        &settlement.name,
        &settlement.id,
        "",
        content,
        Some(&active_character.name),
        theme,
    )
}

/// Active character's combined strategic view.
pub fn party_personal_page(
    settlement: &Settlement,
    active_character: &Character,
    active_inventory: &[InventoryItem],
    party_members: &[Character],
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail("Your attributes", attributes, limbs))
            (party_skills_rail("Your skills", skills, limbs))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &settlement.id, Some(active_character.id)))
            (visual_stage("npc", &active_character.name, &format!("TODO: {} portrait", active_character.name.to_lowercase())))
            (settlement_chat_area(&active_character.name, Some(active_character)))
        }
        aside class="right-sidebar" { (inventory_rail(Some(active_character), active_inventory, None, false)) }
    };
    settlement_layout_with_session(
        "Party",
        &settlement.name,
        &settlement.id,
        "",
        content,
        Some(&active_character.name),
        theme,
    )
}

/// Party stat comparison, with the selected member on the left and the active
/// character on the right.
pub fn party_stats_page(
    settlement: &Settlement,
    selected: &Character,
    active_character: &Character,
    party_members: &[Character],
    selected_attributes: Option<&CharacterAttributes>,
    selected_skills: Option<&CharacterSkills>,
    selected_limbs: Option<&CharacterLimbs>,
    active_attributes: Option<&CharacterAttributes>,
    active_skills: Option<&CharacterSkills>,
    active_limbs: Option<&CharacterLimbs>,
    theme: &str,
) -> Markup {
    let selected_attributes_title = format!("{}'s attributes", selected.name);
    let selected_skills_title = format!("{}'s skills", selected.name);
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail(&selected_attributes_title, selected_attributes, selected_limbs))
            (party_skills_rail(&selected_skills_title, selected_skills, selected_limbs))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &settlement.id, Some(selected.id)))
            (visual_stage("npc", &selected.name, &format!("TODO: {} portrait", selected.name.to_lowercase())))
            (settlement_chat_area(&selected.name, Some(active_character)))
        }
        aside class="right-sidebar" {
            (party_attributes_rail("Your attributes", active_attributes, active_limbs))
            (party_skills_rail("Your skills", active_skills, active_limbs))
        }
    };
    settlement_layout_with_session(
        "Party stats",
        &settlement.name,
        &settlement.id,
        "",
        content,
        Some(&active_character.name),
        theme,
    )
}

fn service_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    npc_name: &str,
    todo: &str,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
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
                    (rest_service_menu("Inn"))
                }
            } @else if service_id == "religion" {
                div class="service-left-stack" {
                    div class="service-inventory-area" {
                        (sidebar_section("Church services", html! {
                            div class="service-placeholder-list" {
                                span { "Sanctuary services" }
                                span class="badge badge-warning" { "TODO" }
                            }
                            p class="text-muted small-copy" { (todo) }
                        }))
                    }
                    (rest_service_menu("Church"))
                }
            } @else if let Some((stock_title, offers)) = trade_offers {
                (merchant_offers_rail(stock_title, offers))
            } @else {
                (sidebar_section("Settlement offerings", html! {
                    div class="service-placeholder-list" {
                        span { "Inventory / offers" }
                        span class="badge badge-warning" { "TODO" }
                    }
                    p class="text-muted small-copy" { (todo) }
                }))
            }
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, active_character, &settlement.id, None))
            (visual_stage("npc", npc_name, &format!("TODO: {} portrait", npc_name.to_lowercase())))
            (settlement_chat_area(title, active_character))
        }
        aside class="right-sidebar" {
            @if trade_offers.is_some() {
                (inventory_rail(
                    active_character,
                    inventory,
                    Some(("Sell", "TODO: selling requires merchant pricing and trade reducers")),
                    matches!(service_id, "weapons" | "armor" | "clothing"),
                ))
            } @else if service_id == "smith" {
                (inventory_rail(
                    active_character,
                    inventory,
                    Some(("Repair", "TODO: repairs require durability, pricing, and smithing reducers")),
                    true,
                ))
            } @else if service_id == "religion" {
                (inventory_rail(active_character, inventory, None, false))
            } @else {
                (sidebar_section("Service", html! {
                    p class="text-muted small-copy" { (todo) }
                }))
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        service_id,
        content,
        logged_in_as,
        theme,
    )
}

fn party_trade_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    recipient_id: u64,
    direction: &str,
    equip: Option<&CharacterEquip>,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                (trade_inventory_table(true, html! {
                    @for item in inventory {
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            tr class=(if direction == "left" { "trade-inventory-row trade-row-player" } else { "trade-inventory-row trade-row-merchant" }) data-item-key=(&item.item_id) {
                                td class="inventory-item-name" {
                                    (&item.item_id)
                                    @if !is_equipped { button type="button" class=(format!("trade-transfer trade-transfer-{direction} party-draft-transfer"))
                                        data-from=(character.id) data-to=(recipient_id) data-item=(item.id) data-key=(&item.item_id) data-count=(item.qty)
                                            aria-label=(format!("Transfer {}", item.item_id))
                                            title="Stage one item for trade" {} }
                                }
                                td class="inventory-count" { (item.qty) }
                                td class="inventory-equipped" { input type="checkbox" checked[is_equipped] disabled; }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                    }
                }))
            }
        }))
    }
}

pub fn live_merchant_shop_page(
    settlement: &Settlement,
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    theme: &str,
    shop: MerchantShop,
) -> Markup {
    let title = shop.title();
    let service_id = shop.service_id();
    let content = html! {
        aside class="left-sidebar" { (sidebar_section("Merchant stock", html! {
            (trade_inventory_table(false, html! {
                @for item in items.iter().filter(|item| shop.stocks(item.kind)) {
                    @let buy_price = (item.base_value.unwrap_or(1) as f32 * 1.375).ceil() as u32;
                    @let sell_price = (item.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32;
                    tr class="trade-inventory-row trade-row-merchant" data-merchant-item=(&item.id) data-merchant-sell-price=(sell_price) { td class="inventory-item-name" { (&item.id) button type="button" class="trade-transfer trade-transfer-right" data-merchant-buy=(&item.id) data-merchant-buy-price=(buy_price) aria-label=(format!("Buy {}", item.id)) title=(format!("Buy {}", item.id)) { "" } } td class="inventory-count" { "999" } td class="inventory-weight" { (weight_display(item.weight)) } td class="inventory-gold" { (buy_price) } }
                }
            }))
        })) }
        main class="center-content settlement-main" { (party_portrait_overlay(party_members, Some(character), &settlement.id, None)) (visual_stage("npc", title, &format!("TODO: {} portrait", title.to_lowercase()))) (settlement_chat_area(title, Some(character))) form # "merchant-offer" class="party-offer" action=(format!("/settlements/{}/merchants/offer", settlement.id)) method="post" hidden { input type="hidden" name="return_to" value=(service_id); button type="button" class="party-offer-cancel" data-cancel-trade="merchant" { "Cancel" } button type="submit" disabled { "Offer" } } }
        aside class="right-sidebar" {
            (sidebar_section(&format!("{}'s inventory", character.name), html! {
                (trade_inventory_table(true, html! {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let sell_price = definition.map_or(0, |definition| (definition.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-merchant-equipped=(is_equipped) {
                        td class="inventory-item-name" { (&item.item_id) @if !is_currency && !is_equipped { button type="button" class="trade-transfer trade-transfer-left" data-merchant-sell=(item.id) data-item-name=(&item.item_id) data-merchant-sell-price=(sell_price) aria-label=(format!("Sell {}", item.item_id)) title=(format!("Sell {}", item.item_id)) {} } }
                        td class="inventory-count" { (item.qty) } td class="inventory-equipped" { input type="checkbox" checked[is_equipped] disabled; } td class="inventory-weight" { (item_weight(definition)) } td class="inventory-gold" { (sell_price) }
                    }}
                }))
            }))
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        service_id,
        content,
        Some(&character.name),
        theme,
    )
}

fn item_weight(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.map_or_else(|| "—".to_owned(), |item| weight_display(item.weight))
}

fn item_value(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.and_then(|item| item.base_value)
        .map_or_else(|| "—".to_owned(), |value| value.to_string())
}

fn weight_display(weight: f32) -> String {
    let display = format!("{weight:.2}");
    display
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn trade_inventory_table(show_equipped: bool, rows: Markup) -> Markup {
    html! {
        table class="trade-inventory-table" {
            @if show_equipped {
                colgroup {
                    col class="inventory-column-item";
                    col class="inventory-column-count";
                    col class="inventory-column-equipped";
                    col class="inventory-column-weight";
                    col class="inventory-column-gold";
                }
            }
            (trade_inventory_table_header(show_equipped))
            tbody { (rows) }
        }
    }
}

fn trade_inventory_table_header(show_equipped: bool) -> Markup {
    html! { thead { tr {
        th scope="col" class="inventory-column-item" { "Item" }
        th scope="col" class="inventory-column-count" { "#" }
        @if show_equipped { th scope="col" class="inventory-column-equipped" title="Equipped" { "✓" } }
        th scope="col" class="inventory-column-weight" title="Weight" { span class="inventory-header-weight" aria-label="Weight" {} }
        th scope="col" class="inventory-column-gold" title="Gold" { span class="inventory-header-coin" aria-label="Gold" {} }
    } } }
}

fn party_skills_rail(
    title: &str,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
) -> Markup {
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let upper_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_arm_health + limbs.right_arm_health) / 2.0
    });
    let lower_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_leg_health + limbs.right_leg_health) / 2.0
    });
    html! {
        (sidebar_section(title, html! {
            @if let Some(skills) = skills {
                table class="party-skills-table" {
                    colgroup {
                        col class="party-skill-icon-column";
                        col class="party-skill-name-column";
                        col class="party-skill-meter-column";
                    }
                    tbody {
                        (party_skill_row("Will", "will", skills.will_hours, 5_000.0, head_health))
                        (party_skill_row("Charisma", "charisma", skills.charisma_hours, 20_000.0, head_health))
                        (party_skill_row("Medicine", "medicine", skills.medicine_hours, 10_000.0, head_health))
                        (party_skill_row("Faith", "faith", skills.faith_hours, 5_000.0, head_health))
                        (party_skill_row("Melee", "melee", skills.melee_hours, 8_000.0, upper_health))
                        (party_skill_row("Ranged", "ranged", skills.ranged_hours, 15_000.0, upper_health))
                        (party_skill_row("Dodge", "dodge", skills.dodge_hours, 20_000.0, lower_health))
                        (party_skill_row("Block", "block", skills.block_hours, 12_000.0, upper_health))
                        (party_skill_row("Stealth", "stealth", skills.stealth_hours, 8_000.0, upper_health))
                        (party_skill_row("Balance", "balance", skills.balance_hours, 30_000.0, lower_health))
                        (party_skill_row("Surgeon", "surgeon", skills.surgeon_hours, 10_000.0, upper_health))
                    }
                }
            } @else {
                p class="text-muted small-copy" { "Skill records have not been created yet." }
            }
        }))
    }
}

fn party_skill_row(name: &str, icon: &str, hours: f32, half_hours: f32, health: f32) -> Markup {
    let rank = 5.0 * hours / (hours + half_hours);
    let effective_rank = rank * health.clamp(0.0, 1.0);
    let current_width = (effective_rank.clamp(0.0, 5.0) / 5.0) * 100.0;
    let damage_width = ((rank - effective_rank).max(0.0) / 5.0) * 100.0;
    html! {
        tr class="party-skill-row" {
            td class="party-skill-icon-cell" { (stat_icon(name, "skills", icon)) }
            td class="party-skill-name" { (name) }
            td class="party-skill-meter" {
                div class="skill-rank-bar" title=(format!("{effective_rank:.1}")) {
                    span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                    span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
                }
            }
        }
    }
}

fn party_attributes_rail(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    limbs: Option<&CharacterLimbs>,
) -> Markup {
    let Some(attributes) = attributes else {
        return html! {};
    };
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let chest_health = limbs.map_or(1.0, |limbs| limbs.chest_health);
    let stomach_health = limbs.map_or(1.0, |limbs| limbs.stomach_health);
    let left_arm_health = limbs.map_or(1.0, |limbs| limbs.left_arm_health);
    let right_arm_health = limbs.map_or(1.0, |limbs| limbs.right_arm_health);
    let left_leg_health = limbs.map_or(1.0, |limbs| limbs.left_leg_health);
    let right_leg_health = limbs.map_or(1.0, |limbs| limbs.right_leg_health);
    html! {
        (sidebar_section(title, html! {
            div class="party-attributes-list" aria-label="Character attributes" {
                (attribute_group("Head", head_health, &[
                    ("Intelligence", "intelligence", attributes.intelligence),
                    ("Instinct", "instinct", attributes.instinct),
                    ("Eyesight", "eyesight", attributes.eyesight),
                    ("Hearing", "hearing", attributes.hearing),
                ]))
                (attribute_group("Chest", chest_health, &[
                    ("Endurance", "endurance", attributes.endurance),
                ]))
                (attribute_group("Stomach", stomach_health, &[
                    ("Immunity", "immunity", attributes.immunity),
                    ("Gut", "gut", attributes.gut),
                ]))
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left arm", "limb-left", left_arm_health, &[
                        ("Strength", "strength-arm", attributes.left_arm_strength),
                        ("Agility", "agility-arm", attributes.left_arm_agility),
                    ]))
                    (limb_attribute_column("Right arm", "limb-right", right_arm_health, &[
                        ("Strength", "strength-arm", attributes.right_arm_strength),
                        ("Agility", "agility-arm", attributes.right_arm_agility),
                    ]))
                }
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left leg", "limb-left", left_leg_health, &[
                        ("Strength", "strength-leg", attributes.left_leg_strength),
                        ("Agility", "agility-leg", attributes.left_leg_agility),
                    ]))
                    (limb_attribute_column("Right leg", "limb-right", right_leg_health, &[
                        ("Strength", "strength-leg", attributes.right_leg_strength),
                        ("Agility", "agility-leg", attributes.right_leg_agility),
                    ]))
                }
            }
        }))
    }
}

fn limb_attribute_column(
    name: &str,
    side: &str,
    health: f32,
    rows: &[(&str, &str, f32)],
) -> Markup {
    attribute_group_with_labels(name, health, rows, false, Some(side))
}

fn attribute_group(name: &str, health: f32, rows: &[(&str, &str, f32)]) -> Markup {
    attribute_group_with_labels(name, health, rows, true, None)
}

fn attribute_group_with_labels(
    name: &str,
    health: f32,
    rows: &[(&str, &str, f32)],
    show_labels: bool,
    side: Option<&str>,
) -> Markup {
    let health = health.clamp(0.0, 1.0);
    let health_width = health * 100.0;
    let damage_width = (1.0 - health) * 100.0;
    html! {
        div class=(match side {
            Some(side) => format!("attribute-group limb-attribute-column {side}"),
            None => "attribute-group".to_owned(),
        }) {
            div class="attribute-group-heading" { (name) }
            div class="attribute-health-bar" title=(format!("{name} health: {health_width:.0}%")) {
                span class="attribute-health-current" style=(format!("width:{health_width:.1}%")) {}
                span class="attribute-health-damage" style=(format!("left:{health_width:.1}%;width:{damage_width:.1}%")) {}
            }
            @for (attribute_name, icon, value) in rows {
                (attribute_row(attribute_name, icon, *value, health, show_labels))
            }
        }
    }
}

fn attribute_row(name: &str, icon: &str, value: f32, health: f32, show_label: bool) -> Markup {
    let effective_value = value * health.clamp(0.0, 1.0);
    let current_width = (effective_value.clamp(0.0, 5.0) / 5.0) * 100.0;
    let damage_width = ((value - effective_value).max(0.0) / 5.0) * 100.0;
    html! {
        div class=(if show_label { "party-attribute-row" } else { "party-attribute-row party-attribute-icon-only" }) {
            (stat_icon(name, "attributes", icon))
            @if show_label { span class="party-attribute-name" { (name) } }
            div class="attribute-rank-bar" title=(format!("{effective_value:.1}")) {
                span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
            }
        }
    }
}

fn stat_icon(label: &str, category: &str, icon: &str) -> Markup {
    html! {
        span
            class=(format!("stat-icon stat-icon-{icon}"))
            style=(format!("--stat-icon: url('/static/icons/stats/{category}/{icon}.png')"))
            role="img"
            aria-label=(label)
            title=(label)
        {}
    }
}

fn visual_stage(kind: &str, title: &str, placeholder: &str) -> Markup {
    html! {
        figure class=(format!("service-visual service-visual-{}", kind)) {
            div class="service-visual-placeholder" role="img" aria-label=(placeholder) {
                @if kind == "map" {
                    span class="visual-symbol" { "⌖" }
                    span class="visual-label" { "Map placeholder" }
                } @else {
                    span class="visual-symbol" { (title.chars().next().unwrap_or('?')) }
                    span class="visual-label" { "Portrait placeholder" }
                }
            }
        }
    }
}

fn party_portrait_overlay(
    party_members: &[Character],
    active_character: Option<&Character>,
    settlement_id: &str,
    selected_character_id: Option<u64>,
) -> Markup {
    let members: Vec<&Character> = if party_members.is_empty() {
        active_character.into_iter().collect()
    } else {
        party_members.iter().collect()
    };

    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label="Active party" {
                @for member in members {
                    @let is_active = active_character.is_some_and(|character| character.id == member.id);
                    @if is_active {
                    a href=(format!("/settlements/{}/party/{}", settlement_id, member.id))
                        class=(if selected_character_id == Some(member.id) { "party-portrait active" } else { "party-portrait" })
                        title=(format!("Open {}'s inventory and stats", member.name)) {
                        (incapacitation_wheel_placeholder())
                        span class="party-portrait-initial" {
                            span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                            span class="party-portrait-name" { (&member.name) }
                        }
                    }
                    } @else {
                    div class=(if selected_character_id == Some(member.id) { "party-portrait active" } else { "party-portrait" })
                        title=(&member.name) {
                        (incapacitation_wheel_placeholder())
                        span class="party-portrait-initial" {
                            span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                            span class="party-portrait-name" { (&member.name) }
                        }
                        span class="party-portrait-actions" aria-label=(format!("Actions for {}", member.name)) {
                            a href=(format!("/settlements/{}/party/{}/stats", settlement_id, member.id))
                                class="party-portrait-action" title=(format!("Compare stats with {}", member.name)) {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/character/stats-sheet.png')"
                                    role="img" aria-label="Stats" {}
                            }
                            a href=(format!("/settlements/{}/party/{}/inventory", settlement_id, member.id))
                                class="party-portrait-action" title=(format!("Compare inventory with {}", member.name)) {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/character/inventory.png')"
                                    role="img" aria-label="Inventory" {}
                            }
                        }
                    }
                    }
                }
            }
        }
    }
}

/// Temporary presentation data until combat/incapacitation state is available to the strategic UI.
fn incapacitation_wheel_placeholder() -> Markup {
    html! {
        span class="incapacitation-wheel"
            role="img"
            aria-label="Incapacitation placeholder: 12% imbalance, 10% exhaustion, 8% pain, 14% blood loss, 9% fear, 11% fatigue"
            title="Placeholder incapacitation: imbalance 12%, exhaustion 10%, pain 8%, blood loss 14%, fear 9%, fatigue 11%" {}
    }
}

/// Layout-only chat panel matching the UX prototype. Channels, message history,
/// and sending are deliberately disabled until the strategic chat backend exists.
fn settlement_chat_area(location: &str, active_character: Option<&Character>) -> Markup {
    let party_label = active_character
        .map(|character| format!("{}'s party", character.name))
        .unwrap_or_else(|| "Party".to_string());

    html! {
        section class="settlement-chat" aria-label="Settlement chat" {
            div class="settlement-chat-tabs" role="tablist" aria-label="Chat channels" {
                button type="button" class="settlement-chat-tab active" disabled
                    title="TODO: party chat requires real-time message delivery" {
                    (party_label)
                }
                button type="button" class="settlement-chat-tab" disabled
                    title="TODO: settlement chat requires real-time message delivery" {
                    "Settlement"
                }
                button type="button" class="settlement-chat-tab" disabled
                    title="TODO: guild chat requires guild membership and real-time message delivery" {
                    "Guild"
                }
            }
            div class="settlement-chat-messages" aria-live="polite" {
                div class="chat-system-message" {
                    span class="chat-timestamp" { "[--:--]" }
                    " Chat is not connected yet. Message history and live delivery are TODO."
                }
                div class="chat-npc-message" {
                    span class="chat-timestamp" { "[--:--]" }
                    (location) " chat will appear here when the strategic chat service is available."
                }
            }
            div class="settlement-chat-composer" {
                input type="text" disabled placeholder="Chat is not implemented yet"
                    title="TODO: sending messages requires the strategic chat backend";
                button type="button" class="btn btn-primary btn-icon" disabled
                    title="TODO: sending messages requires the strategic chat backend" aria-label="Send message" {
                    "➤"
                }
            }
        }
    }
}

fn merchant_offers_rail(title: &str, placeholder_offers: &[&str]) -> Markup {
    html! {
        (sidebar_section(title, html! {
            p class="text-muted small-copy" { "TODO: merchant inventory and prices are not available yet." }
            table class="trade-inventory-table" {
                (trade_inventory_table_header(false))
                tbody {
                @for offer in placeholder_offers {
                    tr class="trade-inventory-row trade-row-merchant"
                        title="TODO: buying requires merchant inventory, pricing, and trade reducers" {
                        td class="inventory-item-name" {
                            (offer)
                            button type="button" class="trade-transfer trade-transfer-right" disabled
                                aria-label=(format!("Buy {}", offer))
                                title="TODO: buying requires merchant inventory, pricing, and trade reducers" { "▶" }
                        }
                        td class="inventory-count" { "1" }
                        td class="inventory-weight" { "—" }
                        td class="inventory-gold" { "—" }
                    }
                }
                }
            }
        }))
    }
}

fn inventory_rail(
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    trade_action: Option<(&str, &str)>,
    show_repair: bool,
) -> Markup {
    let title = active_character
        .map(|character| format!("{}'s inventory", character.name))
        .unwrap_or_else(|| "Your inventory".to_string());

    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                table class="trade-inventory-table" {
                    (trade_inventory_table_header(false))
                    tbody {
                    @for item in inventory {
                        tr class=(if trade_action.is_some() { "trade-inventory-row" } else { "trade-inventory-row inventory-row-readonly" }) {
                            td class="inventory-item-name" {
                                (&item.item_id)
                                @if let Some((action, tooltip)) = trade_action {
                                button type="button" class="trade-transfer trade-transfer-left" disabled
                                    aria-label=(format!("{} {}", action, item.item_id))
                                    title=(tooltip) { "◀" }
                                }
                                @if show_repair {
                                span class="repair-placeholder"
                                    style="--repair-icon: url('/static/icons/character/repair.png')"
                                    role="img"
                                    aria-label=(format!("Repair {}", item.item_id))
                                    title="TODO: repairs require durability, pricing, and repair reducers" {}
                                }
                            }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-weight" { "—" }
                            td class="inventory-gold" { "—" }
                        }
                    }
                }
                }
            }
        }))
    }
}

fn rest_service_menu(location: &str) -> Markup {
    html! {
        section class="rest-service-menu" aria-label=(format!("{} rest service", location)) {
            div class="rest-service-heading" {
                strong { "Rest" }
                span class="badge badge-warning" { "TODO" }
            }
            p class="rest-service-copy" { "Choose how many days to rest. Recovery and time advancement are not implemented yet." }
            div class="rest-days-control" {
                button type="button" class="rest-days-step" disabled aria-label="Decrease rest days"
                    title="TODO: resting requires strategic downtime support" { "−" }
                input type="number" value="0" min="0" disabled aria-label="Rest days"
                    title="Default: the number of days needed to fully heal once strategic recovery is implemented";
                span class="rest-days-unit" { "days" }
                button type="button" class="rest-days-step" disabled aria-label="Increase rest days"
                    title="TODO: resting requires strategic downtime support" { "+" }
            }
            button type="button" class="btn btn-primary btn-small btn-block" disabled
                title="TODO: resting requires strategic downtime support" { "Rest" }
        }
    }
}

fn quest_detail_rail() -> Markup {
    html! {
        (sidebar_section("Quest details", html! {
            div class="context-placeholder" {
                p { "Select a quest to inspect its full details." }
                p class="text-muted small-copy" { "TODO: quest selection and detail rendering are not connected yet." }
            }
        }))
    }
}
