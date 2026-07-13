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
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterSkills, InventoryItem, Party, Quest, Settlement,
};

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

/// A party portrait is a settlement-local tab. Selecting the active character
/// reveals their skills; selecting another member opens a party trade view.
pub fn party_member_page(
    settlement: &Settlement,
    selected: &Character,
    selected_inventory: &[InventoryItem],
    active_character: &Character,
    active_inventory: &[InventoryItem],
    party_members: &[Character],
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    theme: &str,
) -> Markup {
    let is_self = selected.id == active_character.id;
    let content = html! {
        aside class="left-sidebar" {
            @if is_self {
                (party_skills_rail(skills))
            } @else {
                (party_trade_inventory_rail(selected, selected_inventory))
            }
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &settlement.id, Some(selected.id)))
            (visual_stage("npc", &selected.name, &format!("TODO: {} portrait", selected.name.to_lowercase())))
            @if is_self { (attribute_overlay(attributes)) }
            (settlement_chat_area(&selected.name, Some(active_character)))
        }
        aside class="right-sidebar" {
            (inventory_rail(
                Some(active_character),
                active_inventory,
                if is_self { None } else { Some(("Trade", "TODO: party trading requires inventory transfer reducers")) },
            ))
        }
    };
    settlement_layout_with_session("Party", &settlement.name, &settlement.id, "", content, Some(&active_character.name), theme)
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

fn party_trade_inventory_rail(character: &Character, inventory: &[InventoryItem]) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                table class="trade-inventory-table" {
                    (inventory_table_header())
                    tbody {
                        @for item in inventory {
                            tr class="trade-inventory-row trade-row-merchant" {
                                td class="inventory-item-name" {
                                    (&item.item_id)
                                    button type="button" class="trade-transfer trade-transfer-right" disabled
                                        aria-label=(format!("Trade {}", item.item_id))
                                        title="TODO: party trading requires inventory transfer reducers" { "▶" }
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

fn party_skills_rail(skills: Option<&CharacterSkills>) -> Markup {
    html! {
        (sidebar_section("Your skills", html! {
            @if let Some(skills) = skills {
                div class="party-skills-list" {
                    (party_skill_row("Will", "will", skills.will_hours, 5_000.0))
                    (party_skill_row("Charisma", "charisma", skills.charisma_hours, 20_000.0))
                    (party_skill_row("Medicine", "medicine", skills.medicine_hours, 10_000.0))
                    (party_skill_row("Faith", "faith", skills.faith_hours, 5_000.0))
                    (party_skill_row("Melee", "melee", skills.melee_hours, 8_000.0))
                    (party_skill_row("Ranged", "ranged", skills.ranged_hours, 15_000.0))
                    (party_skill_row("Dodge", "dodge", skills.dodge_hours, 20_000.0))
                    (party_skill_row("Block", "block", skills.block_hours, 12_000.0))
                    (party_skill_row("Stealth", "stealth", skills.stealth_hours, 8_000.0))
                    (party_skill_row("Balance", "balance", skills.balance_hours, 30_000.0))
                    (party_skill_row("Surgeon", "surgeon", skills.surgeon_hours, 10_000.0))
                }
            } @else {
                p class="text-muted small-copy" { "Skill records have not been created yet." }
            }
        }))
    }
}

fn party_skill_row(name: &str, icon: &str, hours: f32, half_hours: f32) -> Markup {
    let rank = 5.0 * hours / (hours + half_hours);
    html! { div class="party-skill-row" { (stat_icon(name, "skills", icon)) strong title=(format!("{hours:.0} hours trained")) { (format!("{rank:.0}")) } } }
}

fn attribute_overlay(attributes: Option<&CharacterAttributes>) -> Markup {
    let Some(attributes) = attributes else { return html! {}; };
    html! {
        div class="character-attribute-overlay" aria-label="Character attributes" {
            (attribute_island("attribute-head", "Head", &[
                ("Intelligence", "intelligence", attributes.intelligence),
                ("Instinct", "instinct", attributes.instinct),
                ("Eyesight", "eyesight", attributes.eyesight),
                ("Hearing", "hearing", attributes.hearing),
            ]))
            (attribute_island("attribute-chest", "Chest", &[
                ("Endurance", "endurance", attributes.endurance),
            ]))
            (attribute_island("attribute-stomach", "Stomach", &[
                ("Immunity", "immunity", attributes.immunity),
                ("Gut", "gut", attributes.gut),
            ]))
            (attribute_island("attribute-left-arm", "Left Arm", &[
                ("Strength", "strength-arm", attributes.left_arm_strength),
                ("Agility", "agility-arm", attributes.left_arm_agility),
            ]))
            (attribute_island("attribute-right-arm", "Right Arm", &[
                ("Strength", "strength-arm", attributes.right_arm_strength),
                ("Agility", "agility-arm", attributes.right_arm_agility),
            ]))
            (attribute_island("attribute-left-leg", "Left Leg", &[
                ("Strength", "strength-leg", attributes.left_leg_strength),
                ("Agility", "agility-leg", attributes.left_leg_agility),
            ]))
            (attribute_island("attribute-right-leg", "Right Leg", &[
                ("Strength", "strength-leg", attributes.right_leg_strength),
                ("Agility", "agility-leg", attributes.right_leg_agility),
            ]))
        }
    }
}

fn attribute_island(position: &str, title: &str, rows: &[(&str, &str, f32)]) -> Markup {
    html! {
        section class=(format!("attribute-callout {position}")) {
            h3 { (title) }
            hr;
            @for (name, icon, value) in rows {
                div class="attribute-island-row" {
                    (stat_icon(name, "attributes", icon))
                    strong { (format!("{value:.0}")) }
                }
            }
        }
    }
}

fn stat_icon(label: &str, category: &str, icon: &str) -> Markup {
    html! {
        span
            class="stat-icon"
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
                    a href=(format!("/settlements/{}/party/{}", settlement_id, member.id))
                        class=(if selected_character_id == Some(member.id) { "party-portrait active" } else { "party-portrait" })
                        title=(&member.name) {
                        (incapacitation_wheel_placeholder())
                        span class="party-portrait-initial" {
                            span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                            span class="party-portrait-name" { (&member.name) }
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
