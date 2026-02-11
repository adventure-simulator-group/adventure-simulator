//! Character templates

use maud::{html, Markup};

use super::{
    base_layout_with_session, card, empty_state, input_field, list_item, loading_indicator,
};
use crate::spacetimedb::{Character, InventoryItem};

/// List all characters
pub fn characters_list_page(
    characters: &[Character],
    current_character_id: Option<&str>,
) -> Markup {
    let content = html! {
        div class="characters-page" {
            div class="page-header" {
                h2 { "Characters" }
                a href="/characters/new" class="btn btn-primary" {
                    "Create New Character"
                }
            }

            @if let Some(current_id) = current_character_id {
                @if let Some(current) = characters.iter().find(|c| c.id == current_id) {
                    div class="current-character-banner" {
                        p { "Playing as: " strong { (current.name) } " (Level " (current.level) ")" }
                        form action="/characters/logout" method="post" style="display:inline" {
                            button type="submit" class="btn btn-small btn-secondary" {
                                "Switch Character"
                            }
                        }
                    }
                }
            }

            @if characters.is_empty() {
                (empty_state(
                    "No characters yet. Create your first adventurer!",
                    Some("/characters/new"),
                    Some("Create Character")
                ))
            } @else {
                div #"character-list" class="list" {
                    @for character in characters {
                        @let is_current = current_character_id == Some(&character.id);
                        div class=(if is_current { "list-item-wrapper current" } else { "list-item-wrapper" }) {
                            (list_item(
                                &format!("/characters/{}", character.id),
                                &character.name,
                                Some(&format!("Level {} - {} Gold{}", character.level, character.gold, if is_current { " (Playing)" } else { "" }))
                            ))
                            @if !is_current {
                                form action=(format!("/characters/{}/select", character.id)) method="post" class="select-form" {
                                    button type="submit" class="btn btn-small btn-primary" {
                                        "Play"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    // Find current character name for header
    let logged_in_as = current_character_id
        .and_then(|id| characters.iter().find(|c| c.id == id))
        .map(|c| c.name.as_str());

    base_layout_with_session("Characters", content, logged_in_as)
}

/// Character creation form
pub fn character_new_page(logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="character-new-page" {
            h2 { "Create New Character" }

            (card("Character Details", html! {
                // Use regular form POST to ensure cookie is set properly
                form #"character-form" action="/characters" method="post" {
                    (input_field("name", "Character Name", "text", true, None))

                    div class="form-actions" {
                        button type="submit" class="btn btn-primary" {
                            "Create Character"
                        }
                    }
                }
            }))
        }
    };

    base_layout_with_session("Create Character", content, logged_in_as)
}

/// Character detail/sheet page
pub fn character_detail_page(
    character: &Character,
    inventory: &[InventoryItem],
    is_current: bool,
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        div class="character-detail-page" {
            div class="page-header" {
                h2 { (character.name) }
                span class="level-badge" { "Level " (character.level) }
                @if is_current {
                    span class="playing-badge" { "(Playing)" }
                }
            }

            @if !is_current {
                div class="select-character-prompt" {
                    form action=(format!("/characters/{}/select", character.id)) method="post" {
                        button type="submit" class="btn btn-primary btn-large" {
                            "Play as " (character.name)
                        }
                    }
                }
            }

            div class="character-grid" {
                // Stats card
                (card("Stats", html! {
                    div class="stats-grid" {
                        div class="stat" {
                            span class="stat-label" { "XP" }
                            span class="stat-value" { (character.xp) }
                        }
                        div class="stat" {
                            span class="stat-label" { "Gold" }
                            span class="stat-value" { (character.gold) }
                        }
                        div class="stat" {
                            span class="stat-label" { "Level" }
                            span class="stat-value" { (character.level) }
                        }
                    }
                    div class="xp-progress" {
                        div class="progress-bar" {
                            div class="progress-fill" style=(format!("width: {}%", (character.xp % 100))) {}
                        }
                        span class="progress-label" {
                            (character.xp % 100) " / 100 XP to next level"
                        }
                    }
                }))

                // Location card
                (card("Location", html! {
                    @if let Some(settlement_id) = &character.current_settlement_id {
                        p { "Currently at: " (settlement_id) }
                        a href=(format!("/settlements/{}", settlement_id)) class="btn btn-secondary" {
                            "Go to Settlement"
                        }
                    } @else {
                        p { "Traveling..." }
                        a href="/settlements" class="btn btn-secondary" {
                            "View Map"
                        }
                    }
                }))

                // Party card
                (card("Party", html! {
                    @if let Some(party_id) = &character.party_id {
                        p { "Member of party: " (party_id) }
                        a href=(format!("/parties/{}", party_id)) class="btn btn-secondary" {
                            "View Party"
                        }
                    } @else {
                        p { "Not in a party" }
                        a href="/parties" class="btn btn-primary" {
                            "Find Party"
                        }
                    }
                }))

                // Inventory card
                (card("Inventory", html! {
                    @if inventory.is_empty() {
                        p class="empty-text" { "No items" }
                    } @else {
                        ul class="inventory-list" {
                            @for item in inventory {
                                li class="inventory-item" {
                                    span class="item-name" { (item.item_id) }
                                    span class="item-qty" { "x" (item.qty) }
                                }
                            }
                        }
                    }
                }))
            }

            // Edit form
            (card("Edit Character", html! {
                form data-on-submit=(format!("@post('/characters/{}')", character.id)) {
                    (input_field("name", "Name", "text", true, Some(&character.name)))

                    div class="form-actions" {
                        button type="submit" class="btn btn-secondary" {
                            "Update"
                        }
                        (loading_indicator("update-loading"))
                    }
                }
            }))
        }
    };

    base_layout_with_session(&character.name, content, logged_in_as)
}

/// Character list fragment for Datastar updates
pub fn characters_list_fragment(characters: &[Character]) -> Markup {
    html! {
        div #"character-list" class="list" {
            @for character in characters {
                (list_item(
                    &format!("/characters/{}", character.id),
                    &character.name,
                    Some(&format!("Level {} - {} Gold", character.level, character.gold))
                ))
            }
        }
    }
}
