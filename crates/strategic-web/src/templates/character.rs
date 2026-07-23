//! Character selection and creation templates.

use maud::{Markup, html};

use super::{entry_layout, input_field, item_type_header, item_type_icon, panel, sidebar_section};
use crate::spacetimedb::Character;

/// List all characters and select the adventurer who enters the strategic layer.
pub fn characters_list_page(characters: &[Character], current_character_id: Option<u64>) -> Markup {
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
                            }
                            @if is_current {
                                p class="text-accent small-copy" {
                                    @if character.alive { "Currently selected" } @else { "Currently viewed" }
                                }
                            }
                            form action=(format!("/characters/{}/select", character.id)) method="post" class="mt-1" {
                                button type="submit" class="btn btn-primary btn-block character-select-action" {
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

    entry_layout("Select Adventurer", content)
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
            current_case_site_id: None,
            party_id: Some("solo-7".into()),
            age_years: 30,
            alive: false,
            temporary: false,
        };

        let markup = characters_list_page(&[character], Some(7)).into_string();
        assert!(markup.contains("Dead"));
        assert!(markup.contains("Currently viewed"));
        assert!(markup.contains("View Fallen Adventurer"));
        assert!(!markup.contains("Play as Fallen Adventurer"));
        assert!(!markup.contains(">Continue<"));
    }
}

/// Character creation form.
pub fn character_new_page(_logged_in_as: Option<&str>) -> Markup {
    character_new_page_with_error(None, None)
}

pub fn character_new_page_with_error(name: Option<&str>, error: Option<&str>) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Tips", html! {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" {
                        "Choose a name for your adventurer. You'll start with "
                        "100 coin and some basic supplies."
                    }
                }))
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Create Character" }
            (panel("Character Details", html! {
                form # "character-form" action="/characters" method="post" {
                    (input_field("name", "Character Name", "text", true, name))
                    @if let Some(error) = error {
                        p class="form-error" role="alert" { (error) }
                    }
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
                    table class="trade-inventory-table starting-equipment-table" {
                        thead { tr { (item_type_header()) th scope="col" { "Item" } th scope="col" { "#" } } }
                        tbody {
                            tr { td class="inventory-item-type" { (item_type_icon("torch")) } td { "Torch" } td class="inventory-count" { "1" } }
                            tr { td class="inventory-item-type" { (item_type_icon("bandage")) } td { "Bandage" } td class="inventory-count" { "3" } }
                        }
                    }
                }))
            }))
        }
    };

    entry_layout("Create Character", content)
}

#[cfg(test)]
mod creation_tests {
    use super::character_new_page;

    #[test]
    fn starting_equipment_uses_accessible_exact_item_icons() {
        let markup = character_new_page(None).into_string();
        assert!(markup.contains("starting-equipment-table"));
        assert!(markup.contains("aria-label=\"Item type\""));
        assert!(markup.contains("/static/icons/game/torch.svg"));
        assert!(markup.contains("/static/icons/game/bandage-roll.svg"));
        assert!(markup.find("inventory-column-type").unwrap() < markup.find(">Item</th>").unwrap());
    }
}
