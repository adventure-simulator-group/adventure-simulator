//! Settlement templates

use maud::{html, Markup};

use super::{
    base_layout_with_session, difficulty_stars, divider, empty_state, gold_display, list_item,
    panel, population_description, service_menu, sidebar_section, status_badge, xp_display,
};
use crate::models::{Party, Quest, Settlement};

/// List all settlements
pub fn settlements_list_page(
    settlements: &[Settlement],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlements", html! {
                @if settlements.is_empty() {
                    (empty_state("No settlements discovered.", None, None))
                } @else {
                    @for settlement in settlements {
                        (list_item(
                            &format!("/settlements/{}", settlement.id),
                            &settlement.name,
                            Some(population_description(settlement.population_level)),
                        ))
                    }
                }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "World Map" }
            @if settlements.is_empty() {
                div class="center-welcome" {
                    p { "No settlements have been discovered yet." }
                }
            } @else {
                div class="flex flex-col gap-md" {
                    @for settlement in settlements {
                        a href=(format!("/settlements/{}", settlement.id)) class="settlement-card" {
                            h3 { (settlement.name) }
                            p class="population" { (population_description(settlement.population_level)) }
                            div class="coords" {
                                "(" (settlement.coord_x as i32) ", " (settlement.coord_y as i32) ")"
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
                        "Travel to a settlement to access services, "
                        "find quests, and recruit party members."
                    }
                }))
            }))
        }
    };

    base_layout_with_session("Settlements", content, logged_in_as, theme)
}

/// Settlement detail page
pub fn settlement_detail_page(
    settlement: &Settlement,
    quests: &[Quest],
    parties: &[Party],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section(&settlement.name, html! {
                p style="font-size:var(--font-size-sm)" {
                    (population_description(settlement.population_level))
                }
            }))

            (divider())

            (sidebar_section("Services", html! {
                (service_menu(&settlement.id, ""))
            }))

            (divider())

            (sidebar_section("Travel", html! {
                form action=(format!("/settlements/{}/travel", settlement.id)) method="post" {
                    button type="submit" class="btn btn-primary btn-block btn-small" {
                        "Travel Here"
                    }
                }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { (settlement.name) }

            (panel("About", html! {
                p {
                    "A " (population_description(settlement.population_level).to_lowercase())
                    " with various services available to adventurers."
                }
                p { "Scene: " (settlement.scene_key) }
            }))

            @if !quests.is_empty() {
                (panel("Available Quests", html! {
                    @for quest in quests.iter().take(3) {
                        a href=(format!("/quests/{}", quest.id)) class="quest-card" {
                            div class="quest-card-header" {
                                span class="quest-card-title" { (quest.title) }
                                (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
                            }
                            div class="quest-card-meta" {
                                (difficulty_stars(quest.difficulty))
                                div class="quest-card-reward" {
                                    (gold_display(quest.gold_reward))
                                    (xp_display(quest.xp_reward))
                                }
                            }
                        }
                    }
                    @if quests.len() > 3 {
                        a href=(format!("/settlements/{}/noticeboard", settlement.id)) class="btn btn-secondary btn-small mt-1" {
                            "View All Quests"
                        }
                    }
                }))
            }
        }

        aside class="right-sidebar" {
            @if !parties.is_empty() {
                (sidebar_section("Parties in Town", html! {
                    @for party in parties {
                        (list_item(
                            &format!("/parties/{}", party.id),
                            &party.name,
                            party.active_quest_id.as_ref().map(|_| "On a quest"),
                        ))
                    }
                }))
            }

            (sidebar_section("", html! {
                a href=(format!("/settlements/{}/tavern", settlement.id)) class="btn btn-secondary btn-block btn-small" {
                    "Visit Tavern"
                }
            }))
        }
    };

    base_layout_with_session(&settlement.name, content, logged_in_as, theme)
}

/// Notice board page
pub fn noticeboard_page(
    settlement: &Settlement,
    quests: &[Quest],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section(&settlement.name, html! {
                p style="font-size:var(--font-size-sm)" {
                    (population_description(settlement.population_level))
                }
            }))
            (divider())
            (sidebar_section("Services", html! {
                (service_menu(&settlement.id, "noticeboard"))
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Notice Board" }

            @if quests.is_empty() {
                (empty_state("No quests posted on the notice board.", None, None))
            } @else {
                @for quest in quests {
                    (panel(&quest.title, html! {
                        div class="flex justify-between items-center mb-1" {
                            (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
                            (difficulty_stars(quest.difficulty))
                        }
                        p style="font-size:var(--font-size-sm)" { (quest.description) }
                        (divider())
                        div class="quest-card-meta" {
                            span class="quest-card-enemy" {
                                "Target: " (quest.enemy_count) " " (quest.enemy_type)
                            }
                            div class="quest-card-reward" {
                                (gold_display(quest.gold_reward))
                                (xp_display(quest.xp_reward))
                            }
                        }
                        @if quest.status.to_lowercase().contains("available") {
                            form action=(format!("/quests/{}/accept", quest.id)) method="post" class="mt-1" {
                                button type="submit" class="btn btn-primary btn-small" {
                                    "Accept Quest"
                                }
                            }
                        }
                    }))
                }
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Info", html! {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" {
                        "Accept quests from the notice board. "
                        "You must be a party leader at this settlement."
                    }
                }))
            }))
        }
    };

    base_layout_with_session("Notice Board", content, logged_in_as, theme)
}

