use std::collections::BTreeSet;

use adventuresim_core::{
    bestiary::ThreatId,
    errantry::{FeyPresenterCatalogId, FeySpeechPart, fey_speech},
    strategic_time::{ItinerarySegment, ItinerarySegmentKind},
};
use maud::{Markup, html};

use super::{
    chrome::{
        format_distance, format_journey_time, format_population, party_portrait_overlay,
        settlement_description, visual_stage,
    },
    rest::{SoapRestPreview, party_rest_menu},
    social::settlement_chat_area,
};
use crate::routes::travel::{TravelDestination, TravelProvisionForecast};
use crate::spacetimedb::{
    BackendContextDisposition, BackendRoadChallenge, ChallengePresenterCatalogId, Character,
    ContractPresentation, JourneyPrecipitation, JourneyTerrainKind, Party, PartyJourney,
    PartyJourneyItinerary, PartyJourneyRoute, Settlement, StrategicEncounter,
};
use crate::templates::{
    camp_location_layout_with_session, decorative_game_icon, empty_state, game_icon,
    settlement_layout_with_session, sidebar_section,
};

pub fn settlement_map_page(
    settlement: &Settlement,
    settlements: &[Settlement],
    case_sites: &[crate::spacetimedb::BackendCaseSitePin],
    strategic_map: Option<&crate::strategic_map::StrategicMap>,
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    active_character: Option<&Character>,
    active_party: Option<&Party>,
    _party_members: &[Character],
    default_rest_minutes: u64,
    soap_preview: SoapRestPreview,
    can_travel: bool,
    provision_forecast: Option<&TravelProvisionForecast>,
    provisioning_path: Option<&str>,
    is_current_settlement: bool,
    abandonable_quest: Option<&ContractPresentation>,
    logged_in_as: Option<&str>,
) -> Markup {
    let selected = selected_id.and_then(|id| destinations.iter().find(|entry| entry.id == id));
    let selected_settlement =
        selected_id.and_then(|id| settlements.iter().find(|entry| entry.id == id));
    let base_path = format!("/locations/settlement/{}/map", settlement.id);
    let connected_ids = destinations
        .iter()
        .filter(|destination| !destination.round_trip_destination)
        .map(|destination| destination.id.as_str())
        .collect::<BTreeSet<_>>();
    let content = html! {
        (map_destination_list_with_context(
            destinations,
            selected_id,
            &base_path,
            is_current_settlement.then_some(MapCurrentLocation {
                name: &settlement.name,
            }),
            abandonable_quest.map(|quest| MapAbandonableQuest {
                id: &quest.id,
                title: &quest.title,
            }),
            active_party.filter(|_| can_travel).map(|_| html! {
                section class="rest-service-menu map-rest-menu" aria-label="Rest" {
                    (party_rest_menu(
                        &format!("{base_path}/rest"),
                        "map-rest",
                        "Rest here",
                        "Rest party",
                        default_rest_minutes,
                        None,
                        soap_preview,
                    ))
                }
            }),
        ))
        main class="center-content settlement-main settlement-map-main" {
            @if settlement.source_node_id.is_some() {
                @if let Some(strategic_map) = strategic_map {
                    (crate::strategic_map::strategic_map(
                        strategic_map,
                        settlements,
                        case_sites,
                        &settlement.id,
                        &connected_ids,
                        selected_id,
                        &base_path,
                        selected.and_then(|destination| destination.terrain_route.as_ref()),
                    ))
                } @else {
                    (crate::strategic_map::strategic_map_bundle_unavailable())
                }
            } @else {
                (crate::strategic_map::strategic_map_unavailable(&settlement.name))
            }
        }
        (map_destination_detail(
            selected,
            selected_settlement,
            selected_settlement.is_some_and(|destination| destination.id == settlement.id),
            can_travel,
            provisioning_path,
            provision_forecast,
            active_party,
            active_party.is_some_and(|party| party.leader_id == active_character.map_or(0, |character| character.id)),
            None,
            &base_path,
        ))
    };
    settlement_layout_with_session(
        &format!("{} map", settlement.name),
        &settlement.name,
        &settlement.id,
        &settlement.category,
        "map",
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}

#[derive(Clone, Copy)]
struct MapCurrentLocation<'a> {
    name: &'a str,
}

#[derive(Clone, Copy)]
struct MapAbandonableQuest<'a> {
    id: &'a str,
    title: &'a str,
}

#[cfg(test)]
pub(crate) fn map_destination_list(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
) -> Markup {
    map_destination_list_with_context(destinations, selected_id, base_path, None, None, None)
}

pub(crate) fn map_destination_list_with_rest(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
    rest_menu: Markup,
) -> Markup {
    map_destination_list_with_context(
        destinations,
        selected_id,
        base_path,
        None,
        None,
        Some(rest_menu),
    )
}

fn map_destination_list_with_context(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
    current_location: Option<MapCurrentLocation<'_>>,
    abandonable_quest: Option<MapAbandonableQuest<'_>>,
    rest_menu: Option<Markup>,
) -> Markup {
    html! {
        aside class=(if rest_menu.is_some() { "left-sidebar map-rest-sidebar" } else { "left-sidebar" }) {
            div class=[rest_menu.is_some().then_some("map-rest-sidebar-content")] {
            (sidebar_section("Destinations", html! {
                @if destinations.is_empty() && current_location.is_none() {
                    (empty_state("No destinations are available from this location.", None, None))
                } @else {
                    nav class="location-destination-list" aria-label="Travel destinations" {
                        @if let Some(current) = current_location {
                            div class="list-item travel-destination-row current-location-row"
                                aria-current="location" {
                                strong { (current.name) }
                                span class="text-muted small-copy current-location-label" { "Current" }
                            }
                        }
                        @for destination in destinations {
                            @let destination_tooltip = quest_destination_tooltip(destination);
                            a href=(format!("{}?destination={}", base_path, destination.id))
                                class=(if selected_id == Some(destination.id.as_str()) { "list-item travel-destination-row active" } else { "list-item travel-destination-row" })
                                title=[destination_tooltip.as_deref()]
                                data-travel-name=(&destination.name)
                                data-travel-description=[destination_tooltip.as_deref()]
                                data-travel-minutes=(destination.journey_minutes)
                                data-travel-round-trip=(destination.round_trip_destination)
                                data-travel-camp-stops=(format_camp_stops(&destination.camp_stop_minutes))
                                data-travel-camp-forecasts=(format_camp_forecasts(destination))
                                data-travel-distance=(format_distance(destination.distance_m)) {
                                @if let Some(forecast) = &destination.provision_forecast {
                                    span hidden data-provision-payload
                                        data-planning-minutes=(forecast.planning_minutes)
                                        data-living-members=(forecast.living_members)
                                        data-food-days=(forecast.food_days)
                                        data-water-days=(forecast.water_days)
                                        data-ordinary-water-days=(forecast.ordinary_water_days)
                                        data-emergency-alcohol-days=(forecast.emergency_alcohol_days)
                                        data-ration-kcal=(forecast.ration_kcal)
                                        data-waterskin-ml=(forecast.waterskin_capacity_ml) {}
                                }
                                strong { (&destination.name) }
                                @if destination.active_contract_destination {
                                    span class="destination-quest-badge" title=(destination_tooltip.as_deref().unwrap_or("Active quest destination"))
                                        aria-label="Active quest destination" { "!" }
                                }
                                span class="text-muted small-copy" { (format_distance(destination.distance_m)) }
                            }
                        }
                    }
                }
                @if let Some(quest) = abandonable_quest {
                    div class="map-active-quest-actions" {
                        p class="small-copy" { "Active quest: " strong { (quest.title) } }
                        form method="post" action=(format!("/quests/{}/abandon", quest.id)) {
                            button type="submit" class="btn btn-danger btn-small" { "Abandon active quest" }
                        }
                    }
                }
            }))
            }
            @if let Some(rest_menu) = rest_menu {
                (rest_menu)
            }
        }
    }
}

