use std::collections::BTreeSet;

use maud::{Markup, html};

use super::corpse_medical_dialog;
use super::social::{npc_description_stage, npc_portrait_strip, settlement_npc_chat_area};
use crate::spacetimedb::{
    BackendCorpse, Character, Settlement, SettlementAlias, SettlementCategory,
    SettlementDescription, SettlementDescriptionKind,
};
use crate::templates::{
    game_icon, population_description, settlement_layout_with_session, sidebar_section,
};

fn settlement_has_keep(category: &SettlementCategory) -> bool {
    matches!(
        category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    )
}

fn public_square_place_link(settlement: &Settlement, current: bool) -> Markup {
    use adventuresim_core::settlement_economy::{player_visible_npc_tabs, visible_npc_tab};

    let tabs = player_visible_npc_tabs(
        &settlement.economy,
        settlement_has_keep(&settlement.category),
        &settlement.id,
    );
    let tab = visible_npc_tab(&tabs, "overview")
        .expect("every settlement exposes its overview as a navigable NPC tab");
    html! {
        a href=(format!("/locations/settlement/{}", settlement.id))
            class=(if current { "active" } else { "" })
            aria-current=(if current { "page" } else { "false" }) {
            (tab.label)
        }
    }
}

pub fn settlement_overview_page(
    settlement: &Settlement,
    aliases: &[SettlementAlias],
    descriptions: &[SettlementDescription],
    active_character: Option<&Character>,
    party_members: &[Character],
    logged_in_as: Option<&str>,
    corpses: &[BackendCorpse],
    selected_corpse: Option<(&BackendCorpse, &str)>,
) -> Markup {
    let alias_labels = settlement_alias_labels(settlement, aliases);
    let historical_description = preferred_settlement_description(descriptions);
    let population_tooltip = if settlement.population_estimate == 0 {
        format!(
            "No imported headcount is available\nSettlement class: {}",
            population_description(settlement.population_level)
        )
    } else {
        format!(
            "Imported estimate: {} people\nSettlement class: {}",
            format_number(settlement.population_estimate),
            population_description(settlement.population_level),
        )
    };
    let faith_labels = settlement
        .religious_status
        .represented_religions()
        .iter()
        .map(|religion| religion.label())
        .collect::<Vec<_>>();
    let faiths = joined_or_dash(&faith_labels);
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlement", html! {
                div class="settlement-summary" {
                    dl class="location-stat-list" {
                        div tabindex="0"
                            data-strategic-tooltip=(&population_tooltip) {
                            dt { "Population" } dd { (format_population(settlement)) }
                        }
                        div tabindex="0"
                            data-strategic-tooltip=(format!(
                                "Prosperity score: {}/1000",
                                settlement.economy.prosperity_score,
                            )) {
                            dt { "Prosperity" } dd { (format!("{:?}", settlement.economy.prosperity_tier)) }
                        }
                        div { dt { "Faiths" } dd { (&faiths) } }
                        div data-developer-only { dt { "Services" } dd { (settlement.economy.services.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div data-developer-only { dt { "Specialties" } dd { (settlement.economy.specializations.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div data-developer-only { dt { "Coordinates" } dd { (format!("{:.6}, {:.6}", settlement.coord_x, settlement.coord_y)) } }
                        div data-developer-only { dt { "Languages" } dd { (format!(
                            "East-central {:.1}% · West-central {:.1}% · Low {:.1}%",
                            f32::from(settlement.languages.east_central_bp) / 100.0,
                            f32::from(settlement.languages.west_central_bp) / 100.0,
                            f32::from(settlement.languages.low_bp) / 100.0,
                        )) } }
                        @if !alias_labels.is_empty() {
                            div { dt { "Also known as" } dd { (alias_labels.join(", ")) } }
                        }
                    }
                }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None, false))
            (npc_portrait_strip(&settlement.id, "overview"))
            @if !corpses.is_empty() {
                nav class="settlement-npc-strip corpse-strip" aria-label="Bodies held in the settlement" {
                    @for corpse in corpses {
                        @let corpse_label = if corpse.location == "interred" { "Buried body" } else { &corpse.display_name };
                        a class="npc-portrait corpse-portrait"
                            href=(format!("/locations/settlement/{}?corpse={}&medical=physiology", settlement.id, corpse.corpse_id))
                            aria-label=(format!("Examine {corpse_label} with Physiology")) {
                            span class="npc-portrait-image" aria-hidden="true" { "☠" }
                            span class="npc-portrait-name" { (corpse_label) }
                        }
                    }
                }
                @if let Some((corpse, _)) = selected_corpse {
                    div class="quest-combat-actions corpse-medical-actions" aria-label="Corpse medical windows" {
                        a class="btn btn-secondary" href=(format!("/locations/settlement/{}?corpse={}&medical=physiology", settlement.id, corpse.corpse_id)) { "Physiology" }
                        a class="btn btn-secondary" href=(format!("/locations/settlement/{}?corpse={}&medical=surgery", settlement.id, corpse.corpse_id)) { "Surgery" }
                    }
                }
            }
            (npc_description_stage(&settlement.name, "Select a local resident to see their visible description."))
            (settlement_npc_chat_area(&settlement.name, active_character, &settlement.id, "overview", None))
        }
        aside class="right-sidebar" {
            (sidebar_section("Description", html! {
                p { (settlement_description(settlement.population_level)) }
                @if let Some(description) = historical_description {
                    details class="settlement-historical-description" {
                        summary { (format!("Historical description — {}", language_label(description.language.as_deref()))) }
                        p { (description.body) }
                    }
                }
            }))
        }
        @if let Some((corpse, window)) = selected_corpse {
            (corpse_medical_dialog(
                corpse,
                &format!("/locations/settlement/{}", settlement.id),
                window,
            ))
        }
    };
    settlement_layout_with_session(
        &settlement.name,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        "",
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}

/// Shared authoritative shell for non-service public, residential, and keep locations.
pub fn settlement_npc_location_page(
    settlement: &Settlement,
    active_character: &Character,
    party_members: &[Character],
    location_id: &str,
    logged_in_as: Option<&str>,
) -> Markup {
    let chapter =
        adventuresim_core::organization::organization_chapter_at(&settlement.id, location_id);
    let (title, description) = match location_id {
        "residences" => (
            "Residential quarter",
            "Homes, courtyards, and narrow lanes where local households conduct their daily business.",
        ),
        "keep" => (
            "The keep",
            "The seat of local authority, occupied by retainers, servants, and petitioners.",
        ),
        _ if chapter.is_none() => (
            "Public square",
            "A public gathering place for residents and travelers.",
        ),
        _ => {
            let (organization, chapter) = chapter.expect("guarded chapter");
            (
                chapter.building_name.as_str(),
                organization.description.as_str(),
            )
        }
    };
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Places", html! {
                nav class="settlement-places-nav" aria-label="Settlement places" {
                    (public_square_place_link(settlement, false))
                    a href=(format!("/settlements/{}/places/residences", settlement.id))
                        class=(if location_id == "residences" { "active" } else { "" })
                        aria-current=(if location_id == "residences" { "page" } else { "false" }) {
                        "Residences"
                    }
                    @if settlement_has_keep(&settlement.category) {
                        a href=(format!("/settlements/{}/places/keep", settlement.id))
                            class=(if location_id == "keep" { "active" } else { "" })
                            aria-current=(if location_id == "keep" { "page" } else { "false" }) {
                            "Keep"
                        }
                    }
                    @for organization in adventuresim_core::organization::organizations_for_chapter(&settlement.id) {
                        @let chapter = organization.chapter(&settlement.id).expect("local chapter");
                        @if adventuresim_core::organization::chapter_has_standalone_building(organization, chapter, &settlement.economy) {
                        a href=(format!("/settlements/{}/places/{}", settlement.id, chapter.location_id))
                            class=(if location_id == chapter.location_id { "active" } else { "" })
                            aria-current=(if location_id == chapter.location_id { "page" } else { "false" }) {
                            (&chapter.building_name)
                        }
                        }
                    }
                }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, Some(active_character), &format!("/locations/settlement/{}", settlement.id), None, false))
            (npc_portrait_strip(&settlement.id, location_id))
            (npc_description_stage(title, description))
            (settlement_npc_chat_area(title, Some(active_character), &settlement.id, location_id, None))
        }
        aside class="right-sidebar" { (sidebar_section("Location", html! { p { (description) } })) }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        location_id,
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}

