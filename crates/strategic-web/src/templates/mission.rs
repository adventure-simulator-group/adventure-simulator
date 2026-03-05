//! Mission status templates

use maud::{html, Markup};

use super::{base_layout_with_session, divider, panel, sidebar_section, status_badge};
use crate::spacetimedb::TacticalServer;

/// Mission status page with Datastar polling
pub fn mission_status_page(
    server: &TacticalServer,
    deployment_status: Option<&str>,
    deployment_error: Option<&str>,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let status = effective_status(&server.status, deployment_status);

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
                        div { (status_badge(&status)) }
                    }
                }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Mission Status" }

            div #"mission-status" {
                @match status.as_str() {
                    "Searching" => { (searching_state(&server.mission_id)) }
                    "Deploying" => { (deploying_state(&server.mission_id)) }
                    "Ready" => { (ready_state(server)) }
                    "Failed" => { (failed_state(deployment_error)) }
                    "Ended" => { (ended_state()) }
                    _ => { (pending_state(&server.mission_id)) }
                }
            }
        }

        aside class="right-sidebar" {
            @if status == "Ready" {
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

    base_layout_with_session("Mission Status", content, logged_in_as, theme)
}

fn effective_status(db_status: &str, deployment_status: Option<&str>) -> String {
    let normalized = db_status.to_lowercase();
    if normalized.contains("ready") {
        return "Ready".to_string();
    }
    if normalized.contains("failed") {
        return "Failed".to_string();
    }
    if normalized.contains("ended") {
        return "Ended".to_string();
    }

    match deployment_status.map(|s| s.to_ascii_uppercase()) {
        Some(s) if s.contains("ERROR") || s.contains("FAIL") || s.contains("CANCEL") => {
            "Failed".to_string()
        }
        Some(s)
            if s.contains("QUEUE")
                || s.contains("SEEK")
                || s.contains("SEARCH")
                || s.contains("PLACE") =>
        {
            "Searching".to_string()
        }
        Some(s) if s.contains("READY") || s.contains("DEPLOY") || s.contains("ASSIGN") => {
            "Deploying".to_string()
        }
        _ => "Pending".to_string(),
    }
}

fn pending_state(mission_id: &str) -> Markup {
    let poll_url = format!("@get('/missions/{}/status?fragment=true')", mission_id);
    html! {
        (panel("Waiting", html! {
            div data-on-load=(&poll_url) "data-on-interval__5000ms"=(&poll_url) {
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

fn searching_state(mission_id: &str) -> Markup {
    let poll_url = format!("@get('/missions/{}/status?fragment=true')", mission_id);
    html! {
        (panel("Searching", html! {
            div data-on-load=(&poll_url) "data-on-interval__3000ms"=(&poll_url) {
                div class="status-message" {
                    div class="loading-spinner" { "Finding deployment region" }
                    p class="status-detail mt-1" {
                        "Edgegap is selecting placement and allocating compute."
                    }
                }
            }
        }))
        (cancel_button(mission_id))
    }
}

fn deploying_state(mission_id: &str) -> Markup {
    let poll_url = format!("@get('/missions/{}/status?fragment=true')", mission_id);
    html! {
        (panel("Deploying", html! {
            div data-on-load=(&poll_url) "data-on-interval__3000ms"=(&poll_url) {
                div class="status-message" {
                    div class="loading-spinner" { "Starting server" }
                    p class="status-detail mt-1" {
                        "Deployment assigned. Waiting for tactical-server to register."
                    }
                }
            }
        }))
        (cancel_button(mission_id))
    }
}

fn failed_state(error: Option<&str>) -> Markup {
    html! {
        (panel("Mission Failed", html! {
            div class="status-message" {
                p class="text-danger" style="font-weight:600" { "Mission deployment failed." }
                @if let Some(err) = error {
                    p class="status-detail" { (err) }
                }
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
                a href=(format!("/play?addr={}&cert={}", server.addr, server.cert_digest))
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
pub fn mission_status_fragment(
    server: &TacticalServer,
    deployment_status: Option<&str>,
    deployment_error: Option<&str>,
) -> Markup {
    let status = effective_status(&server.status, deployment_status);

    html! {
        div #"mission-status" {
            @match status.as_str() {
                "Searching" => { (searching_state(&server.mission_id)) }
                "Deploying" => { (deploying_state(&server.mission_id)) }
                "Ready" => { (ready_state(server)) }
                "Failed" => { (failed_state(deployment_error)) }
                "Ended" => { (ended_state()) }
                _ => { (pending_state(&server.mission_id)) }
            }
        }
    }
}
