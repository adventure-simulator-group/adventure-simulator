//! Base layout template - Three-column strategic design.

use crate::spacetimedb::SettlementCategory;
use maud::{DOCTYPE, Markup, html};

use super::religion_game_icon_name;
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptProfile {
    Entry,
    Live,
    Strategic,
}

/// Minimal shell for selecting or creating a character. It intentionally omits
/// strategic navigation: an adventurer must be selected before play begins.
pub fn entry_layout(title: &str, content: Markup, theme: &str) -> Markup {
    let _ = theme;
    page_shell(title, entry_top_bar(), content, ScriptProfile::Entry)
}

/// Transitional shell shown while a tactical server is being allocated.
pub fn mission_layout(
    title: &str,
    content: Markup,
    logged_in_as: Option<&str>,
    _theme: &str,
) -> Markup {
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
    theme: &str,
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
            theme,
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
    theme: &str,
) -> Markup {
    page_shell(
        title,
        quest_location_top_bar(location_name, location_id, active_tab, logged_in_as, theme),
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

                link rel="stylesheet" href="/static/css/base.css?v=environment-1";
                // Shared CSS
                link rel="stylesheet" href="/static/css/reset.css";
                link rel="stylesheet" href="/static/css/layout.css?v=game-icons-2";
                link rel="stylesheet" href="/static/css/components.css?v=game-icons-2";
                link rel="stylesheet" href="/static/css/strategic.css?v=alchemy-medication-1";
                link rel="stylesheet" href="/static/css/utilities.css?v=inventory-dynamic-transfer-2";

                // Datastar
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar/bundles/datastar.js" {}
                script src="/static/background-fetch.js?v=background-fetch-1" {}
                script src="/static/medical-examination.js?v=one-shot-1" defer {}
                @if scripts != ScriptProfile::Entry {
                    script src="/static/live-state.js?v=sse-3" defer {}
                    script src="/static/live-regions.js?v=schedule-pending-2" defer {}
                }
                @if scripts == ScriptProfile::Strategic {
                    script src="/static/party-trade.js?v=inventory-dynamic-transfer-2" {}
                    script src="/static/equipment-toggle.js?v=functional-equipment-1" defer {}
                    script src="/static/party-notifications.js?v=standing-leadership-votes-5" defer {}
                    script src="/static/party-recruitment.js?v=party-recruitment-live-3" defer {}
                    script src="/static/service-quests.js?v=herbalist-care-2" defer {}
                    script src="/static/chat-resize.js?v=floating-chat-3" defer {}
                    script src="/static/local-chat.js?v=herbalist-private-1" defer {}
                    script src="/static/strategic-condition.js?v=strategic-condition-3" defer {}
                    script src="/static/building-state.js?v=environment-1" defer {}
                    script src="/static/travel-planner.js?v=journey-state-1" defer {}
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
    _current_theme: &str,
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
        @let active_tint = building_tint(settlement_id, active_service, material);
        style { (format!(":root{{--active-building-tint:{active_tint};}}")) }
        header class=(format!("top-bar settlement-top-bar material-{material}")) data-environment="settlement" {
            div class="top-bar-left settlement-location" {
                div class="settlement-identity" {
                a href=(format!("/locations/settlement/{}", settlement_id)) class="settlement-name" {
                    (settlement_name)
                }
                span class="settlement-turn-in-badge" data-settlement-turn-in-badge hidden
                    title="A quest is ready to turn in here" aria-label="Quest ready to turn in" { "!" }
                span class="settlement-time" data-player-time title="Loading official time…" {
                    "1st of First Seed · 08:00"
                }
                }
                (current_quest_summary())
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
                        span
                            class=(format!("service-tab-icon service-tab-icon-{}", icon))
                            style=[(path == "religion").then(|| format!("--service-tab-icon: url('/static/icons/game/{}.svg')", religion_game_icon_name(religion_id)))]
                            aria-hidden="true" {}
                        @if path != "map" {
                            span class="service-notification-badge service-quest-badge" data-service-quest-badge hidden { "!" }
                        }
                    }
                }
            }

            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    span class="player-name" { "Party: " strong { (name) } }
                    (switch_character_button())
                } @else {
                    span class="player-name player-name-none" { "No active party" }
                }
            }
        }
        script src="/static/strategic-time.js?v=client-clock-2" {}
        script src="/static/current-quest.js?v=current-quest-status-2" defer {}
    }
}