fn settlement_alias_labels(settlement: &Settlement, aliases: &[SettlementAlias]) -> Vec<String> {
    let canonical = settlement.name.to_lowercase();
    let mut labels = BTreeSet::new();
    for alias in aliases {
        let label = alias.prefix.as_ref().map_or_else(
            || alias.name.trim().to_owned(),
            |prefix| format!("{} {}", prefix.trim(), alias.name.trim()),
        );
        if !label.is_empty() && label.to_lowercase() != canonical {
            labels.insert(label);
        }
    }
    let total = labels.len();
    let mut labels: Vec<_> = labels.into_iter().take(8).collect();
    if total > labels.len() {
        labels.push(format!("and {} more", total - labels.len()));
    }
    labels
}

fn joined_or_dash(labels: &[&str]) -> String {
    if labels.is_empty() {
        "—".into()
    } else {
        labels.join(", ")
    }
}

fn preferred_settlement_description(
    descriptions: &[SettlementDescription],
) -> Option<&SettlementDescription> {
    descriptions.iter().min_by_key(|description| {
        (
            description.language.as_deref() != Some("eng"),
            description.kind != SettlementDescriptionKind::Settlement,
            description.id.as_str(),
        )
    })
}

fn language_label(language: Option<&str>) -> &str {
    match language {
        Some("dan") => "Danish",
        Some("deu") => "German",
        Some("eng") => "English",
        Some("fin") => "Finnish",
        Some("nld") => "Dutch",
        Some(code) => code,
        None => "Unspecified language",
    }
}

