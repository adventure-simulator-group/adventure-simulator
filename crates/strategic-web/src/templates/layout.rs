//! Base layout template - Three-column strategic design.

use crate::spacetimedb::SettlementCategory;
use maud::{DOCTYPE, Markup, html};

use super::religion_icon_path;
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptProfile {
    Entry,
    Live,
    Strategic,
}

/// Minimal shell for selecting or creating a character. It intentionally omits
/// strategic navigation: an adventurer must be selected before play begins.
pub fn entry_layout(title: &str, content: Markup) -> Markup {
    page_shell(title, entry_top_bar(), content, ScriptProfile::Entry)
}

/// Transitional shell shown while a tactical server is being allocated.
pub fn mission_layout(title: &str, content: Markup, logged_in_as: Option<&str>) -> Markup {
    let header = html! {
        header class="top-bar entry-top-bar" {
            div class="top-bar-left" { h1 class="logo" { "Adventure Simulator" } }
            div class="entry-message" { "Preparing tactical mission" }
            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    span class="player-name" { strong { (name) } }
                }
            }
        }
    };
    page_shell(title, header, content, ScriptProfile::Live)
}

/// Settlement-specific layout. Settlement services replace the global navigation
/// so their context stays visible while the player moves between service pages.
pub fn settlement_layout_with_session(
    title: &str,
    settlement_name: &str,
    settlement_id: &str,
    category: &SettlementCategory,
    active_service: &str,
    religion_id: Option<&str>,
    content: Markup,
    logged_in_as: Option<&str>,
) -> Markup {
    page_shell(
        title,
        settlement_top_bar(
            settlement_name,
            settlement_id,
            category,
            active_service,
            religion_id,
            logged_in_as,
        ),
        content,
        ScriptProfile::Strategic,
    )
}

/// Off-road location layout. It keeps the strategic identity, time, and party
/// controls without exposing settlement-only services.
pub fn quest_location_layout_with_session(
    title: &str,
    location_name: &str,
    location_id: &str,
    active_tab: &str,
    content: Markup,
    logged_in_as: Option<&str>,
) -> Markup {
    page_shell(
        title,
        quest_location_top_bar(location_name, location_id, active_tab, logged_in_as),
        content,
        ScriptProfile::Strategic,
    )
}

fn page_shell(title: &str, header: Markup, content: Markup, scripts: ScriptProfile) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Adventure Simulator" }

                link rel="stylesheet" href="/static/css/base.css?v=environment-14";
                // Shared CSS
                link rel="stylesheet" href="/static/css/reset.css";
                link rel="stylesheet" href="/static/css/layout.css?v=left-rail-scrollbars-3-settlement-horizons-1";
                link rel="stylesheet" href="/static/css/components.css?v=coin-currencies-3";
                link rel="stylesheet" href="/static/css/strategic.css?v=inventory-browser-15-religion-5-travel-polish-9";
                link rel="stylesheet" href="/static/css/utilities.css?v=inventory-browser-15";

                // Datastar
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar/bundles/datastar.js" {}
                script src="/static/background-fetch.js?v=background-fetch-2" {}
                script src="/static/medical-examination.js?v=one-shot-1" defer {}
                @if scripts != ScriptProfile::Entry {
                    script src="/static/live-state.js?v=sse-3" defer {}
                    script src="/static/live-regions.js?v=floating-time-editor-1" defer {}
                }
                @if scripts == ScriptProfile::Strategic {
                    script src="/static/numeric-editor.js?v=shared-numeric-editor-2" defer {}
                    script src="/static/inventory-browser.js?v=coin-currencies-3" defer {}
                    script src="/static/party-trade.js?v=coin-currencies-2-travel-provisioning-3" defer {}
                    script src="/static/equipment-toggle.js?v=functional-equipment-1" defer {}
                    script src="/static/party-notifications.js?v=standing-leadership-votes-5" defer {}
                    script src="/static/party-recruitment.js?v=party-recruitment-live-3" defer {}
                    script src="/static/service-quests.js?v=coin-currencies-2-quest-description-tooltip-1" defer {}
                    script src="/static/chat-resize.js?v=floating-chat-3" defer {}
                    script src="/static/local-chat.js?v=herbalist-private-1" defer {}
                    script src="/static/strategic-condition.js?v=strategic-condition-3" defer {}
                    script src="/static/building-state.js?v=village-building-tabs-1" defer {}
                    script src="/static/travel-planner.js?v=travel-polish-6" defer {}
                    script src="/static/rest-duration.js?v=wake-time-3" defer {}
                }
            }
            body {
                @if scripts != ScriptProfile::Entry {
                    div id="strategic-live-stream" data-init="@get('/live')" {
                        span id="strategic-live-revision" data-live-revision="0" hidden {}
                    }
                }
                div class="app" {
                    (header)

                    div class="main-grid" {
                        (content)
                    }
                }
            }
        }
    }
}

