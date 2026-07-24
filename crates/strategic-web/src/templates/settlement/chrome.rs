use super::*;

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
) -> Markup {
    let alias_labels = settlement_alias_labels(settlement, aliases);
    let historical_description = preferred_settlement_description(descriptions);
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlement", html! {
                div class="settlement-summary" {
                    dl class="location-stat-list" {
                        div { dt { "Population" } dd { (format_population(settlement)) } }
                        div { dt { "Size" } dd { (population_description(settlement.population_level)) } }
                        div { dt { "Prosperity" } dd { (format!("{:?} ({}/1000)", settlement.economy.prosperity_tier, settlement.economy.prosperity_score)) } }
                        div { dt { "Services" } dd { (settlement.economy.services.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div { dt { "Specialties" } dd { (settlement.economy.specializations.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>().join(", ")) } }
                        div { dt { "Faiths" } dd { (settlement.religious_status.represented_religions().iter().map(|r| r.label()).collect::<Vec<_>>().join(", ")) } }
                        div { dt { "Coordinates" } dd { (format!("{}, {}", settlement.coord_x as i32, settlement.coord_y as i32)) } }
                        div { dt { "Languages" } dd { (format!(
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
            (sidebar_section("Places", html! {
                nav aria-label="Settlement places" {
                    (public_square_place_link(settlement, true))
                    a href=(format!("/settlements/{}/places/residences", settlement.id)) { "Residences" }
                    @if settlement_has_keep(&settlement.category) {
                        a href=(format!("/settlements/{}/places/keep", settlement.id)) { "Keep" }
                    }
                }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None, false))
            (npc_portrait_strip(&settlement.id, "overview"))
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
    let (title, description) = match location_id {
        "residences" => (
            "Residential quarter",
            "Homes, courtyards, and narrow lanes where local households conduct their daily business.",
        ),
        "keep" => (
            "The keep",
            "The seat of local authority, occupied by retainers, servants, and petitioners.",
        ),
        _ => (
            "Public square",
            "A public gathering place for residents and travelers.",
        ),
    };
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Places", html! {
                nav aria-label="Settlement places" {
                    (public_square_place_link(settlement, false))
                    a href=(format!("/settlements/{}/places/residences", settlement.id)) { "Residences" }
                    @if settlement_has_keep(&settlement.category) {
                        a href=(format!("/settlements/{}/places/keep", settlement.id)) { "Keep" }
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

pub(super) fn settlement_alias_labels(
    settlement: &Settlement,
    aliases: &[SettlementAlias],
) -> Vec<String> {
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

pub(super) fn preferred_settlement_description(
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

pub(super) fn language_label(language: Option<&str>) -> &str {
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

pub(super) fn format_number(value: u32) -> String {
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

    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label="Active party" {
                div data-party-portrait-members {
                @if active_character.is_some() {
                    div class="party-portrait party-inventory-portrait" title="Party inventory" {
                        a class="party-portrait-select" href=(format!("{}/party-inventory", location_path)) {
                            span class="party-portrait-initial party-chest-face" { (game_icon("Party inventory", "knapsack")) }
                        }
                    }
                }
                @for member in members {
                    @let is_active = active_character.is_some_and(|character| character.id == member.id);
                    @let can_remove = Some(member.id) != leader_id;
                    div class=(format!("party-portrait{}{}", if selected_character_id == Some(member.id) { " active" } else { "" }, if !member.alive { " dead" } else { "" }))
                        data-character-id=(member.id)
                        data-character-alive=(member.alive)
                        data-active-character[is_active]
                        title=(&member.name) {
                        a class="party-portrait-select"
                            href=(if is_active {
                                format!("{}/party/{}", location_path, member.id)
                            } else {
                                format!("{}/party/{}/stats", location_path, member.id)
                            })
                            title=(format!("Inspect {}", member.name)) {
                            span class="party-portrait-initial" {
                                span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                                span class="party-portrait-name" { (&member.name) @if !member.alive { " (dead)" } }
                            }
                        }
                        @if member.alive && active_character.is_some_and(|character| character.alive) {
                        span class="party-portrait-actions" aria-label=(format!("Actions for {}", member.name)) {
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
                    }
                }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use crate::templates::settlement::test_support::*;

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

        let markup =
            settlement_overview_page(&settlement(), &aliases, &descriptions, None, &[], None)
                .into_string();

        assert!(markup.contains("Also known as"));
        assert!(markup.contains("Lubeke"));
        assert!(markup.contains("Historical description — German"));
        assert!(markup.contains("Burg &amp; Markt &lt;alt&gt;"));
        assert!(!markup.contains("<alt>"));
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
    }

    #[test]
    fn responsive_and_hidden_control_rules_keep_content_available() {
        let layout = include_str!("../../static/css/layout.css");
        let strategic = include_str!("../../static/css/strategic.css");
        let utilities = include_str!("../../static/css/utilities.css");
        assert!(layout.contains("grid-template-areas: \"main\" \"left\" \"right\""));
        assert!(layout.contains(".right-sidebar {\n    display: block;"));
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
