//! Quest templates

use maud::{Markup, html};

use super::inventory_browser::{InventoryBrowser, InventoryColumnSet};
use super::{empty_state, item_display_name, item_type_icon, sidebar_section};
use crate::routes::travel::TravelDestination;
use crate::spacetimedb::{
    AutoresolveReport, BackendCaseSitePin, BackendCorpse, BackendInvestigationAction,
    BattleLootItem, FoodLot, InventoryQuantityTarget, ItemDefinition, PartyInventoryItem,
};
use crate::{
    spacetimedb::Character,
    templates::settlement::{
        map_destination_detail, map_destination_list_with_rest, party_portrait_overlay,
        party_rest_menu, settlement_chat_area_with_info, travel_preferences_form,
    },
};

pub struct CaseSitePagePresentation {
    pub title: String,
    pub action_id: String,
    pub allow_tactical_combat: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseSiteRecoveryNotice {
    pub member_names: String,
    pub causes: String,
    pub resource_blocked: bool,
    pub withdrawal_destination: String,
    pub withdrawal_href: String,
}

pub fn quest_location_map_page(
    presentation: &CaseSitePagePresentation,
    site: &BackendCaseSitePin,
    onsite_actions: &[BackendInvestigationAction],
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
    soap_preview: super::settlement::SoapRestPreview,
    recovery_notice: Option<&CaseSiteRecoveryNotice>,
    logged_in_as: Option<&str>,
    corpses: &[BackendCorpse],
    selected_corpse: Option<(&BackendCorpse, &str)>,
) -> Markup {
    let selected = selected_id.and_then(|id| nearby.iter().find(|entry| entry.id == id));
    let content = html! {
        (map_destination_list_with_rest(
            nearby,
            selected_id,
            &format!("/locations/case-site/{}/map", site.case_site_id),
            html! {
                @if !resolved {
                section class="rest-service-menu quest-rest-menu" aria-label="Destination rest" {
                    (party_rest_menu(
                        &format!("/locations/case-site/{}/map/rest", site.case_site_id),
                        "quest-map-rest",
                        "Rest before battle",
                        "Rest party",
                        default_rest_minutes,
                        None,
                        soap_preview,
                    ))
                }
                }
            },
        ))
        (quest_location_center(
            presentation,
            site,
            onsite_actions,
            active_character,
            party_members,
            can_fight,
            resolved,
            autoresolve_report,
            None,
            false,
            recovery_notice,
            true,
            corpses,
            selected_corpse,
        ))
        (map_destination_detail(
            selected,
            None,
            false,
            can_travel,
            None,
            None,
            party,
            can_configure_travel,
            None,
            &format!("/locations/case-site/{}/map", site.case_site_id),
        ))
    };
    super::quest_location_layout_with_session(
        &format!("{} map", presentation.title),
        &presentation.title,
        &site.case_site_id,
        "map",
        content,
        logged_in_as,
    )
}

fn quest_location_center(
    presentation: &CaseSitePagePresentation,
    site: &BackendCaseSitePin,
    onsite_actions: &[BackendInvestigationAction],
    active_character: Option<&Character>,
    party_members: &[Character],
    can_fight: bool,
    resolved: bool,
    autoresolve_report: Option<&AutoresolveReport>,
    travel_planner: Option<Markup>,
    show_combat_actions: bool,
    recovery_notice: Option<&CaseSiteRecoveryNotice>,
    map_tab: bool,
    corpses: &[BackendCorpse],
    selected_corpse: Option<(&BackendCorpse, &str)>,
) -> Markup {
    let autoresolve_messages = autoresolve_info_messages(autoresolve_report);
    html! {
        main class="center-content settlement-main quest-location-main" {
            @if resolved {
                section class="strategic-notice quest-complete-notice" role="status" {
                    h3 { "Quest complete" }
                    p { "The local problem has been resolved." }
                }
            }
            @if let Some(notice) = recovery_notice {
                section class="strategic-notice quest-recovery-notice" role="status" {
                    h3 { "Party recovery" }
                    p {
                        (&notice.member_names) " "
                        @if notice.causes.is_empty() {
                            "cannot currently act."
                        } @else {
                            "cannot currently act because of " (&notice.causes) "."
                        }
                    }
                    @if notice.resource_blocked {
                        p {
                            "Field rest does not provide food or water. Resting longer may worsen these deficits."
                        }
                    } @else {
                        p {
                            "Field rest can reduce fatigue and permit natural recovery, but severe conditions may require settlement care."
                        }
                    }
                    p {
                        "An incapacitated party may still withdraw to a settlement. The journey costs time and carries normal travel risk; supplies and care become available after arrival."
                    }
                    p {
                        a class="btn btn-secondary" href=(&notice.withdrawal_href) {
                            @if map_tab {
                                "Select " (&notice.withdrawal_destination) " and begin journey"
                            } @else {
                                "Open map and select " (&notice.withdrawal_destination)
                            }
                        }
                    }
                }
            }
            (party_portrait_overlay(
                party_members,
                active_character,
                &format!("/locations/case-site/{}", site.case_site_id),
                None,
                false,
            ))
            nav class="settlement-npc-strip physical-evidence-strip"
                aria-label="Physical evidence here"
                data-evidence-strip
                data-evidence-case-site=(&site.case_site_id) {
                span class="text-muted" data-evidence-loading { "Looking over the scene…" }
            }
            @if !corpses.is_empty() {
                nav class="settlement-npc-strip corpse-strip" aria-label="Counterparty corpses" {
                    @for corpse in corpses {
                        @let corpse_label = if corpse.location == "interred" { "Buried body" } else { &corpse.display_name };
                        a class="npc-portrait corpse-portrait"
                            href=(format!("/locations/case-site/{}/enemy?corpse={}&medical=physiology", site.case_site_id, corpse.corpse_id))
                            aria-label=(format!("Examine {corpse_label} with Physiology")) {
                            span class="npc-portrait-image" aria-hidden="true" { "☠" }
                            span class="npc-portrait-name" { (corpse_label) }
                        }
                    }
                }
                @if let Some((corpse, _)) = selected_corpse {
                    div class="quest-combat-actions corpse-medical-actions" aria-label="Corpse medical windows" {
                        a class="btn btn-secondary" href=(format!("/locations/case-site/{}/enemy?corpse={}&medical=physiology", site.case_site_id, corpse.corpse_id)) { "Physiology" }
                        a class="btn btn-secondary" href=(format!("/locations/case-site/{}/enemy?corpse={}&medical=surgery", site.case_site_id, corpse.corpse_id)) { "Surgery" }
                    }
                }
            }
            div class="quest-visual-wrap" {
                section class="visual-stage npc-description-stage evidence-description-stage"
                    data-evidence-description aria-live="polite" {
                    div class="visual-stage-placeholder" aria-hidden="true" { "?" }
                    h2 { (&site.name) }
                    p { (&site.description) }
                }
                @if show_combat_actions
                    && (can_fight
                        || (presentation.allow_tactical_combat && !resolved)
                        || autoresolve_report.is_some_and(|report| report.victor == "enemies")) {
                div class="quest-combat-actions" aria-label="Quest actions" {
                    @if can_fight {
                        @if presentation.allow_tactical_combat {
                            form action="/missions/enter" method="post" {
                                button type="submit" class="btn btn-danger" { "Initiate Combat" }
                            }
                        }
                        form action=(format!("/quests/{}/autoresolve", site.case_site_id)) method="post" {
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
            @if !onsite_actions.is_empty() {
                section class="quest-onsite-investigation" aria-label="Onsite investigation" {
                    h3 { "Investigate here" }
                    @for action in onsite_actions {
                        form method="post" action="/quests/actions" {
                            input type="hidden" name="action_id" value=(&action.action_id);
                            input type="hidden" name="method" value=(&action.method);
                            input type="hidden" name="expected_version" value=(action.expected_version);
                            button type="submit" class="btn btn-secondary btn-small" { (&action.summary) }
                        }
                    }
                }
            }
            @if let Some(travel_planner) = travel_planner { (travel_planner) }
            (settlement_chat_area_with_info(&presentation.title, active_character, &autoresolve_messages))
        }
        @if let Some((corpse, window)) = selected_corpse {
            (super::settlement::corpse_medical_dialog(
                corpse,
                &format!("/locations/case-site/{}/enemy", site.case_site_id),
                window,
            ))
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

#[derive(Clone, Debug)]
pub struct QuestCounterparty {
    pub character: Character,
    pub contact_ref: String,
    pub revision: u32,
    pub membership_revision: u32,
    pub contact_decision: crate::spacetimedb::BackendContextualDecision,
    pub treatment_decision: crate::spacetimedb::BackendContextualDecision,
    pub treatment_limb_slug: Option<String>,
}

fn quest_counterparty_strip(case_site_id: &str, counterparties: &[QuestCounterparty]) -> Markup {
    html! {
        nav class="settlement-npc-strip counterparty-strip" aria-label="Counterparty" {
            @for counterparty in counterparties {
                div class="npc-portrait counterparty-portrait" {
                    span class="npc-portrait-image" aria-hidden="true" { "?" }
                    span class="npc-portrait-name" { (&counterparty.character.name) }
                    @if counterparty.contact_decision == crate::spacetimedb::BackendContextualDecision::Request {
                      form method="post" action=(format!("/locations/case-site/{case_site_id}/counterparty/contact")) {
                        input type="hidden" name="target_id" value=(counterparty.character.id);
                        input type="hidden" name="contact_ref" value=(&counterparty.contact_ref);
                        input type="hidden" name="expected_revision" value=(counterparty.revision);
                        input type="hidden" name="action_id" value=(format!("quest-contact:{case_site_id}:{}:{}", counterparty.revision, counterparty.character.id));
                        button type="submit" class="btn btn-secondary btn-small" { "Request" }
                      }
                    } @else {
                      button type="button" class="btn btn-secondary btn-small" disabled {
                        (if counterparty.contact_decision == crate::spacetimedb::BackendContextualDecision::Refused { "Refused" } else { "Unavailable" })
                      }
                    }
                    @if counterparty.character.alive && counterparty.treatment_limb_slug.is_some() && matches!(counterparty.treatment_decision,
                        crate::spacetimedb::BackendContextualDecision::Request
                        | crate::spacetimedb::BackendContextualDecision::EmergencyTreatment) {
                        form method="post" action=(format!("/locations/case-site/{case_site_id}/counterparty/bandage")) {
                            input type="hidden" name="patient_id" value=(counterparty.character.id);
                            input type="hidden" name="limb_slug" value=(counterparty.treatment_limb_slug.as_deref().unwrap_or_default());
                            input type="hidden" name="action_id" value=(crate::templates::fresh_request_token("treatment"));
                            input type="hidden" name="context_ref" value=(&counterparty.contact_ref);
                            input type="hidden" name="expected_membership_revision" value=(counterparty.membership_revision);
                            button type="submit" class="btn btn-secondary btn-small" {
                              (if counterparty.treatment_decision == crate::spacetimedb::BackendContextualDecision::EmergencyTreatment { "Emergency treatment" } else { "Request treatment" })
                            }
                        }
                    } @else {
                      button type="button" class="btn btn-secondary btn-small" disabled {
                        (if counterparty.treatment_decision == crate::spacetimedb::BackendContextualDecision::Refused { "Refused" } else { "Unavailable" })
                      }
                    }
                }
            }
        }
    }
}

/// Enemy encounter and, once resolved, its loot at an off-road quest location.
pub fn quest_location_enemy_page(
    presentation: &CaseSitePagePresentation,
    site: &BackendCaseSitePin,
    onsite_actions: &[BackendInvestigationAction],
    active_character: Option<&Character>,
    party_members: &[Character],
    counterparties: &[QuestCounterparty],
    can_fight: bool,
    resolved: bool,
    autoresolve_report: Option<&AutoresolveReport>,
    party: Option<&crate::spacetimedb::Party>,
    can_configure_travel: bool,
    default_rest_minutes: u64,
    soap_preview: super::settlement::SoapRestPreview,
    recovery_notice: Option<&CaseSiteRecoveryNotice>,
    loot: &[BattleLootItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[ItemDefinition],
    food_lots: &[FoodLot],
    targets: &[InventoryQuantityTarget],
    logged_in_as: Option<&str>,
    corpses: &[BackendCorpse],
    selected_corpse: Option<(&BackendCorpse, &str)>,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            @if !resolved && !counterparties.is_empty() {
                (sidebar_section("Counterparty", html! {
                    (quest_counterparty_strip(&site.case_site_id, counterparties))
                }))
            }
            @if !resolved {
                (sidebar_section("Location", html! { p { (&site.description) } }))
                section class="rest-service-menu quest-rest-menu" aria-label="Destination rest" {
                    (party_rest_menu(
                        &format!("/locations/case-site/{}/rest", site.case_site_id),
                        "quest-rest",
                        "Rest before battle",
                        "Rest party",
                        default_rest_minutes,
                        None,
                        soap_preview,
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
                    (loot_stage_form(&presentation.action_id))
                    (super::settlement::inventory_footer_controls("loot", "Move loot to targets", "Move all loot"))
                }
                }))
            }
        }

        (quest_location_center(
            presentation,
            site,
            onsite_actions,
            active_character,
            party_members,
            can_fight,
            resolved,
            autoresolve_report,
            None,
            true,
            recovery_notice,
            false,
            corpses,
            selected_corpse,
        ))

        aside class=(if resolved { "right-sidebar" } else { "right-sidebar travel-preferences-only-sidebar" })
            aria-label=(if resolved { "Party inventory" } else { "Location details" }) {
            @if !resolved {
                @if let Some(party) = party.filter(|_| can_configure_travel) {
                    (sidebar_section(
                        "Travel preferences",
                        travel_preferences_form(
                            party,
                            &format!("/locations/case-site/{}/map/travel-configuration", site.case_site_id),
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
                                @let food_lot = food_lots.iter().find(|lot| lot.party_inventory_item_id == Some(entry.id));
                                @let display_name = food_lot.map_or_else(|| item_display_name(&entry.item_id), |lot| lot.display_name.clone());
                                @let value = definition.and_then(|item| item.base_value).unwrap_or(0);
                                @let target = inventory_target(targets, &entry.item_id);
                                tr class="trade-inventory-row" data-target=(target) data-party-inventory-id=(entry.id) {
                                    td class="inventory-item-type" { (item_type_icon(&entry.item_id)) }
                                    td class="inventory-item-name" { (super::settlement::item_name_with_food_lot(&entry.item_id, &display_name, definition, food_lot)) }
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
        &presentation.title,
        &presentation.title,
        &site.case_site_id,
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
    fn case_site_counterparties_use_per_row_contact_and_treatment_actions() {
        let template = include_str!("quest.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let routes = include_str!("../routes/quests.rs");
        assert!(template.contains("/counterparty/contact"));
        assert!(template.contains("/counterparty/bandage"));
        assert!(routes.contains("\"treat_limb\""));
        let projection = routes
            .split("let context_memberships: Vec<BackendContextCharacter>")
            .nth(1)
            .and_then(|tail| tail.split("let can_fight").next())
            .expect("case-site counterparty projection");
        assert!(!projection.contains(".first()"));
        assert!(projection.contains("contact_ref: membership.contact_ref"));
        assert!(projection.contains("revision: membership.revision"));
        assert!(!template.contains("examine_outbreak_patient"));
        assert!(!template.contains("outbreak_patient_examination"));
    }

    #[test]
    fn each_counterparty_renders_only_its_own_contextual_claims() {
        let character = |id, name: &str| Character {
            id,
            name: name.into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: Some("case-site:known".into()),
            party_id: None,
            age_years: 30,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let rows = [
            QuestCounterparty {
                character: character(41, "Alice"),
                contact_ref: "claim-a".into(),
                revision: 3,
                membership_revision: 2,
                contact_decision: crate::spacetimedb::BackendContextualDecision::Request,
                treatment_decision: crate::spacetimedb::BackendContextualDecision::Unavailable,
                treatment_limb_slug: None,
            },
            QuestCounterparty {
                character: character(42, "Bert"),
                contact_ref: "claim-b".into(),
                revision: 7,
                membership_revision: 4,
                contact_decision: crate::spacetimedb::BackendContextualDecision::Refused,
                treatment_decision: crate::spacetimedb::BackendContextualDecision::Request,
                treatment_limb_slug: Some("right-leg".into()),
            },
            QuestCounterparty {
                character: character(43, "Charlie"),
                contact_ref: "claim-c".into(),
                revision: 9,
                membership_revision: 6,
                contact_decision: crate::spacetimedb::BackendContextualDecision::Unavailable,
                treatment_decision:
                    crate::spacetimedb::BackendContextualDecision::EmergencyTreatment,
                treatment_limb_slug: Some("chest".into()),
            },
        ];
        let markup = quest_counterparty_strip("case-site:known", &rows).into_string();
        let cards = markup
            .split("<div class=\"npc-portrait counterparty-portrait\">")
            .skip(1)
            .map(|card| {
                card.split_once("</div>")
                    .expect("complete counterparty card")
                    .0
            })
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), 3);
        assert!(cards[0].contains("Alice"));
        assert!(cards[0].contains("value=\"41\""));
        assert!(cards[0].contains("value=\"claim-a\""));
        assert!(!cards[0].contains("value=\"claim-b\""));
        assert!(!cards[0].contains("value=\"claim-c\""));
        assert!(cards[0].contains("value=\"3\""));
        assert!(cards[0].contains("Request"));
        assert!(cards[0].contains("Unavailable"));
        assert!(cards[1].contains("Bert"));
        assert!(cards[1].contains("value=\"42\""));
        assert!(cards[1].contains("value=\"claim-b\""));
        assert!(!cards[1].contains("value=\"claim-a\""));
        assert!(!cards[1].contains("value=\"claim-c\""));
        assert!(!cards[1].contains("/counterparty/contact"));
        assert!(cards[1].contains("/counterparty/bandage"));
        assert!(cards[1].contains("Refused"));
        assert!(cards[1].contains("Request treatment"));
        assert!(cards[1].contains("value=\"right-leg\""));
        assert!(cards[1].contains("value=\"4\""));
        assert!(cards[1].contains("name=\"action_id\" value=\"treatment-"));
        assert!(cards[2].contains("Charlie"));
        assert!(cards[2].contains("value=\"claim-c\""));
        assert!(!cards[2].contains("value=\"claim-a\""));
        assert!(!cards[2].contains("value=\"claim-b\""));
        assert!(cards[2].contains("Unavailable"));
        assert!(cards[2].contains("Emergency treatment"));
        assert!(cards[2].contains("value=\"chest\""));
    }

    #[test]
    fn autoresolve_report_becomes_complete_info_stream_rows() {
        let report = AutoresolveReport {
            battle_id: "battle:quest-1".into(),
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

    #[test]
    fn generated_site_offers_authorized_investigation_and_strategic_finale_only() {
        let presentation = CaseSitePagePresentation {
            title: "Travellers have gone missing".into(),
            action_id: "site:known".into(),
            allow_tactical_combat: false,
        };
        let site = BackendCaseSitePin {
            owner_character_id: 7,
            case_id: "journal:case".into(),
            case_site_id: "site:known".into(),
            origin_settlement_id: "settlement".into(),
            name: "a camp in the woods".into(),
            description: "A known place.".into(),
            scene_key: "forest".into(),
            longitude_e7: 0,
            latitude_e7: 0,
            coordinates_are_geographic: false,
            distance_m: 4_000,
            knowledge_stage: "visited".into(),
            tracked: false,
            display_title: presentation.title.clone(),
            generated_case: true,
            case_resolved: false,
            combat_available: true,
            opposition_count: None,
            opposition_combat_power: None,
        };
        let action = BackendInvestigationAction {
            owner_character_id: 7,
            action_id: "action:inspect".into(),
            method: "inspect_site".into(),
            expected_version: 2,
            summary: "Inspect the camp".into(),
            known_prerequisites: String::new(),
            duration_min_minutes: 15,
            duration_max_minutes: 45,
            uncertainty_bps: 2000,
            skill_contributions: "awareness".into(),
            weather_available: false,
            required_case_site_id: site.case_site_id.clone(),
            available: true,
            can_travel_to_required_site: false,
            unavailable_reason: String::new(),
        };
        let markup = quest_location_center(
            &presentation,
            &site,
            &[action],
            None,
            &[],
            true,
            false,
            None,
            None,
            true,
            None,
            false,
            &[],
            None,
        )
        .into_string();
        assert!(markup.contains("action=\"/quests/actions\""));
        assert!(markup.contains("Inspect the camp"));
        assert!(markup.contains("/quests/site:known/autoresolve"));
        assert!(!markup.contains("/missions/enter"));
    }

    #[test]
    fn unresolved_generated_noncombat_site_has_no_combat_panel() {
        let presentation = CaseSitePagePresentation {
            title: "A trail ends at the old well".into(),
            action_id: "site:evidence".into(),
            allow_tactical_combat: false,
        };
        let site = BackendCaseSitePin {
            owner_character_id: 7,
            case_id: "journal:case".into(),
            case_site_id: "site:evidence".into(),
            origin_settlement_id: "settlement".into(),
            name: "the old well".into(),
            description: "A place to inspect.".into(),
            scene_key: "village".into(),
            longitude_e7: 0,
            latitude_e7: 0,
            coordinates_are_geographic: false,
            distance_m: 100,
            knowledge_stage: "visited".into(),
            tracked: false,
            display_title: presentation.title.clone(),
            generated_case: true,
            case_resolved: false,
            combat_available: false,
            opposition_count: None,
            opposition_combat_power: None,
        };
        let markup = quest_location_center(
            &presentation,
            &site,
            &[],
            None,
            &[],
            false,
            false,
            None,
            None,
            true,
            None,
            false,
            &[],
            None,
        )
        .into_string();
        assert!(!markup.contains("quest-combat-actions"));
        assert!(!markup.contains("Waiting for party leader"));
        assert!(!markup.contains("Autoresolve"));
        assert!(!markup.contains("/missions/enter"));
    }

    #[test]
    fn resolved_generated_noncombat_site_shows_clear_completion() {
        let presentation = CaseSitePagePresentation {
            title: "A missing villager".into(),
            action_id: "site:rescue".into(),
            allow_tactical_combat: false,
        };
        let site = BackendCaseSitePin {
            owner_character_id: 7,
            case_id: "journal:case".into(),
            case_site_id: "site:rescue".into(),
            origin_settlement_id: "settlement".into(),
            name: "a camp in the woods".into(),
            description: "The captive was found here.".into(),
            scene_key: "forest".into(),
            longitude_e7: 0,
            latitude_e7: 0,
            coordinates_are_geographic: false,
            distance_m: 100,
            knowledge_stage: "visited".into(),
            tracked: false,
            display_title: presentation.title.clone(),
            generated_case: true,
            case_resolved: true,
            combat_available: false,
            opposition_count: None,
            opposition_combat_power: None,
        };

        let markup = quest_location_center(
            &presentation,
            &site,
            &[],
            None,
            &[],
            false,
            true,
            None,
            None,
            true,
            None,
            false,
            &[],
            None,
        )
        .into_string();

        assert!(markup.contains("Quest complete"));
        assert!(markup.contains("The local problem has been resolved."));
        assert!(!markup.contains("quest-combat-actions"));
    }

    #[test]
    fn resource_blocked_recovery_notice_is_truthful_and_actionable() {
        let presentation = CaseSitePagePresentation {
            title: "The old graveyard".into(),
            action_id: "site:old-graveyard".into(),
            allow_tactical_combat: false,
        };
        let site = BackendCaseSitePin {
            owner_character_id: 7,
            case_id: "journal:case".into(),
            case_site_id: "site:old-graveyard".into(),
            origin_settlement_id: "settlement".into(),
            name: "the old graveyard".into(),
            description: "A place to inspect.".into(),
            scene_key: "graveyard".into(),
            longitude_e7: 0,
            latitude_e7: 0,
            coordinates_are_geographic: false,
            distance_m: 100,
            knowledge_stage: "visited".into(),
            tracked: false,
            display_title: presentation.title.clone(),
            generated_case: true,
            case_resolved: false,
            combat_available: false,
            opposition_count: None,
            opposition_combat_power: None,
        };
        let notice = CaseSiteRecoveryNotice {
            member_names: "Lukas".into(),
            causes: "hunger, thirst".into(),
            resource_blocked: true,
            withdrawal_destination: "Ironforge".into(),
            withdrawal_href: "/locations/case-site/site:old-graveyard/map?destination=ironforge"
                .into(),
        };
        let enemy = quest_location_center(
            &presentation,
            &site,
            &[],
            None,
            &[],
            false,
            false,
            None,
            None,
            false,
            Some(&notice),
            false,
            &[],
            None,
        )
        .into_string();
        assert!(enemy.contains("Lukas"));
        assert!(enemy.contains("hunger, thirst"));
        assert!(enemy.contains("Field rest does not provide food or water"));
        assert!(enemy.contains("Resting longer may worsen"));
        assert!(enemy.contains("costs time and carries normal travel risk"));
        assert!(enemy.contains("supplies and care become available after arrival"));
        assert!(enemy.contains("Open map and select Ironforge"));
        assert!(enemy.contains(
            "href=\"/locations/case-site/site:old-graveyard/map?destination=ironforge\""
        ));
        assert!(!enemy.contains("guaranteed"));

        let map = quest_location_center(
            &presentation,
            &site,
            &[],
            None,
            &[],
            false,
            false,
            None,
            None,
            false,
            Some(&notice),
            true,
            &[],
            None,
        )
        .into_string();
        assert!(map.contains("Select Ironforge and begin journey"));
    }
}
