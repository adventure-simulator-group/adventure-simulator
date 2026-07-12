//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.

use maud::{html, Markup};

use super::{
    difficulty_stars, empty_state, gold_display, list_item, population_description,
    settlement_layout_with_session, sidebar_section, status_badge,
};
use crate::spacetimedb::{Character, InventoryItem, Party, Quest, Settlement};

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
            (party_portrait_overlay(party_members, active_character))
            (visual_stage("map", "Settlement map", "TODO: settlement map image"))
            (settlement_chat_area("Notice Board", active_character))
        }
        aside class="right-sidebar" { (quest_detail_rail()) }
    };
    settlement_layout_with_session("Notice Board", &settlement.name, &settlement.id, "noticeboard", content, logged_in_as, theme)
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
        settlement, "merchants", "Market Square", "Market Steward",
        "Merchant stock and prices will appear here once the trade backend is available.",
        active_character, inventory, party_members, logged_in_as, theme,
    )
}

/// Weapons shop placeholder.
pub fn weapons_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement, "weapons", "Weaponsmith", "Weaponsmith",
        "Weapon stock, prices, and purchases require settlement inventories and trade reducers.",
        active_character, inventory, party_members, logged_in_as, theme,
    )
}

/// Armour shop placeholder.
pub fn armor_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement, "armor", "Armourer", "Armourer",
        "Armour stock, prices, and purchases require settlement inventories and trade reducers.",
        active_character, inventory, party_members, logged_in_as, theme,
    )
}

/// Clothing shop placeholder.
pub fn clothing_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement, "clothing", "Tailor", "Tailor",
        "Clothing stock, prices, and purchases require settlement inventories and trade reducers.",
        active_character, inventory, party_members, logged_in_as, theme,
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
        settlement, "smith", "The Smithy", "Master Smith",
        "Repair costs and crafting orders require inventory durability and smithing reducers.",
        active_character, inventory, party_members, logged_in_as, theme,
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
        settlement, "inn", "The Inn", "Innkeeper",
        "Rest duration, recovery, training, and strategic time advancement are not connected yet.",
        active_character, inventory, party_members, logged_in_as, theme,
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
        settlement, "religion", "Church", "Priest",
        "Faith, donations, and divine services require the religion and reputation systems.",
        active_character, inventory, party_members, logged_in_as, theme,
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
        "merchants" => Some(("Merchant stock", &["Weapon offer", "Armour offer", "Provision offer"])),
        "weapons" => Some(("Weapons", &["Weapon offer", "Shield offer", "Ammunition offer"])),
        "armor" => Some(("Armour", &["Head protection", "Torso protection", "Limb protection"])),
        "clothing" => Some(("Clothing", &["Travel attire", "Cold-weather clothing", "Fine clothing"])),
        "inn" => Some(("Inn supplies", &["Rations", "Water", "Supplies", "Bed for the night"])),
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
            (party_portrait_overlay(party_members, active_character))
            (visual_stage("npc", npc_name, &format!("TODO: {} portrait", npc_name.to_lowercase())))
            (settlement_chat_area(title, active_character))
        }
        aside class="right-sidebar" {
            @if trade_offers.is_some() {
                (inventory_rail(
                    active_character,
                    inventory,
                    Some(("Sell", "TODO: selling requires merchant pricing and trade reducers")),
                ))
            } @else if service_id == "smith" {
                (inventory_rail(
                    active_character,
                    inventory,
                    Some(("Repair", "TODO: repairs require durability, pricing, and smithing reducers")),
                ))
            } @else if service_id == "religion" {
                (inventory_rail(active_character, inventory, None))
            } @else {
                (sidebar_section("Service", html! {
                    p class="text-muted small-copy" { (todo) }
                }))
            }
        }
    };
    settlement_layout_with_session(title, &settlement.name, &settlement.id, service_id, content, logged_in_as, theme)
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

fn party_portrait_overlay(party_members: &[Character], active_character: Option<&Character>) -> Markup {
    let members: Vec<&Character> = if party_members.is_empty() {
        active_character.into_iter().collect()
    } else {
        party_members.iter().collect()
    };

    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label="Active party" {
                @for member in members {
                    a href=(format!("/characters/{}", member.id)) class="party-portrait" title=(&member.name) {
                        span class="party-portrait-initial" {
                            span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                        }
                        span class="party-portrait-name" { (&member.name) }
                    }
                }
            }
        }
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
                (inventory_table_header())
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
                    (inventory_table_header())
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

fn inventory_table_header() -> Markup {
    html! {
        thead {
            tr {
                th scope="col" class="inventory-column-item" { "Item" }
                th scope="col" class="inventory-column-count" title="Count" { "#" }
                th scope="col" class="inventory-column-weight" title="Weight" {
                    span class="inventory-header-weight" aria-label="Weight" {}
                }
                th scope="col" class="inventory-column-gold" title="Gold" {
                    span class="inventory-header-coin" aria-label="Gold" {}
                }
            }
        }
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
