//! Character templates

use maud::{html, Markup};

use super::{
    base_layout_with_session, divider, gold_display, input_field, list_item, loading_indicator,
    panel, sidebar_section, xp_display,
};
use crate::models::{Character, InventoryItem};

/// List all characters
pub fn characters_list_page(
    characters: &[Character],
    current_character_id: Option<i64>,
    theme: &str,
) -> Markup {
    let logged_in_as = current_character_id
        .and_then(|id| characters.iter().find(|c| c.id == id))
        .map(|c| c.name.as_str());

    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Characters", html! {
                @if characters.is_empty() {
                    p class="text-muted" style="font-size:var(--font-size-sm)" {
                        "No characters yet"
                    }
                } @else {
                    div #"character-list" {
                        @for character in characters {
                            @let is_current = current_character_id == Some(character.id);
                            div class=(if is_current { "list-item-wrapper current" } else { "list-item-wrapper" }) {
                                (list_item(
                                    &format!("/characters/{}", character.id),
                                    &character.name,
                                    Some(&format!("Lv{} - {} gold{}", character.level, character.gold, if is_current { " (playing)" } else { "" })),
                                ))
                            }
                        }
                    }
                }
            }))

            (divider())

            a href="/characters/new" class="btn btn-primary btn-block" {
                "Create Character"
            }
        }

        main class="center-content" {
            @if let Some(current_id) = current_character_id {
                @if let Some(current) = characters.iter().find(|c| c.id == current_id) {
                    h2 class="page-title" { (current.name) }
                    div class="character-banner" {
                        span { "Currently playing as " strong { (current.name) } " (Level " (current.level) ")" }
                        form action="/characters/logout" method="post" style="display:inline" {
                            button type="submit" class="btn btn-small btn-secondary" { "Switch" }
                        }
                    }
                    (panel("Stats", html! {
                        div class="stat-grid" {
                            div class="stat-item" {
                                span class="stat-label" { "Level" }
                                span class="stat-value" { (current.level) }
                            }
                            div class="stat-item" {
                                span class="stat-label" { "Gold" }
                                span class="stat-value" { (gold_display(current.gold)) }
                            }
                            div class="stat-item" {
                                span class="stat-label" { "XP" }
                                span class="stat-value" { (xp_display(current.xp)) }
                            }
                        }
                    }))
                } @else {
                    div class="center-welcome" {
                        h2 { "Select a Character" }
                        p { "Choose a character from the list to view details." }
                    }
                }
            } @else {
                div class="center-welcome" {
                    h2 { "Characters" }
                    p { "Select a character to play, or create a new one." }
                }
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Actions", html! {
                div class="service-menu" {
                    a href="/characters/new" class="service-menu-item" { "Create New Character" }
                    a href="/settlements" class="service-menu-item" { "View Settlements" }
                    a href="/parties" class="service-menu-item" { "Find Party" }
                }
            }))
        }
    };

    base_layout_with_session("Characters", content, logged_in_as, theme)
}

/// Character creation form
pub fn character_new_page(logged_in_as: Option<&str>, theme: &str) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Tips", html! {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" {
                        "Choose a name for your adventurer. You'll start with "
                        "100 gold and some basic supplies."
                    }
                }))
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Create Character" }
            (panel("Character Details", html! {
                form #"character-form" action="/characters" method="post" {
                    (input_field("name", "Character Name", "text", true, None))
                    div class="form-actions" {
                        button type="submit" class="btn btn-primary" { "Create Character" }
                        a href="/characters" class="btn btn-secondary" { "Cancel" }
                    }
                }
            }))
        }

        aside class="right-sidebar" {
            (sidebar_section("Starting Equipment", html! {
                (panel("", html! {
                    div class="inventory-list" {
                        div class="inventory-item" {
                            span class="item-name" { "Torch" }
                            span class="item-qty" { "x1" }
                        }
                        div class="inventory-item" {
                            span class="item-name" { "Bandage" }
                            span class="item-qty" { "x3" }
                        }
                    }
                }))
            }))
        }
    };

    base_layout_with_session("Create Character", content, logged_in_as, theme)
}

/// Character detail page
pub fn character_detail_page(
    character: &Character,
    inventory: &[InventoryItem],
    is_current: bool,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Stats", html! {
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

            (sidebar_section("Location", html! {
                @if let Some(settlement_id) = &character.current_settlement_id {
                    a href=(format!("/settlements/{}", settlement_id)) class="btn btn-secondary btn-block btn-small" {
                        "Go to " (settlement_id)
                    }
                } @else {
                    p class="text-muted" style="font-size:var(--font-size-sm)" { "Traveling..." }
                    a href="/settlements" class="btn btn-secondary btn-block btn-small" { "View Map" }
                }
            }))

            (sidebar_section("Party", html! {
                @if let Some(party_id) = &character.party_id {
                    a href=(format!("/parties/{}", party_id)) class="btn btn-secondary btn-block btn-small" {
                        "View Party"
                    }
                } @else {
                    p class="text-muted" style="font-size:var(--font-size-sm)" { "Not in a party" }
                    a href="/parties" class="btn btn-primary btn-block btn-small" { "Find Party" }
                }
            }))
        }

        main class="center-content" {
            div class="flex items-center justify-between mb-2" {
                h2 class="page-title" style="margin-bottom:0;flex:1" { (character.name) }
                @if is_current {
                    (super::status_badge("Playing"))
                }
            }

            @if !is_current {
                div class="character-banner" {
                    span { "Select this character to play" }
                    form action=(format!("/characters/{}/select", character.id)) method="post" {
                        button type="submit" class="btn btn-primary btn-small" { "Play as " (character.name) }
                    }
                }
            }

            (panel("Edit Character", html! {
                form action=(format!("/characters/{}", character.id)) method="post" {
                    (input_field("name", "Name", "text", true, Some(&character.name)))
                    div class="form-actions" {
                        button type="submit" class="btn btn-secondary" { "Update" }
                        (loading_indicator("update-loading"))
                    }
                }
            }))
        }

        aside class="right-sidebar" {
            (sidebar_section("Inventory", html! {
                @if inventory.is_empty() {
                    p class="text-muted" style="font-size:var(--font-size-sm)" { "No items" }
                } @else {
                    div class="inventory-list" {
                        @for item in inventory {
                            div class="inventory-item" {
                                span class="item-name" { (item.item_id) }
                                span class="item-qty" { "x" (item.qty) }
                            }
                        }
                    }
                }
            }))
        }
    };

    base_layout_with_session(&character.name, content, logged_in_as, theme)
}

/// Character list fragment for Datastar updates
pub fn characters_list_fragment(characters: &[Character]) -> Markup {
    html! {
        div #"character-list" {
            @for character in characters {
                (list_item(
                    &format!("/characters/{}", character.id),
                    &character.name,
                    Some(&format!("Level {} - {} Gold", character.level, character.gold)),
                ))
            }
        }
    }
}
