//! Base layout template

use maud::{html, Markup, DOCTYPE};

/// Base HTML layout with header, nav, and content area
pub fn base_layout(title: &str, content: Markup) -> Markup {
    base_layout_with_session(title, content, None)
}

/// Base HTML layout with session info for "logged in as" display
pub fn base_layout_with_session(title: &str, content: Markup, logged_in_as: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Adventure Simulator" }

                // CSS
                link rel="stylesheet" href="/static/css/variables.css";
                link rel="stylesheet" href="/static/css/typography.css";
                link rel="stylesheet" href="/static/css/parchment.css";
                link rel="stylesheet" href="/static/css/borders.css";
                link rel="stylesheet" href="/static/css/global.css";
                link rel="stylesheet" href="/static/css/responsive.css";
                link rel="stylesheet" href="/static/css/layout.css";

                // Datastar
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar/bundles/datastar.js" {}
            }
            body class="parchment-bg" {
                div class="app-container" {
                    // Header
                    (header_component(logged_in_as))

                    // Navigation tabs
                    (nav_tabs())

                    // Main content
                    main class="main-content" {
                        (content)
                    }

                    // Footer
                    (footer_component())
                }
            }
        }
    }
}

/// Fragment layout for Datastar partial updates (no full page shell)
pub fn fragment(content: Markup) -> Markup {
    content
}

fn header_component(logged_in_as: Option<&str>) -> Markup {
    html! {
        header class="app-header" {
            div class="header-left" {
                h1 class="game-title" { "Adventure Simulator" }
            }
            div class="header-center" {
                // Logged in status
                @if let Some(name) = logged_in_as {
                    span class="logged-in-status" {
                        "Playing as: "
                        strong { (name) }
                    }
                } @else {
                    span class="logged-in-status not-logged-in" {
                        "No character selected"
                    }
                }
            }
            div class="header-right" {
                @if logged_in_as.is_some() {
                    form action="/characters/logout" method="post" style="display:inline" {
                        button type="submit" class="btn btn-small" title="Switch Character" {
                            "Switch"
                        }
                    }
                }
                button class="btn btn-icon" title="Settings" {
                    span { "\u{2699}" } // Gear icon
                }
            }
        }
    }
}

fn nav_tabs() -> Markup {
    html! {
        nav class="tab-nav" {
            a href="/" class="tab" data-on-click="@get('/')" {
                "Home"
            }
            a href="/characters" class="tab" data-on-click="@get('/characters')" {
                "Characters"
            }
            a href="/settlements" class="tab" data-on-click="@get('/settlements')" {
                "Settlements"
            }
            a href="/parties" class="tab" data-on-click="@get('/parties')" {
                "Parties"
            }
            a href="/quests" class="tab" data-on-click="@get('/quests')" {
                "Quests"
            }
        }
    }
}

fn footer_component() -> Markup {
    html! {
        footer class="app-footer" {
            p { "Adventure Simulator - Strategic Layer" }
        }
    }
}
