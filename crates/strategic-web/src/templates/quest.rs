//! Quest templates

use maud::{Markup, html};

use super::{
    base_layout_with_session, difficulty_stars, divider, empty_state, gold_display, list_item,
    panel, sidebar_section, status_badge,
};
use crate::routes::settlements::TravelDestination;
use crate::spacetimedb::{BattleLootItem, ItemDefinition, PartyInventoryItem, Quest};
use crate::{
    spacetimedb::Character,
    templates::settlement::{
        map_destination_detail, map_destination_list, party_portrait_overlay, settlement_chat_area,
        visual_stage,
    },
};

/// List all quests
pub fn quests_list_page(quests: &[Quest], logged_in_as: Option<&str>, theme: &str) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Quests", html! {
                @if quests.is_empty() {
                    (empty_state("No quests available.", None, None))
                } @else {
                    div # "quest-list" {
                        @for quest in quests {
                            (list_item(
                                &format!("/quests/{}", quest.id),
                                &quest.title,
                                Some(&quest.settlement_id),
                            ))
                        }
                    }
                }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "All Quests" }

            div class="filter-tabs" {
                button class="filter-tab active" { "All" }
                button class="filter-tab" { "Available" }
                button class="filter-tab" { "Accepted" }
                button class="filter-tab" { "Completed" }
            }

            @if quests.is_empty() {
                (empty_state("No quests available.", None, None))
            } @else {
                @for quest in quests {
                    a href=(format!("/quests/{}", quest.id)) class="quest-card" {
                        div class="quest-card-header" {
                            span class="quest-card-title" { (quest.title) }
                            (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
                        }
                        p class="quest-card-desc" { (quest.description) }
                        div class="quest-card-meta" {
                            div {
                                span style="font-size:var(--font-size-xs);color:var(--text-muted)" {
                                    (quest.settlement_id)
                                }
                                " "
                                (difficulty_stars(quest.difficulty))
                            }
                            div class="quest-card-reward" {
                                (gold_display(quest.gold_reward))
                            }
                        }
                    }
                }
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Info", html! {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" {
                        "Select a quest to view details. "
                        "You must be a party leader at the quest's settlement to accept it."
                    }
                }))
            }))
        }
    };

    base_layout_with_session("Quests", content, logged_in_as, theme)
}

/// Quest detail page
pub fn quest_detail_page(
    quest: &Quest,
    is_party_quest: bool,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Quest", html! {
                h4 style="font-family:var(--font-display);margin-bottom:0.25rem" { (quest.title) }
                (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
            }))

            (divider())

            (sidebar_section("Description", html! {
                p style="font-size:var(--font-size-sm)" { (quest.description) }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { (quest.title) }

            (panel("Details", html! {
                div class="flex flex-col gap-sm" {
                    div class="detail-row" {
                        span class="detail-label" { "Location" }
                        a href=(format!("/settlements/{}", quest.settlement_id)) class="detail-value text-accent" {
                            (quest.settlement_id)
                        }
                    }
                    div class="detail-row" {
                        span class="detail-label" { "Difficulty" }
                        span class="detail-value" { (difficulty_stars(quest.difficulty)) }
                    }
                    div class="detail-row" {
                        span class="detail-label" { "Target" }
                        span class="detail-value" { (quest.enemy_count) " " (quest.enemy_type) }
                    }
                }
            }))

            (panel("Rewards", html! {
                div class="stat-grid" {
                    div class="stat-item" {
                        span class="stat-label" { "Gold" }
                        span class="stat-value" { (gold_display(quest.gold_reward)) }
                    }
                }
            }))
        }

        aside class="right-sidebar" {
            (sidebar_section("Status", html! {
                (panel("", html! {
                    @match quest.status.to_lowercase().as_str() {
                        "available" => {
                            p style="font-size:var(--font-size-sm)" { "This quest is available." }
                            p class="text-muted" style="font-size:var(--font-size-xs);margin-top:0.5rem" {
                                "Speak with the local NPC who needs the work done to add it to your tracker."
                            }
                        }
                        "accepted" => {
                            p style="font-size:var(--font-size-sm)" { "Quest accepted." }
                            @if let Some(party_id) = &quest.accepted_by {
                                p class="text-muted" style="font-size:var(--font-size-xs)" {
                                    "Party: " (party_id)
                                }
                            }
                            @if is_party_quest {
                                form action=(format!("/quests/{}/abandon", quest.id)) method="post" class="mt-1" {
                                    button type="submit" class="btn btn-danger btn-block" { "Abandon Quest" }
                                }
                            }
                        }
                        "completed" => {
                            p class="text-success" style="font-size:var(--font-size-sm);font-weight:600" {
                                "Quest completed!"
                            }
                        }
                        _ => {
                            p style="font-size:var(--font-size-sm)" { "Status: " (quest.status) }
                        }
                    }
                }))
            }))
        }
    };

    base_layout_with_session(&quest.title, content, logged_in_as, theme)
}

