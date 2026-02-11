//! Mission status templates

use maud::{html, Markup};

use super::{base_layout_with_session, card, status_badge};
use crate::spacetimedb::TacticalServer;

/// Mission status page with Datastar polling
pub fn mission_status_page(
    server: &TacticalServer,
    deployment_status: Option<&str>,
    deployment_error: Option<&str>,
    logged_in_as: Option<&str>,
) -> Markup {
    let status = effective_status(&server.status, deployment_status);

    let content = html! {
        div class="mission-status-page" {
            div class="page-header" {
                h2 { "Mission Status" }
                (status_badge(&status))
            }

            div class="mission-grid" {
                (card("Mission Info", html! {
                    div class="detail-grid" {
                        div class="detail" {
                            span class="detail-label" { "Mission ID" }
                            span class="detail-value" { (server.mission_id) }
                        }
                        div class="detail" {
                            span class="detail-label" { "Scene" }
                            span class="detail-value" { (server.scene_key) }
                        }
                    }
                }))

                div #"mission-status" {
                    @match status.as_str() {
                        "Searching" => {
                            (searching_state(&server.mission_id))
                        }
                        "Deploying" => {
                            (deploying_state(&server.mission_id))
                        }
                        "Ready" => {
                            (ready_state(server))
                        }
                        "Failed" => {
                            (failed_state(deployment_error))
                        }
                        "Ended" => {
                            (ended_state())
                        }
                        _ => {
                            (pending_state(&server.mission_id))
                        }
                    }
                }
            }
        }
    };

    base_layout_with_session("Mission Status", content, logged_in_as)
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
        (card("Status", html! {
            div data-on-load=(&poll_url) "data-on-interval__5000ms"=(&poll_url) {
                div class="status-message" {
                    p { "Waiting for server..." }
                    p class="status-detail" { "A tactical server is being prepared for your mission." }
                }
            }
        }))

        (cancel_button(mission_id))
    }
}

fn searching_state(mission_id: &str) -> Markup {
    let poll_url = format!("@get('/missions/{}/status?fragment=true')", mission_id);
    html! {
        (card("Status", html! {
            div data-on-load=(&poll_url) "data-on-interval__3000ms"=(&poll_url) {
                div class="status-message" {
                    p { "Finding deployment region for your party..." }
                    p class="status-detail" { "Edgegap is selecting placement and allocating compute." }
                    div class="loading-animation" {
                        span class="loading" { "..." }
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
        (card("Status", html! {
            div data-on-load=(&poll_url) "data-on-interval__3000ms"=(&poll_url) {
                div class="status-message" {
                    p { "Deployment assigned, starting server..." }
                    p class="status-detail" { "Waiting for tactical-server to register its connection info." }
                    div class="loading-animation" {
                        span class="loading" { "..." }
                    }
                }
            }
        }))

        (cancel_button(mission_id))
    }
}

fn failed_state(error: Option<&str>) -> Markup {
    html! {
        (card("Mission Failed", html! {
            div class="status-message" {
                p { "Mission deployment failed." }
                @if let Some(err) = error {
                    p class="status-detail" { (err) }
                }
                a href="/" class="btn btn-primary" data-on-click="@get('/')" {
                    "Return Home"
                }
            }
        }))
    }
}

fn ready_state(server: &TacticalServer) -> Markup {
    html! {
        (card("Server Ready", html! {
            div class="status-message success" {
                p { "Your tactical server is ready!" }
                div class="connection-info" {
                    div class="detail" {
                        span class="detail-label" { "Server Address" }
                        span class="detail-value" { (server.addr) }
                    }
                    @if !server.cert_digest.is_empty() {
                        div class="detail" {
                            span class="detail-label" { "Certificate" }
                            span class="detail-value cert-digest" { (server.cert_digest) }
                        }
                    }
                }
                a href=(format!("/play?addr={}&cert={}", server.addr, server.cert_digest))
                    class="btn btn-primary btn-large"
                {
                    "Connect to Mission"
                }
            }
        }))
    }
}

fn ended_state() -> Markup {
    html! {
        (card("Mission Complete", html! {
            div class="status-message" {
                p { "This mission has ended." }
                a href="/" class="btn btn-primary" data-on-click="@get('/')" {
                    "Return Home"
                }
            }
        }))
    }
}

fn cancel_button(mission_id: &str) -> Markup {
    html! {
        div class="mission-actions" {
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
                "Searching" => {
                    (searching_state(&server.mission_id))
                }
                "Deploying" => {
                    (deploying_state(&server.mission_id))
                }
                "Ready" => {
                    (ready_state(server))
                }
                "Failed" => {
                    (failed_state(deployment_error))
                }
                "Ended" => {
                    (ended_state())
                }
                _ => {
                    (pending_state(&server.mission_id))
                }
            }
        }
    }
}
