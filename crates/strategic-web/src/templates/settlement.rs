//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.

use maud::{html, Markup};

use super::{
    difficulty_stars, empty_state, gold_display, list_item, panel, population_description,
    settlement_layout_with_session, sidebar_section, status_badge, xp_display,
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
            (party_rail(None, &[]))
        }
    };

    super::base_layout_with_session("Settlements", content, logged_in_as, theme)
}

/// Settlement overview.
pub fn settlement_detail_page(
    settlement: &Settlement,
    quests: &[Quest],
    parties: &[Party],
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (settlement_rail(settlement, "Town Crier", "A guide to local services and opportunities."))
        }
        main class="center-content settlement-main" {
            h2 class="page-title" { (settlement.name) }
            (service_context("Welcome to town", "The settlement is your base for trade, rest, recruitment, and new work."))
            (panel("Available Quests", html! {
                @if quests.is_empty() {
                    p class="text-muted" { "No notices are posted today." }
                } @else {
                    @for quest in quests.iter().take(3) {
                        a href=(format!("/quests/{}", quest.id)) class="quest-card" {
                            div class="quest-card-header" {
                                span class="quest-card-title" { (quest.title) }
                                (status_badge(&quest.status))
                            }
                            div class="quest-card-meta" {
                                (difficulty_stars(quest.difficulty))
                                div class="quest-card-reward" {
                                    (gold_display(quest.gold_reward))
                                    (xp_display(quest.xp_reward))
                                }
                            }
                        }
                    }
                    @if quests.len() > 3 {
                        a href=(format!("/settlements/{}/noticeboard", settlement.id)) class="btn btn-secondary btn-small mt-1" { "View notice board" }
                    }
                }
            }))
        }
        aside class="right-sidebar" {
            (party_rail(active_character, inventory))
            (sidebar_section("Parties in town", html! {
                @if parties.is_empty() {
                    p class="text-muted small-copy" { "No other parties are currently here." }
                } @else {
                    @for party in parties {
                        (list_item(&format!("/parties/{}", party.id), &party.name, party.active_quest_id.as_deref()))
                    }
                }
            }))
        }
    };

    settlement_layout_with_session(&settlement.name, &settlement.name, &settlement.id, "", content, logged_in_as, theme)
}

/// Notice board page.
pub fn noticeboard_page(
    settlement: &Settlement,
    quests: &[Quest],
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" { (settlement_rail(settlement, "Notice Board", "Notices posted by local patrons.")) }
        main class="center-content settlement-main" {
            h2 class="page-title" { "Notice Board" }
            (service_context("Posted work", "Review local opportunities and accept a quest as your party leader."))
            @if quests.is_empty() {
                (empty_state("No quests posted on the notice board.", None, None))
            } @else {
                @for quest in quests {
                    (panel(&quest.title, html! {
                        div class="flex justify-between items-center mb-1" {
                            (status_badge(&quest.status))
                            (difficulty_stars(quest.difficulty))
                        }
                        p class="small-copy" { (quest.description) }
                        div class="quest-card-meta mt-1" {
                            span class="quest-card-enemy" { "Target: " (quest.enemy_count) " " (quest.enemy_type) }
                            div class="quest-card-reward" { (gold_display(quest.gold_reward)) (xp_display(quest.xp_reward)) }
                        }
                        @if quest.status.to_lowercase().contains("available") {
                            form action=(format!("/quests/{}/accept", quest.id)) method="post" class="mt-1" {
                                button type="submit" class="btn btn-primary btn-small" { "Accept quest" }
                            }
                        }
                    }))
                }
            }
        }
        aside class="right-sidebar" { (party_rail(active_character, inventory)) }
    };
    settlement_layout_with_session("Notice Board", &settlement.name, &settlement.id, "noticeboard", content, logged_in_as, theme)
}

/// Tavern page.
pub fn tavern_page(
    settlement: &Settlement,
    parties: &[Party],
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" { (settlement_rail(settlement, "Innkeeper", "Adventurers, rumours, and open tables.")) }
        main class="center-content settlement-main" {
            h2 class="page-title" { "The Tavern" }
            (service_context("A place to gather", "Meet other adventurers, form a party, and prepare for your next expedition."))
            (panel("Parties looking for members", html! {
                @if parties.is_empty() {
                    p class="text-muted" { "No parties currently recruiting." }
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
            }))
            (panel("Start a party", html! {
                p class="small-copy" { "Gather allies before accepting a job from the notice board." }
                a href="/parties/new" class="btn btn-primary mt-1" { "Create party" }
            }))
        }
        aside class="right-sidebar" { (party_rail(active_character, inventory)) }
    };
    settlement_layout_with_session("Tavern", &settlement.name, &settlement.id, "tavern", content, logged_in_as, theme)
}

