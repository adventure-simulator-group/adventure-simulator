//! Base layout template - Three-column design with theme support

use maud::{DOCTYPE, Markup, html};

const THEMES: &[(&str, &str)] = &[
    ("fraktur-texturina", "Fraktura"),
    ("fraktur-nocturne", "Dark Fraktura"),
    ("dark-arcanum", "Dark Arcanum"),
    ("northern-frost", "Northern Frost"),
    ("verdant-chronicle", "Verdant Chronicle"),
    ("imperial-crimson", "Imperial Crimson"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptProfile {
    Entry,
    Live,
    Strategic,
}

fn validated_theme(theme: &str) -> &str {
    if THEMES.iter().any(|(id, _)| *id == theme) {
        theme
    } else {
        "fraktur-nocturne"
    }
}

/// Minimal shell for selecting or creating a character. It intentionally omits
/// strategic navigation: an adventurer must be selected before play begins.
pub fn entry_layout(title: &str, content: Markup, theme: &str) -> Markup {
    let theme = validated_theme(theme);
    page_shell(
        title,
        entry_top_bar(theme),
        content,
        theme,
        ScriptProfile::Entry,
    )
}

/// Transitional shell shown while a tactical server is being allocated.
pub fn mission_layout(
    title: &str,
    content: Markup,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let theme = validated_theme(theme);
    let header = html! {
        header class="top-bar entry-top-bar" {
            div class="top-bar-left" { h1 class="logo" { "Adventure Simulator" } }
            div class="entry-message" { "Preparing tactical mission" }
            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    span class="player-name" { strong { (name) } }
                }
                (theme_switcher(theme))
            }
        }
    };
    page_shell(title, header, content, theme, ScriptProfile::Live)
}

/// Settlement-specific layout. Settlement services replace the global navigation
/// so their context stays visible while the player moves between service pages.
pub fn settlement_layout_with_session(
    title: &str,
    settlement_name: &str,
    settlement_id: &str,
    active_service: &str,
    content: Markup,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let theme = validated_theme(theme);
    page_shell(
        title,
        settlement_top_bar(
            settlement_name,
            settlement_id,
            active_service,
            logged_in_as,
            theme,
        ),
        content,
        theme,
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
    let theme = validated_theme(theme);
    page_shell(
        title,
        quest_location_top_bar(location_name, location_id, active_tab, logged_in_as, theme),
        content,
        theme,
        ScriptProfile::Strategic,
    )
}

fn page_shell(
    title: &str,
    header: Markup,
    content: Markup,
    theme: &str,
    scripts: ScriptProfile,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Adventure Simulator" }

                // Theme CSS (loaded first so variables are available)
                link rel="stylesheet" href=(format!("/static/css/themes/{}.css", theme));
                // Shared CSS
                link rel="stylesheet" href="/static/css/reset.css";
                link rel="stylesheet" href="/static/css/layout.css?v=quest-status-2";
                link rel="stylesheet" href="/static/css/components.css?v=map-quest-status-1";
                link rel="stylesheet" href="/static/css/strategic.css?v=conviction-demand-1";
                link rel="stylesheet" href="/static/css/utilities.css?v=typed-frontend-1";

                // Datastar
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar/bundles/datastar.js" {}
                script src="/static/background-fetch.js?v=background-fetch-1" {}
                @if scripts != ScriptProfile::Entry {
                    script src="/static/live-state.js?v=sse-2" defer {}
                    script src="/static/live-regions.js?v=live-regions-2" defer {}
                }
                @if scripts == ScriptProfile::Strategic {
                    script src="/static/party-trade.js?v=live-control-init-1" {}
                    script src="/static/party-notifications.js?v=party-requests-2" defer {}
                    script src="/static/party-recruitment.js?v=party-recruitment-live-2" defer {}
                    script src="/static/service-quests.js?v=settlement-faith-1" defer {}
                    script src="/static/chat-resize.js?v=chat-resize-1" defer {}
                    script src="/static/local-chat.js?v=quest-links-live-2" defer {}
                    script src="/static/strategic-condition.js?v=strategic-condition-1" defer {}
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

fn entry_top_bar(current_theme: &str) -> Markup {
    html! {
        header class="top-bar entry-top-bar" {
            div class="top-bar-left" {
                h1 class="logo" { "Adventure Simulator" }
            }
            div class="entry-message" { "Choose an adventurer to begin" }
            div class="top-bar-right" { (theme_switcher(current_theme)) }
        }
    }
}

fn settlement_top_bar(
    settlement_name: &str,
    settlement_id: &str,
    active_service: &str,
    logged_in_as: Option<&str>,
    current_theme: &str,
) -> Markup {
    let services = [
        ("map", "Map", "map"),
        ("merchants", "General Market", "market"),
        ("weapons", "Weapons", "weapons"),
        ("armor", "Armour", "armor"),
        ("clothing", "Clothing", "clothing"),
        ("inn", "Inn", "inn"),
        ("religion", "Church", "church"),
    ];

    html! {
        header class="top-bar settlement-top-bar" {
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
                    a href=(href)
                        class=(if active_service == path { "nav-tab active" } else { "nav-tab" })
                        data-service-id=(path)
                        aria-label=(label)
                        title=(label)
                    {
                        span class=(format!("service-tab-icon service-tab-icon-{}", icon)) aria-hidden="true" {}
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
                (theme_switcher(current_theme))
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
    current_theme: &str,
) -> Markup {
    html! {
        header class="top-bar settlement-top-bar quest-location-top-bar" {
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
                    aria-label="Map" title="Map" {
                    span class="service-tab-icon service-tab-icon-map" aria-hidden="true" {}
                }
                a href=(format!("/locations/quest/{}/loot", location_id))
                    class=(if active_tab == "loot" { "nav-tab active" } else { "nav-tab" })
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
                (theme_switcher(current_theme))
            }
        }
        script src="/static/strategic-time.js?v=client-clock-2" {}
        script src="/static/current-quest.js?v=current-quest-status-2" defer {}
    }
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

fn theme_switcher(current_theme: &str) -> Markup {
    html! {
        details class="theme-switcher" {
            summary
                class=(format!("theme-dot active theme-current theme-{}", current_theme))
                data-theme=(current_theme)
                title="Choose theme"
                aria-label="Choose theme"
            {
                span class="sr-only" { "Choose theme" }
            }
            div class="theme-menu" {
                @for (id, label) in THEMES {
                a href=(format!("/theme/{}", id))
                    class=(if *id == current_theme { "theme-dot active" } else { "theme-dot" })
                    data-theme=(id)
                    title=(label)
                    aria-label=(label)
                {}
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
