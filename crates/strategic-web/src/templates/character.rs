//! Character selection and creation templates.

use maud::{Markup, html};

use super::{entry_layout, panel, sidebar_section};
use crate::spacetimedb::Character;
use adventuresim_core::starting_character::{StartingCharacterSpec, personality_description};

/// List all characters and select the adventurer who enters the strategic layer.
pub fn characters_list_page(characters: &[Character], current_character_id: Option<u64>) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Choose an adventurer", html! {
                p class="small-copy text-muted" { "A character must be selected before entering the strategic layer." }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Select your adventurer" }
            @if characters.is_empty() {
                div class="center-welcome" {
                    p { "No persisted adventurers are available." }
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

const PROTOTYPE_NOTICE: &str = "Early prototype: All text and images are placeholders. Features and saved progress may change or be reset during development.";

pub fn character_candidates_bootstrap_page(version: u16) -> Markup {
    let content = html! {
        main class="center-content candidate-bootstrap" {
            p class="prototype-disclaimer" role="note" { (PROTOTYPE_NOTICE) }
            h2 class="page-title" { "Gathering candidates…" }
            p class="small-copy text-muted" { "Preparing your first company." }
            noscript { p role="alert" { "JavaScript is required to prepare a private candidate roster." } }
            div data-candidate-bootstrap data-generator-version=(version) {}
            script src="/static/character-candidates.js?v=1" defer {}
        }
    };
    entry_layout("Choose Your Adventurer", content)
}

pub fn character_candidates_page(
    version: u16,
    seed: &str,
    candidates: &[StartingCharacterSpec],
    selected: Option<u8>,
) -> Markup {
    let candidate = &candidates[selected.unwrap_or(0) as usize];
    let close_href = format!("/characters/candidates?version={version}&seed={seed}");
    let content = html! {
        aside class="left-sidebar candidate-rail" {
            (sidebar_section("Attributes", html! {
                div class="candidate-stat-list" {
                    @for (label, value) in [("Endurance", candidate.attributes.endurance), ("Precision", candidate.attributes.precision), ("Intelligence", candidate.attributes.intelligence), ("Instinct", candidate.attributes.instinct), ("Strength", candidate.attributes.strength), ("Agility", candidate.attributes.agility)] {
                        div class="stat-item" { span class="stat-label" { (label) } span class="stat-value" { (format!("{value:.1}")) } }
                    }
                }
            }))
        }

        main class="center-content candidate-stage" data-candidate-roster {
            p class="prototype-disclaimer" role="note" { (PROTOTYPE_NOTICE) }
            h2 class="page-title" { "Choose your adventurer" }
            ul class="candidate-portraits" aria-label="Five candidate adventurers" {
                @for (slot, entry) in candidates.iter().enumerate() {
                    li {
                        a class=(if selected == Some(slot as u8) { "party-portrait candidate-portrait active" } else { "party-portrait candidate-portrait" })
                            aria-current=[(selected == Some(slot as u8)).then_some("true")]
                            data-candidate-slot=(slot)
                            href=(format!("/characters/candidates?version={version}&seed={seed}&selected={slot}")) {
                            span class="party-portrait-initial" aria-hidden="true" { span class="party-portrait-face" { (entry.name.chars().next().unwrap_or('?')) } }
                            span class="party-portrait-name" { (&entry.name) }
                        }
                    }
                }
            }
            (panel(&candidate.name, html! {
                p class="candidate-byline" { (candidate.age_years) " years old · " (&candidate.background) }
                p { (personality_description(&candidate.personality)) }
            }))
            @if let Some(selected) = selected {
                section class="candidate-confirm-backdrop" data-candidate-dialog-backdrop {
                    div class="candidate-confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="candidate-dialog-title" tabindex="-1" data-candidate-dialog {
                        h3 id="candidate-dialog-title" { "Begin as " (&candidate.name) "?" }
                        p { "This choice creates the adventurer and enters the game." }
                        form action="/characters/candidates" method="post" data-candidate-confirm-form {
                            input type="hidden" name="version" value=(version);
                            input type="hidden" name="seed" value=(seed);
                            input type="hidden" name="slot" value=(selected);
                            div class="form-actions" {
                                a href=(close_href) class="btn btn-secondary" data-candidate-dialog-close { "Keep looking" }
                                button type="submit" class="btn btn-primary" { "Begin as " (&candidate.name) }
                            }
                        }
                    }
                }
            }
            script src="/static/character-candidates.js?v=1" defer {}
        }
        aside class="right-sidebar candidate-rail" {
            (sidebar_section("Training", html! {
                div class="candidate-skill-list" {
                    @for (label, hours) in [("Polearm", candidate.skills.polearm), ("Sword", candidate.skills.sword), ("Knife", candidate.skills.knife), ("Bow", candidate.skills.bow), ("Crossbow", candidate.skills.crossbow), ("Block", candidate.skills.block), ("Dodge", candidate.skills.dodge), ("Medicine", candidate.skills.medicine)] {
                        @if hours >= 500.0 { div class="stat-item" { span class="stat-label" { (label) } span class="stat-value" { (format!("{hours:.0} h")) } } }
                    }
                }
            }))
            (sidebar_section("Equipment & coin", html! {
                ul class="candidate-equipment" {
                    @for item in &candidate.inventory { li { (item.item_id.replace('_', " ")) @if item.quantity > 1 { " ×" (item.quantity) } @if item.equipped.is_some() { " (equipped)" } } }
                }
                p class="candidate-currency" { strong { (candidate.currency) " coin" } }
            }))
        }
    };

    entry_layout("Choose Your Adventurer", content)
}

#[cfg(test)]
mod creation_tests {
    use super::{PROTOTYPE_NOTICE, character_candidates_page};
    use adventuresim_core::starting_character::roster;

    #[test]
    fn initial_roster_has_preview_but_no_dialog_or_customization() {
        let candidates = roster(1, "00112233445566778899aabbccddeeff").unwrap();
        let markup =
            character_candidates_page(1, "00112233445566778899aabbccddeeff", &candidates, None)
                .into_string();
        assert_eq!(markup.matches("candidate-portrait").count(), 5);
        assert!(markup.contains(PROTOTYPE_NOTICE));
        assert!(!markup.contains("role=\"dialog\""));
        assert!(!markup.contains("role=\"listitem\""));
        assert!(markup.contains("<ul"));
        assert!(!markup.contains("name=\"name\""));
    }

    #[test]
    fn explicit_selection_opens_accessible_confirmation_dialog() {
        let candidates = roster(1, "00112233445566778899aabbccddeeff").unwrap();
        let markup =
            character_candidates_page(1, "00112233445566778899aabbccddeeff", &candidates, Some(2))
                .into_string();
        assert!(markup.contains("role=\"dialog\""));
        assert!(markup.contains("aria-modal=\"true\""));
        assert!(markup.contains("name=\"slot\" value=\"2\""));
    }
}
