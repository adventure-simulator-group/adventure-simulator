//! Quest templates

use maud::{Markup, html};

use super::{empty_state, sidebar_section};
use crate::routes::travel::TravelDestination;
use crate::spacetimedb::{
    AutoresolveReport, BattleLootItem, InventoryQuantityTarget, ItemDefinition, PartyInventoryItem,
    Quest,
};
use crate::{
    spacetimedb::Character,
    templates::settlement::{
        map_destination_detail, map_destination_list, party_portrait_overlay, settlement_chat_area,
        visual_stage,
    },
};

pub fn quest_location_base_page(
    quest: &Quest,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
    autoresolve_report: Option<&AutoresolveReport>,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Location", html! { p { (&quest.location_description) } }))
        }
        (quest_location_center(
            quest,
            active_character,
            party_members,
            can_fight,
            resolved,
            autoresolve_report,
        ))
        aside class="right-sidebar" aria-label="Location details" {}
    };
    super::quest_location_layout_with_session(
        &quest.title,
        &quest.title,
        &quest.id,
        "",
        content,
        logged_in_as,
        theme,
    )
}

pub fn quest_location_map_page(
    quest: &Quest,
    nearby: &[TravelDestination],
    selected_id: Option<&str>,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_travel: bool,
    can_fight: bool,
    resolved: bool,
    autoresolve_report: Option<&AutoresolveReport>,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let selected = selected_id.and_then(|id| nearby.iter().find(|entry| entry.id == id));
    let content = html! {
        (map_destination_list(
            nearby,
            selected_id,
            &format!("/locations/quest/{}/map", quest.id),
        ))
        (quest_location_center(
            quest,
            active_character,
            party_members,
            can_fight,
            resolved,
            autoresolve_report,
        ))
        (map_destination_detail(selected, can_travel, false, None))
    };
    super::quest_location_layout_with_session(
        &format!("{} map", quest.title),
        &quest.title,
        &quest.id,
        "map",
        content,
        logged_in_as,
        theme,
    )
}

fn quest_location_center(
    quest: &Quest,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
    autoresolve_report: Option<&AutoresolveReport>,
) -> Markup {
    html! {
        main class="center-content settlement-main quest-location-main" {
            (party_portrait_overlay(
                party_members,
                active_character,
                &format!("/locations/quest/{}", quest.id),
                None,
            ))
            div class="quest-visual-wrap" {
                (visual_stage("map", &quest.title, "TODO: quest location image"))
                @if can_fight || !resolved {
                div class="quest-combat-actions" aria-label="Quest actions" {
                    @if can_fight {
                        form action="/missions/enter" method="post" {
                            button type="submit" class="btn btn-danger" { "Initiate Combat" }
                        }
                        form action=(format!("/quests/{}/autoresolve", quest.id)) method="post" {
                            button type="submit" class="btn btn-primary" { "Autoresolve" }
                        }
                    } @else {
                        span class="badge badge-info" { "Waiting for party leader" }
                    }
                }
                }
            }
            @if let Some(report) = autoresolve_report {
                section class="autoresolve-report" aria-label="Autoresolve report" {
                    h2 { "Combat summary" }
                    p {
                        strong { (report.victor) }
                        " - Seed " code { (report.seed) }
                    }
                    p { (&report.summary) }
                    details {
                        summary { "Combat log (" (report.log.len()) " exchanges)" }
                        ol {
                            @for entry in &report.log {
                                li { (entry) }
                            }
                        }
                    }
                }
            }
            (settlement_chat_area(&quest.title, active_character))
        }
    }
}

/// Loot and shared inventory at an off-road quest location.
pub fn quest_location_page(
    quest: &Quest,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
    autoresolve_report: Option<&AutoresolveReport>,
    loot: &[BattleLootItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[ItemDefinition],
    targets: &[InventoryQuantityTarget],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Loot", html! {
                @if loot.is_empty() {
                    (empty_state(
                        if resolved { "No unclaimed loot remains." } else { "No loot has been recovered." },
                        None,
                        None,
                    ))
                } @else {
                    table class="trade-inventory-table" {
                        thead { tr { th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in loot {
                                @let value = items.iter().find(|item| item.id == entry.item_id).and_then(|item| item.base_value).unwrap_or(0);
                                @let current = pooled.iter().find(|pooled| pooled.item_id == entry.item_id).map_or(0, |pooled| pooled.quantity);
                                @let target = targets.iter().find(|target| target.item_id == entry.item_id).map_or(0, |target| target.quantity);
                                tr class="trade-inventory-row" data-loot-row data-count=(entry.quantity) data-current=(current) data-target=(target) {
                                    td { (&entry.item_id) span class="inventory-row-actions" {
                                        @for (mode, arrows) in [("one",1),("target",2),("all",3)] {
                                            button type="button" class="trade-transfer trade-transfer-right" data-loot-stage=(entry.id) data-transfer-mode=(mode) aria-label=(format!("Move {} loot", entry.item_id)) { (super::settlement::transfer_glyph(arrows)) }
                                        }
                                    } }
                                    td class="inventory-count" { (entry.quantity) }
                                    td { (u64::from(value) * u64::from(entry.quantity)) }
                                }
                            }
                        }
                    }
                    (loot_stage_form(&quest.id))
                    (super::settlement::inventory_footer_controls("loot", "Move loot to targets", "Move all loot"))
                }
            }))
        }

        (quest_location_center(
            quest,
            active_character,
            party_members,
            can_fight,
            resolved,
            autoresolve_report,
        ))

        aside class="right-sidebar" {
            (sidebar_section("Party inventory", html! {
                div class="party-stake-summary" {
                    span { "Your available stake" }
                    strong { (stake) " gold" }
                }
                @if pooled.is_empty() {
                    (empty_state("The party chest is empty.", None, None))
                } @else {
                    table class="trade-inventory-table" {
                        thead { tr { th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in pooled {
                                @let value = items.iter().find(|item| item.id == entry.item_id).and_then(|item| item.base_value).unwrap_or(0);
                                tr {
                                    td { (&entry.item_id) }
                                    td { (entry.quantity) }
                                    td { (u64::from(value) * u64::from(entry.quantity)) }
                                }
                            }
                        }
                    }
                }
            }))
        }
    };
    super::quest_location_layout_with_session(
        &quest.title,
        &quest.title,
        &quest.id,
        "loot",
        content,
        logged_in_as,
        theme,
    )
}

fn loot_stage_form(quest_id: &str) -> Markup {
    html! {
        form method="post" action=(format!("/quests/{quest_id}/loot/store")) id="loot-transfer-offer" class="party-offer loot-transfer-offer" hidden {
            span class="loot-transfer-prompt" data-loot-transfer-prompt { "Apply staged loot to the party inventory?" }
            button type="button" class="party-offer-cancel" data-cancel-loot { "Cancel" }
            button type="submit" disabled { "Apply" }
        }
    }
}