/// Market interface. Inventory and prices are intentionally UI-only placeholders
/// until settlement-owned inventory and trade reducers exist.
pub fn merchants_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement, "merchants", "Market Square", "Market Steward",
        "Browse settlement goods and compare them against your party's packs.",
        "Merchant stock and prices will appear here once the trade backend is available.",
        active_character, inventory, logged_in_as, theme,
    )
}

/// Smith interface placeholder.
pub fn smith_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement, "smith", "The Smithy", "Master Smith",
        "Inspect equipment on the right, then commission repairs or replacements here.",
        "Repair costs and crafting orders require inventory durability and smithing reducers.",
        active_character, inventory, logged_in_as, theme,
    )
}

/// Inn interface placeholder.
pub fn inn_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement, "inn", "The Inn", "Innkeeper",
        "Choose accommodation and manage downtime for your party.",
        "Rest duration, recovery, training, and strategic time advancement are not connected yet.",
        active_character, inventory, logged_in_as, theme,
    )
}

fn service_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    npc_name: &str,
    introduction: &str,
    todo: &str,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (settlement_rail(settlement, npc_name, introduction))
            (sidebar_section("Settlement offerings", html! {
                div class="service-placeholder-list" {
                    span { "Inventory / offers" }
                    span class="badge badge-warning" { "TODO" }
                }
                p class="text-muted small-copy" { (todo) }
            }))
        }
        main class="center-content settlement-main" {
            h2 class="page-title" { (title) }
            (service_context(npc_name, introduction))
            (panel("Service", html! {
                p { (todo) }
                button class="btn btn-secondary mt-1" disabled title="TODO: requires strategic service reducer" { "Unavailable" }
            }))
        }
        aside class="right-sidebar" { (party_rail(active_character, inventory)) }
    };
    settlement_layout_with_session(title, &settlement.name, &settlement.id, service_id, content, logged_in_as, theme)
}

fn settlement_rail(settlement: &Settlement, npc_name: &str, description: &str) -> Markup {
    html! {
        (sidebar_section("Settlement", html! {
            div class="settlement-summary" {
                strong { (settlement.name) }
                span { (population_description(settlement.population_level)) }
            }
        }))
        (sidebar_section("Service host", html! {
            div class="npc-card" {
                div class="npc-monogram" { (npc_name.chars().next().unwrap_or('?')) }
                div { strong { (npc_name) } p class="small-copy" { (description) } }
            }
        }))
    }
}

fn service_context(title: &str, copy: &str) -> Markup {
    html! {
        section class="service-context" {
            strong class="service-context-title" { (title) }
            p class="service-context-copy" { (copy) }
        }
    }
}

fn party_rail(active_character: Option<&Character>, inventory: &[InventoryItem]) -> Markup {
    html! {
        (sidebar_section("Your party", html! {
            @if let Some(character) = active_character {
                div class="party-member-card" {
                    div class="party-monogram" { (character.name.chars().next().unwrap_or('?')) }
                    div { strong { (&character.name) } span { "Active adventurer · level " (character.level) } }
                }
                a href=(format!("/characters/{}", character.id)) class="btn btn-secondary btn-small btn-block mt-1" { "View character sheet" }
                div class="party-inventory" {
                    span class="party-inventory-title" { "Inventory" }
                    @if inventory.is_empty() {
                        p class="text-muted small-copy" { "No items carried." }
                    } @else {
                        @for item in inventory.iter().take(5) {
                            div class="inventory-item" {
                                span class="item-name" { (&item.item_id) }
                                span class="item-qty" { "×" (item.qty) }
                            }
                        }
                    }
                }
                p class="text-muted small-copy mt-1" { "TODO: party members, shared packs, condition, and equipment slots." }
            } @else {
                p class="text-muted small-copy" { "Select a character to view party-owned information." }
                a href="/characters" class="btn btn-secondary btn-small btn-block mt-1" { "Choose character" }
            }
        }))
    }
}