fn entry_top_bar() -> Markup {
    html! {
        header class="top-bar entry-top-bar" {
            div class="top-bar-left" {
                h1 class="logo" { "Adventure Simulator" }
            }
            div class="entry-message" { "Choose an adventurer to begin" }
            div class="top-bar-right" {}
        }
    }
}

fn settlement_top_bar(
    settlement_name: &str,
    settlement_id: &str,
    category: &SettlementCategory,
    active_service: &str,
    religion_id: Option<&str>,
    logged_in_as: Option<&str>,
) -> Markup {
    let services = [
        ("map", "Map", "map"),
        ("merchants", "General Market", "market"),
        ("weapons", "Weapons", "weapons"),
        ("armor", "Armour", "armor"),
        ("clothing", "Clothing", "clothing"),
        ("herbalist", "Herbalist", "medical-pack"),
        ("inn", "Inn", "inn"),
        ("religion", "Church", "church"),
    ];

    html! {
        @let material = if matches!(category, SettlementCategory::City | SettlementCategory::Capital) { "stone" } else { "wood" };
        @let active_material = if active_service == "religion" { "stone" } else { material };
        @let active_tint = building_tint(settlement_id, active_service, active_material);
        style { (format!(":root{{--active-building-tint:{active_tint};}}")) }
        header class=(format!("top-bar settlement-top-bar material-{material}"))
            data-environment="settlement"
            data-building-tier=(building_tier(category))
            data-horizon-variant=(horizon_variant(settlement_id).as_str()) {
            div class="top-bar-left settlement-location" {
                div class="settlement-identity" {
                a href=(format!("/locations/settlement/{}", settlement_id)) class="settlement-name" {
                    (settlement_name)
                }
                span class="settlement-time" data-player-time title="Loading official time…" {
                    "1st of First Seed · 08:00"
                }
                }
            }

            nav class="top-bar-center settlement-services" aria-label="Settlement services"
                data-settlement-id=(settlement_id) {
                @for (path, label, icon) in services {
                    @let href = if path == "map" {
                        format!("/locations/settlement/{}/map", settlement_id)
                    } else if path.is_empty() {
                        format!("/locations/settlement/{}", settlement_id)
                    } else {
                        format!("/settlements/{}/{}", settlement_id, path)
                    };
                    @let service_material = if path == "religion" { "stone" } else { material };
                    @let tint = building_tint(settlement_id, path, service_material);
                    a href=(href)
                        class=(if active_service == path { "nav-tab active" } else { "nav-tab" })
                        style=(format!("--building-tint:{tint}"))
                        data-service-id=(path)
                        aria-label=(label)
                        title=(label)
                        aria-current=(if active_service == path { "page" } else { "false" })
                    {
                        span class="service-tab-building" aria-hidden="true" {}
                        span
                            class=(format!("service-tab-icon service-tab-icon-{}", icon))
                            style=[(path == "religion").then(|| format!("--service-tab-icon: url('{}')", religion_icon_path(religion_id)))]
                            aria-hidden="true" {}
                        @if path == "map" {
                            span class="service-notification-badge service-map-quest-badge"
                                data-map-quest-badge title="Active quest" aria-hidden="true" hidden { "!" }
                        } @else {
                            span class="service-notification-badge service-quest-badge" data-service-quest-badge hidden { "!" }
                        }
                    }
                }
            }

            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    (character_switcher(name))
                }
            }
        }
        script src="/static/strategic-time.js?v=wake-time-1" {}
    }
}