fn quest_location_top_bar(
    location_name: &str,
    location_id: &str,
    active_tab: &str,
    logged_in_as: Option<&str>,
    _current_theme: &str,
) -> Markup {
    html! {
        style { ":root{--active-building-tint:hsl(126 28% 18%);}" }
        header class="top-bar settlement-top-bar quest-location-top-bar" data-environment="wilderness" {
            div class="top-bar-left settlement-location" {
                div class="settlement-identity" {
                    a href=(format!("/locations/quest/{}", location_id)) class="settlement-name" { (location_name) }
                    span class="settlement-time" data-player-time { "1st of First Seed · 08:00" }
                }
                (current_quest_summary())
            }
            nav class="top-bar-center settlement-services" aria-label="Location views" {
                a href=(format!("/locations/quest/{}/map", location_id))
                    class=(if active_tab == "map" { "nav-tab active" } else { "nav-tab" })
                    style="--building-tint:hsl(126 30% 22%)"
                    aria-current=(if active_tab == "map" { "page" } else { "false" })
                    aria-label="Map" title="Map" {
                    span class="service-tab-icon service-tab-icon-map" aria-hidden="true" {}
                }
                a href=(format!("/locations/quest/{}/loot", location_id))
                    class=(if active_tab == "loot" { "nav-tab active" } else { "nav-tab" })
                    style="--building-tint:hsl(105 27% 19%)"
                    aria-current=(if active_tab == "loot" { "page" } else { "false" })
                    aria-label="Loot" title="Loot" {
                    span class="service-tab-icon service-tab-icon-loot" aria-hidden="true" {}
                }
            }
            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    span class="player-name" { "Party: " strong { (name) } }
                    (switch_character_button())
                } @else {
                    span class="player-name player-name-none" { "No active party" }
                }
            }
        }
        script src="/static/strategic-time.js?v=client-clock-2" {}
        script src="/static/current-quest.js?v=current-quest-status-2" defer {}
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
    let hue = if material == "stone" {
        205 + hash % 21
    } else {
        24 + hash % 18
    };
    let saturation = if material == "stone" {
        7 + (hash >> 8) % 9
    } else {
        28 + (hash >> 8) % 17
    };
    let lightness = 19 + (hash >> 16) % 8;
    format!("hsl({hue} {saturation}% {lightness}%)")
}

fn current_quest_summary() -> Markup {
    html! {
        div class="current-quest-summary" data-current-quest hidden {
            span class="current-quest-status" data-current-quest-status
                title="Quest in progress" aria-label="Quest in progress" { "!" }
            span class="current-quest-name" data-current-quest-name {}
            form class="current-quest-abandon" data-current-quest-abandon method="post" action="/quests" {
                button type="submit" class="btn btn-danger btn-small" { "Abandon quest" }
            }
        }
    }
}

fn switch_character_button() -> Markup {
    html! {
        form action="/characters/switch" method="post" {
            button type="submit" class="btn btn-secondary btn-small" { "Switch character" }
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
    use super::{building_tint, settlement_layout_with_session};
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
        let hue = |tint: String| tint[4..].split(' ').next().unwrap().parse::<u64>().unwrap();
        assert!((24..=41).contains(&hue(building_tint("lubeck", "inn", "wood"))));
        assert!((205..=225).contains(&hue(building_tint("lubeck", "inn", "stone"))));
    }

    #[test]
    fn active_building_is_semantic_but_only_underlined_by_css() {
        let markup = settlement_layout_with_session(
            "Inn",
            "Lubeck",
            "lubeck",
            &SettlementCategory::City,
            "inn",
            html! {},
            None,
            "",
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
