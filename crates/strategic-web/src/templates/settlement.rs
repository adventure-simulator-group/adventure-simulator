//! Settlement templates

use maud::{html, Markup};

use super::{
    base_layout_with_session, card, difficulty_stars, empty_state, gold_display, list_item,
    status_badge, xp_display,
};
use crate::spacetimedb::{Party, Quest, Settlement};

/// List all settlements (map view)
pub fn settlements_list_page(settlements: &[Settlement], logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="settlements-page" {
            h2 { "World Map" }

            @if settlements.is_empty() {
                (empty_state("No settlements discovered yet.", None, None))
            } @else {
                div class="settlements-grid" {
                    @for settlement in settlements {
                        (settlement_card(settlement))
                    }
                }
            }
        }
    };

    base_layout_with_session("Settlements", content, logged_in_as)
}

fn settlement_card(settlement: &Settlement) -> Markup {
    html! {
        a href=(format!("/settlements/{}", settlement.id)) class="settlement-card" {
            h3 { (settlement.name) }
            p class="population" { (population_description(settlement.population_level)) }
            div class="coords" {
                "(" (settlement.coord_x as i32) ", " (settlement.coord_y as i32) ")"
            }
        }
    }
}

/// Settlement detail page with tabs for services
pub fn settlement_detail_page(
    settlement: &Settlement,
    quests: &[Quest],
    parties: &[Party],
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        div class="settlement-detail-page" {
            div class="page-header" {
                h2 { (settlement.name) }
                span class="population-badge" { (population_description(settlement.population_level)) }
            }

            // Service tabs
            nav class="service-tabs" {
                a href=(format!("/settlements/{}", settlement.id)) class="service-tab active" {
                    "Overview"
                }
                a href=(format!("/settlements/{}/noticeboard", settlement.id)) class="service-tab" {
                    "Notice Board"
                }
                a href=(format!("/settlements/{}/tavern", settlement.id)) class="service-tab" {
                    "Tavern"
                }
                a href=(format!("/settlements/{}/merchants", settlement.id)) class="service-tab" {
                    "Merchants"
                }
                a href=(format!("/settlements/{}/smith", settlement.id)) class="service-tab" {
                    "Smith"
                }
                a href=(format!("/settlements/{}/inn", settlement.id)) class="service-tab" {
                    "Inn"
                }
            }

            div class="settlement-content" {
                // Travel action
                (card("Travel", html! {
                    p { "Move your current character to this settlement." }
                    form action=(format!("/settlements/{}/travel", settlement.id)) method="post" {
                        button type="submit" class="btn btn-primary" {
                            "Travel Here"
                        }
                    }
                }))

                // Overview
                (card("About", html! {
                    p { "A " (population_description(settlement.population_level).to_lowercase()) " with various services available." }
                    p { "Scene: " (settlement.scene_key) }
                }))

                // Available quests preview
                (card("Available Quests", html! {
                    @if quests.is_empty() {
                        p { "No quests available at this time." }
                    } @else {
                        div class="quest-preview-list" {
                            @for quest in quests.iter().take(3) {
                                (quest_preview_item(quest))
                            }
                        }
                        @if quests.len() > 3 {
                            a href=(format!("/settlements/{}/noticeboard", settlement.id)) class="btn btn-secondary" {
                                "View All Quests"
                            }
                        }
                    }
                }))

                // Parties in town
                (card("Parties in Town", html! {
                    @if parties.is_empty() {
                        p { "No parties currently in town." }
                    } @else {
                        div class="parties-list" {
                            @for party in parties {
                                (list_item(
                                    &format!("/parties/{}", party.id),
                                    &party.name,
                                    party.active_quest_id.as_ref().map(|_| "On a quest")
                                ))
                            }
                        }
                    }
                    a href=(format!("/settlements/{}/tavern", settlement.id)) class="btn btn-secondary" {
                        "Visit Tavern"
                    }
                }))
            }
        }
    };

    base_layout_with_session(&settlement.name, content, logged_in_as)
}

