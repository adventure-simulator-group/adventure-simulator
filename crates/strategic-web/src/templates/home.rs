//! Home page template

use maud::{html, Markup};

use super::{
    base_layout_with_session, divider, empty_state, gold_display, list_item, panel,
    population_description, sidebar_section, xp_display,
};
use crate::spacetimedb::{Character, Party, Quest, Settlement};

pub fn home_page(
    characters: &[Character],
    current_character: Option<&Character>,
    current_settlement: Option<&Settlement>,
    active_party: Option<&Party>,
    active_quest: Option<&Quest>,
    theme: &str,
) -> Markup {
    let content = html! {
        // Left sidebar
        aside class="left-sidebar" {
            @if let Some(character) = current_character {
                (sidebar_section("Character", html! {
                    div class="stat-grid" {
                        div class="stat-item" {
                            span class="stat-label" { "Level" }
                            span class="stat-value" { (character.level) }
                        }
                        div class="stat-item" {
                            span class="stat-label" { "Gold" }
                            span class="stat-value" { (gold_display(character.gold)) }
                        }
                        div class="stat-item" {
                            span class="stat-label" { "XP" }
                            span class="stat-value" { (xp_display(character.xp)) }
                        }
                    }
                    div class="progress-bar mt-1" {
                        div class="progress-fill" style=(format!("width: {}%", character.xp % 100)) {}
                    }
                    span class="progress-label" { (character.xp % 100) "/100 to next level" }
                }))

                (divider())

                @if let Some(party) = active_party {
                    (sidebar_section("Party", html! {
                        p class="text-accent" style="font-weight:600" { (party.name) }
                        @if let Some(quest) = active_quest {
                            p style="font-size:var(--font-size-sm);margin-top:0.25rem" {
                                "Quest: " (quest.title)
                            }
                        } @else {
                            p class="text-muted" style="font-size:var(--font-size-sm)" {
                                "No active quest"
                            }
                        }
                        a href=(format!("/parties/{}", party.id)) class="btn btn-secondary btn-small mt-1" {
                            "View Party"
                        }
                    }))
                } @else {
                    (sidebar_section("Party", html! {
                        p class="text-muted" style="font-size:var(--font-size-sm)" {
                            "Not in a party"
                        }
                        a href="/parties/new" class="btn btn-primary btn-small mt-1" {
                            "Create Party"
                        }
                    }))
                }
            } @else {
                (sidebar_section("Characters", html! {
                    @if characters.is_empty() {
                        p class="text-muted" style="font-size:var(--font-size-sm)" {
                            "No characters yet"
                        }
                        a href="/characters/new" class="btn btn-primary btn-small mt-1" {
                            "Create Character"
                        }
                    } @else {
                        @for character in characters {
                            (list_item(
                                &format!("/characters/{}", character.id),
                                &character.name,
                                Some(&format!("Level {}", character.level)),
                            ))
                        }
                    }
                }))
            }
        }

        // Center content
        main class="center-content" {
            @if let Some(character) = current_character {
                @if let Some(settlement) = current_settlement {
                    h2 class="page-title" { (settlement.name) }
                    (panel("About", html! {
                        p {
                            "A " (population_description(settlement.population_level).to_lowercase())
                            " with various services available to adventurers."
                        }
                        p { "Scene: " (settlement.scene_key) }
                    }))
                    div class="flex gap-md mt-2" {
                        a href=(format!("/settlements/{}", settlement.id)) class="btn btn-primary" {
                            "Enter " (settlement.name)
                        }
                        a href="/settlements" class="btn btn-secondary" {
                            "View Map"
                        }
                    }
                } @else {
                    div class="center-welcome" {
                        h2 { "Welcome, " (character.name) }
                        p { "You are currently traveling. Choose a destination." }
                        a href="/settlements" class="btn btn-primary btn-large" {
                            "View Settlements"
                        }
                    }
                }
            } @else {
                div class="center-welcome" {
                    h2 { "Adventure Simulator" }
                    p { "Select a character or create a new one to begin your journey." }
                    div class="flex gap-md" {
                        a href="/characters" class="btn btn-primary btn-large" {
                            "Characters"
                        }
                        a href="/characters/new" class="btn btn-secondary btn-large" {
                            "Create New"
                        }
                    }
                }
            }
        }

        // Right sidebar
        aside class="right-sidebar" {
            @if current_character.is_some() {
                @if let Some(quest) = active_quest {
                    (sidebar_section("Active Quest", html! {
                        (panel(&quest.title, html! {
                            p style="font-size:var(--font-size-sm)" { (quest.description) }
                            (divider())
                            div class="quest-card-meta" {
                                div class="quest-card-reward" {
                                    (gold_display(quest.gold_reward))
                                    (xp_display(quest.xp_reward))
                                }
                            }
                            p class="quest-card-enemy mt-1" {
                                "Target: " (quest.enemy_count) " " (quest.enemy_type)
                            }
                            a href=(format!("/quests/{}", quest.id)) class="btn btn-secondary btn-small mt-1" {
                                "View Quest"
                            }
                        }))
                    }))
                }

                (sidebar_section("Quick Actions", html! {
                    div class="service-menu" {
                        a href="/settlements" class="service-menu-item" { "View Settlements" }
                        a href="/quests" class="service-menu-item" { "Browse Quests" }
                        a href="/parties" class="service-menu-item" { "Find Party" }
                        a href="/characters" class="service-menu-item" { "Manage Characters" }
                    }
                }))
            } @else {
                (sidebar_section("Getting Started", html! {
                    (panel("", html! {
                        p style="font-size:var(--font-size-sm)" {
                            "Create a character, join a party, accept a quest, "
                            "and enter tactical missions to earn gold and experience."
                        }
                    }))
                }))
            }
        }
    };

    base_layout_with_session(
        "Home",
        content,
        current_character.map(|c| c.name.as_str()),
        theme,
    )
}

/// Home page fragment (for Datastar updates)
pub fn home_fragment(
    characters: &[Character],
    current_character: Option<&Character>,
    current_settlement: Option<&Settlement>,
    active_party: Option<&Party>,
    active_quest: Option<&Quest>,
) -> Markup {
    html! {
        div #"main-content" {
            @if let Some(character) = current_character {
                @if let Some(settlement) = current_settlement {
                    h2 class="page-title" { (settlement.name) }
                    (panel("About", html! {
                        p { "A " (population_description(settlement.population_level).to_lowercase()) " with various services." }
                    }))
                } @else {
                    div class="center-welcome" {
                        h2 { "Welcome, " (character.name) }
                        p { "You are traveling. Choose a destination." }
                    }
                }
            } @else {
                div class="center-welcome" {
                    h2 { "Adventure Simulator" }
                    p { "Select a character to begin." }
                }
            }
        }
    }
}
