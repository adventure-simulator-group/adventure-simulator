//! Base layout template - Three-column design with theme support

use maud::{html, Markup, DOCTYPE};

const THEMES: &[(&str, &str)] = &[
    ("renaissance-gold", "Renaissance Gold"),
    ("dark-arcanum", "Dark Arcanum"),
    ("northern-frost", "Northern Frost"),
    ("verdant-chronicle", "Verdant Chronicle"),
    ("imperial-crimson", "Imperial Crimson"),
];

fn validated_theme(theme: &str) -> &str {
    if THEMES.iter().any(|(id, _)| *id == theme) {
        theme
    } else {
        "renaissance-gold"
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
                link rel="stylesheet" href="/static/css/layout.css";
                link rel="stylesheet" href="/static/css/components.css";

                // Datastar
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar/bundles/datastar.js" {}
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
                    form action="/characters/logout" method="post" style="display:inline" {
                        button type="submit" class="btn btn-small" title="Switch Character" {
                            "Switch"
                        }
                    }
                } @else {
                    span class="player-name player-name-none" {
                        "No character"
                    }
                }

                div class="theme-switcher" {
                    @for (id, _label) in THEMES {
                        a href=(format!("/theme/{}", id))
                            class=(if *id == current_theme { "theme-dot active" } else { "theme-dot" })
                            data-theme=(id)
                            title=(_label)
                        {}
                    }
                }
            }
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
        ("", "Overview"),
        ("noticeboard", "Notice Board"),
        ("tavern", "Tavern"),
        ("merchants", "Market"),
        ("smith", "Smith"),
        ("inn", "Inn"),
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
                @for (path, label) in services {
                    @let href = if path.is_empty() {
                        format!("/settlements/{}", settlement_id)
                    } else {
                        format!("/settlements/{}/{}", settlement_id, path)
                    };
                    a href=(href)
                        class=(if active_service == path { "nav-tab active" } else { "nav-tab" })
                    { (label) }
                }
            }

            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    span class="player-name" { "Party: " strong { (name) } }
                } @else {
                    span class="player-name player-name-none" { "No active party" }
                }
                (theme_switcher(current_theme))
            }
        }
    }
}

fn theme_switcher(current_theme: &str) -> Markup {
    html! {
        div class="theme-switcher" {
            @for (id, _label) in THEMES {
                a href=(format!("/theme/{}", id))
                    class=(if *id == current_theme { "theme-dot active" } else { "theme-dot" })
                    data-theme=(id)
                    title=(_label)
                {}
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
        ("", "Overview"),
        ("noticeboard", "Notice Board"),
        ("tavern", "Tavern"),
        ("merchants", "Merchants"),
        ("smith", "Smith"),
        ("inn", "Inn"),
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
