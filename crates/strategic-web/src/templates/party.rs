//! Party templates

use maud::{Markup, html};

use super::{
    base_layout_with_session, divider, empty_state, input_field, list_item, panel, sidebar_section,
};
use crate::spacetimedb::{Character, Party, PartyMember, Quest};

/// List all parties
pub fn parties_list_page(
    parties: &[Party],
    settlement_filter: Option<&str>,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Parties", html! {
                @if parties.is_empty() {
                    p class="text-muted" style="font-size:var(--font-size-sm)" {
                        "No parties found"
                    }
                } @else {
                    div # "party-list" {
                        @for party in parties {
                            (list_item(
                                &format!("/parties/{}", party.id),
                                &party.name,
                                party.active_quest_id.as_ref().map(|_| "On a quest"),
                            ))
                        }
                    }
                }
            }))

            (divider())

            a href="/parties/new" class="btn btn-primary btn-block" { "Create Party" }
        }

        main class="center-content" {
            h2 class="page-title" { "Parties" }

            @if let Some(settlement) = settlement_filter {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" { "Showing parties in " strong { (settlement) } }
                }))
            }

            @if parties.is_empty() {
                (empty_state(
                    "No parties found. Be the first to form one!",
                    Some("/parties/new"),
                    Some("Create Party"),
                ))
            } @else {
                @for party in parties {
                    a href=(format!("/parties/{}", party.id)) class="quest-card" {
                        div class="quest-card-header" {
                            span class="quest-card-title" { (party.name) }
                            @if party.active_quest_id.is_some() {
                                span class="badge badge-warning" { "On Quest" }
                            }
                        }
                        @if let Some(settlement_id) = &party.current_settlement_id {
                            p class="quest-card-desc" { "Located at " (settlement_id) }
                        }
                    }
                }
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Info", html! {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" {
                        "Join a party to take on quests together. "
                        "Only the party leader can accept quests and start missions."
                    }
                }))
            }))
        }
    };

    base_layout_with_session("Parties", content, logged_in_as, theme)
}

/// Party creation form
pub fn party_new_page(logged_in_as: Option<&str>, theme: &str) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Tips", html! {
                (panel("", html! {
                    p style="font-size:var(--font-size-sm)" {
                        "As party leader, you can accept quests and start tactical missions. "
                        "Other players can join your party at the same settlement."
                    }
                }))
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Create Party" }
            (panel("Party Details", html! {
                form # "party-form" action="/parties" method="post" {
                    (input_field("name", "Party Name", "text", true, None))
                    div class="form-actions" {
                        button type="submit" class="btn btn-primary" { "Create Party" }
                        a href="/parties" class="btn btn-secondary" { "Cancel" }
                    }
                }
            }))
        }

        aside class="right-sidebar" {}
    };

    base_layout_with_session("Create Party", content, logged_in_as, theme)
}

/// Party detail page
pub fn party_detail_page(
    party: &Party,
    members: &[(PartyMember, Option<Character>)],
    active_quest: Option<&Quest>,
    is_leader: bool,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Members", html! {
                @for (member, character) in members {
                    div class="member-item" {
                        div {
                            @if let Some(ch) = character {
                                span class="member-name" {
                                    (ch.name)
                                    @if member.character_id == party.leader_id {
                                        span class="leader-badge" { " (Leader)" }
                                    }
                                }
                            } @else {
                                span class="member-name" { (member.character_id) }
                            }
                        }
                        @if let Some(role) = &member.role {
                            span class="member-role" { (role) }
                        }
                    }
                }
            }))

            (divider())

            @if let Some(settlement_id) = &party.current_settlement_id {
                (sidebar_section("Location", html! {
                    a href=(format!("/settlements/{}", settlement_id)) class="btn btn-secondary btn-block btn-small" {
                        (settlement_id)
                    }
                }))
            } @else {
                (sidebar_section("Location", html! {
                    p class="text-muted" style="font-size:var(--font-size-sm)" { "Traveling..." }
                }))
            }
        }

        main class="center-content" {
            div class="flex items-center justify-between mb-2" {
                h2 class="page-title" style="margin-bottom:0;flex:1" { (party.name) }
                @if party.active_quest_id.is_some() {
                    span class="badge badge-warning" { "On Quest" }
                }
            }

            @if let Some(quest) = active_quest {
                (panel("Active Quest", html! {
                    h4 style="font-family:var(--font-display);margin-bottom:0.25rem" { (quest.title) }
                    p style="font-size:var(--font-size-sm)" { (quest.description) }
                    p class="quest-card-enemy mt-1" {
                        "Target: " (quest.enemy_count) " " (quest.enemy_type)
                    }
                    div class="flex gap-sm mt-1" {
                        a href=(format!("/quests/{}", quest.id)) class="btn btn-secondary btn-small" {
                            "View Quest"
                        }
                        @if is_leader {
                            form action=(format!("/quests/{}/abandon", quest.id)) method="post" {
                                button type="submit" class="btn btn-danger btn-small" { "Abandon" }
                            }
                        }
                    }
                }))
            } @else {
                (panel("Quest", html! {
                    p class="text-muted" { "No active quest" }
                    @if is_leader {
                        @if let Some(settlement_id) = &party.current_settlement_id {
                            a href=(format!("/settlements/{}/noticeboard", settlement_id)) class="btn btn-primary btn-small mt-1" {
                                "Find Quest"
                            }
                        } @else {
                            a href="/settlements" class="btn btn-primary btn-small mt-1" {
                                "Choose Destination"
                            }
                        }
                    }
                }))
            }

            // Enter mission section
            @if active_quest.is_some() && is_leader {
                (panel("Start Mission", html! {
                    p style="font-size:var(--font-size-sm)" { "Ready to face the enemy? Start a tactical mission!" }
                    form action="/missions/enter" method="post" class="mt-1" {
                        button type="submit" class="btn btn-primary btn-large btn-block" {
                            "Enter Mission"
                        }
                    }
                }))
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Actions", html! {
                div class="flex flex-col gap-sm" {
                    @if is_leader {
                        form action=(format!("/parties/{}/disband", party.id)) method="post" {
                            button type="submit" class="btn btn-danger btn-block" { "Disband Party" }
                        }
                    } @else {
                        form action=(format!("/parties/{}/leave", party.id)) method="post" {
                            button type="submit" class="btn btn-secondary btn-block" { "Leave Party" }
                        }
                    }
                }
            }))
        }
    };

    base_layout_with_session(&party.name, content, logged_in_as, theme)
}

/// Party list fragment for Datastar updates
pub fn parties_list_fragment(parties: &[Party]) -> Markup {
    html! {
        div # "party-list" {
            @for party in parties {
                (list_item(
                    &format!("/parties/{}", party.id),
                    &party.name,
                    party.active_quest_id.as_ref().map(|_| "On a quest"),
                ))
            }
        }
    }
}
