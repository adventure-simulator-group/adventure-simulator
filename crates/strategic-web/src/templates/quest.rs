//! Quest templates

use maud::{Markup, html};

use super::inventory_browser::{InventoryBrowser, InventoryColumnSet};
use super::{empty_state, item_display_name, item_type_icon, sidebar_section};
use crate::routes::travel::TravelDestination;
use crate::spacetimedb::{
    AutoresolveReport, BattleLootItem, InventoryQuantityTarget, ItemDefinition, PartyInventoryItem,
    Quest,
};
use crate::{
    spacetimedb::Character,
    templates::settlement::{
        map_destination_detail, map_destination_list_with_rest, party_portrait_overlay,
        party_rest_menu, settlement_chat_area_with_info, travel_preferences_form, visual_stage,
    },
};

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
    party: Option<&crate::spacetimedb::Party>,
    can_configure_travel: bool,
    default_rest_minutes: u64,
    logged_in_as: Option<&str>,
) -> Markup {
    let selected = selected_id.and_then(|id| nearby.iter().find(|entry| entry.id == id));
    let content = html! {
        (map_destination_list_with_rest(
            nearby,
            selected_id,
            &format!("/locations/quest/{}/map", quest.id),
            html! {
                section class="rest-service-menu quest-rest-menu" aria-label="Destination rest" {
                    (party_rest_menu(
                        &format!("/locations/quest/{}/map/rest", quest.id),
                        "quest-map-rest",
                        "Rest before battle",
                        "Rest party",
                        default_rest_minutes,
                        None,
                    ))
                }
            },
        ))
        (quest_location_center(
            quest,
            active_character,
            party_members,
            can_fight,
            resolved,
            autoresolve_report,
            None,
            false,
        ))
        (map_destination_detail(
            selected,
            None,
            None,
            false,
            can_travel,
            false,
            None,
            party,
            can_configure_travel,
            None,
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
    show_combat_actions: bool,
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
                (visual_stage("quest", &quest.title, &quest.location_description))
                @if show_combat_actions && (can_fight || !resolved) {
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

/// Enemy encounter and, once resolved, its loot at an off-road quest location.
pub fn quest_location_enemy_page(
    quest: &Quest,
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
    autoresolve_report: Option<&AutoresolveReport>,
    party: Option<&crate::spacetimedb::Party>,
    can_configure_travel: bool,
    default_rest_minutes: u64,
    loot: &[BattleLootItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[ItemDefinition],
    targets: &[InventoryQuantityTarget],
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            @if !resolved {
                (sidebar_section("Location", html! { p { (&quest.location_description) } }))
                section class="rest-service-menu quest-rest-menu" aria-label="Destination rest" {
                    (party_rest_menu(
                        &format!("/locations/quest/{}/rest", quest.id),
                        "quest-rest",
                        "Rest before battle",
                        "Rest party",
                        default_rest_minutes,
                        None,
                    ))
                }
            } @else {
                (sidebar_section("Loot", html! {
                @if loot.is_empty() {
                    (empty_state(
                        if resolved { "No unclaimed loot remains." } else { "No loot has been recovered." },
                        None,
                        None,
                    ))
                } @else {
                    (InventoryBrowser { namespace: "quest-loot-left", show_quantities: true, show_equipped: false, show_condition: false, optional_columns: InventoryColumnSet::All, rows: html! {
                            @for entry in loot {
                                @let definition = items.iter().find(|item| item.id == entry.item_id);
                                @let value = definition.and_then(|item| item.base_value).unwrap_or(0);
                                @let current = pooled.iter().find(|pooled| pooled.item_id == entry.item_id).map_or(0, |pooled| pooled.quantity);
                                @let target = inventory_target(targets, &entry.item_id);
                                @let item_name = item_display_name(&entry.item_id);
                                tr class="trade-inventory-row" data-loot-row data-count=(entry.quantity) data-current=(current) data-target=(target) {
                                    td class="inventory-item-type" { (item_type_icon(&entry.item_id)) }
                                    td class="inventory-item-name" { (super::settlement::item_name_with_quality(&entry.item_id, definition)) span class="inventory-row-actions" {
                                        button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-loot-stage=(entry.id) data-transfer-mode="one" data-label-one=(format!("Move one {item_name}")) data-label-target=(format!("Move {item_name} to target")) data-label-all=(format!("Move all {item_name}")) aria-label=(format!("Move one {item_name}")) title=(format!("Move one {item_name}")) { (super::settlement::transfer_glyph(1)) }
                                    } }
                                    td class="inventory-count" { (entry.quantity) }
                                    td class="inventory-weight" { (definition.map_or_else(|| "—".to_string(), |item| item.weight.to_string())) }
                                    td class="inventory-gold" { (u64::from(value) * u64::from(entry.quantity)) }
                                }
                            }
                    }}.render())
                    (loot_stage_form(&quest.id))
                    (super::settlement::inventory_footer_controls("loot", "Move loot to targets", "Move all loot"))
                }
                }))
            }
        }

        (quest_location_center(
            quest,
            active_character,
            party_members,
            can_fight,
            resolved,
            autoresolve_report,
            None,
            true,
        ))

        aside class=(if resolved { "right-sidebar" } else { "right-sidebar travel-preferences-only-sidebar" })
            aria-label=(if resolved { "Party inventory" } else { "Location details" }) {
            @if !resolved {
                @if let Some(party) = party.filter(|_| can_configure_travel) {
                    (sidebar_section(
                        "Travel preferences",
                        travel_preferences_form(
                            party,
                            &format!("/locations/quest/{}/map/travel-configuration", quest.id),
                        ),
                    ))
                }
            } @else {
                (sidebar_section("Party inventory", html! {
                div class="party-stake-summary" {
                    span { "Your available stake" }
                    strong { (stake) " coin" }
                }
                @if pooled.is_empty() {
                    (empty_state("The party chest is empty.", None, None))
                } @else {
                    (InventoryBrowser { namespace: "quest-party-right", show_quantities: true, show_equipped: false, show_condition: false, optional_columns: InventoryColumnSet::All, rows: html! {
                            @for entry in pooled {
                                @let definition = items.iter().find(|item| item.id == entry.item_id);
                                @let value = definition.and_then(|item| item.base_value).unwrap_or(0);
                                @let target = inventory_target(targets, &entry.item_id);
                                tr class="trade-inventory-row" data-target=(target) {
                                    td class="inventory-item-type" { (item_type_icon(&entry.item_id)) }
                                    td class="inventory-item-name" { (super::settlement::item_name_with_quality(&entry.item_id, definition)) }
                                    td class="inventory-count" { (entry.quantity) }
                                    td class="inventory-weight" { (definition.map_or_else(|| "—".to_string(), |item| item.weight.to_string())) }
                                    td class="inventory-gold" { (u64::from(value) * u64::from(entry.quantity)) }
                                }
                            }
                    }}.render())
                }
                }))
            }
        }
    };
    super::quest_location_layout_with_session(
        &quest.title,
        &quest.title,
        &quest.id,
        "enemy",
        content,
        logged_in_as,
    )
}

fn inventory_target(targets: &[InventoryQuantityTarget], item_id: &str) -> u32 {
    targets
        .iter()
        .find(|target| target.item_id == item_id)
        .map_or(0, |target| target.quantity)
}

fn loot_stage_form(quest_id: &str) -> Markup {
    html! {
        form method="post" action=(format!("/quests/{quest_id}/loot/store")) id="loot-transfer-offer" class="party-offer loot-transfer-offer" hidden
            role="dialog" aria-modal="true" aria-label="Confirm collected loot" tabindex="-1" {
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

    #[test]
    fn quest_party_rows_use_the_matching_inventory_target() {
        let targets = [InventoryQuantityTarget {
            id: "7:true:sword".into(),
            owner_character_id: 7,
            party_scope: true,
            item_id: "sword".into(),
            quantity: 4,
        }];
        assert_eq!(inventory_target(&targets, "sword"), 4);
        assert_eq!(inventory_target(&targets, "shield"), 0);
    }
}
