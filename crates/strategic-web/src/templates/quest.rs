//! Quest templates

use maud::{Markup, html};

use super::{empty_state, item_type_header, item_type_icon, sidebar_section};
use crate::routes::travel::TravelDestination;
use crate::spacetimedb::{
    AutoresolveReport, BattleLootItem, InventoryQuantityTarget, ItemDefinition, PartyInventoryItem,
    Quest,
};
use crate::{
    spacetimedb::Character,
    templates::settlement::{
        map_destination_detail, map_destination_list, party_portrait_overlay,
        settlement_chat_area_with_info, travel_planner_bar, visual_stage,
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
            None,
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
            Some(travel_planner_bar(selected, 50)),
        ))
        (map_destination_detail(
            selected,
            can_travel,
            false,
            None,
            None,
            false,
            &format!("/locations/quest/{}/map", quest.id),
        ))
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
    travel_planner: Option<Markup>,
) -> Markup {
    let autoresolve_messages = autoresolve_info_messages(autoresolve_report);
    html! {
        main class="center-content settlement-main quest-location-main" {
            (party_portrait_overlay(
                party_members,
                active_character,
                &format!("/locations/quest/{}", quest.id),
                None,
                false,
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
                    } @else if autoresolve_report.is_some_and(|report| report.victor == "enemies") {
                        span class="badge badge-danger" { "Defeated — rest before trying again" }
                    } @else {
                        span class="badge badge-info" { "Waiting for party leader" }
                    }
                }
                }
            }
            @if let Some(travel_planner) = travel_planner { (travel_planner) }
            (settlement_chat_area_with_info(&quest.title, active_character, &autoresolve_messages))
        }
    }
}

fn autoresolve_info_messages(report: Option<&AutoresolveReport>) -> Vec<String> {
    let Some(report) = report else {
        return Vec::new();
    };
    let mut messages = Vec::with_capacity(report.log.len() + 1);
    messages.push(format!(
        "{} Victor: {}; seed {}.",
        report.summary, report.victor, report.seed
    ));
    messages.extend(report.log.iter().cloned());
    messages
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
                        thead { tr { (item_type_header()) th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in loot {
                                @let definition = items.iter().find(|item| item.id == entry.item_id);
                                @let value = definition.and_then(|item| item.base_value).unwrap_or(0);
                                @let current = pooled.iter().find(|pooled| pooled.item_id == entry.item_id).map_or(0, |pooled| pooled.quantity);
                                @let target = targets.iter().find(|target| target.item_id == entry.item_id).map_or(0, |target| target.quantity);
                                tr class="trade-inventory-row" data-loot-row data-count=(entry.quantity) data-current=(current) data-target=(target) {
                                    td class="inventory-item-type" { (item_type_icon(&entry.item_id)) }
                                    td { (super::settlement::item_name_with_quality(&entry.item_id, definition)) span class="inventory-row-actions" {
                                        button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-loot-stage=(entry.id) data-transfer-mode="one" data-label-one=(format!("Move one {}", entry.item_id)) data-label-target=(format!("Move {} to target", entry.item_id)) data-label-all=(format!("Move all {}", entry.item_id)) aria-label=(format!("Move one {}", entry.item_id)) title=(format!("Move one {}", entry.item_id)) { (super::settlement::transfer_glyph(1)) }
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
            None,
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
                        thead { tr { (item_type_header()) th { "Item" } th { "#" } th { "Value" } } }
                        tbody {
                            @for entry in pooled {
                                @let definition = items.iter().find(|item| item.id == entry.item_id);
                                @let value = definition.and_then(|item| item.base_value).unwrap_or(0);
                                tr {
                                    td class="inventory-item-type" { (item_type_icon(&entry.item_id)) }
                                    td { (super::settlement::item_name_with_quality(&entry.item_id, definition)) }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoresolve_report_becomes_complete_info_stream_rows() {
        let report = AutoresolveReport {
            quest_id: "quest-1".into(),
            party_id: "party-1".into(),
            seed: 42,
            victor: "players".into(),
            rounds: 3,
            summary: "3 rounds: 2 players against 3 enemies; players prevailed.".into(),
            log: vec!["Alice struck a bandit.".into(), "The bandit fell.".into()],
        };

        let messages = autoresolve_info_messages(Some(&report));
        assert_eq!(messages.len(), report.log.len() + 1);
        assert!(messages[0].contains(&report.summary));
        assert_eq!(&messages[1..], report.log.as_slice());

        let markup = settlement_chat_area_with_info("Bandit camp", None, &messages).into_string();
        assert_eq!(markup.matches("data-chat-channel=\"info\"").count(), 4);
        assert!(markup.contains("3 rounds: 2 players against 3 enemies; players prevailed."));
        assert!(!markup.contains("3 rounds; seed"));
        assert!(markup.contains("Alice struck a bandit."));
        assert!(markup.contains("The bandit fell."));
        assert!(!markup.contains("autoresolve-report"));
        assert!(!markup.contains("chat-channel-badge"));
    }
}