/// Notice board page (quests)
pub fn noticeboard_page(
    settlement: &Settlement,
    quests: &[Quest],
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        div class="noticeboard-page" {
            div class="page-header" {
                a href=(format!("/settlements/{}", settlement.id)) class="back-link" {
                    "\u{2190} Back to " (settlement.name)
                }
                h2 { "Notice Board" }
            }

            @if quests.is_empty() {
                (empty_state("No quests posted on the notice board.", None, None))
            } @else {
                div class="quest-list" {
                    @for quest in quests {
                        (quest_notice(quest))
                    }
                }
            }
        }
    };

    base_layout_with_session("Notice Board", content, logged_in_as)
}

fn quest_notice(quest: &Quest) -> Markup {
    html! {
        div class="quest-notice" {
            div class="notice-header" {
                h3 { (quest.title) }
                (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
            }
            p class="notice-description" { (quest.description) }
            div class="notice-details" {
                span class="difficulty" {
                    "Difficulty: "
                    (difficulty_stars(quest.difficulty))
                }
                span class="rewards" {
                    "Rewards: "
                    (gold_display(quest.gold_reward))
                    " / "
                    (xp_display(quest.xp_reward))
                }
            }
            div class="notice-enemy" {
                "Target: " (quest.enemy_count) " " (quest.enemy_type)
            }
            @if quest.status.to_lowercase().contains("available") {
                form data-on-submit=(format!("@post('/quests/{}/accept')", quest.id)) {
                    button type="submit" class="btn btn-primary" {
                        "Accept Quest"
                    }
                }
            }
        }
    }
}

fn quest_preview_item(quest: &Quest) -> Markup {
    html! {
        a href=(format!("/quests/{}", quest.id)) class="quest-preview" {
            span class="quest-title" { (quest.title) }
            span class="quest-reward" { (gold_display(quest.gold_reward)) }
        }
    }
}

/// Tavern page (party formation, social)
pub fn tavern_page(
    settlement: &Settlement,
    parties: &[Party],
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        div class="tavern-page" {
            div class="page-header" {
                a href=(format!("/settlements/{}", settlement.id)) class="back-link" {
                    "\u{2190} Back to " (settlement.name)
                }
                h2 { "The Tavern" }
            }

            (card("Parties Looking for Members", html! {
                @if parties.is_empty() {
                    p { "No parties currently recruiting." }
                } @else {
                    div class="parties-list" {
                        @for party in parties {
                            div class="party-recruit" {
                                h4 { (party.name) }
                                @if party.active_quest_id.is_some() {
                                    span class="badge" { "On Quest" }
                                }
                                form data-on-submit=(format!("@post('/parties/{}/join')", party.id)) {
                                    button type="submit" class="btn btn-secondary" {
                                        "Join Party"
                                    }
                                }
                            }
                        }
                    }
                }
            }))

            (card("Form New Party", html! {
                a href="/parties/new" class="btn btn-primary" {
                    "Create Party"
                }
            }))
        }
    };

    base_layout_with_session("Tavern", content, logged_in_as)
}

/// Merchants page placeholder
pub fn merchants_page(settlement: &Settlement, logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="merchants-page" {
            div class="page-header" {
                a href=(format!("/settlements/{}", settlement.id)) class="back-link" {
                    "\u{2190} Back to " (settlement.name)
                }
                h2 { "Merchants" }
            }

            (card("General Store", html! {
                p { "Coming soon - buy and sell goods here." }
            }))
        }
    };

    base_layout_with_session("Merchants", content, logged_in_as)
}

/// Smith page placeholder
pub fn smith_page(settlement: &Settlement, logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="smith-page" {
            div class="page-header" {
                a href=(format!("/settlements/{}", settlement.id)) class="back-link" {
                    "\u{2190} Back to " (settlement.name)
                }
                h2 { "The Smithy" }
            }

            (card("Services", html! {
                p { "Coming soon - repair and upgrade equipment here." }
            }))
        }
    };

    base_layout_with_session("Smith", content, logged_in_as)
}

/// Inn page placeholder
pub fn inn_page(settlement: &Settlement, logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="inn-page" {
            div class="page-header" {
                a href=(format!("/settlements/{}", settlement.id)) class="back-link" {
                    "\u{2190} Back to " (settlement.name)
                }
                h2 { "The Inn" }
            }

            (card("Rest", html! {
                p { "Coming soon - rest and recover here." }
            }))
        }
    };

    base_layout_with_session("Inn", content, logged_in_as)
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