pub(crate) fn travel_preferences_form(party: &Party, action: &str) -> Markup {
    let walking_hours = f32::from(party.walking_minutes_per_day) / 60.0;
    let travel_at_night = party.travel_at_night;
    let walking_hours_title = if travel_at_night {
        "Walking is centered on midnight; shorter first and final days are forecast automatically."
    } else {
        "Walking is centered on solar noon; shorter first and final days are forecast automatically."
    };
    html! {
        form method="post" action=(action) class="travel-configuration-form" data-travel-configuration {
            div class="travel-setting-heading" title=(walking_hours_title) {
                label for="walking-hours" { "Walking hours per day" }
                span class="travel-walking-value" {
                    output for="walking-hours" data-walking-hours-output { (format!("{walking_hours}")) }
                    span aria-hidden="true" { " h" }
                }
            }
            div class="travel-fatigue-control" {
                input id="walking-hours" type="range" name="walking_hours" min="0" max="24" step="0.25" value=(walking_hours) data-walking-hours {}
            }
            div class="travel-period-control" {
                span { "Travel during" }
                label class="travel-period-toggle" title=(if travel_at_night { "Travel at night; camp time is centered on noon" } else { "Travel during the day; walking time is centered on noon" }) {
                    input type="checkbox" name="travel_at_night" value="true" checked[travel_at_night]
                        aria-label="Travel at night" data-travel-period-toggle;
                    span class="travel-period-track" aria-hidden="true" {
                        span class="travel-period-option travel-period-day" {}
                        span class="travel-period-option travel-period-night" {}
                        span class="travel-period-thumb" {}
                    }
                }
            }
        }
    }
}

pub(crate) fn map_destination_detail(
    selected: Option<&TravelDestination>,
    selected_settlement: Option<&Settlement>,
    selected_is_current: bool,
    can_travel: bool,
    provisioning_path: Option<&str>,
    provision_forecast: Option<&TravelProvisionForecast>,
    party: Option<&Party>,
    can_configure_travel: bool,
    standalone_planner: Option<Markup>,
    map_path: &str,
) -> Markup {
    let camp_fatigue_percent = party.map_or(50, |party| party.camp_fatigue_percent);
    let travel_disabled = party.is_some_and(|party| party.walking_minutes_per_day == 0);
    let inspecting_nonroute = selected.is_none() && selected_settlement.is_some();
    html! {
        aside class=(if party.is_some() && can_configure_travel && !inspecting_nonroute { "right-sidebar travel-configuration-sidebar" } else { "right-sidebar" }) {
            @if party.is_some() && can_configure_travel {
            (sidebar_section("Travel configuration", html! {
                div class=(if selected.is_some() { "travel-planner-vertical" } else { "travel-planner-vertical no-destination" }) {
                    (travel_planner_bar(selected, camp_fatigue_percent))
                }
                (travel_preferences_form(party.expect("party checked above"), &format!("{map_path}/travel-configuration")))
                @if let Some(provisioning_path) = provisioning_path {
                    div class="travel-provisioning-control" data-provisioning-control {
                        div class="travel-provisioning-input" {
                            input type="hidden" value="0" data-target-surplus;
                            span class="travel-provisioning-target" {
                                span id="target-surplus" class="travel-provisioning-value" data-target-surplus-display
                                    role="button" tabindex="0" aria-label="Target surplus in days"
                                    title="Click to edit target surplus" { "0" }
                                span class="travel-provisioning-unit" { "d surplus" }
                            }
                            span class="travel-provisioning-icons" {
                                span class="travel-provisioning-icon food" { (game_icon("Food", "meal")) }
                                span class="travel-provisioning-icon water" { (game_icon("Water", "water-drop")) }
                                @if let Some(forecast) = provision_forecast {
                                    span class="travel-provisioning-icon alcohol"
                                        title=(format!("Emergency alcohol adds {:.2} days of hydration", forecast.emergency_alcohol_days)) {
                                        (game_icon("Emergency alcohol hydration", "beer-stein"))
                                        span class="travel-provisioning-alcohol-days" { (format!("+{:.2}d", forecast.emergency_alcohol_days)) }
                                    }
                                }
                            }
                            @if let Some(forecast) = provision_forecast {
                                a class="btn btn-secondary" data-provision-buy
                                    data-market-path=(provisioning_path)
                                    data-initial-rations=(forecast.rations_to_buy)
                                    data-initial-waterskins=(forecast.waterskins_to_buy)
                                    href=(provisioning_path) { "Buy" }
                            } @else {
                                button type="button" class="btn btn-secondary" disabled title="Provision estimates are unavailable" { "Buy" }
                            }
                        }
                        @if selected.is_some() {
                            p class="text-muted small-copy" data-provisioning-status {
                                @if provision_forecast.is_none() { "Provision estimates are temporarily unavailable." }
                            }
                        }
                    }
                }
            }))
            }
            @if let Some(planner) = standalone_planner {
                (sidebar_section("Journey", html! {
                    div class="travel-planner-vertical" { (planner) }
                }))
            }
            @if let Some(destination) = selected {
                (sidebar_section("", html! {
                    @if can_travel {
                        form method="post" action=(&destination.travel_action) data-travel-submit {
                            button type="submit" class="btn btn-primary btn-block"
                                disabled[travel_disabled]
                                title=(if travel_disabled { "Increase walking hours above zero to begin the journey" } else { "Begin journey" }) {
                                "Begin journey"
                            }
                        }
                        p class="travel-action-status" data-travel-action-status role="alert" hidden {}
                    }
                    @if let Some(track_action) = &destination.track_action {
                        form method="post" action=(track_action) {
                            button type="submit" class="btn btn-secondary btn-block"
                                disabled[destination.tracked] {
                                @if destination.tracked { "Tracked" } @else { "Track site" }
                            }
                        }
                    }
                    p class="text-muted small-copy" {
                        @if let Some(summary) = &destination.summary { (summary) " · " }
                        (format_distance(destination.distance_m))
                        " · " (format_journey_time(destination.journey_minutes))
                        @if destination.route_fallback {
                            span class="travel-route-estimate-warning" { " · Legacy straight-line estimate" }
                        }
                    }
                }))
            } @else if let Some(destination) = selected_settlement {
                (sidebar_section("Destination", html! {
                    h3 { (&destination.name) }
                    p { (settlement_description(destination.population_level)) }
                    dl class="settlement-stats" {
                        div { dt { "Size" } dd { (format_population(destination)) } }
                    }
                    p class="no-direct-route" role="status" {
                        @if selected_is_current {
                            strong { "Current settlement." }
                            " Your party is already here."
                        } @else {
                            strong { "No direct route." }
                            " Travel is only available to settlements connected to the current location."
                        }
                    }
                }))
            }
        }
    }
}

