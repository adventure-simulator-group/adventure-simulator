//! Character selection and creation templates.

use maud::{Markup, html};

use super::{entry_layout, gold_display, input_field, panel, sidebar_section};
use crate::spacetimedb::Character;

/// List all characters and select the adventurer who enters the strategic layer.
pub fn characters_list_page(
    characters: &[Character],
    current_character_id: Option<u64>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Choose an adventurer", html! {
                p class="small-copy text-muted" { "A character must be selected before entering the strategic layer." }
                a href="/characters/new" class="btn btn-primary btn-block mt-1" { "Create adventurer" }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Select your adventurer" }
            @if characters.is_empty() {
                div class="center-welcome" {
                    p { "Create your first adventurer to begin." }
                    a href="/characters/new" class="btn btn-primary mt-1" { "Create adventurer" }
                }
            } @else {
                div class="character-select-grid" {
                    @for character in characters {
                        @let is_current = current_character_id == Some(character.id);
                        (panel(&character.name, html! {
                            div class="stat-grid" {
                                div class="stat-item" {
                                    span class="stat-label" { "Status" }
                                    span class="stat-value" {
                                        @if character.alive { "Alive" } @else { span class="badge badge-danger" { "Dead" } }
                                    }
                                }
                                div class="stat-item" { span class="stat-label" { "Gold" } span class="stat-value" { (gold_display(character.gold)) } }
                            }
                            @if is_current {
                                p class="text-accent small-copy" {
                                    @if character.alive { "Currently selected" } @else { "Currently viewed" }
                                }
                            }
                            form action=(format!("/characters/{}/select", character.id)) method="post" class="mt-1" {
                                button type="submit" class="btn btn-primary btn-block" {
                                    @if !character.alive { "View " (&character.name) }
                                    @else if is_current { "Continue" }
                                    @else { "Play as " (&character.name) }
                                }
                            }
                        }))
                    }
                }
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Starting settlement", html! {
                p class="small-copy text-muted" { "New adventurers begin at a random settlement with basic supplies." }
            }))
        }
    };

    entry_layout("Select Adventurer", content, theme)
}

#[cfg(test)]
mod tests {
    use super::characters_list_page;
    use crate::spacetimedb::Character;

    #[test]
    fn dead_character_is_labeled_and_uses_view_wording() {
        let character = Character {
            id: 7,
            name: "Fallen Adventurer".into(),
            xp: 0,
            level: 1,
            gold: 100,
            current_settlement_id: Some("ironforge".into()),
            current_quest_location_id: None,
            party_id: Some("solo-7".into()),
            age_years: 30,
            alive: false,
            temporary: false,
        };

        let markup = characters_list_page(&[character], Some(7), "light").into_string();
        assert!(markup.contains("Dead"));
        assert!(markup.contains("Currently viewed"));
        assert!(markup.contains("View Fallen Adventurer"));
        assert!(!markup.contains("Play as Fallen Adventurer"));
        assert!(!markup.contains(">Continue<"));
    }
}

/// Character creation form.
pub fn character_new_page(_logged_in_as: Option<&str>, theme: &str) -> Markup {
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
                form # "character-form" action="/characters" method="post" {
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

    entry_layout("Create Character", content, theme)
}
