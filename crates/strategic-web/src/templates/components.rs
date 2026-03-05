//! Reusable Maud components

use maud::{html, Markup};

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

/// A clickable list item
pub fn list_item(href: &str, title: &str, subtitle: Option<&str>) -> Markup {
    html! {
        a href=(href) class="list-item" {
            div class="list-item-content" {
                span class="list-item-title" { (title) }
                @if let Some(sub) = subtitle {
                    span class="list-item-subtitle" { (sub) }
                }
            }
            span class="list-item-arrow" { "\u{203A}" }
        }
    }
}

/// A button component
pub fn button(label: &str, btn_type: &str, extra_class: Option<&str>) -> Markup {
    let class = format!(
        "btn btn-{}{}",
        btn_type,
        extra_class.map(|c| format!(" {}", c)).unwrap_or_default()
    );
    html! {
        button type="submit" class=(class) {
            (label)
        }
    }
}

/// A loading indicator
pub fn loading_indicator(id: &str) -> Markup {
    html! {
        span id=(id) class="loading-spinner" hidden {
            "Loading"
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

/// A select dropdown
pub fn select_field(
    name: &str,
    label: &str,
    options: &[(&str, &str)],
    selected: Option<&str>,
) -> Markup {
    html! {
        div class="form-group" {
            label for=(name) class="form-label" { (label) }
            select id=(name) name=(name) {
                @for (value, text) in options {
                    option value=(value) selected[Some(*value) == selected] {
                        (text)
                    }
                }
            }
        }
    }
}

/// Difficulty stars display
pub fn difficulty_stars(level: i32) -> Markup {
    html! {
        span class="difficulty" {
            @for i in 1..=5 {
                @if i <= level {
                    span class="star filled" { "\u{2605}" }
                } @else {
                    span class="star empty" { "\u{2606}" }
                }
            }
        }
    }
}

/// Gold amount display
pub fn gold_display(amount: i32) -> Markup {
    html! {
        span class="gold-amount" {
            (amount)
            span class="gold-icon" {}
        }
    }
}

/// XP display
pub fn xp_display(amount: i32) -> Markup {
    html! {
        span class="xp-amount" {
            (amount)
            " XP"
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

/// Ornate divider
pub fn divider() -> Markup {
    html! {
        div class="divider" {}
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
