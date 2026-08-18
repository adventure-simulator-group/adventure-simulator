//! Base layout template - Three-column strategic design.

use crate::spacetimedb::SettlementCategory;
use maud::{DOCTYPE, Markup, html};

use super::{organization_charge, organization_colors, religion_icon_path};
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptProfile {
    Entry,
    Live,
    Strategic,
}

/// Minimal shell for selecting or creating a character. It intentionally omits
/// strategic navigation: an adventurer must be selected before play begins.
pub fn entry_layout(title: &str, content: Markup) -> Markup {
    page_shell(title, entry_top_bar(), content, ScriptProfile::Entry)
}

/// Transitional shell shown while a tactical server is being allocated.
pub fn mission_layout(title: &str, content: Markup, logged_in_as: Option<&str>) -> Markup {
    let header = html! {
        header class="top-bar entry-top-bar" {
            div class="top-bar-left" { h1 class="logo" { "Fabelgeist" } }
            div class="entry-message" { "Tactical mission" }
            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    span class="player-name" { strong { (name) } }
                }
            }
        }
    };
    page_shell(title, header, content, ScriptProfile::Strategic)
}

pub fn journal_layout(content: Markup, logged_in_as: Option<&str>) -> Markup {
    page_shell(
        "Journal",
        entry_top_bar_with_session(logged_in_as),
        content,
        ScriptProfile::Strategic,
    )
}

/// A complete strategic shell for guard and error states reached from ordinary
/// navigation. Keeping these responses inside the application prevents a bad
/// URL or stale action from dropping the player into raw browser text.
pub fn strategic_notice_page(
    title: &str,
    message: &str,
    return_href: &str,
    return_label: &str,
    logged_in_as: Option<&str>,
) -> Markup {
    let content = html! {
        aside class="left-sidebar notice-rail" aria-hidden="true" {}
        main class="center-content strategic-notice-main" {
            section class="strategic-notice" role="alert" aria-labelledby="strategic-notice-title" {
                span class="strategic-notice-icon" aria-hidden="true" {}
                h2 id="strategic-notice-title" { (title) }
                p data-strategic-safe-message { (message) }
                a href=(return_href) class="btn btn-primary" { (return_label) }
            }
        }
        aside class="right-sidebar notice-rail" aria-hidden="true" {}
    };
    page_shell(
        title,
        entry_top_bar_with_session(logged_in_as),
        content,
        ScriptProfile::Live,
    )
}

fn entry_top_bar_with_session(logged_in_as: Option<&str>) -> Markup {
    html! {
        header class="top-bar entry-top-bar" {
            div class="top-bar-left" { h1 class="logo" { "Fabelgeist" } }
            div class="entry-message" { "The road ahead" }
            div class="top-bar-right" {
                @if let Some(name) = logged_in_as { span class="player-name" { strong { (name) } } }
            }
        }
    }
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
    economy: Option<&adventuresim_world_schema::SettlementEconomyProfile>,
    content: Markup,
    logged_in_as: Option<&str>,
) -> Markup {
    page_shell(
        title,
        settlement_top_bar(
            settlement_name,
            settlement_id,
            category,
            active_service,
            religion_id,
            economy,
            logged_in_as,
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
) -> Markup {
    page_shell(
        title,
        quest_location_top_bar(location_name, location_id, active_tab, false, logged_in_as),
        content,
        ScriptProfile::Strategic,
    )
}

/// Camp uses the wilderness location shell, while its current fire state is
/// derived by the caller from the persisted journey itinerary.
pub fn camp_location_layout_with_session(
    title: &str,
    location_name: &str,
    location_id: &str,
    camp_fire_lit: bool,
    content: Markup,
    logged_in_as: Option<&str>,
) -> Markup {
    page_shell(
        title,
        quest_location_top_bar(
            location_name,
            location_id,
            "camp",
            camp_fire_lit,
            logged_in_as,
        ),
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
                title { (title) " - Fabelgeist" }

                link rel="stylesheet" href="/static/css/base.css?v=environment-14";
                // Shared CSS
                link rel="stylesheet" href="/static/css/reset.css";
                link rel="stylesheet" href="/static/css/layout.css?v=organization-facades-1";
                link rel="stylesheet" href="/static/css/components.css?v=lowercase-display-type-1";
                link rel="stylesheet" href="/static/css/strategic.css?v=forge-preview-layering-3";
                link rel="stylesheet" href="/static/css/utilities.css?v=strategic-ui-overhaul-1";

                // Datastar
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar/bundles/datastar.js" {}
                script src="/static/background-fetch.js?v=background-fetch-2" {}
                script src="/static/developer-mode.js?v=development-clock-2" defer {}
                script src="/static/tooltips.js?v=delegated-mouseover-1" defer {}
                script src="/static/character-action-dialog.js?v=character-actions-1" defer {}
                @if scripts != ScriptProfile::Entry {
                    script src="/static/live-state.js?v=sse-4" defer {}
                    script src="/static/live-regions.js?v=preserved-client-regions-1" defer {}
                }
                @if scripts == ScriptProfile::Strategic {
                    script src="/static/strategic-navigation.js?v=soft-navigation-1" defer {}
                    script type="module" src="/static/strategic-renderer.js?v=model-owned-forge-controls-1" {}
                    script src="/static/strategic-mutations.js?v=formaction-override-1" defer {}
                    script src="/static/character-switcher.js?v=multi-character-switcher-1" defer {}
                    script src="/static/journal-tab.js?v=journal-tab-1" defer {}
                    script src="/static/numeric-editor.js?v=shared-numeric-editor-2" defer {}
                    script src="/static/inventory-browser.js?v=procedural-weapon-icons-1" defer {}
                    script src="/static/party-trade.js?v=provision-party-food-1-slot-controls-1" defer {}
                    script src="/static/cooking.js?v=fireplace-station-1" defer {}
                    script src="/static/herbalism.js?v=bounded-craft-1" defer {}
                    script src="/static/equipment-toggle.js?v=location-keyboard-slots-5" defer {}
                    script src="/static/party-notifications.js?v=standing-leadership-votes-5" defer {}
                script src="/static/party-recruitment.js?v=party-recruitment-live-3" defer {}
                script src="/static/physiology-dialog.js?v=visual-notebook-2" defer {}
                    script src="/static/service-quests.js?v=apprentice-system-1" defer {}
                    script src="/static/dialogue-client.js?v=location-fixtures-2" defer {}
                    script src="/static/physical-evidence.js?v=deterministic-inspection-1" defer {}
                    script src="/static/developer-quest-editor.js?v=scenario-gallery-1" defer {}
                    script src="/static/chat-resize.js?v=counterparty-portraits-1" defer {}
                    script src="/static/local-chat.js?v=local-chat-location-authority-1" defer {}
                    script src="/static/strategic-condition.js?v=strategic-condition-4" defer {}
                    script src="/static/building-state.js?v=fireplace-context-2" defer {}
                    script src="/static/travel-planner.js?v=travel-rails-1" defer {}
                    script src="/static/strategic-map.js?v=population-culling-3" defer {}
                    script src="/static/rest-duration.js?v=wake-time-4" defer {}
                    script src="/static/training-schedule.js?v=apprentice-system-1" defer {}
                    script src="/static/immediate-activity.js?v=manual-activities-1" defer {}
                }
            }
            body {
                @if scripts == ScriptProfile::Strategic {
                    div id="strategic-render-surface" hidden aria-hidden="true" {
                        canvas id="game-canvas" {}
                    }
                }
                @if scripts != ScriptProfile::Entry {
                    div id="strategic-live-stream" data-init="@get('/live')" {
                        span id="strategic-live-revision" data-live-revision="0" hidden {}
                    }
                }
                (maud::PreEscaped("<!-- strategic-page-start -->"))
                div class="app" id="strategic-page" data-page-title=(title)
                    data-script-profile=(match scripts { ScriptProfile::Entry => "entry", ScriptProfile::Live => "live", ScriptProfile::Strategic => "strategic" }) {
                    (header)

                    div class="main-grid" {
                        (content)
                    }
                }
                (maud::PreEscaped("<!-- strategic-page-end -->"))
            }
        }
    }
}

