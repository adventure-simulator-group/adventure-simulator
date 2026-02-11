//! Quest templates

use maud::{html, Markup};

use super::{
    base_layout_with_session, card, difficulty_stars, empty_state, gold_display, list_item,
    status_badge, xp_display,
};
use crate::spacetimedb::Quest;

/// List all quests
pub fn quests_list_page(quests: &[Quest], logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="quests-page" {
            div class="page-header" {
                h2 { "All Quests" }
            }

            // Filter tabs
            div class="filter-tabs" {
                button class="filter-tab active" { "All" }
                button class="filter-tab" { "Available" }
                button class="filter-tab" { "Accepted" }
                button class="filter-tab" { "Completed" }
            }

            @if quests.is_empty() {
                (empty_state("No quests available.", None, None))
            } @else {
                div #"quest-list" class="quest-list" {
                    @for quest in quests {
                        (quest_list_item(quest))
                    }
                }
            }
        }
    };

    base_layout_with_session("Quests", content, logged_in_as)
}

fn quest_list_item(quest: &Quest) -> Markup {
    html! {
        a href=(format!("/quests/{}", quest.id)) class="quest-list-item" {
            div class="quest-header" {
                h3 { (quest.title) }
                (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
            }
            div class="quest-meta" {
                span class="settlement" { (quest.settlement_id) }
                span class="difficulty" {
                    (difficulty_stars(quest.difficulty))
                }
            }
            div class="quest-rewards" {
                (gold_display(quest.gold_reward))
                " / "
                (xp_display(quest.xp_reward))
            }
        }
    }
}

/// Quest detail page
pub fn quest_detail_page(
    quest: &Quest,
    can_accept: bool,
    is_party_quest: bool,
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        div class="quest-detail-page" {
            div class="page-header" {
                h2 { (quest.title) }
                (status_badge(&format!("{:?}", quest.status).replace("\"", "")))
            }

            div class="quest-grid" {
                // Description card
                (card("Description", html! {
                    p { (quest.description) }
                }))

                // Details card
                (card("Details", html! {
                    div class="detail-grid" {
                        div class="detail" {
                            span class="detail-label" { "Location" }
                            a href=(format!("/settlements/{}", quest.settlement_id)) class="detail-value" {
                                (quest.settlement_id)
                            }
                        }
                        div class="detail" {
                            span class="detail-label" { "Difficulty" }
                            span class="detail-value" {
                                (difficulty_stars(quest.difficulty))
                            }
                        }
                        div class="detail" {
                            span class="detail-label" { "Target" }
                            span class="detail-value" {
                                (quest.enemy_count) " " (quest.enemy_type)
                            }
                        }
                    }
                }))

                // Rewards card
                (card("Rewards", html! {
                    div class="rewards-grid" {
                        div class="reward" {
                            span class="reward-label" { "Gold" }
                            span class="reward-value" { (gold_display(quest.gold_reward)) }
                        }
                        div class="reward" {
                            span class="reward-label" { "Experience" }
                            span class="reward-value" { (xp_display(quest.xp_reward)) }
                        }
                    }
                }))

                // Status card
                (card("Status", html! {
                    @match quest.status.to_lowercase().as_str() {
                        "available" => {
                            p { "This quest is available to accept." }
                            @if can_accept {
                                form data-on-submit=(format!("@post('/quests/{}/accept')", quest.id)) {
                                    button type="submit" class="btn btn-primary" {
                                        "Accept Quest"
                                    }
                                }
                            } @else {
                                p class="warning" { "You must be a party leader at this settlement to accept quests." }
                            }
                        }
                        "accepted" => {
                            p { "This quest has been accepted." }
                            @if let Some(party_id) = &quest.accepted_by {
                                p { "Accepted by party: " (party_id) }
                            }
                            @if is_party_quest {
                                form data-on-submit=(format!("@post('/quests/{}/abandon')", quest.id)) {
                                    button type="submit" class="btn btn-danger" {
                                        "Abandon Quest"
                                    }
                                }
                            }
                        }
                        "completed" => {
                            p class="success" { "This quest has been completed!" }
                        }
                        _ => {
                            p { "Status: " (quest.status) }
                        }
                    }
                }))
            }
        }
    };

    base_layout_with_session(&quest.title, content, logged_in_as)
}

/// Quest list fragment for Datastar updates
pub fn quests_list_fragment(quests: &[Quest]) -> Markup {
    html! {
        div #"quest-list" class="quest-list" {
            @for quest in quests {
                (quest_list_item(quest))
            }
        }
    }
}