pub(crate) fn travel_planner_bar(
    selected: Option<&TravelDestination>,
    camp_fatigue_percent: u8,
) -> Markup {
    let selected_name = selected
        .map(|destination| destination.name.as_str())
        .unwrap_or("");
    let selected_description = selected
        .and_then(quest_destination_tooltip)
        .unwrap_or_default();
    let selected_minutes = selected.map_or(0, |destination| destination.journey_minutes);
    let selected_camp_stops = selected.map_or_else(String::new, |destination| {
        format_camp_stops(&destination.camp_stop_minutes)
    });
    let selected_camp_forecasts = selected.map_or_else(String::new, format_camp_forecasts);
    let provision_forecast =
        selected.and_then(|destination| destination.provision_forecast.as_ref());
    travel_planner_bar_for(
        selected_name,
        &selected_description,
        selected.is_some_and(|destination| {
            destination.round_trip_destination && destination.return_terrain_route.is_none()
        }),
        selected_minutes,
        &selected_camp_stops,
        &selected_camp_forecasts,
        camp_fatigue_percent,
        None,
        None,
        provision_forecast,
        selected
            .map(|destination| destination.departure_minute)
            .unwrap_or(0),
        selected
            .map(|destination| destination.itinerary_total_elapsed_minutes)
            .unwrap_or(selected_minutes),
        &selected.map_or_else(String::new, |destination| {
            format_itinerary_segments(&destination.itinerary_segments)
        }),
        &selected.map_or_else(String::new, |destination| format_terrain_spans(destination)),
    )
}

fn quest_destination_tooltip(destination: &TravelDestination) -> Option<String> {
    destination.case_site_knowledge.map(|_| {
        destination.summary.as_ref().map_or_else(
            || destination.description.clone(),
            |summary| format!("{}\n{summary}", destination.description),
        )
    })
}

pub(crate) fn travel_planner_bar_for(
    destination_name: &str,
    destination_description: &str,
    selected_round_trip: bool,
    journey_minutes: u64,
    camp_stop_minutes: &str,
    camp_forecasts: &str,
    camp_fatigue_percent: u8,
    journey: Option<&PartyJourney>,
    journey_route: Option<&PartyJourneyRoute>,
    provision_forecast: Option<&TravelProvisionForecast>,
    preview_departure_minute: u64,
    preview_elapsed_minutes: u64,
    preview_segments: &str,
    terrain_spans: &str,
) -> Markup {
    let journey_origin_name = journey.map_or("", |item| item.origin.name());
    let journey_destination_name = journey.map_or("", |item| item.destination.name());
    let journey_turnaround_minutes = journey
        .filter(|item| item.destination.case_site_id().is_some())
        .map_or(0, |item| item.total_minutes);
    let journey_total_minutes = journey.map_or(0, |item| {
        if item.destination.case_site_id().is_some() {
            item.total_minutes.saturating_add(
                journey_route
                    .and_then(|route| route.return_route.as_ref())
                    .map_or(item.total_minutes, |route| route.minutes),
            )
        } else {
            item.total_minutes
        }
    });
    let journey_completed_minutes = journey.map_or(0, |item| item.completed_minutes);
    let journey_camp_stops = journey.map_or_else(String::new, |item| {
        format_camp_stops(&item.camp_stop_minutes)
    });
    let journey_forecast_stops = journey.map_or_else(String::new, |item| {
        let mut stops = item.forecast_camp_stop_minutes.clone();
        if item.destination.case_site_id().is_some() {
            stops.extend(
                item.camp_stop_minutes
                    .iter()
                    .chain(item.forecast_camp_stop_minutes.iter())
                    .rev()
                    .map(|minute| journey_total_minutes.saturating_sub(*minute)),
            );
        }
        format_camp_stops(&stops)
    });
    html! {
        section class="travel-planner" data-travel-planner
            data-camp-fatigue-percent=(camp_fatigue_percent)
            data-selected-name=(destination_name)
            data-selected-description=(destination_description)
            data-selected-round-trip=(selected_round_trip)
            data-selected-minutes=(journey_minutes)
            data-selected-camp-stops=(camp_stop_minutes)
            data-selected-camp-forecasts=(camp_forecasts)
            data-journey-origin-name=(journey_origin_name)
            data-journey-destination-name=(journey_destination_name)
            data-journey-total-minutes=(journey_total_minutes)
            data-journey-turnaround-minutes=(journey_turnaround_minutes)
            data-journey-completed-minutes=(journey_completed_minutes)
            data-departure-minute=(journey.map_or(preview_departure_minute, |item| item.departure_minute))
            data-total-elapsed-minutes=(journey.map_or(preview_elapsed_minutes, |item| item.total_elapsed_minutes))
            data-completed-elapsed-minutes=(journey.map_or(0, |item| item.completed_elapsed_minutes))
            data-itinerary-segments=(preview_segments)
            data-terrain-spans=(terrain_spans)
            data-journey-camp-stops=(journey_camp_stops)
            data-journey-forecast-stops=(journey_forecast_stops)
            data-provision-planning-minutes=[provision_forecast.map(|row| row.planning_minutes)]
            data-provision-living-members=[provision_forecast.map(|row| row.living_members)]
            data-provision-food-days=[provision_forecast.map(|row| row.food_days)]
            data-provision-water-days=[provision_forecast.map(|row| row.water_days)]
            data-provision-ordinary-water-days=[provision_forecast.map(|row| row.ordinary_water_days)]
            data-provision-emergency-alcohol-days=[provision_forecast.map(|row| row.emergency_alcohol_days)]
            data-provision-food-reserve=[provision_forecast.map(|row| row.food_reserve_kcal)]
            data-provision-water-reserve=[provision_forecast.map(|row| row.water_reserve_ml)]
            data-provision-rations=[provision_forecast.map(|row| row.ration_count)]
            data-provision-waterskins=[provision_forecast.map(|row| row.waterskin_count)]
            data-provision-ration-kcal=[provision_forecast.map(|row| row.ration_kcal)]
            data-provision-waterskin-ml=[provision_forecast.map(|row| row.waterskin_capacity_ml)]
            aria-live="polite" hidden {
            div class="travel-track" {
                div class="travel-planner-route" data-travel-planner-route {}
                div class="travel-resource-meters" data-travel-resource-meters {
                    div class="travel-resource-row food" aria-label="Food provisions" {
                        span class="travel-resource-icon" { (game_icon("Food", "meal")) }
                        svg class="travel-resource-track" viewBox="0 0 32 100" preserveAspectRatio="none" aria-hidden="true" {
                            path class="travel-resource-path base" d="M 16 0 V 100" pathLength="100" {}
                            path class="travel-resource-path target" data-resource-target pathLength="100" {}
                            path class="travel-resource-path actual" data-resource-fill pathLength="100" {}
                        }
                        span class="sr-only" data-surplus-summary="food" {}
                    }
                    div class="travel-resource-row water" aria-label="Water provisions" {
                        span class="travel-resource-icon" { (game_icon("Water", "water-drop")) }
                        svg class="travel-resource-track" viewBox="0 0 32 100" preserveAspectRatio="none" aria-hidden="true" {
                            path class="travel-resource-path base" d="M 16 0 V 100" pathLength="100" {}
                            path class="travel-resource-path target" data-resource-target pathLength="100" {}
                            path class="travel-resource-path actual" data-resource-fill pathLength="100" {}
                        }
                        span class="sr-only" data-surplus-summary="water" {}
                    }
                    div class="travel-resource-row fatigue" aria-label="Party fatigue" {
                        span class="travel-resource-icon" { (game_icon("Fatigue", "heart-minus")) }
                        div class="travel-fatigue-track" data-fatigue-track {}
                        span class="sr-only" data-fatigue-summary aria-live="polite" {}
                    }
                    div class="travel-resource-row terrain" aria-label="Terrain along route" {
                        span class="travel-resource-icon" { (game_icon("Terrain", "mountains")) }
                        div class="travel-terrain-track" data-terrain-track aria-describedby="terrain-course-description" {}
                        span class="sr-only" data-terrain-summary aria-live="polite" {}
                        ol id="terrain-course-description" class="sr-only" data-terrain-course-description {}
                    }
                    div class="travel-resource-row daylight" aria-label="Day and night" {
                        span class="travel-resource-icon" { (game_icon("Day and night", "sun")) }
                        div class="travel-daylight-track" data-daylight-track {}
                    }
                }
                svg class="travel-progress-track" viewBox="0 0 32 100" preserveAspectRatio="none" aria-hidden="true" {
                    path class="travel-progress-path" data-travel-progress pathLength="100" {}
                }
            }
        }
    }
}