/// Tavern page
pub fn tavern_page(
    settlement: &Settlement,
    parties: &[Party],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section(&settlement.name, html! {
                p style="font-size:var(--font-size-sm)" {
                    (population_description(settlement.population_level))
                }
            }))
            (divider())
            (sidebar_section("Services", html! {
                (service_menu(&settlement.id, "tavern"))
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "The Tavern" }

            (panel("Parties Looking for Members", html! {
                @if parties.is_empty() {
                    p class="text-muted" { "No parties currently recruiting." }
                } @else {
                    @for party in parties {
                        div class="member-item" {
                            div {
                                span class="member-name" { (party.name) }
                                @if party.active_quest_id.is_some() {
                                    span class="badge badge-warning ml-1" style="margin-left:0.5rem" { "On Quest" }
                                }
                            }
                            form action=(format!("/parties/{}/join", party.id)) method="post" {
                                button type="submit" class="btn btn-secondary btn-small" { "Join" }
                            }
                        }
                    }
                }
            }))

            (panel("Form New Party", html! {
                p style="font-size:var(--font-size-sm)" { "Gather allies and take on quests together." }
                a href="/parties/new" class="btn btn-primary mt-1" { "Create Party" }
            }))
        }

        aside class="right-sidebar" {
            (sidebar_section("Info", html! {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" {
                        "The tavern is where adventurers meet. "
                        "Join an existing party or form your own."
                    }
                }))
            }))
        }
    };

    base_layout_with_session("Tavern", content, logged_in_as, theme)
}

/// Merchants page placeholder
pub fn merchants_page(settlement: &Settlement, logged_in_as: Option<&str>, theme: &str) -> Markup {
    service_placeholder_page(
        settlement,
        "merchants",
        "Merchants",
        "Buy and sell goods here.",
        logged_in_as,
        theme,
    )
}

/// Smith page placeholder
pub fn smith_page(settlement: &Settlement, logged_in_as: Option<&str>, theme: &str) -> Markup {
    service_placeholder_page(
        settlement,
        "smith",
        "The Smithy",
        "Repair and upgrade equipment here.",
        logged_in_as,
        theme,
    )
}

/// Inn page placeholder
pub fn inn_page(settlement: &Settlement, logged_in_as: Option<&str>, theme: &str) -> Markup {
    service_placeholder_page(
        settlement,
        "inn",
        "The Inn",
        "Rest and recover here.",
        logged_in_as,
        theme,
    )
}

fn service_placeholder_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    description: &str,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section(&settlement.name, html! {
                p style="font-size:var(--font-size-sm)" {
                    (population_description(settlement.population_level))
                }
            }))
            (divider())
            (sidebar_section("Services", html! {
                (service_menu(&settlement.id, service_id))
            }))
        }

        main class="center-content" {
            h2 class="page-title" { (title) }
            (panel("Services", html! {
                p { "Coming soon - " (description.to_lowercase()) }
            }))
        }

        aside class="right-sidebar" {}
    };

    base_layout_with_session(title, content, logged_in_as, theme)
}
