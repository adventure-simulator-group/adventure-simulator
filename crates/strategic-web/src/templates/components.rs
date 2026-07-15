//! Reusable Maud components

use maud::{Markup, html};

/// A panel component with header and body
pub fn panel(title: &str, content: Markup) -> Markup {
    html! {
        div class="panel" {
            @if !title.is_empty() {
                div class="panel-header" { (title) }
            }
            div class="panel-body" {
                (content)
            }
        }
    }
}

/// A form input field
pub fn input_field(
    name: &str,
    label: &str,
    input_type: &str,
    required: bool,
    value: Option<&str>,
) -> Markup {
    html! {
        div class="form-group" {
            label for=(name) class="form-label" { (label) }
            input
                type=(input_type)
                id=(name)
                name=(name)
                required[required]
                value=[value];
        }
    }
}

/// Gold amount display
pub fn gold_display(amount: impl std::fmt::Display) -> Markup {
    html! {
        span class="gold-amount" {
            (amount)
            span class="gold-icon" {}
        }
    }
}

/// Status badge
pub fn status_badge(status: &str) -> Markup {
    let class = match status.to_lowercase().as_str() {
        "available" => "badge badge-success",
        "accepted" => "badge badge-warning",
        "completed" => "badge badge-info",
        "ready" => "badge badge-success",
        "pending" | "searching" | "deploying" => "badge badge-warning",
        "failed" => "badge badge-danger",
        "ended" => "badge badge-info",
        _ => "badge",
    };
    html! {
        span class=(class) { (status) }
    }
}

/// Empty state placeholder
pub fn empty_state(message: &str, action_href: Option<&str>, action_label: Option<&str>) -> Markup {
    html! {
        div class="empty-state" {
            p { (message) }
            @if let (Some(href), Some(label)) = (action_href, action_label) {
                a href=(href) class="btn btn-primary" {
                    (label)
                }
            }
        }
    }
}

/// Population level description
pub fn population_description(level: i32) -> &'static str {
    match level {
        1 => "Hamlet",
        2 => "Village",
        3 => "Town",
        4 => "City",
        5 => "Capital",
        _ => "Unknown",
    }
}