pub(crate) fn settlement_description(population_level: i32) -> &'static str {
    match population_level {
        i32::MIN..=1 => "A quiet cluster of farmsteads and cottages.",
        2 => "A quaint hamlet gathered around a well-worn road.",
        3 => "A modest village serving the surrounding countryside.",
        4 => "A busy market town with a steady flow of travelers.",
        5 => "A prosperous town enclosed by crowded streets.",
        _ => "A large and bustling city whose streets rarely fall silent.",
    }
}

pub(super) fn format_distance(distance_m: u64) -> String {
    format!("{:.1} km", distance_m as f64 / 1_000.0)
}

pub(super) fn format_population(settlement: &Settlement) -> String {
    match settlement.population_estimate {
        0 => population_description(settlement.population_level).to_string(),
        population => format!("approximately {}", format_number(population)),
    }
}

fn format_number(value: u32) -> String {
    let digits = value.to_string();
    let first_group = match digits.len() % 3 {
        0 => 3,
        remainder => remainder,
    };
    let mut formatted = digits[..first_group].to_string();
    for group in digits[first_group..].as_bytes().chunks(3) {
        formatted.push(',');
        formatted.push_str(std::str::from_utf8(group).expect("population digits are valid UTF-8"));
    }
    formatted
}

pub(super) fn format_journey_time(minutes: u64) -> String {
    let hours = minutes / 60;
    let minutes = minutes % 60;
    if hours == 0 {
        format!("{minutes} min")
    } else if minutes == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {minutes} min")
    }
}

pub(crate) fn visual_stage(kind: &str, title: &str, description: &str) -> Markup {
    let scene_label = match kind {
        "settlement" => "At the settlement gates",
        "map" | "route" => "Roads and destinations",
        "camp" => "Camp beside the road",
        "quest" => "Encounter ground",
        "alchemy" => "The apothecary workbench",
        "service" => "At the counter",
        "chest" => "Shared party stores",
        _ => "Adventurer profile",
    };
    html! {
        figure class=(format!("service-visual service-visual-{}", kind)) {
            div class="service-visual-scene" role="img" aria-label=(format!("{title}. {description}")) {
                span class="visual-scene-sky" aria-hidden="true" {}
                span class="visual-scene-horizon" aria-hidden="true" {}
                span class="visual-scene-route" aria-hidden="true" {}
                span class="visual-scene-caption" {
                    strong { (title) }
                    span { (scene_label) }
                }
            }
            figcaption { (description) }
        }
    }
}

pub(crate) struct CharacterPortraitView<'a> {
    pub id: u64,
    pub name: &'a str,
    pub alive: bool,
    pub active: bool,
    pub selected: bool,
    pub href: String,
    pub title: String,
    pub aria_label: String,
    pub decoration: Option<Markup>,
    pub badge: Option<Markup>,
    pub actions: Option<Markup>,
}