fn format_camp_stops(stops: &[u64]) -> String {
    stops
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_terrain_spans(destination: &TravelDestination) -> String {
    destination
        .terrain_route
        .as_ref()
        .map_or_else(String::new, |route| {
            route
                .spans
                .iter()
                .map(|span| (span, 0_u64))
                .chain(
                    destination
                        .return_terrain_route
                        .iter()
                        .flat_map(|return_route| {
                            return_route.spans.iter().map(|span| (span, route.minutes))
                        }),
                )
                .filter_map(|(span, offset)| {
                    let kind = match span.surface {
                        adventuresim_terrain::Surface::Road => "road",
                        adventuresim_terrain::Surface::Open => "open",
                        adventuresim_terrain::Surface::SparseWoods => "sparse-woods",
                        adventuresim_terrain::Surface::DeepWoods => "deep-woods",
                        adventuresim_terrain::Surface::Wetland => "wetland",
                        adventuresim_terrain::Surface::Water => return None,
                    };
                    Some(format!(
                        "{kind},{},{},{},{},{},{},{},{}",
                        span.start_minute.saturating_add(offset),
                        span.duration_minutes,
                        span.check_millirank,
                        span.terrain.plains,
                        span.terrain.forest,
                        span.terrain.hills,
                        span.terrain.wetlands,
                        span.terrain.urban,
                    ))
                })
                .collect::<Vec<_>>()
                .join("|")
        })
}

fn format_camp_forecasts(destination: &TravelDestination) -> String {
    destination
        .camp_forecasts
        .iter()
        .map(|forecast| {
            format!(
                "{}:{}",
                forecast.fatigue_percent,
                format_camp_stops(&forecast.camp_stop_minutes)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn format_itinerary_segments(segments: &[ItinerarySegment]) -> String {
    segments
        .iter()
        .map(|segment| {
            format!(
                "{},{},{},{},{},{:.4},{:.4},{:.4},{}",
                if matches!(segment.kind, ItinerarySegmentKind::Walking) {
                    "w"
                } else {
                    "c"
                },
                segment.elapsed_start,
                segment.elapsed_minutes,
                segment.movement_start,
                segment.movement_minutes,
                segment.average_fatigue_start,
                segment.average_fatigue_end,
                segment.maximum_fatigue_end,
                segment.required_rest_minutes,
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn format_persisted_itinerary(journey: &PartyJourney, itinerary: &PartyJourneyItinerary) -> String {
    let mut camps: Vec<_> = itinerary
        .actual_camp_intervals
        .iter()
        .cloned()
        .map(|camp| (camp, true, false))
        .chain(
            itinerary
                .forecast_camp_intervals
                .iter()
                .cloned()
                .map(|camp| (camp, false, true)),
        )
        .collect();
    camps.sort_by_key(|(camp, _, _)| camp.elapsed_start_minute);
    let mut merged: Vec<(crate::spacetimedb::JourneyCampInterval, bool, bool)> = Vec::new();
    for (camp, actual, forecast) in camps {
        if let Some((last, was_actual, was_forecast)) = merged.last_mut()
            && last.movement_minute == camp.movement_minute
            && camp.elapsed_start_minute
                <= last
                    .elapsed_start_minute
                    .saturating_add(last.elapsed_minutes)
        {
            let end = last
                .elapsed_start_minute
                .saturating_add(last.elapsed_minutes)
                .max(
                    camp.elapsed_start_minute
                        .saturating_add(camp.elapsed_minutes),
                );
            last.elapsed_minutes = end.saturating_sub(last.elapsed_start_minute);
            last.average_fatigue_end = camp.average_fatigue_end;
            last.maximum_fatigue_end = last.maximum_fatigue_end.max(camp.maximum_fatigue_end);
            *was_actual |= actual;
            *was_forecast |= forecast;
        } else {
            merged.push((camp, actual, forecast));
        }
    }
    let total_movement = if journey.destination.case_site_id().is_some() {
        journey.total_minutes.saturating_mul(2)
    } else {
        journey.total_minutes
    };
    let mut output = Vec::new();
    let mut elapsed_cursor = 0;
    let mut movement_cursor = 0;
    let mut fatigue = merged
        .first()
        .map_or(0.0, |(camp, _, _)| camp.average_fatigue_start);
    for (camp, actual, forecast) in merged {
        if camp.elapsed_start_minute > elapsed_cursor {
            let movement = camp.movement_minute.saturating_sub(movement_cursor);
            output.push(format!(
                "w,{},{},{},{},{:.4},{:.4},{:.4},0",
                elapsed_cursor,
                camp.elapsed_start_minute - elapsed_cursor,
                movement_cursor,
                movement,
                fatigue,
                camp.average_fatigue_start,
                camp.average_fatigue_start
            ));
        }
        let kind = if actual && forecast {
            "m"
        } else if actual {
            "a"
        } else {
            "f"
        };
        output.push(format!(
            "{kind},{},{},{},0,{:.4},{:.4},{:.4},{}",
            camp.elapsed_start_minute,
            camp.elapsed_minutes,
            camp.movement_minute,
            camp.average_fatigue_start,
            camp.average_fatigue_end,
            camp.maximum_fatigue_end,
            camp.elapsed_minutes
        ));
        elapsed_cursor = camp
            .elapsed_start_minute
            .saturating_add(camp.elapsed_minutes);
        movement_cursor = camp.movement_minute;
        fatigue = camp.average_fatigue_end;
    }
    if elapsed_cursor < journey.total_elapsed_minutes {
        output.push(format!(
            "w,{},{},{},{},{:.4},{:.4},{:.4},0",
            elapsed_cursor,
            journey.total_elapsed_minutes - elapsed_cursor,
            movement_cursor,
            total_movement.saturating_sub(movement_cursor),
            fatigue,
            fatigue,
            fatigue
        ));
    }
    output.join("|")
}

fn format_legacy_persisted_itinerary(journey: &PartyJourney) -> String {
    let total_movement = if journey.destination.case_site_id().is_some() {
        journey.total_minutes.saturating_mul(2)
    } else {
        journey.total_minutes
    };
    format!(
        "w,0,{},{},{},0.0000,0.0000,0.0000,0",
        journey.total_elapsed_minutes, 0, total_movement
    )
}

pub(crate) struct CampTravelDestination {
    pub id: String,
    pub name: String,
    pub journey_minutes: u64,
    pub current: bool,
}

fn camp_fire_is_lit(
    journey: Option<&PartyJourney>,
    itinerary: Option<&PartyJourneyItinerary>,
) -> bool {
    !matches!(
        (journey, itinerary),
        (Some(journey), Some(itinerary))
            if itinerary
                .actual_camp_intervals
                .last()
                .is_some_and(|interval| interval.movement_minute == journey.completed_minutes)
    )
}

/// The transient strategic location between planned travel legs.
fn camp_forage_href(has_active_character: bool) -> Option<&'static str> {
    has_active_character.then_some("/camp?forage=true")
}

fn developer_road_encounter_label(id: &str) -> String {
    let words = id.trim_end_matches("_v1").replace('_', " ");
    let mut chars = words.chars();
    chars.next().map_or(words.clone(), |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

pub fn camp_page(
    party: &Party,
    journey: Option<&PartyJourney>,
    itinerary: Option<&PartyJourneyItinerary>,
    terrain_route: Option<&PartyJourneyRoute>,
    destination_name: &str,
    active_character: Option<&Character>,
    party_members: &[Character],
    camp_destinations: &[CampTravelDestination],
    provision_forecast: Option<&TravelProvisionForecast>,
    default_rest_minutes: u64,
    soap_preview: SoapRestPreview,
    planned_wake_minute: u16,
    continue_block_reason: Option<&str>,
    encounter: Option<&StrategicEncounter>,
    counterparties: &[Character],
    dispositions: &[BackendContextDisposition],
    trial: Option<(&str, &str, ChallengePresenterCatalogId)>,
    tactical_insight: Option<(&str, &str)>,
    road_trial: Option<&BackendRoadChallenge>,
    road_history: &[&BackendRoadChallenge],
    foraging_dialog: Option<Markup>,
    logged_in_as: Option<&str>,
) -> Markup {
    let camp_fire_lit = camp_fire_is_lit(journey, itinerary);
    let forage_href = camp_forage_href(active_character.is_some());
    let content = html! {
        aside class="left-sidebar map-rest-sidebar" {
            div class="map-rest-sidebar-content" {
            (sidebar_section("Camp", html! {
                p { "The party has made camp between travel legs." }
                p class="text-muted small-copy" { "Destination: " (destination_name) }
                p class="text-muted small-copy" { (format_journey_time(party.camp_remaining_minutes)) " remaining" }
                @if let Some(href) = forage_href {
                    a class="btn btn-secondary" href=(href) { "Forage nearby" }
                }
            }))
            @if !camp_destinations.is_empty() {
                (sidebar_section("Destinations", html! {
                    nav class="location-destination-list camp-destination-list" aria-label="Available camp destinations" {
                        @for destination in camp_destinations {
                            form action=(format!("/camp/destination/{}", destination.id)) method="post" {
                                button type="submit" class="list-item travel-destination-row camp-destination-row"
                                    disabled[destination.current] {
                                    strong { (&destination.name) }
                                    span class="text-muted small-copy" {
                                        @if destination.current { "Current" }
                                        @else { (format_journey_time(destination.journey_minutes)) }
                                    }
                                }
                            }
                        }
                    }
                }))
            }
            section class="sidebar-section" data-developer-only aria-label="Road encounter demo" {
                h3 class="sidebar-header" { "Road encounter demo" }
                label for="developer-road-encounter-catalog" { "Compiled encounter" }
                select id="developer-road-encounter-catalog" data-developer-road-encounter-catalog {
                    @for definition in adventuresim_core::road_encounter_catalog::definitions() {
                        option value=(&definition.id) { (developer_road_encounter_label(&definition.id)) }
                    }
                }
                button type="button" class="btn btn-small btn-block"
                    data-developer-road-encounter-demo {
                    "Load encounter"
                }
            }
            }
            @if encounter.is_none_or(|encounter| encounter.status != "awaiting_choice") {
                section class="rest-service-menu camp-rest-menu" aria-label="Camp rest" {
                    (party_rest_menu(
                        "/camp/rest",
                        "camp-rest",
                        "Rest at camp",
                        "Rest party",
                        default_rest_minutes,
                        Some(planned_wake_minute),
                        soap_preview,
                    ))
                }
            }
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, "/camp", None, false))
            (visual_stage("camp", "Camp", "A resting place beside the party's onward route"))
            @if let Some((finding, preparation)) = tactical_insight {
                section class="strategic-notice" data-tactical-insight aria-label="Tactical insight" {
                    h3 { "Tactical insight" }
                    p { (finding) }
                    p { strong { "Prepare accordingly: " } (preparation) }
                }
            }
            @if let Some((case_id, challenge_id, presenter_catalog_id)) = trial {
                @let opening = match presenter_catalog_id {
                    ChallengePresenterCatalogId::LadyBeneathThornV1 =>
                        fey_speech(
                            FeyPresenterCatalogId::LadyBeneathThornV1,
                            FeySpeechPart::Introduction,
                        )[0],
                };
                section class="settlement-chat challenge-chat-invitation" aria-label="Fey conversation" {
                    div class="settlement-chat-layout" {
                        div class="settlement-chat-conversation" {
                            div class="settlement-chat-messages" aria-live="polite" {
                                p class="supernatural-spoken-line" {
                                    strong { "The Lady Beneath the Thorn: " }
                                    (opening)
                                }
                                a class="btn btn-primary"
                                    href=(format!("/quests/{case_id}/challenges/{challenge_id}")) {
                                    "Enter the trial"
                                }
                            }
                        }
                    }
                }
            }
            @if let Some(road_trial) = road_trial {
                (generic_road_encounter(road_trial))
            }
            @for challenge in road_history {
                (generic_road_encounter(challenge))
            }
            @if trial.is_none() && road_trial.is_none() && road_history.is_empty() {
                (settlement_chat_area("Camp", active_character))
            }
        }
        aside class="right-sidebar camp-journey-sidebar" {
            @for disposition in dispositions.iter().filter(|row|matches!(row.disposition,crate::spacetimedb::DispositionKind::Surrendered|crate::spacetimedb::DispositionKind::Refused)) {
                section class="sidebar-section systemic-outcome" aria-label="Encounter outcome" {
                    p { "Character " (disposition.character_id) ": " (format!("{:?}",disposition.disposition)) }
                }
            }
            @if let Some(encounter) = encounter.filter(|encounter| encounter.status == "awaiting_choice") {
                (strategic_encounter_panel(encounter, counterparties, dispositions))
            }
            div class="sidebar-section camp-journey-section" {
                h3 class="sidebar-header" { "Journey" }
                @if let Some(route) = terrain_route {
                    (journey_weather_status(route))
                }
                div class="travel-planner-vertical" {
                    (travel_planner_bar_for(destination_name, "", false, party.camp_remaining_minutes, "", "", party.camp_fatigue_percent, journey, terrain_route, provision_forecast, journey.map_or(0, |item| item.departure_minute), journey.map_or(party.camp_remaining_minutes, |item| item.total_elapsed_minutes), &match (journey, itinerary) { (Some(journey), Some(itinerary)) => format_persisted_itinerary(journey, itinerary), (Some(journey), None) => format_legacy_persisted_itinerary(journey), _ => String::new() }, &format_persisted_terrain_spans(terrain_route)))
                }
                (camp_continue_control(continue_block_reason))
                p class="travel-action-status" data-travel-action-status role="alert" hidden {}
            }
            (sidebar_section("Travel preferences", travel_preferences_form(party, "/camp/travel-configuration")))
        }
        @if let Some(dialog) = foraging_dialog {
            (dialog)
        }
    };
    camp_location_layout_with_session(
        "Camp",
        "Camp",
        &party.id,
        camp_fire_lit,
        content,
        logged_in_as,
    )
}

fn camp_continue_control(block_reason: Option<&str>) -> Markup {
    html! {
        form action="/camp/continue" method="post" {
            button type="submit" class="btn btn-primary btn-small btn-block"
                disabled[block_reason.is_some()]
                title=(block_reason.unwrap_or("Continue travel")) {
                "Continue travel"
            }
        }
        @if let Some(reason) = block_reason {
            p class="travel-action-status" data-travel-action-status role="alert" { (reason) }
        }
    }
}

fn generic_road_encounter(challenge: &BackendRoadChallenge) -> Markup {
    let Ok(presentation) = serde_json::from_str::<
        adventuresim_core::road_encounter_catalog::EncounterPresentation,
    >(&challenge.presentation_json) else {
        return html! { p class="encounter-warning" { "This encounter's authored record is unavailable." } };
    };
    html! {
        section class="settlement-chat challenge-chat-invitation" aria-label="Roadside conversation" {
            @if challenge.active && challenge.open && !presentation.cast.is_empty() {
              nav class="settlement-npc-strip counterparty-strip" aria-label="Roadside characters" {
                @for character in &presentation.cast {
                  div class="npc-portrait counterparty-portrait" data-character-id=(character.character_id) {
                    span class="npc-portrait-image" aria-hidden="true" { "?" }
                    span class="npc-portrait-name" { (&character.name) }
                    @if character.can_talk {
                      form action="/camp/counterparty/contact" method="post" {
                        input type="hidden" name="target_id" value=(character.character_id);
                        input type="hidden" name="contact_ref" value=(&challenge.id);
                        input type="hidden" name="expected_revision" value=(character.contact_revision);
                        input type="hidden" name="action_id" value=(format!("road-contact:{}:{}:{}", challenge.id, character.contact_revision, character.character_id));
                        button type="submit" class="btn btn-secondary btn-small" { "Talk" }
                      }
                    }
                    @if character.can_bandage {
                      form action="/camp/counterparty/bandage" method="post" {
                        input type="hidden" name="patient_id" value=(character.character_id);
                        button type="submit" class="btn btn-secondary btn-small" { "Bandage" }
                      }
                    }
                  }
                }
              }
            }
            div class="settlement-chat-layout" { div class="settlement-chat-conversation" {
                div class="settlement-chat-messages" aria-live="polite" {
                    @for line in &presentation.opening {
                        p class=(if line.supernatural { "supernatural-spoken-line" } else { "" }) {
                            strong { (line.speaker_name.as_str()) ": " } (line.text.as_str())
                        }
                    }
                    @if challenge.open {
                        div class="dialogue-actions" {
                            @for choice in &presentation.choices {
                                form action="/camp/errantry-road-challenge" method="post" {
                                    input type="hidden" name="challenge_id" value=(&challenge.id);
                                    input type="hidden" name="expected_revision" value=(challenge.revision);
                                    input type="hidden" name="choice" value=(&choice.id);
                                    input type="hidden" name="action_id" value=(format!("road-choice:{}:{}:{}", challenge.id, challenge.revision, choice.id));
                                    button type="submit" class="btn btn-primary" disabled[!choice.available] { (choice.label.as_str()) }
                                }
                            }
                        }
                    } @else {
                        @for line in &presentation.response {
                            p class=(if line.supernatural { "supernatural-spoken-line" } else { "" }) {
                                strong { (line.speaker_name.as_str()) ": " } (line.text.as_str())
                            }
                        }
                        @if let Some(transcript) = challenge.result_transcript.as_deref() { p { (transcript) } }
                        @if let Some(addendum) = challenge.quest_reward_addendum.as_deref() { p class="text-muted" { (addendum) } }
                    }
                }
            } }
        }
    }
}

fn strategic_encounter_panel(
    encounter: &StrategicEncounter,
    counterparties: &[Character],
    dispositions: &[BackendContextDisposition],
) -> Markup {
    let threat = encounter.archetype.parse::<ThreatId>().ok();
    let threat_name = threat
        .map(|id| id.display_name(u32::from(encounter.enemy_count)))
        .unwrap_or_else(|| "Unknown threats".to_string());
    let awareness = match (encounter.party_aware, encounter.enemy_aware) {
        (true, false) => "Your party spotted them first",
        (false, true) => "The enemy surprised your party",
        (true, true) => "Both sides are aware",
        (false, false) => "Neither side is aware",
    };
    html! {
        section class="sidebar-section strategic-encounter" aria-label="Random encounter" {
            h3 class="sidebar-header" { "Encounter" }
            p class="encounter-summary" {
                strong { (encounter.enemy_count) " " (threat_name) }
                " on " (encounter.terrain.as_str())
            }
            @if let Some(threat) = threat {
                p class="text-muted small-copy" {
                    "Preparation: " (threat.profile().investigation.preparation_advice)
                }
            }
            p { (awareness) }
            p class="text-muted small-copy" { (encounter.selection_explanation.as_str()) }
            @if !counterparties.is_empty() {
                nav class="settlement-npc-strip counterparty-strip" aria-label="Counterparty" {
                    @for character in counterparties {
                        @let disposition=dispositions.iter().find(|row|row.character_id==character.id);
                        div class="npc-portrait counterparty-portrait" {
                            span class="npc-portrait-image" aria-hidden="true" { "?" }
                            span class="npc-portrait-name" { (&character.name) }
                            form action="/camp/counterparty/contact" method="post" {
                                input type="hidden" name="target_id" value=(character.id);
                                input type="hidden" name="contact_ref" value=(&encounter.encounter_id);
                                input type="hidden" name="expected_revision" value=(encounter.revision);
                                input type="hidden" name="action_id" value=(format!("contact:{}:{}:{}", encounter.encounter_id, encounter.revision, character.id));
                                button type="submit" class="btn btn-secondary btn-small" { "Talk" }
                            }
                            @if let Some(disposition)=disposition {
                                span class="text-muted small-copy" { (format!("{:?}",disposition.disposition)) }
                                @if matches!(disposition.disposition,crate::spacetimedb::DispositionKind::Hostile|crate::spacetimedb::DispositionKind::Refused) {
                                    form action="/camp/counterparty/surrender" method="post" {
                                        input type="hidden" name="target_id" value=(character.id);
                                        input type="hidden" name="contact_ref" value=(&disposition.contact_ref);
                                        input type="hidden" name="expected_revision" value=(disposition.revision);
                                        input type="hidden" name="action" value="offer";
                                        input type="hidden" name="source_id" value=(format!("surrender:offer:{}:{}:{}",encounter.encounter_id,character.id,disposition.revision));
                                        button type="submit" class="btn btn-secondary btn-small" { "Offer surrender" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            @if let Some(reason) = encounter.run_ineligibility.as_deref() {
                p class="encounter-warning" { "Cannot run: " (reason) }
            }
            @if !encounter.loss_preview.is_empty() {
                details class="encounter-surrender-preview" {
                    summary { "Exact surrender losses" }
                    ul {
                        @for loss in &encounter.loss_preview {
                            li {
                                (loss.quantity) " × " (loss.item_id.as_str())
                                " (" (loss.value_each) " value each, " (loss.owner_kind.as_str()) ")"
                            }
                        }
                    }
                }
            }
            div class="encounter-actions" {
                @for choice in &encounter.available_choices {
                    form action="/camp/encounter" method="post" {
                        input type="hidden" name="encounter_id" value=(&encounter.encounter_id);
                        input type="hidden" name="choice" value=(choice);
                        input type="hidden" name="expected_revision" value=(encounter.revision);
                        input type="hidden" name="action_id" value=(format!("encounter-choice:{}:{}:{}", encounter.encounter_id, encounter.revision, choice));
                        button type="submit" class="btn btn-primary btn-small btn-block" {
                            (match choice.as_str() {
                                "sneak" => "Sneak past",
                                "detour" => "Take a detour",
                                "attack" => "Attack",
                                "run" => "Run",
                                "surrender" => "Surrender",
                                _ => choice.as_str(),
                            })
                        }
                    }
                }
            }
        }
    }
}

fn journey_weather_status(route: &PartyJourneyRoute) -> Markup {
    let (weather, icon) = match route.precipitation {
        JourneyPrecipitation::Clear => ("Clear", "sun"),
        JourneyPrecipitation::Rain => ("Rain", "water-drop"),
        JourneyPrecipitation::Snow => ("Snow", "water-drop"),
    };
    let ground = if route.snow_cover_bps >= 6_000 {
        "deep snow"
    } else if route.snow_cover_bps >= 1_500 {
        "snow-covered"
    } else if route.ground_moisture_bps >= 7_000 {
        "waterlogged"
    } else if route.ground_moisture_bps >= 3_000 {
        "muddy"
    } else if route.ground_moisture_bps >= 800 {
        "damp"
    } else {
        "dry"
    };
    html! {
        p class="journey-weather-status text-muted small-copy"
            aria-label=(format!("Departure conditions: {weather}; ground condition: {ground}")) {
            span class="travel-resource-icon" { (decorative_game_icon(icon)) }
            strong { "Departure: " (weather) }
            " · " (ground)
        }
    }
}

fn format_persisted_terrain_spans(route: Option<&PartyJourneyRoute>) -> String {
    route.map_or_else(String::new, |route| {
        route
            .spans
            .iter()
            .map(|span| (span, 0_u64))
            .chain(route.return_route.iter().flat_map(|return_route| {
                return_route.spans.iter().map(|span| (span, route.minutes))
            }))
            .map(|(span, offset)| {
                let kind = match span.kind {
                    JourneyTerrainKind::Road => "road",
                    JourneyTerrainKind::Open => "open",
                    JourneyTerrainKind::SparseWoods => "sparse-woods",
                    JourneyTerrainKind::DeepWoods => "deep-woods",
                    JourneyTerrainKind::Wetland => "wetland",
                };
                format!(
                    "{kind},{},{},{},{},{},{},{},{}",
                    span.start_minute.saturating_add(offset),
                    span.duration_minutes,
                    span.check_millirank,
                    span.terrain.plains,
                    span.terrain.forest,
                    span.terrain.hills,
                    span.terrain.wetlands,
                    span.terrain.urban,
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camp_foraging_affordance_is_discoverable_and_returns_to_camp() {
        assert_eq!(camp_forage_href(true), Some("/camp?forage=true"));
        assert_eq!(camp_forage_href(false), None);
    }

    #[test]
    fn weather_status_has_visible_and_accessible_ground_output() {
        let route = PartyJourneyRoute {
            party_id: "party".into(),
            package_digest: "a".repeat(64),
            weather_rules_version: 1,
            weather_interval_start: 0,
            precipitation: JourneyPrecipitation::Rain,
            intensity_bps: 8_000,
            ground_moisture_bps: 8_000,
            snow_cover_bps: 0,
            distance_m: 1,
            minutes: 1,
            points: vec![],
            spans: vec![],
            return_route: None,
        };
        let rendered = journey_weather_status(&route).into_string();
        assert!(rendered.contains("Departure conditions: Rain; ground condition: waterlogged"));
        assert!(rendered.contains("<strong>Departure: Rain</strong>"));
        assert!(rendered.contains("water-drop.svg"));
    }
    use crate::spacetimedb::*;
    use crate::templates::settlement::test_support::*;

    #[test]
    fn encounter_panel_renders_only_authoritative_choices_and_exact_losses() {
        let encounter = StrategicEncounter {
            party_id: "party".into(),
            encounter_id: "party:3".into(),
            archetype: "bandits".into(),
            enemy_count: 4,
            roll_index: 3,
            journey_movement_minute: 540,
            journey_elapsed_minute: 700,
            absolute_minute: 1_700,
            longitude_e7: 1,
            latitude_e7: 2,
            terrain: "road".into(),
            party_aware: false,
            enemy_aware: true,
            available_choices: vec!["attack".into(), "surrender".into()],
            status: "awaiting_choice".into(),
            revision: 4,
            selected_choice: None,
            selection_explanation: "deterministic awareness".into(),
            party_speed_m_per_minute: 60,
            enemy_speed_m_per_minute: 80,
            run_ineligibility: Some("too slow".into()),
            penalty_minutes: 0,
            loss_preview: vec![StrategicEncounterLoss {
                owner_kind: "member".into(),
                owner_id: 7,
                inventory_id: 8,
                item_id: "gold_coin".into(),
                quantity: 12,
                value_each: 1,
            }],
            outcome: None,
        };
        let rendered = strategic_encounter_panel(&encounter, &[], &[]).into_string();
        assert!(rendered.contains("The enemy surprised your party"));
        assert!(rendered.contains("Cannot run: too slow"));
        assert!(rendered.contains("12 × gold_coin"));
        assert!(rendered.contains("value=\"attack\""));
        assert!(rendered.contains("value=\"surrender\""));
        assert!(rendered.contains("name=\"expected_revision\" value=\"4\""));
        assert!(rendered.contains("encounter-choice:party:3:4:surrender"));
        assert!(!rendered.contains("value=\"run\""));
        assert!(!rendered.contains("value=\"sneak\""));
        let source = include_str!("travel.rs");
        assert!(source.contains("/camp/counterparty/surrender"));
        assert!(source.contains("disposition.revision"));
    }

    #[test]
    fn reported_exact_destination_is_neutral_and_keeps_round_trip_planning() {
        let destination = quest_destination();

        let markup = map_destination_list(&[destination], None, "/locations/settlement/test/map")
            .into_string();

        assert!(markup.contains("data-travel-round-trip=\"true\""));
        assert!(markup.contains("Reported exact location"));
        assert!(!markup.contains("destination-quest-badge"));
        assert!(!markup.contains("aria-label=\"Active quest destination\""));
        assert!(markup.contains("title=\"A camp beside the road.\nReported exact location\""));
        assert!(!markup.contains("destination-turn-in-badge"));
    }

    #[test]
    fn current_settlement_has_no_conventional_quest_marker() {
        let markup = map_destination_list_with_context(
            &[],
            None,
            "/locations/settlement/market/map",
            Some(MapCurrentLocation { name: "Market" }),
            None,
            None,
        )
        .into_string();

        assert!(markup.contains("current-location-row"));
        assert!(markup.contains("aria-current=\"location\""));
        assert!(!markup.contains("destination-open-quest-badge"));
        assert!(!markup.contains("destination-quest-badge"));
        assert!(!markup.contains("href="));
    }

    #[test]
    fn map_exposes_abandon_action_for_an_eligible_active_quest() {
        let markup = map_destination_list_with_context(
            &[],
            None,
            "/locations/settlement/issuer/map",
            Some(MapCurrentLocation { name: "Issuer" }),
            Some(MapAbandonableQuest {
                id: "active",
                title: "Drive off the bandits",
            }),
            None,
        )
        .into_string();

        assert!(markup.contains("Active quest: "));
        assert!(markup.contains("Drive off the bandits"));
        assert!(markup.contains("action=\"/quests/active/abandon\""));
        assert!(markup.contains("Abandon active quest"));
    }

    #[test]
    fn map_rest_menu_is_pinned_below_the_destination_list() {
        let markup = map_destination_list_with_rest(
            &[],
            None,
            "/locations/case-site/active/map",
            html! { section class="rest-service-menu" { "Rest party" } },
        )
        .into_string();

        assert!(markup.contains("left-sidebar map-rest-sidebar"));
        assert!(markup.contains("map-rest-sidebar-content"));
        assert!(markup.contains("rest-service-menu"));
        assert!(markup.contains("Rest party"));
    }

    #[test]
    fn quest_location_travel_has_one_plain_action_without_settlement_buying() {
        let destination = quest_destination();
        let markup = map_destination_detail(
            Some(&destination),
            None,
            false,
            true,
            None,
            None,
            None,
            false,
            None,
            "/map",
        )
        .into_string();

        assert!(markup.contains("Begin journey"));
        assert!(markup.contains("action=\"/case-sites/quest-location/track\""));
        assert!(markup.contains("Track site"));
        assert!(!markup.contains("<p>A camp beside the road.</p>"));
        assert!(markup.contains("Reported exact location"));
        assert!(!markup.contains("name=\"provisioning\""));
        assert!(!markup.contains("data-provision-buy"));
    }

    #[test]
    fn nonconnected_map_selection_has_detail_but_no_travel_form() {
        let mut destination = settlement();
        destination.id = "viabundus-99".into();
        destination.name = "Distant town".into();
        let markup = map_destination_detail(
            None,
            Some(&destination),
            false,
            true,
            Some("/settlements/viabundus-1/merchants"),
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();

        assert!(markup.contains("Distant town"));
        assert!(markup.contains("No direct route."));
        assert!(!markup.contains("Begin journey"));
        assert!(!markup.contains("data-travel-submit"));
    }

    #[test]
    fn unselected_map_omits_the_destination_frame() {
        let markup = map_destination_detail(
            None, None, false, true, None, None, None, false, None, "/map",
        )
        .into_string();
        assert!(!markup.contains("sidebar-header\">Destination"));
    }

    #[test]
    fn connected_settlement_selection_keeps_existing_travel_action() {
        let mut destination = quest_destination();
        destination.id = "viabundus-2".into();
        destination.name = "Connected town".into();
        destination.round_trip_destination = false;
        destination.travel_action = "/settlements/viabundus-2/travel".into();
        let markup = map_destination_detail(
            Some(&destination),
            None,
            false,
            true,
            Some("/settlements/viabundus-1/merchants"),
            None,
            None,
            false,
            None,
            "/locations/settlement/viabundus-1/map",
        )
        .into_string();

        assert!(markup.contains("action=\"/settlements/viabundus-2/travel\""));
        assert!(markup.contains("data-travel-submit"));
        assert!(markup.contains("Begin journey"));
        assert!(!markup.contains("No direct route"));
    }

    #[test]
    fn road_encounter_demo_selector_is_generic_and_camp_only() {
        let source = include_str!("travel.rs");
        let camp = source
            .split("pub fn camp_page")
            .nth(1)
            .and_then(|tail| tail.split("fn camp_continue_control").next())
            .unwrap();
        assert!(camp.contains("data-developer-road-encounter-catalog"));
        assert!(camp.contains("road_encounter_catalog::definitions()"));
        assert!(camp.contains("data-developer-road-encounter-demo"));

        let shared_layout = include_str!("../layout.rs");
        assert!(!shared_layout.contains("data-developer-road-encounter-demo"));
        assert!(!shared_layout.contains("wounded_knight_linden_v1"));
        let script = include_str!("../../../static/developer-quest-editor.js");
        assert!(script.contains("data-developer-road-encounter-catalog"));
        assert!(!script.contains("wounded_knight_linden_v1"));
        assert!(!script.contains("button.dataset.catalogId"));
    }

    #[test]
    fn persisted_quest_camp_keeps_turnaround_movement_after_elapsed_rest() {
        let mut journey = PartyJourney {
            party_id: "party".into(),
            gateway_bucket: 0,
            origin: crate::spacetimedb::JourneyEndpoint::Settlement(
                crate::spacetimedb::JourneySettlementEndpoint {
                    id: "home".into(),
                    name: "Home".into(),
                },
            ),
            destination: crate::spacetimedb::JourneyEndpoint::CaseSite(
                crate::spacetimedb::JourneyCaseSiteEndpoint {
                    id: crate::spacetimedb::CaseSiteId {
                        value: "quest".into(),
                    },
                    name: "Quest".into(),
                },
            ),
            total_minutes: 720,
            completed_minutes: 480,
            camp_stop_minutes: vec![480],
            forecast_camp_stop_minutes: vec![480],
            fatigue_percent: 50,
            plan_version: 1,
            departure_minute: 10_000,
            total_elapsed_minutes: 2_040,
            completed_elapsed_minutes: 780,
            walking_minutes_per_day: 480,
            travel_at_night: false,
            camp_duration_mode: crate::spacetimedb::CampDurationMode::Auto,
            fixed_camp_minutes: 0,
        };
        let camp = |start, duration, from, to| crate::spacetimedb::JourneyCampInterval {
            movement_minute: 480,
            elapsed_start_minute: start,
            elapsed_minutes: duration,
            average_fatigue_start: from,
            average_fatigue_end: to,
            maximum_fatigue_end: to,
        };
        let itinerary = PartyJourneyItinerary {
            party_id: "party".into(),
            actual_camp_intervals: vec![camp(480, 300, 0.5, 0.2)],
            forecast_camp_intervals: vec![camp(780, 300, 0.2, 0.0)],
        };
        assert!(
            !camp_fire_is_lit(Some(&journey), Some(&itinerary)),
            "resting at the current movement checkpoint leaves smoke-only embers"
        );
        journey.completed_minutes = 600;
        assert!(
            camp_fire_is_lit(Some(&journey), Some(&itinerary)),
            "reaching a later camp relights the fire"
        );
        journey.completed_minutes = 480;
        let encoded = format_persisted_itinerary(&journey, &itinerary);
        assert!(encoded.contains("w,0,480,0,480"));
        assert!(encoded.contains("m,480,600,480,0"));
        assert!(encoded.contains("w,1080,960,480,960"));
        assert_eq!(
            encoded
                .split('|')
                .filter(|segment| segment.starts_with("m,"))
                .count(),
            1,
            "one physical camp marker"
        );
    }
}
