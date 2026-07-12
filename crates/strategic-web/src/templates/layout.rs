//! Base layout template - Three-column design with theme support

use maud::{html, Markup, DOCTYPE};

const THEMES: &[(&str, &str)] = &[
    ("fraktur-texturina", "Fraktura"),
    ("fraktur-nocturne", "Dark Fraktura"),
    ("dark-arcanum", "Dark Arcanum"),
    ("northern-frost", "Northern Frost"),
    ("verdant-chronicle", "Verdant Chronicle"),
    ("imperial-crimson", "Imperial Crimson"),
];

fn validated_theme(theme: &str) -> &str {
    if THEMES.iter().any(|(id, _)| *id == theme) {
        theme
    } else {
        "fraktur-nocturne"
    }
}

/// Base HTML layout with three-column grid
pub fn base_layout(title: &str, content: Markup, theme: &str) -> Markup {
    base_layout_with_session(title, content, None, theme)
}

/// Base HTML layout with session info and theme
pub fn base_layout_with_session(
    title: &str,
    content: Markup,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let theme = validated_theme(theme);

    page_shell(title, top_bar(logged_in_as, theme), content, theme)
}

/// Minimal shell for selecting or creating a character. It intentionally omits
/// strategic navigation: an adventurer must be selected before play begins.
pub fn entry_layout(title: &str, content: Markup, theme: &str) -> Markup {
    let theme = validated_theme(theme);
    page_shell(title, entry_top_bar(theme), content, theme)
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
    )
}

fn page_shell(title: &str, header: Markup, content: Markup, theme: &str) -> Markup {
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
                link rel="stylesheet" href="/static/css/layout.css?v=theme-dropdown-1";
                link rel="stylesheet" href="/static/css/components.css?v=semantic-stat-icons-1";

                // Datastar
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar/bundles/datastar.js" {}
                script src="/static/party-trade.js" {}
            }
            body {
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

/// Fragment layout for Datastar partial updates (no full page shell)
pub fn fragment(content: Markup) -> Markup {
    content
}

fn top_bar(logged_in_as: Option<&str>, current_theme: &str) -> Markup {
    html! {
        header class="top-bar" {
            div class="top-bar-left" {
                h1 class="logo" {
                    a href="/" { "Adventure Simulator" }
                }
            }

            nav class="top-bar-center" {
                a href="/" class="nav-tab" { "Home" }
                a href="/settlements" class="nav-tab" { "Settlements" }
                a href="/quests" class="nav-tab" { "Quests" }
                a href="/parties" class="nav-tab" { "Parties" }
                a href="/characters" class="nav-tab" { "Characters" }
            }

            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    span class="player-name" {
                        "Playing as " strong { (name) }
                    }
                    (switch_character_button())
                } @else {
                    span class="player-name player-name-none" {
                        "No character"
                    }
                }

                (theme_switcher(current_theme))
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
        ("noticeboard", "Notice Board", "noticeboard"),
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
                a href=(format!("/settlements/{}", settlement_id)) class="settlement-name" {
                    (settlement_name)
                }
                span class="settlement-time" title="TODO: connect this display to strategic world time" {
                    "1st of First Seed · 08:00"
                }
            }

            nav class="top-bar-center settlement-services" aria-label="Settlement services" {
                @for (path, label, icon) in services {
                    @let href = if path.is_empty() {
                        format!("/settlements/{}", settlement_id)
                    } else {
                        format!("/settlements/{}/{}", settlement_id, path)
                    };
                    a href=(href)
                        class=(if active_service == path { "nav-tab active" } else { "nav-tab" })
                        aria-label=(label)
                        title=(label)
                    {
                        span class=(format!("service-tab-icon service-tab-icon-{}", icon)) aria-hidden="true" {}
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

/// Helper for settlement service menu
pub fn service_menu(settlement_id: &str, active: &str) -> Markup {
    let items = [
        ("noticeboard", "Notice Board"),
        ("merchants", "Merchants"),
        ("weapons", "Weapons"),
        ("armor", "Armour"),
        ("clothing", "Clothing"),
        ("inn", "Inn"),
        ("religion", "Church"),
    ];

    html! {
        nav class="service-menu" {
            @for (path, label) in items {
                @let href = if path.is_empty() {
                    format!("/settlements/{}", settlement_id)
                } else {
                    format!("/settlements/{}/{}", settlement_id, path)
                };
                @let is_active = active == path;
                a href=(href)
                    class=(if is_active { "service-menu-item active" } else { "service-menu-item" })
                {
                    (label)
                }
            }
        }
    }
}
