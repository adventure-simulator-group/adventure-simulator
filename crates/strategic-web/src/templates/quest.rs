//! Quest templates

use maud::{html, Markup};

use super::{
    base_layout_with_session, difficulty_stars, divider, empty_state, gold_display, list_item,
    panel, sidebar_section, status_badge, xp_display,
};
use crate::models::Quest;

/// List all quests
pub fn quests_list_page(quests: &[Quest], logged_in_as: Option<&str>, theme: &str) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Quests", html! {
                @if quests.is_empty() {
                    (empty_state("No quests available.", None, None))
                } @else {
                    div #"quest-list" {
                        @for quest in quests {
                            (list_item(
                                &format!("/quests/{}", quest.id),
                                &quest.title,
                                Some(&quest.settlement_id),
                            ))
                        }
                    }
                }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "All Quests" }

            div class="filter-tabs" {
                button class="filter-tab active" { "All" }
                button class="filter-tab" { "Available" }
                button class="filter-tab" { "Accepted" }
                button class="filter-tab" { "Completed" }
            }

            @if quests.is_empty() {
                (empty_state("No quests available.", None, None))
            } @else {
                @for quest in quests {
                    a href=(format!("/quests/{}", quest.id)) class="quest-card" {
                        div class="quest-card-header" {
                            span class="quest-card-title" { (quest.title) }
                            (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
                        }
                        p class="quest-card-desc" { (quest.description) }
                        div class="quest-card-meta" {
                            div {
                                span style="font-size:var(--font-size-xs);color:var(--text-muted)" {
                                    (quest.settlement_id)
                                }
                                " "
                                (difficulty_stars(quest.difficulty))
                            }
                            div class="quest-card-reward" {
                                (gold_display(quest.gold_reward))
                                (xp_display(quest.xp_reward))
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
                        "Select a quest to view details. "
                        "You must be a party leader at the quest's settlement to accept it."
                    }
                }))
            }))
        }
    };

    base_layout_with_session("Quests", content, logged_in_as, theme)
}

/// Quest detail page
pub fn quest_detail_page(
    quest: &Quest,
    can_accept: bool,
    is_party_quest: bool,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Quest", html! {
                h4 style="font-family:var(--font-display);margin-bottom:0.25rem" { (quest.title) }
                (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
            }))

            (divider())

            (sidebar_section("Description", html! {
                p style="font-size:var(--font-size-sm)" { (quest.description) }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { (quest.title) }

            (panel("Details", html! {
                div class="flex flex-col gap-sm" {
                    div class="detail-row" {
                        span class="detail-label" { "Location" }
                        a href=(format!("/settlements/{}", quest.settlement_id)) class="detail-value text-accent" {
                            (quest.settlement_id)
                        }
                    }
                    div class="detail-row" {
                        span class="detail-label" { "Difficulty" }
                        span class="detail-value" { (difficulty_stars(quest.difficulty)) }
                    }
                    div class="detail-row" {
                        span class="detail-label" { "Target" }
                        span class="detail-value" { (quest.enemy_count) " " (quest.enemy_type) }
                    }
                }
            }))

            (panel("Rewards", html! {
                div class="stat-grid" {
                    div class="stat-item" {
                        span class="stat-label" { "Gold" }
                        span class="stat-value" { (gold_display(quest.gold_reward)) }
                    }
                    div class="stat-item" {
                        span class="stat-label" { "Experience" }
                        span class="stat-value" { (xp_display(quest.xp_reward)) }
                    }
                }
            }))
        }

        aside class="right-sidebar" {
            (sidebar_section("Status", html! {
                (panel("", html! {
                    @match quest.status.to_lowercase().as_str() {
                        "available" => {
                            p style="font-size:var(--font-size-sm)" { "This quest is available." }
                            @if can_accept {
                                form action=(format!("/quests/{}/accept", quest.id)) method="post" class="mt-1" {
                                    button type="submit" class="btn btn-primary btn-block" { "Accept Quest" }
                                }
                            } @else {
                                p class="text-muted" style="font-size:var(--font-size-xs);margin-top:0.5rem" {
                                    "Must be a party leader at this settlement to accept."
                                }
                            }
                        }
                        "accepted" => {
                            p style="font-size:var(--font-size-sm)" { "Quest accepted." }
                            @if let Some(party_id) = &quest.accepted_by {
                                p class="text-muted" style="font-size:var(--font-size-xs)" {
                                    "Party: " (party_id)
                                }
                            }
                            @if is_party_quest {
                                form action=(format!("/quests/{}/abandon", quest.id)) method="post" class="mt-1" {
                                    button type="submit" class="btn btn-danger btn-block" { "Abandon Quest" }
                                }
                            }
                        }
                        "completed" => {
                            p class="text-success" style="font-size:var(--font-size-sm);font-weight:600" {
                                "Quest completed!"
                            }
                        }
                        _ => {
                            p style="font-size:var(--font-size-sm)" { "Status: " (quest.status) }
                        }
                    }
                }))
            }))
        }
    };

    base_layout_with_session(&quest.title, content, logged_in_as, theme)
}

/// Quest list fragment for Datastar updates
pub fn quests_list_fragment(quests: &[Quest]) -> Markup {
    html! {
        div #"quest-list" {
            @for quest in quests {
                (list_item(
                    &format!("/quests/{}", quest.id),
                    &quest.title,
                    Some(&quest.settlement_id),
                ))
            }
        }
    }
}
