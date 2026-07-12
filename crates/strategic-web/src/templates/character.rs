//! Character templates

use maud::{html, Markup};

use super::{
    base_layout_with_session, divider, entry_layout, gold_display, input_field, list_item,
    loading_indicator, panel, sidebar_section, xp_display,
};
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, InventoryItem,
};

/// List all characters
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
                                div class="stat-item" { span class="stat-label" { "Level" } span class="stat-value" { (character.level) } }
                                div class="stat-item" { span class="stat-label" { "Gold" } span class="stat-value" { (gold_display(character.gold)) } }
                            }
                            @if is_current {
                                p class="text-accent small-copy" { "Currently selected" }
                            }
                            form action=(format!("/characters/{}/select", character.id)) method="post" class="mt-1" {
                                button type="submit" class="btn btn-primary btn-block" {
                                    @if is_current { "Continue" } @else { "Play as " (&character.name) }
                                }
                            }
                        }))
                    }
                }
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Starting in Riverdale", html! {
                p class="small-copy text-muted" { "New adventurers begin in Riverdale with basic supplies." }
            }))
        }
    };

    entry_layout("Select Adventurer", content, theme)
}

/// Character creation form
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

/// Character detail page
pub fn character_detail_page(
    character: &Character,
    inventory: &[InventoryItem],
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    stats: Option<&CharacterStats>,
    limbs: Option<&CharacterLimbs>,
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

            (character_stats_sheet(attributes, skills, stats, limbs))

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

fn character_stats_sheet(
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    stats: Option<&CharacterStats>,
    limbs: Option<&CharacterLimbs>,
) -> Markup {
    let Some(attributes) = attributes else {
        return panel("Attributes & Skills", html! {
            p class="text-muted" { "Character stat data has not been created yet." }
        });
    };

    html! {
        (panel("Attributes", html! {
            div class="attribute-sheet" {
                (attribute_group("Head", html! {
                    (attribute_row("Intelligence", attributes.intelligence))
                    (attribute_row("Instinct", attributes.instinct))
                    (attribute_row("Eyesight", attributes.eyesight))
                    (attribute_row("Hearing", attributes.hearing))
                }))
                (attribute_group("Chest & Stomach", html! {
                    (attribute_row("Endurance", attributes.endurance))
                    (attribute_row("Immunity", attributes.immunity))
                    (attribute_row("Gut", attributes.gut))
                    (attribute_row("Precision", attributes.precision))
                }))
                (attribute_group("Left arm", html! {
                    (attribute_row("Strength", attributes.left_arm_strength))
                    (attribute_row("Agility", attributes.left_arm_agility))
                    @if let Some(limbs) = limbs { (health_row("Health", limbs.left_arm_health)) }
                }))
                (attribute_group("Right arm", html! {
                    (attribute_row("Strength", attributes.right_arm_strength))
                    (attribute_row("Agility", attributes.right_arm_agility))
                    @if let Some(limbs) = limbs { (health_row("Health", limbs.right_arm_health)) }
                }))
                (attribute_group("Left leg", html! {
                    (attribute_row("Strength", attributes.left_leg_strength))
                    (attribute_row("Agility", attributes.left_leg_agility))
                    @if let Some(limbs) = limbs { (health_row("Health", limbs.left_leg_health)) }
                }))
                (attribute_group("Right leg", html! {
                    (attribute_row("Strength", attributes.right_leg_strength))
                    (attribute_row("Agility", attributes.right_leg_agility))
                    @if let Some(limbs) = limbs { (health_row("Health", limbs.right_leg_health)) }
                }))
            }
            @if let Some(limbs) = limbs {
                p class="text-muted small-copy mt-1" {
                    "Torso health — Head: " (format_percent(limbs.head_health))
                    " · Chest: " (format_percent(limbs.chest_health))
                    " · Stomach: " (format_percent(limbs.stomach_health))
                }
            }
        }))
        @if let Some(skills) = skills {
            (panel("Skills & Training", html! {
                p class="text-muted small-copy" { "Bars show training progress toward the skill's asymptotic rank. Final checks, focus, armour, and fatigue penalties are TODO." }
                div class="skills-columns" {
                    div {
                        h3 class="skill-section-title" { "Mental" }
                        (training_row("Will", skills.will_hours, 5_000.0))
                        (training_row("Charisma", skills.charisma_hours, 20_000.0))
                        (training_row("Medicine", skills.medicine_hours, 10_000.0))
                        (training_row("Faith", skills.faith_hours, 5_000.0))
                    }
                    div {
                        h3 class="skill-section-title" { "Physical" }
                        (training_row("Melee", skills.melee_hours, 8_000.0))
                        (training_row("Ranged", skills.ranged_hours, 15_000.0))
                        (training_row("Dodge", skills.dodge_hours, 20_000.0))
                        (training_row("Block", skills.block_hours, 12_000.0))
                        (training_row("Stealth", skills.stealth_hours, 8_000.0))
                        (training_row("Balance", skills.balance_hours, 30_000.0))
                        (training_row("Surgeon", skills.surgeon_hours, 10_000.0))
                    }
                }
            }))
        }
        @if let Some(stats) = stats {
            (panel("Condition", html! {
                div class="condition-summary" {
                    span { "Focus " strong { (format!("{:.1}", stats.focus)) } }
                    span { "Calories used " strong { (format!("{:.0}", stats.calories_used)) } }
                }
            }))
        }
    }
}

fn attribute_group(title: &str, content: Markup) -> Markup {
    html! { section class="attribute-group" { h3 { (title) } (content) } }
}

fn attribute_row(name: &str, value: f32) -> Markup {
    let width = (value.clamp(0.0, 5.0) / 5.0) * 100.0;
    html! {
        div class="attribute-row" {
            span { (name) }
            div class="attribute-meter" title=(format!("{value:.0} / 5")) {
                span style=(format!("width:{width:.1}%")) {}
            }
            strong { (format!("{value:.0}")) }
        }
    }
}

fn health_row(name: &str, value: f32) -> Markup {
    let width = value.clamp(0.0, 1.0) * 100.0;
    html! {
        div class="attribute-row health-row" {
            span { (name) }
            div class="attribute-meter" title=(format_percent(value)) { span style=(format!("width:{width:.1}%")) {} }
            strong { (format_percent(value)) }
        }
    }
}

fn training_row(name: &str, hours: f32, half_hours: f32) -> Markup {
    let rank = 5.0 * hours / (hours + half_hours);
    let width = (rank / 5.0) * 100.0;
    html! {
        div class="training-row" {
            div class="training-label" { span { (name) } strong title=(format!("{hours:.0} hours trained")) { (format!("{rank:.0} / 5")) } }
            div class="training-meter" title=(format!("{hours:.0} hours trained")) { span style=(format!("width:{width:.1}%")) {} }
            small title=(format!("{hours:.0} hours trained")) { (format!("{rank:.0} / 5")) }
        }
    }
}

fn format_percent(value: f32) -> String {
    format!("{:.0}%", value.clamp(0.0, 1.0) * 100.0)
}

/// Character list fragment for Datastar updates
pub fn characters_list_fragment(characters: &[Character]) -> Markup {
    html! {
        div # "character-list" {
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