fn entry_top_bar() -> Markup {
    html! {
        header class="top-bar entry-top-bar" {
            div class="top-bar-left" {
                h1 class="logo" { "Fabelgeist" }
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
    economy: Option<&adventuresim_world_schema::SettlementEconomyProfile>,
    logged_in_as: Option<&str>,
) -> Markup {
    let services = [
        ("", "public-square", "Public square", "market"),
        ("residences", "residences", "Residences", "house"),
        ("keep", "keep", "Keep", "castle"),
        ("map", "map", "Map", "map"),
        ("merchants", "merchants", "General Market", "market"),
        ("weapons", "weapons", "Weapons", "weapons"),
        ("armor", "armor", "Armour", "armor"),
        ("clothing", "clothing", "Clothing", "clothing"),
        ("herbalist", "herbalist", "Herbalist", "medical-pack"),
        ("books", "books", "Bookstore", "open-book"),
        ("inn", "inn", "Inn", "inn"),
        ("religion", "religion", "Church", "church"),
    ];

    html! {
        @let material = if matches!(category, SettlementCategory::City | SettlementCategory::Capital) { "stone" } else { "wood" };
        @let active_id = if active_service.is_empty() { "public-square" } else { active_service };
        @let active_material = if active_id == "religion" || (active_id == "keep" && !matches!(category, SettlementCategory::Village)) { "stone" } else { material };
        @let active_tint = building_tint(settlement_id, active_id, active_material);
        style { (format!(":root{{--active-building-tint:{active_tint};}}")) }
        header class=(format!("top-bar settlement-top-bar material-{material}"))
            data-environment="settlement"
            data-building-tier=(building_tier(category))
            data-horizon-variant=(horizon_variant(settlement_id).as_str()) {
            div class="top-bar-left settlement-location" {
                div class="settlement-identity" {
                a href=(format!("/locations/settlement/{}", settlement_id)) class="settlement-name" {
                    (settlement_name)
                }
                span class="settlement-time" data-player-time title="Loading official time…"
                    aria-label="1st of First Seed · 08:00" {
                    "1st of First Seed · 08:00"
                }
                }
                (journal_button())
            }

            nav class="top-bar-center settlement-services" aria-label="Settlement services"
                data-settlement-id=(settlement_id) {
                @for (path, service_id, label, icon) in services {
                    @let available = settlement_building_available(settlement_id, category, economy, service_id);
                    @if available {
                    @let href = if path == "map" {
                        format!("/locations/settlement/{}/map", settlement_id)
                    } else if path.is_empty() {
                        format!("/locations/settlement/{}", settlement_id)
                    } else if matches!(path, "residences" | "keep") {
                        format!("/settlements/{}/places/{}", settlement_id, path)
                    } else {
                        format!("/settlements/{}/{}", settlement_id, path)
                    };
                    @let service_material = if service_id == "religion" || (service_id == "keep" && !matches!(category, SettlementCategory::Village)) { "stone" } else { material };
                    @let tint = building_tint(settlement_id, service_id, service_material);
                    @let selected = active_service == path || (path.is_empty() && active_service == service_id);
                    a href=(href)
                        class=(if selected { "nav-tab active" } else { "nav-tab" })
                        style=(format!("--building-tint:{tint}"))
                        data-service-id=(service_id)
                        data-building-id=(service_id)
                        data-building-material=(service_material)
                        data-service-label=(label)
                        aria-label=(label)
                        data-strategic-tooltip=(label)
                        aria-current=(if selected { "page" } else { "false" })
                    {
                        span class="service-tab-building" aria-hidden="true" {}
                        @if path == "weapons" {
                            span class="topbar-scene-effect-plane" aria-hidden="true" {
                                (smoke_effect("wilderness-smoke building-chimney-smoke"))
                            }
                        }
                        span
                            class=(format!("service-tab-icon service-tab-icon-{}", icon))
                            style=[(path == "religion").then(|| format!("--service-tab-icon: url('{}')", religion_icon_path(religion_id)))]
                            aria-hidden="true" {}
                        span class="service-tab-label" aria-hidden="true" { (label) }
                    }
                    }
                }
                @for organization in adventuresim_core::organization::organizations_for_chapter(settlement_id) {
                    @let chapter = organization.chapter(settlement_id).expect("local chapter");
                    @let standalone = economy.is_none_or(|profile| adventuresim_core::organization::chapter_has_standalone_building(organization, chapter, profile));
                    @if standalone {
                    @let kind = format!("{:?}", chapter.building_kind).to_ascii_lowercase();
                    @let charge = organization_charge(organization);
                    @let (field, accent) = organization_colors(&organization.id);
                    @let tint = building_tint(settlement_id, &chapter.location_id, material);
                    a href=(format!("/settlements/{}/places/{}", settlement_id, chapter.location_id))
                        class=(if active_service == chapter.location_id { "nav-tab active" } else { "nav-tab" })
                        style=(format!("--building-tint:{tint}"))
                        data-service-id="organization"
                        data-building-id=(&chapter.location_id)
                        data-organization-building-kind=(kind)
                        data-building-material=(material)
                        data-service-label=(&chapter.building_name)
                        aria-label=(&chapter.building_name)
                        data-strategic-tooltip=(&chapter.building_name)
                        aria-current=(if active_service == chapter.location_id { "page" } else { "false" }) {
                        span class="service-tab-building" aria-hidden="true" {}
                        span class="service-tab-icon service-tab-icon-organization"
                            style=(format!("--service-tab-icon: url('/static/icons/game/{charge}.svg'); --organization-field: {field}; --organization-accent: {accent}"))
                            aria-hidden="true" {}
                        span class="service-tab-label" aria-hidden="true" { (&chapter.building_name) }
                    }
                    }
                }
            }

            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    a href="/developer/scenarios" class="btn btn-small" data-developer-only data-hard-navigation {
                        "Scenario inspector"
                    }
                    button type="button" class="developer-quest-button" data-developer-quest-open
                        data-developer-only aria-label="Spawn a developer quest"
                        title="Spawn a developer quest" {
                        span class="developer-quest-button-icon" aria-hidden="true" {}
                    }
                    (character_switcher(name))
                }
            }
            dialog class="developer-quest-dialog" data-developer-quest-dialog
                aria-labelledby="developer-quest-title" {
                form method="dialog" class="developer-quest-shell" data-developer-quest-form {
                    header class="developer-quest-header" {
                        div {
                            h2 id="developer-quest-title" { "Spawn investigation quest" }
                            p data-developer-quest-settlement {
                                "Catalog and settlement witnesses load when opened."
                            }
                        }
                        button type="button" class="btn btn-small" data-developer-quest-close
                            aria-label="Close quest editor" { "Close" }
                    }
                    div class="developer-quest-status" role="status" aria-live="polite"
                        data-developer-quest-status {}
                    div class="developer-quest-errors" role="alert"
                        data-developer-quest-errors tabindex="-1" hidden {}
                    div class="developer-quest-fields" data-developer-quest-fields {}
                    footer class="developer-quest-footer" {
                        label class="developer-quest-override" {
                            input type="checkbox" data-developer-quest-override;
                            span { "Override compatibility and curation warnings" }
                        }
                        span {
                            button type="button" class="btn" data-developer-quest-close { "Cancel" }
                            button type="submit" class="btn btn-primary" data-developer-quest-submit {
                                "Create latent quest"
                            }
                        }
                    }
                }
            }
        }
        script src="/static/strategic-time.js?v=accessible-clock-1" {}
    }
}

fn settlement_has_keep(category: &SettlementCategory) -> bool {
    matches!(
        category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    )
}

pub(crate) fn settlement_building_available(
    settlement_id: &str,
    category: &SettlementCategory,
    economy: Option<&adventuresim_world_schema::SettlementEconomyProfile>,
    building_id: &str,
) -> bool {
    match building_id {
        "public-square" | "residences" | "map" => true,
        "keep" => settlement_has_keep(category),
        "merchants" | "weapons" | "armor" | "clothing" | "herbalist" | "books" | "inn"
        | "religion" => economy.is_none_or(|profile| service_tab_available(profile, building_id)),
        _ => adventuresim_core::organization::organization_chapter_at(settlement_id, building_id)
            .is_some_and(|(organization, chapter)| {
                economy.is_none_or(|profile| {
                    adventuresim_core::organization::chapter_has_standalone_building(
                        organization,
                        chapter,
                        profile,
                    )
                })
            }),
    }
}

fn service_tab_available(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    path: &str,
) -> bool {
    use adventuresim_core::settlement_economy::{player_visible_npc_tabs, visible_npc_tab};
    let location_id = match path {
        "map" => return true,
        "merchants" => "market",
        "weapons" => "forge",
        "armor" => "armoury",
        "clothing" => "tailor",
        "herbalist" => "herbalist",
        "books" => "bookstore",
        "inn" => "inn",
        "religion" => "church",
        _ => return false,
    };
    visible_npc_tab(
        &player_visible_npc_tabs(profile, false, "fixture-no-orgs"),
        location_id,
    )
    .is_some()
}

fn quest_location_top_bar(
    location_name: &str,
    location_id: &str,
    active_tab: &str,
    camp_fire_lit: bool,
    logged_in_as: Option<&str>,
) -> Markup {
    let enemy_tint = "hsl(134 31% 20%)";
    let map_tint = "hsl(126 30% 22%)";
    let active_tint = if active_tab == "enemy" || active_tab == "camp" {
        enemy_tint
    } else {
        map_tint
    };
    html! {
        style { (format!(":root{{--active-building-tint:{active_tint};}}")) }
        header class="top-bar settlement-top-bar quest-location-top-bar" data-environment="wilderness"
            data-wilderness-variant=(wilderness_variant(location_id).as_str())
            data-camp-fire=[(active_tab == "camp").then_some(if camp_fire_lit { "lit" } else { "embers" })] {
            div class="top-bar-left settlement-location" {
                div class="settlement-identity" {
                    @if active_tab == "camp" {
                        a href="/camp" class="settlement-name" aria-current="page" { (location_name) }
                    } @else {
                        a href=(format!("/locations/case-site/{}", location_id)) class="settlement-name" { (location_name) }
                    }
                    span class="settlement-time" data-player-time
                        aria-label="1st of First Seed · 08:00" { "1st of First Seed · 08:00" }
                }
                (journal_button())
            }
            nav class="top-bar-center settlement-services" aria-label="Location views" {
                @if active_tab == "camp" {
                    a href="/camp" class="nav-tab active quest-context-tab"
                        style=(format!("--building-tint:{enemy_tint}"))
                        data-location-view="camp"
                        data-service-label="Camp"
                        aria-current="page" aria-label="Camp" data-strategic-tooltip="Camp" {
                        span class="service-tab-building wilderness-tab-prop" aria-hidden="true" {}
                        span class="topbar-scene-effect-plane" aria-hidden="true" {
                            @if camp_fire_lit {
                                (camp_flame_effect())
                            }
                            (smoke_effect("wilderness-smoke campfire-smoke"))
                        }
                        span class="service-tab-label" aria-hidden="true" { "Camp" }
                    }
                } @else {
                a href=(format!("/locations/case-site/{}", location_id))
                    class=(if active_tab == "map" { "nav-tab active" } else { "nav-tab" })
                    style=(format!("--building-tint:{map_tint}"))
                    data-location-view="map"
                    data-service-label="Map"
                    aria-current=(if active_tab == "map" { "page" } else { "false" })
                    aria-label="Map" data-strategic-tooltip="Map" {
                    span class="service-tab-building wilderness-tab-prop" aria-hidden="true" {}
                    span class="service-tab-label" aria-hidden="true" { "Map" }
                }
                a href=(format!("/locations/case-site/{}/enemy", location_id))
                    class=(if active_tab == "enemy" { "nav-tab active" } else { "nav-tab" })
                    style=(format!("--building-tint:{enemy_tint}"))
                    data-location-view="enemy"
                    data-service-label="Enemy"
                    aria-current=(if active_tab == "enemy" { "page" } else { "false" })
                    aria-label="Enemy" data-strategic-tooltip="Enemy" {
                    span class="service-tab-building wilderness-tab-prop" aria-hidden="true" {}
                    span class="service-tab-icon service-tab-icon-enemy" aria-hidden="true" {}
                    span class="service-tab-label" aria-hidden="true" { "Enemy" }
                }
                }
            }
            div class="top-bar-right" {
                @if let Some(name) = logged_in_as {
                    (character_switcher(name))
                }
            }
        }
        script src="/static/strategic-time.js?v=accessible-clock-1" {}
    }
}

fn smoke_effect(class: &str) -> Markup {
    let puffs = [
        (30, 76, 5, -8, -0.0, 7.0),
        (35, 76, 4, 7, -0.4, 7.9),
        (27, 76, 6, -3, -0.8, 7.5),
        (33, 76, 4, 10, -1.2, 8.4),
        (29, 76, 5, -9, -1.6, 7.2),
        (36, 76, 3, 5, -2.0, 7.8),
        (26, 76, 4, -6, -2.4, 8.6),
        (32, 76, 6, 9, -2.8, 7.4),
        (37, 76, 4, 3, -3.2, 8.1),
        (28, 76, 3, -10, -3.6, 7.7),
        (34, 76, 5, 6, -4.0, 8.3),
        (25, 76, 4, -4, -4.4, 7.1),
        (31, 76, 3, 12, -4.8, 8.0),
        (38, 76, 5, -7, -5.2, 8.5),
        (24, 76, 4, 4, -5.6, 7.3),
        (33, 76, 6, -11, -6.0, 8.2),
        (29, 76, 4, 8, -6.4, 7.6),
        (36, 76, 3, -2, -6.8, 8.7),
    ];
    html! {
        svg class=(class) viewBox="0 0 64 96" aria-hidden="true" focusable="false" {
            @for (cx, cy, radius, drift, delay, duration) in puffs {
                circle class="smoke-puff" cx=(cx) cy=(cy) r=(radius)
                    style=(format!("--smoke-drift:{drift}px;animation-delay:{delay}s;animation-duration:{duration}s")) {}
            }
        }
    }
}

fn camp_flame_effect() -> Markup {
    let particles = [
        (27, 67, 2, -8, -0.0, 1.75),
        (33, 68, 3, 6, -0.15, 2.05),
        (38, 67, 2, 10, -0.3, 1.9),
        (30, 69, 2, -4, -0.45, 2.2),
        (35, 66, 2, 3, -0.6, 1.7),
        (25, 68, 3, -11, -0.75, 2.35),
        (40, 69, 2, 8, -0.9, 1.85),
        (31, 67, 2, -2, -1.05, 2.1),
        (36, 68, 3, 5, -1.2, 2.4),
        (28, 66, 2, -7, -1.35, 1.8),
        (42, 68, 2, 12, -1.5, 2.25),
        (23, 69, 2, -12, -1.65, 1.95),
        (34, 67, 2, 2, -1.8, 2.3),
        (29, 68, 3, -5, -1.95, 1.9),
        (39, 66, 2, 9, -2.1, 2.15),
        (32, 69, 2, -1, -2.25, 2.45),
    ];
    html! {
        svg class="wilderness-flame campfire-flame" viewBox="0 0 64 80"
            aria-hidden="true" focusable="false" {
            path class="flame-shape flame-outer"
                d="M32 74C17 74 11 64 15 51c3-10 12-15 13-29 10 7 16 16 14 27 5-4 7-9 7-14 7 8 10 17 7 25-3 9-11 14-24 14Z" {}
            path class="flame-shape flame-inner"
                d="M33 69c-8 0-13-6-11-14 1-6 7-10 8-18 7 6 10 12 7 18 3-2 5-5 6-8 3 5 4 11 1 16-2 4-6 6-11 6Z" {}
            @for (cx, cy, radius, drift, delay, duration) in particles {
                circle class="fire-particle" cx=(cx) cy=(cy) r=(radius)
                    style=(format!("--fire-drift:{drift}px;animation-delay:{delay}s;animation-duration:{duration}s")) {}
            }
        }
    }
}

fn building_tint(settlement: &str, service: &str, material: &str) -> String {
    let hash = settlement
        .bytes()
        .chain(*b":")
        .chain(service.bytes())
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    let service_slot = match service {
        "public-square" => 0,
        "residences" => 1,
        "keep" => 2,
        "map" => 3,
        "merchants" => 4,
        "weapons" => 5,
        "armor" => 6,
        "clothing" => 7,
        "herbalist" => 8,
        "books" => 9,
        "inn" => 10,
        "religion" => 11,
        _ => (hash % 12) as usize,
    };
    let settlement_shift = (hash >> 24) % 9;
    let hue = if material == "stone" {
        [46, 198, 218, 205, 224, 252, 282, 164, 128, 68, 36, 214][service_slot] + settlement_shift
    } else {
        [35, 58, 16, 8, 20, 31, 43, 56, 104, 48, 72, 350][service_slot] + settlement_shift
    };
    let saturation = if material == "stone" {
        12 + (hash >> 8) % 13
    } else {
        30 + (hash >> 8) % 25
    };
    let lightness = 19 + (hash >> 16) % 8;
    format!("hsl({hue} {saturation}% {lightness}%)")
}

fn building_tier(category: &SettlementCategory) -> &'static str {
    match category {
        SettlementCategory::Unknown | SettlementCategory::Hamlet | SettlementCategory::Village => {
            "village"
        }
        SettlementCategory::Town => "town",
        SettlementCategory::City | SettlementCategory::Capital => "city",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HorizonVariant {
    Inland,
    Coastal,
    River,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WildernessVariant {
    Forest,
    Grassland,
    Hills,
}

impl WildernessVariant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Forest => "forest",
            Self::Grassland => "grassland",
            Self::Hills => "hills",
        }
    }
}