pub(crate) fn character_portrait_overlay(
    label: &str,
    inventory: Option<Markup>,
    members: &[CharacterPortraitView<'_>],
) -> Markup {
    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label=(label) {
                div data-party-portrait-members {
                    @if let Some(inventory) = inventory {
                        (inventory)
                    }
                    @for member in members {
                        div class=(format!("party-portrait{}{}", if member.selected { " active" } else { "" }, if !member.alive { " dead" } else { "" }))
                            data-character-id=(member.id)
                            data-character-alive=(member.alive)
                            data-active-character[member.active]
                            title=(member.name) {
                            a class="party-portrait-select"
                                href=(&member.href)
                                title=(&member.title)
                                aria-label=(&member.aria_label) {
                                @if let Some(decoration) = &member.decoration {
                                    (decoration)
                                }
                                span class="party-portrait-initial" {
                                    span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                                    span class="party-portrait-name" { (member.name) @if !member.alive { " (dead)" } }
                                    @if let Some(badge) = &member.badge {
                                        (badge)
                                    }
                                }
                            }
                            @if let Some(actions) = &member.actions {
                                (actions)
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn party_portrait_overlay(
    party_members: &[Character],
    active_character: Option<&Character>,
    location_path: &str,
    selected_character_id: Option<u64>,
    can_examine: bool,
) -> Markup {
    let members: Vec<&Character> = if party_members.is_empty() {
        active_character.into_iter().collect()
    } else {
        party_members.iter().collect()
    };
    let leader_id = members.first().map(|member| member.id);

    let inventory = active_character.map(|_| {
        html! {
            div class="party-portrait party-inventory-portrait" title="Party inventory" {
                a class="party-portrait-select" href=(format!("{}/party-inventory", location_path)) {
                    span class="party-portrait-initial party-chest-face" { (game_icon("Party inventory", "knapsack")) }
                }
            }
        }
    });
    let portraits = members
        .into_iter()
        .map(|member| {
            let is_active = active_character.is_some_and(|character| character.id == member.id);
            let can_remove = Some(member.id) != leader_id;
            let notified = member.alive && member.social_notification_count > 0;
            let persistently_notified = notified && !member.automatic_social_chat_enabled;
            let inspection_href = if is_active {
                format!("{}/party/{}", location_path, member.id)
            } else {
                format!("{}/party/{}/stats", location_path, member.id)
            };
            let actions = (member.alive
                && active_character.is_some_and(|character| character.alive))
            .then(|| {
                html! {
                    span class="party-portrait-actions" aria-label=(format!("Actions for {}", member.name)) {
                            a href=(format!("{}/party/{}/social", location_path, member.id))
                                class=(format!("party-portrait-action party-social-action{}", if persistently_notified { " party-social-notified" } else { "" }))
                                title=(if notified { format!("Talk to {} about {} morale concerns", member.name, member.social_notification_count) } else { format!("Talk to {}", member.name) })
                                aria-label=(if notified { format!("Talk to {} about {} unaddressed morale concerns", member.name, member.social_notification_count) } else { format!("Talk to {}", member.name) })
                                aria-haspopup="dialog" {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/game/conversation.svg')"
                                    aria-hidden="true" {}
                                @if notified {
                                    span class="party-social-notification" aria-hidden="true" {
                                        (member.social_notification_count)
                                    }
                                }
                            }
                            @if is_active && can_examine && location_path.starts_with("/locations/settlement/") {
                                a href=(format!("{location_path}/alchemy"))
                                    class="party-portrait-action party-alchemy-action"
                                    title="Prepare medication"
                                    aria-label="Prepare medication" {
                                    span class="party-action-icon"
                                        style="--party-action-icon: url('/static/icons/game/medical-pack.svg')"
                                        role="img" aria-label="Alchemy" {}
                                }
                            }
                            a href=(format!("{}/party/{}/inventory", location_path, member.id))
                                class="party-portrait-action"
                                title=(if is_active { "Open inventory and discard items".to_string() } else { format!("Compare inventory with {}", member.name) }) {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/game/knapsack.svg')"
                                    role="img" aria-label="Inventory" {}
                            }
                            @if can_remove {
                                form method="post" action=(format!("{}/party/{}/remove", location_path, member.id)) {
                                    button type="submit" class=(if is_active { "party-portrait-action party-member-remove party-member-leave" } else { "party-portrait-action party-member-remove party-member-kick-request" })
                                        title=(if is_active { "Leave party".to_string() } else { format!("Request to remove {} from the party", member.name) })
                                        aria-label=(if is_active { "Leave party".to_string() } else { format!("Request to remove {} from the party", member.name) }) {
                                        span aria-hidden="true" { "×" }
                                    }
                                }
                            }
                    }
                }
            });
            CharacterPortraitView {
                id: member.id,
                name: &member.name,
                alive: member.alive,
                active: is_active,
                selected: selected_character_id == Some(member.id),
                href: inspection_href,
                title: format!("Inspect {}", member.name),
                aria_label: format!("Inspect {}", member.name),
                decoration: Some(html! {
                    span class="incapacitation-wheel"
                        data-strategic-condition-wheel=(member.id)
                        role="img"
                        aria-label="Loading strategic condition"
                        title="Loading strategic condition" {}
                }),
                badge: None,
                actions,
            }
        })
        .collect::<Vec<_>>();
    character_portrait_overlay("Active party", inventory, &portraits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use crate::templates::settlement::test_support::*;

    #[test]
    fn notified_social_action_stays_visible_while_portrait_keeps_inspection() {
        let member = Character {
            id: 12,
            name: "Greta".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("lubeck".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 24,
            alive: true,
            temporary: false,
            social_notification_count: 2,
            automatic_social_chat_enabled: false,
        };
        let markup = party_portrait_overlay(
            &[member.clone()],
            Some(&member),
            "/locations/settlement/lubeck",
            None,
            false,
        )
        .into_string();
        assert!(markup.contains(
            "class=\"party-portrait-select\" href=\"/locations/settlement/lubeck/party/12\""
        ));
        assert!(
            markup.contains(
                "class=\"party-portrait-action party-social-action party-social-notified\""
            )
        );
        assert!(markup.contains("href=\"/locations/settlement/lubeck/party/12/social\""));
        assert!(markup.contains("class=\"party-social-notification\""));
        assert!(markup.contains("2 unaddressed morale concerns"));
        assert!(markup.contains("/static/icons/game/conversation.svg"));
        assert!(markup.contains("class=\"incapacitation-wheel\""));
        assert!(markup.contains("data-strategic-condition-wheel=\"12\""));

        let mut quiet = member;
        quiet.social_notification_count = 0;
        let quiet_markup = party_portrait_overlay(
            &[quiet.clone()],
            Some(&quiet),
            "/locations/settlement/lubeck",
            None,
            false,
        )
        .into_string();
        assert!(!quiet_markup.contains("party-social-notification"));
        assert!(quiet_markup.contains("class=\"party-portrait-action party-social-action\""));
        assert!(quiet_markup.contains("/party/12/social"));
        assert!(quiet_markup.contains("aria-label=\"Talk to Greta\""));

        let mut automatic = quiet;
        automatic.social_notification_count = 2;
        automatic.automatic_social_chat_enabled = true;
        let automatic_markup = party_portrait_overlay(
            &[automatic.clone()],
            Some(&automatic),
            "/locations/settlement/lubeck",
            None,
            false,
        )
        .into_string();
        assert!(automatic_markup.contains("class=\"party-social-notification\""));
        assert!(automatic_markup.contains("2 unaddressed morale concerns"));
        assert!(!automatic_markup.contains("party-social-notified"));
    }

    #[test]
    fn aliases_are_deduplicated_and_do_not_repeat_the_canonical_name() {
        let aliases = [
            SettlementAlias {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lubeke".into(),
                prefix: None,
                language: Some("deu".into()),
            },
            SettlementAlias {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lübeck".into(),
                prefix: None,
                language: None,
            },
        ];

        assert_eq!(settlement_alias_labels(&settlement(), &aliases), ["Lubeke"]);
    }

    #[test]
    fn english_settlement_description_is_preferred_deterministically() {
        let descriptions = [
            SettlementDescription {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("deu".into()),
                body: "Deutsch".into(),
            },
            SettlementDescription {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::City,
                language: Some("eng".into()),
                body: "English city".into(),
            },
            SettlementDescription {
                id: "3".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("eng".into()),
                body: "English settlement".into(),
            },
        ];

        assert_eq!(
            preferred_settlement_description(&descriptions)
                .unwrap()
                .body,
            "English settlement"
        );
    }

    #[test]
    fn settlement_overview_renders_enrichment_as_escaped_text() {
        let aliases = [SettlementAlias {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            name: "Lubeke".into(),
            prefix: None,
            language: Some("deu".into()),
        }];
        let descriptions = [SettlementDescription {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            kind: SettlementDescriptionKind::Settlement,
            language: Some("deu".into()),
            body: "Burg & Markt <alt>".into(),
        }];

        let markup = settlement_overview_page(
            &settlement(),
            &aliases,
            &descriptions,
            None,
            &[],
            None,
            &[],
            None,
        )
        .into_string();

        assert!(markup.contains("Also known as"));
        assert!(markup.contains("Lubeke"));
        assert!(markup.contains("Historical description — German"));
        assert!(markup.contains("Burg &amp; Markt &lt;alt&gt;"));
        assert!(!markup.contains("<alt>"));
    }

    #[test]
    fn settlement_overview_treats_zero_population_as_missing_and_empty_faiths_as_unknown() {
        let mut settlement = settlement();
        settlement.population_estimate = 0;
        let markup = settlement_overview_page(&settlement, &[], &[], None, &[], None, &[], None)
            .into_string();
        assert!(markup.contains("No imported headcount is available"));
        assert!(!markup.contains("Imported estimate: 0 people"));
        assert_eq!(joined_or_dash(&[]), "—");
    }

    #[test]
    fn settlement_overview_exposes_moved_corpses_in_existing_medical_windows() {
        let corpse = BackendCorpse {
            owner_character_id: 7,
            corpse_id: "corpse:quest:1".into(),
            display_name: "Unknown victim".into(),
            creature_kind: "human".into(),
            source_id: "quest:1".into(),
            location: "local_custody".into(),
            decomposition: "early".into(),
            case_site_id: "site:1".into(),
            settlement_id: "viabundus-1".into(),
            opened: false,
            permission: "none".into(),
            exhumation_permission: false,
            penalty_free_burning: false,
            revision: 0,
            findings: Vec::new(),
        };
        let markup = settlement_overview_page(
            &settlement(),
            &[],
            &[],
            None,
            &[],
            None,
            std::slice::from_ref(&corpse),
            Some((&corpse, "physiology")),
        )
        .into_string();

        assert!(markup.contains("Bodies held in the settlement"));
        assert!(markup.contains("corpse-portrait"));
        assert!(markup.contains(
            "/locations/settlement/viabundus-1?corpse=corpse:quest:1&amp;medical=surgery"
        ));
        assert!(markup.contains("physiology-dialog"));
        assert!(markup.contains("action=\"/corpses/corpse:quest:1/action\""));
        assert!(markup.contains("name=\"return_to\""));
    }

    #[test]
    fn intentional_stages_have_distinct_semantics_and_no_prototype_copy() {
        for (kind, label) in [
            ("settlement", "At the settlement gates"),
            ("route", "Roads and destinations"),
            ("camp", "Camp beside the road"),
            ("character", "Adventurer profile"),
            ("service", "At the counter"),
            ("quest", "Encounter ground"),
            ("alchemy", "The apothecary workbench"),
            ("chest", "Shared party stores"),
        ] {
            let markup = visual_stage(kind, "A Place", "An intentional scene").into_string();
            assert!(markup.contains(label));
            assert!(markup.contains("role=\"img\""));
            assert!(!markup.contains("placeholder"));
            assert!(!markup.contains("TODO"));
            assert!(!markup.contains("visual-scene-emblem"));
            assert!(!markup.contains("/static/icons/game/"));
        }

        let character = visual_stage("character", "Ada", "Character sheet").into_string();
        assert!(character.contains("role=\"img\" aria-label=\"Ada. Character sheet\""));
        let css = include_str!("../../../static/css/strategic.css");
        let character_figure = css
            .split(".service-visual-character .visual-scene-horizon {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("character stage needs a restrained silhouette");
        assert!(character_figure.contains("/static/icons/game/person.svg"));
        assert!(character_figure.contains("width: clamp(5rem, 22%, 8rem);"));
        assert!(character_figure.contains("top: 8%;"));
        assert!(character_figure.contains("clip-path: inset(0 0 22%);"));
        assert!(!character_figure.contains("border-radius"));
    }

    #[test]
    fn places_navigation_exposes_the_generated_public_square_referral_tab() {
        use adventuresim_core::settlement_economy::{player_visible_npc_tabs, visible_npc_tab};

        let settlement = settlement();
        let tabs = player_visible_npc_tabs(&settlement.economy, true, &settlement.id);
        let public_square = visible_npc_tab(&tabs, "overview").unwrap();
        assert_eq!(public_square.label, "Public square");

        let overview =
            settlement_overview_page(&settlement, &[], &[], None, &[], Some("Visitor"), &[], None)
                .into_string();
        assert!(overview.contains("aria-label=\"Settlement services\""));
        assert!(overview.contains("aria-label=\"Public square\""));
        assert!(overview.contains("href=\"/locations/settlement/viabundus-1\""));
        assert!(!overview.contains("aria-label=\"Settlement places\""));
        assert!(overview.contains("data-strategic-tooltip=\"Imported estimate:"));
        assert!(overview.contains("data-developer-only"));

        let character = Character {
            id: 1,
            name: "Visitor".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some(settlement.id.clone()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let residences = settlement_npc_location_page(
            &settlement,
            &character,
            &[],
            "residences",
            Some("Visitor"),
        )
        .into_string();
        let residence_places = residences
            .split("aria-label=\"Settlement places\"")
            .nth(1)
            .and_then(|tail| tail.split("</nav>").next())
            .expect("residence Places navigation");
        assert!(residences.contains("class=\"settlement-places-nav\""));
        assert!(residence_places.contains("href=\"/locations/settlement/viabundus-1\""));
        assert!(residence_places.contains(&format!(">{}</a>", public_square.label)));
        assert!(residence_places.contains("aria-current=\"false\""));
        assert!(residence_places.contains("class=\"active\" aria-current=\"page\">Residences</a>"));

        let mut colocated = settlement;
        colocated.id = "viabundus-0".into();
        colocated.economy.services = vec![adventuresim_world_schema::SettlementService::Market];
        let mut colocated_character = character;
        colocated_character.current_settlement_id = Some(colocated.id.clone());
        let colocated_places = settlement_npc_location_page(
            &colocated,
            &colocated_character,
            &[],
            "residences",
            Some("Visitor"),
        )
        .into_string();
        assert!(!colocated_places.contains("organization-merchant-guild"));
        assert!(colocated_places.contains("organization-physicians-college"));
        assert!(colocated_places.contains("organization-surgeons-guild"));

        let components = include_str!("../../../static/css/components.css");
        assert!(components.contains(".settlement-places-nav {\n  display: grid;\n  gap: 0.3rem;"));
        assert!(components.contains(
            ".settlement-places-nav a:is(:hover, :focus-visible, .active, [aria-current=\"page\"])"
        ));
    }

    #[test]
    fn responsive_and_hidden_control_rules_keep_content_available() {
        let layout = include_str!("../../../static/css/layout.css").replace("\r\n", "\n");
        let strategic = include_str!("../../../static/css/strategic.css");
        let utilities = include_str!("../../../static/css/utilities.css");
        assert!(layout.contains("grid-template-areas: \"main\" \"left\" \"right\""));
        assert!(
            layout.contains(".right-sidebar {\n    display: block;")
                || layout.contains(".right-sidebar {\n    display: block;")
                || layout.contains(".right-sidebar {\n  display: block;")
        );
        assert!(strategic.contains("@media (hover: none), (pointer: coarse)"));
        assert!(utilities.contains(".inventory-count:focus-within"));
        assert!(utilities.contains("@media (hover:none), (pointer:coarse)"));
        assert!(utilities.contains("width: 2.75rem"));
        assert!(utilities.contains("height: 2.75rem"));
        assert!(utilities.contains("grid-template-columns: 2.75rem 1.4rem 2.75rem"));
        for scene in [
            "settlement",
            "route",
            "camp",
            "quest",
            "character",
            "service",
            "alchemy",
            "chest",
        ] {
            assert!(strategic.contains(&format!(".service-visual-{scene}")));
        }
    }
}