/// Quest list fragment for Datastar updates
pub fn quests_list_fragment(quests: &[Quest]) -> Markup {
    html! {
        div # "quest-list" {
            @for quest in quests {
                (list_item(
                    &format!("/quests/{}", quest.id),
                    &quest.title,
                    Some(&quest.settlement_id),
                ))
            }
        }
    }
}

pub fn quest_location_base_page(
    quest: &Quest,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Location", html! { p { (&quest.location_description) } }))
        }
        (quest_location_center(quest, active_character, party_members, can_fight, resolved))
        aside class="right-sidebar" aria-label="Location details" {}
    };
    super::quest_location_layout_with_session(
        &quest.title,
        &quest.title,
        &quest.id,
        "",
        content,
        logged_in_as,
        theme,
    )
}

pub fn quest_location_map_page(
    quest: &Quest,
    nearby: &[TravelDestination],
    selected_id: Option<&str>,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_travel: bool,
    can_fight: bool,
    resolved: bool,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let selected = selected_id.and_then(|id| nearby.iter().find(|entry| entry.id == id));
    let content = html! {
        (map_destination_list(
            nearby,
            selected_id,
            &format!("/locations/quest/{}/map", quest.id),
        ))
        (quest_location_center(quest, active_character, party_members, can_fight, resolved))
        (map_destination_detail(selected, can_travel))
    };
    super::quest_location_layout_with_session(
        &format!("{} map", quest.title),
        &quest.title,
        &quest.id,
        "map",
        content,
        logged_in_as,
        theme,
    )
}

fn quest_location_center(
    quest: &Quest,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
) -> Markup {
    html! {
        main class="center-content settlement-main quest-location-main" {
            (party_portrait_overlay(
                party_members,
                active_character,
                &format!("/locations/quest/{}", quest.id),
                None,
            ))
            div class="quest-visual-wrap" {
                (visual_stage("map", &quest.title, "TODO: quest location image"))
                div class="quest-combat-actions" aria-label="Quest actions" {
                    @if can_fight {
                        form action="/missions/enter" method="post" {
                            button type="submit" class="btn btn-danger" { "Initiate Combat" }
                        }
                        form action=(format!("/quests/{}/autoresolve", quest.id)) method="post" {
                            button type="submit" class="btn btn-primary" { "Autoresolve" }
                        }
                    } @else if resolved {
                        span class="badge badge-info" { "Quest resolved" }
                    } @else {
                        span class="badge badge-info" { "Waiting for party leader" }
                    }
                }
            }
            (settlement_chat_area(&quest.title, active_character))
        }
    }
}