impl HorizonVariant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inland => "inland",
            Self::Coastal => "coastal",
            Self::River => "river",
        }
    }
}

/// Temporary stable scenery selection. Imported hydrology will replace only
/// this selector; settlement markup and CSS remain variant-driven.
fn horizon_variant(settlement_id: &str) -> HorizonVariant {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in settlement_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    match hash % 3 {
        0 => HorizonVariant::Inland,
        1 => HorizonVariant::Coastal,
        _ => HorizonVariant::River,
    }
}

/// Temporary stable terrain selection. World terrain data can replace this
/// selector without changing the shared camp and quest-location header.
fn wilderness_variant(location_id: &str) -> WildernessVariant {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in location_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    match hash % 3 {
        0 => WildernessVariant::Forest,
        1 => WildernessVariant::Grassland,
        _ => WildernessVariant::Hills,
    }
}

fn character_switcher(name: &str) -> Markup {
    let initial = name.chars().next().unwrap_or('?');
    html! {
        button type="button" class="developer-mode-toggle" data-developer-mode-toggle
            aria-label="Enable developer mode" aria-pressed="false" title="Developer mode" {
            span class="developer-mode-icon" aria-hidden="true" {}
        }
        details class="character-switcher" {
            summary class="character-switcher-toggle"
                aria-label=(format!("Character menu for {name}")) title=(name) {
                span class="party-portrait-initial character-switcher-portrait" aria-hidden="true" {
                    span class="party-portrait-face" { (initial) }
                }
            }
            div class="character-switcher-menu" {
                div data-character-switcher-options data-character-switcher-url="/characters/menu" {
                    p class="character-switcher-empty" { "Loading adventurers…" }
                }
                a href="/characters/candidates" class="btn btn-small" { "Character select" }
            }
        }
    }
}

