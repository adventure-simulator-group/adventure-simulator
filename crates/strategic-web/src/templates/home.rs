//! Home page template

use maud::{html, Markup};

use super::{base_layout_with_session, card, empty_state, list_item};
use crate::spacetimedb::{Character, Party, Quest, Settlement};

pub fn home_page(
    characters: &[Character],
    current_character: Option<&Character>,
    current_settlement: Option<&Settlement>,
    active_party: Option<&Party>,
    active_quest: Option<&Quest>,
) -> Markup {
    let content = html! {
        div class="home-dashboard" {
            // Current character section
            (card("Current Character", current_character_section(current_character, characters)))

            // Current location section
            @if let Some(character) = current_character {
                (card("Current Location", location_section(character, current_settlement)))
            }

            // Party section
            @if current_character.is_some() {
                (card("Party", party_section(active_party, active_quest)))
            }

            // Quick actions
            @if current_character.is_some() {
                (card("Quick Actions", quick_actions()))
            }
        }
    };

    base_layout_with_session("Home", content, current_character.map(|c| c.name.as_str()))
}

fn current_character_section(current: Option<&Character>, all: &[Character]) -> Markup {
    html! {
        @if let Some(character) = current {
            div class="character-summary" {
                h4 { (character.name) }
                p { "Level " (character.level) " (" (character.xp) " XP)" }
                p { (character.gold) " Gold" }
            }
            a href="/characters" class="btn btn-secondary" {
                "Switch Character"
            }
        } @else if all.is_empty() {
            (empty_state(
                "No characters yet. Create one to begin your adventure!",
                Some("/characters/new"),
                Some("Create Character")
            ))
        } @else {
            p { "Select a character to play:" }
            div class="character-list" {
                @for character in all {
                    (list_item(
                        &format!("/characters/{}", character.id),
                        &character.name,
                        Some(&format!("Level {}", character.level))
                    ))
                }
            }
        }
    }
}

fn location_section(character: &Character, settlement: Option<&Settlement>) -> Markup {
    html! {
        @if let Some(settlement) = settlement {
            div class="location-summary" {
                h4 { (settlement.name) }
                p { "Population: " (population_description(settlement.population_level)) }
            }
            a href=(format!("/settlements/{}", settlement.id)) class="btn btn-primary" {
                "Enter " (settlement.name)
            }
        } @else {
            p { "You are currently traveling." }
            a href="/settlements" class="btn btn-primary" {
                "View Map"
            }
        }
    }
}

fn party_section(party: Option<&Party>, quest: Option<&Quest>) -> Markup {
    html! {
        @if let Some(party) = party {
            div class="party-summary" {
                h4 { (party.name) }
                @if let Some(quest) = quest {
                    p { "Active Quest: " (quest.title) }
                } @else {
                    p { "No active quest" }
                }
            }
            a href=(format!("/parties/{}", party.id)) class="btn btn-secondary" {
                "View Party"
            }
        } @else {
            (empty_state(
                "Not in a party. Join or create one to take on quests!",
                Some("/parties/new"),
                Some("Create Party")
            ))
        }
    }
}

fn quick_actions() -> Markup {
    html! {
        div class="quick-actions" {
            a href="/settlements" class="btn btn-action" {
                "View Settlements"
            }
            a href="/quests" class="btn btn-action" {
                "Browse Quests"
            }
            a href="/parties" class="btn btn-action" {
                "Find Party"
            }
        }
    }
}

fn population_description(level: i32) -> &'static str {
    match level {
        1 => "Hamlet",
        2 => "Village",
        3 => "Town",
        4 => "City",
        5 => "Capital",
        _ => "Unknown",
    }
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
        div #"main-content" class="home-dashboard" {
            (card("Current Character", current_character_section(current_character, characters)))

            @if let Some(character) = current_character {
                (card("Current Location", location_section(character, current_settlement)))
            }

            @if current_character.is_some() {
                (card("Party", party_section(active_party, active_quest)))
            }

            @if current_character.is_some() {
                (card("Quick Actions", quick_actions()))
            }
        }
    }
}
