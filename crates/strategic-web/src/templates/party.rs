//! Party templates

use maud::{html, Markup};

use super::{base_layout_with_session, card, empty_state, input_field, list_item, loading_indicator};
use crate::spacetimedb::{Character, Party, PartyMember, Quest};

/// List all parties (optionally filtered by settlement)
pub fn parties_list_page(parties: &[Party], settlement_filter: Option<&str>, logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="parties-page" {
            div class="page-header" {
                h2 { "Parties" }
                a href="/parties/new" class="btn btn-primary" {
                    "Create Party"
                }
            }

            @if let Some(settlement) = settlement_filter {
                p class="filter-note" { "Showing parties in " (settlement) }
            }

            @if parties.is_empty() {
                (empty_state(
                    "No parties found. Be the first to form one!",
                    Some("/parties/new"),
                    Some("Create Party")
                ))
            } @else {
                div #"party-list" class="list" {
                    @for party in parties {
                        (list_item(
                            &format!("/parties/{}", party.id),
                            &party.name,
                            party.active_quest_id.as_ref().map(|_| "On a quest")
                        ))
                    }
                }
            }
        }
    };

    base_layout_with_session("Parties", content, logged_in_as)
}

/// Party creation form
pub fn party_new_page(logged_in_as: Option<&str>) -> Markup {
    let content = html! {
        div class="party-new-page" {
            h2 { "Create New Party" }

            (card("Party Details", html! {
                form #"party-form" data-on-submit="@post('/parties')" {
                    (input_field("name", "Party Name", "text", true, None))

                    div class="form-actions" {
                        button type="submit" class="btn btn-primary" {
                            "Create Party"
                        }
                        (loading_indicator("create-loading"))
                    }
                }
            }))
        }
    };

    base_layout_with_session("Create Party", content, logged_in_as)
}

/// Party detail page
pub fn party_detail_page(
    party: &Party,
    members: &[(PartyMember, Option<Character>)],
    active_quest: Option<&Quest>,
    is_leader: bool,
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        div class="party-detail-page" {
            div class="page-header" {
                h2 { (party.name) }
                @if party.active_quest_id.is_some() {
                    span class="badge badge-warning" { "On Quest" }
                }
            }

            div class="party-grid" {
                // Members card
                (card("Members", html! {
                    ul class="member-list" {
                        @for (member, character) in members {
                            li class="member-item" {
                                @if let Some(char) = character {
                                    span class="member-name" {
                                        (char.name)
                                        @if member.character_id == party.leader_id {
                                            span class="leader-badge" { " (Leader)" }
                                        }
                                    }
                                    span class="member-level" { "Level " (char.level) }
                                } @else {
                                    span class="member-name" { (member.character_id) }
                                }
                                @if let Some(role) = &member.role {
                                    span class="member-role" { (role) }
                                }
                            }
                        }
                    }
                }))

                // Location card
                (card("Location", html! {
                    @if let Some(settlement_id) = &party.current_settlement_id {
                        p { "Currently at: " }
                        a href=(format!("/settlements/{}", settlement_id)) class="btn btn-secondary" {
                            (settlement_id)
                        }
                    } @else {
                        p { "Traveling..." }
                    }
                }))

                // Quest card
                (card("Active Quest", html! {
                    @if let Some(quest) = active_quest {
                        div class="quest-summary" {
                            h4 { (quest.title) }
                            p { (quest.description) }
                            p class="quest-target" {
                                "Target: " (quest.enemy_count) " " (quest.enemy_type)
                            }
                        }
                        a href=(format!("/quests/{}", quest.id)) class="btn btn-secondary" {
                            "View Quest"
                        }
                        @if is_leader {
                            form data-on-submit=(format!("@post('/quests/{}/abandon')", quest.id)) {
                                button type="submit" class="btn btn-danger" {
                                    "Abandon Quest"
                                }
                            }
                        }
                    } @else {
                        p { "No active quest" }
                        @if is_leader {
                            @if let Some(settlement_id) = &party.current_settlement_id {
                                a href=(format!("/settlements/{}/noticeboard", settlement_id)) class="btn btn-primary" {
                                    "Find Quest"
                                }
                            }
                        }
                    }
                }))

                // Actions card
                (card("Actions", html! {
                    div class="party-actions" {
                        @if is_leader {
                            form data-on-submit=(format!("@post('/parties/{}/disband')", party.id)) {
                                button type="submit" class="btn btn-danger" {
                                    "Disband Party"
                                }
                            }
                        } @else {
                            form data-on-submit=(format!("@post('/parties/{}/leave')", party.id)) {
                                button type="submit" class="btn btn-secondary" {
                                    "Leave Party"
                                }
                            }
                        }
                    }
                }))
            }

            // Enter mission section
            @if active_quest.is_some() && is_leader {
                (card("Start Mission", html! {
                    p { "Ready to face the enemy? Start a tactical mission!" }
                    form data-on-submit="@post('/missions/enter')" {
                        button type="submit" class="btn btn-primary btn-large" {
                            "Enter Mission"
                        }
                    }
                }))
            }
        }
    };

    base_layout_with_session(&party.name, content, logged_in_as)
}

/// Party list fragment for Datastar updates
pub fn parties_list_fragment(parties: &[Party]) -> Markup {
    html! {
        div #"party-list" class="list" {
            @for party in parties {
                (list_item(
                    &format!("/parties/{}", party.id),
                    &party.name,
                    party.active_quest_id.as_ref().map(|_| "On a quest")
                ))
            }
        }
    }
}