fn journal_button() -> Markup {
    html! {
        span class="reference-buttons" {
            a href="/quests" class="journal-button" data-journal-tab
                aria-label="Open journal" aria-pressed="false"
                title="Journal" data-strategic-tooltip="Journal" {
                span class="journal-button-icon" aria-hidden="true" {}
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

#[cfg(test)]
mod tests {
    use super::{
        HorizonVariant, ScriptProfile, WildernessVariant, building_tier, building_tint,
        entry_layout, horizon_variant, journal_layout, page_shell, quest_location_top_bar,
        religion_icon_path, settlement_layout_with_session, settlement_top_bar, wilderness_variant,
    };
    use crate::spacetimedb::SettlementCategory;
    use maud::html;

    #[test]
    fn shell_and_entry_header_use_the_official_fabelgeist_name() {
        let shell = page_shell("Chat", html! {}, html! {}, ScriptProfile::Strategic).into_string();
        assert!(shell.contains("<title>Chat - Fabelgeist</title>"));

        let entry = entry_layout("Create", html! {}).into_string();
        assert!(entry.contains("<h1 class=\"logo\">Fabelgeist</h1>"));
        assert!(!entry.contains("Adventure Simulator"));
    }

    #[test]
    fn strategic_shell_cache_busts_exact_location_chat_authority() {
        let markup = page_shell("Chat", html! {}, html! {}, ScriptProfile::Strategic).into_string();
        assert!(markup.contains("/static/local-chat.js?v=local-chat-location-authority-1"));
        assert!(!markup.contains("local-chat.js?v=herbalist-private-1"));
        assert!(markup.contains("/static/live-regions.js?v=preserved-client-regions-1"));
        assert!(markup.contains("id=\"strategic-page\""));
        assert!(markup.contains("/static/strategic-navigation.js"));
        assert!(markup.contains("/static/strategic-mutations.js?v=formaction-override-1\" defer"));
        assert_eq!(markup.matches("/static/training-schedule.js").count(), 1);
        assert_eq!(markup.matches("/static/immediate-activity.js").count(), 1);
        assert!(markup.contains("/static/training-schedule.js?v=apprentice-system-1\" defer"));
        assert!(markup.contains("/static/immediate-activity.js?v=manual-activities-1\" defer"));
        assert_eq!(markup.matches("id=\"strategic-live-stream\"").count(), 1);
        assert!(markup.find("id=\"strategic-live-stream\"") < markup.find("id=\"strategic-page\""));
        assert!(!markup.contains("live-regions.js?v=floating-time-editor-1"));
    }

    #[test]
    fn direct_journal_uses_the_persistent_strategic_profile() {
        let markup = journal_layout(html! {}, Some("Ada")).into_string();
        assert!(markup.contains("data-script-profile=\"strategic\""));
        assert!(markup.contains("/static/strategic-navigation.js"));
        assert_eq!(markup.matches("id=\"strategic-live-stream\"").count(), 1);
    }

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
        let wood_tints = [
            "public-square",
            "residences",
            "keep",
            "map",
            "merchants",
            "weapons",
            "armor",
            "clothing",
            "herbalist",
            "books",
            "inn",
            "religion",
        ]
        .map(|service| building_tint("lubeck", service, "wood"));
        assert_eq!(
            wood_tints
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            wood_tints.len()
        );
        let hue = |tint: String| tint[4..].split(' ').next().unwrap().parse::<u64>().unwrap();
        assert!((8..=80).contains(&hue(building_tint("lubeck", "inn", "wood"))));
        assert!((36..=290).contains(&hue(building_tint("lubeck", "inn", "stone"))));
    }

    #[test]
    fn settlement_categories_select_the_expected_building_tier() {
        for (category, tier) in [
            (SettlementCategory::Unknown, "village"),
            (SettlementCategory::Hamlet, "village"),
            (SettlementCategory::Village, "village"),
            (SettlementCategory::Town, "town"),
            (SettlementCategory::City, "city"),
            (SettlementCategory::Capital, "city"),
        ] {
            assert_eq!(building_tier(&category), tier);
            let markup =
                settlement_top_bar("Place", "p", &category, "map", None, None, None).into_string();
            assert!(markup.contains(&format!("data-building-tier=\"{tier}\"")));
        }
    }

    #[test]
    fn bookstore_service_renders_a_navigable_books_tab() {
        let mut economy = adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder();
        economy
            .services
            .push(adventuresim_world_schema::SettlementService::Bookstore);
        let markup = settlement_top_bar(
            "Place",
            "p",
            &SettlementCategory::City,
            "books",
            None,
            Some(&economy),
            None,
        )
        .into_string();
        assert!(markup.contains("href=\"/settlements/p/books\""));
        assert!(markup.contains("data-service-label=\"Bookstore\""));
        assert!(markup.contains("nav-tab active"));
    }

    #[test]
    fn standalone_organization_facade_has_exact_identity_and_heraldry() {
        let organization = adventuresim_core::organization::catalog()
            .organizations
            .iter()
            .find(|organization| !organization.chapters.is_empty())
            .expect("catalog organization with a chapter");
        let chapter = &organization.chapters[0];
        let markup = settlement_top_bar(
            "Place",
            &chapter.settlement_id,
            &SettlementCategory::Town,
            &chapter.location_id,
            None,
            None,
            None,
        )
        .into_string();
        assert!(markup.contains("data-service-id=\"organization\""));
        assert!(markup.contains(&format!("data-building-id=\"{}\"", chapter.location_id)));
        assert!(markup.contains(&format!(
            "/static/icons/game/{}.svg",
            super::organization_charge(organization)
        )));
    }

    #[test]
    fn horizon_variants_are_stable_reachable_and_emitted() {
        assert_eq!(horizon_variant("lubeck"), horizon_variant("lubeck"));
        let variants = (0..128)
            .map(|id| horizon_variant(&format!("settlement-{id}")))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            variants,
            [
                HorizonVariant::Inland,
                HorizonVariant::Coastal,
                HorizonVariant::River
            ]
            .into_iter()
            .collect()
        );
        let expected = horizon_variant("stable-place").as_str();
        let markup = settlement_top_bar(
            "Stable Place",
            "stable-place",
            &SettlementCategory::Town,
            "map",
            None,
            None,
            None,
        )
        .into_string();
        assert!(markup.contains("data-building-tier=\"town\""));
        assert!(markup.contains(&format!("data-horizon-variant=\"{expected}\"")));
    }

    #[test]
    fn wilderness_variants_are_stable_reachable_and_emitted() {
        assert_eq!(wilderness_variant("party-7"), wilderness_variant("party-7"));
        let variants = (0..128)
            .map(|id| wilderness_variant(&format!("location-{id}")))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            variants,
            [
                WildernessVariant::Forest,
                WildernessVariant::Grassland,
                WildernessVariant::Hills,
            ]
            .into_iter()
            .collect()
        );
        let expected = wilderness_variant("stable-place").as_str();
        let markup = quest_location_top_bar("Stable Place", "stable-place", "map", false, None)
            .into_string();
        assert!(markup.contains(&format!("data-wilderness-variant=\"{expected}\"")));
    }

    #[test]
    fn settlement_tabs_include_public_and_non_service_buildings() {
        let markup = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "religion",
            Some("roman_catholic"),
            None,
            None,
        )
        .into_string();
        assert_eq!(markup.matches("class=\"service-tab-building\"").count(), 11);
        assert_eq!(markup.matches("class=\"service-tab-icon ").count(), 11);
        assert!(markup.contains("aria-label=\"Public square\""));
        assert!(markup.contains("href=\"/locations/settlement/s\""));
        assert!(markup.contains("data-service-id=\"public-square\""));
        assert!(markup.contains("service-tab-icon-market"));
        assert!(markup.contains("aria-label=\"Residences\""));
        assert!(markup.contains("href=\"/settlements/s/places/residences\""));
        assert!(markup.contains("data-service-id=\"residences\""));
        assert!(markup.contains("service-tab-icon-house"));
        assert!(!markup.contains("aria-label=\"Keep\""));
        assert!(markup.contains("aria-label=\"Church\""));
        let inn = markup
            .split("data-building-id=\"inn\"")
            .nth(1)
            .and_then(|tail| tail.split("</a>").next())
            .expect("rendered Inn Place Facade");
        assert!(inn.contains("aria-label=\"Inn\""));
        assert!(inn.contains("data-strategic-tooltip=\"Inn\""));
        assert!(markup.contains("aria-current=\"page\""));
        assert!(markup.contains("--service-tab-icon: url("));
        assert!(markup.contains(religion_icon_path(Some("roman_catholic"))));

        let css = include_str!("../../static/css/layout.css").replace("\r\n", "\n");
        for service in [
            "public-square",
            "residences",
            "keep",
            "map",
            "merchants",
            "weapons",
            "armor",
            "clothing",
            "herbalist",
            "inn",
            "religion",
        ] {
            assert!(css.contains(&format!(
                "/static/styles/timber-framed/building/village/{service}.png"
            )));
        }
        assert!(css.contains("--service-building-image"));
        assert!(css.contains("data-building-tier"));
        assert!(css.contains("data-service-id=\"public-square\"].active"));
        assert!(css.contains("data-service-id=\"residences\"].active"));
        assert!(css.contains("data-service-id=\"keep\"].active"));
        assert!(css.contains(".nav-tab[data-service-id=\"books\"]"));
        for kind in [
            "guildhall",
            "workshop",
            "college",
            "confraternity",
            "commandery",
            "lodge",
        ] {
            assert!(css.contains(&format!("data-organization-building-kind=\"{kind}\"")));
        }
        assert!(css.contains("var(--organization-accent, #fff)"));

        let town = settlement_top_bar(
            "Larger Place",
            "t",
            &SettlementCategory::Town,
            "keep",
            None,
            None,
            None,
        )
        .into_string();
        assert_eq!(town.matches("class=\"service-tab-building\"").count(), 12);
        assert!(town.contains("aria-label=\"Keep\""));
        assert!(town.contains("href=\"/settlements/t/places/keep\" class=\"nav-tab active\""));
        assert!(town.contains("service-tab-icon-castle"));
        assert!(town.contains(
            "data-service-id=\"keep\" data-building-id=\"keep\" data-building-material=\"stone\""
        ));
    }

    #[test]
    fn active_building_is_semantic_but_only_underlined_by_css() {
        let markup = settlement_layout_with_session(
            "Inn",
            "Lubeck",
            "lubeck",
            &SettlementCategory::City,
            "inn",
            None,
            None,
            html! {},
            None,
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
        assert!(building.contains("pathname.includes(\"/party\")"));
        assert!(building.contains("buildingContextPath && buildings.has(requested)"));
        assert!(building.contains("/locations\\/settlement\\/[^/]+\\/fireplace"));
        assert!(building.contains("!buildings.has(requested) || !buildingContextPath"));
        assert!(building.contains("tab.dataset.buildingId === building"));
    }

    #[test]
    fn wilderness_and_church_roots_match_the_active_tab_material() {
        let wilderness = quest_location_top_bar("Ruins", "q", "enemy", false, None).into_string();
        assert!(wilderness.contains(":root{--active-building-tint:hsl(134 31% 20%);}"));
        let church = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "religion",
            None,
            None,
            None,
        )
        .into_string();
        assert!(church.contains(&format!(
            ":root{{--active-building-tint:{};}}",
            building_tint("s", "religion", "stone")
        )));
    }

    #[test]
    fn strategic_header_uses_portrait_menu_without_quest_or_party_labels() {
        let markup = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "map",
            None,
            None,
            Some("Ada"),
        )
        .into_string();
        assert!(markup.contains("class=\"character-switcher\""));
        assert!(markup.contains("Character menu for Ada"));
        assert!(markup.contains("character-switcher-portrait"));
        assert!(markup.contains("Character select"));
        assert!(markup.contains("href=\"/characters/candidates\""));
        assert!(markup.contains("data-character-switcher-options"));
        assert!(markup.contains("data-character-switcher-url=\"/characters/menu\""));
        assert!(!markup.contains("Party: "));
        assert!(!markup.contains("data-current-quest"));
        assert!(!markup.contains("current-quest.js"));
        assert!(!markup.contains("data-settlement-turn-in-badge"));
        assert!(!markup.contains("data-map-quest-badge"));
        let developer = markup.find("data-developer-mode-toggle").unwrap();
        let portrait = markup.find("class=\"character-switcher\"").unwrap();
        assert!(
            developer < portrait,
            "developer toggle must precede the portrait"
        );
        assert!(markup.contains("aria-label=\"Enable developer mode\""));
        assert!(markup.contains("aria-pressed=\"false\""));
        assert!(!markup.contains("data-developer-outbreak-demo"));
        assert!(!markup.contains("data-developer-autopsy-demo"));
        assert!(!markup.contains("data-developer-puzzle-demo"));
        assert!(markup.contains("href=\"/developer/scenarios\""));
        assert!(markup.contains("href=\"/developer/scenarios\" class=\"btn btn-small\" data-developer-only data-hard-navigation"));
        assert!(!markup.contains("data-puzzle-kind="));
        let layout_css = include_str!("../../static/css/layout.css");
        assert!(
            layout_css.contains(
                "html:not([data-developer-mode]) [data-developer-only] { display: none; }"
            )
        );
        assert!(
            layout_css.contains(
                "html[data-developer-mode] [data-developer-only] { display: inline-grid; }"
            )
        );
        assert!(markup.contains("class=\"journal-button\""));
        assert!(markup.contains("aria-label=\"Open journal\""));
        assert!(markup.contains("data-journal-tab"));
        assert!(markup.contains("aria-pressed=\"false\""));
        assert!(!markup.contains("data-journal-dialog"));
        assert!(!markup.contains(">Investigation journal<"));
        let wilderness =
            quest_location_top_bar("Ruins", "q", "map", false, Some("Ada")).into_string();
        assert!(wilderness.contains("class=\"journal-button\""));
        assert!(
            wilderness.find("data-developer-mode-toggle").unwrap()
                < wilderness.find("class=\"character-switcher\"").unwrap()
        );
    }

    #[test]
    fn strategic_shell_loads_the_location_preserving_journal_tab() {
        let markup = page_shell(
            "Test",
            settlement_top_bar(
                "Smallville",
                "s",
                &SettlementCategory::Village,
                "map",
                None,
                None,
                Some("Ada"),
            ),
            html! {},
            ScriptProfile::Strategic,
        )
        .into_string();
        assert!(markup.contains("/static/journal-tab.js"));
        assert!(!markup.contains("data-journal-dialog"));
        assert!(!markup.contains("data-journal-dialog"));
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

    #[test]
    fn location_navigation_reports_only_truthful_active_contexts() {
        let overview = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "",
            None,
            None,
            None,
        )
        .into_string();
        assert!(overview.contains("href=\"/locations/settlement/s\" class=\"nav-tab active\""));
        assert!(overview.contains("aria-label=\"Public square\""));

        let map = quest_location_top_bar("Ruins", "q", "map", false, None).into_string();
        assert!(map.contains("aria-label=\"Map\""));
        assert!(map.contains("href=\"/locations/case-site/q\" class=\"nav-tab active\""));

        let enemy = quest_location_top_bar("Ruins", "q", "enemy", false, None).into_string();
        assert!(enemy.contains("aria-label=\"Enemy\""));
        assert!(enemy.contains("href=\"/locations/case-site/q/enemy\" class=\"nav-tab active\""));

        let camp = quest_location_top_bar("Camp", "party-7", "camp", true, None).into_string();
        assert!(camp.contains("aria-label=\"Camp\""));
        assert!(camp.contains("href=\"/camp\""));
        assert!(camp.contains("data-camp-fire=\"lit\""));
        assert_eq!(
            camp.matches("class=\"nav-tab active quest-context-tab\"")
                .count(),
            1
        );
        assert_eq!(camp.matches("campfire-flame").count(), 1);
        assert_eq!(camp.matches("campfire-smoke").count(), 1);
        assert_eq!(camp.matches("class=\"fire-particle\"").count(), 16);
        assert_eq!(camp.matches("class=\"smoke-puff\"").count(), 18);
        assert!(!camp.contains("/locations/case-site/party-7"));
        assert!(!camp.contains("/locations/case-site/party-7/map"));
        assert!(!camp.contains("/locations/case-site/party-7/enemy"));

        let rested_camp =
            quest_location_top_bar("Camp", "party-7", "camp", false, None).into_string();
        assert!(rested_camp.contains("data-camp-fire=\"embers\""));
        assert!(!rested_camp.contains("campfire-flame"));
        assert_eq!(rested_camp.matches("campfire-smoke").count(), 1);
        assert!(rested_camp.contains("href=\"/camp\""));
        assert!(rested_camp.contains("data-service-label=\"Camp\""));
        assert!(rested_camp.contains("class=\"service-tab-label\""));
    }

    #[test]
    fn quest_tabs_separate_map_arrival_from_the_combined_enemy_and_loot_view() {
        let markup = quest_location_top_bar("Ruins", "q", "map", false, None).into_string();
        assert_eq!(markup.matches("data-location-view=").count(), 2);
        assert_eq!(
            markup
                .matches("class=\"service-tab-building wilderness-tab-prop\"")
                .count(),
            2
        );
        assert!(markup.contains("href=\"/locations/case-site/q\""));
        assert!(markup.contains("href=\"/locations/case-site/q/enemy\""));
        for label in ["Map", "Enemy"] {
            assert!(markup.contains(&format!("aria-label=\"{label}\"")));
        }
        assert!(!markup.contains("aria-label=\"Encounter\""));
        assert!(!markup.contains("aria-label=\"Loot\""));
        assert!(!markup.contains("campfire-flame"));
        assert!(!markup.contains("campfire-smoke"));
    }

    #[test]
    fn weapons_tabs_receive_decorative_smoke_without_changing_service_navigation() {
        let markup = settlement_top_bar(
            "Smallville",
            "s",
            &SettlementCategory::Village,
            "weapons",
            None,
            None,
            None,
        )
        .into_string();
        assert_eq!(markup.matches("building-chimney-smoke").count(), 1);
        assert!(markup.contains("aria-hidden=\"true\""));
        assert_eq!(markup.matches("class=\"nav-tab").count(), 11);
    }

    #[test]
    fn strategic_notice_is_a_complete_accessible_page() {
        let markup = super::strategic_notice_page(
            "Not here",
            "Travel first.",
            "/characters",
            "Return",
            Some("Ada"),
        )
        .into_string();
        assert!(markup.contains("<!DOCTYPE html>"));
        assert!(markup.contains("role=\"alert\""));
        assert!(markup.contains("data-strategic-safe-message"));
        assert!(markup.contains("href=\"/characters\""));
        assert!(markup.contains("Ada"));
    }

    #[test]
    fn narrow_entry_layout_stacks_every_rail_without_clipping_document_scroll() {
        let markup = entry_layout(
            "Create",
            html! {
                aside class="left-sidebar" { "Help" }
                main class="center-content" { "Form" }
                aside class="right-sidebar" { "Equipment" }
            },
        )
        .into_string();
        assert!(markup.contains("class=\"main-grid\""));
        assert!(markup.contains("class=\"right-sidebar\""));
        let css = include_str!("../../static/css/layout.css");
        let mobile = &css[css.find("@media (max-width: 768px)").unwrap()..];
        assert!(mobile.contains(".app {"));
        assert!(mobile.contains("height: auto;"));
        assert!(mobile.contains("overflow: visible;"));
        assert!(mobile.contains("grid-template-areas: \"main\" \"left\" \"right\""));
        assert!(!mobile.contains("body:has(.settlement-top-bar) .main-grid"));
    }
}