fn quest_location_top_bar(
    location_name: &str,
    location_id: &str,
    active_tab: &str,
    logged_in_as: Option<&str>,
) -> Markup {
    let map_tint = "hsl(126 30% 22%)";
    let loot_tint = "hsl(105 27% 19%)";
    let active_tint = if active_tab == "loot" {
        loot_tint
    } else {
        map_tint
    };
    html! {
        style { (format!(":root{{--active-building-tint:{active_tint};}}")) }
        header class="top-bar settlement-top-bar quest-location-top-bar" data-environment="wilderness" {
            div class="top-bar-left settlement-location" {
                div class="settlement-identity" {
                    a href=(format!("/locations/quest/{}", location_id)) class="settlement-name" { (location_name) }
                    span class="settlement-time" data-player-time { "1st of First Seed · 08:00" }
                }
            }
            nav class="top-bar-center settlement-services" aria-label="Location views" {
                a href=(format!("/locations/quest/{}/map", location_id))
                    class=(if active_tab == "map" { "nav-tab active" } else { "nav-tab" })
                    style=(format!("--building-tint:{map_tint}"))
                    aria-current=(if active_tab == "map" { "page" } else { "false" })
                    aria-label="Map" title="Map" {
                    span class="service-tab-icon service-tab-icon-map" aria-hidden="true" {}
                }
                a href=(format!("/locations/quest/{}/loot", location_id))
                    class=(if active_tab == "loot" { "nav-tab active" } else { "nav-tab" })
                    style=(format!("--building-tint:{loot_tint}"))
                    aria-current=(if active_tab == "loot" { "page" } else { "false" })
                    aria-label="Loot" title="Loot" {
                    span class="service-tab-icon service-tab-icon-loot" aria-hidden="true" {}
                }
            }
            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    (character_switcher(name))
                }
            }
        }
        script src="/static/strategic-time.js?v=client-clock-2" {}
    }
}

fn building_tint(settlement: &str, service: &str, material: &str) -> String {
    let hash = settlement
        .bytes()
        .chain([b':'])
        .chain(service.bytes())
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    let service_slot = match service {
        "map" => 0,
        "merchants" => 1,
        "weapons" => 2,
        "armor" => 3,
        "clothing" => 4,
        "herbalist" => 5,
        "inn" => 6,
        "religion" => 7,
        _ => (hash % 8) as usize,
    };
    let settlement_shift = ((hash >> 24) % 9) as u64;
    let hue = if material == "stone" {
        [205, 224, 252, 282, 164, 128, 36, 214][service_slot] + settlement_shift
    } else {
        [8, 20, 31, 43, 56, 104, 72, 350][service_slot] + settlement_shift
    };
    let saturation = if material == "stone" {
        12 + (hash >> 8) % 13
    } else {
        30 + (hash >> 8) % 25
    };
    let lightness = 19 + (hash >> 16) % 8;
    format!("hsl({hue} {saturation}% {lightness}%)")
}

fn building_tier(category: &SettlementCategory) -> &'static str {
    match category {
        SettlementCategory::Unknown | SettlementCategory::Hamlet | SettlementCategory::Village => {
            "village"
        }
        SettlementCategory::Town => "town",
        SettlementCategory::City | SettlementCategory::Capital => "city",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HorizonVariant {
    Inland,
    Coastal,
    River,
}

impl HorizonVariant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inland => "inland",
            Self::Coastal => "coastal",
            Self::River => "river",
        }
    }
}

