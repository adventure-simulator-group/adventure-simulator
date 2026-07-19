//! Mission status templates

use maud::{Markup, html};

use super::{mission_layout, panel, sidebar_section, status_badge};
use crate::spacetimedb::{MissionStatus, TacticalServer};

/// Mission status page updated by the shared strategic SSE stream.
pub fn mission_status_page(server: &TacticalServer, logged_in_as: Option<&str>) -> Markup {
    let status = server.status;

    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Mission Info", html! {
                div class="flex flex-col gap-sm" {
                    div {
                        span class="detail-label" { "Mission ID" }
                        p class="detail-value" style="font-size:var(--font-size-xs);word-break:break-all" {
                            (server.mission_id)
                        }
                    }
                    div {
                        span class="detail-label" { "Scene" }
                        p class="detail-value" { (server.scene_key) }
                    }
                    div {
                        span class="detail-label" { "Status" }
                        div { (status_badge(status.label())) }
                    }
                }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Mission Status" }

            div # "mission-status"
                data-live-refresh
                "data-on:strategic-live-update"=(format!("@get('/missions/{}/status?fragment=true')", server.mission_id)) {
                @match status {
                    MissionStatus::Ready => { (ready_state(server)) }
                    MissionStatus::Failed => { (failed_state()) }
                    MissionStatus::Ended => { (ended_state()) }
                    MissionStatus::Pending => { (pending_state(&server.mission_id)) }
                }
            }
        }

        aside class="right-sidebar" {
            @if status == MissionStatus::Ready {
                (sidebar_section("Connection", html! {
                    (panel("", html! {
                        div class="flex flex-col gap-sm" {
                            div {
                                span class="detail-label" { "Server" }
                                p class="detail-value" style="font-size:var(--font-size-xs)" { (server.addr) }
                            }
                            @if !server.cert_digest.is_empty() {
                                div {
                                    span class="detail-label" { "Certificate" }
                                    p class="detail-value cert-digest" { (server.cert_digest) }
                                }
                            }
                        }
                    }))
                }))
            } @else {
                (sidebar_section("Info", html! {
                    (panel("", html! {
                        p style="font-size:var(--font-size-sm)" {
                            "Your tactical mission is being prepared. "
                            "The page will update automatically."
                        }
                    }))
                }))
            }
        }
    };

    mission_layout("Mission Status", content, logged_in_as)
}

fn pending_state(mission_id: &str) -> Markup {
    html! {
        (panel("Waiting", html! {
            div {
                div class="status-message" {
                    div class="loading-spinner" { "Preparing server" }
                    p class="status-detail mt-1" {
                        "A tactical server is being prepared for your mission."
                    }
                }
            }
        }))
        (cancel_button(mission_id))
    }
}

fn failed_state() -> Markup {
    html! {
        (panel("Mission Failed", html! {
            div class="status-message" {
                p class="text-danger" style="font-weight:600" { "Mission failed." }
                a href="/" class="btn btn-primary mt-2" {
                    "Return Home"
                }
            }
        }))
    }
}

fn ready_state(server: &TacticalServer) -> Markup {
    html! {
        (panel("Server Ready", html! {
            div class="status-message success" {
                p class="text-success" style="font-weight:700;font-size:var(--font-size-lg)" {
                    "Your tactical server is ready!"
                }
                div class="connection-info mt-2" {
                    div class="detail-row" {
                        span class="detail-label" { "Server Address" }
                        span class="detail-value" { (server.addr) }
                    }
                    @if !server.cert_digest.is_empty() {
                        div class="detail-row" {
                            span class="detail-label" { "Certificate" }
                            span class="detail-value cert-digest" { (server.cert_digest) }
                        }
                    }
                }
                a href=(format!("/tactical/tactical.html?server={}&id={}&autostart=1", server.addr, server.character_id.unwrap_or_default()))
                    class="btn btn-primary btn-large btn-block mt-2"
                {
                    "Connect to Mission"
                }
            }
        }))
    }
}

fn ended_state() -> Markup {
    html! {
        (panel("Mission Complete", html! {
            div class="status-message" {
                p { "This mission has ended." }
                a href="/" class="btn btn-primary mt-1" {
                    "Return Home"
                }
            }
        }))
    }
}

fn cancel_button(mission_id: &str) -> Markup {
    html! {
        div class="mt-2" {
            form method="post" action=(format!("/missions/{}/cancel", mission_id)) {
                button type="submit" class="btn btn-danger" {
                    "Cancel Mission"
                }
            }
        }
    }
}

/// Fragment for Datastar polling updates
pub fn mission_status_fragment(server: &TacticalServer) -> Markup {
    let status = server.status;

    html! {
        div # "mission-status"
            data-live-refresh
            "data-on:strategic-live-update"=(format!("@get('/missions/{}/status?fragment=true')", server.mission_id)) {
            @match status {
                MissionStatus::Ready => { (ready_state(server)) }
                MissionStatus::Failed => { (failed_state()) }
                MissionStatus::Ended => { (ended_state()) }
                MissionStatus::Pending => { (pending_state(&server.mission_id)) }
            }
        }
    }
}