/// Loot and shared inventory at an off-road quest location.
pub fn quest_location_page(
    quest: &Quest,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
    loot: &[BattleLootItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[ItemDefinition],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Loot", html! {
                @if loot.is_empty() {
                    (empty_state(
                        if resolved { "No unclaimed loot remains." } else { "No loot has been recovered." },
                        None,
                        None,
                    ))
                } @else {
                    table class="trade-inventory-table" {
                        thead { tr { th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in loot {
                                @let value = items.iter().find(|item| item.id == entry.item_id).and_then(|item| item.base_value).unwrap_or(0);
                                tr { td { (&entry.item_id) } td { (entry.quantity) } td { (u64::from(value) * u64::from(entry.quantity)) } }
                            }
                        }
                    }
                    form method="post" action=(format!("/quests/{}/loot/store", quest.id)) {
                        button type="submit" class="btn btn-primary btn-block" { "Store all in party inventory" }
                    }
                }
            }))
        }

        (quest_location_center(quest, active_character, party_members, can_fight, resolved))

        aside class="right-sidebar" {
            (sidebar_section("Party inventory", html! {
                div class="party-stake-summary" {
                    span { "Your available stake" }
                    strong { (stake) " gold" }
                }
                @if pooled.is_empty() {
                    (empty_state("The party chest is empty.", None, None))
                } @else {
                    table class="trade-inventory-table" {
                        thead { tr { th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in pooled {
                                @let value = items.iter().find(|item| item.id == entry.item_id).and_then(|item| item.base_value).unwrap_or(0);
                                tr {
                                    td { (&entry.item_id) }
                                    td { (entry.quantity) }
                                    td { (u64::from(value) * u64::from(entry.quantity)) }
                                }
                            }
                        }
                    }
                }
            }))
        }
    };
    super::quest_location_layout_with_session(
        &quest.title,
        &quest.title,
        &quest.id,
        "loot",
        content,
        logged_in_as,
        theme,
    )
}

/// Resolution screen shown before the party banks a battle's spoils.
pub fn post_battle_page(
    quest: &Quest,
    character: &Character,
    loot: &[BattleLootItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[ItemDefinition],
    theme: &str,
) -> Markup {
    let loot_value: u64 = loot
        .iter()
        .map(|entry| {
            let value = items
                .iter()
                .find(|item| item.id == entry.item_id)
                .and_then(|item| item.base_value)
                .unwrap_or(0);
            u64::from(value) * u64::from(entry.quantity)
        })
        .sum();
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Loot", html! {
                @if loot.is_empty() {
                    (empty_state("All loot has been moved to the party inventory.", None, None))
                } @else {
                    table class="trade-inventory-table" {
                        thead { tr { th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in loot {
                                @let value = items.iter().find(|item| item.id == entry.item_id).and_then(|item| item.base_value).unwrap_or(0);
                                tr { td { (&entry.item_id) } td { (entry.quantity) } td { (u64::from(value) * u64::from(entry.quantity)) } }
                            }
                        }
                    }
                    p class="party-stake-summary" { span { "Total objective value" } strong { (loot_value) " gold" } }
                    form method="post" action=(format!("/quests/{}/loot/store", quest.id)) {
                        button type="submit" class="btn btn-primary btn-block" { "Store all in party inventory" }
                    }
                }
            }))
        }
        main class="center-content settlement-main quest-location-main" {
            (visual_stage("map", "Victory", "TODO: post-battle scene"))
            div class="post-battle-summary" {
                h2 { "Victory" }
                p { "The party has defeated " (quest.enemy_count) " " (&quest.enemy_type) "." }
            }
            (settlement_chat_area(&quest.title, Some(character)))
        }
        aside class="right-sidebar" {
            (sidebar_section("Party inventory", html! {
                div class="party-stake-summary" { span { "Your available stake" } strong { (stake) " gold" } }
                @if pooled.is_empty() {
                    (empty_state("The party chest is empty.", None, None))
                } @else {
                    table class="trade-inventory-table" {
                        thead { tr { th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in pooled {
                                @let value = items.iter().find(|item| item.id == entry.item_id).and_then(|item| item.base_value).unwrap_or(0);
                                tr { td { (&entry.item_id) } td { (entry.quantity) } td { (u64::from(value) * u64::from(entry.quantity)) } }
                            }
                        }
                    }
                }
                @if loot.is_empty() {
                    p class="small-copy text-muted" { "Your share of the new loot has been credited at its objective value." }
                    @if let Some(settlement_id) = &character.current_settlement_id {
                        a class="btn btn-primary btn-block" href=(format!("/locations/settlement/{}/party-inventory", settlement_id)) { "Open party inventory" }
                    } @else {
                        a class="btn btn-secondary btn-block" href=(format!("/locations/quest/{}", quest.id)) { "Return to location" }
                    }
                }
            }))
        }
    };
    base_layout_with_session("Battle spoils", content, Some(&character.name), theme)
}