/// Temporary stable scenery selection. Imported hydrology will replace only
/// this selector; settlement markup and CSS remain variant-driven.
fn horizon_variant(settlement_id: &str) -> HorizonVariant {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in settlement_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    match hash % 3 {
        0 => HorizonVariant::Inland,
        1 => HorizonVariant::Coastal,
        _ => HorizonVariant::River,
    }
}

fn character_switcher(name: &str) -> Markup {
    let initial = name.chars().next().unwrap_or('?');
    html! {
        details class="character-switcher" {
            summary class="character-switcher-toggle"
                aria-label=(format!("Character menu for {name}")) title=(name) {
                span class="party-portrait-initial character-switcher-portrait" aria-hidden="true" {
                    span class="party-portrait-face" { (initial) }
                }
            }
            div class="character-switcher-menu" {
                form action="/characters/switch" method="post" {
                    button type="submit" class="btn btn-small" { "Character select" }
                }
            }
        }
    }
}

/// Helper to build a left sidebar section
pub fn sidebar_section(title: &str, content: Markup) -> Markup {
    html! {
        div class="sidebar-section" {
            @if !title.is_empty() {
                h3 class="sidebar-header" { (title) }
            }
            (content)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HorizonVariant, building_tier, building_tint, horizon_variant, quest_location_top_bar,
        settlement_layout_with_session, settlement_top_bar,
    };
    use crate::spacetimedb::SettlementCategory;
    use maud::html;

    #[test]
    fn building_tints_are_stable_distinct_and_material_bounded() {
        assert_eq!(
            building_tint("lubeck", "inn", "wood"),
            building_tint("lubeck", "inn", "wood")
        );
        assert_ne!(
            building_tint("lubeck", "inn", "wood"),
            building_tint("lubeck", "weapons", "wood")
        );
        let wood_tints = [
            "map",
            "merchants",
            "weapons",
            "armor",
            "clothing",
            "herbalist",
            "inn",
            "religion",
        ]
        .map(|service| building_tint("lubeck", service, "wood"));
        assert_eq!(
            wood_tints
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            wood_tints.len()
        );
        let hue = |tint: String| tint[4..].split(' ').next().unwrap().parse::<u64>().unwrap();
        assert!((8..=80).contains(&hue(building_tint("lubeck", "inn", "wood"))));
        assert!((36..=290).contains(&hue(building_tint("lubeck", "inn", "stone"))));
    }

    #[test]
    fn settlement_categories_select_the_expected_building_tier() {
        for (category, tier) in [
            (SettlementCategory::Unknown, "village"),
            (SettlementCategory::Hamlet, "village"),
            (SettlementCategory::Village, "village"),
            (SettlementCategory::Town, "town"),
            (SettlementCategory::City, "city"),
            (SettlementCategory::Capital, "city"),
        ] {
            assert_eq!(building_tier(&category), tier);
            let markup =
                settlement_top_bar("Place", "p", &category, "map", None, None).into_string();
            assert!(markup.contains(&format!("data-building-tier=\"{tier}\"")));
        }
    }

    #[test]
    fn horizon_variants_are_stable_reachable_and_emitted() {
        assert_eq!(horizon_variant("lubeck"), horizon_variant("lubeck"));
        let variants = (0..128)
            .map(|id| horizon_variant(&format!("settlement-{id}")))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            variants,
            [
                HorizonVariant::Inland,
                HorizonVariant::Coastal,
                HorizonVariant::River
            ]
            .into_iter()
            .collect()
        );
        let expected = horizon_variant("stable-place").as_str();
        let markup = settlement_top_bar(
            "Stable Place",
            "stable-place",
            &SettlementCategory::Town,
            "map",
            None,
            None,
        )
        .into_string();
        assert!(markup.contains("data-building-tier=\"town\""));
        assert!(markup.contains(&format!("data-horizon-variant=\"{expected}\"")));
    }

    #[test]
    fn settlement_tabs_layer_village_buildings_beneath_accessible_service_icons() {
        let markup = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "religion",
            Some("roman_catholic"),
            None,
        )
        .into_string();
        assert_eq!(markup.matches("class=\"service-tab-building\"").count(), 8);
        assert_eq!(markup.matches("class=\"service-tab-icon ").count(), 8);
        assert!(markup.contains("aria-label=\"Church\""));
        assert!(markup.contains("aria-current=\"page\""));
        assert!(markup.contains("--service-tab-icon: url("));
        assert!(markup.contains("/static/icons/religion/catholic-crucifix.svg"));

        let css = include_str!("../../static/css/layout.css");
        for service in [
            "map",
            "merchants",
            "weapons",
            "armor",
            "clothing",
            "herbalist",
            "inn",
            "religion",
        ] {
            assert!(css.contains(&format!(
                "/static/styles/timber-framed/building/village/{service}.png"
            )));
        }
        assert!(css.contains("--service-building-image"));
        assert!(css.contains("data-building-tier"));
    }

    #[test]
    fn active_building_is_semantic_but_only_underlined_by_css() {
        let markup = settlement_layout_with_session(
            "Inn",
            "Lubeck",
            "lubeck",
            &SettlementCategory::City,
            "inn",
            None,
            html! {},
            None,
        )
        .into_string();
        assert!(markup.contains("aria-current=\"page\""));
        assert!(markup.contains("material-stone"));
        let css = include_str!("../../static/css/layout.css");
        assert!(css.contains("border-bottom: 3px solid var(--accent-light)"));
    }

    #[test]
    fn strategic_clock_is_snapshot_only_and_building_state_is_url_backed() {
        let time = include_str!("../../static/strategic-time.js");
        let building = include_str!("../../static/building-state.js");
        assert!(!time.contains("setInterval"));
        assert!(time.contains("applyLighting(characterMinutes)"));
        assert!(building.contains("searchParams.get(\"building\")"));
        assert!(building.contains("searchParams.set(\"building\", building)"));
        assert!(building.contains("pathname.includes(\"/party\")"));
        assert!(building.contains("partyInspection && services.has(requested)"));
        assert!(building.contains("!services.has(requested) || !partyInspection"));
    }

    #[test]
    fn wilderness_and_church_roots_match_the_active_tab_material() {
        let wilderness = quest_location_top_bar("Ruins", "q", "loot", None).into_string();
        assert!(wilderness.contains(":root{--active-building-tint:hsl(105 27% 19%);}"));
        let church = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "religion",
            None,
            None,
        )
        .into_string();
        assert!(church.contains(&format!(
            ":root{{--active-building-tint:{};}}",
            building_tint("s", "religion", "stone")
        )));
    }

    #[test]
    fn strategic_header_uses_portrait_menu_without_quest_or_party_labels() {
        let markup = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "map",
            None,
            Some("Ada"),
        )
        .into_string();
        assert!(markup.contains("class=\"character-switcher\""));
        assert!(markup.contains("Character menu for Ada"));
        assert!(markup.contains("character-switcher-portrait"));
        assert!(markup.contains("Character select"));
        assert!(markup.contains("action=\"/characters/switch\""));
        assert!(!markup.contains("Party: "));
        assert!(!markup.contains("data-current-quest"));
        assert!(!markup.contains("current-quest.js"));
        assert!(!markup.contains("data-settlement-turn-in-badge"));
        assert!(markup.contains("data-map-quest-badge"));
    }

    #[test]
    fn custom_theme_feature_is_absent() {
        let session = include_str!("../session.rs");
        let routes = include_str!("../routes/mod.rs");
        let layout = include_str!("layout.rs");
        assert!(!session.contains("THEME_COOKIE"));
        assert!(!session.contains("enum Theme"));
        assert!(!routes.contains("/theme/{name}"));
        assert!(!layout.contains("details class=\"theme-switcher\""));
        assert!(layout.contains("/static/css/base.css"));
    }
}
